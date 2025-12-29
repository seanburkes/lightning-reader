use quick_xml::events::Event;
use quick_xml::Reader as XmlReader;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::{cell::RefCell, fs::File};
use zip::ZipArchive;

use crate::epub::ReaderError;

fn read_file_to_string(zip: &mut ZipArchive<File>, path: &Path) -> Result<String, ReaderError> {
    let mut f = zip.by_name(path.to_string_lossy().as_ref())?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    Ok(s)
}

fn normalize_href(base: &Path, href: &str) -> String {
    // Strip fragment
    let href_no_frag = href.split('#').next().unwrap_or(href);
    base.join(href_no_frag).to_string_lossy().to_string()
}

fn parse_epub3_nav(html: &str, base: &Path) -> HashMap<String, String> {
    // Very light-weight anchor extraction: look for href="...">Label
    let mut map = HashMap::new();
    // Only parse within a <nav ... epub:type="toc"> block if present; otherwise parse all anchors
    let lower = html.to_lowercase();
    let nav_start = lower.find("<nav").unwrap_or(0);
    let toc_block = if lower[nav_start..].contains("epub:type=\"toc\"") {
        let end = lower[nav_start..]
            .find("</nav>")
            .map(|i| nav_start + i)
            .unwrap_or(html.len());
        &html[nav_start..end]
    } else {
        html
    };
    let mut rest = toc_block;
    while let Some(hidx) = rest.find("href=") {
        rest = &rest[hidx + 5..];
        let rest_trim = rest.trim_start();
        let quote = if rest_trim.starts_with('"') {
            '"'
        } else {
            '\''
        };
        if let Some(qs) = rest_trim.find(quote) {
            let after_q = &rest_trim[qs + 1..];
            if let Some(qe) = after_q.find(quote) {
                let href = &after_q[..qe];
                // Find label after '>' up to next '<'
                if let Some(gt) = after_q[qe + 1..].find('>') {
                    let after_gt = &after_q[qe + 1 + gt + 1..];
                    let label_end = after_gt.find('<').unwrap_or(after_gt.len());
                    let label = after_gt[..label_end].trim();
                    if !label.is_empty() {
                        let full = normalize_href(base, href);
                        map.entry(full).or_insert(label.to_string());
                    }
                }
                rest = &after_q[qe + 1..];
                continue;
            }
        }
        break;
    }
    map
}

fn parse_epub2_ncx(xml: &str, base: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut reader = XmlReader::from_str(xml);
    let mut current_label: Option<String> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
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
                                    let full = normalize_href(base, &href);
                                    map.entry(full).or_insert(label);
                                }
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
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
            let labels = parse_epub3_nav(&s, base);
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
            let labels = parse_epub3_nav(&s, base);
            if !labels.is_empty() {
                return Ok(labels);
            }
        }
    }

    if let Some(href) = ncx_href {
        let candidate = base.join(strip_fragment(href));
        if zip.by_name(candidate.to_string_lossy().as_ref()).is_ok() {
            let s = read_file_to_string(zip, &candidate)?;
            let labels = parse_epub2_ncx(&s, base);
            if !labels.is_empty() {
                return Ok(labels);
            }
        }
    }

    // Try EPUB2: toc.ncx
    let ncx = base.join("toc.ncx");
    if zip.by_name(ncx.to_string_lossy().as_ref()).is_ok() {
        let s = read_file_to_string(zip, &ncx)?;
        let labels = parse_epub2_ncx(&s, base);
        if !labels.is_empty() {
            return Ok(labels);
        }
    }

    Ok(HashMap::new())
}

fn strip_fragment(href: &str) -> &str {
    href.split('#').next().unwrap_or(href)
}
