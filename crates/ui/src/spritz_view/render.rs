use ratatui::{prelude::*, widgets::*};
use unicode_segmentation::UnicodeSegmentation;

use super::SpritzView;

impl SpritzView {
    pub fn render(&self, f: &mut Frame<'_>, area: Rect, column_width: u16) {
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
            height: content_area.height.saturating_sub(2),
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(centered);

        let header_area = chunks[0];
        let word_area = chunks[1];
        let progress_area = chunks[2];
        let status_area = chunks[3];

        self.render_header(f, header_area);
        self.render_word(f, word_area);
        self.render_progress(f, progress_area);
        self.render_status(f, status_area);
    }

    fn render_header(&self, f: &mut Frame<'_>, area: Rect) {
        let chapter_title = self
            .current_chapter()
            .cloned()
            .unwrap_or_else(|| "Unknown Chapter".to_string());

        let header = Paragraph::new(Line::styled(
            chapter_title,
            Style::default()
                .fg(self.theme.header_fg)
                .bg(self.theme.header_bg),
        ))
        .bg(self.theme.header_bg);
        f.render_widget(header, area);
    }

    fn render_word(&self, f: &mut Frame<'_>, area: Rect) {
        let word = match self.current_word() {
            Some(w) => &w.text,
            None => return,
        };

        let orp = Self::get_orp_position(word);
        let chars: Vec<&str> = word.graphemes(true).collect();

        let mut line = Line::default();

        for (i, &c) in chars.iter().enumerate() {
            if i < orp {
                line.push_span(Span::styled(
                    c,
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::DIM),
                ));
            } else if i == orp {
                line.push_span(Span::styled(
                    c,
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ));
            } else {
                line.push_span(Span::styled(c, Style::default().fg(Color::Reset)));
            }
        }

        let paragraph = Paragraph::new(line)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false });

        f.render_widget(Clear, area);

        let orp_target_x = area.x + area.width / 2;
        let orp_target_y = area.y + (area.height as f32 * 0.35) as u16;
        let word_start_x = orp_target_x.saturating_sub(orp as u16);
        let word_height = 1;

        let word_area = Rect {
            x: word_start_x,
            y: orp_target_y.saturating_sub(word_height),
            width: area.width.saturating_sub(word_start_x - area.x),
            height: word_height,
        };

        f.render_widget(paragraph, word_area);
    }

    fn render_progress(&self, f: &mut Frame<'_>, area: Rect) {
        let progress = if self.words.is_empty() {
            0.0
        } else {
            (self.current_index + 1) as f32 / self.words.len() as f32
        };

        let bar_width = area.width.saturating_sub(2) as usize;
        let filled = (bar_width as f32 * progress).round() as usize;
        let empty = bar_width.saturating_sub(filled);

        let filled_bar = "▮".repeat(filled);
        let empty_bar = "▯".repeat(empty);
        let percentage = (progress * 100.0) as usize;

        let progress_line = Line::from(vec![
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled(filled_bar, Style::default().fg(Color::Blue)),
            Span::styled(empty_bar, Style::default().fg(Color::DarkGray)),
            Span::styled("]", Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled(
                format!("{}%", percentage),
                Style::default().fg(self.theme.footer_fg),
            ),
        ]);

        let paragraph = Paragraph::new(progress_line)
            .bg(self.theme.footer_pad_bg)
            .alignment(Alignment::Center);
        f.render_widget(paragraph, area);
    }

    fn render_status(&self, f: &mut Frame<'_>, area: Rect) {
        let word_display = if self.words.is_empty() {
            "0/0".to_string()
        } else {
            format!("{}/{}", self.current_index + 1, self.words.len())
        };

        let status_icon = if self.is_playing { "▶" } else { "⏸" };
        let status_text = if self.is_playing { "Playing" } else { "Paused" };

        let status_line = Line::from(vec![
            Span::styled(
                format!("{} WPM  ", self.wpm),
                Style::default().fg(self.theme.footer_fg),
            ),
            Span::styled(
                format!("Word {}  ", word_display),
                Style::default().fg(self.theme.footer_fg),
            ),
            Span::styled(
                format!("{} {}", status_icon, status_text),
                Style::default()
                    .fg(if self.is_playing {
                        Color::Green
                    } else {
                        Color::Yellow
                    })
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        let paragraph = Paragraph::new(status_line)
            .bg(self.theme.footer_bg)
            .alignment(Alignment::Center);
        f.render_widget(paragraph, area);
    }
}
