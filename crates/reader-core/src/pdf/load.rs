use std::path::Path;

use crate::types::Block;

use super::error::PdfError;
use super::loader::PdfLoader;
use super::text::page_title_from_blocks;
use super::types::{PdfBackendKind, PdfDocument};

pub fn load_pdf(path: &Path) -> Result<PdfDocument, PdfError> {
    load_pdf_with_backend(path, None, PdfBackendKind::from_env())
}

pub fn load_pdf_with_limit(path: &Path, max_pages: Option<usize>) -> Result<PdfDocument, PdfError> {
    load_pdf_with_backend(path, max_pages, PdfBackendKind::from_env())
}

pub fn load_pdf_with_backend(
    path: &Path,
    max_pages: Option<usize>,
    backend: PdfBackendKind,
) -> Result<PdfDocument, PdfError> {
    let mut loader = PdfLoader::open_with_backend(path, backend)?;
    let total_pages = loader.page_count();
    let to_load = max_pages
        .and_then(|m| if m == 0 { None } else { Some(m) })
        .map(|m| m.min(total_pages))
        .unwrap_or(total_pages);
    let mut blocks: Vec<Block> = Vec::new();
    let mut chapter_titles: Vec<String> = Vec::with_capacity(to_load);
    for idx in 0..to_load {
        if idx > 0 {
            blocks.push(Block::Paragraph(String::new()));
            blocks.push(Block::Paragraph("───".into()));
            blocks.push(Block::Paragraph(String::new()));
        }
        let page_blocks = loader.load_page(idx)?;
        blocks.extend(page_blocks.clone());
        let title =
            page_title_from_blocks(&page_blocks).unwrap_or_else(|| format!("Page {}", idx + 1));
        chapter_titles.push(title);
    }

    let truncated = to_load < total_pages;
    if truncated {
        blocks.push(Block::Paragraph(String::new()));
        blocks.push(Block::Paragraph(format!(
            "[truncated: loaded {} of {} pages; set LIBRARIAN_PDF_PAGE_LIMIT=0 to load all]",
            to_load, total_pages
        )));
    }

    Ok(PdfDocument {
        title: loader.summary().title.clone(),
        author: loader.summary().author.clone(),
        blocks,
        chapter_titles,
        outlines: loader.outlines().unwrap_or_default(),
        truncated,
    })
}
