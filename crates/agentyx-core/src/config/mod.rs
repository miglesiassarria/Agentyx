//! Config — global + per-workspace TOML configuration.
//!
//! See `../../../specs/domains/config.md` for the full design.
//!
//! ## What's in this PR (F05 backend wiring)
//!
//! - `GlobalConfig` data type (provider list, default
//!   provider/model, approval mode, ui).
//! - `WorkspaceConfig` data type (overrides, ignore patterns,
//!   journal cap).
//! - `GlobalConfigPatch` / `WorkspaceConfigPatch` — partial updates
//!   used by the F05 Tauri commands.
//! - `ResolvedConfig` / `EffectiveConfig` — in-memory snapshots with
//!   secrets already expanded (never serialized to disk or to the
//!   IPC DTOs).
//! - `ConfigService::load_global()` — reads `~/.agentyx/config.toml`,
//!   creates defaults if missing.
//! - `ConfigService::load_workspace(workspace_id)` — reads
//!   `<home>/workspaces/<id>/config.toml`; returns `Ok(defaults())`
//!   if missing.
//! - `ConfigService::update_global(patch)` / `update_workspace(...)`
//!   — atomic write with `.bak` backup.
//! - `ConfigService::resolve_secrets()` — expands all `SecretRef`s
//!   against the keychain + env; returns `InvalidInput` for missing
//!   env vars, `Internal` for missing keychain entries.
//! - `SecretRef` parsing (`env:VAR`, `keychain:account`).
//! - `KeychainAccess` trait + `OsKeychain` impl (`keyring` crate).
//! - Atomic TOML write with `.toml.tmp` + rename + `0o600` perms.

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod keychain;
mod schema;
mod service;

pub use keychain::{FakeKeychain, KeychainAccess, OsKeychain};
pub use schema::{
    ApprovalMode, EffectiveConfig, GlobalConfig, GlobalConfigPatch, ProviderConfig, ProviderId,
    ResolvedConfig, SecretRef, Theme, ToolDecision, UiConfig, UpdateChannel, WorkspaceConfig,
    WorkspaceConfigPatch, WorkspaceSettings,
};
pub use service::{ConfigService, ResolvedConfigSnapshot, ServiceConfigPaths};
