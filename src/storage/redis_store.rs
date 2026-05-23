use super::{Storage, StorageError, UserInfo, CodeInfo};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use redis::{Client, aio::ConnectionManager, AsyncCommands};

pub struct RedisStorage {
    _client: Client,
    conn: ConnectionManager,
}

impl RedisStorage {
    pub async fn new(url: &str) -> Result<Self, String> {
        let client = Client::open(url).map_err(|e| format!("redis connect: {e}"))?;
        let conn = ConnectionManager::new(client.clone())
            .await
            .map_err(|e| format!("redis connection: {e}"))?;
        Ok(Self { _client: client, conn })
    }

    fn now_ts() -> f64 {
        Utc::now().timestamp() as f64
    }

    fn today_start_ts() -> f64 {
        let now = Utc::now();
        now.date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp() as f64
    }
}

#[async_trait]
impl Storage for RedisStorage {
    async fn upsert_user(&self, openid: &str, subscribe: bool) -> Result<(), StorageError> {
        let mut conn = self.conn.clone();
        let key = format!("user:{}", openid);
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        let exists: bool = conn.exists(&key).await.map_err(|e| StorageError::Database(e.to_string()))?;

        let created_at = if exists {
            let val: String = conn.hget(&key, "created_at").await.unwrap_or_else(|_| now_str.clone());
            val
        } else {
            now_str.clone()
        };

        let openid_s = openid.to_string();
        let nickname_s = String::new();
        let headimgurl_s = String::new();
        let subscribe_s = if subscribe { "1" } else { "0" }.to_string();
        let _: () = conn
            .hset_multiple(
                &key,
                &[
                    ("openid", &openid_s),
                    ("nickname", &nickname_s),
                    ("headimgurl", &headimgurl_s),
                    ("subscribe", &subscribe_s),
                    ("created_at", &created_at),
                    ("updated_at", &now_str),
                ],
            )
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        if subscribe {
            let ts = now.timestamp() as f64;
            let _: () = conn.zadd("users:subscribed", openid, ts).await.map_err(|e| StorageError::Database(e.to_string()))?;
        } else {
            let _: () = conn.zrem("users:subscribed", openid).await.map_err(|e| StorageError::Database(e.to_string()))?;
        }

        let _: () = conn.zadd("users:all", openid, now.timestamp() as f64).await.map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(())
    }

    async fn list_users(&self, page: i64, size: i64) -> Result<Vec<UserInfo>, StorageError> {
        let mut conn = self.conn.clone();
        let offset = (page - 1).max(0) * size;
        let start = offset as isize;
        let stop = (offset + size - 1) as isize;

        let openids: Vec<String> = conn
            .zrevrange("users:subscribed", start, stop)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let mut users = Vec::new();
        for oid in openids {
            let key = format!("user:{}", oid);
            let fields: std::collections::HashMap<String, String> = conn
                .hgetall(&key)
                .await
                .unwrap_or_default();
            if fields.is_empty() {
                continue;
            }
            users.push(UserInfo {
                openid: fields.get("openid").cloned().unwrap_or(oid),
                nickname: fields.get("nickname").cloned().unwrap_or_default(),
                headimgurl: fields.get("headimgurl").cloned().unwrap_or_default(),
                subscribe: fields.get("subscribe").map(|v| v == "1").unwrap_or(true),
                created_at: fields.get("created_at").and_then(|s| DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))),
                updated_at: fields.get("updated_at").and_then(|s| DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))),
            });
        }
        Ok(users)
    }

    async fn count_subscribers(&self) -> Result<i64, StorageError> {
        let mut conn = self.conn.clone();
        let count: i64 = conn.zcard("users:subscribed").await.map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(count)
    }

    async fn count_total_users(&self) -> Result<i64, StorageError> {
        let mut conn = self.conn.clone();
        let count: i64 = conn.zcard("users:all").await.map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(count)
    }

    async fn count_today_new_users(&self) -> Result<i64, StorageError> {
        let mut conn = self.conn.clone();
        let today = Self::today_start_ts();
        let count: i64 = conn.zcount("users:all", today, "+inf").await.map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(count)
    }

    async fn search_users(&self, query: &str) -> Result<Vec<UserInfo>, StorageError> {
        let mut conn = self.conn.clone();
        let query_lower = query.to_lowercase();
        let all_openids: Vec<String> = conn.zrange("users:all", 0, -1).await.unwrap_or_default();
        let mut results = Vec::new();
        for oid in all_openids {
            if oid.to_lowercase().contains(&query_lower) {
                let key = format!("user:{}", oid);
                let fields: std::collections::HashMap<String, String> = conn.hgetall(&key).await.unwrap_or_default();
                if !fields.is_empty() {
                    results.push(UserInfo {
                        openid: fields.get("openid").cloned().unwrap_or(oid),
                        nickname: fields.get("nickname").cloned().unwrap_or_default(),
                        headimgurl: fields.get("headimgurl").cloned().unwrap_or_default(),
                        subscribe: fields.get("subscribe").map(|v| v == "1").unwrap_or(true),
                        created_at: fields.get("created_at").and_then(|s| DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))),
                        updated_at: fields.get("updated_at").and_then(|s| DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))),
                    });
                    if results.len() >= 50 {
                        break;
                    }
                }
            }
        }
        Ok(results)
    }

    async fn insert_code(&self, openid: &str, code: &str, expires_at: DateTime<Utc>) -> Result<(), StorageError> {
        let mut conn = self.conn.clone();
        let now = Utc::now();
        let id: i64 = conn.incr("code:next_id", 1).await.map_err(|e| StorageError::Database(e.to_string()))?;
        let key = format!("code:{}", id);

        let id_s = id.to_string();
        let openid_s = openid.to_string();
        let code_s = code.to_string();
        let purpose_s = String::new();
        let used_s = "0".to_string();
        let created_s = now.to_rfc3339();
        let expires_s = expires_at.to_rfc3339();
        let _: () = conn
            .hset_multiple(
                &key,
                &[
                    ("id", &id_s),
                    ("openid", &openid_s),
                    ("code", &code_s),
                    ("purpose", &purpose_s),
                    ("used", &used_s),
                    ("created_at", &created_s),
                    ("expires_at", &expires_s),
                ],
            )
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let ttl = (expires_at - now).num_seconds().max(0);
        if ttl > 0 {
            let _: () = conn.expire(&key, ttl).await.unwrap_or_default();
        }

        let _: () = conn.zadd("codes:all", id, now.timestamp() as f64).await.map_err(|e| StorageError::Database(e.to_string()))?;
        let _: () = conn.zadd(format!("codes:user:{}", openid), id, now.timestamp() as f64).await.map_err(|e| StorageError::Database(e.to_string()))?;

        Ok(())
    }

    async fn list_codes(&self, page: i64, size: i64) -> Result<Vec<CodeInfo>, StorageError> {
        let mut conn = self.conn.clone();
        let offset = (page - 1).max(0) * size;
        let start = offset as isize;
        let stop = (offset + size - 1) as isize;

        let ids: Vec<i64> = conn
            .zrevrange("codes:all", start, stop)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let mut codes = Vec::new();
        for id in ids {
            let key = format!("code:{}", id);
            let fields: std::collections::HashMap<String, String> = conn.hgetall(&key).await.unwrap_or_default();
            if fields.is_empty() {
                continue;
            }
            codes.push(CodeInfo {
                id: id as i32,
                openid: fields.get("openid").cloned().unwrap_or_default(),
                code: fields.get("code").cloned().unwrap_or_default(),
                purpose: fields.get("purpose").cloned(),
                used: fields.get("used").map(|v| v == "1").unwrap_or(false),
                created_at: fields.get("created_at").and_then(|s| DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))),
                expires_at: fields.get("expires_at").and_then(|s| DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))),
            });
        }
        Ok(codes)
    }

    async fn count_codes(&self) -> Result<i64, StorageError> {
        let mut conn = self.conn.clone();
        let count: i64 = conn.zcard("codes:all").await.map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(count)
    }

    async fn count_today_codes(&self) -> Result<i64, StorageError> {
        let mut conn = self.conn.clone();
        let today = Self::today_start_ts();
        let count: i64 = conn.zcount("codes:all", today, "+inf").await.map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(count)
    }

    async fn count_used_codes(&self) -> Result<i64, StorageError> {
        let mut conn = self.conn.clone();
        let count: i64 = conn.zcard("codes:used").await.unwrap_or(0);
        Ok(count)
    }

    async fn count_expired_codes(&self) -> Result<i64, StorageError> {
        let mut conn = self.conn.clone();
        let now = Self::now_ts();
        let total: i64 = conn.zcard("codes:all").await.unwrap_or(0);
        let active: i64 = conn.zcount("codes:all", now, "+inf").await.unwrap_or(0);
        let used: i64 = conn.zcard("codes:used").await.unwrap_or(0);
        Ok((total - active - used).max(0))
    }

    async fn get_user_codes(&self, openid: &str) -> Result<Vec<CodeInfo>, StorageError> {
        let mut conn = self.conn.clone();
        let ids: Vec<i64> = conn
            .zrevrange(format!("codes:user:{}", openid), 0, 49)
            .await
            .unwrap_or_default();

        let mut codes = Vec::new();
        for id in ids {
            let key = format!("code:{}", id);
            let fields: std::collections::HashMap<String, String> = conn.hgetall(&key).await.unwrap_or_default();
            if fields.is_empty() {
                continue;
            }
            codes.push(CodeInfo {
                id: id as i32,
                openid: fields.get("openid").cloned().unwrap_or_default(),
                code: fields.get("code").cloned().unwrap_or_default(),
                purpose: fields.get("purpose").cloned(),
                used: fields.get("used").map(|v| v == "1").unwrap_or(false),
                created_at: fields.get("created_at").and_then(|s| DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))),
                expires_at: fields.get("expires_at").and_then(|s| DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))),
            });
        }
        Ok(codes)
    }

    async fn validate_code(&self, code: &str) -> Result<Option<(String, bool, DateTime<Utc>)>, StorageError> {
        let mut conn = self.conn.clone();
        let all_ids: Vec<i64> = conn.zrange("codes:all", 0, -1).await.unwrap_or_default();
        for id in all_ids.iter().rev() {
            let key = format!("code:{}", id);
            let stored_code: String = conn.hget(&key, "code").await.unwrap_or_default();
            if stored_code == code {
                let openid: String = conn.hget(&key, "openid").await.unwrap_or_default();
                let used_str: String = conn.hget(&key, "used").await.unwrap_or_default();
                let expires_str: String = conn.hget(&key, "expires_at").await.unwrap_or_default();
                let used = used_str == "1";
                let expires_at = DateTime::parse_from_rfc3339(&expires_str)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                return Ok(Some((openid, used, expires_at)));
            }
        }
        Ok(None)
    }

    async fn load_config(&self) -> Result<Option<String>, StorageError> {
        let mut conn = self.conn.clone();
        let val: Option<String> = conn.get("app:config").await.map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(val)
    }

    async fn save_config(&self, json: &str) -> Result<(), StorageError> {
        let mut conn = self.conn.clone();
        let _: () = conn.set("app:config", json).await.map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    async fn health_check(&self) -> bool {
        let mut conn = self.conn.clone();
        let result: Result<String, _> = redis::cmd("PING").query_async(&mut conn).await;
        result.is_ok()
    }

    async fn connection_count(&self) -> usize {
        1
    }
}
