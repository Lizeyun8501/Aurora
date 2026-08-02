//! Trait 7: AgentProtocol — AI Agent 通信协议接口，MCP 兼容

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    pub session_id: String,
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub result: serde_json::Value,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEvent {
    pub event_type: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    pub session_id: String,
    pub messages: Vec<crate::traits::ai_provider::Message>,
    pub tool_results: Vec<serde_json::Value>,
    pub user_preferences: serde_json::Value,
}

pub trait AgentProtocol: Send + Sync {
    fn register_tool(&self, tool: &ToolDefinition) -> Result<(), crate::Error>;
    fn execute(&self, request: &AgentRequest) -> Result<AgentResponse, crate::Error>;
    fn subscribe(&self, event_type: &str, callback: Box<dyn Fn(AgentEvent) + Send + Sync>);
    fn get_context(&self, session_id: &str) -> Result<Context, crate::Error>;
}
