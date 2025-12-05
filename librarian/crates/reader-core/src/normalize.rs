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
