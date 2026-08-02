//! 邮件同步 (Email Sync, IMAP)
//!
//! 通过 IMAP 协议拉取邮件并捕获为 Aurora 文档：
//! - 邮件 `subject` → 文档 `title`，`body` → 文档 `content`。
//! - 附件元数据提取 ([`EmailAttachment`])，写入素材库 (asset library)。
//! - [`EmailFilter`] 过滤规则：发件人 / 主题 / 是否含附件 / 大小区间。
//!
//! # 实现说明
//! [`ImapConnector`] 以 `Vec<EmailMessage>` 模拟 IMAP mailbox，
//! 真实实现替换 `fetch_messages` 为 IMAP IDLE / FETCH 命令即可，公开 API 不变。

use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use super::{ConnectorState, SyncConnector, SyncSession};

/// IMAP 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImapConfig {
    /// IMAP 服务器主机。
    pub host: String,
    /// 端口 (通常 993 for IMAPS)。
    pub port: u16,
    /// 用户名。
    pub username: String,
    /// 密码 / OAuth token。
    pub password: String,
    /// 是否启用 SSL。
    pub use_ssl: bool,
    /// 轮询间隔 (秒)。
    pub poll_interval_secs: u64,
    /// 拉取的邮箱名 (如 INBOX)。
    pub mailbox: String,
}

impl Default for ImapConfig {
    fn default() -> Self {
        Self {
            host: "imap.aurora.example".to_string(),
            port: 993,
            username: "aurora".to_string(),
            password: String::new(),
            use_ssl: true,
            poll_interval_secs: 60,
            mailbox: "INBOX".to_string(),
        }
    }
}

/// 邮件附件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAttachment {
    /// 文件名。
    pub filename: String,
    /// MIME 类型 (如 `application/pdf`)。
    pub content_type: String,
    /// 字节大小。
    pub size: usize,
    /// 原始字节 (mock；真实实现通常落盘或存素材库)。
    pub data: Vec<u8>,
}

impl EmailAttachment {
    pub fn new(filename: impl Into<String>, content_type: impl Into<String>, data: Vec<u8>) -> Self {
        let size = data.len();
        Self {
            filename: filename.into(),
            content_type: content_type.into(),
            size,
            data,
        }
    }
}

/// 邮件消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    /// `Message-ID` 头。
    pub message_id: String,
    /// `Subject`。
    pub subject: String,
    /// `From` (原始字符串)。
    pub from: String,
    /// `To` (分号分隔)。
    pub to: Vec<String>,
    /// `Date`。
    pub date: chrono::DateTime<chrono::Utc>,
    /// 正文 (纯文本)。
    pub body: String,
    /// 附件。
    pub attachments: Vec<EmailAttachment>,
    /// IMAP 标志 (如 `\Seen`, `\Flagged`)。
    pub flags: Vec<String>,
}

impl EmailMessage {
    /// 创建新邮件。
    pub fn new(
        message_id: impl Into<String>,
        subject: impl Into<String>,
        from: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            message_id: message_id.into(),
            subject: subject.into(),
            from: from.into(),
            to: Vec::new(),
            date: chrono::Utc::now(),
            body: body.into(),
            attachments: Vec::new(),
            flags: Vec::new(),
        }
    }

    pub fn with_to(mut self, to: Vec<String>) -> Self {
        self.to = to;
        self
    }

    pub fn with_attachment(mut self, att: EmailAttachment) -> Self {
        self.attachments.push(att);
        self
    }

    pub fn with_flags(mut self, flags: Vec<String>) -> Self {
        self.flags = flags;
        self
    }

    /// 是否包含附件。
    pub fn has_attachments(&self) -> bool {
        !self.attachments.is_empty()
    }

    /// 邮件总大小 (正文 + 附件，UTF-8 字节估算)。
    pub fn size_bytes(&self) -> usize {
        let body = self.body.len();
        let atts: usize = self.attachments.iter().map(|a| a.size).sum();
        body + atts
    }

    /// 转换为 Aurora 文档 (邮件捕获)。
    ///
    /// - `subject` → `title`
    /// - `body` → `content`
    /// - `message_id` → `source_message_id`
    /// - 附件 → 元数据列表 (data 不进入文档正文)
    pub fn to_document(&self) -> EmailDocument {
        EmailDocument {
            title: self.subject.clone(),
            content: self.body.clone(),
            source_message_id: self.message_id.clone(),
            from: self.from.clone(),
            date: self.date,
            captured_at: chrono::Utc::now(),
            attachments: self
                .attachments
                .iter()
                .map(|a| AttachmentMeta {
                    filename: a.filename.clone(),
                    content_type: a.content_type.clone(),
                    size: a.size,
                })
                .collect(),
        }
    }
}

/// 附件元数据 (文档中仅保留元信息，原始字节进素材库)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentMeta {
    pub filename: String,
    pub content_type: String,
    pub size: usize,
}

/// 邮件捕获后的文档。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailDocument {
    /// 文档标题 (来自邮件 subject)。
    pub title: String,
    /// 文档正文 (来自邮件 body)。
    pub content: String,
    /// 来源邮件 Message-ID。
    pub source_message_id: String,
    /// 发件人。
    pub from: String,
    /// 邮件日期。
    pub date: chrono::DateTime<chrono::Utc>,
    /// 捕获时间。
    pub captured_at: chrono::DateTime<chrono::Utc>,
    /// 附件元数据 (供素材库索引)。
    pub attachments: Vec<AttachmentMeta>,
}

impl EmailDocument {
    /// 附件数量。
    pub fn attachment_count(&self) -> usize {
        self.attachments.len()
    }

    /// 附件总大小 (字节)。
    pub fn attachments_total_size(&self) -> usize {
        self.attachments.iter().map(|a| a.size).sum()
    }
}

/// 邮件过滤规则。
///
/// 所有条件以 AND 组合：未设置 (None / 0) 的字段视为不限制。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmailFilter {
    /// 发件人包含 (大小写不敏感)。
    pub from_contains: Option<String>,
    /// 主题包含 (大小写不敏感)。
    pub subject_contains: Option<String>,
    /// 必须包含附件。
    pub has_attachments: Option<bool>,
    /// 最小字节数 (含)。
    pub min_size: Option<usize>,
    /// 最大字节数 (含)。
    pub max_size: Option<usize>,
}

impl EmailFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_contains(mut self, s: impl Into<String>) -> Self {
        self.from_contains = Some(s.into());
        self
    }

    pub fn subject_contains(mut self, s: impl Into<String>) -> Self {
        self.subject_contains = Some(s.into());
        self
    }

    pub fn has_attachments(mut self, b: bool) -> Self {
        self.has_attachments = Some(b);
        self
    }

    pub fn min_size(mut self, n: usize) -> Self {
        self.min_size = Some(n);
        self
    }

    pub fn max_size(mut self, n: usize) -> Self {
        self.max_size = Some(n);
        self
    }

    /// 判断邮件是否匹配过滤规则 (AND 语义)。
    pub fn matches(&self, msg: &EmailMessage) -> bool {
        if let Some(s) = &self.from_contains {
            if !msg.from.to_lowercase().contains(&s.to_lowercase()) {
                return false;
            }
        }
        if let Some(s) = &self.subject_contains {
            if !msg.subject.to_lowercase().contains(&s.to_lowercase()) {
                return false;
            }
        }
        if let Some(need_att) = self.has_attachments {
            if msg.has_attachments() != need_att {
                return false;
            }
        }
        let size = msg.size_bytes();
        if let Some(min) = self.min_size {
            if size < min {
                return false;
            }
        }
        if let Some(max) = self.max_size {
            if size > max {
                return false;
            }
        }
        true
    }
}

/// IMAP 连接器 (mock IMAP mailbox)。
pub struct ImapConnector {
    name: String,
    config: ImapConfig,
    state: Arc<RwLock<ConnectorState>>,
    /// 模拟服务端邮箱：按到达顺序存储。
    mailbox: Arc<RwLock<Vec<EmailMessage>>>,
    /// 已拉取的 Message-ID 集合 (用于增量)。
    seen: Arc<RwLock<std::collections::HashSet<String>>>,
}

impl ImapConnector {
    pub fn new(name: impl Into<String>, config: ImapConfig) -> Self {
        Self {
            name: name.into(),
            config,
            state: Arc::new(RwLock::new(ConnectorState::Disconnected)),
            mailbox: Arc::new(RwLock::new(Vec::new())),
            seen: Arc::new(RwLock::new(std::collections::HashSet::new())),
        }
    }

    pub fn config(&self) -> &ImapConfig {
        &self.config
    }

    /// 模拟服务端投递一封邮件到 mailbox。
    pub fn deliver(&self, msg: EmailMessage) {
        self.mailbox.write().push(msg);
    }

    /// 服务端邮件总数。
    pub fn mailbox_count(&self) -> usize {
        self.mailbox.read().len()
    }

    /// 拉取自上次以来未读邮件 (增量)。
    pub fn fetch_new(&self) -> Vec<EmailMessage> {
        let mailbox = self.mailbox.read();
        let mut seen = self.seen.write();
        let mut new_msgs = Vec::new();
        for msg in mailbox.iter() {
            if !seen.contains(&msg.message_id) {
                seen.insert(msg.message_id.clone());
                new_msgs.push(msg.clone());
            }
        }
        debug!("imap fetch_new: {} new messages", new_msgs.len());
        new_msgs
    }

    /// 拉取全部邮件 (重置 seen)。
    pub fn fetch_all(&self) -> Vec<EmailMessage> {
        let mailbox = self.mailbox.read();
        let mut seen = self.seen.write();
        seen.clear();
        for msg in mailbox.iter() {
            seen.insert(msg.message_id.clone());
        }
        mailbox.clone()
    }

    /// 标记某邮件为已读 (添加 `\Seen` flag)。
    pub fn mark_seen(&self, message_id: &str) -> bool {
        let mut mailbox = self.mailbox.write();
        for msg in mailbox.iter_mut() {
            if msg.message_id == message_id {
                if !msg.flags.iter().any(|f| f == "\\Seen") {
                    msg.flags.push("\\Seen".to_string());
                }
                return true;
            }
        }
        false
    }
}

impl SyncConnector for ImapConnector {
    fn name(&self) -> &str {
        &self.name
    }
    fn provider(&self) -> &str {
        "imap"
    }
    fn connect(&self) -> crate::Result<()> {
        *self.state.write() = ConnectorState::Connecting;
        if self.config.host.is_empty() {
            *self.state.write() = ConnectorState::Error("empty host".into());
            return Err(crate::Error::ExternalSync("empty imap host".into()));
        }
        *self.state.write() = ConnectorState::Connected;
        info!("imap connected: {}", self.name);
        Ok(())
    }
    fn disconnect(&self) -> crate::Result<()> {
        *self.state.write() = ConnectorState::Disconnected;
        info!("imap disconnected: {}", self.name);
        Ok(())
    }
    fn sync(&self) -> crate::Result<SyncSession> {
        if !self.state.read().is_connected() {
            return Err(crate::Error::ExternalSync(format!(
                "connector not connected: {}",
                self.name
            )));
        }
        let new_msgs = self.fetch_new();
        let mut session = SyncSession::new(self.name.clone(), "imap");
        session.finish(new_msgs.len(), 0);
        Ok(session)
    }
    fn state(&self) -> ConnectorState {
        self.state.read().clone()
    }
}

/// 邮件同步引擎。
///
/// 拉取 → 过滤 → 转换为文档 → 提取附件到素材库。
pub struct EmailSync {
    connector: Arc<ImapConnector>,
    filter: Arc<RwLock<EmailFilter>>,
    /// 已捕获为文档的邮件。
    documents: Arc<RwLock<Vec<EmailDocument>>>,
    /// 已提取到素材库的附件 (filename + content_type + size)。
    assets: Arc<RwLock<Vec<AttachmentMeta>>>,
}

impl EmailSync {
    pub fn new(connector: Arc<ImapConnector>) -> Self {
        Self {
            connector,
            filter: Arc::new(RwLock::new(EmailFilter::new())),
            documents: Arc::new(RwLock::new(Vec::new())),
            assets: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 设置过滤规则。
    pub fn set_filter(&self, filter: EmailFilter) {
        *self.filter.write() = filter;
    }

    /// 当前已捕获文档数。
    pub fn document_count(&self) -> usize {
        self.documents.read().len()
    }

    /// 当前已提取附件数。
    pub fn asset_count(&self) -> usize {
        self.assets.read().len()
    }

    /// 已捕获文档列表。
    pub fn documents(&self) -> Vec<EmailDocument> {
        self.documents.read().clone()
    }

    /// 已提取附件列表。
    pub fn assets(&self) -> Vec<AttachmentMeta> {
        self.assets.read().clone()
    }

    /// 拉取新邮件，按过滤规则筛选，转换为文档并提取附件。
    ///
    /// 返回 (捕获文档数, 提取附件数)。
    pub fn sync(&self) -> crate::Result<(usize, usize)> {
        let new_msgs = self.connector.fetch_new();
        let filter = self.filter.read().clone();
        let mut docs_added = 0;
        let mut atts_added = 0;
        for msg in &new_msgs {
            if !filter.matches(msg) {
                debug!("imap filter rejected: {}", msg.message_id);
                continue;
            }
            // 转换为文档
            let doc = msg.to_document();
            // 提取附件到素材库
            for att in &msg.attachments {
                self.assets.write().push(AttachmentMeta {
                    filename: att.filename.clone(),
                    content_type: att.content_type.clone(),
                    size: att.size,
                });
                atts_added += 1;
            }
            self.documents.write().push(doc);
            docs_added += 1;
            // 标记已读
            self.connector.mark_seen(&msg.message_id);
        }
        if docs_added > 0 || atts_added > 0 {
            info!(
                "email sync: captured={} docs, extracted={} attachments",
                docs_added, atts_added
            );
        }
        Ok((docs_added, atts_added))
    }

    /// 按发件人搜索已捕获文档。
    pub fn find_by_from(&self, from_contains: &str) -> Vec<EmailDocument> {
        let lower = from_contains.to_lowercase();
        self.documents
            .read()
            .iter()
            .filter(|d| d.from.to_lowercase().contains(&lower))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_connector() -> ImapConnector {
        ImapConnector::new("mail", ImapConfig::default())
    }

    fn sample_msg(id: &str, subject: &str, from: &str) -> EmailMessage {
        EmailMessage::new(id, subject, from, "Hello world")
    }

    #[test]
    fn test_imap_config_default() {
        let cfg = ImapConfig::default();
        assert!(cfg.use_ssl);
        assert_eq!(cfg.port, 993);
        assert!(!cfg.host.is_empty());
        assert_eq!(cfg.mailbox, "INBOX");
    }

    #[test]
    fn test_email_attachment_size() {
        let att = EmailAttachment::new("a.pdf", "application/pdf", vec![0u8; 100]);
        assert_eq!(att.size, 100);
        assert_eq!(att.filename, "a.pdf");
        assert_eq!(att.content_type, "application/pdf");
    }

    #[test]
    fn test_email_message_has_attachments_and_size() {
        let msg = EmailMessage::new("m1", "Hello", "a@b.com", "same body");
        assert!(!msg.has_attachments());
        let msg2 = EmailMessage::new("m2", "With att", "a@b.com", "same body")
            .with_attachment(EmailAttachment::new("x.txt", "text/plain", vec![1, 2, 3]));
        assert!(msg2.has_attachments());
        assert!(msg2.size_bytes() > msg.size_bytes());
    }

    #[test]
    fn test_email_to_document_mapping() {
        let msg = EmailMessage::new("mid-1", "Report Q1", "boss@corp.com", "Q1 numbers")
            .with_to(vec!["me@aurora.com".to_string()])
            .with_attachment(EmailAttachment::new("q1.pdf", "application/pdf", vec![0u8; 50]));
        let doc = msg.to_document();
        assert_eq!(doc.title, "Report Q1");
        assert_eq!(doc.content, "Q1 numbers");
        assert_eq!(doc.source_message_id, "mid-1");
        assert_eq!(doc.from, "boss@corp.com");
        assert_eq!(doc.attachment_count(), 1);
        assert_eq!(doc.attachments_total_size(), 50);
    }

    #[test]
    fn test_email_to_document_empty_attachments() {
        let msg = sample_msg("m1", "Hi", "a@b.com");
        let doc = msg.to_document();
        assert_eq!(doc.attachment_count(), 0);
        assert_eq!(doc.attachments_total_size(), 0);
    }

    #[test]
    fn test_filter_from_contains_case_insensitive() {
        let filter = EmailFilter::new().from_contains("CORP");
        let msg = sample_msg("m1", "Hi", "boss@corp.com");
        assert!(filter.matches(&msg));
        let msg2 = sample_msg("m2", "Hi", "someone@other.com");
        assert!(!filter.matches(&msg2));
    }

    #[test]
    fn test_filter_subject_contains() {
        let filter = EmailFilter::new().subject_contains("invoice");
        assert!(filter.matches(&sample_msg("m1", "Q1 Invoice", "a@b.com")));
        assert!(!filter.matches(&sample_msg("m2", "Hello", "a@b.com")));
    }

    #[test]
    fn test_filter_has_attachments() {
        let with_att = EmailMessage::new("m1", "A", "a@b.com", "b")
            .with_attachment(EmailAttachment::new("x", "text/plain", vec![1]));
        let no_att = sample_msg("m2", "A", "a@b.com");
        let filter = EmailFilter::new().has_attachments(true);
        assert!(filter.matches(&with_att));
        assert!(!filter.matches(&no_att));
    }

    #[test]
    fn test_filter_size_range() {
        let big = EmailMessage::new("m1", "A", "a@b.com", "x".repeat(500));
        let small = sample_msg("m2", "A", "a@b.com"); // ~11 bytes body
        let filter = EmailFilter::new().min_size(100).max_size(1000);
        assert!(filter.matches(&big));
        assert!(!filter.matches(&small));
    }

    #[test]
    fn test_filter_combined_and_semantics() {
        // 同时要求 from 含 corp 且 subject 含 invoice
        let filter = EmailFilter::new()
            .from_contains("corp")
            .subject_contains("invoice");
        let good = sample_msg("m1", "Invoice due", "boss@corp.com");
        let bad_from = sample_msg("m2", "Invoice due", "x@other.com");
        let bad_subj = sample_msg("m3", "Hello", "boss@corp.com");
        assert!(filter.matches(&good));
        assert!(!filter.matches(&bad_from));
        assert!(!filter.matches(&bad_subj));
    }

    #[test]
    fn test_connector_deliver_and_fetch_new() {
        let conn = make_connector();
        conn.deliver(sample_msg("m1", "A", "a@b.com"));
        conn.deliver(sample_msg("m2", "B", "a@b.com"));
        let new = conn.fetch_new();
        assert_eq!(new.len(), 2);
        // 再次 fetch_new 应为空 (已 seen)
        let new2 = conn.fetch_new();
        assert!(new2.is_empty());
    }

    #[test]
    fn test_connector_fetch_all_resets() {
        let conn = make_connector();
        conn.deliver(sample_msg("m1", "A", "a@b.com"));
        conn.fetch_new(); // mark seen
        let all = conn.fetch_all();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn test_connector_mark_seen() {
        let conn = make_connector();
        conn.deliver(sample_msg("m1", "A", "a@b.com"));
        assert!(conn.mark_seen("m1"));
        let msgs = conn.fetch_all();
        assert!(msgs[0].flags.iter().any(|f| f == "\\Seen"));
        assert!(!conn.mark_seen("nonexistent"));
    }

    #[test]
    fn test_connector_connect_disconnect_sync() {
        let conn = make_connector();
        assert_eq!(conn.state(), ConnectorState::Disconnected);
        conn.connect().unwrap();
        assert_eq!(conn.state(), ConnectorState::Connected);
        conn.deliver(sample_msg("m1", "A", "a@b.com"));
        let session = conn.sync().unwrap();
        assert_eq!(session.items_synced, 1);
        // 再次 sync 应为 0 (无新邮件)
        let s2 = conn.sync().unwrap();
        assert_eq!(s2.items_synced, 0);
        conn.disconnect().unwrap();
        assert!(conn.sync().is_err()); // 断开后报错
    }

    #[test]
    fn test_connector_connect_empty_host_errors() {
        let mut cfg = ImapConfig::default();
        cfg.host = String::new();
        let conn = ImapConnector::new("mail", cfg);
        assert!(conn.connect().is_err());
        assert!(conn.state().is_error());
    }

    #[test]
    fn test_email_sync_captures_documents_and_assets() {
        let conn = Arc::new(make_connector());
        conn.connect().unwrap();
        let sync = EmailSync::new(conn.clone());
        conn.deliver(
            EmailMessage::new("m1", "Report", "boss@corp.com", "see attached")
                .with_attachment(EmailAttachment::new("r.pdf", "application/pdf", vec![0u8; 10])),
        );
        conn.deliver(sample_msg("m2", "Spam", "spam@bad.com"));
        let (docs, atts) = sync.sync().unwrap();
        assert_eq!(docs, 2);
        assert_eq!(atts, 1);
        assert_eq!(sync.document_count(), 2);
        assert_eq!(sync.asset_count(), 1);
    }

    #[test]
    fn test_email_sync_applies_filter() {
        let conn = Arc::new(make_connector());
        conn.connect().unwrap();
        let sync = EmailSync::new(conn.clone());
        sync.set_filter(EmailFilter::new().from_contains("corp"));
        conn.deliver(sample_msg("m1", "A", "boss@corp.com"));
        conn.deliver(sample_msg("m2", "B", "spam@bad.com"));
        let (docs, _) = sync.sync().unwrap();
        assert_eq!(docs, 1); // 只有 corp 邮件被捕获
        assert_eq!(sync.document_count(), 1);
        assert_eq!(sync.documents()[0].from, "boss@corp.com");
    }

    #[test]
    fn test_email_sync_find_by_from() {
        let conn = Arc::new(make_connector());
        conn.connect().unwrap();
        let sync = EmailSync::new(conn.clone());
        conn.deliver(sample_msg("m1", "A", "boss@corp.com"));
        conn.deliver(sample_msg("m2", "B", "spam@bad.com"));
        conn.deliver(sample_msg("m3", "C", "ceo@corp.com"));
        sync.sync().unwrap();
        let corp_docs = sync.find_by_from("corp");
        assert_eq!(corp_docs.len(), 2);
    }

    #[test]
    fn test_email_sync_incremental() {
        let conn = Arc::new(make_connector());
        conn.connect().unwrap();
        let sync = EmailSync::new(conn.clone());
        conn.deliver(sample_msg("m1", "A", "a@b.com"));
        let (d1, _) = sync.sync().unwrap();
        assert_eq!(d1, 1);
        // 第二轮无新邮件
        let (d2, _) = sync.sync().unwrap();
        assert_eq!(d2, 0);
        // 新邮件到达
        conn.deliver(sample_msg("m2", "B", "a@b.com"));
        let (d3, _) = sync.sync().unwrap();
        assert_eq!(d3, 1);
        assert_eq!(sync.document_count(), 2);
    }
}
