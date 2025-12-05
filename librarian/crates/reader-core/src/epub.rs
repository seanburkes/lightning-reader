use thiserror::Error;
use std::{fs::File, io::Read, path::{Path, PathBuf}};
use quick_xml::events::Event;
use quick_xml::Reader as XmlReader;
use zip::ZipArchive;

#[derive(Debug, Error)]
pub enum ReaderError {
    #[error("IO error: {0}")] Io(#[from] std::io::Error),
    #[error("Zip error: {0}")] Zip(#[from] zip::result::ZipError),
    #[error("Parse error: {0}")] Parse(String),
}

#[derive(Debug, Clone)]
pub struct SpineItem {
    pub id: String,
    pub href: String,
    pub media_type: Option<String>,
}

pub struct EpubBook {
    pub title: Option<String>,
    pub spine: Vec<SpineItem>,
    rootfile: PathBuf,
    zip_path: PathBuf,
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
                            let val = a.unescape_value().map_err(|e| ReaderError::Parse(e.to_string()))?;
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

fn read_opf(zip: &mut ZipArchive<File>, opf_path: &Path) -> Result<(Option<String>, Vec<(String,String,Option<String>)>, Vec<String>), ReaderError> {
    let mut opf = zip.by_name(opf_path.to_string_lossy().as_ref())?;
    let mut opf_xml = String::new();
    opf.read_to_string(&mut opf_xml)?;
    let mut reader = XmlReader::from_str(&opf_xml);
    let mut manifest: Vec<(String, String, Option<String>)> = Vec::new();
    let mut spine_ids: Vec<String> = Vec::new();
    let mut title: Option<String> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name.ends_with("item") {
                    let mut id = String::new();
                    let mut href = String::new();
                    let mut media: Option<String> = None;
                    for a in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(a.key.as_ref());
                        let val = a.unescape_value().map_err(|e| ReaderError::Parse(e.to_string()))?;
                        let sval = val.into_owned();
                        if key.ends_with("id") { id = sval.clone(); }
                        if key.ends_with("href") { href = sval.clone(); }
                        if key.ends_with("media-type") { media = Some(sval); }
                    }
                    if !id.is_empty() && !href.is_empty() { manifest.push((id, href, media)); }
                } else if name.ends_with("itemref") {
                    for a in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(a.key.as_ref());
                        let val = a.unescape_value().map_err(|e| ReaderError::Parse(e.to_string()))?;
                        let sval = val.into_owned();
                        if key.ends_with("idref") { spine_ids.push(sval); }
                    }
                } else if name.ends_with("title") {
                    if let Ok(Event::Text(t)) = reader.read_event() {
                        let s = String::from_utf8_lossy(t.as_ref()).to_string();
                        title = Some(s);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(ReaderError::Parse(e.to_string())),
            _ => {}
        }
    }
    Ok((title, manifest, spine_ids))
}

impl EpubBook {
    pub fn open(path: &Path) -> Result<Self, ReaderError> {
        let file = std::fs::File::open(path)?;
        let mut zip = ZipArchive::new(file)?;
        let rootfile = read_container(&mut zip)?;
        let (title, manifest, spine_ids) = read_opf(&mut zip, &rootfile)?;
        let spine = spine_ids.into_iter().filter_map(|idref| {
            manifest.iter().find(|(id, _, _)| *id == idref).map(|(_, href, media)| SpineItem { id: idref.clone(), href: href.clone(), media_type: media.clone() })
        }).collect();
        Ok(Self { title, spine, rootfile, zip_path: path.to_path_buf() })
    }

    pub fn spine(&self) -> &[SpineItem] { &self.spine }

    pub fn load_chapter(&self, item: &SpineItem) -> Result<String, ReaderError> {
        // Chapter path relative to OPF base
        let base = self.rootfile.parent().unwrap_or(Path::new(""));
        let chapter_path = base.join(&item.href).to_string_lossy().to_string();
        // Reopen zip and read file by name
        let file = std::fs::File::open(&self.zip_path)?;
        let mut zip = ZipArchive::new(file)?;
        let mut chapter = zip.by_name(&chapter_path)?;
        let mut s = String::new();
        chapter.read_to_string(&mut s)?;
        Ok(s)
    }
}

