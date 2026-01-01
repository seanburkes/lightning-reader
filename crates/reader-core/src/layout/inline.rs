use super::{Segment, StyledLine, TextStyle};
use unicode_segmentation::UnicodeSegmentation;

pub(crate) const STYLE_START: char = '\x1E';
pub(crate) const STYLE_END: char = '\x1F';
pub(crate) const LINK_START: char = '\x1C';
pub(crate) const LINK_END: char = '\x1D';
pub(crate) const ANCHOR_START: char = '\x18';
pub(crate) const ANCHOR_END: char = '\x17';

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

pub(crate) struct WrappedLines {
    pub(crate) lines: Vec<StyledLine>,
    pub(crate) anchors: Vec<Vec<String>>,
}

pub(crate) fn strip_style_markers(input: &str) -> String {
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
            for next in chars.by_ref() {
                if next == LINK_END {
                    break;
                }
            }
            continue;
        }
        if ch == ANCHOR_START {
            for next in chars.by_ref() {
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

pub(crate) fn wrap_styled_text(text: &str, width: usize) -> WrappedLines {
    let width = width.max(1);
    let pieces = parse_inline_pieces(text);
    let tokens = tokenize_pieces(pieces);
    wrap_tokens(tokens, width)
}

pub(crate) fn segments_from_text_with_anchors(text: &str) -> (Vec<Segment>, Vec<String>) {
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

pub(crate) fn justify_styled_line(line: &StyledLine, width: usize) -> StyledLine {
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

pub(crate) fn line_width(line: &StyledLine) -> usize {
    line.segments
        .iter()
        .map(|seg| seg.text.graphemes(true).count())
        .sum()
}

pub(crate) fn uppercase_segments(segments: &mut [Segment]) {
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

pub(crate) fn clip_segments(segments: Vec<Segment>, width: usize) -> StyledLine {
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

fn is_space_segment(seg: &Segment) -> bool {
    !seg.text.is_empty() && seg.text.chars().all(|c| c == ' ')
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
            for next in chars.by_ref() {
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
            for next in chars.by_ref() {
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
