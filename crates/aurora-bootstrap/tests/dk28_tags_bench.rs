//! DK-28 — tags 投影启动成本实测（数据决策卡）。
//!
//! 口径：**release**（`cargo test -p aurora-bootstrap --test dk28_tags_bench
//! --release --ignored --nocapture`），10k 笔记量级 KV（SqliteStorage）全量扫
//! → TagsProjection rebuild 总耗时。
//! 决策线：**<100ms 文档化免优化**；超标才评估增量方案（不做无数据优化）。

use aurora_core::event_bus::projection::Projection;
use aurora_core::l1_infrastructure::storage::SqliteStorage;
use aurora_core::l2_engines::tags_projection::TagsProjection;
use aurora_core::traits::kv_store::KVStore;
use aurora_core::write_path::NoteRecord;
use std::sync::Arc;
use std::time::Instant;

#[tokio::test]
#[ignore = "bench: cargo test -p aurora-bootstrap --test dk28_tags_bench --release --ignored --nocapture"]
async fn dk28_tags_startup_bench_10k() {
    let dir = tempfile::tempdir().unwrap();
    let kv = Arc::new(SqliteStorage::new(dir.path().join("bench.db")).expect("sqlite open"));

    // 10k 笔记 NoteRecord JSON 落 KV（含 30% 带标签——真实分布近似）
    let n = 10_000usize;
    let t_seed = Instant::now();
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
    let seed_ms = t_seed.elapsed().as_millis();

    // 计时主体：TagsProjection rebuild（= 生产启动路径：全量扫 + 映射构建）
    let t0 = Instant::now();
    let kv_for_src = kv.clone();
    let proj = TagsProjection::new(
        kv.clone(),
        Box::new(move || {
            // 同步扫生产 KV——bootstrap 装配口径（thread_scope + rt）
            let kv_for_src = kv_for_src.clone();
            std::thread::scope(|s| {
                s.spawn(move || {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map(|rt| {
                            rt.block_on(async {
                                kv_for_src
                                    .scan_prefix("note:")
                                    .await
                                    .unwrap_or_default()
                                    .iter()
                                    .filter_map(|(k, b)| {
                                        let note_id = k.strip_prefix("note:")?.to_string();
                                        let rec: NoteRecord = serde_json::from_slice(b).ok()?;
                                        Some((note_id, rec.tags))
                                    })
                                    .collect::<Vec<_>>()
                            })
                        })
                        .unwrap_or_default()
                })
                .join()
                .unwrap_or_default()
            })
        }),
    );
    proj.rebuild().await.unwrap();
    let rebuild_ms = t0.elapsed().as_millis();

    let total = proj.row_count();
    println!("[dk28 bench] n={n} seed_ms={seed_ms}");
    println!(
        "[dk28 bench] tags rebuild 总耗时 {rebuild_ms}ms（KV 全量扫 + JSON 解析 + 映射构建；tag 行数 {total}）"
    );
    assert_eq!(total, 10_000, "10k 行全部入映射（含空集行）");
}
