use super::*;
use crate::view::base_component::text_area::caret_map::CaretAffinity;

fn ta(text: &str, cursor: usize) -> TextArea {
    let mut t = TextArea::new();
    t.content = text.to_string();
    t.cursor_char = cursor;
    t
}

#[test]
fn delete_prev_word_strips_back_to_word_start() {
    let mut t = ta("foo bar baz", 11);
    assert!(t.delete_prev_word());
    assert_eq!(t.content, "foo bar ");
    assert_eq!(t.cursor_char, 8);
}

#[test]
fn delete_prev_word_eats_trailing_whitespace_then_word() {
    let mut t = ta("foo bar  ", 9);
    assert!(t.delete_prev_word());
    assert_eq!(t.content, "foo ");
    assert_eq!(t.cursor_char, 4);
}

#[test]
fn delete_prev_word_at_start_is_noop() {
    let mut t = ta("foo", 0);
    assert!(!t.delete_prev_word());
    assert_eq!(t.content, "foo");
}

#[test]
fn delete_next_word_strips_to_word_end() {
    let mut t = ta("foo bar baz", 0);
    assert!(t.delete_next_word());
    assert_eq!(t.content, " bar baz");
    assert_eq!(t.cursor_char, 0);
}

#[test]
fn delete_next_word_eats_leading_whitespace_then_word() {
    let mut t = ta("  foo bar", 0);
    assert!(t.delete_next_word());
    assert_eq!(t.content, " bar");
    assert_eq!(t.cursor_char, 0);
}

#[test]
fn delete_next_word_at_end_is_noop() {
    let mut t = ta("foo", 3);
    assert!(!t.delete_next_word());
    assert_eq!(t.content, "foo");
}

#[test]
fn insert_at_hard_newline_upstream_slot_stays_on_previous_line() {
    let mut t = ta("line1\nline2", "line1\n".chars().count());
    t.cursor_affinity = CaretAffinity::Upstream;

    assert!(t.insert_text("X"));

    assert_eq!(t.content, "line1X\nline2");
    assert_eq!(t.cursor_char, "line1X".chars().count());
    assert_eq!(t.cursor_affinity, CaretAffinity::Downstream);
}

#[test]
fn insert_at_hard_newline_downstream_slot_stays_on_next_line() {
    let mut t = ta("line1\nline2", "line1\n".chars().count());
    t.cursor_affinity = CaretAffinity::Downstream;

    assert!(t.insert_text("X"));

    assert_eq!(t.content, "line1\nXline2");
    assert_eq!(t.cursor_char, "line1\nX".chars().count());
}

#[test]
fn delete_prev_word_with_selection_falls_through_to_selection_delete() {
    let mut t = ta("hello world", 11);
    t.select_range(2, 8);
    assert!(t.delete_prev_word());
    assert_eq!(t.content, "herld");
    assert_eq!(t.cursor_char, 2);
}

#[test]
fn lowering_max_length_normalizes_live_edit_state() {
    let mut text_area = ta("abcdef", 6);
    text_area.selection_anchor_char = Some(1);
    text_area.selection_focus_char = Some(6);
    text_area.ime_preedit = "pending".to_string();
    text_area.ime_preedit_cursor = Some((7, 7));

    assert!(text_area.set_max_length(Some(3)));
    assert_eq!(text_area.content, "abc");
    assert_eq!(text_area.cursor_char, 3);
    assert_eq!(text_area.selection_range_chars(), None);
    assert!(text_area.ime_preedit.is_empty());
    assert_eq!(text_area.ime_preedit_cursor, None);
    assert!(text_area.children_dirty);
}
