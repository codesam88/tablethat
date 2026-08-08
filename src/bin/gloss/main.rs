use clap::Parser;
use std::path::PathBuf;
use tablethat_lib as lib;

#[path = "../../gloss/filter.rs"]
mod filter;
#[path = "../../gloss/tui_preview.rs"]
mod tui_preview;

#[derive(Parser)]
#[command(
    name = "gloss",
    version,
    max_term_width = 80,
    about = "Markdown viewer — filter mode for vim/pagers, TUI for browsing"
)]
struct Cli {
    /// File or directory to view (TUI mode). If omitted, reads stdin (filter mode).
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,

    /// Path to config file (default: auto-detect)
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Disable colored output in filter mode
    #[arg(long)]
    no_color: bool,

    /// Theme name to use (overrides default)
    #[arg(long, value_name = "NAME")]
    theme: Option<String>,

    /// Wrap width for markdown rendering (default: 80)
    #[arg(short, long, value_name = "COLS")]
    width: Option<usize>,
}

fn main() {
    let cli = Cli::parse();

    let cfg = lib::Config::load("gloss", "GLOSS_", cli.config.as_deref());
    let themes = lib::theme::load_themes(cfg.themes_dir.as_deref(), "gloss");
    let default_width = crossterm::terminal::size()
        .map(|(cols, _)| cols as usize)
        .unwrap_or(80);
    let width = cli.width.unwrap_or(default_width);

    // Select theme
    let theme = if let Some(ref name) = cli.theme {
        themes
            .iter()
            .find(|t| t.name == *name)
            .unwrap_or(&themes[0])
            .clone()
    } else {
        themes[0].clone()
    };

    let is_tty = is_tty();

    match cli.path {
        // TUI mode: file or directory argument provided, or no arg but tty
        Some(path) if path.is_file() => {
            tui_preview::run_file_viewer(&path, &cfg, &themes, 0, width);
        }
        Some(path) if path.is_dir() => {
            tui_preview::run_directory_browser(&path, &cfg, &themes, width);
        }
        Some(path) => {
            eprintln!("gloss: {}: not a file or directory", path.display());
            std::process::exit(1);
        }
        // Filter mode: stdin piped
        None if !is_tty => {
            filter::run_filter(&theme, cli.no_color, width);
        }
        // No arg + tty: browse .md files in cwd
        None => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            tui_preview::run_directory_browser(&cwd, &cfg, &themes, width);
        }
    }
}

fn is_tty() -> bool {
    std::io::IsTerminal::is_terminal(&std::io::stdin())
}
