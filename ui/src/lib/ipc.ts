/// IPC bridge — typed wrapper around Tauri `invoke()` and `listen()`.
///
/// All UI ↔ Rust communication goes through this file. The UI
/// never calls `window.__TAURI__` directly; if you find yourself
/// reaching for the global, add a function here instead.
///
/// See `../../../specs/ipc.md` for the full contract (event names,
/// error shapes, snake_case ↔ camelCase conventions).
///
/// **Conventions** (must stay in sync with the Rust side):
/// - Tauri command **names** are the Rust function names, snake_case
///   (e.g. `create_session`, `list_sessions`, `add_extra_path`).
/// - Tauri command **parameter keys** are snake_case (Tauri 2 default;
///   the Rust commands do not opt into `rename_all = "camelCase"`).
/// - DTO **field names** (return values) are camelCase — the Rust
///   DTOs use `#[serde(rename_all = "camelCase")]`.

import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen as tauriListen, type UnlistenFn } from '@tauri-apps/api/event';

import type {
  AgentId,
  AgentInfoDto,
  AtMention,
  ChatContentDeltaPayload,
  ChatMessageStartPayload,
  ChatRunAbortedPayload,
  ChatRunErrorPayload,
  ChatRunFinishedPayload,
  ChatRunStartedPayload,
  ConfigChangedPayload,
  EffectivePathsDto,
  ExtraPathDto,
  FileEntryDto,
  GlobalConfigDto,
  GlobalConfigPatchDto,
  MessageDto,
  PermissionMatrixDto,
  PermissionRequestDto,
  PermissionRequestedPayload,
  ResolvedConfigDto,
  RunHandleDto,
  RunId,
  SessionId,
  SessionSummaryDto,
  TestConnectionRequest,
  TestConnectionResult,
  VenvSpec,
  WorkspaceConfigDto,
  WorkspaceConfigPatchDto,
  WorkspaceId,
  WorkspaceDto,
} from './ipc-types';

/**
 * Wrap a Tauri command call. Surfaces AppError from Rust as a
 * typed JS Error. Use this everywhere instead of `invoke` directly
 * so error handling is consistent.
 */
async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await tauriInvoke<T>(command, args);
  } catch (e) {
    // Tauri serializes our AppError as `{ code, message, context? }`.
    // Normalize to Error for ergonomic `try/catch` in components.
    const err = e as { code?: string; message?: string; context?: unknown };
    const message = err.message ?? String(e);
    const code = err.code ?? 'unknown';
    const error = new Error(`${code}: ${message}`);
    (error as Error & { code: string; context?: unknown }).code = code;
    (error as Error & { code: string; context?: unknown }).context = err.context;
    throw error;
  }
}

/**
 * Subscribe to a Tauri event. Returns an unlisten function.
 * Auto-typed with the payload shape so the UI gets a typed arg.
 */
async function listen<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  return tauriListen<T>(event, (e) => handler(e.payload));
}

// === Session commands (F01) ===
// Tauri command names: create_session, send, abort, list_sessions,
// get_history, set_active_agent, get_active_agent.

export const session = {
  create: (
    workspaceId: WorkspaceId,
    agentId?: AgentId,
    title?: string,
  ): Promise<SessionSummaryDto> =>
    call('create_session', {
      workspace_id: workspaceId,
      agent_id: agentId,
      title,
    }),

  send: (
    sessionId: SessionId,
    content: string,
    mentions: AtMention[] = [],
  ): Promise<RunHandleDto> =>
    call('send', {
      session_id: sessionId,
      content,
      mentions,
    }),

  abort: (sessionId: SessionId): Promise<void> => call('abort', { session_id: sessionId }),

  list: (workspaceId: WorkspaceId, limit?: number): Promise<SessionSummaryDto[]> =>
    call('list_sessions', { workspace_id: workspaceId, limit }),

  getHistory: (sessionId: SessionId, limit?: number): Promise<MessageDto[]> =>
    call('get_history', { session_id: sessionId, limit }),

  setActiveAgent: (sessionId: SessionId, agentId: AgentId): Promise<void> =>
    call('set_active_agent', { session_id: sessionId, agent_id: agentId }),

  getActiveAgent: (sessionId: SessionId): Promise<AgentId> =>
    call('get_active_agent', { session_id: sessionId }),
};

// === Workspace commands (F02) ===

export const workspace = {
  list: (): Promise<WorkspaceDto[]> => call('list_workspaces'),

  open: (rootPath: string, name?: string): Promise<WorkspaceDto> =>
    call('open', { root_path: rootPath, name }),

  get: (workspaceId: WorkspaceId): Promise<WorkspaceDto> =>
    call('get_workspace', { workspace_id: workspaceId }),

  delete: (workspaceId: WorkspaceId, force = false): Promise<void> =>
    call('delete_workspace', { workspace_id: workspaceId, force }),

  detectVenv: (workspaceId: WorkspaceId): Promise<VenvSpec | null> =>
    call('detect_workspace_venv', { workspace_id: workspaceId }),

  addExtraPath: (
    workspaceId: WorkspaceId,
    path: string,
    label?: string | null,
  ): Promise<ExtraPathDto> => call('add_extra_path', { workspace_id: workspaceId, path, label }),

  removeExtraPath: (workspaceId: WorkspaceId, path: string): Promise<void> =>
    call('remove_extra_path', { workspace_id: workspaceId, path }),

  listExtraPaths: (workspaceId: WorkspaceId): Promise<ExtraPathDto[]> =>
    call('list_extra_paths', { workspace_id: workspaceId }),

  effectivePaths: (workspaceId: WorkspaceId): Promise<EffectivePathsDto> =>
    call('effective_paths', { workspace_id: workspaceId }),

  listDir: (workspaceId: WorkspaceId, path: string): Promise<FileEntryDto[]> =>
    call('list_dir', { workspace_id: workspaceId, path }),
};

// === Agents (multi-agent) ===

export const agents = {
  list: (): Promise<AgentInfoDto[]> => call('list_agents'),
  get: (id: AgentId): Promise<AgentInfoDto> => call('get_agent', { id }),
};

// === Config commands (F05) ===

export const config = {
  getGlobal: (): Promise<GlobalConfigDto> => call('config_get_global'),
  updateGlobal: (patch: GlobalConfigPatchDto): Promise<GlobalConfigDto> =>
    call('config_update_global', { patch }),
  getWorkspace: (workspaceId: WorkspaceId): Promise<ResolvedConfigDto> =>
    call('config_get_workspace', { workspace_id: workspaceId }),
  updateWorkspace: (
    workspaceId: WorkspaceId,
    patch: WorkspaceConfigPatchDto,
  ): Promise<WorkspaceConfigDto> =>
    call('config_update_workspace', { workspace_id: workspaceId, patch }),
};

// === Providers (F05 test connection) ===

export const providers = {
  testConnection: (request: TestConnectionRequest): Promise<TestConnectionResult> =>
    call('providers_test_connection', { request }),
};

// === Secrets (F05 keychain) ===

export const secrets = {
  set: (providerId: string, value: string): Promise<void> =>
    call('set_secret', { provider_id: providerId, value }),
  delete: (providerId: string): Promise<void> => call('delete_secret', { provider_id: providerId }),
  listProviders: (): Promise<string[]> => call('list_providers'),
};

// === Permissions (F01 + F05) ===

export const permissions = {
  getMatrix: (workspaceId?: WorkspaceId): Promise<PermissionMatrixDto> =>
    call('get_matrix', { workspace_id: workspaceId }),
  setDefault: (tool: string, decision: 'allow' | 'ask' | 'deny'): Promise<void> =>
    call('set_default', { tool, decision }),
  list: (): Promise<PermissionRequestDto[]> => call('list'),
  respond: (
    requestId: string,
    response:
      | { kind: 'allowOnce' }
      | { kind: 'allowSession' }
      | { kind: 'allowAlways'; tool: string }
      | { kind: 'deny' },
  ): Promise<void> => call('respond', { request_id: requestId, response }),
};

// === Streaming events (F01) ===

export interface ChatRunListeners {
  onStarted: (cb: (p: ChatRunStartedPayload) => void) => Promise<UnlistenFn>;
  onMessageStart: (cb: (p: ChatMessageStartPayload) => void) => Promise<UnlistenFn>;
  onContentDelta: (cb: (p: ChatContentDeltaPayload) => void) => Promise<UnlistenFn>;
  onFinished: (cb: (p: ChatRunFinishedPayload) => void) => Promise<UnlistenFn>;
  onError: (cb: (p: ChatRunErrorPayload) => void) => Promise<UnlistenFn>;
}

/** Filter an event listener to a specific runId. */
function forRun<T extends { runId: RunId }>(
  cb: (p: T) => void,
  runId: RunId | null,
): (p: T) => void {
  return (p) => {
    if (runId === null || p.runId === runId) cb(p);
  };
}

export const events = {
  // chat.*.v1
  chatRunStarted: (cb: (p: ChatRunStartedPayload) => void) => listen('chat.run.started.v1', cb),
  chatMessageStart: (cb: (p: ChatMessageStartPayload) => void) =>
    listen('chat.message_start.v1', cb),
  chatContentDelta: (cb: (p: ChatContentDeltaPayload) => void) =>
    listen('chat.content.delta.v1', cb),
  chatRunFinished: (cb: (p: ChatRunFinishedPayload) => void) => listen('chat.run.finished.v1', cb),
  chatRunError: (cb: (p: ChatRunErrorPayload) => void) => listen('chat.run.error.v1', cb),
  chatRunAborted: (cb: (p: ChatRunAbortedPayload) => void) => listen('chat.run.aborted.v1', cb),
  permissionRequested: (cb: (p: PermissionRequestedPayload) => void) =>
    listen('permission.requested.v1', cb),

  /**
   * Subscribe to all chat events for a specific run. Returns an
   * async unbind handle. Call `await bindRun(...)` to wait for
   * the listeners to be attached, then call the returned function
   * to release them.
   */
  async bindRun(
    runId: RunId,
    handlers: {
      onStarted?: (p: ChatRunStartedPayload) => void;
      onMessageStart?: (p: ChatMessageStartPayload) => void;
      onContentDelta?: (p: ChatContentDeltaPayload) => void;
      onFinished?: (p: ChatRunFinishedPayload) => void;
      onError?: (p: ChatRunErrorPayload) => void;
      onAborted?: (p: ChatRunAbortedPayload) => void;
    },
  ): Promise<() => void> {
    const unlistens: UnlistenFn[] = await Promise.all([
      handlers.onStarted
        ? events.chatRunStarted(forRun(handlers.onStarted, runId))
        : Promise.resolve<UnlistenFn>(() => undefined),
      handlers.onMessageStart
        ? events.chatMessageStart(forRun(handlers.onMessageStart, runId))
        : Promise.resolve<UnlistenFn>(() => undefined),
      handlers.onContentDelta
        ? events.chatContentDelta(forRun(handlers.onContentDelta, runId))
        : Promise.resolve<UnlistenFn>(() => undefined),
      handlers.onFinished
        ? events.chatRunFinished(forRun(handlers.onFinished, runId))
        : Promise.resolve<UnlistenFn>(() => undefined),
      handlers.onError
        ? events.chatRunError(forRun(handlers.onError, runId))
        : Promise.resolve<UnlistenFn>(() => undefined),
      handlers.onAborted
        ? events.chatRunAborted(forRun(handlers.onAborted, runId))
        : Promise.resolve<UnlistenFn>(() => undefined),
    ]);
    return () => unlistens.forEach((u) => u());
  },

  // agent.*.v1 (F-agents-ui)
  agentChanged: (
    cb: (p: { sessionId: SessionId; fromAgentId: AgentId; toAgentId: AgentId }) => void,
  ) => listen('agent.changed.v1', cb),
  subagentStarted: (
    cb: (p: { parentRunId: RunId; childSessionId: SessionId; subagentId: AgentId }) => void,
  ) => listen('subagent.started.v1', cb),
  subagentFinished: (
    cb: (p: { parentRunId: RunId; childSessionId: SessionId; result: unknown }) => void,
  ) => listen('subagent.finished.v1', cb),
  subagentAborted: (
    cb: (p: { parentRunId: RunId; childSessionId: SessionId; reason: string }) => void,
  ) => listen('subagent.aborted.v1', cb),

  // workspace.*.v1 (F02)
  workspaceExtraPathAdded: (
    cb: (p: { workspaceId: WorkspaceId; path: string; label?: string }) => void,
  ) => listen('workspace.extra_path_added.v1', cb),
  workspaceExtraPathRemoved: (cb: (p: { workspaceId: WorkspaceId; path: string }) => void) =>
    listen('workspace.extra_path_removed.v1', cb),

  // config.changed.v1 (F05.AC15) — fired after a successful
  // `config_update_global` or `config_update_workspace`. Multi-tab
  // and multi-window UIs use this to refresh their state.
  configChanged: (cb: (p: ConfigChangedPayload) => void) => listen('config.changed.v1', cb),
};
