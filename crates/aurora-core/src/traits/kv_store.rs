//! KVStore Trait — 键值存储抽象（V19 §28.4）
//!
//! 对应架构设计报告 V19 七大 Trait 之一。V19 原始定义为 `async_trait`，
//! 本实现与现有 Trait 层保持同步签名风格，方法集与 V19 完全对齐。
//!
//! 用途：配置、缓存、索引元数据等简单键值场景；复杂关系查询走
//! [`crate::traits::storage::Storage`]，全文检索走
//! [`crate::traits::search_backend::SearchBackend`]。

/// 键值存储抽象接口。
pub trait KVStore: Send + Sync {
    /// 读取键对应的值；键不存在返回 `Ok(None)`。
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, crate::Error>;

    /// 写入键值对（覆盖写）。
    fn set(&self, key: &str, value: &[u8]) -> Result<(), crate::Error>;

    /// 删除键；键不存在视为成功（幂等）。
    fn delete(&self, key: &str) -> Result<(), crate::Error>;

    /// 判断键是否存在。
    fn exists(&self, key: &str) -> Result<bool, crate::Error>;

    /// 批量读取，返回与 `keys` 等长的 `Option` 序列。
    fn batch_get(&self, keys: &[&str]) -> Result<Vec<Option<Vec<u8>>>, crate::Error>;

    /// 批量写入（同一原子批次）。
    fn batch_set(&self, items: &[(&str, &[u8])]) -> Result<(), crate::Error>;

    /// 前缀范围扫描（用于索引重建、命名空间枚举）。
    ///
    /// 返回按字典序排列的 `(key, value)` 列表。
    fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>)>, crate::Error>;
}
