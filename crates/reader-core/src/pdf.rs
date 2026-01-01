use std::{env, num::NonZeroUsize, path::Path};

use lopdf::Document as LoDocument;
use lru::LruCache;
use pdf::{
    content::Op,
    file::{File as PdfFile, FileOptions, NoCache, NoLog},
    object::Resolve,
    primitive::Primitive as PdfPrimitive,
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
        let resolver = self.file.resolver();
        let links = extract_links(&page, &resolver);
        if !links.is_empty() {
            blocks.push(Block::Paragraph(String::new()));
            for l in links {
                blocks.push(Block::Paragraph(l));
            }
        }
        let images = extract_images(&page, &resolver);
        if !images.is_empty() {
            for img in images {
                blocks.push(Block::Paragraph(img));
            }
        }
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
        _depth: usize,
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
            self.walk_outline_item(first, _depth + 1, resolver, out)?;
        }
        if let Some(next) = &item.next {
            self.walk_outline_item(next, _depth, resolver, out)?;
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
    cache: LruCache<usize, Vec<Block>>,
}

const CACHE_PAGES: usize = 8;

fn cache_capacity() -> NonZeroUsize {
    NonZeroUsize::new(CACHE_PAGES).unwrap_or(NonZeroUsize::MIN)
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
                    cache: LruCache::new(cache_capacity()),
                })
            }
            PdfBackendKind::Lopdf => {
                let backend = LopdfBackend::open(path)?;
                Ok(Self {
                    summary: backend.summary.clone(),
                    backend: Backend::Lopdf(backend),
                    cache: LruCache::new(cache_capacity()),
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

    pub fn load_page(&mut self, page_index: usize) -> Result<Vec<Block>, PdfError> {
        if let Some(cached) = self.cache.get(&page_index).cloned() {
            return Ok(cached);
        }
        let blocks = match &self.backend {
            Backend::PdfRs(b) => b.load_page(page_index)?,
            Backend::Lopdf(b) => b.load_page(page_index)?,
        };
        self.cache.put(page_index, blocks.clone());
        Ok(blocks)
    }

    pub fn load_range(
        &mut self,
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
    let mut lines: Vec<String> = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim_end();
        if line.trim().is_empty() {
            flush_lines(&mut lines, &mut out);
            continue;
        }
        lines.push(line.to_string());
    }
    flush_lines(&mut lines, &mut out);
    out
}

fn flush_lines(lines: &mut Vec<String>, out: &mut Vec<Block>) {
    if lines.is_empty() {
        return;
    }
    if is_monospace_like(lines) {
        let text = lines.join("\n");
        out.push(Block::Code { lang: None, text });
        lines.clear();
        return;
    }
    let para = lines_to_paragraph(lines);
    let cleaned = para.trim();
    if !cleaned.is_empty() {
        out.push(Block::Paragraph(annotate_links(cleaned)));
    }
    lines.clear();
}

fn lines_to_paragraph(lines: &[String]) -> String {
    let mut current = String::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
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
    current
}

fn is_monospace_like(lines: &[String]) -> bool {
    if lines.len() < 2 {
        return false;
    }
    let avg_len: f32 = lines.iter().map(|l| l.len() as f32).sum::<f32>() / lines.len() as f32;
    let variance: f32 = lines
        .iter()
        .map(|l| {
            let diff = l.len() as f32 - avg_len;
            diff * diff
        })
        .sum::<f32>()
        / lines.len() as f32;
    let spaced = lines.iter().filter(|l| l.contains("  ")).count();
    variance < 16.0 && spaced as f32 / lines.len() as f32 > 0.4
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

fn annotate_links(s: &str) -> String {
    s.split_whitespace()
        .map(|tok| {
            let lower = tok.to_ascii_lowercase();
            let is_url = lower.starts_with("http://")
                || lower.starts_with("https://")
                || lower.starts_with("www.");
            let is_anchor = tok.starts_with('#');
            if is_url {
                format!("{} [link:{}]", tok, tok)
            } else if is_anchor {
                format!("{} [anchor:{}]", tok, tok.trim_start_matches('#'))
            } else {
                tok.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
            if (6..=80).contains(&len) {
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
        if let Some(page_ref) = arr.first() {
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

fn extract_links(page: &pdf::object::Page, resolver: &impl Resolve) -> Vec<String> {
    let mut links = Vec::new();
    if let Some(annots_prim) = page.other.get("Annots") {
        if let Ok(PdfPrimitive::Array(arr)) = annots_prim.clone().resolve(resolver) {
            for ann in arr {
                if let Ok(dict) = ann
                    .clone()
                    .resolve(resolver)
                    .and_then(|p| p.into_dictionary())
                {
                    let is_link = dict
                        .get("Subtype")
                        .and_then(|p| p.as_name().ok())
                        .map(|n| n.as_bytes() == b"Link")
                        .unwrap_or(false);
                    if !is_link {
                        continue;
                    }
                    if let Some(action) = dict.get("A") {
                        if let Ok(action_dict) = action
                            .clone()
                            .resolve(resolver)
                            .and_then(|p| p.into_dictionary())
                        {
                            if let Some(uri) = action_dict.get("URI").and_then(pdf_prim_to_string) {
                                links.push(format!("[link] {}", uri));
                                continue;
                            }
                        }
                    }
                    if let Some(dest) = dict.get("Dest").and_then(pdf_prim_to_string) {
                        links.push(format!("[anchor] {}", dest));
                    }
                }
            }
        }
    }
    links
}

fn pdf_prim_to_string(p: &PdfPrimitive) -> Option<String> {
    if let Ok(s) = p.as_string() {
        return Some(s.to_string_lossy());
    }
    if let Ok(name) = p.as_name() {
        return Some(String::from_utf8_lossy(name.as_bytes()).to_string());
    }
    None
}

fn extract_images(page: &pdf::object::Page, resolver: &impl Resolve) -> Vec<String> {
    let mut images = Vec::new();
    if let Ok(resources) = page.resources() {
        for (name, obj) in &resources.xobjects {
            if let Ok(xobj) = resolver.get(*obj) {
                if let pdf::object::XObject::Image(img) = &*xobj {
                    let w = img.width as i64;
                    let h = img.height as i64;
                    let label = match (w, h) {
                        (w, h) if w > 0 && h > 0 => format!(
                            "[image: {} ({}x{})]",
                            String::from_utf8_lossy(name.as_bytes()),
                            w,
                            h
                        ),
                        _ => format!("[image: {}]", String::from_utf8_lossy(name.as_bytes())),
                    };
                    images.push(label);
                }
            }
        }
    }
    images
}
