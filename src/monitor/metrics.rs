//! 请求指标采集

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::RwLock;

/// 请求指标快照
#[derive(Debug, Clone, Serialize)]
pub struct RequestMetricsSnapshot {
    /// 进程启动以来总请求数
    pub total_requests: u64,
    /// 进程启动以来总错误数
    pub total_errors: u64,
    /// 窗口内请求数（用于计算实时错误率和 QPS）
    pub window_requests: u64,
    /// 窗口内错误数
    pub window_errors: u64,
    /// 当前活跃请求数
    pub active_requests: u64,
    /// 窗口内平均响应时间（ms）
    pub avg_response_time_ms: f64,
    /// 窗口内 P95 响应时间（ms）
    pub p95_response_time_ms: f64,
    /// 窗口内 P99 响应时间（ms）
    pub p99_response_time_ms: f64,
    /// 每秒请求数（基于窗口计算）
    pub requests_per_second: f64,
    /// 窗口大小（秒）
    pub window_secs: u64,
}

impl RequestMetricsSnapshot {
    /// 基于滑动窗口计算实时错误率
    pub fn error_rate(&self) -> f64 {
        if self.window_requests == 0 {
            0.0
        } else {
            self.window_errors as f64 / self.window_requests as f64
        }
    }

    /// 总错误率（进程启动以来）
    pub fn total_error_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.total_errors as f64 / self.total_requests as f64
        }
    }
}

struct WindowEntry {
    duration_ms: f64,
    is_error: bool,
}

pub struct Metrics {
    total_requests: AtomicU64,
    total_errors: AtomicU64,
    active_requests: AtomicU64,
    /// 滑动窗口内的请求记录（保留最近 window_secs 秒）
    window: RwLock<Vec<(std::time::Instant, WindowEntry)>>,
    window_secs: u64,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            active_requests: AtomicU64::new(0),
            window: RwLock::new(Vec::new()),
            window_secs: 300,
        }
    }

    /// 记录请求开始，返回一个 Guard（drop 时记录结束）
    pub fn begin_request(self: &std::sync::Arc<Self>) -> RequestGuard {
        self.active_requests.fetch_add(1, Ordering::Relaxed);
        RequestGuard {
            metrics: self.clone(),
            start: std::time::Instant::now(),
            is_error: false,
        }
    }

    fn record_request(&self, duration: Duration, is_error: bool) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        if is_error {
            self.total_errors.fetch_add(1, Ordering::Relaxed);
        }
        self.active_requests.fetch_sub(1, Ordering::Relaxed);

        let entry = WindowEntry {
            duration_ms: duration.as_secs_f64() * 1000.0,
            is_error,
        };
        // 尝试非阻塞写入，失败则跳过窗口记录（避免影响请求路径）
        if let Ok(mut window) = self.window.try_write() {
            let now = std::time::Instant::now();
            let cutoff = now - Duration::from_secs(self.window_secs);
            window.retain(|(t, _)| *t > cutoff);
            window.push((now, entry));
        }
    }

    /// 获取当前指标快照
    pub async fn snapshot(&self) -> RequestMetricsSnapshot {
        let window = self.window.read().await;
        let now = std::time::Instant::now();
        let cutoff = now - Duration::from_secs(self.window_secs);
        let recent: Vec<_> = window.iter().filter(|(t, _)| *t > cutoff).collect();

        let mut durations: Vec<f64> = recent.iter().map(|(_, e)| e.duration_ms).collect();
        durations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let avg = if durations.is_empty() {
            0.0
        } else {
            durations.iter().sum::<f64>() / durations.len() as f64
        };
        let p95 = percentile(&durations, 0.95);
        let p99 = percentile(&durations, 0.99);

        let window_requests = recent.len() as u64;
        let window_errors = recent.iter().filter(|(_, e)| e.is_error).count() as u64;
        let rps = window_requests as f64 / self.window_secs as f64;

        RequestMetricsSnapshot {
            total_requests: self.total_requests.load(Ordering::Relaxed),
            total_errors: self.total_errors.load(Ordering::Relaxed),
            window_requests,
            window_errors,
            active_requests: self.active_requests.load(Ordering::Relaxed),
            avg_response_time_ms: avg,
            p95_response_time_ms: p95,
            p99_response_time_ms: p99,
            requests_per_second: rps,
            window_secs: self.window_secs,
        }
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f64 * p) as usize;
    sorted[idx]
}

/// 请求守卫：离开作用域时自动记录指标
pub struct RequestGuard {
    metrics: std::sync::Arc<Metrics>,
    start: std::time::Instant,
    is_error: bool,
}

impl RequestGuard {
    /// 标记该请求为错误
    pub fn mark_error(&mut self) {
        self.is_error = true;
    }
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        self.metrics.record_request(duration, self.is_error);
    }
}
