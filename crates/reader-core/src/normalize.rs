use crate::types::{Block, ImageBlock};
use kuchiki::{traits::*, NodeRef};

pub fn html_to_blocks(html: &str) -> Vec<Block> {
    html_to_blocks_with_assets(html, None, |_| None, |_| None)
}

pub fn html_to_blocks_with_images<F>(html: &str, resolve: F) -> Vec<Block>
where
    F: FnMut(&str) -> Option<(String, Vec<u8>)>,
{
    html_to_blocks_with_assets(html, None, resolve, |_| None)
}

pub fn html_to_blocks_with_assets<FImg, FLink>(
    html: &str,
    anchor_prefix: Option<&str>,
    mut resolve_image: FImg,
    mut resolve_link: FLink,
) -> Vec<Block>
where
    FImg: FnMut(&str) -> Option<(String, Vec<u8>)>,
    FLink: FnMut(&str) -> Option<String>,
{
    let parser = kuchiki::parse_html().one(html.to_string());
    let mut blocks = Vec::new();

    fn heading_level(tag: &str) -> Option<u8> {
        (tag.len() == 2 && tag.starts_with('h'))
            .then(|| tag[1..].parse::<u8>().ok())
            .flatten()
            .map(|lvl| lvl.min(6))
    }

    fn extract_block<FImg, FLink>(
        node: &NodeRef,
        ctx: &mut InlineContext<'_, FLink>,
        resolve_image: &mut FImg,
    ) -> Option<Block>
    where
        FImg: FnMut(&str) -> Option<(String, Vec<u8>)>,
        FLink: FnMut(&str) -> Option<String>,
    {
        let el = node.as_element()?;
        let tag = el.name.local.to_lowercase();
        if let Some(level) = heading_level(&tag) {
            let text = inline_text(node, ctx);
            return if text.is_empty() {
                None
            } else {
                Some(Block::Heading(text, level))
            };
        }
        match tag.as_str() {
            "p" => {
                let text = inline_text(node, ctx);
                if text.is_empty() {
                    None
                } else {
                    Some(Block::Paragraph(text))
                }
            }
            "blockquote" => {
                let text = inline_text(node, ctx);
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
                            let text = list_item_text(&li, ctx);
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
            "img" => image_block(node, resolve_image),
            "figure" => figure_block(node, ctx, resolve_image),
            "table" => table_block(node, ctx),
            "dl" => definition_list_block(node, ctx),
            "aside" => {
                let text = inline_text(node, ctx);
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

    fn collect<FImg, FLink>(
        node: &NodeRef,
        out: &mut Vec<Block>,
        ctx: &mut InlineContext<'_, FLink>,
        resolve_image: &mut FImg,
    ) where
        FImg: FnMut(&str) -> Option<(String, Vec<u8>)>,
        FLink: FnMut(&str) -> Option<String>,
    {
        for child in node.children() {
            if let Some(block) = extract_block(&child, ctx, resolve_image) {
                out.push(block);
                continue;
            }
            collect(&child, out, ctx, resolve_image);
        }
    }

    let mut ctx = InlineContext {
        resolve_link: &mut resolve_link,
        anchor_prefix,
    };
    collect(&parser, &mut blocks, &mut ctx, &mut resolve_image);

    if blocks.is_empty() {
        // Fallback: whole document text as a paragraph
        let text = parser.text_contents().trim().to_string();
        if !text.is_empty() {
            blocks.push(Block::Paragraph(text));
        }
    }

    blocks
}

const STYLE_START: char = '\x1E';
const STYLE_END: char = '\x1F';
const BR_MARKER: char = '\x1A';
const LINK_START: char = '\x1C';
const LINK_END: char = '\x1D';
const ANCHOR_START: char = '\x18';
const ANCHOR_END: char = '\x17';

struct InlineContext<'a, F>
where
    F: FnMut(&str) -> Option<String>,
{
    resolve_link: &'a mut F,
    anchor_prefix: Option<&'a str>,
}

fn inline_text<F>(node: &NodeRef, ctx: &mut InlineContext<'_, F>) -> String
where
    F: FnMut(&str) -> Option<String>,
{
    let mut out = String::new();
    append_inline_text(node, &mut out, ctx);
    normalize_inline_text(&out)
}

fn append_inline_text<F>(node: &NodeRef, out: &mut String, ctx: &mut InlineContext<'_, F>)
where
    F: FnMut(&str) -> Option<String>,
{
    if let Some(text) = node.as_text() {
        out.push_str(&text.borrow());
        return;
    }
    let Some(el) = node.as_element() else {
        for child in node.children() {
            append_inline_text(&child, out, ctx);
        }
        return;
    };
    if let Some(anchor) = anchor_id(el) {
        push_anchor_marker(out, &anchor, ctx.anchor_prefix);
    }
    let tag = el.name.local.to_lowercase();
    match tag.as_str() {
        "br" => out.push(BR_MARKER),
        "a" => {
            let label = collect_inline_children(node, ctx);
            let href = el.attributes.borrow().get("href").map(|s| s.to_string());
            if label.is_empty() {
                if let Some(href) = href {
                    out.push_str(&href);
                }
                return;
            }
            if let Some(href) = href {
                if !is_external_href(&href) {
                    if let Some(target) = (ctx.resolve_link)(href.trim()) {
                        push_link_start(out, &target);
                        out.push_str(&label);
                        push_link_end(out);
                        return;
                    }
                }
                if is_external_href(&href) && !href.is_empty() && !label.contains(&href) {
                    out.push_str(&label);
                    out.push_str(" (");
                    out.push_str(&href);
                    out.push(')');
                    return;
                }
            }
            out.push_str(&label);
        }
        "img" => out.push_str(&image_inline_text(node)),
        "em" | "i" => append_wrapped_style(node, out, 'i', ctx),
        "strong" | "b" => append_wrapped_style(node, out, 'b', ctx),
        "code" | "kbd" | "samp" => append_wrapped_style(node, out, 'c', ctx),
        "del" | "s" | "strike" => append_wrapped_style(node, out, 'x', ctx),
        "u" => append_wrapped_style(node, out, 'u', ctx),
        "span" => {
            let styles = span_style_codes(el);
            if styles.is_empty() {
                for child in node.children() {
                    append_inline_text(&child, out, ctx);
                }
            } else {
                append_wrapped_styles(node, out, &styles, ctx);
            }
        }
        "sup" => append_wrapped_pair(node, out, "^{", "}", ctx),
        "sub" => append_wrapped_pair(node, out, "_{", "}", ctx),
        "abbr" => {
            let label = collect_inline_children(node, ctx);
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
            let label = collect_inline_children(node, ctx);
            if label.is_empty() {
                out.push_str("[math]");
            } else {
                out.push_str(&label);
            }
        }
        "svg" => {
            let label = collect_inline_children(node, ctx);
            if label.is_empty() {
                out.push_str("[svg]");
            } else {
                out.push_str(&label);
            }
        }
        _ => {
            for child in node.children() {
                append_inline_text(&child, out, ctx);
            }
        }
    }
}

fn collect_inline_children<F>(node: &NodeRef, ctx: &mut InlineContext<'_, F>) -> String
where
    F: FnMut(&str) -> Option<String>,
{
    let mut out = String::new();
    for child in node.children() {
        append_inline_text(&child, &mut out, ctx);
    }
    normalize_inline_text(&out)
}

fn list_item_text<F>(node: &NodeRef, ctx: &mut InlineContext<'_, F>) -> String
where
    F: FnMut(&str) -> Option<String>,
{
    let mut out = String::new();
    append_inline_text_without_lists(node, &mut out, ctx);
    normalize_inline_text(&out)
}

fn append_inline_text_without_lists<F>(
    node: &NodeRef,
    out: &mut String,
    ctx: &mut InlineContext<'_, F>,
) where
    F: FnMut(&str) -> Option<String>,
{
    if let Some(text) = node.as_text() {
        out.push_str(&text.borrow());
        return;
    }
    let Some(el) = node.as_element() else {
        for child in node.children() {
            append_inline_text_without_lists(&child, out, ctx);
        }
        return;
    };
    if let Some(anchor) = anchor_id(el) {
        push_anchor_marker(out, &anchor, ctx.anchor_prefix);
    }
    let tag = el.name.local.to_lowercase();
    if tag == "ul" || tag == "ol" {
        return;
    }
    match tag.as_str() {
        "br" => out.push(BR_MARKER),
        "a" => {
            let label = collect_inline_children(node, ctx);
            let href = el.attributes.borrow().get("href").map(|s| s.to_string());
            if label.is_empty() {
                if let Some(href) = href {
                    out.push_str(&href);
                }
                return;
            }
            if let Some(href) = href {
                if !is_external_href(&href) {
                    if let Some(target) = (ctx.resolve_link)(href.trim()) {
                        push_link_start(out, &target);
                        out.push_str(&label);
                        push_link_end(out);
                        return;
                    }
                }
                if is_external_href(&href) && !href.is_empty() && !label.contains(&href) {
                    out.push_str(&label);
                    out.push_str(" (");
                    out.push_str(&href);
                    out.push(')');
                    return;
                }
            }
            out.push_str(&label);
        }
        "img" => out.push_str(&image_inline_text(node)),
        "em" | "i" => append_wrapped_style(node, out, 'i', ctx),
        "strong" | "b" => append_wrapped_style(node, out, 'b', ctx),
        "code" | "kbd" | "samp" => append_wrapped_style(node, out, 'c', ctx),
        "del" | "s" | "strike" => append_wrapped_style(node, out, 'x', ctx),
        "u" => append_wrapped_style(node, out, 'u', ctx),
        "span" => {
            let styles = span_style_codes(el);
            if styles.is_empty() {
                for child in node.children() {
                    append_inline_text_without_lists(&child, out, ctx);
                }
            } else {
                append_wrapped_styles(node, out, &styles, ctx);
            }
        }
        "sup" => append_wrapped_pair(node, out, "^{", "}", ctx),
        "sub" => append_wrapped_pair(node, out, "_{", "}", ctx),
        "abbr" => {
            let label = collect_inline_children(node, ctx);
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
            let label = collect_inline_children(node, ctx);
            if label.is_empty() {
                out.push_str("[math]");
            } else {
                out.push_str(&label);
            }
        }
        "svg" => {
            let label = collect_inline_children(node, ctx);
            if label.is_empty() {
                out.push_str("[svg]");
            } else {
                out.push_str(&label);
            }
        }
        _ => {
            for child in node.children() {
                append_inline_text_without_lists(&child, out, ctx);
            }
        }
    }
}

fn append_wrapped_style<F>(
    node: &NodeRef,
    out: &mut String,
    code: char,
    ctx: &mut InlineContext<'_, F>,
) where
    F: FnMut(&str) -> Option<String>,
{
    let label = collect_inline_children(node, ctx);
    if label.is_empty() {
        return;
    }
    push_style_start(out, code);
    out.push_str(&label);
    push_style_end(out, code);
}

fn append_wrapped_styles<F>(
    node: &NodeRef,
    out: &mut String,
    codes: &[char],
    ctx: &mut InlineContext<'_, F>,
) where
    F: FnMut(&str) -> Option<String>,
{
    let label = collect_inline_children(node, ctx);
    if label.is_empty() {
        return;
    }
    for code in codes {
        push_style_start(out, *code);
    }
    out.push_str(&label);
    for code in codes.iter().rev() {
        push_style_end(out, *code);
    }
}

fn append_wrapped_pair<F>(
    node: &NodeRef,
    out: &mut String,
    prefix: &str,
    suffix: &str,
    ctx: &mut InlineContext<'_, F>,
) where
    F: FnMut(&str) -> Option<String>,
{
    let label = collect_inline_children(node, ctx);
    if label.is_empty() {
        return;
    }
    out.push_str(prefix);
    out.push_str(&label);
    out.push_str(suffix);
}

fn push_style_start(out: &mut String, code: char) {
    out.push(STYLE_START);
    out.push(code);
}

fn push_style_end(out: &mut String, code: char) {
    out.push(STYLE_END);
    out.push(code);
}

fn push_link_start(out: &mut String, target: &str) {
    let target = target.trim();
    if target.is_empty() {
        return;
    }
    out.push(LINK_START);
    out.push_str(target);
    out.push(LINK_END);
}

fn push_link_end(out: &mut String) {
    out.push(LINK_START);
    out.push(LINK_END);
}

fn push_anchor_marker(out: &mut String, anchor: &str, prefix: Option<&str>) {
    let anchor = anchor.trim();
    if anchor.is_empty() {
        return;
    }
    let anchor = anchor.strip_prefix('#').unwrap_or(anchor).trim();
    if anchor.is_empty() {
        return;
    }
    let mut target = String::new();
    if let Some(prefix) = prefix {
        if !prefix.is_empty() {
            target.push_str(prefix);
        }
    }
    target.push('#');
    target.push_str(anchor);
    out.push(ANCHOR_START);
    out.push_str(&target);
    out.push(ANCHOR_END);
}

fn anchor_id(el: &kuchiki::ElementData) -> Option<String> {
    let attrs = el.attributes.borrow();
    let id = attrs
        .get("id")
        .or_else(|| attrs.get("name"))
        .or_else(|| attrs.get("xml:id"));
    id.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn span_style_codes(el: &kuchiki::ElementData) -> Vec<char> {
    let attrs = el.attributes.borrow();
    let mut codes: Vec<char> = Vec::new();
    if let Some(style) = attrs.get("style") {
        let style = style.to_ascii_lowercase();
        if style.contains("font-style: italic") {
            push_unique_style(&mut codes, 'i');
        }
        if style.contains("font-weight: bold")
            || style.contains("font-weight: 600")
            || style.contains("font-weight: 700")
            || style.contains("font-weight: 800")
            || style.contains("font-weight: 900")
        {
            push_unique_style(&mut codes, 'b');
        }
        if style.contains("text-decoration: underline")
            || style.contains("text-decoration-line: underline")
        {
            push_unique_style(&mut codes, 'u');
        }
        if style.contains("font-variant: small-caps")
            || style.contains("font-variant-caps: small-caps")
        {
            push_unique_style(&mut codes, 's');
        }
    }
    if let Some(class_attr) = attrs.get("class") {
        let class_attr = class_attr.to_ascii_lowercase();
        if class_attr.contains("small-caps")
            || class_attr.contains("smallcaps")
            || class_attr.contains("small_caps")
            || class_attr.contains("smcap")
        {
            push_unique_style(&mut codes, 's');
        }
    }
    codes
}

fn push_unique_style(codes: &mut Vec<char>, code: char) {
    if !codes.contains(&code) {
        codes.push(code);
    }
}

fn normalize_inline_text(s: &str) -> String {
    let s = s
        .replace('\u{00A0}', " ")
        .replace('\r', "")
        .replace(
            ['\u{200B}', '\u{200C}', '\u{200D}', '\u{200E}', '\u{200F}'],
            "",
        )
        .replace(['\u{2028}', '\u{2029}'], " ")
        .replace('\u{FEFF}', "");
    let s = s.replace('\n', " ");
    let s = s.replace(BR_MARKER, "\n");
    normalize_lines(&s)
}

fn is_external_href(href: &str) -> bool {
    let lower = href.trim().to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:")
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

fn image_inline_text(node: &NodeRef) -> String {
    let Some(el) = node.as_element() else {
        return "Image".to_string();
    };
    let attrs = el.attributes.borrow();
    if let Some(label) = image_label_text(&attrs) {
        return label;
    }
    if let Some(dim) = image_dimensions_text(&attrs) {
        return format!("Image ({})", dim);
    }
    "Image".to_string()
}

fn image_block<F>(node: &NodeRef, resolve: &mut F) -> Option<Block>
where
    F: FnMut(&str) -> Option<(String, Vec<u8>)>,
{
    let Some(el) = node.as_element() else {
        return None;
    };
    let attrs = el.attributes.borrow();
    let src = image_src(&attrs);
    let alt = image_label_text(&attrs);
    let (width, height) = image_dimensions(&attrs);
    let Some(src) = src else {
        let text = image_fallback_text(alt.as_deref(), width, height);
        return (!text.is_empty()).then(|| Block::Paragraph(text));
    };
    let (id, data) = match resolve(&src) {
        Some((id, data)) => (id, Some(data)),
        None => (src.clone(), None),
    };
    Some(Block::Image(ImageBlock {
        id,
        data,
        alt,
        caption: None,
        width,
        height,
    }))
}

fn figure_block<F, L>(
    node: &NodeRef,
    ctx: &mut InlineContext<'_, L>,
    resolve: &mut F,
) -> Option<Block>
where
    F: FnMut(&str) -> Option<(String, Vec<u8>)>,
    L: FnMut(&str) -> Option<String>,
{
    let mut src: Option<String> = None;
    let mut alt: Option<String> = None;
    let mut width: Option<u32> = None;
    let mut height: Option<u32> = None;
    if let Ok(mut imgs) = node.select("img") {
        if let Some(img) = imgs.next() {
            if let Some(el) = img.as_node().as_element() {
                let attrs = el.attributes.borrow();
                src = image_src(&attrs);
                alt = image_label_text(&attrs);
                let dims = image_dimensions(&attrs);
                width = dims.0;
                height = dims.1;
            }
        }
    }
    let caption = if let Ok(mut captions) = node.select("figcaption") {
        captions
            .next()
            .map(|cap| inline_text(cap.as_node(), ctx))
            .filter(|text| !text.is_empty())
    } else {
        None
    };
    let Some(src) = src else {
        let text = caption
            .or_else(|| alt.clone())
            .unwrap_or_else(|| image_fallback_text(None, width, height));
        return (!text.trim().is_empty()).then(|| Block::Paragraph(text));
    };
    let (id, data) = match resolve(&src) {
        Some((id, data)) => (id, Some(data)),
        None => (src.clone(), None),
    };
    Some(Block::Image(ImageBlock {
        id,
        data,
        alt,
        caption,
        width,
        height,
    }))
}

fn image_label_text(attrs: &kuchiki::Attributes) -> Option<String> {
    let label = attrs
        .get("alt")
        .or_else(|| attrs.get("title"))
        .or_else(|| attrs.get("aria-label"));
    label
        .map(normalize_inline_text)
        .filter(|label| !label.is_empty())
}

fn image_dimensions_text(attrs: &kuchiki::Attributes) -> Option<String> {
    let width = parse_dimension(attrs.get("width"));
    let height = parse_dimension(attrs.get("height"));
    match (width, height) {
        (Some(w), Some(h)) => Some(format!("{}x{}", w, h)),
        _ => None,
    }
}

fn image_dimensions(attrs: &kuchiki::Attributes) -> (Option<u32>, Option<u32>) {
    (
        parse_dimension(attrs.get("width")),
        parse_dimension(attrs.get("height")),
    )
}

fn image_src(attrs: &kuchiki::Attributes) -> Option<String> {
    let raw = attrs
        .get("src")
        .or_else(|| attrs.get("data-src"))
        .or_else(|| attrs.get("data-original"))?;
    let mut s = raw.trim().to_string();
    if let Some(pos) = s.find('#') {
        s.truncate(pos);
    }
    if let Some(pos) = s.find('?') {
        s.truncate(pos);
    }
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn image_fallback_text(alt: Option<&str>, width: Option<u32>, height: Option<u32>) -> String {
    if let Some(alt) = alt {
        let alt = alt.trim();
        if !alt.is_empty() {
            return format!("Image: {}", alt);
        }
    }
    match (width, height) {
        (Some(w), Some(h)) => format!("Image ({}x{})", w, h),
        _ => "Image".to_string(),
    }
}

fn parse_dimension(value: Option<&str>) -> Option<u32> {
    let value = value?;
    let digits: String = value.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<u32>().ok()
    }
}

fn table_block<F>(node: &NodeRef, ctx: &mut InlineContext<'_, F>) -> Option<Block>
where
    F: FnMut(&str) -> Option<String>,
{
    let mut rows: Vec<String> = Vec::new();
    if let Ok(trs) = node.select("tr") {
        for tr in trs {
            let mut cells: Vec<String> = Vec::new();
            for child in tr.as_node().children() {
                if let Some(el) = child.as_element() {
                    let tag = el.name.local.to_lowercase();
                    if tag == "td" || tag == "th" {
                        let cell = inline_text(&child, ctx);
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
        let fallback = inline_text(node, ctx);
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

fn definition_list_block<F>(node: &NodeRef, ctx: &mut InlineContext<'_, F>) -> Option<Block>
where
    F: FnMut(&str) -> Option<String>,
{
    let mut items: Vec<String> = Vec::new();
    let mut current_term: Option<String> = None;
    for child in node.children() {
        if let Some(el) = child.as_element() {
            let tag = el.name.local.to_lowercase();
            if tag == "dt" {
                let term = inline_text(&child, ctx);
                if !term.is_empty() {
                    current_term = Some(term);
                }
            } else if tag == "dd" {
                let definition = inline_text(&child, ctx);
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
            Block::Image(ref mut img) => {
                if let Some(caption) = img.caption.as_mut() {
                    *caption = clean_text(caption, false);
                }
                if let Some(alt) = img.alt.as_mut() {
                    *alt = clean_text(alt, false);
                }
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

    fn strip_inline_markers(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut chars = input.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == STYLE_START || ch == STYLE_END {
                let _ = chars.next();
                continue;
            }
            if ch == LINK_START {
                while let Some(next) = chars.next() {
                    if next == LINK_END {
                        break;
                    }
                }
                continue;
            }
            if ch == ANCHOR_START {
                while let Some(next) = chars.next() {
                    if next == ANCHOR_END {
                        break;
                    }
                }
                continue;
            }
            out.push(ch);
        }
        out
    }

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
        let Block::Paragraph(text) = &blocks[0] else {
            panic!("expected paragraph");
        };
        assert!(text.contains(STYLE_START));
        let stripped = strip_inline_markers(text);
        assert!(matches!(
            blocks[0],
            Block::Paragraph(ref t)
                if strip_inline_markers(t)
                    == "Read this and that, see link (https://example.com)."
        ));
        assert_eq!(
            stripped,
            "Read this and that, see link (https://example.com)."
        );
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
