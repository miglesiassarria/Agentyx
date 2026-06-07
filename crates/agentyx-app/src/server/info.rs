//! `GET /api/v1/server/info` — returns the current `ServerInfo`.
//!
//! The full bearer token is **never** included; only a 4-char
//! `token_hint` is exposed when `require_token = true`. See
//! `super::state::ServerInfo` for the exact shape.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;

use super::state::{ServerInfo, ServerState};

/// `GET /api/v1/server/info` handler.
pub async fn server_info(State(state): State<Arc<ServerState>>) -> Json<ServerInfo> {
    Json(state.info())
}

/// `GET /api/v1/health` handler. Always returns 200 OK with a
/// minimal JSON body, even when `require_token = true` (health
/// checks must not collide with monitorización).
pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
