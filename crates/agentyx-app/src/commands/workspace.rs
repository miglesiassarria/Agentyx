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

use agentyx_core::agent::RunRegistry;
use agentyx_core::ids::WorkspaceId;
use agentyx_core::workspace::{detect_venv, VenvSpec, Workspace, WorkspaceService};
use agentyx_core::AppResult;
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

/// DTO for a single directory entry returned by `workspace_list_dir`.
///
/// Consumed by the UI `FileTree` component. The `path` is canonical
/// and guaranteed to be within the workspace's effective paths
/// (root ∪ extras). Symlinks are reported via `is_symlink` so the
/// UI can render a different icon, but the `is_dir` flag reflects
/// the symlink's *target*, not the link itself (per Unix
/// `metadata()` semantics). Callers that need to refuse loops
/// must canonicalize and re-check on traversal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntryDto {
    /// Basename (no path separators).
    pub name: String,
    /// Absolute canonical path of this entry.
    pub path: PathBuf,
    /// Whether the entry is a directory (resolved through symlinks).
    pub is_dir: bool,
    /// Whether the entry is itself a symbolic link.
    pub is_symlink: bool,
    /// File size in bytes (0 for directories and on stat failure).
    #[serde(default)]
    pub size: u64,
    /// Last-modified time in epoch milliseconds (0 if unavailable).
    #[serde(default)]
    pub modified_at: i64,
}

// ============================================================
// DTO conversions
// ============================================================

fn workspace_to_dto(w: Workspace) -> WorkspaceDto {
    let has_venv = detect_venv(&w.root_path, None).ok().flatten().is_some();
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

/// Remove a workspace. F02.AC7: refuses with `Conflict` if the
/// workspace has any `Running` runs and `force=false`. With
/// `force=true`, aborts each running run first (the run finishes
/// asynchronously as the agent loop observes the abort flag) and
/// then proceeds to delete. The `RunRegistry` is consulted
/// directly; the per-workspace `SessionService` is updated
/// lazily by the next `workspace_runtime` open after eviction.
///
/// **Errors**:
/// - `not_found` — workspace id is unknown.
/// - `conflict` — workspace has running runs and `force=false`.
///   The `context` (when serialized in debug) carries the count.
#[allow(clippy::unused_async)]
pub(crate) async fn delete_impl(
    svc: &WorkspaceService,
    runs: &RunRegistry,
    id: WorkspaceId,
    force: bool,
) -> AppResult<()> {
    // F02.AC7: refuse delete when the workspace has active runs
    // unless the caller explicitly opts into `force`. We use
    // `iter_for_workspace` + a fresh `is_running()` check to
    // avoid a TOCTOU race (the run may finish between the
    // snapshot and the delete; that is fine, it just means
    // `force=false` may succeed in that window).
    let active: Vec<_> = runs
        .iter_for_workspace(id)
        .into_iter()
        .filter(|(_, h)| h.is_running())
        .collect();
    if !active.is_empty() {
        if !force {
            let count = active.len();
            tracing::warn!(
                workspace_id = %id,
                active_runs = count,
                "refusing to delete workspace with active runs"
            );
            return Err(agentyx_core::AppError::Conflict {
                message: format!(
                    "workspace has {count} active run{}; abort it or retry with force=true",
                    if count == 1 { "" } else { "s" }
                ),
            });
        }
        // `force=true`: request abort on each running run. The
        // loop checks the flag between deltas and finishes
        // within ~100ms (per agent-loop.md §AC5), so we don't
        // block here.
        for (run_id, handle) in &active {
            tracing::info!(
                workspace_id = %id,
                run_id = %run_id,
                "force=true: aborting active run before workspace delete"
            );
            handle.abort();
        }
    }
    svc.delete(id, force)
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

/// List the entries of a directory inside the workspace's sandbox
/// (root ∪ extras per ADR-0007). Returns entries sorted with
/// directories first, then files, both groups alphabetically
/// (case-insensitive). Hidden entries (basename starts with `.`)
/// are included — the UI `FileTree` filters them when the user
/// toggles the "show hidden" affordance.
///
/// **Errors**:
/// - `not_found` — workspace id is unknown.
/// - `path_outside_workspace` — `path` is not within the workspace
///   sandbox (after canonicalization).
/// - `io` — the path does not exist, is not a directory, or cannot
///   be read.
#[allow(clippy::unused_async)]
pub(crate) async fn list_dir_impl(
    svc: &WorkspaceService,
    id: WorkspaceId,
    path: &Path,
) -> AppResult<Vec<FileEntryDto>> {
    let eff = svc
        .effective_paths(id)
        .ok_or(agentyx_core::AppError::NotFound {
            kind: "workspace".into(),
            id: id.to_string(),
        })?;
    let canonical = agentyx_core::workspace::canonicalize(path)?;
    if !eff.contains(&canonical) {
        return Err(agentyx_core::AppError::PathOutsideWorkspace {
            path: canonical.to_string_lossy().to_string(),
        });
    }
    let meta = std::fs::metadata(&canonical).map_err(|e| agentyx_core::AppError::Io {
        op: format!("stat {}", canonical.display()),
        reason: e.to_string(),
    })?;
    if !meta.is_dir() {
        return Err(agentyx_core::AppError::InvalidInput {
            message: format!("not a directory: {}", canonical.display()),
        });
    }
    let read = std::fs::read_dir(&canonical).map_err(|e| agentyx_core::AppError::Io {
        op: format!("read_dir {}", canonical.display()),
        reason: e.to_string(),
    })?;
    let mut entries: Vec<FileEntryDto> = read
        .filter_map(Result::ok)
        .filter_map(|ent| {
            let name = ent.file_name().to_string_lossy().to_string();
            if name.is_empty() {
                return None;
            }
            let path = ent.path();
            let file_meta = ent.metadata().ok();
            let is_symlink = file_meta.as_ref().is_some_and(|m| m.is_symlink());
            let (is_dir, size, modified_at) = match file_meta {
                Some(m) => {
                    let resolved = if is_symlink {
                        std::fs::metadata(&path).ok()
                    } else {
                        Some(m.clone())
                    };
                    match resolved {
                        Some(rm) => (
                            rm.is_dir(),
                            if rm.is_dir() { 0 } else { rm.len() },
                            rm.modified()
                                .ok()
                                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                .map_or(0, |d| d.as_millis() as i64),
                        ),
                        None => (false, 0, 0),
                    }
                }
                None => (false, 0, 0),
            };
            Some(FileEntryDto {
                name,
                path,
                is_dir,
                is_symlink,
                size,
                modified_at,
            })
        })
        .collect();
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    Ok(entries)
}

/// Read the `venv.path` override from the workspace's `config.toml`,
/// if any. Returns `Ok(None)` if the config doesn't exist or has
/// no `venv` block.
fn read_venv_path_override(svc: &WorkspaceService, id: WorkspaceId) -> AppResult<Option<PathBuf>> {
    let ws_dir = svc.agentyx_home().join("workspaces").join(id.to_string());
    let path = ws_dir.join("config.toml");
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).map_err(|e| agentyx_core::AppError::Io {
        op: format!("read {}", path.display()),
        reason: e.to_string(),
    })?;
    let cfg: agentyx_core::workspace::WorkspaceConfig =
        toml::from_str(&text).map_err(|e| agentyx_core::AppError::Internal {
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
pub async fn list_workspaces(state: State<'_, Arc<AppState>>) -> AppResult<Vec<WorkspaceDto>> {
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
pub async fn get_workspace(
    state: State<'_, Arc<AppState>>,
    workspace_id: WorkspaceId,
) -> AppResult<WorkspaceDto> {
    get_impl(&state.workspaces, workspace_id).await
}

/// Remove a workspace. Does NOT delete files on disk. With
/// `force=true`, aborts any active runs first (the runs finish
/// asynchronously). With `force=false`, refuses with `Conflict`
/// if the workspace has any running sessions (F02.AC7).
#[tauri::command]
pub async fn delete_workspace(
    state: State<'_, Arc<AppState>>,
    workspace_id: WorkspaceId,
    force: bool,
) -> AppResult<()> {
    delete_impl(&state.workspaces, &state.runs, workspace_id, force).await?;
    // Drop the cached runtime so subsequent commands don't reuse
    // a dangling session/journal pointing at a deleted db file.
    state.evict_workspace_runtime(workspace_id);
    Ok(())
}

/// Detect a venv for the workspace. Returns `Ok(None)` if no
/// venv is found. Cheap (file existence checks only).
#[tauri::command]
pub async fn detect_workspace_venv(
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

/// List a directory's entries within the workspace sandbox.
/// The path must be canonical and within `root_path ∪ extra_paths`.
/// See [`list_dir_impl`] for error semantics.
#[tauri::command]
pub async fn list_dir(
    state: State<'_, Arc<AppState>>,
    workspace_id: WorkspaceId,
    path: PathBuf,
) -> AppResult<Vec<FileEntryDto>> {
    list_dir_impl(&state.workspaces, workspace_id, &path).await
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
        let config = Arc::new(
            agentyx_core::config::ConfigService::load(
                &agentyx_core::config::ServiceConfigPaths::from_agentyx_home(home.path()),
            )
            .unwrap(),
        );
        let agents = Arc::new(agentyx_core::agents::AgentRegistry::load_builtins());
        let providers = Arc::new(crate::state::ProviderRegistry::from_config(&config).unwrap());
        let runs = Arc::new(agentyx_core::agent::RunRegistry::new());
        let state = Arc::new(AppState {
            agentyx_home: home.path().to_path_buf(),
            workspaces: svc,
            config,
            agents,
            providers,
            runs,
            event_bus: Arc::new(EventBus::new()),
            workspace_runtimes: std::sync::Mutex::new(std::collections::HashMap::new()),
            tool_registry: Arc::new(
                agentyx_core::tools::built_in_registry()
                    .into_iter()
                    .collect(),
            ),
            permission_gate: agentyx_core::permissions::PermissionGate::new(),
            permission_registry: agentyx_core::permissions::PermissionRegistry::new(),
            server: Arc::new(std::sync::OnceLock::new()),
        });
        (home, state)
    }

    /// Create a placeholder `.venv/` inside `root` that matches
    /// the platform convention expected by `detect_venv`:
    /// `.venv/bin/python` on Unix, `.venv/Scripts/python.exe` on
    /// Windows. The content is a stub — `detect_venv` only checks
    /// that the executable path exists, not that it runs.
    fn stub_venv(root: &std::path::Path) {
        let venv = root.join(".venv");
        let exe_dir = if cfg!(target_os = "windows") {
            venv.join("Scripts")
        } else {
            venv.join("bin")
        };
        let exe = if cfg!(target_os = "windows") {
            exe_dir.join("python.exe")
        } else {
            exe_dir.join("python")
        };
        std::fs::create_dir_all(&exe_dir).unwrap();
        std::fs::write(&exe, "#!/bin/sh\nstub\n").unwrap();
    }

    /// Create a workspace under a path that the workspace-root
    /// whitelist accepts. On macOS the whitelist is `/Users`; on
    /// Linux it's `/home`. We synthesize a path inside
    /// `dirs::home_dir()` and use a fresh `tempfile::TempDir` there.
    /// The TempDir is returned to the caller; dropping it cleans up.
    fn make_workspace(state: &AppState, label: &str) -> (tempfile::TempDir, Workspace) {
        let dir = whitelisted_tempdir();
        let ws = state
            .workspaces
            .open(
                dir.path(),
                OpenOptions {
                    name: Some(label.into()),
                },
            )
            .unwrap();
        (dir, ws)
    }

    /// Create a `tempfile::TempDir` under a path the workspace
    /// whitelist accepts (typically `dirs::home_dir()`). The
    /// returned `TempDir` lives in `$HOME` and is removed when
    /// the test ends via `Drop`, so no leftover files
    /// accumulate in the user's home. The caller must bind the
    /// return to a variable for the duration of the test, e.g.
    /// `let dir = whitelisted_tempdir();`.
    fn whitelisted_tempdir() -> tempfile::TempDir {
        let home = dirs::home_dir().expect("home dir must be set in tests");
        tempfile::Builder::new()
            .prefix("agentyx-")
            .tempdir_in(&home)
            .expect("create whitelisted tempdir")
    }

    /// Create a temp dir **inside** the workspace root, so it
    /// passes the `root_path ∪ extra_paths` sandbox check used
    /// by `add_extra_path`. The `TempDir`'s `Drop` removes it
    /// when the test ends, leaving the workspace root clean.
    fn extra_in_workspace(workspace_root: &std::path::Path) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("extra-")
            .tempdir_in(workspace_root)
            .expect("create extra tempdir")
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
        let extra = extra_in_workspace(root.path());
        let _ = state
            .workspaces
            .add_extra_path(ws.id, extra.path(), Some("assets".into()))
            .unwrap();

        let out = list_impl(&state.workspaces).await.unwrap();
        assert_eq!(out.len(), 1);

        let dto = &out[0];
        assert_eq!(dto.id, ws.id);
        assert_eq!(dto.name, "main");
        assert_eq!(
            dto.root_path,
            agentyx_core::workspace::canonicalize(root.path()).unwrap()
        );
        assert_eq!(dto.extra_paths.len(), 1);
        assert_eq!(dto.extra_paths[0].label, "assets");
        assert_eq!(
            dto.extra_paths[0].path,
            agentyx_core::workspace::canonicalize(extra.path()).unwrap()
        );
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
        stub_venv(root.path());

        let out = list_impl(&state.workspaces).await.unwrap();
        assert!(out[0].has_venv);
    }

    #[tokio::test]
    async fn open_impl_round_trip() {
        let (_home, state) = fresh_state().await;
        let dir = whitelisted_tempdir();
        let out = open_impl(&state.workspaces, dir.path(), Some("myproj".into()))
            .await
            .unwrap();
        assert_eq!(out.name, "myproj");

        // Re-open is idempotent.
        let out2 = open_impl(&state.workspaces, dir.path(), None)
            .await
            .unwrap();
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

        delete_impl(&state.workspaces, &state.runs, ws.id, false)
            .await
            .unwrap();

        let out = list_impl(&state.workspaces).await.unwrap();
        assert!(out.is_empty());
    }

    /// F02.AC7 — delete without `force` is rejected when the
    /// workspace has at least one running run.
    #[tokio::test]
    async fn f02_ac7_delete_with_active_runs_rejected() {
        let (_home, state) = fresh_state().await;
        let (_root, ws) = make_workspace(&state, "main");

        // Register a synthetic running run for the workspace.
        let run_id = agentyx_core::ids::RunId::new();
        let session_id = agentyx_core::ids::SessionId::new();
        let agent_id = agentyx_core::ids::AgentId::new();
        state.runs.register(agentyx_core::agent::RunHandle::new(
            run_id, session_id, ws.id, agent_id,
        ));

        // force=false → Conflict, workspace is NOT deleted.
        let err = delete_impl(&state.workspaces, &state.runs, ws.id, false)
            .await
            .unwrap_err();
        assert!(
            matches!(err, agentyx_core::AppError::Conflict { .. }),
            "expected Conflict, got {err:?}"
        );

        let out = list_impl(&state.workspaces).await.unwrap();
        assert_eq!(out.len(), 1, "workspace must remain after Conflict");

        // The run is still running (we only rejected delete; the
        // abort flag is untouched).
        let handle = state.runs.get(run_id).expect("run should still exist");
        assert!(handle.is_running());
    }

    /// F02.AC7 — delete with `force=true` aborts the running run
    /// (sets its abort flag) and proceeds to delete the workspace.
    #[tokio::test]
    async fn f02_ac7_delete_with_force_aborts_runs() {
        let (_home, state) = fresh_state().await;
        let (_root, ws) = make_workspace(&state, "main");

        let run_id = agentyx_core::ids::RunId::new();
        state.runs.register(agentyx_core::agent::RunHandle::new(
            run_id,
            agentyx_core::ids::SessionId::new(),
            ws.id,
            agentyx_core::ids::AgentId::new(),
        ));

        delete_impl(&state.workspaces, &state.runs, ws.id, true)
            .await
            .unwrap();

        let out = list_impl(&state.workspaces).await.unwrap();
        assert!(out.is_empty(), "workspace must be deleted with force=true");

        // The run's handle is still in the registry (we don't
        // remove it from `RunRegistry`; the loop's `mark` will
        // transition it to a terminal status asynchronously),
        // but its abort flag must be set.
        let handle = state.runs.get(run_id).expect("run handle still registered");
        assert!(
            handle.is_aborted(),
            "force-delete must have requested abort"
        );
    }

    /// F02.AC7 — delete with no active runs succeeds regardless
    /// of `force`. Regression for the no-runs happy path.
    #[tokio::test]
    async fn f02_ac7_delete_no_runs_succeeds() {
        let (_home, state) = fresh_state().await;
        let (_root, ws) = make_workspace(&state, "main");

        // No runs registered; both force=true and force=false must work.
        delete_impl(&state.workspaces, &state.runs, ws.id, false)
            .await
            .unwrap();
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
        stub_venv(root.path());

        let out = detect_venv_impl(&state.workspaces, ws.id)
            .await
            .unwrap()
            .expect("should detect");
        assert_eq!(out.kind, agentyx_core::workspace::VenvKind::Uv);
    }

    #[tokio::test]
    async fn add_extra_path_impl_persists_and_returns_dto() {
        let (_home, state) = fresh_state().await;
        let (root, ws) = make_workspace(&state, "main");
        let extra = extra_in_workspace(root.path());

        let dto = add_extra_path_impl(
            &state.workspaces,
            ws.id,
            extra.path(),
            Some("Assets".into()),
        )
        .await
        .unwrap();

        assert_eq!(dto.label, "Assets");
        assert_eq!(
            dto.id,
            agentyx_core::workspace::canonicalize(extra.path())
                .unwrap()
                .to_string_lossy()
        );

        // Persisted (re-list should show it).
        let listed = list_extra_paths_impl(&state.workspaces, ws.id)
            .await
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, dto.id);
    }

    #[tokio::test]
    async fn add_extra_path_impl_defaults_label_to_basename() {
        let (_home, state) = fresh_state().await;
        let (root, ws) = make_workspace(&state, "main");
        // The subdir is created inside the workspace root so the
        // `root_path ∪ extra_paths` sandbox accepts it.
        let sub = root.path().join("my-cool-extras");
        std::fs::create_dir(&sub).unwrap();

        let dto = add_extra_path_impl(&state.workspaces, ws.id, &sub, None)
            .await
            .unwrap();
        assert_eq!(dto.label, "my-cool-extras");
    }

    #[tokio::test]
    async fn remove_extra_path_impl_persists() {
        let (_home, state) = fresh_state().await;
        let (root, ws) = make_workspace(&state, "main");
        let extra = extra_in_workspace(root.path());
        let dto = add_extra_path_impl(&state.workspaces, ws.id, extra.path(), None)
            .await
            .unwrap();

        remove_extra_path_impl(&state.workspaces, ws.id, &dto.path)
            .await
            .unwrap();

        let listed = list_extra_paths_impl(&state.workspaces, ws.id)
            .await
            .unwrap();
        assert!(listed.is_empty());
    }

    #[tokio::test]
    async fn list_extra_paths_impl_orders_by_added_at() {
        let (_home, state) = fresh_state().await;
        let (root, ws) = make_workspace(&state, "main");
        let e1 = extra_in_workspace(root.path());
        std::thread::sleep(std::time::Duration::from_millis(5));
        let e2 = extra_in_workspace(root.path());
        std::thread::sleep(std::time::Duration::from_millis(5));
        let e3 = extra_in_workspace(root.path());

        add_extra_path_impl(&state.workspaces, ws.id, e1.path(), None)
            .await
            .unwrap();
        add_extra_path_impl(&state.workspaces, ws.id, e2.path(), None)
            .await
            .unwrap();
        add_extra_path_impl(&state.workspaces, ws.id, e3.path(), None)
            .await
            .unwrap();

        let listed = list_extra_paths_impl(&state.workspaces, ws.id)
            .await
            .unwrap();
        assert_eq!(listed.len(), 3);
        assert_eq!(
            listed[0].id,
            agentyx_core::workspace::canonicalize(e1.path())
                .unwrap()
                .to_string_lossy()
        );
        assert_eq!(
            listed[2].id,
            agentyx_core::workspace::canonicalize(e3.path())
                .unwrap()
                .to_string_lossy()
        );
    }

    #[tokio::test]
    async fn effective_paths_impl_returns_root_and_extras() {
        let (_home, state) = fresh_state().await;
        let (root, ws) = make_workspace(&state, "main");
        let extra = extra_in_workspace(root.path());
        add_extra_path_impl(&state.workspaces, ws.id, extra.path(), None)
            .await
            .unwrap();

        let eff = effective_paths_impl(&state.workspaces, ws.id)
            .await
            .unwrap();
        assert_eq!(
            eff.root,
            agentyx_core::workspace::canonicalize(root.path()).unwrap()
        );
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
        stub_venv(dir.path());

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

    // ============================================================
    // list_dir_impl tests (F02 UI: FileTree needs a backend listdir)
    // ============================================================

    #[tokio::test]
    async fn f02_ac3_list_dir_impl_returns_root_entries() {
        // F02.AC3: selecting a workspace loads its file tree.
        let (_home, state) = fresh_state().await;
        let (root, ws) = make_workspace(&state, "main");
        // Seed a small tree: a file, a subfolder, and a hidden entry.
        std::fs::write(root.path().join("README.md"), "hello").unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("src").join("lib.rs"), "").unwrap();
        std::fs::write(root.path().join(".gitignore"), "").unwrap();

        let out = list_dir_impl(&state.workspaces, ws.id, root.path())
            .await
            .unwrap();
        let names: Vec<&str> = out.iter().map(|e| e.name.as_str()).collect();

        // Sort order: directories first, then files, both groups
        // case-insensitive. `.gitignore` < `README.md` < `src` per
        // the case-insensitive comparison, so the file group is
        // `.gitignore`, `README.md`; the only directory is `src`.
        assert_eq!(
            names,
            vec!["src", ".gitignore", "README.md"],
            "entries must be sorted: dirs first, then files (case-insensitive)"
        );
        assert_eq!(out[0].name, "src");
        assert!(out[0].is_dir);
        assert!(!out[0].is_symlink);
    }

    #[tokio::test]
    async fn f02_ac9_list_dir_impl_empty_workspace() {
        // F02.AC9: workspace with zero files is not an error.
        let (_home, state) = fresh_state().await;
        let (root, ws) = make_workspace(&state, "empty");
        let out = list_dir_impl(&state.workspaces, ws.id, root.path())
            .await
            .unwrap();
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn f02_list_dir_impl_rejects_path_outside_sandbox() {
        // F02 path-sandbox: a path outside root ∪ extras is rejected.
        let (_home, state) = fresh_state().await;
        let (_root, ws) = make_workspace(&state, "main");
        let outside = tempfile::tempdir().unwrap(); // NOT inside the workspace root
        let err = list_dir_impl(&state.workspaces, ws.id, outside.path())
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            agentyx_core::AppError::PathOutsideWorkspace { .. }
        ));
    }

    #[tokio::test]
    async fn f02_list_dir_impl_allows_extra_path() {
        // F02: extra paths are part of the sandbox.
        let (_home, state) = fresh_state().await;
        let (root, ws) = make_workspace(&state, "main");
        let extra = extra_in_workspace(root.path());
        let _ = state
            .workspaces
            .add_extra_path(ws.id, extra.path(), Some("assets".into()))
            .unwrap();
        std::fs::write(extra.path().join("photo.png"), [0u8; 16]).unwrap();

        let out = list_dir_impl(&state.workspaces, ws.id, extra.path())
            .await
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "photo.png");
        assert!(!out[0].is_dir);
        assert_eq!(out[0].size, 16);
    }

    #[tokio::test]
    async fn f02_list_dir_impl_unknown_workspace_is_not_found() {
        let (_home, state) = fresh_state().await;
        let err = list_dir_impl(
            &state.workspaces,
            WorkspaceId::new(),
            std::path::Path::new("."),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, agentyx_core::AppError::NotFound { .. }));
    }

    #[tokio::test]
    async fn f02_list_dir_impl_rejects_file_not_directory() {
        let (_home, state) = fresh_state().await;
        let (root, ws) = make_workspace(&state, "main");
        let file = root.path().join("README.md");
        std::fs::write(&file, "x").unwrap();
        let err = list_dir_impl(&state.workspaces, ws.id, &file)
            .await
            .unwrap_err();
        assert!(matches!(err, agentyx_core::AppError::InvalidInput { .. }));
    }
}
