use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader as XmlReader;

use crate::types::TocEntry;

use super::paths::normalize_href_with_fragment;

pub(crate) fn parse_epub2_ncx_entries(xml: &str, ncx_path: &Path) -> Vec<TocEntry> {
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
                                    entries.push(TocEntry::new(
                                        full,
                                        label,
                                        depth.saturating_sub(1),
                                    ));
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

pub(crate) fn parse_epub2_ncx(
    xml: &str,
    ncx_path: &Path,
) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for entry in parse_epub2_ncx_entries(xml, ncx_path) {
        if !entry.label().is_empty() && !entry.href().is_empty() {
            let key = super::paths::strip_fragment(entry.href()).to_string();
            map.entry(key).or_insert_with(|| entry.label().to_string());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(entries[0].label(), "Chapter 1");
        assert_eq!(entries[0].level(), 0);
        assert_eq!(entries[0].href(), "OEBPS/text/ch1.xhtml#c1");
        assert_eq!(entries[1].label(), "Section 1");
        assert_eq!(entries[1].level(), 1);
        assert_eq!(entries[1].href(), "OEBPS/text/ch1.xhtml#c1-1");
    }
}
