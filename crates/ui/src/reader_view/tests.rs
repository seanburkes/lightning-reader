use ratatui::style::Color;

use reader_core::layout::{Page, Segment, StyledLine, TextStyle};

use super::ReaderView;

fn page(lines: &[&str]) -> Page {
    Page {
        lines: lines
            .iter()
            .map(|s| StyledLine {
                segments: vec![Segment {
                    text: (*s).to_string(),
                    fg: None,
                    bg: None,
                    style: TextStyle::default(),
                    link: None,
                }],
                image: None,
            })
            .collect(),
    }
}

#[test]
fn search_forward_is_case_insensitive() {
    let mut view = ReaderView::new();
    view.pages = vec![
        page(&["First page"]),
        page(&["Second Match"]),
        page(&["Third"]),
    ];
    assert_eq!(view.search_forward("match", None), Some(1));
    assert_eq!(view.search_forward("SeCoNd", None), Some(1));
}

#[test]
fn search_forward_wraps_from_end() {
    let mut view = ReaderView::new();
    view.pages = vec![page(&["Alpha"]), page(&["Beta"]), page(&["Gamma"])];
    view.current = 2;
    assert_eq!(view.search_forward("alpha", None), Some(0));
}

#[test]
fn search_forward_matches_across_lines() {
    let mut view = ReaderView::new();
    view.pages = vec![page(&["Hello brave", "new world"]), page(&["Unused"])];
    assert_eq!(view.search_forward("brave new", None), Some(0));
}

#[test]
fn search_forward_can_start_after_previous_hit() {
    let mut view = ReaderView::new();
    view.pages = vec![
        page(&["One fish"]),
        page(&["Two fish"]),
        page(&["Red fish"]),
        page(&["Blue fish"]),
    ];
    assert_eq!(view.search_forward("fish", None), Some(0));
    assert_eq!(view.search_forward("fish", Some(1)), Some(1));
    assert_eq!(view.search_forward("fish", Some(2)), Some(2));
    assert_eq!(view.search_forward("fish", Some(3)), Some(3));
    assert_eq!(view.search_forward("fish", Some(4)), Some(0)); // wraps
}

#[test]
fn highlight_line_marks_case_insensitive_matches() {
    let styled = StyledLine {
        segments: vec![Segment {
            text: "Hello World".into(),
            fg: None,
            bg: None,
            style: TextStyle::default(),
            link: None,
        }],
        image: None,
    };
    let line = ReaderView::highlight_line(&styled, Some("world"), None);
    assert_eq!(line.spans.len(), 2);
    assert_eq!(line.spans[0].content, "Hello ");
    assert_eq!(line.spans[1].content, "World");
    assert_eq!(line.spans[1].style.bg, Some(Color::Yellow));
}

#[test]
fn highlight_line_marks_multiple_occurrences() {
    let styled = StyledLine {
        segments: vec![Segment {
            text: "aba ba".into(),
            fg: None,
            bg: None,
            style: TextStyle::default(),
            link: None,
        }],
        image: None,
    };
    let line = ReaderView::highlight_line(&styled, Some("ba"), None);
    assert_eq!(line.spans.len(), 4); // "a" + "ba" + " " + "ba"
    assert_eq!(line.spans[1].content, "ba");
    assert_eq!(line.spans[1].style.bg, Some(Color::Yellow));
    assert_eq!(line.spans[3].content, "ba");
    assert_eq!(line.spans[3].style.bg, Some(Color::Yellow));
}
