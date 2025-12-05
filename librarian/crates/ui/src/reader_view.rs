use ratatui::{prelude::*, widgets::*};
use reader_core::layout::{Page, Size};
use reader_core::types::Block as ReaderBlock;

pub struct ReaderView {
    pub pages: Vec<Page>,
    pub current: usize,
    pub last_key: Option<String>,
    pub justify: bool,
}

impl ReaderView {
    pub fn new() -> Self { Self { pages: Vec::new(), current: 0, last_key: None, justify: false } }

    pub fn inner_size(area: Rect, column_width: u16) -> Size {
        let vchunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(area);
        let content_area = vchunks[0];
        let col_w = column_width.min(content_area.width);
        let inner_w = col_w.saturating_sub(2); // borders left/right
        let inner_h = content_area.height.saturating_sub(2); // borders top/bottom
        Size { width: inner_w, height: inner_h }
    }

    pub fn render(&self, f: &mut Frame<'_>, area: Rect, column_width: u16) {
        let vchunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(area);

        let content_area = vchunks[0];
        let col_w = column_width.min(content_area.width);
        let left_pad = content_area.width.saturating_sub(col_w) / 2;
        let centered = Rect {
            x: content_area.x + left_pad,
            y: content_area.y,
            width: col_w,
            height: content_area.height,
        };

        let lines: Vec<Line> = if let Some(page) = self.pages.get(self.current) {
            page.lines.iter().map(|l| Line::from(l.clone())).collect()
        } else { vec![Line::from("No content")] };
        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(ratatui::widgets::Block::default().borders(Borders::ALL).title("Reader"));
        f.render_widget(paragraph, centered);

        let status_text = {
            let total = self.pages.len();
            let current = if total == 0 { 0 } else { self.current + 1 };
            let key = self.last_key.as_deref().unwrap_or("—");
            format!("Pg {}/{}  |  Key: {}", current, total, key)
        };
        let status = Paragraph::new(Line::from(status_text));
        f.render_widget(status, vchunks[1]);
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
        self.pages = reader_core::layout::paginate_with_justify(blocks, size, self.justify);
        self.current = self.current.min(self.pages.len().saturating_sub(1));
    }
}
