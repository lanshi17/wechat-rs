//! 微信消息处理：XML 类型、webhook、验证、菜单

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use std::sync::Arc;
use tracing::info;

use crate::crypto::{
    check_signature, constant_time_eq, make_safe_signature, wx_decrypt, wx_encrypt,
};
use crate::storage::CallbackClaim;
use crate::{AppState, PageParams};

// ── XML 消息类型 ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename = "xml")]
#[allow(dead_code)]
pub struct WxMessage {
    #[serde(rename = "ToUserName")]
    pub to_user_name: String,
    #[serde(rename = "FromUserName")]
    pub from_user_name: String,
    #[serde(rename = "MsgType")]
    pub msg_type: String,
    #[serde(rename = "Event")]
    pub event: Option<String>,
    #[serde(rename = "EventKey")]
    pub event_key: Option<String>,
    #[serde(rename = "Content")]
    pub content: Option<String>,
    #[serde(rename = "Recognition")]
    pub recognition: Option<String>,
    #[serde(rename = "Label")]
    pub label: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename = "xml")]
pub struct WxEnvelope {
    #[serde(rename = "Encrypt")]
    pub encrypt: String,
}

// ── 查询参数 ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct VerifyParams {
    #[serde(default)]
    pub signature: Option<String>,
    pub timestamp: String,
    pub nonce: String,
    pub echostr: String,
    #[serde(default)]
    pub msg_signature: Option<String>,
    #[serde(default)]
    pub encrypt_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    #[serde(default)]
    signature: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    nonce: Option<String>,
    #[serde(default)]
    msg_signature: Option<String>,
    #[serde(default)]
    encrypt_type: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallbackMode {
    Plaintext,
    Aes,
}

#[derive(Debug, PartialEq, Eq)]
enum CallbackError {
    Forbidden,
    InvalidBody,
}

#[derive(Debug, PartialEq, Eq)]
struct AuthenticatedCallback {
    mode: CallbackMode,
    xml: String,
    replay_key: String,
}

const MAX_CALLBACK_CLOCK_SKEW_SECS: u64 = 5 * 60;
const CALLBACK_LEASE_SECS: i64 = 30;
const CALLBACK_REPLAY_RESERVATION_MINUTES: i64 = 10;

fn authenticate_callback(
    token: &str,
    aes_key: &str,
    appid: &str,
    params: &CallbackParams,
    body: &str,
) -> Result<AuthenticatedCallback, CallbackError> {
    authenticate_callback_at(token, aes_key, appid, params, body, Utc::now().timestamp())
}

fn authenticate_callback_at(
    token: &str,
    aes_key: &str,
    appid: &str,
    params: &CallbackParams,
    body: &str,
    now_timestamp: i64,
) -> Result<AuthenticatedCallback, CallbackError> {
    if token.trim().is_empty() {
        return Err(CallbackError::Forbidden);
    }
    let timestamp = params
        .timestamp
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or(CallbackError::Forbidden)?;
    let signed_timestamp = timestamp
        .parse::<i64>()
        .map_err(|_| CallbackError::Forbidden)?;
    if now_timestamp.abs_diff(signed_timestamp) > MAX_CALLBACK_CLOCK_SKEW_SECS {
        return Err(CallbackError::Forbidden);
    }
    let nonce = params
        .nonce
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or(CallbackError::Forbidden)?;

    let envelope = quick_xml::de::from_str::<WxEnvelope>(body).ok();
    let is_aes = params.encrypt_type.as_deref() == Some("aes")
        || params.msg_signature.is_some()
        || envelope.is_some();

    if is_aes {
        let encrypted = envelope
            .as_ref()
            .map(|value| value.encrypt.as_str())
            .ok_or(CallbackError::InvalidBody)?;
        let msg_signature = params
            .msg_signature
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or(CallbackError::Forbidden)?;
        let expected = make_safe_signature(token, timestamp, nonce, encrypted);
        if !constant_time_eq(expected.as_bytes(), msg_signature.as_bytes()) {
            return Err(CallbackError::Forbidden);
        }
        let xml = wx_decrypt(encrypted, aes_key, appid).map_err(|_| CallbackError::InvalidBody)?;
        return Ok(AuthenticatedCallback {
            mode: CallbackMode::Aes,
            xml,
            replay_key: format!("aes:{msg_signature}"),
        });
    }

    let signature = params
        .signature
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or(CallbackError::Forbidden)?;

    if !check_signature(token, timestamp, nonce, signature) {
        return Err(CallbackError::Forbidden);
    }

    Ok(AuthenticatedCallback {
        mode: CallbackMode::Plaintext,
        xml: body.to_string(),
        replay_key: format!("plain:{signature}"),
    })
}

fn cdata(value: &str) -> String {
    format!("<![CDATA[{}]]>", value.replace("]]>", "]]]]><![CDATA[>"))
}

fn valid_wechat_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn text_reply_xml(to_user: &str, from_user: &str, timestamp: i64, content: &str) -> String {
    format!(
        "<xml><ToUserName>{}</ToUserName><FromUserName>{}</FromUserName><CreateTime>{}</CreateTime><MsgType><![CDATA[text]]></MsgType><Content>{}</Content></xml>",
        cdata(to_user),
        cdata(from_user),
        timestamp,
        cdata(content),
    )
}

fn encrypted_reply_xml(
    token: &str,
    aes_key: &str,
    appid: &str,
    plaintext: &str,
    timestamp: i64,
) -> Result<String, String> {
    let encrypted = wx_encrypt(plaintext, aes_key, appid)?;
    let nonce = hex::encode(rand::random::<[u8; 16]>());
    let timestamp = timestamp.to_string();
    let signature = make_safe_signature(token, &timestamp, &nonce, &encrypted);
    Ok(format!(
        "<xml><Encrypt>{}</Encrypt><MsgSignature>{}</MsgSignature><TimeStamp>{}</TimeStamp><Nonce>{}</Nonce></xml>",
        cdata(&encrypted),
        cdata(&signature),
        timestamp,
        cdata(&nonce),
    ))
}

// ── 路由处理器 ────────────────────────────────────────────────────────────────

#[derive(Debug)]
enum VerifyError {
    Forbidden,
    InvalidCiphertext,
}

fn verify_echo(
    token: &str,
    aes_key: &str,
    appid: &str,
    params: &VerifyParams,
) -> Result<String, VerifyError> {
    if token.trim().is_empty()
        || params.timestamp.trim().is_empty()
        || params.nonce.trim().is_empty()
    {
        return Err(VerifyError::Forbidden);
    }
    let is_aes = params.encrypt_type.as_deref() == Some("aes") || params.msg_signature.is_some();
    if is_aes {
        let msg_signature = params
            .msg_signature
            .as_deref()
            .ok_or(VerifyError::Forbidden)?;
        let expected =
            make_safe_signature(token, &params.timestamp, &params.nonce, &params.echostr);
        if !constant_time_eq(expected.as_bytes(), msg_signature.as_bytes()) {
            return Err(VerifyError::Forbidden);
        }
        return wx_decrypt(&params.echostr, aes_key, appid)
            .map_err(|_| VerifyError::InvalidCiphertext);
    }

    if params.signature.as_deref().is_some_and(|signature| {
        check_signature(token, &params.timestamp, &params.nonce, signature)
    }) {
        Ok(params.echostr.clone())
    } else {
        Err(VerifyError::Forbidden)
    }
}

/// GET /wx — 微信服务器验证
pub async fn verify(
    State(state): State<Arc<AppState>>,
    Query(p): Query<VerifyParams>,
) -> impl IntoResponse {
    let cfg = state.config.read().await;
    let token = cfg.wechat_token.trim().to_string();
    let aes_key = cfg.wechat_encoding_aes_key.trim().to_string();
    let appid = cfg.wechat_appid.trim().to_string();

    match verify_echo(&token, &aes_key, &appid, &p) {
        Ok(echo) => echo.into_response(),
        Err(VerifyError::Forbidden) => (StatusCode::FORBIDDEN, "forbidden").into_response(),
        Err(VerifyError::InvalidCiphertext) => {
            (StatusCode::BAD_REQUEST, "invalid echostr").into_response()
        }
    }
}

/// POST /wx — 微信消息回调
pub async fn webhook(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CallbackParams>,
    body: String,
) -> impl IntoResponse {
    let cfg = state.config.read().await;
    let aes_key = cfg.wechat_encoding_aes_key.trim().to_string();
    let appid = cfg.wechat_appid.trim().to_string();
    let token = cfg.wechat_token.trim().to_string();
    drop(cfg);

    let authenticated = match authenticate_callback(&token, &aes_key, &appid, &params, &body) {
        Ok(callback) => callback,
        Err(CallbackError::Forbidden) => {
            tracing::warn!("webhook: signature mismatch or missing signature metadata");
            return (StatusCode::FORBIDDEN, "signature mismatch").into_response();
        }
        Err(CallbackError::InvalidBody) => {
            tracing::warn!("webhook: invalid encrypted body");
            return (StatusCode::BAD_REQUEST, "invalid body").into_response();
        }
    };
    let AuthenticatedCallback {
        mode: reply_mode,
        xml: xml_to_parse,
        replay_key,
    } = authenticated;

    let msg: WxMessage = match quick_xml::de::from_str(&xml_to_parse) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("xml parse error: {e}");
            return (StatusCode::BAD_REQUEST, "bad xml").into_response();
        }
    };
    if !valid_wechat_identifier(&msg.to_user_name) || !valid_wechat_identifier(&msg.from_user_name)
    {
        tracing::warn!("webhook: invalid WeChat user identifier");
        return (StatusCode::BAD_REQUEST, "invalid user identifier").into_response();
    }

    let lease_id = format!("{replay_key}-{}", rand::random::<u64>());
    let lease_expires_at = Utc::now() + chrono::Duration::seconds(CALLBACK_LEASE_SECS);
    let claim = match state
        .db
        .begin_callback(&replay_key, &lease_id, lease_expires_at)
        .await
    {
        Ok(claim) => claim,
        Err(error) => {
            tracing::error!("webhook replay guard unavailable: {error}");
            return (StatusCode::SERVICE_UNAVAILABLE, "temporarily unavailable").into_response();
        }
    };
    match claim {
        CallbackClaim::Acquired => {}
        CallbackClaim::InProgress => {
            info!(action = "duplicate_callback_in_progress");
            return (StatusCode::OK, "success").into_response();
        }
        CallbackClaim::Completed(reply) => {
            info!(action = "duplicate_callback_replayed");
            return (StatusCode::OK, reply.unwrap_or_else(|| "success".into())).into_response();
        }
    }

    let reply_text = handle_message(&state, &msg).await;

    let final_reply = if let Some(ref text) = reply_text {
        let reply_xml = text_reply_xml(
            &msg.from_user_name,
            &msg.to_user_name,
            Utc::now().timestamp(),
            text,
        );

        if reply_mode == CallbackMode::Aes {
            match encrypted_reply_xml(&token, &aes_key, &appid, &reply_xml, Utc::now().timestamp())
            {
                Ok(response_xml) => Some(response_xml),
                Err(e) => {
                    tracing::error!("encrypt reply error: {e}");
                    return (StatusCode::INTERNAL_SERVER_ERROR, "encrypt error").into_response();
                }
            }
        } else {
            Some(reply_xml)
        }
    } else if reply_mode == CallbackMode::Aes {
        match encrypted_reply_xml(&token, &aes_key, &appid, "success", Utc::now().timestamp()) {
            Ok(response_xml) => Some(response_xml),
            Err(e) => {
                tracing::error!("encrypt reply error: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "encrypt error").into_response();
            }
        }
    } else {
        Some("success".to_string())
    };

    let reply_expires_at =
        Utc::now() + chrono::Duration::minutes(CALLBACK_REPLAY_RESERVATION_MINUTES);
    if let Err(error) = state
        .db
        .complete_callback(
            &replay_key,
            &lease_id,
            final_reply.as_deref(),
            reply_expires_at,
        )
        .await
    {
        tracing::warn!("webhook callback completion lost: {error}");
    }

    (
        StatusCode::OK,
        final_reply.unwrap_or_else(|| "success".into()),
    )
        .into_response()
}

/// GET /users — 受上游令牌保护的用户列表（分页）
pub async fn get_users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(p): Query<PageParams>,
) -> impl IntoResponse {
    let authorization = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    if state.wechat_server_token.trim().is_empty() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "server token is not configured",
        )
            .into_response();
    }
    if !crate::api::server_token_matches(&state.wechat_server_token, authorization) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }

    let p = p.normalized();
    match state.db.list_users(p.page, p.size).await {
        Ok(u) => Json(u).into_response(),
        Err(e) => {
            tracing::error!("{e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response()
        }
    }
}

// ── 消息处理逻辑 ──────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Eq)]
enum MessageAction {
    Subscribe(String),
    Unsubscribe,
    GenerateCode,
    Reply(String),
    Ignore,
}

fn is_code_command(content: &str) -> bool {
    let content = content.trim().to_lowercase();
    content.contains("验证码") || content.contains("verify") || content == "code"
}

fn is_help_command(content: &str) -> bool {
    matches!(content.trim().to_lowercase().as_str(), "help" | "帮助")
}

const HELP_MESSAGE: &str =
    "可用指令：\n- 验证码 / code：获取 6 位验证码\n- 帮助 / help：查看本说明";

fn message_action(message: &WxMessage, welcome_message: &str) -> MessageAction {
    if message.msg_type == "event" {
        return match message.event.as_deref() {
            Some("subscribe") => MessageAction::Subscribe(welcome_message.to_string()),
            Some("unsubscribe") => MessageAction::Unsubscribe,
            Some("CLICK") if message.event_key.as_deref() == Some("GET_VERIFY_CODE") => {
                MessageAction::GenerateCode
            }
            _ => MessageAction::Ignore,
        };
    }

    if message.msg_type == "text" {
        if message.content.as_deref().is_some_and(is_code_command) {
            return MessageAction::GenerateCode;
        }
        if message.content.as_deref().is_some_and(is_help_command) {
            return MessageAction::Reply(HELP_MESSAGE.to_string());
        }
    }

    if message.msg_type == "voice" && message.recognition.as_deref().is_some_and(is_code_command) {
        return MessageAction::GenerateCode;
    }
    if message.msg_type == "image" {
        return MessageAction::Reply("已收到您的图片，谢谢分享。".to_string());
    }
    if message.msg_type == "location" {
        let label = message.label.as_deref().unwrap_or("").trim();
        return if label.is_empty() {
            MessageAction::Reply("已收到您的位置信息，谢谢分享。".to_string())
        } else {
            MessageAction::Reply(format!("已收到您的位置：{label}。"))
        };
    }

    MessageAction::Ignore
}

async fn handle_message(state: &AppState, msg: &WxMessage) -> Option<String> {
    let welcome_message = state.config.read().await.welcome_message.clone();
    match message_action(msg, &welcome_message) {
        MessageAction::Subscribe(reply) => {
            if let Err(error) = state.db.upsert_user(&msg.from_user_name, true).await {
                tracing::error!("upsert subscribed user: {error}");
            }
            info!(action = "subscribe");
            Some(reply)
        }
        MessageAction::Unsubscribe => {
            if let Err(error) = state.db.upsert_user(&msg.from_user_name, false).await {
                tracing::error!("upsert unsubscribed user: {error}");
            }
            info!(action = "unsubscribe");
            None
        }
        MessageAction::GenerateCode => generate_code(state, &msg.from_user_name).await.into(),
        MessageAction::Reply(reply) => Some(reply),
        MessageAction::Ignore => None,
    }
}

async fn generate_code(state: &AppState, openid: &str) -> String {
    let db = state.db.clone();
    generate_code_with(openid.to_string(), move |openid, code, expires_at| {
        let db = db.clone();
        async move { db.insert_code(&openid, &code, expires_at).await }
    })
    .await
}

async fn generate_code_with<F, Fut>(openid: String, mut insert: F) -> String
where
    F: FnMut(String, String, chrono::DateTime<Utc>) -> Fut,
    Fut: std::future::Future<Output = Result<(), crate::storage::StorageError>>,
{
    const MAX_ATTEMPTS: usize = 5;

    for _ in 0..MAX_ATTEMPTS {
        let code = format!("{:06}", rand::random::<u32>() % 1_000_000);
        let expires = Utc::now() + chrono::Duration::minutes(3);

        match insert(openid.clone(), code.clone(), expires).await {
            Ok(()) => {
                info!("verification code generated");
                return format!("您的验证码是：{}\n\n有效期 3 分钟，请勿泄露。", code);
            }
            Err(crate::storage::StorageError::Conflict(_)) => {
                tracing::warn!("verification code collision; retrying");
            }
            Err(error) => {
                tracing::error!("insert verification code: {error}");
                return "验证码生成失败，请稍后重试。".to_string();
            }
        }
    }

    tracing::error!("verification code generation exhausted collision retries");
    "验证码生成失败，请稍后重试。".to_string()
}

// ── 微信 API：access_token 与自定义菜单 ──────────────────────────────────────

pub async fn get_access_token(appid: &str, appsecret: &str) -> Result<String, String> {
    let url = format!(
        "https://api.weixin.qq.com/cgi-bin/token?grant_type=client_credential&appid={}&secret={}",
        appid, appsecret
    );
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("request error: {}", e.without_url()))?;
    let body: serde_json::Value = resp.json().await.map_err(|e| format!("json error: {e}"))?;
    body.get("access_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("no access_token: {}", body))
}

pub async fn create_menu(
    access_token: &str,
    menu_json: &serde_json::Value,
) -> Result<String, String> {
    let url = format!(
        "https://api.weixin.qq.com/cgi-bin/menu/create?access_token={}",
        access_token
    );
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(menu_json)
        .send()
        .await
        .map_err(|e| format!("request error: {}", e.without_url()))?;
    let body: serde_json::Value = resp.json().await.map_err(|e| format!("json error: {e}"))?;
    let errcode = body.get("errcode").and_then(|v| v.as_i64()).unwrap_or(-1);
    if errcode == 0 {
        Ok("菜单创建成功，取消关注后重新关注即可立即看到菜单".into())
    } else {
        Err(format!("菜单创建失败: {}", body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use serde::Deserialize;
    use sha1::{Digest, Sha1};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    fn plain_signature(token: &str, timestamp: &str, nonce: &str) -> String {
        let mut parts = [token, timestamp, nonce];
        parts.sort_unstable();
        hex::encode(Sha1::digest(parts.concat().as_bytes()))
    }

    fn encoding_aes_key() -> String {
        base64::engine::general_purpose::STANDARD
            .encode([11_u8; 32])
            .trim_end_matches('=')
            .to_string()
    }

    fn authenticate_test_callback(
        token: &str,
        aes_key: &str,
        appid: &str,
        params: &CallbackParams,
        body: &str,
    ) -> Result<AuthenticatedCallback, CallbackError> {
        let signed_timestamp = params
            .timestamp
            .as_deref()
            .and_then(|value| value.parse().ok())
            .unwrap_or_default();
        authenticate_callback_at(token, aes_key, appid, params, body, signed_timestamp)
    }

    #[test]
    fn get_verification_accepts_plain_signature() {
        let params = VerifyParams {
            signature: Some(plain_signature("token", "123", "nonce")),
            timestamp: "123".into(),
            nonce: "nonce".into(),
            echostr: "plain-echo".into(),
            msg_signature: None,
            encrypt_type: None,
        };

        assert_eq!(
            verify_echo("token", "", "", &params).expect("valid signature"),
            "plain-echo"
        );
    }

    #[test]
    fn get_verification_fails_closed_when_token_is_empty() {
        let params = VerifyParams {
            signature: Some(plain_signature("", "123", "nonce")),
            timestamp: "123".into(),
            nonce: "nonce".into(),
            echostr: "must-not-pass".into(),
            msg_signature: None,
            encrypt_type: None,
        };

        assert!(matches!(
            verify_echo("", "", "", &params),
            Err(VerifyError::Forbidden)
        ));
    }

    #[test]
    fn get_verification_rejects_empty_signature_metadata() {
        let empty_timestamp = VerifyParams {
            signature: Some(plain_signature("token", "", "nonce")),
            timestamp: String::new(),
            nonce: "nonce".into(),
            echostr: "must-not-pass".into(),
            msg_signature: None,
            encrypt_type: None,
        };
        let empty_nonce = VerifyParams {
            signature: Some(plain_signature("token", "123", "")),
            timestamp: "123".into(),
            nonce: String::new(),
            echostr: "must-not-pass".into(),
            msg_signature: None,
            encrypt_type: None,
        };

        assert!(matches!(
            verify_echo("token", "", "", &empty_timestamp),
            Err(VerifyError::Forbidden)
        ));
        assert!(matches!(
            verify_echo("token", "", "", &empty_nonce),
            Err(VerifyError::Forbidden)
        ));
    }

    #[test]
    fn get_verification_rejects_missing_and_wrong_signatures() {
        let mut plaintext = VerifyParams {
            signature: None,
            timestamp: "123".into(),
            nonce: "nonce".into(),
            echostr: "plain-echo".into(),
            msg_signature: None,
            encrypt_type: None,
        };
        assert!(matches!(
            verify_echo("token", "", "", &plaintext),
            Err(VerifyError::Forbidden)
        ));
        plaintext.signature = Some("wrong".into());
        assert!(matches!(
            verify_echo("token", "", "", &plaintext),
            Err(VerifyError::Forbidden)
        ));

        let key = encoding_aes_key();
        let encrypted_echo = wx_encrypt("aes-echo", &key, "wx-app").expect("encrypt echo");
        let mut aes = VerifyParams {
            signature: None,
            timestamp: "456".into(),
            nonce: "nonce-aes".into(),
            echostr: encrypted_echo,
            msg_signature: None,
            encrypt_type: Some("aes".into()),
        };
        assert!(matches!(
            verify_echo("token", &key, "wx-app", &aes),
            Err(VerifyError::Forbidden)
        ));
        aes.msg_signature = Some("wrong".into());
        assert!(matches!(
            verify_echo("token", &key, "wx-app", &aes),
            Err(VerifyError::Forbidden)
        ));
    }

    #[test]
    fn get_verification_uses_msg_signature_over_encrypted_echostr() {
        let key = encoding_aes_key();
        let encrypted_echo = wx_encrypt("aes-echo", &key, "wx-app").expect("encrypt echo");
        let params = VerifyParams {
            signature: None,
            timestamp: "456".into(),
            nonce: "nonce-aes".into(),
            echostr: encrypted_echo.clone(),
            msg_signature: Some(make_safe_signature(
                "token",
                "456",
                "nonce-aes",
                &encrypted_echo,
            )),
            encrypt_type: Some("aes".into()),
        };

        assert_eq!(
            verify_echo("token", &key, "wx-app", &params).expect("valid AES signature"),
            "aes-echo"
        );
    }

    #[test]
    fn post_plaintext_callback_uses_plain_signature_even_when_aes_is_configured() {
        let body = "<xml><MsgType><![CDATA[text]]></MsgType></xml>";
        let params = CallbackParams {
            signature: Some(plain_signature("token", "789", "plain-nonce")),
            timestamp: Some("789".into()),
            nonce: Some("plain-nonce".into()),
            msg_signature: None,
            encrypt_type: None,
        };

        let authenticated =
            authenticate_test_callback("token", "configured-aes-key", "wx-app", &params, body)
                .expect("valid plaintext callback");

        assert_eq!(authenticated.mode, CallbackMode::Plaintext);
        assert_eq!(authenticated.xml, body);
    }

    #[test]
    fn post_callback_fails_closed_when_token_is_empty() {
        let params = CallbackParams {
            signature: Some(plain_signature("", "789", "nonce")),
            timestamp: Some("789".into()),
            nonce: Some("nonce".into()),
            msg_signature: None,
            encrypt_type: None,
        };

        assert_eq!(
            authenticate_test_callback("", "", "", &params, "<xml></xml>"),
            Err(CallbackError::Forbidden)
        );
    }

    #[test]
    fn post_callback_rejects_stale_signed_metadata() {
        let params = CallbackParams {
            signature: Some(plain_signature("token", "1", "nonce")),
            timestamp: Some("1".into()),
            nonce: Some("nonce".into()),
            msg_signature: None,
            encrypt_type: None,
        };

        assert_eq!(
            authenticate_callback("token", "", "", &params, "<xml></xml>"),
            Err(CallbackError::Forbidden)
        );
    }

    #[test]
    fn wechat_identifiers_reject_markup_and_key_separators() {
        assert!(valid_wechat_identifier("oAbc_123-XYZ"));
        assert!(!valid_wechat_identifier("<img src=x onerror=alert(1)>"));
        assert!(!valid_wechat_identifier("user:forged"));
        assert!(!valid_wechat_identifier(""));
    }

    #[test]
    fn post_callback_rejects_missing_and_wrong_query_signatures() {
        let body = "<xml><MsgType><![CDATA[text]]></MsgType></xml>";
        let mut plaintext = CallbackParams {
            signature: None,
            timestamp: Some("789".into()),
            nonce: Some("plain-nonce".into()),
            msg_signature: None,
            encrypt_type: None,
        };
        assert_eq!(
            authenticate_test_callback("token", "", "", &plaintext, body),
            Err(CallbackError::Forbidden)
        );
        plaintext.signature = Some("wrong".into());
        assert_eq!(
            authenticate_test_callback("token", "", "", &plaintext, body),
            Err(CallbackError::Forbidden)
        );

        let key = encoding_aes_key();
        let encrypted = wx_encrypt(body, &key, "wx-app").expect("encrypt callback");
        let envelope = format!("<xml><Encrypt><![CDATA[{encrypted}]]></Encrypt></xml>");
        let mut aes = CallbackParams {
            signature: None,
            timestamp: Some("901".into()),
            nonce: Some("aes-nonce".into()),
            msg_signature: None,
            encrypt_type: Some("aes".into()),
        };
        assert_eq!(
            authenticate_test_callback("token", &key, "wx-app", &aes, &envelope),
            Err(CallbackError::Forbidden)
        );
        aes.msg_signature = Some("wrong".into());
        assert_eq!(
            authenticate_test_callback("token", &key, "wx-app", &aes, &envelope),
            Err(CallbackError::Forbidden)
        );
    }

    #[test]
    fn post_aes_callback_ignores_signature_metadata_inside_xml() {
        let key = encoding_aes_key();
        let inner_xml = "<xml><MsgType><![CDATA[text]]></MsgType></xml>";
        let encrypted = wx_encrypt(inner_xml, &key, "wx-app").expect("encrypt callback");
        let signature = make_safe_signature("token", "901", "aes-nonce", &encrypted);
        let envelope = format!(
            "<xml><Encrypt><![CDATA[{encrypted}]]></Encrypt><MsgSignature>{signature}</MsgSignature><TimeStamp>901</TimeStamp><Nonce>aes-nonce</Nonce></xml>"
        );
        let params = CallbackParams {
            signature: None,
            timestamp: None,
            nonce: None,
            msg_signature: None,
            encrypt_type: Some("aes".into()),
        };

        assert_eq!(
            authenticate_test_callback("token", &key, "wx-app", &params, &envelope),
            Err(CallbackError::Forbidden)
        );
    }

    #[test]
    fn post_aes_callback_reads_signature_metadata_from_query() {
        let key = encoding_aes_key();
        let inner_xml = "<xml><MsgType><![CDATA[text]]></MsgType></xml>";
        let encrypted = wx_encrypt(inner_xml, &key, "wx-app").expect("encrypt callback");
        let envelope = format!("<xml><Encrypt><![CDATA[{encrypted}]]></Encrypt></xml>");
        let params = CallbackParams {
            signature: None,
            timestamp: Some("901".into()),
            nonce: Some("aes-nonce".into()),
            msg_signature: Some(make_safe_signature("token", "901", "aes-nonce", &encrypted)),
            encrypt_type: Some("aes".into()),
        };

        let authenticated = authenticate_test_callback("token", &key, "wx-app", &params, &envelope)
            .expect("valid AES callback");

        assert_eq!(authenticated.mode, CallbackMode::Aes);
        assert_eq!(authenticated.xml, inner_xml);
    }

    #[test]
    fn webhook_handler_extracts_callback_query_parameters() {
        fn accepts_query_handler<F, Fut>(_handler: F)
        where
            F: Fn(State<Arc<AppState>>, Query<CallbackParams>, String) -> Fut,
        {
        }

        accepts_query_handler(webhook);
    }

    #[test]
    fn users_handler_extracts_authorization_header() {
        fn accepts_authenticated_handler<F, Fut>(_handler: F)
        where
            F: Fn(State<Arc<AppState>>, axum::http::HeaderMap, Query<PageParams>) -> Fut,
        {
        }

        accepts_authenticated_handler(get_users);
    }

    #[test]
    fn text_reply_preserves_cdata_terminator_as_text() {
        #[derive(Debug, Deserialize)]
        #[serde(rename = "xml")]
        struct TextReply {
            #[serde(rename = "Content")]
            content: String,
        }

        let xml = text_reply_xml("to-user", "from-user", 123, "left]]>right");
        let parsed: TextReply = quick_xml::de::from_str(&xml).expect("well-formed reply XML");

        assert_eq!(parsed.content, "left]]>right");
    }

    #[test]
    fn encrypted_replies_use_fresh_random_nonces() {
        #[derive(Debug, Deserialize)]
        #[serde(rename = "xml")]
        struct EncryptedReply {
            #[serde(rename = "Nonce")]
            nonce: String,
        }

        let key = encoding_aes_key();
        let first = encrypted_reply_xml("token", &key, "wx-app", "reply", 123)
            .expect("first encrypted reply");
        let second = encrypted_reply_xml("token", &key, "wx-app", "reply", 123)
            .expect("second encrypted reply");
        let first: EncryptedReply = quick_xml::de::from_str(&first).expect("first reply XML");
        let second: EncryptedReply = quick_xml::de::from_str(&second).expect("second reply XML");

        assert_ne!(first.nonce, second.nonce);
    }

    #[test]
    fn subscribe_uses_configured_welcome_message() {
        let message = WxMessage {
            to_user_name: "account".into(),
            from_user_name: "openid".into(),
            msg_type: "event".into(),
            event: Some("subscribe".into()),
            event_key: None,
            content: None,
            recognition: None,
            label: None,
        };

        assert_eq!(
            message_action(&message, "欢迎关注，发送帮助查看指令。"),
            MessageAction::Subscribe("欢迎关注，发送帮助查看指令。".into())
        );
    }

    #[test]
    fn help_command_returns_usage_instructions() {
        let message = WxMessage {
            to_user_name: "account".into(),
            from_user_name: "openid".into(),
            msg_type: "text".into(),
            event: None,
            event_key: None,
            content: Some("帮助".into()),
            recognition: None,
            label: None,
        };

        assert_eq!(
            message_action(&message, "welcome"),
            MessageAction::Reply(
                "可用指令：\n- 验证码 / code：获取 6 位验证码\n- 帮助 / help：查看本说明".into()
            )
        );
    }

    #[test]
    fn voice_recognition_text_can_request_a_verification_code() {
        let xml = "<xml><ToUserName><![CDATA[account]]></ToUserName><FromUserName><![CDATA[openid]]></FromUserName><MsgType><![CDATA[voice]]></MsgType><Recognition><![CDATA[请给我验证码。]]></Recognition></xml>";
        let message: WxMessage = quick_xml::de::from_str(xml).expect("voice message");

        assert_eq!(
            message_action(&message, "welcome"),
            MessageAction::GenerateCode
        );
    }

    #[test]
    fn image_message_gets_a_friendly_confirmation() {
        let xml = "<xml><ToUserName><![CDATA[account]]></ToUserName><FromUserName><![CDATA[openid]]></FromUserName><MsgType><![CDATA[image]]></MsgType><PicUrl><![CDATA[https://example.com/pic.jpg]]></PicUrl><MediaId><![CDATA[media-id]]></MediaId></xml>";
        let message: WxMessage = quick_xml::de::from_str(xml).expect("image message");

        assert_eq!(
            message_action(&message, "welcome"),
            MessageAction::Reply("已收到您的图片，谢谢分享。".into())
        );
    }

    #[test]
    fn location_message_echoes_the_location_label() {
        let xml = "<xml><ToUserName><![CDATA[account]]></ToUserName><FromUserName><![CDATA[openid]]></FromUserName><MsgType><![CDATA[location]]></MsgType><Location_X>31.2</Location_X><Location_Y>121.5</Location_Y><Scale>15</Scale><Label><![CDATA[上海市]]></Label></xml>";
        let message: WxMessage = quick_xml::de::from_str(xml).expect("location message");

        assert_eq!(
            message_action(&message, "welcome"),
            MessageAction::Reply("已收到您的位置：上海市。".into())
        );
    }

    #[tokio::test]
    async fn code_storage_failure_does_not_return_an_invalid_code() {
        let reply = generate_code_with("openid".into(), |_openid, _code, _expires_at| async {
            Err(crate::storage::StorageError::Database("offline".into()))
        })
        .await;

        assert_eq!(reply, "验证码生成失败，请稍后重试。");
        assert!(!reply.chars().any(|character| character.is_ascii_digit()));
    }

    #[tokio::test]
    async fn active_code_collision_is_retried_before_replying() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_insert = attempts.clone();
        let reply = generate_code_with("openid".into(), move |_openid, _code, _expires_at| {
            let attempt = attempts_for_insert.fetch_add(1, Ordering::SeqCst);
            async move {
                if attempt == 0 {
                    Err(crate::storage::StorageError::Conflict("collision".into()))
                } else {
                    Ok(())
                }
            }
        })
        .await;

        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert!(reply.starts_with("您的验证码是："));
    }
}
