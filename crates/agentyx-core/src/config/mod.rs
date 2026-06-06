//! Config — global + per-workspace TOML configuration.
//!
//! See `../../../specs/domains/config.md` for the full design.
//!
//! ## What's in this PR (F01-Phase1)
//!
//! - `GlobalConfig` data type (subset: provider list, default
//!   provider/model, approval mode, ui).
//! - `ConfigService::load_global()` — reads `~/.agentyx/config.toml`,
//!   creates defaults if missing.
//! - `ConfigService::provider_config(id) -> Option<ProviderConfig>`
//!   — for the agent loop to instantiate a provider.
//! - `ConfigService::update_global(patch)` — atomic write with
//!   `.bak` backup.
//! - `SecretRef` parsing (`env:VAR`, `keychain:account`).
//! - `KeychainAccess` trait + `OsKeychain` impl (skeleton;
//!   tests use `FakeKeychain`).
//! - Atomic TOML write with `.toml.tmp` + rename.
//!
//! ## Deferred
//!
//! - Workspace config resolution (per-config.md, this lands in
//!   F05 when the UI lands).
//! - Full validation (font size, URL parse, etc.) — basic only.
//! - `SecretRef::Keychain` round-trip via `OsKeychain` (the
//!   trait is in place; F05 wires the keychain call sites).
//! - 18-AC coverage of config.md (subset implemented).

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod keychain;
mod schema;
mod service;

pub use keychain::{FakeKeychain, KeychainAccess, OsKeychain};
pub use schema::{
    ApprovalMode, GlobalConfig, ProviderConfig, SecretRef, Theme, UiConfig, UpdateChannel,
};
pub use service::ConfigService;
