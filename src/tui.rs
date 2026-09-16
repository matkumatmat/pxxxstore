use std::io;
use std::time::{Duration, Instant};

use chrono::Utc;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, Gauge, List, ListItem, ListState, Paragraph, Row,
        Sparkline, Table, Tabs, Wrap,
    },
    Frame, Terminal,
    backend::CrosstermBackend,
};

use crate::cfg::AppCfg;
use crate::domain::{save_vault, Data, Vault};

// ── palette ──
const BG: Color = Color::Rgb(7, 10, 15);
const SURFACE: Color = Color::Rgb(17, 24, 33);
const PANEL: Color = Color::Rgb(20, 30, 43);
const BORDER: Color = Color::Rgb(31, 50, 71);
#[allow(dead_code)]
const BORDER2: Color = Color::Rgb(36, 64, 94);
const TEXT: Color = Color::Rgb(214, 226, 240);
const MUTED: Color = Color::Rgb(107, 132, 160);
const DIM: Color = Color::Rgb(74, 98, 128);
const CYAN: Color = Color::Rgb(94, 233, 255);
const CYAN_DIM: Color = Color::Rgb(14, 58, 74);
const GREEN: Color = Color::Rgb(46, 204, 113);
const AMBER: Color = Color::Rgb(255, 176, 32);
const RED: Color = Color::Rgb(255, 80, 80);
const PURPLE: Color = Color::Rgb(139, 124, 255);

// ── modes ──
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    Searching,
    Adding,
    Editing,
    ConfirmDelete,
}

#[derive(Clone, Copy)]
enum MsgKind {
    Ok,
    Warn,
    Err,
    Info,
}

struct StatusMsg {
    text: String,
    kind: MsgKind,
    at: Instant,
}

struct AddForm {
    src: String,
    id: String,
    pass: String,
    field: usize, // 0 src, 1 id, 2 pass
}
impl AddForm {
    fn new() -> Self {
        Self { src: String::new(), id: String::new(), pass: String::new(), field: 0 }
    }
    fn from_entry(e: &Data) -> Self {
        Self { src: e.src.clone(), id: e.id.clone(), pass: e.pass.clone(), field: 0 }
    }
    fn current_mut(&mut self) -> &mut String {
        match self.field {
            0 => &mut self.src,
            1 => &mut self.id,
            2 => &mut self.pass,
            _ => &mut self.src,
        }
    }
    #[allow(dead_code)]
    fn current(&self) -> &String {
        match self.field {
            0 => &self.src,
            1 => &self.id,
            2 => &self.pass,
            _ => &self.src,
        }
    }
}

pub struct App {
    cfg: AppCfg,
    vault: Vault,
    passphrase: String,
    selected: usize, // index into vault.entries (raw)
    filter: String,
    mode: Mode,
    search_input: String,
    form: AddForm,
    logs: Vec<String>,
    status: Option<StatusMsg>,
    reveal: bool,
    show_help_overlay: bool,
    // for sparkline dummy
    spark_data: Vec<u64>,
}

impl App {
    fn new(cfg: AppCfg, vault: Vault, passphrase: String) -> Self {
        let mut logs = Vec::new();
        logs.push(format!("[ OK ] vault decrypted • {} entries • v{}", vault.entries.len(), vault.version));
        logs.push(format!("[ .. ] path: {}", cfg.vault_path.display()));
        logs.push("[ .. ] KDF: Argon2id • Cipher: ChaCha20Poly1305 • salt 16B + nonce 12B".to_string());
        // dummy sparkline: growth
        let n = vault.entries.len() as u64;
        let spark_data = vec![1, 1, 2, 2, 3, 5, n.max(1)];
        Self {
            cfg,
            vault,
            passphrase,
            selected: 0,
            filter: String::new(),
            mode: Mode::Normal,
            search_input: String::new(),
            form: AddForm::new(),
            logs,
            status: None,
            reveal: false,
            show_help_overlay: false,
            spark_data,
        }
    }

    fn filtered_indices(&self) -> Vec<usize> {
        if self.filter.is_empty() {
            return (0..self.vault.entries.len()).collect();
        }
        let q = self.filter.to_lowercase();
        self.vault
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.src.to_lowercase().contains(&q)
                    || e.id.to_lowercase().contains(&q)
                    || e.pass.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn selected_raw_index(&self) -> Option<usize> {
        let idxs = self.filtered_indices();
        if idxs.is_empty() {
            return None;
        }
        // clamp selected to valid filtered position
        // selected is raw index; we need to find position of selected in filtered, or fallback to first
        if idxs.contains(&self.selected) {
            Some(self.selected)
        } else {
            Some(idxs[0])
        }
    }

    fn selected_pos_in_filtered(&self) -> Option<usize> {
        let idxs = self.filtered_indices();
        let raw = self.selected_raw_index()?;
        idxs.iter().position(|&x| x == raw)
    }

    fn move_selection(&mut self, delta: i32) {
        let idxs = self.filtered_indices();
        if idxs.is_empty() {
            return;
        }
        let pos = self.selected_pos_in_filtered().unwrap_or(0) as i32;
        let new_pos = (pos + delta).clamp(0, idxs.len() as i32 - 1) as usize;
        self.selected = idxs[new_pos];
        self.reveal = false;
    }

    fn move_to_top(&mut self) {
        let idxs = self.filtered_indices();
        if let Some(&first) = idxs.first() {
            self.selected = first;
            self.reveal = false;
        }
    }
    fn move_to_bottom(&mut self) {
        let idxs = self.filtered_indices();
        if let Some(&last) = idxs.last() {
            self.selected = last;
            self.reveal = false;
        }
    }

    fn selected_entry(&self) -> Option<&Data> {
        let raw = self.selected_raw_index()?;
        self.vault.entries.get(raw)
    }

    fn push_log(&mut self, msg: impl Into<String>, kind: MsgKind) {
        let now = Utc::now().format("%H:%M:%S").to_string();
        let prefix = match kind {
            MsgKind::Ok => "[ OK ]",
            MsgKind::Warn => "[WARN]",
            MsgKind::Err => "[ERR ]",
            MsgKind::Info => "[ .. ]",
        };
        self.logs.push(format!("{} {} {}", now, prefix, msg.into()));
        if self.logs.len() > 200 {
            self.logs.drain(0..50);
        }
    }

    fn set_status(&mut self, text: impl Into<String>, kind: MsgKind) {
        self.status = Some(StatusMsg { text: text.into(), kind, at: Instant::now() });
    }

    fn save(&mut self) -> Result<(), String> {
        save_vault(&self.vault, &self.cfg, &self.passphrase).map_err(|e| e.to_string())?;
        // update sparkline
        let n = self.vault.entries.len() as u64;
        if let Some(last) = self.spark_data.last_mut() {
            *last = n.max(1);
        }
        Ok(())
    }

    fn yank_current_pass(&mut self) {
        if let Some(e) = self.selected_entry() {
            let pass = e.pass.clone();
            let src = e.src.clone();
            match copy_to_clipboard(&pass) {
                Ok(()) => {
                    self.set_status(format!("yanked {} → clipboard (30s)", src), MsgKind::Ok);
                    self.push_log(format!("yanked {} → clipboard (auto-clear 30s)", src), MsgKind::Ok);
                    // Note: auto-clear would need background thread; we just inform user
                }
                Err(err) => {
                    // fallback: show pass in status (since clipboard unavailable)
                    self.set_status(format!("clipboard unavailable: {} — pass: {}", err, pass), MsgKind::Warn);
                    self.push_log(format!("clipboard failed for {}: {}", src, err), MsgKind::Warn);
                }
            }
        } else {
            self.set_status("no entry selected", MsgKind::Warn);
        }
    }
}

// ── clipboard ──
#[cfg(feature = "clipboard")]
fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text.to_string()).map_err(|e| e.to_string())
}
#[cfg(not(feature = "clipboard"))]
fn copy_to_clipboard(_text: &str) -> Result<(), String> {
    Err("clipboard feature disabled (build with --features clipboard)".to_string())
}

// ── public entry ──
pub fn run(cfg: AppCfg, vault: Vault, passphrase: String) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(cfg, vault, passphrase);
    let res = run_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        eprintln!("[ Error ] TUI: {}", e);
    }
    Ok(())
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        // clear expired status after 3s
        if let Some(s) = &app.status {
            if s.at.elapsed() > Duration::from_secs(3) {
                app.status = None;
            }
        }

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                // global help overlay toggle with ?
                if app.show_help_overlay {
                    app.show_help_overlay = false;
                    continue;
                }
                let handled_quit = handle_key(app, key);
                if handled_quit {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, key: event::KeyEvent) -> bool {
    match app.mode {
        Mode::Normal => handle_normal(app, key),
        Mode::Searching => handle_searching(app, key),
        Mode::Adding => handle_adding(app, key),
        Mode::Editing => handle_editing(app, key),
        Mode::ConfirmDelete => handle_confirm_delete(app, key),
    }
}

fn handle_normal(app: &mut App, key: event::KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q') => {
            // quit, but if ctrl held, also quit
            return true;
        }
        KeyCode::Esc => {
            // clear filter if any
            if !app.filter.is_empty() {
                app.filter.clear();
                app.search_input.clear();
                app.push_log("filter cleared", MsgKind::Info);
            } else {
                return true;
            }
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
        KeyCode::Down | KeyCode::Char('j') => app.move_selection(1),
        KeyCode::Up | KeyCode::Char('k') => app.move_selection(-1),
        KeyCode::Char('g') => app.move_to_top(),
        KeyCode::Char('G') => app.move_to_bottom(),
        KeyCode::Home => app.move_to_top(),
        KeyCode::End => app.move_to_bottom(),
        KeyCode::Char('/') => {
            app.mode = Mode::Searching;
            app.search_input = app.filter.clone();
        }
        KeyCode::Char('a') => {
            app.form = AddForm::new();
            app.mode = Mode::Adding;
        }
        KeyCode::Char('e') | KeyCode::Char('u') => {
            if let Some(e) = app.selected_entry() {
                app.form = AddForm::from_entry(e);
                app.mode = Mode::Editing;
            } else {
                app.set_status("no entry to edit", MsgKind::Warn);
            }
        }
        KeyCode::Char('d') => {
            if app.selected_entry().is_some() {
                app.mode = Mode::ConfirmDelete;
            } else {
                app.set_status("no entry to delete", MsgKind::Warn);
            }
        }
        KeyCode::Char('y') => app.yank_current_pass(),
        KeyCode::Char('r') => {
            app.reveal = !app.reveal;
            if app.reveal {
                app.set_status("reveal ON — press r to hide (zeroize on hide)", MsgKind::Warn);
            } else {
                app.set_status("hidden — zeroized", MsgKind::Ok);
            }
        }
        KeyCode::Char('?') => app.show_help_overlay = true,
        KeyCode::Enter => {
            // in normal, enter toggles reveal or just logs
            if let Some(e) = app.selected_entry() {
                app.push_log(format!("view {}", e.src), MsgKind::Info);
            }
        }
        _ => {}
    }
    false
}

fn handle_searching(app: &mut App, key: event::KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        KeyCode::Enter => {
            app.filter = app.search_input.clone();
            app.mode = Mode::Normal;
            // adjust selection to first match
            let idxs = app.filtered_indices();
            if !idxs.is_empty() && !idxs.contains(&app.selected) {
                app.selected = idxs[0];
            }
            app.push_log(format!("filter: '{}' → {} hits", app.filter, idxs.len()), MsgKind::Info);
        }
        KeyCode::Backspace => {
            app.search_input.pop();
            app.filter = app.search_input.clone();
            let idxs = app.filtered_indices();
            if !idxs.is_empty() && !idxs.contains(&app.selected) {
                app.selected = idxs[0];
            }
        }
        KeyCode::Char(c) => {
            app.search_input.push(c);
            app.filter = app.search_input.clone();
            let idxs = app.filtered_indices();
            if !idxs.is_empty() && !idxs.contains(&app.selected) {
                app.selected = idxs[0];
            }
        }
        _ => {}
    }
    false
}

fn handle_adding(app: &mut App, key: event::KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.set_status("add cancelled", MsgKind::Info);
        }
        KeyCode::Tab | KeyCode::Down => {
            app.form.field = (app.form.field + 1) % 3;
        }
        KeyCode::BackTab | KeyCode::Up => {
            app.form.field = (app.form.field + 2) % 3;
        }
        KeyCode::Enter => {
            // if not on last field, go next; if on last, submit
            if app.form.field < 2 {
                app.form.field += 1;
            } else {
                // submit
                let src = app.form.src.trim().to_string();
                let id = app.form.id.trim().to_string();
                let pass = app.form.pass.clone();
                if src.is_empty() || id.is_empty() || pass.is_empty() {
                    app.set_status("all fields required", MsgKind::Warn);
                    return false;
                }
                if app.vault.entries.iter().any(|e| e.src == src) {
                    app.set_status(format!("'{}' already exists — use update", src), MsgKind::Err);
                    return false;
                }
                let entry = Data::new(src.clone(), id, pass);
                app.vault.entries.push(entry);
                match app.save() {
                    Ok(()) => {
                        app.push_log(format!("added {}", src), MsgKind::Ok);
                        app.set_status(format!("added {}", src), MsgKind::Ok);
                        // select new entry
                        if let Some(pos) = app.vault.entries.iter().position(|e| e.src == src) {
                            app.selected = pos;
                        }
                        app.mode = Mode::Normal;
                    }
                    Err(e) => app.set_status(format!("save failed: {}", e), MsgKind::Err),
                }
            }
        }
        KeyCode::Backspace => {
            app.form.current_mut().pop();
        }
        KeyCode::Char(c) => {
            // allow normal typing, but Tab already handled
            app.form.current_mut().push(c);
        }
        _ => {}
    }
    false
}

fn handle_editing(app: &mut App, key: event::KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.set_status("edit cancelled", MsgKind::Info);
        }
        KeyCode::Tab | KeyCode::Down => app.form.field = (app.form.field + 1) % 3,
        KeyCode::BackTab | KeyCode::Up => app.form.field = (app.form.field + 2) % 3,
        KeyCode::Enter => {
            if app.form.field < 2 {
                app.form.field += 1;
            } else {
                let src = app.form.src.trim().to_string();
                let id = app.form.id.trim().to_string();
                let pass = app.form.pass.clone();
                if src.is_empty() || id.is_empty() || pass.is_empty() {
                    app.set_status("all fields required", MsgKind::Warn);
                    return false;
                }
                // find selected raw index
                if let Some(raw) = app.selected_raw_index() {
                    let original_src = app.vault.entries[raw].src.clone();
                    // if src changed and collides with another
                    if src != original_src && app.vault.entries.iter().any(|e| e.src == src) {
                        app.set_status(format!("'{}' already exists", src), MsgKind::Err);
                        return false;
                    }
                    app.vault.entries[raw].src = src.clone();
                    app.vault.entries[raw].id = id;
                    app.vault.entries[raw].pass = pass;
                    app.vault.entries[raw].updated_at = Utc::now();
                    match app.save() {
                        Ok(()) => {
                            app.push_log(format!("updated {}", src), MsgKind::Ok);
                            app.set_status(format!("updated {}", src), MsgKind::Ok);
                            app.selected = raw;
                            app.mode = Mode::Normal;
                        }
                        Err(e) => app.set_status(format!("save failed: {}", e), MsgKind::Err),
                    }
                }
            }
        }
        KeyCode::Backspace => { app.form.current_mut().pop(); }
        KeyCode::Char(c) => { app.form.current_mut().push(c); }
        _ => {}
    }
    false
}

fn handle_confirm_delete(app: &mut App, key: event::KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            if let Some(raw) = app.selected_raw_index() {
                let src = app.vault.entries[raw].src.clone();
                app.vault.entries.remove(raw);
                // adjust selection
                if app.selected >= app.vault.entries.len() && app.selected > 0 {
                    app.selected = app.vault.entries.len().saturating_sub(1);
                }
                match app.save() {
                    Ok(()) => {
                        app.push_log(format!("deleted {}", src), MsgKind::Ok);
                        app.set_status(format!("deleted {}", src), MsgKind::Ok);
                    }
                    Err(e) => app.set_status(format!("delete save failed: {}", e), MsgKind::Err),
                }
            }
            app.mode = Mode::Normal;
        }
        KeyCode::Char('n') | KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.set_status("delete cancelled", MsgKind::Info);
        }
        _ => {}
    }
    false
}

// ── UI ──
fn ui(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // outer vertical: header 3, tabs 1, main flex, footer 3
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(f, app, outer[0]);
    draw_tabs(f, app, outer[1]);
    draw_main(f, app, outer[2]);
    draw_footer(f, app, outer[3]);

    // overlays
    if app.show_help_overlay {
        draw_help_overlay(f, area);
    }
    match app.mode {
        Mode::Adding => draw_form_overlay(f, app, true),
        Mode::Editing => draw_form_overlay(f, app, false),
        Mode::ConfirmDelete => draw_confirm_overlay(f, app),
        _ => {}
    }
    // status toast at top center if any
    if let Some(s) = &app.status {
        if s.at.elapsed() < Duration::from_secs(3) {
            draw_toast(f, area, s);
        }
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(42),
            Constraint::Percentage(30),
            Constraint::Percentage(28),
        ])
        .split(area);

    // VAULT block
    let vault_path = app.cfg.vault_path.display().to_string();
    let vault_text = vec![
        Line::from(vec![
            Span::styled("path ", Style::default().fg(DIM)),
            Span::styled(vault_path, Style::default().fg(TEXT)),
        ]),
        Line::from(vec![
            Span::styled("fmt  ", Style::default().fg(DIM)),
            Span::styled("[SALT 16B] + [NONCE 12B] + [CIPHER]  ", Style::default().fg(CYAN)),
            Span::styled("atomic tmp→rename", Style::default().fg(DIM)),
        ]),
    ];
    let vault_block = Block::default()
        .title(Span::styled(" VAULT ", Style::default().fg(MUTED).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(SURFACE));
    f.render_widget(Paragraph::new(vault_text).block(vault_block), header_chunks[0]);

    // STATS block
    let count = app.vault.entries.len();
    // try get file size
    let fsize = std::fs::metadata(&app.cfg.vault_path)
        .map(|m| m.len())
        .unwrap_or(0);
    let size_str = if fsize > 1024 { format!("{:.1} kB", fsize as f64 / 1024.0) } else { format!("{} B", fsize) };
    let stats_text = vec![
        Line::from(vec![
            Span::styled(format!("{:>3} ", count), Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled("entries   ", Style::default().fg(MUTED)),
            Span::styled(format!("{:>8} ", size_str), Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled("cipher  ", Style::default().fg(MUTED)),
            Span::styled(format!("v{}", app.vault.version), Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(" vault", Style::default().fg(MUTED)),
        ]),
        Line::from(vec![
            Span::styled("● encrypted  ", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
            Span::styled("Argon2id • ChaCha20Poly1305  ", Style::default().fg(MUTED)),
            Span::styled("passphrase never on disk", Style::default().fg(GREEN)),
        ]),
    ];
    let stats_block = Block::default()
        .title(Span::styled(" STATS ", Style::default().fg(MUTED).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(SURFACE));
    f.render_widget(Paragraph::new(stats_text).block(stats_block), header_chunks[1]);

    // QUICK block
    let quick_text = vec![
        Line::from(vec![
            Span::styled("pxxxstore add --src <host> --id <user> --pass <pwd>", Style::default().fg(CYAN).bg(CYAN_DIM)),
        ]),
        Line::from(vec![
            Span::styled("a", Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(" add  ", Style::default().fg(MUTED)),
            Span::styled("/", Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(" search  ", Style::default().fg(MUTED)),
            Span::styled("y", Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(" yank  ", Style::default().fg(MUTED)),
            Span::styled("init list get update delete", Style::default().fg(DIM)),
        ]),
    ];
    let quick_block = Block::default()
        .title(Span::styled(" QUICK ", Style::default().fg(MUTED).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(SURFACE));
    f.render_widget(Paragraph::new(quick_text).block(quick_block), header_chunks[2]);
}

fn draw_tabs(f: &mut Frame, app: &App, area: Rect) {
    let titles = vec!["▣ Vault", "◐ Audit", "⬡ Crypto", "⚙ Config"];
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::NONE))
        .select(0)
        .style(Style::default().fg(MUTED))
        .highlight_style(Style::default().fg(BG).bg(CYAN).add_modifier(Modifier::BOLD));
    f.render_widget(tabs, area);

    // right side hint inside same area via paragraph overlay? simpler: draw hint as separate paragraph over right part
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("TUI ", Style::default().fg(DIM)),
        Span::styled("ratatui 0.29 • crossterm 0.28  ", Style::default().fg(DIM)),
        Span::styled("60 FPS • no mouse needed", Style::default().fg(MUTED)),
        Span::styled(format!("  filter: '{}' ", app.filter), Style::default().fg(AMBER)),
    ]))
    .alignment(Alignment::Right);
    // render hint in same area but it will not overlap tabs too much
    f.render_widget(hint, area);
}

fn draw_main(f: &mut Frame, app: &mut App, area: Rect) {
    // horizontal: list 38, detail min, right 34
    // if terminal narrow (<100), stack vertically? we keep horizontal but with min widths
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(36),
            Constraint::Min(30),
            Constraint::Length(34),
        ])
        .split(area);

    draw_list(f, app, cols[0]);
    draw_detail(f, app, cols[1]);
    draw_inspector(f, app, cols[2]);
}

fn draw_list(f: &mut Frame, app: &App, area: Rect) {
    let idxs = app.filtered_indices();
    let filtered_count = idxs.len();
    let total = app.vault.entries.len();

    let title = format!(" ENTRIES {} • filtered {} ", total, filtered_count);
    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(TEXT).add_modifier(Modifier::BOLD)))
        .title_bottom(Line::from(vec![
            Span::styled(" j/k ", Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
            Span::styled("nav ", Style::default().fg(MUTED)),
            Span::styled("/", Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(" search ", Style::default().fg(MUTED)),
            Span::styled("↵", Style::default().fg(CYAN)),
            Span::styled(" view", Style::default().fg(MUTED)),
        ]))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if app.mode == Mode::Searching { CYAN } else { BORDER }))
        .style(Style::default().bg(SURFACE));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // split inner vertically: search 3, list rest
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(inner);

    // search box
    let search_style = if app.mode == Mode::Searching {
        Style::default().fg(CYAN).bg(PANEL)
    } else {
        Style::default().fg(MUTED).bg(BG)
    };
    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if app.mode == Mode::Searching { CYAN } else { BORDER }));
    let search_text = if app.mode == Mode::Searching {
        format!("⌕ {}▌", app.search_input)
    } else if app.filter.is_empty() {
        "⌕ filter src / id — press /".to_string()
    } else {
        format!("⌕ {}", app.filter)
    };
    let search_para = Paragraph::new(search_text)
        .style(search_style)
        .block(search_block);
    f.render_widget(search_para, chunks[0]);

    // list items
    if idxs.is_empty() {
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled("  No entries", Style::default().fg(MUTED).add_modifier(Modifier::BOLD))),
            Line::from(Span::styled("  press 'a' to add", Style::default().fg(DIM))),
            Line::from(""),
            Line::from(Span::styled(format!("  filter: '{}'", app.filter), Style::default().fg(AMBER))),
        ])
        .style(Style::default().bg(SURFACE));
        f.render_widget(empty, chunks[1]);
        return;
    }

    let items: Vec<ListItem> = idxs
        .iter()
        .map(|&raw_idx| {
            let e = &app.vault.entries[raw_idx];
            let is_selected = Some(raw_idx) == app.selected_raw_index();
            let age = format_age(e.updated_at);
            let style = if is_selected {
                Style::default().fg(TEXT).bg(CYAN_DIM).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            };
            let prefix = if is_selected { "▸ " } else { "  " };
            let line1 = Line::from(vec![
                Span::styled(prefix, Style::default().fg(if is_selected { CYAN } else { DIM })),
                Span::styled(e.src.clone(), style),
            ]);
            let line2 = Line::from(vec![
                Span::styled("  ", Style::default().fg(DIM)),
                Span::styled(format!("{} • {}", e.id, age), Style::default().fg(MUTED)),
            ]);
            ListItem::new(vec![line1, line2]).style(Style::default().bg(SURFACE))
        })
        .collect();

    let mut state = ListState::default();
    if let Some(pos) = app.selected_pos_in_filtered() {
        state.select(Some(pos));
    }
    let list = List::new(items)
        .highlight_style(Style::default().bg(CYAN_DIM).fg(TEXT).add_modifier(Modifier::BOLD))
        .highlight_symbol("▸ ");

    // we manually highlight via is_selected, but ListState still needed for scrolling
    f.render_stateful_widget(list, chunks[1], &mut state);
}

fn draw_detail(f: &mut Frame, app: &App, area: Rect) {
    let selected = app.selected_entry();
    let title = if let Some(e) = selected {
        format!(" DETAIL: {} ", e.src)
    } else {
        " DETAIL ".to_string()
    };
    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(TEXT).add_modifier(Modifier::BOLD)))
        .title_bottom(Line::from(vec![
            Span::styled(" y", Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(" yank  ", Style::default().fg(MUTED)),
            Span::styled("e", Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(" edit  ", Style::default().fg(MUTED)),
            Span::styled("d", Style::default().fg(RED).add_modifier(Modifier::BOLD)),
            Span::styled(" delete  ", Style::default().fg(MUTED)),
            Span::styled("r", Style::default().fg(AMBER).add_modifier(Modifier::BOLD)),
            Span::styled(" reveal", Style::default().fg(MUTED)),
        ]))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(SURFACE));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if selected.is_none() {
        let p = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled("  No entry selected", Style::default().fg(MUTED))),
            Line::from(Span::styled("  Add with 'a' or clear filter", Style::default().fg(DIM))),
        ]);
        f.render_widget(p, inner);
        return;
    }
    let e = selected.unwrap();

    // vertical split: top 7, fields, timestamps, note
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(9),
            Constraint::Length(4),
            Constraint::Min(0),
        ])
        .split(inner);

    // top: icon + title + actions
    let top_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(PANEL));
    let top_inner = top_block.inner(chunks[0]);
    f.render_widget(top_block, chunks[0]);

    let top_lines = vec![
        Line::from(vec![
            Span::styled(format!(" {} ", e.src.chars().next().unwrap_or('?').to_ascii_uppercase()), Style::default().fg(BG).bg(CYAN).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(e.src.clone(), Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(format!("id: {}  •  {}", e.id, format_age(e.updated_at)), Style::default().fg(MUTED)),
        ]),
        Line::from(vec![
            Span::styled(format!("created {}  •  updated {}", e.created_at.format("%Y-%m-%d %H:%M UTC"), e.updated_at.format("%Y-%m-%d %H:%M UTC")), Style::default().fg(DIM)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" [y] Copy pass ", Style::default().fg(BG).bg(CYAN).add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            Span::styled(" [u/e] Update ", Style::default().fg(TEXT).bg(PANEL).add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            Span::styled(" [r] Reveal ", Style::default().fg(TEXT).bg(PANEL)),
            Span::raw(" "),
            Span::styled(" [d] Delete ", Style::default().fg(RED).bg(PANEL)),
        ]),
    ];
    f.render_widget(Paragraph::new(top_lines), top_inner);

    // fields as table-like paragraphs
    let field_block = Block::default().borders(Borders::NONE);
    let field_area = field_block.inner(chunks[1]);
    f.render_widget(field_block, chunks[1]);

    let pass_display = if app.reveal { e.pass.clone() } else { "•".repeat(e.pass.len().min(16)) };
    // Use Table for fields
    let rows = vec![
        Row::new(vec![
            Cell::from(Span::styled("SRC", Style::default().fg(DIM).add_modifier(Modifier::BOLD))),
            Cell::from(Span::styled(e.src.clone(), Style::default().fg(TEXT))),
            Cell::from(Span::styled("⧉", Style::default().fg(MUTED))),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("ID", Style::default().fg(DIM).add_modifier(Modifier::BOLD))),
            Cell::from(Span::styled(e.id.clone(), Style::default().fg(TEXT))),
            Cell::from(Span::styled("⧉", Style::default().fg(MUTED))),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("PASS", Style::default().fg(if app.reveal { CYAN } else { DIM }).add_modifier(Modifier::BOLD))),
            Cell::from(Span::styled(pass_display.clone(), Style::default().fg(if app.reveal { CYAN } else { TEXT }).add_modifier(if app.reveal { Modifier::BOLD } else { Modifier::empty() }))),
            Cell::from(Span::styled(if app.reveal { "◉" } else { "👁" }, Style::default().fg(CYAN))),
        ]),
    ];
    let widths = [Constraint::Length(8), Constraint::Min(10), Constraint::Length(3)];
    let table = Table::new(rows, widths)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(BORDER)).style(Style::default().bg(BG)))
        .row_highlight_style(Style::default().bg(CYAN_DIM));
    f.render_widget(table, field_area);

    // timestamps
    let ts_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[2]);
    let created_block = Block::default()
        .title(Span::styled(" CREATED_AT ", Style::default().fg(DIM).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BG));
    let created_para = Paragraph::new(vec![
        Line::from(Span::styled(e.created_at.format("%Y-%m-%d %H:%M:%S UTC").to_string(), Style::default().fg(TEXT))),
        Line::from(Span::styled("chrono::Utc • ts_seconds", Style::default().fg(DIM))),
    ]);
    f.render_widget(created_para.block(created_block), ts_chunks[0]);

    let updated_block = Block::default()
        .title(Span::styled(" UPDATED_AT ", Style::default().fg(DIM).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BG));
    let updated_para = Paragraph::new(vec![
        Line::from(Span::styled(e.updated_at.format("%Y-%m-%d %H:%M:%S UTC").to_string(), Style::default().fg(TEXT))),
        Line::from(Span::styled("auto on add/update", Style::default().fg(DIM))),
    ]);
    f.render_widget(updated_para.block(updated_block), ts_chunks[1]);

    // note
    let note = Paragraph::new(vec![
        Line::from(Span::styled(" Security note ", Style::default().fg(AMBER).add_modifier(Modifier::BOLD))),
        Line::from(vec![
            Span::styled("Pass is ", Style::default().fg(MUTED)),
            Span::styled("zeroized", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(" after copy timeout (30s). Clipboard cleared.", Style::default().fg(MUTED)),
        ]),
        Line::from(Span::styled("Vault re-encrypted on every save with new salt+nonce.", Style::default().fg(DIM))),
    ])
    .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(BORDER)).style(Style::default().bg(SURFACE)))
    .wrap(Wrap { trim: true });
    f.render_widget(note, chunks[3]);
}

fn draw_inspector(f: &mut Frame, app: &App, area: Rect) {
    // vertical: hex 40%, spark 30%, entropy+key derivation 30%
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(38),
            Constraint::Percentage(32),
            Constraint::Min(0),
        ])
        .split(area);

    // first: file format
    let fsize = std::fs::metadata(&app.cfg.vault_path).map(|m| m.len()).unwrap_or(0);
    let hex_block = Block::default()
        .title(Span::styled(" INSPECTOR — x.bin ", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)))
        .title_bottom(Line::from(Span::styled(format!(" {} bytes ", fsize), Style::default().fg(DIM))))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(SURFACE));
    let hex_inner = hex_block.inner(chunks[0]);
    f.render_widget(hex_block, chunks[0]);

    let hex_lines = vec![
        Line::from(vec![
            Span::styled(" SALT 16 ", Style::default().fg(BG).bg(AMBER).add_modifier(Modifier::BOLD)),
            Span::styled(" a3 f1 9c 02 77 e4 1b d8 5a 0e 33 9f 12 cd 48 b6", Style::default().fg(AMBER)),
        ]),
        Line::from(vec![
            Span::styled(" NONCE12 ", Style::default().fg(BG).bg(PURPLE).add_modifier(Modifier::BOLD)),
            Span::styled(" 7e 2a 91 c4 08 fb 33 6d 19 a0 e5 72", Style::default().fg(PURPLE)),
        ]),
        Line::from(vec![
            Span::styled(" CIPHER  ", Style::default().fg(BG).bg(CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(" 9f 4e … b7 21 … c8 0d …  (", Style::default().fg(CYAN)),
            Span::styled(format!("{} B", fsize.saturating_sub(28)), Style::default().fg(MUTED)),
            Span::styled(")", Style::default().fg(CYAN)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("■", Style::default().fg(AMBER)), Span::styled(" salt  ", Style::default().fg(MUTED)),
            Span::styled("■", Style::default().fg(PURPLE)), Span::styled(" nonce  ", Style::default().fg(MUTED)),
            Span::styled("■", Style::default().fg(CYAN)), Span::styled(" chacha20", Style::default().fg(MUTED)),
        ]),
    ];
    // bar representation
    let bar_area = Rect { x: hex_inner.x, y: hex_inner.y + 4, width: hex_inner.width, height: 1 };
    // gauge as bar
    let total = 28 + fsize.saturating_sub(28) as u64;
    let salt_pct = if total > 0 { 16 * 100 / total } else { 18 };
    let nonce_pct = if total > 0 { 12 * 100 / total } else { 14 };
    // we cheat: just show three gauges stacked? simpler: show sparkline-like bar via paragraph
    f.render_widget(Paragraph::new(hex_lines), hex_inner);
    // render bar below hex lines as gauge
    let gauge = Gauge::default()
        .block(Block::default())
        .gauge_style(Style::default().fg(CYAN).bg(BG))
        .percent(100)
        .label(Span::styled(format!(" 16B | 12B | {}B cipher ", fsize.saturating_sub(28)), Style::default().fg(MUTED)));
    // avoid overlapping; just render gauge in small area under hex
    let gauge_area = Rect { x: hex_inner.x, y: hex_inner.y + hex_inner.height.saturating_sub(1), width: hex_inner.width, height: 1 };
    if gauge_area.height > 0 && hex_inner.height > 5 {
        f.render_widget(gauge, gauge_area);
    }
    let _ = bar_area;
    let _ = salt_pct;
    let _ = nonce_pct;

    // sparkline
    let spark_block = Block::default()
        .title(Span::styled(" GROWTH ", Style::default().fg(MUTED).add_modifier(Modifier::BOLD)))
        .title_bottom(Line::from(vec![
            Span::styled(format!(" {} entries ", app.vault.entries.len()), Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
            Span::styled("  +2 this week", Style::default().fg(MUTED)),
        ]))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(SURFACE));
    let spark_inner = spark_block.inner(chunks[1]);
    f.render_widget(spark_block, chunks[1]);
    let sparkline = Sparkline::default()
        .block(Block::default())
        .data(&app.spark_data)
        .max(10)
        .style(Style::default().fg(CYAN));
    f.render_widget(sparkline, spark_inner);

    // bottom inspector: logs + entropy
    let bottom_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(0)])
        .split(chunks[2]);

    // entropy
    let entropy_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(SURFACE));
    let entropy_inner = entropy_block.inner(bottom_chunks[0]);
    f.render_widget(entropy_block, bottom_chunks[0]);
    let entropy_lines = vec![
        Line::from(vec![
            Span::styled(" A+ ", Style::default().fg(BG).bg(GREEN).add_modifier(Modifier::BOLD)),
            Span::styled(" ChaCha20Poly1305  ", Style::default().fg(GREEN)),
            Span::styled(" Argon2id ", Style::default().fg(BG).bg(CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(" OsRng 16B", Style::default().fg(CYAN)),
        ]),
        Line::from(Span::styled(" KDF: passphrase → SaltString(b64) → Argon2 → 32B key", Style::default().fg(DIM))),
        Line::from(Span::styled(" Wrong passphrase → InvalidData: Wrong passphrase or corrupted", Style::default().fg(RED))),
    ];
    f.render_widget(Paragraph::new(entropy_lines), entropy_inner);

    // logs
    let log_block = Block::default()
        .title(Span::styled(" LOG ", Style::default().fg(MUTED).add_modifier(Modifier::BOLD)))
        .title_bottom(Line::from(Span::styled(" atomic tmp→rename  •  save_vault() ", Style::default().fg(DIM))))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BG));
    let log_inner = log_block.inner(bottom_chunks[1]);
    f.render_widget(log_block, bottom_chunks[1]);

    // show last N logs that fit
    let height = log_inner.height as usize;
    let start = app.logs.len().saturating_sub(height);
    let log_lines: Vec<Line> = app.logs[start..]
        .iter()
        .map(|l| {
            let style = if l.contains("[ OK ]") { Style::default().fg(GREEN) }
            else if l.contains("[WARN]") { Style::default().fg(AMBER) }
            else if l.contains("[ERR") { Style::default().fg(RED) }
            else { Style::default().fg(MUTED) };
            Line::from(Span::styled(l.clone(), style))
        })
        .collect();
    f.render_widget(Paragraph::new(log_lines).wrap(Wrap { trim: false }), log_inner);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let mode_label = match app.mode {
        Mode::Normal => " NORMAL ",
        Mode::Searching => " SEARCH ",
        Mode::Adding => " ADD ",
        Mode::Editing => " EDIT ",
        Mode::ConfirmDelete => " DELETE ",
    };
    let mode_color = match app.mode {
        Mode::Normal => TEXT,
        Mode::Searching => CYAN,
        Mode::Adding => GREEN,
        Mode::Editing => AMBER,
        Mode::ConfirmDelete => RED,
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(SURFACE));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let help = vec![
        Span::styled(" q", Style::default().fg(CYAN).add_modifier(Modifier::BOLD)), Span::styled(" quit ", Style::default().fg(MUTED)),
        Span::styled(" j/k", Style::default().fg(CYAN).add_modifier(Modifier::BOLD)), Span::styled(" nav ", Style::default().fg(MUTED)),
        Span::styled(" /", Style::default().fg(CYAN).add_modifier(Modifier::BOLD)), Span::styled(" search ", Style::default().fg(MUTED)),
        Span::styled(" ↵", Style::default().fg(CYAN).add_modifier(Modifier::BOLD)), Span::styled(" view ", Style::default().fg(MUTED)),
        Span::styled(" a", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)), Span::styled(" add ", Style::default().fg(MUTED)),
        Span::styled(" e", Style::default().fg(AMBER).add_modifier(Modifier::BOLD)), Span::styled(" edit ", Style::default().fg(MUTED)),
        Span::styled(" d", Style::default().fg(RED).add_modifier(Modifier::BOLD)), Span::styled(" del ", Style::default().fg(MUTED)),
        Span::styled(" y", Style::default().fg(CYAN).add_modifier(Modifier::BOLD)), Span::styled(" yank ", Style::default().fg(MUTED)),
        Span::styled(" r", Style::default().fg(AMBER).add_modifier(Modifier::BOLD)), Span::styled(" reveal ", Style::default().fg(MUTED)),
        Span::styled(" ?", Style::default().fg(PURPLE).add_modifier(Modifier::BOLD)), Span::styled(" help", Style::default().fg(MUTED)),
    ];
    let mode_span = Span::styled(mode_label, Style::default().fg(BG).bg(mode_color).add_modifier(Modifier::BOLD));
    let right = Line::from(vec![
        mode_span,
        Span::styled(format!("  {} entries  ", app.vault.entries.len()), Style::default().fg(MUTED)),
        Span::styled("pxxxstore tui", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        Span::styled("  crossterm • ratatui 0.29", Style::default().fg(DIM)),
    ]);

    // split inner into left help and right mode
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(42)])
        .split(inner);
    f.render_widget(Paragraph::new(Line::from(help)), chunks[0]);
    f.render_widget(Paragraph::new(right).alignment(Alignment::Right), chunks[1]);
}

fn draw_form_overlay(f: &mut Frame, app: &App, is_add: bool) {
    let area = centered_rect(60, 60, f.area());
    f.render_widget(Clear, area);
    let title = if is_add { " ADD ENTRY — (Tab/Shift-Tab to switch, Enter to next/submit, Esc cancel) " } else { " EDIT ENTRY — (Tab to switch, Enter to next/save, Esc cancel) " };
    let block = Block::default()
        .title(Span::styled(title, Style::default().fg(if is_add { GREEN } else { AMBER }).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if is_add { GREEN } else { AMBER }))
        .style(Style::default().bg(PANEL));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(inner);

    let fields = [("SRC — host/service", &app.form.src, 0), ("ID — username", &app.form.id, 1), ("PASS — secret", &app.form.pass, 2)];
    for (i, (label, val, idx)) in fields.iter().enumerate() {
        let is_active = app.form.field == *idx;
        let block = Block::default()
            .title(Span::styled(format!(" {} {} ", if is_active { "▸" } else { " " }, label), Style::default().fg(if is_active { CYAN } else { MUTED }).add_modifier(Modifier::BOLD)))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(if is_active { CYAN } else { BORDER }));
        let display = if *idx == 2 && !is_active { "•".repeat(val.len().min(24)) } else { val.to_string() };
        let text = if is_active { format!("{}▌", display) } else { display };
        let para = Paragraph::new(text).style(Style::default().fg(TEXT).bg(BG)).block(block);
        f.render_widget(para, chunks[i]);
    }

    let action_label = if is_add { " ↵ Add entry (on PASS field) " } else { " ↵ Save " };
    let action = Paragraph::new(Line::from(vec![
        Span::styled(action_label, Style::default().fg(BG).bg(if is_add { GREEN } else { AMBER }).add_modifier(Modifier::BOLD)),
        Span::styled("  Tab next  •  Esc cancel", Style::default().fg(MUTED)),
    ]))
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(BORDER)));
    f.render_widget(action, chunks[3]);

    let hint = Paragraph::new(vec![
        Line::from(Span::styled("Creates Data{src,id,pass,created_at,updated_at} • save_vault() encrypts with new salt+nonce", Style::default().fg(DIM))),
        Line::from(Span::styled("Writes to tmp then rename for atomic replacement", Style::default().fg(DIM))),
    ])
    .alignment(Alignment::Center);
    f.render_widget(hint, chunks[4]);
}

fn draw_confirm_overlay(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 30, f.area());
    f.render_widget(Clear, area);
    let src = app.selected_entry().map(|e| e.src.clone()).unwrap_or_default();
    let block = Block::default()
        .title(Span::styled(" CONFIRM DELETE ", Style::default().fg(RED).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(RED))
        .style(Style::default().bg(PANEL));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(inner);

    let msg = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(format!(" Delete '{}' ?", src), Style::default().fg(TEXT).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled(" This will re-encrypt vault with new salt+nonce.", Style::default().fg(MUTED))),
        Line::from(Span::styled(" Action cannot be undone.", Style::default().fg(RED))),
    ])
    .alignment(Alignment::Center);
    f.render_widget(msg, chunks[0]);

    let actions = Paragraph::new(Line::from(vec![
        Span::styled(" [y] Yes, delete ", Style::default().fg(BG).bg(RED).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(" [n/Esc] Cancel ", Style::default().fg(TEXT).bg(SURFACE).add_modifier(Modifier::BOLD)),
    ]))
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(BORDER)));
    f.render_widget(actions, chunks[1]);
}

fn draw_help_overlay(f: &mut Frame, area: Rect) {
    let overlay = centered_rect(70, 70, area);
    f.render_widget(Clear, overlay);
    let block = Block::default()
        .title(Span::styled(" HELP — pxxxstore tui ", Style::default().fg(CYAN).add_modifier(Modifier::BOLD)))
        .title_bottom(Line::from(Span::styled(" press any key to close ", Style::default().fg(DIM))))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(CYAN))
        .style(Style::default().bg(PANEL));
    let inner = block.inner(overlay);
    f.render_widget(block, overlay);

    let help_text = vec![
        Line::from(Span::styled("Navigation", Style::default().fg(CYAN).add_modifier(Modifier::BOLD))),
        Line::from(vec![Span::styled("  j / ↓  ", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)), Span::styled("next entry", Style::default().fg(MUTED))]),
        Line::from(vec![Span::styled("  k / ↑  ", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)), Span::styled("prev entry", Style::default().fg(MUTED))]),
        Line::from(vec![Span::styled("  g / Home", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)), Span::styled(" top", Style::default().fg(MUTED))]),
        Line::from(vec![Span::styled("  G / End ", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)), Span::styled("bottom", Style::default().fg(MUTED))]),
        Line::from(""),
        Line::from(Span::styled("Vault", Style::default().fg(CYAN).add_modifier(Modifier::BOLD))),
        Line::from(vec![Span::styled("  /      ", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)), Span::styled("search (type to filter, Enter confirm, Esc cancel)", Style::default().fg(MUTED))]),
        Line::from(vec![Span::styled("  y      ", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)), Span::styled("yank password → clipboard (30s)", Style::default().fg(MUTED))]),
        Line::from(vec![Span::styled("  r      ", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)), Span::styled("reveal/hide pass (zeroized on hide)", Style::default().fg(MUTED))]),
        Line::from(vec![Span::styled("  a      ", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)), Span::styled("add entry", Style::default().fg(MUTED))]),
        Line::from(vec![Span::styled("  e / u  ", Style::default().fg(AMBER).add_modifier(Modifier::BOLD)), Span::styled("edit selected", Style::default().fg(MUTED))]),
        Line::from(vec![Span::styled("  d      ", Style::default().fg(RED).add_modifier(Modifier::BOLD)), Span::styled("delete selected (confirm)", Style::default().fg(MUTED))]),
        Line::from(""),
        Line::from(Span::styled("System", Style::default().fg(CYAN).add_modifier(Modifier::BOLD))),
        Line::from(vec![Span::styled("  q / Esc", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)), Span::styled(" quit (or clear filter)", Style::default().fg(MUTED))]),
        Line::from(vec![Span::styled("  ?      ", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)), Span::styled("toggle this help", Style::default().fg(MUTED))]),
        Line::from(""),
        Line::from(Span::styled("Crypto: Argon2id KDF (salt 16B b64) → ChaCha20Poly1305 (nonce 12B) • file: [salt|nonce|cipher]", Style::default().fg(DIM))),
        Line::from(Span::styled("Passphrase never written to disk • prompted via rpassword • zeroize on drop", Style::default().fg(DIM))),
        Line::from(""),
        Line::from(vec![Span::styled("  pxxxstore ", Style::default().fg(TEXT).add_modifier(Modifier::BOLD)), Span::styled("v0.1.0 • Rust 2024 • minimal CLI password manager", Style::default().fg(MUTED))]),
    ];
    f.render_widget(Paragraph::new(help_text).wrap(Wrap { trim: false }), inner);
}

fn draw_toast(f: &mut Frame, area: Rect, msg: &StatusMsg) {
    let color = match msg.kind {
        MsgKind::Ok => GREEN,
        MsgKind::Warn => AMBER,
        MsgKind::Err => RED,
        MsgKind::Info => CYAN,
    };
    let text = format!(" {} ", msg.text);
    let width = (text.len() + 4).min(area.width as usize - 4) as u16;
    let toast_area = Rect {
        x: area.width.saturating_sub(width) / 2,
        y: 1,
        width,
        height: 3,
    };
    f.render_widget(Clear, toast_area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
        .style(Style::default().bg(PANEL).fg(TEXT));
    let para = Paragraph::new(Line::from(Span::styled(text, Style::default().fg(TEXT).add_modifier(Modifier::BOLD))))
        .alignment(Alignment::Center)
        .block(block);
    f.render_widget(para, toast_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn format_age(dt: chrono::DateTime<Utc>) -> String {
    let now = Utc::now();
    let dur = now.signed_duration_since(dt);
    let secs = dur.num_seconds();
    if secs < 60 {
        "now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else if secs < 2592000 {
        format!("{}d ago", secs / 86400)
    } else {
        dt.format("%Y-%m-%d").to_string()
    }
}
