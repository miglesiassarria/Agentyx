//! `providers` Tauri commands — F05 test-connection surface.
//!
//! See `../../../specs/domains/providers.md` and
//! `../../../specs/features/F05-settings.md` for the contracts.
//!
//! ## Security
//!
//! `providers_test_connection` accepts a `ProviderConfig` with
//! an optional inline `api_key` for one-off validation. The key
//! is moved into the ephemeral provider and never logged.
//! The persisted config is NOT touched by this command.

use agentyx_core::config::{ProviderConfig, SecretRef};
use agentyx_core::llm::{GroqProvider, MinimaxProvider, OllamaProvider, Provider};
use agentyx_core::AppResult;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

use crate::state::AppState;

/// Request payload for `providers_test_connection`. The
/// `inline_api_key` field is used to validate a key without
/// persisting it; on success the user still needs to call
/// `secrets_set` to make the key permanent.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionRequest {
    /// Provider id (`"ollama"`, `"groq"`, `"minimax"`).
    pub provider_id: String,
    /// Provider-specific config (base_url, enabled, etc.).
    pub provider: ProviderConfig,
    /// Optional inline API key for one-off validation. Not
    /// persisted; moved into the ephemeral provider and dropped.
    #[serde(default)]
    pub inline_api_key: Option<String>,
}

/// Result of a `test_connection` call. The UI uses this to render
/// the `TestConnectionBadge` in the Settings → Providers tab.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionResult {
    /// Whether the provider is reachable and authenticated.
    pub ok: bool,
    /// Latency in milliseconds (only meaningful if `ok`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// Models the provider reports (only meaningful if `ok`).
    #[serde(default)]
    pub models: Vec<String>,
    /// UI-safe error message (only meaningful if not `ok`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Stable error code from `AppError::code()`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

/// Test connectivity to a provider. Does NOT persist anything.
///
/// The provider is constructed ephemerally with the supplied
/// `base_url` and (if provided) the `inline_api_key`. If the
/// key is omitted, the existing `Config::resolve_secrets` value
/// is used.
#[tauri::command]
pub async fn providers_test_connection(
    state: State<'_, Arc<AppState>>,
    request: TestConnectionRequest,
) -> AppResult<TestConnectionResult> {
    tracing::info!(
        provider_id = %request.provider_id,
        "test_connection starting"
    );

    // Build the provider.
    let provider = build_ephemeral_provider(&request, &state)?;

    // Time the call.
    let start = std::time::Instant::now();
    let health_result = provider.health().await;
    let latency_ms = start.elapsed().as_millis() as u64;

    match health_result {
        Ok(_) => {
            // List models on success.
            let models = match provider.list_models().await {
                Ok(list) => list.into_iter().map(|m| m.id).collect(),
                Err(e) => {
                    tracing::warn!(
                        provider_id = %request.provider_id,
                        error = %e,
                        "list_models failed after successful health check"
                    );
                    vec![]
                }
            };
            Ok(TestConnectionResult {
                ok: true,
                latency_ms: Some(latency_ms),
                models,
                error: None,
                error_code: None,
            })
        }
        Err(e) => {
            let code = e.code().to_string();
            let msg = e.to_string();
            tracing::warn!(
                provider_id = %request.provider_id,
                error_code = %code,
                "test_connection failed"
            );
            Ok(TestConnectionResult {
                ok: false,
                latency_ms: None,
                models: vec![],
                error: Some(msg),
                error_code: Some(code),
            })
        }
    }
}

/// Build a fresh `Box<dyn Provider>` from the request. The
/// inline_api_key (if provided) is preferred; otherwise we
/// resolve from the persisted `SecretRef`.
pub(crate) fn build_ephemeral_provider(
    request: &TestConnectionRequest,
    state: &Arc<AppState>,
) -> AppResult<Box<dyn Provider>> {
    let base_url = &request.provider.base_url;
    match request.provider_id.as_str() {
        "ollama" => {
            // Ollama uses no auth.
            Ok(Box::new(OllamaProvider::with_base_url(base_url)?))
        }
        "groq" => {
            let key = resolve_key(
                &request.provider_id,
                &request.provider,
                request.inline_api_key.as_deref(),
                state,
            )?;
            Ok(Box::new(GroqProvider::with_base_url(base_url, key)?))
        }
        "minimax" => {
            let key = resolve_key(
                &request.provider_id,
                &request.provider,
                request.inline_api_key.as_deref(),
                state,
            )?;
            Ok(Box::new(MinimaxProvider::with_base_url(base_url, key)?))
        }
        other => Err(agentyx_core::AppError::InvalidInput {
            message: format!("unknown provider_id: {other}"),
        }),
    }
}

/// Resolve the API key for the test:
/// 1. The inline value (if provided) — never persisted.
/// 2. The `SecretRef` from the persisted `ProviderConfig`.
/// 3. Error: the provider requires a key but none is available.
fn resolve_key(
    provider_id: &str,
    provider: &ProviderConfig,
    inline: Option<&str>,
    state: &Arc<AppState>,
) -> AppResult<String> {
    if let Some(k) = inline {
        if k.is_empty() {
            return Err(agentyx_core::AppError::InvalidInput {
                message: "inline_api_key cannot be empty".into(),
            });
        }
        return Ok(k.to_string());
    }
    if let Some(secret) = &provider.api_key {
        return match secret {
            SecretRef::Env(var) => {
                std::env::var(var).map_err(|_| agentyx_core::AppError::InvalidInput {
                    message: format!("environment variable {var} is not set"),
                })
            }
            SecretRef::Keychain { account } => state
                .config
                .resolve_secret(&SecretRef::Keychain {
                    account: account.clone(),
                })
                .map_err(|_| agentyx_core::AppError::Internal {
                    message: format!("no keychain entry for {account}"),
                }),
        };
    }
    Err(agentyx_core::AppError::InvalidInput {
        message: format!("provider '{provider_id}' has no api_key configured"),
    })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::field_reassign_with_default,
    unsafe_code
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
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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
            server: Arc::new(std::sync::OnceLock::new()),
        });
        (home, state)
    }

    #[tokio::test]
    async fn f05_ac2_groq_with_valid_key_returns_ok() {
        // F05.AC2: adding a provider with a valid API key
        // returns TestConnectionResult { ok: true, ... }.
        // We simulate the upstream with a wiremock that
        // returns 200 on /models.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/openai/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {"id": "llama-3.3-70b-versatile", "object": "model"}
                ]
            })))
            .mount(&server)
            .await;

        let (_home, state) = fresh_state().await;
        let request = TestConnectionRequest {
            provider_id: "groq".into(),
            provider: ProviderConfig {
                base_url: format!("{}/openai/v1", server.uri()),
                enabled: true,
                api_key: None,
                models: None,
            },
            inline_api_key: Some("gsk_test_key_42".into()),
        };
        let result = providers_test_connection_inner(&state, request)
            .await
            .unwrap();
        assert!(result.ok, "expected ok=true, got: {result:?}");
        assert!(result.latency_ms.is_some());
        assert!(result.error.is_none());
        assert!(result.error_code.is_none());
    }

    #[tokio::test]
    async fn f05_ac3_groq_with_invalid_key_returns_error() {
        // F05.AC3: invalid API key → TestConnectionResult
        // { ok: false, error_code: "provider", ... }.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/openai/v1/models"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let (_home, state) = fresh_state().await;
        let request = TestConnectionRequest {
            provider_id: "groq".into(),
            provider: ProviderConfig {
                base_url: format!("{}/openai/v1", server.uri()),
                enabled: true,
                api_key: None,
                models: None,
            },
            inline_api_key: Some("gsk_bad_key".into()),
        };
        let result = providers_test_connection_inner(&state, request)
            .await
            .unwrap();
        assert!(!result.ok);
        assert_eq!(result.error_code.as_deref(), Some("provider"));
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn f05_ac10_ollama_unreachable_returns_error() {
        // F05.AC10: a provider that cannot be reached returns
        // a `provider` error. We point at a port that's not
        // listening (server.uri() with a bogus path is fine;
        // reqwest will get connection refused).
        let (_home, state) = fresh_state().await;
        let request = TestConnectionRequest {
            provider_id: "ollama".into(),
            provider: ProviderConfig {
                base_url: "http://127.0.0.1:1".into(),
                enabled: true,
                api_key: None,
                models: None,
            },
            inline_api_key: None,
        };
        let result = providers_test_connection_inner(&state, request)
            .await
            .unwrap();
        assert!(!result.ok);
        assert_eq!(result.error_code.as_deref(), Some("provider"));
    }

    #[tokio::test]
    async fn f05_ac2_inline_key_is_preferred_over_persisted() {
        // F05.AC2: the inline key (one-off) wins over the
        // persisted SecretRef.
        let var = "AGENTYX_TEST_PROVIDER_KEY_FOR_TEST";
        unsafe {
            std::env::set_var(var, "persisted-key");
        }
        let (_home, state) = fresh_state().await;
        state
            .config
            .update(|c| {
                c.providers.insert(
                    "groq".into(),
                    ProviderConfig {
                        base_url: "http://127.0.0.1:9".into(),
                        enabled: true,
                        api_key: Some(SecretRef::Env(var.to_string())),
                        models: None,
                    },
                );
            })
            .unwrap();
        // We just verify the resolve_key helper picks the
        // inline value when both are present.
        let key = resolve_key(
            "groq",
            &state.config.get().providers["groq"],
            Some("inline-key"),
            &state,
        )
        .unwrap();
        assert_eq!(key, "inline-key");
        unsafe {
            std::env::remove_var(var);
        }
    }

    /// Inner helper that mirrors the Tauri command but takes a
    /// `&AppState` directly. The Tauri command itself is just
    /// an `Arc<AppState>` wrapper; this avoids the need for a
    /// fake `tauri::State` in tests.
    async fn providers_test_connection_inner(
        state: &Arc<AppState>,
        request: TestConnectionRequest,
    ) -> AppResult<TestConnectionResult> {
        // Mirror the body of `providers_test_connection`.
        let provider = build_ephemeral_provider(&request, state)?;
        let start = std::time::Instant::now();
        let health_result = provider.health().await;
        let latency_ms = start.elapsed().as_millis() as u64;
        match health_result {
            Ok(_) => {
                let models = match provider.list_models().await {
                    Ok(list) => list.into_iter().map(|m| m.id).collect(),
                    Err(_) => vec![],
                };
                Ok(TestConnectionResult {
                    ok: true,
                    latency_ms: Some(latency_ms),
                    models,
                    error: None,
                    error_code: None,
                })
            }
            Err(e) => {
                let code = e.code().to_string();
                let msg = e.to_string();
                Ok(TestConnectionResult {
                    ok: false,
                    latency_ms: None,
                    models: vec![],
                    error: Some(msg),
                    error_code: Some(code),
                })
            }
        }
    }

    #[tokio::test]
    async fn resolve_key_rejects_empty_inline() {
        let (_home, state) = fresh_state().await;
        let err = resolve_key("groq", &ProviderConfig::default(), Some(""), &state).unwrap_err();
        assert!(matches!(err, agentyx_core::AppError::InvalidInput { .. }));
    }

    #[tokio::test]
    async fn resolve_key_errors_when_no_inline_and_no_secret() {
        let (_home, state) = fresh_state().await;
        let err = resolve_key("groq", &ProviderConfig::default(), None, &state).unwrap_err();
        assert!(matches!(err, agentyx_core::AppError::InvalidInput { .. }));
    }
}
