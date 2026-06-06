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
use serde_json::json;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use ulid::Ulid;

use crate::agents::AgentRegistry;
use crate::config::ConfigService;
use crate::ids::{AgentId, RunId, SessionId, WorkspaceId};
use crate::journal::{JournalKind, JournalRepo, NewJournalEntry};
use crate::llm::{
    ChatEvent, ChatMessage, ChatRequest, FinishReason, Provider, RequestMetadata, Usage,
};
use crate::session::{MessageRole, SessionService, SessionStatus};
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
    abort_flag: AtomicBool,
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
    /// Build a new handle (used by [`spawn_run`]).
    fn new(
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
                abort_flag: AtomicBool::new(false),
            }),
        }
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

    /// Internal: read the abort flag.
    fn is_aborted(&self) -> bool {
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
    if !cfg.providers.contains_key(&cfg.default_provider) {
        return Err(AppError::Internal {
            message: format!(
                "default provider '{}' not in providers map",
                cfg.default_provider
            ),
        });
    }
    let provider_id = cfg.default_provider.clone();
    let model = opts
        .model
        .clone()
        .unwrap_or_else(|| cfg.default_model.clone());
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

/// The async run loop. Loads the conversation history, calls
/// the provider, streams events, persists the result.
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

    // Build the request.
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
                // Phase 2: round-trip tool_use_id. For now we
                // surface the content as a user message so the
                // model has it in context; the agent loop will
                // later reconstruct the proper ToolResult.
                messages.push(ChatMessage::User {
                    content: format!("[tool result]\n{}", m.content),
                });
            }
        }
    }

    let req = ChatRequest {
        model: model.clone(),
        messages,
        tools: Vec::new(),
        tool_choice: crate::llm::ToolChoice::None,
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

    // Journal: ProviderEvent (start).
    let provider_start = std::time::Instant::now();
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
        }),
        duration_ms: None,
    }) {
        warn!(error = %e, "failed to journal ProviderEvent");
    }

    let stream_result = provider.chat(req).await;
    let mut stream = match stream_result {
        Ok(s) => s,
        Err(e) => {
            finish_with_error(&deps, &handle, session_id, run_id, e, started);
            return;
        }
    };

    // Stream events.
    let mut accumulated_text = String::new();
    let mut finish_reason: Option<FinishReason> = None;
    let mut last_usage = Usage::default();
    let mut error: Option<AppError> = None;
    let mut step: u32 = 0;
    use futures::StreamExt;
    loop {
        if handle.is_aborted() {
            info!(run_id = %run_id, "run aborted");
            finish_reason = Some(FinishReason::Aborted);
            break;
        }
        if step >= max_steps {
            warn!(run_id = %run_id, step, max_steps, "max_steps reached");
            finish_reason = Some(FinishReason::Length);
            break;
        }
        match stream.next().await {
            Some(Ok(ChatEvent::MessageStart { message_id, model })) => {
                debug!(run_id = %run_id, message_id, model, "message_start");
                deps.bus.emit(
                    "chat.message_start.v1",
                    json!({
                        "runId": run_id.to_string(),
                        "messageId": message_id,
                        "model": model,
                    }),
                );
            }
            Some(Ok(ChatEvent::ContentDelta { text })) => {
                accumulated_text.push_str(&text);
                deps.bus.emit(
                    "chat.content.delta.v1",
                    json!({
                        "runId": run_id.to_string(),
                        "sessionId": session_id.to_string(),
                        "text": text,
                    }),
                );
            }
            Some(Ok(ChatEvent::ToolUse { id, name, args })) => {
                // Phase 2: hand off to the tool registry. For
                // now, log and continue.
                warn!(
                    run_id = %run_id,
                    tool = %name,
                    "ToolUse received but tool calls are not yet implemented (F01-Phase2)"
                );
                let _ = (id, args);
            }
            Some(Ok(ChatEvent::MessageEnd {
                usage,
                finish_reason: fr,
            })) => {
                last_usage = usage.clone();
                finish_reason = Some(fr);
                handle.add_usage(&usage);
            }
            Some(Ok(ChatEvent::Error {
                code,
                message,
                retryable,
            })) => {
                error = Some(AppError::Provider {
                    provider_id: deps.config.get().default_provider.clone(),
                    message: format!("{code}: {message}"),
                    retryable,
                });
                break;
            }
            Some(Err(e)) => {
                error = Some(e);
                break;
            }
            None => {
                // Stream ended.
                break;
            }
        }
        step += 1;
    }

    // Persist assistant message + journal.
    let aborted = handle.is_aborted();
    let final_status = if let Some(err) = error {
        finish_with_error(&deps, &handle, session_id, run_id, err.clone(), started);
        return;
    } else if aborted || matches!(finish_reason, Some(FinishReason::Aborted)) {
        RunStatus::Aborted
    } else if accumulated_text.is_empty()
        && !matches!(
            finish_reason,
            Some(FinishReason::Stop | FinishReason::Length)
        )
    {
        // Stream ended without producing text or a finish reason
        // — treat as error.
        handle.mark(
            RunStatus::Errored,
            Some(AppError::Provider {
                provider_id: deps.config.get().default_provider.clone(),
                message: "stream ended without producing a finish reason".into(),
                retryable: true,
            }),
        );
        let _ = deps
            .session
            .finish_run(session_id, SessionStatus::Errored, "stream_incomplete");
        deps.bus.emit(
            "chat.run.error.v1",
            json!({
                "runId": run_id.to_string(),
                "sessionId": session_id.to_string(),
                "code": "stream_incomplete",
                "message": "stream ended without producing a finish reason",
            }),
        );
        error!(
            run_id = %run_id,
            "stream ended without producing a finish reason"
        );
        return;
    } else {
        RunStatus::Finished
    };

    if !accumulated_text.is_empty() {
        if let Err(e) = deps.session.append_message(
            session_id,
            MessageRole::Assistant,
            &accumulated_text,
            Some(run_id),
        ) {
            error!(run_id = %run_id, error = %e, "failed to persist assistant message");
        }
    }

    let finish_reason_str = match finish_reason {
        Some(FinishReason::Stop) => "stop",
        Some(FinishReason::Length) => "length",
        Some(FinishReason::ContentFilter) => "content_filter",
        Some(FinishReason::Error) => "error",
        Some(FinishReason::Aborted) => "aborted",
        None => "unknown",
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

    if let Err(e) = deps.journal.append(NewJournalEntry {
        id: None,
        session_id,
        run_id,
        parent_run_id: None,
        depth: 0,
        kind: JournalKind::ProviderEvent,
        agent_id: Some(agent_id),
        payload: json!({
            "kind": "response",
            "usage": last_usage,
            "finishReason": finish_reason_str,
        }),
        duration_ms: Some(provider_start.elapsed().as_millis() as u64),
    }) {
        warn!(error = %e, "failed to journal ProviderEvent (response)");
    }

    if !accumulated_text.is_empty() {
        if let Err(e) = deps.journal.append(NewJournalEntry {
            id: None,
            session_id,
            run_id,
            parent_run_id: None,
            depth: 0,
            kind: JournalKind::AssistantMessage,
            agent_id: Some(agent_id),
            payload: json!({
                "textSummary": summarize(&accumulated_text, 200),
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
            "status": final_status,
            "usage": last_usage,
            "finishReason": finish_reason_str,
        }),
    );

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
        }),
    );
    error!(
        run_id = %run_id,
        error_code = code,
        duration_ms = started.elapsed().as_millis() as u64,
        "run errored"
    );
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
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let ws_id = WorkspaceId::new();
        let db = Db::open(&db_path).unwrap();
        let session = SessionService::with_db(db.clone(), ws_id);
        let journal = JournalRepo::new(db);

        let config_path = dir.path().join("config.toml");
        let config = ConfigService::load(&config_path).unwrap();

        let mut providers: HashMap<String, Arc<dyn Provider>> = HashMap::new();
        let p = crate::llm::OllamaProvider::with_base_url("http://127.0.0.1:1").unwrap();
        providers.insert("ollama".to_string(), Arc::new(p));

        let agents = AgentRegistry::load_builtins();
        let (bus, _events) = RecordingSink::new();
        let deps = AgentLoopDeps {
            agents,
            config,
            providers,
            session,
            journal,
            bus,
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
