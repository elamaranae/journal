use anyhow::Result;
use chrono::Local;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table, TableState, Wrap},
    Frame, Terminal,
};
use std::{io, path::PathBuf};

use crate::db::{Db, EntryRow};
use crate::editor::open_editor;
use crate::entry::{Entry, Mood};
use crate::git;

// ── Palette ──────────────────────────────────────────────────────────────────

const C_BORDER: Color = Color::Rgb(55, 55, 65);
const C_DIM: Color = Color::Rgb(90, 90, 105);
const C_SEL: Color = Color::Rgb(80, 200, 220);
const C_TAG: Color = Color::Rgb(220, 190, 90);
const C_WHITE: Color = Color::Rgb(220, 220, 230);
const C_FLASH: Color = Color::Rgb(100, 210, 120);

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(PartialEq, Clone)]
enum Mode {
    List,
    Search,
    Show,
    Command,
}

enum Action {
    None,
    Quit,
    Edit { id: String, body: String },
}

struct App {
    mode: Mode,
    prev_mode: Mode,
    all_entries: Vec<EntryRow>,
    entries: Vec<EntryRow>,
    list_state: TableState,
    search_input: String,
    command_buf: String,
    show_scroll: u16,
    show_preview: bool,
    flash: Option<String>,
    db: Db,
    journal_path: PathBuf,
}

impl App {
    fn new(db: Db, journal_path: PathBuf) -> Result<Self> {
        let entries = db.list(None, None, None, 2000)?;
        let mut list_state = TableState::default();
        if !entries.is_empty() {
            list_state.select(Some(0));
        }
        Ok(Self {
            mode: Mode::List,
            prev_mode: Mode::List,
            all_entries: entries.clone(),
            entries,
            list_state,
            search_input: String::new(),
            command_buf: String::new(),
            show_scroll: 0,
            show_preview: false,
            flash: None,
            db,
            journal_path,
        })
    }

    fn selected(&self) -> Option<&EntryRow> {
        self.list_state.selected().and_then(|i| self.entries.get(i))
    }

    fn move_sel(&mut self, delta: i32) {
        let n = self.entries.len();
        if n == 0 {
            return;
        }
        let cur = self.list_state.selected().unwrap_or(0) as i32;
        let next = ((cur + delta).rem_euclid(n as i32)) as usize;
        self.list_state.select(Some(next));
    }

    fn run_search(&mut self) {
        if self.search_input.is_empty() {
            self.entries = self.all_entries.clone();
        } else {
            self.entries = self.db.search(&self.search_input).unwrap_or_default();
        }
        let sel = if self.entries.is_empty() { None } else { Some(0) };
        self.list_state.select(sel);
    }

    fn reload(&mut self) {
        if let Ok(fresh) = self.db.list(None, None, None, 2000) {
            self.all_entries = fresh.clone();
            let old_sel = self.list_state.selected();
            if self.search_input.is_empty() {
                self.entries = fresh;
            } else {
                self.entries = self.db.search(&self.search_input).unwrap_or_default();
            }
            if let Some(i) = old_sel {
                let clamped = i.min(self.entries.len().saturating_sub(1));
                self.list_state
                    .select(if self.entries.is_empty() { None } else { Some(clamped) });
            }
        }
    }
}

// ── Entry point ──────────────────────────────────────────────────────────────

pub fn run(db: Db, journal_path: PathBuf) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    let mut app = App::new(db, journal_path)?;
    let res = event_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    res
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        terminal.draw(|f| draw(f, app))?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                app.flash = None;
                match on_key(app, key.code) {
                    Action::Quit => return Ok(()),
                    Action::Edit { id, body } => {
                        disable_raw_mode()?;
                        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                        terminal.show_cursor()?;

                        let edited = open_editor(&body).unwrap_or_else(|_| body.clone());
                        let edited = edited.trim().to_string();

                        enable_raw_mode()?;
                        execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                        terminal.hide_cursor()?;
                        terminal.clear()?;

                        if !edited.is_empty() && edited != body.trim() {
                            if let Ok(Some(row)) = app.db.get_by_id(&id) {
                                let now = Local::now();
                                let updated = Entry {
                                    id: row.id.clone(),
                                    title: row.title.clone(),
                                    created_at: row.created_at,
                                    updated_at: now,
                                    tags: row.tags.clone(),
                                    mood: row.mood.clone(),
                                    body: edited,
                                };
                                if app.db.upsert(&updated).is_ok() {
                                    let title = updated.title.as_deref().unwrap_or("(untitled)");
                                    let msg = format!(
                                        "journal: edit {} \"{}\"",
                                        now.format("%Y-%m-%dT%H:%M:%S"),
                                        title
                                    );
                                    let _ = git::commit_db(&app.journal_path, &msg);
                                    app.flash = Some(format!("Saved \"{}\"", title));
                                    app.reload();
                                }
                            }
                        }
                    }
                    Action::None => {}
                }
            }
            // Redraw on terminal resize
            Event::Resize(_, _) => {
                terminal.clear()?;
            }
            _ => {}
        }
    }
}

fn on_key(app: &mut App, code: KeyCode) -> Action {
    match app.mode.clone() {
        Mode::List | Mode::Search => match code {
            KeyCode::Char('q') if app.mode == Mode::List => Action::Quit,
            KeyCode::Char(':') => {
                app.prev_mode = app.mode.clone();
                app.mode = Mode::Command;
                app.command_buf.clear();
                Action::None
            }
            KeyCode::Char('p') => {
                app.show_preview = !app.show_preview;
                Action::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.move_sel(-1);
                Action::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.move_sel(1);
                Action::None
            }
            KeyCode::Enter => {
                if app.selected().is_some() {
                    app.show_scroll = 0;
                    app.prev_mode = app.mode.clone();
                    app.mode = Mode::Show;
                }
                Action::None
            }
            KeyCode::Char('/') if app.mode == Mode::List => {
                app.mode = Mode::Search;
                app.search_input.clear();
                Action::None
            }
            KeyCode::Esc if app.mode == Mode::Search => {
                app.search_input.clear();
                app.mode = Mode::List;
                app.run_search();
                Action::None
            }
            KeyCode::Backspace if app.mode == Mode::Search => {
                app.search_input.pop();
                app.run_search();
                Action::None
            }
            KeyCode::Char(c) if app.mode == Mode::Search => {
                app.search_input.push(c);
                app.run_search();
                Action::None
            }
            _ => Action::None,
        },
        Mode::Show => match code {
            KeyCode::Esc => {
                app.mode = app.prev_mode.clone();
                Action::None
            }
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char(':') => {
                app.prev_mode = Mode::Show;
                app.mode = Mode::Command;
                app.command_buf.clear();
                Action::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.show_scroll = app.show_scroll.saturating_sub(1);
                Action::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.show_scroll += 1;
                Action::None
            }
            KeyCode::Char('o') => {
                if let Some(e) = app.selected() {
                    Action::Edit {
                        id: e.id.clone(),
                        body: e.body.clone().unwrap_or_default(),
                    }
                } else {
                    Action::None
                }
            }
            _ => Action::None,
        },
        Mode::Command => match code {
            KeyCode::Esc => {
                app.mode = app.prev_mode.clone();
                app.command_buf.clear();
                Action::None
            }
            KeyCode::Enter => {
                let cmd = app.command_buf.clone();
                app.command_buf.clear();
                app.mode = app.prev_mode.clone();
                if cmd == "q" {
                    Action::Quit
                } else {
                    Action::None
                }
            }
            KeyCode::Backspace => {
                app.command_buf.pop();
                Action::None
            }
            KeyCode::Char(c) => {
                app.command_buf.push(c);
                Action::None
            }
            _ => Action::None,
        },
    }
}

// ── Rendering ─────────────────────────────────────────────────────────────────

fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // Root layout: main box + 1-line status bar
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let display_mode = match &app.mode {
        Mode::Command => &app.prev_mode,
        m => m,
    };

    // Outer box title
    let n = app.entries.len();
    let box_title = match display_mode {
        Mode::Show => {
            let t = app.selected().and_then(|e| e.title.as_deref()).unwrap_or("untitled");
            format!(" {} ", t)
        }
        _ => format!(
            " journal  ·  {} {} ",
            n,
            if n == 1 { "entry" } else { "entries" }
        ),
    };

    let block = Block::default()
        .title(box_title.as_str())
        .title_style(Style::default().fg(C_WHITE).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(C_BORDER));

    let inner = block.inner(root[0]);
    f.render_widget(block, root[0]);

    match display_mode {
        Mode::Show => draw_show(f, app, inner),
        _ => draw_list(f, app, inner),
    }

    draw_status(f, app, root[1]);
}

// ── List / Search view ────────────────────────────────────────────────────────

fn draw_list(f: &mut Frame, app: &mut App, area: Rect) {
    if app.show_preview {
        let halves = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        draw_list_table(f, app, halves[0]);
        draw_preview(f, app, halves[1]);
    } else {
        draw_list_table(f, app, area);
    }
}

fn draw_list_table(f: &mut Frame, app: &mut App, area: Rect) {
    let has_search = matches!(app.mode, Mode::Search) || !app.search_input.is_empty();
    let narrow = app.show_preview;

    // Vertical: [separator(1)] [table(min)] [search(1) if active]
    let mut v = vec![Constraint::Length(1), Constraint::Min(0)];
    if has_search {
        v.push(Constraint::Length(1));
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(v)
        .split(area);

    // Column separator line
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(area.width as usize),
            Style::default().fg(C_BORDER),
        ))),
        chunks[0],
    );

    // Column headers + rows
    let col_style = Style::default().fg(C_DIM).add_modifier(Modifier::BOLD);
    let (header, widths) = if narrow {
        (
            Row::new(vec![
                Cell::from(""),
                Cell::from("DATE").style(col_style),
                Cell::from("TITLE").style(col_style),
                Cell::from("MOOD").style(col_style),
            ]),
            vec![
                Constraint::Length(2),
                Constraint::Length(11),
                Constraint::Fill(1),
                Constraint::Length(7),
            ],
        )
    } else {
        (
            Row::new(vec![
                Cell::from(""),
                Cell::from("DATE").style(col_style),
                Cell::from("TITLE").style(col_style),
                Cell::from("TAGS").style(col_style),
                Cell::from("MOOD").style(col_style),
                Cell::from("WORDS").style(col_style),
            ]),
            vec![
                Constraint::Length(2),
                Constraint::Length(11),
                Constraint::Fill(2),
                Constraint::Fill(1),
                Constraint::Length(7),
                Constraint::Length(5),
            ],
        )
    };

    let sel_idx = app.list_state.selected();
    let rows: Vec<Row> = app
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| make_row(e, sel_idx == Some(i), narrow))
        .collect();

    let table = Table::new(rows, widths)
        .header(header.height(1).bottom_margin(0))
        .row_highlight_style(Style::default().fg(C_SEL).add_modifier(Modifier::BOLD))
        .column_spacing(1);

    f.render_stateful_widget(table, chunks[1], &mut app.list_state);

    // Search bar
    if has_search && chunks.len() > 2 {
        let cursor = if matches!(app.mode, Mode::Search) { "▌" } else { "" };
        let bar = Line::from(vec![
            Span::styled("  /  ", Style::default().fg(C_DIM)),
            Span::styled(
                app.search_input.clone(),
                Style::default()
                    .fg(Color::Rgb(120, 170, 255))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(cursor, Style::default().fg(Color::Rgb(120, 170, 255))),
        ]);
        f.render_widget(Paragraph::new(bar), chunks[2]);
    }
}

fn make_row(e: &EntryRow, selected: bool, narrow: bool) -> Row<'static> {
    let indicator = if selected { "▶" } else { " " };
    let date = e.created_at.format("%Y-%m-%d").to_string();
    let title = e.title.as_deref().unwrap_or("(untitled)").to_string();
    let tags = e.tags.join(", ");
    let mood_s = e.mood.as_ref().map(|m| m.to_string()).unwrap_or_default();
    let words = e.word_count.to_string();
    let mood_col = mood_color(&e.mood);

    if narrow {
        Row::new(vec![
            Cell::from(indicator),
            Cell::from(date).style(Style::default().fg(C_DIM)),
            Cell::from(title).style(Style::default().fg(C_WHITE)),
            Cell::from(mood_s).style(Style::default().fg(mood_col)),
        ])
    } else {
        Row::new(vec![
            Cell::from(indicator),
            Cell::from(date).style(Style::default().fg(C_DIM)),
            Cell::from(title).style(Style::default().fg(C_WHITE)),
            Cell::from(tags).style(Style::default().fg(C_TAG)),
            Cell::from(mood_s).style(Style::default().fg(mood_col)),
            Cell::from(words).style(Style::default().fg(C_DIM)),
        ])
    }
}

// ── Preview pane ──────────────────────────────────────────────────────────────

fn draw_preview(f: &mut Frame, app: &App, area: Rect) {
    // Left border separating list from preview
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(C_BORDER));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(entry) = app.selected() else {
        return;
    };

    // Clone data needed before borrow ends
    let title = entry.title.as_deref().unwrap_or("(untitled)").to_string();
    let date = entry.created_at.format("%Y-%m-%d  %H:%M").to_string();
    let tags: Vec<String> = entry.tags.iter().map(|t| format!("#{}", t)).collect();
    let mood_col = mood_color(&entry.mood);
    let mood_s = entry.mood.as_ref().map(|m| m.to_string()).unwrap_or_default();
    let body = entry.body.clone().unwrap_or_default();

    // Layout: title(1) + blank(1) + meta(1) + separator(1) + body(rest)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);

    // Title
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(title, Style::default().fg(C_WHITE).add_modifier(Modifier::BOLD)),
        ])),
        chunks[0],
    );

    // Date
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(date, Style::default().fg(C_DIM)),
        ])),
        chunks[1],
    );

    // Tags + mood
    let mut meta_spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    if !tags.is_empty() {
        meta_spans.push(Span::styled(tags.join("  "), Style::default().fg(C_TAG)));
        if !mood_s.is_empty() {
            meta_spans.push(Span::raw("  "));
        }
    }
    if !mood_s.is_empty() {
        meta_spans.push(Span::styled(mood_s, Style::default().fg(mood_col)));
    }
    f.render_widget(Paragraph::new(Line::from(meta_spans)), chunks[2]);

    // Separator
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(inner.width as usize),
            Style::default().fg(C_BORDER),
        ))),
        chunks[3],
    );

    // Body — word-wrapped, dim leading space for padding
    let body_lines: Vec<Line<'static>> = body
        .lines()
        .map(|l| Line::from(vec![Span::raw(" "), Span::raw(l.to_string())]))
        .collect();
    f.render_widget(
        Paragraph::new(body_lines).wrap(Wrap { trim: false }),
        chunks[4],
    );
}

// ── Show view ─────────────────────────────────────────────────────────────────

fn draw_show(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(entry) = app.selected() else {
        return;
    };

    let date = entry.created_at.format("%Y-%m-%d  %H:%M").to_string();
    let tags: Vec<String> = entry.tags.iter().map(|t| format!("#{}", t)).collect();
    let mood_col = mood_color(&entry.mood);
    let mood_s = entry.mood.as_ref().map(|m| m.to_string()).unwrap_or_default();
    let words_s = format!("{} words", entry.word_count);
    let body = entry.body.clone().unwrap_or_default();
    let scroll = app.show_scroll;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    // Metadata
    let mut meta_spans: Vec<Span<'static>> = vec![
        Span::raw("  "),
        Span::styled(date, Style::default().fg(C_DIM)),
    ];
    if !tags.is_empty() {
        meta_spans.push(Span::raw("    "));
        meta_spans.push(Span::styled(tags.join("  "), Style::default().fg(C_TAG)));
    }
    if !mood_s.is_empty() {
        meta_spans.push(Span::raw("    "));
        meta_spans.push(Span::styled(mood_s, Style::default().fg(mood_col)));
    }
    meta_spans.push(Span::raw("    "));
    meta_spans.push(Span::styled(words_s, Style::default().fg(C_DIM)));

    f.render_widget(
        Paragraph::new(vec![Line::from(vec![]), Line::from(meta_spans)]),
        chunks[0],
    );

    // Separator
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(area.width.saturating_sub(2) as usize),
            Style::default().fg(C_BORDER),
        ))),
        chunks[1],
    );

    // Body
    let body_lines: Vec<Line<'static>> = body
        .lines()
        .map(|l| Line::from(vec![Span::raw("  "), Span::raw(l.to_string())]))
        .collect();
    f.render_widget(
        Paragraph::new(body_lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        chunks[2],
    );
}

// ── Status bar ────────────────────────────────────────────────────────────────

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let line = match &app.mode {
        Mode::Command => Line::from(vec![
            Span::styled("  :", Style::default().fg(C_DIM)),
            Span::styled(
                app.command_buf.clone(),
                Style::default().fg(C_WHITE).add_modifier(Modifier::BOLD),
            ),
        ]),
        Mode::Show => {
            if let Some(msg) = &app.flash {
                Line::from(Span::styled(
                    format!("  ✓  {}", msg),
                    Style::default().fg(C_FLASH),
                ))
            } else {
                hints(&[
                    ("o", "edit"),
                    ("↑↓/jk", "scroll"),
                    ("esc", "back"),
                    ("q", "quit"),
                ])
            }
        }
        Mode::Search => hints(&[
            ("type", "filter"),
            ("↑↓/jk", "navigate"),
            ("↵", "open"),
            ("esc", "clear"),
        ]),
        Mode::List => {
            if let Some(msg) = &app.flash {
                Line::from(Span::styled(
                    format!("  ✓  {}", msg),
                    Style::default().fg(C_FLASH),
                ))
            } else {
                let preview_hint = if app.show_preview { "p  hide preview" } else { "p  preview" };
                hints(&[
                    ("↑↓/jk", "navigate"),
                    ("/", "search"),
                    ("↵", "open"),
                    (preview_hint, ""),
                    ("q", "quit"),
                ])
            }
        }
    };
    f.render_widget(Paragraph::new(line), area);
}

fn hints(pairs: &[(&str, &str)]) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = vec![Span::raw("  ")];
    for (i, (key, desc)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("     ", Style::default()));
        }
        spans.push(Span::styled(
            key.to_string(),
            Style::default().fg(C_WHITE).add_modifier(Modifier::BOLD),
        ));
        if !desc.is_empty() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(desc.to_string(), Style::default().fg(C_DIM)));
        }
    }
    Line::from(spans)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn mood_color(mood: &Option<Mood>) -> Color {
    match mood {
        Some(Mood::Great) => Color::Rgb(100, 220, 120),
        Some(Mood::Good) => Color::Rgb(140, 200, 100),
        Some(Mood::Okay) => Color::Rgb(220, 190, 80),
        Some(Mood::Bad) => Color::Rgb(220, 120, 80),
        Some(Mood::Awful) => Color::Rgb(210, 70, 70),
        None => C_DIM,
    }
}
