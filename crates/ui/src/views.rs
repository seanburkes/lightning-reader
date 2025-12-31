use ratatui::{prelude::*, widgets::*};
use unicode_segmentation::UnicodeSegmentation;

use crate::layout::centered_rect;

pub struct TocView {
    pub items: Vec<TocItem>,
    pub selected: usize,
}

pub struct TocItem {
    pub label: String,
    pub level: usize,
    pub page: Option<usize>,
    pub href: Option<String>,
}

impl TocView {
    pub fn new(items: Vec<TocItem>) -> Self {
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

    pub fn current_page(&self) -> Option<usize> {
        self.items.get(self.selected).and_then(|item| item.page)
    }

    pub fn current_item(&self) -> Option<&TocItem> {
        self.items.get(self.selected)
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
            .map(|(i, item)| {
                let style = if i == self.selected {
                    Style::default().bg(Color::Blue).fg(Color::White)
                } else {
                    Style::default()
                };
                let indent = "  ".repeat(item.level.min(6));
                let mut label = format!("{}{}", indent, item.label);
                let page_text = item.page.map(|p| format!("p{}", p + 1));
                label = format_toc_line(&label, page_text.as_deref(), max_w);
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

pub struct FootnoteView {
    pub text: String,
}

impl FootnoteView {
    pub fn new(text: String) -> Self {
        Self { text }
    }

    pub fn render(&self, f: &mut Frame<'_>, area: Rect) {
        let popup_area = centered_rect(70, 50, area);
        let block = Block::default()
            .title("Footnote (Esc to close)")
            .borders(Borders::ALL);
        let body = Paragraph::new(self.text.clone())
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(Clear, popup_area);
        f.render_widget(body, popup_area);
    }
}

fn format_toc_line(label: &str, page_text: Option<&str>, max_w: usize) -> String {
    if max_w == 0 {
        return String::new();
    }
    let page_text = page_text.unwrap_or("");
    let needs_page = !page_text.is_empty();
    let page_len = page_text.graphemes(true).count();
    let space = if needs_page { 1 } else { 0 };
    let max_label = max_w.saturating_sub(page_len + space);
    let mut trimmed = truncate_with_ellipsis(label, max_label);
    if needs_page {
        if !trimmed.is_empty() {
            trimmed.push(' ');
        }
        trimmed.push_str(page_text);
    }
    trimmed
}

fn truncate_with_ellipsis(text: &str, max_w: usize) -> String {
    if max_w == 0 {
        return String::new();
    }
    let gs: Vec<&str> = text.graphemes(true).collect();
    if gs.len() <= max_w {
        return text.to_string();
    }
    if max_w == 1 {
        return "…".to_string();
    }
    let keep = max_w.saturating_sub(1);
    format!("{}…", gs[..keep].concat())
}
