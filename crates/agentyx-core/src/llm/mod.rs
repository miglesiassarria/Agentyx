//! LLM providers — `Provider` trait + `ChatEvent` normalization
//! plus v1 implementations (Ollama only in F01-Phase1; Groq and
//! Minimax land in F01-Phase2+).
//!
//! See `../../../specs/domains/providers.md` for the full design.
//!
//! ## What's in this PR (F01-Phase1)
//!
//! - `Provider` trait with `chat()` returning a `Pin<Box<dyn Stream<...>>>`
//!   of `Result<ChatEvent, AppError>`.
//! - `ChatEvent` enum (`MessageStart`, `ContentDelta`, `MessageEnd`,
//!   `Error`).
//! - `ChatRequest` / `ChatMessage` (system / user / assistant / tool_result).
//! - `Usage` and `FinishReason`.
//! - `ModelInfo` and `ModelCapabilities`.
//! - `Ollama` impl: NDJSON streaming against `POST {base_url}/api/chat`.
//! - Tests with `wiremock` covering the happy path, an Ollama
//!   unreachable path, and a tool call normalization.
//!
//! ## Deferred
//!
//! - `Groq` and `Minimax` providers (Anthropic-compatible SSE
//!   shape). Tracked in F01-Phase2.
//! - `ProviderRegistry` for listing/configuring providers
//!   (lands with F05).
//! - `list_models()` for Ollama (only metadata used in F01-Phase1).

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod mock;
mod ollama;
mod provider;
mod types;

pub use mock::{MockProvider, MockSequence};
pub use ollama::{OllamaProvider, DEFAULT_BASE_URL};
pub use provider::Provider;
pub use types::{
    ChatEvent, ChatMessage, ChatRequest, FinishReason, ModelCapabilities, ModelInfo,
    RequestMetadata, ToolCall, ToolChoice, ToolSchema, Usage,
};
