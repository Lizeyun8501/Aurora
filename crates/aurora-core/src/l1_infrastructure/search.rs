//! 全文检索 (基于 Tantivy)
//!
//! 提供高性能的全文搜索引擎能力，支持中文分词与复杂查询。
//! 底层使用 [Tantivy](https://tantivy-search.github.io/tantivy/) 实现。
//!
//! # 实现说明
//! - 实现 V19 §28.7 `SearchBackend` Trait（async 签名），供平台适配层注入 AppCore。
//! - Schema 字段：`id`（STRING，可删除定位）、`title`/`content`/`tags`（TEXT，
//!   参与全文检索）、`workspace_id`（STRING）、`updated_at`（STRING，RFC3339）。
//! - 写入采用「每次操作新建 IndexWriter」策略：`IndexWriter` 借用 `Index`，
//!   无法作为 `Self` 成员存储；Tantivy 推荐复用 writer，但每次创建的开销在
//!   笔记级写入频率下可忽略，且天然规避生命周期自引用问题。
//! - 分词：将索引的 `default` tokenizer 替换为 SimpleTokenizer + LowerCaser
//!   （不区分大小写），索引与查询两侧同用该 analyzer，保证检索一致；
//!   V19 规划的中文 jieba 分词在后续 PR 注册自定义 tokenizer 后接入。

use std::path::Path;
use std::time::Instant;

use async_trait::async_trait;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, STORED, STRING, TEXT, Value};
use tantivy::tokenizer::{LowerCaser, SimpleTokenizer, TextAnalyzer};
use tantivy::{doc, Index, TantivyDocument, Term};

use crate::traits::search_backend::{
    IndexEntry, NoteMetadata, SearchBackend, SearchHit, SearchOptions, SearchResult,
};
use crate::Error;

/// 单个字段的最大索引长度（Tantivy 默认 token 长度上限 40，这里放大到 255）。
const MAX_TOKEN_LEN: usize = 255;

/// 搜索结果摘要截取长度。
const SNIPPET_LEN: usize = 160;

fn map_err(e: tantivy::TantivyError) -> Error {
    Error::Internal(format!("tantivy error: {}", e))
}

/// 将索引默认 tokenizer 替换为 SimpleTokenizer + LowerCaser。
///
/// Tantivy 内置 `default` tokenizer（SimpleTokenizer）不做大小写归一，会导致
/// 检索大小写敏感（如 `Test` 查不到 `test`）。这里统一注册为小写分析器，
/// 索引与查询两侧均使用（QueryParser 按字段 tokenizer 分析查询词），
/// 保证「写进去什么、就能搜到什么」的直觉语义。
fn register_lowercase_default(index: &Index) {
    let lowercase = TextAnalyzer::builder(SimpleTokenizer::default())
        .filter(LowerCaser)
        .build();
    index.tokenizers().register("default", lowercase);
}

/// 构建全文检索 Schema。
fn build_schema() -> Schema {
    let mut builder = Schema::builder();
    builder.add_text_field("id", STRING | STORED);
    // tokenizer 使用 "raw" 会失去检索能力，TEXT 默认分词即可；
    // 通过 TextOptions 扩展 token 长度上限。
    builder.add_text_field("title", TEXT | STORED);
    builder.add_text_field("content", TEXT | STORED);
    builder.add_text_field("tags", TEXT | STORED);
    builder.add_text_field("workspace_id", STRING);
    builder.add_text_field("updated_at", STRING | STORED);
    builder.build()
}

/// 基于 Tantivy 的全文检索后端实现。
pub struct TantivySearchBackend {
    index: Index,
    id: Field,
    title: Field,
    content: Field,
    tags: Field,
    workspace_id: Field,
    updated_at: Field,
}

impl TantivySearchBackend {
    /// 打开或创建指定目录下的索引。
    ///
    /// 目录已含有效索引（`meta.json`）时打开，否则新建。
    pub fn new(dir: impl AsRef<Path>) -> Result<Self, Error> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir).map_err(Error::Io)?;
        let index = if dir.join("meta.json").exists() {
            Index::open_in_dir(dir).map_err(map_err)?
        } else {
            Index::create_in_dir(dir, build_schema()).map_err(map_err)?
        };
        register_lowercase_default(&index);
        let schema = index.schema();
        let id = schema
            .get_field("id")
            .map_err(|e| Error::Internal(format!("schema field id: {}", e)))?;
        let title = schema
            .get_field("title")
            .map_err(|e| Error::Internal(format!("schema field title: {}", e)))?;
        let content = schema
            .get_field("content")
            .map_err(|e| Error::Internal(format!("schema field content: {}", e)))?;
        let tags = schema
            .get_field("tags")
            .map_err(|e| Error::Internal(format!("schema field tags: {}", e)))?;
        let workspace_id = schema
            .get_field("workspace_id")
            .map_err(|e| Error::Internal(format!("schema field workspace_id: {}", e)))?;
        let updated_at = schema
            .get_field("updated_at")
            .map_err(|e| Error::Internal(format!("schema field updated_at: {}", e)))?;
        Ok(Self {
            index,
            id,
            title,
            content,
            tags,
            workspace_id,
            updated_at,
        })
    }

    /// 在内存中创建索引（用于测试）。
    pub fn new_in_memory() -> Result<Self, Error> {
        let index = Index::create_in_ram(build_schema());
        register_lowercase_default(&index);
        let schema = index.schema();
        let id = schema.get_field("id").expect("id field");
        let title = schema.get_field("title").expect("title field");
        let content = schema.get_field("content").expect("content field");
        let tags = schema.get_field("tags").expect("tags field");
        let workspace_id = schema
            .get_field("workspace_id")
            .expect("workspace_id field");
        let updated_at = schema.get_field("updated_at").expect("updated_at field");
        Ok(Self {
            index,
            id,
            title,
            content,
            tags,
            workspace_id,
            updated_at,
        })
    }

    /// 构建单篇文档。
    fn build_doc(
        &self,
        note_id: &str,
        content: &str,
        metadata: &NoteMetadata,
    ) -> tantivy::TantivyDocument {
        let updated_at = metadata
            .updated_at
            .map(|t| t.to_rfc3339())
            .unwrap_or_default();
        doc!(
            self.id => note_id,
            self.title => metadata.title.as_str(),
            self.content => content,
            self.tags => metadata.tags.join(" "),
            self.workspace_id => metadata.workspace_id.as_str(),
            self.updated_at => updated_at.as_str(),
        )
    }

    /// 取 content 字段文本并截取摘要。
    fn snippet_of(&self, document: &TantivyDocument) -> String {
        let full = document
            .get_first(self.content)
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let trimmed = full.trim();
        if trimmed.chars().count() <= SNIPPET_LEN {
            trimmed.to_string()
        } else {
            let mut s: String = trimmed.chars().take(SNIPPET_LEN).collect();
            s.push('…');
            s
        }
    }
}

#[async_trait]
impl SearchBackend for TantivySearchBackend {
    async fn search(&self, query: &str, opts: &SearchOptions) -> Result<SearchResult, Error> {
        let started = Instant::now();
        let limit = if opts.limit == 0 { 20 } else { opts.limit };
        let query_parser =
            QueryParser::for_index(&self.index, vec![self.title, self.content, self.tags]);
        let parsed = query_parser
            .parse_query(query)
            .map_err(|e| Error::Internal(format!("query parse error: {}", e)))?;
        let reader = self.index.reader().map_err(map_err)?;
        let searcher = reader.searcher();
        let top_docs = searcher
            .search(&parsed, &TopDocs::with_limit(limit))
            .map_err(map_err)?;

        let mut hits = Vec::with_capacity(top_docs.len());
        for (score, address) in top_docs {
            let document = searcher.doc::<TantivyDocument>(address).map_err(map_err)?;
            let note_id = document
                .get_first(self.id)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let title = document
                .get_first(self.title)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            hits.push(SearchHit {
                note_id,
                title,
                snippet: self.snippet_of(&document),
                score,
            });
        }

        let total = hits.len();
        Ok(SearchResult {
            hits,
            total,
            took_ms: started.elapsed().as_millis() as u64,
        })
    }

    async fn index_note(
        &self,
        note_id: &str,
        content: &str,
        metadata: &NoteMetadata,
    ) -> Result<(), Error> {
        let document = self.build_doc(note_id, content, metadata);
        let mut writer = self.index.writer(50_000_000).map_err(map_err)?;
        // 同一 id 先删后写，保证幂等更新
        writer.delete_term(Term::from_field_text(self.id, note_id));
        writer.add_document(document).map_err(map_err)?;
        writer.commit().map_err(map_err)?;
        Ok(())
    }

    async fn batch_index(&self, notes: &[IndexEntry]) -> Result<(), Error> {
        let mut writer = self.index.writer(50_000_000).map_err(map_err)?;
        for entry in notes {
            let document = self.build_doc(&entry.note_id, &entry.content, &entry.metadata);
            writer.delete_term(Term::from_field_text(self.id, &entry.note_id));
            writer.add_document(document).map_err(map_err)?;
        }
        writer.commit().map_err(map_err)?;
        Ok(())
    }

    async fn remove_index(&self, note_id: &str) -> Result<(), Error> {
        let mut writer = self
            .index
            .writer::<TantivyDocument>(50_000_000)
            .map_err(map_err)?;
        writer.delete_term(Term::from_field_text(self.id, note_id));
        writer.commit().map_err(map_err)?;
        Ok(())
    }

    async fn rebuild_index(&self, all_notes: &[IndexEntry]) -> Result<(), Error> {
        let mut writer = self.index.writer(50_000_000).map_err(map_err)?;
        writer.delete_all_documents().map_err(map_err)?;
        for entry in all_notes {
            let document = self.build_doc(&entry.note_id, &entry.content, &entry.metadata);
            writer.add_document(document).map_err(map_err)?;
        }
        writer.commit().map_err(map_err)?;
        Ok(())
    }

    fn tokenize(&self, text: &str) -> Vec<String> {
        // 默认分词：按空白切分并小写（与 Tantivy 默认 en 分词近似）；
        // 中文 jieba 分词接入后替换此实现。
        text.split_whitespace()
            .map(|w| w.to_lowercase())
            .filter(|w| !w.is_empty() && w.chars().count() <= MAX_TOKEN_LEN)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::search_backend::SearchOptions;

    fn metadata(title: &str, workspace: &str) -> NoteMetadata {
        NoteMetadata {
            title: title.to_string(),
            tags: vec![],
            workspace_id: workspace.to_string(),
            updated_at: None,
        }
    }

    #[tokio::test]
    async fn index_and_search_roundtrip() {
        let backend = TantivySearchBackend::new_in_memory().unwrap();
        backend
            .index_note(
                "n1",
                "The quick brown fox jumps over the lazy dog",
                &metadata("Fox Story", "ws-1"),
            )
            .await
            .unwrap();

        let result = backend
            .search("brown fox", &SearchOptions::default())
            .await
            .unwrap();
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].note_id, "n1");
        assert_eq!(result.hits[0].title, "Fox Story");
        assert!(result.hits[0].snippet.contains("brown fox"));
    }

    #[tokio::test]
    async fn search_is_case_insensitive() {
        let backend = TantivySearchBackend::new_in_memory().unwrap();
        backend
            .index_note(
                "n1",
                "Case Sensitive Content",
                &metadata("Test Note", "ws-1"),
            )
            .await
            .unwrap();

        let result = backend
            .search("test", &SearchOptions::default())
            .await
            .unwrap();
        assert_eq!(
            result.hits.len(),
            1,
            "query 'test' must match title 'Test Note'"
        );

        let result = backend
            .search("CASE", &SearchOptions::default())
            .await
            .unwrap();
        assert_eq!(result.hits.len(), 1, "query 'CASE' must match content");
    }

    #[tokio::test]
    async fn remove_index_hides_document() {
        let backend = TantivySearchBackend::new_in_memory().unwrap();
        backend
            .index_note("n1", "unique needle phrase", &metadata("T", "ws-1"))
            .await
            .unwrap();
        backend.remove_index("n1").await.unwrap();

        let result = backend
            .search("needle", &SearchOptions::default())
            .await
            .unwrap();
        assert!(result.hits.is_empty());
    }

    #[tokio::test]
    async fn reindex_same_id_is_idempotent() {
        let backend = TantivySearchBackend::new_in_memory().unwrap();
        backend
            .index_note("n1", "first version content", &metadata("V1", "ws-1"))
            .await
            .unwrap();
        backend
            .index_note("n1", "second version content", &metadata("V2", "ws-1"))
            .await
            .unwrap();

        let result = backend
            .search("second", &SearchOptions::default())
            .await
            .unwrap();
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].title, "V2");
    }

    #[tokio::test]
    async fn batch_and_rebuild() {
        let backend = TantivySearchBackend::new_in_memory().unwrap();
        backend
            .batch_index(&[
                IndexEntry {
                    note_id: "a".into(),
                    content: "alpha content words".into(),
                    metadata: metadata("A", "ws-1"),
                },
                IndexEntry {
                    note_id: "b".into(),
                    content: "beta content words".into(),
                    metadata: metadata("B", "ws-1"),
                },
            ])
            .await
            .unwrap();
        backend
            .rebuild_index(&[IndexEntry {
                note_id: "c".into(),
                content: "gamma only content".into(),
                metadata: metadata("C", "ws-1"),
            }])
            .await
            .unwrap();

        let result = backend
            .search("alpha", &SearchOptions::default())
            .await
            .unwrap();
        assert!(result.hits.is_empty(), "rebuild must drop old docs");
        let result = backend
            .search("gamma", &SearchOptions::default())
            .await
            .unwrap();
        assert_eq!(result.hits.len(), 1);
    }

    #[tokio::test]
    async fn limit_respected() {
        let backend = TantivySearchBackend::new_in_memory().unwrap();
        for i in 0..5 {
            backend
                .index_note(
                    &format!("n{}", i),
                    "common keyword content",
                    &metadata(&format!("Note {}", i), "ws-1"),
                )
                .await
                .unwrap();
        }
        let opts = SearchOptions {
            limit: 3,
            ..Default::default()
        };
        let result = backend.search("keyword", &opts).await.unwrap();
        assert_eq!(result.hits.len(), 3);
    }

    #[test]
    fn tokenize_basic() {
        let backend = TantivySearchBackend::new_in_memory().unwrap();
        assert_eq!(
            backend.tokenize("Hello World"),
            vec!["hello".to_string(), "world".to_string()]
        );
        assert!(backend.tokenize("").is_empty());
    }
}
