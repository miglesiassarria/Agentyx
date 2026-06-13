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
/// The `Serialize + Deserialize` impls produce JSON suitable for
/// crossing the IPC boundary; the UI is expected to pattern-match
/// on `code` (a string) and use `message` for end-user display.
/// See `../../specs/ipc.md` §4.4 for the contractual shape:
/// `{ "code": "...", "message": "...", "context": { ... } }`.
///
/// We use `tag = "code"` (NOT `tag = "code", content = "context"`)
/// so each variant's fields appear at the top level of the JSON
/// object. That keeps `message` and `context` at the same depth
/// as `code`, matching the spec and the JS `ipc.ts` wrapper. The
/// tag (`code`) is snake_case (matching `AppError::code()`) and
/// the variant fields are camelCase for cross-language ergonomics.
#[derive(Debug, Error, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(
    tag = "code",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
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
    /// `reason` field is a short, non-sensitive description; the
    /// original `io::Error` is logged via `tracing::error!` but
    /// never serialized to the UI.
    #[error("io error during {op}: {reason}")]
    Io {
        /// What was being attempted.
        op: String,
        /// Short description (e.g. "permission denied", "no such file").
        reason: String,
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

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::assertions_on_constants
)]
mod tests {
    //! Wire-format tests for `AppError`. These pin the JSON shape
    //! that crosses the Tauri / HTTP boundary so the UI's
    //! `ui/src/lib/ipc.ts` wrapper can rely on `code` and `message`
    //! living at the top level of the payload (per
    //! `specs/ipc.md` §4.4). A previous bug used
    //! `#[serde(tag = "code", content = "context")]`, which nested
    //! every variant's fields under `context` and made `message`
    //! unreachable for the UI — resulting in
    //! `conflict: [object Object]`. The tests below regress that.

    use super::*;
    use serde_json::json;

    fn assert_top_level_code_and_message(err: &AppError, expected_code: &str) {
        let v = serde_json::to_value(err).expect("serialize");
        let obj = v.as_object().expect("object");
        assert_eq!(
            obj.get("code").and_then(|v| v.as_str()),
            Some(expected_code),
            "code is not a top-level string in {obj:?}",
        );
        assert!(
            obj.contains_key("message") && obj["message"].is_string(),
            "message is not a top-level string in {obj:?}",
        );
    }

    #[test]
    fn conflict_serializes_with_top_level_message() {
        let err = AppError::Conflict {
            message: "session has a running run; abort first".into(),
        };
        let v = serde_json::to_value(&err).expect("serialize");
        assert_eq!(
            v,
            json!({
                "code": "conflict",
                "message": "session has a running run; abort first",
            }),
        );
        assert_top_level_code_and_message(&err, "conflict");
    }

    #[test]
    fn not_found_serializes_with_top_level_kind_and_id() {
        let err = AppError::NotFound {
            kind: "workspace".into(),
            id: "01J".into(),
        };
        let v = serde_json::to_value(&err).expect("serialize");
        assert_eq!(
            v,
            json!({
                "code": "not_found",
                "kind": "workspace",
                "id": "01J",
            }),
        );
    }

    #[test]
    fn invalid_input_serializes_with_top_level_message() {
        let err = AppError::InvalidInput {
            message: "bad URL".into(),
        };
        let v = serde_json::to_value(&err).expect("serialize");
        assert_eq!(v, json!({ "code": "invalid_input", "message": "bad URL" }));
        assert_top_level_code_and_message(&err, "invalid_input");
    }

    #[test]
    fn round_trip_via_string_preserves_shape() {
        // Tauri serializes errors via JSON before they reach JS; the
        // reverse path is not used by the UI today, but we still
        // exercise it to lock the on-the-wire contract.
        let err = AppError::Conflict {
            message: "x".into(),
        };
        let s = serde_json::to_string(&err).expect("serialize");
        let back: AppError = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(back, err);
    }
}
