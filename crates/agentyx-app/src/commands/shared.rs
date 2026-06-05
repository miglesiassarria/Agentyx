//! Re-exports and shared types for the `commands` module.

pub use super::session::*;

/// An `@<agent-id>` mention in a user message. Extracted from
/// `content` by the `AgentLoop::expand_at_mentions` function.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AtMention {
    /// The agent id (e.g. `"general"`).
    pub agent_id: agentyx_core::ids::AgentId,
    /// The character range in `content` that this mention covers.
    pub range: (usize, usize),
}
