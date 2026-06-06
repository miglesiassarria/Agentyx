//! `JournalRepo` — append-only entry point.

use std::sync::Arc;

use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use ulid::Ulid;

use crate::ids::{AgentId, RunId, SessionId};
use crate::storage::Db;
use crate::{AppError, AppResult};

use super::types::{JournalEntry, JournalKind};

/// Default maximum payload size (16 KiB) before truncation.
pub const DEFAULT_MAX_PAYLOAD_BYTES: usize = 16 * 1024;

/// Input to [`JournalRepo::append`]. The repo fills in `id` and
/// `ts` if not set.
#[derive(Debug, Clone)]
pub struct NewJournalEntry {
    /// Optional id (auto-generated ULID if `None`).
    pub id: Option<Ulid>,
    /// Session id.
    pub session_id: SessionId,
    /// Run id.
    pub run_id: RunId,
    /// Parent run id (subagent only; F01-Phase2).
    pub parent_run_id: Option<Ulid>,
    /// Depth (0 = primary).
    pub depth: u8,
    /// Entry kind.
    pub kind: JournalKind,
    /// Active agent id at the time of the entry.
    pub agent_id: Option<AgentId>,
    /// JSON payload.
    pub payload: serde_json::Value,
    /// Duration in milliseconds (for tool_result / provider_event).
    pub duration_ms: Option<u64>,
}

/// The journal repository. Cheap to clone (internally `Arc`).
#[derive(Clone)]
pub struct JournalRepo {
    inner: Arc<Inner>,
}

struct Inner {
    db: Db,
    max_payload_bytes: usize,
}

impl JournalRepo {
    /// Build a repo with the default 16 KiB payload cap.
    #[must_use]
    pub fn new(db: Db) -> Self {
        Self::with_max_payload(db, DEFAULT_MAX_PAYLOAD_BYTES)
    }

    /// Build a repo with a custom payload cap (used by tests).
    #[must_use]
    pub fn with_max_payload(db: Db, max_payload_bytes: usize) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                max_payload_bytes,
            }),
        }
    }

    /// Append an entry. Idempotent: re-inserting the same `id` is
    /// a no-op (per AC5).
    ///
    /// Returns the persisted entry with `id` and `ts` filled in.
    pub fn append(&self, new: NewJournalEntry) -> AppResult<JournalEntry> {
        // `ULID` does not implement `Default`, so we use a match
        // instead of `unwrap_or_else(Ulid::new)` (which the
        // `unwrap_or_else_default` lint rejects).
        let id = match new.id {
            Some(id) => id,
            None => Ulid::new(),
        };
        let ts = chrono::Utc::now().timestamp_millis();

        // Truncate payload if needed and compute SHA-256 of the
        // original (per AC5).
        let (payload, truncated, sha) =
            truncate_payload(&new.payload, self.inner.max_payload_bytes);

        let payload_str = serde_json::to_string(&payload).map_err(|e| AppError::Internal {
            message: format!("serialize journal payload: {e}"),
        })?;

        self.inner.db.with_conn(|conn| {
            conn.execute(
                "INSERT OR IGNORE INTO journal
                 (id, ts, session_id, run_id, parent_run_id, depth,
                  kind, agent_id, payload, payload_truncated,
                  payload_sha256, duration_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    id.to_string(),
                    ts,
                    new.session_id.to_string(),
                    new.run_id.to_string(),
                    new.parent_run_id.map(|p| p.to_string()),
                    new.depth as i64,
                    new.kind.as_str(),
                    new.agent_id.map(|a| a.to_string()),
                    payload_str,
                    if truncated { 1 } else { 0 },
                    sha,
                    new.duration_ms.map(|d| d as i64),
                ],
            )
            .map_err(map_sqlite_err)?;
            Ok(())
        })?;

        Ok(JournalEntry {
            id,
            ts,
            session_id: new.session_id,
            run_id: new.run_id,
            parent_run_id: new.parent_run_id,
            depth: new.depth,
            kind: new.kind,
            agent_id: new.agent_id,
            payload,
            payload_truncated: truncated,
            payload_sha256: sha,
            duration_ms: new.duration_ms,
        })
    }

    /// Count entries in the journal.
    pub fn count(&self) -> AppResult<u64> {
        self.inner.db.with_conn(|conn| {
            let n: i64 = conn
                .query_row("SELECT COUNT(*) FROM journal", [], |r| r.get(0))
                .map_err(map_sqlite_err)?;
            Ok(n as u64)
        })
    }

    /// Query entries for a session, ordered by `(ts ASC, id ASC)`.
    ///
    /// Per journal.md §Edge case 5: if `kinds` is `Some(&[])`,
    /// returns an empty vector (the caller filtered out every
    /// kind).
    pub fn query_by_session(
        &self,
        session_id: &SessionId,
        since: Option<i64>,
        until: Option<i64>,
        kinds: Option<&[JournalKind]>,
        limit: Option<u32>,
    ) -> AppResult<Vec<JournalEntry>> {
        if let Some(ks) = kinds {
            if ks.is_empty() {
                return Ok(Vec::new());
            }
        }
        self.inner.db.with_conn(|conn| {
            query_session(
                conn,
                self.inner.db.clone(),
                session_id,
                since,
                until,
                kinds,
                limit,
            )
        })
    }

    /// Query all entries for a specific run.
    pub fn query_by_run(&self, run_id: &RunId) -> AppResult<Vec<JournalEntry>> {
        self.inner.db.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, ts, session_id, run_id, parent_run_id, depth,
                            kind, agent_id, payload, payload_truncated,
                            payload_sha256, duration_ms
                     FROM journal
                     WHERE run_id = ?1
                     ORDER BY ts ASC, id ASC",
                )
                .map_err(map_sqlite_err)?;
            let rows = stmt
                .query_map(params![run_id.to_string()], row_to_entry)
                .map_err(map_sqlite_err)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(map_sqlite_err)?);
            }
            Ok(out)
        })
    }
}

// --- helpers ----------------------------------------------------------

fn map_sqlite_err(e: rusqlite::Error) -> AppError {
    AppError::Io {
        op: "sqlite".into(),
        reason: e.to_string(),
    }
}

fn row_to_entry(r: &rusqlite::Row<'_>) -> rusqlite::Result<JournalEntry> {
    let id_str: String = r.get(0)?;
    let id: Ulid = id_str.parse().map_err(|e: ulid::DecodeError| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{e}"),
            )),
        )
    })?;
    let session_str: String = r.get(2)?;
    let session_id: SessionId = session_str.parse().map_err(|e: ulid::DecodeError| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{e}"),
            )),
        )
    })?;
    let run_str: String = r.get(3)?;
    let run_id: RunId = run_str.parse().map_err(|e: ulid::DecodeError| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{e}"),
            )),
        )
    })?;
    let parent_run_id: Option<Ulid> = r
        .get::<_, Option<String>>(4)?
        .map(|s| {
            s.parse().map_err(|e: ulid::DecodeError| {
                rusqlite::Error::FromSqlConversionFailure(
                    4,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("{e}"),
                    )),
                )
            })
        })
        .transpose()?;
    let depth: i64 = r.get(5)?;
    let kind_str: String = r.get(6)?;
    let kind = JournalKind::from_str_opt(&kind_str).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            6,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown kind: {kind_str}"),
            )),
        )
    })?;
    let agent_id: Option<AgentId> = r
        .get::<_, Option<String>>(7)?
        .map(|s| {
            s.parse().map_err(|e: ulid::DecodeError| {
                rusqlite::Error::FromSqlConversionFailure(
                    7,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("{e}"),
                    )),
                )
            })
        })
        .transpose()?;
    let payload_str: String = r.get(8)?;
    let payload: serde_json::Value = serde_json::from_str(&payload_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            8,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{e}"),
            )),
        )
    })?;
    let truncated: i64 = r.get(9)?;
    let sha: Option<String> = r.get(10)?;
    let duration_ms: Option<i64> = r.get(11)?;

    Ok(JournalEntry {
        id,
        ts: r.get(1)?,
        session_id,
        run_id,
        parent_run_id,
        depth: depth as u8,
        kind,
        agent_id,
        payload,
        payload_truncated: truncated != 0,
        payload_sha256: sha,
        duration_ms: duration_ms.map(|d| d as u64),
    })
}

fn query_session(
    conn: &Connection,
    _db: Db,
    session_id: &SessionId,
    since: Option<i64>,
    until: Option<i64>,
    kinds: Option<&[JournalKind]>,
    limit: Option<u32>,
) -> AppResult<Vec<JournalEntry>> {
    let limit = limit.unwrap_or(200).clamp(1, 1000) as i64;

    // Build SQL dynamically. We track the next placeholder number
    // separately from `args.len()` because the placeholders are
    // appended in lockstep with `args.push`.
    let mut sql = String::from(
        "SELECT id, ts, session_id, run_id, parent_run_id, depth,
                kind, agent_id, payload, payload_truncated,
                payload_sha256, duration_ms
         FROM journal
         WHERE session_id = ?1",
    );
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(session_id.to_string())];
    let mut next_n: usize = 2;
    if since.is_some() {
        sql.push_str(&format!(" AND ts >= ?{next_n}"));
        next_n += 1;
    }
    if until.is_some() {
        sql.push_str(&format!(" AND ts <= ?{next_n}"));
        next_n += 1;
    }
    if let Some(ks) = kinds {
        if !ks.is_empty() {
            sql.push_str(" AND kind IN (");
            for (i, _) in ks.iter().enumerate() {
                if i > 0 {
                    sql.push(',');
                }
                sql.push('?');
                sql.push_str(&next_n.to_string());
                next_n += 1;
            }
            sql.push(')');
        }
    }
    sql.push_str(&format!(" ORDER BY ts ASC, id ASC LIMIT ?{next_n}"));

    if let Some(s) = since {
        args.push(Box::new(s));
    }
    if let Some(u) = until {
        args.push(Box::new(u));
    }
    if let Some(ks) = kinds {
        for k in ks {
            args.push(Box::new(k.as_str().to_string()));
        }
    }
    args.push(Box::new(limit));

    let mut stmt = conn.prepare(&sql).map_err(map_sqlite_err)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(args.iter()), row_to_entry)
        .map_err(map_sqlite_err)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(map_sqlite_err)?);
    }
    Ok(out)
}

/// Truncate a JSON payload to `max_bytes`. If truncation happens,
/// compute SHA-256 of the original JSON serialization.
fn truncate_payload(
    payload: &serde_json::Value,
    max_bytes: usize,
) -> (serde_json::Value, bool, Option<String>) {
    let serialized = match serde_json::to_string(payload) {
        Ok(s) => s,
        Err(_) => return (payload.clone(), false, None),
    };
    if serialized.len() <= max_bytes {
        return (payload.clone(), false, None);
    }

    let mut hasher = Sha256::new();
    hasher.update(serialized.as_bytes());
    let digest = hasher.finalize();
    let sha = format!("{digest:x}");

    // Truncate by trimming a JSON-safe prefix. For object/array
    // payloads this is not strictly valid JSON, but consumers
    // should treat the payload as opaque once `truncated` is true.
    let truncated_str: String = serialized
        .chars()
        .take(max_bytes.saturating_sub(32)) // leave room for "...truncated" marker
        .collect();
    let truncated_value: serde_json::Value =
        serde_json::Value::String(format!("{truncated_str}…[truncated]"));

    (truncated_value, true, Some(sha))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fresh_repo() -> (TempDir, JournalRepo, SessionId, RunId) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let db = Db::open(&db_path).unwrap();
        // Insert a placeholder session row so the journal FK
        // (session_id REFERENCES sessions(id)) passes.
        db.with_conn(|conn| -> crate::AppResult<()> {
            conn.execute(
                "INSERT INTO workspaces (id, root_path, name, created_at, last_opened_at)
                 VALUES ('01ARZ3NDEKTSV4RRFFQ69G5FAV', '', NULL, 0, 0)",
                [],
            )
            .map_err(map_sqlite_err)?;
            Ok(())
        })
        .unwrap();
        let sid = SessionId::new();
        db.with_conn(|conn| -> crate::AppResult<()> {
            conn.execute(
                "INSERT INTO sessions
                 (id, workspace_id, parent_id, title, status,
                  created_at, updated_at, last_run_id, last_run_finish_reason,
                  active_agent_id)
                 VALUES (?1, '01ARZ3NDEKTSV4RRFFQ69G5FAV', NULL, NULL, 'idle',
                         0, 0, NULL, NULL, 'build')",
                rusqlite::params![sid.to_string()],
            )
            .map_err(map_sqlite_err)?;
            Ok(())
        })
        .unwrap();
        let repo = JournalRepo::new(db);
        (dir, repo, sid, RunId::new())
    }

    #[test]
    fn append_returns_entry_with_id_and_ts() {
        let (_d, repo, sid, rid) = fresh_repo();
        let entry = repo
            .append(NewJournalEntry {
                id: None,
                session_id: sid,
                run_id: rid,
                parent_run_id: None,
                depth: 0,
                kind: JournalKind::UserMessage,
                agent_id: None,
                payload: serde_json::json!({"text": "hi"}),
                duration_ms: None,
            })
            .unwrap();
        assert!(entry.id.to_string().len() == 26);
        let now = chrono::Utc::now().timestamp_millis();
        assert!((now - entry.ts).abs() < 1000, "ts should be ~now");
    }

    #[test]
    fn append_idempotent_by_id() {
        let (_d, repo, sid, rid) = fresh_repo();
        let id = Ulid::new();
        let new = NewJournalEntry {
            id: Some(id),
            session_id: sid,
            run_id: rid,
            parent_run_id: None,
            depth: 0,
            kind: JournalKind::UserMessage,
            agent_id: None,
            payload: serde_json::json!({"text": "hi"}),
            duration_ms: None,
        };
        repo.append(new.clone()).unwrap();
        repo.append(new).unwrap();
        assert_eq!(repo.count().unwrap(), 1);
    }

    #[test]
    fn append_ordering_matches_ulid() {
        let (_d, repo, sid, rid) = fresh_repo();
        let mut ids = Vec::new();
        for i in 0..3 {
            let e = repo
                .append(NewJournalEntry {
                    id: None,
                    session_id: sid,
                    run_id: rid,
                    parent_run_id: None,
                    depth: 0,
                    kind: JournalKind::UserMessage,
                    agent_id: None,
                    payload: serde_json::json!({"i": i}),
                    duration_ms: None,
                })
                .unwrap();
            ids.push(e.id.to_string());
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "appends should be in ULID order");
    }

    #[test]
    fn oversized_payload_truncated_with_sha256() {
        let (_d, repo, sid, rid) = fresh_repo();
        let big: String = "x".repeat(20_000);
        let entry = repo
            .append(NewJournalEntry {
                id: None,
                session_id: sid,
                run_id: rid,
                parent_run_id: None,
                depth: 0,
                kind: JournalKind::UserMessage,
                agent_id: None,
                payload: serde_json::json!({ "content": big }),
                duration_ms: None,
            })
            .unwrap();
        assert!(entry.payload_truncated);
        assert!(entry.payload_sha256.is_some());
        // The sha is hex of SHA-256 = 64 chars.
        assert_eq!(entry.payload_sha256.as_ref().unwrap().len(), 64);
    }

    #[test]
    fn query_by_session_filters_kinds_and_orders() {
        let (_d, repo, sid, rid) = fresh_repo();
        for i in 0..3 {
            repo.append(NewJournalEntry {
                id: None,
                session_id: sid,
                run_id: rid,
                parent_run_id: None,
                depth: 0,
                kind: if i % 2 == 0 {
                    JournalKind::UserMessage
                } else {
                    JournalKind::AssistantMessage
                },
                agent_id: None,
                payload: serde_json::json!({"i": i}),
                duration_ms: None,
            })
            .unwrap();
        }
        let all = repo
            .query_by_session(&sid, None, None, None, Some(100))
            .unwrap();
        assert_eq!(all.len(), 3);
        let only_user = repo
            .query_by_session(
                &sid,
                None,
                None,
                Some(&[JournalKind::UserMessage]),
                Some(100),
            )
            .unwrap();
        assert_eq!(only_user.len(), 2);
        assert!(only_user.iter().all(|e| e.kind == JournalKind::UserMessage));
    }

    #[test]
    fn query_by_run_returns_only_that_run() {
        let (_d, repo, sid, _rid) = fresh_repo();
        let rid1 = RunId::new();
        let rid2 = RunId::new();
        repo.append(NewJournalEntry {
            id: None,
            session_id: sid,
            run_id: rid1,
            parent_run_id: None,
            depth: 0,
            kind: JournalKind::UserMessage,
            agent_id: None,
            payload: serde_json::json!({"r": 1}),
            duration_ms: None,
        })
        .unwrap();
        repo.append(NewJournalEntry {
            id: None,
            session_id: sid,
            run_id: rid2,
            parent_run_id: None,
            depth: 0,
            kind: JournalKind::UserMessage,
            agent_id: None,
            payload: serde_json::json!({"r": 2}),
            duration_ms: None,
        })
        .unwrap();
        let r1 = repo.query_by_run(&rid1).unwrap();
        let r2 = repo.query_by_run(&rid2).unwrap();
        assert_eq!(r1.len(), 1);
        assert_eq!(r2.len(), 1);
        assert_eq!(r1[0].run_id, rid1);
    }

    #[test]
    fn query_by_session_with_empty_kinds_returns_empty() {
        let (_d, repo, sid, rid) = fresh_repo();
        repo.append(NewJournalEntry {
            id: None,
            session_id: sid,
            run_id: rid,
            parent_run_id: None,
            depth: 0,
            kind: JournalKind::UserMessage,
            agent_id: None,
            payload: serde_json::json!({}),
            duration_ms: None,
        })
        .unwrap();
        let empty = repo
            .query_by_session(&sid, None, None, Some(&[]), Some(100))
            .unwrap();
        assert!(empty.is_empty());
    }
}
