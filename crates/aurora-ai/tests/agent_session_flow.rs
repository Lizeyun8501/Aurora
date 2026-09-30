//! DK-10 实战片：Agent 工具流 + 完整时间轴
//! 生命周期：创建→状态→授权→执行→审计链→kill
//! 断言全部基于 agent_session.rs / sandbox.rs 实证签名（09-30 终核）

use aurora_ai::agent_session::{AgentSession, AGENT_DEFAULT_DEADLINE_SECS};
use aurora_ai::sandbox::{AuditAction, AuditDecision, AuditEntry, AuditLog};
use std::sync::Arc;

/// 完整时间轴：创建→状态→授权→执行→审计链→kill
#[test]
fn full_lifecycle_tool_flow() {
    let audit = Arc::new(AuditLog::new());
    let s = AgentSession::new("agent-flow-1", Arc::clone(&audit))
        .with_deadline_secs(AGENT_DEFAULT_DEADLINE_SECS);

    assert!(!s.expired());
    assert!(s.remaining_secs() <= AGENT_DEFAULT_DEADLINE_SECS);
    assert!(!s.is_killed());
    assert!(s.kill_reason().is_none());

    // 授权流（不预设 allow/deny 语义——只验证调用面可驱动）
    let _ = s.authorize_tool("read_file");

    // 执行记录（成功 + 失败两路）
    s.record_result("read_file", true, "ok-42");
    s.record_result("read_file", false, "io-miss");

    // 审计链：有落账 + 哈希链完整
    let entries = audit.entries();
    assert!(!entries.is_empty(), "record_result must land audit entries");
    assert!(
        audit.verify_chain(),
        "audit chain must verify after tool flow"
    );

    // kill 语义（确定性断言）
    s.kill("operator-halt");
    assert!(s.is_killed());
    assert_eq!(s.kill_reason().as_deref(), Some("operator-halt"));
}

/// 审计直录：AuditEntry::new → AuditLog::record → 链验证
#[test]
fn audit_entry_record_and_chain() {
    let audit = AuditLog::new();
    let e = AuditEntry::new(
        AuditAction::Check,
        AuditDecision::Allow,
        "operator",
        "manual-note",
    );
    audit.record(e);
    let all = audit.entries();
    assert_eq!(all.len(), 1);
    assert!(audit.verify_chain());
}

/// 默认死线常量对齐（CI 桌面命令层引用同一常量）
#[test]
fn default_deadline_is_900s() {
    assert_eq!(AGENT_DEFAULT_DEADLINE_SECS, 900);
}
