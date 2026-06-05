# Specs — Status board

> Vista por estado. Para índice maestro: [README.md](./README.md).
> Para roadmap de features: [features/ROADMAP.md](./features/ROADMAP.md).
> Para índice de ADRs: [adr/README.md](./adr/README.md).
>
> Última actualización: 2026-06-05

## 🟡 Draft (en construcción)
- **domains/journal.md** (Fase B — log append-only SQLite puro, 16 ACs)
- **domains/config.md** (Fase B — TOML global + workspace, SecretRef/env/keychain, 18 ACs)
- **features/F05-settings.md** (Fase B — Providers/Models/Approval/Workspace tabs, 15 ACs)
- **features/F01-chat-streaming.md** (Fase B — chat con streaming LLM + multi-agent, 15 ACs)
- **features/F04-file-diffs.md** (Fase B — CodeMirror merge read-only en v0.1, 12 ACs)
- **features/F-agents-ui.md** (Fase B — AgentChip + Cmd+[/] + @mention popover + SessionTree, 15 ACs)

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
- domains/session.md
- domains/storage.md
- domains/pty.md
- features/ROADMAP.md (revisado en PR 5: v0.1 sin F03; F-agents-ui nuevo; F-extra-paths-* en v0.1.x)
- features/F02-multi-workspace.md (revisado en este PR — sin venv obligatorio, UI de extra paths)

## ADRs

- ADR-0001 a ADR-0006: accepted (sin cambios en PR 1-3).
- **ADR-0007** (nuevo, PR 3): modelo `root + extra_paths` por workspace.
- **ADR-0008** (nuevo, PR 3): scope de providers v1 (Ollama / Groq / Minimax).

## ✅ Implemented (código en main, AC cumplidos, tests pasando)
_(ninguno todavía — Bloque 3 pendiente)_

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
| `draft` → `review` | El autor pide review (PR o comentario) |
| `review` → `approved` | Al menos 1 aprobación humana y ACs completos |
| `approved` → `implemented` | Código mergeado y tests pasando |
| `implemented` → `deprecated` | Spec retirada o reemplazada (con ADR que lo justifique) |
| cualquier → `draft` | Cambios materiales vuelven al inicio del ciclo |

Una spec `draft` **puede codearse**, pero el código no se mergea hasta que la spec esté `approved` (salvo hotfixes blocker, ver `AGENTS.md` §Gestión de bugs).

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
