//! SearchBackend Trait — 搜索后端抽象（V19 §28.7）
//!
//! 对应架构设计报告 V19 七大 Trait 之一。全文检索后端（Tantivy / SQLite FTS5）
//! 通过本 Trait 与领域服务层解耦，支持中文分词（jieba）与工作区/标签/日期过滤。
//!
//! V19 原始指定 `async_trait`，本批次 PR 推进异步化迁移。
//! 纯计算方法 `tokenize` 保持同步签名。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 搜索选项（分页 + 过滤）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchOptions {
    /// 返回结果上限。
    pub limit: usize,
    /// 结果偏移（分页）。
    pub offset: usize,
    /// 限定 Workspace。
    pub workspace_filter: Option<String>,
    /// 限定标签（任一命中）。
    pub tag_filter: Option<Vec<String>>,
    /// 限定时间范围（闭区间）。
    pub date_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
}

/// 单条搜索命中。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    /// 笔记 ID。
    pub note_id: String,
    /// 标题。
    pub title: String,
    /// 高亮摘要片段。
    pub snippet: String,
    /// 相关性得分。
    pub score: f32,
}

/// 搜索结果集。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// 命中列表（按得分降序）。
    pub hits: Vec<SearchHit>,
    /// 命中总数（不受分页影响）。
    pub total: usize,
    /// 查询耗时（毫秒）。
    pub took_ms: u64,
}

/// 笔记索引元数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NoteMetadata {
    /// 笔记标题。
    pub title: String,
    /// 标签集合。
    pub tags: Vec<String>,
    /// 所属 Workspace。
    pub workspace_id: String,
    /// 最后更新时间。
    pub updated_at: Option<DateTime<Utc>>,
}

/// 批量索引条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    /// 笔记 ID。
    pub note_id: String,
    /// 正文内容。
    pub content: String,
    /// 索引元数据。
    pub metadata: NoteMetadata,
}

/// 搜索后端抽象接口。
#[async_trait]
pub trait SearchBackend: Send + Sync {
    /// 全文搜索。
    async fn search(&self, query: &str, opts: &SearchOptions)
        -> Result<SearchResult, crate::Error>;

    /// 索引单篇笔记（存在则更新）。
    async fn index_note(
        &self,
        note_id: &str,
        content: &str,
        metadata: &NoteMetadata,
    ) -> Result<(), crate::Error>;

    /// 批量索引（同一提交批次，提升吞吐）。
    async fn batch_index(&self, notes: &[IndexEntry]) -> Result<(), crate::Error>;

    /// 删除指定笔记的索引。
    async fn remove_index(&self, note_id: &str) -> Result<(), crate::Error>;

    /// 全量重建索引（崩溃恢复 / 定期校验，对应 V19 ARCH-003 低频通道补偿）。
    async fn rebuild_index(&self, all_notes: &[IndexEntry]) -> Result<(), crate::Error>;

    /// 索引中文档总数（投影 verify 一致性校验用）。
    ///
    /// 返回 `Ok(None)` 表示后端不支持计数（verify 跳过数量对比）。
    async fn doc_count(&self) -> Result<Option<usize>, crate::Error> {
        Ok(None)
    }

    /// 中文分词（jieba），供查询解析与高亮使用。
    /// 纯计算方法，保持同步签名。
    fn tokenize(&self, text: &str) -> Vec<String>;
}
