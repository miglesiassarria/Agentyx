//! Application state — the shared, app-wide data accessible to
//! every Tauri command handler.
//!
//! Wrapped in `Arc<AppState>` and stored in Tauri's `State<...>`.
//! All fields are `Arc<...>`-shared for cheap clones across
//! command handlers (which run on a Tokio runtime).
//!
//! Lifecycle:
//! - `AppState::initialize()` is called once at startup, in `main()`.
//! - Each field is loaded from disk (or fresh state if first run).
//! - Mutations go through dedicated `update_*` methods that persist
//!   atomically (see `config.md` and `storage.md` for the contracts).
//!
//! ## v0.1 status
//!
//! This is a minimal placeholder. The full state will be assembled
//! as the corresponding domain crates land:
//! - `config` crate (PR for `config.md`) → `ResolvedConfig`
//! - `storage` crate (PR for `storage.md`) → `StoragePool`
//! - `agents` module (PR for `agents.md`) → `AgentRegistry`
//! - `llm` module (PR for `providers.md`) → `ProviderRegistry`
//!
//! For now we hold only the home dir + a `WorkspaceService`, which
//! is enough to wire up the F02 Tauri commands.

use std::path::PathBuf;
use std::sync::Arc;

use agentyx_core::AppResult;
use agentyx_core::workspace::WorkspaceService;
use anyhow::Context;

use crate::events::EventBus;

/// Application-wide state.
///
/// `Arc<AppState>` is `Send + Sync` and is held inside
/// `tauri::State<Arc<AppState>>`. Cloning the `Arc` is cheap and
/// is what each command handler does.
pub struct AppState {
    /// Path to `~/.agentyx/`. Source of truth for all per-user
    /// files (registry, workspaces, cache).
    pub agentyx_home: PathBuf,

    /// Workspace service: registry of workspaces, extra-paths,
    /// and per-workspace config.toml. Implemented in PR
    /// "feat(core): workspace model".
    pub workspaces: Arc<WorkspaceService>,

    /// Event bus for streaming events to the UI
    /// (`chat.*.v1`, `pty.*.v1`, `agent.*.v1`, etc.).
    pub event_bus: Arc<EventBus>,
}

impl AppState {
    /// Build the initial `AppState` at app startup. Creates
    /// `~/.agentyx/` if it doesn't exist; loads the workspace
    /// registry; initializes the event bus.
    ///
    /// In v0.1 the rest of the state (config, storage, agents,
    /// providers) is left as future work. See the module docs.
    pub fn initialize() -> AppResult<Self> {
        let agentyx_home = agentyx_home()
            .context("resolving ~/.agentyx")?
            .to_path_buf();

        let workspaces =
            Arc::new(WorkspaceService::new(&agentyx_home).context("loading workspace service")?);

        Ok(Self {
            agentyx_home,
            workspaces,
            event_bus: Arc::new(EventBus::new()),
        })
    }
}

/// Returns the path to the user's Agentyx home directory
/// (`~/.agentyx/` on Unix, `%APPDATA%\agentyx` on Windows).
/// Creates the directory if it doesn't exist.
///
/// This is a thin wrapper around the `dirs` crate that we'll
/// eventually move into `agentyx-core::config` once the
/// `config.md` PR lands.
fn agentyx_home() -> AppResult<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| agentyx_core::AppError::Internal {
        message: "could not resolve user home directory".into(),
    })?;
    let path = home.join(".agentyx");
    std::fs::create_dir_all(&path).map_err(|e| agentyx_core::AppError::Io {
        op: format!("create_dir_all {}", path.display()),
        source: e.to_string(),
    })?;
    Ok(path)
}
