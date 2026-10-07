//! Trait: PluginRuntime — 插件运行时接口，支持 WASM 和 iframe 两种沙箱模式
//!
//! V19 §28 原始指定 `async_trait`，本批次 PR 推进异步化迁移。
//! 纯查询方法 `list_hooks` 保持同步签名。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// DK-42: 沙箱资源限制参数（manifest 级可配——插件市场差异化限额刚需）。
/// serde default 向后兼容：旧清单（无 sandbox 字段）反序列化即缺省安全值。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SandboxLimits {
    /// epoch deadline ticks（tick=10ms；缺省 200 = 2s；0 = 回退 runtime 缺省）。
    pub epoch_deadline_ticks: u32,
    /// 单插件线性内存上限字节（缺省 64 MiB；0 = 回退 runtime 缺省）。
    pub max_memory_bytes: usize,
}

impl Default for SandboxLimits {
    fn default() -> Self {
        Self {
            epoch_deadline_ticks: 200,
            max_memory_bytes: 64 * 1024 * 1024,
        }
    }
}

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
    /// DK-42: 沙箱资源限制（manifest 级覆盖 runtime 缺省；serde default 平滑兼容）。
    #[serde(default)]
    pub sandbox: SandboxLimits,
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
    async fn invoke(
        &self,
        handle: &PluginHandle,
        method: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, crate::Error>;
    async fn unload(&mut self, handle: &PluginHandle) -> Result<(), crate::Error>;
    /// 纯查询方法，保持同步签名。
    fn list_hooks(&self, hook_point: &str) -> Vec<PluginHandle>;
}
