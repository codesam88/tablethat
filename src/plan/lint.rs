use super::model::parse_frontmatter;
use super::store::{read_task_files, sorted_md_entries};
use crate as lib;

/// Parsed schema context used for schema-dependent checks.
struct SchemaCtx {
    required: Vec<String>,
    properties: Option<serde_json::Map<String, serde_json::Value>>,
}

fn load_schema(path: &std::path::Path) -> Result<SchemaCtx, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("{}: cannot read — {e}", path.display()))?;
    let schema: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("{}: invalid JSON — {e}", path.display()))?;
    let required: Vec<String> = schema["required"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let properties = schema["properties"].as_object().cloned();
    Ok(SchemaCtx {
        required,
        properties,
    })
}

fn resolve_adjacent_schema(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let candidates = [
        dir.join(".schema.json"),
        dir.join(".plan").join(".schema.json"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

/// Lint a single markdown file. Returns the list of errors (empty = clean).
/// Schema-dependent checks are skipped when `schema` is `None`.
fn lint_one(
    path: &std::path::Path,
    schema: Option<&SchemaCtx>,
    warnings: &mut Vec<String>,
) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            errors.push(format!("{}: FAIL — cannot read: {e}", path.display()));
            return errors;
        }
    };

    // Schemaless mode: a file with no frontmatter (does not start with `---`
    // after trim_start) has nothing to validate — skip silently. Schema mode
    // keeps today's behavior (fall through to parse_frontmatter).
    if schema.is_none() && !content.trim_start().starts_with("---") {
        return errors;
    }

    let (frontmatter, line) = match parse_frontmatter(&content) {
        Ok(fm) => fm,
        Err(msg) => {
            errors.push(format!("{}: FAIL — {msg}", path.display()));
            return errors;
        }
    };

    let parsed: serde_json::Value = match serde_saphyr::from_str(&frontmatter) {
        Ok(v) => v,
        Err(e) => {
            errors.push(format!("{}: FAIL — YAML parse error: {e}", path.display()));
            return errors;
        }
    };

    let Some(obj) = parsed.as_object() else {
        errors.push(format!(
            "{}: FAIL — frontmatter must be a YAML mapping",
            path.display()
        ));
        return errors;
    };

    if let Some(ctx) = schema {
        for field in &ctx.required {
            if !obj.contains_key(field) {
                errors.push(format!(
                    "{}: FAIL — missing required field '{field}'",
                    path.display()
                ));
            }
        }

        if let Some(props) = &ctx.properties {
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
                !ctx.required.contains(k)
                    && !ctx
                        .properties
                        .as_ref()
                        .is_some_and(|p| p.contains_key(k.as_str()))
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

    errors
}

/// Validate every task file in the `.plan` directory of `root` against the
/// resolved schema (project-local, then config/data dirs).
///
/// Diagnostics are returned, never printed: `Ok(())` means clean,
/// `Err` carries one line per failure. Non-fatal notes (unknown keys,
/// empty tree, missing schema) accumulate in `warnings` either way.
///
/// A missing schema degrades to a warning plus schema-independent checks
/// (delimiter present, YAML parses, frontmatter is a mapping); required-field,
/// enum, and unknown-key checks are skipped.
pub fn validate_all(root: &std::path::Path, warnings: &mut Vec<String>) -> Result<(), Vec<String>> {
    let tasks_dir = root.join(lib::PLAN_DIR);

    let schema_path = lib::resolve_file(root, ".plan", ".schema.json", "schema.json", "plan");
    let schema: Option<SchemaCtx> = match schema_path {
        Some(p) => match load_schema(&p) {
            Ok(s) => Some(s),
            Err(e) => return Err(vec![e]),
        },
        None => {
            warnings.push(
                "warning: no schema found (searched .plan/.schema.json, config dir, data dir)"
                    .into(),
            );
            None
        }
    };

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
        errors.extend(lint_one(path, schema.as_ref(), warnings));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Lint every `.md` file in `dir` (foreign directory). Schema is searched in
/// adjacent locations only (`.schema.json`, `.plan/.schema.json` under `dir`);
/// no global/config/data fallback. A missing schema degrades to a warning.
pub fn validate_dir(dir: &std::path::Path, warnings: &mut Vec<String>) -> Result<(), Vec<String>> {
    let schema_path = resolve_adjacent_schema(dir);
    let schema: Option<SchemaCtx> = match schema_path {
        Some(p) => match load_schema(&p) {
            Ok(s) => Some(s),
            Err(e) => return Err(vec![e]),
        },
        None => {
            warnings.push(format!(
                "warning: no schema found (searched {}, {})",
                dir.join(".schema.json").display(),
                dir.join(".plan").join(".schema.json").display()
            ));
            None
        }
    };

    let entries = match sorted_md_entries(dir) {
        Ok(v) => v,
        Err(e) => return Err(vec![e]),
    };

    if entries.is_empty() {
        warnings.push(format!(
            "warning: no markdown files found in {}",
            dir.display()
        ));
    }

    let mut errors: Vec<String> = Vec::new();

    for path in &entries {
        errors.extend(lint_one(path, schema.as_ref(), warnings));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Lint a single markdown file. Schema is searched in adjacent locations only
/// (under the file's parent directory). A missing schema degrades to a warning.
/// A non-`.md` path returns an error naming the path.
pub fn validate_file(
    file: &std::path::Path,
    warnings: &mut Vec<String>,
) -> Result<(), Vec<String>> {
    if !file.extension().is_some_and(|e| e == "md") {
        return Err(vec![format!("{}: not a markdown file", file.display())]);
    }

    let parent = file.parent().unwrap_or_else(|| std::path::Path::new("."));

    let schema_path = resolve_adjacent_schema(parent);
    let schema: Option<SchemaCtx> = match schema_path {
        Some(p) => match load_schema(&p) {
            Ok(s) => Some(s),
            Err(e) => return Err(vec![e]),
        },
        None => {
            warnings.push(format!(
                "warning: no schema found (searched {}, {})",
                parent.join(".schema.json").display(),
                parent.join(".plan").join(".schema.json").display()
            ));
            None
        }
    };

    let mut errors: Vec<String> = Vec::new();
    errors.extend(lint_one(file, schema.as_ref(), warnings));

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Dispatch helper: lint a file or directory path.
pub fn validate_path(
    path: &std::path::Path,
    warnings: &mut Vec<String>,
) -> Result<(), Vec<String>> {
    if path.is_dir() {
        validate_dir(path, warnings)
    } else if path.is_file() {
        validate_file(path, warnings)
    } else {
        Err(vec![format!("{}: not a file or directory", path.display())])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn plan_root_without_schema() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join(".plan")).unwrap();
        tmp
    }

    #[test]
    fn lint_no_schema_returns_ok() {
        let tmp = plan_root_without_schema();
        let root = tmp.path();
        std::fs::write(root.join(".plan/ok.md"), "---\nfoo: bar\n---\n\nbody\n").unwrap();
        let mut warnings = Vec::new();
        validate_all(root, &mut warnings)
            .expect("schemaless tree with valid frontmatter validates");
    }

    #[test]
    fn lint_no_schema_malformed_frontmatter_errors() {
        let tmp = plan_root_without_schema();
        let root = tmp.path();
        std::fs::write(root.join(".plan/bad.md"), "---\nstatus: open\n").unwrap();
        let mut warnings = Vec::new();
        let err = validate_all(root, &mut warnings).expect_err("unclosed frontmatter must error");
        assert!(
            err.iter().any(|e| e.contains("unclosed")),
            "error must name the unclosed delimiter: {err:?}"
        );
    }

    #[test]
    fn lint_no_schema_non_mapping_frontmatter_errors() {
        let tmp = plan_root_without_schema();
        let root = tmp.path();
        std::fs::write(root.join(".plan/list.md"), "---\n- item\n---\n\nbody\n").unwrap();
        let mut warnings = Vec::new();
        let err =
            validate_all(root, &mut warnings).expect_err("non-mapping frontmatter must error");
        assert!(
            err.iter().any(|e| e.contains("must be a YAML mapping")),
            "error must name the mapping requirement: {err:?}"
        );
    }

    #[test]
    fn lint_no_schema_no_frontmatter_skipped() {
        let tmp = plan_root_without_schema();
        let root = tmp.path();
        std::fs::write(
            root.join(".plan/nofm.md"),
            "# Title\n\nBody, no delimiters.\n",
        )
        .unwrap();
        std::fs::write(root.join(".plan/thematic.md"), "Intro\n\n---\n\nMore\n").unwrap();
        let mut warnings = Vec::new();
        validate_all(root, &mut warnings)
            .expect("schemaless tree with no-frontmatter files validates");
    }
}
