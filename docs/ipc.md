# IPC

> Quick-reference index. The authoritative IPC contract is in
> [`../specs/ipc.md`](../specs/ipc.md).

## Conventions

- **Rust** uses `snake_case` for commands and event names.
- **TypeScript** uses `camelCase` for fields.
- All commands and events are **typed** in both languages.
- Errors are `{ code, message, context? }` (the `AppError` derive
  already does this).
- Commands return `Result<T, AppError>`; events have a
  `schema_version` field (suffix `.v1`).

## Where to look

- **Command handlers** in Rust: `crates/agentyx-app/src/commands/`
- **Typed wrappers** in TS: `ui/src/lib/ipc.ts`
- **DTO types** in TS: `ui/src/lib/ipc-types.ts`
- **Stream events** consumed by the UI: `ui/src/lib/ipc.ts#events`

## Example: chat send

```ts
// UI
import { session, events } from '$lib/ipc';

const { runId } = await session.send(sessionId, 'hello', []);
const unlisten = await events.chatContentDelta((p) => {
  if (p.sessionId !== sessionId) return;
  console.log(p.text);
});
```

```rust
// Rust
#[tauri::command]
pub async fn send(
    state: State<'_, Arc<AppState>>,
    session_id: SessionId,
    content: String,
    mentions: Vec<AtMention>,
) -> AppResult<RunHandle> { ... }
```

## Adding a new command

1. Write the Rust handler in the appropriate `commands/*.rs` file.
2. Declare it in `tauri::generate_handler!` (in `crates/agentyx-app/src/main.rs`).
3. Add a typed wrapper in `ui/src/lib/ipc.ts` and DTOs in
   `ui/src/lib/ipc-types.ts`.
4. Update `specs/ipc.md` with the new command/event shape.
5. Add at least one test in `crates/agentyx-core/tests/`.
