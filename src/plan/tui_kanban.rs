use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lib::Config;
use tablethat_lib as lib;

use lib::markdown::{render_markdown, theme_from_cfg};

const FIELD_COUNT: usize = 4; // type, priority, area, slug

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    None,
    Quit,
    OpenEditor,
    OpenGloss,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Browse,
    Preview,
}

struct App<'a> {
    #[allow(dead_code)]
    root: &'a Path,
    cfg: &'a Config,
    entries: Vec<PathBuf>,
    all_tasks: Vec<crate::tasks::LoadedTask>,
    tasks: Vec<crate::tasks::LoadedTask>,
    task_paths: HashMap<String, PathBuf>,
    columns: Vec<Column>,
    selected_column: usize,
    selected_task: usize,
    selected_field: usize,
    quit: bool,
    filter_status: Option<String>,
    filter_type: Option<String>,
    filter_priority: Option<String>,
    filter_area: Option<String>,
    filter_search: Option<String>,
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
        tasks: Vec<crate::tasks::LoadedTask>,
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
            crate::tasks::load_and_filter_tasks(&entries, None, None, None, None, None, &[]);

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
            quit: false,
            filter_status: None,
            filter_type: None,
            filter_priority: None,
            filter_area: None,
            filter_search: None,
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
            .sort_by(|a, b| crate::tasks::cmp_tasks(a, b, &[], &self.cfg.kanban_order));
    }

    fn reload_tasks(&mut self) {
        let exclude: Vec<String> = if self.hide_done {
            vec!["done".into()]
        } else {
            vec![]
        };
        self.tasks = crate::tasks::load_and_filter_tasks(
            &self.entries,
            self.filter_status.as_deref(),
            self.filter_type.as_deref(),
            self.filter_priority.as_deref(),
            self.filter_area.as_deref(),
            self.filter_search.as_deref(),
            &exclude,
        );
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
            || self.hide_done != self.cfg.hide_done
    }

    fn rebuild_columns(&mut self) {
        let kanban_order = &self.cfg.kanban_order;
        let mut columns = Vec::new();
        for status in kanban_order {
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

pub fn run_tui(
    root: &Path,
    cfg: &Config,
    status_filter: Option<&str>,
    type_filter: Option<&str>,
    priority_filter: Option<&str>,
    area_filter: Option<&str>,
    search_query: Option<&str>,
) {
    let tasks_dir = root.join(".plan");
    let entries = crate::tasks::read_task_files(&tasks_dir).unwrap_or_default();
    let exclude: Vec<String> = if cfg.hide_done {
        vec!["done".into()]
    } else {
        vec![]
    };
    let tasks = crate::tasks::load_and_filter_tasks(
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
        return;
    }

    let mut app = App::new(root, cfg, tasks, entries);
    let mut terminal = match ratatui::try_init() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("failed to init terminal: {e}");
            return;
        }
    };

    while !app.quit {
        if let Err(e) = terminal.draw(|frame| app.render(frame)) {
            if let Ok(t) = ratatui::try_init() {
                terminal = t;
            } else {
                eprintln!("render error: {e}");
                break;
            }
        }
        match app.handle_events() {
            Ok(action) => match action {
                Action::None => {}
                Action::Quit => break,
                Action::OpenGloss => {
                    if let Some((_slug, path)) = app.current_task_path() {
                        ratatui::restore();
                        let width = app.cfg.pager_width.to_string();
                        let status = std::process::Command::new("gloss")
                            .arg("--width")
                            .arg(&width)
                            .arg(path)
                            .status();
                        if let Ok(s) = status
                            && !s.success()
                        {
                            eprintln!("gloss exited with code: {:?}", s.code());
                        }
                        if let Ok(t) = ratatui::try_init() {
                            terminal = t;
                        } else {
                            eprintln!("failed to re-init terminal after gloss");
                            break;
                        }
                    }
                }
                Action::OpenEditor => {
                    if let Some((_slug, path)) = app.current_task_path() {
                        let editor = std::env::var("EDITOR")
                            .ok()
                            .filter(|e| !e.is_empty())
                            .unwrap_or_else(|| "vi".to_string());
                        ratatui::restore();
                        let status = std::process::Command::new(&editor).arg(path).status();
                        if let Ok(s) = status
                            && !s.success()
                        {
                            eprintln!("{editor} exited with code: {:?}", s.code());
                        }
                        if let Ok(t) = ratatui::try_init() {
                            terminal = t;
                        } else {
                            eprintln!("failed to re-init terminal after editor");
                            break;
                        }
                    }
                }
            },
            Err(e) => {
                eprintln!("event error: {e}");
                break;
            }
        }
    }

    ratatui::restore();
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
            self.preview = render_markdown(&theme, &content, tw);
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
            let color = status_color(&column.status, &self.cfg.colors);

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
                    let slug = if is_act(3) {
                        let n = slen.saturating_sub(2);
                        let s = if task.slug.len() > n {
                            format!("{}…", &task.slug[..n.saturating_sub(1)])
                        } else {
                            task.slug.clone()
                        };
                        format!("{:<w$}", format!("[{}]", s), w = slen)
                    } else {
                        let n = slen.saturating_sub(1);
                        let s = if task.slug.len() > n {
                            format!("{}…", &task.slug[..n.saturating_sub(1)])
                        } else {
                            task.slug.clone()
                        };
                        format!("{:<w$}", format!(" {}", s), w = slen)
                    };
                    let slug_st = if is_sel && is_act(3) {
                        sel_act
                    } else if is_sel {
                        sel_base
                    } else {
                        Style::default()
                    };
                    spans.push(Span::styled(slug, slug_st));

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
                    let pc = priority_color(&task.task.priority, &self.cfg.colors);
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

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let done_hint = if self.hide_done {
            "h:show-done"
        } else {
            "h:hide-done"
        };
        let text = format!(
            " Enter:view  f:filter  e:edit  {done_hint}  {}  Ctrl-c:quit ",
            if self.has_active_filters() {
                "q:clear-filters"
            } else {
                "q:quit"
            }
        );

        let filters: Vec<(&str, &str)> = [
            ("status", self.filter_status.as_deref()),
            ("type", self.filter_type.as_deref()),
            ("priority", self.filter_priority.as_deref()),
            ("area", self.filter_area.as_deref()),
        ]
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect();

        let mut spans = vec![Span::styled(text, Style::default().fg(Color::DarkGray))];
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
        if !event::poll(std::time::Duration::from_millis(100))? {
            return Ok(Action::None);
        }
        match event::read()? {
            Event::Resize(_, _) => {}
            Event::Key(key) => {
                if key.kind != KeyEventKind::Press {
                    return Ok(Action::None);
                }
                match self.mode {
                    Mode::Preview => {
                        // Search mode input
                        if self.search_mode {
                            match key.code {
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
                            return Ok(Action::None);
                        }

                        // Normal preview mode
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => self.mode = Mode::Browse,
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
                            KeyCode::Char('e') => return Ok(Action::OpenEditor),
                            KeyCode::Char('/') => {
                                self.search_mode = true;
                                self.search_query.clear();
                            }
                            KeyCode::Char('n') => self.search_next(),
                            KeyCode::Char('N') => self.search_prev(),
                            KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                                return Ok(Action::Quit);
                            }
                            KeyCode::Char('c') => self.cycle_theme(),
                            _ => {}
                        }
                    }
                    Mode::Browse => {
                        return Ok(match key.code {
                            KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                                Action::Quit
                            }
                            KeyCode::Char('q') | KeyCode::Esc => {
                                if self.has_active_filters() {
                                    self.filter_status = None;
                                    self.filter_type = None;
                                    self.filter_priority = None;
                                    self.filter_area = None;
                                    self.filter_search = None;
                                    self.hide_done = self.cfg.hide_done;
                                    self.reload_tasks();
                                    Action::None
                                } else {
                                    Action::Quit
                                }
                            }
                            KeyCode::Enter if !self.columns.is_empty() => Action::OpenGloss,
                            KeyCode::Char('f') if !self.columns.is_empty() => {
                                let col = &self.columns[self.selected_column];
                                if let Some(&task_idx) = col.task_indices.get(self.selected_task) {
                                    let task = &self.tasks[task_idx];
                                    match self.selected_field {
                                        0 => self.filter_type = Some(task.task.task_type.clone()),
                                        1 => {
                                            self.filter_priority = Some(task.task.priority.clone())
                                        }
                                        2 => self.filter_area = Some(task.task.area.clone()),
                                        3 => self.filter_search = Some(task.slug.clone()),
                                        _ => {}
                                    }
                                    self.reload_tasks();
                                }
                                Action::None
                            }
                            KeyCode::Char('h') => {
                                self.hide_done = !self.hide_done;
                                self.reload_tasks();
                                Action::None
                            }
                            KeyCode::Char('e') if !self.columns.is_empty() => Action::OpenEditor,
                            _ => {
                                self.handle_task_key(key.code);
                                Action::None
                            }
                        });
                    }
                }
            }
            _ => {}
        }
        Ok(Action::None)
    }

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
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let col = &self.columns[self.selected_column];
                if self.selected_task + 1 < col.task_indices.len() {
                    self.selected_task += 1;
                } else if self.selected_column + 1 < self.columns.len() {
                    self.selected_column += 1;
                    self.selected_task = 0;
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
        self.preview = render_markdown(&theme, content, self.preview_width);
    }

    fn current_task_path(&self) -> Option<(&str, &Path)> {
        let col = self.columns.get(self.selected_column)?;
        let task_idx = col.task_indices.get(self.selected_task)?;
        let task = self.tasks.get(*task_idx)?;
        let path = self.task_paths.get(&task.slug)?;
        Some((&task.slug, path))
    }
}

fn status_color(status: &str, colors: &tablethat_lib::ColorsConfig) -> Color {
    lib::status_color(status, colors)
}

fn priority_color(p: &str, colors: &tablethat_lib::ColorsConfig) -> Color {
    lib::priority_color(p, colors)
}

#[cfg(test)]
mod quick_table_test {
    use tablethat_lib::markdown::{render_markdown, theme_from_cfg};
    #[test]
    fn table_renders_header_and_body() {
        let md = "| **Name** | `Code` |\n|---|---|\n| foo | bar |\n";
        let th = theme_from_cfg(&tablethat_lib::ThemeConfig::default());
        let lines = render_markdown(&th, md, 80);
        let total: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(total.contains("Name"), "Missing header 'Name'");
        assert!(total.contains("foo"), "Missing body 'foo'");
        assert!(total.contains("="), "Missing separator");
    }
}
