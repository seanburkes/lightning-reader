use chrono::Utc;

use crate::spritz_view::SpritzView;

use super::types::{Mode, SpritzSettings};
use super::App;

impl App {
    pub(super) fn save_spritz_session(&self, spritz: &SpritzView) {
        let Some(book_id) = &self.book_id else {
            return;
        };
        let session = reader_core::SpritzSession::new(
            book_id.clone(),
            spritz.current_index,
            spritz.wpm,
            Utc::now().to_rfc3339(),
        );
        let _ = reader_core::save_spritz_session(&session);
    }

    pub(super) fn start_spritz(&mut self) {
        let words = reader_core::layout::extract_words(&self.blocks);
        let settings = SpritzSettings::default();
        let mut spritz = SpritzView::new(
            words,
            settings,
            self.chapter_titles.clone(),
            self.theme.clone(),
        );

        if let Some(book_id) = &self.book_id {
            if let Some(session) = reader_core::load_spritz_session(book_id) {
                spritz.current_index = session.word_index();
                spritz.wpm = session.wpm();
            }
        }

        self.spritz = Some(spritz);
        self.mode = Mode::Spritz;
    }

    pub(super) fn stop_spritz(&mut self) {
        if let Some(spritz) = &self.spritz {
            self.save_spritz_session(spritz);
        }
        self.spritz = None;
        self.mode = Mode::Reader;
    }
}
