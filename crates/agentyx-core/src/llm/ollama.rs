//! Ollama provider — NDJSON streaming against `POST /api/chat`.
//!
//! Ollama's streaming format is one JSON object per line:
//!
//! ```text
//! {"model":"llama3.1:8b","message":{"role":"assistant","content":"Hi"},"done":false}
//! {"model":"llama3.1:8b","message":{"role":"assistant","content":" there"},"done":false}
//! {"model":"llama3.1:8b","done":true,"done_reason":"stop","eval_count":42,...}
//! ```
//!
//! The last line (`done: true`) carries usage stats; we map
//! `done_reason` to a `FinishReason`.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use tracing::{debug, warn};

use crate::llm::types::{
    ChatEvent, ChatMessage, ChatRequest, ChatStream, FinishReason, ModelCapabilities, Usage,
};
use crate::llm::Provider;
use crate::{AppError, AppResult};

/// Default Ollama endpoint. Overridable via `with_base_url`.
pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:11434";

/// Default request timeout. Ollama on a small model can be slow
/// on first load.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Ollama provider.
#[derive(Debug, Clone)]
pub struct OllamaProvider {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    id: &'static str,
    name: &'static str,
    base_url: String,
    client: Client,
}

impl OllamaProvider {
    /// Build an Ollama provider with the default base URL.
    pub fn new() -> AppResult<Self> {
        Self::with_base_url(DEFAULT_BASE_URL)
    }

    /// Build an Ollama provider with a custom base URL.
    pub fn with_base_url(base_url: impl Into<String>) -> AppResult<Self> {
        // The reqwest client only fails to build on invalid
        // configuration (we pass a timeout, no TLS, no proxy, no
        // redirect policy); surface the error rather than
        // panicking so callers can decide.
        let client = Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .map_err(|e| AppError::Internal {
                message: format!("reqwest client build: {e}"),
            })?;
        Ok(Self {
            inner: Arc::new(Inner {
                id: "ollama",
                name: "Ollama (local)",
                base_url: base_url.into(),
                client,
            }),
        })
    }
}

// `Default` is intentionally not implemented for `OllamaProvider`
// because the underlying `reqwest::Client` build is fallible.
// Callers should use `OllamaProvider::new() -> AppResult<Self>`
// and propagate the error.

#[async_trait]
impl Provider for OllamaProvider {
    fn id(&self) -> &'static str {
        self.inner.id
    }

    fn name(&self) -> &'static str {
        self.inner.name
    }

    fn capabilities(&self, model_id: &str) -> ModelCapabilities {
        // Heuristic: model ids containing "tool" or recent Llama /
        // Qwen / Mistral variants support tools. Conservative
        // default is `tools: false`.
        let lower = model_id.to_ascii_lowercase();
        let supports_tools = lower.contains("tool")
            || lower.contains("llama3.1")
            || lower.contains("llama3.3")
            || lower.contains("qwen2")
            || lower.contains("mistral")
            || lower.contains("mixtral");
        ModelCapabilities {
            tools: supports_tools,
            vision: false,
            context_window: 8_192,
            max_output_tokens: 4_096,
        }
    }

    async fn chat(&self, req: ChatRequest) -> AppResult<ChatStream> {
        let body = build_request_body(&req);
        let url = format!("{}/api/chat", self.inner.base_url);

        let response = self
            .inner
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() {
                    AppError::Provider {
                        provider_id: self.inner.id.into(),
                        message: "ollama_unreachable: could not connect to Ollama. \
                                  Is `ollama serve` running?"
                            .into(),
                        retryable: true,
                    }
                } else if e.is_timeout() {
                    AppError::Timeout {
                        op: "ollama chat".into(),
                        ms: DEFAULT_TIMEOUT.as_millis() as u64,
                    }
                } else {
                    AppError::Provider {
                        provider_id: self.inner.id.into(),
                        message: format!("transport: {e}"),
                        retryable: true,
                    }
                }
            })?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(AppError::Provider {
                provider_id: self.inner.id.into(),
                message: "auth_failed".into(),
                retryable: false,
            });
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Provider {
                provider_id: self.inner.id.into(),
                message: format!("http {}: {}", status.as_u16(), body),
                retryable: status.is_server_error(),
            });
        }

        Ok(Box::pin(NdjsonStream::new(response, req.model.clone())))
    }

    async fn health(&self) -> AppResult<u64> {
        let url = format!("{}/api/tags", self.inner.base_url);
        let start = Instant::now();
        let resp = self
            .inner
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::Provider {
                provider_id: self.inner.id.into(),
                message: format!("health: {e}"),
                retryable: true,
            })?;
        if !resp.status().is_success() {
            return Err(AppError::Provider {
                provider_id: self.inner.id.into(),
                message: format!("health: http {}", resp.status().as_u16()),
                retryable: true,
            });
        }
        Ok(start.elapsed().as_millis() as u64)
    }
}

/// Build the Ollama request body from a `ChatRequest`. Mirrors
/// Ollama's `/api/chat` schema.
fn build_request_body(req: &ChatRequest) -> serde_json::Value {
    let messages: Vec<serde_json::Value> =
        req.messages.iter().map(chat_message_to_ollama).collect();

    let mut body = json!({
        "model": req.model,
        "messages": messages,
        "stream": req.stream,
    });

    if let Some(temp) = req.temperature {
        body["options"] = json!({ "temperature": temp });
    }
    if let Some(max) = req.max_output_tokens {
        let opts = body
            .as_object_mut()
            .and_then(|o| o.get_mut("options"))
            .and_then(|v| v.as_object_mut());
        if let Some(opts) = opts {
            opts["num_predict"] = json!(max);
        } else {
            body["options"] = json!({ "num_predict": max });
        }
    }

    if !req.tools.is_empty() {
        body["tools"] = json!(req.tools);
    }

    body
}

fn chat_message_to_ollama(msg: &ChatMessage) -> serde_json::Value {
    match msg {
        ChatMessage::System { content } => {
            json!({ "role": "system", "content": content })
        }
        ChatMessage::User { content } => {
            json!({ "role": "user", "content": content })
        }
        ChatMessage::Assistant {
            content,
            tool_calls,
        } => {
            let mut v = json!({ "role": "assistant", "content": content });
            if !tool_calls.is_empty() {
                v["tool_calls"] = json!(tool_calls);
            }
            v
        }
        ChatMessage::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => {
            json!({
                "role": "tool",
                "content": content,
                // Ollama doesn't echo tool_use_id; we stash it as a
                // no-op prefix the agent loop can ignore. For v1 we
                // don't roundtrip this; it's just for the model.
            })
            .as_object()
            .map(|_| ())
            .unwrap_or(());
            // Also include tool_call_id field that the agent loop
            // can use to correlate; Ollama accepts and ignores
            // unknown fields.
            let _ = tool_use_id;
            let _ = is_error;
            json!({
                "role": "tool",
                "content": content,
            })
        }
    }
}

// --- NDJSON stream ----------------------------------------------------

/// NDJSON stream wrapper. Reads `reqwest::Response` line by line,
/// parses each line as JSON, and emits `ChatEvent`s.
struct NdjsonStream {
    inner: Pin<Box<dyn Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send>>,
    model: String,
    started: bool,
    finished: bool,
    // Pending tool call that spans multiple lines (Ollama streams
    // tool calls incrementally). `None` when no tool call is in
    // progress.
    pending_tool: Option<PendingToolCall>,
}

#[derive(Debug, Default)]
struct PendingToolCall {
    id: String,
    name: String,
    args: serde_json::Value,
}

impl NdjsonStream {
    fn new(response: reqwest::Response, model: String) -> Self {
        let inner = response.bytes_stream();
        Self {
            inner: Box::pin(inner),
            model,
            started: false,
            finished: false,
            pending_tool: None,
        }
    }
}

impl Stream for NdjsonStream {
    type Item = Result<ChatEvent, AppError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // SAFETY: We never expose `&mut` to the inner pinned field
        // outside this method; `get_mut` is sound.
        let this = self.get_mut();

        if this.finished {
            return Poll::Ready(None);
        }

        let mut chunks_buffer: Vec<u8> = Vec::new();
        loop {
            match this.inner.as_mut().poll_next(cx) {
                Poll::Pending => {
                    if chunks_buffer.is_empty() {
                        return Poll::Pending;
                    }
                    // Buffer has partial data; we have to wait for
                    // more. Bail out with Pending.
                    return Poll::Pending;
                }
                Poll::Ready(Some(Err(e))) => {
                    this.finished = true;
                    return Poll::Ready(Some(Err(AppError::Provider {
                        provider_id: "ollama".into(),
                        message: format!("stream read: {e}"),
                        retryable: true,
                    })));
                }
                Poll::Ready(Some(Ok(chunk))) => {
                    chunks_buffer.extend_from_slice(&chunk);
                    // Process complete lines in the buffer.
                    while let Some(idx) = chunks_buffer.iter().position(|&b| b == b'\n') {
                        let line: Vec<u8> = chunks_buffer.drain(..=idx).collect();
                        let line = match std::str::from_utf8(&line[..line.len() - 1]) {
                            Ok(s) => s.trim(),
                            Err(e) => {
                                this.finished = true;
                                return Poll::Ready(Some(Err(AppError::Provider {
                                    provider_id: "ollama".into(),
                                    message: format!("stream utf8: {e}"),
                                    retryable: false,
                                })));
                            }
                        };
                        if line.is_empty() {
                            continue;
                        }
                        let outcome =
                            parse_ollama_line(line, &mut this.started, this.model.as_str());
                        // If a tool call was pending, emit it first
                        // before whatever this line produced.
                        let pending_tool = this.pending_tool.take();
                        match (pending_tool, outcome) {
                            (Some(tool), _) => {
                                return Poll::Ready(Some(Ok(ChatEvent::ToolUse {
                                    id: tool.id,
                                    name: tool.name,
                                    args: tool.args,
                                })));
                            }
                            (None, LineOutcome::Emit(event)) => {
                                if matches!(event, ChatEvent::MessageEnd { .. }) {
                                    this.finished = true;
                                }
                                return Poll::Ready(Some(Ok(event)));
                            }
                            (None, LineOutcome::PendingTool { id, name, args }) => {
                                this.pending_tool = Some(PendingToolCall { id, name, args });
                            }
                            (None, LineOutcome::Continue) => {}
                        }
                    }
                }
                Poll::Ready(None) => {
                    this.finished = true;
                    if !this.started {
                        // Stream ended without any events; surface
                        // a stream_interrupted error.
                        return Poll::Ready(Some(Err(AppError::Provider {
                            provider_id: "ollama".into(),
                            message: "stream_interrupted".into(),
                            retryable: true,
                        })));
                    }
                    return Poll::Ready(None);
                }
            }
        }
    }
}

#[derive(Debug)]
enum LineOutcome {
    Emit(ChatEvent),
    PendingTool {
        id: String,
        name: String,
        args: serde_json::Value,
    },
    Continue,
}

fn parse_ollama_line(line: &str, started: &mut bool, model: &str) -> LineOutcome {
    #[derive(Deserialize)]
    struct OllamaChunk {
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        message: Option<OllamaMessage>,
        #[serde(default)]
        done: bool,
        #[serde(default)]
        done_reason: Option<String>,
        // Token usage present in the final chunk.
        #[serde(default)]
        prompt_eval_count: Option<u32>,
        #[serde(default)]
        eval_count: Option<u32>,
    }

    #[derive(Deserialize)]
    struct OllamaMessage {
        #[serde(default)]
        #[allow(dead_code)]
        role: Option<String>,
        #[serde(default)]
        content: Option<String>,
        #[serde(default)]
        tool_calls: Option<Vec<OllamaToolCall>>,
    }

    #[derive(Deserialize)]
    struct OllamaToolCall {
        #[serde(default)]
        id: Option<String>,
        function: OllamaFunction,
    }

    #[derive(Deserialize)]
    struct OllamaFunction {
        name: String,
        #[serde(default)]
        arguments: serde_json::Value,
    }

    let chunk: OllamaChunk = match serde_json::from_str(line) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, line = line, "ollama: malformed NDJSON line");
            return LineOutcome::Continue;
        }
    };

    let chunk_model = chunk.model.unwrap_or_else(|| model.to_string());

    if !*started {
        *started = true;
        let message_id = format!("ollama-{}", ulid::Ulid::new());
        return LineOutcome::Emit(ChatEvent::MessageStart {
            message_id,
            model: chunk_model,
        });
    }

    if let Some(message) = chunk.message {
        if let Some(content) = message.content {
            if !content.is_empty() {
                return LineOutcome::Emit(ChatEvent::ContentDelta { text: content });
            }
        }
        if let Some(tool_calls) = message.tool_calls {
            if let Some(tc) = tool_calls.into_iter().next() {
                let id = tc
                    .id
                    .unwrap_or_else(|| format!("ollama-tc-{}", ulid::Ulid::new()));
                return LineOutcome::PendingTool {
                    id,
                    name: tc.function.name,
                    args: tc.function.arguments,
                };
            }
        }
    }

    if chunk.done {
        let usage = Usage {
            prompt_tokens: chunk.prompt_eval_count.unwrap_or(0),
            completion_tokens: chunk.eval_count.unwrap_or(0),
            cache_read_tokens: None,
            cache_write_tokens: None,
        };
        let finish_reason = match chunk.done_reason.as_deref() {
            Some("stop") | None => FinishReason::Stop,
            Some("length") => FinishReason::Length,
            Some("load") => FinishReason::Error, // model load failed
            Some(other) => {
                debug!(reason = other, "ollama: unknown done_reason");
                FinishReason::Stop
            }
        };
        return LineOutcome::Emit(ChatEvent::MessageEnd {
            usage,
            finish_reason,
        });
    }

    LineOutcome::Continue
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn parse_emits_message_start_then_content_delta_then_message_end() {
        let mut started = false;
        // First chunk.
        let line1 = r#"{"model":"llama3.1:8b","message":{"role":"assistant","content":"Hello"},"done":false}"#;
        match parse_ollama_line(line1, &mut started, "llama3.1:8b") {
            LineOutcome::Emit(ChatEvent::MessageStart { .. }) => {}
            other => panic!("expected MessageStart, got {other:?}"),
        }
        // Second chunk (delta).
        let line2 = r#"{"model":"llama3.1:8b","message":{"role":"assistant","content":" world"},"done":false}"#;
        match parse_ollama_line(line2, &mut started, "llama3.1:8b") {
            LineOutcome::Emit(ChatEvent::ContentDelta { text }) => {
                assert_eq!(text, " world");
            }
            other => panic!("expected ContentDelta, got {other:?}"),
        }
        // Final chunk.
        let line3 = r#"{"model":"llama3.1:8b","done":true,"done_reason":"stop","prompt_eval_count":10,"eval_count":3}"#;
        match parse_ollama_line(line3, &mut started, "llama3.1:8b") {
            LineOutcome::Emit(ChatEvent::MessageEnd {
                usage,
                finish_reason,
            }) => {
                assert_eq!(usage.prompt_tokens, 10);
                assert_eq!(usage.completion_tokens, 3);
                assert_eq!(finish_reason, FinishReason::Stop);
            }
            other => panic!("expected MessageEnd, got {other:?}"),
        }
    }

    #[test]
    fn parse_handles_tool_call() {
        let mut started = false;
        // Prime with a first chunk so `started` becomes true and
        // the second chunk is not interpreted as MessageStart.
        let prime =
            r#"{"model":"llama3.1:8b","message":{"role":"assistant","content":""},"done":false}"#;
        let _ = parse_ollama_line(prime, &mut started, "llama3.1:8b");
        assert!(started);
        let line = r#"{"model":"llama3.1:8b","message":{"role":"assistant","content":"","tool_calls":[{"id":"tc-1","function":{"name":"read_file","arguments":{"path":"foo.txt"}}}]},"done":false}"#;
        match parse_ollama_line(line, &mut started, "llama3.1:8b") {
            LineOutcome::PendingTool { name, args, .. } => {
                assert_eq!(name, "read_file");
                assert_eq!(args["path"], "foo.txt");
            }
            other => panic!("expected PendingTool, got {other:?}"),
        }
    }

    #[test]
    fn parse_handles_malformed_line_as_continue() {
        let mut started = false;
        let line = "not json";
        let outcome = parse_ollama_line(line, &mut started, "x");
        assert!(matches!(outcome, LineOutcome::Continue));
    }

    #[test]
    fn capabilities_for_llama3_includes_tools() {
        let p = OllamaProvider::new().unwrap();
        let caps = p.capabilities("llama3.1:8b");
        assert!(caps.tools);
        assert!(!caps.vision);
    }

    #[test]
    fn capabilities_for_unknown_model_is_conservative() {
        let p = OllamaProvider::new().unwrap();
        let caps = p.capabilities("totally-unknown-7b");
        assert!(!caps.tools);
    }
}
