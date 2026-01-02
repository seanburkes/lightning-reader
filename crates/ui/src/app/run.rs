use std::{io::stdout, time::Duration};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::layout::centered_rect;
use crate::reader_view::ReaderView;

use super::selection::handle_mouse_selection;
use super::settings::load_settings;
use super::types::{Command, CommandOutcome, Mode};
use super::App;

impl App {
    pub fn run(mut self) -> std::io::Result<usize> {
        let mut stdout = stdout();
        let raw_ok = enable_raw_mode().is_ok();
        if raw_ok {
            execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        } else {
            execute!(stdout, EnterAlternateScreen)?;
        }
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let mut view = ReaderView::new();
        let (saved_justify, saved_two_pane, _spritz_settings) = load_settings();
        view.justify = saved_justify;
        view.two_pane = saved_two_pane;
        view.book_title = self.book_title.clone();
        view.author = self.author.clone();
        view.theme = self.theme.clone();
        view.add_images_from_blocks(&self.blocks);
        let mut selection_anchor: Option<crate::reader_view::SelectionPoint> = None;
        let mut selection_active = false;
        let mut last_frame = Rect::default();
        let mut width: u16 = 60;
        let mut height: u16 = 20;
        // Use inner size for initial paginate to compute chapter_starts correctly
        let term_size = terminal
            .size()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let inner = ReaderView::inner_size(term_size.into(), width, view.two_pane);
        let p = reader_core::layout::paginate_with_justify(&self.blocks, inner, view.justify);
        view.pages = p.pages;
        view.chapter_starts = p.chapter_starts;
        view.anchors = p.anchors;
        view.chapter_titles = self.chapter_titles.clone();
        view.chapter_hrefs = self.chapter_hrefs.clone();
        view.total_pages = self.total_pages;
        view.total_chapters = self.total_chapters;
        view.toc_overrides = self.outlines.clone();
        if let Some(idx) = self.initial_page {
            view.current = idx.min(view.pages.len().saturating_sub(1));
        }
        let mut last_inner: (u16, u16) = (inner.width, inner.height);
        // ensure initial last_size is used by next draw comparison

        if !raw_ok {
            // Non-interactive fallback: draw once and exit cleanly
            let _ = terminal.draw(|f| {
                let size = f.area();
                last_frame = size;
                height = size.height.saturating_sub(2);
                let inner = ReaderView::inner_size(size, width, view.two_pane);
                self.poll_incoming(&mut view, inner);
                self.poll_incoming_chapters(&mut view, inner);
                self.maybe_request_prefetch(&view);
                self.maybe_request_chapter_prefetch(&view);
                view.reflow(&self.blocks, inner);
                view.render(f, size, width, self.last_search.as_deref());
            });
            #[cfg(feature = "kitty-images")]
            {
                let _ = view.render_images(terminal.backend_mut());
            }
            if raw_ok {
                let _ = disable_raw_mode();
                execute!(
                    terminal.backend_mut(),
                    LeaveAlternateScreen,
                    DisableMouseCapture
                )?;
            } else {
                execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
            }
            return Ok(view.current);
        }

        let mut exit = false;
        while !exit {
            let mut drew_view = false;
            terminal.draw(|f| {
                let size = f.area();
                last_frame = size;
                height = size.height.saturating_sub(2);
                // Respect configured column width; do not override with terminal width
                let inner = ReaderView::inner_size(size, width, view.two_pane);
                self.poll_incoming(&mut view, inner);
                self.poll_incoming_chapters(&mut view, inner);
                self.maybe_request_prefetch(&view);
                self.maybe_request_chapter_prefetch(&view);
                if (inner.width, inner.height) != last_inner {
                    view.reflow(&self.blocks, inner);
                    // Clamp current page if needed
                    view.current = view.current.min(view.pages.len().saturating_sub(1));
                    last_inner = (inner.width, inner.height);
                    view.selection = None;
                    selection_anchor = None;
                    selection_active = false;
                }
                match self.mode {
                    Mode::Reader => {
                        view.render(f, size, width, self.last_search.as_deref());
                        drew_view = true;
                    }
                    Mode::Toc => {
                        if let Some(t) = &self.toc {
                            t.render(f, size, width);
                        } else {
                            view.render(f, size, width, self.last_search.as_deref());
                            drew_view = true;
                        }
                    }
                    Mode::Spritz => {
                        if let Some(spritz) = self.spritz.as_mut() {
                            spritz.update();
                            spritz.render(f, size, width);
                        }
                    }
                }
                if let Some(search) = &self.search {
                    search.render(f, size);
                }
                if self.show_help {
                    let popup_area = centered_rect(70, 70, size);
                    let help_lines = match self.mode {
                        Mode::Spritz => vec![
                            "q / Ctrl-C / s / Esc: exit spritz mode",
                            "Space: toggle play/pause",
                            "j/k or arrows: navigate word-by-word",
                            "h / l or arrows: adjust column width",
                            "Ctrl+j/Ctrl+k: jump 10 words",
                            "+/- or =/_: adjust WPM by 10",
                            "[ / ]: adjust WPM by 50",
                            "r: rewind to chapter start",
                            "f: fast forward to chapter end",
                            "Enter: resume playing if paused",
                            "?: toggle this help",
                        ],
                        _ => vec![
                            "q / Ctrl-C: quit",
                            "s: toggle spritz speed reading mode",
                            "j / k or arrows: scroll lines",
                            "PageUp / PageDown: jump half-page",
                            "h / l or arrows: adjust column width",
                            "t: toggle table of contents; Enter to jump; Esc to close TOC",
                            "/: search; Enter to submit; Esc to cancel",
                            "J: toggle justification (persists)",
                            "b: toggle two-page spread (persists)",
                            "?: toggle this help",
                        ],
                    };
                    let help = Paragraph::new(help_lines.join("\n"))
                        .block(
                            Block::default()
                                .title("Help (Esc or ? to close)")
                                .borders(Borders::ALL),
                        )
                        .wrap(Wrap { trim: false });
                    f.render_widget(Clear, popup_area);
                    f.render_widget(help, popup_area);
                }
                if let Some(footnote) = &self.footnote {
                    footnote.render(f, size);
                }
            })?;
            #[cfg(feature = "kitty-images")]
            {
                if drew_view {
                    view.render_images(terminal.backend_mut())?;
                }
            }

            match event::poll(Duration::from_millis(100)) {
                Ok(true) => match event::read() {
                    Ok(Event::Mouse(mouse)) => {
                        if let Mode::Reader = self.mode {
                            if self.search.is_some() || self.show_help || self.footnote.is_some() {
                                continue;
                            }
                            handle_mouse_selection(
                                &mut self,
                                &mut view,
                                last_frame,
                                width,
                                mouse,
                                &mut selection_anchor,
                                &mut selection_active,
                            );
                        }
                    }
                    Ok(Event::Key(key)) => {
                        if let Some(command) = Command::from_key(&self, key) {
                            if self.apply_command(
                                command,
                                &mut view,
                                &mut width,
                                height,
                                &mut last_inner,
                                &mut terminal,
                            )? == CommandOutcome::Exit
                            {
                                exit = true;
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(_) => {
                        exit = true;
                    }
                },
                Ok(false) => {}
                Err(_) => {
                    exit = true;
                }
            }
        }

        if raw_ok {
            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )?;
        } else {
            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        }
        Ok(view.current)
    }
}
