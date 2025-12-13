use std::{
    collections::{HashSet, VecDeque},
    env,
    path::Path,
    sync::mpsc::{channel, Receiver, Sender},
    thread,
    time::Duration,
};

use directories::ProjectDirs;
use reader_core::{
    epub::EpubBook,
    pdf::PdfLoader,
    state::{load_state, save_state},
    types::{AppStateRecord, BookId, Document, DocumentFormat, DocumentInfo, Location},
};
use ui::app::{IncomingPage, PrefetchRequest};

fn main() {
    // Accept optional EPUB/PDF path: default to docs/alice.epub
    let args: Vec<String> = env::args().collect();
    let input_path = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "docs/alice.epub".to_string());
    let format = detect_format(&input_path);

    if matches!(format, DocumentFormat::Pdf) {
        let path = Path::new(&input_path);
        let page_limit = env::var("LIBRARIAN_PDF_PAGE_LIMIT")
            .ok()
            .and_then(|s| s.parse::<usize>().ok());
        let initial_pages = env::var("LIBRARIAN_PDF_INITIAL_PAGES")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(3);
        let prefetch_window = env::var("LIBRARIAN_PDF_PREFETCH_PAGES")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(2);
        match stream_pdf(path, page_limit, initial_pages) {
            Ok((document, book_id, rx, prefetch_tx, target_pages, actual_pages, truncated)) => {
                if truncated {
                    eprintln!(
                        "Page limit applied: loading up to {} of {} pages (set LIBRARIAN_PDF_PAGE_LIMIT=0 to load all)",
                        target_pages, actual_pages
                    );
                }
                run_reader_streaming(
                    document,
                    book_id,
                    0,
                    rx,
                    prefetch_tx,
                    actual_pages,
                    prefetch_window,
                );
                return;
            }
            Err(reader_core::pdf::PdfError::Encrypted) => {
                eprintln!("Failed to open PDF: file is encrypted (password protected)");
                return;
            }
            Err(e) => {
                eprintln!("Failed to open PDF: {}", e);
                return;
            }
        }
    }

    // Open EPUB and compute BookId (placeholder id = path sha256-like)
    let path = std::path::Path::new(&input_path);
    let book = match EpubBook::open(path) {
        Ok(b) => b,
        Err(_) => match EpubBook::open(Path::new("docs/alice.epub")) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Failed to open EPUB: {}", e);
                return;
            }
        },
    };
    let book_id = BookId {
        id: format!("path:{}", input_path),
        path: input_path.clone(),
        title: book.title.clone(),
        format: DocumentFormat::Epub3,
    };
    let document_info = DocumentInfo::from_book_id(&book_id, book.author.clone());

    // Debug: print spine length and first hrefs
    eprintln!("Title: {:?}", book.title);
    eprintln!("Spine length: {}", book.spine().len());
    for (i, item) in book.spine().iter().take(5).enumerate() {
        eprintln!("  [{}] {}", i, item.href);
    }

    // Select first spine chapter with meaningful text
    let mut blocks = Vec::new();
    let mut selected_index: usize = 0;
    // Common non-content hints to skip
    let skip_hints = [
        "cover",
        "nav",
        "toc",
        "title",
        "front",
        "copyright",
        "acknowledg",
        "glossary",
        "colophon",
        "dedication",
    ];
    for (idx, item) in book.spine().iter().enumerate() {
        let href = item.href.to_lowercase();
        let mt_is_xhtml = item
            .media_type
            .as_deref()
            .map(|mt| mt.contains("xhtml") || mt.contains("html"))
            .unwrap_or(true);
        if !mt_is_xhtml || skip_hints.iter().any(|h| href.contains(h)) {
            continue;
        }
        let html = match book.load_chapter(item) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let mut b = reader_core::normalize::html_to_blocks(&html);
        b = reader_core::normalize::postprocess_blocks(b);
        // Check for meaningful prose: at least one paragraph/heading with length
        let has_meaningful = b.iter().any(|blk| match blk {
            reader_core::types::Block::Paragraph(t) | reader_core::types::Block::Heading(t, _) => {
                t.trim().len() >= 32
            }
            _ => false,
        });
        if has_meaningful {
            blocks = b;
            selected_index = idx;
            break;
        }
    }
    if blocks.is_empty() {
        // Fallback to first spine item that loads
        if let Some((idx, item)) = book.spine().iter().enumerate().next() {
            let html = book.load_chapter(item).unwrap_or_default();
            blocks = reader_core::normalize::html_to_blocks(&html);
            blocks = reader_core::normalize::postprocess_blocks(blocks);
            selected_index = idx;
        }
    }

    // Concatenate subsequent chapters to build a continuous flow
    // Titles for TOC
    let mut chapter_titles: Vec<String> = Vec::new();
    // Build href->label map from official nav if available
    let label_map = book.toc_labels().unwrap_or_default();
    fn book_base(_book: &EpubBook) -> std::path::PathBuf {
        // `toc_labels` uses OPF-parent normalization internally; we cannot access it here.
        // Use empty base to produce relative keys that match when OPF is at root.
        std::path::Path::new("").to_path_buf()
    }
    fn normalize_spine_href(base: &std::path::Path, href: &str) -> String {
        base.join(href.split('#').next().unwrap_or(href))
            .to_string_lossy()
            .to_string()
    }
    if !blocks.is_empty() {
        let mut all_blocks = blocks.clone();
        // Title for the initially selected chapter: prefer official label if available
        let initial_title = {
            let base = book_base(&book);
            let selected_key = normalize_spine_href(&base, &book.spine()[selected_index].href);
            label_map
                .get(&selected_key)
                .cloned()
                .or_else(|| {
                    blocks.iter().find_map(|blk| match blk {
                        reader_core::types::Block::Heading(t, _) => Some(t.clone()),
                        _ => None,
                    })
                })
                .unwrap_or_else(|| format!("Chapter {}", chapter_titles.len() + 1))
        };
        chapter_titles.push(initial_title);
        for item in book.spine().iter().skip(selected_index + 1) {
            let href = item.href.to_lowercase();
            let mt_is_xhtml = item
                .media_type
                .as_deref()
                .map(|mt| mt.contains("xhtml") || mt.contains("html"))
                .unwrap_or(true);
            if !mt_is_xhtml || href.contains("nav") || href.contains("toc") {
                continue;
            }
            if let Ok(html) = book.load_chapter(item) {
                let mut b = reader_core::normalize::html_to_blocks(&html);
                b = reader_core::normalize::postprocess_blocks(b);
                if !b.is_empty() {
                    // Prefer official label; fallback to first heading or href-derived
                    let base = book_base(&book);
                    let key = normalize_spine_href(&base, &item.href);
                    let title = label_map
                        .get(&key)
                        .cloned()
                        .or_else(|| {
                            b.iter().find_map(|blk| match blk {
                                reader_core::types::Block::Heading(t, _) => Some(t.clone()),
                                _ => None,
                            })
                        })
                        .unwrap_or_else(|| {
                            let name = std::path::Path::new(&item.href)
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("chapter");
                            let mut s = name.replace(['_', '-', '.', '%'], " ");
                            s = s.split_whitespace().collect::<Vec<_>>().join(" ");
                            let s = s
                                .trim()
                                .trim_start_matches(|c: char| c.is_ascii_digit())
                                .trim()
                                .to_string();
                            if s.is_empty() {
                                "Chapter".to_string()
                            } else {
                                s
                            }
                        });
                    chapter_titles.push(title);
                    // Insert a visible separator between chapters
                    all_blocks.push(reader_core::types::Block::Paragraph(String::new()));
                    all_blocks.push(reader_core::types::Block::Paragraph("───".into()));
                    all_blocks.push(reader_core::types::Block::Paragraph(String::new()));
                    all_blocks.append(&mut b);
                }
            }
        }
        blocks = all_blocks;
    }

    let document = Document::new(document_info, blocks, chapter_titles);
    run_reader(document, book_id, selected_index);
}

fn stream_pdf(
    path: &Path,
    page_limit: Option<usize>,
    initial_pages: usize,
) -> Result<
    (
        Document,
        BookId,
        Receiver<IncomingPage>,
        std::sync::mpsc::Sender<PrefetchRequest>,
        usize,
        usize,
        bool,
    ),
    reader_core::pdf::PdfError,
> {
    let loader = PdfLoader::open(path)?;
    let total_pages_actual = loader.page_count();
    let target_pages = page_limit
        .and_then(|m| {
            if m == 0 {
                None
            } else {
                Some(m.min(total_pages_actual))
            }
        })
        .unwrap_or(total_pages_actual);
    let initial = initial_pages.max(1).min(target_pages);
    let summary = loader.summary().clone();
    let title = summary.clone().title.or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    });
    let mut blocks: Vec<reader_core::types::Block> = Vec::new();
    let mut chapter_titles: Vec<String> = Vec::new();
    for (idx, page_blocks) in loader.load_range(0, initial)? {
        if idx > 0 {
            blocks.push(reader_core::types::Block::Paragraph(String::new()));
            blocks.push(reader_core::types::Block::Paragraph("───".into()));
            blocks.push(reader_core::types::Block::Paragraph(String::new()));
        }
        blocks.extend(page_blocks);
        chapter_titles.push(format!("Page {}", idx + 1));
    }

    let (tx, rx) = channel();
    let (prefetch_tx, prefetch_rx) = channel::<PrefetchRequest>();
    if initial < target_pages {
        let start_at = initial;
        let loader = loader;
        thread::spawn(move || {
            let mut loaded: HashSet<usize> = (0..start_at).collect();
            let mut pending: VecDeque<usize> = VecDeque::new();
            let mut next_seq = start_at;
            loop {
                while let Ok(req) = prefetch_rx.try_recv() {
                    let end = (req.start + req.window).min(target_pages);
                    for idx in req.start..end {
                        if loaded.insert(idx) {
                            pending.push_back(idx);
                        }
                    }
                }
                if let Some(idx) = pending.pop_front() {
                    match loader.load_page(idx) {
                        Ok(page_blocks) => {
                            let msg = IncomingPage {
                                page_index: idx,
                                blocks: page_blocks,
                            };
                            if tx.send(msg).is_err() {
                                return;
                            }
                        }
                        Err(_) => {}
                    }
                    continue;
                }
                while next_seq < target_pages && loaded.contains(&next_seq) {
                    next_seq += 1;
                }
                if next_seq >= target_pages {
                    // Wait briefly for any remaining requests before exiting
                    if prefetch_rx
                        .recv_timeout(Duration::from_millis(200))
                        .is_err()
                    {
                        break;
                    } else {
                        continue;
                    }
                }
                pending.push_back(next_seq);
                loaded.insert(next_seq);
                next_seq += 1;
            }
        });
    }

    let book_id = BookId {
        id: format!("path:{}", path.display()),
        path: path.display().to_string(),
        title,
        format: DocumentFormat::Pdf,
    };
    let info = DocumentInfo::from_book_id(&book_id, summary.author.clone());
    let document = Document::new(info, blocks, chapter_titles);
    let truncated = target_pages < total_pages_actual;
    Ok((
        document,
        book_id,
        rx,
        prefetch_tx,
        target_pages,
        total_pages_actual,
        truncated,
    ))
}

fn run_reader(document: Document, book_id: BookId, selected_index: usize) {
    // Load last location and update initial spine index
    let mut last = load_state(&book_id)
        .map(|r| r.last_location)
        .unwrap_or(Location {
            spine_index: 0,
            offset: 0,
        });
    last.spine_index = selected_index;

    let mut app = ui::app::App::new_with_document(document, last.offset);
    apply_theme_config(&mut app);

    let current_idx = match app.run() {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Error: {}", e);
            0
        }
    };

    // Save last location using current page index
    last.offset = current_idx;
    let rec = AppStateRecord {
        book: book_id,
        last_location: last,
        bookmarks: vec![],
    };
    let _ = save_state(&rec);

    eprintln!("Run with: cargo run -p librarian [path_to_epub]  # default docs/alice.epub");
}

fn run_reader_streaming(
    document: Document,
    book_id: BookId,
    selected_index: usize,
    incoming: Receiver<IncomingPage>,
    prefetch_tx: Sender<PrefetchRequest>,
    total_pages: usize,
    prefetch_window: usize,
) {
    // Load last location and update initial spine index
    let mut last = load_state(&book_id)
        .map(|r| r.last_location)
        .unwrap_or(Location {
            spine_index: 0,
            offset: 0,
        });
    last.spine_index = selected_index;

    let mut app = ui::app::App::new_with_document_streaming(
        document,
        last.offset,
        incoming,
        total_pages,
        prefetch_tx,
        prefetch_window,
    );
    apply_theme_config(&mut app);

    let current_idx = match app.run() {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Error: {}", e);
            0
        }
    };

    // Save last location using current page index
    last.offset = current_idx;
    let rec = AppStateRecord {
        book: book_id,
        last_location: last,
        bookmarks: vec![],
    };
    let _ = save_state(&rec);

    eprintln!("Run with: cargo run -p librarian [path_to_epub]  # default docs/alice.epub");
}

fn apply_theme_config(app: &mut ui::app::App) {
    // Load theme from ~/.config/librarian/config.toml
    if let Some(proj) = ProjectDirs::from("com", "sean", "librarian") {
        let cfg_path = proj.config_dir().join("config.toml");
        if let Ok(text) = std::fs::read_to_string(&cfg_path) {
            if let Ok(value) = toml::from_str::<toml::Value>(&text) {
                if let Some(theme) = value.get("theme").and_then(|v| v.as_table()) {
                    fn parse_color(s: &str) -> Option<ratatui::style::Color> {
                        match s.to_lowercase().as_str() {
                            "black" => Some(ratatui::style::Color::Black),
                            "red" => Some(ratatui::style::Color::Red),
                            "green" => Some(ratatui::style::Color::Green),
                            "yellow" => Some(ratatui::style::Color::Yellow),
                            "blue" => Some(ratatui::style::Color::Blue),
                            "magenta" => Some(ratatui::style::Color::Magenta),
                            "cyan" => Some(ratatui::style::Color::Cyan),
                            "white" => Some(ratatui::style::Color::White),
                            "gray" | "darkgray" => Some(ratatui::style::Color::DarkGray),
                            _ => None,
                        }
                    }
                    // Presets by name
                    if let Some(name) = theme.get("name").and_then(|v| v.as_str()) {
                        match name.to_lowercase().as_str() {
                            "gruvbox" => {
                                app.theme.header_bg = ratatui::style::Color::Yellow;
                                app.theme.header_fg = ratatui::style::Color::Black;
                                app.theme.header_pad_bg = ratatui::style::Color::DarkGray;
                                app.theme.footer_bg = ratatui::style::Color::Green;
                                app.theme.footer_fg = ratatui::style::Color::Black;
                                app.theme.footer_pad_bg = ratatui::style::Color::DarkGray;
                            }
                            "dracula" => {
                                app.theme.header_bg = ratatui::style::Color::Magenta;
                                app.theme.header_fg = ratatui::style::Color::White;
                                app.theme.header_pad_bg = ratatui::style::Color::DarkGray;
                                app.theme.footer_bg = ratatui::style::Color::Blue;
                                app.theme.footer_fg = ratatui::style::Color::White;
                                app.theme.footer_pad_bg = ratatui::style::Color::DarkGray;
                            }
                            "tokyonight" => {
                                app.theme.header_bg = ratatui::style::Color::Blue;
                                app.theme.header_fg = ratatui::style::Color::White;
                                app.theme.header_pad_bg = ratatui::style::Color::DarkGray;
                                app.theme.footer_bg = ratatui::style::Color::Cyan;
                                app.theme.footer_fg = ratatui::style::Color::Black;
                                app.theme.footer_pad_bg = ratatui::style::Color::DarkGray;
                            }
                            _ => {}
                        }
                    }
                    // Individual overrides
                    if let Some(s) = theme
                        .get("header_bg")
                        .and_then(|v| v.as_str())
                        .and_then(parse_color)
                    {
                        app.theme.header_bg = s;
                    }
                    if let Some(s) = theme
                        .get("header_fg")
                        .and_then(|v| v.as_str())
                        .and_then(parse_color)
                    {
                        app.theme.header_fg = s;
                    }
                    if let Some(s) = theme
                        .get("header_pad_bg")
                        .and_then(|v| v.as_str())
                        .and_then(parse_color)
                    {
                        app.theme.header_pad_bg = s;
                    }
                    if let Some(s) = theme
                        .get("footer_bg")
                        .and_then(|v| v.as_str())
                        .and_then(parse_color)
                    {
                        app.theme.footer_bg = s;
                    }
                    if let Some(s) = theme
                        .get("footer_fg")
                        .and_then(|v| v.as_str())
                        .and_then(parse_color)
                    {
                        app.theme.footer_fg = s;
                    }
                    if let Some(s) = theme
                        .get("footer_pad_bg")
                        .and_then(|v| v.as_str())
                        .and_then(parse_color)
                    {
                        app.theme.footer_pad_bg = s;
                    }
                }
            }
        }
    }
}

fn detect_format(path: &str) -> DocumentFormat {
    Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|ext| {
            let lower = ext.to_ascii_lowercase();
            match lower.as_str() {
                "pdf" => DocumentFormat::Pdf,
                "epub" => DocumentFormat::Epub3,
                _ => DocumentFormat::Other,
            }
        })
        .unwrap_or(DocumentFormat::Epub3)
}
