# Specs — Status board

> Vista por estado. Para índice maestro: [README.md](./README.md).
> Para roadmap de features: [features/ROADMAP.md](./features/ROADMAP.md).
> Para índice de ADRs: [adr/README.md](./adr/README.md).
>
> Última actualización: 2026-06-06 (Pitch-Driven SDD Lite habilitado)
>
> **Disciplina de status**: este archivo se actualiza en el mismo PR
> que cambia el estado real de cualquier pitch/spec o deja el board
> obsoleto. Ver `AGENTS.md` §17 Pitch-Driven SDD Lite (regla §17.5).
>
> Estados preferidos para trabajo nuevo: `proposed` → `ready` →
> `shipped` → `deprecated`. Los estados históricos `draft`, `review`,
> `approved` e `implemented` siguen aceptados para specs existentes.

## 🟡 Draft (en construcción)
_(vacío)_

## 🔵 Review (pendiente de aprobación)
_(vacío)_

## 🟢 Approved (listo para implementar)
- project.md (revisado en PR 1)
- glossary.md (revisado en PR 1)
- architecture.md (revisado en PR 1)
- ipc.md (revisado en PR 2)
- agents.md (revisado en PR 3 — sistema multi-agente)
- domains/agent-loop.md (revisado en PR 3)
- domains/workspace.md (revisado en PR 3)
- domains/permissions.md (revisado en PR 3)
- domains/tools.md (revisado en PR 3)
- domains/providers.md (revisado en PR 3 — reescritura mayor: Ollama / Groq / Minimax)
- domains/journal.md (revisado en PR de foundational — log append-only SQLite puro)
- domains/config.md (revisado en PR de foundational — TOML + SecretRef/env/keychain)
- domains/session.md
- domains/storage.md
- domains/pty.md
- features/ROADMAP.md (revisado en PR 5: v0.1 sin F03; F-agents-ui nuevo; F-extra-paths-* en v0.1.x)
- features/F02-multi-workspace.md *(backend 17/18 + UI 9/9 implementado en PRs #5, #6 y este PR; AC7 parcial — ver § Implementation status en el spec)*
- features/F05-settings.md (revisado en este PR — Providers/Models/Approval/Workspace tabs)
- features/F01-chat-streaming.md (revisado en este PR — chat streaming LLM + multi-agent)
- features/F04-file-diffs.md (revisado en este PR — CodeMirror merge read-only v0.1)
- features/F-agents-ui.md (revisado en este PR — AgentChip + Cmd+[/] + @mention + SessionTree)

## ADRs

- ADR-0001 a ADR-0006: accepted (sin cambios en PR 1-3).
- **ADR-0007** (nuevo, PR 3): modelo `root + extra_paths` por workspace.
- **ADR-0008** (nuevo, PR 3): scope de providers v1 (Ollama / Groq / Minimax).

## ✅ Implemented (código en main, AC cumplidos, tests pasando)
- **features/F02-multi-workspace.md** — `approved` → `implemented`
  (PR de UI: 9 ACs UI + AC3, AC9 backend cubiertos con `list_dir`
  command; AC7 sigue parcial: el check de runs activos llega con
  el PR de `agent-loop`).
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
  - `feat(app,ui): F01-Phase2-app` (este PR):
    Permission Tauri commands (`respond`, `list`, `get_matrix`);
    `PermissionPrompt.svelte` modal en `WorkspaceView`;
    `SessionStore` permission event handling + recovery
    (`AppState::recover_orphan_runs` al startup: `Running` →
    `Aborted` "app_closed"); `chat.run.aborted.v1` emitido
    en run abort. F01-Phase2 backend+app+UI cubiertos.
    Ver `## Implementation status` en el spec.

## ⚫ Deprecated
_(ninguno)_

## Próximas specs a escribir

### Para el MVP (v0.1)

> **Actualizado tras Fase B** (2026-06-05). Las 5 specs fundamentales
> del MVP ya están redactadas en `draft` (journal, config, F01, F05,
> F04, F-agents-ui) y el dominio `agents.md` se promueve en PR 2.
>
> Tras la aprobación del WT actual (PR 3+4), las specs a promover a
> `review` y luego `approved` son, en este orden:
>
> 1. `journal.md` + `config.md` (bloqueantes de F01 y F05).
> 2. `F05-settings.md` (bloqueante de F01).
> 3. `F01-chat-streaming.md` (feature principal).
> 4. `F04-file-diffs.md` (depende de F01).
> 5. `F-agents-ui.md` (depende de F01).
>
> Cuando todas estén `approved`, se puede arrancar la implementación
> del MVP (Fase C del plan: bootstrap del monorepo + Fase D: features).

### Para v0.1.x (no bloquea MVP)

- **F03 — Python en `.venv`** (opt-in, badge pasivo, tool `python_run`
  con UX). Affects: workspace, tools, pty.
- **F-extra-paths-tree / F-extra-paths-cap** (ver [`F02`](./features/F02-multi-workspace.md)).

### Gaps conocidos

- **`agents.md` está en `draft`** (no en `review`/`approved`) — está en
  el WT pero aún no se ha promovido formalmente.
- **El schema de `sessions` con `parent_session_id` para soportar
  child sessions** se introduce en F01 + F-agents-ui; requiere
  migración de `state.db` antes de implementar F-agents-ui si
  F-agents-ui entra antes que F01 (no es el caso previsto en
  ROADMAP: F-agents-ui depende de F01).
- **Plan original mencionaba "PR 5" para ROADMAP**; tras la
  ampliación a Fase B, los PRs 5+ deberían cubrir la promoción
  de las 6 specs nuevas a `review`/`approved`.

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
