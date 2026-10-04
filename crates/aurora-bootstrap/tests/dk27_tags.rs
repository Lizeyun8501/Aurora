//! DK-27 — tags 投影 v2 + SmartFolder tags 条件 行为级测试。
//!
//! 覆盖 DoD：投影一致性（set_note_tags → 事件 → catch_up → tags_of 终态）/
//! tags 条件过滤正确性（include AND / exclude）/ 存量兼容（无 tags JSON
//! 反序列化空集——诚实化口径）。批量重建 parity 由
//! `aurora-core::l2_engines::tags_projection` 内联测试锚定。

use aurora_bootstrap::bootstrap;
use aurora_core::blocks::BlockStore;
use aurora_core::l2_engines::tags_projection::TagsProjection;
use aurora_core::write_path::{
    create_note, set_note_tags, update_rule, FilterRule, NoteKind, NoteRecord, WriteContext,
};

fn ctx_for(booted: &aurora_bootstrap::BootedApp, db: &std::path::Path) -> WriteContext {
    let blocks = BlockStore::open(db).map(std::sync::Arc::new);
    WriteContext {
        core: booted.core.clone(),
        blocks,
        seal: None, // mobile 明文口径——tags 链不涉加密封装
        content_cipher: None,
        attachments: None,
    }
}

/// 注册表内 downcast 出标签投影引用后执行断言闭包。
fn with_tags_proj(
    core: &std::sync::Arc<aurora_core::app_core::AppCore>,
    f: impl Fn(&TagsProjection),
) {
    let proj = core
        .projections()
        .iter()
        .find_map(|p| p.as_any().and_then(|a| a.downcast_ref::<TagsProjection>()))
        .expect("tags projection registered");
    f(proj);
}

/// catch_up 标签投影（注册表按 name 定位——事件驱动增量入口）。
async fn catch_tags(core: &std::sync::Arc<aurora_core::app_core::AppCore>) -> usize {
    let proj = core
        .projections()
        .iter()
        .find(|p| p.name() == "tags-index")
        .expect("tags projection registered")
        .clone();
    core.event_bus.catch_up(proj.as_ref()).await.unwrap()
}

/// 端到端一致性：set_note_tags（KV 写 + 事件）→ catch_up → 投影终态。
#[tokio::test]
async fn dk27_tags_projection_consistency() {
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        std::sync::Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    let ctx = ctx_for(&booted, &dir.path().join("aurora.db"));
    let note_id = create_note(&ctx, "标签笔记").await.unwrap();

    // 1) 设 tags ["rust","core"] → 事件驱动 catch_up
    set_note_tags(
        &booted.core,
        &note_id,
        vec!["rust".into(), "core".into()],
        None,
    )
    .await
    .unwrap();
    catch_tags(&booted.core).await;
    with_tags_proj(&booted.core, |p| {
        let got = p.tags_of(&note_id);
        assert!(got.contains(&"rust".to_string()) && got.contains(&"core".to_string()));
    });

    // 2) 覆盖收窄 ["rust"] → 终态（core 被移除）
    set_note_tags(&booted.core, &note_id, vec!["rust".into()], None)
        .await
        .unwrap();
    catch_tags(&booted.core).await;
    with_tags_proj(&booted.core, |p| {
        let got = p.tags_of(&note_id);
        assert!(got.contains(&"rust".to_string()) && !got.contains(&"core".to_string()));
    });

    // 3) KV 轻量读取源同落（一致性锚）
    let bytes = booted
        .core
        .kv_store
        .get(&format!("note:{note_id}"))
        .await
        .unwrap()
        .unwrap();
    let rec: NoteRecord = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rec.tags, vec!["rust".to_string()]);
}

/// tags 条件过滤正确性：include AND / exclude 任一含即否决 / 标题条件共存。
#[tokio::test]
async fn dk27_filter_tags_correctness() {
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        std::sync::Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    let ctx = ctx_for(&booted, &dir.path().join("aurora.db"));

    let folder = create_note(&ctx, "标签文件夹").await.unwrap();
    let a = create_note(&ctx, "Rust 入门").await.unwrap();
    let b = create_note(&ctx, "Rust 进阶").await.unwrap();
    let c = create_note(&ctx, "随手记").await.unwrap();
    set_note_tags(&booted.core, &a, vec!["rust".into(), "wip".into()], None)
        .await
        .unwrap();
    set_note_tags(&booted.core, &c, vec!["rust".into(), "todo".into()], None)
        .await
        .unwrap();
    let _ = b; // 无标签

    // kind 修正为 SmartFolder（create_note 默认 Note——直写 KV；必须在
    // update_rule 之前——其内置 kind==SmartFolder 校验）。
    let bytes = booted
        .core
        .kv_store
        .get(&format!("note:{folder}"))
        .await
        .unwrap()
        .unwrap();
    let mut rec: NoteRecord = serde_json::from_slice(&bytes).unwrap();
    rec.kind = NoteKind::SmartFolder;
    let payload = serde_json::to_vec(&rec).unwrap();
    booted
        .core
        .kv_store
        .set(&format!("note:{folder}"), &payload)
        .await
        .unwrap();

    // 智能文件夹 + 规则：include ["rust"]，exclude ["todo"]
    update_rule(
        &ctx,
        &folder,
        FilterRule {
            title_contains: None,
            tags_include: vec!["rust".into()],
            tags_exclude: vec!["todo".into()],
        },
    )
    .await
    .unwrap();

    let items = aurora_core::write_path::evaluate_smart_folder_for(&booted.core, None, &folder)
        .await
        .unwrap();
    let ids: Vec<&str> = items.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&a.as_str()), "含 rust 且不含 todo → 命中");
    assert!(!ids.contains(&c.as_str()), "含排除标签 todo → 否决");
    assert!(!ids.contains(&b.as_str()), "无标签不满足 include");
    assert!(!ids.contains(&folder.as_str()), "自身排除");
}

/// 存量兼容：无 tags 字段的旧 JSON 反序列化 → 空集（诚实化口径锚点）。
#[test]
fn dk27_serde_backward_compat() {
    let old = r#"{"id":"n1","title":"旧笔记","content":"","created_at":"","updated_at":""}"#;
    let rec: NoteRecord = serde_json::from_str(old).unwrap();
    assert!(
        rec.tags.is_empty(),
        "存量 JSON 无 tags → 空集（诚实化口径）"
    );
}
