//! `ConfigService` — load, save, and resolve `GlobalConfig` +
//! per-workspace overrides.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::config::{
    EffectiveConfig, FakeKeychain, GlobalConfig, GlobalConfigPatch, KeychainAccess, ProviderConfig,
    ResolvedConfig, SecretRef, WorkspaceConfig, WorkspaceConfigPatch,
};
use crate::ids::WorkspaceId;
use crate::{AppError, AppResult};

/// Paths used by `ConfigService` to locate the global and per-workspace
/// config files. Centralized so `AppState` can pass them in once
/// at startup and so tests can swap individual paths.
#[derive(Debug, Clone)]
pub struct ServiceConfigPaths {
    /// Path to the global `config.toml` (`~/.agentyx/config.toml`).
    pub global: PathBuf,
    /// Root directory for per-workspace configs
    /// (`~/.agentyx/workspaces/`).
    pub workspaces_root: PathBuf,
}

impl ServiceConfigPaths {
    /// Build a `ServiceConfigPaths` rooted at `~/.agentyx/`.
    #[must_use]
    pub fn from_agentyx_home(home: &Path) -> Self {
        Self {
            global: home.join("config.toml"),
            workspaces_root: home.join("workspaces"),
        }
    }

    /// Resolve the per-workspace config file path.
    #[must_use]
    pub fn workspace_config_path(&self, workspace_id: WorkspaceId) -> PathBuf {
        self.workspaces_root
            .join(workspace_id.to_string())
            .join("config.toml")
    }
}

/// Lightweight snapshot of a `ResolvedConfig` that intentionally
/// drops the secrets. Use this to send the resolved config to the
/// UI without leaking API keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedConfigSnapshot {
    /// The global config (latest snapshot).
    pub global: GlobalConfig,
    /// The workspace config (if any).
    pub workspace: Option<WorkspaceConfig>,
    /// The effective final config (workspace override > global).
    pub effective: EffectiveConfig,
}

/// Service that loads and persists the global + per-workspace
/// config. Cheap to clone (internally `Arc`).
#[derive(Clone)]
pub struct ConfigService {
    inner: Arc<Inner>,
}

struct Inner {
    paths: ServiceConfigPaths,
    keychain: Arc<dyn KeychainAccess>,
    cached: parking_lot::RwLock<GlobalConfig>,
}

impl std::fmt::Debug for ConfigService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigService")
            .field("paths", &self.inner.paths)
            .finish_non_exhaustive()
    }
}

impl ConfigService {
    /// Load (or create) the global config from `paths.global`. If
    /// the file doesn't exist, writes defaults atomically.
    ///
    /// The keychain defaults to `FakeKeychain`; tests should use
    /// [`ConfigService::load_with_keychain`] and pass
    /// `FakeKeychain::with_entries(...)`. The app binary
    /// (`agentyx-app`) wires `OsKeychain` directly.
    pub fn load(paths: &ServiceConfigPaths) -> AppResult<Self> {
        Self::load_with_keychain(paths, Arc::new(FakeKeychain::new()))
    }

    /// Load with a custom keychain implementation. Tests pass
    /// `FakeKeychain::with_entries(...)`; production passes
    /// `Arc::new(OsKeychain)`.
    pub fn load_with_keychain(
        paths: &ServiceConfigPaths,
        keychain: Arc<dyn KeychainAccess>,
    ) -> AppResult<Self> {
        let config = if paths.global.exists() {
            let text = std::fs::read_to_string(&paths.global).map_err(|e| AppError::Io {
                op: format!("read {}", paths.global.display()),
                reason: e.to_string(),
            })?;
            toml::from_str::<GlobalConfig>(&text).map_err(|e| AppError::InvalidInput {
                message: format!("config.toml is malformed: {e}"),
            })?
        } else {
            // Create defaults.
            let cfg = GlobalConfig::default();
            if let Some(parent) = paths.global.parent() {
                std::fs::create_dir_all(parent).map_err(|e| AppError::Io {
                    op: format!("create_dir_all {}", parent.display()),
                    reason: e.to_string(),
                })?;
            }
            write_atomic(&paths.global, &cfg).map_err(|e| AppError::Internal {
                message: format!("write default config: {e}"),
            })?;
            cfg
        };

        config
            .validate()
            .map_err(|e| AppError::InvalidInput { message: e })?;

        Ok(Self {
            inner: Arc::new(Inner {
                paths: paths.clone(),
                keychain,
                cached: parking_lot::RwLock::new(config),
            }),
        })
    }

    /// The path the global config was loaded from.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.inner.paths.global
    }

    /// The paths this service uses.
    #[must_use]
    pub fn paths(&self) -> &ServiceConfigPaths {
        &self.inner.paths
    }

    /// Snapshot the current global config.
    #[must_use]
    pub fn get(&self) -> GlobalConfig {
        self.inner.cached.read().clone()
    }

    /// Look up a provider's config.
    #[must_use]
    pub fn provider_config(&self, id: &str) -> Option<ProviderConfig> {
        self.inner.cached.read().providers.get(id).cloned()
    }

    /// Update the global config atomically. Re-validates and
    /// re-resolves secrets.
    pub fn update<F>(&self, mutate: F) -> AppResult<GlobalConfig>
    where
        F: FnOnce(&mut GlobalConfig),
    {
        let mut new_cfg = self.get();
        mutate(&mut new_cfg);
        new_cfg
            .validate()
            .map_err(|e| AppError::InvalidInput { message: e })?;

        // Backup the current file before writing.
        if self.inner.paths.global.exists() {
            let backup = self.inner.paths.global.with_extension("toml.bak");
            let _ = std::fs::copy(&self.inner.paths.global, &backup);
        }

        write_atomic(&self.inner.paths.global, &new_cfg).map_err(|e| AppError::Internal {
            message: format!("write config.toml: {e}"),
        })?;

        *self.inner.cached.write() = new_cfg.clone();
        Ok(new_cfg)
    }

    /// Apply a `GlobalConfigPatch` and persist atomically. The
    /// patch is rejected if it would leave the config invalid
    /// (e.g. `default_provider` not in the new provider map).
    pub fn update_with_patch(&self, patch: &GlobalConfigPatch) -> AppResult<GlobalConfig> {
        if patch.is_empty() {
            return Ok(self.get());
        }
        self.update(|cfg| patch.apply_to(cfg))
    }

    /// Load a workspace's config. Returns `Ok(WorkspaceConfig::defaults())`
    /// if the file does not exist. Returns `Err(InvalidInput)` if
    /// the file exists but is malformed or fails validation.
    pub fn load_workspace(&self, workspace_id: WorkspaceId) -> AppResult<WorkspaceConfig> {
        let path = self.inner.paths.workspace_config_path(workspace_id);
        if !path.exists() {
            return Ok(WorkspaceConfig::defaults());
        }
        let text = std::fs::read_to_string(&path).map_err(|e| AppError::Io {
            op: format!("read {}", path.display()),
            reason: e.to_string(),
        })?;
        let cfg: WorkspaceConfig = toml::from_str(&text).map_err(|e| AppError::InvalidInput {
            message: format!("workspace config.toml is malformed: {e}"),
        })?;
        cfg.validate()
            .map_err(|e| AppError::InvalidInput { message: e })?;
        Ok(cfg)
    }

    /// Update a workspace's config. The patch is applied to the
    /// current config (or the defaults if no file exists), the
    /// result is validated, and persisted atomically.
    ///
    /// Cross-config validation: a workspace `default_provider`
    /// override must exist in the global `providers` map and be
    /// `enabled` (config.md AC14).
    pub fn update_workspace(
        &self,
        workspace_id: WorkspaceId,
        patch: &WorkspaceConfigPatch,
    ) -> AppResult<WorkspaceConfig> {
        let mut cfg = self.load_workspace(workspace_id)?;
        patch.apply_to(&mut cfg);
        cfg.validate()
            .map_err(|e| AppError::InvalidInput { message: e })?;

        // AC14: workspace's default_provider override must be
        // valid against the current global providers map.
        let global = self.get();
        if let Some(ref provider) = cfg.default_provider {
            match global.providers.get(provider) {
                Some(p) if p.enabled => {}
                Some(_) => {
                    return Err(AppError::InvalidInput {
                        message: format!(
                            "workspace default_provider '{provider}' is not enabled in global config"
                        ),
                    });
                }
                None => {
                    return Err(AppError::InvalidInput {
                        message: format!(
                            "workspace default_provider '{provider}' is not in global providers"
                        ),
                    });
                }
            }
        }

        let path = self.inner.paths.workspace_config_path(workspace_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::Io {
                op: format!("create_dir_all {}", parent.display()),
                reason: e.to_string(),
            })?;
        }
        if path.exists() {
            let backup = path.with_extension("toml.bak");
            let _ = std::fs::copy(&path, &backup);
        }
        write_workspace_atomic(&path, &cfg).map_err(|e| AppError::Internal {
            message: format!("write workspace config.toml: {e}"),
        })?;
        Ok(cfg)
    }

    /// Resolve a `SecretRef` to its value. Returns
    /// `InvalidInput` if `env:` points to a missing variable;
    /// `Internal` if `keychain:` is missing.
    pub fn resolve_secret(&self, secret: &SecretRef) -> AppResult<String> {
        match secret {
            SecretRef::Env(var) => std::env::var(var).map_err(|_| AppError::InvalidInput {
                message: format!("environment variable {var} is not set"),
            }),
            SecretRef::Keychain { account } => {
                self.inner
                    .keychain
                    .get(account)?
                    .ok_or_else(|| AppError::Internal {
                        message: format!("no keychain entry for {account}"),
                    })
            }
        }
    }

    /// Resolve all configured provider secrets. The map is
    /// `provider_id -> api_key` and contains entries **only** for
    /// providers that have an `api_key` configured AND whose
    /// `SecretRef` resolves to a value. The map is intentionally
    /// missing providers that don't have an `api_key` (e.g.
    /// `Ollama` without auth).
    pub fn resolve_secrets(&self) -> AppResult<std::collections::HashMap<String, String>> {
        let cfg = self.get();
        let mut out = std::collections::HashMap::new();
        for (id, provider_cfg) in &cfg.providers {
            if !provider_cfg.enabled {
                continue;
            }
            if let Some(secret) = &provider_cfg.api_key {
                let value = self.resolve_secret(secret)?;
                out.insert(id.clone(), value);
            }
        }
        Ok(out)
    }

    /// Resolve the full config for a workspace, expanding secrets.
    /// The returned `ResolvedConfig` is **never** serialized to
    /// disk and is **never** passed to the UI. Use
    /// [`ConfigService::resolve_snapshot`] for the UI.
    pub fn resolve(&self, workspace_id: WorkspaceId) -> AppResult<ResolvedConfig> {
        let global = self.get();
        let workspace = self.load_workspace(workspace_id).map(Some).unwrap_or(None);
        let secrets = self.resolve_secrets()?;
        let effective = EffectiveConfig::from_configs(&global, workspace.as_ref());
        Ok(ResolvedConfig {
            global,
            workspace,
            secrets,
            effective,
        })
    }

    /// Resolve a workspace's config, **without secrets**, for the
    /// UI. This is the DTO shape.
    pub fn resolve_snapshot(&self, workspace_id: WorkspaceId) -> AppResult<ResolvedConfigSnapshot> {
        let global = self.get();
        let workspace = self.load_workspace(workspace_id).map(Some).unwrap_or(None);
        let effective = EffectiveConfig::from_configs(&global, workspace.as_ref());
        Ok(ResolvedConfigSnapshot {
            global,
            workspace,
            effective,
        })
    }

    /// Set a keychain entry. Used by the `secrets_set` Tauri
    /// command. The value is **never** logged.
    pub fn set_keychain(&self, account: &str, value: &str) -> AppResult<()> {
        self.inner.keychain.set(account, value)
    }

    /// Delete a keychain entry. No-op if the entry doesn't exist.
    pub fn delete_keychain(&self, account: &str) -> AppResult<()> {
        self.inner.keychain.delete(account)
    }

    /// List provider ids that have a keychain entry. Does NOT
    /// return the values (they are read by the agent loop, not
    /// exposed to the UI).
    pub fn list_keychain_providers(&self) -> AppResult<Vec<String>> {
        let cfg = self.get();
        let mut out = Vec::new();
        for id in cfg.providers.keys() {
            if self.inner.keychain.get(id)?.is_some() {
                out.push(id.clone());
            }
        }
        out.sort();
        Ok(out)
    }
}

fn write_atomic(path: &Path, cfg: &GlobalConfig) -> std::io::Result<()> {
    let final_path = path;
    let tmp_path = path.with_extension("toml.tmp");
    let bytes = toml::to_string_pretty(cfg).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, format!("toml encode: {e}"))
    })?;
    std::fs::write(&tmp_path, &bytes)?;
    std::fs::rename(&tmp_path, final_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(final_path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn write_workspace_atomic(path: &Path, cfg: &WorkspaceConfig) -> std::io::Result<()> {
    let final_path = path;
    let tmp_path = path.with_extension("toml.tmp");
    let bytes = toml::to_string_pretty(cfg).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, format!("toml encode: {e}"))
    })?;
    std::fs::write(&tmp_path, &bytes)?;
    std::fs::rename(&tmp_path, final_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(final_path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unsafe_code,
    clippy::field_reassign_with_default,
    clippy::unnecessary_get_then_check
)]
mod tests {
    use super::*;
    use crate::config::{ApprovalMode, UpdateChannel};

    fn fresh_paths() -> (tempfile::TempDir, ServiceConfigPaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = ServiceConfigPaths::from_agentyx_home(dir.path());
        (dir, paths)
    }

    fn fresh_service() -> (tempfile::TempDir, ConfigService) {
        let (dir, paths) = fresh_paths();
        let svc = ConfigService::load(&paths).unwrap();
        (dir, svc)
    }

    fn fresh_service_with_keychain(
        keychain: Arc<dyn KeychainAccess>,
    ) -> (tempfile::TempDir, ConfigService) {
        let (dir, paths) = fresh_paths();
        let svc = ConfigService::load_with_keychain(&paths, keychain).unwrap();
        (dir, svc)
    }

    #[test]
    fn approval_mode_default_is_ask() {
        let cfg = GlobalConfig::default();
        assert_eq!(cfg.approval_mode, ApprovalMode::Ask);
    }

    #[test]
    fn load_creates_defaults_when_missing() {
        let (dir, svc) = fresh_service();
        let path = dir.path().join("config.toml");
        assert!(path.exists(), "default config should be created");
        let cfg = svc.get();
        assert_eq!(cfg.default_provider, "ollama");
        assert_eq!(cfg.default_model, "llama3.1:8b");
        assert_eq!(cfg.approval_mode, ApprovalMode::Ask);
    }

    #[test]
    fn load_idempotent() {
        let (dir, paths) = fresh_paths();
        let _ = ConfigService::load(&paths).unwrap();
        let _ = ConfigService::load(&paths).unwrap();
        // File still parses and validates.
        let svc = ConfigService::load(&paths).unwrap();
        assert_eq!(svc.get().default_provider, "ollama");
        let _ = dir;
    }

    #[test]
    fn rejects_wrong_version() {
        let (dir, paths) = fresh_paths();
        std::fs::write(
            &paths.global,
            "version = 99\ndefault_provider = \"ollama\"\ndefault_model = \"x\"\n\
             [providers.ollama]\nbase_url=\"http://x\"\nenabled=true\n",
        )
        .unwrap();
        let err = ConfigService::load(&paths).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput { .. }));
        let _ = dir;
    }

    #[test]
    fn rejects_literal_api_key() {
        let (dir, paths) = fresh_paths();
        std::fs::write(
            &paths.global,
            "version = 1\ndefault_provider = \"ollama\"\ndefault_model = \"x\"\n\
             [providers.ollama]\nbase_url=\"http://x\"\nenabled=true\napi_key=\"sk-1234abcd\"\n",
        )
        .unwrap();
        let err = ConfigService::load(&paths).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput { .. }));
        let _ = dir;
    }

    #[test]
    fn update_creates_backup_and_persists() {
        let (dir, svc) = fresh_service();
        let path = dir.path().join("config.toml");
        let _ = svc
            .update(|c| c.default_model = "qwen2.5:7b".to_string())
            .unwrap();
        // Backup should exist.
        let backup = dir.path().join("config.toml.bak");
        assert!(backup.exists());
        // New file should reflect the change.
        let back = ConfigService::load(&ServiceConfigPaths::from_agentyx_home(dir.path())).unwrap();
        assert_eq!(back.get().default_model, "qwen2.5:7b");
        let _ = path;
    }

    #[test]
    fn update_invalid_patch_keeps_original() {
        let (dir, svc) = fresh_service();
        let _ = svc.update(|c| c.version = 99).unwrap_err();
        // The file on disk is unchanged.
        let text = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
        assert!(text.contains("version = 1"));
    }

    #[test]
    fn resolve_env_secret() {
        let var = "AGENTYX_TEST_SECRET_VAR_UNIQUE";
        unsafe {
            std::env::set_var(var, "topsecret");
        }
        let (_dir, svc) = fresh_service();
        svc.update(|c| {
            c.providers.get_mut("ollama").unwrap().api_key = Some(SecretRef::Env(var.to_string()));
        })
        .unwrap();
        let secret = svc
            .resolve_secret(&SecretRef::Env(var.to_string()))
            .unwrap();
        assert_eq!(secret, "topsecret");
        unsafe {
            std::env::remove_var(var);
        }
    }

    #[test]
    fn resolve_missing_env_returns_invalid_input() {
        let (_dir, svc) = fresh_service();
        let err = svc
            .resolve_secret(&SecretRef::Env("AGENTYX_DEFINITELY_NOT_SET_X9K2".into()))
            .unwrap_err();
        assert!(matches!(err, AppError::InvalidInput { .. }));
    }

    #[test]
    fn resolve_keychain_secret() {
        let (dir, _svc) = fresh_service();
        let paths = ServiceConfigPaths::from_agentyx_home(dir.path());
        let keychain = FakeKeychain::with_entries(&[("groq", "kc-secret")]);
        let svc = ConfigService::load_with_keychain(&paths, Arc::new(keychain)).unwrap();
        let secret = svc
            .resolve_secret(&SecretRef::Keychain {
                account: "groq".into(),
            })
            .unwrap();
        assert_eq!(secret, "kc-secret");
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn default_config_validates() {
        let mut cfg = GlobalConfig::default();
        cfg.update_channel = UpdateChannel::Dev;
        assert!(cfg.validate().is_ok());
    }

    // ===============================================================
    // Workspace config tests (config.md AC12, AC13, AC14)
    // ===============================================================

    #[test]
    fn ac12_load_workspace_missing_returns_default() {
        let (_dir, svc) = fresh_service();
        let ws = WorkspaceId::new();
        let cfg = svc.load_workspace(ws).unwrap();
        assert_eq!(cfg.version, 1);
        assert!(cfg.default_provider.is_none());
        assert!(cfg.approval_mode.is_none());
        // Default ignore patterns are present.
        assert!(!cfg.workspace.ignore_patterns.is_empty());
        assert_eq!(cfg.workspace.journal_max_rows, Some(100_000));
    }

    #[test]
    fn ac13_effective_config_workspace_overrides_global() {
        // Build a ResolvedConfigSnapshot by hand to verify override
        // semantics: workspace's default_model beats global's.
        let mut global = GlobalConfig::default();
        global.default_model = "global-model".into();
        let mut workspace = WorkspaceConfig::defaults();
        workspace.default_model = Some("workspace-model".into());
        workspace.approval_mode = Some(ApprovalMode::Deny);

        let effective = EffectiveConfig::from_configs(&global, Some(&workspace));
        assert_eq!(effective.default_model, "workspace-model");
        assert_eq!(effective.approval_mode, ApprovalMode::Deny);
    }

    #[test]
    fn ac13_effective_config_falls_back_to_global() {
        // When the workspace has no overrides, the global values
        // win.
        let global = GlobalConfig::default();
        let workspace = WorkspaceConfig::defaults();

        let effective = EffectiveConfig::from_configs(&global, Some(&workspace));
        assert_eq!(effective.default_model, global.default_model);
        assert_eq!(effective.approval_mode, global.approval_mode);
        assert_eq!(effective.default_provider, global.default_provider);
    }

    #[test]
    fn ac14_workspace_unknown_provider_rejected() {
        let (_dir, svc) = fresh_service();
        let ws = WorkspaceId::new();
        // Build a patch that sets a default_provider not in the
        // global map. This must be rejected with `invalid_input`.
        let patch = WorkspaceConfigPatch {
            default_provider: Some(Some("nonexistent".into())),
            ..Default::default()
        };

        let err = svc.update_workspace(ws, &patch).unwrap_err();
        match err {
            AppError::InvalidInput { message } => {
                assert!(message.contains("nonexistent"), "msg: {message}");
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[test]
    fn ac14_workspace_disabled_provider_rejected() {
        // AC14 (variant): workspace default_provider must point
        // to an enabled global provider.
        let (_dir, svc) = fresh_service();
        let ws = WorkspaceId::new();
        // Add a disabled provider to the global config.
        svc.update(|c| {
            c.providers.insert(
                "disabled".into(),
                crate::config::ProviderConfig {
                    base_url: "https://x".into(),
                    enabled: false,
                    api_key: None,
                    models: None,
                },
            );
        })
        .unwrap();
        let patch = WorkspaceConfigPatch {
            default_provider: Some(Some("disabled".into())),
            ..Default::default()
        };
        let err = svc.update_workspace(ws, &patch).unwrap_err();
        match err {
            AppError::InvalidInput { message } => {
                assert!(message.contains("not enabled"), "msg: {message}");
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[test]
    fn update_workspace_persists_and_loads_back() {
        let (_dir, svc) = fresh_service();
        let ws = WorkspaceId::new();
        let mut patch = WorkspaceConfigPatch::default();
        patch.default_model = Some(Some("override-model".into()));
        patch.approval_mode = Some(Some(ApprovalMode::Deny));
        let new_cfg = svc.update_workspace(ws, &patch).unwrap();
        assert_eq!(new_cfg.default_model.as_deref(), Some("override-model"));
        assert_eq!(new_cfg.approval_mode, Some(ApprovalMode::Deny));

        // Reload from disk.
        let loaded = svc.load_workspace(ws).unwrap();
        assert_eq!(loaded.default_model.as_deref(), Some("override-model"));
        assert_eq!(loaded.approval_mode, Some(ApprovalMode::Deny));
    }

    #[test]
    fn update_workspace_rejects_invalid_patch() {
        let (_dir, svc) = fresh_service();
        let ws = WorkspaceId::new();
        let mut patch = WorkspaceConfigPatch::default();
        // Empty default_model is structurally invalid.
        patch.default_model = Some(Some(String::new()));

        let err = svc.update_workspace(ws, &patch).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput { .. }));
    }

    // ===============================================================
    // resolve_secrets / resolve_snapshot (AC7, AC8, AC9)
    // ===============================================================

    #[test]
    fn ac7_resolve_env_secret_for_provider() {
        let var = "AGENTYX_TEST_PROVIDER_KEY_42";
        unsafe {
            std::env::set_var(var, "my-groq-key");
        }
        let (_dir, svc) = fresh_service();
        svc.update(|c| {
            c.providers.insert(
                "groq".to_string(),
                crate::config::ProviderConfig {
                    base_url: "https://api.groq.com/openai/v1".into(),
                    enabled: true,
                    api_key: Some(SecretRef::Env(var.to_string())),
                    models: Some(vec!["llama-3.3-70b-versatile".into()]),
                },
            );
        })
        .unwrap();
        let secrets = svc.resolve_secrets().unwrap();
        assert_eq!(secrets.get("groq").map(String::as_str), Some("my-groq-key"));
        // Disabled providers and providers with no api_key are
        // omitted from the result.
        assert!(secrets.get("ollama").is_none());
        unsafe {
            std::env::remove_var(var);
        }
    }

    #[test]
    fn ac8_resolve_missing_env_returns_invalid_input() {
        let (_dir, svc) = fresh_service();
        svc.update(|c| {
            c.providers.insert(
                "missing".to_string(),
                crate::config::ProviderConfig {
                    base_url: "https://x".into(),
                    enabled: true,
                    api_key: Some(SecretRef::Env("AGENTYX_NOT_SET_X9K2".into())),
                    models: None,
                },
            );
        })
        .unwrap();
        let err = svc.resolve_secrets().unwrap_err();
        assert!(matches!(err, AppError::InvalidInput { .. }));
    }

    #[test]
    fn ac9_resolve_snapshot_does_not_include_secrets() {
        let keychain = FakeKeychain::with_entries(&[("groq", "kc-secret")]);
        let (_dir, svc) = fresh_service_with_keychain(Arc::new(keychain));
        svc.update(|c| {
            c.providers.insert(
                "groq".to_string(),
                crate::config::ProviderConfig {
                    base_url: "https://api.groq.com/openai/v1".into(),
                    enabled: true,
                    api_key: Some(SecretRef::Keychain {
                        account: "groq".into(),
                    }),
                    models: None,
                },
            );
        })
        .unwrap();

        let ws = WorkspaceId::new();
        let snap = svc.resolve_snapshot(ws).unwrap();
        let serialized = serde_json::to_string(&snap).unwrap();
        assert!(
            !serialized.contains("kc-secret"),
            "resolved snapshot must not contain the keychain value: {serialized}"
        );
    }

    #[test]
    fn resolve_secrets_skips_disabled_providers() {
        let var = "AGENTYX_TEST_DISABLED_KEY_42";
        unsafe {
            std::env::set_var(var, "should-not-resolve");
        }
        let (_dir, svc) = fresh_service();
        svc.update(|c| {
            c.providers.insert(
                "disabled-prov".to_string(),
                crate::config::ProviderConfig {
                    base_url: "https://x".into(),
                    enabled: false,
                    api_key: Some(SecretRef::Env(var.to_string())),
                    models: None,
                },
            );
        })
        .unwrap();
        let secrets = svc.resolve_secrets().unwrap();
        assert!(secrets.get("disabled-prov").is_none());
        unsafe {
            std::env::remove_var(var);
        }
    }

    // ===============================================================
    // keychain wiring (AC7, AC8, F05 secrets commands)
    // ===============================================================

    #[test]
    fn keychain_set_then_list_returns_ids_only() {
        let (_dir, svc) = fresh_service();
        svc.set_keychain("groq", "the-value").unwrap();
        let ids = svc.list_keychain_providers().unwrap();
        // The list returns only ids; the value is not exposed.
        assert!(
            ids.is_empty(),
            "no provider registered with keychain id 'groq' yet"
        );
        svc.update(|c| {
            c.providers.insert(
                "groq".to_string(),
                crate::config::ProviderConfig {
                    base_url: "https://api.groq.com/openai/v1".into(),
                    enabled: true,
                    api_key: Some(SecretRef::Keychain {
                        account: "groq".into(),
                    }),
                    models: None,
                },
            );
        })
        .unwrap();
        let ids = svc.list_keychain_providers().unwrap();
        assert_eq!(ids, vec!["groq".to_string()]);
        // The value is not part of the list.
        let ids_str = serde_json::to_string(&ids).unwrap();
        assert!(!ids_str.contains("the-value"));
    }

    #[test]
    fn keychain_delete_is_idempotent() {
        let (_dir, svc) = fresh_service();
        svc.set_keychain("groq", "v").unwrap();
        svc.delete_keychain("groq").unwrap();
        svc.delete_keychain("groq").unwrap(); // no-op
    }

    // ===============================================================
    // Patch behavior (AC10, AC11)
    // ===============================================================

    #[test]
    fn update_with_patch_empty_is_noop() {
        let (_dir, svc) = fresh_service();
        let before = svc.get();
        let after = svc
            .update_with_patch(&GlobalConfigPatch::default())
            .unwrap();
        assert_eq!(before.default_model, after.default_model);
    }

    #[test]
    fn update_with_patch_persists_changes() {
        let (_dir, svc) = fresh_service();
        let mut patch = GlobalConfigPatch::default();
        patch.default_model = Some("new-model".into());
        patch.approval_mode = Some(ApprovalMode::Allow);
        let after = svc.update_with_patch(&patch).unwrap();
        assert_eq!(after.default_model, "new-model");
        assert_eq!(after.approval_mode, ApprovalMode::Allow);
    }

    #[test]
    fn update_with_patch_rejects_invalid_default_provider() {
        let (_dir, svc) = fresh_service();
        let mut patch = GlobalConfigPatch::default();
        patch.default_provider = Some("nonexistent".into());
        let err = svc.update_with_patch(&patch).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput { .. }));
    }
}
