Golden markdown vectors rendered by `tests/gloss_render.rs::golden_files_match`
at width 80 (see `WIDTH` in that file) with the default theme. Each `.md`
input has a checked-in `.expected` plain-text rendering; regenerate with
`UPDATE_GOLDEN=1 cargo test --test gloss_render golden_files_match` and review
the diff before committing.

- `brackets.md` — bracket grouping stays together across wraps.
- `code.md` — fenced code blocks render verbatim.
- `edge.md` — blockquote, thematic break, unclosed fence.
- `frontmatter.md` — leading YAML frontmatter is stripped.
- `headings.md` — heading levels and inline emphasis.
- `list.md` — list wrapping, task items, desc terms, ordered lists.
- `paragraph.md` — prose wrapping, link flattening, inline code.
- `table.md` — basic table layout and separators.
- `table-compact.md` — tiny table pins compact natural sizing (T1).
- `table-wrap.md` — long prose cells, hard-break, CJK alignment, orphan prevention (P8/T4/T6).
- `table-hardbreak.md` — overlong URL and overlong word hard-break (T4/T6).
- `table-unicode.md` — em dash and CJK display-width alignment (T6).
- `prose-orphan.md` — `?hello?`, bracket groups, `](` seam, spaced-semicolon boundary, hard punctuation (P2/P6/P8/P9).
