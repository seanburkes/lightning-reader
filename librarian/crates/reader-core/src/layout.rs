use crate::types::Block;

#[derive(Clone, Copy)]
pub struct Size {
    pub width: u16,
    pub height: u16,
}

#[derive(Clone)]
pub struct Page {
    pub lines: Vec<String>,
}

pub fn paginate(blocks: &[Block], size: Size) -> Vec<Page> {
    // Greedy wrap: naive word wrapping for now
    let mut pages = Vec::new();
    let mut current = Page { lines: Vec::new() };
    for block in blocks {
        match block {
            Block::Paragraph(text) | Block::Quote(text) => {
                for line in wrap_text(text, size.width as usize) {
                    current.lines.push(line);
                    if current.lines.len() as u16 >= size.height {
                        pages.push(current.clone());
                        current = Page { lines: Vec::new() };
                    }
                }
                // blank line between paragraphs
                current.lines.push(String::new());
            }
            Block::Heading(text, _) => {
                let heading = text.to_uppercase();
                current.lines.push(heading);
                current.lines.push(String::new());
            }
            Block::List(items) => {
                for item in items {
                    let line = format!("• {}", item);
                    for w in wrap_text(&line, size.width as usize) {
                        current.lines.push(w);
                        if current.lines.len() as u16 >= size.height {
                            pages.push(current.clone());
                            current = Page { lines: Vec::new() };
                        }
                    }
                }
                current.lines.push(String::new());
            }
            Block::Code { text, .. } => {
                for line in text.lines() {
                    current.lines.push(line.to_string());
                    if current.lines.len() as u16 >= size.height {
                        pages.push(current.clone());
                        current = Page { lines: Vec::new() };
                    }
                }
                current.lines.push(String::new());
            }
        }
    }
    if !current.lines.is_empty() {
        pages.push(current);
    }
    pages
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if line.is_empty() {
            line.push_str(word);
        } else if line.len() + 1 + word.len() <= width {
            line.push(' ');
            line.push_str(word);
        } else {
            out.push(line);
            line = word.to_string();
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}
