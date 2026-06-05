//! Auto-updater — checks for new versions on startup (opt-in based
//! on the `update_channel` in `~/.agentyx/config.toml`).
//!
//! v0.1: this is a stub that logs the configured channel but does
//! not actually check for updates. The full implementation lands
//! in v1.0 (F20) with signed manifests and notarized builds.

use tauri::App;

use crate::state::AppState;

/// Check for updates at startup. Respects the `check_updates` and
/// `update_channel` settings in the global config.
pub fn check_on_startup(_app: &mut App) -> tauri::Result<()> {
    let state = _app.state::<std::sync::Arc<AppState>>();

    let enabled = state
        .config
        .try_read()
        .map(|c| c.global.check_updates)
        .unwrap_or(true);

    let channel = state
        .config
        .try_read()
        .map(|c| format!("{:?}", c.global.update_channel))
        .unwrap_or_else(|_| "stable".into());

    if !enabled {
        tracing::info!(channel = %channel, "auto-update disabled by config");
        return Ok(());
    }

    tracing::info!(channel = %channel, "auto-update check (stub in v0.1)");
    // TODO(F20): implement real update check via tauri-plugin-updater
    // when the release pipeline is in place.

    Ok(())
}
