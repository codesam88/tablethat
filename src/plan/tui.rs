use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate as lib;
use lib::Config;

use lib::markdown::{render_markdown, theme_from_cfg};

const FIELD_COUNT: usize = 4; // type, priority, area, slug

/// Post-render action requested by the event loop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    /// Nothing to do.
    None,
    /// Quit the browser.
    Quit,
    /// Suspend TUI and open the selected task in `$EDITOR`.
    OpenEditor,
    /// Suspend TUI and view the selected task in `gloss`.
    OpenGloss,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Browse,
    Preview,
}

/// Attention rank for browse columns: active work first, done last.
/// Unknown statuses sort after known ones, preserving config order.
fn attention_rank(status: &str) -> u8 {
    match status {
        "in-progress" => 0,
        "open" => 1,
        "backlog" => 2,
        "idea" => 3,
        "blocked" => 4,
        "done" => 5,
        _ => 6,
    }
}

/// Split `text` into base-styled spans with case-insensitive `query`
/// matches highlighted (yellow background). Empty query or no match yields
/// one base span. Ranges come from [`crate::text::ci_match_ranges`], so
/// multibyte text never panics slicing.
fn highlight_spans(text: &str, query: &str, base: Style) -> Vec<Span<'static>> {
    if query.is_empty() {
        return vec![Span::styled(text.to_string(), base)];
    }
    let hl = base.bg(Color::Yellow);
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut last = 0;
    for (s, e) in crate::text::ci_match_ranges(text, &query.to_lowercase()) {
        if s > last {
            out.push(Span::styled(text[last..s].to_string(), base));
        }
        out.push(Span::styled(text[s..e].to_string(), hl));
        last = e;
    }
    if last < text.len() {
        out.push(Span::styled(text[last..].to_string(), base));
    }
    if out.is_empty() {
        out.push(Span::styled(text.to_string(), base));
    }
    out
}

/// Truncate a slug to display width `n`, appending … when shortened.
/// Floors to a char boundary, so multibyte slugs never panic slicing.
fn truncate_slug(slug: &str, n: usize) -> String {
    if slug.len() <= n {
        return slug.to_string();
    }
    let mut end = n.saturating_sub(1);
    while !slug.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    format!("{}…", &slug[..end])
}

struct App<'a> {
    #[allow(dead_code)]
    root: &'a Path,
    cfg: &'a Config,
    entries: Vec<PathBuf>,
    all_tasks: Vec<crate::plan::model::LoadedTask>,
    tasks: Vec<crate::plan::model::LoadedTask>,
    task_paths: HashMap<String, PathBuf>,
    columns: Vec<Column>,
    selected_column: usize,
    selected_task: usize,
    selected_field: usize,
    filter_status: Option<String>,
    filter_type: Option<String>,
    filter_priority: Option<String>,
    filter_area: Option<String>,
    filter_search: Option<String>,
    /// Live slug-substring filter from `/` search (titles only, unlike
    /// `filter_search` which also matches bodies).
    browse_title: Option<String>,
    search_fulltext: bool,
    hide_done: bool,
    mode: Mode,
    preview: Vec<Line<'static>>,
    preview_scroll: usize,
    preview_offset: usize,
    preview_width: usize,
    search_mode: bool,
    search_query: String,
    search_hits: Vec<usize>,
    search_index: usize,
    themes: Vec<lib::theme::ThemeFile>,
    current_theme: usize,
}

struct Column {
    status: String,
    task_indices: Vec<usize>,
}

impl<'a> App<'a> {
    fn new(
        root: &'a Path,
        cfg: &'a Config,
        tasks: Vec<crate::plan::model::LoadedTask>,
        entries: Vec<PathBuf>,
    ) -> Self {
        let path_map: HashMap<String, PathBuf> = entries
            .iter()
            .filter_map(|p| {
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|slug| (slug.to_string(), p.clone()))
            })
            .collect();

        // Load all tasks unfiltered for hidden-count display
        let all_tasks =
            crate::plan::query::load_and_filter_tasks(&entries, None, None, None, None, None, &[]);

        let themes = lib::theme::load_themes(cfg.themes_dir.as_deref(), "plan");

        let mut app = Self {
            root,
            cfg,
            entries,
            all_tasks,
            tasks,
            task_paths: path_map,
            columns: Vec::new(),
            selected_column: 0,
            selected_task: 0,
            selected_field: 0,
            filter_status: None,
            filter_type: None,
            filter_priority: None,
            filter_area: None,
            filter_search: None,
            browse_title: None,
            search_fulltext: false,
            hide_done: cfg.hide_done,
            mode: Mode::Browse,
            preview: Vec::new(),
            preview_scroll: 0,
            preview_offset: 0,
            preview_width: cfg.tui_width,
            search_mode: false,
            search_query: String::new(),
            search_hits: Vec::new(),
            search_index: 0,
            themes,
            current_theme: 0,
        };

        app.sort_tasks();
        app.rebuild_columns();
        app
    }

    fn sort_tasks(&mut self) {
        self.tasks
            .sort_by(|a, b| crate::plan::query::cmp_tasks(a, b, &[], &self.cfg.kanban_order));
    }

    fn reload_tasks(&mut self) {
        let exclude: Vec<String> = if self.hide_done {
            vec!["done".into()]
        } else {
            vec![]
        };
        // Fulltext browse search (`/`) matches slug OR body via
        // load_and_filter_tasks; slug-only browse search (`l`) matches slugs
        // only via the retain below.
        let search_for_load = if self.search_fulltext && self.browse_title.is_some() {
            self.browse_title.as_deref()
        } else {
            self.filter_search.as_deref()
        };
        self.tasks = crate::plan::query::load_and_filter_tasks(
            &self.entries,
            self.filter_status.as_deref(),
            self.filter_type.as_deref(),
            self.filter_priority.as_deref(),
            self.filter_area.as_deref(),
            search_for_load,
            &exclude,
        );
        if !self.search_fulltext
            && let Some(q) = &self.browse_title
        {
            let ql = q.to_lowercase();
            self.tasks.retain(|t| t.slug.to_lowercase().contains(&ql));
        }
        let path_map: HashMap<String, PathBuf> = self
            .entries
            .iter()
            .filter_map(|p| {
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|slug| (slug.to_string(), p.clone()))
            })
            .collect();
        self.task_paths = path_map;
        self.selected_column = 0;
        self.selected_task = 0;
        self.sort_tasks();
        self.rebuild_columns();
    }

    fn has_active_filters(&self) -> bool {
        self.filter_status.is_some()
            || self.filter_type.is_some()
            || self.filter_priority.is_some()
            || self.filter_area.is_some()
            || self.filter_search.is_some()
            || self.browse_title.is_some()
            || self.hide_done != self.cfg.hide_done
    }

    /// Clear every filter and reload. Used by Esc (never by search Esc,
    /// which clears only the search itself). `q` no longer clears filters;
    /// it quits from browse mode.
    fn clear_filters(&mut self) {
        self.filter_status = None;
        self.filter_type = None;
        self.filter_priority = None;
        self.filter_area = None;
        self.filter_search = None;
        self.browse_title = None;
        self.search_fulltext = false;
        self.search_query.clear();
        self.search_mode = false;
        self.hide_done = self.cfg.hide_done;
        self.reload_tasks();
    }

    fn rebuild_columns(&mut self) {
        // Browse leads with active work (user attention is at the top),
        // unlike list output which follows config order to the bottom.
        let mut order = self.cfg.kanban_order.clone();
        order.sort_by_key(|s| attention_rank(s));
        let mut columns = Vec::new();
        for status in &order {
            let indices: Vec<usize> = self
                .tasks
                .iter()
                .enumerate()
                .filter(|(_, t)| &t.task.status == status)
                .map(|(i, _)| i)
                .collect();
            columns.push(Column {
                status: status.to_string(),
                task_indices: indices,
            });
        }
        self.columns = columns;

        if self.selected_column >= self.columns.len() {
            self.selected_column = self.columns.len().saturating_sub(1);
        }
        if let Some(col) = self.columns.get(self.selected_column)
            && self.selected_task >= col.task_indices.len()
        {
            self.selected_task = col.task_indices.len().saturating_sub(1);
        }
    }
}

/// Run the interactive kanban browser; returns on quit or empty result.
/// `OpenEditor`/`OpenGloss` suspend the TUI, run the child, then resume.
pub fn run_tui(
    root: &Path,
    cfg: &Config,
    status_filter: Option<&str>,
    type_filter: Option<&str>,
    priority_filter: Option<&str>,
    area_filter: Option<&str>,
    search_query: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let tasks_dir = root.join(".plan");
    let entries = crate::plan::store::read_task_files(&tasks_dir).unwrap_or_default();
    let exclude: Vec<String> = if cfg.hide_done {
        vec!["done".into()]
    } else {
        vec![]
    };
    let tasks = crate::plan::query::load_and_filter_tasks(
        &entries,
        status_filter,
        type_filter,
        priority_filter,
        area_filter,
        search_query,
        &exclude,
    );

    if tasks.is_empty() {
        eprintln!("(no tasks match filters)");
        return Ok(());
    }

    let mut app = App::new(root, cfg, tasks, entries);
    let mut terminal =
        ratatui::try_init().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    let outcome = (|| -> Result<(), Box<dyn std::error::Error>> {
        loop {
            if let Err(e) = terminal.draw(|frame| app.render(frame)) {
                match ratatui::try_init() {
                    Ok(t) => terminal = t,
                    Err(_) => return Err(Box::new(e)),
                }
            }
            let action = app.handle_events()?;
            match action {
                Action::None => {}
                Action::Quit => break,
                Action::OpenGloss => {
                    if let Some((_slug, path)) = app.current_task_path() {
                        ratatui::restore();
                        let width = app.cfg.pager_width.to_string();
                        let gloss = lib::launch::resolve_gloss_program();
                        let status = std::process::Command::new(&gloss)
                            .arg("--width")
                            .arg(&width)
                            .arg(path)
                            .status();
                        if let Ok(s) = status
                            && !s.success()
                        {
                            eprintln!("gloss exited with code: {:?}", s.code());
                        }
                        terminal = ratatui::try_init()
                            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                    }
                }
                Action::OpenEditor => {
                    if let Some((_slug, path)) = app.current_task_path() {
                        let (editor, editor_args) = lib::launch::editor_command();
                        // Preview knows its scroll line; browse opens at top.
                        let line = (app.mode == Mode::Preview).then_some(app.preview_scroll + 1);
                        let line_flag = lib::launch::editor_line_flag(&editor, line);
                        ratatui::restore();
                        let status = std::process::Command::new(&editor)
                            .args(&editor_args)
                            .args(&line_flag)
                            .arg(path)
                            .status();
                        if let Ok(s) = status
                            && !s.success()
                        {
                            eprintln!("{editor} exited with code: {:?}", s.code());
                        }
                        terminal = ratatui::try_init()
                            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                    }
                }
            }
        }
        Ok(())
    })();

    ratatui::restore();
    outcome
}

impl App<'_> {
    fn render(&mut self, frame: &mut Frame) {
        match self.mode {
            Mode::Browse => self.render_browse(frame),
            Mode::Preview => self.render_preview(frame),
        }
    }

    fn render_browse(&mut self, frame: &mut Frame) {
        let slug_w = self
            .tasks
            .iter()
            .map(|t| t.slug.len())
            .max()
            .unwrap_or(4)
            .clamp(4, 28);
        let type_w: usize = 8;
        let prio_w: usize = 8;
        let slot = |w: usize| w + 1;

        let [head_area, task_area, footer_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(frame.area());

        self.render_header(frame, head_area, slug_w, type_w, prio_w, slot);
        self.render_task_area(frame, task_area, slug_w, type_w, prio_w, slot);
        self.render_footer(frame, footer_area);
    }

    fn render_preview(&mut self, frame: &mut Frame) {
        let has_search = self.search_mode || !self.search_query.is_empty();
        let (title_area, body_area, search_area) = if has_search {
            let [ta, ba, sa] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Fill(1),
                Constraint::Length(1),
            ])
            .areas(frame.area());
            (ta, ba, Some(sa))
        } else {
            let [ta, ba] =
                Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(frame.area());
            (ta, ba, None)
        };
        let theme_cfg = self.current_theme_config();
        let theme = theme_from_cfg(&theme_cfg);
        let theme_name = self
            .themes
            .get(self.current_theme)
            .map(|t| t.name.as_str())
            .unwrap_or("default");
        let title = Line::from(Span::styled(
            format!(
                " Preview [{}] \u{2014} q:close  c:theme  e:editor  /:search",
                theme_name
            ),
            Style::default().fg(Color::DarkGray),
        ));
        frame.render_widget(title, title_area);
        let tw = self.cfg.tui_width;
        if tw != self.preview_width
            && self.mode == Mode::Preview
            && let Some((_slug, path)) = self.current_task_path()
            && let Ok(content) = std::fs::read_to_string(path)
        {
            self.preview_width = tw;
            self.preview = render_markdown(&theme, &content, tw, false);
            self.preview_scroll = 0;
            self.preview_offset = 0;
        }
        frame.render_widget(
            Paragraph::new(self.preview.clone())
                .scroll((self.preview_scroll as u16, self.preview_offset as u16)),
            Rect::new(
                body_area.x + 1,
                body_area.y,
                body_area.width.saturating_sub(1),
                body_area.height,
            ),
        );
        if let Some(search_area) = search_area {
            let search_line = if self.search_mode {
                Line::from(Span::styled(
                    format!("/{}", self.search_query),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ))
            } else if self.search_hits.is_empty() {
                Line::from(Span::styled(
                    format!("/{} (no matches)", self.search_query),
                    Style::default().fg(Color::DarkGray),
                ))
            } else {
                Line::from(Span::styled(
                    format!(
                        "/{} ({}/{})",
                        self.search_query,
                        self.search_index + 1,
                        self.search_hits.len()
                    ),
                    Style::default().fg(Color::DarkGray),
                ))
            };
            frame.render_widget(search_line, search_area);
        }
    }

    fn render_header(
        &self,
        frame: &mut Frame,
        area: Rect,
        slug_w: usize,
        type_w: usize,
        prio_w: usize,
        slot: impl Fn(usize) -> usize,
    ) {
        // Header labels in same order as task rows: slug, type, priority, area
        // Each padded with same slot widths so columns align
        let mut spans = Vec::new();
        spans.push(Span::styled(
            format!("{:<w$}", format!(" {}", "Issue"), w = slot(slug_w)),
            if self.selected_field == 3 {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            },
        ));
        spans.push(Span::styled(
            format!("{:<w$}", format!(" {}", "Type"), w = slot(type_w)),
            if self.selected_field == 0 {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            },
        ));
        spans.push(Span::styled(
            format!("{:<w$}", format!(" {}", "Priority"), w = slot(prio_w)),
            if self.selected_field == 1 {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            },
        ));
        spans.push(Span::styled(
            format!(" {}", "Area"),
            if self.selected_field == 2 {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            },
        ));
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn render_task_area(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        slug_w: usize,
        type_w: usize,
        prio_w: usize,
        slot: impl Fn(usize) -> usize,
    ) {
        if self.columns.is_empty() || area.width < 10 {
            return;
        }
        // Color scheme — BOLD only on active field, non-active selected row has plain bg
        let sel_base = Style::default().bg(Color::White).fg(Color::Black);
        let sel_act = Style::default()
            .bg(Color::White)
            .fg(Color::Rgb(0, 0, 0))
            .add_modifier(Modifier::BOLD);
        let sel_prio_base = |pc: Color| Style::default().bg(pc).fg(Color::Black);
        let sel_prio_act = |pc: Color| {
            Style::default()
                .bg(pc)
                .fg(Color::Rgb(0, 0, 0))
                .add_modifier(Modifier::BOLD)
        };
        let is_vivid = |c: Color| {
            matches!(
                c,
                Color::Red
                    | Color::Yellow
                    | Color::Blue
                    | Color::Magenta
                    | Color::Cyan
                    | Color::Green
            )
        };

        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut selected_line = 0usize;

        for (col_idx, column) in self.columns.iter().enumerate() {
            let is_active = col_idx == self.selected_column;
            let color = crate::color::status_color(&column.status, &self.cfg.colors);

            let total = self
                .all_tasks
                .iter()
                .filter(|t| t.task.status == column.status)
                .count();
            let shown = column.task_indices.len();
            let header = if self.hide_done && column.status == "done" && shown < total {
                format!(
                    "{} ({} hidden)",
                    column.status.to_uppercase(),
                    total - shown
                )
            } else {
                format!("{} ({})", column.status.to_uppercase(), shown)
            };
            let dash = area.width.saturating_sub(header.len() as u16 + 2);
            lines.push(Line::from(Span::styled(
                format!("{} {}", header, "─".repeat(dash.max(1) as usize)),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )));

            if column.task_indices.is_empty() {
                let none_style = if is_active {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                lines.push(Line::from(Span::styled(" (none)", none_style)));
            } else {
                for (row_idx, &task_idx) in column.task_indices.iter().enumerate() {
                    let task = &self.tasks[task_idx];
                    let is_sel = is_active && row_idx == self.selected_task;
                    if is_sel {
                        selected_line = lines.len();
                    }
                    let is_act = |f: usize| is_sel && self.selected_field == f;

                    let mut spans = Vec::new();

                    // Slug (field 3)
                    let slen = slot(slug_w);
                    let s = if is_act(3) {
                        let n = slen.saturating_sub(2);
                        format!("[{}]", truncate_slug(&task.slug, n))
                    } else {
                        let n = slen.saturating_sub(1);
                        format!(" {}", truncate_slug(&task.slug, n))
                    };
                    let slug_st = if is_sel && is_act(3) {
                        sel_act
                    } else if is_sel {
                        sel_base
                    } else {
                        Style::default()
                    };
                    let mut slug_spans = highlight_spans(&s, &self.search_query, slug_st);
                    let used: usize = slug_spans.iter().map(|sp| sp.content.len()).sum();
                    if used < slen {
                        slug_spans.push(Span::styled(" ".repeat(slen - used), slug_st));
                    }
                    spans.extend(slug_spans);

                    // Type (field 0)
                    let content = if is_act(0) {
                        format!(
                            "{:<w$}",
                            format!("[{}]", task.task.task_type),
                            w = slot(type_w)
                        )
                    } else {
                        format!(
                            "{:<w$}",
                            format!(" {}", task.task.task_type),
                            w = slot(type_w)
                        )
                    };
                    let type_st = if is_sel && is_act(0) {
                        sel_act
                    } else if is_sel {
                        sel_base
                    } else {
                        Style::default()
                    };
                    spans.push(Span::styled(content, type_st));

                    // Priority (field 1) — vivid colors get colored bg, low uses plain row highlight
                    let pc = crate::color::priority_color(&task.task.priority, &self.cfg.colors);
                    let vivid = is_vivid(pc);
                    let content = if is_act(1) {
                        format!(
                            "{:<w$}",
                            format!("[{}]", task.task.priority),
                            w = slot(prio_w)
                        )
                    } else {
                        format!(
                            "{:<w$}",
                            format!(" {}", task.task.priority),
                            w = slot(prio_w)
                        )
                    };
                    let prio_st = if is_sel && vivid && is_act(1) {
                        sel_prio_act(pc)
                    } else if is_sel && vivid {
                        sel_prio_base(pc)
                    } else if is_sel && is_act(1) {
                        sel_act
                    } else if is_sel {
                        sel_base
                    } else {
                        Style::default().fg(pc)
                    };
                    spans.push(Span::styled(content, prio_st));

                    // Area (field 2)
                    let content = if is_act(2) {
                        format!("[{}]", task.task.area)
                    } else {
                        format!(" {}", task.task.area)
                    };
                    let area_st = if is_sel && is_act(2) {
                        sel_act
                    } else if is_sel {
                        sel_base
                    } else {
                        Style::default()
                    };
                    spans.push(Span::styled(content, area_st));

                    lines.push(Line::from(spans));
                }
            }

            lines.push(Line::from(""));
        }

        lines.pop();
        let visible_lines = area.height as usize;
        let vert_scroll = selected_line.saturating_sub(visible_lines.saturating_sub(2));
        frame.render_widget(Paragraph::new(lines).scroll((vert_scroll as u16, 0)), area);
    }

    /// Search status line, shown INSTEAD of the status bar while typing
    /// or filtered: bare `/` before the first keystroke, `/query (n)`
    /// after. `None` when no search is active.
    fn footer_search_line(&self) -> Option<String> {
        if !self.search_mode && self.search_query.is_empty() {
            return None;
        }
        if self.search_query.is_empty() {
            return Some("/".to_string());
        }
        let n = self.tasks.len();
        Some(format!(
            "/{} ({} {})",
            self.search_query,
            n,
            if n == 1 { "match" } else { "matches" }
        ))
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        if let Some(line) = self.footer_search_line() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    line,
                    Style::default().fg(Color::Yellow),
                ))),
                area,
            );
            return;
        }
        let done_hint = if self.hide_done {
            "H:show-done"
        } else {
            "H:hide-done"
        };
        let text = format!(
            " Enter:view  l:limit  f:filter  /:search  e:edit  {done_hint}  q:quit  Ctrl-c:quit "
        );

        let filters: Vec<(&str, &str)> = [
            ("status", self.filter_status.as_deref()),
            ("type", self.filter_type.as_deref()),
            ("priority", self.filter_priority.as_deref()),
            ("area", self.filter_area.as_deref()),
            ("search", self.browse_title.as_deref()),
        ]
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect();

        let mut spans = vec![Span::styled(text, Style::default().fg(Color::DarkGray))];
        if self.has_active_filters() {
            spans.push(Span::styled(
                " Esc:clear-filters",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        if !filters.is_empty() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled("Filter:", Style::default().fg(Color::Yellow)));
            for (k, v) in &filters {
                spans.push(Span::styled(
                    format!(" {}={}", k, v),
                    Style::default().fg(Color::Yellow),
                ));
            }
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

impl App<'_> {
    fn handle_events(&mut self) -> Result<Action, Box<dyn std::error::Error>> {
        // crossterm's poll/read can surface a transient io::ErrorKind::Interrupted
        // during a tmux SIGWINCH/resize race. Treat only that kind as "no action
        // this poll" so the loop retries; propagate every other error as fatal.
        let polled = match event::poll(std::time::Duration::from_millis(100)) {
            Ok(b) => b,
            Err(e) => {
                let boxed: Box<dyn std::error::Error> = Box::new(e);
                if let Some(io_err) = boxed.downcast_ref::<std::io::Error>()
                    && io_err.kind() == std::io::ErrorKind::Interrupted
                {
                    return Ok(Action::None);
                }
                return Err(boxed);
            }
        };
        if !polled {
            return Ok(Action::None);
        }
        let ev = match event::read() {
            Ok(e) => e,
            Err(e) => {
                let boxed: Box<dyn std::error::Error> = Box::new(e);
                if let Some(io_err) = boxed.downcast_ref::<std::io::Error>()
                    && io_err.kind() == std::io::ErrorKind::Interrupted
                {
                    return Ok(Action::None);
                }
                return Err(boxed);
            }
        };
        match ev {
            Event::Resize(_, _) => {}
            Event::Key(key) => {
                if key.kind != KeyEventKind::Press {
                    return Ok(Action::None);
                }
                return Ok(self.handle_key_event(key));
            }
            _ => {}
        }
        Ok(Action::None)
    }

    /// Handle one key press, returning the resulting action. Pure-ish (no
    /// terminal I/O) so it is unit-testable.
    #[allow(dead_code)]
    pub fn handle_key_event(&mut self, key: KeyEvent) -> Action {
        if key.kind != KeyEventKind::Press {
            return Action::None;
        }
        match self.mode {
            Mode::Preview => {
                // Search mode input
                if self.search_mode {
                    self.handle_preview_search_key(key);
                    return Action::None;
                }

                // Normal preview mode
                match key.code {
                    KeyCode::Char('q') if key.modifiers == KeyModifiers::NONE => {
                        self.mode = Mode::Browse;
                    }
                    // Alt-Esc is treated as Esc here (non-destructive
                    // cancel/close, never quit) — intentional, not a bug.
                    KeyCode::Esc => self.mode = Mode::Browse,
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.preview_scroll = self.preview_scroll.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.preview_scroll = self.preview_scroll.saturating_add(1);
                    }
                    KeyCode::PageUp => {
                        self.preview_scroll = self.preview_scroll.saturating_sub(20);
                    }
                    KeyCode::PageDown | KeyCode::Char(' ') => {
                        self.preview_scroll = self.preview_scroll.saturating_add(20);
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        self.preview_offset = self.preview_offset.saturating_sub(8);
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        self.preview_offset = self.preview_offset.saturating_add(8);
                    }
                    KeyCode::Char('e') => return Action::OpenEditor,
                    KeyCode::Char('/') => {
                        self.search_mode = true;
                        self.search_query.clear();
                    }
                    KeyCode::Char('n') => self.search_next(),
                    KeyCode::Char('N') => self.search_prev(),
                    KeyCode::Char('c')
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && !key.modifiers.contains(KeyModifiers::ALT) =>
                    {
                        return Action::Quit;
                    }
                    KeyCode::Char('c') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.cycle_theme();
                    }
                    _ => {}
                }
                Action::None
            }
            Mode::Browse => {
                // Filter-as-you-type search: handled separately so
                // Up/Down can confirm the filter and start moving.
                if self.search_mode {
                    self.handle_search_key(key);
                    return Action::None;
                }
                match key.code {
                    KeyCode::Char('c')
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && !key.modifiers.contains(KeyModifiers::ALT) =>
                    {
                        Action::Quit
                    }
                    KeyCode::Char('q') if key.modifiers == KeyModifiers::NONE => Action::Quit,
                    // Alt-Esc is treated as Esc here (non-destructive
                    // cancel/close, never quit) — intentional, not a bug.
                    KeyCode::Esc => {
                        if self.has_active_filters() {
                            self.clear_filters();
                        }
                        Action::None
                    }
                    KeyCode::Enter if !self.columns.is_empty() => Action::OpenGloss,
                    KeyCode::Char('f') if !self.columns.is_empty() => {
                        let col = &self.columns[self.selected_column];
                        if let Some(&task_idx) = col.task_indices.get(self.selected_task) {
                            let task = &self.tasks[task_idx];
                            match self.selected_field {
                                0 => self.filter_type = Some(task.task.task_type.clone()),
                                1 => self.filter_priority = Some(task.task.priority.clone()),
                                2 => self.filter_area = Some(task.task.area.clone()),
                                3 => self.filter_search = Some(task.slug.clone()),
                                _ => {}
                            }
                            self.reload_tasks();
                        }
                        Action::None
                    }
                    KeyCode::Char('H') => {
                        self.hide_done = !self.hide_done;
                        self.reload_tasks();
                        Action::None
                    }
                    KeyCode::Char('e') if !self.columns.is_empty() => Action::OpenEditor,
                    KeyCode::Char('l') => {
                        self.search_fulltext = false;
                        self.search_mode = true;
                        self.search_query.clear();
                        self.filter_search = None;
                        self.browse_title = None;
                        self.reload_tasks();
                        Action::None
                    }
                    KeyCode::Char('/') => {
                        self.search_fulltext = true;
                        self.search_mode = true;
                        self.search_query.clear();
                        self.filter_search = None;
                        self.browse_title = None;
                        self.reload_tasks();
                        Action::None
                    }
                    _ => {
                        self.handle_task_key(key.code);
                        Action::None
                    }
                }
            }
        }
    }

    // Canonical key entry point (mode dispatches internally).

    fn handle_task_key(&mut self, code: KeyCode) {
        if self.columns.is_empty() {
            return;
        }
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_task > 0 {
                    self.selected_task -= 1;
                } else if self.selected_column > 0 {
                    self.selected_column -= 1;
                    let col = &self.columns[self.selected_column];
                    self.selected_task = col.task_indices.len().saturating_sub(1);
                } else {
                    self.goto_last();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let col = &self.columns[self.selected_column];
                if self.selected_task + 1 < col.task_indices.len() {
                    self.selected_task += 1;
                } else if self.selected_column + 1 < self.columns.len() {
                    self.selected_column += 1;
                    self.selected_task = 0;
                } else {
                    self.goto_first();
                }
            }
            KeyCode::PageUp => {
                if self.is_first() {
                    self.goto_last();
                } else {
                    self.goto_first();
                }
            }
            KeyCode::PageDown => {
                if self.is_last() {
                    self.goto_first();
                } else {
                    self.goto_last();
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.selected_field = if self.selected_field > 0 {
                    self.selected_field - 1
                } else {
                    FIELD_COUNT - 1
                };
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.selected_field = if self.selected_field + 1 < FIELD_COUNT {
                    self.selected_field + 1
                } else {
                    0
                };
            }
            _ => {}
        }
    }

    fn is_first(&self) -> bool {
        self.selected_column == 0 && self.selected_task == 0
    }

    fn is_last(&self) -> bool {
        match self.columns.get(self.selected_column) {
            Some(col) => {
                self.selected_column + 1 >= self.columns.len()
                    && self.selected_task + 1 >= col.task_indices.len()
            }
            None => true,
        }
    }

    fn goto_first(&mut self) {
        self.selected_column = 0;
        self.selected_task = 0;
    }

    fn goto_last(&mut self) {
        self.selected_column = self.columns.len().saturating_sub(1);
        self.selected_task = self
            .columns
            .last()
            .map(|c| c.task_indices.len().saturating_sub(1))
            .unwrap_or(0);
    }

    fn apply_browse_search(&mut self) {
        self.browse_title = if self.search_query.is_empty() {
            None
        } else {
            Some(self.search_query.clone())
        };
        self.reload_tasks();
    }

    /// Leave browse search, dropping the query and title filter.
    fn cancel_browse_search(&mut self) {
        self.search_mode = false;
        self.search_query.clear();
        self.browse_title = None;
        self.search_fulltext = false;
        self.reload_tasks();
    }

    /// Search-mode keys: typing refilters live, Enter keeps the filter,
    /// Esc/Ctrl-C clears it, Up/Down confirm it and start moving.
    fn handle_search_key(&mut self, key: KeyEvent) {
        // Ctrl-C cancels exactly like Esc (otherwise it would type `c`).
        if matches!(key.code, KeyCode::Char('c')) && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.cancel_browse_search();
            return;
        }
        match key.code {
            KeyCode::Esc => self.cancel_browse_search(),
            KeyCode::Enter => {
                // Confirm: keep the filter, dismiss the search UI.
                self.search_mode = false;
                self.search_query.clear();
            }
            KeyCode::Up | KeyCode::Down => {
                self.search_mode = false;
                self.search_query.clear();
                self.handle_task_key(key.code);
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                self.apply_browse_search();
            }
            KeyCode::Char(ch) => {
                self.search_query.push(ch);
                self.apply_browse_search();
            }
            _ => {}
        }
    }

    /// Preview search-mode keys. Ctrl-C cancels exactly like Esc
    /// (otherwise it would type `c`); unlike browse, the query is kept.
    fn handle_preview_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search_mode = false;
            }
            KeyCode::Esc => {
                self.search_mode = false;
            }
            KeyCode::Enter => {
                self.search_mode = false;
                self.run_preview_search();
            }
            KeyCode::Backspace => {
                self.search_query.pop();
            }
            KeyCode::Char(ch) => {
                self.search_query.push(ch);
            }
            _ => {}
        }
    }

    fn run_preview_search(&mut self) {
        self.search_hits.clear();
        self.search_index = 0;
        let q = self.search_query.to_lowercase();
        if q.is_empty() {
            return;
        }
        if let Some((_slug, path)) = self.current_task_path()
            && let Ok(content) = std::fs::read_to_string(path)
        {
            for (i, line) in content.lines().enumerate() {
                if line.to_lowercase().contains(&q) {
                    self.search_hits.push(i);
                }
            }
        }
        if !self.search_hits.is_empty() {
            self.preview_scroll = self.search_hits[0];
        }
    }

    fn search_next(&mut self) {
        if self.search_hits.is_empty() {
            return;
        }
        self.search_index = (self.search_index + 1) % self.search_hits.len();
        self.preview_scroll = self.search_hits[self.search_index];
    }

    fn search_prev(&mut self) {
        if self.search_hits.is_empty() {
            return;
        }
        self.search_index = self.search_index.wrapping_sub(1) % self.search_hits.len();
        self.preview_scroll = self.search_hits[self.search_index];
    }

    fn current_theme_config(&self) -> lib::ThemeConfig {
        self.themes
            .get(self.current_theme)
            .map(|t| t.theme.clone())
            .unwrap_or_default()
    }

    fn cycle_theme(&mut self) {
        if !self.themes.is_empty() {
            self.current_theme = (self.current_theme + 1) % self.themes.len();
        }
        if let Some((_slug, path)) = self.current_task_path()
            && let Ok(content) = std::fs::read_to_string(path)
        {
            self.render_content(&content);
        }
    }

    fn render_content(&mut self, content: &str) {
        let theme_cfg = self.current_theme_config();
        let theme = theme_from_cfg(&theme_cfg);
        self.preview = render_markdown(&theme, content, self.preview_width, false);
    }

    fn current_task_path(&self) -> Option<(&str, &Path)> {
        let col = self.columns.get(self.selected_column)?;
        let task_idx = col.task_indices.get(self.selected_task)?;
        let task = self.tasks.get(*task_idx)?;
        let path = self.task_paths.get(&task.slug)?;
        Some((&task.slug, path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_slug_ascii_unchanged() {
        assert_eq!(truncate_slug("my-task-123", 6), "my-ta…");
        assert_eq!(truncate_slug("short", 10), "short");
    }

    #[test]
    fn truncate_slug_floors_to_char_boundary() {
        // ë/â are 2 bytes each; byte 8 lands inside â and must floor to 7.
        assert_eq!(truncate_slug("tëst-tâsk", 9), "tëst-t…");
    }

    #[test]
    fn truncate_slug_never_panics() {
        let slug = "tëst-tâsk-日本語";
        for n in 0..=(slug.len() + 2) {
            let out = truncate_slug(slug, n);
            assert!(out.len() <= n + "…".len());
        }
    }
}

#[cfg(test)]
mod attention_tests {
    use super::*;

    fn ordered_statuses(order: &[String]) -> Vec<String> {
        let mut v = order.to_vec();
        v.sort_by_key(|s| attention_rank(s));
        v
    }

    #[test]
    fn default_order_becomes_attention_order() {
        let default = vec![
            "idea".to_string(),
            "backlog".to_string(),
            "open".to_string(),
            "in-progress".to_string(),
            "blocked".to_string(),
            "done".to_string(),
        ];
        let got = ordered_statuses(&default);
        assert_eq!(
            got,
            vec!["in-progress", "open", "backlog", "idea", "blocked", "done"]
        );
    }

    #[test]
    fn unknown_statuses_sort_last_stably() {
        let order = vec!["zzz".to_string(), "open".to_string(), "aaa".to_string()];
        let got = ordered_statuses(&order);
        assert_eq!(got, vec!["open", "zzz", "aaa"]);
    }
}

#[cfg(test)]
mod nav_tests {
    use super::*;
    use crate::plan::model::{LoadedTask, Task};

    fn task(status: &str, slug: &str) -> LoadedTask {
        LoadedTask {
            slug: slug.to_string(),
            task: Task {
                status: status.to_string(),
                task_type: "feature".to_string(),
                priority: "medium".to_string(),
                area: String::new(),
            },
        }
    }

    fn app_two_tasks() -> App<'static> {
        let cfg = Box::leak(Box::new(Config::default()));
        let root = Box::leak(Box::new(std::path::PathBuf::from("/tmp")));
        let tasks = vec![task("in-progress", "aaa"), task("done", "zzz")];
        App::new(root, cfg, tasks, Vec::new())
    }

    #[test]
    fn up_from_first_wraps_to_last() {
        let mut app = app_two_tasks();
        assert_eq!((app.selected_column, app.selected_task), (0, 0));
        app.handle_task_key(crossterm::event::KeyCode::Up);
        let last = app.columns.len() - 1;
        assert_eq!((app.selected_column, app.selected_task), (last, 0));
    }

    #[test]
    fn down_from_last_wraps_to_first() {
        let mut app = app_two_tasks();
        let last = app.columns.len() - 1;
        app.selected_column = last;
        app.selected_task = 0;
        app.handle_task_key(crossterm::event::KeyCode::Down);
        assert_eq!((app.selected_column, app.selected_task), (0, 0));
    }

    #[test]
    fn page_down_jumps_to_last_then_wraps() {
        use crossterm::event::KeyCode;
        let mut app = app_two_tasks();
        let last = app.columns.len() - 1;
        app.handle_task_key(KeyCode::PageDown);
        assert_eq!((app.selected_column, app.selected_task), (last, 0));
        app.handle_task_key(KeyCode::PageDown);
        assert_eq!((app.selected_column, app.selected_task), (0, 0));
    }

    #[test]
    fn page_up_jumps_to_first_then_wraps() {
        use crossterm::event::KeyCode;
        let mut app = app_two_tasks();
        let last = app.columns.len() - 1;
        app.selected_column = last;
        app.handle_task_key(KeyCode::PageUp);
        assert_eq!((app.selected_column, app.selected_task), (0, 0));
        app.handle_task_key(KeyCode::PageUp);
        assert_eq!((app.selected_column, app.selected_task), (last, 0));
    }
}

#[cfg(test)]
mod esc_policy_tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn app() -> App<'static> {
        let cfg = Box::leak(Box::new(Config::default()));
        let root = Box::leak(Box::new(std::path::PathBuf::from("/tmp")));
        let tmp = tempfile::tempdir().unwrap();
        let plan_dir = tmp.path().join(".plan");
        std::fs::create_dir_all(&plan_dir).unwrap();
        let task_path = plan_dir.join("aaa.md");
        std::fs::write(
            &task_path,
            "---\nstatus: open\ntype: feature\npriority: medium\narea: \n---\n",
        )
        .unwrap();
        let entries = vec![task_path];
        let exclude: Vec<String> = Vec::new();
        let tasks = crate::plan::query::load_and_filter_tasks(
            &entries, None, None, None, None, None, &exclude,
        );
        // Keep the temp dir alive for the lifetime of the leaked app.
        let _ = Box::leak(Box::new(tmp));
        App::new(root, cfg, tasks, entries)
    }

    #[test]
    fn esc_in_browse_mode_does_not_quit() {
        let mut app = app();
        app.mode = Mode::Browse;
        let action = app.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(action, Action::None);
    }

    #[test]
    fn q_in_browse_mode_quits() {
        let mut app = app();
        app.mode = Mode::Browse;
        let action = app.handle_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert_eq!(action, Action::Quit);
    }

    #[test]
    fn ctrl_c_in_browse_mode_quits() {
        let mut app = app();
        app.mode = Mode::Browse;
        let action = app.handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(action, Action::Quit);
    }

    #[test]
    fn esc_clears_active_filters_without_quitting() {
        let mut app = app();
        app.mode = Mode::Browse;
        app.filter_status = Some("open".to_string());
        assert!(app.has_active_filters());
        let action = app.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(action, Action::None);
        assert!(!app.has_active_filters());
    }

    #[test]
    fn esc_in_preview_closes_preview() {
        let mut app = app();
        app.mode = Mode::Preview;
        let action = app.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(action, Action::None);
    }

    #[test]
    fn esc_in_browse_search_cancels() {
        let mut app = app();
        app.mode = Mode::Browse;
        app.search_mode = true;
        app.search_query = "zzz".to_string();
        app.browse_title = Some("zzz".to_string());
        app.reload_tasks();
        assert!(app.tasks.is_empty(), "filter should remove all tasks");
        app.handle_search_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.search_mode);
        assert!(app.search_query.is_empty());
        assert!(app.browse_title.is_none(), "browse_title filter cleared");
        assert_eq!(app.tasks.len(), 1, "tasks restored");
    }

    #[test]
    fn alt_q_in_browse_does_not_quit() {
        let mut app = app();
        app.mode = Mode::Browse;
        let action = app.handle_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::ALT));
        assert_ne!(action, Action::Quit);
    }

    #[test]
    fn alt_esc_in_browse_does_not_quit() {
        let mut app = app();
        app.mode = Mode::Browse;
        let action = app.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::ALT));
        assert_ne!(action, Action::Quit);
    }

    #[test]
    fn alt_up_arrow_in_browse_does_not_quit() {
        let mut app = app();
        app.mode = Mode::Browse;
        let action = app.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT));
        assert_ne!(action, Action::Quit);
    }

    #[test]
    fn non_press_kind_is_ignored() {
        let mut app = app();
        app.mode = Mode::Browse;
        let mut key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        key.kind = crossterm::event::KeyEventKind::Release;
        let action = app.handle_key_event(key);
        assert_eq!(action, Action::None);
        assert_ne!(action, Action::Quit);
    }

    #[test]
    fn ctrl_c_in_preview_quits() {
        let mut app = app();
        app.mode = Mode::Preview;
        let action = app.handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(action, Action::Quit);
    }

    #[test]
    fn ctrl_c_in_preview_does_not_cycle_theme() {
        let mut app = app();
        app.mode = Mode::Preview;
        app.themes = vec![
            lib::theme::ThemeFile {
                name: "a".to_string(),
                theme: lib::theme::ThemeConfig::default(),
            },
            lib::theme::ThemeFile {
                name: "b".to_string(),
                theme: lib::theme::ThemeConfig::default(),
            },
        ];
        app.current_theme = 0;
        let before = app.current_theme;
        let _ = app.handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(app.current_theme, before, "Ctrl-c must not cycle theme");
    }

    #[test]
    fn ctrl_alt_c_in_preview_does_not_cycle_theme() {
        let mut app = app();
        app.mode = Mode::Preview;
        app.themes = vec![
            lib::theme::ThemeFile {
                name: "a".to_string(),
                theme: lib::theme::ThemeConfig::default(),
            },
            lib::theme::ThemeFile {
                name: "b".to_string(),
                theme: lib::theme::ThemeConfig::default(),
            },
        ];
        app.current_theme = 0;
        let before = app.current_theme;
        let _ = app.handle_key_event(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ));
        assert_eq!(app.current_theme, before, "Ctrl+Alt-c must not cycle theme");
    }
}

#[cfg(test)]
mod highlight_tests {
    use super::*;

    fn plain(spans: &[Span]) -> Vec<(String, bool)> {
        spans
            .iter()
            .map(|s| (s.content.to_string(), s.style.bg == Some(Color::Yellow)))
            .collect()
    }

    #[test]
    fn empty_query_yields_single_base_span() {
        let spans = highlight_spans("my-task", "", Style::default());
        assert_eq!(plain(&spans), vec![("my-task".to_string(), false)]);
    }

    #[test]
    fn match_splits_span_with_highlighted_middle() {
        let spans = highlight_spans("my-task", "task", Style::default());
        assert_eq!(
            plain(&spans),
            vec![("my-".to_string(), false), ("task".to_string(), true),]
        );
    }

    #[test]
    fn no_match_yields_single_base_span() {
        let spans = highlight_spans("my-task", "zzz", Style::default());
        assert_eq!(plain(&spans), vec![("my-task".to_string(), false)]);
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;
    use crate::plan::model::{LoadedTask, Task};
    use crossterm::event::KeyCode;

    fn task(status: &str, slug: &str) -> LoadedTask {
        LoadedTask {
            slug: slug.to_string(),
            task: Task {
                status: status.to_string(),
                task_type: "feature".to_string(),
                priority: "medium".to_string(),
                area: String::new(),
            },
        }
    }

    fn task_file(dir: &std::path::Path, slug: &str, body: &str) -> PathBuf {
        let path = dir.join(format!("{slug}.md"));
        std::fs::write(
            &path,
            format!("---\nstatus: open\ntype: feature\npriority: medium\n---\n\n{body}\n"),
        )
        .unwrap();
        path
    }

    fn app_with_files() -> (tempfile::TempDir, App<'static>) {
        let tmp = tempfile::tempdir().unwrap();
        let a = task_file(tmp.path(), "abc", "nothing relevant here");
        let b = task_file(tmp.path(), "xyz", "mentions abc in the body");
        let cfg = Box::leak(Box::new(Config::default()));
        let root = Box::leak(Box::new(tmp.path().to_path_buf()));
        let mut app = App::new(root, cfg, Vec::new(), vec![a, b]);
        app.reload_tasks();
        (tmp, app)
    }

    #[test]
    fn typing_filters_titles_only() {
        let (_tmp, mut app) = app_with_files();
        assert_eq!(app.tasks.len(), 2);
        app.search_mode = true;
        app.handle_search_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
        app.handle_search_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        let slugs: Vec<&str> = app.tasks.iter().map(|t| t.slug.as_str()).collect();
        // "xyz" matches 'b'/'c' only via its body — must be excluded.
        assert_eq!(slugs, vec!["abc"]);
    }

    #[test]
    fn typing_filters_fulltext_when_flag_set() {
        let (_tmp, mut app) = app_with_files();
        assert_eq!(app.tasks.len(), 2);
        app.search_fulltext = true;
        app.search_mode = true;
        app.handle_search_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        let slugs: Vec<&str> = app.tasks.iter().map(|t| t.slug.as_str()).collect();
        // "xyz" matches via body ("mentions"); "abc" does not.
        assert_eq!(slugs, vec!["xyz"]);
    }

    #[test]
    fn backspace_to_empty_restores_all() {
        let (_tmp, mut app) = app_with_files();
        app.search_mode = true;
        app.handle_search_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
        assert_eq!(app.tasks.len(), 1);
        app.handle_search_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(app.tasks.len(), 2);
        assert!(app.browse_title.is_none());
    }

    #[test]
    fn esc_clears_search_but_keeps_other_filters() {
        let (_tmp, mut app) = app_with_files();
        app.filter_priority = Some("medium".to_string());
        app.search_mode = true;
        app.handle_search_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
        assert_eq!(app.tasks.len(), 1);
        app.handle_search_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.search_mode);
        assert!(app.search_query.is_empty());
        assert!(app.browse_title.is_none());
        // Priority filter survives; both tasks are medium.
        assert_eq!(app.tasks.len(), 2);
    }

    #[test]
    fn up_confirms_filter_and_moves() {
        let cfg = Box::leak(Box::new(Config::default()));
        let root = Box::leak(Box::new(std::path::PathBuf::from("/tmp")));
        let tasks = vec![task("in-progress", "aaa"), task("done", "zzz")];
        let mut app = App::new(root, cfg, tasks, Vec::new());
        app.search_mode = true;
        app.search_query = "a".to_string();
        app.browse_title = Some("a".to_string());
        app.handle_search_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert!(!app.search_mode, "Up confirms the filter");
        assert_eq!(app.browse_title.as_deref(), Some("a"), "filter kept");
        assert!(app.search_query.is_empty(), "search UI dismissed");
        assert_eq!(app.footer_search_line(), None, "status bar restored");
        let last = app.columns.len() - 1;
        assert_eq!((app.selected_column, app.selected_task), (last, 0));
    }

    #[test]
    fn enter_confirms_filter_and_restores_status_bar() {
        let cfg = Box::leak(Box::new(Config::default()));
        let root = Box::leak(Box::new(std::path::PathBuf::from("/tmp")));
        let tasks = vec![task("open", "aaa"), task("open", "zzz")];
        let mut app = App::new(root, cfg, tasks, Vec::new());
        app.search_mode = true;
        app.search_query = "a".to_string();
        app.browse_title = Some("a".to_string());
        app.handle_search_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(!app.search_mode);
        assert_eq!(app.browse_title.as_deref(), Some("a"), "filter kept");
        assert!(app.search_query.is_empty(), "search UI dismissed");
        assert_eq!(app.footer_search_line(), None, "status bar restored");
    }

    #[test]
    fn footer_search_line_states() {
        let cfg = Box::leak(Box::new(Config::default()));
        let root = Box::leak(Box::new(std::path::PathBuf::from("/tmp")));
        let mut app = App::new(root, cfg, vec![task("open", "aaa")], Vec::new());
        assert_eq!(app.footer_search_line(), None);
        app.search_mode = true;
        assert_eq!(app.footer_search_line(), Some("/".to_string()));
        app.search_query = "a".to_string();
        assert_eq!(app.footer_search_line(), Some("/a (1 match)".to_string()));
    }
}

#[cfg(test)]
mod clear_search_tests {
    use super::*;
    use crate::plan::model::{LoadedTask, Task};

    #[test]
    fn clear_filters_resets_search_ui() {
        let cfg = Box::leak(Box::new(Config::default()));
        let root = Box::leak(Box::new(std::path::PathBuf::from("/tmp")));
        let tasks = vec![LoadedTask {
            slug: "aaa".to_string(),
            task: Task {
                status: "open".to_string(),
                task_type: "feature".to_string(),
                priority: "medium".to_string(),
                area: String::new(),
            },
        }];
        let mut app = App::new(root, cfg, tasks, Vec::new());
        // Kept-filter state: typing done, filter applied.
        app.search_query = "a".to_string();
        app.browse_title = Some("a".to_string());
        app.clear_filters();
        assert!(app.search_query.is_empty(), "query must clear");
        assert!(app.browse_title.is_none(), "filter must clear");
        assert_eq!(app.footer_search_line(), None, "bar must revert");
    }
}

#[cfg(test)]
mod search_cancel_tests {
    use super::*;

    #[test]
    fn ctrl_c_cancels_search_like_esc() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        for slug in ["abc", "xyz"] {
            std::fs::write(
                dir.join(format!("{slug}.md")),
                "---\nstatus: open\ntype: feature\npriority: medium\n---\n\nbody\n",
            )
            .unwrap();
        }
        let cfg = Box::leak(Box::new(Config::default()));
        let root = Box::leak(Box::new(dir.to_path_buf()));
        let entries: Vec<PathBuf> = vec![dir.join("abc.md"), dir.join("xyz.md")];
        let mut app = App::new(root, cfg, Vec::new(), entries);
        app.reload_tasks();
        app.search_mode = true;
        app.handle_search_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
        assert_eq!(app.tasks.len(), 1);
        app.handle_search_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(!app.search_mode, "ctrl-c leaves search mode");
        assert!(app.search_query.is_empty(), "query cleared");
        assert!(app.browse_title.is_none(), "filter cleared");
        assert_eq!(app.tasks.len(), 2, "tasks restored");
    }
}

#[cfg(test)]
mod preview_search_cancel_tests {
    use super::*;

    #[test]
    fn ctrl_c_exits_preview_search_like_esc() {
        let cfg = Box::leak(Box::new(Config::default()));
        let root = Box::leak(Box::new(std::path::PathBuf::from("/tmp")));
        let mut app = App::new(root, cfg, Vec::new(), Vec::new());
        app.search_mode = true;
        app.search_query = "x".to_string();
        app.handle_preview_search_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(!app.search_mode, "ctrl-c leaves search mode");
        assert_eq!(app.search_query, "x", "preview keeps query like Esc");
    }
}
