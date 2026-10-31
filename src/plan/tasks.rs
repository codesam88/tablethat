use serde::Deserialize;
use std::path::PathBuf;
use tablethat_lib::{self as lib, ColorsConfig, Config};

/// `serde_yaml` coerced YAML null to `""` for `String` fields; `serde_saphyr`
/// rejects null for strings. Preserve the old leniency so files with empty
/// values (e.g. `area: `) still load instead of being silently skipped.
fn null_to_empty_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(|opt| opt.unwrap_or_default())
}

#[derive(Deserialize, Debug)]
pub struct Task {
    #[serde(deserialize_with = "null_to_empty_string")]
    pub status: String,
    #[serde(rename = "type", deserialize_with = "null_to_empty_string")]
    pub task_type: String,
    #[serde(deserialize_with = "null_to_empty_string")]
    pub priority: String,
    #[serde(default, deserialize_with = "null_to_empty_string")]
    pub area: String,
}

pub struct LoadedTask {
    pub slug: String,
    pub task: Task,
}

/// Validate every task file against the resolved schema.
///
/// Diagnostics are returned, never printed: `Ok(())` carries no news,
/// `Err(errors)` carries one line per failure. Non-fatal notes (unknown
/// keys, empty tree) go into `warnings`. Callers decide presentation
/// and exit codes.
pub fn validate_all(root: &std::path::Path, warnings: &mut Vec<String>) -> Result<(), Vec<String>> {
    let tasks_dir = root.join(lib::PLAN_DIR);

    let schema_path = match lib::resolve_file(root, ".plan", ".schema.json", "schema.json", "plan")
    {
        Some(p) => p,
        None => {
            return Err(vec![
                "error: no schema found (searched .plan/.schema.json, config dir, data dir)".into(),
            ]);
        }
    };

    let schema: serde_json::Value = match std::fs::read_to_string(&schema_path)
        .map_err(|e| format!("{}: cannot read — {e}", schema_path.display()))
        .and_then(|s| {
            serde_json::from_str(&s)
                .map_err(|e| format!("{}: invalid JSON — {e}", schema_path.display()))
        }) {
        Ok(v) => v,
        Err(e) => return Err(vec![e]),
    };

    let required: Vec<&str> = schema["required"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let properties = schema["properties"].as_object();

    let entries = match read_task_files(&tasks_dir) {
        Ok(v) => v,
        Err(e) => return Err(vec![e]),
    };

    if entries.is_empty() {
        warnings.push(format!(
            "warning: no task files found in {}",
            tasks_dir.display()
        ));
    }

    let mut errors: Vec<String> = Vec::new();

    for path in &entries {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                errors.push(format!("{}: FAIL — cannot read: {e}", path.display()));
                continue;
            }
        };

        let (frontmatter, line) = match parse_frontmatter(&content) {
            Ok(fm) => fm,
            Err(msg) => {
                errors.push(format!("{}: FAIL — {msg}", path.display()));
                continue;
            }
        };

        let parsed: serde_json::Value = match serde_saphyr::from_str(&frontmatter) {
            Ok(v) => v,
            Err(e) => {
                errors.push(format!("{}: FAIL — YAML parse error: {e}", path.display()));
                continue;
            }
        };

        let Some(obj) = parsed.as_object() else {
            errors.push(format!(
                "{}: FAIL — frontmatter must be a YAML mapping",
                path.display()
            ));
            continue;
        };

        for field in &required {
            if !obj.contains_key(*field) {
                errors.push(format!(
                    "{}: FAIL — missing required field '{field}'",
                    path.display()
                ));
            }
        }

        if let Some(props) = properties {
            for (key, prop_schema) in props {
                let Some(val) = obj.get(key) else { continue };

                if let Some(allowed) = prop_schema["enum"].as_array() {
                    if let Some(s) = val.as_str() {
                        let found = allowed.iter().any(|a| a.as_str() == Some(s));
                        if !found {
                            let values: Vec<&str> =
                                allowed.iter().filter_map(|a| a.as_str()).collect();
                            errors.push(format!(
                                "{}: FAIL — '{key}' must be one of [{}], got '{s}'",
                                path.display(),
                                values.join(", ")
                            ));
                        }
                    } else {
                        errors.push(format!(
                            "{}: FAIL — '{key}' must be a string, got {val}",
                            path.display()
                        ));
                    }
                }
            }
        }

        let unknown_keys: Vec<&String> = obj
            .keys()
            .filter(|k| {
                !required.contains(&k.as_str())
                    && !properties.is_some_and(|p| p.contains_key(k.as_str()))
            })
            .collect();

        for k in unknown_keys {
            if line > 0 {
                warnings.push(format!(
                    "{}:{line}: WARNING — unknown frontmatter key '{k}'",
                    path.display()
                ));
            } else {
                warnings.push(format!(
                    "{}: WARNING — unknown frontmatter key '{k}'",
                    path.display()
                ));
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn list_tasks(
    root: &std::path::Path,
    cfg: &Config,
    status_filter: Option<&str>,
    type_filter: Option<&str>,
    priority_filter: Option<&str>,
    area_filter: Option<&str>,
    search_query: Option<&str>,
    kanban: bool,
    sort_keys: &[String],
) {
    let mut lint_warnings = Vec::new();
    match validate_all(root, &mut lint_warnings) {
        Ok(()) => {}
        Err(lint_errors) => {
            for e in &lint_errors {
                eprintln!("{e}");
            }
        }
    }
    for w in &lint_warnings {
        eprintln!("{w}");
    }

    let tasks_dir = root.join(lib::PLAN_DIR);

    let entries = match read_task_files(&tasks_dir) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    let all_tasks = load_and_filter_tasks(&entries, None, None, None, None, None, &[]);
    let exclude: Vec<String> = if cfg.hide_done {
        vec!["done".into()]
    } else {
        vec![]
    };
    let mut tasks = load_and_filter_tasks(
        &entries,
        status_filter,
        type_filter,
        priority_filter,
        area_filter,
        search_query,
        &exclude,
    );

    if tasks.is_empty() {
        println!("(no tasks match filters)");
        return;
    }

    let effective_sort: Vec<String> = if sort_keys.is_empty() {
        cfg.default_sort.clone()
    } else {
        sort_keys.to_vec()
    };
    tasks.sort_by(|a, b| cmp_tasks(a, b, &effective_sort, &cfg.kanban_order));

    if kanban {
        display_kanban(
            &tasks,
            &all_tasks,
            cfg.hide_done,
            &cfg.kanban_order,
            &cfg.colors,
            &effective_sort,
        );
    } else {
        display_table(&tasks, &cfg.colors);
    }
}

pub fn load_and_filter_tasks(
    entries: &[PathBuf],
    status_filter: Option<&str>,
    type_filter: Option<&str>,
    priority_filter: Option<&str>,
    area_filter: Option<&str>,
    search_query: Option<&str>,
    statuses_to_exclude: &[String],
) -> Vec<LoadedTask> {
    let mut tasks: Vec<LoadedTask> = Vec::new();

    for path in entries {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let (frontmatter, _) = match parse_frontmatter(&content) {
            Ok(fm) => fm,
            Err(_) => continue,
        };

        let task: Task = match serde_saphyr::from_str(&frontmatter) {
            Ok(t) => t,
            Err(_) => continue,
        };

        let slug = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let body_start = content
            .lines()
            .enumerate()
            .skip_while(|(_, l)| l.trim() != "---")
            .skip(1)
            .find(|(_, l)| l.trim() == "---")
            .map(|(i, _)| i + 1)
            .unwrap_or(0);

        let body = if body_start > 0 {
            content
                .lines()
                .skip(body_start)
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string()
        } else {
            String::new()
        };

        if statuses_to_exclude.contains(&task.status) {
            continue;
        }
        if let Some(s) = status_filter
            && task.status != s
        {
            continue;
        }
        if let Some(t) = type_filter
            && task.task_type != t
        {
            continue;
        }
        if let Some(p) = priority_filter
            && task.priority != p
        {
            continue;
        }
        if let Some(a) = area_filter
            && task.area != a
        {
            continue;
        }
        if let Some(q) = search_query {
            let q_lower = q.to_lowercase();
            if !slug.to_lowercase().contains(&q_lower) && !body.to_lowercase().contains(&q_lower) {
                continue;
            }
        }

        tasks.push(LoadedTask { slug, task });
    }

    tasks
}

pub fn cmp_tasks(
    a: &LoadedTask,
    b: &LoadedTask,
    sort_keys: &[String],
    kanban_order: &[String],
) -> std::cmp::Ordering {
    let keys: &[String] = if sort_keys.is_empty() {
        &["priority".into(), "slug".into()]
    } else {
        sort_keys
    };

    for key in keys {
        let ord = cmp_by_key(a, b, key, kanban_order);
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    std::cmp::Ordering::Equal
}

pub fn cmp_by_key(
    a: &LoadedTask,
    b: &LoadedTask,
    key: &str,
    kanban_order: &[String],
) -> std::cmp::Ordering {
    match key {
        "status" => {
            let oa = kanban_ord(&a.task.status, kanban_order);
            let ob = kanban_ord(&b.task.status, kanban_order);
            oa.cmp(&ob)
        }
        "type" => a.task.task_type.cmp(&b.task.task_type),
        "priority" => priority_ord(&a.task.priority).cmp(&priority_ord(&b.task.priority)),
        "area" => a.task.area.cmp(&b.task.area),
        "slug" => a.slug.cmp(&b.slug),
        _ => std::cmp::Ordering::Equal,
    }
}

fn kanban_ord(status: &str, kanban_order: &[String]) -> u8 {
    kanban_order
        .iter()
        .position(|s| s == status)
        .map_or(99, |i| i as u8)
}

fn priority_ord(p: &str) -> u8 {
    match p {
        "high" => 0,
        "medium" => 1,
        "low" => 2,
        _ => 3,
    }
}

use std::io::Write;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

macro_rules! write_colored {
    ($stdout:expr, $color:expr, $($arg:tt)*) => {{
        let _ = $stdout.set_color(ColorSpec::new().set_fg(Some($color)));
        let _ = write!($stdout, $($arg)*);
        let _ = $stdout.reset();
    }};
}

fn status_color(status: &str, colors: &ColorsConfig) -> Color {
    lib::ratatui_to_termcolor(lib::status_color(status, colors))
}

fn priority_color(p: &str, colors: &ColorsConfig) -> Color {
    lib::ratatui_to_termcolor(lib::priority_color(p, colors))
}

fn status_label(status: &str) -> &str {
    match status {
        "in-progress" => "IN PROGRESS",
        "open" => "OPEN",
        "blocked" => "BLOCKED",
        "backlog" => "BACKLOG",
        "idea" => "IDEA",
        "done" => "DONE",
        _ => status,
    }
}

fn display_kanban(
    tasks: &[LoadedTask],
    all_tasks: &[LoadedTask],
    hide_done: bool,
    kanban_order: &[String],
    colors: &ColorsConfig,
    sort_keys: &[String],
) {
    let mut groups: Vec<(&str, Vec<&LoadedTask>)> = Vec::new();
    let mut total = 0;

    for status in kanban_order {
        let mut group: Vec<&LoadedTask> =
            tasks.iter().filter(|t| &t.task.status == status).collect();
        group.sort_by(|a, b| cmp_tasks(a, b, sort_keys, kanban_order));
        total += group.len();
        groups.push((status, group));
    }

    let label_w = kanban_order
        .iter()
        .map(|s| status_label(s).len())
        .max()
        .unwrap_or(10);
    let bar_w = 60usize.saturating_sub(label_w + 6);

    let slug_w = tasks.iter().map(|t| t.slug.len()).max().unwrap_or(4).max(4);
    let type_w = 8;
    let prio_w = 8;

    let mut stdout = StandardStream::stdout(ColorChoice::Auto);

    for (status, group) in &groups {
        let label = status_label(status);
        let color = status_color(status, colors);
        let count = group.len();
        let bar = "─".repeat(bar_w);
        let total = all_tasks
            .iter()
            .filter(|t| t.task.status == *status)
            .count();

        let _ = writeln!(stdout);
        if hide_done && *status == "done" && count < total {
            write_colored!(stdout, color, "{} ({} hidden)", label, total - count);
        } else {
            write_colored!(stdout, color, "{} ({})", label, count);
        }
        let _ = writeln!(stdout, " {}", bar);

        if group.is_empty() {
            println!(" (none)");
        } else {
            for t in group {
                let pc = priority_color(&t.task.priority, colors);
                let _ = write!(
                    stdout,
                    " {:<slug_w$} {:<type_w$} ",
                    t.slug,
                    t.task.task_type,
                    slug_w = slug_w,
                    type_w = type_w
                );
                write_colored!(stdout, pc, "{:<prio_w$}", t.task.priority, prio_w = prio_w);
                let _ = writeln!(stdout, " {}", t.task.area);
            }
        }
    }

    println!();
    let _ = write!(stdout, "{} tasks:", total);
    for (status, g) in groups.iter().filter(|(_, g)| !g.is_empty()) {
        let color = status_color(status, colors);
        write_colored!(stdout, color, " {} {}", g.len(), status);
    }
    let _ = writeln!(stdout);
}

fn display_table(tasks: &[LoadedTask], colors: &ColorsConfig) {
    let slug_w = tasks.iter().map(|t| t.slug.len()).max().unwrap_or(4).max(4);
    let status_w = 11;
    let type_w = 8;
    let prio_w = 8;

    println!(
        "{:<slug_w$} {:<status_w$} {:<type_w$} {:<prio_w$} area",
        "SLUG",
        "STATUS",
        "TYPE",
        "PRIORITY",
        slug_w = slug_w,
        status_w = status_w,
        type_w = type_w,
        prio_w = prio_w,
    );
    println!(
        "{:-<slug_w$} {:-<status_w$} {:-<type_w$} {:-<prio_w$} ----",
        "",
        "",
        "",
        "",
        slug_w = slug_w,
        status_w = status_w,
        type_w = type_w,
        prio_w = prio_w,
    );

    let mut stdout = StandardStream::stdout(ColorChoice::Auto);

    for t in tasks {
        let sc = status_color(&t.task.status, colors);
        let pc = priority_color(&t.task.priority, colors);

        let _ = write!(stdout, "{:<slug_w$} ", t.slug, slug_w = slug_w);
        write_colored!(
            stdout,
            sc,
            "{:<status_w$}",
            t.task.status,
            status_w = status_w
        );
        let _ = write!(stdout, " {:<type_w$} ", t.task.task_type, type_w = type_w);
        write_colored!(stdout, pc, "{:<prio_w$}", t.task.priority, prio_w = prio_w);
        let _ = writeln!(stdout, " {}", t.task.area);
    }

    let idea_count = tasks.iter().filter(|t| t.task.status == "idea").count();
    let backlog_count = tasks.iter().filter(|t| t.task.status == "backlog").count();
    let open_count = tasks.iter().filter(|t| t.task.status == "open").count();
    let in_progress_count = tasks
        .iter()
        .filter(|t| t.task.status == "in-progress")
        .count();
    let blocked_count = tasks.iter().filter(|t| t.task.status == "blocked").count();
    let done_count = tasks.iter().filter(|t| t.task.status == "done").count();

    let _ = writeln!(stdout);
    let _ = write!(stdout, "{} tasks:", tasks.len());
    if in_progress_count > 0 {
        write_colored!(
            stdout,
            status_color("in-progress", colors),
            " {} in-progress",
            in_progress_count
        );
    }
    if open_count > 0 {
        write_colored!(stdout, status_color("open", colors), " {} open", open_count);
    }
    if blocked_count > 0 {
        write_colored!(
            stdout,
            status_color("blocked", colors),
            " {} blocked",
            blocked_count
        );
    }
    if backlog_count > 0 {
        write_colored!(
            stdout,
            status_color("backlog", colors),
            " {} backlog",
            backlog_count
        );
    }
    if idea_count > 0 {
        write_colored!(stdout, status_color("idea", colors), " {} idea", idea_count);
    }
    if done_count > 0 {
        write_colored!(stdout, status_color("done", colors), " {} done", done_count);
    }
    let _ = writeln!(stdout);
}

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
    let tasks_dir = root.join(lib::PLAN_DIR);
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

fn parse_frontmatter(content: &str) -> Result<(String, usize), String> {
    let mut lines = content.lines().enumerate();

    let Some((first_line, _)) = lines.find(|(_, l)| l.trim() == "---") else {
        return Err("no frontmatter delimiters found".into());
    };

    let Some((second_line, _)) = lines.find(|(_, l)| l.trim() == "---") else {
        return Err("unclosed frontmatter (missing closing ---)".into());
    };

    let start = first_line + 1;
    let end = second_line;

    if start >= end {
        return Err("empty frontmatter".into());
    }

    let frontmatter: Vec<&str> = content.lines().skip(start).take(end - start).collect();
    Ok((frontmatter.join("\n"), first_line + 1))
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
        lib::resolve_file(root, ".plan", ".schema.json", "schema.json", "plan")
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
    if let Some(tmpl_src) = lib::resolve_file(root, ".plan", ".TEMPLATE.md", "TEMPLATE.md", "plan")
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

    let tasks_dir = root.join(lib::PLAN_DIR);
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
    let gloss = lib::launch::resolve_gloss_program();
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

/// Re-emit a task file with canonical frontmatter field ordering and clean formatting.
/// Preserves the body text and any unknown fields exactly. Returns the full markdown.
fn normalize_frontmatter(content: &str) -> Result<String, String> {
    const CANONICAL_ORDER: &[&str] = &["status", "type", "priority", "area"];
    const FORMAT_WIDTH: usize = 120;

    let (fml, start_line) = parse_frontmatter(content)?;

    let parsed: serde_json::Value =
        serde_saphyr::from_str(&fml).map_err(|e| format!("YAML parse error: {e}"))?;
    let obj = parsed
        .as_object()
        .ok_or("frontmatter must be a YAML mapping")?;

    let mut lines: Vec<String> = vec!["---".into()];

    for &key in CANONICAL_ORDER {
        if let Some(val) = obj.get(key) {
            lines.push(format!("{}: {}", key, value_to_yaml_str(val)));
        }
    }

    for (key, val) in obj.iter() {
        let k = key.as_str();
        if CANONICAL_ORDER.contains(&k) {
            continue;
        }
        lines.push(format!("{k}: {}", value_to_yaml_str(val)));
    }

    lines.push("---".into());

    let line_before_body = content
        .lines()
        .enumerate()
        .skip(start_line)
        .find(|(_, l)| l.trim() == "---")
        .map(|(i, _)| i);

    if let Some(body_start) = line_before_body {
        let rest: Vec<&str> = content.lines().skip(body_start + 1).collect();
        let body = rest.join("\n");
        let formatted = format_markdown_body(&body, FORMAT_WIDTH);
        let trimmed = formatted.trim();
        if !trimmed.is_empty() {
            lines.push(String::new());
            lines.push(trimmed.to_string());
        }
    }

    Ok(lines.join("\n") + "\n")
}

fn value_to_yaml_str(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => serde_saphyr::to_string(val)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

fn format_markdown_body(body: &str, width: usize) -> String {
    lib::markdown::format_commonmark(body, width)
}

/// Format one file in place. Returns whether the file changed;
/// failures are `Err` strings for the caller to present.
pub fn format_file(path: &std::path::Path, width: usize) -> Result<bool, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("{}: cannot read — {e}", path.display()))?;

    let formatted = if path.extension().is_some_and(|ext| ext == "md") {
        // Check if file has frontmatter
        if content.trim_start().starts_with("---") {
            normalize_frontmatter(&content)
                .map_err(|e| format!("{}: cannot normalize — {e}", path.display()))?
        } else {
            format_markdown_body(&content, width)
        }
    } else {
        format_markdown_body(&content, width)
    };

    if formatted != content {
        std::fs::write(path, &formatted)
            .map_err(|e| format!("{}: cannot write — {e}", path.display()))?;
        return Ok(true);
    }
    Ok(false)
}

/// Normalize every task file. Returns the count of files changed;
/// per-file failures are collected into `Err` for the caller to present.
pub fn normalize_all(root: &std::path::Path) -> Result<usize, Vec<String>> {
    let tasks_dir = root.join(lib::PLAN_DIR);

    let entries = match read_task_files(&tasks_dir) {
        Ok(v) => v,
        Err(e) => return Err(vec![e]),
    };

    let mut fixed = 0;
    let mut errors: Vec<String> = Vec::new();

    for path in &entries {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                errors.push(format!("{}: cannot read — {e}", path.display()));
                continue;
            }
        };

        let normalized = match normalize_frontmatter(&content) {
            Ok(n) => n,
            Err(e) => {
                errors.push(format!("{}: cannot normalize — {e}", path.display()));
                continue;
            }
        };

        if normalized != content {
            match std::fs::write(path, &normalized) {
                Ok(()) => fixed += 1,
                Err(e) => errors.push(format!("{}: cannot write — {e}", path.display())),
            }
        }
    }

    if errors.is_empty() {
        Ok(fixed)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_parses_with_pure_rust_backend() {
        let fm = "status: open\ntype: bug\npriority: high\narea: backend\n";
        let task: Task = serde_saphyr::from_str(fm).expect("saphyr backend parses Task");
        assert_eq!(task.status, "open");
        assert_eq!(task.task_type, "bug");
        assert_eq!(task.priority, "high");
        assert_eq!(task.area, "backend");
    }

    #[test]
    fn frontmatter_null_area_parses_as_empty_string() {
        // serde_yaml coerced YAML null (`area: `) to "" for String fields;
        // serde_saphyr rejects it without the null_to_empty_string shim.
        let fm = "status: open\ntype: bug\npriority: high\narea: \n";
        let task: Task = serde_saphyr::from_str(fm).expect("null area parses");
        assert_eq!(task.area, "");
    }

    #[test]
    fn parse_frontmatter_valid() {
        let md = "---\nstatus: open\ntype: bug\n---\n\nbody text";
        let (fm, line) = parse_frontmatter(md).unwrap();
        assert_eq!(fm, "status: open\ntype: bug");
        assert_eq!(line, 1);
    }

    #[test]
    fn parse_frontmatter_no_delimiters() {
        let md = "just text";
        assert!(parse_frontmatter(md).is_err());
    }

    #[test]
    fn parse_frontmatter_unclosed() {
        let md = "---\nstatus: open\n";
        assert!(parse_frontmatter(md).is_err());
    }

    #[test]
    fn parse_frontmatter_empty() {
        let md = "---\n---\nbody";
        assert!(parse_frontmatter(md).is_err());
    }

    #[test]
    fn parse_frontmatter_dashes_in_body() {
        let md =
            "---\nstatus: open\ntype: bug\npriority: high\n---\n\nA --- separator in body text.";
        let (fm, _) = parse_frontmatter(md).unwrap();
        assert!(fm.contains("status: open"));
    }

    #[test]
    fn parse_frontmatter_only_frontmatter_no_body() {
        let md = "---\nstatus: done\ntype: chore\npriority: low\n---";
        let (fm, _) = parse_frontmatter(md).unwrap();
        assert_eq!(fm, "status: done\ntype: chore\npriority: low");
    }

    #[test]
    fn parse_frontmatter_with_trailing_newlines() {
        let md = "---\nstatus: open\n---\n\n\n\n";
        let (fm, _) = parse_frontmatter(md).unwrap();
        assert_eq!(fm, "status: open");
    }

    fn make_task(status: &str, typ: &str, priority: &str, area: &str, slug: &str) -> LoadedTask {
        LoadedTask {
            slug: slug.to_string(),
            task: Task {
                status: status.to_string(),
                task_type: typ.to_string(),
                priority: priority.to_string(),
                area: area.to_string(),
            },
        }
    }

    #[test]
    fn cmp_default_sorts_priority_then_slug() {
        let a = make_task("open", "bug", "medium", "", "alpha");
        let b = make_task("open", "feature", "high", "", "beta");
        let empty: [String; 0] = [];
        let ko: Vec<String> = vec![
            "idea".into(),
            "backlog".into(),
            "open".into(),
            "in-progress".into(),
            "blocked".into(),
            "done".into(),
        ];
        assert_eq!(cmp_tasks(&a, &b, &empty, &ko), std::cmp::Ordering::Greater);
        assert_eq!(cmp_tasks(&b, &a, &empty, &ko), std::cmp::Ordering::Less);
    }

    #[test]
    fn cmp_default_tiebreak_by_slug() {
        let a = make_task("open", "bug", "high", "", "alpha");
        let b = make_task("open", "feature", "high", "", "beta");
        let empty: [String; 0] = [];
        let ko: Vec<String> = vec![
            "idea".into(),
            "backlog".into(),
            "open".into(),
            "in-progress".into(),
            "blocked".into(),
            "done".into(),
        ];
        assert_eq!(cmp_tasks(&a, &b, &empty, &ko), std::cmp::Ordering::Less);
    }

    #[test]
    fn cmp_sorts_by_area_then_priority() {
        let a = make_task("open", "bug", "high", "backend", "x");
        let b = make_task("open", "feature", "low", "frontend", "x");
        let keys = ["area".into(), "priority".into()];
        let ko: Vec<String> = vec![
            "idea".into(),
            "backlog".into(),
            "open".into(),
            "in-progress".into(),
            "blocked".into(),
            "done".into(),
        ];
        assert_eq!(cmp_tasks(&a, &b, &keys, &ko), std::cmp::Ordering::Less);
    }

    #[test]
    fn cmp_sorts_by_status_via_kanban_order() {
        let a = make_task("idea", "feature", "low", "", "x");
        let b = make_task("open", "feature", "low", "", "x");
        let keys = ["status".into()];
        let ko: Vec<String> = vec![
            "idea".into(),
            "backlog".into(),
            "open".into(),
            "in-progress".into(),
            "blocked".into(),
            "done".into(),
        ];
        assert_eq!(cmp_tasks(&a, &b, &keys, &ko), std::cmp::Ordering::Less);
    }

    #[test]
    fn priority_ord_higher_ranks_first() {
        assert!(priority_ord("high") < priority_ord("medium"));
        assert!(priority_ord("medium") < priority_ord("low"));
    }

    #[test]
    fn priority_ord_unknown_is_last() {
        assert!(priority_ord("high") < priority_ord("nonsense"));
        assert!(priority_ord("low") < priority_ord("nonsense"));
    }

    #[test]
    fn normalize_reorders_fields_canonically() {
        let input = "---\narea: backend\nstatus: open\ntype: bug\npriority: high\n---\n\nbody\n";
        let output = normalize_frontmatter(input).unwrap();
        assert_eq!(
            output,
            "---\nstatus: open\ntype: bug\npriority: high\narea: backend\n---\n\nbody\n"
        );
    }

    #[test]
    fn normalize_preserves_unknown_fields() {
        let input = "---\nstatus: open\ntype: bug\npriority: high\ncustom: value\n---\n";
        let output = normalize_frontmatter(input).unwrap();
        assert!(output.contains("custom: value"));
    }

    #[test]
    fn normalize_preserves_body_text() {
        let input = "---\nstatus: open\ntype: bug\npriority: low\n---\n\n## Notes\nSome text.\n";
        let output = normalize_frontmatter(input).unwrap();
        assert!(output.contains("## Notes\n\nSome text."));
    }

    #[test]
    fn normalize_no_body_omits_trailing_blank() {
        let input = "---\nstatus: open\ntype: bug\npriority: low\n---\n";
        let output = normalize_frontmatter(input).unwrap();
        assert_eq!(output, "---\nstatus: open\ntype: bug\npriority: low\n---\n");
    }

    #[test]
    fn normalize_already_canonical_is_unchanged() {
        let input = "---\nstatus: open\ntype: bug\npriority: low\narea: backend\n---\n\nbody\n";
        let output = normalize_frontmatter(input).unwrap();
        assert_eq!(output, input);
    }

    fn make_task_file(dir: &std::path::Path, slug: &str, status: &str, typ: &str, priority: &str) {
        let content = format!(
            "---\nstatus: {status}\ntype: {typ}\npriority: {priority}\narea: \n---\n\n## Notes\n"
        );
        std::fs::write(dir.join(format!("{slug}.md")), content).unwrap();
    }

    #[test]
    fn filter_excludes_done_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        make_task_file(dir, "alpha", "open", "bug", "high");
        make_task_file(dir, "beta", "done", "feature", "low");
        make_task_file(dir, "gamma", "in-progress", "chore", "medium");

        let entries = read_task_files(dir).unwrap();
        let exclude: Vec<String> = vec!["done".into()];
        let tasks = load_and_filter_tasks(&entries, None, None, None, None, None, &exclude);
        let slugs: Vec<&str> = tasks.iter().map(|t| t.slug.as_str()).collect();
        assert!(slugs.contains(&"alpha"));
        assert!(slugs.contains(&"gamma"));
        assert!(!slugs.contains(&"beta"));
    }

    #[test]
    fn filter_no_exclusions_returns_all() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        make_task_file(dir, "alpha", "open", "bug", "high");
        make_task_file(dir, "beta", "done", "feature", "low");

        let entries = read_task_files(dir).unwrap();
        let tasks = load_and_filter_tasks(&entries, None, None, None, None, None, &[]);
        assert_eq!(tasks.len(), 2);
    }

    #[test]
    fn filter_exclusions_combine_with_status_filter() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        make_task_file(dir, "alpha", "open", "bug", "high");
        make_task_file(dir, "beta", "done", "feature", "low");
        make_task_file(dir, "gamma", "open", "chore", "medium");

        let entries = read_task_files(dir).unwrap();
        let exclude: Vec<String> = vec!["done".into()];
        let tasks = load_and_filter_tasks(&entries, Some("open"), None, None, None, None, &exclude);
        let slugs: Vec<&str> = tasks.iter().map(|t| t.slug.as_str()).collect();
        assert!(slugs.contains(&"alpha"));
        assert!(slugs.contains(&"gamma"));
        assert!(!slugs.contains(&"beta"));
        assert_eq!(tasks.len(), 2);
    }

    #[test]
    fn config_hide_done_defaults_to_true() {
        let cfg = lib::Config::default();
        assert!(cfg.hide_done);
    }

    #[test]
    fn hidden_count_matches_difference_between_all_and_filtered() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        make_task_file(dir, "alpha", "open", "bug", "high");
        make_task_file(dir, "beta", "done", "feature", "low");
        make_task_file(dir, "gamma", "done", "chore", "medium");

        let entries = read_task_files(dir).unwrap();
        let all = load_and_filter_tasks(&entries, None, None, None, None, None, &[]);
        let exclude: Vec<String> = vec!["done".into()];
        let filtered = load_and_filter_tasks(&entries, None, None, None, None, None, &exclude);

        let all_done = all.iter().filter(|t| t.task.status == "done").count();
        let filtered_done = filtered.iter().filter(|t| t.task.status == "done").count();
        assert_eq!(all_done, 2);
        assert_eq!(filtered_done, 0);
        assert_eq!(all_done - filtered_done, 2); // hidden count
    }

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

    const TEST_SCHEMA: &str = r#"{
        "required": ["status", "type", "priority"],
        "properties": {
            "status": {"enum": ["open", "done"]},
            "type": {"enum": ["bug", "feature"]},
            "priority": {"enum": ["high", "low"]}
        }
    }"#;

    fn plan_root_with_schema() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join(".plan")).unwrap();
        std::fs::write(tmp.path().join(".plan/.schema.json"), TEST_SCHEMA).unwrap();
        tmp
    }

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
    fn format_file_reports_changed_flag() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        // Canonical content is a fixpoint: Ok(false), byte-identical.
        let path = dir.join("a.md");
        std::fs::write(
            &path,
            "---\nstatus: open\ntype: bug\npriority: high\n---\n\nbody\n",
        )
        .unwrap();
        assert_eq!(format_file(&path, 120).expect("formats"), false);
        // Scrambled order is fixed: Ok(true).
        std::fs::write(
            &path,
            "---\npriority: high\nstatus: open\ntype: bug\n---\n\nbody\n",
        )
        .unwrap();
        assert_eq!(format_file(&path, 120).expect("formats"), true);
        // Missing file is an error, not silent false.
        assert!(format_file(&dir.join("missing.md"), 120).is_err());
    }

    #[test]
    fn validate_all_collects_errors_and_warnings() {
        let tmp = plan_root_with_schema();
        let root = tmp.path();
        std::fs::write(
            root.join(".plan/good.md"),
            "---\nstatus: open\ntype: bug\npriority: high\n---\n\nok\n",
        )
        .unwrap();
        std::fs::write(
            root.join(".plan/bad.md"),
            "---\nstatus: open\ntype: bug\n---\n\nmissing priority\n",
        )
        .unwrap();
        std::fs::write(
            root.join(".plan/warn.md"),
            "---\nstatus: done\ntype: feature\npriority: low\ncustom: x\n---\n\nok\n",
        )
        .unwrap();

        let mut warnings = Vec::new();
        let errors = validate_all(root, &mut warnings).expect_err("bad.md must fail");
        assert_eq!(warnings.len(), 1, "warn.md unknown key: {warnings:?}");
        assert!(
            warnings[0].contains("custom"),
            "warning names the key: {}",
            warnings[0]
        );
        assert!(
            errors.iter().any(|e| e.contains("priority")),
            "error names the missing field: {errors:?}"
        );
    }

    #[test]
    fn validate_all_ok_carries_warnings() {
        let tmp = plan_root_with_schema();
        let root = tmp.path();
        std::fs::write(
            root.join(".plan/warn.md"),
            "---\nstatus: done\ntype: feature\npriority: low\ncustom: x\n---\n\nok\n",
        )
        .unwrap();
        let mut warnings = Vec::new();
        validate_all(root, &mut warnings).expect("no errors");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn normalize_all_returns_fixed_count() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir(root.join(".plan")).unwrap();
        std::fs::write(
            root.join(".plan/a.md"),
            "---\npriority: high\nstatus: open\ntype: bug\n---\n\nbody\n",
        )
        .unwrap();
        assert_eq!(normalize_all(root).expect("normalizes"), 1);
        assert_eq!(normalize_all(root).expect("idempotent"), 0);
    }
}
