//! 外部同步中心 (External Sync Hub)
//!
//! 统一管理 Aurora 与外部系统的双向同步，覆盖四大场景：
//! - [`calendar`]：CalDAV (RFC 4791) 日历同步，GTD 任务 ↔ VEVENT 双向映射。
//! - [`email`]：IMAP 邮件同步，邮件捕获为文档、附件提取到素材库。
//! - [`cloud_drive`]：WebDAV / Google Drive / Dropbox / OneDrive 云盘同步。
//! - [`webhook`]：本地 HTTP 接收 GitHub / Jira / Slack webhook，HMAC-SHA256 验签。
//!
//! # 连接器架构
//! 所有外部系统统一抽象为 [`SyncConnector`] trait，由 [`ConnectorRegistry`]
//! 集中注册、查询与生命周期管理。每个连接器维护自身的 [`ConnectorState`]，
//! 同步动作产生 [`SyncSession`] 报告。
//!
//! # 实现说明
//! 网络层均为内存 mock 实现 (mock CalDAV/IMAP/WebDAV/HTTP server)，
//! 公开 API 与真实实现保持一致，仅需替换内部传输即可接入生产环境。

pub mod calendar;
pub mod cloud_drive;
pub mod email;
pub mod webhook;

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// 重导出子模块公开类型，便于通过 `external::` 路径访问。
pub use calendar::{CalDavConfig, CalDavConnector, CalendarEvent, CalendarSync, Ctag, Etag};
pub use cloud_drive::{
    CloudDriveConnector, CloudDriveSync, DriveFile, DriveProvider, DropboxConnector,
    GoogleDriveConnector, OneDriveConnector, SelectiveSyncConfig, WebDavConnector,
};
pub use email::{
    EmailAttachment, EmailDocument, EmailFilter, EmailMessage, EmailSync, ImapConfig, ImapConnector,
};
pub use webhook::{HmacVerifier, WebhookConfig, WebhookEvent, WebhookReceiver, WebhookSource};

/// 连接器状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConnectorState {
    /// 已断开。
    Disconnected,
    /// 连接中 (握手 / 认证进行中)。
    Connecting,
    /// 已连接，可执行同步。
    Connected,
    /// 错误状态，附带错误信息。
    Error(String),
}

impl ConnectorState {
    /// 是否处于可同步状态。
    pub fn is_connected(&self) -> bool {
        matches!(self, ConnectorState::Connected)
    }

    /// 是否处于错误状态。
    pub fn is_error(&self) -> bool {
        matches!(self, ConnectorState::Error(_))
    }
}

impl std::fmt::Display for ConnectorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectorState::Disconnected => write!(f, "disconnected"),
            ConnectorState::Connecting => write!(f, "connecting"),
            ConnectorState::Connected => write!(f, "connected"),
            ConnectorState::Error(msg) => write!(f, "error: {}", msg),
        }
    }
}

/// 同步会话状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SyncSessionStatus {
    /// 进行中。
    Running,
    /// 已完成。
    Completed,
    /// 失败。
    Failed,
}

/// 单次同步会话报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSession {
    /// 会话唯一 ID。
    pub session_id: String,
    /// 触发同步的连接器名称。
    pub connector: String,
    /// 连接器提供商标识 (caldav / imap / webdav / ...)。
    pub provider: String,
    /// 开始时间。
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// 结束时间 (运行中为 None)。
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 成功同步条目数。
    pub items_synced: usize,
    /// 失败条目数。
    pub items_failed: usize,
    /// 会话状态。
    pub status: SyncSessionStatus,
}

impl SyncSession {
    /// 创建新会话 (运行中状态)。
    pub fn new(connector: impl Into<String>, provider: impl Into<String>) -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            connector: connector.into(),
            provider: provider.into(),
            started_at: chrono::Utc::now(),
            finished_at: None,
            items_synced: 0,
            items_failed: 0,
            status: SyncSessionStatus::Running,
        }
    }

    /// 标记会话成功完成。
    pub fn finish(&mut self, items_synced: usize, items_failed: usize) {
        self.finished_at = Some(chrono::Utc::now());
        self.items_synced = items_synced;
        self.items_failed = items_failed;
        self.status = if items_failed == 0 {
            SyncSessionStatus::Completed
        } else {
            SyncSessionStatus::Failed
        };
    }

    /// 标记会话失败。
    pub fn fail(&mut self, error: impl Into<String>) {
        let _ = error;
        self.finished_at = Some(chrono::Utc::now());
        self.status = SyncSessionStatus::Failed;
    }

    /// 是否已结束。
    pub fn is_done(&self) -> bool {
        self.status != SyncSessionStatus::Running
    }
}

/// 外部同步连接器 trait。
///
/// 所有外部系统 (CalDAV / IMAP / WebDAV / ...) 统一实现该 trait，
/// 由 [`ConnectorRegistry`] 集中管理。实现者通过内部 `Arc<RwLock<>>` 维护状态，
/// 因此方法均接收 `&self` 以支持 `Arc<dyn SyncConnector>` 共享。
pub trait SyncConnector: Send + Sync {
    /// 连接器人类可读名称。
    fn name(&self) -> &str;

    /// 提供商标识 (如 "caldav" / "imap" / "webdav" / "github")。
    fn provider(&self) -> &str;

    /// 建立连接 (握手 + 认证)。
    fn connect(&self) -> crate::Result<()>;

    /// 断开连接。
    fn disconnect(&self) -> crate::Result<()>;

    /// 执行一次同步，返回会话报告。
    fn sync(&self) -> crate::Result<SyncSession>;

    /// 当前连接器状态。
    fn state(&self) -> ConnectorState;
}

/// 连接器注册表。
///
/// 集中注册 / 注销 / 查询外部连接器，并支持批量生命周期操作。
pub struct ConnectorRegistry {
    connectors: Arc<RwLock<HashMap<String, Arc<dyn SyncConnector>>>>,
}

impl ConnectorRegistry {
    /// 创建空注册表。
    pub fn new() -> Self {
        Self {
            connectors: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册一个连接器。
    ///
    /// 若 `name` 已存在则返回错误。
    pub fn register(
        &self,
        name: impl Into<String>,
        connector: Arc<dyn SyncConnector>,
    ) -> crate::Result<()> {
        let name = name.into();
        let mut map = self.connectors.write();
        if map.contains_key(&name) {
            return Err(crate::Error::ExternalSync(format!(
                "connector already registered: {}",
                name
            )));
        }
        info!("connector registered: {} ({})", name, connector.provider());
        map.insert(name, connector);
        Ok(())
    }

    /// 注销连接器，返回是否实际移除。
    pub fn unregister(&self, name: &str) -> bool {
        let removed = self.connectors.write().remove(name).is_some();
        if removed {
            info!("connector unregistered: {}", name);
        }
        removed
    }

    /// 获取连接器引用。
    pub fn get(&self, name: &str) -> Option<Arc<dyn SyncConnector>> {
        self.connectors.read().get(name).cloned()
    }

    /// 列出所有已注册连接器名称。
    pub fn list(&self) -> Vec<String> {
        self.connectors.read().keys().cloned().collect()
    }

    /// 查询某连接器状态。
    pub fn state(&self, name: &str) -> Option<ConnectorState> {
        self.connectors.read().get(name).map(|c| c.state())
    }

    /// 已注册连接器数量。
    pub fn count(&self) -> usize {
        self.connectors.read().len()
    }

    /// 连接指定连接器。
    pub fn connect(&self, name: &str) -> crate::Result<()> {
        let connector = self
            .get(name)
            .ok_or_else(|| crate::Error::NotFound(format!("connector not found: {}", name)))?;
        connector.connect()
    }

    /// 断开指定连接器。
    pub fn disconnect(&self, name: &str) -> crate::Result<()> {
        let connector = self
            .get(name)
            .ok_or_else(|| crate::Error::NotFound(format!("connector not found: {}", name)))?;
        connector.disconnect()
    }

    /// 触发指定连接器同步。
    pub fn sync(&self, name: &str) -> crate::Result<SyncSession> {
        let connector = self
            .get(name)
            .ok_or_else(|| crate::Error::NotFound(format!("connector not found: {}", name)))?;
        connector.sync()
    }

    /// 连接所有已注册连接器，返回失败列表。
    pub fn connect_all(&self) -> Vec<(String, crate::Error)> {
        let names: Vec<String> = self.list();
        let mut failures = Vec::new();
        for name in names {
            if let Err(e) = self.connect(&name) {
                warn!("connect_all failed for {}: {}", name, e);
                failures.push((name, e));
            }
        }
        failures
    }

    /// 断开所有已注册连接器。
    pub fn disconnect_all(&self) -> Vec<(String, crate::Error)> {
        let names: Vec<String> = self.list();
        let mut failures = Vec::new();
        for name in names {
            if let Err(e) = self.disconnect(&name) {
                warn!("disconnect_all failed for {}: {}", name, e);
                failures.push((name, e));
            }
        }
        failures
    }

    /// 同步所有已连接连接器，返回 (会话, 错误) 列表。
    pub fn sync_all(&self) -> Vec<(String, crate::Result<SyncSession>)> {
        let names: Vec<String> = self.list();
        let mut results = Vec::new();
        for name in names {
            debug!("sync_all: {}", name);
            let r = self.sync(&name);
            results.push((name, r));
        }
        results
    }
}

impl Default for ConnectorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用空连接器：状态由内部 RwLock 控制。
    struct StubConnector {
        name: String,
        provider: String,
        state: Arc<RwLock<ConnectorState>>,
    }

    impl StubConnector {
        fn new(name: &str, provider: &str) -> Self {
            Self {
                name: name.to_string(),
                provider: provider.to_string(),
                state: Arc::new(RwLock::new(ConnectorState::Disconnected)),
            }
        }
    }

    impl SyncConnector for StubConnector {
        fn name(&self) -> &str {
            &self.name
        }
        fn provider(&self) -> &str {
            &self.provider
        }
        fn connect(&self) -> crate::Result<()> {
            *self.state.write() = ConnectorState::Connecting;
            *self.state.write() = ConnectorState::Connected;
            Ok(())
        }
        fn disconnect(&self) -> crate::Result<()> {
            *self.state.write() = ConnectorState::Disconnected;
            Ok(())
        }
        fn sync(&self) -> crate::Result<SyncSession> {
            let mut s = SyncSession::new(self.name.clone(), self.provider.clone());
            s.finish(3, 0);
            Ok(s)
        }
        fn state(&self) -> ConnectorState {
            self.state.read().clone()
        }
    }

    #[test]
    fn test_connector_state_helpers() {
        assert!(ConnectorState::Connected.is_connected());
        assert!(!ConnectorState::Disconnected.is_connected());
        let err = ConnectorState::Error("boom".into());
        assert!(err.is_error());
        assert!(!err.is_connected());
    }

    #[test]
    fn test_connector_state_display() {
        assert_eq!(ConnectorState::Disconnected.to_string(), "disconnected");
        assert_eq!(ConnectorState::Connected.to_string(), "connected");
        assert_eq!(ConnectorState::Connecting.to_string(), "connecting");
        assert!(ConnectorState::Error("x".into()).to_string().contains("x"));
    }

    #[test]
    fn test_sync_session_new_running() {
        let s = SyncSession::new("cal", "caldav");
        assert_eq!(s.status, SyncSessionStatus::Running);
        assert!(!s.is_done());
        assert!(s.finished_at.is_none());
    }

    #[test]
    fn test_sync_session_finish_completed() {
        let mut s = SyncSession::new("cal", "caldav");
        s.finish(10, 0);
        assert_eq!(s.status, SyncSessionStatus::Completed);
        assert!(s.is_done());
        assert!(s.finished_at.is_some());
        assert_eq!(s.items_synced, 10);
    }

    #[test]
    fn test_sync_session_finish_with_failures_is_failed() {
        let mut s = SyncSession::new("cal", "caldav");
        s.finish(5, 2);
        assert_eq!(s.status, SyncSessionStatus::Failed);
        assert_eq!(s.items_failed, 2);
    }

    #[test]
    fn test_sync_session_fail() {
        let mut s = SyncSession::new("cal", "caldav");
        s.fail("network down");
        assert_eq!(s.status, SyncSessionStatus::Failed);
        assert!(s.is_done());
    }

    #[test]
    fn test_registry_register_and_count() {
        let reg = ConnectorRegistry::new();
        assert_eq!(reg.count(), 0);
        reg.register("cal", Arc::new(StubConnector::new("cal", "caldav")))
            .unwrap();
        assert_eq!(reg.count(), 1);
        assert!(reg.list().contains(&"cal".to_string()));
    }

    #[test]
    fn test_registry_duplicate_register_errors() {
        let reg = ConnectorRegistry::new();
        reg.register("cal", Arc::new(StubConnector::new("cal", "caldav")))
            .unwrap();
        let dup = reg.register("cal", Arc::new(StubConnector::new("cal", "caldav")));
        assert!(dup.is_err());
    }

    #[test]
    fn test_registry_unregister() {
        let reg = ConnectorRegistry::new();
        reg.register("cal", Arc::new(StubConnector::new("cal", "caldav")))
            .unwrap();
        assert!(reg.unregister("cal"));
        assert!(!reg.unregister("cal"));
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn test_registry_get_and_state() {
        let reg = ConnectorRegistry::new();
        reg.register("cal", Arc::new(StubConnector::new("cal", "caldav")))
            .unwrap();
        assert!(reg.get("cal").is_some());
        assert!(reg.get("missing").is_none());
        assert_eq!(reg.state("cal"), Some(ConnectorState::Disconnected));
        assert_eq!(reg.state("missing"), None);
    }

    #[test]
    fn test_registry_connect_disconnect_sync() {
        let reg = ConnectorRegistry::new();
        reg.register("cal", Arc::new(StubConnector::new("cal", "caldav")))
            .unwrap();
        // 初始 Disconnected
        assert_eq!(reg.state("cal"), Some(ConnectorState::Disconnected));
        // 连接 → Connected
        reg.connect("cal").unwrap();
        assert_eq!(reg.state("cal"), Some(ConnectorState::Connected));
        // 同步
        let session = reg.sync("cal").unwrap();
        assert_eq!(session.items_synced, 3);
        // 断开 → Disconnected
        reg.disconnect("cal").unwrap();
        assert_eq!(reg.state("cal"), Some(ConnectorState::Disconnected));
    }

    #[test]
    fn test_registry_connect_unknown_errors() {
        let reg = ConnectorRegistry::new();
        assert!(reg.connect("ghost").is_err());
        assert!(reg.disconnect("ghost").is_err());
        assert!(reg.sync("ghost").is_err());
    }

    #[test]
    fn test_registry_connect_all_and_disconnect_all() {
        let reg = ConnectorRegistry::new();
        reg.register("cal", Arc::new(StubConnector::new("cal", "caldav")))
            .unwrap();
        reg.register("mail", Arc::new(StubConnector::new("mail", "imap")))
            .unwrap();
        let failures = reg.connect_all();
        assert!(failures.is_empty());
        assert_eq!(reg.state("cal"), Some(ConnectorState::Connected));
        assert_eq!(reg.state("mail"), Some(ConnectorState::Connected));
        let failures = reg.disconnect_all();
        assert!(failures.is_empty());
        assert_eq!(reg.state("cal"), Some(ConnectorState::Disconnected));
    }

    #[test]
    fn test_registry_sync_all() {
        let reg = ConnectorRegistry::new();
        reg.register("cal", Arc::new(StubConnector::new("cal", "caldav")))
            .unwrap();
        reg.register("mail", Arc::new(StubConnector::new("mail", "imap")))
            .unwrap();
        let results = reg.sync_all();
        assert_eq!(results.len(), 2);
        for (_, r) in &results {
            assert!(r.is_ok());
        }
    }

    #[test]
    fn test_registry_default() {
        let reg = ConnectorRegistry::default();
        assert_eq!(reg.count(), 0);
    }
}
