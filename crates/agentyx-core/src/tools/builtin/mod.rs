//! Built-in tool implementations.
//!
//! v0.1 ships the three read-only tools: `read_file`, `list_dir`,
//! `search`. Write/destructive tools (`write_file`, `edit_file`,
//! `apply_patch`, `shell`, `python_run`) are planned for v1.1; the
//! agent loop is already prepared to dispatch them once they
//! exist.

pub mod list_dir;
pub mod read_file;
pub mod search;

pub use list_dir::ListDirTool;
pub use read_file::ReadFileTool;
pub use search::SearchTool;
