---
status: idea
type: chore
priority: low
area: backend
---

## Upstream PR: table column alignment in comrak

We implemented a post-processing `align_tables()` workaround in
`src/markdown.rs` because comrak's `markdown_to_commonmark()` doesn't
pad table columns to align them.

This should be contributed upstream to comrak as a proper feature — either as a render option (`table_column_padding:
bool`) or as default behavior when outputting CommonMark tables.

Upstream location: <https://github.com/kivikakk/comrak> Relevant file:
`src/cm.rs` — `format_table_cell()` function
