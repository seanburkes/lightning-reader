use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DocumentFormat {
    #[serde(alias = "epub")]
    Epub3,
    Epub2,
    Text,
    Markdown,
    Pdf,
    #[serde(other)]
    Other,
}

fn default_format() -> DocumentFormat {
    DocumentFormat::Text
}

#[derive(Clone)]
pub struct DocumentInfo {
    pub id: String,
    pub path: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub format: DocumentFormat,
}

impl DocumentInfo {
    pub fn from_book_id(book: &BookId, author: Option<String>) -> Self {
        Self {
            id: book.id.clone(),
            path: book.path.clone(),
            title: book.title.clone(),
            author,
            format: book.format,
        }
    }
}

#[derive(Clone)]
pub struct Document {
    pub info: DocumentInfo,
    pub blocks: Vec<Block>,
    pub chapter_titles: Vec<String>,
    pub chapter_hrefs: Vec<String>,
    pub toc_entries: Vec<TocEntry>,
    pub outlines: Vec<crate::pdf::OutlineEntry>,
}

impl Document {
    pub fn new(
        info: DocumentInfo,
        blocks: Vec<Block>,
        chapter_titles: Vec<String>,
        chapter_hrefs: Vec<String>,
        toc_entries: Vec<TocEntry>,
    ) -> Self {
        Self {
            info,
            blocks,
            chapter_titles,
            chapter_hrefs,
            toc_entries,
            outlines: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub enum Block {
    Paragraph(String),
    Heading(String, u8),
    List(Vec<String>),
    Code { lang: Option<String>, text: String },
    Quote(String),
    Image(ImageBlock),
}

#[derive(Clone)]
pub struct ImageBlock {
    pub id: String,
    pub data: Option<Vec<u8>>,
    pub alt: Option<String>,
    pub caption: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Clone)]
pub struct TocEntry {
    pub href: String,
    pub label: String,
    pub level: usize,
}

#[derive(Clone, Copy)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpritzSession {
    pub book_id: String,
    pub word_index: usize,
    pub wpm: u16,
    pub saved_at: String,
}
