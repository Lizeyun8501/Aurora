//! OllamaProvider —— 通过 Ollama HTTP API (`http://localhost:11434`) 接入本地 AI。
//!
//! 对应 V19 §7.2「功能依赖（可选懒加载）」分类中的「本地 AI」层。
//! 与 AIProvider Trait 适配后即可由 [`crate::OllamaProvider`] 直接注入
//! [`aurora_core::app_core::AppCore::ai`] 字段，无需修改其它 Trait。
//!
//! # 实现要点
//! - 6 个 Trait 方法全部实现；`is_available`/`stream_complete` 保持同步签名
//!   （Trait 要求）。
//! - 本地 Ollama 不可达时（`available=false`），所有方法自动转给可选的
//!   cloud fallback（`Arc<dyn AIProvider>`）；fallback 也为空时返回
//!   `Error::AiInference(\"Ollama not running and no cloud provider configured\")`。
//! - 可用性探测：构造时启动后台 `tokio::spawn`，初始探测 + 每 30s 周期
//!   探测 GET `/api/tags`（1s 超时）。无 tokio runtime 在跑时跳过探测，
//!   仅依赖 `is_available()` 的瞬时读，由路由层决定是否降级。
//! - 错误统一映射为 [`aurora_core::Error::AiInference`]，`is_retryable` 与
//!   `requires_fallback` 均为 `true`（V19 §33.2 降级矩阵），让上层路由
//!   把本地不可用场景降级到云端 Provider。
//!
//! # 端点对齐
//! - `/api/embeddings` —— embed
//! - `/api/generate`   —— complete / stream_complete
//! - `/api/chat`       —— chat / function_call
//! - `/api/tags`       —— 可用性探测
//!
//! # 模型下载
//! `is_available()` 只确认 Ollama 进程 reachable。模型缺失时第一次实际推理
//! 会返回 404，这里也走 `Error::AiInference`——用户应先 `ollama pull <model>`
//! 再启用功能（V19 §7.2 lazy-load 模式）。后续 PR 可把这一步提示化到 UI。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use aurora_core::traits::ai_provider::{
    AIProvider, ChatOptions, CompletionOptions, Message, Tool, ToolCall,
};

/// 本地 Ollama 探测间隔。
const PROBE_INTERVAL: Duration = Duration::from_secs(30);
/// 本地 Ollama 探测超时。
const PROBE_TIMEOUT: Duration = Duration::from_secs(1);
/// 默认推理调用超时。
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
/// 不可用且无 fallback 时统一返回的 Err 文案。
const NO_PROVIDER_MSG: &str = "Ollama not running and no cloud provider configured";

/// 通过 Ollama HTTP API 实现的本地 AIProvider。
///
/// 不可用时把所有方法转给可选的 `fallback`（云端 Provider）。
pub struct OllamaProvider {
    base_url: String,
    model: String,
    client: reqwest::Client,
    available: Arc<AtomicBool>,
    fallback: Option<Arc<dyn AIProvider>>,
}

// ----- Ollama HTTP 请求/响应 DTOs（不对外暴露） -----

/// 单条 Ollama `/api/embeddings` 请求体。Owned 字段以便 future `'static`，
/// 可以独立 spawn 进并发 collect；不存在借用 dangling 风险。
#[derive(Serialize)]
struct EmbeddingsReq {
    model: String,
    prompt: String,
}

#[derive(Deserialize)]
struct EmbeddingsResp {
    embedding: Vec<f32>,
}

#[derive(Serialize)]
struct GenerateReq<'a> {
    model: &'a str,
    prompt: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<GenerateOptions<'a>>,
}

#[derive(Serialize)]
struct GenerateOptions<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "num_predict")]
    num_predict: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<&'a [String]>,
}

#[derive(Deserialize)]
struct GenerateResp {
    response: String,
}

#[derive(Serialize)]
struct ChatReq<'a> {
    model: &'a str,
    messages: Vec<ChatMsg<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDef<'a>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<ChatOptionsBody>,
}

#[derive(Serialize)]
struct ChatMsg<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ToolDef<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a serde_json::Value,
}

#[derive(Serialize)]
struct ChatOptionsBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "num_predict")]
    num_predict: Option<u32>,
}

#[derive(Deserialize)]
struct ChatResp {
    message: Option<ChatMsgResp>,
    tool_calls: Option<Vec<ToolCallResp>>,
}

#[derive(Deserialize)]
struct ChatMsgResp {
    content: String,
}

#[derive(Deserialize)]
struct ToolCallResp {
    function: ToolCallFunc,
}

#[derive(Deserialize)]
struct ToolCallFunc {
    name: String,
    arguments: serde_json::Value,
}

// `// /api/tags` 探测仅检查 HTTP 200 状态，不反序列化 body ——
// 因此这里没有 `TagsResp` 类型。如果后续需要列出本机已安装的模型，
// 可以新增一个 `TagsResp { models: Vec<TagModel> }`。

impl OllamaProvider {
    /// 构造一个无 fallback 的 Ollama provider。
    ///
    /// 若当前 tokio runtime 可用，会 `tokio::spawn` 后台周期探测
    /// GET `<base_url>/api/tags` 来维护 `is_available()` 状态；否则
    /// `available=false`，需由调用方通过 fallback 处理。
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new_with_fallback(base_url, model, None)
    }

    /// 构造 Ollama provider + 可选云端 fallback。
    ///
    /// 构造时不启动后台探测——这是为了避免：
    /// (a) 在没有 tokio runtime 的语境（如某些 DI 容器初始化阶段）构造时 spawn 失败；
    /// (b) 在 #[tokio::test] 中构造时，探测 task 会被 spawn 但测试结束后
    ///     runtime 被 drop，探测 task 在长轮询里被中断引发 Windows
    ///     `STATUS_STACK_BUFFER_OVERRUN`。
    /// 正经后的生产路径应在 AppCore 启动完毕后调用 [`Self::start_probing`]
    /// 来驱动 `is_available()` 的周期更新。
    pub fn new_with_fallback(
        base_url: impl Into<String>,
        model: impl Into<String>,
        fallback: Option<Arc<dyn AIProvider>>,
    ) -> Self {
        let base_url = base_url.into();
        let model = model.into();
        let available = Arc::new(AtomicBool::new(false));
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            base_url,
            model,
            client,
            available,
            fallback,
        }
    }

    /// 启动 `is_available()` 的后台周期探测。
    ///
    /// - 在 AppCore 启动后调用一次即可——探测 GET `<base_url>/api/tags`，1s 超时，
    ///   每 30s 一次。失败保持 `available=false`，调用方可凭此走 fallback 路径。
    /// - 仅在已有 tokio runtime 上调用；在 shutdown 时探测 task 会随 runtime
    ///   关闭自然结束。
    /// - 多次调用是安全的（每次都会 spawn 一组新探测任务，开销极小）。
    pub fn start_probing(&self) {
        let probe_client = self.client.clone();
        let probe_url = format!("{}/api/tags", self.base_url.trim_end_matches('/'));
        let probe_available = self.available.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                probe_once(&probe_client, &probe_url, &probe_available).await;
                let mut ticker = tokio::time::interval_at(
                    tokio::time::Instant::now() + PROBE_INTERVAL,
                    PROBE_INTERVAL,
                );
                loop {
                    ticker.tick().await;
                    probe_once(&probe_client, &probe_url, &probe_available).await;
                }
            });
        } else {
            debug!(
                "OllamaProvider::start_probing called outside a tokio runtime; \
                 availability probe disabled, fallback path assumes unavailable"
            );
        }
    }

    /// URL 拼装 helper，也供测试直接验证 base+path 拼接。
    #[doc(hidden)]
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url.trim_end_matches('/'), path)
    }

    /// 显式覆盖 `available` 标志。
    ///
    /// 生产代码不应调用（探测循环已经在线维护 `available`）。仅供集成测试
    /// 在不打 Ollama 的情况下构造「本地 unreachable」场景，验证 fallback
    /// 路径走通。`#[doc(hidden)]` 是为了让 `cargo doc` 不暴露给用户。
    #[doc(hidden)]
    pub fn set_available_for_test(&self, value: bool) {
        self.available.store(value, Ordering::Relaxed);
    }

    /// 把一次 reqwest 错误/HTTP 4xx-5xx 映射为统一的 `Error::AiInference`。
    fn err(label: &'static str, e: impl std::fmt::Display) -> aurora_core::Error {
        aurora_core::Error::AiInference(format!("{}: {}", label, e))
    }

    /// local 不可达且无 fallback 的统一错误。
    fn no_provider() -> aurora_core::Error {
        aurora_core::Error::AiInference(NO_PROVIDER_MSG.to_string())
    }
}

async fn probe_once(client: &reqwest::Client, url: &str, flag: &AtomicBool) {
    let ok = client
        .get(url)
        .timeout(PROBE_TIMEOUT)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    flag.store(ok, Ordering::Relaxed);
}

#[async_trait]
impl AIProvider for OllamaProvider {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, aurora_core::Error> {
        if !self.available.load(Ordering::Relaxed) {
            return match &self.fallback {
                Some(fb) => fb.embed(texts).await,
                None => Err(Self::no_provider()),
            };
        }
        // 顺序逐条发请求：Ollama `/api/embeddings` 一次只接受一个 prompt，
        // 多条 prompt 顺序处理即可，避免并行 collect 在 current-thread
        // runtime 上抢 hyper IO 资源导致的栈溢出/UB（之前 #[tokio::test]
        // 中实测 0xc0000409 STATUS_STACK_BUFFER_OVERRUN）。
        // req 使用 owned 字段，避免 async move 中借用 dangling 引发的 UB。
        let mut results: Vec<Vec<f32>> = Vec::with_capacity(texts.len());
        let url = self.url("/api/embeddings");
        for text in texts {
            let req = EmbeddingsReq {
                model: self.model.clone(),
                prompt: (*text).to_string(),
            };
            let resp = self
                .client
                .post(&url)
                .json(&req)
                .send()
                .await
                .map_err(|e| Self::err("ollama embed http", e))?;
            if !resp.status().is_success() {
                return Err(Self::err(
                    "ollama embed status",
                    format!("{} {}", resp.status(), resp.url()),
                ));
            }
            let body: EmbeddingsResp = resp
                .json()
                .await
                .map_err(|e| Self::err("ollama embed decode", e))?;
            results.push(body.embedding);
        }
        Ok(results)
    }

    async fn complete(
        &self,
        prompt: &str,
        opts: &CompletionOptions,
    ) -> Result<String, aurora_core::Error> {
        if !self.available.load(Ordering::Relaxed) {
            return match &self.fallback {
                Some(fb) => fb.complete(prompt, opts).await,
                None => Err(Self::no_provider()),
            };
        }
        let req = GenerateReq {
            model: &self.model,
            prompt,
            stream: Some(false),
            options: Some(GenerateOptions {
                temperature: opts.temperature,
                top_p: opts.top_p,
                num_predict: opts.max_tokens,
                stop: opts.stop.as_deref(),
            }),
        };
        let resp = self
            .client
            .post(self.url("/api/generate"))
            .json(&req)
            .send()
            .await
            .map_err(|e| Self::err("ollama complete http", e))?;
        if !resp.status().is_success() {
            return Err(Self::err(
                "ollama complete status",
                format!("{} {}", resp.status(), resp.url()),
            ));
        }
        let body: GenerateResp = resp
            .json()
            .await
            .map_err(|e| Self::err("ollama complete decode", e))?;
        Ok(body.response)
    }

    fn stream_complete(
        &self,
        prompt: &str,
        opts: &CompletionOptions,
        callback: Box<dyn Fn(String) + Send + Sync>,
    ) {
        // 同步签名下做 async HTTP：在当前 tokio runtime 上 block_on 流式调用。
        // 没有 runtime 时（Handle::try_current 失败）退化为 `complete()` 一次性
        // + 按空格切分（与 MockAIProvider 一致），避免在 trait 同步签名里 panic。
        if !self.available.load(Ordering::Relaxed) {
            match &self.fallback {
                Some(fb) => return fb.stream_complete(prompt, opts, callback),
                None => {
                    warn!(
                        "OllamaProvider::stream_complete unavailable and no fallback; \
                         firing nothing"
                    );
                    return;
                }
            }
        }
        // 把 stream 所需的 owned 数据搬到 future 里（不依赖 self 生命周期）。
        let owned = StreamCtx {
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            client: self.client.clone(),
            prompt: prompt.to_string(),
            opts: opts.clone(),
        };
        // 用 block_in_place 的等价：当前 handle 上 block_on。若调用线程本身已经
        // 在 runtime 上跑（async task），block_on 会 panic——这是 trait 同步
        // 签名的固有约束，使用方在该入口避免在 async worker 上直接调用。
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let fut = stream_ndjson(owned, callback);
                handle.block_on(fut);
            }
            Err(_) => {
                debug!(
                    "stream_complete called outside runtime; \
                     degrading to complete()+split"
                );
                // 无 runtime 语境：创建临时 runtime 驱动完整推理后按空格切分
                // 回放（与 MockAIProvider 的降级行为对齐），避免静默无输出。
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        warn!("stream_complete cannot create temp runtime: {}", e);
                        return;
                    }
                };
                match rt.block_on(self.complete(prompt, opts)) {
                    Ok(text) => {
                        for part in text.split(' ') {
                            callback(part.to_string());
                        }
                    }
                    Err(e) => {
                        warn!("stream_complete degraded complete() failed: {}", e);
                    }
                }
            }
        }
    }

    async fn chat(
        &self,
        messages: &[Message],
        opts: &ChatOptions,
    ) -> Result<String, aurora_core::Error> {
        if !self.available.load(Ordering::Relaxed) {
            return match &self.fallback {
                Some(fb) => fb.chat(messages, opts).await,
                None => Err(Self::no_provider()),
            };
        }
        // trait Message → Ollama 期望的 ChatMsg。role/content lifespan 不同，先
        // 把它们抓成已拥有的 (String, String)，再借给 ChatMsg。
        let msgs: Vec<(String, String)> = messages
            .iter()
            .map(|m| (m.role.clone(), m.content.clone()))
            .collect();
        let ollama_msgs: Vec<ChatMsg> = msgs
            .iter()
            .map(|(r, c)| ChatMsg {
                role: r,
                content: c,
            })
            .collect();
        let req = ChatReq {
            model: &self.model,
            messages: ollama_msgs,
            stream: Some(false),
            tools: None,
            options: Some(ChatOptionsBody {
                temperature: opts.temperature,
                num_predict: opts.max_tokens,
            }),
        };
        let resp = self
            .client
            .post(self.url("/api/chat"))
            .json(&req)
            .send()
            .await
            .map_err(|e| Self::err("ollama chat http", e))?;
        if !resp.status().is_success() {
            return Err(Self::err(
                "ollama chat status",
                format!("{} {}", resp.status(), resp.url()),
            ));
        }
        let body: ChatResp = resp
            .json()
            .await
            .map_err(|e| Self::err("ollama chat decode", e))?;
        body.message
            .map(|m| m.content)
            .ok_or_else(|| aurora_core::Error::AiInference("ollama chat: empty message".into()))
    }

    async fn function_call(
        &self,
        prompt: &str,
        tools: &[Tool],
    ) -> Result<ToolCall, aurora_core::Error> {
        if !self.available.load(Ordering::Relaxed) {
            return match &self.fallback {
                Some(fb) => fb.function_call(prompt, tools).await,
                None => Err(Self::no_provider()),
            };
        }
        // Ollama 0.4+ 支持 tool calling；不支持/未拉模型时响应里没有 `tool_calls`，
        // 这里返回 `Error::AiInference`（上层路由识别为需要降级）。
        let sys = ChatMsg {
            role: "system",
            content: "Use the provided tools when applicable.",
        };
        let user = ChatMsg {
            role: "user",
            content: prompt,
        };
        let tool_defs: Vec<ToolDef> = tools
            .iter()
            .map(|t| ToolDef {
                name: &t.name,
                description: &t.description,
                parameters: &t.parameters,
            })
            .collect();
        let req = ChatReq {
            model: &self.model,
            messages: vec![sys, user],
            stream: Some(false),
            tools: if tool_defs.is_empty() {
                None
            } else {
                Some(tool_defs)
            },
            options: None,
        };
        let resp = self
            .client
            .post(self.url("/api/chat"))
            .json(&req)
            .send()
            .await
            .map_err(|e| Self::err("ollama function_call http", e))?;
        if !resp.status().is_success() {
            return Err(Self::err(
                "ollama function_call status",
                format!("{} {}", resp.status(), resp.url()),
            ));
        }
        let body: ChatResp = resp
            .json()
            .await
            .map_err(|e| Self::err("ollama function_call decode", e))?;
        let call = body
            .tool_calls
            .and_then(|cs| cs.into_iter().next())
            .ok_or_else(|| {
                aurora_core::Error::AiInference(
                    "ollama function_call: no tool_calls in response; \
                     model may not support tools"
                        .into(),
                )
            })?;
        Ok(ToolCall {
            tool_name: call.function.name,
            arguments: call.function.arguments,
        })
    }

    fn is_available(&self) -> bool {
        self.available.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// stream_complete 的 NDJSON 流式 helper
// ---------------------------------------------------------------------------

/// stream_complete 自包含 future 所需的全部 owned 数据。
struct StreamCtx {
    base_url: String,
    model: String,
    client: reqwest::Client,
    prompt: String,
    opts: CompletionOptions,
}

async fn stream_ndjson(ctx: StreamCtx, callback: Box<dyn Fn(String) + Send + Sync>) {
    let url = format!("{}/api/generate", ctx.base_url.trim_end_matches('/'));
    let req = GenerateReq {
        model: &ctx.model,
        prompt: &ctx.prompt,
        stream: Some(true),
        options: Some(GenerateOptions {
            temperature: ctx.opts.temperature,
            top_p: ctx.opts.top_p,
            num_predict: ctx.opts.max_tokens,
            stop: ctx.opts.stop.as_deref(),
        }),
    };
    let resp = match ctx.client.post(&url).json(&req).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            warn!("ollama stream status {}: {}", r.status(), r.url());
            return;
        }
        Err(e) => {
            warn!("ollama stream http err: {}", e);
            return;
        }
    };
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk_res) = stream.next().await {
        let chunk: Bytes = match chunk_res {
            Ok(c) => c,
            Err(e) => {
                warn!("ollama stream read err: {}", e);
                break;
            }
        };
        buf.extend_from_slice(&chunk);
        // 按行切：每条完整 newline-delimited JSON 一行；末段未换行的留到下次。
        let mut start = 0usize;
        for (i, &b) in buf.iter().enumerate() {
            if b == b'\n' {
                emit_ndjson_line(&buf[start..i], &callback);
                start = i + 1;
            }
        }
        buf.drain(0..start);
    }
    if !buf.is_empty() {
        emit_ndjson_line(&buf, &callback);
    }
}

fn emit_ndjson_line(line: &[u8], callback: &(dyn Fn(String) + Send + Sync)) {
    let line_str = match std::str::from_utf8(line) {
        Ok(s) => s.trim(),
        Err(_) => return,
    };
    if line_str.is_empty() {
        return;
    }
    let parsed: serde_json::Value = match serde_json::from_str(line_str) {
        Ok(v) => v,
        Err(_) => return,
    };
    if let Some(token) = parsed.get("response").and_then(|v| v.as_str()) {
        if !token.is_empty() {
            callback(token.to_string());
        }
    }
}
