use quick_xml::events::{BytesStart, Event};
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

use crate::types::{BookMetadata, CreatorEntry, SeriesInfo, TitleEntry, TitleKind};

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
    pub subtitle: Option<String>,
    pub author: Option<String>,
    pub metadata: BookMetadata,
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

type OpfResult = (BookMetadata, Vec<ManifestItem>, Vec<String>, Option<String>);

struct TitleCandidate {
    text: String,
    kind: TitleKind,
}

struct CreatorCandidate {
    name: String,
    roles: Vec<String>,
    is_creator: bool,
}

struct CollectionCandidate {
    name: String,
    collection_type: Option<String>,
    position: Option<f32>,
}

struct OpfParseState {
    manifest: Vec<ManifestItem>,
    spine_ids: Vec<String>,
    spine_toc: Option<String>,
    in_metadata: bool,
    titles: Vec<TitleCandidate>,
    title_index: HashMap<String, usize>,
    creators: Vec<CreatorCandidate>,
    creator_index: HashMap<String, usize>,
    collections: Vec<CollectionCandidate>,
    collection_index: HashMap<String, usize>,
    meta_subtitle: Option<String>,
    calibre_series: Option<String>,
    calibre_series_index: Option<f32>,
}

impl OpfParseState {
    fn new() -> Self {
        Self {
            manifest: Vec::new(),
            spine_ids: Vec::new(),
            spine_toc: None,
            in_metadata: false,
            titles: Vec::new(),
            title_index: HashMap::new(),
            creators: Vec::new(),
            creator_index: HashMap::new(),
            collections: Vec::new(),
            collection_index: HashMap::new(),
            meta_subtitle: None,
            calibre_series: None,
            calibre_series_index: None,
        }
    }

    fn add_title(&mut self, text: String, id: Option<String>) {
        let idx = self.titles.len();
        self.titles.push(TitleCandidate {
            text,
            kind: TitleKind::Unspecified,
        });
        if let Some(id) = id {
            self.title_index.insert(id, idx);
        }
    }

    fn add_creator(
        &mut self,
        name: String,
        id: Option<String>,
        roles: Vec<String>,
        is_creator: bool,
    ) {
        let idx = self.creators.len();
        self.creators.push(CreatorCandidate {
            name,
            roles,
            is_creator,
        });
        if let Some(id) = id {
            self.creator_index.insert(id, idx);
        }
    }

    fn update_title_kind(&mut self, id: &str, kind: TitleKind) {
        if let Some(idx) = self.title_index.get(id).copied() {
            self.titles[idx].kind = kind;
        }
    }

    fn add_creator_role(&mut self, id: &str, role: String) {
        if let Some(idx) = self.creator_index.get(id).copied() {
            push_role(&mut self.creators[idx].roles, role);
        }
    }

    fn set_collection_name(&mut self, id: Option<&str>, name: String) {
        if let Some(id) = id {
            let idx = self.ensure_collection(id);
            if self.collections[idx].name.is_empty() {
                self.collections[idx].name = name;
            }
        } else {
            self.collections.push(CollectionCandidate {
                name,
                collection_type: None,
                position: None,
            });
        }
    }

    fn set_collection_type(&mut self, id: &str, collection_type: String) {
        let idx = self.ensure_collection(id);
        self.collections[idx].collection_type = Some(collection_type);
    }

    fn set_collection_position(&mut self, id: &str, position: f32) {
        let idx = self.ensure_collection(id);
        self.collections[idx].position = Some(position);
    }

    fn ensure_collection(&mut self, id: &str) -> usize {
        if let Some(idx) = self.collection_index.get(id).copied() {
            return idx;
        }
        let idx = self.collections.len();
        self.collections.push(CollectionCandidate {
            name: String::new(),
            collection_type: None,
            position: None,
        });
        self.collection_index.insert(id.to_string(), idx);
        idx
    }
}

fn read_opf(zip: &mut ZipArchive<File>, opf_path: &Path) -> Result<OpfResult, ReaderError> {
    let mut opf = zip.by_name(opf_path.to_string_lossy().as_ref())?;
    let mut opf_xml = String::new();
    opf.read_to_string(&mut opf_xml)?;
    let mut reader = XmlReader::from_str(&opf_xml);
    let mut state = OpfParseState::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                handle_opf_event(&mut reader, &e, false, &mut state)?;
            }
            Ok(Event::Empty(e)) => {
                handle_opf_event(&mut reader, &e, true, &mut state)?;
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if local_name(&name) == "metadata" {
                    state.in_metadata = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(ReaderError::Parse(e.to_string())),
            _ => {}
        }
    }
    let mut title_entries: Vec<TitleEntry> = state
        .titles
        .into_iter()
        .map(|t| TitleEntry {
            text: t.text,
            kind: t.kind,
        })
        .collect();
    if !title_entries.is_empty()
        && !title_entries
            .iter()
            .any(|t| matches!(t.kind, TitleKind::Main))
    {
        title_entries[0].kind = TitleKind::Main;
    }
    if let Some(subtitle) = state.meta_subtitle {
        if !subtitle.is_empty()
            && !title_entries.iter().any(|t| {
                matches!(t.kind, TitleKind::Subtitle) && t.text.eq_ignore_ascii_case(&subtitle)
            })
        {
            title_entries.push(TitleEntry {
                text: subtitle,
                kind: TitleKind::Subtitle,
            });
        }
    }

    let mut creator_entries: Vec<CreatorEntry> = Vec::new();
    for mut creator in state.creators {
        if creator.name.is_empty() {
            continue;
        }
        if creator.roles.is_empty() && creator.is_creator {
            creator.roles.push("aut".to_string());
        }
        dedupe_roles(&mut creator.roles);
        creator_entries.push(CreatorEntry {
            name: creator.name,
            roles: creator.roles,
        });
    }

    let series = resolve_series_info(
        state.calibre_series,
        state.calibre_series_index,
        &state.collections,
    );
    let metadata = BookMetadata {
        titles: title_entries,
        creators: creator_entries,
        series,
    };
    Ok((metadata, state.manifest, state.spine_ids, state.spine_toc))
}

fn handle_opf_event(
    reader: &mut XmlReader<&[u8]>,
    e: &BytesStart<'_>,
    is_empty: bool,
    state: &mut OpfParseState,
) -> Result<(), ReaderError> {
    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
    let local = local_name(&name);

    if local == "metadata" {
        state.in_metadata = true;
        return Ok(());
    }

    if local == "item" {
        let mut id: Option<String> = None;
        let mut href: Option<String> = None;
        let mut media_type: Option<String> = None;
        let mut properties: Option<String> = None;
        for a in e.attributes().flatten() {
            let key = String::from_utf8_lossy(a.key.as_ref());
            let attr = local_name(&key);
            let val = a
                .unescape_value()
                .map_err(|e| ReaderError::Parse(e.to_string()))?;
            let sval = val.into_owned();
            match attr {
                "id" => id = Some(sval),
                "href" => href = Some(sval),
                "media-type" => media_type = Some(sval),
                "properties" => properties = Some(sval),
                _ => {}
            }
        }
        if let (Some(id), Some(href)) = (id, href) {
            state.manifest.push(ManifestItem {
                id,
                href,
                media_type,
                properties,
            });
        }
        return Ok(());
    }

    if local == "itemref" {
        for a in e.attributes().flatten() {
            let key = String::from_utf8_lossy(a.key.as_ref());
            let attr = local_name(&key);
            if attr != "idref" {
                continue;
            }
            let val = a
                .unescape_value()
                .map_err(|e| ReaderError::Parse(e.to_string()))?;
            let sval = val.into_owned();
            if !sval.is_empty() {
                state.spine_ids.push(sval);
            }
        }
        return Ok(());
    }

    if local == "spine" && state.spine_toc.is_none() {
        for a in e.attributes().flatten() {
            let key = String::from_utf8_lossy(a.key.as_ref());
            let attr = local_name(&key);
            if attr != "toc" {
                continue;
            }
            let val = a
                .unescape_value()
                .map_err(|e| ReaderError::Parse(e.to_string()))?;
            state.spine_toc = Some(val.into_owned());
        }
        return Ok(());
    }

    if !state.in_metadata {
        return Ok(());
    }

    if local == "title" {
        let mut id: Option<String> = None;
        for a in e.attributes().flatten() {
            let key = String::from_utf8_lossy(a.key.as_ref());
            let attr = local_name(&key);
            if attr != "id" {
                continue;
            }
            let val = a
                .unescape_value()
                .map_err(|e| ReaderError::Parse(e.to_string()))?;
            id = Some(val.into_owned());
        }
        if !is_empty {
            if let Some(text) = read_text_value(reader) {
                let text = normalize_meta_text(&text);
                if !text.is_empty() {
                    state.add_title(text, id);
                }
            }
        }
        return Ok(());
    }

    if local == "creator" || local == "author" || local == "contributor" {
        let mut id: Option<String> = None;
        let mut roles: Vec<String> = Vec::new();
        for a in e.attributes().flatten() {
            let key = String::from_utf8_lossy(a.key.as_ref());
            let attr = local_name(&key);
            let val = a
                .unescape_value()
                .map_err(|e| ReaderError::Parse(e.to_string()))?;
            let sval = val.into_owned();
            match attr {
                "id" => id = Some(sval),
                "role" => {
                    if let Some(role) = parse_role(&sval) {
                        push_role(&mut roles, role);
                    }
                }
                _ => {}
            }
        }
        if !is_empty {
            if let Some(text) = read_text_value(reader) {
                let text = normalize_meta_text(&text);
                if !text.is_empty() {
                    let is_creator = local == "creator" || local == "author";
                    state.add_creator(text, id, roles, is_creator);
                }
            }
        }
        return Ok(());
    }

    if local == "meta" {
        let mut meta_name: Option<String> = None;
        let mut meta_property: Option<String> = None;
        let mut meta_content: Option<String> = None;
        let mut meta_refines: Option<String> = None;
        let mut meta_id: Option<String> = None;
        for a in e.attributes().flatten() {
            let key = String::from_utf8_lossy(a.key.as_ref());
            let attr = local_name(&key);
            let val = a
                .unescape_value()
                .map_err(|e| ReaderError::Parse(e.to_string()))?;
            let sval = val.into_owned();
            match attr {
                "name" => meta_name = Some(sval),
                "property" => meta_property = Some(sval),
                "content" => meta_content = Some(sval),
                "refines" => meta_refines = Some(sval),
                "id" => meta_id = Some(sval),
                _ => {}
            }
        }
        if meta_content.is_none() && !is_empty {
            if let Some(text) = read_text_value(reader) {
                meta_content = Some(text);
            }
        }
        let content = meta_content
            .map(|c| normalize_meta_text(&c))
            .filter(|c| !c.is_empty());
        let Some(content) = content else {
            return Ok(());
        };

        let name_lower = meta_name.as_deref().map(|v| v.to_ascii_lowercase());
        let property_key = meta_property
            .as_deref()
            .map(|v| local_name(v).to_ascii_lowercase());

        if let Some(name) = name_lower.as_deref() {
            if name == "calibre:series" {
                state.calibre_series = Some(content.clone());
            } else if name == "calibre:series_index" {
                state.calibre_series_index = parse_series_index(&content);
            }
        }

        if state.meta_subtitle.is_none()
            && (name_lower
                .as_deref()
                .map(|v| v.contains("subtitle"))
                .unwrap_or(false)
                || property_key
                    .as_deref()
                    .map(|v| v.contains("subtitle"))
                    .unwrap_or(false))
        {
            state.meta_subtitle = Some(content.clone());
        }

        if property_key.as_deref() == Some("title-type") {
            if let Some(refines) = meta_refines.as_deref() {
                let id = strip_refines(refines);
                state.update_title_kind(id, parse_title_kind(&content));
            }
        } else if property_key.as_deref() == Some("role") {
            if let Some(refines) = meta_refines.as_deref() {
                if let Some(role) = parse_role(&content) {
                    let id = strip_refines(refines);
                    state.add_creator_role(id, role);
                }
            }
        } else if property_key.as_deref() == Some("belongs-to-collection")
            || name_lower.as_deref() == Some("belongs-to-collection")
        {
            state.set_collection_name(meta_id.as_deref(), content);
        } else if property_key.as_deref() == Some("collection-type") {
            if let Some(refines) = meta_refines.as_deref() {
                let id = strip_refines(refines);
                state.set_collection_type(id, content);
            }
        } else if property_key.as_deref() == Some("group-position") {
            if let Some(refines) = meta_refines.as_deref() {
                if let Some(position) = parse_series_index(&content) {
                    let id = strip_refines(refines);
                    state.set_collection_position(id, position);
                }
            }
        }
    }

    Ok(())
}

impl EpubBook {
    pub fn open(path: &Path) -> Result<Self, ReaderError> {
        let file = std::fs::File::open(path)?;
        let mut zip = ZipArchive::new(file)?;
        let rootfile = read_container(&mut zip)?;
        let (metadata, manifest, spine_ids, spine_toc) = read_opf(&mut zip, &rootfile)?;
        let title = metadata.main_title();
        let subtitle = metadata.subtitle();
        let author = metadata.author_string();
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
            subtitle,
            author,
            metadata,
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

    pub fn opf_base(&self) -> PathBuf {
        self.rootfile
            .parent()
            .unwrap_or(Path::new(""))
            .to_path_buf()
    }

    pub fn toc_labels(&self) -> Result<std::collections::HashMap<String, String>, ReaderError> {
        // Read directly from the shared archive to avoid reopening
        crate::nav::read_nav_labels_from_archive_inner(
            &mut self.zip.borrow_mut(),
            &self.rootfile,
            self.nav_href.as_deref(),
            self.ncx_href.as_deref(),
        )
    }

    pub fn toc_entries(&self) -> Result<Vec<crate::types::TocEntry>, ReaderError> {
        crate::nav::read_nav_entries_from_archive_inner(
            &mut self.zip.borrow_mut(),
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

    pub fn load_resource(&self, path: &Path) -> Result<Vec<u8>, ReaderError> {
        let mut zip = self.zip.borrow_mut();
        let mut file = zip.by_name(path.to_string_lossy().as_ref())?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        Ok(buf)
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

fn strip_refines(value: &str) -> &str {
    value.trim().trim_start_matches('#')
}

fn parse_title_kind(value: &str) -> TitleKind {
    let lower = value.trim().to_ascii_lowercase();
    match lower.as_str() {
        "main" => TitleKind::Main,
        "subtitle" => TitleKind::Subtitle,
        "short" => TitleKind::Short,
        "expanded" => TitleKind::Expanded,
        "" => TitleKind::Unspecified,
        _ => TitleKind::Other(lower),
    }
}

fn parse_role(value: &str) -> Option<String> {
    let normalized = normalize_meta_text(value);
    if normalized.is_empty() {
        return None;
    }
    let trimmed = normalized.trim();
    let role = trimmed.rsplit(':').next().unwrap_or(trimmed);
    Some(role.to_ascii_lowercase())
}

fn parse_series_index(value: &str) -> Option<f32> {
    let normalized = normalize_meta_text(value);
    if normalized.is_empty() {
        return None;
    }
    if let Ok(idx) = normalized.parse::<f32>() {
        return Some(idx);
    }
    let fallback = normalized.replace(',', ".");
    fallback.parse::<f32>().ok()
}

fn resolve_series_info(
    calibre_series: Option<String>,
    calibre_series_index: Option<f32>,
    collections: &[CollectionCandidate],
) -> Option<SeriesInfo> {
    if let Some(name) = calibre_series {
        if !name.is_empty() {
            return Some(SeriesInfo {
                name,
                index: calibre_series_index,
            });
        }
    }

    let mut series_candidates = collections.iter().filter(|c| {
        c.collection_type
            .as_deref()
            .map(|t| t.to_ascii_lowercase().contains("series"))
            .unwrap_or(false)
    });
    let candidate = series_candidates.next().or_else(|| {
        if collections.len() == 1 {
            collections.first()
        } else {
            None
        }
    })?;

    if candidate.name.is_empty() {
        return None;
    }
    Some(SeriesInfo {
        name: candidate.name.clone(),
        index: candidate.position,
    })
}

fn push_role(roles: &mut Vec<String>, role: String) {
    if roles.iter().any(|r| r.eq_ignore_ascii_case(&role)) {
        return;
    }
    roles.push(role);
}

fn dedupe_roles(roles: &mut Vec<String>) {
    let mut unique: Vec<String> = Vec::new();
    for role in roles.drain(..) {
        if unique.iter().any(|r| r.eq_ignore_ascii_case(&role)) {
            continue;
        }
        unique.push(role);
    }
    *roles = unique;
}
