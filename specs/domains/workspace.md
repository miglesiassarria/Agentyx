# Workspace

**Status**: draft
**Owner**: @miglesias
**Last update**: 2026-06-04
**Affects**: — (workspace es el contenedor raíz de todo el estado por proyecto).
**Required by**: `session.md` (FK), `tools.md` (path sandboxing),
`agent-loop.md` (tool execution context), `features/F02-multi-workspace`
(spec de feature principal).

> Un workspace = una carpeta elegida por el usuario. Agentyx la trata
> como unidad de aislamiento: su config, su historial, su `.venv` (si
> lo tiene), sus permisos. La detección de `.venv` se delega a este
> dominio (ver [ADR-0004](../adr/0004-detect-venv-priority.md)).

## Goal

Definir el **ciclo de vida de un workspace** (registro, apertura,
configuración, cierre, borrado) y la **detección / creación de su
`.venv`**, con garantías de seguridad (path traversal bloqueado,
canonicalización) y portabilidad (Windows, macOS, Linux).

## Non-goals

- ❌ Sesiones, mensajes, journal. Ver [`session.md`](./session.md).
- ❌ Schema SQLite del `state.db`. Ver [`storage.md`](./storage.md).
- ❌ Tools y su ejecución. Ver [`tools.md`](./tools.md).
- ❌ Permisos. Ver [`permissions.md`](./permissions.md).
- ❌ Sincronización del workspace entre dispositivos. v2.
- ❌ Soporte de conda. v2 (ver ADR-0004).
- ❌ Multi-usuario sobre el mismo workspace. v2.
- ❌ File watcher (notificar a la UI cambios de archivos). v2.

## Glossary

Términos locales:

- **Workspace root**: path absoluto canonicalizado del workspace.
  Fuente de verdad. Todo path que se opere dentro de Agentyx debe
  resolver dentro de este root.
- **VenvSpec**: `{ kind: VenvKind, path: PathBuf, python: PathBuf,
  version: String }` que describe un venv detectado o recién creado.
- **VenvKind**: `Uv` (gestionado por `uv`) | `Venv` (`python -m venv`).
- **Backend**: `uv` o `python -m venv`. Solo `uv` y `venv` en v1.
- **Cache dir**: `~/.agentyx/cache/<workspace-hash>/` para índices
  precomputados. Se puede borrar sin perder estado (se reconstruye).

## State

### Archivos por workspace

```
~/.agentyx/
├── state.json                                    # registry global
├── workspaces/
│   └── <workspace_id>/
│       ├── config.toml                           # config del workspace
│       ├── state.db                              # SQLite (storage.md)
│       ├── journal.jsonl                         # opcional, alt a DB
│       └── .last_opened                          # mtime touch
└── cache/
    └── <workspace-hash>/                         # índices cacheados
```

### `state.json` (registro global)

```json
{
  "version": 1,
  "workspaces": [
    {
      "id": "01HXXXXX…",
      "root_path": "/Users/…/myproject",
      "name": "myproject",
      "created_at": 1717500000000,
      "last_opened_at": 1717590000000
    }
  ],
  "server": {
    "lan_enabled": false,
    "bind": "127.0.0.1"
  }
}
```

### `config.toml` por workspace

```toml
version = 1
name = "myproject"
created_at = 1717500000000

# Provider y modelo por defecto (override posible por sesión)
[provider]
id = "ollama"
model = "llama3.1:8b"

# Si está presente, Agentyx usa este venv en vez de auto-detectar.
# Si está ausente, se aplica el orden de ADR-0004.
[venv]
path = ""                   # vacío = auto-detect
backend = "uv"              # default si está `uv` instalado

# Patrones a ignorar (para search y file watcher)
ignore = [
  ".git",
  "node_modules",
  "target",
  "__pycache__",
  ".venv",
  "venv",
  "dist",
  "build",
  ".next",
  ".cache",
]

# Permisos por defecto del workspace (ver permissions.md)
[permissions]
allow = ["read_file", "search", "list_dir"]
deny_paths = []
ask = ["write_file", "edit_file", "shell", "python_run"]
```

### Tabla `workspaces` en `state.db`

(Ver [`storage.md`](./storage.md) §State.)

```sql
CREATE TABLE workspaces (
  id              TEXT PRIMARY KEY,
  root_path       TEXT NOT NULL UNIQUE,
  name            TEXT NULL,
  created_at      INTEGER NOT NULL,
  last_opened_at  INTEGER NOT NULL
);
```

Es **local a cada `state.db`** y es informativa (eco del registry
global). Si difiere del `state.json`, el registry global gana.

## Operations

### `Workspace::list() -> Vec<WorkspaceInfo>`

Lee `state.json` y devuelve todos los workspaces registrados,
ordenados por `last_opened_at DESC`.

**Errores**:
- `internal` si `state.json` está corrupto (ver §Edge case 4).

### `Workspace::open(path: &Path) -> Result<WorkspaceInfo, AppError>`

Registra un nuevo workspace (o re-registra uno existente con el
mismo `root_path`).

Pasos:
1. Canonicaliza `path` con `std::fs::canonicalize`.
2. Verifica que el path existe y es directorio.
3. Verifica que está dentro de `~/`, `/Users/`, `/home/`, `C:\Users\`,
   `C:\Proyectos\`, `C:\Code\`, `~/Projects/` — **whitelist de
   raíces permitidas** (configurable; ver §Open questions Q1).
4. Genera `id` ULID.
5. Crea `~/.agentyx/workspaces/<id>/` con `config.toml` por defecto
   y `state.db` vacío (vía `Db::open`).
6. Inserta entrada en `state.json` y en `workspaces` (DB).
7. Toca `.last_opened`.

**Errores**:
- `path_traversal` (path no canonicaliza dentro de la whitelist).
- `not_found` (path no existe).
- `invalid_input` (no es directorio, o permisos insuficientes).
- `conflict` (otro workspace con el mismo `root_path` ya existe;
  devuelve el existente).

### `Workspace::get(id) -> Result<WorkspaceInfo, AppError>`

Lee del registry.

**Errores**: `not_found`.

### `Workspace::delete(id, force: bool) -> ()`

Borra el workspace.

Si `force=false` y la `state.db` tiene sesiones en estado `running`,
rechaza con `conflict` ("cierra los runs antes"). Si `force=true`,
aborta los runs (vía `AgentLoop::abort`) y luego borra.

**Pasos**:
1. Si `force=true`, aborta todos los runs activos del workspace.
2. Borra `~/.agentyx/workspaces/<id>/` y
   `~/.agentyx/cache/<workspace-hash>/`.
3. Quita entrada de `state.json` y de la tabla `workspaces`.

**Errores**:
- `not_found`.
- `conflict` (con `force=false` y runs activos).
- `internal` (no se pudo borrar; cleanup parcial, ver §Edge case 3).

### `Workspace::detect_venv(id) -> Result<Option<VenvSpec>, AppError>`

Implementa el orden de detección de [ADR-0004](../adr/0004-detect-venv-priority.md).

Orden (primer match gana):
1. `config.venv.path` explícito (override).
2. `<root>/.venv/` (convención uv / reciente).
3. `<root>/venv/` (convención histórica).
4. `<root>/.python-version` (pyenv) → resolver interpreter.
5. `<root>/pyproject.toml` con `[tool.uv]` o `[tool.poetry]` o
   `[project]` → venv del gestor si existe, si no
   `<root>/.venv/bin/python` si existe.
6. `uv.lock` / `poetry.lock` → sugiere venv del gestor; si no
   existe, retorna `None` y `tracing::info!` con sugerencia.
7. `conda-env.yml` / `environment.yml` → `None` +
   `tracing::warn!` "conda no soportado en v1".
8. Nada → `None` y nada más (sin side effects).

**Output**:
```rust
pub struct VenvSpec {
    pub kind: VenvKind,      // Uv | Venv
    pub path: PathBuf,       // el directorio del venv
    pub python: PathBuf,     // bin/python o Scripts\python.exe
    pub version: String,     // "3.12.1" vía `python --version`
}
```

**Errores**: `not_found` (workspace no existe). No escribe nada en
disco. Cache en memoria por `(root, mtime de los marcadores)`.

### `Workspace::create_venv(id, backend) -> Result<VenvSpec, AppError>`

Crea un `.venv` en el workspace. **Solo lo llama el usuario** (botón
"Crear venv aquí" en la UI). Nunca se invoca implícitamente.

**Pasos**:
1. `uv venv .venv` o `python -m venv .venv` según `backend`.
2. Verifica que `python` del venv arranca y reporta versión.
3. Inserta `journal(kind=workspace.venv_created)`.

**Errores**:
- `not_found` (workspace no existe).
- `invalid_input` (backend desconocido).
- `internal` (`uv`/`python` no encontrado).
- `internal` (permiso denegado, disco lleno, etc., con `tracing::error!`).
- `conflict` (`.venv` ya existe; usar `Workspace::detect_venv`).

### `Workspace::set_config(id, key, value) -> ()`

Escribe en `config.toml`. Refused keys: `id`, `created_at`
(inmutables). Si el TOML resultante es inválido, devuelve
`invalid_input` con línea y columna.

### `Workspace::get_config(id) -> WorkspaceConfig`

Lee `config.toml`. Si está ausente, devuelve el default. Si está
malformado, devuelve `internal` con detalle (ver §Edge case 4).

## Contracts

### Tauri commands

| Command | Notas |
|---|---|
| `workspace_list() -> WorkspaceInfo[]` | |
| `workspace_open(path: string) -> WorkspaceInfo` | |
| `workspace_get(id) -> WorkspaceInfo` | |
| `workspace_delete(id, force: bool) -> ()` | |
| `workspace_detect_venv(id) -> VenvSpec \| null` | |
| `workspace_create_venv(id, backend: "uv" \| "venv") -> VenvSpec` | |
| `workspace_get_config(id) -> WorkspaceConfig` | |
| `workspace_set_config(id, key, value) -> ()` | |

### HTTP endpoints

`GET  /api/v1/workspaces` → `WorkspaceInfo[]`
`POST /api/v1/workspaces` (body: `{ path: string }`) → `WorkspaceInfo`
`GET  /api/v1/workspaces/:id` → `WorkspaceInfo`
`DELETE /api/v1/workspaces/:id?force=<bool>` → `{}`
`GET  /api/v1/workspaces/:id/venv` → `VenvSpec | null`
`POST /api/v1/workspaces/:id/venv` (body: `{ backend }`) → `VenvSpec`
`GET  /api/v1/workspaces/:id/config` → `WorkspaceConfig`
`PATCH /api/v1/workspaces/:id/config` (body: `{ key, value }`) → `{}`

### Eventos

Este dominio **no emite eventos** propios. La creación del `.venv`
queda en el `journal` (consultable, no push).

## Edge cases

1. **`.venv` es un symlink roto**: `detect_venv` retorna `None` y
   `tracing::warn!` con detalle. AC2 + AC3 lo cubren.
2. **`pyproject.toml` con `requires-python = ">=3.12"` pero el
   sistema tiene 3.10**: `detect_venv` retorna `None` (no hay venv
   válido creado aún). Si el usuario intenta `python_run` con
   `--python-version` explícito, el provider devuelve
   `provider_unavailable`.
3. **Workspace en volumen de red (SMB/NFS)**: `python -m venv` puede
   fallar con `OSError`. `create_venv` devuelve `internal` con
   detalle y sugiere `uv` (que maneja mejor las redes). La
   detección funciona pero `create_venv` puede fallar.
4. **`state.json` o `config.toml` corruptos**: la primera lectura
   que falle devuelve `internal` con la línea/columna exacta (vía
   `toml::de::Error` o `serde_json::Error`). El usuario puede
   editar a mano. **Nunca** se sobreescriben automáticamente.
5. **`open` concurrente del mismo path**: serializado por
   `Mutex<()>` en `~/.agentyx/locks/open-<hash>`. El segundo espera
   o falla con `database_busy` si supera 5 s.
6. **Workspace renombrado/movido por el usuario** fuera de Agentyx:
   la próxima `get` o `detect_venv` falla con `not_found`. El
   usuario debe reabrir el workspace en la nueva ubicación.
7. **`.venv` con miles de paquetes**: `version` se obtiene con
   `<python> --version`, no inspeccionando paquetes. Es una llamada
   rápida (< 100 ms en macOS).
8. **Race entre `detect_venv` y `create_venv`**: si dos `create_venv`
   se llaman en paralelo, el segundo ve `.venv` ya creado y devuelve
   `conflict`.
9. **Path con caracteres Unicode o espacios**: soportado. La
   canonicalización usa `OsStr` nativo.
10. **Disco lleno en `create_venv`**: `internal` con `kind: io`.
    `.venv` parcial queda en disco. El siguiente `detect_venv` lo
    detecta como "existente pero inválido" (no arranca) y retorna
    `None` con `tracing::error!`.

## Acceptance criteria

Cada AC → test con nombre derivado `ac<n>_<short>`.

- [ ] AC1: `open` con un path válido nuevo crea el directorio
  `~/.agentyx/workspaces/<id>/` con `config.toml` y `state.db` (con
  migraciones aplicadas). **Test**: `ac1_open_creates_workspace_dir`.
- [ ] AC2: `detect_venv` con un workspace que tiene `.venv/` retorna
  la `VenvSpec` correcta en < 50 ms. **Test**:
  `ac2_detect_venv_with_dotvenv`.
- [ ] AC3: `detect_venv` con un workspace sin venv retorna `None` y
  no escribe nada en disco. **Test**: `ac3_detect_venv_returns_null`.
- [ ] AC4: `detect_venv` con `.venv` roto (symlink inválido) retorna
  `None` y emite `tracing::warn!`. **Test**:
  `ac4_detect_venv_broken_symlink`.
- [ ] AC5: `create_venv` con `uv` instalado y `backend: "uv"` crea
  un venv funcional y retorna `VenvSpec` con `kind: Uv`. **Test**:
  `ac5_create_venv_with_uv`.
- [ ] AC6: `create_venv` con `backend: "venv"` y `python -m venv`
  disponible crea un venv funcional. **Test**:
  `ac6_create_venv_with_python_venv`.
- [ ] AC7: `create_venv` con `.venv` ya existente devuelve
  `conflict`. **Test**: `ac7_create_venv_already_exists`.
- [ ] AC8: `delete` con `force=false` y runs activos devuelve
  `conflict`. **Test**: `ac8_delete_blocks_with_active_runs`.
- [ ] AC9: `delete` con `force=true` aborta runs activos y borra
  el directorio. **Test**: `ac9_delete_force_clears_runs`.
- [ ] AC10: `open` con un path fuera de la whitelist de roots
  permitidos devuelve `path_traversal`. **Test**:
  `ac10_open_path_traversal_rejected`.
- [ ] AC11: `get_config` con `config.toml` ausente devuelve el
  default. **Test**: `ac11_get_config_returns_default`.
- [ ] AC12: `get_config` con `config.toml` malformado devuelve
  `internal` con línea y columna. **Test**:
  `ac12_get_config_malformed_returns_error`.
- [ ] AC13: `set_config` rechaza keys inmutables (`id`,
  `created_at`) con `invalid_input`. **Test**:
  `ac13_set_config_rejects_immutable_keys`.
- [ ] AC14: dos `open` concurrentes del mismo path: el segundo no
  duplica, devuelve el existente. **Test**:
  `ac14_concurrent_open_idempotent`.

## Discovered bugs (post-approval)

| ID | Date | Category | Resolved in | Notes |
|---|---|---|---|---|
| _ninguno aún_ | | | | |

## Open questions

- **Q1**: ¿Cuál es la whitelist de roots permitidos para `open`? →
  **Propuesta v1**: `~`, `/Users`, `/home`, `C:\Users`,
  `C:\Projets`, `C:\Code`, `C:\Source`, `C:\Proyectos`. Lista
  hardcoded; configurable en `~/.agentyx/config.toml` global en v2.
- **Q2**: ¿`config.toml` puede tener secciones custom (e.g. para
  user-defined ignore patterns por lenguaje)? → **Propuesta v1**:
  sí, secciones `[tool.X]` se aceptan y se exponen como
  `WorkspaceConfig::tool: Map<String, Value>`. No las validamos.
- **Q3**: ¿`create_venv` con `--system-site-packages`? → **Propuesta
  v1**: no. Si el user lo quiere, lo hace a mano. v2 quizá.
- **Q4**: ¿Multi-workspace con el mismo `root_path` (vía symlinks
  distintos)? → **Propuesta v1**: rechazado. Canonicalizamos, y
  si dos paths canónicamente son iguales, son el mismo workspace.
- **Q5**: ¿Auto-refresco del cache `last_opened` cuando una sesión
  del workspace arranca? → **Propuesta v1**: sí, lo actualiza
  `AgentLoop::start` como side effect. Documentado en
  `agent-loop.md` Open questions.

## References

- [`../adr/0004-detect-venv-priority.md`](../adr/0004-detect-venv-priority.md) — orden de detección.
- [`../architecture.md`](../architecture.md) — dónde encaja el workspace.
- [`session.md`](./session.md) — sesiones hijas del workspace.
- [`storage.md`](./storage.md) — `state.db` por workspace.
- [`permissions.md`](./permissions.md) — `[permissions]` del config.
- [`tools.md`](./tools.md) — `tool_run` dentro del `root_path`.
