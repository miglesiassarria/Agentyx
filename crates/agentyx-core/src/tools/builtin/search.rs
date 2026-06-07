//! `search` — search for a pattern within files in the workspace.
//!
//! Per `specs/domains/tools.md` §Catalog:
//! - Dangerous: `false`.
//! - Args: `{ query, path?, glob?, regex?, case_insensitive?, max_results? }`.
//! - Output: `{ matches: SearchMatch[], truncated: bool }`.
//! - Errors: `path_outside_workspace`, `invalid_input`
//!   (`query` empty or > 200 chars).

use std::path::Path;
use std::sync::atomic::Ordering;

use async_trait::async_trait;
use globset::Glob;
use regex::RegexBuilder;
use serde::Deserialize;
use serde_json::{json, Value};
use walkdir::WalkDir;

use crate::permissions::sandbox::resolve_workspace_path;
use crate::tools::types::{Tool, ToolContext, ToolId, ToolOutput};
use crate::AppError;

const DEFAULT_MAX_RESULTS: u32 = 100;
const MAX_MAX_RESULTS: u32 = 10_000;
const MAX_QUERY_CHARS: usize = 200;
const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024; // skip files > 10 MB

/// The `search` tool.
pub struct SearchTool;

#[derive(Debug, Deserialize)]
struct SearchArgs {
    query: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    glob: Option<String>,
    #[serde(default)]
    regex: Option<bool>,
    #[serde(default)]
    case_insensitive: Option<bool>,
    #[serde(default)]
    max_results: Option<u32>,
}

#[async_trait]
impl Tool for SearchTool {
    fn name(&self) -> ToolId {
        "search"
    }

    fn is_dangerous(&self) -> bool {
        false
    }

    fn schema(&self) -> Value {
        json!({
            "name": "search",
            "description": "Search for a literal or regex pattern within files in the workspace. Returns up to `max_results` matches (default 100, max 10000). Skips files > 10 MB and respects the workspace's ignore patterns.",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search pattern (literal or regex, up to 200 chars)."
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory to start the search from (default: workspace root)."
                    },
                    "glob": {
                        "type": "string",
                        "description": "Optional glob filter for file names (e.g. \"*.rs\")."
                    },
                    "regex": {
                        "type": "boolean",
                        "description": "Treat the query as a regex (default: false)."
                    },
                    "case_insensitive": {
                        "type": "boolean",
                        "description": "Case-insensitive matching (default: true)."
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum number of matches to return (default: 100, max: 10000).",
                        "minimum": 1,
                        "maximum": 10_000
                    }
                },
                "required": ["query"]
            }
        })
    }

    async fn run(&self, ctx: ToolContext, args: Value) -> Result<ToolOutput, AppError> {
        let parsed: SearchArgs =
            serde_json::from_value(args).map_err(|e| AppError::InvalidInput {
                message: format!("invalid search args: {e}"),
            })?;

        if parsed.query.is_empty() {
            return Ok(ToolOutput::failure("query cannot be empty"));
        }
        if parsed.query.chars().count() > MAX_QUERY_CHARS {
            return Ok(ToolOutput::failure(format!(
                "query too long: {} chars (max {})",
                parsed.query.chars().count(),
                MAX_QUERY_CHARS
            )));
        }

        let raw_path = parsed.path.as_deref().unwrap_or(".");
        let resolved = match resolve_workspace_path(&ctx.workspace_root, &ctx.extra_paths, raw_path)
        {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolOutput::failure(format!("{}: {}", e.code(), e)));
            }
        };

        let case_insensitive = parsed.case_insensitive.unwrap_or(true);
        let use_regex = parsed.regex.unwrap_or(false);
        let max_results = parsed
            .max_results
            .unwrap_or(DEFAULT_MAX_RESULTS)
            .min(MAX_MAX_RESULTS);

        let pattern = if use_regex {
            build_regex(&parsed.query, case_insensitive)?
        } else {
            None
        };

        let glob_matcher = match &parsed.glob {
            Some(g) => Some(Glob::new(g).map_err(|e| AppError::InvalidInput {
                message: format!("invalid glob '{g}': {e}"),
            })?),
            None => None,
        };
        let glob_matcher = glob_matcher.map(|g| g.compile_matcher());

        let ignore_globs: Vec<globset::GlobMatcher> = ctx
            .ignore_patterns
            .iter()
            .filter_map(|p| Glob::new(p).ok())
            .map(|g| g.compile_matcher())
            .collect();

        let query_lc = if !use_regex && case_insensitive {
            Some(parsed.query.to_lowercase())
        } else {
            None
        };

        let mut matches: Vec<SearchMatch> = Vec::new();
        let mut truncated = false;

        for entry in WalkDir::new(&resolved)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !is_ignored_entry(e, &resolved, &ignore_globs))
        {
            if ctx.abort_flag.load(Ordering::SeqCst) {
                return Ok(ToolOutput::failure("aborted"));
            }
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(g) = &glob_matcher {
                if !g.is_match(&name) {
                    continue;
                }
            }
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if metadata.len() > MAX_FILE_BYTES {
                continue;
            }
            let path = entry.path();
            let content = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(_) => continue, // skip non-UTF-8 / unreadable files silently
            };
            for (line_idx, line) in content.lines().enumerate() {
                let hit = if let Some(re) = &pattern {
                    re.is_match(line)
                } else if let Some(lc) = &query_lc {
                    line.to_lowercase().contains(lc)
                } else {
                    line.contains(&parsed.query)
                };
                if hit {
                    matches.push(SearchMatch {
                        file: path
                            .strip_prefix(&resolved)
                            .unwrap_or(path)
                            .display()
                            .to_string(),
                        line: (line_idx + 1) as u32,
                        column: 1,
                        text: line.to_string(),
                    });
                    if matches.len() >= max_results as usize {
                        truncated = true;
                        break;
                    }
                }
            }
            if truncated {
                break;
            }
        }

        let summary = if truncated {
            format!("{} matches (truncated)", matches.len())
        } else {
            format!("{} matches", matches.len())
        };

        let payload = json!({
            "matches": matches,
            "truncated": truncated,
        });

        let mut out = ToolOutput::success(payload.to_string());
        out.summary = summary;
        out.metadata = Some(json!({
            "count": matches.len(),
            "truncated": truncated,
            "maxResults": max_results,
        }));
        Ok(out)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct SearchMatch {
    file: String,
    line: u32,
    column: u32,
    text: String,
}

fn build_regex(query: &str, case_insensitive: bool) -> Result<Option<Regex>, AppError> {
    // Defensive: invalid regex → invalid_input (the model can retry).
    RegexBuilder::new(query)
        .case_insensitive(case_insensitive)
        .build()
        .map(Some)
        .map_err(|e| AppError::InvalidInput {
            message: format!("invalid regex: {e}"),
        })
}

// Re-export the regex type under a short name to keep signatures tidy.
type Regex = regex::Regex;

fn is_ignored_entry(
    entry: &walkdir::DirEntry,
    root: &Path,
    ignore: &[globset::GlobMatcher],
) -> bool {
    if ignore.is_empty() {
        return false;
    }
    let Ok(rel) = entry.path().strip_prefix(root) else {
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
    async fn search_literal() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::write(root.join("a.rs"), "hello world\nfoo bar\n").unwrap();
        let out = SearchTool
            .run(ctx(root.clone()), json!({"query": "foo"}))
            .await
            .unwrap();
        assert!(!out.is_error);
        assert!(out.content.contains("foo"));
        assert!(out.content.contains("a.rs"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn search_regex() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::write(root.join("a.rs"), "foo123\nbar\n").unwrap();
        let out = SearchTool
            .run(
                ctx(root.clone()),
                json!({"query": "foo\\d+", "regex": true}),
            )
            .await
            .unwrap();
        assert!(!out.is_error);
        assert!(out.content.contains("foo123"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn search_empty_query_fails() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let out = SearchTool
            .run(ctx(root.clone()), json!({"query": ""}))
            .await
            .unwrap();
        assert!(out.is_error);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn search_outside_workspace_fails() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let out = SearchTool
            .run(ctx(root.clone()), json!({"query": "x", "path": "/etc"}))
            .await
            .unwrap();
        assert!(out.is_error);
    }
}
