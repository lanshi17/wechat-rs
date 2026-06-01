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
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use std::{env, fs, path::PathBuf, sync::Arc, time::Instant};
use tokio::sync::RwLock;
use tracing::info;

use storage::Storage;

// ── TOML 配置文件结构 ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FileConfig {
    pub server: ServerSection,
    pub admin: AdminSection,
    pub wechat: WechatSection,
    pub upstream: UpstreamSection,
    pub storage: StorageSection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerSection {
    pub listen_addr: String,
    pub site_name: String,
    pub domain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AdminSection {
    pub password: String,
    pub secret: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WechatSection {
    pub token: String,
    pub appid: String,
    pub appsecret: String,
    pub encoding_aes_key: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UpstreamSection {
    pub server_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageSection {
    #[serde(rename = "type")]
    pub storage_type: String,
    pub database_url: String,
    pub redis_url: String,
}

impl Default for ServerSection {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:3000".into(),
            site_name: "微信服务管理后台".into(),
            domain: "localhost".into(),
        }
    }
}

impl Default for AdminSection {
    fn default() -> Self {
        Self {
            password: "admin123".into(),
            secret: "change_me_jwt_secret".into(),
        }
    }
}

impl Default for StorageSection {
    fn default() -> Self {
        Self {
            storage_type: "postgres".into(),
            database_url: String::new(),
            redis_url: String::new(),
        }
    }
}

/// 加载 TOML 配置文件；找不到时返回默认值
pub fn load_file_config(path: &PathBuf) -> FileConfig {
    match fs::read_to_string(path) {
        Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
            tracing::warn!("failed to parse {}: {e}, using defaults", path.display());
            FileConfig::default()
        }),
        Err(_) => {
            tracing::warn!("config file {} not found, using defaults", path.display());
            FileConfig::default()
        }
    }
}

/// 将运行时配置写回 TOML 文件（仅 wechat 和 server 部分）
pub fn write_back_toml(path: &PathBuf, cfg: &AppConfig) -> Result<(), String> {
    // 读取当前文件保留其他 section，若不存在则用默认值
    let mut file_cfg: FileConfig = fs::read_to_string(path)
        .ok()
        .and_then(|c| toml::from_str(&c).ok())
        .unwrap_or_default();

    file_cfg.server.site_name = cfg.site_name.clone();
    file_cfg.server.domain = cfg.domain.clone();
    file_cfg.wechat.token = cfg.wechat_token.clone();
    file_cfg.wechat.appid = cfg.wechat_appid.clone();
    file_cfg.wechat.appsecret = cfg.wechat_appsecret.clone();
    file_cfg.wechat.encoding_aes_key = cfg.wechat_encoding_aes_key.clone();

    let content = toml::to_string_pretty(&file_cfg).map_err(|e| format!("serialize toml: {e}"))?;
    fs::write(path, content).map_err(|e| format!("write {}: {e}", path.display()))
}

// ── 应用状态 ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<dyn Storage>,
    pub config: Arc<RwLock<AppConfig>>,
    pub admin_secret: String,
    pub admin_password: String,
    pub wechat_server_token: String,
    pub config_path: PathBuf,
    pub started_at: Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub wechat_token: String,
    pub wechat_appid: String,
    pub wechat_appsecret: String,
    pub wechat_encoding_aes_key: String,
    pub admin_password_hash: String,
    pub welcome_message: String,
    pub site_name: String,
    pub domain: String,
}

#[derive(Deserialize)]
pub struct PageParams {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_size")]
    pub size: i64,
}
fn default_page() -> i64 {
    1
}
fn default_size() -> i64 {
    20
}

pub async fn save_config(db: &dyn Storage, cfg: &AppConfig) -> Result<(), storage::StorageError> {
    let json = serde_json::to_string(cfg).unwrap();
    db.save_config(&json).await
}

// ── 启动 ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "wechat_rs=info,tower_http=debug".into()),
        )
        .init();

    // 加载 TOML 配置文件（优先 CONFIG_PATH 环境变量，否则当前目录 config.toml）
    let config_path =
        PathBuf::from(env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".into()));
    info!("loading config from: {}", config_path.display());
    let fc = load_file_config(&config_path);

    let addr = fc.server.listen_addr.clone();
    let admin_secret = fc.admin.secret.clone();
    let admin_password = fc.admin.password.clone();
    let wechat_server_token = fc.upstream.server_token.clone();
    let storage_type = fc.storage.storage_type.clone();

    // 从 TOML 构建 AppConfig 初始值
    let initial_cfg = AppConfig {
        wechat_token: fc.wechat.token.clone(),
        wechat_appid: fc.wechat.appid.clone(),
        wechat_appsecret: fc.wechat.appsecret.clone(),
        wechat_encoding_aes_key: fc.wechat.encoding_aes_key.clone(),
        admin_password_hash: String::new(),
        welcome_message: "感谢关注！".into(),
        site_name: fc.server.site_name.clone(),
        domain: fc.server.domain.clone(),
    };

    let db: Arc<dyn Storage> = match storage_type.as_str() {
        "redis" => {
            let redis_url = &fc.storage.redis_url;
            assert!(
                !redis_url.is_empty(),
                "storage.redis_url must be set for redis storage"
            );
            let store = storage::redis_store::RedisStorage::new(redis_url)
                .await
                .expect("failed to connect to redis");
            info!("using Redis storage: {}", redis_url);
            Arc::new(store)
        }
        _ => {
            let database_url = &fc.storage.database_url;
            assert!(
                !database_url.is_empty(),
                "storage.database_url must be set for postgres storage"
            );
            let pool = PgPoolOptions::new()
                .max_connections(10)
                .connect(database_url)
                .await
                .expect("failed to connect to postgres");

            sqlx::query(
                r#"
                CREATE TABLE IF NOT EXISTS wechat_users (
                    openid TEXT PRIMARY KEY, nickname TEXT DEFAULT '',
                    headimgurl TEXT DEFAULT '', subscribe BOOLEAN DEFAULT TRUE,
                    created_at TIMESTAMPTZ, updated_at TIMESTAMPTZ
                )
            "#,
            )
            .execute(&pool)
            .await
            .expect("migrate failed: wechat_users");

            sqlx::query(
                r#"
                CREATE TABLE IF NOT EXISTS app_config (
                    key TEXT PRIMARY KEY, value TEXT NOT NULL
                )
            "#,
            )
            .execute(&pool)
            .await
            .expect("migrate failed: app_config");

            sqlx::query(
                r#"
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
            "#,
            )
            .execute(&pool)
            .await
            .expect("migrate failed: verification_codes");

            sqlx::query(
                r#"
                ALTER TABLE verification_codes ADD COLUMN IF NOT EXISTS purpose TEXT DEFAULT '';
                ALTER TABLE verification_codes ADD COLUMN IF NOT EXISTS upstream_id TEXT DEFAULT '';
            "#,
            )
            .execute(&pool)
            .await
            .ok();

            info!("using PostgreSQL storage: {}", database_url);
            Arc::new(storage::postgres::PgStorage::new(pool))
        }
    };

    // 从 DB 加载运行时配置（DB 值覆盖 TOML 初始值）
    let cfg_json = db.load_config().await.ok().flatten();
    let cfg: AppConfig = match cfg_json {
        Some(json) => {
            let mut db_cfg: AppConfig = serde_json::from_str(&json).unwrap_or(initial_cfg.clone());
            // DB 中未设置的字段回退到 TOML 值
            if db_cfg.wechat_token.is_empty() {
                db_cfg.wechat_token = initial_cfg.wechat_token.clone();
            }
            if db_cfg.wechat_appid.is_empty() {
                db_cfg.wechat_appid = initial_cfg.wechat_appid.clone();
            }
            if db_cfg.wechat_appsecret.is_empty() {
                db_cfg.wechat_appsecret = initial_cfg.wechat_appsecret.clone();
            }
            if db_cfg.wechat_encoding_aes_key.is_empty() {
                db_cfg.wechat_encoding_aes_key = initial_cfg.wechat_encoding_aes_key.clone();
            }
            if db_cfg.site_name == "微信服务管理后台" && initial_cfg.site_name != "微信服务管理后台"
            {
                db_cfg.site_name = initial_cfg.site_name.clone();
            }
            if db_cfg.domain == "localhost" && initial_cfg.domain != "localhost" {
                db_cfg.domain = initial_cfg.domain.clone();
            }
            db_cfg
        }
        None => initial_cfg,
    };
    // Trim all config values to prevent whitespace issues from DB or TOML
    let mut cfg = cfg;
    cfg.wechat_token = cfg.wechat_token.trim().to_string();
    cfg.wechat_appid = cfg.wechat_appid.trim().to_string();
    cfg.wechat_appsecret = cfg.wechat_appsecret.trim().to_string();
    cfg.wechat_encoding_aes_key = cfg.wechat_encoding_aes_key.trim().to_string();
    cfg.site_name = cfg.site_name.trim().to_string();
    cfg.domain = cfg.domain.trim().to_string();
    // Persist trimmed values back to DB
    if let Err(e) = save_config(&*db, &cfg).await {
        tracing::warn!("failed to persist trimmed config: {e}");
    }

    info!(
        "config loaded, wechat_token={}, aes_key={}",
        if cfg.wechat_token.is_empty() {
            "<empty>"
        } else {
            "***"
        },
        if cfg.wechat_encoding_aes_key.is_empty() {
            "<empty>"
        } else {
            "***"
        }
    );

    let state = Arc::new(AppState {
        db,
        config: Arc::new(RwLock::new(cfg)),
        admin_secret,
        admin_password,
        wechat_server_token,
        config_path,
        started_at: Instant::now(),
    });

    let app = Router::new()
        .route("/wx", get(wechat::verify).post(wechat::webhook))
        .route("/users", get(wechat::get_users))
        .route("/api/wechat/user", get(api::wechat_user))
        .route("/admin/menu/create", post(admin::create_menu))
        .nest("/admin", admin::router(state.clone()))
        .with_state(state)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(tower_http::cors::CorsLayer::permissive());

    info!("listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
