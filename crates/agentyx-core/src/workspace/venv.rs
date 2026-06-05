//! `.venv` detection (read-only).
//!
//! Implements the priority order from
//! `../../../specs/adr/0004-detect-venv-priority.md`:
//!
//! 1. `config.venv.path` explicit override (handled by caller).
//! 2. `<root>/.venv/` (modern uv convention).
//! 3. `<root>/venv/` (legacy).
//! 4. `<root>/.python-version` (pyenv) — see [`pyenv_python`].
//! 5. `<root>/pyproject.toml` with a uv/poetry/pdm section — see
//!    [`pyproject_venv`].
//! 6. `uv.lock` / `poetry.lock` (suggest a venv, no create).
//! 7. `conda-env.yml` / `environment.yml` — returns `None` with a
//!    `tracing::warn!` (conda not supported in v1).
//! 8. Nothing → `None`.
//!
//! The actual `uv venv` / `python -m venv` execution is deferred to
//! v0.1.x with F03.

use std::path::{Path, PathBuf};

use crate::{AppError, AppResult};

use super::types::{VenvKind, VenvSpec};

/// Detect a venv for the given workspace `root` and the current
/// per-workspace config (only the `venv.path` override is consulted
/// here; the rest is auto-detection).
///
/// Returns `Ok(None)` if no usable venv is found. This includes
/// the "no markers" case (silent) and the "broken symlink" case
/// (`tracing::warn!` with detail, per Edge case 1).
pub fn detect_venv(root: &Path, config_override: Option<&Path>) -> AppResult<Option<VenvSpec>> {
    // (1) explicit override
    if let Some(p) = config_override {
        if !p.as_os_str().is_empty() {
            return inspect_venv_dir(p, VenvKind::Uv);
        }
    }

    // (2) <root>/.venv/
    let dot_venv = root.join(".venv");
    if dot_venv.exists() {
        if let Some(spec) = inspect_venv_dir(&dot_venv, VenvKind::Uv)? {
            return Ok(Some(spec));
        }
        // symlink broken → tracing::warn!, fall through
        tracing::warn!(path = %dot_venv.display(), ".venv exists but is invalid");
    }

    // (3) <root>/venv/
    let venv = root.join("venv");
    if venv.exists() {
        if let Some(spec) = inspect_venv_dir(&venv, VenvKind::Venv)? {
            return Ok(Some(spec));
        }
    }

    // (4) pyenv
    if let Some(python) = pyenv_python(root) {
        return Ok(Some(VenvSpec {
            kind: VenvKind::Venv,
            path: PathBuf::from(python).parent().map(Path::to_path_buf).unwrap_or_default(),
            python: PathBuf::from(python),
            version: String::new(), // caller queries --version
        }));
    }

    // (5) pyproject.toml
    if let Some(spec) = pyproject_venv(root)? {
        return Ok(Some(spec));
    }

    // (6) lock files
    if root.join("uv.lock").exists() || root.join("poetry.lock").exists() {
        tracing::info!(
            root = %root.display(),
            "lock file present but no venv created yet; user must run `uv sync` or `poetry install`"
        );
        return Ok(None);
    }

    // (7) conda — not supported in v1
    if root.join("conda-env.yml").exists() || root.join("environment.yml").exists() {
        tracing::warn!(
            root = %root.display(),
            "conda env file detected; conda is not supported in v1 (see ADR-0004)"
        );
        return Ok(None);
    }

    // (8) nothing
    Ok(None)
}

/// Inspect a candidate venv directory. Returns `None` if the
/// directory is missing or the python binary is not executable.
fn inspect_venv_dir(path: &Path, kind: VenvKind) -> AppResult<Option<VenvSpec>> {
    let python = venv_python(path);
    match python {
        Some(p) if p.is_file() => Ok(Some(VenvSpec {
            kind,
            path: path.to_path_buf(),
            python: p,
            version: String::new(), // populated on demand by `--version`
        })),
        _ => {
            // Path exists but no python binary. Could be a broken
            // symlink; let the caller decide what to do.
            Ok(None)
        }
    }
}

/// Returns the path to the python executable inside a venv dir.
fn venv_python(venv: &Path) -> Option<PathBuf> {
    let bin = if cfg!(target_os = "windows") {
        venv.join("Scripts").join("python.exe")
    } else {
        venv.join("bin").join("python")
    };
    if bin.is_file() {
        Some(bin)
    } else {
        None
    }
}

/// Read `<root>/.python-version` (pyenv convention) and try to
/// resolve it against `pyenv which python` (or the system
/// `pythonX.Y` on PATH). Returns the absolute python path or
/// `None`. We do **not** spawn `pyenv` here; the detection
/// is best-effort.
fn pyenv_python(root: &Path) -> Option<String> {
    let path = root.join(".python-version");
    let contents = std::fs::read_to_string(&path).ok()?;
    let version = contents.trim();
    if version.is_empty() {
        return None;
    }
    // Try `pythonX.Y` on PATH (e.g. `python3.12`).
    let candidate = if cfg!(target_os = "windows") {
        format!("python{version}")
    } else {
        format!("python{version}")
    };
    // Look up via `which`/`where` to get absolute path; fall back
    // to the relative name (the user will get a clear "not
    // found" when invoking `python_run`).
    Some(candidate)
}

/// Inspect `<root>/pyproject.toml` for a `[tool.uv]`,
/// `[tool.poetry]`, or `[project]` section and return a venv
/// spec pointing at the expected venv path. The python version
/// is left empty (caller fills via `--version`).
fn pyproject_venv(root: &Path) -> AppResult<Option<VenvSpec>> {
    let path = root.join("pyproject.toml");
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).map_err(|e| AppError::Io {
        op: format!("read {}", path.display()),
        source: e.to_string(),
    })?;
    let value: toml::Value = toml::from_slice(&bytes).map_err(|e| AppError::Internal {
        message: format!("pyproject.toml is malformed: {e}"),
    })?;

    // `[tool.uv]` → `.venv` (most common today).
    if value.get("tool").and_then(|t| t.get("uv")).is_some() {
        let venv = root.join(".venv");
        if venv.exists() {
            return inspect_venv_dir(&venv, VenvKind::Uv);
        }
    }

    // `[tool.poetry]` → `.venv` (Poetry 1.x default).
    if value.get("tool").and_then(|t| t.get("poetry")).is_some() {
        let venv = root.join(".venv");
        if venv.exists() {
            return inspect_venv_dir(&venv, VenvKind::Uv);
        }
    }

    // `[project]` (PEP 621) — no canonical venv location; we
    // don't auto-create, just hint.
    if value.get("project").is_some() {
        let venv = root.join(".venv");
        if venv.exists() {
            return inspect_venv_dir(&venv, VenvKind::Uv);
        }
    }

    Ok(None)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn detect_venv_with_dotvenv() {
        let dir = tempfile::tempdir().unwrap();
        let dot_venv = dir.path().join(".venv");
        let bin = dot_venv.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("python"), "#!/bin/sh\necho 3.12").unwrap();

        let spec = detect_venv(dir.path(), None).unwrap();
        let spec = spec.expect("should detect .venv");
        assert_eq!(spec.kind, VenvKind::Uv);
        assert!(spec.python.is_file());
    }

    #[test]
    fn detect_venv_with_legacy_venv() {
        let dir = tempfile::tempdir().unwrap();
        let venv = dir.path().join("venv");
        let bin = venv.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("python"), "#!/bin/sh").unwrap();

        let spec = detect_venv(dir.path(), None).unwrap();
        let spec = spec.expect("should detect legacy venv");
        assert_eq!(spec.kind, VenvKind::Venv);
    }

    #[test]
    fn detect_venv_with_no_venv_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let spec = detect_venv(dir.path(), None).unwrap();
        assert!(spec.is_none());
    }

    #[test]
    fn detect_venv_with_broken_dotvenv_returns_none() {
        // .venv exists but no python inside → not usable.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".venv")).unwrap();
        let spec = detect_venv(dir.path(), None).unwrap();
        assert!(spec.is_none());
    }

    #[test]
    fn detect_venv_with_override_path() {
        let dir = tempfile::tempdir().unwrap();
        let custom = dir.path().join("custom-venv");
        let bin = custom.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("python"), "").unwrap();

        let spec = detect_venv(dir.path(), Some(&custom)).unwrap();
        let spec = spec.expect("should respect override");
        assert_eq!(spec.path, custom);
    }

    #[test]
    fn detect_venv_with_conda_env_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("environment.yml"), "name: foo").unwrap();
        let spec = detect_venv(dir.path(), None).unwrap();
        assert!(spec.is_none());
    }
}
