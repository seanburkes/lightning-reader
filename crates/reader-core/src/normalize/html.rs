use crate::types::{Block, ImageBlock};
use kuchiki::{traits::*, NodeRef};

use super::images::{image_dimensions, image_fallback_text, image_label_text, image_src};
use super::inline::{
    append_inline_text, inline_text, list_item_text, normalize_inline_text, InlineContext,
};
use super::table::{definition_list_block, table_block};

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
        fn flush_pending(pending: &mut String, out: &mut Vec<Block>) {
            if pending.trim().is_empty() {
                pending.clear();
                return;
            }
            let text = normalize_inline_text(pending);
            pending.clear();
            if !text.is_empty() {
                out.push(Block::Paragraph(text));
            }
        }

        fn is_inline_tag(tag: &str) -> bool {
            matches!(
                tag,
                "a" | "abbr"
                    | "b"
                    | "br"
                    | "cite"
                    | "code"
                    | "del"
                    | "em"
                    | "i"
                    | "kbd"
                    | "mark"
                    | "q"
                    | "s"
                    | "samp"
                    | "small"
                    | "span"
                    | "strike"
                    | "strong"
                    | "sub"
                    | "sup"
                    | "u"
            )
        }

        fn is_skippable_tag(tag: &str) -> bool {
            matches!(
                tag,
                "head" | "meta" | "link" | "script" | "style" | "noscript"
            )
        }

        let mut pending = String::new();
        for child in node.children() {
            if let Some(block) = extract_block(&child, ctx, resolve_image) {
                flush_pending(&mut pending, out);
                out.push(block);
                continue;
            }
            if let Some(el) = child.as_element() {
                let tag = el.name.local.to_lowercase();
                if is_skippable_tag(&tag) {
                    continue;
                }
                if is_inline_tag(&tag) {
                    append_inline_text(&child, &mut pending, ctx);
                    continue;
                }
            } else if child.as_text().is_some() {
                append_inline_text(&child, &mut pending, ctx);
                continue;
            }
            flush_pending(&mut pending, out);
            collect(&child, out, ctx, resolve_image);
        }
        flush_pending(&mut pending, out);
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

fn image_block<F>(node: &NodeRef, resolve: &mut F) -> Option<Block>
where
    F: FnMut(&str) -> Option<(String, Vec<u8>)>,
{
    let el = node.as_element()?;
    let attrs = el.attributes.borrow();
    let src = image_src(&attrs);
    let alt = image_label_text(&attrs);
    let (width, height) = image_dimensions(&attrs);
    let Some(src) = src else {
        let text = image_fallback_text(alt.as_deref(), width, height);
        return (!text.is_empty()).then_some(Block::Paragraph(text));
    };
    let (id, data) = match resolve(&src) {
        Some((id, data)) => (id, Some(data)),
        None => (src.clone(), None),
    };
    Some(Block::Image(ImageBlock::new(
        id, data, alt, None, width, height,
    )))
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
        return (!text.trim().is_empty()).then_some(Block::Paragraph(text));
    };
    let (id, data) = match resolve(&src) {
        Some((id, data)) => (id, Some(data)),
        None => (src.clone(), None),
    };
    Some(Block::Image(ImageBlock::new(
        id, data, alt, caption, width, height,
    )))
}
