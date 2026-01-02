use std::time::Instant;

use reader_core::layout::WordToken;

use crate::app::SpritzSettings;
use crate::reader_view::Theme;

pub struct SpritzView {
    pub(super) words: Vec<WordToken>,
    pub current_index: usize,
    pub wpm: u16,
    pub is_playing: bool,
    pub(super) last_update: Instant,
    pub(super) settings: SpritzSettings,
    pub chapter_titles: Vec<String>,
    pub theme: Theme,
}

impl SpritzView {
    pub fn new(
        words: Vec<WordToken>,
        settings: SpritzSettings,
        chapter_titles: Vec<String>,
        theme: Theme,
    ) -> Self {
        let wpm = settings.wpm;
        Self {
            words,
            current_index: 0,
            wpm,
            is_playing: false,
            last_update: Instant::now(),
            settings,
            chapter_titles,
            theme,
        }
    }
}
