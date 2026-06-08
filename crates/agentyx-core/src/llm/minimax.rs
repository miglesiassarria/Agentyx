//! `MinimaxProvider` — Anthropic-compatible cloud provider.
//!
//! Minimax follows the Anthropic Messages API:
//! `POST {base_url}/v1/messages` with bearer auth and
//! `anthropic-version: 2023-06-01`. The official base URL is
//! `https://api.minimax.io/anthropic`. Streaming is SSE with
//! typed events:
//!
//! ```text
//! event: message_start
//! data: {"type":"message_start","message":{"id":"…","model":"…",…}}
//!
//! event: content_block_start
//! data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}
//!
//! event: content_block_delta
//! data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}
//!
//! event: content_block_stop
//! data: {"type":"content_block_stop","index":0}
//!
//! event: message_delta
//! data: {"type":"message_delta","delta":{"stop_reason":"end_turn"}}
//!
//! event: message_stop
//! data: {"type":"message_stop"}
//! ```
//!
//! v1 scope: text-only blocks are emitted incrementally via
//! `ContentDelta`; tool_use blocks are accumulated per `index` and
//! emitted as `ToolUse` on `content_block_stop`. `message_stop`
//! closes the stream with `MessageEnd`. Prompt caching tokens
//! from `message_delta.usage` are preserved in `Usage`.
//!
//! See `../../../specs/domains/providers.md` for the full design.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::{Stream, StreamExt};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{debug, warn};

use crate::llm::types::{
    ChatEvent, ChatMessage, ChatRequest, ChatStream, FinishReason, ModelCapabilities, ModelInfo,
    Usage,
};
use crate::llm::Provider;
use crate::{AppError, AppResult};

/// Default Minimax Anthropic-compatible endpoint.
pub const DEFAULT_BASE_URL: &str = "https://api.minimax.io/anthropic";

/// Previous default shipped by early builds. Keep this compatible
/// so existing configs continue to work after upgrading.
const LEGACY_OPENAI_BASE_URL: &str = "https://api.minimax.io/v1";

/// Default request timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// Anthropic API version header value.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Fallback model list for v1. MiniMax exposes `GET /v1/models`;
/// this list is used only when that endpoint is unavailable.
const MODELS: &[(&str, &str, u32, u32)] = &[
    // (id, display name, context_window, max_output_tokens)
    ("MiniMax-M3", "MiniMax M3", 1_000_000, 8_192),
    ("MiniMax-M2.7", "MiniMax M2.7", 204_800, 8_192),
    (
        "MiniMax-M2.7-highspeed",
        "MiniMax M2.7 Highspeed",
        204_800,
        8_192,
    ),
    ("MiniMax-M2.5", "MiniMax M2.5", 204_800, 8_192),
    (
        "MiniMax-M2.5-highspeed",
        "MiniMax M2.5 Highspeed",
        204_800,
        8_192,
    ),
    ("MiniMax-M2.1", "MiniMax M2.1", 204_800, 8_192),
    (
        "MiniMax-M2.1-highspeed",
        "MiniMax M2.1 Highspeed",
        204_800,
        8_192,
    ),
    ("MiniMax-M2", "MiniMax M2", 204_800, 8_192),
];

/// Minimax provider. Anthropic-compatible.
#[derive(Debug, Clone)]
pub struct MinimaxProvider {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    base_url: String,
    api_key: String,
    client: Client,
}

impl MinimaxProvider {
    /// Build a Minimax provider with the default base URL and
    /// the given API key. The key is **never** logged.
    pub fn new(api_key: impl Into<String>) -> AppResult<Self> {
        Self::with_base_url(DEFAULT_BASE_URL, api_key)
    }

    /// Build a Minimax provider with a custom base URL.
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
impl Provider for MinimaxProvider {
    fn id(&self) -> &'static str {
        "minimax"
    }

    fn name(&self) -> &'static str {
        "Minimax (Anthropic-compatible)"
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
        match self.fetch_models().await {
            Ok(models) if !models.is_empty() => Ok(models),
            Ok(_) => Ok(static_models()),
            Err(AppError::Provider {
                retryable: false,
                message,
                ..
            }) if message.contains("401") || message.contains("403") => Err(AppError::Provider {
                provider_id: "minimax".into(),
                message,
                retryable: false,
            }),
            Err(e) => {
                warn!(error = %e, "minimax: list_models failed; using static fallback");
                Ok(static_models())
            }
        }
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatStream, AppError> {
        let body = build_request_body(&req);
        let url = messages_url(&self.inner.base_url);

        let resp = self
            .inner
            .client
            .post(&url)
            .bearer_auth(&self.inner.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Provider {
                provider_id: "minimax".into(),
                message: format!("chat request: {e}"),
                retryable: e.is_timeout() || e.is_connect() || e.is_request(),
            })?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AppError::Provider {
                provider_id: "minimax".into(),
                message: "401 unauthorized: invalid API key".into(),
                retryable: false,
            });
        }
        if !status.is_success() {
            return Err(AppError::Provider {
                provider_id: "minimax".into(),
                message: format!("HTTP {status}"),
                retryable: status.is_server_error(),
            });
        }
        debug!(model = %req.model, "minimax: chat started");

        let byte_stream = resp.bytes_stream();
        let event_stream = sse_to_chat_events_minimax(byte_stream);
        Ok(Box::pin(event_stream))
    }

    async fn health(&self) -> AppResult<u64> {
        // A tiny 1-token request verifies that the Anthropic
        // messages endpoint is reachable and the key is valid.
        let url = messages_url(&self.inner.base_url);
        let start = Instant::now();
        let resp = self
            .inner
            .client
            .post(&url)
            .bearer_auth(&self.inner.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .body(
                json!({
                    "model": MODELS[0].0,
                    "max_tokens": 1,
                    "messages": [{"role": "user", "content": "hi"}]
                })
                .to_string(),
            )
            .send()
            .await
            .map_err(|e| AppError::Provider {
                provider_id: "minimax".into(),
                message: format!("health: {e}"),
                retryable: true,
            })?;
        if !resp.status().is_success() {
            return Err(AppError::Provider {
                provider_id: "minimax".into(),
                message: format!("health: http {}", resp.status().as_u16()),
                retryable: resp.status().is_server_error(),
            });
        }
        // Drain to release the connection.
        let _ = resp.bytes().await.map_err(|e| AppError::Provider {
            provider_id: "minimax".into(),
            message: format!("health: {e}"),
            retryable: false,
        })?;
        Ok(start.elapsed().as_millis() as u64)
    }
}

impl MinimaxProvider {
    async fn fetch_models(&self) -> AppResult<Vec<ModelInfo>> {
        let url = models_url(&self.inner.base_url);
        let resp = self
            .inner
            .client
            .get(&url)
            .bearer_auth(&self.inner.api_key)
            .send()
            .await
            .map_err(|e| AppError::Provider {
                provider_id: "minimax".into(),
                message: format!("list_models: {e}"),
                retryable: e.is_timeout() || e.is_connect() || e.is_request(),
            })?;

        let status = resp.status();
        if !status.is_success() {
            return Err(AppError::Provider {
                provider_id: "minimax".into(),
                message: format!("list_models: http {}", status.as_u16()),
                retryable: status.is_server_error(),
            });
        }

        let payload = resp
            .json::<ModelListResponse>()
            .await
            .map_err(|e| AppError::Provider {
                provider_id: "minimax".into(),
                message: format!("list_models decode: {e}"),
                retryable: false,
            })?;

        Ok(payload
            .data
            .into_iter()
            .map(|model| model_info_for_id(&model.id))
            .collect())
    }
}

#[derive(Debug, Deserialize)]
struct ModelListResponse {
    #[serde(default)]
    data: Vec<ModelListItem>,
}

#[derive(Debug, Deserialize)]
struct ModelListItem {
    id: String,
}

fn messages_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base == LEGACY_OPENAI_BASE_URL {
        return format!("{DEFAULT_BASE_URL}/v1/messages");
    }
    if base.ends_with("/v1") {
        format!("{base}/messages")
    } else {
        format!("{base}/v1/messages")
    }
}

fn models_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base == DEFAULT_BASE_URL || base == "https://api.minimax.io/anthropic/v1" {
        return format!("{LEGACY_OPENAI_BASE_URL}/models");
    }
    if base == LEGACY_OPENAI_BASE_URL {
        return format!("{base}/models");
    }
    if let Some(prefix) = base.strip_suffix("/anthropic/v1") {
        return format!("{prefix}/v1/models");
    }
    if let Some(prefix) = base.strip_suffix("/anthropic") {
        return format!("{prefix}/v1/models");
    }
    if base.ends_with("/v1") {
        format!("{base}/models")
    } else {
        format!("{base}/v1/models")
    }
}

fn static_models() -> Vec<ModelInfo> {
    MODELS
        .iter()
        .map(|(id, name, ctx, out)| ModelInfo {
            id: (*id).to_string(),
            name: (*name).to_string(),
            context_window: *ctx,
            max_output_tokens: *out,
            supports_tools: true,
            supports_vision: false,
        })
        .collect()
}

fn model_info_for_id(id: &str) -> ModelInfo {
    if let Some((_, name, ctx, out)) = MODELS.iter().find(|(known, _, _, _)| *known == id) {
        ModelInfo {
            id: id.to_string(),
            name: (*name).to_string(),
            context_window: *ctx,
            max_output_tokens: *out,
            supports_tools: true,
            supports_vision: false,
        }
    } else {
        ModelInfo {
            id: id.to_string(),
            name: id.to_string(),
            context_window: 4096,
            max_output_tokens: 1024,
            supports_tools: false,
            supports_vision: false,
        }
    }
}

// ============================================================
// Request body building (Anthropic-compat)
// ============================================================

fn build_request_body(req: &ChatRequest) -> Value {
    let (system, msgs) = split_system_message(&req.messages);
    let supports_tools = !req.tools.is_empty();
    let messages: Vec<Value> = msgs.into_iter().map(anthropic_message).collect();
    let mut body = json!({
        "model": req.model,
        "max_tokens": req.max_output_tokens.unwrap_or(8_192),
        "stream": req.stream,
        "messages": messages,
    });
    if let Some(sys) = system {
        body["system"] = json!(sys);
    }
    if let Some(t) = req.temperature {
        body["temperature"] = json!(t);
    }
    if supports_tools {
        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
                })
            })
            .collect();
        body["tools"] = json!(tools);
    }
    body
}

fn split_system_message(messages: &[ChatMessage]) -> (Option<String>, Vec<&ChatMessage>) {
    let mut system: Option<String> = None;
    let mut rest = Vec::new();
    for m in messages {
        if let ChatMessage::System { content } = m {
            system = Some(match system {
                Some(existing) => format!("{existing}\n{content}"),
                None => content.clone(),
            });
        } else {
            rest.push(m);
        }
    }
    (system, rest)
}

fn anthropic_message(msg: &ChatMessage) -> Value {
    match msg {
        ChatMessage::System { .. } => {
            // Already extracted by split_system_message; never
            // present here, but if it slips through, emit empty.
            json!({"role": "user", "content": ""})
        }
        ChatMessage::User { content } => {
            json!({"role": "user", "content": content})
        }
        ChatMessage::Assistant {
            content,
            tool_calls,
        } => {
            if tool_calls.is_empty() {
                json!({"role": "assistant", "content": content})
            } else {
                let blocks: Vec<Value> = std::iter::once(json!({"type": "text", "text": content}))
                    .chain(tool_calls.iter().map(|tc| {
                        json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.name,
                            "input": tc.args,
                        })
                    }))
                    .collect();
                json!({"role": "assistant", "content": blocks})
            }
        }
        ChatMessage::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => json!({
            "role": "user",
            "content": [{
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "content": content,
                "is_error": is_error,
            }]
        }),
    }
}

// ============================================================
// SSE → ChatEvent stream
// ============================================================

/// State for the Minimax SSE parser. We accumulate per content
/// block (`index` → `BlockState`) and apply `message_delta` usage
/// at `message_stop`.
#[derive(Default)]
struct MinimaxParserState {
    started: bool,
    blocks: HashMap<u32, BlockState>,
    usage: Usage,
    finish_reason: Option<FinishReason>,
}

#[derive(Default)]
struct BlockState {
    kind: BlockKind,
    text: String,
    tool_id: Option<String>,
    tool_name: Option<String>,
    tool_input: String,
}

#[derive(Default, PartialEq, Eq)]
enum BlockKind {
    #[default]
    Text,
    ToolUse,
}

fn sse_to_chat_events_minimax(
    byte_stream: impl Stream<Item = Result<Bytes, reqwest::Error>> + Send + Unpin + 'static,
) -> impl Stream<Item = Result<ChatEvent, AppError>> {
    let init = (String::new(), byte_stream, MinimaxParserState::default());
    futures::stream::unfold(init, |(mut buffer, mut stream, mut state)| async move {
        loop {
            if let Some(idx) = buffer.find("\n\n") {
                let event_str: String = buffer.drain(..idx + 2).collect();
                let events = parse_sse_event(&event_str, &mut state);
                if let Some(ev) = events.into_iter().next() {
                    return Some((Ok(ev), (buffer, stream, state)));
                }
                continue;
            }
            match stream.next().await {
                Some(Ok(chunk)) => {
                    buffer.push_str(&String::from_utf8_lossy(&chunk));
                }
                Some(Err(e)) => {
                    return Some((
                        Err(AppError::Provider {
                            provider_id: "minimax".into(),
                            message: format!("stream read: {e}"),
                            retryable: true,
                        }),
                        (buffer, stream, state),
                    ));
                }
                None => return None,
            }
        }
    })
}

fn parse_sse_event(event_str: &str, state: &mut MinimaxParserState) -> Vec<ChatEvent> {
    // Parse Anthropic-style SSE: lines "event: <name>" and "data: <json>".
    let mut event_name: Option<&str> = None;
    let mut data: Option<String> = None;
    for line in event_str.lines() {
        if let Some(name) = line.strip_prefix("event: ") {
            event_name = Some(name.trim());
        } else if let Some(d) = line.strip_prefix("data: ") {
            data = Some(d.trim().to_string());
        }
    }
    let Some(name) = event_name else {
        return vec![];
    };
    let Some(data) = data else { return vec![] };
    let payload: Value = match serde_json::from_str(&data) {
        Ok(v) => v,
        Err(e) => {
            return vec![ChatEvent::Error {
                code: "chunk_decode_failed".into(),
                message: format!("Anthropic SSE chunk: {e}"),
                retryable: false,
            }];
        }
    };
    match name {
        "message_start" => {
            if !state.started {
                state.started = true;
                let message_id = payload
                    .get("message")
                    .and_then(|m| m.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let model = payload
                    .get("message")
                    .and_then(|m| m.get("model"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                return vec![ChatEvent::MessageStart { message_id, model }];
            }
            vec![]
        }
        "content_block_start" => {
            let index = payload.get("index").and_then(Value::as_u64).unwrap_or(0) as u32;
            let block = payload.get("content_block").cloned().unwrap_or(Value::Null);
            let kind = block.get("type").and_then(Value::as_str).unwrap_or("text");
            let entry = state.blocks.entry(index).or_default();
            if kind == "tool_use" {
                entry.kind = BlockKind::ToolUse;
                entry.tool_id = block.get("id").and_then(Value::as_str).map(str::to_string);
                entry.tool_name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                entry.tool_input.clear();
            } else {
                entry.kind = BlockKind::Text;
            }
            vec![]
        }
        "content_block_delta" => {
            let index = payload.get("index").and_then(Value::as_u64).unwrap_or(0) as u32;
            let delta = payload.get("delta").cloned().unwrap_or(Value::Null);
            let delta_kind = delta.get("type").and_then(Value::as_str).unwrap_or("");
            if delta_kind == "text_delta" {
                let text = delta
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if let Some(entry) = state.blocks.get_mut(&index) {
                    entry.text.push_str(&text);
                }
                if !text.is_empty() {
                    return vec![ChatEvent::ContentDelta { text }];
                }
            } else if delta_kind == "input_json_delta" {
                let partial = delta
                    .get("partial_json")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if let Some(entry) = state.blocks.get_mut(&index) {
                    entry.tool_input.push_str(&partial);
                }
            }
            vec![]
        }
        "content_block_stop" => {
            let index = payload.get("index").and_then(Value::as_u64).unwrap_or(0) as u32;
            if let Some(entry) = state.blocks.remove(&index) {
                if entry.kind == BlockKind::ToolUse {
                    let args: Value =
                        serde_json::from_str(&entry.tool_input).unwrap_or(Value::Null);
                    return vec![ChatEvent::ToolUse {
                        id: entry.tool_id.unwrap_or_default(),
                        name: entry.tool_name.unwrap_or_default(),
                        args,
                    }];
                }
            }
            vec![]
        }
        "message_delta" => {
            // Update usage (caching) and stop_reason.
            if let Some(usage) = payload.get("usage") {
                if let Some(p) = usage.get("input_tokens").and_then(Value::as_u64) {
                    state.usage.prompt_tokens = p as u32;
                }
                if let Some(c) = usage.get("output_tokens").and_then(Value::as_u64) {
                    state.usage.completion_tokens = c as u32;
                }
                if let Some(cr) = usage.get("cache_read_input_tokens").and_then(Value::as_u64) {
                    state.usage.cache_read_tokens = Some(cr as u32);
                }
                if let Some(cw) = usage
                    .get("cache_creation_input_tokens")
                    .and_then(Value::as_u64)
                {
                    state.usage.cache_write_tokens = Some(cw as u32);
                }
            }
            if let Some(reason) = payload
                .get("delta")
                .and_then(|d| d.get("stop_reason"))
                .and_then(Value::as_str)
            {
                state.finish_reason = Some(match reason {
                    "end_turn" => FinishReason::Stop,
                    "max_tokens" => FinishReason::Length,
                    "stop_sequence" => FinishReason::Stop,
                    "tool_use" => FinishReason::Stop,
                    _ => FinishReason::Stop,
                });
            }
            vec![]
        }
        "message_stop" => {
            let finish_reason = state.finish_reason.take().unwrap_or(FinishReason::Stop);
            let usage = std::mem::take(&mut state.usage);
            vec![ChatEvent::MessageEnd {
                usage,
                finish_reason,
            }]
        }
        "error" | "ping" => vec![],
        _ => vec![],
    }
}

#[derive(Debug, Deserialize)]
struct _UnusedAnthropicError {
    #[serde(default)]
    error: Option<AnthropicErrorInner>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AnthropicErrorInner {
    #[serde(default)]
    message: Option<String>,
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
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
    async fn list_models_discovers_models_from_api() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .and(header("authorization", "Bearer fake-key"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_json(json!({
                        "data": [
                            {"id": "MiniMax-M3", "object": "model"},
                            {"id": "MiniMax-Custom-X", "object": "model"}
                        ]
                    })),
            )
            .mount(&server)
            .await;

        let p = MinimaxProvider::with_base_url(server.uri(), "fake-key").unwrap();
        let models = p.list_models().await.unwrap();
        assert_eq!(models.first().map(|m| m.id.as_str()), Some("MiniMax-M3"));
        assert!(models.iter().any(|m| m.id == "MiniMax-Custom-X"));
        assert!(models
            .iter()
            .any(|m| m.id == "MiniMax-M3" && m.supports_tools));
    }

    #[tokio::test]
    async fn list_models_falls_back_to_static_set_when_endpoint_is_unavailable() {
        let server = MockServer::start().await;
        let p = MinimaxProvider::with_base_url(server.uri(), "fake-key").unwrap();
        let models = p.list_models().await.unwrap();
        assert_eq!(models.first().map(|m| m.id.as_str()), Some("MiniMax-M3"));
        assert!(models.iter().any(|m| m.id == "MiniMax-M2.7"));
        assert!(models.iter().all(|m| m.supports_tools));
    }

    #[tokio::test]
    async fn list_models_does_not_fallback_on_auth_failure() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .and(header("authorization", "Bearer bad-key"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let p = MinimaxProvider::with_base_url(server.uri(), "bad-key").unwrap();
        let err = p.list_models().await.unwrap_err();
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

    #[test]
    fn capabilities_known_model_has_tools() {
        let p = MinimaxProvider::new("fake").unwrap();
        let caps = p.capabilities("MiniMax-M3");
        assert!(caps.tools);
        assert_eq!(caps.context_window, 1_000_000);
    }

    #[test]
    fn capabilities_unknown_model_is_conservative() {
        let p = MinimaxProvider::new("fake").unwrap();
        let caps = p.capabilities("minimax-99");
        assert!(!caps.tools);
    }

    #[test]
    fn messages_url_uses_official_anthropic_default() {
        assert_eq!(
            messages_url(DEFAULT_BASE_URL),
            "https://api.minimax.io/anthropic/v1/messages"
        );
    }

    #[test]
    fn messages_url_maps_legacy_openai_default_to_anthropic_endpoint() {
        assert_eq!(
            messages_url("https://api.minimax.io/v1"),
            "https://api.minimax.io/anthropic/v1/messages"
        );
    }

    #[test]
    fn messages_url_preserves_custom_anthropic_v1_base() {
        assert_eq!(
            messages_url("https://example.test/anthropic/v1"),
            "https://example.test/anthropic/v1/messages"
        );
    }

    #[test]
    fn models_url_uses_official_models_endpoint_for_default() {
        assert_eq!(
            models_url(DEFAULT_BASE_URL),
            "https://api.minimax.io/v1/models"
        );
    }

    #[test]
    fn models_url_preserves_legacy_openai_default() {
        assert_eq!(
            models_url("https://api.minimax.io/v1"),
            "https://api.minimax.io/v1/models"
        );
    }

    #[test]
    fn models_url_maps_custom_anthropic_base_to_provider_root() {
        assert_eq!(
            models_url("https://example.test/anthropic/v1"),
            "https://example.test/v1/models"
        );
    }

    #[tokio::test]
    async fn health_against_wiremock_ok() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("authorization", "Bearer fake-key"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_json(json!({
                        "id": "msg_x",
                        "type": "message",
                        "role": "assistant",
                        "content": [{"type": "text", "text": "ok"}],
                        "model": "MiniMax-M2.7",
                        "stop_reason": "end_turn",
                        "usage": {"input_tokens": 1, "output_tokens": 1}
                    })),
            )
            .mount(&server)
            .await;

        let p = MinimaxProvider::with_base_url(server.uri(), "fake-key").unwrap();
        let latency = p.health().await.unwrap();
        assert!(latency < 5_000);
    }

    #[tokio::test]
    async fn health_against_wiremock_unauthorized() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("authorization", "Bearer bad-key"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let p = MinimaxProvider::with_base_url(server.uri(), "bad-key").unwrap();
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

    #[test]
    fn parse_sse_event_message_start_emits_message_start() {
        let sse = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"MiniMax-M2.7\"}}\n\n";
        let mut state = MinimaxParserState::default();
        let events = parse_sse_event(sse, &mut state);
        assert_eq!(events.len(), 1);
        match &events[0] {
            ChatEvent::MessageStart { message_id, model } => {
                assert_eq!(message_id, "msg_1");
                assert_eq!(model, "MiniMax-M2.7");
            }
            other => panic!("expected MessageStart, got {other:?}"),
        }
    }

    #[test]
    fn parse_sse_event_text_delta_emits_content_delta() {
        let sse = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n";
        let mut state = MinimaxParserState::default();
        // Pre-seed the block as text.
        state.blocks.insert(
            0,
            BlockState {
                kind: BlockKind::Text,
                ..Default::default()
            },
        );
        let events = parse_sse_event(sse, &mut state);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ChatEvent::ContentDelta { text } if text == "Hi"));
    }

    #[test]
    fn parse_sse_event_message_delta_with_cache_tokens_records_usage() {
        let sse = "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"input_tokens\":10,\"output_tokens\":5,\"cache_read_input_tokens\":3,\"cache_creation_input_tokens\":1}}\n\n";
        let mut state = MinimaxParserState::default();
        let _ = parse_sse_event(sse, &mut state);
        assert_eq!(state.usage.prompt_tokens, 10);
        assert_eq!(state.usage.completion_tokens, 5);
        assert_eq!(state.usage.cache_read_tokens, Some(3));
        assert_eq!(state.usage.cache_write_tokens, Some(1));
    }

    #[test]
    fn parse_sse_event_message_stop_emits_message_end() {
        let sse = "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
        let mut state = MinimaxParserState::default();
        state.usage.prompt_tokens = 7;
        state.usage.completion_tokens = 4;
        state.finish_reason = Some(FinishReason::Stop);
        let events = parse_sse_event(sse, &mut state);
        assert_eq!(events.len(), 1);
        match &events[0] {
            ChatEvent::MessageEnd {
                usage,
                finish_reason,
            } => {
                assert_eq!(usage.prompt_tokens, 7);
                assert_eq!(usage.completion_tokens, 4);
                assert_eq!(*finish_reason, FinishReason::Stop);
            }
            other => panic!("expected MessageEnd, got {other:?}"),
        }
    }

    #[test]
    fn parse_sse_event_tool_use_block_emits_tool_use() {
        // First content_block_start declares a tool_use block.
        let start = "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"read_file\"}}\n\n";
        let mut state = MinimaxParserState::default();
        let _ = parse_sse_event(start, &mut state);
        // Then a JSON input delta.
        let delta = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"path\\\":\\\"x\\\"}\"}}\n\n";
        let _ = parse_sse_event(delta, &mut state);
        // Finally content_block_stop.
        let stop =
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n";
        let events = parse_sse_event(stop, &mut state);
        assert_eq!(events.len(), 1);
        match &events[0] {
            ChatEvent::ToolUse { id, name, args } => {
                assert_eq!(id, "t1");
                assert_eq!(name, "read_file");
                assert_eq!(args, &json!({"path": "x"}));
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn build_request_body_separates_system_message() {
        let req = chat_request(
            "MiniMax-M2.7",
            vec![
                ChatMessage::System {
                    content: "be helpful".into(),
                },
                ChatMessage::User {
                    content: "hi".into(),
                },
            ],
        );
        let body = build_request_body(&req);
        assert_eq!(body["system"], "be helpful");
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
    }
}
