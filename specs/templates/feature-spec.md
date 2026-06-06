# Feature Pitch — Plantilla ligera

**Status**: proposed
**Owner**: @<nick>
**Last update**: YYYY-MM-DD
**Affects**: `domains/<x>.md`, `specs/ipc.md` si aplica
**Depends on**: `F<NN>` o `N/A`

> Copiar a `specs/features/F<NN>-<slug>.md`. Objetivo: 120-180 líneas.
> Si una feature necesita más, dividirla o añadir solo lo imprescindible.

## Agent context

- Leer este pitch antes de tocar la feature.
- Leer `specs/ipc.md` solo si cambia `Contracts`.
- Leer ADRs solo si se toma o modifica una decisión difícil de revertir.

## Problem

Qué duele hoy, a quién afecta y por qué merece inversión. 5-8 líneas.

## Appetite

**Budget**: small (1-2 días) | medium (1 semana) | large (2-3 semanas)

Cómo el presupuesto limita la solución. Qué se recorta primero si el
trabajo crece.

## Solution Shape

Descripción de alto nivel de la solución. Incluir solo las piezas que
orientan la implementación: módulos Rust, comandos Tauri, componentes
Svelte, persistencia o flujos principales.

## Contracts

Solo contratos que cambian. Si no cambia ninguno, escribir `N/A`.

- **Commands**: `command_name(args) -> Result<T, AppError>`
- **Events**: `event.name.v1 { ... }`
- **Storage**: tabla/migración/campo
- **Errors**: `error_code`

## Acceptance Criteria

Cada AC debe ser observable y testeable. Usar `Given / When / Then`
cuando ayude.

- [ ] F<NN>.AC1: Given ..., When ..., Then ...
- [ ] F<NN>.AC2: Given ..., When ..., Then ...
- [ ] F<NN>.AC3: Given ..., When ..., Then ...

## Test Map

Cada AC implementado debe apuntar a test automatizado o verificación
manual explícita si no es automatizable.

- `F<NN>.AC1` -> `crate_or_file::f<NN>_ac1_<short>`
- `F<NN>.AC2` -> `ui/src/.../<test>.test.ts`
- `F<NN>.AC3` -> manual: <pasos cortos>

## No-gos

Qué queda fuera para mantener el alcance pequeño.

- ...

## Risks / Rabbit holes

Riesgos que pueden disparar coste o ambigüedad.

- ...

## Implementation notes

Opcional. Máximo 5 bullets. Usar solo después de implementar si deja
contexto útil para futuros cambios.

- ...

## References

- `specs/README.md`
- `specs/ipc.md` si aplica
- Dominios afectados
