//! Agentyx desktop app — Tauri 2 entrypoint.
//!
//! This is the **only** crate that depends on Tauri. All business
//! logic lives in `agentyx-core`; this binary is a thin shell that:
//!
//! 1. Sets up the Tauri runtime (window, menu, deep links, updater).
//! 2. Initializes the `AppState` with a `Config`, a `Storage` pool,
//!    an `AgentRegistry`, and the LLM provider clients.
//! 3. Wires up the Tauri command handlers under `commands::*`.
//! 4. Streams events from Rust to the UI via the `EventBus`.
//!
//! The actual agent loop, tools, providers, sessions, etc. are all
//! in `agentyx-core` and are unit-tested there.
//!
//! See `../../specs/architecture.md` and `../../specs/ipc.md` for
//! the IPC contract this binary implements.

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use agentyx_core::AppResult;
use anyhow::Context;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod commands;
mod deep_link;
mod events;
mod menu;
mod server;
mod sink;
mod state;
mod updater;
mod window;

use state::AppState;

/// Entry point for the Tauri desktop app.
///
/// Responsibilities:
/// 1. Initialize structured logging (`tracing` → stderr + file).
/// 2. Build the Tauri runtime with the bundled UI (Vite output).
/// 3. Register the Tauri command handlers and event listeners.
/// 4. Hand off to Tauri's event loop.
fn main() -> anyhow::Result<()> {
    init_tracing();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        target = tauri::utils::platform::target_triple().unwrap_or_else(|_| "unknown".to_string()),
        "agentyx starting"
    );

    let app_state = AppState::initialize().context("initializing AppState")?;

    // Recover from an unclean shutdown: mark any session that
    // was `Running` when the app died as `Aborted` with
    // `last_run_finish_reason = "app_closed"`. The user sees a
    // truncated history next time they open the session.
    if let Err(e) = app_state.recover_orphan_runs() {
        tracing::warn!(error = %e, "orphan run recovery failed; continuing");
    }

    let state = Arc::new(app_state);

    // F06: attach the embedded HTTP server state. We do this
    // **before** the Tauri setup hook so the `server_*` Tauri
    // commands can reach the server. The actual listener is
    // started inside the setup hook (after the Tauri runtime is
    // up and we have an `AppHandle` for the TauriSink).
    let server_state = server::lifecycle::build_state(state.clone());
    state.attach_server(server_state.clone());

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state.clone())
        .setup(move |app| {
            window::configure_main_window(app)?;
            let _ = menu::build_menu(app.handle());
            deep_link::register(app);
            events::register_event_handlers(app, state.clone());

            // F06: register the TauriSink so events fan out to
            // the Tauri webview (and to the broadcast channel
            // for SSE clients in follow-up PRs).
            state
                .event_bus
                .add_sink(std::sync::Arc::new(events::TauriSink::new(
                    app.handle().clone(),
                )));

            // F06: start the embedded HTTP server with the
            // default config (loopback, no auth, random port).
            // Errors are logged but non-fatal — the desktop app
            // still works even if the server fails to bind.
            let server_config = server::ServerConfig::default();
            let server_state_for_setup = state
                .server()
                .ok_or_else(|| anyhow::anyhow!("server state not attached"))?;
            tauri::async_runtime::spawn(async move {
                if let Err(e) =
                    server::lifecycle::start(server_state_for_setup, server_config).await
                {
                    tracing::warn!(error = %e, "embedded HTTP server failed to start");
                }
            });

            updater::check_on_startup(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::workspace::list_workspaces,
            commands::workspace::open,
            commands::workspace::get_workspace,
            commands::workspace::delete_workspace,
            commands::workspace::detect_workspace_venv,
            commands::workspace::add_extra_path,
            commands::workspace::remove_extra_path,
            commands::workspace::list_extra_paths,
            commands::workspace::effective_paths,
            commands::workspace::list_dir,
            commands::session::create_session,
            commands::session::send,
            commands::session::abort,
            commands::session::list_sessions,
            commands::session::get_history,
            commands::session::set_active_agent,
            commands::session::get_active_agent,
            commands::agents::list_agents,
            commands::agents::get_agent,
            commands::config::config_get_global,
            commands::config::config_update_global,
            commands::config::config_get_workspace,
            commands::config::config_update_workspace,
            commands::providers::providers_test_connection,
            commands::secrets::set_secret,
            commands::secrets::delete_secret,
            commands::secrets::list_providers,
            commands::permissions::respond,
            commands::permissions::list,
            commands::permissions::get_matrix,
            commands::permissions::set_default,
            commands::server::server_get_info,
            commands::server::server_update_config,
            commands::server::server_rotate_token,
        ])
        .run(tauri::generate_context!())
        .context("running Tauri app")?;

    Ok(())
}

/// Initialize the global tracing subscriber.
///
/// Format:
/// - Development: pretty, colorized, level from `RUST_LOG` env or `info`.
/// - Production: JSON, level from env or `info`.
///
/// Output:
/// - stderr (always).
/// - rolling file under `~/.agentyx/logs/agentyx.log.YYYY-MM-DD` (planned;
///   in v0.1 just stderr).
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,agentyx_core=debug,agentyx_app=debug"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(true).with_thread_ids(false))
        .init();
}

/// Suppress the warning when no commands are registered yet
/// (during v0.1 scaffolding).
#[allow(dead_code)]
fn _typecheck_state(_s: &AppState) -> AppResult<()> {
    Ok(())
}
