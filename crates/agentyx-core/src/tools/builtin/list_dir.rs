//! `list_dir` — list entries in a directory within the workspace sandbox.
//!
//! Per `specs/domains/tools.md` §Catalog:
//! - Dangerous: `false`.
//! - Args: `{ path?: string, depth?: u32, include_hidden?: bool }`.
//! - Output: `{ entries: DirEntry[] }`.
//! - Errors: `path_outside_workspace`, `not_found`.

use std::path::Path;

use async_trait::async_trait;
use globset::Glob;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::permissions::sandbox::resolve_workspace_path;
use crate::tools::types::{Tool, ToolContext, ToolId, ToolOutput};
use crate::AppError;

const DEFAULT_DEPTH: u32 = 1;
const MAX_DEPTH: u32 = 5;

/// The `list_dir` tool.
pub struct ListDirTool;

#[derive(Debug, Deserialize)]
struct ListDirArgs {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    depth: Option<u32>,
    #[serde(default)]
    include_hidden: Option<bool>,
}

#[async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> ToolId {
        "list_dir"
    }

    fn is_dangerous(&self) -> bool {
        false
    }

    fn schema(&self) -> Value {
        json!({
            "name": "list_dir",
            "description": "List the entries of a directory within the workspace. Recurses up to `depth` levels (default 1, max 5). Skips hidden files (starting with `.`) unless `include_hidden` is true.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the directory (default: workspace root)."
                    },
                    "depth": {
                        "type": "integer",
                        "description": "Maximum depth to recurse (default: 1, max: 5).",
                        "minimum": 1,
                        "maximum": 5
                    },
                    "include_hidden": {
                        "type": "boolean",
                        "description": "Include dotfiles and dot-directories (default: false)."
                    }
                }
            }
        })
    }

    async fn run(&self, ctx: ToolContext, args: Value) -> Result<ToolOutput, AppError> {
        let parsed: ListDirArgs =
            serde_json::from_value(args).map_err(|e| AppError::InvalidInput {
                message: format!("invalid list_dir args: {e}"),
            })?;

        let raw = parsed.path.as_deref().unwrap_or(".");
        let resolved = match resolve_workspace_path(&ctx.workspace_root, &ctx.extra_paths, raw) {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolOutput::failure(format!("{}: {}", e.code(), e)));
            }
        };
        let depth = parsed.depth.unwrap_or(DEFAULT_DEPTH).min(MAX_DEPTH);
        let include_hidden = parsed.include_hidden.unwrap_or(false);

        let ignore_globs = compile_ignore(&ctx.ignore_patterns)?;

        let entries = list_recursive(&resolved, depth, include_hidden, &ignore_globs)?;

        let mut out = ToolOutput::success(json!(entries).to_string());
        out.metadata = Some(json!({
            "root": resolved.display().to_string(),
            "count": entries.len(),
            "depth": depth,
        }));
        Ok(out)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct DirEntry {
    name: String,
    kind: String, // "file" | "dir" | "symlink"
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    children: Option<Vec<DirEntry>>,
}

fn list_recursive(
    root: &Path,
    depth: u32,
    include_hidden: bool,
    ignore: &[globset::GlobMatcher],
) -> Result<Vec<DirEntry>, AppError> {
    let read_dir = std::fs::read_dir(root).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => AppError::NotFound {
            kind: "directory".into(),
            id: root.display().to_string(),
        },
        _ => AppError::Io {
            op: "read_dir".into(),
            reason: e.to_string(),
        },
    })?;

    let mut entries: Vec<DirEntry> = Vec::new();
    let mut dirs: Vec<(u32, std::path::PathBuf, String)> = Vec::new();

    for entry in read_dir {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                return Err(AppError::Io {
                    op: "read_dir entry".into(),
                    reason: e.to_string(),
                });
            }
        };
        let name = entry.file_name().to_string_lossy().to_string();
        if !include_hidden && name.starts_with('.') {
            continue;
        }
        let rel = entry.path();
        if is_ignored(&rel, root, ignore) {
            continue;
        }
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(e) => {
                return Err(AppError::Io {
                    op: "file_type".into(),
                    reason: e.to_string(),
                });
            }
        };
        let kind = if file_type.is_symlink() {
            "symlink"
        } else if file_type.is_dir() {
            "dir"
        } else {
            "file"
        };
        let size = if kind == "file" {
            entry.metadata().ok().map(|m| m.len())
        } else {
            None
        };
        let entry_struct = DirEntry {
            name: name.clone(),
            kind: kind.to_string(),
            size,
            children: None,
        };
        if kind == "dir" {
            dirs.push((depth, rel, name));
        }
        entries.push(entry_struct);
    }

    // Recurse into subdirs.
    if depth > 1 {
        for (d, path, _name) in dirs {
            if let Some(idx) = entries.iter().position(|e| e.name == _name) {
                let children = list_recursive(&path, d - 1, include_hidden, ignore)?;
                if !children.is_empty() {
                    entries[idx].children = Some(children);
                }
            }
        }
    }

    // Sort: dirs first, then files, then symlinks, alphabetical.
    entries.sort_by(|a, b| match (a.kind.as_str(), b.kind.as_str()) {
        ("dir", "file") | ("dir", "symlink") => std::cmp::Ordering::Less,
        ("file", "dir") | ("symlink", "dir") => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });

    Ok(entries)
}

fn compile_ignore(patterns: &[String]) -> Result<Vec<globset::GlobMatcher>, AppError> {
    patterns
        .iter()
        .filter(|p| !p.trim().is_empty())
        .map(|p| {
            Glob::new(p)
                .map(|g| g.compile_matcher())
                .map_err(|e| AppError::InvalidInput {
                    message: format!("invalid ignore glob '{p}': {e}"),
                })
        })
        .collect()
}

fn is_ignored(path: &Path, root: &Path, ignore: &[globset::GlobMatcher]) -> bool {
    if ignore.is_empty() {
        return false;
    }
    let Ok(rel) = path.strip_prefix(root) else {
        return false;
    };
    for m in ignore {
        if m.is_match(rel) {
            return true;
        }
    }
    false
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
    async fn list_dir_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::write(root.join("a.rs"), "x").unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        let out = ListDirTool.run(ctx(root.clone()), json!({})).await.unwrap();
        assert!(!out.is_error);
        assert!(out.content.contains("a.rs"));
        assert!(out.content.contains("sub"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn list_dir_excludes_hidden_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("a.rs"), "x").unwrap();
        let out = ListDirTool.run(ctx(root.clone()), json!({})).await.unwrap();
        assert!(out.content.contains("a.rs"));
        assert!(!out.content.contains(".git"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn list_dir_path_outside_workspace_fails() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let out = ListDirTool
            .run(ctx(root), json!({"path": "/etc"}))
            .await
            .unwrap();
        assert!(out.is_error);
    }
}
