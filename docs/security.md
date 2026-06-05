# Security

> Quick-reference index. The full security model is in
> [`../AGENTS.md`](../AGENTS.md) §9 and
> [`../specs/architecture.md`](../specs/architecture.md) §Security.

## Principles

1. **Least privilege** — Tauri capabilities scoped to the minimum
   needed per window. The main window cannot call `secrets_set` or
   `config_update_*` (defense in depth).
2. **Secrets never on disk** — API keys live in the OS keychain
   (`agentyx` service). `config.toml` only stores `env:VAR_NAME`
   or `keychain:account` references.
3. **No shell or network without explicit user permission** — the
   `PermissionGate` prompts the user (or auto-approves per the
   matrix) before any write, shell, or network call.
4. **Workspace sandbox** — tools are restricted to
   `root_path ∪ extra_paths` (see ADR-0007). Path traversal is
   blocked at the API level.
5. **No telemetry by default** — `telemetry_enabled = false` in
   `~/.agentyx/config.toml`. The user can opt in granularly later.
6. **Signed updates** — `tauri-plugin-updater` with a cryptographic
   signature (F20, v1.0).
7. **CSP locked down in production** — `script-src 'self'`, no
   `unsafe-inline`. HMR `unsafe-eval` is dev-only.

## What we never log

- API key values (the `secrets::set` Tauri command **does not**
  log the value, only the `provider_id`).
- Full message content (we log `content_summary` truncated to
  200 chars in `tracing::info!`).
- File paths containing secrets (e.g. `.env` is filtered out of
  the `args` field of `ToolCall` events).
- The `Authorization` header of any provider request.

## Audit trail

Every agent action lands in the journal
(`specs/domains/journal.md`):

- `tool_call` / `tool_result` — what was invoked, with what args, and the output
- `permission_decision` — who said yes/no, and when
- `subagent_lifecycle` — start/finish/abort of a delegated run
- `provider_event` — latency, usage, error codes
- `error` — internal failures with full context (not user-safe)

The journal is append-only SQLite; no UPDATE/DELETE methods are
exposed by the `JournalRepo` API.

## Reporting a vulnerability

Open a **private** security advisory on GitHub
(https://github.com/miglesiassarria/Agentyx/security/advisories/new).
Do NOT file a public issue. We aim to acknowledge within 48h and
patch within 7 days for `blocker`/`major` severities.
