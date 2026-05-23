//! 微信服务后台入口

mod admin;
mod api;
mod crypto;
mod storage;
mod wechat;

use axum::{
    routing::{get, post},
    Router,
};
use dotenvy::dotenv;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use std::{env, sync::Arc, time::Instant};
use tokio::sync::RwLock;
use tracing::info;

use storage::Storage;

// ── 应用状态 ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub db:                 Arc<dyn Storage>,
    pub config:             Arc<RwLock<AppConfig>>,
    pub admin_secret:       String,
    pub wechat_server_token: String,
    pub started_at:         Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub wechat_token:              String,
    pub wechat_appid:              String,
    pub wechat_appsecret:          String,
    pub wechat_encoding_aes_key:   String,
    pub admin_password_hash:       String,
    pub welcome_message:           String,
    pub site_name:                 String,
    pub domain:                    String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            wechat_token:              env::var("WECHAT_TOKEN").unwrap_or_default(),
            wechat_appid:              env::var("WECHAT_APPID").unwrap_or_default(),
            wechat_appsecret:          env::var("WECHAT_APPSECRET").unwrap_or_default(),
            wechat_encoding_aes_key:   env::var("WECHAT_ENCODING_AES_KEY").unwrap_or_default(),
            admin_password_hash:       env::var("ADMIN_PASSWORD_HASH").unwrap_or_default(),
            welcome_message:           "感谢关注！".into(),
            site_name:                 env::var("SITE_NAME").unwrap_or_else(|_| "微信服务管理后台".into()),
            domain:                    env::var("DOMAIN").unwrap_or_else(|_| "localhost".into()),
        }
    }
}

#[derive(Deserialize)]
pub struct PageParams {
    #[serde(default = "default_page")] pub page: i64,
    #[serde(default = "default_size")] pub size: i64,
}
fn default_page() -> i64 { 1 }
fn default_size() -> i64 { 20 }

pub async fn save_config(db: &dyn Storage, cfg: &AppConfig) -> Result<(), storage::StorageError> {
    let json = serde_json::to_string(cfg).unwrap();
    db.save_config(&json).await
}

// ── 启动 ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "wechat_rs=info,tower_http=debug".into()))
        .init();

    let admin_secret         = env::var("ADMIN_SECRET").unwrap_or_else(|_| "change_me_jwt_secret".into());
    let wechat_server_token  = env::var("WECHAT_SERVER_TOKEN").unwrap_or_else(|_| "change_me_wechat_token".into());
    let addr                 = env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".into());
    let storage_type         = env::var("STORAGE_TYPE").unwrap_or_else(|_| "postgres".into());

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
        .route("/wx",                get(wechat::verify).post(wechat::webhook))
        .route("/users",             get(wechat::get_users))
        .route("/api/wechat/user",   get(api::wechat_user))
        .route("/admin/menu/create", post(admin::create_menu))
        .nest("/admin",              admin::router(state.clone()))
        .with_state(state)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(tower_http::cors::CorsLayer::permissive());

    info!("listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
