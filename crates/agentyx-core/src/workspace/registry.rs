//! Global `state.json` registry of workspaces.
//!
//! Per workspace.md §State: the global registry lives at
//! `~/.agentyx/state.json` and lists every workspace Agentyx has
//! ever opened. The on-disk schema is versioned (`version: 2` in
//! v0.1, with `extra_paths` per workspace).
//!
//! This module is the **persistence** layer only. The
//! [`WorkspaceService`](super::service::WorkspaceService) is the
//! high-level API that mutates the registry and writes the
//! per-workspace `config.toml` and (in a follow-up) the
//! `state.db` row.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::AppError;

use super::paths::canonicalize;
use super::types::Workspace;

/// Current `state.json` schema version. v0.1 introduces per-workspace
/// `extra_paths`; we bumped from `1` to `2`. The migration is trivial
/// (existing workspaces get `extra_paths: []`); we still write `2`
/// on every save.
pub const REGISTRY_VERSION: u32 = 2;

/// In-memory representation of `state.json`. The on-disk file is
/// identical to the JSON form of this struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRegistry {
    /// Schema version. Must equal [`REGISTRY_VERSION`] on load.
    pub version: u32,
    /// All workspaces, in registry order (insertion order).
    /// Sorting for display happens at the service layer.
    pub workspaces: Vec<Workspace>,
    /// Global server settings (F06, v0.2). Optional in v0.1.
    #[serde(default)]
    pub server: ServerConfig,
}

/// Server section of `state.json` (F06 LAN server, deferred to v0.2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    /// Whether the LAN server is enabled. v0.1 always `false`.
    #[serde(default)]
    pub lan_enabled: bool,
    /// Bind address. v0.1 always `127.0.0.1`.
    #[serde(default)]
    pub bind: String,
}

impl Default for WorkspaceRegistry {
    fn default() -> Self {
        Self {
            version: REGISTRY_VERSION,
            workspaces: Vec::new(),
            server: ServerConfig::default(),
        }
    }
}

impl WorkspaceRegistry {
    /// Load the registry from `<agentyx_home>/state.json`. If the
    /// file does not exist, returns an empty registry.
    ///
    /// On any I/O or parse error, returns `AppError::Internal` with
    /// the underlying message. The file is **never** auto-overwritten;
    /// the user is expected to fix it manually (per Edge case 4).
    pub fn load(agentyx_home: &Path) -> Result<Self, AppError> {
        let path = agentyx_home.join("state.json");

        if !path.exists() {
            tracing::debug!(path = %path.display(), "state.json does not exist; returning empty registry");
            return Ok(Self::default());
        }

        let bytes = std::fs::read(&path).map_err(|e| AppError::Io {
            op: format!("read state.json ({})", path.display()),
            reason: e.to_string(),
        })?;

        let registry: Self = serde_json::from_slice(&bytes).map_err(|e| AppError::Internal {
            message: format!("state.json is malformed: {e}"),
        })?;

        if registry.version != REGISTRY_VERSION {
            return Err(AppError::Internal {
                message: format!(
                    "state.json version {} is not supported (expected {})",
                    registry.version, REGISTRY_VERSION
                ),
            });
        }

        Ok(registry)
    }

    /// Atomically write the registry to `<agentyx_home>/state.json`.
    /// Uses the `*.tmp + rename` pattern to ensure the file is
    /// never partially written.
    pub fn save(&self, agentyx_home: &Path) -> Result<(), AppError> {
        std::fs::create_dir_all(agentyx_home).map_err(|e| AppError::Io {
            op: format!("create_dir_all {}", agentyx_home.display()),
            reason: e.to_string(),
        })?;

        let final_path = agentyx_home.join("state.json");
        let tmp_path = agentyx_home.join("state.json.tmp");

        let bytes = serde_json::to_vec_pretty(self).map_err(|e| AppError::Internal {
            message: format!("serialize state.json: {e}"),
        })?;

        std::fs::write(&tmp_path, &bytes).map_err(|e| AppError::Io {
            op: format!("write state.json.tmp ({})", tmp_path.display()),
            reason: e.to_string(),
        })?;

        std::fs::rename(&tmp_path, &final_path).map_err(|e| AppError::Io {
            op: format!(
                "rename state.json.tmp -> state.json ({})",
                final_path.display()
            ),
            reason: e.to_string(),
        })?;

        // Best-effort: tighten permissions to 0o600 (user-only).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&final_path, std::fs::Permissions::from_mode(0o600));
        }

        Ok(())
    }

    /// Look up a workspace by id.
    #[must_use]
    pub fn get(&self, id: &crate::ids::WorkspaceId) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.id == *id)
    }

    /// Look up a workspace by id, mutably. Used by callers that
    /// need to update workspace fields in place (e.g. extra_paths,
    /// venv) while holding a registry lock.
    #[must_use]
    pub fn get_mut(&mut self, id: &crate::ids::WorkspaceId) -> Option<&mut Workspace> {
        self.workspaces.iter_mut().find(|w| w.id == *id)
    }

    /// Look up a workspace by `root_path` (canonical). Used by
    /// `WorkspaceService::open` to detect re-registration of an
    /// already-known workspace.
    #[must_use]
    pub fn find_by_root(&self, root_path: &Path) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| {
            w.root_path == canonicalize(root_path).unwrap_or_else(|_| root_path.to_path_buf())
        })
    }

    /// Insert a new workspace. If a workspace with the same
    /// `id` already exists, replace it. Returns a mutable
    /// reference to the inserted workspace.
    pub fn upsert(&mut self, workspace: Workspace) -> &mut Workspace {
        match self.workspaces.iter().position(|w| w.id == workspace.id) {
            Some(idx) => {
                self.workspaces[idx] = workspace;
                &mut self.workspaces[idx]
            }
            None => {
                self.workspaces.push(workspace);
                // SAFETY: we just pushed, so `last_mut` is guaranteed `Some`.
                self.workspaces
                    .last_mut()
                    .expect("invariant: just pushed a workspace")
            }
        }
    }

    /// Remove a workspace by id. Returns true if a workspace was
    /// actually removed.
    pub fn remove(&mut self, id: &crate::ids::WorkspaceId) -> bool {
        let len_before = self.workspaces.len();
        self.workspaces.retain(|w| w.id != *id);
        self.workspaces.len() != len_before
    }

    /// Returns the set of canonical root paths registered. Used by
    /// the `WorkspaceService::open` flow to detect nested workspaces
    /// (per Edge case 15: a workspace inside another is rejected).
    #[must_use]
    pub fn registered_roots(&self) -> Vec<PathBuf> {
        self.workspaces
            .iter()
            .map(|w| w.root_path.clone())
            .collect()
    }

    /// Returns true if `candidate` (canonical) is **inside** any
    /// registered workspace's `root_path`. Used to reject nested
    /// workspaces per Edge case 15.
    #[must_use]
    pub fn is_nested_workspace(&self, candidate: &Path) -> bool {
        for w in &self.workspaces {
            if super::paths::is_within(candidate, &w.root_path) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::ids::WorkspaceId;

    fn ws(id: &str, root: &str) -> Workspace {
        Workspace {
            id: WorkspaceId::from_ulid(ulid::Ulid::from_string(id).unwrap()),
            root_path: PathBuf::from(root),
            name: root.into(),
            created_at: 0,
            last_opened_at: 0,
            extra_paths: Vec::new(),
        }
    }

    #[test]
    fn empty_registry_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let r = WorkspaceRegistry::default();
        r.save(dir.path()).unwrap();
        let r2 = WorkspaceRegistry::load(dir.path()).unwrap();
        assert_eq!(r, r2);
    }

    #[test]
    fn load_missing_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let r = WorkspaceRegistry::load(dir.path()).unwrap();
        assert!(r.workspaces.is_empty());
        assert_eq!(r.version, REGISTRY_VERSION);
    }

    #[test]
    fn load_wrong_version_errors() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("state.json"),
            r#"{"version": 99, "workspaces": []}"#,
        )
        .unwrap();
        let err = WorkspaceRegistry::load(dir.path()).unwrap_err();
        assert!(matches!(err, AppError::Internal { .. }));
    }

    #[test]
    fn upsert_replaces_existing() {
        let mut r = WorkspaceRegistry::default();
        let w1 = ws("01H0000000000000000000XX0X", "/a");
        let id = w1.id;
        r.upsert(w1.clone());
        assert_eq!(r.workspaces.len(), 1);
        r.upsert(Workspace {
            name: "renamed".into(),
            ..w1.clone()
        });
        assert_eq!(r.workspaces.len(), 1);
        assert_eq!(r.workspaces[0].name, "renamed");
        assert_eq!(r.workspaces[0].id, id);
    }

    #[test]
    fn remove_returns_true_if_existed() {
        let mut r = WorkspaceRegistry::default();
        let w = ws("01H0000000000000000000XX0X", "/a");
        let id = w.id;
        r.upsert(w);
        assert!(r.remove(&id));
        assert!(!r.remove(&id));
    }

    #[test]
    fn is_nested_workspace_detects_inner() {
        let mut r = WorkspaceRegistry::default();
        r.upsert(ws("01H0000000000000000000XX0X", "/a"));
        r.upsert(ws("01H0000000000000000000XX0Y", "/a/b"));
        assert!(r.is_nested_workspace(Path::new("/a/b/c")));
        assert!(!r.is_nested_workspace(Path::new("/c")));
    }
}
