<!--
Plantilla de PR para Agentyx. Secciones obligatorias: ## Refs y
## Spec status changes (regla §17.5 de AGENTS.md).

Pitch-Driven SDD Lite: si el PR no toca comportamiento, contratos,
persistencia, permisos, seguridad o arquitectura, usar N/A con motivo.
-->

## Resumen

<1-3 frases sobre qué cambia y por qué.>

## Refs (obligatorio)

Pegar aquí los anchors de pitches/specs que este PR implementa,
modifica o referencia. Si no aplica, justificarlo en una línea.

```
Refs: specs/domains/<x>.md#<sección>, specs/features/F<NN>-<slug>.md#F<NN>.AC<m>
```

Ejemplo:

```
Refs: specs/domains/workspace.md#Operations, specs/features/F02-multi-workspace.md#F02.AC2
```

N/A válido:

```
Refs: N/A — refactor local sin cambio de contrato ni UX
```

## Spec status changes (obligatorio)

> **Regla §17.5 de AGENTS.md.** Toda spec tocada por este PR debe
> aparecer aquí si cambia su estado. Si no hay cambio de estado,
> escribir `N/A — <motivo>`.

- [ ] `specs/...` — `old_status` → `new_status` (motivo)
- [ ] N/A — <status sin cambios / PR fuera de SDD>

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
- [ ] **Spec sync (regla §17.5)**: pitch/spec actualizado si cambian alcance, contratos, ACs o estado; `specs/STATUS.md` actualizado solo si cambia status o el board queda obsoleto

## Discovered bugs (opcional, ver §18)

Si este PR descubrió un gap o se desvió de la spec, listar aquí
los issues a abrir con `Closes #NN` y referencia a la sección
`## Discovered bugs (post-approval)` de la spec afectada.

- [ ] Categoría A (spec gap) — <descripción, spec#sección>
- [ ] Categoría B (implementation bug) — <descripción, spec#AC>
