//! Task management: model, query, lint, display, store, format, TUI.
//!
//! Entry points: [`crate::plan::store`] (CRUD + slug resolution),
//! [`crate::plan::query`] (filter/sort), [`crate::plan::lint`] (schema
//! validation), [`crate::plan::display`] (list/kanban output),
//! [`crate::plan::format`] (canonical rewriting),
//! [`crate::plan::tui`] (interactive browser).

/// List/kanban terminal output.
pub mod display;
/// Canonical frontmatter + body rewriting.
pub mod format;
/// Schema validation with returned diagnostics.
pub mod lint;
/// Task frontmatter model and parsing.
pub mod model;
/// Loading, filtering, and sorting.
pub mod query;
/// Task files: discovery, slugs, CRUD.
pub mod store;
/// Interactive kanban browser.
pub mod tui;
