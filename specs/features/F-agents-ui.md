# F-agents-ui — UI multi-agente

**Status**: draft
**Owner**: @miglesias
**Last update**: 2026-06-06
**Affects**: [`agents`](../agents.md) (consume `AgentRegistry`,
`AgentSpec`, `expand_at_mentions`, `invoke_subagent`),
[`session`](../domains/session.md) (active agent per session),
[`F01`](./F01-chat-streaming.md) (consume `agent.changed.v1` y
eventos de subagent), `ui` (Svelte components y stores).
**Depends on**: [`F01`](./F01-chat-streaming.md) (chat funciona
y emite eventos), [`F02`](./F02-multi-workspace.md) (workspaces
existen), [`agents.md`](../agents.md) (modelo de agentes), UI shell
existe (lib/app shell + sidebar).

## Agent context

- Leer primero este bloque, `Affected Tauri commands / endpoints /
  events` y `Acceptance criteria`; la UX larga solo aplica al tocar
  `AgentChip`, `AtMentionPopover`, `SessionTree` o shortcuts.
- Objetivo MVP: exponer el modelo multi-agent ya definido en
  `agents.md`: elegir/ciclar primary agents (`build`, `plan`),
  mencionar subagent `@general`, y mostrar child sessions en sidebar.
- Contratos clave: `agents_list`, `agents_get`,
  `session_set_active_agent`, `session_get_active_agent`,
  `agents_invoke_subagent`; eventos `agent.changed.v1`,
  `subagent.started.v1`, `subagent.finished.v1`,
  `subagent.aborted.v1`.
- Pendientes de F01/agents: `@mention` expansion, child session schema
  (`parent_session_id`) y bloqueo de cambio de active agent durante
  runs activos.
- No incluir hidden agents en UI; no custom agents editor en v0.1.

> Nombre informal: **F-agents-ui**. No tiene número `F<NN>` porque
> se introduce como **feature complementaria** al MVP tras la
> decisión de Fase B. Cuando entre en producción se le asignará
> `F<NN>` correlativo; mientras tanto, este spec usa el slug
> "agents-ui" para evitar colisión con el dominio `agents.md`.

## User story

Como **usuario**, quiero **alternar entre los primary agents (build
y plan) con la tecla Tab, mencionar subagents con `@<id>` desde el
composer, y ver en el sidebar un árbol de las child sessions que el
agente ha creado al delegar a subagents**, para aprovechar el modelo
multi-agente desde la UI sin tener que saber qué hay "por debajo".

## Scope

### In-scope (v0.1)

- **`AgentChip` en el header del composer**: muestra el active
  agent de la sesión con color e icono. Click abre un menú con
  la lista de primary agents (vienen de `AgentRegistry::primary_ids()`).
  Selección cambia el active agent vía `session_set_active_agent`.
- **Cycle con Tab**: handler global que, al pulsar `Tab` cuando
  el foco no está en un input de texto, cicla al siguiente primary
  agent. Si solo hay 1 primary visible, no-op.
- **`@mention` popover en el composer**: al teclear `@`, abre
  un popover con la lista de subagents del
  `AgentRegistry::subagents()`. Cada item muestra `id` y
  `description`. Selección inserta `@<id> ` con un chip visible
  en el composer. El `session_send` con mentions se encarga del
  resto (ver F01).
- **Sidebar de sesiones con árbol jerárquico**: cada sesión
  tiene `parent_session_id` (null para top-level). El sidebar
  renderiza un tree view, expandible/colapsable, con badges
  por sesión: `agent_id`, `status` (running / done / aborted /
  error), `message_count`. Click en una sesión la carga en el
  panel principal.
- **Tabs en el header del workspace**: cuando hay >1 sesión
  abierta (F13, v0.2), se muestran tabs; v0.1 solo permite
  1 sesión activa por workspace, pero la **infraestructura** de
  tabs existe (el slot en el header está reservado, oculto
  si no hay >1 sesión).
- **Indicador de subagent en vivo**: cuando un subagent arranca
  (`subagent.started.v1`), el sidebar muestra un dot pulsante
  en la child session con el `subagent_id`. Cuando termina
  (`subagent.finished.v1`), el dot cambia a check (success) o
  X (error/aborted) con el `TaskResult` resumido en tooltip.
- **Cambio de agent preserva el contexto de mensajes**: el
  active agent es un setting de la sesión; cambiarlo no borra
  el historial ni los runs previos. Los runs futuros usan el
  nuevo `AgentSpec`.

### Out-of-scope (v0.1)

- ❌ Crear/editar **custom agents** desde la UI (v1.x con
  F-extra-agents). En v0.1 los agents son los 3 built-in
  declarados en `agents.md`.
- ❌ Cycle entre **hidden agents** (compaction, title, summary).
  Esos son internos; la UI no los expone.
- ❌ Configurar el **`model` por agent** desde la UI (v0.1 todos
  usan el `default_model` del workspace; v1.x permite override).
- ❌ Comunicación entre agentes fuera del modelo
  "primary → subagent" (no hay canales side-channel en v1).
- ❌ **Drag & drop** para reordenar sesiones o mover child
  sessions entre parents.
- ❌ **Rename** de una sesión desde la sidebar (en v0.1 el
  título se deriva del primer mensaje truncado a 60 chars;
  v1.x permite editar).
- ❌ **Visualizar el transcript del subagent** en una vista
  separada (v0.1 lo muestra inline en el parent message; v0.2
  con F-agents-ui-extended lo separa).
- ❌ **Persistencia del collapse state del tree** entre reinicios
  (en v0.1 el tree se expande por default; v1.x lo persiste
  en `localStorage` por workspace).

## UX / UI

### Rutas y componentes

```
ui/src/
├── lib/
│   ├── components/
│   │   ├── agents/
│   │   │   ├── AgentChip.svelte           # chip en header del composer
│   │   │   ├── AgentPickerMenu.svelte     # menú que se abre al click
│   │   │   ├── AtMentionPopover.svelte    # popover al teclear @
│   │   │   ├── SubagentLiveDot.svelte     # dot pulsante en sidebar
│   │   │   ├── SessionTree.svelte         # árbol en sidebar
│   │   │   ├── SessionTreeNode.svelte     # nodo individual
│   │   │   └── SessionTabs.svelte         # tabs (placeholder en v0.1)
│   ├── stores/
│   │   ├── agents.svelte.ts               # AgentRegistry en runes
│   │   ├── active-agent.svelte.ts         # active agent per session
│   │   └── tab-cycle.svelte.ts            # handler de Tab
```

### `AgentChip` + `AgentPickerMenu`

```
+----------------------------------+
|  [🔨 build ▼]                    |  <-- AgentChip en header
+----------------------------------+
         | click
         v
+----------------------------------+
|  Primary agents                  |
|  ● 🔨 build (default)            |
|  ○ 📋 plan (read-only)           |
+----------------------------------+
```

- **Color por agent** (consistente en toda la app):
  - `build` → azul
  - `plan` → ámbar
  - `general` → verde
  - Custom agents (v1.x) → color derivado del `id` (hash).
- **Icono** por agent: 🔨 para `build`, 📋 para `plan`, 🤖 para
  `general`. Custom agents: emoji por defecto o provisto en
  frontmatter (v1.x).
- **Tooltip** con `description` del agent al hacer hover.
- El active agent se marca con `●`; los demás con `○`.
- Click en un item llama `session_set_active_agent(sessionId,
  agentId)`. Si el cambio es exitoso, `AgentChip` se actualiza
  con el nuevo agent. Si falla (e.g. `conflict` por run
  activo), toast con el error.

### Cycle con Tab

```
user: Tab (con foco fuera de input)
  → handleTabCycle()
  → const ids = AgentRegistry::primary_ids()  // ["build", "plan"]
  → const current = activeAgent.value
  → const next = ids[(ids.indexOf(current) + 1) % ids.length]
  → session_set_active_agent(sessionId, next)
  → emit "agent.changed.v1" → AgentChip se actualiza
```

**Reglas**:
- Solo cicla si hay `>1` primary visible.
- Si el foco está en un input (composer, settings search, etc.),
  el Tab es del focus traversal normal, no del cycle. Para evitar
  ambigüedad, el cycle se activa con **`Shift+Tab`** o con un
  shortcut dedicado (e.g. `Cmd+Shift+T` o `Cmd+[` / `Cmd+]`).
- En v0.1 el shortcut primario es **`Cmd+[`** y **`Cmd+]`** (prev /
  next primary agent). `Tab` se reserva para el comportamiento
  estándar del navegador/Svelte.
- El cycle durante un run activo retorna `conflict` (vía
  `session_set_active_agent`); la UI muestra toast "Wait for
  the current run to finish".

### `@mention` popover

```
+----------------------------------+
|  Type a message... @gen|          |  <-- composer con @
+----------------------------------+
        |
        v
+----------------------------------+
|  Subagents                       |
|  🤖 general                      |
|     General-purpose subagent...  |
+----------------------------------+
```

- Trigger: el usuario teclea `@` en el composer. Se detecta
  con un `$effect` que mira el `value` del textarea.
- Filtro: si el usuario sigue tecleando letras después del
  `@`, el popover filtra por prefijo case-insensitive
  (`@gen` → solo `general`).
- Items vienen de `AgentRegistry::subagents()`.
- Selección (Enter, click, o `→`/`Tab`):
  - El texto del composer se actualiza: reemplaza el
    fragmento `@<prefix>` por `@<agent-id> ` con un **chip**
    visible (estilizado como background del color del agent).
  - El popover se cierra.
  - El cursor se posiciona después del chip.
- Si no hay subagents (caso raro; v0.1 siempre tiene
  `general`): el popover muestra "No subagents available".
- Si el usuario escribe `@nonexistent`, no se valida en el
  composer; la validación ocurre en el backend en
  `expand_at_mentions` (F01) y retorna `invalid_input`.

> **El chip no es interactivo** en v0.1 (no se puede quitar
> con click). El usuario puede borrarlo con `Backspace`
> como texto normal. v1.x: chip con botón `×` para quitar.

### `SessionTree` (sidebar)

```
+----------------------------------+
|  Workspace: my-project           |
|  [search sessions...]            |
+----------------------------------+
|  ●  Session "fix auth bug"   🟢  |  <-- active, running subagent
|  │   3 messages · 2 runs         |
|  │   ●  "explore auth"      🟢  |  <-- child session (subagent)
|  │   │   1 message · 1 run       |
|  │   ●  "summarize findings" ✓  |  <-- child session (done)
|  │       1 message · 1 run       |
|  ○  Session "refactor utils" ✓  |  <-- top-level, done
+----------------------------------+
```

- **Active session**: marcada con `●` y highlight. Las no
  activas con `○`.
- **Subagent status**:
  - `running` → dot pulsante azul 🟢 (o color del subagent).
  - `done` → check verde ✓.
  - `aborted` → X gris.
  - `error` → X rojo.
- **Click** en una sesión → la carga en el panel principal.
  El active session cambia.
- **Toggle** del tree (expand/collapse) por nodo: click en
  el icono `▶`/`▼` al lado del nodo.
- **Orden**: por `updated_at DESC`. El más reciente arriba.
- **Filtro de búsqueda**: substring case-insensitive sobre
  título de la sesión. En v0.1 filtra también child
  sessions; v1.x añade scope (top-level only).

### `SessionTabs` (placeholder v0.1)

En v0.1 solo hay 1 sesión activa, así que `SessionTabs` se
renderiza vacío. La estructura DOM existe (con `data-active-count=1`)
para que F13 (multi-sesión simultánea) solo tenga que
rellenar el state, no la UI.

### Estados visuales

| Estado | Indicador |
|---|---|
| `idle` (no run activo) | AgentChip normal, dot apagado en sidebar |
| `cycling` (cambio de agent en curso) | AgentChip con spinner breve |
| `running` (run activo en primary) | Composer disabled, AgentChip con dot pulsante |
| `running` (run activo en subagent) | Sidebar con dot pulsante en la child |
| `conflict` (Tab cycle durante run) | Toast rojo "Wait for the current run" |
| `subagent_running` | Popover de subagents muestra "(running)" en items activos |

## Flow

### Cambio de active agent con click

```
user: click AgentChip → click "plan" en AgentPickerMenu
  → AgentPickerMenu.svelte onSelect
  → ipc.invoke("session_set_active_agent", { sessionId, agentId: "plan" })
  → Tauri command en commands/session.rs
  → SessionService::set_active_agent(sessionId, "plan")
    ├── lee sesión, valida agent_id existe en registry
    ├── UPDATE sessions SET agent_id = "plan" WHERE id = sessionId
    ├── emit "agent.changed.v1" { sessionId, from: "build", to: "plan" }
    └── return Ok(())
  → frontend recibe Ok + evento
  → activeAgent.value = "plan"
  → AgentChip se actualiza con color/label de "plan"
  → tooltip muestra "plan: Read-only analysis and planning..."
```

### Cycle con Cmd+[

```
user: Cmd+[ (con foco en cualquier parte de la app, no en input)
  → handleTabCycle() en stores/tab-cycle.svelte.ts
  → const ids = AgentRegistry::primary_ids()  // ["build", "plan"]
  → si ids.length <= 1: return  // no-op
  → const current = activeAgent.value
  → const idx = ids.indexOf(current)
  → const next = ids[(idx - 1 + ids.length) % ids.length]  // prev
  → session_set_active_agent(sessionId, next)
  → (mismo flujo que click)
```

> **Cmd+]** es el shortcut para **next** primary agent.
> `Cmd+[` es **prev**. La simetría con tab navigation
> (izq/der) ayuda a la memoria.

### @mention en composer

```
user: "explora los @ge" en composer
  → Composer.svelte detecta "@" + "ge" en el value
  → $effect: filteredSubagents = subagents.filter(s => s.id.startsWith("ge"))
  → AtMentionPopover.svelte se posiciona bajo el cursor
  → render: [🤖 general]
  → user: Enter o click
  → composer value actualizado: "explora los @general "
  → chip visible con color verde (color de "general")
  → popover cerrado
  → user: Enter para submit
  → ipc.invoke("session_send", { sessionId, content, mentions: [{ agentId: "general", range: [11, 19] }] })
  → Tauri command expand_at_mentions
  → antes del primary run, invoca subagent "general" con prompt = "explora los "
  → subagent.run completo, retorna TaskResult
  → TaskResult se inserta como assistant_message con agentId: "general"
  → primary run arranca con el prompt original + el TaskResult como contexto
```

### Subagent lifecycle en sidebar

```
agent loop: primary emite un "task" tool call con subagent_id: "general"
  → AgentLoop::invoke_subagent
    ├── genera child_session_id
    ├── child_session.parent_session_id = parent_session_id
    ├── child_session.agent_id = "general"
    ├── arranca child run
    ├── emit "subagent.started.v1" { parentRunId, childSessionId, subagentId }
    └── espera a que termine
  → al terminar:
    ├── emit "subagent.finished.v1" { parentRunId, childSessionId, result }
    ├── return TaskResult al parent
  → subagent.aborted.v1 si el parent aborta o se agota el timeout
```

> El sidebar escucha `subagent.started.v1` y crea
> programáticamente el nodo en el tree (o lo muestra si ya
> existe). `subagent.finished.v1` actualiza el status.

## Affected domains

- [`agents.md`](../agents.md) — consume `AgentSpec`,
  `AgentRegistry`, `expand_at_mentions`, `invoke_subagent`,
  `set_active_agent`. Los eventos `agent.changed.v1`,
  `subagent.*.v1` ya están definidos en `agents.md`; F-agents-ui
  los **consume**, no los redefine.
- [`session.md`](../domains/session.md) — `state.db` con tabla
  `sessions` se extiende con `parent_session_id` (nullable)
  para soportar el tree. `session_set_active_agent` y
  `session_get_active_agent` (de F01) se usan.
- [`F01`](./F01-chat-streaming.md) — `session_send` con
  `mentions` y los eventos `subagent.*.v1` son el contrato
  que esta feature renderiza.
- `ui` — sin dominio formal; los stores y componentes son
  puramente frontend.

## Affected Tauri commands / endpoints / events

### Tauri commands (F-agents-ui)

> La mayoría de los commands necesarios ya están definidos
> en [`agents.md`](../agents.md) y en F01. F-agents-ui solo
> añade 2 commands nuevos.

```rust
// Ya en agents.md:
#[tauri::command]
pub async fn agents_list() -> Result<Vec<AgentInfo>, AppError>;

#[tauri::command]
pub async fn agents_get(id: AgentId) -> Result<AgentInfo, AppError>;

// Ya en F01:
#[tauri::command]
pub async fn session_set_active_agent(
    session_id: SessionId,
    agent_id: AgentId,
) -> Result<(), AppError>;

#[tauri::command]
pub async fn session_get_active_agent(
    session_id: SessionId,
) -> Result<AgentId, AppError>;

// Nuevos en F-agents-ui:
#[tauri::command]
pub async fn session_list_tree(
    workspace_id: WorkspaceId,
) -> Result<Vec<SessionTreeNodeDto>, AppError>;

#[tauri::command]
pub async fn session_get_subagents(
    session_id: SessionId,
) -> Result<Vec<SubagentSummaryDto>, AppError>;
```

> `session_list_tree` devuelve la lista de sesiones con
> `parent_session_id` populado y estructura anidada (no
> plana). El frontend la consume para renderizar el
> `SessionTree`. La query es recursiva: el backend
> itera hasta depth=2 (v0.1: subagents son depth 1).
>
> `session_get_subagents` devuelve la lista de child
> sessions (subagents invocados) de una sesión, con su
> `TaskResult` resumido. Es un subset de
> `session_list_tree` filtrado y proyectado.

### Endpoints HTTP (v0.2, F06)

```
GET    /api/v1/agents                                → Vec<AgentInfo>
GET    /api/v1/agents/:id                            → AgentInfo
POST   /api/v1/sessions/:id/active-agent             (body: { agentId }) → {}
GET    /api/v1/sessions/:id/active-agent             → AgentId
GET    /api/v1/workspaces/:id/sessions/tree          → Vec<SessionTreeNodeDto>
GET    /api/v1/sessions/:id/subagents                → Vec<SubagentSummaryDto>
```

### Eventos (F-agents-ui)

> F-agents-ui **no introduce eventos nuevos** en v0.1.
> Reutiliza los ya definidos en [`agents.md`](../agents.md):
>
> | Evento | Schema | Payload | Cuándo |
> |---|---|---|---|
> | `agent.changed.v1` | `{ sessionId, fromAgentId, toAgentId }` | Cycle con Tab/Click |
> | `subagent.started.v1` | `{ parentRunId, childSessionId, subagentId }` | Cuando un primary delega |
> | `subagent.finished.v1` | `{ parentRunId, childSessionId, result }` | Cuando el subagent termina |
> | `subagent.aborted.v1` | `{ parentRunId, childSessionId, reason }` | Aborto |
>
> F-agents-ui **escucha** estos eventos y actualiza la UI
> (AgentChip, SessionTree, SubagentLiveDot). El agente
> loop (F01) los **emite**.

### Tablas (extensión de `state.db`)

```sql
-- session.md (F01 ya define sessions; aquí se añade parent_session_id)
ALTER TABLE sessions ADD COLUMN parent_session_id TEXT
  REFERENCES sessions(id) ON DELETE CASCADE;
ALTER TABLE sessions ADD COLUMN subagent_id TEXT;  -- si es child, qué agent
ALTER TABLE sessions ADD COLUMN task_result_json TEXT;  -- TaskResult al terminar

CREATE INDEX idx_sessions_parent ON sessions(parent_session_id);
```

> En F01 ya está previsto `parent_session_id` en la
> tabla `sessions` para soportar subagents (F01 AC10).
> Esta spec solo **declara explícitamente** los campos
> adicionales que el sidebar consume: `subagent_id` y
> `task_result_json`.

## Edge cases

1. **`@<id>` en un mensaje donde el agent no existe**:
   `expand_at_mentions` retorna `invalid_input` y `session_send`
   falla; la UI muestra el error inline en el composer (toast
   rojo "Unknown subagent: <id>"). El composer **no** limpia
   el texto; el usuario puede corregir.
2. **`@<id>` apuntando a un `primary`** (no subagent):
   `invalid_input` ("@mention only works with subagents").
   La UI lo previene a nivel de popover (no muestra primaries
   en la lista), pero si el usuario fuerza el texto, el
   backend rechaza.
3. **Cycle con Tab durante un run activo**: `conflict` desde
   `session_set_active_agent`. La UI muestra toast "Wait for
   the current run to finish" y el AgentChip **no** cambia
   visualmente.
4. **Cycle con Tab cuando hay 1 solo primary visible**:
   no-op. Si el usuario está en un input, Tab hace focus
   traversal normal.
5. **Subagent que tarda mucho (>10 min)**: timeout absoluto.
   El run del subagent aborta con `status: "timeout"`. La
   UI muestra `task_result_json = { status: "timeout" }`
   en el sidebar.
6. **Sidebar tree con >50 sesiones**: el tree pagina a 25
   items visibles; "Show more" expande. El resto se
   mantiene colapsado.
7. **Click en un subagent node del sidebar**: en v0.1 no
   hace nada (no hay vista separada del subagent transcript).
   El cursor cambia a `not-allowed` y un tooltip indica
   "Subagent view coming in v0.2".
8. **`session_set_active_agent` con un `agentId` que no
   es primary** (e.g. `general`): `invalid_input` desde
   `agents.md` AC9. La UI previene esto (AgentPickerMenu
   solo lista primary agents).
9. **Cambio de active agent cuando la sesión tiene un run
   activo**: `conflict` (ver F01 AC14 y `agents.md` AC10).
10. **`agent.changed.v1` se emite pero el frontend no lo
    recibe** (p. ej. el evento se pierde): la UI puede
    desincronizarse. La mitigación: el frontend hace
    `session_get_active_agent` al `onMount` de `ChatPanel`
    y al `onFocus` de la ventana. Si la diferencia persiste,
    refetch completo del tree.
11. **Subagent que se invoca a sí mismo recursivamente**
    (vía `task` tool call apuntando a un agent que también
    es subagent que invoca a `general`): el agent loop
    detecta `depth_exceeded` (ver `agents.md` Edge 3) y
    emite `subagent.aborted.v1` con `reason: "depth"`.
    El sidebar muestra el dot rojo.
12. **`subagent.started.v1` llega antes que el
    `session.create` del backend termine de persistir**:
    el sidebar hace un fetch completo del tree al recibir
    el evento si la child session no está en su state.
13. **Workspace con >100 child sessions** (muchos subagents
    anidados en una sola sesión): el tree muestra los
    primeros 25 con paginación; el resto se omite con
    "Show more".
14. **`TaskResult` con `files_changed` que apunta a paths
    fuera del workspace** (no debería pasar, pero el LLM
    puede "mentir" en su output): el backend filtra los
    paths a `root + extra_paths` antes de emitir
    `subagent.finished.v1`. Los paths inválidos se omiten
    con un `tracing::warn!`.
15. **Foco del usuario al recibir `subagent.started.v1`**
    (e.g. el usuario está escribiendo en el composer):
    el sidebar se actualiza en background sin robar foco.
    El dot pulsante es visual, no interactivo.
16. **Cambio de agent cuando el `description` del nuevo
    agent tiene caracteres no-ASCII**: el tooltip y el
    `AgentChip` los renderizan correctamente (UTF-8).
    Sin XSS (todo se trata como texto).

## Acceptance criteria

- [ ] **F-agents-ui.AC1**: el `AgentChip` en el header del
  composer muestra el active agent actual con color e icono
  correctos. Al hacer click, abre `AgentPickerMenu` con la
  lista de primary agents (`build`, `plan` en v0.1). **Test**:
  `f_agents_ui_ac1_agent_chip_shows_active_and_picker_lists_primaries`.
- [ ] **F-agents-ui.AC2**: seleccionar "plan" en el menu llama
  `session_set_active_agent(sessionId, "plan")` y, tras el Ok,
  el `AgentChip` se actualiza con el color/label de "plan".
  El header del composer persiste tras un refresh. **Test**:
  `f_agents_ui_ac2_selecting_plan_persists_and_renders`.
- [ ] **F-agents-ui.AC3**: pulsar `Cmd+[` con foco fuera de
  un input cambia el active agent al previous primary. Con
  `Cmd+]` cambia al next. Si solo hay 1 primary visible,
  no-op (sin error, sin toast). **Test**:
  `f_agents_ui_ac3_cmd_brackets_cycle_primary_agents`.
- [ ] **F-agents-ui.AC4**: pulsar `Cmd+[` durante un run
  activo retorna `conflict` desde el backend; la UI muestra
  toast rojo "Wait for the current run to finish" y el
  `AgentChip` **no** cambia. **Test**:
  `f_agents_ui_ac4_cycle_during_run_blocked_with_toast`.
- [ ] **F-agents-ui.AC5**: teclear `@` en el composer abre
  `AtMentionPopover` con la lista de subagents (`general` en
  v0.1). Teclear `@ge` filtra a `[general]`. **Test**:
  `f_agents_ui_ac5_at_mention_popover_filters_by_prefix`.
- [ ] **F-agents-ui.AC6**: seleccionar un subagent del popover
  reemplaza el fragmento `@<prefix>` por `@<agent-id> ` con
  un chip visible (background del color del agent). El
  composer mantiene el foco y el cursor se posiciona tras
  el chip. **Test**:
  `f_agents_ui_ac6_at_mention_selection_inserts_chip`.
- [ ] **F-agents-ui.AC7**: enviar un mensaje con `@general
  busca los archivos de auth` dispara un subagent run;
  el sidebar muestra la child session con dot pulsante
  (azul o color del subagent) mientras corre. **Test**:
  `f_agents_ui_ac7_at_mention_invocation_creates_child_session_in_sidebar`.
- [ ] **F-agents-ui.AC8**: al terminar el subagent, el dot
  cambia a check verde (success) o X rojo (error/aborted)
  con tooltip mostrando el `TaskResult.summary` truncado a
  200 chars. **Test**:
  `f_agents_ui_ac8_subagent_finished_dot_changes_to_status`.
- [ ] **F-agents-ui.AC9**: el `SessionTree` muestra la sesión
  activa y todas las child sessions con su `parent_session_id`
  correctamente anidadas. Expandir/colapsar nodo funciona.
  **Test**: `f_agents_ui_ac9_session_tree_renders_nested_hierarchy`.
- [ ] **F-agents-ui.AC10**: el filtro de búsqueda en el
  sidebar es case-insensitive substring sobre el título de
  la sesión. No es regex. **Test**:
  `f_agents_ui_ac10_session_tree_search_case_insensitive_substring`.
- [ ] **F-agents-ui.AC11**: cerrar y reabrir la app → el
  `SessionTree` se reconstruye desde `session_list_tree`;
  el active session se marca correctamente. **Test**:
  `f_agents_ui_ac11_session_tree_persists_across_app_restart`.
- [ ] **F-agents-ui.AC12**: `@<nonexistent>` en el composer
  se envía; el backend retorna `invalid_input`; la UI muestra
  toast con el `code` y el composer **no** limpia el texto.
  **Test**: `f_agents_ui_ac12_unknown_at_mention_error_inline`.
- [ ] **F-agents-ui.AC13**: `@build` (un primary) en el
  composer se envía; el backend retorna `invalid_input`
  ("@mention only works with subagents"). **Test**:
  `f_agents_ui_ac13_at_primary_returns_invalid_input`.
- [ ] **F-agents-ui.AC14**: si la lista de subagents cambia
  (v1.x con custom agents), el popover se actualiza en el
  siguiente `@` (no se cachea entre keystrokes; se reconsulta
  el registry al abrir). **Test**:
  `f_agents_ui_ac14_subagent_list_refreshed_on_popover_open`.
- [ ] **F-agents-ui.AC15**: el `AgentChip` muestra el color
  correcto del agent. En el caso de `plan`, el icono es 📋
  y el color es ámbar. **Test**:
  `f_agents_ui_ac15_agent_chip_color_and_icon_per_agent`.

## Tests

- **Unit (TS)**:
  - `ui/src/lib/components/agents/AgentChip.test.ts` —
    render con active agent, click → menu.
  - `ui/src/lib/components/agents/AgentPickerMenu.test.ts` —
    selección, `set_active_agent` invocado.
  - `ui/src/lib/components/agents/AtMentionPopover.test.ts` —
    filtro, navegación con teclado, selección.
  - `ui/src/lib/components/agents/SessionTree.test.ts` —
    tree rendering, expand/collapse, búsqueda.
  - `ui/src/lib/stores/tab-cycle.svelte.test.ts` — `Cmd+[`,
    `Cmd+]`, no-op con 1 primary.
- **Integration (Rust)**:
  - `crates/agentyx-core/tests/agents_registry_integration.rs` —
    carga de registry con 3 built-in, override de active
    agent, persistencia en `state.db`.
  - `crates/agentyx-core/tests/subagent_lifecycle.rs` —
    spawn, ejecución, TaskResult, persistencia de child
    session.
- **E2E (Playwright)**: `ui/e2e/agents-ui.spec.ts` —
  - Flujo: click chip → select plan → enviar mensaje →
    el message usa el system prompt de plan.
  - Flujo: `@general` en composer → child session en
    sidebar → dot pulsante → completion → check.
  - Flujo: cerrar app → reopen → tree reconstruido.
- **Visual regression**: screenshots de `AgentChip` y
  `SessionTree` con varios estados (1 sesión, 1 sesión
  con 3 childs, etc.).

## Telemetry / logs

```rust
tracing::info!(
    session_id = %session_id,
    from_agent = %from,
    to_agent = %to,
    "active agent changed"
);

tracing::info!(
    parent_run_id = %parent_run_id,
    child_session_id = %child_session_id,
    subagent_id = %subagent_id,
    "subagent started"
);

tracing::info!(
    parent_run_id = %parent_run_id,
    child_session_id = %child_session_id,
    duration_ms = ms,
    status = %status,
    files_changed = files.len(),
    "subagent finished"
);
```

> **Nunca** loguear:
> - El `prompt` completo del subagent (puede tener código
>   del usuario).
> - El `TaskResult.summary` si excede 200 chars.
> - Los `files_changed` con paths absolutos completos (log
>   paths relativos al workspace root).

## Security notes

- **Capabilities Tauri**: la ventana principal tiene permiso
  para los commands `agents_*`, `session_set_active_agent`,
  `session_list_tree`, `session_get_subagents`. El popover
  de `@mention` no expone más superficie que el chat
  principal.
- **XSS en `description` de un agent**: si en v1.x un
  custom agent tiene `description` con HTML, la UI lo
  trata como texto plano (escape automático en Svelte).
  Sin `innerHTML` directo.
- **Path traversal en `TaskResult.files_changed`**: el
  backend filtra contra `root + extra_paths` antes de
  emitir el evento. La UI no lee paths del disco basados
  en el `TaskResult`; los paths son solo metadata visual.
- **Subagent `prompt` en el chip**: el texto del chip
  contiene el `prompt` que el usuario pasó al `@<id>`. La
  UI lo trata como texto plano (escape). El `@<id>` mismo
  se valida contra `AgentRegistry::subagents()` (no se
  renderiza como link hasta que el backend confirma).
- **Active agent desde URL**: en v0.1 no se permite deep
  link con `?agent=plan` (F-extra-deep-link, v1.x). El
  active agent siempre se lee del state del backend.

## Rollout

- **Feature flag**: no. F-agents-ui entra con el MVP. La
  base multi-agent ya está implementada en `agents.md`
  (registry, child sessions); esta feature es **la
  exposición visual** de esa base.
- **Compatibilidad**: requiere que `state.db::sessions`
  tenga `parent_session_id` (F01 lo declara). Si F01
  entra antes que F-agents-ui, el campo existe; si entran
  en el mismo PR, migración atómica.
- **Dependencia de keyboard shortcuts**: `Cmd+[` /
  `Cmd+]` se registran en el atajo global de la app.
  v1.x con F24 introduce un panel de shortcuts
  configurable; en v0.1 los atajos son hard-coded.
- **Migración de datos**: ninguna. Las sesiones
  pre-existentes (de versiones internas o de F01) se
  cargan sin `parent_session_id` (null = top-level).

## Open questions

- **Q1**: ¿El cycle con Tab debe ser un **handler global**
  o un handler local de `ChatPanel`? → **Global**, pero
  solo se activa si el foco no está en un input (regla
  en `tab-cycle.svelte.ts`). Esto permite cycle desde
  cualquier parte de la app (e.g. mientras se mira el
  sidebar).
- **Q2**: ¿El sidebar muestra **transcript resumido** del
  subagent (1-2 líneas) o solo el `TaskResult`?
  → En v0.1 solo el `TaskResult`. En v0.2 con
  F-agents-ui-extended se añade un preview expandible.
- **Q3**: ¿Custom agents con `mode: primary` en v1.x
  entran automáticamente en el cycle con Tab? → **Sí**,
  con la posición en el orden del registry (custom
  después de built-in, ordenados alfabéticamente).
- **Q4**: ¿El `@mention` popover debe mostrar **descripción
  detallada** o solo `id`? → En v0.1: `id` + `description`
  truncada a 1 línea. v1.x con custom agents podría
  mostrar más (icono custom, ejemplos de uso del
  frontmatter).
- **Q5**: ¿El `SessionTree` debe soportar **drag & drop**
  para reparentar? → **No en v0.1** ni en v1.x. El
  `parent_session_id` lo define el agent loop, no el
  usuario.
- **Q6**: ¿El `SessionTabs` placeholder en v0.1 debe
  renderizar **al menos un tab** con la sesión activa
  (para que la estructura sea visible)? → **No**. El
  placeholder es invisible (`data-active-count=1` en el
  DOM, sin tabs renderizados). F13 introduce los tabs
  visibles.
- **Q7**: ¿Soporte de **renombrar** la sesión desde el
  sidebar? → **No en v0.1**. Diferido a v1.x.
- **Q8**: ¿El color por agent debe ser **configurable**
  por el usuario (Settings)? → **No en v0.1**. v1.x con
  F-extra-agents podría permitir override del color
  en el frontmatter del custom agent.
- **Q9**: ¿El `subagent.aborted.v1` con `reason: "depth"`
  debe mostrarse de forma distinguible en el sidebar
  (e.g. con icono "infinite")? → **Sí**, con un icono
  distinto (no X rojo genérico). Documentado en el
  componente.

## References

- [`../glossary.md`](../glossary.md) — `AgentSpec`, `AgentMode`,
  `ActiveAgent`, `ChildSession`, `TaskResult`.
- [`../ipc.md`](../ipc.md) — Tauri command shape, error shape,
  eventos.
- [`../architecture.md`](../architecture.md) — flujo de eventos.
- [`agents.md`](../agents.md) — modelo completo: `AgentSpec`,
  `AgentRegistry`, `expand_at_mentions`, `invoke_subagent`,
  eventos `agent.changed.v1`, `subagent.*.v1`.
- [`session.md`](../domains/session.md) — tabla `sessions`,
  `parent_session_id`, `state.db`.
- [`F01-chat-streaming.md`](./F01-chat-streaming.md) —
  `session_send` con `mentions`, eventos consumidos.
- [`F02-multi-workspace.md`](./F02-multi-workspace.md) —
  sesiones pertenecen a un workspace.
- [`features/ROADMAP.md`](./ROADMAP.md) — F-agents-ui en
  Phase 5.
- [opencode-dev agents.mdx](../../opencode-dev/packages/web/src/content/docs/agents.mdx) —
  referencia de UX (cycle con Tab, subagents, child
  sessions).
- AGENTS.md §2.9 (Multi-agent), §6 (Agent loop), §15
  (Checklist).
