//! `secrets` Tauri commands — F05 keychain surface.
//!
//! SECURITY: the value parameter is a one-shot string. It is moved
//! into the keychain in the same call and is never logged. The
//! `tracing` calls in this module MUST NOT include the value.

use agentyx_core::AppResult;
use std::sync::Arc;
use tauri::State;

use crate::state::AppState;

/// Persist a secret (API key) in the OS keychain under the
/// `agentyx` service. Replaces any existing entry for the same
/// provider. **The value is never logged.**
#[tauri::command]
pub async fn set_secret(
    state: State<'_, Arc<AppState>>,
    provider_id: String,
    value: String,
) -> AppResult<()> {
    tracing::info!(provider_id = %provider_id, "setting provider secret");
    state.config.set_keychain(&provider_id, &value)
}

/// Delete a secret from the keychain. No-op if no entry exists.
#[tauri::command]
pub async fn delete_secret(state: State<'_, Arc<AppState>>, provider_id: String) -> AppResult<()> {
    tracing::info!(provider_id = %provider_id, "deleting provider secret");
    state.config.delete_keychain(&provider_id)
}

/// List provider ids that have a secret set. Does NOT return
/// values; the UI uses this to render the "API key: set in
/// keychain" badge.
#[tauri::command]
pub async fn list_providers(state: State<'_, Arc<AppState>>) -> AppResult<Vec<String>> {
    state.config.list_keychain_providers()
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
            event_bus: Arc::new(crate::events::EventBus::new()),
            workspace_runtimes: Mutex::new(HashMap::new()),
            tool_registry,
            permission_gate: PermissionGate::new(),
            permission_registry: PermissionRegistry::new(),
            server: Arc::new(std::sync::OnceLock::new()),
        });
        (home, state)
    }

    // ===============================================================
    // F05.AC7 — secrets_set then list returns only IDs
    // ===============================================================

    #[tokio::test]
    async fn f05_ac7_secrets_set_then_list_returns_only_ids() {
        // F05.AC7: after secrets_set("ollama", "..."), list_providers
        // returns ["ollama"] and the value is never in the response.
        // Note: only providers in cfg.providers are listed; the default
        // config has "ollama" preconfigured.
        let (_home, state) = fresh_state().await;

        set_secret_inner(&state, "ollama", "secret_for_ollama_42")
            .await
            .unwrap();
        let list = list_providers_inner(&state).await.unwrap();
        assert!(
            list.contains(&"ollama".to_string()),
            "ollama should be in list after set_keychain, got: {list:?}"
        );
        // The value must not leak.
        let json = serde_json::to_string(&list).unwrap();
        assert!(
            !json.contains("secret_for_ollama_42"),
            "list must not contain the secret value"
        );
    }

    // ===============================================================
    // F05.AC8 — missing env at runtime surfaces to UI
    // ===============================================================

    #[tokio::test]
    async fn f05_ac8_missing_env_at_runtime_surfaces_to_ui() {
        // F05.AC8: if a provider uses an env-ref secret and the
        // env var is not set, resolving it returns InvalidInput
        // with a clear message. The UI shows this as a toast.
        let (_home, state) = fresh_state().await;

        // Set a groq provider with an env ref to a non-existent var.
        state
            .config
            .update(|c| {
                c.providers.insert(
                    "groq".into(),
                    agentyx_core::config::ProviderConfig {
                        base_url: "https://api.groq.com/openai/v1".into(),
                        enabled: true,
                        api_key: Some(agentyx_core::config::SecretRef::Env(
                            "AGENTYXdefinitivelynotsetXYZ123".to_string(),
                        )),
                        models: None,
                    },
                );
            })
            .unwrap();

        // Trying to resolve the secret should fail.
        let err = state
            .config
            .resolve_secret(&agentyx_core::config::SecretRef::Env(
                "AGENTYXdefinitivelynotsetXYZ123".to_string(),
            ))
            .unwrap_err();
        assert!(matches!(err, agentyx_core::AppError::InvalidInput { .. }));
        let msg = err.to_string();
        assert!(
            msg.contains("AGENTYXdefinitivelynotsetXYZ123") || msg.contains("not set"),
            "error message should mention the missing var: {msg}"
        );
    }

    // Inner helpers that mirror the Tauri commands but take &AppState.

    async fn set_secret_inner(
        state: &Arc<AppState>,
        provider_id: &str,
        value: &str,
    ) -> AppResult<()> {
        state.config.set_keychain(provider_id, value)
    }

    async fn list_providers_inner(state: &Arc<AppState>) -> AppResult<Vec<String>> {
        state.config.list_keychain_providers()
    }
}
