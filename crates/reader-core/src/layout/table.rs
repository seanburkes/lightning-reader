use super::{Segment, StyledLine, TextStyle};
use crate::types::{TableBlock, TableCell};
use unicode_segmentation::UnicodeSegmentation;

use super::inline::{line_width, strip_style_markers, wrap_styled_text, WrappedLines};

pub(crate) fn render_table(table: &TableBlock, width: usize) -> Vec<(StyledLine, Vec<String>)> {
    let width = width.max(1);
    if table.rows().is_empty() {
        return Vec::new();
    }
    let col_count = table.rows().iter().map(|row| row.len()).max().unwrap_or(0);
    if col_count == 0 {
        return Vec::new();
    }

    let sep = table_separator(width, col_count);
    let sep_width = sep.graphemes(true).count();
    let available_cells = width.saturating_sub(sep_width * col_count.saturating_sub(1));
    let max_widths = table_max_widths(table, col_count);
    let col_widths = compute_column_widths(&max_widths, available_cells);

    let header_end = table_header_end(table.rows());
    let mut out: Vec<(StyledLine, Vec<String>)> = Vec::new();

    for (row_idx, row) in table.rows().iter().enumerate() {
        let row_has_header = row.iter().any(|cell| cell.is_header());
        let mut wrapped_cells: Vec<WrappedLines> = Vec::with_capacity(col_count);
        for (col, col_width) in col_widths.iter().enumerate().take(col_count) {
            let text = row.get(col).map(|cell| cell.text().trim()).unwrap_or("");
            let wrapped = wrap_styled_text(text, (*col_width).max(1));
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
            for (col, wrapped) in wrapped_cells.iter().enumerate().take(col_count) {
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
        let is_header = row.iter().any(|cell| cell.is_header());
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
    for row in table.rows() {
        for (idx, cell) in row.iter().enumerate() {
            if idx >= cols {
                continue;
            }
            let plain = strip_style_markers(cell.text());
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
