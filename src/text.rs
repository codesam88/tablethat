//! Case-insensitive text search with byte ranges into the original text.
//!
//! Lowercase expansions (e.g. U+0130 folds to two chars) map back to whole
//! source characters, so every range is slice-safe.

/// Byte ranges of case-insensitive matches of `query_lower` in `text`.
/// Matches are non-overlapping, left to right (like `str::match_indices`).
/// Empty query matches nothing.
pub fn ci_match_ranges(text: &str, query_lower: &str) -> Vec<(usize, usize)> {
    if query_lower.is_empty() {
        return Vec::new();
    }
    // Flattened lowercase stream; each char remembers its source char's
    // byte range so matches resolve to whole source characters.
    let mut stream: Vec<(char, usize, usize)> = Vec::new();
    for (start, c) in text.char_indices() {
        let end = start + c.len_utf8();
        for lc in c.to_lowercase() {
            stream.push((lc, start, end));
        }
    }
    let q: Vec<char> = query_lower.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + q.len() <= stream.len() {
        if stream[i..i + q.len()]
            .iter()
            .map(|(c, _, _)| *c)
            .eq(q.iter().copied())
        {
            out.push((stream[i].1, stream[i + q.len() - 1].2));
            i += q.len();
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ci_match_ranges_ascii_case_insensitive() {
        assert_eq!(
            ci_match_ranges("Hello hello", "hello"),
            vec![(0, 5), (6, 11)]
        );
        assert_eq!(ci_match_ranges("HELLO", "hello"), vec![(0, 5)]);
    }

    #[test]
    fn ci_match_ranges_unicode_expansion() {
        // U+0130 lowercases to two chars; the match must cover the
        // source char's full byte range, never split it.
        let text = "A\u{130}B";
        let ranges = ci_match_ranges(text, "i\u{307}");
        assert_eq!(ranges, vec![(1, 3)]);
        for (s, e) in &ranges {
            let _ = &text[*s..*e];
        }
    }

    #[test]
    fn ci_match_ranges_partial_expansion_never_panics() {
        let text = "A\u{130}B";
        let ranges = ci_match_ranges(text, "i");
        for (s, e) in &ranges {
            assert!(text.is_char_boundary(*s) && text.is_char_boundary(*e));
            let _ = &text[*s..*e];
        }
    }

    #[test]
    fn ci_match_ranges_empty_query_is_empty() {
        assert!(ci_match_ranges("anything", "").is_empty());
    }
}
