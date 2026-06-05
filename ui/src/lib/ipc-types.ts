/// Shared types between Rust and the Svelte UI.
///
/// Mirrors of the DTOs defined in `crates/agentyx-app/src/commands/*`
/// and `crates/agentyx-core/src/**/...rs`. Keep in sync.

export type AgentId = string;
export type SessionId = string;
export type RunId = string;
export type WorkspaceId = string;
export type ToolId = string;

/** `@<agent-id>` mention in a user message. */
export interface AtMention {
  agentId: AgentId;
  /** Character range in the original content. */
  range: [number, number];
}

/** Handle returned by `session.send` to identify a running run. */
export interface RunHandle {
  runId: RunId;
}

/** Session summary in the sidebar. */
export interface SessionSummaryDto {
  id: SessionId;
  activeAgent: AgentId;
  title: string;
  updatedAt: number;
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
  /** Global defaults (per tool id). */
  global: Record<ToolId, 'allow' | 'ask' | 'deny'>;
  /** Per-workspace overrides (if any). */
  workspace?: Record<ToolId, 'allow' | 'ask' | 'deny'>;
  /** Effective matrix (workspace override > global). */
  effective: Record<ToolId, 'allow' | 'ask' | 'deny'>;
}

/** A chat message. */
export interface MessageDto {
  id: string;
  sessionId: SessionId;
  role: 'user' | 'assistant' | 'tool';
  agentId?: AgentId;
  content: string;
  createdAt: number;
  status: 'streaming' | 'complete' | 'aborted' | 'error';
}

/** A run lifecycle event payload. */
export interface ChatRunEvent {
  sessionId: SessionId;
  runId: RunId;
  agentId: AgentId;
}
