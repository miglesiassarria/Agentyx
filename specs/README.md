# Agentyx — Specs

> Fuente de verdad del diseño. Cualquier cambio de comportamiento
> que toque más de un archivo debe estar documentado aquí antes
> de implementarse.
>
> Las specs son **versionadas** con el código: viven en el repo,
> en el mismo commit que el código que describen. No en Notion,
> no en Google Docs, no en otro sistema.

## Estado global (resumen)

| Spec | Tipo | Status | Owner | Última edición |
|---|---|---|---|---|
| [project.md](./project.md) | global | approved | @miglesias | 2026-06-04 |
| [glossary.md](./glossary.md) | global | approved | @miglesias | 2026-06-04 |
| [architecture.md](./architecture.md) | global | draft | @miglesias | 2026-06-04 |
| [ipc.md](./ipc.md) | global | draft | @miglesias | 2026-06-04 |
| [domains/](./domains/) | domain | (vacío) | — | — |
| [features/](./features/) | feature | (vacío) | — | — |
| [adr/](./adr/) | adr | ver [adr/README.md](./adr/README.md) | — | — |
| [bugs/](./bugs/) | bug report | (vacío) | — | — |

> **Nota**: las tablas por categoría están en sus respectivos `README.md`:
> - [specs/STATUS.md](./STATUS.md) — board kanban por estado
> - [specs/features/ROADMAP.md](./features/ROADMAP.md) — vista por features con dependencias y fases
> - [specs/adr/README.md](./adr/README.md) — índice de ADRs

## Cómo navegar

| Quiero… | Voy a… |
|---|---|
| Entender qué es el proyecto | [project.md](./project.md) |
| Saber qué significa un término (workspace, sesión, tool, …) | [glossary.md](./glossary.md) |
| Ver el diagrama de cajas y el flujo de datos | [architecture.md](./architecture.md) |
| Saber cómo se hablan Rust ↔ UI ↔ navegador | [ipc.md](./ipc.md) |
| Conocer un dominio del core | [domains/X.md](./domains/) |
| Conocer una feature vertical | [features/F<NN>-slug.md](./features/) |
| Entender una decisión de stack | [adr/NNNN-slug.md](./adr/) |
| Ver en qué se está trabajando | [STATUS.md](./STATUS.md) |
| Ver el roadmap de features | [features/ROADMAP.md](./features/ROADMAP.md) |
| Documentar un bug | [bugs/](./bugs/) + issue en GitHub/GitLab |

## Convenciones

- **Tipos de spec**: `global` (visión, glosario, arquitectura, IPC) · `domain` (un dominio del core) · `feature` (funcionalidad vertical de cara al usuario) · `adr` (decisión de arquitectura) · `bug` (reporte y resolución).
- **Status**: `draft` → `review` → `approved` → `implemented` → `deprecated`.
- **Numeración**: dominios libres; features `F<NN>-slug.md`; ADRs `NNNN-slug.md`; bugs `BUG-<NN>-slug.md`.
- **Metadata obligatoria** al inicio de cada spec (ver [templates/](./templates/)):

  ```markdown
  **Status**: <uno de los de arriba>
  **Owner**: @<nick>
  **Last update**: YYYY-MM-DD
  ```

- **Acceptance criteria** al final en formato checklist markdown:

  ```markdown
  ## Acceptance criteria
  - [ ] AC1 …
  - [ ] AC2 …
  ```

- **Refs cruzadas**: las features referencian dominios (`Affects:`); los dominios nunca referencian features. Los ADRs referencian la decisión tomada y, si aplica, la spec que la consumirá.

## Reglas de modificación

1. Cambios en APIs de Tauri command, endpoint HTTP o evento streaming → actualizar [ipc.md](./ipc.md) **y** la spec de dominio afectada en el mismo PR.
2. Cambios de comportamiento de usuario → actualizar la feature spec y sus acceptance criteria.
3. Decisiones de stack/arquitectura nuevas → crear ADR (no editar uno antiguo; los ADRs son inmutables una vez `accepted`).
4. PRs que no referencian al menos una spec → **rechazados** en review (ver `AGENTS.md` §Spec-Driven Development).
5. Specs desactualizadas son bugs categoría A (ver `AGENTS.md` §Gestión de bugs).

## Cómo busca la IA (o un humano nuevo)

```bash
# Qué specs hay en draft
rg -l "Status: draft" specs/

# Qué menciona streaming
rg -l "streaming" specs/

# Qué features dependen de F02
rg "Depends on: F02" specs/features/

# Qué specs toca el .venv
rg "\.venv" specs/

# Acceptance criteria abiertos
rg "^- \[ \]" specs/

# ADRs aceptados
rg "Status: accepted" specs/adr/
```

La IA (o el humano) **no carga todas las specs en contexto**. Carga solo las que el cambio toca. Empieza por `specs/README.md` y desde ahí navega con `grep`/`glob`.
