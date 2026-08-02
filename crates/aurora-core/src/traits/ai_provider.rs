//! Trait 4: AIProvider — AI 模型推理服务的统一接口，支持本地与云端模型

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

pub trait AIProvider: Send + Sync {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, crate::Error>;
    fn complete(&self, prompt: &str, opts: &CompletionOptions) -> Result<String, crate::Error>;
    fn stream_complete(&self, prompt: &str, opts: &CompletionOptions, callback: Box<dyn Fn(String) + Send + Sync>);
    fn chat(&self, messages: &[Message], opts: &ChatOptions) -> Result<String, crate::Error>;
    fn function_call(&self, prompt: &str, tools: &[Tool]) -> Result<ToolCall, crate::Error>;
    fn is_available(&self) -> bool;
}
