//! Webhook 告警通道
//!
//! 支持钉钉机器人、飞书机器人、Slack Incoming Webhook、企业微信机器人及通用 JSON webhook。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{AlertMessage, Notifier, NotifyError};

// ── Webhook 类型 ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebhookType {
    /// 通用 JSON（POST 原始消息 JSON）
    Generic,
    /// 钉钉机器人
    DingTalk,
    /// 飞书（Lark）自定义机器人
    Feishu,
    /// Slack Incoming Webhook
    Slack,
    /// 企业微信机器人
    WeCom,
}

impl Default for WebhookType {
    fn default() -> Self {
        WebhookType::Generic
    }
}

// ── Webhook 配置 ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebhookConfig {
    /// 是否启用 webhook 通知
    pub enabled: bool,
    /// Webhook URL
    pub url: String,
    /// Webhook 类型
    #[serde(rename = "type")]
    pub webhook_type: WebhookType,
    /// 钉钉/飞书 加签密钥（可选）
    pub secret: String,
    /// 额外 HTTP 头
    pub extra_headers: HashMap<String, String>,
    /// 超时秒数
    pub timeout_secs: u64,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: String::new(),
            webhook_type: WebhookType::Generic,
            secret: String::new(),
            extra_headers: HashMap::new(),
            timeout_secs: 10,
        }
    }
}

// ── Webhook 发送器 ────────────────────────────────────────────────────────────

pub struct WebhookNotifier {
    config: WebhookConfig,
    client: reqwest::Client,
}

impl WebhookNotifier {
    pub fn new(config: WebhookConfig) -> Result<Self, NotifyError> {
        if config.url.is_empty() {
            return Err(NotifyError::Config("webhook url is required".into()));
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs.max(1)))
            .build()
            .map_err(|e| NotifyError::Config(format!("build http client: {e}")))?;
        Ok(Self { config, client })
    }

    fn build_body(&self, msg: &AlertMessage) -> serde_json::Value {
        let time_str = msg.timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string();
        let level_str = msg.level.as_str();
        let text = format!(
            "[{level_str}] {title}\n{content}\n> {time_str}",
            level_str = level_str,
            title = msg.title,
            content = msg.content,
            time_str = time_str,
        );

        match self.config.webhook_type {
            WebhookType::DingTalk => {
                // 钉钉 markdown 消息（签名通过 URL query 参数传递，不在 body 中）
                let title = format!("[{}] {}", level_str, msg.title);
                let md_text = format!(
                    "## [{}] {}\n\n{}\n\n> 时间: {}",
                    level_str, msg.title, msg.content, time_str
                );
                serde_json::json!({
                    "msgtype": "markdown",
                    "markdown": {
                        "title": title,
                        "text": md_text
                    }
                })
            }
            WebhookType::Feishu => {
                // 飞书富文本消息
                serde_json::json!({
                    "msg_type": "interactive",
                    "card": {
                        "header": {
                            "title": {
                                "tag": "plain_text",
                                "content": format!("[{}] {}", level_str, msg.title)
                            },
                            "template": match msg.level {
                                super::AlertLevel::Critical | super::AlertLevel::Error => "red",
                                super::AlertLevel::Warn => "orange",
                                super::AlertLevel::Info => "blue",
                            }
                        },
                        "elements": [
                            {
                                "tag": "div",
                                "text": {
                                    "tag": "lark_md",
                                    "content": format!("{}\n\n**时间**: {}", msg.content, time_str)
                                }
                            }
                        ]
                    }
                })
            }
            WebhookType::Slack => {
                let color = match msg.level {
                    super::AlertLevel::Critical | super::AlertLevel::Error => "danger",
                    super::AlertLevel::Warn => "warning",
                    super::AlertLevel::Info => "good",
                };
                serde_json::json!({
                    "attachments": [{
                        "color": color,
                        "title": format!("[{}] {}", level_str, msg.title),
                        "text": msg.content,
                        "fields": [
                            {"title": "时间", "value": time_str, "short": true},
                            {"title": "级别", "value": level_str, "short": true},
                        ],
                        "ts": msg.timestamp.timestamp()
                    }]
                })
            }
            WebhookType::WeCom => {
                // 企业微信 markdown 消息
                let color = match msg.level {
                    super::AlertLevel::Critical | super::AlertLevel::Error => "warning",
                    super::AlertLevel::Warn => "comment",
                    super::AlertLevel::Info => "info",
                };
                serde_json::json!({
                    "msgtype": "markdown",
                    "markdown": {
                        "content": format!(
                            "## <font color=\"{color}\">[{level}]</font> {title}\n{content}\n> 时间: <font color=\"info\">{time}</font>",
                            color = color,
                            level = level_str,
                            title = msg.title,
                            content = msg.content,
                            time = time_str,
                        )
                    }
                })
            }
            WebhookType::Generic => {
                serde_json::json!({
                    "level": level_str,
                    "title": msg.title,
                    "content": msg.content,
                    "details": msg.details,
                    "timestamp": msg.timestamp.to_rfc3339(),
                    "text": text,
                })
            }
        }
    }
}

#[async_trait::async_trait]
impl Notifier for WebhookNotifier {
    fn name(&self) -> &str {
        "webhook"
    }

    async fn send(&self, msg: &AlertMessage) -> Result<(), NotifyError> {
        let mut url = self.config.url.clone();

        // 钉钉签名参数拼接到 URL query
        if matches!(self.config.webhook_type, WebhookType::DingTalk)
            && !self.config.secret.is_empty()
        {
            let (ts, sign) = dingtalk_sign(&self.config.secret);
            url = format!("{}&timestamp={}&sign={}", url, ts, sign);
        }

        let body = self.build_body(msg);
        let mut req = self.client.post(&url).json(&body);

        for (k, v) in &self.config.extra_headers {
            req = req.header(k, v);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| NotifyError::Network(format!("webhook request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let resp_body = resp.text().await.unwrap_or_default();
            return Err(NotifyError::Network(format!(
                "webhook returned {status}: {resp_body}"
            )));
        }

        // 检查业务返回码（钉钉等平台返回 errcode:0 表示成功）
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            if let Some(errcode) = json.get("errcode").and_then(|v| v.as_i64()) {
                if errcode != 0 {
                    let errmsg = json
                        .get("errmsg")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown error");
                    return Err(NotifyError::Network(format!(
                        "webhook errcode={errcode}: {errmsg}"
                    )));
                }
            }
            if let Some(code) = json.get("code").and_then(|v| v.as_i64()) {
                if code != 0 {
                    let msg = json
                        .get("msg")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown error");
                    return Err(NotifyError::Network(format!("webhook code={code}: {msg}")));
                }
            }
        }

        Ok(())
    }
}

/// 钉钉机器人加签算法：HmacSHA256(secret, timestamp + "\n" + secret) → base64 → url-encode
fn dingtalk_sign(secret: &str) -> (String, String) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .to_string();

    let string_to_sign = format!("{}\n{}", timestamp, secret);
    // 使用 hmac （不引入额外依赖，手动实现或用现有 sha2 库）
    // 因为我们已经有 sha1 依赖，这里用简易 hmac-sha256 需要 sha2。
    // 为避免新依赖，用 ring 或直接用 Rust 原生 HMAC。
    // 这里用通用方式：如果没有 sha2，就用简单方式。实际上我们直接用 hmac + sha2 crates 更稳妥。
    // 但为了保持依赖精简，直接用已有的 crypto 库。
    // 先提供不签名版本；签名需要 hmac crate。
    // 实际这里通过 sha2 和 hmac 计算：
    let sign = compute_hmac_sha256_base64(secret.as_bytes(), string_to_sign.as_bytes());
    let sign_encoded = urlencode(&sign);
    (timestamp, sign_encoded)
}

fn compute_hmac_sha256_base64(key: &[u8], data: &[u8]) -> String {
    use base64::Engine;
    // 使用 hmac + sha2 — 通过添加到依赖实现；这里用 raw 实现避免新依赖。
    // 简单 HMAC-SHA256 实现参考 RFC 2104
    const BLOCK_SIZE: usize = 64;
    let mut key_block = [0u8; BLOCK_SIZE];
    if key.len() <= BLOCK_SIZE {
        key_block[..key.len()].copy_from_slice(key);
    } else {
        // key 长于 block size，先 hash
        let hash = sha256(key);
        key_block[..hash.len()].copy_from_slice(&hash);
    }
    let mut o_pad = [0x5c; BLOCK_SIZE];
    let mut i_pad = [0x36; BLOCK_SIZE];
    for i in 0..BLOCK_SIZE {
        o_pad[i] ^= key_block[i];
        i_pad[i] ^= key_block[i];
    }
    let mut inner_data = Vec::with_capacity(BLOCK_SIZE + data.len());
    inner_data.extend_from_slice(&i_pad);
    inner_data.extend_from_slice(data);
    let inner_hash = sha256(&inner_data);
    let mut outer_data = Vec::with_capacity(BLOCK_SIZE + 32);
    outer_data.extend_from_slice(&o_pad);
    outer_data.extend_from_slice(&inner_hash);
    let outer_hash = sha256(&outer_data);
    base64::engine::general_purpose::STANDARD.encode(outer_hash)
}

/// 纯 Rust SHA-256 实现（自包含，避免引入新依赖）
fn sha256(data: &[u8]) -> [u8; 32] {
    // SHA-256 常量
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // 预处理：填充消息
    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    // 处理每个 512-bit 块
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let mj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(mj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for i in 0..8 {
        out[i * 4..i * 4 + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    out
}

fn urlencode(s: &str) -> String {
    // 只对 base64 中的 +/= 做编码
    s.replace('+', "%2B")
        .replace('/', "%2F")
        .replace('=', "%3D")
}
