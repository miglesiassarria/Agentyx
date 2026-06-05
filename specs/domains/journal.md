# Journal

**Status**: draft
**Owner**: @miglesias
**Last update**: 2026-06-05
**Affects**: `agent-loop`, `tools`, `permissions`, `agents`, `storage`,
[`F01`](./features/F01-chat-streaming.md), [`F04`](./features/F04-file-diffs.md).
**Required by**: `agent-loop` (escribe cada acción del loop), `tools`
(reporta `tool_call` / `tool_result`), `permissions` (registra
`permission_decision`), `agents` (registra `subagent_lifecycle`), F01
(muestra journal en UI y persiste `user_message` / `assistant_message`),
F04 (referencia para diff proposals aplicadas/rechazadas).

> Log **append-only** en **SQLite puro** (tabla `journal` dentro de
> `state.db` del workspace). Decisión de stack fijada en sesión de
> planning: NO se usa archivo `journal.jsonl` paralelo. La forma
> "append-only" se garantiza por convención + tests de invariantes:
> el `JournalRepo` no expone operaciones de update/delete.
>
> El journal es la **única fuente de verdad** post-mortem: permite
> replay de una sesión, debug de bugs, y auditoría de qué hizo el
> agente (ver AGENTS.md §6.3). Los eventos streaming `chat.*.v1`
> que el frontend consume **en vivo** son ortogonales: el agent
> loop los emite y, en el mismo punto, hace `journal.append`. El
> frontend no lee del journal para mostrar la sesión en vivo (lo
> hace de los eventos); lee del journal solo en cold start (cargar
> historial) o en vistas de debug.

## Goal

Persistir de forma duradera, ordenada y queryable cada acción
significativa del agente durante una sesión, con las propiedades de:

1. **Append-only**: nunca se modifica ni borra un `JournalEntry`.
2. **Orden estable**: por `(ts, id)` (ULID es monótono por tiempo).
3. **Reproducible**: dado un `run_id`, se puede reconstruir el flujo
   completo del run (tool calls, resultados, decisiones de permiso,
   subagent spawns).
4. **Compactable**: entradas muy antiguas se mueven a `journal_archive`
   para mantener `journal` < N filas (default 100k), sin perder
   capacidad de query histórica.
5. **No bloquea el loop**: un fallo al escribir en el journal es
   logueado y surfaced al usuario, pero no aborta la sesión (la
   acción del agente ya ocurrió y no se puede deshacer).

## Non-goals

- ❌ Replicación, sync entre devices, ni WAL streaming. Es local
  por workspace.
- ❌ Búsqueda full-text dentro de payloads del journal (se hace en
  `tools.search`, dominio aparte).
- ❌ Export a formato externo (CSV, JSON) en v1. v1.x: export a
  `journal-export.jsonl` para sharing.
- ❌ Cifrado at-rest (la DB está en disco del usuario; cifrado es
  responsabilidad del FS del SO).
- ❌ Tail en vivo desde la UI leyendo del journal. Eso se hace con
  los eventos `chat.*.v1`; el journal se consulta bajo demanda.

## Glossary

Términos locales (los globales están en [`../glossary.md`](../glossary.md)):

- **JournalEntry**: fila inmutable de la tabla `journal`.
- **Run**: una ejecución completa del `AgentLoop` (desde `session.send`
  hasta `finish_reason` ∈ {`stop`, `error`, `aborted`, `length`}).
  Identificado por `run_id` (ULID). Una sesión puede tener N runs.
- **Run tree**: jerarquía de runs donde un subagent invocado desde un
  primary tiene `parent_run_id` apuntando al run del primary. La
  profundidad máxima es 1 en v1 (ver `agents.md` §Edge 3).
- **Archive**: tabla `journal_archive` con la misma forma que
  `journal`; recibe entradas movidas por `archive_older_than`.

## State

Persiste en SQLite, tabla `journal` dentro de `state.db` del workspace
(ver `storage.md` §State).

| Dato | Ubicación | Quién lee | Quién escribe |
|---|---|---|---|
| `JournalEntry` rows | `state.db::journal` | `agent-loop` (replay, debug), F01 (cold start history), UI de debug | `JournalRepo::append` desde `agent-loop`, `tools`, `permissions`, `agents` |
| Archived entries | `state.db::journal_archive` | UI de debug, export v1.x | `JournalRepo::archive_older_than` (al arranque del workspace y al alcanzar threshold) |
| Threshold de archivado | `workspace.config.toml::journal.max_rows` (default 100_000) | `JournalRepo` | usuario (settings de workspace) |

### Schema

```sql
CREATE TABLE journal (
  id            TEXT PRIMARY KEY,        -- ULID
  ts            INTEGER NOT NULL,        -- epoch milliseconds
  session_id    TEXT NOT NULL,
  run_id        TEXT NOT NULL,
  parent_run_id TEXT,                    -- NULL en runs raíz (primary)
  depth         INTEGER NOT NULL DEFAULT 0,  -- 0 = primary, 1 = subagent en v1
  kind          TEXT NOT NULL,           -- ver §Operations::Kind
  agent_id      TEXT,                    -- id del AgentSpec activo en el run
  payload       TEXT NOT NULL,           -- JSON serializado (forma depende de kind)
  payload_truncated INTEGER NOT NULL DEFAULT 0,  -- 0/1
  payload_sha256 TEXT,                   -- hash del payload completo si fue truncado
  duration_ms   INTEGER,                 -- solo en tool_result y provider_event
  FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX idx_journal_session_ts  ON journal(session_id, ts);
CREATE INDEX idx_journal_run         ON journal(run_id);
CREATE INDEX idx_journal_parent_run  ON journal(parent_run_id);
CREATE INDEX idx_journal_kind_ts     ON journal(kind, ts);

CREATE TABLE journal_archive (
  -- misma forma que journal; separado para que las queries
  -- "recientes" no escaneen histórico.
  id            TEXT PRIMARY KEY,
  ts            INTEGER NOT NULL,
  session_id    TEXT NOT NULL,
  run_id        TEXT NOT NULL,
  parent_run_id TEXT,
  depth         INTEGER NOT NULL DEFAULT 0,
  kind          TEXT NOT NULL,
  agent_id      TEXT,
  payload       TEXT NOT NULL,
  payload_truncated INTEGER NOT NULL DEFAULT 0,
  payload_sha256 TEXT,
  duration_ms   INTEGER
);

CREATE INDEX idx_journal_archive_session_ts ON journal_archive(session_id, ts);
CREATE INDEX idx_journal_archive_kind_ts    ON journal_archive(kind, ts);
```

> **Sin `UPDATE` ni `DELETE` permitidos por el `JournalRepo`**. El
> módulo no expone tales métodos. Tests de invariante verifican que
> un `JournalEntry` insertado es bit-idéntico en queries posteriores.
> Migración física: si el schema cambia, se hace via `storage`
> migrations; las filas existentes se preservan (pueden quedar
> `payload` sin nuevos campos; el deserializador es tolerante).

## Operations

### `Kind`

Enum cerrado en Rust, serializado como `snake_case`:

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JournalKind {
    UserMessage,            // mensaje del usuario (F01.session_send)
    AssistantMessage,       -- respuesta del agente (texto completo al message_end)
    ProviderEvent,          -- evento crudo del provider (latencia, error, usage)
    ToolCall,               -- args del tool call detectado en el stream
    ToolResult,             -- output del tool (ok o error)
    PermissionDecision,     -- allow/deny/ask del permission gate
    SubagentLifecycle,      -- started/finished/aborted de un subagent
    DiffProposal,           -- tool call de edit_file/apply_patch/write_file (F04)
    DiffApplied,
    DiffRejected,
    Error,                  -- error del loop (no del provider; eso es ProviderEvent)
}
```

### `JournalEntry`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntry {
    pub id: Ulid,
    pub ts: i64,                    // epoch ms
    pub session_id: SessionId,
    pub run_id: Ulid,
    pub parent_run_id: Option<Ulid>,
    pub depth: u8,                  // 0 o 1 en v1
    pub kind: JournalKind,
    pub agent_id: Option<AgentId>,
    pub payload: serde_json::Value,
    pub payload_truncated: bool,
    pub payload_sha256: Option<String>,  // hex; presente si payload_truncated
    pub duration_ms: Option<u64>,
}
```

### `JournalRepo`

```rust
pub struct JournalRepo {
    conn: Arc<Mutex<rusqlite::Connection>>,
    max_payload_bytes: usize,       // default 16 KiB; payloads más grandes se truncan
}

impl JournalRepo {
    pub fn new(conn: Arc<Mutex<rusqlite::Connection>>, max_payload_bytes: usize) -> Self;
    pub fn append(&self, entry: NewJournalEntry) -> Result<JournalEntry, AppError>;
    pub fn query_by_session(
        &self,
        session_id: &SessionId,
        since: Option<i64>,
        until: Option<i64>,
        kinds: Option<&[JournalKind]>,
        limit: Option<u32>,
        before: Option<Ulid>,
    ) -> Result<Vec<JournalEntry>, AppError>;
    pub fn query_by_run(&self, run_id: &Ulid) -> Result<Vec<JournalEntry>, AppError>;
    pub fn replay_run(&self, run_id: &Ulid) -> Result<RunReplay, AppError>;
    pub fn count(&self) -> Result<u64, AppError>;
    pub fn archive_older_than(&self, keep_rows: u64) -> Result<u64, AppError>;
}
```

**`NewJournalEntry`**: lo que el caller pasa a `append`; el repo
genera `id` (ULID) y `ts` (now), trunca `payload` si excede
`max_payload_bytes` y calcula `payload_sha256` del original, y
devuelve el `JournalEntry` final con los campos autogenerados.

**`RunReplay`**: estructura de replay que agrupa entries por
sub-agent tree.

```rust
pub struct RunReplay {
    pub run: JournalEntry,                          // primer entry del run
    pub entries: Vec<JournalEntry>,                 // ordenadas por (ts, id)
    pub subagent_runs: Vec<RunReplay>,              // recursivo (depth 1 en v1)
}
```

**Errores** (todos `AppError`):
- `internal` — fallo de SQLite (con detail, sin loguear payload).
- `invalid_input` — `payload` no serializable a JSON (debería ser
  imposible; safety net).
- `not_found` — `query_by_run` con `run_id` inexistente (devuelve
  `Vec` vacía, **no** error; tests lo cubren).

**Concurrencia**: el `Connection` está envuelto en `Arc<Mutex<…>>`
(ver `storage.md` §Concurrency). Cada `append` se hace dentro de
una transacción (`BEGIN IMMEDIATE; INSERT; COMMIT;`).

### Helpers para callers

Cada dominio que escribe al journal debe usar el helper tipado en
`agentyx-core::journal` que valida el shape del payload:

```rust
pub fn log_tool_call(
    &self, session_id: &SessionId, run_id: &Ulid,
    agent_id: &AgentId, call: &ToolCall,
) -> Result<JournalEntry, AppError>;

pub fn log_tool_result(
    &self, session_id: &SessionId, run_id: &Ulid,
    tool_call_id: &str, output: &ToolResult,
    duration_ms: u64,
) -> Result<JournalEntry, AppError>;

pub fn log_permission_decision(
    &self, session_id: &SessionId, run_id: &Ulid,
    tool_name: &str, args: &serde_json::Value,
    decision: PermissionDecision,
) -> Result<JournalEntry, AppError>;

pub fn log_subagent_lifecycle(
    &self, parent_run_id: &Ulid, subagent_id: &AgentId,
    child_run_id: &Ulid, event: SubagentLifecycleEvent,
) -> Result<JournalEntry, AppError>;
```

Estos helpers hacen **dos cosas** además del `append`: validan el
shape del payload y lo serializan canónicamente (claves ordenadas)
para que la replay sea bit-estable.

### Tamaño y rotación

- `max_payload_bytes` default 16 KiB. Si un `tool_result` (e.g. un
  `read_file` de un archivo grande) excede, se trunca a 16 KiB,
  `payload_truncated = 1`, `payload_sha256` guarda hash del original.
  El tool en cuestión debe devolver suficiente metadata adicional
  fuera del payload (path, size) para que la UI muestre "archivo
  de 8.4 MB, contenido truncado".
- `archive_older_than(keep_rows)` se llama:
  - Al arrancar la app, una vez por sesión de app (no por workspace).
  - Cuando `journal.count() > workspace.config::journal.max_rows`
    (default 100_000), se llama al final de cada `append` (con
    throttling: 1 vez cada 1000 appends).
- El archivado mueve las filas más antiguas (orden `ts ASC`) a
  `journal_archive`, dentro de una transacción.

## Contracts

### Tauri commands

> El journal **no expone** Tauri commands propios en v1. El acceso
> desde la UI es a través de:
>
> - `journal_query_by_session(sessionId, filters)` en F01 (historial).
> - `journal_replay_run(runId)` (debug, oculto detrás de flag).
> - `journal_count()` para mostrar tamaño al usuario en Settings.
>
> Si F01 no los declara, este spec deja la forma como referencia y
> se añaden cuando se implemente F01. **No** se implementan hasta
> que F01 esté al menos `approved`.

### Endpoints HTTP

Mismo razonamiento: el servidor HTTP embebido (F06, v0.2) expondrá
`GET /api/v1/sessions/:id/journal` y `GET /api/v1/runs/:id/replay`.
La forma exacta se fija en F06.

### Eventos streaming

El journal **no emite** eventos. El agent loop emite `chat.*.v1`
directamente al bus; en el mismo punto hace `journal.append`. La
relación entre ambos es:

```
provider event ──► chat.*.v1 (frontend)
              └──► journal.append (persistencia)
```

Si el `journal.append` falla después de emitir el evento, se
loguea `tracing::error!` y se continúa (no se reintenta; el evento
ya fue entregado al frontend, y la pérdida de un entry es
recuperable de logs estructurados si se necesitan).

## Edge cases

1. **Payload > `max_payload_bytes`**: se trunca a 16 KiB, se
   calcula SHA-256 del original (antes de truncar) y se almacena
   en `payload_sha256`. El caller (helper) recibe un warning
   estructurado para loguear.
2. **Crash mid-append**: SQLite transaccional; el `BEGIN IMMEDIATE`
   previo garantiza que un crash deja la fila no insertada, no una
   fila a medias.
3. **Múltiples `append` concurrentes del mismo run** (e.g. tool
   result + permission decision en paralelo): el `Arc<Mutex<…>>`
   serializa; el orden final es el del lock, que coincide con `ts`
   si los `now()` se llaman dentro del lock. Tests verifican que
   no se pierde ninguno.
4. **`query_by_run` con `run_id` inexistente**: retorna `Vec`
   vacía, **no** error. (Decisión para no penalizar al caller.)
5. **`query_by_session` con `kinds` vacío**: retorna `Vec` vacía
   (no se aplica filtro "ningún kind", porque sería sin sentido).
6. **Journal crece > `max_rows` durante un run**: el archivado se
   difiere al final del run si hay throttling. Esto es aceptable:
   el run es bounded por `max_steps` (ver `agent-loop.md`).
7. **Workspace abierto en dos instancias de la app (mismo user)**:
   SQLite con WAL permite lecturas concurrentes pero escrituras
   serializadas. El segundo `JournalRepo::new` espera al lock.
   **No** se permite; la app detecta "workspace ya abierto" y
   rechaza el segundo arranque con `conflict` (decisión fuera de
   esta spec, en `app.md` cuando exista).
8. **Migración de schema**: si una nueva versión añade campos al
   payload, las filas viejas quedan con `payload` deserializable
   como JSON parcial. El deserializador es `serde_json::Value`
   (no un struct cerrado), por lo que campos faltantes son `Null`,
   no error.
9. **`parent_run_id` huérfano** (e.g. crash antes de que el
   subagent termine): `replay_run` lo trata como root run;
   `subagent_runs` puede tener un run referenciado pero no
   resoluble; el log lo marca con `tracing::warn!`.
10. **Archivado en plataforma con FS lento (network drive)**:
    la transacción puede tardar; se hace en background thread
    `tokio::task::spawn_blocking` y no bloquea el arranque.

## Acceptance criteria

Cada AC → test con nombre derivado.

- [ ] **AC1**: `JournalRepo::append` con un `NewJournalEntry`
  mínimo retorna un `JournalEntry` con `id` (ULID válido),
  `ts ≈ now()` (dentro de 50 ms), y todos los demás campos
  espejados. **Test**: `ac1_append_returns_entry_with_id_and_ts`.
- [ ] **AC2**: dos `append` consecutivos del mismo run retornan
  entries con `ts` monótono no decreciente y `id` ULID ordenado
  lexicográficamente igual a `ts`. **Test**:
  `ac2_append_ordering_matches_ulid`.
- [ ] **AC3**: el repo no expone métodos `update` ni `delete`
  en su API pública (test de signatures: greps + assert que el
  tipo no tiene tales métodos). **Test**:
  `ac3_repo_api_has_no_mutation_methods`.
- [ ] **AC4**: intentar ejecutar `UPDATE journal SET ...` o
  `DELETE FROM journal ...` desde fuera del repo (en un test
  que use la conexión directamente) no es bloqueado por la
  API del repo, pero el repo no expone un método para
  invocarlo — el contrato es API-level, no DB-level. **Test**:
  `ac4_append_only_is_api_level_contract` (test de contrato,
  no de enforcement).
- [ ] **AC5**: un `payload` > `max_payload_bytes` se trunca a
  16 KiB, `payload_truncated = true`, y `payload_sha256`
  contiene el hash del payload original. **Test**:
  `ac5_oversized_payload_truncated_with_sha256`.
- [ ] **AC6**: `query_by_session(s)` con `kinds = Some(&[ToolCall,
  ToolResult])` retorna solo entries de esos kinds, ordenadas
  por `(ts, id)`. **Test**:
  `ac6_query_by_session_filters_kinds_and_orders`.
- [ ] **AC7**: `query_by_session(s, since, until)` aplica el
  rango temporal inclusive en ambos extremos, con el índice
  `idx_journal_session_ts`. **Test**:
  `ac7_query_by_session_filters_time_range`.
- [ ] **AC8**: `query_by_run(r)` con `r` existente retorna todos
  los entries del run, incluyendo subagent runs (los
  subagent runs se devuelven en `replay_run`; aquí solo los
  entries con `run_id = r`). **Test**:
  `ac8_query_by_run_returns_only_that_run_entries`.
- [ ] **AC9**: `replay_run(r)` retorna `RunReplay { run, entries,
  subagent_runs: [...] }` con el subagent agrupado bajo el
  parent run. **Test**: `ac9_replay_run_groups_subagents`.
- [ ] **AC10**: `query_by_run(r)` con `r` inexistente retorna
  `Vec::new()` y `Ok`, no error. **Test**:
  `ac10_query_by_run_missing_returns_empty`.
- [ ] **AC11**: `archive_older_than(keep_rows = N)` mueve
  `count() - N` filas más antiguas a `journal_archive` y las
  borra de `journal`, todo en una transacción. **Test**:
  `ac11_archive_moves_old_rows_atomically`.
- [ ] **AC12**: tras `archive_older_than`, `query_by_session`
  no retorna las filas archivadas (la tabla `journal` ya no
  las tiene). `journal_archive` sí las tiene, y un test
  separado verifica la query a `journal_archive` con el
  mismo filtro. **Test**: `ac12_archive_excludes_from_main_query`.
- [ ] **AC13**: dos `append` concurrentes del mismo run (uno
  `ToolCall`, otro `PermissionDecision`) terminan ambos
  commitados, sin pérdida, y con orden consistente (ts + id).
  **Test**: `ac13_concurrent_appends_both_persist_ordered`.
- [ ] **AC14**: una falla simulada de SQLite (e.g. DB cerrada
  antes de `append`) retorna `Err(AppError::Internal)` con
  detail, y **no** aborta el caller (verificado con un test
  que ejecuta un append bajo `.expect_err(...)`). **Test**:
  `ac14_sqlite_failure_returns_internal_no_panic`.
- [ ] **AC15**: el helper `log_tool_call` serializa el `ToolCall`
  a JSON con claves ordenadas canónicamente, de forma que dos
  llamadas con los mismos args producen payloads byte-idénticos
  (estabilidad para replay determinista). **Test**:
  `ac15_log_tool_call_canonical_json_is_byte_stable`.
- [ ] **AC16**: el helper `log_subagent_lifecycle` con
  `event = SubagentLifecycleEvent::Started` registra
  `parent_run_id` correctamente y `depth = parent.depth + 1`.
  **Test**:
  `ac16_log_subagent_lifecycle_started_links_parent_and_depth`.

## Discovered bugs (post-approval)

| ID | Date | Category | Resolved in | Notes |
|---|---|---|---|---|
| _ninguno aún_ | | | | |

## Open questions

- **Q1**: ¿Debería el journal incluir también la **diff textual**
  de `edit_file` (no solo los args y el resultado)? → **Propuesta**:
  sí, como `payload` del `DiffProposal`, pero con `payload_truncated`
  agresivo (8 KiB). El diff visual vive en el frontend; el journal
  guarda lo mínimo para reproducir textualmente.
- **Q2**: ¿Cuándo se llama `archive_older_than`? ¿Al arranque de
  la app, al abrir el workspace, o periódicamente? → **Propuesta**:
  las tres, con throttling (no más de 1 vez cada 1000 appends o
  1 vez por sesión de app). Tests cubren solo el comportamiento
  final (rows movidas), no el throttling.
- **Q3**: ¿Soporte de `journal.export` en v1? → **No**. Diferido
  a v1.x. La spec de F01 menciona un botón "Export journal" que
  se queda en stub hasta v1.x.
- **Q4**: ¿El journal debe registrar **errores de red** del
  provider (e.g. 429, 5xx)? → **Sí**, como `ProviderEvent` con
  `payload = { provider_id, status, latency_ms, error_code? }`.
  Esto es lo que F09 (dashboard) consume.
- **Q5**: ¿El journal de un subagent abortado se conserva o se
  purga? → **Se conserva**, siempre. Es append-only. La child
  session queda en estado `aborted` y el journal refleja el
  `SubagentLifecycle::Aborted` con `reason`.

## References

- [`../glossary.md`](../glossary.md) — `JournalEntry`, `Run`, `Subagent`.
- [`../ipc.md`](../ipc.md) — convenciones de errores (`{code, message, context?}`).
- [`../architecture.md`](../architecture.md) — flujo de datos Rust ↔ UI.
- [`storage.md`](./storage.md) — `state.db`, migraciones, `Arc<Mutex<Connection>>`.
- [`agent-loop.md`](./agent-loop.md) — caller principal de `append`.
- [`tools.md`](./tools.md) — caller de `log_tool_call` / `log_tool_result`.
- [`permissions.md`](./permissions.md) — caller de `log_permission_decision`.
- [`agents.md`](../agents.md) — caller de `log_subagent_lifecycle`.
- [`features/F01-chat-streaming.md`](../features/F01-chat-streaming.md) —
  expone `journal_query_by_session` y `journal_replay_run` al UI.
- AGENTS.md §6.3 (Journal) y §15 (Checklist antes de merge).
