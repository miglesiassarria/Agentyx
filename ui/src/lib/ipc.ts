/// IPC bridge — typed wrapper around Tauri `invoke()` and `listen()`.
///
/// All UI ↔ Rust communication goes through this file. The UI
/// never calls `window.__TAURI__` directly; if you find yourself
/// reaching for the global, add a function here instead.
///
/// See `../../../specs/ipc.md` for the full contract (event names,
/// error shapes, snake_case ↔ camelCase conventions).

import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen as tauriListen, type UnlistenFn } from '@tauri-apps/api/event';

import type {
  AtMention,
  EffectivePathsDto,
  ExtraPathDto,
  FileEntryDto,
  PermissionMatrixDto,
  RunHandle,
  SessionSummaryDto,
  TestConnectionResult,
  VenvSpec,
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

export const session = {
  create: (
    workspaceId: string,
    agentId?: string,
    title?: string,
  ): Promise<unknown /* SessionDto */> => call('create', { workspaceId, agentId, title }),

  send: (sessionId: string, content: string, mentions: AtMention[] = []): Promise<RunHandle> =>
    call('send', { sessionId, content, mentions }),

  abort: (sessionId: string): Promise<void> => call('abort', { sessionId }),

  list: (workspaceId: string, limit?: number, before?: string): Promise<SessionSummaryDto[]> =>
    call('list', { workspaceId, limit, before }),

  getHistory: (
    sessionId: string,
    limit?: number,
    before?: string,
  ): Promise<unknown /* JournalEntry[] */> => call('get_history', { sessionId, limit, before }),

  setActiveAgent: (sessionId: string, agentId: string): Promise<void> =>
    call('set_active_agent', { sessionId, agentId }),

  getActiveAgent: (sessionId: string): Promise<string /* AgentId */> =>
    call('get_active_agent', { sessionId }),
};

// === Workspace commands (F02) ===

export const workspace = {
  list: (): Promise<WorkspaceDto[]> => call('list'),

  open: (rootPath: string, name?: string): Promise<WorkspaceDto> =>
    call('open', { rootPath, name }),

  get: (workspaceId: string): Promise<WorkspaceDto> => call('get', { workspaceId }),

  delete: (workspaceId: string, force = false): Promise<void> =>
    call('delete', { workspaceId, force }),

  detectVenv: (workspaceId: string): Promise<VenvSpec | null> =>
    call('detect_venv', { workspaceId }),

  addExtraPath: (workspaceId: string, path: string, label?: string | null): Promise<ExtraPathDto> =>
    call('add_extra_path', { workspaceId, path, label }),

  removeExtraPath: (workspaceId: string, path: string): Promise<void> =>
    call('remove_extra_path', { workspaceId, path }),

  listExtraPaths: (workspaceId: string): Promise<ExtraPathDto[]> =>
    call('list_extra_paths', { workspaceId }),

  effectivePaths: (workspaceId: string): Promise<EffectivePathsDto> =>
    call('effective_paths', { workspaceId }),

  listDir: (workspaceId: string, path: string): Promise<FileEntryDto[]> =>
    call('list_dir', { workspaceId, path }),
};

// === Config commands (F05) ===

export const config = {
  getGlobal: (): Promise<unknown /* GlobalConfigDto */> => call('get_global'),
  updateGlobal: (patch: unknown): Promise<unknown> => call('update_global', { patch }),
  getWorkspace: (workspaceId: string): Promise<unknown> => call('get_workspace', { workspaceId }),
  updateWorkspace: (workspaceId: string, patch: unknown): Promise<unknown> =>
    call('update_workspace', { workspaceId, patch }),
};

// === Agents (multi-agent) ===

export const agents = {
  list: (): Promise<unknown /* AgentInfo[] */> => call('list'),
  get: (id: string): Promise<unknown /* AgentInfo */> => call('get', { id }),
};

// === Providers (F05 test connection) ===

export const providers = {
  testConnection: (providerId: string): Promise<TestConnectionResult> =>
    call('test_connection', { providerId }),
};

// === Secrets (F05 keychain) ===

export const secrets = {
  set: (providerId: string, value: string): Promise<void> => call('set', { providerId, value }),
  delete: (providerId: string): Promise<void> => call('delete', { providerId }),
  listProviders: (): Promise<string[]> => call('list_providers'),
};

// === Permissions (F01 + F05) ===

export const permissions = {
  getMatrix: (workspaceId?: string): Promise<PermissionMatrixDto> =>
    call('get_matrix', { workspaceId }),
  setDefault: (tool: string, decision: 'allow' | 'ask' | 'deny'): Promise<void> =>
    call('set_default', { tool, decision }),
  respond: (
    requestId: string,
    response: { kind: 'allowOnce' | 'allowSession' | 'allowAlways' | 'deny' },
  ): Promise<void> => call('respond', { requestId, response }),
};

// === Streaming events (F01) ===

export const events = {
  // chat.*.v1
  chatRunStarted: (cb: (p: { sessionId: string; runId: string; agentId: string }) => void) =>
    listen('chat.run.started.v1', cb),
  chatMessageStart: (cb: (p: { sessionId: string; runId: string; messageId: string }) => void) =>
    listen('chat.message.start.v1', cb),
  chatContentDelta: (
    cb: (p: { sessionId: string; runId: string; messageId: string; text: string }) => void,
  ) => listen('chat.content.delta.v1', cb),
  chatToolCall: (
    cb: (p: {
      sessionId: string;
      runId: string;
      messageId: string;
      toolCallId: string;
      name: string;
      args: unknown;
      argsSummary: string;
    }) => void,
  ) => listen('chat.tool_call.v1', cb),
  chatToolResult: (
    cb: (p: {
      sessionId: string;
      runId: string;
      toolCallId: string;
      output: string;
      outputSummary: string;
      isError: boolean;
      durationMs: number;
      truncated: boolean;
    }) => void,
  ) => listen('chat.tool_result.v1', cb),
  chatMessageEnd: (
    cb: (p: {
      sessionId: string;
      runId: string;
      messageId: string;
      usage: { promptTokens: number; completionTokens: number; totalTokens: number };
      finishReason: 'stop' | 'length' | 'tool_use' | 'error' | 'aborted';
    }) => void,
  ) => listen('chat.message.end.v1', cb),
  chatRunFinished: (
    cb: (p: {
      sessionId: string;
      runId: string;
      status: 'completed' | 'aborted' | 'error' | 'timeout';
      durationMs: number;
    }) => void,
  ) => listen('chat.run.finished.v1', cb),
  chatRunError: (
    cb: (p: {
      sessionId: string;
      runId: string;
      code: string;
      message: string;
      retryable: boolean;
    }) => void,
  ) => listen('chat.run.error.v1', cb),
  chatRunAborted: (
    cb: (p: {
      sessionId: string;
      runId: string;
      reason: 'user' | 'timeout' | 'error' | 'max_steps';
    }) => void,
  ) => listen('chat.run.aborted.v1', cb),
  permissionRequested: (
    cb: (p: {
      sessionId: string;
      runId: string;
      requestId: string;
      tool: string;
      args: unknown;
      argsSummary: string;
    }) => void,
  ) => listen('permission.requested.v1', cb),

  // agent.*.v1 (F-agents-ui)
  agentChanged: (cb: (p: { sessionId: string; fromAgentId: string; toAgentId: string }) => void) =>
    listen('agent.changed.v1', cb),
  subagentStarted: (
    cb: (p: { parentRunId: string; childSessionId: string; subagentId: string }) => void,
  ) => listen('subagent.started.v1', cb),
  subagentFinished: (
    cb: (p: { parentRunId: string; childSessionId: string; result: unknown }) => void,
  ) => listen('subagent.finished.v1', cb),
  subagentAborted: (
    cb: (p: { parentRunId: string; childSessionId: string; reason: string }) => void,
  ) => listen('subagent.aborted.v1', cb),

  // workspace.*.v1 (F02)
  workspaceExtraPathAdded: (
    cb: (p: { workspaceId: string; path: string; label?: string }) => void,
  ) => listen('workspace.extra_path_added.v1', cb),
  workspaceExtraPathRemoved: (cb: (p: { workspaceId: string; path: string }) => void) =>
    listen('workspace.extra_path_removed.v1', cb),
};
