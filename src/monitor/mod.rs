//! 监控日志模块
//!
//! 提供：
//! - 结构化事件日志（错误、警告、关键事件）
//! - 运行时指标采集（请求计数、错误率、响应时间、系统资源）
//! - 健康检查
//! - 告警规则引擎（阈值触发 → 通知通道）

pub mod metrics;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::System;
use tokio::sync::RwLock;

use crate::notify::{AlertLevel, AlertMessage, NotifyDispatcher};

// ── 监控事件 ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorEvent {
    pub timestamp: DateTime<Utc>,
    pub level: AlertLevel,
    pub category: String,
    pub message: String,
    pub details: serde_json::Value,
}

// ── 告警规则配置 ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertRule {
    /// 规则名称
    pub name: String,
    /// 是否启用
    pub enabled: bool,
    /// 错误率阈值（0.0-1.0），超过则触发 Error 告警
    pub error_rate_threshold: f64,
    /// CPU 使用率阈值（0.0-1.0），超过则触发 Warn 告警
    pub cpu_threshold: f64,
    /// 内存使用率阈值（0.0-1.0），超过则触发 Warn 告警
    pub memory_threshold: f64,
    /// 触发冷却时间（秒），避免重复告警
    pub cooldown_secs: u64,
}

impl Default for AlertRule {
    fn default() -> Self {
        Self {
            name: "default".into(),
            enabled: true,
            error_rate_threshold: 0.1,
            cpu_threshold: 0.9,
            memory_threshold: 0.9,
            cooldown_secs: 300,
        }
    }
}

// ── 监控配置 ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MonitorConfig {
    /// 是否启用监控
    pub enabled: bool,
    /// 系统指标采集间隔（秒）
    pub collect_interval_secs: u64,
    /// 内存中保留的最大事件数
    pub max_events: usize,
    /// 告警规则
    pub alert_rules: Vec<AlertRule>,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            collect_interval_secs: 30,
            max_events: 1000,
            alert_rules: vec![AlertRule::default()],
        }
    }
}

// ── 系统快照 ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SystemSnapshot {
    pub timestamp: DateTime<Utc>,
    pub cpu_usage: f64,
    pub memory_used: u64,
    pub memory_total: u64,
    pub memory_usage: f64,
    pub uptime_secs: u64,
    pub load_avg: f64,
}

// ── 监控器核心 ────────────────────────────────────────────────────────────────

pub struct Monitor {
    config: MonitorConfig,
    events: RwLock<VecDeque<MonitorEvent>>,
    metrics: Arc<metrics::Metrics>,
    system: RwLock<System>,
    dispatcher: Arc<NotifyDispatcher>,
    last_alert: RwLock<std::collections::HashMap<String, Instant>>,
    started_at: Instant,
}

impl Monitor {
    pub fn new(config: MonitorConfig, dispatcher: Arc<NotifyDispatcher>) -> Arc<Self> {
        let mut system = System::new();
        system.refresh_all();

        Arc::new(Self {
            config,
            events: RwLock::new(VecDeque::new()),
            metrics: Arc::new(metrics::Metrics::new()),
            system: RwLock::new(system),
            dispatcher,
            last_alert: RwLock::new(std::collections::HashMap::new()),
            started_at: Instant::now(),
        })
    }

    pub fn metrics(&self) -> Arc<metrics::Metrics> {
        self.metrics.clone()
    }

    /// 记录一条监控事件
    pub async fn event(&self, level: AlertLevel, category: &str, message: &str, details: serde_json::Value) {
        let evt = MonitorEvent {
            timestamp: Utc::now(),
            level,
            category: category.to_string(),
            message: message.to_string(),
            details,
        };

        // 写入 tracing 日志
        match evt.level {
            AlertLevel::Info => tracing::info!(category, "{}", message),
            AlertLevel::Warn => tracing::warn!(category, "{}", message),
            AlertLevel::Error => tracing::error!(category, "{}", message),
            AlertLevel::Critical => {
                tracing::error!(category, "[CRITICAL] {}", message);
            }
        }

        // 存入环形缓冲
        {
            let mut events = self.events.write().await;
            events.push_back(evt.clone());
            if events.len() > self.config.max_events {
                events.pop_front();
            }
        }

        // Error/Critical 立即触发告警
        if matches!(level, AlertLevel::Error | AlertLevel::Critical) {
            self.dispatch_alert(evt).await;
        }
    }

    /// 快捷方法：记录 Info 事件
    pub async fn info(&self, category: &str, message: &str) {
        self.event(AlertLevel::Info, category, message, serde_json::json!({})).await;
    }

    /// 快捷方法：记录 Warn 事件
    pub async fn warn(&self, category: &str, message: &str) {
        self.event(AlertLevel::Warn, category, message, serde_json::json!({})).await;
    }

    /// 快捷方法：记录 Error 事件
    pub async fn error(&self, category: &str, message: &str) {
        self.event(AlertLevel::Error, category, message, serde_json::json!({})).await;
    }

    /// 快捷方法：记录 Error 事件并附带详情
    pub async fn error_with_details(&self, category: &str, message: &str, details: serde_json::Value) {
        self.event(AlertLevel::Error, category, message, details).await;
    }

    /// 快捷方法：记录 Critical 事件
    pub async fn critical(&self, category: &str, message: &str) {
        self.event(AlertLevel::Critical, category, message, serde_json::json!({})).await;
    }

    /// 获取最近事件列表
    pub async fn recent_events(&self, limit: usize) -> Vec<MonitorEvent> {
        let events = self.events.read().await;
        events.iter().rev().take(limit).cloned().collect()
    }

    /// 获取当前系统快照
    pub async fn system_snapshot(&self) -> SystemSnapshot {
        let mut sys = self.system.write().await;
        sys.refresh_all();
        let cpu_usage = sys.global_cpu_info().cpu_usage() as f64 / 100.0;
        let memory_total = sys.total_memory();
        let memory_used = sys.used_memory();
        let memory_usage = if memory_total > 0 {
            memory_used as f64 / memory_total as f64
        } else {
            0.0
        };
        let load_avg = System::load_average().one;
        SystemSnapshot {
            timestamp: Utc::now(),
            cpu_usage,
            memory_used,
            memory_total,
            memory_usage,
            uptime_secs: self.started_at.elapsed().as_secs(),
            load_avg: load_avg as f64,
        }
    }

    /// 启动后台采集任务（定期检查系统指标、评估告警规则）
    pub fn start_collector(self: &Arc<Self>) {
        if !self.config.enabled {
            return;
        }
        let monitor = self.clone();
        let interval = Duration::from_secs(self.config.collect_interval_secs);

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.tick().await; // 跳过第一次立即触发
            loop {
                ticker.tick().await;
                monitor.tick().await;
            }
        });
    }

    /// 单次采集：刷新系统指标，检查告警规则
    async fn tick(&self) {
        let snapshot = self.system_snapshot().await;
        let req_metrics = self.metrics.snapshot().await;

        // 检查各条规则
        for rule in &self.config.alert_rules {
            if !rule.enabled {
                continue;
            }
            self.evaluate_rule(rule, &snapshot, &req_metrics).await;
        }
    }

    async fn evaluate_rule(
        &self,
        rule: &AlertRule,
        snap: &SystemSnapshot,
        req: &metrics::RequestMetricsSnapshot,
    ) {
        // 错误率告警（基于滑动窗口）
        if req.window_requests > 10 && req.error_rate() > rule.error_rate_threshold {
            let key = format!("{}:error_rate", rule.name);
            if self.should_alert(&key, rule.cooldown_secs).await {
                let evt = MonitorEvent {
                    timestamp: Utc::now(),
                    level: AlertLevel::Error,
                    category: "alert.error_rate".into(),
                    message: format!(
                        "错误率过高: {:.2}% (阈值 {:.2}%), 窗口请求: {}, 窗口错误: {}",
                        req.error_rate() * 100.0,
                        rule.error_rate_threshold * 100.0,
                        req.window_requests,
                        req.window_errors,
                    ),
                    details: serde_json::to_value(req).unwrap_or(serde_json::json!({})),
                };
                self.dispatch_alert(evt).await;
            }
        }

        // CPU 告警
        if snap.cpu_usage > rule.cpu_threshold {
            let key = format!("{}:cpu", rule.name);
            if self.should_alert(&key, rule.cooldown_secs).await {
                let evt = MonitorEvent {
                    timestamp: Utc::now(),
                    level: AlertLevel::Warn,
                    category: "alert.cpu".into(),
                    message: format!(
                        "CPU 使用率过高: {:.2}% (阈值 {:.2}%)",
                        snap.cpu_usage * 100.0,
                        rule.cpu_threshold * 100.0,
                    ),
                    details: serde_json::json!({"cpu_usage": snap.cpu_usage}),
                };
                self.dispatch_alert(evt).await;
            }
        }

        // 内存告警
        if snap.memory_usage > rule.memory_threshold {
            let key = format!("{}:memory", rule.name);
            if self.should_alert(&key, rule.cooldown_secs).await {
                let evt = MonitorEvent {
                    timestamp: Utc::now(),
                    level: AlertLevel::Warn,
                    category: "alert.memory".into(),
                    message: format!(
                        "内存使用率过高: {:.2}% (阈值 {:.2}%)",
                        snap.memory_usage * 100.0,
                        rule.memory_threshold * 100.0,
                    ),
                    details: serde_json::json!({
                        "memory_usage": snap.memory_usage,
                        "memory_used_mb": snap.memory_used / 1024 / 1024,
                        "memory_total_mb": snap.memory_total / 1024 / 1024,
                    }),
                };
                self.dispatch_alert(evt).await;
            }
        }
    }

    async fn should_alert(&self, key: &str, cooldown_secs: u64) -> bool {
        let mut last = self.last_alert.write().await;
        let now = Instant::now();
        if let Some(t) = last.get(key) {
            if now.duration_since(*t) < Duration::from_secs(cooldown_secs) {
                return false;
            }
        }
        last.insert(key.to_string(), now);
        true
    }

    async fn dispatch_alert(&self, evt: MonitorEvent) {
        let msg = AlertMessage {
            level: evt.level,
            title: format!("[{}] {}", evt.category, evt.level.as_str()),
            content: evt.message.clone(),
            details: evt.details.clone(),
            timestamp: evt.timestamp,
        };
        if let Err(e) = self.dispatcher.dispatch(msg).await {
            tracing::error!("failed to dispatch alert: {e}");
        }
    }
}

// ── 汇总监控状态（用于管理后台 API） ─────────────────────────────────────────

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct MonitorStatus {
    pub enabled: bool,
    pub system: SystemSnapshot,
    pub requests: metrics::RequestMetricsSnapshot,
    pub recent_events: Vec<MonitorEvent>,
}
