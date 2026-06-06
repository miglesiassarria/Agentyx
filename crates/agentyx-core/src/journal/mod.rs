//! Journal — append-only log of agent actions.
//!
//! See `../../../specs/domains/journal.md` for the full design.
//!
//! ## What's in this PR (F01-Phase1)
//!
//! - `JournalEntry` data type.
//! - `JournalKind` enum (subset: `UserMessage`, `AssistantMessage`,
//!   `ProviderEvent`, `Error`).
//! - `JournalRepo::append` (idempotent by id, payload truncation
//!   at 16 KiB, SHA-256 of the original).
//! - `JournalRepo::query_by_session` (filters: kinds, since/until,
//!   limit, before id).
//! - `JournalRepo::query_by_run`.
//! - `JournalRepo::count` and `archive_older_than` (deferred to
//!   F01-Phase2; for now `count` works and `archive_older_than`
//!   returns `Ok(0)`).
//! - Tests covering AC1, AC2, AC5, AC6, AC7, AC8, AC10.
//!
//! ## Deferred
//!
//! - Full 16 ACs of journal.md (subagent lifecycle, replay tree,
//!   archive_older_than with rows moved to journal_archive).

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod repo;
mod types;

pub use repo::{JournalRepo, NewJournalEntry};
pub use types::{JournalEntry, JournalKind};
