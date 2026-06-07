//! Static file serving for the embedded UI.
//!
//! The Tauri webview loads the Svelte bundle from `ui/dist/`; the
//! embedded server reuses the same files for browser clients.
//! In production, `ui/dist/` is bundled into the Tauri resources;
//! in dev, the path is `../../ui/dist/` relative to the binary's
//! working directory.
//!
//! The fallback returns `index.html` for SPA client-side routing.

use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};

/// Resolve the path to the built UI (`ui/dist/`).
fn ui_dist_path() -> std::path::PathBuf {
    // In development, `ui/dist/` is relative to the workspace root.
    // In production (Tauri bundle), the resources are adjacent to the binary.
    let dev_path = std::path::PathBuf::from("../../ui/dist");
    if dev_path.exists() {
        return dev_path;
    }
    // Fallback: look next to the binary.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let p = parent.join("ui").join("dist");
            if p.exists() {
                return p;
            }
        }
    }
    // Last resort: absolute dev path from CWD.
    std::path::PathBuf::from("ui/dist")
}

/// `GET /*` fallback. Serves static files from `ui/dist/` with
/// SPA fallback (unknown paths return `index.html`).
///
/// `ServeDir` with a `not_found_service(ServeFile)` will serve
/// the index file body for unknown paths but preserves the inner
/// `ServeFile`'s 404 status. We rewrite the status to 200 so
/// deep-link refreshes and client-side routes work correctly
/// per F06.AC10.
pub async fn serve_ui_fallback(request: Request) -> impl IntoResponse {
    let dist = ui_dist_path();
    let index = dist.join("index.html");

    let service = ServeDir::new(&dist).not_found_service(ServeFile::new(&index));

    match service.oneshot(request).await {
        Ok(response) => {
            let status = response.status();
            // If ServeDir missed and fell back to index.html,
            // promote the 404 to 200 so SPA routing works.
            if status == StatusCode::NOT_FOUND {
                let mut resp = response.into_response();
                *resp.status_mut() = StatusCode::OK;
                resp
            } else {
                response.into_response()
            }
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({
                "code": "internal_error",
                "message": "failed to serve static files",
            })),
        )
            .into_response(),
    }
}
