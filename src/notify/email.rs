//! 邮件告警通道（SMTP）

use serde::{Deserialize, Serialize};

use super::{AlertMessage, Notifier, NotifyError};

// ── 邮件配置 ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmailConfig {
    /// 是否启用邮件通知
    pub enabled: bool,
    /// SMTP 服务器地址
    pub smtp_host: String,
    /// SMTP 端口（通常 587 for STARTTLS，465 for SSL/TLS，25 for plain）
    pub smtp_port: u16,
    /// 用户名
    pub username: String,
    /// 密码/授权码
    pub password: String,
    /// 发件人地址
    pub from_addr: String,
    /// 发件人名称
    pub from_name: String,
    /// 收件人地址列表
    pub to_addrs: Vec<String>,
    /// 是否使用 STARTTLS
    pub use_starttls: bool,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            smtp_host: "smtp.example.com".into(),
            smtp_port: 587,
            username: String::new(),
            password: String::new(),
            from_addr: String::new(),
            from_name: "Monitor Alert".into(),
            to_addrs: Vec::new(),
            use_starttls: true,
        }
    }
}

// ── 邮件发送器 ────────────────────────────────────────────────────────────────

pub struct EmailNotifier {
    config: EmailConfig,
}

impl EmailNotifier {
    pub fn new(config: EmailConfig) -> Result<Self, NotifyError> {
        if config.smtp_host.is_empty() {
            return Err(NotifyError::Config("smtp_host is required".into()));
        }
        if config.from_addr.is_empty() {
            return Err(NotifyError::Config("from_addr is required".into()));
        }
        if config.to_addrs.is_empty() {
            return Err(NotifyError::Config(
                "at least one to_addr is required".into(),
            ));
        }
        Ok(Self { config })
    }

    fn build_email(&self, msg: &AlertMessage) -> Result<lettre::Message, NotifyError> {
        use lettre::message::{header, MessageBuilder, MultiPart, SinglePart};

        let level_color = match msg.level {
            super::AlertLevel::Info => "#2196F3",
            super::AlertLevel::Warn => "#FF9800",
            super::AlertLevel::Error => "#F44336",
            super::AlertLevel::Critical => "#B71C1C",
        };

        let html_body = format!(
            r#"<!DOCTYPE html>
<html><body style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto;">
<div style="background: {color}; color: white; padding: 16px; border-radius: 4px 4px 0 0;">
  <h2 style="margin: 0;">[{level}] {title}</h2>
</div>
<div style="border: 1px solid #e0e0e0; border-top: none; padding: 16px; border-radius: 0 0 4px 4px;">
  <p style="font-size: 15px; line-height: 1.6;">{content}</p>
  <p style="color: #888; font-size: 13px;">时间: {time}</p>
  <details style="margin-top: 12px;">
    <summary style="cursor: pointer; color: #666;">附加详情</summary>
    <pre style="background: #f5f5f5; padding: 12px; border-radius: 4px; overflow-x: auto; font-size: 13px;">{details}</pre>
  </details>
</div>
</body></html>"#,
            color = level_color,
            level = msg.level.as_str(),
            title = html_escape(&msg.title),
            content = html_escape(&msg.content).replace('\n', "<br>"),
            time = msg.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            details = html_escape(&msg.details.to_string()),
        );

        let text_body = msg.to_plain_text();

        use lettre::message::Mailbox;

        let from_mailbox: Mailbox =
            format!("{} <{}>", self.config.from_name, self.config.from_addr)
                .parse()
                .map_err(|e| NotifyError::Config(format!("invalid from address: {e}")))?;

        let mut builder = MessageBuilder::new().from(from_mailbox).subject(format!(
            "[{}] {}",
            msg.level.as_str(),
            msg.title
        ));

        for addr in &self.config.to_addrs {
            let to_mailbox: Mailbox = addr
                .parse()
                .map_err(|e| NotifyError::Config(format!("invalid to address '{addr}': {e}")))?;
            builder = builder.to(to_mailbox);
        }

        let email = builder
            .multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(header::ContentType::TEXT_PLAIN)
                            .body(text_body),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(header::ContentType::TEXT_HTML)
                            .body(html_body),
                    ),
            )
            .map_err(|e| NotifyError::Config(format!("build email failed: {e}")))?;

        Ok(email)
    }
}

#[async_trait::async_trait]
impl Notifier for EmailNotifier {
    fn name(&self) -> &str {
        "email"
    }

    async fn send(&self, msg: &AlertMessage) -> Result<(), NotifyError> {
        use lettre::transport::smtp::authentication::Credentials;
        use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

        let email = self.build_email(msg)?;

        let creds = Credentials::new(self.config.username.clone(), self.config.password.clone());

        // use_starttls = true → STARTTLS（端口 587，标准加密提交）
        // use_starttls = false → 明文/机会TLS（用于内网中继等场景）
        let transport = if self.config.use_starttls {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.config.smtp_host)
                .map_err(|e| NotifyError::Config(format!("smtp relay config error: {e}")))?
                .port(self.config.smtp_port)
                .credentials(creds)
                .build()
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.config.smtp_host)
                .port(self.config.smtp_port)
                .credentials(creds)
                .build()
        };

        transport
            .send(email)
            .await
            .map_err(|e| NotifyError::Network(format!("send email failed: {e}")))?;

        Ok(())
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}
