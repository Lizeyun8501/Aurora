//! MockAIProvider — AI 推理服务的 Mock 实现
//!
//! 用于开发期占位与单元测试，所有方法返回空结果或错误。
//! 生产环境应替换为 `LocalLlamaProvider` 或 `CloudAIProvider`。

use async_trait::async_trait;

use aurora_core::traits::ai_provider::{
    AIProvider, ChatOptions, CompletionOptions, Message, Tool, ToolCall,
};

/// Mock AI Provider，所有方法返回占位结果。
pub struct MockAIProvider;

impl MockAIProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockAIProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AIProvider for MockAIProvider {
    async fn embed(&self, _texts: &[&str]) -> Result<Vec<Vec<f32>>, aurora_core::Error> {
        Ok(_texts.iter().map(|_| vec![0.0; 384]).collect())
    }

    async fn complete(
        &self,
        _prompt: &str,
        _opts: &CompletionOptions,
    ) -> Result<String, aurora_core::Error> {
        Ok(String::new())
    }

    fn stream_complete(
        &self,
        _prompt: &str,
        _opts: &CompletionOptions,
        _callback: Box<dyn Fn(String) + Send + Sync>,
    ) {
        // Mock: no-op
    }

    async fn chat(
        &self,
        _messages: &[Message],
        _opts: &ChatOptions,
    ) -> Result<String, aurora_core::Error> {
        Ok(String::new())
    }

    async fn function_call(
        &self,
        _prompt: &str,
        _tools: &[Tool],
    ) -> Result<ToolCall, aurora_core::Error> {
        Err(aurora_core::Error::Internal(
            "MockAIProvider: function_call not implemented".to_string(),
        ))
    }

    fn is_available(&self) -> bool {
        false
    }
}
