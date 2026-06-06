//! `agents` Tauri commands — multi-agent surface.
//!
//! See `../../../specs/agents.md` for the full model.

use std::sync::Arc;

use agentyx_core::agents::AgentMode;
use agentyx_core::ids::AgentId;
use agentyx_core::AppResult;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

/// DTO returned by `agents_list` / `agents_get`. Mirrors the
/// subset of `AgentSpec` the UI needs (id, mode, hidden, name,
/// description, model).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfoDto {
    /// Agent id.
    pub id: AgentId,
    /// Mode: "primary" | "subagent" | "hidden".
    pub mode: String,
    /// Hidden agents don't appear in the UI.
    pub hidden: bool,
    /// Description (one-liner shown in tooltips / @mention).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Display name (defaults to id if not set).
    pub name: String,
}

/// List all visible (non-hidden) agents. The UI uses this for the
/// `AgentChip` picker, the `@mention` popover, and the system prompt
/// metadata.
#[tauri::command]
pub async fn list_agents(state: State<'_, Arc<AppState>>) -> AppResult<Vec<AgentInfoDto>> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || -> AppResult<Vec<AgentInfoDto>> {
        let agents = state
            .agents
            .list_visible()
            .into_iter()
            .map(to_dto)
            .collect();
        Ok(agents)
    })
    .await
    .map_err(|e| agentyx_core::AppError::Internal {
        message: format!("join error: {e}"),
    })?
}

/// Get a single agent by id (built-in or custom). Returns
/// `AppError::NotFound` if no agent with that id is registered.
#[tauri::command]
pub async fn get_agent(state: State<'_, Arc<AppState>>, id: AgentId) -> AppResult<AgentInfoDto> {
    let state = state.inner().clone();
    let dto = tokio::task::spawn_blocking(move || -> AppResult<AgentInfoDto> {
        let spec = state
            .agents
            .get(&id)
            .ok_or_else(|| agentyx_core::AppError::NotFound {
                kind: "agent".to_string(),
                id: id.to_string(),
            })?;
        Ok(to_dto(spec))
    })
    .await
    .map_err(|e| agentyx_core::AppError::Internal {
        message: format!("join error: {e}"),
    })??;
    Ok(dto)
}

fn to_dto(spec: &agentyx_core::agents::AgentSpec) -> AgentInfoDto {
    AgentInfoDto {
        id: spec.id,
        mode: match spec.mode {
            AgentMode::Primary => "primary".to_string(),
            AgentMode::Subagent => "subagent".to_string(),
            AgentMode::Hidden => "hidden".to_string(),
        },
        hidden: matches!(spec.mode, AgentMode::Hidden),
        description: spec.description.clone(),
        name: spec.id.to_string(),
    }
}
