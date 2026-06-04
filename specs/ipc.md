# IPC — Inter-Process Communication

**Status**: draft
**Owner**: @miglesias
**Last update**: 2026-06-04

> Contratos entre Rust (core/app) y el frontend (Svelte) en sus dos
> modos: **Tauri webview nativo** y **navegador HTTP/SSE**.
>
> Esta spec es la **fuente de verdad** de toda API pública de Agentyx.
> Cambios aquí ⇒ actualización en la misma PR.

---

## 1. Principios

1. **Tipado explícito**: argumentos y retornos son tipos serde. No `Value`
   opaco en la frontera.
2. **Naming**:
   - Rust: `snake_case`.
   - TypeScript: `camelCase` (via `#[serde(rename_all = "camelCase")]`).
3. **Errores** uniformes: `{ code: string, message: string, context?: object }`.
   La UI **nunca parsea** `message`; usa `code`.
4. **Versionado de eventos** (streaming): `chat.message.v1`, `pty.output.v1`, ….
   Un cambio incompatible ⇒ nuevo sufijo (`v2`).
5. **Transporte dual**: misma API para el código de UI, dos adapters en
   `lib/ipc.ts` (Tauri nativo o HTTP+SSE). Ver §6.
6. **Canales por concern**. No multiplexar eventos de dominios distintos.
7. **PROHIBIDO** que el renderer haga network calls directos. Todo pasa
   por Rust.

## 2. Tauri commands (request / response)

Definidos en `crates/agentyx-app/src/commands/<scope>.rs`.

### 2.1 Convenciones

```rust
#[tauri::command]
pub async fn list_sessions(
    state: tauri::State<'_, AppState>,
    workspace_id: WorkspaceId,
) -> Result<Vec<SessionInfo>, AppError> {
    // ...
}
```

- Retornos siempre `Result<T, AppError>`.
- Argumentos serializables (no `tauri::State` se serializa, es inyectado).
- Errores con `code` corto y estable.

### 2.2 Códigos de error

| `code` | Significado |
|---|---|
| `not_found` | Recurso no existe (workspace, sesión, file). |
| `forbidden` | Permiso denegado. |
| `invalid_input` | Argumentos no pasan validación. |
| `conflict` | Estado inconsistente (sesión ya abortada, etc.). |
| `internal` | Bug; loguear stack en el core. |
| `provider_unavailable` | El provider LLM no responde / 4xx-5xx. |
| `permission_denied` | La tool fue denegada por la matriz o el usuario. |
| `timeout` | Operación excedió el timeout. |
| `path_traversal` | Intento de escape del workspace root. |

### 2.3 Inventario (borrador — se cierra con cada spec de dominio)

| Command | Scope | Spec |
|---|---|---|
| `workspace_list` | workspace | `domains/workspace.md` |
| `workspace_open(path)` | workspace | `domains/workspace.md` |
| `workspace_detect_venv(workspace_id)` | workspace | `domains/workspace.md` |
| `workspace_create_venv(workspace_id, backend)` | workspace | `domains/workspace.md` |
| `session_list(workspace_id)` | session | `domains/session.md` |
| `session_create(workspace_id, parent_id?)` | session | `domains/session.md` |
| `session_send(session_id, message)` | session | `domains/agent-loop.md` |
| `session_abort(session_id)` | session | `domains/agent-loop.md` |
| `pty_spawn(workspace_id, command, args)` | pty | `domains/pty.md` |
| `pty_write(pty_id, data)` | pty | `domains/pty.md` |
| `pty_resize(pty_id, cols, rows)` | pty | `domains/pty.md` |
| `pty_kill(pty_id)` | pty | `domains/pty.md` |
| `server_get_url()` | server | `domains/server.md` (futuro) |
| `server_set_bind(bind, lan_enabled)` | server | (futuro) |
| `server_get_token()` | server | (futuro) |
| `provider_list()` | providers | `domains/providers.md` |
| `provider_set_active(id, model)` | providers | `domains/providers.md` |
| `config_get(key)` | config | `domains/config.md` (futuro) |
| `config_set(key, value)` | config | (futuro) |

## 3. Eventos (streaming)

Emitidos por Rust, escuchados por el UI. En Tauri nativo: `window.emit`.
En HTTP: SSE en `GET /api/events`.

### 3.1 Convenciones

```ts
type Event<T = unknown> = {
  schema_version: "v1";
  type: string;       // ej: "chat.message"
  payload: T;
  ts: number;         // ms epoch
};
```

Reglas:
- `schema_version` **obligatorio**.
- Cambio incompatible → nuevo tipo (`chat.message.v2`), no edición silenciosa.
- El UI puede descartar versiones que no conoce.

### 3.2 Catálogo

| Evento | Schema | Payload | Spec |
|---|---|---|---|
| `chat.message.v1` | `{ role, content, ... }` | `Message` | `domains/agent-loop.md` |
| `chat.delta.v1` | `{ sessionId, text }` | delta de streaming | `domains/providers.md` |
| `chat.tool_use.v1` | `{ sessionId, toolUseId, name, args }` | tool call del modelo | `domains/agent-loop.md` |
| `chat.tool_result.v1` | `{ sessionId, toolUseId, output, isError }` | resultado de la tool | `domains/agent-loop.md` |
| `chat.message_end.v1` | `{ sessionId, usage, finishReason }` | fin de turno | `domains/providers.md` |
| `pty.output.v1` | `{ ptyId, data: string /* base64 */ }` | salida PTY | `domains/pty.md` |
| `pty.exit.v1` | `{ ptyId, code }` | PTY terminado | `domains/pty.md` |
| `workspace.file_changed.v1` | `{ workspaceId, path }` | file watcher | (futuro) |
| `journal.entry.v1` | `{ entry }` | nueva entrada al journal | (futuro) |
| `permission.request.v1` | `{ requestId, tool, args, danger }` | pedir decisión al usuario | `domains/permissions.md` |
| `permission.resolved.v1` | `{ requestId, decision }` | resolución de la request | `domains/permissions.md` |
| `error.v1` | `{ code, message, context? }` | error global | (transversal) |

## 4. HTTP API (server embebido axum)

Disponible cuando el server está activo. Sirve también los estáticos de
la UI (`ui/dist/`) cuando se accede desde navegador.

### 4.1 Bind y auth

- **Por defecto**: `127.0.0.1:<random>`, **sin auth** (loopback seguro).
- **Opt-in LAN**: `0.0.0.0:<port>`, **auth obligatoria** vía
  `Authorization: Bearer <token>`.
- Token generado al primer arranque, guardado en keychain del SO.
- CORS: allowlist cerrado (solo el propio origen y `null` para file://).

### 4.2 Endpoints (request / response)

Mismo shape que los Tauri commands, en JSON sobre HTTP. Versionado en URL:
`/api/v1/...`.

| Método | Path | Equivalente Tauri | Spec |
|---|---|---|---|
| `GET` | `/api/v1/health` | — | (transversal) |
| `GET` | `/api/v1/workspaces` | `workspace_list` | `domains/workspace.md` |
| `POST` | `/api/v1/workspaces` | `workspace_open` | `domains/workspace.md` |
| `GET` | `/api/v1/workspaces/:id/venv` | `workspace_detect_venv` | `domains/workspace.md` |
| `POST` | `/api/v1/workspaces/:id/venv` | `workspace_create_venv` | `domains/workspace.md` |
| `GET` | `/api/v1/workspaces/:id/sessions` | `session_list` | `domains/session.md` |
| `POST` | `/api/v1/workspaces/:id/sessions` | `session_create` | `domains/session.md` |
| `POST` | `/api/v1/sessions/:id/messages` | `session_send` | `domains/agent-loop.md` |
| `POST` | `/api/v1/sessions/:id/abort` | `session_abort` | `domains/agent-loop.md` |
| `POST` | `/api/v1/pty` | `pty_spawn` | `domains/pty.md` |
| `POST` | `/api/v1/pty/:id/write` | `pty_write` | `domains/pty.md` |
| `POST` | `/api/v1/pty/:id/resize` | `pty_resize` | `domains/pty.md` |
| `DELETE` | `/api/v1/pty/:id` | `pty_kill` | `domains/pty.md` |
| `GET` | `/api/v1/providers` | `provider_list` | `domains/providers.md` |
| `POST` | `/api/v1/providers/active` | `provider_set_active` | `domains/providers.md` |
| `GET` | `/api/v1/server/info` | `server_get_url` | (futuro) |
| `POST` | `/api/v1/server/bind` | `server_set_bind` | (futuro) |

### 4.3 SSE (streaming)

- `GET /api/v1/events` — un SSE stream con todos los eventos tipados.
- Mismo `Event<T>` que §3.1.
- Reconexión con `Last-Event-ID` (futuro).
- Heartbeat cada 15s (`event: ping`).

### 4.4 Respuestas de error

```json
{
  "code": "permission_denied",
  "message": "Tool shell is not allowed in this workspace",
  "context": { "tool": "shell", "workspaceId": "..." }
}
```

Status HTTP:
- `400` `invalid_input`.
- `401` falta token / token inválido.
- `403` `forbidden` / `permission_denied`.
- `404` `not_found` / `path_traversal`.
- `409` `conflict`.
- `502` / `503` `provider_unavailable`.
- `504` `timeout`.
- `500` `internal`.

## 5. Seguridad

- **Sin secretos en el body** de errores. Solo `code` + `message` + `context`
  sin tokens.
- **CSP estricta** en la UI: `script-src 'self'`, sin `unsafe-inline` en
  producción.
- **Capabilities Tauri mínimas** por ventana. Default en
  `crates/agentyx-app/capabilities/default.json`.
- **Path traversal** bloqueado: toda I/O se canonicaliza y se verifica
  que esté dentro del `root` del workspace.
- **Token del server**: solo en `Authorization: Bearer` (nunca en URL).
  Persistido en keychain.

## 6. Adaptador de transporte en la UI

`ui/src/lib/ipc.ts` decide el modo al boot:

```ts
// Pseudo-código
const isTauri = "__TAURI_INTERNALS__" in window;
export const ipc = isTauri ? tauriAdapter() : httpAdapter({ baseUrl, token });
```

API expuesta (idéntica en ambos modos):

```ts
ipc.invoke<TIn, TOut>(command: string, input: TIn): Promise<TOut>;
ipc.listen<T>(eventType: string, handler: (e: Event<T>) => void): () => void;
```

El resto de la UI **nunca** accede a `window.__TAURI__` ni a `fetch` directo.

## 7. Versionado y compatibilidad

- **Tauri commands**: cambios incompatibles → nuevo command (`v2`) y
  deprecación del viejo durante al menos 1 minor.
- **Eventos**: nuevo `schema_version` o nuevo `type` con sufijo.
- **HTTP**: rutas versionadas (`/api/v1/...`). Breaking changes → `/v2`.

## 8. Acceptance criteria

- [ ] AC1: todo `#[tauri::command]` está bajo
  `crates/agentyx-app/src/commands/<scope>.rs` con nombre `snake_case`
  y `#[serde(rename_all = "camelCase")]` en sus tipos.
- [ ] AC2: todo error de comando retorna `{ code, message, context? }`,
  no `String`.
- [ ] AC3: todo evento tiene `schema_version` y `type` con sufijo
  `.<vN>`.
- [ ] AC4: la UI nunca usa `window.__TAURI__` directamente — siempre
  pasa por `lib/ipc.ts`.
- [ ] AC5: el HTTP server exige bearer token cuando `bind != 127.0.0.1`.
- [ ] AC6: las rutas HTTP están versionadas (`/api/v1/...`).
- [ ] AC7: el spec de cada command nuevo o modificado referencia este
  documento (`ipc.md`).
