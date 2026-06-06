//! Domain types for sessions and messages.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, MessageId, RunId, SessionId, WorkspaceId};

/// Status of a session's latest run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    /// No run in progress, last run finished cleanly or never ran.
    Idle,
    /// A run is currently in progress.
    Running,
    /// Last run was aborted by the user or the agent loop.
    Aborted,
    /// Last run ended with an error.
    Errored,
}

impl SessionStatus {
    /// String form used in SQL `CHECK` constraints.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::Aborted => "aborted",
            Self::Errored => "errored",
        }
    }

    /// Parse from a string. Returns `None` for unknown values.
    #[must_use]
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "idle" => Some(Self::Idle),
            "running" => Some(Self::Running),
            "aborted" => Some(Self::Aborted),
            "errored" => Some(Self::Errored),
            _ => None,
        }
    }
}

/// One chat thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Session id.
    pub id: SessionId,
    /// Workspace the session belongs to.
    pub workspace_id: WorkspaceId,
    /// Parent session id (for child sessions; F01-Phase2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<SessionId>,
    /// Title (derived from the first user message; up to 200
    /// chars).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Status of the latest run.
    pub status: SessionStatus,
    /// Wall-clock creation time.
    pub created_at: DateTime<Utc>,
    /// Wall-clock last-update time (changes on every message).
    pub updated_at: DateTime<Utc>,
    /// Last run id (set when a run finishes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_id: Option<RunId>,
    /// Echo of the last run's `finish_reason` for quick UI display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_finish_reason: Option<String>,
    /// Active agent for the next run.
    pub active_agent_id: AgentId,
}

/// Role of a message in a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// User input.
    User,
    /// Assistant response.
    Assistant,
    /// System prompt (rarely persisted; usually a tool/system
    /// prelude).
    System,
    /// Tool result returned to the model.
    ToolResult,
}

impl MessageRole {
    /// String form used in SQL `CHECK` constraints.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
            Self::ToolResult => "tool_result",
        }
    }

    /// Parse from a string. Returns `None` for unknown values.
    #[must_use]
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "user" => Some(Self::User),
            "assistant" => Some(Self::Assistant),
            "system" => Some(Self::System),
            "tool_result" => Some(Self::ToolResult),
            _ => None,
        }
    }
}

/// One message in a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// Message id.
    pub id: MessageId,
    /// Session the message belongs to.
    pub session_id: SessionId,
    /// Run that produced the message (None for user messages and
    /// pre-session system prompts).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    /// Role.
    pub role: MessageRole,
    /// Content. For assistant messages with tool calls, this is
    /// the textual content; tool call data lives in a side channel
    /// (F01-Phase2).
    pub content: String,
    /// Wall-clock creation time.
    pub created_at: DateTime<Utc>,
    /// Monotonic sequence number within the session.
    pub seq: i64,
}
