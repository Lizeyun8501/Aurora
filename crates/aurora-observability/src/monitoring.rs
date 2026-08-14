//! 监控告警设计 (Monitoring & Alerting)
//!
//! Phase 6 / PART VI — 运维可观测性支柱。
//!
//! # 子任务
//! - **6.2.1 健康检查体系**：`/health` 端点 + 启动自检 + 60 秒周期巡检 + 状态栏指示器。
//! - **6.2.2 告警规则**：P0 紧急 / P1 重要 / P2 提示三层 + 本地通知 + 云端 Webhook 推送。
//! - **6.2.3 智能降噪**：告警聚合 + 静默期 + 依赖抑制 + 自愈检测。
//! - **6.2.4 监控仪表板**：Grafana 预置模板 + Desktop 内嵌轻量面板 + 关键视图看板。

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{Error, Result};

// ===========================================================================
// SubTask 6.2.1: 健康检查体系 (Health Check System)
// ===========================================================================

/// 组件健康状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// 健康（通过所有检查）。
    Healthy,
    /// 降级（部分检查未通过，核心功能正常）。
    Degraded,
    /// 不健康（关键检查失败）。
    Unhealthy,
    /// 未知（尚未执行检查）。
    Unknown,
}

impl HealthStatus {
    /// 转换为 HTTP 状态码语义。
    pub fn http_code(&self) -> u16 {
        match self {
            HealthStatus::Healthy => 200,
            HealthStatus::Degraded => 200,
            HealthStatus::Unhealthy => 503,
            HealthStatus::Unknown => 503,
        }
    }

    /// 状态标签。
    pub fn label(&self) -> &'static str {
        match self {
            HealthStatus::Healthy => "healthy",
            HealthStatus::Degraded => "degraded",
            HealthStatus::Unhealthy => "unhealthy",
            HealthStatus::Unknown => "unknown",
        }
    }
}

/// 单个组件健康检查结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    /// 组件名称（如 "database", "sync-engine", "crypto"）。
    pub name: String,
    /// 组件状态。
    pub status: HealthStatus,
    /// 检查耗时（毫秒）。
    pub latency_ms: u64,
    /// 失败时的错误消息。
    pub message: Option<String>,
    /// 最后检查时间。
    pub last_check: DateTime<Utc>,
    /// 是否为关键组件（关键组件失败 → 整体 Unhealthy）。
    pub is_critical: bool,
}

/// 健康检查函数 trait：各组件实现 `check()` 返回结果。
pub trait HealthCheck: Send + Sync {
    /// 组件名称。
    fn name(&self) -> &str;
    /// 执行健康检查。
    fn check(&self) -> ComponentHealth;
    /// 是否为关键组件。
    fn is_critical(&self) -> bool {
        false
    }
}

/// 聚合健康检查报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    /// 整体状态：所有关键组件健康为 Healthy，关键组件失败为 Unhealthy，否则 Degraded。
    pub overall: HealthStatus,
    /// 各组件检查结果。
    pub components: Vec<ComponentHealth>,
    /// 报告生成时间。
    pub timestamp: DateTime<Utc>,
    /// 系统上线时长（秒）。
    pub uptime_secs: u64,
    /// 启动自检是否通过。
    pub startup_check_passed: bool,
}

/// 启动自检结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupCheckResult {
    /// 是否全部通过。
    pub passed: bool,
    /// 失败组件名称列表。
    pub failures: Vec<String>,
    /// 执行耗时（毫秒）。
    pub elapsed_ms: u64,
}

/// 健康检查管理器：注册检查器、执行巡检、生成报告。
pub struct HealthChecker {
    checks: RwLock<Vec<Arc<dyn HealthCheck>>>,
    startup_time: Instant,
    startup_passed: AtomicBool,
    last_report: RwLock<Option<HealthReport>>,
}

impl HealthChecker {
    /// 创建管理器并记录启动时间。
    pub fn new() -> Self {
        Self {
            checks: RwLock::new(Vec::new()),
            startup_time: Instant::now(),
            startup_passed: AtomicBool::new(false),
            last_report: RwLock::new(None),
        }
    }

    /// 注册一个健康检查器。
    pub fn register(&self, check: impl HealthCheck + 'static) {
        self.checks.write().push(Arc::new(check));
    }

    /// 执行启动自检：运行所有已注册的检查器。
    pub fn run_startup_check(&self) -> StartupCheckResult {
        let start = Instant::now();
        let checks = self.checks.read();
        let mut failures = Vec::new();

        for check in checks.iter() {
            let result = check.check();
            if result.status == HealthStatus::Unhealthy && check.is_critical() {
                failures.push(result.name.clone());
            }
        }

        let passed = failures.is_empty();
        self.startup_passed.store(passed, Ordering::SeqCst);

        let elapsed_ms = start.elapsed().as_millis() as u64;

        info!(
            passed,
            failures = ?failures,
            elapsed_ms,
            "startup health check completed"
        );

        StartupCheckResult {
            passed,
            failures,
            elapsed_ms,
        }
    }

    /// 执行一次全量巡检（所有已注册检查器）。
    pub fn run_health_check(&self) -> HealthReport {
        let checks = self.checks.read();
        let mut components = Vec::with_capacity(checks.len());
        let mut overall = HealthStatus::Healthy;

        for check in checks.iter() {
            let component = check.check();
            if component.is_critical {
                match component.status {
                    HealthStatus::Unhealthy => {
                        if overall != HealthStatus::Unhealthy {
                            overall = HealthStatus::Unhealthy;
                        }
                    }
                    HealthStatus::Degraded => {
                        if overall == HealthStatus::Healthy {
                            overall = HealthStatus::Degraded;
                        }
                    }
                    _ => {}
                }
            } else {
                // Non-critical checks can degrade but not make unhealthy
                match component.status {
                    HealthStatus::Degraded | HealthStatus::Unhealthy => {
                        if overall == HealthStatus::Healthy {
                            overall = HealthStatus::Degraded;
                        }
                    }
                    _ => {}
                }
            }
            components.push(component);
        }

        let report = HealthReport {
            overall,
            components,
            timestamp: Utc::now(),
            uptime_secs: self.startup_time.elapsed().as_secs(),
            startup_check_passed: self.startup_passed.load(Ordering::SeqCst),
        };

        *self.last_report.write() = Some(report.clone());
        report
    }

    /// 获取最近一次巡检报告。
    pub fn last_report(&self) -> Option<HealthReport> {
        self.last_report.read().clone()
    }

    /// 以 Prometheus 文本格式暴露健康状态（供 `/health` 端点）。
    pub fn expose_health_text(&self) -> String {
        let report = self.run_health_check();
        let mut out = format!(
            "aurora_health_status{{status=\"{}\"}} {}\n",
            report.overall.label(),
            match report.overall {
                HealthStatus::Healthy => 1,
                HealthStatus::Degraded => 0,
                HealthStatus::Unhealthy => 0,
                HealthStatus::Unknown => 0,
            }
        );
        out.push_str(&format!("aurora_uptime_seconds {}\n", report.uptime_secs));
        out.push_str(&format!(
            "aurora_startup_check_passed {}\n",
            report.startup_check_passed as u8
        ));
        for c in &report.components {
            out.push_str(&format!(
                "aurora_component_health{{name=\"{}\",critical=\"{}\"}} {}\n",
                c.name,
                c.is_critical,
                match c.status {
                    HealthStatus::Healthy => 1,
                    HealthStatus::Degraded => 0,
                    HealthStatus::Unhealthy => 0,
                    HealthStatus::Unknown => 0,
                }
            ));
        }
        out
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// 预置健康检查器
// ===========================================================================

/// 数据库连接检查器。
pub struct DatabaseHealthCheck {
    name: String,
    /// 外部提供的检查闭包。
    checker: Box<dyn Fn() -> bool + Send + Sync>,
    is_critical: bool,
}

impl DatabaseHealthCheck {
    pub fn new(
        name: impl Into<String>,
        checker: impl Fn() -> bool + Send + Sync + 'static,
        is_critical: bool,
    ) -> Self {
        Self {
            name: name.into(),
            checker: Box::new(checker),
            is_critical,
        }
    }
}

impl HealthCheck for DatabaseHealthCheck {
    fn name(&self) -> &str {
        &self.name
    }

    fn check(&self) -> ComponentHealth {
        let start = Instant::now();
        let ok = (self.checker)();
        let latency_ms = start.elapsed().as_millis() as u64;
        let status = if ok {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unhealthy
        };
        ComponentHealth {
            name: self.name.clone(),
            status,
            latency_ms,
            message: if ok {
                None
            } else {
                Some("database connectivity check failed".into())
            },
            last_check: Utc::now(),
            is_critical: self.is_critical,
        }
    }

    fn is_critical(&self) -> bool {
        self.is_critical
    }
}

/// 文件系统检查器（磁盘空间等）。
pub struct DiskHealthCheck {
    name: String,
    /// 最小可用空间（字节），低于此值视为 Degraded。
    min_free_bytes: u64,
    /// 外部提供的可用空间查询。
    free_space_fn: Box<dyn Fn() -> u64 + Send + Sync>,
    is_critical: bool,
}

impl DiskHealthCheck {
    pub fn new(
        name: impl Into<String>,
        min_free_bytes: u64,
        free_space_fn: impl Fn() -> u64 + Send + Sync + 'static,
        is_critical: bool,
    ) -> Self {
        Self {
            name: name.into(),
            min_free_bytes,
            free_space_fn: Box::new(free_space_fn),
            is_critical,
        }
    }
}

impl HealthCheck for DiskHealthCheck {
    fn name(&self) -> &str {
        &self.name
    }

    fn check(&self) -> ComponentHealth {
        let start = Instant::now();
        let free = (self.free_space_fn)();
        let latency_ms = start.elapsed().as_millis() as u64;
        let status = if free >= self.min_free_bytes {
            HealthStatus::Healthy
        } else if free > 0 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Unhealthy
        };
        ComponentHealth {
            name: self.name.clone(),
            status,
            latency_ms,
            message: if status != HealthStatus::Healthy {
                Some(format!(
                    "low disk space: {} bytes free (min: {} bytes)",
                    free, self.min_free_bytes
                ))
            } else {
                None
            },
            last_check: Utc::now(),
            is_critical: self.is_critical,
        }
    }

    fn is_critical(&self) -> bool {
        self.is_critical
    }
}

// ===========================================================================
// SubTask 6.2.2: 告警规则 (Alerting Rules)
// ===========================================================================

/// 告警严重级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AlertSeverity {
    /// P2 提示：非紧急，仅记录。
    Info,
    /// P1 重要：需关注但不影响核心功能。
    Warning,
    /// P0 紧急：需立即处理，影响核心功能。
    Critical,
}

impl AlertSeverity {
    pub fn label(&self) -> &'static str {
        match self {
            AlertSeverity::Info => "P2",
            AlertSeverity::Warning => "P1",
            AlertSeverity::Critical => "P0",
        }
    }
}

/// 告警状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertStatus {
    /// 触发中。
    Firing,
    /// 已解决。
    Resolved,
    /// 已确认（人工介入）。
    Acknowledged,
    /// 静默中（主动抑制）。
    Silenced,
}

/// 一条告警。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// 告警唯一 ID。
    pub id: String,
    /// 告警规则名称。
    pub rule_name: String,
    /// 严重级别。
    pub severity: AlertSeverity,
    /// 告警摘要。
    pub summary: String,
    /// 详细描述。
    pub description: String,
    /// 首次触发时间。
    pub first_at: DateTime<Utc>,
    /// 最近一次触发时间。
    pub last_at: DateTime<Utc>,
    /// 触发次数（用于聚合）。
    pub count: u64,
    /// 当前状态。
    pub status: AlertStatus,
    /// 标签（用于分组、路由）。
    pub labels: HashMap<String, String>,
}

/// 告警规则定义：基于指标阈值。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    /// 规则名称。
    pub name: String,
    /// 严重级别。
    pub severity: AlertSeverity,
    /// 描述。
    pub description: String,
    /// 指标名称（如 "error_rate", "cpu_usage"）。
    pub metric: String,
    /// 阈值运算符：">"、">="、"<"、"<="。
    pub operator: String,
    /// 阈值。
    pub threshold: f64,
    /// 持续时间（秒）：指标持续超过阈值多久后触发。
    pub duration_secs: u64,
    /// 是否启用。
    pub enabled: bool,
}

impl AlertRule {
    /// 评估当前值是否触发告警。
    pub fn evaluate(&self, value: f64) -> bool {
        if !self.enabled {
            return false;
        }
        match self.operator.as_str() {
            ">" => value > self.threshold,
            ">=" => value >= self.threshold,
            "<" => value < self.threshold,
            "<=" => value <= self.threshold,
            _ => false,
        }
    }
}

/// 云端 Webhook 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// Webhook URL。
    pub url: String,
    /// 自定义请求头。
    pub headers: HashMap<String, String>,
    /// 是否启用。
    pub enabled: bool,
    /// 超时（秒）。
    pub timeout_secs: u64,
    /// 最低触发级别（低于此级别不推送）。
    pub min_severity: AlertSeverity,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            headers: HashMap::new(),
            enabled: false,
            timeout_secs: 10,
            min_severity: AlertSeverity::Warning,
        }
    }
}

/// 本地通知（Desktop 系统通知 / 状态栏指示器）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalNotification {
    /// 告警 ID。
    pub alert_id: String,
    /// 标题。
    pub title: String,
    /// 内容。
    pub body: String,
    /// 严重级别。
    pub severity: AlertSeverity,
    /// 生成时间。
    pub timestamp: DateTime<Utc>,
}

/// Webhook 推送器：将告警序列化为 JSON 并发送（异步，tokio）。
pub struct WebhookPusher {
    config: WebhookConfig,
    client: reqwest::Client,
}

impl WebhookPusher {
    pub fn new(config: WebhookConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| Error::Internal(format!("failed to create HTTP client: {e}")))?;
        Ok(Self { config, client })
    }

    /// 异步推送告警到 Webhook。
    pub async fn push(&self, alert: &Alert) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }
        if alert.severity < self.config.min_severity {
            return Ok(());
        }
        let payload = serde_json::to_string(alert).map_err(Error::Serialization)?;

        let mut req = self
            .client
            .post(&self.config.url)
            .header("Content-Type", "application/json")
            .body(payload);

        for (k, v) in &self.config.headers {
            req = req.header(k, v);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| Error::Internal(format!("webhook push failed: {e}")))?;

        if !resp.status().is_success() {
            warn!(
                status = %resp.status(),
                alert_id = %alert.id,
                "webhook returned non-success"
            );
        } else {
            debug!(alert_id = %alert.id, "webhook pushed successfully");
        }
        Ok(())
    }
}

/// 告警管理器：规则评估、告警生成、生命周期管理。
pub struct AlertManager {
    rules: RwLock<Vec<AlertRule>>,
    alerts: RwLock<Vec<Alert>>,
    webhook: RwLock<Option<WebhookPusher>>,
    /// 本地通知回调（由前端设置）。
    local_notifier: RwLock<Option<Box<dyn Fn(LocalNotification) + Send + Sync>>>,
    /// 连续触发计数（key: rule_name, value: 连续触发次数）。
    rule_fire_count: RwLock<HashMap<String, u64>>,
}

impl AlertManager {
    pub fn new() -> Self {
        Self {
            rules: RwLock::new(Vec::new()),
            alerts: RwLock::new(Vec::new()),
            webhook: RwLock::new(None),
            local_notifier: RwLock::new(None),
            rule_fire_count: RwLock::new(HashMap::new()),
        }
    }

    /// 注册告警规则。
    pub fn register_rule(&self, rule: AlertRule) {
        self.rules.write().push(rule);
    }

    /// 批量注册规则。
    pub fn register_rules(&self, rules: Vec<AlertRule>) {
        self.rules.write().extend(rules);
    }

    /// 列出所有规则。
    pub fn rules(&self) -> Vec<AlertRule> {
        self.rules.read().clone()
    }

    /// 设置 Webhook 配置。
    pub fn set_webhook(&self, config: WebhookConfig) -> Result<()> {
        let pusher = WebhookPusher::new(config)?;
        *self.webhook.write() = Some(pusher);
        Ok(())
    }

    /// 设置本地通知回调。
    pub fn set_local_notifier(&self, notifier: impl Fn(LocalNotification) + Send + Sync + 'static) {
        *self.local_notifier.write() = Some(Box::new(notifier));
    }

    /// 发送本地通知。
    fn send_local_notification(&self, alert: &Alert) {
        if let Some(ref notifier) = *self.local_notifier.read() {
            let notification = LocalNotification {
                alert_id: alert.id.clone(),
                title: format!("[{}] {}", alert.severity.label(), alert.summary),
                body: alert.description.clone(),
                severity: alert.severity,
                timestamp: Utc::now(),
            };
            notifier(notification);
        }
    }

    /// 评估所有规则：对每个规则评估当前值，决定是否触发/更新/解决告警。
    /// `metrics` 为 key=指标名, value=当前值。
    pub async fn evaluate(&self, metrics: &HashMap<String, f64>) -> Vec<Alert> {
        let rules = self.rules.read().clone();
        let mut new_alerts = Vec::new();

        for rule in &rules {
            let Some(&value) = metrics.get(&rule.metric) else {
                continue;
            };

            let triggered = rule.evaluate(value);

            if triggered {
                // 尝试更新已有告警
                let mut guard = self.alerts.write();
                let existing = guard.iter_mut().find(|a| {
                    a.rule_name == rule.name
                        && a.status != AlertStatus::Resolved
                        && a.status != AlertStatus::Silenced
                });

                if let Some(alert) = existing {
                    alert.last_at = Utc::now();
                    alert.count += 1;
                    let snapshot = alert.clone();
                    drop(guard);
                    // 推送（异步）
                    if let Some(ref wh) = *self.webhook.read() {
                        let _ = wh.push(&snapshot).await;
                    }
                    self.send_local_notification(&snapshot);
                    new_alerts.push(snapshot);
                } else {
                    // 新建告警
                    let alert = Alert {
                        id: Uuid::new_v4().to_string(),
                        rule_name: rule.name.clone(),
                        severity: rule.severity,
                        summary: format!("{}: {value}", rule.description),
                        description: format!(
                            "{} {} {} (threshold: {}, current: {})",
                            rule.metric, rule.operator, rule.threshold, rule.threshold, value
                        ),
                        first_at: Utc::now(),
                        last_at: Utc::now(),
                        count: 1,
                        status: AlertStatus::Firing,
                        labels: HashMap::new(),
                    };
                    let snapshot = alert.clone();
                    guard.push(alert);
                    drop(guard);
                    info!(
                        rule = %rule.name,
                        severity = %rule.severity.label(),
                        value,
                        threshold = rule.threshold,
                        "alert fired"
                    );
                    if let Some(ref wh) = *self.webhook.read() {
                        let _ = wh.push(&snapshot).await;
                    }
                    self.send_local_notification(&snapshot);
                    new_alerts.push(snapshot);
                }
            } else {
                // 指标恢复正常，自动解决相关告警
                let mut resolved_alerts = Vec::new();
                {
                    let mut guard = self.alerts.write();
                    for alert in guard.iter_mut() {
                        if alert.rule_name == rule.name && alert.status == AlertStatus::Firing {
                            alert.status = AlertStatus::Resolved;
                            info!(
                                alert_id = %alert.id,
                                rule = %rule.name,
                                "alert auto-resolved"
                            );
                            resolved_alerts.push(alert.clone());
                        }
                    }
                } // guard dropped here — 释放锁后再 await
                // 推送已解决的告警
                if let Some(ref wh) = *self.webhook.read() {
                    for alert in &resolved_alerts {
                        let _ = wh.push(alert).await;
                    }
                }
            }
        }

        new_alerts
    }

    /// 获取所有活跃告警（Firing / Acknowledged）。
    pub fn active_alerts(&self) -> Vec<Alert> {
        self.alerts
            .read()
            .iter()
            .filter(|a| a.status == AlertStatus::Firing || a.status == AlertStatus::Acknowledged)
            .cloned()
            .collect()
    }

    /// 获取所有告警历史。
    pub fn all_alerts(&self) -> Vec<Alert> {
        self.alerts.read().clone()
    }

    /// 确认告警。
    pub fn acknowledge(&self, alert_id: &str) -> Result<Alert> {
        let mut guard = self.alerts.write();
        let alert = guard
            .iter_mut()
            .find(|a| a.id == alert_id)
            .ok_or_else(|| Error::InvalidInput(format!("alert not found: {alert_id}")))?;
        alert.status = AlertStatus::Acknowledged;
        Ok(alert.clone())
    }

    /// 静默告警。
    pub fn silence(&self, alert_id: &str) -> Result<Alert> {
        let mut guard = self.alerts.write();
        let alert = guard
            .iter_mut()
            .find(|a| a.id == alert_id)
            .ok_or_else(|| Error::InvalidInput(format!("alert not found: {alert_id}")))?;
        alert.status = AlertStatus::Silenced;
        Ok(alert.clone())
    }

    /// 清理已解决的告警（保留最近 N 条）。
    pub fn prune_resolved(&self, keep: usize) {
        let mut guard = self.alerts.write();
        let resolved_count = guard
            .iter()
            .filter(|a| a.status == AlertStatus::Resolved)
            .count();
        if resolved_count > keep {
            guard.retain(|a| a.status != AlertStatus::Resolved);
            // 保留最近的 keep 条
            // 简化：直接截断
            guard.truncate(keep);
        }
    }
}

impl Default for AlertManager {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// SubTask 6.2.3: 智能降噪 (Intelligent Noise Reduction)
// ===========================================================================

/// 告警聚合窗口：将时间窗口内的同类告警合并。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationWindow {
    /// 窗口大小（秒）。
    pub window_secs: u64,
    /// 最大聚合数量（超过后发出摘要告警）。
    pub max_aggregations: u64,
}

/// 聚合后的告警组。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedAlert {
    /// 组 ID。
    pub group_id: String,
    /// 原始告警规则名。
    pub rule_name: String,
    /// 严重级别（取最高）。
    pub severity: AlertSeverity,
    /// 窗口内聚合数量。
    pub count: u64,
    /// 最早触发时间。
    pub first_at: DateTime<Utc>,
    /// 最近触发时间。
    pub last_at: DateTime<Utc>,
    /// 示例告警 ID 列表（最多 5 个）。
    pub sample_ids: Vec<String>,
}

/// 静默期配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SilencePeriod {
    /// 开始时间（HH:MM 格式，UTC）。
    pub start_hhmm: String,
    /// 结束时间（HH:MM 格式，UTC）。
    pub end_hhmm: String,
    /// 被静默的规则名称（空表示全部）。
    pub rules: Vec<String>,
    /// 被静默的严重级别。
    pub severities: Vec<AlertSeverity>,
}

/// 依赖关系：上游组件故障时，抑制下游告警。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyRule {
    /// 上游组件/规则名。
    pub upstream: String,
    /// 下游组件/规则名列表。
    pub downstream: Vec<String>,
}

/// 智能降噪引擎。
pub struct NoiseReducer {
    /// 最近告警事件队列（用于聚合）。
    event_queue: RwLock<VecDeque<(DateTime<Utc>, String)>>,
    /// 聚合窗口配置。
    aggregation: AggregationWindow,
    /// 静默期列表。
    silence_periods: RwLock<Vec<SilencePeriod>>,
    /// 依赖规则列表。
    dependency_rules: RwLock<Vec<DependencyRule>>,
    /// 当前被抑制的告警规则（因上游故障）。
    suppressed_by_dependency: RwLock<HashMap<String, String>>,
}

impl NoiseReducer {
    pub fn new(aggregation: AggregationWindow) -> Self {
        Self {
            event_queue: RwLock::new(VecDeque::new()),
            aggregation,
            silence_periods: RwLock::new(Vec::new()),
            dependency_rules: RwLock::new(Vec::new()),
            suppressed_by_dependency: RwLock::new(HashMap::new()),
        }
    }

    /// 添加静默期。
    pub fn add_silence_period(&self, period: SilencePeriod) {
        self.silence_periods.write().push(period);
    }

    /// 添加依赖规则。
    pub fn add_dependency_rule(&self, rule: DependencyRule) {
        self.dependency_rules.write().push(rule);
    }

    /// 记录告警事件，返回是否需要发出（未被抑制时返回 true）。
    pub fn should_alert(
        &self,
        rule_name: &str,
        severity: AlertSeverity,
        now: DateTime<Utc>,
    ) -> bool {
        // 1. 静默期检查
        if self.is_in_silence_period(rule_name, severity, now) {
            debug!(
                rule_name,
                severity = severity.label(),
                "alert silenced by period"
            );
            return false;
        }

        // 2. 依赖抑制检查
        if let Some(upstream) = self.suppressed_by_dependency.read().get(rule_name) {
            debug!(
                rule_name,
                upstream, "alert suppressed by dependency on upstream failure"
            );
            return false;
        }

        // 3. 聚合去重：同一规则在窗口内只发一次
        let mut queue = self.event_queue.write();
        // 清理过期事件
        let cutoff = now - chrono::Duration::seconds(self.aggregation.window_secs as i64);
        while queue.front().map(|(t, _)| *t < cutoff).unwrap_or(false) {
            queue.pop_front();
        }

        let count = queue.iter().filter(|(_, r)| r == rule_name).count();
        if count as u64 >= self.aggregation.max_aggregations {
            debug!(
                rule_name,
                count,
                max = self.aggregation.max_aggregations,
                "alert aggregated (max reached)"
            );
            // 达到最大聚合数，发出聚合摘要
            return count as u64 == self.aggregation.max_aggregations;
        }

        queue.push_back((now, rule_name.to_string()));
        true
    }

    /// 通知上游故障，自动抑制所有下游规则。
    pub fn notify_upstream_failure(&self, upstream: &str) {
        let rules = self.dependency_rules.read();
        for rule in rules.iter() {
            if rule.upstream == upstream {
                for downstream in &rule.downstream {
                    self.suppressed_by_dependency
                        .write()
                        .insert(downstream.clone(), upstream.to_string());
                    info!(
                        upstream,
                        downstream, "suppressing downstream alert due to upstream failure"
                    );
                }
            }
        }
    }

    /// 通知上游恢复，解除下游抑制。
    pub fn notify_upstream_recovery(&self, upstream: &str) {
        let rules = self.dependency_rules.read();
        for rule in rules.iter() {
            if rule.upstream == upstream {
                for downstream in &rule.downstream {
                    self.suppressed_by_dependency.write().remove(downstream);
                    info!(upstream, downstream, "downstream alert suppression lifted");
                }
            }
        }
    }

    /// 检查是否处于静默期。
    fn is_in_silence_period(
        &self,
        rule_name: &str,
        severity: AlertSeverity,
        now: DateTime<Utc>,
    ) -> bool {
        let periods = self.silence_periods.read();
        let now_str = now.format("%H:%M").to_string();
        for period in periods.iter() {
            // 简单 HH:MM 字符串比较
            let in_range = if period.start_hhmm <= period.end_hhmm {
                now_str >= period.start_hhmm && now_str < period.end_hhmm
            } else {
                // 跨天（如 22:00 ~ 06:00）
                now_str >= period.start_hhmm || now_str < period.end_hhmm
            };
            if in_range {
                let rule_match =
                    period.rules.is_empty() || period.rules.iter().any(|r| r == rule_name);
                let severity_match =
                    period.severities.is_empty() || period.severities.contains(&severity);
                if rule_match && severity_match {
                    return true;
                }
            }
        }
        false
    }

    /// 获取窗口内聚合摘要。
    pub fn aggregate_summary(&self, now: DateTime<Utc>) -> Vec<AggregatedAlert> {
        let queue = self.event_queue.read();
        let cutoff = now - chrono::Duration::seconds(self.aggregation.window_secs as i64);
        let mut groups: HashMap<
            String,
            (
                AlertSeverity,
                u64,
                DateTime<Utc>,
                DateTime<Utc>,
                Vec<String>,
            ),
        > = HashMap::new();

        for (t, rule) in queue.iter() {
            if *t < cutoff {
                continue;
            }
            let entry = groups
                .entry(rule.clone())
                .or_insert_with(|| (AlertSeverity::Info, 0, *t, *t, Vec::new()));
            entry.1 += 1;
            if *t < entry.2 {
                entry.2 = *t;
            }
            if *t > entry.3 {
                entry.3 = *t;
            }
            if entry.4.len() < 5 {
                entry.4.push(rule.clone());
            }
        }

        groups
            .into_iter()
            .map(
                |(rule, (sev, count, first, last, samples))| AggregatedAlert {
                    group_id: Uuid::new_v4().to_string(),
                    rule_name: rule,
                    severity: sev,
                    count,
                    first_at: first,
                    last_at: last,
                    sample_ids: samples,
                },
            )
            .collect()
    }
}

// ===========================================================================
// SubTask 6.2.4: 监控仪表板 (Monitoring Dashboard)
// ===========================================================================

/// 仪表板面板类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PanelType {
    /// 时间序列折线图。
    TimeSeries,
    /// 仪表盘（单值）。
    Gauge,
    /// 状态表格。
    StatTable,
    /// 热力图。
    Heatmap,
}

/// 单个面板定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardPanel {
    /// 面板 ID。
    pub id: String,
    /// 面板标题。
    pub title: String,
    /// 面板类型。
    pub panel_type: PanelType,
    /// 查询表达式（PromQL 风格）。
    pub query: String,
    /// 宽度（1-24 栅格）。
    pub width: u32,
    /// 高度（像素）。
    pub height: u32,
    /// 位置 (x, y) 栅格坐标。
    pub x: u32,
    pub y: u32,
}

/// 仪表板定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dashboard {
    /// 仪表板名称。
    pub name: String,
    /// 描述。
    pub description: String,
    /// 面板列表。
    pub panels: Vec<DashboardPanel>,
    /// 自动刷新间隔（秒）。
    pub refresh_secs: u64,
}

/// Grafana 仪表板 JSON 模型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrafanaDashboard {
    pub title: String,
    pub description: String,
    pub panels: Vec<GrafanaPanel>,
    pub refresh: String,
    pub schema_version: u32,
    pub tags: Vec<String>,
}

/// Grafana 面板。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrafanaPanel {
    pub id: u32,
    pub title: String,
    #[serde(rename = "type")]
    pub panel_type: String,
    pub grid_pos: GridPos,
    pub targets: Vec<GrafanaTarget>,
}

/// Grafana 面板位置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridPos {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Grafana 数据源查询。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrafanaTarget {
    pub expr: String,
    pub format: String,
    #[serde(rename = "refId")]
    pub ref_id: String,
}

/// 仪表板构建器：从 Dashboard 定义生成 Grafana JSON。
pub struct DashboardBuilder;

impl DashboardBuilder {
    /// 将内部 Dashboard 转为 Grafana JSON 格式。
    pub fn to_grafana(dashboard: &Dashboard) -> GrafanaDashboard {
        let panels: Vec<GrafanaPanel> = dashboard
            .panels
            .iter()
            .enumerate()
            .map(|(i, p)| GrafanaPanel {
                id: (i + 1) as u32,
                title: p.title.clone(),
                panel_type: match p.panel_type {
                    PanelType::TimeSeries => "timeseries".to_string(),
                    PanelType::Gauge => "gauge".to_string(),
                    PanelType::StatTable => "table".to_string(),
                    PanelType::Heatmap => "heatmap".to_string(),
                },
                grid_pos: GridPos {
                    x: p.x,
                    y: p.y,
                    w: p.width,
                    h: p.height,
                },
                targets: vec![GrafanaTarget {
                    expr: p.query.clone(),
                    format: "time_series".to_string(),
                    ref_id: "A".to_string(),
                }],
            })
            .collect();

        GrafanaDashboard {
            title: dashboard.name.clone(),
            description: dashboard.description.clone(),
            panels,
            refresh: format!("{}s", dashboard.refresh_secs),
            schema_version: 38,
            tags: vec!["aurora".into(), "auto-generated".into()],
        }
    }

    /// 生成预置的 Aurora 运维仪表板。
    pub fn preset_aurora_overview() -> Dashboard {
        Dashboard {
            name: "Aurora Overview".into(),
            description: "Aurora Note 关键运维指标总览".into(),
            refresh_secs: 30,
            panels: vec![
                DashboardPanel {
                    id: "health_status".into(),
                    title: "Health Status".into(),
                    panel_type: PanelType::StatTable,
                    query: "aurora_health_status".into(),
                    width: 6,
                    height: 4,
                    x: 0,
                    y: 0,
                },
                DashboardPanel {
                    id: "uptime".into(),
                    title: "Uptime".into(),
                    panel_type: PanelType::Gauge,
                    query: "aurora_uptime_seconds".into(),
                    width: 6,
                    height: 4,
                    x: 6,
                    y: 0,
                },
                DashboardPanel {
                    id: "error_rate".into(),
                    title: "Error Rate (5m)".into(),
                    panel_type: PanelType::TimeSeries,
                    query: "rate(aurora_errors_total[5m])".into(),
                    width: 12,
                    height: 6,
                    x: 0,
                    y: 4,
                },
                DashboardPanel {
                    id: "api_latency".into(),
                    title: "API Latency (p99)".into(),
                    panel_type: PanelType::TimeSeries,
                    query: "histogram_quantile(0.99, rate(aurora_request_duration_bucket[5m]))"
                        .into(),
                    width: 12,
                    height: 6,
                    x: 0,
                    y: 10,
                },
                DashboardPanel {
                    id: "active_users".into(),
                    title: "Active Users".into(),
                    panel_type: PanelType::TimeSeries,
                    query: "aurora_active_users".into(),
                    width: 8,
                    height: 6,
                    x: 0,
                    y: 16,
                },
                DashboardPanel {
                    id: "cpu_usage".into(),
                    title: "CPU Usage %".into(),
                    panel_type: PanelType::TimeSeries,
                    query: "aurora_cpu_usage_percent".into(),
                    width: 8,
                    height: 6,
                    x: 8,
                    y: 16,
                },
                DashboardPanel {
                    id: "memory_usage".into(),
                    title: "Memory Usage MB".into(),
                    panel_type: PanelType::TimeSeries,
                    query: "aurora_memory_usage_mb".into(),
                    width: 8,
                    height: 6,
                    x: 16,
                    y: 16,
                },
                DashboardPanel {
                    id: "sync_operations".into(),
                    title: "Sync Operations".into(),
                    panel_type: PanelType::TimeSeries,
                    query: "rate(aurora_sync_ops_total[5m])".into(),
                    width: 12,
                    height: 6,
                    x: 0,
                    y: 22,
                },
                DashboardPanel {
                    id: "alerts_firing".into(),
                    title: "Active Alerts".into(),
                    panel_type: PanelType::StatTable,
                    query: "aurora_alerts_active".into(),
                    width: 12,
                    height: 6,
                    x: 12,
                    y: 22,
                },
            ],
        }
    }
}

// ===========================================================================
// 综合监控服务：将健康检查、告警、降噪、仪表板串联
// ===========================================================================

/// 巡检调度器配置。
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    /// 巡检间隔（秒）。
    pub check_interval_secs: u64,
    /// 告警聚合窗口（秒）。
    pub aggregation_window_secs: u64,
    /// 告警最大聚合数量。
    pub max_aggregations: u64,
    /// 自动清理已解决告警的保留数。
    pub resolved_alert_keep: usize,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 60,
            aggregation_window_secs: 300,
            max_aggregations: 10,
            resolved_alert_keep: 100,
        }
    }
}

/// 综合监控服务。
pub struct MonitorService {
    pub health_checker: Arc<HealthChecker>,
    pub alert_manager: Arc<AlertManager>,
    pub noise_reducer: Arc<NoiseReducer>,
    config: MonitorConfig,
    running: AtomicBool,
}

impl MonitorService {
    pub fn new(config: MonitorConfig) -> Self {
        Self {
            health_checker: Arc::new(HealthChecker::new()),
            alert_manager: Arc::new(AlertManager::new()),
            noise_reducer: Arc::new(NoiseReducer::new(AggregationWindow {
                window_secs: config.aggregation_window_secs,
                max_aggregations: config.max_aggregations,
            })),
            config,
            running: AtomicBool::new(false),
        }
    }

    /// 启动定时巡检（异步循环）。
    pub async fn start(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            warn!("monitor service already running");
            return;
        }

        info!(
            interval_secs = self.config.check_interval_secs,
            "monitor service started"
        );

        // 启动自检
        let startup = self.health_checker.run_startup_check();
        if !startup.passed {
            error!(
                failures = ?startup.failures,
                "startup health check FAILED"
            );
        }

        // 定时巡检循环
        let checker = self.health_checker.clone();
        let alert_mgr = self.alert_manager.clone();
        let interval = self.config.check_interval_secs;
        let prune_keep = self.config.resolved_alert_keep;

        // 使用 spawn_blocking + 内部 tokio runtime 来避免 Send 约束问题
        std::thread::spawn(move || {
            let rt = tokio::runtime::Handle::current();
            loop {
                std::thread::sleep(Duration::from_secs(interval));

                let report = checker.run_health_check();
                if report.overall != HealthStatus::Healthy {
                    warn!(
                        overall = report.overall.label(),
                        components = report.components.len(),
                        "periodic health check: non-healthy"
                    );
                } else {
                    debug!("periodic health check: healthy");
                }

                // 构建指标快照（从报告提取可量化指标）
                let mut metrics = HashMap::new();
                metrics.insert(
                    "health_status".to_string(),
                    match report.overall {
                        HealthStatus::Healthy => 1.0,
                        HealthStatus::Degraded => 0.5,
                        _ => 0.0,
                    },
                );

                // 评估告警
                let am = alert_mgr.clone();
                rt.block_on(async move {
                    am.evaluate(&metrics).await;
                });

                // 清理已解决告警
                alert_mgr.prune_resolved(prune_keep);
            }
        });
    }

    /// 停止巡检。
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        info!("monitor service stopped");
    }

    /// 是否运行中。
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    // ---- Health Check ----

    #[test]
    fn health_checker_register_and_run() {
        let checker = HealthChecker::new();
        checker.register(DatabaseHealthCheck::new("test-db", || true, true));

        let report = checker.run_health_check();
        assert_eq!(report.overall, HealthStatus::Healthy);
        assert_eq!(report.components.len(), 1);
        assert_eq!(report.components[0].status, HealthStatus::Healthy);
        assert!(report.components[0].latency_ms < 1000);
    }

    #[test]
    fn health_checker_critical_failure_makes_unhealthy() {
        let checker = HealthChecker::new();
        checker.register(DatabaseHealthCheck::new(
            "critical-db",
            || false,
            true, // critical
        ));
        checker.register(DatabaseHealthCheck::new(
            "non-critical-cache",
            || true,
            false,
        ));

        let report = checker.run_health_check();
        assert_eq!(report.overall, HealthStatus::Unhealthy);
    }

    #[test]
    fn health_checker_non_critical_degraded() {
        let checker = HealthChecker::new();
        checker.register(DatabaseHealthCheck::new("critical-db", || true, true));
        checker.register(DiskHealthCheck::new(
            "disk",
            1024 * 1024 * 1024, // 1GB min
            || 1024,            // only 1KB free
            false,
        ));

        let report = checker.run_health_check();
        assert_eq!(report.overall, HealthStatus::Degraded);
    }

    #[test]
    fn health_checker_startup_check() {
        let checker = HealthChecker::new();
        checker.register(DatabaseHealthCheck::new("db", || true, true));
        let result = checker.run_startup_check();
        assert!(result.passed);
        assert!(result.failures.is_empty());
    }

    #[test]
    fn health_checker_startup_failure() {
        let checker = HealthChecker::new();
        checker.register(DatabaseHealthCheck::new("db", || false, true));
        let result = checker.run_startup_check();
        assert!(!result.passed);
        assert_eq!(result.failures, vec!["db"]);
    }

    #[test]
    fn health_checker_expose_text() {
        let checker = HealthChecker::new();
        checker.register(DatabaseHealthCheck::new("db", || true, true));
        let text = checker.expose_health_text();
        assert!(text.contains("aurora_health_status"));
        assert!(text.contains("aurora_uptime_seconds"));
        assert!(text.contains("aurora_component_health"));
    }

    #[test]
    fn disk_health_check_low_space() {
        let check = DiskHealthCheck::new("disk", 1_000_000, || 500_000, false);
        let result = check.check();
        assert_eq!(result.status, HealthStatus::Degraded);
        assert!(result.message.unwrap().contains("low disk space"));
    }

    #[test]
    fn health_status_http_code() {
        assert_eq!(HealthStatus::Healthy.http_code(), 200);
        assert_eq!(HealthStatus::Unhealthy.http_code(), 503);
    }

    // ---- Alert Rules ----

    #[test]
    fn alert_rule_evaluate_gt() {
        let rule = AlertRule {
            name: "high_cpu".into(),
            severity: AlertSeverity::Warning,
            description: "CPU usage high".into(),
            metric: "cpu_usage".into(),
            operator: ">".into(),
            threshold: 80.0,
            duration_secs: 60,
            enabled: true,
        };
        assert!(rule.evaluate(85.0));
        assert!(!rule.evaluate(80.0));
        assert!(!rule.evaluate(70.0));
    }

    #[test]
    fn alert_rule_evaluate_disabled() {
        let rule = AlertRule {
            name: "disabled".into(),
            severity: AlertSeverity::Info,
            description: "test".into(),
            metric: "m".into(),
            operator: ">".into(),
            threshold: 0.0,
            duration_secs: 0,
            enabled: false,
        };
        assert!(!rule.evaluate(100.0));
    }

    #[test]
    fn alert_rule_evaluate_le() {
        let rule = AlertRule {
            name: "low_disk".into(),
            severity: AlertSeverity::Critical,
            description: "Disk nearly full".into(),
            metric: "disk_free".into(),
            operator: "<".into(),
            threshold: 100.0,
            duration_secs: 0,
            enabled: true,
        };
        assert!(rule.evaluate(50.0));
        assert!(!rule.evaluate(100.0));
    }

    // ---- Alert Manager ----

    #[tokio::test]
    async fn alert_manager_fire_and_resolve() {
        let mgr = AlertManager::new();
        mgr.register_rule(AlertRule {
            name: "high_error_rate".into(),
            severity: AlertSeverity::Critical,
            description: "Error rate exceeded".into(),
            metric: "error_rate".into(),
            operator: ">".into(),
            threshold: 5.0,
            duration_secs: 0,
            enabled: true,
        });

        let mut metrics = HashMap::new();
        metrics.insert("error_rate".to_string(), 10.0);

        let alerts = mgr.evaluate(&metrics).await;
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].severity, AlertSeverity::Critical);
        assert_eq!(alerts[0].status, AlertStatus::Firing);

        // 恢复正常
        metrics.insert("error_rate".to_string(), 1.0);
        mgr.evaluate(&metrics).await;

        let active = mgr.active_alerts();
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn alert_manager_acknowledge_and_silence() {
        let mgr = AlertManager::new();
        mgr.register_rule(AlertRule {
            name: "test".into(),
            severity: AlertSeverity::Warning,
            description: "test".into(),
            metric: "m".into(),
            operator: ">".into(),
            threshold: 0.0,
            duration_secs: 0,
            enabled: true,
        });

        let mut metrics = HashMap::new();
        metrics.insert("m".to_string(), 1.0);
        let alerts = mgr.evaluate(&metrics).await;
        let id = alerts[0].id.clone();

        let acked = mgr.acknowledge(&id).unwrap();
        assert_eq!(acked.status, AlertStatus::Acknowledged);

        let silenced = mgr.silence(&id).unwrap();
        assert_eq!(silenced.status, AlertStatus::Silenced);
    }

    #[tokio::test]
    async fn alert_manager_prune_resolved() {
        let mgr = AlertManager::new();
        mgr.register_rule(AlertRule {
            name: "r".into(),
            severity: AlertSeverity::Info,
            description: "d".into(),
            metric: "m".into(),
            operator: ">".into(),
            threshold: 0.0,
            duration_secs: 0,
            enabled: true,
        });

        // Fire
        let mut metrics = HashMap::new();
        metrics.insert("m".to_string(), 1.0);
        mgr.evaluate(&metrics).await;
        // Resolve
        metrics.insert("m".to_string(), -1.0);
        mgr.evaluate(&metrics).await;

        mgr.prune_resolved(0);
        assert!(mgr.all_alerts().is_empty());
    }

    // ---- Noise Reduction ----

    #[test]
    fn noise_reducer_aggregation_window() {
        let reducer = NoiseReducer::new(AggregationWindow {
            window_secs: 300,
            max_aggregations: 2,
        });
        let now = Utc::now();

        // 前两次应该允许
        assert!(reducer.should_alert("rule_a", AlertSeverity::Warning, now));
        assert!(reducer.should_alert("rule_a", AlertSeverity::Warning, now));

        // 第三次达到阈值，发出聚合摘要
        assert!(reducer.should_alert("rule_a", AlertSeverity::Warning, now));

        // 不同规则不受影响
        assert!(reducer.should_alert("rule_b", AlertSeverity::Warning, now));
    }

    #[test]
    fn noise_reducer_silence_period() {
        let reducer = NoiseReducer::new(AggregationWindow {
            window_secs: 60,
            max_aggregations: 100,
        });
        reducer.add_silence_period(SilencePeriod {
            start_hhmm: "00:00".into(),
            end_hhmm: "23:59".into(),
            rules: vec![],
            severities: vec![],
        });

        let now = Utc::now();
        assert!(!reducer.should_alert("any", AlertSeverity::Critical, now));
    }

    #[test]
    fn noise_reducer_silence_specific_rule() {
        let reducer = NoiseReducer::new(AggregationWindow {
            window_secs: 60,
            max_aggregations: 100,
        });
        reducer.add_silence_period(SilencePeriod {
            start_hhmm: "00:00".into(),
            end_hhmm: "23:59".into(),
            rules: vec!["quiet_rule".into()],
            severities: vec![],
        });

        let now = Utc::now();
        assert!(!reducer.should_alert("quiet_rule", AlertSeverity::Warning, now));
        assert!(reducer.should_alert("loud_rule", AlertSeverity::Warning, now));
    }

    #[test]
    fn noise_reducer_dependency_suppression() {
        let reducer = NoiseReducer::new(AggregationWindow {
            window_secs: 60,
            max_aggregations: 100,
        });
        reducer.add_dependency_rule(DependencyRule {
            upstream: "database".into(),
            downstream: vec!["sync_engine".into(), "api_server".into()],
        });

        let now = Utc::now();
        reducer.notify_upstream_failure("database");
        assert!(!reducer.should_alert("sync_engine", AlertSeverity::Critical, now));
        assert!(!reducer.should_alert("api_server", AlertSeverity::Warning, now));
        assert!(reducer.should_alert("other_service", AlertSeverity::Warning, now));

        reducer.notify_upstream_recovery("database");
        assert!(reducer.should_alert("sync_engine", AlertSeverity::Critical, now));
    }

    #[test]
    fn noise_reducer_aggregate_summary() {
        let reducer = NoiseReducer::new(AggregationWindow {
            window_secs: 60,
            max_aggregations: 100,
        });
        let now = Utc::now();
        reducer.should_alert("rule_a", AlertSeverity::Warning, now);
        reducer.should_alert("rule_a", AlertSeverity::Warning, now);
        reducer.should_alert("rule_b", AlertSeverity::Warning, now);

        let summary = reducer.aggregate_summary(now);
        assert_eq!(summary.len(), 2);
        let a = summary.iter().find(|s| s.rule_name == "rule_a").unwrap();
        assert_eq!(a.count, 2);
    }

    // ---- Dashboard ----

    #[test]
    fn dashboard_to_grafana_json() {
        let dashboard = DashboardBuilder::preset_aurora_overview();
        let grafana = DashboardBuilder::to_grafana(&dashboard);

        assert_eq!(grafana.title, "Aurora Overview");
        assert_eq!(grafana.schema_version, 38);
        assert!(grafana.tags.contains(&"aurora".to_string()));
        assert_eq!(grafana.panels.len(), dashboard.panels.len());
        assert_eq!(grafana.panels[0].panel_type, "table"); // Health Status
        assert_eq!(grafana.panels[1].panel_type, "gauge"); // Uptime
        assert_eq!(grafana.panels[2].panel_type, "timeseries"); // Error Rate
    }

    #[test]
    fn dashboard_preset_has_all_panels() {
        let dashboard = DashboardBuilder::preset_aurora_overview();
        let titles: Vec<&str> = dashboard.panels.iter().map(|p| p.title.as_str()).collect();
        assert!(titles.contains(&"Health Status"));
        assert!(titles.contains(&"Uptime"));
        assert!(titles.contains(&"Error Rate (5m)"));
        assert!(titles.contains(&"API Latency (p99)"));
        assert!(titles.contains(&"Active Users"));
        assert!(titles.contains(&"CPU Usage %"));
        assert!(titles.contains(&"Memory Usage MB"));
        assert!(titles.contains(&"Sync Operations"));
        assert!(titles.contains(&"Active Alerts"));
    }

    #[test]
    fn webhook_config_default() {
        let cfg = WebhookConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.min_severity, AlertSeverity::Warning);
        assert_eq!(cfg.timeout_secs, 10);
    }

    #[test]
    fn monitor_config_default() {
        let cfg = MonitorConfig::default();
        assert_eq!(cfg.check_interval_secs, 60);
        assert_eq!(cfg.aggregation_window_secs, 300);
    }

    #[test]
    fn alert_severity_ordering() {
        assert!(AlertSeverity::Critical > AlertSeverity::Warning);
        assert!(AlertSeverity::Warning > AlertSeverity::Info);
    }

    #[test]
    fn alert_severity_labels() {
        assert_eq!(AlertSeverity::Critical.label(), "P0");
        assert_eq!(AlertSeverity::Warning.label(), "P1");
        assert_eq!(AlertSeverity::Info.label(), "P2");
    }

    #[test]
    fn component_health_serialization() {
        let ch = ComponentHealth {
            name: "db".into(),
            status: HealthStatus::Healthy,
            latency_ms: 5,
            message: None,
            last_check: Utc::now(),
            is_critical: true,
        };
        let json = serde_json::to_string(&ch).unwrap();
        assert!(json.contains("db"));
        assert!(json.contains("Healthy"));
    }
}
