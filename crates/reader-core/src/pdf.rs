use std::{env, num::NonZeroUsize, path::Path, sync::Mutex};

use lopdf::Document as LoDocument;
use lru::LruCache;
use pdf::{
    content::Op,
    file::{File as PdfFile, FileOptions, NoCache, NoLog},
    object::Resolve,
};
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
    #[error("PDF parse error (pdf-rs): {0}")]
    PdfRs(String),
}

pub struct PdfDocument {
    pub title: Option<String>,
    pub author: Option<String>,
    pub blocks: Vec<Block>,
    pub chapter_titles: Vec<String>,
    pub outlines: Vec<OutlineEntry>,
    pub truncated: bool,
}

#[derive(Clone)]
pub struct PdfSummary {
    pub title: Option<String>,
    pub author: Option<String>,
    pub page_count: usize,
}

struct PdfRsBackend {
    file: PdfRsFile,
    summary: PdfSummary,
}

type PdfRsFile = PdfFile<Vec<u8>, NoCache, NoCache, NoLog>;

struct LopdfBackend {
    doc: LoDocument,
    pages: Vec<u32>,
    summary: PdfSummary,
}

impl PdfBackendKind {
    pub fn from_env() -> Self {
        match env::var("LIBRARIAN_PDF_BACKEND")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "lopdf" => PdfBackendKind::Lopdf,
            "pdf" | "pdf-rs" => PdfBackendKind::PdfRs,
            _ => PdfBackendKind::PdfRs,
        }
    }
}

impl PdfRsBackend {
    fn open(path: &Path) -> Result<Self, PdfError> {
        let file: PdfRsFile = FileOptions::uncached()
            .open(path)
            .map_err(|e| PdfError::PdfRs(e.to_string()))?;
        if file.trailer.encrypt_dict.is_some() {
            return Err(PdfError::Encrypted);
        }
        let page_count = file.num_pages() as usize;
        if page_count == 0 {
            return Err(PdfError::Empty);
        }
        let (title, author) = pdf_rs_metadata(&file);
        Ok(Self {
            file,
            summary: PdfSummary {
                title,
                author,
                page_count,
            },
        })
    }

    fn load_page(&self, page_index: usize) -> Result<Vec<Block>, PdfError> {
        if page_index >= self.summary.page_count {
            return Err(PdfError::InvalidPage(page_index));
        }
        let page = self
            .file
            .get_page(page_index as u32)
            .map_err(|e| PdfError::PdfRs(e.to_string()))?;
        let mut text = String::new();
        if let Some(content) = &page.contents {
            let resolver = self.file.resolver();
            let ops = content
                .operations(&resolver)
                .map_err(|e| PdfError::PdfRs(e.to_string()))?;
            text = ops_to_text(&ops);
        }
        let mut blocks = page_text_to_blocks(&text);
        if blocks.is_empty() {
            blocks.push(Block::Paragraph("[empty page]".into()));
        }
        Ok(blocks)
    }

    fn outlines(&self) -> Result<Vec<OutlineEntry>, PdfError> {
        let mut out = Vec::new();
        let catalog = &self.file.trailer.root;
        if let Some(outlines) = &catalog.outlines {
            let resolver = self.file.resolver();
            if let Some(first) = &outlines.first {
                self.walk_outline_item(first, 0, &resolver, &mut out)?;
            }
        }
        Ok(out)
    }

    fn walk_outline_item(
        &self,
        item_ref: &pdf::object::Ref<pdf::object::OutlineItem>,
        depth: usize,
        resolver: &impl Resolve,
        out: &mut Vec<OutlineEntry>,
    ) -> Result<(), PdfError> {
        let item = resolver
            .get(*item_ref)
            .map_err(|e| PdfError::PdfRs(e.to_string()))?;
        if let Some(title) = &item.title {
            if let Some(dest_page) = outline_dest_to_page(resolver, &self.file, &item.dest) {
                out.push(OutlineEntry {
                    title: title.to_string_lossy(),
                    page_index: dest_page,
                });
            }
        }
        if let Some(first) = &item.first {
            self.walk_outline_item(first, depth + 1, resolver, out)?;
        }
        if let Some(next) = &item.next {
            self.walk_outline_item(next, depth, resolver, out)?;
        }
        Ok(())
    }
}

impl LopdfBackend {
    fn open(path: &Path) -> Result<Self, PdfError> {
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
        })
    }

    fn load_page(&self, page_index: usize) -> Result<Vec<Block>, PdfError> {
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

pub struct PdfLoader {
    backend: Backend,
    summary: PdfSummary,
    cache: Mutex<LruCache<usize, Vec<Block>>>,
}

#[derive(Clone, Copy)]
pub enum PdfBackendKind {
    PdfRs,
    Lopdf,
}

enum Backend {
    PdfRs(PdfRsBackend),
    Lopdf(LopdfBackend),
}

#[derive(Clone)]
pub struct OutlineEntry {
    pub title: String,
    pub page_index: usize,
}

impl PdfLoader {
    pub fn open(path: &Path) -> Result<Self, PdfError> {
        let backend = PdfBackendKind::from_env();
        Self::open_with_backend(path, backend)
    }

    pub fn open_with_backend(path: &Path, backend: PdfBackendKind) -> Result<Self, PdfError> {
        match backend {
            PdfBackendKind::PdfRs => {
                let backend = PdfRsBackend::open(path)?;
                Ok(Self {
                    summary: backend.summary.clone(),
                    backend: Backend::PdfRs(backend),
                    cache: Mutex::new(LruCache::new(NonZeroUsize::new(8).unwrap())),
                })
            }
            PdfBackendKind::Lopdf => {
                let backend = LopdfBackend::open(path)?;
                Ok(Self {
                    summary: backend.summary.clone(),
                    backend: Backend::Lopdf(backend),
                    cache: Mutex::new(LruCache::new(NonZeroUsize::new(8).unwrap())),
                })
            }
        }
    }

    pub fn summary(&self) -> &PdfSummary {
        &self.summary
    }

    pub fn page_count(&self) -> usize {
        match &self.backend {
            Backend::PdfRs(b) => b.summary.page_count,
            Backend::Lopdf(b) => b.pages.len(),
        }
    }

    pub fn outlines(&self) -> Result<Vec<OutlineEntry>, PdfError> {
        match &self.backend {
            Backend::PdfRs(b) => b.outlines(),
            Backend::Lopdf(_) => Ok(Vec::new()),
        }
    }

    pub fn load_page(&self, page_index: usize) -> Result<Vec<Block>, PdfError> {
        if let Some(cached) = self.cache.lock().unwrap().get(&page_index).cloned() {
            return Ok(cached);
        }
        let blocks = match &self.backend {
            Backend::PdfRs(b) => b.load_page(page_index)?,
            Backend::Lopdf(b) => b.load_page(page_index)?,
        };
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
    let loader = PdfLoader::open_with_backend(path, backend)?;
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
        title: loader.summary.title.clone(),
        author: loader.summary.author.clone(),
        blocks,
        chapter_titles,
        outlines: loader.outlines().unwrap_or_default(),
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

fn pdf_rs_metadata(doc: &PdfRsFile) -> (Option<String>, Option<String>) {
    let title = doc
        .trailer
        .info_dict
        .as_ref()
        .and_then(|info| info.title.as_ref().map(|s| s.to_string_lossy()));
    let author = doc
        .trailer
        .info_dict
        .as_ref()
        .and_then(|info| info.author.as_ref().map(|s| s.to_string_lossy()));
    (title, author)
}

fn ops_to_text(ops: &[Op]) -> String {
    let mut out = String::new();
    for op in ops {
        match op {
            Op::TextDraw { text } => {
                out.push_str(&text.to_string_lossy());
                out.push(' ');
            }
            Op::TextDrawAdjusted { array } => {
                for item in array {
                    match item {
                        pdf::content::TextDrawAdjusted::Text(t) => {
                            out.push_str(&t.to_string_lossy());
                        }
                        pdf::content::TextDrawAdjusted::Spacing(v) => {
                            if *v < -50.0 {
                                out.push(' ');
                            }
                        }
                    }
                }
                out.push(' ');
            }
            Op::TextNewline | Op::MoveTextPosition { .. } => {
                out.push('\n');
            }
            _ => {}
        }
    }
    out
}

fn page_title_from_blocks(blocks: &[Block]) -> Option<String> {
    blocks.iter().find_map(|b| match b {
        Block::Paragraph(t) => {
            let trimmed = t.trim();
            if trimmed.is_empty() {
                return None;
            }
            let len = trimmed.chars().count();
            if len >= 6 && len <= 80 {
                Some(trimmed.to_string())
            } else {
                None
            }
        }
        Block::Heading(t, _) => Some(t.trim().to_string()),
        _ => None,
    })
}

fn outline_dest_to_page(
    resolver: &impl Resolve,
    file: &PdfRsFile,
    dest: &Option<pdf::primitive::Primitive>,
) -> Option<usize> {
    let dest = dest.as_ref()?;
    let resolved = dest.clone().resolve(resolver).ok()?;
    if let Ok(arr) = resolved.as_array() {
        if let Some(page_ref) = arr.get(0) {
            if let Ok(page_ref) = page_ref.clone().into_reference() {
                for (idx, page) in file.pages().enumerate() {
                    if let Ok(page) = page {
                        if page.get_ref().get_inner() == page_ref {
                            return Some(idx);
                        }
                    }
                }
            }
        }
    }
    None
}
