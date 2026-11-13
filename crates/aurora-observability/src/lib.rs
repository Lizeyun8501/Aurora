//! Aurora observability & operations - logging, metrics, tracing, crash reports,
//! monitoring, alerting, canary release, and diagnostics.
//!
//! Implements Phase 6 (PART VI) of the architecture: the runtime observability
//! pillar (logs / metrics / traces / crashes), the monitoring & alerting system,
//! the canary release & rollback machinery, and the log diagnostics toolkit.

pub mod observability; // metrics+tracing 两面（B-04 裁剪后默认面）

// B-04: 企业级面板转 feature-gate（默认关——个人应用减面;`full` 开全部）
#[cfg(feature = "diagnostics")]
pub mod diagnostics;
#[cfg(feature = "monitoring")]
pub mod monitoring;
#[cfg(feature = "release")]
pub mod release;
#[cfg(feature = "testing")]
pub mod testing;

use thiserror::Error;

/// Aurora observability 统一错误类型。
#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Metrics error: {0}")]
    Metrics(String),
    #[error("Tracing error: {0}")]
    Tracing(String),
    #[error("Release error: {0}")]
    Release(String),
    #[error("Diagnostics error: {0}")]
    Diagnostics(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<Error> for aurora_core::Error {
    fn from(err: Error) -> Self {
        aurora_core::Error::Internal(err.to_string())
    }
}
