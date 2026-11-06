---
status: open
type: bug
priority: medium
area: backend
---

## Indented tables and code blocks within list items

Problem: When tables or code blocks are nested inside list items
(indented), the formatter and renderer may not handle them correctly.
The table alignment post-processing and code block detection need to
account for indentation context.

Current behavior:

- Tables inside list items get aligned correctly by `align_tables()`
- But the detection logic only checks `line.starts_with('|')`, which may
  miss indented tables (e.g. `  | Col | ` inside a list item)
- Code blocks inside list items may have similar issues with fence
  detection

Expected behavior:

- `align_tables()` should detect indented table blocks (lines starting
  with whitespace + `|`)
- Code block preservation (`prefer_fenced`) should work correctly when
  nested inside list items or blockquotes
- The `strip_trailing_whitespace()` step should not break list item
  context (blank lines between list item text and nested blocks can be
  empty, but the nested block indentation must be preserved)

Exploration needed:

- Test edge cases: table in blockquote, table in nested list, code block
  in list item, code block in blockquote
- Verify comrak handles these correctly on its own vs needing our
  intervention
