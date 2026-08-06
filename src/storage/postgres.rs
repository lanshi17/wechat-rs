use super::{CallbackClaim, CodeInfo, Storage, StorageError, UserInfo};
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
        let page = page.clamp(1, 1_000_000);
        let size = size.clamp(1, 100);
        let offset = (page - 1) * size;
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
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        // Serialize issuers for the same code without requiring a new database index.
        // The second statement receives a fresh READ COMMITTED snapshot after the lock.
        sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *tx)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0::bigint))")
            .bind(code)
            .execute(&mut *tx)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let now = Utc::now();
        let (reserved_exists,): (bool,) = sqlx::query_as(
            r#"SELECT EXISTS (
                   SELECT 1
                   FROM verification_codes
                   WHERE code = $1
                     AND expires_at >= $2
               )"#,
        )
        .bind(code)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;

        if reserved_exists {
            tx.rollback()
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;
            return Err(StorageError::Conflict(
                "verification code is reserved until expiry".into(),
            ));
        }

        sqlx::query(
            "INSERT INTO verification_codes (openid, code, created_at, expires_at) VALUES ($1, $2, $3, $4)",
        )
        .bind(openid)
        .bind(code)
        .bind(now)
        .bind(expires_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    async fn list_codes(&self, page: i64, size: i64) -> Result<Vec<CodeInfo>, StorageError> {
        let page = page.clamp(1, 1_000_000);
        let size = size.clamp(1, 100);
        let offset = (page - 1) * size;
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

    async fn consume_code(
        &self,
        code: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<String>, StorageError> {
        if code.is_empty() {
            return Ok(None);
        }

        let row: Option<(String,)> = sqlx::query_as(
            r#"WITH candidate AS MATERIALIZED (
                   SELECT id
                   FROM verification_codes
                   WHERE code = $1
                   ORDER BY created_at DESC, id DESC
                   LIMIT 1
                   FOR UPDATE
               )
               UPDATE verification_codes AS codes
               SET used = true
               FROM candidate
               WHERE codes.id = candidate.id
                 AND codes.used = false
                 AND codes.expires_at >= $2
               RETURNING codes.openid"#,
        )
        .bind(code)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(row.map(|(openid,)| openid))
    }

    async fn begin_callback(
        &self,
        key: &str,
        lease_id: &str,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<CallbackClaim, StorageError> {
        if key.is_empty() || lease_id.is_empty() || lease_expires_at <= Utc::now() {
            return Ok(CallbackClaim::InProgress);
        }

        // Keep stale rows bounded; completed replies are kept until their own expiry.
        sqlx::query("DELETE FROM wechat_callback_claims WHERE expires_at <= NOW()")
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let completed: Option<(bool, Option<String>)> = sqlx::query_as(
            r#"SELECT completed, reply FROM wechat_callback_claims WHERE callback_key = $1"#,
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        if let Some((true, reply)) = completed {
            return Ok(CallbackClaim::Completed(reply));
        }

        let updated: Option<(String,)> = sqlx::query_as(
            r#"UPDATE wechat_callback_claims
               SET lease_id = $2, lease_expires_at = $3
               WHERE callback_key = $1
                 AND completed = false
                 AND (lease_expires_at IS NULL OR lease_expires_at <= NOW() OR lease_id = $2)
               RETURNING callback_key"#,
        )
        .bind(key)
        .bind(lease_id)
        .bind(lease_expires_at)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        if updated.is_some() {
            return Ok(CallbackClaim::Acquired);
        }

        let inserted: Option<(String,)> = sqlx::query_as(
            r#"INSERT INTO wechat_callback_claims
                   (callback_key, lease_id, lease_expires_at, expires_at)
               VALUES ($1, $2, $3, $3)
               ON CONFLICT (callback_key) DO NOTHING
               RETURNING callback_key"#,
        )
        .bind(key)
        .bind(lease_id)
        .bind(lease_expires_at)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        if inserted.is_some() {
            return Ok(CallbackClaim::Acquired);
        }

        let completed: Option<(bool, Option<String>)> = sqlx::query_as(
            r#"SELECT completed, reply FROM wechat_callback_claims WHERE callback_key = $1"#,
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        match completed {
            Some((true, reply)) => Ok(CallbackClaim::Completed(reply)),
            _ => Ok(CallbackClaim::InProgress),
        }
    }

    async fn complete_callback(
        &self,
        key: &str,
        lease_id: &str,
        reply: Option<&str>,
        expires_at: DateTime<Utc>,
    ) -> Result<(), StorageError> {
        let updated: Option<(String,)> = sqlx::query_as(
            r#"UPDATE wechat_callback_claims
               SET completed = true, reply = $3, expires_at = $4, lease_expires_at = NULL
               WHERE callback_key = $1
                 AND lease_id = $2
                 AND completed = false
               RETURNING callback_key"#,
        )
        .bind(key)
        .bind(lease_id)
        .bind(reply)
        .bind(expires_at)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Database(e.to_string()))?;
        if updated.is_none() {
            return Err(StorageError::Conflict(
                "callback lease lost before completion".into(),
            ));
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::sync::Arc;

    async fn test_storage() -> PgStorage {
        let url = std::env::var("WECHAT_RS_TEST_DATABASE_URL")
            .expect("set WECHAT_RS_TEST_DATABASE_URL to run ignored PostgreSQL storage tests");
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(&url)
            .await
            .expect("connect to PostgreSQL test database");
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS verification_codes (
                   id SERIAL PRIMARY KEY,
                   openid TEXT NOT NULL,
                   code TEXT NOT NULL,
                   purpose TEXT DEFAULT '',
                   created_at TIMESTAMPTZ NOT NULL,
                   expires_at TIMESTAMPTZ NOT NULL,
                   used BOOLEAN NOT NULL DEFAULT FALSE
               )"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("TRUNCATE verification_codes RESTART IDENTITY")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS wechat_callback_claims (
                   callback_key TEXT PRIMARY KEY,
                   lease_id TEXT NOT NULL DEFAULT '',
                   lease_expires_at TIMESTAMPTZ,
                   expires_at TIMESTAMPTZ NOT NULL,
                   completed BOOLEAN NOT NULL DEFAULT FALSE,
                   reply TEXT,
                   created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
               )"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS wechat_callback_claims_expiry
               ON wechat_callback_claims (expires_at)"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("TRUNCATE wechat_callback_claims")
            .execute(&pool)
            .await
            .unwrap();
        PgStorage::new(pool)
    }

    #[tokio::test]
    #[ignore = "requires a disposable WECHAT_RS_TEST_DATABASE_URL; truncates wechat_callback_claims"]
    async fn callback_begin_has_exactly_one_concurrent_winner() {
        let storage = Arc::new(test_storage().await);
        let lease_expires_at = Utc::now() + chrono::Duration::minutes(1);

        let mut tasks = Vec::new();
        for index in 0..8 {
            let storage = storage.clone();
            tasks.push(tokio::spawn(async move {
                let lease_id = format!("lease-{index}");
                storage
                    .begin_callback("same-callback", &lease_id, lease_expires_at)
                    .await
                    .unwrap()
            }));
        }

        let mut winners = 0;
        for task in tasks {
            match task.await.unwrap() {
                CallbackClaim::Acquired => winners += 1,
                CallbackClaim::InProgress => {}
                claim => panic!("unexpected concurrent claim: {claim:?}"),
            }
        }
        assert_eq!(winners, 1);
    }

    #[tokio::test]
    #[ignore = "requires a disposable WECHAT_RS_TEST_DATABASE_URL; truncates wechat_callback_claims"]
    async fn callback_response_is_replayed_and_abandoned_lease_can_be_reclaimed() {
        let storage = test_storage().await;
        let first_expiry = Utc::now() + chrono::Duration::milliseconds(250);

        assert_eq!(
            storage
                .begin_callback("expiring-callback", "lease-a", first_expiry)
                .await
                .unwrap(),
            CallbackClaim::Acquired
        );
        assert_eq!(
            storage
                .begin_callback(
                    "expiring-callback",
                    "lease-b",
                    Utc::now() + chrono::Duration::minutes(1),
                )
                .await
                .unwrap(),
            CallbackClaim::InProgress
        );

        tokio::time::sleep(std::time::Duration::from_millis(350)).await;

        assert_eq!(
            storage
                .begin_callback(
                    "expiring-callback",
                    "lease-b",
                    Utc::now() + chrono::Duration::minutes(1),
                )
                .await
                .unwrap(),
            CallbackClaim::Acquired
        );
        storage
            .complete_callback(
                "expiring-callback",
                "lease-b",
                Some("cached reply"),
                Utc::now() + chrono::Duration::minutes(10),
            )
            .await
            .unwrap();
        assert_eq!(
            storage
                .begin_callback(
                    "expiring-callback",
                    "lease-c",
                    Utc::now() + chrono::Duration::minutes(1),
                )
                .await
                .unwrap(),
            CallbackClaim::Completed(Some("cached reply".into()))
        );
    }

    #[tokio::test]
    #[ignore = "requires a disposable WECHAT_RS_TEST_DATABASE_URL; truncates verification_codes"]
    async fn code_consumption_is_atomic_and_duplicate_active_codes_conflict() {
        let storage = Arc::new(test_storage().await);
        let now = Utc::now();
        storage
            .insert_code("openid-a", "123456", now + chrono::Duration::minutes(1))
            .await
            .unwrap();

        let mut tasks = Vec::new();
        for _ in 0..8 {
            let storage = storage.clone();
            tasks.push(tokio::spawn(async move {
                storage.consume_code("123456", now).await.unwrap()
            }));
        }
        let mut winners = Vec::new();
        for task in tasks {
            if let Some(openid) = task.await.unwrap() {
                winners.push(openid);
            }
        }
        assert_eq!(winners, vec!["openid-a"]);
        assert_eq!(storage.consume_code("123456", now).await.unwrap(), None);
        let error = storage
            .insert_code(
                "openid-after-consume",
                "123456",
                now + chrono::Duration::minutes(1),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, StorageError::Conflict(_)));
        assert_eq!(storage.consume_code("123456", now).await.unwrap(), None);

        storage
            .insert_code("expired", "234567", now - chrono::Duration::seconds(1))
            .await
            .unwrap();
        assert_eq!(storage.consume_code("234567", now).await.unwrap(), None);

        storage
            .insert_code("first", "345678", now + chrono::Duration::minutes(1))
            .await
            .unwrap();
        let error = storage
            .insert_code("second", "345678", now + chrono::Duration::minutes(1))
            .await
            .unwrap_err();
        assert!(matches!(error, StorageError::Conflict(_)));
        assert_eq!(
            storage.consume_code("345678", now).await.unwrap(),
            Some("first".to_string())
        );

        let mut issuer_tasks = Vec::new();
        for index in 0..8 {
            let storage = storage.clone();
            issuer_tasks.push(tokio::spawn(async move {
                let openid = format!("concurrent-{index}");
                let result = storage
                    .insert_code(&openid, "456789", now + chrono::Duration::minutes(1))
                    .await;
                (openid, result)
            }));
        }

        let mut issued_to = Vec::new();
        for task in issuer_tasks {
            let (openid, result) = task.await.unwrap();
            match result {
                Ok(()) => issued_to.push(openid),
                Err(StorageError::Conflict(_)) => {}
                Err(error) => panic!("unexpected concurrent insert error: {error}"),
            }
        }
        assert_eq!(issued_to.len(), 1);
        assert_eq!(
            storage.consume_code("456789", now).await.unwrap(),
            issued_to.pop()
        );

        sqlx::query(
            r#"INSERT INTO verification_codes (openid, code, created_at, expires_at, used)
               VALUES ($1, $2, $3, $4, false), ($5, $2, $6, $4, false)"#,
        )
        .bind("legacy-older")
        .bind("567890")
        .bind(now - chrono::Duration::seconds(1))
        .bind(now + chrono::Duration::minutes(1))
        .bind("legacy-newer")
        .bind(now)
        .execute(&storage.pool)
        .await
        .unwrap();
        assert_eq!(
            storage.consume_code("567890", now).await.unwrap(),
            Some("legacy-newer".to_string())
        );
        assert_eq!(storage.consume_code("567890", now).await.unwrap(), None);
    }
}
