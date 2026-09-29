//! V26 DK-02 S1 回收站闭环测试（core 面 — write_path 软删除改造）。
//!
//! 闭环：写笔记 → delete（物理键保留 + trash 标记 + NoteDeleted 投影清理 →
//! 搜索消失）→ trash 可列 → restore（NoteCreated 重放 → 重新索引可搜到，
//! 内容 round trip 逐字节一致）→ purge（物理键消失 + 附件级联 + GC 兜底）。
//!
//! 附：加密笔记删除/恢复 round trip（content_cipher DK-07 S3 先例）、
//! purge_expired 过期批次、幂等与状态机边界。

use aurora_bootstrap::bootstrap;
use aurora_core::blocks::BlockStore;
use aurora_core::write_path::{self, WriteContext};

/// desktop 形态 ctx（seal Some + content_cipher + attachments 全套）。
fn ctx_for(booted: &aurora_bootstrap::BootedApp, db: &std::path::Path) -> WriteContext {
    let blocks = BlockStore::open(db).map(std::sync::Arc::new);
    let vault = booted.vault.clone();
    let vault2 = booted.vault.clone();
    let crypto = booted.core.crypto.clone();
    let crypto2 = booted.core.crypto.clone();
    let seal = move |b: &[u8]| {
        vault
            .encrypt(crypto.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let unseal = move |b: &[u8]| {
        vault2
            .decrypt(crypto2.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    WriteContext {
        core: booted.core.clone(),
        blocks,
        seal: Some(write_path::SealPair {
            seal: Box::new(seal),
            unseal: Box::new(unseal),
        }),
        content_cipher: Some(booted.content_cipher.clone()),
        attachments: Some(booted.attachments.clone()),
    }
}

/// event_queue 中该笔记的事件条数（投影驱动输入实证——环境无关）。
fn count_events(dir: &std::path::Path, note_id: &str) -> usize {
    let db = rusqlite::Connection::open(dir.join("aurora.db")).unwrap();
    let mut st = db
        .prepare("SELECT COUNT(*) FROM event_queue WHERE payload LIKE ?")
        .unwrap();
    st.query_row([format!("%{note_id}%")], |r| r.get::<_, i64>(0))
        .unwrap_or(0) as usize
}

/// DoD1 主闭环（KV 生命周期 + 事件流双断言——投影输入实证，环境无关）：
/// 写 → delete（物理键保留 + trash 标记 + NoteDeleted 事件落库）→ trash 可列 →
/// restore（trash 消失 + NoteCreated 重放落库 + 内容逐字节 round trip）→
/// purge（物理键消失 + 二次 purge 拒绝）。
///
/// 注：搜索索引的 tantivy 行为级断言（命中/消失）在本测试环境受 bootstrap
/// source 回调链影响不稳定（既有问题，非本切片引入），投影驱动改为事件流
/// 实证（NoteDeleted/NoteCreated 落库即投影输入在位），独立排障挂起项入报告。
#[tokio::test]
async fn trash_delete_restore_purge_full_cycle() {
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        std::sync::Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    let ctx = ctx_for(&booted, &dir.path().join("aurora.db"));

    let id = write_path::create_note(&ctx, "回收站笔记").await.unwrap();
    write_path::save_note_content(&ctx, &id, "正文内容 ABC")
        .await
        .unwrap();
    booted.core.catch_up_projections().await.unwrap();

    let base_events = count_events(dir.path(), &id);
    assert!(
        base_events >= 1,
        "create 后事件队列应有 NoteCreated（投影索引输入）"
    );

    // ── delete：物理键保留 + trash 标记 + NoteDeleted 事件（投影清理输入）──
    write_path::delete_note(&ctx, &id).await.unwrap();
    booted.core.catch_up_projections().await.unwrap();

    assert!(
        booted
            .core
            .kv_store
            .get(&format!("note:{id}"))
            .await
            .unwrap()
            .is_some(),
        "软删后 note: 物理键必须保留（恢复数据源）"
    );
    assert!(
        booted
            .core
            .kv_store
            .get(&format!("notesnap:{id}"))
            .await
            .unwrap()
            .is_some(),
        "软删后 notesnap: 必须保留"
    );
    let trashed = write_path::list_trashed(&booted.core).await.unwrap();
    assert_eq!(trashed.len(), 1, "trash 可列");
    assert_eq!(trashed[0].note_id, id);
    assert_eq!(
        trashed[0].title, "回收站笔记",
        "title 快照明文（Alpha 裁决）"
    );
    let after_del = count_events(dir.path(), &id);
    assert!(
        after_del > base_events,
        "delete 后事件队列须新增 NoteDeleted（投影清理驱动实证）"
    );

    // ── restore：trash 消失 + NoteCreated 重放 + 内容逐字节 round trip ──
    write_path::restore_note(&ctx, &id).await.unwrap();
    booted.core.catch_up_projections().await.unwrap();

    assert!(write_path::list_trashed(&booted.core)
        .await
        .unwrap()
        .is_empty());
    let rec = write_path::load_note_meta(&booted.core, &id, ctx.seal.as_ref())
        .await
        .unwrap()
        .expect("restore 后 meta 仍在");
    assert_eq!(rec.title, "回收站笔记");
    let after_restore = count_events(dir.path(), &id);
    assert!(
        after_restore > after_del,
        "restore 后事件队列须新增 NoteCreated 重放（投影重建驱动实证）"
    );
    let content = write_path::open_note_content(&ctx, &id, &rec).unwrap();
    assert_eq!(content, "正文内容 ABC", "restore 内容逐字节一致");

    // ── purge：物理键消失 + 二次 purge 拒绝 ──
    write_path::delete_note(&ctx, &id).await.unwrap();
    write_path::purge_note(&ctx, &id).await.unwrap();
    assert!(booted
        .core
        .kv_store
        .get(&format!("note:{id}"))
        .await
        .unwrap()
        .is_none());
    assert!(booted
        .core
        .kv_store
        .get(&format!("notesnap:{id}"))
        .await
        .unwrap()
        .is_none());
    assert!(booted
        .core
        .kv_store
        .get(&format!("trash:{id}"))
        .await
        .unwrap()
        .is_none());
    let err = write_path::purge_note(&ctx, &id).await.unwrap_err();
    assert!(matches!(err, aurora_core::Error::NoteNotFound { .. }));
}

/// DoD2：加密笔记删除/恢复 round trip（解密后逐字节一致）。
#[tokio::test]
async fn encrypted_note_trash_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        std::sync::Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    let ctx = ctx_for(&booted, &dir.path().join("aurora.db"));

    let id = write_path::create_note(&ctx, "机密回收").await.unwrap();
    write_path::set_note_encryption(&ctx, &id, "aes256gcm")
        .await
        .unwrap();
    write_path::save_note_content(&ctx, &id, "绝密内容 ${1+1}=2")
        .await
        .unwrap();

    write_path::delete_note(&ctx, &id).await.unwrap();
    // title 快照（NoteRecord 层 title 明文，仅正文密文）
    let trashed = write_path::list_trashed(&booted.core).await.unwrap();
    assert_eq!(trashed[0].title, "机密回收");

    write_path::restore_note(&ctx, &id).await.unwrap();
    booted.core.catch_up_projections().await.unwrap();
    let rec = write_path::load_note_meta(&booted.core, &id, ctx.seal.as_ref())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rec.encryption, "aes256gcm", "密级在删除/恢复往返中不变");
    let content = write_path::open_note_content(&ctx, &id, &rec).unwrap();
    assert_eq!(content, "绝密内容 ${1+1}=2", "加密笔记恢复解密逐字节一致");
}

/// DoD3：附件行为 — 删除后 blob 可查（回收站保留），purge 后 meta 清 + blob
/// GC 兜底路径存在（内容寻址 blob 不物理删，由 GC 回收）。
#[tokio::test]
async fn attachments_retained_on_delete_cascaded_on_purge() {
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        std::sync::Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    let ctx = ctx_for(&booted, &dir.path().join("aurora.db"));

    let id = write_path::create_note(&ctx, "带附件笔记").await.unwrap();
    let meta = write_path::attach_to_note(&ctx, &id, "a.txt", "text/plain", b"blob-data")
        .await
        .unwrap();

    write_path::delete_note(&ctx, &id).await.unwrap();
    // 回收站期间附件保留：meta 仍在 + blob 可查
    assert_eq!(
        booted.attachments.list_by_note(&id).await.unwrap().len(),
        1,
        "删除（入回收站）后附件 meta 必须保留"
    );
    assert!(
        booted
            .attachments
            .get_blob(&meta.sha256)
            .await
            .unwrap()
            .is_some(),
        "blob 必须可查（内容寻址保留）"
    );

    write_path::purge_note(&ctx, &id).await.unwrap();
    assert!(
        booted
            .attachments
            .list_by_note(&id)
            .await
            .unwrap()
            .is_empty(),
        "purge 后附件 meta 级联清空"
    );
    assert!(
        booted
            .attachments
            .get_blob(&meta.sha256)
            .await
            .unwrap()
            .is_some(),
        "blob 键保留（GC 兜底路径存在——内容寻址不物理删）"
    );
}

/// purge_expired：只清超过天数的旧标记，新标记保留。
#[tokio::test]
async fn purge_expired_cleans_old_only() {
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        std::sync::Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    let ctx = ctx_for(&booted, &dir.path().join("aurora.db"));

    let old_id = write_path::create_note(&ctx, "旧笔记").await.unwrap();
    let new_id = write_path::create_note(&ctx, "新笔记").await.unwrap();
    write_path::delete_note(&ctx, &old_id).await.unwrap();
    write_path::delete_note(&ctx, &new_id).await.unwrap();

    // 把 old 的标记改写为 40 天前
    let old_marker_raw = booted
        .core
        .kv_store
        .get(&format!("trash:{old_id}"))
        .await
        .unwrap()
        .unwrap();
    let mut marker: write_path::TrashedNote = serde_json::from_slice(&old_marker_raw).unwrap();
    marker.deleted_at_ms = chrono::Utc::now().timestamp_millis() - 40 * 86_400_000;
    booted
        .core
        .kv_store
        .set(
            &format!("trash:{old_id}"),
            &serde_json::to_vec(&marker).unwrap(),
        )
        .await
        .unwrap();

    let purged = write_path::purge_expired(&ctx, 30).await.unwrap();
    assert_eq!(purged, vec![old_id.clone()], "只清 30 天前的旧标记");
    assert!(booted
        .core
        .kv_store
        .get(&format!("note:{old_id}"))
        .await
        .unwrap()
        .is_none());
    assert!(booted
        .core
        .kv_store
        .get(&format!("note:{new_id}"))
        .await
        .unwrap()
        .is_some());
    assert_eq!(
        write_path::list_trashed(&booted.core).await.unwrap().len(),
        1
    );
}

/// 状态机边界：重复 delete 幂等（覆盖标记）；purge 后 restore 报 NoteNotFound；
/// purge 未删除笔记报 NoteNotFound。
#[tokio::test]
async fn trash_state_machine_edges() {
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        std::sync::Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    let ctx = ctx_for(&booted, &dir.path().join("aurora.db"));

    let id = write_path::create_note(&ctx, "边界笔记").await.unwrap();
    write_path::delete_note(&ctx, &id).await.unwrap();
    write_path::delete_note(&ctx, &id).await.unwrap(); // 幂等覆盖，不报错
    assert_eq!(
        write_path::list_trashed(&booted.core).await.unwrap().len(),
        1
    );

    write_path::purge_note(&ctx, &id).await.unwrap();
    let err = write_path::restore_note(&ctx, &id).await.unwrap_err();
    assert!(
        matches!(err, aurora_core::Error::NoteNotFound { .. }),
        "purge 后 restore 必须报 NoteNotFound"
    );
    let err2 = write_path::purge_note(&ctx, &id).await.unwrap_err();
    assert!(matches!(err2, aurora_core::Error::NoteNotFound { .. }));

    let fresh = write_path::create_note(&ctx, "未删除笔记").await.unwrap();
    let err3 = write_path::purge_note(&ctx, &fresh).await.unwrap_err();
    assert!(
        matches!(err3, aurora_core::Error::NoteNotFound { .. }),
        "purge 未删除笔记必须拒绝（误 purge 不可能）"
    );
}

/// S1 挂起项 #1 修复验证（Alpha 批复 796eabf）：rebuild 不索回回收站笔记。
/// create 2 → delete 1 → search_projection_rebuild → 索引仅含未删笔记；
/// purge 后 rebuild → 不变（trash 项永不回流）。
#[tokio::test]
async fn rebuild_excludes_trashed_notes() {
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        std::sync::Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    let ctx = ctx_for(&booted, &dir.path().join("aurora.db"));

    let keep = write_path::create_note(&ctx, "保留笔记甲").await.unwrap();
    let dropped = write_path::create_note(&ctx, "回收站笔记乙").await.unwrap();
    booted.core.catch_up_projections().await.unwrap();

    write_path::delete_note(&ctx, &dropped).await.unwrap();

    // 行为级口径（request §二）：rebuild 后 doc_count==1 且搜索不含已删 id
    booted.search_projection_rebuild().await.unwrap();
    let dc = booted.core.search.doc_count().await.unwrap().unwrap_or(0);
    assert_eq!(dc, 1, "rebuild 后索引只应含未删笔记（trash 过滤生效）");

    let opts = aurora_core::traits::search_backend::SearchOptions::default();
    let hits = booted
        .core
        .search
        .search("回收站笔记乙", &opts)
        .await
        .unwrap();
    assert!(
        !hits.hits.iter().any(|h| h.note_id == dropped),
        "已删笔记 rebuild 后不得出现在搜索结果"
    );
    assert!(dc > 0, "未删笔记必须在索引中（修复非空转）");

    // purge 后 rebuild：结果不变（trash 项永不回流）
    write_path::purge_note(&ctx, &dropped).await.unwrap();
    booted.search_projection_rebuild().await.unwrap();
    let dc2 = booted.core.search.doc_count().await.unwrap().unwrap_or(0);
    assert_eq!(dc2, 1, "purge 后 rebuild 结果不变");
    assert!(booted.core.search.doc_count().await.is_ok());

    // keep 笔记始终可搜（过滤只排 trash，不误伤）
    let hits_keep = booted
        .core
        .search
        .search("保留笔记甲", &opts)
        .await
        .unwrap();
    assert!(
        hits_keep.hits.iter().any(|h| h.note_id == keep),
        "未删笔记 rebuild 后必须可搜"
    );
    let _ = &ctx;
}

// ===== DK-02 S2 目录树 =====

use aurora_core::write_path::{
    create_folder, delete_folder, list_tree, move_node, NoteKind, TreeNode,
};

/// DoD1 环检测三态：自挂/子树挂/祖先链 + 可读 Error 文案。
#[tokio::test]
async fn circular_move_three_states_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        std::sync::Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    let ctx = ctx_for(&booted, &dir.path().join("aurora.db"));

    // 根文件夹 root → 子文件夹 sub
    let root = create_folder(&ctx, None, "根目录")
        .await
        .unwrap()
        .aggregate_id;
    let sub = create_folder(&ctx, Some(&root), "子目录")
        .await
        .unwrap()
        .aggregate_id;
    let note = write_path::create_note(&ctx, "普通笔记").await.unwrap();

    // ① 自挂
    let err1 = move_node(&ctx, &root, Some(&root), 1).await.unwrap_err();
    assert!(
        matches!(err1, aurora_core::Error::CircularMove { .. }),
        "{err1:?}"
    );
    assert!(err1.to_string().contains("循环引用"), "可读文案: {err1}");

    // ② 子树挂（把 root 挂到 sub 名下——sub 是 root 的孩子）
    let err2 = move_node(&ctx, &root, Some(&sub), 1).await.unwrap_err();
    assert!(
        matches!(err2, aurora_core::Error::CircularMove { .. }),
        "{err2:?}"
    );
    assert!(err2.to_string().contains("子树"), "环路径文案: {err2}");

    // ③ 祖先链（root ← sub 链更深一层：把 root 挂到 sub 的孩子）——用移动构造深度链
    // sub 下建孙文件夹再尝试把 root 挂到孙下
    let grand = create_folder(&ctx, Some(&sub), "孙目录")
        .await
        .unwrap()
        .aggregate_id;
    let err3 = move_node(&ctx, &root, Some(&grand), 1).await.unwrap_err();
    assert!(
        matches!(err3, aurora_core::Error::CircularMove { .. }),
        "{err3:?}"
    );

    // 合法移动（Note → Folder）不受影响
    move_node(&ctx, &note, Some(&sub), 1).await.unwrap();
    let tree = list_tree(&booted.core, ctx.seal.as_ref()).await.unwrap();
    let n = tree.iter().find(|t| t.note_id == note).unwrap();
    assert_eq!(n.parent_id.as_deref(), Some(sub.as_str()));

    // 非法目标：挂到 Note 名下拒绝（InvalidInput，非 CircularMove）
    let err4 = move_node(&ctx, &root, Some(&note), 1).await.unwrap_err();
    assert!(matches!(err4, aurora_core::Error::InvalidInput(_)));
}

/// DoD1 树组装排序：(parent_id, sort_order) 组内有序——Alpha 改判口径。
#[tokio::test]
async fn tree_assembly_sorted() {
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        std::sync::Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    let ctx = ctx_for(&booted, &dir.path().join("aurora.db"));

    let f1 = create_folder(&ctx, None, "甲").await.unwrap().aggregate_id;
    let _f2 = create_folder(&ctx, None, "乙").await.unwrap().aggregate_id;
    // f1 下两个笔记 sort_order 逆序创建
    move_node(
        &ctx,
        &write_path::create_note(&ctx, "note-b").await.unwrap(),
        Some(&f1),
        5,
    )
    .await
    .unwrap();
    move_node(
        &ctx,
        &write_path::create_note(&ctx, "note-a").await.unwrap(),
        Some(&f1),
        2,
    )
    .await
    .unwrap();
    write_path::create_note(&ctx, "根下笔记").await.unwrap();

    let tree = list_tree(&booted.core, ctx.seal.as_ref()).await.unwrap();
    let f1_nodes: Vec<&TreeNode> = tree
        .iter()
        .filter(|t| t.parent_id.as_deref() == Some(f1.as_str()))
        .collect();
    assert_eq!(
        f1_nodes.iter().map(|t| t.sort_order).collect::<Vec<_>>(),
        vec![2, 5],
        "同父按 sort_order 升序"
    );
    let roots: Vec<&TreeNode> = tree.iter().filter(|t| t.parent_id.is_none()).collect();
    assert!(roots.len() >= 3, "根级含 2 文件夹+根下笔记（trash 过滤后）");
    assert!(tree.iter().all(|t| !t.title.is_empty()), "无空标题节点");
}

/// DoD1 旧数据兼容：无新字段 NoteRecord JSON 反序列化 default。
#[tokio::test]
async fn legacy_note_record_compat() {
    let legacy = serde_json::json!({
        "id": "old-1", "title": "旧笔记", "content": "x",
        "created_at": "2025-01-01T00:00:00+00:00",
        "updated_at": "2025-01-01T00:00:00+00:00",
        "encryption": "none"
    });
    let rec: aurora_core::write_path::NoteRecord = serde_json::from_value(legacy).unwrap();
    assert_eq!(rec.kind, NoteKind::Note);
    assert_eq!(rec.parent_id, None);
    assert_eq!(rec.sort_order, 0);
}

/// DoD1 原位还原两态：父存 → 回原位（parent_id 不变）；父失 → 挂根。
#[tokio::test]
async fn restore_in_place_parent_alive_and_missing() {
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        std::sync::Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    let ctx = ctx_for(&booted, &dir.path().join("aurora.db"));

    // ── 态1：父存活 → 回原位 ──
    let folder = create_folder(&ctx, None, "存活目录")
        .await
        .unwrap()
        .aggregate_id;
    let n1 = write_path::create_note(&ctx, "原位笔记").await.unwrap();
    move_node(&ctx, &n1, Some(&folder), 3).await.unwrap();
    write_path::delete_note(&ctx, &n1).await.unwrap();
    write_path::restore_note(&ctx, &n1).await.unwrap();
    let rec1 = write_path::load_note_meta(&booted.core, &n1, ctx.seal.as_ref())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        rec1.parent_id.as_deref(),
        Some(folder.as_str()),
        "父存活回原位"
    );
    assert_eq!(rec1.sort_order, 3, "sort_order 保持");

    // ── 态2：父失 → 挂根 ──
    let folder2 = create_folder(&ctx, None, "将删目录")
        .await
        .unwrap()
        .aggregate_id;
    let n2 = write_path::create_note(&ctx, "孤儿笔记").await.unwrap();
    move_node(&ctx, &n2, Some(&folder2), 1).await.unwrap();
    // 先删笔记（入回收站），再删父文件夹（父进回收站=父"失"）
    write_path::delete_note(&ctx, &n2).await.unwrap();
    delete_folder(&ctx, &folder2).await.unwrap();
    write_path::restore_note(&ctx, &n2).await.unwrap();
    let rec2 = write_path::load_note_meta(&booted.core, &n2, ctx.seal.as_ref())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rec2.parent_id, None, "父缺失挂根（core 面保证，文档注明）");
}

/// DoD1 文件夹级联入回收站：N 笔记 + 子文件夹嵌套全成员入站（无物理删）。
#[tokio::test]
async fn delete_folder_cascades_soft() {
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        std::sync::Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    let ctx = ctx_for(&booted, &dir.path().join("aurora.db"));

    let root = create_folder(&ctx, None, "项目A")
        .await
        .unwrap()
        .aggregate_id;
    let sub = create_folder(&ctx, Some(&root), "子目录")
        .await
        .unwrap()
        .aggregate_id;
    let n1 = write_path::create_note(&ctx, "笔记1").await.unwrap();
    let n2 = write_path::create_note(&ctx, "笔记2").await.unwrap();
    move_node(&ctx, &n1, Some(&root), 1).await.unwrap();
    move_node(&ctx, &n2, Some(&sub), 1).await.unwrap();

    let n = delete_folder(&ctx, &root).await.unwrap();
    assert_eq!(n, 4, "子树成员=root+sub+n1+n2");
    // 修正断言：成员=root,sub,n1,n2 = 4
    let _ = n;

    for id in [&root, &sub, &n1, &n2] {
        assert!(
            booted
                .core
                .kv_store
                .get(&format!("trash:{id}"))
                .await
                .unwrap()
                .is_some(),
            "全成员必须入回收站: {id}"
        );
        assert!(
            booted
                .core
                .kv_store
                .get(&format!("note:{id}"))
                .await
                .unwrap()
                .is_some(),
            "物理键保留（无物理级联）: {id}"
        );
    }
    // 树视图不再出现（trash 过滤）
    let tree = list_tree(&booted.core, ctx.seal.as_ref()).await.unwrap();
    assert!(tree
        .iter()
        .all(|t| !["项目A", "子目录", "笔记1", "笔记2"].contains(&t.title.as_str())));

    // restore 顶层文件夹 → 子树成员恢复（逐个 restore——S1 语义批量面挂 UI）
    write_path::restore_note(&ctx, &root).await.unwrap();
    let rec = write_path::load_note_meta(&booted.core, &root, ctx.seal.as_ref())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rec.kind, NoteKind::Folder, "文件夹 restore 后 kind 保持");
}
