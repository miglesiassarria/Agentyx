# Config

**Status**: draft
**Owner**: @miglesias
**Last update**: 2026-06-06
**Affects**: `providers`, `permissions`, `workspace`, F05
(Settings), F01 (carga provider/model por defecto), todos los
features que necesiten credenciales.

## Agent context

- Leer primero este bloque, `Operations`, `Validation rules`, `Edge
  cases` y `Acceptance criteria`; el TOML completo solo hace falta si
  cambian campos.
- Dominio bloqueante para F05 y F01: carga config global
  `~/.agentyx/config.toml`, config por workspace y produce
  `ResolvedConfig` en memoria.
- Contratos centrales: `GlobalConfig`, `WorkspaceConfig`,
  `ProviderConfig`, `SecretRef`, `ResolvedConfig`,
  `Config::load_global`, `load_workspace`, `resolve_secrets`,
  `update_global`, `update_workspace`.
- Reglas no negociables: ningún secreto literal en TOML; API keys via
  `env:VAR` o keychain; validar fail-fast; no loguear secretos;
  `telemetry_enabled = false` por defecto.
- F05 escribe config/secrets; F01 solo consume provider/model y
  approval mode resueltos.

> Modelo de configuración de Agentyx: dos niveles (global en
> `~/.agentyx/config.toml` y por workspace en
> `<workspace>/.agentyx/config.toml`), con un sistema de
> `SecretRef` que garantiza que **ningún secreto** (API keys,
> bearer tokens) se persiste en texto plano en disco. Los
> secretos se resuelven en arranque desde variables de entorno
> o desde el keychain del SO, se cachean en memoria y nunca
> se loguean.

## Goal

Definir, validar y resolver la configuración de Agentyx — global
y por workspace — con un único punto de verdad para:

1. Qué providers están activos y sus endpoints / API keys.
2. Qué modelo se usa por defecto (provider + model_id).
3. El modo de aprobación global (`ask` | `allow` | `deny`).
4. Settings de UI (theme, font, telemetry).
5. Overrides por workspace (modelo distinto, venv path, etc.).

Garantías:

- **Secretos fuera de disco**: las API keys nunca se guardan en
  `config.toml`; se referencian vía `env:VAR_NAME` o keychain.
  Si el TOML contiene un string que parece un secret literal,
  el loader falla con `invalid_input` claro.
- **Resolución única**: `Config::load()` se llama una vez al
  arranque y cachea el resultado en `AppState`. Re-cargar es
  explícito (`settings_update` o al abrir un workspace nuevo).
- **Validación fail-fast**: cualquier inconsistencia (provider
  desconocido, URL inválida, modelo vacío) aborta el arranque
  con un error actionable; nunca defaults silenciosos.
- **No telemetría por defecto**: `telemetry_enabled = false`.

## Non-goals

- ❌ Recarga en caliente de config (cambiar provider requiere
  reinicio de la app en v1).
- ❌ Configuración distribuida / sync entre devices.
- ❌ Variables de entorno inline en CLI args (todo va por TOML).
- ❌ Encriptación del TOML mismo (el FS del SO es responsable;
  el usuario puede usar FS encrypted, lo cual es cross-OS).
- ❌ Soporte de `secrets.env` alternativo a OS keychain.
- ❌ Migraciones de config en v1 (el campo `version = 1` se
  valida literal; migraciones se difieren a v1.x).

## Glossary

Términos locales (los globales están en [`../glossary.md`](../glossary.md)):

- **GlobalConfig**: el TOML en `~/.agentyx/config.toml`. Existe
  siempre tras el primer arranque; tiene defaults si no se
  provee.
- **WorkspaceConfig**: el TOML en `<workspace>/.agentyx/config.toml`.
  Opcional; si no existe, el workspace hereda todo de
  `GlobalConfig`.
- **ResolvedConfig**: combinación de `GlobalConfig` +
  `WorkspaceConfig` tras aplicar overrides, **con secretos ya
  expandidos** (en memoria, nunca en disco).
- **SecretRef**: tipo que indica dónde vive un secreto:
  `Env(String)` (var de entorno) o `Keychain { service, account }`
  (entrada del keychain del SO).
- **Keychain service name**: constante `"agentyx"`. Se usa como
  `service` en `keyring` crate. `account` es el `ProviderId`.

## State

Persiste en TOML, en disco.

| Dato | Ubicación | Quién lee | Quién escribe |
|---|---|---|---|
| `GlobalConfig` | `~/.agentyx/config.toml` | `Config::load_global` al arranque | `settings_update` (F05) |
| `WorkspaceConfig` | `<workspace>/.agentyx/config.toml` | `Config::load_workspace` al abrir workspace | `settings_update_workspace` (F05) |
| API keys y bearer tokens | `process.env` o keychain del SO (`agentyx` service) | `Config::resolve_secrets` | `secrets_set` (F05) |
| `ResolvedConfig` (en memoria) | `AppState.config: Arc<RwLock<ResolvedConfig>>` | `agent-loop`, `providers`, `permissions`, F01, F09 | `Config::reload` (explícito) |

> **Sin secretos en `config.toml`**. El TOML contiene solo
> referencias (`SecretRef`); los valores se resuelven en
> `Config::load` y se cachean en `ResolvedConfig.secrets` (un
> `HashMap<ProviderId, String>` en memoria).

### Formato TOML

#### `~/.agentyx/config.toml` (v1)

```toml
version = 1

# Modo de aprobación global. Por defecto "ask".
#   "ask"  → prompt de aprobación para writes, shell, network
#   "allow"→ nunca prompt (no recomendado; use con workspaces sandboxed)
#   "deny" → bloquea writes, shell, network (solo lectura)
approval_mode = "ask"

# Provider por defecto (debe existir en [providers]).
default_provider = "ollama"

# Modelo por defecto (debe existir en providers[default_provider].models
# o ser obtenible por /models en arranque).
default_model = "llama3.1:8b"

# --- Providers ---

[providers.ollama]
base_url = "http://127.0.0.1:11434"
enabled = true
# API key opcional para Ollama (no requerida por defecto).
# Si se omite, Ollama corre sin auth.
api_key = "env:OLLAMA_API_KEY"  # opcional

[providers.groq]
base_url = "https://api.groq.com/openai/v1"
enabled = true
api_key = "env:GROQ_API_KEY"     # requerido
models = ["llama-3.3-70b-versatile", "llama-3.1-8b-instant"]

[providers.minimax]
base_url = "https://api.minimax.io/anthropic"
enabled = true
api_key = "env:MINIMAX_API_KEY"   # requerido
models = ["MiniMax-M3", "MiniMax-M2.7", "MiniMax-M2.5"]

# --- UI ---

[ui]
theme = "auto"                    # "auto" | "light" | "dark"
font_size = 14
show_token_count = true
show_timestamps = true

# --- Misc ---

telemetry_enabled = false         # off by default (local-first)
check_updates = true
update_channel = "stable"         # "stable" | "beta" | "dev"
```

#### `<workspace>/.agentyx/config.toml` (opcional, v1)

```toml
version = 1

# Override del modelo por defecto en ESTE workspace.
default_provider = "groq"
default_model = "llama-3.3-70b-versatile"

# Override del modo de aprobación.
approval_mode = "deny"            # este workspace es read-only

# Settings específicos de workspace.
[workspace]
ignore_patterns = ["node_modules/", "target/", ".venv/", "dist/"]
journal_max_rows = 100_000        # override del threshold de archivado

[python]
# v0.1.x: solo lectura. v0.1 ignora este bloque.
# venv_path = ".venv"
# python_path = ".venv/bin/python"
```

> El workspace config puede **solo overridear** las claves de
> global; no puede definir providers nuevos. Si el workspace
> quiere un provider que no está en global, debe añadirlo vía
> F05 (Settings) que modifica el global.

## Operations

### Tipos Rust (shape)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalConfig {
    pub version: u32,                          // == 1
    pub approval_mode: ApprovalMode,
    pub default_provider: ProviderId,
    pub default_model: String,
    pub providers: HashMap<ProviderId, ProviderConfig>,
    pub ui: UiConfig,
    pub telemetry_enabled: bool,
    pub check_updates: bool,
    pub update_channel: UpdateChannel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub base_url: String,
    pub enabled: bool,
    pub api_key: Option<SecretRef>,            // None para Ollama sin auth
    pub models: Option<Vec<String>>,           // None → se descubre en arranque
    #[serde(default)]
    pub extra: HashMap<String, serde_json::Value>,  // provider-specific
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalMode {
    Ask,    // default
    Allow,  // sin prompts
    Deny,   // bloquea writes/shell/network
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    Stable,
    Beta,
    Dev,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiConfig {
    pub theme: Theme,
    pub font_size: u8,
    pub show_token_count: bool,
    pub show_timestamps: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Theme { Auto, Light, Dark }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SecretRef {
    /// Variante TOML: `env:VAR_NAME`.
    Env(String),
    /// Variante TOML: `keychain:<service>:<account>`.
    /// En v1, `service` se ignora y se usa "agentyx".
    Keychain { account: String },
}
```

> **Custom deserializer para `SecretRef`**: el TOML lo representa
> como string (`"env:GROQ_API_KEY"` o `"keychain:groq"`). El
> deserializer parsea el prefijo y construye la variante. La
> **forma serializada** también es un string.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceConfig {
    pub version: u32,                          // == 1
    pub default_provider: Option<ProviderId>,  // override
    pub default_model: Option<String>,         // override
    pub approval_mode: Option<ApprovalMode>,   // override
    #[serde(default)]
    pub workspace: WorkspaceSettings,
    #[serde(default)]
    pub python: PythonWorkspaceSettings,        // ignorado en v0.1
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSettings {
    pub ignore_patterns: Vec<String>,
    pub journal_max_rows: Option<u64>,         // default 100_000
    pub extra_paths: Vec<ExtraPathConfig>,     // ver workspace.md + ADR-0007
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonWorkspaceSettings {
    pub venv_path: Option<PathBuf>,
    pub python_path: Option<PathBuf>,
}
```

```rust
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub global: GlobalConfig,
    pub workspace: Option<WorkspaceConfig>,
    /// API keys por provider, ya expandidas desde SecretRef.
    /// NUNCA se serializa a disco, NUNCA se loguea.
    pub secrets: HashMap<ProviderId, String>,
    /// Resolved final: workspace override > global.
    pub effective: EffectiveConfig,
}

#[derive(Debug, Clone)]
pub struct EffectiveConfig {
    pub approval_mode: ApprovalMode,
    pub default_provider: ProviderId,
    pub default_model: String,
    pub workspace_settings: WorkspaceSettings,
}
```

### `Config::load_global`

```rust
pub fn load_global() -> Result<GlobalConfig, AppError>;
```

- Lee `~/.agentyx/config.toml`.
- Si no existe, **crea el archivo con defaults** (write atómico:
  `*.toml.tmp` + rename).
- Valida el schema (ver §Validación).
- Retorna `GlobalConfig`.

**Errores**:
- `invalid_input` — TOML malformado, `version != 1`, clave
  requerida faltante, `SecretRef` con formato inválido,
  `api_key` con valor **literal** (no `env:` ni `keychain:`).
- `internal` — error de I/O al crear el archivo de defaults.

### `Config::load_workspace(workspace_root)`

```rust
pub fn load_workspace(workspace_root: &Path) -> Result<WorkspaceConfig, AppError>;
```

- Lee `<workspace_root>/.agentyx/config.toml`.
- Si no existe, retorna `Ok(WorkspaceConfig::default())` (no error).
- Valida el schema.
- Retorna `WorkspaceConfig`.

**Errores**: igual que `load_global` si el archivo existe y es
inválido.

### `Config::resolve(global, workspace, keychain) -> ResolvedConfig`

```rust
pub fn resolve(
    global: GlobalConfig,
    workspace: Option<WorkspaceConfig>,
    keychain: &dyn KeychainAccess,
) -> Result<ResolvedConfig, AppError>;
```

- Para cada `ProviderConfig` con `api_key: Some(SecretRef)`:
  - `SecretRef::Env(var)` → lee `process.env[var]`. Si no
    existe, retorna `invalid_input` claro ("GROQ_API_KEY not
    set").
  - `SecretRef::Keychain { account }` → lee del keychain.
    Si no existe, retorna `internal` con sugerencia de usar
    `secrets_set`.
- Calcula `EffectiveConfig` (workspace > global).
- Retorna `ResolvedConfig`.

> **CRÍTICO**: `ResolvedConfig.secrets` contiene los valores
> reales. Se cachea en `AppState` y **nunca** se serializa a
> disco, **nunca** se loguea (ni en `tracing::debug!`), y se
> descarta al cerrar la app.

### `Config::validate_global`

```rust
pub fn validate_global(&self) -> Result<(), AppError>;
```

Validaciones (todas con `validator` derive + checks custom):

- `version == 1`.
- `default_provider` está en `providers` y `enabled = true`.
- `default_model` no está vacío.
- `providers` no está vacío.
- Cada `ProviderConfig.base_url` parsea como URL válida
  (`url::Url::parse`).
- Si `api_key` es `Some(SecretRef::Env(var))`, validar formato
  del nombre de variable (regex `^[A-Z][A-Z0-9_]*$`).
- `ui.font_size` ∈ [10, 24].
- `approval_mode` es uno de los 3 valores.

### `Config::validate_workspace`

Validaciones análogas +:

- Si `default_provider` está presente, debe existir en el
  `ResolvedConfig.global.providers` y estar `enabled`.
- `ignore_patterns` son strings no vacíos; sintaxis de glob
  se valida lazy (al primer uso, en `tools.search`).
- `journal_max_rows` ≥ 1_000 y ≤ 10_000_000.

### `Config::update_global(patch)`

```rust
pub fn update_global(patch: GlobalConfigPatch) -> Result<GlobalConfig, AppError>;
```

- Llamado por F05 (`settings_update`).
- Aplica el patch al `GlobalConfig` actual.
- Re-valida.
- Re-resuelve secretos (si cambiaron providers o SecretRefs).
- Persiste atómicamente a `~/.agentyx/config.toml` (escribe
  `*.toml.tmp` y `rename`).
- Retorna el nuevo `GlobalConfig`.

### `KeychainAccess` trait

Abstracción sobre `keyring` crate, para que los tests puedan
inyectar un fake:

```rust
pub trait KeychainAccess: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>, AppError>;
    fn set(&self, account: &str, value: &str) -> Result<(), AppError>;
    fn delete(&self, account: &str) -> Result<(), AppError>;
}

pub struct OsKeychain;  // implementación real con keyring::Entry
```

> **Service name**: `agentyx`. Es constante y no configurable
> en v1.

### `SecretRef` shape en TOML

| TOML | Variante |
|---|---|
| `"env:GROQ_API_KEY"` | `Env("GROQ_API_KEY")` |
| `"keychain:groq"` | `Keychain { account: "groq" }` |
| `"sk-1234abcd..."` (literal) | **Error**: `invalid_input` con mensaje "API key literals are not allowed; use `env:VAR_NAME` or `keychain:account`" |
| `""` (vacío) | Error: `invalid_input` con "SecretRef cannot be empty" |

> El check de "literal" se hace por heurística: si el string
> empieza con `env:`, `keychain:` o tiene un patrón conocido
> de API key (e.g. `sk-`, `gsk_`, `sk-ant-`), se rechaza. Si
> no encaja en ningún prefijo conocido, se trata como
> `SecretRef::Env` (asunción conservadora).

## Contracts

### Tauri commands

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
```

> `GlobalConfigDto` y `WorkspaceConfigDto` son las versiones
> serializables (sin secretos), expuestas a la UI. El shape
> exacto se fija cuando F05 implemente la pantalla.

Ver [`../ipc.md`](../ipc.md) para convenciones (snake_case Rust,
camelCase TS, errores como `{code, message, context?}`).

### Endpoints HTTP

En v0.2 con F06:

```
GET  /api/v1/config/global       → GlobalConfigDto
PATCH /api/v1/config/global      (body: GlobalConfigPatch) → GlobalConfigDto
GET  /api/v1/workspaces/:id/config → WorkspaceConfigDto
PATCH /api/v1/workspaces/:id/config (body: WorkspaceConfigPatch) → WorkspaceConfigDto
```

### Eventos streaming

Ninguno. La config no es streamable.

### Tablas / archivos

| Archivo | Formato | Creador | Notas |
|---|---|---|---|
| `~/.agentyx/config.toml` | TOML | `Config::load_global` (crea con defaults si no existe) | Write atómico: `*.toml.tmp` + `rename` |
| `<workspace>/.agentyx/config.toml` | TOML | usuario o `Config::load_workspace` (no crea con defaults) | Si no existe, OK con defaults vacíos |
| `~/.agentyx/config.toml.bak` | TOML | backup automático en `update_global` antes de sobreescribir | Última versión anterior, rotación máx 3 backups |

## Edge cases

1. **TOML malformado** (sintaxis rota, clave duplicada, tipo
   incorrecto): `invalid_input` con línea y columna si `toml`
   crate lo provee.
2. **`api_key` con valor literal** (no prefijo `env:` o
   `keychain:`): rechazado con `invalid_input` y mensaje que
   dice exactamente cómo usar `SecretRef`.
3. **`version` ≠ 1**: `invalid_input` claro: "Config version X
   is not supported. Expected 1. Please update Agentyx or
   migrate your config." (en v1, sin auto-migración).
4. **Provider `enabled = false` referenciado como
   `default_provider`**: `invalid_input` con sugerencia de
   activarlo o cambiar el default.
5. **`env:VAR_NAME` donde la variable no existe**: `invalid_input`
   con el nombre de la variable y la sugerencia de definirla.
6. **Keychain sin entrada para `account`**: `internal` con
   sugerencia de usar `secrets_set` (F05). NO se considera
   warning silencioso; el provider no arranca sin su key.
7. **Dos workspaces abiertos con providers distintos que
   requieren keys distintas**: cada `Config::resolve` se hace
   al abrir el workspace; las keys se cachean en `AppState`
   con scope de workspace. NO hay conflicto (son scopes
   distintos).
8. **Update atómico durante escritura concurrente**: el
   write atómico con `*.tmp + rename` en POSIX es atómico;
   en Windows requiere `ReplaceFileW` (lo maneja `tempfile`
   crate). Si el rename falla, el `.tmp` queda como
   `.toml.tmp` y se borra al siguiente load exitoso.
9. **Archivo `config.toml` con permisos `0o644` (legible por
   todos) en un sistema multi-user**: warning al usuario
   "config.toml is world-readable; consider `chmod 600`".
   No se aborta; el secreto sigue seguro (no está en el
   archivo), pero el resto del contenido puede tener info
   sensible (paths).
10. **`approval_mode = "deny"` global + workspace pide
    `default_provider = "groq"`**: válido. La denegación solo
    afecta a las tools, no a la conectividad de red saliente
    del provider. Tests cubren esto.
11. **`update_channel = "dev"` en producción** (release firmado):
    warning en arranque, no se aborta.
12. **Config con `providers` vacío y `default_provider` apuntando
    a algo**: error de validación "no providers configured".
13. **Lectura de `keychain` durante tests de CI** (sin
    keychain del SO disponible): los tests usan
    `FakeKeychain` (en memoria) inyectado vía `KeychainAccess`.
    No se testea el `OsKeychain` real en CI.

## Acceptance criteria

- [ ] **AC1**: `Config::load_global` con `~/.agentyx/config.toml`
  inexistente crea el archivo con defaults (`default_provider =
  "ollama"`, `default_model = "llama3.1:8b"`, `approval_mode =
  "ask"`, `telemetry_enabled = false`). **Test**:
  `ac1_load_global_creates_defaults_when_missing`.
- [ ] **AC2**: el TOML generado tiene `version = 1` y pasa la
  validación de `validate_global`. **Test**:
  `ac2_defaults_file_is_valid_and_versioned`.
- [ ] **AC3**: `Config::load_global` con TOML válido retorna
  el `GlobalConfig` parseado. **Test**:
  `ac3_load_global_parses_valid_toml`.
- [ ] **AC4**: TOML con `version = 2` retorna `invalid_input`
  con mensaje que incluye la versión encontrada y la esperada.
  **Test**: `ac4_load_global_rejects_wrong_version`.
- [ ] **AC5**: TOML con `api_key = "sk-1234..."` (literal)
  retorna `invalid_input` con sugerencia de `env:` o
  `keychain:`. **Test**:
  `ac5_literal_api_key_rejected_with_helpful_message`.
- [ ] **AC6**: TOML con `api_key = "env:GROQ_API_KEY"` y la
  variable de entorno unset retorna `invalid_input` claro
  durante `resolve`. **Test**:
  `ac6_resolve_missing_env_returns_invalid_input`.
- [ ] **AC7**: `SecretRef::Env("GROQ_API_KEY")` con la var
  definida se expande correctamente; el valor aparece en
  `ResolvedConfig.secrets["groq"]`. **Test**:
  `ac7_resolve_expands_env_secret`.
- [ ] **AC8**: `SecretRef::Keychain { account: "groq" }` se
  lee del keychain (fake en test) y aparece en
  `ResolvedConfig.secrets["groq"]`. **Test**:
  `ac8_resolve_expands_keychain_secret`.
- [ ] **AC9**: `ResolvedConfig` serializado a JSON (para
  debugging en tests) **nunca** contiene los valores de
  `secrets`. Se verifica con un test que hace `serde_json`
  y greps por el valor del secret. **Test**:
  `ac9_resolved_config_dto_never_exposes_secrets`.
- [ ] **AC10**: `update_global(patch)` reescribe el archivo
  atómicamente; el archivo original se preserva como `.bak`
  si el nuevo es inválido. **Test**:
  `ac10_update_global_atomic_write_creates_backup`.
- [ ] **AC11**: `update_global` con un patch inválido no
  modifica el archivo y retorna `invalid_input`. **Test**:
  `ac11_update_global_invalid_patch_keeps_original`.
- [ ] **AC12**: `load_workspace` con archivo inexistente
  retorna `Ok(WorkspaceConfig::default())`. **Test**:
  `ac12_load_workspace_missing_returns_default`.
- [ ] **AC13**: `EffectiveConfig` aplica override del
  workspace: si el workspace tiene `default_model = "..."`
  y global tiene otro, gana el workspace. **Test**:
  `ac13_effective_config_workspace_overrides_global`.
- [ ] **AC14**: `default_provider` que no está en global
  retorna `invalid_input` en `validate_workspace`. **Test**:
  `ac14_workspace_unknown_provider_rejected`.
- [ ] **AC15**: `update_global` loguea `tracing::info!` con
  **qué** providers se modificaron (no los secretos). Un
  test verifica que el log NO contiene el valor de la API
  key. **Test**:
  `ac15_update_global_logs_changes_without_secrets`.
- [ ] **AC16**: el `FakeKeychain` usado en tests se inyecta
  vía `KeychainAccess`; `OsKeychain` no se testea en CI
  (doc-test o test ignored en entornos sin keychain).
  **Test**:
  `ac16_fake_keychain_is_default_in_tests`.
- [ ] **AC17**: `validate_global` rechaza `font_size = 5` y
  `font_size = 100` con `invalid_input`. **Test**:
  `ac17_font_size_out_of_range_rejected`.
- [ ] **AC18**: `validate_global` rechaza `base_url` que no
  parsea como URL. **Test**: `ac18_invalid_base_url_rejected`.

## Discovered bugs (post-approval)

| ID | Date | Category | Resolved in | Notes |
|---|---|---|---|---|
| _ninguno aún_ | | | | |

## Open questions

- **Q1**: ¿`ui.theme` debería estar en global o por workspace?
  → **Propuesta**: global. Un usuario no cambia de tema por
  workspace.
- **Q2**: ¿El override `approval_mode = "deny"` debería
  impedir también la creación de nuevas sesiones? → **No**.
  El usuario puede crear sesiones en modo read-only y el
  agente responde con "I can only read" o emite errores
  controlados. Decisión de UX, no de bloqueante.
- **Q3**: ¿Soporte de `secrets.list` (listar entradas
  existentes en keychain)? → **Sí**, vía F05 Tauri command
  `secrets_list_providers() -> Vec<ProviderId>`. No expone
  los valores, solo qué providers tienen secret configurado.
- **Q4**: ¿`update_channel = "dev"` requiere confirmación
  extra? → **Sí**, en F23 (Onboarding) y F20 (Updaters) se
  define la UX. Aquí solo se valida el valor.
- **Q5**: ¿El config del workspace puede tener un bloque
  `[providers.X]` propio (override del provider en este
  workspace)? → **No en v1**. Diferido a v1.x. v1: el
  workspace solo overridea defaults y settings de workspace.
- **Q6**: ¿Migración automática de `config.toml` si se
  actualiza la app? → **No en v1**. v1.x introducirá
  `Migration` con versionado y semver de config.

## References

- [`../glossary.md`](../glossary.md) — `SecretRef`, `Keychain`.
- [`../ipc.md`](../ipc.md) — Tauri commands, error shape.
- [`../architecture.md`](../architecture.md) — `AppState.config`.
- [`providers.md`](./providers.md) — consume `ProviderConfig`.
- [`permissions.md`](./permissions.md) — consume `ApprovalMode`.
- [`workspace.md`](./workspace.md) — `WorkspaceConfig`, `extra_paths`.
- [`../adr/`](../adr/) — ADR-0006 (sqlite), ADR-0008 (providers).
- [keyring crate](https://docs.rs/keyring/) — backend de
  `OsKeychain`.
- AGENTS.md §8.3 (Config), §9 (Seguridad), §15 (Checklist).
