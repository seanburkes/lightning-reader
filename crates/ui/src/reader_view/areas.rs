use ratatui::layout::{Constraint, Direction, Layout, Rect};

use reader_core::layout::Size;

use super::ReaderView;
use super::SPREAD_GAP;

pub struct ContentAreas {
    pub body: Rect,
    pub left: Rect,
    pub right: Option<Rect>,
}

impl ReaderView {
    pub fn content_areas(&self, area: Rect, column_width: u16) -> ContentAreas {
        let vchunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);
        let content_area = vchunks[0];
        let col_w = if self.two_pane {
            column_width
                .saturating_mul(2)
                .saturating_add(SPREAD_GAP)
                .min(content_area.width)
        } else {
            column_width.min(content_area.width)
        };
        let left_pad = content_area.width.saturating_sub(col_w) / 2;
        let centered = Rect {
            x: content_area.x + left_pad,
            y: content_area.y,
            width: col_w,
            height: content_area.height,
        };
        let header_footer_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(centered);
        let para_area = header_footer_chunks[1];
        if self.two_pane && col_w > 6 {
            let gap = SPREAD_GAP.min(col_w.saturating_sub(2));
            let remaining = col_w.saturating_sub(gap);
            let left_w = remaining / 2;
            let right_w = remaining.saturating_sub(left_w);
            let spreads = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(left_w),
                    Constraint::Length(gap),
                    Constraint::Length(right_w),
                ])
                .split(para_area);
            ContentAreas {
                body: para_area,
                left: spreads[0],
                right: Some(spreads[2]),
            }
        } else {
            ContentAreas {
                body: para_area,
                left: para_area,
                right: None,
            }
        }
    }

    pub fn inner_size(area: Rect, column_width: u16, two_pane: bool) -> Size {
        let vchunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);
        let content_area = vchunks[0];
        let col_w = if two_pane {
            column_width
                .saturating_mul(2)
                .saturating_add(SPREAD_GAP)
                .min(content_area.width)
        } else {
            column_width.min(content_area.width)
        };
        let inner_w = if two_pane {
            col_w.saturating_sub(SPREAD_GAP) / 2
        } else {
            col_w
        };
        let inner_h = content_area.height.saturating_sub(2);
        Size {
            width: inner_w,
            height: inner_h,
        }
    }
}
