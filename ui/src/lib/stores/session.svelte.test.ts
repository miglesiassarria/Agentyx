/// SessionStore unit tests.
///
/// We stub the IPC layer (no real Tauri runtime in vitest) and
/// verify the state machine of the store. Covers:
/// - `createSession` updates `activeSession` and resets messages.
/// - `loadHistory` normalizes persisted messages to `StreamingMessage` form.
/// - `send` appends an optimistic user message, then transitions to `running`.
/// - `send` while running throws and does not duplicate the user message.
/// - `abort` flips `runStatus` to `aborted` when the run finishes.
/// - Event handlers: `onMessageStart` + `onContentDelta` accumulate content
///   on the right message; `onRunFinished` finalizes the message state.
/// - `setActiveAgent` mutates `activeSession.activeAgent`; rejects mid-run.
/// - `cyclePrimary` rotates through the primary agents (excludes hidden).
/// - `detach` clears the state and unbinds the run listeners.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type {
  AgentInfoDto,
  ChatContentDeltaPayload,
  ChatMessageStartPayload,
  ChatRunErrorPayload,
  ChatRunFinishedPayload,
  MessageDto,
  RunHandleDto,
  SessionSummaryDto,
} from '$lib/ipc-types';

// === IPC stub ===
//
// The store imports `lib/ipc` which calls into `@tauri-apps/api`.
// In vitest (jsdom) that import path would fail at module load,
// so we register a global `vi.mock` to swap the module with a
// controllable fake. The `bindRun` helper returns an `unbind`
// handle that the store calls on completion — we capture it to
// drive the event handlers from the test.
//
// `vi.mock` factories are hoisted to the top of the file, so any
// state they need has to live inside `vi.hoisted(...)` to dodge
// the TDZ trap.

type EventHandler = (payload: unknown) => void;

interface FakeEventsApi {
  onStarted?: EventHandler;
  onMessageStart?: EventHandler;
  onContentDelta?: EventHandler;
  onFinished?: EventHandler;
  onError?: EventHandler;
  onAborted?: EventHandler;
}

const stub = vi.hoisted(() => ({
  sessionsCreate: vi.fn(),
  sessionsSend: vi.fn(),
  sessionsAbort: vi.fn(),
  sessionsList: vi.fn(),
  sessionsGetHistory: vi.fn(),
  sessionsSetActiveAgent: vi.fn(),
  sessionsGetActiveAgent: vi.fn(),
  agentsList: vi.fn(),
  agentsGet: vi.fn(),
  permissionsList: vi.fn(),
  permissionsRespond: vi.fn(),
  lastRunHandlers: null as FakeEventsApi | null,
  lastPermissionHandler: null as EventHandler | null,
}));

vi.mock('$lib/ipc', () => {
  return {
    events: {
      bindRun: async (_runId: string, handlers: FakeEventsApi): Promise<() => void> => {
        stub.lastRunHandlers = handlers;
        return () => {
          if (stub.lastRunHandlers === handlers) {
            stub.lastRunHandlers = null;
          }
        };
      },
      permissionRequested: async (cb: EventHandler): Promise<() => void> => {
        stub.lastPermissionHandler = cb;
        return () => {
          if (stub.lastPermissionHandler === cb) {
            stub.lastPermissionHandler = null;
          }
        };
      },
    },
    session: {
      create: stub.sessionsCreate,
      send: stub.sessionsSend,
      abort: stub.sessionsAbort,
      list: stub.sessionsList,
      getHistory: stub.sessionsGetHistory,
      setActiveAgent: stub.sessionsSetActiveAgent,
      getActiveAgent: stub.sessionsGetActiveAgent,
    },
    agents: {
      list: stub.agentsList,
      get: stub.agentsGet,
    },
    permissions: {
      list: stub.permissionsList,
      respond: stub.permissionsRespond,
    },
  };
});

// Now safe to import the store (it pulls in $lib/ipc which is mocked).
import { SessionStore } from '$lib/stores/session.svelte';

// === Helpers ===

const WS_ID = '01HWS0000000000000000000A0';
const SESSION_ID = '01HWS0000000000000000000B0';
const AGENT_BUILD = '01HWS0000000000000000000C0';
const AGENT_PLAN = '01HWS0000000000000000000D0';
const AGENT_GENERAL = '01HWS0000000000000000000E0';
const AGENT_HIDDEN = '01HWS0000000000000000000F0';

const visibleAgents: AgentInfoDto[] = [
  { id: AGENT_BUILD, mode: 'primary', hidden: false, name: 'build' },
  { id: AGENT_PLAN, mode: 'primary', hidden: false, name: 'plan' },
  { id: AGENT_GENERAL, mode: 'subagent', hidden: false, name: 'general' },
  { id: AGENT_HIDDEN, mode: 'hidden', hidden: true, name: 'compaction' },
];

function makeSummary(overrides: Partial<SessionSummaryDto> = {}): SessionSummaryDto {
  return {
    id: SESSION_ID,
    workspaceId: WS_ID,
    activeAgent: AGENT_BUILD,
    title: 'Untitled',
    updatedAt: '2026-06-06T10:00:00.000Z',
    status: 'idle',
    ...overrides,
  };
}

function makeRunHandle(overrides: Partial<RunHandleDto> = {}): RunHandleDto {
  return {
    runId: '01HWS000000000000000000R0N',
    sessionId: SESSION_ID,
    agentId: AGENT_BUILD,
    startedAt: '2026-06-06T10:00:00.000Z',
    ...overrides,
  };
}

function makeMessage(overrides: Partial<MessageDto> = {}): MessageDto {
  return {
    id: '01HWS000000000000000000M00',
    sessionId: SESSION_ID,
    runId: null,
    role: 'user',
    content: 'hello',
    seq: 1,
    createdAt: '2026-06-06T10:00:00.000Z',
    ...overrides,
  };
}

/** Drive the bound `chat.*.v1` listeners (the same closures the store registered). */
function fireMessageStart(p: ChatMessageStartPayload): void {
  stub.lastRunHandlers?.onMessageStart?.(p);
}
function fireContentDelta(p: ChatContentDeltaPayload): void {
  stub.lastRunHandlers?.onContentDelta?.(p);
}
function fireFinished(p: ChatRunFinishedPayload): void {
  stub.lastRunHandlers?.onFinished?.(p);
}
function fireError(p: ChatRunErrorPayload): void {
  stub.lastRunHandlers?.onError?.(p);
}
function fireAborted(p: { runId: string; reason: string }): void {
  stub.lastRunHandlers?.onAborted?.(p);
}

// === Lifecycle ===

beforeEach(() => {
  vi.clearAllMocks();
  stub.lastRunHandlers = null;
  stub.lastPermissionHandler = null;
  stub.sessionsCreate.mockResolvedValue(makeSummary());
  stub.sessionsSend.mockResolvedValue(makeRunHandle());
  stub.sessionsAbort.mockResolvedValue(undefined);
  stub.sessionsList.mockResolvedValue([]);
  stub.sessionsGetHistory.mockResolvedValue([]);
  stub.sessionsSetActiveAgent.mockResolvedValue(undefined);
  stub.sessionsGetActiveAgent.mockResolvedValue(AGENT_BUILD);
  stub.agentsList.mockResolvedValue(visibleAgents);
  stub.permissionsList.mockResolvedValue([]);
  stub.permissionsRespond.mockResolvedValue(undefined);
});

afterEach(() => {
  vi.useRealTimers();
});

// === Tests ===

describe('SessionStore — attach / load', () => {
  it('attaches to a workspace and loads the agent registry', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    expect(store.workspaceId).toBe(WS_ID);
    expect(store.agents).toEqual(visibleAgents);
    expect(store.subagents.map((a) => a.id)).toEqual([AGENT_GENERAL]);
    expect(store.primaryAgents.map((a) => a.id)).toEqual([AGENT_BUILD, AGENT_PLAN]);
  });
});

describe('SessionStore — createSession', () => {
  it('persists the session via IPC and mirrors the summary locally', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    const summary = await store.createSession('Hello');
    expect(stub.sessionsCreate).toHaveBeenCalledWith(WS_ID, undefined, 'Hello');
    expect(summary.id).toBe(SESSION_ID);
    expect(store.activeSession).toEqual(summary);
    expect(store.messages).toEqual([]);
  });
});

describe('SessionStore — loadHistory', () => {
  it('normalizes persisted messages to StreamingMessage form', async () => {
    stub.sessionsGetHistory.mockResolvedValue([
      makeMessage({ id: 'm1', seq: 1, role: 'user', content: 'hi' }),
      makeMessage({ id: 'm2', seq: 2, role: 'assistant', content: 'world' }),
    ]);
    const store = new SessionStore();
    await store.attach(WS_ID);
    store.activeSession = makeSummary();
    await store.loadHistory(SESSION_ID);
    expect(store.messages).toHaveLength(2);
    expect(store.messages[0]?.status).toBe('complete');
    expect(store.messages[0]?.isStreaming).toBe(false);
    expect(store.messages[1]?.content).toBe('world');
  });
});

describe('SessionStore — send', () => {
  it('appends the user message optimistically and transitions to running', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    store.activeSession = makeSummary();
    await store.send('list files in src');
    expect(stub.sessionsSend).toHaveBeenCalledWith(
      SESSION_ID,
      'list files in src',
      expect.any(Array),
    );
    expect(store.messages).toHaveLength(1);
    expect(store.messages[0]?.role).toBe('user');
    expect(store.messages[0]?.content).toBe('list files in src');
    expect(store.runId).toBe('01HWS000000000000000000R0N');
    expect(store.runStatus).toBe('running');
  });

  it('throws and does not duplicate when called while running', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    store.activeSession = makeSummary();
    await store.send('first');
    await expect(store.send('second')).rejects.toThrow();
    expect(store.messages.filter((m) => m.role === 'user')).toHaveLength(1);
  });
});

describe('SessionStore — event folding', () => {
  async function startRun(store: SessionStore): Promise<void> {
    store.activeSession = makeSummary();
    await store.send('hi');
  }

  it('appends a streaming assistant placeholder on chat.message_start.v1', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    await startRun(store);
    fireMessageStart({
      runId: '01HWS000000000000000000R0N',
      messageId: 'm-assistant-1',
    });
    expect(store.messages).toHaveLength(2);
    const last = store.messages[store.messages.length - 1];
    expect(last?.role).toBe('assistant');
    expect(last?.status).toBe('streaming');
    expect(last?.isStreaming).toBe(true);
  });

  it('accumulates content deltas in order on the right message', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    await startRun(store);
    fireMessageStart({ runId: '01HWS000000000000000000R0N', messageId: 'm1' });
    fireContentDelta({
      runId: '01HWS000000000000000000R0N',
      sessionId: SESSION_ID,
      messageId: 'm1',
      text: 'Hello',
    });
    fireContentDelta({
      runId: '01HWS000000000000000000R0N',
      sessionId: SESSION_ID,
      messageId: 'm1',
      text: ', world',
    });
    const assistant = store.messages.find((m) => m.id === 'm1');
    expect(assistant?.content).toBe('Hello, world');
    expect(assistant?.isStreaming).toBe(true);
  });

  it('marks the assistant complete on chat.run.finished.v1 with status=completed', async () => {
    stub.sessionsGetHistory.mockResolvedValue([
      makeMessage({
        id: 'm1',
        seq: 1,
        role: 'assistant',
        content: 'done',
        runId: '01HWS000000000000000000R0N',
      }),
    ]);
    const store = new SessionStore();
    await store.attach(WS_ID);
    await startRun(store);
    fireMessageStart({ runId: '01HWS000000000000000000R0N', messageId: 'm1' });
    fireContentDelta({
      runId: '01HWS000000000000000000R0N',
      sessionId: SESSION_ID,
      messageId: 'm1',
      text: 'done',
    });
    fireFinished({
      runId: '01HWS000000000000000000R0N',
      sessionId: SESSION_ID,
      status: 'completed',
      durationMs: 1234,
    });
    expect(store.runStatus).toBe('completed');
    expect(store.runId).toBeNull();
    // loadHistory is called; the assistant message is now "complete".
    expect(store.messages.find((m) => m.role === 'assistant')?.status).toBe('complete');
  });

  it('marks the assistant aborted on chat.run.finished.v1 with status=aborted', async () => {
    stub.sessionsGetHistory.mockResolvedValue([
      makeMessage({
        id: 'm1',
        seq: 1,
        role: 'assistant',
        content: 'partial',
        runId: '01HWS000000000000000000R0N',
      }),
    ]);
    const store = new SessionStore();
    await store.attach(WS_ID);
    await startRun(store);
    fireMessageStart({ runId: '01HWS000000000000000000R0N', messageId: 'm1' });
    fireContentDelta({
      runId: '01HWS000000000000000000R0N',
      sessionId: SESSION_ID,
      messageId: 'm1',
      text: 'partial',
    });
    fireFinished({
      runId: '01HWS000000000000000000R0N',
      sessionId: SESSION_ID,
      status: 'aborted',
      durationMs: 100,
    });
    expect(store.runStatus).toBe('aborted');
  });

  it('keeps the draft visible when send fails before the run starts', async () => {
    const ipcError = new Error('boom: provider_unavailable') as Error & { code?: string };
    ipcError.code = 'provider_unavailable';
    stub.sessionsSend.mockRejectedValueOnce(ipcError);
    const store = new SessionStore();
    await store.attach(WS_ID);
    store.activeSession = makeSummary();
    await expect(store.send('hi')).rejects.toThrow();
    expect(store.runStatus).toBe('error');
    expect(store.lastError?.code).toBe('provider_unavailable');
    // The optimistic user message must be removed so the user can
    // edit and retry without duplicates.
    expect(store.messages.filter((m) => m.role === 'user')).toHaveLength(0);
  });
});

describe('SessionStore — chat.run.error.v1', () => {
  async function startRun(store: SessionStore): Promise<void> {
    store.activeSession = makeSummary();
    await store.send('hi');
  }

  it('surfaces the error on lastError and flips runStatus to error', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    await startRun(store);
    fireError({
      runId: '01HWS000000000000000000R0N',
      sessionId: SESSION_ID,
      code: 'provider_unavailable',
      message: 'Ollama is down',
      retryable: true,
    });
    expect(store.lastError?.code).toBe('provider_unavailable');
    expect(store.lastError?.message).toBe('Ollama is down');
    expect(store.lastError?.retryable).toBe(true);
    expect(store.runStatus).toBe('error');
  });
});

describe('SessionStore — abort', () => {
  it('calls session.abort when a run is in flight', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    store.activeSession = makeSummary();
    await store.send('hi');
    await store.abort();
    expect(stub.sessionsAbort).toHaveBeenCalledWith(SESSION_ID);
  });

  it('is a no-op when no run is in flight', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    await store.abort();
    expect(stub.sessionsAbort).not.toHaveBeenCalled();
  });
});

describe('SessionStore — setActiveAgent', () => {
  it('persists via IPC and mutates activeSession optimistically', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    store.activeSession = makeSummary();
    await store.setActiveAgent(AGENT_PLAN);
    expect(stub.sessionsSetActiveAgent).toHaveBeenCalledWith(SESSION_ID, AGENT_PLAN);
    expect(store.activeSession?.activeAgent).toBe(AGENT_PLAN);
  });

  it('rejects during a run', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    store.activeSession = makeSummary();
    await store.send('hi');
    await expect(store.setActiveAgent(AGENT_PLAN)).rejects.toThrow();
    expect(store.activeSession?.activeAgent).toBe(AGENT_BUILD);
  });
});

describe('SessionStore — cyclePrimary', () => {
  it('rotates through the visible primary agents, skipping hidden', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    store.activeSession = makeSummary();
    expect(store.activeSession?.activeAgent).toBe(AGENT_BUILD);
    await store.cyclePrimary();
    expect(store.activeSession?.activeAgent).toBe(AGENT_PLAN);
    await store.cyclePrimary();
    expect(store.activeSession?.activeAgent).toBe(AGENT_BUILD);
  });

  it('is a no-op when fewer than 2 primaries are visible', async () => {
    stub.agentsList.mockResolvedValueOnce([
      { id: AGENT_BUILD, mode: 'primary', hidden: false, name: 'build' },
    ]);
    const store = new SessionStore();
    await store.attach(WS_ID);
    store.activeSession = makeSummary();
    await store.cyclePrimary();
    expect(store.activeSession?.activeAgent).toBe(AGENT_BUILD);
  });
});

describe('SessionStore — detach', () => {
  it('clears state and unbinds event listeners', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    store.activeSession = makeSummary();
    await store.send('hi');
    expect(stub.lastRunHandlers).not.toBeNull();
    store.detach();
    expect(store.runId).toBeNull();
    expect(store.runStatus).toBe('idle');
    expect(store.activeSession).toBeNull();
    expect(store.messages).toEqual([]);
    expect(stub.lastRunHandlers).toBeNull();
  });
});

// Compile-time sanity: SessionStore class is re-exported from the
// module so consumers (e.g. tests, the WorkspaceView chat wiring)
// can construct per-workspace instances if needed.
const _ctor: typeof SessionStore | undefined = undefined;
void _ctor;

// Suppress unused-binding lint when the assertion above is the only
// consumer in the file.
const _handlers: FakeEventsApi | null = stub.lastRunHandlers;
void _handlers;

// === Permission flow (F01.AC7) ===

describe('SessionStore — permission flow', () => {
  it('subscribes to permission.requested.v1 on attach', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    expect(stub.lastPermissionHandler).not.toBeNull();
  });

  it('queues a request when a permission.requested.v1 event arrives', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    expect(store.pendingPermissions).toEqual([]);
    expect(store.currentPermission).toBeNull();
    stub.lastPermissionHandler?.({
      runId: '01HWS000000000000000000R0N',
      sessionId: SESSION_ID,
      requestId: '01HWS000000000000000000P01',
      tool: 'shell',
      args: { command: 'ls' },
      argsSummary: '{ "command": "ls" }',
      reason: 'user_approval',
    });
    expect(store.pendingPermissions).toHaveLength(1);
    expect(store.currentPermission?.requestId).toBe('01HWS000000000000000000P01');
    expect(store.currentPermission?.tool).toBe('shell');
  });

  it('de-duplicates requests with the same requestId', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    const payload = {
      runId: '01HWS000000000000000000R0N',
      sessionId: SESSION_ID,
      requestId: '01HWS000000000000000000P01',
      tool: 'shell',
      args: { command: 'ls' },
      argsSummary: '{ "command": "ls" }',
      reason: 'user_approval',
    };
    stub.lastPermissionHandler?.(payload);
    stub.lastPermissionHandler?.(payload);
    expect(store.pendingPermissions).toHaveLength(1);
  });

  it('respondToPermission delivers the decision and removes from the queue', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    stub.lastPermissionHandler?.({
      runId: '01HWS000000000000000000R0N',
      sessionId: SESSION_ID,
      requestId: '01HWS000000000000000000P01',
      tool: 'shell',
      args: { command: 'ls' },
      argsSummary: '{ "command": "ls" }',
      reason: 'user_approval',
    });
    await store.respondToPermission('01HWS000000000000000000P01', { kind: 'allowOnce' });
    expect(stub.permissionsRespond).toHaveBeenCalledWith('01HWS000000000000000000P01', {
      kind: 'allowOnce',
    });
    expect(store.pendingPermissions).toEqual([]);
  });

  it('respondToPermission rolls back the queue on IPC failure', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    stub.lastPermissionHandler?.({
      runId: '01HWS000000000000000000R0N',
      sessionId: SESSION_ID,
      requestId: '01HWS000000000000000000P01',
      tool: 'shell',
      args: { command: 'rm -rf /' },
      argsSummary: '{ "command": "rm -rf /" }',
      reason: 'user_approval',
    });
    stub.permissionsRespond.mockRejectedValueOnce(new Error('ipc failure'));
    await expect(
      store.respondToPermission('01HWS000000000000000000P01', { kind: 'deny' }),
    ).rejects.toThrow('ipc failure');
    expect(store.pendingPermissions).toHaveLength(1);
  });

  it('clears pending permissions on detach', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    stub.lastPermissionHandler?.({
      runId: '01HWS000000000000000000R0N',
      sessionId: SESSION_ID,
      requestId: '01HWS000000000000000000P01',
      tool: 'shell',
      args: {},
      argsSummary: '{}',
      reason: 'user_approval',
    });
    expect(store.pendingPermissions).toHaveLength(1);
    store.detach();
    expect(store.pendingPermissions).toEqual([]);
  });
});

describe('SessionStore — restore pending permissions on attach', () => {
  it('loads pending requests from the backend on attach', async () => {
    stub.permissionsList.mockResolvedValue([
      {
        requestId: '01HWS000000000000000000P01',
        runId: '01HWS000000000000000000R0N',
        sessionId: SESSION_ID,
        tool: 'shell',
        args: { command: 'ls' },
        argsSummary: '{ "command": "ls" }',
        reason: 'user_approval',
        createdAt: '2026-06-06T10:00:00.000Z',
      },
    ]);
    const store = new SessionStore();
    await store.attach(WS_ID);
    expect(stub.permissionsList).toHaveBeenCalled();
    expect(store.pendingPermissions).toHaveLength(1);
    expect(store.currentPermission?.tool).toBe('shell');
  });
});

describe('SessionStore — chat.run.aborted.v1', () => {
  it('surfaces a transient error message when the run is aborted', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    store.activeSession = makeSummary();
    await store.send('hello');
    expect(store.runStatus).toBe('running');
    fireAborted({ runId: '01HWS000000000000000000R0N', reason: 'user' });
    expect(store.lastError?.code).toBe('aborted');
    expect(store.lastError?.message).toContain('user');
  });

  it('ignores the aborted event for a different run', async () => {
    const store = new SessionStore();
    await store.attach(WS_ID);
    store.activeSession = makeSummary();
    await store.send('hello');
    fireAborted({ runId: '01HWS000000000000000000RZZ', reason: 'user' });
    expect(store.lastError).toBeNull();
  });
});
