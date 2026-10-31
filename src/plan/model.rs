use serde::Deserialize;

/// `serde_yaml` coerced YAML null to `""` for `String` fields; `serde_saphyr`
/// rejects null for strings. Preserve the old leniency so files with empty
/// values (e.g. `area: `) still load instead of being silently skipped.
fn null_to_empty_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(|opt| opt.unwrap_or_default())
}

/// Task frontmatter: the filename (sans `.md`) is the unique key.
/// YAML nulls coerce to `""` (legacy `serde_yaml` leniency, kept).
#[derive(Deserialize, Debug)]
pub struct Task {
    /// Lifecycle state (`idea`, `backlog`, `open`, `in-progress`, `blocked`, `done`).
    pub status: String,
    /// Kind of work (`type` in YAML).
    #[serde(rename = "type", deserialize_with = "null_to_empty_string")]
    pub task_type: String,
    /// Urgency (`high`, `medium`, `low`).
    #[serde(deserialize_with = "null_to_empty_string")]
    pub priority: String,
    /// Optional grouping label; defaults to empty.
    #[serde(default, deserialize_with = "null_to_empty_string")]
    pub area: String,
}

/// A task paired with its filename slug.
pub struct LoadedTask {
    /// Filename without `.md`.
    pub slug: String,
    /// Parsed frontmatter.
    pub task: Task,
}

/// Split `---`-delimited frontmatter from a task file. Returns the YAML
/// block and the 1-based line where it starts (for WARNING locations).
pub fn parse_frontmatter(content: &str) -> Result<(String, usize), String> {
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
}
