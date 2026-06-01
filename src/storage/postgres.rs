use super::{CodeInfo, Storage, StorageError, UserInfo};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

pub struct PgStorage {
    pool: PgPool,
}

impl PgStorage {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    #[allow(dead_code)]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl Storage for PgStorage {
    async fn upsert_user(&self, openid: &str, subscribe: bool) -> Result<(), StorageError> {
        let now = Utc::now();
        sqlx::query(
            r#"INSERT INTO wechat_users (openid, subscribe, created_at, updated_at)
               VALUES ($1, $2, $3, $3)
               ON CONFLICT (openid) DO UPDATE
                   SET subscribe = EXCLUDED.subscribe, updated_at = EXCLUDED.updated_at"#,
        )
        .bind(openid)
        .bind(subscribe)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    async fn list_users(&self, page: i64, size: i64) -> Result<Vec<UserInfo>, StorageError> {
        let offset = (page - 1).max(0) * size;
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                bool,
                Option<DateTime<Utc>>,
                Option<DateTime<Utc>>,
            ),
        >(
            r#"SELECT openid, nickname, headimgurl, subscribe, created_at, updated_at
               FROM wechat_users WHERE subscribe = true
               ORDER BY created_at DESC LIMIT $1 OFFSET $2"#,
        )
        .bind(size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(
                |(openid, nickname, headimgurl, subscribe, created_at, updated_at)| UserInfo {
                    openid,
                    nickname,
                    headimgurl,
                    subscribe,
                    created_at,
                    updated_at,
                },
            )
            .collect())
    }

    async fn count_subscribers(&self) -> Result<i64, StorageError> {
        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM wechat_users WHERE subscribe = true")
                .fetch_one(&self.pool)
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(row.0)
    }

    async fn count_total_users(&self) -> Result<i64, StorageError> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM wechat_users")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(row.0)
    }

    async fn count_today_new_users(&self) -> Result<i64, StorageError> {
        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM wechat_users WHERE created_at >= CURRENT_DATE")
                .fetch_one(&self.pool)
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(row.0)
    }

    async fn search_users(&self, query: &str) -> Result<Vec<UserInfo>, StorageError> {
        let pattern = format!("%{}%", query);
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                bool,
                Option<DateTime<Utc>>,
                Option<DateTime<Utc>>,
            ),
        >(
            r#"SELECT openid, nickname, headimgurl, subscribe, created_at, updated_at
               FROM wechat_users WHERE openid LIKE $1
               ORDER BY created_at DESC LIMIT 50"#,
        )
        .bind(&pattern)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(
                |(openid, nickname, headimgurl, subscribe, created_at, updated_at)| UserInfo {
                    openid,
                    nickname,
                    headimgurl,
                    subscribe,
                    created_at,
                    updated_at,
                },
            )
            .collect())
    }

    async fn insert_code(
        &self,
        openid: &str,
        code: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), StorageError> {
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO verification_codes (openid, code, created_at, expires_at) VALUES ($1, $2, $3, $4)",
        )
        .bind(openid)
        .bind(code)
        .bind(now)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    async fn list_codes(&self, page: i64, size: i64) -> Result<Vec<CodeInfo>, StorageError> {
        let offset = (page - 1).max(0) * size;
        let rows = sqlx::query_as::<
            _,
            (
                i32,
                String,
                String,
                Option<String>,
                bool,
                Option<DateTime<Utc>>,
                Option<DateTime<Utc>>,
            ),
        >(
            r#"SELECT id, openid, code, purpose, used, created_at, expires_at
               FROM verification_codes
               ORDER BY created_at DESC LIMIT $1 OFFSET $2"#,
        )
        .bind(size)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(
                |(id, openid, code, purpose, used, created_at, expires_at)| CodeInfo {
                    id,
                    openid,
                    code,
                    purpose,
                    used,
                    created_at,
                    expires_at,
                },
            )
            .collect())
    }

    async fn count_codes(&self) -> Result<i64, StorageError> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM verification_codes")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(row.0)
    }

    async fn count_today_codes(&self) -> Result<i64, StorageError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM verification_codes WHERE created_at >= CURRENT_DATE",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(row.0)
    }

    async fn count_used_codes(&self) -> Result<i64, StorageError> {
        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM verification_codes WHERE used = true")
                .fetch_one(&self.pool)
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(row.0)
    }

    async fn count_expired_codes(&self) -> Result<i64, StorageError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM verification_codes WHERE expires_at < NOW() AND used = false",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(row.0)
    }

    async fn get_user_codes(&self, openid: &str) -> Result<Vec<CodeInfo>, StorageError> {
        let rows = sqlx::query_as::<
            _,
            (
                i32,
                String,
                String,
                Option<String>,
                bool,
                Option<DateTime<Utc>>,
                Option<DateTime<Utc>>,
            ),
        >(
            r#"SELECT id, openid, code, purpose, used, created_at, expires_at
               FROM verification_codes WHERE openid = $1
               ORDER BY created_at DESC LIMIT 50"#,
        )
        .bind(openid)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(
                |(id, openid, code, purpose, used, created_at, expires_at)| CodeInfo {
                    id,
                    openid,
                    code,
                    purpose,
                    used,
                    created_at,
                    expires_at,
                },
            )
            .collect())
    }

    async fn validate_code(
        &self,
        code: &str,
    ) -> Result<Option<(String, bool, DateTime<Utc>)>, StorageError> {
        let row: Option<(String, bool, DateTime<Utc>)> = sqlx::query_as(
            r#"SELECT openid, used, expires_at FROM verification_codes
               WHERE code = $1
               ORDER BY created_at DESC LIMIT 1"#,
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(row)
    }

    async fn load_config(&self) -> Result<Option<String>, StorageError> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT value FROM app_config WHERE key = 'main' LIMIT 1")
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(row.map(|(v,)| v))
    }

    async fn save_config(&self, json: &str) -> Result<(), StorageError> {
        sqlx::query(
            r#"INSERT INTO app_config (key, value) VALUES ('main', $1)
               ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value"#,
        )
        .bind(json)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    async fn health_check(&self) -> bool {
        sqlx::query("SELECT 1").execute(&self.pool).await.is_ok()
    }

    async fn connection_count(&self) -> usize {
        self.pool.size() as usize
    }
}
