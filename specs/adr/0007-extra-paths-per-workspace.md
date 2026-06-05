# ADR-0007 — Modelo `root + extra_paths` por workspace

**Status**: accepted
**Date**: 2026-06-05
**Deciders**: @miglesias

## Context

El modelo inicial del workspace (PRD anterior) era: **un workspace = un
directorio raíz** donde el agente tiene acceso R/W, y nada más. El path
sandboxing era: `path ∈ root_path` → allow, `path ∉ root_path` → deny.

Esto no encaja con el uso real. Hay workflows en los que el usuario
quiere dar al agente acceso a directorios fuera del proyecto principal:

- **Recursos compartidos**: `/Users/pepe/assets/` (imágenes, datos, …)
  que el agente debe poder leer y, a veces, escribir.
- **Exports / outputs**: `/tmp/agentyx-exports/` donde el agente deja
  resultados (PDFs, ZIPs, …) sin contaminar el proyecto principal.
- **Monorepos / multi-project**: `~/code/infra/` y `~/code/app/` se
  gestionan desde un único workspace "platform".
- **Side paths de configuración**: `~/.ssh/` no, pero
  `~/projects/secrets/` sí, en un workspace concreto.

Si no soportamos esto desde v1, el usuario o bien:
1. Mueve archivos dentro del root (contamina el proyecto, no siempre
   deseable), o
2. Usa múltiples workspaces y copia archivos entre ellos (tedioso,
   pierde el modelo de "una sola sesión sobre un conjunto de paths").

Ambas opciones degradan la UX. Decidimos soportarlo de forma nativa.

## Decision

El modelo es **(C) híbrido**:

- Un workspace tiene **un `root_path`** (primario, donde el agente
  trabaja por defecto) y **0..N `extra_paths`** (secundarios, con
  acceso R/W explícito).
- El path sandboxing es: `path ∈ root_path ∨ path ∈ any(extra_paths)`
  → allow (salvo que `deny_paths` diga lo contrario).
- El **prompt del sistema** del agent listará los `extra_paths` con
  un mensaje del estilo: "Adicionalmente, tienes acceso R/W a los
  siguientes directorios: `<lista>`. Por defecto, todo lo que generes
  debe ir en el root; usa los extras solo cuando el usuario lo pida
  explícitamente o cuando sea claramente necesario".
- Los `extra_paths` se declaran en `[[extra_paths]]` del
  `config.toml` del workspace (ver [`workspace.md`](../domains/workspace.md)).
- La matriz de permisos del workspace sigue funcionando igual
  (`deny_paths`, `allow_paths`, etc.). Se añade un override
  `permissions.extra_paths.deny` para reglas finas aplicadas
  **dentro** de los `extra_paths`.
- La UI expone los extras como una sección "Extras" en el sidebar
  del workspace, con botones "+ Add directory" y "✕" para borrar.

## Status

`accepted`. Las specs afectadas (`workspace.md`, `permissions.md`,
`tools.md`, `ipc.md`, `F02-multi-workspace.md`) se actualizan en
PR 3 / PR 4 de la reforma de scope.

## Consequences

### Positivas

- **UX nativa** para el caso "este es mi proyecto + tengo que
  tocar también `~/assets/`". Sin mover archivos, sin múltiples
  workspaces.
- **Arquitectura preparada para el futuro**: en v1.x, si el
  usuario quiere un workspace **sin root** (lista pura de paths,
  modelo B), basta con permitir `root_path = null` en el config.
  No hay refactor mayor.
- **Defensa en profundidad**: el prompt del sistema advierte
  al LLM, pero la matriz de permisos (`root ∪ extras` como
  allowlist) y la canonicalización de paths lo refuerzan.
- **Trazabilidad**: el journal registra el `path` exacto
  tocado en cada tool call, así que es fácil saber en el
  replay "el agente escribió en `/Users/pepe/assets/foo.png`".

### Negativas

- **Más superficie de ataque**: cuantos más paths tenga el
  workspace, más ventanas para "sorpresas" (p. ej. el usuario
  añade `/` por error). Mitigado por la **whitelist de roots
  permitidos** en `Workspace::open` y `Workspace::add_extra_path`
  (mismo control que el root).
- **UI más cargada**: hay que mostrar los extras en el sidebar
  con su path absoluto, label opcional y botón de borrado.
  No es complejo, pero suma una pantalla.
- **El prompt del sistema crece** con N extras. Para N grande
  (>10), podría empezar a molestar al modelo. v1: no hay cap;
  v1.x quizá cap a 10 o 20 con "more…" en la UI.

### Neutras

- Los tools (`read_file`, `write_file`, `list_dir`, `search`,
  `apply_patch`) **no cambian su API**: siguen recibiendo un
  `path` arbitrario. El sandboxing se aplica en
  `Permissions::check` (ver `permissions.md` §Algoritmo).
- El `ToolContext` ahora lleva `extra_paths` para que las tools
  que quieran mostrarlos en logs/errors tengan el dato.

## Alternatives considered

### Alternative A: Lista pura de paths (sin root)

- Pros: máxima flexibilidad. El usuario decide qué paths
  autorizó, sin jerarquía.
- Cons: pierde el "centro de gravedad" del workspace. El
  prompt del sistema queda ambiguo sobre "dónde trabajar
  por defecto". En la práctica, casi todos los casos
  tienen un path "principal".
- **Por qué se descartó**: KISS. La jerarquía root + extras
  cubre el 90 % de los casos con menos reglas.

### Alternative B: Solo root, sin extras

- Pros: el más simple. Una sola fuente de verdad, una sola
  regla de sandboxing.
- Cons: el caso de uso "necesito acceder a `~/assets/`" no
  se soporta de forma natural. El usuario tiene que mover
  archivos o usar varios workspaces.
- **Por qué se descartó**: no encaja con el producto que
  queremos construir. El usuario lo necesita.

### Alternative C: Root + extras (esta decisión)

- Pros: encaja con la realidad. Default sensato (root) +
  overrides explícitos (extras). Arquitectura preparada
  para v1.x (rootless).
- Cons: las dos anteriores (más superficie, UI más cargada,
  prompt más largo).
- **Por qué se eligió**: mejor balance. Las negatives se
  mitigan con controles (whitelist, cap de N, prompt bien
  redactado).

### Alternative D: Extras globales (no por workspace)

- Pros: el usuario configura una vez los paths que siempre
  están disponibles.
- Cons: pierde el aislamiento por workspace. Un workspace
  efímero o de prueba hereda paths de un workspace de
  producción. Inconsistente con el modelo de "cada
  workspace es una jaula".
- **Por qué se descartó**: rompe el principio de sandbox
  por workspace (AGENTS.md §9). Los extras son por
  workspace, siempre.

## References

- [`../glossary.md`](../glossary.md) — definición de `Extra path`.
- [`../domains/workspace.md`](../domains/workspace.md) —
  `extra_paths` en `config.toml`, operaciones
  `add_extra_path` / `remove_extra_path`.
- [`../domains/permissions.md`](../domains/permissions.md) —
  paso 2bis del algoritmo de `check`.
- [`../domains/tools.md`](../domains/tools.md) — `ToolContext`
  con `extra_paths`, path sandboxing = `root ∪ extras`.
- [`../ipc.md`](../ipc.md) — Tauri commands de extra paths.
- [`../features/F02-multi-workspace.md`](../features/F02-multi-workspace.md) —
  UI de "Extras" en el sidebar.
- Patrón de opencode: el proyecto opencode tiene un concepto
  similar (worktree + sandboxing), aunque con otro modelo de
  configuración. Nuestra decisión es más simple (lista explícita
  en el config del workspace).
