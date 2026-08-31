//! V20 §6.2 性能门禁基准 — criterion（P1 / GAP-13）
//!
//! 映射关系（M0 指标 → 可本地回归的内核热点）:
//!
//! | V20 §6.2 门禁            | 本基准                          |
//! |--------------------------|---------------------------------|
//! | 笔记打开延迟 <100ms      | `storage_engine/atomic_write`   |
//! |                          | （单笔记持久化全链路，含 WAL）  |
//! | （事件吞吐支撑索引延迟）  | `event_bus/publish_medium`      |
//! |                          | `event_bus/publish_low`         |
//! |                          | `event_bus/ack_medium`          |
//!
//! 门禁接入方式: CI `cargo bench -p aurora-core -- --save-baseline main`
//! 后续运行 `--baseline main`，回归 >10% 告警（§6.2 阈值）。
//!
//! 注意: 基准跑在 dev 机器上，绝对值受硬件影响；门禁以**相对回归**为准。
//! async 方法（commit_atomic）经 `Runtime::block_on` 驱动（开销 ~ns 级，
//! 相对 fsync 的 ms 级可忽略）。

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};

use aurora_core::event_bus::layered::{AppEvent, LayeredEventBus, LinkAction};
use aurora_core::l1_infrastructure::atomic_transaction::AtomicTransaction;
use aurora_core::l1_infrastructure::storage_engine::{MemoryKVStore, StorageEngine};

fn bench_storage_engine(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut g = c.benchmark_group("storage_engine");
    g.throughput(Throughput::Elements(1));
    g.sample_size(50);

    // 单笔记原子写全链路（WAL + 元数据 + fsync + rename + 清理）
    g.bench_function("atomic_write_1kb", |b| {
        b.iter_batched(
            || {
                let dir = tempfile::tempdir().unwrap();
                let eng = StorageEngine::new(MemoryKVStore::default(), dir.path());
                (eng, dir, vec![b'x'; 1024])
            },
            |(eng, dir, content)| {
                let _ = rt.block_on(eng.commit_atomic(
                    black_box("bench-op"),
                    black_box("notes/bench.md"),
                    black_box(&content),
                    black_box(b"meta"),
                ));
                black_box(&dir);
            },
            criterion::BatchSize::PerIteration,
        )
    });

    // 只测 fs 层（tmp+fsync+rename），隔离 WAL 开销 — 热点归因用
    g.bench_function("fs_layer_atomic_write_1kb", |b| {
        b.iter_batched(
            || {
                let dir = tempfile::tempdir().unwrap();
                let at = AtomicTransaction::new(dir.path());
                (at, dir, vec![b'y'; 1024])
            },
            |(at, dir, content)| {
                let _ = at.atomic_write(
                    black_box("notes/bench.md"),
                    black_box(&content),
                    black_box("bench-op"),
                );
                black_box(&dir);
            },
            criterion::BatchSize::PerIteration,
        )
    });
    g.finish();
}

fn bench_event_bus(c: &mut Criterion) {
    let mut g = c.benchmark_group("event_bus");

    // Medium 发布（含持久化 enqueue — ARCH-003 路径）
    g.bench_function("publish_medium_with_store", |b| {
        use aurora_core::event_bus::layered::InMemoryEventQueue;
        b.iter_batched(
            || {
                let store = std::sync::Arc::new(InMemoryEventQueue::new());
                LayeredEventBus::new(Some(store))
            },
            |bus| {
                bus.publish(black_box(AppEvent::BidiLinkChanged {
                    source_note_id: "src".into(),
                    target_note_id: "dst".into(),
                    action: LinkAction::Created,
                }));
                black_box(&bus);
            },
            criterion::BatchSize::PerIteration,
        )
    });

    // Low 发布（无持久化路径）
    g.bench_function("publish_low", |b| {
        b.iter(|| {
            let bus = LayeredEventBus::new(None);
            bus.publish(black_box(AppEvent::NoteCreated {
                note_id: "n".into(),
                title: "t".into(),
                content: String::new(),
            }));
        })
    });

    // ack_medium（水位线推进 + 积压扣减）
    g.bench_function("ack_medium", |b| {
        let bus = LayeredEventBus::new(None);
        let mut seq = 0u64;
        b.iter(|| {
            seq += 1;
            bus.ack_medium(black_box(seq));
        });
    });

    g.finish();
}

criterion_group!(benches, bench_storage_engine, bench_event_bus);
criterion_main!(benches);
