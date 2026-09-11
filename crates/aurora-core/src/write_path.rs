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
//! - `notesnap:{id}` — Loro 快照字节（内容权威，五容器模型）。
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

use crate::app_core::AppCore;
use crate::blocks::BlockStore;
use crate::error_codes::ErrorCode;
use crate::Error;
use serde::{Deserialize, Serialize};
use tracing::info;

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
pub struct SealPair {
    pub seal: Box<dyn Fn(&[u8]) -> Result<Vec<u8>, Error> + Send + Sync>,
    pub unseal: Box<dyn Fn(&[u8]) -> Result<Vec<u8>, Error> + Send + Sync>,
}

/// 写入上下文 — 唯一写入入口的依赖集合（AppCore + blocks 双轨 + 加密封装）。
pub struct WriteContext {
    pub core: std::sync::Arc<AppCore>,
    /// blocks 双轨存储（None = 内存降级模式，跳过块派生 — 与移动端降级语义一致）。
    pub blocks: Option<std::sync::Arc<BlockStore>>,
    /// 落盘封装（桌面 Some(vault pair)，移动 None）。
    pub seal: Option<SealPair>,
}

/// 笔记元数据记录（两端统一格式 — 从 mobile-ffi 上移，权威定义于此）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteRecord {
    pub id: String,
    pub title: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
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

/// 读取笔记元数据（seal/unseal 对称解封装；不存在返回 Ok(None)）。
pub async fn load_note_meta(
    core: &AppCore,
    note_id: &str,
    unseal: Option<&SealPair>,
) -> Result<Option<NoteRecord>, Error> {
    let bytes = match core.kv_store.get(&format!("note:{note_id}")).await? {
        Some(b) => b,
        None => return Ok(None),
    };
    let plain = match unseal {
        Some(pair) => (pair.unseal)(&bytes)?,
        None => bytes,
    };
    Ok(Some(serde_json::from_slice(&plain)?))
}

/// 加载（或创建）笔记的 Loro 文档：优先从 `notesnap:{id}` 快照恢复。
#[cfg(feature = "loro-crdt")]
async fn load_or_init_doc(core: &AppCore, note_id: &str) -> Result<NoteDoc, Error> {
    match core.kv_store.get(&format!("notesnap:{note_id}")).await? {
        Some(bytes) if !bytes.is_empty() => NoteDoc::from_snapshot(&bytes),
        _ => {
            // 无快照（新建或迁移中）：由调用方 init 后再 touch —— 这里给出空文档由调用方补 meta。
            NoteDoc::new("", "")
        }
    }
}

#[cfg(feature = "loro-crdt")]
async fn persist_doc(core: &AppCore, note_id: &str, doc: &NoteDoc) -> Result<(), Error> {
    let snapshot = doc.export_snapshot()?;
    core.kv_store
        .set(&format!("notesnap:{note_id}"), &snapshot)
        .await
}

// ===== 写入操作（唯一入口）=====

/// 创建笔记（load→apply→原子保存→派生 blocks→发事件 全流程）。
///
/// 签名以 `&WriteContext` 注入依赖；返回笔记 ID（创建后可查 `WriteReceipt` 语义见各写方法）。
#[cfg(feature = "loro-crdt")]
pub async fn create_note(ctx: &WriteContext, title: &str) -> Result<String, Error> {
    if title.trim().is_empty() {
        return Err(Error::Domain {
            code: ErrorCode::A04,
            message: ErrorCode::A04.user_message(),
        });
    }
    let core = ctx.core.as_ref();
    let id = uuid::Uuid::new_v4().to_string();
    let now_ms = chrono::Utc::now().timestamp_millis();

    // 1) Loro 五容器文档（内容权威源）
    let doc = NoteDoc::new(title, "")?;
    doc.set_timestamps(now_ms, now_ms)?;
    let ws = doc.meta().workspace_id.clone();

    // 2) 原子保存（WAL：快照先落，元数据后落为权威指针）
    let record = NoteRecord::new(id.clone(), title.to_string());
    persist_doc(core, &id, &doc).await?;
    put_note_meta(core, &id, &record, ctx.seal.as_ref()).await?;

    // 3) blocks 派生（内容 → 块树）
    if let Some(blocks) = ctx.blocks.as_ref() {
        blocks.sync_note_blocks(&id, Some(&ws), &doc.body())?;
    }

    // 4) 事件（Medium 持久化 → 投影事件驱动；publish 后 seq 即本次事件序号）
    core.event_bus
        .publish(crate::event_bus::layered::AppEvent::NoteCreated {
            note_id: id.clone(),
            title: title.to_string(),
            content: doc.body(),
        });

    info!(note_id = %id, "note created via WritePath (load→apply→atomic→blocks→events)");
    Ok(id)
}

/// 创建笔记 — 无 Loro 降级模式（仅元数据 + 事件，快照缺席由消费方容忍）。
#[cfg(not(feature = "loro-crdt"))]
pub async fn create_note(ctx: &WriteContext, title: &str) -> Result<String, Error> {
    if title.trim().is_empty() {
        return Err(Error::Domain {
            code: ErrorCode::A04,
            message: ErrorCode::A04.user_message(),
        });
    }
    let core = ctx.core.as_ref();
    let id = uuid::Uuid::new_v4().to_string();
    let record = NoteRecord::new(id.clone(), title.to_string());
    put_note_meta(core, &id, &record, ctx.seal.as_ref()).await?;
    core.event_bus
        .publish(crate::event_bus::layered::AppEvent::NoteCreated {
            note_id: id.clone(),
            title: title.to_string(),
            content: String::new(),
        });
    info!(note_id = %id, "note created via WritePath (no-loro fallback)");
    Ok(id)
}

/// 保存笔记正文内容。
#[cfg(feature = "loro-crdt")]
pub async fn save_note_content(
    ctx: &WriteContext,
    note_id: &str,
    content: &str,
) -> Result<WriteReceipt, Error> {
    let core = ctx.core.as_ref();
    let unseal = ctx.seal.as_ref();
    let mut record = load_note_meta(core, note_id, unseal)
        .await?
        .ok_or(Error::NoteNotFound {
            id: note_id.to_string(),
        })?;
    let now_ms = chrono::Utc::now().timestamp_millis();

    // 1) Loro 文档：快照恢复或新建后补时间戳
    let doc = load_or_init_doc(core, note_id).await?;
    doc.set_body(content, now_ms)?;
    let ws = doc.meta().workspace_id.clone();

    // 2) 原子保存（快照 → 元数据）
    record.content = content.to_string();
    record.updated_at = chrono::Utc::now().to_rfc3339();
    persist_doc(core, note_id, &doc).await?;
    put_note_meta(core, note_id, &record, ctx.seal.as_ref()).await?;

    // 3) blocks 派生
    if let Some(blocks) = ctx.blocks.as_ref() {
        blocks.sync_note_blocks(note_id, Some(&ws), content)?;
    }

    // 4) 事件（High 实时 + seq 回执）
    core.event_bus
        .publish(crate::event_bus::layered::AppEvent::NoteContentChanged {
            note_id: note_id.to_string(),
            block_id: None,
        });

    Ok(WriteReceipt {
        seq: core.event_bus.last_seq(),
        aggregate_id: note_id.to_string(),
        committed_at: now_ms,
    })
}

/// 保存笔记正文 — 无 Loro 降级模式（仅元数据 + 事件）。
#[cfg(not(feature = "loro-crdt"))]
pub async fn save_note_content(
    ctx: &WriteContext,
    note_id: &str,
    content: &str,
) -> Result<WriteReceipt, Error> {
    let core = ctx.core.as_ref();
    let unseal = ctx.seal.as_ref();
    let mut record = load_note_meta(core, note_id, unseal)
        .await?
        .ok_or(Error::NoteNotFound {
            id: note_id.to_string(),
        })?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    record.content = content.to_string();
    record.updated_at = chrono::Utc::now().to_rfc3339();
    put_note_meta(core, note_id, &record, ctx.seal.as_ref()).await?;
    core.event_bus
        .publish(crate::event_bus::layered::AppEvent::NoteContentChanged {
            note_id: note_id.to_string(),
            block_id: None,
        });
    Ok(WriteReceipt {
        seq: core.event_bus.last_seq(),
        aggregate_id: note_id.to_string(),
        committed_at: now_ms,
    })
}

/// 重命名笔记（NoteMetadataChanged{title} → 双链/搜索投影标题更新）。
pub async fn rename_note(
    ctx: &WriteContext,
    note_id: &str,
    new_title: &str,
) -> Result<WriteReceipt, Error> {
    let core = ctx.core.as_ref();
    let unseal = ctx.seal.as_ref();
    let mut record = load_note_meta(core, note_id, unseal)
        .await?
        .ok_or(Error::NoteNotFound {
            id: note_id.to_string(),
        })?;
    let old_title = record.title.clone();
    let now_ms = chrono::Utc::now().timestamp_millis();

    #[cfg(feature = "loro-crdt")]
    {
        let doc = load_or_init_doc(core, note_id).await?;
        doc.set_title(new_title, now_ms)?;
        persist_doc(core, note_id, &doc).await?;
    }

    record.title = new_title.to_string();
    record.updated_at = chrono::Utc::now().to_rfc3339();
    put_note_meta(core, note_id, &record, ctx.seal.as_ref()).await?;

    core.event_bus
        .publish(crate::event_bus::layered::AppEvent::NoteMetadataChanged {
            note_id: note_id.to_string(),
            changes: crate::event_bus::layered::NoteChanges {
                title: Some(new_title.to_string()),
                tags: None,
            },
        });
    info!(note_id, old_title, new_title, "note renamed via WritePath");
    Ok(WriteReceipt {
        seq: core.event_bus.last_seq(),
        aggregate_id: note_id.to_string(),
        committed_at: now_ms,
    })
}

/// 软删除笔记（快照与元数据同删 + NoteDeleted 事件驱动投影清理）。
pub async fn delete_note(ctx: &WriteContext, note_id: &str) -> Result<WriteReceipt, Error> {
    let core = ctx.core.as_ref();
    let unseal = ctx.seal.as_ref();
    // 存在性校验（读元数据）
    load_note_meta(core, note_id, unseal)
        .await?
        .ok_or(Error::NoteNotFound {
            id: note_id.to_string(),
        })?;

    core.kv_store.delete(&format!("note:{note_id}")).await?;
    core.kv_store.delete(&format!("notesnap:{note_id}")).await?;

    let now_ms = chrono::Utc::now().timestamp_millis();
    core.event_bus
        .publish(crate::event_bus::layered::AppEvent::NoteDeleted {
            note_id: note_id.to_string(),
        });
    info!(note_id, "note deleted via WritePath");
    Ok(WriteReceipt {
        seq: core.event_bus.last_seq(),
        aggregate_id: note_id.to_string(),
        committed_at: now_ms,
    })
}
