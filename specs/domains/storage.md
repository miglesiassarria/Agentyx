# Storage

**Status**: draft
**Owner**: @miglesias
**Last update**: 2026-06-04
**Affects**: — (todos los demás dominios leen/escriben vía este wrapper).
**Required by**: `session.md`, `workspace.md`, `agent-loop.md`,
`tools.md`, `permissions.md` (todos consumen `Db`).

> Wrapper sobre `rusqlite` (bundled). Define el schema SQLite, las
> migraciones, el journal append-only, y la API de acceso a datos
> usada por el resto del core. **No** es un ORM: ofrece una conexión
> tipada y un set de operaciones CRUD por tabla, no más.
>
> Decisión de stack: ver [ADR-0006](../adr/0006-sqlite-rusqlite.md).

## Goal

Proveer persistencia local fiable, testeable y multiplataforma
**sin servidor SQL externo**, con:
- Schema versionado y migrado de forma automática al abrir.
- Acceso seguro (sin SQL injection) vía prepared statements.
- Garantías razonables de concurrencia (WAL, transacciones
  serializables donde se pidan).
- Journal append-only que **no se borra nunca** (excepto por
  rotación manual).

## Non-goals

- ❌ ORM con queries tipadas. Las queries se escriben como strings en
  repos; la seguridad viene de `?` placeholders + `prepare`.
- ❌ Pool de conexiones. v1: una conexión por `Db`, accesible vía
  `Mutex<Connection>` o `RwLock<Connection>`.
- ❌ Replicación, backup, sync. v1 es local-only.
- ❌ Sharding / partitioning. Una sola DB por workspace, una sola
  DB global aparte.
- ❌ Encriptación en reposo. Si se necesita, v2 (con SQLCipher o
  keychain-backed key).
- ❌ Triggers SQLite complejos. Mantener la lógica en Rust.
- ❌ Buscar el menor tamaño en disco. WAL añade algo, pero es el
  precio por concurrencia.

## Glossary

Términos locales:

- **Migration**: archivo `.sql` versionado en
  `crates/agentyx-core/src/storage/migrations/NNNN_<slug>.sql`,
  aplicado una vez por `Db::open` en orden lexicográfico.
- **Db**: wrapper sobre `rusqlite::Connection` con PRAGMAs aplicados,
  migraciones corridas, y métodos `prepare`/`execute`/`query`.
- **Repo**: módulo Rust que encapsula queries de una tabla. Ej:
  `sessions::Repo`, `messages::Repo`, `journal::Repo`. Vive en
  `crates/agentyx-core/src/storage/<table>.rs`.
- **Stats DB**: una segunda SQLite en `~/.agentyx/stats.db` para
  métricas locales que **no** se quieren en la DB del workspace
  (ver §State).

## State

### Archivos físicos

| Archivo | Quién lo abre | Tamaño esperado |
|---|---|---|
| `~/.agentyx/state.json` | app | < 1 KB. Server URL, settings globales. |
| `~/.agentyx/workspaces/<id>/config.toml` | `workspace` | < 4 KB. |
| `~/.agentyx/workspaces/<id>/state.db` | `Db::open(workspace_path)` | MB a cientos de MB. |
| `~/.agentyx/cache/<workspace-hash>/` | `workspace` (índices) | MB. |
| `~/.agentyx/stats.db` | `Db::open_stats` | MB. |
| `~/.agentyx/journal.jsonl` (opcional) | app, rotación | MB. |

### Tablas — `~/.agentyx/workspaces/<id>/state.db`

Las definiciones formales viven en
`crates/agentyx-core/src/storage/migrations/0001_initial.sql` y
siguientes. Aquí se documenta el shape lógico.

#### `__migrations`

```sql
CREATE TABLE __migrations (
  id          INTEGER PRIMARY KEY,
  applied_at  INTEGER NOT NULL  -- ms epoch
);
```

#### `workspaces`

> Aunque cada workspace tiene su propia `state.db`, mantenemos una
> tabla `workspaces` en **state.db** para recordatorios locales
> (nombre amigable, último acceso, etc.). La fuente de verdad de
> qué workspaces existen vive en `~/.agentyx/workspaces/<id>/` (ver
> `workspace.md`).

```sql
CREATE TABLE workspaces (
  id              TEXT PRIMARY KEY,        -- ULID
  root_path       TEXT NOT NULL UNIQUE,    -- canonicalizado
  name            TEXT NULL,               -- derivado del path si NULL
  created_at      INTEGER NOT NULL,
  last_opened_at  INTEGER NOT NULL
);
CREATE INDEX idx_workspaces_last_opened ON workspaces(last_opened_at DESC);
```

#### `sessions`

(Ver [`session.md`](./session.md) para el shape completo.)

```sql
CREATE TABLE sessions (
  id                       TEXT PRIMARY KEY,     -- ULID
  workspace_id             TEXT NOT NULL,        -- FK a workspaces.id
  parent_id                TEXT NULL,
  title                    TEXT NULL,
  status                   TEXT NOT NULL,        -- idle|running|aborted|errored
  created_at               INTEGER NOT NULL,
  updated_at               INTEGER NOT NULL,
  last_run_id              TEXT NULL,
  last_run_finish_reason   TEXT NULL,
  FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);
CREATE INDEX idx_sessions_ws_updated ON sessions(workspace_id, updated_at DESC);
CREATE INDEX idx_sessions_ws_status  ON sessions(workspace_id, status);
```

#### `messages`

(Ver [`session.md`](./session.md).)

```sql
CREATE TABLE messages (
  id          TEXT PRIMARY KEY,
  session_id  TEXT NOT NULL,
  run_id      TEXT NULL,
  role        TEXT NOT NULL,        -- user|assistant|system|tool_result
  content     TEXT NOT NULL,        -- JSON si assistant con tool_calls (ver session.md#edge4)
  created_at  INTEGER NOT NULL,
  seq         INTEGER NOT NULL,     -- monotónico por session
  FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);
CREATE INDEX idx_messages_session_seq ON messages(session_id, seq);
CREATE INDEX idx_messages_run         ON messages(run_id);
```

#### `usage`

(Ver [`session.md`](./session.md).)

```sql
CREATE TABLE usage (
  id                 INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id         TEXT NOT NULL,
  run_id             TEXT NOT NULL,
  model_id           TEXT NOT NULL,    -- "provider:model"
  prompt_tokens      INTEGER NOT NULL,
  completion_tokens  INTEGER NOT NULL,
  ts                 INTEGER NOT NULL,
  FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);
CREATE INDEX idx_usage_session_ts ON usage(session_id, ts DESC);
```

#### `journal`

Append-only. **Sin** `ON DELETE CASCADE` desde session o workspace.
La rotación es manual (ver §Edge case 4).

```sql
CREATE TABLE journal (
  id                    TEXT PRIMARY KEY,        -- ULID
  run_id                TEXT NULL,
  session_id            TEXT NULL,
  workspace_id          TEXT NOT NULL,           -- siempre presente, aunque session NULL
  ts                    INTEGER NOT NULL,
  kind                  TEXT NOT NULL,           -- run.started|run.step|run.aborted|tool.invoked|tool.completed|…
  payload               TEXT NOT NULL,           -- JSON; ver §Edge case 5 (sin secretos)
  duration_ms           INTEGER NULL,
  permission_decision   TEXT NULL                -- allow|ask|deny
);
CREATE INDEX idx_journal_workspace_ts ON journal(workspace_id, ts DESC);
CREATE INDEX idx_journal_run          ON journal(run_id);
CREATE INDEX idx_journal_session_ts   ON journal(session_id, ts DESC);
```

### Tablas — `~/.agentyx/stats.db`

DB separada para métricas que no contaminan la DB del workspace y
que pueden rotarse independientemente.

```sql
CREATE TABLE token_usage_daily (
  day              TEXT PRIMARY KEY,    -- YYYY-MM-DD
  provider         TEXT NOT NULL,
  model            TEXT NOT NULL,
  prompt_tokens    INTEGER NOT NULL,
  completion_tokens INTEGER NOT NULL
);

CREATE TABLE tool_latency (
  id                INTEGER PRIMARY KEY AUTOINCREMENT,
  ts                INTEGER NOT NULL,
  tool              TEXT NOT NULL,
  duration_ms       INTEGER NOT NULL,
  permission        TEXT NOT NULL,      -- allow|ask|deny
  exit_kind         TEXT NOT NULL       -- success|error|denied
);
```

## Operations

### `Db::open(path: &Path) -> Result<Db, AppError>`

Abre o crea la DB en `path`. Aplica PRAGMAs (`journal_mode=WAL`,
`foreign_keys=ON`, `synchronous=NORMAL`, `busy_timeout=5000`,
`temp_store=MEMORY`). Corre migraciones pendientes.

**Errores**:
- `internal` (no se pudo crear el directorio padre, etc.).
- `internal` (migration falló — ver §Edge case 2).

### `Db::open_stats() -> Result<Db, AppError>`

Equivalente para `~/.agentyx/stats.db`. Migraciones separadas
(`migrations_stats/`).

### `Db::with_conn<F, T>(&self, f: F) -> Result<T, AppError>`

Punto de entrada para operaciones que necesitan acceso directo a
`&Connection`. **`f` se ejecuta dentro de un `spawn_blocking`** para
no bloquear el runtime async.

```rust
db.with_conn(|conn| {
    let mut stmt = conn.prepare("SELECT id, status FROM sessions WHERE id = ?")?;
    let row = stmt.query_row([session_id], |r| {
        Ok(SessionRow { id: r.get(0)?, status: r.get(1)? })
    })?;
    Ok(row)
})
```

### `Db::transaction<F, T>(&self, f: F) -> Result<T, AppError>`

Como `with_conn`, pero envuelve en `BEGIN IMMEDIATE` + `COMMIT` /
`ROLLBACK` (vía `conn.transaction()` de `rusqlite`).

### `journal::append(db, entry: JournalEntry) -> Result<(), AppError>`

Inserta una entrada en `journal`. Garantiza:
- `id` único (ULID generado si no se pasa).
- Inserción idempotente por `id` (re-insertar el mismo `id` no hace
  nada, `INSERT OR IGNORE`).
- `payload` validado: max 64 KB; > 64 KB se rechaza con
  `invalid_input`.

### `journal::query(db, opts) -> Result<Vec<JournalEntry>, AppError>`

Lista entradas con filtros (workspace, run, session, kind, rango ts).
Paginación por `after_id` (orden por `id` ULID, lexicográficamente
monotónico).

### `stats::record_token_usage(db_stats, row) -> Result<(), AppError>`

Upsert en `token_usage_daily`.

### `stats::record_tool_latency(db_stats, row) -> Result<(), AppError>`

Insert en `tool_latency`.

## Contracts

Este dominio **no expone Tauri commands ni HTTP endpoints propios**.
Es una capa interna que el resto del core consume. Sus únicos
"clientes" son otros dominios y los Tauri commands definidos en
ellos.

## Edge cases

1. **DB bloqueada por otro proceso** (segundo Agentyx intentando abrir
   la misma DB): `busy_timeout=5000` reintenta 5 s; si no se libera,
   `AppError::Internal` con detalle. Documentamos que abrir la misma
   `state.db` desde dos procesos es **no soportado** en v1.
2. **Migración falla a mitad**: la DB queda en estado inconsistente.
   `Db::open` envuelve la migración en una transacción; si falla,
   hace `ROLLBACK` y la DB queda como estaba. **No** se avanza
   `__migrations`. El siguiente arranque reintenta.
3. **Schema drift manual** (el usuario abre la DB con `sqlite3` CLI
   y borra una tabla): siguiente `Db::open` falla al aplicar la
   migración. Se loguea `tracing::error!` con SQL de la migración
   que falló. Out-of-band fix; no se auto-repara.
4. **Journal crece sin parar**: el usuario debe poder hacer
   `journal::archive(db, before_ts) -> Result<PathBuf, AppError>`
   que copia entradas antiguas a `journal-<date>.jsonl` y las borra
   de la tabla. **Operación manual**, no automática. En v1 se
   expone como Tauri command; v2 puede ser automática con retención
   configurable.
5. **Payload de journal con secretos** (ej: contenido de un archivo
   leído por `read_file`): **PROHIBIDO**. `JournalEntry::new` valida
   que `payload` no contenga patrones obvios (`sk-…`, `Bearer …`,
   `-----BEGIN … PRIVATE KEY-----`). Es una **red de seguridad**,
   no la única defensa. Los call sites tienen la responsabilidad
   de redactar antes de pasar.
6. **Conexión abierta tras pánico en `f` (dentro de
   `with_conn`/`transaction`)**: el `Drop` de `Connection` cierra
   la DB. El journal puede haber quedado a medias. La **próxima**
   migración o query puede fallar con `internal`. Recomendación:
   nunca panic en `f`; usar `?` siempre.
7. **Estadísticas con muchos datos** (`stats.db` > 100 MB): job
   futuro de agregación a tablas diarias/mensuales. En v1 no se
   rota; si el usuario tiene años de uso, asumimos que cabe.
8. **Path traversal al construir el `state.db` path**: bloqueado
   por `Db::open` que **solo acepta paths absolutos canonicalizados**
   dentro de `~/.agentyx/workspaces/<workspace_id>/`. Si el path
   no cumple, `AppError::InvalidInput` (ver §AC4).
9. **WAL files (`-wal`, `-shm`) huérfanos** tras crash: SQLite los
   reconcilia al siguiente `open`. No requiere acción.

## Acceptance criteria

Cada AC → test con nombre derivado `ac<n>_<short>`.

- [ ] AC1: `Db::open` sobre un path nuevo crea el archivo, aplica
  todas las migraciones, y deja `__migrations` con todas las filas.
  **Test**: `ac1_open_creates_and_migrates`.
- [ ] AC2: `Db::open` sobre una DB ya migrada no reaplica las
  migraciones (idempotente). **Test**: `ac2_open_idempotent`.
- [ ] AC3: `Db::open` con migración inválida devuelve `internal`
  con detalle y la DB queda intacta (rollback). **Test**:
  `ac3_failed_migration_rolls_back`.
- [ ] AC4: `Db::open` rechaza paths no absolutos o que escapan de
  `~/.agentyx/workspaces/`. **Test**: `ac4_path_traversal_rejected`.
- [ ] AC5: `journal::append` con el mismo `id` dos veces produce
  una sola fila (idempotente). **Test**: `ac5_journal_idempotent`.
- [ ] AC6: `journal::append` con `payload` > 64 KB devuelve
  `invalid_input`. **Test**: `ac6_journal_payload_size_limit`.
- [ ] AC7: `journal::append` con `payload` que matchea patrones de
  secreto devuelve `invalid_input` con `code: "secret_in_payload"`.
  **Test**: `ac7_journal_rejects_secrets`.
- [ ] AC8: dos `Db::open` concurrentes en el mismo path: el segundo
  espera hasta 5 s (`busy_timeout`) y luego reintenta; si tras
  timeout no puede, devuelve `internal` con código
  `database_busy`. **Test**: `ac8_concurrent_open_handles_lock`.
- [ ] AC9: `journal::query` con `after_id` pagina y no duplica
  entradas. **Test**: `ac9_journal_pagination`.
- [ ] AC10: `Db::transaction` con `f` que devuelve `Err` hace
  `ROLLBACK` y los cambios no son visibles. **Test**:
  `ac10_transaction_rollback_on_error`.
- [ ] AC11: `Db::transaction` con `f` que retorna `Ok` hace `COMMIT`
  y los cambios son visibles en otro `Db::open`. **Test**:
  `ac11_transaction_commit_visible`.
- [ ] AC12: PRAGMAs aplicados son `journal_mode=WAL` y
  `foreign_keys=ON`. **Test**: `ac12_pragmas_applied`.
- [ ] AC13: `stats::record_token_usage` hace upsert: dos llamadas
  con el mismo `(day, provider, model)` suman los tokens, no
  duplican filas. **Test**: `ac13_stats_upsert_accumulates`.
- [ ] AC14: `journal::archive` mueve entradas con `ts < before_ts` a
  un archivo `journal-<timestamp>.jsonl` y las borra de la tabla.
  **Test**: `ac14_journal_archive_moves_entries`.

## Discovered bugs (post-approval)

| ID | Date | Category | Resolved in | Notes |
|---|---|---|---|---|
| _ninguno aún_ | | | | |

## Open questions

- **Q1**: ¿La `Db::with_conn` debería ser `async` directamente, o
  seguimos con `spawn_blocking`? → **Propuesta v1**: `spawn_blocking`
  dentro de `with_conn`; el call site no nota la diferencia. Tests
  unitarios usan una `Db` síncrona para velocidad.
- **Q2**: ¿Soporte de queries vectoriales (sqlite-vec) para RAG? →
  **Propuesta v1**: no. sqlite-vec es opcional; si una feature lo
  pide, se añade como migration + crate opcional.
- **Q3**: ¿`journal.jsonl` en paralelo a la tabla `journal`? →
  **Propuesta v1**: no. Una sola fuente de verdad. El user puede
  exportar vía `journal::archive` si lo quiere fuera.

## References

- [`../adr/0006-sqlite-rusqlite.md`](../adr/0006-sqlite-rusqlite.md) — decisión de stack.
- [`../architecture.md`](../architecture.md) — capa de storage.
- [`session.md`](./session.md) — tablas `sessions`, `messages`, `usage`.
- [`workspace.md`](./workspace.md) — tabla `workspaces` y path del `state.db`.
- [`agent-loop.md`](./agent-loop.md) — quién escribe en `journal`.
- Web: <https://docs.rs/rusqlite>.
