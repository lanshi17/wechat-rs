//! 告警通知模块
//!
//! 支持多种通知通道：
//! - 邮件（SMTP）
//! - 短信（通过通用 HTTP API，兼容阿里云/腾讯云等主流 SMS 服务）
//! - Webhook（钉钉/飞书/Slack/企业微信等通用 HTTP 回调）

pub mod email;
pub mod sms;
pub mod webhook;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;

// ── 告警级别 ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertLevel {
    Info,
    Warn,
    Error,
    Critical,
}

impl AlertLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertLevel::Info => "INFO",
            AlertLevel::Warn => "WARN",
            AlertLevel::Error => "ERROR",
            AlertLevel::Critical => "CRITICAL",
        }
    }
}

impl fmt::Display for AlertLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── 告警消息 ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct AlertMessage {
    pub level: AlertLevel,
    pub title: String,
    pub content: String,
    pub details: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

impl AlertMessage {
    /// 格式化为纯文本（用于邮件正文、短信）
    pub fn to_plain_text(&self) -> String {
        format!(
            "[{}] {}\n时间: {}\n详情: {}\n{}",
            self.level.as_str(),
            self.title,
            self.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            self.content,
            if self.details.is_object() && !self.details.as_object().unwrap().is_empty() {
                format!("附加信息: {}", self.details)
            } else {
                String::new()
            }
        )
    }
}

// ── Notifier trait ────────────────────────────────────────────────────────────

#[async_trait::async_trait]
pub trait Notifier: Send + Sync + 'static {
    /// 通道名称（用于日志）
    fn name(&self) -> &str;
    /// 发送告警消息
    async fn send(&self, msg: &AlertMessage) -> Result<(), NotifyError>;
}

// ── 错误类型 ──────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum NotifyError {
    Config(String),
    Network(String),
    Auth(String),
    RateLimited(String),
    Other(String),
}

impl fmt::Display for NotifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotifyError::Config(s) => write!(f, "config error: {s}"),
            NotifyError::Network(s) => write!(f, "network error: {s}"),
            NotifyError::Auth(s) => write!(f, "auth error: {s}"),
            NotifyError::RateLimited(s) => write!(f, "rate limited: {s}"),
            NotifyError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for NotifyError {}

// ── 通知配置 ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotifyConfig {
    /// 启用的通道列表: "email", "sms", "webhook"
    pub channels: Vec<String>,
    /// 最低告警级别：低于此级别的消息不发送（info/warn/error/critical）
    pub min_level: AlertLevel,
    /// 邮件配置
    pub email: email::EmailConfig,
    /// 短信配置
    pub sms: sms::SmsConfig,
    /// Webhook 配置
    pub webhook: webhook::WebhookConfig,
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self {
            channels: Vec::new(),
            min_level: AlertLevel::Error,
            email: email::EmailConfig::default(),
            sms: sms::SmsConfig::default(),
            webhook: webhook::WebhookConfig::default(),
        }
    }
}

// ── 通知分发器 ────────────────────────────────────────────────────────────────

pub struct NotifyDispatcher {
    notifiers: Vec<Box<dyn Notifier>>,
    min_level: AlertLevel,
}

impl NotifyDispatcher {
    /// 创建一个空的分发器（无通道，不发送任何通知）
    pub fn empty() -> Arc<Self> {
        Arc::new(Self {
            notifiers: Vec::new(),
            min_level: AlertLevel::Critical,
        })
    }

    pub fn new(config: &NotifyConfig) -> Result<Arc<Self>, NotifyError> {
        let mut notifiers: Vec<Box<dyn Notifier>> = Vec::new();

        for ch in &config.channels {
            match ch.as_str() {
                "email" => {
                    if config.email.enabled {
                        let n = email::EmailNotifier::new(config.email.clone())?;
                        tracing::info!("notify: email channel enabled (smtp={}:{})",
                            config.email.smtp_host, config.email.smtp_port);
                        notifiers.push(Box::new(n));
                    }
                }
                "sms" => {
                    if config.sms.enabled {
                        let n = sms::SmsNotifier::new(config.sms.clone())?;
                        tracing::info!("notify: sms channel enabled (api={})", config.sms.api_url);
                        notifiers.push(Box::new(n));
                    }
                }
                "webhook" => {
                    if config.webhook.enabled {
                        let n = webhook::WebhookNotifier::new(config.webhook.clone())?;
                        tracing::info!("notify: webhook channel enabled (url={})", config.webhook.url);
                        notifiers.push(Box::new(n));
                    }
                }
                other => {
                    tracing::warn!("notify: unknown channel '{other}', skipped");
                }
            }
        }

        if notifiers.is_empty() {
            tracing::info!("notify: no notification channels configured");
        }

        Ok(Arc::new(Self {
            notifiers,
            min_level: config.min_level,
        }))
    }

    /// 分发告警到所有已启用的通道
    pub async fn dispatch(&self, msg: AlertMessage) -> Result<(), NotifyError> {
        // 级别过滤
        if !self.level_passes(msg.level) {
            return Ok(());
        }

        let mut errors = Vec::new();
        for notifier in &self.notifiers {
            let name = notifier.name().to_string();
            if let Err(e) = notifier.send(&msg).await {
                tracing::error!("notify [{name}] failed: {e}");
                errors.push(format!("{name}: {e}"));
            } else {
                tracing::info!("notify [{name}] sent: {}", msg.title);
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(NotifyError::Other(errors.join("; ")))
        }
    }

    fn level_passes(&self, level: AlertLevel) -> bool {
        use AlertLevel::*;
        let order = |l: AlertLevel| match l {
            Info => 0,
            Warn => 1,
            Error => 2,
            Critical => 3,
        };
        order(level) >= order(self.min_level)
    }
}
