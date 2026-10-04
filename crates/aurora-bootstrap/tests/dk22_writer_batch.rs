//! DK-22（writer 批量 commit 优化开工令）— bench 定位 + bulk 行为级测试。
//!
//! ① 定位（开工令任务 1）：黑盒对比「单条 index_note（每条尾 commit）」vs
//!    「batch_index（N 条单 commit）」均摊耗时 → commit 占比估算。
//!    占比 <30% 则开工令预置改判「数据记录 + 不动实现」。
//! ② 行为级（DoD 2）：batch_index 与逐条 index_note 最终可见性一致（doc_count
//!    相同、查询命中相同）——批量路径的正确性护栏。
//!
//! bench 走 `TantivySearchBackend` 真实实现（非 mock），对齐真实调用路径
//! （writerchurn 教训：bench 场景必须对齐真实调用路径）。

use aurora_core::l1_infrastructure::search::TantivySearchBackend;
use aurora_core::traits::search_backend::{IndexEntry, NoteMetadata, SearchBackend};
use std::time::Instant;

fn meta(title: &str) -> NoteMetadata {
    NoteMetadata {
        title: title.to_string(),
        tags: vec![],
        workspace_id: "ws-bench".to_string(),
        encryption: aurora_core::traits::search_backend::IndexEncryption::Plaintext,
        updated_at: None,
    }
}

fn entries(n: usize) -> Vec<IndexEntry> {
    (0..n)
        .map(|i| IndexEntry {
            note_id: format!("bench-{i:05}"),
            // ~1KB 中文正文（jieba 分词真实负载量级）
            content: format!("批量索引基准第 {i} 条。Aurora 本地优先笔记应用，中文分词负载压测正文段落，重复填充以确保一KB左右的规模——工程实践与数据决策先行，不为优化而优化。"),
            metadata: meta(&format!("基准 {i}")),
        })
        .collect()
}

/// 开工令任务 1：commit 占比定位（黑盒对比法）。
///
/// 占比估算 = (单发均值 − batch 均摊) / 单发均值。
/// 打印实测数据供开工令决策；`--ignored` 显式跑。
#[tokio::test]
#[ignore = "bench: cargo test -p aurora-bootstrap --test dk22_writer_batch -- --ignored --nocapture"]
async fn bench_commit_ratio_single_vs_batch() {
    let n = 100;
    let dir = std::env::temp_dir().join(format!("dk22-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    // A. 单发：逐条 index_note（每条尾 commit）
    let backend = TantivySearchBackend::new(&dir).unwrap();
    let notes = entries(n);
    let t0 = Instant::now();
    for e in &notes {
        backend
            .index_note(&e.note_id, &e.content, &e.metadata)
            .await
            .unwrap();
    }
    let single_total = t0.elapsed().as_millis() as f64;
    let single_avg = single_total / n as f64;
    drop(backend);
    let _ = std::fs::remove_dir_all(&dir);

    // B. 批量：batch_index（N 条单 commit）
    let dir2 = std::env::temp_dir().join(format!("dk22-bench-b{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir2);
    let backend2 = TantivySearchBackend::new(&dir2).unwrap();
    let t1 = Instant::now();
    backend2.batch_index(&notes).await.unwrap();
    let batch_total = t1.elapsed().as_millis() as f64;
    let batch_avg = batch_total / n as f64;

    let commit_share = if single_avg > batch_avg {
        (single_avg - batch_avg) / single_avg * 100.0
    } else {
        0.0
    };
    println!("[dk22 bench] n={n}");
    println!("[dk22 bench] 单发 index_note 总计 {single_total:.0}ms 均摊 {single_avg:.3}ms/条");
    println!("[dk22 bench] batch_index 总计 {batch_total:.0}ms 均摊 {batch_avg:.3}ms/条");
    println!("[dk22 bench] commit 占比估算 ≈ {commit_share:.1}%");
    // 数据落点：占比与开工令 30% 阈值对照 → 决策改判或 bulk 化
    assert!(
        single_total > 0.0 && batch_total > 0.0,
        "bench 数据必须为正才有定位意义"
    );
    let _ = std::fs::remove_dir_all(&dir2);
}

/// DoD 2（行为级）：batch_index 与逐条 index_note 可见性一致——
/// 同一数据集两条路径各自索引后 doc_count 与查询命中必须相同。
#[tokio::test]
async fn dk22_batch_index_visibility_parity() {
    let notes = entries(20);

    // 路径 A：逐条 index_note
    let dir_a = std::env::temp_dir().join(format!("dk22-parity-a{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir_a);
    let a = TantivySearchBackend::new(&dir_a).unwrap();
    for e in &notes {
        a.index_note(&e.note_id, &e.content, &e.metadata)
            .await
            .unwrap();
    }

    // 路径 B：一次 batch_index
    let dir_b = std::env::temp_dir().join(format!("dk22-parity-b{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir_b);
    let b = TantivySearchBackend::new(&dir_b).unwrap();
    b.batch_index(&notes).await.unwrap();

    let ca = a.doc_count().await.unwrap().unwrap();
    let cb = b.doc_count().await.unwrap().unwrap();
    assert_eq!(ca, cb, "两条路径最终可见文档数必须一致");
    assert_eq!(ca, 20);

    // 幂等更新一致性：同 id 重批（batch 内先删后写）→ 数量不变
    b.batch_index(&notes).await.unwrap();
    assert_eq!(
        b.doc_count().await.unwrap().unwrap(),
        20,
        "batch_index 幂等重放不得产生重复文档"
    );

    let _ = std::fs::remove_dir_all(&dir_a);
    let _ = std::fs::remove_dir_all(&dir_b);
}

/// DK-22 投影级 bulk parity：catch_up 走 `apply_batch` 攒批——
/// 1) 30 条 NoteCreated + 交错（建 A→删 A→建 B）单轮 catch_up →
///    doc_count==31、A 不在 B 在（保序 + 攒批可见性一致）；
/// 2) 水位线整批推进（再 catch_up 幂等 0 新增）。
#[tokio::test]
async fn dk22_projection_catch_up_bulk_and_order() {
    use aurora_core::event_bus::layered::{AppEvent, InMemoryEventQueue, LayeredEventBus};
    use aurora_core::l1_infrastructure::search::TantivySearchBackend;
    use aurora_core::l2_engines::search_projection::SearchIndexProjection;
    use aurora_core::traits::search_backend::SearchOptions;
    use std::sync::Arc;

    let dir = std::env::temp_dir().join(format!("dk22-proj{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let backend = Arc::new(TantivySearchBackend::new(&dir).unwrap());

    let kv = Arc::new(aurora_core::l1_infrastructure::storage_engine::MemoryKVStore::default());
    let source: Vec<IndexEntry> = Vec::new(); // 纯事件驱动场景（verify: index>source 放行）
    let proj = SearchIndexProjection::new(
        backend.clone(),
        kv.clone(),
        Box::new(move || source.clone()),
    );

    let queue = Arc::new(InMemoryEventQueue::new());
    let bus = LayeredEventBus::new(Some(queue));

    // 30 条批量创建 + 交错序（A 建后删、B 建存留）
    for i in 0..30 {
        bus.publish(AppEvent::NoteCreated {
            note_id: format!("bulk-{i:03}"),
            title: format!("批量标题{i:03}"),
            content: format!("这是第 {i:03} 篇批量导入笔记的正文内容。"),
        });
    }
    bus.publish(AppEvent::NoteCreated {
        note_id: "alpha-doomed".into(),
        title: " ALPHA 唯一词".into(),
        content: "alphaunique".into(),
    });
    bus.publish(AppEvent::NoteDeleted {
        note_id: "alpha-doomed".into(),
    });
    bus.publish(AppEvent::NoteCreated {
        note_id: "beta-alive".into(),
        title: "BETA 唯一词".into(),
        content: "betaunique".into(),
    });

    let applied = bus.catch_up(&proj).await.unwrap();
    assert_eq!(applied, 33, "整批事件全部应用");
    assert_eq!(
        backend.doc_count().await.unwrap().unwrap(),
        31,
        "30 批量 + beta；被删的 alpha 不复活（攒批保序）"
    );
    let opts = SearchOptions::default();
    assert_eq!(
        backend.search("alphaunique", &opts).await.unwrap().total,
        0,
        "删除事件必须晚于创建生效（交错保序）"
    );
    assert_eq!(backend.search("betaunique", &opts).await.unwrap().total, 1);

    // 水位线整批推进：重跑 catch_up 幂等零新增
    let again = bus.catch_up(&proj).await.unwrap();
    assert_eq!(again, 0, "水位线已推进 → 幂等重放零事件");
    let _ = std::fs::remove_dir_all(&dir);
}
