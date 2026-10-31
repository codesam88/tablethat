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

/// Style for one scrollbar row: search matches first — the current match
/// uses the current-highlight style, other matches the match style —
/// else thumb or track, both the status bar font color (glyph, not hue,
/// separates them so the chrome reads as one quiet element).
fn scrollbar_row_style(app: &App, row: usize, track_h: usize) -> Style {
    let content_len = app.content.len();
    if track_h > 0 && !app.search_hits.is_empty() {
        let start = row.saturating_mul(content_len) / track_h;
        let end = ((row + 1).saturating_mul(content_len) / track_h).min(content_len);
        let is_current = app
            .search_hits
            .get(app.search_index)
            .is_some_and(|c| *c >= start && *c < end);
        if is_current {
            return app.highlight_current_style;
        }
        if app.search_hits.iter().any(|h| *h >= start && *h < end) {
            return app.highlight_style;
        }
    }
    // Thumb and track share the status bar font color; the caller picks
    // the glyph (█ vs │) from the thumb range to separate them.
    Style::default().fg(Color::DarkGray)
}

/// Visible-window thumb as a row range within a `track_h`-tall scrollbar.
/// Exact at both ends (0 at top, `track_h` at bottom); proportional in
/// between. `scroll` is clamped defensively; a fitting viewport fills all.
fn thumb_range(
    content_len: usize,
    viewport: usize,
    scroll: usize,
    track_h: usize,
) -> (usize, usize) {
    if track_h == 0 {
        return (0, 0);
    }
    let max_scroll = content_len.saturating_sub(viewport);
    if max_scroll == 0 {
        return (0, track_h);
    }
    let scroll = scroll.min(max_scroll);
    let thumb = (viewport
        .saturating_mul(track_h)
        .saturating_div(content_len.max(1)))
    .max(1)
    .min(track_h);
    let start = scroll * (track_h - thumb) / max_scroll;
    (start, (start + thumb).min(track_h))
}

/// Highlight every occurrence and count hits in ONE pass, so
/// `search_index` always addresses a visible highlight. Returns the
/// lines plus one hit entry (line index) per highlighted occurrence.
/// Matches spanning styled-segment boundaries are highlighted by
/// cutting spans (surrounding styles kept). Empty query passes through.
fn highlight_search(
    lines: &[Line<'static>],
    query_lower: &str,
    search_index: usize,
    hl_current: Style,
    hl: Style,
) -> (Vec<Line<'static>>, Vec<usize>) {
    if query_lower.is_empty() {
        return (lines.to_vec(), Vec::new());
    }
    let mut out_lines = Vec::with_capacity(lines.len());
    let mut hits = Vec::new();
    let mut occurrence = 0usize;
    for (line_idx, line) in lines.iter().enumerate() {
        let mut pieces: Vec<(&str, Style, usize)> = Vec::new();
        let mut off = 0usize;
        for span in &line.spans {
            let text: &str = &span.content;
            pieces.push((text, span.style, off));
            off += text.len();
        }
        let joined: String = pieces.iter().map(|(t, _, _)| *t).collect();
        let ranges = crate::text::ci_match_ranges(&joined, query_lower);
        if ranges.is_empty() {
            out_lines.push(line.clone());
            continue;
        }
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut pos = 0usize;
        for (s, e) in &ranges {
            push_slice(&pieces, &joined, pos, *s, None, &mut spans);
            let style = if occurrence == search_index {
                hl_current
            } else {
                hl
            };
            push_slice(&pieces, &joined, *s, *e, Some(style), &mut spans);
            hits.push(line_idx);
            occurrence += 1;
            pos = *e;
        }
        push_slice(&pieces, &joined, pos, joined.len(), None, &mut spans);
        out_lines.push(Line::from(spans));
    }
    (out_lines, hits)
}

/// Push `joined[from..to)` reusing original span styles (`force` overrides).
/// All bounds derive from span edges or match ranges: always slice-safe.
fn push_slice(
    pieces: &[(&str, Style, usize)],
    joined: &str,
    from: usize,
    to: usize,
    force: Option<Style>,
    out: &mut Vec<Span<'static>>,
) {
    if from >= to {
        return;
    }
    for (text, style, start) in pieces {
        let end = start + text.len();
        let s = (*start).max(from);
        let e = end.min(to);
        if s < e {
            out.push(Span::styled(
                joined[s..e].to_string(),
                force.unwrap_or(*style),
            ));
        }
    }
}

struct App {
    files: Vec<PathBuf>,
    selected: usize,
    content: Vec<Line<'static>>,
    raw_lines: Vec<String>,
    scroll: usize,
    offset: usize,
    /// Wrap target for rendering (user `--width` or terminal default).
    width: usize,
    /// Original wrap target; re-applied on resize (a `--width` survives).
    width_setting: usize,
    /// Whether to reserve a 1-char scrollbar + 1-char gap at the right.
    scrollbar: bool,
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
    fn new(
        files: Vec<PathBuf>,
        themes: Vec<lib::theme::ThemeFile>,
        width: usize,
        scrollbar: bool,
    ) -> Self {
        let term_cols = terminal::size()
            .map(|(cols, _)| cols as usize)
            .unwrap_or(width);
        let mut app = Self {
            files,
            selected: 0,
            content: Vec::new(),
            raw_lines: Vec::new(),
            scroll: 0,
            offset: 0,
            width: Self::effective_width(width, term_cols, scrollbar),
            width_setting: width,
            scrollbar,
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

    /// Wrap width for rendering: the configured target, capped at the
    /// terminal width minus the scrollbar reservation (1-char bar + 1-char
    /// gap) when enabled. Never exceeds what fits on screen.
    fn effective_width(setting: usize, term_cols: usize, scrollbar: bool) -> usize {
        let reserved = if scrollbar { 2 } else { 0 };
        setting.min(term_cols.saturating_sub(reserved))
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
            let q = self.search_query.to_lowercase();
            let (content, hits) = highlight_search(
                &base,
                &q,
                self.search_index,
                self.highlight_current_style,
                self.highlight_style,
            );
            self.content = content;
            self.search_hits = hits;
        }
    }

    fn run_search(&mut self) {
        self.search_index = 0;
        if self.search_query.to_lowercase().is_empty() {
            self.search_hits.clear();
            return;
        }
        self.apply_search_highlight();
        if !self.search_hits.is_empty() {
            self.scroll_to_match(0);
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

    /// Full-viewport page step (at least 1: viewport is unknown pre-render).
    fn page_step(&self) -> usize {
        self.viewport_height.max(1)
    }

    fn page_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(self.page_step());
    }

    fn page_down(&mut self) {
        self.scroll = self.scroll.saturating_add(self.page_step());
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
            let new_width =
                Self::effective_width(self.width_setting, cols as usize, self.scrollbar);
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
    scrollbar: bool,
) {
    let files = vec![path.to_path_buf()];
    let mut app = App::new(files, themes.to_vec(), width, scrollbar);
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
    scrollbar: bool,
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

    let mut app = App::new(files, themes.to_vec(), width, scrollbar);
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
                    let line_flag = lib::launch::editor_line_flag(&editor, Some(app.scroll + 1));
                    ratatui::restore();
                    let _ = std::process::Command::new(&editor)
                        .args(&editor_args)
                        .args(&line_flag)
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
    let show_bar = app.scrollbar && body_area.width >= 4;
    let text_w = if show_bar {
        body_area.width.saturating_sub(3)
    } else {
        body_area.width.saturating_sub(1)
    };
    frame.render_widget(
        Paragraph::new(app.content.clone()).scroll((app.scroll as u16, app.offset as u16)),
        Rect::new(body_area.x + 1, body_area.y, text_w, body_area.height),
    );

    if show_bar {
        let track_h = body_area.height as usize;
        let thumb = thumb_range(app.content.len(), app.viewport_height, app.scroll, track_h);
        let bar: Vec<Line> = (0..body_area.height)
            .map(|row| {
                let row = row as usize;
                let style = scrollbar_row_style(app, row, track_h);
                let glyph = if row >= thumb.0 && row < thumb.1 {
                    "█"
                } else {
                    "│"
                };
                Line::from(Span::styled(glyph, style))
            })
            .collect();
        frame.render_widget(
            Paragraph::new(bar),
            Rect::new(
                body_area.x + body_area.width.saturating_sub(1),
                body_area.y,
                1,
                body_area.height,
            ),
        );
    }

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
                    app.page_up();
                }
                KeyCode::PageDown | KeyCode::Char(' ') => {
                    app.page_down();
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
mod paging_tests {
    use super::*;

    fn app_with_scroll(scroll: usize, viewport_height: usize) -> App {
        let mut app = App::new(Vec::new(), Vec::new(), 80, true);
        app.scroll = scroll;
        app.viewport_height = viewport_height;
        app
    }

    #[test]
    fn page_up_subtracts_viewport_height() {
        let mut app = app_with_scroll(50, 24);
        app.page_up();
        assert_eq!(app.scroll, 26);
    }

    #[test]
    fn page_down_adds_viewport_height() {
        let mut app = app_with_scroll(0, 24);
        app.page_down();
        assert_eq!(app.scroll, 24);
    }

    #[test]
    fn page_up_saturates_at_top() {
        let mut app = app_with_scroll(5, 24);
        app.page_up();
        assert_eq!(app.scroll, 0);
    }

    #[test]
    fn page_step_is_at_least_one() {
        let app = app_with_scroll(0, 0);
        assert_eq!(app.page_step(), 1);
    }
}

#[cfg(test)]
mod scrollbar_tests {
    use super::*;

    #[test]
    fn effective_width_caps_at_usable_terminal_width() {
        // Explicit setting is honored when it fits …
        assert_eq!(App::effective_width(80, 100, true), 80);
        assert_eq!(App::effective_width(80, 100, false), 80);
        // … but never exceeds the terminal minus bar + gap, so a default
        // (terminal-width) setting always leaves 1 char before the bar.
        assert_eq!(App::effective_width(100, 100, true), 98);
        assert_eq!(App::effective_width(100, 100, false), 100);
    }

    #[test]
    fn effective_width_honors_smaller_setting() {
        assert_eq!(App::effective_width(40, 100, true), 40);
    }

    #[test]
    fn effective_width_saturates_on_narrow_terminal() {
        assert_eq!(App::effective_width(80, 10, true), 8);
        assert_eq!(App::effective_width(80, 1, true), 0);
    }

    #[test]
    fn thumb_is_exact_at_top() {
        assert_eq!(thumb_range(100, 20, 0, 10).0, 0);
    }

    #[test]
    fn thumb_is_exact_at_bottom() {
        assert_eq!(thumb_range(100, 20, 80, 10), (8, 10));
    }

    #[test]
    fn thumb_fills_track_when_content_fits() {
        assert_eq!(thumb_range(10, 20, 0, 10), (0, 10));
    }
    #[test]
    fn thumb_has_minimum_size_and_clamps_scroll() {
        assert_eq!(thumb_range(1000, 5, 500, 10), (4, 5));
        assert_eq!(thumb_range(100, 20, 999, 10), (8, 10));
        assert_eq!(thumb_range(100, 20, 0, 0), (0, 0));
    }
}

#[cfg(test)]
mod scrollbar_search_tests {
    use super::*;

    fn plain_lines(n: usize) -> Vec<Line<'static>> {
        (0..n).map(|_| Line::from("x")).collect()
    }

    fn app_with_search(content_len: usize, hits: Vec<usize>, current: usize) -> App {
        let mut app = App::new(Vec::new(), Vec::new(), 80, true);
        app.content = plain_lines(content_len);
        app.search_hits = hits;
        app.search_index = current;
        app.viewport_height = 20;
        app.scroll = 0;
        app
    }

    #[test]
    fn match_row_uses_match_style() {
        let app = app_with_search(100, vec![10, 50], 0);
        let got = scrollbar_row_style(&app, 5, 10);
        assert_eq!(got, app.highlight_style);
    }

    #[test]
    fn current_match_row_uses_current_style() {
        let app = app_with_search(100, vec![20, 50], 1);
        assert_eq!(
            scrollbar_row_style(&app, 5, 10),
            app.highlight_current_style
        );
        assert_eq!(scrollbar_row_style(&app, 2, 10), app.highlight_style);
    }

    #[test]
    fn non_match_rows_use_status_bar_color() {
        let app = app_with_search(100, vec![10, 50], 0);
        let want = Style::default().fg(Color::DarkGray);
        assert_eq!(scrollbar_row_style(&app, 0, 10), want);
        assert_eq!(scrollbar_row_style(&app, 5, 10), app.highlight_style);
    }

    #[test]
    fn no_hits_means_no_markers() {
        let app = app_with_search(100, vec![], 0);
        let track = Style::default().fg(Color::DarkGray);
        assert_eq!(scrollbar_row_style(&app, 5, 10), track);
    }
}

#[cfg(test)]
mod highlight_sync_tests {
    use super::*;

    fn plain(text: &str) -> Span<'static> {
        Span::styled(text.to_string(), Style::default())
    }

    fn hl() -> Style {
        Style::default().fg(Color::Black).bg(Color::Gray)
    }

    fn hl_cur() -> Style {
        Style::default().fg(Color::Black).bg(Color::Yellow)
    }

    #[test]
    fn cross_span_match_is_highlighted_and_counted() {
        let lines = vec![Line::from(vec![plain("hel"), plain("lo world")])];
        let (out, hits) = highlight_search(&lines, "hello", 0, hl_cur(), hl());
        assert_eq!(hits, vec![0]);
        let text: String = out[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "hello world");
        // Adjacent pieces share the highlight style (per-piece styles kept).
        let hl_text: String = out[0]
            .spans
            .iter()
            .filter(|s| s.style == hl_cur())
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(hl_text, "hello");
    }

    #[test]
    fn every_index_addresses_exactly_one_visible_highlight() {
        let lines = vec![
            Line::from(vec![plain("hel"), plain("lo")]),
            Line::from(vec![plain("say hello")]),
            Line::from(vec![plain("nothing here")]),
        ];
        for index in 0..2 {
            let (out, hits) = highlight_search(&lines, "hello", index, hl_cur(), hl());
            assert_eq!(hits, vec![0, 1]);
            // Occurrences may span pieces; joined current-styled text is one match.
            let cur_text: String = out
                .iter()
                .flat_map(|l| l.spans.iter())
                .filter(|s| s.style == hl_cur())
                .map(|s| s.content.as_ref())
                .collect();
            assert_eq!(cur_text, "hello", "index {index} must show one current");
        }
    }

    #[test]
    fn single_span_lines_keep_three_part_split() {
        let lines = vec![Line::from(vec![plain("say hello now")])];
        let (out, hits) = highlight_search(&lines, "hello", 0, hl_cur(), hl());
        assert_eq!(hits, vec![0]);
        let parts: Vec<&str> = out[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(parts, vec!["say ", "hello", " now"]);
    }

    #[test]
    fn empty_query_passes_through_unchanged() {
        let lines = vec![Line::from(vec![plain("hello")])];
        let (out, hits) = highlight_search(&lines, "", 0, hl_cur(), hl());
        assert!(hits.is_empty());
        assert_eq!(out[0].spans.len(), 1);
    }
}
