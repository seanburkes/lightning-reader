use crate::types::Block;

use super::inline::{normalize_line, normalize_lines};

// Lightweight post-processing to smooth whitespace/newlines inside paragraphs/headings
pub fn postprocess_blocks(mut blocks: Vec<Block>) -> Vec<Block> {
    fn clean_text(s: &str, preserve_newlines: bool) -> String {
        let s = s.replace('\u{00A0}', " "); // nbsp to space
        let s = s.replace('\r', "");
        // Strip zero-width/invisible separators
        let s = s
            .replace(
                ['\u{200B}', '\u{200C}', '\u{200D}', '\u{200E}', '\u{200F}'],
                "",
            )
            .replace(['\u{2028}', '\u{2029}'], "\n")
            .replace('\u{FEFF}', "");
        if preserve_newlines {
            normalize_lines(&s)
        } else {
            normalize_line(&s.replace('\n', " "))
        }
    }

    // First pass: whitespace cleanup on headings/paragraphs
    for b in &mut blocks {
        match b {
            Block::Paragraph(ref mut t) => {
                *t = clean_text(t, true);
            }
            Block::Heading(ref mut t, _) => {
                *t = clean_text(t, false);
            }
            Block::Quote(ref mut t) => {
                *t = clean_text(t, true);
            }
            Block::Image(ref mut img) => {
                if let Some(caption) = img.caption_mut() {
                    *caption = clean_text(caption, false);
                }
                if let Some(alt) = img.alt_mut() {
                    *alt = clean_text(alt, false);
                }
            }
            Block::Table(ref mut table) => {
                for row in table.rows_mut() {
                    for cell in row {
                        let cleaned = clean_text(cell.text(), true);
                        *cell.text_mut() = cleaned;
                    }
                }
            }
            _ => {}
        }
    }

    // Removed paragraph merging: respect <p> boundaries strictly
    blocks
}
