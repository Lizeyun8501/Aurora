//! OpenAiCompatProvider —— OpenAI 兼容端点（`/v1/chat/completions` + `/v1/embeddings`）的最小实现。
//!
//! 仅用作 [`crate::OllamaProvider`] 的云端 fallback（V19 §7.2「本地不可用降级
//! 云端」策略）：构造前若 `cloud_api_key` 缺失则 `is_available()=false`，路由层
//! 据此跳过。
//!
//! # 范围
//! 本轮 NOT 一个完整 OpenAI SDK；只支持最小集：
//! - `complete` → `/v1/chat/completions` 单 turn
//! - `chat` → `/v1/chat/completions` 多 turn
//! - `embed` → `/v1/embeddings`
//! - `stream_complete` → 同 MockAIProvider 退化模式：先调 `chat` 再按空格切分
//!   （不实现真正的 SSE streaming，避免引入 eventsource 依赖）
//! - `function_call` → 调 `/v1/chat/completions` 带 `tools`，解析 `tool_calls[0]`
//! - `is_available` → `cloud_api_key.is_some() && !cloud_base_url.is_empty()`
//!
//! 后续轮次（V19 §28.3 OpenAI 云端全套）如需真实 streaming/批 embed/异步透传，
//! 再升级这一文件，或替换为更完整的 crate。

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::warn;

use aurora_core::traits::ai_provider::{
    AIProvider, ChatOptions, CompletionOptions, Message, Tool, ToolCall,
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// OpenAI 兼容云端 AIProvider（最小实现，专做 OllamaProvider 的 fallback）。
pub struct OpenAiCompatProvider {
    base_url: String,
    api_key: String,
    model: String,
    client: reqwest::Client,
    available: AtomicBool,
}

// ----- OpenAI 兼容请求/响应 DTOs（不对外暴露）-----

#[derive(Serialize)]
struct ChatCompletionsReq<'a> {
    model: &'a str,
    messages: Vec<ChatMsg<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "top_p")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "stop")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDef<'a>>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "tool_choice")]
    tool_choice: Option<String>,
}

#[derive(Serialize)]
struct ChatMsg<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ToolDef<'a> {
    #[serde(rename = "type")]
    ty: &'a str,
    function: ToolFunctionDef<'a>,
}

#[derive(Serialize)]
struct ToolFunctionDef<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a serde_json::Value,
}

#[derive(Deserialize)]
struct ChatCompletionsResp {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatChoiceMsg,
}

#[derive(Deserialize)]
struct ChatChoiceMsg {
    content: Option<String>,
    #[serde(rename = "tool_calls")]
    tool_calls: Option<Vec<ChatToolCall>>,
}

#[derive(Deserialize)]
struct ChatToolCall {
    function: ChatToolCallFunc,
}

#[derive(Deserialize)]
struct ChatToolCallFunc {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Serialize)]
struct EmbeddingsReq<'a> {
    model: &'a str,
    input: Vec<&'a str>,
}

#[derive(Deserialize)]
struct EmbeddingsResp {
    data: Vec<EmbeddingsItem>,
}

#[derive(Deserialize)]
struct EmbeddingsItem {
    embedding: Vec<f32>,
}

impl OpenAiCompatProvider {
    /// 构造；若 `api_key` 为空，`is_available()=false` 占位。
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let base_url = base_url.into();
        let api_key = api_key.into();
        let model = model.into();
        // SSRF 防护：仅允许 http/https scheme
        let lower = base_url.to_lowercase();
        if !lower.starts_with("http://") && !lower.starts_with("https://") {
            tracing::warn!(
                "cloud base_url SSRF check failed: {} — provider disabled",
                base_url
            );
            return Self {
                base_url,
                api_key: String::new(), // 清空 key 使 available=false
                model,
                client: reqwest::Client::new(),
                available: AtomicBool::new(false),
            };
        }
        let available = AtomicBool::new(!api_key.is_empty() && !base_url.trim().is_empty());
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            base_url,
            api_key,
            model,
            client,
            available,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url.trim_end_matches('/'), path)
    }

    fn err(label: &'static str, e: impl std::fmt::Display) -> aurora_core::Error {
        aurora_core::Error::AiInference(format!("{}: {}", label, e))
    }

    /// 仅供测试：显式重置 available（生产代码不应调用，路由层用
    /// `cloud_configured()` + 真实的 API key 设置）。
    #[doc(hidden)]
    pub fn set_available_for_test(&self, value: bool) {
        self.available.store(value, Ordering::Relaxed);
    }
}

#[async_trait]
impl AIProvider for OpenAiCompatProvider {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, aurora_core::Error> {
        if !self.available.load(Ordering::Relaxed) {
            return Err(aurora_core::Error::AiInference(
                "cloud provider not configured (no api_key/base_url)".into(),
            ));
        }
        let req = EmbeddingsReq {
            model: &self.model,
            input: texts.to_vec(),
        };
        let resp = self
            .client
            .post(self.url("/v1/embeddings"))
            .bearer_auth(&self.api_key)
            .json(&req)
            .send()
            .await
            .map_err(|e| Self::err("cloud embed http", e))?;
        if !resp.status().is_success() {
            return Err(Self::err(
                "cloud embed status",
                format!("{} {}", resp.status(), resp.url()),
            ));
        }
        let body: EmbeddingsResp = resp
            .json()
            .await
            .map_err(|e| Self::err("cloud embed decode", e))?;
        Ok(body.data.into_iter().map(|d| d.embedding).collect())
    }

    async fn complete(
        &self,
        prompt: &str,
        opts: &CompletionOptions,
    ) -> Result<String, aurora_core::Error> {
        if !self.available.load(Ordering::Relaxed) {
            return Err(aurora_core::Error::AiInference(
                "cloud provider not configured (no api_key/base_url)".into(),
            ));
        }
        let req = ChatCompletionsReq {
            model: &self.model,
            messages: vec![ChatMsg {
                role: "user",
                content: prompt,
            }],
            max_tokens: opts.max_tokens,
            temperature: opts.temperature,
            top_p: opts.top_p,
            stop: opts.stop.clone(),
            tools: None,
            tool_choice: None,
        };
        let resp = self
            .client
            .post(self.url("/v1/chat/completions"))
            .bearer_auth(&self.api_key)
            .json(&req)
            .send()
            .await
            .map_err(|e| Self::err("cloud complete http", e))?;
        if !resp.status().is_success() {
            return Err(Self::err(
                "cloud complete status",
                format!("{} {}", resp.status(), resp.url()),
            ));
        }
        let body: ChatCompletionsResp = resp
            .json()
            .await
            .map_err(|e| Self::err("cloud complete decode", e))?;
        let content = body
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or_else(|| {
                aurora_core::Error::AiInference("cloud complete: empty content".into())
            })?;
        Ok(content)
    }

    fn stream_complete(
        &self,
        prompt: &str,
        opts: &CompletionOptions,
        callback: Box<dyn Fn(String) + Send + Sync>,
    ) {
        if !self.available.load(Ordering::Relaxed) {
            warn!(
                "OpenAiCompatProvider::stream_complete called while not configured; \
                 firing nothing"
            );
            return;
        }
        // 简化：本最小实现不做 SSE，先 complete() 再按空格切片回调。
        // 因为 Trait 签名同步，没法 await；用 block_on 取一次结果。
        let opts_owned = opts.clone();
        let owned_prompt = prompt.to_string();
        let handle = match tokio::runtime::Handle::try_current() {
            Ok(h) => h,
            Err(_) => {
                warn!("OpenAiCompatProvider::stream_complete outside runtime; firing nothing");
                return;
            }
        };
        let text = handle.block_on(self.complete(&owned_prompt, &opts_owned));
        match text {
            Ok(t) => {
                for word in t.split_whitespace() {
                    callback(format!("{} ", word));
                }
            }
            Err(e) => warn!(
                "OpenAiCompatProvider::stream_complete underlying complete failed: {}",
                e
            ),
        }
    }

    async fn chat(
        &self,
        messages: &[Message],
        opts: &ChatOptions,
    ) -> Result<String, aurora_core::Error> {
        if !self.available.load(Ordering::Relaxed) {
            return Err(aurora_core::Error::AiInference(
                "cloud provider not configured (no api_key/base_url)".into(),
            ));
        }
        let msgs: Vec<(String, String)> = messages
            .iter()
            .map(|m| (m.role.clone(), m.content.clone()))
            .collect();
        let oai_msgs: Vec<ChatMsg> = msgs
            .iter()
            .map(|(r, c)| ChatMsg {
                role: r,
                content: c,
            })
            .collect();
        let req = ChatCompletionsReq {
            model: &self.model,
            messages: oai_msgs,
            max_tokens: opts.max_tokens,
            temperature: opts.temperature,
            top_p: None,
            stop: None,
            tools: None,
            tool_choice: None,
        };
        let resp = self
            .client
            .post(self.url("/v1/chat/completions"))
            .bearer_auth(&self.api_key)
            .json(&req)
            .send()
            .await
            .map_err(|e| Self::err("cloud chat http", e))?;
        if !resp.status().is_success() {
            return Err(Self::err(
                "cloud chat status",
                format!("{} {}", resp.status(), resp.url()),
            ));
        }
        let body: ChatCompletionsResp = resp
            .json()
            .await
            .map_err(|e| Self::err("cloud chat decode", e))?;
        let content = body
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or_else(|| aurora_core::Error::AiInference("cloud chat: empty content".into()))?;
        Ok(content)
    }

    async fn function_call(
        &self,
        prompt: &str,
        tools: &[Tool],
    ) -> Result<ToolCall, aurora_core::Error> {
        if !self.available.load(Ordering::Relaxed) {
            return Err(aurora_core::Error::AiInference(
                "cloud provider not configured (no api_key/base_url)".into(),
            ));
        }
        let tool_defs: Vec<ToolDef> = tools
            .iter()
            .map(|t| ToolDef {
                ty: "function",
                function: ToolFunctionDef {
                    name: &t.name,
                    description: &t.description,
                    parameters: &t.parameters,
                },
            })
            .collect();
        let has_tools = !tool_defs.is_empty();
        let req = ChatCompletionsReq {
            model: &self.model,
            messages: vec![ChatMsg {
                role: "user",
                content: prompt,
            }],
            max_tokens: None,
            temperature: None,
            top_p: None,
            stop: None,
            tools: if has_tools { Some(tool_defs) } else { None },
            tool_choice: if has_tools {
                Some("auto".to_string())
            } else {
                None
            },
        };
        let resp = self
            .client
            .post(self.url("/v1/chat/completions"))
            .bearer_auth(&self.api_key)
            .json(&req)
            .send()
            .await
            .map_err(|e| Self::err("cloud function_call http", e))?;
        if !resp.status().is_success() {
            return Err(Self::err(
                "cloud function_call status",
                format!("{} {}", resp.status(), resp.url()),
            ));
        }
        let body: ChatCompletionsResp = resp
            .json()
            .await
            .map_err(|e| Self::err("cloud function_call decode", e))?;
        let call = body
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.tool_calls)
            .and_then(|cs| cs.into_iter().next())
            .ok_or_else(|| {
                aurora_core::Error::AiInference(
                    "cloud function_call: no tool_calls in response".into(),
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
