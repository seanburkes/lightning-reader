use std::collections::HashMap;

use reader_core::layout::Page;
use reader_core::pdf::OutlineEntry;

#[cfg(feature = "kitty-images")]
use super::images::{KittyImage, RenderImage};
use super::SelectionRange;
use super::Theme;

pub struct ReaderView {
    pub pages: Vec<Page>,
    pub current: usize,
    pub last_key: Option<String>,
    pub justify: bool,
    pub two_pane: bool,
    pub chapter_starts: Vec<usize>,
    pub chapter_titles: Vec<String>,
    pub chapter_hrefs: Vec<String>,
    pub anchors: HashMap<String, usize>,
    pub book_title: Option<String>,
    pub author: Option<String>,
    pub theme: Theme,
    pub total_pages: Option<usize>,
    pub total_chapters: Option<usize>,
    pub toc_overrides: Vec<OutlineEntry>,
    pub selection: Option<SelectionRange>,
    pub image_map: HashMap<String, Vec<u8>>,
    #[cfg(feature = "kitty-images")]
    pub(super) image_cache: HashMap<String, KittyImage>,
    #[cfg(feature = "kitty-images")]
    pub(super) image_placements: Vec<RenderImage>,
}

impl Default for ReaderView {
    fn default() -> Self {
        Self::new()
    }
}

impl ReaderView {
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            current: 0,
            last_key: None,
            justify: false,
            two_pane: false,
            chapter_starts: Vec::new(),
            chapter_titles: Vec::new(),
            chapter_hrefs: Vec::new(),
            anchors: HashMap::new(),
            book_title: None,
            author: None,
            theme: Theme::default(),
            total_pages: None,
            total_chapters: None,
            toc_overrides: Vec::new(),
            selection: None,
            image_map: HashMap::new(),
            #[cfg(feature = "kitty-images")]
            image_cache: HashMap::new(),
            #[cfg(feature = "kitty-images")]
            image_placements: Vec::new(),
        }
    }
}
