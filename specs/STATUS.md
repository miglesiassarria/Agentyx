# Specs — Status board

> Vista por estado. Para índice maestro: [README.md](./README.md).
> Para roadmap de features: [features/ROADMAP.md](./features/ROADMAP.md).
> Para índice de ADRs: [adr/README.md](./adr/README.md).
>
> Última actualización: 2026-06-07 (orden de MVP tras auditoría local):
> `cargo test --workspace` pasa (276 tests: 71 app + 205 core), vitest
> pasa (44 tests), `npx tsc --noEmit` pasa. F06 sigue parcial pero
> F06.AC4/AC5 cerrados con `PathPromptDialog` +
> `pathPromptStore` y tests HTTP `f06_ac4_*` / `f06_ac5_*`; el bundle
> del browser ya no carga el plugin de diálogo de Tauri. Siguen
> abiertos para MVP web: HTTP `config/workspaces/:id` GET/PATCH,
> HTTP `permissions.requests` GET/POST, smoke LAN E2E. F04 y
> F-agents-ui siguen parciales (no cierran comportamiento completo).
>
> **Disciplina de status**: este archivo se actualiza en el mismo PR
> que cambia el estado real de cualquier pitch/spec o deja el board
> obsoleto. Ver `AGENTS.md` §17 Pitch-Driven SDD Lite (regla §17.5).
>
> Estados preferidos para trabajo nuevo: `proposed` → `ready` →
> `shipped` → `deprecated`. Los estados históricos `draft`, `review`,
> `approved` e `implemented` siguen aceptados para specs existentes.

## 🟡 Draft (en construcción)
- agents.md (modelo built-in existe; subagent real vía `@mention` sigue
  pendiente).
- domains/providers.md.
- domains/journal.md.
- features/F05-settings.md (UI y backend avanzados; quedan E2E/HTTP de
  workspace config y cierre de ACs abiertos).

## 🟢 Ready (AC + contratos listos, pendiente implementación)
- features/F-agents-ui.md (UI parcial en código: `AgentChip`,
  `AgentPickerMenu`, `AtMentionPopover`, shortcut cycle. Falta backend
  de `@mention`/child sessions y SessionTree para cerrar ACs).
- features/F04-file-diffs.md (infra parcial en código: `diff` domain,
  DTOs, `DiffsSidePanel`, endpoints de lista. Falta write/edit/apply
  tools reales y payload completo para cerrar ACs).

## 🔵 Review (pending approval)
- domains/agent-loop.md
- domains/workspace.md
- domains/permissions.md
- domains/tools.md

## 🟢 Approved (listo para implementar)
- project.md (revisado en PR 1)
- glossary.md (revisado en PR 1)
- architecture.md (revisado en PR 1)
- ipc.md (revisado en PR actual: §4.1 auth configurable con
  `require_token`, default `false` en MVP dogfooding)
- domains/session.md
- domains/storage.md
- domains/pty.md
- domains/server.md
- features/ROADMAP.md (revisado: v0.1 incluye F06 Web server LAN;
  F03 sigue en v0.1.x; F16 queda como navegador avanzado post-MVP)

## ADRs

- ADR-0001 a ADR-0006: accepted (sin cambios en PR 1-3).
- **ADR-0007** (nuevo, PR 3): modelo `root + extra_paths` por workspace.
- **ADR-0008** (nuevo, PR 3): scope de providers v1 (Ollama / Groq / Minimax).

## ✅ Implemented / Partially Implemented (código en main)
- **features/F06-web-server-lan.md** — `ready` → `implemented (partial)`. PRs:
   Axum skeleton (#26): `server` module con `router/events_sse/auth/lifecycle`,
   `AuthLayer` + `BearerGuard`, `axum_extra` typed extractor.
   EventBus upgrade: `tokio::sync::broadcast` + `EventSink` trait,
   Tauri windows + SSE comparten bus.
   Lifecycle en `AppState`: serve loop spawn, `ServerHandle` drop guard,
   initial bearer token generation.
   REST + SSE + ServeDir (#27): `POST /sessions/:id/messages`, `GET /events`
   con heartbeat, `GET/PATCH /config/global`, `POST /providers/test-connection`,
   `/secrets/*`, `/permissions/*`, `/diffs/*`, `ServeDir` con SPA fallback,
   `BroadcastEventSink`, browser IPC adapter dual-mode.
   F06.AC10 (#TBD): SPA fallback ahora retorna 200 (no 404) en deep-links;
   integración test verifica app shell + API JSON.
   F06.AC4/AC5 (PR `feat/f06-browser-workspace-paths`):
   `PathPromptDialog.svelte` + `path-prompt.svelte.ts` +
   `PathPromptHost.svelte` (montado en `app.svelte`); el workspace
   store ahora enruta por `isBrowserMode()` y carga el plugin de
   diálogo Tauri solo en desktop mode. Tests:
   `f06_ac4_http_open_workspace_via_typed_path` y
   `f06_ac5_http_add_extra_path_via_typed_path` (Rust HTTP) +
   `ac5_browser_open_via_typed_path`,
   `ac5_browser_add_extra_via_typed_path`,
   `openViaDialog_returns_null_and_does_not_call_IPC_when_the_user_cancels`
   en `workspace.svelte.test.ts`.
   F06.AC7 + F06.AC9 workspace-config (PR
   `feat/f06-http-config-permissions-gap`): handlers HTTP
   `get_workspace_config`, `update_workspace_config`,
   `list_permission_requests`, `respond_permission_request`;
   `config_get_workspace_impl`/`config_update_workspace_impl`/
   `list_impl`/`respond_impl` extraídos de los Tauri commands;
   el adapter HTTP de `ui/src/lib/ipc.ts` ahora enruta
   `config_get_workspace`, `config_update_workspace`,
   `permissions.list` y `permissions.respond` por HTTP. Tests:
   `f06_ac7_http_permission_request_list_and_respond` y
   `f06_ac9_http_workspace_config_get_and_patch` (Rust HTTP).
   F06.AC6 + F06.AC8 (PR `feat/f06-web-smoke`): tests
   `f06_ac6_chat_send_publishes_events_to_sse` (inyecta un
   `MockProvider` bajo `ollama` via el nuevo
   `ProviderRegistry::register`, suscribe un cliente SSE, y
   verifica que `chat.run.started.v1`,
   `chat.content.delta.v1`, y `chat.run.finished.v1` llegan al
   SSE con el contenido del mock) y
   `f06_ac8_sse_delivers_published_event` (parity del bus).
   `scripts/web-smoke.sh` automatiza la verificación curl-able
   (health, workspaces, config GET/PATCH, permissions,
   SPA fallback).
   Pendiente para `implemented (full)`: smoke manual en
   navegador real (PathPromptDialog UX, SSE en el tab, LAN
   desde otro dispositivo) más allá de `scripts/web-smoke.sh`.
- **features/F02-multi-workspace.md** — `approved` → `implemented (full)`.
  PRs: UI (#12) 9/9 ACs UI + AC3, AC9 backend con `list_dir`; **AC7
  cerrado en PR `fix/f02-ac7-delete-workspace-with-active-runs`**
  (BUG-01, categoría B): `delete_impl` consulta `RunRegistry::iter_for_workspace`,
  rechaza con `Conflict` si hay runs activos y `force=false`, aborta
  con `force=true`, evicta el `WorkspaceRuntime` cacheado. Cambios
  auxiliares: `RunHandle::is_aborted` y `RunHandle::new` se hicieron
  `pub` para que los tests de `agentyx-app` puedan fabricar runs
  sintéticos. 18/18 ACs backend cubiertos.
- **features/F01-chat-streaming.md** — `approved` →
  `implemented (partial — Phase 1 backend + UI + Phase 2-core + Phase 2-app)`. PRs:
  - `feat(core): F01-Phase1 backends` (PR #13): 5/15 ACs
    backend cubiertos (AC1, AC2, AC4, AC5, AC6).
  - `feat(app): F01-Phase1 app wiring` (PR #14):
    9/9 Tauri commands cableados (create_session, send,
    abort, list_sessions, get_history, set/get_active_agent,
    list_agents, get_agent); TauriEventSink; AppState
    refactor.
  - `feat(ui): F01-Phase2 chat UI` (PR #15):
    ChatPanel + MessageList + Composer con Svelte 5 runes;
    `session.svelte.ts` store con state machine completo
    (create/send/abort/setActiveAgent/cyclePrimary + event
    folding para chat.run.started/finished/error, message_start,
    content.delta); 18/18 vitest tests del store pasando;
    UI checks (svelte-check/tsc/eslint/prettier/build) verdes.
  - `feat(core): F01-Phase2-core` (PR #16):
    3 tools read-only (read_file, list_dir, search) en
    `crates/agentyx-core/src/tools/`; `PermissionGate` con
    12-step algorithm + `PermissionRegistry` (oneshot); agent
    loop multi-step en `run_loop` con delta batching, sequential
    tool dispatch, permission ask flow, sequential-to-allow
    transition; `DeltaBatcher` (50ms / 100 chars); `MockProvider`
    para tests; 10/15 ACs backend cubiertos (AC1, AC2, AC3,
    AC4, AC5, AC6, AC7, AC8, AC12, AC13).
  - `feat(app,ui): F01-Phase2-app` (PR #17):
    Permission Tauri commands (`respond`, `list`, `get_matrix`);
    `PermissionPrompt.svelte` modal en `WorkspaceView`;
    `SessionStore` permission event handling + recovery
    (`AppState::recover_orphan_runs` al startup: `Running` →
    `Aborted` "app_closed"); `chat.run.aborted.v1` emitido
    en run abort. F01-Phase2 backend+app+UI cubiertos.
    Ver `## Implementation status` en el spec.
- **domains/config.md (backend de F05)** — `draft` → `ready`
  (PR actual: `feat(app,core): F05 backend wiring (config + secrets)`).
  13/18 ACs cubiertos:
    - `crates/agentyx-core/src/config/`: `WorkspaceConfig`,
      `WorkspaceConfigPatch`, `GlobalConfigPatch`,
      `ResolvedConfig`/`EffectiveConfig` (con `Serialize` para IPC),
      `ServiceConfigPaths` (centraliza paths de TOML).
    - `ConfigService::load_workspace`, `update_workspace`,
      `update_with_patch`, `resolve_secrets`, `resolve_snapshot`,
      `resolve` (con secretos), `set_keychain`, `delete_keychain`,
      `list_keychain_providers`. `OsKeychain` cableado en
      `AppState::initialize` (producción); tests usan
      `FakeKeychain` inyectado.
    - `crates/agentyx-app/src/commands/config.rs`:
      `config_get_global`, `config_update_global`,
      `config_get_workspace`, `config_update_workspace`.
    - `crates/agentyx-app/src/commands/secrets.rs`:
      `set_secret`, `delete_secret`, `list_providers`.
      `set_secret` loguea solo `provider_id`; el `value` se
      mueve a `set_keychain` y se descarta tras la llamada.
    - `crates/agentyx-app/src/main.rs`: 7 nuevos Tauri
      commands cableados en `invoke_handler!`.
    - Tests (5/5 passing en `agentyx-app`, 18/18 passing
      en `agentyx-core`): `f05_ac4_config_update_global_persists`,
      `f05_ac5_approval_mode_deny_blocks_writes_silently`,
      `f05_ac6_workspace_override_isolated_from_global`,
      `f05_ac11_settings_persist_across_app_restart`,
      `f05_ac12_resolved_snapshot_never_includes_secrets`.
  Pendientes para `ready` → `implemented`: edición persistente de
  matriz de permisos (AC9), cobertura E2E de add provider/persistencia
  completa (AC2, AC10, AC11) y eventos `config.changed.v1` (AC15).
  - **PR `feat/f05-permission-matrix-and-config-event`**:
    F05.AC9 + F05.AC15 cerrados.
    - `GlobalConfig.default_tool_decisions: HashMap<String, ToolDecision>`
      (omitido del TOML cuando está vacío para compat con installs
      existentes).
    - `ConfigService::set_default_tool_decision` +
      `clear_default_tool_decision`.
    - `permissions.get_matrix` consulta `default_tool_decisions` antes
      de caer al default estático del catálogo.
    - Nuevo Tauri command `set_default(tool, decision)`.
    - `config_update_global` y `config_update_workspace` emiten
      `config.changed.v1` con payload `{ kind, global?, workspaceId?,
      workspace? }` (builders puros testeados en
      `commands/config.rs::tests::f05_ac15_*`).
    - UI: `SettingsView` reemplaza la tabla read-only por radios
      editables; `events.configChanged` refresca estado cross-tab.
    - Tests añadidos: `f05_ac9_set_default_tool_decision_persists_and_reloads`
      (core), `f05_ac9_set_default_*` (variantes),
      `f05_ac9_set_default_persists_to_disk` (app),
      `f05_ac9_get_matrix_uses_persisted_default`,
      `f05_ac9_get_matrix_falls_back_to_static_default`,
      `f05_ac9_approval_mode_deny_overrides_persisted_default`,
      `f05_ac15_global_changed_payload_shape`,
      `f05_ac15_workspace_changed_payload_shape`,
      `f05_ac15_payloads_are_distinct_by_kind` (más los de round-trip
      de TOML y parseo de `ToolDecision`).
    - UI tests: `f05_ac9_permission_matrix_edits_persist helper returns
      stable order` y `f05_ac9_static_default_decision_matches_known_tools`
      en `helpers.test.ts`.

## ⚫ Deprecated
_(ninguno)_

## Orden de continuidad para agentes

> Para MVP, trabajar de arriba abajo. No abrir F04 write tools ni
> subagents reales hasta cerrar P0 web, salvo instrucción humana.

### P0 — cerrar MVP web funcional

1. **F06-browser-workspace-paths**: ✅ cerrado en PR
   `feat/f06-browser-workspace-paths`. `PathPromptDialog` +
   `pathPromptStore` cubren `openViaDialog` y `addExtraPathViaDialog`
   en browser mode; tests HTTP `f06_ac4_*` y `f06_ac5_*` + UI store
   tests `ac5_browser_*` pasan.
2. **F06-http-config-permissions-gap**: ✅ cerrado en PR
   `feat/f06-http-config-permissions-gap`. Router + handlers +
   `ui/src/lib/ipc.ts` browser adapter para
   `GET /api/v1/config/workspaces/:id`,
   `PATCH /api/v1/config/workspaces/:id`,
   `GET /api/v1/permissions/requests`, y
   `POST /api/v1/permissions/requests/:id/respond` están
   cableados. Tests `f06_ac7_*` y `f06_ac9_*` pasan; el PATCH
   valida el id contra `WorkspaceService` y rechaza con 404.
3. **F06/F05 web smoke**: ✅ parcialmente automatizado.
   - `scripts/web-smoke.sh` arranca el binario en un AGENTYX_HOME
     temporal y verifica con curl: health, workspaces (list +
     open), config GET/PATCH, permission matrix, permission
     requests, SPA fallback. Pendiente: smoke con un browser
     real (PathPromptDialog UX, SSE en el tab, LAN desde otro
     dispositivo). Tests Rust `f06_ac6_*` y `f06_ac8_*` ya
     verifican la parte SSE end-to-end con `MockProvider`.

### P1 — cerrar MVP desktop/read-only

4. **F05-settings**: marcar o cerrar AC2/AC3/AC4/AC5/AC6/AC7/AC8/AC10/
   AC11/AC13 según tests reales. Si falta código, priorizar Ollama
   local configurable y persistencia de settings sobre providers remotos.
5. **F01-chat-streaming**: decidir corte de MVP:
   - si no hay subagents reales en v0.1, mover F01.AC10 y la parte
     funcional de F-agents-ui a v0.1.x;
   - mantener AC11 (429) y AC14 (approval mode mid-run) como hardening
     si el MVP dogfood no depende de ellos.

### P2 — post-MVP / v0.1.x recomendado

6. **F04-file-diffs + write tools**: implementar `write_file`,
   `edit_file`, `apply_patch` antes de exigir diffs completos. La UI de
   diffs sin tools de escritura solo sirve como esqueleto.
7. **F-agents-ui subagents reales**: implementar `@mention` expansion,
   child sessions y SessionTree. La UI parcial actual no ejecuta
   `@general` porque `send` todavía ignora `mentions`.

## Próximas specs a escribir

> Nota de contexto: las specs MVP activas (`F01`, `F02`, `F04`, `F05`,
> `F-agents-ui`, `agents.md`, `domains/config.md`,
> `domains/journal.md`) ya tienen `## Agent context` para lectura
> rápida. F06 está parcialmente implementada; el siguiente trabajo es
> cerrar P0 web funcional antes de retomar F04 o subagents reales.

### Para el MVP (v0.1)

1. Cerrar P0 web funcional (ver arriba).
2. Cerrar F05 al menos para Ollama local + persistencia settings.
3. Confirmar corte read-only del agente: `read_file`, `list_dir`,
   `search` son las tools MVP; escritura/diffs quedan v0.1.x salvo
   decisión humana explícita.

### Para v0.1.x (no bloquea MVP)

- **F03 — Python en `.venv`** (opt-in, badge pasivo, tool `python_run`
  con UX). Affects: workspace, tools, pty.
- **F-extra-paths-tree / F-extra-paths-cap** (ver [`F02`](./features/F02-multi-workspace.md)).

### Gaps conocidos

- **F06 fue corregida a partial**: los gaps operativos siguen siendo
  path manual en browser, HTTP `permissions.respond/list`, config
  workspace y smoke LAN.
- **ROADMAP acceptance estaba obsoleto**: algunos checks web siguen sin
  marcar aunque parte de F06 ya existe; usar la sección "Orden de
  continuidad" como fuente operativa hasta que cada PR sincronice su
  spec afectada.
- **`agents.md` sigue en `draft`** aunque parte del modelo built-in ya
  existe en core; `@mention` real sigue pendiente.
- **F04 depende de tools de escritura inexistentes**: el registry MVP
  solo expone `read_file`, `list_dir`, `search`.

## Reglas de transición

| De → A | Trigger |
|---|---|
| `proposed` → `ready` | Pitch con problema, alcance, contratos y ACs suficientes |
| `ready` → `shipped` | Código mergeado y tests/verificación pasando |
| `shipped` → `deprecated` | Pitch/spec retirado o reemplazado |
| cualquier → `proposed` | Cambios materiales vuelven a diseño |
| `draft` → `review` | El autor pide review (PR o comentario) |
| `review` → `approved` | Al menos 1 aprobación humana y ACs completos |
| `approved` → `implemented` | Código mergeado y tests pasando |
| `implemented` → `deprecated` | Spec retirada o reemplazada (con ADR que lo justifique) |
| cualquier → `draft` | Cambios materiales vuelven al inicio del ciclo |

Las cinco primeras filas son el flujo preferido para trabajo nuevo. Las
filas `draft/review/approved/implemented` quedan como compatibilidad
para specs existentes.

Un pitch `proposed` puede explorarse, pero una feature nueva no se
mergea hasta que esté `ready` (o `approved` si es spec histórica), salvo
hotfixes blocker según `AGENTS.md` §18.

## Nota sobre el ciclo de reforma (PR 1)

El PR 1 introduce cambios materiales en 3 specs globales (`project.md`,
`glossary.md`, `architecture.md`) que estaban `approved`. Estos cambios
son **de alcance/edición**, no de modelo: no refutan las decisiones ya
tomadas, solo las amplían o precisan (sandbox = root + extras, providers
v1 = Ollama/Groq/Minimax, multi-agent como arquitectura base, venv
opt-in). Por tanto:

- **No** se degradan a `draft`; siguen `approved` tras el PR 1.
- Los PRs 2–4 sí degradarán a `review` (o `draft`, en el caso de
  `providers.md` por reescritura mayor) las specs de dominio afectadas,
  según las reglas de `AGENTS.md` §17.
