use super::model::parse_frontmatter;
use super::store;
use crate::markdown_fmt;

/// Re-emit a task file with canonical frontmatter field ordering and clean formatting.
/// Preserves the body text and any unknown fields exactly. Returns the full markdown.
fn normalize_frontmatter(content: &str, width: usize, reflow: bool) -> Result<String, String> {
    const CANONICAL_ORDER: &[&str] = &["status", "type", "priority", "area"];

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
        let formatted = format_markdown_body(&body, width, reflow);
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
        // Null stays bare: `area: ` round-trips instead of becoming `null`.
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(s) => super::model::yaml_quote(s),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => serde_saphyr::to_string(val)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

fn format_markdown_body(body: &str, width: usize, reflow: bool) -> String {
    markdown_fmt::format_commonmark(body, width, reflow)
}

/// Format one file in place. Returns whether the file changed;
/// failures are `Err` strings for the caller to present.
pub fn format_file(
    path: &std::path::Path,
    width: usize,
    reflow: bool,
    check: bool,
) -> Result<bool, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("{}: cannot read — {e}", path.display()))?;

    let formatted = if path.extension().is_some_and(|ext| ext == "md") {
        // Check if file has frontmatter
        if content.trim_start().starts_with("---") {
            normalize_frontmatter(&content, width, reflow)
                .map_err(|e| format!("{}: cannot normalize — {e}", path.display()))?
        } else {
            format_markdown_body(&content, width, reflow)
        }
    } else {
        format_markdown_body(&content, width, reflow)
    };

    if formatted != content {
        if check {
            return Ok(true);
        }
        std::fs::write(path, &formatted)
            .map_err(|e| format!("{}: cannot write — {e}", path.display()))?;
        return Ok(true);
    }
    Ok(false)
}

/// Normalize every task file. Returns the count of files changed;
/// per-file failures are collected into `Err` for the caller to present.
pub fn normalize_all(
    root: &std::path::Path,
    width: usize,
    reflow: bool,
    check: bool,
) -> Result<usize, Vec<String>> {
    let tasks_dir = root.join(crate::PLAN_DIR);

    let entries = match store::read_task_files(&tasks_dir) {
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

        let normalized = match normalize_frontmatter(&content, width, reflow) {
            Ok(n) => n,
            Err(e) => {
                errors.push(format!("{}: cannot normalize — {e}", path.display()));
                continue;
            }
        };

        if normalized != content {
            if check {
                fixed += 1;
            } else {
                match std::fs::write(path, &normalized) {
                    Ok(()) => fixed += 1,
                    Err(e) => errors.push(format!("{}: cannot write — {e}", path.display())),
                }
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
    fn normalize_round_trips_quoted_special_values() {
        let input = "---\nstatus: open\ntype: bug\npriority: high\narea: \"a: b\"\n---\n\nbody\n";
        let output = normalize_frontmatter(input, 80, false).unwrap();
        assert!(
            output.contains("area: \"a: b\""),
            "quoted value must survive: {output:?}"
        );
        // Fixpoint: formatting twice changes nothing.
        assert_eq!(normalize_frontmatter(&output, 80, false).unwrap(), output);
    }

    #[test]
    fn normalize_preserves_empty_values() {
        // `area: ` parses as null; it must stay bare, not become `null`.
        let input = "---\nstatus: open\ntype: bug\npriority: high\narea: \n---\n\nbody\n";
        let output = normalize_frontmatter(input, 80, false).unwrap();
        assert!(
            output.contains("area: \n"),
            "empty must stay bare: {output:?}"
        );
        assert!(
            !output.contains("null"),
            "null must not leak in: {output:?}"
        );
    }

    #[test]
    fn normalize_reorders_fields_canonically() {
        let input = "---\narea: backend\nstatus: open\ntype: bug\npriority: high\n---\n\nbody\n";
        let output = normalize_frontmatter(input, 80, false).unwrap();
        let pos = |key: &str| output.find(&format!("\n{key}:")).unwrap_or(usize::MAX);
        assert!(pos("status") < pos("type"));
        assert!(pos("type") < pos("priority"));
        assert!(pos("priority") < pos("area"));
        assert!(output.contains("body"), "body must survive formatting");
        // Fixpoint.
        assert_eq!(normalize_frontmatter(&output, 80, false).unwrap(), output);
    }

    #[test]
    fn normalize_preserves_unknown_fields() {
        let input = "---\nstatus: open\ntype: bug\npriority: high\ncustom: value\n---\n";
        let output = normalize_frontmatter(input, 80, false).unwrap();
        assert!(output.contains("custom: value"));
    }

    #[test]
    fn normalize_preserves_body_text() {
        let input = "---\nstatus: open\ntype: bug\npriority: low\n---\n\n## Notes\nSome text.\n";
        let output = normalize_frontmatter(input, 80, false).unwrap();
        assert!(output.contains("## Notes\nSome text."));
    }

    #[test]
    fn normalize_no_body_omits_trailing_blank() {
        let input = "---\nstatus: open\ntype: bug\npriority: low\n---\n";
        let output = normalize_frontmatter(input, 80, false).unwrap();
        assert_eq!(output, "---\nstatus: open\ntype: bug\npriority: low\n---\n");
    }

    #[test]
    fn normalize_already_canonical_is_unchanged() {
        let input = "---\nstatus: open\ntype: bug\npriority: low\narea: backend\n---\n\nbody\n";
        let output = normalize_frontmatter(input, 80, false).unwrap();
        assert_eq!(output, input);
    }

    // --- Result contracts (bool + eprintln -> Result) ---

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
        assert_eq!(
            format_file(&path, 80, false, false).expect("formats"),
            false
        );
        // Scrambled order is fixed: Ok(true).
        std::fs::write(
            &path,
            "---\npriority: high\nstatus: open\ntype: bug\n---\n\nbody\n",
        )
        .unwrap();
        assert_eq!(format_file(&path, 80, false, false).expect("formats"), true);
        // Missing file is an error, not silent false.
        assert!(format_file(&dir.join("missing.md"), 80, false, false).is_err());
    }

    #[test]
    fn format_file_check_does_not_write() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let path = dir.join("a.md");
        std::fs::write(
            &path,
            "---\npriority: high\nstatus: open\ntype: bug\n---\n\nbody\n",
        )
        .unwrap();
        let before = std::fs::read(&path).unwrap();
        assert_eq!(format_file(&path, 80, false, true).expect("checks"), true);
        let after = std::fs::read(&path).unwrap();
        assert_eq!(before, after, "check must not write");
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
        assert_eq!(
            normalize_all(root, 80, false, false).expect("normalizes"),
            1
        );
        assert_eq!(
            normalize_all(root, 80, false, false).expect("idempotent"),
            0
        );
    }
}
