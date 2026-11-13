//! B-04: 文件树/智能文件夹域（write_path 拆分——纯搬移）。
use super::*;

/// 同父下尾部排序键：现 max(sort_order) + 1（内存全量口径——Alpha 改判）。
async fn next_sort_order(
    core: &AppCore,
    parent: Option<&str>,
    unseal: Option<&SealPair>,
) -> Result<i64, Error> {
    let mut max = 0i64;
    for (_, bytes) in core.kv_store.scan_prefix("note:").await? {
        if let Some(rec) = decode_record(&bytes, unseal) {
            if rec.parent_id.as_deref() == parent && rec.sort_order > max {
                max = rec.sort_order;
            }
        }
    }
    Ok(max + 1)
}

/// 创建文件夹（DK-02 S2——文件夹即 kind=Folder 的笔记记录，统一存储）。
pub async fn create_folder(
    ctx: &WriteContext,
    parent_id: Option<&str>,
    title: &str,
) -> Result<WriteReceipt, Error> {
    let core = ctx.core.as_ref();
    if let Some(pid) = parent_id {
        ensure_folder_exists(core, pid, ctx.seal.as_ref()).await?;
    }
    // 复用 create_note 主流程（Loro 快照/blocks 派生/NoteCreated 事件），随后补树字段
    let note_id = create_note(ctx, title).await?;
    let mut rec = load_note_meta(core, &note_id, ctx.seal.as_ref())
        .await?
        .ok_or(Error::NoteNotFound {
            id: note_id.clone(),
        })?;
    rec.kind = NoteKind::Folder;
    rec.parent_id = parent_id.map(|s| s.to_string());
    rec.sort_order = next_sort_order(core, parent_id, ctx.seal.as_ref()).await?;
    put_note_meta(core, &note_id, &rec, ctx.seal.as_ref()).await?;
    info!(note_id = %note_id, parent = ?parent_id, "folder created via WritePath (DK-02 S2)");
    Ok(WriteReceipt {
        seq: core.event_bus.last_seq(),
        aggregate_id: note_id,
        committed_at: chrono::Utc::now().timestamp_millis(),
    })
}

/// 校验节点存在且为 Folder（移动/建子目标合法性）。
async fn ensure_folder_exists(
    core: &AppCore,
    folder_id: &str,
    unseal: Option<&SealPair>,
) -> Result<NoteRecord, Error> {
    let bytes = core
        .kv_store
        .get(&format!("note:{folder_id}"))
        .await?
        .ok_or(Error::NoteNotFound {
            id: folder_id.to_string(),
        })?;
    let rec = decode_record(&bytes, unseal).ok_or(Error::NoteNotFound {
        id: folder_id.to_string(),
    })?;
    if rec.kind != NoteKind::Folder {
        return Err(Error::InvalidInput(format!(
            "目标 '{folder_id}' 不是文件夹（kind={:?}），不能作为父节点",
            rec.kind
        )));
    }
    Ok(rec)
}

/// 移动节点（DK-02 S2）：环检测（沿 new_parent 链上溯，depth 上限 = 节点总数）
/// + 目标文件夹校验 + NoteMetadataChanged 事件（树移动=元数据变更，零新事件）。
pub async fn move_node(
    ctx: &WriteContext,
    note_id: &str,
    new_parent_id: Option<&str>,
    sort_order: i64,
) -> Result<WriteReceipt, Error> {
    let core = ctx.core.as_ref();
    let rec = load_note_meta(core, note_id, ctx.seal.as_ref())
        .await?
        .ok_or(Error::NoteNotFound {
            id: note_id.to_string(),
        })?;
    if let Some(pid) = new_parent_id {
        if pid == note_id {
            return Err(Error::CircularMove {
                message: format!("节点 '{note_id}' 不能挂到自己名下"),
            });
        }
        ensure_folder_exists(core, pid, ctx.seal.as_ref()).await?;
        // 环检测：沿 new_parent 链上溯——命中自身即环（节点数为深度上限）
        let total = core.kv_store.scan_prefix("note:").await?.len() as u32;
        let mut cur = pid.to_string();
        let mut depth = 0u32;
        loop {
            depth += 1;
            if depth > total {
                break; // 链长超节点数：防御性终止（历史脏数据环由本检查兜底）
            }
            if cur == note_id {
                return Err(Error::CircularMove {
                    message: format!(
                        "节点 '{note_id}' 的目标父链中包含它自身（子树不能挂到自己子级）"
                    ),
                });
            }
            let Some(parent) = load_note_meta(core, &cur, ctx.seal.as_ref()).await? else {
                break;
            };
            match parent.parent_id.clone() {
                Some(p) => cur = p,
                None => break,
            }
        }
    }
    let mut updated = rec;
    updated.parent_id = new_parent_id.map(|s| s.to_string());
    updated.sort_order = sort_order;
    updated.updated_at = chrono::Utc::now().to_rfc3339();
    put_note_meta(core, note_id, &updated, ctx.seal.as_ref()).await?;
    core.event_bus
        .publish(crate::event_bus::layered::AppEvent::NoteMetadataChanged {
            note_id: note_id.to_string(),
            changes: crate::event_bus::layered::NoteChanges {
                title: None,
                tags: None,
            },
        });
    info!(note_id, parent = ?new_parent_id, sort_order, "node moved via WritePath (DK-02 S2)");
    Ok(WriteReceipt {
        seq: core.event_bus.last_seq(),
        aggregate_id: note_id.to_string(),
        committed_at: chrono::Utc::now().timestamp_millis(),
    })
}

/// 重命名文件夹（kind 校验——笔记走 rename_note 既有路径）。
pub async fn rename_folder(
    ctx: &WriteContext,
    folder_id: &str,
    new_title: &str,
) -> Result<WriteReceipt, Error> {
    let core = ctx.core.as_ref();
    let rec = load_note_meta(core, folder_id, ctx.seal.as_ref())
        .await?
        .ok_or(Error::NoteNotFound {
            id: folder_id.to_string(),
        })?;
    if rec.kind != NoteKind::Folder && rec.kind != NoteKind::SmartFolder {
        return Err(Error::InvalidInput(format!(
            "节点 '{folder_id}' 不是文件夹/智能文件夹（kind={:?}）",
            rec.kind
        )));
    }
    rename_note(ctx, folder_id, new_title).await
}

/// 目录树扁平列表（(parent_id, sort_order) 排序——UI 端组装嵌套）。
/// trash 在册者不出现（回收站独立面——复用 S1 语义）。
pub async fn list_tree(core: &AppCore, unseal: Option<&SealPair>) -> Result<Vec<TreeNode>, Error> {
    let trashed: HashSet<String> = core
        .kv_store
        .scan_prefix("trash:")
        .await?
        .into_iter()
        .map(|(k, _)| k.trim_start_matches("trash:").to_string())
        .collect();
    let mut nodes: Vec<TreeNode> = Vec::new();
    for (k, bytes) in core.kv_store.scan_prefix("note:").await? {
        let Some(id) = k.strip_prefix("note:") else {
            continue;
        };
        if trashed.contains(id) {
            continue;
        }
        if let Some(rec) = decode_record(&bytes, unseal) {
            nodes.push(TreeNode {
                note_id: id.to_string(),
                kind: rec.kind,
                title: rec.title,
                parent_id: rec.parent_id,
                sort_order: rec.sort_order,
            });
        }
    }
    nodes.sort_by(|a, b| {
        (&a.parent_id, a.sort_order, &a.title).cmp(&(&b.parent_id, b.sort_order, &b.title))
    });
    Ok(nodes)
}

/// 删除文件夹（DK-02 S2 改判口径）：递归收集子树 → 全部成员（笔记+子文件夹）
/// 逐个走 S1 软删 delete_note（trash 标记 + NoteDeleted 每成员照发）——
/// 不做物理级联（与「误删即永久损失」产品立场一致）。返回入站成员数。
pub async fn delete_folder(ctx: &WriteContext, folder_id: &str) -> Result<usize, Error> {
    let core = ctx.core.as_ref();
    let rec = load_note_meta(core, folder_id, ctx.seal.as_ref())
        .await?
        .ok_or(Error::NoteNotFound {
            id: folder_id.to_string(),
        })?;
    if rec.kind != NoteKind::Folder {
        return Err(Error::InvalidInput(format!(
            "节点 '{folder_id}' 不是文件夹（kind={:?}），走 delete_note 即可",
            rec.kind
        )));
    }
    let members = collect_subtree(core, folder_id, ctx.seal.as_ref()).await?;
    let n = members.len();
    for id in &members {
        delete_note(ctx, id).await?;
    }
    info!(
        folder_id,
        members = n,
        "folder soft-deleted with subtree (DK-02 S2)"
    );
    Ok(n)
}

// ===== DK-02 S3：智能文件夹（动态视图——结构化规则过滤，不掺向量路）=====

/// 创建智能文件夹（DK-02 S3）：kind=SmartFolder + rule 落 NoteRecord——
/// 统一存储复用 S2 树挂载/restore/回收站全链。动态视图无成员（rule 求值）。
pub async fn create_smartfolder(
    ctx: &WriteContext,
    parent_id: Option<&str>,
    title: &str,
    rule: FilterRule,
) -> Result<WriteReceipt, Error> {
    let core = ctx.core.as_ref();
    if let Some(pid) = parent_id {
        ensure_folder_exists(core, pid, ctx.seal.as_ref()).await?;
    }
    let note_id = create_note(ctx, title).await?;
    let mut rec = load_note_meta(core, &note_id, ctx.seal.as_ref())
        .await?
        .ok_or(Error::NoteNotFound {
            id: note_id.clone(),
        })?;
    rec.kind = NoteKind::SmartFolder;
    rec.parent_id = parent_id.map(|s| s.to_string());
    rec.sort_order = next_sort_order(core, parent_id, ctx.seal.as_ref()).await?;
    rec.rule = Some(rule);
    put_note_meta(core, &note_id, &rec, ctx.seal.as_ref()).await?;
    info!(note_id = %note_id, "smartfolder created via WritePath (DK-02 S3)");
    Ok(WriteReceipt {
        seq: core.event_bus.last_seq(),
        aggregate_id: note_id,
        committed_at: chrono::Utc::now().timestamp_millis(),
    })
}

/// 更新智能文件夹规则（rule 变更发 NoteMetadataChanged——树移动同款零新事件）。
pub async fn update_rule(
    ctx: &WriteContext,
    folder_id: &str,
    rule: FilterRule,
) -> Result<WriteReceipt, Error> {
    let core = ctx.core.as_ref();
    let rec = load_note_meta(core, folder_id, ctx.seal.as_ref())
        .await?
        .ok_or(Error::NoteNotFound {
            id: folder_id.to_string(),
        })?;
    if rec.kind != NoteKind::SmartFolder {
        return Err(Error::InvalidInput(format!(
            "节点 '{folder_id}' 不是智能文件夹（kind={:?}）",
            rec.kind
        )));
    }
    let mut updated = rec;
    updated.rule = Some(rule);
    updated.updated_at = chrono::Utc::now().to_rfc3339();
    put_note_meta(core, folder_id, &updated, ctx.seal.as_ref()).await?;
    core.event_bus
        .publish(crate::event_bus::layered::AppEvent::NoteMetadataChanged {
            note_id: folder_id.to_string(),
            changes: crate::event_bus::layered::NoteChanges {
                title: None,
                tags: None,
            },
        });
    info!(
        folder_id,
        "smartfolder rule updated via WritePath (DK-02 S3)"
    );
    Ok(WriteReceipt {
        seq: core.event_bus.last_seq(),
        aggregate_id: folder_id.to_string(),
        committed_at: chrono::Utc::now().timestamp_millis(),
    })
}

/// 求值智能文件夹（DK-02 S3）：scan note: → 按 rule 过滤（AND 语义）。
/// 排除：自身、trash 在册、非 Note 记录（Folder/SmartFolder 不嵌套列示）。
/// 返回 NoteRecord 全量（调用方取 title/时间——tags 条件 v2 扩展时同样走此结构）。
pub async fn evaluate_smart_folder(
    core: &AppCore,
    unseal: Option<&SealPair>,
    folder_id: &str,
) -> Result<Vec<NoteRecord>, Error> {
    let sf = load_note_meta(core, folder_id, unseal)
        .await?
        .ok_or(Error::NoteNotFound {
            id: folder_id.to_string(),
        })?;
    if sf.kind != NoteKind::SmartFolder {
        return Err(Error::InvalidInput(format!(
            "节点 '{folder_id}' 不是智能文件夹（kind={:?}）",
            sf.kind
        )));
    }
    let rule = sf.rule.clone().unwrap_or_default();
    let trashed: HashSet<String> = core
        .kv_store
        .scan_prefix("trash:")
        .await?
        .into_iter()
        .map(|(k, _)| k.trim_start_matches("trash:").to_string())
        .collect();
    let mut out = Vec::new();
    for (k, bytes) in core.kv_store.scan_prefix("note:").await? {
        let Some(id) = k.strip_prefix("note:") else {
            continue;
        };
        if id == folder_id || trashed.contains(id) {
            continue;
        }
        if let Some(rec) = decode_record(&bytes, unseal) {
            // DK-27: tags 条件 v2——直接用 NoteRecord.tags（与 TagsProjection
            // 同源 KV，一致性由 parity 测试锚定；避免扫描内二次投影查询开销）。
            if rec.kind == NoteKind::Note && rule.matches_full(&rec.title, &rec.tags) {
                out.push(rec);
            }
        }
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(out)
}

/// 求值便捷入口（AppCore 直连——半接入三查：生产可见口径与 vector_index_for 对齐）。
pub async fn evaluate_smart_folder_for(
    core: &AppCore,
    unseal: Option<&SealPair>,
    folder_id: &str,
) -> Result<Vec<NoteRecord>, Error> {
    evaluate_smart_folder(core, unseal, folder_id).await
}
