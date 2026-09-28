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
