//! Agentyx core — pure Rust domain library.
//!
//! This crate is the heart of the application: all business logic
//! (agent loop, providers, tools, storage, journal, permissions,
//! sessions, workspaces, agents) lives here. It has **no** Tauri
//! dependency, which means it can be:
//!
//! - Unit-tested with `cargo test` (no UI / no windowing).
//! - Embedded in a Tauri app (`agentyx-app`), in a future CLI,
//!   in a server (v0.2 with F06), or in the `agentyx-sdk` for
//!   third-party integrations.
//! - Compiled to WASM in the future (post v1).
//!
//! ## Module layout (planned, see `../specs/architecture.md`)
//!
//! - `agent`     — the ReAct agent loop, tool calls, prompts.
//! - `agents`    — multi-agent registry: `AgentSpec`, `AgentRegistry`,
//!                 `Primary | Subagent | Hidden`.
//! - `llm`       — LLM provider trait + Ollama / Groq / Minimax impls.
//! - `tools`     — read_file, write_file, edit_file, search,
//!                 shell, python_run, list_dir, apply_patch.
//! - `storage`   — SQLite, migrations, repos for sessions/messages/journal.
//! - `journal`   — append-only action log (in `state.db`).
//! - `permissions` — permission matrix and gate.
//! - `config`    — TOML config + `SecretRef` (env / keychain).
//! - `workspace` — workspace model + `extra_paths` (ADR-0007). [implemented]
//! - `session`   — sessions, runs, messages, child sessions.
//! - `pty`       — PTY wrapper on `portable-pty`.
//! - `error`     — `AppError` + `From` impls.
//! - `ids`       — `Ulid` newtype wrappers (`SessionId`, `RunId`, ...).
//!
//! ## Conventions (see `../../AGENTS.md` §4)
//!
//! - Edition 2021, MSRV 1.80+.
//! - `Result<T, AppError>` everywhere; no `unwrap` in production.
//! - `&str` over `String`, `&[T]` over `Vec<T>` in signatures.
//! - Errors: `#[derive(thiserror::Error)]` + variants per failure mode.
//! - All public types `Serialize + Deserialize` if they cross IPC.
//! - camelCase JSON via `#[serde(rename_all = "camelCase")]`.
//!
//! See the specs under `../specs/` for the contract this crate
//! implements. Tests live next to the code or under `tests/`.

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]

pub mod error;
pub mod ids;
pub mod workspace;

pub use error::{AppError, AppResult};
pub use ids::*;

/// Re-export of the `tracing` crate so call sites can do
/// `use agentyx_core::tracing;` if they prefer.
pub use ::tracing;
