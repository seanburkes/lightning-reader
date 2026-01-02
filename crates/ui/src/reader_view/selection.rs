use ratatui::prelude::Color;
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;

use reader_core::layout::StyledLine;

use super::ReaderView;

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

pub(super) fn selection_for_line(
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

impl ReaderView {
    pub(super) fn selection_line(
        line: &StyledLine,
        sel_start: usize,
        sel_end: usize,
    ) -> Line<'static> {
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
        Line::from(spans)
    }
}
