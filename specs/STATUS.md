# Specs — Status board

> Vista por estado. Para índice maestro: [README.md](./README.md).
> Para roadmap de features: [features/ROADMAP.md](./features/ROADMAP.md).
> Para índice de ADRs: [adr/README.md](./adr/README.md).
>
> Última actualización: 2026-06-07 (multi-PR session:
> PR1: F01.AC9 cerrado — `agent.changed.v1` event emission + agent
> permission overrides en `build_permission_snapshot` + integration test.
> PR2: F05 ProviderRegistry refresh post-config-change + tests E2E.
> PR3: F06 web server — SSE streaming `/api/v1/events`, `POST
> /sessions/:id/messages` send endpoint, config/secrets/providers/
> permissions HTTP endpoints, `ServeDir` static file serving con SPA
> fallback, browser IPC adapter dual-mode (Tauri + HTTP).
> 65/66 tests passing (1 pre-existing failure en workspace sandbox).
>
> **Disciplina de status**: este archivo se actualiza en el mismo PR
> que cambia el estado real de cualquier pitch/spec o deja el board
> obsoleto. Ver `AGENTS.md` §17 Pitch-Driven SDD Lite (regla §17.5).
>
> Estados preferidos para trabajo nuevo: `proposed` → `ready` →
> `shipped` → `deprecated`. Los estados históricos `draft`, `review`,
> `approved` e `implemented` siguen aceptados para specs existentes.

## 🟡 Draft (en construcción)
- agents.md
- domains/providers.md
- domains/journal.md
- features/F05-settings.md (UI parcial en curso; providers/models/approval/
  workspace shell implementado, edición completa de matriz pendiente)
- features/F04-file-diffs.md

## 🔵 Review (pendiente de aprobación)
_(vacío)_
- agents.md
- domains/providers.md
- domains/journal.md
- features/F05-settings.md (UI parcial en curso; providers/models/approval/
  workspace shell implementado, edición completa de matriz pendiente)

## 🟢 Ready (AC + contratos listos, pendiente implementación)
- features/F-agents-ui.md (spec completada: AgentChip, AgentPickerMenu,
  Cmd+[/Cmd+] cycle, AtMentionPopover, SessionTree, SubagentLiveDot,
  SessionTabs placeholder. 15 ACs definidos. Sin código aún.)
- features/F04-file-diffs.md (spec completada: DiffView con
  CodeMirror 6 Merge, DiffsSidePanel, truncation/binary/image
  detection, collapse state en localStorage. 12 ACs definidos.
  Depende de F01.AC8 (chat.tool_call.v1 enriquecido). Sin código
  aún.)

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

## ✅ Implemented (código en main, ACs cumplidos, tests pasando)
- **features/F06-web-server-lan.md** — `ready` → `implemented (full — AC1-AC10)`. PRs:
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
    - Nuevo Tauri command `permissions_set_default(tool, decision)`.
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

## Próximas specs a escribir

> Nota de contexto: las specs MVP activas (`F01`, `F02`, `F04`, `F05`,
> `F-agents-ui`, `agents.md`, `domains/config.md`,
> `domains/journal.md`) ya tienen `## Agent context` para lectura
> rápida. F06 está shipped; el siguiente trabajo es cerrar `config`/`providers`
> reales (F05), completar los AC abiertos de F01, cerrar F-agents-ui y F04.

### Para el MVP (v0.1)

> F02 está implementada y F01 tiene la foundation core/app/UI en
> `main`. El siguiente trabajo MVP es cerrar la configuración real de
> providers/secrets (F05), completar los AC abiertos de F01, implementar
> F06 para que desktop y web LAN funcionen a la vez, y después cerrar
> F-agents-ui y F04.

1. `F05-settings.md`: cerrar edición persistente de la matriz de
   permisos, cobertura E2E de add provider y evento `config.changed.v1`.
2. `F01-chat-streaming.md`: cerrar AC9, AC10, AC11 y AC14.
3. `F-agents-ui.md`: AgentChip, cycle shortcuts, @mention popover y
   SessionTree.
4. `F04-file-diffs.md`: diffs read-only sobre eventos/tool results.

### Para v0.1.x (no bloquea MVP)

- **F03 — Python en `.venv`** (opt-in, badge pasivo, tool `python_run`
  con UX). Affects: workspace, tools, pty.
- **F-extra-paths-tree / F-extra-paths-cap** (ver [`F02`](./features/F02-multi-workspace.md)).

### Gaps conocidos

_(vacío — F02.AC7 cerrado en PR `fix/f02-ac7-delete-workspace-with-active-runs`)_
- **`agents.md` sigue en `draft`** aunque parte del modelo built-in ya
  existe en core; F-agents-ui debe decidir si promueve la spec o la
  mantiene como dominio en diseño.
- **F05 Settings UI es parcial**: consume comandos reales de config,
  providers y secrets, pero `permissions_set_default`/eventos F05 y E2E
  de persistencia completa siguen pendientes.

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
