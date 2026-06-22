//! 短信告警通道
//!
//! 通过通用 HTTP API 发送短信，可对接阿里云短信、腾讯云短信、Twilio 等主流服务。
//! 用户在配置中指定 API URL、认证信息和请求体模板。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{AlertMessage, NotifyError, Notifier};

// ── 短信配置 ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SmsConfig {
    /// 是否启用短信通知
    pub enabled: bool,
    /// 短信 API 地址（POST）
    pub api_url: String,
    /// 请求方式: "json" (application/json) 或 "form" (application/x-www-form-urlencoded)
    pub content_type: String,
    /// API Key / AppID（放入 header 或 query，取决于 provider_headers）
    pub api_key: String,
    /// API Secret
    pub api_secret: String,
    /// 接收短信的手机号列表
    pub phone_numbers: Vec<String>,
    /// 签名名称（阿里云/腾讯云需要）
    pub sign_name: String,
    /// 模板 ID/Code
    pub template_code: String,
    /// 额外的 HTTP 头（Key: Value），可用于自定义认证
    pub extra_headers: HashMap<String, String>,
    /// 模板变量映射：key 是模板中的变量名，value 固定为以下占位符之一：
    ///   "${level}"  - 告警级别
    ///   "${title}"  - 告警标题
    ///   "${content}" - 告警内容
    ///   "${time}"   - 时间
    /// 若留空则默认发送 content 作为单条短信内容
    pub template_params: HashMap<String, String>,
}

impl Default for SmsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_url: String::new(),
            content_type: "json".into(),
            api_key: String::new(),
            api_secret: String::new(),
            phone_numbers: Vec::new(),
            sign_name: String::new(),
            template_code: String::new(),
            extra_headers: HashMap::new(),
            template_params: HashMap::new(),
        }
    }
}

// ── 短信发送器 ────────────────────────────────────────────────────────────────

pub struct SmsNotifier {
    config: SmsConfig,
    client: reqwest::Client,
}

impl SmsNotifier {
    pub fn new(config: SmsConfig) -> Result<Self, NotifyError> {
        if config.api_url.is_empty() {
            return Err(NotifyError::Config("sms api_url is required".into()));
        }
        if config.phone_numbers.is_empty() {
            return Err(NotifyError::Config("at least one phone_number is required".into()));
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| NotifyError::Config(format!("build http client: {e}")))?;

        Ok(Self { config, client })
    }

    /// 将模板变量中的占位符替换为实际值
    fn render_template_params(&self, msg: &AlertMessage) -> HashMap<String, String> {
        let time_str = msg.timestamp.format("%m-%d %H:%M").to_string();
        let mut result = HashMap::new();
        for (k, v) in &self.config.template_params {
            let rendered = v
                .replace("${level}", msg.level.as_str())
                .replace("${title}", &msg.title)
                .replace("${content}", &msg.content)
                .replace("${time}", &time_str);
            result.insert(k.clone(), rendered);
        }
        // 若没有配置模板参数，默认把 content 放在 "content" 字段
        if result.is_empty() {
            let text = format!(
                "[{}] {}\n{}",
                msg.level.as_str(),
                msg.title,
                msg.content
            );
            result.insert("content".into(), text);
        }
        result
    }

    async fn send_to_phone(&self, phone: &str, msg: &AlertMessage) -> Result<(), NotifyError> {
        let template_params = self.render_template_params(msg);
        let mut req_builder = match self.config.content_type.as_str() {
            "form" => {
                let mut form = HashMap::new();
                form.insert("phone", phone.to_string());
                if !self.config.sign_name.is_empty() {
                    form.insert("sign_name", self.config.sign_name.clone());
                }
                if !self.config.template_code.is_empty() {
                    form.insert("template_code", self.config.template_code.clone());
                }
                if !self.config.api_key.is_empty() {
                    form.insert("api_key", self.config.api_key.clone());
                }
                for (k, v) in &template_params {
                    form.insert(k.as_str(), v.clone());
                }
                self.client.post(&self.config.api_url).form(&form)
            }
            _ => {
                // JSON（默认）
                let mut body = serde_json::json!({
                    "phone": phone,
                });
                if !self.config.sign_name.is_empty() {
                    body["sign_name"] = serde_json::json!(self.config.sign_name);
                }
                if !self.config.template_code.is_empty() {
                    body["template_code"] = serde_json::json!(self.config.template_code);
                }
                if !self.config.api_key.is_empty() {
                    body["api_key"] = serde_json::json!(self.config.api_key);
                }
                if !self.config.api_secret.is_empty() {
                    body["api_secret"] = serde_json::json!(self.config.api_secret);
                }
                body["params"] = serde_json::to_value(&template_params)
                    .unwrap_or(serde_json::json!({}));
                self.client
                    .post(&self.config.api_url)
                    .json(&body)
            }
        };

        // 添加认证头
        if !self.config.api_key.is_empty() {
            req_builder = req_builder.header("X-API-Key", &self.config.api_key);
        }
        if !self.config.api_secret.is_empty() {
            req_builder = req_builder.basic_auth(&self.config.api_key, Some(&self.config.api_secret));
        }
        for (k, v) in &self.config.extra_headers {
            req_builder = req_builder.header(k, v);
        }

        let resp = req_builder
            .send()
            .await
            .map_err(|e| NotifyError::Network(format!("sms request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Err(NotifyError::RateLimited(format!("sms rate limited: {body}")));
            }
            return Err(NotifyError::Network(format!(
                "sms api returned {status}: {body}"
            )));
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl Notifier for SmsNotifier {
    fn name(&self) -> &str {
        "sms"
    }

    async fn send(&self, msg: &AlertMessage) -> Result<(), NotifyError> {
        // Critical/Error 级别群发给所有手机号；Warn 只发给第一个（如有）
        let phones: Vec<&str> = match msg.level {
            super::AlertLevel::Critical | super::AlertLevel::Error => {
                self.config.phone_numbers.iter().map(|s| s.as_str()).collect()
            }
            _ => self
                .config
                .phone_numbers
                .first()
                .map(|s| vec![s.as_str()])
                .unwrap_or_default(),
        };

        let mut errors = Vec::new();
        for phone in phones {
            if let Err(e) = self.send_to_phone(phone, msg).await {
                errors.push(format!("{phone}: {e}"));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(NotifyError::Other(errors.join("; ")))
        }
    }
}
