use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use dirs::config_dir;
use ratatui::{
    prelude::*,
    widgets::{Block as UiBlock, Borders, Clear, Paragraph, Wrap},
};
use std::{fs, io::stdout, path::PathBuf, time::Duration};

use crate::reader_view::ReaderView;
use crate::views::TocView;
use reader_core::{layout::Size, types::Block as ReaderBlock};

pub enum Mode {
    Reader,
    Toc,
}

pub struct App {
    pub blocks: Vec<ReaderBlock>,
    pub initial_page: Option<usize>,
    pub mode: Mode,
    pub toc: Option<TocView>,
    pub chapter_titles: Vec<String>,
    pub book_title: Option<String>,
    pub author: Option<String>,
    pub theme: crate::reader_view::Theme,
    pub show_help: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            initial_page: None,
            mode: Mode::Reader,
            toc: None,
            chapter_titles: Vec::new(),
            book_title: None,
            author: None,
            theme: crate::reader_view::Theme::default(),
            show_help: false,
        }
    }
    pub fn new_with_blocks(blocks: Vec<ReaderBlock>) -> Self {
        Self {
            blocks,
            initial_page: None,
            mode: Mode::Reader,
            toc: None,
            chapter_titles: Vec::new(),
            book_title: None,
            author: None,
            theme: crate::reader_view::Theme::default(),
            show_help: false,
        }
    }
    pub fn new_with_blocks_at(
        blocks: Vec<ReaderBlock>,
        initial_page: usize,
        chapter_titles: Vec<String>,
    ) -> Self {
        Self {
            blocks,
            initial_page: Some(initial_page),
            mode: Mode::Reader,
            toc: None,
            chapter_titles,
            book_title: None,
            author: None,
            theme: crate::reader_view::Theme::default(),
            show_help: false,
        }
    }

    pub fn run(mut self) -> std::io::Result<usize> {
        let mut stdout = stdout();
        let raw_ok = enable_raw_mode().is_ok();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let mut view = ReaderView::new();
        view.justify = load_justify_setting();
        view.book_title = self.book_title.clone();
        view.author = self.author.clone();
        view.theme = self.theme.clone();
        let mut width: u16 = 60;
        let mut height: u16 = 20;
        // Use inner size for initial paginate to compute chapter_starts correctly
        let term_size = terminal.size()?;
        let inner = ReaderView::inner_size(term_size, width);
        let p = reader_core::layout::paginate_with_justify(&self.blocks, inner, view.justify);
        view.pages = p.pages;
        view.chapter_starts = p.chapter_starts;
        if let Some(idx) = self.initial_page {
            view.current = idx.min(view.pages.len().saturating_sub(1));
        }
        let mut last_size: (u16, u16) = (inner.width, inner.height);
        // ensure initial last_size is used by next draw comparison

        if !raw_ok {
            // Non-interactive fallback: draw once and exit cleanly
            let _ = terminal.draw(|f| {
                let size = f.size();
                height = size.height.saturating_sub(2);
                view.reflow(&self.blocks, Size { width, height });
                view.render(f, size, width);
            });
            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
            return Ok(view.current);
        }

        let mut exit = false;
        while !exit {
            terminal.draw(|f| {
                let size = f.size();
                height = size.height.saturating_sub(2);
                // Respect configured column width; do not override with terminal width
                if (width, height) != last_size {
                    let inner = ReaderView::inner_size(size, width);
                    view.reflow(&self.blocks, inner);
                    // Clamp current page if needed
                    view.current = view.current.min(view.pages.len().saturating_sub(1));
                    last_size = (inner.width, inner.height);
                }
                match self.mode {
                    Mode::Reader => {
                        view.render(f, size, width);
                    }
                    Mode::Toc => {
                        if let Some(t) = &self.toc {
                            t.render(f, size, width);
                        } else {
                            view.render(f, size, width);
                        }
                    }
                }
                if self.show_help {
                    let popup_area = centered_rect(70, 70, size);
                    let help_lines = vec![
                        "q / Ctrl-C: quit",
                        "j / k or arrows: scroll lines",
                        "PageUp / PageDown: jump half-page",
                        "h / l or arrows: adjust column width",
                        "t: toggle table of contents; Enter to jump; Esc to close TOC",
                        "J: toggle justification (persists)",
                        "?: toggle this help",
                    ];
                    let help = Paragraph::new(help_lines.join("\n"))
                        .block(
                            UiBlock::default()
                                .title("Help (Esc or ? to close)")
                                .borders(Borders::ALL),
                        )
                        .wrap(Wrap { trim: false });
                    f.render_widget(Clear, popup_area);
                    f.render_widget(help, popup_area);
                }
            })?;

            match event::poll(Duration::from_millis(100)) {
                Ok(true) => {
                    match event::read() {
                        Ok(Event::Key(key)) => {
                            if self.show_help {
                                match key.code {
                                    KeyCode::Esc | KeyCode::Char('?') => {
                                        self.show_help = false;
                                    }
                                    _ => {}
                                }
                                continue;
                            }
                            match key.code {
                                KeyCode::Char('q') => exit = true,
                                KeyCode::Esc => {
                                    if let Mode::Toc = self.mode {
                                        self.mode = Mode::Reader;
                                    }
                                }
                                KeyCode::Enter => {
                                    if let Mode::Toc = self.mode {
                                        if let Some(t) = &self.toc {
                                            // Jump: set view.current to chapter start page index
                                            if let Some(pidx) =
                                                view.chapter_starts.get(t.selected).cloned()
                                            {
                                                view.current =
                                                    pidx.min(view.pages.len().saturating_sub(1));
                                            }
                                            self.mode = Mode::Reader;
                                        }
                                    }
                                }
                                KeyCode::Char('t') => {
                                    // Build TOC items from chapter_starts; show indices for now.
                                    // If empty, still open with a single "Start" entry
                                    let items: Vec<String> = if view.chapter_starts.is_empty() {
                                        vec!["Start".to_string()]
                                    } else {
                                        view.chapter_starts
                                            .iter()
                                            .enumerate()
                                            .map(|(i, pidx)| {
                                                let title = self
                                                    .chapter_titles
                                                    .get(i)
                                                    .cloned()
                                                    .unwrap_or_else(|| {
                                                        format!("Chapter {}", i + 1)
                                                    });
                                                format!("{}  (page {})", title, pidx + 1)
                                            })
                                            .collect()
                                    };
                                    let mut toc = TocView::new(items);
                                    // Default selection = current chapter (last start <= current page)
                                    if let Some(idx) =
                                        view.chapter_starts.iter().rposition(|&p| p <= view.current)
                                    {
                                        toc.selected = idx;
                                    } else if !view.chapter_starts.is_empty() {
                                        // If current is before first start, select the first
                                        toc.selected = 0;
                                    }
                                    self.toc = Some(toc);
                                    self.mode = Mode::Toc;
                                }

                                KeyCode::Char('c')
                                    if key
                                        .modifiers
                                        .contains(crossterm::event::KeyModifiers::CONTROL) =>
                                {
                                    exit = true
                                }
                                KeyCode::Char('j') | KeyCode::Down => match self.mode {
                                    Mode::Reader => {
                                        view.down(1);
                                        view.last_key = Some("j/down".into());
                                    }
                                    Mode::Toc => {
                                        if let Some(t) = &mut self.toc {
                                            t.down();
                                        }
                                    }
                                },
                                KeyCode::Char('k') | KeyCode::Up => match self.mode {
                                    Mode::Reader => {
                                        view.up(1);
                                        view.last_key = Some("k/up".into());
                                    }
                                    Mode::Toc => {
                                        if let Some(t) = &mut self.toc {
                                            t.up();
                                        }
                                    }
                                },

                                KeyCode::Char('h') | KeyCode::Left => {
                                    if let Mode::Reader = self.mode {
                                        width = width.saturating_sub(2);
                                        let inner = ReaderView::inner_size(terminal.size()?, width);
                                        view.reflow(&self.blocks, inner);
                                        view.last_key = Some("h/left".into());
                                    }
                                }
                                KeyCode::Char('l') | KeyCode::Right => {
                                    if let Mode::Reader = self.mode {
                                        width = width.saturating_add(2);
                                        let inner = ReaderView::inner_size(terminal.size()?, width);
                                        view.reflow(&self.blocks, inner);
                                        view.last_key = Some("l/right".into());
                                    }
                                }
                                KeyCode::PageDown => {
                                    if let Mode::Reader = self.mode {
                                        view.down((height / 2) as usize);
                                        view.last_key = Some("PgDn".into());
                                    }
                                }
                                KeyCode::PageUp => {
                                    if let Mode::Reader = self.mode {
                                        view.up((height / 2) as usize);
                                        view.last_key = Some("PgUp".into());
                                    }
                                }
                                KeyCode::Char('J') => {
                                    if let Mode::Reader = self.mode {
                                        view.justify = !view.justify;
                                        save_justify_setting(view.justify);
                                        view.last_key = Some("J toggle".into());
                                        let inner = ReaderView::inner_size(terminal.size()?, width);
                                        view.reflow(&self.blocks, inner);
                                    }
                                }
                                KeyCode::Char('?') => {
                                    self.show_help = !self.show_help;
                                }

                                _ => {}
                            }
                        }
                        Ok(_) => {}
                        Err(_) => {
                            exit = true;
                        }
                    }
                }
                Ok(false) => {}
                Err(_) => {
                    exit = true;
                }
            }
        }

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        Ok(view.current)
    }
}

fn settings_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("lightning-librarian").join("settings.toml"))
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1]);
    horizontal[1]
}

fn load_justify_setting() -> bool {
    if let Some(path) = settings_path() {
        if let Ok(contents) = fs::read_to_string(path) {
            for line in contents.lines() {
                if let Some(val) = line.strip_prefix("justify=") {
                    return val.trim().eq_ignore_ascii_case("true");
                }
            }
        }
    }
    false
}

fn save_justify_setting(justify: bool) {
    if let Some(path) = settings_path() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, format!("justify={justify}\n"));
    }
}
