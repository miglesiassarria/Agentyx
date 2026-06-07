//! Tools — capabilities the agent can invoke.
//!
//! Per `specs/domains/tools.md`, the [`Tool`] trait is the contract
//! every tool implements. The agent loop calls `run(ctx, args)`,
//! gets back a [`ToolOutput`], and emits the appropriate
//! `chat.tool_call.v1` / `chat.tool_result.v1` events.
//!
//! Tools are stateless between invocations. The agent loop holds
//! the [`ToolRegistry`] (a `Vec<Arc<dyn Tool>>`) and dispatches
//! by name.
//!
//! Path sandboxing is **enforced twice**: once by the permission
//! gate (before the tool is even invoked) and once inside the tool
//! itself (defense in depth). See [`crate::permissions`].

pub mod builtin;
pub mod types;

pub use types::{built_in_registry, find, names, schemas, Tool, ToolContext, ToolId, ToolOutput};
