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
