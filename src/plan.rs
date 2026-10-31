//! Task management: model, query, lint, display, store, format, TUI.
//!
//! Entry points: [`store`] (CRUD + slug resolution), [`query`] (filter/sort),
//! [`lint`] (schema validation), [`display`] (list/kanban output),
//! [`format`] (canonical rewriting), [`tui`] (interactive browser).

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
