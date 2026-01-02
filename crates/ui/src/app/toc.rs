use crate::reader_view::ReaderView;
use crate::views::{TocItem, TocView};

use super::types::{ChapterPrefetchRequest, Mode};
use super::App;

impl App {
    pub(super) fn open_toc(&mut self, view: &ReaderView) {
        let items = self.build_toc_items(view);
        let mut toc = TocView::new(items);
        if let Some(idx) = toc
            .items
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, item)| item.page.filter(|p| *p <= view.current).map(|_| i))
        {
            toc.selected = idx;
        }
        self.toc = Some(toc);
        self.mode = Mode::Toc;
    }

    pub(super) fn submit_toc(&mut self, view: &mut ReaderView) {
        if let Some(toc) = &self.toc {
            if let Some(item) = toc.current_item() {
                if let Some(target) = item.page {
                    view.current = target.min(view.pages.len().saturating_sub(1));
                } else if let Some(href) = item.href.as_deref() {
                    if !view.jump_to_target(href) {
                        self.pending_chapter_jump = Some(href.to_string());
                        if let Some(tx) = &self.prefetch_chapter_tx {
                            let target_loaded = self
                                .chapter_index_for_href(href)
                                .map(|idx| idx.saturating_add(1))
                                .unwrap_or_else(|| {
                                    self.chapter_titles
                                        .len()
                                        .saturating_add(self.prefetch_chapter_window.max(1))
                                });
                            let _ = tx.send(ChapterPrefetchRequest {
                                target_loaded,
                                target_href: Some(href.to_string()),
                            });
                        }
                    }
                }
            }
        }
        self.mode = Mode::Reader;
        self.toc = None;
    }

    fn build_toc_items(&self, view: &ReaderView) -> Vec<TocItem> {
        if !self.outlines.is_empty() {
            let mut items = Vec::new();
            for entry in &self.outlines {
                items.push(TocItem {
                    label: entry.title.clone(),
                    level: 0,
                    page: Some(entry.page_index),
                    href: None,
                });
            }
            return items;
        }
        if !self.toc_entries.is_empty() {
            let mut items = Vec::new();
            for entry in &self.toc_entries {
                let page = view.page_for_href(entry.href());
                items.push(TocItem {
                    label: entry.label().to_string(),
                    level: entry.level(),
                    page,
                    href: Some(entry.href().to_string()),
                });
            }
            return items;
        }
        if view.chapter_starts.is_empty() {
            return vec![TocItem {
                label: "Start".to_string(),
                level: 0,
                page: Some(0),
                href: None,
            }];
        }
        let mut items: Vec<TocItem> = Vec::new();
        for (i, pidx) in view.chapter_starts.iter().enumerate() {
            let title = view.chapter_title(i);
            let chapter_label = format!("Chapter {}", i + 1);
            let entry = if title.is_empty() {
                chapter_label
            } else if title.to_ascii_lowercase().starts_with("chapter") {
                title
            } else {
                format!("{}: {}", chapter_label, title)
            };
            items.push(TocItem {
                label: entry,
                level: 0,
                page: Some(*pidx),
                href: None,
            });
        }
        items
    }
}
