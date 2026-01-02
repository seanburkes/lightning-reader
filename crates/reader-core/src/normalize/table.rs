use crate::types::{Block, TableBlock, TableCell};
use kuchiki::NodeRef;

use super::inline::{inline_text, InlineContext};

pub(crate) fn table_block<F>(node: &NodeRef, ctx: &mut InlineContext<'_, F>) -> Option<Block>
where
    F: FnMut(&str) -> Option<String>,
{
    let mut rows: Vec<Vec<TableCell>> = Vec::new();
    if let Ok(trs) = node.select("tr") {
        for tr in trs {
            let mut cells: Vec<TableCell> = Vec::new();
            let mut has_text = false;
            for child in tr.as_node().children() {
                if let Some(el) = child.as_element() {
                    let tag = el.name.local.to_lowercase();
                    if tag == "td" || tag == "th" {
                        let cell = inline_text(&child, ctx);
                        if !cell.trim().is_empty() {
                            has_text = true;
                        }
                        cells.push(TableCell::new(cell, tag == "th"));
                    }
                }
            }
            if !cells.is_empty() && has_text {
                rows.push(cells);
            }
        }
    }
    if rows.is_empty() {
        let fallback = inline_text(node, ctx);
        if fallback.is_empty() {
            None
        } else {
            Some(Block::Paragraph(fallback))
        }
    } else {
        Some(Block::Table(TableBlock::new(rows)))
    }
}

pub(crate) fn definition_list_block<F>(
    node: &NodeRef,
    ctx: &mut InlineContext<'_, F>,
) -> Option<Block>
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
