# Agents

**Status**: draft
**Owner**: @miglesias
**Last update**: 2026-06-05
**Affects**: — (los agentes son consumidos por `agent-loop.md`; no al revés).
**Required by**: `agent-loop.md` (carga el `AgentSpec` activo), `tools.md`
(la `tool_access` del agent filtra qué tools se exponen al provider),
`permissions.md` (los agents pueden tener `permissions` propios que
heredan/overridean los del workspace), `features/F01-chat-streaming`
(la UI muestra el agent activo, permite cycle con Tab, `@mention`).
**Required by (v1.x)**: editor de agentes custom en UI, ciclo de vida de
sesiones child.

> Modela el **sistema de agentes** de Agentyx: qué es un agente, qué
> tipos hay (`Primary | Subagent | Hidden`), cómo se carga, cómo se
> invoca, y cómo se diferencia v1 (3 built-ins) de v1.x (agentes
> custom definidos por el usuario).
>
> Decisión de fondo: aunque v1 solo trae 2 primary + 1 subagent
> built-in, **toda la arquitectura** está diseñada para soportar
> multi-agente desde el día 1. Esto evita refactors masivos cuando
> se añadan agentes custom.

## Goal

Definir:
- El modelo de datos `AgentSpec` (campos, variantes, validaciones).
- Los 3 modos (`Primary`, `Subagent`, `Hidden`) y sus diferencias de
  ciclo de vida, invocación y visibilidad.
- El mecanismo de carga: built-in (Rust) + custom (archivo markdown
  en `~/.agentyx/agents/*.md` y `.agentyx/agents/*.md`).
- El mecanismo de invocación:
  - **Primary** → cycle con Tab (cambia el active agent de la sesión)
    o `agents_set_active` Tauri command.
  - **Subagent** → `task` tool call emitido por un primary, o `@<id>`
    en un mensaje del usuario (manual).
  - **Hidden** → invocado automáticamente por el sistema (compaction,
    title generation, etc.).
- El contrato entre el agent loop y un agent concreto (qué recibe,
  qué emite, qué permisos aplica).
- La **separación de child sessions** cuando un subagent es invocado
  (cada uno tiene su propio journal y replays).

## Non-goals

- ❌ Editor visual de agentes en v1 (los custom en v1.x se editan como
  archivo markdown; UI de CRUD en v1.x).
- ❌ Comunicación entre agentes fuera del canal de "tool result /
  assistant message" del loop. No hay protocolo custom inter-agent en v1.
- ❌ Memoria persistente cross-session por agente. v1: cada sesión
  arranca desde cero en cuanto a "lo que recuerda" el agent; los
  archivos del proyecto son la memoria compartida.
- ❌ Agents que spawnen otros agents recursivamente sin límite. v1:
  un subagent puede invocar a otro subagent con un cap de profundidad
  (default 1) configurable por agent.
- ❌ Marketplace de agentes. v2+.
- ❌ Agentes con `model` distinto del provider configurado en el
  workspace. En v1 todos los agents usan el `provider_id`/`model_id`
  por defecto del workspace; un agent puede **overridear** el `model`
  dentro del mismo provider, pero no cambiar de provider. v1.x
  permitirá override completo de provider.
- ❌ Tools custom por agent. Las tools son globales (registro estático
  en `tools.md`); un agent solo puede allow/deny de la lista global.

## Glossary

Términos locales a este dominio (los globales están en
[`../glossary.md`](../glossary.md)):

- **AgentSpec**: struct serializable que describe un agente concreto
  (id, mode, model, prompt, tool_access, permissions, etc.).
- **Agent registry**: colección en memoria de los `AgentSpec` cargados
  (built-in + custom). Lookup por `AgentId`.
- **Active agent**: el `Primary` actualmente activo de una sesión
  (1 por sesión). Determina qué system prompt, qué tools y qué permisos
  se aplican al siguiente `session_send`.
- **Cycle**: acción de cambiar el active agent entre los primary
  disponibles, típicamente con la tecla Tab. Ver [opencode-dev/agents.mdx](../../opencode-dev/packages/web/src/content/docs/agents.mdx)
  para referencia de UX.
- **Subagent invocation**: forma en que un primary pide a un subagent
  que ejecute una subtarea. Se materializa como un `task` tool call
  que el agent loop intercepta y delega.
- **Child session**: sesión nueva, hija de la sesión padre, que se crea
  cuando un subagent es invocado. Tiene su propio `state.db` row
  (mismo `state.db` de la sesión padre, diferente `session_id`) y
  su propio journal. Al terminar, el subagent devuelve un resumen
  estructurado al parent.
- **@mention**: sintaxis en el mensaje del usuario para invocar
  manualmente un subagent (`@explore búsca los archivos de auth`).
  Solo funciona con subagents que tengan `description` no vacía.

## State

### `AgentSpec`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSpec {
    /// Identificador único del agente. ULID o string slug.
    /// Built-in IDs reservados: "build", "plan", "general".
    /// Custom IDs: cualquier slug en kebab-case.
    pub id: AgentId,

    /// Modo del agente.
    pub mode: AgentMode,

    /// Modelo a usar. Formato: "<provider_id>/<model_id>".
    /// En v1, provider_id debe coincidir con el provider activo del
    /// workspace (ver `non-goal` correspondiente). Override libre
    /// del `model_id` dentro del mismo provider.
    pub model: ModelRef,

    /// System prompt del agente.
    pub prompt: PromptSource,

    /// Acceso a tools. Default = "All" (todas las tools del registry).
    /// `Allowlist(Vec<ToolId>)` para subset explícito.
    /// `Denylist(Vec<ToolId>)` para todas menos las de la lista.
    pub tool_access: ToolAccess,

    /// Override de permisos del workspace. Si está vacío, el agent
    /// hereda la matriz del workspace (`[permissions]` del config).
    /// Si tiene reglas, se mergean con las del workspace con prioridad
    /// al override del agent (ver `permissions.md` §Algoritmo).
    pub permissions: AgentPermissionOverride,

    /// Descripción corta del agent (1 línea). Usada en:
    /// - Autocomplete de @mention.
    /// - Tooltip en el ciclo de Tab.
    /// - System prompt del primary (lista de subagents disponibles).
    pub description: Option<String>,

    /// Si `true`, el agent NO aparece en la UI (ni en cycle ni en
    /// @mention). Sigue siendo invocable por el sistema (p. ej. para
    /// compaction, title generation).
    pub hidden: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    /// Invocado directamente por el usuario. 1 activo por sesión.
    Primary,
    /// Invocado por un primary (vía tool `task`) o por @mention.
    Subagent,
    /// Invocado por el sistema. No seleccionable en UI.
    Hidden,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "kind")]
pub enum ToolAccess {
    /// Todas las tools del registry están disponibles.
    All,
    /// Solo las tools de la lista. Cualquier tool call a una tool
    /// fuera de la lista → `tool_result.v1 { isError: true, output:
    /// "tool not in agent allowlist" }`.
    Allowlist(Vec<ToolId>),
    /// Todas las tools del registry excepto las de la lista.
    Denylist(Vec<ToolId>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPermissionOverride {
    /// Reglas `allow` adicionales a las del workspace. Se evaluan
    /// DESPUÉS de las reglas del workspace.
    pub allow: Vec<String>,
    /// Reglas `deny` adicionales. Se evalúan DESPUÉS de las del
    /// workspace (sobrescriben allows del workspace si hay conflicto).
    pub deny: Vec<String>,
    /// Reglas `ask` adicionales. Si el user ya respondió "always
    /// allow" a nivel global, este override no lo revive.
    pub ask: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "source")]
pub enum PromptSource {
    /// Prompt embebido en el binario (built-in agents).
    Embedded { content: String },
    /// Prompt cargado de un archivo markdown (custom agents).
    /// Soporta frontmatter opcional con `description` y `mode`.
    File { path: PathBuf },
    /// Prompt descargado de una URL (futuro, v2+). v1 no implementado.
    Url { url: String },
}
```

### `AgentRegistry`

In-memory, vive en `AppState` (cargado al arranque).

```rust
pub struct AgentRegistry {
    agents: Vec<AgentSpec>,
}

impl AgentRegistry {
    pub fn load(config: &Config) -> Result<Self, AppError>;
    pub fn list(&self) -> &[AgentSpec];
    pub fn get(&self, id: &AgentId) -> Option<&AgentSpec>;
    pub fn list_by_mode(&self, mode: AgentMode) -> Vec<&AgentSpec>;
    pub fn primary_ids(&self) -> Vec<AgentId>;  // para el cycle con Tab
    pub fn subagents(&self) -> Vec<&AgentSpec>;  // para @mention
}
```

### Active agent (per session)

```rust
pub struct SessionActiveAgent {
    pub session_id: SessionId,
    pub agent_id: AgentId,  // default = primer primary del registry
}
```

Persiste en `state.db` (columna en tabla `sessions` o key-value).
Cuando arranca una sesión, se carga el active agent del último estado
(persiste entre reinicios de la app).

## Built-in agents (v1)

### `build` (Primary, default)

- **Mode**: `primary`
- **Model**: `provider_id/model_id` del workspace (sin override)
- **Prompt**: embebido; identidad "eres un agente agentic de propósito
  general que opera sobre los archivos del workspace del usuario…"
- **Tool access**: `All`
- **Permissions**: hereda del workspace, sin override
- **Description**: `null` (es el default; no se muestra en @mention)
- **Hidden**: `false`
- **Comportamiento**: el agent loop, cuando recibe un `task` tool
  call de `build`, puede delegar a un subagent si el nombre del
  subagent está en su `tool_access` (en v1, siempre; v1.x quizá
  requiera permiso explícito).

### `plan` (Primary, opcional)

- **Mode**: `primary`
- **Model**: mismo provider, pero `temperature: 0.1` para respuestas
  más deterministas
- **Prompt**: embebido; identidad "eres un agente de análisis y
  planificación. NO tienes permiso de escribir archivos ni ejecutar
  comandos. Tu trabajo es leer, explorar y proponer un plan
  estructurado que el usuario revisará antes de aplicar…"
- **Tool access**: `Allowlist([read_file, search, list_dir])`
- **Permissions** override: `deny: ["write_file", "edit_file",
  "shell", "python_run", "apply_patch"]` (defensa en profundidad; si
  el prompt se ignora, la matriz de permisos lo bloquea)
- **Description**: `"Read-only analysis and planning. No writes."`
- **Hidden**: `false`
- **Comportamiento**: cycle con Tab cambia entre `build` y `plan`.
  Si el usuario envía un mensaje a `plan` y el LLM intenta emitir un
  tool call de write, el agent loop retorna `tool_result.v1
  { isError: true, output: "plan agent is read-only" }`.

### `general` (Subagent)

- **Mode**: `subagent`
- **Model**: mismo provider/model que el primary que lo invoca
- **Prompt**: embebido; identidad "eres un subagent de propósito
  general. Ejecutas tareas delegadas por un primary agent. Devuelves
  un resumen estructurado al terminar. NO invoques otros subagents
  recursivamente…"
- **Tool access**: `All` (puede hacer lo que el primary le pida)
- **Permissions**: hereda del workspace
- **Description**: `"General-purpose subagent for multi-step delegated
  tasks. Full tool access."`
- **Hidden**: `false`
- **Comportamiento**: se invoca vía `task` tool call del primary
  (auto-delegación) o vía `@general` en un mensaje del usuario
  (manual). Crea una child session. Al terminar, devuelve un
  `TaskResult { summary, files_changed, key_findings }` al parent.

### Built-in Hidden agents (v1, **no expuestos en UI**)

- **`compaction`** — invocado por el agent loop cuando el contexto
  se acerca al límite del modelo. Resume mensajes antiguos. v1 NO
  está implementado; en v1 truncamos por tokens (ver
  `agent-loop.md` §Edge 8). Se deja la spec para que la
  implementación en v1.x no requiera refactor.
- **`title`** — genera el título corto de la sesión a partir del
  primer mensaje del usuario. v1: el título se deriva por
  truncado (primeros 60 chars). v1.x: este agent.
- **`summary`** — genera un resumen de la sesión al cerrarla (para
  historial en sidebar). v1: no hay resumen. v1.x: este agent.

> **Decisión de v1**: NO implementamos compaction/title/summary
> agents. Se reservan los IDs en la spec para que aparezcan como
> `mode: hidden` en el registry y estén listos cuando se
> implementen. Sus IDs no son invocables por el usuario en v1.

## Custom agents (v1.x, no en v1)

> Documentado pero no implementado en v1. La spec lo deja
> planteado para que añadirlo en v1.x sea un PR pequeño, no un
> refactor.

A partir de v1.x, el usuario puede crear agentes custom poniendo
archivos markdown en:

- **Global**: `~/.agentyx/agents/*.md`
- **Per-workspace**: `<workspace_root>/.agentyx/agents/*.md`

Formato (convención basada en opencode):

```markdown
---
description: Reviews code for security and performance issues
mode: subagent
model: groq/llama-3.3-70b-versatile
---

You are a code reviewer. Your job is to find security issues,
performance bottlenecks, and maintainability problems. Be concise.
Use search and read_file; never modify files.
```

El `AgentRegistry::load` en v1.x escanea estos directorios, parsea
el frontmatter, y los añade al registry. Los custom con el mismo
`id` que un built-in **override** al built-in (warning logueado).

## Operations

### `AgentRegistry::load(config) -> Result<Self, AppError>`

Carga los 3 built-in (en código Rust) y, en v1.x, escanea los
directorios de custom agents. Idempotente (llamar 2 veces da el
mismo resultado).

**Errores**:
- `invalid_input` — un custom agent tiene frontmatter malformado
  (con detalle del campo que falló).
- `internal` — I/O error al leer un custom agent file (con path).

### `AgentRegistry::list() -> &[AgentSpec]`

Devuelve todos los agents, orden: built-in primero (en orden de
declaración), luego custom alfabético.

### `AgentRegistry::get(id) -> Option<&AgentSpec>`

Lookup por id. `None` si no existe.

### `AgentRegistry::primary_ids() -> Vec<AgentId>`

Devuelve los IDs de los agents `mode: primary` y `hidden: false`,
en el orden del registry. Se usa para el cycle con Tab.

### `AgentRegistry::subagents() -> Vec<&AgentSpec>`

Devuelve los agents `mode: subagent` y `hidden: false`. Se usa
para el autocomplete de `@mention` y para el system prompt del
primary (lista de subagents disponibles).

### `Session::set_active_agent(session_id, agent_id) -> Result<(), AppError>`

Cambia el active agent de la sesión. Persiste en `state.db`.

**Errores**:
- `not_found` — sesión o agent no existe.
- `invalid_input` — el agent no es `mode: primary`.

### `AgentLoop::invoke_subagent(parent_run, subagent_id, prompt) -> Result<TaskResult, AppError>`

Invocado por el agent loop cuando un primary emite un `task` tool
call. Crea una child session, arranca un run con el `AgentSpec` del
subagent, y espera a que termine. Devuelve el `TaskResult` al parent.

**Errores**:
- `not_found` — subagent no existe.
- `forbidden` — el primary que invoca no tiene el subagent en su
  `tool_access` (en v1 no aplica; v1.x quizá).
- `internal` — la child session falla al arrancar (DB, etc.).
- `timeout` — el subagent supera `max_steps` o el timeout absoluto
  del run (default 10 min, configurable).

### `AgentLoop::expand_at_mentions(message) -> Vec<(AgentId, String)>`

Pre-procesa el mensaje del usuario. Detecta `@<agent-id>` y devuelve
la lista de (agent, prompt_segment) a invocar antes del resto del
mensaje. Si el agent referenciado no existe, error inline (no
silencioso).

## Contracts

### Tauri commands

| Command | Notas |
|---|---|
| `agents_list() -> AgentInfo[]` | Solo los `hidden: false`. |
| `agents_get(id) -> AgentInfo` | |
| `agents_set_active(session_id, agent_id) -> ()` | |
| `agents_invoke_subagent(session_id, subagent_id, prompt) -> TaskResult` | Para la UI cuando el usuario usa `@<id>` manualmente. |

### HTTP endpoints

```
GET    /api/v1/agents                       → AgentInfo[]
GET    /api/v1/agents/:id                   → AgentInfo
POST   /api/v1/sessions/:id/active-agent    (body: { agentId }) → {}
POST   /api/v1/sessions/:id/invoke-subagent (body: { subagentId, prompt }) → TaskResult
```

### Eventos

| Evento | Cuándo | Payload |
|---|---|---|
| `agent.changed.v1` | Cuando el usuario cambia el active agent de una sesión | `{ sessionId, fromAgentId, toAgentId }` |
| `subagent.started.v1` | Cuando un subagent arranca una child session | `{ parentRunId, childSessionId, subagentId }` |
| `subagent.finished.v1` | Cuando el subagent termina | `{ parentRunId, childSessionId, result: TaskResult }` |
| `subagent.aborted.v1` | Cuando el subagent es abortado (por el parent o el user) | `{ parentRunId, childSessionId, reason }` |

## Edge cases

1. **Sesión sin active agent (estado inconsistente)**: al cargar,
   se asigna el primer `primary` del registry. Loguea `tracing::warn!`.
2. **Cambio de active agent mid-run**: no permitido mientras hay un
   run activo (`conflict`); el user debe esperar a `finish_reason:
   stop` o abortar primero.
3. **Subagent que se invoca a sí mismo recursivamente**: el agent loop
   detecta (vía stack de parent_run_ids) y aborta con `depth_exceeded`.
   Profundidad máxima default = 1 (un subagent no puede invocar a
   otro subagent en v1). Configurable por workspace en v1.x.
4. **`@<id>` en mensaje donde el agent no existe**: el
   `expand_at_mentions` retorna `invalid_input` con el id mal formado.
   El resto del mensaje **no** se procesa (atomicidad).
5. **`@<id>` apuntando a un `primary` (no subagent)**: rechazado
   con `invalid_input`. `@` solo funciona con subagents.
6. **Subagent que devuelve un `TaskResult` con `files_changed` pero
   la sesión padre ya terminó**: el resultado se descarta con
   `tracing::warn!` y se loguea en el journal del parent como
   "orphan subagent result".
7. **Custom agent en v1.x con `mode: primary`**: el registry lo
   acepta. El cycle con Tab lo incluye. Si hay colisión de ID con
   un built-in, gana el custom (con warning al user en la UI:
   "overrode built-in agent 'build'").
8. **Cycle con Tab cuando hay 1 solo primary visible**: no-op. No
   muestra nada en la UI.
9. **`task` tool call con `subagent_id` desconocido**: el agent
   loop emite `tool_result.v1 { isError: true, output: "unknown
   subagent: <id>" }` y el parent puede decidir qué hacer.
10. **Subagent que tarda mucho**: timeout absoluto del run del
    subagent = 10 min (configurable). Si se alcanza, el subagent
    aborta con `finish_reason: length` o `error` (depende del
    provider), y el parent recibe un `TaskResult { status:
    "timeout", summary: null }`.

## Acceptance criteria

Cada AC → test con nombre derivado `ac<n>_<short>`.

- [ ] AC1: `load` con un `Config` vacío retorna un registry con
  exactamente 3 built-in (`build`, `plan`, `general`) y 3 hidden
  (`compaction`, `title`, `summary`). **Test**:
  `ac1_load_creates_three_builtins_and_three_hidden`.
- [ ] AC2: `list()` con un registry cargado retorna 6 agents (los
  3 built-in visibles primero, luego los 3 hidden). **Test**:
  `ac2_list_orders_builtins_first`.
- [ ] AC3: `get("build")` retorna el `AgentSpec` con
  `mode: primary, tool_access: All, hidden: false`. **Test**:
  `ac3_get_build_returns_correct_spec`.
- [ ] AC4: `get("plan")` retorna el `AgentSpec` con
  `tool_access: Allowlist([read_file, search, list_dir])` y
  `permissions.deny` conteniendo `write_file`, `edit_file`, `shell`,
  `python_run`, `apply_patch`. **Test**:
  `ac4_get_plan_has_deny_on_writes`.
- [ ] AC5: `get("nonexistent")` retorna `None`. **Test**:
  `ac5_get_nonexistent_returns_none`.
- [ ] AC6: `primary_ids()` retorna `["build", "plan"]` en ese
  orden. **Test**: `ac6_primary_ids_returns_build_then_plan`.
- [ ] AC7: `subagents()` retorna solo `general` (los hidden
  excluidos). **Test**: `ac7_subagents_excludes_hidden`.
- [ ] AC8: `set_active_agent(session, "plan")` persiste y un
  `load` posterior lo lee correctamente. **Test**:
  `ac8_set_active_agent_persists`.
- [ ] AC9: `set_active_agent(session, "general")` retorna
  `invalid_input` (general no es primary). **Test**:
  `ac9_set_active_agent_rejects_subagent`.
- [ ] AC10: `set_active_agent` durante un run activo retorna
  `conflict`. **Test**: `ac10_set_active_agent_blocks_during_run`.
- [ ] AC11: el primary `plan` recibe `tool_access` filtrado: si el
  LLM intenta `write_file`, el agent loop emite
  `tool_result.v1 { isError: true, output: "plan agent is
  read-only" }` y el archivo no se escribe. **Test**:
  `ac11_plan_agent_blocks_writes_via_tool_access`.
- [ ] AC12: el primary `plan` recibe `permissions.deny` que
  refuerza el bloqueo aunque el prompt del agent se ignore
  (defensa en profundidad). **Test**:
  `ac12_plan_agent_deny_rules_apply`.
- [ ] AC13: `invoke_subagent` con `subagent_id: "general"` desde
  un parent run crea una child session, ejecuta el subagent, y
  devuelve un `TaskResult` al parent. **Test**:
  `ac13_invoke_general_subagent_creates_child_session`.
- [ ] AC14: `invoke_subagent` con `subagent_id` desconocido
  retorna `not_found` y no crea child session. **Test**:
  `ac14_invoke_unknown_subagent_returns_not_found`.
- [ ] AC15: `invoke_subagent` que excede 10 min aborta con
  `TaskResult { status: "timeout" }` y la child session queda en
  estado `aborted`. **Test**:
  `ac15_invoke_subagent_timeout_aborts`.
- [ ] AC16: dos `invoke_subagent` concurrentes del mismo primary
  crean dos child sessions independientes, cada una con su
  propio journal. **Test**:
  `ac16_concurrent_subagent_invocations_isolated`.
- [ ] AC17: `expand_at_mentions("@general búsca los archivos de
  auth")` retorna `[(general, "búsca los archivos de auth")]` y
  el resto del mensaje se preserva. **Test**:
  `ac17_at_mention_extracts_subagent_and_prompt`.
- [ ] AC18: `expand_at_mentions("@nonexistent foo")` retorna
  `invalid_input` y el mensaje NO se procesa. **Test**:
  `ac18_at_mention_unknown_returns_error`.
- [ ] AC19: `expand_at_mentions("@build foo")` retorna
  `invalid_input` (build es primary, no subagent). **Test**:
  `ac19_at_mention_primary_rejected`.
- [ ] AC20: agent_loop de un primary detecta un `task` tool call
  en el stream y lo delega a `invoke_subagent`, retornando el
  `TaskResult` como `tool_result.v1` al LLM del primary. **Test**:
  `ac20_primary_task_tool_call_delegates_to_subagent`.

## Discovered bugs (post-approval)

| ID | Date | Category | Resolved in | Notes |
|---|---|---|---|---|
| _ninguno aún_ | | | | |

## Open questions

- **Q1**: ¿Subagent comparte sesión con el parent o tiene child
  session? → **Propuesta v1**: **child session**, siempre. Razón:
  journal y replays separados, sin acoplamiento de contexto. El
  parent recibe un resumen (`TaskResult`), no el transcript crudo.
- **Q2**: ¿Custom agents en v1 o v1.x? → **Propuesta**: v1.x (no
  MVP). El registry ya soporta el modelo, falta el file scanner y
  la UI de creación.
- **Q3**: ¿El primary puede overridear el `model` del workspace
  (p. ej. usar `gpt-4o` solo para `plan`)? → **Propuesta v1**: no.
  v1.x sí. En v1, todos los agents usan el `model` del workspace.
- **Q4**: ¿Subagent puede overridear permisos del workspace? → Sí
  (definido en `AgentPermissionOverride`). Esencial para `plan` y
  para custom agents con capacidades reducidas.
- **Q5**: ¿`@<id>` con autocompletion en la UI? → **Propuesta v1**:
  sí. La lista sale de `AgentRegistry::subagents()`. Se muestra
  con un popover al teclear `@`.
- **Q6**: ¿El cycle con Tab puede ir a `Hidden` agents? → No. Solo
  primary visibles.
- **Q7**: ¿Soporte de agentes con `model` de un provider distinto
  al del workspace? → **Propuesta**: v1.x, no v1. Necesita resolver
  API keys por agent, lo cual es un cambio de modelo en providers.
- **Q8**: ¿Cómo se manejan agents que dependan entre sí (uno
  importa los tools de otro)? → **Propuesta v1.x**: no hay
  dependencias. Cada agent es autocontenido.

## References

- [`../glossary.md`](../glossary.md) — `Agent`, `AgentMode`, `Workspace`.
- [`agent-loop.md`](./agent-loop.md) — el consumer de `AgentSpec`.
- [`tools.md`](./tools.md) — fuente de `ToolId` para `ToolAccess`.
- [`permissions.md`](./permissions.md) — `AgentPermissionOverride`
  se mergea con la matriz del workspace.
- [`session.md`](./session.md) — `session_id`, `state.db` con la
  columna de active agent.
- [`../ipc.md`](../ipc.md) — shape de Tauri commands y eventos.
- [opencode-dev/agents.mdx](../../opencode-dev/packages/web/src/content/docs/agents.mdx) —
  referencia de UX (cycle con Tab, subagents, child sessions).
