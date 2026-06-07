# Server

**Status**: ready
**Owner**: @miglesias
**Last update**: 2026-06-07
**Affects**: [`ipc`](../ipc.md) (transporte HTTP/SSE),
[`config`](../domains/config.md) (sección `[server]`),
[`event-bus`](#state) (broadcast para SSE), `ui` (httpAdapter).
**Required by**: [`features/F06-web-server-lan.md`](../features/F06-web-server-lan.md),
[`features/F01-chat-streaming.md`](../features/F01-chat-streaming.md) (eventos por SSE),
[`features/F02-multi-workspace.md`](../features/F02-multi-workspace.md) (REST workspaces),
[`features/F05-settings.md`](../features/F05-settings.md) (REST config/secrets).

> Servidor HTTP embebido en el binario desktop. Sirve la UI Svelte
> (mismo `ui/dist/` que Tauri webview) y expone la API REST + SSE
> definida en [`../ipc.md`](../ipc.md) §4. Es la pieza que hace
> Agentyx usable desde un navegador en la LAN sin un proceso backend
> separado.

## Agent context

- Leer primero este bloque, `State`, `Operations`, `Edge cases` y
  `Acceptance criteria`. La sección UX y los endpoints concretos viven
  en [`F06-web-server-lan.md`](../features/F06-web-server-lan.md) y
  en [`../ipc.md`](../ipc.md) §4; este spec define el **cómo** del
  server, no el **qué** de cada endpoint.
- Bloqueante para F06. El MVP (v0.1) requiere desktop y navegador LAN
  funcionales a la vez.
- Reglas no negociables:
  - Bind por defecto `127.0.0.1` (loopback, sin auth).
  - Bind `0.0.0.0` solo si `[server].lan_enabled = true`; auth
    opcional detrás de `[server].require_token` (default `false` en
    MVP dogfooding).
  - Sin secretos en logs, headers de respuesta, ni payloads SSE.
  - Mismo handler `*_impl` debe ser invocable desde Tauri command
    **y** desde HTTP handler — cero duplicación de lógica de negocio.
  - `agentyx-core` no depende de `axum` ni de nada de HTTP; el server
    vive en `agentyx-app`.

## Goal

1. Servir la UI Svelte construida (`ui/dist/`) bajo `/` cuando se
   accede desde navegador, y los assets estáticos requeridos.
2. Exponer los endpoints REST + SSE definidos en
   [`../ipc.md`](../ipc.md) §4, **idénticos en shape** a los Tauri
   commands que ya existen.
3. Multiplexar el `EventBus` interno hacia todos los sinks (Tauri
   windows vía `AppHandle::emit`, SSE clients vía `tokio::sync::broadcast`).
4. Aplicar middleware transversal: bearer opcional, CORS restrictivo,
   CSP estricta, rate-limit básico, timeout de request.
5. Lifecycle limpio: arranca con el proceso desktop, para con él,
   expone un comando `server_get_info` para que la UI muestre
   URL/puerto/token.

## Non-goals

- ❌ HTTPS en el server (lo deja al usuario con un reverse proxy).
- ❌ Multi-user / RBAC. v1 es single-user.
- ❌ Persistencia de mensajes del server; el journal de sesiones
  sigue siendo el de SQLite.
- ❌ Replay SSE por `Last-Event-ID` (futuro, v1.x).
- ❌ Server-side rendering de la UI (es Svelte estático).
- ❌ Tunnels WAN (cloudflared, ngrok). Eso es F19, v0.3.
- ❌ Custom agents editables desde el navegador (igual que desktop,
  v1.x).

## Glossary

Términos locales (los globales están en [`../glossary.md`](../glossary.md)):

- **Embedded server**: el proceso `axum` que corre dentro del
  binario `agentyx-app`. Comparte `AppState` con el resto del proceso.
- **Transport**: en `ui/src/lib/ipc.ts`, el adapter que traduce
  `invoke/listen` a Tauri nativo o a HTTP+SSE. Decidido en bootstrap
  (presencia de `window.__TAURI_INTERNALS__`).
- **Sink**: consumidor de `EventBus`. v0.1 tiene `TauriSink` (vía
  `AppHandle::emit`) y `SseSink` (vía `tokio::sync::broadcast`).
- **Bearer (opcional)**: cuando `[server].require_token = true`, todas
  las rutas `/api/v1/*` requieren `Authorization: Bearer <token>`. El
  token vive en keychain `agentyx` account `server`.
- **CSP**: política de seguridad que el server inyecta en la
  respuesta HTML de la UI estática. Producción: `default-src 'self'`,
  `script-src 'self'`, sin `'unsafe-inline'`.
- **Same-origin**: el navegador que sirve la UI desde el server
  embedded solo puede hacer fetch contra el mismo origen (host + port
  + scheme). CORS bloquea otros orígenes aunque tengan el token.

## State

| Dato | Ubicación | Quién lee | Quién escribe |
|---|---|---|---|
| `[server]` config | `~/.agentyx/config.toml` (sección nueva) | `Server::start` al arranque, `server_*` commands | `server_update_config` Tauri command |
| Token bearer | keychain `agentyx` account `server` | middleware `bearer_layer` | `server_rotate_token` Tauri command |
| `EventBus` (en memoria) | `AppState.events: Arc<EventBus>` | Tauri sink, SSE sink | chat loop, permissions, config updates, workspace events |
| `ServerState` (en memoria) | `AppState.server: Arc<RwLock<ServerInfo>>` con `bind_addr`, `mode` (loopback/lan), `started_at` | Tauri commands `server_*`, `/api/v1/server/info` | `Server::start`, `Server::stop` |
| `ui/dist/` | archivo estático en el bundle del binario | axum `ServeDir` / `ServeFile` | build (`bun run build`) |

> **Sin secretos en disco**: el token nunca aparece en
> `config.toml`. La sección `[server]` guarda solo el `SecretRef`
> apuntando al keychain (igual que las API keys de providers).

### `[server]` config TOML

```toml
[server]
enabled = true              # default true en v0.1
bind_host = "127.0.0.1"     # default loopback
port = 0                    # 0 = random free port al arranque
lan_enabled = false         # opt-in para abrir 0.0.0.0
require_token = false       # MVP default: dogfooding LAN sin auth
token_ref = "keychain:server"  # SecretRef; siempre keychain en v0.1
cors_allowed_origins = []   # default: solo el propio origen
```

> Migración: si el TOML de un usuario existente no tiene `[server]`,
> el loader crea los defaults arriba. Sin acción del usuario.

### `EventBus`

```rust
pub struct EventBus {
    tx: tokio::sync::broadcast::Sender<Event>,
    sinks: Vec<Box<dyn EventSink>>,
}

impl EventBus {
    pub fn publish(&self, event: Event);
    pub fn subscribe(&self) -> broadcast::Receiver<Event>;
    pub fn add_sink(&self, sink: Box<dyn EventSink>);
}

pub trait EventSink: Send + Sync {
    fn name(&self) -> &'static str;
    fn handle(&self, event: &Event) -> Result<(), AppError>;
}
```

Sinks registrados por `AppState::initialize`:

- `TauriSink` — si el proceso corre dentro de Tauri, despacha vía
  `AppHandle::emit(event_name, payload)`. **No-op** cuando el
  `AppHandle` no está disponible (p. ej. tests).
- `SseSink` — publica en el `broadcast::Sender` que alimenta
  `/api/v1/events`. Cada SSE client es un `broadcast::Receiver`
  independiente; los lentos no bloquean a los rápidos.

> **Cambio material vs estado actual**: el `EventBus` actual
> (commit `bebf4a7`) es Tauri-only. Este spec lo convierte en un
> pub/sub fan-out. Es el refactor de mayor riesgo de F06; ver
  `## Risks`.

## Operations

### Lifecycle

```rust
pub struct EmbeddedServer {
    handle: Option<JoinHandle<()>>,
    info: Arc<RwLock<ServerInfo>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl EmbeddedServer {
    pub fn start(state: Arc<AppState>, config: ServerConfig) -> Result<ServerInfo, AppError>;
    pub fn stop(&mut self) -> Result<(), AppError>;
    pub fn info(&self) -> ServerInfo;
}
```

- `start` se llama una sola vez desde `AppState::initialize` si
  `[server].enabled = true`. Si el puerto pedido está ocupado,
  retorna `conflict` y la app sigue sin server (no es fatal).
- `stop` se llama desde `Drop` de `AppState` y desde el handler de
  SIGINT/SIGTERM. Cancela el `JoinHandle` y cierra el listener.
- `info` expone `{ bindAddr, mode, port, startedAt, requireToken }`
  para que la UI lo muestre en `/settings`.

### Router (axum)

```rust
pub fn router(state: Arc<AppState>) -> axum::Router {
    use axum::{routing::get, routing::post, routing::patch, routing::delete, Router};

    let api = Router::new()
        // health + meta
        .route("/health", get(handlers::health))
        .route("/server/info", get(handlers::server_info))
        .route("/server/config", patch(handlers::server_update_config))
        .route("/server/token/rotate", post(handlers::server_rotate_token))
        // ... resto de endpoints según ipc.md §4.2 ...
        .route("/events", get(handlers::sse::sse_handler))
        .layer(middleware::bearer_layer(state.clone()))
        .layer(middleware::cors_layer(state.clone()))
        .layer(middleware::request_timeout(30s));

    Router::new()
        .nest("/api/v1", api)
        .fallback(static_files::serve_ui)  // SPA fallback a index.html
        .with_state(state)
}
```

- `static_files::serve_ui` sirve `ui/dist/index.html` para rutas que
  no son `/api/*` (deep-link fallback, AC10).
- Los `Route` de `/api/v1/*` solo devuelven JSON; nunca caen al
  fallback de estáticos.

### `*_impl` extraction

Patrón: cada Tauri command en `crates/agentyx-app/src/commands/<scope>.rs`
delega a un `pub(crate) async fn <command>_impl(state, args) -> Result<T, AppError>`
que **no** depende de `tauri::State`. Los HTTP handlers importan ese
mismo `<command>_impl` y lo llaman.

```rust
// commands/session.rs
#[tauri::command]
pub async fn create_session(
    state: tauri::State<'_, AppState>,
    workspace_id: WorkspaceId,
    agent_id: Option<AgentId>,
    title: Option<String>,
) -> Result<SessionSummaryDto, AppError> {
    create_session_impl(state.inner().clone(), workspace_id, agent_id, title).await
}

pub(crate) async fn create_session_impl(
    state: Arc<AppState>,
    workspace_id: WorkspaceId,
    agent_id: Option<AgentId>,
    title: Option<String>,
) -> Result<SessionSummaryDto, AppError> {
    // ... lógica de negocio ...
}
```

```rust
// server/handlers/session.rs
pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<SessionSummaryDto>, AppError> {
    commands::session::create_session_impl(state, req.workspace_id, req.agent_id, req.title)
        .await
        .map(Json)
}
```

> Regla: si un Tauri command toca `AppHandle` (p. ej. para emitir
> eventos), el `_impl` se lleva el `Arc<EventBus>` como dependencia
> y emite por el bus, no por `AppHandle` directo. El TauriSink
> re-emite al `AppHandle` desde el bus.

### Middleware: bearer opcional

```rust
pub fn bearer_layer(state: Arc<AppState>) -> impl Layer {
    axum::middleware::from_fn_with_state(state, |state, req, next| async move {
        if !state.server_info().require_token {
            // MVP: LAN sin auth, single warn al arrancar (no por request).
            return Ok(next.run(req).await);
        }
        let token = state.keychain_get("server")?;
        match req.headers().get("authorization").and_then(|v| v.to_str().ok()) {
            Some(h) if h == format!("Bearer {token}") => Ok(next.run(req).await),
            Some(_) => Err(StatusCode::UNAUTHORIZED),
            None => Err(StatusCode::UNAUTHORIZED),
        }
    })
}
```

### Middleware: CORS restrictivo

```rust
pub fn cors_layer(state: Arc<AppState>) -> tower_http::cors::CorsLayer {
    let origins = state.server_info().cors_allowed_origins.clone();
    tower_http::cors::CorsLayer::new()
        .allow_origin(allowlist(origins))  // propio origen + lista explícita
        .allow_methods([GET, POST, PATCH, DELETE])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE])
        .max_age(Duration::from_secs(3600))
}
```

### CSP en la respuesta HTML

```rust
// static_files.rs
const CSP: &str = "default-src 'self'; script-src 'self'; \
                   style-src 'self' 'unsafe-inline'; \
                   img-src 'self' data:; \
                   connect-src 'self'; \
                   base-uri 'self'; frame-ancestors 'none'";
```

Se inyecta vía `tower_http::set_header::SetResponseHeaderLayer` solo en
las respuestas `text/html` (no en `/api/*`).

### SSE handler

```rust
pub async fn sse_handler(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.events.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|evt| {
        evt.ok().map(|e| Ok(Event::default().event(e.name()).data(e.json_payload())))
    });
    Sse::new(stream).keep_alive(KeepAlive::interval(Duration::from_secs(15)))
}
```

- `event: ping` cada 15s (heartbeat del `KeepAlive`).
- Si el client se desconecta, el `Receiver` se descarta y el
  `broadcast` lo detecta; no hay leak.

### Rate-limit

- Default: 60 requests / 10s por IP (suficiente para UI manual; no
  para scraping).
- Implementación: `tower::limit::RateLimitLayer` con key derivada de
  la IP del peer (o del header `X-Forwarded-For` si hay un reverse
  proxy, futuro).
- SSE `/api/v1/events` está exenta del rate-limit (conexiones
  long-lived).

## Contracts

Las surface areas completas viven en [`../ipc.md`](../ipc.md) §4. Este
spec no las redefine; solo aclara particularidades del server:

- `GET /api/v1/health` → `200 { "status": "ok", "version": "0.1.0" }`
  sin auth incluso si `require_token = true` (health check no debe
  colisionar con monitorización).
- `GET /api/v1/server/info` → expone `bindAddr`, `mode`, `port`,
  `requireToken`, `startedAt`. **Nunca** expone el token.
- Errores HTTP: shape `{ code, message, context? }` con status code
  según tabla en `ipc.md` §4.4.
- SSE: `Event<T>` con `schema_version`, `type`, `payload`, `ts`
  (mismo shape que Tauri events, ver `ipc.md` §3.1).

## Edge cases

1. **Puerto ocupado**: `start` retorna `conflict` con
   `bind_addr` y `port` pedidos. La app continúa sin server. La UI
   muestra "Server no disponible" en Settings.
2. **Fallo al construir el `EventSink`**: el server arranca igual;
   el sink problemático se loguea como `error` y queda fuera del
   fan-out. El Tauri sink y el SSE sink son críticos; un fallo allí
   aborta el server.
3. **Token inválido o ausente en LAN con `require_token = true`**:
   `401` con `{ code: "unauthorized", message: "..." }` y
   `WWW-Authenticate: Bearer` en el header.
4. **Doble `start`**: idempotente si el server ya está corriendo con
   la misma config; `conflict` si el puerto o el bind difieren.
5. **Shutdown mientras hay SSE clients abiertos**: el server cierra
   el listener; los clients reciben un `event: bye` final antes del
   EOF. Los handlers en vuelo se cancelan cooperativamente.
6. **El `EventBus` no está inicializado (test runner)**: el TauriSink
   y el SseSink son no-ops con `tracing::debug!`. Los HTTP tests
   usan un `EventBus` de prueba.
7. **`bind_host = "::1"` (IPv6 loopback)**: tratado como loopback
   (sin auth). Mismo trato que `127.0.0.1`.
8. **`bind_host = "0.0.0.0"` con `lan_enabled = false` en el
   config**: el server falla al `start` con `invalid_input`
   ("lan_enabled must be true to bind 0.0.0.0"). El usuario debe
   activar la flag explícitamente.
9. **`require_token = true` sin token en keychain**: el server
   genera un token en `start` y lo guarda en keychain. El usuario
   lo ve en `server_info` (campo `tokenHint` con los últimos 4
   chars; el valor completo se muestra una sola vez tras
   `server_rotate_token`).
10. **Disco lleno o permisos rotos al leer `ui/dist/`**: el fallback
    estático retorna `500 internal` con un mensaje actionable.
    Los endpoints `/api/v1/*` siguen funcionando (no dependen de
    estáticos).
11. **Reverse proxy con `X-Forwarded-For`**: en v0.1 el server usa
    `peer_addr` del socket. El rate-limit no es per-client real
    hasta que se decida la postura de trust del proxy (futuro).
12. **CSP y assets de Svelte con `style-src 'unsafe-inline'`**:
    Svelte en v5 emite estilos inline. Workaround documentado
    en `Implementation notes`; el v0.1 acepta el `'unsafe-inline'`
    en `style-src` y endurece `script-src`. Se reevaluará cuando
    Svelte ofrezca build sin inline styles.

## Acceptance criteria

Cada AC → test con nombre derivado. Los tests viven en
`crates/agentyx-app/src/server/tests.rs` y
`crates/agentyx-app/src/server/<scope>_test.rs`.

- [ ] **AC1**: `Server::start` con `bind_host = "127.0.0.1"` y
  `port = 0` levanta un listener y lo expone en `ServerInfo.port`.
  **Test**: `ac1_start_loopback_binds_to_free_port`.
- [ ] **AC2**: `start` con `bind_host = "0.0.0.0"` y
  `lan_enabled = false` retorna `invalid_input` y no abre
  socket. **Test**: `ac2_start_lan_requires_lan_enabled`.
- [ ] **AC3**: `start` con `bind_host = "0.0.0.0"` y
  `lan_enabled = true`, `require_token = true`, sin token en
  keychain, genera uno y lo persiste. **Test**:
  `ac3_start_lan_with_require_token_generates_token`.
- [ ] **AC4**: middleware `bearer_layer` con `require_token = true`
  rechaza request sin header `Authorization` con `401`. **Test**:
  `ac4_bearer_missing_returns_401`.
- [ ] **AC5**: middleware `bearer_layer` con `require_token = true`
  acepta request con `Authorization: Bearer <correcto>` y rechaza
  con token incorrecto (`401`). **Test**:
  `ac5_bearer_correct_accepts_incorrect_rejects`.
- [ ] **AC6**: middleware `bearer_layer` con `require_token = false`
  deja pasar cualquier request y emite un único `tracing::warn!`
  al `start` (no por request). **Test**:
  `ac6_bearer_disabled_passes_and_warns_once`.
- [ ] **AC7**: `GET /api/v1/health` retorna `200` con el shape
  definido, sin requerir auth incluso si `require_token = true`.
  **Test**: `ac7_health_returns_ok_without_auth`.
- [ ] **AC8**: `GET /api/v1/server/info` **nunca** incluye el valor
  del token en el JSON, ni siquiera en campos derivados. **Test**:
  `ac8_server_info_omits_token_value`.
- [ ] **AC9**: `*_impl` extraction — un Tauri command y el handler
  HTTP equivalente llaman al mismo `<command>_impl`. **Test**:
  `ac9_command_impl_shared_between_tauri_and_http`.
- [ ] **AC10**: `EventBus::publish` con dos sinks registrados
  entrega el evento a ambos. **Test**:
  `ac10_event_bus_fanout_to_all_sinks`.
- [ ] **AC11**: SSE `/api/v1/events` entrega eventos publicados
  después de la suscripción con latencia < 200 ms en CI. **Test**:
  `ac11_sse_streams_published_events`.
- [ ] **AC12**: SSE desconecta limpiamente cuando el client cierra
  la conexión; el `broadcast::Receiver` se descarta sin leak
  (medido con un counter global). **Test**:
  `ac12_sse_disconnect_releases_receiver`.
- [ ] **AC13**: el server para limpio en `Drop` de `AppState`
  (cierra el listener, no deja zombies en `lsof`). **Test**:
  `ac13_server_drop_closes_listener`.
- [ ] **AC14**: el fallback estático sirve `index.html` para
  rutas no-`/api/*` (deep-link/refresh, AC10 de F06). **Test**:
  `ac14_static_fallback_serves_index_for_unknown_routes`.
- [ ] **AC15**: las respuestas `text/html` incluyen la CSP definida
  en el header `Content-Security-Policy`; las respuestas JSON
  no la incluyen. **Test**: `ac15_csp_set_only_on_html`.
- [ ] **AC16**: rate-limit aplica a `/api/v1/*` y NO a
  `/api/v1/events`. **Test**:
  `ac16_rate_limit_applies_to_api_excludes_sse`.
- [ ] **AC17**: `agentyx-core` **no** depende de `axum` ni de
  `tower` (verificado por `cargo metadata` en CI). **Test**:
  build-time assertion + grep en `Cargo.lock`.

## Discovered bugs (post-approval)

| ID | Date | Category | Resolved in | Notes |
|---|---|---|---|---|
| _ninguno aún_ | | | | |

## Open questions

- **Q1**: ¿`port = 0` (random) debe ser el default? → **Propuesta
  MVP**: sí, para evitar conflictos. La UI muestra el puerto
  asignado y el usuario puede fijarlo en Settings.
- **Q2**: ¿Soportamos `WebSocket` además de SSE? → **No en v0.1**.
  SSE cubre el caso de uso (UI pasiva). WebSocket se difiere a v1.x
  si surge un caso (p. ej. subagent colaborativo).
- **Q3**: ¿`require_token` debería poder activarse por cliente
  individual (p. ej. solo para Tauri), o es global? → **Global** en
  v0.1. Per-client se difiere a v0.2 si hay demanda.
- **Q4**: ¿El server debe poder deshabilitarse en runtime (sin
  reiniciar la app) para "apagarlo rápido"? → **No en v0.1**.
  Cambio de `enabled` requiere reinicio. `lan_enabled` y
  `require_token` son hot-reloadable (solo afectan al `bearer_layer`
  y al listener bind).
- **Q5**: ¿Compartimos el puerto entre el server embedded y un
  futuro Tauri webview inspector? → **No** en v0.1. Si en el futuro
  hace falta, se añade un sub-path `/__tauri__` con su propio
  middleware.

## References

- [`../ipc.md`](../ipc.md) — contratos completos de Tauri commands,
  HTTP endpoints, eventos, errores.
- [`../project.md`](../project.md) — visión de desktop-first y la
  decisión de abrir LAN en MVP.
- [`../architecture.md`](../architecture.md) — diagrama de cajas y
  flujo de datos (incluye el server embedded).
- [`../config.md`](../config.md) — `[server]` section del TOML.
- [`../features/F06-web-server-lan.md`](../features/F06-web-server-lan.md) —
  contratos de cara al usuario, ACs end-to-end.
- [`axum docs`](https://docs.rs/axum/) — `Router`, `Sse`, middleware.
- [`tokio::sync::broadcast`](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html) —
  fan-out del `EventBus`.
- AGENTS.md §9 (Seguridad), §15 (Checklist).
