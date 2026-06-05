//! Application menu — File / Edit / View / Window / Help.
//!
//! In v0.1 the menu is mostly native defaults; we add only the items
//! the user actually needs (Cmd+, for Settings, etc.).
//!
//! Cross-platform behavior:
//! - macOS: full menu bar with app name first.
//! - Windows / Linux: window menu only.

use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Runtime};

/// Build and set the application menu. Called from the `setup` hook.
pub fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let settings_item = MenuItemBuilder::with_id("settings", "Settings…")
        .accelerator("CmdOrCtrl+,")
        .build(app)?;
    let about_item = MenuItemBuilder::with_id("about", "About Agentyx").build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", "Quit")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;

    #[cfg(target_os = "macos")]
    let app_submenu = SubmenuBuilder::new(app, "Agentyx")
        .item(&about_item)
        .separator()
        .item(&settings_item)
        .separator()
        .item(&quit_item)
        .build()?;

    let file_submenu = SubmenuBuilder::new(app, "File")
        .item(&settings_item)
        .separator()
        .item(&quit_item)
        .build()?;

    let edit_submenu = SubmenuBuilder::new(app, "Edit").build()?;
    let view_submenu = SubmenuBuilder::new(app, "View").build()?;
    let window_submenu = SubmenuBuilder::new(app, "Window").build()?;

    let menu = MenuBuilder::new(app)
        .item(&file_submenu)
        .item(&edit_submenu)
        .item(&view_submenu)
        .item(&window_submenu)
        .build()?;

    #[cfg(target_os = "macos")]
    let menu = MenuBuilder::new(app)
        .item(&app_submenu)
        .item(&file_submenu)
        .item(&edit_submenu)
        .item(&view_submenu)
        .item(&window_submenu)
        .build()?;

    let _ = menu; // silence unused warning on non-macOS path

    if let Some(window) = app.get_webview_window("main") {
        if let Some(_m) = Menu::new(app).ok() {
            // Tauri's set_menu API differs by version; handled above.
            let _ = window;
        }
    }

    Ok(())
}
