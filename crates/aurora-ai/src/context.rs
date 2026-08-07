//! 上下文持久化 (Context Persistence)
//!
//! 为 Agent 会话提供上下文存储与窗口压缩能力：
//! - **存储**：内存 mock 实现（`ContextStore`）+ SQLite DDL 常量，
//!   便于将来切换到真实 SQLite 后端。
//! - **会话恢复**：通过 `session_id` 检索历史上下文，支持断点续聊。
//! - **窗口压缩**：滑动窗口摘要 mock，将旧消息折叠为单条 summary，
//!   控制上下文长度。
//!
//! # 关键类型
//! - [`SessionId`]：会话 ID 新类型。
//! - [`AgentContext`] / [`ContextWindow`]：上下文与窗口结构。
//! - [`ContextStore`]：内存存储（含 DDL）。
//! - [`CompressedContext`]：压缩结果。

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

// ============================================================================
// 会话 ID 与上下文类型
// ============================================================================

/// 会话 ID 新类型。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for SessionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for SessionId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// 上下文消息条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMessage {
    pub role: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl ContextMessage {
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new("user", content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new("assistant", content)
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::new("system", content)
    }
}

/// 上下文窗口：限制保留的最大消息数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindow {
    /// 保留最近 N 条消息
    pub max_messages: usize,
    /// 当前消息列表（按时间顺序）
    pub messages: Vec<ContextMessage>,
    /// 已压缩的旧消息摘要（如有）
    pub summary: Option<String>,
}

impl ContextWindow {
    pub fn new(max_messages: usize) -> Self {
        Self {
            max_messages,
            messages: Vec::new(),
            summary: None,
        }
    }

    /// 当前消息数。
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// 追加一条消息（不自动压缩）。
    pub fn push(&mut self, msg: ContextMessage) {
        self.messages.push(msg);
    }

    /// 是否达到窗口上限。
    pub fn is_full(&self) -> bool {
        self.messages.len() >= self.max_messages
    }
}

impl Default for ContextWindow {
    fn default() -> Self {
        Self::new(20)
    }
}

/// Agent 上下文：会话级别的状态容器。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContext {
    pub session_id: SessionId,
    pub window: ContextWindow,
    pub tool_results: Vec<serde_json::Value>,
    pub user_preferences: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl AgentContext {
    pub fn new(session_id: SessionId) -> Self {
        let now = chrono::Utc::now();
        Self {
            session_id,
            window: ContextWindow::default(),
            tool_results: Vec::new(),
            user_preferences: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_window(mut self, window: ContextWindow) -> Self {
        self.window = window;
        self
    }

    pub fn with_preferences(mut self, prefs: serde_json::Value) -> Self {
        self.user_preferences = prefs;
        self
    }

    /// 添加一条消息并刷新 updated_at。
    pub fn add_message(&mut self, msg: ContextMessage) {
        self.window.push(msg);
        self.updated_at = chrono::Utc::now();
    }

    /// 记录一个工具结果。
    pub fn add_tool_result(&mut self, result: serde_json::Value) {
        self.tool_results.push(result);
        self.updated_at = chrono::Utc::now();
    }

    /// 消息数。
    pub fn message_count(&self) -> usize {
        self.window.len()
    }
}

// ============================================================================
// 压缩结果
// ============================================================================

/// 窗口压缩结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedContext {
    pub session_id: SessionId,
    /// 压缩前消息数
    pub original_count: usize,
    /// 压缩后保留的消息数
    pub retained_count: usize,
    /// 折叠的旧消息数
    pub compressed_count: usize,
    /// 生成摘要
    pub summary: String,
}

impl CompressedContext {
    pub fn new(
        session_id: SessionId,
        original_count: usize,
        retained_count: usize,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            session_id,
            original_count,
            retained_count,
            compressed_count: original_count - retained_count,
            summary: summary.into(),
        }
    }

    /// 压缩率（0.0 ~ 1.0）。
    pub fn ratio(&self) -> f64 {
        if self.original_count == 0 {
            return 0.0;
        }
        self.compressed_count as f64 / self.original_count as f64
    }
}

// ============================================================================
// 上下文存储
// ============================================================================

/// SQLite DDL：上下文表结构定义（用于将来切换到 rusqlite 后端）。
pub const CONTEXT_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS agent_contexts (
    session_id    TEXT PRIMARY KEY,
    window        TEXT NOT NULL,         -- JSON: ContextWindow
    tool_results  TEXT NOT NULL,         -- JSON: Vec<Value>
    preferences   TEXT NOT NULL,         -- JSON: Value
    created_at    TEXT NOT NULL,         -- ISO8601
    updated_at    TEXT NOT NULL          -- ISO8601
);

CREATE TABLE IF NOT EXISTS context_messages (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id    TEXT NOT NULL,
    role          TEXT NOT NULL,
    content       TEXT NOT NULL,
    timestamp     TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES agent_contexts(session_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_messages_session ON context_messages(session_id);
"#;

/// 上下文存储：内存 mock 实现，API 与 SQLite 后端保持一致。
pub struct ContextStore {
    contexts: Arc<RwLock<HashMap<SessionId, AgentContext>>>,
    /// 是否启用自动压缩
    auto_compress: bool,
    /// 压缩历史
    compressions: Arc<RwLock<Vec<CompressedContext>>>,
}

impl Default for ContextStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextStore {
    pub fn new() -> Self {
        Self {
            contexts: Arc::new(RwLock::new(HashMap::new())),
            auto_compress: false,
            compressions: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 启用自动压缩：每次保存时若窗口已满则触发压缩。
    pub fn with_auto_compress(mut self) -> Self {
        self.auto_compress = true;
        self
    }

    /// 已存储的会话数。
    pub fn len(&self) -> usize {
        self.contexts.read().len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.contexts.read().is_empty()
    }

    /// 创建或覆盖一个上下文。
    pub fn save(&self, ctx: AgentContext) -> Result<(), crate::Error> {
        debug!("context store: save session {}", ctx.session_id);
        let mut ctx = ctx;
        if self.auto_compress && ctx.window.is_full() {
            // compress_window 内部会记录到 compression_history
            self.compress_window(&mut ctx)?;
        }
        ctx.updated_at = chrono::Utc::now();
        self.contexts.write().insert(ctx.session_id.clone(), ctx);
        Ok(())
    }

    /// 读取上下文（会话恢复）。
    pub fn load(&self, session_id: &SessionId) -> Result<AgentContext, crate::Error> {
        self.contexts
            .read()
            .get(session_id)
            .cloned()
            .ok_or_else(|| crate::Error::NotFound(format!("session not found: {}", session_id)))
    }

    /// 删除上下文。
    pub fn delete(&self, session_id: &SessionId) -> Result<bool, crate::Error> {
        Ok(self.contexts.write().remove(session_id).is_some())
    }

    /// 列出全部会话 ID。
    pub fn list_sessions(&self) -> Vec<SessionId> {
        self.contexts.read().keys().cloned().collect()
    }

    /// 恢复会话：若存在则返回，否则创建新会话。
    pub fn resume(&self, session_id: &SessionId) -> Result<AgentContext, crate::Error> {
        if let Some(ctx) = self.contexts.read().get(session_id).cloned() {
            info!("context store: resume existing session {}", session_id);
            return Ok(ctx);
        }
        info!("context store: create new session {}", session_id);
        let ctx = AgentContext::new(session_id.clone());
        Ok(ctx)
    }

    /// 追加消息到指定会话。
    pub fn append_message(
        &self,
        session_id: &SessionId,
        msg: ContextMessage,
    ) -> Result<(), crate::Error> {
        let mut contexts = self.contexts.write();
        let ctx = contexts
            .get_mut(session_id)
            .ok_or_else(|| crate::Error::NotFound(format!("session not found: {}", session_id)))?;
        ctx.add_message(msg);
        Ok(())
    }

    /// 对一个上下文执行窗口压缩（滑动窗口 + 摘要 mock）。
    ///
    /// 保留最近 `max_messages` 条消息，将更早的消息折叠为一条 system 摘要。
    /// 每次调用都会写入 `compression_history`。
    pub fn compress_window(
        &self,
        ctx: &mut AgentContext,
    ) -> Result<CompressedContext, crate::Error> {
        let window = &mut ctx.window;
        let original = window.messages.len();
        let keep = window.max_messages;
        if original <= keep {
            let compressed =
                CompressedContext::new(ctx.session_id.clone(), original, original, String::new());
            self.compressions.write().push(compressed.clone());
            return Ok(compressed);
        }
        let compress_count = original - keep;
        let old_msgs: Vec<&ContextMessage> = window.messages.iter().take(compress_count).collect();
        let summary = summarize_messages(&old_msgs);
        // 截断保留尾部
        window.messages.drain(0..compress_count);
        // 在窗口头部插入 summary system 消息
        let mut summary_msg = ContextMessage::system(format!("[summary] {}", summary));
        summary_msg.timestamp = chrono::Utc::now();
        window.messages.insert(0, summary_msg);
        window.summary = Some(summary.clone());
        info!(
            "context store: compressed session {} ({} original -> {} retained + 1 summary)",
            ctx.session_id, original, keep
        );
        // retained_count 表示保留的原始消息数（不含新生成的 summary），
        // 因此 compressed_count = original - retained 即被折叠的消息数。
        let compressed = CompressedContext::new(ctx.session_id.clone(), original, keep, summary);
        self.compressions.write().push(compressed.clone());
        Ok(compressed)
    }

    /// 压缩历史记录。
    pub fn compression_history(&self) -> Vec<CompressedContext> {
        self.compressions.read().clone()
    }
}

/// 简易摘要 mock：拼接前若干条消息的内容片段。
fn summarize_messages(msgs: &[&ContextMessage]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (i, m) in msgs.iter().enumerate() {
        let snippet: String = m.content.chars().take(40).collect();
        parts.push(format!("[{}] {}: {}", i, m.role, snippet));
    }
    format!(
        "Summary of {} earlier messages: {}",
        msgs.len(),
        parts.join("; ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_session() -> SessionId {
        SessionId::new("test-session-1")
    }

    #[test]
    fn test_session_id_new_and_generate() {
        let id = SessionId::new("abc");
        assert_eq!(id.as_str(), "abc");
        assert_eq!(id.to_string(), "abc");

        let gen = SessionId::generate();
        assert!(!gen.as_str().is_empty());
        assert_ne!(gen.as_str(), "abc");
    }

    #[test]
    fn test_session_id_from_conversions() {
        let id1: SessionId = "hello".into();
        assert_eq!(id1.as_str(), "hello");
        let id2: SessionId = String::from("world").into();
        assert_eq!(id2.as_str(), "world");
    }

    #[test]
    fn test_session_id_eq_hash() {
        let a = SessionId::new("x");
        let b = SessionId::new("x");
        let c = SessionId::new("y");
        assert_eq!(a, b);
        assert_ne!(a, c);
        let mut set = std::collections::HashSet::new();
        set.insert(a.clone());
        assert!(set.contains(&b));
    }

    #[test]
    fn test_context_message_helpers() {
        let u = ContextMessage::user("hi");
        assert_eq!(u.role, "user");
        let a = ContextMessage::assistant("hello");
        assert_eq!(a.role, "assistant");
        let s = ContextMessage::system("rules");
        assert_eq!(s.role, "system");
    }

    #[test]
    fn test_context_window_default() {
        let w = ContextWindow::default();
        assert_eq!(w.max_messages, 20);
        assert!(w.is_empty());
        assert!(!w.is_full());
    }

    #[test]
    fn test_context_window_push_and_full() {
        let mut w = ContextWindow::new(2);
        w.push(ContextMessage::user("a"));
        assert!(!w.is_full());
        w.push(ContextMessage::user("b"));
        assert!(w.is_full());
        assert_eq!(w.len(), 2);
    }

    #[test]
    fn test_agent_context_new() {
        let ctx = AgentContext::new(make_session());
        assert_eq!(ctx.message_count(), 0);
        assert!(ctx.tool_results.is_empty());
        assert_eq!(ctx.created_at, ctx.updated_at);
    }

    #[test]
    fn test_agent_context_add_message_updates_timestamp() {
        let mut ctx = AgentContext::new(make_session());
        let original = ctx.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(5));
        ctx.add_message(ContextMessage::user("hi"));
        assert!(ctx.updated_at > original);
        assert_eq!(ctx.message_count(), 1);
    }

    #[test]
    fn test_agent_context_add_tool_result() {
        let mut ctx = AgentContext::new(make_session());
        ctx.add_tool_result(serde_json::json!({"ok": true}));
        assert_eq!(ctx.tool_results.len(), 1);
    }

    #[test]
    fn test_agent_context_with_window_and_prefs() {
        let ctx = AgentContext::new(make_session())
            .with_window(ContextWindow::new(10))
            .with_preferences(serde_json::json!({"lang": "zh"}));
        assert_eq!(ctx.window.max_messages, 10);
        assert_eq!(ctx.user_preferences["lang"], "zh");
    }

    #[test]
    fn test_context_store_save_and_load() {
        let store = ContextStore::new();
        let mut ctx = AgentContext::new(make_session());
        ctx.add_message(ContextMessage::user("hello"));
        store.save(ctx.clone()).unwrap();

        let loaded = store.load(&make_session()).unwrap();
        assert_eq!(loaded.message_count(), 1);
        assert_eq!(loaded.window.messages[0].content, "hello");
    }

    #[test]
    fn test_context_store_load_missing() {
        let store = ContextStore::new();
        let err = store.load(&SessionId::new("ghost")).unwrap_err();
        assert!(matches!(err, crate::Error::NotFound(_)));
    }

    #[test]
    fn test_context_store_delete() {
        let store = ContextStore::new();
        store.save(AgentContext::new(make_session())).unwrap();
        assert_eq!(store.len(), 1);
        let removed = store.delete(&make_session()).unwrap();
        assert!(removed);
        assert_eq!(store.len(), 0);
        // 再次删除返回 false
        let again = store.delete(&make_session()).unwrap();
        assert!(!again);
    }

    #[test]
    fn test_context_store_list_sessions() {
        let store = ContextStore::new();
        store.save(AgentContext::new(SessionId::new("a"))).unwrap();
        store.save(AgentContext::new(SessionId::new("b"))).unwrap();
        let sessions = store.list_sessions();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_context_store_resume_existing() {
        let store = ContextStore::new();
        let mut ctx = AgentContext::new(make_session());
        ctx.add_message(ContextMessage::user("hi"));
        store.save(ctx).unwrap();

        let resumed = store.resume(&make_session()).unwrap();
        assert_eq!(resumed.message_count(), 1);
    }

    #[test]
    fn test_context_store_resume_creates_new() {
        let store = ContextStore::new();
        let resumed = store.resume(&SessionId::new("fresh")).unwrap();
        assert_eq!(resumed.message_count(), 0);
        // resume 不自动保存
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn test_context_store_append_message() {
        let store = ContextStore::new();
        store.save(AgentContext::new(make_session())).unwrap();
        store
            .append_message(&make_session(), ContextMessage::user("appended"))
            .unwrap();
        let loaded = store.load(&make_session()).unwrap();
        assert_eq!(loaded.message_count(), 1);
        assert_eq!(loaded.window.messages[0].content, "appended");
    }

    #[test]
    fn test_context_store_append_message_missing_session() {
        let store = ContextStore::new();
        let err = store
            .append_message(&SessionId::new("ghost"), ContextMessage::user("x"))
            .unwrap_err();
        assert!(matches!(err, crate::Error::NotFound(_)));
    }

    #[test]
    fn test_context_store_compress_window_no_op_when_under_limit() {
        let store = ContextStore::new();
        let mut ctx = AgentContext::new(make_session()).with_window(ContextWindow::new(5));
        ctx.add_message(ContextMessage::user("a"));
        let compressed = store.compress_window(&mut ctx).unwrap();
        assert_eq!(compressed.original_count, 1);
        assert_eq!(compressed.retained_count, 1);
        assert_eq!(compressed.compressed_count, 0);
        // no-op 压缩也会记录到历史
        assert_eq!(store.compression_history().len(), 1);
    }

    #[test]
    fn test_context_store_compress_window_folds_old_messages() {
        let store = ContextStore::new();
        let mut ctx = AgentContext::new(make_session()).with_window(ContextWindow::new(2));
        // 写入 4 条消息（超出窗口 2）
        ctx.add_message(ContextMessage::user("msg-1"));
        ctx.add_message(ContextMessage::user("msg-2"));
        ctx.add_message(ContextMessage::assistant("msg-3"));
        ctx.add_message(ContextMessage::assistant("msg-4"));
        assert_eq!(ctx.message_count(), 4);

        let compressed = store.compress_window(&mut ctx).unwrap();
        // 原始 4 条，保留 2 条原始消息（不含 summary）
        assert_eq!(compressed.original_count, 4);
        assert_eq!(compressed.retained_count, 2);
        assert_eq!(compressed.compressed_count, 2);
        // 摘要位于窗口头部
        assert_eq!(ctx.window.messages[0].role, "system");
        assert!(ctx.window.messages[0].content.contains("[summary]"));
        assert!(ctx.window.summary.is_some());
        // 保留消息为 msg-3, msg-4
        assert_eq!(ctx.window.messages[1].content, "msg-3");
        assert_eq!(ctx.window.messages[2].content, "msg-4");
    }

    #[test]
    fn test_context_store_auto_compress_on_save() {
        let store = ContextStore::new().with_auto_compress();
        let mut ctx = AgentContext::new(make_session()).with_window(ContextWindow::new(2));
        ctx.add_message(ContextMessage::user("a"));
        ctx.add_message(ContextMessage::user("b"));
        ctx.add_message(ContextMessage::user("c"));
        // 保存时应触发压缩
        store.save(ctx).unwrap();
        let history = store.compression_history();
        assert_eq!(history.len(), 1);
        let loaded = store.load(&make_session()).unwrap();
        // 窗口已压缩：summary + 保留消息
        assert!(loaded.window.summary.is_some());
    }

    #[test]
    fn test_compressed_context_ratio() {
        let c = CompressedContext::new(make_session(), 10, 4, "summary");
        assert_eq!(c.compressed_count, 6);
        let ratio = c.ratio();
        assert!((ratio - 0.6).abs() < 1e-6);
    }

    #[test]
    fn test_compressed_context_zero_original() {
        let c = CompressedContext::new(make_session(), 0, 0, "");
        assert_eq!(c.ratio(), 0.0);
    }

    #[test]
    fn test_context_ddl_contains_tables() {
        assert!(CONTEXT_DDL.contains("CREATE TABLE IF NOT EXISTS agent_contexts"));
        assert!(CONTEXT_DDL.contains("CREATE TABLE IF NOT EXISTS context_messages"));
        assert!(CONTEXT_DDL.contains("FOREIGN KEY"));
        assert!(CONTEXT_DDL.contains("idx_messages_session"));
    }

    #[test]
    fn test_summarize_messages_truncates_long_content() {
        let long = "x".repeat(100);
        let msgs = vec![
            ContextMessage::user(long.clone()),
            ContextMessage::assistant(long.clone()),
        ];
        let refs: Vec<&ContextMessage> = msgs.iter().collect();
        let summary = summarize_messages(&refs);
        assert!(summary.contains("Summary of 2"));
        // 每条消息摘要最多 40 字符
        assert!(summary.len() < long.len() * 2);
    }

    #[test]
    fn test_context_store_is_empty() {
        let store = ContextStore::new();
        assert!(store.is_empty());
        store.save(AgentContext::new(make_session())).unwrap();
        assert!(!store.is_empty());
    }

    #[test]
    fn test_context_store_compression_history_persists() {
        let store = ContextStore::new();
        let mut ctx = AgentContext::new(SessionId::new("s1")).with_window(ContextWindow::new(1));
        ctx.add_message(ContextMessage::user("a"));
        ctx.add_message(ContextMessage::user("b"));
        store.compress_window(&mut ctx).unwrap();

        let mut ctx2 = AgentContext::new(SessionId::new("s2")).with_window(ContextWindow::new(1));
        ctx2.add_message(ContextMessage::user("a"));
        ctx2.add_message(ContextMessage::user("b"));
        store.compress_window(&mut ctx2).unwrap();

        let history = store.compression_history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].session_id, SessionId::new("s1"));
        assert_eq!(history[1].session_id, SessionId::new("s2"));
    }
}
