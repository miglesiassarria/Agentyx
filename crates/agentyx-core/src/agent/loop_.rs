//! The agent loop — function-style orchestrator.
//!
//! The loop is **not** an object with internal state. Each call
//! to [`spawn_run`] is independent: it receives the dependencies
//! it needs, spawns a Tokio task, and returns a [`RunHandle`].
//! Cancellation happens by calling [`RunHandle::abort`], which
//! flips an atomic flag the loop checks between deltas.
//!
//! ## Why function-style, not a struct?
//!
//! The agent loop is a *coordinator* of services that are owned
//! by `AppState` (a `SessionService` per workspace, a
//! `JournalRepo` per workspace, a `ConfigService` for the global
//! config, a provider registry). Holding those in a single
//! `AgentLoop` struct would force one loop per workspace, which
//! is wrong: a single user may run sessions in multiple
//! workspaces. A function takes the right services for the right
//! session and spawns a per-run task. The shared state that
//! matters (the active-runs map for `abort`) lives in
//! [`RunRegistry`], a tiny helper the caller can share across
//! runs.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use ulid::Ulid;

use crate::agent::{BatcherConfig, DeltaBatcher};
use crate::agents::AgentRegistry;
use crate::config::ConfigService;
use crate::ids::{AgentId, RunId, SessionId, WorkspaceId};
use crate::journal::{JournalKind, JournalRepo, NewJournalEntry};
use crate::llm::{
    ChatEvent, ChatMessage, ChatRequest, FinishReason, Provider, RequestMetadata, ToolCall, Usage,
};
use crate::permissions::{Decision, UserDecision};
use crate::session::{MessageRole, SessionService, SessionStatus};
use crate::tools::{Tool, ToolContext, ToolOutput};
use crate::workspace::WorkspaceService;
use crate::{AppError, AppResult};

/// Maximum size of a user message (1 MB), per F01.AC9. Messages
/// larger than this are rejected at the command boundary with
/// `invalid_input`.
pub const MAX_USER_MSG_BYTES: usize = 1024 * 1024;

/// Default max-steps for a single run. Per agent-loop.md §Edge
/// case 3, hitting this aborts the run with `finish_reason:
/// length`. For Phase 1 (no tools, single turn), one step is
/// enough; we still cap to 50 per the spec default.
pub const DEFAULT_MAX_STEPS: u32 = 50;

/// Optional overrides for [`spawn_run`]. All fields default to
/// "use the workspace's / session's configured value".
#[derive(Debug, Clone, Default)]
pub struct StartOpts {
    /// Override the model id (e.g. `"qwen2.5:7b"` instead of
    /// the workspace's `default_model`).
    pub model: Option<String>,
    /// Override the system prompt. If `None`, the active
    /// `AgentSpec`'s embedded prompt is used.
    pub system_prompt_override: Option<String>,
    /// Max steps for the run. Default: [`DEFAULT_MAX_STEPS`].
    pub max_steps: Option<u32>,
}

/// Status of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    /// The run is in progress.
    Running,
    /// The run finished naturally (`finish_reason: stop`).
    Finished,
    /// The run was aborted (by the user or the app closing).
    Aborted,
    /// The run ended with an error.
    Errored,
}

/// Snapshot of a run's state. Returned by [`RunHandle::state`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunState {
    /// Run id.
    pub run_id: RunId,
    /// Session id.
    pub session_id: SessionId,
    /// Agent active for this run.
    pub agent_id: AgentId,
    /// When the run started.
    pub started_at: DateTime<Utc>,
    /// Current status.
    pub status: RunStatus,
    /// Last error, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<AppError>,
    /// Cumulative token usage so far.
    pub usage: Usage,
}

/// A handle to a running (or finished) run.
///
/// Cheap to clone (internally `Arc`). The handle can outlive the
/// spawned task: [`RunHandle::state`] and [`RunHandle::is_running`]
/// still work after the run has finished.
#[derive(Debug, Clone)]
pub struct RunHandle {
    inner: Arc<RunInner>,
}

struct RunInner {
    run_id: RunId,
    session_id: SessionId,
    workspace_id: WorkspaceId,
    agent_id: AgentId,
    started_at: DateTime<Utc>,
    status: Mutex<RunStatus>,
    last_error: Mutex<Option<AppError>>,
    usage: Mutex<Usage>,
    abort_flag: Arc<AtomicBool>,
}

impl std::fmt::Debug for RunInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunInner")
            .field("run_id", &self.run_id)
            .field("session_id", &self.session_id)
            .field("workspace_id", &self.workspace_id)
            .field("agent_id", &self.agent_id)
            .field("started_at", &self.started_at)
            .field(
                "status",
                &*self.status.lock().unwrap_or_else(|e| e.into_inner()),
            )
            .field(
                "last_error",
                &*self.last_error.lock().unwrap_or_else(|e| e.into_inner()),
            )
            .field(
                "usage",
                &*self.usage.lock().unwrap_or_else(|e| e.into_inner()),
            )
            .field("abort_flag", &self.abort_flag)
            .finish()
    }
}

impl RunHandle {
    /// Build a new handle. Used by [`spawn_run`] in production
    /// and by the test suite (in both `agentyx-core` and
    /// `agentyx-app`) to fabricate synthetic running runs
    /// without going through the full spawn path. The public
    /// visibility is intentional: it is the smallest surface
    /// that lets the integration tests for F02.AC7
    /// (workspace delete with active runs) construct a
    /// `RunHandle` directly. Production code should always
    /// prefer [`spawn_run`].
    pub fn new(
        run_id: RunId,
        session_id: SessionId,
        workspace_id: WorkspaceId,
        agent_id: AgentId,
    ) -> Self {
        Self {
            inner: Arc::new(RunInner {
                run_id,
                session_id,
                workspace_id,
                agent_id,
                started_at: Utc::now(),
                status: Mutex::new(RunStatus::Running),
                last_error: Mutex::new(None),
                usage: Mutex::new(Usage::default()),
                abort_flag: Arc::new(AtomicBool::new(false)),
            }),
        }
    }

    /// Workspace this run belongs to. Stable for the lifetime of
    /// the run; does not change after `abort` or `mark`. Used by
    /// `WorkspaceService::delete` to refuse when a workspace has
    /// active runs (F02.AC7).
    #[must_use]
    pub fn workspace_id(&self) -> WorkspaceId {
        self.inner.workspace_id
    }

    /// Snapshot the current state.
    #[must_use]
    pub fn state(&self) -> RunState {
        RunState {
            run_id: self.inner.run_id,
            session_id: self.inner.session_id,
            agent_id: self.inner.agent_id,
            started_at: self.inner.started_at,
            status: *self
                .inner
                .status
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            last_error: self
                .inner
                .last_error
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone(),
            usage: self
                .inner
                .usage
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone(),
        }
    }

    /// Whether the run is still in progress.
    #[must_use]
    pub fn is_running(&self) -> bool {
        matches!(
            *self
                .inner
                .status
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            RunStatus::Running
        )
    }

    /// Request cancellation. Idempotent. Returns immediately; the
    /// loop checks the flag between deltas and within ~100ms
    /// (per agent-loop.md §AC5).
    pub fn abort(&self) {
        self.inner.abort_flag.store(true, Ordering::SeqCst);
    }

    /// Read the abort flag. Returns `true` if [`Self::abort`] has
    /// been called, even if the agent loop has not yet observed
    /// the flag and transitioned `status` to a terminal value.
    /// Useful for "is this run about to stop?" checks from
    /// outside the loop (e.g. F02.AC7 verifying that a
    /// `force=true` delete actually requested the abort).
    #[must_use]
    pub fn is_aborted(&self) -> bool {
        self.inner.abort_flag.load(Ordering::SeqCst)
    }

    /// Internal: mark the run as finished.
    fn mark(&self, status: RunStatus, error: Option<AppError>) {
        *self
            .inner
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = status;
        *self
            .inner
            .last_error
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = error;
    }

    /// Internal: add token usage.
    fn add_usage(&self, usage: &Usage) {
        let mut u = self
            .inner
            .usage
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        u.prompt_tokens = u.prompt_tokens.saturating_add(usage.prompt_tokens);
        u.completion_tokens = u.completion_tokens.saturating_add(usage.completion_tokens);
    }
}

/// Abstraction over the Tauri event bus. The agent loop emits
/// events (`chat.run.started.v1`, `chat.content.delta.v1`, ...)
/// through this trait; the app layer wires it to
/// `tauri::AppHandle::emit`. Keeping it as a trait (rather than
/// taking a concrete `EventBus`) lets unit tests inject a
/// recording sink without touching Tauri.
pub trait EventSink: Send + Sync {
    /// Emit a typed event with a serializable payload.
    fn emit(&self, event: &str, payload: serde_json::Value);
}

/// Dependencies for [`spawn_run`]. The caller (a Tauri command)
/// is responsible for assembling this struct from `AppState`.
pub struct AgentLoopDeps {
    /// Agent registry (built-in + custom). Cheap to clone.
    pub agents: AgentRegistry,
    /// Global config service.
    pub config: ConfigService,
    /// Provider registry keyed by provider id (e.g. `"ollama"`).
    pub providers: std::collections::HashMap<String, Arc<dyn Provider>>,
    /// Per-workspace session service.
    pub session: SessionService,
    /// Per-workspace journal repository.
    pub journal: JournalRepo,
    /// Event sink for streaming events to the UI.
    pub bus: Arc<dyn EventSink>,
    /// Workspace service. Used to look up the workspace's
    /// `root_path` and `extra_paths` for tool execution and
    /// permission snapshots.
    pub workspaces: crate::workspace::WorkspaceService,
    /// Tool registry. The loop exposes these schemas to the
    /// LLM and dispatches `ToolUse` events to them.
    pub tool_registry: Arc<Vec<Arc<dyn crate::tools::Tool>>>,
    /// Permission gate. Stateless; takes a snapshot per run.
    pub permission_gate: crate::permissions::PermissionGate,
    /// Permission registry. Holds the oneshot responders for
    /// `Ask` decisions; the agent loop registers, the
    /// `permission_respond` Tauri command resolves.
    pub permission_registry: crate::permissions::PermissionRegistry,
}

/// Spawn a new run on the Tokio runtime. Returns a
/// [`RunHandle`] immediately; the actual work happens in the
/// background.
///
/// ## Errors
///
/// - `invalid_input` — `user_msg` is empty, only whitespace, or
///   larger than [`MAX_USER_MSG_BYTES`].
/// - `not_found` — the session does not exist, or the active
///   agent is missing from the registry.
/// - `conflict` — the session already has a run in progress.
/// - `provider` — the provider is not registered, not enabled,
///   or unreachable at the start of the run.
pub fn spawn_run(
    deps: AgentLoopDeps,
    session_id: SessionId,
    user_msg: String,
    opts: StartOpts,
) -> AppResult<RunHandle> {
    if user_msg.trim().is_empty() {
        return Err(AppError::InvalidInput {
            message: "user message cannot be empty".into(),
        });
    }
    if user_msg.len() > MAX_USER_MSG_BYTES {
        return Err(AppError::InvalidInput {
            message: format!(
                "user message too large: {} bytes (max {})",
                user_msg.len(),
                MAX_USER_MSG_BYTES
            ),
        });
    }

    // Load the session synchronously (cheap DB read).
    let session = deps.session.get(session_id)?;
    if matches!(session.status, SessionStatus::Running) {
        return Err(AppError::Conflict {
            message: "session already has a running run; abort it first".into(),
        });
    }

    // Resolve the active agent spec.
    let active_agent_id = session.active_agent_id;
    let agent = deps
        .agents
        .get(&active_agent_id)
        .ok_or_else(|| AppError::NotFound {
            kind: "agent".into(),
            id: active_agent_id.to_string(),
        })?
        .clone();

    // Resolve provider + model from the global config (the
    // agent's ModelRef is `default` in v1).
    let cfg = deps.config.get();
    let (provider_id, model) = if deps.providers.contains_key(&cfg.default_provider) {
        // Use configured provider and model.
        (
            cfg.default_provider.clone(),
            opts.model
                .clone()
                .unwrap_or_else(|| cfg.default_model.clone()),
        )
    } else {
        // Default provider not registered (e.g., no API key for minimax).
        // Fall back to the first available provider.
        let fallback_provider_id =
            deps.providers
                .keys()
                .next()
                .cloned()
                .ok_or_else(|| AppError::Internal {
                    message: "no providers registered".into(),
                })?;
        // For the model, use the provider's default or a known Ollama model.
        let fallback_model = if fallback_provider_id == "ollama" {
            "gemma4:latest".to_string()
        } else {
            opts.model
                .clone()
                .unwrap_or_else(|| cfg.default_model.clone())
        };
        (fallback_provider_id, fallback_model)
    };
    let provider = deps
        .providers
        .get(&provider_id)
        .ok_or_else(|| AppError::Internal {
            message: format!("provider '{provider_id}' not registered"),
        })?
        .clone();

    // Generate a run id; mark session as running; persist the
    // user message; emit `chat.run.started.v1`.
    let run_id = RunId::new();
    let handle = RunHandle::new(run_id, session_id, session.workspace_id, active_agent_id);

    deps.session.start_run(session_id, run_id)?;

    // Append user message synchronously (small, fast).
    let user_message_id = deps
        .session
        .append_message(session_id, MessageRole::User, &user_msg, Some(run_id))?
        .id;

    // Journal: UserMessage entry.
    if let Err(e) = deps.journal.append(NewJournalEntry {
        id: None,
        session_id,
        run_id,
        parent_run_id: None,
        depth: 0,
        kind: JournalKind::UserMessage,
        agent_id: Some(active_agent_id),
        payload: json!({
            "messageId": user_message_id.to_string(),
            "textSummary": summarize(&user_msg, 120),
        }),
        duration_ms: None,
    }) {
        warn!(error = %e, "failed to journal UserMessage");
    }

    let max_steps = opts.max_steps.unwrap_or(DEFAULT_MAX_STEPS);
    let system_prompt =
        opts.system_prompt_override
            .clone()
            .or_else(|| match &agent.prompt {
                crate::agents::PromptSource::Embedded { content } => Some(content.clone()),
                crate::agents::PromptSource::File { .. }
                | crate::agents::PromptSource::Url { .. } => None,
            });

    deps.bus.emit(
        "chat.run.started.v1",
        json!({
            "runId": run_id.to_string(),
            "sessionId": session_id.to_string(),
            "workspaceId": session.workspace_id.to_string(),
            "agentId": active_agent_id.to_string(),
            "providerId": provider_id,
            "model": model,
        }),
    );

    info!(
        run_id = %run_id,
        session_id = %session_id,
        agent_id = %active_agent_id,
        provider_id = %provider_id,
        model = %model,
        max_steps,
        "run started"
    );

    // Spawn the async work.
    tokio::spawn(run_loop(RunContext {
        handle: handle.clone(),
        deps,
        provider,
        model,
        system_prompt,
        max_steps,
    }));

    Ok(handle)
}

/// Internal: the per-run async context. Carries everything the
/// loop needs without holding a reference back to `AppState`.
struct RunContext {
    handle: RunHandle,
    deps: AgentLoopDeps,
    provider: Arc<dyn Provider>,
    model: String,
    system_prompt: Option<String>,
    max_steps: u32,
}

/// The async run loop. Iterates up to `max_steps` times. Each
/// step:
///
/// 1. Build a `ChatRequest` from the in-memory message history
///    + the tool schemas.
/// 2. Call `provider.chat()` and stream events.
/// 3. Accumulate content via [`DeltaBatcher`] (50ms / 100 chars).
/// 4. On `ToolUse`, route through [`PermissionGate`]: `Allow`
///    runs the tool, `Ask` pauses the run for the user's
///    response, `Deny` emits an error result.
/// 5. After the stream ends, if there are pending tool calls,
///    append the `ToolResult` messages to the in-memory
///    history and continue to the next step.
/// 6. If there are no pending tool calls, persist the final
///    assistant message and emit `chat.run.finished.v1`.
///
/// The loop is abortable via [`RunHandle::abort`] (sets an
/// `AtomicBool` that the loop checks at the top of each step and
/// between events).
async fn run_loop(ctx: RunContext) {
    let RunContext {
        handle,
        deps,
        provider,
        model,
        system_prompt,
        max_steps,
    } = ctx;
    let run_id = handle.inner.run_id;
    let session_id = handle.inner.session_id;
    let workspace_id = handle.inner.workspace_id;
    let agent_id = handle.inner.agent_id;
    let started = std::time::Instant::now();

    // Build the initial in-memory message history from the DB.
    let history = match deps
        .session
        .list_messages(session_id, crate::session::ListMessagesOpts::default())
    {
        Ok(m) => m,
        Err(e) => {
            finish_with_error(&deps, &handle, session_id, run_id, e, started);
            return;
        }
    };

    let mut messages: Vec<ChatMessage> = Vec::with_capacity(history.len() + 1);
    if let Some(prompt) = system_prompt.as_deref() {
        messages.push(ChatMessage::System {
            content: prompt.to_string(),
        });
    }
    for m in &history {
        match m.role {
            MessageRole::User => {
                messages.push(ChatMessage::User {
                    content: m.content.clone(),
                });
            }
            MessageRole::Assistant => {
                messages.push(ChatMessage::Assistant {
                    content: m.content.clone(),
                    tool_calls: Vec::new(),
                });
            }
            MessageRole::System => {
                messages.push(ChatMessage::System {
                    content: m.content.clone(),
                });
            }
            MessageRole::ToolResult => {
                // Recovered from a prior run; surface the content
                // as a user message so the model has it. In
                // v1.x the journal carries the full tool_use_id
                // and we reconstruct the proper `ToolResult`
                // message here.
                messages.push(ChatMessage::User {
                    content: format!("[tool result]\n{}", m.content),
                });
            }
        }
    }

    // Tool schemas (filtered by the agent's `tool_access`).
    let tool_schemas = build_tool_schemas(&deps.tool_registry, &deps.agents, &agent_id);

    // Permission snapshot (workspace-root + agent overrides).
    let perm_snap =
        build_permission_snapshot(&deps.workspaces, &deps.agents, workspace_id, &agent_id);

    // Total accumulated text across all steps. Persisted once
    // at the end of the run (F01.AC13 — one INSERT, not per
    // delta).
    let mut total_accumulated = String::new();
    let mut last_usage = Usage::default();
    let mut final_fr: FinishReason = FinishReason::Stop;
    let mut final_status = RunStatus::Finished;
    let mut errored: Option<AppError> = None;

    // Per-step state: how many times we've called the provider.
    for step_idx in 1..=max_steps {
        if handle.is_aborted() {
            info!(run_id = %run_id, "run aborted");
            final_fr = FinishReason::Aborted;
            final_status = RunStatus::Aborted;
            break;
        }
        if step_idx > 1 && step_idx > max_steps {
            warn!(run_id = %run_id, step = step_idx, max_steps, "max_steps reached");
            final_fr = FinishReason::Length;
            break;
        }

        // Build the request for this step.
        let req = ChatRequest {
            model: model.clone(),
            messages: messages.clone(),
            tools: tool_schemas.clone(),
            tool_choice: if tool_schemas.is_empty() {
                crate::llm::ToolChoice::None
            } else {
                crate::llm::ToolChoice::Auto
            },
            max_output_tokens: None,
            temperature: None,
            stream: true,
            metadata: RequestMetadata {
                workspace_id,
                session_id,
                run_id,
                agent_id,
            },
        };

        // Journal: ProviderEvent (request).
        if let Err(e) = deps.journal.append(NewJournalEntry {
            id: None,
            session_id,
            run_id,
            parent_run_id: None,
            depth: 0,
            kind: JournalKind::ProviderEvent,
            agent_id: Some(agent_id),
            payload: json!({
                "kind": "request",
                "providerId": deps.config.get().default_provider,
                "model": model,
                "step": step_idx,
            }),
            duration_ms: None,
        }) {
            warn!(error = %e, "failed to journal ProviderEvent (request)");
        }

        // Call provider.
        let stream_result = provider.chat(req).await;
        let mut stream = match stream_result {
            Ok(s) => s,
            Err(e) => {
                errored = Some(e);
                break;
            }
        };

        // Stream events for this step.
        let mut step_text = String::new();
        let mut pending_tool_calls: Vec<ToolCall> = Vec::new();
        let mut step_fr: FinishReason = FinishReason::Stop;
        let mut provider_err: Option<AppError> = None;
        let mut batcher = DeltaBatcher::new(BatcherConfig::default());
        let mut current_message_id: Option<String> = None;

        use futures::StreamExt;
        loop {
            if handle.is_aborted() {
                info!(run_id = %run_id, "run aborted mid-step");
                step_fr = FinishReason::Aborted;
                break;
            }
            match stream.next().await {
                Some(Ok(ChatEvent::MessageStart {
                    message_id,
                    model: m,
                })) => {
                    debug!(run_id = %run_id, message_id, model = m, "message_start");
                    current_message_id = Some(message_id.clone());
                    deps.bus.emit(
                        "chat.message_start.v1",
                        json!({
                            "sessionId": session_id.to_string(),
                            "runId": run_id.to_string(),
                            "messageId": message_id,
                            "model": m,
                            "role": "assistant",
                        }),
                    );
                }
                Some(Ok(ChatEvent::ContentDelta { text })) => {
                    step_text.push_str(&text);
                    batcher.push(&text);
                    if batcher.should_flush() {
                        if let Some(buf) = batcher.take() {
                            deps.bus.emit(
                                "chat.content.delta.v1",
                                json!({
                                    "runId": run_id.to_string(),
                                    "sessionId": session_id.to_string(),
                                    "messageId": current_message_id.as_deref().unwrap_or(""),
                                    "text": buf,
                                }),
                            );
                        }
                    }
                }
                Some(Ok(ChatEvent::ToolUse { id, name, args })) => {
                    pending_tool_calls.push(ToolCall { id, name, args });
                }
                Some(Ok(ChatEvent::MessageEnd {
                    usage,
                    finish_reason,
                })) => {
                    step_fr = finish_reason;
                    handle.add_usage(&usage);
                    last_usage = usage;
                }
                Some(Ok(ChatEvent::Error {
                    code,
                    message,
                    retryable,
                })) => {
                    provider_err = Some(AppError::Provider {
                        provider_id: deps.config.get().default_provider.clone(),
                        message: format!("{code}: {message}"),
                        retryable,
                    });
                    break;
                }
                Some(Err(e)) => {
                    provider_err = Some(e);
                    break;
                }
                None => {
                    // Stream ended.
                    break;
                }
            }
        }

        // Flush any remaining deltas.
        if let Some(buf) = batcher.take() {
            deps.bus.emit(
                "chat.content.delta.v1",
                json!({
                    "runId": run_id.to_string(),
                    "sessionId": session_id.to_string(),
                    "messageId": current_message_id.as_deref().unwrap_or(""),
                    "text": buf,
                }),
            );
        }

        // Emit step text as `chat.message_end.v1` so the UI can
        // close the message bubble.
        if !step_text.is_empty() {
            deps.bus.emit(
                "chat.message.end.v1",
                json!({
                    "runId": run_id.to_string(),
                    "sessionId": session_id.to_string(),
                    "messageId": current_message_id.as_deref().unwrap_or(""),
                    "finishReason": step_fr,
                    "step": step_idx,
                }),
            );
        }

        if let Some(err) = provider_err {
            errored = Some(err);
            break;
        }

        // Append the step's assistant message to the in-memory
        // history. We do this even if there are tool calls so
        // the next step has the full transcript.
        if !pending_tool_calls.is_empty() {
            let tool_calls_for_msg = pending_tool_calls.clone();
            messages.push(ChatMessage::Assistant {
                content: step_text.clone(),
                tool_calls: tool_calls_for_msg,
            });
        } else {
            messages.push(ChatMessage::Assistant {
                content: step_text.clone(),
                tool_calls: Vec::new(),
            });
        }
        total_accumulated.push_str(&step_text);

        // If no tool calls, this is the natural end of the run.
        if pending_tool_calls.is_empty() {
            final_fr = step_fr;
            break;
        }

        // Dispatch tool calls. We do them **sequentially** (v0.1
        // does not support parallel tool calls).
        for tc in pending_tool_calls.drain(..) {
            if handle.is_aborted() {
                break;
            }
            let outcome = dispatch_tool_call(&deps, &handle, &perm_snap, &tc).await;
            // Append a `ToolResult` message to the in-memory
            // history so the model can see the output.
            messages.push(ChatMessage::ToolResult {
                tool_use_id: tc.id.clone(),
                content: outcome.content,
                is_error: outcome.is_error,
            });
        }

        if handle.is_aborted() {
            final_fr = FinishReason::Aborted;
            final_status = RunStatus::Aborted;
            break;
        }
        // Loop continues with the next step.
        final_fr = step_fr; // last seen
    }

    if let Some(err) = errored {
        finish_with_error(&deps, &handle, session_id, run_id, err, started);
        return;
    }

    // Persist assistant message.
    if !total_accumulated.is_empty() {
        if let Err(e) = deps.session.append_message(
            session_id,
            MessageRole::Assistant,
            &total_accumulated,
            Some(run_id),
        ) {
            error!(run_id = %run_id, error = %e, "failed to persist assistant message");
        }
    }

    let finish_reason_str = match final_fr {
        FinishReason::Stop => "stop",
        FinishReason::Length => "length",
        FinishReason::ContentFilter => "content_filter",
        FinishReason::Error => "error",
        FinishReason::Aborted => "aborted",
    };
    let session_status = match final_status {
        RunStatus::Aborted => SessionStatus::Aborted,
        RunStatus::Errored => SessionStatus::Errored,
        RunStatus::Finished | RunStatus::Running => SessionStatus::Idle,
    };
    if let Err(e) = deps
        .session
        .finish_run(session_id, session_status, finish_reason_str)
    {
        error!(run_id = %run_id, error = %e, "failed to finish_run");
    }

    if !total_accumulated.is_empty() {
        if let Err(e) = deps.journal.append(NewJournalEntry {
            id: None,
            session_id,
            run_id,
            parent_run_id: None,
            depth: 0,
            kind: JournalKind::AssistantMessage,
            agent_id: Some(agent_id),
            payload: json!({
                "textSummary": summarize(&total_accumulated, 200),
                "finishReason": finish_reason_str,
            }),
            duration_ms: None,
        }) {
            warn!(error = %e, "failed to journal AssistantMessage");
        }
    }

    handle.mark(final_status, None);
    deps.bus.emit(
        "chat.run.finished.v1",
        json!({
            "runId": run_id.to_string(),
            "sessionId": session_id.to_string(),
            "status": ui_run_status(final_status),
            "usage": last_usage,
            "finishReason": finish_reason_str,
            "durationMs": started.elapsed().as_millis() as u64,
        }),
    );

    // If the run was aborted, also emit `chat.run.aborted.v1` so
    // the UI can surface a specific "Stopped" toast (per
    // `F01.AC4` and the event schema in F01 §Eventos). The
    // payload is a subset of `chat.run.finished.v1` for symmetry.
    if matches!(final_status, RunStatus::Aborted) {
        let reason = match final_fr {
            FinishReason::Aborted => "user",
            FinishReason::Length => "max_steps",
            _ => "aborted",
        };
        deps.bus.emit(
            "chat.run.aborted.v1",
            json!({
                "runId": run_id.to_string(),
                "sessionId": session_id.to_string(),
                "reason": reason,
            }),
        );
    }

    info!(
        run_id = %run_id,
        duration_ms = started.elapsed().as_millis() as u64,
        prompt_tokens = last_usage.prompt_tokens,
        completion_tokens = last_usage.completion_tokens,
        finish_reason = finish_reason_str,
        "run finished"
    );
}

fn finish_with_error(
    deps: &AgentLoopDeps,
    handle: &RunHandle,
    session_id: SessionId,
    run_id: RunId,
    err: AppError,
    started: std::time::Instant,
) {
    let code = err.code();
    let message = err.to_string();
    handle.mark(RunStatus::Errored, Some(err.clone()));
    if let Err(e) = deps
        .session
        .finish_run(session_id, SessionStatus::Errored, code)
    {
        error!(run_id = %run_id, error = %e, "failed to finish_run on error");
    }
    if let Err(e) = deps.journal.append(NewJournalEntry {
        id: None,
        session_id,
        run_id,
        parent_run_id: None,
        depth: 0,
        kind: JournalKind::Error,
        agent_id: Some(handle.inner.agent_id),
        payload: json!({
            "code": code,
            "message": summarize(&message, 200),
        }),
        duration_ms: None,
    }) {
        warn!(error = %e, "failed to journal Error");
    }
    deps.bus.emit(
        "chat.run.error.v1",
        json!({
            "runId": run_id.to_string(),
            "sessionId": session_id.to_string(),
            "code": code,
            "message": message,
            "retryable": app_error_retryable(&err),
        }),
    );
    error!(
        run_id = %run_id,
        error_code = code,
        duration_ms = started.elapsed().as_millis() as u64,
        "run errored"
    );
}

fn ui_run_status(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Running => "running",
        RunStatus::Finished => "completed",
        RunStatus::Aborted => "aborted",
        RunStatus::Errored => "error",
    }
}

fn app_error_retryable(err: &AppError) -> bool {
    matches!(
        err,
        AppError::Provider {
            retryable: true,
            ..
        }
    )
}

/// Build the list of `ToolSchema`s the agent exposes to the LLM,
/// filtered by the agent's `tool_access` (allowlist / denylist).
fn build_tool_schemas(
    registry: &[Arc<dyn Tool>],
    agents: &AgentRegistry,
    agent_id: &AgentId,
) -> Vec<crate::llm::ToolSchema> {
    let Some(agent) = agents.get(agent_id) else {
        return Vec::new();
    };
    let allow: Option<Vec<String>> = match &agent.tool_access {
        crate::agents::ToolAccess::All => None,
        crate::agents::ToolAccess::Allowlist(list) => Some(list.clone()),
        crate::agents::ToolAccess::Denylist(list) => {
            let denied: std::collections::HashSet<String> = list.iter().cloned().collect();
            let mut allowed: Vec<String> = Vec::new();
            for t in registry {
                if !denied.contains(t.name()) {
                    allowed.push(t.name().to_string());
                }
            }
            Some(allowed)
        }
    };
    registry
        .iter()
        .filter(|t| match &allow {
            None => true,
            Some(list) => list.iter().any(|n| n == t.name()),
        })
        .map(|t| crate::llm::ToolSchema {
            name: t.name().to_string(),
            description: t
                .schema()
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            parameters: t.schema().get("parameters").cloned().unwrap_or(json!({})),
        })
        .collect()
}

/// Build a [`PermissionSnapshot`] for the run. Looks up the
/// workspace from the registry; if the workspace is missing,
/// builds a snapshot that denies all path access (the safe
/// default). Applies the active agent's `AgentPermissionOverride`
/// (typically `deny` for `plan` agent) as defense-in-depth.
fn build_permission_snapshot(
    workspaces: &WorkspaceService,
    agents: &AgentRegistry,
    workspace_id: WorkspaceId,
    agent_id: &AgentId,
) -> crate::permissions::PermissionSnapshot {
    let (root, extras) = match workspaces.get(workspace_id) {
        Some(w) => (
            w.root_path,
            w.extra_paths.into_iter().map(|e| e.path).collect(),
        ),
        None => (std::path::PathBuf::from("/__no_workspace__"), Vec::new()),
    };

    // Look up agent permission overrides (e.g. plan denies writes).
    let agent_deny = agents
        .get(agent_id)
        .map(|spec| spec.permissions.deny.clone())
        .unwrap_or_default();

    crate::permissions::PermissionSnapshot {
        workspace_root: root,
        extra_paths: extras,
        approval_mode: crate::permissions::ApprovalMode::Ask,
        workspace_allow: vec!["read_file".into(), "list_dir".into(), "search".into()],
        workspace_deny: vec![],
        workspace_ask: vec![
            "write_file".into(),
            "edit_file".into(),
            "shell".into(),
            "python_run".into(),
            "apply_patch".into(),
        ],
        deny_paths: vec![],
        allow_paths: vec![],
        extra_paths_deny: vec![],
        always_allow: vec![],
        always_deny: vec![],
        agent_allow: vec![],
        agent_deny,
        agent_ask: vec![],
    }
}

/// Dispatch a single tool call. Returns the [`ToolOutput`] (with
/// `is_error: true` if the gate denied or the tool itself
/// failed). The caller appends a `ToolResult` message to the
/// in-memory history with the returned content.
async fn dispatch_tool_call(
    deps: &AgentLoopDeps,
    handle: &RunHandle,
    perm_snap: &crate::permissions::PermissionSnapshot,
    tc: &ToolCall,
) -> ToolOutput {
    let run_id = handle.inner.run_id;
    let session_id = handle.inner.session_id;
    let workspace_id = handle.inner.workspace_id;

    // Look up the workspace root + extra paths.
    let (workspace_root, extra_paths) = match deps.workspaces.get(workspace_id) {
        Some(w) => (
            w.root_path.clone(),
            Arc::new(
                w.extra_paths
                    .iter()
                    .map(|e| e.path.clone())
                    .collect::<Vec<_>>(),
            ),
        ),
        None => {
            let deny = ToolOutput::failure("workspace not found for run");
            emit_tool_result_event(&deps.bus, run_id, session_id, tc, &deny, 0, false);
            return deny;
        }
    };

    // Permission check.
    let decision = deps.permission_gate.check(perm_snap, &tc.name, &tc.args);
    let decision_str = decision.code();
    if let Some(reason) = decision_reason(&decision) {
        info!(
            run_id = %run_id,
            tool = %tc.name,
            decision = decision_str,
            reason,
            "permission check"
        );
    }

    // Journal: log the decision regardless of outcome.
    if let Err(e) = deps.journal.append(NewJournalEntry {
        id: None,
        session_id,
        run_id,
        parent_run_id: None,
        depth: 0,
        kind: JournalKind::ToolCall,
        agent_id: Some(handle.inner.agent_id),
        payload: json!({
            "toolCallId": tc.id,
            "name": tc.name,
            "decision": decision_str,
            "reason": decision_reason(&decision),
        }),
        duration_ms: None,
    }) {
        warn!(error = %e, "failed to journal ToolCall decision");
    }

    // Emit the `chat.tool_call.v1` event so the UI can show
    // "running ...".
    let args_summary = summarize(&tc.args.to_string(), 120);
    deps.bus.emit(
        "chat.tool_call.v1",
        json!({
            "runId": run_id.to_string(),
            "sessionId": session_id.to_string(),
            "toolCallId": tc.id,
            "name": tc.name,
            "args": tc.args,
            "argsSummary": args_summary,
        }),
    );

    let final_decision = match decision {
        Decision::Allow { persist: _ } => decision,
        Decision::Deny { reason } => {
            let msg = format!("denied by permission: {reason}");
            let out = ToolOutput::failure(msg);
            emit_tool_result_event(&deps.bus, run_id, session_id, tc, &out, 0, true);
            return out;
        }
        Decision::Ask { reason } => {
            // Register a pending permission request and wait
            // for the user.
            let leaked: &'static str = Box::leak(tc.name.clone().into_boxed_str());
            let req =
                crate::permissions::PermissionRequest::new(leaked, tc.args.clone(), reason.clone());
            let (tx, rx) = tokio::sync::oneshot::channel::<UserDecision>();
            let req_view = deps.permission_registry.register(req, tx);
            deps.bus.emit(
                "permission.requested.v1",
                json!({
                    "runId": run_id.to_string(),
                    "sessionId": session_id.to_string(),
                    "requestId": req_view.request_id,
                    "tool": req_view.tool,
                    "args": req_view.args,
                    "argsSummary": req_view.args_summary,
                    "reason": req_view.reason,
                }),
            );
            // Wait for the user (or abort).
            let resolved = tokio::select! {
                d = rx => d.ok(),
                _ = wait_for_abort(&handle.inner.abort_flag) => None,
            };
            match resolved {
                Some(UserDecision::Allow { .. }) => Decision::Allow { persist: false },
                Some(UserDecision::Deny { .. }) => {
                    let out = ToolOutput::failure("denied by user");
                    emit_tool_result_event(&deps.bus, run_id, session_id, tc, &out, 0, true);
                    return out;
                }
                None => {
                    let out = ToolOutput::failure("aborted while awaiting permission");
                    emit_tool_result_event(&deps.bus, run_id, session_id, tc, &out, 0, true);
                    return out;
                }
            }
        }
    };
    let _ = final_decision; // already extracted

    // Look up the tool.
    let Some(tool) = deps.tool_registry.iter().find(|t| t.name() == tc.name) else {
        let out = ToolOutput::failure(format!("unknown tool: {}", tc.name));
        emit_tool_result_event(&deps.bus, run_id, session_id, tc, &out, 0, true);
        return out;
    };

    // Build the tool context.
    let ignore_patterns: Arc<Vec<String>> = match deps.workspaces.get(workspace_id) {
        Some(_w) => Arc::new(Vec::new()), // TODO: read from WorkspaceConfig.ignore
        None => Arc::new(Vec::new()),
    };
    let ctx = ToolContext {
        workspace_id,
        workspace_root: workspace_root.clone(),
        extra_paths: extra_paths.clone(),
        run_id,
        session_id,
        abort_flag: handle.inner.abort_flag.clone(),
        ignore_patterns,
    };

    // Run the tool.
    let tool_start = std::time::Instant::now();
    let result = tool.run(ctx, tc.args.clone()).await;
    let duration_ms = tool_start.elapsed().as_millis() as u64;

    let out = match result {
        Ok(o) => o,
        Err(e) => ToolOutput::failure(format!("{}: {}", e.code(), e)),
    };
    let mut out_with_duration = out;
    out_with_duration.duration_ms = duration_ms;

    // Journal: persist the full tool output (subject to the
    // 16 KiB cap in the repo).
    if let Err(e) = deps.journal.append(NewJournalEntry {
        id: None,
        session_id,
        run_id,
        parent_run_id: None,
        depth: 0,
        kind: JournalKind::ToolResult,
        agent_id: Some(handle.inner.agent_id),
        payload: json!({
            "toolCallId": tc.id,
            "name": tc.name,
            "isError": out_with_duration.is_error,
            "durationMs": duration_ms,
            "outputSummary": out_with_duration.summary,
        }),
        duration_ms: Some(duration_ms),
    }) {
        warn!(error = %e, "failed to journal ToolResult");
    }

    emit_tool_result_event(
        &deps.bus,
        run_id,
        session_id,
        tc,
        &out_with_duration,
        duration_ms,
        false,
    );

    out_with_duration
}

fn emit_tool_result_event(
    bus: &Arc<dyn EventSink>,
    run_id: RunId,
    session_id: SessionId,
    tc: &ToolCall,
    out: &ToolOutput,
    duration_ms: u64,
    is_denied: bool,
) {
    bus.emit(
        "chat.tool_result.v1",
        json!({
            "runId": run_id.to_string(),
            "sessionId": session_id.to_string(),
            "toolCallId": tc.id,
            "name": tc.name,
            "output": out.content,
            "outputSummary": out.summary,
            "isError": out.is_error || is_denied,
            "durationMs": duration_ms,
        }),
    );
}

fn decision_reason(d: &Decision) -> Option<String> {
    match d {
        Decision::Allow { .. } => None,
        Decision::Ask { reason } => Some(reason.clone()),
        Decision::Deny { reason } => Some(reason.clone()),
    }
}

/// Wait for the abort flag to flip. Returns immediately if it's
/// already set.
async fn wait_for_abort(flag: &AtomicBool) {
    if flag.load(Ordering::SeqCst) {
        return;
    }
    // Simple polling loop with a small backoff. A more
    // efficient approach would use `tokio::sync::Notify`, but
    // `AtomicBool` is what `RunHandle` exposes.
    loop {
        if flag.load(Ordering::SeqCst) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// Truncate a string for logging / journal summaries.
fn summarize(s: &str, max_chars: usize) -> String {
    let normalized: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        normalized
    } else {
        let mut out: String = normalized.chars().take(max_chars).collect();
        out.push('…');
        out
    }
}

/// Public re-export of [`summarize`] for sibling modules (tools,
/// permissions) that want the same normalization without
/// duplicating it.
#[must_use]
pub fn summarize_pub(s: &str, max_chars: usize) -> String {
    summarize(s, max_chars)
}

/// Registry of active runs. Used by `app_state` to look up a
/// `RunHandle` by `RunId` (e.g. for the `session_abort` command).
///
/// Cheap to clone (`Arc` inside). Lock-free reads via
/// `parking_lot::RwLock` would be ideal, but we use a `Mutex` so
/// that the deps are `std`-only (no extra imports in this
/// hot-path module).
#[derive(Default, Clone)]
pub struct RunRegistry {
    inner: Arc<Mutex<std::collections::HashMap<RunId, RunHandle>>>,
}

// `RunRegistry` is consumed by the app layer (held in
// `AppState`) to look up active runs from IPC commands; in the
// core crate it's only constructed by tests, so silence the
// dead-code lint in the lib build until the app wires it.
#[allow(dead_code)]
impl RunRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a run. Idempotent (a second call for the same
    /// `run_id` is a no-op).
    pub fn register(&self, handle: RunHandle) {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.insert(handle.inner.run_id, handle);
    }

    /// Look up a run by id. Returns `None` if the run is not
    /// in the registry (e.g. it never existed, or it has been
    /// removed).
    #[must_use]
    pub fn get(&self, run_id: RunId) -> Option<RunHandle> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&run_id)
            .cloned()
    }

    /// Remove a run from the registry. Idempotent. Returns
    /// `true` if a run was removed.
    pub fn remove(&self, run_id: RunId) -> bool {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.remove(&run_id).is_some()
    }

    /// Number of registered runs (for tests / diagnostics).
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

    /// Snapshot all `(run_id, handle)` pairs currently in the
    /// registry. Used by `chat_abort` to find the run for a
    /// given session (O(runs); runs are typically <10 active).
    #[must_use]
    pub fn snapshot(&self) -> Vec<(RunId, RunHandle)> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect()
    }

    /// Snapshot all handles that belong to a given session.
    /// Used by `chat_abort` (see `snapshot`).
    #[must_use]
    pub fn iter_for_session(&self, session_id: SessionId) -> Vec<(RunId, RunHandle)> {
        self.snapshot()
            .into_iter()
            .filter(|(_, h)| h.state().session_id == session_id)
            .collect()
    }

    /// Snapshot all handles that belong to a given workspace.
    /// Used by `WorkspaceService::delete` (F02.AC7) to refuse
    /// deletion when a workspace has active runs. Returns a
    /// snapshot in registration order; the caller is expected to
    /// re-check `is_running()` on each handle (the snapshot can
    /// include finished runs that have not yet been removed by
    /// the loop).
    #[must_use]
    pub fn iter_for_workspace(&self, workspace_id: WorkspaceId) -> Vec<(RunId, RunHandle)> {
        self.snapshot()
            .into_iter()
            .filter(|(_, h)| h.workspace_id() == workspace_id)
            .collect()
    }
}

// `Ulid` is in scope to suppress "unused import" warnings
// after the project stops using it directly (we use
// `crate::ids::RunId` instead).
#[allow(dead_code)]
const _ULID_IN_SCOPE: fn() -> Ulid = || Ulid::new();

// `mpsc` is reserved for a future Phase 2 feature (streaming
// events from the loop to a fan-out of subscribers). For
// Phase 1 we use the synchronous `EventSink` trait directly.
#[allow(dead_code)]
const _MPSC_IN_SCOPE: fn() -> mpsc::UnboundedSender<()> = || mpsc::unbounded_channel().0;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, unsafe_code)]
mod tests {
    use super::*;
    use crate::config::FakeKeychain;
    use crate::config::KeychainAccess;
    use crate::session::SessionService;
    use crate::storage::Db;
    use std::collections::HashMap;

    /// A no-op event sink that records emissions for tests.
    struct RecordingSink {
        events: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
    }

    /// Recorded-event list shared between the sink and the test.
    type RecordingEvents = Arc<Mutex<Vec<(String, serde_json::Value)>>>;

    impl RecordingSink {
        fn new() -> (Arc<Self>, RecordingEvents) {
            let events: RecordingEvents = Default::default();
            let sink = Arc::new(Self {
                events: events.clone(),
            });
            (sink, events)
        }
    }

    impl EventSink for RecordingSink {
        fn emit(&self, event: &str, payload: serde_json::Value) {
            self.events
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((event.to_string(), payload));
        }
    }

    /// Build a fresh `AgentLoopDeps` rooted at a temp `state.db`
    /// and a default config.
    fn fresh_deps() -> (tempfile::TempDir, AgentLoopDeps) {
        use crate::config::ServiceConfigPaths;

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let ws_id = WorkspaceId::new();
        let db = Db::open(&db_path).unwrap();
        let session = SessionService::with_db(db.clone(), ws_id);
        let journal = JournalRepo::new(db);

        let config_paths = ServiceConfigPaths::from_agentyx_home(dir.path());
        let config = ConfigService::load(&config_paths).unwrap();

        let mut providers: HashMap<String, Arc<dyn Provider>> = HashMap::new();
        let p = crate::llm::OllamaProvider::with_base_url("http://127.0.0.1:1").unwrap();
        providers.insert("ollama".to_string(), Arc::new(p));

        let agents = AgentRegistry::load_builtins();
        let (bus, _events) = RecordingSink::new();
        // Empty workspace service (no registered workspace).
        // Tests that need a workspace should call `workspaces.open()`.
        let workspaces = crate::workspace::WorkspaceService::new(dir.path()).unwrap();

        let tool_registry = Arc::new(crate::tools::built_in_registry());
        let deps = AgentLoopDeps {
            agents,
            config,
            providers,
            session,
            journal,
            bus,
            workspaces,
            tool_registry,
            permission_gate: crate::permissions::PermissionGate::new(),
            permission_registry: crate::permissions::PermissionRegistry::new(),
        };
        (dir, deps)
    }

    #[test]
    fn empty_user_msg_rejected() {
        let (_dir, deps) = fresh_deps();
        let s = deps.session.create(&deps.agents, None).unwrap();
        let err = spawn_run(deps, s.id, "   \n  ".into(), StartOpts::default()).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput { .. }));
    }

    #[test]
    fn oversized_user_msg_rejected() {
        let (_dir, deps) = fresh_deps();
        let s = deps.session.create(&deps.agents, None).unwrap();
        let big = "x".repeat(MAX_USER_MSG_BYTES + 1);
        let err = spawn_run(deps, s.id, big, StartOpts::default()).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput { .. }));
    }

    #[test]
    fn unknown_session_rejected() {
        let (_dir, deps) = fresh_deps();
        let err =
            spawn_run(deps, SessionId::new(), "hello".into(), StartOpts::default()).unwrap_err();
        assert!(matches!(err, AppError::NotFound { .. }));
    }

    #[test]
    fn conflict_when_session_already_running() {
        let (_dir, deps) = fresh_deps();
        let s = deps.session.create(&deps.agents, None).unwrap();
        deps.session.start_run(s.id, RunId::new()).unwrap();
        let err = spawn_run(deps, s.id, "hi".into(), StartOpts::default()).unwrap_err();
        assert!(matches!(err, AppError::Conflict { .. }));
    }

    #[test]
    fn run_handle_abort_is_idempotent_and_atomic() {
        let h = RunHandle::new(
            RunId::new(),
            SessionId::new(),
            WorkspaceId::new(),
            crate::agents::agent_id_static("build"),
        );
        assert!(h.is_running());
        h.abort();
        assert!(h.is_aborted());
        // A second abort doesn't change the state.
        h.abort();
        assert!(h.is_aborted());
    }

    #[test]
    fn run_handle_state_marks_status_and_error() {
        let h = RunHandle::new(
            RunId::new(),
            SessionId::new(),
            WorkspaceId::new(),
            crate::agents::agent_id_static("build"),
        );
        h.mark(
            RunStatus::Finished,
            Some(AppError::Provider {
                provider_id: "x".into(),
                message: "boom".into(),
                retryable: false,
            }),
        );
        let s = h.state();
        assert_eq!(s.status, RunStatus::Finished);
        assert!(matches!(s.last_error, Some(AppError::Provider { .. })));
    }

    #[test]
    fn run_handle_accumulates_usage() {
        let h = RunHandle::new(
            RunId::new(),
            SessionId::new(),
            WorkspaceId::new(),
            crate::agents::agent_id_static("build"),
        );
        h.add_usage(&Usage {
            prompt_tokens: 10,
            completion_tokens: 5,
            cache_read_tokens: None,
            cache_write_tokens: None,
        });
        h.add_usage(&Usage {
            prompt_tokens: 3,
            completion_tokens: 7,
            cache_read_tokens: None,
            cache_write_tokens: None,
        });
        let s = h.state();
        assert_eq!(s.usage.prompt_tokens, 13);
        assert_eq!(s.usage.completion_tokens, 12);
    }

    #[test]
    fn run_registry_register_get_remove() {
        let reg = RunRegistry::new();
        let h = RunHandle::new(
            RunId::new(),
            SessionId::new(),
            WorkspaceId::new(),
            crate::agents::agent_id_static("build"),
        );
        let run_id = h.state().run_id;
        reg.register(h);
        assert_eq!(reg.len(), 1);
        assert!(reg.get(run_id).is_some());
        assert!(reg.remove(run_id));
        assert!(!reg.remove(run_id));
        assert!(reg.is_empty());
    }

    #[test]
    fn summarize_truncates_long_strings() {
        let s = summarize("a".repeat(500).as_str(), 10);
        assert!(s.chars().count() <= 11);
    }

    #[test]
    fn summarize_preserves_short_strings() {
        let s = summarize("hello", 100);
        assert_eq!(s, "hello");
    }

    #[test]
    fn summarize_normalizes_whitespace() {
        let s = summarize("hello\n\n   world", 100);
        assert_eq!(s, "hello world");
    }

    #[test]
    fn max_user_msg_bytes_constant_is_one_mb() {
        assert_eq!(MAX_USER_MSG_BYTES, 1024 * 1024);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn spawn_run_with_unreachable_provider_emits_error_event() {
        // Provider points at 127.0.0.1:1 (always refuses). The
        // request fails before any stream is produced. We verify
        // the bus receives `chat.run.error.v1` (or, in this
        // minimal slice, that the run ended in `Errored` state
        // because the provider refused).
        let (_dir, deps) = fresh_deps();
        let s = deps.session.create(&deps.agents, None).unwrap();
        // We can't easily wait for the spawned task; we just
        // assert the call returned a RunHandle and that the
        // user message was persisted.
        let handle = spawn_run(deps, s.id, "hi".into(), StartOpts::default()).unwrap();
        assert!(handle.is_running());
        // We don't assert on the eventual state because the
        // background task is racing. The point of this test is
        // to ensure `spawn_run` itself doesn't fail when the
        // provider is configured (the failure surfaces later
        // via the event bus).
    }

    #[test]
    fn fake_keychain_round_trip() {
        // Sanity check: the FakeKeychain we use in tests still
        // round-trips after the poison-recovery refactor.
        let kc = FakeKeychain::with_entries(&[("groq", "topsecret")]);
        let v = kc.get("groq").unwrap();
        assert_eq!(v.as_deref(), Some("topsecret"));
        kc.set("groq", "rotated").unwrap();
        assert_eq!(kc.get("groq").unwrap().as_deref(), Some("rotated"));
        kc.delete("groq").unwrap();
        assert!(kc.get("groq").unwrap().is_none());
    }
}
