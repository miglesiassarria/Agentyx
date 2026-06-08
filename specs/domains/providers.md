# LLM Providers

**Status**: draft
**Owner**: @miglesias
**Last update**: 2026-06-05
**Affects**: — (los providers son consumidos por `agent-loop.md`).
**Required by**: `agent-loop.md`, `features/F01-chat-streaming`,
`features/F04-file-diffs` (necesita al menos un provider activo),
`features/F05-settings` (config de providers), `agents.md` (los
agents consumen un provider/model del registry).

> Trait `Provider` + `ChatEvent` normalizado + **3 implementaciones
> v1** (Ollama, Groq, Minimax). El frontend y el agent loop solo
> conocen `ChatEvent`; los shapes específicos de cada provider se
> quedan dentro de su módulo. Ver
> [ADR-0008](../adr/0008-providers-v1-scope.md) para la decisión de
> scope.

## Goal

Proveer una **interfaz uniforme** sobre 3 servicios de inferencia
(Ollama, Groq, Minimax) que:
- Streamea via SSE (Groq, Minimax) o NDJSON (Ollama).
- Normaliza el output a un único enum `ChatEvent` consumible por el
  agent loop y el UI.
- Resuelve API keys desde env vars o keychain (nunca del código).
- Modela `Capability` por modelo (tools, vision, context length, …)
  para que el agent loop sepa qué tool schemas puede pasar.

## Non-goals

- ❌ Cómo el agent loop consume el stream. Ver
  [`agent-loop.md`](./agent-loop.md).
- ❌ Cómo el UI renderiza `ChatEvent`. Eso es frontend.
- ❌ Cómputo de costes o rate-limiting. v1: registro de tokens en
  `usage`; rate-limiting es responsabilidad del provider (o del
  network).
- ❌ Soporte de providers distintos a Ollama/Groq/Minimax en v1
  (OpenAI nativo, Anthropic nativo, Bedrock, Vertex, Cohere → v1.x /
  v2; ver [ADR-0008](../adr/0008-providers-v1-scope.md)).
- ❌ Fine-tuning, embeddings, image generation, TTS, STT. Fuera de
  scope de v1 (chat es el único caso).
- ❌ Persistir conversaciones cruzando providers. v1 asume que cada
  sesión usa un único provider.

## Glossary

Términos locales:

- **Provider**: implementación de `Provider` trait. Identificado por
  `ProviderId` (`"ollama" | "groq" | "minimax"`).
- **Model**: configuración de un modelo concreto, con `ModelId`
  (string) y `ModelCapabilities`.
- **ChatRequest**: input que el agent loop pasa al provider.
- **ChatStream**: output async, iterable de `ChatEvent`.
- **Capability**: flag booleano o número que indica qué puede hacer
  un modelo (`tools`, `vision`, `max_output_tokens`, `context_window`).
- **SSE**: Server-Sent Events. Estándar HTTP para streaming.
- **NDJSON**: Newline-Delimited JSON. Alternativa usada por Ollama.
- **Anthropic-compatible**: protocolo con `x-api-key`,
  `anthropic-version: 2023-06-01`, `POST {base_url}/v1/messages`,
  stream con eventos tipados (`message_start`, `content_block_start`,
  `content_block_delta`, `content_block_stop`, `message_delta`,
  `message_stop`).
- **OpenAI-compatible**: protocolo con `Authorization: Bearer`,
  `POST {base_url}/chat/completions`, stream SSE con deltas
  `{"choices": [{"delta": {"content": "..."}}]}`.

## State

### Config (en `~/.agentyx/config.toml`)

```toml
[providers.ollama]
base_url = "http://127.0.0.1:11434"
default_model = "llama3.1:8b"   # cualquiera que el user tenga

[providers.groq]
api_key = "env:GROQ_API_KEY"
base_url = "https://api.groq.com/openai/v1"
default_model = "llama-3.3-70b-versatile"

[providers.minimax]
api_key = "env:MINIMAX_API_KEY"
base_url = "https://api.minimax.io/anthropic"
default_model = "MiniMax-M3"

# Estado activo (se cambia en runtime desde la UI)
default_provider = "ollama"
default_model = "llama3.1:8b"
```

**Resolución de API key** (en este orden, primero que matchee):
1. `env:<NAME>` → variable de entorno.
2. Keychain del SO (entry `agentyx:provider:<id>:api_key`).
3. Si ninguna, el provider se considera **deshabilitado** con
   `error: "api_key_missing"`. No se intenta llamar.

> **Ollama no requiere API key** (es local). El provider `ollama`
> se considera habilitado si responde el `GET /api/tags`. Si la
> conexión falla (`connection refused`), se marca `enabled: false`
> con `error: "ollama_unreachable"`.

## ChatEvent (output normalizado)

```rust
pub enum ChatEvent {
    /// Inicio del mensaje. Se emite una vez por turno.
    MessageStart {
        message_id: MessageId,
        model: String,         // "llama3.1:8b" o "MiniMax-M3"
    },

    /// Delta de texto (streaming del content).
    ContentDelta {
        text: String,
    },

    /// El modelo pide ejecutar una tool.
    ToolUse {
        id: String,            // id de la tool call
        name: String,          // nombre (e.g. "read_file")
        args: serde_json::Value,
    },

    /// Resultado de una tool, devuelto al modelo. (El provider no
    /// emite esto; lo emite el agent loop en `chat.tool_result.v1`.
    /// Lo incluimos aquí para completitud cuando un provider
    /// espeja la conversación.)
    ToolResult {
        id: String,
        output: String,
        is_error: bool,
    },

    /// Fin del mensaje.
    MessageEnd {
        usage: Usage,
        finish_reason: FinishReason,
    },

    /// Error recuperable. El agent loop decide si abortar o reintentar.
    Error {
        code: String,
        message: String,
        retryable: bool,
    },
}

pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    /// Prompt caching. Soportado por Minimax (Anthropic-compatible).
    /// `None` para providers que no lo soportan.
    pub cache_read_tokens: Option<u32>,
    pub cache_write_tokens: Option<u32>,
}

pub enum FinishReason {
    Stop,                // el modelo terminó
    ToolUse,             // terminó pidiendo tools (v1: implícito en el stream, no se emite)
    Length,              // cortado por max_tokens
    ContentFilter,       // bloqueado por safety
    Error,               // error del provider
    Aborted,             // el agent loop abortó
}
```

## Provider trait

```rust
#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;

    /// Lista de modelos configurados. Puede ser dinámica cuando el
    /// provider expone catálogo (Ollama `/api/tags`, MiniMax
    /// `/v1/models`) o estática como fallback.
    async fn list_models(&self) -> Result<Vec<ModelInfo>, AppError>;

    /// Capabilities de un modelo concreto. Si no se conoce, se asume
    /// `tools: false, vision: false, context: 4096, output: 4096`.
    fn capabilities(&self, model_id: &str) -> ModelCapabilities;

    /// Streaming de un turno. **El caller itera el stream y procesa
    /// cada `ChatEvent`**. Si el provider no soporta streaming,
    /// emite un único `MessageEnd` con el content completo en
    /// `ContentDelta` acumulado.
    async fn chat(
        &self,
        req: ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ChatEvent, AppError>> + Send>>, AppError>;
}
```

## ChatRequest

```rust
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolSchema>,          // de ToolRegistry (filtrado por agent)
    pub tool_choice: ToolChoice,         // Auto | Any | None | Specific(String)
    pub max_output_tokens: Option<u32>,
    pub temperature: Option<f32>,        // 0.0 - 1.0
    pub stream: bool,                    // siempre true en v1
    pub metadata: RequestMetadata,       // run_id, session_id, workspace_id, agent_id
}

pub enum ChatMessage {
    System { content: String },
    User { content: String },
    Assistant { content: String, tool_calls: Vec<ToolCall> },
    ToolResult { tool_use_id: String, content: String, is_error: bool },
}
```

## Implementaciones v1

### Ollama (local)

- **Endpoint**: `POST {base_url}/api/chat` con `stream: true`.
- **Headers**: ninguno (local).
- **Auth**: ninguna.
- **Tools**: formato OpenAI-style soportado desde Ollama 0.5+;
  versiones más antiguas no lo soportan (se detecta por capabilities).
- **Streaming**: NDJSON (un JSON por línea), no SSE. Cada línea es
  `{"message": {"role": "assistant", "content": "..."}, "done": false, ...}`.
  La última tiene `"done": true`.
- **Modelos**: dinámicos via `GET /api/tags`. `list_models` los
  consulta al inicio y cachea con TTL de 5 min.
- **Capabilities por modelo**: heurística por nombre
  (modelos con `-tool` o `-instruct` recientes → `tools: true`;
  resto → `tools: false`). Si el modelo no se reconoce, default
  seguro = `tools: false`.
- **Normalización a `ChatEvent`**: trivial, NDJSON es fácil de iterar.
- **Caso especial**: si Ollama no está corriendo (`connection
  refused`), el provider se marca `enabled: false` con
  `error: "ollama_unreachable"`. La UI muestra "Ollama: not running"
  en el picker.

### Groq (cloud, OpenAI-compatible)

- **Endpoint**: `POST {base_url}/chat/completions` con
  `stream: true`.
- **Headers**: `Authorization: Bearer {api_key}`.
- **Auth**: API key resuelta de `env:GROQ_API_KEY` o keychain.
- **Tools**: formato OpenAI (`tools: [{ type: "function", function:
  { name, description, parameters } }]`).
- **Streaming**: SSE; cada evento es
  `data: {"choices": [{"delta": {"content": "..."}}]}\n\n`.
  El último es `data: [DONE]`.
- **Tool calls en streaming**: vienen en `delta.tool_calls[]` con
  `index` y campos parciales (se acumulan).
- **Normalización a `ChatEvent`**: trivial, el shape ya es casi
  `ChatEvent`. Reutiliza el normalizador OpenAI-compatible.
- **Modelos hardcoded** (lista inicial, se amplía en v1.x vía
  `config.toml`):
  - `llama-3.3-70b-versatile` (tools: true, vision: false,
    context: 128k, output: 32k)
  - `llama-3.1-8b-instant` (tools: true, vision: false,
    context: 128k, output: 8k)
  - `mixtral-8x7b-32768` (tools: true, vision: false,
    context: 32k, output: 4k)

### Minimax (cloud, Anthropic-compatible)

- **Endpoint**: `POST {base_url}/v1/messages` con `stream: true`.
- **Base URL oficial**: `https://api.minimax.io/anthropic`. Las
  configuraciones antiguas con `https://api.minimax.io/v1` se
  normalizan internamente al endpoint Anthropic para no romper
  instalaciones existentes.
- **Headers**: `Authorization: Bearer {api_key}`,
  `anthropic-version: 2023-06-01`.
- **Auth**: API key resuelta de `env:MINIMAX_API_KEY` o keychain.
- **Tools**: formato Anthropic (`tools: [{ name, description,
  input_schema }]`).
- **Streaming**: SSE con eventos tipados (`message_start`,
  `content_block_start`, `content_block_delta`, `content_block_stop`,
  `message_delta`, `message_stop`).
- **Normalización**: el stream tiene **bloques** (text, tool_use).
  Vamos acumulando cada bloque hasta `content_block_stop` y entonces
  emitimos el `ChatEvent` correspondiente. Más complejo que OpenAI.
- **Modelos**: dinámicos via `GET https://api.minimax.io/v1/models`
  con `Authorization: Bearer {api_key}`. Para `base_url` custom se
  deriva el root del provider: `.../anthropic` y `.../anthropic/v1`
  consultan `.../v1/models`.
- **Fallback estático**: si el catálogo no está disponible por error
  de red, 404/5xx o respuesta vacía, `list_models` devuelve esta lista
  local; si el error es 401/403 se propaga para no ocultar una key
  inválida:
  - `MiniMax-M3` (tools: true, vision: false en v0.1 UI, context:
    hasta 1M, output: 8k, soporta prompt caching)
  - `MiniMax-M2.7` (tools: true, vision: false, context: 204.8k,
    output: 8k, soporta prompt caching)
  - `MiniMax-M2.5` (tools: true, vision: false, context: 204.8k,
    output: 8k, soporta prompt caching)
  - `MiniMax-M2.1` y variantes highspeed (tools: true, vision:
    false, context: 204.8k, output: 8k)
- **API key**: se obtiene en `https://platform.minimax.io/login`.

## Operations (del dominio)

### `Providers::load(config_path) -> Result<Providers, AppError>`

Carga la config global, instancia los 3 providers y resuelve sus
API keys (Ollama no requiere key, solo check de reachability). Los
providers sin key se marcan `enabled: false` (no se incluyen en
`list_active`).

### `Providers::list_active() -> Vec<&'static dyn Provider>`

Devuelve los providers con API key resuelta y Ollama reachable.

### `Providers::get(id) -> Option<&'static dyn Provider>`

Lookup por id.

### `Providers::set_default(id, model) -> ()`

Actualiza `default_provider` y `default_model` en
`~/.agentyx/config.toml` y recarga.

## Contracts

### Tauri commands

| Command | Notas |
|---|---|
| `providers_list() -> ProviderInfo[]` | Solo los enabled. |
| `providers_list_models(provider_id) -> ModelInfo[]` | Dinámicos para Ollama y MiniMax; hardcoded para Groq y fallback MiniMax. |
| `providers_get_capabilities(provider_id, model_id) -> ModelCapabilities` | |
| `providers_set_default(id, model) -> ()` | |
| `providers_set_api_key(id, env_or_keychain_ref) -> ()` | UI para meter keys. |
| `providers_test(id) -> TestResult` | Ping al provider con un mensaje trivial. |

### HTTP endpoints

`GET  /api/v1/providers` → `ProviderInfo[]`
`GET  /api/v1/providers/:id/models` → `ModelInfo[]`
`GET  /api/v1/providers/:id/models/:model/capabilities` → `ModelCapabilities`
`POST /api/v1/providers/default` (body: `{ id, model }`) → `{}`
`POST /api/v1/providers/:id/api_key` (body: `{ env: "GROQ_API_KEY" }` o `{ keychain: true }`) → `{}`
`POST /api/v1/providers/:id/test` → `TestResult`

### Eventos

Los providers **no** emiten eventos propios. Su output es consumido
por el agent loop, que emite `chat.*.v1` (ver
[`agent-loop.md`](./agent-loop.md)).

## Edge cases

1. **API key inválida / 401**: el provider emite
   `ChatEvent::Error { code: "auth_failed", retryable: false }` y
   cierra el stream. El agent loop aborta el run con
   `finish_reason: error`.
2. **Rate limit (429)**: `ChatEvent::Error { code: "rate_limited",
   retryable: true }`. v1 no reintenta automáticamente; el usuario
   puede `session_send` de nuevo.
3. **Stream cortado por el servidor** (TCP reset, timeout): el
   reader detecta EOF inesperado, emite
   `ChatEvent::Error { code: "stream_interrupted", retryable: true }`
   y cierra.
4. **Tool call con `args` parcialmente streamados**: el provider va
   enviando `delta.tool_calls` con `args` incremental (string). El
   provider-specific normalizador **acumula** y solo emite
   `ChatEvent::ToolUse` cuando el bloque está completo.
5. **Modelo desconocido** (id mal escrito): el provider devuelve
   `provider_unavailable`. La UI debe validar al seleccionar.
6. **Ollama no está corriendo** (`connection refused`): el provider
   se marca `enabled: false` con `error: "ollama_unreachable"`. La
   UI muestra "Ollama: not running" en el picker.
7. **Minimax prompt caching**: cuando se usa, `cache_read_tokens`
   y `cache_write_tokens` se incluyen en `Usage`. La persistencia
   en `usage` debe aceptar `NULL` para esos campos.
8. **Ollama con modelo que no soporta tools**: `capabilities(model)`
   retorna `tools: false`. El agent loop no le pasa `tools` y el
   modelo responde solo con texto.
9. **Stream paralelo en dos sesiones**: cada `chat()` retorna un
   `Stream` independiente, con su propio connection pool. No
   comparten estado.
10. **Provider con `base_url` que requiere headers custom**:
    en v1, no soportado vía UI. Workaround: editar `config.toml` a
    mano. v1.x expone `[providers.<id>.headers]` en la UI.

## Acceptance criteria

Cada AC → test con nombre derivado `ac<n>_<short>`.

- [ ] AC1: `load` con config que tiene las 3 secciones carga los 3
  providers. **Test**: `ac1_load_creates_three_providers`.
- [ ] AC2: `load` con Groq sin `GROQ_API_KEY` lo marca
  `enabled: false` con `error: "api_key_missing"`. **Test**:
  `ac2_missing_key_marks_disabled`.
- [ ] AC3: `chat` con Ollama mockeado (servidor fake con NDJSON)
  emite `MessageStart → ContentDelta* → MessageEnd` con
  `finish_reason: "stop"`. **Test**:
  `ac3_ollama_ndjson_normalizes_to_chat_events`.
- [ ] AC4: `chat` con Groq mockeado (servidor fake con SSE OpenAI-style)
  emite los mismos eventos tras acumular los deltas. **Test**:
  `ac4_groq_sse_normalizes_to_chat_events`.
- [ ] AC5: `chat` con Minimax mockeado (SSE Anthropic-style) emite
  los mismos eventos tras acumular los content blocks. **Test**:
  `ac5_minimax_stream_normalizes_blocks`.
- [ ] AC6: cuando Groq recibe 401, emite
  `ChatEvent::Error { code: "auth_failed", retryable: false }` y
  cierra el stream. **Test**: `ac6_auth_failed_emits_error`.
- [ ] AC7: cuando el stream se corta, emite
  `ChatEvent::Error { code: "stream_interrupted", retryable: true }`.
  **Test**: `ac7_stream_interrupted_emits_error`.
- [ ] AC8: tool call con `args` streamado en deltas se acumula y
  solo se emite `ChatEvent::ToolUse` con `args` completos. **Test**:
  `ac8_tool_call_args_accumulated`.
- [ ] AC9: dos `chat()` concurrentes en providers distintos no
  comparten connections. **Test**:
  `ac9_concurrent_chats_isolated`.
- [ ] AC10: `set_default` persiste y `load` posterior lo refleja.
  **Test**: `ac10_set_default_persists`.
- [ ] AC11: `test` (ping) retorna `success: true` con el provider
  reachable; `success: false` con `error` legible si no. **Test**:
  `ac11_test_returns_reachable_status`.
- [ ] AC12: Ollama con `connection refused` se marca
  `enabled: false` con `error: "ollama_unreachable"`. **Test**:
  `ac12_ollama_unreachable_marks_disabled`.
- [ ] AC13: Minimax con prompt caching habilitado emite
  `Usage { cache_read_tokens: Some(N), cache_write_tokens: Some(M) }`
  en el `MessageEnd`. **Test**:
  `ac13_minimax_cache_tokens_in_usage`.
- [ ] AC14: Groq con `tools: []` (modelo sin tools según capabilities)
  no envía el campo `tools` en el request HTTP. **Test**:
  `ac14_groq_omits_tools_field_when_disabled`.

## Discovered bugs (post-approval)

| ID | Date | Category | Resolved in | Notes |
|---|---|---|---|---|
| _ninguno aún_ | | | | |

## Open questions

- **Q1**: ¿Soporte de **image input** (multimodal) en v1? → **Propuesta
  v1**: el `ChatMessage::User` no soporta `content: Vec<ContentBlock>`.
  Se añade en v2 si hay demanda. `capabilities(model).vision = true`
  ya está en el shape, pero ningún modelo de los 3 v1 lo expone
  (`vision: false` en todos los hardcoded).
- **Q2**: ¿Prompt caching automático en Minimax? → **Propuesta
  v1**: lo activamos por defecto si el modelo lo soporta. El
  `usage` ya lo recoge. UI puede mostrar "X tokens cacheados".
- **Q3**: ¿Function calling de Groq vs tools de Minimax — vale
  la pena un wrapper? → **Propuesta v1**: cada provider traduce
  internamente. El agent loop solo conoce `ToolSchema` (un shape
  neutro). v2 quizá un AST compartido.
- **Q4**: ¿Ollama descubre modelos dinámicamente, pero qué pasa si
  el user añade un modelo custom (e.g. fine-tune)? → **Propuesta
  v1**: solo los que Ollama expone. Custom GGUF en v2 con
  `ollama create`.
- **Q5**: ¿Reintroducir un `openai_compat` genérico en v1.x para
  Together/OpenRouter/LM Studio? → Sí, previsto en
  [ADR-0008](../adr/0008-providers-v1-scope.md). Sin rewrite
  mayor: el código de normalización es el mismo que usa Groq.

## References

- [`../architecture.md`](../architecture.md) — provider en el loop.
- [`agent-loop.md`](./agent-loop.md) — el consumidor de `ChatEvent`.
- [`tools.md`](./tools.md) — fuente de `ToolSchema`.
- [`../ipc.md`](../ipc.md) — shape de los Tauri commands.
- [`../glossary.md`](../glossary.md) — `ChatEvent`, `Provider`, `LLM Provider`.
- [`../adr/0008-providers-v1-scope.md`](../adr/0008-providers-v1-scope.md) —
  justificación de los 3 providers.
- OpenCode como referencia de implementación (Minimax token plan,
  Groq).
