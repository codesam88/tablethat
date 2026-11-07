use crate::ThemeConfig;
use comrak::nodes::{NodeCodeBlock, NodeValue};
use comrak::{Arena, Options, parse_document};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

/// Resolved styles for each markdown element, derived from [`ThemeConfig`].
pub struct MarkdownTheme {
    /// H1 headings (bold + underlined).
    pub h1: Style,
    /// H2 headings (underlined).
    pub h2: Style,
    /// H3+ headings.
    pub h3: Style,
    /// Strong text (modifier from `bold_style`).
    pub bold: Style,
    /// Emphasis text (modifier from `emphasis_style`).
    pub dim: Style,
    /// Inline code.
    pub code: Style,
    /// Fenced code blocks (never wrapped).
    pub code_block: Style,
}

/// Build render styles from config; unknown style names fall back to
/// bold (strong) / underlined (emphasis).
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

/// Drop a leading `---` YAML frontmatter block; pass text through unchanged.
/// Used so task metadata never renders as content.
pub fn strip_frontmatter(text: &str) -> &str {
    if let Some(rest) = text.trim_start().strip_prefix("---")
        && let Some(end) = rest.find("---")
    {
        return rest[end + 3..].trim_start();
    }
    text
}

/// Reflow styled spans to `max` columns, preserving style boundaries.
/// Break only at spaces or the `](` link seam (`]` stays left, `(` right);
/// bracket groups (`(word)`, `[word]`, nested) never split — depth is tracked
/// across spans so `(` from one span still guards `)` from another.
/// Full rule table lives in the test module below ("WORD WRAP RULES").
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
        if let Some(last) = pending.last_mut()
            && last.1 == style
            && !ch.is_control()
        {
            last.0.push(ch);
            return;
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
            } else if ch == '(' && pending.last().is_some_and(|last| last.0.ends_with(']')) {
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

pub(crate) fn comrak_options() -> Options<'static> {
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

/// Trim trailing whitespace from a wrapped line's spans (parity with wrap_lines).
fn trim_trailing_spaces(spans: Vec<Span<'static>>) -> Vec<Span<'static>> {
    let mut out = spans;
    while let Some(last) = out.last() {
        if last.content.is_empty() {
            out.pop();
        } else {
            break;
        }
    }
    if let Some(last) = out.last_mut() {
        let trimmed = last.content.trim_end();
        if trimmed.len() != last.content.len() {
            if trimmed.is_empty() {
                out.pop();
            } else {
                last.content = trimmed.to_string().into();
            }
        }
    }
    out
}

/// Patch a description term (if any) in a list item's body spans to the
/// verbatim/code color + bold. Returns the (possibly unchanged) spans.
fn highlight_desc_term(mut body: Vec<Span<'static>>, th: &MarkdownTheme) -> Vec<Span<'static>> {
    let body_text: String = body.iter().map(|s| s.content.as_ref()).collect();
    let chars: Vec<char> = body_text.chars().collect();

    // Find first separator colon: followed by space, or at end-of-content.
    let mut sep_idx: Option<usize> = None;
    for (i, c) in chars.iter().enumerate() {
        if *c == ':' {
            let next_ok = i + 1 == chars.len() || chars[i + 1] == ' ';
            if next_ok {
                sep_idx = Some(i);
                break;
            }
        }
    }

    let Some(si) = sep_idx else {
        return body;
    };

    // Term = trimmed text before separator.
    let before: &[char] = &chars[..si];
    let start = before
        .iter()
        .position(|c| !c.is_whitespace())
        .unwrap_or(before.len());
    let end = before
        .iter()
        .rposition(|c| !c.is_whitespace())
        .map(|i| i + 1)
        .unwrap_or(start);

    let term_len = end.saturating_sub(start);
    if term_len == 0 || term_len > 36 {
        return body;
    }

    let desc_style = th.code.add_modifier(Modifier::BOLD);

    // Patch spans overlapping [start, end).
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut offset = 0usize;
    for span in body.drain(..) {
        let content = span.content.as_ref();
        let span_chars: Vec<char> = content.chars().collect();
        let n = span_chars.len();
        let span_start = offset;
        let span_end = offset + n;
        offset = span_end;

        let ov_start = span_start.max(start);
        let ov_end = span_end.min(end);
        if ov_start >= ov_end {
            out.push(span);
            continue;
        }

        let local_ov_start = ov_start - span_start;
        let local_ov_end = ov_end - span_start;

        if local_ov_start > 0 {
            let before_str: String = span_chars[..local_ov_start].iter().collect();
            out.push(Span::styled(before_str, span.style));
        }

        let term_str: String = span_chars[local_ov_start..local_ov_end].iter().collect();
        let mut term_style = span.style;
        if let Some(fg) = desc_style.fg {
            term_style = term_style.fg(fg);
        }
        term_style = term_style.add_modifier(Modifier::BOLD);
        out.push(Span::styled(term_str, term_style));

        if local_ov_end < n {
            let after_str: String = span_chars[local_ov_end..].iter().collect();
            out.push(Span::styled(after_str, span.style));
        }
    }

    out
}

/// Display width of `s` in terminal columns (unicode-aware).
pub fn display_width(s: &str) -> usize {
    use unicode_width::UnicodeWidthStr;
    s.width()
}

/// Greedy whitespace wrap of a single table cell to `width` display columns.
///
/// Hard-breaks atoms wider than `width` char-wise (accumulating display width,
/// wide char = 2, never splitting a char). Empty input -> `vec![""]`.
/// Marker protection is intentionally omitted: `collect_text` does not preserve
/// backtick/bracket markers, so cells are plain text.
/// Trailing punctuation that must not start a wrapped line (orphan
/// prevention). Used when hard-breaking overlong words in table cells,
/// where breaks fall between characters with no space to break at: the
/// break point backs up over a trailing-punctuation run so `hello;`
/// never becomes `hello` / `;...`. Spaced runs (`word ;`) keep normal
/// break behavior — gluing those would need reflow. The `](` link seam
/// lives in `wrap_spans` and never reaches the hard-breaker as a unit.
fn is_trailing_punct(ch: char) -> bool {
    matches!(
        ch,
        ',' | ';' | ':' | '!' | '?' | '.' | '%' | ')' | ']' | '}' | '>' | '…'
    )
}

fn char_width(ch: char) -> usize {
    use unicode_width::UnicodeWidthChar;
    UnicodeWidthChar::width(ch).unwrap_or(0)
}

/// Hard-break an overlong word into `width`-column lines. Never starts a
/// continuation line with trailing punctuation (see `is_trailing_punct`):
/// the break backs up over the punct run instead, keeping box width
/// intact. If the atom itself starts with punctuation there is nothing
/// to glue to, so it is kept whole. Always makes progress.
fn break_atom(word: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let chars: Vec<(char, usize)> = word.chars().map(|c| (c, char_width(c).max(1))).collect();
    let mut lines = Vec::new();
    let mut start = 0usize;
    while start < chars.len() {
        let mut w = 0usize;
        let mut end = start;
        while end < chars.len() && w + chars[end].1 <= width {
            w += chars[end].1;
            end += 1;
        }
        if end == chars.len() {
            lines.push(chars[start..].iter().map(|(c, _)| *c).collect());
            break;
        }
        // Would break before chars[end]: back up over trailing punctuation
        // so the next line never starts with an orphan, keeping line 1
        // non-empty.
        let mut cut = end;
        while cut > start + 1 && is_trailing_punct(chars[cut].0) {
            cut -= 1;
        }
        lines.push(chars[start..cut].iter().map(|(c, _)| *c).collect());
        start = cut;
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Greedy whitespace wrap of a single table cell to `width` display columns.
///
/// Overlong words hard-break via `break_atom` (orphan-safe); empty input
/// yields `vec![""]`. Marker protection is intentionally omitted:
/// `collect_text` does not preserve backtick/bracket markers, so cells are
/// plain text.
pub fn wrap_cell(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let text = text.trim();
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0usize;

    for word in text.split_whitespace() {
        let ww = display_width(word);
        if cur.is_empty() {
            if ww <= width {
                cur.push_str(word);
                cur_w = ww;
            } else {
                // Overlong word on a fresh line: hard-break it, keeping
                // the last piece as the open line.
                let mut broken = break_atom(word, width);
                if let Some(last) = broken.pop() {
                    cur_w = display_width(&last);
                    cur = last;
                }
                lines.extend(broken);
            }
        } else if cur_w + 1 + ww <= width {
            cur.push(' ');
            cur.push_str(word);
            cur_w += 1 + ww;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur_w = 0;
            if ww <= width {
                cur.push_str(word);
                cur_w = ww;
            } else {
                // Overlong word after a flush: same hard-break path.
                let mut broken = break_atom(word, width);
                if let Some(last) = broken.pop() {
                    cur_w = display_width(&last);
                    cur = last;
                }
                lines.extend(broken);
            }
        }
    }

    if !cur.is_empty() || lines.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn pad_display(s: &str, width: usize) -> String {
    let dw = display_width(s);
    if dw >= width {
        s.to_string()
    } else {
        let mut out = s.to_string();
        out.push_str(&" ".repeat(width - dw));
        out
    }
}

fn render_table(
    node: &comrak::Node<'_>,
    lines: &mut Vec<Line<'static>>,
    ascii: bool,
    table_hi: usize,
) {
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
        natural: usize,
    }
    let mut cols: Vec<ColW> = Vec::new();
    for _ in 0..ncols {
        cols.push(ColW { min: 0, natural: 0 });
    }

    for row in &tbl {
        for (i, cell) in row.iter().enumerate() {
            let lw = cell
                .split_whitespace()
                .map(display_width)
                .max()
                .unwrap_or(display_width(cell));
            cols[i].min = cols[i].min.max(lw.min(40));
            // Uncapped max cell display width (natural content width).
            let cw = display_width(cell);
            if cw > cols[i].natural {
                cols[i].natural = cw;
            }
        }
    }

    let border_w = ncols * 2 + 1;
    let target = table_hi.saturating_sub(border_w);

    // Natural-start: size to content, never stretch.
    let mut col_w: Vec<usize> = cols.iter().map(|c| c.natural.max(c.min).max(1)).collect();

    // Shrink-only: if content overflows the budget, shrink columns proportionally
    // down to their minima. Never grow, never fill slack.
    // Each round either fits the budget or pins at least one column at its min,
    // so at most `ncols` rounds terminate (flex excludes pinned columns).
    for _ in 0..ncols {
        if col_w.iter().sum::<usize>() <= target {
            break;
        }
        let deficit = col_w.iter().sum::<usize>().saturating_sub(target);
        let flex: usize = col_w
            .iter()
            .zip(&cols)
            .map(|(&w, c)| w.saturating_sub(c.min))
            .sum();
        if flex == 0 {
            // All columns at min: mins win; table may overflow budget (accepted).
            break;
        }
        col_w = col_w
            .iter()
            .enumerate()
            .map(|(i, &w)| {
                let room = w.saturating_sub(cols[i].min);
                w.saturating_sub(deficit * room / flex.max(1))
                    .max(cols[i].min)
            })
            .collect();
    }

    // Collect non-empty rows (skip empties, as before).
    let rows: Vec<&Vec<String>> = tbl.iter().filter(|r| !r.is_empty()).collect();
    if rows.is_empty() {
        return;
    }

    let v = if ascii { "|" } else { "│" };

    // Build a horizontal border line from a (left, mid, right, fill) set.
    let hborder = |left: char, mid: char, right: char, fill: char| -> String {
        let mut s = String::from(left);
        for (i, &w) in col_w.iter().enumerate() {
            if i > 0 {
                s.push(mid);
            }
            s.push_str(&std::iter::repeat_n(fill, w + 1).collect::<String>());
        }
        s.push(right);
        s
    };

    // Top border.
    if ascii {
        lines.push(Line::from(hborder('+', '+', '+', '-')));
    } else {
        lines.push(Line::from(hborder('┌', '┬', '┐', '─')));
    }

    // Helper to emit a (possibly wrapped) data row.
    let emit_row = |row: &Vec<String>, lines: &mut Vec<Line<'static>>| {
        let mut cell_lines: Vec<Vec<String>> = Vec::new();
        for (i, &w) in col_w.iter().enumerate() {
            let raw = row.get(i).map(|s| s.as_str()).unwrap_or("");
            cell_lines.push(wrap_cell(raw, w));
        }
        let height = cell_lines.iter().map(|v| v.len()).max().unwrap_or(1).max(1);
        for li in 0..height {
            let mut buf = String::from(v);
            for (i, &w) in col_w.iter().enumerate() {
                let cell_text = cell_lines[i].get(li).map(|s| s.as_str()).unwrap_or("");
                buf.push(' ');
                buf.push_str(&pad_display(cell_text, w));
                buf.push_str(v);
            }
            lines.push(Line::from(buf));
        }
    };

    if rows.len() == 1 {
        // Single (header-only) row: top, row, bottom. No separator needed.
        emit_row(rows[0], lines);
        if ascii {
            lines.push(Line::from(hborder('+', '+', '+', '-')));
        } else {
            lines.push(Line::from(hborder('└', '┴', '┘', '─')));
        }
        return;
    }

    // >=2 rows: top, row0 (header), header-sep, row1, [body-sep, rowN...], bottom.
    emit_row(rows[0], lines);
    if ascii {
        lines.push(Line::from(hborder('+', '+', '+', '=')));
    } else {
        lines.push(Line::from(hborder('┝', '┿', '┥', '━')));
    }
    emit_row(rows[1], lines);
    for r in &rows[2..] {
        if ascii {
            lines.push(Line::from(hborder('+', '+', '+', '-')));
        } else {
            lines.push(Line::from(hborder('├', '┼', '┤', '─')));
        }
        emit_row(r, lines);
    }
    if ascii {
        lines.push(Line::from(hborder('+', '+', '+', '-')));
    } else {
        lines.push(Line::from(hborder('└', '┴', '┘', '─')));
    }
}

fn render_node(
    node: &comrak::Node<'_>,
    th: &MarkdownTheme,
    lines: &mut Vec<Line<'static>>,
    wrap: usize,
    table_hi: usize,
    ascii: bool,
) {
    let data = node.data.borrow();
    match &data.value {
        NodeValue::Document | NodeValue::FrontMatter(_) => {
            for child in node.children() {
                render_node(&child, th, lines, wrap, table_hi, ascii);
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
                render_node(&child, th, lines, wrap, table_hi, ascii);
            }
        }
        NodeValue::List(_list) => {
            for child in node.children() {
                render_node(&child, th, lines, wrap, table_hi, ascii);
            }
            lines.push(Line::from(""));
        }
        NodeValue::Item(_list) => {
            let mut body: Vec<Span<'static>> = Vec::new();
            render_inline(node, th, &mut body, Style::default());

            let body_text: String = body.iter().map(|s| s.content.as_ref()).collect();
            if body_text.trim().is_empty() {
                return;
            }

            // Description-term highlight: "- <desc>: <content>".
            let body = highlight_desc_term(body, th);

            let prefix = "  - ";
            let lw = wrap;
            let content_width = lw.saturating_sub(prefix.len()).max(1);
            let wrapped = wrap_spans(&body, content_width);
            for (i, line_spans) in wrapped.into_iter().enumerate() {
                let line_spans = trim_trailing_spaces(line_spans);
                if i == 0 {
                    let mut first = vec![Span::styled(prefix.to_string(), Style::default())];
                    first.extend(line_spans);
                    lines.push(Line::from(first));
                } else {
                    let mut indented = vec![Span::styled("    ".to_string(), Style::default())];
                    indented.extend(line_spans);
                    lines.push(Line::from(indented));
                }
            }
        }
        NodeValue::TaskItem(task) => {
            let checked = task.symbol == Some('x') || task.symbol == Some('X');
            let mut body: Vec<Span<'static>> = Vec::new();
            render_inline(node, th, &mut body, Style::default());

            let check_mark = if checked { "[x]" } else { "[ ]" };
            let prefix = format!("  - {} ", check_mark);
            let indent_sz = prefix.len();
            let indent = " ".repeat(indent_sz);

            let body_text: String = body.iter().map(|s| s.content.as_ref()).collect();
            let body = if body_text.trim().is_empty() {
                body
            } else {
                highlight_desc_term(body, th)
            };

            let lw = wrap;
            let content_width = lw.saturating_sub(prefix.len()).max(1);
            let wrapped = wrap_spans(&body, content_width);
            for (i, line_spans) in wrapped.into_iter().enumerate() {
                let line_spans = trim_trailing_spaces(line_spans);
                if i == 0 {
                    let mut first = vec![Span::styled(prefix.clone(), Style::default())];
                    first.extend(line_spans);
                    lines.push(Line::from(first));
                } else {
                    let mut indented = vec![Span::styled(indent.clone(), Style::default())];
                    indented.extend(line_spans);
                    lines.push(Line::from(indented));
                }
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
            render_table(node, lines, ascii, table_hi);
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
                render_node(&child, th, lines, wrap, table_hi, ascii);
            }
        }
    }
}

/// Render markdown to styled lines: strips frontmatter, parses with comrak
/// (tables, tasklists, strikethrough, autolink), wraps paragraphs to `wrap`
/// columns. Headings render on one line; code blocks are never word-wrapped;
/// table cells wrap to the computed column width (tables are NOT "never
/// wrapped"). See `wrap_spans` for bracket grouping rules.
///
/// ```
/// # use tablethat_lib::{ThemeConfig, markdown::{render_markdown, theme_from_cfg}};
/// let th = theme_from_cfg(&ThemeConfig::default());
/// let lines = render_markdown(&th, "# Hello\n", 80, false);
/// let text: String = lines.iter()
///     .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
///     .collect();
/// assert!(text.contains("# Hello"));
/// ```
pub fn render_markdown(
    th: &MarkdownTheme,
    text: &str,
    width: usize,
    ascii: bool,
) -> Vec<Line<'static>> {
    let prose_w = width.clamp(40, 120);
    let table_hi = width.clamp(60, 160);
    let body = strip_frontmatter(text);
    let arena = Arena::new();
    let opts = comrak_options();
    let root = parse_document(&arena, body, &opts);

    let mut lines: Vec<Line<'static>> = Vec::new();
    render_node(&root, th, &mut lines, prose_w, table_hi, ascii);
    lines
}
#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // WORD WRAP RULES (authoritative index)
    // ==========================================================================
    //
    // P1. Spaces break prose (wrap_spans): a space is always a safe break
    //     point; runs accumulate until width is exceeded, then flush.
    // P2. The "](" link seam splits deliberately: ']' stays with the
    //     preceding text, '(' stays with the following text (human-reader
    //     exception), so "[text](url)" can break between ']' and '('.
    // P3. Brackets never split individually: standalone '[', ']', '(', ')'
    //     are not break points on their own.
    // P4. Open bracket sticks forward: '(' / '[' bind to the following
    //     word, so "(word" stays together.
    // P5. Close bracket sticks back: ')' / ']' bind to the preceding word,
    //     so "word)" stays together.
    // P6. Groups/nesting stay whole, including across spans: "(word)",
    //     "[word]", "[text](url)", nested brackets; bracket depth is
    //     tracked across spans so a '(' from one span guards a ')' in
    //     another.
    // P7. Long groups break internally at spaces: a bracketed run longer
    //     than the width may wrap at its internal spaces, but the bracket
    //     pairing is preserved across the break.
    // P8. Orphan rule (table hard-break + abutting runs): a continuation
    //     line never STARTS with trailing punctuation
    //     (',', ';', ':', '!', '?', '.', '%', ')', ']', '}', '>', '…');
    //     break_atom backs the break up over the trailing-punct run. A
    //     balanced unit like "?hello?" stays whole; if the atom itself
    //     starts with punctuation it is kept whole (nothing to glue to).
    // P9. Balanced units stay whole: "?hello?"-style symmetric punctuation
    //     is never split across a break.
    //
    // T1. Tables size to natural content (compact, never stretch): column
    //     width starts at the natural (uncapped) content width.
    // T2. Content overflow shrinks proportionally to the exact budget:
    //     when natural width exceeds the budget, columns shrink toward
    //     their minima in proportion to their spare room.
    // T3. Min floor wins with accepted overflow: once every column is at
    //     its minimum and the table still exceeds the budget, the minimums
    //     win and the table may overflow (box stays intact).
    // T4. Cells wrap to column width: each cell's text is greedy-wrapped
    //     (wrap_cell) to its computed column width, including hard-break
    //     of overlong words via break_atom.
    // T5. Header/body separators only between logical rows: top border,
    //     header separator, one separator per additional body row, bottom
    //     border; no separator is emitted inside a wrapped cell.
    // T6. Every emitted table line has identical display width (box
    //     integrity), including wrapped rows and hard-broken words.
    // T7. ASCII vs unicode borders: ascii mode uses "+-|=" borders,
    //     unicode mode uses box-drawing borders; layout is identical.
    // T8. Ragged rows / empty cells: rows with fewer cells are padded;
    //     empty cells render as empty; column count is the max row width.
    // T9. Header-only tables: a single row renders as top + row + bottom
    //     with no separator.
    //
    // W1. Prose width clamp(40, 120): render_markdown clamps the prose
    //     wrap width to [40, 120] regardless of the requested width.
    // W2. Table cap clamp(60, 160): the table high-water mark is clamped
    //     to [60, 160].
    // W3. -w / terminal-minus-chrome base: the base width comes from -w or
    //     (terminal columns - chrome); see gloss::App::effective_width.
    // W4. Effective-width reservation: 3 columns reserved when a scrollbar
    //     is shown, 1 otherwise (gloss::App::effective_width). Covered by
    //     existing tests in src/gloss/tui.rs; not duplicated here.
    //
    // F1. Formatter split-only vs reflow: split-only wraps overlong lines
    //     without joining short ones; reflow joins then re-splits to width.
    // F2. Formatter fixpoint: formatting twice yields the same output.
    // F3. Formatter tables aligned, never wrapped: table rows are aligned
    //     from AST ranges and are not re-wrapped.
    // F4. Formatter verbatim frontmatter: frontmatter bytes are preserved
    //     unchanged (plan format path).
    // F5. Formatter skip-on-invalid-YAML: unparseable frontmatter causes
    //     the file to be skipped with a warning, not corrupted.
    // F6. Formatter hardbreak flavors: both two-space and backslash hard
    //     breaks are preserved (hardbreak-exempt trailing whitespace).
    //
    // KNOWN LIMITATION: spaced trailing punctuation in PROSE ("word ;")
    // may strand at a line start. The renderer performs no reflow, and the
    // orphan guarantee (P8) covers abutting runs ("word;") and table
    // hard-breaks only; spaced runs keep normal break behavior. Gluing them
    // would require a reflow pass.
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
    // 5. HEADINGS:
    //    - Single line, no wrapping
    //    - Prefix (#, ##, ###) included in line
    //
    // ==========================================================================

    #[test]
    fn table_renders_header_and_body() {
        let md = "| **Name** | `Code` |\n|---|---|\n| foo | bar |\n";
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, md, 80, false);
        let total: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(total.contains("Name"), "Missing header 'Name'");
        assert!(total.contains("foo"), "Missing body 'foo'");
        // Representative glyphs per class (corner/vertical/fill) only.
        for ch in ['┌', '│', '─', '━'] {
            assert!(total.contains(ch), "Missing border char {:?}", ch);
        }
        assert!(total.contains("Name"), "Missing header 'Name'");
        assert!(total.contains("foo"), "Missing body 'foo'");
        assert!(!total.contains('═'), "Double separator must be absent");
        // Unicode mode must not use ASCII pipes/corners.
        assert!(
            !total.contains('+'),
            "ascii corner must not render in unicode mode"
        );
    }

    #[test]
    fn table_renders_header_and_body_ascii() {
        let md = "| **Name** | `Code` |\n|---|---|\n| foo | bar |\n";
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, md, 80, true);
        let total: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(total.contains("Name"), "Missing header 'Name'");
        assert!(total.contains("foo"), "Missing body 'foo'");
        assert!(total.contains("+"), "Missing ascii corner/junction");
        assert!(total.contains("="), "Missing separator");
        assert!(total.contains("|"), "Missing vertical");
        // ASCII mode must not use unicode corners.
        assert!(
            !total.contains('┌'),
            "unicode corner must not render in ascii mode"
        );
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
    fn wrap_balanced_query_stays_whole() {
        // ?hello? has no internal spaces, so it is one atom and never splits.
        let spans = vec![Span::styled("say ?hello? loudly now", Style::default())];
        let wrapped = wrap_spans(&spans, 10);
        let lines: Vec<String> = wrapped
            .iter()
            .map(|l| l.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert!(
            lines.iter().any(|l| l.contains("?hello?")),
            "?hello? must stay whole: {lines:?}"
        );
    }

    #[test]
    fn wrap_cell_hard_break_backs_up_over_trailing_punct() {
        // "abcde;fgh" at width 5: a naive break lands right after "abcde"
        // and would strand ";" at the next line start. The break must back
        // up one char instead, keeping ";" glued to "e".
        let out = crate::markdown::wrap_cell("abcde;fgh", 5);
        assert_eq!(out, vec!["abcd".to_string(), "e;fgh".to_string()]);
        for line in out.iter().skip(1) {
            let first = line.chars().next().expect("continuation is non-empty");
            assert!(
                ![',', ';', ':', '!', '?', '.', '%', ')', ']', '}', '>', '…'].contains(&first),
                "continuation must not start with orphan punctuation: {line:?}"
            );
        }
        // Content is preserved across the break.
        assert_eq!(out.concat(), "abcde;fgh");
    }

    #[test]
    fn table_hard_break_never_orphans_punct() {
        // 88-char atom with ";" at break position (width 60 -> col 57):
        // naive hard-break strands ";" at a continuation start. The table
        // must back the break up instead, keeping the box intact.
        let atom = format!("{}hello;{}", "x".repeat(52), "y".repeat(30));
        let md = format!("| T |\n| --- |\n| {} |\n", atom);
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, &md, 60, false);
        let strs: Vec<String> = lines
            .iter()
            .map(|l| l.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        use unicode_width::UnicodeWidthStr;
        let table: Vec<&String> = strs
            .iter()
            .filter(|l| l.contains('│') || l.contains('┌') || l.contains('└'))
            .collect();
        assert!(!table.is_empty(), "expected table lines: {strs:?}");
        for l in &table {
            assert_eq!(l.width(), 60, "box must stay intact: {l:?}");
            let content = l.trim_matches(|c| c == '│' || c == ' ').trim_start();
            if let Some(first) = content.chars().next() {
                assert!(
                    ![',', ';', ':', '!', '?', '.', '%', ')', ']', '}', '>', '…'].contains(&first),
                    "table continuation must not start with orphan: {l:?}"
                );
            }
        }
        // The glued pair survives on one line (not split across the break).
        assert!(
            table.iter().any(|l| l.contains("o;")),
            "';' must stay glued to preceding char: {table:?}"
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
        let lines = render_markdown(&th, md, 8, false);
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
        let lines = render_markdown(&th, md, 7, false);
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
        let lines = render_markdown(&th, md, 40, false);
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
        let lines = render_markdown(&th, md, 40, false);
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
    // Table tests (column-based, cells wrap)
    // -------------------------------------------------------------------------

    #[test]
    fn table_columns_computed_properly() {
        let md = "| Short | A much longer header |\n|---|---|\n| a | b |\n";
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, md, 80, false);
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
        assert!(table_text.contains("━"), "Table should have separator");
        // Columns should contain the header text
        assert!(table_text.contains("Short"));
        // Note: table column calculation may truncate long headers
        assert!(
            table_text.contains("A much") || table_text.contains("longer"),
            "Table should contain header text"
        );
    }

    #[test]
    fn table_columns_computed_properly_ascii() {
        let md = "| Short | A much longer header |\n|---|---|\n| a | b |\n";
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, md, 80, true);
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
    // Table wrapping tests
    // -------------------------------------------------------------------------

    #[test]
    fn table_cell_long_wraps_no_truncation() {
        let md = "| a | this is a very long cell that should wrap instead of being truncated |
|---|---|
| x | y |
";
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, md, 40, false);
        let table_text: String = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        for w in [
            "this",
            "very",
            "long",
            "cell",
            "should",
            "wrap",
            "instead",
            "truncated",
        ] {
            assert!(table_text.contains(w), "missing word {w}");
        }
        // No truncation marker: full word 'truncated' present (not 'truncat' only).
        assert!(table_text.contains("truncated"));
    }

    #[test]
    fn table_box_integrity_uniform_width() {
        let md = "| a | this is a very long cell that wraps |
|---|---|
| x | y |
";
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, md, 40, false);
        use unicode_width::UnicodeWidthStr;
        let strs: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        // Find table block: from first line containing border corner to last.
        let start = strs
            .iter()
            .position(|l| l.contains('┌') || l.contains('+'))
            .unwrap();
        let end = strs
            .iter()
            .rposition(|l| l.contains('└') || l.contains('+'))
            .unwrap();
        let widths: Vec<usize> = strs[start..=end].iter().map(|l| l.width()).collect();
        let first = widths[0];
        for (i, w) in widths.iter().enumerate() {
            assert_eq!(
                *w,
                first,
                "line {} width {} != {} : {:?}",
                start + i,
                w,
                first,
                strs[start + i]
            );
        }
    }

    #[test]
    fn table_separators_only_between_logical_rows() {
        let md = "| h1 | h2 |
|---|---|
| a | this is a long wrapped cell line |
| b | c |
";
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, md, 40, false);
        let strs: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        // Count horizontal separator lines (top/header/body/bottom borders).
        let sep_count = strs
            .iter()
            .filter(|l| {
                let t = l.trim();
                !t.is_empty() && t.chars().all(|c| "┌┬┐├┼┤└┴┘┝┿┥─━=+-".contains(c))
            })
            .count();
        // header + 2 body rows => top + header-sep + 1 body-sep + bottom = 4
        assert_eq!(sep_count, 4, "got lines: {:#?}", strs);
    }

    #[test]
    fn table_overlong_url_hard_breaks() {
        let md = "| a | http://example.com/this/is/a/very/long/url/without/spaces |
|---|---|
| x | y |
";
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, md, 40, false);
        let strs: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        // box integrity
        let start = strs
            .iter()
            .position(|l| l.contains('┌') || l.contains('+'))
            .unwrap();
        let end = strs
            .iter()
            .rposition(|l| l.contains('└') || l.contains('+'))
            .unwrap();
        let widths: Vec<usize> = strs[start..=end]
            .iter()
            .map(|l| {
                use unicode_width::UnicodeWidthStr;
                l.width()
            })
            .collect();
        let first = widths[0];
        for w in &widths {
            assert_eq!(*w, first);
        }
        let table_text = strs.join("\n");
        assert!(table_text.contains("http"));
    }

    #[test]
    fn table_cjk_cell_wraps_aligned() {
        let md = "| a | 你好世界这是一个很长的单元格内容 |
|---|---|
| x | y |
";
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, md, 40, false);
        let strs: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        let start = strs
            .iter()
            .position(|l| l.contains('┌') || l.contains('+'))
            .unwrap();
        let end = strs
            .iter()
            .rposition(|l| l.contains('└') || l.contains('+'))
            .unwrap();
        let widths: Vec<usize> = strs[start..=end]
            .iter()
            .map(|l| {
                use unicode_width::UnicodeWidthStr;
                l.width()
            })
            .collect();
        let first = widths[0];
        for w in &widths {
            assert_eq!(*w, first);
        }
    }

    #[test]
    fn table_narrow_fits_budget_unchanged() {
        // Compact contract: a narrow table sizes to its natural content
        // width instead of stretching. Naturals [4, 4], overhead 2*2+1=5,
        // so every table line is exactly 4+4+5 = 13 columns.
        let md = "| Name | Role |
|---|---|
| Ada | Eng |
";
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, md, 80, false);
        let strs: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        eprintln!("narrow: {:#?}", strs);
        use unicode_width::UnicodeWidthStr;
        let table_widths: Vec<usize> = strs
            .iter()
            .filter(|l| {
                l.contains('┌')
                    || l.contains('│')
                    || l.contains('└')
                    || l.contains('├')
                    || l.contains('┝')
            })
            .map(|l| l.width())
            .collect();
        for w in &table_widths {
            assert_eq!(*w, 13, "narrow table must stay compact");
        }
        assert!(strs.iter().any(|l| l.contains("Name")));
        assert!(strs.iter().any(|l| l.contains("Ada")));
    }

    #[test]
    fn table_fills_budget() {
        // Over-budget tables still fill the width exactly (shrink-to-fit).
        let md = "| Name | Description |\n|---|---|\n| Ada | Engineer who loves tables and writes very long descriptions about everything |\n| Bob | Designer |\n";
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, md, 80, false);
        let strs: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        eprintln!("fills: {:#?}", strs);
        use unicode_width::UnicodeWidthStr;
        let start = strs
            .iter()
            .position(|l| l.contains('┌') || l.contains('+'))
            .expect("top border");
        let end = strs
            .iter()
            .rposition(|l| l.contains('└') || l.contains('+'))
            .expect("bottom border");
        for (i, l) in strs[start..=end].iter().enumerate() {
            assert_eq!(
                l.width(),
                80,
                "table line {} width {} != 80: {:?}",
                start + i,
                l.width(),
                l
            );
        }
    }

    // -------------------------------------------------------------------------
    // Width policy tests
    // -------------------------------------------------------------------------

    #[test]
    fn width_policy_table_max_respects_cap() {
        use unicode_width::UnicodeWidthStr;
        // A table wide enough to exceed the 160 cap.
        let md = "| aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa | bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb | cccccccccccccccccccccccccccccccc | dddddddddddddddddddddddddddddddd |\n|---|---|---|---|\n| x | y | z | w |\n";
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, md, 200, false);
        let max_w = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
                    .width()
            })
            .max()
            .unwrap_or(0);
        assert!(
            max_w <= 160,
            "table at width 200 must respect 160 cap, got {max_w}"
        );
    }

    #[test]
    fn width_policy_prose_clamps_high() {
        use unicode_width::UnicodeWidthStr;
        let md = "this is a long paragraph that should wrap according to the clamped prose width policy and not exceed one hundred twenty columns even when a huge width is requested\n";
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, md, 200, false);
        let max_w = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
                    .width()
            })
            .max()
            .unwrap_or(0);
        assert!(
            max_w <= 120,
            "prose at width 200 must clamp to 120, got {max_w}"
        );
    }

    #[test]
    fn width_policy_prose_clamps_low() {
        use unicode_width::UnicodeWidthStr;
        let md = "this is a long paragraph that should wrap at forty columns rather than the legacy seventy eight when a tiny width is requested by the caller\n";
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let lines = render_markdown(&th, md, 30, false);
        let max_w = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
                    .width()
            })
            .max()
            .unwrap_or(0);
        assert!(
            max_w >= 35 && max_w <= 40,
            "prose at width 30 must clamp to 40 (not 30 or 78), got {max_w}"
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
        let lines = render_markdown(&th, md, 40, false);
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

    // ---------------------------------------------------------------------
    // Description-term highlighting for bullet lists
    // ---------------------------------------------------------------------

    fn desc_style(th: &MarkdownTheme) -> Style {
        th.code.add_modifier(Modifier::BOLD)
    }

    #[test]
    fn desc_highlight_basic_bullet() {
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let md = "- Foo: bar baz\n";
        let lines = render_markdown(&th, md, 80, false);
        let line = &lines[0];
        let foo = line
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "Foo")
            .expect("term span");
        assert_eq!(foo.style.fg, desc_style(&th).fg);
        assert!(foo.style.add_modifier.contains(Modifier::BOLD));
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "  - Foo: bar baz");
    }

    #[test]
    fn desc_highlight_checkbox_variant() {
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let md = "- [x] Foo: bar\n";
        let lines = render_markdown(&th, md, 80, false);
        let line = &lines[0];
        let foo = line
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "Foo")
            .expect("term span");
        assert_eq!(foo.style.fg, desc_style(&th).fg);
        assert!(foo.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn desc_highlight_overlong_plain() {
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let term = "a".repeat(37);
        let md = format!("- {}: content\n", term);
        let lines = render_markdown(&th, &md, 80, false);
        let line = &lines[0];
        // No span should carry desc (code) fg + bold.
        assert!(
            !line.spans.iter().any(|s| s.style.fg == desc_style(&th).fg
                && s.style.add_modifier.contains(Modifier::BOLD)),
            "overlong term must not be highlighted"
        );
    }

    #[test]
    fn desc_highlight_no_separator_plain() {
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let md = "- foo bar baz\n";
        let lines = render_markdown(&th, md, 80, false);
        let line = &lines[0];
        // no span should carry code fg from desc highlighting
        assert!(
            !line.spans.iter().any(|s| s.style.fg == desc_style(&th).fg
                && s.style.add_modifier.contains(Modifier::BOLD)),
            "no desc highlight expected"
        );
    }

    #[test]
    fn desc_highlight_bold_term_composition() {
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let md = "- **Foo**: bar\n";
        let lines = render_markdown(&th, md, 80, false);
        let line = &lines[0];
        let foo = line
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "Foo")
            .expect("term span");
        assert_eq!(foo.style.fg, desc_style(&th).fg);
        assert!(foo.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn desc_highlight_link_at_start_preserves_underline() {
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let md = "- [text](http://example.com): content\n";
        let lines = render_markdown(&th, md, 80, false);
        let line = &lines[0];
        let t = line
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "text")
            .expect("term span");
        assert_eq!(t.style.fg, desc_style(&th).fg);
        assert!(t.style.add_modifier.contains(Modifier::BOLD));
        assert!(t.style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn desc_highlight_nested_colon_first_wins() {
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let md = "- a: b: c\n";
        let lines = render_markdown(&th, md, 80, false);
        let line = &lines[0];
        let a = line
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "a")
            .expect("term span");
        assert_eq!(a.style.fg, desc_style(&th).fg);
        assert!(a.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn desc_highlight_empty_content() {
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let md = "- desc:\n";
        let lines = render_markdown(&th, md, 80, false);
        let line = &lines[0];
        let d = line
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "desc")
            .expect("term span");
        assert_eq!(d.style.fg, desc_style(&th).fg);
        assert!(d.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn desc_highlight_url_time_colons_no_match() {
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let md = "- note: see https://example.com at 12:30\n";
        let lines = render_markdown(&th, md, 80, false);
        let line = &lines[0];
        let note = line
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "note")
            .expect("term span");
        assert_eq!(note.style.fg, desc_style(&th).fg);
        // url/time colon should not create another highlighted term
        assert!(
            !line
                .spans
                .iter()
                .any(|s| s.content.as_ref() == "example.com" && s.style.fg == desc_style(&th).fg)
        );
    }

    #[test]
    fn desc_highlight_url_only_plain() {
        let th = theme_from_cfg(&crate::ThemeConfig::default());
        let md = "- http://example.com\n";
        let lines = render_markdown(&th, md, 80, false);
        let line = &lines[0];
        assert!(
            !line.spans.iter().any(|s| s.style.fg == desc_style(&th).fg
                && s.style.add_modifier.contains(Modifier::BOLD)),
            "no desc highlight expected"
        );
    }

    mod wrap_rules_matrix {
        use super::*;
        use unicode_width::UnicodeWidthStr;

        #[derive(Copy, Clone)]
        struct Case {
            rule_id: &'static str,
            name: &'static str,
            md: &'static str,
            width: usize,
            ascii: bool,
            reflow: bool,
            format: bool,
            must_contain: &'static [&'static str],
            must_not_start_line: &'static [char],
            exact_total_width: Option<usize>,
            max_line_width: Option<usize>,
            exact_lines: Option<&'static [&'static str]>,
            check: Option<fn(&Case, &[String])>,
        }

        // ---- check helpers ----

        fn check_fixpoint(case: &Case, _lines: &[String]) {
            let out1 = crate::markdown_fmt::format_commonmark(case.md, case.width, case.reflow);
            let out2 = crate::markdown_fmt::format_commonmark(&out1, case.width, case.reflow);
            assert_eq!(out1, out2, "fixpoint failed for {}", case.rule_id);
        }

        fn check_html_eq(case: &Case, _lines: &[String]) {
            let opts = crate::markdown::comrak_options();
            let output = crate::markdown_fmt::format_commonmark(case.md, case.width, case.reflow);
            let a = comrak::markdown_to_html(case.md, &opts);
            let b = comrak::markdown_to_html(&output, &opts);
            let norm = |x: &str| -> String { x.replace('\n', " ") };
            assert_eq!(
                norm(&a),
                norm(&b),
                "HTML must be preserved for {}",
                case.rule_id
            );
        }

        fn check_table_lines_equal(case: &Case, lines: &[String]) {
            let table_lines: Vec<&String> = lines
                .iter()
                .filter(|l| {
                    l.contains('│')
                        || l.contains('┌')
                        || l.contains('└')
                        || l.contains('├')
                        || l.contains('┝')
                        || l.contains('+')
                        || l.contains('|')
                })
                .collect();
            assert!(
                !table_lines.is_empty(),
                "rule {} ({}): no table lines",
                case.rule_id,
                case.name
            );
            let first = table_lines[0].width();
            for l in &table_lines {
                assert_eq!(
                    l.width(),
                    first,
                    "rule {} ({}): box integrity broken: {:?}",
                    case.rule_id,
                    case.name,
                    l
                );
            }
            // overflow accepted: may exceed budget; just ensure consistent.
            let _ = first;
        }

        fn check_narrow_compact(case: &Case, lines: &[String]) {
            let table_lines: Vec<&String> = lines
                .iter()
                .filter(|l| {
                    l.contains('┌')
                        || l.contains('│')
                        || l.contains('└')
                        || l.contains('+')
                        || l.contains('|')
                })
                .collect();
            let w = table_lines.iter().map(|l| l.width()).max().unwrap_or(0);
            assert!(
                w < case.width,
                "rule {} ({}): compact table {} not < budget {}",
                case.rule_id,
                case.name,
                w,
                case.width
            );
        }

        fn check_separators(case: &Case, lines: &[String]) {
            let sep_count = lines
                .iter()
                .filter(|l| {
                    let t = l.trim();
                    !t.is_empty() && t.chars().all(|c| "┌┬┐├┼┤└┴┘┝┿┥─━=+-".contains(c))
                })
                .count();
            // header + 2 body rows => 4
            assert_eq!(
                sep_count, 4,
                "rule {} ({}): sep count {} != 4: {:#?}",
                case.rule_id, case.name, sep_count, lines
            );
        }

        fn check_header_only_separators(case: &Case, lines: &[String]) {
            let sep_count = lines
                .iter()
                .filter(|l| {
                    let t = l.trim();
                    !t.is_empty() && t.chars().all(|c| "┌┬┐├┼┤└┴┘┝┿┥─━=+-".contains(c))
                })
                .count();
            assert_eq!(
                sep_count, 2,
                "rule {} ({}): header-only sep count {} != 2: {:#?}",
                case.rule_id, case.name, sep_count, lines
            );
        }

        fn check_ragged(case: &Case, lines: &[String]) {
            check_table_lines_equal(case, lines);
            let text = lines.join("\n");
            assert!(
                text.contains("Ada") || text.contains("empty") || text.contains("a"),
                "rule {} ({}): content missing",
                case.rule_id,
                case.name
            );
        }

        fn check_p2_seam(case: &Case, lines: &[String]) {
            let joined = lines.join("\n");
            // The seam must have been a break opportunity: ']' and '(' present.
            assert!(
                joined.contains(']'),
                "rule {} ({}): missing ']'",
                case.rule_id,
                case.name
            );
            assert!(
                joined.contains('('),
                "rule {} ({}): missing '('",
                case.rule_id,
                case.name
            );
            // No line should contain the unsplit seam if a break occurred.
            // (If everything fits on one line this is vacuous; ensure at least one break happened.)
            if lines.len() > 1 {
                for l in lines {
                    assert!(
                        !l.contains("]("),
                        "rule {} ({}): seam not split: {:?}",
                        case.rule_id,
                        case.name,
                        l
                    );
                }
            }
        }

        fn check_balanced_whole(case: &Case, lines: &[String]) {
            let joined = lines.join("\n");
            assert!(
                joined.contains("?hello?"),
                "rule {} ({}): balanced unit broken: {:?}",
                case.rule_id,
                case.name,
                lines
            );
        }

        fn check_p4_open(case: &Case, lines: &[String]) {
            // open bracket should not be stranded at end of a line alone
            for l in lines {
                let t = l.trim_end();
                assert!(
                    !t.ends_with('(') && !t.ends_with('['),
                    "rule {} ({}): open bracket stranded: {:?}",
                    case.rule_id,
                    case.name,
                    l
                );
            }
        }

        fn check_p5_close(case: &Case, lines: &[String]) {
            // close bracket should not start a line
            for l in lines {
                let t = l.trim_start();
                if !t.is_empty() {
                    let first = t.chars().next().unwrap();
                    assert!(
                        ![')', ']', '}'].contains(&first),
                        "rule {} ({}): close bracket starts line: {:?}",
                        case.rule_id,
                        case.name,
                        l
                    );
                }
            }
        }

        fn check_t1_less(case: &Case, lines: &[String]) {
            check_narrow_compact(case, lines);
        }

        fn check_f1_split(case: &Case, lines: &[String]) {
            check_fixpoint(case, lines);
            check_html_eq(case, lines);
            // short line preserved as its own line
            assert!(
                lines.iter().any(|l| l.trim() == "short line"),
                "rule {} ({}): short line not preserved: {:?}",
                case.rule_id,
                case.name,
                lines
            );
        }

        fn check_f6_hardbreak(case: &Case, lines: &[String]) {
            check_fixpoint(case, lines);
            check_html_eq(case, lines);
            // two distinct lines survive
            assert!(
                lines.len() >= 2,
                "rule {} ({}): hardbreak collapsed: {:?}",
                case.rule_id,
                case.name,
                lines
            );
        }

        const CASES: &[Case] = &[
            // P1
            Case {
                rule_id: "P1", name: "spaces_break_prose",
                md: "this is a long paragraph that must wrap at spaces rather than mid word when rendered at a modest width
",
                width: 40, ascii: false, reflow: false, format: false,
                must_contain: &["this", "paragraph", "wrap", "spaces"],
                must_not_start_line: &[' '],
                exact_total_width: None, max_line_width: Some(40), exact_lines: None,
                check: None,
            },
            // P2
            Case {
                rule_id: "P2", name: "seam_splits",
                md: "prefix `x](http://example.com/very/long/url/path/that/forces/break)` suffix after and more trailing words\n",
                width: 40, ascii: false, reflow: false, format: false,
                must_contain: &["prefix", "suffix"],
                must_not_start_line: &[],
                exact_total_width: None, max_line_width: None, exact_lines: None,
                check: Some(check_p2_seam),
            },
            // P3
            Case {
                rule_id: "P3", name: "standalone_brackets_not_breaks",
                md: "words with [brackets] and (parens) inside text
",
                width: 50, ascii: false, reflow: false, format: false,
                must_contain: &["[brackets]", "(parens)"],
                must_not_start_line: &[],
                exact_total_width: None, max_line_width: Some(50), exact_lines: None,
                check: None,
            },
            // P4
            Case {
                rule_id: "P4", name: "open_bracket_sticks_forward",
                md: "prefix words and then (important) trailing words to force wrapping across the line
",
                width: 30, ascii: false, reflow: false, format: false,
                must_contain: &["(important)"],
                must_not_start_line: &[],
                exact_total_width: None, max_line_width: Some(40), exact_lines: None,
                check: Some(check_p4_open),
            },
            // P5
            Case {
                rule_id: "P5", name: "close_bracket_sticks_back",
                md: "prefix words then value) and more trailing words to force wrapping across the line
",
                width: 30, ascii: false, reflow: false, format: false,
                must_contain: &["value)"],
                must_not_start_line: &[')', ']', '}'],
                exact_total_width: None, max_line_width: Some(40), exact_lines: None,
                check: Some(check_p5_close),
            },
            // P6
            Case {
                rule_id: "P6", name: "cross_span_group_stays_whole",
                md: "x (input)
",
                width: 8, ascii: false, reflow: false, format: false,
                must_contain: &["(input)"],
                must_not_start_line: &[],
                exact_total_width: None, max_line_width: None, exact_lines: Some(&["x (input)", ""]),
                check: None,
            },
            // P7
            Case {
                rule_id: "P7", name: "long_group_breaks_internally",
                md: "here is a (very long parenthetical phrase that should wrap internally at spaces) after
",
                width: 30, ascii: false, reflow: false, format: false,
                must_contain: &["very", "parenthetical", "wrap", "internally"],
                must_not_start_line: &[],
                exact_total_width: None, max_line_width: Some(40), exact_lines: None,
                check: None,
            },
            // P8 (balanced)
            Case {
                rule_id: "P8", name: "balanced_unit_whole",
                md: "prefix words say ?hello? loudly now and many more words to force a wrap around the balanced unit
",
                width: 40, ascii: false, reflow: false, format: false,
                must_contain: &["?hello?"],
                must_not_start_line: &[],
                exact_total_width: None, max_line_width: Some(40), exact_lines: None,
                check: Some(check_balanced_whole),
            },
            // P9
            Case {
                rule_id: "P9", name: "balanced_units_stay_whole",
                md: "prefix words ?hello? suffix words and more trailing content to force wrapping in the paragraph
",
                width: 40, ascii: false, reflow: false, format: false,
                must_contain: &["?hello?"],
                must_not_start_line: &[],
                exact_total_width: None, max_line_width: Some(40), exact_lines: None,
                check: Some(check_balanced_whole),
            },
            // T1
            Case {
                rule_id: "T1", name: "compact_natural",
                md: "| Name | Role |\n|---|---|\n| Ada | Eng |\n",
                width: 80, ascii: false, reflow: false, format: false,
                must_contain: &["Name", "Ada"],
                must_not_start_line: &[],
                exact_total_width: Some(13), max_line_width: None, exact_lines: None,
                check: Some(check_t1_less),
            },
            // T2
            Case {
                rule_id: "T2", name: "overbudget_fills_exact",
                md: "| Name | Description |\n|---|---|\n| Ada | Engineer who loves tables and writes very long descriptions about everything |\n| Bob | Designer |\n",
                width: 80, ascii: false, reflow: false, format: false,
                must_contain: &["Ada", "Designer"],
                must_not_start_line: &[],
                exact_total_width: Some(80), max_line_width: None, exact_lines: None,
                check: None,
            },
            // T6
            Case {
                rule_id: "T6", name: "separators_count",
                md: "| h1 | h2 |\n|---|---|\n| a | this is a long wrapped cell line |\n| b | c |\n",
                width: 40, ascii: false, reflow: false, format: false,
                must_contain: &["h1", "h2"],
                must_not_start_line: &[],
                exact_total_width: None, max_line_width: None, exact_lines: None,
                check: Some(check_separators),
            },
            // T7 ascii
            Case {
                rule_id: "T7", name: "ascii_borders",
                md: "| a | b |\n|---|---|\n| x | y |\n",
                width: 40, ascii: true, reflow: false, format: false,
                must_contain: &["+"],
                must_not_start_line: &[],
                exact_total_width: None, max_line_width: None, exact_lines: None,
                check: None,
            },
            // T7 unicode
            Case {
                rule_id: "T7", name: "unicode_borders",
                md: "| a | b |\n|---|---|\n| x | y |\n",
                width: 40, ascii: false, reflow: false, format: false,
                must_contain: &["┌"],
                must_not_start_line: &[],
                exact_total_width: None, max_line_width: None, exact_lines: None,
                check: None,
            },
            // T8
            Case {
                rule_id: "T8", name: "ragged_empty_cells",
                md: "| a | b | c |\n|---|---|---|\n| x |  | z |\n| p | q |\n",
                width: 40, ascii: false, reflow: false, format: false,
                must_contain: &["x", "z", "p", "q"],
                must_not_start_line: &[],
                exact_total_width: None, max_line_width: None, exact_lines: None,
                check: Some(check_ragged),
            },
            // T9
            Case {
                rule_id: "T9", name: "header_only",
                md: "| Name | Role |\n|---|---|\n",
                width: 40, ascii: false, reflow: false, format: false,
                must_contain: &["Name", "Role"],
                must_not_start_line: &[],
                exact_total_width: None, max_line_width: None, exact_lines: None,
                check: Some(check_header_only_separators),
            },
            // W1
            Case {
                rule_id: "W1", name: "prose_cap_high",
                md: "this is a long paragraph that should wrap according to the clamped prose width policy and not exceed one hundred twenty columns even when a huge width is requested
",
                width: 200, ascii: false, reflow: false, format: false,
                must_contain: &["paragraph"],
                must_not_start_line: &[],
                exact_total_width: None, max_line_width: Some(120), exact_lines: None,
                check: None,
            },
            // W2
            Case {
                rule_id: "W2", name: "table_cap_high",
                md: "| aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa | bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb | cccccccccccccccccccccccccccccccc | dddddddddddddddddddddddddddddddd |\n|---|---|---|---|\n| x | y | z | w |\n",
                width: 200, ascii: false, reflow: false, format: false,
                must_contain: &["x"],
                must_not_start_line: &[],
                exact_total_width: None, max_line_width: Some(160), exact_lines: None,
                check: None,
            },
            // F1
            Case {
                rule_id: "F1", name: "split_only_preserves_short",
                md: "short line\nthis is a very long line that definitely exceeds the configured width and must be split into multiple lines\n",
                width: 40, ascii: false, reflow: false, format: true,
                must_contain: &["short line"],
                must_not_start_line: &[],
                exact_total_width: None, max_line_width: None, exact_lines: None,
                check: Some(check_f1_split),
            },
            // F6
            Case {
                rule_id: "F6", name: "hardbreak_flavors",
                md: "line one  \nline two\\\nline three\n",
                width: 80, ascii: false, reflow: false, format: true,
                must_contain: &["line one", "line two", "line three"],
                must_not_start_line: &[],
                exact_total_width: None, max_line_width: None, exact_lines: None,
                check: Some(check_f6_hardbreak),
            },
        ];

        // Fix up placeholder md fields that need computed strings.
        // (P8 table orphan and T3 overflow use runtime-built md; handled in a
        // separate test below.)

        #[test]
        fn wrap_rules_matrix() {
            let th = theme_from_cfg(&crate::ThemeConfig::default());
            // Build the two runtime-md cases and run them inline.
            // P8 table orphan
            {
                let atom = format!("{}hello;{}", "x".repeat(52), "y".repeat(30));
                let md = format!("| T |\n| --- |\n| {} |\n", atom);
                let lines = render_markdown(&th, &md, 60, false);
                let strs: Vec<String> = lines
                    .iter()
                    .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
                    .collect();
                for l in &strs {
                    let t = l.trim_start();
                    let content = t.trim_start_matches(|c| c == '│' || c == '|' || c == ' ');
                    if let Some(first) = content.chars().next() {
                        assert!(
                            ![',', ';', ':', '!', '?', '.', '%', ')', ']', '}', '>', '…']
                                .contains(&first),
                            "P8 table orphan start: {:?}",
                            l
                        );
                    }
                }
                let joined = strs.join("\n");
                assert!(joined.contains("o;"), "P8 glued pair missing: {strs:?}");
            }
            // T3 overflow
            {
                let long = "abcdefghijklmnopqrstuvwxyz".repeat(3);
                let md = format!("| a | {} |\n|---|---|\n| x | y |\n", long);
                let lines = render_markdown(&th, &md, 40, false);
                let strs: Vec<String> = lines
                    .iter()
                    .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
                    .collect();
                let table_lines: Vec<&String> = strs
                    .iter()
                    .filter(|l| {
                        l.contains('│')
                            || l.contains('┌')
                            || l.contains('└')
                            || l.contains('+')
                            || l.contains('|')
                    })
                    .collect();
                let first = table_lines[0].width();
                for l in &table_lines {
                    assert_eq!(l.width(), first, "T3 box integrity: {:?}", l);
                }
                assert!(first > 40, "T3 expected overflow, got {}", first);
                let joined = strs.join("\n");
                assert!(joined.contains("abc"), "T3 content missing");
            }

            for case in CASES {
                // Skip the two placeholder rows whose md is empty.
                if case.md.is_empty() {
                    continue;
                }
                let strs: Vec<String> = if case.format {
                    let out =
                        crate::markdown_fmt::format_commonmark(case.md, case.width, case.reflow);
                    out.trim_end_matches('\n')
                        .split('\n')
                        .map(String::from)
                        .collect()
                } else {
                    let lines = render_markdown(&th, case.md, case.width, case.ascii);
                    lines
                        .iter()
                        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
                        .collect()
                };

                for needle in case.must_contain {
                    let joined = strs.join("\n");
                    assert!(
                        joined.contains(needle),
                        "rule {} ({}): missing {:?}: {:#?}",
                        case.rule_id,
                        case.name,
                        needle,
                        strs
                    );
                }

                for line in &strs {
                    if let Some(first) = line.chars().next() {
                        assert!(
                            !case.must_not_start_line.contains(&first),
                            "rule {} ({}): line starts with orphan {:?}: {:?}",
                            case.rule_id,
                            case.name,
                            first,
                            line
                        );
                    }
                }

                if let Some(w) = case.exact_total_width {
                    let table_lines: Vec<&String> = strs
                        .iter()
                        .filter(|l| {
                            l.contains('│')
                                || l.contains('┌')
                                || l.contains('└')
                                || l.contains('├')
                                || l.contains('┝')
                                || l.contains('+')
                                || l.contains('|')
                        })
                        .collect();
                    assert!(
                        !table_lines.is_empty(),
                        "rule {} ({}): no table lines",
                        case.rule_id,
                        case.name
                    );
                    for l in &table_lines {
                        assert_eq!(
                            l.width(),
                            w,
                            "rule {} ({}): width {} != {}: {:?}",
                            case.rule_id,
                            case.name,
                            l.width(),
                            w,
                            l
                        );
                    }
                }

                if let Some(maxw) = case.max_line_width {
                    for l in &strs {
                        assert!(
                            l.width() <= maxw,
                            "rule {} ({}): line width {} > {}: {:?}",
                            case.rule_id,
                            case.name,
                            l.width(),
                            maxw,
                            l
                        );
                    }
                }

                if let Some(expected) = case.exact_lines {
                    let expected_vec: Vec<String> =
                        expected.iter().map(|s| s.to_string()).collect();
                    assert_eq!(
                        strs, expected_vec,
                        "rule {} ({}): lines mismatch",
                        case.rule_id, case.name
                    );
                }

                if let Some(check) = case.check {
                    check(case, &strs);
                }
            }
        }
    }
}
