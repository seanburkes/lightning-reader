use std::env;
use std::path::Path;
use reader_core::{epub::EpubBook, types::{BookId, AppStateRecord, Location}};
use reader_core::state::{load_state, save_state};

fn main() {
    // Accept optional EPUB path: default to docs/alice.epub
    let args: Vec<String> = env::args().collect();
    let epub_path = args.get(1).cloned().unwrap_or_else(|| "docs/alice.epub".to_string());

    // Open EPUB and compute BookId (placeholder id = path sha256-like)
    let path = std::path::Path::new(&epub_path);
    let book = match EpubBook::open(path) {
        Ok(b) => b,
        Err(_) => match EpubBook::open(Path::new("docs/alice.epub")) { Ok(b) => b, Err(e) => { eprintln!("Failed to open EPUB: {}", e); return; } }
    };
    let book_id = BookId { id: format!("path:{}", epub_path), path: epub_path.clone(), title: book.title.clone() };

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
        "cover", "nav", "toc", "title", "front", "copyright",
        "acknowledg", "glossary", "colophon", "dedication",
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
            reader_core::types::Block::Paragraph(t)
            | reader_core::types::Block::Heading(t, _) => t.trim().len() >= 32,
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
    if !blocks.is_empty() {
        let mut all_blocks = blocks.clone();
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

    // Load last location and update initial spine index
    let mut last = load_state(&book_id)
        .map(|r| r.last_location)
        .unwrap_or(Location { spine_index: 0, offset: 0 });
    last.spine_index = selected_index;

    let app = ui::app::App::new_with_blocks_at(blocks, last.offset);
    let current_idx = match app.run() {
        Ok(idx) => idx,
        Err(e) => { eprintln!("Error: {}", e); 0 }
    };

    // Save last location using current page index
    last.offset = current_idx;
    let rec = AppStateRecord { book: book_id, last_location: last, bookmarks: vec![] };
    let _ = save_state(&rec);

    eprintln!("Run with: cargo run -p librarian [path_to_epub]  # default docs/alice.epub");
}
