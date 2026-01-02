use std::path::Path;

use kuchiki::{traits::*, NodeRef};

use crate::types::TocEntry;

use super::paths::normalize_href_with_fragment;

pub(crate) fn parse_epub3_nav_entries(html: &str, nav_path: &Path) -> Vec<TocEntry> {
    let mut entries = Vec::new();
    let doc = kuchiki::parse_html().one(html.to_string());

    let nav_nodes: Vec<NodeRef> = doc
        .select("nav")
        .ok()
        .map(|iter| iter.map(|n| n.as_node().clone()).collect())
        .unwrap_or_default();

    let toc_nav = nav_nodes
        .iter()
        .find(|node| node.as_element().is_some_and(nav_is_toc))
        .cloned()
        .or_else(|| nav_nodes.first().cloned());

    if let Some(nav) = toc_nav {
        if let Ok(mut lists) = nav.select("ol, ul") {
            if let Some(list) = lists.next() {
                parse_nav_list(list.as_node(), nav_path, 0, &mut entries);
                return entries;
            }
        }
        parse_nav_list(&nav, nav_path, 0, &mut entries);
        return entries;
    }

    // Fallback: scan all anchors in document
    if let Ok(anchors) = doc.select("a[href]") {
        for anchor in anchors {
            if let Some((href, label)) = anchor_href_label(anchor.as_node()) {
                let full = normalize_href_with_fragment(nav_path, &href);
                if !label.is_empty() {
                    entries.push(TocEntry::new(full, label, 0));
                }
            }
        }
    }
    entries
}

pub(crate) fn parse_epub3_nav(
    html: &str,
    nav_path: &Path,
) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for entry in parse_epub3_nav_entries(html, nav_path) {
        if !entry.label().is_empty() && !entry.href().is_empty() {
            let key = super::paths::strip_fragment(entry.href()).to_string();
            map.entry(key).or_insert_with(|| entry.label().to_string());
        }
    }
    map
}

fn nav_is_toc(el: &kuchiki::ElementData) -> bool {
    let attrs = el.attributes.borrow();
    let epub_type = attrs.get("epub:type").or_else(|| attrs.get("type"));
    let role = attrs.get("role");
    if let Some(epub_type) = epub_type {
        if attr_has_token(epub_type, "toc") {
            return true;
        }
    }
    if let Some(role) = role {
        if attr_has_token(role, "doc-toc") {
            return true;
        }
    }
    false
}

fn attr_has_token(value: &str, needle: &str) -> bool {
    value
        .split_whitespace()
        .any(|token| token.eq_ignore_ascii_case(needle))
}

fn parse_nav_list(list: &NodeRef, nav_path: &Path, level: usize, out: &mut Vec<TocEntry>) {
    for child in list.children() {
        let Some(el) = child.as_element() else {
            continue;
        };
        if el.name.local.as_ref() != "li" {
            continue;
        }
        if let Some((href, label)) = find_li_link_label(&child) {
            let full = normalize_href_with_fragment(nav_path, &href);
            if !label.is_empty() {
                out.push(TocEntry::new(full, label, level));
            }
        }
        for nested in child.children() {
            let Some(nested_el) = nested.as_element() else {
                continue;
            };
            let tag = nested_el.name.local.as_ref();
            if tag == "ol" || tag == "ul" {
                parse_nav_list(&nested, nav_path, level.saturating_add(1), out);
            }
        }
    }
}

fn find_li_link_label(li: &NodeRef) -> Option<(String, String)> {
    find_link_label_skipping_lists(li)
}

fn find_link_label_skipping_lists(node: &NodeRef) -> Option<(String, String)> {
    for child in node.children() {
        if let Some(el) = child.as_element() {
            let tag = el.name.local.as_ref();
            if tag == "ol" || tag == "ul" {
                continue;
            }
            if tag == "a" {
                return anchor_href_label(&child);
            }
            if let Some(found) = find_link_label_skipping_lists(&child) {
                return Some(found);
            }
        }
    }
    None
}

fn anchor_href_label(anchor: &NodeRef) -> Option<(String, String)> {
    let el = anchor.as_element()?;
    let href = el.attributes.borrow().get("href")?.to_string();
    let label = normalize_label(&anchor.text_contents());
    Some((href, label))
}

fn normalize_label(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut last_space = false;
    for ch in raw.chars() {
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
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_epub3_nav_with_nested_levels() {
        let html = r#"
        <nav epub:type="toc">
          <h2>Contents</h2>
          <ol>
            <li><a href="../text/ch1.xhtml#c1">Chapter 1</a>
              <ol>
                <li><a href="../text/ch1.xhtml#c1-1">Section 1</a></li>
              </ol>
            </li>
            <li><a href="../text/ch2.xhtml">Chapter 2</a></li>
          </ol>
        </nav>
        "#;
        let nav_path = Path::new("OEBPS/nav/nav.xhtml");
        let entries = parse_epub3_nav_entries(html, nav_path);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].label(), "Chapter 1");
        assert_eq!(entries[0].level(), 0);
        assert_eq!(entries[0].href(), "OEBPS/text/ch1.xhtml#c1");
        assert_eq!(entries[1].label(), "Section 1");
        assert_eq!(entries[1].level(), 1);
        assert_eq!(entries[1].href(), "OEBPS/text/ch1.xhtml#c1-1");
        assert_eq!(entries[2].label(), "Chapter 2");
        assert_eq!(entries[2].level(), 0);
    }
}
