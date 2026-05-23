//! 管理后台 API 处理器

use axum::{
    extract::{State, Query, Path},
    http::{StatusCode, HeaderMap},
    response::IntoResponse,
    Json,
};
use bcrypt::{hash, DEFAULT_COST};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{AppState, PageParams, save_config, storage};
use storage::CodeInfo;
use super::{make_token, mask, auth, verify as bcrypt_verify};

// ── 请求 / 响应结构 ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LoginReq { pub password: String }

#[derive(Serialize)]
pub struct LoginResp { pub token: String }

#[derive(Deserialize)]
pub struct UpdateConfigReq {
    pub wechat_token:     Option<String>,
    pub wechat_appid:     Option<String>,
    pub wechat_appsecret: Option<String>,
    pub wechat_encoding_aes_key: Option<String>,
    pub welcome_message:  Option<String>,
    pub new_password:     Option<String>,
    pub site_name:        Option<String>,
    pub domain:           Option<String>,
}

#[derive(Serialize)]
pub struct ConfigResp {
    pub wechat_token:     String,
    pub wechat_appid:     String,
    pub wechat_appsecret_masked: String,
    pub wechat_encoding_aes_key: String,
    pub welcome_message:  String,
    pub has_password:     bool,
    pub site_name:        String,
    pub domain:           String,
}

#[derive(Serialize)]
pub struct StatsResp {
    pub total_subscribers: i64,
}

#[derive(Serialize)]
pub struct DetailedStats {
    pub total_subscribers: i64,
    pub total_users: i64,
    pub today_new_users: i64,
    pub today_codes: i64,
    pub used_codes: i64,
    pub expired_codes: i64,
    pub total_codes: i64,
}

#[derive(Serialize)]
pub struct CodesResponse {
    pub codes: Vec<CodeInfo>,
    pub total: i64,
}

#[derive(Deserialize)]
pub struct SearchParams {
    pub q: Option<String>,
}

// ── 处理器 ────────────────────────────────────────────────────────────────────

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginReq>,
) -> impl IntoResponse {
    let hash_stored = state.config.read().await.admin_password_hash.clone();

    let ok = if hash_stored.is_empty() {
        let plain = std::env::var("ADMIN_PASSWORD").unwrap_or_default();
        body.password == plain
    } else {
        bcrypt_verify(&body.password, &hash_stored).unwrap_or(false)
    };

    if !ok {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error":"wrong password"}))).into_response();
    }

    if hash_stored.is_empty() {
        let h = hash(&body.password, DEFAULT_COST).unwrap();
        let mut cfg = state.config.write().await;
        cfg.admin_password_hash = h;
        let _ = save_config(&*state.db, &cfg).await;
    }

    Json(LoginResp { token: make_token(&state.admin_secret) }).into_response()
}

pub async fn get_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    auth!(state, headers);
    let cfg = state.config.read().await;
    Json(ConfigResp {
        wechat_token:           cfg.wechat_token.clone(),
        wechat_appid:           cfg.wechat_appid.clone(),
        wechat_appsecret_masked: mask(&cfg.wechat_appsecret),
        wechat_encoding_aes_key: mask(&cfg.wechat_encoding_aes_key),
        welcome_message:        cfg.welcome_message.clone(),
        has_password:           !cfg.admin_password_hash.is_empty(),
        site_name:              cfg.site_name.clone(),
        domain:                 cfg.domain.clone(),
    }).into_response()
}

pub async fn update_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<UpdateConfigReq>,
) -> impl IntoResponse {
    auth!(state, headers);
    let mut cfg = state.config.write().await;
    if let Some(v) = body.wechat_token    { cfg.wechat_token = v; }
    if let Some(v) = body.wechat_appid   { cfg.wechat_appid = v; }
    if let Some(v) = body.wechat_appsecret { cfg.wechat_appsecret = v; }
    if let Some(v) = body.wechat_encoding_aes_key { cfg.wechat_encoding_aes_key = v; }
    if let Some(v) = body.welcome_message { cfg.welcome_message = v; }
    if let Some(v) = body.site_name { cfg.site_name = v; }
    if let Some(v) = body.domain { cfg.domain = v; }
    if let Some(pw) = body.new_password {
        if !pw.is_empty() { cfg.admin_password_hash = hash(&pw, DEFAULT_COST).unwrap(); }
    }
    if let Err(e) = save_config(&*state.db, &cfg).await {
        tracing::error!("save config: {e}");
        return (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response();
    }
    Json(serde_json::json!({"ok": true})).into_response()
}

pub async fn stats(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    auth!(state, headers);
    match state.db.count_subscribers().await {
        Ok(n)  => Json(StatsResp { total_subscribers: n }).into_response(),
        Err(e) => { tracing::error!("{e}"); (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response() }
    }
}

pub async fn users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(p): Query<PageParams>,
) -> impl IntoResponse {
    auth!(state, headers);
    match state.db.list_users(p.page, p.size).await {
        Ok(u)  => Json(u).into_response(),
        Err(e) => { tracing::error!("{e}"); (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response() }
    }
}

pub async fn stats_detailed(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    auth!(state, headers);
    let db = &state.db;
    let subscribers = db.count_subscribers().await.unwrap_or(0);
    let total_users = db.count_total_users().await.unwrap_or(0);
    let today_new = db.count_today_new_users().await.unwrap_or(0);
    let today_codes = db.count_today_codes().await.unwrap_or(0);
    let used = db.count_used_codes().await.unwrap_or(0);
    let expired = db.count_expired_codes().await.unwrap_or(0);
    let total_codes = db.count_codes().await.unwrap_or(0);
    Json(DetailedStats {
        total_subscribers: subscribers,
        total_users,
        today_new_users: today_new,
        today_codes,
        used_codes: used,
        expired_codes: expired,
        total_codes,
    }).into_response()
}

pub async fn codes_list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(p): Query<PageParams>,
) -> impl IntoResponse {
    auth!(state, headers);
    let codes = match state.db.list_codes(p.page, p.size).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("list_codes error: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response();
        }
    };
    let total = state.db.count_codes().await.unwrap_or(0);
    Json(CodesResponse { codes, total }).into_response()
}

pub async fn users_search(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<SearchParams>,
) -> impl IntoResponse {
    auth!(state, headers);
    let q = params.q.unwrap_or_default();
    if q.is_empty() {
        return Json(Vec::<storage::UserInfo>::new()).into_response();
    }
    match state.db.search_users(&q).await {
        Ok(u) => Json(u).into_response(),
        Err(e) => { tracing::error!("{e}"); (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response() }
    }
}

pub async fn user_codes(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(openid): Path<String>,
) -> impl IntoResponse {
    auth!(state, headers);
    match state.db.get_user_codes(&openid).await {
        Ok(c) => Json(c).into_response(),
        Err(e) => { tracing::error!("{e}"); (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response() }
    }
}

pub async fn health(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    auth!(state, headers);
    let uptime_secs = state.started_at.elapsed().as_secs();
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let db_ok = state.db.health_check().await;
    let db_conns = state.db.connection_count().await;
    Json(serde_json::json!({
        "uptime_seconds": uptime_secs,
        "memory_total_mb": sys.total_memory() / 1024 / 1024,
        "memory_used_mb": sys.used_memory() / 1024 / 1024,
        "db_connected": db_ok,
        "db_connections": db_conns,
    })).into_response()
}
