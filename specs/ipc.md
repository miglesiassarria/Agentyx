# IPC — Inter-Process Communication

**Status**: approved
**Owner**: @miglesias
**Last update**: 2026-06-07

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

### 2.3 Inventario

| Command | Scope | Spec |
|---|---|---|
| `list_workspaces()` | workspace | `domains/workspace.md` |
| `open(root_path, name?)` | workspace | `domains/workspace.md` |
| `get_workspace(workspace_id)` | workspace | `domains/workspace.md` |
| `delete_workspace(workspace_id, force)` | workspace | `domains/workspace.md` |
| `detect_workspace_venv(workspace_id)` | workspace | `domains/workspace.md` |
| `add_extra_path(workspace_id, path, label?)` | workspace | `domains/workspace.md` |
| `remove_extra_path(workspace_id, path)` | workspace | `domains/workspace.md` |
| `list_extra_paths(workspace_id)` | workspace | `domains/workspace.md` |
| `effective_paths(workspace_id)` | workspace | `domains/workspace.md` |
| `list_dir(workspace_id, path)` | workspace | `domains/workspace.md` |
| `create_session(workspace_id, agent_id?, title?)` | session | `domains/session.md` |
| `send(session_id, content, mentions)` | session | `domains/agent-loop.md` |
| `abort(session_id)` | session | `domains/agent-loop.md` |
| `list_sessions(workspace_id, limit?)` | session | `domains/session.md` |
| `get_history(session_id, limit?)` | session | `domains/session.md` |
| `set_active_agent(session_id, agent_id)` | agents/session | `agents.md` |
| `get_active_agent(session_id)` | agents/session | `agents.md` |
| `list_agents()` | agents | `agents.md` |
| `get_agent(id)` | agents | `agents.md` |
| `config_get_global()` | config | `domains/config.md` |
| `config_update_global(patch)` | config | `domains/config.md` |
| `config_get_workspace(workspace_id)` | config | `domains/config.md` |
| `config_update_workspace(workspace_id, patch)` | config | `domains/config.md` |
| `providers_test_connection(request)` | providers | `domains/providers.md` |
| `set_secret(provider_id, value)` | secrets | `domains/config.md` |
| `delete_secret(provider_id)` | secrets | `domains/config.md` |
| `list_providers()` | secrets | `domains/config.md` |
| `get_matrix(workspace_id?)` | permissions | `domains/permissions.md` |
| `set_default(tool, decision)` | permissions | `domains/permissions.md` |
| `list()` | permissions | `domains/permissions.md` |
| `respond(request_id, response)` | permissions | `domains/permissions.md` |
| `server_get_info()` | server | `features/F06-web-server-lan.md` |
| `server_update_config(patch)` | server | `features/F06-web-server-lan.md` |
| `server_rotate_token()` | server | `features/F06-web-server-lan.md` |

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
| `agent.changed.v1` | `{ sessionId, fromAgentId, toAgentId }` | active agent cambió | `agents.md` |
| `subagent.started.v1` | `{ parentRunId, childSessionId, subagentId }` | subagent arrancó | `agents.md` |
| `subagent.finished.v1` | `{ parentRunId, childSessionId, result }` | subagent terminó | `agents.md` |
| `subagent.aborted.v1` | `{ parentRunId, childSessionId, reason }` | subagent abortado | `agents.md` |
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
- **Opt-in LAN**: `0.0.0.0:<port>`, controlado por `[server].require_token`:
  - `require_token = true` (recomendado fuera del dogfooding): **auth
    obligatoria** vía `Authorization: Bearer <token>`. 401 sin token
    o con token inválido.
  - `require_token = false` (default MVP para dogfooding en LAN de
    confianza): server emite un `tracing::warn!` único al arrancar y
    sirve `/api/v1/*` sin requerir token. Ver
    [`features/F06-web-server-lan.md`](./features/F06-web-server-lan.md) §MVP dogfooding caveats.
- Token generado al rotar (`server_rotate_token`), guardado en keychain del SO.
- CORS: allowlist cerrado (solo el propio origen).

### 4.2 Endpoints (request / response)

Mismo shape que los Tauri commands, en JSON sobre HTTP. Versionado en URL:
`/api/v1/...`.

| Método | Path | Equivalente Tauri | Spec |
|---|---|---|---|
| `GET` | `/api/v1/health` | — | (transversal) |
| `GET` | `/api/v1/server/info` | `server_get_info` | `features/F06-web-server-lan.md` |
| `PATCH` | `/api/v1/server/config` | `server_update_config` | `features/F06-web-server-lan.md` |
| `POST` | `/api/v1/server/token/rotate` | `server_rotate_token` | `features/F06-web-server-lan.md` |
| `GET` | `/api/v1/workspaces` | `list_workspaces` | `domains/workspace.md`, `features/F06-web-server-lan.md` |
| `POST` | `/api/v1/workspaces` | `open` | `domains/workspace.md`, `features/F06-web-server-lan.md` |
| `GET` | `/api/v1/workspaces/:id` | `get_workspace` | `domains/workspace.md`, `features/F06-web-server-lan.md` |
| `DELETE` | `/api/v1/workspaces/:id` | `delete_workspace` | `domains/workspace.md`, `features/F06-web-server-lan.md` |
| `GET` | `/api/v1/workspaces/:id/venv` | `detect_workspace_venv` | `domains/workspace.md`, `features/F06-web-server-lan.md` |
| `GET` | `/api/v1/workspaces/:id/extra-paths` | `list_extra_paths` | `domains/workspace.md`, `features/F06-web-server-lan.md` |
| `POST` | `/api/v1/workspaces/:id/extra-paths` | `add_extra_path` | `domains/workspace.md`, `features/F06-web-server-lan.md` |
| `DELETE` | `/api/v1/workspaces/:id/extra-paths` | `remove_extra_path` | `domains/workspace.md`, `features/F06-web-server-lan.md` |
| `GET` | `/api/v1/workspaces/:id/effective-paths` | `effective_paths` | `domains/workspace.md`, `features/F06-web-server-lan.md` |
| `POST` | `/api/v1/workspaces/:id/list-dir` | `list_dir` | `domains/workspace.md`, `features/F06-web-server-lan.md` |
| `GET` | `/api/v1/workspaces/:id/sessions` | `list_sessions` | `domains/session.md`, `features/F06-web-server-lan.md` |
| `POST` | `/api/v1/workspaces/:id/sessions` | `create_session` | `domains/session.md`, `features/F06-web-server-lan.md` |
| `GET` | `/api/v1/sessions/:id/history` | `get_history` | `domains/session.md`, `features/F06-web-server-lan.md` |
| `POST` | `/api/v1/sessions/:id/messages` | `send` | `domains/agent-loop.md`, `features/F06-web-server-lan.md` |
| `POST` | `/api/v1/sessions/:id/abort` | `abort` | `domains/agent-loop.md`, `features/F06-web-server-lan.md` |
| `GET` | `/api/v1/sessions/:id/active-agent` | `get_active_agent` | `agents.md`, `features/F06-web-server-lan.md` |
| `POST` | `/api/v1/sessions/:id/active-agent` | `set_active_agent` | `agents.md`, `features/F06-web-server-lan.md` |
| `GET` | `/api/v1/agents` | `list_agents` | `agents.md`, `features/F06-web-server-lan.md` |
| `GET` | `/api/v1/agents/:id` | `get_agent` | `agents.md`, `features/F06-web-server-lan.md` |
| `GET` | `/api/v1/config/global` | `config_get_global` | `domains/config.md`, `features/F06-web-server-lan.md` |
| `PATCH` | `/api/v1/config/global` | `config_update_global` | `domains/config.md`, `features/F06-web-server-lan.md` |
| `GET` | `/api/v1/config/workspaces/:id` | `config_get_workspace` | `domains/config.md`, `features/F06-web-server-lan.md` |
| `PATCH` | `/api/v1/config/workspaces/:id` | `config_update_workspace` | `domains/config.md`, `features/F06-web-server-lan.md` |
| `POST` | `/api/v1/providers/test-connection` | `providers_test_connection` | `domains/providers.md`, `features/F06-web-server-lan.md` |
| `POST` | `/api/v1/secrets/:provider_id` | `set_secret` | `domains/config.md`, `features/F06-web-server-lan.md` |
| `DELETE` | `/api/v1/secrets/:provider_id` | `delete_secret` | `domains/config.md`, `features/F06-web-server-lan.md` |
| `GET` | `/api/v1/secrets/providers` | `list_providers` | `domains/config.md`, `features/F06-web-server-lan.md` |
| `GET` | `/api/v1/permissions/matrix` | `get_matrix` | `domains/permissions.md`, `features/F06-web-server-lan.md` |
| `POST` | `/api/v1/permissions/default` | `set_default` | `domains/permissions.md`, `features/F06-web-server-lan.md` |
| `GET` | `/api/v1/permissions/requests` | `list` | `domains/permissions.md`, `features/F06-web-server-lan.md` |
| `POST` | `/api/v1/permissions/requests/:id/respond` | `respond` | `domains/permissions.md`, `features/F06-web-server-lan.md` |

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
- [ ] AC5: el HTTP server exige bearer token cuando
  `[server].require_token = true` y `bind != 127.0.0.1`; cuando
  `require_token = false`, emite un único warning al arrancar y sirve
  sin auth (MVP dogfooding).
- [ ] AC6: las rutas HTTP están versionadas (`/api/v1/...`).
- [ ] AC7: el spec de cada command nuevo o modificado referencia este
  documento (`ipc.md`).
