//! OllamaProvider 集成测试 (单元测试 + `#[ignore]` 联调)。
//!
//! 设计目标（对应 Plan §5 测试矩阵）：
//! - 不打外网；mockito 启动临时 HTTP server。
//! - 验证 URL 拼装、请求体字段、响应解析、降级路由。
//! - 真实联调测试带 `#[ignore]`，CI 默认不跑；本地起 Ollama 后
//!   `cargo test -- --ignored ollama_live` 才会触发。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use aurora_ai::OllamaProvider;
use aurora_core::l3_domain::ai_system::MockAIProvider;
use aurora_core::traits::ai_provider::{AIProvider, ChatOptions, CompletionOptions, Message, Tool};

// ---------------------------------------------------------------------------
// 1. URL 拼装 (pure helper，不打网)
// ---------------------------------------------------------------------------

#[test]
fn test_ollama_url_joining() {
    let provider = OllamaProvider::new("http://localhost:11434/", "llama3.2");
    assert_eq!(
        provider.url("/api/generate"),
        "http://localhost:11434/api/generate"
    );
    assert_eq!(provider.url("/api/chat"), "http://localhost:11434/api/chat");
    assert_eq!(
        provider.url("/api/embeddings"),
        "http://localhost:11434/api/embeddings"
    );
    assert_eq!(provider.url("/api/tags"), "http://localhost:11434/api/tags");

    // trailing-slash normalization: only one '/' between base and path.
    let p2 = OllamaProvider::new("http://example.com", "m");
    assert_eq!(p2.url("/api/generate"), "http://example.com/api/generate");
}

// ---------------------------------------------------------------------------
// 2. 请求体字段形状 (mockito stub)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ollama_request_body_shape_for_complete() {
    let mut server = mockito::Server::new_async().await;
    let base_url = server.url();

    // 预期请求体包含 Ollama `/api/generate` 关键字段：model, prompt, stream:false,
    // options:{temperature, num_predict, stop}
    let m = server
        .mock("POST", "/api/generate")
        .match_body(mockito::Matcher::Json(serde_json::json!({
            "model": "llama3.2",
            "prompt": "hello",
            "stream": false,
            "options": {
                "temperature": 0.7,
                "num_predict": 16,
                "stop": ["\n"]
            }
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"response":"hi"}"#)
        .create();

    let provider = OllamaProvider::new(base_url, "llama3.2");
    provider.set_available_for_test(true);

    let opts = CompletionOptions {
        max_tokens: Some(16),
        temperature: Some(0.7),
        top_p: None,
        stop: Some(vec!["\n".to_string()]),
    };
    let result = provider.complete("hello", &opts).await.unwrap();
    assert_eq!(result, "hi");
    m.assert();
}

#[tokio::test]
async fn test_ollama_request_body_shape_for_chat() {
    let mut server = mockito::Server::new_async().await;
    let base_url = server.url();

    let m = server
        .mock("POST", "/api/chat")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({
            "model": "llama3.2",
            "messages": [{"role": "user", "content": "ping"}]
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"message":{"role":"assistant","content":"pong"}}"#)
        .create();

    let provider = OllamaProvider::new(base_url, "llama3.2");
    provider.set_available_for_test(true);

    let opts = ChatOptions {
        max_tokens: None,
        temperature: None,
    };
    let result = provider
        .chat(
            &[Message {
                role: "user".into(),
                content: "ping".into(),
            }],
            &opts,
        )
        .await
        .unwrap();
    assert_eq!(result, "pong");
    m.assert();
}

// ---------------------------------------------------------------------------
// 3. 响应解析 (embed / function_call)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ollama_response_parse_embed() {
    let mut server = mockito::Server::new_async().await;
    let base_url = server.url();

    // 两条 prompt 顺序发出请求，因此 stub 出两个不同响应（mockito 的
    // mock-顺序即定义顺序，所以第一条请求 = 第一个 stub）。使用 Regex
    // + 简单子串模式（无正则元字符）做 body 区分。
    let m1 = server
        .mock("POST", "/api/embeddings")
        .match_body(mockito::Matcher::Regex(r#""prompt":"a""#.to_string()))
        .with_status(200)
        .with_body(r#"{"embedding":[0.1,0.2,0.3]}"#)
        .create();
    let m2 = server
        .mock("POST", "/api/embeddings")
        .match_body(mockito::Matcher::Regex(r#""prompt":"b""#.to_string()))
        .with_status(200)
        .with_body(r#"{"embedding":[0.4,0.5]}"#)
        .create();

    let provider = OllamaProvider::new(base_url, "llama3.2");
    provider.set_available_for_test(true);

    let v = provider.embed(&["a", "b"]).await.unwrap();
    assert_eq!(v.len(), 2);
    assert_eq!(v[0], vec![0.1, 0.2, 0.3]);
    assert_eq!(v[1], vec![0.4, 0.5]);
    m1.assert();
    m2.assert();
}

#[tokio::test]
async fn test_ollama_response_parse_function_call() {
    let mut server = mockito::Server::new_async().await;
    let base_url = server.url();

    let m = server
        .mock("POST", "/api/chat")
        .with_status(200)
        .with_body(
            r#"{"message":{"role":"assistant","content":""},
               "tool_calls":[{"function":{"name":"search","arguments":{"q":"rust"}}}]}"#,
        )
        .create();

    let provider = OllamaProvider::new(base_url, "llama3.2");
    provider.set_available_for_test(true);

    let tools = vec![Tool {
        name: "search".into(),
        description: "Search the docs.".into(),
        parameters: serde_json::json!({"type":"object"}),
    }];
    let call = provider
        .function_call("find rust stuff", &tools)
        .await
        .unwrap();
    assert_eq!(call.tool_name, "search");
    assert_eq!(call.arguments, serde_json::json!({"q":"rust"}));
    m.assert();
}

// ---------------------------------------------------------------------------
// 4. 降级路由：本地不可达时转给 fallback
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fallback_on_unavailable() {
    // 本地 provider 指向一个不可达端口，available=false 时所有方法应转给 fallback。
    let mut mock = MockAIProvider::new();
    mock = mock.with_response("fallback marker", "called fallback");
    mock.set_available(true);
    let fallback: Arc<dyn AIProvider> = Arc::new(mock);

    let provider =
        OllamaProvider::new_with_fallback("http://127.0.0.1:1", "llama3.2", Some(fallback.clone()));
    provider.set_available_for_test(false);

    let opts = CompletionOptions {
        max_tokens: None,
        temperature: None,
        top_p: None,
        stop: None,
    };
    // MockAIProvider 的 find_response 在没命中关键词时返回 "Mock AI response"。
    let result = provider
        .complete("anything fallback marker", &opts)
        .await
        .unwrap();
    assert_eq!(result, "called fallback");
}

#[tokio::test]
async fn test_no_fallback_returns_ainference_error() {
    // 无 fallback 且本地不可达 → 返回 Error::AiInference
    let provider = OllamaProvider::new("http://127.0.0.1:1", "llama3.2");
    provider.set_available_for_test(false);

    let opts = CompletionOptions {
        max_tokens: None,
        temperature: None,
        top_p: None,
        stop: None,
    };
    let err = provider.complete("any", &opts).await.unwrap_err();
    assert!(matches!(err, aurora_core::Error::AiInference(_)));
}

// ---------------------------------------------------------------------------
// 5. stream_complete 同步回调（不走网络，仅断言 callback 形状/不 panic）
// ---------------------------------------------------------------------------

#[test]
fn test_stream_complete_outside_runtime_does_not_panic() {
    let provider = OllamaProvider::new("http://127.0.0.1:1", "llama3.2");
    provider.set_available_for_test(false);
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();
    provider.stream_complete(
        "anything",
        &CompletionOptions {
            max_tokens: None,
            temperature: None,
            top_p: None,
            stop: None,
        },
        Box::new(move |_chunk| {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        }),
    );
    // 无 runtime 时退化路径不应调用 callback，也不应 panic
    assert_eq!(counter.load(Ordering::Relaxed), 0);
}

// ---------------------------------------------------------------------------
// 真实联调 (默认 ignored)
// ---------------------------------------------------------------------------

/// 仅在本机起 Ollama 且 `ollama pull llama3.2` 后手动 `cargo test -- --ignored ollama_live` 运行。
#[tokio::test]
#[ignore = "requires a running Ollama on http://localhost:11434 with llama3.2 pulled"]
async fn ollama_live_smoke() {
    let provider = OllamaProvider::new("http://localhost:11434", "llama3.2");
    provider.start_probing();
    // 假设本地 Ollama 在跑，最多 30s 应已探测到可用
    for _ in 0..30 {
        if provider.is_available() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    assert!(
        provider.is_available(),
        "Ollama not running on http://localhost:11434"
    );

    let opts = CompletionOptions {
        max_tokens: Some(8),
        temperature: None,
        top_p: None,
        stop: None,
    };
    let out = provider
        .complete("Say the word hello.", &opts)
        .await
        .unwrap();
    assert!(!out.is_empty());
}
