use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DocumentFormat {
    Epub,
    Pdf,
}

fn default_format() -> DocumentFormat {
    DocumentFormat::Epub
}

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
    #[serde(default = "default_format")]
    pub format: DocumentFormat,
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
