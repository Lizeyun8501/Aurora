//! 事件溯源引擎 (Event Sourcing Engine)
//!
//! 基于 Loro CRDT 实现，所有用户操作被记录为不可变事件序列。
//! 每 1000 个事件自动生成快照，启动时加载最新快照 + 增量事件。

pub mod aggregate;
pub mod event;
pub mod snapshot;
pub mod store;

pub use aggregate::{BlockAggregate, DocumentAggregate, WorkspaceAggregate};
pub use event::Event;
pub use snapshot::SnapshotManager;
pub use store::EventStore;
