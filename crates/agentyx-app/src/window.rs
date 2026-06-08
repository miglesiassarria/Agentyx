//! Window configuration — main window, settings window, splash, etc.
//!
//! In v0.1 only the main window exists. v0.2+ may add a separate
//! settings window (currently a Svelte route within the main webview).

use tauri::App;

/// Configure the main application window after Tauri has created it.
/// In Tauri 2 the window is declared in `tauri.conf.json` and created
/// automatically before the setup hook runs, so this function is a
/// no-op. The URL, title, size etc. are already set from the config.
pub fn configure_main_window(_app: &App) -> tauri::Result<()> {
    Ok(())
}
