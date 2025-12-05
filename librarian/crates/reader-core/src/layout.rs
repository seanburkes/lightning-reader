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
    paginate_with_justify(blocks, size, false)
}

pub fn paginate_with_justify(blocks: &[Block], size: Size, justify: bool) -> Vec<Page> {
    // Greedy wrap with optional full justification
    let mut pages = Vec::new();
    let mut current = Page { lines: Vec::new() };
    for block in blocks {
        match block {
            Block::Paragraph(text) | Block::Quote(text) => {
                let lines = wrap_text(text, size.width as usize);
                for i in 0..lines.len() {
                    let is_last = i == lines.len() - 1;
                    let line = if justify && !is_last { justify_line(&lines[i], size.width as usize) } else { lines[i].clone() };
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
                    let lines = wrap_text(&line, size.width as usize);
                    for i in 0..lines.len() {
                        let is_last = i == lines.len() - 1;
                        let out = if justify && !is_last { justify_line(&lines[i], size.width as usize) } else { lines[i].clone() };
                        current.lines.push(out);
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

fn justify_line(line: &str, width: usize) -> String {
    // Do not justify single-word or already full/overfull lines
    if !line.contains(' ') || line.len() >= width {
        return line.to_string();
    }
    let words: Vec<&str> = line.split(' ').collect();
    let gaps = words.len().saturating_sub(1);
    if gaps == 0 { return line.to_string(); }

    // Current length includes existing single spaces
    let current_len = line.len();
    let extra = width.saturating_sub(current_len);
    if extra == 0 { return line.to_string(); }

    // Build with extra spaces distributed across gaps; no trailing spaces
    let mut out = String::with_capacity(width);
    let base = extra / gaps;
    let mut remainder = extra % gaps;
    for (i, w) in words.iter().enumerate() {
        out.push_str(w);
        if i < gaps {
            // one existing space + extra padding
            out.push(' ');
            for _ in 0..base { out.push(' '); }
            if remainder > 0 { out.push(' '); remainder -= 1; }
        }
    }

    // Clamp in case of any overshoot due to unicode width assumptions
    if out.len() > width { out.truncate(width); }
    out
}
