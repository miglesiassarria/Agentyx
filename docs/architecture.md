# Architecture

> This file is a quick-reference index. The authoritative
> architecture is in [`../specs/architecture.md`](../specs/architecture.md).

## High-level diagram

```
┌────────────────────────────────────────────────────┐
│  Tauri Webview                                     │
│  ┌──────────────────────────────────────────┐     │
│  │  UI (Svelte 5)                           │     │
│  │  src/lib/ipc.ts ─► @tauri-apps/api       │     │
│  └──────────────────────────────────────────┘     │
│            ▲                                       │
│            │ invoke(command) + listen(event)       │
│            ▼                                       │
├────────────┼───────────────────────────────────────┤
│  Rust process                                       │
│  ┌──────────────────────────────────────────┐     │
│  │  agentyx-app  (Tauri 2 entrypoint)       │     │
│  │  • commands::*  (#[tauri::command])      │     │
│  │  • events::EventBus  → window.emit(...)  │     │
│  │  • state::AppState  (Arc<RwLock<...>>)   │     │
│  └──────────────────────────────────────────┘     │
│            │                                       │
│            ▼                                       │
│  ┌──────────────────────────────────────────┐     │
│  │  agentyx-core  (pure Rust, no Tauri)     │     │
│  │  • agent::loop        (ReAct + tools)    │     │
│  │  • agents::registry   (Primary|Subagent) │     │
│  │  • llm::Provider      (Ollama/Groq/Mini) │     │
│  │  • tools::*           (read|write|edit)  │     │
│  │  • storage::*         (SQLite, rusqlite) │     │
│  │  • journal::*         (append-only)      │     │
│  │  • permissions::*     (matrix + gate)    │     │
│  │  • config::*          (TOML + SecretRef) │     │
│  │  • workspace::*       (root + extras)    │     │
│  │  • session::*         (sessions, runs)   │     │
│  │  • pty::*             (portable-pty)     │     │
│  └──────────────────────────────────────────┘     │
└────────────────────────────────────────────────────┘
```

## Key principles

1. **Business logic in Rust** — UI is presentation only.
2. **IPC typed and explicit** — no string magic, every command/event
   has a Rust struct + TS type.
3. **Streaming by default** — LLM, PTY, and logs stream events
   (`chat.*.v1`, `pty.*.v1`, `agent.*.v1`).
4. **Sandbox by workspace** — each workspace is `root_path ∪ extra_paths`.
5. **Reversible and reproducible** — every action lands in the journal
   (append-only SQLite table).
6. **Multi-agent from day 1** — the agent loop models
   `Primary | Subagent | Hidden` even though v0.1 only has 2+1 built-ins.
7. **Local-first** — no telemetry, no sync, only the LLM provider
   calls go over the network.

See [`../specs/architecture.md`](../specs/architecture.md) for the
full design, error handling, observability, and security model.
