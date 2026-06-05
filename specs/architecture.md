# Architecture

**Status**: approved
**Owner**: @miglesias
**Last update**: 2026-06-05

> Diagrama de cajas y flujo de datos. Los detalles de cada caja viven
> en su spec de dominio correspondiente.

---

## Vista de procesos (un solo binario)

```
┌─────────────────────────────────────────────────────────────────────┐
│                       Proceso Agentyx (Rust)                        │
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │                    agentyx-app (binario Tauri)                │  │
│  │                                                              │  │
│  │   ┌──────────────────┐         ┌────────────────────────┐    │  │
│  │   │  Tauri commands  │         │  HTTP server (axum)    │    │  │
│  │   │  (#[tauri::cmd]) │         │  - REST + SSE          │    │  │
│  │   │  Request/Resp.   │         │  - Sirve ui/dist/      │    │  │
│  │   └────────┬─────────┘         │  - Auth bearer token   │    │  │
│  │            │                   └──────────┬─────────────┘    │  │
│  │            │                              │                  │  │
│  │            ▼                              ▼                  │  │
│  │   ┌──────────────────────────────────────────────────────┐  │  │
│  │   │              agentyx-core (librería pura)              │  │  │
│  │   │                                                       │  │  │
│  │   │   agent::loop ──► llm::Provider ──► tools::*          │  │  │
│  │   │   (consume              ▲              │              │  │  │
│  │   │    agents::*)           │              ▼              │  │  │
│  │   │        │                │     workspace::* / pty::*  │  │  │
│  │   │        ▼                │     permissions::*          │  │  │
│  │   │   journal::*            │     config::*               │  │  │
│  │   │   storage::*            │                             │  │  │
│  │   └──────────────────────────────────────────────────────┘  │  │
│  │                                                              │  │
│  │   ┌──────────────────────────────────────────────────────┐  │  │
│  │   │              Tauri runtime + Webview                  │  │  │
│  │   │  (WebKit/WKWebView en macOS, WebView2 en Windows,    │  │  │
│  │   │   WebKitGTK en Linux)                                 │  │  │
│  │   │  Carga ui/dist/ desde el filesystem o desde el        │  │  │
│  │   │  HTTP server local                                    │  │  │
│  │   └──────────────────────────────────────────────────────┘  │  │
│  │                                                              │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │          Subprocesos y procesos externos (spawn)             │  │
│  │                                                              │  │
│  │  - .venv/bin/python (vía PTY)                               │  │
│  │  - uv, pip, git, shell commands                             │  │
│  │  - file watcher (inotify/FSEvents/ReadDirectoryChangesW)    │  │
│  │                                                              │  │
│  └──────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘

            ▲ Tauri IPC (invoke + listen)            ▲ HTTP/SSE
            │                                        │
   ┌────────┴────────┐                      ┌────────┴────────┐
   │   Webview       │                      │  Navegador      │
   │   nativo        │                      │  (mismo proceso │
   │   (Svelte UI)   │                      │   o LAN)        │
   └─────────────────┘                      └─────────────────┘
```

## Cajas

### `agentyx-core` — librería pura
Único dueño del dominio. **No depende de Tauri**. Testeable sin GUI.
Ver [AGENTS.md](../AGENTS.md) §3.1.

Submódulos:
- `agent/` — loop ReAct, definición de tool, prompts.
- `agents/` — `AgentSpec`, `AgentRegistry`, prompts built-in y carga de
  custom. Modela primary/subagent/hidden. Ver [agents.md](./agents.md).
- `llm/` — trait `Provider`, implementaciones (Ollama, Groq, Minimax),
  `ChatEvent` normalizado, SSE.
- `tools/` — `read_file`, `write_file`, `edit_file`, `search`, `shell`,
  `python_run`, `list_dir`, `apply_patch`. Path sandboxing = `root ∪ extra_paths`.
- `workspace/` — detección, lifecycle, `root + extra_paths`, `.venv` (opt-in).
- `pty/` — wrapper sobre `portable-pty`.
- `storage/` — SQLite, migraciones, repos.
- `journal/` — log append-only.
- `permissions/` — matriz y decisiones (root + extras).
- `config/` — carga/validación de TOML.
- `server/` — axum router, auth, SSE, estáticos.
- `error.rs` — `AppError` + `From` impls.
- `ids.rs` — ULIDs.

### `agentyx-app` — binario Tauri
Único crate con `tauri = ...`. Contiene:
- `commands/` — `#[tauri::command]` handlers (uno por scope).
- `events.rs` — canales internos → `window.emit`.
- `state.rs` — `AppState` (Arc<Mutex<...>>) compartido entre commands.
- `ipc.rs` — tipos compartidos con el frontend.
- `window.rs`, `menu.rs`, `updater.rs`, `deep_link.rs`.
- **Spawn del server HTTP** (axum) en el arranque.

### `agentyx-sdk` — SDK Rust (futuro, no v1)
Pensado para que terceros integren Agentyx como librería.

### `ui/` — frontend Svelte 5
- Misma build, dos modos de carga:
  - **Tauri webview**: `tauri://` con IPC nativo (`invoke` + `listen`).
  - **Navegador**: HTTP al server local con `fetch` + `EventSource`.
- `lib/ipc.ts` abstrae el transporte: misma API para la UI, distinto
  adapter por entorno.
- Componentes: ChatPanel, MessageList, Composer, DiffView (CodeMirror),
  Editor, FileTree, PdfViewer (lazy), DocxViewer (lazy), WebArtifact
  (iframe sandbox), Dashboard (uPlot), PtyTerminal (xterm.js opcional),
  VenvStatus, ProviderPicker.

## Flujo de datos (chat con LLM)

```
┌───────┐  user msg   ┌─────────┐  invoke   ┌─────────────┐
│  UI   │ ──────────► │ Tauri   │ ────────► │ commands/   │
│ Svelte│             │ runtime │           │ session.rs  │
└───┬───┘             └────┬────┘           └──────┬──────┘
    ▲                      │ events               │
    │ ◄────────────────────┤ SSE                   ▼
    │                      │              ┌────────────────┐
    │                      │              │ agent::loop    │
    │                      │              │ (agentyx-core) │
    │                      │              └────┬───────────┘
    │                      │                   │ chat()
    │                      │                   ▼
     │                      │              ┌────────────────┐
     │                      │              │ llm::Provider  │
     │                      │              │ (Ollama/Groq/  │
     │                      │              │  Minimax)      │
     │                      │              └────┬───────────┘
     │                      │                   │ SSE / NDJSON
     │                      │                   ▼
     │                      │              ChatEvent stream
     │                      │                   │
     │                      │   emit / SSE     │
     │                      └───────────────────┘
```

## Flujo de datos (tool call)

```
agent loop
   │  ChatEvent::ToolUse { id, name, args }
   ▼
permissions::check(tool, args, workspace_ctx)
   │
   ├─ allow ──► tools::<name>.run(args) ──► journal.append ──► ChatEvent::ToolResult
   ├─ ask   ──► UI prompt (Tauri event / HTTP SSE) ──► user decision ──► (allow | deny)
   └─ deny  ──► ChatEvent::ToolResult { is_error: true, output: "denied by permission" }
```

## Estado persistente

| Estado | Ubicación | Quién lo escribe |
|---|---|---|
| Config global | `~/.agentyx/config.toml` | `config::*` |
| Config de workspace | `~/.agentyx/workspaces/<id>/config.toml` | `workspace::*` |
| Sesiones + mensajes | `~/.agentyx/workspaces/<id>/state.db` (SQLite) | `storage::*` |
| Journal | `state.db` (tabla) o `journal.jsonl` (rotado) | `journal::*` |
| Cache de índices | `~/.agentyx/cache/<workspace-hash>/` | `workspace::*` |
| Auth token del server | Keychain del SO (Keychain/DPAPI/Secret Service) | `server::auth` |

## Restricciones arquitectónicas

1. **Core no importa Tauri**. Punto. (ver `AGENTS.md` §3.1)
2. **Lógica de negocio en Rust**, no en Svelte.
3. **IPC tipado y versionado**. Sin strings mágicas. (ver [ipc.md](./ipc.md))
4. **Streaming por defecto**: LLM, PTY, logs → eventos.
5. **Sandbox por workspace**: toda I/O se canonicaliza contra
   `root_path ∪ extra_paths` del workspace (no solo `root`).
6. **Reversible**: el journal hace toda acción reproducible.
7. **Fail loudly**: `?` + `Context`/`thiserror`, nunca `unwrap()`.
8. **DRY/KISS/SOLID** sin abstracciones especulativas.
9. **Multi-agent desde el inicio**: el agent loop consume `AgentSpec` y
   distingue `Primary | Subagent | Hidden`. Aunque v1 solo traiga 1–2
   primary + 1 subagent built-in, no debe haber una sola ruta de código
   que asuma "un único agente por sesión".
10. **Local-first**: ningún archivo del workspace sale del dispositivo.
    El único tráfico de red saliente son las llamadas a providers LLM
    que el usuario haya configurado explícitamente.

## Trade-offs documentados

Ver [adr/](./adr/) para las decisiones y sus justificaciones:
- 0001 Tauri vs Electron.
- 0002 Rust core + Svelte UI.
- 0003 Servidor HTTP embebido.
- 0004 Orden de detección de `.venv`.
- 0005 PTY con `portable-pty`.
- 0006 SQLite con `rusqlite` (no `sqlx`).
- 0007 Modelo `root + extra_paths` por workspace.
- 0008 Scope de providers v1 (Ollama / Groq / Minimax).
