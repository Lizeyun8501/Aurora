//! iframe 插件隔离 (iframe Plugin Isolation)
//!
//! 基于 sandbox iframe + `postMessage` 实现 JSON-RPC 2.0 双向通信，
//! 并通过 CSS 变量透传主题令牌（theme tokens），保证插件 UI 与宿主主题一致。
//!
//! # 沙箱模型
//! 每个 iframe 以 `sandbox` 属性受限运行（默认禁用同源脚本、表单、弹窗等），
//! 仅显式放行所需能力（如 `allow-scripts`）。宿主与插件之间仅通过
//! 结构化克隆的 JSON-RPC 消息通信，杜绝直接 DOM 访问。

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// JSON-RPC 2.0 标识符类型。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum JsonRpcId {
    Num(i64),
    Str(String),
    Null,
}

impl JsonRpcId {
    pub fn num(n: i64) -> Self {
        JsonRpcId::Num(n)
    }
    pub fn str(s: impl Into<String>) -> Self {
        JsonRpcId::Str(s.into())
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
}

/// JSON-RPC 2.0 错误对象。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
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
    pub fn success(id: JsonRpcId, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

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
        self.error.is_none()
    }
}

/// 主题令牌集合：通过 CSS 变量透传给 iframe。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CssTheme {
    pub name: String,
    pub tokens: HashMap<String, String>,
}

impl CssTheme {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tokens: HashMap::new(),
        }
    }

    /// 预定义亮色主题。
    pub fn light() -> Self {
        let mut tokens = HashMap::new();
        tokens.insert("--bg".to_string(), "#ffffff".to_string());
        tokens.insert("--fg".to_string(), "#1f2328".to_string());
        tokens.insert("--accent".to_string(), "#0969da".to_string());
        tokens.insert("--border".to_string(), "#d0d7de".to_string());
        tokens.insert("--muted".to_string(), "#656d76".to_string());
        Self {
            name: "light".to_string(),
            tokens,
        }
    }

    /// 预定义暗色主题。
    pub fn dark() -> Self {
        let mut tokens = HashMap::new();
        tokens.insert("--bg".to_string(), "#0d1117".to_string());
        tokens.insert("--fg".to_string(), "#e6edf3".to_string());
        tokens.insert("--accent".to_string(), "#2f81f7".to_string());
        tokens.insert("--border".to_string(), "#30363d".to_string());
        tokens.insert("--muted".to_string(), "#7d8590".to_string());
        Self {
            name: "dark".to_string(),
            tokens,
        }
    }

    /// 读取某令牌值。
    pub fn get(&self, name: &str) -> Option<&str> {
        self.tokens.get(name).map(|s| s.as_str())
    }

    /// 设置令牌值。
    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.tokens.insert(name.into(), value.into());
    }

    /// 转换为 CSS 变量声明字符串，例如：
    /// `:root { --bg: #ffffff; --fg: #1f2328; }`
    pub fn to_css_variables(&self) -> String {
        let mut entries: Vec<(&String, &String)> = self.tokens.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        let body: Vec<String> = entries
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect();
        format!(":root {{ {} }}", body.join("; "))
    }

    /// 转换为 JSON 对象（用于 postMessage 透传）。
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.tokens).unwrap_or(serde_json::Value::Null)
    }
}

/// 单个 iframe 框架的注册信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IframeFrame {
    pub id: String,
    pub src: String,
    /// sandbox 放行令牌（如 `allow-scripts`、`allow-same-origin`）
    pub sandbox: Vec<String>,
    pub theme: CssTheme,
}

impl IframeFrame {
    /// 渲染为 `<iframe sandbox="...">` 的 sandbox 属性值。
    pub fn sandbox_attr(&self) -> String {
        if self.sandbox.is_empty() {
            // 空 sandbox 表示最严格：完全不放开
            String::new()
        } else {
            self.sandbox.join(" ")
        }
    }
}

/// iframe 插件运行时：管理 sandbox 框架、postMessage 队列与主题透传。
pub struct IframeRuntime {
    frames: Arc<RwLock<HashMap<String, IframeFrame>>>,
    /// 模拟 postMessage 收件箱：每个框架一个待处理请求队列。
    inbox: Arc<RwLock<HashMap<String, VecDeque<JsonRpcRequest>>>>,
    theme: Arc<RwLock<CssTheme>>,
    next_id: AtomicI64,
}

impl IframeRuntime {
    /// 使用指定初始主题构造。
    pub fn new(theme: CssTheme) -> Self {
        Self {
            frames: Arc::new(RwLock::new(HashMap::new())),
            inbox: Arc::new(RwLock::new(HashMap::new())),
            theme: Arc::new(RwLock::new(theme)),
            next_id: AtomicI64::new(1),
        }
    }

    /// 使用亮色主题构造。
    pub fn with_light_theme() -> Self {
        Self::new(CssTheme::light())
    }

    /// 注册一个 iframe 框架，默认放行 `allow-scripts`。
    pub fn register(&self, id: impl Into<String>, src: impl Into<String>) {
        let id = id.into();
        let theme = self.theme.read().clone();
        let frame = IframeFrame {
            id: id.clone(),
            src: src.into(),
            sandbox: vec!["allow-scripts".to_string()],
            theme,
        };
        debug!("iframe runtime: register frame {}", id);
        self.frames.write().insert(id, frame);
    }

    /// 注销框架。
    pub fn unregister(&self, id: &str) {
        self.frames.write().remove(id);
        self.inbox.write().remove(id);
    }

    /// 是否已注册。
    pub fn is_registered(&self, id: &str) -> bool {
        self.frames.read().contains_key(id)
    }

    /// 返回框架信息。
    pub fn frame(&self, id: &str) -> Option<IframeFrame> {
        self.frames.read().get(id).cloned()
    }

    /// 当前全局主题快照。
    pub fn theme(&self) -> CssTheme {
        self.theme.read().clone()
    }

    /// 更新全局主题并广播到所有已注册框架。
    pub fn update_theme(&self, theme: CssTheme) {
        debug!("iframe runtime: update theme -> {}", theme.name);
        *self.theme.write() = theme.clone();
        let mut frames = self.frames.write();
        for frame in frames.values_mut() {
            frame.theme = theme.clone();
        }
    }

    /// 返回某框架的 CSS 变量声明（主题透传）。
    pub fn css_variables(&self, id: &str) -> Option<String> {
        Some(self.frames.read().get(id)?.theme.to_css_variables())
    }

    /// 分配下一个请求 id。
    fn next_id(&self) -> JsonRpcId {
        JsonRpcId::Num(self.next_id.fetch_add(1, Ordering::SeqCst))
    }

    /// 发送 postMessage 请求到框架收件箱。
    pub fn send(&self, id: &str, request: JsonRpcRequest) -> Result<(), crate::Error> {
        if !self.is_registered(id) {
            return Err(crate::Error::NotFound(format!(
                "iframe not registered: {}",
                id
            )));
        }
        self.inbox
            .write()
            .entry(id.to_string())
            .or_default()
            .push_back(request);
        Ok(())
    }

    /// 从框架收件箱取出下一条请求（模拟框架侧消费）。
    pub fn recv(&self, id: &str) -> Option<JsonRpcRequest> {
        self.inbox.write().get_mut(id).and_then(|q| q.pop_front())
    }

    /// 模拟框架侧处理请求并生成响应。
    ///
    /// 内置方法：
    /// - `ping` → `"pong"`
    /// - `echo` → 原样回显 `params`
    /// - `get_theme` → 主题令牌对象（CSS 变量透传）
    /// - `get_sandbox` → 框架 sandbox 令牌
    /// - 其他 → `-32601 Method not found`
    pub fn handle_request(&self, id: &str, request: &JsonRpcRequest) -> JsonRpcResponse {
        let frame = match self.frame(id) {
            Some(f) => f,
            None => {
                return JsonRpcResponse::error(
                    request.id.clone(),
                    JsonRpcError::invalid_params("frame not found"),
                );
            }
        };
        match request.method.as_str() {
            "ping" => JsonRpcResponse::success(request.id.clone(), serde_json::json!("pong")),
            "echo" => JsonRpcResponse::success(
                request.id.clone(),
                request.params.clone().unwrap_or(serde_json::Value::Null),
            ),
            "get_theme" => JsonRpcResponse::success(request.id.clone(), frame.theme.to_json()),
            "get_sandbox" => {
                JsonRpcResponse::success(request.id.clone(), serde_json::json!(frame.sandbox))
            }
            other => {
                warn!("iframe {}: unknown method `{}`", id, other);
                JsonRpcResponse::error(request.id.clone(), JsonRpcError::method_not_found())
            }
        }
    }

    /// 同步请求-响应往返：发送请求并立即模拟框架处理返回响应。
    pub fn request(
        &self,
        id: &str,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<JsonRpcResponse, crate::Error> {
        let rpc_id = self.next_id();
        let request = JsonRpcRequest::new(rpc_id, method, params);
        self.send(id, request.clone())?;
        let dequeued = self
            .recv(id)
            .ok_or_else(|| crate::Error::JsonRpc("message lost in transit".into()))?;
        if dequeued.id != request.id {
            return Err(crate::Error::JsonRpc("request/response id mismatch".into()));
        }
        Ok(self.handle_request(id, &dequeued))
    }

    /// 便捷调用：执行 JSON-RPC 并直接返回 result，失败返回错误。
    pub fn invoke(
        &self,
        id: &str,
        method: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, crate::Error> {
        let resp = self.request(id, method, Some(args.clone()))?;
        match resp.error {
            Some(e) => Err(crate::Error::JsonRpc(format!("{}: {}", e.code, e.message))),
            None => Ok(resp.result.unwrap_or(serde_json::Value::Null)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonrpc_request_response_serialize() {
        let req = JsonRpcRequest::new(JsonRpcId::Num(1), "ping", None);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"ping\""));
        // params 为 None 时应被跳过
        assert!(!json.contains("params"));

        let back: JsonRpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.method, "ping");
        assert_eq!(back.id, JsonRpcId::Num(1));
    }

    #[test]
    fn test_jsonrpc_success_and_error() {
        let ok = JsonRpcResponse::success(JsonRpcId::str("a"), serde_json::json!({"v": 1}));
        assert!(ok.is_success());
        assert_eq!(ok.result.unwrap()["v"], serde_json::json!(1));

        let err = JsonRpcResponse::error(JsonRpcId::Num(2), JsonRpcError::method_not_found());
        assert!(!err.is_success());
        assert_eq!(err.error.unwrap().code, -32601);
    }

    #[test]
    fn test_jsonrpc_id_untagged_variants() {
        let n: JsonRpcId = serde_json::from_str("5").unwrap();
        assert_eq!(n, JsonRpcId::Num(5));
        let s: JsonRpcId = serde_json::from_str("\"abc\"").unwrap();
        assert_eq!(s, JsonRpcId::Str("abc".to_string()));
        let null: JsonRpcId = serde_json::from_str("null").unwrap();
        assert_eq!(null, JsonRpcId::Null);
    }

    #[test]
    fn test_css_theme_light_dark() {
        let light = CssTheme::light();
        let dark = CssTheme::dark();
        assert_eq!(light.name, "light");
        assert_eq!(dark.name, "dark");
        assert_ne!(light.get("--bg"), dark.get("--bg"));
        assert_eq!(light.get("--bg"), Some("#ffffff"));
        assert_eq!(dark.get("--bg"), Some("#0d1117"));
    }

    #[test]
    fn test_css_theme_to_css_variables() {
        let mut theme = CssTheme::new("custom");
        theme.set("--bg", "#fff");
        theme.set("--fg", "#000");
        let css = theme.to_css_variables();
        assert!(css.starts_with(":root {"));
        assert!(css.contains("--bg: #fff"));
        assert!(css.contains("--fg: #000"));
        assert!(css.ends_with(" }"));
    }

    #[test]
    fn test_css_theme_to_json() {
        let theme = CssTheme::light();
        let json = theme.to_json();
        assert_eq!(json["--bg"], serde_json::json!("#ffffff"));
    }

    #[test]
    fn test_iframe_register_and_sandbox() {
        let rt = IframeRuntime::with_light_theme();
        rt.register("p1", "https://example.com/plugin.html");
        assert!(rt.is_registered("p1"));

        let frame = rt.frame("p1").unwrap();
        assert_eq!(frame.src, "https://example.com/plugin.html");
        assert_eq!(frame.sandbox, vec!["allow-scripts".to_string()]);
        assert_eq!(frame.sandbox_attr(), "allow-scripts");
        // 框架主题应继承全局主题
        assert_eq!(frame.theme.name, "light");
    }

    #[test]
    fn test_iframe_send_recv_queue() {
        let rt = IframeRuntime::with_light_theme();
        rt.register("p1", "about:blank");
        let r1 = JsonRpcRequest::new(JsonRpcId::Num(1), "ping", None);
        let r2 = JsonRpcRequest::new(JsonRpcId::Num(2), "echo", None);
        rt.send("p1", r1).unwrap();
        rt.send("p1", r2).unwrap();

        assert_eq!(rt.recv("p1").unwrap().id, JsonRpcId::Num(1));
        assert_eq!(rt.recv("p1").unwrap().id, JsonRpcId::Num(2));
        assert!(rt.recv("p1").is_none());
    }

    #[test]
    fn test_iframe_request_round_trip_ping() {
        let rt = IframeRuntime::with_light_theme();
        rt.register("p1", "about:blank");
        let resp = rt.request("p1", "ping", None).unwrap();
        assert!(resp.is_success());
        assert_eq!(resp.result.unwrap(), serde_json::json!("pong"));
    }

    #[test]
    fn test_iframe_request_echo() {
        let rt = IframeRuntime::with_light_theme();
        rt.register("p1", "about:blank");
        let resp = rt
            .request("p1", "echo", Some(serde_json::json!({"hello": "world"})))
            .unwrap();
        assert_eq!(resp.result.unwrap()["hello"], serde_json::json!("world"));
    }

    #[test]
    fn test_iframe_request_get_theme_passthrough() {
        let rt = IframeRuntime::with_light_theme();
        rt.register("p1", "about:blank");
        let resp = rt.request("p1", "get_theme", None).unwrap();
        let tokens = resp.result.unwrap();
        assert_eq!(tokens["--bg"], serde_json::json!("#ffffff"));
        assert_eq!(tokens["--accent"], serde_json::json!("#0969da"));
    }

    #[test]
    fn test_iframe_request_unknown_method_error() {
        let rt = IframeRuntime::with_light_theme();
        rt.register("p1", "about:blank");
        let resp = rt.request("p1", "does_not_exist", None).unwrap();
        assert!(!resp.is_success());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[test]
    fn test_iframe_theme_update_propagation() {
        let rt = IframeRuntime::with_light_theme();
        rt.register("p1", "about:blank");
        assert_eq!(rt.frame("p1").unwrap().theme.name, "light");

        rt.update_theme(CssTheme::dark());
        assert_eq!(rt.theme().name, "dark");
        // 已注册框架的主题应被同步更新
        assert_eq!(rt.frame("p1").unwrap().theme.name, "dark");
        // CSS 变量声明应反映暗色
        let css = rt.css_variables("p1").unwrap();
        assert!(css.contains("#0d1117"));
    }

    #[test]
    fn test_iframe_invoke_returns_result() {
        let rt = IframeRuntime::with_light_theme();
        rt.register("p1", "about:blank");
        let result = rt.invoke("p1", "ping", &serde_json::json!({})).unwrap();
        assert_eq!(result, serde_json::json!("pong"));

        let err = rt
            .invoke("p1", "missing", &serde_json::json!({}))
            .unwrap_err();
        assert!(matches!(err, crate::Error::JsonRpc(_)));
    }

    #[test]
    fn test_iframe_unregistered_errors() {
        let rt = IframeRuntime::with_light_theme();
        let err = rt
            .send(
                "ghost",
                JsonRpcRequest::new(JsonRpcId::Num(1), "ping", None),
            )
            .unwrap_err();
        assert!(matches!(err, crate::Error::NotFound(_)));
    }

    #[test]
    fn test_iframe_unregister() {
        let rt = IframeRuntime::with_light_theme();
        rt.register("p1", "about:blank");
        assert!(rt.is_registered("p1"));
        rt.unregister("p1");
        assert!(!rt.is_registered("p1"));
    }
}
