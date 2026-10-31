use crate as lib;
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

/// Byte ranges of case-insensitive matches of `query_lower` in `text`.
/// Lowercase expansions (e.g. U+0130 folds to two chars) map back to whole
/// source characters, so every range is slice-safe on `text`. Matches are
/// non-overlapping, left to right (like `str::match_indices`). Empty query
/// matches nothing.
fn ci_match_ranges(text: &str, query_lower: &str) -> Vec<(usize, usize)> {
    if query_lower.is_empty() {
        return Vec::new();
    }
    // Flattened lowercase stream; each char remembers its source char's
    // byte range so matches resolve to whole source characters.
    let mut stream: Vec<(char, usize, usize)> = Vec::new();
    for (start, c) in text.char_indices() {
        let end = start + c.len_utf8();
        for lc in c.to_lowercase() {
            stream.push((lc, start, end));
        }
    }
    let q: Vec<char> = query_lower.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + q.len() <= stream.len() {
        if stream[i..i + q.len()]
            .iter()
            .map(|(c, _, _)| *c)
            .eq(q.iter().copied())
        {
            out.push((stream[i].1, stream[i + q.len() - 1].2));
            i += q.len();
        } else {
            i += 1;
        }
    }
    out
}

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
                    let style = span.style;
                    let mut last = 0;
                    for (idx, end) in ci_match_ranges(text, &q) {
                        if idx > last {
                            new_spans.push(Span::styled(text[last..idx].to_string(), style));
                        }
                        let hl = if global_match == self.search_index {
                            self.highlight_current_style
                        } else {
                            self.highlight_style
                        };
                        new_spans.push(Span::styled(text[idx..end].to_string(), hl));
                        global_match += 1;
                        last = end;
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
        if let Ok((cols, rows)) = terminal::size() {
            let new_width = cols as usize;
            if new_width != self.width {
                self.width = new_width;
                // Save scroll position before reload (load_selected resets it)
                let saved_scroll = self.scroll;
                let saved_offset = self.offset;
                self.load_selected();
                // Restore scroll position, clamped to new content bounds
                let max_scroll = self.content.len().saturating_sub(rows as usize);
                self.scroll = saved_scroll.min(max_scroll);
                self.offset = saved_offset;
            } else {
                // Width unchanged, just clamp scroll
                let max_scroll = self.content.len().saturating_sub(rows as usize);
                if self.scroll > max_scroll {
                    self.scroll = max_scroll;
                }
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
    let files = match lib::plan::store::sorted_md_entries(dir) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("gloss: {e}");
            return;
        }
    };

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
                    let (editor, editor_args) = lib::launch::editor_command();
                    ratatui::restore();
                    let _ = std::process::Command::new(&editor)
                        .args(&editor_args)
                        .arg(&path)
                        .status();
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
    let area = frame.area();
    // Guard against zero-size terminal
    if area.width < 3 || area.height < 2 {
        return;
    }

    let (body_area, search_area, status_area) = if app.search_mode || !app.search_query.is_empty() {
        let [ba, sa, sta] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);
        (ba, Some(sa), sta)
    } else {
        let [ba, sta] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);
        (ba, None, sta)
    };

    app.viewport_height = body_area.height as usize;
    let max_scroll = app.content.len().saturating_sub(app.viewport_height);
    if app.scroll > max_scroll {
        app.scroll = max_scroll;
    }

    // Ensure body_area has valid dimensions
    if body_area.width < 2 || body_area.height == 0 {
        return;
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
                    let max = app.max_line_width().saturating_sub(app.width);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ci_match_ranges_ascii_case_insensitive() {
        assert_eq!(
            ci_match_ranges("Hello hello", "hello"),
            vec![(0, 5), (6, 11)]
        );
        assert_eq!(ci_match_ranges("HELLO", "hello"), vec![(0, 5)]);
    }

    #[test]
    fn ci_match_ranges_unicode_expansion() {
        // U+0130 lowercases to two chars; the match must cover the
        // source char's full byte range, never split it.
        let text = "A\u{130}B";
        let ranges = ci_match_ranges(text, "i\u{307}");
        assert_eq!(ranges, vec![(1, 3)]);
        for (s, e) in &ranges {
            let _ = &text[*s..*e];
        }
    }

    #[test]
    fn ci_match_ranges_partial_expansion_never_panics() {
        let text = "A\u{130}B";
        let ranges = ci_match_ranges(text, "i");
        for (s, e) in &ranges {
            assert!(text.is_char_boundary(*s) && text.is_char_boundary(*e));
            let _ = &text[*s..*e];
        }
    }

    #[test]
    fn ci_match_ranges_empty_query_is_empty() {
        assert!(ci_match_ranges("anything", "").is_empty());
    }
}
