//! Session domain — chat threads, messages, and runs.
//!
//! See `../../../specs/domains/session.md` for the full design.
//!
//! ## What's in this PR (F01-Phase1)
//!
//! - `Session` and `Message` data types (Rust structs).
//! - `SessionService` — high-level operations over a `Db`:
//!   `create`, `list`, `get`, `delete`, `list_messages`,
//!   `append_message`, `set_status`, `set_active_agent`,
//!   `get_active_agent`, `start_run`, `finish_run`.
//! - Per-session `state.db` opened lazily by the service
//!   constructor.
//! - Tests covering AC1–AC11 of session.md (modulo the
//!   `agent-loop` deferred bits).
//!
//! ## Deferred
//!
//! - Pagination with `before` ULID (F01-Phase2; F01-Phase1
//!   returns the most recent `limit`).
//! - Stats `usage` aggregation.
//! - Multi-workspace transaction (cross-workspace, out of scope).

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod service;
mod types;

pub use service::{ListMessagesOpts, ListOpts, SessionService};
pub use types::{Message, MessageRole, Session, SessionStatus};
