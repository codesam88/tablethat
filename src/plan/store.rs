use crate::launch;
use std::path::PathBuf;

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
/// Returns the selected path, or None if the user aborted (q/Ctrl-C).
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
                    .title(" Select task (Enter:select  q:abort) ")
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
                KeyCode::Char('q') | KeyCode::Esc => {
                    ratatui::restore();
                    return None;
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
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

    let content = format!(
        "---\nstatus: {status}\ntype: {task_type}\npriority: {priority}\narea: {area}\n---\n\n## Notes\n\n"
    );

    std::fs::write(&path, &content)
        .map_err(|e| format!("{}: cannot write — {e}", path.display()))?;
    Ok(path)
}

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
}
