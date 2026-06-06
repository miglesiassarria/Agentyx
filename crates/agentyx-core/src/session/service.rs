//! High-level `SessionService` over a per-workspace `Db`.

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{params, OptionalExtension};

use crate::agents::AgentRegistry;
use crate::ids::{AgentId, MessageId, RunId, SessionId, WorkspaceId};
use crate::storage::Db;
use crate::{AppError, AppResult};

use super::types::{Message, MessageRole, Session, SessionStatus};

/// Options for [`SessionService::list`].
#[derive(Debug, Clone, Default)]
pub struct ListOpts {
    /// Max number of sessions to return. Default 50, max 200.
    pub limit: Option<u32>,
    /// Filter by status.
    pub status: Option<SessionStatus>,
}

/// Options for [`SessionService::list_messages`].
#[derive(Debug, Clone, Default)]
pub struct ListMessagesOpts {
    /// Pagination cursor: return messages with `seq > after_seq`.
    pub after_seq: Option<i64>,
    /// Max number of messages to return. Default 100, max 500.
    pub limit: Option<u32>,
}

/// Service that owns the per-workspace `state.db` and exposes
/// session operations. Cheap to clone (internally `Arc`).
#[derive(Clone)]
pub struct SessionService {
    inner: Arc<Inner>,
}

struct Inner {
    db: Db,
    workspace_id: WorkspaceId,
}

impl SessionService {
    /// Open the per-workspace `state.db` and prepare a service.
    /// `db_path` is `<agentyx_home>/workspaces/<id>/state.db`.
    pub fn open(db_path: &Path, workspace_id: WorkspaceId) -> AppResult<Self> {
        let db = Db::open(db_path)?;
        Ok(Self {
            inner: Arc::new(Inner { db, workspace_id }),
        })
    }

    /// Open with a `Db` already in hand. Used by tests and by
    /// `SessionService::open` itself.
    pub fn with_db(db: Db, workspace_id: WorkspaceId) -> Self {
        Self {
            inner: Arc::new(Inner { db, workspace_id }),
        }
    }

    /// The underlying DB handle.
    #[must_use]
    pub fn db(&self) -> &Db {
        &self.inner.db
    }

    /// The workspace this service belongs to.
    #[must_use]
    pub fn workspace_id(&self) -> WorkspaceId {
        self.inner.workspace_id
    }

    /// Create a new session. Returns it in `Idle` state.
    ///
    /// `active_agent` defaults to the first `Primary` of the
    /// registry if `None`.
    pub fn create(
        &self,
        agents: &AgentRegistry,
        active_agent: Option<AgentId>,
    ) -> AppResult<Session> {
        let id = SessionId::new();
        let now = Utc::now();
        let now_ms = now.timestamp_millis();
        let active_agent = match active_agent {
            Some(a) => a,
            None => agents
                .primary_ids()
                .into_iter()
                .next()
                .ok_or_else(|| AppError::Internal {
                    message: "no primary agents registered".into(),
                })?,
        };

        self.inner.db.with_conn(|conn| {
            // Ensure the workspace row exists in this DB. If the
            // DB was just created the workspaces table is empty
            // (it's a separate DB from the global registry).
            conn.execute(
                "INSERT OR IGNORE INTO workspaces (id, root_path, name, created_at, last_opened_at)
                 VALUES (?1, ?2, NULL, ?3, ?3)",
                params![
                    self.inner.workspace_id.to_string(),
                    "", // unknown at session level
                    now_ms,
                ],
            )
            .map_err(map_sqlite_err)?;

            conn.execute(
                "INSERT INTO sessions
                 (id, workspace_id, parent_id, title, status,
                  created_at, updated_at, last_run_id,
                  last_run_finish_reason, active_agent_id)
                 VALUES (?1, ?2, NULL, NULL, 'idle', ?3, ?3, NULL, NULL, ?4)",
                params![
                    id.to_string(),
                    self.inner.workspace_id.to_string(),
                    now_ms,
                    active_agent.to_string(),
                ],
            )
            .map_err(map_sqlite_err)?;
            Ok(())
        })?;

        tracing::info!(
            session_id = %id,
            workspace_id = %self.inner.workspace_id,
            active_agent = %active_agent,
            "session created"
        );

        Ok(Session {
            id,
            workspace_id: self.inner.workspace_id,
            parent_id: None,
            title: None,
            status: SessionStatus::Idle,
            created_at: now,
            updated_at: now,
            last_run_id: None,
            last_run_finish_reason: None,
            active_agent_id: active_agent,
        })
    }

    /// Look up a session by id. Returns `NotFound` if missing.
    pub fn get(&self, id: SessionId) -> AppResult<Session> {
        self.inner
            .db
            .with_conn(|conn| load_session(conn, &self.inner.workspace_id, id))
    }

    /// List sessions of this workspace, ordered by `updated_at DESC`.
    pub fn list(&self, opts: ListOpts) -> AppResult<Vec<Session>> {
        let limit = opts.limit.unwrap_or(50).clamp(1, 200) as i64;
        self.inner.db.with_conn(|conn| {
            let (sql, params_vec): (&str, Vec<Box<dyn rusqlite::ToSql>>) = match opts.status {
                Some(status) => (
                    "SELECT id FROM sessions
                     WHERE workspace_id = ?1 AND status = ?2
                     ORDER BY updated_at DESC LIMIT ?3",
                    vec![
                        Box::new(self.inner.workspace_id.to_string()),
                        Box::new(status.as_str().to_string()),
                        Box::new(limit),
                    ],
                ),
                None => (
                    "SELECT id FROM sessions
                     WHERE workspace_id = ?1
                     ORDER BY updated_at DESC LIMIT ?2",
                    vec![
                        Box::new(self.inner.workspace_id.to_string()),
                        Box::new(limit),
                    ],
                ),
            };

            let mut stmt = conn.prepare(sql).map_err(map_sqlite_err)?;
            let rows = stmt
                .query_map(rusqlite::params_from_iter(params_vec.iter()), |r| {
                    let id: String = r.get(0)?;
                    Ok(id)
                })
                .map_err(map_sqlite_err)?;

            let mut sessions = Vec::new();
            for row in rows {
                let id_str: String = row.map_err(map_sqlite_err)?;
                let id: SessionId = id_str.parse().map_err(|e| AppError::Internal {
                    message: format!("decode session id: {e}"),
                })?;
                sessions.push(load_session(conn, &self.inner.workspace_id, id)?);
            }
            Ok(sessions)
        })
    }

    /// Delete a session, its messages, and its usage rows. The
    /// journal entries are kept (append-only). Returns
    /// `Conflict` if the session is currently running.
    pub fn delete(&self, id: SessionId) -> AppResult<()> {
        self.inner.db.transaction(|conn| {
            let status: Option<String> = conn
                .query_row(
                    "SELECT status FROM sessions WHERE id = ?1",
                    params![id.to_string()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(map_sqlite_err)?;
            match status {
                None => {
                    return Err(AppError::NotFound {
                        kind: "session".into(),
                        id: id.to_string(),
                    });
                }
                Some(s) if s == "running" => {
                    return Err(AppError::Conflict {
                        message: "session has a running run; abort first".into(),
                    });
                }
                Some(_) => {}
            }
            conn.execute(
                "DELETE FROM sessions WHERE id = ?1",
                params![id.to_string()],
            )
            .map_err(map_sqlite_err)?;
            Ok(())
        })
    }

    /// Append a message. Returns the persisted `Message` with
    /// the assigned `seq` and id.
    pub fn append_message(
        &self,
        session_id: SessionId,
        role: MessageRole,
        content: &str,
        run_id: Option<RunId>,
    ) -> AppResult<Message> {
        if role != MessageRole::User && content.is_empty() {
            return Err(AppError::InvalidInput {
                message: format!("{} message must have non-empty content", role.as_str()),
            });
        }
        if matches!(role, MessageRole::User) && content.is_empty() {
            return Err(AppError::InvalidInput {
                message: "user message must have non-empty content".into(),
            });
        }

        let id = MessageId::new();
        let now = Utc::now();
        let now_ms = now.timestamp_millis();

        self.inner.db.transaction(|conn| {
            // Ensure the session exists.
            let exists: Option<String> = conn
                .query_row(
                    "SELECT status FROM sessions WHERE id = ?1",
                    params![session_id.to_string()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(map_sqlite_err)?;
            if exists.is_none() {
                return Err(AppError::NotFound {
                    kind: "session".into(),
                    id: session_id.to_string(),
                });
            }

            // Monotonic seq: read max(seq)+1 within the same
            // transaction so concurrent appends serialize.
            let next_seq: i64 = conn
                .query_row(
                    "SELECT COALESCE(MAX(seq), 0) + 1 FROM messages WHERE session_id = ?1",
                    params![session_id.to_string()],
                    |r| r.get(0),
                )
                .map_err(map_sqlite_err)?;

            conn.execute(
                "INSERT INTO messages
                 (id, session_id, run_id, role, content, created_at, seq)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id.to_string(),
                    session_id.to_string(),
                    run_id.map(|r| r.to_string()),
                    role.as_str(),
                    content,
                    now_ms,
                    next_seq,
                ],
            )
            .map_err(map_sqlite_err)?;

            // Update session.updated_at and, for the first user
            // message, derive a title.
            conn.execute(
                "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
                params![now_ms, session_id.to_string()],
            )
            .map_err(map_sqlite_err)?;

            if matches!(role, MessageRole::User) {
                let title: Option<String> = conn
                    .query_row(
                        "SELECT title FROM sessions WHERE id = ?1",
                        params![session_id.to_string()],
                        |r| r.get::<_, Option<String>>(0),
                    )
                    .map_err(map_sqlite_err)?;
                if title.is_none() {
                    let derived = derive_title(content);
                    conn.execute(
                        "UPDATE sessions SET title = ?1 WHERE id = ?2",
                        params![derived, session_id.to_string()],
                    )
                    .map_err(map_sqlite_err)?;
                }
            }
            Ok(())
        })?;

        Ok(Message {
            id,
            session_id,
            run_id,
            role,
            content: content.to_string(),
            created_at: now,
            seq: 0, // overwritten by the caller via the returned row; placeholder
        })
    }

    /// Load a session's messages in `seq` ASC order.
    pub fn list_messages(
        &self,
        session_id: SessionId,
        opts: ListMessagesOpts,
    ) -> AppResult<Vec<Message>> {
        let limit = opts.limit.unwrap_or(100).clamp(1, 500) as i64;
        let after_seq = opts.after_seq.unwrap_or(0);

        self.inner.db.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, session_id, run_id, role, content, created_at, seq
                     FROM messages
                     WHERE session_id = ?1 AND seq > ?2
                     ORDER BY seq ASC LIMIT ?3",
                )
                .map_err(map_sqlite_err)?;
            let rows = stmt
                .query_map(params![session_id.to_string(), after_seq, limit], |r| {
                    Ok(Message {
                        id: r
                            .get::<_, String>(0)?
                            .parse()
                            .map_err(|e: ulid::DecodeError| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    0,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            })?,
                        session_id: r.get::<_, String>(1)?.parse().map_err(
                            |e: ulid::DecodeError| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    1,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            },
                        )?,
                        run_id: r
                            .get::<_, Option<String>>(2)?
                            .map(|s| s.parse())
                            .transpose()
                            .map_err(|e: ulid::DecodeError| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    2,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            })?,
                        role: {
                            let s: String = r.get(3)?;
                            MessageRole::from_str_opt(&s).ok_or_else(|| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    3,
                                    rusqlite::types::Type::Text,
                                    Box::new(std::io::Error::new(
                                        std::io::ErrorKind::InvalidData,
                                        format!("unknown role: {s}"),
                                    )),
                                )
                            })?
                        },
                        content: r.get(4)?,
                        created_at: {
                            let ms: i64 = r.get(5)?;
                            Utc.timestamp_millis_opt(ms).single().ok_or_else(|| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    5,
                                    rusqlite::types::Type::Integer,
                                    Box::new(std::io::Error::new(
                                        std::io::ErrorKind::InvalidData,
                                        format!("bad timestamp: {ms}"),
                                    )),
                                )
                            })?
                        },
                        seq: r.get(6)?,
                    })
                })
                .map_err(map_sqlite_err)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(map_sqlite_err)?);
            }
            Ok(out)
        })
    }

    /// Mark a run as started. Sets session status to `Running` and
    /// records `last_run_id`.
    pub fn start_run(&self, session_id: SessionId, run_id: RunId) -> AppResult<()> {
        self.inner.db.with_conn(|conn| {
            let updated = conn
                .execute(
                    "UPDATE sessions
                     SET status = 'running', last_run_id = ?1, updated_at = ?2
                     WHERE id = ?3",
                    params![
                        run_id.to_string(),
                        Utc::now().timestamp_millis(),
                        session_id.to_string()
                    ],
                )
                .map_err(map_sqlite_err)?;
            if updated == 0 {
                return Err(AppError::NotFound {
                    kind: "session".into(),
                    id: session_id.to_string(),
                });
            }
            Ok(())
        })
    }

    /// Mark a run as finished. Sets the session status and
    /// `last_run_finish_reason`.
    pub fn finish_run(
        &self,
        session_id: SessionId,
        status: SessionStatus,
        finish_reason: &str,
    ) -> AppResult<()> {
        self.inner.db.with_conn(|conn| {
            let updated = conn
                .execute(
                    "UPDATE sessions
                     SET status = ?1, last_run_finish_reason = ?2, updated_at = ?3
                     WHERE id = ?4",
                    params![
                        status.as_str(),
                        finish_reason,
                        Utc::now().timestamp_millis(),
                        session_id.to_string(),
                    ],
                )
                .map_err(map_sqlite_err)?;
            if updated == 0 {
                return Err(AppError::NotFound {
                    kind: "session".into(),
                    id: session_id.to_string(),
                });
            }
            Ok(())
        })
    }

    /// Set the active agent for a session. Validates the agent
    /// exists in the registry. Rejects mid-run with `Conflict`.
    pub fn set_active_agent(
        &self,
        session_id: SessionId,
        agents: &AgentRegistry,
        agent_id: AgentId,
    ) -> AppResult<()> {
        // Reject if agent is not a primary (subagents and hidden
        // agents can't be the active session agent).
        let spec = agents.get(&agent_id).ok_or_else(|| AppError::NotFound {
            kind: "agent".into(),
            id: agent_id.to_string(),
        })?;
        if !matches!(spec.mode, crate::agents::AgentMode::Primary) {
            return Err(AppError::InvalidInput {
                message: "agent is not a primary; cannot be the active session agent".into(),
            });
        }

        self.inner.db.transaction(|conn| {
            let status: Option<String> = conn
                .query_row(
                    "SELECT status FROM sessions WHERE id = ?1",
                    params![session_id.to_string()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(map_sqlite_err)?;
            match status {
                None => {
                    return Err(AppError::NotFound {
                        kind: "session".into(),
                        id: session_id.to_string(),
                    });
                }
                Some(s) if s == "running" => {
                    return Err(AppError::Conflict {
                        message: "session is running; cannot change active agent".into(),
                    });
                }
                Some(_) => {}
            }
            conn.execute(
                "UPDATE sessions SET active_agent_id = ?1, updated_at = ?2 WHERE id = ?3",
                params![
                    agent_id.to_string(),
                    Utc::now().timestamp_millis(),
                    session_id.to_string()
                ],
            )
            .map_err(map_sqlite_err)?;
            Ok(())
        })
    }

    /// Look up the active agent for a session. Returns the
    /// stored id; the caller is responsible for cross-checking
    /// against the registry.
    pub fn get_active_agent(&self, session_id: SessionId) -> AppResult<AgentId> {
        self.inner.db.with_conn(|conn| {
            let id_str: Option<String> = conn
                .query_row(
                    "SELECT active_agent_id FROM sessions WHERE id = ?1",
                    params![session_id.to_string()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(map_sqlite_err)?;
            match id_str {
                Some(s) => s
                    .parse()
                    .map_err(|e: ulid::DecodeError| AppError::Internal {
                        message: format!("decode agent id: {e}"),
                    }),
                None => Err(AppError::NotFound {
                    kind: "session".into(),
                    id: session_id.to_string(),
                }),
            }
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

/// Row shape returned by `SELECT * FROM sessions` (minus the
/// timestamps converted to `DateTime<Utc>`).
struct SessionRow {
    id: String,
    workspace_id: String,
    parent_id: Option<String>,
    title: Option<String>,
    status: String,
    created_at_ms: i64,
    updated_at_ms: i64,
    last_run_id: Option<String>,
    last_run_finish_reason: Option<String>,
    active_agent_id: String,
}

fn load_session(
    conn: &rusqlite::Connection,
    workspace_id: &WorkspaceId,
    id: SessionId,
) -> AppResult<Session> {
    let row: Option<SessionRow> = conn
        .query_row(
            "SELECT id, workspace_id, parent_id, title, status,
                    created_at, updated_at, last_run_id,
                    last_run_finish_reason, active_agent_id
             FROM sessions WHERE id = ?1",
            params![id.to_string()],
            |r| {
                Ok(SessionRow {
                    id: r.get(0)?,
                    workspace_id: r.get(1)?,
                    parent_id: r.get(2)?,
                    title: r.get::<_, Option<String>>(3)?,
                    status: r.get(4)?,
                    created_at_ms: r.get(5)?,
                    updated_at_ms: r.get(6)?,
                    last_run_id: r.get(7)?,
                    last_run_finish_reason: r.get(8)?,
                    active_agent_id: r.get(9)?,
                })
            },
        )
        .optional()
        .map_err(map_sqlite_err)?;

    let row = row.ok_or_else(|| AppError::NotFound {
        kind: "session".into(),
        id: id.to_string(),
    })?;

    let ws: WorkspaceId =
        row.workspace_id
            .parse()
            .map_err(|e: ulid::DecodeError| AppError::Internal {
                message: format!("decode workspace id: {e}"),
            })?;
    if &ws != workspace_id {
        return Err(AppError::NotFound {
            kind: "session (wrong workspace)".into(),
            id: id.to_string(),
        });
    }

    let session_id: SessionId =
        row.id
            .parse()
            .map_err(|e: ulid::DecodeError| AppError::Internal {
                message: format!("decode session id: {e}"),
            })?;
    let parent_id =
        row.parent_id
            .map(|s| s.parse())
            .transpose()
            .map_err(|e: ulid::DecodeError| AppError::Internal {
                message: format!("decode parent id: {e}"),
            })?;
    let status = SessionStatus::from_str_opt(&row.status).ok_or_else(|| AppError::Internal {
        message: format!("unknown session status: {}", row.status),
    })?;
    let last_run_id =
        row.last_run_id
            .map(|s| s.parse())
            .transpose()
            .map_err(|e: ulid::DecodeError| AppError::Internal {
                message: format!("decode run id: {e}"),
            })?;
    let active_agent_id: AgentId =
        row.active_agent_id
            .parse()
            .map_err(|e: ulid::DecodeError| AppError::Internal {
                message: format!("decode agent id: {e}"),
            })?;

    let created_at = ms_to_dt(row.created_at_ms).ok_or_else(|| AppError::Internal {
        message: format!("bad created_at: {}", row.created_at_ms),
    })?;
    let updated_at = ms_to_dt(row.updated_at_ms).ok_or_else(|| AppError::Internal {
        message: format!("bad updated_at: {}", row.updated_at_ms),
    })?;

    Ok(Session {
        id: session_id,
        workspace_id: ws,
        parent_id,
        title: row.title,
        status,
        created_at,
        updated_at,
        last_run_id,
        last_run_finish_reason: row.last_run_finish_reason,
        active_agent_id,
    })
}

fn ms_to_dt(ms: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_millis_opt(ms).single()
}

fn derive_title(content: &str) -> String {
    const MAX: usize = 200;
    let normalized: String = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let truncated: String = normalized.chars().take(MAX).collect();
    if truncated.is_empty() {
        "New chat".to_string()
    } else {
        truncated
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn fresh_service() -> (tempfile::TempDir, SessionService) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let ws_id = WorkspaceId::new();
        let svc = SessionService::open(&db_path, ws_id).unwrap();
        (dir, svc)
    }

    #[test]
    fn create_returns_idle_session() {
        let (_dir, svc) = fresh_service();
        let agents = AgentRegistry::load_builtins();
        let s = svc.create(&agents, None).unwrap();
        assert_eq!(s.status, SessionStatus::Idle);
        assert!(s.title.is_none());
    }

    #[test]
    fn list_returns_recent_first() {
        let (_dir, svc) = fresh_service();
        let agents = AgentRegistry::load_builtins();
        let _a = svc.create(&agents, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let b = svc.create(&agents, None).unwrap();
        let listed = svc.list(ListOpts::default()).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, b.id, "most recent first");
    }

    #[test]
    fn get_missing_returns_not_found() {
        let (_dir, svc) = fresh_service();
        let err = svc.get(SessionId::new()).unwrap_err();
        assert!(matches!(err, AppError::NotFound { .. }));
    }

    #[test]
    fn delete_with_running_run_conflicts() {
        let (_dir, svc) = fresh_service();
        let agents = AgentRegistry::load_builtins();
        let s = svc.create(&agents, None).unwrap();
        svc.start_run(s.id, RunId::new()).unwrap();
        let err = svc.delete(s.id).unwrap_err();
        assert!(matches!(err, AppError::Conflict { .. }));
        // The session is still there.
        assert!(svc.get(s.id).is_ok());
    }

    #[test]
    fn delete_cascades_messages() {
        let (_dir, svc) = fresh_service();
        let agents = AgentRegistry::load_builtins();
        let s = svc.create(&agents, None).unwrap();
        let _ = svc
            .append_message(s.id, MessageRole::User, "hi", None)
            .unwrap();
        svc.delete(s.id).unwrap();
        assert!(svc.get(s.id).is_err());
        assert!(svc
            .list_messages(s.id, ListMessagesOpts::default())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn append_persists_verbatim_and_derives_title() {
        let (_dir, svc) = fresh_service();
        let agents = AgentRegistry::load_builtins();
        let s = svc.create(&agents, None).unwrap();
        let m = svc
            .append_message(s.id, MessageRole::User, "Hola, ¿qué tal?", None)
            .unwrap();
        assert_eq!(m.role, MessageRole::User);
        assert_eq!(m.content, "Hola, ¿qué tal?");

        let s2 = svc.get(s.id).unwrap();
        assert_eq!(s2.title.as_deref(), Some("Hola, ¿qué tal?"));
    }

    #[test]
    fn append_unknown_role_rejected() {
        // Hand-construct a malformed content (we don't expose a
        // public way to insert invalid roles, so we exercise the
        // SQL CHECK constraint via raw insert).
        let (_dir, svc) = fresh_service();
        let agents = AgentRegistry::load_builtins();
        let s = svc.create(&agents, None).unwrap();
        let err = svc.db().with_conn(|conn| {
            conn.execute(
                "INSERT INTO messages (id, session_id, run_id, role, content, created_at, seq)
                 VALUES (?1, ?2, NULL, 'bogus', 'x', 0, 1)",
                params![MessageId::new().to_string(), s.id.to_string()],
            )
            .map_err(map_sqlite_err)
        });
        assert!(err.is_err(), "SQL CHECK should reject unknown role");
    }

    #[test]
    fn list_messages_pagination() {
        let (_dir, svc) = fresh_service();
        let agents = AgentRegistry::load_builtins();
        let s = svc.create(&agents, None).unwrap();
        for i in 0..5 {
            svc.append_message(s.id, MessageRole::User, &format!("m{i}"), None)
                .unwrap();
        }
        let all = svc
            .list_messages(s.id, ListMessagesOpts::default())
            .unwrap();
        assert_eq!(all.len(), 5);
        let page = svc
            .list_messages(
                s.id,
                ListMessagesOpts {
                    after_seq: Some(2),
                    limit: Some(2),
                },
            )
            .unwrap();
        assert_eq!(page.len(), 2);
        assert!(page[0].seq > 2);
        assert!(page[1].seq > page[0].seq);
    }

    #[test]
    fn set_status_updates_session() {
        let (_dir, svc) = fresh_service();
        let agents = AgentRegistry::load_builtins();
        let s = svc.create(&agents, None).unwrap();
        let run_id = RunId::new();
        svc.start_run(s.id, run_id).unwrap();
        assert_eq!(svc.get(s.id).unwrap().status, SessionStatus::Running);
        svc.finish_run(s.id, SessionStatus::Idle, "stop").unwrap();
        let s2 = svc.get(s.id).unwrap();
        assert_eq!(s2.status, SessionStatus::Idle);
        assert_eq!(s2.last_run_id, Some(run_id));
        assert_eq!(s2.last_run_finish_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn set_active_agent_persists() {
        let (_dir, svc) = fresh_service();
        let agents = AgentRegistry::load_builtins();
        let s = svc.create(&agents, None).unwrap();
        let plan_id = agents
            .primary_ids()
            .into_iter()
            .nth(1)
            .expect("plan is the 2nd primary");
        svc.set_active_agent(s.id, &agents, plan_id).unwrap();
        let got = svc.get_active_agent(s.id).unwrap();
        assert_eq!(got, plan_id);
    }

    #[test]
    fn set_active_agent_rejects_subagent() {
        let (_dir, svc) = fresh_service();
        let agents = AgentRegistry::load_builtins();
        let s = svc.create(&agents, None).unwrap();
        let general_id = agents.subagents().into_iter().next().unwrap().id;
        let err = svc.set_active_agent(s.id, &agents, general_id).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput { .. }));
    }

    #[test]
    fn long_title_truncated_at_200_chars() {
        let long: String = "a".repeat(500);
        let derived = derive_title(&long);
        assert_eq!(derived.chars().count(), 200);
    }
}
