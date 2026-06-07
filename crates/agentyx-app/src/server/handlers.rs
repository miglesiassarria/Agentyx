//! HTTP handlers for workspace endpoints (F06).
//!
//! Each handler is a thin wrapper over the corresponding
//! `*_impl` in `commands::workspace`. The HTTP layer adds
//! the Axum-specific extractors (`Path`, `Query`, `Json`) and
//! the JSON request/response shapes; all business logic lives
//! in the `_impl` functions and is shared with the Tauri
//! command wrappers.

use std::path::PathBuf;
use std::sync::Arc;

use agentyx_core::ids::WorkspaceId;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::commands::workspace as ws;
use crate::server::state::ServerState;

// Helper: map an `AppError` to an HTTP response with the
// `{code, message, context?}` shape. We centralize this so every
// handler returns consistent error semantics across the API.
fn app_error_to_response(err: agentyx_core::AppError) -> axum::response::Response {
    let status = match err.code() {
        "not_found" => StatusCode::NOT_FOUND,
        "invalid_input" => StatusCode::BAD_REQUEST,
        "forbidden" | "permission_denied" => StatusCode::FORBIDDEN,
        "conflict" => StatusCode::CONFLICT,
        "timeout" => StatusCode::GATEWAY_TIMEOUT,
        "provider" => StatusCode::BAD_GATEWAY,
        "tool" => StatusCode::UNPROCESSABLE_ENTITY,
        "path_outside_workspace" => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let body = serde_json::json!({
        "code": err.code(),
        "message": err.to_string(),
    });
    (status, Json(body)).into_response()
}

fn app_state(server: &Arc<ServerState>) -> Arc<crate::state::AppState> {
    server.app_state()
}

// ===== Workspaces =====

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ListWorkspacesQuery {
    pub limit: Option<u32>,
}

pub async fn list_workspaces(
    State(server): State<Arc<ServerState>>,
    _q: Query<ListWorkspacesQuery>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match ws::list_impl(&app.workspaces).await {
        Ok(dtos) => Json(dtos).into_response(),
        Err(e) => app_error_to_response(e),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenWorkspaceRequest {
    pub root_path: PathBuf,
    pub name: Option<String>,
}

pub async fn open_workspace(
    State(server): State<Arc<ServerState>>,
    Json(req): Json<OpenWorkspaceRequest>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match ws::open_impl(&app.workspaces, &req.root_path, req.name).await {
        Ok(dto) => Json(dto).into_response(),
        Err(e) => app_error_to_response(e),
    }
}

pub async fn get_workspace(
    State(server): State<Arc<ServerState>>,
    Path(id): Path<WorkspaceId>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match ws::get_impl(&app.workspaces, id).await {
        Ok(dto) => Json(dto).into_response(),
        Err(e) => app_error_to_response(e),
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct DeleteWorkspaceQuery {
    pub force: Option<bool>,
}

pub async fn delete_workspace(
    State(server): State<Arc<ServerState>>,
    Path(id): Path<WorkspaceId>,
    Query(q): Query<DeleteWorkspaceQuery>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match ws::delete_impl(&app.workspaces, &app.runs, id, q.force.unwrap_or(false)).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => app_error_to_response(e),
    }
}

pub async fn detect_venv(
    State(server): State<Arc<ServerState>>,
    Path(id): Path<WorkspaceId>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match ws::detect_venv_impl(&app.workspaces, id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => app_error_to_response(e),
    }
}

pub async fn effective_paths(
    State(server): State<Arc<ServerState>>,
    Path(id): Path<WorkspaceId>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match ws::effective_paths_impl(&app.workspaces, id).await {
        Ok(p) => Json(p).into_response(),
        Err(e) => app_error_to_response(e),
    }
}

// ===== Extra paths =====

pub async fn list_extra_paths(
    State(server): State<Arc<ServerState>>,
    Path(id): Path<WorkspaceId>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match ws::list_extra_paths_impl(&app.workspaces, id).await {
        Ok(dtos) => Json(dtos).into_response(),
        Err(e) => app_error_to_response(e),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddExtraPathRequest {
    pub path: PathBuf,
    pub label: Option<String>,
}

pub async fn add_extra_path(
    State(server): State<Arc<ServerState>>,
    Path(id): Path<WorkspaceId>,
    Json(req): Json<AddExtraPathRequest>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match ws::add_extra_path_impl(&app.workspaces, id, &req.path, req.label).await {
        Ok(dto) => Json(dto).into_response(),
        Err(e) => app_error_to_response(e),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveExtraPathQuery {
    pub path: PathBuf,
}

pub async fn remove_extra_path(
    State(server): State<Arc<ServerState>>,
    Path(id): Path<WorkspaceId>,
    Query(q): Query<RemoveExtraPathQuery>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match ws::remove_extra_path_impl(&app.workspaces, id, &q.path).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => app_error_to_response(e),
    }
}

// ===== list_dir =====

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListDirRequest {
    pub path: PathBuf,
}

pub async fn list_dir(
    State(server): State<Arc<ServerState>>,
    Path(id): Path<WorkspaceId>,
    Json(req): Json<ListDirRequest>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match ws::list_dir_impl(&app.workspaces, id, &req.path).await {
        Ok(entries) => Json(entries).into_response(),
        Err(e) => app_error_to_response(e),
    }
}

// ===== Sessions =====

#[derive(Debug, Deserialize, Default)]
pub struct ListSessionsQuery {
    pub limit: Option<u32>,
}

pub async fn list_sessions(
    State(server): State<Arc<ServerState>>,
    Path(id): Path<WorkspaceId>,
    _q: Query<ListSessionsQuery>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match crate::commands::session::list_sessions_impl(app, id, _q.0.limit).await {
        Ok(s) => Json(s).into_response(),
        Err(e) => app_error_to_response(e),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    pub agent_id: Option<agentyx_core::ids::AgentId>,
    pub title: Option<String>,
}

pub async fn create_session(
    State(server): State<Arc<ServerState>>,
    Path(id): Path<WorkspaceId>,
    Json(req): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match crate::commands::session::create_session_impl(app, id, req.agent_id, req.title).await {
        Ok(s) => Json(s).into_response(),
        Err(e) => app_error_to_response(e),
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct GetHistoryQuery {
    pub limit: Option<u32>,
}

pub async fn get_history(
    State(server): State<Arc<ServerState>>,
    Path(id): Path<agentyx_core::ids::SessionId>,
    Query(q): Query<GetHistoryQuery>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match crate::commands::session::get_history_impl(app, id, q.limit).await {
        Ok(m) => Json(m).into_response(),
        Err(e) => app_error_to_response(e),
    }
}

pub async fn abort_session(
    State(server): State<Arc<ServerState>>,
    Path(id): Path<agentyx_core::ids::SessionId>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match crate::commands::session::abort_impl(app, id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => app_error_to_response(e),
    }
}

pub async fn get_active_agent(
    State(server): State<Arc<ServerState>>,
    Path(id): Path<agentyx_core::ids::SessionId>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match crate::commands::session::get_active_agent_impl(app, id).await {
        Ok(a) => Json(a).into_response(),
        Err(e) => app_error_to_response(e),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetActiveAgentRequest {
    pub agent_id: agentyx_core::ids::AgentId,
}

pub async fn set_active_agent(
    State(server): State<Arc<ServerState>>,
    Path(id): Path<agentyx_core::ids::SessionId>,
    Json(req): Json<SetActiveAgentRequest>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match crate::commands::session::set_active_agent_impl(app, id, req.agent_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => app_error_to_response(e),
    }
}

// ===== Agents =====

pub async fn list_agents(State(server): State<Arc<ServerState>>) -> impl IntoResponse {
    let app = app_state(&server);
    let agents = app.agents.list();
    #[derive(Serialize)]
    struct AgentInfo {
        id: agentyx_core::ids::AgentId,
        mode: String,
        description: Option<String>,
    }
    let dtos: Vec<AgentInfo> = agents
        .iter()
        .filter(|a| !a.hidden)
        .map(|a| AgentInfo {
            id: a.id,
            mode: match a.mode {
                agentyx_core::agents::AgentMode::Primary => "primary".into(),
                agentyx_core::agents::AgentMode::Subagent => "subagent".into(),
                agentyx_core::agents::AgentMode::Hidden => "hidden".into(),
            },
            description: a.description.clone(),
        })
        .collect();
    Json(dtos).into_response()
}

pub async fn get_agent(
    State(server): State<Arc<ServerState>>,
    Path(id): Path<agentyx_core::ids::AgentId>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match app.agents.get(&id) {
        Some(a) => {
            let mode = match a.mode {
                agentyx_core::agents::AgentMode::Primary => "primary",
                agentyx_core::agents::AgentMode::Subagent => "subagent",
                agentyx_core::agents::AgentMode::Hidden => "hidden",
            };
            Json(serde_json::json!({
                "id": a.id,
                "mode": mode,
                "description": a.description,
                "model": a.model,
            }))
            .into_response()
        }
        None => app_error_to_response(agentyx_core::AppError::NotFound {
            kind: "agent".into(),
            id: id.to_string(),
        }),
    }
}

// ===== Send message (F06 AC6) =====

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    pub content: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub mentions: Vec<crate::commands::AtMention>,
}

pub async fn send_message(
    State(server): State<Arc<ServerState>>,
    Path(session_id): Path<agentyx_core::ids::SessionId>,
    Json(req): Json<SendMessageRequest>,
) -> impl IntoResponse {
    let app = app_state(&server);
    let sink: Arc<dyn agentyx_core::agent::EventSink> =
        Arc::new(crate::sink::BroadcastEventSink::new(app.event_bus.clone()));

    let (_workspace_id, session_svc, journal_svc) = match tokio::task::spawn_blocking({
        let app = app.clone();
        move || -> Result<
            (
                agentyx_core::ids::WorkspaceId,
                agentyx_core::session::SessionService,
                agentyx_core::journal::JournalRepo,
            ),
            agentyx_core::AppError,
        > {
            for workspace in app.workspaces.list() {
                let rt = app.workspace_runtime(workspace.id)?;
                if rt.session.get(session_id).is_ok() {
                    return Ok((workspace.id, (*rt.session).clone(), (*rt.journal).clone()));
                }
            }
            Err(agentyx_core::AppError::NotFound {
                kind: "session".into(),
                id: session_id.to_string(),
            })
        }
    })
    .await
    {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => return app_error_to_response(e),
        Err(e) => {
            return app_error_to_response(agentyx_core::AppError::Internal {
                message: format!("join error: {e}"),
            })
        }
    };

    let deps = agentyx_core::agent::AgentLoopDeps {
        agents: (*app.agents).clone(),
        config: (*app.config).clone(),
        providers: app.providers.to_hashmap(),
        session: session_svc,
        journal: journal_svc,
        bus: sink,
        workspaces: (*app.workspaces).clone(),
        tool_registry: app.tool_registry.clone(),
        permission_gate: app.permission_gate.clone(),
        permission_registry: app.permission_registry.clone(),
    };

    let handle = match agentyx_core::agent::spawn_run(
        deps,
        session_id,
        req.content,
        agentyx_core::agent::StartOpts::default(),
    ) {
        Ok(h) => h,
        Err(e) => return app_error_to_response(e),
    };

    let dto = crate::commands::session::RunHandleDto {
        run_id: handle.state().run_id,
        session_id: handle.state().session_id,
        agent_id: handle.state().agent_id,
        started_at: handle.state().started_at.to_rfc3339(),
    };
    app.runs.register(handle);

    (StatusCode::OK, Json(dto)).into_response()
}

// ===== Config (F06 AC9) =====

pub async fn get_config_global(State(server): State<Arc<ServerState>>) -> impl IntoResponse {
    let app = app_state(&server);
    let dto = app.config.get();
    Json(dto).into_response()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfigGlobalRequest {
    #[serde(default)]
    pub default_provider: Option<String>,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub approval_mode: Option<String>,
    #[serde(default)]
    pub providers: Option<serde_json::Value>,
}

pub async fn update_config_global(
    State(server): State<Arc<ServerState>>,
    Json(req): Json<UpdateConfigGlobalRequest>,
) -> impl IntoResponse {
    let app = app_state(&server);
    let mut patch = agentyx_core::config::GlobalConfigPatch {
        default_provider: req.default_provider,
        default_model: req.default_model,
        ..Default::default()
    };
    if let Some(mode_str) = req.approval_mode {
        if let Ok(mode) = serde_json::from_value::<agentyx_core::config::ApprovalMode>(
            serde_json::Value::String(mode_str),
        ) {
            patch.approval_mode = Some(mode);
        }
    }
    if let Some(providers_val) = req.providers {
        patch.providers = serde_json::from_value(providers_val).ok();
    }
    match app.config.update_with_patch(&patch) {
        Ok(new_cfg) => {
            let payload =
                crate::commands::config::build_config_changed_payload_global(new_cfg.clone());
            let _ = app.event_bus.publish_typed("config.changed.v1", payload);
            // Refresh providers so newly added ones are available immediately.
            let _ = app.refresh_providers();
            Json(new_cfg).into_response()
        }
        Err(e) => app_error_to_response(e),
    }
}

// ===== Providers (F06 AC9) =====

pub async fn test_provider_connection(
    State(server): State<Arc<ServerState>>,
    Json(request): Json<crate::commands::providers::TestConnectionRequest>,
) -> impl IntoResponse {
    let app = app_state(&server);
    let result = match crate::commands::providers::build_ephemeral_provider(&request, &app) {
        Ok(provider) => {
            let start = std::time::Instant::now();
            match provider.health().await {
                Ok(_) => {
                    let latency_ms = start.elapsed().as_millis() as u64;
                    let models = provider
                        .list_models()
                        .await
                        .map(|list| list.into_iter().map(|m| m.id).collect())
                        .unwrap_or_default();
                    crate::commands::providers::TestConnectionResult {
                        ok: true,
                        latency_ms: Some(latency_ms),
                        models,
                        error: None,
                        error_code: None,
                    }
                }
                Err(e) => {
                    let code = e.code().to_string();
                    crate::commands::providers::TestConnectionResult {
                        ok: false,
                        latency_ms: None,
                        models: vec![],
                        error: Some(e.to_string()),
                        error_code: Some(code),
                    }
                }
            }
        }
        Err(e) => crate::commands::providers::TestConnectionResult {
            ok: false,
            latency_ms: None,
            models: vec![],
            error: Some(e.to_string()),
            error_code: Some(e.code().to_string()),
        },
    };
    Json(result).into_response()
}

// ===== Secrets (F06 AC9) =====

pub async fn list_secret_providers(State(server): State<Arc<ServerState>>) -> impl IntoResponse {
    let app = app_state(&server);
    match app.config.list_keychain_providers() {
        Ok(ids) => Json(ids).into_response(),
        Err(e) => app_error_to_response(e),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSecretRequest {
    pub value: String,
}

pub async fn set_secret(
    State(server): State<Arc<ServerState>>,
    Path(provider_id): Path<String>,
    Json(req): Json<SetSecretRequest>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match app.config.set_keychain(&provider_id, &req.value) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => app_error_to_response(e),
    }
}

pub async fn delete_secret(
    State(server): State<Arc<ServerState>>,
    Path(provider_id): Path<String>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match app.config.delete_keychain(&provider_id) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => app_error_to_response(e),
    }
}

// ===== Permissions (F06 AC9) =====

pub async fn get_permission_matrix(State(server): State<Arc<ServerState>>) -> impl IntoResponse {
    let app = app_state(&server);
    // Synthesize matrix from static catalog + persisted overrides.
    use crate::commands::permissions::{
        default_decision_for, static_tool_names, DecisionDto, PermissionMatrixDto,
    };
    use std::collections::HashMap;

    let cfg = app.config.get();
    let mut global: HashMap<String, DecisionDto> = HashMap::new();
    for tool in static_tool_names() {
        let d = cfg
            .default_tool_decisions
            .get(*tool)
            .copied()
            .map(DecisionDto::from)
            .unwrap_or_else(|| default_decision_for(tool));
        global.insert((*tool).to_string(), d);
    }
    if matches!(cfg.approval_mode, agentyx_core::config::ApprovalMode::Deny) {
        for v in global.values_mut() {
            *v = DecisionDto::Deny;
        }
    }
    if matches!(cfg.approval_mode, agentyx_core::config::ApprovalMode::Allow) {
        for v in global.values_mut() {
            if *v == DecisionDto::Ask {
                *v = DecisionDto::Allow;
            }
        }
    }

    let effective = global.clone();
    Json(PermissionMatrixDto {
        global,
        workspace: None,
        effective,
    })
    .into_response()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetDefaultPermissionRequest {
    pub tool: String,
    pub decision: agentyx_core::config::ToolDecision,
}

pub async fn set_default_permission(
    State(server): State<Arc<ServerState>>,
    Json(req): Json<SetDefaultPermissionRequest>,
) -> impl IntoResponse {
    let app = app_state(&server);
    match app
        .config
        .set_default_tool_decision(&req.tool, req.decision)
    {
        Ok(_new_cfg) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => app_error_to_response(e),
    }
}

// ===== SSE streaming (F06 AC6/AC8) =====

use axum::response::sse::{Event, Sse};
use futures::stream::Stream;
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

/// SSE endpoint. Streams all domain events to the browser client.
/// Heartbeat (`:` comment) every 15 seconds keeps the connection alive.
pub async fn sse_events(
    State(server): State<Arc<ServerState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = server.app_state().event_bus.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| {
        result.ok().map(|event| {
            let data = serde_json::to_string(&event.payload).unwrap_or_default();
            Ok::<_, Infallible>(Event::default().event(&event.name).data(data))
        })
    });

    let heartbeat = tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(
        std::time::Duration::from_secs(15),
    ))
    .map(|_| Ok::<_, Infallible>(Event::default().comment("heartbeat")));

    let merged = tokio_stream::StreamExt::merge(stream, heartbeat);
    Sse::new(merged).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    )
}
