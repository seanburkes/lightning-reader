use crate::types::Block;
use kuchiki::traits::*;

pub fn html_to_blocks(html: &str) -> Vec<Block> {
    let parser = kuchiki::parse_html().one(html.to_string());
    let mut blocks = Vec::new();

    // Headings h1..h6
    for level in 1..=6 {
        let sel = format!("h{}", level);
        if let Ok(iter) = parser.select(&sel) {
            for node in iter {
                let text = node.text_contents().trim().to_string();
                if !text.is_empty() {
                    blocks.push(Block::Heading(text, level as u8));
                }
            }
        }
    }

    // Paragraphs
    if let Ok(iter) = parser.select("p") {
        for node in iter {
            let text = node.text_contents().trim().to_string();
            if !text.is_empty() {
                blocks.push(Block::Paragraph(text));
            }
        }
    }

    // Blockquotes
    if let Ok(iter) = parser.select("blockquote") {
        for node in iter {
            let text = node.text_contents().trim().to_string();
            if !text.is_empty() {
                blocks.push(Block::Quote(text));
            }
        }
    }

    // Lists (flatten li)
    if let Ok(iter) = parser.select("ul, ol") {
        for node in iter {
            let mut items = Vec::new();
            if let Ok(li_iter) = node.as_node().select("li") {
                for li in li_iter {
                    let text = li.text_contents().trim().to_string();
                    if !text.is_empty() { items.push(text); }
                }
            }
            if !items.is_empty() { blocks.push(Block::List(items)); }
        }
    }

    // Code blocks
    if let Ok(iter) = parser.select("pre code") {
        for node in iter {
            let text = node.text_contents();
            let lang = node.attributes.borrow().get("class").map(|s| s.to_string());
            blocks.push(Block::Code { lang, text });
        }
    }

    // Images placeholder
    if let Ok(iter) = parser.select("img") {
        for _ in iter {
            blocks.push(Block::Paragraph("[image]".into()));
        }
    }

    if blocks.is_empty() {
        // Fallback: whole document text as a paragraph
        let text = parser.text_contents().trim().to_string();
        if !text.is_empty() {
            blocks.push(Block::Paragraph(text));
        }
    }

    blocks
}

// Lightweight post-processing to smooth whitespace/newlines inside paragraphs/headings
pub fn postprocess_blocks(mut blocks: Vec<Block>) -> Vec<Block> {
    fn clean_text(s: &str) -> String {
        let s = s.replace('\u{00A0}', " "); // nbsp to space
        let s = s.replace('\n', " "); // strip hard newlines (incl. soft breaks like <br>)
        let s = s.replace('\r', " ");
        // Strip zero-width/invisible separators
        let s = s
            .replace('\u{200B}', "") // zero width space
            .replace('\u{200C}', "") // zero width non-joiner
            .replace('\u{200D}', "") // zero width joiner
            .replace('\u{200E}', "") // LRM
            .replace('\u{200F}', ""
            ) // RLM
            .replace('\u{2028}', " ") // line separator -> space
            .replace('\u{2029}', " ") // paragraph separator -> space
            .replace('\u{FEFF}', ""); // BOM
        // Collapse whitespace to single spaces
        let mut out = String::with_capacity(s.len());
        let mut last_space = false;
        for ch in s.chars() {
            if ch.is_whitespace() {
                if !last_space { out.push(' '); }
                last_space = true;
            } else {
                out.push(ch);
                last_space = false;
            }
        }
        let s = out.trim().to_string();
        // Remove spaces before common punctuation
        let punct = [',', '.', ';', ':', '!', '?', ')', ']', '”'];
        let mut cleaned = String::with_capacity(s.len());
        let mut prev_was_space = false;
        for ch in s.chars() {
            if punct.contains(&ch) {
                if prev_was_space {
                    // drop the preceding space
                    let _ = cleaned.pop();
                }
                cleaned.push(ch);
                prev_was_space = false;
            } else if ch.is_whitespace() {
                // ensure single space
                if !prev_was_space {
                    cleaned.push(' ');
                    prev_was_space = true;
                }
            } else {
                cleaned.push(ch);
                prev_was_space = false;
            }
        }
        let cleaned = cleaned.trim().to_string();
        // Conservative de-hyphenation across soft line joins: "word- next" => "wordnext" if next starts lowercase
        fn dehyphenate(input: &str) -> String {
            let tokens: Vec<&str> = input.split(' ').collect();
            if tokens.len() < 2 { return input.to_string(); }
            let mut out: Vec<String> = Vec::with_capacity(tokens.len());
            let mut i = 0;
            while i < tokens.len() {
                let tok = tokens[i];
                if tok.ends_with('-') && i + 1 < tokens.len() {
                    let next = tokens[i + 1];
                    if next.chars().next().map(|c| c.is_lowercase()).unwrap_or(false) {
                        // Join and skip next
                        let mut joined = tok.trim_end_matches('-').to_string();
                        joined.push_str(next);
                        out.push(joined);
                        i += 2;
                        continue;
                    }
                }
                out.push(tok.to_string());
                i += 1;
            }
            out.join(" ")
        }
        dehyphenate(&cleaned)
    }

    // First pass: whitespace cleanup on headings/paragraphs
    for b in &mut blocks {
        match b {
            Block::Paragraph(ref mut t) | Block::Heading(ref mut t, _) => {
                *t = clean_text(t);
            }
            _ => {}
        }
    }

    // Removed paragraph merging: respect <p> boundaries strictly
    blocks
}

