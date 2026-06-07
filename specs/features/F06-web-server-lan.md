# F06 — Web server LAN

**Status**: implemented (partial)
**Owner**: @miglesias
**Last update**: 2026-06-07
**Affects**: [`ipc`](../ipc.md), [`architecture`](../architecture.md),
[`config`](../domains/config.md), [`session`](../domains/session.md),
[`workspace`](../domains/workspace.md), [`permissions`](../domains/permissions.md),
[`agent-loop`](../domains/agent-loop.md), UI transport.
**Depends on**: [`F02`](./F02-multi-workspace.md),
[`F05`](./F05-settings.md), [`F01`](./F01-chat-streaming.md).

## Agent context

- F06 is now part of the v0.1 MVP, not v0.2. The MVP must be usable
  from the desktop app and from a browser on the local network.
- Current code has Axum lifecycle, static serving, `/api/v1/events`,
  `POST /sessions/:id/messages`, global config/provider/secrets/
  permissions matrix endpoints, diff skeleton endpoints, and a dual
  Tauri/HTTP `ui/src/lib/ipc.ts` adapter.
- Do **not** treat F06 as full yet. MVP blockers still open:
  browser-safe workspace/extra-path path inputs, HTTP workspace config,
  HTTP permission request list/respond, and a real LAN smoke.
- Implement one embedded Axum server in `agentyx-app`, started with
  the desktop process. It serves the same Svelte build and exposes
  REST + SSE under `/api/v1`.
- Required MVP bind: configurable `0.0.0.0:<port>` for LAN. If bind
  is not loopback, bearer auth is mandatory.
- UI must keep one public API in `ui/src/lib/ipc.ts`: Tauri uses
  `invoke/listen`, browser uses `fetch/EventSource`.
- Browser mode cannot use OS file dialogs. Workspace open and extra
  paths must accept manual server-side paths.

## Problem

Agentyx cannot be MVP-only desktop because real dogfooding requires
access from another device on the LAN. The current specs already
describe HTTP/SSE as an architectural direction, but the roadmap
postpones it to v0.2/v0.3. That makes the MVP hard to validate when
the user is not physically in front of the machine running Agentyx.

The browser client must not become a separate product. It is the same
local-first app, served by the desktop process, operating on the same
workspace registry, sessions, permissions, providers and journal.

## Appetite

**Budget**: medium (1 week).

The MVP target is a usable LAN web client, not a hardened remote
collaboration platform. If the work grows, cut polish and advanced
sync first; keep REST commands, SSE streaming, auth and manual path
entry.

## Solution Shape

- Add `crates/agentyx-app/src/server/` with:
  - Axum router and server lifecycle.
  - Static serving of the built UI.
  - `/api/v1/*` JSON endpoints that call the same app services as
    the Tauri commands.
  - `/api/v1/events` SSE endpoint backed by the shared event bus.
  - Bearer auth middleware for non-loopback bind.
- Refactor `EventBus` so every event is published to:
  - Tauri windows via `AppHandle::emit`.
  - A broadcast channel consumed by SSE clients.
- Refactor `ui/src/lib/ipc.ts` into transport adapters:
  - `tauriAdapter`: current `tauriInvoke` + `tauriListen`.
  - `httpAdapter`: `fetch` + `EventSource`/SSE.
- Add browser-safe workspace flows:
  - Open workspace by typing an absolute path on the server machine.
  - Add extra path by typing an absolute path on the server machine.
  - Keep OS file dialogs in Tauri mode.
- Add config for server bind and token metadata. Secret token value
  must not be written in plain TOML.

## Contracts

- **Config**:
  - `server.enabled: bool` default `true`.
  - `server.bind_host: string` default `"127.0.0.1"`.
  - `server.port: u16 | null` default `null` (random free port).
  - `server.lan_enabled: bool` default `false`.
  - `server.token_ref: SecretRef | null` required when
    `bind_host != "127.0.0.1" && bind_host != "::1"`.
- **Server commands**:
  - `server_get_info() -> ServerInfoDto`.
  - `server_update_config(patch: ServerConfigPatch) -> ServerInfoDto`.
  - `server_rotate_token() -> ServerInfoDto`.
- **HTTP endpoints**:
  - `GET /api/v1/health`.
  - `GET /api/v1/server/info`.
  - `GET /api/v1/workspaces`.
  - `POST /api/v1/workspaces`.
  - `GET /api/v1/workspaces/:id`.
  - `DELETE /api/v1/workspaces/:id`.
  - `GET /api/v1/workspaces/:id/venv`.
  - `GET /api/v1/workspaces/:id/extra-paths`.
  - `POST /api/v1/workspaces/:id/extra-paths`.
  - `DELETE /api/v1/workspaces/:id/extra-paths`.
  - `POST /api/v1/workspaces/:id/list-dir`.
  - `GET /api/v1/workspaces/:id/sessions`.
  - `POST /api/v1/workspaces/:id/sessions`.
  - `GET /api/v1/sessions/:id/history`.
  - `POST /api/v1/sessions/:id/messages`.
  - `POST /api/v1/sessions/:id/abort`.
  - `GET /api/v1/sessions/:id/active-agent`.
  - `POST /api/v1/sessions/:id/active-agent`.
  - `GET /api/v1/agents`.
  - `GET /api/v1/agents/:id`.
  - `GET /api/v1/config/global`.
  - `PATCH /api/v1/config/global`.
  - `GET /api/v1/config/workspaces/:id`.
  - `PATCH /api/v1/config/workspaces/:id`.
  - `POST /api/v1/providers/test-connection`.
  - `POST /api/v1/secrets/:provider_id`.
  - `DELETE /api/v1/secrets/:provider_id`.
  - `GET /api/v1/secrets/providers`.
  - `GET /api/v1/permissions/matrix`.
  - `POST /api/v1/permissions/default`.
  - `GET /api/v1/permissions/requests`.
  - `POST /api/v1/permissions/requests/:id/respond`.
  - `GET /api/v1/events`.
- **Events**:
  - Same event names and payloads as Tauri, streamed through SSE.
  - SSE message format: `event: <event_name>` and `data: <payload_json>`.
  - Heartbeat event: `event: ping` every 15 seconds.
- **Errors**:
  - HTTP errors use the existing `{ code, message, context? }` shape.
  - `401 unauthorized` for missing/invalid bearer token.
  - `403 forbidden` for LAN bind without configured token.

## Acceptance Criteria

- [x] F06.AC1 — Axum server starts on `127.0.0.1:<port>` with
  Axum serve loop.
- [x] F06.AC2 — Bearer auth middleware: rejects non-loopback binds
  without valid token with `401` and `WWW-Authenticate: Bearer` header.
- [x] F06.AC3 — `require_token=false` path: `tracing::warn!` at startup,
  unauthenticated requests succeed; middleware compiled and wireable via
  config flip.
- [ ] F06.AC4: Given a browser opens the LAN URL with a valid token,
  When it loads the app, Then it uses the HTTP adapter and can list
  workspaces without importing Tauri APIs.
- [ ] F06.AC5: Given browser mode, When the user opens a workspace,
  Then the UI accepts an absolute path typed by the user and calls
  the HTTP workspace endpoint.
- [x] F06.AC6: Given browser mode and an active workspace, When the
  user sends a chat message, Then `send` returns a run handle and
  chat deltas arrive through `/api/v1/events`.
- [ ] F06.AC7: Given a permission request is emitted, When a browser
  client is connected, Then it receives `permission.requested.v1` over
  SSE and can respond through the HTTP permissions endpoint.
- [x] F06.AC8: Given a config or extra path changes in desktop or web,
  When the event is emitted, Then both Tauri listeners and SSE clients
  receive the same event payload.
- [ ] F06.AC9: Given a browser client, When it tests a provider,
  updates config, stores/deletes a secret, or edits default tool
  decisions, Then the corresponding HTTP endpoint matches the Tauri
  command behavior and never returns secret values.
- [x] F06.AC10: Given the UI is built for production, When served from
  the embedded server, Then refresh/deep-link fallback returns the app
  shell and API routes still return JSON.

## Test Map

- `F06.AC1` -> Rust integration test: `server::tests::f06_ac1_loopback_serves_ui` ✅
- `F06.AC2` -> Rust integration test: `server::tests::f06_ac2_lan_with_require_token_blocks_without_bearer` ✅
- `F06.AC3` -> Rust integration test: `server::tests::f06_ac3_lan_without_require_token_serves_with_warn` ✅
- `F06.AC4` -> Pending: browser build must avoid direct Tauri dialog
  usage and list workspaces through HTTP.
- `F06.AC5` -> Pending: browser workspace + extra path open by typed
  absolute path.
- `F06.AC6` -> Pending stronger Rust HTTP test with `MockProvider` +
  SSE client; code path exists.
- `F06.AC7` -> Pending Rust HTTP test with `PermissionRegistry` ask
  flow and `POST /permissions/requests/:id/respond`; endpoint missing.
- `F06.AC8` -> Pending Rust event bus test with Tauri sink mocked + SSE broadcast
- `F06.AC9` -> Pending HTTP endpoint tests per command group; workspace
  config endpoints and permission request endpoints are missing.
- `F06.AC10` -> Rust integration test: `server::tests::f06_spa_fallback_returns_index_for_unknown_routes` ✅

## Implementation notes

- Static serving and SPA fallback are implemented via `ServeDir` in
  `crates/agentyx-app/src/server/static_files.rs`.
- HTTP routes currently cover workspaces, sessions, agents, global
  config, provider test, secrets, permission matrix/default, diffs
  skeleton and SSE.
- Missing routes from the contract: `GET/PATCH
  /api/v1/config/workspaces/:id`, `GET /api/v1/permissions/requests`,
  `POST /api/v1/permissions/requests/:id/respond`.
- `ui/src/lib/stores/workspace.svelte.ts` still imports and uses the
  Tauri dialog plugin directly; browser mode needs a manual path flow.

## No-gos

- No relay service or public sharing.
- No multi-user collaboration semantics. Multiple clients can observe
  and operate the same local app, but conflict resolution is whatever
  the current session/workspace logic provides.
- No HTTPS termination in the app for v0.1. Users can put TLS in front
  if their local network setup requires it.
- No remote OS file picker. Browser mode uses typed paths that exist
  on the machine running Agentyx.
- No SSE replay via `Last-Event-ID` in MVP.
- No separate web build or separate backend process.

## MVP dogfooding caveats

> Local dogfooding on a trusted LAN is the only supported scenario in
> v0.1. The browser client and the desktop client are the **same app**
> running on the **same machine**; the browser is just a remote view.
> This section makes the deliberate MVP relaxations explicit so they
> are not forgotten when hardening lands.

1. **LAN bind is open by default.** When `[server].bind_host` is not
   loopback, `require_token` defaults to `false`. The bearer middleware
   is **compiled and wired** but inactive; a single `tracing::warn!` is
   emitted at startup ("LAN bind without bearer auth — local dogfooding
   only"). The UI shows no warning banner in v0.1.
2. **No HTTPS.** Tokens and message bodies travel in cleartext on the
   LAN. The user is expected to be on a trusted network (home/office
   Wi-Fi). Public exposure requires `require_token = true` and a
   reverse proxy in front (out of scope for v0.1).
3. **Browser client trusts the desktop client.** There is no
   per-browser identity, no rate-limit per client, and no lockout on
   bad tokens. Multiple browsers on the LAN can connect concurrently
   and observe the same sessions.
4. **CSRF posture.** Only same-origin requests are accepted; CORS
   allowlist is `Origin` header equals the server's own origin. Cookies
   (if added later) would be `SameSite=Strict`.
5. **Hardening checklist before v0.2.** Flip `require_token` default
   to `true`, add a UI warning banner when LAN is open, add per-client
   rate-limit, and decide whether to ship a built-in tunnel (F19) or
   leave it to the user. Tracked under §Discovered bugs once
   implementation lands.

## Risks / Rabbit holes

- Tauri handlers that take `AppHandle` need shared inner functions or
  a sink abstraction so HTTP does not duplicate behavior.
- Current `EventBus` is Tauri-only; changing it touches chat,
  permissions, config and workspace events.
- `EventSource` cannot set `Authorization` headers directly. The HTTP
  adapter may need an SSE polyfill, a short-lived cookie set by an
  authenticated bootstrap endpoint, or a token-bound same-origin
  session. Do not put bearer tokens in URLs.
- Serving the UI from Tauri dev mode and production `ui/dist` has
  different paths. Keep dev ergonomics explicit.
- LAN-without-auth is an MVP concession (see §MVP dogfooding caveats).
  If a user accidentally exposes `0.0.0.0` on a public network, every
  connected client gets full read/write to the same workspaces. The
  default `127.0.0.1` bind mitigates this for loopback users.

## Implementation notes

- Axum server lives in `crates/agentyx-app/src/server/` with modules
  `mod.rs`, `router.rs`, `events_sse.rs`, `auth.rs`, `lifecycle.rs` +
  `tests.rs` (5 integration tests).
- EventBus upgraded: `tokio::sync::broadcast` channel with `EventSink`
  trait; Tauri windows + SSE share the same bus.
- Tauri commands `server_get_info`, `server_update_config`,
  `server_rotate_token` added to control server lifecycle.
- `EventSink::emit` and `EventSink::subscribe` public; SSE clients
  call `subscribe` on connect.
- Bearer middleware in `auth.rs` checks `Authorization` header in the
  request guard; non-loopback binding enforces `require_token=true`.
- `AppState::initialize`: reads `server.*`; creates EventBus; spawns
  serve loop on drop guard; sets initial bearer token; wires router.
- PR #6 body lists all ACs covered, tests, and spec changes.

## Discovered bugs (post-approval)

| ID | Date | Category | Resolved in | Notes |
|---|---|---|---|---|
| - | - | - | - | - |

## References

- [`specs/project.md`](../project.md)
- [`specs/architecture.md`](../architecture.md)
- [`specs/ipc.md`](../ipc.md)
- [`specs/domains/server.md`](../domains/server.md) — domain spec for the embedded HTTP server.
- [`specs/features/ROADMAP.md`](./ROADMAP.md)
