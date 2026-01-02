use std::path::Path;

use lopdf::Document as LoDocument;

use crate::types::Block;

use super::error::PdfError;
use super::text::page_text_to_blocks;
use super::types::PdfSummary;

pub(super) struct LopdfBackend {
    doc: LoDocument,
    pub(super) pages: Vec<u32>,
    pub(super) summary: PdfSummary,
}

impl LopdfBackend {
    pub(super) fn open(path: &Path) -> Result<Self, PdfError> {
        let doc = LoDocument::load(path)?;
        if doc.is_encrypted() {
            return Err(PdfError::Encrypted);
        }
        let pages_map = doc.get_pages();
        if pages_map.is_empty() {
            return Err(PdfError::Empty);
        }
        let page_count = pages_map.len();
        let pages = pages_map.into_keys().collect();
        let (title, author) = pdf_metadata(&doc);
        Ok(Self {
            doc,
            pages,
            summary: PdfSummary {
                title,
                author,
                page_count,
            },
        })
    }

    pub(super) fn load_page(&self, page_index: usize) -> Result<Vec<Block>, PdfError> {
        let page_num = *self
            .pages
            .get(page_index)
            .ok_or(PdfError::InvalidPage(page_index))?;
        let text = self.doc.extract_text(&[page_num]).unwrap_or_default();
        let mut blocks = page_text_to_blocks(&text);
        if blocks.is_empty() {
            blocks.push(Block::Paragraph("[empty page]".into()));
        }
        Ok(blocks)
    }
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
