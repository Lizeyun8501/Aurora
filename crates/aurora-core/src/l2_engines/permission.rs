//! 权限引擎 (Permission Engine)
//!
//! 基于 RBAC + ABAC 的混合权限模型，支持四级层级继承传播。
//!
//! # 核心设计
//! - RBAC：五级角色（Owner / Admin / Editor / Commenter / Viewer），每级角色拥有固定的权限集合。
//! - ABAC：在角色基础上附加属性条件（时间、IP、设备等），条件不满足时权限降级或拒绝。
//! - 传播：Workspace → Collection → Document → Block，下级默认继承上级权限，但可在任意层级显式覆盖。

use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::Arc;

use chrono::{Datelike, Local, NaiveTime};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// 五级角色枚举
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Role {
    /// 所有者：拥有一切权限，包括删除 Workspace、转让所有权
    Owner,
    /// 管理员：除转让所有权外，可管理成员与设置
    Admin,
    /// 编辑者：可读写内容，可评论
    Editor,
    /// 评论者：可查看与评论，不可修改内容
    Commenter,
    /// 查看者：仅只读
    Viewer,
}

impl Role {
    /// 返回角色级别数值，越大权限越高
    pub fn level(&self) -> u8 {
        match self {
            Role::Owner => 5,
            Role::Admin => 4,
            Role::Editor => 3,
            Role::Commenter => 2,
            Role::Viewer => 1,
        }
    }

    /// 返回该角色拥有的全部基础权限
    pub fn permissions(&self) -> HashSet<Permission> {
        let mut set = HashSet::new();
        match self {
            Role::Owner => {
                set.insert(Permission::Read);
                set.insert(Permission::Write);
                set.insert(Permission::Delete);
                set.insert(Permission::Comment);
                set.insert(Permission::ManageUsers);
                set.insert(Permission::ManageSettings);
                set.insert(Permission::TransferOwnership);
            }
            Role::Admin => {
                set.insert(Permission::Read);
                set.insert(Permission::Write);
                set.insert(Permission::Delete);
                set.insert(Permission::Comment);
                set.insert(Permission::ManageUsers);
                set.insert(Permission::ManageSettings);
            }
            Role::Editor => {
                set.insert(Permission::Read);
                set.insert(Permission::Write);
                set.insert(Permission::Comment);
            }
            Role::Commenter => {
                set.insert(Permission::Read);
                set.insert(Permission::Comment);
            }
            Role::Viewer => {
                set.insert(Permission::Read);
            }
        }
        set
    }

    /// 检查该角色是否天然拥有某权限（不考虑 ABAC）
    pub fn has_permission(&self, permission: Permission) -> bool {
        self.permissions().contains(&permission)
    }
}

/// 资源类型，对应四级层级
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ResourceType {
    Workspace,
    Collection,
    Document,
    Block,
}

impl ResourceType {
    /// 返回层级深度，越深数值越大（Block 最大）
    pub fn depth(&self) -> u8 {
        match self {
            ResourceType::Workspace => 1,
            ResourceType::Collection => 2,
            ResourceType::Document => 3,
            ResourceType::Block => 4,
        }
    }

    /// 返回父级资源类型（若存在）
    pub fn parent(&self) -> Option<ResourceType> {
        match self {
            ResourceType::Workspace => None,
            ResourceType::Collection => Some(ResourceType::Workspace),
            ResourceType::Document => Some(ResourceType::Collection),
            ResourceType::Block => Some(ResourceType::Document),
        }
    }
}

/// 权限操作粒度
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Permission {
    /// 读取
    Read,
    /// 写入（创建、更新）
    Write,
    /// 删除
    Delete,
    /// 评论
    Comment,
    /// 管理成员
    ManageUsers,
    /// 管理设置（权限策略、属性配置等）
    ManageSettings,
    /// 转让所有权
    TransferOwnership,
}

/// 资源标识符
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ResourceId {
    pub resource_type: ResourceType,
    pub id: String,
}

impl ResourceId {
    pub fn new(resource_type: ResourceType, id: impl Into<String>) -> Self {
        Self {
            resource_type,
            id: id.into(),
        }
    }
}

/// 请求上下文，包含环境属性，用于 ABAC 判断
#[derive(Debug, Clone, Default)]
pub struct AccessContext {
    /// 请求者 IP
    pub client_ip: Option<IpAddr>,
    /// 请求时间（未指定时使用系统当前时间）
    pub request_time: Option<chrono::DateTime<Local>>,
    /// 设备标识
    pub device_id: Option<String>,
    /// 用户代理
    pub user_agent: Option<String>,
    /// 额外自定义属性
    pub extra: HashMap<String, serde_json::Value>,
}

/// ABAC 属性条件
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AbacCondition {
    /// 始终成立
    Always,
    /// 始终不成立
    Never,
    /// 仅允许指定星期几（1=Mon, 7=Sun）
    Weekdays(Vec<u8>),
    /// 仅允许指定时间段（本地时间）
    TimeRange { start: String, end: String },
    /// IP 白名单
    IpWhitelist(Vec<String>),
    /// IP 黑名单
    IpBlacklist(Vec<String>),
    /// 设备 ID 必须匹配前缀
    DevicePrefix(String),
    /// 自定义属性等于某值
    CustomEq { key: String, value: serde_json::Value },
    /// 多条件：全部满足
    And(Vec<AbacCondition>),
    /// 多条件：任一满足
    Or(Vec<AbacCondition>),
    /// 条件取反
    Not(Box<AbacCondition>),
}

impl AbacCondition {
    /// 评估条件在特定上下文下是否成立
    pub fn evaluate(&self, ctx: &AccessContext) -> bool {
        match self {
            AbacCondition::Always => true,
            AbacCondition::Never => false,
            AbacCondition::Weekdays(allowed) => {
                let now = ctx.request_time.unwrap_or_else(Local::now);
                let wd = now.weekday().number_from_monday() as u8;
                allowed.contains(&wd)
            }
            AbacCondition::TimeRange { start, end } => {
                let now = ctx.request_time.unwrap_or_else(Local::now);
                let time = now.time();
                let start_t = NaiveTime::parse_from_str(start, "%H:%M").ok();
                let end_t = NaiveTime::parse_from_str(end, "%H:%M").ok();
                match (start_t, end_t) {
                    (Some(s), Some(e)) => time >= s && time <= e,
                    _ => {
                        warn!("Invalid time range format: {} - {}", start, end);
                        false
                    }
                }
            }
            AbacCondition::IpWhitelist(ips) => {
                if let Some(ip) = ctx.client_ip {
                    ips.iter().any(|s| parse_ip_or_cidr(s).map(|range| range.contains(ip)).unwrap_or(false))
                } else {
                    false
                }
            }
            AbacCondition::IpBlacklist(ips) => {
                if let Some(ip) = ctx.client_ip {
                    !ips.iter().any(|s| parse_ip_or_cidr(s).map(|range| range.contains(ip)).unwrap_or(false))
                } else {
                    true
                }
            }
            AbacCondition::DevicePrefix(prefix) => ctx
                .device_id
                .as_ref()
                .map(|d| d.starts_with(prefix))
                .unwrap_or(false),
            AbacCondition::CustomEq { key, value } => ctx
                .extra
                .get(key)
                .map(|v| v == value)
                .unwrap_or(false),
            AbacCondition::And(conds) => conds.iter().all(|c| c.evaluate(ctx)),
            AbacCondition::Or(conds) => conds.iter().any(|c| c.evaluate(ctx)),
            AbacCondition::Not(cond) => !cond.evaluate(ctx),
        }
    }
}

/// 简易 IP 范围判断辅助结构
#[derive(Debug, Clone)]
enum IpRange {
    Single(IpAddr),
    V4Subnet { addr: std::net::Ipv4Addr, prefix: u8 },
    V6Subnet { addr: std::net::Ipv6Addr, prefix: u8 },
}

impl IpRange {
    fn contains(&self, ip: IpAddr) -> bool {
        match (self, ip) {
            (IpRange::Single(a), b) => *a == b,
            (IpRange::V4Subnet { addr, prefix }, IpAddr::V4(target)) => {
                let mask = if *prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - prefix)
                };
                let addr_u = u32::from(*addr);
                let target_u = u32::from(target);
                (addr_u & mask) == (target_u & mask)
            }
            (IpRange::V6Subnet { addr, prefix }, IpAddr::V6(target)) => {
                let addr_u128 = u128::from(*addr);
                let target_u128 = u128::from(target);
                let mask = if *prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - prefix)
                };
                (addr_u128 & mask) == (target_u128 & mask)
            }
            _ => false,
        }
    }
}

fn parse_ip_or_cidr(s: &str) -> Option<IpRange> {
    if let Ok(addr) = s.parse::<IpAddr>() {
        return Some(IpRange::Single(addr));
    }
    if let Some((ip_part, prefix_part)) = s.split_once('/') {
        let prefix: u8 = prefix_part.parse().ok()?;
        if let Ok(v4) = ip_part.parse::<std::net::Ipv4Addr>() {
            return Some(IpRange::V4Subnet { addr: v4, prefix });
        }
        if let Ok(v6) = ip_part.parse::<std::net::Ipv6Addr>() {
            return Some(IpRange::V6Subnet { addr: v6, prefix });
        }
    }
    None
}

/// 权限策略：角色 + 可选 ABAC 条件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub role: Role,
    /// 若条件为 None，则仅按 RBAC 判断；否则需同时满足条件
    pub condition: Option<AbacCondition>,
    /// 条件不满足时的降级角色（为 None 则直接拒绝）
    pub fallback_role: Option<Role>,
}

impl Policy {
    pub fn new(role: Role) -> Self {
        Self {
            role,
            condition: None,
            fallback_role: None,
        }
    }

    pub fn with_condition(mut self, condition: AbacCondition) -> Self {
        self.condition = Some(condition);
        self
    }

    pub fn with_fallback(mut self, role: Role) -> Self {
        self.fallback_role = Some(role);
        self
    }

    /// 在上下文中解析出实际生效的角色
    pub fn effective_role(&self, ctx: &AccessContext) -> Option<Role> {
        match &self.condition {
            Some(cond) if cond.evaluate(ctx) => Some(self.role),
            Some(_) => self.fallback_role,
            None => Some(self.role),
        }
    }
}

/// 用户对某资源的绑定记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assignment {
    pub user_id: String,
    pub resource_id: ResourceId,
    pub policy: Policy,
}

/// 权限评估结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionResult {
    Allow,
    Deny,
    /// 条件不满足，权限被拒绝并附带原因
    DenyWithReason(String),
}

impl PermissionResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, PermissionResult::Allow)
    }
}

/// 层级传播策略
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum InheritanceStrategy {
    /// 严格继承：下级权限不得超过上级（默认）
    Strict,
    /// 宽松继承：下级可显式覆盖，即使上级未授权
    Permissive,
}

/// 权限引擎
pub struct PermissionEngine {
    /// 用户-资源-策略映射
    assignments: Arc<RwLock<HashMap<(String, ResourceId), Policy>>>,
    /// 资源父子关系：子资源 ID -> 父资源 ID
    resource_parents: Arc<RwLock<HashMap<ResourceId, ResourceId>>>,
    /// 全局默认策略（未命中任何显式绑定时使用）
    default_policy: Arc<RwLock<Option<Policy>>>,
    /// 传播策略
    inheritance: InheritanceStrategy,
}

impl PermissionEngine {
    pub fn new() -> Self {
        Self {
            assignments: Arc::new(RwLock::new(HashMap::new())),
            resource_parents: Arc::new(RwLock::new(HashMap::new())),
            default_policy: Arc::new(RwLock::new(None)),
            inheritance: InheritanceStrategy::Strict,
        }
    }

    pub fn with_inheritance(mut self, strategy: InheritanceStrategy) -> Self {
        self.inheritance = strategy;
        self
    }

    /// 设置全局默认策略
    pub fn set_default_policy(&self, policy: Policy) {
        *self.default_policy.write() = Some(policy);
    }

    /// 注册资源父子关系
    pub fn register_parent(&self, child: ResourceId, parent: ResourceId) {
        self.resource_parents.write().insert(child, parent);
    }

    /// 移除资源父子关系
    pub fn unregister_parent(&self, child: &ResourceId) {
        self.resource_parents.write().remove(child);
    }

    /// 为用户分配策略
    pub fn assign(&self, user_id: impl Into<String>, resource_id: ResourceId, policy: Policy) {
        self.assignments
            .write()
            .insert((user_id.into(), resource_id), policy);
    }

    /// 撤销用户策略
    pub fn revoke(&self, user_id: &str, resource_id: &ResourceId) {
        self.assignments.write().remove(&(user_id.to_string(), resource_id.clone()));
    }

    /// 检查某用户是否对某资源拥有指定权限
    pub fn check(
        &self,
        user_id: &str,
        resource_id: &ResourceId,
        permission: Permission,
        ctx: &AccessContext,
    ) -> PermissionResult {
        debug!(
            "check permission: user={} resource={:?} perm={:?}",
            user_id, resource_id, permission
        );

        let effective_role = match self.resolve_effective_role(user_id, resource_id, ctx) {
            Some(role) => role,
            None => {
                return PermissionResult::DenyWithReason(
                    "No applicable policy found".to_string(),
                );
            }
        };

        if effective_role.has_permission(permission) {
            PermissionResult::Allow
        } else {
            PermissionResult::DenyWithReason(format!(
                "Role {:?} does not have {:?} permission",
                effective_role, permission
            ))
        }
    }

    /// 检查某用户是否对某资源拥有指定权限（便捷方法，使用默认上下文）
    pub fn check_simple(
        &self,
        user_id: &str,
        resource_id: &ResourceId,
        permission: Permission,
    ) -> PermissionResult {
        self.check(user_id, resource_id, permission, &AccessContext::default())
    }

    /// 获取某用户对某资源在上下文中的有效角色（考虑传播）
    pub fn resolve_effective_role(
        &self,
        user_id: &str,
        resource_id: &ResourceId,
        ctx: &AccessContext,
    ) -> Option<Role> {
        // 1. 直接在该资源上的策略
        let direct = self
            .assignments
            .read()
            .get(&(user_id.to_string(), resource_id.clone()))
            .and_then(|p| p.effective_role(ctx));

        // 2. 向上传播查找祖先策略
        let inherited = self.resolve_inherited_role(user_id, resource_id, ctx);

        // 3. 合并直接策略与继承策略
        let merged = match (direct, inherited) {
            (Some(d), Some(i)) => {
                if self.inheritance == InheritanceStrategy::Strict {
                    // 严格模式下取权限较小者（level 更小）
                    if d.level() <= i.level() { Some(d) } else { Some(i) }
                } else {
                    // 宽松模式下直接策略优先
                    Some(d)
                }
            }
            (Some(d), None) => Some(d),
            (None, Some(i)) => Some(i),
            (None, None) => None,
        };

        // 4. 若全部未命中，使用全局默认策略
        merged.or_else(|| {
            self.default_policy
                .read()
                .as_ref()
                .and_then(|p| p.effective_role(ctx))
        })
    }

    /// 向上递归解析继承角色
    fn resolve_inherited_role(
        &self,
        user_id: &str,
        resource_id: &ResourceId,
        ctx: &AccessContext,
    ) -> Option<Role> {
        let parents = self.resource_parents.read();
        let mut current = resource_id.clone();
        let mut best_role: Option<Role> = None;

        while let Some(parent) = parents.get(&current) {
            if let Some(policy) = self
                .assignments
                .read()
                .get(&(user_id.to_string(), parent.clone()))
            {
                if let Some(role) = policy.effective_role(ctx) {
                    best_role = match best_role {
                        Some(r) if self.inheritance == InheritanceStrategy::Strict => {
                            if role.level() < r.level() {
                                Some(role)
                            } else {
                                Some(r)
                            }
                        }
                        _ => Some(role),
                    };
                }
            }
            current = parent.clone();
        }

        best_role
    }

    /// 列出某用户对某资源拥有的全部权限（基于有效角色）
    pub fn list_permissions(
        &self,
        user_id: &str,
        resource_id: &ResourceId,
        ctx: &AccessContext,
    ) -> HashSet<Permission> {
        self.resolve_effective_role(user_id, resource_id, ctx)
            .map(|r| r.permissions())
            .unwrap_or_default()
    }

    /// 批量检查权限（只要有一项不满足即返回 Deny）
    pub fn check_all(
        &self,
        user_id: &str,
        resource_id: &ResourceId,
        permissions: &[Permission],
        ctx: &AccessContext,
    ) -> PermissionResult {
        for perm in permissions {
            let result = self.check(user_id, resource_id, *perm, ctx);
            if !result.is_allowed() {
                return result;
            }
        }
        PermissionResult::Allow
    }

    /// 批量检查权限（任一满足即返回 Allow）
    pub fn check_any(
        &self,
        user_id: &str,
        resource_id: &ResourceId,
        permissions: &[Permission],
        ctx: &AccessContext,
    ) -> PermissionResult {
        let mut last_deny = PermissionResult::Deny;
        for perm in permissions {
            let result = self.check(user_id, resource_id, *perm, ctx);
            if result.is_allowed() {
                return PermissionResult::Allow;
            }
            last_deny = result;
        }
        last_deny
    }

    /// 返回某用户的全部资源权限快照（用于前端展示）
    pub fn user_permissions_snapshot(
        &self,
        user_id: &str,
        ctx: &AccessContext,
    ) -> HashMap<ResourceId, HashSet<Permission>> {
        let assignments = self.assignments.read();
        let mut result: HashMap<ResourceId, HashSet<Permission>> = HashMap::new();

        for ((uid, res_id), policy) in assignments.iter() {
            if uid == user_id {
                if let Some(role) = policy.effective_role(ctx) {
                    result.insert(res_id.clone(), role.permissions());
                }
            }
        }

        result
    }
}

impl Default for PermissionEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> PermissionEngine {
        PermissionEngine::new()
    }

    fn user(id: &str) -> String {
        id.to_string()
    }

    fn res_ws(id: &str) -> ResourceId {
        ResourceId::new(ResourceType::Workspace, id)
    }

    fn res_col(id: &str) -> ResourceId {
        ResourceId::new(ResourceType::Collection, id)
    }

    fn res_doc(id: &str) -> ResourceId {
        ResourceId::new(ResourceType::Document, id)
    }

    fn res_block(id: &str) -> ResourceId {
        ResourceId::new(ResourceType::Block, id)
    }

    #[test]
    fn test_rbac_role_permissions() {
        assert!(Role::Viewer.has_permission(Permission::Read));
        assert!(!Role::Viewer.has_permission(Permission::Write));
        assert!(!Role::Viewer.has_permission(Permission::Comment));

        assert!(Role::Commenter.has_permission(Permission::Read));
        assert!(Role::Commenter.has_permission(Permission::Comment));
        assert!(!Role::Commenter.has_permission(Permission::Write));

        assert!(Role::Editor.has_permission(Permission::Read));
        assert!(Role::Editor.has_permission(Permission::Write));
        assert!(Role::Editor.has_permission(Permission::Comment));
        assert!(!Role::Editor.has_permission(Permission::ManageUsers));

        assert!(Role::Admin.has_permission(Permission::ManageUsers));
        assert!(Role::Admin.has_permission(Permission::ManageSettings));
        assert!(!Role::Admin.has_permission(Permission::TransferOwnership));

        assert!(Role::Owner.has_permission(Permission::TransferOwnership));
        assert!(Role::Owner.has_permission(Permission::Delete));
    }

    #[test]
    fn test_direct_assignment() {
        let engine = make_engine();
        engine.assign(user("u1"), res_ws("ws1"), Policy::new(Role::Editor));

        let ctx = AccessContext::default();
        assert!(
            engine
                .check("u1", &res_ws("ws1"), Permission::Write, &ctx)
                .is_allowed()
        );
        assert!(
            !engine
                .check("u1", &res_ws("ws1"), Permission::ManageUsers, &ctx)
                .is_allowed()
        );
    }

    #[test]
    fn test_revoke() {
        let engine = make_engine();
        engine.assign(user("u1"), res_ws("ws1"), Policy::new(Role::Editor));
        engine.revoke("u1", &res_ws("ws1"));

        let ctx = AccessContext::default();
        assert!(
            !engine
                .check("u1", &res_ws("ws1"), Permission::Read, &ctx)
                .is_allowed()
        );
    }

    #[test]
    fn test_inheritance_workspace_to_block() {
        let engine = make_engine();
        engine.assign(user("u1"), res_ws("ws1"), Policy::new(Role::Editor));
        engine.register_parent(res_col("col1"), res_ws("ws1"));
        engine.register_parent(res_doc("doc1"), res_col("col1"));
        engine.register_parent(res_block("blk1"), res_doc("doc1"));

        let ctx = AccessContext::default();
        // Block 未显式分配，应继承 Workspace 的 Editor 权限
        assert!(
            engine
                .check("u1", &res_block("blk1"), Permission::Write, &ctx)
                .is_allowed()
        );
        assert!(
            !engine
                .check("u1", &res_block("blk1"), Permission::Delete, &ctx)
                .is_allowed()
        );
    }

    #[test]
    fn test_inheritance_strict_mode() {
        let engine = make_engine();
        // Workspace 分配 Admin
        engine.assign(user("u1"), res_ws("ws1"), Policy::new(Role::Admin));
        engine.register_parent(res_col("col1"), res_ws("ws1"));
        // Collection 显式分配 Viewer（权限更小）
        engine.assign(user("u1"), res_col("col1"), Policy::new(Role::Viewer));

        let ctx = AccessContext::default();
        // 在 Collection 上，Strict 模式下直接分配的 Viewer 与继承的 Admin 取较小者 => Viewer
        assert!(
            engine
                .check("u1", &res_col("col1"), Permission::Read, &ctx)
                .is_allowed()
        );
        assert!(
            !engine
                .check("u1", &res_col("col1"), Permission::Write, &ctx)
                .is_allowed()
        );
    }

    #[test]
    fn test_inheritance_permissive_mode() {
        let engine = PermissionEngine::new().with_inheritance(InheritanceStrategy::Permissive);
        engine.assign(user("u1"), res_ws("ws1"), Policy::new(Role::Viewer));
        engine.register_parent(res_col("col1"), res_ws("ws1"));
        engine.assign(user("u1"), res_col("col1"), Policy::new(Role::Editor));

        let ctx = AccessContext::default();
        // Permissive 模式下直接策略优先，Collection 可使用 Editor 覆盖上级 Viewer
        assert!(
            engine
                .check("u1", &res_col("col1"), Permission::Write, &ctx)
                .is_allowed()
        );
    }

    #[test]
    fn test_abac_weekday_condition() {
        let engine = make_engine();
        let weekday_policy = Policy::new(Role::Editor)
            .with_condition(AbacCondition::Weekdays(vec![1, 2, 3, 4, 5]));
        engine.assign(user("u1"), res_ws("ws1"), weekday_policy);

        // 构造一个工作日的上下文
        // 注：必须用 UTC 凌晨时间戳，否则在正偏移时区（如 Asia/Shanghai UTC+8）
        // 投影后会跨午夜变成周一，weekday=1 ∈ [1..5] 反而满足条件，断言反转。
        // 旧值 1_721_000_000 = UTC 2024-07-14 23:33 周日深夜 → 北京周一 → 假阳。
        // 新值 1_720_956_800 = UTC 2024-07-14 00:00 = 北京 08:00 仍周日 (weekday=7)。
        let weekday_ctx = AccessContext {
            request_time: Some(
                chrono::DateTime::from_timestamp(1_720_956_800, 0)
                    .unwrap()
                    .with_timezone(&Local),
            ),
            ..Default::default()
        };
        // UTC 2024-07-14 00:00 是周日 (weekday=7)，条件 [1..5] 不满足
        assert!(
            !engine
                .check("u1", &res_ws("ws1"), Permission::Write, &weekday_ctx)
                .is_allowed()
        );

        // 构造一个周一的上下文
        let monday_ctx = AccessContext {
            request_time: Some(
                chrono::DateTime::from_timestamp(1_721_200_000, 0)
                    .unwrap()
                    .with_timezone(&Local),
            ),
            ..Default::default()
        };
        // 该时间戳 2024-07-17 是周三 (weekday=3)，条件满足
        assert!(
            engine
                .check("u1", &res_ws("ws1"), Permission::Write, &monday_ctx)
                .is_allowed()
        );
    }

    #[test]
    fn test_abac_time_range_condition() {
        let engine = make_engine();
        let time_policy = Policy::new(Role::Editor).with_condition(AbacCondition::TimeRange {
            start: "09:00".to_string(),
            end: "18:00".to_string(),
        });
        engine.assign(user("u1"), res_ws("ws1"), time_policy);

        let noon_ctx = AccessContext {
            request_time: Some(
                chrono::NaiveDate::from_ymd_opt(2024, 1, 1)
                    .unwrap()
                    .and_hms_opt(12, 0, 0)
                    .unwrap()
                    .and_local_timezone(Local)
                    .unwrap(),
            ),
            ..Default::default()
        };
        assert!(
            engine
                .check("u1", &res_ws("ws1"), Permission::Write, &noon_ctx)
                .is_allowed()
        );

        let night_ctx = AccessContext {
            request_time: Some(
                chrono::NaiveDate::from_ymd_opt(2024, 1, 1)
                    .unwrap()
                    .and_hms_opt(22, 0, 0)
                    .unwrap()
                    .and_local_timezone(Local)
                    .unwrap(),
            ),
            ..Default::default()
        };
        assert!(
            !engine
                .check("u1", &res_ws("ws1"), Permission::Write, &night_ctx)
                .is_allowed()
        );
    }

    #[test]
    fn test_abac_ip_whitelist() {
        let engine = make_engine();
        let ip_policy = Policy::new(Role::Editor)
            .with_condition(AbacCondition::IpWhitelist(vec![
                "192.168.1.0/24".to_string(),
                "10.0.0.5".to_string(),
            ]));
        engine.assign(user("u1"), res_ws("ws1"), ip_policy);

        let allowed_ctx = AccessContext {
            client_ip: Some("192.168.1.100".parse().unwrap()),
            ..Default::default()
        };
        assert!(
            engine
                .check("u1", &res_ws("ws1"), Permission::Write, &allowed_ctx)
                .is_allowed()
        );

        let denied_ctx = AccessContext {
            client_ip: Some("8.8.8.8".parse().unwrap()),
            ..Default::default()
        };
        assert!(
            !engine
                .check("u1", &res_ws("ws1"), Permission::Write, &denied_ctx)
                .is_allowed()
        );
    }

    #[test]
    fn test_abac_fallback_role() {
        let engine = make_engine();
        let fallback_policy = Policy::new(Role::Editor)
            .with_condition(AbacCondition::Weekdays(vec![1, 2, 3, 4, 5]))
            .with_fallback(Role::Viewer);
        engine.assign(user("u1"), res_ws("ws1"), fallback_policy);

        // 周日访问，条件不满足，降级为 Viewer
        // 注：1_721_000_000 = UTC 2024-07-14 23:33 周日深夜 → 北京 (UTC+8) 周一 07:33，
        // 假阳满足 Weekdays([1..5]) 条件，无法触发 fallback。改用 1_720_956_800
        // = UTC 2024-07-14 00:00 = 北京 08:00，仍周日 (weekday=7)，触发 Viewer 降级。
        let sunday_ctx = AccessContext {
            request_time: Some(
                chrono::DateTime::from_timestamp(1_720_956_800, 0)
                    .unwrap()
                    .with_timezone(&Local),
            ),
            ..Default::default()
        };
        assert!(
            engine
                .check("u1", &res_ws("ws1"), Permission::Read, &sunday_ctx)
                .is_allowed()
        );
        assert!(
            !engine
                .check("u1", &res_ws("ws1"), Permission::Write, &sunday_ctx)
                .is_allowed()
        );
    }

    #[test]
    fn test_abac_composite_condition() {
        let engine = make_engine();
        let composite = AbacCondition::And(vec![
            AbacCondition::Weekdays(vec![1, 2, 3, 4, 5]),
            AbacCondition::IpWhitelist(vec!["192.168.1.0/24".to_string()]),
        ]);
        let policy = Policy::new(Role::Editor).with_condition(composite);
        engine.assign(user("u1"), res_ws("ws1"), policy);

        let ok_ctx = AccessContext {
            request_time: Some(
                chrono::DateTime::from_timestamp(1_721_200_000, 0)
                    .unwrap()
                    .with_timezone(&Local),
            ),
            client_ip: Some("192.168.1.10".parse().unwrap()),
            ..Default::default()
        };
        assert!(
            engine
                .check("u1", &res_ws("ws1"), Permission::Write, &ok_ctx)
                .is_allowed()
        );

        let bad_ip_ctx = AccessContext {
            request_time: Some(
                chrono::DateTime::from_timestamp(1_721_200_000, 0)
                    .unwrap()
                    .with_timezone(&Local),
            ),
            client_ip: Some("10.0.0.1".parse().unwrap()),
            ..Default::default()
        };
        assert!(
            !engine
                .check("u1", &res_ws("ws1"), Permission::Write, &bad_ip_ctx)
                .is_allowed()
        );
    }

    #[test]
    fn test_default_policy() {
        let engine = make_engine();
        engine.set_default_policy(Policy::new(Role::Viewer));

        let ctx = AccessContext::default();
        // u1 未在任何资源上显式分配，应走默认策略
        assert!(
            engine
                .check("u1", &res_ws("ws1"), Permission::Read, &ctx)
                .is_allowed()
        );
        assert!(
            !engine
                .check("u1", &res_ws("ws1"), Permission::Write, &ctx)
                .is_allowed()
        );
    }

    #[test]
    fn test_list_permissions() {
        let engine = make_engine();
        engine.assign(user("u1"), res_ws("ws1"), Policy::new(Role::Editor));

        let ctx = AccessContext::default();
        let perms = engine.list_permissions("u1", &res_ws("ws1"), &ctx);
        assert!(perms.contains(&Permission::Read));
        assert!(perms.contains(&Permission::Write));
        assert!(!perms.contains(&Permission::Delete));
    }

    #[test]
    fn test_batch_check_all() {
        let engine = make_engine();
        engine.assign(user("u1"), res_ws("ws1"), Policy::new(Role::Editor));

        let ctx = AccessContext::default();
        assert!(
            engine
                .check_all(
                    "u1",
                    &res_ws("ws1"),
                    &[Permission::Read, Permission::Write],
                    &ctx
                )
                .is_allowed()
        );
        assert!(
            !engine
                .check_all(
                    "u1",
                    &res_ws("ws1"),
                    &[Permission::Read, Permission::Delete],
                    &ctx
                )
                .is_allowed()
        );
    }

    #[test]
    fn test_batch_check_any() {
        let engine = make_engine();
        engine.assign(user("u1"), res_ws("ws1"), Policy::new(Role::Editor));

        let ctx = AccessContext::default();
        assert!(
            engine
                .check_any(
                    "u1",
                    &res_ws("ws1"),
                    &[Permission::Read, Permission::Delete],
                    &ctx
                )
                .is_allowed()
        );
        assert!(
            !engine
                .check_any(
                    "u1",
                    &res_ws("ws1"),
                    &[Permission::Delete, Permission::ManageUsers],
                    &ctx
                )
                .is_allowed()
        );
    }

    #[test]
    fn test_user_permissions_snapshot() {
        let engine = make_engine();
        engine.assign(user("u1"), res_ws("ws1"), Policy::new(Role::Editor));
        engine.assign(user("u1"), res_col("col1"), Policy::new(Role::Viewer));

        let ctx = AccessContext::default();
        let snapshot = engine.user_permissions_snapshot("u1", &ctx);
        assert_eq!(snapshot.len(), 2);
        assert!(snapshot.get(&res_ws("ws1")).unwrap().contains(&Permission::Write));
        assert!(!snapshot.get(&res_col("col1")).unwrap().contains(&Permission::Write));
    }

    #[test]
    fn test_resource_type_depth_and_parent() {
        assert_eq!(ResourceType::Workspace.depth(), 1);
        assert_eq!(ResourceType::Block.depth(), 4);
        assert_eq!(ResourceType::Collection.parent(), Some(ResourceType::Workspace));
        assert_eq!(ResourceType::Document.parent(), Some(ResourceType::Collection));
        assert_eq!(ResourceType::Block.parent(), Some(ResourceType::Document));
        assert_eq!(ResourceType::Workspace.parent(), None);
    }
}
