//! V26 DK-01W 同构对拍 — 桌面/移动端写路径一致性（I2 DoD）
//!
//! 桌面封装 = `WriteContext { seal: Some(vault pair) }`
//! 移动封装 = `WriteContext { seal: None }`（明文，加密统一为后续卡）
//!
//! 断言：同一操作序列下，两封装产出的 **key 集合、元数据字段、
//! blocks 派生、删除后状态** 全一致（seal 字节除外 — 同步层工作在
//! Loro oplog 层不受本机封装影响）。

use aurora_bootstrap::bootstrap;
use aurora_core::blocks::BlockStore;
use aurora_core::write_path::{self, NoteRecord, SealPair, WriteContext};

/// 端角色（desktop=seal Some / mobile=seal None）。
#[derive(Clone, Copy)]
enum Endpoint {
    Desktop,
    Mobile,
}

fn ctx_for(
    endpoint: Endpoint,
    booted: &aurora_bootstrap::BootedApp,
    db: &std::path::Path,
) -> WriteContext {
    let blocks = BlockStore::open(db).map(std::sync::Arc::new);
    let seal = match endpoint {
        Endpoint::Desktop => {
            let vault = booted.vault.clone();
            let vault2 = booted.vault.clone();
            let crypto = booted.core.crypto.clone();
            let crypto2 = crypto.clone();
            Some(SealPair {
                seal: Box::new(move |b| {
                    vault
                        .encrypt(crypto.as_ref(), b)
                        .map_err(|e| aurora_core::Error::Internal(e.to_string()))
                }),
                unseal: Box::new(move |b| {
                    vault2
                        .decrypt(crypto2.as_ref(), b)
                        .map_err(|e| aurora_core::Error::Internal(e.to_string()))
                }),
            })
        }
        Endpoint::Mobile => None,
    };
    WriteContext {
        core: booted.core.clone(),
        blocks,
        seal,
    }
}

async fn kv_keys(booted: &aurora_bootstrap::BootedApp, prefix: &str) -> Vec<String> {
    booted
        .core
        .kv_store
        .scan_prefix(prefix)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(k, _)| k)
        .collect()
}

async fn read_meta(
    endpoint: Endpoint,
    booted: &aurora_bootstrap::BootedApp,
    note_id: &str,
) -> Option<NoteRecord> {
    let core = booted.core.clone();
    let vault = booted.vault.clone();
    let crypto = core.crypto.clone();
    // 手动内联解封装（desktop unseal / mobile 直读）— 与 load_note_meta 同语义
    let bytes = core.kv_store.get(&format!("note:{note_id}")).await.ok()??;
    let plain = match endpoint {
        Endpoint::Desktop => vault
            .decrypt(crypto.as_ref(), &bytes)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
            .ok()?,
        Endpoint::Mobile => bytes,
    };
    serde_json::from_slice(&plain).ok()
}

#[tokio::test]
async fn desktop_and_mobile_write_paths_are_isomorphic() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let desktop = bootstrap(dir_a.path()).expect("desktop bootstrap");
    let mobile = bootstrap(dir_b.path()).expect("mobile bootstrap");

    let db_a = dir_a.path().join("aurora.db");
    let db_b = dir_b.path().join("aurora.db");
    let ctx_d = ctx_for(Endpoint::Desktop, &desktop, &db_a);
    let ctx_m = ctx_for(Endpoint::Mobile, &mobile, &db_b);

    // ── 同一操作序列：create → save → rename ──
    let id_d = write_path::create_note(&ctx_d, "对拍笔记").await.unwrap();
    let id_m = write_path::create_note(&ctx_m, "对拍笔记").await.unwrap();

    write_path::save_note_content(&ctx_d, &id_d, "# 标题\n\n- [ ] 行动项")
        .await
        .unwrap();
    write_path::save_note_content(&ctx_m, &id_m, "# 标题\n\n- [ ] 行动项")
        .await
        .unwrap();

    write_path::rename_note(&ctx_d, &id_d, "对拍笔记改名")
        .await
        .unwrap();
    write_path::rename_note(&ctx_m, &id_m, "对拍笔记改名")
        .await
        .unwrap();

    // ── 对拍 1: key 集合形态一致（note: + notesnap: 各 1）──
    let keys_d = kv_keys(&desktop, "note:").await;
    let keys_m = kv_keys(&mobile, "note:").await;
    assert_eq!(keys_d.len(), 1, "desktop note: keys = {keys_d:?}");
    assert_eq!(keys_m.len(), 1, "mobile note: keys = {keys_m:?}");
    assert_eq!(
        kv_keys(&desktop, "notesnap:").await.len(),
        1,
        "desktop notesnap"
    );
    assert_eq!(
        kv_keys(&mobile, "notesnap:").await.len(),
        1,
        "mobile notesnap"
    );

    // ── 对拍 2: 元数据字段一致（seal 往返后内容相同）──
    let meta_d = read_meta(Endpoint::Desktop, &desktop, &id_d)
        .await
        .expect("desktop meta");
    let meta_m = read_meta(Endpoint::Mobile, &mobile, &id_m)
        .await
        .expect("mobile meta");
    assert_eq!(meta_d.title, meta_m.title, "title isomorphic");
    assert_eq!(meta_d.title, "对拍笔记改名", "rename applied");
    assert_eq!(meta_d.content, meta_m.content, "content isomorphic");
    assert_eq!(meta_d.content, "# 标题\n\n- [ ] 行动项");
    assert!(!meta_d.updated_at.is_empty());
    assert!(!meta_m.updated_at.is_empty());

    // ── 对拍 3: blocks 派生一致（标题块 + 列表块）──
    {
        let blocks_d = BlockStore::open(&db_a).unwrap();
        let blocks_m = BlockStore::open(&db_b).unwrap();
        let n_d = blocks_d
            .list_note_blocks(&id_d)
            .map(|v| v.len())
            .unwrap_or(0);
        let n_m = blocks_m
            .list_note_blocks(&id_m)
            .map(|v| v.len())
            .unwrap_or(0);
        assert_eq!(n_d, n_m, "blocks derived isomorphic");
        assert!(n_d >= 2, "标题+列表至少两块, got {n_d}");
    }

    // ── 对拍 3.5: blocks 派生重建（DK-01 权威源 — notes 权威, blocks 派生）──
    {
        // 模拟 blocks 损坏/丢失: 清空两端 blocks 表
        let blocks_d = BlockStore::open(&db_a).unwrap();
        let blocks_m = BlockStore::open(&db_b).unwrap();
        {
            let conn = blocks_d.conn_handle();
            conn.execute("DELETE FROM blocks", []).unwrap();
        }
        {
            let conn = blocks_m.conn_handle();
            conn.execute("DELETE FROM blocks", []).unwrap();
        }
        // 权威重建
        let n_d = write_path::rebuild_blocks_derivation(&ctx_d).await.unwrap();
        let n_m = write_path::rebuild_blocks_derivation(&ctx_m).await.unwrap();
        assert_eq!(n_d, 1, "desktop rebuilt 1 note");
        assert_eq!(n_m, 1, "mobile rebuilt 1 note");
        let rd = blocks_d
            .list_note_blocks(&id_d)
            .map(|v| v.len())
            .unwrap_or(0);
        let rm = blocks_m
            .list_note_blocks(&id_m)
            .map(|v| v.len())
            .unwrap_or(0);
        assert_eq!(rd, rm, "rebuild isomorphic");
        assert!(rd >= 2, "rebuild 后块数恢复, got {rd}");
    }

    // ── 对拍 3.6: 投影启动策略决策（DK-01 水位线 vs 事件流对比）──
    {
        // 正常场景（水位线在事件流内）→ incremental
        let (strategy_d, _) = desktop.decide_projection_strategy(1000).await;
        assert_eq!(strategy_d, "incremental", "正常场景应增量");

        // 人为把水位线推到超前（模拟数据源回退）→ rebuild
        for p in desktop.core.projections() {
            if p.name() == "search-index" {
                p.set_watermark(u64::MAX / 2).await.unwrap();
            }
        }
        let (strategy_d2, why) = desktop.decide_projection_strategy(1000).await;
        assert_eq!(strategy_d2, "rebuild", "水位线超前应重建: {why}");
        desktop.search_projection_rebuild().await.unwrap();
        // 重建后水位线复位到事件流末端 → 恢复 incremental
        let (strategy_d3, _) = desktop.decide_projection_strategy(1000).await;
        assert_eq!(strategy_d3, "incremental", "重建后恢复增量");
    }

    // ── 对拍 4: delete 后两端 key 集合一致（全清）──
    write_path::delete_note(&ctx_d, &id_d).await.unwrap();
    write_path::delete_note(&ctx_m, &id_m).await.unwrap();
    assert_eq!(
        kv_keys(&desktop, "note:").await.len(),
        0,
        "desktop after delete"
    );
    assert_eq!(
        kv_keys(&mobile, "note:").await.len(),
        0,
        "mobile after delete"
    );
    assert_eq!(kv_keys(&desktop, "notesnap:").await.len(), 0);
    assert_eq!(kv_keys(&mobile, "notesnap:").await.len(), 0);
}
