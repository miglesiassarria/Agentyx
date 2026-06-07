/// Session state — Svelte 5 runes-based store for the chat surface (F01).
///
/// One instance per workspace. Owns:
/// - The active session (id, summary, active agent).
/// - The message list (persisted + streaming).
/// - The active run (runId, status, last error).
/// - The agent registry (visible agents from `agents.list`).
///
/// All mutations go through methods. Components consume the
/// `$state` proxies reactively. The store also subscribes to
/// `chat.*.v1` Tauri events via `events.bindRun` and folds them
/// into the local state machine.
///
/// See `../../../specs/features/F01-chat-streaming.md` and
/// `../../../specs/domains/agent-loop.md` for the full contract.

import { events, session as sessionIpc, agents as agentsIpc, permissions } from '$lib/ipc';
import type {
  AgentId,
  AgentInfoDto,
  AtMention,
  ChatContentDeltaPayload,
  ChatMessageStartPayload,
  ChatRunErrorPayload,
  ChatRunFinishedPayload,
  ChatRunStartedPayload,
  MessageDto,
  PermissionRequestDto,
  PermissionRequestedPayload,
  RunHandleDto,
  RunId,
  SessionId,
  SessionSummaryDto,
  StreamingMessage,
  WorkspaceId,
} from '$lib/ipc-types';

type PermissionResponse =
  | { kind: 'allowOnce' }
  | { kind: 'allowSession' }
  | { kind: 'allowAlways'; tool: string }
  | { kind: 'deny' };

/// Run lifecycle status in the UI store. Mirrors the Rust `RunStatus`
/// but adds a transient `starting` state (between user submit and
/// the first `chat.run.started.v1` event).
export type UiRunStatus =
  | 'idle'
  | 'starting'
  | 'running'
  | 'completed'
  | 'aborted'
  | 'error'
  | 'timeout';

export interface UiError {
  code: string;
  message: string;
  retryable: boolean;
  /** ISO-8601 UTC of when the error was first set. */
  at: string;
}

interface SessionStoreInit {
  /** Initial agent id for new sessions; defaults to first visible primary. */
  defaultAgentId?: AgentId;
}

class SessionStore {
  // === Workspace-scoped state ===

  /** The workspace this store is bound to (set by `attach`). */
  workspaceId = $state<WorkspaceId | null>(null);

  /** Summary of the active session, or null if no session exists. */
  activeSession = $state<SessionSummaryDto | null>(null);

  /** Persisted + streaming messages, ordered by `seq` ASC. */
  messages = $state<StreamingMessage[]>([]);

  /** Active run state. `null` when no run is in flight. */
  runId = $state<RunId | null>(null);
  runStatus = $state<UiRunStatus>('idle');

  /** The most recent run error (cleared on the next `send`). */
  lastError = $state<UiError | null>(null);

  /** True while a `send` is in flight but no `chat.run.started.v1` arrived yet. */
  starting = $state<boolean>(false);

  /** True while `loadHistory` / `create` is fetching the persisted state. */
  hydrating = $state<boolean>(false);

  /** Visible agents from the registry. Populated by `loadAgents`. */
  agents = $state<AgentInfoDto[]>([]);

  /**
   * Queue of pending permission requests (F01.AC7). The UI
   * surfaces the first one in a modal; subsequent ones stack
   * behind it.
   */
  pendingPermissions = $state<PermissionRequestDto[]>([]);

  // === Private state ===

  /** Monotonically increasing seq for synthetic messages (user input, optimistic). */
  #nextLocalSeq = 0;
  /** Currently-bound unlisten handle for chat events. */
  #unbindRun: (() => void) | null = null;
  /** Currently-bound unlisten handle for permission events. */
  #unbindPermission: (() => void) | null = null;
  /** AbortController for in-flight loaders (so we can cancel on switch). */
  #abortCtl: AbortController | null = null;

  // === Derived ===

  /** Subagents available for `@mention` (mode = subagent, not hidden). */
  subagents = $derived(this.agents.filter((a) => a.mode === 'subagent' && !a.hidden));

  /** Primary agents available for cycle-with-Tab (excludes hidden). */
  primaryAgents = $derived(
    this.agents
      .filter((a) => a.mode === 'primary' && !a.hidden)
      .sort((a, b) => a.id.localeCompare(b.id)),
  );

  /** True when the composer should be disabled (a run is in flight). */
  composerDisabled = $derived(this.runStatus === 'starting' || this.runStatus === 'running');

  /** The currently-active agent's spec (resolved from `agents` list). */
  activeAgent = $derived<AgentInfoDto | null>(
    this.activeSession === null
      ? null
      : (this.agents.find((a) => a.id === this.activeSession?.activeAgent) ?? null),
  );

  /** The first pending permission request (or null). The modal
   * renders this; subsequent requests stack behind it. */
  currentPermission = $derived<PermissionRequestDto | null>(this.pendingPermissions[0] ?? null);

  // === Lifecycle ===

  /**
   * Bind the store to a workspace. Loads the agent registry and
   * (re)hydrates any active session if one exists. Safe to call
   * multiple times; the previous binding is torn down.
   */
  async attach(workspaceId: WorkspaceId): Promise<void> {
    this.#teardown();
    this.workspaceId = workspaceId;
    this.#abortCtl = new AbortController();
    this.#bindPermissionEvents();
    // Restore any pending requests that were left over from a
    // prior page session (the agent loop keeps them in
    // PermissionRegistry across the reload).
    try {
      const list = await permissions.list();
      this.pendingPermissions = list.map(toPermissionRequest);
    } catch (e) {
      console.warn('permissions.list failed:', e);
    }
    try {
      await this.loadAgents();
    } catch (e) {
      // Agents failing to load is non-fatal; chat still works without
      // @mention popover. Log to console for the user.
      console.warn('agents.list failed:', e);
    }
  }

  /**
   * Unbind from the current workspace. Cancels in-flight loaders
   * and unbinds event listeners. State is preserved (the chat
   * panel will re-hydrate on the next `attach`).
   */
  detach(): void {
    this.#teardown();
  }

  #teardown(): void {
    this.#abortCtl?.abort();
    this.#abortCtl = null;
    if (this.#unbindRun !== null) {
      this.#unbindRun();
      this.#unbindRun = null;
    }
    if (this.#unbindPermission !== null) {
      this.#unbindPermission();
      this.#unbindPermission = null;
    }
    this.runId = null;
    this.runStatus = 'idle';
    this.starting = false;
    this.messages = [];
    this.activeSession = null;
    this.lastError = null;
    this.agents = [];
    this.pendingPermissions = [];
  }

  // === Loaders ===

  /** Fetch the agent registry (visible only). */
  async loadAgents(): Promise<void> {
    const list = await agentsIpc.list();
    this.agents = list;
  }

  /**
   * Create a new session in the current workspace and start hydrating
   * its history. The session is persisted on the backend; we only
   * mirror the summary locally.
   */
  async createSession(title?: string): Promise<SessionSummaryDto> {
    const wsId = this.workspaceId;
    if (wsId === null) throw new Error('SessionStore: attach() not called');
    this.hydrating = true;
    this.lastError = null;
    try {
      const summary = await sessionIpc.create(wsId, undefined, title);
      this.activeSession = summary;
      this.messages = [];
      return summary;
    } finally {
      this.hydrating = false;
    }
  }

  /**
   * Load the persisted history of the current session. Called on
   * cold start (when the backend already knows the active session)
   * or after a session switch.
   */
  async loadHistory(sessionId: SessionId, limit?: number): Promise<void> {
    this.hydrating = true;
    this.lastError = null;
    try {
      const list = await sessionIpc.getHistory(sessionId, limit);
      this.messages = list.map((m) => toStreaming(m, 'complete'));
    } finally {
      this.hydrating = false;
    }
  }

  /** Set the active session (e.g. picked from a sidebar list). */
  setActiveSession(summary: SessionSummaryDto): void {
    this.activeSession = summary;
    void this.loadHistory(summary.id);
  }

  // === Send / abort ===

  /**
   * Send a user message. Optimistically appends the user message to
   * the local list, then fires the IPC. While the run is in flight,
   * binds the chat.*.v1 listeners for that `runId`.
   *
   * Throws if the composer should not be enabled (already running).
   */
  async send(content: string, mentions: AtMention[] = []): Promise<void> {
    if (this.composerDisabled) {
      throw new Error('SessionStore.send: a run is already in progress');
    }
    if (this.activeSession === null) {
      throw new Error('SessionStore.send: no active session');
    }
    const trimmed = content.trim();
    if (trimmed.length === 0) return;

    this.lastError = null;
    this.runStatus = 'starting';

    // Optimistic user message (no real id/seq until persisted; we
    // use a synthetic seq that we replace after loadHistory or
    // when the backend reflects it back via events).
    const localSeq = this.#allocateLocalSeq();
    const userMessage: StreamingMessage = {
      id: `local-${localSeq}`,
      sessionId: this.activeSession.id,
      runId: null,
      role: 'user',
      content: trimmed,
      seq: localSeq,
      createdAt: new Date().toISOString(),
      status: 'complete',
      isStreaming: false,
    };
    this.messages = [...this.messages, userMessage];

    let handle: RunHandleDto;
    try {
      handle = await sessionIpc.send(this.activeSession.id, trimmed, mentions);
    } catch (e) {
      // The send failed before the run started; surface and stay idle.
      this.runStatus = 'error';
      this.lastError = normalizeError(e);
      // Drop the optimistic user message so the user can retry cleanly.
      this.messages = this.messages.filter((m) => m.id !== userMessage.id);
      throw e;
    }

    this.runId = handle.runId;
    this.runStatus = 'running';
    this.#bindRunEvents(handle.runId);
  }

  /**
   * Abort the active run. Idempotent: a no-op if no run is running.
   * The actual `chat.run.aborted.v1` event will arrive via the
   * event subscription; we just set the local intent here.
   */
  async abort(): Promise<void> {
    if (this.activeSession === null) return;
    if (this.runStatus !== 'running' && this.runStatus !== 'starting') return;
    try {
      await sessionIpc.abort(this.activeSession.id);
    } catch (e) {
      // Surface but don't throw — the user clicked stop.
      console.warn('session.abort failed:', e);
    }
  }

  /**
   * Change the active agent of the session. The change persists
   * on the backend; we update the local `activeSession.activeAgent`
   * optimistically. Throws if a run is in flight (backend returns
   * `Conflict`).
   */
  async setActiveAgent(agentId: AgentId): Promise<void> {
    if (this.activeSession === null) throw new Error('no active session');
    if (this.composerDisabled) {
      throw new Error('cannot change agent while a run is in progress');
    }
    const prev = this.activeSession.activeAgent;
    try {
      await sessionIpc.setActiveAgent(this.activeSession.id, agentId);
      this.activeSession = { ...this.activeSession, activeAgent: agentId };
    } catch (e) {
      // Keep the previous agent; surface the error.
      this.lastError = normalizeError(e);
      throw e;
    }
    return void prev;
  }

  // === Event subscriptions ===

  #bindPermissionEvents(): void {
    if (this.#unbindPermission !== null) {
      this.#unbindPermission();
      this.#unbindPermission = null;
    }
    void events
      .permissionRequested((p) => this.#onPermissionRequested(p))
      .then((unbind) => {
        this.#unbindPermission = unbind;
      });
  }

  #onPermissionRequested(p: PermissionRequestedPayload): void {
    const req: PermissionRequestDto = {
      requestId: p.requestId,
      runId: p.runId,
      sessionId: p.sessionId,
      tool: p.tool,
      args: p.args,
      argsSummary: p.argsSummary,
      reason: p.reason,
      createdAt: new Date().toISOString(),
    };
    // Skip duplicates (a re-emit for an existing request_id).
    if (this.pendingPermissions.some((r) => r.requestId === req.requestId)) return;
    this.pendingPermissions = [...this.pendingPermissions, req];
  }

  /**
   * Deliver the user's response to a pending permission request.
   * Calls the `permission_respond` Tauri command and removes
   * the request from the local queue.
   */
  async respondToPermission(requestId: string, response: PermissionResponse): Promise<void> {
    // Optimistic remove; if the IPC fails, re-add on catch.
    const prev = this.pendingPermissions;
    this.pendingPermissions = prev.filter((r) => r.requestId !== requestId);
    try {
      await permissions.respond(requestId, response);
    } catch (e) {
      this.pendingPermissions = prev;
      this.lastError = normalizeError(e);
      throw e;
    }
  }

  #bindRunEvents(runId: RunId): void {
    // Defensive: if a previous binding is still live (shouldn't
    // happen, but guards against double-`send` race), unbind first.
    if (this.#unbindRun !== null) {
      this.#unbindRun();
      this.#unbindRun = null;
    }
    void events
      .bindRun(runId, {
        onStarted: (p) => this.#onRunStarted(p),
        onMessageStart: (p) => this.#onMessageStart(p),
        onContentDelta: (p) => this.#onContentDelta(p),
        onFinished: (p) => this.#onRunFinished(p),
        onError: (p) => this.#onRunError(p),
        onAborted: (p) => this.#onRunAborted(p),
      })
      .then((unbind) => {
        this.#unbindRun = unbind;
      });
  }

  #onRunAborted(p: { runId: RunId; reason: string }): void {
    if (this.runId !== p.runId) return;
    // Surface a transient "Stopped" toast via `lastError` with a
    // synthetic code. The `chat.run.finished.v1` will arrive
    // right after with `status: "aborted"` and finalize the run.
    this.lastError = {
      code: 'aborted',
      message: `Run stopped (${p.reason})`,
      retryable: false,
      at: new Date().toISOString(),
    };
  }

  #onRunStarted(_p: ChatRunStartedPayload): void {
    // The `send` already moved us to `running`. This event is
    // mostly informational for the UI (e.g. a "Connecting..." →
    // "Receiving" transition). No state change required for v0.1.
    if (this.runId === _p.runId) this.runStatus = 'running';
  }

  #onMessageStart(p: ChatMessageStartPayload): void {
    // A new assistant message is beginning. Append a streaming
    // placeholder. We don't know `seq` yet; allocate a local one
    // that will be reconciled on the next `loadHistory` call.
    const localSeq = this.#allocateLocalSeq();
    const placeholder: StreamingMessage = {
      id: p.messageId,
      sessionId: this.activeSession?.id ?? '',
      runId: p.runId,
      role: 'assistant',
      content: '',
      seq: localSeq,
      createdAt: new Date().toISOString(),
      status: 'streaming',
      isStreaming: true,
    };
    this.messages = [...this.messages, placeholder];
  }

  #onContentDelta(p: ChatContentDeltaPayload): void {
    const idx = this.messages.findIndex((m) => m.id === p.messageId);
    if (idx === -1) {
      // Stale delta for a message we don't know about. The
      // placeholder should have been created by `onMessageStart`,
      // but if the first delta arrives before the start event we
      // may need to backfill. For now, ignore.
      return;
    }
    const current = this.messages[idx];
    if (current === undefined) return;
    const next: StreamingMessage = {
      ...current,
      content: current.content + p.text,
    };
    const copy = this.messages.slice();
    copy[idx] = next;
    this.messages = copy;
  }

  #onRunFinished(p: ChatRunFinishedPayload): void {
    if (this.runId !== p.runId) return;
    this.#unbindRun?.();
    this.#unbindRun = null;
    this.runId = null;
    this.runStatus = p.status;
    // Mark any in-flight streaming messages as complete/aborted.
    this.messages = this.messages.map((m) => {
      if (m.runId !== p.runId || m.status !== 'streaming') return m;
      return {
        ...m,
        status: p.status === 'completed' ? 'complete' : (p.status as StreamingMessage['status']),
        isStreaming: false,
      };
    });
    // Reload the persisted history so seq/id are canonical. The
    // optimistic local-* ids get replaced. This is a tiny extra
    // round-trip; in v1.x we'd reconcile incrementally.
    if (this.activeSession !== null) {
      void this.loadHistory(this.activeSession.id);
    }
  }

  #onRunError(p: ChatRunErrorPayload): void {
    if (this.runId !== p.runId) return;
    this.lastError = {
      code: p.code,
      message: p.message,
      retryable: p.retryable,
      at: new Date().toISOString(),
    };
    this.runStatus = 'error';
    this.messages = this.messages.map((m) => {
      if (m.runId !== p.runId || m.status !== 'streaming') return m;
      return { ...m, status: 'error', isStreaming: false };
    });
    // The run may still emit a `finished` event afterwards; the
    // unbind happens there.
  }

  // === Helpers ===

  /** Allocate a synthetic seq for optimistic/local messages. */
  #allocateLocalSeq(): number {
    // The next seq must be greater than the highest known seq so
    // the local message sorts after the persisted ones until
    // loadHistory reconciles it.
    const maxPersisted = this.messages.reduce((acc, m) => (m.seq > acc ? m.seq : acc), 0);
    this.#nextLocalSeq += 1;
    return maxPersisted + this.#nextLocalSeq;
  }

  /**
   * Cycle to the next primary agent. UX: Tab in the composer. If
   * only one primary exists, this is a no-op. v0.1: invoked
   * programmatically from the chip; the keyboard handler lives in
   * the composer component.
   */
  async cyclePrimary(): Promise<void> {
    const primaries = this.primaryAgents;
    if (primaries.length < 2) return;
    const currentId = this.activeSession?.activeAgent;
    const idx = primaries.findIndex((a) => a.id === currentId);
    const next = primaries[(idx + 1 + primaries.length) % primaries.length];
    if (next === undefined) return;
    await this.setActiveAgent(next.id);
  }
}

// === Module-level singleton ===

export const sessionStore = new SessionStore();

// === Utilities ===

function toStreaming(m: MessageDto, status: StreamingMessage['status']): StreamingMessage {
  return {
    ...m,
    status,
    isStreaming: false,
  };
}

function toPermissionRequest(p: PermissionRequestDto): PermissionRequestDto {
  return {
    requestId: p.requestId,
    runId: p.runId,
    sessionId: p.sessionId,
    tool: p.tool,
    args: p.args,
    argsSummary: p.argsSummary,
    reason: p.reason,
    createdAt: p.createdAt,
  };
}

function normalizeError(e: unknown): UiError {
  if (e instanceof Error) {
    const err = e as Error & { code?: string };
    return {
      code: err.code ?? 'unknown',
      message: err.message,
      retryable: false,
      at: new Date().toISOString(),
    };
  }
  return {
    code: 'unknown',
    message: String(e),
    retryable: false,
    at: new Date().toISOString(),
  };
}

// Re-export for tests
export { SessionStore };
export type { SessionStoreInit };
