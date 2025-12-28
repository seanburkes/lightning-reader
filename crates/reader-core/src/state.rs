use crate::{
    config,
    types::{AppStateRecord, BookId, SpritzSession},
};
use serde_json;
use std::{fs, io::Write, path::PathBuf};

pub fn config_dir() -> Option<PathBuf> {
    config::config_root()
}

fn state_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(primary) = config::config_root() {
        paths.push(primary.join("state.json"));
    }
    for legacy in config::legacy_config_roots() {
        paths.push(legacy.join("state.json"));
    }
    paths
}

pub fn load_state(book: &BookId) -> Option<AppStateRecord> {
    for path in state_paths() {
        let data = fs::read(&path).ok()?;
        let records: Vec<AppStateRecord> = serde_json::from_slice(&data).ok()?;
        if let Some(rec) = records
            .into_iter()
            .find(|r| r.book.id == book.id || r.book.path == book.path)
        {
            return Some(rec);
        }
    }
    None
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

pub fn load_spritz_session(book_id: &str) -> Option<SpritzSession> {
    let mut paths = Vec::new();
    if let Some(primary) = config::config_root() {
        paths.push(primary.join("spritz_sessions.json"));
    }
    for legacy in config::legacy_config_roots() {
        paths.push(legacy.join("spritz_sessions.json"));
    }

    for path in paths {
        if let Ok(data) = fs::read(&path) {
            if let Ok(sessions) = serde_json::from_slice::<Vec<SpritzSession>>(&data) {
                if let Some(session) = sessions.into_iter().find(|s| s.book_id == book_id) {
                    return Some(session);
                }
            }
        }
    }
    None
}

pub fn save_spritz_session(session: &SpritzSession) -> std::io::Result<()> {
    let dir = config_dir()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no config dir"))?;
    fs::create_dir_all(&dir)?;
    let path = dir.join("spritz_sessions.json");
    let mut sessions: Vec<SpritzSession> = fs::read(&path)
        .ok()
        .and_then(|d| serde_json::from_slice(&d).ok())
        .unwrap_or_default();

    if let Some(existing) = sessions.iter_mut().find(|s| s.book_id == session.book_id) {
        *existing = session.clone();
    } else {
        sessions.push(session.clone());
    }

    let mut f = fs::File::create(path)?;
    let s = serde_json::to_string_pretty(&sessions).unwrap_or_else(|_| "[]".into());
    f.write_all(s.as_bytes())
}
