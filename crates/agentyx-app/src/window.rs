//! Window configuration — main window, settings window, splash, etc.
//!
//! In v0.1 only the main window exists. v0.2+ may add a separate
//! settings window (currently a Svelte route within the main webview).

use tauri::{App, WebviewUrl, WebviewWindowBuilder};

/// Build the main application window and configure its properties
/// (title, size, decorations, dev URL).
pub fn configure_main_window(app: &mut App) -> tauri::Result<()> {
    let url = if cfg!(dev) {
        // Dev: Vite serves at http://localhost:1420 (Tauri convention).
        // `url::ParseError` is not in `tauri::Error`'s `From` impl
        // list, so we map it explicitly to `Error::InvalidUrl`.
        WebviewUrl::External(
            "http://localhost:1420"
                .parse()
                .map_err(tauri::Error::InvalidUrl)?,
        )
    } else {
        // Prod: load the bundled index.html.
        WebviewUrl::App("index.html".into())
    };

    WebviewWindowBuilder::new(app, "main", url)
        .title("Agentyx")
        .inner_size(1280.0, 800.0)
        .min_inner_size(800.0, 600.0)
        .resizable(true)
        .fullscreen(false)
        .decorations(true)
        .visible(true)
        .center()
        .build()?;

    Ok(())
}
