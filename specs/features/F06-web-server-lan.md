# F06 — Web server LAN

**Status**: implemented (partial)
**Owner**: @miglesias
**Last update**: 2026-06-08
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
  Tauri/HTTP `ui/src/lib/ipc.ts` adapter. The desktop binary accepts
  `--lan` for dogfooding. The canonical headless server command is
  `agentix serve`; `agentyx-web` remains as a legacy/internal runner
  used by browser smoke tests.
- Do **not** treat F06 as full yet. All automatable ACs are covered,
  including chat→SSE (F06.AC6), event bus SSE parity (F06.AC8),
  browser path prompts (F06.AC4/AC5), HTTP permission requests
  (F06.AC7), workspace config HTTP (F06.AC9) and SPA fallback
  (F06.AC10). Real LAN access from a second device has been verified
  after fixing static file path resolution and no-cache app-shell
  headers. Remaining manual smoke: PathPromptDialog UX, chat→SSE in a
  real browser tab, permission prompt response, and Settings over HTTP.
- Implement one embedded Axum server in `agentyx-app`, started with
  the desktop process. It serves the same Svelte build and exposes
  REST + SSE under `/api/v1`.
- Required MVP bind: configurable `0.0.0.0:<port>` for LAN. Bearer auth
  is enforced when `server.require_token = true`; `false` is an MVP
  dogfooding concession that emits a startup warning.
- UI must keep one public API in `ui/src/lib/ipc.ts`: Tauri uses
  `invoke/listen`, browser uses `fetch/EventSource`.
- Browser mode cannot use OS file dialogs. Workspace open and extra
  paths must accept manual server-side paths. The
  `PathPromptDialog` component owns this UX; the workspace store
  routes through it via `isBrowserMode()` from `lib/ipc.ts`.
- The LAN web client must be usable from a phone during dogfooding:
  workspace navigation collapses into a drawer, workspace file/chat
  panes stack, and prompts/settings avoid desktop-only fixed widths.

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
  - Bearer auth middleware for non-loopback bind when enabled.
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
  - `server.require_token: bool` default `false` in v0.1 dogfooding.
  - `server.token_ref: SecretRef | null` required when
    `server.require_token = true`.
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
  - `403 forbidden` for `require_token = true` without configured token.

## Acceptance Criteria

- [x] F06.AC1 — Axum server starts on `127.0.0.1:<port>` with
  Axum serve loop.
- [x] F06.AC2 — Bearer auth middleware: rejects non-loopback binds
  without valid token with `401` and `WWW-Authenticate: Bearer` header.
- [x] F06.AC3 — `require_token=false` path: `tracing::warn!` at startup,
  unauthenticated requests succeed; middleware compiled and wireable via
  config flip.
- [x] F06.AC4: Given a browser opens the LAN URL with a valid token,
  When it loads the app, Then it uses the HTTP adapter and can list
  workspaces without importing Tauri APIs.
- [x] F06.AC5: Given browser mode, When the user opens a workspace,
  Then the UI accepts an absolute path typed by the user and calls
  the HTTP workspace endpoint.
- [x] F06.AC6: Given browser mode and an active workspace, When the
  user sends a chat message, Then `send` returns a run handle and
  chat deltas arrive through `/api/v1/events`.
- [x] F06.AC7: Given a permission request is emitted, When a browser
  client is connected, Then it receives `permission.requested.v1` over
  SSE and can respond through the HTTP permissions endpoint.
- [x] F06.AC8: Given a config or extra path changes in desktop or web,
  When the event is emitted, Then both Tauri listeners and SSE clients
  receive the same event payload.
- [x] F06.AC9: Given a browser client, When it tests a provider,
  updates config, stores/deletes a secret, or edits default tool
  decisions, Then the corresponding HTTP endpoint matches the Tauri
  command behavior and never returns secret values.
- [x] F06.AC10: Given the UI is built for production, When served from
  the embedded server, Then refresh/deep-link fallback returns the app
  shell and API routes still return JSON.
- [x] F06.AC11: Given the LAN web client is opened on a phone-width
  viewport, When the user navigates workspaces, opens settings, reads
  files, chats, or sees path/permission prompts, Then controls remain
  reachable without horizontal page scrolling.

## Test Map

- `F06.AC1` -> Rust integration test: `server::tests::f06_ac1_loopback_serves_ui` ✅
- `F06.AC2` -> Rust integration test: `server::tests::f06_ac2_lan_with_require_token_blocks_without_bearer` ✅
- `F06.AC3` -> Rust integration test: `server::tests::f06_ac3_lan_without_require_token_serves_with_warn` ✅
- `F06.AC4` -> Rust integration test: `server::tests::f06_ac4_http_open_workspace_via_typed_path` ✅
  - + UI store test: `ac5_browser_open_via_typed_path` (`stores/workspace.svelte.test.ts`)
- `F06.AC5` -> Rust integration test: `server::tests::f06_ac5_http_add_extra_path_via_typed_path` ✅
  - + UI store tests: `ac5_browser_add_extra_via_typed_path`,
    `ac5_browser_open_via_typed_path`,
    `openViaDialog_returns_null_and_does_not_call_IPC_when_the_user_cancels`,
    `addExtraPathViaDialog_returns_null_when_no_workspace_is_selected`
- `F06.AC6` -> Rust integration test:
  `server::tests::f06_ac6_chat_send_publishes_events_to_sse` ✅
  (injects a `MockProvider` under `ollama`, subscribes an SSE
  client, POSTs a message, and verifies the SSE body contains
  `chat.run.started.v1`, `chat.content.delta.v1`, and
  `chat.run.finished.v1` with the mock's content).
- `F06.AC8` -> Rust integration test:
  `server::tests::f06_ac8_sse_delivers_published_event` ✅
  (publishes a `chat.content.delta.v1` event to the bus and
  verifies the SSE client receives it with the expected name
  and payload). The Tauri↔SSE parity is implied: Tauri windows
  receive events through the same `EventBus::publish_typed`
  that SSE subscribes to.
- `F06.AC7` -> Rust integration test:
  `server::tests::f06_ac7_http_permission_request_list_and_respond` ✅
  (registers a real `PermissionRequest` in the in-memory
  registry, lists it over HTTP, responds with `deny`, and
  verifies the oneshot channel delivered the decision).
- `F06.AC8` -> Rust integration test:
  `server::tests::f06_ac8_sse_delivers_published_event` ✅
  (publishes a `chat.content.delta.v1` event to the bus and
  verifies the SSE client receives it with the expected name
  and payload). The Tauri↔SSE parity is implied: Tauri windows
  receive events through the same `EventBus::publish_typed`
  that SSE subscribes to.
- `F06.AC9` -> Rust integration test:
  `server::tests::f06_ac9_http_workspace_config_get_and_patch` ✅
  (GET returns the resolved DTO without secrets; PATCH persists
  new `ignorePatterns`; PATCH on an unknown id returns 404
  thanks to the `WorkspaceService` validation in the HTTP
  handler). Other F06.AC9 endpoints (global config, providers,
  secrets, permission matrix/default) are covered by the
  existing `f06_http_*` tests from PR #27.
- `F06.AC10` -> Rust integration test: `server::tests::f06_spa_fallback_returns_index_for_unknown_routes` ✅
- `F06.AC11` -> Manual browser smoke at phone viewport after UI build:
  root route loads, sidebar drawer opens/closes, workspace file/chat
  panes stack, composer remains visible, settings tabs scroll
  horizontally, and path/permission dialogs fit the viewport.
- Browser smoke coverage -> Playwright E2E:
  `ui/e2e/smoke.spec.ts` covers F06.AC4/F06.AC5/F06.AC6/F06.AC7/
  F06.AC9 workspace settings load/F06.AC10 against `agentyx-web`.

## Implementation notes

- Static serving and SPA fallback are implemented via `ServeDir` in
  `crates/agentyx-app/src/server/static_files.rs`.
- Static file path resolution walks ancestors from both the process CWD
  and current binary so dev launches from repo root, `crates/`, or
  `target/debug` find the same `ui/dist`. HTML app-shell responses add
  `Cache-Control: no-store, no-cache, must-revalidate` so a blank/stale
  cached `/` cannot survive after a bad LAN smoke attempt.
- HTTP routes currently cover workspaces, sessions, agents, global
  config, provider test, secrets, permission matrix/default, diffs
  skeleton and SSE.
- Browser-safe path flow: `ui/src/lib/stores/path-prompt.svelte.ts`
  exposes a single-request queue rendered by
  `ui/src/lib/components/PathPromptDialog.svelte` (mounted via
  `PathPromptHost.svelte` in `app.svelte`). The workspace store
  routes `openViaDialog` / `addExtraPathViaDialog` through it when
  `isBrowserMode()` returns `true`; in Tauri mode the
  `@tauri-apps/plugin-dialog` is loaded **dynamically** (no static
  import remains) so the browser bundle no longer pulls the plugin.
- HTTP `open_workspace` and `add_extra_path` already accept arbitrary
  absolute paths in their JSON bodies, so the browser path prompt
  just forwards the typed value.
- Missing routes from the contract: none — every route in
  §Contracts is now wired.
- Web smoke: `scripts/web-smoke.sh` automates the curl-based
  part of the F06/F05 LAN verification (health, workspaces,
  agents, config GET/PATCH, permission matrix, permission
  requests, SPA fallback). The browser-only checks
  (PathPromptDialog UX, real SSE in a browser tab, LAN
  access from a second device) remain manual.
- Playwright smoke lives in `ui/e2e/smoke.spec.ts` and starts the
  headless `agentyx-web` runner via `ui/playwright.config.ts`.
- Mobile web layout now uses a drawer for workspace navigation and
  phone-width stacking for workspace files/chat, settings, composer,
  and path/permission dialogs.

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
- Headless LAN startup is exposed as `agentix serve`, defaulting to
  `0.0.0.0:18765` with `require_token = false` for MVP dogfooding.
  Optional overrides: `--host`, `--port`, `--require-token`.
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
- PR `feat/f06-browser-workspace-paths` (this work) closes
  F06.AC4/AC5 by introducing
  `ui/src/lib/stores/path-prompt.svelte.ts` +
  `ui/src/lib/components/PathPromptDialog.svelte` (mounted by
  `PathPromptHost.svelte` in `app.svelte`), removing the static
  `@tauri-apps/plugin-dialog` import from the workspace store, and
  adding Rust integration tests
  `f06_ac4_http_open_workspace_via_typed_path` and
  `f06_ac5_http_add_extra_path_via_typed_path`.
- PR `feat/f06-http-config-permissions-gap` (this work) closes
  F06.AC7 + the workspace-config half of F06.AC9 by adding:
  - `crates/agentyx-app/src/commands/config.rs`:
    `config_get_global_impl`, `config_update_global_impl`,
    `config_get_workspace_impl`, `config_update_workspace_impl`
    (extracted from the Tauri commands so HTTP and Tauri share
    the same code path; the Tauri commands keep the
    `AppHandle::emit` event emission, the HTTP handlers use
    `EventBus::publish_typed` for SSE).
  - `crates/agentyx-app/src/commands/permissions.rs`:
    `list_impl`, `respond_impl`.
  - HTTP routes in `router.rs` + handlers in `handlers.rs`:
    `GET /api/v1/config/workspaces/:id`,
    `PATCH /api/v1/config/workspaces/:id`,
    `GET /api/v1/permissions/requests`,
    `POST /api/v1/permissions/requests/:id/respond`.
  - Browser IPC adapter: `ui/src/lib/ipc.ts` now routes
    `config_get_workspace`, `config_update_workspace`,
    `permissions.list`, and `permissions.respond` through the
    new HTTP endpoints (no more "Unknown command in browser
    mode").
  - Rust integration tests
    `f06_ac7_http_permission_request_list_and_respond` and
    `f06_ac9_http_workspace_config_get_and_patch`.

## Discovered bugs (post-approval)

| ID | Date | Category | Resolved in | Notes |
|---|---|---|---|---|
| F06-BUG-001 | 2026-06-08 | B. Implementation deviation | PR actual | Launching the web runner from `crates/` made `ui_dist_path()` prefer a bad relative path, so `/` and assets could return empty bodies while `/api/v1/health` was OK. Fix: resolve `ui/dist` by walking ancestors from cwd/current exe and mark HTML app-shell responses as no-cache. |

## References

- [`specs/project.md`](../project.md)
- [`specs/architecture.md`](../architecture.md)
- [`specs/ipc.md`](../ipc.md)
- [`specs/domains/server.md`](../domains/server.md) — domain spec for the embedded HTTP server.
- [`specs/features/ROADMAP.md`](./ROADMAP.md)
