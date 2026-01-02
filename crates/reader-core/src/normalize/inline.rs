use kuchiki::NodeRef;

pub(crate) const STYLE_START: char = '\x1E';
pub(crate) const STYLE_END: char = '\x1F';
const BR_MARKER: char = '\x1A';
pub(crate) const LINK_START: char = '\x1C';
pub(crate) const LINK_END: char = '\x1D';
pub(crate) const ANCHOR_START: char = '\x18';
pub(crate) const ANCHOR_END: char = '\x17';

pub(crate) struct InlineContext<'a, F>
where
    F: FnMut(&str) -> Option<String>,
{
    pub(crate) resolve_link: &'a mut F,
    pub(crate) anchor_prefix: Option<&'a str>,
}

pub(crate) fn inline_text<F>(node: &NodeRef, ctx: &mut InlineContext<'_, F>) -> String
where
    F: FnMut(&str) -> Option<String>,
{
    let mut out = String::new();
    append_inline_text(node, &mut out, ctx);
    normalize_inline_text(&out)
}

pub(crate) fn append_inline_text<F>(
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

pub(crate) fn list_item_text<F>(node: &NodeRef, ctx: &mut InlineContext<'_, F>) -> String
where
    F: FnMut(&str) -> Option<String>,
{
    let mut out = String::new();
    append_inline_text_without_lists(node, &mut out, ctx);
    normalize_inline_text(&out)
}

pub(crate) fn normalize_inline_text(s: &str) -> String {
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

pub(crate) fn normalize_lines(s: &str) -> String {
    let mut out_lines = Vec::new();
    for line in s.split('\n') {
        out_lines.push(normalize_line(line));
    }
    let out = out_lines.join("\n");
    out.trim().to_string()
}

pub(crate) fn normalize_line(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = false;
    for ch in s.chars() {
        if ch == '\u{00AD}' {
            continue;
        }
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

fn is_external_href(href: &str) -> bool {
    let lower = href.trim().to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:")
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
    if let Some(label) = inline_image_label_text(&attrs) {
        return label;
    }
    if let Some(dim) = inline_image_dimensions_text(&attrs) {
        return format!("Image ({})", dim);
    }
    "Image".to_string()
}

fn inline_image_label_text(attrs: &kuchiki::Attributes) -> Option<String> {
    let label = attrs
        .get("alt")
        .or_else(|| attrs.get("title"))
        .or_else(|| attrs.get("aria-label"));
    label
        .map(normalize_inline_text)
        .filter(|label| !label.is_empty())
}

fn inline_image_dimensions_text(attrs: &kuchiki::Attributes) -> Option<String> {
    let width = inline_parse_dimension(attrs.get("width"));
    let height = inline_parse_dimension(attrs.get("height"));
    match (width, height) {
        (Some(w), Some(h)) => Some(format!("{}x{}", w, h)),
        _ => None,
    }
}

fn inline_parse_dimension(value: Option<&str>) -> Option<u32> {
    let value = value?;
    let digits: String = value.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<u32>().ok()
    }
}
