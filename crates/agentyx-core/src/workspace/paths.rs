//! Path security primitives — canonicalization, sandbox checks,
//! and the root whitelist.

use std::path::{Component, Path, PathBuf};

use crate::AppError;

/// Returns the canonical absolute path of `path`.
///
/// Uses `std::fs::canonicalize`, which resolves symlinks and
/// `..` components. On platforms where the path doesn't exist,
/// returns `AppError::NotFound` (with the original path as detail).
/// On I/O errors, returns `AppError::Io`.
///
/// Trailing separators are stripped; on Windows, the result uses
/// the canonical prefix (`\\?\`).
pub fn canonicalize(path: &Path) -> Result<PathBuf, AppError> {
    match std::fs::canonicalize(path) {
        Ok(p) => Ok(p),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(AppError::NotFound {
            kind: "path".into(),
            id: path.display().to_string(),
        }),
        Err(e) => Err(AppError::Io {
            op: format!("canonicalize({})", path.display()),
            reason: e.to_string(),
        }),
    }
}

/// Returns true if `path` is a child of (or equal to) `root`.
///
/// Both arguments should be canonical (see [`canonicalize`]).
/// This is a pure path comparison; no filesystem I/O.
///
/// `path == root` → `true` (a file in the root is "within" the root).
/// A `path` that is a sibling of `root` is **not** within it.
#[must_use]
pub fn is_within(path: &Path, root: &Path) -> bool {
    let path = normalize(path);
    let root = normalize(root);

    if path == root {
        return true;
    }

    path.starts_with(root)
}

/// Returns true if `path` is within `root_path` or any of the
/// `extra_paths`. See ADR-0007. Both arguments should be canonical.
///
/// `path == root_path` → `true`.
/// `path == any extra_path` → `true`.
/// Otherwise → `false`.
#[must_use]
pub fn is_within_sandbox(path: &Path, root_path: &Path, extra_paths: &[PathBuf]) -> bool {
    if is_within(path, root_path) {
        return true;
    }
    for ep in extra_paths {
        if is_within(path, ep) {
            return true;
        }
    }
    false
}

/// Strips `.` components and normalizes trailing slashes.
/// Does **not** resolve `..` (use `canonicalize` for that).
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {} // skip "."
            Component::ParentDir => {
                // Pop the last component; if none, keep the "..".
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The hardcoded whitelist of roots that are acceptable as
/// `root_path` of a workspace (or as an `extra_path`).
///
/// Per workspace.md §Open questions Q1:
/// - `~` (the user's home directory, expanded)
/// - `/Users` (macOS user homes)
/// - `/home` (Linux user homes)
/// - `C:\Users` (Windows user homes)
/// - `C:\Projets`, `C:\Code`, `C:\Source`, `C:\Proyectos`
///   (typical project roots; configurable globally in v2)
///
/// Each entry is a canonical absolute path **prefix** (directory).
/// A workspace `root_path` is accepted if it starts with any of
/// these prefixes.
#[must_use]
pub fn root_whitelist() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(home) = dirs::home_dir() {
        if let Ok(home) = canonicalize(&home) {
            roots.push(home);
        }
    }
    if cfg!(target_os = "macos") {
        roots.push(PathBuf::from("/Users"));
    }
    if cfg!(target_os = "linux") {
        roots.push(PathBuf::from("/home"));
    }
    if cfg!(target_os = "windows") {
        roots.push(PathBuf::from(r"C:\Users"));
        for r in [r"C:\Projets", r"C:\Code", r"C:\Source", r"C:\Proyectos"] {
            roots.push(PathBuf::from(r));
        }
    }

    roots
}

/// Returns true if `path` is within any of the [`root_whitelist`]
/// entries. Both sides are normalized first; `path` should be
/// canonical (typically the result of [`canonicalize`]).
///
/// If the whitelist is empty (e.g. home dir is unset on a
/// Unix-like system), **all** paths are rejected — fail-closed.
#[must_use]
pub fn is_in_whitelisted_root(path: &Path) -> bool {
    let path = normalize(path);
    let whitelist = root_whitelist();
    if whitelist.is_empty() {
        return false;
    }
    for root in &whitelist {
        if is_within(&path, root) {
            return true;
        }
    }
    false
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn is_within_identical_paths() {
        let p = Path::new("/a/b");
        assert!(is_within(p, p));
    }

    #[test]
    fn is_within_child() {
        assert!(is_within(Path::new("/a/b/c"), Path::new("/a/b")));
        assert!(is_within(Path::new("/a/b/c/d.txt"), Path::new("/a/b")));
    }

    #[test]
    fn is_within_sibling_is_false() {
        assert!(!is_within(Path::new("/a/c"), Path::new("/a/b")));
        assert!(!is_within(Path::new("/a"), Path::new("/a/b")));
    }

    #[test]
    fn is_within_does_not_match_prefix_components() {
        // /a/bbx is NOT within /a/b (a common bug).
        assert!(!is_within(Path::new("/a/bbx"), Path::new("/a/b")));
    }

    #[test]
    fn is_within_sandbox_finds_extra() {
        let root = PathBuf::from("/a");
        let extras = vec![PathBuf::from("/x"), PathBuf::from("/y")];
        assert!(is_within_sandbox(Path::new("/a"), &root, &extras));
        assert!(is_within_sandbox(Path::new("/a/c"), &root, &extras));
        assert!(is_within_sandbox(Path::new("/x"), &root, &extras));
        assert!(is_within_sandbox(Path::new("/y/d"), &root, &extras));
        assert!(!is_within_sandbox(Path::new("/z"), &root, &extras));
    }

    #[test]
    fn is_within_handles_trailing_separator() {
        assert!(is_within(Path::new("/a/b/"), Path::new("/a/b")));
        assert!(is_within(Path::new("/a/b"), Path::new("/a/b/")));
    }

    #[test]
    fn canonicalize_nonexistent_returns_not_found() {
        let result = canonicalize(Path::new("/this/does/not/exist/at/all"));
        assert!(matches!(result, Err(AppError::NotFound { .. })));
    }
}
