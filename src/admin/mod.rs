//! 管理后台模块

mod handlers;
mod ui;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use bcrypt::verify;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::AppState;

// ── JWT ───────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: i64,
}

pub fn make_token(secret: &str) -> String {
    let claims = Claims {
        sub: "admin".into(),
        exp: (Utc::now() + Duration::hours(24)).timestamp(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap()
}

pub fn verify_token(secret: &str, token: &str) -> bool {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .is_ok()
}

pub fn extract_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("Authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

/// 认证宏：验证 JWT token
macro_rules! auth {
    ($state:expr, $headers:expr) => {{
        let tok = match $crate::admin::extract_token(&$headers) {
            Some(t) => t.to_owned(),
            None => return (StatusCode::UNAUTHORIZED, "missing token").into_response(),
        };
        if !$crate::admin::verify_token(&$state.admin_secret, &tok) {
            return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
        }
    }};
}

pub(crate) use auth;

// ── 工具函数 ──────────────────────────────────────────────────────────────────

pub fn mask(s: &str) -> String {
    if s.len() <= 4 {
        return "****".into();
    }
    format!("{}****", &s[..4])
}

// ── UI 路由 ───────────────────────────────────────────────────────────────────

async fn ui() -> impl IntoResponse {
    Html(ui::ADMIN_HTML)
}

// ── 路由组装 ──────────────────────────────────────────────────────────────────

pub fn router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(ui))
        .route("/login", post(handlers::login))
        .route(
            "/config",
            get(handlers::get_config).put(handlers::update_config),
        )
        .route("/stats", get(handlers::stats))
        .route("/stats/detailed", get(handlers::stats_detailed))
        .route("/codes", get(handlers::codes_list))
        .route("/users", get(handlers::users))
        .route("/users/search", get(handlers::users_search))
        .route("/users/:openid/codes", get(handlers::user_codes))
        .route("/health", get(handlers::health))
        .with_state(state)
}

// ── 菜单创建路由（需单独注册在顶层） ─────────────────────────────────────────

pub async fn create_menu(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let tok = match headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
    {
        Some(t) => t.to_owned(),
        None => return (StatusCode::UNAUTHORIZED, "missing token").into_response(),
    };
    if jsonwebtoken::decode::<serde_json::Value>(
        &tok,
        &jsonwebtoken::DecodingKey::from_secret(state.admin_secret.as_bytes()),
        &jsonwebtoken::Validation::default(),
    )
    .is_err()
    {
        return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
    }

    let cfg = state.config.read().await;
    let appid = cfg.wechat_appid.clone();
    let appsecret = cfg.wechat_appsecret.clone();
    drop(cfg);

    if appid.is_empty() || appsecret.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false, "message": "请先配置 AppID 和 AppSecret"
            })),
        )
            .into_response();
    }

    let access_token = match crate::wechat::get_access_token(&appid, &appsecret).await {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false, "message": format!("获取 access_token 失败: {}", e)
                })),
            )
                .into_response();
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

    match crate::wechat::create_menu(&access_token, &menu).await {
        Ok(msg) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": true, "message": msg
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false, "message": e
            })),
        )
            .into_response(),
    }
}
