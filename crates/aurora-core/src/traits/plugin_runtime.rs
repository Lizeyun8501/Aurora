//! Trait 6: PluginRuntime — 插件运行时接口，支持 WASM 和 iframe 两种沙箱模式

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

pub trait PluginRuntime: Send + Sync {
    fn load(&mut self, manifest: &PluginManifest) -> Result<PluginHandle, crate::Error>;
    fn invoke(&self, handle: &PluginHandle, method: &str, args: &serde_json::Value) -> Result<serde_json::Value, crate::Error>;
    fn unload(&mut self, handle: &PluginHandle) -> Result<(), crate::Error>;
    fn list_hooks(&self, hook_point: &str) -> Vec<PluginHandle>;
}
