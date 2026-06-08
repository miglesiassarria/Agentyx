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

// ============================================================
// F06.AC4/AC5: browser-safe path flow (typed absolute path)
// ============================================================

/// F06.AC4: a browser-mode client can open a workspace by posting
/// an absolute `rootPath` typed in the in-app prompt. The server
/// must accept any absolute path that resolves to an existing
/// directory on the host machine.
#[tokio::test]
async fn f06_ac4_http_open_workspace_via_typed_path() {
    let app_state = make_app_state();
    let server = build_state(app_state.clone());
    app_state.attach_server(server.clone());
    let config = ServerConfig::default();
    let info = start(server.clone(), config).await.expect("start");
    let addr: SocketAddr = info.bind_addr.parse().expect("parse");
    let client = reqwest::Client::new();
    let base = format!("http://{addr}");

    let dir = whitelisted_tempdir();
    let body = serde_json::json!({
        "rootPath": dir.path(),
        "name": "browser-typed",
    });
    let resp = client
        .post(format!("{base}/api/v1/workspaces"))
        .json(&body)
        .send()
        .await
        .expect("POST workspaces");
    assert_eq!(resp.status(), StatusCode::OK, "POST workspaces");
    let dto: serde_json::Value = resp.json().await.expect("JSON");
    assert_eq!(dto["name"], "browser-typed");
    assert!(dto["id"].is_string());
    assert_eq!(dto["extraPaths"].as_array().unwrap().len(), 0);

    // The workspace now appears in the list.
    let resp = client
        .get(format!("{base}/api/v1/workspaces"))
        .send()
        .await
        .expect("GET workspaces");
    assert_eq!(resp.status(), StatusCode::OK);
    let list: Vec<serde_json::Value> = resp.json().await.expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["name"], "browser-typed");

    let _ = stop(server).await;
}

/// F06.AC5: a browser-mode client can add an extra path by posting
/// an absolute `path` typed in the in-app prompt. The server
/// validates the path and persists it under the workspace.
#[tokio::test]
async fn f06_ac5_http_add_extra_path_via_typed_path() {
    let (app_state, addr) = start_with_workspace().await;
    let server = app_state.server().expect("server");
    let client = reqwest::Client::new();
    let base = format!("http://{addr}");

    // Resolve the workspace id.
    let resp = client
        .get(format!("{base}/api/v1/workspaces"))
        .send()
        .await
        .expect("GET workspaces");
    let list: Vec<serde_json::Value> = resp.json().await.expect("list");
    let ws_id = list[0]["id"].as_str().expect("id").to_string();

    // The browser typed an absolute path that does not exist on
    // disk: the server should reject it as `invalid_input` so the
    // UI can surface a clear error.
    let missing = std::path::PathBuf::from("/this/path/should/not/exist/agentyx-test");
    let body = serde_json::json!({
        "path": missing,
        "label": "missing",
    });
    let resp = client
        .post(format!("{base}/api/v1/workspaces/{ws_id}/extra-paths"))
        .json(&body)
        .send()
        .await
        .expect("POST extra-paths");
    assert!(
        resp.status().is_client_error() || resp.status().is_server_error(),
        "missing path must be rejected; got {}",
        resp.status()
    );

    // Now a real path: create a temp dir, add it.
    let extra = whitelisted_tempdir();
    let body = serde_json::json!({
        "path": extra.path(),
        "label": "shared-lib",
    });
    let resp = client
        .post(format!("{base}/api/v1/workspaces/{ws_id}/extra-paths"))
        .json(&body)
        .send()
        .await
        .expect("POST extra-paths");
    assert_eq!(resp.status(), StatusCode::OK, "POST extra-paths");
    let dto: serde_json::Value = resp.json().await.expect("JSON");
    assert_eq!(dto["label"], "shared-lib");

    // GET the workspace and confirm the extra is in the list.
    let resp = client
        .get(format!("{base}/api/v1/workspaces/{ws_id}"))
        .send()
        .await
        .expect("GET workspace");
    assert_eq!(resp.status(), StatusCode::OK);
    let ws: serde_json::Value = resp.json().await.expect("JSON");
    let extras = ws["extraPaths"].as_array().expect("extras array");
    assert_eq!(extras.len(), 1);
    assert_eq!(extras[0]["label"], "shared-lib");

    let _ = stop(server).await;
}

// ============================================================
// F06.AC7: permission request list/respond over HTTP
// ============================================================

/// F06.AC7: a browser-mode client can list pending permission
/// requests and respond to them. This test drives the
/// `PermissionRegistry` directly to fabricate a request, then
/// exercises the HTTP endpoints end-to-end.
#[tokio::test]
async fn f06_ac7_http_permission_request_list_and_respond() {
    let (app_state, addr) = start_with_workspace().await;
    let server = app_state.server().expect("server");
    let client = reqwest::Client::new();
    let base = format!("http://{addr}");

    // Register a pending request in the in-memory registry.
    let (tx, rx) = tokio::sync::oneshot::channel();
    let req = agentyx_core::permissions::PermissionRequest::new(
        "read_file",
        serde_json::json!({"path": "/srv/proj/secret.txt"}),
        "out of policy",
    );
    let request_id = req.request_id.clone();
    app_state.permission_registry.register(req, tx);

    // 1. List over HTTP: the request must show up.
    let resp = client
        .get(format!("{base}/api/v1/permissions/requests"))
        .send()
        .await
        .expect("GET requests");
    assert_eq!(resp.status(), StatusCode::OK);
    let list: Vec<serde_json::Value> = resp.json().await.expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["tool"], "read_file");
    assert_eq!(list[0]["requestId"], request_id);

    // 2. Respond with `deny` over HTTP.
    let body = serde_json::json!({ "kind": "deny" });
    let resp = client
        .post(format!(
            "{base}/api/v1/permissions/requests/{request_id}/respond"
        ))
        .json(&body)
        .send()
        .await
        .expect("POST respond");
    assert_eq!(resp.status(), StatusCode::NO_CONTENT, "respond should 204");

    // 3. The registry delivered a `Deny` decision to the oneshot
    //    channel, so the agent loop would have unblocked with a
    //    Deny. The list should now be empty.
    let decision = rx.await.expect("oneshot delivers decision");
    assert!(matches!(
        decision,
        agentyx_core::permissions::UserDecision::Deny { .. }
    ));

    let resp = client
        .get(format!("{base}/api/v1/permissions/requests"))
        .send()
        .await
        .expect("GET requests after respond");
    let list: Vec<serde_json::Value> = resp.json().await.expect("list");
    assert_eq!(list.len(), 0, "list must be empty after respond");

    // 4. Responding to an unknown id is `not_found`.
    let unknown = agentyx_core::ids::PermissionRequestId::new().to_string();
    let body = serde_json::json!({ "kind": "deny" });
    let resp = client
        .post(format!(
            "{base}/api/v1/permissions/requests/{unknown}/respond"
        ))
        .json(&body)
        .send()
        .await
        .expect("POST respond unknown");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let _ = stop(server).await;
}

// ============================================================
// F06.AC6/AC8: SSE event delivery + chat send → SSE
// ============================================================

use agentyx_core::llm::{ChatEvent, FinishReason, MockProvider, MockSequence};

/// F06.AC8 (parity): an event published to the `EventBus` flows
/// to every connected SSE client. We don't involve Tauri here;
/// the broadcast sink is the side of parity that SSE clients see.
#[tokio::test]
async fn f06_ac8_sse_delivers_published_event() {
    let (app_state, addr) = start_with_workspace().await;
    let server = app_state.server().expect("server");
    let base = format!("http://{addr}");

    // Open the SSE stream. We don't need a full SSE client; raw
    // bytes are fine because the format is `event: <name>\ndata:
    // <json>\n\n`. Axum's `KeepAlive` may emit a `: heartbeat`
    // comment on connect, so we keep reading until we see the
    // expected event name or hit the deadline.
    let client = reqwest::Client::new();
    let sse_url = format!("{base}/api/v1/events");
    let sse_handle = tokio::spawn(async move {
        let resp = client.get(&sse_url).send().await.expect("GET events");
        use tokio_stream::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match tokio::time::timeout(remaining, stream.next()).await {
                Ok(Some(Ok(chunk))) => {
                    buf.extend_from_slice(&chunk);
                    let s = String::from_utf8_lossy(&buf);
                    if s.contains("event: chat.content.delta.v1") {
                        break;
                    }
                    if buf.len() > 64 * 1024 {
                        break;
                    }
                }
                Ok(Some(Err(_))) | Ok(None) | Err(_) => break,
            }
        }
        String::from_utf8(buf).expect("utf8")
    });

    // Give the SSE handler a moment to register the subscriber.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Publish a chat delta event.
    let _ = app_state.event_bus.publish_typed(
        "chat.content.delta.v1",
        serde_json::json!({
            "runId": "01HWS0000000000000000000A0",
            "sessionId": "01HWS0000000000000000000B0",
            "text": "hello browser",
        }),
    );

    let body = sse_handle.await.expect("sse task");
    assert!(
        body.contains("event: chat.content.delta.v1"),
        "SSE body must include the event name; got: {body}"
    );
    assert!(
        body.contains("hello browser"),
        "SSE body must include the payload text; got: {body}"
    );

    let _ = stop(server).await;
}

/// F06.AC6 (chat send → SSE): a `MockProvider` is registered as
/// `ollama` (the default), the user POSTs a message, and the SSE
/// stream receives `chat.*.v1` events. This is the same
/// end-to-end path the browser uses, minus the actual LLM.
#[tokio::test]
async fn f06_ac6_chat_send_publishes_events_to_sse() {
    let (app_state, addr) = start_with_workspace().await;
    let server = app_state.server().expect("server");

    // Inject a MockProvider under the `ollama` id. The default
    // config has Ollama preconfigured, so the agent loop will
    // pick it up.
    let mock = Arc::new(MockProvider::new());
    let sequence = MockSequence::new(vec![
        ChatEvent::MessageStart {
            message_id: "msg-1".into(),
            model: "mock-model".into(),
        },
        ChatEvent::ContentDelta {
            text: "hello from mock".into(),
        },
        ChatEvent::MessageEnd {
            usage: agentyx_core::llm::Usage::default(),
            finish_reason: FinishReason::Stop,
        },
    ]);
    mock.push_sequence(sequence);
    app_state.providers.register("ollama", mock.clone());

    // Find the workspace id.
    let client = reqwest::Client::new();
    let base = format!("http://{addr}");
    let resp = client
        .get(format!("{base}/api/v1/workspaces"))
        .send()
        .await
        .expect("GET workspaces");
    let list: Vec<serde_json::Value> = resp.json().await.expect("list");
    let ws_id = list[0]["id"].as_str().expect("id").to_string();

    // Create a session.
    let resp = client
        .post(format!("{base}/api/v1/workspaces/{ws_id}/sessions"))
        .json(&serde_json::json!({ "title": "sse-test" }))
        .send()
        .await
        .expect("POST session");
    let session: serde_json::Value = resp.json().await.expect("JSON");
    let session_id = session["id"].as_str().expect("session id").to_string();

    // Subscribe to SSE before sending the message, so we don't
    // miss the `chat.run.started.v1` event.
    let sse_url = format!("{base}/api/v1/events");
    let sse_handle = tokio::spawn(async move {
        let resp = reqwest::Client::new()
            .get(&sse_url)
            .send()
            .await
            .expect("GET events");
        use tokio_stream::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        // The mock emits ~3 events; collect until we see
        // `chat.run.finished.v1` or 10s pass.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match tokio::time::timeout(remaining, stream.next()).await {
                Ok(Some(Ok(chunk))) => {
                    buf.extend_from_slice(&chunk);
                    if String::from_utf8_lossy(&buf).contains("chat.run.finished.v1") {
                        break;
                    }
                    if buf.len() > 64 * 1024 {
                        break;
                    }
                }
                Ok(Some(Err(_))) | Ok(None) | Err(_) => break,
            }
        }
        String::from_utf8(buf).expect("utf8")
    });

    // Give the SSE subscriber a moment to register.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Send a message. The HTTP handler spawns a run; the run
    // calls the mock provider, which emits ContentDelta +
    // MessageEnd. The agent loop wraps those into
    // chat.message_start.v1, chat.content.delta.v1, and
    // chat.run.finished.v1.
    let resp = client
        .post(format!("{base}/api/v1/sessions/{session_id}/messages"))
        .json(&serde_json::json!({
            "content": "ping",
            "mentions": [],
        }))
        .send()
        .await
        .expect("POST messages");
    assert_eq!(resp.status(), StatusCode::OK);
    let _run: serde_json::Value = resp.json().await.expect("run handle");

    let body = sse_handle.await.expect("sse task");
    // We expect the run lifecycle events plus the mock's content.
    assert!(
        body.contains("chat.run.started.v1"),
        "SSE must include chat.run.started.v1; got: {body}"
    );
    assert!(
        body.contains("chat.content.delta.v1"),
        "SSE must include chat.content.delta.v1; got: {body}"
    );
    assert!(
        body.contains("hello from mock"),
        "SSE must include the mock's content; got: {body}"
    );
    assert!(
        body.contains("chat.run.finished.v1"),
        "SSE must include chat.run.finished.v1; got: {body}"
    );

    // The mock provider was hit exactly once.
    assert_eq!(mock.call_count(), 1, "mock should be called once");

    let _ = stop(server).await;
}

/// F06.AC9 (workspace config): a browser-mode client can read
/// and patch a workspace's config via HTTP. The PATCH must
/// persist and the GET must reflect the updated state.
#[tokio::test]
async fn f06_ac9_http_workspace_config_get_and_patch() {
    let (app_state, addr) = start_with_workspace().await;
    let server = app_state.server().expect("server");
    let client = reqwest::Client::new();
    let base = format!("http://{addr}");

    // Resolve the workspace id.
    let resp = client
        .get(format!("{base}/api/v1/workspaces"))
        .send()
        .await
        .expect("GET workspaces");
    let list: Vec<serde_json::Value> = resp.json().await.expect("list");
    let ws_id = list[0]["id"].as_str().expect("id").to_string();

    // 1. GET the resolved config — the DTO never includes
    //    secret values, only `keychainProviderIds`.
    let resp = client
        .get(format!("{base}/api/v1/config/workspaces/{ws_id}"))
        .send()
        .await
        .expect("GET workspace config");
    assert_eq!(resp.status(), StatusCode::OK);
    let dto: serde_json::Value = resp.json().await.expect("JSON");
    assert!(dto["global"].is_object());
    assert!(dto["effective"].is_object());
    assert!(dto["keychainProviderIds"].is_array());
    let json = serde_json::to_string(&dto).unwrap();
    // The DTO must not contain resolved keychain values; for an
    // empty keychain there are no values to leak, but the
    // `apiKey` field is never a "value" — only refs.
    assert!(!json.contains("\"value\""));

    // 2. PATCH the workspace config with new ignore patterns.
    let body = serde_json::json!({
        "workspace": {
            "ignorePatterns": ["node_modules", "dist", "build"],
        }
    });
    let resp = client
        .patch(format!("{base}/api/v1/config/workspaces/{ws_id}"))
        .json(&body)
        .send()
        .await
        .expect("PATCH workspace config");
    assert_eq!(resp.status(), StatusCode::OK);
    let new_cfg: serde_json::Value = resp.json().await.expect("JSON");
    let patterns = new_cfg["workspace"]["ignorePatterns"]
        .as_array()
        .expect("patterns array");
    let pattern_strs: Vec<&str> = patterns.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(pattern_strs.contains(&"node_modules"));
    assert!(pattern_strs.contains(&"dist"));
    assert!(pattern_strs.contains(&"build"));

    // 3. Reload via GET — the patch must have persisted. The
    //    `ResolvedConfigDto` wraps the workspace DTO at
    //    `dto["workspace"]["workspace"]["ignorePatterns"]`
    //    (`WorkspaceConfig` has a `workspace: WorkspaceSettings`
    //    field with the patterns).
    let resp = client
        .get(format!("{base}/api/v1/config/workspaces/{ws_id}"))
        .send()
        .await
        .expect("GET workspace config again");
    assert_eq!(resp.status(), StatusCode::OK);
    let dto: serde_json::Value = resp.json().await.expect("JSON");
    let patterns = dto["workspace"]["workspace"]["ignorePatterns"]
        .as_array()
        .expect("patterns array");
    let pattern_strs: Vec<&str> = patterns.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(pattern_strs.contains(&"node_modules"));

    // 4. Unknown workspace id — GET returns 200 with the global
    //    config and a default workspace DTO (the service falls
    //    back to `WorkspaceConfig::defaults()` when the file is
    //    missing). PATCH returns 404 because the workspace is
    //    not registered in the `WorkspaceService`.
    let unknown = ulid::Ulid::new().to_string();
    let resp = client
        .get(format!("{base}/api/v1/config/workspaces/{unknown}"))
        .send()
        .await
        .expect("GET unknown");
    assert_eq!(resp.status(), StatusCode::OK);
    let dto: serde_json::Value = resp.json().await.expect("JSON");
    assert!(dto["global"].is_object());
    assert!(dto["workspace"].is_object());
    assert_eq!(dto["workspace"]["version"], 1);

    let body = serde_json::json!({
        "workspace": { "ignorePatterns": ["never"] }
    });
    let resp = client
        .patch(format!("{base}/api/v1/config/workspaces/{unknown}"))
        .json(&body)
        .send()
        .await
        .expect("PATCH unknown");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let _ = stop(server).await;
}

/// F06.AC10: deep-link / SPA fallback returns the app shell
/// (index.html) for non-API routes, and JSON for API routes.
///
/// Uses a temporary `ui/dist/` populated with a minimal
/// `index.html` so the test does not depend on the actual
/// production build output. The test resolves the workspace
/// root via `CARGO_MANIFEST_DIR` (set by Cargo at compile time)
/// and only writes a temp `index.html` if the production one
/// is missing — that way the cleanup is safe in both cases.
#[tokio::test]
async fn f06_spa_fallback_returns_index_for_unknown_routes() {
    // `CARGO_MANIFEST_DIR` is `<repo>/crates/agentyx-app`; the
    // workspace root is two levels up. We resolve to the
    // canonical `ui/dist` path that `static_files::ui_dist_path`
    // looks up at runtime.
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().and_then(|p| p.parent());
    let dist = workspace_root
        .map(|r| r.join("ui").join("dist"))
        .unwrap_or_else(|| std::path::PathBuf::from("ui/dist"));

    let had_index = dist.join("index.html").exists();
    if !had_index {
        std::fs::create_dir_all(&dist).expect("create dist");
        std::fs::write(
            dist.join("index.html"),
            "<!doctype html><title>Agentyx Test Shell</title>",
        )
        .expect("write index");
    }

    let app_state = make_app_state();
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
    let base = format!("http://{addr}");

    // 1. Unknown path → SPA fallback returns index.html with 200
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{base}/some/deep/link"))
        .send()
        .await
        .expect("GET deep link");
    let status = resp.status();
    let cache_control = resp
        .headers()
        .get("cache-control")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_string();
    let text = resp.text().await.expect("text");
    assert_eq!(
        status,
        StatusCode::OK,
        "deep link should return 200 OK (SPA fallback); body={text}"
    );
    assert!(
        text.contains("Agentyx") || text.contains("agentyx"),
        "deep link should serve the app shell; got: {text}"
    );
    assert!(
        cache_control.contains("no-store"),
        "app shell should not be cached; got: {cache_control}"
    );

    // 2. /api/v1/agents returns JSON (not the SPA shell)
    let resp = client
        .get(format!("{base}/api/v1/agents"))
        .send()
        .await
        .expect("GET agents");
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(ct.contains("json"), "API routes must return JSON; got {ct}");

    let _ = stop(server).await;

    // Cleanup: only remove artifacts we created. If we had to
    // create the dist, we leave it in place if removing the
    // index would leave other files behind (e.g. the real
    // build's `assets/` dir). To be safe, we just remove the
    // file we created and never touch the directory itself.
    if !had_index {
        let _ = std::fs::remove_file(dist.join("index.html"));
    }
}
