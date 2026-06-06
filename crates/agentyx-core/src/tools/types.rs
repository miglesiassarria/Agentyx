//! Tools — capabilities the agent can invoke.
//!
//! Per `specs/domains/tools.md`, the [`Tool`] trait is the contract
//! every tool implements. The agent loop calls `run(ctx, args)`,
//! gets back a [`ToolOutput`], and emits the appropriate
//! `chat.tool_call.v1` / `chat.tool_result.v1` events.
//!
//! The tool **does not** know about events, the journal, or
//! permission state — those are concerns of the agent loop. The
//! only side-effect channels are: the filesystem (read/write),
//! child processes (shell, python), and [`ToolOutput::metadata`]
//! (small structured data the caller may want to surface to the UI).
//!
//! Path sandboxing is **enforced twice**: once by the permission
//! gate (before the tool is even invoked) and once inside the tool
//! itself (defense in depth). See [`crate::permissions`].

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ids::{RunId, SessionId, WorkspaceId};
use crate::tools::builtin;
use crate::AppError;

/// Static identifier of a tool (e.g. `"read_file"`, `"shell"`).
///
/// Cheap to clone / copy; the registry uses `&'static str` literals.
pub type ToolId = &'static str;

/// Context passed to every tool invocation. Carries everything
/// that's not in `args` (workspace paths, run metadata, abort
/// signal, etc.).
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Workspace the tool is being invoked in.
    pub workspace_id: WorkspaceId,
    /// Canonical absolute path of the workspace root.
    pub workspace_root: std::path::PathBuf,
    /// Canonical absolute paths of the workspace's extra paths.
    /// See ADR-0007.
    pub extra_paths: Arc<Vec<std::path::PathBuf>>,
    /// The run that requested the tool call.
    pub run_id: RunId,
    /// The session that owns the run.
    pub session_id: SessionId,
    /// Cooperative abort signal. Long-running tools (`shell`,
    /// `python_run`) should poll this between sub-steps.
    pub abort_flag: Arc<AtomicBool>,
    /// Glob patterns to ignore (from the workspace's `ignore` config).
    /// Tools that walk the filesystem should skip these.
    pub ignore_patterns: Arc<Vec<String>>,
}

/// The normalized output of a tool. Serialized to JSON for the
/// `chat.tool_result.v1` event and to a `ToolResult` message in
/// the session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// The textual content of the tool's result. For `read_file`
    /// this is the file body; for `shell` it's `{stdout, stderr}`;
    /// for `list_dir` it's the directory listing. Always a
    /// **string** (callers that want structured data should use
    /// [`ToolOutput::metadata`]).
    pub content: String,
    /// A short summary (≤ 200 chars) suitable for inline UI
    /// rendering and the journal. Truncated to 200 chars if
    /// `content` is larger.
    pub summary: String,
    /// Whether the tool considers itself to have failed. The
    /// agent loop surfaces this in the `tool_result.v1` event
    /// (`isError: true`) and continues; a `false` value does not
    /// guarantee success — the caller still inspects `content`.
    pub is_error: bool,
    /// Optional structured metadata. The `summary` is always
    /// present and human-readable; `metadata` is for callers that
    /// want to render a richer UI (e.g. file tree, diff).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub metadata: Option<Value>,
    /// Duration of the tool execution in milliseconds. Set by the
    /// agent loop, not by the tool itself.
    #[serde(default)]
    pub duration_ms: u64,
}

impl ToolOutput {
    /// Construct a successful output from a `content` string.
    /// `summary` is derived automatically (first 200 chars).
    #[must_use]
    pub fn success(content: impl Into<String>) -> Self {
        let content = content.into();
        let summary = crate::agent::summarize_pub(&content, 200);
        Self {
            content,
            summary,
            is_error: false,
            metadata: None,
            duration_ms: 0,
        }
    }

    /// Construct an error output. The `summary` carries the short
    /// message; full `content` is preserved for `journal` /
    /// `tool_result.v1` consumers.
    #[must_use]
    pub fn failure(content: impl Into<String>) -> Self {
        let content = content.into();
        let summary = crate::agent::summarize_pub(&content, 200);
        Self {
            content,
            summary,
            is_error: true,
            metadata: None,
            duration_ms: 0,
        }
    }
}

/// The trait every tool implements.
///
/// Tools are stateless between invocations. The agent loop holds
/// the [`ToolRegistry`] (a slice of trait objects) and dispatches
/// by name.
#[async_trait]
pub trait Tool: Send + Sync {
    /// The tool's identifier (e.g. `"read_file"`). Must be unique
    /// within the registry.
    fn name(&self) -> ToolId;

    /// Whether this tool modifies state (writes files, runs
    /// subprocesses, etc.). The permission gate uses this flag to
    /// decide which tools fall into `Ask` by default.
    fn is_dangerous(&self) -> bool;

    /// JSON schema describing the tool's args. This schema is sent
    /// to the LLM so it knows what tool calls it can emit. We
    /// return `serde_json::Value` rather than a typed struct to
    /// keep the trait object-safe.
    fn schema(&self) -> Value;

    /// Execute the tool. `args` is the JSON value the LLM emitted
    /// (already validated by the agent loop's arg validator).
    ///
    /// Errors from this method are treated as
    /// [`ToolOutput::failure`]; the tool result still gets
    /// persisted and emitted to the UI so the model can react.
    /// Use [`ToolOutput::failure`] for explicit "this didn't work"
    /// cases (file not found, command exited 1, etc.). Use
    /// `Err(AppError::...)` only for catastrophic failures (panic
    /// recovery, FS unavailable). In practice tools should
    /// **always** return `Ok` with `is_error: true` for any
    /// user-facing failure.
    async fn run(&self, ctx: ToolContext, args: Value) -> Result<ToolOutput, AppError>;
}

/// Build the registry of built-in tools. Returns a `Vec<Arc<dyn
/// Tool>>` so the caller can either keep it as a `Vec` or push
/// custom tools on top.
///
/// This is the **single source of truth** for the set of tools
/// the agent exposes to the LLM.
#[must_use]
pub fn built_in_registry() -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(builtin::ReadFileTool),
        Arc::new(builtin::ListDirTool),
        Arc::new(builtin::SearchTool),
    ]
}

/// Look up a tool by name in a registry. Returns `None` if
/// missing (the agent loop surfaces this as
/// `tool_result.v1 { isError: true, output: "unknown tool: ..." }`).
#[must_use]
pub fn find<'a>(registry: &'a [Arc<dyn Tool>], name: &str) -> Option<&'a Arc<dyn Tool>> {
    registry.iter().find(|t| t.name() == name)
}

/// Collect the JSON schemas of every tool in the registry.
/// Passed to the provider in `ChatRequest.tools`.
#[must_use]
pub fn schemas(registry: &[Arc<dyn Tool>]) -> Vec<Value> {
    registry.iter().map(|t| t.schema()).collect()
}

/// Names of all tools in the registry. Useful for the
/// permission gate's "is this a known tool?" check.
#[must_use]
pub fn names(registry: &[Arc<dyn Tool>]) -> Vec<&'static str> {
    registry.iter().map(|t| t.name()).collect()
}
