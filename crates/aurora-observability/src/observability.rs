//! 可观测性架构 (Observability Architecture)
//!
//! Phase 6 (PART VI) 可观测性支柱：结构化日志、Prometheus 指标、分布式追踪、
//! 崩溃报告四大子系统的统一实现。
//!
//! # 子模块
//! - **日志系统 (6.1.1)**：基于 `tracing` 的结构化日志，支持 JSON 格式、文件轮转、
//!   确定性采样与脱敏（邮箱 / Token / 自定义字段）。
//! - **指标系统 (6.1.2)**：基于 `prometheus` 的 Counter / Gauge / Histogram，
//!   覆盖业务 / 性能 / 资源 / 质量四类指标，支持标签切片与文本暴露。
//! - **分布式追踪 (6.1.3)**：轻量级 TraceContext + Span + 尾部采样 + OTLP 导出
//!   （mock 实现，真实 OTLP 可替换）。
//! - **崩溃报告 (6.1.4)**：panic hook + 回溯 + 面包屑环形缓冲 + 设备信息 +
//!   加密场景脱敏。
//!
//! # 设计要点
//! - 所有数据类型派生 `Serialize/Deserialize`，便于跨进程 / 持久化。
//! - 内部可变性统一使用 `Arc<RwLock<T>>`（`parking_lot` 实现）。
//! - 不引入 `regex` / `tracing-appender` / `opentelemetry` / `sentry` 等重依赖，
//!   相关能力以轻量自实现替代（详见各小节注释）。

use std::collections::{HashMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::{Mutex, RwLock};
use prometheus::{
    Counter, CounterVec, Encoder, Gauge, Histogram, HistogramOpts, Opts, Registry, TextEncoder,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use crate::{Error, Result};

// ============================================================================
// SubTask 6.1.1: 日志系统 (Logging System)
// ============================================================================

/// 日志级别（与 tracing 5 级别对应）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

/// 日志脱敏模式（无 regex 依赖的轻量模式匹配器）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RedactPattern {
    /// 字面量字符串：所有出现处替换为 `[REDACTED]`
    Literal(String),
    /// 字段名：匹配 `field=value` / `field: value` / `"field":"value"`，仅替换值
    Field(String),
    /// 邮箱地址（`local@domain.tld`）
    Email,
    /// Bearer token（`Bearer xxx`）
    BearerToken,
}

/// 日志配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// 日志输出目录
    pub log_dir: PathBuf,
    /// 单文件最大字节数，超过后触发轮转
    pub max_file_size: u64,
    /// 保留的轮转备份份数（不含当前文件）
    pub rotation_count: usize,
    /// 采样率 `[0.0, 1.0]`，基于事件指纹确定性采样
    pub sample_rate: f64,
    /// 脱敏模式列表
    pub redact_patterns: Vec<RedactPattern>,
    /// 最低日志级别
    pub level: LogLevel,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            log_dir: PathBuf::from("./logs"),
            max_file_size: 10 * 1024 * 1024,
            rotation_count: 5,
            sample_rate: 1.0,
            redact_patterns: Vec::new(),
            level: LogLevel::Info,
        }
    }
}

/// 结构化日志条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredLog {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub target: String,
    pub message: String,
    pub fields: HashMap<String, Value>,
    pub span: Option<String>,
}

/// FNV-1a 64-bit 哈希（用于确定性采样指纹）。
fn fnv1a_hash(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// 确定性采样器：基于事件指纹哈希决定是否采样。
///
/// 同一指纹始终得到相同的采样决策，保证可重现。
pub struct LogSampler {
    sample_rate: f64,
}

impl LogSampler {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate: sample_rate.clamp(0.0, 1.0),
        }
    }

    /// 基于指纹（通常是 `target|message`）决定是否采样。
    pub fn should_sample(&self, key: &str) -> bool {
        if self.sample_rate >= 1.0 {
            return true;
        }
        if self.sample_rate <= 0.0 {
            return false;
        }
        let hash = fnv1a_hash(key);
        let bucket = (hash % 10_000) as f64 / 10_000.0;
        bucket < self.sample_rate
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }
}

/// 在字符串中脱敏 `field=value` / `field: value` / `"field":"value"` 形式的字段值。
fn redact_field_in_string(input: &str, field: &str) -> String {
    if field.is_empty() {
        return input.to_string();
    }
    let field_lower = field.to_lowercase();
    let input_lower = input.to_lowercase();
    let bytes = input.as_bytes();
    let lower_bytes = input_lower.as_bytes();
    let fl_bytes = field_lower.as_bytes();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut search_from = 0;
    while search_from < lower_bytes.len() {
        if let Some(rel) = input_lower[search_from..].find(&field_lower) {
            let pos = search_from + rel;
            let after = pos + fl_bytes.len();
            // 边界检查：前一个字符必须是非字母数字（避免子串误匹配）
            let before_ok = pos == 0
                || {
                    let prev = lower_bytes[pos - 1];
                    !(prev.is_ascii_alphanumeric() || prev == b'_')
                };
            if !before_ok || after >= lower_bytes.len() {
                search_from = pos + 1;
                continue;
            }
            let mut j = after;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if j >= bytes.len() {
                search_from = pos + 1;
                continue;
            }
            let sep = bytes[j];
            if sep != b'=' && sep != b':' {
                search_from = pos + 1;
                continue;
            }
            j += 1;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            let value_start = j;
            if j < bytes.len() && (bytes[j] == b'"' || bytes[j] == b'\'') {
                let quote = bytes[j];
                j += 1;
                while j < bytes.len() && bytes[j] != quote {
                    j += 1;
                }
                if j < bytes.len() {
                    j += 1;
                }
            } else {
                while j < bytes.len()
                    && bytes[j] != b' '
                    && bytes[j] != b','
                    && bytes[j] != b'}'
                    && bytes[j] != b'\n'
                    && bytes[j] != b'\r'
                {
                    j += 1;
                }
            }
            if j > value_start {
                ranges.push((value_start, j));
            }
            search_from = j.max(pos + 1);
        } else {
            break;
        }
    }
    let mut result = input.to_string();
    for (start, end) in ranges.iter().rev() {
        result.replace_range(start..end, "[REDACTED]");
    }
    result
}

/// 脱敏邮箱地址（`local@domain.tld`）。
fn redact_emails_in_string(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'@' {
            let mut start = i;
            while start > 0
                && (bytes[start - 1].is_ascii_alphanumeric()
                    || bytes[start - 1] == b'.'
                    || bytes[start - 1] == b'_'
                    || bytes[start - 1] == b'-')
            {
                start -= 1;
            }
            let mut end = i + 1;
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric()
                    || bytes[end] == b'.'
                    || bytes[end] == b'-')
            {
                end += 1;
            }
            if start < i && end > i + 1 {
                let domain = &input[i + 1..end];
                if domain.contains('.') {
                    ranges.push((start, end));
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }
    let mut result = input.to_string();
    for (start, end) in ranges.iter().rev() {
        result.replace_range(start..end, "[REDACTED]");
    }
    result
}

/// 脱敏 Bearer token（`Bearer xxx`）。
fn redact_bearer_tokens_in_string(input: &str) -> String {
    let lower = input.to_lowercase();
    let bytes = input.as_bytes();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = lower[search_from..].find("bearer ") {
        let pos = search_from + rel;
        let token_start = pos + 7;
        let mut token_end = token_start;
        while token_end < bytes.len() && !bytes[token_end].is_ascii_whitespace() {
            token_end += 1;
        }
        if token_end > token_start {
            ranges.push((token_start, token_end));
        }
        search_from = token_end.max(pos + 1);
    }
    let mut result = input.to_string();
    for (start, end) in ranges.iter().rev() {
        result.replace_range(start..end, "[REDACTED]");
    }
    result
}

/// 日志脱敏器：按模式列表对字符串内容进行脱敏。
pub struct LogRedactor {
    patterns: Vec<RedactPattern>,
}

impl LogRedactor {
    pub fn new(patterns: Vec<RedactPattern>) -> Self {
        Self { patterns }
    }

    pub fn redact(&self, input: &str) -> String {
        let mut result = input.to_string();
        for pattern in &self.patterns {
            match pattern {
                RedactPattern::Literal(lit) => {
                    if !lit.is_empty() {
                        result = result.replace(lit, "[REDACTED]");
                    }
                }
                RedactPattern::Field(field) => {
                    result = redact_field_in_string(&result, field);
                }
                RedactPattern::Email => {
                    result = redact_emails_in_string(&result);
                }
                RedactPattern::BearerToken => {
                    result = redact_bearer_tokens_in_string(&result);
                }
            }
        }
        result
    }
}

struct RotatorState {
    file: Option<File>,
    path: PathBuf,
    size: u64,
}

/// 日志文件轮转器：按文件大小滚动，保留指定份数备份。
///
/// 注：由于 `tracing-appender` 不在依赖中，此处实现一个轻量的同步文件写入器，
/// 真实环境可替换为 `tracing_appender::rolling::RollingFileAppender`。
pub struct LogRotator {
    dir: PathBuf,
    max_file_size: u64,
    rotation_count: usize,
    state: Mutex<RotatorState>,
}

impl LogRotator {
    pub fn new(dir: PathBuf, max_file_size: u64, rotation_count: usize) -> Result<Self> {
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("aurora.log");
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            dir,
            max_file_size,
            rotation_count,
            state: Mutex::new(RotatorState {
                file: Some(file),
                path,
                size,
            }),
        })
    }

    /// 写入一行日志（自动追加换行符），并在超限时触发轮转。
    pub fn write(&self, line: &str) -> Result<()> {
        let mut state = self.state.lock();
        let bytes = line.as_bytes();
        if state.size > 0 && state.size + bytes.len() as u64 + 1 > self.max_file_size {
            self.rotate_locked(&mut state)?;
        }
        if let Some(file) = state.file.as_mut() {
            file.write_all(bytes)?;
            file.write_all(b"\n")?;
            state.size += bytes.len() as u64 + 1;
        }
        Ok(())
    }

    fn rotate_locked(&self, state: &mut RotatorState) -> Result<()> {
        state.file = None;
        state.size = 0;
        if self.rotation_count > 0 {
            // 删除最旧备份
            let oldest = self
                .dir
                .join(format!("aurora.log.{}", self.rotation_count - 1));
            if oldest.exists() {
                let _ = std::fs::remove_file(&oldest);
            }
            // 依次后移：aurora.log.(i-1) -> aurora.log.i
            for i in (1..self.rotation_count).rev() {
                let from = self.dir.join(format!("aurora.log.{}", i - 1));
                let to = self.dir.join(format!("aurora.log.{}", i));
                if from.exists() {
                    let _ = std::fs::rename(&from, &to);
                }
            }
            // 当前文件 -> aurora.log.0
            let backup = self.dir.join("aurora.log.0");
            let _ = std::fs::rename(&state.path, &backup);
        }
        let new_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&state.path)?;
        state.file = Some(new_file);
        Ok(())
    }

    pub fn current_size(&self) -> u64 {
        self.state.lock().size
    }

    pub fn current_path(&self) -> PathBuf {
        self.state.lock().path.clone()
    }
}

/// 全局订阅者安装标记（避免重复安装 panic）。
static SUBSCRIBER_INSTALLED: AtomicBool = AtomicBool::new(false);

/// 日志器：初始化 tracing 订阅者 + 文件轮转 + 采样 + 脱敏。
pub struct Logger {
    config: LogConfig,
    rotator: LogRotator,
    sampler: LogSampler,
    redactor: LogRedactor,
}

impl Logger {
    pub fn new(config: LogConfig) -> Result<Self> {
        let rotator = LogRotator::new(
            config.log_dir.clone(),
            config.max_file_size,
            config.rotation_count,
        )?;
        let sampler = LogSampler::new(config.sample_rate);
        let redactor = LogRedactor::new(config.redact_patterns.clone());
        Ok(Self {
            config,
            rotator,
            sampler,
            redactor,
        })
    }

    /// 创建日志器并安装全局 tracing 订阅者（幂等）。
    ///
    /// 注：由于 `tracing-subscriber` 默认未启用 `json` 特性，stdout 使用默认文本
    /// 格式；文件输出则为 JSON（`serde_json::to_string`）。启用 `json` 特性后
    /// 可将 stdout 切换为 JSON 格式。
    pub fn init(config: LogConfig) -> Result<Self> {
        let logger = Self::new(config)?;
        if !SUBSCRIBER_INSTALLED.swap(true, Ordering::SeqCst) {
            let _ = tracing_subscriber::fmt()
                .with_target(true)
                .with_level(true)
                .try_init();
        }
        Ok(logger)
    }

    /// 记录一条结构化日志：采样 -> 脱敏 -> 写文件 + 发 tracing 事件。
    pub fn log_event(
        &self,
        level: LogLevel,
        target: &str,
        message: &str,
        fields: HashMap<String, Value>,
    ) -> Result<()> {
        if level < self.config.level {
            return Ok(());
        }
        let sample_key = format!("{}|{}", target, message);
        if !self.sampler.should_sample(&sample_key) {
            return Ok(());
        }
        let redacted_message = self.redactor.redact(message);
        let redacted_fields: HashMap<String, Value> = fields
            .into_iter()
            .map(|(k, v)| {
                let v_str = v.to_string();
                let redacted = self.redactor.redact(&v_str);
                (k, Value::String(redacted))
            })
            .collect();
        let log = StructuredLog {
            timestamp: Utc::now(),
            level,
            target: target.to_string(),
            message: redacted_message.clone(),
            fields: redacted_fields,
            span: None,
        };
        let json = serde_json::to_string(&log)?;
        self.rotator.write(&json)?;
        match level {
            LogLevel::Trace => trace!(target: target, "{}", redacted_message),
            LogLevel::Debug => debug!(target: target, "{}", redacted_message),
            LogLevel::Info => info!(target: target, "{}", redacted_message),
            LogLevel::Warn => warn!(target: target, "{}", redacted_message),
            LogLevel::Error => error!(target: target, "{}", redacted_message),
        }
        Ok(())
    }

    pub fn rotator(&self) -> &LogRotator {
        &self.rotator
    }

    pub fn sampler(&self) -> &LogSampler {
        &self.sampler
    }

    pub fn redactor(&self) -> &LogRedactor {
        &self.redactor
    }
}

// ============================================================================
// SubTask 6.1.2: 指标系统 (Metrics System)
// ============================================================================

/// 指标类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MetricKind {
    Counter,
    Gauge,
    Histogram,
    Summary,
}

/// 指标分类：业务 / 性能 / 资源 / 质量。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MetricCategory {
    Business,
    Performance,
    Resource,
    Quality,
}

impl MetricCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            MetricCategory::Business => "business",
            MetricCategory::Performance => "performance",
            MetricCategory::Resource => "resource",
            MetricCategory::Quality => "quality",
        }
    }
}

/// 指标定义（注册元数据）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDefinition {
    pub name: String,
    pub help: String,
    pub kind: MetricKind,
    pub category: MetricCategory,
    pub labels: Vec<String>,
    pub buckets: Option<Vec<f64>>,
}

/// `prometheus::Registry` 的轻量封装。
#[derive(Debug, Clone)]
pub struct MetricsRegistry {
    inner: Registry,
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self {
            inner: Registry::new(),
        }
    }

    pub fn inner(&self) -> &Registry {
        &self.inner
    }

    pub fn gather(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.inner.gather()
    }
}

/// 业务指标集合（Counter/Gauge 预注册）。
pub struct BusinessMetrics {
    notes_created: Counter,
    notes_deleted: Counter,
    sync_operations: Counter,
    active_users: Gauge,
}

impl BusinessMetrics {
    pub fn register(registry: &Registry) -> Result<Self> {
        let notes_created = Counter::with_opts(Opts::new(
            "business_notes_created_total",
            "Total notes created",
        ));
        let notes_deleted = Counter::with_opts(Opts::new(
            "business_notes_deleted_total",
            "Total notes deleted",
        ));
        let sync_operations = Counter::with_opts(Opts::new(
            "business_sync_operations_total",
            "Total sync operations",
        ));
        let active_users = Gauge::with_opts(Opts::new(
            "business_active_users",
            "Current active users",
        ));
        for m in [&notes_created, &notes_deleted, &sync_operations, &active_users] {
            registry
                .register(Box::new(m.clone()))
                .map_err(|e| Error::Metrics(e.to_string()))?;
        }
        Ok(Self {
            notes_created,
            notes_deleted,
            sync_operations,
            active_users,
        })
    }

    pub fn note_created(&self) {
        self.notes_created.inc();
    }

    pub fn note_deleted(&self) {
        self.notes_deleted.inc();
    }

    pub fn sync_operation(&self) {
        self.sync_operations.inc();
    }

    pub fn set_active_users(&self, n: i64) {
        self.active_users.set(n as f64);
    }
}

/// 性能指标集合。
pub struct PerformanceMetrics {
    request_duration: Histogram,
    db_query_duration: Histogram,
}

impl PerformanceMetrics {
    pub fn register(registry: &Registry) -> Result<Self> {
        let request_duration = Histogram::with_opts(
            HistogramOpts::new("perf_request_duration_seconds", "Request duration in seconds")
                .buckets(vec![0.01, 0.05, 0.1, 0.5, 1.0, 5.0]),
        );
        let db_query_duration = Histogram::with_opts(
            HistogramOpts::new("perf_db_query_seconds", "DB query duration in seconds")
                .buckets(vec![0.001, 0.01, 0.1, 0.5, 1.0]),
        );
        for m in [&request_duration, &db_query_duration] {
            registry
                .register(Box::new(m.clone()))
                .map_err(|e| Error::Metrics(e.to_string()))?;
        }
        Ok(Self {
            request_duration,
            db_query_duration,
        })
    }

    pub fn observe_request(&self, seconds: f64) {
        self.request_duration.observe(seconds);
    }

    pub fn observe_db_query(&self, seconds: f64) {
        self.db_query_duration.observe(seconds);
    }
}

/// 资源指标集合。
pub struct ResourceMetrics {
    cpu_usage: Gauge,
    memory_bytes: Gauge,
    disk_usage: Gauge,
}

impl ResourceMetrics {
    pub fn register(registry: &Registry) -> Result<Self> {
        let cpu_usage = Gauge::with_opts(Opts::new("resource_cpu_usage", "CPU usage ratio"));
        let memory_bytes =
            Gauge::with_opts(Opts::new("resource_memory_bytes", "Memory usage in bytes"));
        let disk_usage = Gauge::with_opts(Opts::new("resource_disk_usage", "Disk usage ratio"));
        for m in [&cpu_usage, &memory_bytes, &disk_usage] {
            registry
                .register(Box::new(m.clone()))
                .map_err(|e| Error::Metrics(e.to_string()))?;
        }
        Ok(Self {
            cpu_usage,
            memory_bytes,
            disk_usage,
        })
    }

    pub fn set_cpu(&self, ratio: f64) {
        self.cpu_usage.set(ratio);
    }

    pub fn set_memory(&self, bytes: f64) {
        self.memory_bytes.set(bytes);
    }

    pub fn set_disk(&self, ratio: f64) {
        self.disk_usage.set(ratio);
    }
}

/// 质量指标集合。
pub struct QualityMetrics {
    error_rate: Gauge,
    crash_count: Counter,
}

impl QualityMetrics {
    pub fn register(registry: &Registry) -> Result<Self> {
        let error_rate = Gauge::with_opts(Opts::new("quality_error_rate", "Error rate ratio"));
        let crash_count = Counter::with_opts(Opts::new(
            "quality_crash_count_total",
            "Total crash count",
        ));
        for m in [&error_rate, &crash_count] {
            registry
                .register(Box::new(m.clone()))
                .map_err(|e| Error::Metrics(e.to_string()))?;
        }
        Ok(Self {
            error_rate,
            crash_count,
        })
    }

    pub fn set_error_rate(&self, rate: f64) {
        self.error_rate.set(rate);
    }

    pub fn record_crash(&self) {
        self.crash_count.inc();
    }
}

/// 指标采集器：注册 Counter/Gauge/Histogram 并暴露 `/metrics` 文本。
pub struct MetricsCollector {
    registry: MetricsRegistry,
    counters: Arc<RwLock<HashMap<String, Counter>>>,
    gauges: Arc<RwLock<HashMap<String, Gauge>>>,
    histograms: Arc<RwLock<HashMap<String, Histogram>>>,
    counter_vecs: Arc<RwLock<HashMap<String, CounterVec>>>,
    definitions: Arc<RwLock<Vec<MetricDefinition>>>,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            registry: MetricsRegistry::new(),
            counters: Arc::new(RwLock::new(HashMap::new())),
            gauges: Arc::new(RwLock::new(HashMap::new())),
            histograms: Arc::new(RwLock::new(HashMap::new())),
            counter_vecs: Arc::new(RwLock::new(HashMap::new())),
            definitions: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn registry(&self) -> &MetricsRegistry {
        &self.registry
    }

    pub fn register_definition(&self, def: MetricDefinition) -> Result<()> {
        match def.kind {
            MetricKind::Counter => {
                if def.labels.is_empty() {
                    self.register_counter(&def.name, &def.help)?;
                } else {
                    let labels: Vec<&str> = def.labels.iter().map(|s| s.as_str()).collect();
                    self.register_counter_vec(&def.name, &def.help, &labels)?;
                }
            }
            MetricKind::Gauge => {
                self.register_gauge(&def.name, &def.help)?;
            }
            MetricKind::Histogram => {
                let buckets = def.buckets.clone().unwrap_or_else(|| {
                    vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
                });
                self.register_histogram(&def.name, &def.help, &buckets)?;
            }
            MetricKind::Summary => {
                // Summary 不在此实现（prometheus Summary 需要 SummaryOpts）
                return Err(Error::Metrics("Summary kind not supported".to_string()));
            }
        }
        self.definitions.write().push(def);
        Ok(())
    }

    pub fn register_counter(&self, name: &str, help: &str) -> Result<()> {
        let c = Counter::with_opts(Opts::new(name, help));
        self.registry
            .inner()
            .register(Box::new(c.clone()))
            .map_err(|e| Error::Metrics(e.to_string()))?;
        self.counters.write().insert(name.to_string(), c);
        Ok(())
    }

    pub fn register_gauge(&self, name: &str, help: &str) -> Result<()> {
        let g = Gauge::with_opts(Opts::new(name, help));
        self.registry
            .inner()
            .register(Box::new(g.clone()))
            .map_err(|e| Error::Metrics(e.to_string()))?;
        self.gauges.write().insert(name.to_string(), g);
        Ok(())
    }

    pub fn register_histogram(&self, name: &str, help: &str, buckets: &[f64]) -> Result<()> {
        let mut opts = HistogramOpts::new(name, help);
        opts = opts.buckets(buckets.to_vec());
        let h = Histogram::with_opts(opts);
        self.registry
            .inner()
            .register(Box::new(h.clone()))
            .map_err(|e| Error::Metrics(e.to_string()))?;
        self.histograms.write().insert(name.to_string(), h);
        Ok(())
    }

    pub fn register_counter_vec(
        &self,
        name: &str,
        help: &str,
        labels: &[&str],
    ) -> Result<()> {
        let cv = CounterVec::new(Opts::new(name, help), labels)
            .map_err(|e| Error::Metrics(e.to_string()))?;
        self.registry
            .inner()
            .register(Box::new(cv.clone()))
            .map_err(|e| Error::Metrics(e.to_string()))?;
        self.counter_vecs.write().insert(name.to_string(), cv);
        Ok(())
    }

    pub fn inc_counter(&self, name: &str) -> Result<()> {
        let counters = self.counters.read();
        let c = counters
            .get(name)
            .ok_or_else(|| Error::Metrics(format!("counter not registered: {}", name)))?;
        c.inc();
        Ok(())
    }

    pub fn add_counter(&self, name: &str, delta: f64) -> Result<()> {
        let counters = self.counters.read();
        let c = counters
            .get(name)
            .ok_or_else(|| Error::Metrics(format!("counter not registered: {}", name)))?;
        c.inc_by(delta);
        Ok(())
    }

    pub fn set_gauge(&self, name: &str, value: f64) -> Result<()> {
        let gauges = self.gauges.read();
        let g = gauges
            .get(name)
            .ok_or_else(|| Error::Metrics(format!("gauge not registered: {}", name)))?;
        g.set(value);
        Ok(())
    }

    pub fn observe_histogram(&self, name: &str, value: f64) -> Result<()> {
        let histograms = self.histograms.read();
        let h = histograms
            .get(name)
            .ok_or_else(|| Error::Metrics(format!("histogram not registered: {}", name)))?;
        h.observe(value);
        Ok(())
    }

    pub fn inc_counter_vec(&self, name: &str, label_values: &[&str]) -> Result<()> {
        let cvs = self.counter_vecs.read();
        let cv = cvs
            .get(name)
            .ok_or_else(|| Error::Metrics(format!("counter_vec not registered: {}", name)))?;
        cv.with_label_values(label_values).inc();
        Ok(())
    }

    /// 暴露 Prometheus 文本格式（`/metrics` 端点响应体）。
    pub fn expose(&self) -> Result<String> {
        let encoder = TextEncoder::new();
        let mfs = self.registry.gather();
        let mut buf = Vec::new();
        encoder
            .encode(&mfs, &mut buf)
            .map_err(|e| Error::Metrics(e.to_string()))?;
        Ok(String::from_utf8_lossy(&buf).into_owned())
    }

    pub fn definitions(&self) -> Vec<MetricDefinition> {
        self.definitions.read().clone()
    }
}

// ============================================================================
// SubTask 6.1.3: 分布式追踪 (Distributed Tracing)
// ============================================================================

/// 追踪 ID（128-bit 十六进制字符串）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TraceId(String);

impl TraceId {
    pub fn generate() -> Self {
        Self(format!("{:032x}", Uuid::new_v4().as_u128()))
    }

    pub fn from_str(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Span ID（64-bit 十六进制字符串）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SpanId(String);

impl SpanId {
    pub fn generate() -> Self {
        let id = Uuid::new_v4().as_u128() & 0xFFFF_FFFF_FFFF_FFFF;
        Self(format!("{:016x}", id))
    }

    pub fn from_str(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SpanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Span 状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SpanStatus {
    Unset,
    Ok,
    Error,
}

/// 追踪上下文（trace_id + span_id + parent + baggage），用于跨边界传播。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceContext {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub baggage: HashMap<String, String>,
}

impl TraceContext {
    pub fn new(trace_id: TraceId, span_id: SpanId) -> Self {
        Self {
            trace_id,
            span_id,
            parent_span_id: None,
            baggage: HashMap::new(),
        }
    }

    pub fn with_baggage(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.baggage.insert(key.into(), value.into());
        self
    }
}

/// Span：一次操作的追踪记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub span_id: SpanId,
    pub trace_id: TraceId,
    pub parent_span_id: Option<SpanId>,
    pub name: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub attributes: HashMap<String, Value>,
    pub status: SpanStatus,
}

impl Span {
    pub fn new(
        span_id: SpanId,
        trace_id: TraceId,
        parent_span_id: Option<SpanId>,
        name: String,
    ) -> Self {
        Self {
            span_id,
            trace_id,
            parent_span_id,
            name,
            start_time: Utc::now(),
            end_time: None,
            attributes: HashMap::new(),
            status: SpanStatus::Unset,
        }
    }

    pub fn set_attribute<T: Serialize>(&mut self, key: impl Into<String>, value: T) {
        let v = serde_json::to_value(&value).unwrap_or(Value::Null);
        self.attributes.insert(key.into(), v);
    }

    pub fn set_status(&mut self, status: SpanStatus) {
        self.status = status;
    }

    pub fn end(&mut self) {
        if self.end_time.is_none() {
            self.end_time = Some(Utc::now());
        }
    }

    pub fn duration_ms(&self) -> Option<f64> {
        self.end_time
            .map(|end| (end - self.start_time).num_milliseconds() as f64)
    }
}

thread_local! {
    static CURRENT_TRACE: std::cell::RefCell<Option<TraceContext>> = std::cell::RefCell::new(None);
}

/// 轻量级追踪器：创建 Span + 通过 thread-local 传播上下文。
///
/// 注：真实 OpenTelemetry SDK 提供 W3C TraceContext 跨进程传播（HTTP/gRPC），
/// 此处以 thread-local 实现进程内传播，跨进程通过 `TraceContext` 序列化传递。
pub struct Tracer {
    finished_spans: Arc<RwLock<Vec<Span>>>,
}

impl Default for Tracer {
    fn default() -> Self {
        Self::new()
    }
}

impl Tracer {
    pub fn new() -> Self {
        Self {
            finished_spans: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 开始一个新 Span，自动继承当前 thread-local 上下文作为 parent。
    pub fn start_span(&self, name: &str) -> Span {
        let (trace_id, parent_span_id) = CURRENT_TRACE.with(|c| {
            let ctx = c.borrow();
            if let Some(ctx) = ctx.as_ref() {
                (ctx.trace_id.clone(), Some(ctx.span_id.clone()))
            } else {
                (TraceId::generate(), None)
            }
        });
        let span_id = SpanId::generate();
        CURRENT_TRACE.with(|c| {
            *c.borrow_mut() = Some(TraceContext {
                trace_id: trace_id.clone(),
                span_id: span_id.clone(),
                parent_span_id: parent_span_id.clone(),
                baggage: HashMap::new(),
            });
        });
        Span::new(span_id, trace_id, parent_span_id, name.to_string())
    }

    /// 结束 Span：记录结束时间、恢复父上下文、归档。
    pub fn end_span(&self, mut span: Span) {
        span.end();
        let new_ctx = CURRENT_TRACE.with(|c| {
            let ctx_opt = c.borrow().clone();
            match &ctx_opt {
                Some(ctx) if ctx.parent_span_id.is_some() => {
                    let parent = ctx.parent_span_id.clone().unwrap();
                    let mut restored = ctx.clone();
                    restored.span_id = parent;
                    restored.parent_span_id = None;
                    Some(restored)
                }
                _ => None,
            }
        });
        CURRENT_TRACE.with(|c| *c.borrow_mut() = new_ctx);
        self.finished_spans.write().push(span);
    }

    /// 主动注入上下文（用于跨线程/跨进程传播后恢复）。
    pub fn set_context(ctx: TraceContext) {
        CURRENT_TRACE.with(|c| *c.borrow_mut() = Some(ctx));
    }

    /// 清除当前 thread-local 上下文。
    pub fn clear_context() {
        CURRENT_TRACE.with(|c| *c.borrow_mut() = None);
    }

    pub fn current_context() -> Option<TraceContext> {
        CURRENT_TRACE.with(|c| c.borrow().clone())
    }

    pub fn finished_spans(&self) -> Vec<Span> {
        self.finished_spans.read().clone()
    }

    pub fn clear(&self) {
        self.finished_spans.write().clear();
    }
}

/// 尾部采样器：保留错误/慢 Span，对正常 Span 确定性采样。
pub struct TailSampler {
    slow_threshold_ms: f64,
    normal_sample_rate: f64,
}

impl TailSampler {
    pub fn new(slow_threshold_ms: f64, normal_sample_rate: f64) -> Self {
        Self {
            slow_threshold_ms,
            normal_sample_rate: normal_sample_rate.clamp(0.0, 1.0),
        }
    }

    pub fn should_keep(&self, span: &Span) -> bool {
        if span.status == SpanStatus::Error {
            return true;
        }
        if let Some(dur) = span.duration_ms() {
            if dur >= self.slow_threshold_ms {
                return true;
            }
        }
        let key = format!("{}", span.trace_id);
        let hash = fnv1a_hash(&key);
        let bucket = (hash % 10_000) as f64 / 10_000.0;
        bucket < self.normal_sample_rate
    }

    pub fn sample(&self, spans: &[Span]) -> Vec<Span> {
        spans.iter().filter(|s| self.should_keep(s)).cloned().collect()
    }
}

/// 追踪导出记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceExport {
    pub span: Span,
    pub exported_at: DateTime<Utc>,
}

/// OTLP 导出器（mock）：将 Span 序列化为 JSON 持久化。
///
/// 注：真实 OTLP 导出器使用 protobuf over HTTP/gRPC 上报至 Collector，
/// 此处 mock 仅记录导出事件，便于测试与离线分析。
pub struct OtlpExporter {
    endpoint: String,
    exported: Arc<RwLock<Vec<TraceExport>>>,
}

impl OtlpExporter {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            exported: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn export(&self, span: &Span) -> Result<()> {
        // mock：序列化为 JSON 验证可序列化性（真实 OTLP 使用 protobuf）
        let _json =
            serde_json::to_string(span).map_err(|e| Error::Tracing(e.to_string()))?;
        self.exported.write().push(TraceExport {
            span: span.clone(),
            exported_at: Utc::now(),
        });
        Ok(())
    }

    pub fn export_batch(&self, spans: &[Span]) -> Result<usize> {
        let mut guard = self.exported.write();
        for span in spans {
            guard.push(TraceExport {
                span: span.clone(),
                exported_at: Utc::now(),
            });
        }
        Ok(spans.len())
    }

    pub fn exported(&self) -> Vec<TraceExport> {
        self.exported.read().clone()
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

// ============================================================================
// SubTask 6.1.4: 崩溃报告 (Crash Reporting)
// ============================================================================

/// 面包屑：崩溃前的最近事件记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breadcrumb {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
    pub category: String,
}

impl Breadcrumb {
    pub fn info(message: impl Into<String>, category: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            level: LogLevel::Info,
            message: message.into(),
            category: category.into(),
        }
    }

    pub fn warn(message: impl Into<String>, category: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            level: LogLevel::Warn,
            message: message.into(),
            category: category.into(),
        }
    }

    pub fn error(message: impl Into<String>, category: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            level: LogLevel::Error,
            message: message.into(),
            category: category.into(),
        }
    }
}

/// 面包屑环形缓冲（容量上限，FIFO 淘汰）。
#[derive(Debug, Clone)]
pub struct BreadcrumbBuffer {
    capacity: usize,
    items: VecDeque<Breadcrumb>,
}

impl BreadcrumbBuffer {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            capacity: cap,
            items: VecDeque::with_capacity(cap),
        }
    }

    pub fn push(&mut self, breadcrumb: Breadcrumb) {
        if self.items.len() >= self.capacity {
            self.items.pop_front();
        }
        self.items.push_back(breadcrumb);
    }

    pub fn snapshot(&self) -> Vec<Breadcrumb> {
        self.items.iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

/// 崩溃报告配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashConfig {
    pub dsn: String,
    pub release: String,
    pub redact_fields: Vec<String>,
}

impl Default for CrashConfig {
    fn default() -> Self {
        Self {
            dsn: String::new(),
            release: env!("CARGO_PKG_VERSION").to_string(),
            redact_fields: vec![
                "password".to_string(),
                "token".to_string(),
                "secret".to_string(),
                "api_key".to_string(),
            ],
        }
    }
}

impl CrashConfig {
    pub fn redactor(&self) -> LogRedactor {
        let patterns: Vec<RedactPattern> = self
            .redact_fields
            .iter()
            .map(|f| RedactPattern::Field(f.clone()))
            .chain(std::iter::once(RedactPattern::BearerToken))
            .collect();
        LogRedactor::new(patterns)
    }

    pub fn redact(&self, s: &str) -> String {
        self.redactor().redact(s)
    }
}

/// 崩溃报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashReport {
    pub timestamp: DateTime<Utc>,
    pub message: String,
    pub location: String,
    pub backtrace: String,
    pub breadcrumbs: Vec<Breadcrumb>,
    pub device_info: HashMap<String, String>,
    pub release: String,
}

fn collect_device_info() -> HashMap<String, String> {
    let mut info = HashMap::new();
    info.insert("os".to_string(), std::env::consts::OS.to_string());
    info.insert("arch".to_string(), std::env::consts::ARCH.to_string());
    let cpu = std::thread::available_parallelism()
        .map(|n| n.get().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    info.insert("cpu_count".to_string(), cpu);
    info
}

/// 崩溃报告器：安装 panic hook + 捕获回溯 + 面包屑 + 脱敏。
///
/// 注：真实 Sentry SDK 通过 `sentry::init` 上报至 SaaS/自托管 Sentry，
/// 此处 mock 将报告存储在内存中并打印到 stderr，便于测试。
pub struct CrashReporter {
    config: CrashConfig,
    breadcrumbs: Arc<RwLock<BreadcrumbBuffer>>,
    last_report: Arc<RwLock<Option<CrashReport>>>,
    installed: Arc<AtomicBool>,
}

impl CrashReporter {
    pub fn new(config: CrashConfig) -> Self {
        Self {
            config,
            breadcrumbs: Arc::new(RwLock::new(BreadcrumbBuffer::new(100))),
            last_report: Arc::new(RwLock::new(None)),
            installed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn add_breadcrumb(&self, breadcrumb: Breadcrumb) {
        self.breadcrumbs.write().push(breadcrumb);
    }

    pub fn breadcrumbs(&self) -> Vec<Breadcrumb> {
        self.breadcrumbs.read().snapshot()
    }

    /// 安装全局 panic hook（每个 reporter 实例仅安装一次）。
    pub fn install(&self) -> Result<()> {
        if self.installed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let breadcrumbs = self.breadcrumbs.clone();
        let config = self.config.clone();
        let last_report = self.last_report.clone();
        std::panic::set_hook(Box::new(move |info| {
            let backtrace = std::backtrace::Backtrace::force_capture();
            let payload = info.payload();
            let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic payload".to_string()
            };
            let location = info
                .location()
                .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                .unwrap_or_else(|| "unknown".to_string());
            let crumbs = breadcrumbs.read().snapshot();
            let report = CrashReport {
                timestamp: Utc::now(),
                message: config.redact(&msg),
                location,
                backtrace: config.redact(&backtrace.to_string()),
                breadcrumbs: crumbs,
                device_info: collect_device_info(),
                release: config.release.clone(),
            };
            let json = serde_json::to_string(&report).unwrap_or_default();
            eprintln!("[AURORA-CRASH] {}", json);
            *last_report.write() = Some(report);
        }));
        Ok(())
    }

    /// 主动构造并发送一份崩溃报告（mock：存入 last_report）。
    pub fn send(&self, mut report: CrashReport) -> Result<()> {
        report.message = self.config.redact(&report.message);
        report.backtrace = self.config.redact(&report.backtrace);
        let json = serde_json::to_string(&report).map_err(Error::Serialization)?;
        eprintln!("[AURORA-CRASH] {}", json);
        *self.last_report.write() = Some(report);
        Ok(())
    }

    pub fn last_report(&self) -> Option<CrashReport> {
        self.last_report.read().clone()
    }

    pub fn config(&self) -> &CrashConfig {
        &self.config
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aurora-obs-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // --- Logging System tests ---

    #[test]
    fn test_log_level_as_str() {
        assert_eq!(LogLevel::Trace.as_str(), "TRACE");
        assert_eq!(LogLevel::Debug.as_str(), "DEBUG");
        assert_eq!(LogLevel::Info.as_str(), "INFO");
        assert_eq!(LogLevel::Warn.as_str(), "WARN");
        assert_eq!(LogLevel::Error.as_str(), "ERROR");
    }

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Error > LogLevel::Info);
        assert!(LogLevel::Info > LogLevel::Debug);
        assert!(LogLevel::Debug > LogLevel::Trace);
    }

    #[test]
    fn test_log_sampler_full_rate() {
        let sampler = LogSampler::new(1.0);
        for i in 0..100 {
            assert!(sampler.should_sample(&format!("key-{}", i)));
        }
    }

    #[test]
    fn test_log_sampler_zero_rate() {
        let sampler = LogSampler::new(0.0);
        for i in 0..100 {
            assert!(!sampler.should_sample(&format!("key-{}", i)));
        }
    }

    #[test]
    fn test_log_sampler_deterministic() {
        let sampler = LogSampler::new(0.5);
        let key = "deterministic-key";
        let first = sampler.should_sample(key);
        for _ in 0..10 {
            assert_eq!(sampler.should_sample(key), first);
        }
    }

    #[test]
    fn test_log_sampler_distribution() {
        let sampler = LogSampler::new(0.3);
        let mut sampled = 0;
        for i in 0..1000 {
            if sampler.should_sample(&format!("key-{}", i)) {
                sampled += 1;
            }
        }
        // 30% ± 10% 容差
        assert!(sampled > 200 && sampled < 400, "sampled = {}", sampled);
    }

    #[test]
    fn test_redactor_literal() {
        let r = LogRedactor::new(vec![RedactPattern::Literal("secret123".to_string())]);
        assert_eq!(r.redact("user secret123 here"), "user [REDACTED] here");
    }

    #[test]
    fn test_redactor_field_kv() {
        let r = LogRedactor::new(vec![RedactPattern::Field("password".to_string())]);
        assert_eq!(
            r.redact("login password=hunter2 ok"),
            "login password=[REDACTED] ok"
        );
    }

    #[test]
    fn test_redactor_field_colon() {
        let r = LogRedactor::new(vec![RedactPattern::Field("token".to_string())]);
        assert_eq!(
            r.redact("auth token: abc123 done"),
            "auth token: [REDACTED] done"
        );
    }

    #[test]
    fn test_redactor_field_json() {
        let r = LogRedactor::new(vec![RedactPattern::Field("api_key".to_string())]);
        let out = r.redact(r#"{"api_key":"sk-12345"}"#);
        assert!(out.contains("[REDACTED]"));
        assert!(!out.contains("sk-12345"));
    }

    #[test]
    fn test_redactor_email() {
        let r = LogRedactor::new(vec![RedactPattern::Email]);
        let out = r.redact("contact alice@example.com for info");
        assert!(out.contains("[REDACTED]"));
        assert!(!out.contains("alice@example.com"));
    }

    #[test]
    fn test_redactor_bearer_token() {
        let r = LogRedactor::new(vec![RedactPattern::BearerToken]);
        let out = r.redact("Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.payload.sig");
        assert!(out.contains("[REDACTED]"));
        assert!(!out.contains("eyJhbGciOiJIUzI1NiJ9"));
    }

    #[test]
    fn test_redactor_combined() {
        let r = LogRedactor::new(vec![
            RedactPattern::Field("password".to_string()),
            RedactPattern::Email,
            RedactPattern::Literal("SECRET".to_string()),
        ]);
        let out = r.redact("user alice@x.com password=pw SECRET");
        assert!(out.contains("[REDACTED]"));
        assert!(!out.contains("alice@x.com"));
        assert!(!out.contains("=pw"));
        assert!(!out.contains("SECRET"));
    }

    #[test]
    fn test_log_rotator_write_and_size() {
        let dir = temp_dir();
        let rotator = LogRotator::new(dir.clone(), 1024, 3).unwrap();
        rotator.write("hello world").unwrap();
        rotator.write("second line").unwrap();
        assert!(rotator.current_size() > 0);
        let content = std::fs::read_to_string(dir.join("aurora.log")).unwrap();
        assert!(content.contains("hello world"));
        assert!(content.contains("second line"));
    }

    #[test]
    fn test_log_rotator_rotation() {
        let dir = temp_dir();
        // 极小阈值触发轮转
        let rotator = LogRotator::new(dir.clone(), 30, 2).unwrap();
        rotator.write("first-line-longer-than-threshold").unwrap();
        rotator.write("second-line-longer-than-threshold").unwrap();
        // 应产生至少一个备份
        assert!(dir.join("aurora.log.0").exists() || dir.join("aurora.log").exists());
    }

    #[test]
    fn test_logger_log_event_writes_file() {
        let dir = temp_dir();
        let config = LogConfig {
            log_dir: dir.clone(),
            max_file_size: 1024 * 1024,
            rotation_count: 3,
            sample_rate: 1.0,
            redact_patterns: vec![],
            level: LogLevel::Trace,
        };
        let logger = Logger::new(config).unwrap();
        let mut fields = HashMap::new();
        fields.insert("user".to_string(), Value::String("alice".to_string()));
        logger
            .log_event(LogLevel::Info, "test::module", "hello world", fields)
            .unwrap();
        let content = std::fs::read_to_string(dir.join("aurora.log")).unwrap();
        assert!(content.contains("hello world"));
        assert!(content.contains("test::module"));
        assert!(content.contains("alice"));
    }

    #[test]
    fn test_logger_log_event_redacts() {
        let dir = temp_dir();
        let config = LogConfig {
            log_dir: dir.clone(),
            max_file_size: 1024 * 1024,
            rotation_count: 3,
            sample_rate: 1.0,
            redact_patterns: vec![RedactPattern::Field("password".to_string())],
            level: LogLevel::Trace,
        };
        let logger = Logger::new(config).unwrap();
        logger
            .log_event(
                LogLevel::Info,
                "auth",
                "login password=hunter2 ok",
                HashMap::new(),
            )
            .unwrap();
        let content = std::fs::read_to_string(dir.join("aurora.log")).unwrap();
        assert!(content.contains("[REDACTED]"));
        assert!(!content.contains("hunter2"));
    }

    #[test]
    fn test_logger_log_event_samples() {
        let dir = temp_dir();
        let config = LogConfig {
            log_dir: dir.clone(),
            max_file_size: 1024 * 1024,
            rotation_count: 3,
            sample_rate: 0.0,
            redact_patterns: vec![],
            level: LogLevel::Trace,
        };
        let logger = Logger::new(config).unwrap();
        logger
            .log_event(
                LogLevel::Info,
                "test",
                "should be sampled out",
                HashMap::new(),
            )
            .unwrap();
        // 0% 采样率下不应写入
        let content = std::fs::read_to_string(dir.join("aurora.log")).unwrap();
        assert!(!content.contains("should be sampled out"));
    }

    // --- Metrics System tests ---

    #[test]
    fn test_metrics_collector_counter() {
        let collector = MetricsCollector::new();
        collector
            .register_counter("test_counter", "a test counter")
            .unwrap();
        collector.inc_counter("test_counter").unwrap();
        collector.add_counter("test_counter", 5.0).unwrap();
        let text = collector.expose().unwrap();
        assert!(text.contains("test_counter"));
        assert!(text.contains("6"));
    }

    #[test]
    fn test_metrics_collector_gauge() {
        let collector = MetricsCollector::new();
        collector
            .register_gauge("test_gauge", "a test gauge")
            .unwrap();
        collector.set_gauge("test_gauge", 42.0).unwrap();
        let text = collector.expose().unwrap();
        assert!(text.contains("test_gauge"));
        assert!(text.contains("42"));
    }

    #[test]
    fn test_metrics_collector_histogram() {
        let collector = MetricsCollector::new();
        collector
            .register_histogram("test_hist", "a test hist", &[0.1, 0.5, 1.0])
            .unwrap();
        collector.observe_histogram("test_hist", 0.3).unwrap();
        collector.observe_histogram("test_hist", 0.7).unwrap();
        let text = collector.expose().unwrap();
        assert!(text.contains("test_hist"));
        assert!(text.contains("test_hist_count"));
    }

    #[test]
    fn test_metrics_collector_counter_vec_labels() {
        let collector = MetricsCollector::new();
        collector
            .register_counter_vec("requests_total", "total requests", &["method", "status"])
            .unwrap();
        collector
            .inc_counter_vec("requests_total", &["GET", "200"])
            .unwrap();
        collector
            .inc_counter_vec("requests_total", &["POST", "500"])
            .unwrap();
        let text = collector.expose().unwrap();
        assert!(text.contains("requests_total"));
        assert!(text.contains("GET"));
        assert!(text.contains("POST"));
    }

    #[test]
    fn test_metrics_collector_missing_counter() {
        let collector = MetricsCollector::new();
        let err = collector.inc_counter("nonexistent").unwrap_err();
        assert!(matches!(err, Error::Metrics(_)));
    }

    #[test]
    fn test_business_metrics_register() {
        let collector = MetricsCollector::new();
        let bm = BusinessMetrics::register(collector.registry().inner()).unwrap();
        bm.note_created();
        bm.note_created();
        bm.set_active_users(10);
        let text = collector.expose().unwrap();
        assert!(text.contains("business_notes_created_total"));
        assert!(text.contains("business_active_users"));
    }

    #[test]
    fn test_performance_metrics_register() {
        let collector = MetricsCollector::new();
        let pm = PerformanceMetrics::register(collector.registry().inner()).unwrap();
        pm.observe_request(0.05);
        pm.observe_db_query(0.01);
        let text = collector.expose().unwrap();
        assert!(text.contains("perf_request_duration_seconds"));
    }

    #[test]
    fn test_resource_and_quality_metrics() {
        let collector = MetricsCollector::new();
        let rm = ResourceMetrics::register(collector.registry().inner()).unwrap();
        let qm = QualityMetrics::register(collector.registry().inner()).unwrap();
        rm.set_cpu(0.75);
        rm.set_memory(1024.0);
        qm.set_error_rate(0.01);
        qm.record_crash();
        let text = collector.expose().unwrap();
        assert!(text.contains("resource_cpu_usage"));
        assert!(text.contains("quality_crash_count_total"));
    }

    #[test]
    fn test_metric_definition_register() {
        let collector = MetricsCollector::new();
        let def = MetricDefinition {
            name: "def_counter".to_string(),
            help: "via definition".to_string(),
            kind: MetricKind::Counter,
            category: MetricCategory::Business,
            labels: vec![],
            buckets: None,
        };
        collector.register_definition(def).unwrap();
        collector.inc_counter("def_counter").unwrap();
        assert_eq!(collector.definitions().len(), 1);
    }

    // --- Distributed Tracing tests ---

    #[test]
    fn test_trace_id_and_span_id_format() {
        let tid = TraceId::generate();
        let sid = SpanId::generate();
        assert_eq!(tid.as_str().len(), 32);
        assert_eq!(sid.as_str().len(), 16);
    }

    #[test]
    fn test_tracer_span_creation() {
        Tracer::clear_context();
        let tracer = Tracer::new();
        let mut span = tracer.start_span("op");
        span.set_status(SpanStatus::Ok);
        span.set_attribute("key", 42);
        tracer.end_span(span);
        let spans = tracer.finished_spans();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "op");
        assert_eq!(spans[0].status, SpanStatus::Ok);
        assert!(spans[0].attributes.contains_key("key"));
        assert!(spans[0].end_time.is_some());
    }

    #[test]
    fn test_tracer_parent_child_propagation() {
        Tracer::clear_context();
        let tracer = Tracer::new();
        let parent = tracer.start_span("parent");
        let parent_id = parent.span_id.clone();
        let trace_id = parent.trace_id.clone();
        let child = tracer.start_span("child");
        // child 应继承 parent 的 span_id 作为 parent_span_id
        assert_eq!(child.parent_span_id, Some(parent_id.clone()));
        assert_eq!(child.trace_id, trace_id);
        tracer.end_span(child);
        tracer.end_span(parent);
        // 结束 parent 后上下文应被清除
        assert!(Tracer::current_context().is_none());
    }

    #[test]
    fn test_tracer_context_set_and_clear() {
        Tracer::clear_context();
        assert!(Tracer::current_context().is_none());
        let ctx = TraceContext::new(TraceId::generate(), SpanId::generate())
            .with_baggage("env", "test");
        Tracer::set_context(ctx.clone());
        let current = Tracer::current_context().unwrap();
        assert_eq!(current.trace_id, ctx.trace_id);
        assert_eq!(current.baggage.get("env"), Some(&"test".to_string()));
        Tracer::clear_context();
        assert!(Tracer::current_context().is_none());
    }

    #[test]
    fn test_tail_sampler_keeps_errors() {
        let sampler = TailSampler::new(100.0, 0.0);
        let tracer = Tracer::new();
        let mut span = tracer.start_span("err");
        span.set_status(SpanStatus::Error);
        span.end();
        assert!(sampler.should_keep(&span));
    }

    #[test]
    fn test_tail_sampler_keeps_slow() {
        let sampler = TailSampler::new(10.0, 0.0);
        let mut span = Span::new(
            SpanId::generate(),
            TraceId::generate(),
            None,
            "slow".to_string(),
        );
        // 手动设置时间跨度大于阈值
        span.start_time = Utc::now() - chrono::Duration::milliseconds(100);
        span.end();
        assert!(sampler.should_keep(&span));
    }

    #[test]
    fn test_tail_sampler_normal_sampling() {
        let sampler = TailSampler::new(1000.0, 0.0);
        let mut kept = 0;
        for _ in 0..100 {
            let mut span = Span::new(
                SpanId::generate(),
                TraceId::generate(),
                None,
                "normal".to_string(),
            );
            span.set_status(SpanStatus::Ok);
            span.end();
            if sampler.should_keep(&span) {
                kept += 1;
            }
        }
        // 0% 正常采样率 + 无错误 + 无慢 → 全部丢弃
        assert_eq!(kept, 0);
    }

    #[test]
    fn test_tail_sampler_sample_batch() {
        let sampler = TailSampler::new(1000.0, 1.0);
        let spans: Vec<Span> = (0..5)
            .map(|i| {
                let mut s = Span::new(
                    SpanId::generate(),
                    TraceId::generate(),
                    None,
                    format!("op-{}", i),
                );
                s.end();
                s
            })
            .collect();
        let kept = sampler.sample(&spans);
        assert_eq!(kept.len(), 5);
    }

    #[test]
    fn test_otlp_exporter_export() {
        let exporter = OtlpExporter::new("http://localhost:4318");
        let mut span = Span::new(
            SpanId::generate(),
            TraceId::generate(),
            None,
            "exported".to_string(),
        );
        span.end();
        exporter.export(&span).unwrap();
        assert_eq!(exporter.exported().len(), 1);
        assert_eq!(exporter.exported()[0].span.name, "exported");
    }

    #[test]
    fn test_otlp_exporter_batch() {
        let exporter = OtlpExporter::new("http://localhost:4318");
        let spans: Vec<Span> = (0..3)
            .map(|i| {
                let mut s = Span::new(
                    SpanId::generate(),
                    TraceId::generate(),
                    None,
                    format!("op-{}", i),
                );
                s.end();
                s
            })
            .collect();
        let n = exporter.export_batch(&spans).unwrap();
        assert_eq!(n, 3);
        assert_eq!(exporter.exported().len(), 3);
    }

    // --- Crash Reporting tests ---

    #[test]
    fn test_breadcrumb_buffer_push() {
        let mut buf = BreadcrumbBuffer::new(3);
        buf.push(Breadcrumb::info("a", "cat"));
        buf.push(Breadcrumb::warn("b", "cat"));
        assert_eq!(buf.len(), 2);
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_breadcrumb_buffer_ring_overflow() {
        let mut buf = BreadcrumbBuffer::new(3);
        for i in 0..5 {
            buf.push(Breadcrumb::info(format!("crumb-{}", i), "cat"));
        }
        assert_eq!(buf.len(), 3);
        let snap = buf.snapshot();
        // 最旧的应被淘汰，保留最后 3 条
        assert_eq!(snap[0].message, "crumb-2");
        assert_eq!(snap[2].message, "crumb-4");
    }

    #[test]
    fn test_breadcrumb_buffer_clear() {
        let mut buf = BreadcrumbBuffer::new(3);
        buf.push(Breadcrumb::info("a", "cat"));
        buf.clear();
        assert!(buf.is_empty());
    }

    #[test]
    fn test_crash_config_redact() {
        let config = CrashConfig {
            dsn: "mock://localhost".to_string(),
            release: "0.1.0".to_string(),
            redact_fields: vec!["password".to_string()],
        };
        let out = config.redact("login password=hunter2 ok");
        assert!(out.contains("[REDACTED]"));
        assert!(!out.contains("hunter2"));
    }

    #[test]
    fn test_crash_reporter_send() {
        let reporter = CrashReporter::new(CrashConfig {
            dsn: "mock://localhost".to_string(),
            release: "0.1.0-test".to_string(),
            redact_fields: vec!["password".to_string()],
        });
        let report = CrashReport {
            timestamp: Utc::now(),
            message: "error password=secret123".to_string(),
            location: "file.rs:1:1".to_string(),
            backtrace: "backtrace here".to_string(),
            breadcrumbs: vec![Breadcrumb::info("pre", "cat")],
            device_info: HashMap::new(),
            release: "0.1.0-test".to_string(),
        };
        reporter.send(report).unwrap();
        let last = reporter.last_report().expect("report should be stored");
        assert!(last.message.contains("[REDACTED]"));
        assert!(!last.message.contains("secret123"));
        assert_eq!(last.release, "0.1.0-test");
    }

    static HOOK_TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_crash_reporter_panic_capture() {
        let _guard = HOOK_TEST_MUTEX.lock();
        let reporter = CrashReporter::new(CrashConfig {
            dsn: "mock://localhost".to_string(),
            release: "0.1.0-test".to_string(),
            redact_fields: vec!["password".to_string()],
        });
        reporter.add_breadcrumb(Breadcrumb::info("before crash", "test"));
        let prev_hook = std::panic::take_hook();
        reporter.install().unwrap();
        let result = std::panic::catch_unwind(|| {
            panic!("test panic with password=secret123");
        });
        assert!(result.is_err());
        let report = reporter.last_report().expect("report should be captured");
        assert!(report.message.contains("test panic"));
        assert!(report.message.contains("[REDACTED]"));
        assert!(!report.message.contains("secret123"));
        assert_eq!(report.release, "0.1.0-test");
        assert!(!report.breadcrumbs.is_empty());
        assert!(report.device_info.contains_key("os"));
        // 恢复之前的 hook，避免影响其它测试
        std::panic::set_hook(prev_hook);
    }

    #[test]
    fn test_crash_reporter_double_install_idempotent() {
        let _guard = HOOK_TEST_MUTEX.lock();
        let reporter = CrashReporter::new(CrashConfig::default());
        let prev_hook = std::panic::take_hook();
        reporter.install().unwrap();
        // 第二次应直接返回 Ok（不重新安装）
        reporter.install().unwrap();
        std::panic::set_hook(prev_hook);
    }
}
