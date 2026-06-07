/// IPC bridge — typed wrapper around Tauri `invoke()` and `listen()`.
///
/// All UI ↔ Rust communication goes through this file. The UI
/// never calls `window.__TAURI__` directly; if you find yourself
/// reaching for the global, add a function here instead.
///
/// In browser mode (no Tauri), calls are routed to the embedded
/// HTTP server via fetch + SSE. Detection is automatic.

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

type UnlistenFn = () => void;

// ============================================================
// Environment detection
// ============================================================

const isBrowser = typeof window !== 'undefined' && !('__TAURI__' in window);

// ============================================================
// Tauri mode (desktop)
// ============================================================

let tauriInvoke: ((cmd: string, args?: Record<string, unknown>) => Promise<unknown>) | null = null;
let tauriListen: ((event: string, handler: (e: { payload: unknown }) => void) => Promise<() => void>) | null = null;

if (!isBrowser) {
  const core = await import('@tauri-apps/api/core');
  const event = await import('@tauri-apps/api/event');
  tauriInvoke = core.invoke;
  tauriListen = (ev: string, handler: (e: { payload: unknown }) => void) =>
    event.listen(ev, handler);
}

// ============================================================
// Browser mode (HTTP + SSE)
// ============================================================

function httpBaseUrl(): string {
  // In browser mode, the API is served from the same origin.
  return '';
}

interface HttpError {
  code: string;
  message: string;
  context?: unknown;
}

async function httpCall<T>(method: string, path: string, body?: unknown): Promise<T> {
  const opts: RequestInit = {
    method,
    headers: { 'Content-Type': 'application/json' },
  };
  if (body !== undefined) {
    opts.body = JSON.stringify(body);
  }
  const res = await fetch(`${httpBaseUrl()}${path}`, opts);
  if (!res.ok) {
    const err: HttpError = await res.json().catch(() => ({
      code: 'http_error',
      message: `HTTP ${res.status}`,
    }));
    const error = new Error(`${err.code}: ${err.message}`);
    (error as Error & { code: string }).code = err.code;
    throw error;
  }
  if (res.status === 204) return undefined as T;
  return res.json();
}

// SSE connection singleton for browser mode.
let sseConnection: EventSource | null = null;
const sseListeners = new Map<string, Set<(payload: unknown) => void>>();

function ensureSse(): EventSource {
  if (sseConnection) return sseConnection;
  sseConnection = new EventSource(`${httpBaseUrl()}/api/v1/events`);
  sseConnection.onmessage = (e) => {
    // Default "message" events are dispatched to all listeners with no event name.
  };
  sseConnection.addEventListener('ping', () => {}); // heartbeat, ignore
  // Dynamic events are registered via addEventListener below.
  return sseConnection;
}

function listenSse<T>(eventName: string, handler: (payload: T) => void): () => void {
  const sse = ensureSse();
  let handlers = sseListeners.get(eventName);
  if (!handlers) {
    handlers = new Set();
    sseListeners.set(eventName, handlers);
    sse.addEventListener(eventName, ((e: MessageEvent) => {
      const payload = JSON.parse(e.data) as unknown;
      for (const h of sseListeners.get(eventName) ?? []) {
        h(payload);
      }
    }) as EventListener);
  }
  handlers.add(handler as (payload: unknown) => void);
  return () => {
    handlers!.delete(handler as (payload: unknown) => void);
    if (handlers!.size === 0) {
      sseListeners.delete(eventName);
    }
  };
}

// ============================================================
// Unified call() and listen()
// ============================================================

function normalizeArgs(args: Record<string, unknown> | undefined): Record<string, unknown> | undefined {
  if (!args) return args;
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(args)) {
    out[k] = v;
  }
  return out;
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isBrowser && tauriInvoke) {
    try {
      return await tauriInvoke<T>(command, args);
    } catch (e) {
      const err = e as { code?: string; message?: string; context?: unknown };
      const message = err.message ?? String(e);
      const code = err.code ?? 'unknown';
      const error = new Error(`${code}: ${message}`);
      (error as Error & { code: string; context?: unknown }).code = code;
      (error as Error & { code: string; context?: unknown }).context = err.context;
      throw error;
    }
  }
  // Browser mode: route to HTTP.
  return httpCallBrowser<T>(command, args);
}

async function httpCallBrowser<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const a = args ?? {};
  switch (command) {
    // Session
    case 'create_session':
      return httpCall<T>('POST', `/api/v1/workspaces/${a.workspace_id}/sessions`, {
        agentId: a.agent_id,
        title: a.title,
      });
    case 'send':
      return httpCall<T>('POST', `/api/v1/sessions/${a.session_id}/messages`, {
        content: a.content,
        mentions: a.mentions ?? [],
      });
    case 'abort':
      return httpCall<T>('POST', `/api/v1/sessions/${a.session_id}/abort`);
    case 'list_sessions':
      return httpCall<T>('GET', `/api/v1/workspaces/${a.workspace_id}/sessions${a.limit ? `?limit=${a.limit}` : ''}`);
    case 'get_history':
      return httpCall<T>('GET', `/api/v1/sessions/${a.session_id}/history${a.limit ? `?limit=${a.limit}` : ''}`);
    case 'set_active_agent':
      return httpCall<T>('POST', `/api/v1/sessions/${a.session_id}/active-agent`, { agentId: a.agent_id });
    case 'get_active_agent':
      return httpCall<T>('GET', `/api/v1/sessions/${a.session_id}/active-agent`);
    // Workspace
    case 'list_workspaces':
      return httpCall<T>('GET', '/api/v1/workspaces');
    case 'open':
      return httpCall<T>('POST', '/api/v1/workspaces', { rootPath: a.root_path, name: a.name });
    case 'get_workspace':
      return httpCall<T>('GET', `/api/v1/workspaces/${a.workspace_id}`);
    case 'delete_workspace':
      return httpCall<T>('DELETE', `/api/v1/workspaces/${a.workspace_id}?force=${a.force ?? false}`);
    case 'detect_workspace_venv':
      return httpCall<T>('GET', `/api/v1/workspaces/${a.workspace_id}/venv`);
    case 'add_extra_path':
      return httpCall<T>('POST', `/api/v1/workspaces/${a.workspace_id}/extra-paths`, { path: a.path, label: a.label });
    case 'remove_extra_path':
      return httpCall<T>('DELETE', `/api/v1/workspaces/${a.workspace_id}/extra-paths/delete?path=${encodeURIComponent(a.path as string)}`);
    case 'list_extra_paths':
      return httpCall<T>('GET', `/api/v1/workspaces/${a.workspace_id}/extra-paths`);
    case 'effective_paths':
      return httpCall<T>('GET', `/api/v1/workspaces/${a.workspace_id}/effective-paths`);
    case 'list_dir':
      return httpCall<T>('POST', `/api/v1/workspaces/${a.workspace_id}/list-dir`, { path: a.path });
    // Agents
    case 'list_agents':
      return httpCall<T>('GET', '/api/v1/agents');
    case 'get_agent':
      return httpCall<T>('GET', `/api/v1/agents/${a.id}`);
    // Config
    case 'config_get_global':
      return httpCall<T>('GET', '/api/v1/config/global');
    case 'config_update_global':
      return httpCall<T>('PATCH', '/api/v1/config/global', a.patch);
    // Providers
    case 'providers_test_connection':
      return httpCall<T>('POST', '/api/v1/providers/test-connection', a.request);
    // Secrets
    case 'set_secret':
      return httpCall<T>('POST', `/api/v1/secrets/${a.provider_id}`, { value: a.value });
    case 'delete_secret':
      return httpCall<T>('DELETE', `/api/v1/secrets/${a.provider_id}`);
    case 'list_providers':
      return httpCall<T>('GET', '/api/v1/secrets/providers');
    // Permissions
    case 'get_matrix':
      return httpCall<T>('GET', `/api/v1/permissions/matrix${a.workspace_id ? `?workspace=${a.workspace_id}` : ''}`);
    case 'set_default':
      return httpCall<T>('POST', '/api/v1/permissions/default', { tool: a.tool, decision: a.decision });
    default:
      throw new Error(`Unknown command in browser mode: ${command}`);
  }
}

async function listen<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  if (!isBrowser && tauriListen) {
    return tauriListen<T>(event, handler);
  }
  // Browser mode: route to SSE.
  return listenSse<T>(event, handler);
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
