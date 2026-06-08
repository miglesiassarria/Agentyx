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

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agentyx_core::agents::AgentRegistry;
use agentyx_core::config::{ConfigService, OsKeychain, ServiceConfigPaths};
use agentyx_core::ids::WorkspaceId;
use agentyx_core::journal::JournalRepo;
use agentyx_core::llm::Provider;
use agentyx_core::permissions::{PermissionGate, PermissionRegistry};
use agentyx_core::session::SessionService;
use agentyx_core::storage::Db;
use agentyx_core::tools::{built_in_registry, Tool};
use agentyx_core::workspace::WorkspaceService;
use agentyx_core::AppResult;

use crate::events::EventBus;
use crate::server::state::ServerState;

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
    /// and per-workspace config.toml.
    pub workspaces: Arc<WorkspaceService>,

    /// Global config (providers, default model, approval mode,
    /// UI prefs, telemetry, update channel). Persists to
    /// `~/.agentyx/config.toml`. Cheap to clone (`Arc` inside).
    pub config: Arc<ConfigService>,

    /// Built-in agent registry (3 visible + 3 hidden). Loaded
    /// once at startup; immutable thereafter. Cheap to clone.
    pub agents: Arc<AgentRegistry>,

    /// LLM provider registry. Maps `provider_id` →
    /// `Arc<dyn Provider>`. For Phase 1 only `ollama` is wired;
    /// `groq`/`minimax` arrive in F01-Phase2. Cheap to clone.
    pub providers: Arc<ProviderRegistry>,

    /// Active run registry. Tracks every `RunHandle` produced by
    /// `spawn_run` so that `chat_abort` and `chat_state` can look
    /// them up by id from any IPC command. Cheap to clone.
    pub runs: Arc<agentyx_core::agent::RunRegistry>,

    /// Event bus for streaming events to the UI
    /// (`chat.*.v1`, `pty.*.v1`, `agent.*.v1`, etc.).
    pub event_bus: Arc<EventBus>,

    /// Per-workspace runtime cache. Lazily created on first
    /// access via [`AppState::workspace_runtime`]. Holds the
    /// `Db` (state.db), the `SessionService`, and the
    /// `JournalRepo` for the workspace.
    pub workspace_runtimes: Mutex<HashMap<WorkspaceId, Arc<WorkspaceRuntime>>>,

    /// Tool registry. Built once at startup from the built-in
    /// set; custom tools land in v1.1. Cheap to clone
    /// (`Arc<Vec<Arc<dyn Tool>>>`).
    pub tool_registry: Arc<Vec<Arc<dyn Tool>>>,

    /// Permission gate. Stateless; takes a snapshot per run.
    pub permission_gate: PermissionGate,

    /// Permission registry. Holds the oneshot responders for
    /// `Ask` decisions; the `permission_respond` Tauri command
    /// (F01-Phase2-core follow-up) resolves them.
    pub permission_registry: PermissionRegistry,

    /// Embedded HTTP server state (F06). Created at startup
    /// alongside the rest of the app state; the actual listener
    /// is started from `main.rs` after the Tauri runtime is up.
    /// The state is shared with the Axum router so the Tauri
    /// command handlers and the HTTP handlers see the same
    /// configuration snapshot.
    ///
    /// Indirected through a `OnceLock` because `ServerState`
    /// holds an `Arc<AppState>` and `AppState` holds an
    /// `Arc<ServerState>` — a direct cycle would prevent either
    /// from being constructed. The lock is filled in once by
    /// [`AppState::attach_server`] right after `initialize`.
    pub server: Arc<std::sync::OnceLock<Arc<ServerState>>>,
}

impl AppState {
    /// Recover from an unclean shutdown. For every workspace,
    /// find sessions that were `Running` (i.e. had an active run
    /// when the app died) and mark them `Aborted` with
    /// `last_run_finish_reason = "app_closed"`. The user sees a
    /// truncated history next time they open the session.
    ///
    /// Called once at startup from [`AppState::initialize`].
    /// Idempotent.
    pub fn recover_orphan_runs(&self) -> AppResult<usize> {
        let mut total = 0usize;
        for workspace in self.workspaces.list() {
            let db_path = self
                .agentyx_home
                .join("workspaces")
                .join(workspace.id.to_string())
                .join("state.db");
            // Open the DB directly (no `workspace_runtime` cache
            // involvement — we don't want to keep orphan sessions
            // in the runtime cache after this).
            let db = agentyx_core::storage::Db::open(&db_path)?;
            let session = agentyx_core::session::SessionService::with_db(db, workspace.id);
            let orphans = session.list(agentyx_core::session::ListOpts {
                limit: None,
                status: Some(agentyx_core::session::SessionStatus::Running),
            })?;
            for s in orphans {
                tracing::warn!(
                    workspace_id = %workspace.id,
                    session_id = %s.id,
                    "recovering orphan run: app closed while running"
                );
                if let Err(e) = session.finish_run(
                    s.id,
                    agentyx_core::session::SessionStatus::Aborted,
                    "app_closed",
                ) {
                    tracing::warn!(
                        session_id = %s.id,
                        error = %e,
                        "failed to mark orphan session as aborted"
                    );
                } else {
                    total += 1;
                }
            }
        }
        Ok(total)
    }

    /// Build the initial `AppState` at app startup. Creates
    /// `~/.agentyx/` if it doesn't exist; loads the workspace
    /// registry, global config, and built-in agents; wires the
    /// LLM provider registry.
    pub fn initialize() -> AppResult<Self> {
        let agentyx_home = agentyx_home()?.to_path_buf();

        let workspaces = Arc::new(WorkspaceService::new(&agentyx_home)?);

        let config_paths = ServiceConfigPaths::from_agentyx_home(&agentyx_home);
        let keychain: Arc<dyn agentyx_core::config::KeychainAccess> = Arc::new(OsKeychain);
        let config = Arc::new(ConfigService::load_with_keychain(&config_paths, keychain)?);

        let agents = Arc::new(AgentRegistry::load_builtins());

        let providers = Arc::new(ProviderRegistry::from_config(&config)?);

        let runs = Arc::new(agentyx_core::agent::RunRegistry::new());

        let tool_registry: Arc<Vec<Arc<dyn Tool>>> =
            Arc::new(built_in_registry().into_iter().collect());
        let permission_gate = PermissionGate::new();
        let permission_registry = PermissionRegistry::new();

        Ok(Self {
            agentyx_home,
            workspaces,
            config,
            agents,
            providers,
            runs,
            event_bus: Arc::new(EventBus::new()),
            workspace_runtimes: Mutex::new(HashMap::new()),
            tool_registry,
            permission_gate,
            permission_registry,
            server: Arc::new(std::sync::OnceLock::new()),
        })
    }

    /// Attach the live `ServerState` to this `AppState`. Called
    /// once, right after `AppState::initialize()`, from `main.rs`.
    /// Panics if called twice (a programmer error — only one
    /// server state is ever built per process).
    pub fn attach_server(self: &Arc<Self>, server: Arc<ServerState>) {
        if self.server.set(server).is_err() {
            tracing::error!("AppState::attach_server called twice — ignoring the second call");
        }
    }

    /// Get the live `ServerState`. Returns `None` only if
    /// `attach_server` has not been called yet (which should not
    /// happen in production; `main.rs` attaches the server before
    /// the Tauri runtime starts).
    #[must_use]
    pub fn server(&self) -> Option<Arc<ServerState>> {
        self.server.get().cloned()
    }

    /// Get or create the per-workspace runtime (Db, SessionService,
    /// JournalRepo). Looks up the workspace in the `WorkspaceService`
    /// first to verify it exists, then opens (or reuses from the
    /// cache) the workspace's `state.db`.
    pub fn workspace_runtime(&self, workspace_id: WorkspaceId) -> AppResult<Arc<WorkspaceRuntime>> {
        if let Some(rt) = self
            .workspace_runtimes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&workspace_id)
        {
            return Ok(rt.clone());
        }

        let _workspace =
            self.workspaces
                .get(workspace_id)
                .ok_or_else(|| agentyx_core::AppError::NotFound {
                    kind: "workspace".to_string(),
                    id: workspace_id.to_string(),
                })?;
        let db_path = self
            .agentyx_home
            .join("workspaces")
            .join(workspace_id.to_string())
            .join("state.db");
        let db = Db::open(&db_path)?;
        let session = Arc::new(SessionService::with_db(db.clone(), workspace_id));
        let journal = Arc::new(JournalRepo::new(db));
        let runtime = Arc::new(WorkspaceRuntime {
            workspace_id,
            db_path,
            session,
            journal,
        });

        self.workspace_runtimes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(workspace_id, runtime.clone());

        Ok(runtime)
    }

    /// Drop the cached runtime for a workspace. Called from
    /// `workspace_delete` so subsequent commands don't reuse a
    /// dangling session/journal pointing at a deleted db file.
    #[allow(dead_code)]
    pub fn evict_workspace_runtime(&self, workspace_id: WorkspaceId) {
        self.workspace_runtimes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&workspace_id);
    }

    /// Refresh the provider registry from the current config.
    /// Called after provider config or secrets change so newly
    /// added providers become available without restart.
    pub fn refresh_providers(&self) -> AppResult<()> {
        self.providers.refresh(&self.config)
    }
}

/// Per-workspace runtime state. Created lazily and cached in
/// `AppState::workspace_runtimes`. Cheap to clone (`Arc` inside).
pub struct WorkspaceRuntime {
    /// The workspace this runtime belongs to.
    #[allow(dead_code)]
    pub workspace_id: WorkspaceId,
    /// Absolute path to the `state.db` file (for diagnostics).
    #[allow(dead_code)]
    pub db_path: PathBuf,
    /// Session service: create, list, append_message, etc.
    pub session: Arc<SessionService>,
    /// Journal repo: append-only log of run events.
    pub journal: Arc<JournalRepo>,
}

/// Registry of LLM providers, keyed by provider id
/// (`"ollama"`, `"groq"`, ...).
#[derive(Clone)]
pub struct ProviderRegistry {
    providers: Arc<parking_lot::RwLock<HashMap<String, Arc<dyn Provider>>>>,
}

impl std::fmt::Debug for ProviderRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let keys: Vec<_> = self.providers.read().keys().cloned().collect();
        f.debug_struct("ProviderRegistry")
            .field("providers", &keys)
            .finish()
    }
}

impl ProviderRegistry {
    /// Build a registry from the global config. For each enabled
    /// provider we instantiate a fresh client with the configured
    /// `base_url`. Cloud providers (Groq, Minimax) require a
    /// resolved API key; if the key is missing, the provider is
    /// skipped with a `tracing::warn!` (the user can still add
    /// the key via the Settings UI).
    pub fn from_config(config: &ConfigService) -> AppResult<Self> {
        let providers = Self::build_providers(config)?;
        Ok(Self {
            providers: Arc::new(parking_lot::RwLock::new(providers)),
        })
    }

    /// Rebuild the registry from the current config. Called after
    /// provider config or secrets change so newly added providers
    /// become available without restarting the app.
    pub fn refresh(&self, config: &ConfigService) -> AppResult<()> {
        let new = Self::build_providers(config)?;
        *self.providers.write() = new;
        Ok(())
    }

    fn build_providers(config: &ConfigService) -> AppResult<HashMap<String, Arc<dyn Provider>>> {
        use agentyx_core::llm::{GroqProvider, MinimaxProvider, OllamaProvider};
        let cfg = config.get();
        let mut providers: HashMap<String, Arc<dyn Provider>> = HashMap::new();

        for (id, provider_cfg) in &cfg.providers {
            if !provider_cfg.enabled {
                tracing::debug!(provider = id, "provider disabled in config; skipping");
                continue;
            }
            match id.as_str() {
                "ollama" => {
                    let base = if provider_cfg.base_url.is_empty() {
                        agentyx_core::llm::DEFAULT_BASE_URL
                    } else {
                        &provider_cfg.base_url
                    };
                    match OllamaProvider::with_base_url(base) {
                        Ok(p) => {
                            providers.insert(id.clone(), Arc::new(p));
                            tracing::info!(provider = id, base, "Ollama provider registered");
                        }
                        Err(e) => {
                            tracing::warn!(provider = id, error = %e, "failed to construct OllamaProvider");
                        }
                    }
                }
                "groq" => {
                    let base = if provider_cfg.base_url.is_empty() {
                        agentyx_core::llm::GROQ_DEFAULT_BASE_URL
                    } else {
                        &provider_cfg.base_url
                    };
                    let key = match config.resolve_secrets() {
                        Ok(secrets) => secrets.get("groq").cloned(),
                        Err(_) => None,
                    };
                    match key {
                        Some(k) => match GroqProvider::with_base_url(base, k) {
                            Ok(p) => {
                                providers.insert(id.clone(), Arc::new(p));
                                tracing::info!(provider = id, base, "Groq provider registered");
                            }
                            Err(e) => {
                                tracing::warn!(provider = id, error = %e, "failed to construct GroqProvider");
                            }
                        },
                        None => {
                            tracing::debug!(
                                provider = id,
                                "Groq provider skipped: no resolved API key"
                            );
                        }
                    }
                }
                "minimax" => {
                    let base = if provider_cfg.base_url.is_empty() {
                        agentyx_core::llm::MINIMAX_DEFAULT_BASE_URL
                    } else {
                        &provider_cfg.base_url
                    };
                    let key = match config.resolve_secrets() {
                        Ok(secrets) => secrets.get("minimax").cloned(),
                        Err(_) => None,
                    };
                    match key {
                        Some(k) => match MinimaxProvider::with_base_url(base, k) {
                            Ok(p) => {
                                providers.insert(id.clone(), Arc::new(p));
                                tracing::info!(provider = id, base, "Minimax provider registered");
                            }
                            Err(e) => {
                                tracing::warn!(provider = id, error = %e, "failed to construct MinimaxProvider");
                            }
                        },
                        None => {
                            tracing::debug!(
                                provider = id,
                                "Minimax provider skipped: no resolved API key"
                            );
                        }
                    }
                }
                other => {
                    tracing::warn!(provider = other, "unknown provider in config; skipping");
                }
            }
        }

        Ok(providers)
    }

    /// Get a provider by id.
    #[allow(dead_code)]
    #[must_use]
    pub fn get(&self, id: &str) -> Option<Arc<dyn Provider>> {
        self.providers.read().get(id).cloned()
    }

    /// List the registered provider ids.
    #[allow(dead_code)]
    #[must_use]
    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.providers.read().keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Snapshot the underlying map (cheap clone of the Arcs).
    /// Used by the agent loop when constructing `AgentLoopDeps`.
    #[must_use]
    pub fn to_hashmap(&self) -> HashMap<String, Arc<dyn Provider>> {
        self.providers.read().clone()
    }

    /// Register or replace a provider by id. Used by tests to
    /// inject a `MockProvider` (see
    /// `crates/agentyx-core/src/llm/mock.rs`) so chat/SSE
    /// integration tests don't need a live LLM. Idempotent.
    #[allow(dead_code)]
    pub fn register(&self, id: &str, provider: Arc<dyn Provider>) {
        self.providers.write().insert(id.to_string(), provider);
    }
}

/// Returns the path to the user's Agentyx home directory
/// (`~/.agentyx/` on Unix, `%APPDATA%\agentyx` on Windows).
/// Creates the directory if it doesn't exist.
fn agentyx_home() -> AppResult<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| agentyx_core::AppError::Internal {
        message: "could not resolve user home directory".into(),
    })?;
    let path = home.join(".agentyx");
    std::fs::create_dir_all(&path).map_err(|e| agentyx_core::AppError::Io {
        op: format!("create_dir_all {}", path.display()),
        reason: e.to_string(),
    })?;
    Ok(path)
}
