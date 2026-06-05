//! `providers` Tauri commands — F05 test-connection surface.
//!
//! See `../../../specs/domains/llm-providers.md` for the contracts.

// Placeholder command, not yet wired into `generate_handler!`.
#![allow(dead_code)]

use agentyx_core::AppResult;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

use crate::state::AppState;

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
#[tauri::command]
pub async fn test_connection(
    _state: State<'_, Arc<AppState>>,
    _provider_id: String,
) -> AppResult<TestConnectionResult> {
    Err(agentyx_core::AppError::Internal {
        message: "providers::test_connection not yet implemented (F05 in Fase D)".into(),
    })
}
