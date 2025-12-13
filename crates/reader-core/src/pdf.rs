use std::{num::NonZeroUsize, path::Path, sync::Mutex};

use lopdf::Document as LoDocument;
use lru::LruCache;
use thiserror::Error;

use crate::types::Block;

#[derive(Debug, Error)]
pub enum PdfError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("PDF parse error: {0}")]
    Pdf(#[from] lopdf::Error),
    #[error("PDF requires a password or is encrypted")]
    Encrypted,
    #[error("PDF is empty")]
    Empty,
    #[error("Requested page {0} is out of bounds")]
    InvalidPage(usize),
}

pub struct PdfDocument {
    pub title: Option<String>,
    pub author: Option<String>,
    pub blocks: Vec<Block>,
    pub chapter_titles: Vec<String>,
    pub truncated: bool,
}

#[derive(Clone)]
pub struct PdfSummary {
    pub title: Option<String>,
    pub author: Option<String>,
    pub page_count: usize,
}

pub struct PdfLoader {
    doc: LoDocument,
    pages: Vec<u32>,
    summary: PdfSummary,
    cache: Mutex<LruCache<usize, Vec<Block>>>,
}

impl PdfLoader {
    pub fn open(path: &Path) -> Result<Self, PdfError> {
        let doc = LoDocument::load(path)?;
        if doc.is_encrypted() {
            return Err(PdfError::Encrypted);
        }
        let pages_map = doc.get_pages();
        if pages_map.is_empty() {
            return Err(PdfError::Empty);
        }
        let page_count = pages_map.len();
        let pages = pages_map.into_iter().map(|(page, _)| page).collect();
        let (title, author) = pdf_metadata(&doc);
        Ok(Self {
            doc,
            pages,
            summary: PdfSummary {
                title,
                author,
                page_count,
            },
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(8).unwrap())),
        })
    }

    pub fn summary(&self) -> &PdfSummary {
        &self.summary
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn load_page(&self, page_index: usize) -> Result<Vec<Block>, PdfError> {
        if let Some(cached) = self.cache.lock().unwrap().get(&page_index).cloned() {
            return Ok(cached);
        }
        let page_num = *self
            .pages
            .get(page_index)
            .ok_or(PdfError::InvalidPage(page_index))?;
        let text = self.doc.extract_text(&[page_num]).unwrap_or_default();
        let mut blocks = page_text_to_blocks(&text);
        if blocks.is_empty() {
            blocks.push(Block::Paragraph("[empty page]".into()));
        }
        self.cache.lock().unwrap().put(page_index, blocks.clone());
        Ok(blocks)
    }

    pub fn load_range(
        &self,
        start: usize,
        count: usize,
    ) -> Result<Vec<(usize, Vec<Block>)>, PdfError> {
        let end = (start + count).min(self.page_count());
        let mut out = Vec::with_capacity(end.saturating_sub(start));
        for idx in start..end {
            out.push((idx, self.load_page(idx)?));
        }
        Ok(out)
    }
}

pub fn load_pdf(path: &Path) -> Result<PdfDocument, PdfError> {
    load_pdf_with_limit(path, None)
}

pub fn load_pdf_with_limit(path: &Path, max_pages: Option<usize>) -> Result<PdfDocument, PdfError> {
    let loader = PdfLoader::open(path)?;
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
        blocks.extend(page_blocks);
        chapter_titles.push(format!("Page {}", idx + 1));
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
        title: loader.summary.title.clone(),
        author: loader.summary.author.clone(),
        blocks,
        chapter_titles,
        truncated,
    })
}

fn page_text_to_blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut current = String::new();
    for raw_line in text.lines() {
        let line = raw_line.trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush_para(&mut current, &mut out);
            continue;
        }
        if ends_with_hard_hyphen(trimmed) {
            current.push_str(trimmed.trim_end_matches('-'));
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(trimmed);
        }
    }
    flush_para(&mut current, &mut out);
    out
}

fn flush_para(current: &mut String, out: &mut Vec<Block>) {
    let cleaned = current.trim();
    if !cleaned.is_empty() {
        out.push(Block::Paragraph(cleaned.to_string()));
    }
    current.clear();
}

fn ends_with_hard_hyphen(s: &str) -> bool {
    s.ends_with('-') && !s.ends_with("--")
}

fn pdf_metadata(doc: &LoDocument) -> (Option<String>, Option<String>) {
    let mut title: Option<String> = None;
    let mut author: Option<String> = None;
    if let Ok(info_obj) = doc.trailer.get(b"Info") {
        let dict_opt: Option<lopdf::Dictionary> = if let Ok(info_ref) = info_obj.as_reference() {
            doc.get_dictionary(info_ref).ok().cloned()
        } else if let Ok(dict_ref) = info_obj.as_dict() {
            Some(dict_ref.clone())
        } else {
            None
        };
        if let Some(dict) = dict_opt {
            if let Ok(val) = dict.get(b"Title") {
                title = object_to_string(val);
            }
            if let Ok(val) = dict.get(b"Author") {
                author = object_to_string(val);
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
