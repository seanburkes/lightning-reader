use ratatui::{prelude::*, widgets::*};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone)]
pub struct Theme {
    pub header_bg: Color,
    pub header_fg: Color,
    pub header_pad_bg: Color,
    pub footer_bg: Color,
    pub footer_fg: Color,
    pub footer_pad_bg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            header_bg: Color::Blue,
            header_fg: Color::White,
            header_pad_bg: Color::DarkGray,
            footer_bg: Color::Green,
            footer_fg: Color::Black,
            footer_pad_bg: Color::DarkGray,
        }
    }
}
use reader_core::layout::{Page, Size};
use reader_core::types::Block as ReaderBlock;

pub struct ReaderView {
    pub pages: Vec<Page>,
    pub current: usize,
    pub last_key: Option<String>,
    pub justify: bool,
    pub chapter_starts: Vec<usize>,
    pub chapter_titles: Vec<String>,
    pub book_title: Option<String>,
    pub author: Option<String>,
    pub theme: Theme,
}

impl Default for ReaderView {
    fn default() -> Self {
        Self::new()
    }
}

impl ReaderView {
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            current: 0,
            last_key: None,
            justify: false,
            chapter_starts: Vec::new(),
            chapter_titles: Vec::new(),
            book_title: None,
            author: None,
            theme: Theme::default(),
        }
    }

    pub fn inner_size(area: Rect, column_width: u16) -> Size {
        let vchunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);
        let content_area = vchunks[0];
        let col_w = column_width.min(content_area.width);
        let inner_w = col_w.saturating_sub(2); // borders left/right
        let inner_h = content_area.height.saturating_sub(2); // borders top/bottom
        Size {
            width: inner_w,
            height: inner_h,
        }
    }

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
            height: content_area.height,
        };

        let lines: Vec<Line> = if let Some(page) = self.pages.get(self.current) {
            page.lines.iter().map(|l| Line::from(l.clone())).collect()
        } else {
            vec![Line::from("No content")]
        };
        // Header/footer inside centered area
        let header_footer_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(centered);

        // Header: chapter title (left) | page X/Y (right)
        let total = self.pages.len();
        let current = if total == 0 { 0 } else { self.current + 1 };
        let chapter_label = self
            .chapter_starts
            .iter()
            .rposition(|&p| p <= self.current)
            .map(|idx| {
                self.chapter_titles
                    .get(idx)
                    .cloned()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| format!("Chapter {}", idx + 1))
            })
            .unwrap_or_else(|| "".to_string());
        // Build powerline-style header segments: left chapter, right page
        let mut left = chapter_label;
        let mut right = format!("Pg {}/{}", current, total);
        let total_width = header_footer_chunks[0].width as usize;
        // Reserve one space padding around segments if present
        let mut header_line = Line::default();
        // Truncate with priority to keep right visible
        // Compute raw lengths
        let mut left_seg_len = left.graphemes(true).count();
        let mut right_seg_len = right.graphemes(true).count();
        // If both present, ensure at least one space separation
        let sep = if !left.is_empty() && !right.is_empty() {
            1
        } else {
            0
        };
        // If overflow, truncate left first
        if left_seg_len + sep + right_seg_len > total_width {
            let max_left = total_width.saturating_sub(sep + right_seg_len);
            if max_left < left_seg_len {
                if max_left > 1 {
                    // Truncate by graphemes
                    let mut acc = String::new();
                    for (used, g) in left.graphemes(true).enumerate() {
                        if used + 1 >= max_left {
                            break;
                        }
                        acc.push_str(g);
                    }
                    left = format!("{}…", acc);
                    left_seg_len = left.graphemes(true).count();
                } else {
                    left.clear();
                    left_seg_len = 0;
                }
            }
        }
        // If still overflow, truncate right (keep last chars, e.g., page numbers)
        if left_seg_len + sep + right_seg_len > total_width {
            let max_right = total_width.saturating_sub(left_seg_len + sep);
            if max_right < right_seg_len {
                if max_right > 1 {
                    // Keep last `keep` graphemes
                    let keep = max_right.saturating_sub(1);
                    let gs: Vec<&str> = right.graphemes(true).collect();
                    let start = gs.len().saturating_sub(keep);
                    right = format!("…{}", gs[start..].concat());
                    right_seg_len = right.graphemes(true).count();
                } else {
                    right.clear();
                    right_seg_len = 0;
                }
            }
        }
        // Left segment with transition separator
        if !left.is_empty() {
            // Left header uses footer colors for consistency
            header_line.push_span(Span::styled(
                left,
                Style::default()
                    .bg(self.theme.footer_bg)
                    .fg(self.theme.footer_fg),
            ));
            if right_seg_len > 0 {
                header_line.push_span(Span::styled(
                    " ",
                    Style::default().bg(self.theme.header_pad_bg),
                ));
            }
        }
        // Middle pad before right segment
        let pad = total_width.saturating_sub(
            left_seg_len
                + sep
                + right_seg_len
                + if left_seg_len > 0 && right_seg_len > 0 {
                    1
                } else {
                    0
                },
        );
        if pad > 0 {
            header_line.push_span(Span::styled(
                " ".repeat(pad),
                Style::default().bg(self.theme.header_pad_bg),
            ));
        }
        if !right.is_empty() {
            // Right header uses footer pad bg and footer fg to match right footer
            header_line.push_span(Span::styled(
                right,
                Style::default()
                    .bg(self.theme.footer_pad_bg)
                    .fg(self.theme.footer_fg),
            ));
        }
        let header = Paragraph::new(header_line);
        f.render_widget(header, header_footer_chunks[0]);

        // Content
        let para_area = header_footer_chunks[1];
        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(ratatui::widgets::Block::default().borders(Borders::ALL));
        f.render_widget(paragraph, para_area);

        // Footer: powerline segments left author, right title
        let mut author = self.author.clone().unwrap_or_default();
        let mut title = self.book_title.clone().unwrap_or_default();
        let total_width = header_footer_chunks[2].width as usize;
        let mut footer_line = Line::default();
        let mut left_seg_len = author.graphemes(true).count();
        let mut right_seg_len = title.graphemes(true).count();
        let sep = if !author.is_empty() && !title.is_empty() {
            1
        } else {
            0
        };
        if left_seg_len + sep + right_seg_len > total_width {
            let max_left = total_width.saturating_sub(sep + right_seg_len);
            if max_left < left_seg_len {
                if max_left > 1 {
                    let mut acc = String::new();
                    for (used, g) in author.graphemes(true).enumerate() {
                        if used + 1 >= max_left {
                            break;
                        }
                        acc.push_str(g);
                    }
                    author = format!("{}…", acc);
                    left_seg_len = author.graphemes(true).count();
                } else {
                    author.clear();
                    left_seg_len = 0;
                }
            }
        }
        if left_seg_len + sep + right_seg_len > total_width {
            let max_right = total_width.saturating_sub(left_seg_len + sep);
            if max_right < right_seg_len {
                if max_right > 1 {
                    let keep = max_right.saturating_sub(1);
                    let gs: Vec<&str> = title.graphemes(true).collect();
                    let start = gs.len().saturating_sub(keep);
                    title = format!("…{}", gs[start..].concat());
                    right_seg_len = title.graphemes(true).count();
                } else {
                    title.clear();
                    right_seg_len = 0;
                }
            }
        }
        if !author.is_empty() {
            footer_line.push_span(Span::styled(
                author,
                Style::default()
                    .bg(self.theme.footer_bg)
                    .fg(self.theme.footer_fg),
            ));
            if right_seg_len > 0 {
                footer_line.push_span(Span::styled(
                    " ",
                    Style::default().bg(self.theme.footer_pad_bg),
                ));
            }
        }
        let pad = total_width.saturating_sub(
            left_seg_len
                + sep
                + right_seg_len
                + if left_seg_len > 0 && right_seg_len > 0 {
                    1
                } else {
                    0
                },
        );
        if pad > 0 {
            footer_line.push_span(Span::styled(
                " ".repeat(pad),
                Style::default().bg(self.theme.footer_pad_bg),
            ));
        }
        if !title.is_empty() {
            footer_line.push_span(Span::styled(
                title,
                Style::default()
                    .bg(self.theme.footer_pad_bg)
                    .fg(self.theme.footer_fg),
            ));
        }
        let footer = Paragraph::new(footer_line);
        f.render_widget(footer, header_footer_chunks[2]);
    }

    pub fn up(&mut self, lines: usize) {
        let delta = lines.max(1);
        self.current = self.current.saturating_sub(delta);
    }

    pub fn down(&mut self, lines: usize) {
        let delta = lines.max(1);
        self.current = (self.current + delta).min(self.pages.len().saturating_sub(1));
    }

    pub fn reflow(&mut self, blocks: &[ReaderBlock], size: Size) {
        let p = reader_core::layout::paginate_with_justify(blocks, size, self.justify);
        self.pages = p.pages;
        self.chapter_starts = p.chapter_starts;
        self.current = self.current.min(self.pages.len().saturating_sub(1));
    }
}
