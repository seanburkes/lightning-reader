use std::{num::NonZeroUsize, path::Path};

use lru::LruCache;

use crate::types::Block;

use super::error::PdfError;
use super::lopdf::LopdfBackend;
use super::pdf_rs::PdfRsBackend;
use super::types::{OutlineEntry, PdfBackendKind, PdfSummary};

enum Backend {
    PdfRs(PdfRsBackend),
    Lopdf(LopdfBackend),
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
