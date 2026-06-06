//! Embedded migrations. Each migration is pure SQL DDL, applied
//! exactly once in order.

/// A single migration step.
pub struct Migration {
    /// Numeric id (matches the on-disk `__migrations.id`).
    pub id: i64,
    /// Slug used in tracing logs.
    pub name: &'static str,
    /// The DDL to execute. Multiple statements OK (run via
    /// `execute_batch`).
    pub sql: &'static str,
}

/// All migrations, in apply order.
#[must_use]
pub fn all() -> &'static [Migration] {
    MIGRATIONS
}

/// Migration 0001 — initial schema.
///
/// Creates the tables the rest of the core needs:
/// - `workspaces` (per storage.md §State)
/// - `sessions` (per session.md §State)
/// - `messages` (per session.md §State)
/// - `usage` (per session.md §State)
/// - `journal` (per journal.md §State)
///
/// `journal_archive` is added in migration 0002 to keep this one
/// tight.
const M0001_INITIAL: Migration = Migration {
    id: 1,
    name: "initial_schema",
    sql: "\
CREATE TABLE workspaces (
  id              TEXT PRIMARY KEY,
  root_path       TEXT NOT NULL UNIQUE,
  name            TEXT,
  created_at      INTEGER NOT NULL,
  last_opened_at  INTEGER NOT NULL
);
CREATE INDEX idx_workspaces_last_opened ON workspaces(last_opened_at DESC);

CREATE TABLE sessions (
  id                       TEXT PRIMARY KEY,
  workspace_id             TEXT NOT NULL,
  parent_id                TEXT,
  title                    TEXT,
  status                   TEXT NOT NULL CHECK (status IN ('idle','running','aborted','errored')),
  created_at               INTEGER NOT NULL,
  updated_at               INTEGER NOT NULL,
  last_run_id              TEXT,
  last_run_finish_reason   TEXT,
  active_agent_id          TEXT NOT NULL DEFAULT 'build',
  FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);
CREATE INDEX idx_sessions_ws_updated ON sessions(workspace_id, updated_at DESC);
CREATE INDEX idx_sessions_ws_status  ON sessions(workspace_id, status);

CREATE TABLE messages (
  id          TEXT PRIMARY KEY,
  session_id  TEXT NOT NULL,
  run_id      TEXT,
  role        TEXT NOT NULL CHECK (role IN ('user','assistant','system','tool_result')),
  content     TEXT NOT NULL,
  created_at  INTEGER NOT NULL,
  seq         INTEGER NOT NULL,
  FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);
CREATE INDEX idx_messages_session_seq ON messages(session_id, seq);
CREATE INDEX idx_messages_run         ON messages(run_id);

CREATE TABLE usage (
  id                 INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id         TEXT NOT NULL,
  run_id             TEXT NOT NULL,
  model_id           TEXT NOT NULL,
  prompt_tokens      INTEGER NOT NULL,
  completion_tokens  INTEGER NOT NULL,
  ts                 INTEGER NOT NULL,
  FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);
CREATE INDEX idx_usage_session_ts ON usage(session_id, ts DESC);

CREATE TABLE journal (
  id                    TEXT PRIMARY KEY,
  ts                    INTEGER NOT NULL,
  session_id            TEXT NOT NULL,
  run_id                TEXT NOT NULL,
  parent_run_id         TEXT,
  depth                 INTEGER NOT NULL DEFAULT 0,
  kind                  TEXT NOT NULL,
  agent_id              TEXT,
  payload               TEXT NOT NULL,
  payload_truncated     INTEGER NOT NULL DEFAULT 0,
  payload_sha256        TEXT,
  duration_ms           INTEGER,
  FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);
CREATE INDEX idx_journal_session_ts ON journal(session_id, ts);
CREATE INDEX idx_journal_run        ON journal(run_id);
",
};

/// Migration 0002 — journal archive table.
///
/// Same shape as `journal` (per journal.md §State). Adds the
/// `journal_archive` table for entries moved out of `journal`
/// by `archive_older_than` (F01-Phase2).
const M0002_JOURNAL_ARCHIVE: Migration = Migration {
    id: 2,
    name: "journal_archive",
    sql: "\
CREATE TABLE journal_archive (
  id                    TEXT PRIMARY KEY,
  ts                    INTEGER NOT NULL,
  session_id            TEXT NOT NULL,
  run_id                TEXT NOT NULL,
  parent_run_id         TEXT,
  depth                 INTEGER NOT NULL DEFAULT 0,
  kind                  TEXT NOT NULL,
  agent_id              TEXT,
  payload               TEXT NOT NULL,
  payload_truncated     INTEGER NOT NULL DEFAULT 0,
  payload_sha256        TEXT,
  duration_ms           INTEGER
);
CREATE INDEX idx_journal_archive_session_ts ON journal_archive(session_id, ts);
CREATE INDEX idx_journal_archive_kind_ts    ON journal_archive(kind, ts);
",
};

static MIGRATIONS: &[Migration] = &[M0001_INITIAL, M0002_JOURNAL_ARCHIVE];
