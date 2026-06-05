//! Auto-updater — checks for new versions on startup (opt-in based
//! on the `update_channel` in `~/.agentyx/config.toml`).
//!
//! v0.1: stub. Reads no config (the `config.md` module doesn't
//! exist yet). The full implementation lands in v1.0 (F20) with
//! signed manifests and notarized builds.

use tauri::App;

/// Check for updates at startup. v0.1: logs that we're skipping
/// the check; the real check lands with F20.
pub fn check_on_startup(_app: &mut App) -> tauri::Result<()> {
    tracing::info!("auto-update check (stub in v0.1; F20 in v1.0)");
    Ok(())
}
