//! Server state, configuration, and lifecycle.
//!
//! See `../../../../specs/domains/server.md` for the full design.
//! This module is the **only** place where the server is started
//! and stopped; command handlers and HTTP handlers both read
//! from / write to the shared `ServerState`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::state::AppState;

/// Public-facing server configuration. Mirrors the `[server]`
/// section of `~/.agentyx/config.toml` (see `domains/config.md`).
///
/// Defaults match the MVP dogfooding stance: loopback only, no
/// auth required, port 0 (= random free port at startup). LAN
/// bind must be explicitly opted in via `lan_enabled = true` and
/// auth is opt-in via `require_token`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    /// Whether the server is enabled at all. Default `true` in v0.1.
    pub enabled: bool,
    /// Bind host. `"127.0.0.1"` (loopback) or `"::1"` are treated
    /// as loopback (no auth). Anything else is LAN and requires
    /// `lan_enabled = true`.
    pub bind_host: String,
    /// Port to bind. `0` = random free port at startup.
    pub port: u16,
    /// Opt-in to bind on `0.0.0.0` (or any non-loopback). The
    /// server refuses to start with `bind_host != loopback` if
    /// this is `false`.
    pub lan_enabled: bool,
    /// When LAN is enabled, require `Authorization: Bearer <token>`
    /// on every `/api/v1/*` request. Default `false` for MVP
    /// dogfooding on a trusted LAN; flip to `true` for hardening.
    pub require_token: bool,
    /// Per-client rate limit (requests per `rate_window`). SSE
    /// `/api/v1/events` is exempt. Default 60 / 10s.
    pub rate_limit_per_window: u32,
    /// Rate-limit window duration. Default 10s.
    #[serde(with = "duration_secs")]
    pub rate_window: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind_host: "127.0.0.1".to_string(),
            port: 0,
            lan_enabled: false,
            require_token: false,
            rate_limit_per_window: 60,
            rate_window: Duration::from_secs(10),
        }
    }
}

impl ServerConfig {
    /// Whether the configured bind host is loopback (no auth
    /// required, regardless of `require_token`).
    #[must_use]
    pub fn is_loopback(&self) -> bool {
        matches!(self.bind_host.as_str(), "127.0.0.1" | "::1" | "localhost")
    }
}

/// Public-facing server info, returned by the `server_get_info`
/// Tauri command and the `GET /api/v1/server/info` HTTP endpoint.
/// Never includes the bearer token (only a `token_hint` of the
/// last 4 chars, when `require_token = true`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    /// Mirror of `ServerConfig` with the live `bind_addr` (host +
    /// resolved port) instead of the requested `port`.
    pub enabled: bool,
    pub bind_addr: String,
    pub port: u16,
    pub lan_enabled: bool,
    pub require_token: bool,
    /// Last 4 chars of the bearer token, or `None` if no token is
    /// configured. The full value is **never** included.
    pub token_hint: Option<String>,
    /// Epoch milliseconds when the server started, or `None` if
    /// not running.
    pub started_at_ms: Option<i64>,
    /// Per-client rate limit (requests per `rate_window`).
    pub rate_limit_per_window: u32,
    /// Rate-limit window duration, in seconds.
    pub rate_window_secs: u64,
}

/// Internal state shared between the Axum router, the Tauri
/// command handlers, and the lifecycle task. Cheap to clone
/// (`Arc` inside).
#[derive(Clone)]
pub struct ServerState {
    inner: Arc<ServerStateInner>,
}

struct ServerStateInner {
    /// Live config + info. Updated by `start`/`stop` and by
    /// `server_update_config`.
    info: RwLock<ServerInfo>,
    /// Resolved bearer token, if `require_token = true`. The token
    /// itself never leaves the process; only `ServerInfo::token_hint`
    /// is exposed. Read by the `bearer_layer` middleware.
    token: RwLock<Option<String>>,
    /// Handle to the background Axum task. `None` when the server
    /// is not running.
    handle: RwLock<Option<JoinHandle<()>>>,
    /// Shutdown signal sender; replaced on every `start`.
    shutdown_tx: RwLock<Option<oneshot::Sender<()>>>,
    /// Shared app state (used by handlers to call into the
    /// business logic).
    app_state: Arc<AppState>,
}

impl ServerState {
    /// Build a new, not-yet-started `ServerState` bound to the
    /// given `AppState`.
    #[must_use]
    pub fn new(app_state: Arc<AppState>) -> Self {
        let default_info = ServerConfig::default();
        Self {
            inner: Arc::new(ServerStateInner {
                info: RwLock::new(ServerInfo {
                    enabled: false,
                    bind_addr: String::new(),
                    port: 0,
                    lan_enabled: default_info.lan_enabled,
                    require_token: default_info.require_token,
                    token_hint: None,
                    started_at_ms: None,
                    rate_limit_per_window: default_info.rate_limit_per_window,
                    rate_window_secs: default_info.rate_window.as_secs(),
                }),
                token: RwLock::new(None),
                handle: RwLock::new(None),
                shutdown_tx: RwLock::new(None),
                app_state,
            }),
        }
    }

    /// Current public info (cheap clone of an `Arc`-backed struct).
    #[must_use]
    pub fn info(&self) -> ServerInfo {
        self.inner.info.read().clone()
    }

    /// Current bearer token, if `require_token = true`. Used by
    /// the `bearer_layer` middleware.
    #[must_use]
    pub fn bearer_token(&self) -> Option<String> {
        self.inner.token.read().clone()
    }

    /// Whether the server is currently running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.inner.handle.read().is_some()
    }

    /// Resolved bind address (host + port), or `None` when the
    /// server is not running.
    #[allow(dead_code)]
    #[must_use]
    pub fn bound_addr(&self) -> Option<SocketAddr> {
        let info = self.inner.info.read();
        info.bind_addr.parse().ok()
    }

    /// Reference to the shared `AppState` (used by handlers).
    #[must_use]
    pub fn app_state(&self) -> Arc<AppState> {
        self.inner.app_state.clone()
    }

    /// Update the live `ServerInfo` (used internally by `start`,
    /// `stop`, and the Tauri command handlers).
    pub fn set_info(&self, info: ServerInfo) {
        *self.inner.info.write() = info;
    }

    /// Set / clear the bearer token. The token is also mirrored
    /// into `info.token_hint` (last 4 chars only).
    pub fn set_token(&self, token: Option<String>) {
        let hint = token.as_ref().map(|t| {
            if t.len() <= 4 {
                t.clone()
            } else {
                t[t.len() - 4..].to_string()
            }
        });
        self.inner.info.write().token_hint = hint;
        *self.inner.token.write() = token;
    }

    /// Store the Axum task handle and the shutdown signal sender.
    pub fn set_task(&self, handle: JoinHandle<()>, shutdown_tx: oneshot::Sender<()>) {
        *self.inner.handle.write() = Some(handle);
        *self.inner.shutdown_tx.write() = Some(shutdown_tx);
    }

    /// Take the current task handle and shutdown sender, leaving
    /// `None` in their place. Used by `stop` to join the task
    /// and signal shutdown.
    pub fn take_task(&self) -> Option<(JoinHandle<()>, oneshot::Sender<()>)> {
        let handle = self.inner.handle.write().take();
        let shutdown = self.inner.shutdown_tx.write().take();
        match (handle, shutdown) {
            (Some(h), Some(s)) => Some((h, s)),
            _ => None,
        }
    }
}

/// Helper for serializing `Duration` as integer seconds in TOML/JSON.
mod duration_secs {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(d.as_secs())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let secs = u64::deserialize(d)?;
        Ok(Duration::from_secs(secs))
    }
}
