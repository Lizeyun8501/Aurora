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

impl<T: PolicyResolver + ?Sized> PolicyResolver for std::sync::Arc<T> {
    fn policy_for(&self, workspace_id: &str) -> WorkspacePolicy {
        (**self).policy_for(workspace_id)
    }
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

    /// Arc 共享构造——供装配层把网关适配为 `Arc<dyn Fn>` 闭包
    /// 注入 provider（`PolicyResolver` 对象安全，`R = Arc<dyn PolicyResolver>` 同法）。
    pub fn new_arc(resolver: std::sync::Arc<R>) -> PolicyGate<std::sync::Arc<R>> {
        PolicyGate { resolver }
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

/// 生产 `PolicyResolver`：从工作区配置（KV `ws-policy:{id}`）读策略。
///
/// KVStore 为 async 接口而 [`PolicyResolver::policy_for`] 为同步——采用
/// **预载式**设计：装配期调用 [`Self::refresh_from`] 把各工作区策略拉进
/// 内存映射，运行时 [`policy_for`] 同步查表（缺省 [`WorkspacePolicy::AllowCloud`]，
/// 与 KV 键缺省语义一致）。bytes 内容 `"deny"`（不区分大小写）即 DenyCloud。
///
/// （DK-10 重派第一切片：原 Charlie 职责，实地锚点 crates/aurora-ai/src/policy.rs）
pub struct WorkspaceConfigResolver {
    policies: std::sync::RwLock<std::collections::HashMap<String, WorkspacePolicy>>,
}

impl WorkspaceConfigResolver {
    /// 空表构造（全部工作区缺省 AllowCloud）。
    pub fn new() -> Self {
        Self {
            policies: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// 装配期预载/刷新：从 KV 读各工作区 `ws-policy:{id}` 键。
    /// 键缺失 → AllowCloud（缺省契约）；bytes == b"deny"（忽略大小写）→ DenyCloud。
    pub async fn refresh_from(
        &self,
        kv: &dyn aurora_core::traits::kv_store::KVStore,
        workspace_ids: &[&str],
    ) -> Result<(), Error> {
        for id in workspace_ids {
            let key = format!("ws-policy:{id}");
            let policy = match kv.get(&key).await? {
                Some(bytes) if bytes.eq_ignore_ascii_case(b"deny") => WorkspacePolicy::DenyCloud,
                _ => WorkspacePolicy::AllowCloud,
            };
            self.set_policy(id, policy);
        }
        Ok(())
    }

    /// 运行时直设（测试/无 KV 场景/管理端变更即时生效）。
    pub fn set_policy(&self, workspace_id: &str, policy: WorkspacePolicy) {
        self.policies
            .write()
            .expect("workspace policy lock poisoned")
            .insert(workspace_id.to_string(), policy);
    }
}

impl Default for WorkspaceConfigResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyResolver for WorkspaceConfigResolver {
    fn policy_for(&self, workspace_id: &str) -> WorkspacePolicy {
        self.policies
            .read()
            .expect("workspace policy lock poisoned")
            .get(workspace_id)
            .copied()
            .unwrap_or(WorkspacePolicy::AllowCloud)
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

/// 内存 KVStore mock（resolver 预载测试用）。
#[allow(dead_code)] // mock 全量实现 trait，部分方法测试未触达
struct MockKv(std::collections::HashMap<String, Vec<u8>>);

#[async_trait::async_trait]
impl aurora_core::traits::kv_store::KVStore for MockKv {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, aurora_core::Error> {
        Ok(self.0.get(key).cloned())
    }
    async fn set(&self, _key: &str, _value: &[u8]) -> Result<(), aurora_core::Error> {
        unimplemented!("resolver 只读")
    }
    async fn delete(&self, _key: &str) -> Result<(), aurora_core::Error> {
        unimplemented!("resolver 只读")
    }
    async fn scan_prefix(
        &self,
        _prefix: &str,
    ) -> Result<Vec<(String, Vec<u8>)>, aurora_core::Error> {
        Ok(Vec::new())
    }
    async fn exists(&self, key: &str) -> Result<bool, aurora_core::Error> {
        Ok(self.0.contains_key(key))
    }
    async fn batch_get(&self, keys: &[&str]) -> Result<Vec<Option<Vec<u8>>>, aurora_core::Error> {
        Ok(keys.iter().map(|k| self.0.get(*k).cloned()).collect())
    }
    async fn batch_set(&self, _kvs: &[(&str, &[u8])]) -> Result<(), aurora_core::Error> {
        unimplemented!("resolver 只读")
    }
}

#[tokio::test]
async fn workspace_config_resolver_reads_kv_policy() {
    let kv = MockKv(std::collections::HashMap::from([
        ("ws-policy:private-ws".to_string(), b"deny".to_vec()),
        ("ws-policy:upper-ws".to_string(), b"DENY".to_vec()),
        ("ws-policy:other".to_string(), b"junk-value".to_vec()),
    ]));
    let resolver = WorkspaceConfigResolver::new();
    resolver
        .refresh_from(&kv, &["private-ws", "upper-ws", "other", "missing-ws"])
        .await
        .expect("refresh ok");
    assert_eq!(
        resolver.policy_for("private-ws"),
        WorkspacePolicy::DenyCloud,
        "b\"deny\" → DenyCloud"
    );
    assert_eq!(
        resolver.policy_for("upper-ws"),
        WorkspacePolicy::DenyCloud,
        "大小写不敏感"
    );
    assert_eq!(
        resolver.policy_for("other"),
        WorkspacePolicy::AllowCloud,
        "非 deny 值 → 缺省 Allow"
    );
    assert_eq!(
        resolver.policy_for("missing-ws"),
        WorkspacePolicy::AllowCloud,
        "键缺失 → 缺省 Allow（契约）"
    );
}

#[test]
fn workspace_config_resolver_runtime_set() {
    let resolver = WorkspaceConfigResolver::new();
    assert_eq!(resolver.policy_for("ws"), WorkspacePolicy::AllowCloud);
    resolver.set_policy("ws", WorkspacePolicy::DenyCloud);
    assert_eq!(resolver.policy_for("ws"), WorkspacePolicy::DenyCloud);
}
