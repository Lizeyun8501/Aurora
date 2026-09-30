//! DK-03 S2：混合检索（BM25 + 向量 RRF 融合）——组合层方案（Alpha 冻结：
//! SearchBackend trait 签名零改动，单路语义保持）。
//!
//! RRF（Reciprocal Rank Fusion）标准常数 k=60：`score = Σ 1/(60 + rank_i)`，
//! 双路命中（HitSource::Both）天然排序靠前；各路拉取深度 = limit × 2（可配）。
//!
//! 向后兼容：SearchOptions.mode / SearchHit.source 均 serde default——旧序列化
//! 数据与旧调用方零破坏。trash 双路各自保证（S1 语义）；workspace 过滤走
//! SearchOptions.workspace_filter → VectorIndex 记录级 filter（None=匹配全部）。

use std::sync::Arc;

use crate::l2_engines::vector_search::{EmbedProvider, VectorIndex};
use crate::traits::search_backend::{
    HitSource, SearchBackend, SearchHit, SearchOptions, SearchResult,
};
use crate::Error;

/// RRF 常数（Cormack et al. 2009 标准值）。
pub const RRF_K: f32 = 60.0;

/// 混合检索器：BM25 路（tantivy）+ 向量路（VectorIndex KNN）→ RRF 融合。
pub struct HybridSearcher {
    bm25: Arc<dyn SearchBackend>,
    vectors: Arc<VectorIndex>,
    embed: Arc<dyn EmbedProvider>,
    /// 各路拉取深度倍率（深度 = limit × depth_factor，default 2——Alpha 冻结）。
    pub depth_factor: usize,
}

impl HybridSearcher {
    pub fn new(
        bm25: Arc<dyn SearchBackend>,
        vectors: Arc<VectorIndex>,
        embed: Arc<dyn EmbedProvider>,
    ) -> Self {
        Self {
            bm25,
            vectors,
            embed,
            depth_factor: 2,
        }
    }

    /// 混合检索：双路并行 → RRF 融合 → 截断 limit。
    ///
    /// - 查询向量 = `embed.embed(&[query])`（嵌入面 fail-closed 语义由 provider
    ///   侧保证——Deny 工作区走本地/拒绝，见 DK-10 门禁）；
    /// - snippet：Bm25 路沿用 tantivy 高亮；纯 Vector 命中用内容预览（index 时
    ///   截取前 120 字符，S1 旧记录 None → 空串诚实标注）。
    pub async fn search_hybrid(
        &self,
        query: &str,
        opts: &SearchOptions,
    ) -> Result<SearchResult, Error> {
        let t0 = std::time::Instant::now();
        let limit = if opts.limit == 0 { 20 } else { opts.limit };
        let deep = limit * self.depth_factor.max(1);

        // 各路拉取（BM25 用相同 opts——limit 在 tantivy 侧已按 limit=0→20 兜底，
        // 深度要求通过克隆 opts 覆写 limit 实现）
        let mut bm25_opts = opts.clone();
        bm25_opts.limit = deep;

        let bm25_fut = self.bm25.search(query, &bm25_opts);
        let embed = self.embed.clone();
        let q = query.to_string();
        let vec_fut = async move {
            let qvec = embed.embed(&[q.as_str()]).await?;
            let v = qvec.into_iter().next().unwrap_or_default();
            self.vectors
                .search_vector(&v, deep, opts.workspace_filter.as_deref())
                .await
        };
        let (bm25_res, vec_res) = tokio::join!(bm25_fut, vec_fut);
        let bm25_hits = bm25_res?.hits;
        let vec_hits = vec_res?;

        // RRF 融合：note_id → (rrf_score, source, hit)
        let mut fused: HashMap<String, (f32, HitSource, SearchHit)> = HashMap::new();
        for (rank, hit) in bm25_hits.iter().enumerate() {
            let e = fused
                .entry(hit.note_id.clone())
                .or_insert_with(|| (0.0, HitSource::Bm25, hit.clone()));
            e.0 += 1.0 / (RRF_K + rank as f32 + 1.0);
            e.1 = HitSource::Bm25;
        }
        let mut preview_of: HashMap<String, String> = HashMap::new();
        for (rank, vh) in vec_hits.iter().enumerate() {
            let e = fused.entry(vh.note_id.clone()).or_insert_with(|| {
                (
                    0.0,
                    HitSource::Vector,
                    SearchHit {
                        note_id: vh.note_id.clone(),
                        title: String::new(),
                        snippet: String::new(),
                        score: 0.0,
                        source: HitSource::Vector,
                    },
                )
            });
            e.0 += 1.0 / (RRF_K + rank as f32 + 1.0);
            e.1 = if e.1 == HitSource::Bm25 {
                HitSource::Both
            } else {
                HitSource::Vector
            };
            // S3：preview 随命中返回（与 content_hash 同步——纯向量 snippet 新鲜快照；
            // title 仍留空——调用方实时回填，改标题不触发重嵌故不入向量记录）
            preview_of.insert(vh.note_id.clone(), vh.preview.clone().unwrap_or_default());
        }

        let mut hits: Vec<SearchHit> = fused
            .into_iter()
            .map(|(id, (score, source, mut hit))| {
                hit.score = score;
                hit.source = source;
                if source == HitSource::Vector {
                    hit.snippet = preview_of.get(&id).cloned().unwrap_or_default();
                }
                hit
            })
            .collect();
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(limit);

        // 纯向量命中：snippet = content_preview（S3 起随 VectorHit 返回，与
        // content_hash 同步的新鲜快照；S1 旧记录 None → 空串诚实标注）。
        // title 留空由调用方实时回填（改标题不触发重嵌，不入向量记录）。
        Ok(SearchResult {
            hits,
            total: 0, // RRF 截断后 total=hits.len()（分页语义由 BM25 路承担）
            took_ms: t0.elapsed().as_millis() as u64,
        })
    }
}

use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::l1_infrastructure::search::TantivySearchBackend;
    use crate::l1_infrastructure::storage_engine::MemoryKVStore;
    use crate::l2_engines::vector_search::VectorIndex;
    use crate::traits::search_backend::SearchMode;

    /// 词袋 mock 嵌入：词典词 → 独立维度（语义命中=共享词，可断言 RRF 融合行为）。
    struct BagEmbed {
        vocab: Vec<&'static str>,
    }
    impl BagEmbed {
        fn new() -> Self {
            Self {
                vocab: vec!["苹果", "手机", "蛋糕", "做法", "锈", "语言"],
            }
        }
        fn vector_for(&self, text: &str) -> Vec<f32> {
            let mut v = vec![0.0f32; self.vocab.len()];
            for (i, w) in self.vocab.iter().enumerate() {
                if text.contains(w) {
                    v[i] = 1.0;
                }
            }
            if v.iter().all(|&x| x == 0.0) {
                v[0] = 0.01; // 全零退避（余弦分母保护）
            }
            v
        }
    }
    #[async_trait::async_trait]
    impl EmbedProvider for BagEmbed {
        async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, Error> {
            Ok(texts.iter().map(|t| self.vector_for(t)).collect())
        }
        fn model(&self) -> &str {
            "bag-mock"
        }
    }

    async fn fixture() -> (Arc<dyn SearchBackend>, Arc<VectorIndex>, Arc<BagEmbed>) {
        let bm25 = Arc::new(TantivySearchBackend::new_in_memory().unwrap());
        let kv: Arc<dyn crate::traits::kv_store::KVStore> = Arc::new(MemoryKVStore::default());
        let vec = Arc::new(VectorIndex::new(kv, "bag-mock", 6));
        let embed = Arc::new(BagEmbed::new());
        (bm25, vec, embed)
    }

    async fn index_both(
        bm25: &Arc<dyn SearchBackend>,
        vec: &Arc<VectorIndex>,
        id: &str,
        title: &str,
        content: &str,
        embed: &BagEmbed,
    ) -> Result<(), Error> {
        bm25.index_note(
            id,
            content,
            &crate::traits::search_backend::NoteMetadata {
                title: title.to_string(),
                ..Default::default()
            },
        )
        .await?;
        vec.index_note(id, content, embed).await?;
        Ok(())
    }

    fn searcher(
        bm25: Arc<dyn SearchBackend>,
        vec: Arc<VectorIndex>,
        embed: Arc<dyn EmbedProvider>,
    ) -> HybridSearcher {
        HybridSearcher::new(bm25, vec, embed)
    }

    /// T1 混合 round trip：命中集合正确 + source 标记符合双路命中事实。
    #[tokio::test]
    async fn hybrid_round_trip_sources() {
        let (bm25, vec, embed) = fixture().await;
        index_both(
            &bm25,
            &vec,
            "a1",
            "苹果手机评测",
            "苹果手机 全面评测",
            &embed,
        )
        .await
        .unwrap();
        index_both(
            &bm25,
            &vec,
            "a2",
            "蛋糕做法",
            "巧克力蛋糕 做法 步骤",
            &embed,
        )
        .await
        .unwrap();
        let embed_dyn: Arc<dyn EmbedProvider> = embed.clone();
        let s = searcher(bm25, vec, embed_dyn);
        let opts = SearchOptions {
            mode: SearchMode::Hybrid,
            ..SearchOptions::default()
        };
        let res = s.search_hybrid("苹果手机", &opts).await.unwrap();
        assert!(res.hits.iter().any(|h| h.note_id == "a1"));
        // 查询含「苹果手机」→ a1 双路命中（BM25 词命中 + 向量「苹果」「手机」维命中）
        assert_eq!(res.hits[0].source, HitSource::Both, "{:?}", res.hits);
    }

    /// T2 RRF 性质：双路命中排序高于单路命中。
    #[tokio::test]
    async fn rrf_both_beats_single() {
        let (bm25, vec, embed) = fixture().await;
        // both：标题+内容+向量共享词；single：仅向量命中（内容不含查询词但共享语义维）
        index_both(&bm25, &vec, "both", "苹果手机", "苹果手机 开箱", &embed)
            .await
            .unwrap();
        index_both(
            &bm25,
            &vec,
            "single",
            " unrelated ",
            "苹果 生态 讨论",
            &embed,
        )
        .await
        .unwrap();
        let embed_dyn: Arc<dyn EmbedProvider> = embed.clone();
        let s = searcher(bm25, vec, embed_dyn);
        let opts = SearchOptions {
            mode: SearchMode::Hybrid,
            ..SearchOptions::default()
        };
        let res = s.search_hybrid("苹果手机", &opts).await.unwrap();
        let pos = |id: &str| res.hits.iter().position(|h| h.note_id == id).unwrap();
        assert!(
            pos("both") < pos("single"),
            "双路命中应排前: {:?}",
            res.hits
        );
    }

    /// T3 单路退化：向量表空 → 等价 BM25（source 全 Bm25，不报错）。
    #[tokio::test]
    async fn degrades_to_bm25_when_no_vectors() {
        let (bm25, vec, embed) = fixture().await;
        index_both(&bm25, &vec, "a1", "苹果手机", "苹果手机 评测", &embed)
            .await
            .unwrap();
        // 向量缓存空（未 index_note 向量面——直接构造空 VectorIndex）
        let kv: Arc<dyn crate::traits::kv_store::KVStore> = Arc::new(MemoryKVStore::default());
        let empty_vec = Arc::new(VectorIndex::new(kv, "bag-mock", 6));
        let embed_dyn: Arc<dyn EmbedProvider> = embed.clone();
        let s = searcher(bm25, empty_vec, embed_dyn);
        let opts = SearchOptions {
            mode: SearchMode::Hybrid,
            ..SearchOptions::default()
        };
        let res = s.search_hybrid("苹果手机", &opts).await.unwrap();
        assert!(!res.hits.is_empty());
        assert!(res.hits.iter().all(|h| h.source == HitSource::Bm25));
    }

    /// T4 向后兼容：旧 JSON（无 mode/source）反序列化 Default 成功。
    #[tokio::test]
    async fn serde_backward_compat() {
        let opts_json = r#"{"limit": 10, "offset": 0}"#;
        let opts: SearchOptions = serde_json::from_str(opts_json).unwrap();
        assert_eq!(opts.mode, SearchMode::Bm25);
        let hit_json = r#"{"note_id":"n1","title":"t","snippet":"s","score":1.0}"#;
        let hit: SearchHit = serde_json::from_str(hit_json).unwrap();
        assert_eq!(hit.source, HitSource::Bm25);
    }

    /// DoD 3：万条混合 <500ms（#[ignore] 基准——release 本地跑）。
    #[tokio::test]
    #[ignore = "基准单测：万条混合本地跑（release）"]
    async fn hybrid_bench() {
        let (bm25, vec, embed) = fixture().await;
        for i in 0..10_000u32 {
            let id = format!("b{i}");
            bm25.index_note(
                &id,
                &format!("content {i} 苹果"),
                &crate::traits::search_backend::NoteMetadata {
                    title: format!("t{i}"),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            vec.index_note(&id, &format!("content {i} 苹果"), embed.as_ref())
                .await
                .unwrap();
        }
        let embed_dyn: Arc<dyn EmbedProvider> = embed.clone();
        let s = searcher(bm25, vec, embed_dyn);
        let opts = SearchOptions {
            mode: SearchMode::Hybrid,
            ..SearchOptions::default()
        };
        let t0 = std::time::Instant::now();
        let res = s.search_hybrid("苹果 手机", &opts).await.unwrap();
        let took = t0.elapsed();
        assert!(!res.hits.is_empty());
        assert!(took.as_millis() < 500, "万条混合应 <500ms, got {:?}", took);
    }

    /// T7 S3 纯向量 snippet：仅向量面索引的笔记（BM25 未索引）→ source=Vector
    /// 且 snippet = content_preview（S2 缺口关闭断言——纯向量结果可解释可展示）。
    #[tokio::test]
    async fn pure_vector_hit_snippet_filled_from_preview() {
        let (bm25, vec, embed) = fixture().await;
        // 只向量化不进 BM25 索引——模拟「BM25 不命中、纯语义召回」场景
        vec.embed_and_store_for_bench("x1", "苹果 生态 讨论", embed.vector_for("苹果 生态 讨论"))
            .await;
        let embed_dyn: Arc<dyn EmbedProvider> = embed.clone();
        let s = searcher(bm25, vec, embed_dyn);
        let opts = SearchOptions {
            mode: SearchMode::Hybrid,
            ..SearchOptions::default()
        };
        let res = s.search_hybrid("苹果手机 新品", &opts).await.unwrap();
        let x1 = res
            .hits
            .iter()
            .find(|h| h.note_id == "x1")
            .expect("向量共享维应命中 x1");
        assert_eq!(x1.source, HitSource::Vector);
        assert!(
            x1.snippet.contains("苹果 生态"),
            "纯向量 snippet 应为 preview 快照, got {:?}",
            x1.snippet
        );
        assert!(x1.title.is_empty(), "title 留空待调用方实时回填");
    }
}
