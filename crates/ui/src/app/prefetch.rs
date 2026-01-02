use reader_core::layout::Size;
use reader_core::types::Block as ReaderBlock;

use crate::reader_view::ReaderView;

use super::types::{ChapterPrefetchRequest, PrefetchRequest};
use super::App;

impl App {
    pub(super) fn poll_incoming(&mut self, view: &mut ReaderView, inner: Size) {
        let mut added = false;
        if let Some(rx) = &self.incoming_pages {
            while let Ok(msg) = rx.try_recv() {
                view.add_images_from_blocks(&msg.blocks);
                if !self.blocks.is_empty() {
                    self.blocks.push(ReaderBlock::Paragraph(String::new()));
                    self.blocks.push(ReaderBlock::Paragraph("───".into()));
                    self.blocks.push(ReaderBlock::Paragraph(String::new()));
                }
                self.blocks.extend(msg.blocks);
                self.chapter_titles
                    .push(format!("Page {}", msg.page_index + 1));
                self.chapter_hrefs
                    .push(format!("page:{}", msg.page_index + 1));
                added = true;
            }
        }
        if added {
            view.reflow(&self.blocks, inner);
            view.chapter_titles = self.chapter_titles.clone();
            view.chapter_hrefs = self.chapter_hrefs.clone();
            view.total_pages = self.total_pages;
            view.total_chapters = self.total_chapters;
            view.selection = None;
        }
    }

    pub(super) fn poll_incoming_chapters(&mut self, view: &mut ReaderView, inner: Size) {
        let mut added = false;
        if let Some(rx) = &self.incoming_chapters {
            while let Ok(msg) = rx.try_recv() {
                view.add_images_from_blocks(&msg.blocks);
                if !self.blocks.is_empty() {
                    self.blocks.push(ReaderBlock::Paragraph(String::new()));
                    self.blocks.push(ReaderBlock::Paragraph("───".into()));
                    self.blocks.push(ReaderBlock::Paragraph(String::new()));
                }
                self.blocks.extend(msg.blocks);
                self.chapter_titles.push(msg.title);
                self.chapter_hrefs.push(msg.href);
                added = true;
            }
        }
        if added {
            view.reflow(&self.blocks, inner);
            view.chapter_titles = self.chapter_titles.clone();
            view.chapter_hrefs = self.chapter_hrefs.clone();
            view.total_pages = self.total_pages;
            view.total_chapters = self.total_chapters;
            view.selection = None;
        }
        if let Some(target) = self.pending_chapter_jump.clone() {
            if view.jump_to_target(&target) {
                self.pending_chapter_jump = None;
            }
        }
    }

    pub(super) fn maybe_request_prefetch(&mut self, view: &ReaderView) {
        let Some(tx) = &self.prefetch_tx else {
            return;
        };
        let loaded_pages = self.chapter_titles.len();
        let total = self.total_pages.unwrap_or(loaded_pages);
        if loaded_pages >= total {
            return;
        }
        let current = view.current;
        if self.last_prefetch_at == Some(current) {
            return;
        }
        self.last_prefetch_at = Some(current);
        let start = current + 1;
        if start >= total {
            return;
        }
        let _ = tx.send(PrefetchRequest {
            start,
            window: self.prefetch_window,
        });
    }

    pub(super) fn maybe_request_chapter_prefetch(&mut self, view: &ReaderView) {
        let Some(tx) = &self.prefetch_chapter_tx else {
            return;
        };
        let loaded = self.chapter_titles.len();
        if loaded == 0 {
            return;
        }
        let page_count = view.pages.len();
        if page_count == 0 {
            return;
        }
        if let Some(total) = self.total_chapters {
            if loaded >= total {
                return;
            }
        }
        let current = view.current;
        if self.last_chapter_prefetch_at == Some(current) {
            return;
        }
        let remaining = page_count.saturating_sub(current + 1);
        if remaining > 2 {
            return;
        }
        self.last_chapter_prefetch_at = Some(current);
        let target_loaded = loaded.saturating_add(self.prefetch_chapter_window.max(1));
        let _ = tx.send(ChapterPrefetchRequest {
            target_loaded,
            target_href: None,
        });
    }

    pub(super) fn chapter_index_for_href(&self, href: &str) -> Option<usize> {
        if let Some(idx) = self.chapter_index_by_href.get(href) {
            return Some(*idx);
        }
        let stripped = strip_fragment(href);
        self.chapter_index_by_href.get(stripped).copied()
    }
}

fn strip_fragment(href: &str) -> &str {
    href.split('#').next().unwrap_or(href)
}
