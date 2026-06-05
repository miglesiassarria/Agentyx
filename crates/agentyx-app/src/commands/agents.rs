//! `agents` Tauri commands — multi-agent surface.
//!
//! See `../../../specs/agents.md` for the full model.

use agentyx_core::AppResult;
use agentyx_core::agents::AgentSpec;
use agentyx_core::ids::AgentId;
use std::sync::Arc;
use tauri::State;

use crate::state::AppState;

/// List all visible (non-hidden) agents. The UI uses this for the
/// `AgentChip` picker, the `@mention` popover, and the system prompt
/// metadata.
#[tauri::command]
pub async fn list(
    _state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<AgentSpec>> {
    Err(agentyx_core::AppError::Internal {
        message: "agents::list not yet implemented (agents.md in Fase D)".into(),
    })
}

/// Get a single agent by id (built-in or custom). Returns
/// `AppError::NotFound` if no agent with that id is registered.
#[tauri::command]
pub async fn get(
    _state: State<'_, Arc<AppState>>,
    _id: AgentId,
) -> AppResult<AgentSpec> {
    Err(agentyx_core::AppError::Internal {
        message: "agents::get not yet implemented (agents.md in Fase D)".into(),
    })
}
