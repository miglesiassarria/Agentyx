# F04 — File diffs en UI

**Status**: ready
**Owner**: @miglesias
**Last update**: 2026-06-07
**Affects**: [`tools`](../domains/tools.md) (`edit_file`, `apply_patch`,
`write_file`), [`F01`](./F01-chat-streaming.md) (consume los tool
calls que producen cambios), [`journal`](../domains/journal.md)
(`DiffProposal`, `DiffApplied`, `DiffRejected`).
**Depends on**: [`F01`](./F01-chat-streaming.md) (chat emite los
tool calls), [`F02`](./F02-multi-workspace.md) (workspaces tienen
archivos), [`tools.md`](../domains/tools.md) (la forma de los args
de `edit_file` / `apply_patch` / `write_file`).

## User story

Como **usuario**, quiero **ver un diff visual antes/después cuando
el agente propone editar, parchear o crear un archivo**, para
entender exactamente qué va a cambiar sin tener que abrir el
archivo en otro editor ni esperar a que el cambio se aplique.

## Scope

### In-scope (v0.1)

- Render de un diff inline en el `MessageList` del chat, justo
  después del `tool_call` que lo produjo, usando **CodeMirror 6 +
  `@codemirror/merge`**.
- Diff aplicable a:
  - `edit_file(path, old_text, new_text)` — diff de hunks.
  - `apply_patch(diff_unified)` — diff de patch unificado.
  - `write_file(path, content)` — diff "todo el archivo es nuevo"
    (inserciones masivas o archivo nuevo).
- Side panel opcional (toggle) con la lista de **diffs pendientes
  en esta sesión**, agrupados por path. Click en uno hace scroll
  al `tool_call` correspondiente en el `MessageList`.
- **Read-only en v0.1**: el diff se muestra, pero el usuario no
  lo "aprueba" ni lo "rechaza" desde aquí. La aprobación ya
  ocurrió (vía el permission prompt de F01 antes de ejecutar la
  tool). En v0.1 el diff es **post-mortem visual** del cambio
  que el agente acaba de hacer.
- **Truncation indicator**: si el archivo es muy grande (>256
  KB) o el diff excede 8 KiB, se renderiza la primera porción +
  botón "View full" (carga el contenido on-demand, no inline).
- **Archivos binarios**: si el tool toca un archivo binario
  (detección por nul bytes en los primeros 8 KiB), el diff
  muestra "Binary file changed · <size> · <mime>"; no se
  renderiza diff textual.
- **Búsqueda de archivos modificados en la sesión**: filtro en el
  side panel (input pequeño) sobre la lista de paths.
- **Soporte de imagen como `write_file`**: si el path termina en
  `.png`/`.jpg`/`.webp`/`.gif`, se muestra thumbnail
  (data URL del nuevo) en lugar de diff textual.

### Out-of-scope (v0.1)

- ❌ Approve / Reject desde el diff (la decisión de aplicar
  el cambio ya la tomó F01 vía permission gate). Diferido a
  v0.2 con F12 (permisos en UI).
- ❌ Dry-run explícito antes de aplicar. En v0.1 el diff
  aparece **después** de que la tool ya se ejecutó.
- ❌ Edit inline desde el diff (el usuario no puede tipear
  cambios en el diff y mandarlos de vuelta al agente).
- ❌ Diff entre dos versiones históricas del archivo (no hay
  versiones; el journal no guarda blobs de archivos, solo
  paths + hash). En v1.x con `git integration` (F40) esto
  es natural.
- ❌ Diff entre sesiones distintas o entre workspaces.
- ❌ Render de PDF, DOCX (F07, F08, v0.2).

## UX / UI

### Rutas y componentes

```
ui/src/
├── lib/
│   ├── components/
│   │   ├── diff/
│   │   │   ├── DiffView.svelte         # contenedor (header + body)
│   │   │   ├── DiffHeader.svelte       # path, +/- counts, expand/collapse
│   │   │   ├── DiffBody.svelte         # CodeMirror Merge
│   │   │   ├── DiffTruncatedNotice.svelte
│   │   │   ├── BinaryDiffNotice.svelte
│   │   │   ├── ImageDiffNotice.svelte
│   │   │   └── DiffsSidePanel.svelte   # lista de pendientes
```

### Render en `MessageList`

Cuando llega un `chat.tool_call.v1` con `name ∈ {edit_file,
apply_patch, write_file}`, el `ToolCallBlock` (de F01) detecta
que es "diffable" y, además de mostrar los args resumidos,
incrusta un `DiffView` debajo:

```
+--------------------------------------------------+
|  🔧 edit_file                                     |
|  path: src/lib.rs                                 |
|  args_summary: "edit_file src/lib.rs"            |
+--------------------------------------------------+
|  📄 src/lib.rs                          [−12 +45] |
|  ┌─ CodeMirror Merge ─────────────────────────┐  |
|  │  10  │  fn old() {                          │  |
|  │  11  │-     println!("hello");              │  |
|  │  12  │+     println!("hello, world!");      │  |
|  │  13  │  }                                   │  |
|  └─────────────────────────────────────────────┘  |
+--------------------------------------------------+
|  ✓ tool_result (12ms)                             |
|  output_summary: "edit applied"                  |
+--------------------------------------------------+
```

- `−12 +45` es el header del diff: número de líneas borradas /
  añadidas (calculado en backend al construir el evento).
- CodeMirror Merge con theme que matchea el resto de la app
  (oscuro/claro, fonts de `app.css`).
- Si el usuario colapsa el bloque (click en header), solo
  queda `📄 src/lib.rs · [−12 +45]`. Se persiste el estado
  collapse en `localStorage` por path para no expandir
  siempre los mismos diffs largos.

### `DiffsSidePanel`

```
+-------------------------------------+
|  Pending diffs in this session      |
|  [search...]                        |
+-------------------------------------+
|  src/lib.rs              [−12 +45]  |
|  src/main.rs             [+3 −1]    |
|  README.md               [+10]      |
|  new_module.rs           [+200]     |
+-------------------------------------+
```

- Visible solo si la sesión tiene ≥1 diff.
- Toggle en el header del chat panel (icono `panel-right`).
- Click en un item → scroll al `DiffView` correspondiente en
  `MessageList` y highlight temporal (2s).
- Filtro de búsqueda: substring case-insensitive sobre path.
- Counter en el toggle: "Diffs (3)" si hay >0.

### Estados visuales

| Estado | Indicador |
|---|---|
| `idle` (sin side panel) | Toggle con counter oculto |
| `pending` (diffs disponibles) | Toggle con counter "Diffs (N)" |
| `loading` (cargando diff full) | Spinner inline en `DiffTruncatedNotice` |
| `error` (no se pudo cargar full) | Banner rojo inline con `code` |
| `binary` | `BinaryDiffNotice` (icono + size + mime) |
| `image` | Thumbnail centrado, click abre lightbox |

## Flow

### Render de un diff tras un tool call

```
agent loop: edit_file("src/lib.rs", old, new)
  → PermissionGate::check → allow (o ask + user allow)
  → Tool::run(args) → escribe el archivo, retorna output
  → journal.append(ToolCall { name, args })
  → emit "chat.tool_call.v1" {
       name: "edit_file",
       args,
       argsSummary: "edit_file src/lib.rs",
       toolCallId,
       runId,
       ...
     }
  → tool ejecuta, retorna output
  → journal.append(ToolResult { output, durationMs, isError })
  → emit "chat.tool_result.v1" {
       toolCallId,
       outputSummary: "edit applied",
       durationMs: 12,
       ...
     }
  → si el tool fue edit_file/apply_patch/write_file Y args tienen
     shape diffable:
       compute diff (en el agent loop, antes de emitir el event)
       enrich "chat.tool_call.v1" con campos opcionales:
         diff: {
           kind: "edit_file" | "apply_patch" | "write_file",
           before: String | null,   // null para write_file en archivo nuevo
           after: String,
           beforeTruncated: boolean,
           afterTruncated: boolean,
           isBinary: boolean,
           mime: string | null,
           additions: number,
           deletions: number,
         }
```

> **El diff se calcula en el agent loop** (Rust), no en el
> renderer. Esto es importante porque (a) el renderer es
> Svelte y no tiene lógica de negocio, y (b) queremos poder
> mostrar el diff también en logs estructurados y en el
> journal sin recalcularlo.

> **Truncation**:
> - `before` y `after` se truncan a 8 KiB cada uno (configurable
>   via `journal.max_payload_bytes` override? — no, son
>   independientes). Si exceden, `*Truncated = true` y el
>   renderer muestra `DiffTruncatedNotice`.
> - El payload completo se persiste en `journal` (que ya trunca
>   a 16 KiB por entry, ver `journal.md` §Tamaño).
> - El "View full" carga el contenido desde el backend
>   (`diff_get_full(toolCallId)` — ver §Tauri commands).

### Side panel: lista de pendientes

```
ChatPanel: onMount
  → ipc.invoke("diff_list_pending", { sessionId })
    → query journal por sessionId con kind IN (DiffProposal)
    → para cada DiffProposal, devolver:
       { toolCallId, path, kind, additions, deletions, createdAt }
  → render DiffsSidePanel con la lista
```

> "Pending" significa "propuesto en esta sesión, en journal".
> No hay estado "applied" separado en v0.1: aplicado = el
> archivo en disco está modificado. Para v0.2 con F12
> (approve/reject) se introduce el estado.

## Affected domains

- [`tools.md`](../domains/tools.md) — el shape de args de
  `edit_file`/`apply_patch`/`write_file` ya está definido;
  F04 lo consume. No requiere cambios en tools.md.
- [`journal.md`](../domains/journal.md) — se usan los kinds
  `DiffProposal`, `DiffApplied`, `DiffRejected` (ya
  declarados en journal.md §Kind). `DiffApplied` /
  `DiffRejected` **no se usan en v0.1** (read-only); se
  reservan para v0.2 con F12. `DiffProposal` **sí** se usa
  (vía el `chat.tool_call.v1` enriquecido, que se persiste
  como ToolCall + journal entry).
- [`F01`](./F01-chat-streaming.md) — el `ToolCallBlock` se
  extiende para detectar tools diffable y mostrar el
  `DiffView`. F01 declara el shape del evento enriquecido;
  F04 lo documenta explícitamente aquí.

## Affected Tauri commands / endpoints / events

### Tauri commands (F04)

```rust
#[tauri::command]
pub async fn diff_list_pending(
    session_id: SessionId,
) -> Result<Vec<DiffSummaryDto>, AppError>;

#[tauri::command]
pub async fn diff_get_full(
    tool_call_id: Ulid,
) -> Result<DiffFullDto, AppError>;
```

> **`diff_apply` y `diff_reject` NO se implementan en v0.1**.
> El archivo ya está aplicado (la tool corrió). v0.2 con F12
> introduce el modelo "aplicar diferido" donde se necesitan.

### Endpoints HTTP (v0.2, F06)

```
GET /api/v1/sessions/:id/diffs           → Vec<DiffSummaryDto>
GET /api/v1/diffs/:toolCallId            → DiffFullDto
POST /api/v1/diffs/:toolCallId/apply     → {}    # v0.2 con F12
POST /api/v1/diffs/:toolCallId/reject    → {}    # v0.2 con F12
```

### Eventos (F04)

> F04 **no introduce eventos nuevos** en v0.1. Reutiliza
> `chat.tool_call.v1` (de F01) con un campo opcional
> `diff: DiffPayload | null`. Si `diff != null`, la UI
> renderiza `DiffView` además de los args.

```rust
// En chat.tool_call.v1 (F01)
pub struct ChatToolCallV1 {
    // ...campos existentes de F01
    pub diff: Option<DiffPayload>,
}

pub struct DiffPayload {
    pub kind: DiffKind,                  // "edit_file" | "apply_patch" | "write_file"
    pub before: Option<String>,          // None para write_file en archivo nuevo
    pub after: String,
    pub before_truncated: bool,
    pub after_truncated: bool,
    pub is_binary: bool,
    pub mime: Option<String>,
    pub additions: u32,
    pub deletions: u32,
}
```

> **Shape en TS**:
> ```ts
> type DiffPayload = {
>   kind: 'edit_file' | 'apply_patch' | 'write_file';
>   before: string | null;
>   after: string;
>   beforeTruncated: boolean;
>   afterTruncated: boolean;
>   isBinary: boolean;
>   mime: string | null;
>   additions: number;
>   deletions: number;
> };
> ```

### Tablas

F04 **no crea tablas nuevas** en `state.db`. Usa:

- `journal` (kind `DiffProposal` y el tool_call/tool_result
  asociados).
- `messages` (la fila del `assistant_message` que contiene
  el tool_call).

En v0.2 con F12 se introducirá `diff_proposals` table con
`status: 'pending'|'applied'|'rejected'`; aquí se declara
solo como referencia.

## Edge cases

1. **Archivo modificado fuera del agente** (otro proceso
   escribe entre el `read` del agent y el `write`): el diff
   muestra `before` como el contenido que el agent vio en
   el `read` previo, no el actual del disco. Si la UI quiere
   mostrar el diff contra el estado real del disco, debe
   re-leer (no se hace en v0.1). Documentado en tooltip:
   "Diff against agent's view, not latest disk state".
2. **`write_file` sobre archivo que ya existe**: el diff
   muestra el contenido viejo (leído del disco justo antes
   de escribir) y el nuevo. Si el archivo es muy grande
   (>1 MB), `before` se trunca a 8 KiB y `beforeTruncated = true`.
3. **`write_file` sobre archivo nuevo**: `before = null` en
   el payload; el diff se renderiza como "all insertions" con
   el contenido completo de `after`.
4. **`edit_file` con `old_text` que no matchea el archivo**
   (race condition): la tool retorna error; el diff se
   renderiza igual (mostrando lo que se intentó), pero el
   `tool_result` tiene `isError: true` y `outputSummary:
   "old_text not found"`.
5. **Archivo binario** (e.g. una imagen accidentalmente
   pasada como path): detección por nul bytes en los primeros
   8 KiB. El diff muestra `BinaryDiffNotice` con size y mime.
   No se renderiza CodeMirror.
6. **Archivo muy grande** (>1 MB): CodeMirror Merge se
   vuelve lento; el `DiffBody` lazy-renderiza: muestra el
   header y un placeholder "Click to load diff" hasta que
   el usuario lo pida. Esto evita jank al cargar una sesión
   con muchos archivos grandes.
7. **Patch unificado malformado** (no aplica hunks, offsets
   inválidos): el diff no se calcula; `diff = null` en el
   evento; el `ToolCallBlock` se renderiza sin `DiffView`.
8. **Cambio de paths en medio de la sesión** (workspace
   cambió de root): el side panel puede mostrar paths que
   ya no existen. La UI los marca con icono "missing" y
   tooltip. No se borran del journal.
9. **Sesión con 100+ diffs**: el side panel pagina a 50
   items; el resto se muestra con "Show more" (carga la
   siguiente página). CodeMirror Merge solo se monta
   cuando el `DiffView` entra en viewport (lazy).
10. **Diff de un archivo `.env`** u otro path sensible: el
    diff se renderiza con un warning "This file may contain
    secrets". En v0.1 solo es el warning; v0.2 con F12
    podría permitir esconder/redactar.
11. **Búsqueda en el side panel con caracteres regex**:
    el filtro es substring literal, no regex (defensive).
12. **El `chat.tool_call.v1` llega antes que el archivo
    haya sido escrito en disco** (improbable pero posible
    en una race): el renderer pide el contenido via
    `diff_get_full` y el backend hace un `read_file`
    best-effort; si falla, `BinaryDiffNotice` con
    "file not accessible".

## Acceptance criteria

- [ ] **F04.AC1**: tras un `edit_file`, el `MessageList` muestra
  un `DiffView` con `before`/`after` coloreados en rojo/verde,
  counts de `−N +M` en el header, y el `path` clickable
  (abre el archivo en el editor en v1.x; en v0.1 abre
  `show in Finder`/Explorer/Nautilus). **Test**:
  `f04_ac1_edit_file_renders_diff_with_colors_and_counts`.
- [ ] **F04.AC2**: `apply_patch` con un patch unificado válido
  se renderiza con hunks expandibles (CodeMirror Merge los
  colapsa por default si hay >10 hunks). **Test**:
  `f04_ac2_apply_patch_renders_hunks_collapsed_by_default`.
- [ ] **F04.AC3**: `write_file` en archivo nuevo muestra "All
  insertions" con el contenido completo de `after`. **Test**:
  `f04_ac3_write_file_new_renders_all_insertions`.
- [ ] **F04.AC4**: `write_file` sobre un archivo binario (e.g.
  PNG) muestra `BinaryDiffNotice` con size y mime, **no**
  CodeMirror. **Test**:
  `f04_ac4_binary_file_renders_binary_notice`.
- [ ] **F04.AC5**: un diff con `after` de 100 KB (excede 8 KiB)
  tiene `afterTruncated = true`; el `DiffView` muestra la
  primera porción y un `DiffTruncatedNotice` con botón
  "View full" que carga el contenido on-demand vía
  `diff_get_full`. **Test**:
  `f04_ac5_large_diff_truncated_with_view_full_button`.
- [ ] **F04.AC6**: el `DiffsSidePanel` lista todos los diffs
  de la sesión con `{path, additions, deletions, createdAt}`,
  ordenados por `createdAt DESC`. Click en un item hace scroll
  al `DiffView` correspondiente y lo highlight 2s. **Test**:
  `f04_ac6_side_panel_lists_diffs_and_jumps_to_view`.
- [ ] **F04.AC7**: el filtro de búsqueda del side panel es
  case-insensitive substring sobre `path`. **Test**:
  `f04_ac7_side_panel_search_case_insensitive_substring`.
- [ ] **F04.AC8**: el estado collapse del `DiffView` se persiste
  en `localStorage` por path; al volver a la sesión, los
  diffs con `path` previamente colapsado se renderizan
  colapsados. **Test**:
  `f04_ac8_collapse_state_persists_in_localstorage`.
- [ ] **F04.AC9**: cerrar y reabrir la app → la sesión se
  rehidrata con todos los `DiffView` re-renderizados desde
  el journal (cold start). **Test**:
  `f04_ac9_diffs_persist_via_journal_on_reopen`.
- [ ] **F04.AC10**: si el path del diff ya no existe en
  disco (e.g. workspace cerrado, archivo borrado), el
  `DiffView` muestra "file missing" en lugar de intentar
  leer. **Test**: `f04_ac10_missing_file_renders_missing_state`.
- [ ] **F04.AC11**: la detección de binario funciona
  correctamente: PNG, JPEG, ZIP, ELF → `BinaryDiffNotice`;
  texto con caracteres no-ASCII (UTF-8) → diff textual
  normal. **Test**:
  `f04_ac11_binary_detection_handles_unicode_text_correctly`.
- [ ] **F04.AC12**: una sesión con 100 diffs pagina el side
  panel a 50 items; "Show more" carga los siguientes 50.
  CodeMirror Merge no se monta para diffs fuera de viewport.
  **Test**: `f04_ac12_large_session_paginates_and_lazyrenders`.

## Tests

- **Unit (TS)**:
  - `ui/src/lib/components/diff/DiffView.test.ts` — render con
    diff fixture, colapso/expansión, truncation notice.
  - `ui/src/lib/components/diff/DiffsSidePanel.test.ts` —
    lista, búsqueda, scroll-to-view.
  - `ui/src/lib/utils/diff-detection.test.ts` — isBinary
    (nul bytes), isImage (extensión), size thresholds.
- **Integration (Rust)**:
  - `crates/agentyx-core/tests/diff_computation.rs` — diff
    computation en `edit_file` y `apply_patch` con fixtures.
  - `crates/agentyx-core/tests/diff_truncation.rs` — payloads
    grandes con truncation flag correcto.
- **E2E (Playwright)**: `ui/e2e/diff.spec.ts` — flujo completo
  con un tool call que produce diff, verificación visual de
  CodeMirror Merge (colores rojo/verde, counts).
- **Visual regression (Playwright + screenshots)**: en CI, capturar
  el `DiffView` con 3 fixtures (edit_file, apply_patch, write_file
  nuevo) y comparar contra baseline. Tolera ±2% por font rendering.

## Telemetry / logs

```rust
tracing::debug!(
    tool_call_id = %id,
    path = %path,
    kind = %kind,
    additions = adds,
    deletions = dels,
    truncated_before = before_trunc,
    truncated_after = after_trunc,
    is_binary = bin,
    "diff computed"
);
```

> **Nunca** loguear el contenido de `before` o `after` (puede
> tener código, secrets del usuario). Solo el `path` y los
> counts. El `args_summary` ya está truncado en el evento
> (ver F01.AC8).

## Security notes

- **CSP**: CodeMirror 6 se carga vía npm (no inline). El
  `DiffView` no introduce `eval` ni `innerHTML` directo. El
  contenido del diff se pasa a `EditorView` que lo trata
  como texto plano con syntax highlight, no como HTML.
- **Path traversal**: el `path` del diff se valida contra
  el sandbox del workspace (root + extra_paths) antes de
  renderizar. Si el `path` no es válido, el `DiffView` se
  renderiza con `file missing` (no se intenta leer).
- **Secretos en diff**: el diff de un archivo `.env` u otro
  path en `ignore_patterns` se renderiza con un warning.
  En v0.1 el contenido sigue visible; en v0.2 con F12 el
  usuario puede marcar paths como "always redact" (no en
  scope aquí).
- **Click en `path`**: en v0.1 abre el file manager del SO
  en el directorio del archivo (`xdg-open` / `open` /
  `explorer.exe`). No se abre el archivo en un editor
  embebido (eso es v1.x con F40 git integration o un
  editor propio).

## Rollout

- **Feature flag**: no. F04 entra con el MVP; la
  renderización de diffs es **automática** cuando un
  tool call es diffable. No hay toggle.
- **Lazy load**: CodeMirror 6 + `@codemirror/merge` se
  importan en el bundle principal (no lazy) porque el
  size del bundle los hace <100 KB gzipped y son
  necesarios en cada sesión. Si el bundle crece, se
  mueven a `import()` dinámico en el `onMount` de
  `DiffView`.
- **Compatibilidad**: no requiere migración de datos.

## Open questions

- **Q1**: ¿El `side panel` debe ser global (visible siempre)
  o contextual (solo si hay diffs en la sesión actual)?
  → **Contextual**: se muestra solo si la sesión tiene ≥1
  diff, con un toggle siempre visible (con counter "0"
  cuando vacío). Esto evita ruido visual en sesiones sin
  cambios de archivos.
- **Q2**: ¿El "View full" debe leer del disco o del journal?
  → **Del journal** en v0.1 (lo que el agent vio). En v0.2
  se podría añadir un "View current" que lee del disco,
  con un warning si difiere.
- **Q3**: ¿Soporte de **render 3-way** (before / after /
  common ancestor)? → **No en v0.1**. Diferido a v1.x con
  integración git.
- **Q4**: ¿Sintaxis highlight en el diff usa el `language`
  detectado por extensión o el `language` del tool call?
  → **Por extensión del path** (`.rs` → Rust, `.py` → Python,
  `.md` → Markdown, etc.). CodeMirror 6 tiene un set
  built-in; para lenguajes no soportados, fallback a texto
  plano con coloreado de hunks.
- **Q5**: ¿El `collapse state` debe ser per-session o global?
  → **Global** (un solo `localStorage` key `diff-collapse-state`
  con un set de paths colapsados). Más simple; los paths
  suelen repetirse entre sesiones del mismo workspace.
- **Q6**: ¿Los `DiffApplied` / `DiffRejected` se emiten en
  v0.1 aunque no haya UI para ellos? → **No**. v0.1 no
  emite esos eventos. Se introducen en v0.2 con F12.
- **Q7**: ¿Soporte de **renderizar un diff "did this change
  happen?"** sin un tool call explícito (e.g. file watcher
  detecta un cambio externo)? → **No en v0.1**. En v0.3
  con F18 (file_changed events) se podría añadir.

## References

- [`../glossary.md`](../glossary.md) — `ToolCall`, `DiffPayload`,
  `JournalEntry`.
- [`../ipc.md`](../ipc.md) — Tauri command shape, error shape.
- [`../architecture.md`](../architecture.md) — bundle size budget.
- [`tools.md`](../domains/tools.md) — args de `edit_file`,
  `apply_patch`, `write_file`.
- [`journal.md`](../domains/journal.md) — `DiffProposal` kind,
  `truncated` flag, `payload_sha256`.
- [`workspace.md`](../domains/workspace.md) — sandbox de paths,
  `ignore_patterns`, `extra_paths`.
- [`F01-chat-streaming.md`](./F01-chat-streaming.md) —
  `chat.tool_call.v1` enriquecido con `diff`.
- [`F02-multi-workspace.md`](./F02-multi-workspace.md) — sandbox
  para validar el `path` del diff.
- [`features/ROADMAP.md`](./ROADMAP.md) — F04 en Phase 4.
- [CodeMirror 6 Merge docs](https://codemirror.net/docs/merge/).
- AGENTS.md §6.1 (Tools), §15 (Checklist).
