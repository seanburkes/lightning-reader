use crate::types::Block;
use highlight;
use std::collections::HashMap;

use super::{ImagePlacement, Page, Pagination, Segment, Size, StyledLine, TextStyle};
use super::inline::{
    clip_segments, justify_styled_line, segments_from_text_with_anchors, uppercase_segments,
    wrap_styled_text,
};
use super::table::render_table;
use super::is_chapter_separator;

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
                            fg: span.fg.map(|c| crate::types::RgbColor::new(c.r, c.g, c.b)),
                            bg: span.bg.map(|c| crate::types::RgbColor::new(c.r, c.g, c.b)),
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
                let mut caption = image
                    .caption()
                    .map(str::to_string)
                    .or_else(|| image.alt().map(str::to_string));
                if caption.is_none() && image.data().is_none() {
                    caption = Some("Image".to_string());
                }
                if image.data().is_some() {
                    let cols = size.width.max(1);
                    let max_rows = size.height.saturating_sub(2).max(3);
                    let rows = image_rows_from_dims(image.width(), image.height(), cols, max_rows);
                    let blank = " ".repeat(cols as usize);
                    for row in 0..rows {
                        let mut line = StyledLine::from_plain(blank.clone());
                        if row == 0 {
                            line.image = Some(ImagePlacement {
                                id: image.id().to_string(),
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
