use crate::reader_view::ReaderView;

use super::types::SearchCommand;
use super::App;

impl App {
    pub(super) fn apply_search_command(&mut self, view: &mut ReaderView, command: SearchCommand) {
        match command {
            SearchCommand::Cancel => {
                self.search = None;
            }
            SearchCommand::Backspace => {
                if let Some(search) = &mut self.search {
                    search.backspace();
                }
            }
            SearchCommand::Insert(c) => {
                if let Some(search) = &mut self.search {
                    search.push_char(c);
                }
            }
            SearchCommand::Submit => {
                let (trimmed, start_from) = match &self.search {
                    Some(search) => {
                        let trimmed = search.query.trim().to_string();
                        let start_from = if self.last_search.as_deref().map(str::trim)
                            == Some(trimmed.as_str())
                        {
                            self.last_search_hit.map(|p| p + 1)
                        } else {
                            None
                        };
                        (trimmed, start_from)
                    }
                    None => return,
                };
                self.last_search = Some(trimmed.clone());
                if let Some(idx) = view.search_forward(&trimmed, start_from) {
                    let target = if view.two_pane {
                        idx.saturating_sub(idx % 2)
                    } else {
                        idx
                    };
                    view.current = target;
                    self.last_search_hit = Some(idx);
                } else {
                    self.last_search_hit = None;
                }
                self.search = None;
            }
        }
    }
}
