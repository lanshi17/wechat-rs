//! 上游 API 路由：受令牌保护的验证码单次核销接口

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use std::sync::Arc;
use tracing::info;

use crate::crypto::constant_time_eq;
use crate::AppState;

/// 校验上游服务令牌。未配置令牌时始终拒绝，避免空字符串意外放行。
pub(crate) fn server_token_matches(configured: &str, authorization: Option<&str>) -> bool {
    let expected = configured.trim();
    if expected.is_empty() {
        return false;
    }

    let Some(header) = authorization else {
        return false;
    };
    let presented = header
        .split_once(' ')
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
        .map(|(_, token)| token)
        .unwrap_or(header);

    constant_time_eq(expected.as_bytes(), presented.as_bytes())
}

fn valid_verification_code(code: &str) -> bool {
    code.len() == 6 && code.bytes().all(|byte| byte.is_ascii_digit())
}

/// GET /api/wechat/user?code=xxx — 上游网站验证接口
pub async fn wechat_user(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let code = params.get("code").cloned().unwrap_or_default();

    let auth_token = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .filter(|value| !value.is_empty());
    if state.wechat_server_token.trim().is_empty() {
        tracing::error!("wechat_user: upstream server token is not configured");
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "success": false,
                "message": "server token is not configured",
                "data": ""
            })),
        )
            .into_response();
    }
    if !server_token_matches(&state.wechat_server_token, auth_token) {
        info!("wechat_user: unauthorized request rejected");
        return (
            StatusCode::UNAUTHORIZED,
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
    if !valid_verification_code(&code) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "message": "验证码格式错误",
                "data": ""
            })),
        )
            .into_response();
    }

    match state.db.consume_code(&code, Utc::now()).await {
        Ok(Some(openid)) => {
            info!("wechat_user: verification code consumed");
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
        Ok(None) => {
            info!("wechat_user: invalid, expired, or already consumed code");
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": false,
                    "message": "验证码错误、已过期或已使用",
                    "data": ""
                })),
            )
                .into_response()
        }
        Err(error) => {
            tracing::error!("wechat_user: storage error: {error}");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "success": false,
                    "message": "verification service temporarily unavailable",
                    "data": ""
                })),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{server_token_matches, wechat_user};
    use crate::{
        monitor::{Monitor, MonitorConfig},
        notify::NotifyDispatcher,
        storage::{CallbackClaim, CodeInfo, Storage, StorageError, UserInfo},
        AppConfig, AppState,
    };
    use async_trait::async_trait;
    use axum::{
        body::to_bytes,
        extract::{Query, State},
        http::{header::AUTHORIZATION, HeaderMap, HeaderValue, StatusCode},
        response::IntoResponse,
    };
    use chrono::{DateTime, Utc};
    use std::{
        collections::HashMap,
        path::PathBuf,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Arc, Mutex,
        },
        time::Instant,
    };

    struct TestStorage {
        code: Mutex<Option<(String, String)>>,
        fail_consumption: AtomicBool,
        consume_calls: AtomicUsize,
    }

    impl TestStorage {
        fn with_code(code: &str, openid: &str) -> Arc<Self> {
            Arc::new(Self {
                code: Mutex::new(Some((code.to_string(), openid.to_string()))),
                fail_consumption: AtomicBool::new(false),
                consume_calls: AtomicUsize::new(0),
            })
        }

        fn failing() -> Arc<Self> {
            Arc::new(Self {
                code: Mutex::new(None),
                fail_consumption: AtomicBool::new(true),
                consume_calls: AtomicUsize::new(0),
            })
        }
    }

    #[async_trait]
    impl Storage for TestStorage {
        async fn upsert_user(&self, _: &str, _: bool) -> Result<(), StorageError> {
            Ok(())
        }

        async fn list_users(&self, _: i64, _: i64) -> Result<Vec<UserInfo>, StorageError> {
            Ok(Vec::new())
        }

        async fn count_subscribers(&self) -> Result<i64, StorageError> {
            Ok(0)
        }

        async fn count_total_users(&self) -> Result<i64, StorageError> {
            Ok(0)
        }

        async fn count_today_new_users(&self) -> Result<i64, StorageError> {
            Ok(0)
        }

        async fn search_users(&self, _: &str) -> Result<Vec<UserInfo>, StorageError> {
            Ok(Vec::new())
        }

        async fn insert_code(
            &self,
            _: &str,
            _: &str,
            _: DateTime<Utc>,
        ) -> Result<(), StorageError> {
            Ok(())
        }

        async fn list_codes(&self, _: i64, _: i64) -> Result<Vec<CodeInfo>, StorageError> {
            Ok(Vec::new())
        }

        async fn count_codes(&self) -> Result<i64, StorageError> {
            Ok(0)
        }

        async fn count_today_codes(&self) -> Result<i64, StorageError> {
            Ok(0)
        }

        async fn count_used_codes(&self) -> Result<i64, StorageError> {
            Ok(0)
        }

        async fn count_expired_codes(&self) -> Result<i64, StorageError> {
            Ok(0)
        }

        async fn get_user_codes(&self, _: &str) -> Result<Vec<CodeInfo>, StorageError> {
            Ok(Vec::new())
        }

        async fn consume_code(
            &self,
            code: &str,
            _: DateTime<Utc>,
        ) -> Result<Option<String>, StorageError> {
            self.consume_calls.fetch_add(1, Ordering::SeqCst);
            if self.fail_consumption.load(Ordering::SeqCst) {
                return Err(StorageError::Database("offline".into()));
            }

            let mut stored = self.code.lock().expect("test code mutex poisoned");
            if stored.as_ref().is_some_and(|(value, _)| value == code) {
                return Ok(stored.take().map(|(_, openid)| openid));
            }
            Ok(None)
        }

        async fn begin_callback(
            &self,
            _: &str,
            _: &str,
            _: DateTime<Utc>,
        ) -> Result<CallbackClaim, StorageError> {
            Ok(CallbackClaim::Acquired)
        }

        async fn complete_callback(
            &self,
            _: &str,
            _: &str,
            _: Option<&str>,
            _: DateTime<Utc>,
        ) -> Result<(), StorageError> {
            Ok(())
        }

        async fn load_config(&self) -> Result<Option<String>, StorageError> {
            Ok(None)
        }

        async fn save_config(&self, _: &str) -> Result<(), StorageError> {
            Ok(())
        }

        async fn health_check(&self) -> bool {
            true
        }

        async fn connection_count(&self) -> usize {
            1
        }
    }

    fn state(storage: Arc<TestStorage>, server_token: &str) -> Arc<AppState> {
        let monitor = Monitor::new(
            MonitorConfig {
                enabled: false,
                ..MonitorConfig::default()
            },
            NotifyDispatcher::empty(),
        );
        Arc::new(AppState {
            db: storage,
            config: Arc::new(tokio::sync::RwLock::new(AppConfig {
                wechat_token: String::new(),
                wechat_appid: String::new(),
                wechat_appsecret: String::new(),
                wechat_encoding_aes_key: String::new(),
                admin_password_hash: String::new(),
                welcome_message: String::new(),
                site_name: String::new(),
                domain: String::new(),
            })),
            admin_secret: "test-admin-secret".into(),
            admin_password: "test-admin-password".into(),
            wechat_server_token: server_token.into(),
            config_path: PathBuf::from("test-config.toml"),
            started_at: Instant::now(),
            monitor,
        })
    }

    fn request_headers(token: Option<&'static str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(token) = token {
            headers.insert(AUTHORIZATION, HeaderValue::from_static(token));
        }
        headers
    }

    fn params(code: &str) -> HashMap<String, String> {
        HashMap::from([("code".into(), code.into())])
    }

    async fn response_json(response: axum::response::Response) -> serde_json::Value {
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        serde_json::from_slice(&body).expect("JSON response")
    }

    #[test]
    fn empty_server_token_always_fails_closed() {
        assert!(!server_token_matches("", None));
        assert!(!server_token_matches("   ", Some("Bearer ")));
    }

    #[test]
    fn server_token_accepts_bearer_or_legacy_raw_header() {
        assert!(server_token_matches("secret", Some("Bearer secret")));
        assert!(server_token_matches("secret", Some("secret")));
    }

    #[test]
    fn server_token_rejects_missing_or_incorrect_credentials() {
        assert!(!server_token_matches("secret", None));
        assert!(!server_token_matches("secret", Some("Bearer wrong")));
        assert!(!server_token_matches("secret", Some("Bearer secret ")));
    }

    #[tokio::test]
    async fn empty_server_token_returns_service_unavailable_without_consuming() {
        let storage = TestStorage::with_code("123456", "openid-a");
        let response = wechat_user(
            State(state(storage.clone(), "")),
            request_headers(None),
            Query(params("123456")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(storage.consume_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn unauthorized_request_does_not_consume_code() {
        let storage = TestStorage::with_code("123456", "openid-a");
        let response = wechat_user(
            State(state(storage.clone(), "secret")),
            request_headers(Some("Bearer wrong")),
            Query(params("123456")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(storage.consume_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn malformed_code_is_rejected_without_touching_storage() {
        let storage = TestStorage::with_code("123456", "openid-a");
        let response = wechat_user(
            State(state(storage.clone(), "secret")),
            request_headers(Some("Bearer secret")),
            Query(params("123456 OR 1=1")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(storage.consume_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn verification_code_can_be_consumed_exactly_once() {
        let storage = TestStorage::with_code("123456", "openid-a");
        let app_state = state(storage.clone(), "secret");

        let first = wechat_user(
            State(app_state.clone()),
            request_headers(Some("Bearer secret")),
            Query(params("123456")),
        )
        .await
        .into_response();
        assert_eq!(first.status(), StatusCode::OK);
        let first_json = response_json(first).await;
        assert_eq!(first_json["success"], true);
        assert_eq!(first_json["data"], "openid-a");

        let second = wechat_user(
            State(app_state),
            request_headers(Some("Bearer secret")),
            Query(params("123456")),
        )
        .await
        .into_response();
        assert_eq!(second.status(), StatusCode::OK);
        assert_eq!(response_json(second).await["success"], false);
        assert_eq!(storage.consume_calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn storage_failure_is_not_reported_as_an_invalid_code() {
        let storage = TestStorage::failing();
        let response = wechat_user(
            State(state(storage, "secret")),
            request_headers(Some("secret")),
            Query(params("123456")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response_json(response).await["success"], false);
    }
}
