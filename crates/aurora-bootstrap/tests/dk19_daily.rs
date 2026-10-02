//! DK-19 每日笔记 — 行为级测试（幂等/模板/跨重启/模式/索引自愈）。

use aurora_bootstrap::bootstrap;
use aurora_core::blocks::BlockStore;
use aurora_core::write_path::{
    ensure_daily_note, get_daily_mode, get_daily_template, render_daily_template, set_daily_mode,
    set_daily_template, DailyNoteMode, NoteKind, WriteContext,
};

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
        seal: Some(write_path_seal(seal, unseal)),
        content_cipher: Some(booted.content_cipher.clone()),
        attachments: Some(booted.attachments.clone()),
    }
}

/// SealPair 构造小帮手（避免长字面量）。
fn write_path_seal(
    seal: impl Fn(&[u8]) -> Result<Vec<u8>, aurora_core::Error> + Send + Sync + 'static,
    unseal: impl Fn(&[u8]) -> Result<Vec<u8>, aurora_core::Error> + Send + Sync + 'static,
) -> aurora_core::write_path::SealPair {
    aurora_core::write_path::SealPair {
        seal: Box::new(seal),
        unseal: Box::new(unseal),
    }
}

/// T1 并发幂等：同日 5 路并发触发仅存在一篇（进程内互斥 + 索引去重）。
#[tokio::test]
async fn concurrent_ensure_creates_single_daily_note() {
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        std::sync::Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    let _ctx = ctx_for(&booted, &dir.path().join("aurora.db"));

    let mut handles = Vec::new();
    for _ in 0..5 {
        let c = ctx_for(&booted, &dir.path().join("aurora.db"));
        handles.push(tokio::spawn(async move {
            ensure_daily_note(&c, "2026-10-02").await
        }));
    }
    let mut created = 0usize;
    let mut ids = Vec::new();
    for h in handles {
        if let Some(r) = h.await.unwrap().unwrap() {
            ids.push(r.note_id.clone());
            if r.created {
                created += 1;
            }
        }
    }
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 1, "同日重复触发仅一篇每日笔记: {ids:?}");
    assert_eq!(created, 1, "仅一路真实创建");
}

/// T2 模板渲染：{{date}}/{{weekday}} 占位符 + 自定义模板。
#[tokio::test]
async fn template_rendering() {
    // 默认模板渲染（2026-10-02 是周五——枚举锚定）
    let rendered = render_daily_template("# {{date}} {{weekday}}\n\n", "2026-10-02");
    assert_eq!(rendered, "# 2026-10-02 星期五\n\n");
    // 自定义模板 + 非法日期回退星期日
    let custom = render_daily_template("日志 {{date}} / {{weekday}}", "2026-01-01");
    assert_eq!(custom, "日志 2026-01-01 / 星期四");
    let fallback = render_daily_template("{{weekday}}", "bad-date");
    assert_eq!(fallback, "星期日");
    // 模板存取
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        std::sync::Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    set_daily_template(&booted.core, "周报 {{weekday}}")
        .await
        .unwrap();
    assert_eq!(
        get_daily_template(&booted.core).await.unwrap(),
        "周报 {{weekday}}"
    );
}

/// T3 跨重启后当日笔记仍唯一且可定位（daily:{date} 索引 + note 持久化）。
#[tokio::test]
async fn daily_note_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let key = std::sync::Arc::new(aurora_sync::sync_gate::AlwaysUnmetered);
    {
        let booted = bootstrap(dir.path(), key.clone()).unwrap();
        let ctx = ctx_for(&booted, &dir.path().join("aurora.db"));
        let r = ensure_daily_note(&ctx, "2026-10-02")
            .await
            .unwrap()
            .unwrap();
        assert!(r.created);
        let rec =
            aurora_core::write_path::load_note_meta(&booted.core, &r.note_id, ctx.seal.as_ref())
                .await
                .unwrap()
                .unwrap();
        assert_eq!(rec.kind, NoteKind::DailyNote);
        assert_eq!(rec.daily_date.as_deref(), Some("2026-10-02"));
    }
    // 重启
    let booted2 = bootstrap(dir.path(), key).unwrap();
    let ctx2 = ctx_for(&booted2, &dir.path().join("aurora.db"));
    let r2 = ensure_daily_note(&ctx2, "2026-10-02")
        .await
        .unwrap()
        .unwrap();
    assert!(!r2.created, "重启后定位命中而非重建");
    // 定位到的是同一篇（读旧库验证 title/kind）
    let rec2 =
        aurora_core::write_path::load_note_meta(&booted2.core, &r2.note_id, ctx2.seal.as_ref())
            .await
            .unwrap()
            .unwrap();
    assert_eq!(rec2.kind, NoteKind::DailyNote);
}

/// T4 模式语义：Manual 仅定位不创建；Off 拒绝；默认 Auto。
#[tokio::test]
async fn mode_semantics() {
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        std::sync::Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    let ctx = ctx_for(&booted, &dir.path().join("aurora.db"));

    // 默认 Auto
    assert_eq!(
        get_daily_mode(&booted.core).await.unwrap(),
        DailyNoteMode::Auto
    );

    // Manual：定位为空，不创建
    set_daily_mode(&booted.core, DailyNoteMode::Manual)
        .await
        .unwrap();
    assert!(ensure_daily_note(&ctx, "2026-10-02")
        .await
        .unwrap()
        .is_none());
    // Off：拒绝
    set_daily_mode(&booted.core, DailyNoteMode::Off)
        .await
        .unwrap();
    let err = ensure_daily_note(&ctx, "2026-10-02").await.unwrap_err();
    assert!(matches!(err, aurora_core::Error::InvalidInput(_)));
    // 切回 Auto 正常创建
    set_daily_mode(&booted.core, DailyNoteMode::Auto)
        .await
        .unwrap();
    let r = ensure_daily_note(&ctx, "2026-10-02")
        .await
        .unwrap()
        .unwrap();
    assert!(r.created);
}

/// T5 投影索引自愈：删除 daily:{date} 索引后 DailyNoteOpened 消费重建。
#[tokio::test]
async fn projection_heals_daily_index() {
    use aurora_core::event_bus::projection::Projection;
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        std::sync::Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    let ctx = ctx_for(&booted, &dir.path().join("aurora.db"));
    let r = ensure_daily_note(&ctx, "2026-10-02")
        .await
        .unwrap()
        .unwrap();

    // 破坏索引
    booted
        .core
        .kv_store
        .delete("daily:2026-10-02")
        .await
        .unwrap();
    // 投影消费 Opened 事件（事件载荷的 note_id 为准）→ 自愈
    let proj = aurora_core::event_bus::projection::DailyNoteProjection::new(booted.core.clone());
    proj.apply(
        &aurora_core::event_bus::layered::AppEvent::DailyNoteOpened {
            date: "2026-10-02".into(),
            note_id: r.note_id.clone(),
        },
    )
    .await
    .unwrap();
    let healed = booted.core.kv_store.get("daily:2026-10-02").await.unwrap();
    assert_eq!(
        healed.as_deref(),
        Some(r.note_id.as_bytes()),
        "投影自愈重建 daily 索引"
    );
}
