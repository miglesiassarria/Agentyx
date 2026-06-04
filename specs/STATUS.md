# Specs — Status board

> Vista por estado. Para índice maestro: [README.md](./README.md).
> Para roadmap de features: [features/ROADMAP.md](./features/ROADMAP.md).
> Para índice de ADRs: [adr/README.md](./adr/README.md).
>
> Última actualización: 2026-06-04

## 🟡 Draft (en construcción)
_(ninguno — todas las specs escritas están en review o approved)_

## 🔵 Review (pendiente de aprobación)
_(ninguno — el autor promovió directamente tras auto-revisión)_

## 🟢 Approved (listo para implementar)
- project.md
- glossary.md
- architecture.md
- ipc.md
- domains/agent-loop.md
- domains/session.md
- domains/storage.md
- domains/workspace.md
- domains/permissions.md
- domains/tools.md
- domains/pty.md
- domains/providers.md
- features/ROADMAP.md
- features/F02-multi-workspace.md

## ✅ Implemented (código en main, AC cumplidos, tests pasando)
_(ninguno todavía — Bloque 3 pendiente)_

## ⚫ Deprecated
_(ninguno)_

## Próximas specs a escribir

Para el MVP (v0.1), falta escribir las **features** que complementan
a F02 (que ya está `approved`):

1. **F05 — Settings** (providers, modelos, keychain, approval_mode).
   Affects: providers, permissions, config.
2. **F01 — Chat con streaming LLM**. Affects: agent-loop, providers,
   session. Feature principal del MVP.
3. **F03 — Python en .venv** (badge + CTA + tool python_run con
   UX). Affects: workspace, tools, pty.
4. **F04 — File diffs en UI** (CodeMirror merge). Affects: tools, ui.

Los **dominios están todos aprobados** y listos para implementar.
Faltan los **journal** spec (mencionado en `agent-loop.md` y
`storage.md` como dependencia, pero no escrito aún) y el spec de
**config** global (`~/.agentyx/config.toml`).

> Nota: la spec de `journal` debería crearse antes de implementar el
> agent loop (F01) porque es donde se persiste cada step. Se
> propondrá en el Bloque 3 si surge como bloqueante.

## Reglas de transición

| De → A | Trigger |
|---|---|
| `draft` → `review` | El autor pide review (PR o comentario) |
| `review` → `approved` | Al menos 1 aprobación humana y ACs completos |
| `approved` → `implemented` | Código mergeado y tests pasando |
| `implemented` → `deprecated` | Spec retirada o reemplazada (con ADR que lo justifique) |
| cualquier → `draft` | Cambios materiales vuelven al inicio del ciclo |

Una spec `draft` **puede codearse**, pero el código no se mergea hasta que la spec esté `approved` (salvo hotfixes blocker, ver `AGENTS.md` §Gestión de bugs).
