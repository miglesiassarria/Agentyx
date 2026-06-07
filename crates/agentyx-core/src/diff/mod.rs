//! Diff computation for tool calls that modify files.
//!
//! F04 (`specs/features/F04-file-diffs.md`) defines the contract
//! for the `diff` field on `chat.tool_call.v1`. This module owns:
//!
//! - [`DiffPayload`] — the DTO emitted with the event and
//!   persisted in `state.db`.
//! - [`DiffKind`] — `edit_file | apply_patch | write_file`.
//! - [`compute`] — produces a `DiffPayload` for a tool call by
//!   reading the file from disk and running the same edit the
//!   tool would have applied.
//! - [`detect_binary`] — nul-byte heuristic for binary detection.
//! - [`is_image_path`] — extension-based image detection.
//!
//! The diff is **post-mortem**: the file has already been written
//! to disk by the time we read it. The agent loop calls
//! [`compute`] after the tool returns; the resulting payload is
//! what the UI renders.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Truncation threshold for `before` and `after` in bytes.
/// Anything larger gets `*Truncated = true` and the renderer
/// shows a "View full" affordance. 8 KiB matches the value in
/// `specs/features/F04-file-diffs.md` §Scope.
pub const DIFF_PAYLOAD_LIMIT: usize = 8 * 1024;

/// Kinds of diffs the UI can render. Maps 1:1 to the tool name
/// that produced the change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
    /// `edit_file(path, old_text, new_text)` — hunks-style diff.
    EditFile,
    /// `apply_patch(diff_unified)` — unified diff hunks.
    ApplyPatch,
    /// `write_file(path, content)` — full-file diff (often
    /// "all insertions" when the file is new).
    WriteFile,
}

impl DiffKind {
    /// String representation matching the tool name.
    #[must_use]
    pub fn as_tool_name(self) -> &'static str {
        match self {
            Self::EditFile => "edit_file",
            Self::ApplyPatch => "apply_patch",
            Self::WriteFile => "write_file",
        }
    }
}

/// Diff payload attached to `chat.tool_call.v1` events for
/// diffable tool calls. The renderer (Svelte) consumes this
/// directly to draw the CodeMirror Merge view.
///
/// The fields are intentionally flat: TS sees a single object,
/// not a discriminated union, to keep the wire shape simple.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffPayload {
    /// What kind of tool produced this diff.
    pub kind: DiffKind,
    /// The pre-change content. `None` for `write_file` on a brand
    /// new file. Truncated to [`DIFF_PAYLOAD_LIMIT`] if the
    /// original was larger; see [`Self::before_truncated`].
    pub before: Option<String>,
    /// The post-change content. Truncated to
    /// [`DIFF_PAYLOAD_LIMIT`] if the file is larger; see
    /// [`Self::after_truncated`].
    pub after: String,
    /// `true` if `before` was truncated to fit in the payload.
    pub before_truncated: bool,
    /// `true` if `after` was truncated to fit in the payload.
    pub after_truncated: bool,
    /// `true` if the file appears to be binary (nul byte in
    /// the first 8 KiB). The UI shows `BinaryDiffNotice` instead
    /// of a textual diff when this is set.
    pub is_binary: bool,
    /// MIME type if detected. `None` for text files. The UI
    /// uses this to render the icon and (in v0.2) the
    /// `ImageDiffNotice` thumbnail.
    pub mime: Option<String>,
    /// Approximate number of added lines.
    pub additions: u32,
    /// Approximate number of removed lines.
    pub deletions: u32,
}

/// Returned by [`compute`] when the diff is not available
/// (e.g. file missing, binary, malformed patch). The caller
/// emits a `chat.tool_call.v1` with `diff = null` in this case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffError {
    /// File does not exist on disk (post-mortem read failed).
    FileMissing,
    /// File is binary and the kind requested a textual diff.
    Binary,
    /// Patch was malformed (hunks could not be applied).
    MalformedPatch,
    /// I/O error.
    Io,
}

impl std::fmt::Display for DiffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileMissing => write!(f, "file missing"),
            Self::Binary => write!(f, "binary file"),
            Self::MalformedPatch => write!(f, "malformed patch"),
            Self::Io => write!(f, "i/o error"),
        }
    }
}

impl std::error::Error for DiffError {}

/// Read up to 8 KiB of `path` and return whether it contains a
/// nul byte. Used by the renderer to decide between
/// `DiffBody` (textual) and `BinaryDiffNotice` (binary).
///
/// We deliberately only sample the first 8 KiB to keep the
/// detection O(1) on file size. False positives (text files
/// with a stray nul in the first 8 KiB) are rare and the UI
/// shows a "View as text" affordance to recover.
#[must_use]
pub fn detect_binary(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    bytes.iter().take(DIFF_PAYLOAD_LIMIT).any(|b| *b == 0)
}

/// Guess MIME type from extension. Returns `None` for
/// unknown extensions. The list is intentionally short —
/// covers the formats F04 cares about (binary / image) and
/// defers everything else to the renderer.
#[must_use]
pub fn mime_for(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        "pdf" => Some("application/pdf"),
        "zip" => Some("application/zip"),
        "gz" => Some("application/gzip"),
        "tar" => Some("application/x-tar"),
        "wasm" => Some("application/wasm"),
        _ => None,
    }
}

/// Returns `true` for paths whose MIME type is a raster image.
/// Used by the UI to render `ImageDiffNotice` (thumbnail)
/// instead of a textual diff.
#[must_use]
pub fn is_image_path(path: &Path) -> bool {
    matches!(
        mime_for(path),
        Some("image/png") | Some("image/jpeg") | Some("image/gif") | Some("image/webp")
    )
}

/// Count added/removed lines by splitting on `\n`. Used as a
/// quick metric for the diff header (`[−12 +45]`).
#[must_use]
pub fn line_diff_counts(before: Option<&str>, after: &str) -> (u32, u32) {
    let additions = after.lines().count() as u32;
    let deletions = before.map_or(0, |b| b.lines().count() as u32);
    (additions, deletions)
}

/// Truncate `s` to [`DIFF_PAYLOAD_LIMIT`] bytes, returning the
/// possibly-shortened string and whether truncation happened.
///
/// Truncation is byte-based (not char-based) because the
/// downstream consumer (CodeMirror) treats input as bytes too.
/// UTF-8 boundary violations are extremely unlikely at 8 KiB
/// but not impossible for non-ASCII text.
#[must_use]
pub fn truncate(s: &str) -> (String, bool) {
    if s.len() <= DIFF_PAYLOAD_LIMIT {
        return (s.to_string(), false);
    }
    let mut cut = DIFF_PAYLOAD_LIMIT;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    (s[..cut].to_string(), true)
}

/// Build a `DiffPayload` for a `write_file` tool call.
/// `before_content` is what was on disk before the write;
/// `None` means the file was new.
#[must_use]
pub fn write_file_payload(
    path: &Path,
    before_content: Option<String>,
    after_content: String,
) -> DiffPayload {
    let is_binary = detect_binary(path) || detect_binary_in_str(&after_content);
    let mime = mime_for(path).map(str::to_string);
    let (before, before_truncated) = match before_content {
        Some(b) => {
            let (t, truncated) = truncate(&b);
            (Some(t), truncated)
        }
        None => (None, false),
    };
    let (after, after_truncated) = truncate(&after_content);
    let (additions, deletions) = line_diff_counts(before.as_deref(), &after);
    DiffPayload {
        kind: DiffKind::WriteFile,
        before,
        after,
        before_truncated,
        after_truncated,
        is_binary,
        mime,
        additions,
        deletions,
    }
}

/// Check if `s` contains a nul byte in the first
/// [`DIFF_PAYLOAD_LIMIT`] bytes. Used when the file content
/// is provided directly (not via path).
fn detect_binary_in_str(s: &str) -> bool {
    s.as_bytes()
        .iter()
        .take(DIFF_PAYLOAD_LIMIT)
        .any(|b| *b == 0)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn detects_binary_via_nul() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path().join("bin");
        std::fs::write(&p, b"hello\x00world").expect("write");
        assert!(detect_binary(&p));
    }

    #[test]
    fn does_not_flag_utf8() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path().join("utf8");
        std::fs::write(&p, "héllo, 世界 🌍".as_bytes()).expect("write");
        assert!(!detect_binary(&p));
    }

    #[test]
    fn mime_for_known_extensions() {
        assert_eq!(mime_for(Path::new("a.png")), Some("image/png"));
        assert_eq!(mime_for(Path::new("a.JPG")), Some("image/jpeg"));
        assert_eq!(mime_for(Path::new("a.zip")), Some("application/zip"));
        assert_eq!(mime_for(Path::new("a.rs")), None);
    }

    #[test]
    fn is_image_path_matches_known_image_exts() {
        assert!(is_image_path(Path::new("foo.png")));
        assert!(is_image_path(Path::new("foo.jpg")));
        assert!(!is_image_path(Path::new("foo.pdf")));
        assert!(!is_image_path(Path::new("foo.rs")));
    }

    #[test]
    fn line_diff_counts_basic() {
        let (a, d) = line_diff_counts(Some("a\nb\nc"), "x\ny");
        assert_eq!(a, 2);
        assert_eq!(d, 3);
    }

    #[test]
    fn truncate_under_limit() {
        let s = "short";
        let (out, truncated) = truncate(s);
        assert_eq!(out, s);
        assert!(!truncated);
    }

    #[test]
    fn truncate_over_limit() {
        let s = "x".repeat(DIFF_PAYLOAD_LIMIT + 100);
        let (out, truncated) = truncate(&s);
        assert!(truncated);
        assert_eq!(out.len(), DIFF_PAYLOAD_LIMIT);
    }

    #[test]
    fn truncate_respects_char_boundary() {
        // Build a string with a 2-byte UTF-8 char crossing the
        // boundary at position 8 KiB.
        let pad = "a".repeat(DIFF_PAYLOAD_LIMIT - 1);
        let s = format!("{pad}é");
        let (out, truncated) = truncate(&s);
        assert!(truncated);
        // The cut must not split the 'é' (2 bytes).
        assert!(out.ends_with('a'));
    }

    #[test]
    fn write_file_payload_new_file() {
        let p = Path::new("/tmp/foo.txt");
        let d = write_file_payload(p, None, "hello\nworld".into());
        assert_eq!(d.kind, DiffKind::WriteFile);
        assert!(d.before.is_none());
        assert_eq!(d.after, "hello\nworld");
        assert!(!d.is_binary);
        assert_eq!(d.additions, 2);
        assert_eq!(d.deletions, 0);
    }

    #[test]
    fn write_file_payload_existing_file() {
        let p = Path::new("/tmp/foo.txt");
        let d = write_file_payload(p, Some("a\nb\nc".into()), "x\ny".into());
        assert_eq!(d.additions, 2);
        assert_eq!(d.deletions, 3);
    }
}
