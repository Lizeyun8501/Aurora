//! V19 §30.1 五容器笔记文档模型（DEV-004）
//!
//! 每篇笔记对应一个独立的 `LoroDoc`，内容通过 Loro 容器层次化建模：
//!
//! ```text
//! LoroDoc {
//!   map("meta"):         LoroMap { title, workspace_id, tags: LoroList,
//!                                 created_at, updated_at, encryption }
//!   text("body"):        LoroText（mark() 支持富文本样式）
//!   tree("blocks"):      LoroTree（node.meta: block_type/block_id/attrs/content_ref）
//!   movable_list("tasks"): LoroMovableList<LoroMap>（GTD 任务，可重排序）
//!   list("backlinks"):   LoroList<LoroMap>（双链本地缓存，主索引在 SQLite links 表）
//! }
//! ```
//!
//! 同步编码策略（V19 §30.2）：
//! - 实时增量: `export_update_since()` (update, ~1-10KB)
//! - 全量持久: `export_snapshot()` (snapshot, ~50-500KB)
//! - 历史裁剪: `export_shallow()` (shallow-snapshot)

use std::collections::HashMap;

use loro::{ExportMode, LoroDoc, LoroList, LoroMap, LoroText, LoroTree, TreeParentId};

use crate::Error;

// ===========================================================================
// 容器命名（V19 §30.1 固定契约，跨端必须一致）
// ===========================================================================

pub const CONTAINER_META: &str = "meta";
pub const CONTAINER_BODY: &str = "body";
pub const CONTAINER_BLOCKS: &str = "blocks";
pub const CONTAINER_TASKS: &str = "tasks";
pub const CONTAINER_BACKLINKS: &str = "backlinks";

// ===========================================================================
// 数据结构
// ===========================================================================

/// 加密级别（V19 §30.1 meta.encryption）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EncryptionLevel {
    /// 明文存储
    #[default]
    None,
    /// AES-256-GCM 信封加密
    Aes256Gcm,
}

impl EncryptionLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            EncryptionLevel::None => "none",
            EncryptionLevel::Aes256Gcm => "aes-256-gcm",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "aes-256-gcm" => EncryptionLevel::Aes256Gcm,
            _ => EncryptionLevel::None,
        }
    }
}

/// 笔记元数据（map("meta") 的类型化视图）。
#[derive(Debug, Clone, PartialEq)]
pub struct NoteMeta {
    pub title: String,
    pub workspace_id: String,
    pub tags: Vec<String>,
    /// Unix epoch 毫秒
    pub created_at: i64,
    /// Unix epoch 毫秒
    pub updated_at: i64,
    pub encryption: EncryptionLevel,
}

/// 块级结构树节点（tree("blocks") 的节点数据）。
#[derive(Debug, Clone, PartialEq)]
pub struct BlockNode {
    /// 块级 UUID
    pub block_id: String,
    /// "paragraph" | "heading" | "list" | "code" | "quote" | "table"
    pub block_type: String,
    /// 块属性（heading 级别、列表符号等）
    pub attrs: HashMap<String, String>,
    /// 指向 text("body") 中的文本范围
    pub content_ref: String,
}

/// 任务项（movable_list("tasks") 的元素，GTD 集成）。
#[derive(Debug, Clone, PartialEq)]
pub struct NoteTask {
    pub task_id: String,
    pub title: String,
    /// "inbox" | "next" | "waiting" | "scheduled" | "done"
    pub status: String,
    /// "low" | "medium" | "high" | "urgent"
    pub priority: String,
    /// Unix epoch 毫秒
    pub due_date: Option<i64>,
}

impl NoteTask {
    pub const STATUS_INBOX: &'static str = "inbox";
    pub const STATUS_NEXT: &'static str = "next";
    pub const STATUS_WAITING: &'static str = "waiting";
    pub const STATUS_SCHEDULED: &'static str = "scheduled";
    pub const STATUS_DONE: &'static str = "done";

    /// GTD 状态机合法迁移（V19 §13 GTD 集成）。
    pub fn is_valid_transition(from: &str, to: &str) -> bool {
        if from == to {
            return false;
        }
        matches!(
            (from, to),
            (Self::STATUS_INBOX, Self::STATUS_NEXT)
                | (Self::STATUS_INBOX, Self::STATUS_SCHEDULED)
                | (Self::STATUS_INBOX, Self::STATUS_DONE)
                | (Self::STATUS_NEXT, Self::STATUS_WAITING)
                | (Self::STATUS_NEXT, Self::STATUS_DONE)
                | (Self::STATUS_NEXT, Self::STATUS_SCHEDULED)
                | (Self::STATUS_WAITING, Self::STATUS_NEXT)
                | (Self::STATUS_WAITING, Self::STATUS_DONE)
                | (Self::STATUS_SCHEDULED, Self::STATUS_DONE)
                | (Self::STATUS_SCHEDULED, Self::STATUS_NEXT)
                | (Self::STATUS_DONE, Self::STATUS_NEXT)
        )
    }
}

/// 双链引用（list("backlinks") 的元素；主索引在 SQLite links 表）。
#[derive(Debug, Clone, PartialEq)]
pub struct Backlink {
    pub source_note_id: String,
    pub link_text: String,
}

// ===========================================================================
// NoteDoc — 五容器模型门面
// ===========================================================================

/// V19 §30.1 五容器笔记文档。
///
/// 所有变更方法自动 `commit()`；快照/增量导出按 §30.2 编码策略。
#[derive(Debug, Clone)]
pub struct NoteDoc {
    doc: LoroDoc,
}

impl NoteDoc {
    // ------------------------------------------------------------------
    // 生命周期
    // ------------------------------------------------------------------

    /// 创建带初始化 meta 的笔记文档（五容器全部就位）。
    pub fn new(title: &str, workspace_id: &str) -> Result<Self, Error> {
        let doc = LoroDoc::new();
        let note = Self { doc };
        note.init_meta(title, workspace_id)?;
        note.ensure_containers()?;
        Ok(note)
    }

    /// 从已有 LoroDoc 包装（例如从快照恢复后）。
    pub fn from_doc(doc: LoroDoc) -> Self {
        Self { doc }
    }

    /// 从快照恢复（V19 §30.2 全量持久化编码）。
    pub fn from_snapshot(bytes: &[u8]) -> Result<Self, Error> {
        let doc = LoroDoc::new();
        doc.import(bytes)
            .map_err(|e| Error::Internal(format!("loro snapshot import: {e}")))?;
        Ok(Self { doc })
    }

    /// 访问底层 LoroDoc。
    pub fn inner(&self) -> &LoroDoc {
        &self.doc
    }

    /// fork 出独立副本（CRDT 分叉测试/合并场景）。
    pub fn fork(&self) -> Self {
        Self {
            doc: self.doc.fork(),
        }
    }

    // ------------------------------------------------------------------
    // 容器句柄
    // ------------------------------------------------------------------

    fn meta_map(&self) -> LoroMap {
        self.doc.get_map(CONTAINER_META)
    }

    fn body_text(&self) -> LoroText {
        self.doc.get_text(CONTAINER_BODY)
    }

    fn blocks_tree(&self) -> LoroTree {
        self.doc.get_tree(CONTAINER_BLOCKS)
    }

    fn tasks_list(&self) -> loro::LoroMovableList {
        self.doc.get_movable_list(CONTAINER_TASKS)
    }

    fn backlinks_list(&self) -> LoroList {
        self.doc.get_list(CONTAINER_BACKLINKS)
    }

    fn tags_list(&self) -> Result<LoroList, Error> {
        self.meta_map()
            .get_or_create_container("tags", LoroList::new())
            .map_err(|e| Error::Internal(format!("loro tags container: {e}")))
    }

    /// 初始化 meta 字段（幂等：仅当字段缺失时写入）。
    fn init_meta(&self, title: &str, workspace_id: &str) -> Result<(), Error> {
        let meta = self.meta_map();
        if meta.get("title").is_none() {
            meta.insert("title", title).map_err(err("meta.title"))?;
        }
        if meta.get("workspace_id").is_none() {
            meta.insert("workspace_id", workspace_id)
                .map_err(err("meta.workspace_id"))?;
        }
        if meta.get("created_at").is_none() {
            meta.insert("created_at", 0_i64)
                .map_err(err("meta.created_at"))?;
        }
        if meta.get("updated_at").is_none() {
            meta.insert("updated_at", 0_i64)
                .map_err(err("meta.updated_at"))?;
        }
        if meta.get("encryption").is_none() {
            meta.insert("encryption", EncryptionLevel::None.as_str())
                .map_err(err("meta.encryption"))?;
        }
        self.tags_list()?;
        Ok(())
    }

    /// 确保 body/blocks/tasks/backlinks 容器存在（写入空根操作，幂等）。
    fn ensure_containers(&self) -> Result<(), Error> {
        // 各 get_* 会在访问时自动创建关联容器句柄；
        // 但要在文档中"落地"需要至少一次写操作或 commit。
        self.doc.commit();
        Ok(())
    }

    // ------------------------------------------------------------------
    // meta 容器
    // ------------------------------------------------------------------

    /// 读取元数据快照。
    pub fn meta(&self) -> NoteMeta {
        let deep = self.meta_map().get_deep_value();
        let tags = deep
            .get_by_key("tags")
            .and_then(|v| v.as_list())
            .map(|l| {
                l.as_ref()
                    .iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        NoteMeta {
            title: deep_str(&deep, "title").unwrap_or_default(),
            workspace_id: deep_str(&deep, "workspace_id").unwrap_or_default(),
            tags,
            created_at: deep_i64(&deep, "created_at").unwrap_or(0),
            updated_at: deep_i64(&deep, "updated_at").unwrap_or(0),
            encryption: EncryptionLevel::from_str(
                &deep_str(&deep, "encryption").unwrap_or_default(),
            ),
        }
    }

    /// 设置标题并刷新 updated_at。
    pub fn set_title(&self, title: &str, now: i64) -> Result<(), Error> {
        self.meta_map()
            .insert("title", title)
            .map_err(err("meta.title"))?;
        self.touch(now)
    }

    /// 设置 created_at/updated_at（导入或初始化场景）。
    pub fn set_timestamps(&self, created_at: i64, updated_at: i64) -> Result<(), Error> {
        let meta = self.meta_map();
        meta.insert("created_at", created_at)
            .map_err(err("meta.created_at"))?;
        meta.insert("updated_at", updated_at)
            .map_err(err("meta.updated_at"))?;
        self.doc.commit();
        Ok(())
    }

    /// 刷新 updated_at。
    pub fn touch(&self, now: i64) -> Result<(), Error> {
        self.meta_map()
            .insert("updated_at", now)
            .map_err(err("meta.updated_at"))?;
        self.doc.commit();
        Ok(())
    }

    /// 添加标签（去重）。
    pub fn add_tag(&self, tag: &str, now: i64) -> Result<(), Error> {
        let tags = self.tags_list()?;
        let exists = tags
            .to_vec()
            .iter()
            .any(|v| v.as_string().map(|s| sv_eq(s, tag)).unwrap_or(false));
        if !exists {
            tags.push(tag).map_err(err("tags.push"))?;
        }
        self.touch(now)
    }

    /// 移除标签。
    pub fn remove_tag(&self, tag: &str, now: i64) -> Result<(), Error> {
        let tags = self.tags_list()?;
        let items = tags.to_vec();
        for (i, v) in items.iter().enumerate().rev() {
            if v.as_string().map(|s| sv_eq(s, tag)).unwrap_or(false) {
                tags.delete(i, 1).map_err(err("tags.delete"))?;
            }
        }
        self.touch(now)
    }

    /// 设置加密级别（信封加密切换时）。
    pub fn set_encryption(&self, level: EncryptionLevel, now: i64) -> Result<(), Error> {
        self.meta_map()
            .insert("encryption", level.as_str())
            .map_err(err("meta.encryption"))?;
        self.touch(now)
    }

    // ------------------------------------------------------------------
    // body 容器
    // ------------------------------------------------------------------

    /// 读取正文纯文本。
    pub fn body(&self) -> String {
        self.body_text().to_string()
    }

    /// 正文 Unicode 字符数。
    pub fn body_len(&self) -> usize {
        self.body_text().len_unicode()
    }

    /// 整体替换正文（编辑器全量同步路径）。
    pub fn set_body(&self, content: &str, now: i64) -> Result<(), Error> {
        let text = self.body_text();
        let old = text.len_unicode();
        if old > 0 {
            text.delete(0, old).map_err(err("body.delete"))?;
        }
        if !content.is_empty() {
            text.insert(0, content).map_err(err("body.insert"))?;
        }
        self.touch(now)
    }

    /// 在位置 `pos` 插入文本（增量编辑路径）。
    pub fn insert_body(&self, pos: usize, content: &str, now: i64) -> Result<(), Error> {
        self.body_text()
            .insert(pos, content)
            .map_err(err("body.insert"))?;
        self.touch(now)
    }

    /// 删除正文区间 [pos, pos+len)。
    pub fn delete_body(&self, pos: usize, len: usize, now: i64) -> Result<(), Error> {
        self.body_text()
            .delete(pos, len)
            .map_err(err("body.delete"))?;
        self.touch(now)
    }

    // ------------------------------------------------------------------
    // blocks 容器（LoroTree）
    // ------------------------------------------------------------------

    /// 追加块节点；`parent` 为 None 时挂到根。返回新节点 TreeID。
    pub fn add_block(
        &self,
        parent: Option<loro::TreeID>,
        block: &BlockNode,
        now: i64,
    ) -> Result<loro::TreeID, Error> {
        let tree = self.blocks_tree();
        let parent_id: TreeParentId = parent.map(TreeParentId::Node).unwrap_or(TreeParentId::Root);
        let node_id = tree.create(parent_id).map_err(err("blocks.create"))?;
        let meta = tree.get_meta(node_id).map_err(err("blocks.get_meta"))?;
        meta.insert("block_id", block.block_id.as_str())
            .map_err(err("block.block_id"))?;
        meta.insert("block_type", block.block_type.as_str())
            .map_err(err("block.block_type"))?;
        meta.insert("content_ref", block.content_ref.as_str())
            .map_err(err("block.content_ref"))?;
        let attrs: loro::LoroMap = meta
            .insert_container("attrs", LoroMap::new())
            .map_err(err("block.attrs"))?;
        for (k, v) in &block.attrs {
            attrs
                .insert(k.as_str(), v.as_str())
                .map_err(err("block.attrs.insert"))?;
        }
        self.touch(now)?;
        Ok(node_id)
    }

    /// 深度优先遍历块树（含已删除过滤），返回 (depth, TreeID, BlockNode)。
    pub fn blocks(&self) -> Vec<(usize, loro::TreeID, BlockNode)> {
        let tree = self.blocks_tree();
        let mut out = Vec::new();
        fn walk(
            tree: &LoroTree,
            parent: TreeParentId,
            depth: usize,
            out: &mut Vec<(usize, loro::TreeID, BlockNode)>,
        ) {
            // children 返回 Option（None = 父节点不存在/已删除）
            let Some(children) = tree.children(parent) else {
                return;
            };
            for id in children {
                if tree.is_node_deleted(&id).unwrap_or(false) {
                    continue;
                }
                let Ok(meta) = tree.get_meta(id) else {
                    continue;
                };
                let deep = meta.get_deep_value();
                let attrs: HashMap<String, String> = deep
                    .get_by_key("attrs")
                    .and_then(|v| v.as_map())
                    .map(|m| {
                        m.as_ref()
                            .iter()
                            .filter_map(|(k, v)| v.as_string().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    })
                    .unwrap_or_default();
                out.push((
                    depth,
                    id,
                    BlockNode {
                        block_id: deep_str(&deep, "block_id").unwrap_or_default(),
                        block_type: deep_str(&deep, "block_type").unwrap_or_default(),
                        attrs,
                        content_ref: deep_str(&deep, "content_ref").unwrap_or_default(),
                    },
                ));
                walk(tree, TreeParentId::Node(id), depth + 1, out);
            }
        }
        walk(&tree, TreeParentId::Root, 0, &mut out);
        out
    }

    /// 删除块节点（子树随之不可见）。
    pub fn remove_block(&self, node: loro::TreeID, now: i64) -> Result<(), Error> {
        self.blocks_tree()
            .delete(node)
            .map_err(err("blocks.delete"))?;
        self.touch(now)
    }

    /// 移动块节点（块编辑器拖拽）。
    pub fn move_block(
        &self,
        node: loro::TreeID,
        new_parent: Option<loro::TreeID>,
        now: i64,
    ) -> Result<(), Error> {
        let parent_id: TreeParentId = new_parent
            .map(TreeParentId::Node)
            .unwrap_or(TreeParentId::Root);
        self.blocks_tree()
            .mov(node, parent_id)
            .map_err(err("blocks.mov"))?;
        self.touch(now)
    }

    // ------------------------------------------------------------------
    // tasks 容器（LoroMovableList，GTD 集成）
    // ------------------------------------------------------------------

    /// 追加任务项。
    pub fn add_task(&self, task: &NoteTask, now: i64) -> Result<usize, Error> {
        let list = self.tasks_list();
        let item: LoroMap = list
            .push_container(LoroMap::new())
            .map_err(err("tasks.push_container"))?;
        item.insert("task_id", task.task_id.as_str())
            .map_err(err("task.task_id"))?;
        item.insert("title", task.title.as_str())
            .map_err(err("task.title"))?;
        item.insert("status", task.status.as_str())
            .map_err(err("task.status"))?;
        item.insert("priority", task.priority.as_str())
            .map_err(err("task.priority"))?;
        if let Some(due) = task.due_date {
            item.insert("due_date", due).map_err(err("task.due_date"))?;
        }
        self.touch(now)?;
        Ok(list.len())
    }

    /// 读取全部任务（按列表顺序）。
    pub fn tasks(&self) -> Vec<NoteTask> {
        let deep = self.tasks_list().get_deep_value();
        let Some(items) = deep.as_list() else {
            return Vec::new();
        };
        items
            .as_ref()
            .iter()
            .filter_map(|v| {
                let m = v.as_map()?;
                Some(NoteTask {
                    task_id: str_of(m, "task_id"),
                    title: str_of(m, "title"),
                    status: str_of(m, "status"),
                    priority: str_of(m, "priority"),
                    due_date: i64_of(m, "due_date"),
                })
            })
            .collect()
    }

    /// 按 task_id 更新任务状态（含 GTD 状态机校验）。
    pub fn update_task_status(
        &self,
        task_id: &str,
        new_status: &str,
        now: i64,
    ) -> Result<bool, Error> {
        let list = self.tasks_list();
        let deep = list.get_deep_value();
        let Some(items) = deep.as_list() else {
            return Ok(false);
        };
        for (i, v) in items.as_ref().iter().enumerate() {
            let Some(m) = v.as_map() else { continue };
            if m.as_ref()
                .get("task_id")
                .and_then(|v| v.as_string())
                .map(|s| sv_eq(s, task_id))
                != Some(true)
            {
                continue;
            }
            let old = str_of(m, "status");
            if !NoteTask::is_valid_transition(&old, new_status) {
                return Err(Error::Internal(format!(
                    "invalid GTD transition: {old} -> {new_status}"
                )));
            }
            // 获取嵌套容器句柄（值快照不可写，必须走 ValueOrContainer）
            match list.get(i) {
                Some(loro::ValueOrContainer::Container(loro::Container::Map(item))) => {
                    item.insert("status", new_status)
                        .map_err(err("task.status"))?;
                }
                _ => {
                    return Err(Error::Internal(format!(
                        "task {task_id}: item is not a map container"
                    )))
                }
            }
            self.touch(now)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// 重排序任务（GTD 优先级调整 / 手动排序）。
    pub fn move_task(&self, from: usize, to: usize, now: i64) -> Result<(), Error> {
        let list = self.tasks_list();
        let len = list.len();
        if from >= len || to >= len {
            return Err(Error::Internal("tasks index out of range".to_string()));
        }
        list.mov(from, to).map_err(err("tasks.mov"))?;
        self.touch(now)
    }

    /// 按 task_id 移除任务。
    pub fn remove_task(&self, task_id: &str, now: i64) -> Result<bool, Error> {
        let list = self.tasks_list();
        let deep = list.get_deep_value();
        let Some(items) = deep.as_list() else {
            return Ok(false);
        };
        for (i, v) in items.as_ref().iter().enumerate().rev() {
            let Some(m) = v.as_map() else { continue };
            if m.as_ref()
                .get("task_id")
                .and_then(|v| v.as_string())
                .map(|s| sv_eq(s, task_id))
                == Some(true)
            {
                list.delete(i, 1).map_err(err("tasks.delete"))?;
                self.touch(now)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    // ------------------------------------------------------------------
    // backlinks 容器（LoroList，双链本地缓存）
    // ------------------------------------------------------------------

    /// 添加反向链接（源笔记 → 本笔记）。
    pub fn add_backlink(
        &self,
        source_note_id: &str,
        link_text: &str,
        now: i64,
    ) -> Result<(), Error> {
        let list = self.backlinks_list();
        let item: LoroMap = list
            .push_container(LoroMap::new())
            .map_err(err("backlinks.push_container"))?;
        item.insert("source_note_id", source_note_id)
            .map_err(err("backlink.source_note_id"))?;
        item.insert("link_text", link_text)
            .map_err(err("backlink.link_text"))?;
        self.touch(now)
    }

    /// 读取全部反向链接。
    pub fn backlinks(&self) -> Vec<Backlink> {
        let deep = self.backlinks_list().get_deep_value();
        let Some(items) = deep.as_list() else {
            return Vec::new();
        };
        items
            .as_ref()
            .iter()
            .filter_map(|v| {
                let m = v.as_map()?;
                Some(Backlink {
                    source_note_id: str_of(m, "source_note_id"),
                    link_text: str_of(m, "link_text"),
                })
            })
            .collect()
    }

    /// 移除来自指定笔记的全部反向链接。
    pub fn remove_backlinks_from(&self, source_note_id: &str, now: i64) -> Result<usize, Error> {
        let list = self.backlinks_list();
        let deep = list.get_deep_value();
        let Some(items) = deep.as_list() else {
            return Ok(0);
        };
        let mut removed = 0;
        for (i, v) in items.as_ref().iter().enumerate().rev() {
            let Some(m) = v.as_map() else { continue };
            if m.as_ref()
                .get("source_note_id")
                .and_then(|v| v.as_string())
                .map(|s| sv_eq(s, source_note_id))
                == Some(true)
            {
                list.delete(i, 1).map_err(err("backlinks.delete"))?;
                removed += 1;
            }
        }
        if removed > 0 {
            self.touch(now)?;
        }
        Ok(removed)
    }

    // ------------------------------------------------------------------
    // 同步编码（V19 §30.2）
    // ------------------------------------------------------------------

    /// 全量快照导出（持久化 / 新设备首次同步）。
    pub fn export_snapshot(&self) -> Result<Vec<u8>, Error> {
        self.doc
            .export(ExportMode::Snapshot)
            .map_err(|e| Error::Internal(format!("loro snapshot export: {e}")))
    }

    /// 增量导出（自 `since` 之后的更新，实时同步）。
    pub fn export_update_since(&self, since: &loro::VersionVector) -> Result<Vec<u8>, Error> {
        self.doc
            .export(ExportMode::updates(since))
            .map_err(|e| Error::Internal(format!("loro update export: {e}")))
    }

    /// 当前版本向量（增量同步水位线）。
    pub fn version_vector(&self) -> loro::VersionVector {
        self.doc.oplog_vv()
    }

    /// 应用远端增量/快照（CRDT 自动合并）。
    pub fn apply_update(&self, bytes: &[u8]) -> Result<(), Error> {
        self.doc
            .import(bytes)
            .map(|_| ())
            .map_err(|e| Error::Internal(format!("loro import: {e}")))
    }
}

fn err(field: &'static str) -> impl Fn(loro::LoroError) -> Error {
    move |e| Error::Internal(format!("loro {field}: {e}"))
}

// ---------------------------------------------------------------------------
// LoroValue 深值树导航辅助（读路径统一走 get_deep_value，避免值/容器二义性）
// ---------------------------------------------------------------------------

/// 深值 Map 中取 String 字段。
fn deep_str(deep: &loro::LoroValue, key: &str) -> Option<String> {
    deep.get_by_key(key)
        .and_then(|v| v.as_string())
        .map(|s| s.to_string())
}

/// 深值 Map 中取 i64 字段。
fn deep_i64(deep: &loro::LoroValue, key: &str) -> Option<i64> {
    deep.get_by_key(key).and_then(|v| v.as_i64()).copied()
}

/// 深值 Map 键 → String 的闭包视图（列表项读取用）。
fn str_of(m: &loro::LoroMapValue, key: &str) -> String {
    m.as_ref()
        .get(key)
        .and_then(|v| v.as_string())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

/// 深值 Map 键 → Option<i64>。
fn i64_of(m: &loro::LoroMapValue, key: &str) -> Option<i64> {
    m.as_ref().get(key).and_then(|v| v.as_i64()).copied()
}

/// LoroStringValue 与 &str 比较。
fn sv_eq(s: &loro::LoroStringValue, other: &str) -> bool {
    s.as_ref() == other
}

// ===========================================================================
// 测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_700_000_000_000;

    #[test]
    fn meta_roundtrip() {
        let note = NoteDoc::new("我的笔记", "ws-1").unwrap();
        note.set_timestamps(T0, T0).unwrap();
        let m = note.meta();
        assert_eq!(m.title, "我的笔记");
        assert_eq!(m.workspace_id, "ws-1");
        assert_eq!(m.created_at, T0);
        assert_eq!(m.encryption, EncryptionLevel::None);
        assert!(m.tags.is_empty());
    }

    #[test]
    fn tags_dedup_and_remove() {
        let note = NoteDoc::new("t", "ws").unwrap();
        note.add_tag("rust", T0).unwrap();
        note.add_tag("rust", T0 + 1).unwrap(); // 重复添加被忽略
        note.add_tag("loro", T0 + 2).unwrap();
        assert_eq!(note.meta().tags, vec!["rust", "loro"]);
        note.remove_tag("rust", T0 + 3).unwrap();
        assert_eq!(note.meta().tags, vec!["loro"]);
    }

    #[test]
    fn body_edit_paths() {
        let note = NoteDoc::new("t", "ws").unwrap();
        note.set_body("Hello 世界", T0).unwrap();
        assert_eq!(note.body(), "Hello 世界");
        note.insert_body(5, " Loro", T0 + 1).unwrap();
        assert_eq!(note.body(), "Hello Loro 世界");
        note.delete_body(0, 6, T0 + 2).unwrap();
        assert_eq!(note.body(), "Loro 世界");
        assert_eq!(note.body_len(), "Loro 世界".chars().count());
        assert_eq!(note.meta().updated_at, T0 + 2);
    }

    #[test]
    fn blocks_tree_structure() {
        let note = NoteDoc::new("t", "ws").unwrap();
        let heading = BlockNode {
            block_id: "b-1".into(),
            block_type: "heading".into(),
            attrs: HashMap::from([("level".into(), "2".into())]),
            content_ref: "body:0:5".into(),
        };
        let para = BlockNode {
            block_id: "b-2".into(),
            block_type: "paragraph".into(),
            attrs: HashMap::new(),
            content_ref: "body:5:20".into(),
        };
        let h = note.add_block(None, &heading, T0).unwrap();
        note.add_block(None, &para, T0).unwrap();
        // 子块（heading 下的列表项）
        let item = BlockNode {
            block_id: "b-3".into(),
            block_type: "list".into(),
            attrs: HashMap::from([("ordered".into(), "true".into())]),
            content_ref: "body:20:30".into(),
        };
        note.add_block(Some(h), &item, T0).unwrap();

        let blocks = note.blocks();
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].2.block_type, "heading");
        assert_eq!(
            blocks[0].2.attrs.get("level").map(String::as_str),
            Some("2")
        );
        // DFS: heading(0) -> list child(1) -> paragraph(0)
        assert_eq!(blocks[0].0, 0);
        assert_eq!(blocks[1].0, 1);
        assert_eq!(blocks[1].2.block_id, "b-3");
        assert_eq!(blocks[2].0, 0);
        assert_eq!(blocks[2].2.block_id, "b-2");
    }

    #[test]
    fn blocks_remove_and_move() {
        let note = NoteDoc::new("t", "ws").unwrap();
        let b1 = note
            .add_block(
                None,
                &BlockNode {
                    block_id: "1".into(),
                    block_type: "paragraph".into(),
                    attrs: HashMap::new(),
                    content_ref: String::new(),
                },
                T0,
            )
            .unwrap();
        let _b2 = note
            .add_block(
                None,
                &BlockNode {
                    block_id: "2".into(),
                    block_type: "paragraph".into(),
                    attrs: HashMap::new(),
                    content_ref: String::new(),
                },
                T0,
            )
            .unwrap();
        assert_eq!(note.blocks().len(), 2);
        note.remove_block(b1, T0).unwrap();
        assert_eq!(note.blocks().len(), 1);
        assert_eq!(note.blocks()[0].2.block_id, "2");
    }

    #[test]
    fn tasks_gtd_lifecycle() {
        let note = NoteDoc::new("t", "ws").unwrap();
        note.add_task(
            &NoteTask {
                task_id: "task-1".into(),
                title: "写周报".into(),
                status: NoteTask::STATUS_INBOX.into(),
                priority: "medium".into(),
                due_date: Some(T0 + 86400_000),
            },
            T0,
        )
        .unwrap();
        note.add_task(
            &NoteTask {
                task_id: "task-2".into(),
                title: "复盘".into(),
                status: NoteTask::STATUS_NEXT.into(),
                priority: "high".into(),
                due_date: None,
            },
            T0,
        )
        .unwrap();
        assert_eq!(note.tasks().len(), 2);
        assert_eq!(note.tasks()[0].title, "写周报");
        assert_eq!(note.tasks()[0].due_date, Some(T0 + 86400_000));

        // inbox → next 合法
        note.update_task_status("task-1", NoteTask::STATUS_NEXT, T0 + 1)
            .unwrap();
        assert_eq!(note.tasks()[0].status, "next");

        // next → inbox 非法（GTD 状态机）
        let e = note.update_task_status("task-1", NoteTask::STATUS_INBOX, T0 + 2);
        assert!(e.is_err());

        // 重排序
        note.move_task(0, 1, T0 + 3).unwrap();
        assert_eq!(note.tasks()[0].task_id, "task-2");

        // 移除
        assert!(note.remove_task("task-2", T0 + 4).unwrap());
        assert_eq!(note.tasks().len(), 1);
        assert_eq!(note.tasks()[0].task_id, "task-1");
    }

    #[test]
    fn backlinks_cache() {
        let note = NoteDoc::new("t", "ws").unwrap();
        note.add_backlink("note-a", "[[我的笔记]]", T0).unwrap();
        note.add_backlink("note-b", "[[我的笔记|别名]]", T0)
            .unwrap();
        note.add_backlink("note-a", "[[我的笔记]]重复", T0).unwrap();
        assert_eq!(note.backlinks().len(), 3);
        assert_eq!(note.backlinks()[1].source_note_id, "note-b");

        let removed = note.remove_backlinks_from("note-a", T0 + 1).unwrap();
        assert_eq!(removed, 2);
        assert_eq!(note.backlinks().len(), 1);
        assert_eq!(note.backlinks()[0].source_note_id, "note-b");
    }

    #[test]
    fn snapshot_roundtrip_all_containers() {
        let note = NoteDoc::new("快照测试", "ws-9").unwrap();
        note.set_timestamps(T0, T0).unwrap();
        note.add_tag("p0", T0).unwrap();
        note.set_body("正文内容 body content", T0 + 1).unwrap();
        note.add_block(
            None,
            &BlockNode {
                block_id: "b1".into(),
                block_type: "heading".into(),
                attrs: HashMap::from([("level".into(), "1".into())]),
                content_ref: "body:0:4".into(),
            },
            T0 + 2,
        )
        .unwrap();
        note.add_task(
            &NoteTask {
                task_id: "t1".into(),
                title: "任务".into(),
                status: "inbox".into(),
                priority: "low".into(),
                due_date: None,
            },
            T0 + 3,
        )
        .unwrap();
        note.add_backlink("src", "[[链接]]", T0 + 4).unwrap();
        note.set_encryption(EncryptionLevel::Aes256Gcm, T0 + 5)
            .unwrap();

        let bytes = note.export_snapshot().unwrap();
        let restored = NoteDoc::from_snapshot(&bytes).unwrap();

        assert_eq!(restored.meta(), note.meta());
        assert_eq!(restored.meta().encryption, EncryptionLevel::Aes256Gcm);
        assert_eq!(restored.body(), "正文内容 body content");
        let blocks = restored.blocks();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].2.block_type, "heading");
        assert_eq!(restored.tasks().len(), 1);
        assert_eq!(restored.tasks()[0].task_id, "t1");
        assert_eq!(restored.backlinks().len(), 1);
    }

    #[test]
    fn crdt_concurrent_merge_body() {
        // §30.2: 两端离线编辑同篇笔记 → 增量互通 → 收敛一致
        let a = NoteDoc::new("协同", "ws").unwrap();
        a.set_timestamps(T0, T0).unwrap();
        a.set_body("base", T0).unwrap();

        let b = a.fork(); // 分叉副本

        // 并发编辑: A 在头部插入, B 在尾部插入
        a.insert_body(0, "A!", T0 + 1).unwrap();
        b.insert_body(4, "B!", T0 + 1).unwrap();
        assert_eq!(a.body(), "A!base");
        assert_eq!(b.body(), "baseB!");

        // 增量同步: A 的更新导出 → 应用到 B
        let vv_before = {
            let mut vv = a.version_vector();
            vv.clear();
            vv
        };
        let update_a = a.export_update_since(&vv_before).unwrap();
        b.apply_update(&update_a).unwrap();

        let update_b = b.export_update_since(&vv_before).unwrap();
        a.apply_update(&update_b).unwrap();

        // CRDT 收敛: 两端一致
        assert_eq!(a.body(), b.body());
        assert!(a.body().starts_with("A!") || a.body().contains("A!"));
        assert!(a.body().contains("B!"));
    }

    #[test]
    fn crdt_concurrent_merge_containers() {
        // 并发: A 加任务, B 加标签 → 合并后两者都在
        let a = NoteDoc::new("t", "ws").unwrap();
        a.set_timestamps(T0, T0).unwrap();
        let b = a.fork();

        a.add_task(
            &NoteTask {
                task_id: "ta".into(),
                title: "A的任务".into(),
                status: "inbox".into(),
                priority: "low".into(),
                due_date: None,
            },
            T0,
        )
        .unwrap();
        b.add_tag("B标签", T0).unwrap();
        b.set_body("B的正文", T0).unwrap();

        let mut empty = a.version_vector();
        empty.clear();
        let ua = a.export_update_since(&empty).unwrap();
        let ub = b.export_update_since(&empty).unwrap();

        a.apply_update(&ub).unwrap();
        b.apply_update(&ua).unwrap();

        assert_eq!(a.meta().tags, b.meta().tags);
        assert_eq!(a.meta().tags, vec!["B标签"]);
        assert_eq!(a.body(), b.body());
        assert_eq!(a.body(), "B的正文");
        assert_eq!(a.tasks().len(), 1);
        assert_eq!(a.tasks()[0].task_id, "ta");
        assert_eq!(b.tasks().len(), 1);
    }
}
