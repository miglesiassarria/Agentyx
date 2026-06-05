# Architecture Decision Records (ADRs)

> Decisiones de arquitectura inmutables. Cada ADR se escribe **antes**
> de implementar la decisión y, una vez `accepted`, no se edita: para
> "cambiar de opinión" se crea un nuevo ADR que `supersedes` al anterior.
>
> Plantilla: [../templates/adr.md](../templates/adr.md)

## Índice

| # | Título | Status | Date |
|---|---|---|---|
| [0001](0001-tauri-vs-electron.md) | Tauri 2 vs Electron como shell de escritorio | accepted | 2026-06-04 |
| [0002](0002-rust-core-svelte-ui.md) | Rust core + Svelte 5 UI | accepted | 2026-06-04 |
| [0003](0003-axum-embedded-server.md) | Servidor HTTP embebido (axum) en el mismo proceso Tauri | accepted | 2026-06-04 |
| [0004](0004-detect-venv-priority.md) | Orden de detección del entorno virtual de Python | accepted | 2026-06-04 |
| [0005](0005-pty-portable-pty.md) | PTY con `portable-pty` | accepted | 2026-06-04 |
| [0006](0006-sqlite-rusqlite.md) | SQLite con `rusqlite` (bundled) en vez de `sqlx` | accepted | 2026-06-04 |
| [0007](0007-extra-paths-per-workspace.md) | Modelo `root + extra_paths` por workspace | accepted | 2026-06-05 |
| [0008](0008-providers-v1-scope.md) | Scope de providers LLM en v1 (Ollama / Groq / Minimax) | accepted | 2026-06-05 |

## Cómo crear un ADR nuevo

1. Copiar [../templates/adr.md](../templates/adr.md) a
   `NNNN-<slug>.md` con el siguiente número libre.
2. Rellenar **Context** y **Alternatives considered** ANTES de
   implementar.
3. Marcar como `proposed`.
4. Una vez haya consenso, cambiar `Status` a `accepted`.
5. Añadir a este índice.
6. Si invalida un ADR anterior, añadir nota `Supersedes: NNNN-<slug>`
   al inicio del nuevo y en el índice.

## Estados

- `proposed` — en discusión.
- `accepted` — vigente.
- `rejected` — descartado tras discusión (se conserva para historia).
- `deprecated` — era `accepted` pero ya no aplica (con
  `Supersedes: NNNN-<slug>` apuntando al ADR que lo reemplaza).
- `superseded by NNNN` — ver `deprecated`.

## Cuándo escribir un ADR

Escribir un ADR **antes** de:

- Elegir un crate, framework o servicio nuevo.
- Cambiar un crate/framework/servicio ya en uso.
- Cambiar la estructura de directorios del proyecto.
- Cambiar un protocolo o contrato cross-cutting (IPC, storage layout,
  formato del journal).
- Decidir cómo se gestiona secrets, telemetría, auth, permisos.

**No** requieren ADR:

- Detalles de implementación internos a un módulo.
- Refactors pequeños sin impacto externo.
- Decisiones que se revierten en el siguiente commit.

## Cuándo NO escribir un ADR

- Si la decisión ya está en un ADR aceptado y solo estás aplicándolo.
- Si es una elección puramente estética (color, naming interno).
- Si es un workaround temporal con fecha de retirada.
