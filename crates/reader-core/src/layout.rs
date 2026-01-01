use crate::types::{Block, TableBlock, TableCell};
use highlight;
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy)]
pub struct Size {
    pub width: u16,
    pub height: u16,
}

#[derive(Clone)]
pub struct Page {
    pub lines: Vec<StyledLine>,
}

#[derive(Clone)]
pub struct StyledLine {
    pub segments: Vec<Segment>,
    pub image: Option<ImagePlacement>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct TextStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub dim: bool,
    pub reverse: bool,
    pub strike: bool,
    pub small_caps: bool,
}

const STYLE_START: char = '\x1E';
const STYLE_END: char = '\x1F';
const LINK_START: char = '\x1C';
const LINK_END: char = '\x1D';
const ANCHOR_START: char = '\x18';
const ANCHOR_END: char = '\x17';

#[derive(Clone)]
pub struct Segment {
    pub text: String,
    pub fg: Option<crate::types::RgbColor>,
    pub bg: Option<crate::types::RgbColor>,
    pub style: TextStyle,
    pub link: Option<String>,
}

#[derive(Clone)]
pub struct ImagePlacement {
    pub id: String,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Clone)]
pub struct Pagination {
    pub pages: Vec<Page>,
    pub chapter_starts: Vec<usize>, // page indices where a chapter begins
    pub anchors: HashMap<String, usize>,
}

#[derive(Clone)]
struct InlineSpan {
    text: String,
    style: TextStyle,
    link: Option<String>,
}

enum InlinePiece {
    Span(InlineSpan),
    Anchor(String),
}

#[derive(Default)]
struct StyleCounts {
    bold: u16,
    italic: u16,
    underline: u16,
    code: u16,
    strike: u16,
    small_caps: u16,
}

#[derive(Clone)]
struct InlineWord {
    segments: Vec<Segment>,
    width: usize,
}

enum InlineToken {
    Word(InlineWord),
    Space(TextStyle, Option<String>),
    Newline,
    Anchor(String),
}

struct WrappedLines {
    lines: Vec<StyledLine>,
    anchors: Vec<Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct WordToken {
    pub text: String,
    pub is_sentence_end: bool,
    pub is_comma: bool,
    pub chapter_index: Option<usize>,
}

pub fn extract_words(blocks: &[Block]) -> Vec<WordToken> {
    let mut words = Vec::new();
    let mut current_chapter: Option<usize> = None;
    let mut chapter_counter = 0;

    for (idx, block) in blocks.iter().enumerate() {
        match block {
            Block::Code { .. } => {
                continue;
            }
            Block::Paragraph(text) => {
                let cleaned = strip_style_markers(text);
                if cleaned.trim() == "───" {
                    if is_chapter_separator(blocks, idx) {
                        chapter_counter += 1;
                        current_chapter = Some(chapter_counter);
                    }
                    continue;
                }
                if cleaned.trim() == "[image]" {
                    continue;
                }
                for word in cleaned.split_whitespace() {
                    let token = WordToken::from_word(word.to_string(), current_chapter);
                    words.push(token);
                }
            }
            Block::Heading(text, _) => {
                let cleaned = strip_style_markers(text);
                for word in cleaned.split_whitespace() {
                    let token = WordToken::from_word(word.to_string(), current_chapter);
                    words.push(token);
                }
            }
            Block::List(items) => {
                for item in items {
                    let cleaned = strip_style_markers(item);
                    for word in cleaned.split_whitespace() {
                        let token = WordToken::from_word(word.to_string(), current_chapter);
                        words.push(token);
                    }
                }
            }
            Block::Quote(text) => {
                let cleaned = strip_style_markers(text);
                for word in cleaned.split_whitespace() {
                    let token = WordToken::from_word(word.to_string(), current_chapter);
                    words.push(token);
                }
            }
            Block::Image(image) => {
                let label = image
                    .caption
                    .as_ref()
                    .or(image.alt.as_ref())
                    .map(|s| strip_style_markers(s));
                if let Some(label) = label {
                    for word in label.split_whitespace() {
                        let token = WordToken::from_word(word.to_string(), current_chapter);
                        words.push(token);
                    }
                }
            }
            Block::Table(table) => {
                for row in &table.rows {
                    for cell in row {
                        let cleaned = strip_style_markers(&cell.text);
                        for word in cleaned.split_whitespace() {
                            let token = WordToken::from_word(word.to_string(), current_chapter);
                            words.push(token);
                        }
                    }
                }
            }
        }
    }

    words
}

impl WordToken {
    fn from_word(text: String, chapter_index: Option<usize>) -> Self {
        let is_sentence_end = Self::has_sentence_end_punct(&text);
        let is_comma = Self::has_comma_punct(&text);

        WordToken {
            text,
            is_sentence_end,
            is_comma,
            chapter_index,
        }
    }

    fn has_sentence_end_punct(text: &str) -> bool {
        let trimmed =
            text.trim_end_matches(|c: char| c == ')' || c == ']' || c == '"' || c == '\'');
        trimmed.ends_with('.')
            || trimmed.ends_with('!')
            || trimmed.ends_with('?')
            || trimmed.ends_with(':')
            || trimmed.ends_with(';')
    }

    fn has_comma_punct(text: &str) -> bool {
        let trimmed =
            text.trim_end_matches(|c: char| c == ')' || c == ']' || c == '"' || c == '\'');
        trimmed.ends_with(',') || trimmed.ends_with('-') || trimmed.ends_with(')')
    }
}

fn is_chapter_separator(blocks: &[Block], idx: usize) -> bool {
    let Block::Paragraph(text) = &blocks[idx] else {
        return false;
    };
    if text.trim() != "───" {
        return false;
    }
    let prev_empty = idx
        .checked_sub(1)
        .and_then(|i| match &blocks[i] {
            Block::Paragraph(prev) if prev.trim().is_empty() => Some(()),
            _ => None,
        })
        .is_some();
    let next_empty = blocks
        .get(idx + 1)
        .and_then(|b| match b {
            Block::Paragraph(next) if next.trim().is_empty() => Some(()),
            _ => None,
        })
        .is_some();
    prev_empty && next_empty
}

fn strip_style_markers(input: &str) -> String {
    if !input.contains(STYLE_START)
        && !input.contains(STYLE_END)
        && !input.contains(LINK_START)
        && !input.contains(ANCHOR_START)
    {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == STYLE_START || ch == STYLE_END {
            let _ = chars.next();
            continue;
        }
        if ch == LINK_START {
            while let Some(next) = chars.next() {
                if next == LINK_END {
                    break;
                }
            }
            continue;
        }
        if ch == ANCHOR_START {
            while let Some(next) = chars.next() {
                if next == ANCHOR_END {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

pub fn paginate(blocks: &[Block], size: Size) -> Vec<Page> {
    paginate_with_justify(blocks, size, false).pages
}

pub fn paginate_with_justify(blocks: &[Block], size: Size, justify: bool) -> Pagination {
    // Greedy wrap with optional full justification
    let mut pages: Vec<Page> = Vec::new();
    let mut current = Page { lines: Vec::new() };
    let mut chapter_starts: Vec<usize> = Vec::new();
    let mut anchors: HashMap<String, usize> = HashMap::new();
    let mut at_page_index: usize = pages.len();
    let push_line = |line: StyledLine,
                     line_anchors: &[String],
                     pages: &mut Vec<Page>,
                     current: &mut Page,
                     at_page_index: &mut usize,
                     anchors: &mut HashMap<String, usize>| {
        for anchor in line_anchors {
            anchors.entry(anchor.clone()).or_insert(*at_page_index);
        }
        current.lines.push(line);
        if current.lines.len() as u16 >= size.height {
            pages.push(current.clone());
            *current = Page { lines: Vec::new() };
            *at_page_index += 1;
        }
    };
    let mut pending_chapter_start: Option<usize> = Some(0); // initial chapter starts at page 0
    for (idx, block) in blocks.iter().enumerate() {
        match block {
            Block::Paragraph(text) => {
                // If a separator was seen, mark the next content start as a chapter start
                if let Some(start_idx) = pending_chapter_start.take() {
                    chapter_starts.push(start_idx);
                }
                // Detect separator and set the next start index
                if is_chapter_separator(blocks, idx) {
                    pending_chapter_start = Some(at_page_index);
                }
                let wrapped = wrap_styled_text(text, size.width as usize);
                for i in 0..wrapped.lines.len() {
                    let is_last = i == wrapped.lines.len().saturating_sub(1);
                    let line = if justify && !is_last {
                        justify_styled_line(&wrapped.lines[i], size.width as usize)
                    } else {
                        wrapped.lines[i].clone()
                    };
                    let anchors_for_line = &wrapped.anchors[i];
                    push_line(
                        line,
                        anchors_for_line,
                        &mut pages,
                        &mut current,
                        &mut at_page_index,
                        &mut anchors,
                    );
                }
                // blank line between paragraphs
                push_line(
                    StyledLine::from_plain(String::new()),
                    &[],
                    &mut pages,
                    &mut current,
                    &mut at_page_index,
                    &mut anchors,
                );
            }
            Block::Quote(text) => {
                if let Some(start_idx) = pending_chapter_start.take() {
                    chapter_starts.push(start_idx);
                }
                // Two-space indent; add a rule when there is room
                let show_rule = size.width >= 16;
                let prefix = if show_rule { "│ " } else { "  " };
                let max_width = size.width.max(4) as usize;
                // Preserve line breaks like a code/pre block; truncate when too long
                for raw_line in text.lines() {
                    let (mut segs, line_anchors) = segments_from_text_with_anchors(raw_line);
                    let mut prefixed = Vec::with_capacity(segs.len() + 1);
                    prefixed.push(Segment {
                        text: prefix.to_string(),
                        fg: None,
                        bg: None,
                        style: TextStyle::default(),
                        link: None,
                    });
                    prefixed.append(&mut segs);
                    let clipped = clip_segments(prefixed, max_width);
                    push_line(
                        clipped,
                        &line_anchors,
                        &mut pages,
                        &mut current,
                        &mut at_page_index,
                        &mut anchors,
                    );
                }
                push_line(
                    StyledLine::from_plain(String::new()),
                    &[],
                    &mut pages,
                    &mut current,
                    &mut at_page_index,
                    &mut anchors,
                );
            }
            Block::Heading(text, _) => {
                if let Some(start_idx) = pending_chapter_start.take() {
                    chapter_starts.push(start_idx);
                }
                let mut wrapped = wrap_styled_text(text, size.width as usize);
                for i in 0..wrapped.lines.len() {
                    uppercase_segments(&mut wrapped.lines[i].segments);
                    let anchors_for_line = &wrapped.anchors[i];
                    push_line(
                        wrapped.lines[i].clone(),
                        anchors_for_line,
                        &mut pages,
                        &mut current,
                        &mut at_page_index,
                        &mut anchors,
                    );
                }
                push_line(
                    StyledLine::from_plain(String::new()),
                    &[],
                    &mut pages,
                    &mut current,
                    &mut at_page_index,
                    &mut anchors,
                );
            }
            Block::List(items) => {
                if let Some(start_idx) = pending_chapter_start.take() {
                    chapter_starts.push(start_idx);
                }
                for item in items {
                    let line = format!("• {}", item);
                    let wrapped = wrap_styled_text(&line, size.width as usize);
                    for i in 0..wrapped.lines.len() {
                        let is_last = i == wrapped.lines.len().saturating_sub(1);
                        let out = if justify && !is_last {
                            justify_styled_line(&wrapped.lines[i], size.width as usize)
                        } else {
                            wrapped.lines[i].clone()
                        };
                        let anchors_for_line = &wrapped.anchors[i];
                        push_line(
                            out,
                            anchors_for_line,
                            &mut pages,
                            &mut current,
                            &mut at_page_index,
                            &mut anchors,
                        );
                    }
                }
                push_line(
                    StyledLine::from_plain(String::new()),
                    &[],
                    &mut pages,
                    &mut current,
                    &mut at_page_index,
                    &mut anchors,
                );
            }
            Block::Table(table) => {
                if let Some(start_idx) = pending_chapter_start.take() {
                    chapter_starts.push(start_idx);
                }
                let table_lines = render_table(table, size.width as usize);
                for (line, line_anchors) in table_lines {
                    push_line(
                        line,
                        &line_anchors,
                        &mut pages,
                        &mut current,
                        &mut at_page_index,
                        &mut anchors,
                    );
                }
                push_line(
                    StyledLine::from_plain(String::new()),
                    &[],
                    &mut pages,
                    &mut current,
                    &mut at_page_index,
                    &mut anchors,
                );
            }
            Block::Code { text, lang } => {
                if let Some(start_idx) = pending_chapter_start.take() {
                    chapter_starts.push(start_idx);
                }
                let show_rule = size.width >= 12;
                let prefix = if show_rule { "│ " } else { "  " };
                let max_width = size.width as usize;
                let highlighted = highlight::highlight_code(lang.as_deref(), text);
                for line in highlighted {
                    let mut segs = Vec::new();
                    segs.push(Segment {
                        text: prefix.to_string(),
                        fg: None,
                        bg: None,
                        style: TextStyle::default(),
                        link: None,
                    });
                    for span in line.spans {
                        segs.push(Segment {
                            text: span.text,
                            fg: span.fg.map(|c| crate::types::RgbColor {
                                r: c.r,
                                g: c.g,
                                b: c.b,
                            }),
                            bg: span.bg.map(|c| crate::types::RgbColor {
                                r: c.r,
                                g: c.g,
                                b: c.b,
                            }),
                            style: TextStyle::default(),
                            link: None,
                        });
                    }
                    let clipped = clip_segments(segs, max_width.max(4));
                    push_line(
                        clipped,
                        &[],
                        &mut pages,
                        &mut current,
                        &mut at_page_index,
                        &mut anchors,
                    );
                }
                push_line(
                    StyledLine::from_plain(String::new()),
                    &[],
                    &mut pages,
                    &mut current,
                    &mut at_page_index,
                    &mut anchors,
                );
            }
            Block::Image(image) => {
                if let Some(start_idx) = pending_chapter_start.take() {
                    chapter_starts.push(start_idx);
                }
                let mut caption = image.caption.clone().or_else(|| image.alt.clone());
                if caption.is_none() && image.data.is_none() {
                    caption = Some("Image".to_string());
                }
                if image.data.is_some() {
                    let cols = size.width.max(1);
                    let max_rows = size.height.saturating_sub(2).max(3);
                    let rows = image_rows_from_dims(image.width, image.height, cols, max_rows);
                    let blank = " ".repeat(cols as usize);
                    for row in 0..rows {
                        let mut line = StyledLine::from_plain(blank.clone());
                        if row == 0 {
                            line.image = Some(ImagePlacement {
                                id: image.id.clone(),
                                cols,
                                rows,
                            });
                        }
                        push_line(
                            line,
                            &[],
                            &mut pages,
                            &mut current,
                            &mut at_page_index,
                            &mut anchors,
                        );
                    }
                }
                if let Some(caption) = caption {
                    let wrapped = wrap_styled_text(&caption, size.width as usize);
                    for i in 0..wrapped.lines.len() {
                        let anchors_for_line = &wrapped.anchors[i];
                        push_line(
                            wrapped.lines[i].clone(),
                            anchors_for_line,
                            &mut pages,
                            &mut current,
                            &mut at_page_index,
                            &mut anchors,
                        );
                    }
                }
                push_line(
                    StyledLine::from_plain(String::new()),
                    &[],
                    &mut pages,
                    &mut current,
                    &mut at_page_index,
                    &mut anchors,
                );
            }
        }
    }
    if !current.lines.is_empty() {
        pages.push(current);
    }
    Pagination {
        pages,
        chapter_starts,
        anchors,
    }
}

fn wrap_styled_text(text: &str, width: usize) -> WrappedLines {
    let width = width.max(1);
    let pieces = parse_inline_pieces(text);
    let tokens = tokenize_pieces(pieces);
    wrap_tokens(tokens, width)
}

fn render_table(table: &TableBlock, width: usize) -> Vec<(StyledLine, Vec<String>)> {
    let width = width.max(1);
    if table.rows.is_empty() {
        return Vec::new();
    }
    let col_count = table.rows.iter().map(|row| row.len()).max().unwrap_or(0);
    if col_count == 0 {
        return Vec::new();
    }

    let sep = table_separator(width, col_count);
    let sep_width = sep.graphemes(true).count();
    let available_cells = width.saturating_sub(sep_width * col_count.saturating_sub(1));
    let max_widths = table_max_widths(table, col_count);
    let col_widths = compute_column_widths(&max_widths, available_cells);

    let header_end = table_header_end(&table.rows);
    let mut out: Vec<(StyledLine, Vec<String>)> = Vec::new();

    for (row_idx, row) in table.rows.iter().enumerate() {
        let row_has_header = row.iter().any(|cell| cell.is_header);
        let mut wrapped_cells: Vec<WrappedLines> = Vec::with_capacity(col_count);
        for col in 0..col_count {
            let text = row.get(col).map(|cell| cell.text.trim()).unwrap_or("");
            let wrapped = wrap_styled_text(text, col_widths[col].max(1));
            wrapped_cells.push(wrapped);
        }
        let row_height = wrapped_cells
            .iter()
            .map(|w| w.lines.len())
            .max()
            .unwrap_or(1);

        for line_idx in 0..row_height {
            let mut segments: Vec<Segment> = Vec::new();
            let mut line_anchors: Vec<String> = Vec::new();
            for col in 0..col_count {
                let wrapped = &wrapped_cells[col];
                let line = wrapped
                    .lines
                    .get(line_idx)
                    .cloned()
                    .unwrap_or_else(|| StyledLine::from_plain(String::new()));
                let mut segs = line.segments;
                if row_has_header {
                    for seg in &mut segs {
                        seg.style.bold = true;
                    }
                }
                let current_width = line_width(&StyledLine {
                    segments: segs.clone(),
                    image: None,
                });
                let pad = col_widths[col].saturating_sub(current_width);
                if pad > 0 {
                    segs.push(Segment {
                        text: " ".repeat(pad),
                        fg: None,
                        bg: None,
                        style: TextStyle::default(),
                        link: None,
                    });
                }
                segments.extend(segs);
                if col + 1 < col_count && !sep.is_empty() {
                    segments.push(Segment {
                        text: sep.to_string(),
                        fg: None,
                        bg: None,
                        style: TextStyle::default(),
                        link: None,
                    });
                }
                if let Some(anchors) = wrapped.anchors.get(line_idx) {
                    line_anchors.extend(anchors.iter().cloned());
                }
            }
            out.push((
                StyledLine {
                    segments,
                    image: None,
                },
                line_anchors,
            ));
        }

        if header_end == Some(row_idx) {
            out.push((table_rule_line(&col_widths, sep), Vec::new()));
        }
    }

    out
}

fn table_separator(width: usize, cols: usize) -> &'static str {
    if cols <= 1 {
        ""
    } else if width >= cols + (cols - 1) * 3 {
        " | "
    } else if width >= cols + (cols - 1) {
        " "
    } else {
        ""
    }
}

fn table_header_end(rows: &[Vec<TableCell>]) -> Option<usize> {
    let mut last_header: Option<usize> = None;
    let mut seen_header = false;
    for (idx, row) in rows.iter().enumerate() {
        let is_header = row.iter().any(|cell| cell.is_header);
        if is_header {
            last_header = Some(idx);
            seen_header = true;
        } else if seen_header {
            break;
        }
    }
    last_header
}

fn table_rule_line(widths: &[usize], sep: &str) -> StyledLine {
    if widths.is_empty() {
        return StyledLine::from_plain(String::new());
    }
    let rule_sep = if sep == " | " { "-+-" } else { sep };
    let mut out = String::new();
    for (idx, width) in widths.iter().enumerate() {
        let width = (*width).max(1);
        out.push_str(&"-".repeat(width));
        if idx + 1 < widths.len() {
            out.push_str(rule_sep);
        }
    }
    StyledLine::from_plain(out)
}

fn table_max_widths(table: &TableBlock, cols: usize) -> Vec<usize> {
    let mut widths = vec![0usize; cols];
    for row in &table.rows {
        for (idx, cell) in row.iter().enumerate() {
            if idx >= cols {
                continue;
            }
            let plain = strip_style_markers(&cell.text);
            let cell_max = plain
                .split('\n')
                .map(|line| line.graphemes(true).count())
                .max()
                .unwrap_or(0);
            if cell_max > widths[idx] {
                widths[idx] = cell_max;
            }
        }
    }
    widths
}

fn compute_column_widths(max_widths: &[usize], available: usize) -> Vec<usize> {
    let cols = max_widths.len();
    if cols == 0 {
        return Vec::new();
    }
    let min_width = if available >= cols * 3 { 3 } else { 1 };
    let mut widths = vec![min_width; cols];
    let mut remaining = available.saturating_sub(min_width * cols);
    let mut capacity: Vec<usize> = max_widths
        .iter()
        .map(|w| w.saturating_sub(min_width))
        .collect();

    while remaining > 0 {
        let mut best_idx: Option<usize> = None;
        let mut best_cap = 0usize;
        for (idx, cap) in capacity.iter().enumerate() {
            if *cap > best_cap {
                best_cap = *cap;
                best_idx = Some(idx);
            }
        }
        let Some(idx) = best_idx else {
            break;
        };
        widths[idx] += 1;
        capacity[idx] = capacity[idx].saturating_sub(1);
        remaining -= 1;
    }

    widths
}

fn image_rows_from_dims(width: Option<u32>, height: Option<u32>, cols: u16, max_rows: u16) -> u16 {
    let cols = cols.max(1) as f32;
    let mut rows = if let (Some(w), Some(h)) = (width, height) {
        let ratio = h as f32 / w.max(1) as f32;
        (ratio * cols).ceil() as u16
    } else {
        6
    };
    rows = rows.max(3);
    rows.min(max_rows.max(3))
}

fn wrap_tokens(tokens: Vec<InlineToken>, width: usize) -> WrappedLines {
    let mut lines: Vec<StyledLine> = Vec::new();
    let mut anchors: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<Segment> = Vec::new();
    let mut current_anchors: Vec<String> = Vec::new();
    let mut line_width = 0usize;
    let mut pending_space: Option<(TextStyle, Option<String>)> = None;

    let push_current = |lines: &mut Vec<StyledLine>,
                        anchors: &mut Vec<Vec<String>>,
                        current: &mut Vec<Segment>,
                        current_anchors: &mut Vec<String>,
                        line_width: &mut usize| {
        lines.push(StyledLine {
            segments: std::mem::take(current),
            image: None,
        });
        anchors.push(std::mem::take(current_anchors));
        *line_width = 0;
    };

    for token in tokens {
        match token {
            InlineToken::Space(style, link) => {
                pending_space = Some((style, link));
            }
            InlineToken::Anchor(target) => {
                if !target.is_empty() {
                    current_anchors.push(target);
                }
            }
            InlineToken::Newline => {
                pending_space = None;
                push_current(
                    &mut lines,
                    &mut anchors,
                    &mut current,
                    &mut current_anchors,
                    &mut line_width,
                );
            }
            InlineToken::Word(word) => {
                let space_style = pending_space.take();
                let space_width = if space_style.is_some() && !current.is_empty() {
                    1
                } else {
                    0
                };
                if line_width + space_width + word.width <= width {
                    if let Some((style, link)) = space_style {
                        if !current.is_empty() {
                            current.push(space_segment(style, link));
                            line_width += 1;
                        }
                    }
                    current.extend(word.segments);
                    line_width += word.width;
                } else {
                    if !current.is_empty() {
                        push_current(
                            &mut lines,
                            &mut anchors,
                            &mut current,
                            &mut current_anchors,
                            &mut line_width,
                        );
                    }
                    if word.width > width {
                        let parts = split_word_segments(&word.segments, width);
                        let parts_len = parts.len();
                        for (idx, part) in parts.into_iter().enumerate() {
                            if idx + 1 == parts_len {
                                current = part.segments;
                                line_width = part.width;
                            } else {
                                lines.push(StyledLine {
                                    segments: part.segments,
                                    image: None,
                                });
                                anchors.push(Vec::new());
                            }
                        }
                    } else {
                        current = word.segments;
                        line_width = word.width;
                    }
                }
                pending_space = None;
            }
        }
    }

    if !current.is_empty() || lines.is_empty() || !current_anchors.is_empty() {
        lines.push(StyledLine {
            segments: current,
            image: None,
        });
        anchors.push(current_anchors);
    }
    WrappedLines { lines, anchors }
}

fn parse_inline_pieces(text: &str) -> Vec<InlinePiece> {
    let mut pieces: Vec<InlinePiece> = Vec::new();
    let mut current = String::new();
    let mut counts = StyleCounts::default();
    let mut current_link: Option<String> = None;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == STYLE_START || ch == STYLE_END {
            let is_start = ch == STYLE_START;
            let Some(code) = chars.next() else {
                current.push(ch);
                break;
            };
            if matches!(code, 'b' | 'i' | 'u' | 'c' | 'x' | 's') {
                if !current.is_empty() {
                    pieces.push(InlinePiece::Span(InlineSpan {
                        text: std::mem::take(&mut current),
                        style: style_from_counts(&counts),
                        link: current_link.clone(),
                    }));
                }
                apply_style_code(&mut counts, code, is_start);
                continue;
            }
            current.push(ch);
            current.push(code);
            continue;
        }
        if ch == LINK_START {
            let mut target = String::new();
            let mut found_end = false;
            while let Some(next) = chars.next() {
                if next == LINK_END {
                    found_end = true;
                    break;
                }
                target.push(next);
            }
            if !found_end {
                current.push(ch);
                current.push_str(&target);
                break;
            }
            if !current.is_empty() {
                pieces.push(InlinePiece::Span(InlineSpan {
                    text: std::mem::take(&mut current),
                    style: style_from_counts(&counts),
                    link: current_link.clone(),
                }));
            }
            if target.is_empty() {
                current_link = None;
            } else {
                current_link = Some(target);
            }
            continue;
        }
        if ch == ANCHOR_START {
            let mut target = String::new();
            let mut found_end = false;
            while let Some(next) = chars.next() {
                if next == ANCHOR_END {
                    found_end = true;
                    break;
                }
                target.push(next);
            }
            if !found_end {
                current.push(ch);
                current.push_str(&target);
                break;
            }
            if !current.is_empty() {
                pieces.push(InlinePiece::Span(InlineSpan {
                    text: std::mem::take(&mut current),
                    style: style_from_counts(&counts),
                    link: current_link.clone(),
                }));
            }
            let target = target.trim().to_string();
            if !target.is_empty() {
                pieces.push(InlinePiece::Anchor(target));
            }
            continue;
        }
        current.push(ch);
    }
    if !current.is_empty() {
        pieces.push(InlinePiece::Span(InlineSpan {
            text: current,
            style: style_from_counts(&counts),
            link: current_link,
        }));
    }
    pieces
}

fn style_from_counts(counts: &StyleCounts) -> TextStyle {
    TextStyle {
        bold: counts.bold > 0,
        italic: counts.italic > 0,
        underline: counts.underline > 0,
        dim: counts.code > 0,
        reverse: counts.code > 0,
        strike: counts.strike > 0,
        small_caps: counts.small_caps > 0,
    }
}

fn apply_style_code(counts: &mut StyleCounts, code: char, is_start: bool) -> bool {
    let target = match code {
        'b' => &mut counts.bold,
        'i' => &mut counts.italic,
        'u' => &mut counts.underline,
        'c' => &mut counts.code,
        'x' => &mut counts.strike,
        's' => &mut counts.small_caps,
        _ => return false,
    };
    if is_start {
        *target = target.saturating_add(1);
    } else {
        *target = target.saturating_sub(1);
    }
    true
}

fn tokenize_pieces(pieces: Vec<InlinePiece>) -> Vec<InlineToken> {
    let mut tokens: Vec<InlineToken> = Vec::new();
    let mut current_segments: Vec<Segment> = Vec::new();
    let mut current_width = 0usize;

    let flush_word = |tokens: &mut Vec<InlineToken>,
                      current_segments: &mut Vec<Segment>,
                      current_width: &mut usize| {
        if !current_segments.is_empty() {
            tokens.push(InlineToken::Word(InlineWord {
                segments: std::mem::take(current_segments),
                width: *current_width,
            }));
            *current_width = 0;
        }
    };

    for piece in pieces {
        match piece {
            InlinePiece::Anchor(target) => {
                flush_word(&mut tokens, &mut current_segments, &mut current_width);
                tokens.push(InlineToken::Anchor(target));
            }
            InlinePiece::Span(span) => {
                let style = span.style;
                let link = span.link.clone();
                for g in span.text.graphemes(true) {
                    if g == "\n" {
                        flush_word(&mut tokens, &mut current_segments, &mut current_width);
                        tokens.push(InlineToken::Newline);
                        continue;
                    }
                    if g.chars().all(|c| c.is_whitespace()) {
                        flush_word(&mut tokens, &mut current_segments, &mut current_width);
                        if !matches!(
                            tokens.last(),
                            Some(InlineToken::Space(..) | InlineToken::Newline)
                        ) {
                            tokens.push(InlineToken::Space(style, link.clone()));
                        }
                        continue;
                    }
                    if let Some(last) = current_segments.last_mut() {
                        if last.style == style
                            && last.fg.is_none()
                            && last.bg.is_none()
                            && last.link == link
                        {
                            last.text.push_str(g);
                        } else {
                            current_segments.push(Segment {
                                text: g.to_string(),
                                fg: None,
                                bg: None,
                                style,
                                link: link.clone(),
                            });
                        }
                    } else {
                        current_segments.push(Segment {
                            text: g.to_string(),
                            fg: None,
                            bg: None,
                            style,
                            link: link.clone(),
                        });
                    }
                    current_width += 1;
                }
            }
        }
    }
    flush_word(&mut tokens, &mut current_segments, &mut current_width);
    tokens
}

fn split_word_segments(segments: &[Segment], width: usize) -> Vec<InlineWord> {
    let mut parts: Vec<InlineWord> = Vec::new();
    let mut current: Vec<Segment> = Vec::new();
    let mut used = 0usize;
    for seg in segments {
        for g in seg.text.graphemes(true) {
            if used >= width && !current.is_empty() {
                parts.push(InlineWord {
                    segments: std::mem::take(&mut current),
                    width: used,
                });
                used = 0;
            }
            if let Some(last) = current.last_mut() {
                if last.style == seg.style
                    && last.fg.is_none()
                    && last.bg.is_none()
                    && last.link == seg.link
                {
                    last.text.push_str(g);
                } else {
                    current.push(Segment {
                        text: g.to_string(),
                        fg: None,
                        bg: None,
                        style: seg.style,
                        link: seg.link.clone(),
                    });
                }
            } else {
                current.push(Segment {
                    text: g.to_string(),
                    fg: None,
                    bg: None,
                    style: seg.style,
                    link: seg.link.clone(),
                });
            }
            used += 1;
            if used == width {
                parts.push(InlineWord {
                    segments: std::mem::take(&mut current),
                    width: used,
                });
                used = 0;
            }
        }
    }
    if !current.is_empty() {
        parts.push(InlineWord {
            segments: current,
            width: used,
        });
    }
    if parts.is_empty() {
        parts.push(InlineWord {
            segments: Vec::new(),
            width: 0,
        });
    }
    parts
}

fn segments_from_text_with_anchors(text: &str) -> (Vec<Segment>, Vec<String>) {
    let pieces = parse_inline_pieces(text);
    let mut segments: Vec<Segment> = Vec::new();
    let mut anchors: Vec<String> = Vec::new();
    for piece in pieces {
        match piece {
            InlinePiece::Anchor(target) => {
                if !target.is_empty() {
                    anchors.push(target);
                }
            }
            InlinePiece::Span(span) => {
                if span.text.is_empty() {
                    continue;
                }
                if let Some(last) = segments.last_mut() {
                    if last.style == span.style
                        && last.fg.is_none()
                        && last.bg.is_none()
                        && last.link == span.link
                    {
                        last.text.push_str(&span.text);
                        continue;
                    }
                }
                segments.push(Segment {
                    text: span.text,
                    fg: None,
                    bg: None,
                    style: span.style,
                    link: span.link,
                });
            }
        }
    }
    (segments, anchors)
}

fn space_segment(style: TextStyle, link: Option<String>) -> Segment {
    Segment {
        text: " ".to_string(),
        fg: None,
        bg: None,
        style,
        link,
    }
}

fn justify_styled_line(line: &StyledLine, width: usize) -> StyledLine {
    let current_len = line_width(line);
    if current_len >= width {
        return line.clone();
    }
    if current_len * 10 < width * 7 {
        return line.clone();
    }
    let gaps: Vec<usize> = line
        .segments
        .iter()
        .enumerate()
        .filter_map(|(idx, seg)| (is_space_segment(seg)).then_some(idx))
        .collect();
    if gaps.len() < 3 {
        return line.clone();
    }
    let extra = width.saturating_sub(current_len);
    if extra == 0 {
        return line.clone();
    }
    let mut out = line.clone();
    let base = extra / gaps.len();
    let mut remainder = extra % gaps.len();
    for idx in gaps {
        let mut add = base;
        if remainder > 0 {
            add += 1;
            remainder -= 1;
        }
        if add > 0 {
            out.segments[idx].text.push_str(&" ".repeat(add));
        }
    }
    out
}

fn line_width(line: &StyledLine) -> usize {
    line.segments
        .iter()
        .map(|seg| seg.text.graphemes(true).count())
        .sum()
}

fn is_space_segment(seg: &Segment) -> bool {
    !seg.text.is_empty() && seg.text.chars().all(|c| c == ' ')
}

fn uppercase_segments(segments: &mut [Segment]) {
    for seg in segments {
        if seg.text.is_empty() {
            continue;
        }
        let mut out = String::with_capacity(seg.text.len());
        for ch in seg.text.chars() {
            if ch.is_ascii() {
                out.push(ch.to_ascii_uppercase());
            } else {
                out.push(ch);
            }
        }
        seg.text = out;
    }
}

fn clip_segments(segments: Vec<Segment>, width: usize) -> StyledLine {
    let mut out = Vec::new();
    let mut used = 0usize;
    for seg in segments {
        if used >= width {
            break;
        }
        let mut buf = String::new();
        for g in seg.text.graphemes(true) {
            if used >= width {
                break;
            }
            buf.push_str(g);
            used += 1;
        }
        if !buf.is_empty() {
            out.push(Segment {
                text: buf,
                fg: seg.fg,
                bg: seg.bg,
                style: seg.style,
                link: seg.link.clone(),
            });
        }
        if used >= width {
            out.push(Segment {
                text: "…".into(),
                fg: seg.fg,
                bg: seg.bg,
                style: seg.style,
                link: seg.link,
            });
            break;
        }
    }
    StyledLine {
        segments: out,
        image: None,
    }
}

impl StyledLine {
    pub fn from_plain(text: String) -> Self {
        Self {
            segments: vec![Segment {
                text,
                fg: None,
                bg: None,
                style: TextStyle::default(),
                link: None,
            }],
            image: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_words_from_paragraph() {
        let blocks = vec![Block::Paragraph("Hello world!".to_string())];
        let words = extract_words(&blocks);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "Hello");
        assert!(!words[0].is_sentence_end);
        assert_eq!(words[1].text, "world!");
        assert!(words[1].is_sentence_end);
    }

    #[test]
    fn extract_words_from_heading() {
        let blocks = vec![Block::Heading("Chapter One".to_string(), 1)];
        let words = extract_words(&blocks);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "Chapter");
        assert_eq!(words[1].text, "One");
    }

    #[test]
    fn extract_words_from_list() {
        let blocks = vec![Block::List(vec![
            "First item".to_string(),
            "Second item".to_string(),
        ])];
        let words = extract_words(&blocks);
        assert_eq!(words.len(), 4);
        assert_eq!(words[0].text, "First");
        assert_eq!(words[1].text, "item");
        assert_eq!(words[2].text, "Second");
        assert_eq!(words[3].text, "item");
    }

    #[test]
    fn extract_words_from_quote() {
        let blocks = vec![Block::Quote("A quote here.".to_string())];
        let words = extract_words(&blocks);
        assert_eq!(words.len(), 3);
        assert_eq!(words[2].text, "here.");
        assert!(words[2].is_sentence_end);
    }

    #[test]
    fn skip_code_blocks() {
        let blocks = vec![Block::Code {
            lang: Some("rust".to_string()),
            text: "fn main() {}".to_string(),
        }];
        let words = extract_words(&blocks);
        assert!(words.is_empty());
    }

    #[test]
    fn skip_image_placeholders() {
        let blocks = vec![Block::Paragraph("[image]".to_string())];
        let words = extract_words(&blocks);
        assert!(words.is_empty());
    }

    #[test]
    fn detect_sentence_end_punctuation() {
        let blocks = vec![Block::Paragraph("Hello world. Goodbye? Yes!".to_string())];
        let words = extract_words(&blocks);
        assert!(words[1].is_sentence_end);
        assert!(words[2].is_sentence_end);
        assert!(words[3].is_sentence_end);
    }

    #[test]
    fn detect_comma_punctuation() {
        let blocks = vec![Block::Paragraph("First, second, third".to_string())];
        let words = extract_words(&blocks);
        assert!(words[0].is_comma);
        assert!(words[1].is_comma);
        assert!(!words[2].is_comma);
    }

    #[test]
    fn track_chapters() {
        let blocks = vec![
            Block::Paragraph("Chapter one text".to_string()),
            Block::Paragraph(String::new()),
            Block::Paragraph("───".to_string()),
            Block::Paragraph(String::new()),
            Block::Paragraph("Chapter two text".to_string()),
        ];
        let words = extract_words(&blocks);
        assert_eq!(words.len(), 6);
        assert_eq!(words[0].chapter_index, None);
        assert_eq!(words[3].chapter_index, Some(1));
    }
}
