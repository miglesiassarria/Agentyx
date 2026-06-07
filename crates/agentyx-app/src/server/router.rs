//! Axum router — assembly of all HTTP routes and middleware.
//!
//! See `../../../../specs/domains/server.md` for the full design.
//! This module is the only place where the route tree is built;
//! every endpoint added in PR5/6 will hang off the `Router::nest`
//! call below.

use std::sync::Arc;

use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use super::auth::bearer_layer;
use super::handlers;
use super::info::{health, server_info};
use super::state::ServerState;
use super::static_files::serve_ui_fallback;

/// Build the Axum router bound to the given `ServerState`. The
/// returned `Router` is fully self-contained and ready to be
/// `.serve(...)`'d.
pub fn build_router(state: Arc<ServerState>) -> Router {
    // Per-`/api/v1/*` routes that don't require auth.
    let public_api = Router::new().route("/health", get(health));

    // Per-`/api/v1/*` routes that require the bearer layer.
    let protected_api = Router::new()
        .route("/server/info", get(server_info))
        // Workspaces
        .route(
            "/workspaces",
            get(handlers::list_workspaces).post(handlers::open_workspace),
        )
        .route(
            "/workspaces/:id",
            get(handlers::get_workspace).delete(handlers::delete_workspace),
        )
        .route("/workspaces/:id/venv", get(handlers::detect_venv))
        .route(
            "/workspaces/:id/effective-paths",
            get(handlers::effective_paths),
        )
        .route(
            "/workspaces/:id/extra-paths",
            get(handlers::list_extra_paths).post(handlers::add_extra_path),
        )
        .route(
            "/workspaces/:id/extra-paths/delete",
            delete(handlers::remove_extra_path),
        )
        .route("/workspaces/:id/list-dir", post(handlers::list_dir))
        // Sessions
        .route(
            "/workspaces/:id/sessions",
            get(handlers::list_sessions).post(handlers::create_session),
        )
        .route("/sessions/:id/history", get(handlers::get_history))
        .route("/sessions/:id/abort", post(handlers::abort_session))
        .route(
            "/sessions/:id/active-agent",
            get(handlers::get_active_agent).post(handlers::set_active_agent),
        )
        .route("/sessions/:id/messages", post(handlers::send_message))
        // Agents
        .route("/agents", get(handlers::list_agents))
        .route("/agents/:id", get(handlers::get_agent))
        // Config (F06 AC9)
        .route(
            "/config/global",
            get(handlers::get_config_global).patch(handlers::update_config_global),
        )
        .route(
            "/config/workspaces/:id",
            get(handlers::get_workspace_config).patch(handlers::update_workspace_config),
        )
        // Providers (F06 AC9)
        .route(
            "/providers/test-connection",
            post(handlers::test_provider_connection),
        )
        // Secrets (F06 AC9)
        .route("/secrets/providers", get(handlers::list_secret_providers))
        .route(
            "/secrets/:provider_id",
            post(handlers::set_secret).delete(handlers::delete_secret),
        )
        // Permissions (F06 AC7 + AC9)
        .route("/permissions/matrix", get(handlers::get_permission_matrix))
        .route(
            "/permissions/default",
            post(handlers::set_default_permission),
        )
        .route(
            "/permissions/requests",
            get(handlers::list_permission_requests),
        )
        .route(
            "/permissions/requests/:id/respond",
            post(handlers::respond_permission_request),
        )
        // Diffs (F04)
        .route("/sessions/:id/diffs", get(handlers::list_session_diffs))
        .route("/diffs/:tool_call_id", get(handlers::get_diff_full))
        // SSE streaming (F06 AC6/AC8)
        .route("/events", get(handlers::sse_events))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            bearer_layer,
        ));

    let api = Router::new().merge(public_api).merge(protected_api);

    let cors = CorsLayer::very_permissive();

    let nosniff = SetResponseHeaderLayer::if_not_present(
        axum::http::header::HeaderName::from_static("x-content-type-options"),
        axum::http::HeaderValue::from_static("nosniff"),
    );

    Router::new()
        .nest("/api/v1", api)
        .fallback(serve_ui_fallback)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(nosniff)
        .with_state(state)
}
