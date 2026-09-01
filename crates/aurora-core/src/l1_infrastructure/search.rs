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
use tantivy::schema::{Field, Schema, Value, STORED, STRING, TEXT};
use tantivy::schema::TextOptions;
use tantivy::tokenizer::{LowerCaser, SimpleTokenizer, TextAnalyzer};

use crate::l1_infrastructure::jieba_tokenizer::register_jieba_with;
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
    // 中文检索（V20 jieba）: title/content/tags 显式绑定 "jieba" analyzer；
    // 旧索引（default tokenizer 建立的）打开时仍按其 schema 的 default 走，
    // 全量 rebuild_index 后自然切换。
    let jieba_text = || {
        TextOptions::default()
            .set_indexing_options(
                tantivy::schema::TextFieldIndexing::default()
                    .set_tokenizer("jieba")
                    .set_index_option(
                        tantivy::schema::IndexRecordOption::WithFreqsAndPositions,
                    ),
            )
            .set_stored()
    };
    builder.add_text_field("title", jieba_text());
    builder.add_text_field("content", jieba_text());
    builder.add_text_field("tags", jieba_text());
    builder.add_text_field("workspace_id", STRING);
    builder.add_text_field("updated_at", STRING | STORED);
    builder.build()
}

/// 基于 Tantivy 的全文检索后端实现。
pub struct TantivySearchBackend {
    index: Index,
    /// 查询侧切词（与索引侧 analyzer 共享同一词典实例）。
    jieba: std::sync::Arc<jieba_rs::Jieba>,
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
        let jieba = std::sync::Arc::new(jieba_rs::Jieba::new());
        register_jieba_with(&index, &jieba);
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
            jieba,
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
        let jieba = std::sync::Arc::new(jieba_rs::Jieba::new());
        register_jieba_with(&index, &jieba);
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
            jieba,
            id,
            title,
            content,
            tags,
            workspace_id,
            updated_at,
        })
    }

    /// 构建单篇文档。
    /// 查询串预处理: 「分布式 系统」→ ("分布" OR "布式" OR "分布式") AND "系统"。
    /// 子词与特殊字符经引号包裹转义（单 token phrase == TermQuery）。
    fn preprocess_query(&self, raw: &str) -> String {
        let escape = |w: &str| format!("\"{}\"", w.replace('"', ""));
        let words: Vec<&str> = raw.split_whitespace().collect();
        if words.is_empty() {
            return String::new();
        }
        let groups: Vec<String> = words
            .iter()
            .map(|w| {
                let tokens = self
                    .jieba
                    .tokenize(w, jieba_rs::TokenizeMode::Search, true);
                if tokens.is_empty() {
                    escape(w)
                } else {
                    // 去重（保序）后 OR 组合
                    let mut seen = std::collections::HashSet::new();
                    let subs: Vec<String> = tokens
                        .iter()
                        .map(|t| t.word)
                        .filter(|t| seen.insert(*t))
                        .map(escape)
                        .collect();
                    if subs.len() == 1 {
                        subs[0].clone()
                    } else {
                        format!("({})", subs.join(" OR "))
                    }
                }
            })
            .collect();
        groups.join(" AND ")
    }

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
        // 查询语义（对齐 ES match query）:
        // - 用户词组按空白分隔 → 词组间 AND
        // - 每个词经 jieba 切分子词 → 词内 OR（tantivy 默认把多 token 组
        //   短语查询，中文子词乱序 position 永不命中 — 实测踩坑）
        let processed = self.preprocess_query(query);
        let parsed = query_parser
            .parse_query(&processed)
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

    async fn doc_count(&self) -> Result<Option<usize>, Error> {
        let reader = self.index.reader().map_err(map_err)?;
        Ok(Some(reader.searcher().num_docs() as usize))
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

    /// V20 jieba: 中文子串检索（此前 SimpleTokenizer 整句成单 token 无法命中）。
    #[tokio::test]
    async fn search_chinese_substring_hits() {
        let backend = TantivySearchBackend::new_in_memory().unwrap();
        backend
            .index_note("n1", "架构投影验证：单一事实源与读模型", &metadata("中文笔记", "ws-1"))
            .await
            .unwrap();

        // 单词子串
        let r = backend.search("架构", &SearchOptions::default()).await.unwrap();
        assert_eq!(r.hits.len(), 1, "「架构」应命中正文: {:?}", r.hits);

        // 多词（AND 语义，QueryParser 用同一 jieba analyzer 切查询词）
        let r = backend.search("投影 验证", &SearchOptions::default()).await.unwrap();
        assert_eq!(r.hits.len(), 1);

        // 标题字段
        let r = backend.search("笔记", &SearchOptions::default()).await.unwrap();
        assert_eq!(r.hits.len(), 1, "「笔记」应命中标题");

        // 未出现的词
        let r = backend.search("区块链", &SearchOptions::default()).await.unwrap();
        assert_eq!(r.hits.len(), 0);
    }

    /// V20 jieba: 中英混排 + 大小写归一。
    #[tokio::test]
    async fn search_mixed_chinese_english() {
        let backend = TantivySearchBackend::new_in_memory().unwrap();
        backend
            .index_note("n1", "Aurora 本地优先笔记", &metadata("混合标题", "ws-1"))
            .await
            .unwrap();
        let r = backend.search("aurora", &SearchOptions::default()).await.unwrap();
        assert_eq!(r.hits.len(), 1, "英文小写应命中（LowerCaser）");
        let r = backend.search("本地", &SearchOptions::default()).await.unwrap();
        assert_eq!(r.hits.len(), 1, "中文词应命中");
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
