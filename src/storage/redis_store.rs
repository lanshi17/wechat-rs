use super::{CallbackClaim, CodeInfo, Storage, StorageError, UserInfo};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use redis::{aio::ConnectionManager, AsyncCommands, Client};

pub struct RedisStorage {
    _client: Client,
    conn: ConnectionManager,
}

const INSERT_CODE_SCRIPT: &str = r#"
local ttl_ms = tonumber(ARGV[8])

if ttl_ms > 0 and redis.call('EXISTS', KEYS[1]) == 1 then
    return 0
end

redis.call(
    'HSET', KEYS[2],
    'id', ARGV[1],
    'openid', ARGV[2],
    'code', ARGV[3],
    'purpose', '',
    'used', '0',
    'created_at', ARGV[4],
    'expires_at', ARGV[5],
    'expires_at_ms', ARGV[7]
)

if ttl_ms > 0 then
    redis.call('SET', KEYS[1], ARGV[1], 'PX', ttl_ms)
end

redis.call('ZADD', KEYS[3], ARGV[6], ARGV[1])
redis.call('ZADD', KEYS[4], ARGV[6], ARGV[1])
redis.call('ZADD', KEYS[5], ARGV[7], ARGV[1])
return 1
"#;

const CONSUME_CODE_SCRIPT: &str = r#"
local id = redis.call('GET', KEYS[1])
if not id then
    return nil
end

local record_key = 'code:' .. id
if redis.call('EXISTS', record_key) == 0 then
    redis.call('DEL', KEYS[1])
    redis.call('ZREM', KEYS[3], id)
    return nil
end

local used = redis.call('HGET', record_key, 'used')
if not used then
    return redis.error_reply('verification code record is missing used')
end
if used ~= '0' then
    redis.call('ZREM', KEYS[3], id)
    return nil
end

local expires_at_ms = tonumber(redis.call('HGET', record_key, 'expires_at_ms'))
if not expires_at_ms then
    return redis.error_reply('verification code record is missing expires_at_ms')
end
if expires_at_ms < tonumber(ARGV[1]) then
    redis.call('DEL', KEYS[1])
    redis.call('ZREM', KEYS[3], id)
    return nil
end

local openid = redis.call('HGET', record_key, 'openid')
if not openid or openid == '' then
    return redis.error_reply('verification code record is missing openid')
end

redis.call('HSET', record_key, 'used', '1')
redis.call('ZADD', KEYS[2], ARGV[1], id)
redis.call('ZREM', KEYS[3], id)
return openid
"#;

const COUNT_EXPIRED_CODES_SCRIPT: &str = r#"
local total = redis.call('ZCARD', KEYS[1])
local active = redis.call('ZCOUNT', KEYS[2], ARGV[1], '+inf')
local used = redis.call('ZCARD', KEYS[3])
return math.max(total - active - used, 0)
"#;

const BEGIN_CALLBACK_SCRIPT: &str = r#"
local state_key = KEYS[1]
local now = tonumber(ARGV[1])
local lease_id = ARGV[2]
local lease_expires_ms = tonumber(ARGV[3])

local completed = redis.call('HGET', state_key, 'completed')
if completed == '1' then
    local reply = redis.call('HGET', state_key, 'reply') or ''
    return {'2', reply}
end

local current_lease = redis.call('HGET', state_key, 'lease_id') or ''
local current_exp = tonumber(redis.call('HGET', state_key, 'lease_expires_at_ms') or '0')
if now < current_exp and current_lease ~= lease_id then
    return {'1', ''}
end

redis.call(
    'HSET', state_key,
    'lease_id', lease_id,
    'lease_expires_at_ms', lease_expires_ms,
    'completed', '0',
    'reply', ''
)
redis.call('PEXPIRE', state_key, math.max(lease_expires_ms - now, 1))
return {'0', ''}
"#;

const COMPLETE_CALLBACK_SCRIPT: &str = r#"
local state_key = KEYS[1]
local lease_id = ARGV[1]
local reply = ARGV[2]
local retention_expires_ms = tonumber(ARGV[3])
local now = tonumber(ARGV[4])

if (redis.call('HGET', state_key, 'lease_id') or '') ~= lease_id then
    return redis.error_reply('callback lease lost')
end

redis.call(
    'HSET', state_key,
    'lease_id', '',
    'lease_expires_at_ms', '0',
    'completed', '1',
    'reply', reply
)
redis.call('PEXPIRE', state_key, math.max(retention_expires_ms - now, 1))
return 1
"#;

impl RedisStorage {
    pub async fn new(url: &str) -> Result<Self, String> {
        let client = Client::open(url).map_err(|e| format!("redis connect: {e}"))?;
        let conn = ConnectionManager::new(client.clone())
            .await
            .map_err(|e| format!("redis connection: {e}"))?;
        Ok(Self {
            _client: client,
            conn,
        })
    }

    fn now_ms() -> f64 {
        Utc::now().timestamp_millis() as f64
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

        let exists: bool = conn
            .exists(&key)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let created_at = if exists {
            let val: Option<String> = conn
                .hget(&key, "created_at")
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;
            val.unwrap_or_else(|| now_str.clone())
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
            let _: () = conn
                .zadd("users:subscribed", openid, ts)
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;
        } else {
            let _: () = conn
                .zrem("users:subscribed", openid)
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;
        }

        if !exists {
            let _: () = conn
                .zadd("users:all", openid, now.timestamp() as f64)
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;
        }

        Ok(())
    }

    async fn list_users(&self, page: i64, size: i64) -> Result<Vec<UserInfo>, StorageError> {
        let mut conn = self.conn.clone();
        let page = page.clamp(1, 1_000_000);
        let size = size.clamp(1, 100);
        let offset = (page - 1) * size;
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
                .map_err(|e| StorageError::Database(e.to_string()))?;
            if fields.is_empty() {
                continue;
            }
            users.push(UserInfo {
                openid: fields.get("openid").cloned().unwrap_or(oid),
                nickname: fields.get("nickname").cloned().unwrap_or_default(),
                headimgurl: fields.get("headimgurl").cloned().unwrap_or_default(),
                subscribe: fields.get("subscribe").map(|v| v == "1").unwrap_or(true),
                created_at: fields.get("created_at").and_then(|s| {
                    DateTime::parse_from_rfc3339(s)
                        .ok()
                        .map(|d| d.with_timezone(&Utc))
                }),
                updated_at: fields.get("updated_at").and_then(|s| {
                    DateTime::parse_from_rfc3339(s)
                        .ok()
                        .map(|d| d.with_timezone(&Utc))
                }),
            });
        }
        Ok(users)
    }

    async fn count_subscribers(&self) -> Result<i64, StorageError> {
        let mut conn = self.conn.clone();
        let count: i64 = conn
            .zcard("users:subscribed")
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(count)
    }

    async fn count_total_users(&self) -> Result<i64, StorageError> {
        let mut conn = self.conn.clone();
        let count: i64 = conn
            .zcard("users:all")
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(count)
    }

    async fn count_today_new_users(&self) -> Result<i64, StorageError> {
        let mut conn = self.conn.clone();
        let today = Self::today_start_ts();
        let count: i64 = conn
            .zcount("users:all", today, "+inf")
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(count)
    }

    async fn search_users(&self, query: &str) -> Result<Vec<UserInfo>, StorageError> {
        let mut conn = self.conn.clone();
        let query_lower = query.to_lowercase();
        let all_openids: Vec<String> = conn
            .zrange("users:all", 0, -1)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let mut results = Vec::new();
        for oid in all_openids {
            if oid.to_lowercase().contains(&query_lower) {
                let key = format!("user:{}", oid);
                let fields: std::collections::HashMap<String, String> = conn
                    .hgetall(&key)
                    .await
                    .map_err(|e| StorageError::Database(e.to_string()))?;
                if !fields.is_empty() {
                    results.push(UserInfo {
                        openid: fields.get("openid").cloned().unwrap_or(oid),
                        nickname: fields.get("nickname").cloned().unwrap_or_default(),
                        headimgurl: fields.get("headimgurl").cloned().unwrap_or_default(),
                        subscribe: fields.get("subscribe").map(|v| v == "1").unwrap_or(true),
                        created_at: fields.get("created_at").and_then(|s| {
                            DateTime::parse_from_rfc3339(s)
                                .ok()
                                .map(|d| d.with_timezone(&Utc))
                        }),
                        updated_at: fields.get("updated_at").and_then(|s| {
                            DateTime::parse_from_rfc3339(s)
                                .ok()
                                .map(|d| d.with_timezone(&Utc))
                        }),
                    });
                    if results.len() >= 50 {
                        break;
                    }
                }
            }
        }
        Ok(results)
    }

    async fn insert_code(
        &self,
        openid: &str,
        code: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.clone();
        let now = Utc::now();
        let id: i64 = conn
            .incr("code:next_id", 1)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let record_key = format!("code:{}", id);
        let lookup_key = format!("code:lookup:{}", code);
        let user_codes_key = format!("codes:user:{}", openid);

        let id_s = id.to_string();
        let created_s = now.to_rfc3339();
        let expires_s = expires_at.to_rfc3339();
        let created_at = now.timestamp() as f64;
        let expires_at_ms = expires_at.timestamp_millis();
        let ttl_ms = (expires_at - now).num_milliseconds().max(0);

        let inserted: i64 = redis::Script::new(INSERT_CODE_SCRIPT)
            .key(&lookup_key)
            .key(&record_key)
            .key("codes:all")
            .key(&user_codes_key)
            .key("codes:expires")
            .arg(&id_s)
            .arg(openid)
            .arg(code)
            .arg(&created_s)
            .arg(&expires_s)
            .arg(created_at)
            .arg(expires_at_ms)
            .arg(ttl_ms)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        if inserted == 0 {
            return Err(StorageError::Conflict(
                "verification code is already active".into(),
            ));
        }

        Ok(())
    }

    async fn list_codes(&self, page: i64, size: i64) -> Result<Vec<CodeInfo>, StorageError> {
        let mut conn = self.conn.clone();
        let page = page.clamp(1, 1_000_000);
        let size = size.clamp(1, 100);
        let offset = (page - 1) * size;
        let start = offset as isize;
        let stop = (offset + size - 1) as isize;

        let ids: Vec<i64> = conn
            .zrevrange("codes:all", start, stop)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let mut codes = Vec::new();
        for id in ids {
            let key = format!("code:{}", id);
            let fields: std::collections::HashMap<String, String> = conn
                .hgetall(&key)
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;
            if fields.is_empty() {
                continue;
            }
            codes.push(CodeInfo {
                id: id as i32,
                openid: fields.get("openid").cloned().unwrap_or_default(),
                code: fields.get("code").cloned().unwrap_or_default(),
                purpose: fields.get("purpose").cloned(),
                used: fields.get("used").map(|v| v == "1").unwrap_or(false),
                created_at: fields.get("created_at").and_then(|s| {
                    DateTime::parse_from_rfc3339(s)
                        .ok()
                        .map(|d| d.with_timezone(&Utc))
                }),
                expires_at: fields.get("expires_at").and_then(|s| {
                    DateTime::parse_from_rfc3339(s)
                        .ok()
                        .map(|d| d.with_timezone(&Utc))
                }),
            });
        }
        Ok(codes)
    }

    async fn count_codes(&self) -> Result<i64, StorageError> {
        let mut conn = self.conn.clone();
        let count: i64 = conn
            .zcard("codes:all")
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(count)
    }

    async fn count_today_codes(&self) -> Result<i64, StorageError> {
        let mut conn = self.conn.clone();
        let today = Self::today_start_ts();
        let count: i64 = conn
            .zcount("codes:all", today, "+inf")
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(count)
    }

    async fn count_used_codes(&self) -> Result<i64, StorageError> {
        let mut conn = self.conn.clone();
        let count: i64 = conn
            .zcard("codes:used")
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(count)
    }

    async fn count_expired_codes(&self) -> Result<i64, StorageError> {
        let mut conn = self.conn.clone();
        redis::Script::new(COUNT_EXPIRED_CODES_SCRIPT)
            .key("codes:all")
            .key("codes:expires")
            .key("codes:used")
            .arg(Self::now_ms())
            .invoke_async(&mut conn)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))
    }

    async fn get_user_codes(&self, openid: &str) -> Result<Vec<CodeInfo>, StorageError> {
        let mut conn = self.conn.clone();
        let ids: Vec<i64> = conn
            .zrevrange(format!("codes:user:{}", openid), 0, 49)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let mut codes = Vec::new();
        for id in ids {
            let key = format!("code:{}", id);
            let fields: std::collections::HashMap<String, String> = conn
                .hgetall(&key)
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;
            if fields.is_empty() {
                continue;
            }
            codes.push(CodeInfo {
                id: id as i32,
                openid: fields.get("openid").cloned().unwrap_or_default(),
                code: fields.get("code").cloned().unwrap_or_default(),
                purpose: fields.get("purpose").cloned(),
                used: fields.get("used").map(|v| v == "1").unwrap_or(false),
                created_at: fields.get("created_at").and_then(|s| {
                    DateTime::parse_from_rfc3339(s)
                        .ok()
                        .map(|d| d.with_timezone(&Utc))
                }),
                expires_at: fields.get("expires_at").and_then(|s| {
                    DateTime::parse_from_rfc3339(s)
                        .ok()
                        .map(|d| d.with_timezone(&Utc))
                }),
            });
        }
        Ok(codes)
    }

    async fn consume_code(
        &self,
        code: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<String>, StorageError> {
        if code.is_empty() {
            return Ok(None);
        }

        let mut conn = self.conn.clone();
        redis::Script::new(CONSUME_CODE_SCRIPT)
            .key(format!("code:lookup:{}", code))
            .key("codes:used")
            .key("codes:expires")
            .arg(now.timestamp_millis())
            .invoke_async(&mut conn)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))
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

        let mut conn = self.conn.clone();
        let now_ms = Self::now_ms();
        let lease_ms = (lease_expires_at.timestamp_millis() as f64).max(now_ms + 1.0);
        let result: Vec<String> = redis::Script::new(BEGIN_CALLBACK_SCRIPT)
            .key(format!("callback:state:{key}"))
            .arg(now_ms)
            .arg(lease_id)
            .arg(lease_ms)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let status = result.first().map(String::as_str).unwrap_or("");
        match status {
            "0" => Ok(CallbackClaim::Acquired),
            "2" => Ok(CallbackClaim::Completed(result.get(1).cloned())),
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
        let mut conn = self.conn.clone();
        let now_ms = Self::now_ms();
        let retention_ms = (expires_at.timestamp_millis() as f64).max(now_ms + 1.0);
        let _: i64 = redis::Script::new(COMPLETE_CALLBACK_SCRIPT)
            .key(format!("callback:state:{key}"))
            .arg(lease_id)
            .arg(reply.unwrap_or(""))
            .arg(retention_ms)
            .arg(now_ms)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(())
    }

    async fn load_config(&self) -> Result<Option<String>, StorageError> {
        let mut conn = self.conn.clone();
        let val: Option<String> = conn
            .get("app:config")
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
        Ok(val)
    }

    async fn save_config(&self, json: &str) -> Result<(), StorageError> {
        let mut conn = self.conn.clone();
        let _: () = conn
            .set("app:config", json)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        net::TcpListener,
        process::{Child, Command, Stdio},
        sync::Arc,
        time::Duration as StdDuration,
    };

    struct RedisTestServer {
        child: Child,
        data_dir: std::path::PathBuf,
    }

    impl Drop for RedisTestServer {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = std::fs::remove_dir_all(&self.data_dir);
        }
    }

    async fn test_storage() -> (RedisTestServer, RedisStorage) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("allocate Redis test port");
        let port = listener.local_addr().expect("Redis test address").port();
        drop(listener);

        let data_dir = std::env::temp_dir().join(format!(
            "wechat-rs-redis-test-{}-{port}",
            std::process::id()
        ));
        std::fs::create_dir_all(&data_dir).expect("create Redis test data directory");

        let child = Command::new("redis-server")
            .args([
                "--bind",
                "127.0.0.1",
                "--protected-mode",
                "no",
                "--port",
                &port.to_string(),
                "--save",
                "",
                "--appendonly",
                "no",
                "--dir",
                data_dir.to_str().expect("UTF-8 Redis test path"),
                "--loglevel",
                "warning",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start redis-server; install it to run ignored storage tests");
        let server = RedisTestServer { child, data_dir };
        let url = format!("redis://127.0.0.1:{port}/");

        for _ in 0..40 {
            if let Ok(storage) = RedisStorage::new(&url).await {
                return (server, storage);
            }
            tokio::time::sleep(StdDuration::from_millis(25)).await;
        }
        panic!("redis-server did not become ready at {url}");
    }

    #[tokio::test]
    #[ignore = "requires the redis-server binary"]
    async fn active_code_is_consumed_once_and_marked_used() {
        let (_server, storage) = test_storage().await;
        let now = Utc::now();
        storage
            .insert_code("openid-a", "123456", now + chrono::Duration::minutes(1))
            .await
            .unwrap();
        let mut conn = storage.conn.clone();
        let lookup_ttl_ms: i64 = redis::cmd("PTTL")
            .arg("code:lookup:123456")
            .query_async(&mut conn)
            .await
            .unwrap();
        assert!(lookup_ttl_ms > 0);
        let record_id: String = conn.get("code:lookup:123456").await.unwrap();
        let record_ttl_ms: i64 = redis::cmd("PTTL")
            .arg(format!("code:{record_id}"))
            .query_async(&mut conn)
            .await
            .unwrap();
        assert_eq!(record_ttl_ms, -1, "audit records must outlive lookup TTLs");

        assert_eq!(
            storage.consume_code("123456", now).await.unwrap(),
            Some("openid-a".to_string())
        );
        assert_eq!(storage.consume_code("123456", now).await.unwrap(), None);
        let lookup_exists: bool = conn.exists("code:lookup:123456").await.unwrap();
        assert!(lookup_exists, "consumed values stay reserved until expiry");
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
        assert_eq!(storage.count_used_codes().await.unwrap(), 1);
        assert_eq!(storage.count_expired_codes().await.unwrap(), 0);

        let codes = storage.get_user_codes("openid-a").await.unwrap();
        assert_eq!(codes.len(), 1);
        assert!(codes[0].used);
    }

    #[tokio::test]
    #[ignore = "requires the redis-server binary"]
    async fn concurrent_consumers_have_exactly_one_winner() {
        let (_server, storage) = test_storage().await;
        let now = Utc::now();
        storage
            .insert_code("openid-a", "234567", now + chrono::Duration::minutes(1))
            .await
            .unwrap();
        let storage = Arc::new(storage);

        let mut tasks = Vec::new();
        for _ in 0..8 {
            let storage = storage.clone();
            tasks.push(tokio::spawn(async move {
                storage.consume_code("234567", now).await.unwrap()
            }));
        }

        let mut winners = 0;
        for task in tasks {
            if task.await.unwrap().is_some() {
                winners += 1;
            }
        }
        assert_eq!(winners, 1);
    }

    #[tokio::test]
    #[ignore = "requires the redis-server binary"]
    async fn callback_begin_has_exactly_one_concurrent_winner() {
        let (_server, storage) = test_storage().await;
        let storage = Arc::new(storage);
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
    #[ignore = "requires the redis-server binary"]
    async fn callback_response_is_replayed_and_abandoned_lease_can_be_reclaimed() {
        let (_server, storage) = test_storage().await;
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

        tokio::time::sleep(StdDuration::from_millis(350)).await;

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
    #[ignore = "requires the redis-server binary"]
    async fn updating_existing_user_preserves_original_creation_score() {
        let (_server, storage) = test_storage().await;
        storage.upsert_user("openid-a", true).await.unwrap();
        let mut conn = storage.conn.clone();
        let original: f64 = conn.zscore("users:all", "openid-a").await.unwrap();

        tokio::time::sleep(StdDuration::from_millis(1_100)).await;
        storage.upsert_user("openid-a", false).await.unwrap();
        let updated: f64 = conn.zscore("users:all", "openid-a").await.unwrap();

        assert_eq!(updated, original);
    }

    #[tokio::test]
    #[ignore = "requires the redis-server binary"]
    async fn expired_and_duplicate_active_codes_fail_closed() {
        let (_server, storage) = test_storage().await;
        let now = Utc::now();
        storage
            .insert_code("expired", "345678", now - chrono::Duration::seconds(1))
            .await
            .unwrap();
        assert_eq!(storage.consume_code("", now).await.unwrap(), None);
        assert_eq!(storage.consume_code("345678", now).await.unwrap(), None);
        assert_eq!(storage.count_expired_codes().await.unwrap(), 1);

        storage
            .insert_code("first", "456789", now + chrono::Duration::minutes(1))
            .await
            .unwrap();
        let error = storage
            .insert_code("second", "456789", now + chrono::Duration::minutes(1))
            .await
            .unwrap_err();
        assert!(matches!(error, StorageError::Conflict(_)));
        assert_eq!(
            storage.consume_code("456789", now).await.unwrap(),
            Some("first".to_string())
        );
    }
}
