//! DK-03 S1：嵌入供给方实现（EmbedProvider trait 在 aurora-core）。
//!
//! - [`OllamaEmbedProvider`]：本地 Ollama `/api/embeddings` 主路径（V19 §7.2
//!   「本地 AI」口径——嵌入不出网）；
//! - [`EmbedFromAiProvider`]：把 `AIProvider`（云 fallback 链）适配成
//!   EmbedProvider——DK-10 门禁在 AIProvider 内部已生效（Deny 工作区出网请求
//!   离机前被拒，`PermissionDenied` + 零 HTTP），适配零额外逻辑。

use std::sync::Arc;
use std::time::Duration;

use aurora_core::l2_engines::vector_search::EmbedProvider;
use aurora_core::Error;

use crate::cloud::OpenAiCompatProvider;

/// 本地 Ollama 嵌入（`POST /api/embeddings`，模型默认 `nomic-embed-text` 768 维）。
pub struct OllamaEmbedProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
    dim: u32,
}

impl OllamaEmbedProvider {
    /// 构造：base_url 一般为 `http://localhost:11434`（本机推理不出网）。
    pub fn new(base_url: &str, model: &str, dim: u32) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            dim,
        }
    }
}

#[async_trait::async_trait]
impl EmbedProvider for OllamaEmbedProvider {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, Error> {
        // Ollama /api/embeddings 单 prompt 逐条（与 OllamaProvider.embed 同口径）
        let mut out = Vec::with_capacity(texts.len());
        for text in texts {
            let resp = self
                .client
                .post(format!("{}/api/embeddings", self.base_url))
                .json(&serde_json::json!({ "model": self.model, "prompt": text }))
                .send()
                .await
                .map_err(|e| Error::AiInference(format!("ollama embed http: {e}")))?;
            if !resp.status().is_success() {
                return Err(Error::AiInference(format!(
                    "ollama embed status: {}",
                    resp.status()
                )));
            }
            let body: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| Error::AiInference(format!("ollama embed body: {e}")))?;
            let arr = body
                .get("embedding")
                .and_then(|v| v.as_array())
                .ok_or_else(|| Error::AiInference("embedding field missing".into()))?;
            let vec_f: Vec<f32> = arr
                .iter()
                .filter_map(|x| x.as_f64().map(|f| f as f32))
                .collect();
            // 维度守卫：模型返回维度与配置不符即报错（淘汰重建逻辑依赖一致口径）
            if vec_f.len() != self.dim as usize {
                return Err(Error::InvalidInput(format!(
                    "ollama embed dim mismatch: got {}, expected {} (model {})",
                    vec_f.len(),
                    self.dim,
                    self.model
                )));
            }
            out.push(vec_f);
        }
        Ok(out)
    }

    fn model(&self) -> &str {
        &self.model
    }
}

/// 云 fallback 适配器：`AIProvider` → `EmbedProvider`。
///
/// DK-10 门禁复用：底层 provider 的 `embed` 已内嵌 policy_check（Deny 工作区
/// 四出网方法含 embed 请求前 fail-closed），适配层零额外逻辑——门禁语义原样透传。
pub struct EmbedFromAiProvider {
    inner: Arc<dyn aurora_core::traits::ai_provider::AIProvider>,
    model: String,
}

impl EmbedFromAiProvider {
    /// `dim` 参数为调用方口径文档化（维度守卫实际在 VectorIndex 侧——
    /// VecRecord.dim 与缓存判定不匹配即淘汰重建）。
    pub fn new(
        inner: Arc<dyn aurora_core::traits::ai_provider::AIProvider>,
        model: &str,
        dim: u32,
    ) -> Self {
        let _ = dim;
        Self {
            inner,
            model: model.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl EmbedProvider for EmbedFromAiProvider {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, Error> {
        self.inner.embed(texts).await
    }

    fn model(&self) -> &str {
        &self.model
    }
}

/// 便捷构造：gated 云 provider → EmbedProvider（测试与装配共用口径）。
pub fn gated_cloud_embed(
    cloud: OpenAiCompatProvider,
    model: &str,
    dim: u32,
) -> EmbedFromAiProvider {
    EmbedFromAiProvider::new(Arc::new(cloud), model, dim)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::OpenAiCompatProvider;
    use crate::policy::{PolicyGate, WorkspaceConfigResolver};
    use std::sync::Arc;

    /// DK-10 门禁复用断言：Deny 工作区 embed 经适配器仍 fail-closed 零 HTTP
    /// （mockito 挂载——请求若发出必然命中）。
    #[tokio::test]
    async fn deny_workspace_embed_zero_http() {
        let m = mockito::Server::new_async().await;
        let resolver = Arc::new(WorkspaceConfigResolver::new());
        resolver.set_policy("ws-deny", crate::policy::WorkspacePolicy::DenyCloud);

        let cloud = OpenAiCompatProvider::new(m.url(), "test-key", "test-model");
        cloud.set_workspace(Some("ws-deny".into()));
        let gate = PolicyGate::new_arc(resolver);
        let cloud = cloud.with_policy_check(Arc::new(move |ws| gate.guard(ws)));

        let adapter = EmbedFromAiProvider::new(Arc::new(cloud), "test-model", 8);
        let err = adapter.embed(&["hello"]).await.expect_err("Deny 必拒");
        assert!(matches!(err, aurora_core::Error::PermissionDenied(_)));
        // mockito 无 mock 挂载即零交互——到达此处即证明零 HTTP
    }

    /// Ollama 本地主路径：mockito 模拟 /api/embeddings → 解码正确 + 维度守卫。
    #[tokio::test]
    async fn ollama_embed_round_trip() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/api/embeddings")
            .with_status(200)
            .with_body(r#"{"embedding":[0.1,0.2,0.3,0.4]}"#)
            .create_async()
            .await;
        let p = OllamaEmbedProvider::new(&server.url(), "nomic-embed-text", 4);
        let out = p.embed(&["hello"]).await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), 4);
        assert!((out[0][0] - 0.1).abs() < 1e-6);
        assert_eq!(p.model(), "nomic-embed-text");
    }

    /// 维度守卫：模型返回维度与配置不符即报错。
    #[tokio::test]
    async fn ollama_embed_dim_guard() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/api/embeddings")
            .with_status(200)
            .with_body(r#"{"embedding":[0.1,0.2]}"#)
            .create_async()
            .await;
        let p = OllamaEmbedProvider::new(&server.url(), "nomic-embed-text", 4);
        let err = p.embed(&["hello"]).await.expect_err("维度不符必拒");
        assert!(matches!(err, aurora_core::Error::InvalidInput(_)));
    }
}
