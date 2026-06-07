//! `EventSink` implementations — bridges `agentyx_core::agent::EventSink`
//! to the `EventBus` for different contexts.
//!
//! - `TauriEventSink` — used by Tauri command handlers; emits to both
//!   Tauri windows and the broadcast channel.
//! - `BroadcastEventSink` — used by the embedded HTTP server; emits
//!   only to the broadcast channel (no Tauri `AppHandle` needed).

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

/// Bridges `EventSink` to the broadcast channel only (no Tauri).
/// Used by the HTTP server's `send_message` handler so agent loop
/// events reach SSE clients without requiring an `AppHandle`.
#[derive(Clone)]
pub struct BroadcastEventSink {
    bus: Arc<EventBus>,
}

impl std::fmt::Debug for BroadcastEventSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BroadcastEventSink")
            .field("bus", &self.bus)
            .finish()
    }
}

impl BroadcastEventSink {
    #[must_use]
    pub fn new(bus: Arc<EventBus>) -> Self {
        Self { bus }
    }
}

impl EventSink for BroadcastEventSink {
    fn emit(&self, event: &str, payload: Value) {
        let _ = self.bus.publish_typed(event, payload);
    }
}
