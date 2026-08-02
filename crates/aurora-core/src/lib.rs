//! Aurora core - the core layer of the Aurora Note knowledge management system.
//!
//! Provides L1 infrastructure, L2 engines, L3 domain services, shared traits,
//! and the event bus that wires them together.

pub mod event_bus;
pub mod l1_infrastructure;
pub mod l2_engines;
pub mod l3_domain;
pub mod traits;

use thiserror::Error;

/// Aurora Note 核心层统一错误类型。
///
/// 所有 Trait 方法均返回 `Result<T, Error>`，便于跨模块传递与归一化处理。
#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Internal error: {0}")]
    Internal(String),
}
