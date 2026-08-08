//! 插件权限控制 (Plugin Permission Control)
//!
//! 遵循最小权限原则，在运行时强制校验插件权限。支持动态升级：
//! 插件运行期间可请求清单外的新权限，由 `PermissionGrantor` 决定是否授予，
//! 全部授权决策记录至 `PermissionAuditLog` 以备审计。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::wasm_runtime::Capability;

/// 插件权限粒度。包含四大沙箱能力（映射到 `Capability`）以及若干 UI 级权限。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    /// 文件系统
    Fs,
    /// 网络
    Network,
    /// 原生宿主调用
    Native,
    /// 文档/存储
    Storage,
    /// 剪贴板
    Clipboard,
    /// 系统通知
    Notifications,
    /// 模态对话框
    Dialog,
}

impl Permission {
    /// 从字符串解析权限。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "fs" => Some(Permission::Fs),
            "network" => Some(Permission::Network),
            "native" => Some(Permission::Native),
            "storage" => Some(Permission::Storage),
            "clipboard" => Some(Permission::Clipboard),
            "notifications" => Some(Permission::Notifications),
            "dialog" => Some(Permission::Dialog),
            _ => None,
        }
    }

    /// 映射为沙箱能力（若适用）。
    pub fn as_capability(&self) -> Option<Capability> {
        match self {
            Permission::Fs => Some(Capability::Fs),
            Permission::Network => Some(Capability::Network),
            Permission::Native => Some(Capability::Native),
            Permission::Storage => Some(Capability::Storage),
            Permission::Clipboard | Permission::Notifications | Permission::Dialog => None,
        }
    }

    /// 返回权限字符串标识。
    pub fn as_str(&self) -> &'static str {
        match self {
            Permission::Fs => "fs",
            Permission::Network => "network",
            Permission::Native => "native",
            Permission::Storage => "storage",
            Permission::Clipboard => "clipboard",
            Permission::Notifications => "notifications",
            Permission::Dialog => "dialog",
        }
    }
}

/// 运行时权限请求：插件在执行期间请求追加权限时构造。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub plugin_id: String,
    pub permission: Permission,
    pub reason: String,
    pub requested_at: chrono::DateTime<chrono::Utc>,
}

impl PermissionRequest {
    pub fn new(
        plugin_id: impl Into<String>,
        permission: Permission,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            permission,
            reason: reason.into(),
            requested_at: chrono::Utc::now(),
        }
    }
}

/// 授权决策结果。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PermissionDecision {
    /// 授予
    Granted,
    /// 拒绝
    Denied,
    /// 撤销
    Revoked,
    /// 动态升级授予
    Upgraded,
}

/// 审计日志条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionAuditEntry {
    pub id: String,
    pub plugin_id: String,
    pub permission: Permission,
    pub decision: PermissionDecision,
    pub reason: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl PermissionAuditEntry {
    pub fn new(
        plugin_id: impl Into<String>,
        permission: Permission,
        decision: PermissionDecision,
        reason: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            plugin_id: plugin_id.into(),
            permission,
            decision,
            reason,
            timestamp: chrono::Utc::now(),
        }
    }
}

/// 权限审计日志。
#[derive(Debug, Clone, Default)]
pub struct PermissionAuditLog {
    entries: Arc<RwLock<Vec<PermissionAuditEntry>>>,
}

impl PermissionAuditLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一条审计条目。
    pub fn record(&self, entry: PermissionAuditEntry) {
        debug!(
            "audit: plugin={} perm={:?} decision={:?}",
            entry.plugin_id, entry.permission, entry.decision
        );
        self.entries.write().push(entry);
    }

    /// 返回全部条目快照。
    pub fn entries(&self) -> Vec<PermissionAuditEntry> {
        self.entries.read().clone()
    }

    /// 返回某插件的全部审计条目。
    pub fn for_plugin(&self, plugin_id: &str) -> Vec<PermissionAuditEntry> {
        self.entries
            .read()
            .iter()
            .filter(|e| e.plugin_id == plugin_id)
            .cloned()
            .collect()
    }

    /// 统计某决策类型的条目数。
    pub fn count_by_decision(&self, decision: PermissionDecision) -> usize {
        self.entries
            .read()
            .iter()
            .filter(|e| e.decision == decision)
            .count()
    }

    /// 日志总数。
    pub fn len(&self) -> usize {
        self.entries.read().len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.entries.read().is_empty()
    }
}

/// 权限授予者 trait：决定是否同意某项运行时权限请求。
pub trait PermissionGrantor: Send + Sync {
    fn decide(&self, request: &PermissionRequest) -> PermissionDecision;
}

/// 默认授予者：基于一份显式白名单决策。
pub struct DefaultGrantor {
    allowed: Arc<RwLock<HashSet<(String, Permission)>>>,
}

impl Default for DefaultGrantor {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultGrantor {
    pub fn new() -> Self {
        Self {
            allowed: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// 预先批准某插件某权限。
    pub fn allow(&self, plugin_id: impl Into<String>, perm: Permission) {
        self.allowed.write().insert((plugin_id.into(), perm));
    }

    /// 撤回某插件某权限的预批准。
    pub fn disallow(&self, plugin_id: &str, perm: &Permission) {
        self.allowed
            .write()
            .remove(&(plugin_id.to_string(), perm.clone()));
    }
}

impl PermissionGrantor for DefaultGrantor {
    fn decide(&self, request: &PermissionRequest) -> PermissionDecision {
        let allowed = self
            .allowed
            .read()
            .contains(&(request.plugin_id.clone(), request.permission.clone()));
        if allowed {
            PermissionDecision::Granted
        } else {
            warn!(
                "permission request denied: plugin={} perm={:?}",
                request.plugin_id, request.permission
            );
            PermissionDecision::Denied
        }
    }
}

/// 插件权限引擎：维护每插件已授予权限集合 + 审计日志，支持动态升级。
pub struct PermissionEngine {
    grants: Arc<RwLock<HashMap<String, HashSet<Permission>>>>,
    audit: PermissionAuditLog,
    grantor: Arc<dyn PermissionGrantor>,
}

impl PermissionEngine {
    /// 使用指定授予者构造引擎。
    pub fn new(grantor: Arc<dyn PermissionGrantor>) -> Self {
        Self {
            grants: Arc::new(RwLock::new(HashMap::new())),
            audit: PermissionAuditLog::new(),
            grantor,
        }
    }

    /// 使用默认授予者构造引擎。
    pub fn with_default_grantor() -> Self {
        Self::new(Arc::new(DefaultGrantor::new()))
    }

    /// 返回审计日志引用。
    pub fn audit(&self) -> &PermissionAuditLog {
        &self.audit
    }

    /// 授予权限（带可选原因）。
    pub fn grant(
        &self,
        plugin_id: &str,
        perm: Permission,
        reason: Option<String>,
    ) -> Result<(), crate::Error> {
        self.grants
            .write()
            .entry(plugin_id.to_string())
            .or_default()
            .insert(perm.clone());
        self.audit.record(PermissionAuditEntry::new(
            plugin_id,
            perm,
            PermissionDecision::Granted,
            reason,
        ));
        Ok(())
    }

    /// 撤销权限。
    pub fn revoke(&self, plugin_id: &str, perm: Permission) -> Result<(), crate::Error> {
        let mut grants = self.grants.write();
        let existed = grants
            .get_mut(plugin_id)
            .map(|set| set.remove(&perm))
            .unwrap_or(false);
        if !existed {
            return Err(crate::Error::PermissionDenied(format!(
                "permission {:?} not granted to {}",
                perm, plugin_id
            )));
        }
        self.audit.record(PermissionAuditEntry::new(
            plugin_id,
            perm,
            PermissionDecision::Revoked,
            None,
        ));
        Ok(())
    }

    /// 运行时权限检查：返回是否已授予。
    pub fn check(&self, plugin_id: &str, perm: &Permission) -> bool {
        self.grants
            .read()
            .get(plugin_id)
            .map(|set| set.contains(perm))
            .unwrap_or(false)
    }

    /// 确保权限已授予，否则返回错误。
    pub fn ensure(&self, plugin_id: &str, perm: &Permission) -> Result<(), crate::Error> {
        if self.check(plugin_id, perm) {
            Ok(())
        } else {
            Err(crate::Error::PermissionDenied(format!(
                "plugin {} lacks permission {:?}",
                plugin_id, perm
            )))
        }
    }

    /// 动态权限请求：询问授予者，通过则升级授权，否则拒绝。
    pub fn request(
        &self,
        plugin_id: &str,
        perm: Permission,
        reason: impl Into<String>,
    ) -> Result<(), crate::Error> {
        let request = PermissionRequest::new(plugin_id, perm.clone(), reason);
        match self.grantor.decide(&request) {
            PermissionDecision::Granted => {
                self.grants
                    .write()
                    .entry(plugin_id.to_string())
                    .or_default()
                    .insert(perm.clone());
                self.audit.record(PermissionAuditEntry::new(
                    plugin_id,
                    perm,
                    PermissionDecision::Upgraded,
                    Some(request.reason),
                ));
                info!(
                    "permission upgraded: plugin={} perm={:?}",
                    plugin_id, request.permission
                );
                Ok(())
            }
            _ => {
                self.audit.record(PermissionAuditEntry::new(
                    plugin_id,
                    perm,
                    PermissionDecision::Denied,
                    Some(request.reason),
                ));
                Err(crate::Error::PermissionDenied(format!(
                    "permission {:?} denied for {}",
                    request.permission, plugin_id
                )))
            }
        }
    }

    /// 返回某插件当前已授予的全部权限。
    pub fn permissions(&self, plugin_id: &str) -> HashSet<Permission> {
        self.grants
            .read()
            .get(plugin_id)
            .cloned()
            .unwrap_or_default()
    }

    /// 返回授予者（用于配置预批准白名单等）。
    pub fn grantor(&self) -> &dyn PermissionGrantor {
        self.grantor.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_parse_and_capability() {
        assert_eq!(Permission::parse("fs"), Some(Permission::Fs));
        assert_eq!(Permission::parse("clipboard"), Some(Permission::Clipboard));
        assert_eq!(Permission::parse("nope"), None);

        assert_eq!(Permission::Fs.as_capability(), Some(Capability::Fs));
        assert_eq!(
            Permission::Network.as_capability(),
            Some(Capability::Network)
        );
        assert_eq!(Permission::Clipboard.as_capability(), None);
        assert_eq!(Permission::Storage.as_str(), "storage");
    }

    #[test]
    fn test_permission_engine_grant_check() {
        let engine = PermissionEngine::with_default_grantor();
        engine
            .grant("p1", Permission::Storage, Some("seed".into()))
            .unwrap();
        assert!(engine.check("p1", &Permission::Storage));
        assert!(!engine.check("p1", &Permission::Network));
        assert!(engine.ensure("p1", &Permission::Storage).is_ok());
        assert!(engine.ensure("p1", &Permission::Fs).is_err());
    }

    #[test]
    fn test_permission_engine_revoke() {
        let engine = PermissionEngine::with_default_grantor();
        engine.grant("p1", Permission::Network, None).unwrap();
        assert!(engine.check("p1", &Permission::Network));
        engine.revoke("p1", Permission::Network).unwrap();
        assert!(!engine.check("p1", &Permission::Network));
        // 再次撤销应失败
        assert!(engine.revoke("p1", Permission::Network).is_err());
    }

    #[test]
    fn test_permission_engine_request_granted_upgrade() {
        let grantor = Arc::new(DefaultGrantor::new());
        grantor.allow("p1", Permission::Fs);
        let engine = PermissionEngine::new(grantor);

        // 初始未授予 Fs
        assert!(!engine.check("p1", &Permission::Fs));
        // 运行时请求 → 授予者同意 → 升级
        engine
            .request("p1", Permission::Fs, "need fs to export")
            .unwrap();
        assert!(engine.check("p1", &Permission::Fs));

        // 审计记录应包含 Upgraded
        let upgrades = engine
            .audit()
            .count_by_decision(PermissionDecision::Upgraded);
        assert_eq!(upgrades, 1);
    }

    #[test]
    fn test_permission_engine_request_denied() {
        let engine = PermissionEngine::with_default_grantor();
        let err = engine
            .request("p1", Permission::Network, "want net")
            .unwrap_err();
        assert!(matches!(err, crate::Error::PermissionDenied(_)));
        assert!(!engine.check("p1", &Permission::Network));
        assert_eq!(
            engine.audit().count_by_decision(PermissionDecision::Denied),
            1
        );
    }

    #[test]
    fn test_permission_audit_log_records() {
        let engine = PermissionEngine::with_default_grantor();
        engine.grant("p1", Permission::Storage, None).unwrap();
        engine.grant("p1", Permission::Fs, None).unwrap();
        engine.revoke("p1", Permission::Storage).unwrap();

        let log = engine.audit();
        assert_eq!(log.len(), 3);
        assert_eq!(log.count_by_decision(PermissionDecision::Granted), 2);
        assert_eq!(log.count_by_decision(PermissionDecision::Revoked), 1);
    }

    #[test]
    fn test_permission_audit_for_plugin() {
        let engine = PermissionEngine::with_default_grantor();
        engine.grant("p1", Permission::Storage, None).unwrap();
        engine.grant("p2", Permission::Fs, None).unwrap();

        let p1 = engine.audit().for_plugin("p1");
        assert_eq!(p1.len(), 1);
        assert_eq!(p1[0].permission, Permission::Storage);
        let p2 = engine.audit().for_plugin("p2");
        assert_eq!(p2.len(), 1);
        assert_eq!(p2[0].permission, Permission::Fs);
    }

    #[test]
    fn test_default_grantor_decide() {
        let grantor = DefaultGrantor::new();
        let req = PermissionRequest::new("p1", Permission::Network, "x");
        assert_eq!(grantor.decide(&req), PermissionDecision::Denied);

        grantor.allow("p1", Permission::Network);
        assert_eq!(grantor.decide(&req), PermissionDecision::Granted);

        grantor.disallow("p1", &Permission::Network);
        assert_eq!(grantor.decide(&req), PermissionDecision::Denied);
    }

    #[test]
    fn test_permissions_snapshot() {
        let engine = PermissionEngine::with_default_grantor();
        engine.grant("p1", Permission::Storage, None).unwrap();
        engine.grant("p1", Permission::Fs, None).unwrap();
        let perms = engine.permissions("p1");
        assert_eq!(perms.len(), 2);
        assert!(perms.contains(&Permission::Storage));
        assert!(perms.contains(&Permission::Fs));
    }
}
