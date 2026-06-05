//! Event bus — bridges domain events from `agentyx-core` to
//! Tauri window events.
//!
//! The agent loop, journal, and storage layers emit events
//! (`ChatEvent`, `JournalEvent`, `WorkspaceEvent`, ...). The
//! `EventBus` translates these into Tauri `Window::emit(...)` calls
//! so the Svelte UI can listen via `lib/ipc.ts`.
//!
//! See `../../specs/ipc.md` §"Eventos streaming" for the event
//! schema and naming conventions.

// Placeholder methods (`emit_to`) are part of the v0.2 event-routing
// surface; suppressed here so the v0.1 scaffold builds cleanly.
#![allow(dead_code)]

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// Thin wrapper around an `AppHandle` that knows how to publish
/// events to the main window.
///
/// Cloning is cheap (`Arc` inside). The bus itself is stateless;
/// all routing goes through the Tauri event system.
pub struct EventBus {
    // Reserved for future use: a bounded mpsc channel if we need
    // to backpressure, or an in-memory ring buffer for replay.
    _phantom: std::marker::PhantomData<()>,
}

impl EventBus {
    /// Create a new event bus. Does not need an `AppHandle` at
    /// construction time; one is injected per `emit` call.
    #[must_use]
    pub fn new() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }

    /// Emit a typed event to all windows.
    ///
    /// `event` is the event name (e.g. `"chat.content.delta.v1"`)
    /// and `payload` is the serializable body. The event is delivered
    /// asynchronously via Tauri's event loop.
    pub fn emit<T: Serialize + Clone>(&self, app: &AppHandle, event: &str, payload: T) {
        if let Err(e) = app.emit(event, payload) {
            tracing::warn!(event = event, error = %e, "failed to emit event");
        }
    }

    /// Emit a typed event to a specific window by label.
    pub fn emit_to<T: Serialize + Clone>(
        &self,
        app: &AppHandle,
        target: &str,
        event: &str,
        payload: T,
    ) {
        if let Some(window) = app.get_webview_window(target) {
            if let Err(e) = window.emit(event, payload) {
                tracing::warn!(
                    event = event,
                    target = target,
                    error = %e,
                    "failed to emit event to window"
                );
            }
        } else {
            tracing::warn!(target = target, "target window not found");
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Register global event handlers with the Tauri app. Currently
/// a no-op (all events are emitted on demand by command handlers);
/// in v1.x this is where cross-cutting listeners (e.g. file
/// watcher → `file_changed.v1`) will be registered.
pub fn register_event_handlers(_app: &mut tauri::App, _state: Arc<crate::state::AppState>) {
    tracing::debug!("event handlers registered");
}
