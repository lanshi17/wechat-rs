//! 微信消息处理：XML 类型、webhook、验证、菜单

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use std::sync::Arc;
use tracing::info;

use crate::crypto::{wx_decrypt, wx_encrypt, make_safe_signature, check_signature};
use crate::{AppState, PageParams};

// ── XML 消息类型 ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename = "xml")]
#[allow(dead_code)]
pub struct WxMessage {
    #[serde(rename = "ToUserName")]   pub to_user_name:   String,
    #[serde(rename = "FromUserName")] pub from_user_name: String,
    #[serde(rename = "MsgType")]      pub msg_type:       String,
    #[serde(rename = "Event")]        pub event:           Option<String>,
    #[serde(rename = "EventKey")]     pub event_key:       Option<String>,
    #[serde(rename = "Content")]      pub content:         Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename = "xml")]
pub struct WxEnvelope {
    #[serde(rename = "Encrypt")]      pub encrypt:      String,
    #[serde(rename = "MsgSignature")] pub msg_signature: Option<String>,
    #[serde(rename = "TimeStamp")]    pub timestamp:    Option<String>,
    #[serde(rename = "Nonce")]        pub nonce:        Option<String>,
}

// ── 查询参数 ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct VerifyParams {
    pub signature:   String,
    pub timestamp:   String,
    pub nonce:       String,
    pub echostr:     String,
    #[serde(default)] pub msg_signature: Option<String>,
    #[serde(default)] pub encrypt:       Option<String>,
}

// ── 路由处理器 ────────────────────────────────────────────────────────────────

/// GET /wx — 微信服务器验证
pub async fn verify(
    State(state): State<Arc<AppState>>,
    Query(p): Query<VerifyParams>,
) -> impl IntoResponse {
    let cfg = state.config.read().await;
    let token = &cfg.wechat_token;
    let aes_key = &cfg.wechat_encoding_aes_key;

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

    if check_signature(token, &p.timestamp, &p.nonce, &p.signature) {
        p.echostr.into_response()
    } else {
        (StatusCode::FORBIDDEN, "forbidden").into_response()
    }
}

/// POST /wx — 微信消息回调
pub async fn webhook(
    State(state): State<Arc<AppState>>,
    body: String,
) -> impl IntoResponse {
    let cfg = state.config.read().await;
    let aes_key = cfg.wechat_encoding_aes_key.clone();
    let appid = cfg.wechat_appid.clone();
    let token = cfg.wechat_token.clone();
    drop(cfg);

    let xml_to_parse = if aes_key.len() == 43 {
        match quick_xml::de::from_str::<WxEnvelope>(&body) {
            Ok(env) => {
                let ts = env.timestamp.as_deref().unwrap_or("");
                let nc = env.nonce.as_deref().unwrap_or("");
                if let Some(ref sig) = env.msg_signature {
                    let expected = make_safe_signature(&token, ts, nc, &env.encrypt);
                    if expected != *sig {
                        tracing::warn!("webhook: signature mismatch");
                        return (StatusCode::FORBIDDEN, "signature mismatch").into_response();
                    }
                }
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
            Err(_) => body.clone(),
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

    let reply_text = handle_message(&state, &msg).await;

    if let Some(ref text) = reply_text {
        let reply_xml = format!(
            "<xml><ToUserName><![CDATA[{}]]></ToUserName><FromUserName><![CDATA[{}]]></FromUserName><CreateTime>{}</CreateTime><MsgType><![CDATA[text]]></MsgType><Content><![CDATA[{}]]></Content></xml>",
            msg.from_user_name, msg.to_user_name, Utc::now().timestamp(), text
        );

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

        return (StatusCode::OK, reply_xml).into_response();
    }

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

/// GET /users — 公开用户列表（分页）
pub async fn get_users(
    State(state): State<Arc<AppState>>,
    Query(p): Query<PageParams>,
) -> impl IntoResponse {
    match state.db.list_users(p.page, p.size).await {
        Ok(u)  => Json(u).into_response(),
        Err(e) => { tracing::error!("{e}"); (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response() }
    }
}

// ── 消息处理逻辑 ──────────────────────────────────────────────────────────────

async fn handle_message(state: &AppState, msg: &WxMessage) -> Option<String> {
    if msg.msg_type == "event" {
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
                    Some(generate_code(state, &msg.from_user_name).await)
                } else {
                    None
                }
            }
            _ => None,
        }
    } else if msg.msg_type == "text" {
        if let Some(ref content) = msg.content {
            let content_lower = content.to_lowercase();
            if content_lower.contains("验证码") || content_lower.contains("verify") || content_lower == "code" {
                Some(generate_code(state, &msg.from_user_name).await)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    }
}

async fn generate_code(state: &AppState, openid: &str) -> String {
    let code: String = format!("{:06}", rand::random::<u32>() % 1_000_000);
    let expires = Utc::now() + chrono::Duration::minutes(3);

    if let Err(e) = state.db.insert_code(openid, &code, expires).await {
        tracing::error!("insert verification_code: {e}");
    }

    info!(openid = %openid, code = %code, "verification code generated");
    format!("您的验证码是：{}\n\n有效期 3 分钟，请勿泄露。", code)
}

// ── 微信 API：access_token 与自定义菜单 ──────────────────────────────────────

pub async fn get_access_token(appid: &str, appsecret: &str) -> Result<String, String> {
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

pub async fn create_menu(access_token: &str, menu_json: &serde_json::Value) -> Result<String, String> {
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
