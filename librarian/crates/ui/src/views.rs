use ratatui::{prelude::*, widgets::*};
use unicode_segmentation::UnicodeSegmentation;

pub struct TocView {
    pub items: Vec<String>,
    pub selected: usize,
}

impl TocView {
    pub fn new(items: Vec<String>) -> Self {
        Self { items, selected: 0 }
    }

    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
    pub fn down(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + 1).min(self.items.len() - 1);
        }
    }

    pub fn render(&self, f: &mut Frame<'_>, area: Rect, column_width: u16) {
        // Centered TOC view with same reader width
        let vchunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
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

        let block = Block::default()
            .borders(Borders::ALL)
            .title("Table of Contents (j/k, Enter, Esc)");
        let max_w = centered.width as usize - 2; // borders
        let items: Vec<ListItem> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let style = if i == self.selected {
                    Style::default().bg(Color::Blue).fg(Color::White)
                } else {
                    Style::default()
                };
                let mut label = s.clone();
                if label.graphemes(true).count() > max_w {
                    // Keep last graphemes to show differentiating numbers if any
                    let keep = max_w.saturating_sub(1);
                    let gs: Vec<&str> = label.graphemes(true).collect();
                    let start = gs.len().saturating_sub(keep);
                    label = format!("…{}", gs[start..].concat());
                }
                ListItem::new(Line::from(label)).style(style)
            })
            .collect();
        let list = List::new(items).block(block);
        f.render_widget(Clear, centered);
        f.render_widget(list, centered);

        // Status bar (reuse lower chunk)
        let status = Paragraph::new(Line::from("TOC: Enter to jump, Esc to return"));
        f.render_widget(status, vchunks[1]);
    }
}
