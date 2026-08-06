//! AI 智能系统（AI Intelligence System）
//!
//! 实现混合推理架构、智能续写、内容摘要、问答对话、混合搜索、语义搜索、自动标签、任务分解。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use parking_lot::RwLock;

use crate::traits::ai_provider::{AIProvider, ChatOptions, CompletionOptions, Message, Tool, ToolCall};

/// 推理策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InferenceStrategy {
    /// 优先本地模型
    LocalFirst,
    /// 仅使用云端
    CloudOnly,
    /// 自动选择（本地可用则用本地，否则云端）
    Auto,
}

/// AI Provider 路由
pub struct AIProviderRouter {
    local: Option<Arc<dyn AIProvider>>,
    cloud: Option<Arc<dyn AIProvider>>,
    strategy: InferenceStrategy,
}

impl AIProviderRouter {
    pub fn new(strategy: InferenceStrategy) -> Self {
        Self {
            local: None,
            cloud: None,
            strategy,
        }
    }

    pub fn with_local(mut self, provider: Arc<dyn AIProvider>) -> Self {
        self.local = Some(provider);
        self
    }

    pub fn with_cloud(mut self, provider: Arc<dyn AIProvider>) -> Self {
        self.cloud = Some(provider);
        self
    }

    fn select_provider(&self) -> Option<Arc<dyn AIProvider>> {
        match self.strategy {
            InferenceStrategy::LocalFirst => {
                self.local.as_ref().filter(|p| p.is_available()).cloned()
                    .or_else(|| self.cloud.clone())
            }
            InferenceStrategy::CloudOnly => self.cloud.clone(),
            InferenceStrategy::Auto => {
                self.local.as_ref().filter(|p| p.is_available()).cloned()
                    .or_else(|| self.cloud.clone())
            }
        }
    }

    pub async fn complete(&self, prompt: &str, opts: &CompletionOptions) -> Result<String, crate::Error> {
        match self.select_provider() {
            Some(provider) => provider.complete(prompt, opts).await,
            None => Err(crate::Error::Internal("No AI provider available".to_string())),
        }
    }

    pub async fn chat(&self, messages: &[Message], opts: &ChatOptions) -> Result<String, crate::Error> {
        match self.select_provider() {
            Some(provider) => provider.chat(messages, opts).await,
            None => Err(crate::Error::Internal("No AI provider available".to_string())),
        }
    }

    pub async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, crate::Error> {
        match self.select_provider() {
            Some(provider) => provider.embed(texts).await,
            None => Err(crate::Error::Internal("No AI provider available".to_string())),
        }
    }
}

/// 模拟 AI Provider（用于测试和离线场景）
pub struct MockAIProvider {
    responses: Arc<RwLock<HashMap<String, String>>>,
    embeddings: Arc<RwLock<HashMap<String, Vec<f32>>>>,
    available: bool,
}

impl Default for MockAIProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockAIProvider {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(RwLock::new(HashMap::new())),
            embeddings: Arc::new(RwLock::new(HashMap::new())),
            available: true,
        }
    }

    pub fn with_response(self, prompt_keyword: impl Into<String>, response: impl Into<String>) -> Self {
        self.responses.write().insert(prompt_keyword.into(), response.into());
        self
    }

    pub fn with_embedding(self, text: impl Into<String>, vector: Vec<f32>) -> Self {
        self.embeddings.write().insert(text.into(), vector);
        self
    }

    pub fn set_available(&mut self, available: bool) {
        self.available = available;
    }

    fn find_response(&self, prompt: &str) -> String {
        let responses = self.responses.read();
        for (keyword, response) in responses.iter() {
            if prompt.contains(keyword) {
                return response.clone();
            }
        }
        "Mock AI response".to_string()
    }
}

#[async_trait]
impl AIProvider for MockAIProvider {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, crate::Error> {
        let embeddings = self.embeddings.read();
        let mut result = Vec::new();
        for text in texts {
            let vec = embeddings.get(*text).cloned()
                .unwrap_or_else(|| vec![0.0; 384]);
            result.push(vec);
        }
        Ok(result)
    }

    async fn complete(&self, prompt: &str, _opts: &CompletionOptions) -> Result<String, crate::Error> {
        Ok(self.find_response(prompt))
    }

    fn stream_complete(&self, prompt: &str, _opts: &CompletionOptions, callback: Box<dyn Fn(String) + Send + Sync>) {
        let response = self.find_response(prompt);
        for word in response.split_whitespace() {
            callback(format!("{} ", word));
        }
    }

    async fn chat(&self, messages: &[Message], _opts: &ChatOptions) -> Result<String, crate::Error> {
        let last = messages.last().map(|m| m.content.clone()).unwrap_or_default();
        Ok(self.find_response(&last))
    }

    async fn function_call(&self, _prompt: &str, _tools: &[Tool]) -> Result<ToolCall, crate::Error> {
        Ok(ToolCall {
            tool_name: "mock_tool".to_string(),
            arguments: serde_json::json!({}),
        })
    }

    fn is_available(&self) -> bool {
        self.available
    }
}

/// 文档片段（用于 RAG）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunk {
    pub id: String,
    pub doc_id: String,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// RAG 问答引擎
pub struct RagEngine {
    router: Arc<AIProviderRouter>,
    chunks: Arc<RwLock<Vec<DocumentChunk>>>,
}

impl RagEngine {
    pub fn new(router: Arc<AIProviderRouter>) -> Self {
        Self {
            router,
            chunks: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn add_chunks(&self, chunks: Vec<DocumentChunk>) {
        self.chunks.write().extend(chunks);
    }

    /// 简单的余弦相似度计算
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot / (norm_a * norm_b)
        }
    }

    /// 检索相关片段
    pub fn retrieve(&self, query_embedding: &[f32], top_k: usize) -> Vec<(DocumentChunk, f32)> {
        let chunks = self.chunks.read();
        let mut scored: Vec<_> = chunks.iter()
            .filter_map(|chunk| {
                chunk.embedding.as_ref().map(|emb| {
                    let score = Self::cosine_similarity(query_embedding, emb);
                    (chunk.clone(), score)
                })
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        scored.into_iter().take(top_k).collect()
    }

    /// RAG 问答
    pub async fn ask(&self, question: &str, top_k: usize) -> Result<String, crate::Error> {
        // 1. Embedding 查询
        let query_embedding = self.router.embed(&[question]).await?;
        let query_embedding = query_embedding.into_iter().next().unwrap_or_default();

        // 2. 检索相关片段
        let retrieved = self.retrieve(&query_embedding, top_k);

        // 3. 构建上下文
        let context = retrieved.iter()
            .map(|(chunk, _)| chunk.content.clone())
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        // 4. 拼接 prompt 并生成答案
        let prompt = format!(
            "基于以下上下文回答问题。如果上下文中没有相关信息，请明确说明。\n\n上下文：\n{}\n\n问题：{}\n\n答案：",
            context, question
        );

        self.router.complete(&prompt, &CompletionOptions {
            max_tokens: Some(1024),
            temperature: Some(0.3),
            top_p: None,
            stop: None,
        }).await
    }
}

/// 摘要级别
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SummaryLevel {
    /// 段落级
    Paragraph,
    /// 文档级
    Document,
    /// 多文档级
    MultiDocument,
}

/// 摘要引擎
pub struct Summarizer {
    router: Arc<AIProviderRouter>,
}

impl Summarizer {
    pub fn new(router: Arc<AIProviderRouter>) -> Self {
        Self { router }
    }

    /// 生成摘要
    pub async fn summarize(&self, texts: &[String], level: SummaryLevel) -> Result<String, crate::Error> {
        match level {
            SummaryLevel::Paragraph => {
                let prompt = format!("请用一句话总结以下段落：\n\n{}", texts.join("\n\n"));
                self.router.complete(&prompt, &CompletionOptions {
                    max_tokens: Some(100),
                    temperature: Some(0.3),
                    top_p: None,
                    stop: None,
                }).await
            }
            SummaryLevel::Document => {
                let prompt = format!(
                    "请用 3-5 句话总结以下文档的核心内容：\n\n{}",
                    texts.join("\n\n")
                );
                self.router.complete(&prompt, &CompletionOptions {
                    max_tokens: Some(300),
                    temperature: Some(0.3),
                    top_p: None,
                    stop: None,
                }).await
            }
            SummaryLevel::MultiDocument => {
                // Map-Reduce 策略：先分别摘要，再合并
                let mut partials = Vec::new();
                for chunk in texts.chunks(3) {
                    let prompt = format!(
                        "请总结以下 {} 个文档的核心要点（每个文档一句话）：\n\n{}",
                        chunk.len(),
                        chunk.join("\n\n---\n\n")
                    );
                    let partial = self.router.complete(&prompt, &CompletionOptions {
                        max_tokens: Some(200),
                        temperature: Some(0.3),
                        top_p: None,
                        stop: None,
                    }).await?;
                    partials.push(partial);
                }

                let final_prompt = format!(
                    "基于以下各组文档的摘要，生成一个综合摘要：\n\n{}",
                    partials.join("\n\n")
                );
                self.router.complete(&final_prompt, &CompletionOptions {
                    max_tokens: Some(400),
                    temperature: Some(0.3),
                    top_p: None,
                    stop: None,
                }).await
            }
        }
    }
}

/// 自动标签引擎（TF-IDF + TextRank 简化版）
pub struct AutoTagger;

impl AutoTagger {
    /// 从文本中提取关键词作为候选标签
    pub fn extract_keywords(text: &str, top_k: usize) -> Vec<(String, f32)> {
        let words = Self::tokenize(text);
        let _doc_count = 1;
        let word_freq = Self::compute_tf(&words);

        // 停用词过滤
        let stop_words: HashSet<&str> = ["the", "a", "an", "is", "are", "was", "were",
            "this", "that", "these", "those", "and", "or", "but", "in", "on", "at", "to", "for",
            "的", "了", "在", "是", "我", "有", "和", "就", "不", "人", "都", "一", "一个", "上", "也", "很", "到", "说", "要", "去", "你", "会", "着", "没有", "看", "好", "自己", "这"]
            .iter().cloned().collect();

        let mut keywords: Vec<_> = word_freq.into_iter()
            .filter(|(word, _)| {
                let word_lower = word.to_lowercase();
                !stop_words.contains(word_lower.as_str()) && word.len() > 1
            })
            .map(|(word, tf)| {
                // 简化的 TF-IDF 分数（这里只有一篇文档，用词频作为近似）
                let score = tf * (word.len() as f32).sqrt();
                (word, score)
            })
            .collect();

        keywords.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        keywords.into_iter().take(top_k).collect()
    }

    fn tokenize(text: &str) -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase())
            .collect()
    }

    fn compute_tf(words: &[String]) -> HashMap<String, f32> {
        let total = words.len() as f32;
        let mut freq = HashMap::new();
        for word in words {
            *freq.entry(word.clone()).or_insert(0.0) += 1.0;
        }
        for count in freq.values_mut() {
            *count /= total;
        }
        freq
    }
}

/// 任务分解引擎
pub struct TaskDecomposer {
    router: Arc<AIProviderRouter>,
}

impl TaskDecomposer {
    pub fn new(router: Arc<AIProviderRouter>) -> Self {
        Self { router }
    }

    /// 将复杂任务分解为子任务列表
    pub async fn decompose(&self, task_description: &str) -> Result<Vec<String>, crate::Error> {
        let prompt = format!(
            "请将以下复杂任务分解为 3-7 个可执行的子任务。每个子任务一行，只输出子任务列表，不要添加编号或额外说明。\n\n任务：{}\n\n子任务：",
            task_description
        );

        let response = self.router.complete(&prompt, &CompletionOptions {
            max_tokens: Some(500),
            temperature: Some(0.5),
            top_p: None,
            stop: None,
        }).await?;

        let subtasks: Vec<String> = response.lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .map(|line| {
                // 去除常见的列表前缀
                line.trim_start_matches(['-', '*', '•'])
                    .trim_start_matches(|c: char| c.is_numeric())
                    .trim_start_matches('.')
                    .trim_start_matches(')')
                    .trim()
                    .to_string()
            })
            .filter(|s| !s.is_empty())
            .collect();

        Ok(subtasks)
    }
}

/// 智能续写请求
#[derive(Debug, Clone)]
pub struct ContinuationRequest {
    pub doc_id: String,
    pub cursor_position: usize,
    pub preceding_text: String,
    pub debounce_ms: u64,
}

/// 智能续写引擎
pub struct ContinuationEngine {
    router: Arc<AIProviderRouter>,
}

impl ContinuationEngine {
    pub fn new(router: Arc<AIProviderRouter>) -> Self {
        Self { router }
    }

    /// 生成续写建议（异步版本，实际使用时应配合 debounce）
    pub async fn suggest(&self, preceding_text: &str) -> Result<String, crate::Error> {
        let prompt = format!(
            "请根据以下文本的上下文，续写接下来的 1-2 句话。只输出续写的内容，不要重复原文。\n\n{}\n",
            preceding_text
        );

        self.router.complete(&prompt, &CompletionOptions {
            max_tokens: Some(100),
            temperature: Some(0.4),
            top_p: Some(0.9),
            stop: None,
        }).await
    }
}

/// 混合搜索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchResult {
    pub doc_id: String,
    pub content: String,
    pub keyword_score: f32,
    pub vector_score: f32,
    pub final_score: f32,
}

/// 混合搜索引擎
pub struct HybridSearcher {
    router: Arc<AIProviderRouter>,
}

impl HybridSearcher {
    pub fn new(router: Arc<AIProviderRouter>) -> Self {
        Self { router }
    }

    /// 混合搜索：关键词匹配 + 向量相似度（简化版，实际应结合 Tantivy 和 LanceDB）
    pub async fn search(
        &self,
        query: &str,
        documents: &[DocumentChunk],
        keyword_weight: f32,
        vector_weight: f32,
    ) -> Result<Vec<HybridSearchResult>, crate::Error> {
        let query_embedding = self.router.embed(&[query]).await?;
        let query_embedding = query_embedding.into_iter().next().unwrap_or_default();

        let query_terms: HashSet<String> = query.to_lowercase()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        let mut results = Vec::new();

        for doc in documents {
            // 关键词分数（简单 Jaccard 相似度）
            let doc_terms: HashSet<String> = doc.content.to_lowercase()
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
            let intersection: HashSet<_> = query_terms.intersection(&doc_terms).collect();
            let union: HashSet<_> = query_terms.union(&doc_terms).collect();
            let keyword_score = if union.is_empty() {
                0.0
            } else {
                intersection.len() as f32 / union.len() as f32
            };

            // 向量分数
            let vector_score = doc.embedding.as_ref()
                .map(|emb| RagEngine::cosine_similarity(&query_embedding, emb))
                .unwrap_or(0.0);

            let final_score = keyword_score * keyword_weight + vector_score * vector_weight;

            results.push(HybridSearchResult {
                doc_id: doc.doc_id.clone(),
                content: doc.content.clone(),
                keyword_score,
                vector_score,
                final_score,
            });
        }

        results.sort_by(|a, b| b.final_score.partial_cmp(&a.final_score).unwrap());
        Ok(results)
    }
}

/// AI 系统主引擎
pub struct AISystemEngine {
    pub router: Arc<AIProviderRouter>,
    pub rag: RagEngine,
    pub summarizer: Summarizer,
    pub task_decomposer: TaskDecomposer,
    pub continuation: ContinuationEngine,
    pub hybrid_searcher: HybridSearcher,
}

impl AISystemEngine {
    pub fn new(router: Arc<AIProviderRouter>) -> Self {
        Self {
            router: router.clone(),
            rag: RagEngine::new(router.clone()),
            summarizer: Summarizer::new(router.clone()),
            task_decomposer: TaskDecomposer::new(router.clone()),
            continuation: ContinuationEngine::new(router.clone()),
            hybrid_searcher: HybridSearcher::new(router.clone()),
        }
    }

    /// 自动为文档生成标签
    pub fn auto_tag(&self, content: &str, max_tags: usize) -> Vec<String> {
        AutoTagger::extract_keywords(content, max_tags)
            .into_iter()
            .map(|(word, _)| word)
            .collect()
    }

    /// 语义搜索（向量相似度）
    pub async fn semantic_search(
        &self,
        query: &str,
        documents: &[DocumentChunk],
        top_k: usize,
    ) -> Result<Vec<(DocumentChunk, f32)>, crate::Error> {
        let query_embedding = self.router.embed(&[query]).await?;
        let query_embedding = query_embedding.into_iter().next().unwrap_or_default();

        let mut scored: Vec<_> = documents.iter()
            .filter_map(|doc| {
                doc.embedding.as_ref().map(|emb| {
                    let score = RagEngine::cosine_similarity(&query_embedding, emb);
                    (doc.clone(), score)
                })
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        Ok(scored.into_iter().take(top_k).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_mock_router() -> Arc<AIProviderRouter> {
        let mock = Arc::new(MockAIProvider::new()
            .with_response("summary", "This is a summary.")
            .with_response("question", "Based on the context, the answer is 42.")
            .with_response("子任务", "Research topic\nGather materials\nWrite draft")
            .with_response("continue", "continuing the thought further."));

        Arc::new(AIProviderRouter::new(InferenceStrategy::Auto)
            .with_local(mock))
    }

    #[tokio::test]
    async fn test_mock_provider() {
        let mock = MockAIProvider::new()
            .with_response("hello", "Hi there!");

        let result = mock.complete("Say hello", &CompletionOptions {
            max_tokens: None,
            temperature: None,
            top_p: None,
            stop: None,
        }).await.unwrap();

        assert_eq!(result, "Hi there!");
    }

    #[tokio::test]
    async fn test_router_selects_available() {
        let mut local = MockAIProvider::new();
        local.set_available(false);
        let local = Arc::new(local);

        let cloud = Arc::new(MockAIProvider::new()
            .with_response("test", "cloud response"));

        let router = AIProviderRouter::new(InferenceStrategy::Auto)
            .with_local(local)
            .with_cloud(cloud);

        let result = router.complete("test", &CompletionOptions {
            max_tokens: None,
            temperature: None,
            top_p: None,
            stop: None,
        }).await.unwrap();

        assert_eq!(result, "cloud response");
    }

    #[test]
    fn test_rag_retrieve() {
        let router = create_mock_router();
        let rag = RagEngine::new(router);

        rag.add_chunks(vec![
            DocumentChunk {
                id: "1".to_string(),
                doc_id: "doc1".to_string(),
                content: "Rust is a systems programming language.".to_string(),
                embedding: Some(vec![1.0, 0.0, 0.0]),
                metadata: HashMap::new(),
            },
            DocumentChunk {
                id: "2".to_string(),
                doc_id: "doc2".to_string(),
                content: "Python is great for data science.".to_string(),
                embedding: Some(vec![0.0, 1.0, 0.0]),
                metadata: HashMap::new(),
            },
        ]);

        let results = rag.retrieve(&[1.0, 0.1, 0.0], 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0.doc_id, "doc1");
    }

    #[tokio::test]
    async fn test_summarizer() {
        let router = create_mock_router();
        let summarizer = Summarizer::new(router);

        let result = summarizer.summarize(
            &["Long text about something important.".to_string()],
            SummaryLevel::Paragraph,
        ).await.unwrap();

        assert!(!result.is_empty());
    }

    #[tokio::test]
    async fn test_task_decomposer() {
        let router = create_mock_router();
        let decomposer = TaskDecomposer::new(router);

        let subtasks = decomposer.decompose("Build a website").await.unwrap();
        assert_eq!(subtasks.len(), 3);
        assert_eq!(subtasks[0], "Research topic");
    }

    #[test]
    fn test_auto_tagger() {
        let text = "Rust programming language for systems development and web assembly";
        let keywords = AutoTagger::extract_keywords(text, 5);

        assert!(!keywords.is_empty());
        // "rust", "programming", "language" 等应该出现
        let words: Vec<_> = keywords.iter().map(|(w, _)| w.as_str()).collect();
        assert!(words.contains(&"rust") || words.contains(&"programming"));
    }

    #[tokio::test]
    async fn test_hybrid_search() {
        let router = create_mock_router();
        let searcher = HybridSearcher::new(router);

        let docs = vec![
            DocumentChunk {
                id: "1".to_string(),
                doc_id: "doc1".to_string(),
                content: "Rust memory safety without garbage collector".to_string(),
                embedding: Some(vec![1.0, 0.0]),
                metadata: HashMap::new(),
            },
            DocumentChunk {
                id: "2".to_string(),
                doc_id: "doc2".to_string(),
                content: "Python dynamic typing and easy syntax".to_string(),
                embedding: Some(vec![0.0, 1.0]),
                metadata: HashMap::new(),
            },
        ];

        let results = searcher.search("rust safety", &docs, 0.5, 0.5).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0].final_score >= results[1].final_score);
    }

    #[tokio::test]
    async fn test_continuation() {
        let router = create_mock_router();
        let engine = ContinuationEngine::new(router);

        let suggestion = engine.suggest("Once upon a time").await.unwrap();
        assert!(!suggestion.is_empty());
    }

    #[test]
    fn test_ai_system_engine() {
        let router = create_mock_router();
        let ai = AISystemEngine::new(router);

        let tags = ai.auto_tag("Rust programming for systems development", 3);
        assert!(!tags.is_empty());
    }
}