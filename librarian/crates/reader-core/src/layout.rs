use crate::types::Block;

#[derive(Clone, Copy)]
pub struct Size {
    pub width: u16,
    pub height: u16,
}

#[derive(Clone)]
pub struct Page {
    pub lines: Vec<String>,
}

#[derive(Clone)]
pub struct Pagination {
    pub pages: Vec<Page>,
    pub chapter_starts: Vec<usize>, // page indices where a chapter begins
}

pub fn paginate(blocks: &[Block], size: Size) -> Vec<Page> {
    paginate_with_justify(blocks, size, false).pages
}

pub fn paginate_with_justify(blocks: &[Block], size: Size, justify: bool) -> Pagination {
    // Greedy wrap with optional full justification
    let mut pages: Vec<Page> = Vec::new();
    let mut current = Page { lines: Vec::new() };
    let mut chapter_starts: Vec<usize> = Vec::new();
    let mut at_page_index: usize = pages.len();
    let mut pending_chapter_start: Option<usize> = Some(0); // initial chapter starts at page 0
    for block in blocks {
        match block {
            Block::Paragraph(text) | Block::Quote(text) => {
                // If a separator was seen, mark the next content start as a chapter start
                if let Some(start_idx) = pending_chapter_start.take() {
                    chapter_starts.push(start_idx);
                }
                // Detect separator and set the next start index
                if text.trim() == "───" {
                    pending_chapter_start = Some(at_page_index);
                }
                let lines = wrap_text(text, size.width as usize);
                for i in 0..lines.len() {
                    let is_last = i == lines.len() - 1;
                    let line = if justify && !is_last {
                        justify_line(&lines[i], size.width as usize)
                    } else {
                        lines[i].clone()
                    };
                    current.lines.push(line);
                    if current.lines.len() as u16 >= size.height {
                        pages.push(current.clone());
                        current = Page { lines: Vec::new() };
                        at_page_index += 1;
                    }
                }
                // blank line between paragraphs
                current.lines.push(String::new());
            }
            Block::Heading(text, _) => {
                if let Some(start_idx) = pending_chapter_start.take() {
                    chapter_starts.push(start_idx);
                }
                let heading = text.to_uppercase();
                current.lines.push(heading);
                current.lines.push(String::new());
            }
            Block::List(items) => {
                if let Some(start_idx) = pending_chapter_start.take() {
                    chapter_starts.push(start_idx);
                }
                for item in items {
                    let line = format!("• {}", item);
                    let lines = wrap_text(&line, size.width as usize);
                    for i in 0..lines.len() {
                        let is_last = i == lines.len() - 1;
                        let out = if justify && !is_last {
                            justify_line(&lines[i], size.width as usize)
                        } else {
                            lines[i].clone()
                        };
                        current.lines.push(out);
                        if current.lines.len() as u16 >= size.height {
                            pages.push(current.clone());
                            current = Page { lines: Vec::new() };
                            at_page_index += 1;
                        }
                    }
                }
                current.lines.push(String::new());
            }
            Block::Code { text, .. } => {
                if let Some(start_idx) = pending_chapter_start.take() {
                    chapter_starts.push(start_idx);
                }
                for line in text.lines() {
                    current.lines.push(line.to_string());
                    if current.lines.len() as u16 >= size.height {
                        pages.push(current.clone());
                        current = Page { lines: Vec::new() };
                        at_page_index += 1;
                    }
                }
                current.lines.push(String::new());
            }
        }
    }
    if !current.lines.is_empty() {
        pages.push(current);
    }
    Pagination {
        pages,
        chapter_starts,
    }
}

use unicode_linebreak::{linebreaks, BreakOpportunity};
use unicode_segmentation::UnicodeSegmentation;

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    let mut line_len = 0usize; // grapheme count
    for token in text.split_whitespace() {
        let token_len = token.graphemes(true).count();
        if line.is_empty() {
            // Try to place entire token
            if token_len <= width {
                line.push_str(token);
                line_len = token_len;
            } else {
                // Soft-break long token using unicode_linebreak opportunities
                let mut start = 0usize;
                for (idx, opp) in linebreaks(token) {
                    if matches!(opp, BreakOpportunity::Mandatory | BreakOpportunity::Allowed) {
                        let part = &token[start..idx];
                        let part_len = part.graphemes(true).count();
                        if part_len > 0 {
                            out.push(part.to_string());
                        }
                        start = idx;
                    }
                }
                // Tail
                if start < token.len() {
                    out.push(token[start..].to_string());
                }
                line.clear();
                line_len = 0;
            }
        } else if line_len + 1 + token_len <= width {
            line.push(' ');
            line.push_str(token);
            line_len += 1 + token_len;
        } else {
            // Flush current line
            out.push(line);
            line = String::new();
            line_len = 0;
            // Place token (may still be too long)
            if token_len <= width {
                line.push_str(token);
                line_len = token_len;
            } else {
                // Break within token
                let mut start = 0usize;
                let mut acc = String::new();
                let mut acc_len = 0usize;
                for (idx, opp) in linebreaks(token) {
                    if matches!(opp, BreakOpportunity::Mandatory | BreakOpportunity::Allowed) {
                        let part = &token[start..idx];
                        let part_len = part.graphemes(true).count();
                        if acc_len == 0 {
                            if part_len <= width {
                                acc.push_str(part);
                                acc_len = part_len;
                            } else {
                                out.push(part.to_string());
                            }
                        } else if acc_len + 1 + part_len <= width {
                            acc.push(' ');
                            acc.push_str(part);
                            acc_len += 1 + part_len;
                        } else {
                            out.push(acc);
                            acc = part.to_string();
                            acc_len = part_len;
                        }
                        start = idx;
                    }
                }
                // Tail
                let tail = &token[start..];
                let tail_len = tail.graphemes(true).count();
                if acc_len == 0 {
                    if tail_len <= width {
                        line = tail.to_string();
                        line_len = tail_len;
                    } else {
                        out.push(tail.to_string());
                    }
                } else if acc_len + 1 + tail_len <= width {
                    acc.push(' ');
                    acc.push_str(tail);
                    out.push(acc);
                    line.clear();
                    line_len = 0;
                } else {
                    out.push(acc);
                    line = tail.to_string();
                    line_len = tail_len;
                }
            }
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

fn justify_line(line: &str, width: usize) -> String {
    // Do not justify single-word or already full/overfull lines
    if !line.contains(' ') || line.graphemes(true).count() >= width {
        return line.to_string();
    }
    let words: Vec<&str> = line.split(' ').collect();
    let gaps = words.len().saturating_sub(1);
    if gaps == 0 {
        return line.to_string();
    }

    // Current grapheme length includes existing single spaces
    let current_len = line.graphemes(true).count();
    let extra = width.saturating_sub(current_len);
    if extra == 0 {
        return line.to_string();
    }

    // Build with extra spaces distributed across gaps; no trailing spaces
    let mut out = String::new();
    let base = extra / gaps;
    let mut remainder = extra % gaps;
    for (i, w) in words.iter().enumerate() {
        out.push_str(w);
        if i < gaps {
            // one existing space + extra padding
            out.push(' ');
            for _ in 0..base {
                out.push(' ');
            }
            if remainder > 0 {
                out.push(' ');
                remainder -= 1;
            }
        }
    }

    // Clamp by grapheme count if overshoot
    let count = out.graphemes(true).count();
    if count > width {
        // Truncate by grapheme count
        let mut acc = String::new();
        for (used, g) in out.graphemes(true).enumerate() {
            if used >= width {
                break;
            }
            acc.push_str(g);
        }
        return acc;
    }
    out
}
