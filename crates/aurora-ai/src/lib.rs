//! Aurora AI 智能体网关 (Aurora AgentGateway)
//!
//! 为 Aurora Note 提供 AI Agent 通信、工具编排与会话管理能力，对应
//! Phase 4 P2 的 Task 4.1（AgentGateway 智能体网关）；以及 V19 §7.2
//! 「本地 AI（可选懒加载）」的两个真实 AIProvider 实现。
//!
//! # 模块组织
//! - [`ollama`]：通过 Ollama HTTP API（`http://localhost:11434`）接入
//!   本地 AI；本地不可达时可降级给 [`cloud`] 兜底（V19 §7.2）。
//! - [`cloud`]：OpenAI 兼容端点（`/v1/chat/completions` + `/v1/embeddings`）
//!   最小实现，专做 `OllamaProvider` 的 fallback。
//! - [`mcp`]：MCP 协议（JSON-RPC 2.0 + initialize/tools.list/tools.call/
//!   resources.list）与 stdio/SSE 传输抽象。
//! - [`registry`]：基于 `aurora_core::AgentProtocol` 的工具注册与发现，
//!   提供统一目录、按名称/能力检索与跨智能体调用路由。
//! - [`orchestration`]：Agent 编排（顺序/并行/层级三种模式）、确定性
//!   LLM 规划器与可序列化的计划图。
//! - [`context`]：上下文持久化（内存 mock + SQLite DDL）、会话恢复与
//!   滑动窗口压缩。
//! - [`sandbox`]：安全沙箱（权限校验 + 审计日志 + 只读模式）。

pub mod cloud;
pub mod context;
pub mod mcp;
pub mod mock_provider;
pub mod ollama;
pub mod orchestration;
pub mod registry;
pub mod sandbox;

use thiserror::Error;

/// AgentGateway 统一错误类型。
#[derive(Error, Debug)]
pub enum Error {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Transport error: {0}")]
    Transport(String),
    #[error("JSON-RPC error: {0}")]
    JsonRpc(String),
    #[error("Orchestration error: {0}")]
    Orchestration(String),
    #[error("Context error: {0}")]
    Context(String),
    #[error("Sandbox error: {0}")]
    Sandbox(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Bincode error: {0}")]
    Bincode(#[from] bincode::Error),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<aurora_core::Error> for Error {
    fn from(e: aurora_core::Error) -> Self {
        match e {
            aurora_core::Error::NotFound(s) => Error::NotFound(s),
            aurora_core::Error::PermissionDenied(s) => Error::PermissionDenied(s),
            aurora_core::Error::InvalidInput(s) => Error::InvalidInput(s),
            aurora_core::Error::Serialization(err) => Error::Serialization(err),
            aurora_core::Error::Io(err) => Error::Internal(err.to_string()),
            other => Error::Internal(other.to_string()),
        }
    }
}

impl From<Error> for aurora_core::Error {
    fn from(e: Error) -> Self {
        match e {
            Error::NotFound(s) => aurora_core::Error::NotFound(s),
            Error::PermissionDenied(s) => aurora_core::Error::PermissionDenied(s),
            Error::InvalidInput(s) => aurora_core::Error::InvalidInput(s),
            Error::Serialization(err) => aurora_core::Error::Serialization(err),
            other => aurora_core::Error::Internal(other.to_string()),
        }
    }
}

// 顶层常用类型再导出，便于外部 `use aurora_ai::{...}`。
pub use cloud::OpenAiCompatProvider;
pub use context::{
    AgentContext, CompressedContext, ContextMessage, ContextStore, ContextWindow, SessionId,
    CONTEXT_DDL,
};
pub use mcp::{
    InitializeResult, JsonRpcError, JsonRpcId, JsonRpcRequest, JsonRpcResponse, McpClient,
    McpMethod, McpResource, McpServer, McpTool, McpTransport, ResourcesListResult, ServerInfo,
    SseTransport, StdioTransport, ToolsCallResult, ToolsListResult, MCP_PROTOCOL_VERSION,
};
pub use ollama::OllamaProvider;
pub use orchestration::{
    AgentOrchestrator, DeterministicPlanner, OrchestrationMode, OrchestrationPlan,
    OrchestrationSummary, PlanEdge, PlanGraph, PlanNode, PlanStep, StepResult,
};
pub use registry::{MockAgent, Tool, ToolInvocation, ToolRegistry, ToolResult};
pub use sandbox::{
    is_read_tool, is_write_tool, AuditAction, AuditDecision, AuditEntry, AuditLog, SandboxConfig,
    SecuritySandbox, READ_TOOL_PREFIXES, WRITE_TOOL_PREFIXES,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let e = Error::NotFound("missing".into());
        assert!(format!("{}", e).contains("missing"));
        let e2 = Error::PermissionDenied("no".into());
        assert!(format!("{}", e2).contains("no"));
        let e3 = Error::JsonRpc("rpc".into());
        assert!(format!("{}", e3).contains("rpc"));
    }

    #[test]
    fn test_error_from_aurora_core_not_found() {
        let core_err = aurora_core::Error::NotFound("x".into());
        let ai_err: Error = core_err.into();
        assert!(matches!(ai_err, Error::NotFound(_)));
    }

    #[test]
    fn test_error_from_aurora_core_permission_denied() {
        let core_err = aurora_core::Error::PermissionDenied("denied".into());
        let ai_err: Error = core_err.into();
        assert!(matches!(ai_err, Error::PermissionDenied(_)));
    }

    #[test]
    fn test_error_from_aurora_core_invalid_input() {
        let core_err = aurora_core::Error::InvalidInput("bad".into());
        let ai_err: Error = core_err.into();
        assert!(matches!(ai_err, Error::InvalidInput(_)));
    }

    #[test]
    fn test_error_from_aurora_core_serialization() {
        let core_err =
            aurora_core::Error::Serialization(serde_json::from_str::<()>("bad").unwrap_err());
        let ai_err: Error = core_err.into();
        assert!(matches!(ai_err, Error::Serialization(_)));
    }

    #[test]
    fn test_error_from_aurora_core_internal() {
        let core_err = aurora_core::Error::Internal("oops".into());
        let ai_err: Error = core_err.into();
        assert!(matches!(ai_err, Error::Internal(_)));
    }

    #[test]
    fn test_error_from_aurora_core_database() {
        let core_err = aurora_core::Error::Database("db".into());
        let ai_err: Error = core_err.into();
        assert!(matches!(ai_err, Error::Internal(_)));
    }

    #[test]
    fn test_error_into_aurora_core_roundtrip_not_found() {
        let ai_err = Error::NotFound("x".into());
        let core_err: aurora_core::Error = ai_err.into();
        assert!(matches!(core_err, aurora_core::Error::NotFound(_)));
    }

    #[test]
    fn test_error_into_aurora_core_roundtrip_permission_denied() {
        let ai_err = Error::PermissionDenied("x".into());
        let core_err: aurora_core::Error = ai_err.into();
        assert!(matches!(core_err, aurora_core::Error::PermissionDenied(_)));
    }

    #[test]
    fn test_error_into_aurora_core_roundtrip_invalid_input() {
        let ai_err = Error::InvalidInput("x".into());
        let core_err: aurora_core::Error = ai_err.into();
        assert!(matches!(core_err, aurora_core::Error::InvalidInput(_)));
    }

    #[test]
    fn test_error_into_aurora_core_roundtrip_internal() {
        let ai_err = Error::JsonRpc("rpc".into());
        let core_err: aurora_core::Error = ai_err.into();
        assert!(matches!(core_err, aurora_core::Error::Internal(_)));
    }

    #[test]
    fn test_error_from_serde_json_via_from() {
        let json_err = serde_json::from_str::<serde_json::Value>("bad").unwrap_err();
        let ai_err: Error = json_err.into();
        assert!(matches!(ai_err, Error::Serialization(_)));
    }
}
