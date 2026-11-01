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

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
enum SpliceEdit {
    StripTrailingWs { line: usize },
}

#[allow(dead_code)]
impl SpliceEdit {
    fn line(self) -> usize {
        match self {
            SpliceEdit::StripTrailingWs { line } => line,
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

#[allow(dead_code)]
/// Conservative splice skeleton (worker 1): strip trailing whitespace,
/// exempting hardbreak markers. Everything else passes through byte-identical.
fn splice_format(text: &str, _width: usize) -> String {
    let arena = comrak::Arena::new();
    let opts = crate::markdown::comrak_options();
    let root = comrak::parse_document(&arena, text, &opts);

    let mut hardbreak_lines = std::collections::HashSet::new();
    collect_hardbreak_lines(&root, &mut hardbreak_lines);

    let lines: Vec<&str> = text.split('\n').collect();
    let mut edits: Vec<SpliceEdit> = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        let lineno = idx + 1;
        if *line != line.trim_end() && !hardbreak_lines.contains(&lineno) {
            edits.push(SpliceEdit::StripTrailingWs { line: lineno });
        }
    }

    edits.sort_by_key(|e| e.line());
    for i in 1..edits.len() {
        assert!(
            edits[i].line() > edits[i - 1].line(),
            "splice edits must be sorted and disjoint"
        );
    }

    let mut out_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    for e in &edits {
        let SpliceEdit::StripTrailingWs { line } = e;
        let idx = line - 1;
        if idx < out_lines.len() {
            out_lines[idx] = out_lines[idx].trim_end().to_string();
        }
    }
    out_lines.join("\n")
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
}
