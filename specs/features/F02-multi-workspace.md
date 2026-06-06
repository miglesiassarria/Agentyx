# F02 — Multi-Workspace (open, list, delete, extra paths, .venv pasivo)

**Status**: approved *(backend 8/18 ACs implementado; UI 0/18 ACs pendiente — ver [§ Implementation status](#implementation-status))*
**Owner**: @miglesias
**Last update**: 2026-06-06
**Affects**: [`workspace.md`](../domains/workspace.md), [`tools.md`](../domains/tools.md), [`session.md`](../domains/session.md), [`storage.md`](../domains/storage.md), [`permissions.md`](../domains/permissions.md)
**Depends on**: — (es la feature piloto; valida la cadena Rust → IPC → UI end-to-end)

> Primera feature vertical de Agentyx. Permite al usuario **abrir
> una carpeta como workspace**, verla en una sidebar persistente,
> navegar su árbol de archivos, **añadir 0..N directorios extra**
> sobre los que el agente también tiene R/W, y (si el workspace ya
> tiene un venv) ver su estado. **No** incluye chat ni agent loop —
> eso es F01. La creación de venv se mueve a F03 (opt-in en v0.1.x).

## User story

Como usuario, quiero **abrir una carpeta de mi proyecto como
workspace** y, opcionalmente, **darle al agente acceso a directorios
adicionales** sobre los que trabajar, para empezar a delegar tareas
sobre mis proyectos sin tener que configurar nada manualmente.

## Scope

**In-scope**:
- Abrir una carpeta del filesystem como workspace nuevo.
- Listar workspaces en una sidebar persistente.
- Seleccionar un workspace y ver su árbol de archivos (lazy).
- **Añadir 0..N "extra paths"** al workspace: directorios fuera del
  root sobre los que el agente tiene R/W (ver
  [ADR-0007](../adr/0007-extra-paths-per-workspace.md)).
- **Quitar extra paths** del workspace.
- Mostrar el badge "🐍 .venv X.Y" **si el workspace ya tiene un
  venv detectado** (modo pasivo; no se ofrece crearlo aquí).
- Borrar un workspace (con confirmación y manejo de runs activos).
- Persistencia de la lista de workspaces, sus configs y sus extra
  paths entre arranques.

**Out-of-scope**:
- Chat con LLM (F01).
- Diff visual (F04).
- Múltiples sesiones en el sidebar (F13).
- Mover/renombrar el `root_path` desde la UI.
- Watch de cambios en archivos (F18, v0.3).
- Permisos personalizados por workspace (F05 cubre defaults; custom
  via Settings → otra feature).
- Búsqueda en el workspace (F10, v0.2).
- **Creación de `.venv` desde la UI** (movido a F03, v0.1.x; un
  workspace sin venv es perfectamente válido en v1).
- Cycle con Tab entre primary agents (v0.2, depende de F01).

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

### Workspace seleccionado (con venv detectado)

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

### Workspace seleccionado (con extra paths)

```
┌──────────┬─────────────────────────────────────────────────┐
│  🏠 Home │  📁 myproject                                   │
│          │  /Users/.../myproject                            │
│  Workspaces:           ┌──────────────────────────────┐    │
│  ┌──────┐ │           │ ▼ myproject/                  │    │
│  │📁 agentyx │           │   ▶ src/                     │    │
│  │📁 myproject* │           │   • README.md              │    │
│  └──────┘ │           │                              │    │
│          │           └──────────────────────────────┘    │
│          │                                                  │
│          │  📂 Extras (2):                                 │
│          │     • /Users/pepe/assets          ✕             │
│          │     • /tmp/agentyx-exports        ✕             │
│          │     [ + Add directory ]                          │
│          │                                                  │
│  + Add   │  [Open chat] (deshabilitado, F01)               │
└──────────┴─────────────────────────────────────────────────┘
```

### Diálogo "Add directory" (file dialog del SO)

```
┌─────────────────────────────────────┐
│  Add directory to "myproject"       │
│                                     │
│  Pick a folder that the agent       │
│  will be able to read and write.    │
│  You'll see it in the sidebar as    │
│  an "Extra" path.                   │
│                                     │
│  Label (optional):                  │
│  [ Assets compartidos         ]    │
│                                     │
│            [ Cancel ]  [ Add ]      │
└─────────────────────────────────────┘
```

### Confirmación de borrado de extra path

```
┌─────────────────────────────────────┐
│  Quitar "/Users/pepe/assets" de     │
│  "myproject"?                       │
│                                     │
│  El agente ya no podrá acceder a    │
│  este directorio. Los archivos      │
│  no se tocan.                       │
│                                     │
│   [ Cancel ]  [ Remove ]            │
└─────────────────────────────────────┘
```

### Confirmación de borrado de workspace

```
┌─────────────────────────────────────┐
│  ¿Borrar workspace "myproject"?     │
│                                     │
│  Esto eliminará:                    │
│   • ~/.agentyx/workspaces/<id>/     │
│   • Su historial de chat            │
│   • Su journal                      │
│   • Sus extra paths (de la config)  │
│                                     │
│  Los archivos del proyecto NO se    │
│  tocan (ni el root ni los extras).  │
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
       → write config.toml (con extra_paths: [] por defecto)
       → Db::open(state.db)
       → registry en state.json
     → return WorkspaceInfo (con extra_paths: [])
   → UI: refresh sidebar, select new workspace
   → UI: load file tree (tool list_dir)
   → UI: load extra paths list (workspace_list_extra_paths)
   → UI: detect venv (Workspace::detect_venv)
     → si Some: badge "🐍 .venv X.Y"
     → si None: no se muestra badge (workspace sin venv es OK)
```

### Add extra path

```
1. UI: user clicks "+ Add directory" en sección Extras
   → dialog modal con file dialog del SO
   → user selecciona carpeta y opcionalmente un label
   → UI: workspace_add_extra_path(id, path, label?)
     → core: Workspace::add_extra_path
       → canonicalize
       → check whitelist (mismo que open)
       → check not equal to root
       → check not duplicate
       → persist en state.json + config.toml + state.db
       → emit workspace.extra_path_added.v1
     → return ExtraPathSpec
   → UI: refresh extras list
```

### Remove extra path

```
1. UI: user clicks ✕ en un extra path
   → confirmation dialog
   → UI: workspace_remove_extra_path(id, path)
     → core: Workspace::remove_extra_path
       → persist en los 3 sitios
       → emit workspace.extra_path_removed.v1
     → return {}
   → UI: refresh extras list
```

### Borrar workspace

```
1. UI: user clicks "..." en sidebar item → "Delete"
   → confirmation dialog
   → UI: workspace_delete(id, force=false)
     → core: Workspace::delete(id, force)
       → check active runs → conflict if any
       → rm -rf ~/.agentyx/workspaces/<id>/
       → remove from state.json (incluyendo extra_paths)
     → return {}
   → UI: refresh sidebar, deselect
```

## Affected Tauri commands / endpoints / events

(Ver [`../ipc.md`](../ipc.md) para convenciones.)

**Commands**:
- `workspace_list() -> WorkspaceInfo[]` (incluye `extra_paths`)
- `workspace_open(path: string) -> WorkspaceInfo` (con
  `extra_paths: []` por defecto)
- `workspace_get(id) -> WorkspaceInfo`
- `workspace_delete(id, force: bool) -> ()`
- `workspace_detect_venv(id) -> VenvSpec | null`
- `workspace_create_venv(id, backend: "uv" | "venv") -> VenvSpec` —
  **fuera del MVP**; documentado para F03.
- `workspace_get_config(id) -> WorkspaceConfig`
- `workspace_add_extra_path(id, path, label?) -> ExtraPathSpec` —
  **nuevo en v1**.
- `workspace_remove_extra_path(id, path) -> ()` — **nuevo en v1**.
- `workspace_list_extra_paths(id) -> ExtraPathSpec[]` —
  **nuevo en v1**.

**HTTP endpoints** (cuando el server esté activo, F06):
- `GET  /api/v1/workspaces`
- `POST /api/v1/workspaces`
- `DELETE /api/v1/workspaces/:id?force=...`
- `GET  /api/v1/workspaces/:id/venv`
- `GET  /api/v1/workspaces/:id/extra-paths` — **nuevo**
- `POST /api/v1/workspaces/:id/extra-paths` — **nuevo**
- `DELETE /api/v1/workspaces/:id/extra-paths` — **nuevo**

**Eventos**:
- `workspace.extra_path_added.v1` — emitido por `add_extra_path`.
- `workspace.extra_path_removed.v1` — emitido por `remove_extra_path`.
- (F01 trae los `chat.*.v1`; este dominio no emite eventos de chat.)

## Acceptance criteria

Cada AC → test con nombre derivado `f02_ac<n>_<short>`.

- [ ] **F02.AC1**: el usuario puede abrir una carpeta como workspace
  desde un file dialog. La carpeta aparece en la sidebar.
  **Test** (e2e Playwright): `f02_ac1_open_workspace_from_dialog`.
- [x] **F02.AC2**: la lista de workspaces persiste tras cerrar y
  reabrir la app. **Test** (Rust integration):
  `f02_ac2_workspaces_persist_across_restart`.
- [ ] **F02.AC3**: al seleccionar un workspace, se carga su file
  tree (vía `list_dir`). **Test** (e2e):
  `f02_ac3_selecting_workspace_loads_tree`.
- [x] **F02.AC4**: el badge "🐍 .venv X.Y" aparece si y solo si
  el workspace tiene un venv detectado por `Workspace::detect_venv`.
  Un workspace sin venv **no** muestra badge y **no** muestra CTA
  "Crear venv aquí" (esa acción se difiere a F03). **Test** (Rust
  unit + e2e): `f02_ac4_venv_badge_pasive_only`.
- [x] **F02.AC5**: workspace con `.venv` roto (symlink inválido)
  no muestra badge de venv y no crashea. El usuario ve el workspace
  sin badge. **Test** (Rust integration):
  `f02_ac5_broken_venv_handled_gracefully`.
- [ ] **F02.AC6**: borrar un workspace pide confirmación, luego lo
  elimina del sidebar y del filesystem. **Test** (e2e):
  `f02_ac6_delete_with_confirmation`.
- [ ] **F02.AC7**: borrar un workspace con sesiones en estado
  `running` se rechaza con `conflict` y un mensaje claro. **Test**
  (Rust integration): `f02_ac7_delete_with_active_runs_rejected`.
  **Nota (v0.1)**: el backend `WorkspaceService::delete` es un
  placeholder que siempre permite el borrado; el check de runs
  activos se cableará cuando aterrice el PR de `agent-loop`.
- [ ] **F02.AC8**: el file tree es **lazy**: las subcarpetas
  cerradas no se listan hasta que el usuario las expande. **Test**
  (e2e con workspace de 1000 archivos):
  `f02_ac8_file_tree_is_lazy`.
- [ ] **F02.AC9**: workspace con cero archivos se abre sin
  error y muestra el tree vacío con CTA "Add files". **Test** (e2e):
  `f02_ac9_empty_workspace_handled`.
- [x] **F02.AC10**: abrir una carpeta fuera de la whitelist de
  roots permitidos (ver `workspace.md#open`) muestra un error
  claro y no crea el workspace. **Test** (Rust integration):
  `f02_ac10_path_outside_whitelist_rejected`.
- [x] **F02.AC11**: el sidebar muestra los workspaces en orden
  `last_opened_at DESC` (más reciente arriba). **Test** (e2e):
  `f02_ac11_sidebar_orders_by_recent`.
- [ ] **F02.AC12**: el file tree respeta los `ignore` patterns del
  `config.toml` (no muestra `node_modules`, `.git`, etc.). **Test**
  (e2e): `f02_ac12_file_tree_respects_ignore`.
- [ ] **F02.AC13**: en la sección "Extras" del sidebar se listan
  los extra paths del workspace con su label (si lo tienen) y un
  botón ✕ para borrarlos. **Test** (e2e):
  `f02_ac13_extras_listed_in_sidebar`.
- [ ] **F02.AC14**: el usuario puede pulsar "+ Add directory" en
  la sección Extras, seleccionar una carpeta del file dialog, y
  verla añadida a la lista (y persistida tras restart). **Test**
  (e2e + Rust integration):
  `f02_ac14_add_extra_path_from_dialog`.
- [x] **F02.AC15**: intentar añadir como extra path un directorio
  fuera de la whitelist de roots permitidos retorna `path_traversal`
  y muestra un error en la UI. El workspace queda sin cambios.
  **Test** (Rust integration + e2e):
  `f02_ac15_add_extra_path_outside_whitelist_rejected`.
- [x] **F02.AC16**: intentar añadir como extra path el propio
  `root_path` del workspace retorna `conflict { reason:
  "path_is_root" }`. **Test** (Rust integration):
  `f02_ac16_add_extra_path_equal_to_root_rejected`.
- [ ] **F02.AC17**: el botón ✕ de un extra path abre un
  confirmation dialog; al confirmar, el path desaparece de la
  lista y se persiste la baja. **Test** (e2e + Rust integration):
  `f02_ac17_remove_extra_path_with_confirmation`.
- [x] **F02.AC18**: tras `add_extra_path` o `remove_extra_path`,
  un reload desde la sidebar refleja el cambio sin necesidad de
  cerrar y reabrir la app. **Test** (e2e):
  `f02_ac18_extras_update_in_place`.

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
- **Q5**: ¿Se muestra en la UI el árbol de archivos de un extra
  path? → **Propuesta v1**: **no**. El extra path es accesible
  para tools; la UI lo lista en la sección "Extras" con su path
  absoluto y label, pero no expande su árbol. v1.x quizá.
- **Q6**: ¿Cap de N extra paths por workspace? → **Propuesta v1**:
  no hay cap. v1.x quizá cap a 20 con "more…" en la UI si
  afecta el system prompt.

## References

- [`../ipc.md`](../ipc.md) — Tauri commands, HTTP, eventos.
- [`../domains/workspace.md`](../domains/workspace.md) — dominio principal.
- [`../domains/tools.md`](../domains/tools.md) — `list_dir`, `read_file`.
- [`../domains/storage.md`](../domains/storage.md) — `state.db` por workspace.
- [`../domains/session.md`](../domains/session.md) — sesiones por workspace.
- [`../domains/permissions.md`](../domains/permissions.md) — defaults y
  paso 2bis (path_outside_workspace).
- [`../adr/0004-detect-venv-priority.md`](../adr/0004-detect-venv-priority.md) —
  orden de detección del venv.
- [`../adr/0007-extra-paths-per-workspace.md`](../adr/0007-extra-paths-per-workspace.md) —
  modelo `root + extra_paths`.
- [`ROADMAP.md`](ROADMAP.md) — vista general y dependencias.

## Implementation status

> Snapshot del estado real de implementación. Se actualiza en el
> mismo PR que cambia el código (ver `AGENTS.md` §17 Spec-Driven
> Development). La fecha indica el último sync.

**Última sync**: 2026-06-06
**Backend (Rust)**: **8 / 18 ACs cubiertos** ✅
**IPC (Tauri commands)**: **9 / 9 commands cableados** ✅
**UI (Svelte)**: **0 / 18 ACs cubiertos** ❌

### ACs cubiertos (backend)

| AC | Cobertura | Tests |
|---|---|---|
| F02.AC2 | `WorkspaceRegistry` persiste `state.json` v2; carga/guarda atómico | `registry::tests::empty_registry_round_trip`, `load_wrong_version_errors` |
| F02.AC4 | `detect_venv` con prioridad completa (uv → venv → pyenv → pyproject → lock → conda) | `venv::tests::*`, `commands::workspace::tests::workspace_to_dto_runs_detect_venv` |
| F02.AC5 | `inspect_venv_dir` retorna `None` en symlink roto + `tracing::warn!` | `venv::tests::detect_venv_with_broken_dotvenv_returns_none` |
| F02.AC10 | `WorkspaceService::open` rechaza con `InvalidInput` si el path canónico no está en la whitelist | `service::tests::open_path_outside_whitelist_rejected` |
| F02.AC11 | `WorkspaceService::list` ordena por `last_opened_at DESC` | `service::tests::list_returns_all_workspaces_ordered_by_last_opened` |
| F02.AC15 | `add_extra_path` retorna `AppError::PathOutsideWorkspace` y **no** persiste | `service::tests::add_extra_path_outside_whitelist_rejected` |
| F02.AC16 | `add_extra_path` retorna `AppError::Conflict` si el path == `root_path` | `service::tests::add_extra_path_equal_to_root_rejected` |
| F02.AC18 | Tauri commands emiten `workspace.extra_path_added.v1` / `extra_path_removed.v1` | `commands::workspace` (eventos en `add_extra_path` / `remove_extra_path`) |

### ACs pendientes (UI)

| AC | Pendiente |
|---|---|
| F02.AC1 | File dialog del SO (`tauri-plugin-dialog`) + sidebar |
| F02.AC3 | Componente `FileTree.svelte` que consume `list_dir` |
| F02.AC6 | Confirmation dialog de borrado de workspace |
| F02.AC8 | `FileTree` lazy (subcarpetas no se listan hasta expandir) |
| F02.AC9 | Empty state del file tree |
| F02.AC12 | File tree filtra por `config.ignore` |
| F02.AC13 | Sección "Extras" en sidebar |
| F02.AC14 | "+ Add directory" + dialog del SO + refresh |
| F02.AC17 | Confirmation dialog de borrado de extra path |

### ACs parcialmente cubiertos

| AC | Estado |
|---|---|
| F02.AC7 | **Backend parcial**: `WorkspaceService::delete` es un placeholder que siempre permite borrado. El check de runs activos se cablea con el PR de `agent-loop`. **UI**: confirmation dialog pendiente. |

### PRs de referencia

- `feat(core): workspace model + extra_paths (ADR-0007)` (PR #5) — 34 tests en `agentyx-core`.
- `feat(app): F02 Tauri commands wired to WorkspaceService` (PR #6) — 18 tests en `agentyx-app`.
- `fix(tests): Windows venv layout + canonical path comparison in effective_paths` (PR #7) — Windows parity.

## Discovered bugs (post-approval)

| ID | Date | Category | Resolved in | Notes |
|---|---|---|---|---|
| BUG-001 | 2026-06-06 | A. Spec gap (proceso) | este PR | F02 fue mergeado en PRs #5 y #6 cuando aún estaba en status `review`, no `approved`. Se sube retroactivamente a `approved` aquí y se refuerza la regla §17 de `AGENTS.md` (STATUS.md debe actualizarse en el mismo PR). El spec en sí no cambió; la cobertura AC es la documentada arriba. |

## Próximos pasos

1. **PR de `agent-loop`**: cablea F02.AC7 (rechazo de delete con runs activos).
2. **PR de UI F02 (Fase D)**: cubre F02.AC1, AC3, AC6, AC8, AC9, AC12, AC13, AC14, AC17.
3. **Migración final a `implemented`**: cuando los 18 ACs estén ✅, mover F02 a la sección ✅ en `STATUS.md` y al estado `implemented` en este spec.
