# Specs — Status board

> Vista por estado. Para índice maestro: [README.md](./README.md).
> Para roadmap de features: [features/ROADMAP.md](./features/ROADMAP.md).
> Para índice de ADRs: [adr/README.md](./adr/README.md).
>
> Última actualización: 2026-06-04

## 🟡 Draft (en construcción)
- architecture.md
- ipc.md

## 🔵 Review (pendiente de aprobación)
_(ninguno)_

## 🟢 Approved (listo para implementar)
- project.md
- glossary.md

## ✅ Implemented (código en main, AC cumplidos, tests pasando)
_(ninguno todavía — Bloque 1 en curso)_

## ⚫ Deprecated
_(ninguno)_

## Próximas specs a escribir

Siguiente paso tras el Bloque 1 (specs globales):

1. **Dominios** (orden sugerido):
   - `domains/agent-loop.md` — corazón del producto
   - `domains/providers.md` — normalización OpenAI/Anthropic/Ollama
   - `domains/storage.md` — SQLite, journal, migraciones
   - `domains/workspace.md` — workspaces, `.venv`, paths
   - `domains/permissions.md` — matriz y decisiones
   - `domains/tools.md` — contrato de tools
   - `domains/pty.md` — wrapper de `portable-pty`
   - `domains/session.md` — ciclo de vida de sesión

2. **Features** (la primera será tras tener dominios):
   - `features/ROADMAP.md` — vista roadmap
   - `features/F01-…` — primera feature vertical

3. **ADRs** — los seis ya decididos (`0001`–`0006`) se crean en este bloque.

## Reglas de transición

| De → A | Trigger |
|---|---|
| `draft` → `review` | El autor pide review (PR o comentario) |
| `review` → `approved` | Al menos 1 aprobación humana y ACs completos |
| `approved` → `implemented` | Código mergeado y tests pasando |
| `implemented` → `deprecated` | Spec retirada o reemplazada (con ADR que lo justifique) |
| cualquier → `draft` | Cambios materiales vuelven al inicio del ciclo |

Una spec `draft` **puede codearse**, pero el código no se mergea hasta que la spec esté `approved` (salvo hotfixes blocker, ver `AGENTS.md` §Gestión de bugs).
