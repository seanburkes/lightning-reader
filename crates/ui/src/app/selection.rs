use arboard::Clipboard;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::prelude::Rect;
use unicode_segmentation::UnicodeSegmentation;

use crate::reader_view::{ReaderView, SelectionPoint, SelectionRange};

use super::footnotes::{find_footnote_text, is_footnote_link};
use super::App;

impl App {
    pub(super) fn mouse_selection_point(
        view: &ReaderView,
        area: Rect,
        column_width: u16,
        mouse: MouseEvent,
    ) -> Option<SelectionPoint> {
        let areas = view.content_areas(area, column_width);
        let is_spread = areas.right.is_some();
        let (pane, page_idx) = if rect_contains(areas.left, mouse.column, mouse.row) {
            let page = if is_spread {
                view.current.saturating_sub(view.current % 2)
            } else {
                view.current
            };
            (areas.left, page)
        } else if let Some(right) = areas.right {
            if rect_contains(right, mouse.column, mouse.row) {
                let base = view.current.saturating_sub(view.current % 2);
                (right, base + 1)
            } else {
                return None;
            }
        } else {
            return None;
        };

        let page = view.pages.get(page_idx)?;
        let line_idx = mouse.row.saturating_sub(pane.y) as usize;
        let line = page.lines.get(line_idx)?;
        let line_text = line_text(line);
        let line_len = line_text.graphemes(true).count();
        let col_idx = mouse.column.saturating_sub(pane.x) as usize;
        let col_idx = col_idx.min(line_len);
        Some(SelectionPoint {
            page: page_idx,
            line: line_idx,
            col: col_idx,
        })
    }

    pub(super) fn copy_selection(&mut self, view: &ReaderView, selection: SelectionRange) {
        let text = selection_text(view, selection);
        if text.trim().is_empty() {
            return;
        }
        if self.clipboard.is_none() {
            self.clipboard = Clipboard::new().ok();
        }
        if let Some(clipboard) = &mut self.clipboard {
            let _ = clipboard.set_text(text);
        }
    }

    pub(super) fn maybe_open_footnote(
        &mut self,
        _view: &ReaderView,
        target: &str,
        label: Option<&str>,
    ) -> bool {
        if !is_footnote_link(target, label) {
            return false;
        }
        let Some(text) = find_footnote_text(&self.blocks, target, label) else {
            return false;
        };
        if text.trim().is_empty() {
            return false;
        }
        self.footnote = Some(crate::views::FootnoteView::new(text));
        true
    }
}

pub(super) fn handle_mouse_selection(
    app: &mut App,
    view: &mut ReaderView,
    area: Rect,
    column_width: u16,
    mouse: MouseEvent,
    selection_anchor: &mut Option<SelectionPoint>,
    selection_active: &mut bool,
) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(point) = App::mouse_selection_point(view, area, column_width, mouse) {
                *selection_anchor = Some(point);
                *selection_active = true;
                view.selection = Some(SelectionRange {
                    start: point,
                    end: point,
                });
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if *selection_active {
                if let Some(point) = App::mouse_selection_point(view, area, column_width, mouse) {
                    if let Some(anchor) = *selection_anchor {
                        view.selection = Some(SelectionRange {
                            start: anchor,
                            end: point,
                        });
                    }
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if *selection_active {
                if let Some(selection) = view.selection {
                    let (start, end) = selection.normalized();
                    if start == end {
                        if let Some(target) = view.link_at_point(start) {
                            let label = view.link_label_at_point(start);
                            if !app.maybe_open_footnote(view, &target, label.as_deref()) {
                                view.jump_to_target(&target);
                            }
                        }
                    } else {
                        app.copy_selection(view, selection);
                    }
                }
                view.selection = None;
                *selection_anchor = None;
                *selection_active = false;
            }
        }
        _ => {}
    }
}

fn rect_contains(rect: Rect, col: u16, row: u16) -> bool {
    let x_end = rect.x.saturating_add(rect.width);
    let y_end = rect.y.saturating_add(rect.height);
    col >= rect.x && col < x_end && row >= rect.y && row < y_end
}

fn line_text(line: &reader_core::layout::StyledLine) -> String {
    line.segments
        .iter()
        .map(|seg| seg.text.as_str())
        .collect::<String>()
}

fn slice_graphemes(text: &str, start: usize, end: usize) -> String {
    if start >= end {
        return String::new();
    }
    let graphemes: Vec<&str> = text.graphemes(true).collect();
    if start >= graphemes.len() {
        return String::new();
    }
    let end = end.min(graphemes.len());
    graphemes[start..end].concat()
}

fn selection_text(view: &ReaderView, selection: SelectionRange) -> String {
    let (start, end) = selection.normalized();
    let mut out: Vec<String> = Vec::new();
    for page_idx in start.page..=end.page {
        let Some(page) = view.pages.get(page_idx) else {
            continue;
        };
        if page.lines.is_empty() {
            continue;
        }
        let last_line = page.lines.len().saturating_sub(1);
        let start_line = if page_idx == start.page {
            start.line.min(last_line)
        } else {
            0
        };
        let end_line = if page_idx == end.page {
            end.line.min(last_line)
        } else {
            last_line
        };
        if start_line > end_line {
            continue;
        }
        for line_idx in start_line..=end_line {
            let text = line_text(&page.lines[line_idx]);
            let line_len = text.graphemes(true).count();
            let selected = if page_idx == start.page && line_idx == start_line {
                let start_col = start.col.min(line_len);
                let end_col = if page_idx == end.page && line_idx == end_line {
                    end.col.min(line_len)
                } else {
                    line_len
                };
                slice_graphemes(&text, start_col, end_col)
            } else if page_idx == end.page && line_idx == end_line {
                let end_col = end.col.min(line_len);
                slice_graphemes(&text, 0, end_col)
            } else {
                text
            };
            out.push(selected);
        }
    }
    out.join("\n")
}
