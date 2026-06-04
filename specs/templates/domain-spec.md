# Domain Spec — Plantilla

**Status**: draft
**Owner**: @<nick>
**Last update**: YYYY-MM-DD
**Affects**: (otros dominios o features que consumen este)
**Required by**: (features o dominios que dependen de este — opcional)

> Copiar este archivo a `specs/domains/<nombre>.md` y rellenar cada
> sección. Las secciones marcadas como **obligatorias** no pueden
> quedar vacías. Las marcadas como **opcional** pueden borrarse si
> no aplican.

## Goal

Una frase describiendo qué resuelve este dominio. Qué problema existe
hoy y cómo este dominio lo elimina.

## Non-goals

- Qué NO entra (explícito). Mejor sobre-especificar que dejar zonas grises.

## Glossary

Términos locales con definición corta, si este dominio introduce alguno
que no esté ya en [specs/glossary.md](../glossary.md).

## State

Qué estado persiste y dónde:

| Dato | Ubicación | Quién lee | Quién escribe |
|---|---|---|---|
| ... | SQLite / archivo / memoria | módulo | módulo |

Migraciones de SQLite (si aplica): archivo `.sql` con nombre incremental
bajo `crates/agentyx-core/src/storage/migrations/`.

## Operations

Lista de operaciones con:

- **Firma**: `OpName(input) -> Result<output, AppError>` (Rust style).
- **Errores posibles**: lista de variantes de `AppError` con precondiciones.
- **Permisos requeridos**: variante de `Permission` necesaria.
- **Efectos colaterales**: journal, eventos, IPC, I/O.

```rust
pub async fn operation_name(
    &self,
    input: Input,
) -> Result<Output, AppError> {
    // ...
}
```

## Contracts

### Tauri commands

```rust
#[tauri::command]
pub async fn command_name(
    state: tauri::State<'_, AppState>,
    input: Input,
) -> Result<Output, AppError> { ... }
```

Ver [ipc.md](../ipc.md) para convenciones (snake_case, camelCase en TS,
errores como `{code, message, context?}`).

### Endpoints HTTP

`POST /api/v1/<scope>/<action>` → `Output` (JSON).

Ver [ipc.md](../ipc.md) §4 para el versionado y códigos de error HTTP.

### Eventos streaming

| Evento | Schema | Payload | Cuándo se emite |
|---|---|---|---|
| `<domain>.event.v1` | `{ ... }` | `...` | ... |

Ver [ipc.md](../ipc.md) §3 para convenciones (`schema_version`, etc.).

### Tablas / archivos

```sql
CREATE TABLE example (
  id TEXT PRIMARY KEY,
  ...
);
```

## Edge cases

Lista de casos borde y comportamiento esperado. **Cada uno debería ser
un test**.

- Edge case 1 → comportamiento esperado.
- Edge case 2 → comportamiento esperado.

## Acceptance criteria

Cada item debe ser **testeable**. Naming de tests derivado del AC
(`ac3_returns_null_on_empty_input`).

- [ ] AC1: ...
- [ ] AC2: ...
- [ ] AC3: ...

## Discovered bugs (post-approval)

Se rellena automáticamente por los PRs de fix. **No borrar**.

| ID | Date | Category | Resolved in | Notes |
|---|---|---|---|---|
| _ninguno aún_ | | | | |

## Open questions

Decisiones pendientes con opciones y trade-offs. Si se cierra una
pregunta, moverla al ADR correspondiente (si es de stack) o a "Decisions"
dentro de esta spec (si es de comportamiento).

- Q1: ...
- Q2: ...

## References

- [ipc.md](../ipc.md)
- [architecture.md](../architecture.md)
- [glossary.md](../glossary.md)
- Specs de dominios relacionados
