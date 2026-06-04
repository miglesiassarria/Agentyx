# PTY (Pseudo-Terminal)

**Status**: draft
**Owner**: @miglesias
**Last update**: 2026-06-04
**Affects**: — (la PTY es consumida por `tools.md` cuando una tool
  requiere TTY).
**Required by**: `tools.md` (vía `python_run` interactivo y `shell`
  con heurística PTY), futuro terminal embebido en la UI
  (vía `xterm.js`).

> Wrapper multiplataforma sobre `portable-pty` (ver
> [ADR-0005](../adr/0005-pty-portable-pty.md)). Ofrece un handle
> async-friendly con `spawn` / `write` / `resize` / `kill` y emite
> los outputs como eventos `pty.output.v1` / `pty.exit.v1`.

## Goal

Proveer un PTY (ConPTY en Windows, openpty en macOS/Linux) con API
async sobre el que `python_run` (modo interactivo) y `shell` (con
heurística) puedan delegar. **Toda** la I/O con la PTY se streamea
como evento, no se acumula en memoria.

## Non-goals

- ❌ Decidir **cuándo** usar PTY. Eso es de la tool
  (`python_run` / `shell`); este dominio solo provee la capacidad.
- ❌ Render del output en la UI. Eso es `PtyTerminal.svelte` con
  `xterm.js` (futuro).
- ❌ Persistir sesiones PTY tras cerrar la app. Las PTYs son
  efímeras; al cerrar Agentyx, los child se matan.
- ❌ Soporte de PTY en WebSockets desde el navegador. v1: el
  cliente HTTP solo puede **ver** el output, no enviar input. v2:
  websocket bidireccional.
- ❌ Forwarding de señales distintas a SIGTERM/SIGKILL.

## Glossary

Términos locales:

- **PtyHandle**: `Arc<Mutex<PtyState>>` con `id`, `master`,
  `writer`, `child`, `metadata`.
- **PtyId**: ULID, expuesto en eventos.
- **PTY output**: stream de bytes (generalmente UTF-8 con secuencias
  ANSI para color). Lo emitimos como `pty.output.v1` con payload
  base64.
- **PTY child**: el proceso spawneado (e.g. `python -i`).

## State

In-memory, en `AppState` (`crates/agentyx-app/src/state.rs`):

| Campo | Tipo | Notas |
|---|---|---|
| `ptys` | `RwLock<HashMap<PtyId, PtyHandle>>` | Activas. |
| `next_id` | `Mutex<ULID>` | Generador. |
| `default_size` | `(cols: u16, rows: u16)` | Default 80×24. |

**Sin** estado persistente. Las PTYs son volátiles.

## Tool trait / interface

```rust
pub struct PtySpec {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,                 // canonicalizado, dentro de workspace_root
    pub env: HashMap<String, String>, // filtrado (ver §Edge 7)
    pub cols: u16,                    // default 80
    pub rows: u16,                    // default 24
    pub timeout: Option<Duration>,    // opcional
}

pub struct PtyInfo {
    pub id: PtyId,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub started_at: DateTime<Utc>,
    pub pid: Option<u32>,             // del child
    pub cols: u16,
    pub rows: u16,
}

pub enum PtyExit {
    Normal(i32),
    Killed,
    SpawnError(String),
}
```

## Operations

### `Pty::spawn(spec) -> Result<PtyInfo, AppError>`

Arranca el child en una PTY.

**Pasos**:
1. `portable_pty::native_pty_system().openpty(specs)`.
2. `slave.spawn_command(cmd)` con `command = spec.command`,
   `args = spec.args`, `cwd = spec.cwd`, `env = spec.env`.
3. Obtener `master` y `child_pid`.
4. Spawn de **dos tareas**:
   - **Reader task** (`tokio::spawn`): lee del master con
     `tokio::task::spawn_blocking`, emite `pty.output.v1` con
     chunks base64.
   - **Waiter task** (`tokio::spawn`): espera al child con
     `child.wait()`, emite `pty.exit.v1` al terminar.

**Output**: `PtyInfo`.

**Errores**:
- `not_found` (el `cwd` no existe; ya validado en `Permissions`).
- `invalid_input` (command vacío, args > 1000, cols/rows fuera de
  1..10000).
- `internal` (PTY no disponible, fallo al spawn).
- `permission_denied` (cwd en `deny_paths`).

### `Pty::write(id, data: &[u8]) -> Result<(), AppError>`

Escribe bytes al stdin del child.

**Errores**:
- `not_found` (PTY id desconocido o ya cerrada).
- `internal` (master cerrado / error de I/O).

### `Pty::resize(id, cols, rows) -> Result<(), AppError>`

Cambia el tamaño de la PTY. Internamente envía `TIOCSWINSZ` en Unix
o el equivalente ConPTY en Windows.

**Errores**:
- `not_found`.
- `invalid_input` (cols/rows fuera de 1..10000).
- `internal` (resize no soportado en la plataforma).

### `Pty::kill(id, grace_ms: u32) -> Result<(), AppError>`

Mata el child. `grace_ms` default 2000:
- Inmediatamente: `SIGTERM` (Unix) o `TerminateProcess` (Windows).
- Tras `grace_ms`: `SIGKILL` (Unix) o `TerminateProcess` (Windows,
  hard kill).

**Errores**:
- `not_found`.

### `Pty::list() -> Vec<PtyInfo>`

Snapshot de las PTYs activas. Usado por la UI y por `Pty::cleanup_on_exit`.

### `Pty::cleanup_on_exit() -> ()`

Lo llama `agentyx-app` al cerrar la app. Mata todas las PTYs con
`grace_ms: 500` (no espera mucho al cerrar).

## Contracts

### Tauri commands

| Command | Notas |
|---|---|
| `pty_spawn(spec) -> PtyInfo` | |
| `pty_write(id, data: string /* base64 */) -> ()` | |
| `pty_resize(id, cols, rows) -> ()` | |
| `pty_kill(id) -> ()` | |
| `pty_list() -> PtyInfo[]` | |

### HTTP endpoints

`POST /api/v1/pty` → `PtyInfo`
`POST /api/v1/pty/:id/write` (body: `{ data: base64 }`) → `{}`
`POST /api/v1/pty/:id/resize` (body: `{ cols, rows }`) → `{}`
`DELETE /api/v1/pty/:id` → `{}`
`GET  /api/v1/pty` → `PtyInfo[]`

### Eventos streaming

(ver `../ipc.md` §3)

| Evento | Cuándo | Payload |
|---|---|---|
| `pty.output.v1` | Cada chunk leído del master | `{ ptyId, data: string /* base64 */ }` |
| `pty.exit.v1` | Cuando el child termina o es matado | `{ ptyId, exit: PtyExit }` |

El reader task hace **flush** cada 16 ms o cada 4 KB, lo que llegue
primero. Esto balancea latencia vs throughput.

## Edge cases

1. **Child termina inesperadamente** (`exit_code != 0` o señal):
   `pty.exit.v1 { exit: Normal(code) }` o
   `pty.exit.v1 { exit: Killed }`.
2. **Child que nunca termina** (hang): el waiter task no emite
   `pty.exit.v1` por sí solo. El usuario debe `pty_kill`. v1 no
   tiene watchdog de "si > N min sin output, matar".
3. **Resize durante un read activo**: el read actual termina con
   el tamaño anterior; el siguiente read ve el nuevo. **No** se
   descarta output a mitad de chunk.
4. **Writes concurrentes a la misma PTY**: el `writer` está
   protegido por un `Mutex` interno. Writes se serializan.
5. **Output rate muy alto** (e.g. `yes` con 1 MB/s): el reader
   emite en chunks de 4 KB. La UI hace backpressure vía SSE (no se
   publica más rápido de lo que el cliente HTTP puede consumir).
6. **Windows pre-1809 sin ConPTY**: la versión mínima soportada es
   Windows 10 build 1809+. Builds más antiguas: `internal { code:
   "conpty_not_available" }` con mensaje claro. (ver
   `../project.md` non-goal de Win 7/8.)
7. **Env con variables peligrosas** (`LD_PRELOAD`,
   `DYLD_INSERT_LIBRARIES`, `PATH`): filtradas. **Solo** se permite
   `PATH` si se pasa explícitamente y se loguea con `tracing::warn!`.
8. **Output no-UTF-8** (child escribe bytes binarios): el reader
   lee bytes puros, los pasa a base64. La UI decide si renderiza
   como texto o como hex. **No** se hace `String::from_utf8_lossy`
   en el core.
9. **Master drop por el child que cerró stdout**: el read devuelve
   EOF. El waiter detecta que el child terminó. `pty.exit.v1` se
   emite con el `exit_code` real.
10. **PTY con `command` que no existe** (`/bin/nosuch`): el spawn
    falla con `internal { code: "spawn_failed" }` y stderr del
    propio `portable_pty` quejándose del ENOENT.

## Acceptance criteria

Cada AC → test con nombre derivado `ac<n>_<short>`.

- [ ] AC1: `spawn` con un comando válido (e.g. `echo hola`) arranca
  una PTY, emite `pty.output.v1` con "hola\n", y luego
  `pty.exit.v1` con `exit_code: 0`. **Test**:
  `ac1_spawn_echo_completes`.
- [ ] AC2: `write` con `data: "user\n"` a un `cat` (sin args) hace
  eco del input y se emite como `pty.output.v1`. **Test**:
  `ac2_write_is_echoed`.
- [ ] AC3: `resize(40, 20)` en una PTY activa con `tput cols` se
  refleja: el child ve 40 cols. **Test**:
  `ac3_resize_propagates_to_child`.
- [ ] AC4: `kill` con `grace_ms: 100` sobre un child que ignora
  SIGTERM (e.g. `trap '' TERM; sleep 60`) lo mata en ~100 ms.
  **Test**: `ac4_kill_uses_sigkill_after_grace`.
- [ ] AC5: `spawn` con `cwd` fuera de `workspace_root` devuelve
  `permission_denied` (delegado a `Permissions::check`). **Test**:
  `ac5_spawn_cwd_outside_workspace`.
- [ ] AC6: `spawn` con `env: { "LD_PRELOAD": "evil.so" }` no
  propaga `LD_PRELOAD` al child (verificable con
  `printenv LD_PRELOAD` dentro del child). **Test**:
  `ac6_spawn_filters_ld_preload`.
- [ ] AC7: dos PTYs concurrentes con el mismo comando no se
  interfieren: cada una tiene su `id` y su reader. **Test**:
  `ac7_concurrent_ptys_isolated`.
- [ ] AC8: `list` devuelve las PTYs activas en orden de creación.
  **Test**: `ac8_list_returns_active_ptys`.
- [ ] AC9: `pty_kill` sobre un id desconocido devuelve `not_found`.
  **Test**: `ac9_kill_unknown_id_returns_not_found`.
- [ ] AC10: `pty.output.v1` payload es base64, decodifica a los
  bytes originales verbatim (no se altera UTF-8). **Test**:
  `ac10_output_payload_preserves_bytes`.
- [ ] AC11: `cleanup_on_exit` mata todas las PTYs en < 1 s. **Test**:
  `ac11_cleanup_on_exit_kills_all`.

## Discovered bugs (post-approval)

| ID | Date | Category | Resolved in | Notes |
|---|---|---|---|---|
| _ninguno aún_ | | | | |

## Open questions

- **Q1**: ¿Soporte de resize durante un read activo puede generar
  artifacts visuales en la UI? → **Propuesta v1**: no testeamos
  este detalle; queda a la UI. Si se ve mal, se mejora el protocol
  en v2.
- **Q2**: ¿PTY que recibe `EOF` del master y luego se quiere
  reusar? → **Propuesta v1**: no. Una PTY cerrada no se reabre.
- **Q3**: ¿Encoding distinto a UTF-8? → **Propuesta v1**: la PTY
  emite bytes puros. La UI decide. v1 no tiene heurística
  auto-detect de encoding.
- **Q4**: ¿PTY que necesita `sudo` (sin password)? → **Propuesta
  v1**: **fuera de scope**. El user debe preconfigurar sudoers NOPASSWD
  si lo necesita. Riesgo de seguridad tratar sudo en v1.

## References

- [`../adr/0005-pty-portable-pty.md`](../adr/0005-pty-portable-pty.md) — decisión de stack.
- [`../architecture.md`](../architecture.md) — dónde encaja el PTY.
- [`tools.md`](./tools.md) — `python_run` y `shell` consumen PTY.
- [`permissions.md`](./permissions.md) — `cwd` validado allí.
- [`../ipc.md`](../ipc.md) — eventos `pty.output.v1`, `pty.exit.v1`.
- Web: <https://docs.rs/portable-pty>.
