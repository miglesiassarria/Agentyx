//! `agents` Tauri commands — multi-agent surface.
//!
//! See `../../../specs/agents.md` for the full model.

// Placeholder commands are not yet wired into `generate_handler!`;
// they are kept in the module so the surface is documented and the
// IPC contract is reviewable ahead of the Fase D implementation.
#![allow(dead_code)]

use agentyx_core::ids::AgentId;
use agentyx_core::AppResult;
use std::sync::Arc;
use tauri::State;

use crate::state::AppState;

/// List all visible (non-hidden) agents. The UI uses this for the
/// `AgentChip` picker, the `@mention` popover, and the system prompt
/// metadata.
#[tauri::command]
pub async fn list_agents(_state: State<'_, Arc<AppState>>) -> AppResult<Vec<serde_json::Value>> {
    Err(agentyx_core::AppError::Internal {
        message: "agents::list not yet implemented (agents.md in Fase D)".into(),
    })
}

/// Get a single agent by id (built-in or custom). Returns
/// `AppError::NotFound` if no agent with that id is registered.
#[tauri::command]
pub async fn get_agent(
    _state: State<'_, Arc<AppState>>,
    _id: AgentId,
) -> AppResult<serde_json::Value> {
    Err(agentyx_core::AppError::Internal {
        message: "agents::get not yet implemented (agents.md in Fase D)".into(),
    })
}
