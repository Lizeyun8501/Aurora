//! SQLite 存储
//!
//! 提供关系型数据持久化能力，作为本地数据存储的基石。
//! 底层使用 [rusqlite](https://docs.rs/rusqlite) (bundled SQLite) 实现，
//! 并通过 `sqlite-vec` 可加载扩展支持向量检索。

/// SQLite 数据库连接占位类型。
///
/// 实际实现将在后续任务中封装 rusqlite 的 `Connection` 与迁移管理能力。
pub struct StorageConnection;
