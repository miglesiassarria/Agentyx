//! `permissions` Tauri commands — F01/F12 permission gate surface.

use agentyx_core::AppResult;
use agentyx_core::ids::{PermissionRequestId, ToolId, WorkspaceId};
use agentyx_core::permissions::{PermissionDecision, PermissionMatrixDto};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

use crate::state::AppState;

/// User's response to a permission prompt. Maps to the 4 buttons
/// in `PermissionPrompt.svelte`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PermissionResponse {
    /// Allow this call only.
    AllowOnce,
    /// Allow for the duration of this run.
    AllowSession,
    /// Allow forever (persists as the default decision in
    /// `GlobalConfig`; equivalent to editing the matrix).
    AllowAlways {
        /// The tool this applies to.
        tool: ToolId,
    },
    /// Deny.
    Deny,
}

/// Get the current permission matrix for a workspace (or global
/// if `workspace_id` is `None`).
#[tauri::command]
pub async fn get_matrix(
    _state: State<'_, Arc<AppState>>,
    _workspace_id: Option<WorkspaceId>,
) -> AppResult<PermissionMatrixDto> {
    Err(agentyx_core::AppError::Internal {
        message: "permissions::get_matrix not yet implemented (F01 in Fase D)".into(),
    })
}

/// Set the default decision for a tool globally.
#[tauri::command]
pub async fn set_default(
    _state: State<'_, Arc<AppState>>,
    _tool: ToolId,
    _decision: PermissionDecision,
) -> AppResult<()> {
    Err(agentyx_core::AppError::Internal {
        message: "permissions::set_default not yet implemented (F05 in Fase D)".into(),
    })
}

/// Respond to a permission request from the agent loop.
#[tauri::command]
pub async fn respond(
    _state: State<'_, Arc<AppState>>,
    _request_id: PermissionRequestId,
    _response: PermissionResponse,
) -> AppResult<()> {
    Err(agentyx_core::AppError::Internal {
        message: "permissions::respond not yet implemented (F01 in Fase D)".into(),
    })
}
