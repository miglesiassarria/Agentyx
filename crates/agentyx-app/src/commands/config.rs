//! `config` Tauri commands — F05 settings surface.
//!
//! See `../../../specs/features/F05-settings.md` and
//! `../../../specs/domains/config.md` for the contracts.
//!
//! ## Security
//!
//! The DTOs returned to the UI **never** include resolved secret
//! values. API keys travel through the dedicated `secrets_set` /
//! `secrets_delete` / `secrets_list_providers` commands, which
//! write to the OS keychain and never log the value.

use agentyx_core::config::{
    GlobalConfig, GlobalConfigPatch, ResolvedConfigSnapshot, WorkspaceConfig, WorkspaceConfigPatch,
};
use agentyx_core::ids::WorkspaceId;
use agentyx_core::AppResult;
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

use crate::state::AppState;

/// DTO for the global config as exposed to the UI. The shape is
/// `#[serde(rename_all = "camelCase")]` and is identical to the
/// `GlobalConfig` from `agentyx_core` minus any fields that should
/// not cross the IPC boundary (none today; secret values are
/// resolved via the dedicated `secrets_*` commands).
pub type GlobalConfigDto = GlobalConfig;

/// DTO for a workspace's config as exposed to the UI. Mirrors
/// `WorkspaceConfig`.
pub type WorkspaceConfigDto = WorkspaceConfig;

/// DTO for the resolved config (global + workspace + effective).
/// Does **not** include secrets.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedConfigDto {
    /// The global config snapshot.
    pub global: GlobalConfigDto,
    /// The workspace config (if any).
    pub workspace: Option<WorkspaceConfigDto>,
    /// The effective final config (workspace override > global).
    pub effective: agentyx_core::config::EffectiveConfig,
    /// List of provider ids that have a secret in the keychain.
    /// Used by the UI to render the "API key: set in keychain"
    /// badge without ever returning the value.
    #[serde(rename = "keychainProviderIds")]
    pub keychain_provider_ids: Vec<String>,
}

impl ResolvedConfigDto {
    fn from_snapshot(snap: ResolvedConfigSnapshot, keychain_provider_ids: Vec<String>) -> Self {
        Self {
            global: snap.global,
            workspace: snap.workspace,
            effective: snap.effective,
            keychain_provider_ids,
        }
    }
}

/// Get the global config (without secrets).
#[tauri::command]
pub async fn config_get_global(state: State<'_, Arc<AppState>>) -> AppResult<GlobalConfigDto> {
    Ok(state.config.get())
}

/// Patch and persist the global config. The patch is rejected if
/// it would leave the config invalid. The DTO returned reflects
/// the new state.
#[tauri::command]
pub async fn config_update_global(
    state: State<'_, Arc<AppState>>,
    patch: GlobalConfigPatch,
) -> AppResult<GlobalConfigDto> {
    state.config.update_with_patch(&patch)
}

/// Get a workspace's resolved config. Returns the workspace
/// config, the global config, the effective config, and the
/// list of provider ids that have a keychain entry.
#[tauri::command]
pub async fn config_get_workspace(
    state: State<'_, Arc<AppState>>,
    workspace_id: WorkspaceId,
) -> AppResult<ResolvedConfigDto> {
    let snap = state.config.resolve_snapshot(workspace_id)?;
    let ids = state.config.list_keychain_providers()?;
    Ok(ResolvedConfigDto::from_snapshot(snap, ids))
}

/// Patch and persist a workspace's config. Returns the new
/// workspace config DTO.
#[tauri::command]
pub async fn config_update_workspace(
    state: State<'_, Arc<AppState>>,
    workspace_id: WorkspaceId,
    patch: WorkspaceConfigPatch,
) -> AppResult<WorkspaceConfigDto> {
    state.config.update_workspace(workspace_id, &patch)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::field_reassign_with_default
)]
mod tests {
    use super::*;
    use crate::events::EventBus;
    use agentyx_core::agents::AgentRegistry;
    use agentyx_core::config::{FakeKeychain, ServiceConfigPaths};
    use agentyx_core::permissions::{PermissionGate, PermissionRegistry};
    use agentyx_core::tools::{built_in_registry, Tool};
    use std::collections::HashMap;
    use std::sync::Mutex;

    async fn fresh_state() -> (tempfile::TempDir, Arc<AppState>) {
        let home = tempfile::tempdir().unwrap();
        let paths = ServiceConfigPaths::from_agentyx_home(home.path());
        let keychain: Arc<dyn agentyx_core::config::KeychainAccess> = Arc::new(FakeKeychain::new());
        let config = Arc::new(
            agentyx_core::config::ConfigService::load_with_keychain(&paths, keychain).unwrap(),
        );
        let workspaces =
            Arc::new(agentyx_core::workspace::WorkspaceService::new(home.path()).unwrap());
        let agents = Arc::new(AgentRegistry::load_builtins());
        let providers = Arc::new(crate::state::ProviderRegistry::from_config(&config).unwrap());
        let tool_registry: Arc<Vec<Arc<dyn Tool>>> =
            Arc::new(built_in_registry().into_iter().collect());
        let state = Arc::new(AppState {
            agentyx_home: home.path().to_path_buf(),
            workspaces,
            config,
            agents,
            providers,
            runs: Arc::new(agentyx_core::agent::RunRegistry::new()),
            event_bus: Arc::new(EventBus::new()),
            workspace_runtimes: Mutex::new(HashMap::new()),
            tool_registry,
            permission_gate: PermissionGate::new(),
            permission_registry: PermissionRegistry::new(),
        });
        (home, state)
    }

    // The bulk of the config ACs (AC1, AC2, AC4-AC11, AC14, AC15)
    // are covered by unit tests in
    // `agentyx_core::config::service::tests` and
    // `agentyx_core::config::schema::tests`. Here we only test
    // the Tauri command layer, which is responsible for DTO
    // construction (e.g. never exposing resolved secrets).
    #[tokio::test]
    async fn f05_ac12_resolved_config_dto_never_includes_secrets() {
        // F05.AC12 (DTO half): the ResolvedConfigDto serialized
        // to JSON must not contain resolved keychain values.
        let keychain = FakeKeychain::with_entries(&[("groq", "secret-groq-value-42")]);
        let home = tempfile::tempdir().unwrap();
        let paths = ServiceConfigPaths::from_agentyx_home(home.path());
        let config = Arc::new(
            agentyx_core::config::ConfigService::load_with_keychain(&paths, Arc::new(keychain))
                .unwrap(),
        );
        let mut providers = HashMap::new();
        providers.insert(
            "groq".into(),
            agentyx_core::config::ProviderConfig {
                base_url: "https://api.groq.com/openai/v1".into(),
                enabled: true,
                api_key: Some(agentyx_core::config::SecretRef::Keychain {
                    account: "groq".into(),
                }),
                models: None,
            },
        );
        let patch = GlobalConfigPatch {
            providers: Some(providers),
            ..Default::default()
        };
        let _ = config.update_with_patch(&patch).unwrap();

        let snap = config.resolve_snapshot(WorkspaceId::new()).unwrap();
        let dto = ResolvedConfigDto::from_snapshot(snap, config.list_keychain_providers().unwrap());
        let json = serde_json::to_string(&dto).unwrap();
        assert!(
            !json.contains("secret-groq-value-42"),
            "DTO must not contain the resolved keychain value: {json}"
        );
    }

    #[tokio::test]
    async fn f05_resolved_dto_lists_keychain_providers_by_id() {
        // The DTO includes the keychain provider ids (used by
        // the UI to render the "API key: set in keychain"
        // badge) but never the values.
        let keychain = FakeKeychain::with_entries(&[("groq", "v1"), ("minimax", "v2")]);
        let home = tempfile::tempdir().unwrap();
        let paths = ServiceConfigPaths::from_agentyx_home(home.path());
        let config = Arc::new(
            agentyx_core::config::ConfigService::load_with_keychain(&paths, Arc::new(keychain))
                .unwrap(),
        );
        let mut providers = HashMap::new();
        for (id, account) in [("groq", "groq"), ("minimax", "minimax")] {
            providers.insert(
                id.into(),
                agentyx_core::config::ProviderConfig {
                    base_url: "https://x".into(),
                    enabled: true,
                    api_key: Some(agentyx_core::config::SecretRef::Keychain {
                        account: account.into(),
                    }),
                    models: None,
                },
            );
        }
        let patch = GlobalConfigPatch {
            providers: Some(providers),
            ..Default::default()
        };
        let _ = config.update_with_patch(&patch).unwrap();

        let snap = config.resolve_snapshot(WorkspaceId::new()).unwrap();
        let dto = ResolvedConfigDto::from_snapshot(snap, config.list_keychain_providers().unwrap());
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("\"keychainProviderIds\""));
        assert!(json.contains("groq"));
        assert!(json.contains("minimax"));
        assert!(!json.contains("v1"));
        assert!(!json.contains("v2"));
    }

    #[tokio::test]
    async fn f05_config_get_global_returns_dto() {
        // The Tauri command returns the global config as a DTO.
        let (_home, state) = fresh_state().await;
        let dto = state.config.get();
        // The default has Ollama preconfigured.
        assert!(dto.providers.contains_key("ollama"));
    }
}
