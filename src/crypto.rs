//! 微信安全模式 AES-CBC 加解密与签名

use aes::Aes256;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use cbc::{Decryptor, Encryptor};
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use sha1::{Digest, Sha1};

type Aes256CbcDec = Decryptor<Aes256>;
type Aes256CbcEnc = Encryptor<Aes256>;

/// 微信 EncodingAESKey 的 base64 填充位可能非零，需要完全忽略填充验证
fn decode_base64_ignore_padding(s: &str) -> Result<Vec<u8>, String> {
    let s = s.trim_end_matches('=');
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;

    for &byte in s.as_bytes() {
        let val = match alphabet.iter().position(|&c| c == byte) {
            Some(pos) => pos as u32,
            None => return Err(format!("invalid base64 character: {}", byte as char)),
        };
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            result.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    Ok(result)
}

/// 从 EncodingAESKey 派生 AES 密钥和 IV
fn derive_key_iv(encoding_aes_key: &str) -> Result<([u8; 32], [u8; 16]), String> {
    let key_bytes = decode_base64_ignore_padding(&format!("{}=", encoding_aes_key))?;
    if key_bytes.len() < 32 {
        return Err(format!("key length {} < 32", key_bytes.len()));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&key_bytes[..32]);
    let mut iv = [0u8; 16];
    iv.copy_from_slice(&key_bytes[..16]);
    Ok((key, iv))
}

/// AES-CBC 解密微信安全模式消息
pub fn wx_decrypt(ciphertext_b64: &str, encoding_aes_key: &str) -> Result<String, String> {
    let (key, iv) = derive_key_iv(encoding_aes_key)?;
    let ciphertext = decode_base64_ignore_padding(ciphertext_b64)
        .map_err(|e| format!("base64 decode ciphertext: {e}"))?;

    let mut buf = ciphertext.to_vec();
    let pt = Aes256CbcDec::new(&key.into(), &iv.into())
        .decrypt_padded_mut::<cbc::cipher::block_padding::NoPadding>(&mut buf)
        .map_err(|e| format!("decrypt error: {e}"))?;

    if pt.is_empty() {
        return Err("empty plaintext".into());
    }
    let pad_byte = *pt.last().unwrap();
    if pad_byte == 0 || pad_byte > 32 {
        return Err(format!("invalid padding byte: {pad_byte}"));
    }
    let pt = &pt[..pt.len() - pad_byte as usize];

    if pt.len() < 20 {
        return Err("plaintext too short".into());
    }
    let msg_len = u32::from_be_bytes([pt[16], pt[17], pt[18], pt[19]]) as usize;
    if pt.len() < 20 + msg_len {
        return Err("plaintext too short for msg_len".into());
    }
    let msg = std::str::from_utf8(&pt[20..20 + msg_len])
        .map_err(|e| format!("utf8 error: {e}"))?;
    Ok(msg.to_string())
}

/// AES-CBC 加密回复消息（安全模式）
pub fn wx_encrypt(plaintext: &str, encoding_aes_key: &str, appid: &str) -> Result<String, String> {
    let (key, iv) = derive_key_iv(encoding_aes_key)?;
    let msg_bytes = plaintext.as_bytes();
    let appid_bytes = appid.as_bytes();

    let mut buf = Vec::with_capacity(16 + 4 + msg_bytes.len() + appid_bytes.len());
    let rand_bytes: [u8; 16] = rand::random();
    buf.extend_from_slice(&rand_bytes);
    buf.extend_from_slice(&(msg_bytes.len() as u32).to_be_bytes());
    buf.extend_from_slice(msg_bytes);
    buf.extend_from_slice(appid_bytes);

    let pad_len = 32 - (buf.len() % 32);
    buf.extend(std::iter::repeat(pad_len as u8).take(pad_len));

    let msg_len = buf.len();
    let ct = Aes256CbcEnc::new(&key.into(), &iv.into())
        .encrypt_padded_mut::<cbc::cipher::block_padding::NoPadding>(&mut buf, msg_len)
        .map_err(|e| format!("encrypt error: {e}"))?;

    Ok(B64.encode(ct))
}

/// 生成安全模式签名: SHA1(sort([token, timestamp, nonce, encrypt_msg]))
pub fn make_safe_signature(token: &str, timestamp: &str, nonce: &str, encrypt_msg: &str) -> String {
    let mut parts = [token, timestamp, nonce, encrypt_msg];
    parts.sort_unstable();
    hex::encode(Sha1::digest(parts.concat().as_bytes()))
}

/// 明文模式签名验证: SHA1(sort([token, timestamp, nonce]))
pub fn check_signature(token: &str, timestamp: &str, nonce: &str, sig: &str) -> bool {
    let mut parts = [token, timestamp, nonce];
    parts.sort_unstable();
    hex::encode(Sha1::digest(parts.concat().as_bytes())) == sig
}
