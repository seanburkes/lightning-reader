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
    text: String,
    kind: TitleKind,
}

impl TitleEntry {
    pub fn new(text: impl Into<String>, kind: TitleKind) -> Self {
        Self {
            text: text.into(),
            kind,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn kind(&self) -> &TitleKind {
        &self.kind
    }

    pub(crate) fn set_kind(&mut self, kind: TitleKind) {
        self.kind = kind;
    }
}

#[derive(Clone, Debug)]
pub struct CreatorEntry {
    name: String,
    roles: Vec<String>,
}

impl CreatorEntry {
    pub fn new(name: impl Into<String>, roles: Vec<String>) -> Self {
        Self {
            name: name.into(),
            roles,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn roles(&self) -> &[String] {
        &self.roles
    }
}

#[derive(Clone, Debug)]
pub struct SeriesInfo {
    name: String,
    index: Option<f32>,
}

impl SeriesInfo {
    pub fn new(name: impl Into<String>, index: Option<f32>) -> Self {
        Self {
            name: name.into(),
            index,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn index(&self) -> Option<f32> {
        self.index
    }
}

#[derive(Clone, Debug, Default)]
pub struct BookMetadata {
    titles: Vec<TitleEntry>,
    creators: Vec<CreatorEntry>,
    series: Option<SeriesInfo>,
}

impl BookMetadata {
    pub fn new(
        titles: Vec<TitleEntry>,
        creators: Vec<CreatorEntry>,
        series: Option<SeriesInfo>,
    ) -> Self {
        Self {
            titles,
            creators,
            series,
        }
    }

    pub fn titles(&self) -> &[TitleEntry] {
        &self.titles
    }

    pub fn creators(&self) -> &[CreatorEntry] {
        &self.creators
    }

    pub fn series(&self) -> Option<&SeriesInfo> {
        self.series.as_ref()
    }

    pub fn main_title(&self) -> Option<&str> {
        self.titles
            .iter()
            .find(|t| matches!(t.kind, TitleKind::Main))
            .or_else(|| self.titles.first())
            .map(|t| t.text.as_str())
    }

    pub fn subtitle(&self) -> Option<&str> {
        self.titles
            .iter()
            .find(|t| matches!(t.kind, TitleKind::Subtitle))
            .map(|t| t.text.as_str())
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
    id: String,
    path: String,
    title: Option<String>,
    subtitle: Option<String>,
    author: Option<String>,
    metadata: Option<BookMetadata>,
    format: DocumentFormat,
}

impl DocumentInfo {
    pub fn new(
        id: impl Into<String>,
        path: impl Into<String>,
        title: Option<String>,
        subtitle: Option<String>,
        author: Option<String>,
        metadata: Option<BookMetadata>,
        format: DocumentFormat,
    ) -> Self {
        Self {
            id: id.into(),
            path: path.into(),
            title,
            subtitle,
            author,
            metadata,
            format,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn subtitle(&self) -> Option<&str> {
        self.subtitle.as_deref()
    }

    pub fn author(&self) -> Option<&str> {
        self.author.as_deref()
    }

    pub fn metadata(&self) -> Option<&BookMetadata> {
        self.metadata.as_ref()
    }

    pub fn format(&self) -> DocumentFormat {
        self.format
    }

    pub fn from_book_id(
        book: &BookId,
        author: Option<String>,
        metadata: Option<BookMetadata>,
    ) -> Self {
        let subtitle = metadata
            .as_ref()
            .and_then(|m| m.subtitle())
            .map(|s| s.to_string());
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
    info: DocumentInfo,
    blocks: Vec<Block>,
    chapter_titles: Vec<String>,
    chapter_hrefs: Vec<String>,
    toc_entries: Vec<TocEntry>,
    outlines: Vec<crate::pdf::OutlineEntry>,
}

pub type DocumentParts = (
    DocumentInfo,
    Vec<Block>,
    Vec<String>,
    Vec<String>,
    Vec<TocEntry>,
    Vec<crate::pdf::OutlineEntry>,
);

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

    pub fn info(&self) -> &DocumentInfo {
        &self.info
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    pub fn chapter_titles(&self) -> &[String] {
        &self.chapter_titles
    }

    pub fn chapter_hrefs(&self) -> &[String] {
        &self.chapter_hrefs
    }

    pub fn toc_entries(&self) -> &[TocEntry] {
        &self.toc_entries
    }

    pub fn outlines(&self) -> &[crate::pdf::OutlineEntry] {
        &self.outlines
    }

    pub fn set_outlines(&mut self, outlines: Vec<crate::pdf::OutlineEntry>) {
        self.outlines = outlines;
    }

    pub fn into_parts(self) -> DocumentParts {
        (
            self.info,
            self.blocks,
            self.chapter_titles,
            self.chapter_hrefs,
            self.toc_entries,
            self.outlines,
        )
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
    Table(TableBlock),
}

#[derive(Clone)]
pub struct ImageBlock {
    id: String,
    data: Option<Vec<u8>>,
    alt: Option<String>,
    caption: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
}

impl ImageBlock {
    pub fn new(
        id: impl Into<String>,
        data: Option<Vec<u8>>,
        alt: Option<String>,
        caption: Option<String>,
        width: Option<u32>,
        height: Option<u32>,
    ) -> Self {
        Self {
            id: id.into(),
            data,
            alt,
            caption,
            width,
            height,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn data(&self) -> Option<&[u8]> {
        self.data.as_deref()
    }

    pub fn alt(&self) -> Option<&str> {
        self.alt.as_deref()
    }

    pub fn caption(&self) -> Option<&str> {
        self.caption.as_deref()
    }

    pub fn width(&self) -> Option<u32> {
        self.width
    }

    pub fn height(&self) -> Option<u32> {
        self.height
    }

    pub(crate) fn alt_mut(&mut self) -> Option<&mut String> {
        self.alt.as_mut()
    }

    pub(crate) fn caption_mut(&mut self) -> Option<&mut String> {
        self.caption.as_mut()
    }
}

#[derive(Clone)]
pub struct TableCell {
    text: String,
    is_header: bool,
}

impl TableCell {
    pub fn new(text: impl Into<String>, is_header: bool) -> Self {
        Self {
            text: text.into(),
            is_header,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_header(&self) -> bool {
        self.is_header
    }

    pub(crate) fn text_mut(&mut self) -> &mut String {
        &mut self.text
    }
}

#[derive(Clone)]
pub struct TableBlock {
    rows: Vec<Vec<TableCell>>,
}

impl TableBlock {
    pub fn new(rows: Vec<Vec<TableCell>>) -> Self {
        Self { rows }
    }

    pub fn rows(&self) -> &[Vec<TableCell>] {
        &self.rows
    }

    pub(crate) fn rows_mut(&mut self) -> &mut [Vec<TableCell>] {
        &mut self.rows
    }
}

#[derive(Clone)]
pub struct TocEntry {
    href: String,
    label: String,
    level: usize,
}

impl TocEntry {
    pub fn new(href: impl Into<String>, label: impl Into<String>, level: usize) -> Self {
        Self {
            href: href.into(),
            label: label.into(),
            level,
        }
    }

    pub fn href(&self) -> &str {
        &self.href
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn level(&self) -> usize {
        self.level
    }
}

#[derive(Clone, Copy)]
pub struct RgbColor {
    r: u8,
    g: u8,
    b: u8,
}

impl RgbColor {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn r(&self) -> u8 {
        self.r
    }

    pub fn g(&self) -> u8 {
        self.g
    }

    pub fn b(&self) -> u8 {
        self.b
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookId {
    id: String,   // sha256 or dc:identifier
    path: String, // absolute path
    title: Option<String>,
    #[serde(default = "default_format")]
    format: DocumentFormat,
}

impl BookId {
    pub fn new(
        id: impl Into<String>,
        path: impl Into<String>,
        title: Option<String>,
        format: DocumentFormat,
    ) -> Self {
        Self {
            id: id.into(),
            path: path.into(),
            title,
            format,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn format(&self) -> DocumentFormat {
        self.format
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    spine_index: usize,
    offset: usize,
}

impl Location {
    pub fn new(spine_index: usize, offset: usize) -> Self {
        Self {
            spine_index,
            offset,
        }
    }

    pub fn spine_index(&self) -> usize {
        self.spine_index
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn set_spine_index(&mut self, spine_index: usize) {
        self.spine_index = spine_index;
    }

    pub fn set_offset(&mut self, offset: usize) {
        self.offset = offset;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStateRecord {
    book: BookId,
    last_location: Location,
    bookmarks: Vec<Location>,
}

impl AppStateRecord {
    pub fn new(book: BookId, last_location: Location, bookmarks: Vec<Location>) -> Self {
        Self {
            book,
            last_location,
            bookmarks,
        }
    }

    pub fn book(&self) -> &BookId {
        &self.book
    }

    pub fn last_location(&self) -> &Location {
        &self.last_location
    }

    pub fn last_location_mut(&mut self) -> &mut Location {
        &mut self.last_location
    }

    pub fn bookmarks(&self) -> &[Location] {
        &self.bookmarks
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpritzSession {
    book_id: String,
    word_index: usize,
    wpm: u16,
    saved_at: String,
}

impl SpritzSession {
    pub fn new(
        book_id: impl Into<String>,
        word_index: usize,
        wpm: u16,
        saved_at: impl Into<String>,
    ) -> Self {
        Self {
            book_id: book_id.into(),
            word_index,
            wpm,
            saved_at: saved_at.into(),
        }
    }

    pub fn book_id(&self) -> &str {
        &self.book_id
    }

    pub fn word_index(&self) -> usize {
        self.word_index
    }

    pub fn wpm(&self) -> u16 {
        self.wpm
    }

    pub fn saved_at(&self) -> &str {
        &self.saved_at
    }
}
