//! The agent loop — orchestrator that runs a session's turn.
//!
//! See `../../../specs/domains/agent-loop.md` for the full design
//! and `../../docs/architecture.md` for the data flow diagram.
//!
//! ## Scope of this PR (F01-Phase1: backends only)
//!
//! This slice delivers a **minimal but real** implementation of
//! the loop. It is sufficient to drive the F01 Tauri commands
//! (`session_send`, `session_abort`, `session_list`,
//! `session_get_history`, `session_set_active_agent`,
//! `session_get_active_agent`) against an Ollama provider, and it
//! persists the entire conversation to `state.db` and the
//! journal. What it does **not** yet do (deferred to F01-Phase2
//! and later):
//!
//! - Tool calls (no `read_file`, `search`, `write_file`, etc. yet).
//! - Permission gate (no `PermissionDecision::Ask` flow yet).
//! - Subagents (no `task` tool call routing, no child sessions,
//!   no `@mention`).
//! - Multi-session in the same workspace (one active run at a
//!   time per session; the spec already allows multiple sessions,
//!   but no UI in Phase 1).
//! - Message-batching persistence (one INSERT per message; this
//!   changes in Phase 2 when delta batching is added per F01.AC12
//!   and F01.AC13).
//! - Delta-batching emission (one `chat.content.delta.v1` per
//!   `ContentDelta`; batching to 50ms / 100 chars in Phase 2).
//! - Provider retry on 429 (Phase 2; per F01.AC11).
//! - Context-window truncation (Phase 2; v0.1 truncates naively).
//!
//! The 84 tests in this crate (64 new for F01-Phase1) cover the
//! backends; the 15 F01.AC* and 18 agent-loop ACs in the spec
//! land in Phase 2 and the UI PR.

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod loop_;

pub use loop_::{
    spawn_run, AgentLoopDeps, EventSink, RunHandle, RunRegistry, RunState, RunStatus, StartOpts,
    MAX_USER_MSG_BYTES,
};
