//! Tauri command handlers — the IPC surface between UI and core.
//!
//! Every `#[tauri::command]` lives in a sub-module by concern:
//!
//! - `session`     — create, send, abort, list sessions (F01).
//! - `workspace`   — list, open, delete workspaces + extra paths (F02).
//! - `config`      — get/update global + workspace config (F05).
//! - `agents`      — list, get agent specs (multi-agent, F-agents-ui).
//! - `providers`   — test provider connection (F05).
//! - `secrets`     — set/delete/list API key secrets via keychain (F05).
//! - `permissions` — get/set permission matrix, respond to prompts
//!   (F01, F12 in v0.2).
//!
//! Conventions (see `../../specs/ipc.md`):
//! - All commands are `async fn` and return `Result<T, AppError>`.
//! - All args and returns are typed and `Serialize + Deserialize`.
//! - Errors are surfaced as `{ code, message, context? }` (the
//!   `AppError` derive already does this).
//! - State is `tauri::State<'_, Arc<AppState>>` (cheap clone).
//! - Long-running work is spawned on the Tokio runtime; commands
//!   that initiate a run return a `RunHandle` immediately.

pub mod agents;
pub mod config;
pub mod permissions;
pub mod providers;
pub mod secrets;
pub mod session;
pub mod workspace;

mod shared;
pub use shared::*;
