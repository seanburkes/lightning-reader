use std::time::Instant;

use reader_core::layout::WordToken;
use unicode_segmentation::UnicodeSegmentation;

use super::SpritzView;

impl SpritzView {
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
