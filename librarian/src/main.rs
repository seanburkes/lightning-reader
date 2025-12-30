use std::{
    collections::{HashSet, VecDeque},
    env,
    path::Path,
    sync::mpsc::{channel, Receiver, Sender},
    thread,
    time::Duration,
};

use reader_core::{
    epub::EpubBook,
    pdf::PdfLoader,
    state::{load_state, save_state},
    types::{AppStateRecord, BookId, Document, DocumentFormat, DocumentInfo, Location},
};
use ui::app::{IncomingPage, PrefetchRequest};

fn main() {
    // Accept optional EPUB/PDF/TXT/MD path: default to docs/alice.epub
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
            .unwrap_or(1);
        let backend = reader_core::pdf::PdfBackendKind::from_env();
        let prefetch_window = env::var("LIBRARIAN_PDF_PREFETCH_PAGES")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(2);
        match stream_pdf(path, page_limit, initial_pages, backend) {
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

    if matches!(format, DocumentFormat::Text | DocumentFormat::Markdown) {
        let path = Path::new(&input_path);
        match reader_core::text::TextFile::open(path) {
            Ok(text_doc) => {
                let document = text_doc.to_document();
                let book_id = BookId {
                    id: format!("path:{}", path.display()),
                    path: path.display().to_string(),
                    title: document.info.title.clone(),
                    format,
                };
                run_reader(document, book_id, 0);
                return;
            }
            Err(e) => {
                eprintln!("Failed to open text file: {}", e);
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
                eprintln!("Failed to open file: {}", e);
                return;
            }
        },
    };
    let book_id = BookId {
        id: format!("path:{}", path.display()),
        path: path.display().to_string(),
        title: book.title.clone(),
        format,
    };
    let document_info = DocumentInfo::from_book_id(&book_id, book.author.clone());

    // Debug: print spine length and first hrefs
    eprintln!("Title: {:?}", book.title);
    eprintln!("Spine length: {}", book.spine().len());
    for (i, item) in book.spine().iter().take(5).enumerate() {
        eprintln!("  [{}] {}", i, item.href);
    }

    let label_map = book.toc_labels().unwrap_or_default();
    fn normalize_spine_href(base: &std::path::Path, href: &str) -> String {
        base.join(href.split('#').next().unwrap_or(href))
            .to_string_lossy()
            .to_string()
    }
    fn normalize_epub_path(path: &std::path::Path) -> std::path::PathBuf {
        let mut out = std::path::PathBuf::new();
        for comp in path.components() {
            match comp {
                std::path::Component::ParentDir => {
                    out.pop();
                }
                std::path::Component::CurDir => {}
                _ => out.push(comp.as_os_str()),
            }
        }
        out
    }
    fn normalize_spine_href_for_links(base: &std::path::Path, href: &str) -> String {
        let joined = base.join(href.split('#').next().unwrap_or(href));
        normalize_epub_path(&joined).to_string_lossy().to_string()
    }
    fn resolve_internal_link(
        base_root: &std::path::Path,
        chapter_dir: &std::path::Path,
        chapter_prefix: &str,
        href: &str,
    ) -> Option<String> {
        let href = href.trim();
        if href.is_empty() {
            return None;
        }
        let mut parts = href.splitn(2, '#');
        let path_part = parts.next().unwrap_or("");
        let frag = parts.next();
        let resolved_base = if path_part.is_empty() {
            chapter_prefix.to_string()
        } else {
            let resolved = if path_part.starts_with('/') {
                normalize_epub_path(&base_root.join(path_part.trim_start_matches('/')))
            } else {
                normalize_epub_path(&chapter_dir.join(path_part))
            };
            resolved.to_string_lossy().to_string()
        };
        let mut target = resolved_base;
        if let Some(frag) = frag {
            if !frag.is_empty() {
                target.push('#');
                target.push_str(frag);
            }
        }
        Some(target)
    }
    fn is_placeholder_text(text: &str) -> bool {
        let trimmed = text.trim();
        trimmed == "───"
            || trimmed == "[math]"
            || trimmed == "[svg]"
            || trimmed == "[image]"
            || trimmed.starts_with("[image:")
    }
    fn has_content(blocks: &[reader_core::types::Block]) -> bool {
        blocks.iter().any(|blk| match blk {
            reader_core::types::Block::Paragraph(t)
            | reader_core::types::Block::Heading(t, _)
            | reader_core::types::Block::Quote(t) => {
                let trimmed = t.trim();
                !trimmed.is_empty() && !is_placeholder_text(trimmed)
            }
            reader_core::types::Block::List(items) => items.iter().any(|item| {
                let trimmed = item.trim();
                !trimmed.is_empty() && !is_placeholder_text(trimmed)
            }),
            reader_core::types::Block::Code { text, .. } => !text.trim().is_empty(),
            reader_core::types::Block::Image(image) => {
                image.data.as_ref().map(|d| !d.is_empty()).unwrap_or(false)
                    || image
                        .caption
                        .as_ref()
                        .map(|t| !t.trim().is_empty())
                        .unwrap_or(false)
                    || image
                        .alt
                        .as_ref()
                        .map(|t| !t.trim().is_empty())
                        .unwrap_or(false)
            }
        })
    }
    fn heading_title(blocks: &[reader_core::types::Block]) -> Option<String> {
        blocks.iter().find_map(|blk| match blk {
            reader_core::types::Block::Heading(t, _) => {
                let trimmed = t.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            _ => None,
        })
    }
    fn fallback_title(href: &str) -> String {
        let name = std::path::Path::new(href)
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
    }
    let base = book.opf_base();

    let mut blocks: Vec<reader_core::types::Block> = Vec::new();
    let mut chapter_titles: Vec<String> = Vec::new();
    let mut chapter_hrefs: Vec<String> = Vec::new();
    let mut selected_index: Option<usize> = None;
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
        if !mt_is_xhtml || href.contains("nav") || href.contains("toc") {
            continue;
        }
        let key = normalize_spine_href(&base, &item.href);
        let label = label_map.get(&key).cloned();
        if label.is_none() && skip_hints.iter().any(|h| href.contains(h)) {
            continue;
        }
        let html = match book.load_chapter(item) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let chapter_prefix = normalize_spine_href_for_links(&base, &item.href);
        let chapter_path = normalize_epub_path(&base.join(&item.href));
        let chapter_dir = chapter_path.parent().unwrap_or(&base).to_path_buf();
        let base_root = base.clone();
        let link_base_root = base_root.clone();
        let link_chapter_dir = chapter_dir.clone();
        let link_prefix = chapter_prefix.clone();
        let mut b = reader_core::normalize::html_to_blocks_with_assets(
            &html,
            Some(chapter_prefix.as_str()),
            |src| {
                if src.starts_with("http://") || src.starts_with("https://") {
                    return None;
                }
                let resolved = if src.starts_with('/') {
                    base_root.join(src.trim_start_matches('/'))
                } else {
                    chapter_dir.join(src)
                };
                let resolved = normalize_epub_path(&resolved);
                let data = book.load_resource(&resolved).ok()?;
                Some((resolved.to_string_lossy().to_string(), data))
            },
            move |href| {
                resolve_internal_link(&link_base_root, &link_chapter_dir, &link_prefix, href)
            },
        );
        b = reader_core::normalize::postprocess_blocks(b);
        if !has_content(&b) {
            continue;
        }
        let title = label
            .or_else(|| heading_title(&b))
            .unwrap_or_else(|| fallback_title(&item.href));
        if !blocks.is_empty() {
            blocks.push(reader_core::types::Block::Paragraph(String::new()));
            blocks.push(reader_core::types::Block::Paragraph("───".into()));
            blocks.push(reader_core::types::Block::Paragraph(String::new()));
        }
        blocks.append(&mut b);
        chapter_titles.push(title);
        chapter_hrefs.push(chapter_prefix);
        if selected_index.is_none() {
            selected_index = Some(idx);
        }
    }
    if blocks.is_empty() {
        if let Some((idx, item)) = book.spine().iter().enumerate().find(|(_, item)| {
            item.media_type
                .as_deref()
                .map(|mt| mt.contains("xhtml") || mt.contains("html"))
                .unwrap_or(true)
        }) {
            let html = book.load_chapter(item).unwrap_or_default();
            let chapter_prefix = normalize_spine_href_for_links(&base, &item.href);
            let chapter_path = normalize_epub_path(&base.join(&item.href));
            let chapter_dir = chapter_path.parent().unwrap_or(&base).to_path_buf();
            let base_root = base.clone();
            let link_base_root = base_root.clone();
            let link_chapter_dir = chapter_dir.clone();
            let link_prefix = chapter_prefix.clone();
            let mut b = reader_core::normalize::html_to_blocks_with_assets(
                &html,
                Some(chapter_prefix.as_str()),
                |src| {
                    if src.starts_with("http://") || src.starts_with("https://") {
                        return None;
                    }
                    let resolved = if src.starts_with('/') {
                        base_root.join(src.trim_start_matches('/'))
                    } else {
                        chapter_dir.join(src)
                    };
                    let resolved = normalize_epub_path(&resolved);
                    let data = book.load_resource(&resolved).ok()?;
                    Some((resolved.to_string_lossy().to_string(), data))
                },
                move |href| {
                    resolve_internal_link(&link_base_root, &link_chapter_dir, &link_prefix, href)
                },
            );
            b = reader_core::normalize::postprocess_blocks(b);
            blocks = b;
            let key = normalize_spine_href(&base, &item.href);
            let title = label_map
                .get(&key)
                .cloned()
                .or_else(|| heading_title(&blocks))
                .unwrap_or_else(|| fallback_title(&item.href));
            chapter_titles.push(title);
            chapter_hrefs.push(chapter_prefix);
            selected_index = Some(idx);
        }
    }
    let selected_index = selected_index.unwrap_or(0);

    let document = Document::new(document_info, blocks, chapter_titles, chapter_hrefs);
    run_reader(document, book_id, selected_index);
}

fn stream_pdf(
    path: &Path,
    page_limit: Option<usize>,
    initial_pages: usize,
    backend: reader_core::pdf::PdfBackendKind,
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
    let loader = PdfLoader::open_with_backend(path, backend)?;
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
    let mut chapter_hrefs: Vec<String> = Vec::new();
    for (idx, page_blocks) in loader.load_range(0, initial)? {
        if idx > 0 {
            blocks.push(reader_core::types::Block::Paragraph(String::new()));
            blocks.push(reader_core::types::Block::Paragraph("───".into()));
            blocks.push(reader_core::types::Block::Paragraph(String::new()));
        }
        blocks.extend(page_blocks);
        chapter_titles.push(format!("Page {}", idx + 1));
        chapter_hrefs.push(format!("page:{}", idx + 1));
    }

    let (tx, rx) = channel();
    let (prefetch_tx, prefetch_rx) = channel::<PrefetchRequest>();
    if initial < target_pages {
        let start_at = initial;
        let loader = loader;
        thread::spawn(move || {
            let mut loaded: HashSet<usize> = (0..start_at).collect();
            let mut pending: VecDeque<usize> = (start_at..target_pages).collect();
            loop {
                while let Ok(req) = prefetch_rx.try_recv() {
                    let end = (req.start + req.window).min(target_pages);
                    for idx in req.start..end {
                        if loaded.insert(idx) {
                            pending.push_front(idx);
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
                // Nothing pending; wait briefly for any new requests, otherwise exit.
                if prefetch_rx
                    .recv_timeout(Duration::from_millis(200))
                    .is_err()
                {
                    break;
                }
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
    let document = Document::new(info, blocks, chapter_titles, chapter_hrefs);
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

    eprintln!("Run with: cargo run -p librarian [path_to_epub|path_to_txt|path_to_md]  # default docs/alice.epub");
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

    eprintln!("Run with: cargo run -p librarian [path_to_epub|path_to_txt|path_to_md]  # default docs/alice.epub");
}

fn apply_theme_config(app: &mut ui::app::App) {
    // Load theme from primary config root with legacy fallbacks
    let mut candidates = Vec::new();
    if let Some(dir) = reader_core::config::config_root() {
        candidates.push(dir.join("config.toml"));
    }
    for legacy in reader_core::config::legacy_config_roots() {
        candidates.push(legacy.join("config.toml"));
    }
    for cfg_path in candidates {
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
            break;
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
                "txt" | "text" => DocumentFormat::Text,
                "md" | "markdown" => DocumentFormat::Markdown,
                _ => DocumentFormat::Other,
            }
        })
        .unwrap_or(DocumentFormat::Text)
}
