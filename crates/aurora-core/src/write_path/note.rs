//! B-04: 笔记 CRUD 域（write_path 拆分——纯搬移零语义;API 路径不变
//! `aurora_core::write_path::*` 经 mod.rs 再导出,消费端零 diff）。
use super::*;

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

/// DK-27: 设置笔记标签集（全量覆盖语义——UI 多选编辑直传终态）。
///
/// 轻量读取源：KV `NoteRecord.tags`（TagsProjection/智能文件夹 tags 条件）。
/// 并发布 `NoteMetadataChanged { tags: Some(..) }`（Medium 通道，投影联动）。
/// **诚实化注记**：Loro doc meta.tags 的双写同步暂缓（无现成 NoteDoc::open
/// 加载路径）——tags 权威源 = KV NoteRecord；doc meta 侧同步待标签编辑 UI
/// 接线卡处理（doc.add_tag/remove_tag 现无调用者，暂无分叉风险）。
pub async fn set_note_tags(
    core: &AppCore,
    note_id: &str,
    tags: Vec<String>,
    seal: Option<&SealPair>,
) -> Result<(), Error> {
    // 1) KV NoteRecord.tags 轻量读取源（seal 对称写回）
    let mut rec = load_note_meta(core, note_id, seal)
        .await?
        .ok_or(Error::NoteNotFound {
            id: note_id.to_string(),
        })?;

    rec.tags = tags.clone();
    put_note_meta(core, note_id, &rec, seal).await?;

    // 2) 事件发布（投影联动）
    core.event_bus
        .publish(crate::event_bus::layered::AppEvent::NoteMetadataChanged {
            note_id: note_id.to_string(),
            changes: crate::event_bus::layered::NoteChanges {
                title: None,
                tags: Some(tags),
            },
        });
    Ok(())
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
    // 同级别幂等短路（重复锁定/解锁无副作用）。
    if record.encryption == level {
        return Ok(());
    }
    // DK-20：密级切换必须同步重写 content（洞修复——旧实现只改字段，
    // 明文笔记"锁定"后 content 仍明文落库）。fail-closed：无 cipher / 解密
    // 校验失败一律拒绝，record 不落任何变更。
    let plain = open_note_content(ctx, note_id, &record)?;
    let ws = None::<String>; // workspace 归属由 blocks 内部口径承担（单工作区）
    record.content = seal_content(ctx, note_id, &plain, level)?;
    record.encryption = level.to_string();
    record.updated_at = chrono::Utc::now().to_rfc3339();
    put_note_meta(core, note_id, &record, ctx.seal.as_ref()).await?;
    // 向量残留清理（锁定即从向量缓存剔除，防 enc1 密文被回填嵌入）。
    core.kv_store.delete(&format!("notevec:{note_id}")).await?;
    // blocks（图谱/关系源）：锁定态不派生（写入侧跳过——开工令教训 5），
    // 已有块清空（sync 空 content = 全软删）；解锁后由事件驱动重派生。
    if let Some(blocks) = ctx.blocks.as_ref() {
        if let Err(e) = blocks.sync_note_blocks(note_id, ws.as_deref(), "") {
            tracing::warn!(note_id = %note_id, error = %e, "blocks clear on lock failed; non-blocking");
        }
    }
    // 锁定后 blocks 保持为空：投影/图谱消费无明文块。解锁时重派生由
    // 下方"解锁恢复"分支的显式 sync 承担（见 level == ENC_NONE）。
    // 元数据变更事件 → 搜索投影 reindex（加密后从索引移除, 解密后恢复）
    core.event_bus
        .publish(crate::event_bus::layered::AppEvent::NoteMetadataChanged {
            note_id: note_id.to_string(),
            changes: crate::event_bus::layered::NoteChanges {
                title: None,
                ..Default::default()
            },
        });
    // 解锁恢复：blocks 重派生（明文块重新可被图谱/关系消费）。
    if level == ENC_NONE {
        if let Some(blocks) = ctx.blocks.as_ref() {
            if let Err(e) = blocks.sync_note_blocks(note_id, ws.as_deref(), &plain) {
                tracing::warn!(note_id = %note_id, error = %e, "blocks derive on unlock failed; non-blocking");
            }
        }
    }
    tracing::info!(note_id = %note_id, level = %level, "note encryption level set (content rewritten)");
    Ok(())
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
    persist_doc(core.kv_store.as_ref(), &core.user_id, &id, &doc).await?;
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
    let doc = load_or_init_doc(core.kv_store.as_ref(), &core.user_id, note_id).await?;
    doc.set_body(content, now_ms)?;
    let ws = doc.meta().workspace_id.clone();

    // 2) 原子保存（快照 → 元数据）— 加密笔记 record.content 存密文（S2），
    //    Loro 快照/blocks 属本机信任边界明文（锁定态排除由 S3 全链路过滤承担）
    record.content = seal_content(ctx, note_id, content, &record.encryption)?;
    record.updated_at = chrono::Utc::now().to_rfc3339();
    persist_doc(core.kv_store.as_ref(), &core.user_id, note_id, &doc).await?;
    put_note_meta(core, note_id, &record, ctx.seal.as_ref()).await?;

    // 3) blocks 派生 — 派生失败不阻断主流程（DK-01 DoD: notes/快照为
    //    权威已落盘, blocks 索引滞后可由启动重建补齐）。
    //    DK-20：锁定态（encryption=aes256gcm）写入侧跳过——图谱/关系面
    //    不消费锁定篇（开工令教训 5），解锁后 set_note_encryption 重派生。
    if record.encryption != ENC_AES256GCM {
        if let Some(blocks) = ctx.blocks.as_ref() {
            if let Err(e) = blocks.sync_note_blocks(note_id, Some(&ws), content) {
                tracing::warn!(note_id = %note_id, error = %e, "blocks derive failed; non-blocking");
            }
        }
    }

    // 3.5) 链接索引重建（DK-双向链接 S2，复核拍板 1：from_index 变体——
    //     旧集=KV links:out 权威读，无需 old_content 明文）。锁定态跳过
    //     （与 blocks 同款纪律：图谱/反链面不消费锁定篇）。失败不阻断
    //     主流程（notes/快照为权威，链接索引滞后可由后续保存补齐）。
    if record.encryption != ENC_AES256GCM {
        if let Err(e) =
            crate::links::reindex_note_links_from_index(core.kv_store.as_ref(), note_id, content)
                .await
        {
            tracing::warn!(note_id = %note_id, error = %e, "links reindex failed; non-blocking");
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
        // Loro 恢复对比（DK-40a 混合存储：恢复态 = 快照 + update log 重放；
        // 无快照且无 log = 观察期前历史数据, 放行不判不一致。DK-47 迁移读——
        // legacy 无段 key 命中即搬迁，观察语义对带段 key 成立）
        let snap =
            app_core::get_notesnap_with_migration(core.kv_store.as_ref(), &core.user_id, note_id)
                .await?
                .unwrap_or_default();
        let has_log = count_update_log(core.kv_store.as_ref(), note_id).await > 0;
        if snap.is_empty() && !has_log {
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
        match load_or_init_doc(core.kv_store.as_ref(), &core.user_id, note_id).await {
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
        let doc = load_or_init_doc(core.kv_store.as_ref(), &core.user_id, note_id).await?;
        doc.set_title(new_title, now_ms)?;
        persist_doc(core.kv_store.as_ref(), &core.user_id, note_id, &doc).await?;
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

    // 拍板 2 A 方案：rename 链接跟随（内容级改写传播）。非阻断——
    // rename 本体已提交，跟随失败仅 warn（悬挂惰性 S1 兜底+重跑幂等）。
    // 边界：仅笔记 kind 触发——rename_folder 复用本函数，文件夹标题
    // 不在 wiki 链接域（链接指向笔记标题），跟随会误改（语义污染）。
    if record.kind == NoteKind::Note {
        match follow_links_on_rename(ctx, &old_title, new_title).await {
            Ok(followed) if followed > 0 => {
                info!(note_id, followed, "wiki links followed on rename");
            }
            Ok(_) => {}
            Err(e) => {
                warn!(note_id, error = %e, "wiki link follow failed; non-blocking (S1 lazy fallback)");
            }
        }
    }

    Ok(WriteReceipt {
        seq: core.event_bus.last_seq(),
        aggregate_id: note_id.to_string(),
        committed_at: now_ms,
    })
}

/// rename 链接跟随（拍板 2 A 方案）：全库扫描出链含旧标题的笔记，
/// 逐篇改写 `[[旧标题]]` → `[[新标题]]` 并走 save_note_content 全纪律
/// 保存（blocks 派生+links reindex 继承）。
///
/// 纪律：
/// - 范围 = `links:out:*` 扫描（索引驱动，不扫全库正文）；
/// - 锁定/解封失败篇 → warn 跳过（悬挂惰性 S1 兜底，诚实边界注记）；
/// - 逐篇非原子，中断可重跑（幂等：已改写篇不再含旧标题 → 跳过保存）；
/// - 失败不阻断 rename 本体（调用方 warn）。
///
/// # Errors
/// KV 扫描失败（逐篇错误内部消化为 warn）。
async fn follow_links_on_rename(
    ctx: &WriteContext,
    old_title: &str,
    new_title: &str,
) -> Result<usize, Error> {
    let core = ctx.core.as_ref();
    let pairs = core
        .kv_store
        .scan_prefix(crate::links::LINKS_OUT_PREFIX)
        .await?;
    let mut followed = 0usize;

    for (key, bytes) in &pairs {
        let Some(src_id) = key.strip_prefix(crate::links::LINKS_OUT_PREFIX) else {
            continue;
        };
        let Ok(outs) = serde_json::from_slice::<Vec<String>>(bytes) else {
            warn!(src = %src_id, "follow scan deserialize failed; skip");
            continue;
        };
        if !outs.iter().any(|t| t == old_title) {
            continue; // 不引用旧标题：跳过（索引驱动省正文读）
        }

        // 读明文（seal 端解封 / 明文端直读）；失败篇跳过（悬挂惰性兜底）
        let Some(raw) = core.kv_store.get(&format!("note:{src_id}")).await? else {
            continue;
        };
        let plain = match ctx.seal.as_ref() {
            Some(seal) => match (seal.unseal)(&raw) {
                Ok(b) => b,
                Err(e) => {
                    warn!(src = %src_id, error = %e, "follow unseal failed; skip (S1 lazy fallback)");
                    continue;
                }
            },
            None => raw.clone(),
        };
        let Ok(record) = serde_json::from_slice::<NoteRecord>(&plain) else {
            warn!(src = %src_id, "follow deserialize failed; skip");
            continue;
        };

        // 语法面改写（围栏/行内码保护；alias 保留）；无命中跳过保存
        let Some((rewritten, _hits)) =
            crate::links::rewrite_wiki_links(&record.content, old_title, new_title)
        else {
            continue;
        };

        // 走 save_note_content 全纪律（blocks 派生+links reindex 同款）；
        // 单篇失败 warn 继续（最终一致+幂等重跑）
        match save_note_content(ctx, src_id, &rewritten).await {
            Ok(_) => followed += 1,
            Err(e) => {
                warn!(src = %src_id, error = %e, "follow rewrite save failed; skip (idempotent rerun)");
            }
        }
    }
    Ok(followed)
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
    let origin_path = origin_path_for(core, note_id, unseal).await;
    let marker = TrashedNote {
        note_id: note_id.to_string(),
        deleted_at_ms: now_ms,
        title: record.title.clone(),
        origin_path,
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

    // DK-02 S2 原位还原：删除时快照的路径（S1 旧标记无此字段 → None）
    let origin_path: Option<String> = match core.kv_store.get(&trash_key(note_id)).await? {
        Some(bytes) => serde_json::from_slice::<TrashedNote>(&bytes)
            .ok()
            .and_then(|t| t.origin_path),
        None => None,
    };
    core.kv_store.delete(&trash_key(note_id)).await?;

    // DK-02 S2 原位还原：父存（且为 Folder）→ 回原位；父失 → 按 origin_path
    // 逐级 title 找现存最深 Folder；仍找不到 → 挂根（core 面保证 parent_id=None
    // 并文档注明——UI 端由「最近位置」提示补足，Alpha 改判口径）。
    let mut parent = record.parent_id.clone();
    if let Some(pid) = &parent {
        // 父存活判定：物理键在 + 是 Folder + **不在回收站**（S1 语义联动——
        // 父与子同批软删时，子 restore 不应挂回已删父）
        let parent_trashed = matches!(core.kv_store.get(&trash_key(pid)).await, Ok(Some(_)));
        let ok = matches!(
            load_note_meta(core, pid, ctx.seal.as_ref()).await,
            Ok(Some(pr)) if pr.kind == NoteKind::Folder
        ) && !parent_trashed;
        if !ok {
            parent = resolve_origin_parent(core, origin_path.as_deref(), ctx.seal.as_ref()).await;
        }
    } else {
        // 根下笔记（或 S1 旧数据无树字段）：origin_path 可解析则升位
        parent = resolve_origin_parent(core, origin_path.as_deref(), ctx.seal.as_ref()).await;
    }
    let mut updated = record.clone();
    updated.parent_id = parent;
    updated.updated_at = chrono::Utc::now().to_rfc3339();
    put_note_meta(core, note_id, &updated, ctx.seal.as_ref()).await?;

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
    // DK-47：双删——per-user 带段 key + legacy 无段 key（用户未读过时
    // legacy 可能残留；delete 不存在 key 无害，幂等）
    core.kv_store
        .delete(&app_core::notesnap_key(&core.user_id, note_id))
        .await?;
    core.kv_store
        .delete(&format!("{}{note_id}", app_core::LEGACY_NOTESNAP_PREFIX))
        .await?;

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

    // 链接索引级联（DK-双向链接 S2 复核拍板 3）：清出链键+倒排残留；
    // 失败不阻塞 purge（最终一致——残余悬挂由 S1 惰性语义兜底）。
    if let Err(e) = crate::links::purge_note_links(core.kv_store.as_ref(), note_id).await {
        tracing::warn!(note_id = %note_id, error = %e, "links cascade purge failed; non-blocking");
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

/// 删除时路径快照：沿 parent 链上溯收集 Folder title（根→叶以 "/" 连接）。
/// visited 环防护（历史脏数据兜底）；向上遇到缺父/根即止。
async fn origin_path_for(
    core: &AppCore,
    note_id: &str,
    unseal: Option<&SealPair>,
) -> Option<String> {
    let mut chain: Vec<String> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut cur = note_id.to_string();
    loop {
        if !visited.insert(cur.clone()) {
            break; // 环兜底
        }
        let Ok(Some(rec)) = load_note_meta(core, &cur, unseal).await else {
            break;
        };
        match rec.parent_id.clone() {
            Some(p) => {
                if let Ok(Some(parent)) = load_note_meta(core, &p, unseal).await {
                    chain.push(parent.title.clone());
                }
                cur = p;
            }
            None => break,
        }
    }
    if chain.is_empty() {
        None
    } else {
        chain.reverse();
        Some(chain.join("/"))
    }
}

/// 按 origin_path（"A/B/C"）自根逐级解析最深现存 Folder id。
/// 任一级缺失即停在上一级（core 面只保证挂到现存最深祖先——原位语义见 restore_note）。
async fn resolve_origin_parent(
    core: &AppCore,
    origin_path: Option<&str>,
    unseal: Option<&SealPair>,
) -> Option<String> {
    let path = origin_path?;
    let mut current: Option<String> = None;
    for segment in path.split('/').filter(|s| !s.is_empty()) {
        // 在 current（或根）下按 title 找 Folder
        let mut found: Option<String> = None;
        for (k, bytes) in core.kv_store.scan_prefix("note:").await.unwrap_or_default() {
            let Some(id) = k.strip_prefix("note:") else {
                continue;
            };
            // DK-02 S2 复核补丁（Alpha 2026-09-29）：回收站中的 Folder 不作为
            // 还原锚点——与 restore_note 主链路「父存校验含 trash 判定」同构
            // （软删物理键仍在，漏判会把孤儿挂回已删父，违反挂根兜底语义）。
            if is_trashed(core, id).await {
                continue;
            }
            if let Some(rec) = decode_record(&bytes, unseal) {
                if rec.kind == NoteKind::Folder && rec.title == segment && rec.parent_id == current
                {
                    found = Some(id.to_string());
                    break;
                }
            }
        }
        match found {
            Some(id) => current = Some(id),
            None => break,
        }
    }
    current
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
