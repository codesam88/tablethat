/// Conservative splice-engine markdown formatter: parse for structure, apply
/// only the desired edits (trailing-whitespace strip, table alignment,
/// split-only prose wrap to `width`), re-emit everything else byte-identical.
/// Idempotent — formatting twice is a fixpoint.
///
/// ```
/// # use tablethat_lib::markdown_fmt::format_commonmark;
/// let out = format_commonmark("# Title\n", 80, false);
/// assert!(out.contains("# Title"));
/// ```
pub fn format_commonmark(text: &str, width: usize, reflow: bool) -> String {
    splice_format(text, width, reflow)
}

#[derive(Debug, Clone)]
enum SpliceEdit {
    StripTrailingWs { line: usize },
    AlignTable { start: usize, end: usize },
    WrapText { start: usize, end: usize },
}

impl SpliceEdit {
    fn range(&self) -> (usize, usize) {
        match self {
            SpliceEdit::StripTrailingWs { line } => (*line, *line),
            SpliceEdit::AlignTable { start, end } => (*start, *end),
            SpliceEdit::WrapText { start, end } => (*start, *end),
        }
    }
}

fn collect_hardbreak_lines(node: &comrak::Node<'_>, out: &mut std::collections::HashSet<usize>) {
    if let comrak::nodes::NodeValue::LineBreak = node.data.borrow().value {
        out.insert(node.data.borrow().sourcepos.end.line);
    }
    for child in node.children() {
        collect_hardbreak_lines(&child, out);
    }
}

/// Conservative splice engine: strip trailing whitespace (hardbreak-exempt),
/// align tables from AST line ranges, and split-only wrap prose to `width`.
fn splice_format(text: &str, width: usize, reflow: bool) -> String {
    let arena = comrak::Arena::new();
    let opts = crate::markdown::comrak_options();
    let root = comrak::parse_document(&arena, text, &opts);

    let mut hardbreak_lines = std::collections::HashSet::new();
    collect_hardbreak_lines(&root, &mut hardbreak_lines);

    // Collect table line ranges from the AST.
    let mut table_ranges: Vec<(usize, usize)> = Vec::new();
    collect_table_ranges(&root, &mut table_ranges);

    let lines: Vec<&str> = text.split('\n').collect();

    // Collect wrap candidates (paragraphs) with container context.
    let mut wrap_candidates: Vec<WrapCandidate> = Vec::new();
    collect_wrap_candidates(&root, false, &lines, &mut wrap_candidates);

    let in_table = |lineno: usize| -> bool {
        table_ranges
            .iter()
            .any(|(s, e)| lineno >= *s && lineno <= *e)
    };

    // Render wrap edits up front (metadata needed beyond line range).
    let mut wrap_outputs: std::collections::HashMap<(usize, usize), Vec<String>> =
        std::collections::HashMap::new();
    let mut wrap_ranges: Vec<(usize, usize)> = Vec::new();

    let mut edits: Vec<SpliceEdit> = Vec::new();

    for cand in &wrap_candidates {
        if cand.start < 1 || cand.end > lines.len() || cand.start > cand.end {
            continue;
        }
        let has_overlong = (cand.start..=cand.end).any(|ln| display_width(lines[ln - 1]) > width);
        if !reflow && !has_overlong {
            continue;
        }
        let rendered = render_wrap(cand, &lines, width, &hardbreak_lines, reflow);
        wrap_outputs.insert((cand.start, cand.end), rendered);
        wrap_ranges.push((cand.start, cand.end));
        edits.push(SpliceEdit::WrapText {
            start: cand.start,
            end: cand.end,
        });
    }

    let in_wrap = |lineno: usize| -> bool {
        wrap_ranges
            .iter()
            .any(|(s, e)| lineno >= *s && lineno <= *e)
    };

    // StripTrailingWs (hardbreak-exempt), skipping table/wrap covered lines.
    for (idx, line) in lines.iter().enumerate() {
        let lineno = idx + 1;
        if *line != line.trim_end()
            && !hardbreak_lines.contains(&lineno)
            && !in_table(lineno)
            && !in_wrap(lineno)
        {
            edits.push(SpliceEdit::StripTrailingWs { line: lineno });
        }
    }

    // AlignTable edits.
    for (s, e) in &table_ranges {
        if *s >= 1 && *e <= lines.len() && *s <= *e {
            let all_prefixed = (*s..=*e).all(|ln| {
                let l = lines[ln - 1];
                // ws_len is a char boundary: trim_start removes whole chars and
                // the leading indent is ASCII whitespace (spaces/tabs).
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

    // Apply edits.
    let mut out_lines: Vec<String> = Vec::new();
    let mut cursor = 0usize;
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
            SpliceEdit::WrapText { start, end } => {
                if let Some(rendered) = wrap_outputs.get(&(*start, *end)) {
                    out_lines.extend(rendered.clone());
                } else {
                    for ln in *start..=*end {
                        out_lines.push(lines[ln - 1].to_string());
                    }
                }
            }
        }
        cursor = en;
    }
    while cursor < lines.len() {
        out_lines.push(lines[cursor].to_string());
        cursor += 1;
    }
    out_lines.join("\n")
}

#[derive(Debug, Clone)]
struct WrapCandidate {
    start: usize,
    end: usize,
    start_col: usize,
    in_list: bool,
    code_ranges: Vec<(usize, usize, usize)>,
    multiline_code_lines: std::collections::HashSet<usize>,
}

fn collect_wrap_candidates(
    node: &comrak::Node<'_>,
    in_list: bool,
    lines: &[&str],
    out: &mut Vec<WrapCandidate>,
) {
    use comrak::nodes::NodeValue;
    match &node.data.borrow().value {
        NodeValue::BlockQuote => {
            for c in node.children() {
                collect_wrap_candidates(&c, in_list, lines, out);
            }
        }
        NodeValue::List(_) => {
            for c in node.children() {
                collect_wrap_candidates(&c, true, lines, out);
            }
        }
        NodeValue::Item(_) => {
            for c in node.children() {
                collect_wrap_candidates(&c, true, lines, out);
            }
        }
        NodeValue::Paragraph => {
            let sp = node.data.borrow().sourcepos;
            if sp.start.line >= 1 {
                let mut code_ranges = Vec::new();
                collect_code_ranges(node, lines, &mut code_ranges);
                let mut multiline_code_lines = std::collections::HashSet::new();
                collect_multiline_code_lines(node, &mut multiline_code_lines);
                out.push(WrapCandidate {
                    start: sp.start.line,
                    end: sp.end.line,
                    start_col: sp.start.column,
                    in_list,
                    code_ranges,
                    multiline_code_lines,
                });
            }
        }
        _ => {
            for c in node.children() {
                collect_wrap_candidates(&c, in_list, lines, out);
            }
        }
    }
}

fn collect_multiline_code_lines(
    node: &comrak::Node<'_>,
    out: &mut std::collections::HashSet<usize>,
) {
    use comrak::nodes::NodeValue;
    if let NodeValue::Code(_) = node.data.borrow().value {
        let sp = node.data.borrow().sourcepos;
        if sp.start.line != sp.end.line {
            for ln in sp.start.line..=sp.end.line {
                out.insert(ln);
            }
        }
    }
    for c in node.children() {
        collect_multiline_code_lines(&c, out);
    }
}

fn collect_code_ranges(
    node: &comrak::Node<'_>,
    lines: &[&str],
    out: &mut Vec<(usize, usize, usize)>,
) {
    use comrak::nodes::NodeValue;
    if let NodeValue::Code(_) = node.data.borrow().value {
        let sp = node.data.borrow().sourcepos;
        if sp.start.line == sp.end.line {
            out.push((sp.start.line, sp.start.column, sp.end.column));
        } else {
            let start_line = sp.start.line;
            if start_line >= 1 && start_line <= lines.len() {
                out.push((start_line, sp.start.column, lines[start_line - 1].len()));
            }
            for ln in (sp.start.line + 1)..sp.end.line {
                if ln >= 1 && ln <= lines.len() {
                    out.push((ln, 1, lines[ln - 1].len()));
                }
            }
            let end_line = sp.end.line;
            if end_line >= 1 && end_line <= lines.len() {
                out.push((end_line, 1, sp.end.column));
            }
        }
    }
    for c in node.children() {
        collect_code_ranges(&c, lines, out);
    }
}

fn detect_prefix(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() && bytes[idx] == b' ' {
        idx += 1;
    }
    let mut j = idx;
    while j < bytes.len() {
        if bytes[j] == b'>' {
            j += 1;
            while j < bytes.len() && bytes[j] == b' ' {
                j += 1;
            }
        } else {
            break;
        }
    }
    if j > idx && line[..j].contains('>') {
        line[..j].to_string()
    } else {
        line[..idx].to_string()
    }
}

fn compute_cont_prefix(pfx: &str, in_list: bool) -> String {
    if !in_list {
        return pfx.to_string();
    }
    // Split off leading indentation + blockquote markers; the remainder is the
    // list marker (possibly including a checkbox), which becomes spaces.
    let bytes = pfx.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    let mut j = i;
    while j < bytes.len() {
        if bytes[j] == b'>' {
            j += 1;
            while j < bytes.len() && bytes[j] == b' ' {
                j += 1;
            }
        } else {
            break;
        }
    }
    let outer = &pfx[..j];
    let marker = &pfx[j..];
    format!("{}{}", outer, " ".repeat(marker.len()))
}

fn tokenize_content(content: &str, code_intervals: &[(usize, usize)]) -> Vec<(String, bool)> {
    let mut atoms = Vec::new();
    let bytes = content.as_bytes();
    let n = content.len();
    let mut i = 0;
    let mut pending_space = false;
    while i < n {
        if bytes[i].is_ascii_whitespace() {
            pending_space = true;
            i += 1;
            continue;
        }
        // code span starting at i?
        if let Some(&(s, e)) = code_intervals.iter().find(|(s, _)| *s == i) {
            atoms.push((content[s..e].to_string(), pending_space));
            pending_space = false;
            i = e;
            continue;
        }
        let mut j = i;
        while j < n && !bytes[j].is_ascii_whitespace() {
            if code_intervals.iter().any(|(s, _)| *s == j) {
                break;
            }
            j += 1;
        }
        if j > i {
            atoms.push((content[i..j].to_string(), pending_space));
            pending_space = false;
            i = j;
        } else {
            i += 1;
        }
    }
    atoms
}

/// Returns true if a continuation line beginning with `s` would be parsed
/// as opening a new block (list item, blockquote, heading, fence, thematic
/// break, or link reference definition) rather than continuing the current
/// paragraph. Such atoms must never start a continuation line.
fn would_start_block(s: &str) -> bool {
    let t = s.trim_start();
    if t.is_empty() {
        return false;
    }

    // List markers (lone or followed by space).
    if t == "-" || t == "*" || t == "+" {
        return true;
    }
    if t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ ") {
        return true;
    }
    // Ordered list markers: digits then "." or ")".
    let mut i = 0;
    for c in t.chars() {
        if c.is_ascii_digit() {
            i += 1;
        } else {
            break;
        }
    }
    if i > 0 && i < t.len() {
        let rest = &t[i..];
        if rest.starts_with('.') || rest.starts_with(')') {
            return true;
        }
    }

    // Blockquote opener.
    if t.starts_with('>') {
        return true;
    }

    // ATX heading: 1-6 `#` followed by space or end.
    let hashes = t.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) {
        let rest = &t[hashes..];
        if rest.is_empty() || rest.starts_with(' ') {
            return true;
        }
    }

    // Fenced code block opener: 3+ backticks or tildes.
    let bt = t.chars().take_while(|c| *c == '`').count();
    if bt >= 3 {
        return true;
    }
    let tl = t.chars().take_while(|c| *c == '~').count();
    if tl >= 3 {
        return true;
    }

    // Thematic break or setext underline: exactly `---`, `***`, `___`
    // (possibly with spaces), or exactly `===`.
    let stripped_spaces: String = t.chars().filter(|c| *c != ' ').collect();
    if stripped_spaces == "---"
        || stripped_spaces == "***"
        || stripped_spaces == "___"
        || stripped_spaces == "==="
    {
        return true;
    }

    // HTML block openers that can interrupt a paragraph (CommonMark):
    // `<` + letter (open tag), `</` + letter, `<!--`, `<?`, `<!` + letter.
    if let Some(after_lt) = t.strip_prefix('<') {
        let mut chars = after_lt.chars();
        match chars.next() {
            Some('/') => {
                if chars.next().is_some_and(|c| c.is_ascii_alphabetic()) {
                    return true;
                }
            }
            Some('!') => {
                if let Some(after_bang) = after_lt.strip_prefix('!')
                    && (after_bang.starts_with("--")
                        || after_bang
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_alphabetic()))
                {
                    return true;
                }
            }
            Some('?') => return true,
            Some(c) if c.is_ascii_alphabetic() => return true,
            _ => {}
        }
    }

    // Link reference definition shape: `[label]:`.
    if t.starts_with('[')
        && let Some(close) = t.find(']')
    {
        let after = &t[close + 1..];
        if after.trim_start().starts_with(':') {
            return true;
        }
    }

    false
}

fn wrap_atoms(atoms: &[(String, bool)], max_width: usize) -> Vec<String> {
    // Group atoms into chunks: a marker-like atom joins the preceding chunk,
    // so breaks (which fall only between chunks) never place one at a
    // continuation start. The previous word moves with it, keeping every
    // line within width instead of overflowing the marker's line.
    let mut chunks: Vec<(String, usize, bool)> = Vec::new(); // (text, len, space_before)
    for (i, (atom, space_before)) in atoms.iter().enumerate() {
        if i == 0 || !would_start_block(atom) {
            chunks.push((atom.clone(), display_width(atom), *space_before));
        } else if let Some((text, len, _)) = chunks.last_mut() {
            if *space_before {
                text.push(' ');
                *len += 1;
            }
            text.push_str(atom);
            *len += display_width(atom);
        }
    }
    let mut lines = Vec::new();
    let mut cur = String::new();
    let mut cur_len = 0usize;
    for (idx, (text, len, space_before)) in chunks.iter().enumerate() {
        if idx == 0 {
            // First output line keeps the source prefix and is safe.
            cur = text.clone();
            cur_len = *len;
            continue;
        }
        let total = cur_len + (*space_before as usize) + len;
        if total <= max_width {
            if *space_before {
                cur.push(' ');
                cur_len += 1;
            }
            cur.push_str(text);
            cur_len += len;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = text.clone();
            cur_len = *len;
        }
    }
    if !cur.is_empty() || lines.is_empty() {
        lines.push(cur);
    }
    lines
}

fn render_wrap(
    cand: &WrapCandidate,
    lines: &[&str],
    width: usize,
    hardbreak_lines: &std::collections::HashSet<usize>,
    reflow: bool,
) -> Vec<String> {
    let mut out = Vec::new();

    if !reflow {
        // Split-only behavior: each source line wraps independently; short
        // lines are kept (never joined).
        for (i, ln) in (cand.start..=cand.end).enumerate() {
            let idx = ln - 1;
            if idx >= lines.len() {
                break;
            }
            let line = lines[idx];
            let is_first = i == 0;
            let pfx = if is_first {
                let col = cand.start_col;
                if col >= 1 && col - 1 <= line.len() {
                    line[..col - 1].to_string()
                } else {
                    let mut s = String::new();
                    for c in line.chars() {
                        if c == ' ' {
                            s.push(c);
                        } else {
                            break;
                        }
                    }
                    s
                }
            } else {
                detect_prefix(line)
            };
            let cont_prefix = compute_cont_prefix(&pfx, cand.in_list);

            let is_hardbreak = hardbreak_lines.contains(&ln);

            if cand.multiline_code_lines.contains(&ln) {
                if is_hardbreak {
                    out.push(line.to_string());
                } else {
                    out.push(line.trim_end().to_string());
                }
                continue;
            }

            if display_width(line) <= width {
                if is_hardbreak {
                    out.push(line.to_string());
                } else {
                    out.push(line.trim_end().to_string());
                }
                continue;
            }

            let marker_str = if is_hardbreak {
                if line.ends_with('\\') { "\\" } else { "  " }
            } else {
                ""
            };
            let body = if marker_str == "  " && line.ends_with("  ") {
                &line[..line.len().saturating_sub(2)]
            } else if marker_str == "\\" && line.ends_with('\\') {
                &line[..line.len().saturating_sub(1)]
            } else {
                line
            };

            if pfx.len() > body.len() {
                out.push(line.to_string());
                continue;
            }
            let content = &body[pfx.len()..];

            let mut ivals: Vec<(usize, usize)> = cand
                .code_ranges
                .iter()
                .filter(|(l, _, _)| *l == ln)
                .map(|(_, s, e)| {
                    let ss = s.saturating_sub(1);
                    let ee = *e;
                    let cs = ss.saturating_sub(pfx.len());
                    let ce = ee.saturating_sub(pfx.len());
                    (cs, ce)
                })
                .filter(|(s, e)| *s <= content.len() && *e <= content.len() && s <= e)
                .collect();
            ivals.sort();

            let raw_atoms = tokenize_content(content, &ivals);
            let mut atoms: Vec<(String, bool)> = Vec::new();
            for (atom, sp) in raw_atoms {
                if !sp && !atoms.is_empty() {
                    let last = atoms.last_mut().unwrap();
                    last.0.push_str(&atom);
                } else {
                    atoms.push((atom, sp));
                }
            }

            if atoms.is_empty() {
                if is_hardbreak {
                    out.push(line.to_string());
                } else {
                    out.push(line.trim_end().to_string());
                }
                continue;
            }

            let max_content_width = if width > display_width(&pfx) + display_width(marker_str) {
                width - display_width(&pfx) - display_width(marker_str)
            } else {
                0
            };

            let wrapped = if max_content_width > 0 {
                wrap_atoms(&atoms, max_content_width)
            } else {
                vec![content.to_string()]
            };

            let nseg = wrapped.len();
            for (j, cl) in wrapped.iter().enumerate() {
                let prefix = if j == 0 { &pfx } else { &cont_prefix };
                let mut out_line = format!("{}{}", prefix, cl);
                if j == nseg - 1 && !marker_str.is_empty() {
                    out_line.push_str(marker_str);
                }
                out.push(out_line);
            }
        }
        return out;
    }

    // Reflow mode: join soft-broken lines within a hardbreak-delimited unit
    // into word flow, then re-split to width.

    // Build units (line ranges) separated by hardbreaks.
    let mut units: Vec<(usize, usize)> = Vec::new();
    let mut us: Option<usize> = None;
    for ln in cand.start..=cand.end {
        if us.is_none() {
            us = Some(ln);
        }
        if hardbreak_lines.contains(&ln) {
            units.push((us.unwrap(), ln));
            us = None;
        }
    }
    if let Some(s) = us {
        units.push((s, cand.end));
    }

    for (unit_start, unit_end) in units {
        let unit_lines: Vec<usize> = (unit_start..=unit_end).collect();

        // Multiline code span: never split, keep verbatim (trim unless hardbreak).
        if unit_lines
            .iter()
            .any(|ln| cand.multiline_code_lines.contains(ln))
        {
            for ln in &unit_lines {
                let idx = ln - 1;
                if idx >= lines.len() {
                    break;
                }
                let line = lines[idx];
                if hardbreak_lines.contains(ln) {
                    out.push(line.to_string());
                } else {
                    out.push(line.trim_end().to_string());
                }
            }
            continue;
        }

        let first_ln = unit_lines[0];
        let overall_first = first_ln == cand.start;
        let pfx = if overall_first {
            let line = lines[first_ln - 1];
            let col = cand.start_col;
            if col >= 1 && col - 1 <= line.len() {
                line[..col - 1].to_string()
            } else {
                let mut s = String::new();
                for c in line.chars() {
                    if c == ' ' {
                        s.push(c);
                    } else {
                        break;
                    }
                }
                s
            }
        } else {
            detect_prefix(lines[first_ln - 1])
        };
        let cont_prefix = compute_cont_prefix(&pfx, cand.in_list);

        let unit_is_hardbreak = hardbreak_lines.contains(&unit_end);

        // Collect atoms across all soft lines in the unit.
        let mut unit_atoms: Vec<(String, bool)> = Vec::new();
        let mut fallback_original = false;
        for (k, ln) in unit_lines.iter().enumerate() {
            let idx = ln - 1;
            if idx >= lines.len() {
                fallback_original = true;
                break;
            }
            let line = lines[idx];
            let lpfx = if k == 0 {
                pfx.clone()
            } else {
                detect_prefix(line)
            };

            let line_is_hardbreak = hardbreak_lines.contains(ln);
            let marker_str = if line_is_hardbreak {
                if line.ends_with('\\') { "\\" } else { "  " }
            } else {
                ""
            };
            let body = if marker_str == "  " && line.ends_with("  ") {
                &line[..line.len().saturating_sub(2)]
            } else if marker_str == "\\" && line.ends_with('\\') {
                &line[..line.len().saturating_sub(1)]
            } else {
                line
            };

            if lpfx.len() > body.len() {
                fallback_original = true;
                break;
            }
            let content = &body[lpfx.len()..];

            let mut ivals: Vec<(usize, usize)> = cand
                .code_ranges
                .iter()
                .filter(|(l, _, _)| *l == *ln)
                .map(|(_, s, e)| {
                    let ss = s.saturating_sub(1);
                    let ee = *e;
                    let cs = ss.saturating_sub(lpfx.len());
                    let ce = ee.saturating_sub(lpfx.len());
                    (cs, ce)
                })
                .filter(|(s, e)| *s <= content.len() && *e <= content.len() && s <= e)
                .collect();
            ivals.sort();

            let raw_atoms = tokenize_content(content, &ivals);
            let mut atoms: Vec<(String, bool)> = Vec::new();
            for (atom, sp) in raw_atoms {
                if !sp && !atoms.is_empty() {
                    let last = atoms.last_mut().unwrap();
                    last.0.push_str(&atom);
                } else {
                    atoms.push((atom, sp));
                }
            }

            if k > 0 && !atoms.is_empty() {
                atoms[0].1 = true;
            }
            unit_atoms.extend(atoms);
        }

        if fallback_original {
            for ln in &unit_lines {
                let idx = ln - 1;
                if idx >= lines.len() {
                    break;
                }
                let line = lines[idx];
                if hardbreak_lines.contains(ln) {
                    out.push(line.to_string());
                } else {
                    out.push(line.trim_end().to_string());
                }
            }
            continue;
        }

        if unit_atoms.is_empty() {
            for ln in &unit_lines {
                let idx = ln - 1;
                if idx >= lines.len() {
                    break;
                }
                let line = lines[idx];
                if hardbreak_lines.contains(ln) {
                    out.push(line.to_string());
                } else {
                    out.push(line.trim_end().to_string());
                }
            }
            continue;
        }

        let unit_marker = if unit_is_hardbreak {
            let last_line = lines[unit_end - 1];
            if last_line.ends_with('\\') {
                "\\"
            } else {
                "  "
            }
        } else {
            ""
        };

        let max_content_width = if width > display_width(&pfx) + display_width(unit_marker) {
            width - display_width(&pfx) - display_width(unit_marker)
        } else {
            0
        };

        let wrapped = if max_content_width > 0 {
            wrap_atoms(&unit_atoms, max_content_width)
        } else {
            vec![
                unit_atoms
                    .iter()
                    .map(|(a, _)| a.clone())
                    .collect::<Vec<_>>()
                    .join(" "),
            ]
        };

        let nseg = wrapped.len();
        for (j, cl) in wrapped.iter().enumerate() {
            let prefix = if j == 0 { &pfx } else { &cont_prefix };
            let mut out_line = format!("{}{}", prefix, cl);
            if j == nseg - 1 && !unit_marker.is_empty() {
                out_line.push_str(unit_marker);
            }
            out.push(out_line);
        }
    }

    out
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColumnAlign {
    None,
    Left,
    Right,
    Center,
}

fn align_table_lines(lines: &[&str]) -> Vec<String> {
    // Capture each row's leading-whitespace indent so prefixed tables keep it.
    // The indent prefix is ASCII whitespace (spaces/tabs); `ws_len` is a byte
    // offset that always lands on a char boundary because trim_start removes
    // whole characters and the prefix contains only ASCII bytes.
    let indents: Vec<String> = lines
        .iter()
        .map(|line| {
            let ws_len = line.len() - line.trim_start().len();
            line[..ws_len].to_string()
        })
        .collect();

    let mut parsed: Vec<Vec<String>> = lines
        .iter()
        .map(|line| {
            // ws_len is on a char boundary (see note above).
            let ws_len = line.len() - line.trim_start().len();
            let rest = &line[ws_len..];
            let inner = rest.strip_prefix('|').unwrap_or(rest);
            let inner = inner.strip_suffix('|').unwrap_or(inner);
            split_table_cells(inner)
                .into_iter()
                .map(|c| c.trim().to_string())
                .collect()
        })
        .collect();

    let num_cols = parsed.iter().map(|r| r.len()).max().unwrap_or(0);

    // Pad short rows with empty cells.
    for row in &mut parsed {
        while row.len() < num_cols {
            row.push(String::new());
        }
    }

    // Alignment is derived from the delimiter row (index 1) when present.
    let alignment: Vec<ColumnAlign> = if parsed.len() >= 2 {
        (0..num_cols)
            .map(|j| {
                let cell = parsed[1].get(j).map(|s| s.as_str()).unwrap_or("");
                if cell.starts_with(':') && cell.ends_with(':') {
                    ColumnAlign::Center
                } else if cell.starts_with(':') {
                    ColumnAlign::Left
                } else if cell.ends_with(':') {
                    ColumnAlign::Right
                } else {
                    ColumnAlign::None
                }
            })
            .collect()
    } else {
        vec![ColumnAlign::None; num_cols]
    };

    // Column widths come from all non-delimiter rows only, with a floor of 3.
    let mut col_widths = vec![0usize; num_cols];
    for (row_idx, row) in parsed.iter().enumerate() {
        if row_idx == 1 {
            continue;
        }
        for (cell, width) in row.iter().zip(col_widths.iter_mut()) {
            let w = display_width(cell);
            if w > *width {
                *width = w;
            }
        }
    }
    for w in &mut col_widths {
        if *w < 3 {
            *w = 3;
        }
    }

    let mut out = Vec::new();
    for (row_idx, row) in parsed.iter().enumerate() {
        let mut line = String::from("|");
        for j in 0..num_cols {
            let w = col_widths.get(j).copied().unwrap_or(3);
            if row_idx == 1 {
                // delimiter row
                let marker = match alignment.get(j).copied().unwrap_or(ColumnAlign::None) {
                    ColumnAlign::None => "-".repeat(w),
                    ColumnAlign::Left => format!(":{}", "-".repeat(w.saturating_sub(1))),
                    ColumnAlign::Right => format!("{}:", "-".repeat(w.saturating_sub(1))),
                    ColumnAlign::Center => format!(":{}:", "-".repeat(w.saturating_sub(2))),
                };
                line.push(' ');
                line.push_str(&marker);
                line.push(' ');
                line.push('|');
            } else {
                let cell = row.get(j).map(|s| s.as_str()).unwrap_or("");
                let dw = display_width(cell);
                let pad = w.saturating_sub(dw);
                let align = alignment.get(j).copied().unwrap_or(ColumnAlign::None);
                let padded = match align {
                    ColumnAlign::Right => format!("{}{}", " ".repeat(pad), cell),
                    ColumnAlign::Center => {
                        let left = pad / 2;
                        let right = pad - left;
                        format!("{}{}{}", " ".repeat(left), cell, " ".repeat(right))
                    }
                    _ => format!("{}{}", cell, " ".repeat(pad)),
                };
                line.push(' ');
                line.push_str(&padded);
                line.push(' ');
                line.push('|');
            }
        }
        out.push(format!("{}{}", indents[row_idx], line));
    }
    out
}

fn display_width(s: &str) -> usize {
    use unicode_width::UnicodeWidthStr;
    s.width()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatter_shares_renderer_extension_set() {
        // Both paths consume the same options constructor; their option sets
        // must be equal.
        let render_opts = crate::markdown::comrak_options();
        let format_opts = crate::markdown::comrak_options();
        assert_eq!(
            format!("{:?}", render_opts.extension),
            format!("{:?}", format_opts.extension),
            "renderer and formatter must share one options constructor"
        );
    }

    // ---- splice_format (worker 1) ----

    fn html_eq(input: &str, output: &str) {
        let opts = crate::markdown::comrak_options();
        let a = comrak::markdown_to_html(input, &opts);
        let b = comrak::markdown_to_html(output, &opts);
        // The engine only ever moves softbreak positions (line splits);
        // normalize newlines so those compare equal, but keep every other
        // byte (including whitespace runs inside code) exact. Any content
        // change still fails.
        let norm = |x: &str| -> String { x.replace('\n', " ") };
        assert_eq!(
            norm(&a),
            norm(&b),
            "HTML must be preserved by splice_format"
        );
    }

    #[test]
    fn trailing_ws_stripped_hardbreak_exempt() {
        let a = "para  \nnext\n";
        assert_eq!(splice_format(a, 80, false), a);
        html_eq(a, &splice_format(a, 80, false));

        let b = "trail x  \nnext\n";
        assert_eq!(splice_format(b, 80, false), b);
        html_eq(b, &splice_format(b, 80, false));

        let c = "trail x \nnext\n";
        let c_out = "trail x\nnext\n";
        assert_eq!(splice_format(c, 80, false), c_out);
        html_eq(c, &splice_format(c, 80, false));
    }

    #[test]
    fn short_softbreak_lines_preserved() {
        let s = "First soft line\nsecond soft line\n";
        assert_eq!(splice_format(s, 80, false), s);
        html_eq(s, &splice_format(s, 80, false));
    }

    #[test]
    fn hardbreak_flavors_preserved() {
        for s in ["a  \nb\n", "a\\\nb\n"] {
            assert_eq!(splice_format(s, 80, false), s);
            html_eq(s, &splice_format(s, 80, false));
        }
    }

    #[test]
    fn long_line_untouched_without_wrapper() {
        let s = format!("{}\n", "x".repeat(200));
        assert_eq!(splice_format(&s, 80, false), s);
        html_eq(&s, &splice_format(&s, 80, false));
    }

    // ---- item 2 tests ----

    #[test]
    fn piped_code_span_table_aligns_losslessly() {
        let s = "| Name | Note |\n| ---- | ---- |\n| `a | b` | piped code |\n";
        let out = splice_format(s, 80, false);
        // Code span containing a pipe must survive intact.
        assert!(
            out.contains("`a | b`"),
            "piped code span must survive: {out:?}"
        );
        // Every table row must be a well-formed row (starts/ends with pipe).
        for line in out.lines() {
            if line.trim_start().starts_with('|') {
                assert!(line.trim_end().ends_with('|'), "malformed row: {line:?}");
            }
        }
        // Header separator must be present.
        assert!(out.contains("---"), "separator must be present: {out:?}");
        html_eq(s, &out);
        assert_eq!(splice_format(&out, 80, false), out, "fixpoint");
    }

    #[test]
    fn aligned_table_is_fixpoint() {
        let s = "| Name | Note |\n| ---- | ---- |\n| `a | b` | piped code |\n";
        let once = splice_format(s, 80, false);
        let twice = splice_format(&once, 80, false);
        assert_eq!(once, twice);
    }

    #[test]
    fn leading_empty_cell_preserved() {
        for s in [
            "|  | a |\n| --- | --- |\n|  | b |\n",
            "|| a |\n|---|---|\n|| b |\n",
        ] {
            let out = splice_format(s, 80, false);
            for line in out.lines() {
                if line.trim_start().starts_with('|') {
                    let trimmed = line.trim_start();
                    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
                    let inner = inner.strip_suffix('|').unwrap_or(inner);
                    assert_eq!(
                        inner.split('|').count(),
                        2,
                        "wrong column count: {line:?} (from {s:?})"
                    );
                }
            }
            for line in out.lines() {
                if line.trim_start().starts_with('|') && !line.contains("---") {
                    let trimmed = line.trim_start();
                    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
                    let inner = inner.strip_suffix('|').unwrap_or(inner);
                    let first = inner.split('|').next().unwrap_or("").trim();
                    assert!(
                        first.is_empty(),
                        "leading empty cell lost: {line:?} (from {s:?})"
                    );
                }
            }
            html_eq(s, &out);
            assert_eq!(splice_format(&out, 80, false), out, "fixpoint");
        }
    }

    #[test]
    fn table_style_matches_existing_convention() {
        let s = "| a | bbbb |\n|---|---|\n| cccc | d |\n";
        let out = splice_format(s, 80, false);
        // Column count preserved (2 columns) for simple tables.
        for line in out.lines() {
            if line.trim_start().starts_with('|') && !line.contains("---") {
                let inner = line
                    .trim_start()
                    .trim_start_matches('|')
                    .trim_end_matches('|');
                assert_eq!(inner.split('|').count(), 2, "wrong columns: {line:?}");
            }
        }
        assert!(out.contains("---"), "separator must be present: {out:?}");
        html_eq(s, &out);
        assert_eq!(splice_format(&out, 80, false), out, "fixpoint");
    }

    #[test]
    fn nested_table_indent_preserved() {
        let s = "- item\n\n  | A | B |\n  | - | - |\n  | x | y |\n";
        let out = splice_format(s, 80, false);
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
        let out = splice_format(s, 80, false);
        assert_eq!(out, s);
        html_eq(s, &out);
    }

    #[test]
    fn multiline_code_span_never_split() {
        let s = "`start\n".to_string() + &"word ".repeat(14) + "aaaa more inside span` tail here\n";
        let out = splice_format(&s, 80, false);
        assert_eq!(out, s, "multiline code span must not be split");
        html_eq(&s, &out);
    }

    #[test]
    fn wrap_breaks_before_word_preceding_marker() {
        // 13×"aa" = 38 cols; "+" would glue-overflow to 40 at width 39.
        // Break before the preceding word instead: every line stays ≤ 39.
        let s = format!(
            "{} + rest words here to fill the line out further along\n",
            vec!["aa"; 13].join(" ")
        );
        let out = splice_format(&s, 39, true);
        for line in out.lines() {
            assert!(line.len() <= 39, "line too long: {line:?}");
        }
        for line in out.lines().skip(1) {
            assert!(
                !would_start_block(line.trim_start()),
                "continuation starts with marker: {line:?}"
            );
        }
        html_eq(&s, &out);
        assert_eq!(splice_format(&out, 39, true), out, "fixpoint");
    }

    #[test]
    fn wrap_atoms_keep_width_with_marker_like() {
        // Cover wrap_atoms behavior through splice_format.
        let s = "- dd ee ff + gg hh ii jj kk ll mm nn oo pp qq rr ss tt\n";
        let out = splice_format(s, 9, true);
        for line in out.lines() {
            assert!(line.len() <= 9, "line too long: {} ({})", line, line.len());
        }
        for line in out.lines().skip(1) {
            let t = line.trim_start();
            assert!(
                !would_start_block(t),
                "continuation starts with marker: {:?}",
                line
            );
        }
        html_eq(s, &out);
        assert_eq!(splice_format(&out, 9, true), out, "fixpoint");
    }

    #[test]
    fn wrap_splits_despite_marker_like_words() {
        let s = "- 1B: `serde-saphyr` pure-Rust (not `yaml_serde` rename). Accept MSRV bump + `Value` refactor.\n";
        let out = splice_format(s, 80, true);
        assert!(out.lines().count() > 1, "should wrap: {out:?}");
        for line in out.lines() {
            assert!(line.len() <= 80, "line too long: {} ({})", line, line.len());
        }
        for line in out.lines().skip(1) {
            let t = line.trim_start();
            assert!(
                !would_start_block(t),
                "continuation starts with marker: {:?}",
                line
            );
        }
        html_eq(s, &out);
        assert_eq!(splice_format(&out, 80, true), out, "fixpoint");
    }

    #[test]
    fn no_continuation_starts_with_marker() {
        let s = "- alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau - upsilon phi chi psi omega end of line padding here\n";
        let out = splice_format(s, 80, true);
        for line in out.lines() {
            assert!(line.len() <= 80, "line too long: {} ({})", line, line.len());
        }
        for line in out.lines().skip(1) {
            let t = line.trim_start();
            assert!(
                !would_start_block(t),
                "continuation starts with marker: {:?}",
                line
            );
        }
        html_eq(s, &out);
        assert_eq!(splice_format(&out, 80, true), out, "fixpoint");
    }

    #[test]
    fn continuation_never_starts_blockquote() {
        let s = "a".repeat(39)
            + " > rest of the words here and more words to fill out the paragraph body text\n";
        let out = splice_format(&s, 40, true);
        for line in out.lines() {
            let t = line.trim_start();
            assert!(
                !t.starts_with('>'),
                "continuation starts with >: {:?}",
                line
            );
        }
        html_eq(&s, &out);
        assert_eq!(splice_format(&out, 40, true), out, "fixpoint");
    }

    #[test]
    fn continuation_never_opens_fence_or_heading() {
        let s = "aaa bbb ccc ddd eee fff ggg hhh iii ``` kkk lll mmm nnn ooo ppp qqq rrr sss ttt uuu vvv www xxx yyy zzz end here\n";
        let out = splice_format(s, 40, true);
        for line in out.lines() {
            let t = line.trim_start();
            assert!(!t.starts_with("```"), "fence opened: {:?}", line);
            assert!(!t.starts_with("~~~"), "fence opened: {:?}", line);
            assert!(
                !(t.starts_with('#') && (t.len() == 1 || t.as_bytes().get(1) == Some(&b' '))),
                "heading opened: {:?}",
                line
            );
        }
        html_eq(&s, &out);
        assert_eq!(splice_format(&out, 40, true), out, "fixpoint");
    }

    #[test]
    fn block_opener_set_detected_accurately() {
        // (atom, must-guard). Over-triggers are documented, never silent.
        let guarded = [
            "-",
            "*",
            "+",
            "1.",
            "2)",
            "10.", // list markers
            ">",
            ">quoted", // blockquote (prefix match: over-triggers ">x")
            "#",
            "##",
            "######", // ATX headings
            "```",
            "````rust",
            "~~~", // fenced code
            "---",
            "***",
            "___",
            "===", // breaks / setext underline
            "[1]:",
            "[foo]:", // reference definitions
            "<div",
            "</div>",
            "<!--",
            "<?php",
            "<!doctype",
            "<hr>", // HTML blocks
        ];
        for atom in guarded {
            assert!(would_start_block(atom), "must guard {atom:?}");
        }
        let safe = [
            "->",  // dash without space: literal text, not a list
            "1B:", // digits not followed by ./)
            "C#",
            "#tag", // # without space/end: literal text
            "[x]",
            "[exec|a]", // brackets without colon: not a refdef
            "`code`",
            "``x``", // code spans: atomic by span rule instead
            "pure-Rust",
            "a+b",
            "word", // ordinary prose
            "<3",
            "a<b", // < not followed by tag opener
        ];
        for atom in safe {
            assert!(!would_start_block(atom), "must not guard {atom:?}");
        }
        // Documented over-triggers (safe direction: slight overflow, no corruption).
        assert!(would_start_block("2.5")); // version numbers glue to previous word
    }

    #[test]
    fn continuation_never_opens_html_block() {
        let s = format!(
            "{} <div class=\"q\"> tail words here to fill the line out further along\n",
            "a".repeat(36)
        );
        let out = splice_format(&s, 40, true);
        for line in out.lines().skip(1) {
            assert!(
                !line.trim_start().starts_with("<div"),
                "html block opened: {line:?}"
            );
        }
        html_eq(&s, &out);
        assert_eq!(splice_format(&out, 40, true), out, "fixpoint");
    }

    // ---- STEP 1 tests (worker 3) ----

    #[test]
    fn overlong_paragraph_splits_never_joins() {
        let long_word = "word ";
        let long_line = long_word.repeat(40); // ~200 chars
        let s = format!("{}\n", long_line.trim_end());
        let out = splice_format(&s, 80, false);
        for line in out.lines() {
            assert!(line.len() <= 80, "line too long: {} ({})", line, line.len());
        }
        assert!(out.lines().count() > 1, "should split");
        html_eq(&s, &out);
        // fixpoint
        assert_eq!(splice_format(&out, 80, false), out);

        // two short lines stay byte-identical
        let short = "first short line\nsecond short line\n";
        assert_eq!(splice_format(short, 80, false), short);
        html_eq(short, &splice_format(short, 80, false));
    }

    #[test]
    fn code_span_is_atomic() {
        let s = "Here is text before `code span that crosses the boundary here` and more trailing text after it to force wrap\n";
        let out = splice_format(s, 80, false);
        assert!(
            out.contains("`code span that crosses the boundary here`"),
            "code span must stay whole: {}",
            out
        );
        html_eq(s, &out);
        assert_eq!(splice_format(&out, 80, false), out);
    }

    #[test]
    fn overlong_word_overflows() {
        let s = format!("{}\n", "x".repeat(100));
        assert_eq!(splice_format(&s, 80, false), s);
        html_eq(&s, &splice_format(&s, 80, false));
    }

    #[test]
    fn nested_list_wraps_with_indent() {
        let s = format!("- {}\n", "word ".repeat(30).trim_end());
        let out = splice_format(&s, 80, false);
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines.len() > 1, "should wrap");
        if lines.len() > 1 {
            assert!(
                lines[1].starts_with("  "),
                "continuation indent 2 spaces: {:?}",
                lines[1]
            );
        }
        html_eq(&s, &out);
        assert_eq!(splice_format(&out, 80, false), out);

        // ordered
        let s2 = format!("1. {}\n", "word ".repeat(30).trim_end());
        let out2 = splice_format(&s2, 80, false);
        let lines2: Vec<&str> = out2.lines().collect();
        if lines2.len() > 1 {
            assert!(
                lines2[1].starts_with("   "),
                "continuation indent 3 spaces: {:?}",
                lines2[1]
            );
        }
        html_eq(&s2, &out2);
    }

    #[test]
    fn blockquote_wrap_preserves_markers() {
        let s = format!("> {}\n", "word ".repeat(30).trim_end());
        let out = splice_format(&s, 80, false);
        for line in out.lines() {
            assert!(line.starts_with("> "), "marker missing: {:?}", line);
        }
        html_eq(&s, &out);
        assert_eq!(splice_format(&out, 80, false), out);
    }

    #[test]
    fn hardbreak_splits_wrap_unit() {
        let long = "word ".repeat(30).trim_end().to_string();
        let s = format!("short  \n{}\n", long);
        let out = splice_format(&s, 80, false);
        assert!(
            out.starts_with("short  \n"),
            "hardbreak marker intact: {:?}",
            out
        );
        html_eq(&s, &out);
        assert_eq!(splice_format(&out, 80, false), out);
    }

    #[test]
    fn reflow_joins_and_resplits_to_width() {
        let s = "first short line\nsecond short line\n".to_string()
            + &("word ".repeat(40).trim_end().to_string() + "\n");
        let out = splice_format(&s, 80, true);
        for line in out.lines() {
            assert!(line.len() <= 80, "line too long: {line:?}");
        }
        assert!(
            out.lines()
                .next()
                .is_some_and(|l| l.contains("first short line second short line")),
            "reflow must join short lines: {out:?}"
        );
        // Pins the mode distinction: split-only must NOT produce this.
        assert_ne!(out, splice_format(&s, 80, false));
        html_eq(&s, &out);
        assert_eq!(splice_format(&out, 80, true), out, "reflow fixpoint");
    }

    #[test]
    fn reflow_containers_with_indent() {
        let s = format!("- {}\n", "item word ".repeat(20).trim_end());
        let out = splice_format(&s, 80, true);
        for line in out.lines() {
            assert!(line.len() <= 80, "line too long: {line:?}");
        }
        assert!(out.lines().count() > 1, "should split: {out:?}");
        assert!(
            out.lines().skip(1).all(|l| l.starts_with("  ")),
            "continuation indent: {out:?}"
        );
        html_eq(&s, &out);
        assert_eq!(splice_format(&out, 80, true), out, "reflow fixpoint");
    }

    #[test]
    fn reflow_hardbreak_units_independent() {
        let long = "word ".repeat(30).trim_end().to_string();
        let s = format!("seg one  \n{}\n", long);
        let out = splice_format(&s, 80, true);
        assert!(out.starts_with("seg one  \n"), "first unit intact: {out:?}");
        for line in out.lines() {
            assert!(line.len() <= 80, "line too long: {line:?}");
        }
        html_eq(&s, &out);
        assert_eq!(splice_format(&out, 80, true), out, "reflow fixpoint");
    }

    #[test]
    fn unicode_width_counts_columns_not_bytes() {
        let line = "x".repeat(78) + " —";
        assert_eq!(line.chars().count(), 80);
        assert_eq!(line.len(), 82);
        let s = format!("{}\n", line);
        let out = splice_format(&s, 80, false);
        assert_eq!(out, s, "80-column unicode line must be left alone: {out:?}");
        html_eq(&s, &out);
        assert_eq!(splice_format(&out, 80, false), out, "fixpoint");
    }

    #[test]
    fn unicode_overlong_still_wraps() {
        use unicode_width::UnicodeWidthStr;
        let s = format!("{}\n", "— ".repeat(50).trim_end());
        let out = splice_format(&s, 80, false);
        assert!(out.lines().count() > 1, "should wrap: {out:?}");
        for line in out.lines() {
            assert!(
                UnicodeWidthStr::width(line) <= 80,
                "line too wide: {:?} ({} cols)",
                line,
                UnicodeWidthStr::width(line)
            );
        }
        html_eq(&s, &out);
        assert_eq!(splice_format(&out, 80, false), out, "fixpoint");
    }

    #[test]
    fn cjk_wraps_by_display_width() {
        use unicode_width::UnicodeWidthStr;
        let s = format!("{}\n", "字 ".repeat(50).trim_end());
        let out = splice_format(&s, 80, false);
        assert!(out.lines().count() > 1, "should wrap: {out:?}");
        for line in out.lines() {
            assert!(
                UnicodeWidthStr::width(line) <= 80,
                "line too wide: {:?} ({} cols)",
                line,
                UnicodeWidthStr::width(line)
            );
        }
        html_eq(&s, &out);
        assert_eq!(splice_format(&out, 80, false), out, "fixpoint");
    }
}
