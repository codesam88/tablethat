use super::model::{LoadedTask, Task, parse_frontmatter};
#[cfg(test)]
use super::store::read_task_files;
#[cfg(test)]
use crate as lib;
use std::path::PathBuf;

/// Load tasks from `entries`, skipping unreadable/unparseable files.
/// Filters are conjunctive exact matches; `search_query` matches slug or
/// body case-insensitively; `statuses_to_exclude` drops states (e.g. `done`).
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

/// Order two tasks by `sort_keys` in sequence (default `priority`, `slug`).
/// Unknown keys compare equal (stable, no-op).
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

/// Order by one key: `status` follows kanban order, `priority` ranks
/// high < medium < low < unknown, the rest compare lexicographically.
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
