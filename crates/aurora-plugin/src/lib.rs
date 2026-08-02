//! Aurora 插件系统 (Aurora Plugin System)
//!
//! 为 Aurora Note 提供可扩展的插件管理能力，支持 WASM 与 iframe 双沙箱模式、
//! 最小权限控制、去中心化市场（Ed25519 代码签名）与热更新。
//!
//! # 模块组织
//! - [`lifecycle`]：双模式架构与插件生命周期状态机，实现 `aurora_core::PluginRuntime`。
//! - [`wasm_runtime`]：WASM 沙箱运行时（Wasmtime）、能力清单与宿主函数注册表。
//! - [`iframe_runtime`]：iframe 沙箱隔离、JSON-RPC 2.0 通信与 CSS 主题透传。
//! - [`permission`]：插件权限控制、动态升级与审计日志。
//! - [`marketplace`]：去中心化插件市场、Ed25519 签名验证与更新检测。
//! - [`hot_update`]：后台预加载、确认切换、失败回滚与金丝雀发布。

pub mod hot_update;
pub mod iframe_runtime;
pub mod lifecycle;
pub mod marketplace;
pub mod permission;
pub mod wasm_runtime;

use thiserror::Error;

/// 插件系统统一错误类型。
#[derive(Error, Debug)]
pub enum Error {
    #[error("Plugin not found: {0}")]
    NotFound(String),
    #[error("Invalid plugin manifest: {0}")]
    InvalidManifest(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Invalid lifecycle transition: {from:?} -> {to:?}")]
    InvalidStateTransition {
        from: lifecycle::PluginStatus,
        to: lifecycle::PluginStatus,
    },
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Capability not granted: {0:?}")]
    CapabilityDenied(wasm_runtime::Capability),
    #[error("Host function error: {0}")]
    HostFunction(String),
    #[error("WASM runtime error: {0}")]
    WasmRuntime(String),
    #[error("JSON-RPC error: {0}")]
    JsonRpc(String),
    #[error("Signature verification failed: {0}")]
    SignatureInvalid(String),
    #[error("Marketplace error: {0}")]
    Marketplace(String),
    #[error("Hot update error: {0}")]
    HotUpdate(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Bincode error: {0}")]
    Bincode(#[from] bincode::Error),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<Error> for aurora_core::Error {
    fn from(e: Error) -> Self {
        match e {
            Error::NotFound(s) => aurora_core::Error::NotFound(s),
            Error::PermissionDenied(s) => aurora_core::Error::PermissionDenied(s),
            Error::InvalidManifest(s) | Error::InvalidInput(s) => {
                aurora_core::Error::InvalidInput(s)
            }
            Error::Serialization(e) => aurora_core::Error::Serialization(e),
            other => aurora_core::Error::Internal(other.to_string()),
        }
    }
}

// 顶层常用类型再导出，便于外部 `use aurora_plugin::{Plugin, PluginMode, ...}`。
pub use hot_update::{HotUpdateManager, UpdateResult, UpdateState};
pub use iframe_runtime::{
    CssTheme, IframeFrame, IframeRuntime, JsonRpcError, JsonRpcId, JsonRpcRequest, JsonRpcResponse,
};
pub use lifecycle::{
    Plugin, PluginId, PluginLifecycle, PluginManager, PluginMode, PluginStatus,
};
pub use marketplace::{
    generate_keypair, Marketplace, PluginListing, PluginSignature, PluginSource, UpdateCheck,
};
pub use permission::{
    DefaultGrantor, Permission, PermissionAuditEntry, PermissionAuditLog, PermissionDecision,
    PermissionEngine, PermissionGrantor, PermissionRequest,
};
pub use wasm_runtime::{
    Capability, CapabilityManifest, DefaultHostFunctions, HostFunction, HostFunctionRegistry,
    WasmRuntime,
};
