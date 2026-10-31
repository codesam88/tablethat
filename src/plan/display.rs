use super::model::LoadedTask;
use super::query;
use super::store;
use crate::{ColorsConfig, Config};
use std::io::Write;
use termcolor::{ColorChoice, ColorSpec, StandardStream, WriteColor};

/// Validate, filter, sort, and print the task list (table or kanban).
/// Validation diagnostics print to stderr; an unreadable task dir is `Err`.
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
) -> Result<(), String> {
    let mut lint_warnings = Vec::new();
    match super::lint::validate_all(root, &mut lint_warnings) {
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

    let tasks_dir = root.join(crate::PLAN_DIR);

    let entries = store::read_task_files(&tasks_dir)?;

    let all_tasks = query::load_and_filter_tasks(&entries, None, None, None, None, None, &[]);
    let exclude: Vec<String> = if cfg.hide_done {
        vec!["done".into()]
    } else {
        vec![]
    };
    let mut tasks = query::load_and_filter_tasks(
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
        return Ok(());
    }

    let effective_sort: Vec<String> = if sort_keys.is_empty() {
        cfg.default_sort.clone()
    } else {
        sort_keys.to_vec()
    };
    tasks.sort_by(|a, b| query::cmp_tasks(a, b, &effective_sort, &cfg.kanban_order));

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
    Ok(())
}

macro_rules! write_colored {
    ($stdout:expr, $color:expr, $($arg:tt)*) => {{
        let _ = $stdout.set_color(ColorSpec::new().set_fg(Some($color)));
        let _ = write!($stdout, $($arg)*);
        let _ = $stdout.reset();
    }};
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
        group.sort_by(|a, b| query::cmp_tasks(a, b, sort_keys, kanban_order));
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
        let color = crate::color::ratatui_to_termcolor(crate::color::status_color(status, colors));
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
                let pc = crate::color::ratatui_to_termcolor(crate::color::priority_color(
                    &t.task.priority,
                    colors,
                ));
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
        let color = crate::color::ratatui_to_termcolor(crate::color::status_color(status, colors));
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
        let sc =
            crate::color::ratatui_to_termcolor(crate::color::status_color(&t.task.status, colors));
        let pc = crate::color::ratatui_to_termcolor(crate::color::priority_color(
            &t.task.priority,
            colors,
        ));

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
            crate::color::ratatui_to_termcolor(crate::color::status_color("in-progress", colors)),
            " {} in-progress",
            in_progress_count
        );
    }
    if open_count > 0 {
        write_colored!(
            stdout,
            crate::color::ratatui_to_termcolor(crate::color::status_color("open", colors)),
            " {} open",
            open_count
        );
    }
    if blocked_count > 0 {
        write_colored!(
            stdout,
            crate::color::ratatui_to_termcolor(crate::color::status_color("blocked", colors)),
            " {} blocked",
            blocked_count
        );
    }
    if backlog_count > 0 {
        write_colored!(
            stdout,
            crate::color::ratatui_to_termcolor(crate::color::status_color("backlog", colors)),
            " {} backlog",
            backlog_count
        );
    }
    if idea_count > 0 {
        write_colored!(
            stdout,
            crate::color::ratatui_to_termcolor(crate::color::status_color("idea", colors)),
            " {} idea",
            idea_count
        );
    }
    if done_count > 0 {
        write_colored!(
            stdout,
            crate::color::ratatui_to_termcolor(crate::color::status_color("done", colors)),
            " {} done",
            done_count
        );
    }
    let _ = writeln!(stdout);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_tasks_unreadable_dir_is_err() {
        let tmp = tempfile::tempdir().unwrap();
        // .plan exists as a FILE so read_task_files fails deterministically
        // on every platform (no chmod/privilege games).
        std::fs::write(tmp.path().join(".plan"), "not a dir").unwrap();
        let cfg = Config::default();
        let err = list_tasks(tmp.path(), &cfg, None, None, None, None, None, false, &[])
            .expect_err("unreadable task dir must be an error, not exit(1)");
        assert!(err.contains(".plan"), "error names the dir: {err}");
    }
}
