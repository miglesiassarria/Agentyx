//! Domain types for the journal.

use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, RunId, SessionId};
use ulid::Ulid;

/// The kind of journal entry. Matches journal.md §Operations::Kind
/// (subset for F01-Phase1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalKind {
    /// User message (from `session.send`).
    UserMessage,
    /// Assistant message (full text at `MessageEnd`).
    AssistantMessage,
    /// Provider transport event (latency, status, error code).
    ProviderEvent,
    /// Tool invocation request from the model.
    ToolCall,
    /// Tool execution result.
    ToolResult,
    /// Permission decision (allow/deny/ask).
    PermissionDecision,
    /// Subagent lifecycle event.
    SubagentLifecycle,
    /// Error from the agent loop (not from the provider).
    Error,
}

impl JournalKind {
    /// String form used in SQL.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UserMessage => "user_message",
            Self::AssistantMessage => "assistant_message",
            Self::ProviderEvent => "provider_event",
            Self::ToolCall => "tool_call",
            Self::ToolResult => "tool_result",
            Self::PermissionDecision => "permission_decision",
            Self::SubagentLifecycle => "subagent_lifecycle",
            Self::Error => "error",
        }
    }

    /// Parse from a string. Returns `None` for unknown values.
    #[must_use]
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "user_message" => Some(Self::UserMessage),
            "assistant_message" => Some(Self::AssistantMessage),
            "provider_event" => Some(Self::ProviderEvent),
            "tool_call" => Some(Self::ToolCall),
            "tool_result" => Some(Self::ToolResult),
            "permission_decision" => Some(Self::PermissionDecision),
            "subagent_lifecycle" => Some(Self::SubagentLifecycle),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

/// A single journal entry. See journal.md §State::JournalEntry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntry {
    /// Unique entry id (ULID).
    pub id: Ulid,
    /// Wall-clock timestamp (ms epoch).
    pub ts: i64,
    /// Session the entry belongs to.
    pub session_id: SessionId,
    /// Run that produced the entry.
    pub run_id: RunId,
    /// Parent run id (for subagent entries; F01-Phase2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<Ulid>,
    /// Depth in the run tree (0 = primary).
    pub depth: u8,
    /// Entry kind.
    pub kind: JournalKind,
    /// Agent id (active agent when the entry was logged).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<AgentId>,
    /// JSON payload (truncated to 16 KiB if needed).
    pub payload: serde_json::Value,
    /// Whether `payload` was truncated from the original.
    pub payload_truncated: bool,
    /// SHA-256 of the original payload, hex-encoded, if truncated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_sha256: Option<String>,
    /// Duration in milliseconds, where applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}
