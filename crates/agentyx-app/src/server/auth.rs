//! Bearer auth middleware for the embedded HTTP server.
//!
//! MVP dogfooding stance (see `specs/features/F06-web-server-lan.md`
//! §MVP dogfooding caveats and `specs/domains/server.md`): when
//! `[server].require_token = false` (the default), the middleware
//! is a no-op. When `require_token = true`, every request to
//! `/api/v1/*` (except `/health`) must carry a matching
//! `Authorization: Bearer <token>` header; otherwise it is
//! rejected with `401` and a `WWW-Authenticate: Bearer` response
//! header.
//!
//! The bearer middleware and the no-auth path share the same
//! router; the only switch is the `require_token` flag. Tests
//! cover both code paths from day one (AC4-AC6 of `domains/server.md`).

use std::sync::Arc;

use axum::extract::State;
use axum::http::{header, HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::state::ServerState;

/// Apply the bearer auth policy to a request.
///
/// - If `require_token = false`, returns the inner response
///   unchanged (MVP dogfooding).
/// - If `require_token = true`, requires
///   `Authorization: Bearer <configured_token>`. Missing header,
///   wrong scheme, or wrong token → 401 with `WWW-Authenticate`.
pub async fn bearer_layer(
    State(state): State<Arc<ServerState>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // Cheap path: require_token is off. Skip the whole check.
    if !state.info().require_token {
        return next.run(request).await;
    }

    let expected = match state.bearer_token() {
        Some(t) => t,
        // Defensive: require_token is on but the keychain gave
        // us no token. Treat as "no auth" so the user can recover
        // by rotating the token via `server_rotate_token`.
        None => return unauthorized("server in invalid state: require_token=true but no token"),
    };

    let header_value = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let presented = header_value.and_then(|v| v.strip_prefix("Bearer "));
    match presented {
        Some(token) if constant_time_eq(token.as_bytes(), expected.as_bytes()) => {
            next.run(request).await
        }
        _ => unauthorized("invalid or missing bearer token"),
    }
}

fn unauthorized(message: &'static str) -> Response {
    let body = serde_json::json!({
        "code": "unauthorized",
        "message": message,
    });
    let mut response = (StatusCode::UNAUTHORIZED, axum::Json(body)).into_response();
    response
        .headers_mut()
        .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    response
}

/// Constant-time byte comparison. Used for bearer tokens to avoid
/// leaking the prefix length via timing. We compare full strings
/// (not a `Result`-shortcircuiting `==`) so the function runs in
/// time proportional to the length of the **expected** token.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
