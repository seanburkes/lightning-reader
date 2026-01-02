use std::path::Path;

use pdf::{
    content::Op,
    file::{File as PdfFile, FileOptions, NoCache, NoLog},
    object::Resolve,
    primitive::Primitive as PdfPrimitive,
};

use crate::types::Block;

use super::error::PdfError;
use super::text::page_text_to_blocks;
use super::types::{OutlineEntry, PdfSummary};

type PdfRsFile = PdfFile<Vec<u8>, NoCache, NoCache, NoLog>;

pub(super) struct PdfRsBackend {
    file: PdfRsFile,
    pub(super) summary: PdfSummary,
}

impl PdfRsBackend {
    pub(super) fn open(path: &Path) -> Result<Self, PdfError> {
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

    pub(super) fn load_page(&self, page_index: usize) -> Result<Vec<Block>, PdfError> {
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

    pub(super) fn outlines(&self) -> Result<Vec<OutlineEntry>, PdfError> {
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
