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

#[derive(Clone)]
pub struct Segment {
    pub text: String,
    pub fg: Option<crate::types::RgbColor>,
    pub bg: Option<crate::types::RgbColor>,
    pub style: TextStyle,
}

#[derive(Clone)]
pub struct Pagination {
    pub pages: Vec<Page>,
    pub chapter_starts: Vec<usize>, // page indices where a chapter begins
}

#[derive(Clone)]
struct InlineSpan {
    text: String,
    style: TextStyle,
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
    Space(TextStyle),
    Newline,
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
    if !input.contains(STYLE_START) && !input.contains(STYLE_END) {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == STYLE_START || ch == STYLE_END {
            let _ = chars.next();
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
    let mut at_page_index: usize = pages.len();
    let push_line =
        |line: StyledLine, pages: &mut Vec<Page>, current: &mut Page, at_page_index: &mut usize| {
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
                let lines = wrap_styled_text(text, size.width as usize);
                for i in 0..lines.len() {
                    let is_last = i == lines.len().saturating_sub(1);
                    let line = if justify && !is_last {
                        justify_styled_line(&lines[i], size.width as usize)
                    } else {
                        lines[i].clone()
                    };
                    push_line(line, &mut pages, &mut current, &mut at_page_index);
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
                let max_width = size.width.max(4) as usize;
                // Preserve line breaks like a code/pre block; truncate when too long
                for raw_line in text.lines() {
                    let mut segs = Vec::new();
                    segs.push(Segment {
                        text: prefix.to_string(),
                        fg: None,
                        bg: None,
                        style: TextStyle::default(),
                    });
                    segs.extend(segments_from_text(raw_line));
                    let clipped = clip_segments(segs, max_width);
                    push_line(clipped, &mut pages, &mut current, &mut at_page_index);
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
                let mut lines = wrap_styled_text(text, size.width as usize);
                for line in &mut lines {
                    uppercase_segments(&mut line.segments);
                    push_line(line.clone(), &mut pages, &mut current, &mut at_page_index);
                }
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
                    let lines = wrap_styled_text(&line, size.width as usize);
                    for i in 0..lines.len() {
                        let is_last = i == lines.len().saturating_sub(1);
                        let out = if justify && !is_last {
                            justify_styled_line(&lines[i], size.width as usize)
                        } else {
                            lines[i].clone()
                        };
                        push_line(out, &mut pages, &mut current, &mut at_page_index);
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
                        style: TextStyle::default(),
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

fn wrap_styled_text(text: &str, width: usize) -> Vec<StyledLine> {
    let width = width.max(1);
    let spans = parse_inline_spans(text);
    let tokens = tokenize_spans(spans);
    wrap_tokens(tokens, width)
}

fn wrap_tokens(tokens: Vec<InlineToken>, width: usize) -> Vec<StyledLine> {
    let mut lines: Vec<StyledLine> = Vec::new();
    let mut current: Vec<Segment> = Vec::new();
    let mut line_width = 0usize;
    let mut pending_space: Option<TextStyle> = None;

    let push_current =
        |lines: &mut Vec<StyledLine>, current: &mut Vec<Segment>, line_width: &mut usize| {
            lines.push(StyledLine {
                segments: std::mem::take(current),
            });
            *line_width = 0;
        };

    for token in tokens {
        match token {
            InlineToken::Space(style) => {
                pending_space = Some(style);
            }
            InlineToken::Newline => {
                pending_space = None;
                push_current(&mut lines, &mut current, &mut line_width);
            }
            InlineToken::Word(word) => {
                let space_style = pending_space.take();
                let space_width = if space_style.is_some() && !current.is_empty() {
                    1
                } else {
                    0
                };
                if line_width + space_width + word.width <= width {
                    if let Some(style) = space_style {
                        if !current.is_empty() {
                            current.push(space_segment(style));
                            line_width += 1;
                        }
                    }
                    current.extend(word.segments);
                    line_width += word.width;
                } else {
                    if !current.is_empty() {
                        push_current(&mut lines, &mut current, &mut line_width);
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
                                });
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

    if !current.is_empty() || lines.is_empty() {
        lines.push(StyledLine { segments: current });
    }
    lines
}

fn parse_inline_spans(text: &str) -> Vec<InlineSpan> {
    let mut spans: Vec<InlineSpan> = Vec::new();
    let mut current = String::new();
    let mut counts = StyleCounts::default();
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
                    spans.push(InlineSpan {
                        text: current,
                        style: style_from_counts(&counts),
                    });
                    current = String::new();
                }
                apply_style_code(&mut counts, code, is_start);
                continue;
            }
            current.push(ch);
            current.push(code);
            continue;
        }
        current.push(ch);
    }
    if !current.is_empty() {
        spans.push(InlineSpan {
            text: current,
            style: style_from_counts(&counts),
        });
    }
    spans
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

fn tokenize_spans(spans: Vec<InlineSpan>) -> Vec<InlineToken> {
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

    for span in spans {
        let style = span.style;
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
                    Some(InlineToken::Space(_) | InlineToken::Newline)
                ) {
                    tokens.push(InlineToken::Space(style));
                }
                continue;
            }
            if let Some(last) = current_segments.last_mut() {
                if last.style == style && last.fg.is_none() && last.bg.is_none() {
                    last.text.push_str(g);
                } else {
                    current_segments.push(Segment {
                        text: g.to_string(),
                        fg: None,
                        bg: None,
                        style,
                    });
                }
            } else {
                current_segments.push(Segment {
                    text: g.to_string(),
                    fg: None,
                    bg: None,
                    style,
                });
            }
            current_width += 1;
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
                if last.style == seg.style && last.fg.is_none() && last.bg.is_none() {
                    last.text.push_str(g);
                } else {
                    current.push(Segment {
                        text: g.to_string(),
                        fg: None,
                        bg: None,
                        style: seg.style,
                    });
                }
            } else {
                current.push(Segment {
                    text: g.to_string(),
                    fg: None,
                    bg: None,
                    style: seg.style,
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

fn segments_from_text(text: &str) -> Vec<Segment> {
    parse_inline_spans(text)
        .into_iter()
        .map(|span| Segment {
            text: span.text,
            fg: None,
            bg: None,
            style: span.style,
        })
        .collect()
}

fn space_segment(style: TextStyle) -> Segment {
    Segment {
        text: " ".to_string(),
        fg: None,
        bg: None,
        style,
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
            });
        }
        if used >= width {
            out.push(Segment {
                text: "…".into(),
                fg: seg.fg,
                bg: seg.bg,
                style: seg.style,
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
                style: TextStyle::default(),
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
