//! 公开 API 路由：验证码验证接口

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use std::sync::Arc;
use tracing::info;

use crate::AppState;

/// GET /api/wechat/user?code=xxx — 上游网站验证接口
pub async fn wechat_user(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let code = params.get("code").cloned().unwrap_or_default();
    info!("wechat_user request: code={}", code);

    let auth_token = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let auth_ok = auth_token == state.wechat_server_token
        || auth_token == format!("Bearer {}", state.wechat_server_token);
    if !auth_ok {
        info!("wechat_user: unauthorized, token={}", auth_token);
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": false,
                "message": "unauthorized",
                "data": ""
            })),
        )
            .into_response();
    }

    if code.is_empty() {
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "success": false,
                "message": "missing code parameter",
                "data": ""
            })),
        )
            .into_response();
    }

    let now = Utc::now();
    let row = state.db.validate_code(&code).await.ok().flatten();

    info!("wechat_user query result: {:?}", row.is_some());

    match row {
        Some((openid, _used, expires_at)) => {
            if expires_at < now {
                info!("wechat_user: code expired");
                return (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "success": false,
                        "message": "验证码已过期",
                        "data": ""
                    })),
                )
                    .into_response();
            }
            info!(code = %code, openid = %openid, "wechat_user validated");
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "message": "",
                    "data": openid
                })),
            )
                .into_response()
        }
        None => {
            info!("wechat_user: code not found");
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": false,
                    "message": "验证码错误或已过期",
                    "data": ""
                })),
            )
                .into_response()
        }
    }
}
