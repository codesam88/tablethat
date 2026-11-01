use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Markdown rendering theme (colors + emphasis modifiers).
/// Loaded from `themes/*.toml`; see [`ThemeFile`].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ThemeConfig {
    /// Level-1 heading color (rendered bold + underlined).
    pub h1_color: Color,
    /// Level-2 heading color (rendered underlined).
    pub h2_color: Color,
    /// Level-3+ heading color.
    pub h3_color: Color,
    /// Inline-code color.
    pub code_color: Color,
    /// Fenced-code-block color.
    #[serde(default = "default_code_block_color")]
    pub code_block_color: Color,
    /// Style for strong text: `bold`, `dim`, `italic`, `underlined`.
    pub bold_style: String,
    /// Style for emphasis text (same vocabulary; default `underlined`).
    pub emphasis_style: String,
}

fn default_code_block_color() -> Color {
    Color::Yellow
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

/// A theme loaded from a TOML file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ThemeFile {
    /// Theme name (selectable via `--theme`).
    pub name: String,
    /// Rendering theme.
    pub theme: ThemeConfig,
}

/// Discover and load theme TOML files from a directory.
/// Searches in order:
/// 1. Explicit `themes_dir` path
/// 2. `./themes/` relative to cwd
/// 3. `~/.config/<app_name>/themes/`
/// 4. System dirs (`/usr/local/share`, `/usr/share`) for packaged installs.
///
/// Thin wrapper over [`load_themes_with_errors`] that warns on stderr for
/// each skipped file.
pub fn load_themes(themes_dir: Option<&Path>, app_name: &str) -> Vec<ThemeFile> {
    let (themes, errors) = load_themes_with_errors(themes_dir, app_name);
    for (path, reason) in &errors {
        eprintln!(
            "warning: skipping invalid theme file {}: {reason}",
            path.display()
        );
    }
    themes
}

/// Shared user-local themes dir for tarball installs:
/// `$XDG_DATA_HOME/tablethat/themes`, else `~/.local/share/...`.
/// Pure helper so the env lookup stays testable without mutating `HOME`.
fn user_share_themes_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("tablethat/themes")
}

/// System data dirs per the XDG spec: `$XDG_DATA_DIRS`, defaulting to
/// `/usr/local/share:/usr/share`. Empty segments are skipped. Pure for
/// testability; callers pass `env::var("XDG_DATA_DIRS").ok()`.
fn xdg_data_dirs(raw: Option<&str>) -> Vec<PathBuf> {
    raw.unwrap_or("/usr/local/share:/usr/share")
        .split(':')
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Candidate theme directories in priority order: explicit dir, else
/// project `./themes/`, user config, user share, then system locations.
fn theme_search_dirs(themes_dir: Option<&Path>, app_name: &str) -> Vec<PathBuf> {
    if let Some(dir) = themes_dir {
        return vec![dir.to_path_buf()];
    }
    let mut candidates = vec![PathBuf::from("themes")];
    if let Some(proj_dirs) = directories::ProjectDirs::from("", "", app_name) {
        candidates.push(proj_dirs.config_dir().join("themes"));
    }
    // User-local share (PREFIX=$HOME/.local installs): honors
    // $XDG_DATA_HOME, falls back to ~/.local/share.
    if let Some(base) = directories::BaseDirs::new() {
        candidates.push(user_share_themes_dir(base.data_dir()));
    }
    // System locations for deb/rpm installs (read-only, lowest priority).
    candidates.extend(
        xdg_data_dirs(std::env::var("XDG_DATA_DIRS").ok().as_deref())
            .iter()
            .map(|d| d.join("tablethat/themes")),
    );
    candidates
}
/// Like [`load_themes`], but also returns the files that failed to load
/// as `(path, reason)` pairs instead of only warning about them.
pub fn load_themes_with_errors(
    themes_dir: Option<&Path>,
    app_name: &str,
) -> (Vec<ThemeFile>, Vec<(PathBuf, String)>) {
    let dirs = theme_search_dirs(themes_dir, app_name);

    let mut themes = Vec::new();
    let mut errors = Vec::new();
    for dir in &dirs {
        if !dir.is_dir() {
            continue;
        }
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "toml"))
            .collect();
        entries.sort();

        for path in entries {
            match std::fs::read_to_string(&path) {
                Ok(content) => match toml::from_str::<ThemeFile>(&content) {
                    Ok(theme) => themes.push(theme),
                    Err(e) => errors.push((path, e.to_string())),
                },
                Err(e) => errors.push((path, e.to_string())),
            }
        }
        if !themes.is_empty() {
            break;
        }
    }

    if themes.is_empty() {
        themes.push(ThemeFile {
            name: "default".into(),
            theme: ThemeConfig::default(),
        });
    }

    (themes, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_themes_from_dir() {
        let themes = load_themes(Some(Path::new("themes")), "plan");
        assert!(
            !themes.is_empty(),
            "should find themes in themes/ directory"
        );
        // Should find at least the built-in themes
        assert!(
            themes.len() >= 10,
            "expected at least 10 themes, got {}",
            themes.len()
        );
    }

    #[test]
    fn load_themes_includes_default() {
        let themes = load_themes(Some(Path::new("themes")), "plan");
        let has_default = themes.iter().any(|t| t.name == "default");
        assert!(has_default, "should include default theme");
    }

    #[test]
    fn load_themes_fallback_to_default() {
        let themes = load_themes(Some(Path::new("/nonexistent/path")), "plan");
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].name, "default");
    }

    #[test]
    fn load_themes_with_errors_keeps_good_and_reports_bad() {
        let dir = std::env::temp_dir().join(format!("tablethat_theme_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp themes dir");

        // One well-formed theme file.
        let good = ThemeFile {
            name: "good".into(),
            theme: ThemeConfig::default(),
        };
        let good_path = dir.join("good.toml");
        std::fs::write(&good_path, toml::to_string(&good).expect("serialize theme"))
            .expect("write good theme");

        // One malformed theme file.
        let bad_path = dir.join("bad.toml");
        std::fs::write(&bad_path, "this is [[[ not valid toml").expect("write bad theme");

        let (themes, errors) = load_themes_with_errors(Some(&dir), "plan");

        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            themes.iter().any(|t| t.name == "good"),
            "good theme should load, got: {:?}",
            themes.iter().map(|t| &t.name).collect::<Vec<_>>()
        );
        assert_eq!(
            errors.len(),
            1,
            "bad theme should be reported once: {errors:?}"
        );
        assert_eq!(
            errors[0].0, bad_path,
            "reported path should be the bad file"
        );
        assert!(!errors[0].1.is_empty(), "error should carry a reason");
    }

    #[test]
    fn theme_file_has_required_fields() {
        let themes = load_themes(Some(Path::new("themes")), "plan");
        for theme in &themes {
            assert!(!theme.name.is_empty(), "theme name should not be empty");
            // All themes should have valid colors (they deserialize via ratatui Color)
            let _ = theme.theme.h1_color;
            let _ = theme.theme.h2_color;
            let _ = theme.theme.h3_color;
            let _ = theme.theme.code_color;
            let _ = theme.theme.code_block_color;
        }
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
}

#[cfg(test)]
mod share_dir_tests {
    use super::*;

    #[test]
    fn user_share_dir_nests_tablethat_themes() {
        assert_eq!(
            user_share_themes_dir(Path::new("/home/u/.local/share")),
            PathBuf::from("/home/u/.local/share/tablethat/themes")
        );
    }
}

#[cfg(test)]
mod share_discovery_tests {
    use super::*;

    #[test]
    fn xdg_data_dirs_defaults_and_skips_empties() {
        assert_eq!(
            xdg_data_dirs(None),
            vec![
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share")
            ]
        );
        assert_eq!(
            xdg_data_dirs(Some("/opt/share::/srv/share")),
            vec![PathBuf::from("/opt/share"), PathBuf::from("/srv/share")]
        );
        assert!(xdg_data_dirs(Some("")).is_empty());
    }

    #[test]
    fn search_order_is_xdg_user_then_global() {
        // Guarded env: only this test writes XDG_DATA_DIRS, nothing else reads it.
        let saved = std::env::var_os("XDG_DATA_DIRS");
        unsafe {
            std::env::remove_var("XDG_DATA_DIRS");
        }
        let dirs = theme_search_dirs(None, "nosuchapp38472");
        match saved {
            Some(v) => unsafe { std::env::set_var("XDG_DATA_DIRS", v) },
            None => {}
        }
        let names: Vec<String> = dirs
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        assert_eq!(names[0], "themes");
        let user = names
            .iter()
            .position(|p| p.ends_with("tablethat/themes") && !p.starts_with("/usr/"))
            .expect("user share candidate");
        let tail = &names[user + 1..];
        assert_eq!(
            tail,
            &[
                "/usr/local/share/tablethat/themes",
                "/usr/share/tablethat/themes"
            ],
            "system entries last, XDG order: {names:?}"
        );
    }

    #[test]
    fn explicit_dir_short_circuits_search() {
        let dirs = theme_search_dirs(Some(Path::new("/x")), "whatever");
        assert_eq!(dirs, vec![PathBuf::from("/x")]);
    }
}
