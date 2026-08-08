use crate::ThemeConfig;
use comrak::nodes::{NodeCodeBlock, NodeValue};
use comrak::{Arena, Options, parse_document};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

pub struct MarkdownTheme {
    pub h1: Style,
    pub h2: Style,
    pub h3: Style,
    pub bold: Style,
    pub dim: Style,
    pub code: Style,
    pub code_block: Style,
}

pub fn theme_from_cfg(cfg: &ThemeConfig) -> MarkdownTheme {
    let bold_mod = match cfg.bold_style.as_str() {
        "bold" => Modifier::BOLD,
        "dim" => Modifier::DIM,
        "italic" => Modifier::ITALIC,
        "underlined" => Modifier::UNDERLINED,
        _ => Modifier::BOLD,
    };
    let emphasis_mod = match cfg.emphasis_style.as_str() {
        "bold" => Modifier::BOLD,
        "dim" => Modifier::DIM,
        "italic" => Modifier::ITALIC,
        "underlined" => Modifier::UNDERLINED,
        _ => Modifier::UNDERLINED,
    };

    MarkdownTheme {
        h1: Style::default()
            .fg(cfg.h1_color)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        h2: Style::default()
            .fg(cfg.h2_color)
            .add_modifier(Modifier::UNDERLINED),
        h3: Style::default().fg(cfg.h3_color),
        bold: Style::default().add_modifier(bold_mod),
        dim: Style::default().add_modifier(emphasis_mod),
        code: Style::default().fg(cfg.code_color),
        code_block: Style::default().fg(cfg.code_block_color),
    }
}

pub fn strip_frontmatter(text: &str) -> &str {
    if let Some(rest) = text.trim_start().strip_prefix("---")
        && let Some(end) = rest.find("---")
    {
        return rest[end + 3..].trim_start();
    }
    text
}

fn wrap_lines(text: &str, max: usize) -> Vec<String> {
    if text.len() <= max {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in text.split_inclusive(' ') {
        if cur.len() + word.trim_end().len() > max && !cur.is_empty() {
            out.push(cur.trim_end().to_string());
            cur = word.trim_start().to_string();
        } else {
            cur.push_str(word);
        }
    }
    if !cur.is_empty() {
        out.push(cur.trim_end().to_string());
    }
    out
}

fn wrap_spans(spans: &[Span<'static>], max: usize) -> Vec<Vec<Span<'static>>> {
    // Strategy: process spans character by character, accumulating a "pending" buffer.
    // The pending buffer holds (text, style) pieces that haven't been committed to a line yet.
    // We only flush pending to the current line when we reach a safe break point.
    // This preserves style boundaries while keeping brackets with their content.

    let mut out: Vec<Vec<Span<'static>>> = Vec::new();
    let mut cur: Vec<(String, Style)> = Vec::new(); // current line
    let mut pending: Vec<(String, Style)> = Vec::new(); // accumulated, not yet committed
    let mut line_len = 0usize;
    let mut bracket_depth = 0i32; // positive = inside [] or ()

    // Helper: try to commit pending pieces to current line
    // Returns the length of pending text
    fn pending_len(pending: &[(String, Style)]) -> usize {
        pending.iter().map(|(s, _)| s.len()).sum()
    }

    // Helper: check if pending ends with an opening bracket
    fn pending_ends_with_open(pending: &[(String, Style)]) -> bool {
        pending
            .last()
            .map(|(s, _)| s.ends_with('(') || s.ends_with('['))
            .unwrap_or(false)
    }

    // Helper: append a character to pending, merging with last piece if same style
    fn push_char(pending: &mut Vec<(String, Style)>, ch: char, style: Style) {
        if let Some(last) = pending.last_mut() {
            if last.1 == style && !ch.is_control() {
                last.0.push(ch);
                return;
            }
        }
        pending.push((ch.to_string(), style));
    }

    for span in spans {
        let text = span.content.as_ref();
        let style = span.style;

        for ch in text.chars() {
            if ch == ' ' {
                // Space is always a break point
                // First, commit any pending that shouldn't be held back
                let pends_with_open = pending_ends_with_open(&pending);
                if !pends_with_open && !pending.is_empty() {
                    let plen = pending_len(&pending);
                    if line_len + plen > max && !cur.is_empty() {
                        out.push(cur.drain(..).map(|(s, st)| Span::styled(s, st)).collect());
                        line_len = 0;
                    }
                    for piece in pending.drain(..) {
                        line_len += piece.0.len();
                        cur.push(piece);
                    }
                }
                // Now handle the space
                if line_len + 1 > max && !cur.is_empty() {
                    out.push(cur.drain(..).map(|(s, st)| Span::styled(s, st)).collect());
                    line_len = 0;
                } else {
                    cur.push((" ".to_string(), style));
                    line_len += 1;
                }
            } else if ch == ']' && bracket_depth > 0 {
                // Closing bracket while inside brackets - part of the group
                push_char(&mut pending, ch, style);
                bracket_depth -= 1;
            } else if ch == ']' {
                // Standalone ']' - could be start of '](' link syntax
                push_char(&mut pending, ch, style);
            } else if ch == '(' && !pending.is_empty() && pending.last().unwrap().0.ends_with(']') {
                // '(' immediately after ']' - this is '](' link syntax
                // Commit pending up to and including ']', then start new group with '('
                let plen = pending_len(&pending);
                if line_len + plen > max && !cur.is_empty() {
                    out.push(cur.drain(..).map(|(s, st)| Span::styled(s, st)).collect());
                    line_len = 0;
                }
                for piece in pending.drain(..) {
                    line_len += piece.0.len();
                    cur.push(piece);
                }
                // '(' starts the URL part - new bracket group
                pending.push(("(".to_string(), style));
                bracket_depth += 1;
            } else if ch == '(' || ch == '[' {
                // Opening bracket - increase depth, add to pending
                push_char(&mut pending, ch, style);
                bracket_depth += 1;
            } else if ch == ')' && bracket_depth > 0 {
                // Closing bracket inside group
                push_char(&mut pending, ch, style);
                bracket_depth -= 1;
                // If we just closed the outermost bracket, check if we should commit
                if bracket_depth == 0 {
                    let plen = pending_len(&pending);
                    if line_len + plen > max && !cur.is_empty() {
                        out.push(cur.drain(..).map(|(s, st)| Span::styled(s, st)).collect());
                        line_len = 0;
                    }
                    for piece in pending.drain(..) {
                        line_len += piece.0.len();
                        cur.push(piece);
                    }
                }
            } else {
                // Regular character
                push_char(&mut pending, ch, style);
            }
        }
    }

    // Flush any remaining pending
    if !pending.is_empty() {
        let plen = pending_len(&pending);
        if line_len + plen > max && !cur.is_empty() {
            out.push(cur.drain(..).map(|(s, st)| Span::styled(s, st)).collect());
        }
        cur.append(&mut pending);
    }

    if !cur.is_empty() {
        out.push(cur.drain(..).map(|(s, st)| Span::styled(s, st)).collect());
    }

    out
}

fn comrak_options() -> Options<'static> {
    let mut opts = Options::default();
    opts.extension.table = true;
    opts.extension.tasklist = true;
    opts.extension.strikethrough = true;
    opts.extension.autolink = true;
    opts
}

fn collect_text(node: &comrak::Node<'_>) -> String {
    let mut buf = String::new();
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Text(t) => buf.push_str(t),
            NodeValue::Code(c) => buf.push_str(&c.literal),
            NodeValue::SoftBreak | NodeValue::LineBreak => buf.push(' '),
            _ => buf.push_str(&collect_text(&child)),
        }
    }
    buf
}

fn render_inline(
    node: &comrak::Node<'_>,
    th: &MarkdownTheme,
    spans: &mut Vec<Span<'static>>,
    base_style: Style,
) {
    for child in node.children() {
        let data = child.data.borrow();
        match &data.value {
            NodeValue::Text(t) => {
                if !t.is_empty() {
                    spans.push(Span::styled(t.to_string(), base_style));
                }
            }
            NodeValue::Code(code) => {
                if !code.literal.is_empty() {
                    spans.push(Span::styled(code.literal.clone(), th.code));
                }
            }
            NodeValue::Strong => {
                let s = base_style.add_modifier(Modifier::BOLD);
                render_inline(&child, th, spans, s);
            }
            NodeValue::Emph => {
                let s = base_style.add_modifier(Modifier::DIM);
                render_inline(&child, th, spans, s);
            }
            NodeValue::Strikethrough => {
                render_inline(&child, th, spans, base_style);
            }
            NodeValue::Link(_link) => {
                let mut s = base_style;
                s = s.add_modifier(Modifier::UNDERLINED);
                render_inline(&child, th, spans, s);
            }
            NodeValue::SoftBreak | NodeValue::LineBreak => {
                spans.push(Span::styled(" ".to_string(), base_style));
            }
            NodeValue::HtmlInline(html) => {
                spans.push(Span::styled(html.clone(), base_style));
            }
            NodeValue::Image(link) => {
                let alt = collect_text(&child);
                let text = if alt.is_empty() {
                    link.url.clone()
                } else {
                    alt
                };
                spans.push(Span::styled(text, base_style));
            }
            _ => {
                render_inline(&child, th, spans, base_style);
            }
        }
    }
}

fn render_table(node: &comrak::Node<'_>, lines: &mut Vec<Line<'static>>) {
    let mut tbl: Vec<Vec<String>> = Vec::new();
    for row_node in node.children() {
        if let NodeValue::TableRow(_) = row_node.data.borrow().value {
            let mut cur_row: Vec<String> = Vec::new();
            for cell_node in row_node.children() {
                if let NodeValue::TableCell = cell_node.data.borrow().value {
                    cur_row.push(collect_text(&cell_node).trim().to_string());
                }
            }
            tbl.push(cur_row);
        }
    }

    if tbl.is_empty() {
        return;
    }
    let ncols = tbl.iter().map(|r| r.len()).max().unwrap_or(0);
    if ncols == 0 {
        return;
    }

    struct ColW {
        min: usize,
        p60: usize,
        p80: usize,
        p100: usize,
    }
    let mut cols: Vec<ColW> = Vec::new();
    for _ in 0..ncols {
        cols.push(ColW {
            min: 0,
            p60: 0,
            p80: 0,
            p100: 0,
        });
    }
    let mut all_lens: Vec<Vec<usize>> = vec![Vec::new(); ncols];

    for row in &tbl {
        for (i, cell) in row.iter().enumerate() {
            let lw = cell
                .split_whitespace()
                .map(|w| w.len())
                .max()
                .unwrap_or(cell.len());
            cols[i].min = cols[i].min.max(lw.min(40));
            all_lens[i].push(cell.len());
        }
    }

    for i in 0..ncols {
        let mut s = all_lens[i].clone();
        s.sort_unstable();
        let min = cols[i].min;
        let n = s.len();
        let idx60 = ((n as f64 * 0.6).ceil() as usize).saturating_sub(1);
        let idx80 = ((n as f64 * 0.8).ceil() as usize).saturating_sub(1);
        let last = n.saturating_sub(1);
        let cap = |v: usize| v.max(min).min(40);
        cols[i].p60 = cap(s.get(idx60).copied().unwrap_or(4));
        cols[i].p80 = cap(s.get(idx80).copied().unwrap_or(4)).max(cols[i].p60);
        cols[i].p100 = cap(s.get(last).copied().unwrap_or(4)).max(cols[i].p80);
    }

    let border_w = ncols * 3 + 1;
    let p80_total: usize = cols.iter().map(|c| c.p80).sum::<usize>() + border_w;
    let base_target = 78usize;
    let wide_target = 160usize;
    let target = if p80_total <= wide_target {
        base_target
            .max(p80_total)
            .min(wide_target)
            .saturating_sub(border_w)
    } else {
        base_target.saturating_sub(border_w)
    };

    let mut col_w: Vec<usize> = cols.iter().map(|c| c.p60).collect();
    let used: usize = col_w.iter().sum();

    if used > target {
        let deficit = used - target;
        let flex: usize = col_w
            .iter()
            .zip(&cols)
            .map(|(&w, c)| w.saturating_sub(c.min))
            .sum();
        col_w = col_w
            .iter()
            .enumerate()
            .map(|(i, &w)| {
                let room = w.saturating_sub(cols[i].min);
                w.saturating_sub(if flex > 0 {
                    deficit * room / flex.max(1)
                } else {
                    0
                })
                .max(cols[i].min)
            })
            .collect();
    }

    for &level in &[1, 2] {
        if col_w.iter().sum::<usize>() >= target {
            break;
        }
        let mut order: Vec<usize> = (0..ncols).collect();
        let gain = |i: usize| -> usize {
            let target_w = if level == 1 {
                cols[i].p80
            } else {
                cols[i].p100
            };
            target_w.saturating_sub(col_w[i])
        };
        order.sort_by_key(|&b| std::cmp::Reverse(gain(b)));
        for &i in &order {
            let target_w = if level == 1 {
                cols[i].p80
            } else {
                cols[i].p100
            };
            let add = target_w.saturating_sub(col_w[i]);
            if add == 0 {
                continue;
            }
            let room = target - col_w.iter().sum::<usize>();
            let take = add.min(room);
            col_w[i] += take;
            if col_w.iter().sum::<usize>() >= target {
                break;
            }
        }
    }

    let slack = target.saturating_sub(col_w.iter().sum());
    if slack > 0
        && let Some(max_i) = (0..ncols).max_by_key(|&i| col_w[i])
    {
        col_w[max_i] += slack;
    }

    let mut sep_count = 0usize;
    for row in &tbl {
        if row.is_empty() {
            continue;
        }
        if sep_count > 0 {
            let ch = if sep_count == 1 { '=' } else { '-' };
            let mut sep = String::from("|");
            for &w in &col_w {
                let dashes: String = std::iter::repeat_n(ch, w + 1).collect();
                sep.push_str(&format!("{}|", dashes));
            }
            lines.push(Line::from(sep));
        }
        let mut buf = String::from("|");
        for (i, &w) in col_w.iter().enumerate() {
            let raw = row.get(i).map(|s| s.as_str()).unwrap_or("");
            let cell: String = raw.chars().take(w).collect();
            buf.push_str(&format!(" {:<w$}|", cell, w = w));
        }
        lines.push(Line::from(buf));
        sep_count += 1;
    }
}

fn render_node(
    node: &comrak::Node<'_>,
    th: &MarkdownTheme,
    lines: &mut Vec<Line<'static>>,
    wrap: usize,
) {
    let data = node.data.borrow();
    match &data.value {
        NodeValue::Document | NodeValue::FrontMatter(_) => {
            for child in node.children() {
                render_node(&child, th, lines, wrap);
            }
        }
        NodeValue::Heading(heading) => {
            let (prefix, style) = match heading.level {
                1 => ("# ", th.h1),
                2 => ("## ", th.h2),
                _ => ("### ", th.h3),
            };
            let mut spans = vec![Span::styled(prefix.to_string(), style)];
            render_inline(node, th, &mut spans, style);
            lines.push(Line::from(spans));
            lines.push(Line::from(""));
        }
        NodeValue::Paragraph => {
            let mut spans: Vec<Span<'static>> = Vec::new();
            render_inline(node, th, &mut spans, Style::default());
            if !spans.is_empty() {
                let wrapped = wrap_spans(&spans, wrap);
                for line_spans in wrapped {
                    lines.push(Line::from(line_spans));
                }
            }
            lines.push(Line::from(""));
        }
        NodeValue::BlockQuote => {
            for child in node.children() {
                render_node(&child, th, lines, wrap);
            }
        }
        NodeValue::List(_list) => {
            for child in node.children() {
                render_node(&child, th, lines, wrap);
            }
            lines.push(Line::from(""));
        }
        NodeValue::Item(_list) => {
            let mut item_text = String::new();

            for child in node.children() {
                item_text.push_str(&collect_text(&child));
            }

            let content = item_text.trim();
            if !content.is_empty() {
                let prefix = "  - ";
                let first = format!("{}{}", prefix, content);
                let lw = if wrap < 40 { 78 } else { wrap };
                for (i, seg) in wrap_lines(&first, lw).iter().enumerate() {
                    let s: String = if i == 0 {
                        seg.into()
                    } else {
                        format!("    {}", seg)
                    };
                    lines.push(Line::from(s));
                }
            }
        }
        NodeValue::TaskItem(task) => {
            let checked = task.symbol == Some('x') || task.symbol == Some('X');
            let mut item_text = String::new();

            for child in node.children() {
                item_text.push_str(&collect_text(&child));
            }

            let content = item_text.trim();
            let check_mark = if checked { "[x]" } else { "[ ]" };
            let prefix = format!("  - {} ", check_mark);
            let indent_sz = prefix.len();
            let first = format!("{}{}", prefix, content);
            let indent = " ".repeat(indent_sz);
            let lw = if wrap < 40 { 78 } else { wrap };
            for (i, seg) in wrap_lines(&first, lw).iter().enumerate() {
                let s: String = if i == 0 {
                    seg.into()
                } else {
                    format!("{}{}", indent, seg)
                };
                lines.push(Line::from(s));
            }
        }
        NodeValue::CodeBlock(code_block) => {
            let NodeCodeBlock { literal, .. } = code_block.as_ref();
            for line in literal.lines() {
                lines.push(Line::from(Span::styled(line.to_string(), th.code_block)));
            }
            lines.push(Line::from(""));
        }
        NodeValue::Table(_) => {
            render_table(node, lines);
            lines.push(Line::from(""));
        }
        NodeValue::ThematicBreak => {
            lines.push(Line::from("---".to_string()));
            lines.push(Line::from(""));
        }
        NodeValue::HtmlBlock(html_block) => {
            for line in html_block.literal.lines() {
                lines.push(Line::from(line.to_string()));
            }
        }
        _ => {
            for child in node.children() {
                render_node(&child, th, lines, wrap);
            }
        }
    }
}

pub fn render_markdown(th: &MarkdownTheme, text: &str, wrap: usize) -> Vec<Line<'static>> {
    let body = strip_frontmatter(text);
    let arena = Arena::new();
    let opts = comrak_options();
    let root = parse_document(&arena, body, &opts);

    let mut lines: Vec<Line<'static>> = Vec::new();
    render_node(&root, th, &mut lines, wrap);
    lines
}

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

    // ==========================================================================
    // WORD WRAP RULES
    // ==========================================================================
    //
    // 1. PARAGRAPH TEXT (wrap_spans):
    //    - Break at spaces
    //    - Break at '](' sequence: ']' stays with preceding text, '(' stays with following text
    //    - Do NOT break at: '[', ']', '(', ')' individually
    //    - Opening brackets stay with following word: (word stays together
    //    - Closing brackets stay with preceding word: word) stays together
    //    - Bracket groups stay together: (word), [word], [text](url)
    //    - Bracket nesting is tracked across spans for proper grouping
    //
    // 2. LIST ITEMS (wrap_lines):
    //    - Standard word wrap at spaces only
    //    - No special bracket handling (plain text)
    //    - Continuation lines are indented to align with content
    //
    // 3. CODE BLOCKS:
    //    - NO wrapping - output as-is, preserving original line breaks
    //    - Each line of code becomes one output line
    //
    // 4. TABLES:
    //    - Column-based layout with computed widths
    //    - No word wrap within cells
    //    - Columns padded to calculated width (p60/p80/p100 percentiles)
    //
    // 5. HEADINGS:
    //    - Single line, no wrapping
    //    - Prefix (#, ##, ###) included in line
    //
    // ==========================================================================

    #[test]
    fn table_renders_header_and_body() {
        let md = "| **Name** | `Code` |\n|---|---|\n| foo | bar |\n";
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, md, 80);
        let total: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(total.contains("Name"), "Missing header 'Name'");
        assert!(total.contains("foo"), "Missing body 'foo'");
        assert!(total.contains("="), "Missing separator");
    }

    // -------------------------------------------------------------------------
    // Paragraph wrapping tests
    // -------------------------------------------------------------------------

    #[test]
    fn wrap_paragraph_at_spaces() {
        let spans = vec![Span::styled("one two three four five", Style::default())];
        let wrapped = wrap_spans(&spans, 15);
        let lines: Vec<String> = wrapped
            .iter()
            .map(|l| l.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        eprintln!("wrap_paragraph_at_spaces lines: {:?}", lines);
        // Should break at spaces into multiple lines
        assert!(lines.len() > 1, "Should wrap into multiple lines");
        // First line should start with 'one'
        assert!(lines[0].starts_with("one"));
    }

    #[test]
    fn wrap_does_not_break_between_bracket_and_word() {
        let spans = vec![Span::styled(
            "some text before [link text](http://example.com) and more",
            Style::default(),
        )];
        let wrapped = wrap_spans(&spans, 40);
        let lines: Vec<String> = wrapped
            .iter()
            .map(|line| line.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        for line in &lines {
            assert!(
                !line.starts_with("link"),
                "Should not break between '[' and 'link', got line: {}",
                line
            );
        }
        let all_text = lines.join("\n");
        assert!(
            all_text.contains("[link"),
            "Bracket and word should stay together"
        );
    }

    #[test]
    fn wrap_does_not_break_between_paren_and_word() {
        let spans = vec![Span::styled(
            "some text before (paren word) and more after",
            Style::default(),
        )];
        let wrapped = wrap_spans(&spans, 30);
        let lines: Vec<String> = wrapped
            .iter()
            .map(|line| line.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        for line in &lines {
            assert!(
                !line.ends_with("("),
                "Should not break between '(' and 'paren', got line: {}",
                line
            );
        }
        let all_text = lines.join("\n");
        assert!(
            all_text.contains("(paren"),
            "Paren and word should stay together"
        );
    }

    #[test]
    fn wrap_breaks_at_link_boundary() {
        // The '](' sequence is a valid break point
        // ']' stays with preceding text, '(' stays with following text
        let spans = vec![Span::styled(
            "text before [link text](http://example.com) and more text after",
            Style::default(),
        )];
        let wrapped = wrap_spans(&spans, 25);
        let lines: Vec<String> = wrapped
            .iter()
            .map(|l| l.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        // Should break between ']' and '('
        let all_text = lines.join("\n");
        assert!(all_text.contains("]"), "']' should be present");
        assert!(all_text.contains("("), "'(' should be present");
        // '](' should NOT appear together (they should be split across lines)
        assert!(
            !all_text.contains("]("),
            "']' and '(' should be on separate lines"
        );
    }

    #[test]
    fn wrap_standalone_brackets_not_break_points() {
        // Standalone '[', ']', '(', ')' should not cause breaks
        let spans = vec![Span::styled(
            "words with [brackets] and (parens) inside",
            Style::default(),
        )];
        let wrapped = wrap_spans(&spans, 50);
        let lines: Vec<String> = wrapped
            .iter()
            .map(|l| l.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        // All on one line since it fits
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("[brackets]"));
        assert!(lines[0].contains("(parens)"));
    }

    #[test]
    fn wrap_closing_bracket_stays_with_previous() {
        // Closing ')' should stay with previous word even across span boundaries
        // This simulates markdown like: (`input`)
        let spans = vec![
            Span::styled("x (", Style::default()),
            Span::styled("input", Style::default()),
            Span::styled(")", Style::default()),
        ];
        let wrapped = wrap_spans(&spans, 8);
        let lines: Vec<String> = wrapped
            .iter()
            .map(|l| l.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        // '(input)' should stay together
        let all_text = lines.join("\n");
        assert!(
            all_text.contains("(input)"),
            "Closing paren should stay with word, got: {:?}",
            lines
        );
    }

    #[test]
    fn render_markdown_with_inline_code_and_parens() {
        // Test the full rendering pipeline with markdown like: x (`input`)
        // The inline code creates separate spans for '(' and ')'
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let md = "x (`input`)";

        // Test at width 8 - should fit on one line
        let lines = render_markdown(&th, md, 8);
        let text_lines: Vec<String> = lines
            .iter()
            .filter(|l| !l.spans.is_empty())
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        eprintln!("width=8: {:?}", text_lines);
        let all_text = text_lines.join("\n");
        assert!(
            all_text.contains("(input)"),
            "'(input)' should stay together at width 8, got: {:?}",
            text_lines
        );

        // Test at width 7 - should break but keep (input) together
        let lines = render_markdown(&th, md, 7);
        let text_lines: Vec<String> = lines
            .iter()
            .filter(|l| !l.spans.is_empty())
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        eprintln!("width=7: {:?}", text_lines);
        let all_text = text_lines.join("\n");
        assert!(
            all_text.contains("(input)"),
            "'(input)' should stay together at width 7, got: {:?}",
            text_lines
        );
    }

    // -------------------------------------------------------------------------
    // List item wrapping tests
    // -------------------------------------------------------------------------

    #[test]
    fn list_item_wraps_with_indent() {
        let md = "- This is a long list item that should wrap at the margin and continue with proper indentation\n";
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, md, 40);
        let text_lines: Vec<String> = lines
            .iter()
            .filter(|l| !l.spans.is_empty())
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        // First line has the bullet
        assert!(text_lines[0].starts_with("  - "));
        // Continuation lines are indented
        if text_lines.len() > 1 {
            assert!(
                text_lines[1].starts_with("    "),
                "Continuation should be indented: {:?}",
                text_lines[1]
            );
        }
    }

    // -------------------------------------------------------------------------
    // Code block tests (no wrapping)
    // -------------------------------------------------------------------------

    #[test]
    fn code_block_no_wrapping() {
        let md = "```\nfn main() {\n    let x = 1;\n    let long_variable_name = something_very_long_that_would_never_fit_on_one_line_if_wrapped;\n}\n```\n";
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, md, 40);
        // Find the code lines (they have code_block style)
        let code_lines: Vec<String> = lines
            .iter()
            .filter(|l| l.spans.iter().any(|s| s.style == th.code_block))
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();
        // Code should NOT be wrapped - long line stays as-is
        assert!(
            code_lines.iter().any(|l| l.contains("long_variable_name")),
            "Code should preserve original lines without wrapping"
        );
    }

    // -------------------------------------------------------------------------
    // Table tests (column-based, no word wrap)
    // -------------------------------------------------------------------------

    #[test]
    fn table_columns_computed_properly() {
        let md = "| Short | A much longer header |\n|---|---|\n| a | b |\n";
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, md, 80);
        let table_lines: Vec<String> = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();
        eprintln!("table_lines: {:?}", table_lines);
        let table_text = table_lines.join("\n");
        // Table should have separators
        assert!(table_text.contains("="), "Table should have separator");
        // Columns should contain the header text
        assert!(table_text.contains("Short"));
        // Note: table column calculation may truncate long headers
        assert!(
            table_text.contains("A much") || table_text.contains("longer"),
            "Table should contain header text"
        );
    }

    // -------------------------------------------------------------------------
    // Heading tests (no wrapping)
    // -------------------------------------------------------------------------

    #[test]
    fn heading_no_wrapping() {
        let md =
            "# This is a very long heading that goes on and on and on and should not be wrapped\n";
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, md, 40);
        // Heading should be on one line with prefix
        let heading_line = lines
            .iter()
            .find(|l| l.spans.iter().any(|s| s.content.starts_with("# ")))
            .expect("Should have heading");
        let heading_text: String = heading_line
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(heading_text.starts_with("# "));
        assert!(heading_text.contains("very long heading"));
    }

    // ==========================================================================
    // CORNER CASE TESTS
    // ==========================================================================

    #[test]
    fn corner_nested_brackets() {
        // Nested brackets should stay together on same line
        let spans = vec![Span::styled("x ([word]) y", Style::default())];
        let wrapped = wrap_spans(&spans, 8);
        let lines: Vec<String> = wrapped
            .iter()
            .map(|l| l.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        // Check that ([word]) appears on a SINGLE line, not split across lines
        assert!(
            lines.iter().any(|l| l.contains("([word])")),
            "Nested brackets should be on same line, got: {:?}",
            lines
        );
    }

    #[test]
    fn corner_double_brackets() {
        // Double brackets should stay together on same line
        let spans = vec![Span::styled("x ((word)) y", Style::default())];
        let wrapped = wrap_spans(&spans, 8);
        let lines: Vec<String> = wrapped
            .iter()
            .map(|l| l.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert!(
            lines.iter().any(|l| l.contains("((word))")),
            "Double brackets should be on same line, got: {:?}",
            lines
        );
    }

    #[test]
    fn corner_multiple_bracket_groups() {
        // Multiple bracket groups should each stay together on same line
        let spans = vec![Span::styled("x (a) (b) y", Style::default())];
        let wrapped = wrap_spans(&spans, 8);
        let lines: Vec<String> = wrapped
            .iter()
            .map(|l| l.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert!(
            lines.iter().any(|l| l.contains("(a)")),
            "(a) should be on same line"
        );
        assert!(
            lines.iter().any(|l| l.contains("(b)")),
            "(b) should be on same line"
        );
    }

    #[test]
    fn corner_empty_brackets() {
        // Empty brackets should stay together on same line
        let spans = vec![Span::styled("x () y", Style::default())];
        let wrapped = wrap_spans(&spans, 6);
        let lines: Vec<String> = wrapped
            .iter()
            .map(|l| l.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert!(
            lines.iter().any(|l| l.contains("()")),
            "Empty brackets should be on same line, got: {:?}",
            lines
        );
    }

    #[test]
    fn corner_unclosed_bracket() {
        // Unclosed opening bracket should stay with following content on same line
        let spans = vec![Span::styled("x (word more", Style::default())];
        let wrapped = wrap_spans(&spans, 8);
        let lines: Vec<String> = wrapped
            .iter()
            .map(|l| l.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert!(
            lines.iter().any(|l| l.contains("(word")),
            "Unclosed opening bracket should stay with word on same line, got: {:?}",
            lines
        );
    }

    #[test]
    fn corner_long_bracket_group_breaks_internally() {
        // Very long bracket group should break at internal spaces
        // but ( stays with first word, ) stays with last word
        let spans = vec![Span::styled("x (very long text) y", Style::default())];
        let wrapped = wrap_spans(&spans, 10);
        let lines: Vec<String> = wrapped
            .iter()
            .map(|l| l.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        // Opening ( should be on same line as "very"
        assert!(
            lines.iter().any(|l| l.contains("(very")),
            "Opening ( should stay with first word, got: {:?}",
            lines
        );
        // Closing ) should be on same line as "text"
        assert!(
            lines.iter().any(|l| l.contains("text)")),
            "Closing ) should stay with last word, got: {:?}",
            lines
        );
    }

    #[test]
    fn corner_bracket_across_spans_preserves_style() {
        // Test that styles are preserved when brackets span multiple spans
        let spans = vec![
            Span::styled("x (", Style::default()),
            Span::styled(
                "word",
                ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::styled(")", Style::default()),
        ];
        let wrapped = wrap_spans(&spans, 8);
        // Check that we have the right pieces
        let all_spans: Vec<(&str, bool)> = wrapped
            .iter()
            .flat_map(|line| {
                line.iter().map(|s| {
                    let is_bold = s
                        .style
                        .add_modifier
                        .contains(ratatui::style::Modifier::BOLD);
                    (s.content.as_ref(), is_bold)
                })
            })
            .collect();
        // 'word' should be bold, others should not
        assert!(
            all_spans
                .iter()
                .any(|(text, bold)| *text == "word" && *bold),
            "'word' should be bold, got: {:?}",
            all_spans
        );
        assert!(
            all_spans.iter().any(|(text, bold)| *text == "(" && !*bold),
            "'(' should not be bold"
        );
        assert!(
            all_spans.iter().any(|(text, bold)| *text == ")" && !*bold),
            "')' should not be bold"
        );
    }

    #[test]
    fn corner_square_brackets_stay_together() {
        // Square brackets should behave like parentheses
        let spans = vec![Span::styled("x [word] y", Style::default())];
        let wrapped = wrap_spans(&spans, 8);
        let lines: Vec<String> = wrapped
            .iter()
            .map(|l| l.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert!(
            lines.iter().any(|l| l.contains("[word]")),
            "Square brackets should be on same line, got: {:?}",
            lines
        );
    }

    #[test]
    fn corner_adjacent_bracket_groups() {
        // Adjacent bracket groups ([]) should stay together
        let spans = vec![Span::styled("x ([]) y", Style::default())];
        let wrapped = wrap_spans(&spans, 6);
        let lines: Vec<String> = wrapped
            .iter()
            .map(|l| l.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert!(
            lines.iter().any(|l| l.contains("([])")),
            "Adjacent bracket groups should be on same line, got: {:?}",
            lines
        );
    }
}
