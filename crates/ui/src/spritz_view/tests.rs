use reader_core::layout::WordToken;

use crate::app::SpritzSettings;
use crate::reader_view::Theme;

use super::SpritzView;

fn dummy_settings() -> SpritzSettings {
    SpritzSettings {
        wpm: 250,
        pause_on_punct: true,
        punct_pause_ms: 100,
    }
}

fn dummy_theme() -> Theme {
    Theme::default()
}

#[test]
fn orp_position_short_word() {
    assert_eq!(SpritzView::get_orp_position("a"), 0);
    assert_eq!(SpritzView::get_orp_position("hi"), 1);
    assert_eq!(SpritzView::get_orp_position("cat"), 1);
}

#[test]
fn orp_position_medium_word() {
    assert_eq!(SpritzView::get_orp_position("hello"), 2);
    assert_eq!(SpritzView::get_orp_position("reading"), 2);
}

#[test]
fn orp_position_long_word() {
    assert_eq!(SpritzView::get_orp_position("extraordinary"), 5);
}

#[test]
fn adjust_wpm_clamps_to_range() {
    let words = vec![];
    let settings = dummy_settings();
    let theme = dummy_theme();
    let mut view = SpritzView::new(words, settings, vec![], theme);
    view.wpm = 100;

    view.adjust_wpm(-200);
    assert_eq!(view.wpm, 100);

    view.wpm = 1000;
    view.adjust_wpm(200);
    assert_eq!(view.wpm, 1000);
}

#[test]
fn rewind_saturates_at_zero() {
    let words = vec![WordToken {
        text: "test".to_string(),
        is_sentence_end: false,
        is_comma: false,
        chapter_index: None,
    }];
    let settings = dummy_settings();
    let theme = dummy_theme();
    let mut view = SpritzView::new(words, settings, vec![], theme);
    view.current_index = 0;

    view.rewind(10);
    assert_eq!(view.current_index, 0);
}

#[test]
fn fast_forward_clamps_to_end() {
    let words = vec![
        WordToken {
            text: "one".to_string(),
            is_sentence_end: false,
            is_comma: false,
            chapter_index: None,
        },
        WordToken {
            text: "two".to_string(),
            is_sentence_end: false,
            is_comma: false,
            chapter_index: None,
        },
    ];
    let settings = dummy_settings();
    let theme = dummy_theme();
    let mut view = SpritzView::new(words, settings, vec![], theme);
    view.current_index = 0;

    view.fast_forward(10);
    assert_eq!(view.current_index, 1);
}

#[test]
fn toggle_play_switches_state() {
    let words = vec![];
    let settings = dummy_settings();
    let theme = dummy_theme();
    let mut view = SpritzView::new(words, settings, vec![], theme);

    assert!(!view.is_playing);
    view.toggle_play();
    assert!(view.is_playing);
    view.toggle_play();
    assert!(!view.is_playing);
}
