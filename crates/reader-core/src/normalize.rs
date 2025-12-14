use crate::types::Block;
use kuchiki::traits::*;

pub fn html_to_blocks(html: &str) -> Vec<Block> {
    let parser = kuchiki::parse_html().one(html.to_string());
    let mut blocks = Vec::new();

    fn collect(node: &kuchiki::NodeRef, out: &mut Vec<Block>) {
        for child in node.children() {
            if let Some(el) = child.as_element() {
                let tag = el.name.local.to_lowercase();
                if tag.len() == 2 && tag.starts_with('h') {
                    if let Ok(level) = tag[1..].parse::<u8>() {
                        let text = child.text_contents().trim().to_string();
                        if !text.is_empty() {
                            out.push(Block::Heading(text, level.min(6)));
                            continue;
                        }
                    }
                }
                match tag.as_str() {
                    "p" => {
                        let text = child.text_contents().trim().to_string();
                        if !text.is_empty() {
                            out.push(Block::Paragraph(text));
                        }
                        continue;
                    }
                    "blockquote" => {
                        let text = child.text_contents().trim().to_string();
                        if !text.is_empty() {
                            out.push(Block::Quote(text));
                        }
                        continue;
                    }
                    "ul" | "ol" => {
                        let mut items = Vec::new();
                        for li in child.children() {
                            if let Some(li_el) = li.as_element() {
                                if li_el.name.local.as_ref() == "li" {
                                    let text = li.text_contents().trim().to_string();
                                    if !text.is_empty() {
                                        items.push(text);
                                    }
                                }
                            }
                        }
                        if !items.is_empty() {
                            out.push(Block::List(items));
                        }
                        continue;
                    }
                    "pre" => {
                        let mut lang: Option<String> = None;
                        let text = child
                            .select("code")
                            .ok()
                            .and_then(|mut iter| iter.next())
                            .map(|code| {
                                lang = code.attributes.borrow().get("class").map(|s| s.to_string());
                                code.text_contents()
                            })
                            .unwrap_or_else(|| child.text_contents());
                        out.push(Block::Code { lang, text });
                        continue;
                    }
                    "img" => {
                        out.push(Block::Paragraph("[image]".into()));
                        continue;
                    }
                    _ => {}
                }
            }
            collect(&child, out);
        }
    }

    collect(&parser, &mut blocks);

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
            .replace(
                ['\u{200B}', '\u{200C}', '\u{200D}', '\u{200E}', '\u{200F}'],
                "",
            )
            .replace(['\u{2028}', '\u{2029}'], " ")
            .replace('\u{FEFF}', "");
        // Collapse whitespace to single spaces
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
            if tokens.len() < 2 {
                return input.to_string();
            }
            let mut out: Vec<String> = Vec::with_capacity(tokens.len());
            let mut i = 0;
            while i < tokens.len() {
                let tok = tokens[i];
                if tok.ends_with('-') && i + 1 < tokens.len() {
                    let next = tokens[i + 1];
                    if next
                        .chars()
                        .next()
                        .map(|c| c.is_lowercase())
                        .unwrap_or(false)
                    {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_dom_order() {
        let html = r#"
        <h1>Title</h1>
        <p>Intro text.</p>
        <ul><li>One</li><li>Two</li></ul>
        <p>Tail.</p>
        "#;
        let blocks = html_to_blocks(html);
        assert!(matches!(blocks[0], Block::Heading(ref t, 1) if t == "Title"));
        assert!(matches!(blocks[1], Block::Paragraph(ref t) if t == "Intro text."));
        assert!(matches!(blocks[2], Block::List(ref items) if items == &["One", "Two"]));
        assert!(matches!(blocks[3], Block::Paragraph(ref t) if t == "Tail."));
    }

    #[test]
    fn captures_code_with_language() {
        let html = r#"<pre><code class="rust">fn main() {}</code></pre>"#;
        let blocks = html_to_blocks(html);
        assert!(matches!(
            blocks[0],
            Block::Code { ref lang, ref text }
                if lang.as_deref() == Some("rust") && text.contains("fn main")
        ));
    }
}
