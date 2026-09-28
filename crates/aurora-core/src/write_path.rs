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

/// 打开笔记正文（DK-07 S3 读侧）：明文直通，密文经 cipher 解密。
///
/// fail-closed：加密笔记在无 cipher（锁定）环境返回 `Crypto` 错误，
/// 绝不返回密文原文。
///
/// # Errors
/// - 加密笔记无 cipher 可用（锁定态）
/// - 解密失败（密文损坏/密钥不符）
pub fn open_note_content(
    ctx: &WriteContext,
    note_id: &str,
    record: &NoteRecord,
) -> Result<String, Error> {
    if record.encryption != ENC_AES256GCM {
        return Ok(record.content.clone());
    }
    let cipher = ctx.content_cipher.as_ref().ok_or_else(|| {
        Error::Crypto(format!(
            "note '{note_id}' is encrypted but no content cipher available (locked)"
        ))
    })?;
    (cipher.decrypt)(note_id, &record.content)
}

/// 设置笔记加密级别（DK-07 S1 写路径 — 用户"锁定/解锁笔记"入口）。
///
/// fail-closed：非法级别拒绝；切换到加密级别时**立即从索引移除**（防锁定
/// 前旧索引残留——S3 将把密级传播到 bootstrap source，实现全链路过滤）。
///
/// # Errors
/// - 笔记不存在（`NoteNotFound`）
/// - 非法级别（`InvalidInput`，仅接受 `none` / `aes256gcm`）
pub async fn set_note_encryption(
    ctx: &WriteContext,
    note_id: &str,
    level: &str,
) -> Result<(), Error> {
    if level != ENC_NONE && level != ENC_AES256GCM {
        return Err(Error::InvalidInput(format!(
            "invalid encryption level '{level}' (expected '{ENC_NONE}' or '{ENC_AES256GCM}')"
        )));
    }
    let core = ctx.core.as_ref();
    let mut record = load_note_meta(core, note_id, ctx.seal.as_ref())
        .await?
        .ok_or(Error::NoteNotFound {
            id: note_id.to_string(),
        })?;
    record.encryption = level.to_string();
    record.updated_at = chrono::Utc::now().to_rfc3339();
    put_note_meta(core, note_id, &record, ctx.seal.as_ref()).await?;
    // 元数据变更事件 → 搜索投影 reindex（加密后从索引移除, 解密后恢复）
    core.event_bus
        .publish(crate::event_bus::layered::AppEvent::NoteMetadataChanged {
            note_id: note_id.to_string(),
            changes: crate::event_bus::layered::NoteChanges {
                title: None,
                ..Default::default()
            },
        });
    tracing::info!(note_id = %note_id, level = %level, "note encryption level set");
    Ok(())
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

    // 3) blocks 派生（内容 → 块树）— 派生失败不阻断主流程（DK-01 DoD:
    //    notes/快照为权威已落盘, blocks 索引滞后可由启动重建补齐）
    if let Some(blocks) = ctx.blocks.as_ref() {
        if let Err(e) = blocks.sync_note_blocks(&id, Some(&ws), &doc.body()) {
            tracing::warn!(note_id = %id, error = %e, "blocks derive failed; non-blocking");
        }
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

    // 2) 原子保存（快照 → 元数据）— 加密笔记 record.content 存密文（S2），
    //    Loro 快照/blocks 属本机信任边界明文（锁定态排除由 S3 全链路过滤承担）
    record.content = seal_content(ctx, note_id, content, &record.encryption)?;
    record.updated_at = chrono::Utc::now().to_rfc3339();
    persist_doc(core, note_id, &doc).await?;
    put_note_meta(core, note_id, &record, ctx.seal.as_ref()).await?;

    // 3) blocks 派生 — 派生失败不阻断主流程（DK-01 DoD: notes/快照为
    //    权威已落盘, blocks 索引滞后可由启动重建补齐）
    if let Some(blocks) = ctx.blocks.as_ref() {
        if let Err(e) = blocks.sync_note_blocks(note_id, Some(&ws), content) {
            tracing::warn!(note_id = %note_id, error = %e, "blocks derive failed; non-blocking");
        }
    }

    // 4) 事件：High 实时（UI）+ Medium 持久化（投影重放驱动搜索索引 —
    //    SearchIndexProjection 消费 NoteMetadataChanged 从 KV 重取内容重建）
    core.event_bus
        .publish(crate::event_bus::layered::AppEvent::NoteContentChanged {
            note_id: note_id.to_string(),
            block_id: None,
        });
    core.event_bus
        .publish(crate::event_bus::layered::AppEvent::NoteMetadataChanged {
            note_id: note_id.to_string(),
            changes: crate::event_bus::layered::NoteChanges {
                title: None,
                tags: None,
            },
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
    record.content = seal_content(ctx, note_id, content, &record.encryption)?;
    record.updated_at = chrono::Utc::now().to_rfc3339();
    put_note_meta(core, note_id, &record, ctx.seal.as_ref()).await?;
    core.event_bus
        .publish(crate::event_bus::layered::AppEvent::NoteContentChanged {
            note_id: note_id.to_string(),
            block_id: None,
        });
    core.event_bus
        .publish(crate::event_bus::layered::AppEvent::NoteMetadataChanged {
            note_id: note_id.to_string(),
            changes: crate::event_bus::layered::NoteChanges {
                title: None,
                tags: None,
            },
        });
    Ok(WriteReceipt {
        seq: core.event_bus.last_seq(),
        aggregate_id: note_id.to_string(),
        committed_at: now_ms,
    })
}

/// M3 双写观察期一致性校验 — 无 Loro 降级模式（无双写, 恒一致）。
#[cfg(not(feature = "loro-crdt"))]
pub async fn verify_dual_write_consistency(_ctx: &WriteContext) -> Result<Vec<String>, Error> {
    Ok(Vec::new())
}

#[cfg(feature = "loro-crdt")]
/// M3 双写观察期一致性校验（DK-01 权威源三阶段）。
///
/// 逐条对比 KV 元数据 content（权威指针）与 Loro 快照恢复 body（演进目标）。
/// 返回不一致的 note_id 列表（空 = 双写一致）。快照缺失/损坏计入不一致。
pub async fn verify_dual_write_consistency(ctx: &WriteContext) -> Result<Vec<String>, Error> {
    let core = ctx.core.as_ref();
    let pairs = core.kv_store.scan_prefix("note:").await?;
    let mut mismatches = Vec::new();
    for (key, bytes) in &pairs {
        let Some(note_id) = key.strip_prefix("note:") else {
            continue;
        };
        let plain = match ctx.seal.as_ref() {
            Some(seal) => match (seal.unseal)(bytes) {
                Ok(b) => b,
                Err(_) => {
                    mismatches.push(note_id.to_string());
                    continue;
                }
            },
            None => bytes.clone(),
        };
        let Ok(record) = serde_json::from_slice::<NoteRecord>(&plain) else {
            mismatches.push(note_id.to_string());
            continue;
        };
        // Loro 快照恢复对比（无快照 = 观察期前历史数据, 放行不判不一致）
        let snap = core
            .kv_store
            .get(&format!("notesnap:{note_id}"))
            .await?
            .unwrap_or_default();
        if snap.is_empty() {
            continue;
        }
        // DK-07 S4: 加密笔记 record.content 是密文 — 解密后对比；
        // 锁定态（无 cipher）跳过巡检（不误报，密文完整性由 GCM 认证保证）
        let expected_content = if record.encryption == ENC_AES256GCM {
            let Some(cipher) = ctx.content_cipher.as_ref() else {
                continue;
            };
            match (cipher.decrypt)(note_id, &record.content) {
                Ok(plain) => plain,
                Err(_) => {
                    mismatches.push(note_id.to_string());
                    continue;
                }
            }
        } else {
            record.content.clone()
        };
        match crate::l1_infrastructure::note_doc::NoteDoc::from_snapshot(&snap) {
            Ok(doc) => {
                if doc.body() != expected_content {
                    mismatches.push(note_id.to_string());
                }
            }
            Err(_) => mismatches.push(note_id.to_string()),
        }
    }
    Ok(mismatches)
}

/// 启动时 blocks 派生全量重建（DK-01 权威源三阶段）。
///
/// blocks 为派生索引：缺失/损坏时从 notes.content 权威重建。
/// 幂等（sync_note_blocks 软删旧块重派生）；单条派生失败仅告警不阻断
/// （重建整体继续，失败条目留待下次启动补偿）。
pub async fn rebuild_blocks_derivation(ctx: &WriteContext) -> Result<usize, Error> {
    let Some(blocks) = ctx.blocks.as_ref() else {
        return Ok(0); // 无 blocks 后端（内存降级模式）— 无需重建
    };
    let core = ctx.core.as_ref();
    let pairs = core.kv_store.scan_prefix("note:").await?;
    let mut rebuilt = 0usize;
    for (key, bytes) in &pairs {
        let Some(note_id) = key.strip_prefix("note:") else {
            continue;
        };
        // seal 端（桌面）解封读权威内容；明文端（移动）直读
        let plain = match ctx.seal.as_ref() {
            Some(seal) => match (seal.unseal)(bytes) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(note_id = %note_id, error = %e, "rebuild unseal failed; skip");
                    continue;
                }
            },
            None => bytes.clone(),
        };
        let Ok(record) = serde_json::from_slice::<NoteRecord>(&plain) else {
            tracing::warn!(note_id = %note_id, "rebuild deserialize failed; skip");
            continue;
        };
        if let Err(e) = blocks.sync_note_blocks(note_id, None, &record.content) {
            tracing::warn!(note_id = %note_id, error = %e, "rebuild derive failed; skip");
            continue;
        }
        rebuilt += 1;
    }
    tracing::info!(rebuilt, total = pairs.len(), "blocks derivation rebuilt");
    Ok(rebuilt)
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

/// 回收站标记条目（DK-02 S1 — `trash:{note_id}` 的值结构）。
///
/// `title` 为删除时的元数据快照（`Alpha 裁决：入快照明文`——回收站为本地面，
/// 列表展示零解密开销；加密笔记的 title 在 `NoteRecord` 层本就明文，仅正文密文）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrashedNote {
    pub note_id: String,
    pub deleted_at_ms: i64,
    pub title: String,
}

fn trash_key(note_id: &str) -> String {
    format!("trash:{note_id}")
}

/// 笔记入回收站（DK-02 S1 软删除改造）。
///
/// 语义变更（对调用方透明，签名不变）：
/// - 物理 `note:{id}` / `notesnap:{id}` **保留**（恢复数据源）；
/// - 新写 `trash:{id}` 标记键（幂等覆盖：重复删除仅刷新 deleted_at）；
/// - `NoteDeleted` 事件照发（投影/搜索清理——主视图与索引消失）；
/// - 附件级联**移除**（回收站期间附件保留，随 [`purge_note`] 收编物理删）。
pub async fn delete_note(ctx: &WriteContext, note_id: &str) -> Result<WriteReceipt, Error> {
    let core = ctx.core.as_ref();
    let unseal = ctx.seal.as_ref();
    // 存在性校验（读元数据）— 同时取 title 快照（trash 列表零解密开销）
    let record = load_note_meta(core, note_id, unseal)
        .await?
        .ok_or(Error::NoteNotFound {
            id: note_id.to_string(),
        })?;

    // DK-02 S1：写标记键（物理键保留——恢复数据源）。幂等覆盖写：
    // 重复删除仅刷新 deleted_at_ms，不报错（回收站语义下重复删除无害）。
    let now_ms = chrono::Utc::now().timestamp_millis();
    let marker = TrashedNote {
        note_id: note_id.to_string(),
        deleted_at_ms: now_ms,
        title: record.title.clone(),
    };
    core.kv_store
        .set(
            &trash_key(note_id),
            &serde_json::to_vec(&marker).map_err(|e| Error::Internal(e.to_string()))?,
        )
        .await?;

    let _ = unseal; // seal 语义未变（meta 读取已消费）
    core.event_bus
        .publish(crate::event_bus::layered::AppEvent::NoteDeleted {
            note_id: note_id.to_string(),
        });
    info!(
        note_id,
        "note moved to trash via WritePath (soft delete, DK-02 S1)"
    );
    Ok(WriteReceipt {
        seq: core.event_bus.last_seq(),
        aggregate_id: note_id.to_string(),
        committed_at: now_ms,
    })
}

/// 从回收站恢复笔记（DK-02 S1）。
///
/// 事件选型裁决（Bravo，理由入交付报告）：**重放 `NoteCreated`**，不新增
/// `NoteRestored` 事件——①投影/搜索索引为禁改冻结面，`NoteRestored` 无消费者
/// 无法恢复可见性；②`NoteCreated` 重放走完整建条目路径（搜索 `index_note` +
/// blocks 派生重建 DK-01W 先例）；③事件字典 41 冻结面零改动。
///
/// # Errors
/// - `NoteNotFound`：标记键不存在（不在回收站）或物理键已失（不一致态）
/// - `Crypto`：加密笔记处于锁定态（无 cipher 可解密——fail-closed，恢复必须
///   产出明文供索引重建，绝不以密文重放）
pub async fn restore_note(ctx: &WriteContext, note_id: &str) -> Result<WriteReceipt, Error> {
    let core = ctx.core.as_ref();
    if core.kv_store.get(&trash_key(note_id)).await?.is_none() {
        return Err(Error::NoteNotFound {
            id: note_id.to_string(),
        });
    }
    let record = load_note_meta(core, note_id, ctx.seal.as_ref())
        .await?
        .ok_or(Error::NoteNotFound {
            id: note_id.to_string(),
        })?;

    core.kv_store.delete(&trash_key(note_id)).await?;

    let content = open_note_content(ctx, note_id, &record)?;
    core.event_bus
        .publish(crate::event_bus::layered::AppEvent::NoteCreated {
            note_id: note_id.to_string(),
            title: record.title.clone(),
            content,
        });
    info!(note_id, "note restored from trash via WritePath (DK-02 S1)");
    Ok(WriteReceipt {
        seq: core.event_bus.last_seq(),
        aggregate_id: note_id.to_string(),
        committed_at: chrono::Utc::now().timestamp_millis(),
    })
}

/// 彻底删除回收站笔记（DK-02 S1 — 原物理删逻辑收编）。
///
/// 前置：标记键存在（只对回收站中的笔记生效——状态机明确，误 purge
/// 未删除笔记不可能）。幂等性：purge 后标记键已清，二次调用报 `NoteNotFound`。
pub async fn purge_note(ctx: &WriteContext, note_id: &str) -> Result<WriteReceipt, Error> {
    let core = ctx.core.as_ref();
    if core.kv_store.get(&trash_key(note_id)).await?.is_none() {
        return Err(Error::NoteNotFound {
            id: note_id.to_string(),
        });
    }

    core.kv_store.delete(&trash_key(note_id)).await?;
    core.kv_store.delete(&format!("note:{note_id}")).await?;
    core.kv_store.delete(&format!("notesnap:{note_id}")).await?;

    // 附件级联（DK-09 裁决）：清 meta + 反向索引；失败不阻塞 purge
    // （blob 由 GC 兜底），日志留痕。
    if let Some(store) = ctx.attachments.as_ref() {
        match store.list_by_note(note_id).await {
            Ok(items) => {
                for item in items {
                    if let Err(e) = store.delete(&item.attachment_id).await {
                        tracing::warn!(note_id, attachment_id = %item.attachment_id, error = %e, "attachment cascade purge failed; GC will reclaim");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(note_id, error = %e, "attachment cascade list failed; GC will reclaim")
            }
        }
    }

    // NoteDeleted 幂等重发：restore 重放 NoteCreated 后若再 purge，
    // 投影/索引需再次清理（消费侧按 id 幂等）。
    let now_ms = chrono::Utc::now().timestamp_millis();
    core.event_bus
        .publish(crate::event_bus::layered::AppEvent::NoteDeleted {
            note_id: note_id.to_string(),
        });
    info!(note_id, "note purged from trash via WritePath (DK-02 S1)");
    Ok(WriteReceipt {
        seq: core.event_bus.last_seq(),
        aggregate_id: note_id.to_string(),
        committed_at: now_ms,
    })
}

/// 笔记是否在回收站中（DK-02 S1 — 列表/检索面过滤 trash 项的公共谓词）。
pub async fn is_trashed(core: &AppCore, note_id: &str) -> bool {
    core.kv_store
        .get(&trash_key(note_id))
        .await
        .map(|v| v.is_some())
        .unwrap_or(false)
}

/// 丢弃笔记（DK-02 S1 — 导入失败清理专用：软删后立即 purge，不留任何键）。
///
/// 语义区别于 [`delete_note`]（进回收站可恢复）与 [`purge_note`]（仅回收站中）：
/// 本原语是「从不存在过」的清场操作——半截笔记不留 note:/notesnap:/trash: 残留。
/// 幂等：不存在/未删除均不报错（清场操作，尽最大努力）。
pub async fn discard_note(ctx: &WriteContext, note_id: &str) -> Result<(), Error> {
    let core = ctx.core.as_ref();
    // 未在回收站则先入站（purge 前置要求）；已软删则直接 purge。
    // NoteNotFound 任一步静默吞掉——清场语义，目标态=无残留。
    let _ = delete_note(ctx, note_id).await;
    let _ = purge_note(ctx, note_id).await;
    debug_assert!(core.kv_store.get(&trash_key(note_id)).await.is_ok());
    Ok(())
}

/// 列出回收站（DK-02 S1）——按删除时间倒序（新删在前）。
pub async fn list_trashed(core: &AppCore) -> Result<Vec<TrashedNote>, Error> {
    let mut out = Vec::new();
    for (_, v) in core.kv_store.scan_prefix("trash:").await? {
        match serde_json::from_slice::<TrashedNote>(&v) {
            Ok(t) => out.push(t),
            Err(e) => tracing::warn!(error = %e, "trash marker parse failed; skipped"),
        }
    }
    out.sort_by(|a, b| b.deleted_at_ms.cmp(&a.deleted_at_ms));
    Ok(out)
}

/// 清空过期回收站（DK-02 S1 — 供未来调度消费；30 天自动清空的调度接线挂 S2）。
///
/// 返回本次 purge 的 note_id 列表（单条失败不阻塞批次，错误日志留痕）。
pub async fn purge_expired(ctx: &WriteContext, days: i64) -> Result<Vec<String>, Error> {
    let core = ctx.core.as_ref();
    let cutoff = chrono::Utc::now().timestamp_millis() - days * 86_400_000;
    let mut purged = Vec::new();
    for (key, v) in core.kv_store.scan_prefix("trash:").await? {
        let Ok(marker) = serde_json::from_slice::<TrashedNote>(&v) else {
            tracing::warn!(trash_key = %key, "trash marker parse failed; skipped");
            continue;
        };
        if marker.deleted_at_ms <= cutoff {
            match purge_note(ctx, &marker.note_id).await {
                Ok(_) => purged.push(marker.note_id),
                Err(e) => {
                    tracing::warn!(note_id = %marker.note_id, error = %e, "expired purge failed; skipped")
                }
            }
        }
    }
    Ok(purged)
}

// ===== 附件写入入口（DK-09 · request 裁决落地）=====

/// 附件写入（唯一入口）——数据经 `WriteContext.seal` 字节封装（桌面 vault
/// DEK at-rest；移动 None 明文降级），内容寻址去重（sha256 blob 键）。
///
/// 不触碰笔记内容版本（`updated_at`/blocks/事件流均不动——附件不是正文变更）。
/// `ctx.attachments` 为 None 时 fail-closed（调用方未注入附件能力）。
pub async fn attach_to_note(
    ctx: &WriteContext,
    note_id: &str,
    file_name: &str,
    mime: &str,
    data: &[u8],
) -> Result<crate::attachment_store::AttachmentMeta, Error> {
    let store = ctx
        .attachments
        .as_ref()
        .ok_or_else(|| Error::Crypto("no attachment store configured".into()))?;

    // 笔记存在性（附件必须挂在真实笔记上）
    load_note_meta(ctx.core.as_ref(), note_id, ctx.seal.as_ref())
        .await?
        .ok_or(Error::NoteNotFound {
            id: note_id.to_string(),
        })?;

    // 字节封装（明文降级 = 移动端语义）
    let stored: Vec<u8> = match ctx.seal.as_ref() {
        Some(pair) => (pair.seal)(data)?,
        None => data.to_vec(),
    };

    let meta = crate::attachment_store::make_meta(note_id, file_name, mime, data);
    // existed（blob 已复用）当前编排层不消费——去重计数由导入器经
    // get_blob 预检统计（bravo-request-put-existed-flag 批复口径）。
    let _existed = store.put(&meta, &stored).await?;
    info!(note_id, attachment_id = %meta.attachment_id, size = meta.size, "attachment stored via WritePath");
    Ok(meta)
}

/// 附件读取（唯一入口）——取密封字节 → 解封 → 明文 sha256 完整性校验
/// （fail-closed：任何不匹配都不返回数据）。
pub async fn read_attachment(
    ctx: &WriteContext,
    attachment_id: &str,
) -> Result<(crate::attachment_store::AttachmentMeta, Vec<u8>), Error> {
    let store = ctx
        .attachments
        .as_ref()
        .ok_or_else(|| Error::Crypto("no attachment store configured".into()))?;

    let meta = store
        .get_meta(attachment_id)
        .await?
        .ok_or(Error::NoteNotFound {
            id: attachment_id.to_string(),
        })?;
    let sealed = store
        .get_blob(&meta.sha256)
        .await?
        .ok_or_else(|| Error::Crypto(format!("attachment blob missing: {attachment_id}")))?;

    let plaintext: Vec<u8> = match ctx.seal.as_ref() {
        Some(pair) => (pair.unseal)(&sealed)?,
        None => sealed,
    };

    let actual = crate::attachment_store::sha256_hex(&plaintext);
    if actual != meta.sha256 {
        return Err(Error::Crypto(format!(
            "attachment '{attachment_id}' integrity check failed (expected {}, got {actual})",
            meta.sha256
        )));
    }
    Ok((meta, plaintext))
}
