//! The agent loop — orchestrator that runs a session's turn.
//!
//! See `../../../specs/domains/agent-loop.md` for the full design
//! and `../../docs/architecture.md` for the data flow diagram.
//!
//! ## Scope (F01-Phase1 backends + F01-Phase2-core)
//!
//! Phase 1 (merged in PR #13) delivered: spawn_run / RunHandle /
//! EventSink / single-turn loop, journal + session persistence,
//! Ollama provider, abort, and run state tracking.
//!
//! Phase 2-core (this PR) adds:
//! - **Tool dispatch**: when the provider emits a `ToolUse`
//!   event, the loop looks up the tool in the
//!   [`crate::tools::ToolRegistry`], runs it with a
//!   [`crate::tools::ToolContext`], and surfaces the result as
//!   `chat.tool_call.v1` + `chat.tool_result.v1` (F01.AC3).
//! - **Permission gate**: every tool call goes through the
//!   [`crate::permissions::PermissionGate`]. `Ask` decisions
//!   emit `permission.requested.v1` and pause the run until
//!   the user responds (F01.AC7).
//! - **Args/output summaries**: large tool args / outputs have
//!   truncated summaries in the events; full content is
//!   persisted in the journal (F01.AC8).
//! - **Delta batching**: `chat.content.delta.v1` events are
//!   emitted at most every 50ms or 100 chars (F01.AC12).
//! - **Multi-step loop**: the loop can iterate up to
//!   `max_steps` (default 50), feeding tool results back to the
//!   model. v0.1 keeps it bounded and simple — no parallel
//!   tool calls, no subagent routing.
//!
//! Out of scope for Phase 2-core (planned for follow-ups):
//! - Subagent invocation (the `task` tool call) — F01-Phase3.
//! - @mention expansion — F01-Phase3.
//! - Per-delta persistence batching to 500ms — the loop
//!   persists messages at natural cut points (end of step,
//!   tool call, run finish) which satisfies F01.AC13 for v0.1.
//! - Tauri command for `permission_respond` — follow-up PR
//!   wires the IPC command and consumes the
//!   [`crate::permissions::PermissionRegistry`].

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod batcher;
mod loop_;

pub use batcher::{BatcherConfig, DeltaBatcher};
pub use loop_::{
    spawn_run, summarize_pub, AgentLoopDeps, EventSink, RunHandle, RunRegistry, RunState,
    RunStatus, StartOpts, MAX_USER_MSG_BYTES,
};
