# Agentyx

> Fast, lightweight, local-first agentic app — Tauri 2 + Axum + Svelte 5 + Rust.

Agentyx is a desktop application with an embedded LAN web server that
turns open-source LLM code into autonomous productivity agents. You ask,
Agentyx coordinates the work to get it done. Built for action, not for
programming.

## Status

**v0.1.0** — MVP foundation in progress. The monorepo skeleton is in
place, F02 multi-workspace is implemented, and F01 chat streaming has
the core/app/UI Phase 1 + Phase 2 foundation in `main`. The MVP now
requires both desktop and browser access over LAN via the embedded
server (`F06`). Settings, LAN web, diffs, and the full agents UI remain
the next MVP blocks.

| Component | State |
|---|---|
| Specs | mixed: `implemented`, `approved`, `review`, and `draft` (see `specs/STATUS.md`) |
| Monorepo skeleton | ✅ in place |
| F02 multi-workspace | ✅ implemented, with AC7 still tracked as partial |
| F01 chat streaming | 🚧 implemented partial: sessions, events, read-only tools, permissions, chat UI |
| F05 settings | 🚧 draft; backend/config/secrets/permission matrix partly implemented, UI/E2E still pending |
| F06 web server LAN | ✅ shipped: Axum skeleton + EventBus SSE + bearer middleware (AC1-AC3) |
| F04 file diffs | 🚧 draft; not implemented |
| F-agents-ui | 🚧 draft; agent model exists, dedicated UI still pending |
| CI (fmt, clippy, test, audit, deny, typecheck, vitest) | ✅ GitHub Actions |

## Project layout

```
agentyx/
├── AGENTS.md              # rules and architecture for AI/human agents
├── specs/                 # spec-driven design and status board
│   ├── STATUS.md          # board by status
│   ├── features/ROADMAP.md # features by phase
│   ├── architecture.md    # global architecture
│   ├── ipc.md             # IPC contract
│   ├── agents.md          # multi-agent model
│   ├── domains/           # domain specs
│   └── features/          # MVP and roadmap feature specs
├── crates/                # Rust workspace
│   ├── agentyx-core/      # pure Rust domain (no Tauri)
│   ├── agentyx-app/       # Tauri 2 desktop binary
│   └── agentyx-sdk/       # SDK for third-party embeds
├── ui/                    # Svelte 5 + Vite + TypeScript strict
├── scripts/               # fmt, lint, test, release, dev, clean
└── .github/workflows/     # CI (fmt, clippy, test, audit, deny, UI)
```

## Tech stack

- **Tauri 2** (Rust) — desktop shell
- **Axum** — embedded HTTP/SSE server for browser access over LAN
- **Svelte 5** (runes) + Vite + TypeScript strict
- **rusqlite** (bundled) — local storage
- **reqwest + SSE** — LLM provider streaming
- **portable-pty** — PTY for tool execution
- **tracing** — structured logging
- **bun** — package manager and task runner (with npm fallback)
- **CodeMirror 6** + **uPlot** — UI components
- **marked** + **DOMPurify** — safe markdown

## Development

### Prerequisites

- **Rust 1.80+** — install via [`rustup`](https://rustup.rs)
- **Node 20.10+** (use the pinned version in `.nvmrc`)
- **bun 1.1+** (or `npm` as a fallback)
- **Platform deps**:
  - **macOS**: Xcode Command Line Tools (`xcode-select --install`)
  - **Windows**: Microsoft C++ Build Tools + WebView2
  - **Linux**: `webkit2gtk-4.1`, `libssl-dev`, `libsqlite3-dev`, etc.
    See [Tauri prerequisites](https://tauri.app/start/prerequisites/)

### Install

```bash
# Install JS deps
bun install
# or: npm install

# Install Rust deps (downloads and compiles ~500 crates the first time)
cargo fetch
```

### Run dev mode

```bash
# Boots Vite + Tauri + Rust binary in dev mode with HMR
bun run dev
# or: npm run dev
```

### Build and binaries

For day-to-day local checks, compile the Rust app from the Cargo
workspace:

```bash
cd crates
cargo build -p agentyx-app
```

Debug binaries are written to `crates/target/debug/`:

| Binary | Path | Purpose |
|---|---|---|
| `agentyx-app` | `crates/target/debug/agentyx-app` | Tauri desktop entrypoint. Use this when you need the desktop shell binary updated after Rust backend changes. |
| `agentix` | `crates/target/debug/agentix` | CLI/dev helper. It includes commands such as `serve` for running the embedded web server from the terminal. |
| `agentyx-web` | `crates/target/debug/agentyx-web` | Web-server-only binary for browser/LAN flows without launching the Tauri desktop window. |

Release binaries are generated with:

```bash
cd crates
cargo build -p agentyx-app --release
```

They are written to `crates/target/release/` with the same binary
names. For the full production desktop bundle/installable artifact, use
the Tauri build pipeline:

```bash
bun run build
# or: bun run tauri:build
```

Those artifacts are produced under `crates/target/release/bundle/`
(platform-specific subdirectories such as `macos/`, `dmg/`, `msi/` or
`deb/`, depending on the OS and Tauri configuration).

### Other scripts

```bash
bun run fmt        # rustfmt + prettier --write
bun run fmt:check  # CI mode (no writes)
bun run lint       # clippy + deny + tsc + eslint
bun run test       # cargo test + vitest
bun run typecheck  # tsc --noEmit
bun run audit      # cargo deny check
bun run build      # release build of UI + Tauri
bun run release    # full release pipeline (test, lint, build)
bun run clean      # remove target/, node_modules/, dist/
```

## Architecture

- **Rust core** (`agentyx-core`): pure domain logic, no Tauri. All
  business rules, types, and tests live here.
- **Tauri app** (`agentyx-app`): thin shell. Sets up the window,
  configures plugins, exposes IPC commands, starts the embedded
  HTTP/SSE server, and streams events.
- **SDK** (`agentyx-sdk`): reusable Rust API for third-party
  integrations.
- **UI** (`ui/`): Svelte 5 + runes. All IPC goes through
  `src/lib/ipc.ts`; desktop uses Tauri `invoke/listen`, browser LAN
  uses HTTP + SSE.

See `specs/architecture.md` for the full design.

## Multi-agent from day 1

Agentyx ships with 2 primary agents and 1 subagent built-in:

- `build` — full tool access, default
- `plan` — read-only, deny on writes/shell
- `general` — subagent, invoked via `@general` or the `task` tool

Cycle between primaries with `Cmd+[` / `Cmd+]`. See `specs/agents.md`.

## Security

- **Sandbox by workspace**: each workspace is a `root_path ∪ extra_paths`
  (see ADR-0007). Tools cannot escape this union.
- **Secrets in keychain**: API keys are never written to `config.toml`.
  They live in the OS keychain (`agentyx` service).
- **LAN auth**: when the embedded server binds to `0.0.0.0`, bearer
  token auth is mandatory.
- **No telemetry by default**: `telemetry_enabled = false` in config.
- **CSP locked down in production**: `script-src 'self'`, no
  `unsafe-inline`.
- **Content sanitized**: markdown via `marked` + `DOMPurify`.

See `specs/architecture.md` and `AGENTS.md` §9.

## Contributing

Read `AGENTS.md` first. It's the source of truth for conventions,
style, and architecture. Key rules:

- Spec-driven: every cross-file change references a spec.
- `cargo fmt` and `cargo clippy` clean before commit.
- `bun run lint` passes before push.
- Conventional commits (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`).

## License

MIT OR Apache-2.0 (at your option).
