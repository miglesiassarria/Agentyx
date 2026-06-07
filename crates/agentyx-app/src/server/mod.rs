//! Embedded HTTP server (F06) — modules re-exported for convenience.

pub mod auth;
pub mod handlers;
pub mod info;
pub mod lifecycle;
pub mod router;
pub mod state;
pub mod static_files;

#[cfg(test)]
mod tests;

pub use state::ServerConfig;
