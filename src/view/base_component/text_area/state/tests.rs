use super::super::caret_map::CaretAffinity;
use super::*;

#[test]
fn pointer_drag_update_resets_stale_affinity() {
    let mut text_area = TextArea::new();
    text_area.content = "abcdef".to_string();
    text_area.cursor_affinity = CaretAffinity::Upstream;
    text_area.vertical_cursor_x = Some(42.0);
    text_area.pointer_selecting = true;
    text_area.selection_anchor_char = Some(0);
    text_area.selection_focus_char = Some(0);

    text_area.update_pointer_selection_with_affinity(3, CaretAffinity::Downstream);

    assert_eq!(text_area.cursor_char, 3);
    assert_eq!(text_area.selection_focus_char, Some(3));
    assert_eq!(text_area.cursor_affinity, CaretAffinity::Downstream);
    assert_eq!(text_area.vertical_cursor_x, None);
}
