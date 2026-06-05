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
pub async fn set(
    _state: State<'_, Arc<AppState>>,
    _provider_id: String,
    _value: String,
) -> AppResult<()> {
    Err(agentyx_core::AppError::Internal {
        message: "secrets::set not yet implemented (F05 in Fase D)".into(),
    })
}

/// Delete a secret from the keychain. No-op if no entry exists.
#[tauri::command]
pub async fn delete(
    _state: State<'_, Arc<AppState>>,
    _provider_id: String,
) -> AppResult<()> {
    Err(agentyx_core::AppError::Internal {
        message: "secrets::delete not yet implemented (F05 in Fase D)".into(),
    })
}

/// List provider ids that have a secret set. Does NOT return values.
#[tauri::command]
pub async fn list_providers(
    _state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<String>> {
    Err(agentyx_core::AppError::Internal {
        message: "secrets::list_providers not yet implemented (F05 in Fase D)".into(),
    })
}
