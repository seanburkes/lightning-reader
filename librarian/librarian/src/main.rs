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
    for item in book.spine() {
        let href = item.href.to_lowercase();
        let mt_ok = item.media_type.as_deref().map(|mt| mt.contains("xhtml")).unwrap_or(true);
        if !mt_ok || href.contains("cover") || href.contains("toc") || href.contains("nav") {
            continue;
        }
        let html = match book.load_chapter(item) { Ok(s) => s, Err(_) => continue };
        let b = reader_core::normalize::html_to_blocks(&html);
        // Check for content
        let has_text = b.iter().any(|blk| match blk { reader_core::types::Block::Paragraph(t) | reader_core::types::Block::Heading(t, _) => !t.trim().is_empty(), _ => false });
        if has_text { blocks = b; break; }
    }
    if blocks.is_empty() {
        // Fallback to first spine item
        if let Some(item) = book.spine().get(0) {
            let html = book.load_chapter(item).unwrap_or_default();
            blocks = reader_core::normalize::html_to_blocks(&html);
        }
    }

    // Load last location
    let last = load_state(&book_id).map(|r| r.last_location).unwrap_or(Location { spine_index: 0, offset: 0 });

    let app = ui::app::App::new_with_blocks(blocks);
    if let Err(e) = app.run() {
        eprintln!("Error: {}", e);
    }

    // Save last location (stub)
    let rec = AppStateRecord { book: book_id, last_location: last, bookmarks: vec![] };
    let _ = save_state(&rec);

    eprintln!("Run with: cargo run -p librarian [path_to_epub]  # default docs/alice.epub");
}
