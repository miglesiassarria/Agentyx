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
        // Default per-tool decision from the static catalog.
        for tool in static_tool_names() {
            let d = default_decision_for(tool);
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
}
