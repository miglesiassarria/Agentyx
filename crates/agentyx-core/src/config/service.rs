//! `ConfigService` — load, save, and resolve `GlobalConfig`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::config::{FakeKeychain, GlobalConfig, KeychainAccess, ProviderConfig, SecretRef};
use crate::{AppError, AppResult};

/// Service that loads and persists the global config. Cheap to
/// clone (internally `Arc`).
#[derive(Clone)]
pub struct ConfigService {
    inner: Arc<Inner>,
}

struct Inner {
    config_path: PathBuf,
    keychain: Arc<dyn KeychainAccess>,
    cached: parking_lot::RwLock<GlobalConfig>,
}

impl std::fmt::Debug for ConfigService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigService")
            .field("path", &self.inner.config_path)
            .finish_non_exhaustive()
    }
}

impl ConfigService {
    /// Load (or create) the global config from `config_path`. If
    /// the file doesn't exist, writes defaults atomically.
    pub fn load(config_path: &Path) -> AppResult<Self> {
        Self::load_with_keychain(config_path, Arc::new(FakeKeychain::new()))
    }

    /// Load with a custom keychain implementation. Tests pass
    /// `FakeKeychain::with_entries(...)`; production passes
    /// `Arc::new(OsKeychain)`.
    pub fn load_with_keychain(
        config_path: &Path,
        keychain: Arc<dyn KeychainAccess>,
    ) -> AppResult<Self> {
        let config = if config_path.exists() {
            let text = std::fs::read_to_string(config_path).map_err(|e| AppError::Io {
                op: format!("read {}", config_path.display()),
                reason: e.to_string(),
            })?;
            toml::from_str::<GlobalConfig>(&text).map_err(|e| AppError::InvalidInput {
                message: format!("config.toml is malformed: {e}"),
            })?
        } else {
            // Create defaults.
            let cfg = GlobalConfig::default();
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| AppError::Io {
                    op: format!("create_dir_all {}", parent.display()),
                    reason: e.to_string(),
                })?;
            }
            write_atomic(config_path, &cfg).map_err(|e| AppError::Internal {
                message: format!("write default config: {e}"),
            })?;
            cfg
        };

        config
            .validate()
            .map_err(|e| AppError::InvalidInput { message: e })?;

        Ok(Self {
            inner: Arc::new(Inner {
                config_path: config_path.to_path_buf(),
                keychain,
                cached: parking_lot::RwLock::new(config),
            }),
        })
    }

    /// The path this config was loaded from.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.inner.config_path
    }

    /// Snapshot the current config.
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
        if self.inner.config_path.exists() {
            let backup = self.inner.config_path.with_extension("toml.bak");
            let _ = std::fs::copy(&self.inner.config_path, &backup);
        }

        write_atomic(&self.inner.config_path, &new_cfg).map_err(|e| AppError::Internal {
            message: format!("write config.toml: {e}"),
        })?;

        *self.inner.cached.write() = new_cfg.clone();
        Ok(new_cfg)
    }

    /// Resolve a `SecretRef` to its value. Returns
    /// `InvalidInput` if `env:` points to a missing variable.
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, unsafe_code)]
mod tests {
    use super::*;
    use crate::config::{ApprovalMode, UpdateChannel};

    fn fresh_service() -> (tempfile::TempDir, ConfigService) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let svc = ConfigService::load(&path).unwrap();
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
        let (dir, _svc) = fresh_service();
        let path = dir.path().join("config.toml");
        let _ = ConfigService::load(&path).unwrap();
        let _ = ConfigService::load(&path).unwrap();
        // File still parses and validates.
        let svc = ConfigService::load(&path).unwrap();
        assert_eq!(svc.get().default_provider, "ollama");
    }

    #[test]
    fn rejects_wrong_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "version = 99\ndefault_provider = \"ollama\"\ndefault_model = \"x\"\n\
             [providers.ollama]\nbase_url=\"http://x\"\nenabled=true\n",
        )
        .unwrap();
        let err = ConfigService::load(&path).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput { .. }));
    }

    #[test]
    fn rejects_literal_api_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "version = 1\ndefault_provider = \"ollama\"\ndefault_model = \"x\"\n\
             [providers.ollama]\nbase_url=\"http://x\"\nenabled=true\napi_key=\"sk-1234abcd\"\n",
        )
        .unwrap();
        let err = ConfigService::load(&path).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput { .. }));
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
        let back = ConfigService::load(&path).unwrap();
        assert_eq!(back.get().default_model, "qwen2.5:7b");
    }

    #[test]
    fn update_invalid_patch_keeps_original() {
        let (dir, svc) = fresh_service();
        let original = svc.get().clone();
        let _ = svc.update(|c| c.version = 99).unwrap_err();
        // The file on disk is unchanged.
        let text = std::fs::read_to_string(dir.path().join("config.toml")).unwrap();
        assert!(text.contains("version = 1"));
        let _ = original; // silence unused
    }

    #[test]
    fn resolve_env_secret() {
        // SAFETY: tests run in a single-threaded setup so this
        // is safe in practice. CI runs tests in parallel though,
        // so we use a unique var name to avoid collisions.
        let var = "AGENTYX_TEST_SECRET_VAR_UNIQUE";
        // SAFETY: set_var is unsafe in concurrent contexts; we
        // use a process-unique var name and run before fork.
        unsafe {
            std::env::set_var(var, "topsecret");
        }
        let (dir, svc) = fresh_service();
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
        let _ = dir;
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
        let path = dir.path().join("config.toml");
        let keychain = FakeKeychain::with_entries(&[("groq", "kc-secret")]);
        let svc = ConfigService::load_with_keychain(&path, Arc::new(keychain)).unwrap();
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
}
