use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::types::{Block, Document, DocumentFormat, DocumentInfo};

#[derive(Debug, Error)]
pub enum TextError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct TextFile {
    pub path: PathBuf,
    pub content: String,
}

impl TextFile {
    pub fn open(path: &Path) -> Result<Self, TextError> {
        let content = std::fs::read_to_string(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            content,
        })
    }

    pub fn to_document(&self) -> Document {
        let format = detect_format(&self.path);
        let blocks = parse_blocks(&self.content, format);
        let title = title_from_path(&self.path)
            .or_else(|| first_heading_title(&blocks))
            .unwrap_or_else(|| "Untitled".to_string());
        let path_str = self.path.to_string_lossy().into_owned();
        let info = DocumentInfo {
            id: format!("path:{}", path_str),
            path: path_str,
            title: Some(title.clone()),
            subtitle: None,
            author: None,
            metadata: None,
            format,
        };
        Document::new(
            info,
            blocks,
            vec![title],
            vec![self.path.to_string_lossy().into_owned()],
            Vec::new(),
        )
    }
}

fn detect_format(path: &Path) -> DocumentFormat {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("md") | Some("markdown") => DocumentFormat::Markdown,
        _ => DocumentFormat::Text,
    }
}

fn title_from_path(path: &Path) -> Option<String> {
    let stem = path.file_stem().and_then(|s| s.to_str())?;
    let title = prettify_title(stem);
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

fn prettify_title(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut last_space = false;
    for ch in raw.chars() {
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
    out.trim().to_string()
}

fn parse_blocks(content: &str, format: DocumentFormat) -> Vec<Block> {
    let markdown = matches!(format, DocumentFormat::Markdown);
    let mut blocks = Vec::new();
    let mut paragraph_lines: Vec<String> = Vec::new();
    let mut list_items: Vec<String> = Vec::new();
    let mut quote_lines: Vec<String> = Vec::new();
    let mut code_lines: Vec<String> = Vec::new();
    let mut code_lang: Option<String> = None;
    let mut code_fence: Option<char> = None;

    let mut lines = content.lines().peekable();
    while let Some(raw_line) = lines.next() {
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim();

        if let Some(fence_char) = code_fence {
            if let Some((found, _)) = parse_fence(trimmed) {
                if found == fence_char {
                    flush_code(&mut code_lines, &mut code_lang, &mut blocks);
                    code_fence = None;
                    continue;
                }
            }
            code_lines.push(line.to_string());
            continue;
        }

        if markdown {
            if let Some((fence, lang)) = parse_fence(trimmed) {
                flush_paragraph(&mut paragraph_lines, &mut blocks);
                flush_list(&mut list_items, &mut blocks);
                flush_quote(&mut quote_lines, &mut blocks);
                code_fence = Some(fence);
                code_lang = lang;
                continue;
            }
        }

        if trimmed.is_empty() {
            flush_paragraph(&mut paragraph_lines, &mut blocks);
            flush_list(&mut list_items, &mut blocks);
            flush_quote(&mut quote_lines, &mut blocks);
            continue;
        }

        if markdown {
            if let Some(next_line) = lines.peek() {
                if let Some(level) = setext_level(next_line) {
                    flush_paragraph(&mut paragraph_lines, &mut blocks);
                    flush_list(&mut list_items, &mut blocks);
                    flush_quote(&mut quote_lines, &mut blocks);
                    blocks.push(Block::Heading(trimmed.to_string(), level));
                    let _ = lines.next();
                    continue;
                }
            }

            if let Some((level, text)) = parse_atx_heading(trimmed) {
                flush_paragraph(&mut paragraph_lines, &mut blocks);
                flush_list(&mut list_items, &mut blocks);
                flush_quote(&mut quote_lines, &mut blocks);
                blocks.push(Block::Heading(text, level));
                continue;
            }
        }

        if is_separator_line(trimmed) {
            flush_paragraph(&mut paragraph_lines, &mut blocks);
            flush_list(&mut list_items, &mut blocks);
            flush_quote(&mut quote_lines, &mut blocks);
            blocks.push(Block::Paragraph("───".to_string()));
            continue;
        }

        if let Some(item) = parse_list_item(trimmed) {
            flush_paragraph(&mut paragraph_lines, &mut blocks);
            flush_quote(&mut quote_lines, &mut blocks);
            list_items.push(item);
            continue;
        } else if !list_items.is_empty() {
            flush_list(&mut list_items, &mut blocks);
        }

        if let Some(quote_line) = parse_quote_line(trimmed) {
            flush_paragraph(&mut paragraph_lines, &mut blocks);
            flush_list(&mut list_items, &mut blocks);
            quote_lines.push(quote_line);
            continue;
        } else if !quote_lines.is_empty() {
            flush_quote(&mut quote_lines, &mut blocks);
        }

        paragraph_lines.push(trimmed.to_string());
    }

    if code_fence.is_some() {
        flush_code(&mut code_lines, &mut code_lang, &mut blocks);
    }
    flush_list(&mut list_items, &mut blocks);
    flush_quote(&mut quote_lines, &mut blocks);
    flush_paragraph(&mut paragraph_lines, &mut blocks);

    crate::normalize::postprocess_blocks(blocks)
}

fn parse_fence(line: &str) -> Option<(char, Option<String>)> {
    let trimmed = line.trim();
    let bytes = trimmed.as_bytes();
    let first = *bytes.first()?;
    if first != b'`' && first != b'~' {
        return None;
    }
    let count = bytes.iter().take_while(|&&b| b == first).count();
    if count < 3 {
        return None;
    }
    let rest = trimmed[count..].trim();
    let lang = if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    };
    Some((first as char, lang))
}

fn parse_atx_heading(line: &str) -> Option<(u8, String)> {
    let trimmed = line.trim_start();
    let count = trimmed.bytes().take_while(|b| *b == b'#').count();
    if count == 0 || count > 6 {
        return None;
    }
    let rest = trimmed[count..].trim();
    let rest = rest.trim_end_matches('#').trim();
    if rest.is_empty() {
        return None;
    }
    Some((count as u8, rest.to_string()))
}

fn setext_level(line: &str) -> Option<u8> {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return None;
    }
    let first = trimmed.chars().next()?;
    if first != '=' && first != '-' {
        return None;
    }
    if trimmed.chars().all(|c| c == first) {
        Some(if first == '=' { 1 } else { 2 })
    } else {
        None
    }
}

fn parse_list_item(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(bullet) {
            let cleaned = clean_inline(rest);
            if !cleaned.is_empty() {
                return Some(cleaned);
            }
            return None;
        }
    }

    let bytes = trimmed.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx == 0 || idx + 1 >= bytes.len() {
        return None;
    }
    let sep = bytes[idx];
    if (sep == b'.' || sep == b')') && bytes[idx + 1].is_ascii_whitespace() {
        let rest = trimmed[idx + 1..].trim_start();
        let cleaned = clean_inline(rest);
        if cleaned.is_empty() {
            None
        } else {
            Some(cleaned)
        }
    } else {
        None
    }
}

fn parse_quote_line(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('>') {
        return None;
    }
    let rest = trimmed.trim_start_matches('>').trim_start();
    Some(clean_inline(rest))
}

fn is_separator_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed == "\u{000C}" {
        return true;
    }
    let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() < 3 {
        return false;
    }
    let Some(first) = compact.chars().next() else {
        return false;
    };
    if first != '-' && first != '*' && first != '_' {
        return false;
    }
    compact.chars().all(|c| c == first)
}

fn clean_inline(input: &str) -> String {
    let s = input
        .replace('\u{00A0}', " ")
        .replace(
            ['\u{200B}', '\u{200C}', '\u{200D}', '\u{200E}', '\u{200F}'],
            "",
        )
        .replace(['\u{2028}', '\u{2029}'], " ")
        .replace('\u{FEFF}', "");
    let mut out = String::with_capacity(s.len());
    let mut last_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim().to_string()
}

fn flush_paragraph(lines: &mut Vec<String>, blocks: &mut Vec<Block>) {
    if lines.is_empty() {
        return;
    }
    let text = join_lines(lines);
    lines.clear();
    if !text.trim().is_empty() {
        blocks.push(Block::Paragraph(text));
    }
}

fn join_lines(lines: &[String]) -> String {
    let cap = lines.iter().map(|line| line.len() + 1).sum();
    let mut out = String::with_capacity(cap);
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if out.is_empty() {
            out.push_str(trimmed);
        } else {
            out.push(' ');
            out.push_str(trimmed);
        }
    }
    out
}

fn flush_list(items: &mut Vec<String>, blocks: &mut Vec<Block>) {
    if items.is_empty() {
        return;
    }
    blocks.push(Block::List(std::mem::take(items)));
}

fn flush_quote(lines: &mut Vec<String>, blocks: &mut Vec<Block>) {
    if lines.is_empty() {
        return;
    }
    let mut out = String::new();
    for (idx, line) in lines.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        out.push_str(line);
    }
    lines.clear();
    if !out.trim().is_empty() {
        blocks.push(Block::Quote(out));
    }
}

fn flush_code(code_lines: &mut Vec<String>, lang: &mut Option<String>, blocks: &mut Vec<Block>) {
    if code_lines.is_empty() {
        *lang = None;
        return;
    }
    let mut text = String::new();
    for (idx, line) in code_lines.iter().enumerate() {
        if idx > 0 {
            text.push('\n');
        }
        text.push_str(line);
    }
    code_lines.clear();
    blocks.push(Block::Code {
        lang: lang.take(),
        text,
    });
}

fn first_heading_title(blocks: &[Block]) -> Option<String> {
    for block in blocks {
        if let Block::Heading(text, _) = block {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_paragraphs_as_continuous_text() {
        let input = "Hello\nworld\n\nNext paragraph.";
        let blocks = parse_blocks(input, DocumentFormat::Text);
        assert!(matches!(blocks[0], Block::Paragraph(ref t) if t == "Hello world"));
        assert!(matches!(blocks[1], Block::Paragraph(ref t) if t == "Next paragraph."));
    }

    #[test]
    fn parses_simple_lists() {
        let input = "- One\n- Two\n\nAfter";
        let blocks = parse_blocks(input, DocumentFormat::Text);
        assert!(matches!(
            blocks[0],
            Block::List(ref items) if items.len() == 2 && items[0] == "One" && items[1] == "Two"
        ));
        assert!(matches!(blocks[1], Block::Paragraph(ref t) if t == "After"));
    }

    #[test]
    fn parses_markdown_heading_and_code() {
        let input = "# Title\n\n```rust\nfn main() {}\n```\n";
        let blocks = parse_blocks(input, DocumentFormat::Markdown);
        assert!(matches!(blocks[0], Block::Heading(ref t, 1) if t == "Title"));
        assert!(matches!(
            blocks[1],
            Block::Code {
                ref lang,
                ref text,
            } if lang.as_deref() == Some("rust") && text.contains("fn main")
        ));
    }

    #[test]
    fn converts_horizontal_rules_to_separators() {
        let input = "First\n\n---\n\nSecond";
        let blocks = parse_blocks(input, DocumentFormat::Text);
        assert!(matches!(blocks[0], Block::Paragraph(ref t) if t == "First"));
        assert!(matches!(blocks[1], Block::Paragraph(ref t) if t == "───"));
        assert!(matches!(blocks[2], Block::Paragraph(ref t) if t == "Second"));
    }
}
