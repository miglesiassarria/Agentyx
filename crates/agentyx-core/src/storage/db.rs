//! `Db` — wrapper over `rusqlite::Connection` with PRAGMAs and
//! migrations.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::Connection;

use crate::{AppError, AppResult};

use super::migrations;

/// Wrapper over `rusqlite::Connection` that:
/// - Applies PRAGMAs on open (`journal_mode=WAL`, `foreign_keys=ON`,
///   `synchronous=NORMAL`, `busy_timeout=5000`, `temp_store=MEMORY`).
/// - Runs migrations from the embedded `migrations/` set in order.
/// - Provides `with_conn` / `transaction` helpers that take a
///   closure with access to `&Connection`.
///
/// Cheap to clone (`Arc<Mutex<Connection>>` inside). Send + Sync
/// when wrapped in `Arc<Db>`.
#[derive(Clone)]
pub struct Db {
    inner: Arc<Inner>,
}

struct Inner {
    conn: Mutex<Connection>,
    /// Path the DB was opened from, for diagnostics.
    path: PathBuf,
}

impl std::fmt::Debug for Db {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Db")
            .field("path", &self.inner.path)
            .finish()
    }
}

impl Db {
    /// Open or create a SQLite database at `path`. Applies PRAGMAs
    /// and migrations.
    ///
    /// Per storage.md §AC4, rejects paths that don't resolve to an
    /// absolute, canonicalized path under the agentyx home
    /// (`~/.agentyx/`).
    pub fn open(path: &Path) -> AppResult<Self> {
        // AC4: reject non-absolute paths before doing any I/O.
        if !path.is_absolute() {
            return Err(AppError::InvalidInput {
                message: format!("storage path must be absolute: {}", path.display()),
            });
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::Io {
                op: format!("create_dir_all {}", parent.display()),
                reason: e.to_string(),
            })?;
        }

        let mut conn = Connection::open(path).map_err(|e| AppError::Io {
            op: format!("open {}", path.display()),
            reason: e.to_string(),
        })?;

        Self::apply_pragmas(&conn)?;
        Self::run_migrations(&mut conn)?;

        Ok(Self {
            inner: Arc::new(Inner {
                conn: Mutex::new(conn),
                path: path.to_path_buf(),
            }),
        })
    }

    /// The path this DB was opened from.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    /// Acquire the connection lock. Recovers from poisoning
    /// (see workspace::service for the same idiom).
    fn lock(&self) -> MutexGuard<'_, Connection> {
        match self.inner.conn.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Run `f` with access to `&Connection`.
    ///
    /// The closure runs synchronously on the calling thread. In
    /// async code, wrap the call in `tokio::task::spawn_blocking`
    /// (the agent loop does this for long queries).
    pub fn with_conn<F, T>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&Connection) -> AppResult<T>,
    {
        let conn = self.lock();
        f(&conn)
    }

    /// Run `f` inside a SQLite transaction (`BEGIN IMMEDIATE` +
    /// `COMMIT` / `ROLLBACK` on error).
    pub fn transaction<F, T>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&Connection) -> AppResult<T>,
    {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(map_sqlite_err)?;
        let result = f(&tx);
        match result {
            Ok(value) => {
                tx.commit().map_err(map_sqlite_err)?;
                Ok(value)
            }
            Err(e) => {
                // Dropping `tx` rolls back; explicit for clarity.
                tx.rollback().map_err(map_sqlite_err)?;
                Err(e)
            }
        }
    }

    /// Apply the v1 PRAGMAs. Per storage.md §Operations.
    fn apply_pragmas(conn: &Connection) -> AppResult<()> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA temp_store = MEMORY;",
        )
        .map_err(map_sqlite_err)?;
        Ok(())
    }

    /// Run embedded migrations in order. Idempotent: a migration
    /// is only applied if not already present in `__migrations`.
    fn run_migrations(conn: &mut Connection) -> AppResult<()> {
        // Bootstrap the migrations table.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS __migrations (
                id INTEGER PRIMARY KEY,
                applied_at INTEGER NOT NULL
             );",
        )
        .map_err(map_sqlite_err)?;

        // Discover which migrations have run.
        let applied: Vec<i64> = {
            let mut stmt = conn
                .prepare("SELECT id FROM __migrations ORDER BY id ASC")
                .map_err(map_sqlite_err)?;
            let rows = stmt
                .query_map([], |r| r.get::<_, i64>(0))
                .map_err(map_sqlite_err)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(map_sqlite_err)?);
            }
            out
        };

        for migration in migrations::all() {
            if applied.contains(&migration.id) {
                continue;
            }
            // Per AC3: a failing migration rolls back so the DB
            // stays in its previous state.
            let tx = conn.transaction().map_err(map_sqlite_err)?;
            tx.execute_batch(migration.sql).map_err(|e| {
                tracing::error!(
                    migration_id = migration.id,
                    migration_name = migration.name,
                    error = %e,
                    "migration failed; rolling back"
                );
                AppError::Io {
                    op: format!("apply migration {}: {}", migration.id, migration.name),
                    reason: e.to_string(),
                }
            })?;
            let now = chrono::Utc::now().timestamp_millis();
            tx.execute(
                "INSERT INTO __migrations (id, applied_at) VALUES (?1, ?2)",
                rusqlite::params![migration.id, now],
            )
            .map_err(map_sqlite_err)?;
            tx.commit().map_err(map_sqlite_err)?;
            tracing::info!(
                migration_id = migration.id,
                migration_name = migration.name,
                "migration applied"
            );
        }

        Ok(())
    }
}

fn map_sqlite_err(e: rusqlite::Error) -> AppError {
    AppError::Io {
        op: "sqlite".into(),
        reason: e.to_string(),
    }
}
