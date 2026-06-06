//! Permissions — who can do what.
//!
//! Per `specs/domains/permissions.md`, the [`PermissionGate`]
//! decides whether a tool call is allowed, denied, or needs
//! user approval. The gate is stateless; the snapshot is built
//! once per run from the workspace config + the global config +
//! the active agent's `AgentPermissionOverride`.
//!
//! This module also owns:
//! - [`PermissionRegistry`] — a thread-safe map of pending
//!   permission requests, indexed by `request_id`. The agent
//!   loop inserts when the gate returns `Ask`; the
//!   `permission_respond` Tauri command (cabled in a follow-up
//!   PR) removes and resolves.
//! - [`sandbox`] — path safety helpers shared with the tool
//!   implementations.

pub mod gate;
pub mod sandbox;

pub use gate::{
    ApprovalMode, CompiledGlobs, Decision, PendingPermission, PermissionGate, PermissionRegistry,
    PermissionRequest, PermissionSnapshot, UserDecision,
};
