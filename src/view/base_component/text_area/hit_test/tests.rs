use super::*;
use crate::view::base_component::ElementTrait;

/// `"a\n\nb"` splits into 3 Runs ("a"[0..2,ttn], ""[2..3,ttn],
/// "b"[3..4]). The middle empty Run has `snap.width=0` (no shaping
/// for empty paragraphs), so first-pass `point_in_rect` always
/// misses it. The fallback path's x-midpoint rule used to bias every
/// click in that band to `range_end` (= start of "b" Run), making
/// it impossible to click into the visible blank middle line. This
/// test pins the empty-Run behavior: any click in the empty Run's
/// vertical band must collapse to `range_start` (= the empty
/// paragraph's own char index).
#[test]
fn click_in_middle_empty_paragraph_lands_in_empty_run_not_following_sibling() {
    let mut text_area = TextArea::new();
    text_area.content = "a\n\nb".to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;

    let mut arena = crate::view::test_support::new_test_arena();
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(text_area) as Box<dyn ElementTrait>,
    );
    arena.with_element_taken(root, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea root")
            .set_self_node_key(root);
    });
    crate::view::test_support::measure_and_place(
        &mut arena,
        root,
        crate::view::base_component::LayoutConstraints {
            max_width: 300.0,
            max_height: 300.0,
            viewport_width: 300.0,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
        crate::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 300.0,
            available_height: 300.0,
            viewport_width: 300.0,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
    );

    // Visual lines stack at line_height = 14 * 1.25 = 17.5.
    // Empty Run sits in the y band [17.5, 35.0].
    let click_y = 25.0;
    let click_x_left = 5.0;
    let click_x_right = 250.0;
    let cursor_left = arena
        .with_element_taken_ref(root, |el, arena| {
            el.as_any()
                .downcast_ref::<TextArea>()
                .unwrap()
                .cursor_target_at_screen(arena, click_x_left, click_y)
                .char_index
        })
        .unwrap();
    let cursor_right = arena
        .with_element_taken_ref(root, |el, arena| {
            el.as_any()
                .downcast_ref::<TextArea>()
                .unwrap()
                .cursor_target_at_screen(arena, click_x_right, click_y)
                .char_index
        })
        .unwrap();
    assert_eq!(
        cursor_left, 2,
        "click on empty middle line at small x should land on the empty Run's char (got {cursor_left})",
    );
    assert_eq!(
        cursor_right, 2,
        "click on empty middle line at large x should land on the empty Run's char (got {cursor_right})",
    );
}
