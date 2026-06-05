//! `workspace` Tauri commands — F02 multi-workspace surface.
//!
//! All command signatures are placeholders in v0.1; they will be
//! implemented in Fase D following the contracts in
//! `../../../specs/features/F02-multi-workspace.md`.

use agentyx_core::AppResult;
use agentyx_core::ids::{ExtraPathId, WorkspaceId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

use crate::state::AppState;

/// DTO for a workspace in the sidebar / settings list.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDto {
    /// The workspace id.
    pub id: WorkspaceId,
    /// Display name (defaults to folder basename).
    pub name: String,
    /// Absolute path to the workspace root.
    pub root_path: PathBuf,
    /// Extra paths the user has authorized (see ADR-0007).
    pub extra_paths: Vec<ExtraPathDto>,
    /// True if `.venv/` (or similar) was detected.
    pub has_venv: bool,
}

/// DTO for an extra path entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraPathDto {
    /// The entry id.
    pub id: ExtraPathId,
    /// Absolute path to the directory.
    pub path: PathBuf,
    /// Display label (defaults to the basename).
    pub label: String,
}

/// List all workspaces known to Agentyx.
#[tauri::command]
pub async fn list(
    _state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<WorkspaceDto>> {
    // TODO(F02): implement in Fase D.
    Err(agentyx_core::AppError::Internal {
        message: "workspace::list not yet implemented (F02 in Fase D)".into(),
    })
}

/// Open a workspace by its root path. Creates a new entry if it
/// doesn't exist; otherwise re-opens an existing one.
#[tauri::command]
pub async fn open(
    _state: State<'_, Arc<AppState>>,
    _root_path: PathBuf,
) -> AppResult<WorkspaceDto> {
    // TODO(F02): implement in Fase D.
    Err(agentyx_core::AppError::Internal {
        message: "workspace::open not yet implemented (F02 in Fase D)".into(),
    })
}

/// Remove a workspace from Agentyx. Does NOT delete any files
/// on disk; just forgets the entry and its state.db.
#[tauri::command]
pub async fn delete(
    _state: State<'_, Arc<AppState>>,
    _workspace_id: WorkspaceId,
) -> AppResult<()> {
    // TODO(F02): implement in Fase D.
    Err(agentyx_core::AppError::Internal {
        message: "workspace::delete not yet implemented (F02 in Fase D)".into(),
    })
}

/// Add an extra path to a workspace's sandbox. The path is
/// canonicalized and validated against the workspace root
/// (it can be a sibling directory; see ADR-0007).
#[tauri::command]
pub async fn add_extra_path(
    _state: State<'_, Arc<AppState>>,
    _workspace_id: WorkspaceId,
    _path: PathBuf,
) -> AppResult<ExtraPathDto> {
    // TODO(F02): implement in Fase D.
    Err(agentyx_core::AppError::Internal {
        message: "workspace::add_extra_path not yet implemented (F02 in Fase D)".into(),
    })
}

/// Remove an extra path from a workspace's sandbox. Does NOT
/// delete any files on disk.
#[tauri::command]
pub async fn remove_extra_path(
    _state: State<'_, Arc<AppState>>,
    _workspace_id: WorkspaceId,
    _extra_path_id: ExtraPathId,
) -> AppResult<()> {
    // TODO(F02): implement in Fase D.
    Err(agentyx_core::AppError::Internal {
        message: "workspace::remove_extra_path not yet implemented (F02 in Fase D)".into(),
    })
}
