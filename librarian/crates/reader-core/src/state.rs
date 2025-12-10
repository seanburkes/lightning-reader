use crate::types::{AppStateRecord, BookId};
use directories::ProjectDirs;
use serde_json;
use std::{fs, io::Write, path::PathBuf};

pub fn config_dir() -> Option<PathBuf> {
    ProjectDirs::from("org", "sean", "librarian").map(|p| p.config_dir().to_path_buf())
}

pub fn load_state(book: &BookId) -> Option<AppStateRecord> {
    let dir = config_dir()?;
    let path = dir.join("state.json");
    let data = fs::read(path).ok()?;
    let records: Vec<AppStateRecord> = serde_json::from_slice(&data).ok()?;
    records
        .into_iter()
        .find(|r| r.book.id == book.id || r.book.path == book.path)
}

pub fn save_state(record: &AppStateRecord) -> std::io::Result<()> {
    let dir = config_dir()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no config dir"))?;
    fs::create_dir_all(&dir)?;
    let path = dir.join("state.json");
    let mut records: Vec<AppStateRecord> = fs::read(&path)
        .ok()
        .and_then(|d| serde_json::from_slice(&d).ok())
        .unwrap_or_default();
    if let Some(existing) = records
        .iter_mut()
        .find(|r| r.book.id == record.book.id || r.book.path == record.book.path)
    {
        *existing = record.clone();
    } else {
        records.push(record.clone());
    }
    let mut f = fs::File::create(path)?;
    let s = serde_json::to_string_pretty(&records).unwrap_or_else(|_| "[]".into());
    f.write_all(s.as_bytes())
}
