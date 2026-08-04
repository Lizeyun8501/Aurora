//! Trait 抽象层 (Core Trait Abstractions)
//!
//! Trait 是 Aurora Note 架构的核心抽象机制，定义了模块间交互的标准接口。
//! 所有 L3 领域服务均通过 Trait 与 L2 引擎通信，实现模块解耦与可测试性。
//!
//! # 与 V19 架构报告（§28 七大 Trait）的对齐关系
//!
//! | V19 §28 Trait   | 本模块对应 | 说明 |
//! |-----------------|-----------|------|
//! | `SyncTarget`    | [`sync_target`] | 直接对应 |
//! | `OCREngine`     | [`ocr_provider`] | 命名差异，语义一致 |
//! | `AIProvider`    | [`ai_provider`] | 直接对应 |
//! | `KVStore`       | [`kv_store`] | V19 对齐新增 |
//! | `PluginRuntime` | [`plugin_runtime`] | 直接对应 |
//! | `CryptoProvider`| [`crypto_provider`] | V19 对齐新增（跨层服务入口） |
//! | `SearchBackend` | [`search_backend`] | V19 对齐新增 |
//!
//! 以下为 V16 阶段引入、V19 未单列但仍保留的扩展抽象：
//! [`crdt_engine`]（CRDT 引擎可替换性）、[`vector_store`]（向量检索）、
//! [`storage`]（关系型/KV 混合存储）、[`agent_protocol`]（MCP Agent 协议）。

pub mod agent_protocol;
pub mod ai_provider;
pub mod crdt_engine;
pub mod crypto_provider;
pub mod kv_store;
pub mod ocr_provider;
pub mod plugin_runtime;
pub mod search_backend;
pub mod storage;
pub mod sync_target;
pub mod vector_store;
