//! 工具注册与发现 (Tool Registration & Discovery)
//!
//! 基于 `aurora_core::traits::AgentProtocol` 的工具聚合层：将多个智能体
//! 实现注册到统一的 `ToolRegistry` 中，提供全局工具目录、按名称/能力
//! 动态发现，以及跨智能体的工具调用路由。
//!
//! # 设计要点
//! - `Tool`：扩展 `aurora_core::traits::agent_protocol::ToolDefinition`，
//!   额外携带 `output_schema` 与 `capabilities` 标签用于能力发现。
//! - `ToolRegistry`：聚合多个 `AgentProtocol` 实现，维护工具到智能体的
//!   路由映射，支持 `list_all` / `find_by_name` / `find_by_capability`。
//! - `MockAgent`：`AgentProtocol` 的内存实现，用于测试与本地编排。

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use aurora_core::traits::agent_protocol::{
    AgentEvent, AgentProtocol, AgentRequest, AgentResponse, Context, ToolDefinition,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::sandbox::{SandboxConfig, SecuritySandbox};

// ============================================================================
// 工具描述与调用类型
// ============================================================================

/// 工具描述，扩展自 `aurora_core` 的 `ToolDefinition`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    /// 工具名（全局唯一）
    pub name: String,
    /// 人类可读说明
    pub description: String,
    /// 输入 JSON Schema
    pub input_schema: serde_json::Value,
    /// 输出 JSON Schema
    pub output_schema: serde_json::Value,
    /// 能力标签，用于 `find_by_capability` 检索
    pub capabilities: Vec<String>,
    /// 拥有该工具的智能体 ID
    pub agent_id: String,
}

impl Tool {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            output_schema: serde_json::json!({}),
            capabilities: Vec::new(),
            agent_id: String::new(),
        }
    }

    pub fn with_output_schema(mut self, schema: serde_json::Value) -> Self {
        self.output_schema = schema;
        self
    }

    pub fn with_capabilities(mut self, caps: Vec<String>) -> Self {
        self.capabilities = caps;
        self
    }

    pub fn with_agent(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = agent_id.into();
        self
    }

    /// 是否具备某个能力标签（大小写不敏感）。
    pub fn has_capability(&self, cap: &str) -> bool {
        let cap_lower = cap.to_lowercase();
        self.capabilities
            .iter()
            .any(|c| c.to_lowercase() == cap_lower)
    }

    /// 转换为 `aurora_core` 的 `ToolDefinition`。
    pub fn to_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name.clone(),
            description: self.description.clone(),
            input_schema: self.input_schema.clone(),
        }
    }
}

impl From<ToolDefinition> for Tool {
    fn from(def: ToolDefinition) -> Self {
        Self {
            name: def.name,
            description: def.description,
            input_schema: def.input_schema,
            output_schema: serde_json::json!({}),
            capabilities: Vec::new(),
            agent_id: String::new(),
        }
    }
}

/// 工具调用请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocation {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub session_id: String,
}

impl ToolInvocation {
    pub fn new(
        tool_name: impl Into<String>,
        arguments: serde_json::Value,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            tool_name: tool_name.into(),
            arguments,
            session_id: session_id.into(),
        }
    }
}

/// 工具调用结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// 工具输出
    pub output: serde_json::Value,
    /// 错误信息（若调用失败）
    pub error: Option<String>,
    /// 执行耗时（毫秒）
    pub latency_ms: u64,
    /// 工具名
    pub tool_name: String,
}

impl ToolResult {
    pub fn ok(tool_name: impl Into<String>, output: serde_json::Value, latency_ms: u64) -> Self {
        Self {
            output,
            error: None,
            latency_ms,
            tool_name: tool_name.into(),
        }
    }

    pub fn err(tool_name: impl Into<String>, message: impl Into<String>, latency_ms: u64) -> Self {
        Self {
            output: serde_json::Value::Null,
            error: Some(message.into()),
            latency_ms,
            tool_name: tool_name.into(),
        }
    }

    pub fn is_ok(&self) -> bool {
        self.error.is_none()
    }

    pub fn is_err(&self) -> bool {
        self.error.is_some()
    }
}

// ============================================================================
// 工具注册表
// ============================================================================

/// 工具注册表：聚合多个 `AgentProtocol` 实现，提供统一发现与路由。
pub struct ToolRegistry {
    /// 工具目录：name -> Tool
    tools: Arc<RwLock<HashMap<String, Tool>>>,
    /// 已注册的智能体：agent_id -> Arc<dyn AgentProtocol>
    agents: Arc<RwLock<HashMap<String, Arc<dyn AgentProtocol>>>>,
    /// 工具到智能体的路由：tool_name -> agent_id
    tool_routes: Arc<RwLock<HashMap<String, String>>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            agents: Arc::new(RwLock::new(HashMap::new())),
            tool_routes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册一个智能体（同时可作为工具执行后端）。
    pub fn register_agent(&self, agent_id: impl Into<String>, agent: Arc<dyn AgentProtocol>) {
        let id = agent_id.into();
        info!("registry: register agent {}", id);
        self.agents.write().insert(id, agent);
    }

    /// 注销智能体（同时移除其全部工具）。
    pub fn unregister_agent(&self, agent_id: &str) {
        info!("registry: unregister agent {}", agent_id);
        self.agents.write().remove(agent_id);
        // 移除该 agent 拥有的工具与路由
        let mut tools = self.tools.write();
        let mut routes = self.tool_routes.write();
        let to_remove: Vec<String> = tools
            .iter()
            .filter(|(_, t)| t.agent_id == agent_id)
            .map(|(k, _)| k.clone())
            .collect();
        for name in to_remove {
            tools.remove(&name);
            routes.remove(&name);
        }
    }

    /// 注册一个工具，并绑定到指定智能体。
    pub fn register_tool(&self, tool: Tool) -> Result<(), crate::Error> {
        if tool.agent_id.is_empty() {
            return Err(crate::Error::InvalidInput(
                "tool.agent_id must be set".into(),
            ));
        }
        // 校验对应 agent 是否已注册
        if !self.agents.read().contains_key(&tool.agent_id) {
            return Err(crate::Error::NotFound(format!(
                "agent not registered: {}",
                tool.agent_id
            )));
        }
        let name = tool.name.clone();
        debug!(
            "registry: register tool {} -> agent {}",
            name, tool.agent_id
        );
        // 同步到底层 agent（通过 AgentProtocol::register_tool）
        if let Some(agent) = self.agents.read().get(&tool.agent_id).cloned() {
            let def = tool.to_definition();
            if let Err(e) = agent.register_tool(&def) {
                return Err(crate::Error::from(e));
            }
        }
        self.tool_routes.write().insert(name.clone(), tool.agent_id.clone());
        self.tools.write().insert(name, tool);
        Ok(())
    }

    /// 注销工具。
    pub fn unregister_tool(&self, name: &str) -> Option<Tool> {
        self.tool_routes.write().remove(name);
        self.tools.write().remove(name)
    }

    /// 列出全部工具。
    pub fn list_all(&self) -> Vec<Tool> {
        self.tools.read().values().cloned().collect()
    }

    /// 按名称查找工具。
    pub fn find_by_name(&self, name: &str) -> Option<Tool> {
        self.tools.read().get(name).cloned()
    }

    /// 按能力标签查找工具（任一标签匹配即返回）。
    pub fn find_by_capability(&self, capability: &str) -> Vec<Tool> {
        self.tools
            .read()
            .values()
            .filter(|t| t.has_capability(capability))
            .cloned()
            .collect()
    }

    /// 按描述关键字模糊查找。
    pub fn search(&self, query: &str) -> Vec<Tool> {
        let q = query.to_lowercase();
        self.tools
            .read()
            .values()
            .filter(|t| {
                t.name.to_lowercase().contains(&q)
                    || t.description.to_lowercase().contains(&q)
            })
            .cloned()
            .collect()
    }

    /// 已注册工具数。
    pub fn tool_count(&self) -> usize {
        self.tools.read().len()
    }

    /// 已注册智能体数。
    pub fn agent_count(&self) -> usize {
        self.agents.read().len()
    }

    /// 调用一个工具：根据路由表找到对应 agent，转发 `AgentRequest`。
    pub fn invoke(&self, invocation: &ToolInvocation) -> Result<ToolResult, crate::Error> {
        let started = std::time::Instant::now();
        let agent_id = self
            .tool_routes
            .read()
            .get(&invocation.tool_name)
            .cloned()
            .ok_or_else(|| {
                crate::Error::NotFound(format!(
                    "tool not registered: {}",
                    invocation.tool_name
                ))
            })?;
        let agent = self
            .agents
            .read()
            .get(&agent_id)
            .cloned()
            .ok_or_else(|| {
                crate::Error::NotFound(format!("agent not found: {}", agent_id))
            })?;
        debug!(
            "registry: invoke tool {} via agent {}",
            invocation.tool_name, agent_id
        );
        let request = AgentRequest {
            session_id: invocation.session_id.clone(),
            method: invocation.tool_name.clone(),
            params: invocation.arguments.clone(),
        };
        match agent.execute(&request) {
            Ok(resp) => {
                let latency = started.elapsed().as_millis() as u64;
                if let Some(err) = resp.error {
                    Ok(ToolResult::err(&invocation.tool_name, err, latency))
                } else {
                    Ok(ToolResult::ok(&invocation.tool_name, resp.result, latency))
                }
            }
            Err(e) => {
                let latency = started.elapsed().as_millis() as u64;
                warn!(
                    "registry: invoke {} failed: {}",
                    invocation.tool_name, e
                );
                Ok(ToolResult::err(&invocation.tool_name, e.to_string(), latency))
            }
        }
    }

    /// 在沙箱约束下调用工具：先做权限检查，再执行调用并写入审计。
    pub fn invoke_sandboxed(
        &self,
        invocation: &ToolInvocation,
        sandbox: &SecuritySandbox,
    ) -> Result<ToolResult, crate::Error> {
        sandbox.check_tool(&invocation.tool_name)?;
        sandbox.audit_invoke(invocation)?;
        let result = self.invoke(invocation)?;
        sandbox.audit_result(&invocation.tool_name, &result)?;
        Ok(result)
    }

    /// 获取默认沙箱配置引用（便于上层构造）。
    pub fn default_sandbox_config() -> SandboxConfig {
        SandboxConfig::default()
    }
}

// ============================================================================
// MockAgent — AgentProtocol 的内存实现
// ============================================================================

/// `AgentProtocol` 的内存 mock 实现，用于测试与本地编排。
///
/// 内部维护：
/// - 已注册工具表（`register_tool` 写入）
/// - 一个处理器表（`tool_name -> handler`），用于 `execute` 时回放
/// - 事件订阅器列表
/// - 会话上下文表（`session_id -> Context`）
pub struct MockAgent {
    agent_id: String,
    tools: Arc<RwLock<HashMap<String, ToolDefinition>>>,
    handlers:
        Arc<RwLock<HashMap<String, Arc<dyn Fn(&serde_json::Value) -> Result<serde_json::Value, String> + Send + Sync>>>>,
    subscribers: Arc<RwLock<Vec<(String, Arc<dyn Fn(AgentEvent) + Send + Sync>)>>>,
    contexts: Arc<RwLock<HashMap<String, Context>>>,
}

impl MockAgent {
    pub fn new(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            tools: Arc::new(RwLock::new(HashMap::new())),
            handlers: Arc::new(RwLock::new(HashMap::new())),
            subscribers: Arc::new(RwLock::new(Vec::new())),
            contexts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn id(&self) -> &str {
        &self.agent_id
    }

    /// 注册一个工具处理器：`execute` 时按 method 名查找并调用。
    pub fn register_handler<F>(&self, tool_name: impl Into<String>, handler: F)
    where
        F: Fn(&serde_json::Value) -> Result<serde_json::Value, String> + Send + Sync + 'static,
    {
        self.handlers
            .write()
            .insert(tool_name.into(), Arc::new(handler));
    }

    /// 写入一个会话上下文（便于 `get_context` 回放）。
    pub fn put_context(&self, ctx: Context) {
        self.contexts.write().insert(ctx.session_id.clone(), ctx);
    }

    /// 已注册工具数。
    pub fn tool_count(&self) -> usize {
        self.tools.read().len()
    }

    /// 向所有订阅者广播一个事件。
    pub fn emit_event(&self, event: AgentEvent) {
        let subscribers = self.subscribers.read().clone();
        for (etype, cb) in subscribers {
            if etype == event.event_type {
                cb(event.clone());
            }
        }
    }
}

#[async_trait]
impl AgentProtocol for MockAgent {
    async fn register_tool(&self, tool: &ToolDefinition) -> Result<(), aurora_core::Error> {
        debug!(
            "mock agent {}: register tool {}",
            self.agent_id, tool.name
        );
        self.tools.write().insert(tool.name.clone(), tool.clone());
        Ok(())
    }

    fn execute(&self, request: &AgentRequest) -> Result<AgentResponse, aurora_core::Error> {
        debug!(
            "mock agent {}: execute method={}",
            self.agent_id, request.method
        );
        // 优先查 handlers
        if let Some(handler) = self.handlers.read().get(&request.method).cloned() {
            return match handler(&request.params) {
                Ok(result) => Ok(AgentResponse {
                    result,
                    error: None,
                }),
                Err(msg) => Ok(AgentResponse {
                    result: serde_json::Value::Null,
                    error: Some(msg),
                }),
            };
        }
        // 没有处理器时返回 not implemented
        Ok(AgentResponse {
            result: serde_json::Value::Null,
            error: Some(format!(
                "tool {} not implemented on agent {}",
                request.method, self.agent_id
            )),
        })
    }

    fn subscribe(&self, event_type: &str, callback: Box<dyn Fn(AgentEvent) + Send + Sync>) {
        self.subscribers
            .write()
            .push((event_type.to_string(), Arc::from(callback)));
    }

    fn get_context(&self, session_id: &str) -> Result<Context, aurora_core::Error> {
        self.contexts
            .read()
            .get(session_id)
            .cloned()
            .ok_or_else(|| {
                aurora_core::Error::NotFound(format!(
                    "session context not found: {}",
                    session_id
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_agent(id: &str) -> Arc<MockAgent> {
        Arc::new(MockAgent::new(id))
    }

    fn make_tool(name: &str, agent_id: &str, caps: &[&str]) -> Tool {
        let mut t = Tool::new(name, format!("{} tool", name), serde_json::json!({}))
            .with_agent(agent_id);
        let caps: Vec<String> = caps.iter().map(|s| s.to_string()).collect();
        if !caps.is_empty() {
            t = t.with_capabilities(caps);
        }
        t
    }

    #[test]
    fn test_tool_new_and_builder() {
        let t = Tool::new("echo", "echoes input", serde_json::json!({"type": "object"}))
            .with_output_schema(serde_json::json!({"type": "string"}))
            .with_capabilities(vec!["io".to_string(), "text".to_string()])
            .with_agent("agent-1");
        assert_eq!(t.name, "echo");
        assert_eq!(t.agent_id, "agent-1");
        assert_eq!(t.capabilities.len(), 2);
        assert_eq!(t.output_schema["type"], "string");
    }

    #[test]
    fn test_tool_has_capability_case_insensitive() {
        let t = make_tool("t", "a", &["Network", "IO"]);
        assert!(t.has_capability("network"));
        assert!(t.has_capability("io"));
        assert!(t.has_capability("NETWORK"));
        assert!(!t.has_capability("math"));
    }

    #[test]
    fn test_tool_from_definition() {
        let def = ToolDefinition {
            name: "x".into(),
            description: "d".into(),
            input_schema: serde_json::json!({}),
        };
        let t: Tool = def.into();
        assert_eq!(t.name, "x");
        assert!(t.capabilities.is_empty());
        assert!(t.agent_id.is_empty());
    }

    #[test]
    fn test_tool_to_definition() {
        let t = make_tool("x", "a", &[]);
        let def = t.to_definition();
        assert_eq!(def.name, "x");
        assert_eq!(def.description, "x tool");
    }

    #[test]
    fn test_tool_result_ok_err() {
        let ok = ToolResult::ok("echo", serde_json::json!("hi"), 5);
        assert!(ok.is_ok());
        assert!(!ok.is_err());
        assert_eq!(ok.output, serde_json::json!("hi"));
        assert_eq!(ok.latency_ms, 5);

        let err = ToolResult::err("echo", "boom", 3);
        assert!(err.is_err());
        assert_eq!(err.error.as_deref(), Some("boom"));
    }

    #[test]
    fn test_registry_register_agent() {
        let r = ToolRegistry::new();
        let agent = make_agent("a1");
        r.register_agent("a1", agent as Arc<dyn AgentProtocol>);
        assert_eq!(r.agent_count(), 1);
    }

    #[test]
    fn test_registry_unregister_agent_removes_tools() {
        let r = ToolRegistry::new();
        let agent = make_agent("a1");
        r.register_agent("a1", agent as Arc<dyn AgentProtocol>);
        r.register_tool(make_tool("t1", "a1", &[])).unwrap();
        r.register_tool(make_tool("t2", "a1", &[])).unwrap();
        assert_eq!(r.tool_count(), 2);

        r.unregister_agent("a1");
        assert_eq!(r.agent_count(), 0);
        assert_eq!(r.tool_count(), 0);
    }

    #[test]
    fn test_registry_register_tool_without_agent_id_fails() {
        let r = ToolRegistry::new();
        let tool = Tool::new("t", "d", serde_json::json!({}));
        let err = r.register_tool(tool).unwrap_err();
        assert!(matches!(err, crate::Error::InvalidInput(_)));
    }

    #[test]
    fn test_registry_register_tool_with_unknown_agent_fails() {
        let r = ToolRegistry::new();
        let err = r.register_tool(make_tool("t", "ghost", &[])).unwrap_err();
        assert!(matches!(err, crate::Error::NotFound(_)));
    }

    #[test]
    fn test_registry_list_all() {
        let r = ToolRegistry::new();
        let agent = make_agent("a1");
        r.register_agent("a1", agent as Arc<dyn AgentProtocol>);
        r.register_tool(make_tool("t1", "a1", &[])).unwrap();
        r.register_tool(make_tool("t2", "a1", &[])).unwrap();
        assert_eq!(r.list_all().len(), 2);
    }

    #[test]
    fn test_registry_find_by_name() {
        let r = ToolRegistry::new();
        let agent = make_agent("a1");
        r.register_agent("a1", agent as Arc<dyn AgentProtocol>);
        r.register_tool(make_tool("t1", "a1", &[])).unwrap();
        assert!(r.find_by_name("t1").is_some());
        assert!(r.find_by_name("ghost").is_none());
    }

    #[test]
    fn test_registry_find_by_capability() {
        let r = ToolRegistry::new();
        let agent = make_agent("a1");
        r.register_agent("a1", agent as Arc<dyn AgentProtocol>);
        r.register_tool(make_tool("t1", "a1", &["network"])).unwrap();
        r.register_tool(make_tool("t2", "a1", &["io", "network"])).unwrap();
        r.register_tool(make_tool("t3", "a1", &["math"])).unwrap();

        let net = r.find_by_capability("network");
        assert_eq!(net.len(), 2);
        let io = r.find_by_capability("io");
        assert_eq!(io.len(), 1);
        let math = r.find_by_capability("math");
        assert_eq!(math.len(), 1);
    }

    #[test]
    fn test_registry_search() {
        let r = ToolRegistry::new();
        let agent = make_agent("a1");
        r.register_agent("a1", agent as Arc<dyn AgentProtocol>);
        r.register_tool(Tool::new("echo", "echo tool", serde_json::json!({})).with_agent("a1"))
            .unwrap();
        r.register_tool(Tool::new("calc", "calculator tool", serde_json::json!({})).with_agent("a1"))
            .unwrap();
        assert_eq!(r.search("echo").len(), 1);
        assert_eq!(r.search("tool").len(), 2);
        assert_eq!(r.search("nothing").len(), 0);
    }

    #[test]
    fn test_registry_unregister_tool() {
        let r = ToolRegistry::new();
        let agent = make_agent("a1");
        r.register_agent("a1", agent as Arc<dyn AgentProtocol>);
        r.register_tool(make_tool("t1", "a1", &[])).unwrap();
        let removed = r.unregister_tool("t1");
        assert!(removed.is_some());
        assert_eq!(r.tool_count(), 0);
    }

    #[test]
    fn test_registry_invoke_routes_to_agent() {
        let r = ToolRegistry::new();
        let agent = make_agent("a1");
        agent.register_handler("echo", |args| Ok(args.clone()));
        r.register_agent("a1", agent as Arc<dyn AgentProtocol>);
        r.register_tool(make_tool("echo", "a1", &[])).unwrap();

        let inv = ToolInvocation::new("echo", serde_json::json!({"msg": "hi"}), "s1");
        let result = r.invoke(&inv).unwrap();
        assert!(result.is_ok());
        assert_eq!(result.output["msg"], "hi");
        assert_eq!(result.tool_name, "echo");
    }

    #[test]
    fn test_registry_invoke_unknown_tool() {
        let r = ToolRegistry::new();
        let inv = ToolInvocation::new("ghost", serde_json::json!({}), "s1");
        let err = r.invoke(&inv).unwrap_err();
        assert!(matches!(err, crate::Error::NotFound(_)));
    }

    #[test]
    fn test_registry_invoke_handler_error_propagated() {
        let r = ToolRegistry::new();
        let agent = make_agent("a1");
        agent.register_handler("fail", |_| Err("boom".to_string()));
        r.register_agent("a1", agent as Arc<dyn AgentProtocol>);
        r.register_tool(make_tool("fail", "a1", &[])).unwrap();

        let inv = ToolInvocation::new("fail", serde_json::json!({}), "s1");
        let result = r.invoke(&inv).unwrap();
        assert!(result.is_err());
        assert_eq!(result.error.as_deref(), Some("boom"));
    }

    #[test]
    fn test_registry_invoke_no_handler_returns_not_implemented() {
        let r = ToolRegistry::new();
        let agent = make_agent("a1");
        r.register_agent("a1", agent as Arc<dyn AgentProtocol>);
        // 注册工具但不注册 handler
        r.register_tool(make_tool("t", "a1", &[])).unwrap();
        let inv = ToolInvocation::new("t", serde_json::json!({}), "s1");
        let result = r.invoke(&inv).unwrap();
        assert!(result.is_err());
        assert!(result.error.unwrap().contains("not implemented"));
    }

    #[test]
    fn test_mock_agent_register_tool() {
        let agent = MockAgent::new("a1");
        let def = ToolDefinition {
            name: "t".into(),
            description: "d".into(),
            input_schema: serde_json::json!({}),
        };
        agent.register_tool(&def).unwrap();
        assert_eq!(agent.tool_count(), 1);
    }

    #[test]
    fn test_mock_agent_execute_with_handler() {
        let agent = MockAgent::new("a1");
        agent.register_handler("echo", |args| Ok(args.clone()));
        let req = AgentRequest {
            session_id: "s1".into(),
            method: "echo".into(),
            params: serde_json::json!({"x": 1}),
        };
        let resp = agent.execute(&req).unwrap();
        assert!(resp.error.is_none());
        assert_eq!(resp.result["x"], 1);
    }

    #[test]
    fn test_mock_agent_execute_without_handler() {
        let agent = MockAgent::new("a1");
        let req = AgentRequest {
            session_id: "s1".into(),
            method: "ghost".into(),
            params: serde_json::json!({}),
        };
        let resp = agent.execute(&req).unwrap();
        assert!(resp.error.is_some());
    }

    #[test]
    fn test_mock_agent_get_context_missing() {
        let agent = MockAgent::new("a1");
        let result = agent.get_context("ghost");
        assert!(result.is_err());
    }

    #[test]
    fn test_mock_agent_get_context_present() {
        let agent = MockAgent::new("a1");
        let ctx = Context {
            session_id: "s1".into(),
            messages: vec![],
            tool_results: vec![],
            user_preferences: serde_json::json!({}),
        };
        agent.put_context(ctx);
        let got = agent.get_context("s1").unwrap();
        assert_eq!(got.session_id, "s1");
    }

    #[test]
    fn test_mock_agent_subscribe_and_emit() {
        let agent = MockAgent::new("a1");
        let counter = Arc::new(RwLock::new(0u32));
        let c = counter.clone();
        agent.subscribe(
            "tool.called",
            Box::new(move |_ev| {
                *c.write() += 1;
            }),
        );
        agent.emit_event(AgentEvent {
            event_type: "tool.called".into(),
            data: serde_json::json!({}),
        });
        agent.emit_event(AgentEvent {
            event_type: "other".into(),
            data: serde_json::json!({}),
        });
        assert_eq!(*counter.read(), 1);
    }

    #[test]
    fn test_registry_default_sandbox_config() {
        let cfg = ToolRegistry::default_sandbox_config();
        assert!(!cfg.read_only || cfg.read_only); // 仅验证可构造
        assert!(cfg.max_runtime_secs > 0);
    }
}
