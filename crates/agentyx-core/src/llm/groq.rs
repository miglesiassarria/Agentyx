//! `GroqProvider` — OpenAI-compatible cloud provider.
//!
//! Groq uses the same shape as OpenAI's `POST /chat/completions`
//! with `Authorization: Bearer {api_key}`. Streaming is SSE:
//!
//! ```text
//! data: {"id":"…","choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":null}]}\n\n
//! data: {"id":"…","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}\n\n
//! data: [DONE]\n\n
//! ```
//!
//! Tool calls come in `delta.tool_calls` with `index` and partial
//! fields; for v1 we emit one `ChatEvent::ToolUse` per
//! `tool_call` chunk (most providers send a tool call in a
//! single chunk; partial accumulation can be added in v1.x).
//!
//! See `../../../specs/domains/providers.md` for the full design.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::{Stream, StreamExt};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::llm::types::{
    ChatEvent, ChatMessage, ChatRequest, ChatStream, FinishReason, ModelCapabilities, ModelInfo,
    Usage,
};
use crate::llm::Provider;
use crate::{AppError, AppResult};

/// Default Groq endpoint.
pub const DEFAULT_BASE_URL: &str = "https://api.groq.com/openai/v1";

/// Default request timeout. Groq is fast; this is generous.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// Hardcoded model list for v1. Per `providers.md` AC: the
/// `list_models` surface returns this set; capability lookup
/// uses [`Self::capabilities`] (heuristic by id).
const MODELS: &[(&str, &str, u32, u32)] = &[
    // (id, display name, context_window, max_output_tokens)
    (
        "llama-3.3-70b-versatile",
        "Llama 3.3 70B Versatile",
        131_072,
        32_768,
    ),
    (
        "llama-3.1-8b-instant",
        "Llama 3.1 8B Instant",
        131_072,
        8_192,
    ),
    ("mixtral-8x7b-32768", "Mixtral 8x7B 32K", 32_768, 4_096),
];

/// Groq provider. OpenAI-compatible.
#[derive(Debug, Clone)]
pub struct GroqProvider {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    base_url: String,
    api_key: String,
    client: Client,
}

impl GroqProvider {
    /// Build a Groq provider with the default base URL and the
    /// given API key. The key is **never** logged.
    pub fn new(api_key: impl Into<String>) -> AppResult<Self> {
        Self::with_base_url(DEFAULT_BASE_URL, api_key)
    }

    /// Build a Groq provider with a custom base URL.
    pub fn with_base_url(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> AppResult<Self> {
        let client = Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .map_err(|e| AppError::Internal {
                message: format!("reqwest client build: {e}"),
            })?;
        Ok(Self {
            inner: Arc::new(Inner {
                base_url: base_url.into(),
                api_key: api_key.into(),
                client,
            }),
        })
    }
}

#[async_trait]
impl Provider for GroqProvider {
    fn id(&self) -> &'static str {
        "groq"
    }

    fn name(&self) -> &'static str {
        "Groq (OpenAI-compatible)"
    }

    fn capabilities(&self, model_id: &str) -> ModelCapabilities {
        if let Some((_, _, ctx, out)) = MODELS.iter().find(|(id, _, _, _)| *id == model_id) {
            ModelCapabilities {
                tools: true,
                vision: false,
                context_window: *ctx,
                max_output_tokens: *out,
            }
        } else {
            ModelCapabilities {
                tools: false,
                vision: false,
                context_window: 4096,
                max_output_tokens: 1024,
            }
        }
    }

    async fn list_models(&self) -> AppResult<Vec<ModelInfo>> {
        Ok(MODELS
            .iter()
            .map(|(id, name, ctx, out)| ModelInfo {
                id: (*id).to_string(),
                name: (*name).to_string(),
                context_window: *ctx,
                max_output_tokens: *out,
                supports_tools: true,
                supports_vision: false,
            })
            .collect())
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatStream, AppError> {
        let body = build_request_body(&req);
        let url = format!("{}/chat/completions", self.inner.base_url);

        let resp = self
            .inner
            .client
            .post(&url)
            .bearer_auth(&self.inner.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Provider {
                provider_id: "groq".into(),
                message: format!("chat request: {e}"),
                retryable: e.is_timeout() || e.is_connect() || e.is_request(),
            })?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AppError::Provider {
                provider_id: "groq".into(),
                message: "401 unauthorized: invalid API key".into(),
                retryable: false,
            });
        }
        if !status.is_success() {
            return Err(AppError::Provider {
                provider_id: "groq".into(),
                message: format!("HTTP {status}"),
                retryable: status.is_server_error(),
            });
        }

        let model = req.model.clone();
        let byte_stream = resp.bytes_stream();
        let event_stream = sse_to_chat_events(byte_stream, move |payload, started| {
            parse_openai_chunk(payload, &model, started)
        });
        Ok(Box::pin(event_stream))
    }

    async fn health(&self) -> AppResult<u64> {
        // GET /models is the cheap, auth-protected endpoint
        // that confirms both reachability and key validity.
        let url = format!("{}/models", self.inner.base_url);
        let start = Instant::now();
        let resp = self
            .inner
            .client
            .get(&url)
            .bearer_auth(&self.inner.api_key)
            .send()
            .await
            .map_err(|e| AppError::Provider {
                provider_id: "groq".into(),
                message: format!("health: {e}"),
                retryable: true,
            })?;
        if !resp.status().is_success() {
            return Err(AppError::Provider {
                provider_id: "groq".into(),
                message: format!("health: http {}", resp.status().as_u16()),
                retryable: resp.status().is_server_error(),
            });
        }
        let _ = resp.bytes().await.map_err(|e| AppError::Provider {
            provider_id: "groq".into(),
            message: format!("health: {e}"),
            retryable: false,
        })?;
        Ok(start.elapsed().as_millis() as u64)
    }
}

// ============================================================
// Request body building (OpenAI-compat)
// ============================================================

fn build_request_body(req: &ChatRequest) -> Value {
    let supports_tools = !req.tools.is_empty();
    let messages: Vec<Value> = req.messages.iter().map(openai_message).collect();
    let mut body = json!({
        "model": req.model,
        "stream": req.stream,
        "messages": messages,
    });
    if supports_tools {
        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();
        body["tools"] = json!(tools);
        body["tool_choice"] = match req.tool_choice {
            crate::llm::ToolChoice::Auto => json!("auto"),
            crate::llm::ToolChoice::Any => json!("required"),
            crate::llm::ToolChoice::None => json!("none"),
        };
    }
    if let Some(t) = req.temperature {
        body["temperature"] = json!(t);
    }
    if let Some(n) = req.max_output_tokens {
        body["max_tokens"] = json!(n);
    }
    body
}

fn openai_message(msg: &ChatMessage) -> Value {
    match msg {
        ChatMessage::System { content } => json!({"role": "system", "content": content}),
        ChatMessage::User { content } => json!({"role": "user", "content": content}),
        ChatMessage::Assistant {
            content,
            tool_calls,
        } => {
            if tool_calls.is_empty() {
                json!({"role": "assistant", "content": content})
            } else {
                let tcs: Vec<Value> = tool_calls
                    .iter()
                    .map(|tc| {
                        json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": tc.args.to_string(),
                            }
                        })
                    })
                    .collect();
                json!({"role": "assistant", "content": content, "tool_calls": tcs})
            }
        }
        ChatMessage::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => json!({
            "role": "tool",
            "tool_call_id": tool_use_id,
            "content": content,
            "is_error": is_error,
        }),
    }
}

// ============================================================
// SSE → ChatEvent stream
// ============================================================

/// Build a `Stream<Item = Result<ChatEvent, AppError>>` from a
/// byte stream and a per-chunk parser. Handles the standard SSE
/// framing: blank line separates events; lines starting with
/// `data: ` carry the payload. `data: [DONE]` ends the stream.
///
/// `parse_chunk` receives a `&mut bool` `started` flag it can
/// toggle to ensure `MessageStart` is emitted only once per
/// stream (OpenAI sends the same `id` in every chunk).
///
/// A small `Vec` queue is used to flatten the multiple events
/// each chunk can produce (MessageStart + ContentDelta + ...).
fn sse_to_chat_events<F, S>(
    byte_stream: S,
    parse_chunk: F,
) -> impl Stream<Item = Result<ChatEvent, AppError>>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Send + Unpin + 'static,
    F: Fn(&str, &mut bool) -> Vec<ChatEvent> + Send + 'static,
{
    let init = (
        String::new(),
        byte_stream,
        parse_chunk,
        false,
        Vec::<ChatEvent>::new(),
    );
    futures::stream::unfold(
        init,
        |(mut buffer, mut stream, parse_chunk, mut started, mut queue)| async move {
            loop {
                // Drain queue first.
                if let Some(ev) = queue.pop() {
                    return Some((Ok(ev), (buffer, stream, parse_chunk, started, queue)));
                }
                // Drain complete events from the buffer.
                if let Some(idx) = buffer.find("\n\n") {
                    let event: String = buffer.drain(..idx + 2).collect();
                    for line in event.lines() {
                        if let Some(payload) = line.strip_prefix("data: ") {
                            let payload = payload.trim();
                            if payload == "[DONE]" {
                                return None;
                            }
                            let mut events = parse_chunk(payload, &mut started);
                            // Yield the first event; keep the
                            // rest in the queue (preserving order).
                            if !events.is_empty() {
                                let first = events.remove(0);
                                queue.extend(events);
                                return Some((
                                    Ok(first),
                                    (buffer, stream, parse_chunk, started, queue),
                                ));
                            }
                        }
                    }
                    continue;
                }
                // Need more bytes.
                match stream.next().await {
                    Some(Ok(chunk)) => {
                        buffer.push_str(&String::from_utf8_lossy(&chunk));
                    }
                    Some(Err(e)) => {
                        return Some((
                            Err(AppError::Provider {
                                provider_id: "groq".into(),
                                message: format!("stream read: {e}"),
                                retryable: true,
                            }),
                            (buffer, stream, parse_chunk, started, queue),
                        ));
                    }
                    None => return None,
                }
            }
        },
    )
}

/// Parse one SSE data line as an OpenAI-compatible chunk and
/// produce zero or more `ChatEvent`s. The `started` flag is
/// set to `true` the first time a `MessageStart` is emitted
/// and is then used to dedupe subsequent identical `id`s
/// (OpenAI sends the same id in every chunk of one response).
fn parse_openai_chunk(payload: &str, model: &str, started: &mut bool) -> Vec<ChatEvent> {
    let mut chunk: OpenAiChunk = match serde_json::from_str(payload) {
        Ok(c) => c,
        Err(e) => {
            return vec![ChatEvent::Error {
                code: "chunk_decode_failed".into(),
                message: format!("SSE chunk decode: {e}"),
                retryable: false,
            }];
        }
    };
    let mut out = Vec::new();
    if let Some(id) = chunk.id {
        if !*started {
            *started = true;
            out.push(ChatEvent::MessageStart {
                message_id: id,
                model: model.into(),
            });
        }
    }
    for choice in chunk.choices {
        if let Some(content) = choice.delta.content {
            if !content.is_empty() {
                out.push(ChatEvent::ContentDelta { text: content });
            }
        }
        if let Some(tcs) = choice.delta.tool_calls {
            for tc in tcs {
                let args = tc
                    .function
                    .as_ref()
                    .and_then(|f| f.arguments.as_deref())
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or(Value::Null);
                out.push(ChatEvent::ToolUse {
                    id: tc.id.unwrap_or_default(),
                    name: tc
                        .function
                        .as_ref()
                        .and_then(|f| f.name.clone())
                        .unwrap_or_default(),
                    args,
                });
            }
        }
        if let Some(reason) = choice.finish_reason {
            let finish_reason = match reason.as_str() {
                "stop" | "tool_calls" => FinishReason::Stop,
                "length" => FinishReason::Length,
                "content_filter" => FinishReason::ContentFilter,
                _ => FinishReason::Stop,
            };
            let usage = chunk
                .usage
                .take()
                .map(OpenAiUsage::into_usage)
                .unwrap_or_default();
            out.push(ChatEvent::MessageEnd {
                usage,
                finish_reason,
            });
        }
    }
    out
}

#[derive(Debug, Deserialize)]
struct OpenAiChunk {
    #[serde(default)]
    id: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    choices: Vec<OpenAiChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    delta: OpenAiDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct OpenAiDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiToolCall {
    #[serde(default)]
    id: Option<String>,
    function: Option<OpenAiFunction>,
}

#[derive(Debug, Deserialize)]
struct OpenAiFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

impl OpenAiUsage {
    fn into_usage(self) -> Usage {
        Usage {
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            cache_read_tokens: None,
            cache_write_tokens: None,
        }
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn chat_request(model: &str, msgs: Vec<ChatMessage>) -> ChatRequest {
        ChatRequest {
            model: model.into(),
            messages: msgs,
            tools: vec![],
            tool_choice: crate::llm::ToolChoice::Auto,
            max_output_tokens: None,
            temperature: None,
            stream: true,
            metadata: crate::llm::RequestMetadata {
                workspace_id: crate::ids::WorkspaceId::new(),
                session_id: crate::ids::SessionId::new(),
                run_id: crate::ids::RunId::new(),
                agent_id: crate::ids::AgentId::new(),
            },
        }
    }

    #[tokio::test]
    async fn list_models_returns_hardcoded_set() {
        let p = GroqProvider::new("fake").unwrap();
        let models = p.list_models().await.unwrap();
        assert!(models.len() >= 2);
        assert!(models.iter().any(|m| m.id == "llama-3.3-70b-versatile"));
        assert!(models.iter().all(|m| m.supports_tools));
    }

    #[test]
    fn capabilities_known_model_has_tools() {
        let p = GroqProvider::new("fake").unwrap();
        let caps = p.capabilities("llama-3.1-8b-instant");
        assert!(caps.tools);
        assert_eq!(caps.context_window, 131_072);
    }

    #[test]
    fn capabilities_unknown_model_is_conservative() {
        let p = GroqProvider::new("fake").unwrap();
        let caps = p.capabilities("totally-unknown-7b");
        assert!(!caps.tools);
    }

    #[tokio::test]
    async fn health_against_wiremock_ok() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": [
                    {"id": "llama-3.3-70b-versatile", "object": "model"}
                ]
            })))
            .mount(&server)
            .await;

        let p = GroqProvider::with_base_url(server.uri(), "fake-key").unwrap();
        let latency = p.health().await.unwrap();
        assert!(latency < 5_000, "latency too high: {latency}");
    }

    #[tokio::test]
    async fn health_against_wiremock_unauthorized() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let p = GroqProvider::with_base_url(server.uri(), "bad-key").unwrap();
        let err = p.health().await.unwrap_err();
        match err {
            AppError::Provider {
                message, retryable, ..
            } => {
                assert!(!retryable);
                assert!(message.contains("401"));
            }
            other => panic!("expected Provider error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn chat_normalizes_sse_to_chat_events() {
        let server = MockServer::start().await;
        let body = "data: {\"id\":\"chat-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n\
                    data: {\"id\":\"chat-1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" there\"},\"finish_reason\":null}]}\n\n\
                    data: {\"id\":\"chat-1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2}}\n\n\
                    data: [DONE]\n\n";
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(body),
            )
            .mount(&server)
            .await;

        let p = GroqProvider::with_base_url(server.uri(), "fake").unwrap();
        let req = chat_request(
            "llama-3.1-8b-instant",
            vec![ChatMessage::User {
                content: "hello".into(),
            }],
        );
        let stream = p.chat(req).await.unwrap();
        let events: Vec<ChatEvent> = stream.map(|r| r.unwrap()).collect().await;

        assert!(
            matches!(&events[0], ChatEvent::MessageStart { message_id, .. } if message_id == "chat-1")
        );
        assert!(matches!(&events[1], ChatEvent::ContentDelta { text } if text == "Hi"));
        assert!(matches!(&events[2], ChatEvent::ContentDelta { text } if text == " there"));
        assert!(matches!(
            &events[3],
            ChatEvent::MessageEnd { usage, finish_reason }
            if usage.prompt_tokens == 3 && usage.completion_tokens == 2
                && *finish_reason == FinishReason::Stop
        ));
    }

    #[tokio::test]
    async fn chat_unauthorized_returns_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let p = GroqProvider::with_base_url(server.uri(), "bad-key").unwrap();
        let req = chat_request("llama-3.1-8b-instant", vec![]);
        // Discard the success return type (it returns a stream
        // on Ok) by using `match` on the result.
        match p.chat(req).await {
            Err(AppError::Provider {
                message, retryable, ..
            }) => {
                assert!(!retryable);
                assert!(message.contains("401"));
            }
            Ok(_) => panic!("expected error, got Ok"),
            Err(other) => panic!("expected Provider error, got {other:?}"),
        }
    }

    #[test]
    fn parse_openai_chunk_text_delta() {
        let payload = r#"{"id":"x","choices":[{"index":0,"delta":{"content":"hi"}}]}"#;
        let mut started = false;
        let events = parse_openai_chunk(payload, "test", &mut started);
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], ChatEvent::MessageStart { .. }));
        assert!(matches!(&events[1], ChatEvent::ContentDelta { text } if text == "hi"));
    }

    #[test]
    fn parse_openai_chunk_finish_stop() {
        let payload = r#"{"id":"x","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":2}}"#;
        let mut started = false;
        let events = parse_openai_chunk(payload, "test", &mut started);
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], ChatEvent::MessageStart { .. }));
        match &events[1] {
            ChatEvent::MessageEnd {
                usage,
                finish_reason,
            } => {
                assert_eq!(usage.prompt_tokens, 1);
                assert_eq!(usage.completion_tokens, 2);
                assert_eq!(*finish_reason, FinishReason::Stop);
            }
            other => panic!("expected MessageEnd, got {other:?}"),
        }
    }

    #[test]
    fn parse_openai_chunk_invalid_json_emits_error() {
        let mut started = false;
        let events = parse_openai_chunk("not json", "test", &mut started);
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], ChatEvent::Error { code, .. } if code == "chunk_decode_failed")
        );
    }

    #[test]
    fn build_request_body_omits_tools_when_empty() {
        let req = chat_request("llama-3.1-8b-instant", vec![]);
        let body = build_request_body(&req);
        // AC14: tools field is absent when the model has no tools.
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn build_request_body_includes_tools_when_present() {
        let mut req = chat_request("llama-3.1-8b-instant", vec![]);
        req.tools.push(crate::llm::ToolSchema {
            name: "read_file".into(),
            description: "Read a file".into(),
            parameters: json!({"type": "object"}),
        });
        let body = build_request_body(&req);
        let tools = body.get("tools").unwrap().as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["function"]["name"], "read_file");
        assert_eq!(body["tool_choice"], "auto");
    }
}
