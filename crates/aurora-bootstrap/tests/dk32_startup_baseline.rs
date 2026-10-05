//! DK-32 — 桌面启动链路耗时分解基线（Alpha 自立卡，数据决策卡）。
//!
//! 口径：**release**（`cargo test -p aurora-bootstrap --test dk32_startup_baseline
//! --release --ignored --nocapture`）。
//! 两口径：A=首建启动（空 data_dir 全新 bootstrap）；B=二次启动（存量 10k 笔记
//! 重建语义——**用户日常体感核心指标**）。
//! 分段方法：migration/vault 公开组件单独计时 + bootstrap 整体，差值 =
//! core 构建+startup+其余（口径注记，不改产品代码不插桩）。
//! 决策线（桌面应用惯例锚定）：二次启动 **<500ms 优 / <1s 良 / >1s 须优化**。

use aurora_bootstrap::bootstrap;
use aurora_core::traits::kv_store::KVStore;
use aurora_core::write_path::NoteRecord;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

/// 分段探针：migration + vault（公开构件），返回 (migration_ms, vault_ms)。
/// 注：**仅在独立临时库上探针**（不污染被测 data_dir——探针本身有独立开销，
/// 与 bootstrap 内部同构件同实现，数值可直接对照）。
fn probe_segments(tmp_parent: &Path) -> (u128, u128) {
    let probe_dir = tmp_parent.join("probe-seg");
    std::fs::create_dir_all(&probe_dir).unwrap();

    let t = Instant::now();
    let m = aurora_migration::MigrationManager::new(probe_dir.join("probe.db")).unwrap();
    m.migrate().unwrap();
    let migration_ms = t.elapsed().as_millis();

    let t = Instant::now();
    let _vault = aurora_security::LocalDekVault::load_or_create(&probe_dir.join("keys")).unwrap();
    let vault_ms = t.elapsed().as_millis();
    (migration_ms, vault_ms)
}

/// 10k 笔记注入（写路径 NoteRecord JSON，含 30% 带标签——与 dk28 同分布）。
async fn seed_10k(kv: &Arc<dyn KVStore>) {
    let n = 10_000usize;
    for i in 0..n {
        let tags: Vec<String> = if i % 10 < 3 {
            vec![format!("tag-{}", i % 7), "wip".into()]
        } else {
            vec![]
        };
        let rec = NoteRecord {
            id: format!("n{i:05}"),
            title: format!("笔记{i:05}"),
            content: String::new(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            encryption: "none".into(),
            parent_id: None,
            kind: aurora_core::write_path::NoteKind::Note,
            sort_order: 0,
            rule: None,
            daily_date: None,
            tags,
        };
        kv.set(&format!("note:n{i:05}"), &serde_json::to_vec(&rec).unwrap())
            .await
            .unwrap();
    }
}

#[tokio::test]
#[ignore = "bench: cargo test -p aurora-bootstrap --test dk32_startup_baseline --release --ignored --nocapture"]
async fn dk32_startup_baseline() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("workspace");

    // ── 口径 A：首建启动（全新 data_dir）────────────────────────────
    let t0 = Instant::now();
    let app_a = bootstrap(&data_dir, Arc::new(aurora_sync::sync_gate::AlwaysUnmetered)).unwrap();
    let first_boot_ms = t0.elapsed().as_millis();
    println!("[dk32] 首建启动总耗时            = {first_boot_ms} ms");

    // ── 注入 10k 笔记（存量语义准备）───────────────────────────────
    let t_seed = Instant::now();
    seed_10k(&app_a.core.kv_store).await;
    let seed_ms = t_seed.elapsed().as_millis();
    println!("[dk32] 10k 笔记注入（写路径）    = {seed_ms} ms");
    drop(app_a); // 释放 SQLite 句柄，二次启动干净衔接

    // ── 分段探针（独立临时库，不污染被测目录）──────────────────────
    let (mig_ms, vault_ms) = probe_segments(tmp.path());
    println!("[dk32] 分段·migration           = {mig_ms} ms");
    println!("[dk32] 分段·vault(DEK)          = {vault_ms} ms");

    // ── 口径 B：二次启动（存量 10k 重建——核心指标）─────────────────
    let t1 = Instant::now();
    let app_b = bootstrap(&data_dir, Arc::new(aurora_sync::sync_gate::AlwaysUnmetered)).unwrap();
    let warm_boot_ms = t1.elapsed().as_millis();
    println!("[dk32] 二次启动总耗时（10k 存量）= {warm_boot_ms} ms  ← 核心指标");

    // 差值口径：二次启动 - migration - vault ≈ core 构建+startup（投影
    // rebuild/tags 映射/索引 open）+ ai 预载 + boot 备份。粗粒度但免插桩。
    let core_side_ms = warm_boot_ms.saturating_sub(mig_ms + vault_ms);
    println!("[dk32] 差值·core+startup+其余    ≈ {core_side_ms} ms");

    // ── 存量数据完整性核验（二次启动后 10k 仍在）────────────────────
    let all = app_b.core.kv_store.scan_prefix("note:").await.unwrap();
    let count = all.len();
    println!("[dk32] 二次启动后存量核验        = {count}/10000 行");
    assert_eq!(count, 10_000, "二次启动后 10k 存量完整");

    // ── 决策线判定 ─────────────────────────────────────────────────
    let verdict = if warm_boot_ms < 500 {
        "优（<500ms）"
    } else if warm_boot_ms < 1000 {
        "良（<1s）"
    } else {
        "须优化（>1s）"
    };
    println!("[dk32] 决策线判定（<500 优/<1s 良/>1s 须优化）= {verdict}");
}
