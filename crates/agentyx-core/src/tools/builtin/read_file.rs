//! `read_file` — read a UTF-8 text file within the workspace sandbox.
//!
//! Per `specs/domains/tools.md` §Catalog:
//! - Dangerous: `false` (read-only).
//! - Args: `{ path: string, offset?: u32, limit?: u32 }`.
//! - Output: `{ content, total_lines, returned_lines }`.
//! - Errors: `not_found`, `path_outside_workspace`, `invalid_input`
//!   (file > 50 MB or non-UTF-8).

use std::path::Path;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::permissions::sandbox::resolve_workspace_path;
use crate::tools::types::{Tool, ToolContext, ToolId, ToolOutput};
use crate::AppError;

/// The `read_file` tool.
pub struct ReadFileTool;

#[derive(Debug, Deserialize)]
struct ReadFileArgs {
    path: String,
    #[serde(default)]
    offset: Option<u32>,
    #[serde(default)]
    limit: Option<u32>,
}

const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024; // 50 MB
const DEFAULT_LIMIT: u32 = 2000;
const MAX_LIMIT: u32 = 100_000;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> ToolId {
        "read_file"
    }

    fn is_dangerous(&self) -> bool {
        false
    }

    fn schema(&self) -> Value {
        json!({
            "name": "read_file",
            "description": "Read a UTF-8 text file from the workspace. Returns the file body (or a window of lines if `offset`/`limit` are given). Binary files and files > 50 MB are rejected.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file, absolute or relative to the workspace root."
                    },
                    "offset": {
                        "type": "integer",
                        "description": "0-indexed line number to start reading from (default: 0).",
                        "minimum": 0
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of lines to return (default: 2000, max: 100000).",
                        "minimum": 1,
                        "maximum": 100_000
                    }
                },
                "required": ["path"]
            }
        })
    }

    async fn run(&self, ctx: ToolContext, args: Value) -> Result<ToolOutput, AppError> {
        let parsed: ReadFileArgs =
            serde_json::from_value(args).map_err(|e| AppError::InvalidInput {
                message: format!("invalid read_file args: {e}"),
            })?;

        let resolved =
            match resolve_workspace_path(&ctx.workspace_root, &ctx.extra_paths, &parsed.path) {
                Ok(p) => p,
                Err(e) => {
                    return Ok(ToolOutput::failure(format!("{}: {}", e.code(), e)));
                }
            };

        read_within_sandbox(&resolved, parsed.offset, parsed.limit)
    }
}

/// Read a file, applying the workspace sandbox and the `offset`/
/// `limit` window. The caller is expected to have already
/// canonicalized the path via [`resolve_workspace_path`].
fn read_within_sandbox(
    path: &Path,
    offset: Option<u32>,
    limit: Option<u32>,
) -> Result<ToolOutput, AppError> {
    let metadata = std::fs::metadata(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => AppError::NotFound {
            kind: "file".into(),
            id: path.display().to_string(),
        },
        std::io::ErrorKind::PermissionDenied => AppError::Forbidden {
            action: format!("read {}", path.display()),
        },
        _ => AppError::Io {
            op: "stat".into(),
            reason: e.to_string(),
        },
    })?;

    if !metadata.is_file() {
        return Ok(ToolOutput::failure(format!(
            "not a regular file: {}",
            path.display()
        )));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Ok(ToolOutput::failure(format!(
            "file too large: {} bytes (max {})",
            metadata.len(),
            MAX_FILE_BYTES
        )));
    }

    let bytes = std::fs::read(path).map_err(|e| AppError::Io {
        op: "read".into(),
        reason: e.to_string(),
    })?;

    let text = match std::str::from_utf8(&bytes) {
        Ok(s) => s.to_string(),
        Err(e) => {
            return Ok(ToolOutput::failure(format!(
                "file is not valid UTF-8 at byte {}",
                e.valid_up_to()
            )));
        }
    };

    let total_lines = text.lines().count() as u32;
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let offset = offset.unwrap_or(0);

    let (content, returned_lines) = apply_window(&text, offset, limit);

    let mut out = ToolOutput::success(content);
    out.metadata = Some(json!({
        "totalLines": total_lines,
        "returnedLines": returned_lines,
        "offset": offset,
    }));
    Ok(out)
}

/// Apply an `offset` / `limit` window to a string. Returns the
/// truncated content and the number of lines returned.
#[must_use]
pub fn apply_window(text: &str, offset: u32, limit: u32) -> (String, u32) {
    if offset == 0 && limit == u32::MAX {
        return (text.to_string(), text.lines().count() as u32);
    }
    let mut out_lines: Vec<&str> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if i < offset as usize {
            continue;
        }
        if out_lines.len() >= limit as usize {
            break;
        }
        out_lines.push(line);
    }
    (out_lines.join("\n"), out_lines.len() as u32)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::ids::{RunId, SessionId, WorkspaceId};
    use crate::tools::ToolContext;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    fn ctx(root: std::path::PathBuf) -> ToolContext {
        ToolContext {
            workspace_id: WorkspaceId::new(),
            workspace_root: root,
            extra_paths: Arc::new(vec![]),
            run_id: RunId::new(),
            session_id: SessionId::new(),
            abort_flag: Arc::new(AtomicBool::new(false)),
            ignore_patterns: Arc::new(vec![]),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn read_file_returns_content() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::write(root.join("hello.txt"), "hello world").unwrap();
        let out = ReadFileTool
            .run(ctx(root.clone()), json!({"path": "hello.txt"}))
            .await
            .unwrap();
        assert_eq!(out.content, "hello world");
        assert!(!out.is_error);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn read_file_path_outside_workspace_fails() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let out = ReadFileTool
            .run(ctx(root), json!({"path": "/etc/passwd"}))
            .await
            .unwrap();
        assert!(out.is_error);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn read_file_offset_and_limit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::write(root.join("lines.txt"), "a\nb\nc\nd\ne\n").unwrap();
        let out = ReadFileTool
            .run(
                ctx(root),
                json!({"path": "lines.txt", "offset": 1, "limit": 2}),
            )
            .await
            .unwrap();
        assert_eq!(out.content, "b\nc");
        let meta = out.metadata.unwrap();
        assert_eq!(meta["returnedLines"], json!(2));
    }

    #[test]
    fn apply_window_no_truncation() {
        let (s, n) = apply_window("a\nb\nc", 0, u32::MAX);
        assert_eq!(s, "a\nb\nc");
        assert_eq!(n, 3);
    }

    #[test]
    fn apply_window_offset_past_end() {
        let (s, n) = apply_window("a\nb", 10, 5);
        assert_eq!(s, "");
        assert_eq!(n, 0);
    }
}
