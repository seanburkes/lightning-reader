use crate::types::Block;
use std::collections::HashMap;

mod inline;
mod paginate;
mod table;
mod words;

pub use paginate::{paginate, paginate_with_justify};
pub use words::extract_words;

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

#[derive(Clone, Debug)]
pub struct WordToken {
    pub text: String,
    pub is_sentence_end: bool,
    pub is_comma: bool,
    pub chapter_index: Option<usize>,
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

pub(crate) fn is_chapter_separator(blocks: &[Block], idx: usize) -> bool {
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
