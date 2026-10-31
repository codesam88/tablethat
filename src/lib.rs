//! tablethat: task management (`plan`) and markdown viewing (`gloss`).
//!
//! Layout: `config` (layered config), `theme` (TOML themes), `color`
//! (ratatui/termcolor mapping), `markdown` (render engine),
//! `markdown_fmt` (CommonMark rewriting), `launch` (editor/gloss
//! spawning), `plan` (task model/query/lint/display/store/format/TUI),
//! `gloss` (stdin filter + file viewer). Bins are parse + dispatch glue.
#![warn(missing_docs)]

/// Ratatui/termcolor mapping and kanban palette lookup.
pub mod color;
/// Layered configuration and workspace lookup.
pub mod config;
/// Markdown viewer (stdin filter + file/directory TUI).
pub mod gloss;
/// Editor/gloss process spawning (no-shell invariant).
pub mod launch;
/// comrak-backed markdown renderer (ratatui output).
pub mod markdown;
/// CommonMark rewriting (formatter + table alignment).
pub mod markdown_fmt;
/// Task management (model/query/lint/display/store/format/TUI).
pub mod plan;
/// TOML theme discovery and loading.
pub mod theme;

pub use color::*;
pub use config::*;
pub use theme::*;

/// Task directory name inside a workspace root.
pub const PLAN_DIR: &str = ".plan";
