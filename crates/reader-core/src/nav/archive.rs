use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use zip::ZipArchive;

use crate::epub::ReaderError;
use crate::types::TocEntry;

use super::epub2::{parse_epub2_ncx, parse_epub2_ncx_entries};
use super::epub3::{parse_epub3_nav, parse_epub3_nav_entries};
use super::paths::strip_fragment;

fn read_file_to_string(zip: &mut ZipArchive<File>, path: &Path) -> Result<String, ReaderError> {
    let mut f = zip.by_name(path.to_string_lossy().as_ref())?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    Ok(s)
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

pub(crate) fn read_nav_labels_from_archive_inner(
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

pub(crate) fn read_nav_entries_from_archive_inner(
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
