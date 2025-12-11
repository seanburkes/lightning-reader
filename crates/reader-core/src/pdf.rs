use std::path::Path;

use lopdf::Document as LoDocument;
use thiserror::Error;

use crate::types::Block;

#[derive(Debug, Error)]
pub enum PdfError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("PDF parse error: {0}")]
    Pdf(#[from] lopdf::Error),
    #[error("PDF is empty")]
    Empty,
}

pub struct PdfDocument {
    pub title: Option<String>,
    pub author: Option<String>,
    pub blocks: Vec<Block>,
    pub chapter_titles: Vec<String>,
}

pub fn load_pdf(path: &Path) -> Result<PdfDocument, PdfError> {
    let doc = LoDocument::load(path)?;
    let pages = doc.get_pages();
    if pages.is_empty() {
        return Err(PdfError::Empty);
    }

    let (title, author) = pdf_metadata(&doc);

    let mut blocks: Vec<Block> = Vec::new();
    let mut chapter_titles: Vec<String> = Vec::with_capacity(pages.len());
    for (idx, (page_num, _)) in pages.iter().enumerate() {
        if idx > 0 {
            blocks.push(Block::Paragraph(String::new()));
            blocks.push(Block::Paragraph("───".into()));
            blocks.push(Block::Paragraph(String::new()));
        }
        let text = doc.extract_text(&[*page_num]).unwrap_or_default();
        let page_blocks = page_text_to_blocks(&text);
        if page_blocks.is_empty() {
            blocks.push(Block::Paragraph("[empty page]".into()));
        } else {
            blocks.extend(page_blocks);
        }
        chapter_titles.push(format!("Page {}", idx + 1));
    }

    Ok(PdfDocument {
        title,
        author,
        blocks,
        chapter_titles,
    })
}

fn page_text_to_blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    for para in text.split("\n\n") {
        let cleaned = para.trim();
        if cleaned.is_empty() {
            continue;
        }
        let mut normalized = String::new();
        for (i, line) in cleaned.lines().enumerate() {
            if i > 0 {
                normalized.push(' ');
            }
            normalized.push_str(line.trim());
        }
        if !normalized.is_empty() {
            out.push(Block::Paragraph(normalized));
        }
    }
    out
}

fn pdf_metadata(doc: &LoDocument) -> (Option<String>, Option<String>) {
    let mut title: Option<String> = None;
    let mut author: Option<String> = None;
    if let Ok(info_obj) = doc.trailer.get(b"Info") {
        if let Ok(info_ref) = info_obj.as_reference() {
            if let Ok(dict) = doc.get_dictionary(info_ref) {
                if let Ok(val) = dict.get(b"Title") {
                    title = object_to_string(val);
                }
                if let Ok(val) = dict.get(b"Author") {
                    author = object_to_string(val);
                }
            }
        }
    }
    (title, author)
}

fn object_to_string(obj: &lopdf::Object) -> Option<String> {
    match obj {
        lopdf::Object::String(s, _) => Some(String::from_utf8_lossy(&s[..]).to_string()),
        lopdf::Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
        _ => None,
    }
}
