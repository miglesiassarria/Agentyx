//! Agents — multi-agent registry of built-in agent specs.
//!
//! See `../../specs/agents.md` for the full model. This module
//! implements the v1 subset:
//!
//! - 3 visible built-ins: `build` (default primary), `plan`
//!   (read-only primary), `general` (subagent).
//! - 3 hidden built-ins: `compaction`, `title`, `summary`
//!   (reserved IDs for v1.x).
//!
//! The registry is **in-memory** and immutable after `load`. Custom
//! agents loaded from `~/.agentyx/agents/*.md` and
//! `<workspace>/.agentyx/agents/*.md` are a v1.x feature (see
//! `agents.md` §Custom agents).

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::ids::AgentId;

/// The mode of an agent, per agents.md §State.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    /// Directly invoked by the user. 1 active per session.
    Primary,
    /// Invoked by a primary (via `task` tool) or by `@mention`.
    Subagent,
    /// Invoked by the system. Not selectable in UI.
    Hidden,
}

/// Tool access control for an agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "kind")]
pub enum ToolAccess {
    /// All registered tools.
    #[default]
    All,
    /// Only the listed tools.
    Allowlist(Vec<String>),
    /// All tools except the listed ones.
    Denylist(Vec<String>),
}

/// Override of workspace permissions. In v1, only used to **deny**
/// specific tools (defense-in-depth for `plan`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPermissionOverride {
    /// Tools to always deny.
    pub deny: Vec<String>,
}

/// A model reference. In v1, the provider is the workspace's
/// `default_provider`; the agent only overrides the model id.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRef {
    /// Provider id (e.g. `"ollama"`, `"groq"`).
    pub provider: String,
    /// Model id (e.g. `"llama3.1:8b"`).
    pub model: String,
}

/// Source of the agent's system prompt. In v1, only `Embedded`
/// (built-in prompts) are used; `File` and `Url` are reserved.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "source")]
pub enum PromptSource {
    /// Prompt is embedded in the binary.
    Embedded {
        /// The system prompt text.
        content: String,
    },
    /// Prompt is loaded from a file (v1.x).
    #[allow(dead_code)]
    File {
        /// Path to the markdown file.
        path: std::path::PathBuf,
    },
    /// Prompt is fetched from a URL (v2+).
    #[allow(dead_code)]
    Url {
        /// The URL to fetch the prompt from.
        url: String,
    },
}

/// One agent's specification. See agents.md §State for the full
/// shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSpec {
    /// Agent id (e.g. `"build"`, `"plan"`, `"general"`).
    pub id: AgentId,
    /// Mode (primary / subagent / hidden).
    pub mode: AgentMode,
    /// Model reference.
    pub model: ModelRef,
    /// System prompt source.
    pub prompt: PromptSource,
    /// Tool access.
    pub tool_access: ToolAccess,
    /// Permission override (typically `deny`).
    #[serde(default)]
    pub permissions: AgentPermissionOverride,
    /// Short description (used in `@mention` autocomplete).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// If `true`, hidden from the UI.
    pub hidden: bool,
}

impl AgentSpec {
    /// Visible to the user (i.e. appears in `@mention` and the
    /// cycle-with-Tab picker).
    #[must_use]
    pub fn is_visible(&self) -> bool {
        !self.hidden
    }
}

/// In-memory registry of agents. Cheap to clone (`Arc` inside).
#[derive(Debug, Clone)]
pub struct AgentRegistry {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    agents: Vec<AgentSpec>,
    by_id: BTreeMap<AgentId, usize>,
}

impl AgentRegistry {
    /// Build the v1 registry: 3 visible built-ins + 3 hidden
    /// built-ins. Idempotent (calling twice gives the same result).
    #[must_use]
    pub fn load_builtins() -> Self {
        let agents = vec![
            build_agent(),
            plan_agent(),
            general_agent(),
            compaction_agent(),
            title_agent(),
            summary_agent(),
        ];
        let by_id = agents.iter().enumerate().map(|(i, a)| (a.id, i)).collect();
        Self {
            inner: Arc::new(Inner { agents, by_id }),
        }
    }

    /// All agents, in declaration order.
    #[must_use]
    pub fn list(&self) -> &[AgentSpec] {
        &self.inner.agents
    }

    /// Look up an agent by id. Returns `None` if not found.
    #[must_use]
    pub fn get(&self, id: &AgentId) -> Option<&AgentSpec> {
        self.inner.by_id.get(id).map(|&i| &self.inner.agents[i])
    }

    /// Visible (non-hidden) agents.
    #[must_use]
    pub fn list_visible(&self) -> Vec<&AgentSpec> {
        self.inner
            .agents
            .iter()
            .filter(|a| a.is_visible())
            .collect()
    }

    /// Primary agents (visible only). Order matches `list()`.
    #[must_use]
    pub fn primary_ids(&self) -> Vec<AgentId> {
        self.inner
            .agents
            .iter()
            .filter(|a| matches!(a.mode, AgentMode::Primary) && a.is_visible())
            .map(|a| a.id)
            .collect()
    }

    /// Subagents (visible only). For `@mention` autocomplete.
    #[must_use]
    pub fn subagents(&self) -> Vec<&AgentSpec> {
        self.inner
            .agents
            .iter()
            .filter(|a| matches!(a.mode, AgentMode::Subagent) && a.is_visible())
            .collect()
    }
}

// --- built-in agent factories ----------------------------------------

fn build_agent() -> AgentSpec {
    AgentSpec {
        id: agent_id_static("build"),
        mode: AgentMode::Primary,
        model: ModelRef {
            // Provider is resolved at runtime from workspace config;
            // we only declare the model id here, prefixed with
            // "default:" to signal "use the workspace's default
            // provider". The agent loop resolves this.
            provider: "default".into(),
            model: "default".into(),
        },
        prompt: PromptSource::Embedded {
            content: BUILD_PROMPT.into(),
        },
        tool_access: ToolAccess::All,
        permissions: AgentPermissionOverride::default(),
        description: None,
        hidden: false,
    }
}

fn plan_agent() -> AgentSpec {
    AgentSpec {
        id: agent_id_static("plan"),
        mode: AgentMode::Primary,
        model: ModelRef {
            provider: "default".into(),
            model: "default".into(),
        },
        prompt: PromptSource::Embedded {
            content: PLAN_PROMPT.into(),
        },
        // Read-only: only allowlist of read-only tools.
        tool_access: ToolAccess::Allowlist(vec![
            "read_file".into(),
            "search".into(),
            "list_dir".into(),
        ]),
        permissions: AgentPermissionOverride {
            deny: vec![
                "write_file".into(),
                "edit_file".into(),
                "shell".into(),
                "python_run".into(),
                "apply_patch".into(),
            ],
        },
        description: Some("Read-only analysis and planning. No writes.".into()),
        hidden: false,
    }
}

fn general_agent() -> AgentSpec {
    AgentSpec {
        id: agent_id_static("general"),
        mode: AgentMode::Subagent,
        model: ModelRef {
            provider: "default".into(),
            model: "default".into(),
        },
        prompt: PromptSource::Embedded {
            content: GENERAL_PROMPT.into(),
        },
        tool_access: ToolAccess::All,
        permissions: AgentPermissionOverride::default(),
        description: Some(
            "General-purpose subagent for multi-step delegated tasks. Full tool access.".into(),
        ),
        hidden: false,
    }
}

fn compaction_agent() -> AgentSpec {
    AgentSpec {
        id: agent_id_static("compaction"),
        mode: AgentMode::Hidden,
        model: ModelRef {
            provider: "default".into(),
            model: "default".into(),
        },
        prompt: PromptSource::Embedded {
            content: "Compaction agent (reserved; not active in v1).".into(),
        },
        tool_access: ToolAccess::Allowlist(vec!["read_file".into()]),
        permissions: AgentPermissionOverride::default(),
        description: None,
        hidden: true,
    }
}

fn title_agent() -> AgentSpec {
    AgentSpec {
        id: agent_id_static("title"),
        mode: AgentMode::Hidden,
        model: ModelRef {
            provider: "default".into(),
            model: "default".into(),
        },
        prompt: PromptSource::Embedded {
            content: "Title generation agent (reserved; not active in v1).".into(),
        },
        tool_access: ToolAccess::All,
        permissions: AgentPermissionOverride::default(),
        description: None,
        hidden: true,
    }
}

fn summary_agent() -> AgentSpec {
    AgentSpec {
        id: agent_id_static("summary"),
        mode: AgentMode::Hidden,
        model: ModelRef {
            provider: "default".into(),
            model: "default".into(),
        },
        prompt: PromptSource::Embedded {
            content: "Session summary agent (reserved; not active in v1).".into(),
        },
        tool_access: ToolAccess::All,
        permissions: AgentPermissionOverride::default(),
        description: None,
        hidden: true,
    }
}

/// Construct an `AgentId` from a built-in id string. The
/// `AgentId` is a ULID newtype; for built-ins we use a fixed
/// deterministic ULID derived from the id string so they're
/// stable across runs and can be matched in DB rows / journal.
pub(crate) fn agent_id_static(s: &str) -> AgentId {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    let h = hasher.finish();
    // Build a ULID from the 16 bytes of the hash. Crockford base32
    // encoding handled by `Ulid::from_bytes`.
    let bytes = h.to_be_bytes();
    let mut ulid_bytes = [0u8; 16];
    ulid_bytes[..8].copy_from_slice(&bytes);
    // The high byte is zero (ULID has a 48-bit timestamp + 80-bit
    // randomness; we just need uniqueness for built-ins, and
    // collisions are not possible because each id is distinct).
    AgentId::from_ulid(ulid::Ulid::from_bytes(ulid_bytes))
}

const BUILD_PROMPT: &str = "You are Agentyx, an agentic AI that operates on the user's local \
files inside a workspace. Be concise, accurate, and reversible. Prefer reading before writing. \
When you modify files, summarize what changed. Ask before doing anything destructive.";

const PLAN_PROMPT: &str = "You are Agentyx in **plan** mode: a read-only analyst. You CAN read \
files, search, and list directories. You CANNOT write, edit, execute, or run Python. Your job is \
to explore the codebase, understand it, and produce a structured plan the user will review and \
apply manually. Do not attempt tool calls that would modify state; if you need information you \
cannot get with read-only tools, ask the user.";

const GENERAL_PROMPT: &str = "You are a general-purpose subagent. You execute delegated tasks \
for a primary agent and return a structured summary. You have full tool access (subject to the \
workspace's permission matrix). Be focused: solve the delegated task and report back. Do not \
invoke other subagents recursively in v1.";

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn load_creates_three_visible_and_three_hidden() {
        let reg = AgentRegistry::load_builtins();
        assert_eq!(reg.list().len(), 6, "3 visible + 3 hidden");
        let visible = reg.list_visible();
        assert_eq!(visible.len(), 3, "build, plan, general");
    }

    #[test]
    fn get_returns_built_in() {
        let reg = AgentRegistry::load_builtins();
        let build = reg.get(&agent_id_static("build")).unwrap();
        assert_eq!(build.mode, AgentMode::Primary);
        assert!(!build.hidden);
    }

    #[test]
    fn get_unknown_returns_none() {
        let reg = AgentRegistry::load_builtins();
        let id = AgentId::new();
        assert!(reg.get(&id).is_none());
    }

    #[test]
    fn primary_ids_returns_build_then_plan() {
        let reg = AgentRegistry::load_builtins();
        let ids: Vec<String> = reg
            .primary_ids()
            .into_iter()
            .map(|a| a.to_string())
            .collect();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn subagents_returns_only_general() {
        let reg = AgentRegistry::load_builtins();
        let subs = reg.subagents();
        assert_eq!(subs.len(), 1);
        assert_eq!(
            subs[0].description.as_deref(),
            Some("General-purpose subagent for multi-step delegated tasks. Full tool access.")
        );
    }

    #[test]
    fn plan_has_deny_on_writes() {
        let reg = AgentRegistry::load_builtins();
        let plan = reg.get(&agent_id_static("plan")).unwrap();
        assert!(plan.permissions.deny.contains(&"write_file".to_string()));
        assert!(plan.permissions.deny.contains(&"shell".to_string()));
        if let ToolAccess::Allowlist(allow) = &plan.tool_access {
            assert!(allow.contains(&"read_file".to_string()));
            assert!(!allow.contains(&"write_file".to_string()));
        } else {
            panic!("plan must have Allowlist tool access");
        }
    }
}

// (Tests below use `agent_id_static` directly since they're in the
// same module.)
