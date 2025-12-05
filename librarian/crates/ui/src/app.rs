use crossterm::{event::{self, Event, KeyCode}, execute, terminal::{EnterAlternateScreen, LeaveAlternateScreen, enable_raw_mode, disable_raw_mode}};
use ratatui::{prelude::*, widgets::*};
use std::{io::stdout, time::Duration};

use crate::reader_view::ReaderView;
use reader_core::{layout::Size, types::Block};

pub struct App {
    blocks: Vec<Block>,
}

impl App {
    pub fn new() -> Self { Self { blocks: Vec::new() } }
    pub fn new_with_blocks(blocks: Vec<Block>) -> Self { Self { blocks } }

    pub fn run(self) -> std::io::Result<()> {
        let mut stdout = stdout();
        let raw_ok = enable_raw_mode().is_ok();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let mut view = ReaderView::new();
        let mut width: u16 = 60;
        let mut height: u16 = 20;
        view.pages = reader_core::layout::paginate(&self.blocks, Size { width, height });

        if !raw_ok {
            // Non-interactive fallback: draw once and exit cleanly
            let _ = terminal.draw(|f| {
                let size = f.size();
                height = size.height.saturating_sub(2);
                view.reflow(&self.blocks, Size { width, height });
                view.render(f, size);
            });
            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
            return Ok(());
        }

        let mut exit = false;
        while !exit {
            terminal.draw(|f| {
                let size = f.size();
                height = size.height.saturating_sub(2);
                view.reflow(&self.blocks, Size { width, height });
                view.render(f, size);
            })?;


            match event::poll(Duration::from_millis(100)) {
                Ok(true) => {
                    match event::read() {
                        Ok(Event::Key(key)) => {
                            match key.code {
                                KeyCode::Char('q') => exit = true,
                                KeyCode::Char('c') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => exit = true,
                                KeyCode::Char('j') => view.down(1),
                                KeyCode::Char('k') => view.up(1),
                                KeyCode::Char('h') => { width = width.saturating_sub(2); view.reflow(&self.blocks, Size { width, height }); },
                                KeyCode::Char('l') => { width = width.saturating_add(2); view.reflow(&self.blocks, Size { width, height }); },
                                KeyCode::PageDown => view.down((height/2) as usize),
                                KeyCode::PageUp => view.up((height/2) as usize),
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
        Ok(())
    }
}
