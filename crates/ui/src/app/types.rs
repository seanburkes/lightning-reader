use reader_core::types::Block as ReaderBlock;

#[derive(Clone, Copy, Debug)]
pub struct SpritzSettings {
    pub wpm: u16,
    pub pause_on_punct: bool,
    pub punct_pause_ms: u16,
}

impl Default for SpritzSettings {
    fn default() -> Self {
        Self {
            wpm: 250,
            pause_on_punct: true,
            punct_pause_ms: 100,
        }
    }
}

pub enum Mode {
    Reader,
    Toc,
    Spritz,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum SearchCommand {
    Cancel,
    Submit,
    Backspace,
    Insert(char),
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Command {
    Exit,
    Cancel,
    Submit,
    StartSearch,
    ToggleToc,
    ToggleSpritz,
    ToggleHelp,
    CloseHelp,
    CloseFootnote,
    AdjustWidth(i16),
    NavigateDown(usize),
    NavigateUp(usize),
    PageDown,
    PageUp,
    ToggleJustify,
    ToggleTwoPane,
    SpritzTogglePlay,
    SpritzJumpToChapterStart,
    SpritzJumpToChapterEnd,
    SpritzAdjustWpm(i16),
    SpritzAdvance(usize),
    SpritzRewind(usize),
    Search(SearchCommand),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CommandOutcome {
    Continue,
    Exit,
}

pub struct IncomingPage {
    pub page_index: usize,
    pub blocks: Vec<ReaderBlock>,
}

pub struct PrefetchRequest {
    pub start: usize,
    pub window: usize,
}

pub struct IncomingChapter {
    pub chapter_index: usize,
    pub blocks: Vec<ReaderBlock>,
    pub title: String,
    pub href: String,
}

pub struct ChapterPrefetchRequest {
    pub target_loaded: usize,
    pub target_href: Option<String>,
}
