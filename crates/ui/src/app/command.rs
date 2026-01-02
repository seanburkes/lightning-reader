use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::{Backend, Terminal};

use crate::reader_view::ReaderView;
use crate::search_view::SearchView;

use super::settings::save_settings;
use super::types::{Command, CommandOutcome, Mode, SearchCommand, SpritzSettings};
use super::App;

impl Command {
    pub(super) fn from_key(app: &App, key: KeyEvent) -> Option<Self> {
        if app.search.is_some() {
            return match key.code {
                KeyCode::Esc => Some(Command::Search(SearchCommand::Cancel)),
                KeyCode::Enter => Some(Command::Search(SearchCommand::Submit)),
                KeyCode::Backspace => Some(Command::Search(SearchCommand::Backspace)),
                KeyCode::Char(c) => Some(Command::Search(SearchCommand::Insert(c))),
                _ => None,
            };
        }
        if app.footnote.is_some() {
            return matches!(key.code, KeyCode::Esc).then_some(Command::CloseFootnote);
        }
        if app.show_help {
            return matches!(key.code, KeyCode::Esc | KeyCode::Char('?'))
                .then_some(Command::CloseHelp);
        }

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('q') => Some(Command::Exit),
            KeyCode::Char('c') if ctrl => Some(Command::Exit),
            KeyCode::Esc => Some(Command::Cancel),
            KeyCode::Enter => Some(Command::Submit),
            KeyCode::Char('/') => Some(Command::StartSearch),
            KeyCode::Char('t') => Some(Command::ToggleToc),
            KeyCode::Char('s') => Some(Command::ToggleSpritz),
            KeyCode::Char('?') => Some(Command::ToggleHelp),
            KeyCode::Char('j') | KeyCode::Down => {
                if ctrl && matches!(app.mode, Mode::Spritz) {
                    Some(Command::SpritzAdvance(10))
                } else if ctrl {
                    None
                } else {
                    Some(Command::NavigateDown(1))
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if ctrl && matches!(app.mode, Mode::Spritz) {
                    Some(Command::SpritzRewind(10))
                } else if ctrl {
                    None
                } else {
                    Some(Command::NavigateUp(1))
                }
            }
            KeyCode::Char('h') | KeyCode::Left => Some(Command::AdjustWidth(-2)),
            KeyCode::Char('l') | KeyCode::Right => Some(Command::AdjustWidth(2)),
            KeyCode::PageDown => Some(Command::PageDown),
            KeyCode::PageUp => Some(Command::PageUp),
            KeyCode::Char('J') => Some(Command::ToggleJustify),
            KeyCode::Char('b') => Some(Command::ToggleTwoPane),
            KeyCode::Char(' ') => Some(Command::SpritzTogglePlay),
            KeyCode::Char('r') => Some(Command::SpritzJumpToChapterStart),
            KeyCode::Char('f') => Some(Command::SpritzJumpToChapterEnd),
            KeyCode::Char('+') | KeyCode::Char('=') => Some(Command::SpritzAdjustWpm(10)),
            KeyCode::Char('-') | KeyCode::Char('_') => Some(Command::SpritzAdjustWpm(-10)),
            KeyCode::Char(']') => Some(Command::SpritzAdjustWpm(50)),
            KeyCode::Char('[') => Some(Command::SpritzAdjustWpm(-50)),
            _ => None,
        }
    }
}

impl App {
    pub(super) fn apply_command<B: Backend>(
        &mut self,
        command: Command,
        view: &mut ReaderView,
        width: &mut u16,
        height: u16,
        last_inner: &mut (u16, u16),
        terminal: &mut Terminal<B>,
    ) -> std::io::Result<CommandOutcome> {
        match command {
            Command::Exit => return Ok(CommandOutcome::Exit),
            Command::Search(search) => {
                self.apply_search_command(view, search);
            }
            Command::CloseFootnote => {
                self.footnote = None;
            }
            Command::CloseHelp => {
                self.show_help = false;
            }
            Command::ToggleHelp => {
                self.show_help = !self.show_help;
            }
            Command::Cancel => match self.mode {
                Mode::Toc => {
                    self.mode = Mode::Reader;
                    self.toc = None;
                }
                Mode::Spritz => {
                    self.stop_spritz();
                }
                Mode::Reader => {}
            },
            Command::Submit => match self.mode {
                Mode::Toc => {
                    self.submit_toc(view);
                }
                Mode::Spritz => {
                    if let Some(spritz) = &mut self.spritz {
                        if !spritz.is_playing {
                            spritz.play();
                        }
                    }
                }
                Mode::Reader => {}
            },
            Command::StartSearch => {
                let search = if let Some(prev) = &self.last_search {
                    SearchView::with_query(prev)
                } else {
                    SearchView::new()
                };
                self.search = Some(search);
            }
            Command::ToggleToc => {
                self.open_toc(view);
            }
            Command::ToggleSpritz => match self.mode {
                Mode::Reader => {
                    self.start_spritz();
                }
                Mode::Spritz => {
                    self.stop_spritz();
                }
                Mode::Toc => {}
            },
            Command::NavigateDown(lines) => match self.mode {
                Mode::Reader => {
                    view.down(lines);
                    view.last_key = Some("j/down".into());
                }
                Mode::Toc => {
                    if let Some(toc) = &mut self.toc {
                        toc.down();
                    }
                }
                Mode::Spritz => {
                    if let Some(spritz) = &mut self.spritz {
                        spritz.fast_forward(lines);
                    }
                }
            },
            Command::NavigateUp(lines) => match self.mode {
                Mode::Reader => {
                    view.up(lines);
                    view.last_key = Some("k/up".into());
                }
                Mode::Toc => {
                    if let Some(toc) = &mut self.toc {
                        toc.up();
                    }
                }
                Mode::Spritz => {
                    if let Some(spritz) = &mut self.spritz {
                        spritz.rewind(lines);
                    }
                }
            },
            Command::PageDown => {
                if let Mode::Reader = self.mode {
                    view.down((height / 2) as usize);
                    view.last_key = Some("PgDn".into());
                }
            }
            Command::PageUp => {
                if let Mode::Reader = self.mode {
                    view.up((height / 2) as usize);
                    view.last_key = Some("PgUp".into());
                }
            }
            Command::AdjustWidth(delta) => match self.mode {
                Mode::Reader => {
                    Self::apply_width_delta(width, delta);
                    self.reflow_view(view, terminal, *width, last_inner)?;
                    view.last_key = Some(if delta < 0 { "h/left" } else { "l/right" }.into());
                }
                Mode::Spritz => {
                    Self::apply_width_delta(width, delta);
                    *last_inner = (*width, last_inner.1);
                }
                Mode::Toc => {}
            },
            Command::ToggleJustify => {
                if let Mode::Reader = self.mode {
                    view.justify = !view.justify;
                    save_settings(view.justify, view.two_pane, &SpritzSettings::default());
                    view.last_key = Some("J toggle".into());
                    self.reflow_view(view, terminal, *width, last_inner)?;
                }
            }
            Command::ToggleTwoPane => {
                if let Mode::Reader = self.mode {
                    view.two_pane = !view.two_pane;
                    if view.two_pane {
                        view.current = view.current.saturating_sub(view.current % 2);
                    }
                    save_settings(view.justify, view.two_pane, &SpritzSettings::default());
                    self.reflow_view(view, terminal, *width, last_inner)?;
                    view.last_key = Some(
                        if view.two_pane {
                            "b spread on"
                        } else {
                            "b spread off"
                        }
                        .into(),
                    );
                }
            }
            Command::SpritzTogglePlay => {
                if let Mode::Spritz = self.mode {
                    if let Some(spritz) = &mut self.spritz {
                        spritz.toggle_play();
                    }
                }
            }
            Command::SpritzJumpToChapterStart => {
                if let Mode::Spritz = self.mode {
                    if let Some(spritz) = &mut self.spritz {
                        spritz.jump_to_chapter_start();
                    }
                }
            }
            Command::SpritzJumpToChapterEnd => {
                if let Mode::Spritz = self.mode {
                    if let Some(spritz) = &mut self.spritz {
                        spritz.jump_to_chapter_end();
                    }
                }
            }
            Command::SpritzAdjustWpm(delta) => {
                if let Mode::Spritz = self.mode {
                    if let Some(spritz) = &mut self.spritz {
                        spritz.adjust_wpm(delta);
                    }
                }
            }
            Command::SpritzAdvance(steps) => {
                if let Mode::Spritz = self.mode {
                    if let Some(spritz) = &mut self.spritz {
                        spritz.fast_forward(steps);
                    }
                }
            }
            Command::SpritzRewind(steps) => {
                if let Mode::Spritz = self.mode {
                    if let Some(spritz) = &mut self.spritz {
                        spritz.rewind(steps);
                    }
                }
            }
        }
        Ok(CommandOutcome::Continue)
    }

    fn apply_width_delta(width: &mut u16, delta: i16) {
        if delta < 0 {
            *width = width.saturating_sub((-delta) as u16);
        } else {
            *width = width.saturating_add(delta as u16);
        }
    }

    fn reflow_view<B: Backend>(
        &self,
        view: &mut ReaderView,
        terminal: &mut Terminal<B>,
        width: u16,
        last_inner: &mut (u16, u16),
    ) -> std::io::Result<()> {
        let size = terminal
            .size()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let inner = ReaderView::inner_size(size.into(), width, view.two_pane);
        view.reflow(&self.blocks, inner);
        *last_inner = (inner.width, inner.height);
        Ok(())
    }
}
