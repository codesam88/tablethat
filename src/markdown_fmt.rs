use comrak::Options;

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
    let mut opts = Options::default();
    opts.extension.table = true;
    opts.extension.tasklist = true;
    opts.extension.strikethrough = true;
    opts.extension.autolink = true;
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
}
