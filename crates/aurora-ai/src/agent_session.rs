//! DK-10 Agent 会话面（15 分钟限时 + 审计 + Kill-Switch）。
//!
//! 清单卡原文：**Agent 会话 15 分钟限时 + 审计 + Kill-Switch**。
//! - `AgentSession` 是 Agent 工具调用的**唯一编排入口**：每次工具调用先过
//!   `authorize_tool`（Kill-Switch/限时/沙箱三重门），判定落审计链（哈希链
//!   防篡改，复用 [`crate::sandbox::AuditLog`]）；
//! - 限时语义：**过期的会话不产生新的授权**（进行中的单次调用由调用方在
//!   `max_runtime_secs` 内自行约束——单调用超时属传输层职责）；
//! - Kill-Switch 语义：立即生效、不可撤销（`killed` 置位后 authorize 一律
//!   Deny），kill 理由落审计链；
//! - 本模块只做**授权判定**，不执行工具——执行方拿到 Allow 后自行调用工具，
//!   并以 `record_result` 回写结果审计（Result 条目）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::sandbox::{
    is_write_tool, AuditAction, AuditDecision, AuditEntry, AuditLog, SandboxConfig,
};

/// Agent 会话默认限时（清单卡口径：15 分钟）。
pub const AGENT_DEFAULT_DEADLINE_SECS: u64 = 900;

/// Agent 会话（工具调用授权 + 限时 + Kill-Switch + 审计）。
pub struct AgentSession {
    id: String,
    started_at: Instant,
    deadline: Duration,
    killed: Arc<AtomicBool>,
    kill_reason: Mutex<Option<String>>,
    sandbox: SandboxConfig,
    audit: Arc<AuditLog>,
}

impl AgentSession {
    /// 新会话（默认 15 分钟限时，只读沙箱——Agent 默认无写权，两段式提交
    /// 是唯一写入通道）。
    pub fn new(id: impl Into<String>, audit: Arc<AuditLog>) -> Self {
        Self {
            id: id.into(),
            started_at: Instant::now(),
            deadline: Duration::from_secs(AGENT_DEFAULT_DEADLINE_SECS),
            killed: Arc::new(AtomicBool::new(false)),
            kill_reason: Mutex::new(None),
            sandbox: SandboxConfig::default().read_only(),
            audit,
        }
    }

    /// 自定义限时（秒）。
    pub fn with_deadline_secs(mut self, secs: u64) -> Self {
        self.deadline = Duration::from_secs(secs);
        self
    }

    /// 自定义沙箱配置（工具白名单/只读开关——放宽需调用方显式负责）。
    pub fn with_sandbox(mut self, sandbox: SandboxConfig) -> Self {
        self.sandbox = sandbox;
        self
    }

    /// 是否已超时。
    pub fn expired(&self) -> bool {
        self.started_at.elapsed() >= self.deadline
    }

    /// 剩余秒数（归零下限）。
    pub fn remaining_secs(&self) -> u64 {
        self.deadline
            .checked_sub(self.started_at.elapsed())
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Kill-Switch：立即置位 + 理由落审计。不可撤销（重复 kill 无害）。
    pub fn kill(&self, reason: impl Into<String>) {
        let reason = reason.into();
        self.kill_reason.lock().unwrap().replace(reason.clone());
        self.killed.store(true, Ordering::SeqCst);
        self.audit.record(
            AuditEntry::new(
                AuditAction::Invoke,
                AuditDecision::Deny,
                "agent_session.kill_switch",
                format!("Kill-Switch 触发: {reason}"),
            )
            .with_session(self.id.clone()),
        );
    }

    /// 是否已被 Kill。
    pub fn is_killed(&self) -> bool {
        self.killed.load(Ordering::SeqCst)
    }

    /// Kill 理由（未被 Kill 返回 None）。
    pub fn kill_reason(&self) -> Option<String> {
        self.kill_reason.lock().unwrap().clone()
    }

    /// 工具调用授权判定（唯一入口）：Kill > 超时 > 只读写拦截 > 白名单。
    /// 每次判定落审计链（Allow/Deny 均记录——审计完整性优先于静默性能）。
    pub fn authorize_tool(&self, tool: &str) -> Result<(), String> {
        /// 审计+拒绝的统一路径。
        macro_rules! deny {
            ($reason:expr) => {{
                self.audit.record(
                    AuditEntry::new(AuditAction::Invoke, AuditDecision::Deny, tool, $reason)
                        .with_session(self.id.clone()),
                );
                return Err($reason);
            }};
        }
        // 1) Kill-Switch（立即生效，优先级最高）
        if self.is_killed() {
            let reason = self.kill_reason().unwrap_or_else(|| "unspecified".into());
            deny!(format!("会话已被 Kill-Switch 终止: {reason}"));
        }
        // 2) 限时
        if self.expired() {
            deny!("会话已超时（15 分钟限时）".to_string());
        }
        // 3) 只读沙箱：写前缀工具一律拒绝（AI 不得静默写入——走 aiLiquify 两段式）
        if self.sandbox.read_only && is_write_tool(tool) {
            deny!("只读沙箱拒绝写操作（写入走 aiLiquify 两段式提交）".to_string());
        }
        // 4) 白名单（非空时未显式允许即拒）
        if !self.sandbox.allows_all() && !self.sandbox.explicitly_allows(tool) {
            deny!("工具不在会话白名单".to_string());
        }
        // 5) 放行（审计 Allow）
        self.audit.record(
            AuditEntry::new(
                AuditAction::Invoke,
                AuditDecision::Allow,
                tool,
                "授权通过（限时/沙箱/白名单）",
            )
            .with_session(self.id.clone()),
        );
        Ok(())
    }

    /// 工具执行结果回写（调用方执行完工具后必须回写——Result 条目用
    /// Success/Failure 语义）。
    pub fn record_result(&self, tool: &str, ok: bool, detail: impl Into<String>) {
        self.audit.record(
            AuditEntry::new(
                AuditAction::Result,
                if ok {
                    AuditDecision::Success
                } else {
                    AuditDecision::Failure
                },
                tool,
                detail,
            )
            .with_session(self.id.clone()),
        );
    }

    /// 会话 ID。
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 审计链句柄（供会话结束后的完整性校验/导出）。
    pub fn audit(&self) -> &Arc<AuditLog> {
        &self.audit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> AgentSession {
        AgentSession::new("agent-test-1", Arc::new(AuditLog::new()))
    }

    /// 默认 15 分钟限时 + remaining 语义。
    #[test]
    fn dk10_agent_default_deadline() {
        let s = session();
        assert!(!s.expired());
        let r = s.remaining_secs();
        assert!((895..=900).contains(&r), "剩余约 900s，实际 {r}");
        let s2 = AgentSession::new("t2", Arc::new(AuditLog::new())).with_deadline_secs(1);
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert!(s2.expired());
        assert_eq!(s2.remaining_secs(), 0);
    }

    /// 超时 → 授权拒绝 + 审计 Deny 落链。
    #[test]
    fn dk10_agent_expired_denies() {
        let s = AgentSession::new("t3", Arc::new(AuditLog::new())).with_deadline_secs(1);
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let err = s.authorize_tool("read_note").unwrap_err();
        assert!(err.contains("超时"));
        assert!(s.audit().verify_chain(), "审计链完整");
    }

    /// Kill-Switch：立即生效 + 不可撤销 + 理由落审计。
    #[test]
    fn dk10_agent_kill_switch() {
        let s = session();
        assert!(s.authorize_tool("read_note").is_ok(), "未 Kill 前正常放行");
        s.kill("用户手动终止");
        assert!(s.is_killed());
        let err = s.authorize_tool("read_note").unwrap_err();
        assert!(err.contains("Kill-Switch"));
        assert_eq!(s.kill_reason().as_deref(), Some("用户手动终止"));
        s.kill("二次 kill 无害");
        assert!(s.authorize_tool("read_note").is_err());
        assert!(s.audit().verify_chain());
    }

    /// 只读沙箱拦写前缀 + 白名单语义 + 正常 Allow 路径 + 结果回写。
    #[test]
    fn dk10_agent_sandbox_gates() {
        let s = session();
        assert!(s.authorize_tool("create_note").is_err(), "只读拦写前缀");
        assert!(s.authorize_tool("write_file").is_err());
        assert!(s.authorize_tool("read_note").is_ok());
        s.record_result("read_note", true, "42 bytes");
        s.record_result("read_note", false, "miss");
        assert!(s.audit().verify_chain());

        let s2 = AgentSession::new("t5", Arc::new(AuditLog::new()))
            .with_sandbox(SandboxConfig::new().with_allowed_tools(vec!["read_note".into()]));
        assert!(s2.authorize_tool("read_note").is_ok());
        assert!(s2.authorize_tool("search_notes").is_err(), "白名单外拒绝");
        // 只读优先于白名单（双门串联）
        let s3 = AgentSession::new("t6", Arc::new(AuditLog::new())).with_sandbox(
            SandboxConfig::new()
                .read_only()
                .with_allowed_tools(vec!["create_note".into()]),
        );
        assert!(
            s3.authorize_tool("create_note").is_err(),
            "只读优先于白名单"
        );
        assert!(s.audit().verify_chain());
    }

    /// 审计链全程可验证（多次授权/拒绝/结果/kill 混合序列）。
    #[test]
    fn dk10_agent_audit_chain_integrity() {
        let s = session();
        let _ = s.authorize_tool("read_note");
        let _ = s.authorize_tool("create_note");
        let _ = s.authorize_tool("search_notes");
        s.record_result("read_note", true, "ok");
        s.kill("audit test");
        let _ = s.authorize_tool("read_note");
        assert!(s.audit().verify_chain(), "混合序列后链完整");
    }
}
