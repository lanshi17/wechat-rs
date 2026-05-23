mod admin;
mod storage;

use aes::Aes256;
use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};

// 微信 EncodingAESKey 的 base64 填充位可能非零，需要完全忽略填充验证
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
use cbc::{Decryptor, Encryptor};
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use chrono::{DateTime, Utc};
use dotenvy::dotenv;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use sqlx::postgres::PgPoolOptions;
use std::{env, sync::Arc, time::Instant};
use tokio::sync::RwLock;
use tracing::info;

use storage::Storage;

type Aes256CbcDec = Decryptor<Aes256>;
type Aes256CbcEnc = Encryptor<Aes256>;

// ── 应用状态 ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub db:            Arc<dyn Storage>,
    pub config:        Arc<RwLock<AppConfig>>,
    pub admin_secret:  String,
    pub wechat_server_token: String,
    pub started_at:    Instant,
}

/// 可在管理界面动态修改的配置（持久化到 app_config 表）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub wechat_token:    String,
    pub wechat_appid:    String,
    pub wechat_appsecret: String,
    pub wechat_encoding_aes_key: String,
    pub admin_password_hash: String,
    pub welcome_message: String,
    pub site_name:       String,
    pub domain:          String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            wechat_token:        env::var("WECHAT_TOKEN").unwrap_or_default(),
            wechat_appid:        env::var("WECHAT_APPID").unwrap_or_default(),
            wechat_appsecret:    env::var("WECHAT_APPSECRET").unwrap_or_default(),
            wechat_encoding_aes_key: env::var("WECHAT_ENCODING_AES_KEY").unwrap_or_default(),
            admin_password_hash: env::var("ADMIN_PASSWORD_HASH").unwrap_or_default(),
            welcome_message:     "感谢关注！".into(),
            site_name:           env::var("SITE_NAME").unwrap_or_else(|_| "微信服务管理后台".into()),
            domain:              env::var("DOMAIN").unwrap_or_else(|_| "localhost".into()),
        }
    }
}

// ── 微信安全模式加解密 ─────────────────────────────────────────────────────────

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
fn wx_decrypt(ciphertext_b64: &str, encoding_aes_key: &str) -> Result<String, String> {
    let (key, iv) = derive_key_iv(encoding_aes_key)?;
    let ciphertext = decode_base64_ignore_padding(ciphertext_b64)
        .map_err(|e| format!("base64 decode ciphertext: {e}"))?;

    let mut buf = ciphertext.to_vec();
    let pt = Aes256CbcDec::new(&key.into(), &iv.into())
        .decrypt_padded_mut::<cbc::cipher::block_padding::NoPadding>(&mut buf)
        .map_err(|e| format!("decrypt error: {e}"))?;

    // 去除 PKCS#7 填充（块大小 32）
    if pt.is_empty() {
        return Err("empty plaintext".into());
    }
    let pad_byte = *pt.last().unwrap();
    if pad_byte == 0 || pad_byte > 32 {
        return Err(format!("invalid padding byte: {pad_byte}"));
    }
    let pt = &pt[..pt.len() - pad_byte as usize];

    // 格式: 16 bytes random + 4 bytes msg_len (big-endian) + msg + appid
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
fn wx_encrypt(plaintext: &str, encoding_aes_key: &str, appid: &str) -> Result<String, String> {
    let (key, iv) = derive_key_iv(encoding_aes_key)?;
    let msg_bytes = plaintext.as_bytes();
    let appid_bytes = appid.as_bytes();

    // 16 bytes random + 4 bytes msg_len + msg + appid
    let mut buf = Vec::with_capacity(16 + 4 + msg_bytes.len() + appid_bytes.len());
    // random 16 bytes
    let rand_bytes: [u8; 16] = rand::random();
    buf.extend_from_slice(&rand_bytes);
    buf.extend_from_slice(&(msg_bytes.len() as u32).to_be_bytes());
    buf.extend_from_slice(msg_bytes);
    buf.extend_from_slice(appid_bytes);

    // PKCS#7 填充到块大小 32
    let pad_len = 32 - (buf.len() % 32);
    buf.extend(std::iter::repeat(pad_len as u8).take(pad_len));

    let msg_len = buf.len();
    let ct = Aes256CbcEnc::new(&key.into(), &iv.into())
        .encrypt_padded_mut::<cbc::cipher::block_padding::NoPadding>(&mut buf, msg_len)
        .map_err(|e| format!("encrypt error: {e}"))?;

    Ok(B64.encode(ct))
}

/// 生成安全模式签名: SHA1(sort([token, timestamp, nonce, encrypt_msg]))
fn make_safe_signature(token: &str, timestamp: &str, nonce: &str, encrypt_msg: &str) -> String {
    let mut parts = [token, timestamp, nonce, encrypt_msg];
    parts.sort_unstable();
    hex::encode(Sha1::digest(parts.concat().as_bytes()))
}

// ── 用户模型 ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct UserInfo {
    pub openid:     String,
    pub nickname:   String,
    pub headimgurl: String,
    pub subscribe:  bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

// ── 微信 XML 消息 ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename = "xml")]
#[allow(dead_code)]
struct WxMessage {
    #[serde(rename = "ToUserName")]   to_user_name:   String,
    #[serde(rename = "FromUserName")] from_user_name: String,
    #[serde(rename = "MsgType")]      msg_type:       String,
    #[serde(rename = "Event")]        event:           Option<String>,
    #[serde(rename = "EventKey")]     event_key:       Option<String>,
    #[serde(rename = "Content")]      content:         Option<String>,
}

/// 安全模式信封
#[derive(Debug, Deserialize)]
#[serde(rename = "xml")]
struct WxEnvelope {
    #[serde(rename = "Encrypt")]      encrypt:      String,
    #[serde(rename = "MsgSignature")] msg_signature: Option<String>,
    #[serde(rename = "TimeStamp")]    timestamp:    Option<String>,
    #[serde(rename = "Nonce")]        nonce:        Option<String>,
}

// ── 查询参数 ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct VerifyParams {
    signature:   String,
    timestamp:   String,
    nonce:       String,
    echostr:     String,
    #[serde(default)] msg_signature: Option<String>,
    #[serde(default)] encrypt:       Option<String>,
}

#[derive(Deserialize)]
pub struct PageParams {
    #[serde(default = "default_page")] pub page: i64,
    #[serde(default = "default_size")] pub size: i64,
}
fn default_page() -> i64 { 1 }
fn default_size() -> i64 { 20 }

// ── 数据库操作 (通过 Storage trait) ──────────────────────────────────────────

pub async fn save_config(db: &dyn Storage, cfg: &AppConfig) -> Result<(), storage::StorageError> {
    let json = serde_json::to_string(cfg).unwrap();
    db.save_config(&json).await
}

// ── 路由处理器 ────────────────────────────────────────────────────────────────

async fn verify(
    State(state): State<Arc<AppState>>,
    Query(p): Query<VerifyParams>,
) -> impl IntoResponse {
    let cfg = state.config.read().await;
    let token = &cfg.wechat_token;
    let aes_key = &cfg.wechat_encoding_aes_key;

    // 安全模式: 有 encrypt 参数时，先验签再解密 echostr
    if let (Some(encrypt), Some(msg_sig)) = (&p.encrypt, &p.msg_signature) {
        let expected = make_safe_signature(token, &p.timestamp, &p.nonce, encrypt);
        if expected != *msg_sig {
            tracing::warn!("safe mode verify: signature mismatch");
            return (StatusCode::FORBIDDEN, "signature mismatch").into_response();
        }
        match wx_decrypt(encrypt, aes_key) {
            Ok(decrypted) => return decrypted.into_response(),
            Err(e) => {
                tracing::error!("safe mode verify decrypt error: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "decrypt error").into_response();
            }
        }
    }

    // 明文模式
    if check_signature(token, &p.timestamp, &p.nonce, &p.signature) {
        p.echostr.into_response()
    } else {
        (StatusCode::FORBIDDEN, "forbidden").into_response()
    }
}

async fn webhook(
    State(state): State<Arc<AppState>>,
    body: String,
) -> impl IntoResponse {
    let cfg = state.config.read().await;
    let aes_key = cfg.wechat_encoding_aes_key.clone();
    let appid = cfg.wechat_appid.clone();
    let token = cfg.wechat_token.clone();
    drop(cfg);

    // 尝试解析安全模式信封
    let xml_to_parse = if aes_key.len() == 43 {
        match quick_xml::de::from_str::<WxEnvelope>(&body) {
            Ok(env) => {
                // 验证签名
                let ts = env.timestamp.as_deref().unwrap_or("");
                let nc = env.nonce.as_deref().unwrap_or("");
                if let Some(ref sig) = env.msg_signature {
                    let expected = make_safe_signature(&token, ts, nc, &env.encrypt);
                    if expected != *sig {
                        tracing::warn!("webhook: signature mismatch");
                        return (StatusCode::FORBIDDEN, "signature mismatch").into_response();
                    }
                }
                // 解密
                match wx_decrypt(&env.encrypt, &aes_key) {
                    Ok(decrypted) => {
                        info!("decrypted message: {} bytes", decrypted.len());
                        decrypted
                    }
                    Err(e) => {
                        tracing::error!("webhook decrypt error: {e}");
                        return (StatusCode::BAD_REQUEST, "decrypt error").into_response();
                    }
                }
            }
            Err(_) => body.clone(), // 非安全模式 XML，直接用原文
        }
    } else {
        body.clone()
    };

    let msg: WxMessage = match quick_xml::de::from_str(&xml_to_parse) {
        Ok(m)  => m,
        Err(e) => {
            tracing::warn!("xml parse error: {e}");
            return (StatusCode::BAD_REQUEST, "bad xml").into_response();
        }
    };

    // 处理消息并生成回复文本
    let reply_text = if msg.msg_type == "event" {
        match msg.event.as_deref() {
            Some("subscribe") => {
                let _ = state.db.upsert_user(&msg.from_user_name, true).await;
                info!(openid = %msg.from_user_name, action = "subscribe");
                None
            }
            Some("unsubscribe") => {
                let _ = state.db.upsert_user(&msg.from_user_name, false).await;
                info!(openid = %msg.from_user_name, action = "unsubscribe");
                None
            }
            Some("CLICK") => {
                let event_key = msg.event_key.as_deref().unwrap_or("");
                info!(openid = %msg.from_user_name, event_key = %event_key, "menu click event");
                if event_key == "GET_VERIFY_CODE" {
                    // 生成 6 位验证码
                    let code: String = format!("{:06}", rand::random::<u32>() % 1_000_000);
                    let openid = msg.from_user_name.clone();
                    let expires = Utc::now() + chrono::Duration::minutes(3);

                    if let Err(e) = state.db.insert_code(&openid, &code, expires).await {
                        tracing::error!("insert verification_code: {e}");
                    }

                    info!(openid = %openid, code = %code, "verification code generated via menu click");
                    Some(format!("您的验证码是：{}\n\n有效期 3 分钟，请勿泄露。", code))
                } else {
                    None
                }
            }
            _ => None,
        }
    } else if msg.msg_type == "text" {
        // 处理文本消息
        if let Some(ref content) = msg.content {
            let content_lower = content.to_lowercase();
            if content_lower.contains("验证码") || content_lower.contains("verify") || content_lower == "code" {
                // 生成 6 位验证码
                let code: String = format!("{:06}", rand::random::<u32>() % 1_000_000);
                let openid = msg.from_user_name.clone();
                let expires = Utc::now() + chrono::Duration::minutes(3);

                // 存入数据库
                if let Err(e) = state.db.insert_code(&openid, &code, expires).await {
                    tracing::error!("insert verification_code: {e}");
                }

                info!(openid = %openid, code = %code, "verification code generated");
                Some(format!("您的验证码是：{}\n\n有效期 3 分钟，请勿泄露。", code))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // 如果有回复内容，构建 XML 响应
    if let Some(ref text) = reply_text {
        let reply_xml = format!(
            "<xml><ToUserName><![CDATA[{}]]></ToUserName><FromUserName><![CDATA[{}]]></FromUserName><CreateTime>{}</CreateTime><MsgType><![CDATA[text]]></MsgType><Content><![CDATA[{}]]></Content></xml>",
            msg.from_user_name, msg.to_user_name, Utc::now().timestamp(), text
        );

        // 安全模式加密回复
        if aes_key.len() == 43 && !appid.is_empty() {
            match wx_encrypt(&reply_xml, &aes_key, &appid) {
                Ok(encrypted) => {
                    let timestamp = Utc::now().timestamp().to_string();
                    let nonce = "wechat_rs_nonce";
                    let sig = make_safe_signature(&token, &timestamp, nonce, &encrypted);
                    let resp_xml = format!(
                        "<xml><Encrypt><![CDATA[{}]]></Encrypt><MsgSignature><![CDATA[{}]]></MsgSignature><TimeStamp>{}</TimeStamp><Nonce>{}</Nonce></xml>",
                        encrypted, sig, timestamp, nonce
                    );
                    return (StatusCode::OK, resp_xml).into_response();
                }
                Err(e) => {
                    tracing::error!("encrypt reply error: {e}");
                }
            }
        }

        // 明文模式直接返回 XML
        return (StatusCode::OK, reply_xml).into_response();
    }

    // 安全模式需要加密空回复
    if aes_key.len() == 43 && !appid.is_empty() {
        let reply = "success";
        match wx_encrypt(reply, &aes_key, &appid) {
            Ok(encrypted) => {
                let timestamp = Utc::now().timestamp().to_string();
                let nonce = "wechat_rs_nonce";
                let sig = make_safe_signature(&token, &timestamp, nonce, &encrypted);
                let resp_xml = format!(
                    "<xml><Encrypt><![CDATA[{}]]></Encrypt><MsgSignature><![CDATA[{}]]></MsgSignature><TimeStamp>{}</TimeStamp><Nonce>{}</Nonce></xml>",
                    encrypted, sig, timestamp, nonce
                );
                return (StatusCode::OK, resp_xml).into_response();
            }
            Err(e) => {
                tracing::error!("encrypt reply error: {e}");
            }
        }
    }

    (StatusCode::OK, "success").into_response()
}

async fn get_users(
    State(state): State<Arc<AppState>>,
    Query(p): Query<PageParams>,
) -> impl IntoResponse {
    match state.db.list_users(p.page, p.size).await {
        Ok(u)  => Json(u).into_response(),
        Err(e) => { tracing::error!("{e}"); (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response() }
    }
}

// ── 工具函数 ──────────────────────────────────────────────────────────────────

fn check_signature(token: &str, timestamp: &str, nonce: &str, sig: &str) -> bool {
    let mut parts = [token, timestamp, nonce];
    parts.sort_unstable();
    hex::encode(Sha1::digest(parts.concat().as_bytes())) == sig
}

// GET /api/wechat/user?code=xxx - 上游网站验证接口
async fn api_wechat_user(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let code = params.get("code").cloned().unwrap_or_default();
    info!("wechat_user request: code={}", code);

    // NewAPI 发送的 Authorization 头是原始 token（不带 Bearer 前缀）
    let auth_token = headers.get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let auth_ok = auth_token == state.wechat_server_token
        || auth_token == format!("Bearer {}", state.wechat_server_token);
    if !auth_ok {
        info!("wechat_user: unauthorized, token={}", auth_token);
        return (StatusCode::OK, Json(serde_json::json!({
            "success": false,
            "message": "unauthorized",
            "data": ""
        }))).into_response();
    }

    if code.is_empty() {
        return (StatusCode::OK, Json(serde_json::json!({
            "success": false,
            "message": "missing code parameter",
            "data": ""
        }))).into_response();
    }

    let now = Utc::now();

    // 查询验证码
    let row = state.db.validate_code(&code).await.ok().flatten();

    info!("wechat_user query result: {:?}", row.is_some());

    match row {
        Some((openid, _used, expires_at)) => {
            if expires_at < now {
                info!("wechat_user: code expired");
                return (StatusCode::OK, Json(serde_json::json!({
                    "success": false,
                    "message": "验证码已过期",
                    "data": ""
                }))).into_response();
            }
            // 不标记为已使用，支持轮询
            info!(code = %code, openid = %openid, "wechat_user validated");
            (StatusCode::OK, Json(serde_json::json!({
                "success": true,
                "message": "",
                "data": openid
            }))).into_response()
        }
        None => {
            info!("wechat_user: code not found");
            (StatusCode::OK, Json(serde_json::json!({
                "success": false,
                "message": "验证码错误或已过期",
                "data": ""
            }))).into_response()
        }
    }
}

// ── 微信 access_token 与自定义菜单 ────────────────────────────────────────────

/// 获取微信 access_token
async fn get_access_token(appid: &str, appsecret: &str) -> Result<String, String> {
    let url = format!(
        "https://api.weixin.qq.com/cgi-bin/token?grant_type=client_credential&appid={}&secret={}",
        appid, appsecret
    );
    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await.map_err(|e| format!("request error: {e}"))?;
    let body: serde_json::Value = resp.json().await.map_err(|e| format!("json error: {e}"))?;
    body.get("access_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("no access_token: {}", body))
}

/// 创建自定义菜单
async fn create_menu(access_token: &str, menu_json: &serde_json::Value) -> Result<String, String> {
    let url = format!(
        "https://api.weixin.qq.com/cgi-bin/menu/create?access_token={}",
        access_token
    );
    let client = reqwest::Client::new();
    let resp = client.post(&url)
        .json(menu_json)
        .send().await.map_err(|e| format!("request error: {e}"))?;
    let body: serde_json::Value = resp.json().await.map_err(|e| format!("json error: {e}"))?;
    let errcode = body.get("errcode").and_then(|v| v.as_i64()).unwrap_or(-1);
    if errcode == 0 {
        Ok("菜单创建成功，取消关注后重新关注即可立即看到菜单".into())
    } else {
        Err(format!("菜单创建失败: {}", body))
    }
}

// POST /admin/menu/create - 创建自定义菜单
async fn admin_create_menu(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // 验证管理员 JWT
    let tok = match headers.get("Authorization").and_then(|v| v.to_str().ok()).and_then(|s| s.strip_prefix("Bearer ")) {
        Some(t) => t.to_owned(),
        None => return (StatusCode::UNAUTHORIZED, "missing token").into_response(),
    };
    if jsonwebtoken::decode::<serde_json::Value>(
        &tok,
        &jsonwebtoken::DecodingKey::from_secret(state.admin_secret.as_bytes()),
        &jsonwebtoken::Validation::default(),
    ).is_err() {
        return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
    }

    let cfg = state.config.read().await;
    let appid = cfg.wechat_appid.clone();
    let appsecret = cfg.wechat_appsecret.clone();
    drop(cfg);

    if appid.is_empty() || appsecret.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "success": false, "message": "请先配置 AppID 和 AppSecret"
        }))).into_response();
    }

    let access_token = match get_access_token(&appid, &appsecret).await {
        Ok(t) => t,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "success": false, "message": format!("获取 access_token 失败: {}", e)
            }))).into_response();
        }
    };

    let menu = serde_json::json!({
        "button": [
            {
                "type": "click",
                "name": "获取验证码",
                "key": "GET_VERIFY_CODE"
            }
        ]
    });

    match create_menu(&access_token, &menu).await {
        Ok(msg) => (StatusCode::OK, Json(serde_json::json!({
            "success": true, "message": msg
        }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "success": false, "message": e
        }))).into_response(),
    }
}

// ── 启动 ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "wechat_rs=info,tower_http=debug".into()))
        .init();

    let admin_secret  = env::var("ADMIN_SECRET").unwrap_or_else(|_| "change_me_jwt_secret".into());
    let wechat_server_token = env::var("WECHAT_SERVER_TOKEN").unwrap_or_else(|_| "change_me_wechat_token".into());
    let addr          = env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".into());
    let storage_type  = env::var("STORAGE_TYPE").unwrap_or_else(|_| "postgres".into());

    let db: Arc<dyn Storage> = match storage_type.as_str() {
        "redis" => {
            let redis_url = env::var("REDIS_URL").expect("REDIS_URL not set for redis storage");
            let store = storage::redis_store::RedisStorage::new(&redis_url)
                .await
                .expect("failed to connect to redis");
            info!("using Redis storage: {}", redis_url);
            Arc::new(store)
        }
        _ => {
            let database_url = env::var("DATABASE_URL").expect("DATABASE_URL not set");
            let pool = PgPoolOptions::new().max_connections(10)
                .connect(&database_url).await.expect("failed to connect to postgres");

            // 建表 (仅 PostgreSQL)
            sqlx::query(r#"
                CREATE TABLE IF NOT EXISTS wechat_users (
                    openid TEXT PRIMARY KEY, nickname TEXT DEFAULT '',
                    headimgurl TEXT DEFAULT '', subscribe BOOLEAN DEFAULT TRUE,
                    created_at TIMESTAMPTZ, updated_at TIMESTAMPTZ
                )
            "#).execute(&pool).await.expect("migrate failed: wechat_users");

            sqlx::query(r#"
                CREATE TABLE IF NOT EXISTS app_config (
                    key TEXT PRIMARY KEY, value TEXT NOT NULL
                )
            "#).execute(&pool).await.expect("migrate failed: app_config");

            sqlx::query(r#"
                CREATE TABLE IF NOT EXISTS verification_codes (
                    id SERIAL PRIMARY KEY,
                    openid TEXT NOT NULL,
                    code TEXT NOT NULL,
                    purpose TEXT DEFAULT '',
                    upstream_id TEXT DEFAULT '',
                    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                    expires_at TIMESTAMPTZ NOT NULL DEFAULT (NOW() + INTERVAL '5 minutes'),
                    used BOOLEAN NOT NULL DEFAULT FALSE
                )
            "#).execute(&pool).await.expect("migrate failed: verification_codes");

            // 为上游网站添加字段
            sqlx::query(r#"
                ALTER TABLE verification_codes ADD COLUMN IF NOT EXISTS purpose TEXT DEFAULT '';
                ALTER TABLE verification_codes ADD COLUMN IF NOT EXISTS upstream_id TEXT DEFAULT '';
            "#).execute(&pool).await.ok();

            info!("using PostgreSQL storage: {}", database_url);
            Arc::new(storage::postgres::PgStorage::new(pool))
        }
    };

    let cfg_json = db.load_config().await.ok().flatten();
    let cfg: AppConfig = match cfg_json {
        Some(json) => serde_json::from_str(&json).unwrap_or_default(),
        None => AppConfig::default(),
    };
    info!("config loaded, wechat_token={}, aes_key={}",
        if cfg.wechat_token.is_empty() { "<empty>" } else { "***" },
        if cfg.wechat_encoding_aes_key.is_empty() { "<empty>" } else { "***" });

    let state = Arc::new(AppState {
        db,
        config: Arc::new(RwLock::new(cfg)),
        admin_secret,
        wechat_server_token,
        started_at: Instant::now(),
    });

    let app = Router::new()
        .route("/wx",    get(verify).post(webhook))
        .route("/users", get(get_users))
        .route("/api/wechat/user",    get(api_wechat_user))
        .route("/admin/menu/create", post(admin_create_menu))
        .nest("/admin",  admin::router(state.clone()))
        .with_state(state)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(tower_http::cors::CorsLayer::permissive());

    info!("listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
