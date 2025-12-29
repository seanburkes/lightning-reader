use chrono::Utc;
use std::{
    fs,
    io::stdout,
    path::PathBuf,
    sync::mpsc::{Receiver, Sender},
    time::Duration,
};

use arboard::Clipboard;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton, MouseEvent,
        MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block as UiBlock, Borders, Clear, Paragraph, Wrap},
};
use reader_core::types::{Block as ReaderBlock, Document};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    layout::centered_rect,
    reader_view::{ReaderView, SelectionPoint, SelectionRange},
    search_view::SearchView,
    spritz_view::SpritzView,
    views::TocView,
};

#[derive(Clone, Copy, Debug)]
pub struct SpritzSettings {
    pub wpm: u16,
    pub pause_on_punct: bool,
    pub punct_pause_ms: u16,
}

impl Default for SpritzSettings {
    fn default() -> Self {
        Self {
            wpm: 250,
            pause_on_punct: true,
            punct_pause_ms: 100,
        }
    }
}

pub enum Mode {
    Reader,
    Toc,
    Spritz,
}

pub struct App {
    pub blocks: Vec<ReaderBlock>,
    pub initial_page: Option<usize>,
    pub mode: Mode,
    pub toc: Option<TocView>,
    pub search: Option<SearchView>,
    pub spritz: Option<SpritzView>,
    pub chapter_titles: Vec<String>,
    pub outlines: Vec<reader_core::pdf::OutlineEntry>,
    pub book_title: Option<String>,
    pub author: Option<String>,
    pub book_id: Option<String>,
    pub theme: crate::reader_view::Theme,
    pub last_search: Option<String>,
    pub last_search_hit: Option<usize>,
    pub show_help: bool,
    pub incoming_pages: Option<Receiver<IncomingPage>>,
    pub total_pages: Option<usize>,
    pub prefetch_tx: Option<Sender<PrefetchRequest>>,
    pub prefetch_window: usize,
    pub last_prefetch_at: Option<usize>,
    pub clipboard: Option<Clipboard>,
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
            spritz: None,
            chapter_titles: Vec::new(),
            outlines: Vec::new(),
            book_title: None,
            author: None,
            book_id: None,
            theme: crate::reader_view::Theme::default(),
            last_search: None,
            last_search_hit: None,
            show_help: false,
            incoming_pages: None,
            total_pages: None,
            prefetch_tx: None,
            prefetch_window: 2,
            last_prefetch_at: None,
            clipboard: None,
        }
    }
    pub fn new_with_blocks(blocks: Vec<ReaderBlock>) -> Self {
        Self {
            blocks,
            initial_page: None,
            mode: Mode::Reader,
            toc: None,
            search: None,
            spritz: None,
            chapter_titles: Vec::new(),
            outlines: Vec::new(),
            book_title: None,
            author: None,
            book_id: None,
            theme: crate::reader_view::Theme::default(),
            last_search: None,
            last_search_hit: None,
            show_help: false,
            incoming_pages: None,
            total_pages: None,
            prefetch_tx: None,
            prefetch_window: 2,
            last_prefetch_at: None,
            clipboard: None,
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
            spritz: None,
            chapter_titles,
            outlines: Vec::new(),
            book_title: None,
            author: None,
            book_id: None,
            theme: crate::reader_view::Theme::default(),
            last_search: None,
            last_search_hit: None,
            show_help: false,
            incoming_pages: None,
            total_pages: None,
            prefetch_tx: None,
            prefetch_window: 2,
            last_prefetch_at: None,
            clipboard: None,
        }
    }
    pub fn new_with_document(document: Document, initial_page: usize) -> Self {
        let mut app =
            Self::new_with_blocks_at(document.blocks, initial_page, document.chapter_titles);
        app.book_title = document.info.title;
        app.author = document.info.author;
        app.book_id = Some(document.info.id);
        app.outlines = document.outlines;
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
            view.selection = None;
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

    fn build_toc_items(&self, view: &ReaderView) -> (Vec<String>, Vec<usize>) {
        if !self.outlines.is_empty() {
            let mut items = Vec::new();
            let mut pages = Vec::new();
            for entry in &self.outlines {
                items.push(entry.title.clone());
                pages.push(entry.page_index);
            }
            return (items, pages);
        }
        if view.chapter_starts.is_empty() {
            return (vec!["Start".to_string()], vec![0]);
        }
        let mut items: Vec<String> = Vec::new();
        let mut pages: Vec<usize> = Vec::new();
        for (i, pidx) in view.chapter_starts.iter().enumerate() {
            let title = view.chapter_title(i);
            let chapter_label = format!("Chapter {}", i + 1);
            let entry = if title.is_empty() {
                chapter_label
            } else if title.to_ascii_lowercase().starts_with("chapter") {
                title
            } else {
                format!("{}: {}", chapter_label, title)
            };
            items.push(format!("{}  (page {})", entry, pidx + 1));
            pages.push(*pidx);
        }
        (items, pages)
    }

    fn mouse_selection_point(
        view: &ReaderView,
        area: Rect,
        column_width: u16,
        mouse: MouseEvent,
    ) -> Option<SelectionPoint> {
        let areas = view.content_areas(area, column_width);
        let is_spread = areas.right.is_some();
        let (pane, page_idx) = if rect_contains(areas.left, mouse.column, mouse.row) {
            let page = if is_spread {
                view.current.saturating_sub(view.current % 2)
            } else {
                view.current
            };
            (areas.left, page)
        } else if let Some(right) = areas.right {
            if rect_contains(right, mouse.column, mouse.row) {
                let base = view.current.saturating_sub(view.current % 2);
                (right, base + 1)
            } else {
                return None;
            }
        } else {
            return None;
        };

        let page = view.pages.get(page_idx)?;
        let line_idx = mouse.row.saturating_sub(pane.y) as usize;
        let line = page.lines.get(line_idx)?;
        let line_text = line_text(line);
        let line_len = line_text.graphemes(true).count();
        let col_idx = mouse.column.saturating_sub(pane.x) as usize;
        let col_idx = col_idx.min(line_len);
        Some(SelectionPoint {
            page: page_idx,
            line: line_idx,
            col: col_idx,
        })
    }

    fn copy_selection(&mut self, view: &ReaderView, selection: SelectionRange) {
        let text = selection_text(view, selection);
        if text.trim().is_empty() {
            return;
        }
        if self.clipboard.is_none() {
            self.clipboard = Clipboard::new().ok();
        }
        if let Some(clipboard) = &mut self.clipboard {
            let _ = clipboard.set_text(text);
        }
    }

    pub fn run(mut self) -> std::io::Result<usize> {
        let mut stdout = stdout();
        let raw_ok = enable_raw_mode().is_ok();
        if raw_ok {
            execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        } else {
            execute!(stdout, EnterAlternateScreen)?;
        }
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let mut view = ReaderView::new();
        let (saved_justify, saved_two_pane, _spritz_settings) = load_settings();
        view.justify = saved_justify;
        view.two_pane = saved_two_pane;
        view.book_title = self.book_title.clone();
        view.author = self.author.clone();
        view.theme = self.theme.clone();
        let mut selection_anchor: Option<SelectionPoint> = None;
        let mut selection_active = false;
        let mut last_frame = Rect::default();
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
        view.toc_overrides = self.outlines.clone();
        if let Some(idx) = self.initial_page {
            view.current = idx.min(view.pages.len().saturating_sub(1));
        }
        let mut last_inner: (u16, u16) = (inner.width, inner.height);
        // ensure initial last_size is used by next draw comparison

        if !raw_ok {
            // Non-interactive fallback: draw once and exit cleanly
            let _ = terminal.draw(|f| {
                let size = f.size();
                last_frame = size;
                height = size.height.saturating_sub(2);
                let inner = ReaderView::inner_size(size, width, view.two_pane);
                self.poll_incoming(&mut view, inner);
                self.maybe_request_prefetch(&view);
                view.reflow(&self.blocks, inner);
                view.render(f, size, width, self.last_search.as_deref());
            });
            if raw_ok {
                let _ = disable_raw_mode();
                execute!(
                    terminal.backend_mut(),
                    LeaveAlternateScreen,
                    DisableMouseCapture
                )?;
            } else {
                execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
            }
            return Ok(view.current);
        }

        let mut exit = false;
        while !exit {
            terminal.draw(|f| {
                let size = f.size();
                last_frame = size;
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
                    view.selection = None;
                    selection_anchor = None;
                    selection_active = false;
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
                    Mode::Spritz => {
                        if let Some(spritz) = self.spritz.as_mut() {
                            spritz.update();
                            spritz.render(f, size, width);
                        }
                    }
                }
                if let Some(search) = &self.search {
                    search.render(f, size);
                }
                if self.show_help {
                    let popup_area = centered_rect(70, 70, size);
                    let help_lines = match self.mode {
                        Mode::Spritz => vec![
                            "q / Ctrl-C / s / Esc: exit spritz mode",
                            "Space: toggle play/pause",
                            "j/k or arrows: navigate word-by-word",
                            "h / l or arrows: adjust column width",
                            "Ctrl+j/Ctrl+k: jump 10 words",
                            "+/- or =/_: adjust WPM by 10",
                            "[ / ]: adjust WPM by 50",
                            "r: rewind to chapter start",
                            "f: fast forward to chapter end",
                            "Enter: resume playing if paused",
                            "?: toggle this help",
                        ],
                        _ => vec![
                            "q / Ctrl-C: quit",
                            "s: toggle spritz speed reading mode",
                            "j / k or arrows: scroll lines",
                            "PageUp / PageDown: jump half-page",
                            "h / l or arrows: adjust column width",
                            "t: toggle table of contents; Enter to jump; Esc to close TOC",
                            "/: search; Enter to submit; Esc to cancel",
                            "J: toggle justification (persists)",
                            "b: toggle two-page spread (persists)",
                            "?: toggle this help",
                        ],
                    };
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
                        Ok(Event::Mouse(mouse)) => {
                            if let Mode::Reader = self.mode {
                                if self.search.is_some() || self.show_help {
                                    continue;
                                }
                                handle_mouse_selection(
                                    &mut self,
                                    &mut view,
                                    last_frame,
                                    width,
                                    mouse,
                                    &mut selection_anchor,
                                    &mut selection_active,
                                );
                            }
                        }
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
                                    } else if let Mode::Spritz = self.mode {
                                        if let Some(spritz) = &self.spritz {
                                            if let Some(book_id) = &self.book_id {
                                                let session = reader_core::SpritzSession {
                                                    book_id: book_id.clone(),
                                                    word_index: spritz.current_index,
                                                    wpm: spritz.wpm,
                                                    saved_at: Utc::now().to_rfc3339(),
                                                };
                                                let _ = reader_core::save_spritz_session(&session);
                                            }
                                        }
                                        self.spritz = None;
                                        self.mode = Mode::Reader;
                                    }
                                }
                                KeyCode::Enter => {
                                    if let Mode::Toc = self.mode {
                                        if let Some(t) = &self.toc {
                                            if let Some(target) = t.current_page() {
                                                view.current =
                                                    target.min(view.pages.len().saturating_sub(1));
                                            }
                                            self.mode = Mode::Reader;
                                            self.toc = None;
                                        }
                                    } else if let Mode::Spritz = self.mode {
                                        if let Some(spritz) = &mut self.spritz {
                                            if !spritz.is_playing {
                                                spritz.play();
                                            }
                                        }
                                    } else if let Some(search) = &self.search {
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
                                    let (items, page_map) = self.build_toc_items(&view);
                                    let mut toc = TocView::new(items, page_map);
                                    if let Some(idx) =
                                        toc.page_map.iter().rposition(|p| *p <= view.current)
                                    {
                                        toc.selected = idx;
                                    }
                                    self.toc = Some(toc);
                                    self.mode = Mode::Toc;
                                }
                                KeyCode::Char('s') => match self.mode {
                                    Mode::Reader => {
                                        let words =
                                            reader_core::layout::extract_words(&self.blocks);
                                        let settings = SpritzSettings::default();
                                        let mut spritz = SpritzView::new(
                                            words,
                                            settings,
                                            self.chapter_titles.clone(),
                                            self.theme.clone(),
                                        );

                                        if let Some(book_id) = &self.book_id {
                                            if let Some(session) =
                                                reader_core::load_spritz_session(book_id)
                                            {
                                                spritz.current_index = session.word_index;
                                                spritz.wpm = session.wpm;
                                            }
                                        }

                                        self.spritz = Some(spritz);
                                        self.mode = Mode::Spritz;
                                    }
                                    Mode::Spritz => {
                                        if let Some(spritz) = &self.spritz {
                                            if let Some(book_id) = &self.book_id {
                                                let session = reader_core::SpritzSession {
                                                    book_id: book_id.clone(),
                                                    word_index: spritz.current_index,
                                                    wpm: spritz.wpm,
                                                    saved_at: Utc::now().to_rfc3339(),
                                                };
                                                let _ = reader_core::save_spritz_session(&session);
                                            }
                                        }
                                        self.spritz = None;
                                        self.mode = Mode::Reader;
                                    }
                                    Mode::Toc => {}
                                },

                                KeyCode::Char('c')
                                    if key
                                        .modifiers
                                        .contains(crossterm::event::KeyModifiers::CONTROL) =>
                                {
                                    exit = true
                                }
                                KeyCode::Char('j') | KeyCode::Down => {
                                    if key
                                        .modifiers
                                        .contains(crossterm::event::KeyModifiers::CONTROL)
                                    {
                                        if let Mode::Spritz = self.mode {
                                            if let Some(spritz) = &mut self.spritz {
                                                spritz.fast_forward(10);
                                            }
                                        }
                                    } else {
                                        match self.mode {
                                            Mode::Reader => {
                                                view.down(1);
                                                view.last_key = Some("j/down".into());
                                            }
                                            Mode::Toc => {
                                                if let Some(t) = &mut self.toc {
                                                    t.down();
                                                }
                                            }
                                            Mode::Spritz => {
                                                if let Some(spritz) = &mut self.spritz {
                                                    spritz.fast_forward(1);
                                                }
                                            }
                                        }
                                    }
                                }
                                KeyCode::Char('k') | KeyCode::Up => {
                                    if key
                                        .modifiers
                                        .contains(crossterm::event::KeyModifiers::CONTROL)
                                    {
                                        if let Mode::Spritz = self.mode {
                                            if let Some(spritz) = &mut self.spritz {
                                                spritz.rewind(10);
                                            }
                                        }
                                    } else {
                                        match self.mode {
                                            Mode::Reader => {
                                                view.up(1);
                                                view.last_key = Some("k/up".into());
                                            }
                                            Mode::Toc => {
                                                if let Some(t) = &mut self.toc {
                                                    t.up();
                                                }
                                            }
                                            Mode::Spritz => {
                                                if let Some(spritz) = &mut self.spritz {
                                                    spritz.rewind(1);
                                                }
                                            }
                                        }
                                    }
                                }

                                KeyCode::Char('h') | KeyCode::Left => match self.mode {
                                    Mode::Reader => {
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
                                    Mode::Spritz => {
                                        width = width.saturating_sub(2);
                                        last_inner = (width, last_inner.1);
                                    }
                                    Mode::Toc => {}
                                },
                                KeyCode::Char('l') | KeyCode::Right => match self.mode {
                                    Mode::Reader => {
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
                                    Mode::Spritz => {
                                        width = width.saturating_add(2);
                                        last_inner = (width, last_inner.1);
                                    }
                                    Mode::Toc => {}
                                },
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
                                        save_settings(
                                            view.justify,
                                            view.two_pane,
                                            &SpritzSettings::default(),
                                        );
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
                                        save_settings(
                                            view.justify,
                                            view.two_pane,
                                            &SpritzSettings::default(),
                                        );
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
                                KeyCode::Char(' ') => {
                                    if let Mode::Spritz = self.mode {
                                        if let Some(spritz) = &mut self.spritz {
                                            spritz.toggle_play();
                                        }
                                    }
                                }

                                KeyCode::Char('r') => {
                                    if let Mode::Spritz = self.mode {
                                        if let Some(spritz) = &mut self.spritz {
                                            spritz.jump_to_chapter_start();
                                        }
                                    }
                                }
                                KeyCode::Char('f') => {
                                    if let Mode::Spritz = self.mode {
                                        if let Some(spritz) = &mut self.spritz {
                                            spritz.jump_to_chapter_end();
                                        }
                                    }
                                }
                                KeyCode::Char('+') | KeyCode::Char('=') => {
                                    if let Mode::Spritz = self.mode {
                                        if let Some(spritz) = &mut self.spritz {
                                            spritz.adjust_wpm(10);
                                        }
                                    }
                                }
                                KeyCode::Char('-') | KeyCode::Char('_') => {
                                    if let Mode::Spritz = self.mode {
                                        if let Some(spritz) = &mut self.spritz {
                                            spritz.adjust_wpm(-10);
                                        }
                                    }
                                }
                                KeyCode::Char(']') => {
                                    if let Mode::Spritz = self.mode {
                                        if let Some(spritz) = &mut self.spritz {
                                            spritz.adjust_wpm(50);
                                        }
                                    }
                                }
                                KeyCode::Char('[') => {
                                    if let Mode::Spritz = self.mode {
                                        if let Some(spritz) = &mut self.spritz {
                                            spritz.adjust_wpm(-50);
                                        }
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

        if raw_ok {
            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )?;
        } else {
            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        }
        Ok(view.current)
    }
}

fn handle_mouse_selection(
    app: &mut App,
    view: &mut ReaderView,
    area: Rect,
    column_width: u16,
    mouse: MouseEvent,
    selection_anchor: &mut Option<SelectionPoint>,
    selection_active: &mut bool,
) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(point) = App::mouse_selection_point(view, area, column_width, mouse) {
                *selection_anchor = Some(point);
                *selection_active = true;
                view.selection = Some(SelectionRange {
                    start: point,
                    end: point,
                });
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if *selection_active {
                if let Some(point) = App::mouse_selection_point(view, area, column_width, mouse) {
                    if let Some(anchor) = *selection_anchor {
                        view.selection = Some(SelectionRange {
                            start: anchor,
                            end: point,
                        });
                    }
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if *selection_active {
                if let Some(selection) = view.selection {
                    app.copy_selection(view, selection);
                }
                view.selection = None;
                *selection_anchor = None;
                *selection_active = false;
            }
        }
        _ => {}
    }
}

fn rect_contains(rect: Rect, col: u16, row: u16) -> bool {
    let x_end = rect.x.saturating_add(rect.width);
    let y_end = rect.y.saturating_add(rect.height);
    col >= rect.x && col < x_end && row >= rect.y && row < y_end
}

fn line_text(line: &reader_core::layout::StyledLine) -> String {
    line.segments
        .iter()
        .map(|seg| seg.text.as_str())
        .collect::<String>()
}

fn slice_graphemes(text: &str, start: usize, end: usize) -> String {
    if start >= end {
        return String::new();
    }
    let graphemes: Vec<&str> = text.graphemes(true).collect();
    if start >= graphemes.len() {
        return String::new();
    }
    let end = end.min(graphemes.len());
    graphemes[start..end].concat()
}

fn selection_text(view: &ReaderView, selection: SelectionRange) -> String {
    let (start, end) = selection.normalized();
    let mut out: Vec<String> = Vec::new();
    for page_idx in start.page..=end.page {
        let Some(page) = view.pages.get(page_idx) else {
            continue;
        };
        if page.lines.is_empty() {
            continue;
        }
        let last_line = page.lines.len().saturating_sub(1);
        let start_line = if page_idx == start.page {
            start.line.min(last_line)
        } else {
            0
        };
        let end_line = if page_idx == end.page {
            end.line.min(last_line)
        } else {
            last_line
        };
        if start_line > end_line {
            continue;
        }
        for line_idx in start_line..=end_line {
            let text = line_text(&page.lines[line_idx]);
            let line_len = text.graphemes(true).count();
            let selected = if page_idx == start.page && line_idx == start_line {
                let start_col = start.col.min(line_len);
                let end_col = if page_idx == end.page && line_idx == end_line {
                    end.col.min(line_len)
                } else {
                    line_len
                };
                slice_graphemes(&text, start_col, end_col)
            } else if page_idx == end.page && line_idx == end_line {
                let end_col = end.col.min(line_len);
                slice_graphemes(&text, 0, end_col)
            } else {
                text
            };
            out.push(selected);
        }
    }
    out.join("\n")
}

fn settings_path() -> Option<PathBuf> {
    reader_core::config::config_root().map(|dir| dir.join("settings.toml"))
}

fn legacy_settings_paths() -> Vec<PathBuf> {
    reader_core::config::legacy_config_roots()
        .into_iter()
        .map(|dir| dir.join("settings.toml"))
        .collect()
}

fn load_settings() -> (bool, bool, SpritzSettings) {
    let mut justify = false;
    let mut two_pane = false;
    let mut spritz = SpritzSettings::default();
    let mut candidates = Vec::new();
    if let Some(primary) = settings_path() {
        candidates.push(primary);
    }
    candidates.extend(legacy_settings_paths());
    for path in candidates {
        if let Ok(contents) = fs::read_to_string(path) {
            for line in contents.lines() {
                if let Some(val) = line.strip_prefix("justify=") {
                    justify = val.trim().eq_ignore_ascii_case("true");
                } else if let Some(val) = line.strip_prefix("two_pane=") {
                    two_pane = val.trim().eq_ignore_ascii_case("true");
                } else if let Some(val) = line.strip_prefix("spritz_wpm=") {
                    spritz.wpm = val.trim().parse().unwrap_or(spritz.wpm).clamp(100, 1000);
                } else if let Some(val) = line.strip_prefix("spritz_pause_on_punct=") {
                    spritz.pause_on_punct = val.trim().eq_ignore_ascii_case("true");
                } else if let Some(val) = line.strip_prefix("spritz_punct_pause_ms=") {
                    spritz.punct_pause_ms = val.trim().parse().unwrap_or(spritz.punct_pause_ms);
                }
            }
            break;
        }
    }
    (justify, two_pane, spritz)
}

fn save_settings(justify: bool, two_pane: bool, spritz: &SpritzSettings) {
    let target = settings_path().or_else(|| legacy_settings_paths().into_iter().next());
    if let Some(path) = target {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(
            path,
            format!(
                "justify={justify}\ntwo_pane={two_pane}\nspritz_wpm={}\nspritz_pause_on_punct={}\nspritz_punct_pause_ms={}\n",
                spritz.wpm,
                spritz.pause_on_punct,
                spritz.punct_pause_ms
            ),
        );
    }
}
