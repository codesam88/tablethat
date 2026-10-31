pub mod launch;
pub mod markdown;
pub mod theme;

use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PLAN_DIR: &str = ".plan";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub root: Option<PathBuf>,
    pub editor: Option<String>,
    pub themes_dir: Option<PathBuf>,
    pub default_view: String,
    pub default_sort: Vec<String>,
    pub hide_done: bool,
    pub kanban_order: Vec<String>,
    pub tui_width: usize,
    pub pager_width: usize,
    pub width: usize,
    pub colors: ColorsConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ThemeConfig {
    pub h1_color: Color,
    pub h2_color: Color,
    pub h3_color: Color,
    pub code_color: Color,
    #[serde(default = "default_code_block_color")]
    pub code_block_color: Color,
    pub bold_style: String,
    pub emphasis_style: String,
}

fn default_code_block_color() -> Color {
    Color::Yellow
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ColorsConfig {
    pub status: StatusColors,
    pub priority: PriorityColors,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StatusColors {
    pub in_progress: Color,
    pub open: Color,
    pub blocked: Color,
    pub backlog: Color,
    pub idea: Color,
    pub done: Color,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PriorityColors {
    pub high: Color,
    pub medium: Color,
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

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            h1_color: Color::Cyan,
            h2_color: Color::Cyan,
            h3_color: Color::Cyan,
            code_color: Color::Yellow,
            code_block_color: Color::Yellow,
            bold_style: "bold".into(),
            emphasis_style: "underlined".into(),
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
/// Returns the first path that exists, or None.
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

/// Convert a ratatui Color to a termcolor Color.
pub fn ratatui_to_termcolor(c: Color) -> termcolor::Color {
    match c {
        Color::Black => termcolor::Color::Black,
        Color::Red => termcolor::Color::Red,
        Color::Green => termcolor::Color::Green,
        Color::Yellow => termcolor::Color::Yellow,
        Color::Blue => termcolor::Color::Blue,
        Color::Magenta => termcolor::Color::Magenta,
        Color::Cyan => termcolor::Color::Cyan,
        Color::Gray => termcolor::Color::Ansi256(7),
        Color::DarkGray => termcolor::Color::Ansi256(8),
        Color::White => termcolor::Color::White,
        Color::LightRed => termcolor::Color::Ansi256(9),
        Color::LightGreen => termcolor::Color::Ansi256(10),
        Color::LightYellow => termcolor::Color::Ansi256(11),
        Color::LightBlue => termcolor::Color::Ansi256(12),
        Color::LightMagenta => termcolor::Color::Ansi256(13),
        Color::LightCyan => termcolor::Color::Ansi256(14),
        Color::Indexed(n) => termcolor::Color::Ansi256(n),
        Color::Rgb(r, g, b) => termcolor::Color::Ansi256(rgb_to_ansi256(r, g, b)),
        _ => termcolor::Color::White,
    }
}

fn rgb_to_ansi256(r: u8, g: u8, b: u8) -> u8 {
    let ri = (r as u16 * 5 / 255) as u8;
    let gi = (g as u16 * 5 / 255) as u8;
    let bi = (b as u16 * 5 / 255) as u8;
    16 + 36 * ri + 6 * gi + bi
}

pub fn workspace_root() -> PathBuf {
    let dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut candidate = Some(dir.as_path());
    while let Some(path) = candidate {
        if path.join(PLAN_DIR).is_dir() {
            return path.to_path_buf();
        }
        candidate = path.parent();
    }
    dir
}

pub fn status_color(status: &str, colors: &ColorsConfig) -> ratatui::style::Color {
    match status {
        "in-progress" => colors.status.in_progress,
        "open" => colors.status.open,
        "blocked" => colors.status.blocked,
        "backlog" => colors.status.backlog,
        "idea" => colors.status.idea,
        "done" => colors.status.done,
        _ => Color::White,
    }
}

pub fn priority_color(priority: &str, colors: &ColorsConfig) -> ratatui::style::Color {
    match priority {
        "high" => colors.priority.high,
        "medium" => colors.priority.medium,
        "low" => colors.priority.low,
        _ => Color::White,
    }
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

    #[test]
    fn ratatui_to_termcolor_basic_colors() {
        assert_eq!(ratatui_to_termcolor(Color::Red), termcolor::Color::Red);
        assert_eq!(ratatui_to_termcolor(Color::Green), termcolor::Color::Green);
        assert_eq!(ratatui_to_termcolor(Color::Blue), termcolor::Color::Blue);
        assert_eq!(
            ratatui_to_termcolor(Color::Yellow),
            termcolor::Color::Yellow
        );
        assert_eq!(ratatui_to_termcolor(Color::Cyan), termcolor::Color::Cyan);
        assert_eq!(
            ratatui_to_termcolor(Color::Magenta),
            termcolor::Color::Magenta
        );
        assert_eq!(ratatui_to_termcolor(Color::Black), termcolor::Color::Black);
        assert_eq!(ratatui_to_termcolor(Color::White), termcolor::Color::White);
    }

    #[test]
    fn ratatui_to_termcolor_gray() {
        assert_eq!(
            ratatui_to_termcolor(Color::Gray),
            termcolor::Color::Ansi256(7)
        );
        assert_eq!(
            ratatui_to_termcolor(Color::DarkGray),
            termcolor::Color::Ansi256(8)
        );
    }

    #[test]
    fn ratatui_to_termcolor_indexed() {
        assert_eq!(
            ratatui_to_termcolor(Color::Indexed(42)),
            termcolor::Color::Ansi256(42)
        );
        assert_eq!(
            ratatui_to_termcolor(Color::Indexed(0)),
            termcolor::Color::Ansi256(0)
        );
        assert_eq!(
            ratatui_to_termcolor(Color::Indexed(255)),
            termcolor::Color::Ansi256(255)
        );
    }

    #[test]
    fn ratatui_to_termcolor_rgb() {
        // RGB should convert to an ANSI256 index
        let result = ratatui_to_termcolor(Color::Rgb(255, 0, 0));
        match result {
            termcolor::Color::Ansi256(_) => {} // expected
            _ => panic!("expected Ansi256, got {:?}", result),
        }
    }

    #[test]
    fn status_color_maps_correctly() {
        let colors = ColorsConfig::default();
        assert_eq!(status_color("open", &colors), Color::Yellow);
        assert_eq!(status_color("in-progress", &colors), Color::Magenta);
        assert_eq!(status_color("blocked", &colors), Color::Red);
        assert_eq!(status_color("backlog", &colors), Color::Blue);
        assert_eq!(status_color("idea", &colors), Color::Cyan);
        assert_eq!(status_color("done", &colors), Color::Green);
        assert_eq!(status_color("unknown", &colors), Color::White);
    }

    #[test]
    fn priority_color_maps_correctly() {
        let colors = ColorsConfig::default();
        assert_eq!(priority_color("high", &colors), Color::Red);
        assert_eq!(priority_color("medium", &colors), Color::Yellow);
        assert_eq!(priority_color("low", &colors), Color::DarkGray);
        assert_eq!(priority_color("unknown", &colors), Color::White);
    }

    #[test]
    fn theme_config_defaults() {
        let theme = ThemeConfig::default();
        assert_eq!(theme.h1_color, Color::Cyan);
        assert_eq!(theme.h2_color, Color::Cyan);
        assert_eq!(theme.h3_color, Color::Cyan);
        assert_eq!(theme.code_color, Color::Yellow);
        assert_eq!(theme.code_block_color, Color::Yellow);
        assert_eq!(theme.bold_style, "bold");
        assert_eq!(theme.emphasis_style, "underlined");
    }

    // ---- try_load tests ----------------------------------------------------

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
