//! `AppError` — single error type for the entire `agentyx-core` crate.
//!
//! Used in every public API of the crate. Variants are deliberately
//! small and orthogonal so the UI can map them to user-facing
//! messages without parsing free-form text.
//!
//! See `../../specs/ipc.md` §"Error shape" for the JSON contract:
//! ```json
//! { "code": "not_found", "message": "...", "context": { ... } }
//! ```
//!
//! Conventions:
//! - **Stable `code`s**: the variant name is the code (`NotFound`
//!   → `"not_found"` via serde rename). UI maps these to i18n keys.
//! - **No secrets in `message`**: the Display impl must never include
//!   API keys, file contents, or absolute paths. Use `context` (only
//!   serialized in debug builds) for diagnostics.
//! - **No PII in `message`**: same rule, harder to enforce automatically.
//! - **`From` impls**: see `impl_from.rs` (when needed) for
//!   ergonomic `?` usage with `io::Error`, `rusqlite::Error`, etc.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Crate-wide result alias. Use this everywhere instead of
/// `Result<T, AppError>` to keep call sites terse.
pub type AppResult<T> = std::result::Result<T, AppError>;

/// All errors surfaced from `agentyx-core` to the rest of the app
/// (the Tauri command layer, the HTTP layer, the UI, the journal).
///
/// The `Serialize + Deserialize` impls produce camelCase JSON
/// suitable for crossing the IPC boundary; the UI is expected to
/// pattern-match on `code` (a string) and use `message` for
/// end-user display.
#[derive(Debug, Error, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "code", content = "context")]
pub enum AppError {
    /// A requested resource (workspace, session, agent, file) was not found.
    /// `context` typically contains the `id` and the `kind` of resource.
    #[error("not found: {kind} (id={id})")]
    NotFound {
        /// What kind of resource was being looked up.
        kind: String,
        /// Its identifier.
        id: String,
    },

    /// Input failed validation (bad URL, missing field, value out of range,
    /// a literal API key in TOML, etc.).
    #[error("invalid input: {message}")]
    InvalidInput {
        /// Human-readable explanation, safe to show in the UI.
        message: String,
    },

    /// The user (or their settings) explicitly denied an action.
    /// Distinct from `InvalidInput` (which is about shape) — this is
    /// about policy. Used by the permission gate.
    #[error("forbidden: {action}")]
    Forbidden {
        /// What the user declined.
        action: String,
    },

    /// A concurrency conflict (e.g. trying to set the active agent
    /// while a run is in progress).
    #[error("conflict: {message}")]
    Conflict {
        /// Human-readable explanation.
        message: String,
    },

    /// Operation timed out (provider request, tool execution, subagent run).
    #[error("timeout after {ms}ms ({op})")]
    Timeout {
        /// Operation that timed out.
        op: String,
        /// Timeout in milliseconds.
        ms: u64,
    },

    /// I/O error (file system, network, PTY, SQLite I/O). The
    /// `source` field is a short, non-sensitive description; the
    /// original `io::Error` is logged via `tracing::error!` but
    /// never serialized to the UI.
    #[error("io error during {op}: {source}")]
    Io {
        /// What was being attempted.
        op: String,
        /// Short description (e.g. "permission denied", "no such file").
        source: String,
    },

    /// Provider (LLM) error. Wraps both transport failures and
    /// provider-reported errors (4xx, 5xx, malformed SSE, etc.).
    #[error("provider error ({provider_id}): {message}")]
    Provider {
        /// Provider id (`"ollama"`, `"groq"`, `"minimax"`).
        provider_id: String,
        /// Short, UI-safe message.
        message: String,
        /// Whether retrying the same request is likely to succeed.
        #[serde(default)]
        retryable: bool,
    },

    /// A tool execution failed. Distinct from `Provider` because
    /// tools run inside our process and have stable error modes
    /// (file not found, command not found, exit code != 0, etc.).
    #[error("tool error ({tool}): {message}")]
    Tool {
        /// Tool name (`"read_file"`, `"shell"`, etc.).
        tool: String,
        /// Short message.
        message: String,
    },

    /// A path was outside the workspace sandbox (root + extra_paths).
    /// See ADR-0007.
    #[error("path outside workspace: {path}")]
    PathOutsideWorkspace {
        /// The rejected path (relative form, never absolute user paths).
        path: String,
    },

    /// Catch-all for unexpected internal failures. Always logged
    /// with full context. The UI shows a generic "Something went
    /// wrong" message; details go to the journal.
    #[error("internal error: {message}")]
    Internal {
        /// Short, UI-safe summary. Full backtrace in `tracing`.
        message: String,
    },
}

impl AppError {
    /// Stable string code used by the UI for i18n. Equivalent to
    /// the JSON `code` field but available without serializing.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound { .. } => "not_found",
            Self::InvalidInput { .. } => "invalid_input",
            Self::Forbidden { .. } => "forbidden",
            Self::Conflict { .. } => "conflict",
            Self::Timeout { .. } => "timeout",
            Self::Io { .. } => "io",
            Self::Provider { .. } => "provider",
            Self::Tool { .. } => "tool",
            Self::PathOutsideWorkspace { .. } => "path_outside_workspace",
            Self::Internal { .. } => "internal",
        }
    }
}
