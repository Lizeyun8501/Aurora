//! 分层事件总线 (Layered Event Bus) — V19 DEF-002 / ARCH-003 / §32
//!
//! 将单一 EventBus 拆分为三条优先级通道，解决背压与容错隔离问题：
//!
//! | 通道 | 队列上限 | 超限策略 | 持久化 | 典型消费者 |
//! |------|---------|---------|--------|-----------|
//! | High（高频实时） | 100 | 丢弃最旧事件 | 否（允许丢失，下次刷新自愈） | TodayView 聚合刷新 |
//! | Medium（中频异步） | 无界（有序） | 积压>50 时 Low 通道延迟 5s | **SQLite `event_queue` 表**（ARCH-003） | BidiLinkEngine 双链更新 |
//! | Low（低频后台） | 1000 | 切换批量处理模式 | 幂等处理 + 定期全量补偿 | SearchEngine / VersionControl |
//!
//! # 顺序性约束（§32.2）
//!
//! `BidiLinkChanged`（Medium）必须先于 `NoteCreated`（Low）的索引更新完成，
//! 否则搜索可能返回失效链接。实现：每条事件携带全局递增序列号 `seq`，
//! Medium 通道处理完成后再放行 Low 通道中 `seq` 更小的事件；
//! Medium 积压超过 [`MEDIUM_BACKLOG_THRESHOLD`] 时，Low 通道自动延迟
//! [`LOW_CHANNEL_DELAY`] 再批量处理。
//!
//! # 崩溃恢复（ARCH-003）
//!
//! Medium 通道事件在发布时先写入持久化存储（[`EventQueueStore`]，生产实现
//! 落 SQLite `event_queue` 表），消费成功后标记 `consumed`；启动时调用
//! [`LayeredEventBus::replay_unconsumed`] 重放未消费事件。
//! Low 通道消费者须实现幂等处理，崩溃后通过每日全量索引校验补偿。

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, warn};

/// High 通道容量（超限丢弃最旧事件）。
pub const HIGH_CHANNEL_CAPACITY: usize = 100;
/// Low 通道容量（超限切换批量模式）。
pub const LOW_CHANNEL_CAPACITY: usize = 1000;
/// Medium 积压阈值：超过后 Low 通道延迟处理。
pub const MEDIUM_BACKLOG_THRESHOLD: usize = 50;
/// Medium 积压时 Low 通道的延迟时长。
pub const LOW_CHANNEL_DELAY: Duration = Duration::from_secs(5);

/// 事件通道标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventChannel {
    /// 高频实时：队列上限 100，超限丢弃旧事件，无持久化。
    High,
    /// 中频异步：独立通道，保证顺序，SQLite 持久化（ARCH-003）。
    Medium,
    /// 低频后台：队列上限 1000，超限批量模式，幂等处理。
    Low,
}

impl EventChannel {
    /// 通道名（用于 `event_queue.channel` 列与日志）。
    pub fn as_str(&self) -> &'static str {
        match self {
            EventChannel::High => "high",
            EventChannel::Medium => "medium",
            EventChannel::Low => "low",
        }
    }
}

/// 链接变更动作。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LinkAction {
    /// 创建链接。
    Created,
    /// 删除链接。
    Deleted,
}

/// 笔记元数据变更集。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NoteChanges {
    /// 新标题（未变更为 `None`）。
    pub title: Option<String>,
    /// 新标签集（未变更为 `None`）。
    pub tags: Option<Vec<String>>,
}

/// 应用事件枚举（V19 §32.1）。
///
/// 每个变体通过 [`AppEvent::channel`] 归属固定通道，发布方无需关心路由。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AppEvent {
    // ===== 高频实时通道 (High) =====
    /// 笔记内容变更（每次按键 debounce 后触发）。
    NoteContentChanged {
        /// 笔记 ID。
        note_id: String,
        /// 变更块 ID（整篇变更为 `None`）。
        block_id: Option<String>,
    },
    /// 光标位置变更（协同编辑 Awareness）。
    CursorMoved {
        /// 笔记 ID。
        note_id: String,
        /// 用户 ID。
        user_id: String,
        /// 光标位置。
        pos: usize,
    },
    /// TodayView 数据更新。
    TodayViewRefresh {
        /// 日期（`YYYY-MM-DD`）。
        date: String,
    },

    // ===== 中频异步通道 (Medium) =====
    /// 双链创建/删除。
    BidiLinkChanged {
        /// 源笔记 ID。
        source_note_id: String,
        /// 目标笔记 ID。
        target_note_id: String,
        /// 变更动作。
        action: LinkAction,
    },
    /// 笔记元数据变更（标题/标签）。
    NoteMetadataChanged {
        /// 笔记 ID。
        note_id: String,
        /// 变更内容。
        changes: NoteChanges,
    },
    /// 任务状态变更。
    TaskStatusChanged {
        /// 任务 ID。
        task_id: String,
        /// 原状态。
        old_status: String,
        /// 新状态。
        new_status: String,
    },

    // ===== 低频后台通道 (Low) =====
    /// 笔记创建（需索引）。
    NoteCreated {
        /// 笔记 ID。
        note_id: String,
        /// 标题。
        title: String,
        /// 正文内容。
        content: String,
    },
    /// 笔记删除（需清理索引）。
    NoteDeleted {
        /// 笔记 ID。
        note_id: String,
    },
    /// 请求创建版本快照。
    SnapshotRequested {
        /// 笔记 ID。
        note_id: String,
        /// 用户标注。
        label: Option<String>,
    },
    /// CRDT 同步完成。
    SyncCompleted {
        /// 对端节点 ID。
        peer_id: String,
        /// 同步笔记数。
        note_count: usize,
    },
}

impl AppEvent {
    /// 事件归属的通道（V19 §32.1 路由表）。
    pub fn channel(&self) -> EventChannel {
        match self {
            AppEvent::NoteContentChanged { .. }
            | AppEvent::CursorMoved { .. }
            | AppEvent::TodayViewRefresh { .. } => EventChannel::High,
            AppEvent::BidiLinkChanged { .. }
            | AppEvent::NoteMetadataChanged { .. }
            | AppEvent::TaskStatusChanged { .. } => EventChannel::Medium,
            AppEvent::NoteCreated { .. }
            | AppEvent::NoteDeleted { .. }
            | AppEvent::SnapshotRequested { .. }
            | AppEvent::SyncCompleted { .. } => EventChannel::Low,
        }
    }

    /// 事件类型名（用于持久化与日志）。
    pub fn event_type(&self) -> &'static str {
        match self {
            AppEvent::NoteContentChanged { .. } => "NoteContentChanged",
            AppEvent::CursorMoved { .. } => "CursorMoved",
            AppEvent::TodayViewRefresh { .. } => "TodayViewRefresh",
            AppEvent::BidiLinkChanged { .. } => "BidiLinkChanged",
            AppEvent::NoteMetadataChanged { .. } => "NoteMetadataChanged",
            AppEvent::TaskStatusChanged { .. } => "TaskStatusChanged",
            AppEvent::NoteCreated { .. } => "NoteCreated",
            AppEvent::NoteDeleted { .. } => "NoteDeleted",
            AppEvent::SnapshotRequested { .. } => "SnapshotRequested",
            AppEvent::SyncCompleted { .. } => "SyncCompleted",
        }
    }
}

/// 携带全局序列号的信封事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequencedEvent {
    /// 全局递增序列号（跨通道顺序约束依据）。
    pub seq: u64,
    /// 事件本体。
    pub event: AppEvent,
}

/// 持久化队列中的记录（对应 SQLite `event_queue` 表）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedEvent {
    /// 全局序列号。
    pub seq: u64,
    /// 通道。
    pub channel: EventChannel,
    /// 事件类型名。
    pub event_type: String,
    /// JSON 序列化的事件本体。
    pub payload: String,
}

/// Medium 通道持久化存储抽象（ARCH-003）。
///
/// 生产实现落 SQLite `event_queue` 表（V19 §29）；
/// 测试可用内存实现。
pub trait EventQueueStore: Send + Sync {
    /// 追加待消费事件。
    fn enqueue(&self, record: &QueuedEvent) -> Result<(), crate::Error>;
    /// 标记事件已消费。
    fn mark_consumed(&self, seq: u64) -> Result<(), crate::Error>;
    /// 读取全部未消费事件（启动重放用，按 `seq` 升序）。
    fn pending(&self) -> Result<Vec<QueuedEvent>, crate::Error>;
}

/// 分层事件总线。
///
/// 与单通道 [`super::EventBus`] 并存：旧代码继续走 `EventBus`，
/// 新代码按 V19 语义走 `LayeredEventBus`。后续版本将统一迁移。
pub struct LayeredEventBus {
    /// High 通道（broadcast，lagged 即丢弃旧事件）。
    high_tx: broadcast::Sender<SequencedEvent>,
    /// Medium 通道（mpsc，保证顺序）。
    medium_tx: mpsc::UnboundedSender<SequencedEvent>,
    /// Medium 接收端（仅可在启动期取出一次，交给 BidiLink 等消费者）。
    medium_rx: RwLock<Option<mpsc::UnboundedReceiver<SequencedEvent>>>,
    /// Low 通道（mpsc，有界，满则批量模式）。
    low_tx: mpsc::Sender<SequencedEvent>,
    /// Low 接收端（仅可在启动期取出一次）。
    low_rx: RwLock<Option<mpsc::Receiver<SequencedEvent>>>,
    /// 全局序列号。
    seq: Arc<AtomicU64>,
    /// Medium 通道积压计数（背压信号）。
    medium_backlog: Arc<AtomicU64>,
    /// Medium 已处理完成的最大序列号（Low 通道放行依据）。
    medium_watermark: Arc<AtomicU64>,
    /// 持久化存储（可选；缺省时 Medium 事件崩溃后丢失并告警）。
    store: Option<Arc<dyn EventQueueStore>>,
}

impl LayeredEventBus {
    /// 创建分层事件总线。
    pub fn new(store: Option<Arc<dyn EventQueueStore>>) -> Self {
        let (high_tx, _) = broadcast::channel(HIGH_CHANNEL_CAPACITY);
        let (medium_tx, medium_rx) = mpsc::unbounded_channel();
        let (low_tx, low_rx) = mpsc::channel(LOW_CHANNEL_CAPACITY);
        Self {
            high_tx,
            medium_tx,
            medium_rx: RwLock::new(Some(medium_rx)),
            low_tx,
            low_rx: RwLock::new(Some(low_rx)),
            seq: Arc::new(AtomicU64::new(1)),
            medium_backlog: Arc::new(AtomicU64::new(0)),
            medium_watermark: Arc::new(AtomicU64::new(0)),
            store,
        }
    }

    /// 发布事件：按 [`AppEvent::channel`] 自动路由到对应通道。
    pub fn publish(&self, event: AppEvent) {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let envelope = SequencedEvent {
            seq,
            event: event.clone(),
        };
        match event.channel() {
            EventChannel::High => {
                // broadcast lagged 时最旧事件自动丢弃（背压策略）。
                let _ = self.high_tx.send(envelope);
            }
            EventChannel::Medium => {
                // ARCH-003：先持久化再入队。
                if let Some(store) = &self.store {
                    let record = QueuedEvent {
                        seq,
                        channel: EventChannel::Medium,
                        event_type: event.event_type().to_string(),
                        payload: serde_json::to_string(&event).unwrap_or_default(),
                    };
                    if let Err(e) = store.enqueue(&record) {
                        warn!(seq, error = %e, "event_queue persist failed");
                    }
                } else {
                    debug!(
                        seq,
                        "no EventQueueStore configured; medium event not persisted"
                    );
                }
                self.medium_backlog.fetch_add(1, Ordering::SeqCst);
                if self.medium_tx.send(envelope).is_err() {
                    warn!(seq, "medium channel closed; event dropped");
                }
            }
            EventChannel::Low => {
                // 有界队列满 → 批量模式：尝试非阻塞发送，失败则告警丢弃
                // （低频事件幂等，可由每日全量补偿恢复）。
                if let Err(e) = self.low_tx.try_send(envelope) {
                    warn!(seq, error = %e, "low channel full; switching to batch compensation");
                }
            }
        }
    }

    /// 订阅 High 通道（TodayView 等实时消费者）。
    pub fn subscribe_high(&self) -> broadcast::Receiver<SequencedEvent> {
        self.high_tx.subscribe()
    }

    /// 接管 Medium 通道接收端（唯一消费者，保证顺序）。
    ///
    /// 只能在启动期调用一次，第二次调用返回 `None`。
    /// 消费者处理完每条事件后必须调用 [`Self::ack_medium`]。
    pub fn take_medium_receiver(&self) -> Option<mpsc::UnboundedReceiver<SequencedEvent>> {
        self.medium_rx.write().take()
    }

    /// 接管 Low 通道接收端（SearchEngine / VersionControl 等后台消费者）。
    ///
    /// 只能在启动期调用一次，第二次调用返回 `None`。
    /// 消费前应查询 [`Self::low_channel_backpressure`] 做背压延迟。
    pub fn take_low_receiver(&self) -> Option<mpsc::Receiver<SequencedEvent>> {
        self.low_rx.write().take()
    }

    /// Medium 事件处理完成确认：更新水位线并标记持久化记录已消费。
    pub fn ack_medium(&self, seq: u64) {
        self.medium_backlog.fetch_sub(1, Ordering::SeqCst);
        self.medium_watermark.fetch_max(seq, Ordering::SeqCst);
        if let Some(store) = &self.store {
            if let Err(e) = store.mark_consumed(seq) {
                warn!(seq, error = %e, "event_queue mark_consumed failed");
            }
        }
    }

    /// Low 通道在消费前调用：若 Medium 积压超阈值，返回建议延迟。
    ///
    /// 返回 `Some(duration)` 表示应延迟后再批量处理。
    pub fn low_channel_backpressure(&self) -> Option<Duration> {
        if self.medium_backlog.load(Ordering::SeqCst) > MEDIUM_BACKLOG_THRESHOLD as u64 {
            Some(LOW_CHANNEL_DELAY)
        } else {
            None
        }
    }

    /// 当前 Medium 水位线（已完成处理的最大序列号）。
    pub fn medium_watermark(&self) -> u64 {
        self.medium_watermark.load(Ordering::SeqCst)
    }

    /// 启动时重放未消费的 Medium 事件（ARCH-003 崩溃恢复）。
    ///
    /// 返回按 `seq` 升序排列的待重放事件；调用方负责重新走消费流程。
    pub fn replay_unconsumed(&self) -> Result<Vec<SequencedEvent>, crate::Error> {
        let store = match &self.store {
            Some(s) => s,
            None => return Ok(Vec::new()),
        };
        let pending = store.pending()?;
        let mut events = Vec::with_capacity(pending.len());
        for record in pending {
            match serde_json::from_str::<AppEvent>(&record.payload) {
                Ok(event) => events.push(SequencedEvent {
                    seq: record.seq,
                    event,
                }),
                Err(e) => warn!(seq = record.seq, error = %e, "skip undecodable queued event"),
            }
        }
        // 恢复序列号生成器，避免与重放事件冲突。
        if let Some(max_seq) = events.iter().map(|e| e.seq).max() {
            self.seq.fetch_max(max_seq + 1, Ordering::SeqCst);
        }
        Ok(events)
    }
}

/// 内存版 [`EventQueueStore`]（测试 / 开发用）。
#[derive(Default)]
pub struct InMemoryEventQueue {
    inner: RwLock<VecDeque<(QueuedEvent, bool)>>,
}

impl InMemoryEventQueue {
    /// 创建空的内存队列。
    pub fn new() -> Self {
        Self::default()
    }
}

impl EventQueueStore for InMemoryEventQueue {
    fn enqueue(&self, record: &QueuedEvent) -> Result<(), crate::Error> {
        self.inner.write().push_back((record.clone(), false));
        Ok(())
    }

    fn mark_consumed(&self, seq: u64) -> Result<(), crate::Error> {
        let mut guard = self.inner.write();
        if let Some(entry) = guard.iter_mut().find(|(r, _)| r.seq == seq) {
            entry.1 = true;
        }
        Ok(())
    }

    fn pending(&self) -> Result<Vec<QueuedEvent>, crate::Error> {
        let guard = self.inner.read();
        Ok(guard
            .iter()
            .filter(|(_, consumed)| !consumed)
            .map(|(r, _)| r.clone())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_routing() {
        assert_eq!(
            AppEvent::NoteContentChanged {
                note_id: "n1".into(),
                block_id: None
            }
            .channel(),
            EventChannel::High
        );
        assert_eq!(
            AppEvent::BidiLinkChanged {
                source_note_id: "a".into(),
                target_note_id: "b".into(),
                action: LinkAction::Created
            }
            .channel(),
            EventChannel::Medium
        );
        assert_eq!(
            AppEvent::NoteCreated {
                note_id: "n1".into(),
                title: "t".into(),
                content: "c".into()
            }
            .channel(),
            EventChannel::Low
        );
    }

    #[tokio::test]
    async fn high_channel_delivers() {
        let bus = LayeredEventBus::new(None);
        let mut rx = bus.subscribe_high();
        bus.publish(AppEvent::TodayViewRefresh {
            date: "2026-08-04".into(),
        });
        let env = rx.recv().await.unwrap();
        assert!(matches!(env.event, AppEvent::TodayViewRefresh { .. }));
        assert_eq!(env.seq, 1);
    }

    #[tokio::test]
    async fn medium_events_persisted() {
        let store = Arc::new(InMemoryEventQueue::new());
        let bus = LayeredEventBus::new(Some(store.clone()));
        bus.publish(AppEvent::TaskStatusChanged {
            task_id: "t1".into(),
            old_status: "inbox".into(),
            new_status: "next".into(),
        });
        let pending = store.pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].event_type, "TaskStatusChanged");

        // ack 后不再出现在 pending 中
        bus.ack_medium(pending[0].seq);
        assert!(store.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn replay_restores_unconsumed() {
        let store = Arc::new(InMemoryEventQueue::new());
        let bus = LayeredEventBus::new(Some(store));
        bus.publish(AppEvent::NoteMetadataChanged {
            note_id: "n1".into(),
            changes: NoteChanges {
                title: Some("new".into()),
                tags: None,
            },
        });
        let replayed = bus.replay_unconsumed().unwrap();
        assert_eq!(replayed.len(), 1);
        assert!(matches!(
            replayed[0].event,
            AppEvent::NoteMetadataChanged { .. }
        ));
    }
}
