//! 事件总线与层间通信 (Event Bus & Inter-Layer Communication)
//!
//! 基于 tokio::sync::broadcast 实现异步事件总线，支持多消费者。

pub mod event;
pub mod event_bus;
pub mod serialization;

pub use event::CoreEvent;
pub use event_bus::EventBus;
