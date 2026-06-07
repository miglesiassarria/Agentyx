//! Event bus — fan-out of domain events to multiple sinks.
//!
//! v0.1 (F06) replaces the original Tauri-only `EventBus` with a
//! pub/sub fan-out: every published event is delivered to every
//! registered `EventSink`. The Tauri sink (the original behavior)
//! stays, and a new `BroadcastSink` is added so the embedded HTTP
//! server's SSE handler can subscribe to the same event stream
//! without duplicating the source of truth.
//!
//! `agentyx-core` does **not** depend on this bus; the bus lives
//! in `agentyx-app` and is owned by `AppState`. Domain code emits
//! events through their existing `events.rs` modules and the
//! Tauri command wrappers translate them into `EventBus::publish`
//! calls. The `EventSink` trait is local to this crate.
//!
//! See `../../../specs/domains/server.md` for the full design and
//! `../../../specs/ipc.md` §3 for the event schema.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::broadcast;

/// Capacity of the in-process broadcast channel. Sized for the
/// expected concurrency of SSE clients during MVP dogfooding
/// (a few browsers on the LAN); larger values waste memory, smaller
/// values force slow SSE clients to disconnect.
const BROADCAST_CAPACITY: usize = 256;

/// A single published event: the wire name (e.g.
/// `"chat.content.delta.v1"`) and the serializable payload.
#[derive(Debug, Clone)]
pub struct PublishedEvent {
    /// Event name, including the `.vN` suffix.
    pub name: String,
    /// JSON-serializable payload. Stored as `serde_json::Value`
    /// so the broadcast channel can be type-erased.
    pub payload: serde_json::Value,
}

impl PublishedEvent {
    /// Build a new `PublishedEvent` by serializing the payload.
    ///
    /// # Errors
    /// Returns `serde_json::Error` if `payload` cannot be
    /// serialized. In practice this only happens for custom
    /// serializers; the standard `Serialize` impls from
    /// `agentyx-core` and `serde_json` always succeed.
    pub fn new<T: Serialize>(
        name: impl Into<String>,
        payload: T,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            name: name.into(),
            payload: serde_json::to_value(payload)?,
        })
    }
}

/// Sink of `PublishedEvent`s. Implementations route events to a
/// specific delivery channel (Tauri windows, SSE clients, in-memory
/// test loggers, …).
///
/// Sinks are registered with the `EventBus` at startup and are
/// invoked synchronously from `publish` on the same task. Sinks
/// **must not** block; if a sink needs I/O, it should spawn a
/// task or use a non-blocking API. A misbehaving sink can stall
/// all subsequent publishes.
pub trait EventSink: Send + Sync {
    /// Stable identifier for the sink. Used in `tracing` logs.
    fn name(&self) -> &'static str;

    /// Deliver an event. Returns `Err` on transport failure; the
    /// bus logs the error and continues with the remaining sinks.
    fn handle(&self, event: &PublishedEvent) -> Result<(), EventSinkError>;
}

/// Errors returned by `EventSink::handle`. The bus doesn't
/// distinguish between them at the call site; sinks return
/// `EventSinkError::Other` for anything that's not a transport
/// hiccup.
#[derive(Debug, thiserror::Error)]
pub enum EventSinkError {
    /// Tauri `AppHandle::emit` failed (window closed, IPC down).
    #[error("tauri emit failed: {0}")]
    Tauri(String),
    /// Broadcast channel is closed or lagged.
    #[error("broadcast: {0}")]
    Broadcast(String),
    /// Anything else. Reserved for future sinks (file, metrics,
    /// …); current sinks only emit `Tauri` and `Broadcast`.
    #[allow(dead_code)]
    #[error("sink error: {0}")]
    Other(String),
}

/// Default Tauri sink. Re-emits the event to all Tauri windows
/// via `AppHandle::emit`. Failure to emit is logged but never
/// propagated (UI windows come and go during shutdown).
pub struct TauriSink {
    app: AppHandle,
}

impl TauriSink {
    /// Wrap the given `AppHandle`.
    #[must_use]
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl EventSink for TauriSink {
    fn name(&self) -> &'static str {
        "tauri"
    }

    fn handle(&self, event: &PublishedEvent) -> Result<(), EventSinkError> {
        self.app
            .emit(event.name.as_str(), event.payload.clone())
            .map_err(|e| EventSinkError::Tauri(e.to_string()))
    }
}

/// Broadcast sink. Hands every published event to a
/// `tokio::sync::broadcast::Sender`; SSE clients (and tests) hold
/// `broadcast::Receiver`s and consume the stream.
///
/// The sender is held inside the bus; the receivers are handed
/// out via [`EventBus::subscribe`].
#[derive(Clone)]
pub struct BroadcastSink {
    tx: broadcast::Sender<PublishedEvent>,
}

impl BroadcastSink {
    /// Build a new broadcast sink with the default capacity.
    #[must_use]
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(BROADCAST_CAPACITY);
        Self { tx }
    }

    /// Build a new broadcast sink with a custom capacity. Used
    /// by tests that want a tight buffer to verify "slow client
    /// drops" behavior.
    #[allow(dead_code)]
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { tx }
    }
}

impl Default for BroadcastSink {
    fn default() -> Self {
        Self::new()
    }
}

impl EventSink for BroadcastSink {
    fn name(&self) -> &'static str {
        "broadcast"
    }

    fn handle(&self, event: &PublishedEvent) -> Result<(), EventSinkError> {
        // `send` returns `Err` when there are no active receivers;
        // we treat that as a non-fatal "nobody listening" case.
        // When receivers are slow and the buffer is full, they get
        // a `RecvError::Lagged` on their side and we drop the
        // overflow event silently here.
        self.tx
            .send(event.clone())
            .map(|_| ())
            .or_else(|e| match e {
                broadcast::error::SendError(_) => Ok(()),
            })
            .map_err(|e: std::convert::Infallible| EventSinkError::Broadcast(e.to_string()))
    }
}

/// The event bus. Holds the broadcast channel and the list of
/// registered sinks. Cheap to clone (`Arc` inside).
///
/// `EventBus` itself is a `Arc<Inner>` so the broadcast channel
/// and the sink list can be shared between the Tauri command
/// wrappers and the embedded HTTP server.
#[derive(Clone, Default)]
pub struct EventBus {
    inner: Arc<EventBusInner>,
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus")
            .field("sinks", &self.inner.sinks.lock().len())
            .finish()
    }
}

struct EventBusInner {
    broadcast: BroadcastSink,
    sinks: parking_lot::Mutex<Vec<Arc<dyn EventSink>>>,
}

impl Default for EventBusInner {
    fn default() -> Self {
        Self {
            broadcast: BroadcastSink::new(),
            sinks: parking_lot::Mutex::new(Vec::new()),
        }
    }
}

impl EventBus {
    /// Build a new bus with the default broadcast sink already
    /// registered. The Tauri sink is added separately (it needs
    /// an `AppHandle` which isn't available here).
    #[must_use]
    pub fn new() -> Self {
        let bus = Self {
            inner: Arc::new(EventBusInner::default()),
        };
        bus.add_sink(Arc::new(bus.inner.broadcast.clone()));
        bus
    }

    /// Add a sink. The same sink can be added more than once; the
    /// bus invokes each registration independently.
    pub fn add_sink(&self, sink: Arc<dyn EventSink>) {
        self.inner.sinks.lock().push(sink);
    }

    /// Subscribe to the broadcast channel. Returns a
    /// `broadcast::Receiver` that yields every event published
    /// **after** this call. Used by the SSE handler (lands in
    /// PR5).
    #[allow(dead_code)]
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<PublishedEvent> {
        self.inner.broadcast.tx.subscribe()
    }

    /// Publish an event to all registered sinks.
    ///
    /// Sinks are invoked synchronously in registration order. A
    /// sink error is logged at `warn` and the loop continues; a
    /// panicking sink would crash the task (sinks must catch
    /// their own errors). Returns the number of sinks that
    /// successfully handled the event.
    pub fn publish(&self, event: &PublishedEvent) -> usize {
        let sinks = self.inner.sinks.lock();
        let mut ok = 0;
        for sink in sinks.iter() {
            match sink.handle(event) {
                Ok(()) => ok += 1,
                Err(e) => tracing::warn!(
                    sink = sink.name(),
                    event = %event.name,
                    error = %e,
                    "event sink failed; continuing with next sink"
                ),
            }
        }
        ok
    }

    /// Convenience wrapper that serializes the payload before
    /// publishing. Returns the number of successful sinks, or
    /// `Err(serde_json::Error)` if serialization fails.
    pub fn publish_typed<T: Serialize>(
        &self,
        name: &str,
        payload: T,
    ) -> Result<usize, serde_json::Error> {
        let event = PublishedEvent::new(name, payload)?;
        Ok(self.publish(&event))
    }

    /// Emit a typed event to all sinks. Convenience wrapper used
    /// by the Tauri command wrappers; equivalent to
    /// `publish_typed` but ignores the result of `serde_json::to_value`
    /// (we use `serde_json::Value` semantics internally so this
    /// can never fail for the standard serializable types).
    pub fn emit<T: Serialize + Clone>(&self, _app: &AppHandle, event: &str, payload: T) {
        match self.publish_typed(event, payload) {
            Ok(0) => tracing::trace!(event, "no sinks handled event"),
            Ok(_) => {}
            Err(e) => tracing::warn!(event, error = %e, "failed to serialize event payload"),
        }
    }

    /// Emit a typed event to a specific Tauri window by label.
    /// In v0.1 this is a thin wrapper around `app.emit_to`; the
    /// broadcast sink does **not** see this event. Used for
    /// window-scoped events like deep-link navigation.
    #[allow(dead_code)]
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
                    event,
                    target,
                    error = %e,
                    "failed to emit event to window"
                );
            }
        } else {
            tracing::warn!(target, "target window not found");
        }
    }

    /// Number of registered sinks (for tests / diagnostics).
    #[allow(dead_code)]
    #[must_use]
    pub fn sink_count(&self) -> usize {
        self.inner.sinks.lock().len()
    }
}

/// Register global event handlers with the Tauri app. Currently
/// a no-op (all events are emitted on demand by command handlers);
/// in v1.x this is where cross-cutting listeners (e.g. file
/// watcher → `file_changed.v1`) will be registered.
pub fn register_event_handlers(_app: &mut tauri::App, _state: Arc<crate::state::AppState>) {
    tracing::debug!("event handlers registered");
}
