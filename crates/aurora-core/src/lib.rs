//! Aurora core - the core layer of the Aurora Note knowledge management system.
//!
//! Provides L1 infrastructure, L2 engines, L3 domain services, shared traits,
//! and the event bus that wires them together.
//!
//! # 入口点
//!
//! [`AppCore`] 是 V19 §36.1 定义的聚合根，通过 [`AppCoreBuilder`] 注入各 Trait
//! 实现后作为平台适配层的依赖容器。

pub mod app_core;
pub mod event_bus;
pub mod l1_infrastructure;
pub mod l2_engines;
pub mod l3_domain;
pub mod traits;

use thiserror::Error;

/// Aurora Note 核心层统一错误类型（对齐 V19 §33 `AuroraError`）。
///
/// 所有 Trait 方法均返回 `Result<T, Error>`，便于跨模块传递与归一化处理。
///
/// # 错误处理三问（V19 §33.1）
/// - 用户看到什么？→ [`Error::user_message`]
/// - 能否自动重试？→ [`Error::is_retryable`]
/// - 是否需要降级？→ [`Error::requires_fallback`]（降级矩阵见 V19 §33.2）
#[derive(Error, Debug)]
pub enum Error {
    // ===== 基础设施层错误 =====
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Loro CRDT error: {0}")]
    Loro(String),

    // ===== 领域服务层错误 =====
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("笔记不存在: {id}")]
    NoteNotFound {
        /// 缺失的笔记 ID。
        id: String,
    },
    #[error("Workspace 权限不足: 需要 {required}, 当前 {current}")]
    InsufficientPermission {
        /// 所需权限级别。
        required: String,
        /// 当前权限级别。
        current: String,
    },
    #[error("加密错误: {0}")]
    Crypto(String),
    #[error("同步冲突: note_id={note_id}, local_vv={local}, remote_vv={remote}")]
    SyncConflict {
        /// 冲突笔记 ID。
        note_id: String,
        /// 本地版本向量。
        local: String,
        /// 远端版本向量。
        remote: String,
    },

    // ===== 开放集成层错误 =====
    #[error("插件错误: {0}")]
    Plugin(String),
    #[error("Agent 认证失败: {0}")]
    AgentAuth(String),
    #[error("NL→SQL 生成失败: {reason}")]
    NlToSqlFailed {
        /// 失败原因。
        reason: String,
    },
    #[error("OCR 识别失败: {0}")]
    Ocr(String),

    // ===== 配置与初始化错误 =====
    #[error("配置错误: {0}")]
    Config(String),
    #[error("模型文件未找到: {path}")]
    ModelNotFound {
        /// 缺失的模型路径。
        path: String,
    },

    #[error("Internal error: {0}")]
    Internal(String),
}

impl Error {
    /// 用户面向的错误消息（用于 UI 展示，V19 §33.1）。
    pub fn user_message(&self) -> String {
        match self {
            Error::NoteNotFound { .. } => "笔记不存在，可能已被删除".into(),
            Error::InsufficientPermission { .. } | Error::PermissionDenied(_) => {
                "权限不足，无法执行此操作".into()
            }
            Error::Crypto(_) => "加密操作失败，请检查密码".into(),
            Error::SyncConflict { .. } => "同步冲突，已保存冲突副本".into(),
            Error::Network(_) => "网络同步失败，已切换到离线模式".into(),
            Error::Plugin(_) => "插件执行出错，已自动终止".into(),
            Error::AgentAuth(_) => "Agent 认证失败，请重新授权".into(),
            Error::ModelNotFound { .. } => "模型文件未找到，请先下载模型".into(),
            _ => "操作失败，请重试".into(),
        }
    }

    /// 是否可自动重试（V19 §33.1）。
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Error::Network(_) | Error::Database(_) | Error::Io(_)
        )
    }

    /// 是否需要降级处理（V19 §33.1，降级矩阵见 §33.2）。
    ///
    /// 典型降级路径：
    /// - `Network` → P2P 失败切换 WebDAV 同步
    /// - `Loro` → import 失败保存原始字节到 `.corrupt` 文件
    /// - `Ocr` / `ModelNotFound` → 提示下载模型，阻塞对应功能
    pub fn requires_fallback(&self) -> bool {
        matches!(
            self,
            Error::Network(_)
                | Error::Loro(_)
                | Error::Ocr(_)
                | Error::ModelNotFound { .. }
        )
    }
}
