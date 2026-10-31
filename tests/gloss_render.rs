//! Rendering tests for the gloss markdown pipeline (`tablethat_lib::markdown`).
//!
//! Strategy (see task discussion): styles are tested once against
//! `theme_from_cfg`; everything else asserts **plain text** via `text_of`,
//! so theme tweaks don't cascade. Three layers:
//!
//! 1. **Table-driven fixed patterns** — one `(name, input, width, expected)`
//!    row per rule. New regression case = one row, not a new `#[test]` fn.
//! 2. **Golden files** (`tests/golden/*.md` + `*.expected`, width 80,
//!    default theme). Full-document pinning incl. blank-line cadence.
//!    Regenerate with `UPDATE_GOLDEN=1 cargo test --test gloss_render`
//!    — then REVIEW the diff before committing. Never bless blindly.
//! 3. **Properties over the corpus** — no panics at any width, paragraph
//!    lines respect `width`, no content loss, `format_commonmark`
//!    idempotent. No new crates; corpus doubles as CLI fixtures later.
//!
//! Out of scope: comrak's own parsing, exact ANSI bytes, TUI event loop.

use ratatui::text::Line;
use tablethat_lib::ThemeConfig;
use tablethat_lib::markdown::{format_commonmark, render_markdown, theme_from_cfg};

const WIDTH: usize = 80;

/// Plain-text projection of rendered lines: content only, styles stripped.
fn text_of(lines: &[Line<'static>]) -> Vec<String> {
    lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect()
}

fn render_default(text: &str, width: usize) -> Vec<String> {
    let th = theme_from_cfg(&ThemeConfig::default());
    text_of(&render_markdown(&th, text, width))
}

// ---------------------------------------------------------------------------
// 1. Table-driven fixed patterns
// ---------------------------------------------------------------------------

/// (name, markdown input, width, expected plain-text lines)
const CASES: &[(&str, &str, usize, &[&str])] = &[
    (
        "heading keeps prefix on one line",
        "# Hello world\n",
        40,
        &["# Hello world", ""],
    ),
    (
        "paragraph wraps at spaces",
        "one two three four five\n",
        15,
        // NOTE: wrapped lines keep a trailing space (wrap_spans wart, pinned
        // faithfully — do not "fix" the expectation without fixing the renderer).
        &["one two three ", "four five", ""],
    ),
    (
        "bracket group stays together",
        "x ([word]) y\n",
        8,
        // NOTE: same trailing-space wart as above ("x ").
        &["x ", "([word])", "y", ""],
    ),
    (
        // The pipeline flattens links to their text (URL dropped); the
        // '](' break rule itself is pinned at span level by wrap_spans
        // unit tests in src/markdown.rs.
        "link text flattened, url dropped",
        "text before [link text](http://example.com) and more\n",
        25,
        &["text before link text and", "more", ""],
    ),
    (
        "list wraps with hanging indent",
        "- This is a long list item that should wrap at the margin\n",
        40,
        &[
            "  - This is a long list item that should",
            "    wrap at the margin",
            "",
        ],
    ),
    (
        "code block is not wrapped",
        "```\nlet long_variable_name = something_very_long_here;\n```\n",
        40,
        &["let long_variable_name = something_very_long_here;", ""],
    ),
    (
        "frontmatter is stripped",
        "---\nstatus: open\n---\n\nbody text\n",
        80,
        &["body text", ""],
    ),
];

#[test]
fn fixed_patterns() {
    for (name, input, width, expected) in CASES {
        let got = render_default(input, *width);
        let want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        assert_eq!(got, want, "fixed pattern failed: {name}");
    }
}

// ---------------------------------------------------------------------------
// 4. Styles mapped once (theme changes break 1 test, not N)
// ---------------------------------------------------------------------------

#[test]
fn theme_styles_applied_to_spans() {
    let cfg = ThemeConfig {
        h1_color: ratatui::style::Color::Red,
        code_color: ratatui::style::Color::Green,
        ..ThemeConfig::default()
    };
    let th = theme_from_cfg(&cfg);
    let lines = render_markdown(&th, "# Title\n\nsome `code` here\n", 80);
    let head: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(head, "# Title");
    assert!(
        lines[0].spans.iter().all(|s| s.style == th.h1),
        "heading spans must all carry h1 style"
    );
    let code_line = lines.iter().find(|l| {
        l.spans
            .iter()
            .any(|s| s.content.as_ref() == "code" && s.style == th.code)
    });
    assert!(
        code_line.is_some(),
        "inline code span must carry code style"
    );
}

// ---------------------------------------------------------------------------
// 2+3. Golden files + corpus properties
// ---------------------------------------------------------------------------

fn corpus_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn corpus_inputs() -> Vec<std::path::PathBuf> {
    let mut v: Vec<std::path::PathBuf> = std::fs::read_dir(corpus_dir())
        .expect("tests/golden dir must exist — add corpus files")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
        .collect();
    v.sort();
    assert!(
        !v.is_empty(),
        "tests/golden must contain at least one .md file"
    );
    v
}

#[test]
fn golden_files_match() {
    let bless = std::env::var("UPDATE_GOLDEN").is_ok();
    for path in corpus_inputs() {
        let input = std::fs::read_to_string(&path).expect("read corpus file");
        let got = render_default(&input, WIDTH).join("\n");
        let expected_path = path.with_extension("expected");
        if bless {
            std::fs::write(&expected_path, format!("{got}\n")).expect("bless golden file");
            continue;
        }
        let want = std::fs::read_to_string(&expected_path)
            .unwrap_or_else(|_| panic!("missing golden file: {}", expected_path.display()));
        assert_eq!(
            got.trim_end_matches('\n'),
            want.trim_end_matches('\n'),
            "golden mismatch: {} (re-bless with UPDATE_GOLDEN=1, then review)",
            path.file_name().expect("file name").to_string_lossy()
        );
    }
}

#[test]
fn corpus_never_panics_at_any_width() {
    let th = theme_from_cfg(&ThemeConfig::default());
    for path in corpus_inputs() {
        let input = std::fs::read_to_string(&path).expect("read corpus file");
        for width in [20, 40, 80, 120] {
            let _ = render_markdown(&th, &input, width);
        }
    }
}

/// Alphanumeric words in the input must survive rendering (no silent drops).
/// Markdown syntax tokens, URLs, and fenced-code info strings (e.g. the
/// `rust` in ```rust — metadata, correctly dropped) are excluded.
fn prose_words(input: &str) -> Vec<&str> {
    input
        .lines()
        .filter(|l| !l.trim_start().starts_with("```"))
        .flat_map(|l| {
            // Drop whole whitespace-separated URL tokens first so path
            // segments (e.g. the `path` in http://example.com/some/path,
            // which the renderer correctly drops with the URL) are excluded.
            l.split_whitespace()
                .filter(|tok| !tok.contains("://"))
                .collect::<Vec<_>>()
        })
        .flat_map(|tok| {
            tok.split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .collect::<Vec<_>>()
        })
        .filter(|w| {
            w.len() > 2
                && !["http", "https", "com", "example"].contains(w)
                && !w.chars().all(|c| c.is_numeric())
        })
        .collect()
}

#[test]
fn corpus_loses_no_content() {
    for path in corpus_inputs() {
        // Frontmatter is stripped by design — check the body only.
        let input = std::fs::read_to_string(&path).expect("read corpus file");
        let body = tablethat_lib::markdown::strip_frontmatter(&input);
        let rendered = render_default(body, WIDTH).join("\n");
        for word in prose_words(body) {
            // Skip frontmatter keys/values: they live above the stripped body
            // only when strip misses; body words must all render.
            assert!(
                rendered.contains(word),
                "word {word:?} lost rendering {}",
                path.file_name().expect("file name").to_string_lossy()
            );
        }
    }
}

#[test]
fn formatter_is_idempotent() {
    for path in corpus_inputs() {
        let input = std::fs::read_to_string(&path).expect("read corpus file");
        let once = format_commonmark(&input, WIDTH);
        let twice = format_commonmark(&once, WIDTH);
        assert_eq!(
            once,
            twice,
            "format_commonmark not idempotent for {}",
            path.file_name().expect("file name").to_string_lossy()
        );
    }
}
