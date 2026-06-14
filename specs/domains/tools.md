# Tools

**Status**: ready
**Owner**: @miglesias
**Last update**: 2026-06-14 (promovido `review → ready`: 6/19 ACs
cubiertos con tests; los 13 ACs de write/exec tools se
implementan con el workstream 2026-06-14 — ver
[`../STATUS.md#workstream-en-curso`](../STATUS.md#workstream-en-curso),
PR-2 a PR-4).
**Affects**: — (las tools son invocadas por `agent-loop.md` con
permisos decididos por `permissions.md`).
**Required by**: `agent-loop.md`, `features/F01-chat-streaming`,
`features/F02-multi-workspace` (vía `list_dir` y `read_file`),
`features/F03-python-venv` (vía `python_run`, opt-in en v1).

> Contrato común de las tools que el agente invoca + catálogo de las
> 8 tools de v1. Las tools son **puras desde el punto de vista del
> agent loop**: reciben `ToolContext` (workspace, run, abort,
> permisos) y devuelven `ToolOutput`. Toda side effect (journal,
> eventos) es responsabilidad del agent loop, no de la tool.

## Goal

Definir la **interfaz de las tools** (`Tool` trait), el mecanismo de
registro y lookup, y el comportamiento detallado de las **8 tools v1**
(`read_file`, `write_file`, `edit_file`, `search`, `shell`,
`python_run`, `list_dir`, `apply_patch`), con sus contratos de
seguridad (path sandboxing, timeouts, manejo de abort).

## Non-goals

- ❌ La orquestación de las tools (cuándo se llaman, en qué orden).
  Ver [`agent-loop.md`](./agent-loop.md).
- ❌ La decisión de permisos. Ver [`permissions.md`](./permissions.md).
- ❌ Tools custom definidas por el usuario. v2 (plugin system).
- ❌ Tools que se ejecutan en paralelo dentro del mismo step. v1:
  serie.
- ❌ Tool marketplace. v2.

## Glossary

Términos locales:

- **Tool**: una capacidad invocable por el agente, identificada por
  `name` (`snake_case`).
- **ToolContext**: input no-args de toda tool. Lleva `workspace_id`,
  `workspace_root`, `extra_paths`, `run_id`, `session_id`,
  `permission_decision`, `abort_flag`.
- **ToolOutput**: output normalizado de toda tool:
  `{ content: String, is_error: bool, metadata: Option<Value> }`.
- **Dangerous tool**: tool que muta estado del usuario o ejecuta
  código. `read_file`, `search`, `list_dir` **no** son dangerous;
  las demás sí (ver tabla en §Catalog).
- **Path sandboxing**: toda tool que recibe un `path` lo canonicaliza
  y verifica que está dentro de **`root_path ∪ extra_paths`** del
  workspace (no solo `root`). Si no, devuelve `path_traversal` sin
  tocar el filesystem. Ver
  [ADR-0007](../adr/0007-extra-paths-per-workspace.md).

## State

Las tools son **stateless** entre invocaciones. Todo el estado
necesario viene en `ToolContext` o en los args.

| Campo (en `ToolContext`) | Tipo | Notas |
|---|---|---|
| `workspace_id` | `WorkspaceId` | |
| `workspace_root` | `PathBuf` | Canonicalizado. |
| `extra_paths` | `Arc<Vec<PathBuf>>` | Canonicalizados, en orden de declaración. Path sandboxing = `root ∪ extras`. |
| `run_id` | `RunId` | |
| `session_id` | `SessionId` | |
| `permission_decision` | `Decision` | Eco de `Permissions::check`. |
| `abort_flag` | `Arc<AtomicBool>` | Para tools de larga duración (shell, python_run). |
| `timeout` | `Duration` | Default 30s para shell, configurable. |

## Tool trait

```rust
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;

    /// JSON schema de los args. Se publica al provider LLM para que
    /// pueda emitir tool calls válidas.
    fn schema(&self) -> serde_json::Value;

    /// Si la tool muta estado. Usado por `Permissions::check` para
    /// decidir si entra en `ask`.
    fn is_dangerous(&self) -> bool;

    async fn run(
        &self,
        ctx: ToolContext,
        args: serde_json::Value,
    ) -> Result<ToolOutput, AppError>;
}
```

### `ToolRegistry`

Colección estática de tools conocidas. Lookup por nombre.

```rust
pub fn registry() -> &'static [&'static dyn Tool] {
    &[
        &ReadFileTool,
        &WriteFileTool,
        &EditFileTool,
        &SearchTool,
        &ShellTool,
        &PythonRunTool,
        &ListDirTool,
        &ApplyPatchTool,
    ]
}
```

`AgentLoop` itera la registry, expone los `schema()` al provider, y
dispatch de tool calls por nombre.

## Catalog (8 tools v1)

Para cada tool: schema de args, output, errores, edge cases.

### `read_file`

- **Dangerous**: `false`.
- **Args**: `{ path: string, offset?: u32, limit?: u32 }`.
- **Output**: `{ content: string, total_lines: u32, returned_lines: u32 }`.
- **Errors**: `not_found`, `path_traversal`, `invalid_input`
  (file > 50 MB).
- **Path sandboxing**: el path se canonicaliza y se verifica dentro
  de `workspace_root ∪ extra_paths`. Si no, `path_traversal`.
- **Comportamiento**:
  - Lee como UTF-8. Si el archivo no es UTF-8 válido, devuelve
    `invalid_input` con detalle de la posición.
  - Si `offset` y `limit` están, lee solo el rango (en líneas).
  - `limit` default: 2000 líneas, max 100 000.

### `write_file`

- **Dangerous**: `true`.
- **Args**: `{ path: string, content: string, mode?: "overwrite"|"create_only"|"append" }`.
- **Output**: `{ bytes_written: u32, path: string }`.
- **Errors**: `path_traversal`, `invalid_input` (path no es texto,
  > 50 MB), `forbidden` (path en `deny_paths`, decidido en
  `Permissions::check`).
- **Path sandboxing**: idem `read_file`.
- **Comportamiento**:
  - Default `mode: "overwrite"`. Crea el archivo si no existe.
  - `create_only` falla con `conflict` si existe.
  - `append` concatena.
  - Crea directorios padre hasta `workspace_root` (sin escaparlo).
  - Para directorios padre **fuera** de `root`, requiere que estén
    dentro de algún `extra_path` declarado. Si no, `path_traversal`.
  - **No** sigue symlinks que salgan del workspace (ver §Edge 4).

### `edit_file`

- **Dangerous**: `true`.
- **Args**: `{ path: string, old_text: string, new_text: string, replace_all?: bool }`.
- **Output**: `{ replaced: u32, path: string }`.
- **Errors**: `path_traversal`, `not_found` (archivo no existe),
  `invalid_input` (`old_text` no aparece, o aparece más de una vez
  con `replace_all: false`).
- **Path sandboxing**: idem.
- **Comportamiento**:
  - Búsqueda literal de `old_text`. Si aparece N veces:
    - `replace_all: true` → reemplaza las N ocurrencias.
    - `replace_all: false` y N == 1 → OK.
    - `replace_all: false` y N > 1 → `invalid_input` con `count: N`.
  - El archivo debe ser texto UTF-8.

### `search`

- **Dangerous**: `false`.
- **Args**: `{ query: string, path?: string, glob?: string, regex?: bool, case_insensitive?: bool, max_results?: u32 }`.
- **Output**: `{ matches: SearchMatch[], truncated: bool }`. `SearchMatch = { file: string, line: u32, column: u32, text: string }`.
- **Errors**: `path_traversal`, `invalid_input` (`query` vacío o > 200 chars).
- **Path sandboxing**: el `path` (default `workspace_root`) se
  canonicaliza. Si está dentro de un `extra_path`, también es válido.
  Search **no** sigue symlinks.
- **Comportamiento**:
  - `regex: true` usa la crate `regex`. **No** se aplica a `query` si
    contiene caracteres sospechosos de ReDoS; ver §Edge 5.
  - `glob` se filtra con `globset`.
  - `case_insensitive: true` por defecto.
  - Respeta `ignore` patterns del workspace.
  - `max_results` default 100, max 10 000. Si se excede, `truncated: true`.
  - Ignora directorios listados en `ignore` y archivos > 10 MB.

### `shell`

- **Dangerous**: `true`.
- **Args**: `{ command: string, cwd?: string, timeout_ms?: u32, env?: Record<string, string> }`.
- **Output**: `{ stdout: string, stderr: string, exit_code: i32, duration_ms: u32 }`.
- **Errors**: `path_traversal` (`cwd` fuera de
  `workspace_root ∪ extra_paths`), `invalid_input` (`command` vacío,
  `timeout_ms` > 600_000), `timeout` (expiró el timeout).
- **Path sandboxing**: el `cwd` (default `workspace_root`) se
  canonicaliza. **Solo** se permite cwd dentro de
  `workspace_root ∪ extra_paths`.
- **Comportamiento**:
  - **No es PTY**. Para REPLs o color, usar `python_run` con PTY.
  - `timeout_ms` default 30 000 (30 s), max 600 000 (10 min).
  - `env` se **suma** al env actual; no reemplaza nada peligroso
    (`PATH`, `LD_PRELOAD`, `DYLD_INSERT_LIBRARIES` se filtran).
  - El abort_flag se chequea cada 100 ms. Si se activa, el child
    recibe `SIGTERM` y luego `SIGKILL` tras 2 s.
  - Output truncado a 1 MB por stream; si excede, queda un marker
    `... truncated ...` al final.

### `python_run`

- **Dangerous**: `true`.
- **Args**: `{ code: string, venv?: "auto"|"system"|{ path: string }, timeout_ms?: u32 }`.
- **Output**: `{ stdout: string, stderr: string, exit_code: i32, python_version: string, venv_kind: "Uv"|"Venv"|"System", duration_ms: u32 }`.
- **Errors**: `path_traversal`, `invalid_input` (code > 1 MB,
  timeout > 600 000, **`venv: "auto"` y workspace sin venv — con
  mensaje claro sugiriendo `workspace_create_venv` o usar
  `venv: "system"`**), `internal` (venv roto).
- **Path sandboxing**: si `venv: { path: string }`, el path se
  canonicaliza y se verifica dentro de
  `workspace_root ∪ extra_paths`.
- **Comportamiento**:
  - Resuelve el interpreter según ADR-0004 (auto → `Workspace::detect_venv`).
  - **Usa PTY** (ver `pty.md`) si el code parece interactivo
    (heurística: contiene `input()` o `sys.stdin`); si no, ejecución
    no-PTY como `shell`.
  - Variables de env del venv activadas (`VIRTUAL_ENV`,
    `PATH` reescrito). No se usa `source activate`; se ejecuta el
    binario directamente.
  - `timeout_ms` default 30 000, max 600 000.
  - `code` se guarda en un archivo temporal `.agentyx-python-<run_id>.py`
    en el workspace, ejecutado y borrado. Si el run aborta, el
    archivo temporal se limpia en el siguiente arranque (ver
    §Edge 6).
  - **Opt-in venv (v1)**: si `venv: "auto"` y el workspace no tiene
    `.venv/` ni `venv/`, la tool **no** auto-crea nada. Retorna
    `invalid_input` con:
    - `message`: "Workspace sin .venv. Crea uno con
      'workspace_create_venv' o usa venv: 'system'."
    - `context: { suggestion: "create_venv" | "use_system" }`.
    El modelo recibe el error y puede decidir qué hacer.

### `list_dir`

- **Dangerous**: `false`.
- **Args**: `{ path?: string, depth?: u32, include_hidden?: bool }`.
- **Output**: `{ entries: DirEntry[] }`. `DirEntry = { name: string, kind: "file"|"dir"|"symlink", size?: u32, children?: DirEntry[] }`.
- **Errors**: `path_traversal`, `not_found`.
- **Path sandboxing**: idem.
- **Comportamiento**:
  - `depth` default 1, max 5.
  - `include_hidden: false` por defecto (oculta los que empiezan por `.`).
  - Respeta `ignore` patterns.
  - No sigue symlinks (los lista como `symlink` con su target
    visible solo si está dentro del workspace).
  - Ordena: directorios primero, luego archivos, alfabético.

### `apply_patch`

- **Dangerous**: `true`.
- **Args**: `{ diff: string, dry_run?: bool }`.
- **Output**: `{ files_changed: u32, additions: u32, deletions: u32, preview?: string }`.
- **Errors**: `path_traversal`, `invalid_input` (diff malformado),
  `conflict` (archivo destino no coincide con el `--- a/...`).
- **Path sandboxing**: cada path en el diff se trata como
  `write_file`/`read_file`.
- **Comportamiento**:
  - Formato de diff: unified diff (`--- a/path`, `+++ b/path`,
    `@@ ... @@` hunks).
  - `dry_run: true` devuelve el `preview` sin escribir; útil para
    mostrar al usuario antes de aplicar.
  - Sin `dry_run` aplica todos los hunks en una **transacción**: si
    alguno falla, se hace rollback de todos.

## Operations (expuestas por el dominio)

### `ToolRegistry::lookup(name) -> Option<&'static dyn Tool>`

Lookup por nombre. Devuelve `None` si no existe.

### `ToolRegistry::schemas() -> Vec<ToolSchema>`

Devuelve los schemas de todas las tools. Se pasa al provider LLM
para que sepa qué tool calls puede emitir.

## Contracts

Las tools **no exponen** Tauri commands ni HTTP endpoints propios.
Son una capa interna que el agent loop invoca. Su resultado se
anuncia al UI como `chat.tool_result.v1` (ver `agent-loop.md`).

## Edge cases

1. **Tool llamada con `args` que no cumplen el schema**: el agent
   loop lo trata como `tool_result.v1 { isError: true, output:
   "invalid args: <detail del validator>"}`. La tool **no** se ejecuta.
2. **Tool llamada con nombre desconocido** (no en la registry): el
   agent loop emite `error.v1` y `tool_result.v1 { isError: true,
   output: "unknown tool: <name>" }`.
3. **Path con `..` literal en cualquier arg de path**: rechazado
   antes de cualquier I/O, en `Permissions::check` (paso 1 del
   algoritmo). La tool no ve el path.
4. **Symlink que apunta fuera del workspace**: la canonicalización
   (`std::fs::canonicalize`) lo resuelve al destino final. Si ese
   destino está fuera, `path_traversal`. **PROHIBIDO** saltarse
   `canonicalize` por performance.
5. **ReDoS en `search` con regex malicioso**: `regex` crate tiene
   un timeout interno. Si el matcher tarda > 2 s, se cancela y se
   devuelve `invalid_input` con detalle. `query` > 200 chars se
   rechaza de entrada.
6. **`python_run` deja archivo temporal huérfano** (kill, panic):
   el agent loop, al limpiar el run, llama a
   `python_run::cleanup_tmp(workspace_id)` que borra
   `.agentyx-python-*.py` del workspace. También se hace al
   arrancar la app.
7. **`shell` con comando destructivo** (`rm -rf /`, `dd of=/dev/...`):
   `Permissions::check` lo bloquea si está en `deny` o en
   `deny_paths` (si el `cwd` está). **No** hay heurística adicional
   para "comandos destructivos" en v1; queda a criterio del
   usuario + la matriz.
8. **Tool que excede el timeout**: el child recibe `SIGTERM`. Si
   tras 2 s no ha muerto, `SIGKILL`. El output queda con
   `duration_ms` cercano al timeout y `is_error: true`.
9. **Output de tool > 1 MB**: se trunca con marker. **No** se
   persiste completo en el journal; el journal guarda solo
   `{ tool, args_summary, duration_ms, exit_kind, is_error }` (ver
   `storage.md#edge5`).
10. **Tool llamada mientras el run está abortado**: el agent loop
    no invoca tools después de `abort`. Si por algún bug lo hace,
    la tool ve `abort_flag == true` y devuelve
    `internal { code: "aborted" }` sin hacer trabajo.

## Acceptance criteria

Cada AC → test con nombre derivado `ac<n>_<short>`.

- [ ] AC1: `read_file` con path dentro de `workspace_root` devuelve
  el contenido. **Test**: `ac1_read_file_returns_content`.
- [ ] AC2: `read_file` con path fuera de `workspace_root ∪ extra_paths`
  devuelve `path_traversal` sin tocar el filesystem (verificable
  con `tempdir` + assert no-leído). **Test**:
  `ac2_read_file_path_traversal`.
- [ ] AC3: `write_file` con `mode: "create_only"` y archivo
  existente devuelve `conflict`. **Test**:
  `ac3_write_file_create_only_conflict`.
- [ ] AC4: `edit_file` con `old_text` que aparece 3 veces y
  `replace_all: false` devuelve `invalid_input` con `count: 3`.
  **Test**: `ac4_edit_file_ambiguous_old_text`.
- [ ] AC5: `search` con regex maliciosa que tarda > 2 s devuelve
  `invalid_input`. **Test**: `ac5_search_regex_timeout`.
- [ ] AC6: `shell` con `timeout_ms: 100` y comando que duerme 5 s
  termina en ~100 ms con `is_error: true, exit_code: null`.
  **Test**: `ac6_shell_timeout_kills_child`.
- [ ] AC7: `shell` con `env: { "LD_PRELOAD": "evil.so" }` ignora la
  key (no se inyecta). **Test**:
  `ac7_shell_filters_dangerous_env`.
- [ ] AC8: `python_run` con `venv: "auto"` y workspace con `.venv/`
  usa el venv detectado y devuelve `venv_kind: "Uv"|"Venv"`.
  **Test**: `ac8_python_run_uses_workspace_venv`.
- [ ] AC9: `python_run` con `venv: "auto"` y workspace sin venv
  devuelve `invalid_input` con sugerencia (crear venv o usar system),
  y **no** auto-crea nada. **Test**:
  `ac9_python_run_no_venv_returns_invalid_input`.
- [ ] AC10: `apply_patch` con `dry_run: true` no escribe nada y
  devuelve `preview` con el diff. **Test**:
  `ac10_apply_patch_dry_run_no_writes`.
- [ ] AC11: `apply_patch` con un hunk que no matchea el archivo
  actual no aplica **ningún** hunk (transaccional). **Test**:
  `ac11_apply_patch_atomic_on_failure`.
- [ ] AC12: `list_dir` con `depth: 3` sobre un árbol de 5 niveles
  trunca en nivel 3. **Test**: `ac12_list_dir_respects_depth`.
- [ ] AC13: `list_dir` con `include_hidden: false` no lista
  `.git/`. **Test**: `ac13_list_dir_excludes_hidden`.
- [ ] AC14: tool llamada con `args` que no cumplen el schema
  devuelve `is_error: true` sin ejecutar. **Test**:
  `ac14_invalid_args_no_execution`.
- [ ] AC15: dos invocaciones concurrentes de la misma tool en
  distintos workspaces no se interfieren. **Test**:
  `ac15_concurrent_tools_isolated`.
- [ ] AC16: `read_file` con path **dentro de un extra_path declarado**
  devuelve el contenido (verificable: workspace con root `/proj` y
  extra `/assets`, `read_file("/assets/foo.png")` → OK). **Test**:
  `ac16_read_file_in_extra_path_succeeds`.
- [ ] AC17: `write_file` con path en un extra_path escribe el archivo
  y devuelve `{ bytes_written, path }`. **Test**:
  `ac17_write_file_in_extra_path_succeeds`.
- [ ] AC18: `write_file` con path en directorio padre que está fuera
  de `root ∪ extras` (p. ej. `/etc/...` o un path en `..`) devuelve
  `path_traversal` sin crear directorios. **Test**:
  `ac18_write_file_parent_outside_sandbox_rejected`.
- [ ] AC19: `python_run` con `venv: { path: "/path/in/extra" }` donde
  `/path/in/extra` es un extra_path declarado acepta el venv
  explícito. **Test**:
  `ac19_python_run_explicit_venv_in_extra_path`.

## Discovered bugs (post-approval)

| ID | Date | Category | Resolved in | Notes |
|---|---|---|---|---|
| _ninguno aún_ | | | | |

## Open questions

- **Q1**: ¿Las tools deberían poder emitir eventos propios (no
  esperar al `tool_result` del agent loop)? → **Propuesta v1**: no.
  El agent loop es el único emisor. Si una tool quiere progreso
  fino, lo acumula en `ToolOutput::metadata` y se emite al final.
- **Q2**: ¿Tool `apply_patch` debería aceptar el formato
  opencode-dev (que tiene su propia sintaxis) o unified diff
  estándar? → **Propuesta v1**: unified diff estándar (compatible
  con `git diff`). Si opencode-dev demuestra que su formato es
  mejor, se reescribe el parser en v2.
- **Q3**: ¿Tool que necesite GPU (e.g. inferencia local) — cómo
  rate-limita? → **Propuesta v1**: rate-limit es del provider, no
  de las tools. `python_run` no tiene rate-limit (responsabilidad
  del user).
- **Q4**: ¿`shell` debería tener un **conjunto de comandos seguros**
  (allowlist) en vez de solo `deny`/`deny_paths`? → **Propuesta v1**:
  no. Allowlist es muy restrictivo y rompe flujos comunes (`git`,
  `cargo`, `npm`, …). v2 quizá, con un parser más serio.

## References

- [`../architecture.md`](../architecture.md) — flujo de tool call.
- [`agent-loop.md`](./agent-loop.md) — el caller.
- [`permissions.md`](./permissions.md) — el decisor.
- [`workspace.md`](./workspace.md) — `root_path` y `extra_paths` para
  sandboxing.
- [`pty.md`](./pty.md) — wrapper de PTY que `python_run` usa.
- [`../adr/0004-detect-venv-priority.md`](../adr/0004-detect-venv-priority.md) —
  resolución de venv.
- [`../adr/0007-extra-paths-per-workspace.md`](../adr/0007-extra-paths-per-workspace.md) —
  modelo `root + extra_paths`.
