use ratatui::prelude::Color;

// Tokyonight-inspired palette; tweak these to change header/footer colors.
const TN_BG: Color = Color::Rgb(26, 27, 38); // #1a1b26
const TN_BG_ALT: Color = Color::Rgb(31, 35, 53); // #1f2335
const TN_BG_STRONG: Color = Color::Rgb(65, 72, 104); // #414868
const TN_FG: Color = Color::Rgb(192, 202, 245); // #c0caf5
const TN_BLUE: Color = Color::Rgb(122, 162, 247); // #7aa2f7

#[derive(Clone)]
pub struct Theme {
    pub header_bg: Color,
    pub header_fg: Color,
    pub header_pad_bg: Color,
    pub footer_bg: Color,
    pub footer_fg: Color,
    pub footer_pad_bg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            header_bg: TN_BG_ALT,
            header_fg: TN_FG,
            header_pad_bg: TN_BG,
            footer_bg: TN_BG_STRONG,
            footer_fg: TN_BLUE,
            footer_pad_bg: TN_BG_ALT,
        }
    }
}
