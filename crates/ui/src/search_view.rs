use ratatui::{prelude::*, widgets::*};
use unicode_segmentation::UnicodeSegmentation;

pub struct SearchView {
    pub query: String,
}

impl SearchView {
    pub fn new() -> Self {
        Self {
            query: String::new(),
        }
    }

    pub fn with_query(query: &str) -> Self {
        Self {
            query: query.to_string(),
        }
    }

    pub fn push_char(&mut self, c: char) {
        if !c.is_control() {
            self.query.push(c);
        }
    }

    pub fn backspace(&mut self) {
        if let Some((idx, _)) = self.query.grapheme_indices(true).last() {
            self.query.truncate(idx);
        }
    }

    pub fn render(&self, f: &mut Frame<'_>, area: Rect) {
        let mut width = ((area.width as f32) * 0.5) as u16;
        width = width.max(20).min(area.width.saturating_sub(2).max(1)); // keep borders visible
        let height: u16 = 3;
        let popup_area = Rect {
            x: area.x + (area.width.saturating_sub(width)) / 2,
            y: area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        };

        let block = Block::default()
            .title("Search (Enter submit, Esc cancel)")
            .borders(Borders::ALL);
        let prompt = Paragraph::new(format!("> {}", self.query)).block(block);
        f.render_widget(Clear, popup_area);
        f.render_widget(prompt, popup_area);
    }
}
