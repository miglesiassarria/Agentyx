//! Application menu — File / Edit / View / Window / Help.
//!
//! v0.1: minimal menu. The full menu lands once Settings (F05)
//! has more items to expose.
//!
//! Cross-platform behavior:
//! - macOS: full menu bar with app name first.
//! - Windows / Linux: window menu only.

use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Manager, Runtime};

/// Build and set the application menu on the main window.
/// Called from the `setup` hook. Failures are non-fatal; the
/// window just shows up without a menu.
pub fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let settings_item = MenuItemBuilder::with_id("settings", "Settings…")
        .accelerator("CmdOrCtrl+,")
        .build(app)?;
    // `about_item` is only used inside the macOS app submenu; on
    // Windows/Linux we still build it so the menu builder does
    // not need platform-specific branches, but we suppress the
    // unused-variable lint for those targets.
    let about_item = MenuItemBuilder::with_id("about", "About Agentyx").build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", "Quit")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;

    let file_submenu = SubmenuBuilder::new(app, "File")
        .item(&settings_item)
        .separator()
        .item(&quit_item)
        .build()?;

    let edit_submenu = SubmenuBuilder::new(app, "Edit").build()?;
    let view_submenu = SubmenuBuilder::new(app, "View").build()?;
    let window_submenu = SubmenuBuilder::new(app, "Window").build()?;

    #[cfg_attr(not(target_os = "macos"), allow(unused_mut, unused_variables))]
    let mut menu_builder = MenuBuilder::new(app);

    #[cfg(target_os = "macos")]
    {
        let app_submenu = SubmenuBuilder::new(app, "Agentyx")
            .item(&about_item)
            .separator()
            .item(&settings_item)
            .separator()
            .item(&quit_item)
            .build()?;
        menu_builder = menu_builder.item(&app_submenu);
    }

    let menu = menu_builder
        .item(&file_submenu)
        .item(&edit_submenu)
        .item(&view_submenu)
        .item(&window_submenu)
        .build()?;

    if let Some(window) = app.get_webview_window("main") {
        window.set_menu(menu)?;
    }

    Ok(())
}
