//! 捕获引擎 (Capture Engine)
//!
//! 负责从多种外部来源捕获知识素材，经过解析、富化、归一化后持久化存储。
//! 采用管道架构（Source → Parser → Enricher → Normalizer → Storage），
//! 并内置 SimHash + URL 指纹双重去重机制。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use tracing::{debug, error, info, warn};

use crate::Error;

// ==================== 数据模型 ====================

/// 捕获来源类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CaptureSourceType {
    /// 浏览器扩展
    BrowserExtension,
    /// 系统分享
    SystemShare,
    /// 邮件 IMAP
    EmailImap,
    /// API Webhook
    ApiWebhook,
}

/// 捕获项的原始形态，由 Source 产生
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureItem {
    /// 唯一 ID
    pub id: String,
    /// 来源类型
    pub source_type: CaptureSourceType,
    /// 原始内容字节
    pub raw_content: Vec<u8>,
    /// 附加元数据
    pub metadata: HashMap<String, String>,
    /// 捕获时间戳 (毫秒)
    pub captured_at: u64,
}

impl CaptureItem {
    /// 创建一个新的捕获项，自动生成 ID 与当前时间戳
    pub fn new(source_type: CaptureSourceType, raw_content: Vec<u8>) -> Self {
        let captured_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            source_type,
            raw_content,
            metadata: HashMap::new(),
            captured_at,
        }
    }
}

/// 解析后的结构化数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedItem {
    /// 唯一 ID（继承自 CaptureItem）
    pub id: String,
    /// 来源类型
    pub source_type: CaptureSourceType,
    /// 标题
    pub title: Option<String>,
    /// 正文内容
    pub content: String,
    /// 来源 URL
    pub url: Option<String>,
    /// MIME 类型
    pub mime_type: String,
    /// 附加元数据
    pub metadata: HashMap<String, String>,
    /// 捕获时间戳
    pub captured_at: u64,
}

/// 富化后的数据（添加标签、摘要等）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedItem {
    /// 唯一 ID
    pub id: String,
    /// 来源类型
    pub source_type: CaptureSourceType,
    /// 标题
    pub title: Option<String>,
    /// 正文内容
    pub content: String,
    /// 来源 URL
    pub url: Option<String>,
    /// MIME 类型
    pub mime_type: String,
    /// 自动提取的标签
    pub tags: Vec<String>,
    /// 内容摘要
    pub summary: Option<String>,
    /// 附加元数据
    pub metadata: HashMap<String, String>,
    /// 捕获时间戳
    pub captured_at: u64,
}

/// 归一化后的最终数据，准备入库
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedItem {
    /// 唯一 ID
    pub id: String,
    /// 来源类型
    pub source_type: CaptureSourceType,
    /// 标题（已规范化，非空）
    pub title: String,
    /// 正文内容（已 trim）
    pub content: String,
    /// 来源 URL
    pub url: Option<String>,
    /// MIME 类型
    pub mime_type: String,
    /// 标签列表
    pub tags: Vec<String>,
    /// 摘要（非空）
    pub summary: String,
    /// 附加元数据
    pub metadata: HashMap<String, String>,
    /// 捕获时间戳
    pub captured_at: u64,
    /// SimHash 值（64 位）
    pub simhash: u64,
    /// URL 指纹（若存在 URL）
    pub url_fingerprint: Option<String>,
}

// ==================== 管道 Trait ====================

/// 来源插件接口
///
/// 所有外部数据来源均需实现此接口，由 CaptureEngine 统一轮询。
pub trait Source: Send + Sync {
    /// 轮询获取当前可用的原始捕获项
    fn poll(&self) -> Result<Vec<CaptureItem>, Error>;
    /// 返回该来源的类型标识
    fn source_type(&self) -> CaptureSourceType;
}

/// 解析器接口
///
/// 将原始字节转换为结构化文本与元数据。
pub trait Parser: Send + Sync {
    fn parse(&self, item: CaptureItem) -> Result<ParsedItem, Error>;
}

/// 富化器接口
///
/// 在解析结果基础上提取标签、生成摘要等。
pub trait Enricher: Send + Sync {
    fn enrich(&self, item: ParsedItem) -> Result<EnrichedItem, Error>;
}

/// 归一化器接口
///
/// 统一字段格式、计算 SimHash 与 URL 指纹。
pub trait Normalizer: Send + Sync {
    fn normalize(&self, item: EnrichedItem) -> Result<NormalizedItem, Error>;
}

/// 捕获存储接口
///
/// 负责将归一化后的数据持久化。
pub trait CaptureStorage: Send + Sync {
    fn store(&self, item: &NormalizedItem) -> Result<(), Error>;
}

// ==================== 去重策略 ====================

/// 计算两个 64 位 SimHash 的海明距离
fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// 对文本进行简单分词（按非字母数字字符分割）
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty() && s.len() > 1)
        .map(|s| s.to_string())
        .collect()
}

/// 计算单个 token 的 64 位哈希（取 Sha3-256 的低 64 位）
fn hash_token(token: &str) -> u64 {
    let mut hasher = Sha3_256::new();
    hasher.update(token.as_bytes());
    let result = hasher.finalize();
    let mut hash: u64 = 0;
    for i in 0..8.min(result.len()) {
        hash |= (result[i] as u64) << (i * 8);
    }
    hash
}

/// 计算文本的 SimHash（64 位）
///
/// 采用特征词加权累加方式，适合中短文本的近似去重。
pub fn compute_simhash(text: &str) -> u64 {
    let tokens = tokenize(text);
    let mut weights: HashMap<String, usize> = HashMap::new();
    for token in tokens {
        *weights.entry(token).or_insert(0) += 1;
    }

    let mut vec = [0i32; 64];
    for (token, weight) in weights {
        let hash = hash_token(&token);
        for i in 0..64 {
            if (hash >> i) & 1 == 1 {
                vec[i] += weight as i32;
            } else {
                vec[i] -= weight as i32;
            }
        }
    }

    let mut simhash: u64 = 0;
    for i in 0..64 {
        if vec[i] > 0 {
            simhash |= 1 << i;
        }
    }
    simhash
}

/// 将字节数组转换为小写十六进制字符串
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// 规范化 URL
///
/// 去掉 fragment、移除常见跟踪参数、统一小写。
pub fn normalize_url(url: &str) -> String {
    let mut url = url.trim().to_string();

    // 去掉 fragment
    if let Some(pos) = url.find('#') {
        url.truncate(pos);
    }

    // 移除常见跟踪参数
    if let Some(query_pos) = url.find('?') {
        let base = &url[..query_pos];
        let query = &url[query_pos + 1..];
        let params: Vec<&str> = query.split('&').collect();
        let filtered: Vec<&str> = params
            .into_iter()
            .filter(|p| {
                let key = p.split('=').next().unwrap_or("");
                !key.starts_with("utm_")
                    && !key.starts_with("fbclid")
                    && key != "ref"
                    && !key.starts_with("sid")
            })
            .collect();
        if filtered.is_empty() {
            url = base.to_string();
        } else {
            // 稳定排序参数，保证同一 URL 不同参数顺序得到相同指纹
            let mut filtered = filtered;
            filtered.sort_unstable();
            url = format!("{}?{}", base, filtered.join("&"));
        }
    }

    url.to_lowercase()
}

/// 计算 URL 指纹（SHA3-256 前 16 字节 hex）
pub fn compute_url_fingerprint(url: &str) -> String {
    let normalized = normalize_url(url);
    let mut hasher = Sha3_256::new();
    hasher.update(normalized.as_bytes());
    let result = hasher.finalize();
    bytes_to_hex(&result[..16])
}

/// 去重器，基于 SimHash 与 URL 指纹实现双重去重
///
/// - URL 指纹：精确匹配，用于完全相同的链接
/// - SimHash：海明距离阈值匹配，用于内容高度相似的去重
pub struct Deduplicator {
    simhash_index: Arc<RwLock<Vec<(u64, String)>>>,
    url_fingerprints: Arc<RwLock<HashMap<String, String>>>,
    threshold: u32,
}

impl Deduplicator {
    /// 创建去重器，指定 SimHash 海明距离阈值（通常 3~5）
    pub fn new(threshold: u32) -> Self {
        Self {
            simhash_index: Arc::new(RwLock::new(Vec::new())),
            url_fingerprints: Arc::new(RwLock::new(HashMap::new())),
            threshold,
        }
    }

    /// 创建去重器并预分配容量
    pub fn with_capacity(threshold: u32, capacity: usize) -> Self {
        Self {
            simhash_index: Arc::new(RwLock::new(Vec::with_capacity(capacity))),
            url_fingerprints: Arc::new(RwLock::new(HashMap::with_capacity(capacity))),
            threshold,
        }
    }

    /// 检查给定指纹是否已重复
    pub fn is_duplicate(&self, simhash: u64, url_fingerprint: Option<&str>) -> bool {
        // 1. URL 指纹去重（精确匹配）
        if let Some(fp) = url_fingerprint {
            if self.url_fingerprints.read().contains_key(fp) {
                debug!(url_fingerprint = %fp, "URL fingerprint duplicate detected");
                return true;
            }
        }

        // 2. SimHash 内容相似去重
        let index = self.simhash_index.read();
        for &(existing_hash, _) in index.iter() {
            if hamming_distance(simhash, existing_hash) <= self.threshold {
                debug!(simhash, existing_hash, "SimHash duplicate detected");
                return true;
            }
        }

        false
    }

    /// 记录一个新的指纹
    pub fn record(&self, simhash: u64, url_fingerprint: Option<String>, item_id: String) {
        self.simhash_index.write().push((simhash, item_id.clone()));
        if let Some(fp) = url_fingerprint {
            self.url_fingerprints.write().insert(fp, item_id);
        }
    }

    /// 从 NormalizedItem 记录指纹
    pub fn record_item(&self, item: &NormalizedItem) {
        self.record(item.simhash, item.url_fingerprint.clone(), item.id.clone());
    }
}

// ==================== 默认实现 ====================

/// 默认解析器
///
/// 将原始内容按 UTF-8 解码，提取 metadata 中的 title、url、mime_type。
pub struct DefaultParser;

impl DefaultParser {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultParser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser for DefaultParser {
    fn parse(&self, item: CaptureItem) -> Result<ParsedItem, Error> {
        let content = String::from_utf8_lossy(&item.raw_content).to_string();
        let mime_type = item
            .metadata
            .get("mime_type")
            .cloned()
            .unwrap_or_else(|| "text/plain".to_string());

        let title = item.metadata.get("title").cloned();
        let url = item.metadata.get("url").cloned();

        Ok(ParsedItem {
            id: item.id,
            source_type: item.source_type,
            title,
            content,
            url,
            mime_type,
            metadata: item.metadata,
            captured_at: item.captured_at,
        })
    }
}

/// 默认富化器
///
/// 简单截取前 200 字符作为摘要，并根据 URL 域名推断标签。
pub struct DefaultEnricher;

impl DefaultEnricher {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultEnricher {
    fn default() -> Self {
        Self::new()
    }
}

impl Enricher for DefaultEnricher {
    fn enrich(&self, item: ParsedItem) -> Result<EnrichedItem, Error> {
        let summary = if item.content.len() > 200 {
            Some(format!("{}...", &item.content[..200]))
        } else {
            Some(item.content.clone())
        };

        let mut tags = Vec::new();
        if let Some(ref url) = item.url {
            let lower = url.to_lowercase();
            if lower.contains("github.com") {
                tags.push("code".to_string());
            } else if lower.contains("youtube.com")
                || lower.contains("bilibili.com")
                || lower.contains("vimeo.com")
            {
                tags.push("video".to_string());
            } else if lower.contains("arxiv.org")
                || lower.contains("scholar.google")
                || lower.contains("pubmed")
            {
                tags.push("paper".to_string());
            }
        }

        Ok(EnrichedItem {
            id: item.id,
            source_type: item.source_type,
            title: item.title,
            content: item.content,
            url: item.url,
            mime_type: item.mime_type,
            tags,
            summary,
            metadata: item.metadata,
            captured_at: item.captured_at,
        })
    }
}

/// 默认归一化器
///
/// 清理空白、补全缺失标题、计算 SimHash 与 URL 指纹。
pub struct DefaultNormalizer;

impl DefaultNormalizer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultNormalizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Normalizer for DefaultNormalizer {
    fn normalize(&self, item: EnrichedItem) -> Result<NormalizedItem, Error> {
        let title = item.title.unwrap_or_else(|| {
            item.content
                .lines()
                .next()
                .unwrap_or("Untitled")
                .to_string()
        });
        let title = title.trim().to_string();
        let title = if title.is_empty() {
            "Untitled".to_string()
        } else {
            title
        };

        let summary = item.summary.unwrap_or_default().trim().to_string();
        let summary = if summary.is_empty() {
            "(无摘要)".to_string()
        } else {
            summary
        };

        let content = item.content.trim().to_string();

        let simhash = compute_simhash(&content);
        let url_fingerprint = item.url.as_ref().map(|u| compute_url_fingerprint(u));

        Ok(NormalizedItem {
            id: item.id,
            source_type: item.source_type,
            title,
            content,
            url: item.url,
            mime_type: item.mime_type,
            tags: item.tags,
            summary,
            metadata: item.metadata,
            captured_at: item.captured_at,
            simhash,
            url_fingerprint,
        })
    }
}

/// 内存存储，用于测试与本地缓存场景
pub struct InMemoryCaptureStorage {
    items: Arc<Mutex<Vec<NormalizedItem>>>,
}

impl InMemoryCaptureStorage {
    pub fn new() -> Self {
        Self {
            items: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 获取当前存储的所有项目
    pub fn items(&self) -> Vec<NormalizedItem> {
        self.items.lock().clone()
    }

    /// 获取存储数量
    pub fn len(&self) -> usize {
        self.items.lock().len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.items.lock().is_empty()
    }
}

impl Default for InMemoryCaptureStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureStorage for InMemoryCaptureStorage {
    fn store(&self, item: &NormalizedItem) -> Result<(), Error> {
        self.items.lock().push(item.clone());
        Ok(())
    }
}

// ==================== 来源插件 ====================

/// 浏览器扩展来源
///
/// 通过 tokio mpsc channel 接收浏览器扩展推送的数据。
pub struct BrowserExtensionSource {
    receiver: Arc<Mutex<tokio::sync::mpsc::Receiver<CaptureItem>>>,
    sender: tokio::sync::mpsc::Sender<CaptureItem>,
}

impl BrowserExtensionSource {
    pub fn new(buffer: usize) -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(buffer);
        Self {
            receiver: Arc::new(Mutex::new(receiver)),
            sender,
        }
    }

    /// 获取发送端，供浏览器扩展服务调用
    pub fn sender(&self) -> tokio::sync::mpsc::Sender<CaptureItem> {
        self.sender.clone()
    }
}

impl Source for BrowserExtensionSource {
    fn poll(&self) -> Result<Vec<CaptureItem>, Error> {
        let mut items = Vec::new();
        let mut rx = self.receiver.lock();
        while let Ok(item) = rx.try_recv() {
            items.push(item);
            if items.len() >= 100 {
                break;
            }
        }
        Ok(items)
    }

    fn source_type(&self) -> CaptureSourceType {
        CaptureSourceType::BrowserExtension
    }
}

/// 系统分享来源
///
/// 通过共享 inbox 接收系统分享的数据。
pub struct SystemShareSource {
    inbox: Arc<Mutex<Vec<CaptureItem>>>,
}

impl SystemShareSource {
    pub fn new() -> Self {
        Self {
            inbox: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 提交一个捕获项
    pub fn submit(&self, item: CaptureItem) {
        self.inbox.lock().push(item);
    }

    /// 便捷方法：直接提交原始内容与元数据
    pub fn submit_raw(&self, raw_content: Vec<u8>, metadata: HashMap<String, String>) {
        let mut item = CaptureItem::new(CaptureSourceType::SystemShare, raw_content);
        item.metadata = metadata;
        self.submit(item);
    }
}

impl Default for SystemShareSource {
    fn default() -> Self {
        Self::new()
    }
}

impl Source for SystemShareSource {
    fn poll(&self) -> Result<Vec<CaptureItem>, Error> {
        let mut inbox = self.inbox.lock();
        if inbox.is_empty() {
            Ok(Vec::new())
        } else {
            let items = inbox.drain(..).collect();
            Ok(items)
        }
    }

    fn source_type(&self) -> CaptureSourceType {
        CaptureSourceType::SystemShare
    }
}

/// IMAP 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImapConfig {
    pub server: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub use_ssl: bool,
    pub folder: String,
}

/// 邮件 IMAP 来源
///
/// 实际 IMAP 网络连接将在后续任务中接入异步网络库；
/// 当前提供 mock 拉取能力与统一的 Source 接口。
pub struct EmailImapSource {
    config: ImapConfig,
    pending: Arc<Mutex<Vec<CaptureItem>>>,
}

impl EmailImapSource {
    pub fn new(config: ImapConfig) -> Self {
        Self {
            config,
            pending: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn config(&self) -> &ImapConfig {
        &self.config
    }

    /// 模拟拉取邮件，用于测试与接口验证
    pub fn fetch_mock(&self, count: usize) -> Result<Vec<CaptureItem>, Error> {
        let mut items = Vec::new();
        for i in 0..count {
            let content = format!(
                "Mock email content #{0}\nFrom: mock@example.com\nSubject: Test {0}",
                i
            );
            let mut metadata = HashMap::new();
            metadata.insert("subject".to_string(), format!("Test {}", i));
            metadata.insert("from".to_string(), "mock@example.com".to_string());
            metadata.insert("mime_type".to_string(), "text/plain".to_string());

            let mut item = CaptureItem::new(CaptureSourceType::EmailImap, content.into_bytes());
            item.metadata = metadata;
            items.push(item);
        }
        {
            let mut pending = self.pending.lock();
            pending.extend(items.clone());
        }
        Ok(items)
    }
}

impl Source for EmailImapSource {
    fn poll(&self) -> Result<Vec<CaptureItem>, Error> {
        let mut pending = self.pending.lock();
        if pending.is_empty() {
            Ok(Vec::new())
        } else {
            let items = pending.drain(..).collect();
            Ok(items)
        }
    }

    fn source_type(&self) -> CaptureSourceType {
        CaptureSourceType::EmailImap
    }
}

/// Webhook 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub endpoint: String,
    pub secret: Option<String>,
}

/// API Webhook 来源
///
/// 通过共享 inbox 接收外部 Webhook 推送的数据。
pub struct ApiWebhookSource {
    config: WebhookConfig,
    inbox: Arc<Mutex<Vec<CaptureItem>>>,
}

impl ApiWebhookSource {
    pub fn new(config: WebhookConfig) -> Self {
        Self {
            config,
            inbox: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn config(&self) -> &WebhookConfig {
        &self.config
    }

    /// 提交一个捕获项
    pub fn submit(&self, item: CaptureItem) {
        self.inbox.lock().push(item);
    }

    /// 便捷方法：直接提交 JSON payload
    pub fn submit_json(&self, payload: serde_json::Value) -> Result<(), Error> {
        let raw = serde_json::to_vec(&payload).map_err(Error::Serialization)?;
        let mut metadata = HashMap::new();
        metadata.insert("mime_type".to_string(), "application/json".to_string());
        let mut item = CaptureItem::new(CaptureSourceType::ApiWebhook, raw);
        item.metadata = metadata;
        self.submit(item);
        Ok(())
    }
}

impl Source for ApiWebhookSource {
    fn poll(&self) -> Result<Vec<CaptureItem>, Error> {
        let mut inbox = self.inbox.lock();
        if inbox.is_empty() {
            Ok(Vec::new())
        } else {
            let items = inbox.drain(..).collect();
            Ok(items)
        }
    }

    fn source_type(&self) -> CaptureSourceType {
        CaptureSourceType::ApiWebhook
    }
}

// ==================== 捕获引擎 ====================

/// 捕获引擎配置
#[derive(Debug, Clone)]
pub struct CaptureEngineConfig {
    /// SimHash 海明距离阈值（默认 3）
    pub simhash_threshold: u32,
    /// 轮询间隔（毫秒，默认 5000）
    pub poll_interval_ms: u64,
    /// 每轮单来源最大处理数（默认 100）
    pub max_batch_size: usize,
}

impl Default for CaptureEngineConfig {
    fn default() -> Self {
        Self {
            simhash_threshold: 3,
            poll_interval_ms: 5000,
            max_batch_size: 100,
        }
    }
}

/// 捕获引擎，编排 Source → Parser → Enricher → Normalizer → Storage 管道
///
/// 使用 `Arc<Self>` 启动后台异步任务，支持 graceful stop。
pub struct CaptureEngine {
    sources: Vec<Box<dyn Source>>,
    parser: Arc<dyn Parser>,
    enricher: Arc<dyn Enricher>,
    normalizer: Arc<dyn Normalizer>,
    storage: Arc<dyn CaptureStorage>,
    deduplicator: Deduplicator,
    config: CaptureEngineConfig,
    running: Arc<AtomicBool>,
}

impl CaptureEngine {
    pub fn new(
        parser: Arc<dyn Parser>,
        enricher: Arc<dyn Enricher>,
        normalizer: Arc<dyn Normalizer>,
        storage: Arc<dyn CaptureStorage>,
        config: CaptureEngineConfig,
    ) -> Self {
        let deduplicator = Deduplicator::new(config.simhash_threshold);
        Self {
            sources: Vec::new(),
            parser,
            enricher,
            normalizer,
            storage,
            deduplicator,
            config,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 添加一个来源插件
    pub fn add_source(&mut self, source: Box<dyn Source>) {
        self.sources.push(source);
    }

    /// 获取当前来源数量
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// 停止后台异步轮询
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        info!("CaptureEngine stop signal sent");
    }

    /// 执行单轮捕获与处理（同步）
    ///
    /// 依次轮询所有来源，将数据推过完整管道，并返回本轮成功存储的数量。
    pub fn run_once(&self) -> Result<usize, Error> {
        let mut total_stored = 0;

        for source in &self.sources {
            let items = match source.poll() {
                Ok(items) => items,
                Err(e) => {
                    error!(source_type = ?source.source_type(), error = %e, "Source poll failed");
                    continue;
                }
            };

            debug!(count = items.len(), source_type = ?source.source_type(), "Polled items");

            for item in items.into_iter().take(self.config.max_batch_size) {
                let parsed = match self.parser.parse(item) {
                    Ok(p) => p,
                    Err(e) => {
                        warn!(error = %e, "Parse failed");
                        continue;
                    }
                };

                let enriched = match self.enricher.enrich(parsed) {
                    Ok(e) => e,
                    Err(e) => {
                        warn!(error = %e, "Enrich failed");
                        continue;
                    }
                };

                let normalized = match self.normalizer.normalize(enriched) {
                    Ok(n) => n,
                    Err(e) => {
                        warn!(error = %e, "Normalize failed");
                        continue;
                    }
                };

                // 去重检查
                if self
                    .deduplicator
                    .is_duplicate(normalized.simhash, normalized.url_fingerprint.as_deref())
                {
                    info!(item_id = %normalized.id, "Duplicate item dropped");
                    continue;
                }

                // 存储
                if let Err(e) = self.storage.store(&normalized) {
                    error!(error = %e, "Storage failed");
                    continue;
                }

                // 记录去重指纹
                self.deduplicator.record_item(&normalized);

                total_stored += 1;
                info!(item_id = %normalized.id, title = %normalized.title, "Item captured");
            }
        }

        Ok(total_stored)
    }

    /// 启动后台异步轮询循环
    ///
    /// 返回 `JoinHandle`，调用方可通过 `engine.stop()` 请求停止。
    pub fn start_async(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        self.running.store(true, Ordering::Relaxed);
        let interval = tokio::time::Duration::from_millis(self.config.poll_interval_ms);
        let mut ticker = tokio::time::interval(interval);

        tokio::spawn(async move {
            info!("CaptureEngine async loop started");

            while self.running.load(Ordering::Relaxed) {
                ticker.tick().await;
                match self.run_once() {
                    Ok(count) => {
                        if count > 0 {
                            info!(count, "Capture batch completed");
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Capture run failed");
                    }
                }
            }

            info!("CaptureEngine async loop stopped");
        })
    }
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simhash_identical() {
        let text = "Hello world, this is a test for SimHash.";
        let h1 = compute_simhash(text);
        let h2 = compute_simhash(text);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_simhash_similar() {
        let t1 = "Rust is a systems programming language with fearless concurrency.";
        let t2 = "Rust is a systems programming language featuring fearless concurrency.";
        let h1 = compute_simhash(t1);
        let h2 = compute_simhash(t2);
        let dist = hamming_distance(h1, h2);
        eprintln!("SimHash distance: {}", dist);
        assert!(dist <= 10, "expected distance <= 10, got {}", dist);
    }

    #[test]
    fn test_simhash_different() {
        let t1 = "The quick brown fox jumps over the lazy dog.";
        let t2 = "Machine learning models require large amounts of training data.";
        let h1 = compute_simhash(t1);
        let h2 = compute_simhash(t2);
        assert!(hamming_distance(h1, h2) > 10);
    }

    #[test]
    fn test_normalize_url() {
        assert_eq!(
            normalize_url("https://example.com/page?utm_source=xxx"),
            "https://example.com/page"
        );
        assert_eq!(
            normalize_url("https://Example.com/Page#section"),
            "https://example.com/page"
        );
        let norm = normalize_url("https://example.com?b=2&a=1");
        assert!(norm.contains("a=1") && norm.contains("b=2"));
    }

    #[test]
    fn test_url_fingerprint_stability() {
        let fp1 = compute_url_fingerprint("https://example.com?utm_source=abc");
        let fp2 = compute_url_fingerprint("https://example.com?utm_medium=xyz");
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn test_deduplicator_url_fingerprint() {
        let dedup = Deduplicator::new(3);
        let fp = Some("abc123".to_string());
        assert!(!dedup.is_duplicate(0, fp.as_deref()));
        dedup.record(0, fp.clone(), "id-1".to_string());
        assert!(dedup.is_duplicate(0, fp.as_deref()));
    }

    #[test]
    fn test_deduplicator_simhash() {
        let dedup = Deduplicator::new(3);
        let h1 = compute_simhash("Capture engine test content A.");
        let h2 = compute_simhash("Capture engine test content A.");
        let h3 = compute_simhash("Totally different content about cats and dogs.");

        assert!(!dedup.is_duplicate(h1, None));
        dedup.record(h1, None, "id-1".to_string());
        assert!(dedup.is_duplicate(h2, None)); // identical/similar
        assert!(!dedup.is_duplicate(h3, None)); // different
    }

    #[test]
    fn test_default_parser() {
        let parser = DefaultParser::new();
        let mut meta = HashMap::new();
        meta.insert("title".to_string(), "My Title".to_string());
        meta.insert("url".to_string(), "https://example.com".to_string());
        let item = CaptureItem {
            id: "test-1".to_string(),
            source_type: CaptureSourceType::SystemShare,
            raw_content: b"Hello content".to_vec(),
            metadata: meta,
            captured_at: 0,
        };
        let parsed = parser.parse(item).unwrap();
        assert_eq!(parsed.title, Some("My Title".to_string()));
        assert_eq!(parsed.content, "Hello content");
        assert_eq!(parsed.url, Some("https://example.com".to_string()));
    }

    #[test]
    fn test_default_normalizer_computes_simhash() {
        let normalizer = DefaultNormalizer::new();
        let enriched = EnrichedItem {
            id: "id-1".to_string(),
            source_type: CaptureSourceType::BrowserExtension,
            title: Some("Title".to_string()),
            content: "Some content here.".to_string(),
            url: Some("https://example.com/page".to_string()),
            mime_type: "text/plain".to_string(),
            tags: vec![],
            summary: Some("Summary".to_string()),
            metadata: HashMap::new(),
            captured_at: 0,
        };
        let norm = normalizer.normalize(enriched).unwrap();
        assert!(!norm.title.is_empty());
        assert!(norm.simhash != 0 || norm.content.is_empty());
        assert!(norm.url_fingerprint.is_some());
    }

    #[test]
    fn test_system_share_source() {
        let source = SystemShareSource::new();
        assert!(source.poll().unwrap().is_empty());

        let mut meta = HashMap::new();
        meta.insert("title".to_string(), "Shared".to_string());
        source.submit_raw(b"shared data".to_vec(), meta);

        let items = source.poll().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].source_type, CaptureSourceType::SystemShare);
    }

    #[test]
    fn test_api_webhook_source() {
        let source = ApiWebhookSource::new(WebhookConfig {
            endpoint: "/hooks/v1".to_string(),
            secret: None,
        });
        source
            .submit_json(serde_json::json!({"title": "Hook", "body": "data"}))
            .unwrap();
        let items = source.poll().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].source_type, CaptureSourceType::ApiWebhook);
    }

    #[test]
    fn test_email_imap_mock() {
        let source = EmailImapSource::new(ImapConfig {
            server: "imap.example.com".to_string(),
            port: 993,
            username: "user".to_string(),
            password: "pass".to_string(),
            use_ssl: true,
            folder: "INBOX".to_string(),
        });
        source.fetch_mock(3).unwrap();
        let items = source.poll().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].source_type, CaptureSourceType::EmailImap);
    }

    #[test]
    fn test_capture_engine_pipeline() {
        let storage = Arc::new(InMemoryCaptureStorage::new());
        let mut engine = CaptureEngine::new(
            Arc::new(DefaultParser::new()),
            Arc::new(DefaultEnricher::new()),
            Arc::new(DefaultNormalizer::new()),
            storage.clone(),
            CaptureEngineConfig::default(),
        );

        let share = SystemShareSource::new();
        let mut meta = HashMap::new();
        meta.insert("title".to_string(), "Pipeline Test".to_string());
        share.submit_raw(b"Pipeline test content.".to_vec(), meta);
        engine.add_source(Box::new(share));

        let count = engine.run_once().unwrap();
        assert_eq!(count, 1);
        assert_eq!(storage.len(), 1);

        let items = storage.items();
        assert_eq!(items[0].title, "Pipeline Test");
    }

    #[test]
    fn test_capture_engine_dedup() {
        let storage = Arc::new(InMemoryCaptureStorage::new());
        let mut engine = CaptureEngine::new(
            Arc::new(DefaultParser::new()),
            Arc::new(DefaultEnricher::new()),
            Arc::new(DefaultNormalizer::new()),
            storage.clone(),
            CaptureEngineConfig::default(),
        );

        let share = SystemShareSource::new();
        let mut meta = HashMap::new();
        meta.insert("url".to_string(), "https://example.com/same".to_string());
        share.submit_raw(b"Same content.".to_vec(), meta.clone());
        share.submit_raw(b"Same content.".to_vec(), meta);
        engine.add_source(Box::new(share));

        let count = engine.run_once().unwrap();
        assert_eq!(count, 1); // 第二个被去重
        assert_eq!(storage.len(), 1);
    }

    #[tokio::test]
    async fn test_browser_extension_source_async() {
        let source = Arc::new(BrowserExtensionSource::new(16));
        let sender = source.sender();

        let item = CaptureItem::new(CaptureSourceType::BrowserExtension, b"ext data".to_vec());
        sender.send(item).await.unwrap();

        let polled = source.poll().unwrap();
        assert_eq!(polled.len(), 1);
        assert_eq!(polled[0].source_type, CaptureSourceType::BrowserExtension);
    }

    #[tokio::test]
    async fn test_capture_engine_start_stop() {
        let storage = Arc::new(InMemoryCaptureStorage::new());
        let mut engine = CaptureEngine::new(
            Arc::new(DefaultParser::new()),
            Arc::new(DefaultEnricher::new()),
            Arc::new(DefaultNormalizer::new()),
            storage.clone(),
            CaptureEngineConfig {
                simhash_threshold: 3,
                poll_interval_ms: 50,
                max_batch_size: 10,
            },
        );

        let share = SystemShareSource::new();
        share.submit_raw(b"async test".to_vec(), HashMap::new());
        engine.add_source(Box::new(share));

        let engine = Arc::new(engine);
        let handle = engine.clone().start_async();

        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
        engine.stop();

        let _ = tokio::time::timeout(tokio::time::Duration::from_secs(2), handle).await;
        assert!(!storage.is_empty());
    }
}
