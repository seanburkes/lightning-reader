use crate::types::Block;

use super::inline::strip_style_markers;
use super::{is_chapter_separator, WordToken};

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
                    let token = WordToken::from_word(word, current_chapter);
                    words.push(token);
                }
            }
            Block::Heading(text, _) => {
                let cleaned = strip_style_markers(text);
                for word in cleaned.split_whitespace() {
                    let token = WordToken::from_word(word, current_chapter);
                    words.push(token);
                }
            }
            Block::List(items) => {
                for item in items {
                    let cleaned = strip_style_markers(item);
                    for word in cleaned.split_whitespace() {
                        let token = WordToken::from_word(word, current_chapter);
                        words.push(token);
                    }
                }
            }
            Block::Quote(text) => {
                let cleaned = strip_style_markers(text);
                for word in cleaned.split_whitespace() {
                    let token = WordToken::from_word(word, current_chapter);
                    words.push(token);
                }
            }
            Block::Image(image) => {
                let label = image.caption().or(image.alt()).map(strip_style_markers);
                if let Some(label) = label {
                    for word in label.split_whitespace() {
                        let token = WordToken::from_word(word, current_chapter);
                        words.push(token);
                    }
                }
            }
            Block::Table(table) => {
                for row in table.rows() {
                    for cell in row {
                        let cleaned = strip_style_markers(cell.text());
                        for word in cleaned.split_whitespace() {
                            let token = WordToken::from_word(word, current_chapter);
                            words.push(token);
                        }
                    }
                }
            }
        }
    }

    words
}

impl WordToken {
    fn from_word(text: &str, chapter_index: Option<usize>) -> Self {
        let is_sentence_end = Self::has_sentence_end_punct(text);
        let is_comma = Self::has_comma_punct(text);

        WordToken {
            text: text.to_string(),
            is_sentence_end,
            is_comma,
            chapter_index,
        }
    }

    fn has_sentence_end_punct(text: &str) -> bool {
        let trimmed = text.trim_end_matches([')', ']', '"', '\'']);
        trimmed.ends_with('.')
            || trimmed.ends_with('!')
            || trimmed.ends_with('?')
            || trimmed.ends_with(':')
            || trimmed.ends_with(';')
    }

    fn has_comma_punct(text: &str) -> bool {
        let trimmed = text.trim_end_matches([')', ']', '"', '\'']);
        trimmed.ends_with(',') || trimmed.ends_with('-') || trimmed.ends_with(')')
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
            Block::Paragraph(String::new()),
            Block::Paragraph("───".to_string()),
            Block::Paragraph(String::new()),
            Block::Paragraph("Chapter two text".to_string()),
        ];
        let words = extract_words(&blocks);
        assert_eq!(words.len(), 6);
        assert_eq!(words[0].chapter_index, None);
        assert_eq!(words[3].chapter_index, Some(1));
    }
}
