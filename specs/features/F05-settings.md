# F05 — Settings

**Status**: draft
**Owner**: @miglesias
**Last update**: 2026-06-05
**Affects**: [`providers`](../domains/providers.md), [`permissions`](../domains/permissions.md),
[`config`](../domains/config.md), [`workspace`](../domains/workspace.md).
**Depends on**: [`F02`](./F02-multi-workspace.md) (workspaces existen
y se listan), [`config`](../domains/config.md) (carga/valida TOML),
[`providers`](../domains/providers.md) (cliente HTTP para test
connection).

## User story

Como **usuario**, quiero **configurar providers, modelos, secretos y
el modo de aprobación desde una pantalla de Settings**, para que
Agentyx sepa a qué provider enviar mis chats, qué modelo usar por
defecto, qué tools requieren aprobación, y para no tener que editar
`config.toml` a mano.

## Scope

### In-scope (v0.1)

- Pantalla `/settings` con 4 tabs: **Providers**, **Models**,
  **Approval**, **Workspace**.
- **Providers**:
  - Listar providers configurados (enabled/disabled, base_url).
  - Añadir un provider (Groq, Minimax; Ollama viene preconfigurado).
  - Editar `base_url` de un provider existente.
  - Activar/desactivar un provider.
  - Eliminar un provider (solo si no es el `default_provider`).
  - **Test connection** por provider: muestra `ok` + latencia +
    modelos detectados; muestra error actionable si falla.
  - Configurar `api_key` (input enmascarado → `secrets_set` →
    keychain del SO; nunca persiste en `config.toml`).
- **Models**:
  - Cambiar `default_provider` y `default_model`.
  - Listar modelos detectados por provider (caché del último
    test connection).
  - Override por workspace (`default_model` por workspace).
- **Approval**:
  - Cambiar `approval_mode` global (`ask` / `allow` / `deny`).
  - Override por workspace (mismo enum).
  - Lista de las tools con su **prompt default** (ask/allow/deny),
    editable. Esto es la **matriz de permisos** que `permissions.md`
    ya define; aquí es solo la UI para editarla.
- **Workspace**:
  - Editar `ignore_patterns`.
  - Editar `journal_max_rows`.
  - Gestionar `extra_paths` (añadir/quitar directorios con
    confirmación; ver [`F02`](./F02-multi-workspace.md) §Extras).
- **Misc**:
  - Theme (auto/light/dark).
  - Update channel (stable/beta/dev).
  - Telemetry on/off.

### Out-of-scope (v0.1)

- ❌ Crear/editar **custom agents** desde la UI (v1.x, F-extra-agents).
- ❌ Marketplace de providers o import/export de configs.
- ❌ Logs viewer (F26, v1.0).
- ❌ Auto-detección de providers en la red local (búsqueda de
  Ollama en `0.0.0.0:11434`); en v0.1 el usuario introduce
  `base_url` a mano.
- ❌ Configurar un provider **nuevo** que no sea Ollama/Groq/Minimax
  (los 3 de v1 según ADR-0008). El dropdown solo muestra esos 3;
  un `provider_kind` custom se difiere a v1.x con
  `openai_compat` genérico.

## UX / UI

### Rutas y componentes

```
ui/src/
├── lib/
│   ├── routes/
│   │   └── Settings.svelte              # /settings (tabs)
│   ├── components/
│   │   ├── settings/
│   │   │   ├── ProvidersTab.svelte      # lista de providers
│   │   │   ├── ProviderCard.svelte      # card por provider
│   │   │   ├── AddProviderDialog.svelte # modal de creación
│   │   │   ├── ApiKeyField.svelte       # input enmascarado → keychain
│   │   │   ├── TestConnectionBadge.svelte
│   │   │   ├── ModelsTab.svelte
│   │   │   ├── ApprovalTab.svelte       # approval_mode + matriz
│   │   │   ├── PermissionMatrix.svelte # tabla tool × decision
│   │   │   ├── WorkspaceTab.svelte
│   │   │   └── ExtraPathsEditor.svelte
```

### Pantalla `/settings`

- **Layout**: tabs en la parte superior (radios), contenido debajo.
  En móvil (cuando se implemente F16) → tabs verticales.
- **Estado**: cada tab tiene su propio `loading` / `error` /
  `dirty` (cambios sin guardar). Botón **Save** en la parte
  inferior; **Discard** para revertir cambios pendientes.
- **Strings**: todos en `i18n` desde el inicio (F25, v1.0; v0.1
  usa `en` como base y deja los strings hard-coded).

### `ProvidersTab`

```
+------------------------------------------+
|  Providers                                |
+------------------------------------------+
|  [Ollama]              [enabled] [edit]  |
|  http://127.0.0.1:11434                  |
|  Status: connected (12ms)                |
|  Models: 3 (llama3.1:8b, mistral, …)     |
|  [Test connection]                       |
+------------------------------------------+
|  [Groq]               [enabled] [edit]   |
|  https://api.groq.com/openai/v1          |
|  Status: connected (220ms)               |
|  Models: 7                               |
|  API key: ●●●●●●●● (set in keychain)    |
|  [Edit API key] [Test connection]        |
+------------------------------------------+
|  [+ Add provider]                        |
+------------------------------------------+
```

`AddProviderDialog`:
- Dropdown `Kind`: Ollama / Groq / Minimax.
- Input `Base URL` (con placeholder del default del kind).
- Input `API key` (enmascarado, **opcional para Ollama**).
- Botón `Test & Add` que hace `providers_test_connection`
  antes de persistir. Si falla, muestra el error y permite
  "Add anyway" (con warning).

`ApiKeyField`:
- Es un **password input**. Al hacer submit, llama
  `secrets_set(provider_id, value)` y limpia el input.
- **Nunca** se guarda el valor en el estado de Svelte más
  allá del ciclo de envío (se borra del DOM tras success).
- El componente padre solo sabe "tiene key" / "no tiene key"
  (vía `secrets_list_providers`).

### `ApprovalTab`

```
+---------------------------------------------+
|  Approval mode (global)                     |
|  ( ) Ask   (•) Allow   ( ) Deny             |
+---------------------------------------------+
|  Tool permissions (default)                 |
|  Tool           | Default decision          |
|  read_file      | (•) allow                 |
|  search         | (•) allow                 |
|  list_dir       | (•) allow                 |
|  write_file     | ( ) ask    (•) allow     |
|  edit_file      | ( ) ask    (•) allow     |
|  apply_patch    | ( ) ask    (•) allow     |
|  shell          | (•) ask    ( ) allow     |
|  python_run     | (•) ask    ( ) allow     |
+---------------------------------------------+
|  Override per workspace                     |
|  [Workspace picker ▼]                       |
+---------------------------------------------+
```

> El **default** de cada tool está hard-coded en `permissions.md`
> §Defaults. Aquí solo se muestra y se permite editar. La UI no
> expone reglas custom de tipo `pattern` (e.g. "allow `read_file`
> pero no para `*.env`"); eso es v0.2 con F12.

### `WorkspaceTab`

- `ignore_patterns` como lista editable (tag input).
- `journal_max_rows` como número con validación (≥ 1_000).
- `ExtraPathsEditor` delegando a F02 (mismo componente).

### Estados visuales

| Estado | Indicador |
|---|---|
| `idle` (sin cambios) | Save disabled |
| `dirty` (cambios sin guardar) | Save enabled, dot naranja en el tab |
| `saving` | Spinner en Save button |
| `saved` | Toast "Saved" + dot verde breve |
| `error` | Banner rojo arriba con el `code` y `message` (i18n futura) |

## Flow

### Cambiar `default_model`

```
user: cambia el dropdown de default_model
  → Settings.svelte (binding local $state)
  → user click "Save"
  → ipc.invoke("config_update_global", { patch: { defaultModel } })
  → Tauri command en commands/config.rs
  → Config::update_global(GlobalConfigPatch { default_model: Some(...) })
  → valida, persiste atómicamente, re-resuelve secretos (no aplica aquí)
  → retorna GlobalConfigDto
  → UI refresca estado
  → emit "config.changed.v1" (evento para que otras ventanas/pestañas
     recarguen providers si es necesario)
```

### Añadir un provider con API key

```
user: "+ Add provider" → kind: Groq
  → AddProviderDialog.svelte (modal)
  → user rellena base_url, pega api_key
  → user: "Test & Add"
  → ipc.invoke("providers_test_connection", { provider: { kind: "groq", baseUrl, apiKey } })
  → Tauri command llama HTTP GET /models con bearer
  → TestConnectionResult { ok, latencyMs, models[], error? }
  → UI muestra resultado
  → si ok: ipc.invoke("secrets_set", { providerId: "groq", value: apiKey })
           → keychain.set("agentyx", "groq", apiKey)
           → borra apiKey de memoria
  → ipc.invoke("config_update_global", { patch: { providers: { groq: { ... } } } })
  → Config::update_global persiste config.toml (sin secret)
  → emit "config.changed.v1"
  → dialog cierra, ProvidersTab refresca
```

### Cambiar `approval_mode`

```
user: cambia radio a "allow"
  → ApprovalTab.svelte
  → user: "Save"
  → ipc.invoke("config_update_global", { patch: { approvalMode: "allow" } })
  → Config::update_global
  → emit "config.changed.v1"
  → el próximo session_send (F01) lee el nuevo approval_mode al
     arrancar el run (el AgentLoop toma una snapshot de ResolvedConfig
     al inicio de cada run; cambiar approval_mode mid-run no afecta
     al run en curso; ver permissions.md §Snapshot semantics).
```

## Affected domains

- [`providers`](../domains/providers.md) — se añade la operación
  `test_connection(ProviderId)` (nueva en providers.md) y
  `list_models(ProviderId)` (para refrescar caché).
- [`permissions`](../domains/permissions.md) — se añade la operación
  `permissions_get_matrix(workspace_id?)` y
  `permissions_set_default(tool, decision)`.
- [`config`](../domains/config.md) — consume `Config::update_global`
  y `Config::update_workspace`. **F05 no toca el schema**, solo la
  UI.
- [`workspace`](../domains/workspace.md) — `extra_paths` editor delega
  a F02.

## Affected Tauri commands / endpoints / events

### Tauri commands (nuevos en F05)

```rust
#[tauri::command]
pub async fn config_get_global() -> Result<GlobalConfigDto, AppError>;

#[tauri::command]
pub async fn config_update_global(
    state: tauri::State<'_, AppState>,
    patch: GlobalConfigPatch,
) -> Result<GlobalConfigDto, AppError>;

#[tauri::command]
pub async fn config_get_workspace(
    workspace_id: WorkspaceId,
) -> Result<WorkspaceConfigDto, AppError>;

#[tauri::command]
pub async fn config_update_workspace(
    workspace_id: WorkspaceId,
    patch: WorkspaceConfigPatch,
) -> Result<WorkspaceConfigDto, AppError>;

#[tauri::command]
pub async fn providers_test_connection(
    request: TestConnectionRequest,
) -> Result<TestConnectionResult, AppError>;

#[tauri::command]
pub async fn secrets_set(
    provider_id: ProviderId,
    value: String,
) -> Result<(), AppError>;

#[tauri::command]
pub async fn secrets_delete(
    provider_id: ProviderId,
) -> Result<(), AppError>;

#[tauri::command]
pub async fn secrets_list_providers() -> Result<Vec<ProviderId>, AppError>;

#[tauri::command]
pub async fn permissions_get_matrix(
    workspace_id: Option<WorkspaceId>,
) -> Result<PermissionMatrixDto, AppError>;

#[tauri::command]
pub async fn permissions_set_default(
    tool: ToolId,
    decision: PermissionDecision,
) -> Result<(), AppError>;
```

> **CRÍTICO — seguridad de `secrets_set`**: el value de la API key
> viaja desde el frontend al backend en una llamada IPC. La cadena
> vive en memoria Rust solo el tiempo del comando y se descarta
> (la `String` se mueve al `keyring::Entry::set_password` y se
  dropea). El Tauri command loguea **solo** el `provider_id` y
  el resultado, nunca el valor.

### Endpoints HTTP (v0.2, F06)

```
GET    /api/v1/config/global       → GlobalConfigDto
PATCH  /api/v1/config/global       (body: GlobalConfigPatch) → GlobalConfigDto
GET    /api/v1/workspaces/:id/config → WorkspaceConfigDto
PATCH  /api/v1/workspaces/:id/config (body: WorkspaceConfigPatch) → WorkspaceConfigDto
POST   /api/v1/providers/:id/test  → TestConnectionResult
POST   /api/v1/secrets             (body: { providerId, value }) → {}
DELETE /api/v1/secrets/:providerId → {}
GET    /api/v1/secrets             → Vec<ProviderId>
GET    /api/v1/permissions         ?workspace=... → PermissionMatrixDto
PATCH  /api/v1/permissions/default (body: { tool, decision }) → {}
```

> El endpoint `POST /api/v1/secrets` requiere `Authorization: Bearer <token>`
> y el body viaja sobre HTTPS local (F06 + F19). v0.1 (solo Tauri)
> no expone HTTP.

### Eventos (nuevos en F05)

| Evento | Schema | Payload | Cuándo se emite |
|---|---|---|---|
| `config.changed.v1` | `{ kind, global, workspace? }` | `kind: "global" \| "workspace"`, `global?: GlobalConfigSummaryDto`, `workspace?: WorkspaceConfigSummaryDto` | Tras `config_update_*` exitoso |
| `providers.changed.v1` | `{ providerId, action }` | `providerId: ProviderId`, `action: "added" \| "updated" \| "removed" \| "enabled" \| "disabled" \| "key_set" \| "key_deleted"` | Tras mutación de provider o secret |
| `permissions.changed.v1` | `{ workspaceId?, tool, decision }` | `workspaceId?: WorkspaceId`, `tool: ToolId`, `decision: PermissionDecision` | Tras `permissions_set_default` o `permissions_update_matrix` |

> `permissions.changed.v1` se emite también desde F01 cuando el
> usuario hace "remember this decision" en un prompt; este spec
> solo declara el shape y cuándo se emite desde F05.

## Acceptance criteria

- [ ] **F05.AC1**: con la app recién instalada y `config.toml`
  global aún no creado, abrir `/settings` → `ProvidersTab` muestra
  **Ollama preconfigurado** con `base_url = "http://127.0.0.1:11434"`,
  `enabled = true`, y los botones `Test connection` /
  `Add provider`. **Test**:
  `f05_ac1_settings_shows_ollama_preconfigured`.
- [ ] **F05.AC2**: añadir Groq con una API key válida → la nueva
  card aparece en `ProvidersTab`, con `Status: connected`, latencia
  medida, y modelos listados. `~/.agentyx/config.toml` contiene
  `[providers.groq]` con `api_key = "env:..."` apuntando a la env
  var de la sesión de test (o keychain, según el camino del
  test). **Test**:
  `f05_ac2_add_provider_persists_with_secret_ref`.
- [ ] **F05.AC3**: añadir Groq con API key inválida → `Test
  connection` retorna `{ ok: false, error: "invalid_api_key" }`
  con un mensaje user-friendly. La UI muestra el error y permite
  re-intentar o cancelar. **Test**:
  `f05_ac3_test_connection_invalid_key_shows_error`.
- [ ] **F05.AC4**: cambiar `default_model` y guardar → el siguiente
  `session_send` (verificado con un test que llama a
  `agents.default_for_new_session` o equivalente) usa el nuevo
  modelo. `config.toml` se actualiza atómicamente. **Test**:
  `f05_ac4_change_default_model_persists_and_takes_effect`.
- [ ] **F05.AC5**: cambiar `approval_mode` global a `"deny"` y
  guardar → el siguiente tool call de `write_file` desde un run
  activo retorna `PermissionDecision::Deny` sin prompt al usuario.
  **Test**: `f05_ac5_approval_mode_deny_blocks_writes_silently`.
- [ ] **F05.AC6**: cambiar `approval_mode` por workspace
  (override) → el override se aplica solo a ese workspace. Abrir
  otro workspace distinto → usa el global. **Test**:
  `f05_ac6_workspace_approval_mode_override_isolated`.
- [ ] **F05.AC7**: `secrets_set("groq", "gsk_...")` con un fake
  keychain → `secrets_list_providers()` retorna `["groq"]`. El
  valor de la key **no** aparece en `secrets_list_providers` ni
  en `config_get_global` (DTO). **Test**:
  `f05_ac7_secrets_set_then_list_returns_only_ids`.
- [ ] **F05.AC8**: `secrets_set` con un value que coincide con un
  patrón de API key literal (e.g. `"sk-abc..."`) y la env var
  `GROQ_API_KEY` no está seteada → la próxima vez que el agent
  loop intente usar Groq, retorna `invalid_input` con mensaje
  claro (verificado en `config.md` AC6, pero F05 re-verifica que
  la UI lo muestra con un toast). **Test**:
  `f05_ac8_missing_env_at_runtime_surfaces_to_ui`.
- [ ] **F05.AC9**: la matriz de permisos en `ApprovalTab` lista
  todas las tools de [`tools.md`](../domains/tools.md) y permite
  cambiar el default de `ask` a `allow` y viceversa. Los cambios
  persisten en `GlobalConfig` y se aplican al siguiente run.
  **Test**: `f05_ac9_permission_matrix_edits_persist`.
- [ ] **F05.AC10**: al añadir un provider, el `Test & Add` hace
  `providers_test_connection` **antes** de persistir; si falla,
  el provider no se añade y el usuario puede cancelar o
  "Add anyway". **Test**:
  `f05_ac10_failed_test_does_not_persist_provider`.
- [ ] **F05.AC11**: cerrar la app y reabrir → la pantalla
  `/settings` carga los valores persistidos y los secrets
  siguen accesibles (no se pierde el `api_key` que está en
  keychain). **Test**:
  `f05_ac11_settings_persist_across_app_restart`.
- [ ] **F05.AC12**: la UI **nunca** muestra el valor real de un
  secret; solo el badge "API key: set in keychain" o el botón
  "Edit API key" (que abre un input vacío). Test E2E en
  Playwright: capturar el árbol DOM tras un save y verificar
  que no contiene el value literal. **Test**:
  `f05_ac12_ui_never_displays_secret_value`.
- [ ] **F05.AC13**: el `ExtraPathsEditor` (delegado a F02)
  aparece dentro de `WorkspaceTab` y se comporta como F02
  describe (add con confirmación, remove con confirmación,
  validación de path absoluto, no duplicados). **Test**:
  `f05_ac13_extra_paths_editor_delegates_to_f02`.
- [ ] **F05.AC14**: el cambio de `update_channel` a `"dev"`
  muestra un warning visible ("You're switching to a less
  stable channel") y requiere confirmación antes de guardar.
  **Test**: `f05_ac14_update_channel_dev_requires_confirmation`.
- [ ] **F05.AC15**: emitir `config.changed.v1` con `kind: "global"`
  tras un `config_update_global` exitoso; los listeners (otro
  tab, otra ventana) refrescan su estado. **Test**:
  `f05_ac15_config_changed_event_emitted_and_received`.

## Tests

- **Unit (Rust)**:
  - `crates/agentyx-core/src/config.rs::tests` (ya en config.md).
  - `crates/agentyx-core/src/providers/{ollama,groq,minimax}.rs::tests`
    — `test_connection` y `list_models` con HTTP mock (wiremock).
  - `crates/agentyx-core/src/permissions/matrix.rs::tests` — para
    `permissions_set_default`.
- **Integration (Rust)**: `crates/agentyx-core/tests/config_integration.rs`
  con `FakeKeychain` y DB temporal.
- **Unit (TS)**: `ui/src/lib/components/settings/ProviderCard.test.ts`,
  `AddProviderDialog.test.ts`, `PermissionMatrix.test.ts` con
  `@testing-library/svelte` y mocks de `ipc.invoke`.
- **E2E (Playwright)**: `ui/e2e/settings.spec.ts` con un
  test que arranca la app con un fake provider y verifica
  el flujo completo (add provider, save, restart, ver persistido).
- **Security test**: `crates/agentyx-app/tests/secrets_no_leak.rs` —
  instrumenta `tracing` y verifica que ningún log contiene el
  valor de un secret durante el flujo de F05.

## Telemetry / logs

```rust
tracing::info!(
    provider_id = %provider_id,
    action = "added",
    "provider configured"
);

tracing::warn!(
    provider_id = %provider_id,
    error_code = %e.code(),
    "test connection failed"
);

tracing::info!(
    setting = "approval_mode",
    from = %old,
    to = %new,
    "approval mode changed"
);
```

> **Nunca** loguear:
> - El valor de un secret (ni siquiera en debug).
> - El contenido de `~/.agentyx/config.toml` (puede tener
>   rutas que el usuario considera sensibles).
> - El header `Authorization` de un provider.

## Security notes

- **CSP**: la pantalla de Settings no introduce inline scripts.
  El `ApiKeyField` usa `<input type="password">` estándar.
- **Capabilities Tauri**: el `tauri.conf.json` capability `settings`
  habilita solo los commands `config_*`, `providers_*`, `secrets_*`,
  `permissions_*` para la ventana de Settings. El chat principal
  (F01) no puede llamar a `secrets_set` ni `config_update_*`
  (defensa en profundidad).
- **Persistencia**: el `config.toml` se escribe a `~/.agentyx/`
  con permisos `0o600` (el `tempfile` crate respeta el umask;
  en macOS/Linux se aplica `chmod` post-rename). Si el FS no lo
  permite, warning al usuario.
- **Audit**: cada `config_update_*` se registra en el journal
  del workspace activo (no en global). El payload NO contiene
  el valor de un secret; solo el `providerId` y la `action`.
- **Browser extension / DevTools**: si el usuario abre las
  DevTools de Tauri, podría ver el value del `ApiKeyField`
  mientras se escribe. Esto es aceptable: el threat model no
  incluye "usuario comprometido"; F05 no defiende contra
  keyloggers ni DevTools activas.
- **No secrets en URL**: `providers_test_connection` usa body o
  headers, **nunca** query params con el `api_key`.

## Rollout

- **Feature flag**: no. F05 entra con el MVP.
- **Settings al primer arranque**: la pantalla `/settings` se
  muestra automáticamente en F23 (Onboarding) si no hay
  provider `enabled` distinto de Ollama (o si Ollama no está
  corriendo). En v0.1, el usuario llega a `/settings` desde
  el menú / shortcut `Cmd+,`.
- **Migración de datos**: ninguna (config.toml no existe al
  primer arranque; se crea con defaults).
- **Compatibilidad**: el formato de `config.toml` se versiona
  con `version = 1`; futuras versiones de la app introducirán
  migraciones (ver `config.md` §Open questions Q6).

## Open questions

- **Q1**: ¿Settings debe tener un **wizard** de primer arranque
  (F23) o la pantalla plana con tabs es suficiente? → **Decisión
  diferida a F23**. F05 implementa la pantalla plana; F23
  decide si añade un wizard encima.
- **Q2**: ¿La matriz de permisos en v0.1 permite reglas con
  patrón (e.g. "ask para `read_file` si path contiene
  `.env`")? → **No**. v0.1 es matriz simple (tool → decision).
  v0.2 con F12 introduce reglas con patrón.
- **Q3**: ¿El usuario puede añadir un provider **custom** (no
  Ollama/Groq/Minimax) en v0.1? → **No** (ver §Scope). En v0.1
  el dropdown muestra solo los 3 kinds.
- **Q4**: ¿El cambio de `approval_mode` aplica al run en curso
  o solo al siguiente? → **Solo al siguiente** (snapshot
  semantics; ver `permissions.md` §Snapshot). La UI lo indica
  con un tooltip "Applies to new runs".
- **Q5**: ¿`providers_test_connection` debe correr con timeout
  y reintentos? → **Sí**, timeout 5s, sin reintentos. Test
  cubre timeout con un servidor fake que duerme 10s.
- **Q6**: ¿`secrets_set` con un value vacío se permite (para
  "borrar" un secret sin `secrets_delete`)? → **No**. value
  vacío retorna `invalid_input`. Para borrar, usar
  `secrets_delete`.
- **Q7**: ¿`extra_paths` en Settings debe poder **editar** un
  path existente (renombrar, mover) o solo add/remove? → v0.1
  solo add/remove. Edit se difiere a v1.x.

## References

- [`../glossary.md`](../glossary.md) — `SecretRef`, `Keychain`,
  `ProviderId`, `ToolId`, `WorkspaceId`.
- [`../ipc.md`](../ipc.md) — Tauri command shape, error shape,
  eventos.
- [`../architecture.md`](../architecture.md) — `AppState.config`,
  flujo de datos.
- [`config.md`](../domains/config.md) — schema TOML, validación,
  `SecretRef`, `Config::update_global`.
- [`providers.md`](../domains/providers.md) — `Provider` trait,
  `test_connection`, `list_models`.
- [`permissions.md`](../domains/permissions.md) — `PermissionDecision`,
  matriz, defaults por tool.
- [`workspace.md`](../domains/workspace.md) — `extra_paths`,
  `ignore_patterns`, `journal_max_rows`.
- [`F02-multi-workspace.md`](./F02-multi-workspace.md) — UI de
  `extra_paths` que F05 delega.
- [`features/ROADMAP.md`](./ROADMAP.md) — F05 en Phase 2.
- AGENTS.md §8.3 (Config), §9 (Seguridad), §15 (Checklist).
