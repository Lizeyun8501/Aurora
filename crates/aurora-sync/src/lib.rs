//! Aurora 同步服务 (Aurora Sync Service)
//!
//! 本地优先 (local-first) 的多端同步服务层，构建于 `aurora-core` 之上，
//! 围绕 [`aurora_core::traits::sync_target::SyncTarget`] 抽象提供完整的同步能力。
//!
//! # 模块组织
//! - [`p2p`]: P2P 同步 (iroh + QUIC + NAT 穿透)
//! - [`iroh_transport`]: iroh 真实传输层 (V19 §31 DEV-005，生产替代 MockTransport)
//! - [`cloud`]: 云端同步 (WebSocket 实时推送 + HTTPS 批量，零知识密文中转)
//! - [`lan`]: 局域网同步 (mDNS 自动发现 + LAN 直连 QUIC，优先于云端)
//! - [`conflict`]: 冲突解决 (CRDT 自动合并 + 语义冲突手动选择 + 分支模式)
//! - [`incremental`]: 增量同步 (CRDT ops 增量 + rsync 块级增量 + zstd 压缩)
//! - [`offline_queue`]: 离线队列 (SQLite 持久化 + 优先级 + 幂等键 + 批量压缩)
//! - [`device`]: 多设备管理 (Ed25519 设备 ID + QR 授权 + 远程吊销 + DEK 失效)
//! - [`external`]: 外部同步中心 (CalDAV 日历 + IMAP 邮件 + 云盘 + Webhook 接收)

pub mod cloud;
pub mod conflict;
pub mod device;
pub mod external;
pub mod incremental;
// iroh_transport 模块需要 iroh 1.0+ (Rust 1.91+)，临时禁用
// #[cfg(feature = "iroh-transport")]
// pub mod iroh_transport;
pub mod lan;
pub mod offline_queue;
pub mod p2p;

use thiserror::Error;

/// 同步服务统一错误类型。
///
/// 所有子模块返回 `Result<T, Error>`，便于跨模块归一化处理，
/// 并可无缝转换为 `aurora_core::Error` 以适配 `SyncTarget` trait 签名。
#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Bincode error: {0}")]
    Bincode(#[from] bincode::Error),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Sync error: {0}")]
    Sync(String),
    #[error("Conflict error: {0}")]
    Conflict(String),
    #[error("Device error: {0}")]
    Device(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("External sync error: {0}")]
    ExternalSync(String),
    #[error("HMAC signature verification failed")]
    HmacVerificationFailed,
}

/// 同步服务 Result 别名。
pub type Result<T> = std::result::Result<T, Error>;

/// 将本 crate 错误转换为 aurora-core 错误，便于跨 crate 传递给 SyncTarget。
impl From<Error> for aurora_core::Error {
    fn from(e: Error) -> Self {
        match e {
            Error::Io(e) => aurora_core::Error::Io(e),
            Error::Serialization(e) => aurora_core::Error::Serialization(e),
            Error::Database(msg) => aurora_core::Error::Database(msg),
            Error::Network(msg) => aurora_core::Error::Network(msg),
            Error::NotFound(msg) => aurora_core::Error::NotFound(msg),
            Error::InvalidInput(msg) => aurora_core::Error::InvalidInput(msg),
            Error::Unauthorized(msg) => aurora_core::Error::PermissionDenied(msg),
            other => aurora_core::Error::Internal(other.to_string()),
        }
    }
}

pub use cloud::{CloudConfig, CloudSyncEngine, SyncBatch};
pub use conflict::{Branch, ConflictResolution, ConflictResolver, SemanticConflict};
pub use device::{Device, DeviceId, DeviceManager, DeviceStatus, QrAuthorization};
pub use external::{
    CalDavConfig, CalDavConnector, CalendarEvent, CalendarSync, CloudDriveConnector,
    ConnectorRegistry, ConnectorState, DropboxConnector, DriveFile, DriveProvider,
    EmailAttachment, EmailDocument, EmailFilter, EmailMessage, EmailSync, GoogleDriveConnector,
    HmacVerifier, ImapConfig, ImapConnector, OneDriveConnector, SelectiveSyncConfig,
    SyncConnector, SyncSession, SyncSessionStatus, WebDavConnector, WebhookConfig, WebhookEvent,
    WebhookReceiver, WebhookSource,
};
pub use incremental::{BlockDelta, BlockSignature, IncrementalSync, RollingHash};
pub use lan::{LanPeer, LanSyncEngine, MdnsDiscovery, SyncRoute};
pub use offline_queue::{OfflineQueue, Priority, QueueItem};
pub use p2p::{P2pSyncEngine, PeerId, SyncMessage, VersionVector};
// iroh_transport 类型重导出已随模块一起禁用
// #[cfg(feature = "iroh-transport")]
// pub use iroh_transport::{IrohTransport, SyncReport, run_accept_loop, AURORA_ALPN, MAX_SYNC_MESSAGE_SIZE};
