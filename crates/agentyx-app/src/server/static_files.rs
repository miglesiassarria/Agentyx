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
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::IntoResponse;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};

/// Resolve the path to the built UI (`ui/dist/`).
fn ui_dist_path() -> std::path::PathBuf {
    // In development, callers may start from the repo root, from
    // `crates/`, or from a target directory. Walk ancestors and
    // look for the monorepo's `ui/dist` instead of relying on one
    // fragile relative path.
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(path) = find_ui_dist_from(&cwd) {
            return path;
        }
    }

    // In production (Tauri bundle), resources are expected near
    // the binary. The same ancestor walk keeps dev binaries under
    // `target/debug` working when launched directly.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent().and_then(find_ui_dist_from) {
            return parent;
        }
    }

    // Last resort: absolute dev path from CWD.
    std::path::PathBuf::from("ui/dist")
}

fn find_ui_dist_from(start: &std::path::Path) -> Option<std::path::PathBuf> {
    for ancestor in start.ancestors() {
        let candidate = ancestor.join("ui").join("dist");
        if candidate.join("index.html").is_file() {
            return Some(candidate);
        }
    }
    None
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
            let is_html = response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.starts_with("text/html"));

            let mut resp = response.into_response();

            // If ServeDir missed and fell back to index.html,
            // promote the 404 to 200 so SPA routing works.
            if status == StatusCode::NOT_FOUND {
                *resp.status_mut() = StatusCode::OK;
            }

            // The shell HTML must be revalidated every time. During
            // LAN dogfooding a stale cached `/` can otherwise keep
            // showing an old blank page while `/?v=...` works.
            if is_html || status == StatusCode::NOT_FOUND {
                resp.headers_mut().insert(
                    CACHE_CONTROL,
                    HeaderValue::from_static("no-store, no-cache, must-revalidate"),
                );
            }

            resp
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

#[cfg(test)]
mod tests {
    use super::find_ui_dist_from;

    #[test]
    fn f06_ac10_finds_ui_dist_from_crates_directory() -> Result<(), Box<dyn std::error::Error>> {
        let repo = tempfile::tempdir()?;
        let crates_dir = repo.path().join("crates");
        let dist = repo.path().join("ui").join("dist");
        std::fs::create_dir_all(&crates_dir)?;
        std::fs::create_dir_all(&dist)?;
        std::fs::write(
            dist.join("index.html"),
            "<!doctype html><title>Agentyx</title>",
        )?;

        let found = find_ui_dist_from(&crates_dir).ok_or("find ui/dist from crates dir")?;

        if !found.join("index.html").is_file() {
            return Err(format!(
                "resolved ui/dist should contain index.html: {}",
                found.display()
            )
            .into());
        }
        Ok(())
    }
}
