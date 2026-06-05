//! Deep link handling — `agentyx://...` URLs from the OS.
//!
//! In v0.1 this is a stub. The full implementation lands when we
//! add shareable session links (F35, backlog).
//!
//! Examples (planned):
//! - `agentyx://workspace/open?path=/Users/me/proj`
//! - `agentyx://session/<id>` (v1.x)

use tauri::App;

/// Register a deep-link handler. Currently a no-op.
pub fn register(_app: &mut App) {
    tracing::debug!("deep link handler registered (stub in v0.1)");
}
