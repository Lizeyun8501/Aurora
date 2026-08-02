//! 七大 Trait 抽象 (Seven Core Traits)
//!
//! Trait 是 Aurora Note 架构的核心抽象机制，定义了模块间交互的标准接口。
//! 所有 L3 领域服务均通过 Trait 与 L2 引擎通信，实现模块解耦与可测试性。

pub mod crdt_engine;
pub mod sync_target;
pub mod vector_store;
pub mod ai_provider;
pub mod storage;
pub mod plugin_runtime;
pub mod agent_protocol;
pub mod ocr_provider;
