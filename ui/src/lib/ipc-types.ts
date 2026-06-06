/// Shared types between Rust and the Svelte UI.
///
/// Mirrors of the DTOs defined in `crates/agentyx-app/src/commands/*`
/// and `crates/agentyx-core/src/**/...rs`. Keep in sync.
///
/// **Conventions**
/// - DTO field names (return values) are `camelCase` — the Rust side
///   uses `#[serde(rename_all = "camelCase")]` on the DTO structs.
/// - Tauri command **parameter** names are `snake_case` (Tauri 2
///   default). The wrapper functions in `lib/ipc.ts` convert the
///   typed args into the snake_case payload before calling `invoke`.

export type AgentId = string;
export type SessionId = string;
export type RunId = string;
export type WorkspaceId = string;
export type ToolId = string;
export type MessageId = string;

/** A single directory entry returned by `workspace.list_dir`. */
export interface FileEntryDto {
  name: string;
  path: string;
  isDir: boolean;
  isSymlink: boolean;
  size: number;
  modifiedAt: number;
}

/** `@<agent-id>` mention in a user message. */
export interface AtMention {
  agentId: AgentId;
  /** `[start, end)` character range in the original content. */
  range: [number, number];
}

/**
 * Handle returned by `session.send` to identify a running run.
 * The frontend listens for `chat.*.v1` events filtered by `runId`.
 */
export interface RunHandleDto {
  runId: RunId;
  sessionId: SessionId;
  agentId: AgentId;
  /** ISO-8601 UTC timestamp of when the run was created. */
  startedAt: string;
}

/** Session summary in the sidebar. */
export interface SessionSummaryDto {
  id: SessionId;
  workspaceId: WorkspaceId;
  activeAgent: AgentId;
  title: string;
  /** ISO-8601 UTC. */
  updatedAt: string;
  status: 'idle' | 'running' | 'aborted' | 'errored';
}

/** A persisted message (user / assistant / system / tool_result). */
export interface MessageDto {
  id: MessageId;
  sessionId: SessionId;
  runId: RunId | null;
  role: 'user' | 'assistant' | 'system' | 'tool_result';
  agentId?: AgentId;
  content: string;
  /** Sequence number within the session (ASC). */
  seq: number;
  /** ISO-8601 UTC. */
  createdAt: string;
}

/** Live message kept in the UI store during a streaming run. */
export interface StreamingMessage extends MessageDto {
  status: 'streaming' | 'complete' | 'aborted' | 'error';
  /** True while the message is accumulating deltas. */
  isStreaming: boolean;
}

/** A workspace in the sidebar / settings list. */
export interface WorkspaceDto {
  id: WorkspaceId;
  name: string;
  rootPath: string;
  extraPaths: ExtraPathDto[];
  hasVenv: boolean;
}

export interface ExtraPathDto {
  id: string;
  path: string;
  label: string;
  addedAt: number;
}

/** Effective paths the agent can operate on. */
export interface EffectivePathsDto {
  root: string;
  extras: string[];
}

/** A detected venv (returned by `workspace.detect_venv`). */
export interface VenvSpec {
  kind: 'uv' | 'venv';
  path: string;
  python: string;
  version: string;
}

/** Result of a `providers.test_connection` call. */
export interface TestConnectionResult {
  ok: boolean;
  latencyMs?: number;
  models: string[];
  error?: string;
  errorCode?: string;
}

/** Permission matrix for a tool × decision. */
export interface PermissionMatrixDto {
  global: Record<ToolId, 'allow' | 'ask' | 'deny'>;
  workspace?: Record<ToolId, 'allow' | 'ask' | 'deny'>;
  effective: Record<ToolId, 'allow' | 'ask' | 'deny'>;
}

/** DTO returned by `agents.list` / `agents.get`. */
export interface AgentInfoDto {
  id: AgentId;
  mode: 'primary' | 'subagent' | 'hidden';
  hidden: boolean;
  description?: string;
  name: string;
}

/** A run lifecycle event payload (subset shared by `run.started/finished`). */
export interface ChatRunEvent {
  sessionId: SessionId;
  runId: RunId;
  agentId: AgentId;
}

// ============================================================
// Chat event payload shapes (chat.*.v1)
// ============================================================

export interface ChatRunStartedPayload {
  runId: RunId;
  sessionId: SessionId;
  workspaceId?: WorkspaceId;
  agentId: AgentId;
  model?: string;
  /** ISO-8601 UTC. */
  startedAt: string;
}

export interface ChatMessageStartPayload {
  runId: RunId;
  messageId: MessageId;
  model?: string;
}

export interface ChatContentDeltaPayload {
  runId: RunId;
  sessionId: SessionId;
  messageId: MessageId;
  text: string;
}

export interface ChatRunFinishedPayload {
  runId: RunId;
  sessionId: SessionId;
  status: 'completed' | 'aborted' | 'error' | 'timeout';
  durationMs: number;
}

export interface ChatRunErrorPayload {
  runId: RunId;
  sessionId: SessionId;
  code: string;
  message: string;
  retryable: boolean;
}

/** Payload of `chat.run.aborted.v1` (F01.AC4 finalization). */
export interface ChatRunAbortedPayload {
  runId: RunId;
  sessionId: SessionId;
  reason: 'user' | 'timeout' | 'error' | 'max_steps' | 'aborted';
}

/** Payload of `permission.requested.v1` (F01.AC7). */
export interface PermissionRequestedPayload {
  runId: RunId;
  sessionId: SessionId;
  requestId: string;
  tool: ToolId;
  args: unknown;
  argsSummary: string;
  reason: string;
}

/** DTO returned by `permissions.list`. */
export interface PermissionRequestDto {
  requestId: string;
  runId: string;
  sessionId: string;
  tool: ToolId;
  args: unknown;
  argsSummary: string;
  reason: string;
  createdAt: string;
}
