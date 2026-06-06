//! Common types for the LLM layer: `ChatEvent`, `ChatRequest`,
//! `ChatMessage`, `Usage`, `ModelInfo`, `ModelCapabilities`.

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, RunId, SessionId, WorkspaceId};
use crate::AppError;

/// A request to a provider. The agent loop builds this from the
/// session history, the active agent, and the tool registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    /// Model id (e.g. `"llama3.1:8b"`).
    pub model: String,
    /// Conversation history.
    pub messages: Vec<ChatMessage>,
    /// Tool schemas to expose to the model (filtered by agent's
    /// `tool_access`).
    #[serde(default)]
    pub tools: Vec<ToolSchema>,
    /// Tool choice (auto / any / none / specific).
    #[serde(default)]
    pub tool_choice: ToolChoice,
    /// Max output tokens (provider default if `None`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    /// Sampling temperature. Range 0.0–1.0; provider default if
    /// `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Whether to stream. Always `true` in v1.
    pub stream: bool,
    /// Tracing metadata (not sent to the provider).
    pub metadata: RequestMetadata,
}

/// Metadata attached to a request. The provider **does not**
/// include this in the wire format; the agent loop uses it for
/// tracing and journal correlation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestMetadata {
    /// Workspace the run belongs to.
    pub workspace_id: WorkspaceId,
    /// Session the run belongs to.
    pub session_id: SessionId,
    /// Run id (ULID).
    pub run_id: RunId,
    /// Agent id (built-in or custom).
    pub agent_id: AgentId,
}

/// A message in the conversation. Mirrors the OpenAI/Anthropic
/// shapes; the provider layer normalizes per-protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "role")]
pub enum ChatMessage {
    /// System prompt.
    System {
        /// Prompt text.
        content: String,
    },
    /// User input.
    User {
        /// Message text.
        content: String,
    },
    /// Assistant response. May include tool calls.
    Assistant {
        /// Response text.
        content: String,
        /// Tool calls the model wants to make.
        #[serde(default)]
        tool_calls: Vec<ToolCall>,
    },
    /// Tool result returned to the model.
    ToolResult {
        /// Id of the tool use this is responding to.
        tool_use_id: String,
        /// Tool output.
        content: String,
        /// Whether the tool returned an error.
        is_error: bool,
    },
}

/// A tool call the model wants to make.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    /// Tool call id (provider-assigned).
    pub id: String,
    /// Tool name.
    pub name: String,
    /// Tool arguments.
    pub args: serde_json::Value,
}

/// Schema of a tool exposed to the model. Mirrors OpenAI's
/// function-calling shape; Anthropic-format translation lives
/// in the Minimax provider (F01-Phase2).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSchema {
    /// Tool name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON schema for the tool's parameters.
    pub parameters: serde_json::Value,
}

/// Tool choice directive.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoice {
    /// Model decides whether to call a tool.
    #[default]
    Auto,
    /// Model must call at least one tool.
    Any,
    /// Model must not call any tool.
    None,
}

/// Normalized streaming event from a provider. The agent loop
/// and the UI only know `ChatEvent`; provider-specific shapes
/// are translated inside each `Provider` impl.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ChatEvent {
    /// The run is starting. Emitted exactly once per `chat()`.
    MessageStart {
        /// Provider-assigned message id (used to correlate
        /// subsequent events).
        message_id: String,
        /// Model id that produced the message.
        model: String,
    },
    /// A chunk of content text.
    ContentDelta {
        /// The new text to append.
        text: String,
    },
    /// The model wants to call a tool. The agent loop is
    /// responsible for routing it through the permission gate.
    ToolUse {
        /// Tool call id.
        id: String,
        /// Tool name.
        name: String,
        /// Tool arguments.
        args: serde_json::Value,
    },
    /// End of the message.
    MessageEnd {
        /// Token usage for the turn.
        usage: Usage,
        /// Why the model stopped.
        finish_reason: FinishReason,
    },
    /// A transport or protocol error. The agent loop decides
    /// whether to retry or abort the run.
    Error {
        /// Stable error code (e.g. `"auth_failed"`, `"stream_interrupted"`).
        code: String,
        /// UI-safe error message.
        message: String,
        /// Whether retrying is likely to succeed.
        retryable: bool,
    },
}

/// Token usage for a turn.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    /// Tokens in the prompt.
    pub prompt_tokens: u32,
    /// Tokens in the completion.
    pub completion_tokens: u32,
    /// Cached prompt tokens read (Anthropic / Minimax).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u32>,
    /// Cached prompt tokens written (Anthropic / Minimax).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write_tokens: Option<u32>,
}

/// Why the model stopped.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// Model finished naturally.
    Stop,
    /// Hit the max tokens limit.
    Length,
    /// Blocked by content filter.
    ContentFilter,
    /// Provider error.
    Error,
    /// Aborted by the agent loop.
    Aborted,
}

/// Information about a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    /// Model id (e.g. `"llama3.1:8b"`).
    pub id: String,
    /// Human-readable name (may be the same as `id`).
    pub name: String,
    /// Context window in tokens.
    pub context_window: u32,
    /// Max output tokens.
    pub max_output_tokens: u32,
    /// Whether the model supports function-calling / tools.
    pub supports_tools: bool,
    /// Whether the model accepts image inputs.
    pub supports_vision: bool,
}

/// Capabilities of a specific model. Returned by
/// `Provider::capabilities(model_id)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilities {
    /// Whether the model supports tools.
    pub tools: bool,
    /// Whether the model supports image input.
    pub vision: bool,
    /// Context window in tokens.
    pub context_window: u32,
    /// Max output tokens.
    pub max_output_tokens: u32,
}

/// Stream of `ChatEvent` returned by a `Provider::chat()` call.
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatEvent, AppError>> + Send + 'static>>;

/// A LLM provider.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Stable id (e.g. `"ollama"`).
    fn id(&self) -> &'static str;

    /// Human-readable name.
    fn name(&self) -> &'static str;

    /// Capabilities of a specific model. If the model is unknown,
    /// returns conservative defaults (no tools, no vision,
    /// 4096-token context).
    fn capabilities(&self, model_id: &str) -> ModelCapabilities;

    /// Start a chat. The returned stream **must** end with
    /// `ChatEvent::MessageEnd` or `ChatEvent::Error` (not both).
    async fn chat(&self, req: ChatRequest) -> Result<ChatStream, AppError>;

    /// Health check. Returns `Ok(latency_ms)` if the provider
    /// is reachable. Used by `providers_test_connection` (F05).
    async fn health(&self) -> Result<u64, AppError>;
}
