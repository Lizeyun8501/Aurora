//! 全文检索 (基于 Tantivy)
//!
//! 提供高性能的全文搜索引擎能力，支持中文分词与复杂查询。
//! 底层使用 [Tantivy](https://tantivy-search.github.io/tantivy/) 实现。

/// 全文检索索引占位类型。
///
/// 实际实现将在后续任务中封装 Tantivy 的 `Index` 与 `IndexWriter` 等能力。
pub struct SearchIndex;
