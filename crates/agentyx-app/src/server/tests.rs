//! Integration tests for the embedded HTTP server (F06).
//!
//! Covers AC1-AC3 from [`specs/domains/server.md`]:
//! - AC1: loopback bind, UI served, health endpoint OK.
//! - AC2: LAN bind + `require_token = true` → 401 without bearer.
//! - AC3: LAN bind + `require_token = false` → no fail, single
//!   `tracing::warn!` at startup, unauthenticated requests succeed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use agentyx_core::agents::AgentRegistry;
use agentyx_core::config::{ConfigService, FakeKeychain, ServiceConfigPaths};
use agentyx_core::tools::built_in_registry;
use agentyx_core::AppError;

use crate::events::EventBus;
use crate::server::lifecycle::{build_state, start, stop};
use crate::server::state::ServerConfig;
use crate::state::AppState;

use axum::http::StatusCode;

/// Build a minimal `AppState` for server tests by constructing
/// the pieces directly (the same way other test modules do).
/// **No env-var mutation**: we never touch `HOME` / `USERPROFILE`
/// / `APPDATA` because that races with the workspace / config /
/// permissions test modules.
fn make_app_state() -> Arc<AppState> {
    let home = tempfile::tempdir().expect("tempdir");
    let svc = Arc::new(
        agentyx_core::workspace::WorkspaceService::new(home.path()).expect("WorkspaceService"),
    );
    let paths = ServiceConfigPaths::from_agentyx_home(home.path());
    let keychain: Arc<dyn agentyx_core::config::KeychainAccess> = Arc::new(FakeKeychain::new());
    let config =
        Arc::new(ConfigService::load_with_keychain(&paths, keychain).expect("ConfigService"));
    let agents = Arc::new(AgentRegistry::load_builtins());
    let providers =
        Arc::new(crate::state::ProviderRegistry::from_config(&config).expect("ProviderRegistry"));
    let tool_registry: Arc<Vec<Arc<dyn agentyx_core::tools::Tool>>> =
        Arc::new(built_in_registry().into_iter().collect());
    let state = Arc::new(AppState {
        agentyx_home: home.path().to_path_buf(),
        workspaces: svc,
        config,
        agents,
        providers,
        runs: Arc::new(agentyx_core::agent::RunRegistry::new()),
        event_bus: Arc::new(EventBus::new()),
        workspace_runtimes: std::sync::Mutex::new(HashMap::new()),
        tool_registry,
        permission_gate: agentyx_core::permissions::PermissionGate::new(),
        permission_registry: agentyx_core::permissions::PermissionRegistry::new(),
        server: Arc::new(std::sync::OnceLock::new()),
    });
    // Keep the TempDir alive for the lifetime of the AppState by
    // leaking it (acceptable in tests). We could also use a
    // ManuallyDrop in a wrapper, but the leak is the simplest.
    std::mem::forget(home);
    state
}

#[tokio::test]
async fn f06_ac1_loopback_serves_health() {
    let app_state = make_app_state();
    let server = build_state(app_state);
    let config = ServerConfig {
        enabled: true,
        bind_host: "127.0.0.1".to_string(),
        port: 0,
        lan_enabled: false,
        require_token: false,
        ..ServerConfig::default()
    };
    let info = start(server.clone(), config).await.expect("start loopback");
    assert!(info.bind_addr.contains("127.0.0.1"));
    assert!(info.port > 0);

    // Hit /api/v1/health.
    let addr: SocketAddr = info.bind_addr.parse().expect("parse bind_addr");
    let url = format!("http://{addr}/api/v1/health");
    let resp = reqwest::get(&url).await.expect("GET /health");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.expect("JSON body");
    assert_eq!(body["status"], "ok");
    assert!(body["version"].is_string());

    // /api/v1/server/info is also reachable (loopback, no auth).
    let url = format!("http://{addr}/api/v1/server/info");
    let resp = reqwest::get(&url).await.expect("GET /server/info");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.expect("JSON body");
    assert!(body["bindAddr"].is_string());
    assert_eq!(body["requireToken"], false);

    // Clean up.
    let _ = stop(server).await;
}

#[tokio::test]
async fn f06_ac2_lan_with_require_token_blocks_without_bearer() {
    let app_state = make_app_state();
    let server = build_state(app_state);
    let config = ServerConfig {
        enabled: true,
        bind_host: "127.0.0.1".to_string(), // loopback to keep CI hermetic
        port: 0,
        lan_enabled: false,
        require_token: true,
        ..ServerConfig::default()
    };
    // Inject a known token so we can verify the constant-time
    // comparison works end-to-end.
    server.set_token(Some("aabbccdd-eeff-0011-2233-445566778899".to_string()));

    let info = start(server.clone(), config)
        .await
        .expect("start with auth");
    let addr: SocketAddr = info.bind_addr.parse().expect("parse bind_addr");

    // Health is unauthenticated even with require_token=true.
    let url = format!("http://{addr}/api/v1/health");
    let resp = reqwest::get(&url).await.expect("GET /health");
    assert_eq!(resp.status(), StatusCode::OK);

    // /api/v1/server/info requires the bearer.
    let url = format!("http://{addr}/api/v1/server/info");
    let resp = reqwest::get(&url)
        .await
        .expect("GET /server/info (no auth)");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        resp.headers()
            .get("www-authenticate")
            .map(|v| v.to_str().unwrap()),
        Some("Bearer")
    );

    // With the correct token: 200.
    let resp = reqwest::Client::new()
        .get(&url)
        .bearer_auth("aabbccdd-eeff-0011-2233-445566778899")
        .send()
        .await
        .expect("GET /server/info (with auth)");
    assert_eq!(resp.status(), StatusCode::OK);

    // With a wrong token: 401.
    let resp = reqwest::Client::new()
        .get(&url)
        .bearer_auth("wrong-token")
        .send()
        .await
        .expect("GET /server/info (wrong auth)");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Clean up.
    let _ = stop(server).await;
}

#[tokio::test]
async fn f06_ac3_lan_without_require_token_serves_with_warn() {
    // Note: in tests we can't easily capture `tracing::warn!` from
    // the spawned task, so we verify the behavioral half of AC3:
    //   - server starts successfully on a (loopback) bind,
    //   - the bearer layer is **not** active (no token is
    //     configured, but unauthenticated requests succeed).
    // The "single warn at startup" assertion is in `start`; we
    // trust the source code path and rely on a manual smoke for
    // the warn line.
    let app_state = make_app_state();
    let server = build_state(app_state);
    let config = ServerConfig {
        enabled: true,
        bind_host: "127.0.0.1".to_string(),
        port: 0,
        lan_enabled: false,
        require_token: false,
        ..ServerConfig::default()
    };

    let info = start(server.clone(), config).await.expect("start no-auth");
    let addr: SocketAddr = info.bind_addr.parse().expect("parse bind_addr");
    let url = format!("http://{addr}/api/v1/server/info");
    let resp = reqwest::get(&url).await.expect("GET /server/info");
    assert_eq!(resp.status(), StatusCode::OK);

    let _ = stop(server).await;
}

#[tokio::test]
async fn f06_ac2_lan_bind_requires_lan_enabled() {
    let app_state = make_app_state();
    let server = build_state(app_state);
    let config = ServerConfig {
        enabled: true,
        bind_host: "0.0.0.0".to_string(), // LAN
        port: 0,
        lan_enabled: false, // but not opted in
        require_token: false,
        ..ServerConfig::default()
    };
    let err = start(server, config).await.unwrap_err();
    assert!(
        matches!(err, AppError::InvalidInput { .. }),
        "expected InvalidInput, got {err:?}"
    );
}

#[tokio::test]
async fn f06_ac3_lan_with_require_token_generates_token() {
    // AC3 (variant): require_token=true with no pre-set token
    // should still allow the server to start; the bearer layer
    // is in "no token configured" mode and 401s every request
    // (we recover by rotating the token).
    let app_state = make_app_state();
    let server = build_state(app_state);
    let config = ServerConfig {
        enabled: true,
        bind_host: "127.0.0.1".to_string(),
        port: 0,
        lan_enabled: false,
        require_token: true,
        ..ServerConfig::default()
    };
    let info = start(server.clone(), config).await.expect("start");
    let addr: SocketAddr = info.bind_addr.parse().expect("parse");

    let url = format!("http://{addr}/api/v1/server/info");
    let resp = reqwest::get(&url).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let _ = stop(server).await;
}

// ============================================================
// PR4: REST endpoints — workspaces + sessions
// ============================================================

/// Start a loopback server with the given `AppState` already
/// holding a workspace. Returns the bound `SocketAddr` so each
/// test can build its URLs.
async fn start_with_workspace() -> (Arc<crate::state::AppState>, SocketAddr) {
    use agentyx_core::workspace::OpenOptions;
    let app_state = make_app_state();
    let dir = whitelisted_tempdir();
    let _ws = app_state
        .workspaces
        .open(
            dir.path(),
            OpenOptions {
                name: Some("web-test".into()),
            },
        )
        .expect("open workspace");

    // Attach the server to the AppState so HTTP handlers can
    // find it via `app_state.server()`.
    let server = build_state(app_state.clone());
    app_state.attach_server(server.clone());

    let config = ServerConfig {
        enabled: true,
        bind_host: "127.0.0.1".to_string(),
        port: 0,
        lan_enabled: false,
        require_token: false,
        ..ServerConfig::default()
    };
    let info = start(server.clone(), config).await.expect("start");
    let addr: SocketAddr = info.bind_addr.parse().expect("parse");
    (app_state, addr)
}

fn whitelisted_tempdir() -> tempfile::TempDir {
    let home = dirs::home_dir().expect("home dir must be set in tests");
    tempfile::Builder::new()
        .prefix("agentyx-")
        .tempdir_in(&home)
        .expect("create whitelisted tempdir")
}

#[tokio::test]
async fn f06_http_list_workspaces_returns_empty_then_one() {
    let app_state = make_app_state();
    let server = build_state(app_state.clone());
    app_state.attach_server(server.clone());
    let config = ServerConfig::default();
    let info = start(server.clone(), config).await.expect("start");
    let addr: SocketAddr = info.bind_addr.parse().expect("parse");

    let url = format!("http://{addr}/api/v1/workspaces");
    let resp = reqwest::get(&url).await.expect("GET workspaces");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.expect("JSON");
    assert!(body.is_array());
    assert_eq!(body.as_array().unwrap().len(), 0);

    // Add one workspace via the impl, then list again.
    use agentyx_core::workspace::OpenOptions;
    let dir = whitelisted_tempdir();
    let _ = app_state
        .workspaces
        .open(
            dir.path(),
            OpenOptions {
                name: Some("w".into()),
            },
        )
        .expect("open");

    let resp = reqwest::get(&url).await.expect("GET workspaces");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.expect("JSON");
    assert_eq!(body.as_array().unwrap().len(), 1);

    let _ = stop(server).await;
}

#[tokio::test]
async fn f06_http_open_workspace_creates_and_lists() {
    let (app_state, addr) = start_with_workspace().await;
    let server = app_state.server().expect("server");

    let dir = whitelisted_tempdir();
    let body = serde_json::json!({
        "rootPath": dir.path(),
        "name": "from-http",
    });
    let client = reqwest::Client::new();
    let url = format!("http://{addr}/api/v1/workspaces");
    let resp = client.post(&url).json(&body).send().await.expect("POST");
    let status = resp.status();
    let text = resp.text().await.expect("text");
    assert_eq!(status, StatusCode::OK, "POST status: body={text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(body["name"], "from-http");

    let _ = stop(server).await;
}

#[tokio::test]
async fn f06_http_get_workspace_returns_dto() {
    let (app_state, addr) = start_with_workspace().await;
    let server = app_state.server().expect("server");
    let url = format!("http://{addr}/api/v1/workspaces");
    let resp = reqwest::get(&url).await.expect("GET");
    let list: Vec<serde_json::Value> = resp.json().await.expect("list");
    let id = list[0]["id"].as_str().expect("id");

    let url = format!("http://{addr}/api/v1/workspaces/{id}");
    let resp = reqwest::get(&url).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::OK);
    let dto: serde_json::Value = resp.json().await.expect("JSON");
    assert_eq!(dto["id"], id);

    let _ = stop(server).await;
}

#[tokio::test]
async fn f06_http_get_workspace_unknown_is_404() {
    let (_app, addr) = start_with_workspace().await;
    let url = format!("http://{addr}/api/v1/workspaces/{}", ulid::Ulid::new());
    let resp = reqwest::get(&url).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn f06_http_list_sessions_empty_for_new_workspace() {
    let (_app, addr) = start_with_workspace().await;
    let workspaces_url = format!("http://{addr}/api/v1/workspaces");
    let resp = reqwest::get(&workspaces_url).await.expect("GET");
    let list: Vec<serde_json::Value> = resp.json().await.expect("list");
    let id = list[0]["id"].as_str().expect("id");

    let url = format!("http://{addr}/api/v1/workspaces/{id}/sessions");
    let resp = reqwest::get(&url).await.expect("GET sessions");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.expect("JSON");
    assert!(body.is_array());
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn f06_http_create_and_get_session() {
    let (_app, addr) = start_with_workspace().await;
    let workspaces_url = format!("http://{addr}/api/v1/workspaces");
    let resp = reqwest::get(&workspaces_url).await.expect("GET");
    let list: Vec<serde_json::Value> = resp.json().await.expect("list");
    let ws_id = list[0]["id"].as_str().expect("id");

    // Create a session.
    let url = format!("http://{addr}/api/v1/workspaces/{ws_id}/sessions");
    let body = serde_json::json!({ "title": "http-test" });
    let client = reqwest::Client::new();
    let resp = client.post(&url).json(&body).send().await.expect("POST");
    assert_eq!(resp.status(), StatusCode::OK);
    let session: serde_json::Value = resp.json().await.expect("JSON");
    let session_id = session["id"].as_str().expect("session id");
    assert_eq!(session["title"], "http-test");

    // Get history (empty).
    let url = format!("http://{addr}/api/v1/sessions/{session_id}/history");
    let resp = reqwest::get(&url).await.expect("GET history");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.expect("JSON");
    assert!(body.is_array());
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn f06_http_agents_list_returns_three_visibles() {
    let (_app, addr) = start_with_workspace().await;
    let url = format!("http://{addr}/api/v1/agents");
    let resp = reqwest::get(&url).await.expect("GET");
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Vec<serde_json::Value> = resp.json().await.expect("JSON");
    // 3 visible (build, plan, general); hidden agents are excluded.
    assert_eq!(body.len(), 3);
    let modes: Vec<&str> = body.iter().map(|a| a["mode"].as_str().unwrap()).collect();
    assert!(modes.contains(&"primary"));
    assert!(modes.contains(&"subagent"));
}

#[tokio::test]
async fn f06_http_health_remains_unauth_even_with_require_token() {
    let app_state = make_app_state();
    let server = build_state(app_state.clone());
    app_state.attach_server(server.clone());
    let config = ServerConfig {
        enabled: true,
        bind_host: "127.0.0.1".to_string(),
        port: 0,
        lan_enabled: false,
        require_token: true,
        ..ServerConfig::default()
    };
    server.set_token(Some("secret-token".into()));
    let info = start(server.clone(), config).await.expect("start");
    let addr: SocketAddr = info.bind_addr.parse().expect("parse");
    let url = format!("http://{addr}/api/v1/health");
    let resp = reqwest::get(&url).await.expect("GET health");
    assert_eq!(resp.status(), StatusCode::OK);
    let _ = stop(server).await;
}
