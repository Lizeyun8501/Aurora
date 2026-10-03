//! DK-03 S1：向量检索基建（纯 Rust 暴力 KNN——Alpha 改判：不装 sqlite-vec，
//! 万条 × 768 维 ≈ 10ms 级远够用，零分发风险零 unsafe；>5 万条再评估）。
//!
//! 架构：
//! - 存储：KV `notevec:{note_id}` → [`VecRecord`]（dim/model/vector/content_hash）；
//! - 抽象：[`EmbedProvider`]（嵌入供给方）——Ollama 本地实现在 aurora-ai（reqwest
//!   已在该 crate），云端 fallback 经 `EmbedFromAiProvider` 适配 `AIProvider`，
//!   DK-10 门禁自动生效（Deny 工作区出网请求离机前被拒）；
//! - 检索：[`VectorIndex::search_vector`] 内存缓存 + 余弦相似度暴力 KNN，
//!   trash 过滤复用 DK-02 软删语义（`trash:` 前缀在册者不参与检索）；
//! - 管线：[`VectorIndex::index_note`]（内容 hash 去重 + 维度/模型不匹配淘汰重建）
//!   + [`VectorIndex::backfill_missing`]（boot 后批量补齐，限速防 Ollama 过载）。
//!
//! 挂起（诚实化）：Private 工作区锁定态内存索引（DK-03 DoD 3——加密面，S2 与
//! DK-20 合并裁决）；reranker（S3 可选）；sqlite-vec（>5 万条再评估）。

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::app_core::AppCore;
use crate::traits::kv_store::KVStore;
use crate::write_path::ENC_AES256GCM;
use crate::Error;

/// 单篇笔记的向量记录（`notevec:{note_id}` 的值）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VecRecord {
    /// 向量维度（与 `vector.len()` 一致——反序列化后校验，不一致视为损坏淘汰）。
    pub dim: u32,
    pub vector: Vec<f32>,
    /// 嵌入模型名（nomic-embed-text 口径）——模型不匹配的旧向量淘汰重建。
    pub model: String,
    /// 内容指纹（sha256 hex）——去重依据：内容未变跳过重嵌入。
    pub content_hash: String,
    /// DK-03 S2：工作区归属（None=单工作区口径，匹配全部 filter——S1 旧记录兼容）。
    #[serde(default)]
    pub workspace_id: Option<String>,
    /// DK-20：笔记密级（"none" | "aes256gcm"）——锁定篇不入缓存（写入侧跳过）
    /// 且检索侧防御过滤（双保险）；S1 旧记录 default "none" 兼容。
    #[serde(default)]
    pub encryption: String,
    /// DK-03 S2：纯向量命中 snippet 用内容预览（index 时截取；S1 旧记录 None=空）。
    #[serde(default)]
    pub content_preview: Option<String>,
}

/// 单条向量检索命中（DK-03 S3：preview 随命中返回——纯向量 snippet 数据源，
/// 与 content_hash 同步更新故快照新鲜；title 不入向量记录——改标题不触发重嵌
/// 会过期，title 回填由调用方实时查 note store，见 desktop cmd 层）。
#[derive(Debug, Clone, PartialEq)]
pub struct VectorHit {
    pub note_id: String,
    /// 余弦相似度（降序排序键）。
    pub score: f32,
    /// 内容预览快照（index 时截取前 120 字符；S1 旧记录 None → UI 诚实留空）。
    pub preview: Option<String>,
}

/// 嵌入供给方抽象（实现方：aurora-ai 的 OllamaEmbedProvider 本地主路径 /
/// EmbedFromAiProvider 云 fallback 适配器 / 测试 Mock）。
#[async_trait::async_trait]
pub trait EmbedProvider: Send + Sync {
    /// 批量嵌入（与 `AIProvider::embed` 同构，保持 trait 生态一致）。
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, Error>;
    /// 嵌入模型名（写入 VecRecord.model，供不匹配淘汰判定）。
    fn model(&self) -> &str;
}

/// 索引动作结果（调用方可观测——半接入三查：管线行为可断言）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedOutcome {
    /// 新写入向量。
    Indexed,
    /// 内容 hash 未变 + 模型/维度匹配 → 跳过重嵌入（去重）。
    SkippedUnchanged,
    /// 旧向量模型/维度不匹配 → 淘汰后按当前 provider 重建。
    EvictedRebuilt,
}

/// 向量索引（KV 权威 + 内存缓存 + 暴力 KNN）。
pub struct VectorIndex {
    kv: std::sync::Arc<dyn KVStore>,
    cache: RwLock<HashMap<String, VecRecord>>,
    /// 期望模型名——不匹配的旧向量淘汰重建（维度配置化，默认 768 nomic-embed-text）。
    model: String,
    dim: u32,
}

impl VectorIndex {
    /// 构造（内存缓存惰性预热：首次查询/索引时 `refresh_cache`）。
    pub fn new(kv: std::sync::Arc<dyn KVStore>, model: impl Into<String>, dim: u32) -> Self {
        Self {
            kv,
            cache: RwLock::new(HashMap::new()),
            model: model.into(),
            dim,
        }
    }

    /// 期望维度。
    pub fn dim(&self) -> u32 {
        self.dim
    }

    /// 期望模型名。
    pub fn model(&self) -> &str {
        &self.model
    }

    /// 从 KV 全量预热内存缓存（boot / 测试夹具用；幂等覆盖）。
    pub async fn refresh_cache(&self) -> Result<usize, Error> {
        // scan 在锁外 await（MutexGuard 不得跨 await——clippy await_holding_lock）
        let pairs = self.kv.scan_prefix("notevec:").await?;
        let mut map = self
            .cache
            .write()
            .map_err(|_| Error::Internal("vector cache lock poisoned".into()))?;
        let mut n = 0usize;
        for (k, v) in pairs {
            if let Ok(rec) = serde_json::from_slice::<VecRecord>(&v) {
                if rec.dim as usize == rec.vector.len() {
                    map.insert(k.trim_start_matches("notevec:").to_string(), rec);
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    /// 物理删除单条向量记录（DK-03 S3b purge 联动：trash 软删不删记录、靠检索面
    /// 过滤；purge 后残留会命中已删笔记，故须物理清 + 缓存同步移除）。
    pub async fn remove_note(&self, note_id: &str) -> Result<(), Error> {
        self.kv.delete(&format!("notevec:{note_id}")).await?;
        let mut cache = self
            .cache
            .write()
            .map_err(|_| Error::Internal("vector cache lock poisoned".into()))?;
        cache.remove(note_id);
        Ok(())
    }

    fn content_hash(content: &str) -> String {
        let mut h = Sha256::new();
        h.update(content.as_bytes());
        format!("{:x}", h.finalize())
    }

    /// 索引单篇（去重 + 不匹配淘汰）。
    ///
    /// - 内容 hash 与现存一致且模型/维度匹配 → `SkippedUnchanged`（不调 embed）；
    /// - 模型或维度不匹配 → `EvictedRebuilt`（旧向量淘汰，按当前 provider 重嵌）；
    /// - 其余 → `Indexed`。
    pub async fn index_note(
        &self,
        note_id: &str,
        content: &str,
        embed: &dyn EmbedProvider,
    ) -> Result<EmbedOutcome, Error> {
        let hash = Self::content_hash(content);
        let cached = self
            .cache
            .read()
            .map_err(|_| Error::Internal("vector cache lock poisoned".into()))?
            .get(note_id)
            .cloned();
        if let Some(ref rec) = cached {
            if rec.content_hash == hash && rec.model == self.model && rec.dim == self.dim {
                return Ok(EmbedOutcome::SkippedUnchanged);
            }
        }
        let outcome = if cached.is_some() {
            EmbedOutcome::EvictedRebuilt
        } else {
            EmbedOutcome::Indexed
        };
        self.embed_and_store(note_id, content, &hash, embed).await?;
        Ok(outcome)
    }

    async fn embed_and_store(
        &self,
        note_id: &str,
        content: &str,
        hash: &str,
        embed: &dyn EmbedProvider,
    ) -> Result<(), Error> {
        let vectors = embed.embed(&[content]).await?;
        let vector = vectors
            .into_iter()
            .next()
            .ok_or_else(|| Error::Internal("embed provider returned no vector".into()))?;
        if vector.len() != self.dim as usize {
            return Err(Error::Internal(format!(
                "embed dim mismatch: provider returned {}, expected {}",
                vector.len(),
                self.dim
            )));
        }
        let record = VecRecord {
            dim: self.dim,
            vector,
            model: self.model.clone(),
            content_hash: hash.to_string(),
            // DK-03 S2：单工作区口径（None=匹配全部 filter）；preview 取内容前 120 字符
            workspace_id: None,
            // DK-20：嵌入路径仅接受明文（enc1 密文已在 backfill 防御跳过）
            encryption: "none".to_string(),
            content_preview: Some(content.chars().take(120).collect()),
        };
        self.kv
            .set(
                &format!("notevec:{note_id}"),
                &serde_json::to_vec(&record).map_err(|e| Error::Internal(e.to_string()))?,
            )
            .await?;
        self.cache
            .write()
            .map_err(|_| Error::Internal("vector cache lock poisoned".into()))?
            .insert(note_id.to_string(), record);
        Ok(())
    }

    /// 显式淘汰单篇向量（笔记物理删除时调用；软删不淘汰——恢复后即刻可用）。
    pub async fn evict(&self, note_id: &str) -> Result<(), Error> {
        self.kv.delete(&format!("notevec:{note_id}")).await?;
        if let Ok(mut map) = self.cache.write() {
            map.remove(note_id);
        }
        Ok(())
    }

    /// 回收站中的笔记 id 集合（每次检索实时扫描——DK-02 软删语义，量小成本可忽略）。
    async fn trashed_ids(&self) -> HashSet<String> {
        self.kv
            .scan_prefix("trash:")
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(k, _)| k.trim_start_matches("trash:").to_string())
            .collect()
    }

    /// 暴力 KNN 检索（余弦相似度，降序取前 k；trash 过滤复用 DK-02 语义）。
    ///
    /// `query` 维度必须与索引维度一致（不匹配报错——向量口径错误应显式暴露）。
    /// **S3 签名变更列明**：返回 `(note_id, f32)` → [`VectorHit`]（preview 随
    /// 命中返回，S2 留缺口在此关闭）；调用点仅 hybrid_search 与测试（core 内）。
    pub async fn search_vector(
        &self,
        query: &[f32],
        k: usize,
        ws_filter: Option<&str>,
    ) -> Result<Vec<VectorHit>, Error> {
        if query.len() != self.dim as usize {
            return Err(Error::InvalidInput(format!(
                "query dim mismatch: got {}, expected {}",
                query.len(),
                self.dim
            )));
        }
        let trashed = self.trashed_ids().await;
        let cache = self
            .cache
            .read()
            .map_err(|_| Error::Internal("vector cache lock poisoned".into()))?;
        let mut scored: Vec<VectorHit> = cache
            .iter()
            .filter(|(id, _)| !trashed.contains(id.as_str()))
            // DK-20：锁定篇防御过滤（写入侧已跳过，此处兜底防存量残留）
            .filter(|(_, rec)| rec.encryption != ENC_AES256GCM)
            // DK-03 S2：workspace 过滤（记录 None=单工作区口径匹配全部；Some(w)≠ws 跳过）
            .filter(|(_, rec)| match (ws_filter, rec.workspace_id.as_deref()) {
                (Some(ws), Some(w)) => w == ws,
                _ => true,
            })
            .map(|(id, rec)| VectorHit {
                note_id: id.clone(),
                score: Self::cosine(query, &rec.vector),
                preview: rec.content_preview.clone(),
            })
            .collect();
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(k);
        Ok(scored)
    }

    /// 批量回填缺失/过时向量（boot 后补齐；限速防 Ollama 过载）。
    ///
    /// 扫描 `note:` 全量，凡「无 notevec: 或 content_hash/模型/维度不匹配」者
    /// 逐篇重嵌；每篇间隔 `min_interval`。返回回填篇数。
    pub async fn backfill_missing(
        &self,
        // S3b 签名变更列明：+ Send + Sync——desktop 侧 tauri::async_runtime::spawn
        // 要求 future Send，&dyn Fn 跨 .await 持有必须 Send+Sync（语义零变化）
        content_of: &(dyn Fn(&str) -> Option<String> + Send + Sync),
        embed: &dyn EmbedProvider,
        limit: usize,
        min_interval: Duration,
    ) -> Result<usize, Error> {
        let mut done = 0usize;
        for (k, _) in self.kv.scan_prefix("note:").await? {
            if done >= limit {
                break;
            }
            let note_id = match k.strip_prefix("note:") {
                Some(id) => id.to_string(),
                None => continue,
            };
            if self.trashed_ids().await.contains(&note_id) {
                continue; // 回收站笔记不回填（恢复时按需重建）
            }
            let Some(content) = content_of(&note_id) else {
                continue;
            };
            // DK-20：enc1 密文（锁定篇）不回填嵌入——向量面无密文亦无明文语义
            if content.starts_with("enc1:") {
                continue;
            }
            let hash = Self::content_hash(&content);
            let cached = self
                .cache
                .read()
                .map_err(|_| Error::Internal("vector cache lock poisoned".into()))?
                .get(&note_id)
                .cloned();
            let needs = match cached {
                None => true,
                Some(r) => r.content_hash != hash || r.model != self.model || r.dim != self.dim,
            };
            if !needs {
                continue;
            }
            self.embed_and_store(&note_id, &content, &hash, embed)
                .await?;
            done += 1;
            tokio::time::sleep(min_interval).await;
        }
        Ok(done)
    }

    /// 基准专用：绕过 embed 直接落向量（bench 只测 KNN 遍历成本）。
    #[cfg(test)]
    pub async fn embed_and_store_for_bench(&self, note_id: &str, content: &str, vector: Vec<f32>) {
        let hash = Self::content_hash(content);
        let rec = VecRecord {
            dim: self.dim,
            vector,
            model: self.model.clone(),
            content_hash: hash,
            workspace_id: None,
            encryption: "none".to_string(),
            content_preview: Some(content.chars().take(120).collect()),
        };
        self.kv
            .set(
                &format!("notevec:{note_id}"),
                &serde_json::to_vec(&rec).unwrap_or_default(),
            )
            .await
            .ok();
        self.cache
            .write()
            .expect("vector cache lock poisoned")
            .insert(note_id.to_string(), rec);
    }

    /// 余弦相似度（调用方保证等长——search_vector 入口已校验维度）。
    fn cosine(a: &[f32], b: &[f32]) -> f32 {
        let (mut dot, mut na, mut nb) = (0.0f32, 0.0f32, 0.0f32);
        for i in 0..a.len() {
            dot += a[i] * b[i];
            na += a[i] * a[i];
            nb += b[i] * b[i];
        }
        let denom = na.sqrt() * nb.sqrt();
        if denom == 0.0 {
            0.0
        } else {
            dot / denom
        }
    }
}

/// 直接以 AppCore 构造（生产装配便捷入口——半接入三查：core 侧生产可见）。
pub fn vector_index_for(core: &AppCore, model: impl Into<String>, dim: u32) -> VectorIndex {
    VectorIndex::new(core.kv_store.clone(), model, dim)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::l1_infrastructure::storage_engine::MemoryKVStore;
    use std::sync::Arc;

    /// 确定性 mock 嵌入：文本 hash 驱动向量（同文本同向量，可断言排序）。
    struct MockEmbed {
        dim: u32,
        calls: std::sync::atomic::AtomicUsize,
    }
    impl MockEmbed {
        fn new(dim: u32) -> Self {
            Self {
                dim,
                calls: std::sync::atomic::AtomicUsize::new(0),
            }
        }
        fn calls(&self) -> usize {
            self.calls.load(std::sync::atomic::Ordering::SeqCst)
        }
        fn vector_for(&self, text: &str) -> Vec<f32> {
            let mut h = Sha256::digest(text.as_bytes());
            let mut v = Vec::with_capacity(self.dim as usize);
            while v.len() < self.dim as usize {
                for b in h.iter() {
                    v.push((*b as f32 - 128.0) / 128.0);
                    if v.len() == self.dim as usize {
                        break;
                    }
                }
                h = Sha256::digest(h);
            }
            v
        }
    }
    #[async_trait::async_trait]
    impl EmbedProvider for MockEmbed {
        async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, Error> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(texts.iter().map(|t| self.vector_for(t)).collect())
        }
        fn model(&self) -> &str {
            "mock-embed"
        }
    }

    async fn boot() -> Arc<MemoryKVStore> {
        Arc::new(MemoryKVStore::default())
    }

    async fn seed_note(kv: &MemoryKVStore, note_id: &str, title: &str) {
        let rec = serde_json::json!({
            "id": note_id, "title": title, "content": "",
            "created_at": "2026-09-28T00:00:00+00:00",
            "updated_at": "2026-09-28T00:00:00+00:00",
            "encryption": "none"
        });
        kv.set(&format!("note:{note_id}"), &rec.to_string().into_bytes())
            .await
            .unwrap();
    }

    /// T1 索引→检索 round trip：自身 top1 + 相关排序。
    #[tokio::test]
    async fn index_search_round_trip() {
        let kv = boot().await;
        seed_note(&kv, "n1", "rust").await;
        seed_note(&kv, "n2", "cooking").await;
        let idx = VectorIndex::new(kv.clone() as Arc<dyn KVStore>, "mock-embed", 64);
        let e = MockEmbed::new(64);
        idx.index_note("n1", "rust systems programming", &e)
            .await
            .unwrap();
        idx.index_note("n2", "chocolate cake recipe", &e)
            .await
            .unwrap();

        let q = e.vector_for("rust systems programming");
        let hits = idx.search_vector(&q, 2, None).await.unwrap();
        assert_eq!(hits[0].note_id, "n1", "自身应 top1");
        assert!(
            hits[0]
                .preview
                .as_deref()
                .unwrap_or_default()
                .contains("rust"),
            "S3：preview 应随命中返回（纯向量 snippet 数据源）"
        );
        let q2 = e.vector_for("chocolate cake recipe");
        let hits2 = idx.search_vector(&q2, 2, None).await.unwrap();
        assert_eq!(hits2[0].note_id, "n2", "另一查询 top1 是 n2");
    }

    /// T2 trash 过滤：软删笔记不参与检索（DK-02 联动——复用 trash: 前缀语义）。
    #[tokio::test]
    async fn search_excludes_trashed() {
        let kv = boot().await;
        seed_note(&kv, "n1", "a").await;
        seed_note(&kv, "n2", "b").await;
        let idx = VectorIndex::new(kv.clone() as Arc<dyn KVStore>, "mock-embed", 64);
        let e = MockEmbed::new(64);
        idx.index_note("n1", "alpha content", &e).await.unwrap();
        idx.index_note("n2", "alpha content twin", &e)
            .await
            .unwrap();

        // 软删 n2（trash: 标记键——delete_note 同款语义）
        kv.set("trash:n2", b"{\"deleted_at_ms\":1,\"title\":\"b\"}")
            .await
            .unwrap();
        // notevec: 保留（恢复数据源——检索面过滤而非物理删）
        let q = e.vector_for("alpha content");
        let hits = idx.search_vector(&q, 5, None).await.unwrap();
        assert!(
            !hits.iter().any(|h| h.note_id == "n2"),
            "trash 中笔记不得返回"
        );
        assert!(hits.iter().any(|h| h.note_id == "n1"));
        // 向量记录仍在（restore 后可检索）
        assert!(kv.get("notevec:n2").await.unwrap().is_some());
    }

    /// T8 purge 联动（S3b）：remove_note 物理删记录 + 缓存同步——检索不再返回。
    #[tokio::test]
    async fn remove_note_purges_record_and_cache() {
        let kv = boot().await;
        let idx = Arc::new(VectorIndex::new(kv.clone() as Arc<dyn KVStore>, "m", 2));
        let e = MockEmbed::new(2);
        idx.index_note("n1", "alpha content", &e).await.unwrap();
        idx.remove_note("n1").await.unwrap();
        assert!(
            kv.get("notevec:n1").await.unwrap().is_none(),
            "记录应物理删除"
        );
        let q = e.vector_for("alpha content");
        let hits = idx.search_vector(&q, 5, None).await.unwrap();
        assert!(hits.is_empty(), "缓存已同步，检索不得返回: {hits:?}");
    }

    /// T3 维度/模型不匹配淘汰：旧向量淘汰重建（DoD 2——配置化）。
    #[tokio::test]
    async fn dim_mismatch_evicts() {
        let kv = boot().await;
        seed_note(&kv, "n1", "a").await;
        let idx = VectorIndex::new(kv.clone() as Arc<dyn KVStore>, "mock-embed", 64);
        let e64 = MockEmbed::new(64);
        idx.index_note("n1", "same text", &e64).await.unwrap();
        assert_eq!(e64.calls(), 1);

        // 维度变更（换模型口径）：重新 index 应淘汰重建
        let idx2 = VectorIndex::new(kv.clone() as Arc<dyn KVStore>, "mock-embed", 128);
        let e128 = MockEmbed::new(128);
        idx2.refresh_cache().await.unwrap();
        let out = idx2.index_note("n1", "same text", &e128).await.unwrap();
        assert!(
            matches!(out, EmbedOutcome::EvictedRebuilt),
            "维度不匹配必须淘汰重建, got {out:?}"
        );
        assert_eq!(e128.calls(), 1, "新 provider 应实际重嵌");
        assert!(kv.get("notevec:n1").await.unwrap().unwrap().len() > 500);
    }

    /// T4 内容 hash 去重：同内容重复 index 不重嵌（省 Ollama 算力）。
    #[tokio::test]
    async fn content_hash_dedup() {
        let kv = boot().await;
        seed_note(&kv, "n1", "a").await;
        let idx = VectorIndex::new(kv.clone() as Arc<dyn KVStore>, "mock-embed", 64);
        let e = MockEmbed::new(64);
        let o1 = idx.index_note("n1", "identical text", &e).await.unwrap();
        let o2 = idx.index_note("n1", "identical text", &e).await.unwrap();
        assert!(matches!(o1, EmbedOutcome::Indexed));
        assert!(matches!(o2, EmbedOutcome::SkippedUnchanged));
        assert_eq!(e.calls(), 1, "同内容第二次不得重嵌");
    }

    /// T5 批量回填：缺向量/哈希过期的补齐（boot 后补齐口径）。
    #[tokio::test]
    async fn backfill_missing_only() {
        let kv = boot().await;
        seed_note(&kv, "n1", "a").await;
        seed_note(&kv, "n2", "b").await;
        seed_note(&kv, "n3", "c").await;
        let idx = Arc::new(VectorIndex::new(
            kv.clone() as Arc<dyn KVStore>,
            "mock-embed",
            64,
        ));
        let e = Arc::new(MockEmbed::new(64));
        idx.index_note("n1", "n1 content", e.as_ref())
            .await
            .unwrap();

        let content_of = |id: &str| Some(format!("{id} content"));
        let done = idx
            .backfill_missing(&content_of, e.as_ref(), 10, Duration::from_millis(0))
            .await
            .unwrap();
        assert_eq!(done, 2, "n2/n3 缺向量应补齐");
        let done2 = idx
            .backfill_missing(&content_of, e.as_ref(), 10, Duration::from_millis(0))
            .await
            .unwrap();
        assert_eq!(done2, 0, "补齐后无欠账");
    }

    /// DoD 3：万条 × 768 维暴力 KNN <50ms（#[ignore] 基准——本地跑：cargo test -p aurora-core
    ///   --release -- --ignored vector_knn_bench）。
    #[tokio::test]
    #[ignore = "基准单测：万条向量本地跑（release）"]
    async fn vector_knn_bench() {
        let kv = boot().await;
        let idx = Arc::new(VectorIndex::new(
            kv.clone() as Arc<dyn KVStore>,
            "bench",
            768,
        ));
        let e = MockEmbed::new(768);
        for i in 0..10_000u32 {
            let id = format!("b{i}");
            idx.embed_and_store_for_bench(
                &id,
                &format!("content {i}"),
                e.vector_for(&format!("content {i}")),
            )
            .await;
        }
        let q = e.vector_for("content 5000");
        let t0 = std::time::Instant::now();
        let hits = idx.search_vector(&q, 10, None).await.unwrap();
        let took = t0.elapsed();
        assert_eq!(hits.len(), 10);
        assert!(took.as_millis() < 50, "万条 KNN 应 <50ms, got {:?}", took);
    }
}
