/// Shared types between Rust and the Svelte UI.
///
/// Mirrors of the DTOs defined in `crates/agentyx-app/src/commands/*`
/// and `crates/agentyx-core/src/**/...rs`. Keep in sync.

export type AgentId = string;
export type SessionId = string;
export type RunId = string;
export type WorkspaceId = string;
export type ToolId = string;

/** A single directory entry returned by `workspace.list_dir`. */
export interface FileEntryDto {
  /** Basename (no path separators). */
  name: string;
  /** Absolute canonical path of the entry. */
  path: string;
  /** Whether the entry is a directory (resolved through symlinks). */
  isDir: boolean;
  /** Whether the entry is itself a symbolic link. */
  isSymlink: boolean;
  /** File size in bytes (0 for directories or on stat failure). */
  size: number;
  /** Last-modified time in epoch milliseconds (0 if unavailable). */
  modifiedAt: number;
}

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
