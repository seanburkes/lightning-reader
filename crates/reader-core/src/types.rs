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

#[derive(Clone, Debug)]
pub enum TitleKind {
    Main,
    Subtitle,
    Short,
    Expanded,
    Unspecified,
    Other(String),
}

#[derive(Clone, Debug)]
pub struct TitleEntry {
    pub text: String,
    pub kind: TitleKind,
}

#[derive(Clone, Debug)]
pub struct CreatorEntry {
    pub name: String,
    pub roles: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct SeriesInfo {
    pub name: String,
    pub index: Option<f32>,
}

#[derive(Clone, Debug, Default)]
pub struct BookMetadata {
    pub titles: Vec<TitleEntry>,
    pub creators: Vec<CreatorEntry>,
    pub series: Option<SeriesInfo>,
}

impl BookMetadata {
    pub fn main_title(&self) -> Option<String> {
        self.titles
            .iter()
            .find(|t| matches!(t.kind, TitleKind::Main))
            .or_else(|| self.titles.first())
            .map(|t| t.text.clone())
    }

    pub fn subtitle(&self) -> Option<String> {
        self.titles
            .iter()
            .find(|t| matches!(t.kind, TitleKind::Subtitle))
            .map(|t| t.text.clone())
    }

    pub fn author_string(&self) -> Option<String> {
        let mut authors: Vec<String> = self
            .creators
            .iter()
            .filter(|c| {
                if c.roles.is_empty() {
                    return true;
                }
                c.roles.iter().any(|role| is_author_role(role))
            })
            .map(|c| c.name.clone())
            .collect();
        if authors.is_empty() {
            authors = self.creators.iter().map(|c| c.name.clone()).collect();
        }
        if authors.is_empty() {
            None
        } else {
            Some(authors.join(", "))
        }
    }
}

fn is_author_role(role: &str) -> bool {
    let lower = role.to_ascii_lowercase();
    lower == "aut" || lower.contains("author")
}

#[derive(Clone)]
pub struct DocumentInfo {
    pub id: String,
    pub path: String,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub author: Option<String>,
    pub metadata: Option<BookMetadata>,
    pub format: DocumentFormat,
}

impl DocumentInfo {
    pub fn from_book_id(
        book: &BookId,
        author: Option<String>,
        metadata: Option<BookMetadata>,
    ) -> Self {
        let subtitle = metadata.as_ref().and_then(|m| m.subtitle());
        Self {
            id: book.id.clone(),
            path: book.path.clone(),
            title: book.title.clone(),
            subtitle,
            author,
            metadata,
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
