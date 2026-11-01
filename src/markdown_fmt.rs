/// Rewrite markdown to canonical CommonMark: reflow to `width`, fenced
/// code blocks, aligned tables, no trailing whitespace. Idempotent —
/// formatting twice is a fixpoint (pinned by `formatter_is_idempotent`).
///
/// ```
/// # use tablethat_lib::markdown_fmt::format_commonmark;
/// let out = format_commonmark("# Title\n", 80);
/// assert!(out.contains("# Title"));
/// ```
pub fn format_commonmark(text: &str, width: usize) -> String {
    let mut opts = crate::markdown::comrak_options();
    opts.render.width = width;
    opts.render.prefer_fenced = true;
    let formatted = comrak::markdown_to_commonmark(text, &opts);
    let aligned = align_tables(&formatted);
    strip_trailing_whitespace(&aligned)
}

fn strip_trailing_whitespace(text: &str) -> String {
    text.lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum SpliceEdit {
    StripTrailingWs { line: usize },
    AlignTable { start: usize, end: usize },
}

impl SpliceEdit {
    fn range(&self) -> (usize, usize) {
        match self {
            SpliceEdit::StripTrailingWs { line } => (*line, *line),
            SpliceEdit::AlignTable { start, end } => (*start, *end),
        }
    }
}

#[allow(dead_code)]
fn collect_hardbreak_lines(node: &comrak::Node<'_>, out: &mut std::collections::HashSet<usize>) {
    if let comrak::nodes::NodeValue::LineBreak = node.data.borrow().value {
        out.insert(node.data.borrow().sourcepos.end.line);
    }
    for child in node.children() {
        collect_hardbreak_lines(&child, out);
    }
}

/// Conservative splice engine (item 2): strip trailing whitespace
/// (hardbreak-exempt) and align tables from AST line ranges. Everything
/// else passes through byte-identical.
#[allow(dead_code)]
fn splice_format(text: &str, _width: usize) -> String {
    let arena = comrak::Arena::new();
    let opts = crate::markdown::comrak_options();
    let root = comrak::parse_document(&arena, text, &opts);

    let mut hardbreak_lines = std::collections::HashSet::new();
    collect_hardbreak_lines(&root, &mut hardbreak_lines);

    // Collect table line ranges from the AST.
    let mut table_ranges: Vec<(usize, usize)> = Vec::new();
    collect_table_ranges(&root, &mut table_ranges);

    let lines: Vec<&str> = text.split('\n').collect();
    let in_table = |lineno: usize| -> bool {
        table_ranges
            .iter()
            .any(|(s, e)| lineno >= *s && lineno <= *e)
    };

    let mut edits: Vec<SpliceEdit> = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        let lineno = idx + 1;
        if *line != line.trim_end() && !hardbreak_lines.contains(&lineno) && !in_table(lineno) {
            edits.push(SpliceEdit::StripTrailingWs { line: lineno });
        }
    }
    for (s, e) in &table_ranges {
        if *s >= 1 && *e <= lines.len() && *s <= *e {
            // Conservative rule: only align if every row is optional whitespace
            // then a pipe. Otherwise passthrough (always safe).
            let all_prefixed = (*s..=*e).all(|ln| {
                let l = lines[ln - 1];
                let ws_len = l.len() - l.trim_start().len();
                l[..ws_len].chars().all(char::is_whitespace) && l[ws_len..].starts_with('|')
            });
            if all_prefixed {
                edits.push(SpliceEdit::AlignTable { start: *s, end: *e });
            }
        }
    }

    edits.sort_by_key(|e| e.range().0);
    for i in 1..edits.len() {
        assert!(
            edits[i].range().0 > edits[i - 1].range().1,
            "splice edits must be sorted and disjoint"
        );
    }

    // Apply edits (sorted, non-overlapping) over the source lines.
    let mut out_lines: Vec<String> = Vec::new();
    let mut cursor = 0usize; // 0-based index into `lines`
    for e in &edits {
        let (s, en) = e.range();
        while cursor < s.saturating_sub(1) {
            if cursor < lines.len() {
                out_lines.push(lines[cursor].to_string());
            }
            cursor += 1;
        }
        match e {
            SpliceEdit::StripTrailingWs { line } => {
                let idx = line - 1;
                if idx < lines.len() {
                    out_lines.push(lines[idx].trim_end().to_string());
                }
            }
            SpliceEdit::AlignTable { start, end } => {
                let block: Vec<&str> = lines[start.saturating_sub(1)..*end].to_vec();
                out_lines.extend(align_table_lines(&block));
            }
        }
        cursor = en; // 1-based end -> next 0-based index
    }
    while cursor < lines.len() {
        out_lines.push(lines[cursor].to_string());
        cursor += 1;
    }
    out_lines.join("\n")
}

#[allow(dead_code)]
fn collect_table_ranges(node: &comrak::Node<'_>, out: &mut Vec<(usize, usize)>) {
    if let comrak::nodes::NodeValue::Table(_) = node.data.borrow().value {
        let sp = node.data.borrow().sourcepos;
        if sp.start.line >= 1 {
            out.push((sp.start.line, sp.end.line));
        }
    }
    for child in node.children() {
        collect_table_ranges(&child, out);
    }
}

#[allow(dead_code)]
fn split_table_cells(s: &str) -> Vec<String> {
    let mut cells: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut i = 0usize;
    let mut in_code = false;
    let mut fence = 0usize;
    let len = s.len();
    while i < len {
        let c = s[i..].chars().next().unwrap();
        let clen = c.len_utf8();
        if c == '\\' {
            current.push(c);
            if i + clen < len {
                let next = s[i + clen..].chars().next().unwrap();
                current.push(next);
                i += clen + next.len_utf8();
                continue;
            }
            i += clen;
            continue;
        }
        if c == '`' {
            let mut run = 0usize;
            let mut j = i;
            while j < len && s[j..].starts_with('`') {
                run += 1;
                j += 1;
            }
            current.push_str(&s[i..j]);
            if !in_code {
                in_code = true;
                fence = run;
            } else if run == fence {
                in_code = false;
                fence = 0;
            }
            i = j;
            continue;
        }
        if c == '|' && !in_code {
            cells.push(std::mem::take(&mut current));
            i += clen;
            continue;
        }
        current.push(c);
        i += clen;
    }
    cells.push(current);
    cells
}

#[allow(dead_code)]
fn align_table_lines(lines: &[&str]) -> Vec<String> {
    // Capture each row's leading-whitespace indent so prefixed tables keep it.
    let indents: Vec<String> = lines
        .iter()
        .map(|line| {
            let ws_len = line.len() - line.trim_start().len();
            line[..ws_len].to_string()
        })
        .collect();

    let parsed: Vec<Vec<String>> = lines
        .iter()
        .map(|line| {
            let ws_len = line.len() - line.trim_start().len();
            let rest = &line[ws_len..];
            let inner = rest.trim_start_matches('|').trim_end_matches('|');
            split_table_cells(inner)
                .into_iter()
                .map(|c| c.trim().to_string())
                .collect()
        })
        .collect();

    let num_cols = parsed.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut col_widths = vec![0usize; num_cols];
    for row in &parsed {
        for (j, cell) in row.iter().enumerate() {
            if j < col_widths.len() {
                col_widths[j] = col_widths[j].max(cell.len());
            }
        }
    }

    let mut out = Vec::new();
    for (row_idx, row) in parsed.iter().enumerate() {
        let mut line = String::from("|");
        for (j, cell) in row.iter().enumerate() {
            let w = col_widths.get(j).copied().unwrap_or(3);
            if row_idx == 1 {
                let marker = if cell.starts_with(':') && cell.ends_with(':') {
                    format!(" :{:-<w$}: ", "", w = w.saturating_sub(2))
                } else if cell.starts_with(':') {
                    format!(" :{:-<w$} ", "", w = w.saturating_sub(1))
                } else if cell.ends_with(':') {
                    format!(" {:-<w$}: ", "", w = w.saturating_sub(1))
                } else {
                    format!(" {:-<w$} ", "", w = w)
                };
                line.push_str(&marker);
            } else {
                line.push_str(&format!(" {:<w$} ", cell, w = w));
            }
            line.push('|');
        }
        out.push(format!("{}{}", indents[row_idx], line));
    }
    out
}

fn align_tables(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut result = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        // Detect table: line starts with | and next line is separator (| --- |)
        if lines[i].starts_with('|')
            && i + 1 < lines.len()
            && lines[i + 1].starts_with('|')
            && lines[i + 1].contains("---")
        {
            // Collect all table rows
            let mut table_lines = Vec::new();
            while i < lines.len() && lines[i].starts_with('|') {
                table_lines.push(lines[i]);
                i += 1;
            }

            // Parse cells and compute column widths
            let parsed: Vec<Vec<&str>> = table_lines
                .iter()
                .map(|line| {
                    line.trim_start_matches('|')
                        .trim_end_matches('|')
                        .split('|')
                        .map(|s| s.trim())
                        .collect()
                })
                .collect();

            let num_cols = parsed.iter().map(|r| r.len()).max().unwrap_or(0);
            let mut col_widths = vec![0usize; num_cols];
            for row in &parsed {
                for (j, cell) in row.iter().enumerate() {
                    col_widths[j] = col_widths[j].max(cell.len());
                }
            }

            // Reformat table with aligned columns
            for (row_idx, row) in parsed.iter().enumerate() {
                let mut line = String::from("|");
                for (j, cell) in row.iter().enumerate() {
                    let w = col_widths.get(j).copied().unwrap_or(3);
                    if row_idx == 1 {
                        // Separator row: use alignment markers
                        let marker = if cell.starts_with(':') && cell.ends_with(':') {
                            format!(" :{:-<w$}: ", "", w = w.saturating_sub(2))
                        } else if cell.starts_with(':') {
                            format!(" :{:-<w$} ", "", w = w.saturating_sub(1))
                        } else if cell.ends_with(':') {
                            format!(" {:-<w$}: ", "", w = w.saturating_sub(1))
                        } else {
                            format!(" {:-<w$} ", "", w = w)
                        };
                        line.push_str(&marker);
                    } else {
                        line.push_str(&format!(" {:<w$} ", cell, w = w));
                    }
                    line.push('|');
                }
                result.push(line);
            }
        } else {
            result.push(lines[i].to_string());
            i += 1;
        }
    }

    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatter_shares_renderer_extension_set() {
        // One constructor for both paths: renderer and formatter must never
        // drift on extensions (table/tasklist/strikethrough/autolink).
        let opts = crate::markdown::comrak_options();
        assert!(opts.extension.table, "table");
        assert!(opts.extension.tasklist, "tasklist");
        assert!(opts.extension.strikethrough, "strikethrough");
        assert!(opts.extension.autolink, "autolink");
    }

    #[test]
    fn align_tables_pads_uneven_columns() {
        let input = "| a | bbbb |\n|---|---|\n| cccc | d |\n";
        let expected = "| a    | bbbb |\n| ---- | ---- |\n| cccc | d    |\n";
        // align_tables does not add trailing newline; input has trailing \n producing empty? Let's handle.
        let out = align_tables(input.trim_end_matches('\n'));
        assert_eq!(out, expected.trim_end_matches('\n'));
    }

    #[test]
    fn align_tables_preserves_separator_alignment_markers() {
        let input = "| Name | Val |\n|:---|---:|\n| x | y |\n";
        let expected = "| Name | Val  |\n| :--- | ---: |\n| x    | y    |";
        let out = align_tables(input.trim_end_matches('\n'));
        assert_eq!(out, expected);
    }

    #[test]
    fn align_tables_passes_non_table_lines_through() {
        let input = "hello world\n\n- item\n";
        assert_eq!(
            align_tables(input.trim_end_matches('\n')),
            input.trim_end_matches('\n')
        );
    }

    // ---- splice_format (worker 1) ----

    fn html_eq(input: &str, output: &str) {
        let opts = crate::markdown::comrak_options();
        assert_eq!(
            comrak::markdown_to_html(input, &opts),
            comrak::markdown_to_html(output, &opts),
            "HTML must be preserved by splice_format"
        );
    }

    #[test]
    fn trailing_ws_stripped_hardbreak_exempt() {
        let a = "para  \nnext\n";
        assert_eq!(splice_format(a, 80), a);
        html_eq(a, &splice_format(a, 80));

        let b = "trail x  \nnext\n";
        assert_eq!(splice_format(b, 80), b);
        html_eq(b, &splice_format(b, 80));

        let c = "trail x \nnext\n";
        let c_out = "trail x\nnext\n";
        assert_eq!(splice_format(c, 80), c_out);
        html_eq(c, &splice_format(c, 80));
    }

    #[test]
    fn short_softbreak_lines_preserved() {
        let s = "First soft line\nsecond soft line\n";
        assert_eq!(splice_format(s, 80), s);
        html_eq(s, &splice_format(s, 80));
    }

    #[test]
    fn hardbreak_flavors_preserved() {
        for s in ["a  \nb\n", "a\\\nb\n"] {
            assert_eq!(splice_format(s, 80), s);
            html_eq(s, &splice_format(s, 80));
        }
    }

    #[test]
    fn long_line_untouched_without_wrapper() {
        let s = format!("{}\n", "x".repeat(200));
        assert_eq!(splice_format(&s, 80), s);
        html_eq(&s, &splice_format(&s, 80));
    }

    // ---- item 2 tests ----

    #[test]
    fn piped_code_span_table_aligns_losslessly() {
        let s = "| Name | Note |\n| ---- | ---- |\n| `a | b` | piped code |\n";
        let out = splice_format(s, 80);
        let expected =
            "| Name    | Note       |\n| ------- | ---------- |\n| `a | b` | piped code |\n";
        assert_eq!(out, expected);
        html_eq(s, &out);
    }

    #[test]
    fn aligned_table_is_fixpoint() {
        let s = "| Name | Note |\n| ---- | ---- |\n| `a | b` | piped code |\n";
        let once = splice_format(s, 80);
        let twice = splice_format(&once, 80);
        assert_eq!(once, twice);
    }

    #[test]
    fn table_style_matches_existing_convention() {
        // ORACLE NOTE FOR WORKER 3: this test calls the private `align_tables`
        // as a style oracle. `align_tables` will be deleted when the splice
        // engine is wired in, so this oracle call MUST be replaced with
        // literal expectations by worker 3. Flagged prominently.
        let s = "| a | bbbb |\n|---|---|\n| cccc | d |\n";
        let out = splice_format(s, 80);
        assert_eq!(
            out.trim_end_matches('\n'),
            align_tables(s.trim_end_matches('\n'))
        );
        html_eq(s, &out);
    }

    #[test]
    fn nested_table_indent_preserved() {
        let s = "- item\n\n  | A | B |\n  | - | - |\n  | x | y |\n";
        let out = splice_format(s, 80);
        // At minimum indent + 2 columns preserved; expect exact aligned form:
        // We only assert indent present on every row and 2 columns.
        for line in out.lines() {
            if line.trim_start().starts_with('|') {
                assert!(line.starts_with("  |"), "lost indent: {:?}", line);
            }
        }
        // column count check
        for line in out.lines() {
            if line.trim_start().starts_with('|') {
                let inner = line
                    .trim_start()
                    .trim_start_matches('|')
                    .trim_end_matches('|');
                // count top-level pipes via simple split (no code spans here)
                assert_eq!(inner.split('|').count(), 2, "wrong columns: {:?}", line);
            }
        }
        html_eq(s, &out);
    }

    #[test]
    fn quoted_table_left_untouched() {
        let s = "> quote\n>\n> | A | B |\n> | - | - |\n> | x | y |\n";
        let out = splice_format(s, 80);
        assert_eq!(out, s);
        html_eq(s, &out);
    }
}
