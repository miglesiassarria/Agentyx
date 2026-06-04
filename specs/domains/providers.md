# LLM Providers

**Status**: approved
**Owner**: @miglesias
**Last update**: 2026-06-04
**Affects**: — (los providers son consumidos por `agent-loop.md`).
**Required by**: `agent-loop.md`, `features/F01-chat-streaming`,
`features/F04-file-diffs` (necesita al menos un provider activo),
`features/F05-settings` (config de providers).

> Trait `Provider` + `ChatEvent` normalizado + 4 implementaciones v1
> (OpenAI, Anthropic, Ollama, OpenAI-compatible). El frontend y el
> agent loop solo conocen `ChatEvent`; los shapes específicos de cada
> provider se quedan dentro de su módulo.

## Goal

Proveer una **interfaz uniforme** sobre 4 servicios de inferencia
(OpenAI, Anthropic, Ollama, OpenAI-compatible genérico) que:
- Streamea via SSE (o NDJSON para Ollama si SSE no está disponible).
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
- ❌ Soporte de providers que no sean OpenAI-compatibles o
  Anthropic-native (Bedrock, Vertex, Cohere, …). v2.
- ❌ Fine-tuning, embeddings, image generation, TTS, STT. Fuera de
  scope de v1 (chat es el único caso).
- ❌ Persistir conversaciones cruzando providers. v1 asume que cada
  sesión usa un único provider.

## Glossary

Términos locales:

- **Provider**: implementación de `Provider` trait. Identificado por
  `ProviderId` (`"openai" | "anthropic" | "ollama" | "openai_compat"`).
- **Model**: configuración de un modelo concreto, con `ModelId`
  (string) y `ModelCapabilities`.
- **ChatRequest**: input que el agent loop pasa al provider.
- **ChatStream**: output async, iterable de `ChatEvent`.
- **Capability**: flag booleano o número que indica qué puede hacer
  un modelo (`tools`, `vision`, `max_output_tokens`, `context_window`).
- **SSE**: Server-Sent Events. Estándar HTTP para streaming.
- **NDJSON**: Newline-Delimited JSON. Alternativa usada por Ollama
  cuando no expone SSE.

## State

### Config (en `~/.agentyx/config.toml`)

```toml
[providers.openai]
api_key = "env:OPENAI_API_KEY"        # o vacío para keychain lookup
base_url = "https://api.openai.com/v1"
default_model = "gpt-4o"

[providers.anthropic]
api_key = "env:ANTHROPIC_API_KEY"
base_url = "https://api.anthropic.com"
default_model = "claude-3-5-sonnet-latest"

[providers.ollama]
base_url = "http://127.0.0.1:11434"
default_model = "llama3.1:8b"

# Cualquier endpoint OpenAI-compatible: Together, Groq, OpenRouter, …
[providers.openai_compat]
base_url = "https://api.together.xyz/v1"
api_key = "env:TOGETHER_API_KEY"
default_model = "meta-llama/Llama-3-70b-chat-hf"

# Estado activo (se cambia en runtime desde la UI)
default_provider = "ollama"
default_model = "llama3.1:8b"
```

**Resolución de API key** (en este orden, primero que matchee):
1. `env:<NAME>` → variable de entorno.
2. Keychain del SO (entry `agentyx:provider:<id>:api_key`).
3. Si ninguna, el provider se considera **deshabilitado** con
   `error: "api_key_missing"`. No se intenta llamar.

## ChatEvent (output normalizado)

```rust
pub enum ChatEvent {
    /// Inicio del mensaje. Se emite una vez por turno.
    MessageStart {
        message_id: MessageId,
        model: String,         // "gpt-4o-2024-08-06" o "llama3.1:8b"
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
    pub cache_read_tokens: Option<u32>,   // Anthropic prompt caching
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

    /// Lista de modelos configurados. Puede ser estática (hardcoded)
    /// o dinámica (e.g. Ollama expone `/api/tags`).
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
    pub tools: Vec<ToolSchema>,          // de ToolRegistry
    pub tool_choice: ToolChoice,         // Auto | Any | None | Specific(String)
    pub max_output_tokens: Option<u32>,
    pub temperature: Option<f32>,        // 0.0 - 1.0
    pub stream: bool,                    // siempre true en v1
    pub metadata: RequestMetadata,       // run_id, session_id, workspace_id
}

pub enum ChatMessage {
    System { content: String },
    User { content: String },
    Assistant { content: String, tool_calls: Vec<ToolCall> },
    ToolResult { tool_use_id: String, content: String, is_error: bool },
}
```

## Implementaciones v1

### OpenAI (API oficial)

- **Endpoint**: `POST {base_url}/chat/completions` con
  `stream: true`.
- **Headers**: `Authorization: Bearer {api_key}`.
- **Tools**: formato OpenAI (`tools: [{ type: "function", function: { name, description, parameters } }]`).
- **Streaming**: SSE; cada evento es
  `data: {"choices": [{"delta": {"content": "..."}}]}\n\n`.
  El último es `data: [DONE]`.
- **Tool calls en streaming**: vienen en `delta.tool_calls[]` con
  `index` y campos parciales (se acumulan).
- **Normalización a `ChatEvent`**: trivial, el shape ya es casi
  `ChatEvent`.

### Anthropic (Claude)

- **Endpoint**: `POST {base_url}/v1/messages` con `stream: true`.
- **Headers**: `x-api-key: {api_key}`, `anthropic-version: 2023-06-01`.
- **Tools**: formato Anthropic (`tools: [{ name, description, input_schema }]`).
- **Streaming**: SSE con eventos tipados
  (`message_start`, `content_block_start`, `content_block_delta`,
  `content_block_stop`, `message_delta`, `message_stop`).
- **Normalización**: el stream tiene **bloques** (text, tool_use).
  Vamos acumulando cada bloque hasta `content_block_stop` y entonces
  emitimos el `ChatEvent` correspondiente. Más complejo que OpenAI.

### Ollama (local)

- **Endpoint**: `POST {base_url}/api/chat` con `stream: true`.
- **Headers**: ninguno (local).
- **Tools**: formato OpenAI-style soportado desde Ollama 0.5+;
  versiones más antiguas no lo soportan (se detecta por capabilities).
- **Streaming**: NDJSON (un JSON por línea), no SSE. Cada línea es
  `{"message": {"role": "assistant", "content": "..."}, "done": false, ...}`.
  La última tiene `"done": true`.
- **Modelos**: dinámicos via `GET /api/tags`. `list_models` los
  consulta al inicio y cachea con TTL de 5 min.
- **Normalización**: trivial, NDJSON es fácil de iterar.

### OpenAI-compatible genérico

- Mismo shape que OpenAI. La única diferencia es el `base_url`.
- Usado para Together, Groq, OpenRouter, LM Studio, etc.
- La detección de capabilities es por nombre de modelo (hardcoded
  lookup) o por `default_model` del config.

## Operations (del dominio)

### `Providers::load(config_path) -> Result<Providers, AppError>`

Carga la config global, instancia los 4 providers y resuelve sus
API keys. Providers sin key se marcan `enabled: false` (no se
incluyen en `list_active`).

### `Providers::list_active() -> Vec<&'static dyn Provider>`

Devuelve los providers con API key resuelta.

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
| `providers_list_models(provider_id) -> ModelInfo[]` | |
| `providers_get_capabilities(provider_id, model_id) -> ModelCapabilities` | |
| `providers_set_default(id, model) -> ()` | |
| `providers_set_api_key(id, env_or_keychain_ref) -> ()` | UI para meter keys. |
| `providers_test(id) -> TestResult` | Ping al provider con un mensaje trivial. |

### HTTP endpoints

`GET  /api/v1/providers` → `ProviderInfo[]`
`GET  /api/v1/providers/:id/models` → `ModelInfo[]`
`GET  /api/v1/providers/:id/models/:model/capabilities` → `ModelCapabilities`
`POST /api/v1/providers/default` (body: `{ id, model }`) → `{}`
`POST /api/v1/providers/:id/api_key` (body: `{ env: "OPENAI_API_KEY" }` o `{ keychain: true }`) → `{}`
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
7. **Anthropic prompt caching**: cuando se usa, `cache_read_tokens`
   y `cache_write_tokens` se incluyen en `Usage`. La persistencia
   en `usage` debe aceptar `NULL` para esos campos.
8. **Ollama con modelo que no soporta tools**: `capabilities(model)`
   retorna `tools: false`. El agent loop no le pasa `tools` y el
   modelo responde solo con texto.
9. **Stream paralelo en dos sesiones**: cada `chat()` retorna un
   `Stream` independiente, con su propio connection pool. No
   comparten estado.
10. **Provider con `base_url` que requiere headers custom** (e.g.
    OpenRouter requiere `HTTP-Referer`): soportado vía
    `[providers.openai_compat.headers]` en config (futuro, v1 no lo
    expone en UI).

## Acceptance criteria

Cada AC → test con nombre derivado `ac<n>_<short>`.

- [ ] AC1: `load` con config que tiene las 4 secciones carga los 4
  providers. **Test**: `ac1_load_creates_four_providers`.
- [ ] AC2: `load` con OpenAI sin `OPENAI_API_KEY` lo marca
  `enabled: false` con `error: "api_key_missing"`. **Test**:
  `ac2_missing_key_marks_disabled`.
- [ ] AC3: `chat` con OpenAI mockeado (servidor fake con SSE)
  emite `MessageStart → ContentDelta* → MessageEnd` con
  `finish_reason: "stop"`. **Test**:
  `ac3_openai_stream_normalizes_to_chat_events`.
- [ ] AC4: `chat` con Anthropic mockeado emite los mismos eventos
  tras acumular los content blocks. **Test**:
  `ac4_anthropic_stream_normalizes_blocks`.
- [ ] AC5: `chat` con Ollama mockeado (NDJSON) emite
  `MessageStart → ContentDelta* → MessageEnd`. **Test**:
  `ac5_ollama_ndjson_normalizes_to_chat_events`.
- [ ] AC6: `chat` con OpenAI-compatible (Together mockeado) emite
  los mismos eventos que OpenAI. **Test**:
  `ac6_openai_compat_uses_openai_normalizer`.
- [ ] AC7: cuando el provider recibe 401, emite
  `ChatEvent::Error { code: "auth_failed", retryable: false }` y
  cierra el stream. **Test**: `ac7_auth_failed_emits_error`.
- [ ] AC8: cuando el stream se corta, emite
  `ChatEvent::Error { code: "stream_interrupted", retryable: true }`.
  **Test**: `ac8_stream_interrupted_emits_error`.
- [ ] AC9: tool call con `args` streamado en deltas se acumula y
  solo se emite `ChatEvent::ToolUse` con `args` completos. **Test**:
  `ac9_tool_call_args_accumulated`.
- [ ] AC10: dos `chat()` concurrentes en providers distintos no
  comparten connections. **Test**:
  `ac10_concurrent_chats_isolated`.
- [ ] AC11: `set_default` persiste y `load` posterior lo refleja.
  **Test**: `ac11_set_default_persists`.
- [ ] AC12: `test` (ping) retorna `success: true` con el provider
  reachable; `success: false` con `error` legible si no. **Test**:
  `ac12_test_returns_reachable_status`.

## Discovered bugs (post-approval)

| ID | Date | Category | Resolved in | Notes |
|---|---|---|---|---|
| _ninguno aún_ | | | | |

## Open questions

- **Q1**: ¿Soporte de **image input** (multimodal) en v1? → **Propuesta
  v1**: el `ChatMessage::User` no soporta `content: Vec<ContentBlock>`.
  Se añade en v2 si hay demanda. `capabilities(model).vision = true`
  ya está en el shape.
- **Q2**: ¿Prompt caching automático en Anthropic? → **Propuesta
  v1**: lo activamos por defecto si el modelo lo soporta. El
  `usage` ya lo recoge. UI puede mostrar "X tokens cacheados".
- **Q3**: ¿Function calling de OpenAI vs tools de Anthropic — vale
  la pena un wrapper? → **Propuesta v1**: cada provider traduce
  internamente. El agent loop solo conoce `ToolSchema` (un shape
  neutro). v2 quizá un AST compartido.
- **Q4**: ¿Ollama descubre modelos dinámicamente, pero qué pasa si
  el user añade un modelo custom (e.g. fine-tune)? → **Propuesta
  v1**: solo los que Ollama expone. Custom GGUF en v2 con
  `ollama create`.

## References

- [`../architecture.md`](../architecture.md) — provider en el loop.
- [`agent-loop.md`](./agent-loop.md) — el consumidor de `ChatEvent`.
- [`tools.md`](./tools.md) — fuente de `ToolSchema`.
- [`../ipc.md`](../ipc.md) — shape de los Tauri commands.
- [`../glossary.md`](../glossary.md) — `ChatEvent`, `Provider`, `LLM Provider`.
