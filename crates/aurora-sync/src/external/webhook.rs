//! Webhook 接收 (Webhook Receiver)
//!
//! 本地 HTTP 服务 (mock) 接收 GitHub / Jira / Slack 等 webhook，
//! 使用 HMAC-SHA256 验证请求签名 ([`HmacVerifier`]，基于 `ring::hmac`)。
//!
//! # 签名格式
//! - 通用：原始 32 字节 HMAC-SHA256 摘要，或 hex 编码。
//! - GitHub：`X-Hub-Signature-256: sha256=<hex>`。
//! - Slack：`X-Slack-Signature: v0=<hex>`，签名基为 `v0:{timestamp}:{body}`。
//!
//! # 实现说明
//! [`WebhookReceiver`] 以内存列表模拟接收队列，不绑定真实 TCP 端口。
//! 真实实现替换 `receive` 内部为 `axum` / `hyper` handler 即可，公开 API 不变。

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Webhook 来源。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum WebhookSource {
    GitHub,
    Jira,
    Slack,
    /// 自定义来源 (如企业内部服务名)。
    Custom(String),
}

impl WebhookSource {
    pub fn as_str(&self) -> &str {
        match self {
            WebhookSource::GitHub => "github",
            WebhookSource::Jira => "jira",
            WebhookSource::Slack => "slack",
            WebhookSource::Custom(name) => name.as_str(),
        }
    }
}

impl std::fmt::Display for WebhookSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Webhook 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// 监听端口 (mock，不实际绑定)。
    pub port: u16,
    /// 接收路径 (如 `/webhook`)。
    pub path: String,
    /// HMAC 共享密钥。
    pub secret: Vec<u8>,
    /// 允许的来源 (为空表示全部允许)。
    pub allowed_sources: Vec<WebhookSource>,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            port: 8080,
            path: "/webhook".to_string(),
            secret: b"aurora-webhook-secret".to_vec(),
            allowed_sources: vec![
                WebhookSource::GitHub,
                WebhookSource::Jira,
                WebhookSource::Slack,
            ],
        }
    }
}

/// 接收到的 webhook 事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    /// 事件唯一 ID。
    pub id: String,
    /// 来源。
    pub source: WebhookSource,
    /// 事件类型 (如 `push` / `issue.created` / `message.channels`)。
    pub event_type: String,
    /// 原始 payload。
    pub payload: Vec<u8>,
    /// 接收时间。
    pub received_at: chrono::DateTime<chrono::Utc>,
    /// 签名是否验证通过。
    pub signature_valid: bool,
}

impl WebhookEvent {
    /// 解析 payload 为 JSON。
    pub fn payload_json(&self) -> crate::Result<serde_json::Value> {
        serde_json::from_slice(&self.payload).map_err(crate::Error::from)
    }
}

/// HMAC-SHA256 签名验证器 (基于 `ring::hmac`)。
pub struct HmacVerifier {
    key: ring::hmac::Key,
}

impl HmacVerifier {
    /// 用共享密钥构造验证器。
    pub fn new(secret: &[u8]) -> Self {
        Self {
            key: ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret),
        }
    }

    /// 对 payload 计算原始 HMAC-SHA256 签名 (32 字节)。
    pub fn sign(&self, payload: &[u8]) -> Vec<u8> {
        let mut ctx = ring::hmac::Context::with_key(&self.key);
        ctx.update(payload);
        ctx.sign().as_ref().to_vec()
    }

    /// 计算 hex 编码签名。
    pub fn sign_hex(&self, payload: &[u8]) -> String {
        hex_encode(&self.sign(payload))
    }

    /// 验证原始字节签名 (常数时间比较)。
    pub fn verify(&self, payload: &[u8], signature: &[u8]) -> bool {
        ring::hmac::verify(&self.key, payload, signature).is_ok()
    }

    /// 验证 hex 编码签名。
    pub fn verify_hex(&self, payload: &[u8], hex_sig: &str) -> bool {
        match hex_decode(hex_sig) {
            Some(bytes) => self.verify(payload, &bytes),
            None => false,
        }
    }

    /// 验证 GitHub 签名头 (`sha256=<hex>`)。
    pub fn verify_github(&self, payload: &[u8], header: &str) -> bool {
        let hex = header.strip_prefix("sha256=").unwrap_or(header);
        self.verify_hex(payload, hex)
    }

    /// 验证 Slack 签名头 (`v0=<hex>`)。
    ///
    /// Slack 的签名基为 `v0:{timestamp}:{body}`。
    pub fn verify_slack(&self, body: &[u8], timestamp: &str, header: &str) -> bool {
        let mut signing_base: Vec<u8> = Vec::new();
        signing_base.extend_from_slice(b"v0:");
        signing_base.extend_from_slice(timestamp.as_bytes());
        signing_base.push(b':');
        signing_base.extend_from_slice(body);
        let hex = header.strip_prefix("v0=").unwrap_or(header);
        self.verify_hex(&signing_base, hex)
    }
}

/// hex 编码字节数组。
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// hex 解码 (小写 / 大写均支持)。
fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    for chunk in bytes.chunks(2) {
        let hi = hex_val(chunk[0])?;
        let lo = hex_val(chunk[1])?;
        out.push((hi << 4) | lo);
    }
    Some(out)
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Webhook 接收器 (mock HTTP server)。
pub struct WebhookReceiver {
    config: WebhookConfig,
    verifier: HmacVerifier,
    events: Arc<RwLock<Vec<WebhookEvent>>>,
    handlers: Arc<RwLock<HashMap<WebhookSource, u32>>>,
    listening: Arc<RwLock<bool>>,
}

impl WebhookReceiver {
    pub fn new(config: WebhookConfig) -> Self {
        let verifier = HmacVerifier::new(&config.secret);
        Self {
            config,
            verifier,
            events: Arc::new(RwLock::new(Vec::new())),
            handlers: Arc::new(RwLock::new(HashMap::new())),
            listening: Arc::new(RwLock::new(false)),
        }
    }

    pub fn config(&self) -> &WebhookConfig {
        &self.config
    }

    /// 启动 mock HTTP server (标记为 listening)。
    pub fn start(&self) -> crate::Result<()> {
        *self.listening.write() = true;
        info!(
            "webhook receiver started on port {} {}",
            self.config.port, self.config.path
        );
        Ok(())
    }

    /// 停止 mock HTTP server。
    pub fn stop(&self) -> crate::Result<()> {
        *self.listening.write() = false;
        info!("webhook receiver stopped");
        Ok(())
    }

    /// 是否正在监听。
    pub fn is_listening(&self) -> bool {
        *self.listening.read()
    }

    /// 注册某来源的处理器 (mock：仅计数)。
    pub fn register_handler(&self, source: WebhookSource) {
        let mut handlers = self.handlers.write();
        *handlers.entry(source).or_insert(0) += 1;
    }

    /// 某来源已注册处理器数。
    pub fn handler_count(&self, source: &WebhookSource) -> u32 {
        self.handlers.read().get(source).copied().unwrap_or(0)
    }

    /// 检查来源是否被允许。
    fn is_allowed(&self, source: &WebhookSource) -> bool {
        if self.config.allowed_sources.is_empty() {
            return true;
        }
        self.config.allowed_sources.contains(source)
    }

    /// 通用接收：验证原始字节签名，校验通过则入队。
    ///
    /// `signature` 为原始 HMAC-SHA256 字节。验签失败返回
    /// [`crate::Error::HmacVerificationFailed`]。
    pub fn receive(
        &self,
        source: WebhookSource,
        event_type: impl Into<String>,
        payload: Vec<u8>,
        signature: &[u8],
    ) -> crate::Result<WebhookEvent> {
        if !self.is_listening() {
            return Err(crate::Error::ExternalSync(
                "webhook receiver not started".into(),
            ));
        }
        if !self.is_allowed(&source) {
            return Err(crate::Error::Unauthorized(format!(
                "webhook source not allowed: {}",
                source
            )));
        }
        let valid = self.verifier.verify(&payload, signature);
        if !valid {
            warn!("webhook hmac verification failed: source={}", source);
            return Err(crate::Error::HmacVerificationFailed);
        }
        let event = WebhookEvent {
            id: uuid::Uuid::new_v4().to_string(),
            source,
            event_type: event_type.into(),
            received_at: chrono::Utc::now(),
            signature_valid: true,
            payload,
        };
        debug!(
            "webhook received: source={} type={}",
            event.source, event.event_type
        );
        self.events.write().push(event.clone());
        Ok(event)
    }

    /// 接收 GitHub webhook (签名头 `sha256=<hex>`)。
    pub fn receive_github(
        &self,
        event_type: impl Into<String>,
        payload: Vec<u8>,
        signature_header: &str,
    ) -> crate::Result<WebhookEvent> {
        if !self.verifier.verify_github(&payload, signature_header) {
            return Err(crate::Error::HmacVerificationFailed);
        }
        // 验签通过后构造原始签名供 receive 复用逻辑 (这里直接入队)
        self.receive_signed(WebhookSource::GitHub, event_type, payload)
    }

    /// 接收 Slack webhook (签名头 `v0=<hex>`)。
    pub fn receive_slack(
        &self,
        event_type: impl Into<String>,
        body: Vec<u8>,
        timestamp: &str,
        signature_header: &str,
    ) -> crate::Result<WebhookEvent> {
        if !self
            .verifier
            .verify_slack(&body, timestamp, signature_header)
        {
            return Err(crate::Error::HmacVerificationFailed);
        }
        self.receive_signed(WebhookSource::Slack, event_type, body)
    }

    /// 接收 Jira webhook (hex 签名)。
    pub fn receive_jira(
        &self,
        event_type: impl Into<String>,
        payload: Vec<u8>,
        hex_signature: &str,
    ) -> crate::Result<WebhookEvent> {
        if !self.verifier.verify_hex(&payload, hex_signature) {
            return Err(crate::Error::HmacVerificationFailed);
        }
        self.receive_signed(WebhookSource::Jira, event_type, payload)
    }

    /// 已验签通过的事件入队 (内部复用)。
    fn receive_signed(
        &self,
        source: WebhookSource,
        event_type: impl Into<String>,
        payload: Vec<u8>,
    ) -> crate::Result<WebhookEvent> {
        if !self.is_listening() {
            return Err(crate::Error::ExternalSync(
                "webhook receiver not started".into(),
            ));
        }
        if !self.is_allowed(&source) {
            return Err(crate::Error::Unauthorized(format!(
                "webhook source not allowed: {}",
                source
            )));
        }
        let event = WebhookEvent {
            id: uuid::Uuid::new_v4().to_string(),
            source,
            event_type: event_type.into(),
            received_at: chrono::Utc::now(),
            signature_valid: true,
            payload,
        };
        self.events.write().push(event.clone());
        Ok(event)
    }

    /// 已接收事件总数。
    pub fn event_count(&self) -> usize {
        self.events.read().len()
    }

    /// 全部已接收事件。
    pub fn events(&self) -> Vec<WebhookEvent> {
        self.events.read().clone()
    }

    /// 按来源筛选已接收事件。
    pub fn events_for_source(&self, source: &WebhookSource) -> Vec<WebhookEvent> {
        self.events
            .read()
            .iter()
            .filter(|e| &e.source == source)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_receiver() -> WebhookReceiver {
        let r = WebhookReceiver::new(WebhookConfig::default());
        r.start().unwrap();
        r
    }

    #[test]
    fn test_webhook_source_as_str() {
        assert_eq!(WebhookSource::GitHub.as_str(), "github");
        assert_eq!(WebhookSource::Jira.as_str(), "jira");
        assert_eq!(WebhookSource::Slack.as_str(), "slack");
        assert_eq!(WebhookSource::Custom("gitlab".into()).as_str(), "gitlab");
        assert_eq!(WebhookSource::GitHub.to_string(), "github");
    }

    #[test]
    fn test_webhook_config_default() {
        let cfg = WebhookConfig::default();
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.path, "/webhook");
        assert!(!cfg.secret.is_empty());
        assert_eq!(cfg.allowed_sources.len(), 3);
    }

    #[test]
    fn test_hex_encode_decode_roundtrip() {
        let bytes = vec![0u8, 1, 2, 255, 128];
        let hex = hex_encode(&bytes);
        assert_eq!(hex.len(), 10);
        assert_eq!(hex_decode(&hex), Some(bytes));
    }

    #[test]
    fn test_hex_decode_invalid() {
        assert_eq!(hex_decode("abc"), None); // 奇数长度
        assert_eq!(hex_decode("xy"), None); // 非法字符
    }

    #[test]
    fn test_hmac_sign_verify_valid() {
        let v = HmacVerifier::new(b"secret");
        let payload = b"hello world";
        let sig = v.sign(payload);
        assert_eq!(sig.len(), 32); // SHA-256 = 32 字节
        assert!(v.verify(payload, &sig));
    }

    #[test]
    fn test_hmac_verify_invalid_signature() {
        let v = HmacVerifier::new(b"secret");
        let payload = b"hello world";
        let wrong_sig = vec![0u8; 32];
        assert!(!v.verify(payload, &wrong_sig));
    }

    #[test]
    fn test_hmac_verify_tampered_payload() {
        let v = HmacVerifier::new(b"secret");
        let payload = b"hello world";
        let sig = v.sign(payload);
        // 篡改 payload
        assert!(!v.verify(b"hello WORLD", &sig));
    }

    #[test]
    fn test_hmac_verify_hex() {
        let v = HmacVerifier::new(b"secret");
        let payload = b"hello";
        let hex = v.sign_hex(payload);
        assert!(v.verify_hex(payload, &hex));
        // 错误 hex
        assert!(!v.verify_hex(payload, "deadbeef"));
        // 非法 hex
        assert!(!v.verify_hex(payload, "nothex"));
    }

    #[test]
    fn test_hmac_verify_github_format() {
        let v = HmacVerifier::new(b"secret");
        let payload = br#"{"ref":"refs/heads/main"}"#;
        let hex = v.sign_hex(payload);
        let header = format!("sha256={}", hex);
        assert!(v.verify_github(payload, &header));
        // 错误前缀
        assert!(!v.verify_github(payload, "sha256=deadbeef"));
        // 篡改
        assert!(!v.verify_github(b"tampered", &header));
    }

    #[test]
    fn test_hmac_verify_slack_format() {
        let v = HmacVerifier::new(b"secret");
        let body = br#"{"type":"event_callback"}"#;
        let timestamp = "1531420618";
        // 构造 Slack 签名基
        let mut base: Vec<u8> = Vec::new();
        base.extend_from_slice(b"v0:");
        base.extend_from_slice(timestamp.as_bytes());
        base.push(b':');
        base.extend_from_slice(body);
        let hex = hex_encode(&v.sign(&base));
        let header = format!("v0={}", hex);
        assert!(v.verify_slack(body, timestamp, &header));
        // 错误时间戳
        assert!(!v.verify_slack(body, "9999", &header));
    }

    #[test]
    fn test_receiver_start_stop_listening() {
        let r = WebhookReceiver::new(WebhookConfig::default());
        assert!(!r.is_listening());
        r.start().unwrap();
        assert!(r.is_listening());
        r.stop().unwrap();
        assert!(!r.is_listening());
    }

    #[test]
    fn test_receiver_receive_valid_signature() {
        let r = make_receiver();
        let v = HmacVerifier::new(&r.config().secret);
        let payload = b"{\"event\":\"push\"}".to_vec();
        let sig = v.sign(&payload);
        let event = r
            .receive(WebhookSource::GitHub, "push", payload.clone(), &sig)
            .unwrap();
        assert!(event.signature_valid);
        assert_eq!(event.source, WebhookSource::GitHub);
        assert_eq!(event.event_type, "push");
        assert_eq!(r.event_count(), 1);
    }

    #[test]
    fn test_receiver_receive_invalid_signature_errors() {
        let r = make_receiver();
        let payload = b"{}".to_vec();
        let bad_sig = vec![0u8; 32];
        let result = r.receive(WebhookSource::GitHub, "push", payload, &bad_sig);
        assert!(matches!(result, Err(crate::Error::HmacVerificationFailed)));
        assert_eq!(r.event_count(), 0); // 未入队
    }

    #[test]
    fn test_receiver_receive_not_listening_errors() {
        let r = WebhookReceiver::new(WebhookConfig::default());
        // 未 start
        let payload = b"{}".to_vec();
        let v = HmacVerifier::new(&r.config().secret);
        let sig = v.sign(&payload);
        let result = r.receive(WebhookSource::GitHub, "push", payload, &sig);
        assert!(result.is_err());
    }

    #[test]
    fn test_receiver_disallowed_source_errors() {
        let mut cfg = WebhookConfig::default();
        cfg.allowed_sources = vec![WebhookSource::GitHub]; // 仅允许 GitHub
        let r = WebhookReceiver::new(cfg);
        r.start().unwrap();
        let v = HmacVerifier::new(&r.config().secret);
        let payload = b"{}".to_vec();
        let sig = v.sign(&payload);
        // Slack 不在允许列表
        let result = r.receive(WebhookSource::Slack, "msg", payload, &sig);
        assert!(result.is_err());
    }

    #[test]
    fn test_receiver_github_dispatch() {
        let r = make_receiver();
        let v = HmacVerifier::new(&r.config().secret);
        let payload = br#"{"action":"opened"}"#.to_vec();
        let header = format!("sha256={}", v.sign_hex(&payload));
        let event = r.receive_github("pull_request", payload, &header).unwrap();
        assert_eq!(event.source, WebhookSource::GitHub);
        assert_eq!(event.event_type, "pull_request");
        assert_eq!(r.events_for_source(&WebhookSource::GitHub).len(), 1);
    }

    #[test]
    fn test_receiver_slack_dispatch() {
        let r = make_receiver();
        let v = HmacVerifier::new(&r.config().secret);
        let body = br#"{"type":"event_callback"}"#.to_vec();
        let timestamp = "1531420618";
        let mut base: Vec<u8> = Vec::new();
        base.extend_from_slice(b"v0:");
        base.extend_from_slice(timestamp.as_bytes());
        base.push(b':');
        base.extend_from_slice(&body);
        let header = format!("v0={}", hex_encode(&v.sign(&base)));
        let event = r
            .receive_slack("message.channels", body, timestamp, &header)
            .unwrap();
        assert_eq!(event.source, WebhookSource::Slack);
        assert_eq!(r.events_for_source(&WebhookSource::Slack).len(), 1);
    }

    #[test]
    fn test_receiver_jira_dispatch() {
        let r = make_receiver();
        let v = HmacVerifier::new(&r.config().secret);
        let payload = br#"{"issue":{"key":"AUR-1"}}"#.to_vec();
        let hex = v.sign_hex(&payload);
        let event = r
            .receive_jira("issue.created", payload.clone(), &hex)
            .unwrap();
        assert_eq!(event.source, WebhookSource::Jira);
        assert_eq!(r.events_for_source(&WebhookSource::Jira).len(), 1);
        // payload_json 可解析
        let json = event.payload_json().unwrap();
        assert_eq!(json["issue"]["key"], "AUR-1");
    }

    #[test]
    fn test_receiver_github_tampered_rejected() {
        let r = make_receiver();
        let v = HmacVerifier::new(&r.config().secret);
        let payload = br#"{"action":"opened"}"#.to_vec();
        let header = format!("sha256={}", v.sign_hex(&payload));
        // 篡改 payload
        let tampered = br#"{"action":"closed"}"#.to_vec();
        let result = r.receive_github("pull_request", tampered, &header);
        assert!(matches!(result, Err(crate::Error::HmacVerificationFailed)));
        assert_eq!(r.event_count(), 0);
    }

    #[test]
    fn test_receiver_handler_registration() {
        let r = make_receiver();
        assert_eq!(r.handler_count(&WebhookSource::GitHub), 0);
        r.register_handler(WebhookSource::GitHub);
        r.register_handler(WebhookSource::GitHub);
        r.register_handler(WebhookSource::Slack);
        assert_eq!(r.handler_count(&WebhookSource::GitHub), 2);
        assert_eq!(r.handler_count(&WebhookSource::Slack), 1);
        assert_eq!(r.handler_count(&WebhookSource::Jira), 0);
    }

    #[test]
    fn test_receiver_multiple_sources_dispatch() {
        let r = make_receiver();
        let v = HmacVerifier::new(&r.config().secret);
        // GitHub
        let gh_payload = b"{}".to_vec();
        let gh_header = format!("sha256={}", v.sign_hex(&gh_payload));
        r.receive_github("push", gh_payload, &gh_header).unwrap();
        // Jira
        let jira_payload = b"{}".to_vec();
        let jira_hex = v.sign_hex(&jira_payload);
        r.receive_jira("issue.updated", jira_payload, &jira_hex)
            .unwrap();
        // Slack
        let body = b"{}".to_vec();
        let ts = "123";
        let mut base: Vec<u8> = Vec::new();
        base.extend_from_slice(b"v0:");
        base.extend_from_slice(ts.as_bytes());
        base.push(b':');
        base.extend_from_slice(&body);
        let slack_header = format!("v0={}", hex_encode(&v.sign(&base)));
        r.receive_slack("message", body, ts, &slack_header).unwrap();

        assert_eq!(r.event_count(), 3);
        assert_eq!(r.events_for_source(&WebhookSource::GitHub).len(), 1);
        assert_eq!(r.events_for_source(&WebhookSource::Jira).len(), 1);
        assert_eq!(r.events_for_source(&WebhookSource::Slack).len(), 1);
    }
}
