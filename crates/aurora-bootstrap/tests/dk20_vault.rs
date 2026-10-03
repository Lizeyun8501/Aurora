//! DK-20 单篇笔记加密 Vault — 行为级测试（锁定全链不可见 / fail-closed / 解锁 round-trip）。
//!
//! 口径（开工令教训注入）：测试住本 crate；fail-closed C03；写入侧跳过方案；
//! serde default 行为级断言（非仅注解）。

use aurora_bootstrap::bootstrap;
use aurora_core::blocks::BlockStore;
use aurora_core::write_path::{
    load_note_meta, open_note_content, save_note_content, set_note_encryption, WriteContext,
    ENC_AES256GCM, ENC_NONE,
};
use std::path::Path;
use std::sync::Arc;

fn ctx_for(booted: &aurora_bootstrap::BootedApp, db: &Path) -> WriteContext {
    let blocks = BlockStore::open(db).map(Arc::new);
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
        seal: Some(aurora_core::write_path::SealPair {
            seal: Box::new(seal),
            unseal: Box::new(unseal),
        }),
        content_cipher: Some(booted.content_cipher.clone()),
        attachments: Some(booted.attachments.clone()),
    }
}

async fn make_note(ctx: &WriteContext, title: &str, body: &str) -> String {
    let id = aurora_core::write_path::create_note(ctx, title)
        .await
        .unwrap();
    save_note_content(ctx, &id, body).await.unwrap();
    id
}

/// T1 锁定全链不可见：content 密文落库 + FTS 零命中 + 向量无残留 + blocks 无明文块 + 导出（load 直出）非明文。
#[tokio::test]
async fn locked_note_invisible_everywhere() {
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    let ctx = ctx_for(&booted, &dir.path().join("aurora.db"));
    let secret = "北极星行动密码 X7-99";
    let id = make_note(&ctx, "机密", secret).await;

    // 锁定
    set_note_encryption(&ctx, &id, ENC_AES256GCM).await.unwrap();

    // 1) content 密文落库（enc1: 前缀 + 原文零泄漏）
    let rec = load_note_meta(&booted.core, &id, ctx.seal.as_ref())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rec.encryption, ENC_AES256GCM);
    assert!(
        rec.content.starts_with("enc1:"),
        "锁定后落库必须是 enc1 密文: {}",
        &rec.content[..16.min(rec.content.len())]
    );
    assert!(!rec.content.contains("X7-99"), "明文泄漏");

    // 2) FTS 零命中（写入侧跳过——锁定篇不入索引；先推进投影消费锁定的 reindex 移除）
    booted.core.catch_up_projections().await.unwrap();
    let hits = booted
        .core
        .search
        .search(
            "X7-99",
            &aurora_core::traits::search_backend::SearchOptions::default(),
        )
        .await
        .unwrap();
    assert!(
        !hits.hits.iter().any(|h| h.note_id == id),
        "锁定后 FTS 仍命中明文 = 泄漏"
    );

    // 3) 向量缓存无残留（锁定时删除 + 防御过滤）
    assert!(
        booted
            .core
            .kv_store
            .get(&format!("notevec:{id}"))
            .await
            .unwrap()
            .is_none(),
        "锁定后向量缓存残留"
    );

    // 4) blocks 无明文块（锁定清空 + 写入侧跳过）
    let blocks = ctx.blocks.as_ref().unwrap();
    assert_eq!(
        blocks.list_note_blocks(&id).unwrap().len(),
        0,
        "锁定后 blocks 残留明文块"
    );

    // 5) 「导出」= KV 直读字节非明文（导出路径 load_note_meta 直出密文）
    let raw = booted
        .core
        .kv_store
        .get(&format!("note:{id}"))
        .await
        .unwrap()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&raw).contains("X7-99"),
        "导出字节泄漏明文"
    );
}

/// T2 fail-closed（C03）：密文篡改拒绝返回明文；非法级别拒绝；重复锁定幂等。
#[tokio::test]
async fn fail_closed_on_tamper_and_invalid() {
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    let ctx = ctx_for(&booted, &dir.path().join("aurora.db"));
    let id = make_note(&ctx, "账本", "金额 42").await;
    set_note_encryption(&ctx, &id, ENC_AES256GCM).await.unwrap();

    // 篡改密文 → open/decrypt 一律 Err（不返回内容）
    let mut rec = load_note_meta(&booted.core, &id, ctx.seal.as_ref())
        .await
        .unwrap()
        .unwrap();
    let mut tampered = rec.content.clone();
    tampered.insert_str(5, "ff"); // 破坏 hex nonce 域
    rec.content = tampered;
    assert!(
        open_note_content(&ctx, &id, &rec).is_err(),
        "篡改密文必须拒绝"
    );

    // 重复锁定幂等（同级别短路）
    set_note_encryption(&ctx, &id, ENC_AES256GCM).await.unwrap();

    // 非法级别拒绝
    let err = set_note_encryption(&ctx, &id, "rot13").await.unwrap_err();
    assert!(matches!(err, aurora_core::Error::InvalidInput(_)));
}

/// T3 解锁 round-trip 逐字节一致 + 解锁后恢复可检索/块重建。
#[tokio::test]
async fn unlock_roundtrip_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    let ctx = ctx_for(&booted, &dir.path().join("aurora.db"));
    let body = "# 周报\n\n本周完成 DK-20 锁定语义。\n- [ ] 补测试\n";
    let id = make_note(&ctx, "周记", body).await;

    set_note_encryption(&ctx, &id, ENC_AES256GCM).await.unwrap();
    set_note_encryption(&ctx, &id, ENC_NONE).await.unwrap();

    let rec = load_note_meta(&booted.core, &id, ctx.seal.as_ref())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rec.encryption, ENC_NONE);
    assert_eq!(rec.content, body, "解锁 round-trip 必须逐字节一致");

    // 解锁后重进索引可检索（推进投影消费 NoteMetadataChanged → reindex）
    booted.core.catch_up_projections().await.unwrap();
    let hits = booted
        .core
        .search
        .search(
            "DK-20",
            &aurora_core::traits::search_backend::SearchOptions::default(),
        )
        .await
        .unwrap();
    assert!(
        hits.hits.iter().any(|h| h.note_id == id),
        "解锁后应恢复可检索"
    );

    // 解锁后 blocks 重派生
    let blocks = ctx.blocks.as_ref().unwrap();
    assert!(
        !blocks.list_note_blocks(&id).unwrap().is_empty(),
        "解锁后 blocks 应重建"
    );
}

/// T4 serde default 行为级：无 encryption 字段的存量 KV JSON 反序列化 = "none"（非仅注解）。
#[tokio::test]
async fn serde_default_encryption_field() {
    let legacy = r#"{"id":"n1","title":"t","content":"c","created_at":"","updated_at":""}"#;
    let rec: aurora_core::write_path::NoteRecord = serde_json::from_str(legacy).unwrap();
    assert_eq!(rec.encryption, "none", "存量数据缺字段必须 default none");
    // 锁定值 round trip
    let locked: aurora_core::write_path::NoteRecord = serde_json::from_str(&legacy.replace(
        "\"updated_at\":\"\"",
        "\"updated_at\":\"\",\"encryption\":\"aes256gcm\"",
    ))
    .unwrap();
    assert_eq!(locked.encryption, ENC_AES256GCM);
}

/// T5 创建即锁（create→set 全链在同会话）：保存内容自动加密、事件后投影不重灌密文。
#[tokio::test]
async fn create_then_lock_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let booted = bootstrap(
        dir.path(),
        Arc::new(aurora_sync::sync_gate::AlwaysUnmetered),
    )
    .unwrap();
    let ctx = ctx_for(&booted, &dir.path().join("aurora.db"));
    let id = make_note(&ctx, "日记", "私密内容 abc").await;
    set_note_encryption(&ctx, &id, ENC_AES256GCM).await.unwrap();
    // 锁定后继续 save：新内容也必须密文落库
    save_note_content(&ctx, &id, "第二段私密 def")
        .await
        .unwrap();
    let rec = load_note_meta(&booted.core, &id, ctx.seal.as_ref())
        .await
        .unwrap()
        .unwrap();
    assert!(
        rec.content.starts_with("enc1:") && !rec.content.contains("def"),
        "锁定态增量保存泄漏明文"
    );
}
