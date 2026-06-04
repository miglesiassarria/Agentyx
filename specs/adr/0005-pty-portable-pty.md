# ADR-0005 — PTY con `portable-pty`

**Status**: accepted
**Date**: 2026-06-04
**Deciders**: @miglesias

## Context

Necesitamos un PTY (pseudo-terminal) multiplataforma para:

- `python -i` (REPL interactivo con color).
- `pytest` con output coloreado.
- Cualquier comando del usuario que requiera TTY.
- (Futuro) terminal embebido en la UI (`xterm.js` consume el stream).

Las opciones en el ecosistema Rust:

- **`portable-pty`** — abstracción multiplataforma sobre ConPTY (Windows),
  openpty (Unix). API un async-friendly.
- **`wezterm-pty`** — parte del stack de Wezterm, robusta.
- **`nix`-only** (`/dev/ptmx`) en Unix + bindings nativas en Windows →
  escribir el wrapper nosotros.
- **Lanzar `script` Unix + winpty Windows** — fragile y abandonado.

## Decision

**Adoptamos `portable-pty`** como crate de PTY.

## Status

`accepted`.

## Consequences

### Positivas
- **Multiplataforma real** desde el día uno (ConPTY en Windows, openpty
  en macOS/Linux).
- **Maneja resize y señales** correctamente por plataforma.
- **API limpia** (`native_pty_system.spawn(cmd)`) sobre la que podemos
  envolver con un wrapper async-friendly.
- **Mantenimiento activo**.

### Negativas
- **Bindings nativas en runtime** (`.so`/`.dylib`/`.dll` para ConPTY).
  Hay que asegurar que el binario distribuya las DLLs correctas en
  Windows (vía `tauri.bundle.resources` o similar).
- **No es 100 % async**: las APIs son síncronas. Hay que envolver en
  `tokio::task::spawn_blocking` para no bloquear el runtime.
- **El manejo de tamaño de ventana** (`resize`) requiere enviar
  señales específicas al child; `portable-pty` lo hace, pero hay que
  probarlo en cada plataforma.

### Neutras
- Wrapper interno en `crates/agentyx-core/src/pty/` para提供一个
  surface async limpio a los commands Tauri y al UI.
- Stream de output: enviamos como evento `pty.output.v1` con payload
  base64 (ver [ipc.md](../ipc.md) §3.2).

## Alternatives considered

### Alternative A: `wezterm-pty`
- Pros: muy robusto, mismos autores que Wezterm.
- Cons: surface más compleja; arrastra dependencias de Wezterm.
- **Por qué se descartó**: `portable-pty` es **más liviano** y su
  surface encaja con nuestro caso (no necesitamos el escape parser
  completo de Wezterm).

### Alternative B: `nix` (Unix) + bindings nativas Windows propias
- Pros: control total, sin dependencias externas.
- Cons: 2× trabajo, 2× bugs multiplataforma.
- **Por qué se descartó**: el ecosistema ya resolvió esto.

### Alternative C: `script` (Unix) + `winpty` (Windows)
- Pros: nada de Rust.
- Cons: `winpty` está discontinuado, `script` no existe en Windows.
  No es mantenible.
- **Por qué se descartó**: legacy.

## Implementation notes

- Wrapper en `crates/agentyx-core/src/pty/mod.rs` con:
  - `PtyHandle` (id ULID, writer, reader, master).
  - `spawn(command, args, cwd, env) -> PtyHandle`.
  - `write(PtyHandle, bytes)`.
  - `resize(PtyHandle, cols, rows)`.
  - `kill(PtyHandle)`.
- Output reader: tarea dedicada que lee del master y emite
  `pty.output.v1` (base64 del chunk).
- En Windows: asegurar que las DLLs de ConPTY se distribuyen.

## References

- [architecture.md](../architecture.md) — `pty::*` en el core.
- [ipc.md](../ipc.md) — eventos `pty.output.v1`, `pty.exit.v1`.
- Spec de dominio: `domains/pty.md` (pendiente).
- Web: <https://docs.rs/portable-pty>.
