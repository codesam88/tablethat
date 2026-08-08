use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use crossterm::terminal;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::path::{Path, PathBuf};
use tablethat_lib as lib;

struct App {
    files: Vec<PathBuf>,
    selected: usize,
    content: Vec<Line<'static>>,
    raw_lines: Vec<String>,
    scroll: usize,
    offset: usize,
    width: usize,
    themes: Vec<lib::theme::ThemeFile>,
    current_theme: usize,
    quit: bool,
    search_mode: bool,
    search_query: String,
    search_hits: Vec<usize>,
    search_index: usize,
    highlight_style: Style,
    highlight_current_style: Style,
    viewport_height: usize,
}

impl App {
    fn new(files: Vec<PathBuf>, themes: Vec<lib::theme::ThemeFile>, width: usize) -> Self {
        let mut app = Self {
            files,
            selected: 0,
            content: Vec::new(),
            raw_lines: Vec::new(),
            scroll: 0,
            offset: 0,
            width,
            themes,
            current_theme: 0,
            quit: false,
            search_mode: false,
            search_query: String::new(),
            search_hits: Vec::new(),
            search_index: 0,
            highlight_style: Style::default()
                .fg(Color::Black)
                .bg(Color::Gray)
                .add_modifier(Modifier::BOLD),
            highlight_current_style: Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            viewport_height: 1,
        };
        app.load_selected();
        app
    }

    fn load_selected(&mut self) {
        if let Some(path) = self.files.get(self.selected)
            && let Ok(text) = std::fs::read_to_string(path)
        {
            self.raw_lines = text.lines().map(|l| l.to_string()).collect();
            let theme_cfg = self
                .themes
                .get(self.current_theme)
                .map(|t| &t.theme)
                .unwrap_or_else(|| {
                    static DEFAULT: std::sync::OnceLock<lib::ThemeConfig> =
                        std::sync::OnceLock::new();
                    DEFAULT.get_or_init(lib::ThemeConfig::default)
                });
            let th = lib::markdown::theme_from_cfg(theme_cfg);
            self.content = lib::markdown::render_markdown(&th, &text, self.width);
            self.scroll = 0;
            self.offset = 0;
            self.search_hits.clear();
            self.search_index = 0;
        }
    }

    fn highlight_matches(&self, lines: &[Line<'static>]) -> Vec<Line<'static>> {
        if self.search_query.is_empty() {
            return lines.to_vec();
        }
        let q = self.search_query.to_lowercase();
        let mut global_match = 0usize;
        lines
            .iter()
            .map(|line| {
                let mut new_spans: Vec<Span<'static>> = Vec::new();
                for span in &line.spans {
                    let text: &str = &span.content;
                    let lower = text.to_lowercase();
                    let style = span.style;
                    let mut last = 0;
                    for (idx, _) in lower.match_indices(&q) {
                        if idx > last {
                            new_spans.push(Span::styled(text[last..idx].to_string(), style));
                        }
                        let hl = if global_match == self.search_index {
                            self.highlight_current_style
                        } else {
                            self.highlight_style
                        };
                        new_spans.push(Span::styled(text[idx..idx + q.len()].to_string(), hl));
                        global_match += 1;
                        last = idx + q.len();
                    }
                    if last < text.len() {
                        new_spans.push(Span::styled(text[last..].to_string(), style));
                    }
                }
                Line::from(new_spans)
            })
            .collect()
    }

    fn apply_search_highlight(&mut self) {
        let theme_cfg = self
            .themes
            .get(self.current_theme)
            .map(|t| &t.theme)
            .unwrap_or_else(|| {
                static DEFAULT: std::sync::OnceLock<lib::ThemeConfig> = std::sync::OnceLock::new();
                DEFAULT.get_or_init(lib::ThemeConfig::default)
            });
        let th = lib::markdown::theme_from_cfg(theme_cfg);
        if let Some(path) = self.files.get(self.selected)
            && let Ok(text) = std::fs::read_to_string(path)
        {
            let base = lib::markdown::render_markdown(&th, &text, self.width);
            self.content = self.highlight_matches(&base);
        }
    }

    fn run_search(&mut self) {
        self.search_hits.clear();
        self.search_index = 0;
        let q = self.search_query.to_lowercase();
        if q.is_empty() {
            return;
        }
        for (i, line) in self.content.iter().enumerate() {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            let lower = text.to_lowercase();
            for _ in lower.match_indices(&q) {
                self.search_hits.push(i);
            }
        }
        if !self.search_hits.is_empty() {
            self.scroll_to_match(0);
            self.apply_search_highlight();
        }
    }

    fn search_next(&mut self) {
        if self.search_hits.is_empty() {
            return;
        }
        self.search_index = (self.search_index + 1) % self.search_hits.len();
        self.scroll_to_match(self.search_index);
        self.apply_search_highlight();
    }

    fn search_prev(&mut self) {
        if self.search_hits.is_empty() {
            return;
        }
        self.search_index = self.search_index.wrapping_sub(1) % self.search_hits.len();
        self.scroll_to_match(self.search_index);
        self.apply_search_highlight();
    }

    fn scroll_to_match(&mut self, index: usize) {
        if let Some(&line) = self.search_hits.get(index) {
            self.scroll = line.saturating_sub(6);
        }
    }

    fn current_file(&self) -> Option<&Path> {
        self.files.get(self.selected).map(|p| p.as_path())
    }

    fn max_line_width(&self) -> usize {
        self.content
            .iter()
            .map(|line| line.spans.iter().map(|s| s.content.len()).sum::<usize>())
            .max()
            .unwrap_or(0)
    }

    fn resize_to_terminal(&mut self) {
        if let Ok((cols, _)) = terminal::size() {
            let new_width = cols as usize;
            if new_width != self.width {
                self.width = new_width;
                self.load_selected();
            }
        }
    }
}

/// Run single-file TUI viewer
pub fn run_file_viewer(
    path: &Path,
    cfg: &lib::Config,
    themes: &[lib::theme::ThemeFile],
    initial_theme: usize,
    width: usize,
) {
    let files = vec![path.to_path_buf()];
    let mut app = App::new(files, themes.to_vec(), width);
    app.current_theme = initial_theme;
    app.load_selected();
    run_tui(&mut app, cfg);
}

/// Run directory browser TUI
pub fn run_directory_browser(
    dir: &Path,
    cfg: &lib::Config,
    themes: &[lib::theme::ThemeFile],
    width: usize,
) {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
        .collect();
    files.sort();

    if files.is_empty() {
        eprintln!("no .md files found in {}", dir.display());
        return;
    }

    let mut app = App::new(files, themes.to_vec(), width);
    run_tui(&mut app, cfg);
}

fn run_tui(app: &mut App, cfg: &lib::Config) {
    let mut terminal = match ratatui::try_init() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("failed to init terminal: {e}");
            return;
        }
    };
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture);

    while !app.quit {
        if let Err(e) = terminal.draw(|frame| render(frame, app)) {
            if let Ok(t) = ratatui::try_init() {
                terminal = t;
            } else {
                eprintln!("render error: {e}");
                break;
            }
        }
        match handle_events(app, cfg) {
            Ok(Action::OpenEditor) => {
                if let Some(path) = app.current_file().map(|p| p.to_path_buf()) {
                    let editor = std::env::var("EDITOR")
                        .ok()
                        .filter(|e| !e.is_empty())
                        .unwrap_or_else(|| "vi".to_string());
                    ratatui::restore();
                    let _ = std::process::Command::new(&editor).arg(&path).status();
                    app.load_selected();
                    terminal = match ratatui::try_init() {
                        Ok(t) => t,
                        Err(e) => {
                            eprintln!("failed to reinit terminal: {e}");
                            break;
                        }
                    };
                }
            }
            Ok(Action::None) => {}
            Err(e) => {
                eprintln!("event error: {e}");
                break;
            }
        }
    }

    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
    ratatui::restore();
}

enum Action {
    None,
    OpenEditor,
}
fn render(frame: &mut Frame, app: &mut App) {
    let (body_area, search_area, status_area) = if app.search_mode || !app.search_query.is_empty() {
        let [ba, sa, sta] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(frame.area());
        (ba, Some(sa), sta)
    } else {
        let [ba, sta] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
        (ba, None, sta)
    };

    app.viewport_height = body_area.height as usize;
    let max_scroll = app.content.len().saturating_sub(app.viewport_height);
    if app.scroll > max_scroll {
        app.scroll = max_scroll;
    }

    // Note: We do NOT call .wrap() here because render_markdown already
    // handles word wrapping with our bracket-aware logic. Ratatui's default
    // wrapping would re-wrap and break our bracket grouping rules.
    frame.render_widget(
        Paragraph::new(app.content.clone()).scroll((app.scroll as u16, app.offset as u16)),
        Rect::new(
            body_area.x + 1,
            body_area.y,
            body_area.width.saturating_sub(1),
            body_area.height,
        ),
    );

    if let Some(search_area) = search_area {
        let search_line = if app.search_mode {
            Line::from(Span::styled(
                format!("/{}", app.search_query),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ))
        } else if app.search_hits.is_empty() {
            Line::from(Span::styled(
                format!("/{} (no matches)", app.search_query),
                Style::default().fg(Color::DarkGray),
            ))
        } else {
            Line::from(Span::styled(
                format!(
                    "/{} ({}/{})",
                    app.search_query,
                    app.search_index + 1,
                    app.search_hits.len()
                ),
                Style::default().fg(Color::DarkGray),
            ))
        };
        frame.render_widget(search_line, search_area);
    }

    let file_name = app
        .current_file()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("(no file)");

    let theme_name = app
        .themes
        .get(app.current_theme)
        .map(|t| t.name.as_str())
        .unwrap_or("default");

    let status = Line::from(Span::styled(
        format!(
            " {} [{}] \u{2014} q:quit  c:theme  /:search  e:editor  g/G:top/bottom",
            file_name, theme_name
        ),
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(status, status_area);
}

fn handle_events(app: &mut App, _cfg: &lib::Config) -> Result<Action, Box<dyn std::error::Error>> {
    if !event::poll(std::time::Duration::from_millis(100))? {
        return Ok(Action::None);
    }
    match event::read()? {
        Event::Resize(_, _) => {
            app.resize_to_terminal();
        }
        Event::Mouse(MouseEvent { kind, .. }) => match kind {
            MouseEventKind::ScrollUp => {
                app.scroll = app.scroll.saturating_sub(3);
            }
            MouseEventKind::ScrollDown => {
                app.scroll = app.scroll.saturating_add(3);
            }
            _ => {}
        },
        Event::Key(key) => {
            if key.kind != KeyEventKind::Press {
                return Ok(Action::None);
            }

            // Search mode input
            if app.search_mode {
                match key.code {
                    KeyCode::Esc => {
                        app.search_mode = false;
                        app.search_query.clear();
                        app.search_hits.clear();
                        app.search_index = 0;
                        app.load_selected();
                    }
                    KeyCode::Enter => {
                        app.search_mode = false;
                        app.run_search();
                    }
                    KeyCode::Backspace => {
                        app.search_query.pop();
                    }
                    KeyCode::Char(ch) => {
                        app.search_query.push(ch);
                    }
                    _ => {}
                }
                return Ok(Action::None);
            }

            // Normal mode
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => app.quit = true,
                KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => app.quit = true,
                KeyCode::Char('c') => {
                    if !app.themes.is_empty() {
                        app.current_theme = (app.current_theme + 1) % app.themes.len();
                        app.load_selected();
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    app.scroll = app.scroll.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    app.scroll = app.scroll.saturating_add(1);
                }
                KeyCode::PageUp => {
                    app.scroll = app.scroll.saturating_sub(20);
                }
                KeyCode::PageDown | KeyCode::Char(' ') => {
                    app.scroll = app.scroll.saturating_add(20);
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    app.offset = app.offset.saturating_sub(8);
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    let max = app.max_line_width().saturating_sub(app.viewport_height);
                    app.offset = app.offset.saturating_add(8).min(max);
                }
                KeyCode::Enter if app.files.len() > 1 => {
                    app.load_selected();
                }
                KeyCode::Char('e') => {
                    return Ok(Action::OpenEditor);
                }
                KeyCode::Char('/') => {
                    app.search_mode = true;
                    app.search_query.clear();
                }
                KeyCode::Char('n') => {
                    app.search_next();
                }
                KeyCode::Char('N') => {
                    app.search_prev();
                }
                KeyCode::Char('g') => {
                    app.scroll = 0;
                }
                KeyCode::Char('G') => {
                    app.scroll = app.content.len().saturating_sub(app.viewport_height);
                }
                _ => {}
            }
        }
        _ => {}
    }
    Ok(Action::None)
}
