use once_cell::sync::Lazy;
use syntect::{
    easy::HighlightLines,
    highlighting::{Color as SynColor, Theme, ThemeSet},
    parsing::SyntaxSet,
};

#[derive(Clone, Copy)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Clone)]
pub struct HighlightSpan {
    pub text: String,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
}

#[derive(Clone)]
pub struct HighlightLine {
    pub spans: Vec<HighlightSpan>,
}

static SYNTAXES: Lazy<SyntaxSet> = Lazy::new(|| SyntaxSet::load_defaults_newlines());
static THEMES: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);
static THEME: Lazy<Theme> = Lazy::new(|| {
    THEMES
        .themes
        .get("InspiredGitHub")
        .cloned()
        .or_else(|| THEMES.themes.values().next().cloned())
        .unwrap_or_default()
});

pub fn highlight_code(lang: Option<&str>, text: &str) -> Vec<HighlightLine> {
    let syntax = lang
        .and_then(|l| SYNTAXES.find_syntax_by_token(l))
        .unwrap_or_else(|| SYNTAXES.find_syntax_plain_text());
    let mut h = HighlightLines::new(syntax, &THEME);
    let mut out = Vec::new();
    for line in text.lines() {
        let ranges = h
            .highlight_line(line, &SYNTAXES)
            .unwrap_or_else(|_| vec![(syntect::highlighting::Style::default(), line)]);
        let spans = ranges
            .into_iter()
            .map(|(style, content)| HighlightSpan {
                text: content.to_string(),
                fg: to_color(style.foreground),
                bg: to_color(style.background),
            })
            .collect();
        out.push(HighlightLine { spans });
    }
    if text.is_empty() {
        out.push(HighlightLine { spans: Vec::new() });
    }
    out
}

fn to_color(c: SynColor) -> Option<Color> {
    Some(Color {
        r: c.r,
        g: c.g,
        b: c.b,
    })
}
