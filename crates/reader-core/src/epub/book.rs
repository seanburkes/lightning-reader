use std::{
    cell::RefCell,
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use zip::ZipArchive;

use crate::nav;
use crate::types::{BookMetadata, TocEntry};

use super::container::read_container;
use super::error::ReaderError;
use super::opf::read_opf;

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

impl EpubBook {
    pub fn open(path: &Path) -> Result<Self, ReaderError> {
        let file = std::fs::File::open(path)?;
        let mut zip = ZipArchive::new(file)?;
        let rootfile = read_container(&mut zip)?;
        let (metadata, manifest, spine_ids, spine_toc) = read_opf(&mut zip, &rootfile)?;
        let title = metadata.main_title().map(|s| s.to_string());
        let subtitle = metadata.subtitle().map(|s| s.to_string());
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
        nav::read_nav_labels_from_archive_inner(
            &mut self.zip.borrow_mut(),
            &self.rootfile,
            self.nav_href.as_deref(),
            self.ncx_href.as_deref(),
        )
    }

    pub fn toc_entries(&self) -> Result<Vec<TocEntry>, ReaderError> {
        nav::read_nav_entries_from_archive_inner(
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
