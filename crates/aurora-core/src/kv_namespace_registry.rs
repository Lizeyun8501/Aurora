//! KV 命名空间注册表（ADR-003「写入面分类」单一来源，O-02 落地）。
//!
//! ## A 类 / B 类分界（ADR-003 §写入面分类）
//!
//! - **A 类**（笔记/块/目录树/附件/元数据）——必须经 `write_path`
//!   （load → apply → 原子保存 → 派生 → 发事件）。A 类前缀在注册表中
//!   标记 `class: A`，任何 `core.kv_store.set` 直写命中 A 类前缀即
//!   CI 门禁错误（write_path 内部实现除外——豁免清单见 [`EXEMPT_CALLERS`]）。
//! - **B 类**（用户配置/UI 状态/缓存/派生投影）——允许直写 KV，但必须
//!   在本注册表登记（前缀/用途/含用户内容与否/同步/加密）。**含用户
//!   内容的 B 类必须 seal 加密**（落库即加密原则）；确未 seal 的存量
//!   在 `debt` 字段登记为 OPEN 债（修复后移除并恢复断言强制）。
//!
//! ## 未登记前缀
//!
//! debug 构建下 [`warn_if_unregistered`] 输出告警；CI 静态检查
//! （`scripts/check-kv-namespaces.mjs`）扫全仓 `kv_store.set(` 调用点
//! 字面量前缀，未登记即阻断。
//!
//! ## write_path 内部豁免
//!
//! `crates/aurora-core/src/write_path/` 是 A 类唯一合法直写层（自身
//! 实现）；`projection.rs`（投影水位线）与 `bootstrap`（启动种子）为
//! 框架层豁免，在 [`EXEMPT_CALLERS`] 登记。

/// 命名空间条目。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Namespace {
    /// KV key 前缀（冒号结尾形态按无冒号登记，匹配时兼容两种写法）。
    pub prefix: &'static str,
    /// A=笔记域（必须走 write_path）；B=配置/缓存（可直写但须登记）。
    pub class: KvClass,
    /// 用途一句话（人类可读，进 CI 报告）。
    pub purpose: &'static str,
    /// 是否含用户生成内容（B 类含用户内容须 seal）。
    pub user_content: bool,
    /// 是否参与跨端同步。
    pub sync: bool,
    /// 是否加密（seal）。
    pub sealed: bool,
    /// 登记时已知的债（实现缺口）。None=无债；Some=描述（修复后移除，
    /// 并恢复 tests 中 seal 断言的强制校验）。
    pub debt: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KvClass {
    /// 笔记域写入面——唯一合法写入方是 write_path。
    A,
    /// 配置/缓存/派生态——登记后允许直写。
    B,
}

/// 注册表本体（唯一来源——新增前缀必须在此登记，PR 审查点）。
pub static REGISTRY: &[Namespace] = &[
    // ── A 类（笔记域）──────────────────────────────────────────
    Namespace { prefix: "note", class: KvClass::A, purpose: "笔记正文/元数据（write_path 原子保存）", user_content: true, sync: true, sealed: true, debt: None },
    Namespace { prefix: "notesnap", class: KvClass::A, purpose: "笔记快照（WAL 快照先落）", user_content: true, sync: true, sealed: true, debt: None },
    Namespace { prefix: "trash", class: KvClass::A, purpose: "回收站游标（A 类软删域）", user_content: true, sync: true, sealed: true, debt: None },
    Namespace { prefix: "attachment", class: KvClass::A, purpose: "附件 blob/元数据（write_path/attach）", user_content: true, sync: true, sealed: true, debt: None },
    // ── B 类（配置/UI/缓存）────────────────────────────────────
    Namespace { prefix: "notevec", class: KvClass::B, purpose: "笔记向量索引（投影派生缓存,可全量重建;源 note: 已 A 类加密——缓存非权威用户数据副本）", user_content: false, sync: false, sealed: false, debt: None },
    Namespace { prefix: "event", class: KvClass::B, purpose: "事件存储（持久化事件流）", user_content: true, sync: false, sealed: false, debt: Some("O-02 登记: event_store 用户内容（日历事件标题等）明文落库——seal 改造另卡（O-02 停手边界: 不改实现）") },
    Namespace { prefix: "cal", class: KvClass::B, purpose: "日历事件（calendar_vm 数据面）", user_content: true, sync: false, sealed: false, debt: Some("O-02 登记: 日历事件标题明文——同 event: 债面,seal 改造另卡") },
    Namespace { prefix: "task", class: KvClass::B, purpose: "任务存储（task_store）", user_content: true, sync: false, sealed: false, debt: Some("O-02 登记: 任务标题明文——同 event: 债面,seal 改造另卡") },
    Namespace { prefix: "dbview", class: KvClass::B, purpose: "智能文件夹视图配置（collection_commands）", user_content: true, sync: false, sealed: false, debt: Some("O-02 登记: 查询条件含用户内容明文——seal 改造另卡") },
    Namespace { prefix: "palette", class: KvClass::B, purpose: "命令面板最近命令（palette_recents,C-12 上移 core——命令 ID 为产品词表非用户创作）", user_content: false, sync: false, sealed: false, debt: None },
    Namespace { prefix: "settings", class: KvClass::B, purpose: "用户设置（密钥位独立成 ai. 前缀且已 sealed——settings: 本体仅 UI 开关/偏好,产品侧配置非用户创作内容）", user_content: false, sync: true, sealed: false, debt: None },
    Namespace { prefix: "ai", class: KvClass::B, purpose: "AI 云密钥（sealed 形态存,legacy 明文位已迁移）", user_content: true, sync: false, sealed: true, debt: None },
    Namespace { prefix: "shortcut", class: KvClass::B, purpose: "快捷指令（settings:shortcuts——{action:combo} 键位绑定,产品动作映射非用户创作内容）", user_content: false, sync: false, sealed: false, debt: None },
    Namespace { prefix: "bookmark", class: KvClass::B, purpose: "书签/收藏（bookmark_store）", user_content: true, sync: false, sealed: false, debt: Some("O-02 登记: 用户收藏明文——seal 改造另卡") },
    Namespace { prefix: "fsrs", class: KvClass::B, purpose: "FSRS 复习队列", user_content: false, sync: false, sealed: false, debt: None },
    Namespace { prefix: "seed", class: KvClass::B, purpose: "启动种子（bootstrap 框架层）", user_content: false, sync: false, sealed: false, debt: None },
    Namespace { prefix: "share", class: KvClass::B, purpose: "分享追踪（share:trace:）", user_content: false, sync: false, sealed: false, debt: None },
    Namespace { prefix: "plugin", class: KvClass::B, purpose: "插件 KV（强制 {id}: 前缀隔离,DK-16 L1）", user_content: false, sync: false, sealed: false, debt: None },
    Namespace { prefix: "daily", class: KvClass::B, purpose: "每日笔记索引投影（projection.rs 事件驱动,可重建）", user_content: false, sync: false, sealed: false, debt: None },
    Namespace { prefix: "theme", class: KvClass::B, purpose: "用户自定义主题（theme:user:None=默认,A-11）", user_content: true, sync: false, sealed: false, debt: Some("O-02 登记: 自定义主题 JSON 明文——seal 改造另卡") },
    Namespace { prefix: "liq", class: KvClass::B, purpose: "AI 提案存档（liq:proposal:{id},aurora_ai）", user_content: true, sync: false, sealed: false, debt: Some("O-02 登记: AI 提案含笔记派生内容明文——seal 改造另卡") },
    Namespace { prefix: "plugins", class: KvClass::B, purpose: "插件 WASM 模块缓存（plugins:modcache:,fail-open 缓存可丢弃）", user_content: false, sync: false, sealed: false, debt: None },
    Namespace { prefix: "sync", class: KvClass::B, purpose: "同步水位线/节点态（aurora-sync）", user_content: false, sync: false, sealed: false, debt: None },
];

/// write_path / 框架层豁免（A 类前缀的合法直写者）。
pub static EXEMPT_CALLERS: &[&str] = &[
    "crates/aurora-core/src/write_path/",
    "crates/aurora-core/src/event_bus/projection.rs",
    "crates/aurora-bootstrap/src/lib.rs",
];

/// 按 key 查注册（前缀匹配，兼容 `note:` / `note{}` / `note` 三形态）。
pub fn lookup(key: &str) -> Option<&'static Namespace> {
    let head = key.split([':', '{']).next().unwrap_or(key);
    REGISTRY.iter().find(|n| n.prefix == head)
}

/// debug 告警：未登记前缀（生产静默——CI 静态检查兜底）。
pub fn warn_if_unregistered(key: &str) {
    if cfg!(debug_assertions) && lookup(key).is_none() {
        eprintln!(
            "[kv-namespace] 未登记前缀: {:?}——登记于 crates/aurora-core/src/kv_namespace_registry.rs（O-02/ADR-003）",
            key.split([':', '{']).next().unwrap_or(key)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_unique_prefixes() {
        let mut seen = std::collections::HashSet::new();
        for n in REGISTRY {
            assert!(seen.insert(n.prefix), "前缀重复登记: {}", n.prefix);
        }
    }

    /// A 类含用户内容：必须 seal + sync（无债登记时）。
    #[test]
    fn a_class_user_content_must_be_sealed_and_synced() {
        for n in REGISTRY {
            if n.class == KvClass::A && n.user_content && n.debt.is_none() {
                assert!(n.sealed, "A 类含用户内容必须 seal: {}", n.prefix);
                assert!(n.sync, "A 类含用户内容应参与同步: {}", n.prefix);
            }
        }
    }

    /// B 类含用户内容：须 seal——未 seal 者必须显式挂 debt（债可审计）。
    /// debt=None 且未 seal 的 user_content B 类即失败。
    #[test]
    fn b_class_user_content_sealed_or_debt() {
        for n in REGISTRY {
            if n.class == KvClass::B && n.user_content && !n.sealed {
                assert!(
                    n.debt.is_some(),
                    "B 类含用户内容未 seal 且未登记债: {}",
                    n.prefix
                );
            }
        }
    }

    #[test]
    fn lookup_matches_three_key_forms() {
        assert_eq!(lookup("note:abc").map(|n| n.prefix), Some("note"));
        assert_eq!(lookup("note{abc}").map(|n| n.prefix), Some("note"));
        assert_eq!(lookup("note").map(|n| n.prefix), Some("note"));
        assert_eq!(
            lookup("palette:recent_commands").map(|n| n.prefix),
            Some("palette")
        );
        assert!(lookup("unknown_ns:x").is_none());
    }

    /// 债登记面快照（修复一处移一处——本测试锁当前 4 处,防静默新增）。
    #[test]
    fn debt_register_snapshot() {
        let debts: Vec<&str> = REGISTRY
            .iter()
            .filter(|n| n.debt.is_some())
            .map(|n| n.prefix)
            .collect();
        assert_eq!(
            debts,
            vec!["event", "cal", "task", "dbview", "bookmark", "theme", "liq"],
            "债面变化须显式更新本快照"
        );
    }
}
