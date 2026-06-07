//! Path sandboxing — the shared helper that tools and the
//! permission gate use to decide whether a `path` argument is
//! inside the workspace's `root_path ∪ extra_paths`.
//!
//! Per ADR-0007, the sandbox is the **union** of the workspace
//! root and the user-declared extra paths. The agent cannot
//! touch anything outside that union, regardless of what
//! `permissions.allow` says.
//!
//! Canonicalization (`std::fs::canonicalize`) is mandatory. It
//! resolves symlinks, `..` segments and `.` segments; if the
//! resolved path is not inside the sandbox, the access is
//! rejected. Non-existent paths are canonicalized against the
//! parent and the relative remainder is appended (so `new_file`
//! under the workspace can be checked even before it exists).

use std::path::{Component, Path, PathBuf};

use crate::AppError;

/// Canonicalize a path that may not exist yet.
///
/// On Unix, `canonicalize` requires the path to exist; we work
/// around that by canonicalizing the deepest existing ancestor
/// and appending the relative remainder. The result is what
/// `canonicalize` would return **if** the path existed, with all
/// symlinks resolved.
pub fn canonicalize_any(path: &Path) -> std::io::Result<PathBuf> {
    if path.exists() {
        return path.canonicalize();
    }
    let mut ancestor = path.to_path_buf();
    let mut tail: Vec<PathBuf> = Vec::new();
    loop {
        if !ancestor.exists() {
            match ancestor.parent() {
                Some(p) => {
                    tail.push(
                        ancestor
                            .file_name()
                            .map_or_else(PathBuf::new, PathBuf::from),
                    );
                    ancestor = p.to_path_buf();
                }
                None => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("no existing ancestor for {}", path.display()),
                    ));
                }
            }
        } else {
            let canon = ancestor.canonicalize()?;
            // Re-append the missing tail in canonical form.
            let mut result = canon;
            for piece in tail.iter().rev() {
                if !piece.as_os_str().is_empty() {
                    result.push(piece);
                }
            }
            return Ok(result);
        }
    }
}

/// Verify that `path` is inside the workspace sandbox
/// (`workspace_root ∪ extra_paths`).
///
/// Steps:
/// 1. Reject any literal `..` or absolute path that escapes the
///    sandbox **before** canonicalization (defense in depth).
/// 2. Treat `~` literally — we do **not** expand it.
/// 3. Resolve relative paths against `workspace_root`.
/// 4. Canonicalize.
/// 5. Check membership in the union of the root and the extras.
///
/// Errors:
/// - `path_traversal` (we don't currently have a dedicated
///   variant; this surfaces as `PathOutsideWorkspace` so the UI
///   can map it consistently).
/// - `PathOutsideWorkspace` if the resolved path is not inside
///   the union.
pub fn resolve_workspace_path(
    workspace_root: &Path,
    extra_paths: &[PathBuf],
    raw: &str,
) -> Result<PathBuf, AppError> {
    if raw.is_empty() {
        return Err(AppError::InvalidInput {
            message: "path cannot be empty".into(),
        });
    }
    // Step 1: reject `..` literal segments in the **raw** input.
    for comp in Path::new(raw).components() {
        if matches!(comp, Component::ParentDir) {
            return Err(AppError::PathOutsideWorkspace {
                path: raw.to_string(),
            });
        }
    }

    // Step 2-3: join with workspace_root if relative.
    let candidate = Path::new(raw);
    let absolute = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        workspace_root.join(candidate)
    };

    // Step 4: canonicalize (or canonicalize-any for non-existent).
    let resolved = canonicalize_any(&absolute).map_err(|_e| AppError::PathOutsideWorkspace {
        path: raw.to_string(),
    })?;

    // Step 5: membership in root ∪ extras.
    if is_within(&resolved, workspace_root) {
        return Ok(resolved);
    }
    for extra in extra_paths {
        if is_within(&resolved, extra) {
            return Ok(resolved);
        }
    }
    Err(AppError::PathOutsideWorkspace {
        path: raw.to_string(),
    })
}

/// Canonicalize `raw` against `workspace_root`, returning a
/// canonical absolute path or an `PathOutsideWorkspace` error.
/// Unlike [`resolve_workspace_path`], this does **not** check the
/// extra paths — only the root. Used by code paths that only
/// want a canonical absolute path (e.g. the `apply_patch` parser
/// will do its own membership check).
pub fn canonicalize_in_workspace(workspace_root: &Path, raw: &str) -> Result<PathBuf, AppError> {
    if raw.is_empty() {
        return Err(AppError::InvalidInput {
            message: "path cannot be empty".into(),
        });
    }
    let candidate = Path::new(raw);
    let absolute = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        workspace_root.join(candidate)
    };
    canonicalize_any(&absolute).map_err(|_| AppError::PathOutsideWorkspace {
        path: raw.to_string(),
    })
}

/// Test if `path` is inside (or equal to) `base`. Both must be
/// canonical (i.e. not contain `.` or `..` segments).
#[must_use]
pub fn is_within(path: &Path, base: &Path) -> bool {
    if path == base {
        return true;
    }
    path.starts_with(base)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn rejects_parent_dir_segment() {
        let d = tmp();
        let root = d.path().to_path_buf();
        let extras: Vec<PathBuf> = vec![];
        let err = resolve_workspace_path(&root, &extras, "../foo").unwrap_err();
        assert!(matches!(err, AppError::PathOutsideWorkspace { .. }));
    }

    #[test]
    fn rejects_traversal_inside_relative() {
        let d = tmp();
        let root = d.path().to_path_buf();
        let extras: Vec<PathBuf> = vec![];
        let err = resolve_workspace_path(&root, &extras, "a/../../b").unwrap_err();
        assert!(matches!(err, AppError::PathOutsideWorkspace { .. }));
    }

    #[test]
    fn accepts_path_inside_root() {
        let d = tmp();
        let root = d.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::write(root.join("a/b/file.txt"), "x").unwrap();
        let p = resolve_workspace_path(&root, &[], "a/b/file.txt").unwrap();
        assert!(p.ends_with("a/b/file.txt"));
    }

    #[test]
    fn rejects_absolute_path_outside_root() {
        let d = tmp();
        let root = d.path().canonicalize().unwrap();
        let extras: Vec<PathBuf> = vec![];
        let err = resolve_workspace_path(&root, &extras, "/etc/passwd").unwrap_err();
        assert!(matches!(err, AppError::PathOutsideWorkspace { .. }));
    }

    #[test]
    fn accepts_path_inside_extra() {
        let d = tmp();
        let root = d.path().join("proj");
        let extra_dir = d.path().join("assets");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&extra_dir).unwrap();
        std::fs::write(extra_dir.join("foo.png"), "x").unwrap();
        let p = resolve_workspace_path(
            &root.canonicalize().unwrap(),
            std::slice::from_ref(&extra_dir.canonicalize().unwrap()),
            extra_dir.join("foo.png").to_str().unwrap(),
        )
        .unwrap();
        assert!(p.ends_with("foo.png"));
    }

    #[test]
    fn canonicalize_any_handles_nonexistent() {
        let d = tmp();
        let root = d.path().to_path_buf();
        std::fs::create_dir_all(&root).unwrap();
        let p = canonicalize_any(&root.join("does/not/exist/yet")).unwrap();
        assert!(p.starts_with(root.canonicalize().unwrap()));
    }

    #[test]
    fn is_within_basic() {
        let d = tmp();
        let root = d.path().to_path_buf();
        let sub = root.join("a");
        let deep = sub.join("b");
        assert!(is_within(&sub, &root));
        assert!(is_within(&deep, &root));
        assert!(!is_within(&root, &sub));
    }
}
