use reader_core::layout::WordToken;
use std::time::Instant;
use unicode_segmentation::UnicodeSegmentation;

use crate::app::SpritzSettings;

pub struct SpritzView {
    words: Vec<WordToken>,
    pub current_index: usize,
    pub wpm: u16,
    pub is_playing: bool,
    last_update: Instant,
    settings: SpritzSettings,
    pub chapter_titles: Vec<String>,
}

impl SpritzView {
    pub fn new(
        words: Vec<WordToken>,
        settings: SpritzSettings,
        chapter_titles: Vec<String>,
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
        }
    }

    pub fn play(&mut self) {
        self.is_playing = true;
        self.last_update = Instant::now();
    }

    pub fn pause(&mut self) {
        self.is_playing = false;
    }

    pub fn toggle_play(&mut self) {
        if self.is_playing {
            self.pause();
        } else {
            self.play();
        }
    }

    pub fn rewind(&mut self, steps: usize) {
        self.current_index = self.current_index.saturating_sub(steps);
        self.last_update = Instant::now();
    }

    pub fn fast_forward(&mut self, steps: usize) {
        if !self.words.is_empty() {
            self.current_index = (self.current_index + steps).min(self.words.len() - 1);
        }
        self.last_update = Instant::now();
    }

    pub fn adjust_wpm(&mut self, delta: i16) {
        let new_wpm = self.wpm as i16 + delta;
        self.wpm = new_wpm.clamp(100, 1000) as u16;
    }

    pub fn get_orp_position(word: &str) -> usize {
        let len = word.graphemes(true).count();

        if len <= 1 {
            0
        } else if len <= 13 {
            (len as f32 * 0.35).round() as usize
        } else {
            (len as f32 * 0.22).round() as usize
        }
    }

    pub fn update(&mut self) -> bool {
        if !self.is_playing || self.words.is_empty() {
            return false;
        }

        let word = match self.words.get(self.current_index) {
            Some(w) => w,
            None => return false,
        };

        let base_delay_ms = 60000.0 / self.wpm as f64;
        let mut delay_ms = base_delay_ms;

        if self.settings.pause_on_punct {
            if word.is_sentence_end {
                delay_ms += self.settings.punct_pause_ms as f64 * 2.0;
            } else if word.is_comma {
                delay_ms += self.settings.punct_pause_ms as f64;
            }
        }

        let elapsed = self.last_update.elapsed().as_millis() as f64;
        if elapsed >= delay_ms {
            if self.current_index < self.words.len() - 1 {
                self.current_index += 1;
                self.last_update = Instant::now();
                return true;
            } else {
                self.pause();
            }
        }

        false
    }

    pub fn current_word(&self) -> Option<&WordToken> {
        self.words.get(self.current_index)
    }

    pub fn word_count(&self) -> usize {
        self.words.len()
    }

    pub fn current_chapter(&self) -> Option<&String> {
        if let Some(word) = self.current_word() {
            if let Some(idx) = word.chapter_index {
                return self.chapter_titles.get(idx);
            }
        }
        None
    }

    pub fn jump_to_chapter_start(&mut self) {
        if let Some(word) = self.current_word() {
            if let Some(idx) = word.chapter_index {
                for (i, w) in self.words.iter().enumerate() {
                    if w.chapter_index == Some(idx) {
                        self.current_index = i;
                        self.last_update = Instant::now();
                        return;
                    }
                }
            }
        }
        self.current_index = 0;
        self.last_update = Instant::now();
    }

    pub fn jump_to_chapter_end(&mut self) {
        if let Some(word) = self.current_word() {
            if let Some(current_chapter_idx) = word.chapter_index {
                for (i, w) in self.words.iter().enumerate().rev() {
                    if w.chapter_index == Some(current_chapter_idx) {
                        self.current_index = i;
                        self.last_update = Instant::now();
                        return;
                    }
                }
            }
        }
        if !self.words.is_empty() {
            self.current_index = self.words.len() - 1;
            self.last_update = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_settings() -> SpritzSettings {
        SpritzSettings {
            wpm: 250,
            pause_on_punct: true,
            punct_pause_ms: 100,
        }
    }

    #[test]
    fn orp_position_short_word() {
        assert_eq!(SpritzView::get_orp_position("a"), 0);
        assert_eq!(SpritzView::get_orp_position("hi"), 1);
        assert_eq!(SpritzView::get_orp_position("cat"), 1);
    }

    #[test]
    fn orp_position_medium_word() {
        assert_eq!(SpritzView::get_orp_position("hello"), 2);
        assert_eq!(SpritzView::get_orp_position("reading"), 2);
    }

    #[test]
    fn orp_position_long_word() {
        assert_eq!(SpritzView::get_orp_position("extraordinary"), 5);
    }

    #[test]
    fn adjust_wpm_clamps_to_range() {
        let words = vec![];
        let settings = dummy_settings();
        let mut view = SpritzView::new(words, settings, vec![]);
        view.wpm = 100;

        view.adjust_wpm(-200);
        assert_eq!(view.wpm, 100);

        view.wpm = 1000;
        view.adjust_wpm(200);
        assert_eq!(view.wpm, 1000);
    }

    #[test]
    fn rewind_saturates_at_zero() {
        let words = vec![WordToken {
            text: "test".to_string(),
            is_sentence_end: false,
            is_comma: false,
            chapter_index: None,
        }];
        let settings = dummy_settings();
        let mut view = SpritzView::new(words, settings, vec![]);
        view.current_index = 0;

        view.rewind(10);
        assert_eq!(view.current_index, 0);
    }

    #[test]
    fn fast_forward_clamps_to_end() {
        let words = vec![
            WordToken {
                text: "one".to_string(),
                is_sentence_end: false,
                is_comma: false,
                chapter_index: None,
            },
            WordToken {
                text: "two".to_string(),
                is_sentence_end: false,
                is_comma: false,
                chapter_index: None,
            },
        ];
        let settings = dummy_settings();
        let mut view = SpritzView::new(words, settings, vec![]);
        view.current_index = 0;

        view.fast_forward(10);
        assert_eq!(view.current_index, 1);
    }

    #[test]
    fn toggle_play_switches_state() {
        let words = vec![];
        let settings = dummy_settings();
        let mut view = SpritzView::new(words, settings, vec![]);

        assert!(!view.is_playing);
        view.toggle_play();
        assert!(view.is_playing);
        view.toggle_play();
        assert!(!view.is_playing);
    }
}
