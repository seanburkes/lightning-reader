use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub enum Block {
    Paragraph(String),
    Heading(String, u8),
    List(Vec<String>),
    Code { lang: Option<String>, text: String },
    Quote(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookId {
    pub id: String,   // sha256 or dc:identifier
    pub path: String, // absolute path
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub spine_index: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStateRecord {
    pub book: BookId,
    pub last_location: Location,
    pub bookmarks: Vec<Location>,
}
