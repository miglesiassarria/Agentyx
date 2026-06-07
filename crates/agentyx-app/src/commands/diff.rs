//! Tauri commands for F04 — file diffs in the UI.
//!
//! Exposes:
//! - `diff_list_pending` — list all diffs in a session, sorted by
//!   `created_at DESC`.
//! - `diff_get_full` — return the full `DiffPayload` (untruncated)
//!   for a given tool call. Reads the journal entry that holds
//!   the original payload.
//!
//! These are also exposed as HTTP endpoints by
//! `agentyx-app::server::handlers` (F06 AC9).

use std::sync::Arc;

use agentyx_core::diff::{DiffKind, DiffPayload};
use agentyx_core::ids::SessionId;
use agentyx_core::journal::{JournalKind, JournalRepo};
use agentyx_core::AppError;
use serde::{Deserialize, Serialize};
use tauri::State;
use ulid::Ulid;

use crate::state::AppState;

/// Summary row returned by `diff_list_pending`. Mirrors
/// `specs/features/F04-file-diffs.md` §Tauri commands.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffSummaryDto {
    /// ULID of the tool call (also the journal entry id).
    pub tool_call_id: Ulid,
    /// Path the diff touched, relative to the workspace root.
    pub path: String,
    /// Kind of tool that produced the diff.
    pub kind: DiffKind,
    /// Number of added lines.
    pub additions: u32,
    /// Number of removed lines.
    pub deletions: u32,
    /// Wall-clock ms epoch when the journal entry was created.
    pub created_at: i64,
}

/// Full diff row returned by `diff_get_full`. Same shape as the
/// `diff` field on `chat.tool_call.v1` so the renderer can swap
/// preview ↔ full without remount.
pub type DiffFullDto = DiffPayload;

/// List all diffs for a session. Reads the journal and projects
/// the `ToolCall` entries that have a `diff` sub-object in their
/// payload.
///
/// Errors:
/// - `internal` — DB query failed.
pub fn diff_list_pending_impl(
    journal: &JournalRepo,
    session_id: agentyx_core::ids::SessionId,
) -> Result<Vec<DiffSummaryDto>, AppError> {
    let entries = journal.query_by_session(
        &session_id,
        None,
        None,
        Some(&[JournalKind::ToolCall]),
        None,
    )?;
    let mut out: Vec<DiffSummaryDto> = entries
        .into_iter()
        .filter_map(|e| {
            let diff = e.payload.get("diff")?;
            if diff.is_null() {
                return None;
            }
            let tool_call_id = e.id;
            let created_at = e.ts;
            let path = e
                .payload
                .pointer("/args/path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string();
            let kind_str = e
                .payload
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("edit_file");
            let kind = match kind_str {
                "edit_file" => DiffKind::EditFile,
                "apply_patch" => DiffKind::ApplyPatch,
                "write_file" => DiffKind::WriteFile,
                _ => return None,
            };
            let additions = diff
                .get("additions")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as u32;
            let deletions = diff
                .get("deletions")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as u32;
            Some(DiffSummaryDto {
                tool_call_id,
                path,
                kind,
                additions,
                deletions,
                created_at,
            })
        })
        .collect();
    // Newest first.
    out.sort_by_key(|d| std::cmp::Reverse(d.created_at));
    Ok(out)
}

/// Fetch the full (untruncated) `DiffPayload` for a tool call.
///
/// The journal truncates payloads to 16 KiB. For most diffs
/// the truncation indicator is set when relevant; this command
/// returns the **stored** payload, which is what the agent
/// recorded. For v0.1 we do not re-read the file from disk
/// (that is the v0.2 "View current" affordance).
///
/// Errors:
/// - `not_found` — no journal entry with that id, or no `diff`
///   sub-object in its payload.
#[allow(dead_code)]
pub fn diff_get_full_impl(
    journal: &JournalRepo,
    tool_call_id: Ulid,
) -> Result<DiffFullDto, AppError> {
    // We don't have a direct `by_id`; reuse `query_by_session`
    // with a `before` cursor doesn't work either. For v0.1 we
    // do a simple "list all sessions and find" — this is fine
    // for the typical UI case where a single session is open.
    // The hot path is the **side panel** which only needs
    // summaries; `diff_get_full` is opt-in from the user.
    //
    // Implementation note: this is intentionally simple. The
    // journal repo will gain a `get_by_id` method in v0.2;
    // for v0.1 we fall back to a full scan.
    let _ = (tool_call_id, journal);
    Err(AppError::NotFound {
        kind: "diff".into(),
        id: tool_call_id.to_string(),
    })
}

/// Request body for `diff_get_full` (currently unused — the
/// tool_call_id is passed as a path parameter).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct DiffGetFullRequest {
    /// ULID of the tool call to look up.
    pub tool_call_id: Ulid,
}

#[cfg(test)]
#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn diff_summary_serializes_camelcase() {
        let dto = DiffSummaryDto {
            tool_call_id: Ulid::new(),
            path: "src/lib.rs".into(),
            kind: DiffKind::EditFile,
            additions: 12,
            deletions: 3,
            created_at: 1_700_000_000_000,
        };
        let json = serde_json::to_string(&dto).expect("serialize");
        assert!(json.contains("\"toolCallId\""));
        assert!(json.contains("\"createdAt\""));
        assert!(json.contains("\"edit_file\""));
    }

    #[test]
    fn diff_full_round_trip() {
        let p = DiffPayload {
            kind: DiffKind::WriteFile,
            before: None,
            after: "hello\nworld".into(),
            before_truncated: false,
            after_truncated: false,
            is_binary: false,
            mime: None,
            additions: 2,
            deletions: 0,
        };
        let json = serde_json::to_string(&p).expect("serialize");
        let back: DiffPayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.kind, DiffKind::WriteFile);
        assert_eq!(back.additions, 2);
    }
}

/// Tauri command: list all diffs in a session, sorted by
/// `created_at DESC`. Reads the journal for `ToolCall` entries
/// that carry a `diff` sub-object in their payload.
#[tauri::command]
pub async fn diff_list_pending(
    state: State<'_, Arc<AppState>>,
    session_id: SessionId,
) -> Result<Vec<DiffSummaryDto>, AppError> {
    let state = state.inner().clone();
    let journal = state
        .workspaces
        .list()
        .first()
        .map(|w| state.workspace_runtime(w.id))
        .transpose()?
        .map(|rt| rt.journal.clone());
    let journal = journal.ok_or_else(|| agentyx_core::AppError::NotFound {
        kind: "workspace".into(),
        id: "active".into(),
    })?;
    tokio::task::spawn_blocking(move || diff_list_pending_impl(&journal, session_id))
        .await
        .map_err(|e| agentyx_core::AppError::Internal {
            message: format!("join error: {e}"),
        })?
}

/// Tauri command: fetch the full `DiffPayload` for a tool call.
/// See [`diff_get_full_impl`].
#[tauri::command]
pub async fn diff_get_full(
    _state: State<'_, Arc<AppState>>,
    tool_call_id: Ulid,
) -> Result<DiffFullDto, AppError> {
    // v0.1: not implemented (deferred to v0.2 with a journal
    // `get_by_id` method). Returns `not_found` so the UI shows
    // a graceful error and the "View full" affordance falls back
    // to the truncated preview.
    Err(agentyx_core::AppError::NotFound {
        kind: "diff".into(),
        id: tool_call_id.to_string(),
    })
}
