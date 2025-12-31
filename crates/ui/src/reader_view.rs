use ratatui::{prelude::*, widgets::*};
use unicode_segmentation::UnicodeSegmentation;

// Tokyonight-inspired palette; tweak these to change header/footer colors.
const TN_BG: Color = Color::Rgb(26, 27, 38); // #1a1b26
const TN_BG_ALT: Color = Color::Rgb(31, 35, 53); // #1f2335
const TN_BG_STRONG: Color = Color::Rgb(65, 72, 104); // #414868
const TN_FG: Color = Color::Rgb(192, 202, 245); // #c0caf5
const TN_BLUE: Color = Color::Rgb(122, 162, 247); // #7aa2f7

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
            header_bg: TN_BG_ALT,
            header_fg: TN_FG,
            header_pad_bg: TN_BG,
            footer_bg: TN_BG_STRONG,
            footer_fg: TN_BLUE,
            footer_pad_bg: TN_BG_ALT,
        }
    }
}

#[cfg(feature = "kitty-images")]
struct KittyImage {
    png_base64: String,
}

#[cfg(feature = "kitty-images")]
#[derive(Clone)]
struct RenderImage {
    id: String,
    x: u16,
    y: u16,
    cols: u16,
    rows: u16,
}
#[cfg(feature = "kitty-images")]
use base64::Engine;
#[cfg(feature = "kitty-images")]
use crossterm::{cursor::MoveTo, queue};
#[cfg(feature = "kitty-images")]
use image::ImageFormat;
use reader_core::layout::{Page, Segment, Size, StyledLine};
use reader_core::types::Block as ReaderBlock;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
#[cfg(feature = "kitty-images")]
use std::{env, io::Write};

const SPREAD_GAP: u16 = 4;

pub struct ReaderView {
    pub pages: Vec<Page>,
    pub current: usize,
    pub last_key: Option<String>,
    pub justify: bool,
    pub two_pane: bool,
    pub chapter_starts: Vec<usize>,
    pub chapter_titles: Vec<String>,
    pub chapter_hrefs: Vec<String>,
    pub anchors: HashMap<String, usize>,
    pub book_title: Option<String>,
    pub author: Option<String>,
    pub theme: Theme,
    pub total_pages: Option<usize>,
    pub toc_overrides: Vec<reader_core::pdf::OutlineEntry>,
    pub selection: Option<SelectionRange>,
    pub image_map: HashMap<String, Arc<Vec<u8>>>,
    #[cfg(feature = "kitty-images")]
    image_cache: HashMap<String, KittyImage>,
    #[cfg(feature = "kitty-images")]
    image_placements: Vec<RenderImage>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectionPoint {
    pub page: usize,
    pub line: usize,
    pub col: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct SelectionRange {
    pub start: SelectionPoint,
    pub end: SelectionPoint,
}

impl SelectionRange {
    pub fn normalized(self) -> (SelectionPoint, SelectionPoint) {
        let a = (self.start.page, self.start.line, self.start.col);
        let b = (self.end.page, self.end.line, self.end.col);
        if a <= b {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }
}

pub struct ContentAreas {
    pub body: Rect,
    pub left: Rect,
    pub right: Option<Rect>,
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
            two_pane: false,
            chapter_starts: Vec::new(),
            chapter_titles: Vec::new(),
            chapter_hrefs: Vec::new(),
            anchors: HashMap::new(),
            book_title: None,
            author: None,
            theme: Theme::default(),
            total_pages: None,
            toc_overrides: Vec::new(),
            selection: None,
            image_map: HashMap::new(),
            #[cfg(feature = "kitty-images")]
            image_cache: HashMap::new(),
            #[cfg(feature = "kitty-images")]
            image_placements: Vec::new(),
        }
    }

    pub fn content_areas(&self, area: Rect, column_width: u16) -> ContentAreas {
        let vchunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);
        let content_area = vchunks[0];
        let col_w = if self.two_pane {
            let total = column_width
                .saturating_mul(2)
                .saturating_add(SPREAD_GAP)
                .min(content_area.width);
            total
        } else {
            column_width.min(content_area.width)
        };
        let left_pad = content_area.width.saturating_sub(col_w) / 2;
        let centered = Rect {
            x: content_area.x + left_pad,
            y: content_area.y,
            width: col_w,
            height: content_area.height,
        };
        let header_footer_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(centered);
        let para_area = header_footer_chunks[1];
        if self.two_pane && col_w > 6 {
            let gap = SPREAD_GAP.min(col_w.saturating_sub(2));
            let remaining = col_w.saturating_sub(gap);
            let left_w = remaining / 2;
            let right_w = remaining.saturating_sub(left_w);
            let spreads = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(left_w),
                    Constraint::Length(gap),
                    Constraint::Length(right_w),
                ])
                .split(para_area);
            ContentAreas {
                body: para_area,
                left: spreads[0],
                right: Some(spreads[2]),
            }
        } else {
            ContentAreas {
                body: para_area,
                left: para_area,
                right: None,
            }
        }
    }

    pub fn add_images_from_blocks(&mut self, blocks: &[ReaderBlock]) {
        for block in blocks {
            let ReaderBlock::Image(image) = block else {
                continue;
            };
            let Some(data) = &image.data else {
                continue;
            };
            if !self.image_map.contains_key(&image.id) {
                self.image_map
                    .insert(image.id.clone(), Arc::new(data.clone()));
            }
        }
    }

    pub fn inner_size(area: Rect, column_width: u16, two_pane: bool) -> Size {
        let vchunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);
        let content_area = vchunks[0];
        let col_w = if two_pane {
            let total = column_width
                .saturating_mul(2)
                .saturating_add(SPREAD_GAP)
                .min(content_area.width);
            total
        } else {
            column_width.min(content_area.width)
        };
        let inner_w = if two_pane {
            col_w.saturating_sub(SPREAD_GAP) / 2
        } else {
            col_w
        };
        let inner_h = content_area.height.saturating_sub(2);
        Size {
            width: inner_w,
            height: inner_h,
        }
    }

    pub fn render(
        &mut self,
        f: &mut Frame<'_>,
        area: Rect,
        column_width: u16,
        highlight: Option<&str>,
    ) {
        #[cfg(feature = "kitty-images")]
        {
            self.image_placements.clear();
        }
        let vchunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);

        let content_area = vchunks[0];
        let col_w = if self.two_pane {
            let total = column_width
                .saturating_mul(2)
                .saturating_add(SPREAD_GAP)
                .min(content_area.width);
            total
        } else {
            column_width.min(content_area.width)
        };
        let left_pad = content_area.width.saturating_sub(col_w) / 2;
        let centered = Rect {
            x: content_area.x + left_pad,
            y: content_area.y,
            width: col_w,
            height: content_area.height,
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
        let body_width = header_footer_chunks[1].width as usize;

        // Header: chapter title (left) | page X/Y (right)
        let loaded = self.pages.len();
        let total = self.total_pages.unwrap_or(loaded).max(loaded);
        let current = if loaded == 0 { 0 } else { self.current + 1 };
        let chapter_label = self
            .chapter_starts
            .iter()
            .rposition(|&p| p <= self.current)
            .map(|idx| self.chapter_label(idx))
            .unwrap_or_else(|| "".to_string());
        // Build powerline-style header segments: left chapter, right page
        let mut left = chapter_label;
        let mut right = format!("Pg {}/{}", current, total);
        let total_width = body_width;
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
        let pad = total_width.saturating_sub(left_seg_len + sep + right_seg_len);
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
        f.render_widget(Clear, para_area);
        if self.two_pane && col_w > 6 {
            let gap = SPREAD_GAP.min(col_w.saturating_sub(2));
            let remaining = col_w.saturating_sub(gap);
            let left_w = remaining / 2;
            let right_w = remaining.saturating_sub(left_w);
            let spreads = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(left_w),
                    Constraint::Length(gap),
                    Constraint::Length(right_w),
                ])
                .split(para_area);
            let base = self.current.saturating_sub(self.current % 2);
            let left_lines = self.page_lines(base, highlight);
            let right_lines = self.page_lines(base + 1, highlight);
            #[cfg(feature = "kitty-images")]
            {
                self.collect_image_placements(base, spreads[0]);
                self.collect_image_placements(base + 1, spreads[2]);
            }
            let left_p = Paragraph::new(left_lines).wrap(Wrap { trim: false });
            let right_p = Paragraph::new(right_lines).wrap(Wrap { trim: false });
            f.render_widget(left_p, spreads[0]);
            f.render_widget(right_p, spreads[2]);
        } else {
            let lines = self.page_lines(self.current, highlight);
            #[cfg(feature = "kitty-images")]
            {
                self.collect_image_placements(self.current, para_area);
            }
            let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
            f.render_widget(paragraph, para_area);
        }

        // Footer: powerline segments left author, right title
        let mut author = self.author.clone().unwrap_or_default();
        let mut title = self.book_title.clone().unwrap_or_default();
        let total_width = body_width;
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
        let pad = total_width.saturating_sub(left_seg_len + sep + right_seg_len);
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

    pub fn search_forward(&self, query: &str, start_from: Option<usize>) -> Option<usize> {
        let needle = query.trim();
        if needle.is_empty() || self.pages.is_empty() {
            return None;
        }
        let needle = needle.to_lowercase();
        let total = self.pages.len();
        let start_raw = start_from.unwrap_or(self.current);
        let start = if total == 0 { 0 } else { start_raw % total };
        for offset in 0..total {
            let idx = (start + offset) % total;
            if Self::page_contains(&self.pages[idx], &needle) {
                return Some(idx);
            }
        }
        None
    }

    fn page_contains(page: &Page, needle: &str) -> bool {
        if needle.is_empty() {
            return false;
        }
        let mut buf = String::new();
        for (i, line) in page.lines.iter().enumerate() {
            if i > 0 {
                buf.push(' ');
            }
            for seg in &line.segments {
                buf.push_str(&seg.text.to_lowercase());
            }
        }
        buf.contains(needle)
    }

    pub fn up(&mut self, lines: usize) {
        let delta = lines.max(1);
        let step = if self.two_pane {
            ((delta + 1) / 2) * 2
        } else {
            delta
        };
        self.current = self.current.saturating_sub(step);
        if self.two_pane {
            self.current = self.current.saturating_sub(self.current % 2);
        }
    }

    pub fn down(&mut self, lines: usize) {
        let delta = lines.max(1);
        let step = if self.two_pane {
            ((delta + 1) / 2) * 2
        } else {
            delta
        };
        self.current = (self.current + step).min(self.pages.len().saturating_sub(1));
        if self.two_pane {
            self.current = self.current.saturating_sub(self.current % 2);
        }
    }

    pub fn reflow(&mut self, blocks: &[ReaderBlock], size: Size) {
        let p = reader_core::layout::paginate_with_justify(blocks, size, self.justify);
        self.pages = p.pages;
        self.chapter_starts = p.chapter_starts;
        self.anchors = p.anchors;
        self.current = self.current.min(self.pages.len().saturating_sub(1));
        if self.two_pane {
            self.current = self.current.saturating_sub(self.current % 2);
        }
    }

    fn page_lines(&self, idx: usize, highlight: Option<&str>) -> Vec<ratatui::text::Line<'static>> {
        if let Some(page) = self.pages.get(idx) {
            page.lines
                .iter()
                .enumerate()
                .map(|(line_idx, l)| {
                    let sel = self
                        .selection
                        .and_then(|selection| selection_for_line(selection, idx, line_idx, l));
                    Self::highlight_line(l, highlight, sel)
                })
                .collect()
        } else {
            vec![ratatui::text::Line::from("")] // empty placeholder for missing spread page
        }
    }

    #[cfg(feature = "kitty-images")]
    fn collect_image_placements(&mut self, page_idx: usize, area: Rect) {
        let Some(page) = self.pages.get(page_idx) else {
            return;
        };
        for (line_idx, line) in page.lines.iter().enumerate() {
            let Some(image) = &line.image else {
                continue;
            };
            let y = area.y.saturating_add(line_idx as u16);
            if y >= area.y.saturating_add(area.height) {
                break;
            }
            let cols = image.cols.min(area.width.max(1));
            self.image_placements.push(RenderImage {
                id: image.id.clone(),
                x: area.x,
                y,
                cols,
                rows: image.rows,
            });
        }
    }

    #[cfg(feature = "kitty-images")]
    pub fn render_images<W: Write>(&mut self, out: &mut W) -> std::io::Result<()> {
        if self.image_placements.is_empty() || !kitty_supported() {
            return Ok(());
        }
        write!(out, "\x1b_Ga=d\x1b\\")?;
        let placements = self.image_placements.clone();
        for placement in placements {
            let data = self.image_map.get(&placement.id).cloned();
            let Some(data) = data else {
                continue;
            };
            let Some(encoded) = self.ensure_png_base64(&placement.id, data.as_slice()) else {
                continue;
            };
            queue!(out, MoveTo(placement.x, placement.y))?;
            send_kitty_image(out, &encoded, placement.cols.max(1), placement.rows.max(1))?;
        }
        out.flush()
    }

    fn highlight_line(
        line: &StyledLine,
        highlight: Option<&str>,
        selection: Option<(usize, usize)>,
    ) -> ratatui::text::Line<'static> {
        if let Some((start, end)) = selection {
            return Self::selection_line(line, start, end);
        }
        let needle = highlight
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or("");
        let mut spans: Vec<Span<'static>> = Vec::new();
        for seg in &line.segments {
            let base_style = Self::segment_style(seg);
            let seg_text = Self::segment_display_text(seg);
            let segment_spans = Self::highlight_text(seg_text.as_ref(), needle, base_style);
            spans.extend(segment_spans);
        }
        ratatui::text::Line::from(spans)
    }

    fn highlight_text(text: &str, needle: &str, base_style: Style) -> Vec<Span<'static>> {
        if needle.is_empty() {
            return vec![Span::styled(text.to_string(), base_style)];
        }
        let needle_g: Vec<String> = needle.graphemes(true).map(|g| g.to_lowercase()).collect();
        let mut spans: Vec<Span<'static>> = Vec::new();
        let line_g: Vec<&str> = text.graphemes(true).collect();
        let mut start = 0;
        let mut i = 0;
        while i + needle_g.len() <= line_g.len() {
            let window = &line_g[i..i + needle_g.len()];
            let matches = window
                .iter()
                .zip(needle_g.iter())
                .all(|(a, b)| a.to_lowercase() == *b);
            if matches {
                if start < i {
                    let plain = line_g[start..i].concat();
                    spans.push(Span::styled(plain, base_style));
                }
                let matched = window.concat();
                spans.push(Span::styled(
                    matched,
                    Style::default().bg(Color::Yellow).fg(Color::Black),
                ));
                i += needle_g.len();
                start = i;
            } else {
                i += 1;
            }
        }
        if start < line_g.len() {
            spans.push(Span::styled(line_g[start..].concat(), base_style));
        }
        spans
    }

    fn segment_style(seg: &Segment) -> Style {
        let mut style = Style::default();
        if let Some(rgb) = &seg.fg {
            style = style.fg(Color::Rgb(rgb.r, rgb.g, rgb.b));
        }
        if let Some(rgb) = &seg.bg {
            style = style.bg(Color::Rgb(rgb.r, rgb.g, rgb.b));
        }
        if seg.style.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if seg.style.italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if seg.style.underline {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        if seg.style.dim {
            style = style.add_modifier(Modifier::DIM);
        }
        if seg.style.reverse {
            style = style.add_modifier(Modifier::REVERSED);
        }
        if seg.style.strike {
            style = style.add_modifier(Modifier::CROSSED_OUT);
        }
        if seg.link.is_some() {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        style
    }

    #[cfg(feature = "kitty-images")]
    fn ensure_png_base64(&mut self, id: &str, data: &[u8]) -> Option<String> {
        if let Some(cached) = self.image_cache.get(id) {
            return Some(cached.png_base64.clone());
        }
        let png = encode_png(data)?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(png);
        self.image_cache.insert(
            id.to_string(),
            KittyImage {
                png_base64: encoded.clone(),
            },
        );
        Some(encoded)
    }

    fn segment_display_text(seg: &Segment) -> Cow<'_, str> {
        if !seg.style.small_caps {
            return Cow::Borrowed(seg.text.as_str());
        }
        Cow::Owned(Self::small_caps_text(&seg.text))
    }

    fn small_caps_text(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for ch in text.chars() {
            if ch.is_ascii() {
                out.push(ch.to_ascii_uppercase());
            } else {
                out.push(ch);
            }
        }
        out
    }

    pub fn chapter_title(&self, idx: usize) -> String {
        self.chapter_titles
            .get(idx)
            .map(|title| sanitize_chapter_title(title))
            .unwrap_or_default()
    }

    pub fn chapter_label(&self, idx: usize) -> String {
        let title = self.chapter_title(idx);
        if title.is_empty() {
            format!("Chapter {}", idx + 1)
        } else {
            title
        }
    }

    pub fn link_at_point(&self, point: SelectionPoint) -> Option<String> {
        let page = self.pages.get(point.page)?;
        let line = page.lines.get(point.line)?;
        let mut offset = 0usize;
        for seg in &line.segments {
            let seg_text = Self::segment_display_text(seg);
            let seg_len = seg_text.graphemes(true).count();
            if point.col < offset + seg_len {
                return seg.link.clone();
            }
            offset += seg_len;
        }
        None
    }

    pub fn link_label_at_point(&self, point: SelectionPoint) -> Option<String> {
        let page = self.pages.get(point.page)?;
        let line = page.lines.get(point.line)?;
        let mut offset = 0usize;
        for seg in &line.segments {
            let seg_text = Self::segment_display_text(seg);
            let seg_len = seg_text.graphemes(true).count();
            if point.col < offset + seg_len {
                return Some(seg_text.into_owned());
            }
            offset += seg_len;
        }
        None
    }

    pub fn page_for_href(&self, href: &str) -> Option<usize> {
        self.resolve_target_page(href)
    }

    pub fn jump_to_target(&mut self, target: &str) -> bool {
        let Some(mut page) = self.resolve_target_page(target) else {
            return false;
        };
        if !self.pages.is_empty() {
            page = page.min(self.pages.len().saturating_sub(1));
        }
        if self.two_pane {
            page = page.saturating_sub(page % 2);
        }
        self.current = page;
        true
    }

    fn resolve_target_page(&self, target: &str) -> Option<usize> {
        let target = target.trim();
        if target.is_empty() {
            return None;
        }
        if let Some(page) = self.anchors.get(target) {
            return Some(*page);
        }
        if target.starts_with('#') {
            if let Some(prefix) = self.current_chapter_href() {
                let full = format!("{}{}", prefix, target);
                if let Some(page) = self.anchors.get(&full) {
                    return Some(*page);
                }
            }
        }
        if let Some((path, _frag)) = target.split_once('#') {
            if let Some(idx) = self.chapter_hrefs.iter().position(|h| h == path) {
                return self.chapter_starts.get(idx).copied();
            }
        } else if let Some(idx) = self.chapter_hrefs.iter().position(|h| h == target) {
            return self.chapter_starts.get(idx).copied();
        }
        None
    }

    fn current_chapter_href(&self) -> Option<&str> {
        let idx = self.current_chapter_index()?;
        self.chapter_hrefs.get(idx).map(|s| s.as_str())
    }

    fn current_chapter_index(&self) -> Option<usize> {
        if self.chapter_starts.is_empty() {
            return None;
        }
        let mut idx = 0usize;
        for (i, start) in self.chapter_starts.iter().enumerate() {
            if *start <= self.current {
                idx = i;
            } else {
                break;
            }
        }
        Some(idx)
    }

    fn selection_line(
        line: &StyledLine,
        sel_start: usize,
        sel_end: usize,
    ) -> ratatui::text::Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut offset = 0;
        for seg in &line.segments {
            let base_style = Self::segment_style(seg);
            let seg_text = Self::segment_display_text(seg);
            let seg_text = seg_text.as_ref();
            let seg_len = seg_text.graphemes(true).count();
            let seg_start = offset;
            let seg_end = offset + seg_len;
            if sel_end <= seg_start || sel_start >= seg_end {
                spans.push(Span::styled(seg_text.to_string(), base_style));
            } else {
                let local_start = sel_start.saturating_sub(seg_start).min(seg_len);
                let local_end = sel_end.saturating_sub(seg_start).min(seg_len);
                let gs: Vec<&str> = seg_text.graphemes(true).collect();
                if local_start > 0 {
                    spans.push(Span::styled(gs[..local_start].concat(), base_style));
                }
                if local_end > local_start {
                    let sel_style = base_style.bg(Color::DarkGray);
                    spans.push(Span::styled(gs[local_start..local_end].concat(), sel_style));
                }
                if local_end < seg_len {
                    spans.push(Span::styled(gs[local_end..].concat(), base_style));
                }
            }
            offset += seg_len;
        }
        ratatui::text::Line::from(spans)
    }
}

fn selection_for_line(
    selection: SelectionRange,
    page_idx: usize,
    line_idx: usize,
    line: &StyledLine,
) -> Option<(usize, usize)> {
    let line_len = line
        .segments
        .iter()
        .map(|seg| seg.text.graphemes(true).count())
        .sum();
    if line_len == 0 {
        return None;
    }
    let (start, end) = selection.normalized();
    if page_idx < start.page || page_idx > end.page {
        return None;
    }
    if page_idx == start.page && line_idx < start.line {
        return None;
    }
    if page_idx == end.page && line_idx > end.line {
        return None;
    }
    let start_col = if page_idx == start.page && line_idx == start.line {
        start.col.min(line_len)
    } else {
        0
    };
    let end_col = if page_idx == end.page && line_idx == end.line {
        end.col.min(line_len)
    } else {
        line_len
    };
    if start_col == end_col {
        None
    } else {
        Some((start_col.min(end_col), end_col.max(start_col)))
    }
}

fn sanitize_chapter_title(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let stripped = strip_parenthetical_links(trimmed);
    let stripped = strip_inline_markers(&stripped);
    if stripped.is_empty() {
        return String::new();
    }
    let lower = stripped.to_ascii_lowercase();
    let looks_like_file =
        lower.ends_with(".xhtml") || lower.ends_with(".html") || lower.ends_with(".htm");
    let has_path = stripped.contains('/') || stripped.contains('\\');
    if !looks_like_file && !has_path {
        return stripped.to_string();
    }
    let mut s = stripped.as_str();
    if let Some(pos) = s.find('#') {
        s = &s[..pos];
    }
    if let Some(pos) = s.find('?') {
        s = &s[..pos];
    }
    if let Some(seg) = s.rsplit(&['/', '\\'][..]).next() {
        s = seg;
    }
    let mut cleaned = s.to_string();
    let lower = cleaned.to_ascii_lowercase();
    for ext in [".xhtml", ".html", ".htm"] {
        if lower.ends_with(ext) && cleaned.len() > ext.len() {
            let new_len = cleaned.len() - ext.len();
            cleaned.truncate(new_len);
            break;
        }
    }
    let mut out = String::with_capacity(cleaned.len());
    let mut last_space = false;
    for ch in cleaned.chars() {
        let mapped = match ch {
            '_' | '-' | '.' => ' ',
            _ => ch,
        };
        if mapped.is_whitespace() {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(mapped);
            last_space = false;
        }
    }
    let out = out.trim().to_string();
    if out.is_empty() {
        stripped
    } else {
        out
    }
}

fn strip_parenthetical_links(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut buf = String::new();
    let mut in_paren = false;
    for ch in input.chars() {
        if ch == '(' {
            if !in_paren {
                in_paren = true;
                buf.clear();
                continue;
            }
        }
        if ch == ')' && in_paren {
            let lower = buf.to_ascii_lowercase();
            let is_link = lower.contains(".xhtml")
                || lower.contains(".html")
                || lower.contains(".htm")
                || lower.contains("http://")
                || lower.contains("https://");
            if !is_link {
                out.push(' ');
                out.push('(');
                out.push_str(buf.trim());
                out.push(')');
            }
            in_paren = false;
            buf.clear();
            continue;
        }
        if in_paren {
            buf.push(ch);
        } else {
            out.push(ch);
        }
    }
    if in_paren {
        out.push(' ');
        out.push('(');
        out.push_str(buf.trim());
    }
    let mut cleaned = String::with_capacity(out.len());
    let mut last_space = false;
    for ch in out.chars() {
        if ch.is_whitespace() {
            if !last_space {
                cleaned.push(' ');
            }
            last_space = true;
        } else {
            cleaned.push(ch);
            last_space = false;
        }
    }
    cleaned.trim().to_string()
}

fn strip_inline_markers(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1E' || ch == '\x1F' {
            let _ = chars.next();
            continue;
        }
        if ch == '\x1C' {
            while let Some(next) = chars.next() {
                if next == '\x1D' {
                    break;
                }
            }
            continue;
        }
        if ch == '\x18' {
            while let Some(next) = chars.next() {
                if next == '\x17' {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

#[cfg(feature = "kitty-images")]
fn kitty_supported() -> bool {
    if env::var("KITTY_WINDOW_ID").is_ok() {
        return true;
    }
    env::var("TERM")
        .map(|term| term.contains("kitty"))
        .unwrap_or(false)
}

#[cfg(feature = "kitty-images")]
fn encode_png(data: &[u8]) -> Option<Vec<u8>> {
    let image = image::load_from_memory(data).ok()?;
    let mut out = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Png)
        .ok()?;
    Some(out)
}

#[cfg(feature = "kitty-images")]
fn send_kitty_image<W: Write>(
    out: &mut W,
    base64: &str,
    cols: u16,
    rows: u16,
) -> std::io::Result<()> {
    let chunk_size = 4096usize;
    let bytes = base64.as_bytes();
    let total = (bytes.len() + chunk_size - 1) / chunk_size;
    for idx in 0..total {
        let start = idx * chunk_size;
        let end = (start + chunk_size).min(bytes.len());
        let chunk = std::str::from_utf8(&bytes[start..end]).unwrap_or("");
        let last = idx + 1 == total;
        let mut params = String::new();
        if idx == 0 {
            params.push_str(&format!("a=T,f=100,C=1,c={},r={},q=2", cols, rows));
        }
        if !last {
            if !params.is_empty() {
                params.push(',');
            }
            params.push_str("m=1");
        }
        write!(out, "\x1b_G{};{}\x1b\\", params, chunk)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reader_core::layout::TextStyle;

    fn page(lines: &[&str]) -> Page {
        Page {
            lines: lines
                .iter()
                .map(|s| StyledLine {
                    segments: vec![Segment {
                        text: (*s).to_string(),
                        fg: None,
                        bg: None,
                        style: TextStyle::default(),
                        link: None,
                    }],
                    image: None,
                })
                .collect(),
        }
    }

    #[test]
    fn search_forward_is_case_insensitive() {
        let mut view = ReaderView::new();
        view.pages = vec![
            page(&["First page"]),
            page(&["Second Match"]),
            page(&["Third"]),
        ];
        assert_eq!(view.search_forward("match", None), Some(1));
        assert_eq!(view.search_forward("SeCoNd", None), Some(1));
    }

    #[test]
    fn search_forward_wraps_from_end() {
        let mut view = ReaderView::new();
        view.pages = vec![page(&["Alpha"]), page(&["Beta"]), page(&["Gamma"])];
        view.current = 2;
        assert_eq!(view.search_forward("alpha", None), Some(0));
    }

    #[test]
    fn search_forward_matches_across_lines() {
        let mut view = ReaderView::new();
        view.pages = vec![page(&["Hello brave", "new world"]), page(&["Unused"])];
        assert_eq!(view.search_forward("brave new", None), Some(0));
    }

    #[test]
    fn search_forward_can_start_after_previous_hit() {
        let mut view = ReaderView::new();
        view.pages = vec![
            page(&["One fish"]),
            page(&["Two fish"]),
            page(&["Red fish"]),
            page(&["Blue fish"]),
        ];
        assert_eq!(view.search_forward("fish", None), Some(0));
        assert_eq!(view.search_forward("fish", Some(1)), Some(1));
        assert_eq!(view.search_forward("fish", Some(2)), Some(2));
        assert_eq!(view.search_forward("fish", Some(3)), Some(3));
        assert_eq!(view.search_forward("fish", Some(4)), Some(0)); // wraps
    }

    #[test]
    fn highlight_line_marks_case_insensitive_matches() {
        let styled = StyledLine {
            segments: vec![Segment {
                text: "Hello World".into(),
                fg: None,
                bg: None,
                style: TextStyle::default(),
                link: None,
            }],
            image: None,
        };
        let line = ReaderView::highlight_line(&styled, Some("world"), None);
        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].content, "Hello ");
        assert_eq!(line.spans[1].content, "World");
        assert_eq!(line.spans[1].style.bg, Some(Color::Yellow));
    }

    #[test]
    fn highlight_line_marks_multiple_occurrences() {
        let styled = StyledLine {
            segments: vec![Segment {
                text: "aba ba".into(),
                fg: None,
                bg: None,
                style: TextStyle::default(),
                link: None,
            }],
            image: None,
        };
        let line = ReaderView::highlight_line(&styled, Some("ba"), None);
        assert_eq!(line.spans.len(), 4); // "a" + "ba" + " " + "ba"
        assert_eq!(line.spans[1].content, "ba");
        assert_eq!(line.spans[1].style.bg, Some(Color::Yellow));
        assert_eq!(line.spans[3].content, "ba");
        assert_eq!(line.spans[3].style.bg, Some(Color::Yellow));
    }
}
