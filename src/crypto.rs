//! 微信安全模式 AES-CBC 加解密与签名

use aes::Aes256;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use cbc::{Decryptor, Encryptor};
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
pub fn wx_decrypt(
    ciphertext_b64: &str,
    encoding_aes_key: &str,
    expected_appid: &str,
) -> Result<String, String> {
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
    if pad_byte == 0 || pad_byte > 32 || pad_byte as usize > pt.len() {
        return Err(format!("invalid padding byte: {pad_byte}"));
    }
    let padding_start = pt.len() - pad_byte as usize;
    if !pt[padding_start..].iter().all(|byte| *byte == pad_byte) {
        return Err("invalid padding bytes".into());
    }
    let pt = &pt[..padding_start];

    if pt.len() < 20 {
        return Err("plaintext too short".into());
    }
    let msg_len = u32::from_be_bytes([pt[16], pt[17], pt[18], pt[19]]) as usize;
    let message_end = 20usize
        .checked_add(msg_len)
        .ok_or_else(|| "message length overflow".to_string())?;
    if pt.len() < message_end {
        return Err("plaintext too short for msg_len".into());
    }
    if pt[message_end..] != *expected_appid.as_bytes() {
        return Err("appid mismatch".into());
    }
    let msg = std::str::from_utf8(&pt[20..message_end]).map_err(|e| format!("utf8 error: {e}"))?;
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
    buf.resize(buf.len() + pad_len, pad_len as u8);

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

pub fn constant_time_eq(expected: &[u8], presented: &[u8]) -> bool {
    let max_len = expected.len().max(presented.len());
    let mut diff = expected.len() ^ presented.len();
    for index in 0..max_len {
        let left = expected.get(index).copied().unwrap_or_default();
        let right = presented.get(index).copied().unwrap_or_default();
        diff |= usize::from(left ^ right);
    }
    diff == 0
}

/// 明文模式签名验证: SHA1(sort([token, timestamp, nonce]))
pub fn check_signature(token: &str, timestamp: &str, nonce: &str, sig: &str) -> bool {
    let mut parts = [token, timestamp, nonce];
    parts.sort_unstable();
    let expected = hex::encode(Sha1::digest(parts.concat().as_bytes()));
    constant_time_eq(expected.as_bytes(), sig.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_encoding_aes_key() -> String {
        B64.encode([7_u8; 32]).trim_end_matches('=').to_string()
    }

    fn encrypt_raw(plaintext: &[u8], encoding_aes_key: &str) -> String {
        let (key, iv) = derive_key_iv(encoding_aes_key).expect("valid test key");
        let mut buf = plaintext.to_vec();
        let len = buf.len();
        let ciphertext = Aes256CbcEnc::new(&key.into(), &iv.into())
            .encrypt_padded_mut::<cbc::cipher::block_padding::NoPadding>(&mut buf, len)
            .expect("block-aligned plaintext");
        B64.encode(ciphertext)
    }

    fn raw_wechat_plaintext(message: &str, appid: &str) -> Vec<u8> {
        let mut plaintext = vec![3_u8; 16];
        plaintext.extend_from_slice(&(message.len() as u32).to_be_bytes());
        plaintext.extend_from_slice(message.as_bytes());
        plaintext.extend_from_slice(appid.as_bytes());
        let pad_len = 32 - plaintext.len() % 32;
        plaintext.resize(plaintext.len() + pad_len, pad_len as u8);
        plaintext
    }

    #[test]
    fn decrypt_rejects_padding_longer_than_plaintext_without_panicking() {
        let key = test_encoding_aes_key();
        let mut malformed = [0_u8; 16];
        malformed[15] = 17;
        let ciphertext = encrypt_raw(&malformed, &key);

        let result = std::panic::catch_unwind(|| wx_decrypt(&ciphertext, &key, "wx-app"));

        assert!(result.is_ok(), "malformed padding must not panic");
        assert!(result.expect("no panic").is_err());
    }

    #[test]
    fn decrypt_rejects_non_uniform_padding() {
        let key = test_encoding_aes_key();
        let mut malformed = raw_wechat_plaintext("hi", "wx-app");
        let pad_len = *malformed.last().expect("padding") as usize;
        let padding_start = malformed.len() - pad_len;
        malformed[padding_start] ^= 1;
        let ciphertext = encrypt_raw(&malformed, &key);

        assert!(wx_decrypt(&ciphertext, &key, "wx-app").is_err());
    }

    #[test]
    fn decrypt_rejects_mismatched_appid() {
        let key = test_encoding_aes_key();
        let ciphertext = wx_encrypt("hello", &key, "expected-app").expect("encrypt");

        assert!(wx_decrypt(&ciphertext, &key, "different-app").is_err());
    }
}
