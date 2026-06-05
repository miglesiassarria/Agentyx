//! `workspace` Tauri commands — F02 multi-workspace surface.
//!
//! Each command is a thin wrapper around an inner function that
//! takes `&AppState` (or `&WorkspaceService`) and does the real
//! work. The inner functions are unit-testable without a Tauri
//! runtime; the wrappers handle event emission and the
//! `State<'_, Arc<AppState>>` / `AppHandle` extraction.
//!
//! See `../../../specs/features/F02-multi-workspace.md` and
//! `../../../specs/domains/workspace.md` for the contracts.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentyx_core::AppResult;
use agentyx_core::ids::WorkspaceId;
use agentyx_core::workspace::{detect_venv, VenvSpec, Workspace, WorkspaceService};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, State};

use crate::events::EventBus;
use crate::state::AppState;

// ============================================================
// DTOs (the shapes that cross the IPC boundary)
// ============================================================

/// DTO for a workspace in the sidebar / settings list.
///
/// Mirrors the `Workspace` domain type but uses string-shaped
/// fields and a derived `has_venv` flag (computed on conversion).
/// The `id` is the workspace's `WorkspaceId` (a ULID, serialized
/// as a 26-char string). The `path` is canonical.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDto {
    /// The workspace id.
    pub id: WorkspaceId,
    /// Display name (defaults to folder basename on open).
    pub name: String,
    /// Canonical absolute path of the workspace root.
    pub root_path: PathBuf,
    /// Extra paths the user has authorized (see ADR-0007).
    pub extra_paths: Vec<ExtraPathDto>,
    /// True if `.venv/` (or similar) was detected at conversion
    /// time. Recomputed each call; cheap (file existence checks).
    pub has_venv: bool,
}

/// DTO for a single extra path entry.
///
/// The `id` is the entry's path as a string (used by the UI as
/// the React key in `each` blocks). The `label` defaults to the
/// folder basename if the domain `label` is `None`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraPathDto {
    /// Canonical absolute path (also serves as the unique id).
    pub id: String,
    /// Absolute path of the directory.
    pub path: PathBuf,
    /// Display label (defaults to folder basename).
    pub label: String,
    /// Epoch ms when the entry was added.
    pub added_at: i64,
}

/// DTO for the effective paths the agent can operate on.
/// Returned by `workspace_effective_paths`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectivePathsDto {
    /// Canonical root path.
    pub root: PathBuf,
    /// Canonical extra paths, in `added_at` order.
    pub extras: Vec<PathBuf>,
}

// ============================================================
// DTO conversions
// ============================================================

fn workspace_to_dto(w: Workspace) -> WorkspaceDto {
    let has_venv = detect_venv(&w.root_path, None)
        .ok()
        .flatten()
        .is_some();
    WorkspaceDto {
        id: w.id,
        name: w.name,
        root_path: w.root_path,
        extra_paths: w.extra_paths.into_iter().map(extra_path_to_dto).collect(),
        has_venv,
    }
}

fn extra_path_to_dto(ep: agentyx_core::workspace::ExtraPath) -> ExtraPathDto {
    let label = ep.label.clone().unwrap_or_else(|| {
        ep.path
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_string)
            .unwrap_or_default()
    });
    ExtraPathDto {
        id: ep.path.to_string_lossy().to_string(),
        path: ep.path,
        label,
        added_at: ep.added_at,
    }
}

// ============================================================
// Inner functions (testable; no Tauri deps)
// ============================================================

/// List all workspaces known to Agentyx, ordered by
/// `last_opened_at DESC`. Each is converted to a `WorkspaceDto`
/// (computes `has_venv` on the fly).
#[allow(clippy::unused_async)] // async for future-proofing
pub(crate) async fn list_impl(svc: &WorkspaceService) -> AppResult<Vec<WorkspaceDto>> {
    Ok(svc.list().into_iter().map(workspace_to_dto).collect())
}

/// Open (or re-open) a workspace at `root_path`.
#[allow(clippy::unused_async)]
pub(crate) async fn open_impl(
    svc: &WorkspaceService,
    root_path: &Path,
    name: Option<String>,
) -> AppResult<WorkspaceDto> {
    let opts = agentyx_core::workspace::OpenOptions { name };
    let ws = svc.open(root_path, opts)?;
    Ok(workspace_to_dto(ws))
}

/// Look up a workspace by id. Returns `AppError::NotFound` if
/// the id is not in the registry.
#[allow(clippy::unused_async)]
pub(crate) async fn get_impl(svc: &WorkspaceService, id: WorkspaceId) -> AppResult<WorkspaceDto> {
    svc.get(id)
        .map(workspace_to_dto)
        .ok_or(agentyx_core::AppError::NotFound {
            kind: "workspace".into(),
            id: id.to_string(),
        })
}

/// Remove a workspace. With `force=true`, aborts any active runs
/// first (deferred to the agent-loop PR). With `force=false`,
/// refuses if there are active runs (also deferred; for now we
/// just always allow).
#[allow(clippy::unused_async)]
pub(crate) async fn delete_impl(
    svc: &WorkspaceService,
    id: WorkspaceId,
    _force: bool,
) -> AppResult<()> {
    svc.delete(id, _force)
}

/// Detect a venv for the workspace. Returns `Ok(None)` if no
/// venv is found. Reads the per-workspace config.toml for the
/// `venv.path` override and falls back to auto-detection
/// (ADR-0004) per workspace.md §Operations.
#[allow(clippy::unused_async)]
pub(crate) async fn detect_venv_impl(
    svc: &WorkspaceService,
    id: WorkspaceId,
) -> AppResult<Option<VenvSpec>> {
    let ws = svc.get(id).ok_or(agentyx_core::AppError::NotFound {
        kind: "workspace".into(),
        id: id.to_string(),
    })?;
    let config_override = read_venv_path_override(svc, id).ok().flatten();
    detect_venv(&ws.root_path, config_override.as_deref())
}

/// Add an extra path to a workspace. Persists in `state.json` and
/// `config.toml` (the `state.db` write lands with the storage PR).
/// Returns the new `ExtraPathDto`.
#[allow(clippy::unused_async)]
pub(crate) async fn add_extra_path_impl(
    svc: &WorkspaceService,
    id: WorkspaceId,
    path: &Path,
    label: Option<String>,
) -> AppResult<ExtraPathDto> {
    let extra = svc.add_extra_path(id, path, label)?;
    Ok(extra_path_to_dto(extra))
}

/// Remove an extra path from a workspace.
#[allow(clippy::unused_async)]
pub(crate) async fn remove_extra_path_impl(
    svc: &WorkspaceService,
    id: WorkspaceId,
    path: &Path,
) -> AppResult<()> {
    svc.remove_extra_path(id, path)
}

/// List a workspace's extra paths, ordered by `added_at ASC`.
#[allow(clippy::unused_async)]
pub(crate) async fn list_extra_paths_impl(
    svc: &WorkspaceService,
    id: WorkspaceId,
) -> AppResult<Vec<ExtraPathDto>> {
    Ok(svc
        .list_extra_paths(id)
        .into_iter()
        .map(extra_path_to_dto)
        .collect())
}

/// Effective paths the agent can operate on. The result is the
/// sandbox = `root_path ∪ extra_paths` per ADR-0007.
#[allow(clippy::unused_async)]
pub(crate) async fn effective_paths_impl(
    svc: &WorkspaceService,
    id: WorkspaceId,
) -> AppResult<EffectivePathsDto> {
    svc.effective_paths(id)
        .map(|p| EffectivePathsDto {
            root: p.root,
            extras: p.extras,
        })
        .ok_or(agentyx_core::AppError::NotFound {
            kind: "workspace".into(),
            id: id.to_string(),
        })
}

/// Read the `venv.path` override from the workspace's `config.toml`,
/// if any. Returns `Ok(None)` if the config doesn't exist or has
/// no `venv` block.
fn read_venv_path_override(
    svc: &WorkspaceService,
    id: WorkspaceId,
) -> AppResult<Option<PathBuf>> {
    let ws_dir = svc.agentyx_home().join("workspaces").join(id.to_string());
    let path = ws_dir.join("config.toml");
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).map_err(|e| agentyx_core::AppError::Io {
        op: format!("read {}", path.display()),
        source: e.to_string(),
    })?;
    let cfg: agentyx_core::workspace::WorkspaceConfig =
        toml::from_slice(&bytes).map_err(|e| agentyx_core::AppError::Internal {
            message: format!("config.toml is malformed: {e}"),
        })?;
    Ok(cfg
        .venv
        .as_ref()
        .map(|v| v.path.clone())
        .filter(|p| !p.as_os_str().is_empty()))
}

// ============================================================
// Event emission helpers
// ============================================================

/// Emit the `workspace.extra_path_added.v1` event. Called from
/// the Tauri command wrapper after a successful add.
fn emit_extra_path_added(bus: &EventBus, app: &AppHandle, ws_id: WorkspaceId, dto: &ExtraPathDto) {
    let payload = json!({
        "workspaceId": ws_id,
        "path": dto.path,
        "label": dto.label,
    });
    bus.emit(app, "workspace.extra_path_added.v1", payload);
}

/// Emit the `workspace.extra_path_removed.v1` event.
fn emit_extra_path_removed(bus: &EventBus, app: &AppHandle, ws_id: WorkspaceId, path: &Path) {
    let payload = json!({
        "workspaceId": ws_id,
        "path": path,
    });
    bus.emit(app, "workspace.extra_path_removed.v1", payload);
}

// ============================================================
// Tauri command wrappers
// ============================================================

/// List all workspaces. Each is converted to a `WorkspaceDto`.
#[tauri::command]
pub async fn list(state: State<'_, Arc<AppState>>) -> AppResult<Vec<WorkspaceDto>> {
    list_impl(&state.workspaces).await
}

/// Open a workspace. If the canonical `root_path` is already
/// registered, returns the existing workspace (idempotent).
#[tauri::command]
pub async fn open(
    state: State<'_, Arc<AppState>>,
    root_path: PathBuf,
    name: Option<String>,
) -> AppResult<WorkspaceDto> {
    open_impl(&state.workspaces, &root_path, name).await
}

/// Look up a workspace by id.
#[tauri::command]
pub async fn get(
    state: State<'_, Arc<AppState>>,
    workspace_id: WorkspaceId,
) -> AppResult<WorkspaceDto> {
    get_impl(&state.workspaces, workspace_id).await
}

/// Remove a workspace. Does NOT delete files on disk. With
/// `force=true`, aborts any active runs first (deferred to the
/// agent-loop PR; currently a no-op for the abort).
#[tauri::command]
pub async fn delete(
    state: State<'_, Arc<AppState>>,
    workspace_id: WorkspaceId,
    force: bool,
) -> AppResult<()> {
    delete_impl(&state.workspaces, workspace_id, force).await
}

/// Detect a venv for the workspace. Returns `Ok(None)` if no
/// venv is found. Cheap (file existence checks only).
#[tauri::command]
pub async fn detect_venv(
    state: State<'_, Arc<AppState>>,
    workspace_id: WorkspaceId,
) -> AppResult<Option<VenvSpec>> {
    detect_venv_impl(&state.workspaces, workspace_id).await
}

/// Add an extra path. Emits `workspace.extra_path_added.v1` on
/// success. Per ADR-0007.
#[tauri::command]
pub async fn add_extra_path(
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
    workspace_id: WorkspaceId,
    path: PathBuf,
    label: Option<String>,
) -> AppResult<ExtraPathDto> {
    let dto = add_extra_path_impl(&state.workspaces, workspace_id, &path, label).await?;
    emit_extra_path_added(&state.event_bus, &app, workspace_id, &dto);
    Ok(dto)
}

/// Remove an extra path. Emits `workspace.extra_path_removed.v1`
/// on success.
#[tauri::command]
pub async fn remove_extra_path(
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
    workspace_id: WorkspaceId,
    path: PathBuf,
) -> AppResult<()> {
    remove_extra_path_impl(&state.workspaces, workspace_id, &path).await?;
    emit_extra_path_removed(&state.event_bus, &app, workspace_id, &path);
    Ok(())
}

/// List a workspace's extra paths, ordered by `added_at ASC`.
#[tauri::command]
pub async fn list_extra_paths(
    state: State<'_, Arc<AppState>>,
    workspace_id: WorkspaceId,
) -> AppResult<Vec<ExtraPathDto>> {
    list_extra_paths_impl(&state.workspaces, workspace_id).await
}

/// Effective paths the agent can operate on
/// (`root_path ∪ extra_paths`).
#[tauri::command]
pub async fn effective_paths(
    state: State<'_, Arc<AppState>>,
    workspace_id: WorkspaceId,
) -> AppResult<EffectivePathsDto> {
    effective_paths_impl(&state.workspaces, workspace_id).await
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use agentyx_core::workspace::{ExtraPath, OpenOptions, Workspace, WorkspaceConfig};

    /// Build a fresh `AppState` rooted at a temp `~/.agentyx/`.
    async fn fresh_state() -> (tempfile::TempDir, Arc<AppState>) {
        let home = tempfile::tempdir().unwrap();
        let svc = Arc::new(WorkspaceService::new(home.path()).unwrap());
        let state = Arc::new(AppState {
            agentyx_home: home.path().to_path_buf(),
            workspaces: svc,
            event_bus: Arc::new(EventBus::new()),
        });
        (home, state)
    }

    fn make_workspace(state: &AppState, label: &str) -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws = state
            .workspaces
            .open(dir.path(), OpenOptions { name: Some(label.into()) })
            .unwrap();
        (dir, ws)
    }

    #[tokio::test]
    async fn list_impl_empty_registry() {
        let (_home, state) = fresh_state().await;
        let out = list_impl(&state.workspaces).await.unwrap();
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn list_impl_returns_workspaces_with_extras() {
        let (_home, state) = fresh_state().await;
        let (root, ws) = make_workspace(&state, "main");
        let extra = tempfile::tempdir().unwrap();
        let _ = state
            .workspaces
            .add_extra_path(ws.id, extra.path(), Some("assets".into()))
            .unwrap();

        let out = list_impl(&state.workspaces).await.unwrap();
        assert_eq!(out.len(), 1);

        let dto = &out[0];
        assert_eq!(dto.id, ws.id);
        assert_eq!(dto.name, "main");
        assert_eq!(dto.root_path, agentyx_core::workspace::canonicalize(root.path()).unwrap());
        assert_eq!(dto.extra_paths.len(), 1);
        assert_eq!(dto.extra_paths[0].label, "assets");
        assert_eq!(dto.extra_paths[0].path, agentyx_core::workspace::canonicalize(extra.path()).unwrap());
    }

    #[tokio::test]
    async fn list_impl_does_not_auto_detect_venv_for_clean_workspace() {
        // A workspace without .venv → has_venv is false.
        let (_home, state) = fresh_state().await;
        let (_root, _ws) = make_workspace(&state, "main");
        let out = list_impl(&state.workspaces).await.unwrap();
        assert!(!out[0].has_venv);
    }

    #[tokio::test]
    async fn list_impl_detects_dotvenv_when_present() {
        // A workspace with .venv → has_venv is true.
        let (_home, state) = fresh_state().await;
        let (root, _ws) = make_workspace(&state, "main");
        let venv = root.join(".venv");
        let bin = venv.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("python"), "#!/bin/sh").unwrap();

        let out = list_impl(&state.workspaces).await.unwrap();
        assert!(out[0].has_venv);
    }

    #[tokio::test]
    async fn open_impl_round_trip() {
        let (_home, state) = fresh_state().await;
        let dir = tempfile::tempdir().unwrap();
        let out = open_impl(
            &state.workspaces,
            dir.path(),
            Some("myproj".into()),
        )
        .await
        .unwrap();
        assert_eq!(out.name, "myproj");

        // Re-open is idempotent.
        let out2 = open_impl(&state.workspaces, dir.path(), None).await.unwrap();
        assert_eq!(out.id, out2.id);
    }

    #[tokio::test]
    async fn get_impl_found_and_not_found() {
        let (_home, state) = fresh_state().await;
        let (_root, ws) = make_workspace(&state, "main");

        let out = get_impl(&state.workspaces, ws.id).await.unwrap();
        assert_eq!(out.id, ws.id);

        let missing = WorkspaceId::new();
        let err = get_impl(&state.workspaces, missing).await.unwrap_err();
        assert!(matches!(err, agentyx_core::AppError::NotFound { .. }));
    }

    #[tokio::test]
    async fn delete_impl_removes_workspace() {
        let (_home, state) = fresh_state().await;
        let (_root, ws) = make_workspace(&state, "main");

        delete_impl(&state.workspaces, ws.id, false).await.unwrap();

        let out = list_impl(&state.workspaces).await.unwrap();
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn detect_venv_impl_returns_none_for_clean_workspace() {
        let (_home, state) = fresh_state().await;
        let (_root, ws) = make_workspace(&state, "main");
        let out = detect_venv_impl(&state.workspaces, ws.id).await.unwrap();
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn detect_venv_impl_finds_dotvenv() {
        let (_home, state) = fresh_state().await;
        let (root, ws) = make_workspace(&state, "main");
        let venv = root.join(".venv");
        let bin = venv.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("python"), "#!/bin/sh").unwrap();

        let out = detect_venv_impl(&state.workspaces, ws.id)
            .await
            .unwrap()
            .expect("should detect");
        assert_eq!(out.kind, agentyx_core::workspace::VenvKind::Uv);
    }

    #[tokio::test]
    async fn add_extra_path_impl_persists_and_returns_dto() {
        let (_home, state) = fresh_state().await;
        let (_root, ws) = make_workspace(&state, "main");
        let extra = tempfile::tempdir().unwrap();

        let dto = add_extra_path_impl(
            &state.workspaces,
            ws.id,
            extra.path(),
            Some("Assets".into()),
        )
        .await
        .unwrap();

        assert_eq!(dto.label, "Assets");
        assert_eq!(dto.id, agentyx_core::workspace::canonicalize(extra.path()).unwrap().to_string_lossy());

        // Persisted (re-list should show it).
        let listed = list_extra_paths_impl(&state.workspaces, ws.id).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, dto.id);
    }

    #[tokio::test]
    async fn add_extra_path_impl_defaults_label_to_basename() {
        let (_home, state) = fresh_state().await;
        let (_root, ws) = make_workspace(&state, "main");
        let extra_dir = tempfile::tempdir().unwrap();
        // Create a uniquely-named subdir so we can assert the basename.
        let sub = extra_dir.path().join("my-cool-extras");
        std::fs::create_dir(&sub).unwrap();

        let dto = add_extra_path_impl(&state.workspaces, ws.id, &sub, None)
            .await
            .unwrap();
        assert_eq!(dto.label, "my-cool-extras");
    }

    #[tokio::test]
    async fn remove_extra_path_impl_persists() {
        let (_home, state) = fresh_state().await;
        let (_root, ws) = make_workspace(&state, "main");
        let extra = tempfile::tempdir().unwrap();
        let dto = add_extra_path_impl(&state.workspaces, ws.id, extra.path(), None)
            .await
            .unwrap();

        remove_extra_path_impl(&state.workspaces, ws.id, &dto.path)
            .await
            .unwrap();

        let listed = list_extra_paths_impl(&state.workspaces, ws.id).await.unwrap();
        assert!(listed.is_empty());
    }

    #[tokio::test]
    async fn list_extra_paths_impl_orders_by_added_at() {
        let (_home, state) = fresh_state().await;
        let (_root, ws) = make_workspace(&state, "main");
        let e1 = tempfile::tempdir().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let e2 = tempfile::tempdir().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let e3 = tempfile::tempdir().unwrap();

        add_extra_path_impl(&state.workspaces, ws.id, e1.path(), None)
            .await
            .unwrap();
        add_extra_path_impl(&state.workspaces, ws.id, e2.path(), None)
            .await
            .unwrap();
        add_extra_path_impl(&state.workspaces, ws.id, e3.path(), None)
            .await
            .unwrap();

        let listed = list_extra_paths_impl(&state.workspaces, ws.id).await.unwrap();
        assert_eq!(listed.len(), 3);
        assert_eq!(listed[0].id, agentyx_core::workspace::canonicalize(e1.path()).unwrap().to_string_lossy());
        assert_eq!(listed[2].id, agentyx_core::workspace::canonicalize(e3.path()).unwrap().to_string_lossy());
    }

    #[tokio::test]
    async fn effective_paths_impl_returns_root_and_extras() {
        let (_home, state) = fresh_state().await;
        let (root, ws) = make_workspace(&state, "main");
        let extra = tempfile::tempdir().unwrap();
        add_extra_path_impl(&state.workspaces, ws.id, extra.path(), None)
            .await
            .unwrap();

        let eff = effective_paths_impl(&state.workspaces, ws.id).await.unwrap();
        assert_eq!(eff.root, agentyx_core::workspace::canonicalize(root.path()).unwrap());
        assert_eq!(eff.extras.len(), 1);
        assert_eq!(
            eff.extras[0],
            agentyx_core::workspace::canonicalize(extra.path()).unwrap()
        );
    }

    #[tokio::test]
    async fn effective_paths_impl_unknown_workspace_returns_not_found() {
        let (_home, state) = fresh_state().await;
        let err = effective_paths_impl(&state.workspaces, WorkspaceId::new())
            .await
            .unwrap_err();
        assert!(matches!(err, agentyx_core::AppError::NotFound { .. }));
    }

    // ----------------------------------------------------------
    // DTO conversion
    // ----------------------------------------------------------

    #[test]
    fn extra_path_to_dto_uses_path_as_id_and_defaults_label() {
        let dir = tempfile::tempdir().unwrap();
        let canonical = agentyx_core::workspace::canonicalize(dir.path()).unwrap();
        let ep = ExtraPath {
            path: canonical.clone(),
            label: None,
            added_at: 12345,
        };
        let dto = extra_path_to_dto(ep);
        // id is the canonical path as string.
        assert_eq!(dto.id, canonical.to_string_lossy());
        // label falls back to the folder basename.
        assert_eq!(dto.label, dir.path().file_name().unwrap().to_str().unwrap());
    }

    #[test]
    fn extra_path_to_dto_preserves_explicit_label() {
        let dir = tempfile::tempdir().unwrap();
        let canonical = agentyx_core::workspace::canonicalize(dir.path()).unwrap();
        let ep = ExtraPath {
            path: canonical,
            label: Some("Custom".into()),
            added_at: 0,
        };
        let dto = extra_path_to_dto(ep);
        assert_eq!(dto.label, "Custom");
    }

    #[test]
    fn workspace_to_dto_runs_detect_venv() {
        let dir = tempfile::tempdir().unwrap();
        // Make a workspace without a venv.
        let canonical = agentyx_core::workspace::canonicalize(dir.path()).unwrap();
        let ws = Workspace {
            id: WorkspaceId::new(),
            root_path: canonical,
            name: "x".into(),
            created_at: 0,
            last_opened_at: 0,
            extra_paths: vec![],
        };
        let dto = workspace_to_dto(ws);
        assert!(!dto.has_venv);

        // Now add a venv and convert again.
        let venv = dir.path().join(".venv");
        let bin = venv.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("python"), "#!/bin/sh").unwrap();

        let ws2 = Workspace {
            id: WorkspaceId::new(),
            root_path: agentyx_core::workspace::canonicalize(dir.path()).unwrap(),
            name: "x".into(),
            created_at: 0,
            last_opened_at: 0,
            extra_paths: vec![],
        };
        let dto2 = workspace_to_dto(ws2);
        assert!(dto2.has_venv);
    }

    // Suppress unused warning for the unused-but-imported type
    // (kept in case future tests need it).
    #[allow(dead_code)]
    fn _ensure_config_imported(_: WorkspaceConfig) {}
}
