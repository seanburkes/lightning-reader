use kuchiki::{traits::*, NodeRef};
use quick_xml::events::Event;
use quick_xml::Reader as XmlReader;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::{cell::RefCell, fs::File};
use zip::ZipArchive;

use crate::epub::ReaderError;
use crate::types::TocEntry;

fn read_file_to_string(zip: &mut ZipArchive<File>, path: &Path) -> Result<String, ReaderError> {
    let mut f = zip.by_name(path.to_string_lossy().as_ref())?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    Ok(s)
}

fn normalize_href_with_fragment(base_file: &Path, href: &str) -> String {
    let base_dir = base_file.parent().unwrap_or(Path::new(""));
    let (path_part, frag) = href
        .split_once('#')
        .map_or((href, None), |(p, f)| (p, Some(f)));
    let joined = if path_part.is_empty() {
        base_file.to_path_buf()
    } else {
        base_dir.join(path_part)
    };
    let mut out = normalize_path(&joined).to_string_lossy().to_string();
    if let Some(frag) = frag {
        if !frag.is_empty() {
            out.push('#');
            out.push_str(frag);
        }
    }
    out
}

fn parse_epub3_nav_entries(html: &str, nav_path: &Path) -> Vec<TocEntry> {
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
                    entries.push(TocEntry {
                        href: full,
                        label,
                        level: 0,
                    });
                }
            }
        }
    }
    entries
}

fn parse_epub3_nav(html: &str, nav_path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for entry in parse_epub3_nav_entries(html, nav_path) {
        if !entry.label.is_empty() && !entry.href.is_empty() {
            let key = strip_fragment(&entry.href).to_string();
            map.entry(key).or_insert(entry.label);
        }
    }
    map
}

fn parse_epub2_ncx_entries(xml: &str, ncx_path: &Path) -> Vec<TocEntry> {
    let mut entries = Vec::new();
    let mut reader = XmlReader::from_str(xml);
    let mut current_label: Option<String> = None;
    let mut depth: usize = 0;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name.ends_with("navPoint") {
                    depth = depth.saturating_add(1);
                }
                if name.ends_with("text") {
                    if let Ok(Event::Text(t)) = reader.read_event() {
                        current_label = Some(String::from_utf8_lossy(t.as_ref()).to_string());
                    }
                } else if name.ends_with("content") {
                    for a in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(a.key.as_ref());
                        if key.ends_with("src") {
                            if let Ok(val) = a.unescape_value() {
                                let href = val.into_owned();
                                if let Some(label) = current_label.clone() {
                                    let full = normalize_href_with_fragment(ncx_path, &href);
                                    entries.push(TocEntry {
                                        href: full,
                                        label,
                                        level: depth.saturating_sub(1),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name.ends_with("navPoint") {
                    depth = depth.saturating_sub(1);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    entries
}

fn parse_epub2_ncx(xml: &str, ncx_path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for entry in parse_epub2_ncx_entries(xml, ncx_path) {
        if !entry.label.is_empty() && !entry.href.is_empty() {
            let key = strip_fragment(&entry.href).to_string();
            map.entry(key).or_insert(entry.label);
        }
    }
    map
}

pub fn read_nav_labels(
    zip_path: &Path,
    opf_path: &Path,
) -> Result<HashMap<String, String>, ReaderError> {
    read_nav_labels_with_hints(zip_path, opf_path, None, None)
}

pub fn read_nav_labels_with_hints(
    zip_path: &Path,
    opf_path: &Path,
    nav_href: Option<&str>,
    ncx_href: Option<&str>,
) -> Result<HashMap<String, String>, ReaderError> {
    let file = File::open(zip_path)?;
    let mut zip = ZipArchive::new(file)?;
    read_nav_labels_from_archive_inner(&mut zip, opf_path, nav_href, ncx_href)
}

pub fn read_nav_labels_from_archive(
    zip: &RefCell<ZipArchive<File>>,
    opf_path: &Path,
) -> Result<HashMap<String, String>, ReaderError> {
    read_nav_labels_from_archive_with_hints(zip, opf_path, None, None)
}

pub fn read_nav_labels_from_archive_with_hints(
    zip: &RefCell<ZipArchive<File>>,
    opf_path: &Path,
    nav_href: Option<&str>,
    ncx_href: Option<&str>,
) -> Result<HashMap<String, String>, ReaderError> {
    let mut borrow = zip.borrow_mut();
    read_nav_labels_from_archive_inner(&mut borrow, opf_path, nav_href, ncx_href)
}

pub fn read_nav_entries(zip_path: &Path, opf_path: &Path) -> Result<Vec<TocEntry>, ReaderError> {
    read_nav_entries_with_hints(zip_path, opf_path, None, None)
}

pub fn read_nav_entries_with_hints(
    zip_path: &Path,
    opf_path: &Path,
    nav_href: Option<&str>,
    ncx_href: Option<&str>,
) -> Result<Vec<TocEntry>, ReaderError> {
    let file = File::open(zip_path)?;
    let mut zip = ZipArchive::new(file)?;
    read_nav_entries_from_archive_inner(&mut zip, opf_path, nav_href, ncx_href)
}

pub fn read_nav_entries_from_archive(
    zip: &RefCell<ZipArchive<File>>,
    opf_path: &Path,
) -> Result<Vec<TocEntry>, ReaderError> {
    read_nav_entries_from_archive_with_hints(zip, opf_path, None, None)
}

pub fn read_nav_entries_from_archive_with_hints(
    zip: &RefCell<ZipArchive<File>>,
    opf_path: &Path,
    nav_href: Option<&str>,
    ncx_href: Option<&str>,
) -> Result<Vec<TocEntry>, ReaderError> {
    let mut borrow = zip.borrow_mut();
    read_nav_entries_from_archive_inner(&mut borrow, opf_path, nav_href, ncx_href)
}

fn read_nav_labels_from_archive_inner(
    zip: &mut ZipArchive<File>,
    opf_path: &Path,
    nav_href: Option<&str>,
    ncx_href: Option<&str>,
) -> Result<HashMap<String, String>, ReaderError> {
    let base = opf_path.parent().unwrap_or(Path::new(""));

    if let Some(href) = nav_href {
        let candidate = base.join(strip_fragment(href));
        if zip.by_name(candidate.to_string_lossy().as_ref()).is_ok() {
            let s = read_file_to_string(zip, &candidate)?;
            let labels = parse_epub3_nav(&s, &candidate);
            if !labels.is_empty() {
                return Ok(labels);
            }
        }
    }

    // Try EPUB3: nav.xhtml or toc.xhtml in OPF directory
    for name in ["nav.xhtml", "toc.xhtml"] {
        let candidate = base.join(name);
        if zip.by_name(candidate.to_string_lossy().as_ref()).is_ok() {
            let s = read_file_to_string(zip, &candidate)?;
            let labels = parse_epub3_nav(&s, &candidate);
            if !labels.is_empty() {
                return Ok(labels);
            }
        }
    }

    if let Some(href) = ncx_href {
        let candidate = base.join(strip_fragment(href));
        if zip.by_name(candidate.to_string_lossy().as_ref()).is_ok() {
            let s = read_file_to_string(zip, &candidate)?;
            let labels = parse_epub2_ncx(&s, &candidate);
            if !labels.is_empty() {
                return Ok(labels);
            }
        }
    }

    // Try EPUB2: toc.ncx
    let ncx = base.join("toc.ncx");
    if zip.by_name(ncx.to_string_lossy().as_ref()).is_ok() {
        let s = read_file_to_string(zip, &ncx)?;
        let labels = parse_epub2_ncx(&s, &ncx);
        if !labels.is_empty() {
            return Ok(labels);
        }
    }

    Ok(HashMap::new())
}

fn read_nav_entries_from_archive_inner(
    zip: &mut ZipArchive<File>,
    opf_path: &Path,
    nav_href: Option<&str>,
    ncx_href: Option<&str>,
) -> Result<Vec<TocEntry>, ReaderError> {
    let base = opf_path.parent().unwrap_or(Path::new(""));

    if let Some(href) = nav_href {
        let candidate = base.join(strip_fragment(href));
        if zip.by_name(candidate.to_string_lossy().as_ref()).is_ok() {
            let s = read_file_to_string(zip, &candidate)?;
            let entries = parse_epub3_nav_entries(&s, &candidate);
            if !entries.is_empty() {
                return Ok(entries);
            }
        }
    }

    for name in ["nav.xhtml", "toc.xhtml"] {
        let candidate = base.join(name);
        if zip.by_name(candidate.to_string_lossy().as_ref()).is_ok() {
            let s = read_file_to_string(zip, &candidate)?;
            let entries = parse_epub3_nav_entries(&s, &candidate);
            if !entries.is_empty() {
                return Ok(entries);
            }
        }
    }

    if let Some(href) = ncx_href {
        let candidate = base.join(strip_fragment(href));
        if zip.by_name(candidate.to_string_lossy().as_ref()).is_ok() {
            let s = read_file_to_string(zip, &candidate)?;
            let entries = parse_epub2_ncx_entries(&s, &candidate);
            if !entries.is_empty() {
                return Ok(entries);
            }
        }
    }

    let ncx = base.join("toc.ncx");
    if zip.by_name(ncx.to_string_lossy().as_ref()).is_ok() {
        let s = read_file_to_string(zip, &ncx)?;
        let entries = parse_epub2_ncx_entries(&s, &ncx);
        if !entries.is_empty() {
            return Ok(entries);
        }
    }

    Ok(Vec::new())
}

fn strip_fragment(href: &str) -> &str {
    href.split('#').next().unwrap_or(href)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            _ => out.push(comp.as_os_str()),
        }
    }
    out
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
                out.push(TocEntry {
                    href: full,
                    label,
                    level,
                });
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
        assert_eq!(entries[0].label, "Chapter 1");
        assert_eq!(entries[0].level, 0);
        assert_eq!(entries[0].href, "OEBPS/text/ch1.xhtml#c1");
        assert_eq!(entries[1].label, "Section 1");
        assert_eq!(entries[1].level, 1);
        assert_eq!(entries[1].href, "OEBPS/text/ch1.xhtml#c1-1");
        assert_eq!(entries[2].label, "Chapter 2");
        assert_eq!(entries[2].level, 0);
    }

    #[test]
    fn parse_epub2_ncx_nested_levels() {
        let xml = r#"
        <ncx>
          <navMap>
            <navPoint id="p1">
              <navLabel><text>Chapter 1</text></navLabel>
              <content src="text/ch1.xhtml#c1"/>
              <navPoint id="p1-1">
                <navLabel><text>Section 1</text></navLabel>
                <content src="text/ch1.xhtml#c1-1"/>
              </navPoint>
            </navPoint>
          </navMap>
        </ncx>
        "#;
        let ncx_path = Path::new("OEBPS/toc.ncx");
        let entries = parse_epub2_ncx_entries(xml, ncx_path);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].label, "Chapter 1");
        assert_eq!(entries[0].level, 0);
        assert_eq!(entries[0].href, "OEBPS/text/ch1.xhtml#c1");
        assert_eq!(entries[1].label, "Section 1");
        assert_eq!(entries[1].level, 1);
        assert_eq!(entries[1].href, "OEBPS/text/ch1.xhtml#c1-1");
    }
}
