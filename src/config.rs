use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Layered app configuration: defaults < config file < env vars < CLI.
/// See [`Config::try_load`] for precedence and failure contract.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// Repo root containing `.plan` (CLI `--root`; else auto-detected).
    pub root: Option<PathBuf>,
    /// Editor for `open` (CLI/config; else `$EDITOR`, else `vi`).
    pub editor: Option<String>,
    /// Theme lookup dir (else `./themes/`, then platform config dir).
    pub themes_dir: Option<PathBuf>,
    /// View for bare `plan`: `kanban` or `list`.
    pub default_view: String,
    /// Sort keys used when none are given (`priority`, `slug`).
    pub default_sort: Vec<String>,
    /// Hide `done` tasks unless `--show-done`.
    pub hide_done: bool,
    /// Column order for kanban display.
    pub kanban_order: Vec<String>,
    /// Render width for interactive TUI views.
    pub tui_width: usize,
    /// Render width for pager/`open` views.
    pub pager_width: usize,
    /// Render width for piped (filter) output.
    pub width: usize,
    /// Status/priority colors for kanban display.
    pub colors: ColorsConfig,
}

/// Kanban color groups.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ColorsConfig {
    /// Per-status colors.
    pub status: StatusColors,
    /// Per-priority colors.
    pub priority: PriorityColors,
}

/// Color per task status; unknown statuses render white.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StatusColors {
    /// `in-progress` tasks.
    pub in_progress: Color,
    /// `open` tasks.
    pub open: Color,
    /// `blocked` tasks.
    pub blocked: Color,
    /// `backlog` tasks.
    pub backlog: Color,
    /// `idea` tasks.
    pub idea: Color,
    /// `done` tasks.
    pub done: Color,
}

/// Color per priority; unknown priorities render white.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PriorityColors {
    /// `high` priority.
    pub high: Color,
    /// `medium` priority.
    pub medium: Color,
    /// `low` priority.
    pub low: Color,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            root: None,
            editor: None,
            themes_dir: None,
            default_view: "kanban".into(),
            default_sort: vec!["priority".into(), "slug".into()],
            hide_done: true,
            kanban_order: vec![
                "idea".into(),
                "backlog".into(),
                "open".into(),
                "in-progress".into(),
                "blocked".into(),
                "done".into(),
            ],
            tui_width: 80,
            pager_width: 100,
            width: 80,
            colors: ColorsConfig::default(),
        }
    }
}

impl Default for StatusColors {
    fn default() -> Self {
        Self {
            in_progress: Color::Magenta,
            open: Color::Yellow,
            blocked: Color::Red,
            backlog: Color::Blue,
            idea: Color::Cyan,
            done: Color::Green,
        }
    }
}

impl Default for PriorityColors {
    fn default() -> Self {
        Self {
            high: Color::Red,
            medium: Color::Yellow,
            low: Color::DarkGray,
        }
    }
}

/// Validate that `path` exists and contains parseable TOML, returning a
/// human-readable reason on failure. Used to give per-file error messages
/// before handing the file to figment (whose merge errors are lazy).
fn check_toml_file(path: &Path) -> Result<(), String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    toml::from_str::<toml::Value>(&content).map_err(|e| e.to_string())?;
    Ok(())
}

impl Config {
    /// Load configuration with layered precedence:
    /// defaults < config file < env vars < CLI overrides
    ///
    /// `app_name` is used for config file search paths (e.g. "plan" → plan.toml)
    /// `env_prefix` is used for environment variable filtering (e.g. "PLAN_")
    ///
    /// Error contract:
    /// - An explicit `cli_config_path` that is missing, unparseable, or
    ///   type-invalid produces `Err` naming the file — never a silent default.
    /// - Auto-detected config files (env var / project-local / config dir)
    ///   that are bad warn on stderr and are skipped; extraction failures
    ///   fall back to defaults (with a stderr warning).
    pub fn try_load(
        app_name: &str,
        env_prefix: &str,
        cli_config_path: Option<&Path>,
    ) -> Result<Self, String> {
        let mut figment = Figment::from(Serialized::defaults(Config::default()));

        // Layer 1: config file(s)
        if let Some(path) = cli_config_path {
            check_toml_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
            figment = figment.merge(Toml::file(path));
        } else {
            // Env var for config path
            let config_env_key = format!("{env_prefix}CONFIG");
            if let Ok(path) = std::env::var(&config_env_key) {
                let p = PathBuf::from(&path);
                if p.is_file() {
                    match check_toml_file(&p) {
                        Ok(()) => figment = figment.merge(Toml::file(&p)),
                        Err(e) => {
                            eprintln!("warning: ignoring invalid config file {}: {e}", p.display())
                        }
                    }
                } else {
                    eprintln!(
                        "warning: {config_env_key} points to missing config file: {}",
                        p.display()
                    );
                }
            }

            // Project-local
            let local = PathBuf::from(format!("{app_name}.toml"));
            if local.is_file() {
                match check_toml_file(&local) {
                    Ok(()) => figment = figment.merge(Toml::file(&local)),
                    Err(e) => eprintln!(
                        "warning: ignoring invalid config file {}: {e}",
                        local.display()
                    ),
                }
            }

            // Platform config dir
            if let Some(proj_dirs) = directories::ProjectDirs::from("", "", app_name) {
                let sys_config = proj_dirs.config_dir().join("config.toml");
                if sys_config.is_file() {
                    match check_toml_file(&sys_config) {
                        Ok(()) => figment = figment.merge(Toml::file(&sys_config)),
                        Err(e) => eprintln!(
                            "warning: ignoring invalid config file {}: {e}",
                            sys_config.display()
                        ),
                    }
                }
            }
        }

        // Layer 2: environment variables
        figment = figment.merge(Env::prefixed(env_prefix));

        match figment.extract::<Config>() {
            Ok(cfg) => Ok(cfg),
            Err(err) => {
                if let Some(path) = cli_config_path {
                    Err(format!("{}: {err}", path.display()))
                } else {
                    eprintln!("warning: falling back to default config: {err}");
                    Ok(Config::default())
                }
            }
        }
    }
}

/// Resolve a file by checking project-local, then config dir, then data dir.
/// Returns the first path that exists, or `None`.
pub fn resolve_file(
    root: &Path,
    plan_dir: &str,
    local_name: &str,
    global_name: &str,
    app_name: &str,
) -> Option<PathBuf> {
    // Project-local (dot-prefixed)
    let local = root.join(plan_dir).join(local_name);
    if local.exists() {
        return Some(local);
    }

    // Config dir (user)
    if let Some(proj_dirs) = directories::ProjectDirs::from("", "", app_name) {
        let config_file = proj_dirs.config_dir().join(global_name);
        if config_file.exists() {
            return Some(config_file);
        }
    }

    // Data dir (system)
    if let Some(proj_dirs) = directories::ProjectDirs::from("", "", app_name) {
        let data_file = proj_dirs.data_dir().join(global_name);
        if data_file.exists() {
            return Some(data_file);
        }
    }

    None
}

/// Nearest ancestor (or self) containing a `.plan/` dir; falls back to cwd.
/// Defines the workspace for task lookup and local config discovery.
pub fn workspace_root() -> PathBuf {
    let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut candidate = Some(dir.as_path());
    while let Some(path) = candidate {
        if path.join(crate::PLAN_DIR).is_dir() {
            return path.to_path_buf();
        }
        candidate = path.parent();
    }
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_values() {
        let cfg = Config::default();
        assert_eq!(cfg.default_view, "kanban");
        assert_eq!(cfg.width, 80);
        assert_eq!(cfg.tui_width, 80);
        assert_eq!(cfg.pager_width, 100);
        assert!(cfg.root.is_none());
        assert!(cfg.editor.is_none());
    }

    #[test]
    fn config_default_sort() {
        let cfg = Config::default();
        assert_eq!(cfg.default_sort, vec!["priority", "slug"]);
    }

    #[test]
    fn config_default_kanban_order() {
        let cfg = Config::default();
        assert_eq!(
            cfg.kanban_order,
            vec!["idea", "backlog", "open", "in-progress", "blocked", "done"]
        );
    }

    /// Helper: unique temp file path for config tests (no tempfile dep).
    fn temp_config_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tablethat_cfg_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp config dir");
        dir.join(name)
    }

    #[test]
    fn try_load_explicit_missing_file_is_err() {
        let missing = Path::new("/nonexistent/tablethat-should-not-exist.toml");
        let result = Config::try_load("plan", "PLAN_", Some(missing));
        let err = result.expect_err("explicit missing --config path must be an error");
        assert!(
            err.contains("should-not-exist.toml"),
            "error should mention the path, got: {err}"
        );
    }

    #[test]
    fn try_load_explicit_invalid_toml_is_err() {
        let path = temp_config_path("invalid.toml");
        std::fs::write(&path, "this is [[[ not valid toml").expect("write invalid toml");
        let result = Config::try_load("plan", "PLAN_", Some(&path));
        let err = result.expect_err("explicit invalid --config TOML must be an error");
        assert!(
            err.contains("invalid.toml"),
            "error should mention the path, got: {err}"
        );
    }

    #[test]
    fn try_load_explicit_wrong_type_is_err() {
        let path = temp_config_path("wrong_type.toml");
        std::fs::write(&path, "width = \"not-a-number\"").expect("write wrong-type toml");
        let result = Config::try_load("plan", "PLAN_", Some(&path));
        let err = result.expect_err("explicit --config with wrong value type must be an error");
        assert!(
            err.contains("wrong_type.toml"),
            "error should mention the path, got: {err}"
        );
    }

    #[test]
    fn try_load_explicit_valid_toml_is_ok() {
        let path = temp_config_path("valid.toml");
        std::fs::write(&path, "width = 42").expect("write valid toml");
        let cfg = Config::try_load("plan", "PLAN_", Some(&path))
            .expect("explicit valid --config must load");
        assert_eq!(cfg.width, 42);
    }

    #[test]
    fn try_load_auto_bad_toml_warns_and_falls_back_to_defaults() {
        let path = temp_config_path("auto_bad.toml");
        std::fs::write(&path, "this is [[[ not valid toml").expect("write invalid toml");
        // Unique prefix so the env var can't collide with real user config.
        unsafe { std::env::set_var("TESTAUTOBAD_CONFIG", &path) };
        let result = Config::try_load("testautobad", "TESTAUTOBAD_", None);
        unsafe { std::env::remove_var("TESTAUTOBAD_CONFIG") };
        let cfg = result.expect("auto-detected bad TOML must warn, not fail");
        // Bad file skipped => built-in defaults remain.
        assert_eq!(cfg.width, Config::default().width);
    }

    #[test]
    fn try_load_auto_valid_toml_merges() {
        let path = temp_config_path("auto_good.toml");
        std::fs::write(&path, "width = 55").expect("write valid toml");
        unsafe { std::env::set_var("TESTAUTOGOOD_CONFIG", &path) };
        let result = Config::try_load("testautogood", "TESTAUTOGOOD_", None);
        unsafe { std::env::remove_var("TESTAUTOGOOD_CONFIG") };
        let cfg = result.expect("auto-detected valid TOML must load");
        assert_eq!(cfg.width, 55);
    }
}
