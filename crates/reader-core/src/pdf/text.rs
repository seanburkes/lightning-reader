use crate::types::Block;

pub(super) fn page_text_to_blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim_end();
        if line.trim().is_empty() {
            flush_lines(&mut lines, &mut out);
            continue;
        }
        lines.push(line.to_string());
    }
    flush_lines(&mut lines, &mut out);
    out
}

pub(super) fn page_title_from_blocks(blocks: &[Block]) -> Option<String> {
    blocks.iter().find_map(|b| match b {
        Block::Paragraph(t) => {
            let trimmed = t.trim();
            if trimmed.is_empty() {
                return None;
            }
            let len = trimmed.chars().count();
            if (6..=80).contains(&len) {
                Some(trimmed.to_string())
            } else {
                None
            }
        }
        Block::Heading(t, _) => Some(t.trim().to_string()),
        _ => None,
    })
}

fn flush_lines(lines: &mut Vec<String>, out: &mut Vec<Block>) {
    if lines.is_empty() {
        return;
    }
    if is_monospace_like(lines) {
        let text = lines.join("\n");
        out.push(Block::Code { lang: None, text });
        lines.clear();
        return;
    }
    let para = lines_to_paragraph(lines);
    let cleaned = para.trim();
    if !cleaned.is_empty() {
        out.push(Block::Paragraph(annotate_links(cleaned)));
    }
    lines.clear();
}

fn lines_to_paragraph(lines: &[String]) -> String {
    let mut current = String::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if ends_with_hard_hyphen(trimmed) {
            current.push_str(trimmed.trim_end_matches('-'));
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(trimmed);
        }
    }
    current
}

fn is_monospace_like(lines: &[String]) -> bool {
    if lines.len() < 2 {
        return false;
    }
    let avg_len: f32 = lines.iter().map(|l| l.len() as f32).sum::<f32>() / lines.len() as f32;
    let variance: f32 = lines
        .iter()
        .map(|l| {
            let diff = l.len() as f32 - avg_len;
            diff * diff
        })
        .sum::<f32>()
        / lines.len() as f32;
    let spaced = lines.iter().filter(|l| l.contains("  ")).count();
    variance < 16.0 && spaced as f32 / lines.len() as f32 > 0.4
}

fn ends_with_hard_hyphen(s: &str) -> bool {
    s.ends_with('-') && !s.ends_with("--")
}

fn annotate_links(s: &str) -> String {
    s.split_whitespace()
        .map(|tok| {
            let lower = tok.to_ascii_lowercase();
            let is_url = lower.starts_with("http://")
                || lower.starts_with("https://")
                || lower.starts_with("www.");
            let is_anchor = tok.starts_with('#');
            if is_url {
                format!("{} [link:{}]", tok, tok)
            } else if is_anchor {
                format!("{} [anchor:{}]", tok, tok.trim_start_matches('#'))
            } else {
                tok.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
