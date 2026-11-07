use clap::Parser;
use std::path::PathBuf;
use tablethat_lib as lib;

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

    /// Base wrap width for markdown rendering.
    /// Base = -w, or live terminal width minus pad/scrollbar/gap, else 80.
    /// Prose is clamped to 40-120 columns; tables to 60-160.
    #[arg(short, long, value_name = "COLS")]
    width: Option<usize>,

    /// Hide the scrollbar in TUI mode.
    #[arg(long)]
    no_scrollbar: bool,

    /// Use ASCII table borders instead of Unicode glyphs.
    #[arg(long)]
    ascii: bool,
}

fn run(cli: Cli) -> Result<(), String> {
    let cfg = lib::Config::try_load("gloss", "GLOSS_", cli.config.as_deref())?;
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
    let no_color = cli.no_color;
    let scrollbar = !cli.no_scrollbar;
    let ascii = cli.ascii;

    match cli.path {
        // TUI mode: file or directory argument provided, or no arg but tty
        Some(path) if path.is_file() => {
            tablethat_lib::gloss::tui::run_file_viewer(
                &path, &cfg, &themes, 0, width, scrollbar, ascii,
            )
            .map_err(err_chain)?;
            Ok(())
        }
        Some(path) if path.is_dir() => {
            tablethat_lib::gloss::tui::run_directory_browser(
                &path, &cfg, &themes, width, scrollbar, ascii,
            )
            .map_err(err_chain)?;
            Ok(())
        }
        Some(path) => Err(format!("{}: not a file or directory", path.display())),
        // Filter mode: stdin piped
        None if !is_tty => {
            tablethat_lib::gloss::filter::run_filter(&theme, no_color, width, ascii)?;
            Ok(())
        }
        // No arg + tty: browse .md files in cwd
        None => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            tablethat_lib::gloss::tui::run_directory_browser(
                &cwd, &cfg, &themes, width, scrollbar, ascii,
            )
            .map_err(err_chain)?;
            Ok(())
        }
    }
}

fn err_chain(e: Box<dyn std::error::Error>) -> String {
    let mut msg = e.to_string();
    let mut cur = e.source();
    while let Some(s) = cur {
        msg.push_str(": ");
        msg.push_str(&s.to_string());
        cur = s.source();
    }
    msg
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("gloss: {e}");
        std::process::exit(1);
    }
}

fn is_tty() -> bool {
    std::io::IsTerminal::is_terminal(&std::io::stdin())
}
