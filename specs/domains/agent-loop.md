# Agent Loop

**Status**: draft
**Owner**: @miglesias
**Last update**: 2026-06-04
**Affects**: — (el agent loop es orquestador; los demás dominios lo consumen, no al revés)
**Required by**: `features/F01-chat-streaming`, `features/F03-python-venv`, `features/F06-web-server-lan` (vía IPC), y todas las features que ejecuten tools.

> Corazón del producto. Define el bucle ReAct (Reason + Act) que ejecuta
> una sesión: lee mensajes, llama al LLM, detecta tool calls, ejecuta
> tools con permisos, registra en el journal, y emite eventos
> normalizados a la UI.

## Goal

Ejecutar de forma **determinista, cancelable y observable** una
iteración completa del agente en respuesta a un mensaje del usuario:
desde la primera llamada al provider LLM hasta la finalización del
turno (`finish_reason == "stop"`, `max_steps` alcanzado, error, o
abort solicitado).

## Non-goals

- ❌ Implementar providers LLM (OpenAI, Anthropic, Ollama). Ver
  [`providers.md`](./providers.md).
- ❌ Implementar las tools en sí. Ver [`tools.md`](./tools.md).
- ❌ Implementar la matriz de permisos. Ver [`permissions.md`](./permissions.md).
- ❌ Persistir sesiones, mensajes y journal. Ver [`session.md`](./session.md)
  y [`storage.md`](./storage.md).
- ❌ Diseñar prompts del sistema. Eso vive en el código como constantes,
  no en specs.
- ❌ Compactación / summarization de mensajes cuando se excede la
  ventana del modelo. Fuera de v1.
- ❌ Multi-agente (varios agentes hablando entre sí). Fuera de v1.
- ❌ Tool calls en paralelo dentro del mismo step. Por ahora, una tool
  por step; múltiples tool calls en el mismo message se ejecutan en
  serie, en orden de aparición.

## Glossary

Términos locales a este dominio (los globales están en
[`../glossary.md`](../glossary.md)):

- **Run**: una invocación del agent loop en respuesta a un mensaje
  del usuario. Tiene un `RunId` y un ciclo de vida propio.
- **Step**: una iteración dentro del run (un round-trip con el
  provider). Numerado `1..=max_steps`.
- **StepOutput**: resultado de un step. Puede ser `Finish` (turno
  terminado) o `ToolCalls` (modelo pide tools; continuamos).
- **ToolCall**: un `id, name, args` que el modelo pidió ejecutar.
- **ToolResult**: un `tool_use_id, output, is_error` que devolvemos al
  modelo en el siguiente step.

## State

### In-memory (por Run activo, vive en `AppState`)

| Campo | Tipo | Descripción |
|---|---|---|
| `run_id` | `RunId` (ULID) | Identificador único del run. |
| `session_id` | `SessionId` (ULID) | Sesión a la que pertenece. |
| `step` | `u32` | Step actual, 1-indexed. |
| `abort_flag` | `Arc<AtomicBool>` | Señal de abort cooperativo. |
| `started_at` | `DateTime<Utc>` | Cuándo arrancó. |
| `cancelled` | `bool` | `true` si se abortó. |

### Persistente (lo escriben otros dominios; el agent loop solo lee/escribe a través de repos)

| Dato | Ubicación | Quién |
|---|---|---|
| `messages` (input y output) | `state.db` → tabla `messages` | `session.md` |
| `journal` (cada step + tool call) | `state.db` → tabla `journal` o `journal.jsonl` | `journal.md` |
| `usage` (tokens consumidos) | `state.db` → tabla `usage` | `session.md` |

El agent loop **no** es dueño de la persistencia. Llama a los repos
de `session` y `journal` para escribir.

## Operations

### `AgentLoop::start(session_id, user_msg, opts) -> Result<RunHandle, AppError>`

Arranca un nuevo run para la sesión dada.

**Input**:
```rust
pub struct StartOpts {
    pub provider_id: ProviderId,    // OpenAI, Anthropic, Ollama, …
    pub model_id: ModelId,          // modelo concreto
    pub max_steps: Option<u32>,     // default 50
    pub system_prompt_override: Option<String>,
    pub stream: bool,               // siempre true en v1
}
```

**Output**: `RunHandle { run_id, session_id }`.

**Errores**:
- `not_found` — la sesión no existe.
- `invalid_input` — `user_msg` vacío o > 1 MB.
- `provider_unavailable` — el provider no responde.
- `conflict` — la sesión ya tiene un run activo (rechazar; el caller
  puede `abort` primero).

**Permisos requeridos**: el `WorkspaceId` de la sesión debe permitir
tool calls (decisión final por tool en `permissions::check`).

**Efectos colaterales**:
- Inserta el `user_msg` en `messages` (`role: user`).
- Inserta entrada `journal(kind=run.started)`.
- Emite `chat.message.v1` con el mensaje del usuario.
- Lanza el loop en background (`tokio::task`) que va emitiendo eventos
  a medida que avanza.

### `AgentLoop::abort(run_id) -> Result<(), AppError>`

Marca el `abort_flag` del run. El loop se detiene en el siguiente punto
de cancelación (≤ 100 ms, ver AC5).

**Errores**:
- `not_found` — el run ya terminó o nunca existió.

**Efectos colaterales**:
- Inserta entrada `journal(kind=run.aborted)`.
- Emite `chat.message_end.v1` con `finish_reason: "aborted"`.

### `AgentLoop::state(run_id) -> Result<RunState, AppError>`

Snapshot del estado actual de un run activo.

**Output**:
```rust
pub struct RunState {
    pub run_id: RunId,
    pub session_id: SessionId,
    pub step: u32,
    pub started_at: DateTime<Utc>,
    pub status: RunStatus,        // running | finished | aborted | errored
    pub last_error: Option<AppError>,
}
```

## Contracts

### Tauri commands

Ver [`../ipc.md`](../ipc.md) para convenciones (snake_case → camelCase,
errores `{code, message, context?}`).

| Command | Notas |
|---|---|
| `session_send(session_id, user_msg, opts) -> RunHandle` | Equivale a `AgentLoop::start`. |
| `session_abort(run_id) -> ()` | Equivale a `AgentLoop::abort`. |
| `session_get_run_state(run_id) -> RunState` | Equivale a `AgentLoop::state`. |

### HTTP endpoints

`POST /api/v1/sessions/:id/messages` → `RunHandle`
`POST /api/v1/sessions/:id/abort` → `{}`
`GET  /api/v1/runs/:run_id` → `RunState`

### Eventos streaming

(ver [`../ipc.md`](../ipc.md) §3 para el shape y versionado)

| Evento | Cuándo se emite | Payload |
|---|---|---|
| `chat.message.v1` | Al insertar el `user_msg` (inicio) o al final de un step que terminó con `Finish` | `Message` (role: user\|assistant) |
| `chat.delta.v1` | Cada chunk de streaming del provider | `{ sessionId, runId, text }` |
| `chat.tool_use.v1` | Cuando el modelo emite tool calls en un step | `{ runId, toolCalls: ToolCall[] }` |
| `chat.tool_result.v1` | Tras ejecutar (o denegar) una tool | `{ runId, toolUseId, output, isError }` |
| `chat.message_end.v1` | Fin de un run (cualquier razón) | `{ runId, usage, finishReason }` |
| `error.v1` | Errores no recuperables | `{ code, message, context? }` |

### Esquema de base de datos

Este dominio **no crea tablas nuevas**. Las que necesita
(`messages`, `journal`, `usage`) se definen en
[`storage.md`](./storage.md). Aquí solo se documenta qué escribe:

```sql
-- Al insertar user_msg y mensajes assistant (delegado a session.md)
INSERT INTO messages (id, session_id, role, content, created_at) VALUES (?, ?, ?, ?, ?);

-- Cada step + cada tool call (delegado a journal.md)
INSERT INTO journal (id, run_id, session_id, ts, kind, payload, duration_ms, permission_decision)
VALUES (?, ?, ?, ?, ?, ?, ?, ?);

-- Acumulado de tokens (delegado a session.md)
INSERT INTO usage (session_id, run_id, model_id, prompt_tokens, completion_tokens, ts)
VALUES (?, ?, ?, ?, ?, ?);
```

## Edge cases

Cada uno debe ser un test y un AC.

1. **Run abortado durante el streaming**: el provider está enviando
   deltas. El loop ve `abort_flag`, cierra el stream, emite
   `chat.message_end.v1 { finishReason: aborted }`. No se guarda el
   texto parcial como `Message` con `role: assistant`.
2. **Tool call denegada por permisos**: la tool no se ejecuta. Se emite
   `chat.tool_result.v1 { isError: true, output: "denied by permission" }`.
   El modelo recibe el resultado y decide qué hacer (puede pedir otra
   tool o terminar con texto).
3. **`max_steps` alcanzado**: el loop aborta con
   `chat.message_end.v1 { finishReason: length }`. Sesión queda en
   estado `aborted` (no se puede reanudar automáticamente).
4. **Provider timeout / 5xx**: emite `error.v1 { code: provider_unavailable }`
   y termina el run con `finishReason: error`. La sesión queda
   en `errored`. El user puede reintentar el mismo mensaje
   (`session_send` de nuevo).
5. **Tool panic / crash**: capturado, logueado en `tracing::error!`,
   `chat.tool_result.v1 { isError: true, output: "tool crashed" }`.
   El loop **no muere**; sigue.
6. **Mensaje del usuario vacío** (`""` o solo whitespace):
   rechazado en `start` con `invalid_input`. No se crea run.
7. **Múltiples sesiones concurrentes en el mismo workspace**:
   permitido. Cada una tiene su `RunHandle` independiente.
8. **Mensajes históricos largos** que exceden la ventana del modelo:
   en v1, **truncamos por los últimos N tokens** con un warning
   logueado. Compactación queda para v2.
9. **Tool call con `args` malformado** (no es JSON válido): tratado
   como si la tool hubiera devuelto error. `chat.tool_result.v1
   { isError: true, output: "invalid tool args: <detail>" }`.
10. **Sesión en estado terminal** (`finished`/`errored`) que recibe un
    nuevo `session_send`: se permite; crea un nuevo run sobre la misma
    sesión. (Equivalente a "continuar la conversación".)

## Acceptance criteria

Cada AC → test cuyo nombre deriva: `ac<n>_<short>`.

- [ ] AC1: un mensaje del usuario sin tools solicitadas produce
  exactamente un `chat.message.v1` (assistant) + un
  `chat.message_end.v1` con `finishReason: "stop"`. **Test**:
  `ac1_simple_chat_completes_with_stop`.
- [ ] AC2: un mensaje que el modelo responde con un tool call produce
  el ciclo completo: `tool_use.v1` → `tool_result.v1` → (opcional
  más steps) → `message_end.v1`. **Test**:
  `ac2_tool_call_produces_full_cycle`.
- [ ] AC3: si `permissions::check` deniega la tool, el loop **no la
  ejecuta**, emite `tool_result.v1 { isError: true }` y sigue. **Test**:
  `ac3_denied_tool_returns_error_to_model`.
- [ ] AC4: si el loop alcanza `max_steps`, emite
  `message_end.v1 { finishReason: "length" }` y termina. **Test**:
  `ac4_max_steps_terminates_with_length`.
- [ ] AC5: si `abort_flag` se setea durante un step, el loop termina
  en ≤ 100 ms con `message_end.v1 { finishReason: "aborted" }`.
  **Test**: `ac5_abort_terminates_within_100ms`.
- [ ] AC6: un error del provider (timeout, 5xx) emite `error.v1
  { code: "provider_unavailable" }` y termina el run con
  `finishReason: "error"`. La sesión queda en estado `errored`.
  **Test**: `ac6_provider_error_emits_event_and_errored_state`.
- [ ] AC7: cada step y cada tool call dejan entrada en `journal` con
  `run_id`, `step`, `kind`, `payload` (sin secretos), `duration_ms`.
  **Test**: `ac7_each_step_appends_journal`.
- [ ] AC8: dos sesiones activas en paralelo ejecutan sin interferirse
  (cada una mantiene su `abort_flag` y `step` independientes).
  **Test**: `ac8_concurrent_sessions_isolated`.
- [ ] AC9: un `user_msg` con contenido > 1 MB se rechaza en `start`
  con `invalid_input` y no se crea run. **Test**:
  `ac9_oversized_message_rejected`.
- [ ] AC10: el contenido completo de `tool_result` se persiste
  íntegro en `messages` aunque supere tamaños grandes. **Test**:
  `ac10_large_tool_result_persisted_fully`.
- [ ] AC11: un tool call con `args` no-JSON produce
  `tool_result.v1 { isError: true, output: "invalid tool args" }` y
  el loop sigue. **Test**: `ac11_invalid_tool_args_handled`.
- [ ] AC12: el contenido completo del `user_msg` original aparece
  verbatim en el `Message` insertado (sin truncar). **Test**:
  `ac12_user_message_persisted_verbatim`.

## Discovered bugs (post-approval)

Se rellena por PRs de fix. **No borrar**.

| ID | Date | Category | Resolved in | Notes |
|---|---|---|---|---|
| _ninguno aún_ | | | | |

## Open questions

- **Q1**: ¿Abort tiene que ser "graceful" (espera a terminar el step
  actual) o "hard" (corta el step en curso)? → **Propuesta v1**:
  hard, vía `abort_flag` chequeado entre deltas y entre sub-pasos de
  la tool. Un timeout duro en la tool sigue activo. Si en uso real
  resulta abrupto, lo revisamos.
- **Q2**: ¿La tool que está en ejecución cuando llega `abort` recibe
  `SIGTERM`? → **Propuesta v1**: no automáticamente. Si la tool cuelga,
  el user puede `kill` desde otra UI. Auto-SIGTERM es v2.
- **Q3**: ¿Qué pasa con el `assistant` parcial cuando abortamos
  mid-stream? → **Propuesta v1**: se descarta; no se guarda
  `Message` con `role: assistant`. Decisión registrada en este
  spec (edge case 1).
- **Q4**: ¿Un run abortado puede "resumirse" (continuar desde donde
  quedó)? → **Propuesta v1**: no. La sesión queda en estado terminal
  y un nuevo `session_send` crea un run nuevo. Resume explícito es
  v2.

## References

- [`../ipc.md`](../ipc.md) — shape de Tauri commands, HTTP, eventos.
- [`../architecture.md`](../architecture.md) — diagrama del agent loop.
- [`../glossary.md`](../glossary.md) — `ChatEvent`, `Provider`, `Session`,
  `Journal`, `Tool`, `Workspace`, `Permission`.
- [`providers.md`](./providers.md) — `ChatEvent` y `Provider` (próxima spec).
- [`tools.md`](./tools.md) — contrato de tools (próxima spec).
- [`permissions.md`](./permissions.md) — matriz y decisiones (próxima spec).
- [`session.md`](./session.md) — persistencia de mensajes (próxima spec).
- [`storage.md`](./storage.md) — schema SQLite (próxima spec).
