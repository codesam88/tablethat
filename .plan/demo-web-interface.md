---
status: idea
type: feature
priority: low
area: backend
---

## Web interface for plan/gloss

### Problem

The kanban TUI and gloss viewer are terminal-only. For teams or users
who prefer browsers, or for sharing boards with non-technical
stakeholders, a web UI would lower the barrier.

### Direction spectrum

| Approach           | Complexity | Interactivity      | Dependencies       |
| ------------------ | ---------- | ------------------ | ------------------ |
| Static HTML export | Low        | Read-only          | None               |
| Local HTTP server  | Medium     | Read-only + themes | axum or actix      |
| Interactive kanban | High       | Drag-and-drop      | Frontend framework |
| WASM (leptos/yew)  | High       | Full               | wasm-pack          |

### Static export (simplest)

Generate HTML from the same data `plan list --kanban` produces. Comrak
already has `format_html()` — map theme colors to CSS variables:

```rust
// Reuse existing pipeline
let root = comrak::parse_document(&arena, body, &options);
let mut html = String::new();
comrak::format_html(root, &options, &mut html)?;
// Wrap in themed HTML template
```

Serve with `plan export --html > board.html` or `plan serve --static`.

### Interactive kanban (heavier)

Needs a frontend (htmx, Alpine.js, or a SPA framework) and a backend
that can write to .plan/ files:

```
Browser  ──HTTP──►  plan serve  ──filesystem──►  .plan/*.md
         ◄─JSON──   (axum)      ◄─read────────
```

Operations: `GET /tasks`, `PATCH /tasks/:slug/status`, `POST /tasks`.

### Gloss as web viewer

The markdown rendering pipeline produces `Vec<Line<Span>>` for ratatui.
For web, we'd need a parallel path that produces HTML. The comrak
`format_html()` function handles this directly — theme colors become
CSS.

### Exploration needed

- Can we reuse the theme system for CSS? Map `ThemeConfig` fields to CSS custom properties: `--h1-color: #D06020;
  --code-color: #39FF14;`
- What's the MVP? Probably `plan serve` that opens a browser to
  `localhost:3000` showing a read-only kanban board. No auth, no writes.
- Should this be a separate binary or a subcommand? Subcommand is
  simpler for users — `plan serve` just works.
