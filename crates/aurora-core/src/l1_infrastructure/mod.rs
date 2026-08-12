//! L1 基础设施层 (Infrastructure Layer)
//!
//! 提供 5-10 年不变的底层基础设施能力，采用「库而非框架」策略。

pub mod atomic_transaction; // V19 ARCH-002 原子事务恢复
#[cfg(feature = "loro-crdt")]
pub mod crdt;               // Loro CRDT 引擎
pub mod p2p;                // iroh P2P 同步
pub mod search;             // Tantivy 全文检索
pub mod vector_db;          // LanceDB 向量数据库
pub mod storage;            // SQLite 存储
pub mod wasm;               // Wasmtime WASM 运行时
pub mod crypto;             // 密码学
pub mod ocr;                // OCR 引擎
