mod areas;
mod images;
mod navigation;
mod render;
mod search;
mod selection;
mod text;
mod theme;
mod view;

#[cfg(test)]
mod tests;

const SPREAD_GAP: u16 = 4;

pub use areas::ContentAreas;
pub use selection::{SelectionPoint, SelectionRange};
pub use theme::Theme;
pub use view::ReaderView;
