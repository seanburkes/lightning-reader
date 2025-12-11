use std::{env, path::Path};

use directories::ProjectDirs;
use reader_core::{
    epub::EpubBook,
    state::{load_state, save_state},
    types::{AppStateRecord, BookId, Document, DocumentFormat, DocumentInfo, Location},
};

fn main() {
    // Accept optional EPUB path: default to docs/alice.epub
    let args: Vec<String> = env::args().collect();
    let epub_path = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "docs/alice.epub".to_string());

    // Open EPUB and compute BookId (placeholder id = path sha256-like)
    let path = std::path::Path::new(&epub_path);
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
        id: format!("path:{}", epub_path),
        path: epub_path.clone(),
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

    // Load last location and update initial spine index
    let mut last = load_state(&book_id)
        .map(|r| r.last_location)
        .unwrap_or(Location {
            spine_index: 0,
            offset: 0,
        });
    last.spine_index = selected_index;

    let mut app = ui::app::App::new_with_document(document, last.offset);

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
