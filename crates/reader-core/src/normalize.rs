use crate::types::Block;
use kuchiki::{traits::*, NodeRef};

pub fn html_to_blocks(html: &str) -> Vec<Block> {
    let parser = kuchiki::parse_html().one(html.to_string());
    let mut blocks = Vec::new();

    fn heading_level(tag: &str) -> Option<u8> {
        (tag.len() == 2 && tag.starts_with('h'))
            .then(|| tag[1..].parse::<u8>().ok())
            .flatten()
            .map(|lvl| lvl.min(6))
    }

    fn extract_block(node: &NodeRef) -> Option<Block> {
        let el = node.as_element()?;
        let tag = el.name.local.to_lowercase();
        if let Some(level) = heading_level(&tag) {
            let text = inline_text(node);
            return if text.is_empty() {
                None
            } else {
                Some(Block::Heading(text, level))
            };
        }
        match tag.as_str() {
            "p" => {
                let text = inline_text(node);
                if text.is_empty() {
                    None
                } else {
                    Some(Block::Paragraph(text))
                }
            }
            "blockquote" => {
                let text = inline_text(node);
                if text.is_empty() {
                    None
                } else {
                    Some(Block::Quote(text))
                }
            }
            "ul" | "ol" => {
                let mut items = Vec::new();
                for li in node.children() {
                    if let Some(li_el) = li.as_element() {
                        if li_el.name.local.as_ref() == "li" {
                            let text = inline_text(&li);
                            if !text.is_empty() {
                                items.push(text);
                            }
                        }
                    }
                }
                if items.is_empty() {
                    None
                } else {
                    Some(Block::List(items))
                }
            }
            "pre" => {
                let mut lang: Option<String> = None;
                let text = node
                    .select("code")
                    .ok()
                    .and_then(|mut iter| iter.next())
                    .map(|code| {
                        lang = code.attributes.borrow().get("class").map(|s| s.to_string());
                        code.text_contents()
                    })
                    .unwrap_or_else(|| node.text_contents());
                Some(Block::Code { lang, text })
            }
            "img" => Some(Block::Paragraph(image_placeholder(node))),
            "figure" => figure_block(node),
            "table" => table_block(node),
            "dl" => definition_list_block(node),
            "aside" => {
                let text = inline_text(node);
                if text.is_empty() {
                    None
                } else {
                    Some(Block::Quote(text))
                }
            }
            "hr" => Some(Block::Paragraph("───".into())),
            "math" => Some(Block::Paragraph("[math]".into())),
            "svg" => Some(Block::Paragraph("[svg]".into())),
            _ => None,
        }
    }

    fn collect(node: &NodeRef, out: &mut Vec<Block>) {
        for child in node.children() {
            if let Some(block) = extract_block(&child) {
                out.push(block);
                continue;
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

fn inline_text(node: &NodeRef) -> String {
    let mut out = String::new();
    append_inline_text(node, &mut out);
    normalize_inline_text(&out)
}

fn append_inline_text(node: &NodeRef, out: &mut String) {
    if let Some(text) = node.as_text() {
        out.push_str(&text.borrow());
        return;
    }
    let Some(el) = node.as_element() else {
        for child in node.children() {
            append_inline_text(&child, out);
        }
        return;
    };
    let tag = el.name.local.to_lowercase();
    match tag.as_str() {
        "br" => out.push('\n'),
        "a" => {
            let label = collect_inline_children(node);
            let href = el.attributes.borrow().get("href").map(|s| s.to_string());
            if label.is_empty() {
                if let Some(href) = href {
                    out.push_str(&href);
                }
                return;
            }
            out.push_str(&label);
            if let Some(href) = href {
                if !href.is_empty() && !label.contains(&href) {
                    out.push_str(" (");
                    out.push_str(&href);
                    out.push(')');
                }
            }
        }
        "img" => out.push_str(&image_placeholder(node)),
        "em" | "i" => append_wrapped_marker(node, out, "*"),
        "strong" | "b" => append_wrapped_marker(node, out, "**"),
        "code" | "kbd" | "samp" => append_wrapped_marker(node, out, "`"),
        "del" | "s" | "strike" => append_wrapped_marker(node, out, "~~"),
        "sup" => append_wrapped_pair(node, out, "^{", "}"),
        "sub" => append_wrapped_pair(node, out, "_{", "}"),
        "abbr" => {
            let label = collect_inline_children(node);
            if label.is_empty() {
                return;
            }
            out.push_str(&label);
            if let Some(title) = el.attributes.borrow().get("title") {
                let title = normalize_inline_text(title);
                if !title.is_empty() && !label.contains(&title) {
                    out.push_str(" (");
                    out.push_str(&title);
                    out.push(')');
                }
            }
        }
        "math" => {
            let label = collect_inline_children(node);
            if label.is_empty() {
                out.push_str("[math]");
            } else {
                out.push_str(&label);
            }
        }
        "svg" => {
            let label = collect_inline_children(node);
            if label.is_empty() {
                out.push_str("[svg]");
            } else {
                out.push_str(&label);
            }
        }
        _ => {
            for child in node.children() {
                append_inline_text(&child, out);
            }
        }
    }
}

fn collect_inline_children(node: &NodeRef) -> String {
    let mut out = String::new();
    for child in node.children() {
        append_inline_text(&child, &mut out);
    }
    normalize_inline_text(&out)
}

fn append_wrapped_marker(node: &NodeRef, out: &mut String, marker: &str) {
    let label = collect_inline_children(node);
    if label.is_empty() {
        return;
    }
    out.push_str(marker);
    out.push_str(&label);
    out.push_str(marker);
}

fn append_wrapped_pair(node: &NodeRef, out: &mut String, prefix: &str, suffix: &str) {
    let label = collect_inline_children(node);
    if label.is_empty() {
        return;
    }
    out.push_str(prefix);
    out.push_str(&label);
    out.push_str(suffix);
}

fn normalize_inline_text(s: &str) -> String {
    let s = s
        .replace('\u{00A0}', " ")
        .replace('\r', "")
        .replace(
            ['\u{200B}', '\u{200C}', '\u{200D}', '\u{200E}', '\u{200F}'],
            "",
        )
        .replace(['\u{2028}', '\u{2029}'], "\n")
        .replace('\u{FEFF}', "");
    normalize_lines(&s)
}

fn normalize_lines(s: &str) -> String {
    let mut out_lines = Vec::new();
    for line in s.split('\n') {
        out_lines.push(normalize_line(line));
    }
    let out = out_lines.join("\n");
    out.trim().to_string()
}

fn normalize_line(s: &str) -> String {
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
    let mut cleaned = String::with_capacity(out.len());
    let mut prev_was_space = false;
    let punct = [',', '.', ';', ':', '!', '?', ')', ']', '”'];
    for ch in out.chars() {
        if punct.contains(&ch) {
            if prev_was_space {
                let _ = cleaned.pop();
            }
            cleaned.push(ch);
            prev_was_space = false;
        } else if ch.is_whitespace() {
            if !prev_was_space {
                cleaned.push(' ');
                prev_was_space = true;
            }
        } else {
            cleaned.push(ch);
            prev_was_space = false;
        }
    }
    dehyphenate(&cleaned).trim().to_string()
}

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

fn image_placeholder(node: &NodeRef) -> String {
    let Some(el) = node.as_element() else {
        return "[image]".to_string();
    };
    let attrs = el.attributes.borrow();
    let alt = attrs.get("alt").or_else(|| attrs.get("title"));
    if let Some(alt) = alt {
        let alt = normalize_inline_text(alt);
        if !alt.is_empty() {
            return format!("[image: {}]", alt);
        }
    }
    "[image]".to_string()
}

fn figure_block(node: &NodeRef) -> Option<Block> {
    let mut parts: Vec<String> = Vec::new();
    if let Ok(mut imgs) = node.select("img") {
        if let Some(img) = imgs.next() {
            let text = image_placeholder(img.as_node());
            if !text.is_empty() {
                parts.push(text);
            }
        }
    }
    if let Ok(mut captions) = node.select("figcaption") {
        if let Some(cap) = captions.next() {
            let text = inline_text(cap.as_node());
            if !text.is_empty() {
                parts.push(text);
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(Block::Paragraph(parts.join(" ")))
    }
}

fn table_block(node: &NodeRef) -> Option<Block> {
    let mut rows: Vec<String> = Vec::new();
    if let Ok(trs) = node.select("tr") {
        for tr in trs {
            let mut cells: Vec<String> = Vec::new();
            for child in tr.as_node().children() {
                if let Some(el) = child.as_element() {
                    let tag = el.name.local.to_lowercase();
                    if tag == "td" || tag == "th" {
                        let cell = inline_text(&child);
                        cells.push(cell);
                    }
                }
            }
            if !cells.is_empty() {
                rows.push(cells.join(" | "));
            }
        }
    }
    if rows.is_empty() {
        let fallback = inline_text(node);
        if fallback.is_empty() {
            None
        } else {
            Some(Block::Code {
                lang: None,
                text: fallback,
            })
        }
    } else {
        Some(Block::Code {
            lang: None,
            text: rows.join("\n"),
        })
    }
}

fn definition_list_block(node: &NodeRef) -> Option<Block> {
    let mut items: Vec<String> = Vec::new();
    let mut current_term: Option<String> = None;
    for child in node.children() {
        if let Some(el) = child.as_element() {
            let tag = el.name.local.to_lowercase();
            if tag == "dt" {
                let term = inline_text(&child);
                if !term.is_empty() {
                    current_term = Some(term);
                }
            } else if tag == "dd" {
                let definition = inline_text(&child);
                if !definition.is_empty() {
                    let item = match current_term.take() {
                        Some(term) if !term.is_empty() => format!("{}: {}", term, definition),
                        _ => definition,
                    };
                    items.push(item);
                }
            }
        }
    }
    if items.is_empty() {
        None
    } else {
        Some(Block::List(items))
    }
}

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
    fn preserves_inline_markup_and_links() {
        let html = r#"
        <p>Read <em>this</em> and <strong>that</strong>, see
        <a href="https://example.com">link</a>.</p>
        "#;
        let blocks = html_to_blocks(html);
        assert!(matches!(
            blocks[0],
            Block::Paragraph(ref t)
                if t == "Read *this* and **that**, see link (https://example.com)."
        ));
    }

    #[test]
    fn extracts_tables_as_code_blocks() {
        let html = r#"
        <table>
          <tr><th>Head</th><th>Value</th></tr>
          <tr><td>A</td><td>B</td></tr>
        </table>
        "#;
        let blocks = html_to_blocks(html);
        assert!(matches!(
            blocks[0],
            Block::Code { ref text, .. } if text == "Head | Value\nA | B"
        ));
    }

    #[test]
    fn preserves_line_breaks_from_br() {
        let html = r#"<p>Line one<br/>Line two</p>"#;
        let blocks = postprocess_blocks(html_to_blocks(html));
        assert!(matches!(
            blocks[0],
            Block::Paragraph(ref t) if t == "Line one\nLine two"
        ));
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
