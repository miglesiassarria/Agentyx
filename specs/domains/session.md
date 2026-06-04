# Session

**Status**: approved
**Owner**: @miglesias
**Last update**: 2026-06-04
**Affects**: — (la sesión es el contenedor que el agent loop y el UI consumen).
**Required by**: `agent-loop.md` (la sesión es input y output del loop),
`features/F01-chat-streaming`, `features/F02-multi-workspace`,
`features/F06-web-server-lan`.

> Una sesión es una unidad de trabajo: un chat + una lista de mensajes
> + un journal + un acumulador de tokens, asociados a un workspace.
> Este dominio define la **entidad persistente** y sus operaciones.
> El agent loop orquesta lo que ocurre dentro; este spec solo guarda.

## Goal

Persistir y consultar el estado de las sesiones de un workspace: qué
sesiones existen, qué mensajes contienen, qué tokens consumieron, y
cuál es el estado de su último run.

## Non-goals

- ❌ Definir el bucle del agente. Ver [`agent-loop.md`](./agent-loop.md).
- ❌ Definir el schema SQLite y las migraciones. Ver
  [`storage.md`](./storage.md).
- ❌ Definir qué es un workspace. Ver [`workspace.md`](./workspace.md).
- ❌ Compactación / summarization de mensajes. Fuera de v1.
- ❌ Búsqueda full-text en mensajes. v1 hace `LIKE` simple si el UI
  lo pide; FTS queda para v2.
- ❌ Compartir sesiones entre workspaces o entre users. v2.

## Glossary

Términos locales:

- **Session status**: estado del último run en esta sesión.
  Ver §State.
- **Message role**: `user` | `assistant` | `system` | `tool_result`.
  Vienen del shape OpenAI/Anthropic.
- **Usage row**: una fila de `usage` con tokens consumidos por un run.

## State

### Tabla `sessions` (definida formalmente en `storage.md`)

| Columna | Tipo | Notas |
|---|---|---|
| `id` | TEXT PRIMARY KEY | ULID. |
| `workspace_id` | TEXT NOT NULL | FK a `workspaces.id`. |
| `parent_id` | TEXT NULL | Para forks / continuaciones (futuro). |
| `title` | TEXT NULL | Derivado del primer mensaje (heurística, ver §Ops). |
| `status` | TEXT NOT NULL | `idle` \| `running` \| `aborted` \| `errored`. |
| `created_at` | INTEGER NOT NULL | ms epoch. |
| `updated_at` | INTEGER NOT NULL | ms epoch (cambia en cada `append_message`). |
| `last_run_id` | TEXT NULL | ULID del último run. |
| `last_run_finish_reason` | TEXT NULL | Eco del `finishReason` para queries rápidas. |

Índices: `(workspace_id, updated_at DESC)` para listar recientes,
`(workspace_id, status)` para filtrar activas.

### Tabla `messages` (referencia)

| Columna | Tipo | Notas |
|---|---|---|
| `id` | TEXT PRIMARY KEY | ULID. |
| `session_id` | TEXT NOT NULL | FK. |
| `run_id` | TEXT NULL | Run que produjo este mensaje (NULL para `user`). |
| `role` | TEXT NOT NULL | Ver glossary. |
| `content` | TEXT NOT NULL | El contenido verbatim (JSON si assistant con tool calls, ver §Edge case 4). |
| `created_at` | INTEGER NOT NULL | ms epoch. |
| `seq` | INTEGER NOT NULL | Monotónico por sesión, para orden estable. |

Índices: `(session_id, seq)` para listar, `(run_id)` para "qué produjo
este run".

### Tabla `usage` (referencia)

| Columna | Tipo | Notas |
|---|---|---|
| `id` | INTEGER PRIMARY KEY AUTOINCREMENT | |
| `session_id` | TEXT NOT NULL | FK. |
| `run_id` | TEXT NOT NULL | FK. |
| `model_id` | TEXT NOT NULL | `provider:model` para distinguir. |
| `prompt_tokens` | INTEGER NOT NULL | |
| `completion_tokens` | INTEGER NOT NULL | |
| `ts` | INTEGER NOT NULL | ms epoch. |

## Operations

### `Session::list(workspace_id, opts) -> Vec<SessionInfo>`

Lista sesiones de un workspace en orden cronológico inverso.

**Input**:
```rust
pub struct ListOpts {
    pub limit: Option<u32>,    // default 50, max 200
    pub status: Option<StatusFilter>,  // optional: idle | running | aborted | errored
}
```

**Errores**: `not_found` (workspace no existe).

### `Session::create(workspace_id, parent_id?) -> SessionInfo`

Crea una sesión nueva en estado `idle`.

**Errores**:
- `not_found` (workspace no existe).
- `invalid_input` (parent_id no pertenece al workspace).

### `Session::get(session_id) -> SessionInfo`

Devuelve una sesión por id.

**Errores**: `not_found`.

### `Session::delete(session_id) -> ()`

Borra la sesión y todos sus mensajes y entradas de usage en una
**transacción** (cascade). El journal se queda (es append-only,
ver `journal.md` futuro).

**Errores**:
- `not_found`.
- `conflict` (la sesión tiene un run activo). El caller debe
  `AgentLoop::abort` primero.

### `Session::list_messages(session_id, opts) -> Vec<Message>`

Lista mensajes en orden `seq` ascendente.

**Input**:
```rust
pub struct ListMessagesOpts {
    pub after_seq: Option<i64>,   // para paginación
    pub limit: Option<u32>,       // default 100, max 500
}
```

**Errores**: `not_found` (sesión no existe).

### `Session::append_message(session_id, msg) -> ()`

Inserta un mensaje. **No** valida semántica (eso es del agent loop).
Sí garantiza que `seq` sea monotónico (lock a nivel de fila o
transacción serializable).

**Errores**:
- `not_found` (sesión no existe).
- `invalid_input` (role desconocido, content vacío, etc.).
- `conflict` (la sesión está borrada o cerrada, v1 nunca cierra
  sesiones así que este código queda para v2).

### `Session::set_status(session_id, status, last_run_id?, last_finish_reason?) -> ()`

Lo llama el agent loop en transiciones:
- `running` al `start`.
- `idle | aborted | errored` al terminar un run (con `last_run_id`).

**Errores**: `not_found`.

### `Session::set_title(session_id, title) -> ()`

Lo llama un job async (futuro) o el UI manualmente. Límite: 200 chars.

## Contracts

### Tauri commands

| Command | Notas |
|---|---|
| `session_list(workspace_id, opts) -> SessionInfo[]` | |
| `session_create(workspace_id, parent_id?) -> SessionInfo` | |
| `session_get(session_id) -> SessionInfo` | |
| `session_delete(session_id) -> ()` | |
| `session_list_messages(session_id, opts) -> Message[]` | |

### HTTP endpoints

`GET  /api/v1/workspaces/:id/sessions` → `SessionInfo[]`
`POST /api/v1/workspaces/:id/sessions` → `SessionInfo`
`GET  /api/v1/sessions/:id` → `SessionInfo`
`DELETE /api/v1/sessions/:id` → `{}`
`GET  /api/v1/sessions/:id/messages` → `Message[]`

### Eventos

Este dominio **no emite eventos** propios. La inserción de mensajes
y los cambios de status los anuncia el agent loop como
`chat.message.v1` y `chat.message_end.v1` (ver [`agent-loop.md`](./agent-loop.md)).

## Edge cases

1. **Borrar sesión con run activo**: rechazado con `conflict`. El
   caller aborta primero.
2. **Crear sesión con `parent_id` que no pertenece al mismo workspace**:
   `invalid_input`. No se hace cross-workspace.
3. **`append_message` concurrente en la misma sesión**: serializado
   vía transacción SQLite (`BEGIN IMMEDIATE`). Garantiza `seq`
   monotónico.
4. **Assistant message con tool calls**: el campo `content` se guarda
   como JSON con shape:
   ```json
   {
     "text": "Voy a leer el archivo.",
     "tool_calls": [{"id": "...", "name": "read_file", "args": {...}}]
   }
   ```
   Esto evita perder estructura en el round-trip con el modelo.
5. **Listar mensajes de una sesión con 10 000+ mensajes**: paginación
   con `after_seq`. La UI debe paginar en carga y scroll.
6. **Título auto-derivado** del primer `user` message (primeros 60
   chars, normalizado). Implementación: job best-effort, no bloquea
   el insert.
7. **Sesión huérfana** (workspace borrado): `ON DELETE CASCADE` en
   `sessions.workspace_id` las limpia. No se permite borrar
   workspace con sesiones activas (ver `workspace.md`).
8. **`set_status` con `last_run_id` que no existe**: la FK falla y
   devuelve `internal` con detalle. Bug si ocurre.

## Acceptance criteria

Cada AC → test con nombre derivado `ac<n>_<short>`.

- [ ] AC1: `create` retorna una `SessionInfo` con `status: "idle"` y
  `id` ULID válido. **Test**: `ac1_create_returns_idle_session`.
- [ ] AC2: `list` devuelve las sesiones de un workspace en orden
  `updated_at DESC`. **Test**: `ac2_list_returns_recent_first`.
- [ ] AC3: `get` de un id inexistente devuelve `not_found`. **Test**:
  `ac3_get_missing_returns_not_found`.
- [ ] AC4: `delete` con run activo devuelve `conflict` y no borra
  nada. **Test**: `ac4_delete_with_active_run_conflicts`.
- [ ] AC5: `delete` sin run activo borra mensajes y usage en una
  transacción. Tras `delete`, `get` devuelve `not_found` y
  `list_messages` también. **Test**:
  `ac5_delete_cascades_messages_and_usage`.
- [ ] AC6: `append_message` con `content` arbitrario (incluido
  multi-byte, JSON, vacío `""` con `role: user` rechazado) lo guarda
  **verbatim**. **Test**: `ac6_append_persists_verbatim`.
- [ ] AC7: dos `append_message` concurrentes asignan `seq` distintos
  y monotónicos. **Test**: `ac7_concurrent_append_monotonic_seq`.
- [ ] AC8: `list_messages` con `after_seq` pagina correctamente y
  no duplica. **Test**: `ac8_list_messages_pagination`.
- [ ] AC9: `append_message` con `role` desconocido devuelve
  `invalid_input`. **Test**: `ac9_unknown_role_rejected`.
- [ ] AC10: tras un run, `set_status(idle, last_run_id, stop)`
  actualiza la fila; `get` lo refleja. **Test**:
  `ac10_set_status_updates_session`.
- [ ] AC11: `set_title` con > 200 chars se trunca silenciosamente
  con `tracing::warn!`. **Test**:
  `ac11_long_title_truncated_with_warning`.

## Discovered bugs (post-approval)

| ID | Date | Category | Resolved in | Notes |
|---|---|---|---|---|
| _ninguno aún_ | | | | |

## Open questions

- **Q1**: ¿La paginación de mensajes debe ser por `seq` o por
  `created_at`? → **Propuesta v1**: por `seq` (monotónico, simple).
  `created_at` puede colisionar en escrituras muy rápidas.
- **Q2**: ¿`set_status` debería ser idempotente? → **Propuesta v1**:
  sí; mismo `(status, last_run_id)` no genera evento duplicado. La
  transición de status en sí la anuncia el agent loop, no este dominio.
- **Q3**: ¿Borrar una sesión borra también el journal? → **Propuesta
  v1**: **no**. El journal es append-only y shared. Las entradas de
  journal de una sesión borrada quedan como históricas (con
  `session_id` que ya no resuelve). Bug si esto causa confusión en
  el UI; si pasa, v2 introduce "journal retention" con pruning.

## References

- [`../ipc.md`](../ipc.md) — Tauri commands, HTTP, eventos.
- [`../architecture.md`](../architecture.md) — dónde encaja `session`.
- [`agent-loop.md`](./agent-loop.md) — quién llama a `append_message`
  y `set_status`.
- [`storage.md`](./storage.md) — schema SQLite (próxima spec).
- [`workspace.md`](./workspace.md) — FK a workspaces (próxima spec).
