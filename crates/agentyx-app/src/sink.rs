//! `EventSink` impl for Tauri — bridges `agentyx_core::agent::EventSink`
//! to Tauri's `AppHandle::emit`.
//!
//! The agent loop in `agentyx_core` emits events through the
//! `EventSink` trait (string event name + JSON payload). The app
//! layer implements that trait over the existing `EventBus` so
//! the events reach the Svelte UI via `window.emit`.
//!
//! Usage (from a Tauri command):
//!
//! ```ignore
//! #[tauri::command]
//! pub async fn send(
//!     state: State<'_, Arc<AppState>>,
//!     app: AppHandle,
//!     session_id: SessionId,
//!     content: String,
//! ) -> AppResult<RunHandleDto> {
//!     let sink = Arc::new(TauriEventSink::new(state.event_bus.clone(), app));
//!     let deps = build_deps(&state, sink)?;
//!     let handle = spawn_run(deps, session_id, content, StartOpts::default())?;
//!     // ...
//! }
//! ```
//!
//! `TauriEventSink` is `Send + Sync` (the `EventBus` is
//! `Arc`-shared and the `AppHandle` is `Clone + Send + Sync`),
//! which is the bound `AgentLoopDeps` requires for the
//! `bus: Arc<dyn EventSink>` field.

use std::sync::Arc;

use agentyx_core::agent::EventSink;
use serde_json::Value;
use tauri::AppHandle;

use crate::events::EventBus;

/// Bridges `EventSink` to Tauri's `AppHandle` via the `EventBus`.
#[derive(Clone)]
pub struct TauriEventSink {
    bus: Arc<EventBus>,
    app: AppHandle,
}

impl std::fmt::Debug for TauriEventSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TauriEventSink")
            .field("bus", &self.bus)
            .field("app", &"<AppHandle>")
            .finish()
    }
}

impl TauriEventSink {
    /// Build a new sink.
    #[must_use]
    pub fn new(bus: Arc<EventBus>, app: AppHandle) -> Self {
        Self { bus, app }
    }
}

impl EventSink for TauriEventSink {
    fn emit(&self, event: &str, payload: Value) {
        self.bus.emit(&self.app, event, payload);
    }
}
