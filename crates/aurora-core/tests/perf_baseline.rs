//! RV-01 性能基准三件套（Aurora 侧）—— 把「性能上限」变成可引用的数据。
//!
//! 基准面：
//! 1. `bench_index_rebuild_10k` —— 万级索引重建耗时（冷启动代理：ADR-004 搜索索引为派生，
//!    启动时自动重建，重建耗时即冷启动主要成分）
//! 2. `bench_search_10k_p50_p99` —— 万级笔记全文检索 P50/P99（V26 指标 <200ms）
//! 3. `bench_sync_converge` —— 双端收敛（MockSyncBus serialize→publish→drain→restore 往返）
//!
//! 运行（release 显式跑，常规 `cargo test` 跳过）：
//! `cargo test --release -p aurora-core --test perf_baseline -- --ignored --nocapture`
//!
//! 数据集：确定性伪随机（LCG，可复现）；中文正文（jieba 分词真实负载）；
//! 10% 长文档（2000-5000 字）；1k/5k/10k 三档。

use std::time::Instant;

mod common;

use aurora_core::l1_infrastructure::search::TantivySearchBackend;
use aurora_core::traits::search_backend::{NoteMetadata, SearchBackend, SearchOptions};
use chrono::Utc;

use common::MockSyncBus;

// ==================== 数据集生成器 ====================

/// 确定性 LCG 伪随机（可复现基准——同参数同数据集）。
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 16
    }
}

const WORDS: &[&str] = &[
    "会议", "笔记", "项目", "计划", "评审", "检索", "性能", "基准", "架构", "设计", "存储",
    "索引", "同步", "加密", "知识", "网络", "任务", "依赖", "画布", "编辑器", "系统", "数据",
    "安全", "审计", "回收", "恢复", "目录", "层级", "标签", "智能",
];

/// 生成 count 篇笔记：(note_id, content, metadata)。
/// 10% 概率长文档（2000-5000 字）；正文由词表拼接（jieba 可切分）。
fn gen_dataset(count: usize) -> Vec<(String, String, NoteMetadata)> {
    let mut rng = Lcg(20260930);
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let long_doc = rng.next() % 10 == 0;
        let body_words = if long_doc { 800 + (rng.next() % 1200) as usize } else { 30 + (rng.next() % 120) as usize };
        let mut content = String::with_capacity(body_words * 6);
        for _ in 0..body_words {
            content.push_str(WORDS[(rng.next() as usize) % WORDS.len()]);
        }
        let title = format!("基准笔记{i:06}{}", WORDS[(rng.next() as usize) % WORDS.len()]);
        let meta = NoteMetadata {
            title: title.clone(),
            tags: vec!["bench".into()],
            workspace_id: "bench-ws".into(),
            encryption: aurora_core::traits::search_backend::IndexEncryption::Plaintext,
            updated_at: Some(Utc::now()),
        };
        out.push((format!("bench-{i:06}"), content, meta));
    }
    out
}

fn search_opts() -> SearchOptions {
    SearchOptions {
        limit: 20,
        offset: 0,
        workspace_filter: None,
        tag_filter: None,
        date_range: None,
    }
}

fn percentile(v: &mut Vec<f64>, p: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = ((v.len() as f64 - 1.0) * p).round() as usize;
    v[idx.min(v.len() - 1)]
}

// ==================== 基准 1：万级索引重建（冷启动代理） ====================

#[tokio::test]
#[ignore = "perf bench — cargo test --release -- --ignored 显式跑"]
async fn bench_index_rebuild_10k() {
    for count in [1_000usize, 5_000, 10_000] {
        let dataset = gen_dataset(count);
        let backend = TantivySearchBackend::new_in_memory().expect("in-memory index");
        let t0 = Instant::now();
        for (id, content, meta) in &dataset {
            backend.index_note(id, content, meta).await.expect("index");
        }
        let total = t0.elapsed().as_millis();
        println!(
            "[index-rebuild] notes={count} total={total}ms avg_per_note={:.3}ms",
            total as f64 / count as f64
        );
    }
}

// ==================== 基准 2：万级检索 P50/P99 ====================

#[tokio::test]
#[ignore = "perf bench — 显式跑"]
async fn bench_search_10k_p50_p99() {
    let count = 10_000;
    let dataset = gen_dataset(count);
    let backend = TantivySearchBackend::new_in_memory().expect("in-memory index");
    for (id, content, meta) in &dataset {
        backend.index_note(id, content, meta).await.expect("index");
    }
    // 100 次查询：单词/双词/三词混合（命中面宽窄不一）
    let queries: Vec<String> = (0..100)
        .map(|i| {
            let a = WORDS[(i * 7) % WORDS.len()];
            if i % 3 == 0 {
                format!("{a}")
            } else if i % 3 == 1 {
                format!("{a} {}", WORDS[(i * 13) % WORDS.len()])
            } else {
                format!("{a} {} {}", WORDS[(i * 13) % WORDS.len()], WORDS[(i * 29) % WORDS.len()])
            }
        })
        .collect();
    let opts = search_opts();
    let mut latencies = Vec::with_capacity(queries.len());
    for q in &queries {
        let t0 = Instant::now();
        let r = backend.search(q, &opts).await.expect("search");
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        assert!(!r.hits.is_empty(), "query {q:?} 命中为空——数据集/查询不匹配");
        latencies.push(ms);
    }
    println!(
        "[search-10k] n={} p50={:.2}ms p99={:.2}ms max={:.2}ms (V26 指标 <200ms)",
        queries.len(),
        percentile(&mut latencies, 0.50),
        percentile(&mut latencies, 0.99),
        latencies.iter().cloned().fold(0.0, f64::max),
    );
}

// ==================== 基准 3：双端收敛（MockSyncBus） ====================

#[test]
#[ignore = "perf bench — 显式跑"]
fn bench_sync_converge() {
    for count in [1_000usize, 5_000, 10_000] {
        let dataset = gen_dataset(count);
        let docs: Vec<_> = dataset
            .iter()
            .map(|(id, content, _)| common::make_document(id.as_str(), vec![common::make_text_block(content)]))
            .collect();
        let bus = MockSyncBus::new();
        let t0 = Instant::now();
        // 端 A 发布 → 端 B 排空恢复（收敛一轮）
        for d in &docs {
            bus.publish(MockSyncBus::serialize_doc(d));
        }
        let drained = bus.drain();
        let mut restored = 0usize;
        for bytes in &drained {
            let _doc = MockSyncBus::deserialize_doc(bytes);
            restored += 1;
        }
        let total = t0.elapsed().as_millis();
        assert_eq!(restored, count, "收敛后条数必须一致");
        println!(
            "[sync-converge] docs={count} total={total}ms avg_per_doc={:.3}ms",
            total as f64 / count as f64
        );
    }
}
