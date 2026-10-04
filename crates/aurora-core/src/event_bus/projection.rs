//! 投影（Projection）— V20 §3.2 / §4.5：单一事实源 + 投影读模型
//!
//! Loro OpLog 是唯一写入真相；SQLite / FTS5 / 双链图 / 任务视图 /
//! TodayView 全部是可从事件重放重建的投影（读侧）。
//!
//! # 语义
//!
//! - [`Projection::apply`] 必须**幂等**（同一事件重复应用结果一致）
//! - [`Projection::watermark`] 返回已消费的最大事件 seq（增量追赶起点）
//! - [`LayeredEventBus::catch_up`] 按水位线增量追赶；投影健康校验
//!   失败时自动触发全量重建（[`Projection::rebuild`]）
//!
//! # 消费循环约定（跨通道顺序 §32.2）
//!
//! ```text
//! for ev in low_rx {
//!     while !bus.low_ready(&ev) { /* 等待 Medium ack 追赶 */ }
//!     projection.apply(&ev.event).await?;
//!     projection.set_watermark(ev.seq).await?;
//! }
//! ```
//!
//! 崩溃后：投影水位线持久化于各投影自身存储；启动时
//! `catch_up` 从 `event_queue` 重放 `seq > watermark` 的事件。

use async_trait::async_trait;

use crate::event_bus::layered::{AppEvent, LayeredEventBus, QueuedEvent};

/// 投影健康状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionHealth {
    /// 校验通过，可继续增量追赶。
    Ok,
    /// 校验失败（索引与源数据不一致），需全量重建。
    Corrupted,
}

/// 投影抽象 — 可从事件重放重建的读侧结构。
///
/// 实现方：SearchEngine（FTS 索引）、BidiLinkEngine（双链图）、
/// TaskProjection（GTD/TodayView）、GraphProjection（知识图谱）。
#[async_trait]
pub trait Projection: Send + Sync {
    /// 投影名（日志与度量用）。
    fn name(&self) -> &'static str;

    /// 已消费的最大事件 seq（水位线）。
    async fn watermark(&self) -> Result<u64, crate::Error>;

    /// 幂等应用一条事件。
    async fn apply(&self, event: &crate::event_bus::layered::AppEvent) -> Result<(), crate::Error>;

    /// 健康校验（如索引条数与源比对、抽样一致性检查）。
    ///
    /// 默认 `Ok`（轻量投影可跳过校验，仅靠 rebuild 兜底）。
    async fn verify(&self) -> Result<ProjectionHealth, crate::Error> {
        Ok(ProjectionHealth::Ok)
    }

    /// 更新水位线（apply 成功后由消费循环调用；须持久化）。
    async fn set_watermark(&self, seq: u64) -> Result<(), crate::Error>;

    /// 下行转换支持（具体类型查询; 覆写返回 Some(self)）。
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        None
    }

    /// 批量应用（DK-22：writer 批量 commit 优化 — 开工令数据决策 commit 占比
    /// 98.2%，逐事件 commit 是增量路径的主要成本）。
    ///
    /// 默认实现 = 逐事件循环 `apply`（行为与旧 catch_up 完全一致，其他投影
    /// 零改动向后兼容）；攒批型投影（SearchIndexProjection）覆写本方法，
    /// 将连续创建事件攒批为单次 commit。
    ///
    /// **可见性约束**：批中途不 commit——读端可见延迟 = 批间隔。明确排除
    /// 后台线程自动 flush（复杂度不成比例，开工令裁决）。批内任一事件失败
    /// 即整批返回 Err，水位线不推进，下次 catch_up 整批重放（投影幂等保证
    /// 重放安全）。
    async fn apply_batch(&self, events: &[AppEvent]) -> Result<(), crate::Error> {
        for event in events {
            self.apply(event).await?;
        }
        Ok(())
    }

    /// 全量重建（从空状态追赶至最新）。
    async fn rebuild(&self) -> Result<(), crate::Error>;
}

impl LayeredEventBus {
    /// 投影增量追赶 — V20 §4.5 `catch_up`。
    ///
    /// 从投影水位线之后重放持久化事件；`verify` 失败则触发全量重建。
    /// 返回本次追赶应用的事件数。
    pub async fn catch_up(&self, projection: &dyn Projection) -> Result<usize, crate::Error> {
        use crate::event_bus::layered::AppEvent;
        let name = projection.name();
        let from = projection.watermark().await?;
        let records = self.events_after(from).await?;

        // DK-22：解码整批后走 apply_batch（默认逐事件循环 = 旧行为；
        // 攒批型投影覆写后单 commit 摊薄 98.2% 的 commit 成本）。
        // 水位线整批推进到末位 seq：批内失败 → 不推进 → 整批重放（投影幂等）。
        let mut batch: Vec<AppEvent> = Vec::with_capacity(records.len());
        let mut last_seq = from;
        let mut applied = 0usize;
        for rec in &records {
            // 反序列化 payload；损坏记录跳过（投影幂等可由后续全量校验补偿）
            let event: AppEvent = match serde_json::from_str(&rec.payload) {
                Ok(ev) => ev,
                Err(e) => {
                    tracing::warn!(seq = rec.seq, error = %e, "catch_up skip undecodable");
                    continue;
                }
            };
            batch.push(event);
            last_seq = rec.seq;
            applied += 1;
        }
        if !batch.is_empty() {
            projection.apply_batch(&batch).await?;
            projection.set_watermark(last_seq).await?;
        }
        if applied > 0 {
            tracing::debug!(projection = name, applied, "catch_up done");
        }

        // 健康校验失败 → 全量重建（自愈）
        if projection.verify().await? == ProjectionHealth::Corrupted {
            tracing::warn!(projection = name, "verify failed; rebuilding");
            projection.rebuild().await?;
        }
        Ok(applied)
    }

    /// 读取 `seq > from` 的全部持久化事件（含已消费，投影重放用）。
    async fn events_after(&self, from: u64) -> Result<Vec<QueuedEvent>, crate::Error> {
        let Some(store) = &self.store else {
            return Ok(Vec::new());
        };
        store.events_after(from)
    }
}

// ===== DK-19：每日笔记投影（DailyNoteOpened 消费——daily 索引自愈）=====

/// 每日笔记投影：消费 DailyNoteOpened 事件，校验 `daily:{date}` 索引指向
/// 与事件 note_id 一致；不一致（索引损坏/丢键）时以事件为准自愈。
/// 幂等：set 为覆盖写；水位线照常推进。
pub struct DailyNoteProjection {
    core: std::sync::Arc<crate::app_core::AppCore>,
}

impl DailyNoteProjection {
    pub fn new(core: std::sync::Arc<crate::app_core::AppCore>) -> Self {
        Self { core }
    }
}

const DAILY_PROJECTION_WM: &str = "projection_watermark:daily_note";

#[async_trait::async_trait]
impl Projection for DailyNoteProjection {
    fn name(&self) -> &'static str {
        "daily_note_projection"
    }

    async fn watermark(&self) -> Result<u64, crate::Error> {
        Ok(self
            .core
            .kv_store
            .get(DAILY_PROJECTION_WM)
            .await?
            .and_then(|b| String::from_utf8_lossy(&b).parse().ok())
            .unwrap_or(0))
    }

    async fn apply(&self, event: &crate::event_bus::layered::AppEvent) -> Result<(), crate::Error> {
        if let crate::event_bus::layered::AppEvent::DailyNoteOpened { date, note_id } = event {
            let key = format!("daily:{date}");
            let current = self.core.kv_store.get(&key).await?;
            if current.as_deref() != Some(note_id.as_bytes()) {
                self.core.kv_store.set(&key, note_id.as_bytes()).await?;
                tracing::info!(date, note_id, "daily index healed by projection (DK-19)");
            }
        }
        Ok(())
    }

    async fn set_watermark(&self, seq: u64) -> Result<(), crate::Error> {
        self.core
            .kv_store
            .set(DAILY_PROJECTION_WM, seq.to_string().as_bytes())
            .await
    }

    async fn rebuild(&self) -> Result<(), crate::Error> {
        // 全量重建：扫描全部 DailyNote（daily_date 非空）重灌 daily:{date} 索引
        for (k, bytes) in self.core.kv_store.scan_prefix("note:").await? {
            let _ = k;
            if let Some(rec) = crate::write_path::decode_record(&bytes, None) {
                if rec.kind == crate::write_path::NoteKind::DailyNote {
                    if let Some(date) = &rec.daily_date {
                        self.core
                            .kv_store
                            .set(&format!("daily:{date}"), rec.id.as_bytes())
                            .await?;
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::layered::{
        AppEvent, EventChannel, EventQueueStore, InMemoryEventQueue, QueuedEvent,
    };
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    /// 测试投影：记录 watermark，前 2 条后报告损坏（触发 rebuild）。
    struct TestProjection {
        watermark: AtomicU64,
        applied: AtomicU64,
        rebuilds: AtomicU64,
        fail_verify_until: AtomicU64,
    }

    impl TestProjection {
        fn new() -> Self {
            Self {
                watermark: AtomicU64::new(0),
                applied: AtomicU64::new(0),
                rebuilds: AtomicU64::new(0),
                fail_verify_until: AtomicU64::new(2),
            }
        }
    }

    #[async_trait]
    impl Projection for TestProjection {
        async fn set_watermark(&self, seq: u64) -> Result<(), crate::Error> {
            self.watermark.store(seq, Ordering::SeqCst);
            Ok(())
        }

        fn name(&self) -> &'static str {
            "test-projection"
        }
        async fn watermark(&self) -> Result<u64, crate::Error> {
            Ok(self.watermark.load(Ordering::SeqCst))
        }
        async fn apply(&self, event: &AppEvent) -> Result<(), crate::Error> {
            let _ = event;
            self.applied.fetch_add(1, Ordering::SeqCst);
            self.watermark.fetch_max(seq_of(event), Ordering::SeqCst);
            Ok(())
        }
        async fn verify(&self) -> Result<ProjectionHealth, crate::Error> {
            if self.applied.load(Ordering::SeqCst) < self.fail_verify_until.load(Ordering::SeqCst) {
                Ok(ProjectionHealth::Corrupted)
            } else {
                Ok(ProjectionHealth::Ok)
            }
        }
        async fn rebuild(&self) -> Result<(), crate::Error> {
            self.rebuilds.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn seq_of(_e: &AppEvent) -> u64 {
        0 // 测试投影不依赖真实 seq（watermark 由 catch_up 数据驱动）
    }

    fn queued(seq: u64) -> QueuedEvent {
        use crate::event_bus::layered::AppEvent;
        let event = AppEvent::NoteCreated {
            note_id: format!("note-{seq}"),
            title: "t".into(),
            content: String::new(),
        };
        QueuedEvent {
            event_id: None,
            seq,
            channel: EventChannel::Low,
            event_type: "NoteCreated".into(),
            payload: serde_json::to_string(&event).unwrap(),
        }
    }

    #[tokio::test]
    async fn catch_up_healthy_then_incremental() {
        let store = InMemoryEventQueue::new();
        store.enqueue(&queued(1)).unwrap();
        store.enqueue(&queued(2)).unwrap();
        store.enqueue(&queued(3)).unwrap();
        let bus = LayeredEventBus::new(Some(Arc::new(store.clone())));

        let p = TestProjection::new();
        // 阶段1: watermark=0 → 全量追赶 3 条，verify Ok（applied=3 ≥ 2）
        let n = bus.catch_up(&p).await.unwrap();
        assert_eq!(n, 3);
        assert_eq!(p.watermark.load(Ordering::SeqCst), 3);
        assert_eq!(p.rebuilds.load(Ordering::SeqCst), 0);

        // 阶段2（增量）: 补 seq=4 → 仅应用 1 条
        store.enqueue(&queued(4)).unwrap();
        let n2 = bus.catch_up(&p).await.unwrap();
        assert_eq!(n2, 1);
        assert_eq!(p.watermark.load(Ordering::SeqCst), 4);
        assert_eq!(p.rebuilds.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn catch_up_verify_failure_triggers_rebuild() {
        let store = InMemoryEventQueue::new();
        store.enqueue(&queued(1)).unwrap();
        store.enqueue(&queued(2)).unwrap();
        let bus = LayeredEventBus::new(Some(Arc::new(store)));

        let p = TestProjection::new();
        // 人为置为"永久损坏"（fail_verify_until 巨大 → verify 恒 Corrupted）
        p.fail_verify_until.store(u64::MAX, Ordering::SeqCst);

        let n = bus.catch_up(&p).await.unwrap();
        // 事件正常应用 + verify 失败 → rebuild 恰好一次（自愈路径）
        assert_eq!(n, 2);
        assert_eq!(p.rebuilds.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn catch_up_without_store_applies_nothing() {
        // 无持久化 store 的总线（纯内存）→ catch_up 空转不报错
        let bus = LayeredEventBus::new(None);
        let p = TestProjection::new();
        let n = bus.catch_up(&p).await.unwrap();
        assert_eq!(n, 0);
    }
}
