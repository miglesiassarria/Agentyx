//! `session` Tauri commands — F01 chat surface.
//!
//! Wires the `agentyx_core::agent::spawn_run` loop and the
//! `SessionService` / `JournalRepo` from F01-Phase1 to the IPC
//! layer. The UI talks to these commands via `lib/ipc.ts`.
//!
//! See `../../../specs/features/F01-chat-streaming.md` and
//! `../../../specs/domains/agent-loop.md` for the contracts.

use std::sync::Arc;

use agentyx_core::agent::{
    spawn_run, AgentLoopDeps, EventSink, RunHandle as CoreRunHandle, StartOpts,
};
use agentyx_core::ids::{AgentId, RunId, SessionId, WorkspaceId};
use agentyx_core::session::{ListMessagesOpts, ListOpts as SessionListOpts};
use agentyx_core::AppResult;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::sink::TauriEventSink;
use crate::state::AppState;

// ============================================================
// DTOs (shapes that cross the IPC boundary)
// ============================================================

/// Handle returned by `session_send` to identify a running run.
/// The frontend listens for `chat.*.v1` events on this `runId`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunHandleDto {
    /// The run that was started.
    pub run_id: RunId,
    /// The session this run belongs to.
    pub session_id: SessionId,
    /// The active agent of the session at run start.
    pub agent_id: AgentId,
    /// ISO-8601 UTC timestamp of when the run was created.
    pub started_at: String,
}

/// DTO for a session summary in `session_list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummaryDto {
    /// The session id.
    pub id: SessionId,
    /// The workspace the session belongs to.
    pub workspace_id: WorkspaceId,
    /// The agent active for new runs.
    pub active_agent: AgentId,
    /// Display title (truncated first user message in v0.1).
    pub title: String,
    /// Wall-clock timestamp of last update (ISO-8601).
    pub updated_at: String,
    /// Session status (idle / running / aborted / errored).
    pub status: String,
}

/// DTO for a session message in `session_get_history`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageDto {
    /// Message id.
    pub id: String,
    /// Session id.
    pub session_id: SessionId,
    /// Run id (None for user messages and pre-run system).
    pub run_id: Option<RunId>,
    /// Role (user / assistant / system / tool_result).
    pub role: String,
    /// Plain-text content (or JSON for tool_result in Phase 2).
    pub content: String,
    /// Sequence number within the session (ASC).
    pub seq: i64,
    /// ISO-8601 UTC timestamp.
    pub created_at: String,
}

// ============================================================
// Commands
// ============================================================

/// Create a new session in a workspace.
///
/// If `agent_id` is `None`, defaults to the first `Primary` of
/// the agent registry. If `title` is `None`, the session will
/// be titled by the first user message (deferred to Phase 2;
/// for now, title is `Untitled`).
#[tauri::command]
pub async fn create_session(
    state: State<'_, Arc<AppState>>,
    workspace_id: WorkspaceId,
    agent_id: Option<AgentId>,
    title: Option<String>,
) -> AppResult<SessionSummaryDto> {
    let state = state.inner().clone();
    // Run sync DB work on a blocking thread (rusqlite is sync).
    let summary = tokio::task::spawn_blocking(move || {
        let runtime = state.workspace_runtime(workspace_id)?;
        let session = runtime.session.create(&state.agents, agent_id)?;
        Ok::<_, agentyx_core::AppError>(to_summary(&session, title))
    })
    .await
    .map_err(|e| agentyx_core::AppError::Internal {
        message: format!("join error: {e}"),
    })??;
    Ok(summary)
}

/// Send a message to a session, starting a new run.
///
/// The command returns immediately with a `RunHandleDto`. The
/// run executes asynchronously on a Tokio task; events
/// (`chat.run.started.v1`, `chat.content.delta.v1`,
/// `chat.run.finished.v1`, etc.) stream to the UI via
/// `TauriEventSink` (which delegates to the `EventBus`).
///
/// Phase 1 limitations:
/// - No `@mention` resolution (mentions are ignored).
/// - No tool calls (LLM tool_use events are logged and
///   discarded; the run produces a single assistant message).
/// - One active run per session (concurrent sends return
///   `Conflict`).
#[tauri::command]
pub async fn send(
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
    session_id: SessionId,
    content: String,
    _mentions: Vec<super::AtMention>,
) -> AppResult<RunHandleDto> {
    let state = state.inner().clone();
    let sink: Arc<dyn EventSink> = Arc::new(TauriEventSink::new(state.event_bus.clone(), app));

    // Locate the session's workspace and snapshot its session + journal services.
    let (workspace_id, session_svc, journal_svc) = tokio::task::spawn_blocking({
        let state = state.clone();
        move || -> AppResult<(
            WorkspaceId,
            agentyx_core::session::SessionService,
            agentyx_core::journal::JournalRepo,
        )> {
            for workspace in state.workspaces.list() {
                let rt = state.workspace_runtime(workspace.id)?;
                if rt.session.get(session_id).is_ok() {
                    return Ok((workspace.id, (*rt.session).clone(), (*rt.journal).clone()));
                }
            }
            Err(agentyx_core::AppError::NotFound {
                kind: "session".into(),
                id: session_id.to_string(),
            })
        }
    })
    .await
    .map_err(|e| agentyx_core::AppError::Internal {
        message: format!("join error: {e}"),
    })??;

    let _ = workspace_id;

    // Build the AgentLoopDeps and spawn the run.
    let deps = AgentLoopDeps {
        agents: (*state.agents).clone(),
        config: (*state.config).clone(),
        providers: state.providers.to_hashmap(),
        session: session_svc,
        journal: journal_svc,
        bus: sink,
        workspaces: (*state.workspaces).clone(),
        tool_registry: state.tool_registry.clone(),
        permission_gate: state.permission_gate.clone(),
        permission_registry: state.permission_registry.clone(),
    };

    let handle: CoreRunHandle = spawn_run(deps, session_id, content, StartOpts::default())?;
    let dto = RunHandleDto {
        run_id: handle.state().run_id,
        session_id: handle.state().session_id,
        agent_id: handle.state().agent_id,
        started_at: handle.state().started_at.to_rfc3339(),
    };
    state.runs.register(handle);
    Ok(dto)
}

/// Abort the currently active run of a session, if any. Idempotent.
#[tauri::command]
pub async fn abort(state: State<'_, Arc<AppState>>, session_id: SessionId) -> AppResult<()> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || -> AppResult<()> {
        // We don't track session -> run mapping explicitly; the
        // app can keep a UI-side pointer via `chat.run.started.v1`
        // events. For v0.1 we abort all running handles that
        // belong to the session. This is O(runs) but runs are
        // typically <10 active.
        for (_, handle) in state.runs.iter_for_session(session_id) {
            if handle.is_running() {
                handle.abort();
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| agentyx_core::AppError::Internal {
        message: format!("join error: {e}"),
    })??;
    Ok(())
}

/// List sessions in a workspace, ordered by `updated_at DESC`.
#[tauri::command]
pub async fn list_sessions(
    state: State<'_, Arc<AppState>>,
    workspace_id: WorkspaceId,
    limit: Option<u32>,
) -> AppResult<Vec<SessionSummaryDto>> {
    let state = state.inner().clone();
    let list = tokio::task::spawn_blocking(move || -> AppResult<Vec<SessionSummaryDto>> {
        let runtime = state.workspace_runtime(workspace_id)?;
        let sessions = runtime.session.list(SessionListOpts {
            limit,
            status: None,
        })?;
        Ok(sessions.iter().map(|s| to_summary(s, None)).collect())
    })
    .await
    .map_err(|e| agentyx_core::AppError::Internal {
        message: format!("join error: {e}"),
    })??;
    Ok(list)
}

/// Load the persisted history of a session (cold start, sidebar, etc.).
#[tauri::command]
pub async fn get_history(
    state: State<'_, Arc<AppState>>,
    session_id: SessionId,
    limit: Option<u32>,
) -> AppResult<Vec<MessageDto>> {
    let state = state.inner().clone();
    let list = tokio::task::spawn_blocking(move || -> AppResult<Vec<MessageDto>> {
        // Locate the session's workspace and its runtime.
        let (session_svc, workspace_id) = locate_session(&state, session_id)?;
        let _ = workspace_id;
        let messages = session_svc.list_messages(
            session_id,
            ListMessagesOpts {
                limit,
                after_seq: None,
            },
        )?;
        Ok(messages
            .iter()
            .map(|m| MessageDto {
                id: m.id.to_string(),
                session_id: m.session_id,
                run_id: m.run_id,
                role: m.role.as_str().to_string(),
                content: m.content.clone(),
                seq: m.seq,
                created_at: m.created_at.to_rfc3339(),
            })
            .collect())
    })
    .await
    .map_err(|e| agentyx_core::AppError::Internal {
        message: format!("join error: {e}"),
    })??;
    Ok(list)
}

/// Change the active agent of a session. Blocked mid-run with `Conflict`.
#[tauri::command]
pub async fn set_active_agent(
    state: State<'_, Arc<AppState>>,
    session_id: SessionId,
    agent_id: AgentId,
) -> AppResult<()> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || -> AppResult<()> {
        let (session_svc, _ws) = locate_session(&state, session_id)?;
        session_svc.set_active_agent(session_id, &state.agents, agent_id)
    })
    .await
    .map_err(|e| agentyx_core::AppError::Internal {
        message: format!("join error: {e}"),
    })??;
    Ok(())
}

/// Get the currently active agent of a session.
#[tauri::command]
pub async fn get_active_agent(
    state: State<'_, Arc<AppState>>,
    session_id: SessionId,
) -> AppResult<AgentId> {
    let state = state.inner().clone();
    let id = tokio::task::spawn_blocking(move || -> AppResult<AgentId> {
        let (session_svc, _ws) = locate_session(&state, session_id)?;
        session_svc.get_active_agent(session_id)
    })
    .await
    .map_err(|e| agentyx_core::AppError::Internal {
        message: format!("join error: {e}"),
    })??;
    Ok(id)
}

// ============================================================
// Helpers
// ============================================================

fn to_summary(
    session: &agentyx_core::session::Session,
    title_override: Option<String>,
) -> SessionSummaryDto {
    SessionSummaryDto {
        id: session.id,
        workspace_id: session.workspace_id,
        active_agent: session.active_agent_id,
        title: title_override
            .or_else(|| session.title.clone())
            .unwrap_or_else(|| "Untitled".to_string()),
        updated_at: session.updated_at.to_rfc3339(),
        status: session.status.as_str().to_string(),
    }
}

/// Locate the `SessionService` for a session id by scanning the
/// per-workspace runtimes cache (small, <10 workspaces typically).
/// Returns `(session_svc, workspace_id)`.
fn locate_session(
    state: &AppState,
    session_id: SessionId,
) -> AppResult<(Arc<agentyx_core::session::SessionService>, WorkspaceId)> {
    for workspace in state.workspaces.list() {
        let runtime = state.workspace_runtime(workspace.id)?;
        if runtime.session.get(session_id).is_ok() {
            return Ok((runtime.session.clone(), workspace.id));
        }
    }
    Err(agentyx_core::AppError::NotFound {
        kind: "session".to_string(),
        id: session_id.to_string(),
    })
}
