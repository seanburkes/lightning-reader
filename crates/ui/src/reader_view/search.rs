use reader_core::layout::Page;

use super::ReaderView;

impl ReaderView {
    pub fn search_forward(&self, query: &str, start_from: Option<usize>) -> Option<usize> {
        let needle = query.trim();
        if needle.is_empty() || self.pages.is_empty() {
            return None;
        }
        let needle = needle.to_lowercase();
        let total = self.pages.len();
        let start_raw = start_from.unwrap_or(self.current);
        let start = if total == 0 { 0 } else { start_raw % total };
        for offset in 0..total {
            let idx = (start + offset) % total;
            if Self::page_contains(&self.pages[idx], &needle) {
                return Some(idx);
            }
        }
        None
    }

    fn page_contains(page: &Page, needle: &str) -> bool {
        if needle.is_empty() {
            return false;
        }
        let mut buf = String::new();
        for (i, line) in page.lines.iter().enumerate() {
            if i > 0 {
                buf.push(' ');
            }
            for seg in &line.segments {
                buf.push_str(&seg.text.to_lowercase());
            }
        }
        buf.contains(needle)
    }
}
