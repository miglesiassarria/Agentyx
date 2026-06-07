//! Tauri commands for the embedded HTTP server (F06).
//!
//! - `server_get_info` — returns the current `ServerInfo`.
//! - `server_update_config` — applies a config patch and
//!   restarts the server with the new config if it was running.
//! - `server_rotate_token` — generates a fresh bearer token and
//!   stores it in the in-memory state.

use std::sync::Arc;

use agentyx_core::AppResult;
use serde::Deserialize;
use tauri::State;

use crate::server::lifecycle::{generate_token, start, stop};
use crate::server::state::{ServerConfig, ServerInfo, ServerState};
use crate::state::AppState;

/// Helper: get the live `ServerState` from `AppState`. Returns
/// `internal` if `attach_server` was never called (shouldn't
/// happen in production).
fn server(state: &AppState) -> AppResult<Arc<ServerState>> {
    state
        .server()
        .ok_or_else(|| agentyx_core::AppError::Internal {
            message: "HTTP server state not attached to AppState".into(),
        })
}

/// `server_get_info() -> ServerInfoDto`
#[tauri::command]
pub async fn server_get_info(state: State<'_, Arc<AppState>>) -> AppResult<ServerInfo> {
    Ok(server(state.inner())?.info())
}

/// `server_update_config(patch: ServerConfigPatch) -> ServerInfoDto`
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerConfigPatch {
    pub enabled: Option<bool>,
    pub bind_host: Option<String>,
    pub port: Option<u16>,
    pub lan_enabled: Option<bool>,
    pub require_token: Option<bool>,
}

#[tauri::command]
pub async fn server_update_config(
    state: State<'_, Arc<AppState>>,
    patch: ServerConfigPatch,
) -> AppResult<ServerInfo> {
    let srv = server(state.inner())?;
    // Read current config from the state and apply the patch.
    let current = srv.info();
    let bind_host = patch.bind_host.unwrap_or_else(|| {
        current
            .bind_addr
            .split(':')
            .next()
            .unwrap_or("127.0.0.1")
            .to_string()
    });
    let port = patch.port.unwrap_or(current.port);
    let new_config = ServerConfig {
        enabled: patch.enabled.unwrap_or(true),
        bind_host,
        port,
        lan_enabled: patch.lan_enabled.unwrap_or(current.lan_enabled),
        require_token: patch.require_token.unwrap_or(current.require_token),
        ..ServerConfig::default()
    };

    if srv.is_running() {
        stop(srv.clone()).await?;
    }

    if new_config.enabled {
        start(srv, new_config).await
    } else {
        Ok(srv.info())
    }
}

/// `server_rotate_token() -> ServerInfoDto`
#[tauri::command]
pub async fn server_rotate_token(state: State<'_, Arc<AppState>>) -> AppResult<ServerInfo> {
    let srv = server(state.inner())?;
    let token = generate_token();
    srv.set_token(Some(token));
    Ok(srv.info())
}
