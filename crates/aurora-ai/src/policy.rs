//! 工作区云策略网关（DK-07 DoD 2 接口冻结切片）。
//!
//! **契约**：云端 AI 请求对 Private（云禁用）工作区一律被阻止——
//! 决策点在请求发起前（fail-closed），而非请求后过滤。
//!
//! 分工（见 reports/协同开发分工分析.md）：
//! - Alpha（本切片）：冻结 [`WorkspacePolicy`] / [`PolicyResolver`] / [`PolicyGate`]
//! - Charlie：实现生产 [`PolicyResolver`]（从工作区配置/加密级别读取），
//!   并把 [`PolicyGate`] 接入 [`cloud::OpenAiCompatProvider`] 调用链

use aurora_core::Error;

/// 工作区云端请求策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkspacePolicy {
    /// 允许云端 AI 请求（Shared/Public 或用户显式开启）
    #[default]
    AllowCloud,
    /// 禁止云端 AI 请求（Private 工作区 — DoD 2: 请求被阻止）
    DenyCloud,
}

/// 工作区策略解析器（生产实现由 Charlie 接线：工作区配置 → 策略）。
pub trait PolicyResolver: Send + Sync {
    /// 解析指定工作区的云端请求策略。
    fn policy_for(&self, workspace_id: &str) -> WorkspacePolicy;
}

/// 策略网关：AI 请求发起前的强制检查点（fail-closed）。
///
/// 装配层把网关置于 provider 调用链之前；任何 DenyCloud 工作区
/// 的请求在离开本机前被拒绝，内容永不触网。
pub struct PolicyGate<R: PolicyResolver> {
    resolver: R,
}

impl<R: PolicyResolver> PolicyGate<R> {
    /// 构造网关。
    pub fn new(resolver: R) -> Self {
        Self { resolver }
    }

    /// 请求前检查。DenyCloud → Err（不发起网络请求）。
    ///
    /// # Errors
    /// - 工作区策略为 [`WorkspacePolicy::DenyCloud`] 时返回
    ///   [`Error::PermissionDenied`]（fail-closed：拒绝即不触网）。
    pub fn guard(&self, workspace_id: &str) -> Result<(), Error> {
        match self.resolver.policy_for(workspace_id) {
            WorkspacePolicy::AllowCloud => Ok(()),
            WorkspacePolicy::DenyCloud => Err(Error::PermissionDenied(format!(
                "cloud AI request blocked: workspace '{workspace_id}' is private (DK-07 DoD 2)"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// 测试用解析器：id 含 "private" → DenyCloud。
    struct MockResolver(HashMap<String, WorkspacePolicy>);

    impl PolicyResolver for MockResolver {
        fn policy_for(&self, id: &str) -> WorkspacePolicy {
            self.0
                .get(id)
                .copied()
                .unwrap_or(WorkspacePolicy::AllowCloud)
        }
    }

    #[test]
    fn dk07_private_workspace_cloud_request_blocked() {
        let mut map = HashMap::new();
        map.insert("ws-private".into(), WorkspacePolicy::DenyCloud);
        let gate = PolicyGate::new(MockResolver(map));

        // Private → 拒绝，且错误信息明确指出原因（fail-closed）
        let err = gate.guard("ws-private").unwrap_err().to_string();
        assert!(err.contains("private"), "错误须指明 Private 拦截: {err}");
    }

    #[test]
    fn dk07_public_workspace_cloud_request_allowed() {
        let mut map = HashMap::new();
        map.insert("ws-private".into(), WorkspacePolicy::DenyCloud);
        let gate = PolicyGate::new(MockResolver(map));

        // Shared/Public/未知 → 放行
        assert!(gate.guard("ws-public").is_ok());
        assert!(gate.guard("ws-unknown").is_ok());
    }
}
