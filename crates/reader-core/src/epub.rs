use quick_xml::events::Event;
use quick_xml::Reader as XmlReader;
use std::{
    cell::RefCell,
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
use thiserror::Error;
use zip::ZipArchive;

#[derive(Debug, Error)]
pub enum ReaderError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Parse error: {0}")]
    Parse(String),
}

#[derive(Debug, Clone)]
pub struct SpineItem {
    pub id: String,
    pub href: String,
    pub media_type: Option<String>,
}

pub struct EpubBook {
    pub title: Option<String>,
    pub author: Option<String>,
    pub spine: Vec<SpineItem>,
    nav_href: Option<String>,
    ncx_href: Option<String>,
    rootfile: PathBuf,
    zip: RefCell<ZipArchive<File>>,
    chapter_cache: RefCell<HashMap<String, String>>,
}

fn read_container(zip: &mut ZipArchive<File>) -> Result<PathBuf, ReaderError> {
    let mut container = zip.by_name("META-INF/container.xml")?;
    let mut xml = String::new();
    container.read_to_string(&mut xml)?;
    let mut reader = XmlReader::from_str(&xml);
    let mut rootfile_path: Option<String> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name.contains("rootfile") {
                    for a in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(a.key.as_ref());
                        if key.contains("full-path") {
                            let val = a
                                .unescape_value()
                                .map_err(|e| ReaderError::Parse(e.to_string()))?;
                            rootfile_path = Some(val.into_owned());
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(ReaderError::Parse(e.to_string())),
            _ => {}
        }
    }
    let root = rootfile_path.ok_or_else(|| ReaderError::Parse("missing rootfile".into()))?;
    Ok(PathBuf::from(root))
}

struct ManifestItem {
    id: String,
    href: String,
    media_type: Option<String>,
    properties: Option<String>,
}

type OpfResult = (
    Option<String>,
    Vec<ManifestItem>,
    Vec<String>,
    Option<String>,
    Option<String>,
);

fn read_opf(zip: &mut ZipArchive<File>, opf_path: &Path) -> Result<OpfResult, ReaderError> {
    let mut opf = zip.by_name(opf_path.to_string_lossy().as_ref())?;
    let mut opf_xml = String::new();
    opf.read_to_string(&mut opf_xml)?;
    let mut reader = XmlReader::from_str(&opf_xml);
    let mut manifest: Vec<ManifestItem> = Vec::new();
    let mut spine_ids: Vec<String> = Vec::new();
    let mut titles: Vec<String> = Vec::new();
    let mut creators: Vec<String> = Vec::new();
    let mut spine_toc: Option<String> = None;
    let mut in_metadata = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = local_name(&name);
                if local == "metadata" {
                    in_metadata = true;
                }
                if name.ends_with("item") {
                    let mut id = String::new();
                    let mut href = String::new();
                    let mut media: Option<String> = None;
                    let mut properties: Option<String> = None;
                    for a in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(a.key.as_ref());
                        let val = a
                            .unescape_value()
                            .map_err(|e| ReaderError::Parse(e.to_string()))?;
                        let sval = val.into_owned();
                        if key.ends_with("id") {
                            id = sval.clone();
                        }
                        if key.ends_with("href") {
                            href = sval.clone();
                        }
                        if key.ends_with("media-type") {
                            media = Some(sval);
                        } else if key.ends_with("properties") {
                            properties = Some(sval);
                        }
                    }
                    if !id.is_empty() && !href.is_empty() {
                        manifest.push(ManifestItem {
                            id,
                            href,
                            media_type: media,
                            properties,
                        });
                    }
                } else if name.ends_with("itemref") {
                    for a in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(a.key.as_ref());
                        let val = a
                            .unescape_value()
                            .map_err(|e| ReaderError::Parse(e.to_string()))?;
                        let sval = val.into_owned();
                        if key.ends_with("idref") {
                            spine_ids.push(sval);
                        }
                    }
                } else if local == "spine" && spine_toc.is_none() {
                    for a in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(a.key.as_ref());
                        if key.ends_with("toc") {
                            let val = a
                                .unescape_value()
                                .map_err(|e| ReaderError::Parse(e.to_string()))?;
                            spine_toc = Some(val.into_owned());
                        }
                    }
                } else if in_metadata && local == "title" {
                    if let Some(text) = read_text_value(&mut reader) {
                        let text = normalize_meta_text(&text);
                        if !text.is_empty() {
                            titles.push(text);
                        }
                    }
                } else if in_metadata && (local == "creator" || local == "author") {
                    if let Some(text) = read_text_value(&mut reader) {
                        let text = normalize_meta_text(&text);
                        if !text.is_empty() {
                            creators.push(text);
                        }
                    }
                } else if in_metadata && local == "meta" {
                    let mut meta_name: Option<String> = None;
                    let mut meta_property: Option<String> = None;
                    let mut meta_content: Option<String> = None;
                    for a in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(a.key.as_ref());
                        let val = a
                            .unescape_value()
                            .map_err(|e| ReaderError::Parse(e.to_string()))?;
                        let sval = val.into_owned();
                        let attr = local_name(&key);
                        match attr {
                            "name" => meta_name = Some(sval),
                            "property" => meta_property = Some(sval),
                            "content" => meta_content = Some(sval),
                            _ => {}
                        }
                    }
                    if let Some(content) = meta_content {
                        let content = normalize_meta_text(&content);
                        if !content.is_empty() {
                            if meta_matches(&meta_name, &meta_property, "title") {
                                titles.push(content.clone());
                            } else if meta_matches(&meta_name, &meta_property, "creator")
                                || meta_matches(&meta_name, &meta_property, "author")
                            {
                                creators.push(content.clone());
                            }
                        }
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if local_name(&name) == "metadata" {
                    in_metadata = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(ReaderError::Parse(e.to_string())),
            _ => {}
        }
    }
    let title = titles.into_iter().next();
    let author = if creators.is_empty() {
        None
    } else {
        Some(creators.join(", "))
    };
    Ok((title, manifest, spine_ids, author, spine_toc))
}

impl EpubBook {
    pub fn open(path: &Path) -> Result<Self, ReaderError> {
        let file = std::fs::File::open(path)?;
        let mut zip = ZipArchive::new(file)?;
        let rootfile = read_container(&mut zip)?;
        let (title, manifest, spine_ids, author, spine_toc) = read_opf(&mut zip, &rootfile)?;
        let nav_href = manifest
            .iter()
            .find(|item| {
                item.properties
                    .as_deref()
                    .map(properties_has_nav)
                    .unwrap_or(false)
            })
            .map(|item| item.href.clone());
        let ncx_href = spine_toc
            .as_deref()
            .and_then(|toc_id| {
                manifest
                    .iter()
                    .find(|item| item.id == toc_id)
                    .map(|item| item.href.clone())
            })
            .or_else(|| {
                manifest
                    .iter()
                    .find(|item| {
                        item.media_type
                            .as_deref()
                            .map(is_ncx_media_type)
                            .unwrap_or(false)
                    })
                    .map(|item| item.href.clone())
            });
        let spine = spine_ids
            .into_iter()
            .filter_map(|idref| {
                manifest
                    .iter()
                    .find(|item| item.id == idref)
                    .map(|item| SpineItem {
                        id: idref.clone(),
                        href: item.href.clone(),
                        media_type: item.media_type.clone(),
                    })
            })
            .collect();
        Ok(Self {
            title,
            author,
            spine,
            rootfile,
            zip: RefCell::new(zip),
            chapter_cache: RefCell::new(HashMap::new()),
            nav_href,
            ncx_href,
        })
    }

    pub fn spine(&self) -> &[SpineItem] {
        &self.spine
    }

    pub fn toc_labels(&self) -> Result<std::collections::HashMap<String, String>, ReaderError> {
        // Read directly from the shared archive to avoid reopening
        crate::nav::read_nav_labels_from_archive_with_hints(
            &self.zip,
            &self.rootfile,
            self.nav_href.as_deref(),
            self.ncx_href.as_deref(),
        )
    }

    pub fn load_chapter(&self, item: &SpineItem) -> Result<String, ReaderError> {
        // Chapter path relative to OPF base
        let base = self.rootfile.parent().unwrap_or(Path::new(""));
        let chapter_path = base.join(&item.href).to_string_lossy().to_string();
        if let Some(cached) = self.chapter_cache.borrow().get(&chapter_path).cloned() {
            return Ok(cached);
        }
        let mut zip = self.zip.borrow_mut();
        let mut chapter = zip.by_name(&chapter_path)?;
        let mut s = String::new();
        chapter.read_to_string(&mut s)?;
        self.chapter_cache
            .borrow_mut()
            .insert(chapter_path, s.clone());
        Ok(s)
    }
}

fn properties_has_nav(properties: &str) -> bool {
    properties
        .split_whitespace()
        .any(|prop| prop.eq_ignore_ascii_case("nav"))
}

fn is_ncx_media_type(media_type: &str) -> bool {
    media_type.eq_ignore_ascii_case("application/x-dtbncx+xml")
}

fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn read_text_value(reader: &mut XmlReader<&[u8]>) -> Option<String> {
    match reader.read_event() {
        Ok(Event::Text(t)) => Some(String::from_utf8_lossy(t.as_ref()).to_string()),
        Ok(Event::CData(t)) => Some(String::from_utf8_lossy(t.as_ref()).to_string()),
        _ => None,
    }
}

fn normalize_meta_text(s: &str) -> String {
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
    out.trim().to_string()
}

fn meta_matches(name: &Option<String>, property: &Option<String>, needle: &str) -> bool {
    let matches = |value: &str| {
        let lower = value.to_ascii_lowercase();
        lower.contains(needle)
    };
    name.as_deref().map(matches).unwrap_or(false)
        || property.as_deref().map(matches).unwrap_or(false)
}
