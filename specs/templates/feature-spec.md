# Feature Spec — Plantilla

**Status**: draft
**Owner**: @<nick>
**Last update**: YYYY-MM-DD
**Affects**: lista de specs de dominio que esta feature consume
**Depends on**: lista de features (F<NN>) de las que depende

> Copiar este archivo a `specs/features/F<NN>-<slug>.md` (con el
> siguiente número libre) y rellenar. **Toda feature debe referenciar
> al menos un dominio en `Affects:`**.

## User story

Como **<rol>**, quiero **<acción>**, para **<beneficio>**.

## Scope

- **In-scope**: qué entra.
- **Out-of-scope**: qué NO entra. (Mejor explícito que implícito.)

## UX / UI

- Pantallas afectadas (rutas de Svelte, componentes).
- Estados: `empty`, `loading`, `error`, `success`, etc.
- Bocetos en ASCII o links a Figma si los hay.
- Strings visibles al usuario (i18n futura).

## Flow

Diagrama de secuencia: `user → UI → IPC → core → side-effect`.

```
user: "<acción>"
  → UI (Svelte) llama ipc.invoke("command_name", { ... })
  → Tauri command / HTTP handler
  → core: domain::operation
  → side effect (file, network, pty, …)
  → journal.append
  → emit evento chat.tool_result.v1
  → UI actualiza
```

## Affected domains

- `domains/<x>.md` — qué cambia, qué operations se usan.
- `domains/<y>.md` — idem.
- ...

## Affected Tauri commands / endpoints / events

Ver [ipc.md](../ipc.md).

- Commands: lista.
- HTTP endpoints: lista.
- Eventos: lista.

## Acceptance criteria

Cada item es **medible y testeable**. El test del AC vive en el crate
de Rust o de TS correspondiente, con nombre derivado del AC.

- [ ] F<NN>.AC1: ...
- [ ] F<NN>.AC2: ...
- [ ] F<NN>.AC3: ...

## Tests

- **Unit (Rust)**: `crates/agentyx-core/src/<scope>/<file>.rs::tests`.
  Naming: `f<NN>_ac<n>_<short>`.
- **Integration (Rust)**: `crates/agentyx-core/tests/<scope>.rs`.
- **Unit (TS)**: `ui/src/lib/components/<X>.test.ts`.
- **E2E (Playwright)**: `ui/e2e/<feature>.spec.ts`.

## Telemetry / logs

Qué se loguea, con qué nivel y qué campos. Recordar: **nunca** loguear
contenido de archivos del usuario ni secrets.

```rust
tracing::info!(
    workspace_id = %id,
    duration_ms = ms,
    "feature completed"
);
```

## Security notes

- ¿Toca auth, permisos, secrets, path traversal?
- ¿Cambia capabilities Tauri o CSP?

## Rollout

- ¿Feature flag?
- ¿Detrás de settings del workspace o global?
- ¿Migración de datos necesaria?

## Open questions

- ...

## References

- [ipc.md](../ipc.md)
- Dominios afectados (ver `Affects:` arriba)
