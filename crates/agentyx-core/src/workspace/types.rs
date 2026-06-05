//! Workspace data types.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ids::WorkspaceId;

/// A registered workspace — a project the user has opened, with
/// its `root_path` and 0..N `extra_paths` (per ADR-0007).
///
/// The `Workspace` is the **in-memory** representation. Persistence
/// is handled by [`crate::workspace::WorkspaceRegistry`] (the
/// `state.json` registry) and (in a follow-up PR) by the
/// `workspaces` table in `state.db`.
///
/// All paths are stored canonical (see
/// [`crate::workspace::paths::canonicalize`]). Two workspaces
/// cannot share a `root_path` (Open question Q4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    /// The workspace's unique id.
    pub id: WorkspaceId,
    /// Canonical absolute path of the workspace root.
    pub root_path: PathBuf,
    /// Display name (defaults to the folder basename on open).
    pub name: String,
    /// Epoch milliseconds when the workspace was first opened.
    pub created_at: i64,
    /// Epoch milliseconds when the workspace was last opened.
    /// Updated by `WorkspaceService::open` (idempotent) and
    /// `AgentLoop::start` (per Open question Q5).
    pub last_opened_at: i64,
    /// Additional R/W directories the user has authorized.
    /// See ADR-0007 and `WorkspaceService::add_extra_path`.
    pub extra_paths: Vec<ExtraPath>,
}

/// A single extra path entry. Belongs to a [`Workspace`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraPath {
    /// Canonical absolute path. Must not equal the parent
    /// workspace's `root_path` (enforced by `add_extra_path`).
    pub path: PathBuf,
    /// Optional human-readable label. Shown in the UI sidebar.
    /// Defaults to the folder basename when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Epoch milliseconds when the entry was added.
    pub added_at: i64,
}

/// Per-workspace configuration, persisted as
/// `<workspace_dir>/config.toml`. This is the v1 schema; the
/// `version` field is required.
///
/// Most fields are optional. Missing fields fall back to the
/// defaults defined in [`WorkspaceConfig::default`]. Unknown
/// fields are collected into [`WorkspaceConfig::extra`] (a
/// flat `BTreeMap`) so user-added `[tool.X]` sections
/// round-trip cleanly. Per workspace.md §Open questions Q2.
///
/// Note: no `Eq` derive because `toml::Value` only implements
/// `PartialEq` (TOML floats can be NaN, which breaks `Eq`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceConfig {
    /// Schema version. Must be `1` in v0.1.
    pub version: u32,
    /// Display name (overrides the registry's `name`).
    #[serde(default)]
    pub name: Option<String>,
    /// Epoch ms. Immutables (cannot be changed by `set_config`).
    pub created_at: i64,
    /// Provider override for this workspace (e.g. use `groq` even
    /// though the global default is `ollama`).
    #[serde(default)]
    pub provider: Option<ProviderRef>,
    /// Venv override. If `None`, the system falls back to
    /// `detect_venv` per ADR-0004. Opt-in in v0.1.
    #[serde(default)]
    pub venv: Option<VenvConfig>,
    /// Glob patterns to ignore in `search` and the file watcher.
    /// Defaults to a sensible set in [`WorkspaceConfig::default`].
    #[serde(default)]
    pub ignore: Vec<String>,
    /// Per-workspace permission matrix. Merged with the global
    /// defaults per `permissions.md` §Algoritmo.
    #[serde(default)]
    pub permissions: Option<PermissionsConfig>,
    /// Extra paths (synced with the registry on `add_extra_path` /
    /// `remove_extra_path`).
    #[serde(default)]
    pub extra_paths: Vec<ExtraPath>,
    /// Catch-all for unknown sections (e.g. `[tool.X]`). Not
    /// validated; we round-trip them but don't expose them in
    /// the typed API. Per workspace.md §Open questions Q2.
    #[serde(default, flatten)]
    pub extra: std::collections::BTreeMap<String, toml::Value>,
}

/// Provider/model override in the workspace config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRef {
    /// Provider id, e.g. `"ollama"`, `"groq"`, `"minimax"`.
    pub id: String,
    /// Model id within the provider, e.g. `"llama3.1:8b"`.
    pub model: String,
}

/// Venv config (override; absent = auto-detect per ADR-0004).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VenvConfig {
    /// Path to the venv. If empty, fall back to auto-detect.
    #[serde(default)]
    pub path: PathBuf,
    /// Which backend to use when `create_venv` is called.
    /// `"uv"` if available, else `"venv"`.
    #[serde(default = "default_venv_backend")]
    pub backend: VenvBackend,
}

fn default_venv_backend() -> VenvBackend {
    VenvBackend::Uv
}

/// Backend to use when creating a venv.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VenvBackend {
    /// `uv` (https://github.com/astral-sh/uv). Preferred.
    Uv,
    /// `python -m venv` (stdlib).
    Venv,
}

/// Per-workspace permission matrix (a subset of `permissions.md`).
/// Full per-tool allow/ask/deny rules are modeled in
/// `permissions.md`; here we just carry the configuration shape.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionsConfig {
    /// Tools allowed without prompting in this workspace.
    /// Glob-style patterns (e.g. `"read_file"`, `"shell:git *"`).
    #[serde(default)]
    pub allow: Vec<String>,
    /// Tools explicitly denied in this workspace.
    #[serde(default)]
    pub deny: Vec<String>,
    /// Tools that always require an interactive approval prompt.
    #[serde(default)]
    pub ask: Vec<String>,
    /// Fine-grained rules applied **within** the extra paths.
    /// The extra paths themselves are accessible by virtue of
    /// being declared; this is for "dentro de este extra, no
    /// toques X".
    #[serde(default)]
    pub extra_paths: Option<ExtraPathsPermissionsConfig>,
}

/// Inner config for the `[permissions.extra_paths]` block.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraPathsPermissionsConfig {
    /// Glob-style patterns for paths **inside** the extra paths
    /// that are denied (e.g. `"**/.env"`).
    #[serde(default)]
    pub deny: Vec<String>,
}

/// Detected virtual environment (read-only — see ADR-0004).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VenvSpec {
    /// Which venv kind was detected.
    pub kind: VenvKind,
    /// Path to the venv directory.
    pub path: PathBuf,
    /// Path to the Python executable inside the venv.
    pub python: PathBuf,
    /// Reported Python version (e.g. `"3.12.1"`).
    pub version: String,
}

/// Venv implementation (per ADR-0004).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VenvKind {
    /// A `.venv/` (or similar) managed by `uv`.
    Uv,
    /// A venv created by `python -m venv`.
    Venv,
}

impl Default for WorkspaceConfig {
    /// Returns a config with sensible v0.1 defaults, suitable for
    /// `WorkspaceService::open` to seed a brand-new workspace.
    fn default() -> Self {
        Self {
            version: 1,
            name: None,
            created_at: 0,
            provider: None,
            venv: None,
            ignore: vec![
                ".git".into(),
                "node_modules".into(),
                "target".into(),
                "__pycache__".into(),
                ".venv".into(),
                "venv".into(),
                "dist".into(),
                "build".into(),
                ".next".into(),
                ".cache".into(),
            ],
            permissions: Some(PermissionsConfig::default()),
            extra_paths: Vec::new(),
            extra: std::collections::BTreeMap::new(),
        }
    }
}

impl WorkspaceConfig {
    /// Returns the default config with the `created_at` populated.
    /// Use this when seeding a brand-new workspace.
    #[must_use]
    pub fn new_default(created_at: i64) -> Self {
        Self {
            created_at,
            ..Self::default()
        }
    }

    /// Keys that cannot be modified by `WorkspaceService::set_config`.
    /// Per workspace.md §Operations `Workspace::set_config`.
    pub const IMMUTABLE_KEYS: &'static [&'static str] = &["id", "created_at"];

    /// Returns true if `key` is an immutable key.
    #[must_use]
    pub fn is_immutable_key(key: &str) -> bool {
        Self::IMMUTABLE_KEYS.contains(&key)
    }
}
