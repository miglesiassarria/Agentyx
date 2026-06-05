//! Workspace domain — the root unit of isolation in Agentyx.
//!
//! See `../../../specs/domains/workspace.md` and
//! `../../../specs/adr/0007-extra-paths-per-workspace.md` for the
//! full design.
//!
//! ## Module layout
//!
//! - [`types`]    — `Workspace`, `ExtraPath`, `WorkspaceConfig`,
//!                  `VenvSpec`, `VenvKind` (the data types).
//! - [`paths`]    — `canonicalize`, `is_within`, root whitelist
//!                  (the security primitives).
//! - [`registry`] — `WorkspaceRegistry`, the global `state.json`
//!                  persistence layer.
//! - [`service`]  — high-level operations (`open`, `list`, `get`,
//!                  `delete`, `add_extra_path`, `remove_extra_path`,
//!                  `list_extra_paths`, `effective_paths`).
//! - [`venv`]     — `detect_venv` (basic detection only; creation
//!                  is a v0.1.x feature, see F03).
//!
//! ## What's in this PR
//!
//! - Domain types (the structs).
//! - Path security primitives (canonicalize, sandbox check,
//!   root whitelist per workspace.md §Open questions Q1).
//! - `WorkspaceRegistry` (load + save `state.json`, version 2).
//! - High-level service operations for the **core flow**:
//!   open / list / get / delete / add_extra_path /
//!   remove_extra_path / list_extra_paths / effective_paths.
//! - Basic `detect_venv` (read-only; no `uv` / `python -m venv`
//!   execution — that's v0.1.x with F03).
//! - Tests covering ~12 of the 24 ACs in `workspace.md`.
//!
//! ## What is deferred to follow-up PRs
//!
//! - `state.db` integration: the `workspaces` table is created in
//!   the storage PR; here we just open the registry.
//! - `create_venv` execution: needs `uv` / `python` subprocess
//!   handling (PTY / process spawn); v0.1.x with F03.
//! - `set_config` / `get_config` for the full per-workspace config:
//!   needs the `config.md` PR for the unified config system.
//! - `delete(force=true)` aborting active runs: needs the
//!   `agent-loop` PR (cross-crate coordination).
//! - Multi-process lock: `~/.agentyx/locks/open-<hash>` per Edge 5.
//!   In v0.1 we use a process-local `Mutex`; cross-process lock
//!   is a v1.x hardening.

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod paths;
pub mod registry;
pub mod service;
pub mod types;
pub mod venv;

pub use paths::{canonicalize, is_within, is_within_sandbox, root_whitelist};
pub use registry::{WorkspaceRegistry, REGISTRY_VERSION};
pub use service::{OpenOptions, WorkspaceService};
pub use types::{ExtraPath, VenvKind, VenvSpec, Workspace, WorkspaceConfig};
