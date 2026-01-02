use std::borrow::Cow;

use ratatui::prelude::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;

use reader_core::layout::{Segment, StyledLine};

use super::ReaderView;

impl ReaderView {
    pub(super) fn highlight_line(
        line: &StyledLine,
        highlight: Option<&str>,
        selection: Option<(usize, usize)>,
    ) -> Line<'static> {
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
        Line::from(spans)
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

    pub(super) fn segment_style(seg: &Segment) -> Style {
        let mut style = Style::default();
        if let Some(rgb) = &seg.fg {
            style = style.fg(Color::Rgb(rgb.r(), rgb.g(), rgb.b()));
        }
        if let Some(rgb) = &seg.bg {
            style = style.bg(Color::Rgb(rgb.r(), rgb.g(), rgb.b()));
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

    pub(super) fn segment_display_text(seg: &Segment) -> Cow<'_, str> {
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
}
