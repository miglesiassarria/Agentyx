# Providers

> Quick-reference index. The authoritative provider spec is in
> [`../specs/domains/providers.md`](../specs/domains/providers.md).

## Supported in v0.1

| Provider | Kind | Endpoint | Auth |
|---|---|---|---|
| **Ollama** | Local, default | `http://127.0.0.1:11434` (configurable) | None (optional API key) |
| **Groq** | Cloud, OpenAI-compatible | `https://api.groq.com/openai/v1` | `GROQ_API_KEY` env or keychain |
| **Minimax** | Cloud, Anthropic-compatible | `https://api.minimax.io/v1` | `MINIMAX_API_KEY` env or keychain |

See [ADR-0008](../specs/adr/0008-providers-v1-scope.md) for the
scope decision (OpenAI native, Anthropic native, and a generic
`openai_compat` are deferred to v1.x).

## Adding a new provider (v1.x or later)

1. Implement the `agentyx_core::llm::Provider` trait.
2. Add the kind to `ProviderKind` enum in `crates/agentyx-core/src/llm/`.
3. Register in `ProviderRegistry::from_config`.
4. Update `ui/src/lib/components/settings/ProviderCard.svelte`
   and `AddProviderDialog.svelte` if UI needs to know.
5. Add at least one integration test with `wiremock`.
6. Add the provider to `specs/domains/providers.md` and update
   ADR if scope is changing.

## Streaming

All providers normalize to `ChatEvent` (one enum, one wire shape):

```rust
pub enum ChatEvent {
    MessageStart { id: String, model: String },
    ContentDelta { text: String },
    ToolUse { id: String, name: String, args: serde_json::Value },
    ToolResult { id: String, output: String, is_error: bool },
    MessageEnd { usage: Usage, finish_reason: FinishReason },
    Error { code: String, message: String },
}
```

The frontend only knows `ChatEvent`. Provider-specific shapes are
hidden inside the Rust `Provider` implementation.

## Rate limiting

Each provider tracks its own rate limit (tokens/min, req/min) and
returns `ChatEvent::Error { code: "rate_limited", retryable: true }`
when exceeded. The agent loop retries once with 1s backoff; if the
second attempt fails, the run ends with `chat.run.error.v1`.
