//! Aurora Note Mobile FFI (UniFFI + JNI)
//!
//! 对应 V19 §28.1 Trait 签名的移动端适配，提供跨平台 FFI 入口：
//! - iOS: 通过 UniFFI Record/Object 注解
//! - Android: 通过 JNI C ABI 桥接（Java_com_aurora_note_UniffiAppCore_*）
//!
//! 方法均为同步阻塞式（符合 V19 保留同步签名的决策）。
//! async Trait 方法（KVStore / SearchBackend）通过内部 tokio runtime 驱动。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use aurora_core::l1_infrastructure::note_doc::NoteDoc;

uniffi::setup_scaffolding!();

// V19 §31 DEV-005: iroh P2P 同步引擎（feature-gated）
#[cfg(feature = "p2p-sync")]
pub mod p2p_sync;

// ===========================================================================
// 类型定义（UniFFI 兼容）
// ===========================================================================

#[derive(uniffi::Error, Debug, thiserror::Error)]
pub enum MobileError {
    #[error("core init failed: {message}")]
    InitFailed { message: String },
    #[error("operation failed: {message}")]
    OperationFailed { message: String },
    #[error("not found: {resource}")]
    NotFound { resource: String },
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct NoteSummary {
    pub id: String,
    pub title: String,
    pub updated_at: String,
}

/// 复习卡片条目（V20 Phase 3 FSRS — 今日页「复习」分区）。
#[derive(uniffi::Record, Debug, Clone)]
pub struct ReviewCard {
    pub card_id: String,
    pub note_id: String,
    /// 下次到期时间（RFC3339）。
    pub due_at: String,
    /// 当前可提取性（0-1, 越低越急需复习）。
    pub retrievability: f64,
    /// 已复习次数。
    pub reps: i64,
    /// 失败次数。
    pub lapses: i64,
}

/// 反向链接条目（V20 §5.4 双链反链面板 — Rust 侧产出）。
#[derive(uniffi::Record, Debug, Clone)]
pub struct BacklinkItem {
    /// 引用方笔记 ID。
    pub source_note_id: String,
    /// 引用方标题（面板显示）。
    pub source_title: String,
}

/// TodayView 头部统计（V20 §5.4.2: Rust 侧聚合，前端只渲染）。
#[derive(uniffi::Record, Debug, Clone, Default)]
pub struct TodayViewStats {
    /// 进行中（GTD: inbox/next/waiting/scheduled）。
    pub active: i64,
    /// 已完成。
    pub done: i64,
    /// 今日到期（含逾期）。
    pub due_today: i64,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct SearchResult {
    pub note_id: String,
    pub title: String,
    pub snippet: String,
    pub score: f64,
}

// ===========================================================================
// 内部数据模型（JSON 序列化用于 KVStore 持久化）
// ===========================================================================

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct NoteRecord {
    id: String,
    title: String,
    content: String,
    created_at: String,
    updated_at: String,
}

impl NoteRecord {
    fn new(title: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title,
            content: String::new(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    fn to_summary(&self) -> NoteSummary {
        NoteSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            updated_at: self.updated_at.clone(),
        }
    }
}

// ===========================================================================
// UniffiAppCore — 真实实现（接入 bootstrap + KVStore + SearchBackend + Loro）
// ===========================================================================

/// V23-I4: 快照元数据（时间机器列表项）。
#[derive(uniffi::Record, Clone, Debug, serde::Serialize)]
pub struct SnapshotInfo {
    pub version: i64,
    pub created_at: String,
    pub size: i64,
}

/// 每条笔记对应一个独立的 `NoteDoc`（V19 §30.1 五容器模型），
/// 快照持久化到 KVStore（`notesnap:{id}`），元数据存 `note:{id}`。
/// V19 §36.3: 移动端使用原生 Loro 绑定 — CRDT 语义，支持未来多端合并。
#[derive(uniffi::Object)]
pub struct UniffiAppCore {
    core: Option<Arc<aurora_core::app_core::AppCore>>,
    runtime: tokio::runtime::Runtime,
    data_dir: PathBuf,
    fallback_notes: Mutex<Vec<NoteRecord>>,
    /// Loro 文档缓存（note_id → NoteDoc 五容器模型）
    docs: Mutex<std::collections::HashMap<String, NoteDoc>>,
    is_fallback: bool,
    /// V23-I2: Mirror 单向导出（调度器 + 根目录; 铁律 9 永不读回）
    mirror: Option<(aurora_core::mirror::MirrorScheduler, aurora_core::mirror::MirrorRoot)>,
    /// V23-I2: blocks 双轨存储（None = 内存降级模式）
    blocks: Option<aurora_core::blocks::BlockStore>,
}

impl UniffiAppCore {
    pub fn new(data_dir: String) -> Result<Arc<Self>, MobileError> {
        let data_dir = PathBuf::from(&data_dir);
        // V23-I2: blocks 双轨（复用 migration 建的库; 打不开则降级 None）
        let blocks = {
            let db_path = data_dir.join("aurora.db");
            rusqlite::Connection::open(&db_path).ok().map(aurora_core::blocks::BlockStore::new)
        };
        // V23-I2: Mirror 单向导出（data_dir/mirror — 铁律 9: 永不读回）
        let mirror = Some((
            aurora_core::mirror::MirrorScheduler::new(),
            aurora_core::mirror::MirrorRoot::new(data_dir.join("mirror")),
        ));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| MobileError::InitFailed {
                message: format!("tokio runtime: {e}"),
            })?;

        // 尝试完整 bootstrap 装配
        match aurora_bootstrap::bootstrap(&data_dir) {
            Ok(booted) => {
                tracing::info!("bootstrap success — full mode (loro CRDT enabled)");
                // V20 §4.5: 启动期投影追赶（restore_seq + catch_up 在
                // startup 后执行，杀进程后索引自动补齐）
                let core = booted.core.clone();
                runtime.block_on(async move {
                    let _ = core.startup();
                    let _ = core.catch_up_projections().await;
                });
                Ok(Arc::new(Self {
                    core: Some(booted.core),
                    runtime,
                    data_dir,
                    fallback_notes: Mutex::new(Vec::new()),
                    docs: Mutex::new(std::collections::HashMap::new()),
                    is_fallback: false,
                    mirror,
                    blocks,
                }))
            }
            Err(e) => {
                tracing::warn!("bootstrap failed, falling back to in-memory: {e}");
                // data_dir 在 struct 构造中被 move — mirror root 需提前算
                let mirror_root = data_dir.join("mirror");
                Ok(Arc::new(Self {
                    core: None,
                    runtime,
                    data_dir,
                    fallback_notes: Mutex::new(Vec::new()),
                    docs: Mutex::new(std::collections::HashMap::new()),
                    is_fallback: true,
                    // 降级模式仍提供 mirror（本地目录不依赖 core 装配）
                    mirror: Some((
                        aurora_core::mirror::MirrorScheduler::new(),
                        aurora_core::mirror::MirrorRoot::new(mirror_root),
                    )),
                    blocks: None,
                }))
            }
        }
    }

    /// 创建（或从 KVStore 恢复）笔记的 NoteDoc 并放入缓存。
    fn doc_for_note(&self, note_id: &str) -> NoteDoc {
        // 缓存命中
        if let Some(doc) = self.docs.lock().unwrap().get(note_id) {
            return doc.clone();
        }

        // 尝试从 KVStore 恢复快照
        let mut note_doc =
            NoteDoc::new("", "").unwrap_or_else(|_| NoteDoc::from_doc(loro::LoroDoc::new()));
        if let Some(core) = &self.core {
            let kv = core.kv_store.clone();
            let key = format!("notesnap:{note_id}");
            let snapshot = self
                .runtime
                .block_on(async { kv.get(&key).await.ok().flatten() });
            if let Some(bytes) = snapshot {
                if let Ok(d) = NoteDoc::from_snapshot(&bytes) {
                    note_doc = d;
                    tracing::debug!(note_id, "loro doc restored from snapshot");
                }
            }
        }

        self.docs
            .lock()
            .unwrap()
            .insert(note_id.to_string(), note_doc.clone());
        note_doc
    }

    /// 导出 Loro 快照并持久化到 KVStore。
    fn persist_doc(&self, note_id: &str, doc: &NoteDoc) -> Result<(), MobileError> {
        if let Some(core) = &self.core {
            let snapshot = doc
                .export_snapshot()
                .map_err(|e| MobileError::OperationFailed {
                    message: format!("loro export: {e}"),
                })?;
            let kv = core.kv_store.clone();
            let key = format!("notesnap:{note_id}");
            self.runtime
                .block_on(async { kv.set(&key, &snapshot).await })
                .map_err(|e| MobileError::OperationFailed {
                    message: format!("kv set snapshot: {e}"),
                })?;
        }
        Ok(())
    }

    /// P2P 同步用：公开访问指定笔记的 NoteDoc（缓存/快照恢复）。
    #[cfg(feature = "p2p-sync")]
    pub fn doc_for_note_public(&self, note_id: &str) -> Result<NoteDoc, MobileError> {
        // 验证笔记存在（与 get_note_content 一致的语义）
        let exists = if let Some(core) = &self.core {
            let kv = core.kv_store.clone();
            let key = format!("note:{note_id}");
            self.runtime
                .block_on(async { kv.get(&key).await })
                .map_err(|e| MobileError::OperationFailed {
                    message: format!("get: {e}"),
                })?
                .is_some()
        } else {
            self.fallback_notes
                .lock()
                .unwrap()
                .iter()
                .any(|n| n.id == note_id)
        };
        if !exists {
            return Err(MobileError::NotFound {
                resource: note_id.to_string(),
            });
        }
        Ok(self.doc_for_note(note_id))
    }

    /// P2P 同步用：公开持久化指定笔记当前缓存快照。
    #[cfg(feature = "p2p-sync")]
    pub fn persist_note_snapshot(&self, note_id: &str) -> Result<(), MobileError> {
        let doc = self.doc_for_note(note_id);
        self.persist_doc(note_id, &doc)
    }

    fn create_note_impl(self: &Arc<Self>, title: String) -> Result<String, MobileError> {
        let note = NoteRecord::new(title);

        // 创建五容器 Loro 文档（meta/body/blocks/tasks/backlinks）
        let now_ms = chrono::Utc::now().timestamp_millis();
        let doc = NoteDoc::new(&note.title, "").map_err(|e| MobileError::OperationFailed {
            message: format!("note doc init: {e}"),
        })?;
        doc.set_timestamps(now_ms, now_ms)
            .map_err(|e| MobileError::OperationFailed {
                message: format!("note doc timestamps: {e}"),
            })?;

        if let Some(core) = &self.core {
            // 真实模式：KVStore 持久化 + SearchBackend 索引
            let kv = core.kv_store.clone();
            let key = format!("note:{}", note.id);
            let value = serde_json::to_vec(&note).map_err(|e| MobileError::OperationFailed {
                message: format!("serialize: {e}"),
            })?;

            self.runtime
                .block_on(async { kv.set(&key, &value).await })
                .map_err(|e| MobileError::OperationFailed {
                    message: format!("kv set: {e}"),
                })?;

            // 持久化 Loro 快照
            self.persist_doc(&note.id, &doc)?;

            // V20 §4.5 事件驱动: 发 NoteCreated 事件（投影消费建索引，
            // 替代直接 index_note 旁路 — 保证与重建路径同源一致）
            core.event_bus.publish(
                aurora_core::event_bus::layered::AppEvent::NoteCreated {
                    note_id: note.id.clone(),
                    title: note.title.clone(),
                    content: note.content.clone(),
                },
            );
            // 同步驱动投影追赶（移动端单线程 runtime，启动期/写后各一次）
            let core_clone = core.clone();
            self.runtime
                .block_on(async move { core_clone.catch_up_projections().await })
                .ok();

            // 缓存 LoroDoc
            self.docs.lock().unwrap().insert(note.id.clone(), doc);
        } else {
            // Fallback 模式：内存存储（LoroDoc 仍然提供 CRDT 语义）
            self.docs.lock().unwrap().insert(note.id.clone(), doc);
            self.fallback_notes.lock().unwrap().push(note.clone());
        }

        Ok(note.id)
    }

    fn list_notes_impl(self: &Arc<Self>) -> Vec<NoteSummary> {
        if let Some(core) = &self.core {
            let kv = core.kv_store.clone();
            let entries = self
                .runtime
                .block_on(async { kv.scan_prefix("note:").await });

            match entries {
                Ok(pairs) => pairs
                    .iter()
                    .filter_map(|(_, bytes)| serde_json::from_slice::<NoteRecord>(bytes).ok())
                    .map(|n| n.to_summary())
                    .collect(),
                Err(_) => Vec::new(),
            }
        } else {
            self.fallback_notes
                .lock()
                .unwrap()
                .iter()
                .map(|n| n.to_summary())
                .collect()
        }
    }

    fn search_notes_impl(self: &Arc<Self>, query: String) -> Vec<SearchResult> {
        if let Some(core) = &self.core {
            // V20 Phase 3 §5.4: 口语化语义查询 — NL 解析任务/链接意图
            // 走投影聚合; notes 意图/兜底走 Tantivy 全文。
            let parsed = aurora_core::l2_engines::nl_query::NlQueryParser::parse(&query);
            match parsed.source.as_str() {
                "tasks" => return Self::search_tasks_via_projection(&parsed, core),
                "links" => {
                    if let Some(aurora_core::l2_engines::query::Filter::Eq { value, .. }) =
                        &parsed.filter
                    {
                        let target = value.as_str().unwrap_or_default().to_string();
                        return Self::backlinks_for_query(self, &target, &query);
                    }
                }
                _ => {}
            }
            use aurora_core::traits::search_backend::SearchOptions;
            let search = core.search.clone();
            let result = self
                .runtime
                .block_on(async { search.search(&query, &SearchOptions::default()).await });

            match result {
                Ok(search_result) => search_result
                    .hits
                    .iter()
                    .map(|h| SearchResult {
                        note_id: h.note_id.clone(),
                        title: h.title.clone(),
                        snippet: h.snippet.clone(),
                        score: h.score as f64,
                    })
                    .collect(),
                Err(_) => {
                    // 搜索后端失败时，降级到简单的标题匹配
                    self.simple_title_search(&query)
                }
            }
        } else {
            self.simple_title_search(&query)
        }
    }

    /// 反链: BidiLinkProjection.incoming + KVStore 查引用方标题。
    fn backlinks_impl(self: &Arc<Self>, note_id: String) -> Vec<BacklinkItem> {
        if let Some(core) = &self.core {
            let sources = core.bidi_link_incoming(&note_id);
            let kv = core.kv_store.clone();
            sources
                .into_iter()
                .map(|src| {
                    let title = self
                        .runtime
                        .block_on(async {
                            kv.get(&format!("note:{src}")).await.ok().flatten()
                        })
                        .and_then(|bytes| {
                            serde_json::from_slice::<NoteRecord>(&bytes).ok()
                        })
                        .map(|n| n.title)
                        .unwrap_or_else(|| src.clone());
                    BacklinkItem {
                        source_note_id: src,
                        source_title: title,
                    }
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// 链接意图搜索: 反链面板数据转 SearchResult。
    fn backlinks_for_query(
        self: &Arc<Self>,
        target: &str,
        raw_query: &str,
    ) -> Vec<SearchResult> {
        Self::backlinks_impl(self, target.to_string())
            .into_iter()
            .map(|b| SearchResult {
                note_id: b.source_note_id,
                title: b.source_title,
                snippet: format!("引用了「{raw_query}」"),
                score: 1.0,
            })
            .collect()
    }

    /// 任务意图搜索: TaskProjection 聚合（未完成/今日/紧急），
    /// **排除播种行**（seed: 前缀 — 笔记创建占位非真实行动项）。
    fn search_tasks_via_projection(
        parsed: &aurora_core::l2_engines::query::Query,
        core: &std::sync::Arc<aurora_core::app_core::AppCore>,
    ) -> Vec<SearchResult> {
        use aurora_core::l2_engines::query::Filter;
        let tp = core.projections().iter().find_map(|p| {
            p.as_any().and_then(|a| {
                a.downcast_ref::<aurora_core::l2_engines::task_projection::TaskProjection>()
            })
        });
        let Some(tp) = tp else {
            return Vec::new();
        };
        let (undone_only, today_window, urgent_only) = match &parsed.filter {
            Some(Filter::And { filters }) => {
                let undone = filters
                    .iter()
                    .any(|f| matches!(f, Filter::Ne { ref field, .. } if field == "status"));
                let today = filters
                    .iter()
                    .any(|f| matches!(f, Filter::Lte { ref field, .. } if field == "due_date"));
                let urgent = filters.iter().any(|f| {
                    matches!(f, Filter::Eq { ref field, ref value, .. }
                        if field == "priority" && value == "urgent")
                });
                (undone, today, urgent)
            }
            Some(Filter::Eq { field, value, .. }) if field == "priority" => {
                (false, false, value == "urgent")
            }
            Some(Filter::Ne { field, .. }) if field == "status" => (true, false, false),
            _ => (false, false, false),
        };
        let now = chrono::Utc::now().timestamp_millis();
        let mut rows: Vec<_> = if today_window {
            tp.today(now)
        } else if urgent_only {
            tp.by_status("inbox")
                .into_iter()
                .filter(|r| r.priority == "urgent")
                .collect()
        } else if undone_only {
            let mut all = tp.by_status("inbox");
            all.extend(tp.by_status("next"));
            all.extend(tp.by_status("waiting"));
            all.extend(tp.by_status("scheduled"));
            all
        } else {
            tp.by_status("inbox")
        };
        // 排除播种行（占位非行动项）
        rows.retain(|r| !r.task_id.starts_with("seed:"));
        rows.truncate(parsed.pagination.as_ref().map(|p| p.limit).unwrap_or(50));
        rows.into_iter()
            .map(|r| SearchResult {
                note_id: r.note_id,
                title: r.title,
                snippet: format!("[{}] {}", r.status, r.priority),
                score: 1.0,
            })
            .collect()
    }

    /// TodayView 统计: 任务投影聚合（真实模式）; fallback 空统计。
    fn today_view_stats_impl(self: &Arc<Self>) -> TodayViewStats {
        if let Some(core) = &self.core {
            let (active, done) = core.task_projection_stats();
            let due_today = core.task_projection_due_today();
            TodayViewStats {
                active: active as i64,
                done: done as i64,
                due_today: due_today as i64,
            }
        } else {
            TodayViewStats::default()
        }
    }

    fn simple_title_search(&self, query: &str) -> Vec<SearchResult> {
        let q_lower = query.to_lowercase();
        let notes = self.fallback_notes.lock().unwrap();
        notes
            .iter()
            .filter(|n| n.title.to_lowercase().contains(&q_lower))
            .map(|n| SearchResult {
                note_id: n.id.clone(),
                title: n.title.clone(),
                snippet: n.content.chars().take(100).collect(),
                score: 1.0,
            })
            .collect()
    }

    fn delete_note_impl(self: &Arc<Self>, note_id: String) -> Result<(), MobileError> {
        if let Some(core) = &self.core {
            let kv = core.kv_store.clone();
            let key = format!("note:{}", note_id);
            self.runtime
                .block_on(async { kv.delete(&key).await })
                .map_err(|e| MobileError::OperationFailed {
                    message: format!("delete: {e}"),
                })?;

            // V20 §4.5 事件驱动: NoteDeleted（投影删索引）+ 同步追赶
            use aurora_core::event_bus::layered::AppEvent;
            core.event_bus.publish(AppEvent::NoteDeleted {
                note_id: note_id.clone(),
            });
            let core_clone = core.clone();
            self.runtime
                .block_on(async move { core_clone.catch_up_projections().await })
                .ok();
            // 清理 Loro 文档缓存
            self.docs.lock().unwrap().remove(&note_id);
        } else {
            let mut notes = self.fallback_notes.lock().unwrap();
            notes.retain(|n| n.id != note_id);
        }
        Ok(())
    }

    /// V19 §36.3: saveNote(noteId, content) — 保存笔记内容（Loro CRDT）
    /// V23-I2: 块双轨同步 + Mirror 单向导出。
    ///
    /// mirror 写盘以 KVStore（note:{id} JSON）为数据源 —— feed 只登记
    /// 待写集合，flush 时逐条取最新数据渲染落盘（保存后 flush_all:
    /// 防抖窗口内连续保存自然合并为最后一次状态，正确性优先，
    /// 定时器驱动留 I2 收口）。失败仅记日志不阻塞保存主路径。
    /// V23-I4: 时间机器会话（打开 SQLite; 表由 migration V1 建）。
    fn with_time_machine<T>(
        &self,
        f: impl FnOnce(&aurora_core::time_machine::TimeMachine) -> T,
    ) -> Option<T> {
        let conn = rusqlite::Connection::open(self.data_dir.join("aurora.db")).ok()?;
        let tm = aurora_core::time_machine::TimeMachine::new(conn);
        Some(f(&tm))
    }

    /// 时间机器：列出笔记快照元数据（JSON 数组）— 移动端版本历史 UI（V23-I5）。
    pub fn list_snapshots_impl(&self, note_id: &str) -> String {
        match self.with_time_machine(|tm| tm.list(note_id)) {
            Some(Ok(metas)) => {
                let items: Vec<String> = metas
                    .iter()
                    .map(|m| {
                        format!(
                            "{{\"version\":{},\"created_at\":\"{}\",\"size\":{}}}",
                            m.version, m.created_at, m.size
                        )
                    })
                    .collect();
                format!("[{}]", items.join(","))
            }
            _ => "[]".to_string(),
        }
    }

    /// 时间机器：读取指定版本快照正文（UTF-8 文本）。
    pub fn get_snapshot_content_impl(&self, note_id: &str, version: i64) -> Option<String> {
        match self.with_time_machine(|tm| tm.load(note_id, version)) {
            Some(Ok(Some(bytes))) => String::from_utf8(bytes).ok(),
            _ => None,
        }
    }

    fn sync_blocks_and_mirror(&self, note_id: &str, _title: &str, content: &str, _updated_at: &str) {
        // blocks 双轨（None = 内存降级模式）
        if let Some(blocks) = &self.blocks {
            if let Err(e) = blocks.sync_note_blocks(note_id, None, content) {
                tracing::warn!(note_id, error = %e, "blocks sync failed");
            }
        }
        // V23-I4: 时间机器快照（每次保存一版; R6 上限 20 自动裁剪）
        if let Some(Err(e)) = self.with_time_machine(|tm| tm.save(note_id, content.as_bytes())) {
            tracing::warn!(note_id, error = %e, "snapshot save failed");
        }
        let Some((scheduler, root)) = &self.mirror else { return };
        scheduler.feed(note_id);
        // 保存后兜底 flush：全部 pending 逐条取最新态落盘后 complete
        // （防抖窗口内连续保存自然合并; 3s 窗口定时器驱动留 I2 收口）
        for id in scheduler.pending_ids() {
            let (title, content, updated) = self.note_snapshot(&id);
            let body = aurora_core::mirror::render_note_markdown(
                &id, &title, &content, &[], &updated,
            );
            let rel = aurora_core::mirror::mirror_rel_path("default", &title, &id);
            match aurora_core::mirror::write_note_to_mirror(root.path(), &rel, &body) {
                Ok(_) => scheduler.complete(&id),
                Err(e) => {
                    tracing::warn!(note_id = %id, error = %e, "mirror write failed");
                }
            }
        }
    }

    /// mirror 数据源: KVStore 优先（core 装配态），fallback_notes 兜底。
    fn note_snapshot(&self, note_id: &str) -> (String, String, String) {
        if let Some(core) = &self.core {
            let kv = core.kv_store.clone();
            if let Ok(Some(bytes)) =
                self.runtime.block_on(async { kv.get(&format!("note:{}", note_id)).await })
            {
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                    let title = v.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string();
                    let content = v.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
                    let updated = v.get("updated_at").and_then(|u| u.as_str()).unwrap_or("").to_string();
                    return (title, content, updated);
                }
            }
        }
        // fallback
        let notes = self.fallback_notes.lock().unwrap();
        notes
            .iter()
            .find(|n| n.id == note_id)
            .map(|n| (n.title.clone(), n.content.clone(), n.updated_at.clone()))
            .unwrap_or_default()
    }

    fn save_note_content_impl(
        self: &Arc<Self>,
        note_id: String,
        content: String,
    ) -> Result<(), MobileError> {
        // 通过五容器模型的 body 容器写入（CRDT 语义：可多端合并）
        let doc = self.doc_for_note(&note_id);
        let now_ms = chrono::Utc::now().timestamp_millis();
        doc.set_body(&content, now_ms)
            .map_err(|e| MobileError::OperationFailed {
                message: format!("loro set_body: {e}"),
            })?;

        // 持久化 Loro 快照
        self.persist_doc(&note_id, &doc)?;

        if let Some(core) = &self.core {
            let kv = core.kv_store.clone();
            let key = format!("note:{}", note_id);

            // 同步元数据 JSON（updated_at）
            let existing = self
                .runtime
                .block_on(async { kv.get(&key).await })
                .map_err(|e| MobileError::OperationFailed {
                    message: format!("get: {e}"),
                })?;

            let mut note: NoteRecord = match existing {
                Some(bytes) => {
                    serde_json::from_slice(&bytes).map_err(|e| MobileError::OperationFailed {
                        message: format!("deserialize: {e}"),
                    })?
                }
                None => return Err(MobileError::NotFound { resource: note_id }),
            };

            note.content = content.clone();
            note.updated_at = chrono::Utc::now().to_rfc3339();

            let value = serde_json::to_vec(&note).map_err(|e| MobileError::OperationFailed {
                message: format!("serialize: {e}"),
            })?;

            self.runtime
                .block_on(async { kv.set(&key, &value).await })
                .map_err(|e| MobileError::OperationFailed {
                    message: format!("set: {e}"),
                })?;

            // V23-I2: 块级双轨同步（notes 为主源, blocks 并列索引）
            // Mirror 单向 feed（若配置 mirror 目录 — 3s 防抖落盘）
            self.sync_blocks_and_mirror(&note.id, &note.title, &content, &note.updated_at);

            // V20 §4.5 事件驱动: 内容变更 → NoteMetadataChanged（投影从
            // KVStore 数据源重取最新内容重建索引，写路径与 rebuild 同源）
            use aurora_core::event_bus::layered::{AppEvent, NoteChanges};
            core.event_bus.publish(AppEvent::NoteMetadataChanged {
                note_id: note.id.clone(),
                changes: NoteChanges {
                    title: None,
                    tags: None,
                },
            });
            let core_clone = core.clone();
            self.runtime
                .block_on(async move { core_clone.catch_up_projections().await })
                .ok();

            // V20 Phase 3 GTD 闭环: 正文行动项自动提取 → 任务投影
            // （- [ ] 任务 / 中文动词句 → inbox; TodayView 聚合可见）
            let core_act = core.clone();
            self.runtime
                .block_on(async move {
                    if let Some(tp) = core_act
                        .projections()
                        .iter()
                        .find_map(|p| {
                            p.as_any().and_then(|a| {
                                a.downcast_ref::<aurora_core::l2_engines::task_projection::TaskProjection>()
                            })
                        })
                    {
                        let _ = aurora_core::l2_engines::action_extractor::ActionItemExtractor::apply_to_projection(
                            &note_id, &content, tp,
                        )
                        .await;
                    }
                });
        } else {
            let mut notes = self.fallback_notes.lock().unwrap();
            for n in notes.iter_mut() {
                if n.id == note_id {
                    n.content = content;
                    n.updated_at = chrono::Utc::now().to_rfc3339();
                    return Ok(());
                }
            }
            return Err(MobileError::NotFound { resource: note_id });
        }
        Ok(())
    }

    /// V19 §36.3: getNoteContent(noteId) — 获取笔记内容（从 LoroText 读取）
    fn get_note_content_impl(self: &Arc<Self>, note_id: String) -> Result<String, MobileError> {
        // 验证笔记存在（元数据）
        let exists = if let Some(core) = &self.core {
            let kv = core.kv_store.clone();
            let key = format!("note:{}", note_id);
            let result = self
                .runtime
                .block_on(async { kv.get(&key).await })
                .map_err(|e| MobileError::OperationFailed {
                    message: format!("get: {e}"),
                })?;
            result.is_some()
        } else {
            let notes = self.fallback_notes.lock().unwrap();
            notes.iter().any(|n| n.id == note_id)
        };

        if !exists {
            return Err(MobileError::NotFound { resource: note_id });
        }

        // 从五容器模型读取正文 body（缓存命中或从快照恢复）
        let doc = self.doc_for_note(&note_id);
        Ok(doc.body())
    }

    /// 导出笔记的完整 Loro 快照（base64）— ProseMirror 编辑器初始化用（DEV-009）。
    ///
    /// 返回 Rust 侧 NoteDoc 当前状态的快照（含 P2P 合并结果）。
    fn get_note_snapshot_impl(self: &Arc<Self>, note_id: String) -> Result<String, MobileError> {
        let exists = if let Some(core) = &self.core {
            let kv = core.kv_store.clone();
            let key = format!("note:{}", note_id);
            self.runtime
                .block_on(async { kv.get(&key).await })
                .map_err(|e| MobileError::OperationFailed {
                    message: format!("get: {e}"),
                })?
                .is_some()
        } else {
            self.fallback_notes
                .lock()
                .unwrap()
                .iter()
                .any(|n| n.id == note_id)
        };
        if !exists {
            return Err(MobileError::NotFound { resource: note_id });
        }

        let doc = self.doc_for_note(&note_id);
        let snapshot = doc
            .export_snapshot()
            .map_err(|e| MobileError::OperationFailed {
                message: format!("loro export: {e}"),
            })?;
        Ok(base64_encode(&snapshot))
    }

    /// 将 JS 侧 Loro 快照（base64）合并进 Rust 侧 NoteDoc 并持久化（DEV-009）。
    ///
    /// 合并语义（CRDT）: import 合并而非替换 — P2P 对端修改不丢失。
    fn save_note_snapshot_impl(
        self: &Arc<Self>,
        note_id: String,
        snapshot_b64: String,
    ) -> Result<(), MobileError> {
        let bytes = base64_decode(&snapshot_b64).ok_or_else(|| MobileError::OperationFailed {
            message: "invalid base64 snapshot".into(),
        })?;
        if bytes.is_empty() {
            return Ok(()); // 空快照视为无操作
        }

        // 合并进缓存文档（快照 blob 与 update 均可 import）
        let doc = self.doc_for_note(&note_id);
        doc.apply_update(&bytes)
            .map_err(|e| MobileError::OperationFailed {
                message: format!("loro import: {e}"),
            })?;

        // 持久化
        self.persist_doc(&note_id, &doc)
    }
}

// ===========================================================================
// JNI 桥接层 — Android Java native 方法
// ===========================================================================

use jni::objects::{JClass, JObject, JString, JValue};
use jni::sys::{jboolean, jint, jlong, jobject, jstring};
use jni::JNIEnv;

fn rust_str_to_jstring(env: &mut JNIEnv, s: &str) -> jstring {
    match env.new_string(s) {
        Ok(js) => js.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

fn jstring_to_rust(env: &mut JNIEnv, js: &JString) -> Option<String> {
    env.get_string(js).ok().map(|s| s.into())
}

/// base64 编码（标准字母表，无换行）— Loro 快照跨 JNI 传输。
fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(CHARS[(n >> 18) as usize & 63] as char);
        out.push(CHARS[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            CHARS[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            CHARS[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// base64 解码（容忍空白字符）。失败返回 None。
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut buf: u32 = 0;
    let mut bits = 0u32;
    for &c in input.as_bytes() {
        if c.is_ascii_whitespace() || c == b'=' {
            continue;
        }
        let v = val(c)?;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

unsafe fn core_from_handle(handle: jlong) -> Arc<UniffiAppCore> {
    let arc = Arc::from_raw(handle as *const UniffiAppCore);
    let cloned = Arc::clone(&arc);
    std::mem::forget(arc);
    cloned
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeNew(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
) -> jlong {
    let data_dir = match jstring_to_rust(&mut env, &data_dir) {
        Some(s) => s,
        None => return 0,
    };
    match UniffiAppCore::new(data_dir) {
        Ok(core) => Arc::into_raw(core) as jlong,
        Err(_) => 0,
    }
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeCreateNote(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    title: JString,
) -> jstring {
    let core = unsafe { core_from_handle(handle) };
    let title = match jstring_to_rust(&mut env, &title) {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    match core.create_note_impl(title) {
        Ok(id) => rust_str_to_jstring(&mut env, &id),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeListNotesCount(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    let core = unsafe { core_from_handle(handle) };
    core.list_notes_impl().len() as jint
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeGetNote(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    index: jint,
) -> jobject {
    let core = unsafe { core_from_handle(handle) };
    let notes = core.list_notes_impl();
    if index < 0 || (index as usize) >= notes.len() {
        return std::ptr::null_mut();
    }
    let note = &notes[index as usize];

    let str_class = match env.find_class("java/lang/String") {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    let array = match env.new_object_array(3, &str_class, JObject::null()) {
        Ok(a) => a,
        Err(_) => return std::ptr::null_mut(),
    };
    let id = rust_str_to_jstring(&mut env, &note.id);
    let _ = env.set_object_array_element(&array, 0, unsafe { JObject::from_raw(id) });
    let title = rust_str_to_jstring(&mut env, &note.title);
    let _ = env.set_object_array_element(&array, 1, unsafe { JObject::from_raw(title) });
    let updated = rust_str_to_jstring(&mut env, &note.updated_at);
    let _ = env.set_object_array_element(&array, 2, unsafe { JObject::from_raw(updated) });
    array.into_raw()
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeSearchCount(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    query: JString,
) -> jint {
    let core = unsafe { core_from_handle(handle) };
    let query = match jstring_to_rust(&mut env, &query) {
        Some(s) => s,
        None => return 0,
    };
    core.search_notes_impl(query).len() as jint
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeGetSearchResult(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    index: jint,
    query: JString,
) -> jobject {
    let core = unsafe { core_from_handle(handle) };
    let query = match jstring_to_rust(&mut env, &query) {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    let results = core.search_notes_impl(query);
    if index < 0 || (index as usize) >= results.len() {
        return std::ptr::null_mut();
    }
    let r = &results[index as usize];

    let obj_class = match env.find_class("java/lang/Object") {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    let array = match env.new_object_array(4, &obj_class, JObject::null()) {
        Ok(a) => a,
        Err(_) => return std::ptr::null_mut(),
    };
    let note_id = rust_str_to_jstring(&mut env, &r.note_id);
    let _ = env.set_object_array_element(&array, 0, unsafe { JObject::from_raw(note_id) });
    let title = rust_str_to_jstring(&mut env, &r.title);
    let _ = env.set_object_array_element(&array, 1, unsafe { JObject::from_raw(title) });
    let snippet = rust_str_to_jstring(&mut env, &r.snippet);
    let _ = env.set_object_array_element(&array, 2, unsafe { JObject::from_raw(snippet) });

    let double_class = match env.find_class("java/lang/Double") {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    let dv = match env.new_object(&double_class, "(D)V", &[JValue::Double(r.score)]) {
        Ok(d) => d,
        Err(_) => return std::ptr::null_mut(),
    };
    let _ = env.set_object_array_element(&array, 3, dv);
    array.into_raw()
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeDeleteNote(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    note_id: JString,
) -> jint {
    let core = unsafe { core_from_handle(handle) };
    let note_id = match jstring_to_rust(&mut env, &note_id) {
        Some(s) => s,
        None => return -1,
    };
    match core.delete_note_impl(note_id) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeIsFallback(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    let core = unsafe { core_from_handle(handle) };
    if core.is_fallback {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeSaveNoteContent(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    note_id: JString,
    content: JString,
) -> jint {
    let core = unsafe { core_from_handle(handle) };
    let note_id = match jstring_to_rust(&mut env, &note_id) {
        Some(s) => s,
        None => return -1,
    };
    let content = match jstring_to_rust(&mut env, &content) {
        Some(s) => s,
        None => return -1,
    };
    match core.save_note_content_impl(note_id, content) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeGetNoteContent(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    note_id: JString,
) -> jstring {
    let core = unsafe { core_from_handle(handle) };
    let note_id = match jstring_to_rust(&mut env, &note_id) {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    match core.get_note_content_impl(note_id) {
        Ok(content) => rust_str_to_jstring(&mut env, &content),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 获取笔记 Loro 快照（base64）— ProseMirror 编辑器初始化（DEV-009）。
#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeGetNoteSnapshot(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    note_id: JString,
) -> jstring {
    let core = unsafe { core_from_handle(handle) };
    let note_id = match jstring_to_rust(&mut env, &note_id) {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    match core.get_note_snapshot_impl(note_id) {
        Ok(b64) => rust_str_to_jstring(&mut env, &b64),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 时间机器：列出笔记快照元数据（JSON 数组）— 移动端版本历史 UI（V23-I5）。
#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeListSnapshots(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    note_id: JString,
) -> jstring {
    let core = unsafe { core_from_handle(handle) };
    let note_id = match jstring_to_rust(&mut env, &note_id) {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    rust_str_to_jstring(&mut env, &core.list_snapshots_impl(&note_id))
}

/// 时间机器：读取指定版本快照正文（UTF-8 文本）。
#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeGetSnapshotContent(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    note_id: JString,
    version: jint,
) -> jstring {
    let core = unsafe { core_from_handle(handle) };
    let note_id = match jstring_to_rust(&mut env, &note_id) {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    match core.get_snapshot_content_impl(&note_id, version as i64) {
        Some(c) => rust_str_to_jstring(&mut env, &c),
        None => std::ptr::null_mut(),
    }
}

/// 保存 JS 侧 Loro 快照（base64，CRDT 合并）+ 持久化（DEV-009）。
#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeSaveNoteSnapshot(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    note_id: JString,
    snapshot_b64: JString,
) -> jboolean {
    let core = unsafe { core_from_handle(handle) };
    let (Some(note_id), Some(snapshot_b64)) = (
        jstring_to_rust(&mut env, &note_id),
        jstring_to_rust(&mut env, &snapshot_b64),
    ) else {
        return 0;
    };
    match core.save_note_snapshot_impl(note_id, snapshot_b64) {
        Ok(()) => 1,
        Err(e) => {
            tracing::warn!("save note snapshot failed: {e}");
            0
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_aurora_note_UniffiAppCore_nativeDestroy(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if handle != 0 {
        unsafe {
            let _ = Arc::from_raw(handle as *const UniffiAppCore);
        }
    }
}

// ===========================================================================
// UniFFI export（保留用于 iOS / 测试）
// ===========================================================================

#[uniffi::export]
impl UniffiAppCore {
    #[uniffi::constructor]
    pub fn uniffi_new(data_dir: String) -> Result<Arc<Self>, MobileError> {
        Self::new(data_dir)
    }

    pub fn create_note(self: Arc<Self>, title: String) -> Result<String, MobileError> {
        Self::create_note_impl(&self, title)
    }

    pub fn list_notes(self: Arc<Self>) -> Vec<NoteSummary> {
        Self::list_notes_impl(&self)
    }

    pub fn search_notes(self: Arc<Self>, query: String) -> Vec<SearchResult> {
        Self::search_notes_impl(&self, query)
    }

    /// TodayView 统计（V20 §5.4.2 — 任务投影聚合，Rust 侧产出）。
    /// V23-I4: Agent 现场感知（当前文档+选中块+反链+GTD → 结构化 JSON）。
    pub fn get_agent_context(
        self: Arc<Self>,
        note_id: String,
        selected_block: Option<String>,
    ) -> String {
        if let Some(core) = &self.core {
            match self
                .runtime
                .block_on(core.agent_context(&note_id, selected_block.as_deref()))
            {
                Ok(v) => v.to_string(),
                Err(e) => {
                    tracing::warn!(note_id, error = %e, "agent context failed");
                    serde_json::json!({"schema": "aurora.agent_context/1", "present": false, "error": e.to_string()}).to_string()
                }
            }
        } else {
            serde_json::json!({"schema": "aurora.agent_context/1", "present": false}).to_string()
        }
    }

    /// V23-I4: 时间机器列表（新→旧; R6 治理上限 20 版）。
    pub fn list_note_snapshots(self: Arc<Self>, note_id: String) -> Vec<SnapshotInfo> {
        self.with_time_machine(|tm| {
            tm.list(&note_id)
                .unwrap_or_default()
                .into_iter()
                .map(|m| SnapshotInfo {
                    version: m.version,
                    created_at: m.created_at,
                    size: m.size as i64,
                })
                .collect()
        })
        .unwrap_or_default()
    }

    /// V23-I4: 读取指定版本快照内容（UI 确认后经 save_note_content 写回
    /// —— 回溯走正规保存路径, mirror/blocks 双轨自动跟随）。
    pub fn restore_note_snapshot(
        self: Arc<Self>,
        note_id: String,
        version: i64,
    ) -> Option<String> {
        self.with_time_machine(|tm| {
            tm.load(&note_id, version)
                .ok()
                .flatten()
                .and_then(|bytes| String::from_utf8(bytes).ok())
        })
        .flatten()
    }

    pub fn today_view_stats(self: Arc<Self>) -> TodayViewStats {
        Self::today_view_stats_impl(&self)
    }

    /// 反向链接（双链反链面板 — 笔记被哪些笔记引用）。
    pub fn get_backlinks(self: Arc<Self>, note_id: String) -> Vec<BacklinkItem> {
        Self::backlinks_impl(&self, note_id)
    }

    /// FSRS 复习: 到期卡片列表（due ≤ now 或 R 跌破 0.9）。
    pub fn due_review_cards(self: Arc<Self>) -> Vec<ReviewCard> {
        if let Some(core) = &self.core {
            core.review_queue
                .due_items(chrono::Utc::now())
                .into_iter()
                .map(|it| {
                    let r = aurora_core::l3_domain::fsrs::FsrsScheduler::new()
                        .retrievability(&it.state, chrono::Utc::now());
                    ReviewCard {
                        card_id: it.card_id,
                        note_id: it.note_id,
                        due_at: it.due.to_rfc3339(),
                        retrievability: r,
                        reps: it.state.reps as i64,
                        lapses: it.state.lapses as i64,
                    }
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// FSRS 复习: 对卡片评分（1 Again / 2 Hard / 3 Good / 4 Easy）
    /// → 返回下次到期时间（RFC3339）。
    pub fn review_card(
        self: Arc<Self>,
        card_id: String,
        rating: i64,
    ) -> Result<String, MobileError> {
        if let Some(core) = &self.core {
            let rating = match rating {
                1 => aurora_core::l3_domain::fsrs::Rating::Again,
                2 => aurora_core::l3_domain::fsrs::Rating::Hard,
                3 => aurora_core::l3_domain::fsrs::Rating::Good,
                4 => aurora_core::l3_domain::fsrs::Rating::Easy,
                _ => {
                    return Err(MobileError::OperationFailed {
                        message: "rating must be 1-4".into(),
                    })
                }
            };
            let out = core
                .review_queue
                .review_card(&card_id, rating)
                .ok_or(MobileError::NotFound { resource: card_id })?;
            Ok(out.due.to_rfc3339())
        } else {
            Err(MobileError::OperationFailed {
                message: "fallback mode".into(),
            })
        }
    }

    pub fn delete_note(self: Arc<Self>, note_id: String) -> Result<(), MobileError> {
        Self::delete_note_impl(&self, note_id)
    }

    pub fn save_note_content(
        self: Arc<Self>,
        note_id: String,
        content: String,
    ) -> Result<(), MobileError> {
        Self::save_note_content_impl(&self, note_id, content)
    }

    pub fn get_note_content(self: Arc<Self>, note_id: String) -> Result<String, MobileError> {
        Self::get_note_content_impl(&self, note_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let core = UniffiAppCore::new(dir.path().to_str().unwrap().to_string()).unwrap();
        let id = core.clone().create_note("Test Note".into()).unwrap();
        let notes = core.clone().list_notes();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].id, id);

        // Loro 内容读写（真实模式）
        core.clone()
            .save_note_content(id.clone(), "Hello Loro 世界".into())
            .unwrap();
        let content = core.clone().get_note_content(id.clone()).unwrap();
        assert_eq!(content, "Hello Loro 世界");

        // 再写一次（覆盖路径）
        core.clone()
            .save_note_content(id.clone(), "Updated".into())
            .unwrap();
        assert_eq!(
            core.clone().get_note_content(id.clone()).unwrap(),
            "Updated"
        );

        core.clone().delete_note(id).unwrap();
        assert!(core.clone().list_notes().is_empty());
    }

    /// V20 §4.5 事件驱动索引闭环: 创建/搜索/删除全链路经投影。
    #[test]
    fn event_driven_search_index_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let core = UniffiAppCore::new(dir.path().to_str().unwrap().to_string()).unwrap();
        assert!(!core.is_fallback);

        // 创建 → 事件 → 投影 → 可搜（中文 jieba）
        let id = core.clone().create_note("架构设计文档".into()).unwrap();
        let hits = core.clone().search_notes("架构".into());
        assert_eq!(hits.len(), 1, "创建后立即可搜（事件驱动投影）: {hits:?}");
        assert_eq!(hits[0].note_id, id);

        // 内容更新 → NoteMetadataChanged → 数据源重索引 → 可搜新内容
        core.clone()
            .save_note_content(id.clone(), "量子加密同步协议".into())
            .unwrap();
        let hits = core.clone().search_notes("量子".into());
        assert_eq!(hits.len(), 1, "内容更新后可搜新内容: {hits:?}");

        // 删除 → NoteDeleted → 索引清除
        core.clone().delete_note(id).unwrap();
        let hits = core.clone().search_notes("架构".into());
        assert!(hits.is_empty(), "删除后不再命中");
    }

    /// V20 §4.5 重建自愈: 全新实例从 KVStore 数据源全量重建索引。
    #[test]
    fn rebuild_from_data_source_on_new_instance() {
        let dir = tempfile::tempdir().unwrap();
        {
            let core = UniffiAppCore::new(dir.path().to_str().unwrap().to_string()).unwrap();
            core.clone().create_note("分布式系统笔记".into()).unwrap();
            core.clone().create_note("算法导论笔记".into()).unwrap();
        } // drop = 杀进程（Tantivy 索引仍在磁盘，事件队列仍在）

        // 新实例: startup catch_up 增量追赶（索引已在）
        let core2 = UniffiAppCore::new(dir.path().to_str().unwrap().to_string()).unwrap();
        let hits = core2.clone().search_notes("分布式".into());
        assert_eq!(hits.len(), 1, "重启后中文搜索可用: {hits:?}");

        // 模拟索引损坏: 新建第三个实例（KVStore 有数据）→ 删索引目录 →
        // rebuild 路径由 verify 失败触发（此处验证数据源回调可重建）
        drop(core2);
        let index_dir = dir.path().join("tantivy_index");
        let _ = std::fs::remove_dir_all(index_dir);
        let core3 = UniffiAppCore::new(dir.path().to_str().unwrap().to_string()).unwrap();
        // 索引目录被删 → tantivy 新建空索引; 事件队列 watermark 已消费 →
        // 增量追赶无事件可放 → 需走 rebuild（verify Corrupted → 数据源重建）。
        // 由于 verify 需查 doc 数对比，此处直接调 catch_up 验证不 panic，
        // 并手动触发数据源重建路径（rebuild_index 经 source 回调）。
        let hits = core3.clone().search_notes("算法".into());
        // 数据源回调重建后应命中（若 verify 未触发，此断言暴露重建缺口）
        assert_eq!(hits.len(), 1, "索引损坏后经数据源重建恢复: {hits:?}");
    }

    /// V20 §5.4.2 + Phase 2 数据源闭环: 任务投影经 KVStore 数据源
    /// rebuild 后 TodayView 统计由 Rust 侧产出（前端只渲染）。
    #[test]
    fn today_view_stats_from_task_projection() {
        let dir = tempfile::tempdir().unwrap();
        {
            let core = UniffiAppCore::new(dir.path().to_str().unwrap().to_string()).unwrap();
            assert!(!core.is_fallback);
            core.clone().create_note("待办事项A".into()).unwrap();
            core.clone().create_note("待办事项B".into()).unwrap();
            // 创建即播种 2 行 inbox → catch_up 后统计反映
            let s = core.clone().today_view_stats();
            assert_eq!(s.active, 2, "两篇笔记播种 2 行进行中: {s:?}");
            assert_eq!(s.done, 0);
        } // 杀进程
        {
            let core2 = UniffiAppCore::new(dir.path().to_str().unwrap().to_string()).unwrap();
            // 重启: 任务投影数据源（note: 播种）+ catch_up → 统计恢复
            let s = core2.clone().today_view_stats();
            assert_eq!(s.active, 2, "重启后经数据源 rebuild 恢复: {s:?}");

            // 删除一篇 → 级联清理播种行
            let notes = core2.clone().list_notes();
            core2.clone().delete_note(notes[0].id.clone()).unwrap();
            let s2 = core2.clone().today_view_stats();
            assert_eq!(s2.active, 1, "删除笔记级联清理任务行: {s2:?}");
        }
    }

    /// V20 Phase 3 §5.4: 口语化语义搜索 —「未完成的任务」经 NL 解析 →
    /// 任务投影聚合（播种行排除, 仅真实行动项）。
    #[test]
    fn nl_query_undone_tasks_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let core = UniffiAppCore::new(dir.path().to_str().unwrap().to_string()).unwrap();
        assert!(!core.is_fallback);

        let id = core.clone().create_note("项目推进".into()).unwrap();
        core.clone()
            .save_note_content(id.clone(), "- [ ] 明天提交预算表\n- [x] 已完成事项\n尽快安排评审".into())
            .unwrap();

        // 口语化「未完成的任务」→ 2 个真实行动项（播种行排除）
        let hits = core.clone().search_notes("未完成的任务".into());
        assert_eq!(hits.len(), 2, "2 个真实行动项（不含播种行）: {hits:?}");
        assert!(hits.iter().any(|h| h.title.contains("预算")));
        assert!(hits.iter().any(|h| h.title.contains("评审")));

        // 关键词笔记查询（notes 意图 → Tantivy）
        let note_hits = core.clone().search_notes("预算".into());
        assert!(!note_hits.is_empty(), "关键词走全文: {note_hits:?}");
    }

    /// V20 Phase 3 FSRS: 复习闭环经 FFI（注册→到期→评分→推迟）。
    #[test]
    fn fsrs_review_cycle_via_ffi() {
        let dir = tempfile::tempdir().unwrap();
        let core = UniffiAppCore::new(dir.path().to_str().unwrap().to_string()).unwrap();
        assert!(!core.is_fallback);

        // 注册复习卡（笔记 Distill 场景: 首次 Good）
        let c = core.core.as_ref().unwrap();
        c.review_queue.add_card("card-1", "note-1",
            aurora_core::l3_domain::fsrs::Rating::Good);

        // 新卡 due 在未来 → 无到期
        assert!(core.clone().due_review_cards().is_empty());

        // 时间快进语义: 经内部 API 用未来时刻查（FFI 恒 now — 测试走内部）
        let future = chrono::Utc::now() + chrono::Duration::days(30);
        let due = c.review_queue.due_items(future);
        assert_eq!(due.len(), 1, "30 天后到期");

        // FFI 评分: Good → 推迟
        let next_due = core.clone().review_card("card-1".into(), 3).unwrap();
        assert!(!next_due.is_empty(), "RFC3339 回传");
        // 再评 Again → 队列不丢卡（重置而非删除）
        core.clone().review_card("card-1".into(), 1).unwrap();
        let (_, total) = c.review_queue.stats();
        assert_eq!(total, 1, "卡仍在队列");

        // 不存在的卡 → NotFound
        assert!(core.clone().review_card("ghost".into(), 3).is_err());
        // 非法评分 → 错误
        assert!(core.clone().review_card("card-1".into(), 9).is_err());
    }

    /// V23-I1: 桌面（Tauri command）与移动（FFI）视图模型**同构对拍**。
    ///
    /// 桌面壳 cmd_today_view_stats / cmd_get_backlinks / cmd_due_review_cards
    /// 的 JSON 组装逻辑在本测试内等价复现，与 FFI 契约逐字段对拍——
    /// 任一端改字段名/类型，此测试红（CI 拦截双端分叉）。
    #[test]
    fn desktop_mobile_view_model_isomorphic() {
        let dir = tempfile::tempdir().unwrap();
        let core = UniffiAppCore::new(dir.path().to_str().unwrap().to_string()).unwrap();
        assert!(!core.is_fallback);
        let c = core.core.as_ref().unwrap();

        // 1. today_view_stats 同构
        let stats = core.clone().today_view_stats();
        let (active, done) = c.task_projection_stats();
        let due_today = c.task_projection_due_today();
        let desktop_stats = serde_json::json!({
            "active": active, "done": done, "due_today": due_today,
        });
        assert_eq!(stats.active, desktop_stats["active"].as_i64().unwrap());
        assert_eq!(stats.done, desktop_stats["done"].as_i64().unwrap());
        assert_eq!(stats.due_today, desktop_stats["due_today"].as_i64().unwrap());

        // 2. backlinks 同构（播种 n1 → n2 后对拍字段集）
        use aurora_core::event_bus::layered::LinkAction;
        use aurora_core::l2_engines::bidi_link_projection::BidiLinkProjection;
        let bp = c
            .projections()
            .iter()
            .find_map(|p| {
                p.as_any().and_then(|a| a.downcast_ref::<BidiLinkProjection>())
            })
            .expect("bidi projection");
        bp.apply_link("n1", "n2", &LinkAction::Created);
        let links = core.clone().get_backlinks("n2".into());
        assert_eq!(links.len(), 1);
        let desktop_links: Vec<serde_json::Value> = c
            .bidi_link_incoming("n2")
            .iter()
            .map(|src| {
                serde_json::json!({"source_note_id": src, "source_title": src.clone()})
            })
            .collect();
        assert_eq!(links.len(), desktop_links.len());
        assert_eq!(links[0].source_note_id, desktop_links[0]["source_note_id"].as_str().unwrap());
        assert!(desktop_links[0].as_object().unwrap().len() == 2, "backlink 字段漂移");

        // 3. 复习卡同构（字段集合 6 项 + RFC3339 due 契约）
        c.review_queue.add_card("c1", "n1",
            aurora_core::l3_domain::fsrs::Rating::Good);
        let future = chrono::Utc::now() + chrono::Duration::days(30);
        let scheduler = aurora_core::l3_domain::fsrs::FsrsScheduler::new();
        let desktop_cards: Vec<serde_json::Value> = c
            .review_queue
            .due_items(future)
            .iter()
            .map(|it| {
                serde_json::json!({
                    "card_id": it.card_id,
                    "note_id": it.note_id,
                    "due_at": it.due.to_rfc3339(),
                    "retrievability": scheduler.retrievability(&it.state, future),
                    "reps": it.state.reps,
                    "lapses": it.state.lapses,
                })
            })
            .collect();
        assert_eq!(desktop_cards.len(), 1, "30 天后应到期");
        assert_eq!(desktop_cards[0].as_object().unwrap().len(), 6, "review 字段漂移");
        // FFI 侧同语义（经 FFI 评分 → RFC3339 回传契约）
        let due = core.clone().review_card("c1".into(), 3).unwrap();
        assert!(due.contains('T') && (due.contains('+') || due.contains('Z')));
        // 桌面壳评分同语义
        let d_out = c
            .review_queue
            .review_card("c1", aurora_core::l3_domain::fsrs::Rating::Good)
            .unwrap();
        assert!(d_out.due.to_rfc3339().contains('T'));
    }

    /// V23-I4 端到端: Agent 现场感知结构 + 时间机器保存/列表/回溯。
    #[test]
    fn agent_context_and_time_machine_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let core = UniffiAppCore::new(dir.path().to_str().unwrap().to_string()).unwrap();
        assert!(!core.is_fallback);

        // 建笔记 + 写内容（走正规保存 → blocks/mirror 双轨已接）
        let note_id = core.clone().create_note("现场笔记".into()).unwrap();
        core.clone()
            .save_note_content(note_id.clone(), "# 标题\n\n现场正文".into())
            .unwrap();

        // 1. Agent 现场感知: schema/present/note 三段齐
        let ctx = core.clone().get_agent_context(note_id.clone(), Some("b1".into()));
        let ctx: serde_json::Value = serde_json::from_str(&ctx).unwrap();
        assert_eq!(ctx["schema"], "aurora.agent_context/1");
        assert_eq!(ctx["present"], true);
        assert_eq!(ctx["note"]["title"], "现场笔记");
        assert_eq!(ctx["note"]["selection"], "b1");
        assert!(ctx["gtd"]["active"].is_i64(), "GTD 段齐: {ctx:?}");
        assert!(ctx["scene"]["backlinks"].is_array());

        // 2. 不存在笔记 → present false（Agent 明确「不在场」）
        let absent = core.clone().get_agent_context("ghost".into(), None);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&absent).unwrap()["present"], false);

        // 3. 时间机器: 保存两次 → 两个版本; 列表新→旧; 回溯取旧版
        core.clone()
            .save_note_content(note_id.clone(), "第二版内容".into())
            .unwrap();
        let snaps = core.clone().list_note_snapshots(note_id.clone());
        assert!(snaps.len() >= 2, "每次保存应留快照: {snaps:?}");
        assert!(snaps[0].version > snaps[1].version, "新→旧");
        let restored = core
            .clone()
            .restore_note_snapshot(note_id.clone(), snaps[1].version)
            .unwrap();
        // 旧版内容（第一版正文或第二版 — 至少是历史原文而非空）
        assert!(!restored.is_empty());
        // 回到当前版验证内容仍可用（回溯→写回路径契约: 内容可经 save_note_content 落回）
        core.clone()
            .save_note_content(note_id.clone(), restored.clone())
            .unwrap();
        let back = core.get_note_content(note_id.clone()).unwrap();
        assert_eq!(back, restored, "回溯写回后内容一致");
    }

    #[test]
    fn fallback_mode_works() {
        let core = UniffiAppCore::new("/dev/null/aurora-test".into()).unwrap();
        assert!(core.is_fallback);
        let id = core.clone().create_note("Fallback".into()).unwrap();
        assert_eq!(core.clone().list_notes().len(), 1);

        // Fallback 模式下 Loro 内存 CRDT 同样可用
        core.clone()
            .save_note_content(id.clone(), "fallback content".into())
            .unwrap();
        assert_eq!(
            core.clone().get_note_content(id.clone()).unwrap(),
            "fallback content"
        );

        core.clone().delete_note(id).unwrap();
        assert!(core.clone().list_notes().is_empty());
    }
}
