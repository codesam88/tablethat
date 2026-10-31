use super::model::parse_frontmatter;
use super::store::read_task_files;
use crate as lib;

/// Validate every task file against the resolved schema.
///
/// Diagnostics are returned, never printed: `Ok(())` means clean,
/// `Err` carries one line per failure. Non-fatal notes (unknown keys,
/// empty tree) accumulate in `warnings` either way.
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
}
