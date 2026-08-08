//! Trait: AIProvider — AI 模型推理服务的统一接口，支持本地与云端模型
//!
//! V19 §28 原始指定 `async_trait`，本批次 PR 推进异步化迁移。
//! 流式回调 `stream_complete` 与纯查询 `is_available` 保持同步签名。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionOptions {
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub stop: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatOptions {
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

#[async_trait]
pub trait AIProvider: Send + Sync {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, crate::Error>;
    async fn complete(
        &self,
        prompt: &str,
        opts: &CompletionOptions,
    ) -> Result<String, crate::Error>;
    /// 流式回调（fire-and-forget，保持同步签名）。
    fn stream_complete(
        &self,
        prompt: &str,
        opts: &CompletionOptions,
        callback: Box<dyn Fn(String) + Send + Sync>,
    );
    async fn chat(&self, messages: &[Message], opts: &ChatOptions) -> Result<String, crate::Error>;
    async fn function_call(&self, prompt: &str, tools: &[Tool]) -> Result<ToolCall, crate::Error>;
    /// 纯查询方法，保持同步签名。
    fn is_available(&self) -> bool;
}
