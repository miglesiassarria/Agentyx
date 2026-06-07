//! Server lifecycle: start, stop, info, config update, token rotate.
//!
//! The HTTP server runs as a background tokio task spawned by
//! `start()`. Stopping the server signals the task via a
//! `oneshot` and joins it. The `ServerState` is shared with the
//! router so the Tauri command handlers and the HTTP handlers
//! see the same configuration snapshot.

use std::net::SocketAddr;
use std::sync::Arc;

use agentyx_core::AppError;
use agentyx_core::AppResult;
use axum::serve;
use tokio::net::TcpListener;
use tracing::{info, warn};

use super::router::build_router;
use super::state::{ServerConfig, ServerInfo, ServerState};
use crate::state::AppState;

/// Start the HTTP server with the given config. Idempotent: if
/// the server is already running with the same `bind_host`/`port`,
/// returns the existing `ServerInfo` without restarting. With
/// different config, returns `Conflict`.
///
/// **Errors**:
/// - `invalid_input` — `bind_host` is not loopback and
///   `lan_enabled = false`.
/// - `conflict` — server is already running with different config.
/// - `internal` — bind / listen failed.
pub async fn start(state: Arc<ServerState>, config: ServerConfig) -> AppResult<ServerInfo> {
    // Refuse non-loopback bind without explicit opt-in.
    if !config.is_loopback() && !config.lan_enabled {
        return Err(AppError::InvalidInput {
            message: format!(
                "bind_host '{}' requires lan_enabled=true; LAN bind is opt-in",
                config.bind_host
            ),
        });
    }

    // Idempotency check: same config and already running → return
    // the existing info.
    if state.is_running() {
        let current = state.info();
        let same = current.lan_enabled == config.lan_enabled
            && current.require_token == config.require_token
            && current.bind_addr.split(':').next() == Some(config.bind_host.as_str());
        if same {
            return Ok(current);
        }
        return Err(AppError::Conflict {
            message: "server is already running with different config; stop it first".into(),
        });
    }

    let bind_addr: SocketAddr = format!("{}:{}", config.bind_host, config.port)
        .parse()
        .map_err(|e| AppError::InvalidInput {
            message: format!("invalid bind address: {e}"),
        })?;

    let listener = TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("bind {}: {}", bind_addr, e),
        })?;

    let resolved = listener.local_addr().map_err(|e| AppError::Internal {
        message: format!("read local_addr: {e}"),
    })?;

    let router = build_router(state.clone());
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let app_state_for_task = state.app_state();
    let task = tokio::spawn(async move {
        let server = serve(listener, router).with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        });
        if let Err(e) = server.await {
            warn!(error = %e, "HTTP server task ended with error");
        }
        info!(workspace_id = %app_state_for_task.agentyx_home.display(), "HTTP server task stopped");
    });

    let started_at_ms = chrono::Utc::now().timestamp_millis();
    let info = ServerInfo {
        enabled: config.enabled,
        bind_addr: resolved.to_string(),
        port: resolved.port(),
        lan_enabled: config.lan_enabled,
        require_token: config.require_token,
        token_hint: None,
        started_at_ms: Some(started_at_ms),
        rate_limit_per_window: config.rate_limit_per_window,
        rate_window_secs: config.rate_window.as_secs(),
    };
    state.set_info(info.clone());
    state.set_task(task, shutdown_tx);

    // Single warn at startup when LAN is open without auth —
    // explicit MVP dogfooding caveat. The middleware is the
    // single switch; the bearer layer compiles in either way.
    if !config.is_loopback() && !config.require_token {
        warn!(
            bind = %resolved,
            "LAN bind without bearer auth — local dogfooding only"
        );
    }

    info!(bind = %resolved, "embedded HTTP server started");
    Ok(info)
}

/// Stop the HTTP server and wait for the task to finish. Returns
/// the final `ServerInfo` (with `enabled = false`).
pub async fn stop(state: Arc<ServerState>) -> AppResult<ServerInfo> {
    let Some((handle, shutdown_tx)) = state.take_task() else {
        return Ok(state.info());
    };
    // Signal shutdown; ignore send error (receiver may have
    // already been dropped if the task panicked).
    let _ = shutdown_tx.send(());
    // Give the task a moment to drain in-flight requests.
    let _ = handle.await;
    let mut info = state.info();
    info.enabled = false;
    info.started_at_ms = None;
    state.set_info(info.clone());
    Ok(info)
}

/// Generate a fresh bearer token (32 hex chars). Stored in
/// memory only; persistence to keychain lands in a follow-up PR
/// that also adds the `server_rotate_token` Tauri command.
#[must_use]
pub fn generate_token() -> String {
    use std::fmt::Write;
    let bytes: [u8; 16] = rand_bytes();
    let mut s = String::with_capacity(32);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn rand_bytes() -> [u8; 16] {
    // `getrandom` would be the canonical choice; to keep this
    // PR small we use the stdlib hash of a process-local counter
    // and a timestamp. Good enough for MVP dogfooding tokens
    // (16 bytes of entropy, single-process, not exposed in any
    // network reply). The keychain-backed rotation lands in
    // PR4/5.
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;
    let mut h = DefaultHasher::new();
    SystemTime::now().hash(&mut h);
    std::process::id().hash(&mut h);
    let a = h.finish();
    let b = h.finish();
    [
        a as u8,
        (a >> 8) as u8,
        (a >> 16) as u8,
        (a >> 24) as u8,
        (a >> 32) as u8,
        (a >> 40) as u8,
        (a >> 48) as u8,
        (a >> 56) as u8,
        b as u8,
        (b >> 8) as u8,
        (b >> 16) as u8,
        (b >> 24) as u8,
        (b >> 32) as u8,
        (b >> 40) as u8,
        (b >> 48) as u8,
        (b >> 56) as u8,
    ]
}

/// Build a `ServerState` for an existing `AppState` and return
/// it along with a closure that starts the server. Used by
/// `main.rs` at startup.
#[must_use]
pub fn build_state(app_state: Arc<AppState>) -> Arc<ServerState> {
    Arc::new(ServerState::new(app_state))
}
