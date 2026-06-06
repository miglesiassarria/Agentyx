<!--
Plantilla de PR para Agentyx. Secciones obligatorias: ## Refs y
## Spec status changes (regla §17.5 de AGENTS.md).

Para PRs puramente operativos que NO tocan código cubierto por
specs (chore: docs, refactor: estilo, etc.), se permite dejar
## Spec status changes con "N/A" y justificarlo en una línea.
-->

## Resumen

<1-3 frases sobre qué cambia y por qué.>

## Refs (obligatorio)

Pegar aquí, literalmente, los anchors de specs que este PR
implementa, modifica o referencia. Formato (ver AGENTS.md §17.1.3):

```
Refs: specs/domains/<x>.md#<sección>, specs/features/F<NN>-<slug>.md#F<NN>.AC<m>
```

Ejemplo:

```
Refs: specs/domains/workspace.md#Operations, specs/features/F02-multi-workspace.md#F02.AC2
```

## Spec status changes (obligatorio)

> **Regla §17.5 de AGENTS.md.** Toda spec tocada por este PR debe
> aparecer aquí con su nuevo estado. Si el PR no toca specs
> (p. ej. chore puro), escribir `N/A — <motivo>`.

- [ ] `specs/...` — `old_status` → `new_status` (motivo / ACs cubiertos)

## Affected surfaces

Marca lo que aplique:

- [ ] Backend Rust (`crates/agentyx-core`)
- [ ] Shell Tauri (`crates/agentyx-app`)
- [ ] IPC (`specs/ipc.md` — Tauri commands, eventos, errores)
- [ ] UI Svelte (`ui/`)
- [ ] Specs (`specs/**`)
- [ ] CI / scripts (`scripts/`, `.github/`)
- [ ] Docs (`docs/`, `README.md`, `AGENTS.md`)

## Checklist §15 (AGENTS.md)

- [ ] `cargo fmt --all -- --check` pasa
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` pasa
- [ ] `cargo test` pasa al 100%
- [ ] `cargo deny check` pasa
- [ ] `pnpm lint` / `bun run lint` pasa
- [ ] `pnpm typecheck` pasa
- [ ] `pnpm test` pasa
- [ ] Smoke test manual (si aplica)
- [ ] Sin secretos nuevos en el diff
- [ ] **Spec sync (regla §17.5)**: `specs/STATUS.md` + specs afectadas actualizadas en el mismo PR

## Discovered bugs (opcional, ver §18)

Si este PR descubrió un gap o se desvió de la spec, listar aquí
los issues a abrir con `Closes #NN` y referencia a la sección
`## Discovered bugs (post-approval)` de la spec afectada.

- [ ] Categoría A (spec gap) — <descripción, spec#sección>
- [ ] Categoría B (implementation bug) — <descripción, spec#AC>
