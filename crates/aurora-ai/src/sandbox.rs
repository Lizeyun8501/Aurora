//! 安全沙箱 (Security Sandbox)
//!
//! 在 Agent 调用工具前进行权限校验与审计记录，支持只读模式与工具白名单。
//! 参考 `aurora_core::l2_engines::permission` 的 RBAC 思路，但聚焦于
//! Agent 工具调用粒度。
//!
//! # 校验规则
//! 1. **白名单**：若 `allowed_tools` 非空，仅允许其中列出的工具。
//! 2. **只读模式**：`read_only = true` 时，写操作类工具（按名称前缀判定）
//!    默认拒绝，除非显式列入 `allowed_tools`。
//! 3. **运行时上限**：`max_runtime_secs` 记录为审计元数据（mock 实现
//!    不主动中断长任务，由调用方自行处理超时）。
//!
//! # 审计
//! 所有 `check_tool` / `audit_invoke` / `audit_result` 调用都会写入
//! `AuditLog`，支持按动作、决策、工具名过滤回溯。

use std::collections::HashSet;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::registry::{ToolInvocation, ToolResult};

// ============================================================================
// 沙箱配置
// ============================================================================

/// 写操作类工具名称前缀（在只读模式下默认拒绝）。
pub const WRITE_TOOL_PREFIXES: &[&str] = &[
    "write_",
    "delete_",
    "create_",
    "update_",
    "exec_",
    "shell_",
    "rm_",
    "drop_",
    "insert_",
    "modify_",
];

/// 判定一个工具名是否属于写操作（按前缀）。
pub fn is_write_tool(name: &str) -> bool {
    let lower = name.to_lowercase();
    WRITE_PREFIXES
        .iter()
        .any(|p| lower.starts_with(p))
}

// 为保持与 const 列表一致的命名
const WRITE_PREFIXES: &[&str] = WRITE_TOOL_PREFIXES;

/// 沙箱配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// 是否只读模式
    pub read_only: bool,
    /// 允许的工具白名单（为空表示不限制，但仍受 read_only 约束）
    pub allowed_tools: Vec<String>,
    /// 最大运行时长（秒）
    pub max_runtime_secs: u64,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            read_only: false,
            allowed_tools: Vec::new(),
            max_runtime_secs: 300,
        }
    }
}

impl SandboxConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    pub fn with_allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.allowed_tools = tools;
        self
    }

    pub fn with_max_runtime(mut self, secs: u64) -> Self {
        self.max_runtime_secs = secs;
        self
    }

    /// 是否显式允许某工具（白名单非空时使用）。
    pub fn explicitly_allows(&self, tool_name: &str) -> bool {
        self.allowed_tools
            .iter()
            .any(|t| t == tool_name)
    }

    /// 白名单是否为空（即不限制）。
    pub fn allows_all(&self) -> bool {
        self.allowed_tools.is_empty()
    }
}

// ============================================================================
// 审计条目与日志
// ============================================================================

/// 审计动作类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AuditAction {
    /// 权限检查
    Check,
    /// 调用发起
    Invoke,
    /// 调用结果记录
    Result,
}

impl AuditAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditAction::Check => "check",
            AuditAction::Invoke => "invoke",
            AuditAction::Result => "result",
        }
    }
}

/// 审计决策。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuditDecision {
    /// 允许
    Allow,
    /// 拒绝
    Deny,
    /// 成功
    Success,
    /// 失败
    Failure,
}

impl AuditDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditDecision::Allow => "allow",
            AuditDecision::Deny => "deny",
            AuditDecision::Success => "success",
            AuditDecision::Failure => "failure",
        }
    }

    pub fn is_positive(&self) -> bool {
        matches!(self, AuditDecision::Allow | AuditDecision::Success)
    }
}

/// 审计条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub action: AuditAction,
    pub decision: AuditDecision,
    pub tool_name: String,
    pub session_id: Option<String>,
    pub detail: String,
}

impl AuditEntry {
    pub fn new(
        action: AuditAction,
        decision: AuditDecision,
        tool_name: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: chrono::Utc::now(),
            action,
            decision,
            tool_name: tool_name.into(),
            session_id: None,
            detail: detail.into(),
        }
    }

    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }
}

/// 审计日志：累积全部审计条目。
pub struct AuditLog {
    entries: Arc<RwLock<Vec<AuditEntry>>>,
}

impl Default for AuditLog {
    fn default() -> Self {
        Self::new()
    }
}

impl AuditLog {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 追加一条审计记录。
    pub fn record(&self, entry: AuditEntry) {
        debug!(
            "audit: {} {} {} - {}",
            entry.action.as_str(),
            entry.decision.as_str(),
            entry.tool_name,
            entry.detail
        );
        self.entries.write().push(entry);
    }

    /// 全部条目快照。
    pub fn entries(&self) -> Vec<AuditEntry> {
        self.entries.read().clone()
    }

    /// 条目数。
    pub fn len(&self) -> usize {
        self.entries.read().len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.entries.read().is_empty()
    }

    /// 按动作过滤。
    pub fn filter_by_action(&self, action: AuditAction) -> Vec<AuditEntry> {
        self.entries
            .read()
            .iter()
            .filter(|e| e.action == action)
            .cloned()
            .collect()
    }

    /// 按决策过滤。
    pub fn filter_by_decision(&self, decision: AuditDecision) -> Vec<AuditEntry> {
        self.entries
            .read()
            .iter()
            .filter(|e| e.decision == decision)
            .cloned()
            .collect()
    }

    /// 按工具名过滤。
    pub fn filter_by_tool(&self, tool_name: &str) -> Vec<AuditEntry> {
        self.entries
            .read()
            .iter()
            .filter(|e| e.tool_name == tool_name)
            .cloned()
            .collect()
    }

    /// 统计拒绝次数。
    pub fn deny_count(&self) -> usize {
        self.filter_by_decision(AuditDecision::Deny).len()
    }

    /// 清空日志。
    pub fn clear(&self) {
        self.entries.write().clear();
    }
}

// ============================================================================
// 安全沙箱
// ============================================================================

/// 安全沙箱：组合配置与审计日志，对外提供工具调用前的校验接口。
pub struct SecuritySandbox {
    config: SandboxConfig,
    audit_log: AuditLog,
    /// 已命中只读拒绝的工具集合（用于统计）
    denied_tools: Arc<RwLock<HashSet<String>>>,
}

impl SecuritySandbox {
    pub fn new(config: SandboxConfig) -> Self {
        Self {
            config,
            audit_log: AuditLog::new(),
            denied_tools: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// 使用默认配置构造。
    pub fn default_config() -> Self {
        Self::new(SandboxConfig::default())
    }

    /// 配置引用。
    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    /// 审计日志引用。
    pub fn audit_log(&self) -> &AuditLog {
        &self.audit_log
    }

    /// 是否只读模式。
    pub fn is_read_only(&self) -> bool {
        self.config.read_only
    }

    /// 校验某工具是否可在当前沙箱下调用。
    ///
    /// 返回 `Ok(())` 表示允许，`Err` 表示拒绝并附原因。
    pub fn check_tool(&self, tool_name: &str) -> Result<(), crate::Error> {
        let decision = self.evaluate(tool_name);
        let entry = match &decision {
            Ok(()) => AuditEntry::new(
                AuditAction::Check,
                AuditDecision::Allow,
                tool_name,
                "tool allowed",
            ),
            Err(reason) => {
                self.denied_tools.write().insert(tool_name.to_string());
                AuditEntry::new(
                    AuditAction::Check,
                    AuditDecision::Deny,
                    tool_name,
                    reason.to_string(),
                )
            }
        };
        self.audit_log.record(entry);
        decision
    }

    /// 内部评估逻辑（不写审计）。
    fn evaluate(&self, tool_name: &str) -> Result<(), crate::Error> {
        // 1. 白名单检查
        if !self.config.allows_all() && !self.config.explicitly_allows(tool_name) {
            return Err(crate::Error::PermissionDenied(format!(
                "tool {} not in allowed_tools whitelist",
                tool_name
            )));
        }
        // 2. 只读模式检查
        if self.config.read_only && is_write_tool(tool_name) {
            // 即使是写工具，若显式列入白名单（即用户明确放行），仍然允许
            if !self.config.explicitly_allows(tool_name) {
                return Err(crate::Error::PermissionDenied(format!(
                    "tool {} is a write operation, denied in read-only mode",
                    tool_name
                )));
            }
        }
        Ok(())
    }

    /// 记录一次工具调用（在执行前调用）。
    pub fn audit_invoke(&self, invocation: &ToolInvocation) -> Result<(), crate::Error> {
        let entry = AuditEntry::new(
            AuditAction::Invoke,
            AuditDecision::Allow,
            invocation.tool_name.clone(),
            format!(
                "invoking tool with args: {}",
                invocation.arguments
            ),
        )
        .with_session(&invocation.session_id);
        self.audit_log.record(entry);
        Ok(())
    }

    /// 记录工具调用结果（在执行后调用）。
    pub fn audit_result(
        &self,
        tool_name: &str,
        result: &ToolResult,
    ) -> Result<(), crate::Error> {
        let decision = if result.is_ok() {
            AuditDecision::Success
        } else {
            AuditDecision::Failure
        };
        let detail = match &result.error {
            Some(e) => format!("tool failed: {} (latency={}ms)", e, result.latency_ms),
            None => format!("tool succeeded (latency={}ms)", result.latency_ms),
        };
        let entry = AuditEntry::new(AuditAction::Result, decision, tool_name, detail);
        self.audit_log.record(entry);
        Ok(())
    }

    /// 已被拒绝过的工具集合快照。
    pub fn denied_tools(&self) -> Vec<String> {
        self.denied_tools.read().iter().cloned().collect()
    }

    /// 当前审计条目数。
    pub fn audit_count(&self) -> usize {
        self.audit_log.len()
    }
}

// ============================================================================
// 默认只读工具集合辅助
// ============================================================================

/// 默认在只读模式下允许的「读」工具前缀。
pub const READ_TOOL_PREFIXES: &[&str] = &[
    "read_", "get_", "list_", "search_", "query_", "view_", "fetch_", "extract_", "summarize_",
];

/// 判定一个工具名是否属于「读」操作（按前缀）。
pub fn is_read_tool(name: &str) -> bool {
    let lower = name.to_lowercase();
    READ_TOOL_PREFIXES
        .iter()
        .any(|p| lower.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_write_tool_by_prefix() {
        assert!(is_write_tool("write_file"));
        assert!(is_write_tool("DELETE_ROW"));
        assert!(is_write_tool("create_user"));
        assert!(is_write_tool("exec_shell"));
        assert!(is_write_tool("rm_temp"));
        assert!(!is_write_tool("read_file"));
        assert!(!is_write_tool("search_web"));
    }

    #[test]
    fn test_is_read_tool_by_prefix() {
        assert!(is_read_tool("read_file"));
        assert!(is_read_tool("LIST_USERS"));
        assert!(is_read_tool("search_web"));
        assert!(!is_read_tool("write_file"));
        assert!(!is_read_tool("delete_row"));
    }

    #[test]
    fn test_sandbox_config_default() {
        let cfg = SandboxConfig::default();
        assert!(!cfg.read_only);
        assert!(cfg.allows_all());
        assert_eq!(cfg.max_runtime_secs, 300);
    }

    #[test]
    fn test_sandbox_config_builder() {
        let cfg = SandboxConfig::new()
            .read_only()
            .with_allowed_tools(vec!["read_a".into(), "read_b".into()])
            .with_max_runtime(60);
        assert!(cfg.read_only);
        assert!(!cfg.allows_all());
        assert!(cfg.explicitly_allows("read_a"));
        assert!(!cfg.explicitly_allows("read_c"));
        assert_eq!(cfg.max_runtime_secs, 60);
    }

    #[test]
    fn test_sandbox_check_tool_default_allows_all() {
        let s = SecuritySandbox::default_config();
        assert!(s.check_tool("any_tool").is_ok());
        assert!(s.check_tool("write_file").is_ok());
    }

    #[test]
    fn test_sandbox_whitelist_denies_unlisted() {
        let cfg = SandboxConfig::new()
            .with_allowed_tools(vec!["read_a".into(), "read_b".into()]);
        let s = SecuritySandbox::new(cfg);
        assert!(s.check_tool("read_a").is_ok());
        let err = s.check_tool("read_c").unwrap_err();
        assert!(matches!(err, crate::Error::PermissionDenied(_)));
    }

    #[test]
    fn test_sandbox_read_only_denies_write_tools() {
        let cfg = SandboxConfig::new().read_only();
        let s = SecuritySandbox::new(cfg);
        // 写工具默认拒绝
        let err = s.check_tool("write_file").unwrap_err();
        assert!(matches!(err, crate::Error::PermissionDenied(_)));
        // 读工具允许
        assert!(s.check_tool("read_file").is_ok());
        assert!(s.check_tool("search_web").is_ok());
    }

    #[test]
    fn test_sandbox_read_only_allows_explicit_write_in_whitelist() {
        // 显式列入白名单的写工具即使在只读模式下也允许
        let cfg = SandboxConfig::new()
            .read_only()
            .with_allowed_tools(vec!["write_special".into()]);
        let s = SecuritySandbox::new(cfg);
        assert!(s.check_tool("write_special").is_ok());
    }

    #[test]
    fn test_sandbox_records_check_audit_entries() {
        let cfg = SandboxConfig::new().read_only();
        let s = SecuritySandbox::new(cfg);
        s.check_tool("read_ok").unwrap();
        let _ = s.check_tool("write_denied");
        let checks = s.audit_log().filter_by_action(AuditAction::Check);
        assert_eq!(checks.len(), 2);
        let denies = s.audit_log().filter_by_decision(AuditDecision::Deny);
        assert_eq!(denies.len(), 1);
        assert_eq!(denies[0].tool_name, "write_denied");
    }

    #[test]
    fn test_sandbox_audit_invoke_records() {
        let s = SecuritySandbox::default_config();
        let inv = ToolInvocation::new("echo", serde_json::json!({"x": 1}), "s1");
        s.audit_invoke(&inv).unwrap();
        let invokes = s.audit_log().filter_by_action(AuditAction::Invoke);
        assert_eq!(invokes.len(), 1);
        assert_eq!(invokes[0].tool_name, "echo");
        assert_eq!(invokes[0].session_id.as_deref(), Some("s1"));
    }

    #[test]
    fn test_sandbox_audit_result_success_and_failure() {
        let s = SecuritySandbox::default_config();
        let ok = ToolResult::ok("echo", serde_json::json!("hi"), 5);
        let err = ToolResult::err("echo", "boom", 3);
        s.audit_result("echo", &ok).unwrap();
        s.audit_result("echo", &err).unwrap();
        let results = s.audit_log().filter_by_action(AuditAction::Result);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].decision, AuditDecision::Success);
        assert_eq!(results[1].decision, AuditDecision::Failure);
    }

    #[test]
    fn test_sandbox_denied_tools_set() {
        let cfg = SandboxConfig::new().read_only();
        let s = SecuritySandbox::new(cfg);
        let _ = s.check_tool("write_a");
        let _ = s.check_tool("write_b");
        let _ = s.check_tool("read_ok");
        let denied = s.denied_tools();
        assert_eq!(denied.len(), 2);
        assert!(denied.contains(&"write_a".to_string()));
        assert!(denied.contains(&"write_b".to_string()));
    }

    #[test]
    fn test_sandbox_is_read_only() {
        let s = SecuritySandbox::new(SandboxConfig::new().read_only());
        assert!(s.is_read_only());
        let s2 = SecuritySandbox::default_config();
        assert!(!s2.is_read_only());
    }

    #[test]
    fn test_audit_action_as_str() {
        assert_eq!(AuditAction::Check.as_str(), "check");
        assert_eq!(AuditAction::Invoke.as_str(), "invoke");
        assert_eq!(AuditAction::Result.as_str(), "result");
    }

    #[test]
    fn test_audit_decision_is_positive() {
        assert!(AuditDecision::Allow.is_positive());
        assert!(AuditDecision::Success.is_positive());
        assert!(!AuditDecision::Deny.is_positive());
        assert!(!AuditDecision::Failure.is_positive());
    }

    #[test]
    fn test_audit_entry_with_session() {
        let e = AuditEntry::new(
            AuditAction::Invoke,
            AuditDecision::Allow,
            "t",
            "d",
        )
        .with_session("s1");
        assert_eq!(e.session_id.as_deref(), Some("s1"));
        assert_eq!(e.tool_name, "t");
    }

    #[test]
    fn test_audit_log_filter_by_tool() {
        let log = AuditLog::new();
        log.record(AuditEntry::new(
            AuditAction::Check,
            AuditDecision::Allow,
            "t1",
            "d",
        ));
        log.record(AuditEntry::new(
            AuditAction::Check,
            AuditDecision::Deny,
            "t2",
            "d",
        ));
        log.record(AuditEntry::new(
            AuditAction::Check,
            AuditDecision::Allow,
            "t1",
            "d2",
        ));
        let t1 = log.filter_by_tool("t1");
        assert_eq!(t1.len(), 2);
        let t2 = log.filter_by_tool("t2");
        assert_eq!(t2.len(), 1);
    }

    #[test]
    fn test_audit_log_deny_count() {
        let log = AuditLog::new();
        log.record(AuditEntry::new(
            AuditAction::Check,
            AuditDecision::Deny,
            "t1",
            "d",
        ));
        log.record(AuditEntry::new(
            AuditAction::Check,
            AuditDecision::Allow,
            "t2",
            "d",
        ));
        log.record(AuditEntry::new(
            AuditAction::Check,
            AuditDecision::Deny,
            "t3",
            "d",
        ));
        assert_eq!(log.deny_count(), 2);
    }

    #[test]
    fn test_audit_log_clear_and_is_empty() {
        let log = AuditLog::new();
        assert!(log.is_empty());
        log.record(AuditEntry::new(
            AuditAction::Check,
            AuditDecision::Allow,
            "t1",
            "d",
        ));
        assert!(!log.is_empty());
        log.clear();
        assert!(log.is_empty());
    }

    #[test]
    fn test_sandbox_full_invoke_flow() {
        // 模拟 invoke_sandboxed 的完整审计流程
        let cfg = SandboxConfig::new().read_only();
        let s = SecuritySandbox::new(cfg);

        // 读工具通过
        s.check_tool("read_file").unwrap();
        let inv = ToolInvocation::new("read_file", serde_json::json!({"path": "a"}), "s1");
        s.audit_invoke(&inv).unwrap();
        let result = ToolResult::ok("read_file", serde_json::json!("content"), 2);
        s.audit_result("read_file", &result).unwrap();
        assert_eq!(s.audit_count(), 3);

        // 写工具被拒
        let err = s.check_tool("write_file").unwrap_err();
        assert!(matches!(err, crate::Error::PermissionDenied(_)));
        assert_eq!(s.audit_count(), 4);
    }

    #[test]
    fn test_sandbox_config_explicitly_allows_case_sensitive() {
        let cfg = SandboxConfig::new().with_allowed_tools(vec!["ReadFile".into()]);
        let s = SecuritySandbox::new(cfg);
        assert!(s.check_tool("ReadFile").is_ok());
        // 大小写敏感：readfile 不在白名单
        assert!(s.check_tool("readfile").is_err());
    }

    #[test]
    fn test_sandbox_max_runtime_secs_recorded_in_config() {
        let cfg = SandboxConfig::new().with_max_runtime(120);
        let s = SecuritySandbox::new(cfg);
        assert_eq!(s.config().max_runtime_secs, 120);
    }
}
