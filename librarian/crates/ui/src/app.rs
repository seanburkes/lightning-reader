use crossterm::{event::{self, Event, KeyCode}, execute, terminal::{EnterAlternateScreen, LeaveAlternateScreen, enable_raw_mode, disable_raw_mode}};
use ratatui::{prelude::*, widgets::*};
use std::{io::stdout, time::Duration};

use crate::reader_view::ReaderView;
use reader_core::{layout::Size, types::Block};

pub struct App {
    blocks: Vec<Block>,
    initial_page: Option<usize>,
}

impl App {
    pub fn new() -> Self { Self { blocks: Vec::new(), initial_page: None } }
    pub fn new_with_blocks(blocks: Vec<Block>) -> Self { Self { blocks, initial_page: None } }
    pub fn new_with_blocks_at(blocks: Vec<Block>, initial_page: usize) -> Self {
        Self { blocks, initial_page: Some(initial_page) }
    }

    pub fn run(mut self) -> std::io::Result<usize> {
        let mut stdout = stdout();
        let raw_ok = enable_raw_mode().is_ok();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let mut view = ReaderView::new();
        // If initial page set via env or state, we could pass it
        let mut width: u16 = 60;
        let mut height: u16 = 20;
        let mut last_size: (u16, u16) = (0, 0);
        view.pages = reader_core::layout::paginate_with_justify(&self.blocks, Size { width, height }, view.justify);
        if let Some(idx) = self.initial_page {
            view.current = idx.min(view.pages.len().saturating_sub(1));
        }
        last_size = (width, height);

        if !raw_ok {
            // Non-interactive fallback: draw once and exit cleanly
            let _ = terminal.draw(|f| {
                let size = f.size();
                height = size.height.saturating_sub(2);
                view.reflow(&self.blocks, Size { width, height });
                view.render(f, size, width);
            });
            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
            return Ok(view.current);
        }

        let mut exit = false;
        while !exit {
            terminal.draw(|f| {
                let size = f.size();
                height = size.height.saturating_sub(2);
                // Respect configured column width; do not override with terminal width
                if (width, height) != last_size {
                    let inner = ReaderView::inner_size(size, width);
                    view.reflow(&self.blocks, inner);
                    // Clamp current page if needed
                    view.current = view.current.min(view.pages.len().saturating_sub(1));
                    last_size = (inner.width, inner.height);
                }
                view.render(f, size, width);
            })?;


            match event::poll(Duration::from_millis(100)) {
                Ok(true) => {
                    match event::read() {
                        Ok(Event::Key(key)) => {
                            match key.code {
                                KeyCode::Char('q') => exit = true,
                                KeyCode::Char('c') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => exit = true,
                                KeyCode::Char('j') | KeyCode::Down => { view.down(1); view.last_key = Some("j/down".into()); },
                                KeyCode::Char('k') | KeyCode::Up => { view.up(1); view.last_key = Some("k/up".into()); },
                                KeyCode::Char('h') | KeyCode::Left => { width = width.saturating_sub(2); let inner = ReaderView::inner_size(terminal.size()?, width); view.reflow(&self.blocks, inner); view.last_key = Some("h/left".into()); },
                                KeyCode::Char('l') | KeyCode::Right => { width = width.saturating_add(2); let inner = ReaderView::inner_size(terminal.size()?, width); view.reflow(&self.blocks, inner); view.last_key = Some("l/right".into()); },
                                KeyCode::PageDown => { view.down((height/2) as usize); view.last_key = Some("PgDn".into()); },
                                KeyCode::PageUp => { view.up((height/2) as usize); view.last_key = Some("PgUp".into()); },
                                KeyCode::Char('J') => { view.justify = !view.justify; view.last_key = Some("J toggle".into()); let inner = ReaderView::inner_size(terminal.size()?, width); view.reflow(&self.blocks, inner); },
                                _ => {}
                            }
                        }
                        Ok(_) => {}
                        Err(_) => { exit = true; }
                    }
                }
                Ok(false) => {}
                Err(_) => { exit = true; }
            }
        }

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        Ok(view.current)
    }
}
