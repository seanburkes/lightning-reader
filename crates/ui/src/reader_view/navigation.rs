use unicode_segmentation::UnicodeSegmentation;

use reader_core::layout::Size;
use reader_core::types::Block as ReaderBlock;

use super::ReaderView;

impl ReaderView {
    pub fn up(&mut self, lines: usize) {
        let delta = lines.max(1);
        let step = if self.two_pane {
            delta.div_ceil(2) * 2
        } else {
            delta
        };
        self.current = self.current.saturating_sub(step);
        if self.two_pane {
            self.current = self.current.saturating_sub(self.current % 2);
        }
    }

    pub fn down(&mut self, lines: usize) {
        let delta = lines.max(1);
        let step = if self.two_pane {
            delta.div_ceil(2) * 2
        } else {
            delta
        };
        self.current = (self.current + step).min(self.pages.len().saturating_sub(1));
        if self.two_pane {
            self.current = self.current.saturating_sub(self.current % 2);
        }
    }

    pub fn reflow(&mut self, blocks: &[ReaderBlock], size: Size) {
        let p = reader_core::layout::paginate_with_justify(blocks, size, self.justify);
        self.pages = p.pages;
        self.chapter_starts = p.chapter_starts;
        self.anchors = p.anchors;
        self.current = self.current.min(self.pages.len().saturating_sub(1));
        if self.two_pane {
            self.current = self.current.saturating_sub(self.current % 2);
        }
    }

    pub fn chapter_title(&self, idx: usize) -> String {
        self.chapter_titles
            .get(idx)
            .map(|title| sanitize_chapter_title(title))
            .unwrap_or_default()
    }

    pub fn chapter_label(&self, idx: usize) -> String {
        let title = self.chapter_title(idx);
        if title.is_empty() {
            format!("Chapter {}", idx + 1)
        } else {
            title
        }
    }

    pub(super) fn chapter_left_label(&self) -> Option<String> {
        let idx = self.current_chapter_index()?;
        let label = self.chapter_label(idx);
        let mut parts = Vec::new();
        if let Some(total) = self.chapter_total() {
            parts.push(format!("Chapter {}/{}", idx + 1, total));
        } else {
            parts.push(format!("Chapter {}/?", idx + 1));
        }
        let default_label = format!("Chapter {}", idx + 1);
        if !label.is_empty() && label != default_label {
            parts.push(label);
        }
        Some(parts.join(" · "))
    }

    fn chapter_total(&self) -> Option<usize> {
        if let Some(total) = self.total_chapters {
            if total > 0 {
                return Some(total);
            }
        }
        let total = self.chapter_starts.len();
        if total == 0 {
            return None;
        }
        if !self.has_full_page_set() {
            return None;
        }
        Some(total)
    }

    pub(super) fn chapter_percent(&self, idx: usize) -> Option<usize> {
        let (start, end) = self.chapter_page_range(idx)?;
        if end <= start {
            return None;
        }
        let chapter_len = end.saturating_sub(start);
        if chapter_len == 0 {
            return None;
        }
        let pos = self
            .current
            .saturating_sub(start)
            .min(chapter_len.saturating_sub(1));
        let pct = ((pos + 1) as f32 / chapter_len as f32 * 100.0).round() as usize;
        Some(pct.clamp(1, 100))
    }

    fn chapter_page_range(&self, idx: usize) -> Option<(usize, usize)> {
        let start = *self.chapter_starts.get(idx)?;
        let end = if idx + 1 < self.chapter_starts.len() {
            self.chapter_starts[idx + 1]
        } else if self.has_full_page_set() {
            self.total_pages.unwrap_or(self.pages.len())
        } else {
            return None;
        };
        if end <= start {
            return None;
        }
        Some((start, end))
    }

    fn has_full_page_set(&self) -> bool {
        self.total_pages
            .is_none_or(|total| total == self.pages.len())
    }

    pub fn link_at_point(&self, point: super::SelectionPoint) -> Option<String> {
        let page = self.pages.get(point.page)?;
        let line = page.lines.get(point.line)?;
        let mut offset = 0usize;
        for seg in &line.segments {
            let seg_text = Self::segment_display_text(seg);
            let seg_len = seg_text.graphemes(true).count();
            if point.col < offset + seg_len {
                return seg.link.clone();
            }
            offset += seg_len;
        }
        None
    }

    pub fn link_label_at_point(&self, point: super::SelectionPoint) -> Option<String> {
        let page = self.pages.get(point.page)?;
        let line = page.lines.get(point.line)?;
        let mut offset = 0usize;
        for seg in &line.segments {
            let seg_text = Self::segment_display_text(seg);
            let seg_len = seg_text.graphemes(true).count();
            if point.col < offset + seg_len {
                return Some(seg_text.into_owned());
            }
            offset += seg_len;
        }
        None
    }

    pub fn page_for_href(&self, href: &str) -> Option<usize> {
        self.resolve_target_page(href)
    }

    pub fn jump_to_target(&mut self, target: &str) -> bool {
        let Some(mut page) = self.resolve_target_page(target) else {
            return false;
        };
        if !self.pages.is_empty() {
            page = page.min(self.pages.len().saturating_sub(1));
        }
        if self.two_pane {
            page = page.saturating_sub(page % 2);
        }
        self.current = page;
        true
    }

    fn resolve_target_page(&self, target: &str) -> Option<usize> {
        let target = target.trim();
        if target.is_empty() {
            return None;
        }
        if let Some(page) = self.anchors.get(target) {
            return Some(*page);
        }
        if target.starts_with('#') {
            if let Some(prefix) = self.current_chapter_href() {
                let full = format!("{}{}", prefix, target);
                if let Some(page) = self.anchors.get(&full) {
                    return Some(*page);
                }
            }
        }
        if let Some((path, _frag)) = target.split_once('#') {
            if let Some(idx) = self.chapter_hrefs.iter().position(|h| h == path) {
                return self.chapter_starts.get(idx).copied();
            }
        } else if let Some(idx) = self.chapter_hrefs.iter().position(|h| h == target) {
            return self.chapter_starts.get(idx).copied();
        }
        None
    }

    pub(super) fn current_chapter_href(&self) -> Option<&str> {
        let idx = self.current_chapter_index()?;
        self.chapter_hrefs.get(idx).map(|s| s.as_str())
    }

    pub(super) fn current_chapter_index(&self) -> Option<usize> {
        if self.chapter_starts.is_empty() {
            return None;
        }
        let mut idx = 0usize;
        for (i, start) in self.chapter_starts.iter().enumerate() {
            if *start <= self.current {
                idx = i;
            } else {
                break;
            }
        }
        Some(idx)
    }
}

fn sanitize_chapter_title(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let stripped = strip_parenthetical_links(trimmed);
    let stripped = strip_inline_markers(&stripped);
    if stripped.is_empty() {
        return String::new();
    }
    let lower = stripped.to_ascii_lowercase();
    let looks_like_file =
        lower.ends_with(".xhtml") || lower.ends_with(".html") || lower.ends_with(".htm");
    let has_path = stripped.contains('/') || stripped.contains('\\');
    if !looks_like_file && !has_path {
        return stripped.to_string();
    }
    let mut s = stripped.as_str();
    if let Some(pos) = s.find('#') {
        s = &s[..pos];
    }
    if let Some(pos) = s.find('?') {
        s = &s[..pos];
    }
    if let Some(seg) = s.rsplit(&['/', '\\'][..]).next() {
        s = seg;
    }
    let mut cleaned = s.to_string();
    let lower = cleaned.to_ascii_lowercase();
    for ext in [".xhtml", ".html", ".htm"] {
        if lower.ends_with(ext) && cleaned.len() > ext.len() {
            let new_len = cleaned.len() - ext.len();
            cleaned.truncate(new_len);
            break;
        }
    }
    let mut out = String::with_capacity(cleaned.len());
    let mut last_space = false;
    for ch in cleaned.chars() {
        let mapped = match ch {
            '_' | '-' | '.' => ' ',
            _ => ch,
        };
        if mapped.is_whitespace() {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(mapped);
            last_space = false;
        }
    }
    let out = out.trim().to_string();
    if out.is_empty() {
        stripped
    } else {
        out
    }
}

fn strip_parenthetical_links(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut buf = String::new();
    let mut in_paren = false;
    for ch in input.chars() {
        if ch == '(' && !in_paren {
            in_paren = true;
            buf.clear();
            continue;
        }
        if ch == ')' && in_paren {
            let lower = buf.to_ascii_lowercase();
            let is_link = lower.contains(".xhtml")
                || lower.contains(".html")
                || lower.contains(".htm")
                || lower.contains("http://")
                || lower.contains("https://");
            if !is_link {
                out.push(' ');
                out.push('(');
                out.push_str(buf.trim());
                out.push(')');
            }
            in_paren = false;
            buf.clear();
            continue;
        }
        if in_paren {
            buf.push(ch);
        } else {
            out.push(ch);
        }
    }
    if in_paren {
        out.push(' ');
        out.push('(');
        out.push_str(buf.trim());
    }
    let mut cleaned = String::with_capacity(out.len());
    let mut last_space = false;
    for ch in out.chars() {
        if ch.is_whitespace() {
            if !last_space {
                cleaned.push(' ');
            }
            last_space = true;
        } else {
            cleaned.push(ch);
            last_space = false;
        }
    }
    cleaned.trim().to_string()
}

fn strip_inline_markers(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1E' || ch == '\x1F' {
            let _ = chars.next();
            continue;
        }
        if ch == '\x1C' {
            for next in chars.by_ref() {
                if next == '\x1D' {
                    break;
                }
            }
            continue;
        }
        if ch == '\x18' {
            for next in chars.by_ref() {
                if next == '\x17' {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}
