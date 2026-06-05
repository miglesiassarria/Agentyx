//! `session` Tauri commands — F01 chat surface.
//!
//! All command signatures are placeholders in v0.1; they will be
//! implemented in Fase D (post-bootstrap) following the contracts
//! in `../../../specs/features/F01-chat-streaming.md`.

use agentyx_core::AppResult;
use agentyx_core::ids::SessionId;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

use crate::state::AppState;

/// Handle returned by `session_send` to identify a running run.
/// The frontend listens for events on this `runId`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunHandle {
    /// The run that was started.
    pub run_id: agentyx_core::ids::RunId,
}

/// DTO for a session summary in `session_list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummaryDto {
    /// The session id.
    pub id: SessionId,
    /// The agent active for new runs.
    pub active_agent: agentyx_core::ids::AgentId,
    /// Display title (truncated first user message in v0.1).
    pub title: String,
    /// Wall-clock timestamp of last update.
    pub updated_at: i64,
}

/// Create a new session in a workspace.
#[tauri::command]
pub async fn create(
    _state: State<'_, Arc<AppState>>,
    _workspace_id: agentyx_core::ids::WorkspaceId,
    _agent_id: Option<agentyx_core::ids::AgentId>,
    _title: Option<String>,
) -> AppResult<agentyx_core::session::Session> {
    // TODO(F01): implement in Fase D.
    Err(agentyx_core::AppError::Internal {
        message: "session::create not yet implemented (F01 in Fase D)".into(),
    })
}

/// Send a message to the active session, starting a new run.
#[tauri::command]
pub async fn send(
    _state: State<'_, Arc<AppState>>,
    _session_id: SessionId,
    _content: String,
    _mentions: Vec<super::AtMention>,
) -> AppResult<RunHandle> {
    // TODO(F01): implement in Fase D.
    Err(agentyx_core::AppError::Internal {
        message: "session::send not yet implemented (F01 in Fase D)".into(),
    })
}

/// Abort the currently active run of a session, if any. Idempotent.
#[tauri::command]
pub async fn abort(
    _state: State<'_, Arc<AppState>>,
    _session_id: SessionId,
) -> AppResult<()> {
    // TODO(F01): implement in Fase D.
    Err(agentyx_core::AppError::Internal {
        message: "session::abort not yet implemented (F01 in Fase D)".into(),
    })
}

/// List sessions in a workspace, ordered by `updated_at DESC`.
#[tauri::command]
pub async fn list(
    _state: State<'_, Arc<AppState>>,
    _workspace_id: agentyx_core::ids::WorkspaceId,
    _limit: Option<u32>,
    _before: Option<agentyx_core::Ulid>,
) -> AppResult<Vec<SessionSummaryDto>> {
    // TODO(F01, F-agents-ui): implement in Fase D.
    Err(agentyx_core::AppError::Internal {
        message: "session::list not yet implemented (F-agents-ui in Fase D)".into(),
    })
}

/// Load the persisted history of a session (cold start, sidebar, etc.).
#[tauri::command]
pub async fn get_history(
    _state: State<'_, Arc<AppState>>,
    _session_id: SessionId,
    _limit: Option<u32>,
    _before: Option<agentyx_core::Ulid>,
) -> AppResult<Vec<agentyx_core::journal::JournalEntry>> {
    // TODO(F01): implement in Fase D.
    Err(agentyx_core::AppError::Internal {
        message: "session::get_history not yet implemented (F01 in Fase D)".into(),
    })
}

/// Change the active agent of a session. Blocked mid-run with `Conflict`.
#[tauri::command]
pub async fn set_active_agent(
    _state: State<'_, Arc<AppState>>,
    _session_id: SessionId,
    _agent_id: agentyx_core::ids::AgentId,
) -> AppResult<()> {
    // TODO(F01, F-agents-ui): implement in Fase D.
    Err(agentyx_core::AppError::Internal {
        message: "session::set_active_agent not yet implemented (F-agents-ui in Fase D)".into(),
    })
}

/// Get the currently active agent of a session.
#[tauri::command]
pub async fn get_active_agent(
    _state: State<'_, Arc<AppState>>,
    _session_id: SessionId,
) -> AppResult<agentyx_core::ids::AgentId> {
    // TODO(F01, F-agents-ui): implement in Fase D.
    Err(agentyx_core::AppError::Internal {
        message: "session::get_active_agent not yet implemented (F-agents-ui in Fase D)".into(),
    })
}
