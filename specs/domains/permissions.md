# Permissions

**Status**: review
**Owner**: @miglesias
**Last update**: 2026-06-05
**Affects**: — (las permissions son consultadas por `agent-loop.md`
antes de cada tool call).
**Required by**: `agent-loop.md`, `tools.md` (cada tool declara qué
permisos requiere), `features/F01-chat-streaming` (decisión visible
en la UI), `agents.md` (`AgentPermissionOverride` del agent se
mergea con la matriz del workspace).

> Matriz de permisos por workspace + decisión por tool call. Define
> quién puede hacer qué, con `allow` / `ask` / `deny` y la regla
> "safe default". El sandbox de paths es **`root_path ∪ extra_paths`**
> del workspace (no solo `root`). Toda decisión se loguea en el
> `journal` (audit).

## Goal

Decidir de forma **determinista, auditable y con default seguro** si
el agente puede ejecutar una tool concreta, dados:
- El tool name.
- Los argumentos (incluido el path si aplica).
- El workspace (su `config.toml`).
- Las settings globales del usuario (override por tool).

## Non-goals

- ❌ Implementar las tools. Ver [`tools.md`](./tools.md).
- ❌ El agent loop en sí. Ver [`agent-loop.md`](./agent-loop.md).
- ❌ RBAC multi-usuario, OAuth, etc. v1 es single-user, single-machine.
- ❌ Sandboxing nativo del SO (Seatbelt, Landlock, AppContainer). v1
  es aislamiento lógico; v2 será enforcement real.
- ❌ Revertir una decisión "always allow" pasada de forma retroactiva
  sobre runs ya en el journal. v2 si hay demanda.

## Glossary

Términos locales:

- **Permission matrix**: configuración de un workspace
  (`[permissions]` en `config.toml`) que lista
  `allow`, `deny`, `ask` y `deny_paths`.
- **Decision**: el resultado de `Permissions::check`. Uno de:
  `Allow { persist: bool }`, `Ask { reason: String }`,
  `Deny { reason: String }`. `persist: true` significa "remember
  this for the rest of the session" (no se vuelve a preguntar).
- **Approval mode** (global, en `~/.agentyx/config.toml`):
  - `ask` (default) — prompt en cada acción marcada como
    destructiva.
  - `auto` — ejecuta con guardrails (las `deny` y `deny_paths`
    siguen activas, `ask` se trata como `allow`).
  - `never` — read-only, ninguna tool de escritura puede ejecutarse.
- **Dangerous tool**: tool que modifica estado (escribe archivos,
  ejecuta comandos, etc.). Las tools de solo lectura (`read_file`,
  `search`, `list_dir`) no son dangerous.
- **Path guard**: regex/lista de paths en `deny_paths` o
  `allow_paths`. Evalúa **después** de canonicalizar el path.

## State

### Por workspace (en `config.toml`)

```toml
[permissions]
allow  = ["read_file", "search", "list_dir"]
deny   = []                          # explícito; rara vez usado
ask    = ["write_file", "edit_file", "shell", "python_run", "apply_patch"]
deny_paths = [
  "**/.git/**",
  "**/node_modules/**",
  "**/.venv/**",
  "**/target/**",
  "**/secrets/**",
]
allow_paths = []                     # si no vacío, **solo** estos paths son escribibles

# Reglas finas aplicadas **dentro** de los extra_paths. Los
# extra_paths ya son accesibles por estar declarados; esto es para
# "dentro de este extra, no toques X". Aplica solo a paths que
# resuelven dentro de algún extra_path.
[permissions.extra_paths]
deny = ["**/.git/**", "**/secrets/**"]
```

### Global (en `~/.agentyx/config.toml`)

```toml
[permissions]
approval_mode = "ask"                 # ask | auto | never

# Overrides por tool, aplican a TODOS los workspaces
[permissions.always_allow]
"shell" = false                       # true = nunca pregunta para shell
"python_run" = true

[permissions.always_deny]
"shell" = false                       # true = nunca permite shell
```

## Operations

### `Permissions::check(workspace_id, tool, args) -> Decision`

Decide si la tool se ejecuta.

**Algoritmo** (orden de evaluación, primer match gana):

1. **Path traversal** en cualquier arg de tipo `path` → `Deny
   { reason: "path_traversal" }`. Chequeado **antes** de cualquier
   regla, sin excepción.
2. **Path fuera de `root_path ∪ extra_paths`** (cualquier path
   que no resuelva dentro del root ni de ningún extra_path
   declarado) → `Deny { reason: "path_outside_workspace" }`.
   Chequeado **antes** de `allow_paths` porque los extra paths
   ya están "allow" por estar declarados.
3. Tool en `always_deny` global → `Deny { reason: "always_deny" }`.
4. `approval_mode == "never"` y tool es dangerous → `Deny
   { reason: "approval_mode_never" }`.
5. Path del arg en `deny_paths` del workspace → `Deny
   { reason: "denied_path" }`.
6. Path del arg en `extra_paths.deny` (solo si el path está
   dentro de un extra_path declarado) → `Deny
   { reason: "denied_path_in_extra" }`.
7. `allow_paths` no vacío y path NO está en `allow_paths` → `Deny
   { reason: "path_not_in_allow_list" }`.
8. Tool en `always_allow` global → `Allow { persist: true }`.
9. Tool en `allow` del workspace → `Allow { persist: true }`.
10. Tool en `deny` del workspace → `Deny { reason: "denied" }`.
11. Tool en `ask` del workspace →
    - `approval_mode == "auto"` → `Allow { persist: false }`.
    - `approval_mode == "ask"` → `Ask { reason: "user_approval" }`.
12. Tool desconocida (no en `allow`/`deny`/`ask`) → `Ask
    { reason: "unknown_tool" }` (safe default; ver §Edge case 1).

**Merge con `AgentPermissionOverride`** (vía `agents.md`):

Si el agent activo del run tiene un `AgentPermissionOverride`, sus
reglas se evalúan **después** de las del workspace con la prioridad
siguiente:

- `agent.deny[i]` → equivalente a añadir a `deny` con peso máximo.
- `agent.allow[i]` → equivalente a añadir a `allow` (peso alto, pero
  por debajo de `deny`).
- `agent.ask[i]` → equivalente a añadir a `ask`.

Si el agent es `mode: primary` con `permissions.deny: ["write_file",
…]`, ese deny se evalúa **antes** que el `allow` del workspace
(porque deny gana siempre). Ver
[`agents.md`](../agents.md) §Algoritmo (regla "último match
prioritario").

**Output**:
```rust
pub enum Decision {
    Allow { persist: bool },
    Ask { reason: String },
    Deny { reason: String },
}
```

**Errores**: `not_found` (workspace no existe). No escribe en disco;
sí registra en `journal` (vía `Journal::append` que es
responsabilidad del caller, ver §Effect).

**Permisos requeridos**: N/A (es el decisor).

**Efectos colaterales**: el **caller** (agent loop) debe loguear la
decisión en `journal` con `permission_decision`:
- `allow` para `Allow`.
- `ask` para `Ask`.
- `deny` para `Deny`.

### `Permissions::resolve(request_id, user_decision) -> Result<(), AppError>`

Resuelve una `Ask` pendiente. Llamado por el command Tauri cuando el
usuario aprueba o deniega en la UI.

**Input**:
```rust
pub enum UserDecision {
    Allow { persist: bool },           // persist=true = "always for this session"
    Deny { persist: bool },
}
```

Si `persist: true`, se actualiza el global
`[permissions.always_allow]` o `[permissions.always_deny]`.

**Errores**:
- `not_found` (request_id desconocido o ya expirado).
- `internal` (no se pudo persistir el global).

### `Permissions::get_matrix(workspace_id) -> PermissionMatrix`

Lee `[permissions]` del `config.toml` del workspace y los overrides
globales; devuelve la matriz efectiva.

### `Permissions::set_matrix(workspace_id, matrix) -> ()`

Escribe `[permissions]` en `config.toml`. **No** toca los overrides
globales.

### `Permissions::list_effective_paths(workspace_id) -> Vec<PathSpec>`

Devuelve el conjunto efectivo de paths donde el agente puede
operar, fusionando `root_path` con `extra_paths` del workspace.
Usado por la UI (sidebar "Extras") y por el system prompt del
agent.

```rust
pub struct PathSpec {
    pub path: PathBuf,
    pub role: PathRole,            // Root | Extra
    pub label: Option<String>,
}

pub enum PathRole { Root, Extra }
```

**Errores**:
- `not_found` — workspace no existe.

## Contracts

### Tauri commands

| Command | Notas |
|---|---|
| `permissions_check(workspace_id, tool, args, agent_id?) -> Decision` | Usado por tests y por el UI para "dry-run". `agent_id` opcional para evaluar el override del agent. |
| `permissions_resolve(request_id, user_decision) -> ()` | |
| `permissions_get_matrix(workspace_id) -> PermissionMatrix` | |
| `permissions_set_matrix(workspace_id, matrix) -> ()` | |
| `permissions_list_effective_paths(workspace_id) -> PathSpec[]` | **Nuevo** (ver `workspace.md` `Workspace::list_extra_paths`). |

### HTTP endpoints

`POST /api/v1/permissions/check` → `Decision`
`POST /api/v1/permissions/resolve` → `{}`
`GET  /api/v1/workspaces/:id/permissions` → `PermissionMatrix`
`PUT  /api/v1/workspaces/:id/permissions` → `{}`
`GET  /api/v1/workspaces/:id/effective-paths` → `PathSpec[]`

### Eventos

| Evento | Cuándo | Payload |
|---|---|---|
| `permission.request.v1` | Cuando `check` retorna `Ask` | `{ requestId, workspaceId, tool, args, danger, reason }` |
| `permission.resolved.v1` | Cuando el usuario responde en la UI | `{ requestId, decision: "allow"\|"deny", persist: bool }` |

## Edge cases

1. **Tool desconocida** (no está en `allow`/`deny`/`ask`/`always_*`):
   safe default = `Ask { reason: "unknown_tool" }`. **No** allow
   silencioso. El usuario debe aceptarla explícitamente la primera
   vez; a partir de ahí el UI puede sugerir añadirla a `allow`.
2. **Path traversal disfrazado** (`..`, `~/`, symlinks): la
   canonicalización previa (responsabilidad de la tool, ver
   `tools.md`) ya debe haber resuelto el path; este dominio lo
   rechaza si ve `..` literal en cualquier arg.
3. **Tool safe (`read_file`) con path en `deny_paths`**: la tool
   safe **igual** respeta `deny_paths` (ver §AC4). Es coherente: si
   el usuario dijo "no toques `.git/`", no se lee ni se escribe.
4. **Permutaciones contradictorias** (tool en `allow` y `deny`):
   `deny` gana. Documentado en §Algoritmo paso 8.
5. **Permission request expira** (usuario no responde en 5 min):
   `request_id` se invalida; `resolve` devuelve `not_found`. El
   agent loop recibe esto como `Deny { reason: "timeout" }` y emite
   `tool_result.v1 { isError: true }` al modelo.
6. **Múltiples permission requests concurrentes**: cada una tiene
   un `request_id` ULID. La UI muestra una cola (no un modal único).
7. **El usuario marca "always allow" pero cambia de workspace**:
   el override se guarda en `[permissions.always_allow]` global;
   aplica a **todos** los workspaces. Es lo más simple en v1. v2
   podría ser per-workspace.
8. **Workspace config sin sección `[permissions]`**: la matriz
   efectiva es la default: `allow = ["read_file", "search",
   "list_dir"]`, `ask = [todas las demás]`, `deny_paths` según
   ignore patterns del workspace.
9. **`approval_mode = "auto"` + tool denegada por `deny_paths`**:
   `auto` **no** override `deny_paths`. Es solo para `ask`. Si
   quieres bypass de `deny_paths`, hay que editar `config.toml` a
   mano.
10. **El `config.toml` se edita a mano mientras la app corre**:
    la app cachea la matriz en memoria. La cache se invalida en
    `permissions_resolve` (que escribe) o al hacer `set_matrix`.
    Reads desde otros procesos no se notan; v2 introduce un file
    watcher (ver `workspace.md` non-goals).
11. **Path en extra_paths que ya no existe en el filesystem**
    (usuario lo borró fuera de Agentyx): la canonicalización
    falla, el path no entra en el set efectivo. `check` lo trata
    como `path_outside_workspace`. La UI puede mostrar un warning
    "este extra path ya no existe" en el sidebar.
12. **Path en `allow_paths` del workspace que NO está en
    `root ∪ extras`**: se respeta la regla (`allow_paths` no se
    ensancha implícitamente). El path será denegado por
    `path_outside_workspace` (paso 2) **antes** de chequear
    `allow_paths`. Workaround: añadir el path como extra path.
13. **Plan agent con `permissions.deny: ["write_file", …]`** y
    tool safe (`read_file`) en el mismo step: la `tool_access`
    `Allowlist([read_file, search, list_dir])` del agent hace
    que el loop **no exponga** `write_file` al provider. Si por
    algún bug se filtrara, el `deny` del agent lo bloquea
    (defensa en profundidad).
14. **Mismo path en `deny_paths` y `allow_paths`**: `deny` gana
    (consistente con tools). Ver §Algoritmo paso 5.

## Acceptance criteria

Cada AC → test con nombre derivado `ac<n>_<short>`.

- [ ] AC1: `check` con tool en `allow` retorna `Allow { persist: true }`.
  **Test**: `ac1_allow_tool_returns_allow`.
- [ ] AC2: `check` con tool en `deny` retorna `Deny`. **Test**:
  `ac2_deny_tool_returns_deny`.
- [ ] AC3: `check` con tool en `ask` y `approval_mode = "ask"` retorna
  `Ask`. **Test**: `ac3_ask_tool_returns_ask`.
- [ ] AC4: `check` con path en `deny_paths` retorna `Deny
  { reason: "denied_path" }` aunque la tool esté en `allow`. **Test**:
  `ac4_denied_path_overrides_allow`.
- [ ] AC5: `check` con `..` literal en un arg retorna `Deny
  { reason: "path_traversal" }`. **Test**:
  `ac5_path_traversal_always_denied`.
- [ ] AC6: `check` con tool en `always_allow` global retorna `Allow`
  sin importar el workspace. **Test**:
  `ac6_global_always_allow_overrides_workspace`.
- [ ] AC7: `check` con tool en `always_deny` global retorna `Deny`
  sin importar el workspace. **Test**:
  `ac7_global_always_deny_overrides_workspace`.
- [ ] AC8: `check` con tool desconocida y `approval_mode = "ask"`
  retorna `Ask { reason: "unknown_tool" }`. **Test**:
  `ac8_unknown_tool_safe_default_ask`.
- [ ] AC9: `check` con `approval_mode = "never"` y tool dangerous
  retorna `Deny { reason: "approval_mode_never" }`. **Test**:
  `ac9_never_mode_blocks_dangerous`.
- [ ] AC10: `check` con `approval_mode = "auto"` y tool en `ask`
  retorna `Allow { persist: false }`. **Test**:
  `ac10_auto_mode_skips_prompt`.
- [ ] AC11: `resolve(request_id, Allow { persist: true })` actualiza
  el global `[permissions.always_allow]` y `check` posteriores
  reflejan el cambio. **Test**:
  `ac11_persist_allow_updates_global`.
- [ ] AC12: `resolve` con `request_id` expirado (> 5 min) retorna
  `not_found`. **Test**: `ac12_resolve_expired_returns_not_found`.
- [ ] AC13: dos `check` concurrentes con misma tool/args/workspace
  retornan la misma `Decision` (idempotencia). **Test**:
  `ac13_concurrent_check_idempotent`.
- [ ] AC14: el caller (test) puede verificar que la decisión quedó
  en `journal` con `permission_decision` correcto. **Test**:
  `ac14_decision_logged_in_journal`.
- [ ] AC15: `check` con un path **dentro de un extra_path declarado**
  y tool `read_file` retorna `Allow` aunque el path no esté en el
  `root_path`. **Test**: `ac15_path_in_extra_path_is_allowed`.
- [ ] AC16: `check` con un path **fuera de `root ∪ extras`** retorna
  `Deny { reason: "path_outside_workspace" }` aunque la tool esté
  en `allow` y `allow_paths` esté vacío. **Test**:
  `ac16_path_outside_root_and_extras_denied`.
- [ ] AC17: `check` con un path dentro de un extra_path y match
  contra `permissions.extra_paths.deny` retorna `Deny
  { reason: "denied_path_in_extra" }`. **Test**:
  `ac17_extra_paths_deny_overrides_allow`.
- [ ] AC18: `check` con `agent_id = "plan"` y tool `write_file`
  retorna `Deny { reason: "denied" }` aunque el workspace no tenga
  `deny` para esa tool (viene del `AgentPermissionOverride` del
  agent). **Test**: `ac18_agent_deny_overrides_workspace_allow`.
- [ ] AC19: `list_effective_paths` devuelve el `root` como `Root`
  y los extras como `Extra` en orden de `added_at`. **Test**:
  `ac19_list_effective_paths_orders_by_role_and_added_at`.

## Discovered bugs (post-approval)

| ID | Date | Category | Resolved in | Notes |
|---|---|---|---|---|
| _ninguno aún_ | | | | |

## Open questions

- **Q1**: ¿Los overrides `always_allow` / `always_deny` globales
  tienen UI para gestionarlos? → **Propuesta v1**: settings global
  con tabla simple, editada desde Settings en la UI.
- **Q2**: ¿Una decisión `Ask` puede expirar antes de 5 min? → **Propuesta
  v1**: no, 5 min fijo en v1. Configurable por workspace en v2.
- **Q3**: ¿Soportar regex avanzado en `deny_paths` (más allá de
  `**` glob)? → **Propuesta v1**: no. Glob simple, y paths literales.
  Regex en v2 si alguien lo pide.
- **Q4**: ¿`approval_mode = "auto"` loguea diferente en el journal? →
  **Propuesta v1**: sí, el `permission_decision` es `allow_auto` en
  vez de `allow` para distinguir en el journal. La diferencia es solo
  analítica.

## References

- [`../architecture.md`](../architecture.md) — flujo de tool call con
  permission check.
- [`agent-loop.md`](./agent-loop.md) — el caller.
- [`tools.md`](./tools.md) — qué tool es dangerous.
- [`workspace.md`](./workspace.md) — `[permissions]` y `extra_paths` del config.
- [`../ipc.md`](../ipc.md) — eventos `permission.*.v1`.
- [`../agents.md`](../agents.md) — `AgentPermissionOverride` se mergea
  con la matriz del workspace.
