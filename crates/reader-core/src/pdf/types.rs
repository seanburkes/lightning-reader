use std::env;

use crate::types::Block;

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

#[derive(Clone)]
pub struct OutlineEntry {
    pub title: String,
    pub page_index: usize,
}

#[derive(Clone, Copy)]
pub enum PdfBackendKind {
    PdfRs,
    Lopdf,
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
