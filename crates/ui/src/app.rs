use std::{
    fs,
    io::stdout,
    path::PathBuf,
    sync::mpsc::{Receiver, Sender},
    time::Duration,
};

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
use reader_core::types::{Block as ReaderBlock, Document};

use crate::{
    layout::centered_rect, reader_view::ReaderView, search_view::SearchView, views::TocView,
};
pub enum Mode {
    Reader,
    Toc,
}

pub struct App {
    pub blocks: Vec<ReaderBlock>,
    pub initial_page: Option<usize>,
    pub mode: Mode,
    pub toc: Option<TocView>,
    pub search: Option<SearchView>,
    pub chapter_titles: Vec<String>,
    pub book_title: Option<String>,
    pub author: Option<String>,
    pub theme: crate::reader_view::Theme,
    pub last_search: Option<String>,
    pub last_search_hit: Option<usize>,
    pub show_help: bool,
    pub incoming_pages: Option<Receiver<IncomingPage>>,
    pub total_pages: Option<usize>,
    pub prefetch_tx: Option<Sender<PrefetchRequest>>,
    pub prefetch_window: usize,
    pub last_prefetch_at: Option<usize>,
}

pub struct IncomingPage {
    pub page_index: usize,
    pub blocks: Vec<ReaderBlock>,
}

pub struct PrefetchRequest {
    pub start: usize,
    pub window: usize,
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
            search: None,
            chapter_titles: Vec::new(),
            book_title: None,
            author: None,
            theme: crate::reader_view::Theme::default(),
            last_search: None,
            last_search_hit: None,
            show_help: false,
            incoming_pages: None,
            total_pages: None,
            prefetch_tx: None,
            prefetch_window: 2,
            last_prefetch_at: None,
        }
    }
    pub fn new_with_blocks(blocks: Vec<ReaderBlock>) -> Self {
        Self {
            blocks,
            initial_page: None,
            mode: Mode::Reader,
            toc: None,
            search: None,
            chapter_titles: Vec::new(),
            book_title: None,
            author: None,
            theme: crate::reader_view::Theme::default(),
            last_search: None,
            last_search_hit: None,
            show_help: false,
            incoming_pages: None,
            total_pages: None,
            prefetch_tx: None,
            prefetch_window: 2,
            last_prefetch_at: None,
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
            search: None,
            chapter_titles,
            book_title: None,
            author: None,
            theme: crate::reader_view::Theme::default(),
            last_search: None,
            last_search_hit: None,
            show_help: false,
            incoming_pages: None,
            total_pages: None,
            prefetch_tx: None,
            prefetch_window: 2,
            last_prefetch_at: None,
        }
    }
    pub fn new_with_document(document: Document, initial_page: usize) -> Self {
        let mut app =
            Self::new_with_blocks_at(document.blocks, initial_page, document.chapter_titles);
        app.book_title = document.info.title;
        app.author = document.info.author;
        app
    }

    pub fn new_with_document_streaming(
        document: Document,
        initial_page: usize,
        incoming_pages: Receiver<IncomingPage>,
        total_pages: usize,
        prefetch_tx: Sender<PrefetchRequest>,
        prefetch_window: usize,
    ) -> Self {
        let mut app = Self::new_with_document(document, initial_page);
        app.incoming_pages = Some(incoming_pages);
        app.total_pages = Some(total_pages);
        app.prefetch_tx = Some(prefetch_tx);
        app.prefetch_window = prefetch_window;
        app
    }

    fn poll_incoming(&mut self, view: &mut ReaderView, inner: reader_core::layout::Size) {
        let mut added = false;
        if let Some(rx) = &self.incoming_pages {
            while let Ok(msg) = rx.try_recv() {
                if !self.blocks.is_empty() {
                    self.blocks.push(ReaderBlock::Paragraph(String::new()));
                    self.blocks.push(ReaderBlock::Paragraph("───".into()));
                    self.blocks.push(ReaderBlock::Paragraph(String::new()));
                }
                self.blocks.extend(msg.blocks);
                self.chapter_titles
                    .push(format!("Page {}", msg.page_index + 1));
                added = true;
            }
        }
        if added {
            view.reflow(&self.blocks, inner);
            view.chapter_titles = self.chapter_titles.clone();
            view.total_pages = self.total_pages;
        }
    }

    fn maybe_request_prefetch(&mut self, view: &ReaderView) {
        let Some(tx) = &self.prefetch_tx else {
            return;
        };
        let loaded_pages = self.chapter_titles.len();
        let total = self.total_pages.unwrap_or(loaded_pages);
        if loaded_pages >= total {
            return;
        }
        let current = view.current;
        if self.last_prefetch_at == Some(current) {
            return;
        }
        self.last_prefetch_at = Some(current);
        let start = current + 1;
        if start >= total {
            return;
        }
        let _ = tx.send(PrefetchRequest {
            start,
            window: self.prefetch_window,
        });
    }

    pub fn run(mut self) -> std::io::Result<usize> {
        let mut stdout = stdout();
        let raw_ok = enable_raw_mode().is_ok();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let mut view = ReaderView::new();
        let (saved_justify, saved_two_pane) = load_settings();
        view.justify = saved_justify;
        view.two_pane = saved_two_pane;
        view.book_title = self.book_title.clone();
        view.author = self.author.clone();
        view.theme = self.theme.clone();
        let mut width: u16 = 60;
        let mut height: u16 = 20;
        // Use inner size for initial paginate to compute chapter_starts correctly
        let term_size = terminal.size()?;
        let inner = ReaderView::inner_size(term_size, width, view.two_pane);
        let p = reader_core::layout::paginate_with_justify(&self.blocks, inner, view.justify);
        view.pages = p.pages;
        view.chapter_starts = p.chapter_starts;
        view.chapter_titles = self.chapter_titles.clone();
        view.total_pages = self.total_pages;
        if let Some(idx) = self.initial_page {
            view.current = idx.min(view.pages.len().saturating_sub(1));
        }
        let mut last_inner: (u16, u16) = (inner.width, inner.height);
        // ensure initial last_size is used by next draw comparison

        if !raw_ok {
            // Non-interactive fallback: draw once and exit cleanly
            let _ = terminal.draw(|f| {
                let size = f.size();
                height = size.height.saturating_sub(2);
                let inner = ReaderView::inner_size(size, width, view.two_pane);
                self.poll_incoming(&mut view, inner);
                self.maybe_request_prefetch(&view);
                view.reflow(&self.blocks, inner);
                view.render(f, size, width, self.last_search.as_deref());
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
                let inner = ReaderView::inner_size(size, width, view.two_pane);
                self.poll_incoming(&mut view, inner);
                self.maybe_request_prefetch(&view);
                if (inner.width, inner.height) != last_inner {
                    view.reflow(&self.blocks, inner);
                    // Clamp current page if needed
                    view.current = view.current.min(view.pages.len().saturating_sub(1));
                    last_inner = (inner.width, inner.height);
                }
                match self.mode {
                    Mode::Reader => {
                        view.render(f, size, width, self.last_search.as_deref());
                    }
                    Mode::Toc => {
                        if let Some(t) = &self.toc {
                            t.render(f, size, width);
                        } else {
                            view.render(f, size, width, self.last_search.as_deref());
                        }
                    }
                }
                if let Some(search) = &self.search {
                    search.render(f, size);
                }
                if self.show_help {
                    let popup_area = centered_rect(70, 70, size);
                    let help_lines = vec![
                        "q / Ctrl-C: quit",
                        "j / k or arrows: scroll lines",
                        "PageUp / PageDown: jump half-page",
                        "h / l or arrows: adjust column width",
                        "t: toggle table of contents; Enter to jump; Esc to close TOC",
                        "/: search; Enter to submit; Esc to cancel",
                        "J: toggle justification (persists)",
                        "b: toggle two-page spread (persists)",
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
                            if let Some(search) = &mut self.search {
                                match key.code {
                                    KeyCode::Esc => {
                                        self.search = None;
                                    }
                                    KeyCode::Enter => {
                                        let query = search.query.clone();
                                        let trimmed = query.trim().to_string();
                                        let start_from =
                                            if self.last_search.as_deref().map(str::trim)
                                                == Some(trimmed.as_str())
                                            {
                                                self.last_search_hit.map(|p| p + 1)
                                            } else {
                                                None
                                            };
                                        self.last_search = Some(trimmed.clone());
                                        if let Some(idx) = view.search_forward(&trimmed, start_from)
                                        {
                                            let target = if view.two_pane {
                                                idx.saturating_sub(idx % 2)
                                            } else {
                                                idx
                                            };
                                            view.current = target;
                                            self.last_search_hit = Some(idx);
                                        } else {
                                            self.last_search_hit = None;
                                        }
                                        self.search = None;
                                    }
                                    KeyCode::Backspace => {
                                        search.backspace();
                                    }
                                    KeyCode::Char(c) => {
                                        search.push_char(c);
                                    }
                                    _ => {}
                                }
                                continue;
                            }
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
                                KeyCode::Char('/') => {
                                    let search = if let Some(prev) = &self.last_search {
                                        SearchView::with_query(prev.clone())
                                    } else {
                                        SearchView::new()
                                    };
                                    self.search = Some(search);
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
                                        let inner = ReaderView::inner_size(
                                            terminal.size()?,
                                            width,
                                            view.two_pane,
                                        );
                                        view.reflow(&self.blocks, inner);
                                        last_inner = (inner.width, inner.height);
                                        view.last_key = Some("h/left".into());
                                    }
                                }
                                KeyCode::Char('l') | KeyCode::Right => {
                                    if let Mode::Reader = self.mode {
                                        width = width.saturating_add(2);
                                        let inner = ReaderView::inner_size(
                                            terminal.size()?,
                                            width,
                                            view.two_pane,
                                        );
                                        view.reflow(&self.blocks, inner);
                                        last_inner = (inner.width, inner.height);
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
                                        save_settings(view.justify, view.two_pane);
                                        view.last_key = Some("J toggle".into());
                                        let inner = ReaderView::inner_size(
                                            terminal.size()?,
                                            width,
                                            view.two_pane,
                                        );
                                        view.reflow(&self.blocks, inner);
                                        last_inner = (inner.width, inner.height);
                                    }
                                }
                                KeyCode::Char('b') => {
                                    if let Mode::Reader = self.mode {
                                        view.two_pane = !view.two_pane;
                                        // Align to left page when entering spread mode
                                        if view.two_pane {
                                            view.current =
                                                view.current.saturating_sub(view.current % 2);
                                        }
                                        save_settings(view.justify, view.two_pane);
                                        let inner = ReaderView::inner_size(
                                            terminal.size()?,
                                            width,
                                            view.two_pane,
                                        );
                                        view.reflow(&self.blocks, inner);
                                        last_inner = (inner.width, inner.height);
                                        view.last_key = Some(
                                            if view.two_pane {
                                                "b spread on"
                                            } else {
                                                "b spread off"
                                            }
                                            .into(),
                                        );
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

fn load_settings() -> (bool, bool) {
    let mut justify = false;
    let mut two_pane = false;
    if let Some(path) = settings_path() {
        if let Ok(contents) = fs::read_to_string(path) {
            for line in contents.lines() {
                if let Some(val) = line.strip_prefix("justify=") {
                    justify = val.trim().eq_ignore_ascii_case("true");
                } else if let Some(val) = line.strip_prefix("two_pane=") {
                    two_pane = val.trim().eq_ignore_ascii_case("true");
                }
            }
        }
    }
    (justify, two_pane)
}

fn save_settings(justify: bool, two_pane: bool) {
    if let Some(path) = settings_path() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, format!("justify={justify}\ntwo_pane={two_pane}\n"));
    }
}
