//! Shared process-launch helpers: `$EDITOR` command splitting and `gloss`
//! binary resolution. Used by the `plan` and `gloss` binaries via the lib.
//!
//! No-shell invariant: these helpers never invoke a shell; they only split a
//! command string into program + args for `std::process::Command`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Split an `$EDITOR` command string into `(program, args)` without invoking
/// a shell. Handles quoting/escaping (`"my editor" --wait` keeps the embedded
/// space). Falls back to `("vi", [])` when the raw string is empty,
/// whitespace-only, or fails to parse.
///
/// ```
/// # use tablethat_lib::launch::split_editor_cmd;
/// assert_eq!(split_editor_cmd("code --wait"), ("code".into(), vec!["--wait".into()]));
/// assert_eq!(split_editor_cmd(""), ("vi".into(), Vec::new()));
/// ```
pub fn split_editor_cmd(raw: &str) -> (String, Vec<String>) {
    match shell_words::split(raw) {
        Ok(mut parts) if !parts.is_empty() => {
            let prog = parts.remove(0);
            (prog, parts)
        }
        _ => ("vi".to_string(), Vec::new()),
    }
}

/// Read `$EDITOR` and split it with [`split_editor_cmd`]. Unset/empty/
/// whitespace-only/invalid values fall back to `vi`.
pub fn editor_command() -> (String, Vec<String>) {
    let raw = std::env::var("EDITOR").unwrap_or_default();
    split_editor_cmd(&raw)
}

/// Resolve the `gloss` binary: prefer a sibling of the currently running
/// executable (`plan` and `gloss` ship in the same directory via
/// `cargo install` and in `target/*/debug|release`), otherwise fall back to
/// a bare `gloss` resolved via `PATH`.
pub fn resolve_gloss_program() -> OsString {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    resolve_gloss_program_in(exe_dir.as_deref())
}

/// Pure resolution core: given the current exe's directory (if known),
/// return `<exe_dir>/gloss{EXE_SUFFIX}` when that file exists, else the
/// bare `gloss` program name for `PATH` lookup.
fn resolve_gloss_program_in(exe_dir: Option<&Path>) -> OsString {
    let sibling: Option<PathBuf> =
        exe_dir.map(|dir| dir.join(format!("gloss{}", std::env::consts::EXE_SUFFIX)));
    match sibling {
        Some(path) if path.is_file() => path.into_os_string(),
        _ => OsString::from("gloss"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // ---- split_editor_cmd ----

    #[test]
    fn split_editor_cmd_simple_flags() {
        let (prog, args) = split_editor_cmd("code --wait");
        assert_eq!(prog, "code");
        assert_eq!(args, vec!["--wait"]);
    }

    #[test]
    fn split_editor_cmd_quoted_program_keeps_embedded_space() {
        let (prog, args) = split_editor_cmd("\"my editor\" --wait");
        assert_eq!(prog, "my editor");
        assert_eq!(args, vec!["--wait"]);
    }

    #[test]
    fn split_editor_cmd_multiple_args() {
        let (prog, args) = split_editor_cmd("emacs -nw --no-init-file");
        assert_eq!(prog, "emacs");
        assert_eq!(args, vec!["-nw", "--no-init-file"]);
    }

    #[test]
    fn split_editor_cmd_empty_falls_back_to_vi() {
        assert_eq!(split_editor_cmd(""), ("vi".to_string(), Vec::new()));
    }

    #[test]
    fn split_editor_cmd_whitespace_only_falls_back_to_vi() {
        assert_eq!(split_editor_cmd("  \t "), ("vi".to_string(), Vec::new()));
    }

    #[test]
    fn split_editor_cmd_parse_error_falls_back_to_vi() {
        // Unterminated quote is a shell-words parse error.
        assert_eq!(
            split_editor_cmd("code \"unterminated"),
            ("vi".to_string(), Vec::new())
        );
    }

    // ---- gloss resolution (pure: temp dirs only, no process/env mutation) ----

    #[test]
    fn resolve_gloss_prefers_sibling_of_current_exe() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let sibling = dir
            .path()
            .join(format!("gloss{}", std::env::consts::EXE_SUFFIX));
        fs::write(&sibling, b"fake gloss binary").expect("write sibling");
        let resolved = resolve_gloss_program_in(Some(dir.path()));
        assert_eq!(resolved, sibling.into_os_string());
    }

    #[test]
    fn resolve_gloss_falls_back_to_path_when_sibling_missing() {
        let dir = tempfile::tempdir().expect("create tempdir");
        // Empty dir: no gloss binary next to the exe.
        let resolved = resolve_gloss_program_in(Some(dir.path()));
        assert_eq!(resolved, std::ffi::OsString::from("gloss"));
    }

    #[test]
    fn resolve_gloss_falls_back_to_path_when_exe_dir_unknown() {
        let resolved = resolve_gloss_program_in(None);
        assert_eq!(resolved, std::ffi::OsString::from("gloss"));
    }
}
