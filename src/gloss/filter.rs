use std::io::{self, Read, Write};
use tablethat_lib::markdown;
use tablethat_lib::theme::ThemeFile;
use termcolor::{ColorChoice, ColorSpec, StandardStream, WriteColor};

/// Run in filter mode: read markdown from stdin, render styled output to stdout.
pub fn run_filter(theme: &ThemeFile, no_color: bool, width: usize) {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .expect("failed to read stdin");

    let md_theme = markdown::theme_from_cfg(&theme.theme);
    let lines = markdown::render_markdown(&md_theme, &input, width);

    let is_tty = atty_check();
    let use_color = is_tty && !no_color;

    if use_color {
        let mut stdout = StandardStream::stdout(ColorChoice::Auto);
        for line in &lines {
            for span in &line.spans {
                let spec = ratatui_style_to_colorspec(span.style);
                let _ = stdout.set_color(&spec);
                let _ = write!(stdout, "{}", span.content);
            }
            let _ = stdout.reset();
            let _ = writeln!(stdout);
        }
    } else {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        for line in &lines {
            for span in &line.spans {
                let _ = out.write_all(span.content.as_ref().as_bytes());
            }
            let _ = out.write_all(b"\n");
        }
    }
}

fn atty_check() -> bool {
    std::io::IsTerminal::is_terminal(&std::io::stdout())
}

fn ratatui_style_to_colorspec(style: ratatui::style::Style) -> ColorSpec {
    let mut spec = ColorSpec::new();
    if let Some(fg) = style.fg {
        spec.set_fg(Some(ratatui_color_to_termcolor(fg)));
    }
    if style.add_modifier.contains(ratatui::style::Modifier::BOLD) {
        spec.set_bold(true);
    }
    if style.add_modifier.contains(ratatui::style::Modifier::DIM) {
        spec.set_dimmed(true);
    }
    if style
        .add_modifier
        .contains(ratatui::style::Modifier::ITALIC)
    {
        spec.set_italic(true);
    }
    if style
        .add_modifier
        .contains(ratatui::style::Modifier::UNDERLINED)
    {
        spec.set_underline(true);
    }
    spec
}

fn ratatui_color_to_termcolor(c: ratatui::style::Color) -> termcolor::Color {
    tablethat_lib::ratatui_to_termcolor(c)
}
