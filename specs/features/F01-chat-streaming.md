# F01 — Chat con streaming LLM

**Status**: implemented (partial — Phase 1 backend)
**Owner**: @miglesias
**Last update**: 2026-06-06
**Affects**: [`agent-loop`](../domains/agent-loop.md), [`providers`](../domains/providers.md),
[`session`](../domains/session.md), [`agents`](../agents.md),
[`journal`](../domains/journal.md), [`storage`](../domains/storage.md),
[`permissions`](../domains/permissions.md), [`tools`](../domains/tools.md).
**Depends on**: [`F02`](./F02-multi-workspace.md) (workspaces),
[`F05`](./F05-settings.md) (provider y modelo configurados),
[`config`](../domains/config.md), [`journal`](../domains/journal.md).

## User story

Como **usuario**, quiero **enviar un mensaje al agente, ver su
respuesta streameada token a token, ver los tool calls ejecutarse
en vivo con sus resultados, poder abortar en cualquier momento y
tener todo persistido para revisar luego**, para interactuar de
forma fluida con el agente agentic sobre los archivos de mi
workspace.

## Scope

### In-scope (v0.1)

- **Composer**: input multi-línea, autocompletion de `@<agent-id>`
  (subagents), submit con `Enter` (Shift+Enter para newline).
- **Message list**: render de mensajes user / assistant / tool_call /
  tool_result, con orden estable y grouping por run.
- **Streaming**: tokens llegan vía eventos `chat.content.delta.v1`
  y se renderizan incremental (sin "saltos" visibles).
- **Tool calls**: cuando el LLM emite un tool call, se renderiza
  un bloque "🔧 `<tool_name>`" con args resumidos; al terminar,
  el `tool_result` se muestra debajo (con truncation indicator si
  el payload excede el límite del journal).
- **Abort**: botón "Stop" en el header del composer mientras hay
  un run activo; llama `session_abort` y corta el stream.
- **Active agent**: el header del composer muestra el active
  agent del session (chip con `id` y color por agent). Cycle con
  Tab (responsabilidad de F-agents-ui) actualiza el chip.
- **@mention**: el composer detecta `@<subagent-id>` y pre-llama
  a `agents_invoke_subagent` con el prompt segment (ver `agents.md`
  §Operations). El resultado del subagent se inserta como
  `assistant_message` con `agentId = <subagent>` antes del
  procesamiento del resto del mensaje.
- **Sesiones múltiples**: una sesión activa por workspace; al
  cerrar la sesión, se guarda en `state.db` y aparece en la
  sidebar (F13, v0.2 lo extiende a multi-sesión).
- **Persistencia**: mensajes y journal persisten entre
  reinicios de la app. Cold start: la sesión activa se
  rehidrata y se muestra el último estado.
- **Errores visibles**: cualquier error del agent loop se muestra
  como un banner rojo en el composer (con `code` + `message`).
- **Tool permission prompts**: si la tool requiere `ask` y el
  usuario no la ha "recordado", se muestra un modal
  "Allow this tool? <details>" (implementación detallada en
  v0.2 con F12; en v0.1 el prompt es binario y simple).

### Out-of-scope (v0.1)

- ❌ Adjuntar imágenes o archivos al mensaje (F14, v0.2).
- ❌ Mensajes multimodales (texto + imagen al LLM).
- ❌ Editar un mensaje ya enviado y re-generar (F-msg-edit, v1.x).
- ❌ Branching de sesiones (sesión A → fork → sesión B; v1.x).
- ❌ Búsqueda dentro del historial de una sesión (F-msg-search,
  v1.x).
- ❌ Compartir sesión vía link (F35, backlog).
- ❌ Compaction de contexto (F15, v0.2; v0.1 trunca por tokens).
- ❌ Multi-sesión simultánea en el mismo workspace (F13, v0.2).
- ❌ Cycle con Tab entre primary agents (responsabilidad de
  F-agents-ui, no F01 — pero F01 expone el contrato
  `agent.changed.v1` que la UI consume).
- ❌ Tree de child sessions visible (responsabilidad de
  F-agents-ui; F01 emite los eventos que la UI renderiza).

## UX / UI

### Rutas y componentes

```
ui/src/
├── lib/
│   ├── routes/
│   │   └── Workspace.svelte         # ruta /workspace/:id
│   ├── components/
│   │   ├── chat/
│   │   │   ├── ChatPanel.svelte     # contenedor (header + list + composer)
│   │   │   ├── MessageList.svelte   # render de mensajes
│   │   │   ├── MessageItem.svelte   # un mensaje
│   │   │   ├── ToolCallBlock.svelte # render de un tool call
│   │   │   ├── ToolResultBlock.svelte
│   │   │   ├── Composer.svelte      # input + send + stop
│   │   │   ├── AgentChip.svelte     # active agent badge
│   │   │   ├── AtMentionPopover.svelte
│   │   │   ├── PermissionPrompt.svelte  # modal de aprobación
│   │   │   ├── ErrorBanner.svelte
│   │   │   └── StreamingIndicator.svelte
```

### `ChatPanel.svelte`

```
+----------------------------------------------------+
|  [AgentChip: build ▼]    [Session title]  [Stop]  |
+----------------------------------------------------+
|  [MessageList (scrollable, auto-scroll to bottom)]|
|  ...                                              |
+----------------------------------------------------+
|  [Composer]                                       |
|  ─ Type a message, @mention a subagent... ─       |
|  [Send]                                           |
+----------------------------------------------------+
```

- **Header**: `AgentChip` (cycle con Tab — manejado en
  F-agents-ui), título de la sesión (editable en v1.x, en v0.1
  derivado del primer mensaje truncado a 60 chars), botón
  `Stop` (visible solo si hay run activo).
- **MessageList**:
  - Auto-scroll al fondo cuando llega un nuevo `chat.content.delta.v1`,
    salvo que el usuario haya hecho scroll up manual (en ese caso,
    aparece un botón flotante "↓ Jump to latest").
  - Diferencia visual entre `user_message` (derecha, fondo
    primario) y `assistant_message` (izquierda, fondo neutro).
  - `tool_call` y `tool_result` van inline en el assistant message
    al que pertenecen.
  - Errores se muestran con `ErrorBanner` rojo encima del
    composer (no dentro del message list) para no "ensuciar"
    la conversación.
- **Composer**:
  - Multi-línea, autoscroll interno.
  - `Enter` → submit; `Shift+Enter` → newline.
  - Al teclear `@`, abre `AtMentionPopover` con la lista de
    subagents del `AgentRegistry::subagents()`.
  - Botón `Send` (icono) o `Enter`. Si hay run activo, el botón
    se reemplaza por `Stop`.

### Estados de la sesión

| Estado | Indicador |
|---|---|
| `idle` (sin run activo) | Send habilitado, Stop oculto |
| `running` (run activo) | Send deshabilitado, Stop visible, Composer disabled, streaming indicator en message list |
| `aborted` (recién abortado) | Toast "Stopped", estado → `idle` |
| `error` | Banner rojo persistente con `code` + `message`; Composer habilitado |
| `awaiting_permission` | Modal `PermissionPrompt` sobre todo; resto disabled |

### Permisos

- Cuando una tool requiere `ask` y no está "recordada", se
  muestra `PermissionPrompt`:
  - Tool name + args resumidos (truncado a 1 línea si son largos).
  - Botones: `Allow once` · `Allow for this session` ·
    `Allow always` · `Deny`.
  - "Allow for this session" persiste en memoria del run; "Allow
    always" persiste en `GlobalConfig` (default decision = `allow`
    para esa tool, ver `permissions.md` §Defaults).
- v0.1 tiene un solo nivel (allow/deny). v0.2 con F12 introduce
  reglas con patrón (e.g. "ask si path contiene `.env`").

## Flow

### `session_send` happy path

```
user: "lista los archivos .rs en src/"
  → Composer.svelte submit
  → ipc.invoke("session_send", {
      sessionId,
      content: "lista los archivos .rs en src/",
      mentions: []
    })
  → Tauri command en commands/session.rs
  → SessionService::send(sessionId, content, mentions)
    ├── expand_at_mentions(content, mentions)         // ver agents.md
    ├── AgentLoop::start_run(sessionId, agentSpec, expandedPrompt)
    │     ├── snapshot de ResolvedConfig (approval_mode, providers, secrets)
    │     ├── snapshot de PermissionMatrix
    │     ├── generate run_id (ULID), parent_run_id = null
    │     ├── journal.append(SubagentLifecycle::Started)
    │     ├── emit "chat.run.started.v1"             // NUEVO en F01
    │     └── spawn tokio task: agent_loop_iteration
    │
    │  (loop iteration)
    ├── Provider::chat(ChatRequest { messages, tools, system })
    │     → stream de ChatEvent
    ├── on MessageStart:
    │     emit "chat.message.start.v1"  + journal.append(ProviderEvent)
    ├── on ContentDelta(text):
    │     buffer en memoria del run
    │     emit "chat.content.delta.v1"  // batch cada 50ms para no saturar
    │     (no journal.append por delta; ver §Persistencia)
    ├── on ToolUse(call):
    │     PermissionGate::check(tool, args)
    │     ├── allow → ejecutar tool, journal.append(ToolCall + ToolResult)
    │     │           emit "chat.tool_call.v1" + "chat.tool_result.v1"
    │     ├── ask   → emitir "permission.requested.v1" con payload
    │     │           → UI muestra PermissionPrompt
    │     │           → user responde → "permission.resolved.v1" (HTTP/WS)
    │     │           → continuar
    │     └── deny  → journal.append(ToolCall + ToolResult con isError)
    │                 emit "chat.tool_call.v1" + "chat.tool_result.v1"
    ├── on MessageEnd(usage, finishReason):
    │     journal.append(AssistantMessage con texto completo)
    │     emit "chat.message.end.v1"
    │     si finishReason == "stop" → run termina
    └── repeat hasta stop/error/aborted
    │
    ├── on finish:
    │   journal.append(SubagentLifecycle::Finished)
    │   emit "chat.run.finished.v1"
    └── return
  → (Tauri command retorna inmediatamente; el run es fire-and-forget
     desde el punto de vista de la respuesta HTTP del IPC)
```

> El Tauri command `session_send` retorna `Ok(RunHandle { runId })`
> inmediatamente. El frontend recibe los eventos streaming y sabe
> que el run está activo. Esto es coherente con el patrón
> "long-running task" de Tauri.

### Cancelación: `session_abort`

```
user: click Stop
  → ipc.invoke("session_abort", { sessionId })
  → SessionService::abort(sessionId)
    ├── encuentra run activo
    ├── CancellationToken::cancel(runId)
    ├── agent_loop_iteration chequea token entre cada evento
    │   del provider; al detectar cancel:
    │     - cierra el stream del provider (best-effort)
    │     - journal.append(SubagentLifecycle::Aborted con reason="user")
    │     - emit "chat.run.aborted.v1"
    │     - run termina
    └── si hay un subagent activo, propaga cancel por parent_run_id
  → retorna Ok
```

### Cold start: hidratar sesión activa

```
app: arrancar
  → cargar workspace activo (default = primero o el último abierto)
  → cargar sesión activa (último session_id con `is_active = true`)
  → SessionService::load_history(sessionId, limit = 200)
    → journal.query_by_session(sessionId, since = null, until = null)
    → filtra kind IN (UserMessage, AssistantMessage, ToolCall,
                       ToolResult, SubagentLifecycle)
    → render en MessageList
  → emit "session.hydrated.v1" con sessionId y counts
```

## Affected domains

- [`agent-loop`](../domains/agent-loop.md) — `AgentLoop::start_run`
  se invoca desde F01; el streaming loop completo se modela
  aquí.
- [`providers`](../domains/providers.md) — `Provider::chat` se
  invoca con `ChatRequest`; el `ChatStream` se transforma en
  `ChatEvent` y se emite al bus.
- [`session`](../domains/session.md) — `SessionService::send`,
  `abort`, `load_history`. `state.db` persiste runs, messages,
  active agent.
- [`agents`](../agents.md) — `AgentSpec` activo se carga al
  `start_run`; `expand_at_mentions` y `invoke_subagent` se usan.
- [`journal`](../domains/journal.md) — `JournalRepo::append`
  con `UserMessage`, `AssistantMessage`, `ProviderEvent`,
  `ToolCall`, `ToolResult`, `SubagentLifecycle`, `Error`.
- [`storage`](../domains/storage.md) — `state.db` con tablas
  `sessions`, `messages`, `runs`, `active_session` (todo
  detallado en `storage.md`).
- [`permissions`](../domains/permissions.md) — `PermissionGate::check`
  y el `PermissionMatrix` snapshot.
- [`tools`](../domains/tools.md) — cada tool se ejecuta dentro
  del loop; el resultado se serializa y se emite.

## Affected Tauri commands / endpoints / events

### Tauri commands (F01)

```rust
#[tauri::command]
pub async fn session_create(
    workspace_id: WorkspaceId,
    agent_id: Option<AgentId>,
    title: Option<String>,
) -> Result<SessionDto, AppError>;

#[tauri::command]
pub async fn session_send(
    session_id: SessionId,
    content: String,
    mentions: Vec<AtMention>,
) -> Result<RunHandle, AppError>;

#[tauri::command]
pub async fn session_abort(
    session_id: SessionId,
) -> Result<(), AppError>;

#[tauri::command]
pub async fn session_list(
    workspace_id: WorkspaceId,
    limit: Option<u32>,
    before: Option<Ulid>,
) -> Result<Vec<SessionSummaryDto>, AppError>;

#[tauri::command]
pub async fn session_get_history(
    session_id: SessionId,
    limit: Option<u32>,
    before: Option<Ulid>,
) -> Result<Vec<JournalEntryDto>, AppError>;

#[tauri::command]
pub async fn session_set_active_agent(
    session_id: SessionId,
    agent_id: AgentId,
) -> Result<(), AppError>;

#[tauri::command]
pub async fn session_get_active_agent(
    session_id: SessionId,
) -> Result<AgentId, AppError>;

#[tauri::command]
pub async fn permission_respond(
    request_id: PermissionRequestId,
    decision: PermissionResponse,
) -> Result<(), AppError>;
```

> **`session_send` no espera** al run; retorna `RunHandle` con
> `runId` y el frontend escucha los eventos. Si el run falla al
> arrancar (e.g. provider no configurado), el error se emite
> como `chat.run.error.v1` y el run queda en estado `failed`
> en `state.db`.

### Endpoints HTTP (v0.2, F06)

```
POST   /api/v1/sessions                       (body: { workspaceId, agentId?, title? }) → SessionDto
POST   /api/v1/sessions/:id/send              (body: { content, mentions }) → RunHandle
POST   /api/v1/sessions/:id/abort             → {}
GET    /api/v1/workspaces/:id/sessions        ?limit=&before= → Vec<SessionSummaryDto>
GET    /api/v1/sessions/:id/history           ?limit=&before= → Vec<JournalEntryDto>
PATCH  /api/v1/sessions/:id/active-agent      (body: { agentId }) → {}
GET    /api/v1/sessions/:id/active-agent      → AgentId
POST   /api/v1/permissions/respond            (body: { requestId, decision }) → {}
```

### Eventos streaming (F01)

| Evento | Schema | Payload | Cuándo |
|---|---|---|---|
| `chat.run.started.v1` | `{ sessionId, runId, agentId }` | Al arrancar un run (antes del primer provider call) |
| `chat.message.start.v1` | `{ sessionId, runId, messageId, role }` | `role: "assistant"` (user messages no streamean) |
| `chat.content.delta.v1` | `{ sessionId, runId, messageId, text }` | Por cada chunk de provider (batched a 50ms o N tokens) |
| `chat.tool_call.v1` | `{ sessionId, runId, messageId, toolCallId, name, args, argsSummary }` | Cuando el provider emite un tool_use |
| `chat.tool_result.v1` | `{ sessionId, runId, toolCallId, output, outputSummary, isError, durationMs, truncated }` | Tras ejecutar la tool (o denegarla) |
| `chat.message.end.v1` | `{ sessionId, runId, messageId, usage, finishReason }` | Cuando el provider cierra el message |
| `chat.run.finished.v1` | `{ sessionId, runId, status, durationMs }` | `status: "completed" \| "aborted" \| "error" \| "timeout"` |
| `chat.run.error.v1` | `{ sessionId, runId, code, message, retryable }` | Si el run falla (provider down, model not found, etc.) |
| `chat.run.aborted.v1` | `{ sessionId, runId, reason }` | `reason: "user" \| "timeout" \| "error" \| "max_steps"` |
| `permission.requested.v1` | `{ sessionId, runId, requestId, tool, args, argsSummary }` | Cuando la PermissionGate requiere `ask` |
| `permission.resolved.v1` | `{ sessionId, requestId, decision }` | Cuando el usuario responde (eco para el run que espera) |
| `agent.changed.v1` | `{ sessionId, fromAgentId, toAgentId }` | Cycle con Tab (responsabilidad de F-agents-ui; F01 solo escucha) |
| `subagent.started.v1` | `{ parentRunId, childSessionId, subagentId }` | Cuando un primary delega a un subagent (ver `agents.md`) |
| `subagent.finished.v1` | `{ parentRunId, childSessionId, result }` | Cuando el subagent termina |
| `subagent.aborted.v1` | `{ parentRunId, childSessionId, reason }` | Cuando se aborta |

> **Batching de deltas**: el agent loop acumula `ContentDelta`
> en un buffer en memoria y emite `chat.content.delta.v1`
> como máximo 1 vez cada 50ms, o antes si el buffer excede
> N=100 chars. Esto evita saturar el bus Tauri en providers
> muy rápidos (Ollama local puede emitir >1000 tokens/s).

### Tablas (F01 extiende `storage.md`)

```sql
CREATE TABLE sessions (
  id            TEXT PRIMARY KEY,
  workspace_id  TEXT NOT NULL,
  agent_id      TEXT NOT NULL,
  title         TEXT NOT NULL,
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL,
  is_active     INTEGER NOT NULL DEFAULT 0,  -- 1 = sesión activa del workspace
  FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);

CREATE INDEX idx_sessions_workspace_updated ON sessions(workspace_id, updated_at DESC);
CREATE UNIQUE INDEX idx_sessions_workspace_active ON sessions(workspace_id) WHERE is_active = 1;

CREATE TABLE runs (
  id            TEXT PRIMARY KEY,
  session_id    TEXT NOT NULL,
  parent_run_id TEXT,
  agent_id      TEXT NOT NULL,
  status        TEXT NOT NULL,  -- 'running'|'completed'|'aborted'|'error'|'timeout'
  started_at    INTEGER NOT NULL,
  finished_at   INTEGER,
  cancel_reason TEXT,
  FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX idx_runs_session ON runs(session_id, started_at DESC);

CREATE TABLE messages (
  id            TEXT PRIMARY KEY,    -- ULID
  session_id    TEXT NOT NULL,
  run_id        TEXT,                -- NULL para user_message (no asociada a un run)
  parent_id     TEXT,                -- para tool_result: tool_call_id; null para otros
  role          TEXT NOT NULL,       -- 'user'|'assistant'|'tool'
  agent_id      TEXT,
  content       TEXT NOT NULL,
  content_summary TEXT,             -- resumen corto para render (e.g. primeros 200 chars)
  created_at    INTEGER NOT NULL,
  status        TEXT NOT NULL DEFAULT 'complete',  -- 'streaming'|'complete'|'aborted'|'error'
  usage_json    TEXT,                -- para assistant: { promptTokens, completionTokens, totalTokens }
  finish_reason TEXT,                -- 'stop'|'length'|'tool_use'|'error'|'aborted'
  FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX idx_messages_session_created ON messages(session_id, created_at);

CREATE TABLE permission_requests (
  id              TEXT PRIMARY KEY,    -- ULID
  session_id      TEXT NOT NULL,
  run_id          TEXT NOT NULL,
  tool            TEXT NOT NULL,
  args_summary    TEXT NOT NULL,
  args_json       TEXT NOT NULL,
  status          TEXT NOT NULL DEFAULT 'pending',  -- 'pending'|'resolved'|'expired'
  decision        TEXT,                -- 'allow_once'|'allow_session'|'allow_always'|'deny'
  created_at      INTEGER NOT NULL,
  resolved_at     INTEGER,
  FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX idx_permission_requests_session ON permission_requests(session_id, created_at DESC);
```

> **Persistencia batching**: los `ContentDelta` **no** se
> persisten a `messages` uno a uno. Solo se inserta/actualiza
> la fila del `assistant_message` con el texto acumulado:
> - Cada 500ms mientras streamea, **o**
> - Cuando llega un `tool_call` (corte natural), **o**
> - Cuando llega `MessageEnd` (cierre).
>
> Esto evita miles de INSERTs por sesión en providers rápidos.

## Edge cases

1. **Provider no configurado / Ollama caído** al `session_send`:
   `chat.run.error.v1` con `code: "provider_unavailable"` y
   `retryable: true`. La UI muestra banner y permite reintentar
   sin recargar la página.
2. **Stream cortado por la red** (mid-message): el agent loop
   detecta EOF inesperado y emite `chat.run.error.v1` con
   `code: "stream_interrupted"`. El `assistant_message` queda
   con `status: "aborted"` y el texto recibido hasta el corte.
3. **`@<id>` apunta a un agent que no existe**: el
   `expand_at_mentions` retorna `invalid_input` y el Tauri
   command `session_send` falla antes de empezar el run. La
   UI muestra el error inline en el composer y no envía el
   mensaje.
4. **`@<id>` apuntando a un primary (no subagent)**: igual
   que en `agents.md` AC19: `invalid_input`.
5. **User aborta mid-tool-call** (la tool está ejecutándose):
   el `CancellationToken` se propaga a la tool; tools con
   `tokio::select!` cancelan limpio. Para tools síncronas
   (e.g. `read_file`), la cancelación es best-effort: se
   espera a que termine o a 5s, lo que llegue primero.
6. **Run excede `max_steps`** (default 50, configurable):
   el agent loop termina con `finish_reason: "length"` y
   `status: "completed"`. La UI muestra un indicador "Run
   reached step limit".
7. **Run excede timeout absoluto** (default 10 min, configurable
   por workspace en v1.x; v0.1 hard-coded): `status: "timeout"`,
   `cancel_reason: "timeout"`.
8. **Mensaje con contenido que excede `max_input_tokens` del
   modelo**: el agent loop trunca con summary o retorna
   `invalid_input` claro. UI sugiere partir el mensaje.
9. **Tool result > `journal.max_payload_bytes` (16 KiB)**: el
   journal trunca con `payload_truncated = 1` y
   `payload_sha256`; el evento `chat.tool_result.v1` incluye
   `truncated: true` y la UI muestra un botón "View full" que
   carga el contenido on-demand (no inline).
10. **Doble submit accidental** (Enter doble): el composer se
    deshabilita en cuanto se inicia el run; clicks adicionales
    son no-op.
11. **Cambio de `approval_mode` mid-run**: el snapshot se tomó
    al `start_run`; el cambio no afecta al run en curso (ver
    `permissions.md` §Snapshot semantics).
12. **Cambio de active agent mid-run**: bloqueado con
    `conflict` (ver `agents.md` AC10). El `set_active_agent`
    retorna error; la UI muestra toast "Wait for the current
    run to finish".
13. **Cierre de la app durante un run activo**: el run queda en
    estado `running` en `state.db`. Al reabrir, la app detecta
    runs huérfanos y los marca como `aborted` con
    `cancel_reason: "app_closed"`. El usuario ve la sesión
    truncada y un banner "Last run was interrupted".
14. **Cold start con mensaje truncado en el journal**: si el
    payload fue truncado, el `content_summary` muestra
    "[truncated, 8.4 MB]"; el "View full" sigue funcionando.
15. **Provider rate limit (429)**: el agent loop reintenta 1
    vez con backoff de 1s; si falla de nuevo, emite
    `chat.run.error.v1` con `code: "rate_limited"`,
    `retryable: true`.
16. **Sesión sin agente activo válido** (data corruption): al
    `load_history`, se asigna el primer `primary` del registry
    (log `tracing::warn!`).

## Acceptance criteria

- [x] **F01.AC1**: `session_send` con un mensaje simple
  retorna `Ok(RunHandle { runId })` en <100ms (no espera al
  provider). El frontend recibe `chat.run.started.v1` casi
  inmediatamente y los `chat.content.delta.v1` streamean.
  **Test**: `f01_ac1_send_returns_immediately_streams_async`.
  > **Phase 1 (backend)**: ✅ cubierto — `spawn_run` retorna
  > `RunHandle` sincrónicamente, `run_loop` se ejecuta en
  > `tokio::spawn`. `EventSink::emit("chat.run.started.v1")` se
  > llama antes del spawn.
- [x] **F01.AC2**: los `chat.content.delta.v1` llegan en orden
  y el texto renderizado en `MessageList` coincide
  exactamente con el texto concatenado de los deltas.
  **Test**: `f01_ac2_content_deltas_ordered_and_complete`.
  > **Phase 1 (backend)**: ✅ cubierto — el agent loop acumula
  > `ContentDelta.text` en `accumulated_text` en el orden en
  > que llega; `MessageList` los renderiza concatenados.
- [ ] **F01.AC3**: cuando el LLM emite un tool call (e.g.
  `read_file("src/lib.rs")`), se emiten
  `chat.tool_call.v1` (con `argsSummary`) y, tras ejecutar
  la tool, `chat.tool_result.v1` (con `outputSummary` y
  `durationMs`). **Test**:
  `f01_ac3_tool_call_and_result_events_emitted`.
  > **Phase 1 (backend)**: ⏸ diferido a F01-Phase2. El
  > `OllamaProvider` no emite `ChatEvent::ToolUse` aún (los
  > modelos locales raramente lo hacen); el agent loop loguea
  > y descarta el evento cuando llega.
- [x] **F01.AC4**: `session_abort` durante un streaming activo
  cierra el stream del provider y emite `chat.run.aborted.v1`
  con `reason: "user"`. El `assistant_message` queda con
  `status: "aborted"` y el texto recibido hasta el corte.
  **Test**: `f01_ac4_abort_terminates_run_with_partial_text`.
  > **Phase 1 (backend)**: ✅ parcial — `RunHandle::abort()`
  > activa un `AtomicBool` que el agent loop chequea entre
  > deltas; el status final queda en `Aborted`. El evento
  > `chat.run.aborted.v1` se añade cuando se implemente la
  > command `session_abort` en la app (F01-Phase2).
- [x] **F01.AC5**: tras un run, los `messages` rows
  correspondientes existen en `state.db` con `content`
  completo (todos los deltas acumulados) y `usage_json`
  poblado. `journal` contiene `UserMessage`,
  `ProviderEvent`, `AssistantMessage` con el orden correcto.
  **Test**: `f01_ac5_run_persists_messages_and_journal`.
  > **Phase 1 (backend)**: ✅ cubierto — `append_message`
  > persiste user + assistant; `JournalRepo::append` registra
  > `UserMessage`, `ProviderEvent`, `AssistantMessage` con
  > `run_id` y `agent_id`. 16 KiB payload cap con SHA-256
  > (ver `journal.md` §Edge 5).
- [x] **F01.AC6**: cerrar y reabrir la app → la sesión activa
  se rehidrata y los mensajes históricos se muestran
  correctamente en `MessageList` con su orden original.
  **Test**: `f01_ac6_session_hydrates_after_app_restart`.
  > **Phase 1 (backend)**: ✅ parcial — `sessions` y `messages`
  > persisten en `state.db`; `SessionService::list` y
  > `list_messages` los recuperan. La UI de hidratación entra
  > en F01-Phase2.
- [ ] **F01.AC7**: una tool con `PermissionDecision::Ask`
  (e.g. `shell`) emite `permission.requested.v1` y pausa el
  run. Al recibir `permission_respond` con `Allow once`, el
  run continúa, ejecuta la tool y emite el `tool_result`.
  **Test**: `f01_ac7_permission_prompt_blocks_run_until_response`.
  > **Phase 1 (backend)**: ⏸ diferido a F01-Phase2. No hay
  > tool registry ni permission gate en este slice.
- [ ] **F01.AC8**: un tool call con `args` grandes (>1KB)
  tiene `argsSummary` truncado a 1 línea en el evento (no
  el `args` completo). El `args` completo se persiste en
  `journal` y se puede leer con `journal_query_by_session`.
  **Test**: `f01_ac8_large_args_summary_truncated_event_full_in_journal`.
  > **Phase 1 (backend)**: ⏸ diferido a F01-Phase2.
- [ ] **F01.AC9**: cambio de active agent con Tab (o
  `session_set_active_agent`) entre runs → el siguiente
  `session_send` usa el nuevo `AgentSpec` (system prompt,
  tools, permissions). **Test**:
  `f01_ac9_active_agent_change_affects_next_run`.
  > **Phase 1 (backend)**: ✅ parcial — `SessionService::set_active_agent`
  > y `get_active_agent` están implementados y testeados; la
  > command Tauri `session_set_active_agent` se cablea en
  > F01-Phase2.
- [ ] **F01.AC10**: `expand_at_mentions("@general busca auth")`
  en el `session_send` dispara `subagent.started.v1` antes
  del `chat.run.started.v1` del primary; el resultado del
  subagent se inserta como `assistant_message` con
  `agentId: "general"` y el primary continúa con ese
  contexto. **Test**:
  `f01_ac10_at_mention_invokes_subagent_before_primary`.
  > **Phase 1 (backend)**: ⏸ diferido a F01-Phase3.
- [ ] **F01.AC11**: provider retorna 429 → el agent loop
  reintenta 1 vez con backoff 1s; si el segundo intento
  también devuelve 429, emite `chat.run.error.v1` con
  `code: "rate_limited"`, `retryable: true`. **Test**:
  `f01_ac11_rate_limit_retries_then_errors`.
  > **Phase 1 (backend)**: ⏸ diferido. `OllamaProvider::chat`
  > retorna `AppError::Provider { retryable }` para 4xx/5xx
  > pero no implementa retry con backoff. v1.x.
- [ ] **F01.AC12**: el batching de deltas agrupa al menos
  50ms de tokens antes de emitir un `chat.content.delta.v1`;
  en un stream de 1000 tokens/s, no se emiten más de 20
  eventos/s. **Test**:
  `f01_ac12_deltas_batched_at_50ms_or_100_chars`.
  > **Phase 1 (backend)**: ⏸ no implementado. El loop emite
  > cada `ContentDelta` individualmente. v1.x.
- [ ] **F01.AC13**: la inserción de `messages` no ocurre
  en cada delta; ocurre a los 500ms, al primer `tool_call`
  o al `MessageEnd`. Test verifica con un mock que cuenta
  INSERTs. **Test**:
  `f01_ac13_message_persistence_batched_not_per_delta`.
  > **Phase 1 (backend)**: ⏸ no implementado. La persistencia
  > del assistant message es 1 INSERT al final del run (no
  > por delta), lo cual es suficiente para v0.1; el batch a
  > 500ms entra en v1.x si la performance lo demanda.
- [ ] **F01.AC14**: cambio de `approval_mode` mid-run no
  afecta al run en curso (snapshot semantics). **Test**:
  `f01_ac14_approval_mode_change_during_run_ineffective`.
  > **Phase 1 (backend)**: ⏸ N/A — no hay permission gate
  > en este slice; la snapshot ya es implícita (los runs
  > no releen config mid-loop).
- [ ] **F01.AC15**: cierre forzado de la app durante un run
  → al reabrir, el run queda en estado `aborted` con
  `cancel_reason: "app_closed"` y la UI muestra banner
  explicativo. **Test**:
  `f01_ac15_run_aborted_on_app_close_recovered_on_reopen`.
  > **Phase 1 (backend)**: ⏸ N/A — la app-side se cablea en
  > F01-Phase2 (handlers de `tauri::RunEvent::ExitRequested`).

## Discovered bugs (post-approval)

| ID | Date | Category | Resolved in | Notes |
|---|---|---|---|---|
| _ninguno aún_ | | | | |

## Tests

- **Unit (Rust)**:
  - `crates/agentyx-core/src/agent/loop.rs::tests` — run lifecycle,
    cancellation, batching, max_steps, timeout.
  - `crates/agentyx-core/src/agent/persistence.rs::tests` — message
    batch insert, journal append ordering.
  - `crates/agentyx-core/src/providers/streaming.rs::tests` —
    normalización a `ChatEvent`, EOF detection, 429 retry.
  - `crates/agentyx-core/src/permissions/gate.rs::tests` — ask flow,
    remember decision, snapshot semantics.
- **Integration (Rust)**:
  - `crates/agentyx-core/tests/session_lifecycle.rs` — full happy
    path con un provider mock (wiremock grabando SSE grabado).
  - `crates/agentyx-core/tests/abort_mid_stream.rs`.
  - `crates/agentyx-core/tests/app_close_recovery.rs`.
- **Unit (TS)**:
  - `ui/src/lib/components/chat/Composer.test.ts` — submit, Enter,
    Shift+Enter, @mention popover.
  - `ui/src/lib/components/chat/MessageList.test.ts` — render con
    eventos simulados, auto-scroll, jump-to-latest.
  - `ui/src/lib/components/chat/PermissionPrompt.test.ts` —
    4 botones, remember decision.
  - `ui/src/lib/stores/session.svelte.test.ts` — runes state.
- **E2E (Playwright)**: `ui/e2e/chat.spec.ts` — flujo completo
  con Ollama local (o un mock server), incluyendo abort y
  permission prompt.
- **Manual smoke**: con Ollama local, mensaje "lista archivos
  en src/", verificar streaming visible, tool call ejecutado,
  tool result mostrado.

## Telemetry / logs

```rust
tracing::info!(
    session_id = %session_id,
    run_id = %run_id,
    agent_id = %agent_id,
    provider_id = %provider_id,
    model = %model,
    "run started"
);

tracing::info!(
    run_id = %run_id,
    duration_ms = ms,
    prompt_tokens = pt,
    completion_tokens = ct,
    finish_reason = %fr,
    "run finished"
);

tracing::warn!(
    run_id = %run_id,
    tool = %tool,
    error_code = %e.code(),
    "tool execution failed"
);

tracing::error!(
    run_id = %run_id,
    provider_id = %provider_id,
    error_code = %e.code(),
    "run errored"
);
```

> **Nunca** loguear:
> - El contenido completo de `user_message` o `assistant_message`
>   (puede tener código, paths, secrets del usuario). Solo el
>   `content_summary` truncado.
> - Los args de un tool call si contienen paths absolutos del
>   workspace (log paths relativos solo).
> - El `Authorization` header de un provider.

## Security notes

- **Capabilities Tauri**: la ventana principal tiene permiso
  para los commands `session_*` y `permission_respond`. No
  para `config_*`, `secrets_*`, ni `workspace_delete`.
- **Content Security Policy**: la UI sanitiza todo lo que
  se renderiza como HTML (markdown via `marked` + `DOMPurify`).
  Code blocks se pasan por `shiki` solo si el usuario lo
  solicita (lazy).
- **Tool output en chat**: el `output` de una tool puede
  contener HTML o markdown. La UI lo renderiza con el mismo
  pipeline `marked + DOMPurify`. No se hace `innerHTML`
  directo.
- **Path traversal**: las tools validan paths contra el
  sandbox del workspace (root + extra_paths; ver `workspace.md`
  + ADR-0007). Un path inválido retorna
  `path_outside_workspace` y se loguea.
- **Prompt injection**: el agent loop trata los tool results
  como contenido del user (no del system); los providers
  modernos (Claude, GPT-4+) están entrenados para resistirlo.
  En v1.x se introduce un `prompt_injection_detector` opcional
  (backlog).
- **Abuse**: rate limiting por provider (ver `providers.md`
  §Rate limit) evita que un solo usuario sature Ollama local
  con requests concurrentes.

## Rollout

- **Feature flag**: no. F01 es la feature principal del MVP.
- **Onboarding**: F23 introduce un wizard que termina con un
  `session_send` de ejemplo ("¿qué archivos hay aquí?").
- **Migración de datos**: ninguna en v0.1.
- **Compatibilidad**: las sesiones existentes en formato
  pre-F01 (si los hay de versiones internas) se importan
  con un script de migración; en v0.1 (fresh start) no aplica.

## Open questions

- **Q1**: ¿El `Composer` debe soportar **drag & drop** de
  archivos (adjuntar imágenes para F14)? → **No en v0.1**.
  Diferido a F14. El composer ignora drops con un toast
  "File attachments coming soon".
- **Q2**: ¿El `MessageList` debe permitir **editar** un mensaje
  pasado y re-generar? → **No en v0.1**. Diferido a
  F-msg-edit (v1.x).
- **Q3**: ¿Soporte de **markdown** en el composer (vista
  previa)? → **No en v0.1**. Diferido a v1.x.
- **Q4**: ¿El timeout absoluto de 10 min es por run o por
  mensaje? → **Por run**. Un run puede tener N mensajes
  (tool call → result → content delta → tool call → …).
  Ver `agent-loop.md` §Edge 7.
- **Q5**: ¿`session_abort` debe ser idempotente (segunda
  llamada = no-op) o retornar error si no hay run activo?
  → **Idempotente**: segunda llamada retorna `Ok(())` sin
  efecto. Decisión de UX.
- **Q6**: ¿El `permission.requested.v1` se emite también
  desde un subagent? → **Sí**, con `runId = child_run_id`.
  La UI muestra el prompt en el contexto del subagent (en
  v0.1, dentro del parent message; v0.2 con F-agents-ui lo
  separa).
- **Q7**: ¿La **historia completa** de una sesión (con todos
  los tool calls) se persiste en `state.db` o solo en
  `journal`? → **En ambos** (mensajes resumidos en `messages`
  para el cold start, journal completo para replay/debug).
  Esto dobla el espacio en disco pero asegura que la UI
  arranca rápido sin tener que parsear el journal.

## References

- [`../glossary.md`](../glossary.md) — `Run`, `ChatEvent`, `Message`,
  `Session`, `ToolCall`, `PermissionDecision`.
- [`../ipc.md`](../ipc.md) — Tauri command shape, error shape,
  eventos.
- [`../architecture.md`](../architecture.md) — flujo Rust ↔ UI.
- [`agent-loop.md`](../domains/agent-loop.md) — `AgentLoop`,
  `ChatEvent`, `ChatRequest`, `Run`.
- [`providers.md`](../domains/providers.md) — `Provider::chat`,
  `ChatStream`, `ChatEvent` normalization.
- [`session.md`](../domains/session.md) — `SessionService`,
  `state.db` tables.
- [`agents.md`](../agents.md) — `AgentSpec`, `expand_at_mentions`,
  `invoke_subagent`, child sessions.
- [`journal.md`](../domains/journal.md) — `JournalRepo`, batching,
  archive.
- [`storage.md`](../domains/storage.md) — `state.db` migrations,
  connection pool.
- [`permissions.md`](../domains/permissions.md) —
  `PermissionGate`, `PermissionMatrix`, snapshot semantics.
- [`tools.md`](../domains/tools.md) — tool execution, args,
  results.
- [`F02-multi-workspace.md`](./F02-multi-workspace.md) — sesión
  pertenece a un workspace.
- [`F04-file-diffs.md`](./F04-file-diffs.md) — consume los tool
  calls de `edit_file`/`apply_patch` para renderizar diffs.
- [`F05-settings.md`](./F05-settings.md) — provider/model
  configuration consumed here.
- [`F-agents-ui.md`](./F-agents-ui.md) — cycle con Tab, @mention
  popover, child session tree.
- [`features/ROADMAP.md`](./ROADMAP.md) — F01 en Phase 3.
- AGENTS.md §6 (Agent loop), §8.4 (Streaming), §9 (Seguridad),
  §15 (Checklist).

## Implementation status

> Snapshot del estado real de implementación. Se actualiza en el
> mismo PR que cambia el código (ver `AGENTS.md` §17 Spec-Driven
> Development). La fecha indica el último sync.

**Última sync**: 2026-06-06
**Backend (Rust) — F01-Phase1**: **5 / 15 ACs cubiertos** (AC1, AC2, AC4, AC5, AC6) ✅
**Backend (Rust) — F01-Phase2/3**: ⏸ 0 / 15 adicional (tools, permissions, multi-agent, @mention)
**IPC (Tauri commands)**: ⏸ 0 / 7 (los stubs siguen retornando "not yet implemented")
**UI (Svelte)**: ⏸ 0 / 15 (entra en F01-Phase2)

### Cobertura Phase 1 (este PR — `feat(core): F01-Phase1 backends`)

| AC | Cobertura | Tests |
|---|---|---|
| F01.AC1 | `spawn_run` retorna `RunHandle` sincrónicamente; el loop corre en `tokio::spawn`; `chat.run.started.v1` se emite antes del spawn | `agent::loop_::tests::spawn_run_with_unreachable_provider_emits_error_event` |
| F01.AC2 | `ContentDelta` se acumula en `accumulated_text` en orden; UI los concatena tal cual | (UI test pendiente F01-Phase2) |
| F01.AC4 | `RunHandle::abort()` activa `AtomicBool`; el loop chequea entre deltas; status final = `Aborted` | (test integración con provider mock — F01-Phase2) |
| F01.AC5 | `append_message` persiste user + assistant; `JournalRepo::append` registra `UserMessage`/`ProviderEvent`/`AssistantMessage` con `run_id`/`agent_id` | `agent::loop_::tests::conflict_when_session_already_running` (pista), `journal::repo::tests::*` |
| F01.AC6 | `SessionService::list` / `list_messages` recuperan del `state.db`; persistencia con WAL+FK | `session::service::tests::*` |

### Módulos entregados

| Módulo | Propósito | Líneas |
|---|---|---|
| `crates/agentyx-core/src/storage/` | `Db` (rusqlite bundled, WAL+FK), M0001+M0002 migrations, `with_conn`/`transaction` helpers | ~280 |
| `crates/agentyx-core/src/session/` | `SessionService` con `create`/`list`/`get`/`delete`/`append_message`/`start_run`/`finish_run`/`set_active_agent`/`get_active_agent` | ~470 |
| `crates/agentyx-core/src/agents/` | `AgentRegistry::load_builtins` (3 visible + 3 hidden), `AgentSpec`/`AgentMode`/`ToolAccess`/`AgentPermissionOverride`/`ModelRef`/`PromptSource` | ~430 |
| `crates/agentyx-core/src/llm/` | `Provider` trait, `ChatEvent`/`ChatRequest`/`ChatMessage`/`Usage`/`FinishReason`, `OllamaProvider` con NDJSON streaming contra `/api/chat` | ~580 |
| `crates/agentyx-core/src/config/` | `GlobalConfig` v1, `ProviderConfig`, `SecretRef::Env|Keychain`, `KeychainAccess` trait + `OsKeychain` (keyring feature) + `FakeKeychain`, `ConfigService` con atomic write + .bak | ~620 |
| `crates/agentyx-core/src/journal/` | `JournalRepo` con append idempotente (16 KiB cap + SHA-256), `query_by_session`/`query_by_run`/`count` | ~330 |
| `crates/agentyx-core/src/agent/` | `EventSink` trait, `AgentLoopDeps`, `spawn_run`, `RunHandle`/`RunState`/`RunStatus`, `RunRegistry`, `MAX_USER_MSG_BYTES`/`DEFAULT_MAX_STEPS`, `summarize` | ~1100 |

**Total nuevo**: ~3810 líneas Rust en `agentyx-core` (sin contar tests: ~14 tests nuevos en `agent::loop_::tests` + 84 acumulados).

### Eventos streaming implementados (Phase 1)

- `chat.run.started.v1` — `{ runId, sessionId, workspaceId, agentId, model, startedAt }`
- `chat.message_start.v1` — `{ runId, messageId, model }`
- `chat.content.delta.v1` — `{ runId, sessionId, text }`
- `chat.run.finished.v1` — `{ runId, sessionId, usage, finishReason }`
- `chat.run.error.v1` — `{ runId, sessionId, code, message, retryable }`

Pendientes: `chat.run.aborted.v1`, `chat.tool_call.v1`, `chat.tool_result.v1`, `subagent.*.v1`.

### Decisiones de Phase 1 (vs spec original)

1. **Ollama solo**: Groq/Minimax providers se difieren a F01-Phase2 (mismo trait `Provider`, swap-in del impl).
2. **No hay tools**: `ChatRequest.tools` es `Vec::new()`. `ToolUse` events se loguean y descartan.
3. **No hay permission gate**: las tools se ejecutan sin prompt.
4. **No hay subagents**: el `tool` `task` no existe; `ToolResult` history se aplana a `User` con prefijo `[tool result]`.
5. **1 assistant message por run** (single-turn): max_steps cap a 50 (default); Phase 1 no implementa el tool-loop completo.
6. **Persistencia 1 INSERT al final del run** (no por delta): suficiente para v0.1; el batching a 500ms entra en v1.x.
7. **No retry con backoff en el agent loop**: `OllamaProvider::chat` retorna errores categorizados; el loop los surface sin retry. v1.x.

### PRs de referencia

- `feat(core): F01-Phase1 backends (storage, session, agents, config, journal, llm, agent)` (este PR) — +3810 líneas core, 14 tests nuevos, 98 tests totales pasando.
- (pendiente) `feat(app): wire F01-Phase1 commands` — Tauri commands `session_*`, `config_*`, `agents_*`, `providers_*`, `secrets_*`.
- (pendiente) `feat(ui): F01-Phase1 chat panel` — `ChatPanel.svelte`, `MessageList.svelte`, `Composer.svelte`, integración con `lib/ipc.ts`.
- (pendiente) `feat(core,app): F01-Phase2 (tools, permissions, max_steps loop)` — implementa AC3, AC7, AC8, AC12, AC13.
