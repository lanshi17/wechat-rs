pub mod postgres;
pub mod redis_store;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::fmt;

// ── Error ──────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum StorageError {
    Database(String),
    Conflict(String),
    NotFound,
    Other(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::Database(s) => write!(f, "database error: {}", s),
            StorageError::Conflict(s) => write!(f, "conflict: {}", s),
            StorageError::NotFound => write!(f, "not found"),
            StorageError::Other(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for StorageError {}

// ── Models ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct UserInfo {
    pub openid: String,
    pub nickname: String,
    pub headimgurl: String,
    pub subscribe: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodeInfo {
    pub id: i32,
    pub openid: String,
    pub code: String,
    pub purpose: Option<String>,
    pub used: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallbackClaim {
    Acquired,
    InProgress,
    Completed(Option<String>),
}

// ── Trait ──────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait Storage: Send + Sync + 'static {
    // Users
    async fn upsert_user(&self, openid: &str, subscribe: bool) -> Result<(), StorageError>;
    async fn list_users(&self, page: i64, size: i64) -> Result<Vec<UserInfo>, StorageError>;
    async fn count_subscribers(&self) -> Result<i64, StorageError>;
    async fn count_total_users(&self) -> Result<i64, StorageError>;
    async fn count_today_new_users(&self) -> Result<i64, StorageError>;
    async fn search_users(&self, query: &str) -> Result<Vec<UserInfo>, StorageError>;

    // Verification codes
    async fn insert_code(
        &self,
        openid: &str,
        code: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), StorageError>;
    async fn list_codes(&self, page: i64, size: i64) -> Result<Vec<CodeInfo>, StorageError>;
    async fn count_codes(&self) -> Result<i64, StorageError>;
    async fn count_today_codes(&self) -> Result<i64, StorageError>;
    async fn count_used_codes(&self) -> Result<i64, StorageError>;
    async fn count_expired_codes(&self) -> Result<i64, StorageError>;
    async fn get_user_codes(&self, openid: &str) -> Result<Vec<CodeInfo>, StorageError>;
    /// Atomically consumes the newest matching code when it is unused and has not expired.
    /// Concurrent calls for the same code must yield at most one `Some(openid)` result.
    /// A consumed value remains reserved until its original expiry to prevent a retry from
    /// consuming a newly issued code that happens to use the same six digits.
    async fn consume_code(
        &self,
        code: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<String>, StorageError>;

    // WeChat callbacks
    /// Acquires a short processing lease, reports another in-flight worker, or returns the
    /// cached reply from a completed callback. Only one lease may be active for a key.
    async fn begin_callback(
        &self,
        key: &str,
        lease_id: &str,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<CallbackClaim, StorageError>;
    /// Completes a callback only when `lease_id` still owns it and caches its passive reply.
    async fn complete_callback(
        &self,
        key: &str,
        lease_id: &str,
        reply: Option<&str>,
        expires_at: DateTime<Utc>,
    ) -> Result<(), StorageError>;

    // Config
    async fn load_config(&self) -> Result<Option<String>, StorageError>;
    async fn save_config(&self, json: &str) -> Result<(), StorageError>;

    // Health
    async fn health_check(&self) -> bool;
    async fn connection_count(&self) -> usize;
}
