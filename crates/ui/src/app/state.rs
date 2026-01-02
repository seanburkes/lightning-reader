use std::{
    collections::HashMap,
    sync::mpsc::{Receiver, Sender},
};

use arboard::Clipboard;
use reader_core::{
    pdf::OutlineEntry,
    types::{Block as ReaderBlock, Document, TocEntry},
};

use crate::{
    reader_view::Theme,
    search_view::SearchView,
    spritz_view::SpritzView,
    views::{FootnoteView, TocView},
};

use super::types::{ChapterPrefetchRequest, IncomingChapter, IncomingPage, Mode, PrefetchRequest};

pub struct App {
    pub blocks: Vec<ReaderBlock>,
    pub initial_page: Option<usize>,
    pub mode: Mode,
    pub toc: Option<TocView>,
    pub search: Option<SearchView>,
    pub spritz: Option<SpritzView>,
    pub footnote: Option<FootnoteView>,
    pub chapter_titles: Vec<String>,
    pub chapter_hrefs: Vec<String>,
    pub toc_entries: Vec<TocEntry>,
    pub outlines: Vec<OutlineEntry>,
    pub book_title: Option<String>,
    pub author: Option<String>,
    pub book_id: Option<String>,
    pub theme: Theme,
    pub last_search: Option<String>,
    pub last_search_hit: Option<usize>,
    pub show_help: bool,
    pub incoming_pages: Option<Receiver<IncomingPage>>,
    pub total_pages: Option<usize>,
    pub prefetch_tx: Option<Sender<PrefetchRequest>>,
    pub prefetch_window: usize,
    pub last_prefetch_at: Option<usize>,
    pub incoming_chapters: Option<Receiver<IncomingChapter>>,
    pub total_chapters: Option<usize>,
    pub prefetch_chapter_tx: Option<Sender<ChapterPrefetchRequest>>,
    pub prefetch_chapter_window: usize,
    pub last_chapter_prefetch_at: Option<usize>,
    pub pending_chapter_jump: Option<String>,
    pub chapter_index_by_href: HashMap<String, usize>,
    pub clipboard: Option<Clipboard>,
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
            footnote: None,
            chapter_titles: Vec::new(),
            chapter_hrefs: Vec::new(),
            toc_entries: Vec::new(),
            outlines: Vec::new(),
            book_title: None,
            author: None,
            book_id: None,
            theme: Theme::default(),
            last_search: None,
            last_search_hit: None,
            show_help: false,
            incoming_pages: None,
            total_pages: None,
            prefetch_tx: None,
            prefetch_window: 2,
            last_prefetch_at: None,
            incoming_chapters: None,
            total_chapters: None,
            prefetch_chapter_tx: None,
            prefetch_chapter_window: 2,
            last_chapter_prefetch_at: None,
            pending_chapter_jump: None,
            chapter_index_by_href: HashMap::new(),
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
            footnote: None,
            chapter_titles: Vec::new(),
            chapter_hrefs: Vec::new(),
            toc_entries: Vec::new(),
            outlines: Vec::new(),
            book_title: None,
            author: None,
            book_id: None,
            theme: Theme::default(),
            last_search: None,
            last_search_hit: None,
            show_help: false,
            incoming_pages: None,
            total_pages: None,
            prefetch_tx: None,
            prefetch_window: 2,
            last_prefetch_at: None,
            incoming_chapters: None,
            total_chapters: None,
            prefetch_chapter_tx: None,
            prefetch_chapter_window: 2,
            last_chapter_prefetch_at: None,
            pending_chapter_jump: None,
            chapter_index_by_href: HashMap::new(),
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
            footnote: None,
            chapter_titles,
            chapter_hrefs: Vec::new(),
            toc_entries: Vec::new(),
            outlines: Vec::new(),
            book_title: None,
            author: None,
            book_id: None,
            theme: Theme::default(),
            last_search: None,
            last_search_hit: None,
            show_help: false,
            incoming_pages: None,
            total_pages: None,
            prefetch_tx: None,
            prefetch_window: 2,
            last_prefetch_at: None,
            incoming_chapters: None,
            total_chapters: None,
            prefetch_chapter_tx: None,
            prefetch_chapter_window: 2,
            last_chapter_prefetch_at: None,
            pending_chapter_jump: None,
            chapter_index_by_href: HashMap::new(),
            clipboard: None,
        }
    }

    pub fn new_with_document(document: Document, initial_page: usize) -> Self {
        let (info, blocks, chapter_titles, chapter_hrefs, toc_entries, outlines) =
            document.into_parts();
        let mut app = Self::new_with_blocks_at(blocks, initial_page, chapter_titles);
        app.chapter_hrefs = chapter_hrefs;
        app.toc_entries = toc_entries;
        app.book_title = info.title().map(str::to_string);
        app.author = info.author().map(str::to_string);
        app.book_id = Some(info.id().to_string());
        app.outlines = outlines;
        if !app.chapter_titles.is_empty() {
            app.total_chapters = Some(app.chapter_titles.len());
        }
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
        app.total_chapters = None;
        app
    }

    pub fn new_with_document_chapter_streaming(
        document: Document,
        initial_page: usize,
        incoming_chapters: Receiver<IncomingChapter>,
        total_chapters: Option<usize>,
        prefetch_tx: Sender<ChapterPrefetchRequest>,
        prefetch_window: usize,
        chapter_index_by_href: HashMap<String, usize>,
    ) -> Self {
        let mut app = Self::new_with_document(document, initial_page);
        app.incoming_chapters = Some(incoming_chapters);
        app.total_chapters = total_chapters;
        app.prefetch_chapter_tx = Some(prefetch_tx);
        app.prefetch_chapter_window = prefetch_window;
        app.chapter_index_by_href = chapter_index_by_href;
        app
    }
}
