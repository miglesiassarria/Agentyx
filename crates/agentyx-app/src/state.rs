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

use std::sync::Arc;

use agentyx_core::AppResult;
use anyhow::Context;
use tokio::sync::RwLock;

use crate::events::EventBus;

/// Application-wide state.
///
/// `Arc<AppState>` is `Send + Sync` and is held inside
/// `tauri::State<Arc<AppState>>`. Cloning the `Arc` is cheap and
/// is what each command handler does.
///
/// Each inner field is itself wrapped in a sync primitive (`RwLock`
/// for mutable state, plain `Arc` for immutable) so handlers can
/// access state concurrently without blocking the UI.
pub struct AppState {
    /// Global configuration (`~/.agentyx/config.toml`), parsed
    /// and with secrets resolved (in memory only, never serialized).
    pub config: Arc<RwLock<agentyx_core::config::ResolvedConfig>>,

    /// Workspace storage pool (one `state.db` per workspace).
    /// Lazy-opened on first access; concurrent-safe via internal
    /// `Arc<Mutex<Connection>>`.
    pub storage: Arc<agentyx_core::storage::StoragePool>,

    /// Agent registry: 3 built-in + custom (v1.x). Loaded once
    /// at startup from the registry code in `agentyx-core`.
    pub agents: Arc<agentyx_core::agents::AgentRegistry>,

    /// LLM provider clients (Ollama, Groq, Minimax). Each
    /// implements the `Provider` trait; the registry routes
    /// requests by `ProviderId`.
    pub providers: Arc<agentyx_core::llm::ProviderRegistry>,

    /// Event bus for streaming events to the UI
    /// (`chat.*.v1`, `pty.*.v1`, `agent.*.v1`, etc.).
    pub event_bus: Arc<EventBus>,
}

impl AppState {
    /// Build the initial `AppState` at app startup. Loads config,
    /// opens the default workspace (if any), initializes the agent
    /// registry, and creates the provider clients.
    ///
    /// In v0.1 this is a stub that returns an error if the system
    /// is not in a valid state (e.g. no config dir writable). In
    /// later versions it gets richer.
    pub fn initialize() -> AppResult<Self> {
        // 1. Resolve `~/.agentyx/` paths.
        let agentyx_dir = agentyx_core::config::agentyx_home()
            .context("resolving ~/.agentyx")?;

        // 2. Load (or create) the global config.
        let config = agentyx_core::config::Config::load_global(&agentyx_dir)
            .context("loading global config")?;
        let resolved = agentyx_core::config::Config::resolve(
            config,
            None,
            &agentyx_core::config::OsKeychain,
        )
        .context("resolving config")?;

        // 3. Open the storage pool (lazy: workspaces opened on demand).
        let storage = agentyx_core::storage::StoragePool::new(&agentyx_dir)
            .context("initializing storage pool")?;

        // 4. Load the agent registry.
        let agents = agentyx_core::agents::AgentRegistry::load(&resolved.global)
            .context("loading agent registry")?;

        // 5. Build provider clients from the resolved config.
        let providers =
            agentyx_core::llm::ProviderRegistry::from_config(&resolved)
                .context("building provider clients")?;

        Ok(Self {
            config: Arc::new(RwLock::new(resolved)),
            storage: Arc::new(storage),
            agents: Arc::new(agents),
            providers: Arc::new(providers),
            event_bus: Arc::new(EventBus::new()),
        })
    }
}
