//! WritePath — 唯一写入入口（V26 DK-01W / I2）
//!
//! 桌面与移动端此前是两条互不相通的路：移动端 `NoteDoc`（Loro CRDT）+
//! `notesnap:` 快照 + 事件驱动投影；桌面端裸 JSON + `note:` key + 手动索引。
//! 后果：写模型唯一铁律被违反、TodayView 桌面端永远为空、两端数据无法互相同步。
//!
//! 本模块把写入收敛为单一流程：`load → apply → 原子保存 → 派生 blocks → 发事件`。
//!
//! ## Key 与序列化契约（两端一致，R-02/DK-01W）
//!
//! - `note:{id}` — 元数据 JSON（`NoteRecord`）。桌面端经 `SealPair` 加密封装，
//!   移动端暂为明文（V26 加密统一为后续卡）；同步层工作在 Loro oplog 层，
//!   不受本机落盘封装影响。
//! - `notesnap:{user_id}:{id}` — Loro 快照字节（内容权威，五容器模型）。
//!   DK-47 per-user 段（key 构造/惰性迁移统一走 [`crate::app_core`] helper）；
//!   历史无段 `notesnap:{id}` 由读取侧惰性搬迁。
//!
//! ## 写入顺序（WAL 思想）
//!
//! 先写快照后写元数据：快照写失败时元数据未动（旧版本仍自洽）；
//! 元数据是指向最新快照的"权威指针"，最后落盘。
//!
//! ## 投影事件驱动
//!
//! 写入后发 [`crate::event_bus::layered::AppEvent`]（Medium 通道持久化），
//! 投影（搜索/双链/任务）由 `AppCore::catch_up_projections` 事件驱动更新，
//! **禁止**调用方手动触发索引。

use std::collections::HashSet;

use crate::app_core::{self, AppCore};
use crate::blocks::BlockStore;
use crate::error_codes::ErrorCode;
// KVStore 仅 loro-crdt 面消费（updatelog 系全 cfg 门）——cfg 门复归
// （b66b1a2 误判移除：follow_links_on_rename 经 WriteContext.core 具体
// 类型间接消费 KV，不引 trait use——default 面 unused 实证，S1 前缀闸
// CI 兜底收口）。
#[cfg(feature = "loro-crdt")]
use crate::traits::kv_store::KVStore;
use crate::Error;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

// ===== B-04 拆分（纯搬移;API 路径零变——pub use 再导出,消费端零 diff） =====
pub mod attach;
pub mod blocks;
pub mod daily;
pub mod note;
pub mod tree;

pub use attach::*;
pub use blocks::*;
pub use daily::*;
pub use note::*;
pub use tree::*;

#[cfg(feature = "loro-crdt")]
use crate::l1_infrastructure::note_doc::NoteDoc;

/// 写入回执 — 投影水位线推进依据（V26 CoreAPI `WriteReceipt` 镜像）。
#[derive(Debug, Clone, Serialize, Deserialize)]

pub struct WriteReceipt {
    /// 本次写入产生的事件序号。
    pub seq: u64,
    /// 受影响聚合 ID（笔记 ID）。
    pub aggregate_id: String,
    /// 服务端毫秒时间戳。
    pub committed_at: i64,
}

/// 加密封装对（seal/unseal）— 桌面端注入 LocalDekVault 实现，移动端 None（明文）。
///
/// Owned 设计（'static boxed 闭包 + Arc）：调用方 move 捕获，WriteContext
/// 不借用任何栈上局部 — 避免 async 状态机自借用（E0597）。
/// 字节封装闭包（Boxed 'static — owned ctx 消除 async 自借用）。
pub type SealFnBox = Box<dyn Fn(&[u8]) -> Result<Vec<u8>, Error> + Send + Sync>;

pub struct SealPair {
    pub seal: SealFnBox,
    pub unseal: SealFnBox,
}

/// 写入上下文 — 唯一写入入口的依赖集合（AppCore + blocks 双轨 + 加密封装）。
pub struct WriteContext {
    pub core: std::sync::Arc<AppCore>,
    /// blocks 双轨存储（None = 内存降级模式，跳过块派生 — 与移动端降级语义一致）。
    pub blocks: Option<std::sync::Arc<BlockStore>>,
    /// 落盘封装（桌面 Some(vault pair)，移动 None）。
    pub seal: Option<SealPair>,
    /// 内容级加密（DK-07 S2：桌面 Some，移动 None）。
    pub content_cipher: Option<std::sync::Arc<ContentCipherPair>>,
    /// 附件存储（DK-09：桌面 Some(KvAttachmentStore)，移动/测试可 None——
    /// attach_to_note/read_attachment fail-closed）。
    pub attachments: Option<std::sync::Arc<dyn crate::attachment_store::AttachmentStore>>,
}

/// S2 内容加密裁决：加密笔记必须有 cipher（fail-closed），明文直通。
/// 返回写入 record.content 的最终字符串（明文或 enc1: 密文）。
fn seal_content(
    ctx: &WriteContext,
    note_id: &str,
    content: &str,
    level: &str,
) -> Result<String, Error> {
    if level != ENC_AES256GCM {
        return Ok(content.to_string());
    }
    let cipher = ctx.content_cipher.as_ref().ok_or_else(|| {
        Error::Crypto(format!(
            "note '{note_id}' is encrypted but no content cipher available (locked?)"
        ))
    })?;
    (cipher.encrypt)(note_id, content)
}

/// 笔记级加密级别常量（DK-07 S1）。
pub const ENC_NONE: &str = "none";
pub const ENC_AES256GCM: &str = "aes256gcm";

/// serde 默认值函数 — 存量 KV JSON（无 encryption 字段）读回 "none"。
fn default_encryption() -> String {
    ENC_NONE.to_string()
}

/// 内容级加解密函数类型（(note_id, 内容) → 结果）。
pub type ContentCipherFn = Box<dyn Fn(&str, &str) -> Result<String, Error> + Send + Sync>;

/// 笔记内容级加密对（DK-07 S2）— 由装配层注入（桌面 = vault HKDF 实现）。
/// 与 [`SealPair`]（at-rest 整体封装）正交：本对保护**字段级**正文，
/// 锁定态下即使 unseal 读出 JSON，content 仍是密文。
pub struct ContentCipherPair {
    /// (note_id, plaintext) → 密文（enc1: 格式）
    pub encrypt: ContentCipherFn,
    /// (note_id, 密文) → 明文（fail-closed：任何异常都不返回内容）
    pub decrypt: ContentCipherFn,
}

/// 笔记元数据记录（两端统一格式 — 从 mobile-ffi 上移，权威定义于此）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteRecord {
    pub id: String,
    pub title: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
    /// 笔记级加密级别（DK-07 S1）— "none" | "aes256gcm"。
    /// 自定义 default 保证存量 KV JSON（无此字段）反序列化兼容。
    #[serde(default = "default_encryption")]
    pub encryption: String,
    /// DK-02 S2 目录树：父节点（None = 根下）。存量数据 default 兼容。
    #[serde(default)]
    pub parent_id: Option<String>,
    /// DK-02 S2：节点类型（Note | Folder，default Note——旧数据全是笔记）。
    #[serde(default)]
    pub kind: NoteKind,
    /// DK-02 S2：同父下排序键（default 0——旧数据全排根前）。
    #[serde(default)]
    pub sort_order: i64,
    /// DK-02 S3：智能文件夹求值规则（kind=SmartFolder 时有效；None=非智能文件夹）。
    /// serde default 兼容存量数据（批复补点 1）。
    #[serde(default)]
    pub rule: Option<FilterRule>,
    /// DK-19：每日笔记归属日期（kind=DailyNote 时有效；YYYY-MM-DD）。
    #[serde(default)]
    pub daily_date: Option<String>,
    /// DK-27：标签集（轻量读取源——与 Loro doc meta.tags 同步双写；
    /// serde default 兼容存量数据，存量笔记首次标签变更前投影态为空集——诚实化注记）。
    #[serde(default)]
    pub tags: Vec<String>,
}

/// DK-02 S3：智能文件夹过滤规则（各条件 AND；缺省条件忽略）。
///
/// v1 条件面：title_contains（子串）。tags 条件待 tags 投影/索引就绪后扩展
/// （tags 存于 Loro doc meta，evaluate 路径无轻量读取源——诚实化挂起）。
/// 结构化过滤不掺向量路（批复补点 2——检索面复用 SearchOptions 既有 filter）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct FilterRule {
    /// 标题子串（大小写不敏感）。
    #[serde(default)]
    pub title_contains: Option<String>,
    /// DK-27 v2：tags 包含条件——note 必须含有全部列出的标签（AND 语义；空 = 忽略）。
    #[serde(default)]
    pub tags_include: Vec<String>,
    /// DK-27 v2：tags 排除条件——note 含有任一列出标签即不匹配（空 = 忽略）。
    #[serde(default)]
    pub tags_exclude: Vec<String>,
}

impl FilterRule {
    /// 是否匹配（AND 语义；全部条件缺省 = 匹配全部）。
    /// v1 兼容入口：无 tags 条件语义（等价空标签集）。
    pub fn matches(&self, title: &str) -> bool {
        self.matches_full(title, &[])
    }

    /// v2 全量求值：标题 + tags（include 全含 AND / exclude 任一含即否决）。
    pub fn matches_full(&self, title: &str, tags: &[String]) -> bool {
        if let Some(needle) = &self.title_contains {
            if !title.to_lowercase().contains(&needle.to_lowercase()) {
                return false;
            }
        }
        for want in &self.tags_include {
            if !tags.iter().any(|t| t == want) {
                return false;
            }
        }
        if self
            .tags_exclude
            .iter()
            .any(|bad| tags.iter().any(|t| t == bad))
        {
            return false;
        }
        true
    }
}

/// DK-02 S2：树节点类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum NoteKind {
    #[default]
    Note,
    Folder,
    /// DK-02 S3：智能文件夹（动态视图——rule 求值，非成员制）。
    SmartFolder,
    /// DK-19：每日笔记（幂等自动创建，daily_date 归属日期）。
    DailyNote,
}

impl NoteRecord {
    fn new(id: String, title: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id,
            title,
            content: String::new(),
            created_at: now.clone(),
            updated_at: now,
            encryption: ENC_NONE.to_string(),
            parent_id: None,
            kind: NoteKind::Note,
            sort_order: 0,
            rule: None,
            daily_date: None,
            tags: Vec::new(),
        }
    }
}

// ===== 密封读写辅助 =====

async fn put_note_meta(
    core: &AppCore,
    note_id: &str,
    record: &NoteRecord,
    seal: Option<&SealPair>,
) -> Result<(), Error> {
    let payload = serde_json::to_vec(record)?;
    let bytes = match seal {
        Some(pair) => (pair.seal)(&payload)?,
        None => payload,
    };
    core.kv_store.set(&format!("note:{note_id}"), &bytes).await
}

/// DK-40a: 混合存储 compaction 阈值——update log 达此条数后快照回写+清空。
#[cfg(feature = "loro-crdt")]
const UPDATE_LOG_COMPACT_THRESHOLD: usize = 500;

/// DK-40a 修复：补齐同组 cfg——update_log_key 仅 update log 系（loro-crdt）调用；
/// 漏挂导致 iroh-transport 面下 dead_code（-D warnings → error，run 37564438794）。
#[cfg(feature = "loro-crdt")]
fn update_log_key(note_id: &str, seq: usize) -> String {
    format!("updatelog:{note_id}:{seq:020}")
}

/// P2 修复：已 append 水位 vv 键。独立前缀 `updatelogvv:` ——不可挂
/// `updatelog:{id}:` 下（scan_prefix/count_update_log 按条目计数，混入即错位）。
#[cfg(feature = "loro-crdt")]
fn update_vv_key(note_id: &str) -> String {
    format!("updatelogvv:{note_id}")
}

/// DK-40a: compaction —— 快照回写（调用方已持最新 doc）+ log 清空 + 水位对齐。
#[cfg(feature = "loro-crdt")]
async fn compact_update_log(
    kv: &dyn KVStore,
    user_id: &str,
    note_id: &str,
    doc: &NoteDoc,
) -> Result<(), Error> {
    let snap = doc.export_snapshot()?;
    kv.set(&app_core::notesnap_key(user_id, note_id), &snap)
        .await?;
    let pairs = kv.scan_prefix(&format!("updatelog:{note_id}:")).await?;
    for (k, _) in pairs {
        kv.delete(&k).await?;
    }
    // P2 修复：水位对齐快照态（快照=当前 doc，下轮 base 自此起算；函数
    // 自洽——被 persist 阈值路径调用时水位已是同值，幂等；被测试直调时补齐）
    kv.set(&update_vv_key(note_id), &doc.version_vector().encode())
        .await?;
    Ok(())
}

/// 加载（或创建）笔记的 Loro 文档：`notesnap:{user}:{id}` 快照（DK-47 惰性
/// 迁移读——legacy 无段 key 命中即搬迁）+ update log 重放
/// （DK-40a 混合存储打开链——快照低频 + 增量高频，重放后即为最新态）。
#[cfg(feature = "loro-crdt")]
async fn load_or_init_doc(
    kv: &dyn KVStore,
    user_id: &str,
    note_id: &str,
) -> Result<NoteDoc, Error> {
    match app_core::get_notesnap_with_migration(kv, user_id, note_id).await? {
        Some(bytes) if !bytes.is_empty() => {
            let doc = NoteDoc::from_snapshot(&bytes)?;
            for u in read_update_log(kv, note_id).await? {
                doc.apply_update(&u)?;
            }
            Ok(doc)
        }
        _ => {
            // 无快照（新建或迁移中）：空文档 + 重放 log（首 persist 的
            // 初始化增量已在 log——DK-40a 修复：原实现漏重放致首开丢数据）
            let doc = NoteDoc::new("", "")?;
            for u in read_update_log(kv, note_id).await? {
                doc.apply_update(&u)?;
            }
            Ok(doc)
        }
    }
}

/// DK-40a 混合存储写路径：增量追加 update log；
/// log 达 `UPDATE_LOG_COMPACT_THRESHOLD` 后全量快照回写+log 清空
/// （写放大消除——快照低频 + 增量高频，38b 传输单元即 log 内 update）。
///
/// P2 修复（alpha-P2修复设计卡.md 方案 A 水位键）：base_vv 改取**已 append
/// 水位**（`updatelogvv:{id}`）而非旧快照态——旧实现快照仅 compaction 回写，
/// 每轮 `export_update_since(快照 vv)` 重导出全部历史 delta → 条目 bytes O(n²)
/// 膨胀。存量 note 无水位键 → 回落旧快照 vv（首次 append 后水位落地，旧 log
/// 重复内容随下次 compaction 渐进清空）。append 后写水位前崩溃 → 下轮重导出
/// 上一轮 delta，loro apply 幂等自愈（较旧实现每轮全量重复的本质改善）。
#[cfg(feature = "loro-crdt")]
async fn persist_doc(
    kv: &dyn KVStore,
    user_id: &str,
    note_id: &str,
    doc: &NoteDoc,
) -> Result<(), Error> {
    // DK-47：迁移读——legacy 无段 key 有存量时搬迁过来（防存量快照被
    // 「首次全量初始化」分支覆盖丢失——旧数据零丢失裁决）
    let old_snap = app_core::get_notesnap_with_migration(kv, user_id, note_id).await?;
    // 无旧快照（首次持久化）→ 直接全量快照初始化（与旧版语义一致——
    // notesnap 从首写即存在，软删/恢复数据源语义不变）；水位=当前态 vv
    if old_snap.as_ref().map(|b| b.is_empty()).unwrap_or(true) {
        let snapshot = doc.export_snapshot()?;
        kv.set(&app_core::notesnap_key(user_id, note_id), &snapshot)
            .await?;
        kv.set(&update_vv_key(note_id), &doc.version_vector().encode())
            .await?;
        return Ok(());
    }
    // P2 修复：base_vv = 已 append 水位（精确单轮增量）；无水位回落旧快照 vv
    let base_vv = match kv.get(&update_vv_key(note_id)).await? {
        Some(bytes) if !bytes.is_empty() => loro::VersionVector::decode(&bytes)
            .map_err(|e| Error::Internal(format!("update vv decode failed: {e}")))?,
        _ => NoteDoc::from_snapshot(old_snap.as_ref().unwrap())?.version_vector(),
    };
    let update = doc.export_update_since(&base_vv)?;
    append_update_log(kv, note_id, &update).await?;
    // 水位推进（本轮末态）
    kv.set(&update_vv_key(note_id), &doc.version_vector().encode())
        .await?;

    // compaction：log 达阈值 → 全量快照回写 + log 清空（行为不变性由测试锚定）
    if count_update_log(kv, note_id).await >= UPDATE_LOG_COMPACT_THRESHOLD {
        compact_update_log(kv, user_id, note_id, doc).await?;
    }
    Ok(())
}

/// 回收站标记条目（DK-02 S1 — `trash:{note_id}` 的值结构）。
///
/// `title` 为删除时的元数据快照（`Alpha 裁决：入快照明文`——回收站为本地面，
/// 列表展示零解密开销；加密笔记的 title 在 `NoteRecord` 层本就明文，仅正文密文）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrashedNote {
    pub note_id: String,
    pub deleted_at_ms: i64,
    pub title: String,
    /// DK-02 S2：删除时路径快照（沿 parent 链上溯的 Folder title，"/" 连接）。
    /// S1 旧标记无此字段 → None（restore 兜底挂根，serde default 兼容）。
    #[serde(default)]
    pub origin_path: Option<String>,
}

fn trash_key(note_id: &str) -> String {
    format!("trash:{note_id}")
}

// ===== DK-02 S2：目录树 CRUD 与原位还原 =====

/// seal 形态统一解码：note:{id} 落库可能是 vault 密文（desktop）或明文（mobile/
/// 测试）——所有树面 NoteRecord 消费者必须经此函数（S2 教训：裸 from_slice 在
/// seal 形态下静默滤空，list_tree 会产出空树）。
pub(crate) fn decode_record(bytes: &[u8], unseal: Option<&SealPair>) -> Option<NoteRecord> {
    match unseal {
        Some(seal) => {
            let plain = (seal.unseal)(bytes).ok()?;
            serde_json::from_slice(&plain).ok()
        }
        None => serde_json::from_slice(bytes).ok(),
    }
}

/// 目录树扁平节点（UI 端组装——虚拟滚动友好，Alpha 改判口径）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TreeNode {
    pub note_id: String,
    pub kind: NoteKind,
    pub title: String,
    pub parent_id: Option<String>,
    pub sort_order: i64,
}

/// 子树递归收集（BFS，visited 环防护——历史脏数据兜底）。
async fn collect_subtree(
    core: &AppCore,
    root: &str,
    unseal: Option<&SealPair>,
) -> Result<Vec<String>, Error> {
    let mut out = Vec::new();
    let mut queue = std::collections::VecDeque::from([root.to_string()]);
    let mut visited: HashSet<String> = HashSet::new();
    while let Some(cur) = queue.pop_front() {
        if !visited.insert(cur.clone()) {
            continue;
        }
        out.push(cur.clone());
        for (k, bytes) in core.kv_store.scan_prefix("note:").await? {
            let Some(id) = k.strip_prefix("note:") else {
                continue;
            };
            if visited.contains(id) {
                continue;
            }
            if let Some(rec) = decode_record(&bytes, unseal) {
                if rec.parent_id.as_deref() == Some(cur.as_str()) {
                    queue.push_back(id.to_string());
                }
            }
        }
    }
    Ok(out)
}

// ===== DK-19：每日笔记（幂等自动创建——去重键=日期，进程内互斥+索引兜底）=====

/// 每日笔记默认模板。
pub const DAILY_TEMPLATE_KEY: &str = "daily_template";
pub const DEFAULT_DAILY_TEMPLATE: &str = "# {{date}} {{weekday}}\n\n";
/// 设置键：auto（默认，自动创建）/ manual（仅定位，不创建）/ off（功能关闭）。
pub const DAILY_MODE_KEY: &str = "daily_note_mode";

/// 每日笔记模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum DailyNoteMode {
    /// 启动/访问当日视图时自动创建（默认开）。
    #[default]
    Auto,
    /// 仅定位当日笔记，不自动创建（「手动才创建」）。
    Manual,
    /// 功能关闭（返回错误，UI 隐藏入口）。
    Off,
}

impl DailyNoteMode {
    fn from_str(s: &str) -> Self {
        match s {
            "manual" => Self::Manual,
            "off" => Self::Off,
            _ => Self::Auto,
        }
    }
}

/// ensure 结果：note_id + 本次是否新建。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DailyNoteRef {
    pub note_id: String,
    pub created: bool,
}

/// 每日笔记日期→ID 索引键。
fn daily_key(date: &str) -> String {
    format!("daily:{date}")
}

// ============================================================================
// DK-40a 测试 — 混合存储（快照低频 + update log 高频）与零丢失断言
// ============================================================================
#[cfg(all(test, feature = "loro-crdt"))]
mod dk40a_tests {
    use super::*;
    use crate::l1_infrastructure::note_doc::{BlockNode, NoteDoc};
    use crate::l1_infrastructure::storage_engine::MemoryKVStore;
    use std::sync::Arc;

    fn kv() -> Arc<dyn KVStore> {
        Arc::new(MemoryKVStore::default())
    }

    /// DoD 1 核心断言（用户裁决）：双端并发编辑——A 插块+改文本、
    /// B 删块+改同块文本 → 交换增量合并后两侧增量全保留、零丢失。
    #[test]
    fn dk40a_concurrent_edits_zero_loss() {
        // 基线：含块 blk-base（双端共享起点）
        let base = NoteDoc::new("基线", "ws-test").unwrap();
        base.add_block(
            None,
            &BlockNode {
                block_id: "blk-base".into(),
                block_type: "paragraph".into(),
                attrs: Default::default(),
                content_ref: String::new(),
            },
            900,
        )
        .unwrap();
        base.insert_body(0, "base", 901).unwrap();

        let a = base.fork();
        let b = base.fork();

        // A：新增块 blk-a + 正文追加 "A-contrib"
        a.add_block(
            None,
            &BlockNode {
                block_id: "blk-a".into(),
                block_type: "paragraph".into(),
                attrs: Default::default(),
                content_ref: String::new(),
            },
            1_000,
        )
        .unwrap();
        a.insert_body(a.body_len(), "A-contrib", 1_001).unwrap();

        // B：删除基线块 blk-base（定位 TreeID）+ 正文头部插入 "B-contrib"
        let (tid, _) = b
            .blocks()
            .into_iter()
            .find(|(_, _, n)| n.block_id == "blk-base")
            .map(|(_, t, n)| (t, n.block_id))
            .expect("基线应有 blk-base");
        b.remove_block(tid, 2_000).unwrap();
        b.insert_body(0, "B-contrib", 2_001).unwrap();

        // 交换增量（CRDT 合并）
        let ua = a
            .export_update_since(&loro::VersionVector::default())
            .unwrap();
        let ub = b
            .export_update_since(&loro::VersionVector::default())
            .unwrap();
        a.apply_update(&ub).unwrap();
        b.apply_update(&ua).unwrap();

        // 块级：A 新增块在（双端）；字符级：双方插入均保留
        for d in [&a, &b] {
            let ids: Vec<String> = d
                .blocks()
                .iter()
                .map(|(_, _, n)| n.block_id.clone())
                .collect();
            assert!(ids.contains(&"blk-a".to_string()), "A 新增块零丢失");
            let body = d.body();
            assert!(body.contains("A-contrib"), "A 字符插入零丢失");
            assert!(body.contains("B-contrib"), "B 字符插入零丢失");
        }
    }

    /// DoD 2/3: 快照+log 重放等价 + compaction（阈值回写清 log）后行为不变。
    #[tokio::test]
    async fn dk40a_log_replay_and_compaction() {
        let kv = kv();
        let note_id = "dk40a-note";
        let user = app_core::DEFAULT_USER_ID;
        let doc = NoteDoc::new("标题", "ws-test").unwrap();
        doc.insert_body(0, "v0", 1_000).unwrap();
        persist_doc(kv.as_ref(), user, note_id, &doc).await.unwrap();

        // 连续 3 轮编辑（每轮 persist：增量 append log；无 compaction——<阈值）
        for i in 1..=3 {
            doc.insert_body(doc.body_len(), &format!("v{i}"), 1_000 + i)
                .unwrap();
            persist_doc(kv.as_ref(), user, note_id, &doc).await.unwrap();
        }
        assert_eq!(
            count_update_log(kv.as_ref(), note_id).await,
            3,
            "3 轮编辑增量（首轮走全量快照初始化不入 log）"
        );

        // 重放等价：全新打开（快照+log 重放）== 连续编辑态
        let reopened = load_or_init_doc(kv.as_ref(), user, note_id).await.unwrap();
        assert_eq!(reopened.body(), doc.body(), "快照+log 重放 == 连续编辑态");

        // compaction：模拟达阈值（阈值常量）——直接 compact 后行为不变 + log 清空
        compact_update_log(kv.as_ref(), user, note_id, &doc)
            .await
            .unwrap();
        assert_eq!(count_update_log(kv.as_ref(), note_id).await, 0);
        let after_compact = load_or_init_doc(kv.as_ref(), user, note_id).await.unwrap();
        assert_eq!(
            after_compact.body(),
            doc.body(),
            "compaction 前后文档态一致"
        );
    }

    /// DoD 4: 存量 note 首次打开自动初始化（无快照 → 空文档灌入后 persist 落快照）。
    #[tokio::test]
    async fn dk40a_legacy_first_open_initializes() {
        let kv = kv();
        let note_id = "legacy-note";
        let user = app_core::DEFAULT_USER_ID;
        // 无 notesnap：首开 → 空文档
        let doc = load_or_init_doc(kv.as_ref(), user, note_id).await.unwrap();
        doc.insert_body(0, "seed", 1_000).unwrap();
        persist_doc(kv.as_ref(), user, note_id, &doc).await.unwrap();
        // 二次打开：快照已落（自动初始化闭环）
        let reopened = load_or_init_doc(kv.as_ref(), user, note_id).await.unwrap();
        assert_eq!(reopened.body(), "seed");
    }

    /// P2 修复回归（alpha-P2修复设计卡.md 方案 A）：每轮 log 条目==该轮精确
    /// 增量（与 persist 前单独 export 的单轮 delta 逐字节相等）——旧实现以旧
    /// 快照 vv 为 base，重导出全部历史 delta，在此断言下逐轮失败。
    #[tokio::test]
    async fn dk40a_log_exact_delta_per_round() {
        let kv = kv();
        let note_id = "exact-delta";
        let doc = NoteDoc::new("t", "ws").unwrap();
        doc.insert_body(0, "v0", 1).unwrap();
        persist_doc(kv.as_ref(), app_core::DEFAULT_USER_ID, note_id, &doc)
            .await
            .unwrap();
        let mut prev_vv = doc.version_vector();

        for i in 1..=20usize {
            doc.insert_body(doc.body_len(), &format!("edit-{i:02}"), (i * 100) as i64)
                .unwrap();
            let expect = doc.export_update_since(&prev_vv).unwrap();
            persist_doc(kv.as_ref(), app_core::DEFAULT_USER_ID, note_id, &doc)
                .await
                .unwrap();
            let last = read_update_log(kv.as_ref(), note_id)
                .await
                .unwrap()
                .pop()
                .unwrap();
            assert_eq!(
                last, expect,
                "第{i}轮 log 条目须为单轮精确增量（旧实现重导出历史在此失败）"
            );
            prev_vv = doc.version_vector();
        }
        // 条目数=编辑轮数（首 persist 为全量快照不入 log）+ 水位键落地且=末态 vv
        assert_eq!(count_update_log(kv.as_ref(), note_id).await, 20);
        let stored = kv.get(&update_vv_key(note_id)).await.unwrap().unwrap();
        assert_eq!(
            loro::VersionVector::decode(&stored).unwrap(),
            doc.version_vector(),
            "水位=末态 vv"
        );
        // 重放等价回归：打开链 == 连续编辑态（P2 改动不得破坏 DoD2）
        let reopened = load_or_init_doc(kv.as_ref(), app_core::DEFAULT_USER_ID, note_id)
            .await
            .unwrap();
        assert_eq!(reopened.body(), doc.body());
    }

    /// P2 修复：存量 note（快照+log、无水位键）首次 persist 回落旧快照 vv——
    /// append 正常 + 水位键落地（旧 log 重复内容随下次 compaction 渐进清空）。
    #[tokio::test]
    async fn dk40a_legacy_no_watermark_fallback() {
        let kv = kv();
        let note_id = "legacy-no-wm";
        let doc = NoteDoc::new("t", "ws").unwrap();
        doc.insert_body(0, "v0", 1).unwrap();
        persist_doc(kv.as_ref(), app_core::DEFAULT_USER_ID, note_id, &doc)
            .await
            .unwrap();
        // 模拟存量旧态：删水位键（旧版本无此键）
        kv.delete(&update_vv_key(note_id)).await.unwrap();
        assert!(kv.get(&update_vv_key(note_id)).await.unwrap().is_none());

        doc.insert_body(doc.body_len(), "legacy-edit", 2_000)
            .unwrap();
        persist_doc(kv.as_ref(), app_core::DEFAULT_USER_ID, note_id, &doc)
            .await
            .unwrap();
        // 回落路径：append 正常 + 水位键重新落地
        assert_eq!(count_update_log(kv.as_ref(), note_id).await, 1);
        let stored = kv.get(&update_vv_key(note_id)).await.unwrap().unwrap();
        assert_eq!(
            loro::VersionVector::decode(&stored).unwrap(),
            doc.version_vector()
        );
        // 重放等价：回落路径不破坏打开链
        let reopened = load_or_init_doc(kv.as_ref(), app_core::DEFAULT_USER_ID, note_id)
            .await
            .unwrap();
        assert!(reopened.body().contains("legacy-edit"));
    }
}

// ============================================================================
// DK-02 S1 回收站补测（Leo 对向复核产出——Alpha 回 receipt 虚报清债:
// 原「dk02_s1_trash.rs 352 行」全仓不存在,cargo test dk02=0 matched 实锤）。
// 纯 KV 面: list 排序/坏 marker 降级/is_trashed/purge_expired 窗口+
// 物理键失批次不炸。restore 全链（NoteCreated 重放需 loro doc）挂
// 桌面 e2e 批。TestStack Noop 桩抄 aurora-ai liquify TestStack（同源复用）。
// ============================================================================
#[cfg(test)]
mod dk02_trash_tests {
    use super::{is_trashed, list_trashed, purge_expired, TrashedNote, WriteContext};
    use crate::app_core::{AppCore, AppCoreBuilder};
    use crate::l1_infrastructure::storage_engine::MemoryKVStore;
    use crate::traits::kv_store::KVStore;
    use std::sync::Arc;

    // === TestStack：AppCore 全依赖空转 mock（liquify 只碰 kv+write_path； ===
    // === 后续 DK-10 Agent 会话片测试可复用/提炼）===

    struct NoopAi;
    #[async_trait::async_trait]
    impl crate::traits::ai_provider::AIProvider for NoopAi {
        async fn embed(&self, _t: &[&str]) -> Result<Vec<Vec<f32>>, crate::Error> {
            Ok(vec![])
        }
        async fn complete(
            &self,
            _p: &str,
            _o: &crate::traits::ai_provider::CompletionOptions,
        ) -> Result<String, crate::Error> {
            Ok(String::new())
        }
        fn stream_complete(
            &self,
            _p: &str,
            _o: &crate::traits::ai_provider::CompletionOptions,
            _c: Box<dyn Fn(String) + Send + Sync>,
        ) {
        }
        async fn chat(
            &self,
            _m: &[crate::traits::ai_provider::Message],
            _o: &crate::traits::ai_provider::ChatOptions,
        ) -> Result<String, crate::Error> {
            Ok(String::new())
        }
        async fn function_call(
            &self,
            _p: &str,
            _t: &[crate::traits::ai_provider::Tool],
        ) -> Result<crate::traits::ai_provider::ToolCall, crate::Error> {
            Err(crate::Error::Internal("noop ai".into()))
        }
        fn is_available(&self) -> bool {
            false
        }
    }

    struct NoopSyncTarget;
    #[async_trait::async_trait]
    impl crate::traits::sync_target::SyncTarget for NoopSyncTarget {
        async fn connect(
            &mut self,
            _: &crate::traits::sync_target::Endpoint,
        ) -> Result<crate::traits::sync_target::Connection, crate::Error> {
            unimplemented!("liquify 测试不触同步")
        }
        async fn sync(
            &self,
            _: &crate::traits::sync_target::Connection,
            _: &crate::traits::sync_target::DocSet,
        ) -> Result<crate::traits::sync_target::SyncReport, crate::Error> {
            unimplemented!("liquify 测试不触同步")
        }
        async fn send_update(
            &self,
            _: &crate::traits::sync_target::Connection,
            _: &crate::traits::sync_target::UpdatePayload,
        ) -> Result<(), crate::Error> {
            unimplemented!("liquify 测试不触同步")
        }
        async fn recv_update(
            &self,
            _: &crate::traits::sync_target::Connection,
            _: &str,
        ) -> Result<Vec<u8>, crate::Error> {
            unimplemented!("liquify 测试不触同步")
        }
        async fn sync_version(
            &self,
            _: &crate::traits::sync_target::Connection,
            _: &str,
        ) -> Result<Option<u64>, crate::Error> {
            unimplemented!("liquify 测试不触同步")
        }
        fn watch(&self, _: Box<dyn Fn(crate::traits::sync_target::SyncEvent) + Send + Sync>) {}
        async fn disconnect(
            &self,
            _: &crate::traits::sync_target::Connection,
        ) -> Result<(), crate::Error> {
            unimplemented!("liquify 测试不触同步")
        }
    }

    struct NoopCrypto;
    impl crate::traits::crypto_provider::CryptoProvider for NoopCrypto {
        fn encrypt(
            &self,
            _: &[u8],
            _: &[u8; 32],
        ) -> Result<crate::traits::crypto_provider::Ciphertext, crate::Error> {
            unimplemented!("liquify 测试不触加密")
        }
        fn decrypt(
            &self,
            _: &crate::traits::crypto_provider::Ciphertext,
            _: &[u8; 32],
        ) -> Result<Vec<u8>, crate::Error> {
            unimplemented!("liquify 测试不触加密")
        }
        fn derive_key(&self, _: &str, _: &[u8]) -> Result<[u8; 32], crate::Error> {
            unimplemented!("liquify 测试不触加密")
        }
        fn kem_keypair(
            &self,
        ) -> Result<
            (
                crate::traits::crypto_provider::KemPublicKey,
                crate::traits::crypto_provider::KemSecretKey,
            ),
            crate::Error,
        > {
            unimplemented!("liquify 测试不触加密")
        }
        fn kem_encapsulate(
            &self,
            _: &crate::traits::crypto_provider::KemPublicKey,
        ) -> Result<
            (
                crate::traits::crypto_provider::KemSharedSecret,
                crate::traits::crypto_provider::KemCiphertext,
            ),
            crate::Error,
        > {
            unimplemented!("liquify 测试不触加密")
        }
        fn kem_decapsulate(
            &self,
            _: &crate::traits::crypto_provider::KemSecretKey,
            _: &crate::traits::crypto_provider::KemCiphertext,
        ) -> Result<crate::traits::crypto_provider::KemSharedSecret, crate::Error> {
            unimplemented!("liquify 测试不触加密")
        }
        fn random_bytes(&self, _: usize) -> Vec<u8> {
            unimplemented!("liquify 测试不触加密")
        }
        fn hash(&self, _: &[u8]) -> [u8; 32] {
            unimplemented!("liquify 测试不触加密")
        }
        fn hmac_sign(&self, _: &[u8], _: &[u8]) -> Vec<u8> {
            unimplemented!("liquify 测试不触加密")
        }
        fn hmac_verify(&self, _: &[u8], _: &[u8], _: &[u8]) -> bool {
            unimplemented!("liquify 测试不触加密")
        }
        fn ed25519_verify(
            &self,
            _: &crate::traits::crypto_provider::Ed25519PublicKey,
            _: &[u8],
            _: &crate::traits::crypto_provider::Ed25519Signature,
        ) -> bool {
            unimplemented!("liquify 测试不触加密")
        }
        fn algorithm_version(&self) -> u16 {
            unimplemented!("liquify 测试不触加密")
        }
    }

    struct NoopSearch;
    #[async_trait::async_trait]
    impl crate::traits::search_backend::SearchBackend for NoopSearch {
        async fn search(
            &self,
            _: &str,
            _: &crate::traits::search_backend::SearchOptions,
        ) -> Result<crate::traits::search_backend::SearchResult, crate::Error> {
            unimplemented!("liquify 测试不触检索")
        }
        async fn index_note(
            &self,
            _: &str,
            _: &str,
            _: &crate::traits::search_backend::NoteMetadata,
        ) -> Result<(), crate::Error> {
            Ok(())
        }
        async fn batch_index(
            &self,
            _: &[crate::traits::search_backend::IndexEntry],
        ) -> Result<(), crate::Error> {
            Ok(())
        }
        async fn remove_index(&self, _: &str) -> Result<(), crate::Error> {
            Ok(())
        }
        async fn rebuild_index(
            &self,
            _: &[crate::traits::search_backend::IndexEntry],
        ) -> Result<(), crate::Error> {
            Ok(())
        }
        fn tokenize(&self, text: &str) -> Vec<String> {
            text.split_whitespace().map(String::from).collect()
        }
    }

    struct NoopOcr;
    #[async_trait::async_trait]
    impl crate::traits::ocr_provider::OcrProvider for NoopOcr {
        async fn recognize(
            &self,
            _: &[u8],
        ) -> Result<crate::traits::ocr_provider::OcrResult, crate::Error> {
            unimplemented!("liquify 测试不触 OCR")
        }
        async fn recognize_batch(
            &self,
            _: &[&[u8]],
        ) -> Result<Vec<crate::traits::ocr_provider::OcrResult>, crate::Error> {
            unimplemented!("liquify 测试不触 OCR")
        }
        fn is_available(&self) -> bool {
            false
        }
    }

    struct NoopPlugin;
    #[async_trait::async_trait]
    impl crate::traits::plugin_runtime::PluginRuntime for NoopPlugin {
        async fn load(
            &mut self,
            _: &crate::traits::plugin_runtime::PluginManifest,
        ) -> Result<crate::traits::plugin_runtime::PluginHandle, crate::Error> {
            unimplemented!("liquify 测试不触插件")
        }
        async fn invoke(
            &self,
            _: &crate::traits::plugin_runtime::PluginHandle,
            _: &str,
            _: &serde_json::Value,
        ) -> Result<serde_json::Value, crate::Error> {
            unimplemented!("liquify 测试不触插件")
        }
        async fn unload(
            &mut self,
            _: &crate::traits::plugin_runtime::PluginHandle,
        ) -> Result<(), crate::Error> {
            unimplemented!("liquify 测试不触插件")
        }
        fn list_hooks(&self, _: &str) -> Vec<crate::traits::plugin_runtime::PluginHandle> {
            vec![]
        }
    }

    fn core(kv: Arc<MemoryKVStore>) -> AppCore {
        AppCoreBuilder::new()
            .sync_target(Arc::new(NoopSyncTarget))
            .crypto(Arc::new(NoopCrypto))
            .ai(Arc::new(NoopAi))
            .kv_store(kv)
            .search(Arc::new(NoopSearch))
            .ocr(Arc::new(NoopOcr))
            .plugin(Arc::new(NoopPlugin))
            .build()
    }
    fn ctx(core: &Arc<AppCore>) -> WriteContext {
        WriteContext {
            core: core.clone(),
            blocks: None,
            seal: None,
            content_cipher: None,
            attachments: None,
        }
    }
    async fn put_marker(kv: &dyn KVStore, note_id: &str, at_ms: i64, title: &str) {
        let m = TrashedNote {
            note_id: note_id.into(),
            deleted_at_ms: at_ms,
            title: title.into(),
            origin_path: None,
        };
        kv.set(
            &format!("trash:{note_id}"),
            &serde_json::to_vec(&m).unwrap(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn list_trashed_desc_and_bad_marker_skipped() {
        let kv = Arc::new(MemoryKVStore::default());
        put_marker(kv.as_ref(), "n-new", 2_000, "新删").await;
        put_marker(kv.as_ref(), "n-old", 1_000, "旧删").await;
        // 烂 marker: 非 JSON——批次降级不炸（warn 跳过惯例,同 list_events）
        kv.set("trash:n-bad", b"{not-json".as_ref()).await.unwrap();

        let out = list_trashed(&core(kv)).await.unwrap();
        assert_eq!(out.len(), 2, "烂 marker 跳过,好 marker 全收");
        assert_eq!(out[0].note_id, "n-new", "按删除时间倒序（新删在前）");
        assert_eq!(out[1].note_id, "n-old");
        assert_eq!(out[1].title, "旧删", "title 快照随 marker 透出");
    }

    #[tokio::test]
    async fn is_trashed_membership() {
        let kv = Arc::new(MemoryKVStore::default());
        put_marker(kv.as_ref(), "n-1", 1, "t").await;
        let c = core(kv.clone());
        assert!(is_trashed(&c, "n-1").await);
        assert!(
            !is_trashed(&c, "n-2").await,
            "无 marker=false（防误伤未删笔记）"
        );
    }

    #[tokio::test]
    async fn purge_expired_window_and_resilient_batch() {
        let kv = Arc::new(MemoryKVStore::default());
        // 基准 now-40 天（cutoff 外）vs now-5 天（窗内不清）
        let now_ms = chrono::Utc::now().timestamp_millis();
        put_marker(kv.as_ref(), "n-expired", now_ms - 40 * 86_400_000, "过期").await;
        put_marker(kv.as_ref(), "n-fresh", now_ms - 5 * 86_400_000, "新鲜").await;
        // 烂 marker 同批（批次不炸面）
        kv.set("trash:n-bad", b"~".as_ref()).await.unwrap();
        // 物理 note 键失（purge_note 物理清理步会失败——单条跳过不阻塞）

        let c = Arc::new(core(kv.clone()));
        let ctx = ctx(&c);
        let purged = purge_expired(&ctx, 30).await.unwrap();
        // purge_note 幂等清场: 物理 note 键失（KV delete 不存在=Ok）——
        // marker+物理双删均成功,purged 含过期条目（实测行为,比预期更强）。
        assert_eq!(purged, vec!["n-expired".to_string()], "仅清 30 天窗外条目");
        assert!(
            is_trashed(&c, "n-fresh").await,
            "窗内 5 天不动（marker 保留）"
        );
        assert!(!is_trashed(&c, "n-expired").await, "过期条目 marker 已清");
    }
}
