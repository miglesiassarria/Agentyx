//! `config` Tauri commands — F05 settings surface.
//!
//! All command signatures are placeholders in v0.1; they will be
//! implemented in Fase D following the contracts in
//! `../../../specs/features/F05-settings.md` and
//! `../../../specs/domains/config.md`.

use agentyx_core::AppResult;
use agentyx_core::config::{GlobalConfig, WorkspaceConfig};
use agentyx_core::ids::WorkspaceId;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

use crate::state::AppState;

/// Get the global config (without secrets).
#[tauri::command]
pub async fn get_global(
    _state: State<'_, Arc<AppState>>,
) -> AppResult<GlobalConfig> {
    Err(agentyx_core::AppError::Internal {
        message: "config::get_global not yet implemented (F05 in Fase D)".into(),
    })
}

/// Patch and persist the global config.
#[tauri::command]
pub async fn update_global(
    _state: State<'_, Arc<AppState>>,
    _patch: serde_json::Value,
) -> AppResult<GlobalConfig> {
    Err(agentyx_core::AppError::Internal {
        message: "config::update_global not yet implemented (F05 in Fase D)".into(),
    })
}

/// Get a workspace's config overrides (if any).
#[tauri::command]
pub async fn get_workspace(
    _state: State<'_, Arc<AppState>>,
    _workspace_id: WorkspaceId,
) -> AppResult<WorkspaceConfig> {
    Err(agentyx_core::AppError::Internal {
        message: "config::get_workspace not yet implemented (F05 in Fase D)".into(),
    })
}

/// Patch and persist a workspace's config overrides.
#[tauri::command]
pub async fn update_workspace(
    _state: State<'_, Arc<AppState>>,
    _workspace_id: WorkspaceId,
    _patch: serde_json::Value,
) -> AppResult<WorkspaceConfig> {
    Err(agentyx_core::AppError::Internal {
        message: "config::update_workspace not yet implemented (F05 in Fase D)".into(),
    })
}
