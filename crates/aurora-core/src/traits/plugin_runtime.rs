//! Trait: PluginRuntime — 插件运行时接口，支持 WASM 和 iframe 两种沙箱模式
//!
//! V19 §28 原始指定 `async_trait`，本批次 PR 推进异步化迁移。
//! 纯查询方法 `list_hooks` 保持同步签名。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub runtime: RuntimeType,
    pub entry: String,
    pub permissions: Vec<String>,
    pub hooks: Vec<String>,
    pub block_types: Vec<String>,
    pub config_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RuntimeType {
    Wasm,
    Iframe,
}

#[derive(Debug, Clone)]
pub struct PluginHandle {
    pub id: String,
    pub manifest: PluginManifest,
}

#[async_trait]
pub trait PluginRuntime: Send + Sync {
    async fn load(&mut self, manifest: &PluginManifest) -> Result<PluginHandle, crate::Error>;
    async fn invoke(&self, handle: &PluginHandle, method: &str, args: &serde_json::Value) -> Result<serde_json::Value, crate::Error>;
    async fn unload(&mut self, handle: &PluginHandle) -> Result<(), crate::Error>;
    /// 纯查询方法，保持同步签名。
    fn list_hooks(&self, hook_point: &str) -> Vec<PluginHandle>;
}
