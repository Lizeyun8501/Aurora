//! MCP 协议 (Model Context Protocol)
//!
//! 基于 JSON-RPC 2.0 的模型上下文协议实现，支持 `initialize` / `tools.list` /
//! `tools.call` / `resources.list` 四个核心方法，并提供 stdio 与 SSE 两种
//! 传输抽象（此处为内存 mock 实现，便于测试与离线运行）。
//!
//! # 协议结构
//! - 请求/响应：标准 JSON-RPC 2.0 报文，`id` 可为数字/字符串/null。
//! - 方法分发：`McpMethod` 枚举集中描述支持的方法，`McpServer` 负责派发。
//! - 传输层：`McpTransport` trait 抽象发送行为，`StdioTransport` 与
//!   `SseTransport` 提供可注入预置响应的内存实现。

use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

// ============================================================================
// JSON-RPC 2.0 基础类型
// ============================================================================

/// JSON-RPC 2.0 标识符类型。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum JsonRpcId {
    /// 数字标识
    Num(i64),
    /// 字符串标识
    Str(String),
    /// 通知（无 id）
    Null,
}

impl JsonRpcId {
    pub fn num(n: i64) -> Self {
        JsonRpcId::Num(n)
    }
    pub fn str(s: impl Into<String>) -> Self {
        JsonRpcId::Str(s.into())
    }
    pub fn null() -> Self {
        JsonRpcId::Null
    }
}

impl Default for JsonRpcId {
    fn default() -> Self {
        JsonRpcId::Num(0)
    }
}

/// JSON-RPC 2.0 请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    pub fn new(
        id: JsonRpcId,
        method: impl Into<String>,
        params: Option<serde_json::Value>,
    ) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }

    /// 序列化为 JSON 字符串。
    pub fn to_json(&self) -> Result<String, crate::Error> {
        serde_json::to_string(self).map_err(crate::Error::from)
    }

    /// 从 JSON 字符串反序列化。
    pub fn from_json(s: &str) -> Result<Self, crate::Error> {
        serde_json::from_str(s).map_err(crate::Error::from)
    }
}

/// JSON-RPC 2.0 错误对象。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    /// 解析错误（-32700）
    pub fn parse_error() -> Self {
        Self {
            code: -32700,
            message: "Parse error".to_string(),
            data: None,
        }
    }

    /// 无效请求（-32600）
    pub fn invalid_request() -> Self {
        Self {
            code: -32600,
            message: "Invalid Request".to_string(),
            data: None,
        }
    }

    /// 方法不存在（-32601）
    pub fn method_not_found() -> Self {
        Self {
            code: -32601,
            message: "Method not found".to_string(),
            data: None,
        }
    }

    /// 参数无效（-32602）
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
            data: None,
        }
    }

    /// 内部错误（-32603）
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: message.into(),
            data: None,
        }
    }

    /// 带附加数据的构造
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// JSON-RPC 2.0 响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// 成功响应。
    pub fn success(id: JsonRpcId, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// 错误响应。
    pub fn error(id: JsonRpcId, err: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(err),
        }
    }

    /// 是否为成功响应。
    pub fn is_success(&self) -> bool {
        self.error.is_none() && self.result.is_some()
    }

    /// 是否为错误响应。
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }

    /// 序列化为 JSON 字符串。
    pub fn to_json(&self) -> Result<String, crate::Error> {
        serde_json::to_string(self).map_err(crate::Error::from)
    }

    /// 从 JSON 字符串反序列化。
    pub fn from_json(s: &str) -> Result<Self, crate::Error> {
        serde_json::from_str(s).map_err(crate::Error::from)
    }
}

// ============================================================================
// MCP 方法
// ============================================================================

/// MCP 支持的方法集合。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum McpMethod {
    /// 握手初始化，协商协议版本与能力
    Initialize,
    /// 列出可用工具
    ToolsList,
    /// 调用某个工具
    ToolsCall,
    /// 列出可用资源
    ResourcesList,
}

impl McpMethod {
    /// 方法名（JSON-RPC method 字段）。
    pub fn as_str(&self) -> &'static str {
        match self {
            McpMethod::Initialize => "initialize",
            McpMethod::ToolsList => "tools/list",
            McpMethod::ToolsCall => "tools/call",
            McpMethod::ResourcesList => "resources/list",
        }
    }

    /// 从方法名解析为枚举。
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "initialize" => Some(McpMethod::Initialize),
            "tools/list" => Some(McpMethod::ToolsList),
            "tools/call" => Some(McpMethod::ToolsCall),
            "resources/list" => Some(McpMethod::ResourcesList),
            _ => None,
        }
    }

    /// 列出全部支持的方法名。
    pub fn all_methods() -> Vec<&'static str> {
        vec![
            McpMethod::Initialize.as_str(),
            McpMethod::ToolsList.as_str(),
            McpMethod::ToolsCall.as_str(),
            McpMethod::ResourcesList.as_str(),
        ]
    }
}

/// MCP 协议版本常量。
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// `initialize` 响应体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    pub protocol_version: String,
    pub server_info: ServerInfo,
    pub capabilities: serde_json::Value,
}

/// 服务端信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

impl ServerInfo {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
        }
    }
}

/// 工具描述（MCP 工具列表项）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

impl McpTool {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
        }
    }
}

/// `tools/list` 响应体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsListResult {
    pub tools: Vec<McpTool>,
}

/// `tools/call` 响应体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCallResult {
    pub content: Vec<serde_json::Value>,
    pub is_error: bool,
}

impl ToolsCallResult {
    pub fn ok(content: Vec<serde_json::Value>) -> Self {
        Self {
            content,
            is_error: false,
        }
    }
    pub fn err(content: Vec<serde_json::Value>) -> Self {
        Self {
            content,
            is_error: true,
        }
    }
}

/// 资源描述。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

impl McpResource {
    pub fn new(uri: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            name: name.into(),
            description: None,
            mime_type: None,
        }
    }
}

/// `resources/list` 响应体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesListResult {
    pub resources: Vec<McpResource>,
}

// ============================================================================
// 传输层
// ============================================================================

/// MCP 传输层抽象。
///
/// 实现者负责把 JSON-RPC 请求送达服务端并返回响应。生产实现可能是
/// 子进程的 stdin/stdout 管道，或 SSE 长连接；此处抽象为同步 trait
/// 以便在测试与编排器中直接调用。
pub trait McpTransport: Send + Sync {
    /// 传输名称（用于日志与诊断）。
    fn name(&self) -> &'static str;

    /// 发送一个 JSON-RPC 请求并等待响应。
    fn send(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, crate::Error>;

    /// 是否已建立连接。
    fn is_connected(&self) -> bool;

    /// 关闭传输。
    fn close(&self) -> Result<(), crate::Error>;
}

/// stdio 传输的 mock 实现。
///
/// 内部维护一个请求/响应队列：可预置响应供下一次 `send` 返回，
/// 也可注入一个处理器（handler）按方法动态生成响应。
pub struct StdioTransport {
    connected: Arc<RwLock<bool>>,
    /// 待消费的预置响应队列（FIFO）
    queued_responses: Arc<RwLock<VecDeque<JsonRpcResponse>>>,
    /// 已发送请求的历史记录（用于断言）
    sent_requests: Arc<RwLock<Vec<JsonRpcRequest>>>,
}

impl Default for StdioTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl StdioTransport {
    pub fn new() -> Self {
        Self {
            connected: Arc::new(RwLock::new(true)),
            queued_responses: Arc::new(RwLock::new(VecDeque::new())),
            sent_requests: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 预置一个响应，将按 FIFO 顺序返回。
    pub fn enqueue_response(&self, response: JsonRpcResponse) {
        self.queued_responses.write().push_back(response);
    }

    /// 已发送请求快照。
    pub fn sent_requests(&self) -> Vec<JsonRpcRequest> {
        self.sent_requests.read().clone()
    }

    /// 设置连接状态。
    pub fn set_connected(&self, connected: bool) {
        *self.connected.write() = connected;
    }

    /// 已预置但尚未消费的响应数。
    pub fn pending_responses(&self) -> usize {
        self.queued_responses.read().len()
    }
}

impl McpTransport for StdioTransport {
    fn name(&self) -> &'static str {
        "stdio"
    }

    fn send(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, crate::Error> {
        if !*self.connected.read() {
            return Err(crate::Error::Transport("stdio transport not connected".into()));
        }
        debug!(
            "stdio transport: send method={} id={:?}",
            request.method, request.id
        );
        self.sent_requests.write().push(request.clone());
        match self.queued_responses.write().pop_front() {
            Some(resp) => Ok(resp),
            None => Ok(JsonRpcResponse::error(
                request.id.clone(),
                JsonRpcError::internal_error("no queued response available"),
            )),
        }
    }

    fn is_connected(&self) -> bool {
        *self.connected.read()
    }

    fn close(&self) -> Result<(), crate::Error> {
        *self.connected.write() = false;
        Ok(())
    }
}

/// SSE 传输的 mock 实现。
///
/// 与 `StdioTransport` 类似的内存实现，但语义上模拟服务端推送：
/// `enqueue_response` 表示服务端即将推送的事件，`send` 表示客户端
/// 发起请求后等待下一个推送事件。
pub struct SseTransport {
    connected: Arc<RwLock<bool>>,
    endpoint: Arc<RwLock<String>>,
    queued_events: Arc<RwLock<VecDeque<JsonRpcResponse>>>,
    sent_requests: Arc<RwLock<Vec<JsonRpcRequest>>>,
}

impl Default for SseTransport {
    fn default() -> Self {
        Self::new("http://localhost:8080/sse")
    }
}

impl SseTransport {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            connected: Arc::new(RwLock::new(true)),
            endpoint: Arc::new(RwLock::new(endpoint.into())),
            queued_events: Arc::new(RwLock::new(VecDeque::new())),
            sent_requests: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 预置一个待推送事件。
    pub fn enqueue_event(&self, response: JsonRpcResponse) {
        self.queued_events.write().push_back(response);
    }

    /// 已发送请求快照。
    pub fn sent_requests(&self) -> Vec<JsonRpcRequest> {
        self.sent_requests.read().clone()
    }

    /// 设置连接状态。
    pub fn set_connected(&self, connected: bool) {
        *self.connected.write() = connected;
    }

    /// 当前 SSE 端点。
    pub fn endpoint(&self) -> String {
        self.endpoint.read().clone()
    }
}

impl McpTransport for SseTransport {
    fn name(&self) -> &'static str {
        "sse"
    }

    fn send(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, crate::Error> {
        if !*self.connected.read() {
            return Err(crate::Error::Transport("sse transport not connected".into()));
        }
        debug!(
            "sse transport: send method={} id={:?} endpoint={}",
            request.method,
            request.id,
            self.endpoint.read()
        );
        self.sent_requests.write().push(request.clone());
        match self.queued_events.write().pop_front() {
            Some(resp) => Ok(resp),
            None => Ok(JsonRpcResponse::error(
                request.id.clone(),
                JsonRpcError::internal_error("no queued event available"),
            )),
        }
    }

    fn is_connected(&self) -> bool {
        *self.connected.read()
    }

    fn close(&self) -> Result<(), crate::Error> {
        *self.connected.write() = false;
        Ok(())
    }
}

// ============================================================================
// MCP 客户端
// ============================================================================

/// MCP 客户端：封装一个传输层，提供高层方法。
pub struct McpClient {
    transport: Arc<dyn McpTransport>,
    next_id: Arc<RwLock<i64>>,
}

impl McpClient {
    pub fn new(transport: Arc<dyn McpTransport>) -> Self {
        Self {
            transport,
            next_id: Arc::new(RwLock::new(1)),
        }
    }

    /// 分配下一个请求 id。
    fn next_id(&self) -> JsonRpcId {
        let mut id = self.next_id.write();
        let current = *id;
        *id += 1;
        JsonRpcId::Num(current)
    }

    /// 发起 `initialize` 握手。
    pub fn initialize(
        &self,
        client_info: &ServerInfo,
    ) -> Result<InitializeResult, crate::Error> {
        let params = serde_json::json!({
            "protocol_version": MCP_PROTOCOL_VERSION,
            "client_info": client_info,
            "capabilities": {},
        });
        let request = JsonRpcRequest::new(
            self.next_id(),
            McpMethod::Initialize.as_str(),
            Some(params),
        );
        let resp = self.transport.send(&request)?;
        if let Some(err) = resp.error {
            return Err(crate::Error::JsonRpc(err.message));
        }
        let result = resp.result.ok_or_else(|| {
            crate::Error::JsonRpc("initialize response missing result".into())
        })?;
        serde_json::from_value(result).map_err(crate::Error::from)
    }

    /// 列出工具。
    pub fn tools_list(&self) -> Result<ToolsListResult, crate::Error> {
        let request =
            JsonRpcRequest::new(self.next_id(), McpMethod::ToolsList.as_str(), None);
        let resp = self.transport.send(&request)?;
        if let Some(err) = resp.error {
            return Err(crate::Error::JsonRpc(err.message));
        }
        let result = resp
            .result
            .ok_or_else(|| crate::Error::JsonRpc("tools/list response missing result".into()))?;
        serde_json::from_value(result).map_err(crate::Error::from)
    }

    /// 调用工具。
    pub fn tools_call(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolsCallResult, crate::Error> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments,
        });
        let request = JsonRpcRequest::new(
            self.next_id(),
            McpMethod::ToolsCall.as_str(),
            Some(params),
        );
        let resp = self.transport.send(&request)?;
        if let Some(err) = resp.error {
            return Err(crate::Error::JsonRpc(err.message));
        }
        let result = resp
            .result
            .ok_or_else(|| crate::Error::JsonRpc("tools/call response missing result".into()))?;
        serde_json::from_value(result).map_err(crate::Error::from)
    }

    /// 列出资源。
    pub fn resources_list(&self) -> Result<ResourcesListResult, crate::Error> {
        let request = JsonRpcRequest::new(
            self.next_id(),
            McpMethod::ResourcesList.as_str(),
            None,
        );
        let resp = self.transport.send(&request)?;
        if let Some(err) = resp.error {
            return Err(crate::Error::JsonRpc(err.message));
        }
        let result = resp.result.ok_or_else(|| {
            crate::Error::JsonRpc("resources/list response missing result".into())
        })?;
        serde_json::from_value(result).map_err(crate::Error::from)
    }

    /// 底层传输引用。
    pub fn transport(&self) -> &Arc<dyn McpTransport> {
        &self.transport
    }
}

// ============================================================================
// MCP 服务端（dispatch helper）
// ============================================================================

/// MCP 服务端：根据方法名将 `JsonRpcRequest` 派发为 `JsonRpcResponse`。
///
/// 这是一个轻量级派发器，不直接持有工具实现，而是接受外部处理函数。
/// 主要用于测试与协议层验证。
pub struct McpServer {
    server_info: ServerInfo,
    tools: Arc<RwLock<Vec<McpTool>>>,
    resources: Arc<RwLock<Vec<McpResource>>>,
}

impl McpServer {
    pub fn new(server_info: ServerInfo) -> Self {
        Self {
            server_info,
            tools: Arc::new(RwLock::new(Vec::new())),
            resources: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 注册一个工具。
    pub fn register_tool(&self, tool: McpTool) {
        self.tools.write().push(tool);
    }

    /// 注册一个资源。
    pub fn register_resource(&self, resource: McpResource) {
        self.resources.write().push(resource);
    }

    /// 处理一个请求，返回响应。
    pub fn handle(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        let method = match McpMethod::from_str(&request.method) {
            Some(m) => m,
            None => {
                warn!("mcp server: unknown method {}", request.method);
                return JsonRpcResponse::error(request.id.clone(), JsonRpcError::method_not_found());
            }
        };
        debug!("mcp server: dispatch method={:?}", method);

        match method {
            McpMethod::Initialize => {
                let result = InitializeResult {
                    protocol_version: MCP_PROTOCOL_VERSION.to_string(),
                    server_info: self.server_info.clone(),
                    capabilities: serde_json::json!({
                        "tools": {},
                        "resources": {},
                    }),
                };
                JsonRpcResponse::success(
                    request.id.clone(),
                    serde_json::to_value(&result).unwrap_or_default(),
                )
            }
            McpMethod::ToolsList => {
                let result = ToolsListResult {
                    tools: self.tools.read().clone(),
                };
                JsonRpcResponse::success(
                    request.id.clone(),
                    serde_json::to_value(&result).unwrap_or_default(),
                )
            }
            McpMethod::ToolsCall => {
                // 默认行为：返回一个空内容结果（实际工具执行由调用方注入）
                JsonRpcResponse::success(
                    request.id.clone(),
                    serde_json::to_value(&ToolsCallResult::ok(vec![serde_json::json!({
                        "type": "text",
                        "text": "ok",
                    })]))
                    .unwrap_or_default(),
                )
            }
            McpMethod::ResourcesList => {
                let result = ResourcesListResult {
                    resources: self.resources.read().clone(),
                };
                JsonRpcResponse::success(
                    request.id.clone(),
                    serde_json::to_value(&result).unwrap_or_default(),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_response(id: JsonRpcId, result: serde_json::Value) -> JsonRpcResponse {
        JsonRpcResponse::success(id, result)
    }

    #[test]
    fn test_jsonrpc_id_variants() {
        assert_eq!(JsonRpcId::num(1), JsonRpcId::Num(1));
        assert_eq!(JsonRpcId::str("abc"), JsonRpcId::Str("abc".into()));
        assert_eq!(JsonRpcId::null(), JsonRpcId::Null);
        assert_eq!(JsonRpcId::default(), JsonRpcId::Num(0));
    }

    #[test]
    fn test_jsonrpc_request_new() {
        let req = JsonRpcRequest::new(
            JsonRpcId::num(1),
            "tools/list",
            Some(serde_json::json!({"a": 1})),
        );
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "tools/list");
        assert_eq!(req.id, JsonRpcId::num(1));
        assert!(req.params.is_some());
    }

    #[test]
    fn test_jsonrpc_request_serialization() {
        let req = JsonRpcRequest::new(JsonRpcId::num(1), "initialize", None);
        let json = req.to_json().unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"initialize\""));
        // params 应当被跳过（skip_serializing_if）
        assert!(!json.contains("params"));

        let back = JsonRpcRequest::from_json(&json).unwrap();
        assert_eq!(back.method, req.method);
        assert_eq!(back.id, req.id);
    }

    #[test]
    fn test_jsonrpc_request_with_params_serialization() {
        let req = JsonRpcRequest::new(
            JsonRpcId::str("req-1"),
            "tools/call",
            Some(serde_json::json!({"name": "echo"})),
        );
        let json = req.to_json().unwrap();
        assert!(json.contains("\"id\":\"req-1\""));
        assert!(json.contains("\"name\":\"echo\""));
    }

    #[test]
    fn test_jsonrpc_request_from_json_invalid() {
        let result = JsonRpcRequest::from_json("{ not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_jsonrpc_error_codes() {
        assert_eq!(JsonRpcError::parse_error().code, -32700);
        assert_eq!(JsonRpcError::invalid_request().code, -32600);
        assert_eq!(JsonRpcError::method_not_found().code, -32601);
        assert_eq!(JsonRpcError::invalid_params("bad").code, -32602);
        assert_eq!(JsonRpcError::internal_error("oops").code, -32603);
    }

    #[test]
    fn test_jsonrpc_error_with_data() {
        let err = JsonRpcError::invalid_params("missing field")
            .with_data(serde_json::json!({"field": "name"}));
        assert!(err.data.is_some());
        let data = err.data.unwrap();
        assert_eq!(data["field"], "name");
    }

    #[test]
    fn test_jsonrpc_response_success() {
        let resp = ok_response(JsonRpcId::num(1), serde_json::json!({"ok": true}));
        assert!(resp.is_success());
        assert!(!resp.is_error());
        assert_eq!(resp.result.unwrap()["ok"], true);
    }

    #[test]
    fn test_jsonrpc_response_error() {
        let resp = JsonRpcResponse::error(
            JsonRpcId::num(2),
            JsonRpcError::method_not_found(),
        );
        assert!(!resp.is_success());
        assert!(resp.is_error());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[test]
    fn test_jsonrpc_response_serialization() {
        let resp = ok_response(JsonRpcId::num(1), serde_json::json!("hello"));
        let json = resp.to_json().unwrap();
        // 错误字段应被跳过
        assert!(!json.contains("error"));
        assert!(json.contains("\"result\":\"hello\""));

        let back = JsonRpcResponse::from_json(&json).unwrap();
        assert!(back.is_success());
    }

    #[test]
    fn test_mcp_method_as_str() {
        assert_eq!(McpMethod::Initialize.as_str(), "initialize");
        assert_eq!(McpMethod::ToolsList.as_str(), "tools/list");
        assert_eq!(McpMethod::ToolsCall.as_str(), "tools/call");
        assert_eq!(McpMethod::ResourcesList.as_str(), "resources/list");
    }

    #[test]
    fn test_mcp_method_from_str_roundtrip() {
        for method in [
            McpMethod::Initialize,
            McpMethod::ToolsList,
            McpMethod::ToolsCall,
            McpMethod::ResourcesList,
        ] {
            let s = method.as_str();
            assert_eq!(McpMethod::from_str(s), Some(method));
        }
        assert_eq!(McpMethod::from_str("unknown"), None);
    }

    #[test]
    fn test_mcp_method_all_methods() {
        let all = McpMethod::all_methods();
        assert_eq!(all.len(), 4);
        assert!(all.contains(&"initialize"));
        assert!(all.contains(&"tools/call"));
    }

    #[test]
    fn test_stdio_transport_basic() {
        let t = StdioTransport::new();
        assert_eq!(t.name(), "stdio");
        assert!(t.is_connected());

        t.enqueue_response(ok_response(JsonRpcId::num(1), serde_json::json!({"x": 1})));
        let req = JsonRpcRequest::new(JsonRpcId::num(1), "ping", None);
        let resp = t.send(&req).unwrap();
        assert!(resp.is_success());
        assert_eq!(resp.result.unwrap()["x"], 1);

        // 请求历史应记录
        let sent = t.sent_requests();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].method, "ping");
    }

    #[test]
    fn test_stdio_transport_no_queued_response() {
        let t = StdioTransport::new();
        let req = JsonRpcRequest::new(JsonRpcId::num(1), "ping", None);
        let resp = t.send(&req).unwrap();
        assert!(resp.is_error());
        assert_eq!(resp.error.unwrap().code, -32603);
    }

    #[test]
    fn test_stdio_transport_not_connected() {
        let t = StdioTransport::new();
        t.set_connected(false);
        let req = JsonRpcRequest::new(JsonRpcId::num(1), "ping", None);
        let err = t.send(&req).unwrap_err();
        assert!(matches!(err, crate::Error::Transport(_)));
    }

    #[test]
    fn test_stdio_transport_close() {
        let t = StdioTransport::new();
        assert!(t.is_connected());
        t.close().unwrap();
        assert!(!t.is_connected());
    }

    #[test]
    fn test_stdio_transport_multiple_responses_fifo() {
        let t = StdioTransport::new();
        t.enqueue_response(ok_response(JsonRpcId::num(1), serde_json::json!(1)));
        t.enqueue_response(ok_response(JsonRpcId::num(2), serde_json::json!(2)));

        let r1 = t.send(&JsonRpcRequest::new(JsonRpcId::num(1), "a", None)).unwrap();
        let r2 = t.send(&JsonRpcRequest::new(JsonRpcId::num(2), "b", None)).unwrap();
        assert_eq!(r1.result.unwrap(), serde_json::json!(1));
        assert_eq!(r2.result.unwrap(), serde_json::json!(2));
        assert_eq!(t.pending_responses(), 0);
    }

    #[test]
    fn test_sse_transport_basic() {
        let t = SseTransport::new("http://localhost:9999/sse");
        assert_eq!(t.name(), "sse");
        assert!(t.is_connected());
        assert_eq!(t.endpoint(), "http://localhost:9999/sse");

        t.enqueue_event(ok_response(JsonRpcId::num(1), serde_json::json!({"event": "open"})));
        let req = JsonRpcRequest::new(JsonRpcId::num(1), "subscribe", None);
        let resp = t.send(&req).unwrap();
        assert!(resp.is_success());
        assert_eq!(resp.result.unwrap()["event"], "open");

        let sent = t.sent_requests();
        assert_eq!(sent.len(), 1);
    }

    #[test]
    fn test_sse_transport_not_connected() {
        let t = SseTransport::new("http://localhost/sse");
        t.set_connected(false);
        let req = JsonRpcRequest::new(JsonRpcId::num(1), "x", None);
        assert!(t.send(&req).is_err());
    }

    #[test]
    fn test_sse_transport_close() {
        let t = SseTransport::default();
        t.close().unwrap();
        assert!(!t.is_connected());
    }

    #[test]
    fn test_sse_transport_no_event_returns_internal_error() {
        let t = SseTransport::default();
        let req = JsonRpcRequest::new(JsonRpcId::num(1), "x", None);
        let resp = t.send(&req).unwrap();
        assert!(resp.is_error());
        assert_eq!(resp.error.unwrap().code, -32603);
    }

    #[test]
    fn test_mcp_client_initialize() {
        let stdio: Arc<StdioTransport> = Arc::new(StdioTransport::new());
        stdio.enqueue_response(ok_response(
            JsonRpcId::num(1),
            serde_json::json!({
                "protocol_version": MCP_PROTOCOL_VERSION,
                "server_info": {"name": "test-server", "version": "0.1.0"},
                "capabilities": {"tools": {}}
            }),
        ));
        let client = McpClient::new(stdio);
        let info = ServerInfo::new("test-client", "0.1.0");
        let result = client.initialize(&info).unwrap();
        assert_eq!(result.protocol_version, MCP_PROTOCOL_VERSION);
        assert_eq!(result.server_info.name, "test-server");
    }

    #[test]
    fn test_mcp_client_tools_list() {
        let stdio = Arc::new(StdioTransport::new());
        stdio.enqueue_response(ok_response(
            JsonRpcId::num(1),
            serde_json::json!({
                "tools": [
                    {"name": "echo", "description": "echo back", "input_schema": {}}
                ]
            }),
        ));
        let client = McpClient::new(stdio);
        let result = client.tools_list().unwrap();
        assert_eq!(result.tools.len(), 1);
        assert_eq!(result.tools[0].name, "echo");
    }

    #[test]
    fn test_mcp_client_tools_call() {
        let stdio = Arc::new(StdioTransport::new());
        stdio.enqueue_response(ok_response(
            JsonRpcId::num(1),
            serde_json::json!({
                "content": [{"type": "text", "text": "echoed"}],
                "is_error": false
            }),
        ));
        let client = McpClient::new(stdio);
        let result = client.tools_call("echo", serde_json::json!({"msg": "hi"})).unwrap();
        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);
        assert_eq!(result.content[0]["text"], "echoed");
    }

    #[test]
    fn test_mcp_client_resources_list() {
        let stdio = Arc::new(StdioTransport::new());
        stdio.enqueue_response(ok_response(
            JsonRpcId::num(1),
            serde_json::json!({
                "resources": [
                    {"uri": "file:///a.txt", "name": "a", "description": null, "mime_type": null}
                ]
            }),
        ));
        let client = McpClient::new(stdio);
        let result = client.resources_list().unwrap();
        assert_eq!(result.resources.len(), 1);
        assert_eq!(result.resources[0].uri, "file:///a.txt");
    }

    #[test]
    fn test_mcp_client_error_response() {
        let stdio = Arc::new(StdioTransport::new());
        stdio.enqueue_response(JsonRpcResponse::error(
            JsonRpcId::num(1),
            JsonRpcError::method_not_found(),
        ));
        let client = McpClient::new(stdio);
        let err = client.tools_list().unwrap_err();
        assert!(matches!(err, crate::Error::JsonRpc(_)));
    }

    #[test]
    fn test_mcp_client_id_increments() {
        let stdio = Arc::new(StdioTransport::new());
        // 保留一个具名 clone 用于事后检查 sent_requests
        let probe = stdio.clone();
        stdio.enqueue_response(ok_response(JsonRpcId::num(1), serde_json::json!({"tools": []})));
        stdio.enqueue_response(ok_response(JsonRpcId::num(2), serde_json::json!({"tools": []})));
        let client = McpClient::new(stdio);
        client.tools_list().unwrap();
        client.tools_list().unwrap();
        // 两次调用应生成两个递增 id（1, 2）
        let sent = probe.sent_requests();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].id, JsonRpcId::num(1));
        assert_eq!(sent[1].id, JsonRpcId::num(2));
    }

    #[test]
    fn test_mcp_server_initialize() {
        let server = McpServer::new(ServerInfo::new("srv", "1.0.0"));
        let req = JsonRpcRequest::new(
            JsonRpcId::num(1),
            McpMethod::Initialize.as_str(),
            Some(serde_json::json!({"protocol_version": MCP_PROTOCOL_VERSION})),
        );
        let resp = server.handle(&req);
        assert!(resp.is_success());
        let result = resp.result.unwrap();
        assert_eq!(result["protocol_version"], MCP_PROTOCOL_VERSION);
        assert_eq!(result["server_info"]["name"], "srv");
    }

    #[test]
    fn test_mcp_server_tools_list() {
        let server = McpServer::new(ServerInfo::new("srv", "1.0.0"));
        server.register_tool(McpTool::new("echo", "echo tool", serde_json::json!({})));
        server.register_tool(McpTool::new("calc", "calc tool", serde_json::json!({})));

        let req = JsonRpcRequest::new(JsonRpcId::num(1), McpMethod::ToolsList.as_str(), None);
        let resp = server.handle(&req);
        assert!(resp.is_success());
        let result = resp.result.unwrap();
        assert_eq!(result["tools"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_mcp_server_resources_list() {
        let server = McpServer::new(ServerInfo::new("srv", "1.0.0"));
        server.register_resource(McpResource::new("file:///a", "a"));
        let req = JsonRpcRequest::new(JsonRpcId::num(1), McpMethod::ResourcesList.as_str(), None);
        let resp = server.handle(&req);
        assert!(resp.is_success());
        let result = resp.result.unwrap();
        assert_eq!(result["resources"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_mcp_server_unknown_method() {
        let server = McpServer::new(ServerInfo::new("srv", "1.0.0"));
        let req = JsonRpcRequest::new(JsonRpcId::num(1), "ghost/method", None);
        let resp = server.handle(&req);
        assert!(resp.is_error());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[test]
    fn test_mcp_server_tools_call_default() {
        let server = McpServer::new(ServerInfo::new("srv", "1.0.0"));
        let req = JsonRpcRequest::new(
            JsonRpcId::num(1),
            McpMethod::ToolsCall.as_str(),
            Some(serde_json::json!({"name": "echo", "arguments": {}})),
        );
        let resp = server.handle(&req);
        assert!(resp.is_success());
        let result = resp.result.unwrap();
        assert_eq!(result["is_error"], false);
    }

    #[test]
    fn test_mcp_resource_new() {
        let r = McpResource::new("file:///x", "x");
        assert_eq!(r.uri, "file:///x");
        assert_eq!(r.name, "x");
        assert!(r.description.is_none());
        assert!(r.mime_type.is_none());
    }

    #[test]
    fn test_tools_call_result_variants() {
        let ok = ToolsCallResult::ok(vec![serde_json::json!("hi")]);
        assert!(!ok.is_error);
        let err = ToolsCallResult::err(vec![serde_json::json!("boom")]);
        assert!(err.is_error);
    }

    #[test]
    fn test_server_info_new() {
        let info = ServerInfo::new("name", "1.2.3");
        assert_eq!(info.name, "name");
        assert_eq!(info.version, "1.2.3");
    }
}
