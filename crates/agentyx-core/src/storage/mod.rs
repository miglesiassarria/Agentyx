//! Storage domain — SQLite wrapper, schema, migrations.
//!
//! See `../../../specs/domains/storage.md` for the full design.
//! This module is the foundation for `session`, `journal`, and
//! the `usage` table.
//!
//! ## What's in this PR (F01-Phase1)
//!
//! - `Db` wrapper over `rusqlite::Connection` with PRAGMAs
//!   (`journal_mode=WAL`, `foreign_keys=ON`, `synchronous=NORMAL`,
//!   `busy_timeout=5000`, `temp_store=MEMORY`).
//! - Migrations loaded from `migrations/` directory in order.
//! - `with_conn` / `transaction` helpers.
//! - Per-workspace `state.db` and global `stats.db` openers.
//! - Tests for AC1 (open+migrate), AC2 (idempotent re-open),
//!   AC3 (rollback on bad migration), AC4 (path canonicalization
//!   rejection), AC12 (PRAGMAs applied).
//!
//! ## What's deferred to follow-up PRs
//!
//! - `journal::archive_older_than` (Edge case 6 in journal spec).
//! - `stats::record_token_usage` / `record_tool_latency` (stats.db
//!   writes — F01-Phase2 once we have Usage to record).
//! - Custom JSON migration runners (e.g. data backfill). All
//!   migrations in v1 are pure SQL DDL.

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod db;
mod migrations;

pub use db::Db;
