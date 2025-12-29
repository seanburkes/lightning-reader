use crate::types::Block;
use highlight;
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
}

#[derive(Clone)]
pub struct Segment {
    pub text: String,
    pub fg: Option<crate::types::RgbColor>,
    pub bg: Option<crate::types::RgbColor>,
}

#[derive(Clone)]
pub struct Pagination {
    pub pages: Vec<Page>,
    pub chapter_starts: Vec<usize>, // page indices where a chapter begins
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
                if text.trim() == "───" {
                    if is_chapter_separator(blocks, idx) {
                        chapter_counter += 1;
                        current_chapter = Some(chapter_counter);
                    }
                    continue;
                }
                if text.trim() == "[image]" {
                    continue;
                }
                for word in text.split_whitespace() {
                    let token = WordToken::from_word(word.to_string(), current_chapter);
                    words.push(token);
                }
            }
            Block::Heading(text, _) => {
                for word in text.split_whitespace() {
                    let token = WordToken::from_word(word.to_string(), current_chapter);
                    words.push(token);
                }
            }
            Block::List(items) => {
                for item in items {
                    for word in item.split_whitespace() {
                        let token = WordToken::from_word(word.to_string(), current_chapter);
                        words.push(token);
                    }
                }
            }
            Block::Quote(text) => {
                for word in text.split_whitespace() {
                    let token = WordToken::from_word(word.to_string(), current_chapter);
                    words.push(token);
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

pub fn paginate(blocks: &[Block], size: Size) -> Vec<Page> {
    paginate_with_justify(blocks, size, false).pages
}

pub fn paginate_with_justify(blocks: &[Block], size: Size, justify: bool) -> Pagination {
    // Greedy wrap with optional full justification
    let mut pages: Vec<Page> = Vec::new();
    let mut current = Page { lines: Vec::new() };
    let mut chapter_starts: Vec<usize> = Vec::new();
    let mut at_page_index: usize = pages.len();
    let mut push_line = |line: StyledLine,
                         pages: &mut Vec<Page>,
                         current: &mut Page,
                         at_page_index: &mut usize| {
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
                let lines = wrap_text(text, size.width as usize);
                for i in 0..lines.len() {
                    let is_last = i == lines.len() - 1;
                    let line = if justify && !is_last {
                        justify_line(&lines[i], size.width as usize)
                    } else {
                        lines[i].clone()
                    };
                    push_line(
                        StyledLine::from_plain(line),
                        &mut pages,
                        &mut current,
                        &mut at_page_index,
                    );
                }
                // blank line between paragraphs
                push_line(
                    StyledLine::from_plain(String::new()),
                    &mut pages,
                    &mut current,
                    &mut at_page_index,
                );
            }
            Block::Quote(text) => {
                if let Some(start_idx) = pending_chapter_start.take() {
                    chapter_starts.push(start_idx);
                }
                // Two-space indent; add a rule when there is room
                let show_rule = size.width >= 16;
                let prefix = if show_rule { "│ " } else { "  " };
                let prefix_width = prefix.graphemes(true).count() as u16;
                let eff_width = size.width.saturating_sub(prefix_width) as usize;
                // Preserve line breaks like a code/pre block; truncate when too long
                for raw_line in text.lines() {
                    let clipped = truncate_graphemes(raw_line, eff_width.max(4));
                    let prefixed = format!("{}{}", prefix, clipped);
                    push_line(
                        StyledLine::from_plain(prefixed),
                        &mut pages,
                        &mut current,
                        &mut at_page_index,
                    );
                }
                push_line(
                    StyledLine::from_plain(String::new()),
                    &mut pages,
                    &mut current,
                    &mut at_page_index,
                );
            }
            Block::Heading(text, _) => {
                if let Some(start_idx) = pending_chapter_start.take() {
                    chapter_starts.push(start_idx);
                }
                let heading = text.to_uppercase();
                push_line(
                    StyledLine::from_plain(heading),
                    &mut pages,
                    &mut current,
                    &mut at_page_index,
                );
                push_line(
                    StyledLine::from_plain(String::new()),
                    &mut pages,
                    &mut current,
                    &mut at_page_index,
                );
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
                        push_line(
                            StyledLine::from_plain(out),
                            &mut pages,
                            &mut current,
                            &mut at_page_index,
                        );
                    }
                }
                push_line(
                    StyledLine::from_plain(String::new()),
                    &mut pages,
                    &mut current,
                    &mut at_page_index,
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
                        });
                    }
                    let clipped = clip_segments(segs, max_width.max(4));
                    push_line(clipped, &mut pages, &mut current, &mut at_page_index);
                }
                push_line(
                    StyledLine::from_plain(String::new()),
                    &mut pages,
                    &mut current,
                    &mut at_page_index,
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
    }
}

use unicode_linebreak::{linebreaks, BreakOpportunity};

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

fn truncate_graphemes(s: &str, width: usize) -> String {
    let mut out = String::new();
    let mut count = 0usize;
    for g in s.graphemes(true) {
        if count >= width {
            out.push('…');
            break;
        }
        out.push_str(g);
        count += 1;
    }
    out
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
            });
        }
        if used >= width {
            out.push(Segment {
                text: "…".into(),
                fg: seg.fg,
                bg: seg.bg,
            });
            break;
        }
    }
    StyledLine { segments: out }
}

impl StyledLine {
    pub fn from_plain(text: String) -> Self {
        Self {
            segments: vec![Segment {
                text,
                fg: None,
                bg: None,
            }],
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
            Block::Paragraph("───".to_string()),
            Block::Paragraph("Chapter two text".to_string()),
        ];
        let words = extract_words(&blocks);
        assert_eq!(words.len(), 6);
        assert_eq!(words[0].chapter_index, None);
        assert_eq!(words[3].chapter_index, Some(1));
    }
}
