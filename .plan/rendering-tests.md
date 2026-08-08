---
status: open
type: feature
priority: medium
area: testing
---

## End-to-end testing for rendering and formatting

### Current test coverage

| Module         | Tests | What's covered                                             |
| -------------- | ----- | ---------------------------------------------------------- |
| lib.rs         | 14    | Config defaults, color conversion, status/priority mapping |
| theme.rs       | 4     | Theme discovery, fallback, field validation                |
| tasks.rs       | 19    | Frontmatter parsing, sorting, normalization                |
| markdown.rs    | 1     | Table rendering (basic)                                    |
| tui\_kanban.rs | 1     | Table rendering (basic)                                    |

### What's missing

#### Formatting pipeline (format\_commonmark)

The `format_commonmark()` function chains three steps: comrak formatting → `align_tables()` →
`strip_trailing_whitespace()`. No tests cover this pipeline.

**Test cases needed:**

- Table alignment: verify columns are padded to equal width
- Table alignment: verify alignment markers (`:--`, `:-:`, `--:`) are preserved
- Table alignment: verify indented tables (inside list items) are handled
- Fenced code blocks: verify they're preserved (not converted to indented)
- Trailing whitespace: verify all lines are trimmed
- Text wrapping: verify paragraphs wrap at specified width
- Task lists: verify `- [x]` / `- [ ]` items are preserved
- Nested content: verify tables/code blocks inside list items stay nested

**Approach:** Snapshot/golden-file tests. For each test case, provide input markdown and expected output. Compare
`format_commonmark(input, width)` against expected.

```
tests/
  format/
    table-alignment.md          # input
    table-alignment.expected.md # expected output
    fenced-code-blocks.md
    fenced-code-blocks.expected.md
    ...
```

#### Markdown rendering (render\_markdown)

The `render_markdown()` function produces `Vec<Line<Span>>` for ratatui. Only one test covers table rendering.

**Test cases needed:**

- Headings: verify h1/h2/h3 styles match theme
- Code spans: verify inline code gets `code` style
- Code blocks: verify multiline code blocks get `code_block` style
- Task lists: verify `- [x]` / `- [ ]` rendering
- Nested styles: verify bold inside heading, emphasis inside list item
- Links: verify link text gets underlined style
- Empty input: verify no panic
- Frontmatter: verify it's stripped before rendering

**Approach:** Check that specific spans have expected styles. The `Vec<Line<Span>>` output can be inspected for both
content and style properties.

#### Slug resolution

`resolve_slugs()` and `resolve_single_slug()` have no tests.

**Test cases needed:**

- Exact match: `foo` matches `foo.md`
- Prefix match: `fo` matches `foo.md` and `foobar.md`
- Substring match: `oo` matches `foo.md`
- Case insensitive: `FOO` matches `foo.md`
- No match: returns empty
- Priority: exact \> prefix \> substring

**Approach:** Create temp `.plan/` directory with test files, run resolution, verify results.

#### Filter mode (gloss)

`run_filter()` has no tests.

**Test cases needed:**

- Plain text output (no\_color mode)
- Verify stdin → stdout pipeline works
- Verify frontmatter is stripped

**Approach:** Capture stdout, compare against expected. May need to mock stdin.

### Recommended implementation order

1. **Formatting snapshot tests** — highest value, easiest to write
2. **Markdown rendering style tests** — catch regressions in theme application
3. **Slug resolution tests** — pure logic, easy to test
4. **Filter mode tests** — need to figure out stdin/stdout capture

### Example snapshot test structure

````rust
#[test]
fn format_table_alignment() {
    let input = "| A | B |\n|---|---|\n| short | x |\n| longer | y |\n";
    let expected = "| A      | B |\n| ------ | --- |\n| short  | x   |\n| longer | y   |\n";
    assert_eq!(format_commonmark(input, 80), expected);
}

#[test]
fn format_preserves_fenced_code_blocks() {
    let input = "text\n\n```\ncode\n```\n\nmore text\n";
    let result = format_commonmark(input, 80);
    assert!(result.contains("```\ncode\n```"), "fenced blocks should be preserved");
}

#[test]
fn format_strips_trailing_whitespace() {
    let input = "hello  \nworld\t\n";
    let result = format_commonmark(input, 80);
    for line in result.lines() {
        assert_eq!(line, line.trim_end(), "no trailing whitespace");
    }
}
````
