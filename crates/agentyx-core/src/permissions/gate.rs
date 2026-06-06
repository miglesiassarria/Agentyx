//! Permission matrix and the [`PermissionGate`] that decides
//! whether a tool call is allowed, denied, or needs user approval.
//!
//! Per `specs/domains/permissions.md` §Operations, the algorithm
//! is a fixed ladder:
//!
//! 1. Path traversal (`..` literal) → `Deny`.
//! 2. Path outside `root_path ∪ extra_paths` → `Deny`.
//! 3. Tool in `always_deny` global → `Deny`.
//! 4. `approval_mode = "never"` and tool is dangerous → `Deny`.
//! 5. Path matches `deny_paths` (glob) → `Deny`.
//! 6. Path matches `extra_paths.deny` (glob) → `Deny`.
//! 7. `allow_paths` non-empty and path not in it → `Deny`.
//! 8. Tool in `always_allow` global → `Allow`.
//! 9. Tool in `allow` of workspace → `Allow`.
//! 10. Tool in `deny` of workspace → `Deny`.
//! 11. Tool in `ask` of workspace → `Ask` (or `Allow` if mode=auto).
//! 12. Unknown tool → `Ask { reason: "unknown_tool" }`.
//!
//! The gate is **stateless**; it takes a [`PermissionSnapshot`]
//! (built once per run from config + workspace + agent override)
//! and produces a [`Decision`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use globset::GlobMatcher;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::tools::ToolId;

/// The decision of [`PermissionGate::check`]. One of three
/// variants: allow, ask the user, or deny.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "kind")]
pub enum Decision {
    /// Run the tool.
    Allow {
        /// If `true`, persist this decision for the rest of the
        /// session (i.e. skip future prompts for the same tool).
        persist: bool,
    },
    /// Pause the run and ask the user. The agent loop emits
    /// `permission.requested.v1`; the user responds via
    /// [`PermissionRequest::resolve`].
    Ask {
        /// Stable, machine-readable reason. Used by the UI to
        /// customize the prompt.
        reason: String,
    },
    /// Do not run the tool. The agent loop surfaces a
    /// `tool_result.v1 { isError: true, output: "denied: <reason>" }`
    /// to the model.
    Deny {
        /// Short, machine-readable reason (e.g. `denied_path`,
        /// `always_deny`, `approval_mode_never`).
        reason: String,
    },
}

impl Decision {
    /// Short, stable string code for logging / the UI.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Allow { .. } => "allow",
            Self::Ask { .. } => "ask",
            Self::Deny { .. } => "deny",
        }
    }
}

/// Global approval mode (lives in the user's `~/.agentyx/config.toml`).
///
/// In v0.1 only `Ask` and `Auto` are consumed by the gate;
/// `Never` is plumbed for the read-only mode toggle.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalMode {
    /// Prompt the user on each `Ask` decision.
    #[default]
    Ask,
    /// Skip the prompt — `Ask` is treated as `Allow { persist: false }`.
    /// `Deny` rules and `deny_paths` still apply.
    Auto,
    /// Read-only — every dangerous tool is denied with
    /// `approval_mode_never`.
    Never,
}

/// The full snapshot of permission state used by
/// [`PermissionGate::check`]. Built once per run from the
/// workspace config, the global config, and the active agent's
/// `AgentPermissionOverride`.
#[derive(Debug, Clone)]
pub struct PermissionSnapshot {
    /// Workspace root (canonical).
    pub workspace_root: PathBuf,
    /// Workspace extra paths (canonical).
    pub extra_paths: Vec<PathBuf>,
    /// Global approval mode.
    pub approval_mode: ApprovalMode,
    /// Tools allowed at the workspace level.
    pub workspace_allow: Vec<String>,
    /// Tools denied at the workspace level.
    pub workspace_deny: Vec<String>,
    /// Tools that require `Ask` at the workspace level.
    pub workspace_ask: Vec<String>,
    /// Path globs denied at the workspace level.
    pub deny_paths: Vec<String>,
    /// Path globs explicitly allowed (if non-empty, paths must
    /// match this list to be writeable).
    pub allow_paths: Vec<String>,
    /// Per-extras denials (path globs).
    pub extra_paths_deny: Vec<String>,
    /// Global "always allow" tool set.
    pub always_allow: Vec<String>,
    /// Global "always deny" tool set.
    pub always_deny: Vec<String>,
    /// Per-agent override allow.
    pub agent_allow: Vec<String>,
    /// Per-agent override deny.
    pub agent_deny: Vec<String>,
    /// Per-agent override ask.
    pub agent_ask: Vec<String>,
}

impl PermissionSnapshot {
    /// Compile path globs into matchers. Called once per snapshot
    /// build; matchers are stored on [`PermissionGate`] (not the
    /// snapshot, to keep the snapshot `Clone`).
    pub fn compile_globs(&self) -> CompiledGlobs {
        CompiledGlobs {
            deny_paths: compile(&self.deny_paths),
            allow_paths: compile(&self.allow_paths),
            extra_paths_deny: compile(&self.extra_paths_deny),
        }
    }
}

/// Pre-compiled glob matchers, used by [`PermissionGate`].
#[derive(Debug, Default, Clone)]
pub struct CompiledGlobs {
    pub(crate) deny_paths: Vec<GlobMatcher>,
    pub(crate) allow_paths: Vec<GlobMatcher>,
    pub(crate) extra_paths_deny: Vec<GlobMatcher>,
}

fn compile(patterns: &[String]) -> Vec<GlobMatcher> {
    patterns
        .iter()
        .filter_map(|p| globset::Glob::new(p).ok())
        .map(|g| g.compile_matcher())
        .collect()
}

/// The gate. Cheap to clone (state is `Arc`).
#[derive(Debug, Clone)]
pub struct PermissionGate {
    compiled: Arc<CompiledGlobs>,
}

impl Default for PermissionGate {
    fn default() -> Self {
        Self::new()
    }
}

impl PermissionGate {
    /// Create a gate with no pre-compiled globs. Use
    /// [`PermissionGate::with_globs`] if you have a snapshot.
    #[must_use]
    pub fn new() -> Self {
        Self {
            compiled: Arc::new(CompiledGlobs::default()),
        }
    }

    /// Create a gate from a pre-compiled globs set.
    #[must_use]
    pub fn with_globs(compiled: CompiledGlobs) -> Self {
        Self {
            compiled: Arc::new(compiled),
        }
    }

    /// Evaluate a tool call.
    ///
    /// `tool` is the tool name. `args` is the JSON object the LLM
    /// emitted. The gate extracts string-typed path-like args
    /// (any key ending in `path`, plus the well-known `cwd`)
    /// and runs them through the sandbox check.
    #[must_use]
    pub fn check(
        &self,
        snap: &PermissionSnapshot,
        tool: &str,
        args: &serde_json::Value,
    ) -> Decision {
        // Step 1+2: path safety. Reject any path that's outside
        // the sandbox or contains `..`.
        let paths: Vec<&str> = extract_path_args(args);
        let mut sandbox_decision: Option<Decision> = None;
        for raw in &paths {
            // Step 1: literal `..` → deny.
            if raw.split(['/', '\\']).any(|seg| seg == "..") {
                sandbox_decision = Some(Decision::Deny {
                    reason: "path_traversal".into(),
                });
                break;
            }
            // Step 2: outside root ∪ extras → deny.
            let candidate = Path::new(raw);
            let joined = if candidate.is_absolute() {
                candidate.to_path_buf()
            } else {
                snap.workspace_root.join(candidate)
            };
            // Canonicalize (symlink-resolve) so we compare
            // canonical-to-canonical — necessary on macOS where
            // `/tmp` is a symlink to `/private/tmp`.
            let abs = crate::permissions::sandbox::canonicalize_any(&joined).unwrap_or(joined);
            let in_root = abs.starts_with(&snap.workspace_root);
            let in_extra = snap.extra_paths.iter().any(|e| abs.starts_with(e));
            if !in_root && !in_extra {
                sandbox_decision = Some(Decision::Deny {
                    reason: "path_outside_workspace".into(),
                });
                break;
            }
        }
        if let Some(d) = sandbox_decision {
            return d;
        }

        // Step 3: always_deny.
        if snap.always_deny.iter().any(|t| matches_tool(t, tool)) {
            return Decision::Deny {
                reason: "always_deny".into(),
            };
        }

        // Step 4: approval_mode=never + dangerous tool.
        if matches!(snap.approval_mode, ApprovalMode::Never) {
            // We don't know is_dangerous at the gate level (the
            // gate doesn't know about tool metadata). The agent
            // loop pre-filters dangerous tools to a synthetic
            // list; here we treat any tool not in `allow` as
            // potentially dangerous in `Never` mode.
            if !snap.workspace_allow.iter().any(|t| matches_tool(t, tool))
                && !snap.always_allow.iter().any(|t| matches_tool(t, tool))
            {
                return Decision::Deny {
                    reason: "approval_mode_never".into(),
                };
            }
        }

        // Step 5: deny_paths (path globs).
        if !self.compiled.deny_paths.is_empty() && !paths.is_empty() {
            for raw in &paths {
                let candidate = Path::new(raw);
                let abs = canonicalized_abs(candidate, &snap.workspace_root);
                if let Ok(rel) = abs.strip_prefix(&snap.workspace_root) {
                    for m in &self.compiled.deny_paths {
                        if m.is_match(rel) {
                            return Decision::Deny {
                                reason: "denied_path".into(),
                            };
                        }
                    }
                }
                // Also match within extras.
                for extra in &snap.extra_paths {
                    if let Ok(rel) = abs.strip_prefix(extra) {
                        for m in &self.compiled.deny_paths {
                            if m.is_match(rel) {
                                return Decision::Deny {
                                    reason: "denied_path".into(),
                                };
                            }
                        }
                    }
                }
            }
        }

        // Step 6: extra_paths.deny (path globs, only inside an extra).
        if !self.compiled.extra_paths_deny.is_empty() && !paths.is_empty() {
            for raw in &paths {
                let candidate = Path::new(raw);
                let abs = canonicalized_abs(candidate, &snap.workspace_root);
                for extra in &snap.extra_paths {
                    if let Ok(rel) = abs.strip_prefix(extra) {
                        for m in &self.compiled.extra_paths_deny {
                            if m.is_match(rel) {
                                return Decision::Deny {
                                    reason: "denied_path_in_extra".into(),
                                };
                            }
                        }
                    }
                }
            }
        }

        // Step 7: allow_paths non-empty and path not in it.
        if !self.compiled.allow_paths.is_empty() && !paths.is_empty() {
            let mut any_in = false;
            for raw in &paths {
                let candidate = Path::new(raw);
                let abs = canonicalized_abs(candidate, &snap.workspace_root);
                let rel = abs
                    .strip_prefix(&snap.workspace_root)
                    .ok()
                    .or_else(|| {
                        snap.extra_paths
                            .iter()
                            .find_map(|e| abs.strip_prefix(e).ok())
                    })
                    .unwrap_or(&abs);
                for m in &self.compiled.allow_paths {
                    if m.is_match(rel) {
                        any_in = true;
                        break;
                    }
                }
                if any_in {
                    break;
                }
            }
            if !any_in {
                return Decision::Deny {
                    reason: "path_not_in_allow_list".into(),
                };
            }
        }

        // Step 8: always_allow.
        if snap.always_allow.iter().any(|t| matches_tool(t, tool)) {
            return Decision::Allow { persist: true };
        }

        // Step 9-11: workspace allow / deny / ask.
        if snap.workspace_allow.iter().any(|t| matches_tool(t, tool))
            || snap.agent_allow.iter().any(|t| matches_tool(t, tool))
        {
            return Decision::Allow { persist: true };
        }
        if snap.workspace_deny.iter().any(|t| matches_tool(t, tool))
            || snap.agent_deny.iter().any(|t| matches_tool(t, tool))
        {
            return Decision::Deny {
                reason: "denied".into(),
            };
        }
        if snap.workspace_ask.iter().any(|t| matches_tool(t, tool))
            || snap.agent_ask.iter().any(|t| matches_tool(t, tool))
        {
            return match snap.approval_mode {
                ApprovalMode::Auto => Decision::Allow { persist: false },
                _ => Decision::Ask {
                    reason: "user_approval".into(),
                },
            };
        }

        // Step 12: unknown tool → safe default.
        Decision::Ask {
            reason: "unknown_tool".into(),
        }
    }
}

/// Extract path-shaped args from a JSON object. Any key whose
/// name ends with `path` or equals `cwd` is treated as a path.
fn extract_path_args(args: &serde_json::Value) -> Vec<&str> {
    let Some(obj) = args.as_object() else {
        return Vec::new();
    };
    let mut out: Vec<&str> = Vec::new();
    for (k, v) in obj {
        let lower = k.to_ascii_lowercase();
        let is_path = lower == "cwd" || lower.ends_with("path");
        if !is_path {
            continue;
        }
        if let Some(s) = v.as_str() {
            out.push(s);
        }
    }
    out
}

/// Match a tool name against a rule. The rule can be a bare name
/// (`"read_file"`) or a prefix glob (`"shell:*"`). We only
/// support `*` as a wildcard.
fn matches_tool(rule: &str, tool: &str) -> bool {
    if rule == "*" {
        return true;
    }
    if let Some(prefix) = rule.strip_suffix(":*") {
        return tool.starts_with(prefix);
    }
    rule == tool
}

/// Join a candidate path with the workspace root (if relative)
/// and canonicalize the result. Falls back to the joined path if
/// canonicalization fails (e.g. the path doesn't exist).
fn canonicalized_abs(candidate: &Path, workspace_root: &Path) -> PathBuf {
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        workspace_root.join(candidate)
    };
    crate::permissions::sandbox::canonicalize_any(&joined).unwrap_or(joined)
}

/// A pending permission request. Created by the agent loop when
/// the gate returns `Ask`, resolved by the user via the
/// `permission_respond` Tauri command.
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    /// Unique id of the request (used by the response).
    pub request_id: String,
    /// Tool that needs approval.
    pub tool: ToolId,
    /// Args the model emitted (snapshot, not the live reference).
    pub args: serde_json::Value,
    /// Short summary of the args (1 line).
    pub args_summary: String,
    /// The reason from the gate.
    pub reason: String,
}

impl PermissionRequest {
    /// Build a new request. Generates a fresh ULID for
    /// `request_id`.
    #[must_use]
    pub fn new(tool: ToolId, args: serde_json::Value, reason: impl Into<String>) -> Self {
        let args_summary = crate::agent::summarize_pub(&args.to_string(), 120);
        Self {
            request_id: Ulid::new().to_string(),
            tool,
            args,
            args_summary,
            reason: reason.into(),
        }
    }
}

/// A user response to a permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "kind")]
pub enum UserDecision {
    /// Allow the tool to run. If `persist: true`, this decision
    /// is remembered for the rest of the session (and the global
    /// `always_allow` is updated in v1.1).
    Allow {
        /// Persist the decision.
        #[serde(default)]
        persist: bool,
    },
    /// Deny the tool call.
    Deny {
        /// Persist the decision.
        #[serde(default)]
        persist: bool,
    },
}

impl UserDecision {
    /// Convert to the gate's `Decision` (without going through
    /// `Ask`). For use after the user has answered.
    #[must_use]
    pub fn into_decision(self) -> Decision {
        match self {
            Self::Allow { persist } => Decision::Allow { persist },
            Self::Deny { .. } => Decision::Deny {
                reason: "user".into(),
            },
        }
    }
}

/// Aggregator of pending permission requests, keyed by
/// `request_id`. Held in `AppState`; the agent loop creates
/// entries when it emits `permission.requested.v1`, and the
/// `permission_respond` command removes them.
///
/// Each entry is paired with a [`tokio::sync::oneshot::Sender`]
/// that the command uses to deliver the [`UserDecision`]. The
/// agent loop awaits the corresponding receiver; if the user
/// never answers, the loop can be aborted via the
/// `RunHandle::abort` flag, which we honour on the receiver
/// side via [`PendingPermission::wait`].
#[derive(Debug, Default, Clone)]
pub struct PermissionRegistry {
    inner: Arc<std::sync::Mutex<std::collections::HashMap<String, PendingPermission>>>,
}

/// A pending permission request: the request data plus the
/// oneshot channel that the [`PermissionRegistry::respond`]
/// method will use to deliver the [`UserDecision`].
#[derive(Debug)]
pub struct PendingPermission {
    /// The request as built by the agent loop.
    pub request: PermissionRequest,
    /// The oneshot sender. Consumed by `respond`.
    pub responder: tokio::sync::oneshot::Sender<UserDecision>,
}

impl PermissionRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new pending request. Returns a clone of the
    /// request (the sender stays in the registry).
    pub fn register(
        &self,
        req: PermissionRequest,
        responder: tokio::sync::oneshot::Sender<UserDecision>,
    ) -> PermissionRequest {
        let id = req.request_id.clone();
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                id,
                PendingPermission {
                    request: req.clone(),
                    responder,
                },
            );
        req
    }

    /// Deliver a user decision to the request awaiting it. Returns
    /// `Ok(())` if the request was found; `Err(NotFound)` otherwise.
    /// The caller is expected to log and emit
    /// `permission.resolved.v1` separately.
    pub fn respond(&self, request_id: &str, decision: UserDecision) -> Result<(), crate::AppError> {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let entry = guard
            .remove(request_id)
            .ok_or_else(|| crate::AppError::NotFound {
                kind: "permission_request".into(),
                id: request_id.into(),
            })?;
        // If the receiver was already dropped (run was aborted),
        // the send returns Err. We ignore that — the run is gone.
        let _ = entry.responder.send(decision);
        Ok(())
    }

    /// Take a request out of the registry **without** resolving
    /// it. Used when the run is aborted and we want to drain
    /// the pending requests.
    pub fn take(&self, request_id: &str) -> Option<PermissionRequest> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(request_id)
            .map(|p| p.request)
    }

    /// Snapshot of pending requests (used by the UI to render a
    /// queue, if any).
    #[must_use]
    pub fn list(&self) -> Vec<PermissionRequest> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .map(|p| p.request.clone())
            .collect()
    }

    /// Number of pending requests.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// Whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base_snap() -> PermissionSnapshot {
        let dir = tempfile::tempdir().unwrap();
        PermissionSnapshot {
            workspace_root: dir.path().canonicalize().unwrap(),
            extra_paths: vec![],
            approval_mode: ApprovalMode::Ask,
            workspace_allow: vec!["read_file".into(), "list_dir".into(), "search".into()],
            workspace_deny: vec![],
            workspace_ask: vec!["write_file".into(), "shell".into()],
            deny_paths: vec!["**/.git/**".into()],
            allow_paths: vec![],
            extra_paths_deny: vec![],
            always_allow: vec![],
            always_deny: vec![],
            agent_allow: vec![],
            agent_deny: vec![],
            agent_ask: vec![],
        }
    }

    fn base_gate(snap: &PermissionSnapshot) -> PermissionGate {
        PermissionGate::with_globs(snap.compile_globs())
    }

    #[test]
    fn ac1_allow_tool_returns_allow() {
        let snap = base_snap();
        let g = base_gate(&snap);
        let d = g.check(&snap, "read_file", &json!({"path": "x.rs"}));
        assert!(matches!(d, Decision::Allow { persist: true }));
    }

    #[test]
    fn ac2_deny_tool_returns_deny() {
        let mut snap = base_snap();
        snap.workspace_deny.push("shell".into());
        let g = base_gate(&snap);
        let d = g.check(&snap, "shell", &json!({"command": "ls"}));
        assert!(matches!(d, Decision::Deny { .. }));
    }

    #[test]
    fn ac3_ask_tool_returns_ask() {
        let snap = base_snap();
        let g = base_gate(&snap);
        let d = g.check(
            &snap,
            "write_file",
            &json!({"path": "x.rs", "content": "hi"}),
        );
        assert!(matches!(d, Decision::Ask { .. }));
    }

    #[test]
    fn ac4_denied_path_overrides_allow() {
        let mut snap = base_snap();
        snap.deny_paths.push("**/secret/**".into());
        let g = base_gate(&snap);
        let d = g.check(&snap, "read_file", &json!({"path": "secret/x.rs"}));
        assert!(matches!(d, Decision::Deny { reason } if reason == "denied_path"));
    }

    #[test]
    fn ac5_path_traversal_always_denied() {
        let snap = base_snap();
        let g = base_gate(&snap);
        let d = g.check(&snap, "read_file", &json!({"path": "../foo"}));
        assert!(matches!(d, Decision::Deny { reason } if reason == "path_traversal"));
    }

    #[test]
    fn ac6_global_always_allow_overrides_workspace() {
        let mut snap = base_snap();
        snap.always_allow.push("shell".into());
        let g = base_gate(&snap);
        let d = g.check(&snap, "shell", &json!({"command": "ls"}));
        assert!(matches!(d, Decision::Allow { persist: true }));
    }

    #[test]
    fn ac7_global_always_deny_overrides_workspace() {
        let mut snap = base_snap();
        snap.always_deny.push("read_file".into());
        let g = base_gate(&snap);
        let d = g.check(&snap, "read_file", &json!({"path": "x.rs"}));
        assert!(matches!(d, Decision::Deny { reason } if reason == "always_deny"));
    }

    #[test]
    fn ac8_unknown_tool_safe_default_ask() {
        let snap = base_snap();
        let g = base_gate(&snap);
        let d = g.check(&snap, "unknown_tool_xyz", &json!({}));
        assert!(matches!(d, Decision::Ask { reason } if reason == "unknown_tool"));
    }

    #[test]
    fn ac9_never_mode_blocks_dangerous() {
        let mut snap = base_snap();
        snap.approval_mode = ApprovalMode::Never;
        // shell is not in workspace_allow; gate denies.
        let g = base_gate(&snap);
        let d = g.check(&snap, "shell", &json!({"command": "ls"}));
        assert!(matches!(d, Decision::Deny { reason } if reason == "approval_mode_never"));
    }

    #[test]
    fn ac10_auto_mode_skips_prompt() {
        let mut snap = base_snap();
        snap.approval_mode = ApprovalMode::Auto;
        let g = base_gate(&snap);
        let d = g.check(
            &snap,
            "write_file",
            &json!({"path": "x.rs", "content": "hi"}),
        );
        assert!(matches!(d, Decision::Allow { persist: false }));
    }

    #[test]
    fn ac13_concurrent_check_idempotent() {
        use std::sync::Arc;
        let snap = Arc::new(base_snap());
        let gate = Arc::new(base_gate(&snap));
        let mut handles = vec![];
        for _ in 0..8 {
            let s = Arc::clone(&snap);
            let g = Arc::clone(&gate);
            handles.push(std::thread::spawn(move || {
                g.check(&s, "read_file", &json!({"path": "x.rs"}))
            }));
        }
        for h in handles {
            let d = h.join().unwrap();
            assert!(matches!(d, Decision::Allow { .. }));
        }
    }

    #[test]
    fn ac15_path_in_extra_path_is_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("proj");
        let extra = dir.path().join("assets");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&extra).unwrap();
        let mut snap = base_snap();
        snap.workspace_root = root.canonicalize().unwrap();
        snap.extra_paths = vec![extra.canonicalize().unwrap()];
        let g = base_gate(&snap);
        // Pass the **canonical** absolute path to defeat the
        // macOS `/tmp` → `/private/tmp` symlink.
        let d = g.check(
            &snap,
            "read_file",
            &json!({"path": extra.canonicalize().unwrap().join("foo.png").to_str().unwrap()}),
        );
        assert!(matches!(d, Decision::Allow { .. }));
    }

    #[test]
    fn ac16_path_outside_root_and_extras_denied() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("proj");
        let extra = dir.path().join("assets");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&extra).unwrap();
        let mut snap = base_snap();
        snap.workspace_root = root.canonicalize().unwrap();
        snap.extra_paths = vec![extra.canonicalize().unwrap()];
        let g = base_gate(&snap);
        let d = g.check(&snap, "read_file", &json!({"path": "/etc/passwd"}));
        assert!(matches!(d, Decision::Deny { reason } if reason == "path_outside_workspace"));
    }

    #[test]
    fn ac17_extra_paths_deny_overrides_allow() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("proj");
        let extra = dir.path().join("assets");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(extra.join("secret")).unwrap();
        let mut snap = base_snap();
        snap.workspace_root = root.canonicalize().unwrap();
        snap.extra_paths = vec![extra.canonicalize().unwrap()];
        snap.extra_paths_deny.push("**/secret/**".into());
        let g = base_gate(&snap);
        let d = g.check(
            &snap,
            "read_file",
            &json!({"path": extra.canonicalize().unwrap().join("secret/x").to_str().unwrap()}),
        );
        assert!(matches!(d, Decision::Deny { reason } if reason == "denied_path_in_extra"));
    }

    #[test]
    fn ac18_agent_deny_overrides_workspace_allow() {
        let mut snap = base_snap();
        snap.agent_deny.push("write_file".into());
        let g = base_gate(&snap);
        let d = g.check(
            &snap,
            "write_file",
            &json!({"path": "x.rs", "content": "hi"}),
        );
        assert!(matches!(d, Decision::Deny { reason } if reason == "denied"));
    }

    #[test]
    fn extract_path_args_collects_known_keys() {
        let args = json!({
            "path": "x.rs",
            "cwd": "/tmp",
            "glob": "*.rs",
            "depth": 3,
        });
        let mut v: Vec<&str> = extract_path_args(&args);
        v.sort();
        assert_eq!(v, vec!["/tmp", "x.rs"]);
    }

    #[test]
    fn permission_registry_take_removes_entry() {
        let reg = PermissionRegistry::new();
        let req = PermissionRequest::new("read_file", json!({"path": "x.rs"}), "user_approval");
        let (tx, _rx) = tokio::sync::oneshot::channel();
        reg.register(req.clone(), tx);
        assert_eq!(reg.len(), 1);
        let taken = reg.take(&req.request_id).unwrap();
        assert_eq!(taken.request_id, req.request_id);
        assert!(reg.is_empty());
    }

    #[test]
    fn permission_registry_respond_delivers_decision() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let reg = PermissionRegistry::new();
            let req = PermissionRequest::new(
                "write_file",
                json!({"path": "x.rs", "content": "hi"}),
                "user_approval",
            );
            let (tx, rx) = tokio::sync::oneshot::channel();
            reg.register(req.clone(), tx);
            // Spawn a task that resolves the request.
            let reg2 = reg.clone();
            let id = req.request_id.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                reg2.respond(&id, UserDecision::Allow { persist: true })
                    .unwrap();
            });
            let d = rx.await.unwrap();
            assert!(matches!(d, UserDecision::Allow { persist: true }));
            assert!(reg.is_empty());
        });
    }

    #[test]
    fn user_decision_into_decision() {
        assert!(matches!(
            UserDecision::Allow { persist: true }.into_decision(),
            Decision::Allow { persist: true }
        ));
        assert!(matches!(
            UserDecision::Deny { persist: false }.into_decision(),
            Decision::Deny { .. }
        ));
    }
}
