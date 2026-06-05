//! Agentyx SDK — reusable Rust API for embedding Agentyx in other apps.
//!
//! This crate is a thin façade over `agentyx-core` that exposes a
//! stable, minimal API surface for third-party integrations. It is
//! **not** used by `agentyx-app` (the desktop shell) directly —
//! that binary depends on `agentyx-core` for full control.
//!
//! Scope (planned, see `../../specs/architecture.md`):
//! - `Agent` — start a session in a workspace, send messages, stream
//!   events. Async, cancellable.
//! - `Workspace` — open/list workspaces programmatically.
//! - `Config` — load/save `~/.agentyx/config.toml` (read-only here;
//!   full config editing is internal to the app).
//!
//! Non-goals (in v0.1):
//! - No CLI binary (use the Tauri app).
//! - No FFI bindings (consider `agentyx-sdk-ffi` in v1.x).
//! - No JS/TS bindings (consider `napi-rs` in v1.x).
//!
//! See `../../specs/architecture.md` §"agentyx-sdk" for the
//! long-term design.

#![deny(unsafe_code)]
#![warn(missing_docs)]

/// Re-export of the underlying `agentyx_core` for convenience.
pub use agentyx_core;

/// Re-export the tracing macros for SDK users that don't want a
/// direct dep on `tracing`.
pub use ::tracing;
