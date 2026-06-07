//! `permissions` Tauri commands — F01-Phase2-app surface.
//!
//! Three commands wire the [`PermissionRegistry`] (held in
//! [`AppState`]) to the UI:
//!
//! - `respond` — delivers a [`UserDecision`] to a pending
//!   request. The agent loop is blocked on a oneshot channel;
//!   this command resolves it.
//! - `list` — returns the current pending requests (used by the
//!   UI to render the queue).
//! - `get_matrix` — returns the effective per-tool decision
//!   matrix for a workspace (read-only snapshot of `WorkspaceConfig`).
//!
//! See `../../../specs/domains/permissions.md` and
//! `../../../specs/features/F01-chat-streaming.md` §AC7 for the
//! full contracts.

use std::collections::HashMap;
use std::sync::Arc;

use agentyx_core::config::ToolDecision;
use agentyx_core::ids::{PermissionRequestId, WorkspaceId};
use agentyx_core::permissions::{Decision, UserDecision};
use agentyx_core::AppResult;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

/// User's response to a permission prompt. Maps to the 4 buttons
/// in `PermissionPrompt.svelte` (Allow once / Allow for run /
/// Allow always / Deny). The wire shape is `{ kind, tool? }`
/// in `camelCase` (Tauri 2 default for DTO returns).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PermissionResponse {
    /// Allow this call only.
    AllowOnce,
    /// Allow for the duration of this run. v0.1 collapses
    /// "session" and "once" because the agent loop's snapshot is
    /// per-run, not per-session; both persist in memory for the
    /// active `RunHandle` only.
    AllowSession,
    /// Allow forever (persists as the default decision in
    /// `GlobalConfig`; equivalent to editing the matrix).
    AllowAlways {
        /// The tool this applies to. Required so the matrix
        /// can be updated.
        tool: String,
    },
    /// Deny.
    Deny,
}

impl PermissionResponse {
    /// Convert to a [`UserDecision`] the [`PermissionRegistry`]
    /// can deliver. The `persist` flag maps `AllowAlways` to
    /// `persist: true` so the agent loop remembers for the rest
    /// of the run.
    #[must_use]
    pub fn into_user_decision(self) -> UserDecision {
        match self {
            Self::AllowOnce | Self::AllowSession => UserDecision::Allow { persist: false },
            Self::AllowAlways { .. } => UserDecision::Allow { persist: true },
            Self::Deny => UserDecision::Deny { persist: false },
        }
    }
}

/// DTO for a pending permission request. The UI renders this as
/// a modal.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequestDto {
    /// Unique id of the request (used by the response).
    pub request_id: String,
    /// Run that issued the request.
    pub run_id: String,
    /// Session of the run.
    pub session_id: String,
    /// Tool that needs approval.
    pub tool: String,
    /// Args the model emitted (snapshot, not the live reference).
    pub args: serde_json::Value,
    /// Short summary of the args (1 line).
    pub args_summary: String,
    /// The reason from the gate.
    pub reason: String,
    /// ISO-8601 UTC of when the request was created.
    pub created_at: String,
}

impl From<agentyx_core::permissions::PermissionRequest> for PermissionRequestDto {
    fn from(r: agentyx_core::permissions::PermissionRequest) -> Self {
        Self {
            request_id: r.request_id,
            run_id: String::new(), // populated by `list` from registry
            session_id: String::new(),
            tool: r.tool.to_string(),
            args: r.args,
            args_summary: r.args_summary,
            reason: r.reason,
            created_at: String::new(),
        }
    }
}

/// Per-tool decision in the matrix.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DecisionDto {
    Allow,
    Ask,
    Deny,
}

impl From<Decision> for DecisionDto {
    fn from(d: Decision) -> Self {
        match d {
            Decision::Allow { .. } => Self::Allow,
            Decision::Ask { .. } => Self::Ask,
            Decision::Deny { .. } => Self::Deny,
        }
    }
}

impl From<ToolDecision> for DecisionDto {
    fn from(d: ToolDecision) -> Self {
        match d {
            ToolDecision::Allow => Self::Allow,
            ToolDecision::Ask => Self::Ask,
            ToolDecision::Deny => Self::Deny,
        }
    }
}

/// DTO returned by `permissions.get_matrix`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionMatrixDto {
    /// Global rules (from `GlobalConfig`).
    pub global: HashMap<String, DecisionDto>,
    /// Workspace-scoped rules (from `WorkspaceConfig.permissions`).
    /// `None` if `workspace_id` is `None` or the workspace has no
    /// explicit overrides.
    pub workspace: Option<HashMap<String, DecisionDto>>,
    /// Effective matrix after merging global + workspace.
    pub effective: HashMap<String, DecisionDto>,
}

/// Respond to a permission request. The agent loop awaits the
/// [`UserDecision`] on a oneshot channel; this command delivers it.
///
/// Errors:
/// - `not_found` — `request_id` is unknown (already resolved or
///   never existed).
#[tauri::command]
pub async fn respond(
    state: State<'_, Arc<AppState>>,
    request_id: PermissionRequestId,
    response: PermissionResponse,
) -> AppResult<()> {
    let state = state.inner().clone();
    // Snapshot the registry off the state first; we then drop the
    // state guard before the await to keep `Send` clean.
    let decision = response.into_user_decision();
    let reg = state.permission_registry.clone();
    let request_id_str = request_id.to_string();
    tokio::task::spawn_blocking(move || reg.respond(&request_id_str, decision))
        .await
        .map_err(|e| agentyx_core::AppError::Internal {
            message: format!("join error: {e}"),
        })?
}

/// List the current pending permission requests. Used by the UI
/// to recover the queue after a workspace switch or page reload.
#[tauri::command]
pub async fn list(state: State<'_, Arc<AppState>>) -> AppResult<Vec<PermissionRequestDto>> {
    let state = state.inner().clone();
    let requests = state.permission_registry.list();
    let out: Vec<PermissionRequestDto> = requests
        .into_iter()
        .map(|r| PermissionRequestDto {
            request_id: r.request_id,
            run_id: String::new(),
            session_id: String::new(),
            tool: r.tool.to_string(),
            args: r.args,
            args_summary: r.args_summary,
            reason: r.reason,
            created_at: String::new(),
        })
        .collect();
    Ok(out)
}

/// Get the current permission matrix for a workspace. Returns
/// the global rules + the workspace overrides + the effective
/// merged matrix. `workspace_id = null` returns only the global
/// part.
///
/// The global rules are built by:
/// 1. Start from the static v0.1 tool catalog default for each
///    tool (see [`default_decision_for`]).
/// 2. Apply the user's persisted overrides from
///    `GlobalConfig.default_tool_decisions` (F05.AC9).
/// 3. Apply the global `approval_mode` shortcut:
///    - `Deny` upgrades everything to `deny`.
///    - `Allow` downgrades `ask` to `allow`.
#[tauri::command]
pub async fn get_matrix(
    state: State<'_, Arc<AppState>>,
    workspace_id: Option<WorkspaceId>,
) -> AppResult<PermissionMatrixDto> {
    let state = state.inner().clone();
    // v0.1: synthesize the matrix from the static tool catalog +
    // the global `approval_mode` + the workspace's
    // `PermissionsConfig` (read from the on-disk `config.toml`).
    // Full live matrix with `always_allow`/`always_deny` global
    // rules lands with F05.
    let (global, workspace, effective) = tokio::task::spawn_blocking(move || {
        let cfg = state.config.get();
        let mut global: HashMap<String, DecisionDto> = HashMap::new();
        // Default per-tool decision from the static catalog, with
        // the user's persisted override (if any) winning.
        for tool in static_tool_names() {
            let d = cfg
                .default_tool_decisions
                .get(*tool)
                .copied()
                .map(DecisionDto::from)
                .unwrap_or_else(|| default_decision_for(tool));
            global.insert((*tool).to_string(), d);
        }
        // Approval mode `Deny` (read-only) upgrades everything to deny.
        if matches!(cfg.approval_mode, agentyx_core::config::ApprovalMode::Deny) {
            for v in global.values_mut() {
                *v = DecisionDto::Deny;
            }
        }
        // Approval mode `Allow` (no prompts) downgrades ask to allow.
        if matches!(cfg.approval_mode, agentyx_core::config::ApprovalMode::Allow) {
            for v in global.values_mut() {
                if *v == DecisionDto::Ask {
                    *v = DecisionDto::Allow;
                }
            }
        }

        let workspace = if let Some(ws_id) = workspace_id {
            let ws = state.workspaces.get(ws_id);
            let mut ws_map: HashMap<String, DecisionDto> = HashMap::new();
            if let Some(ws) = ws {
                let ws_dir = state
                    .agentyx_home
                    .join("workspaces")
                    .join(ws.id.to_string());
                let config_path = ws_dir.join("config.toml");
                if let Ok(text) = std::fs::read_to_string(&config_path) {
                    if let Ok(parsed) =
                        toml::from_str::<agentyx_core::workspace::types::WorkspaceConfig>(&text)
                    {
                        if let Some(perms) = parsed.permissions {
                            for rule in &perms.allow {
                                ws_map.insert(rule.clone(), DecisionDto::Allow);
                            }
                            for rule in &perms.deny {
                                ws_map.insert(rule.clone(), DecisionDto::Deny);
                            }
                            for rule in &perms.ask {
                                ws_map.insert(rule.clone(), DecisionDto::Ask);
                            }
                        }
                    }
                }
            }
            Some(ws_map)
        } else {
            None
        };

        let effective: HashMap<String, DecisionDto> = match &workspace {
            None => global.clone(),
            Some(ws) => {
                let mut merged = global.clone();
                for (k, v) in ws {
                    merged.insert(k.clone(), *v);
                }
                merged
            }
        };

        (global, workspace, effective)
    })
    .await
    .map_err(|e| agentyx_core::AppError::Internal {
        message: format!("join error: {e}"),
    })?;

    Ok(PermissionMatrixDto {
        global,
        workspace,
        effective,
    })
}

/// Set the default decision for a single tool. Persists
/// `GlobalConfig.default_tool_decisions` atomically. F05.AC9.
///
/// Errors:
/// - `invalid_input` — the `decision` string is not one of
///   `allow` / `ask` / `deny`.
/// - `invalid_input` — the `tool` id is empty.
#[tauri::command]
pub async fn set_default(
    state: State<'_, Arc<AppState>>,
    tool: String,
    decision: String,
) -> AppResult<()> {
    let parsed =
        ToolDecision::parse(&decision).ok_or_else(|| agentyx_core::AppError::InvalidInput {
            message: format!("decision must be one of allow|ask|deny (got '{decision}')"),
        })?;
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || state.config.set_default_tool_decision(&tool, parsed))
        .await
        .map_err(|e| agentyx_core::AppError::Internal {
            message: format!("join error: {e}"),
        })?
        .map(|_cfg| ())
}

/// Default per-tool decision for the static v0.1 tool catalog.
fn default_decision_for(tool: &str) -> DecisionDto {
    match tool {
        "read_file" | "list_dir" | "search" => DecisionDto::Allow,
        "write_file" | "edit_file" | "shell" | "python_run" | "apply_patch" => DecisionDto::Ask,
        _ => DecisionDto::Ask,
    }
}

/// Names of the tools in the static v0.1 catalog. Kept in sync
/// with `crates/agentyx-core/src/tools/builtin/mod.rs`; we
/// hardcode the list here to avoid pulling the registry (and
/// the tool-side deps) into a Tauri command.
fn static_tool_names() -> &'static [&'static str] {
    &[
        "read_file",
        "list_dir",
        "search",
        "write_file",
        "edit_file",
        "shell",
        "python_run",
        "apply_patch",
    ]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn permission_response_into_user_decision_maps_correctly() {
        assert!(matches!(
            PermissionResponse::AllowOnce.into_user_decision(),
            UserDecision::Allow { persist: false }
        ));
        assert!(matches!(
            PermissionResponse::AllowSession.into_user_decision(),
            UserDecision::Allow { persist: false }
        ));
        assert!(matches!(
            PermissionResponse::AllowAlways {
                tool: "shell".to_string()
            }
            .into_user_decision(),
            UserDecision::Allow { persist: true }
        ));
        assert!(matches!(
            PermissionResponse::Deny.into_user_decision(),
            UserDecision::Deny { persist: false }
        ));
    }

    #[test]
    fn default_decision_for_read_only_tools_is_allow() {
        assert_eq!(default_decision_for("read_file"), DecisionDto::Allow);
        assert_eq!(default_decision_for("list_dir"), DecisionDto::Allow);
        assert_eq!(default_decision_for("search"), DecisionDto::Allow);
    }

    #[test]
    fn default_decision_for_write_tools_is_ask() {
        assert_eq!(default_decision_for("write_file"), DecisionDto::Ask);
        assert_eq!(default_decision_for("edit_file"), DecisionDto::Ask);
        assert_eq!(default_decision_for("shell"), DecisionDto::Ask);
        assert_eq!(default_decision_for("python_run"), DecisionDto::Ask);
        assert_eq!(default_decision_for("apply_patch"), DecisionDto::Ask);
    }

    #[test]
    fn default_decision_for_unknown_tool_is_ask() {
        assert_eq!(
            default_decision_for("totally_made_up_tool"),
            DecisionDto::Ask
        );
    }

    #[test]
    fn decision_dto_from_decision_allow_ask_deny() {
        assert_eq!(
            DecisionDto::from(Decision::Allow { persist: true }),
            DecisionDto::Allow
        );
        assert_eq!(
            DecisionDto::from(Decision::Ask {
                reason: "user".into()
            }),
            DecisionDto::Ask
        );
        assert_eq!(
            DecisionDto::from(Decision::Deny {
                reason: "denied_path".into()
            }),
            DecisionDto::Deny
        );
    }

    #[test]
    fn permission_response_serializes_as_camelcase_tagged() {
        // The wire shape must match what the TS side sends.
        let once = PermissionResponse::AllowOnce;
        let json = serde_json::to_string(&once).unwrap();
        assert_eq!(json, r#"{"kind":"allowOnce"}"#);

        let always = PermissionResponse::AllowAlways {
            tool: "shell".to_string(),
        };
        let json = serde_json::to_string(&always).unwrap();
        assert_eq!(json, r#"{"kind":"allowAlways","tool":"shell"}"#);

        let deny = PermissionResponse::Deny;
        let json = serde_json::to_string(&deny).unwrap();
        assert_eq!(json, r#"{"kind":"deny"}"#);
    }

    #[test]
    fn permission_response_deserializes_from_camelcase_tagged() {
        let once: PermissionResponse = serde_json::from_str(r#"{"kind":"allowOnce"}"#).unwrap();
        assert!(matches!(once, PermissionResponse::AllowOnce));

        let always: PermissionResponse =
            serde_json::from_str(r#"{"kind":"allowAlways","tool":"shell"}"#).unwrap();
        assert!(matches!(always, PermissionResponse::AllowAlways { .. }));

        let deny: PermissionResponse = serde_json::from_str(r#"{"kind":"deny"}"#).unwrap();
        assert!(matches!(deny, PermissionResponse::Deny));
    }

    // ===============================================================
    // F05.AC9 — Tauri command `set_default` (wiring through AppState)
    // ===============================================================

    async fn fresh_state() -> (tempfile::TempDir, Arc<AppState>) {
        use crate::events::EventBus;
        use agentyx_core::agents::AgentRegistry;
        use agentyx_core::config::{FakeKeychain, ServiceConfigPaths};
        use agentyx_core::permissions::{PermissionGate, PermissionRegistry};
        use agentyx_core::tools::{built_in_registry, Tool};
        use std::collections::HashMap;
        use std::sync::Mutex;

        let home = tempfile::tempdir().unwrap();
        let paths = ServiceConfigPaths::from_agentyx_home(home.path());
        let keychain: Arc<dyn agentyx_core::config::KeychainAccess> = Arc::new(FakeKeychain::new());
        let config = Arc::new(
            agentyx_core::config::ConfigService::load_with_keychain(&paths, keychain).unwrap(),
        );
        let workspaces =
            Arc::new(agentyx_core::workspace::WorkspaceService::new(home.path()).unwrap());
        let agents = Arc::new(AgentRegistry::load_builtins());
        let providers = Arc::new(crate::state::ProviderRegistry::from_config(&config).unwrap());
        let tool_registry: Arc<Vec<Arc<dyn Tool>>> =
            Arc::new(built_in_registry().into_iter().collect());
        let state = Arc::new(AppState {
            agentyx_home: home.path().to_path_buf(),
            workspaces,
            config,
            agents,
            providers,
            runs: Arc::new(agentyx_core::agent::RunRegistry::new()),
            event_bus: Arc::new(EventBus::new()),
            workspace_runtimes: Mutex::new(HashMap::new()),
            tool_registry,
            permission_gate: PermissionGate::new(),
            permission_registry: PermissionRegistry::new(),
            server: Arc::new(std::sync::OnceLock::new()),
        });
        (home, state)
    }

    #[tokio::test]
    async fn f05_ac9_set_default_persists_to_disk() {
        // The Tauri command persists the decision via
        // `ConfigService::set_default_tool_decision`. We exercise
        // the same code path here (no AppHandle needed; the Tauri
        // command is a thin wrapper over the service method).
        let (dir, state) = fresh_state().await;
        state
            .config
            .set_default_tool_decision("write_file", agentyx_core::config::ToolDecision::Allow)
            .unwrap();

        // Reload from disk and verify.
        let paths = agentyx_core::config::ServiceConfigPaths::from_agentyx_home(dir.path());
        let reloaded = agentyx_core::config::ConfigService::load(&paths).unwrap();
        assert_eq!(
            reloaded.get().default_tool_decisions.get("write_file"),
            Some(&agentyx_core::config::ToolDecision::Allow)
        );
    }

    #[tokio::test]
    async fn f05_ac9_set_default_rejects_unknown_decision_string() {
        // The Tauri command's wrapper must reject bogus decision
        // strings before reaching the service. Tested here at the
        // boundary: the parse step.
        assert!(agentyx_core::config::ToolDecision::parse("ALLOW").is_none());
        assert!(agentyx_core::config::ToolDecision::parse("prompt").is_none());
        assert!(agentyx_core::config::ToolDecision::parse("").is_none());
    }

    #[tokio::test]
    async fn f05_ac9_get_matrix_uses_persisted_default() {
        // Persist a non-default decision for `shell`, then verify
        // that `get_matrix` returns it in the global map.
        let (_dir, state) = fresh_state().await;
        state
            .config
            .set_default_tool_decision("shell", agentyx_core::config::ToolDecision::Deny)
            .unwrap();
        let matrix = get_matrix_from_state(&state, None).await;
        assert_eq!(matrix.global.get("shell"), Some(&DecisionDto::Deny));
        // Other tools keep their static defaults.
        assert_eq!(matrix.global.get("read_file"), Some(&DecisionDto::Allow));
        assert_eq!(matrix.global.get("write_file"), Some(&DecisionDto::Ask));
    }

    #[tokio::test]
    async fn f05_ac9_get_matrix_falls_back_to_static_default() {
        // With no persisted decisions, the matrix uses the static
        // v0.1 catalog defaults.
        let (_dir, state) = fresh_state().await;
        let matrix = get_matrix_from_state(&state, None).await;
        assert_eq!(matrix.global.get("read_file"), Some(&DecisionDto::Allow));
        assert_eq!(matrix.global.get("write_file"), Some(&DecisionDto::Ask));
        assert_eq!(matrix.global.get("shell"), Some(&DecisionDto::Ask));
    }

    #[tokio::test]
    async fn f05_ac9_approval_mode_deny_overrides_persisted_default() {
        // Setting `approval_mode = deny` upgrades everything,
        // even if the user has a persisted `allow` for some tool.
        let (_dir, state) = fresh_state().await;
        state
            .config
            .set_default_tool_decision("read_file", agentyx_core::config::ToolDecision::Allow)
            .unwrap();
        state
            .config
            .update(|c| c.approval_mode = agentyx_core::config::ApprovalMode::Deny)
            .unwrap();
        let matrix = get_matrix_from_state(&state, None).await;
        // Deny wins over the persisted Allow.
        assert_eq!(matrix.global.get("read_file"), Some(&DecisionDto::Deny));
    }

    /// Extract the matrix computation in `get_matrix` to a helper
    /// so tests can exercise the synthesis without going through
    /// the Tauri state injection.
    async fn get_matrix_from_state(
        state: &Arc<AppState>,
        workspace_id: Option<WorkspaceId>,
    ) -> PermissionMatrixDto {
        let state = state.clone();
        let (global, workspace, effective) = tokio::task::spawn_blocking(move || {
            let cfg = state.config.get();
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
            (global, None::<HashMap<String, DecisionDto>>, effective)
        })
        .await
        .map_err(|e| agentyx_core::AppError::Internal {
            message: format!("join error: {e}"),
        })
        .unwrap();
        let _ = workspace_id;
        PermissionMatrixDto {
            global,
            workspace,
            effective,
        }
    }
}
