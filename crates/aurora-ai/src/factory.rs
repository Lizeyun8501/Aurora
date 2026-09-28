//! DK-10 切片 2：AI 装配闭环 — 生产装配入口（策略接线落地到大门）。
//!
//! 切片 1 的策略门禁是「造好了闸机」：[`crate::cloud::OpenAiCompatProvider`]
//! 内嵌 `policy_check`，四出网方法（embed/complete/chat/function_call）请求前
//! fail-closed。本切片把装配补齐：主链 [`crate::ollama::OllamaProvider`]（本地
//! 推理）+ 云端 fallback（gated）→ [`assemble_gated_provider`] 一个入口交付
//! `Arc<dyn AIProvider>`。
//!
//! **勿双 gate**：guard 已在 provider 内部（切片 1 冻结语义），装配层只把
//! [`crate::policy::WorkspaceConfigResolver`] 适配进 fallback 的 policy_check，
//! 不再额外包裹 PolicyGate 包装器。

use std::sync::Arc;

use crate::cloud::OpenAiCompatProvider;
use crate::ollama::OllamaProvider;
use crate::policy::{PolicyGate, WorkspaceConfigResolver};
use aurora_core::traits::ai_provider::AIProvider;

/// 生产装配入口：本地 Ollama 主链 + 策略门控的云端 fallback。
///
/// - 主链：`OllamaProvider::new_with_fallback`（构造时不启动探测——生产方在
///   AppCore 启动后对具体类型调 `start_probing` 驱动可用性，见回执挂起项）；
/// - fallback：`cloud_fallback` 注入 `PolicyGate::guard`（resolver 驱动，
///   DenyCloud 工作区出网请求在离机前被拒——fail-closed，零 HTTP）；
/// - 返回 `Arc<dyn AIProvider>` 供 DI/注册表直接持有。
///
/// # 工作区上下文
///
/// fallback 的工作区上下文经 [`OpenAiCompatProvider::set_workspace`] 设定
/// （可在**传入装配前**预设，运行时切换需持具体类型——内部为 RwLock，
/// 状态随对象进入 `Arc`）。上下文为 `None` 时不启用策略检查（切片 1
/// 向后兼容语义）。
pub fn assemble_gated_provider(
    ollama_base: &str,
    model: &str,
    cloud_fallback: Option<OpenAiCompatProvider>,
    resolver: Arc<WorkspaceConfigResolver>,
) -> Arc<dyn AIProvider> {
    let fallback: Option<Arc<dyn AIProvider>> = cloud_fallback.map(|cloud| {
        let gate = PolicyGate::new_arc(resolver.clone());
        let gated = cloud.with_policy_check(Arc::new(move |ws| gate.guard(ws)));
        Arc::new(gated) as Arc<dyn AIProvider>
    });
    let ollama = OllamaProvider::new_with_fallback(ollama_base, model, fallback);
    Arc::new(ollama)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::WorkspacePolicy;
    use aurora_core::traits::ai_provider::{ChatOptions, CompletionOptions, Message, Tool};

    const CHAT_BODY: &str = r#"{"choices":[{"message":{"content":"pong"}}]}"#;
    const EMBED_BODY: &str = r#"{"data":[{"embedding":[0.1,0.2]}]}"#;

    /// DenyCloud 工作区端到端：主链不可达（未探测，available=false）→ fallback
    /// 链 → 云端策略门拒 → **四出网方法全 PermissionDenied 且 mockito 零 HTTP**。
    #[tokio::test]
    async fn deny_workspace_end_to_end_zero_http() {
        let mut server = mockito::Server::new_async().await;
        // 云端桩：guard 若失效任何请求都会命中——matched=false 即零触网证据
        let chat_m = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_body(CHAT_BODY)
            .create_async()
            .await;
        let embed_m = server
            .mock("POST", "/v1/embeddings")
            .with_status(200)
            .with_body(EMBED_BODY)
            .create_async()
            .await;

        let resolver = Arc::new(WorkspaceConfigResolver::new());
        resolver.set_policy("ws-deny", WorkspacePolicy::DenyCloud);

        // 装配前设工作区上下文（RwLock 状态随对象进入 Arc——最小通路，零 hack）
        let cloud = OpenAiCompatProvider::new(server.url(), "test-key", "test-model");
        cloud.set_workspace(Some("ws-deny".into()));
        let provider =
            assemble_gated_provider("http://127.0.0.1:1", "test-model", Some(cloud), resolver);

        let chat_err = provider
            .chat(
                &[Message {
                    role: "user".into(),
                    content: "hi".into(),
                }],
                &ChatOptions {
                    max_tokens: Some(8),
                    temperature: Some(0.1),
                },
            )
            .await
            .expect_err("Deny 工作区 chat 必须拒");
        assert!(matches!(chat_err, aurora_core::Error::PermissionDenied(_)));
        let complete_err = provider
            .complete(
                "hi",
                &CompletionOptions {
                    max_tokens: Some(8),
                    temperature: Some(0.1),
                    stop: None,
                    top_p: None,
                },
            )
            .await
            .expect_err("Deny 工作区 complete 必须拒");
        assert!(matches!(
            complete_err,
            aurora_core::Error::PermissionDenied(_)
        ));
        let embed_err = provider
            .embed(&["x"])
            .await
            .expect_err("Deny 工作区 embed 必须拒");
        assert!(matches!(embed_err, aurora_core::Error::PermissionDenied(_)));
        let fc_err = provider
            .function_call(
                "hi",
                &[Tool {
                    name: "t".into(),
                    description: "d".into(),
                    parameters: serde_json::json!({}),
                }],
            )
            .await
            .expect_err("Deny 工作区 function_call 必须拒");
        assert!(matches!(fc_err, aurora_core::Error::PermissionDenied(_)));

        // 零 HTTP 实证：云端桩零命中（mockito matched=false = 无任何请求到达）
        assert!(!chat_m.matched(), "Deny 工作区不得触网 chat/completions");
        assert!(!embed_m.matched(), "Deny 工作区不得触网 embeddings");
    }

    /// AllowCloud 端到端：主链不可达（未探测）→ fallback 可达（mockito 命中）。
    #[tokio::test]
    async fn allow_workspace_fallback_reaches_network() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_body(CHAT_BODY)
            .create_async()
            .await;
        let resolver = Arc::new(WorkspaceConfigResolver::new()); // 缺省 AllowCloud

        let cloud = OpenAiCompatProvider::new(server.url(), "test-key", "test-model");
        cloud.set_workspace(Some("ws-public".into()));
        let provider =
            assemble_gated_provider("http://127.0.0.1:1", "test-model", Some(cloud), resolver);

        let out = provider
            .chat(
                &[Message {
                    role: "user".into(),
                    content: "ping".into(),
                }],
                &ChatOptions {
                    max_tokens: Some(8),
                    temperature: Some(0.1),
                },
            )
            .await
            .expect("AllowCloud 主链不可达时应走 fallback 并成功");
        assert_eq!(out, "pong");
        assert!(m.matched(), "fallback 应真实触网");
    }

    /// Deny 上下文经 fallback 链透传：主链不可达 → fallback 的 guard 拒绝
    /// function_call（Alpha 13f0cc5 补的第四出网口在装配链上同样被覆盖）。
    #[tokio::test]
    async fn deny_workspace_function_call_blocked_via_chain() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_body(CHAT_BODY)
            .create_async()
            .await;
        let resolver = Arc::new(WorkspaceConfigResolver::new());
        resolver.set_policy("ws-deny", WorkspacePolicy::DenyCloud);

        let cloud = OpenAiCompatProvider::new(server.url(), "test-key", "test-model");
        cloud.set_workspace(Some("ws-deny".into()));
        let provider =
            assemble_gated_provider("http://127.0.0.1:1", "test-model", Some(cloud), resolver);

        let fc_err = provider
            .function_call(
                "pick a tool",
                &[Tool {
                    name: "read_note".into(),
                    description: "d".into(),
                    parameters: serde_json::json!({}),
                }],
            )
            .await
            .expect_err("Deny 工作区 function_call 必须拒");
        assert!(matches!(fc_err, aurora_core::Error::PermissionDenied(_)));
        assert!(!m.matched(), "function_call 拒绝路径零 HTTP");
    }
}
