//! `GlobalConfig` schema + `SecretRef` deserialization.

use std::collections::HashMap;

use serde::de::Error;
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

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            enabled: true,
            api_key: None,
            models: None,
        }
    }
}

/// Reference to a secret (API key, bearer token). Serialized as
/// a string in TOML: `env:VAR_NAME` or `keychain:account`.
#[derive(Debug, Clone, Serialize)]
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

impl<'de> Deserialize<'de> for SecretRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        SecretRef::parse(&s).map_err(D::Error::custom)
    }
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

/// Default per-tool decision. Persisted in
/// `GlobalConfig.default_tool_decisions`. When a tool is present
/// in the map, its decision wins over the static default from
/// `tools.md`. Used by the `permissions_set_default` Tauri
/// command (F05.AC9).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ToolDecision {
    /// Allow without prompting.
    Allow,
    /// Prompt the user.
    Ask,
    /// Deny without prompting.
    Deny,
}

impl ToolDecision {
    /// Parse from a lowercase string. Returns `None` for unknown
    /// values so the caller can return `invalid_input` cleanly.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "allow" => Some(Self::Allow),
            "ask" => Some(Self::Ask),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
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
    /// Per-tool default decision overrides. Empty means "use the
    /// static default from `tools.md`". When a tool id is present,
    /// its decision wins over the static default. Populated by
    /// the `permissions_set_default` Tauri command (F05.AC9).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub default_tool_decisions: HashMap<String, ToolDecision>,
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
            default_tool_decisions: HashMap::new(),
        }
    }
}

/// Per-workspace config overrides. Persisted at
/// `<workspace_root>/.agentyx/config.toml` (note: the workspace
/// `service.rs` uses a different layout — `<home>/workspaces/<id>/config.toml`
/// — but the schema is the same). All fields are optional; missing
/// fields fall back to `GlobalConfig`.
///
/// See `specs/domains/config.md` §State.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceConfig {
    /// Schema version. Must be `1` in v0.1.
    pub version: u32,
    /// Override the default provider in this workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_provider: Option<ProviderId>,
    /// Override the default model in this workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    /// Override the approval mode in this workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_mode: Option<ApprovalMode>,
    /// Workspace-specific settings.
    #[serde(default)]
    pub workspace: WorkspaceSettings,
}

/// Workspace settings: ignore patterns, journal cap, etc.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSettings {
    /// Glob patterns to ignore in `search` and the file watcher.
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
    /// Maximum number of journal rows before archiving (default 100_000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub journal_max_rows: Option<u64>,
}

impl WorkspaceSettings {
    /// Default ignore patterns (used when no workspace config exists).
    pub const DEFAULT_IGNORE_PATTERNS: &'static [&'static str] = &[
        ".git/",
        "node_modules/",
        "target/",
        "__pycache__/",
        ".venv/",
        "venv/",
        "dist/",
        "build/",
        ".next/",
        ".cache/",
    ];

    /// Convert to a fresh `Vec<String>` from the defaults.
    #[must_use]
    pub fn default_ignore() -> Vec<String> {
        Self::DEFAULT_IGNORE_PATTERNS
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    }
}

impl WorkspaceConfig {
    /// Default value (used when no `config.toml` exists for the workspace).
    #[must_use]
    pub fn defaults() -> Self {
        Self {
            version: 1,
            default_provider: None,
            default_model: None,
            approval_mode: None,
            workspace: WorkspaceSettings {
                ignore_patterns: WorkspaceSettings::default_ignore(),
                journal_max_rows: Some(100_000),
            },
        }
    }

    /// Validate the workspace config. Returns `InvalidInput` on
    /// the first problem.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!(
                "Workspace config version {} is not supported. Expected 1.",
                self.version
            ));
        }
        if self.default_model.is_some() && self.default_model.as_deref() == Some("") {
            return Err("default_model cannot be empty when set".into());
        }
        if let Some(rows) = self.workspace.journal_max_rows {
            if !(1_000..=10_000_000).contains(&rows) {
                return Err(format!(
                    "journal_max_rows {rows} out of range [1000, 10000000]"
                ));
            }
        }
        for pat in &self.workspace.ignore_patterns {
            if pat.is_empty() {
                return Err("ignore_patterns entry cannot be empty".into());
            }
        }
        Ok(())
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

    #[test]
    fn workspace_config_defaults_are_valid() {
        let cfg = WorkspaceConfig::defaults();
        cfg.validate().unwrap();
    }

    #[test]
    fn workspace_config_rejects_wrong_version() {
        let mut cfg = WorkspaceConfig::defaults();
        cfg.version = 99;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn workspace_config_rejects_journal_max_rows_out_of_range() {
        let mut cfg = WorkspaceConfig::defaults();
        cfg.workspace.journal_max_rows = Some(500);
        assert!(cfg.validate().is_err());

        cfg.workspace.journal_max_rows = Some(100_000_000);
        assert!(cfg.validate().is_err());

        cfg.workspace.journal_max_rows = Some(50_000);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn workspace_config_rejects_empty_ignore_pattern() {
        let mut cfg = WorkspaceConfig::defaults();
        cfg.workspace.ignore_patterns.push(String::new());
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn tool_decision_parse_known_values() {
        assert_eq!(ToolDecision::parse("allow"), Some(ToolDecision::Allow));
        assert_eq!(ToolDecision::parse("ask"), Some(ToolDecision::Ask));
        assert_eq!(ToolDecision::parse("deny"), Some(ToolDecision::Deny));
    }

    #[test]
    fn tool_decision_parse_unknown_returns_none() {
        assert_eq!(ToolDecision::parse(""), None);
        assert_eq!(ToolDecision::parse("Allow"), None);
        assert_eq!(ToolDecision::parse("prompt"), None);
    }

    #[test]
    fn default_config_has_empty_tool_decisions() {
        let cfg = GlobalConfig::default();
        assert!(cfg.default_tool_decisions.is_empty());
        cfg.validate().unwrap();
    }

    #[test]
    fn tool_decisions_toml_roundtrips() {
        let mut cfg = GlobalConfig::default();
        cfg.default_tool_decisions
            .insert("write_file".into(), ToolDecision::Allow);
        cfg.default_tool_decisions
            .insert("shell".into(), ToolDecision::Deny);
        let s = toml::to_string_pretty(&cfg).unwrap();
        let back: GlobalConfig = toml::from_str(&s).unwrap();
        assert_eq!(
            back.default_tool_decisions.get("write_file"),
            Some(&ToolDecision::Allow)
        );
        assert_eq!(
            back.default_tool_decisions.get("shell"),
            Some(&ToolDecision::Deny)
        );
    }

    #[test]
    fn tool_decisions_omitted_from_toml_when_empty() {
        // `skip_serializing_if = "HashMap::is_empty"` keeps the
        // TOML minimal and matches the existing v0.1 on-disk format
        // for users upgrading from a build that didn't have this
        // field.
        let cfg = GlobalConfig::default();
        let s = toml::to_string_pretty(&cfg).unwrap();
        assert!(
            !s.contains("default_tool_decisions"),
            "default_config.toml should not contain the empty map: {s}"
        );
    }
}

/// Patch for a `GlobalConfig`. All fields are optional; `None`
/// means "leave unchanged". Used by the `config_update_global`
/// Tauri command (F05).
///
/// Per `config.md` §Contracts, secrets are **never** written via
/// the patch. The UI uses the dedicated `secrets_set` command for
/// that, which writes to the OS keychain.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalConfigPatch {
    /// New approval mode, if changing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_mode: Option<ApprovalMode>,
    /// New default provider, if changing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_provider: Option<ProviderId>,
    /// New default model, if changing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    /// Provider updates, keyed by provider id. Add/update/remove
    /// semantics: if the entry exists, it is updated; if it doesn't
    /// exist, it is added. The patch does NOT support removing
    /// providers (UI uses a separate flow).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub providers: Option<HashMap<ProviderId, ProviderConfig>>,
    /// New UI settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiConfig>,
    /// Toggle telemetry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telemetry_enabled: Option<bool>,
    /// Toggle update check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_updates: Option<bool>,
    /// New update channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_channel: Option<UpdateChannel>,
}

impl GlobalConfigPatch {
    /// Returns `true` if the patch contains no changes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.approval_mode.is_none()
            && self.default_provider.is_none()
            && self.default_model.is_none()
            && self.providers.is_none()
            && self.ui.is_none()
            && self.telemetry_enabled.is_none()
            && self.check_updates.is_none()
            && self.update_channel.is_none()
    }

    /// Apply the patch to `cfg`, returning the new `GlobalConfig`.
    /// Re-validation is the caller's responsibility.
    pub fn apply_to(&self, cfg: &mut GlobalConfig) {
        if let Some(m) = self.approval_mode {
            cfg.approval_mode = m;
        }
        if let Some(ref p) = self.default_provider {
            cfg.default_provider = p.clone();
        }
        if let Some(ref m) = self.default_model {
            cfg.default_model = m.clone();
        }
        if let Some(ref providers) = self.providers {
            for (id, p) in providers {
                cfg.providers.insert(id.clone(), p.clone());
            }
        }
        if let Some(ref ui) = self.ui {
            cfg.ui = ui.clone();
        }
        if let Some(t) = self.telemetry_enabled {
            cfg.telemetry_enabled = t;
        }
        if let Some(c) = self.check_updates {
            cfg.check_updates = c;
        }
        if let Some(c) = self.update_channel {
            cfg.update_channel = c;
        }
    }
}

/// Patch for a `WorkspaceConfig`. Mirrors `GlobalConfigPatch`
/// but with workspace-scoped fields only.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceConfigPatch {
    /// New approval mode override, if changing. `Some(None)` is
    /// not representable in TOML; to clear an override, the UI
    /// re-sends the full workspace config without the field. Here
    /// we use a wrapper: `Some(Some(mode))` to set, `None` to leave
    /// alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_mode: Option<Option<ApprovalMode>>,
    /// New default provider override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_provider: Option<Option<ProviderId>>,
    /// New default model override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<Option<String>>,
    /// New workspace settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceSettings>,
}

impl WorkspaceConfigPatch {
    /// Returns `true` if the patch contains no changes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.approval_mode.is_none()
            && self.default_provider.is_none()
            && self.default_model.is_none()
            && self.workspace.is_none()
    }

    /// Apply the patch to `cfg`, returning the new `WorkspaceConfig`.
    pub fn apply_to(&self, cfg: &mut WorkspaceConfig) {
        if let Some(m) = self.approval_mode {
            cfg.approval_mode = m;
        }
        if let Some(ref p) = self.default_provider {
            cfg.default_provider = p.clone();
        }
        if let Some(ref m) = self.default_model {
            cfg.default_model = m.clone();
        }
        if let Some(ref w) = self.workspace {
            cfg.workspace = w.clone();
        }
    }
}

/// In-memory resolved config: global + workspace overrides, with
/// secrets already expanded. **Never** serialized to disk; **never**
/// included in the DTOs that cross the IPC boundary.
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// The global config (latest snapshot from `ConfigService`).
    pub global: GlobalConfig,
    /// The workspace config (if any; `None` for a brand-new
    /// workspace with no `config.toml`).
    pub workspace: Option<WorkspaceConfig>,
    /// API keys per provider, already expanded from `SecretRef`.
    /// The values are **never** serialized to disk and **never**
    /// logged. They live only in the in-memory `AppState.config`
    /// and are read by the agent loop / providers.
    pub secrets: HashMap<ProviderId, String>,
    /// The effective final config (workspace override > global).
    pub effective: EffectiveConfig,
}

/// Final, fully-resolved config. Computed by `resolve()` and
/// consumed by the agent loop and providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveConfig {
    /// Approval mode (workspace override > global).
    pub approval_mode: ApprovalMode,
    /// Default provider (workspace override > global).
    pub default_provider: ProviderId,
    /// Default model (workspace override > global).
    pub default_model: String,
    /// Workspace settings (always present; defaults if the
    /// workspace has no `config.toml`).
    pub workspace_settings: WorkspaceSettings,
}

impl EffectiveConfig {
    /// Compute the effective config from a global + optional
    /// workspace. Workspace overrides take precedence.
    #[must_use]
    pub fn from_configs(global: &GlobalConfig, workspace: Option<&WorkspaceConfig>) -> Self {
        let ws_settings =
            workspace
                .map(|w| w.workspace.clone())
                .unwrap_or_else(|| WorkspaceSettings {
                    ignore_patterns: WorkspaceSettings::default_ignore(),
                    journal_max_rows: Some(100_000),
                });
        Self {
            approval_mode: workspace
                .and_then(|w| w.approval_mode)
                .unwrap_or(global.approval_mode),
            default_provider: workspace
                .and_then(|w| w.default_provider.clone())
                .unwrap_or_else(|| global.default_provider.clone()),
            default_model: workspace
                .and_then(|w| w.default_model.clone())
                .unwrap_or_else(|| global.default_model.clone()),
            workspace_settings: ws_settings,
        }
    }
}
