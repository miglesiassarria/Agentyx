//! Static file serving for the embedded UI.
//!
//! The Tauri webview loads the Svelte bundle from `ui/dist/`; the
//! embedded server reuses the same files for browser clients.
//! In production, `ui/dist/` is bundled into the Tauri resources;
//! in dev, the path is `../../ui/dist/` relative to the binary's
//! working directory (see `crate::window::resolve_ui_dev_url` for
//! the dev URL).
//!
//! For the MVP, the static file resolver is a stub that returns
//! 404 for unknown paths. The real `ServeDir` integration lands
//! in PR4/5 when we wire the browser UI to actual workspace
//! endpoints.

use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::IntoResponse;

/// `GET /*` fallback. Returns 404 in this PR; full static serving
/// is wired in a follow-up once the UI build artifact path is
/// resolved (Tauri production resources vs dev `ui/dist/`).
pub async fn serve_ui_fallback(_request: Request) -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        axum::Json(serde_json::json!({
            "code": "not_found",
            "message": "static file serving is wired in a follow-up PR",
        })),
    )
}
