use ratatui::{prelude::*, widgets::*};
use reader_core::layout::{Page, Size};
use reader_core::types::Block as ReaderBlock;

pub struct ReaderView {
    pub pages: Vec<Page>,
    pub current: usize,
}

impl ReaderView {
    pub fn new() -> Self { Self { pages: Vec::new(), current: 0 } }

    pub fn render(&self, f: &mut Frame<'_>, area: Rect) {
        let lines: Vec<Line> = if let Some(page) = self.pages.get(self.current) {
            page.lines.iter().map(|l| Line::from(l.clone())).collect()
        } else { vec![Line::from("No content")] };
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false }).block(ratatui::widgets::Block::default().borders(Borders::ALL).title("Reader"));
        f.render_widget(paragraph, area);
    }

    pub fn up(&mut self, lines: usize) {
        let delta = lines.max(1);
        self.current = self.current.saturating_sub(delta);
    }

    pub fn down(&mut self, lines: usize) {
        let delta = lines.max(1);
        self.current = (self.current + delta).min(self.pages.len().saturating_sub(1));
    }

    pub fn reflow(&mut self, blocks: &Vec<ReaderBlock>, size: Size) {
        self.pages = reader_core::layout::paginate(blocks, size);
        self.current = self.current.min(self.pages.len().saturating_sub(1));
    }
}
