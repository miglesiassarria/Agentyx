//! `GlobalConfig` schema + `SecretRef` deserialization.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Provider id (e.g. `"ollama"`, `"groq"`). Newtype around `String`
/// for type-safety; stored in `HashMap<String, ProviderConfig>`.
pub type ProviderId = String;

/// One provider's configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    /// Base URL (e.g. `"http://127.0.0.1:11434"`).
    pub base_url: String,
    /// Whether this provider is enabled.
    pub enabled: bool,
    /// API key reference. `None` for Ollama without auth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<SecretRef>,
    /// Hardcoded model list (Ollama discovers models dynamically).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<String>>,
}

/// Reference to a secret (API key, bearer token). Serialized as
/// a string in TOML: `env:VAR_NAME` or `keychain:account`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SecretRef {
    /// Variable in the process environment.
    Env(String),
    /// Entry in the OS keychain (`agentyx` service).
    Keychain {
        /// Account (provider id).
        account: String,
    },
}

impl SecretRef {
    /// Parse from a string. Rejects literal API keys (heuristic:
    /// starting with `sk-`, `gsk_`, `sk-ant-`).
    pub fn parse(s: &str) -> Result<Self, String> {
        if let Some(var) = s.strip_prefix("env:") {
            if var.is_empty() {
                return Err("SecretRef::Env cannot be empty".into());
            }
            if !var
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            {
                return Err(format!(
                    "env variable name must match ^[A-Z][A-Z0-9_]*$: {var}"
                ));
            }
            Ok(Self::Env(var.to_string()))
        } else if let Some(account) = s.strip_prefix("keychain:") {
            if account.is_empty() {
                return Err("SecretRef::Keychain account cannot be empty".into());
            }
            Ok(Self::Keychain {
                account: account.to_string(),
            })
        } else if s.starts_with("sk-") || s.starts_with("gsk_") || s.starts_with("sk-ant-") {
            Err("API key literals are not allowed; use `env:VAR_NAME` or `keychain:account`".into())
        } else {
            // Conservative default: treat as env var name.
            Ok(Self::Env(s.to_string()))
        }
    }

    /// Serialize back to the canonical string form.
    #[must_use]
    pub fn as_canonical_string(&self) -> String {
        match self {
            Self::Env(var) => format!("env:{var}"),
            Self::Keychain { account } => format!("keychain:{account}"),
        }
    }
}

impl std::fmt::Display for SecretRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.as_canonical_string())
    }
}

/// Approval mode (per config.md §State).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalMode {
    /// Prompt for writes, shell, network.
    #[default]
    Ask,
    /// No prompts.
    Allow,
    /// Block writes, shell, network.
    Deny,
}

/// Update channel (per config.md §State).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    /// Stable releases only (default).
    #[default]
    Stable,
    /// Stable + beta releases.
    Beta,
    /// Stable + beta + dev (nightly) builds.
    Dev,
}

/// UI settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiConfig {
    /// Theme preference.
    pub theme: Theme,
    /// Font size in pt (10..=24).
    pub font_size: u8,
    /// Whether to display token counts in the UI.
    pub show_token_count: bool,
    /// Whether to display message timestamps.
    pub show_timestamps: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: Theme::Auto,
            font_size: 14,
            show_token_count: true,
            show_timestamps: true,
        }
    }
}

/// UI theme.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Follow the OS preference (default).
    #[default]
    Auto,
    /// Light theme regardless of OS.
    Light,
    /// Dark theme regardless of OS.
    Dark,
}

/// Global config. Persisted to `~/.agentyx/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalConfig {
    /// Config schema version (== 1 in v1).
    pub version: u32,
    /// Approval mode.
    pub approval_mode: ApprovalMode,
    /// Default provider id (must exist in `providers` and be enabled).
    pub default_provider: ProviderId,
    /// Default model id.
    pub default_model: String,
    /// Providers keyed by id.
    pub providers: HashMap<ProviderId, ProviderConfig>,
    /// UI settings.
    pub ui: UiConfig,
    /// Whether to send anonymous telemetry. Off by default
    /// (per config.md §Goal).
    pub telemetry_enabled: bool,
    /// Whether to check for updates on startup.
    pub check_updates: bool,
    /// Update channel.
    pub update_channel: UpdateChannel,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        let mut providers = HashMap::new();
        providers.insert(
            "ollama".to_string(),
            ProviderConfig {
                base_url: "http://127.0.0.1:11434".to_string(),
                enabled: true,
                api_key: None,
                models: None,
            },
        );
        Self {
            version: 1,
            approval_mode: ApprovalMode::Ask,
            default_provider: "ollama".to_string(),
            default_model: "llama3.1:8b".to_string(),
            providers,
            ui: UiConfig::default(),
            telemetry_enabled: false,
            check_updates: true,
            update_channel: UpdateChannel::Stable,
        }
    }
}

impl GlobalConfig {
    /// Validate the config. Returns `InvalidInput` on the first
    /// problem.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!(
                "Config version {} is not supported. Expected 1.",
                self.version
            ));
        }
        if !self.providers.contains_key(&self.default_provider) {
            return Err(format!(
                "default_provider '{}' is not in providers",
                self.default_provider
            ));
        }
        if !self.providers[&self.default_provider].enabled {
            return Err(format!(
                "default_provider '{}' is not enabled",
                self.default_provider
            ));
        }
        if self.default_model.is_empty() {
            return Err("default_model cannot be empty".into());
        }
        if !(10..=24).contains(&self.ui.font_size) {
            return Err(format!(
                "ui.font_size {} out of range [10, 24]",
                self.ui.font_size
            ));
        }
        for (id, p) in &self.providers {
            if url::Url::parse(&p.base_url).is_err() {
                return Err(format!("provider '{id}': base_url is not a valid URL"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn secret_ref_parses_env() {
        let s = SecretRef::parse("env:GROQ_API_KEY").unwrap();
        assert!(matches!(s, SecretRef::Env(v) if v == "GROQ_API_KEY"));
    }

    #[test]
    fn secret_ref_parses_keychain() {
        let s = SecretRef::parse("keychain:groq").unwrap();
        assert!(matches!(s, SecretRef::Keychain { account } if account == "groq"));
    }

    #[test]
    fn secret_ref_rejects_literal_api_key() {
        let err = SecretRef::parse("sk-1234567890abcdef").unwrap_err();
        assert!(err.contains("API key literals"));
    }

    #[test]
    fn secret_ref_rejects_empty_env() {
        assert!(SecretRef::parse("env:").is_err());
    }

    #[test]
    fn secret_ref_rejects_invalid_env_name() {
        assert!(SecretRef::parse("env:lowercase").is_err());
    }

    #[test]
    fn default_config_is_valid() {
        let cfg = GlobalConfig::default();
        cfg.validate().unwrap();
    }

    #[test]
    fn default_config_toml_roundtrips() {
        let cfg = GlobalConfig::default();
        let s = toml::to_string_pretty(&cfg).unwrap();
        let back: GlobalConfig = toml::from_str(&s).unwrap();
        assert_eq!(back.default_provider, "ollama");
        assert_eq!(back.default_model, "llama3.1:8b");
    }

    #[test]
    fn font_size_out_of_range_rejected() {
        let mut cfg = GlobalConfig::default();
        cfg.ui.font_size = 100;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn invalid_base_url_rejected() {
        let mut cfg = GlobalConfig::default();
        cfg.providers.get_mut("ollama").unwrap().base_url = "not a url".into();
        assert!(cfg.validate().is_err());
    }
}
