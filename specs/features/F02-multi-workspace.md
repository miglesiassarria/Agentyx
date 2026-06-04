# F02 — Multi-Workspace (open, list, delete, .venv detect/create)

**Status**: draft
**Owner**: @miglesias
**Last update**: 2026-06-04
**Affects**: [`workspace.md`](../domains/workspace.md), [`tools.md`](../domains/tools.md), [`session.md`](../domains/session.md), [`storage.md`](../domains/storage.md), [`permissions.md`](../domains/permissions.md)
**Depends on**: — (es la feature piloto; valida la cadena Rust → IPC → UI end-to-end)

> Primera feature vertical de Agentyx. Permite al usuario **abrir
> una carpeta como workspace**, verla en una sidebar persistente,
> navegar su árbol de archivos, ver el estado del `.venv`, y (si
> quiere) crearlo. **No** incluye chat ni agent loop — eso es F01.

## User story

Como developer, quiero **abrir una carpeta de mi proyecto como
workspace** y ver su estructura + el estado de su `.venv`, para
empezar a trabajar con Agentyx en ese proyecto sin tener que configurar
nada manualmente.

## Scope

**In-scope**:
- Abrir una carpeta del filesystem como workspace nuevo.
- Listar workspaces en una sidebar persistente.
- Seleccionar un workspace y ver su árbol de archivos (lazy).
- Mostrar el estado del `.venv` (detectado o ausente) con un badge.
- Crear un `.venv` explícitamente desde la UI (con `uv` o `python -m venv`).
- Borrar un workspace (con confirmación y manejo de runs activos).
- Persistencia de la lista de workspaces entre arranques.

**Out-of-scope**:
- Chat con LLM (F01).
- Diff visual (F04).
- Múltiples sesiones en el sidebar (F13).
- Mover/renombrar el `root_path` desde la UI.
- Watch de cambios en archivos (F18, v0.3).
- Permisos personalizados por workspace (F05 cubre defaults; custom
  via Settings → otra feature).
- Búsqueda en el workspace (F10, v0.2).

## UX / UI

### Pantalla principal (default route `/`)

```
┌────────────────────────────────────────────────────────────┐
│  Agentyx                                       ⚙ Settings │
├──────────┬─────────────────────────────────────────────────┤
│  🏠 Home │                                                  │
│          │  + Open workspace                                │
│  Workspaces:                                               │
│  ┌──────┐ │                                                  │
│  │📁 agentyx │  (vacío, sin workspace seleccionado)          │
│  │📁 myproject│                                                │
│  └──────┘ │                                                  │
│          │                                                  │
│  + Add   │                                                  │
└──────────┴─────────────────────────────────────────────────┘
```

### Workspace seleccionado

```
┌──────────┬─────────────────────────────────────────────────┐
│  🏠 Home │  📁 myproject                  🐍 .venv · 3.12  │
│          │  /Users/.../myproject                            │
│  Workspaces:           ┌──────────────────────────────┐    │
│  ┌──────┐ │           │ ▼ myproject/                  │    │
│  │📁 agentyx │           │   ▼ src/                    │    │
│  │📁 myproject* │           │     • main.py             │    │
│  └──────┘ │           │     • utils.py              │    │
│          │           │   ▶ tests/                   │    │
│          │           │   • pyproject.toml           │    │
│          │           │   • README.md                │    │
│          │           └──────────────────────────────┘    │
│          │                                                  │
│  + Add   │  [Open chat] (deshabilitado, F01)               │
└──────────┴─────────────────────────────────────────────────┘
```

### Workspace sin venv

```
│  📁 myproject          🐍 No venv                            │
│  /Users/.../myproject     [ + Crear venv aquí ]              │
```

### Diálogo "Crear venv"

```
┌─────────────────────────────────────┐
│  Crear .venv en myproject           │
│                                     │
│  Backend:                           │
│    ◉ uv  (rápido, recomendado)      │
│    ○ python -m venv                 │
│                                     │
│  Esto creará un .venv/ con          │
│  pip y el Python del sistema.       │
│                                     │
│            [ Cancel ]  [ Create ]   │
└─────────────────────────────────────┘
```

### Confirmación de borrado

```
┌─────────────────────────────────────┐
│  ¿Borrar workspace "myproject"?     │
│                                     │
│  Esto eliminará:                    │
│   • ~/.agentyx/workspaces/<id>/     │
│   • Su historial de chat            │
│   • Su journal                      │
│                                     │
│  Los archivos del proyecto NO se    │
│  tocan.                             │
│                                     │
│   [ Cancel ]  [ Delete ]            │
└─────────────────────────────────────┘
```

## Flow

### Open workspace

```
1. UI: user clicks "+ Open workspace" en sidebar
   → Tauri command workspace_open_dialog()
   → abre file dialog del SO (tauri-plugin-dialog)
   → user selecciona carpeta
   → UI: workspace_open(path)
     → core: Workspace::open(path)
       → canonicalize
       → check whitelist
       → mkdir ~/.agentyx/workspaces/<id>/
       → write config.toml
       → Db::open(state.db)
       → registry en state.json
     → return WorkspaceInfo
   → UI: refresh sidebar, select new workspace
   → UI: load file tree (tool list_dir)
   → UI: detect venv (Workspace::detect_venv)
   → UI: render badge
```

### Crear venv

```
1. UI: user clicks "+ Crear venv aquí"
   → dialog con choice uv/venv
   → UI: workspace_create_venv(id, backend)
     → core: Workspace::create_venv
       → uv venv .venv (o python -m venv .venv)
       → verify python arranca
       → journal.append
     → return VenvSpec
   → UI: badge updates, dialog closes
```

### Borrar workspace

```
1. UI: user clicks "..." en sidebar item → "Delete"
   → confirmation dialog
   → UI: workspace_delete(id, force=false)
     → core: Workspace::delete(id, force)
       → check active runs → conflict if any
       → rm -rf ~/.agentyx/workspaces/<id>/
       → remove from state.json
     → return {}
   → UI: refresh sidebar, deselect
```

## Affected Tauri commands / endpoints / events

(Ver [`../ipc.md`](../ipc.md) para convenciones.)

**Commands**:
- `workspace_list() -> WorkspaceInfo[]`
- `workspace_open(path: string) -> WorkspaceInfo`
- `workspace_get(id) -> WorkspaceInfo`
- `workspace_delete(id, force: bool) -> ()`
- `workspace_detect_venv(id) -> VenvSpec | null`
- `workspace_create_venv(id, backend: "uv" | "venv") -> VenvSpec`
- `workspace_get_config(id) -> WorkspaceConfig`

**HTTP endpoints** (cuando el server esté activo, F06):
- `GET  /api/v1/workspaces`
- `POST /api/v1/workspaces`
- `DELETE /api/v1/workspaces/:id?force=...`
- `GET  /api/v1/workspaces/:id/venv`
- `POST /api/v1/workspaces/:id/venv`

**Eventos**: este dominio **no emite** eventos propios (F01 trae
los `chat.*.v1`).

## Acceptance criteria

Cada AC → test con nombre derivado `f02_ac<n>_<short>`.

- [ ] **F02.AC1**: el usuario puede abrir una carpeta como workspace
  desde un file dialog. La carpeta aparece en la sidebar.
  **Test** (e2e Playwright): `f02_ac1_open_workspace_from_dialog`.
- [ ] **F02.AC2**: la lista de workspaces persiste tras cerrar y
  reabrir la app. **Test** (Rust integration):
  `f02_ac2_workspaces_persist_across_restart`.
- [ ] **F02.AC3**: al seleccionar un workspace, se carga su file
  tree (vía `list_dir`). **Test** (e2e):
  `f02_ac3_selecting_workspace_loads_tree`.
- [ ] **F02.AC4**: el badge "🐍 .venv" muestra correctamente el
  venv detectado (vía `Workspace::detect_venv`). **Test** (Rust
  unit): `f02_ac4_venv_badge_reflects_detect`.
- [ ] **F02.AC5**: si el workspace no tiene venv, aparece el CTA
  "Crear venv aquí" y el badge "🐍 No venv". **Test** (e2e):
  `f02_ac5_no_venv_shows_cta`.
- [ ] **F02.AC6**: al pulsar "Crear venv aquí" y elegir `uv`, se
  crea el venv y el badge se actualiza. **Test** (Rust integration):
  `f02_ac6_create_venv_with_uv_updates_badge`.
- [ ] **F02.AC7**: borrar un workspace pide confirmación, luego lo
  elimina del sidebar y del filesystem. **Test** (e2e):
  `f02_ac7_delete_with_confirmation`.
- [ ] **F02.AC8**: borrar un workspace con sesiones en estado
  `running` se rechaza con `conflict` y un mensaje claro. **Test**
  (Rust integration): `f02_ac8_delete_with_active_runs_rejected`.
- [ ] **F02.AC9**: el file tree es **lazy**: las subcarpetas
  cerradas no se listan hasta que el usuario las expande. **Test**
  (e2e con workspace de 1000 archivos):
  `f02_ac9_file_tree_is_lazy`.
- [ ] **F02.AC10**: workspace con cero archivos se abre sin
  error y muestra el tree vacío con CTA "Add files". **Test** (e2e):
  `f02_ac10_empty_workspace_handled`.
- [ ] **F02.AC11**: workspace con `.venv` roto (symlink inválido)
  muestra "No venv" + warning en la consola del dev, no crash.
  **Test** (Rust integration):
  `f02_ac11_broken_venv_handled_gracefully`.
- [ ] **F02.AC12**: abrir una carpeta fuera de la whitelist de
  roots permitidos (ver `workspace.md#open`) muestra un error
  claro y no crea el workspace. **Test** (Rust integration):
  `f02_ac12_path_outside_whitelist_rejected`.
- [ ] **F02.AC13**: el sidebar muestra los workspaces en orden
  `last_opened_at DESC` (más reciente arriba). **Test** (e2e):
  `f02_ac13_sidebar_orders_by_recent`.
- [ ] **F02.AC14**: el file tree respeta los `ignore` patterns del
  `config.toml` (no muestra `node_modules`, `.git`, etc.). **Test**
  (e2e): `f02_ac14_file_tree_respects_ignore`.

## Tests

- **Unit (Rust)**: en
  `crates/agentyx-core/src/workspace/detect.rs::tests` y
  `crates/agentyx-core/src/workspace/mod.rs::tests` para los
  algorithms de detection, open, delete.
- **Integration (Rust)**: `crates/agentyx-core/tests/workspace.rs`
  con DB temporal.
- **Unit (TS)**: `ui/src/lib/components/Sidebar.svelte.test.ts` y
  `ui/src/lib/components/FileTree.svelte.test.ts` con `@testing-library/svelte`
  + stores mockeados.
- **E2E (Playwright)**: `ui/e2e/F02-multi-workspace.spec.ts` con
  el server HTTP embebido (no requiere Tauri para los tests, lo
  cual es la razón de ser de F06 server-first).

## Telemetry / logs

```rust
tracing::info!(
    workspace_id = %id,
    root = %root.display(),
    "workspace opened"
);

tracing::info!(
    workspace_id = %id,
    venv_kind = ?spec.kind,
    "venv detected"
);

tracing::info!(
    workspace_id = %id,
    backend = %backend,
    duration_ms = ms,
    "venv created"
);

tracing::info!(
    workspace_id = %id,
    deleted_files = n,
    "workspace deleted"
);
```

**No** se loguea el árbol de archivos del workspace, ni el path
completo cuando es del usuario (solo el `workspace_id` y el basename).

## Security notes

- **Path traversal** bloqueado en `Workspace::open` vía whitelist
  + canonicalización (ver `workspace.md#open`).
- **`.venv` activation** se hace ejecutando el binario directamente,
  **nunca** `source activate` (ver `workspace.md` y `tools.md#python_run`).
- **Delete** borra solo `~/.agentyx/workspaces/<id>/`, **nunca**
  toca el `root_path` del proyecto del usuario.
- **Logs sin secretos**: redactar paths con tokens si los hay (no
  aplica en F02 porque solo se loguean IDs y basenames, pero el
  helper `tracing` debe estar preparado).

## Rollout

- **Detrás de feature flag**: no en v1; el MVP entra con esta feature
  on por defecto.
- **Settings del workspace**: no requiere settings globales.
- **Migración de datos**: no. Es la primera feature; no hay datos
  previos que migrar.

## Open questions

- **Q1**: ¿El sidebar debe permitir drag-and-drop para reordenar
  workspaces? → **Propuesta v1**: no. El orden es por
  `last_opened_at`, fijo.
- **Q2**: ¿Soporte de "recent workspaces" con un menú
  `File → Open Recent`? → **Propuesta v1**: no. El sidebar ya
  cumple esa función.
- **Q3**: ¿El file tree debe permitir acciones rápidas (right-click:
  copy path, open in Finder, etc.)? → **Propuesta v1**: no. Queda
  para F10 con la búsqueda.
- **Q4**: ¿El `Open workspace` dialog debe recordar el último
  directorio? → **Propuesta v1**: sí, vía `tauri-plugin-dialog`.
  Q1 del `workspace.md` ya cubre parte.

## References

- [`../ipc.md`](../ipc.md) — Tauri commands, HTTP, eventos.
- [`../domains/workspace.md`](../domains/workspace.md) — dominio principal.
- [`../domains/tools.md`](../domains/tools.md) — `list_dir`, `read_file`.
- [`../domains/storage.md`](../domains/storage.md) — `state.db` por workspace.
- [`../domains/session.md`](../domains/session.md) — sesiones por workspace.
- [`../domains/permissions.md`](../domains/permissions.md) — defaults para open.
- [`../adr/0004-detect-venv-priority.md`](../adr/0004-detect-venv-priority.md) —
  orden de detección del venv.
- [`ROADMAP.md`](ROADMAP.md) — vista general y dependencias.
