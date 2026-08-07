//! 事件总线与层间通信 (Event Bus & Inter-Layer Communication)
//!
//! 两代实现并存：
//! - [`event_bus`]：V16 单通道广播总线（兼容保留）。
//! - [`layered`]：V19 DEF-002 分层事件总线（High/Medium/Low 三通道 +
//!   背压策略 + Medium 通道持久化，对应 §32 事件定义与 ARCH-003 崩溃恢复）。

pub mod event;
#[allow(clippy::module_inception)]
pub mod event_bus;
pub mod layered;
pub mod serialization;
pub mod sqlite_queue;

pub use event::CoreEvent;
pub use event_bus::EventBus;
pub use layered::{
    AppEvent, EventChannel, EventQueueStore, InMemoryEventQueue, LayeredEventBus, SequencedEvent,
};
pub use sqlite_queue::SqliteEventQueue;
