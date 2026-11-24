use crate::launch;
use std::path::PathBuf;

/// Sorted top-level `.md` entries of a directory (no recursion; see
/// [`sorted_md_tree`] for the recursive variant used by `plan format`).
/// Unlike a bare `read_dir`, an unreadable directory is `Err` — callers
/// must not mistake IO failure for an empty listing.
pub fn sorted_md_entries(dir: &std::path::Path) -> Result<Vec<PathBuf>, String> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
        .collect();
    v.sort();
    Ok(v)
}

/// Sorted `.md` entries of a directory tree, recursing into subdirectories.
/// Symlinked directories are not descended into (loop protection);
/// symlinked `.md` files are included. Unlike a bare `read_dir`, an
/// unreadable directory is `Err` — callers must not mistake IO failure for
/// an empty listing.
///
/// `excludes` prunes the walk via [`is_excluded`]: an excluded subdirectory
/// is not descended into and excluded files are skipped.
pub fn sorted_md_tree(dir: &std::path::Path, excludes: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut v: Vec<PathBuf> = Vec::new();
    collect_md_recursive(dir, &mut v, excludes)?;
    v.sort();
    Ok(v)
}

fn collect_md_recursive(
    dir: &std::path::Path,
    out: &mut Vec<PathBuf>,
    excludes: &[String],
) -> Result<(), String> {
    let rd = std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    for entry in rd {
        let entry = entry.map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        let path = entry.path();
        // file_type() does not follow symlinks; is_dir() does. Skip symlinked
        // dirs so a symlink loop cannot recurse forever.
        let is_symlink = entry.file_type().map(|t| t.is_symlink()).unwrap_or(false);
        if path.is_dir() && !is_symlink {
            if is_excluded(&path, excludes) {
                continue;
            }
            collect_md_recursive(&path, out, excludes)?;
        } else if path.extension().is_some_and(|ext| ext == "md") && !is_excluded(&path, excludes) {
            out.push(path);
        }
    }
    Ok(())
}

/// Whether a format target is dropped by `--exclude` patterns.
///
/// Exclude patterns use `*`, `?`, `[...]`, and `**` (`**` spans path
/// separators); quote them so the shell passes them through. Trailing
/// slashes are ignored. A pattern without `/` matches against every path
/// component, so `target` excludes a `target/` directory (pruning its whole
/// subtree) as well as any file or directory named `target`. A pattern
/// containing `/` matches against the whole target path as displayed
/// (relative stays relative), e.g. `docs/draft/**` or `**/*.gen.md`.
fn is_excluded(path: &std::path::Path, excludes: &[String]) -> bool {
    if excludes.is_empty() {
        return false;
    }
    let full = path.to_string_lossy().replace('\\', "/");
    let comps: Vec<&str> = full.split('/').collect();
    excludes.iter().any(|raw| {
        let pat = raw.trim_end_matches('/');
        if pat.is_empty() {
            return false;
        }
        if pat.contains('/') {
            glob_match(pat, &full)
        } else {
            comps.iter().any(|c| match_component(pat, c))
        }
    })
}

fn match_component(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    fn rec(p: &[char], t: &[char]) -> bool {
        if p.is_empty() {
            return t.is_empty();
        }
        match p[0] {
            '*' => {
                if rec(&p[1..], t) {
                    return true;
                }
                if !t.is_empty() && rec(p, &t[1..]) {
                    return true;
                }
                false
            }
            '?' => !t.is_empty() && rec(&p[1..], &t[1..]),
            '[' => {
                if t.is_empty() {
                    return false;
                }
                let mut i = 1usize;
                let neg = if i < p.len() && (p[i] == '!' || p[i] == '^') {
                    i += 1;
                    true
                } else {
                    false
                };
                // leading ']' is literal
                if i < p.len() && p[i] == ']' {
                    // handled below as normal char check? treat literal
                    // We'll process as a char match
                }
                let mut matched = false;
                // handle literal leading ]
                if i < p.len() && p[i] == ']' {
                    if t[0] == ']' {
                        matched = true;
                    }
                    i += 1;
                }
                while i < p.len() && p[i] != ']' {
                    if i + 2 < p.len() && p[i + 1] == '-' {
                        // range i..i+2 (need i+2 valid)
                        if i + 2 < p.len() {
                            let lo = p[i];
                            let hi = p[i + 2];
                            if t[0] >= lo && t[0] <= hi {
                                matched = true;
                            }
                            i += 3;
                            continue;
                        }
                    }
                    if p[i] == t[0] {
                        matched = true;
                    }
                    i += 1;
                }
                if i >= p.len() {
                    // no closing bracket: treat '[' literally
                    return t[0] == '[' && rec(&p[1..], &t[1..]);
                }
                if neg {
                    matched = !matched;
                }
                if matched {
                    rec(&p[i + 1..], &t[1..])
                } else {
                    false
                }
            }
            c => !t.is_empty() && c == t[0] && rec(&p[1..], &t[1..]),
        }
    }
    rec(&p, &t)
}

fn match_components(pattern: &[&str], text: &[&str]) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }
    if pattern[0] == "**" {
        if pattern.len() == 1 {
            return true;
        }
        for i in 0..=text.len() {
            if match_components(&pattern[1..], &text[i..]) {
                return true;
            }
        }
        false
    } else {
        if text.is_empty() {
            return false;
        }
        if match_component(pattern[0], text[0]) {
            match_components(&pattern[1..], &text[1..])
        } else {
            false
        }
    }
}

fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<&str> = if pattern.is_empty() {
        vec![]
    } else {
        pattern.split('/').collect()
    };
    let t: Vec<&str> = if text.is_empty() {
        vec![]
    } else {
        text.split('/').collect()
    };
    match_components(&p, &t)
}

/// Expand a list of format targets (files and/or directories) into a sorted,
/// deduplicated list of markdown files. Directories expand to their
/// top-level `.md` files, or recursively via [`sorted_md_tree`] when
/// `recursive` is set. `excludes` drops matching targets via
/// [`is_excluded`] (an excluded directory is pruned before descending).
///
/// PATHs are literal filesystem paths: no glob interpretation is applied
/// (`*` and `**` may be part of a real filename). Shell expansion (quoting,
/// `globstar`, etc.) is the shell's job — `plan` only ever sees the paths
/// it is given. Read errors for a directory and non-file/non-dir inputs are
/// collected into the returned error vector instead of aborting.
pub fn expand_format_targets(
    paths: &[PathBuf],
    excludes: &[String],
    recursive: bool,
) -> (Vec<PathBuf>, Vec<String>) {
    let mut targets: Vec<PathBuf> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    for p in paths {
        if p.is_dir() {
            let entries = if recursive {
                sorted_md_tree(p, excludes)
            } else {
                sorted_md_entries(p)
            };
            match entries {
                Ok(entries) => {
                    targets.extend(entries.into_iter().filter(|e| !is_excluded(e, excludes)))
                }
                Err(e) => errors.push(e),
            }
        } else if p.is_file() {
            if !is_excluded(p, excludes) {
                targets.push(p.clone());
            }
        } else {
            errors.push(format!("{}: not a file or directory", p.display()));
        }
    }
    // Dedupe while preserving sorted order.
    let unique: Vec<PathBuf> = {
        let set: std::collections::BTreeSet<PathBuf> = targets.into_iter().collect();
        set.into_iter().collect()
    };
    (unique, errors)
}

/// List `.md` task files in a directory, sorted, excluding `.TEMPLATE.md`.
pub fn read_task_files(tasks_dir: &std::path::Path) -> Result<Vec<PathBuf>, String> {
    match std::fs::read_dir(tasks_dir) {
        Ok(rd) => {
            let mut v: Vec<PathBuf> = rd
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().is_some_and(|ext| ext == "md")
                        && p.file_name().is_some_and(|n| n != ".TEMPLATE.md")
                })
                .collect();
            v.sort();
            Ok(v)
        }
        Err(e) => Err(format!("cannot read {}: {e}", tasks_dir.display())),
    }
}

/// Resolve a slug-ish query against .plan/*.md files.
/// Matches by exact slug, prefix, or substring (case-insensitive).
/// Returns matching paths sorted by relevance (exact > prefix > substring).
pub fn resolve_slugs(root: &std::path::Path, query: &str) -> Vec<PathBuf> {
    let tasks_dir = root.join(crate::PLAN_DIR);
    let entries = match read_task_files(&tasks_dir) {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    let q = query.to_lowercase();

    let mut exact: Vec<PathBuf> = Vec::new();
    let mut prefix: Vec<PathBuf> = Vec::new();
    let mut substring: Vec<PathBuf> = Vec::new();

    for path in &entries {
        let slug = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        if slug == q {
            exact.push(path.clone());
        } else if slug.starts_with(&q) {
            prefix.push(path.clone());
        } else if slug.contains(&q) {
            substring.push(path.clone());
        }
    }

    let mut result = exact;
    result.extend(prefix);
    result.extend(substring);
    result
}

/// TUI selector for choosing from multiple matches.
/// Returns the selected path, or None if the user aborted (Esc/q/Ctrl-C).
/// Esc cancels (non-destructive); q is not a selection char here, so it
/// also cancels. Ctrl-C cancels too.
pub fn select_slug(matches: &[PathBuf]) -> Option<PathBuf> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    use ratatui::{
        layout::Rect,
        style::{Color, Modifier, Style},
        text::Line,
        widgets::{Block, Borders, List, ListItem, ListState},
    };

    let mut terminal = match ratatui::try_init() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("failed to init terminal: {e}");
            return None;
        }
    };

    let mut state = ListState::default();
    state.select(Some(0));

    loop {
        let items: Vec<ListItem> = matches
            .iter()
            .map(|p| {
                let slug = p.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
                ListItem::new(Line::from(slug.to_string()))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(" Select task (Enter:select  Esc/q/Ctrl-C:abort) ")
                    .borders(Borders::ALL),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        if let Err(e) = terminal.draw(|f| {
            let area = f.area();
            let w = 60.min(area.width.saturating_sub(4));
            let h = (matches.len() as u16 + 2).min(area.height.saturating_sub(4));
            let x = (area.width.saturating_sub(w)) / 2;
            let y = (area.height.saturating_sub(h)) / 2;
            let popup = Rect::new(x, y, w, h);
            f.render_widget(ratatui::widgets::Clear, popup);
            f.render_stateful_widget(list, popup, &mut state);
        }) {
            ratatui::restore();
            eprintln!("render error: {e}");
            return None;
        }

        if event::poll(std::time::Duration::from_millis(100)).unwrap_or(false)
            && let Ok(Event::Key(key)) = event::read()
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('q') if key.modifiers == KeyModifiers::NONE => {
                    ratatui::restore();
                    return None;
                }
                KeyCode::Esc => {
                    ratatui::restore();
                    return None;
                }
                KeyCode::Char('c')
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    ratatui::restore();
                    return None;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    let i = state.selected().unwrap_or(0);
                    if i > 0 {
                        state.select(Some(i - 1));
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let i = state.selected().unwrap_or(0);
                    if i + 1 < matches.len() {
                        state.select(Some(i + 1));
                    }
                }
                KeyCode::Enter => {
                    ratatui::restore();
                    return state.selected().and_then(|i| matches.get(i).cloned());
                }
                _ => {}
            }
        }
    }
}

/// Resolve a slug query to a single path.
/// If exactly one match, returns it directly.
/// If multiple matches, opens TUI selector.
/// If no match, returns None.
pub fn resolve_single_slug(root: &std::path::Path, query: &str) -> Option<PathBuf> {
    let matches = resolve_slugs(root, query);

    match matches.len() {
        0 => None,
        1 => matches.into_iter().next(),
        _ => select_slug(&matches),
    }
}

/// Create `.plan/` with the schema and template (from config/data dirs,
/// else embedded defaults). No-op with a notice if it already exists.
pub fn init_plan(root: &std::path::Path) {
    let plan_dir = root.join(".plan");
    if plan_dir.exists() {
        eprintln!(".plan/ already exists at {}", root.display());
        return;
    }

    if let Err(e) = std::fs::create_dir_all(&plan_dir) {
        eprintln!("failed to create .plan/: {e}");
        return;
    }

    // Copy schema from config/data dir if available
    if let Some(schema_src) =
        crate::resolve_file(root, ".plan", ".schema.json", "schema.json", "plan")
    {
        let dest = plan_dir.join(".schema.json");
        if let Err(e) = std::fs::copy(&schema_src, &dest) {
            eprintln!("failed to copy schema: {e}");
        } else {
            eprintln!("created {}", dest.display());
        }
    } else {
        // Write default schema
        let schema = include_str!("../../.plan/.schema.json");
        let dest = plan_dir.join(".schema.json");
        if let Err(e) = std::fs::write(&dest, schema) {
            eprintln!("failed to write schema: {e}");
        } else {
            eprintln!("created {}", dest.display());
        }
    }

    // Copy template from config/data dir if available
    if let Some(tmpl_src) =
        crate::resolve_file(root, ".plan", ".TEMPLATE.md", "TEMPLATE.md", "plan")
    {
        let dest = plan_dir.join(".TEMPLATE.md");
        if let Err(e) = std::fs::copy(&tmpl_src, &dest) {
            eprintln!("failed to copy template: {e}");
        } else {
            eprintln!("created {}", dest.display());
        }
    } else {
        let tmpl = include_str!("../../.plan/.TEMPLATE.md");
        let dest = plan_dir.join(".TEMPLATE.md");
        if let Err(e) = std::fs::write(&dest, tmpl) {
            eprintln!("failed to write template: {e}");
        } else {
            eprintln!("created {}", dest.display());
        }
    }

    eprintln!("initialized .plan/ in {}", root.display());
}

/// Returns true when `slug` is a safe task slug: nonempty, only lowercase
/// ASCII alphanumerics and hyphens, with no leading or trailing hyphen
/// (i.e. `^[a-z0-9]+(?:-[a-z0-9]+)*$`). Slugs become file names, so any
/// other character (uppercase, `_`, `.`, space, `/`, ...) is rejected.
fn is_valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && slug
            .chars()
            .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-'))
}

/// Create a task file. Returns the created path; all diagnostics are
/// `Err` strings for the caller to present.
pub fn create_task(
    root: &std::path::Path,
    slug: &str,
    status: &str,
    task_type: &str,
    priority: &str,
    area: &str,
) -> Result<PathBuf, String> {
    if !is_valid_slug(slug) {
        return Err(format!(
            "invalid slug: '{slug}' (must match ^[a-z0-9]+(?:-[a-z0-9]+)*$)"
        ));
    }

    let tasks_dir = root.join(crate::PLAN_DIR);
    let path = tasks_dir.join(format!("{slug}.md"));

    // Defense in depth: the resolved path must stay inside the tasks dir.
    if path.strip_prefix(&tasks_dir).is_err() {
        return Err(format!("invalid slug: '{slug}' escapes the task directory"));
    }

    if path.exists() {
        return Err(format!("task already exists: {}", path.display()));
    }

    // Frontmatter fields are single-line by design; a newline would inject
    // extra keys. Quotable specials (e.g. `a: b`) are handled by yaml_quote.
    for (name, value) in [
        ("status", status),
        ("type", task_type),
        ("priority", priority),
        ("area", area),
    ] {
        if value.contains(['\n', '\r']) {
            return Err(format!("invalid {name}: values must be a single line"));
        }
    }

    let content = format!(
        "---\nstatus: {}\ntype: {}\npriority: {}\narea: {}\n---\n\n## Notes\n\n",
        super::model::yaml_quote(status),
        super::model::yaml_quote(task_type),
        super::model::yaml_quote(priority),
        super::model::yaml_quote(area)
    );

    std::fs::write(&path, &content)
        .map_err(|e| format!("{}: cannot write — {e}", path.display()))?;
    Ok(path)
}

/// Render a task file through the `gloss` binary. `Err` on spawn failure
/// or nonzero exit (message carries the exit code).
pub fn open_task(path: &std::path::Path, width: usize) -> Result<(), String> {
    let gloss = launch::resolve_gloss_program();
    match std::process::Command::new(&gloss)
        .arg("--width")
        .arg(width.to_string())
        .arg(path)
        .status()
    {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("gloss exited with code: {:?}", status.code())),
        Err(e) => Err(format!("failed to launch gloss: {e}")),
    }
}

/// Delete a task file. Returns the removed path.
pub fn delete_task(path: &std::path::Path) -> Result<PathBuf, String> {
    std::fs::remove_file(path).map_err(|e| format!("{}: cannot delete — {e}", path.display()))?;
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- slug validation (create_task is the only sink for user-supplied slugs) ---

    #[test]
    fn create_task_rejects_traversal_and_separator_slugs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir(root.join(".plan")).unwrap();
        for slug in ["../foo", "..", "a/../b", "/abs", "foo/bar"] {
            assert!(
                create_task(root, slug, "open", "bug", "high", "").is_err(),
                "slug '{slug}' must be rejected"
            );
        }
        // Nothing may be created outside .plan/ (e.g. `../foo` → root/foo.md).
        assert!(
            !root.join("foo.md").exists(),
            "traversal slug created a file outside .plan/"
        );
        assert_eq!(
            std::fs::read_dir(root.join(".plan")).unwrap().count(),
            0,
            "no file may be created for a rejected slug"
        );
    }

    #[test]
    fn create_task_rejects_embedded_traversal_slug_even_when_resolvable_on_fs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // If `a/../b` ever reached the fs, it would resolve to `.plan/b.md`.
        std::fs::create_dir_all(root.join(".plan/a")).unwrap();
        assert!(create_task(root, "a/../b", "open", "bug", "high", "").is_err());
        assert!(!root.join(".plan/b.md").exists());
        assert!(!root.join(".plan/a/b.md").exists());
    }

    #[test]
    fn create_task_rejects_charset_violation_slugs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir(root.join(".plan")).unwrap();
        for slug in ["Foo_Bar", "a b", "", "a.b"] {
            assert!(
                create_task(root, slug, "open", "bug", "high", "").is_err(),
                "slug '{slug}' must be rejected"
            );
        }
        assert_eq!(
            std::fs::read_dir(root.join(".plan")).unwrap().count(),
            0,
            "no file may be created for a rejected slug"
        );
    }

    #[test]
    fn create_task_rejects_leading_or_trailing_hyphen_slugs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir(root.join(".plan")).unwrap();
        for slug in ["-foo", "foo-"] {
            assert!(
                create_task(root, slug, "open", "bug", "high", "").is_err(),
                "slug '{slug}' must be rejected"
            );
        }
        assert_eq!(std::fs::read_dir(root.join(".plan")).unwrap().count(), 0);
    }

    #[test]
    fn create_task_accepts_valid_slug() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir(root.join(".plan")).unwrap();
        assert!(create_task(root, "my-task-123", "open", "bug", "high", "backend").is_ok());
        let path = root.join(".plan/my-task-123.md");
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("status: open"));
        assert!(content.contains("type: bug"));
    }

    // --- Result contracts (bool + eprintln -> Result) ---

    #[test]
    fn create_task_ok_carries_created_path() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir(root.join(".plan")).unwrap();
        let path = create_task(root, "pathed", "open", "bug", "high", "").expect("created");
        assert_eq!(path, root.join(".plan/pathed.md"));
        assert!(path.is_file());
    }

    #[test]
    fn create_task_rejects_newline_field_values() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir(root.join(".plan")).unwrap();
        for bad in ["a\nb: c", "a\rb", "trailing\n"] {
            assert!(
                create_task(root, "inj-test", "open", "bug", "high", bad).is_err(),
                "area {bad:?} must be rejected"
            );
        }
        assert!(
            !root.join(".plan/inj-test.md").exists(),
            "rejected create must not write"
        );
    }

    #[test]
    fn create_task_quotes_yaml_special_values() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir(root.join(".plan")).unwrap();
        create_task(root, "quoted", "open", "bug", "high", "a: b").expect("created");
        let text = std::fs::read_to_string(root.join(".plan/quoted.md")).unwrap();
        assert!(
            text.contains("area: \"a: b\""),
            "special value must be quoted: {text:?}"
        );
        // Round-trip: the quoted value parses back to itself.
        let (fm, _) = crate::plan::model::parse_frontmatter(&text).unwrap();
        let v: serde_json::Value = serde_saphyr::from_str(&fm).unwrap();
        assert_eq!(v["area"], "a: b");
    }
}

#[cfg(test)]
mod entries_tests {
    use crate::plan::store::{sorted_md_entries, sorted_md_tree};

    #[test]
    fn sorted_md_entries_returns_sorted_markdown_only() {
        let tmp = tempfile::tempdir().unwrap();
        for name in ["c.md", "a.md", "b.md", "note.txt"] {
            std::fs::write(tmp.path().join(name), "x").unwrap();
        }
        let got = sorted_md_entries(tmp.path()).expect("readable");
        let names: Vec<String> = got
            .iter()
            .map(|p| p.file_name().expect("name").to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["a.md", "b.md", "c.md"]);
    }

    #[test]
    fn sorted_md_entries_errors_on_unreadable_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nope");
        let err = sorted_md_entries(&missing).expect_err("missing dir must error");
        assert!(err.contains("nope"), "error names the dir: {err}");
    }

    #[test]
    fn sorted_md_tree_recurses_into_subdirectories() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("sub/deep")).unwrap();
        for name in [
            "top.md",
            "sub/nested.md",
            "sub/deep/leaf.md",
            "sub/skip.txt",
        ] {
            std::fs::write(tmp.path().join(name), "x").unwrap();
        }
        let got = sorted_md_tree(tmp.path(), &[]).expect("readable");
        let rels: Vec<String> = got
            .iter()
            .map(|p| {
                p.strip_prefix(tmp.path())
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(rels, vec!["sub/deep/leaf.md", "sub/nested.md", "top.md"]);
    }

    #[cfg(unix)]
    #[test]
    fn sorted_md_tree_does_not_follow_symlinked_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("real")).unwrap();
        std::fs::write(tmp.path().join("real/a.md"), "x").unwrap();
        // Symlink loop: link inside real/ pointing back at the tmp root.
        std::os::unix::fs::symlink(tmp.path(), tmp.path().join("real/loop")).unwrap();
        let got = sorted_md_tree(tmp.path(), &[]).expect("readable");
        assert_eq!(got.len(), 1, "loop must not recurse: {got:?}");
        assert!(got[0].ends_with("real/a.md"));
    }

    #[test]
    fn expand_format_targets_expands_dir_and_dedupes() {
        let tmp = tempfile::tempdir().unwrap();
        for name in ["a.md", "b.md"] {
            std::fs::write(tmp.path().join(name), "x").unwrap();
        }
        let dir = tmp.path().to_path_buf();
        let file = dir.join("a.md");
        let (targets, errors) =
            super::expand_format_targets(&[dir.clone(), file.clone()], &[], false);
        assert!(errors.is_empty(), "no errors: {errors:?}");
        let names: Vec<String> = targets
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["a.md", "b.md"], "deduped + sorted");
    }

    #[test]
    fn expand_format_targets_reports_missing_path() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.md"), "x").unwrap();
        let missing = tmp.path().join("nope");
        let (targets, errors) =
            super::expand_format_targets(&[missing.clone(), tmp.path().to_path_buf()], &[], false);
        assert!(!errors.is_empty(), "missing path must error");
        assert!(
            errors.iter().any(|e| e.contains("nope")),
            "error names missing path: {errors:?}"
        );
        assert_eq!(targets.len(), 1, "readable dir still expands");
    }

    #[test]
    fn expand_format_targets_unreadable_dir_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let missing_dir = tmp.path().join("nope");
        let (targets, errors) = super::expand_format_targets(&[missing_dir.clone()], &[], false);
        assert!(targets.is_empty());
        assert!(!errors.is_empty());
        assert!(errors[0].contains("nope"));
    }

    #[cfg(unix)]
    #[test]
    fn expand_format_targets_treats_glob_chars_literally() {
        let tmp = tempfile::tempdir().unwrap();
        // A real file whose name contains glob chars: must be treated as a
        // literal path, not a pattern.
        let literal = tmp.path().join("star*.md");
        std::fs::write(&literal, "x").unwrap();
        let (targets, errors) = super::expand_format_targets(&[literal.clone()], &[], false);
        assert!(errors.is_empty(), "literal path must resolve: {errors:?}");
        assert_eq!(targets, vec![literal]);
    }

    #[test]
    fn expand_format_targets_recursive_flag_controls_descent() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("sub")).unwrap();
        std::fs::write(tmp.path().join("top.md"), "x").unwrap();
        std::fs::write(tmp.path().join("sub/nested.md"), "x").unwrap();
        let dir = tmp.path().to_path_buf();
        let (shallow, errors) = super::expand_format_targets(&[dir.clone()], &[], false);
        assert!(errors.is_empty(), "no errors: {errors:?}");
        assert_eq!(shallow.len(), 1, "shallow sees top level only");
        let (deep, errors) = super::expand_format_targets(&[dir], &[], true);
        assert!(errors.is_empty(), "no errors: {errors:?}");
        assert_eq!(deep.len(), 2, "recursive sees the subtree");
    }

    #[test]
    fn expand_format_targets_exclude_prunes_subdirectory() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("keep")).unwrap();
        std::fs::create_dir_all(tmp.path().join("skip")).unwrap();
        for name in ["keep/a.md", "skip/b.md", "top.md"] {
            std::fs::write(tmp.path().join(name), "x").unwrap();
        }
        let excl = ["skip".to_string()];
        let (targets, errors) =
            super::expand_format_targets(&[tmp.path().to_path_buf()], &excl, true);
        assert!(errors.is_empty(), "no errors: {errors:?}");
        let rels: Vec<String> = targets
            .iter()
            .map(|p| {
                p.strip_prefix(tmp.path())
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(rels, vec!["keep/a.md", "top.md"]);
    }

    #[test]
    fn expand_format_targets_exclude_matches_filenames() {
        let tmp = tempfile::tempdir().unwrap();
        for name in ["a.md", "b.gen.md"] {
            std::fs::write(tmp.path().join(name), "x").unwrap();
        }
        let excl = ["*.gen.md".to_string()];
        let (targets, errors) =
            super::expand_format_targets(&[tmp.path().to_path_buf()], &excl, false);
        assert!(errors.is_empty(), "no errors: {errors:?}");
        let names: Vec<String> = targets
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["a.md"]);
    }

    #[test]
    fn expand_format_targets_exclude_applies_to_explicit_files() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("a.md");
        std::fs::write(&file, "x").unwrap();
        let excl = ["a.md".to_string()];
        let (targets, errors) = super::expand_format_targets(&[file], &excl, false);
        assert!(
            errors.is_empty(),
            "excluded file is skipped, not an error: {errors:?}"
        );
        assert!(targets.is_empty());
    }

    #[test]
    fn expand_format_targets_exclude_with_slash_matches_full_path() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("draft")).unwrap();
        std::fs::create_dir_all(tmp.path().join("final")).unwrap();
        for name in ["draft/a.md", "final/b.md"] {
            std::fs::write(tmp.path().join(name), "x").unwrap();
        }
        let pattern = format!("{}/draft/**", tmp.path().to_string_lossy());
        let excl = [pattern];
        let (targets, errors) =
            super::expand_format_targets(&[tmp.path().to_path_buf()], &excl, true);
        assert!(errors.is_empty(), "no errors: {errors:?}");
        assert_eq!(targets.len(), 1, "only final/b.md survives: {targets:?}");
        assert!(targets[0].ends_with("final/b.md"));
    }
}
