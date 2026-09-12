use super::*;
use crate::view::base_component::{
    ElementTrait, LayoutConstraints, LayoutPlacement, TextArea as HostTextArea,
};

fn wrapped_text_area(content: &str, max_width: f32) -> (NodeArena, NodeKey) {
    let mut text_area = HostTextArea::new();
    text_area.content = content.to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.is_focused = true;

    let mut arena = crate::view::test_support::new_test_arena();
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(text_area) as Box<dyn ElementTrait>,
    );
    arena.with_element_taken(root, |el, _| {
        el.as_any_mut()
            .downcast_mut::<HostTextArea>()
            .expect("TextArea root")
            .set_self_node_key(root);
    });
    crate::view::test_support::measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width,
            max_height: 600.0,
            viewport_width: max_width,
            viewport_height: 600.0,
            percent_base_width: Some(max_width),
            percent_base_height: Some(600.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: max_width,
            available_height: 600.0,
            viewport_width: max_width,
            viewport_height: 600.0,
            percent_base_width: Some(max_width),
            percent_base_height: Some(600.0),
        },
    );
    (arena, root)
}

fn nowrap_text_area(content: &str, cursor_char: usize, max_width: f32) -> (NodeArena, NodeKey) {
    let mut text_area = HostTextArea::new();
    text_area.content = content.to_string();
    text_area.cursor_char = cursor_char;
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.auto_wrap = false;
    text_area.is_focused = true;
    text_area.pending_caret_scroll = true;

    let mut arena = crate::view::test_support::new_test_arena();
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(text_area) as Box<dyn ElementTrait>,
    );
    arena.with_element_taken(root, |el, _| {
        el.as_any_mut()
            .downcast_mut::<HostTextArea>()
            .expect("TextArea root")
            .set_self_node_key(root);
    });
    place_nowrap_text_area(&mut arena, root, max_width);
    (arena, root)
}

fn place_nowrap_text_area(arena: &mut NodeArena, root: NodeKey, max_width: f32) {
    crate::view::test_support::measure_and_place(
        arena,
        root,
        LayoutConstraints {
            max_width,
            max_height: 600.0,
            viewport_width: max_width,
            viewport_height: 600.0,
            percent_base_width: Some(max_width),
            percent_base_height: Some(600.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: max_width,
            available_height: 600.0,
            viewport_width: max_width,
            viewport_height: 600.0,
            percent_base_width: Some(max_width),
            percent_base_height: Some(600.0),
        },
    );
}

fn first_consumed_wrap_whitespace(
    text_area: &mut HostTextArea,
    arena: &NodeArena,
) -> (usize, f32, f32) {
    let map = CaretNavigationMap::build(text_area, arena);
    for pair in map.lines.windows(2) {
        let Some(upper_tail) = pair[0].stops.last() else {
            continue;
        };
        let Some(lower_head) = pair[1].stops.first() else {
            continue;
        };
        let consumed: String = text_area
            .content
            .chars()
            .skip(upper_tail.char_index)
            .take(lower_head.char_index.saturating_sub(upper_tail.char_index))
            .collect();
        if !consumed.is_empty() && consumed.chars().all(char::is_whitespace) {
            return (upper_tail.char_index, pair[0].y_top, pair[1].y_top);
        }
    }
    panic!("expected automatic wrap that consumes trailing whitespace");
}

#[test]
fn pointer_down_at_soft_wrap_tail_keeps_caret_on_upper_line() {
    let (mut arena, root) = wrapped_text_area("the quick brown fox jumps over the lazy dog", 80.0);
    arena.with_element_taken(root, |el, arena| {
        let text_area = el
            .as_any_mut()
            .downcast_mut::<HostTextArea>()
            .expect("TextArea root");
        let map = CaretNavigationMap::build(text_area, arena);
        assert!(map.lines.len() >= 2, "soft-wrap expected");
        let line0 = &map.lines[0];
        let upper_y = line0.y_top;
        let line0_mid_y = (line0.y_top + line0.y_bottom) * 0.5;
        let upper_tail = line0.stops.last().expect("upper line tail");

        let target =
            text_area.cursor_target_at_screen(arena, upper_tail.x + 1000.0, line0_mid_y);
        text_area.start_pointer_selection_with_affinity(target.char_index, target.affinity);

        assert_eq!(text_area.cursor_affinity, CaretAffinity::Upstream);
        let (_, caret_y, _) = text_area
            .caret_screen_position(arena)
            .expect("caret should resolve");
        assert!(
            (caret_y - upper_y).abs() < 0.5,
            "pointer-down at upper line tail should keep caret on upper line: caret_y={caret_y}, upper_y={upper_y}",
        );
    });
}

#[test]
fn arrow_right_skips_trailing_wrap_whitespace() {
    let (mut arena, root) = wrapped_text_area("the quick brown fox jumps over the lazy dog", 80.0);
    arena.with_element_taken(root, |el, arena| {
        let text_area = el
            .as_any_mut()
            .downcast_mut::<HostTextArea>()
            .expect("TextArea root");
        let (upper_tail, up_y, down_y) = first_consumed_wrap_whitespace(text_area, arena);

        text_area.cursor_char = upper_tail;
        text_area.cursor_affinity = CaretAffinity::Downstream;
        assert!(text_area.handle_horizontal_arrow(arena, true));
        let (_, caret_y, _) = text_area.caret_screen_position(arena).expect("caret");
        assert!((caret_y - down_y).abs() <= 0.5);
        assert!(caret_y > up_y);
    });
}

#[test]
fn arrow_left_skips_trailing_wrap_whitespace() {
    let (mut arena, root) = wrapped_text_area("the quick brown fox jumps over the lazy dog", 80.0);
    arena.with_element_taken(root, |el, arena| {
        let text_area = el
            .as_any_mut()
            .downcast_mut::<HostTextArea>()
            .expect("TextArea root");
        let (upper_tail, up_y, down_y) = first_consumed_wrap_whitespace(text_area, arena);
        let lower_head = text_area
            .content
            .chars()
            .enumerate()
            .skip(upper_tail)
            .find_map(|(index, ch)| (!ch.is_whitespace()).then_some(index))
            .expect("wrapped lower line should begin with visible text");

        text_area.cursor_char = lower_head;
        text_area.cursor_affinity = CaretAffinity::Downstream;
        assert!((text_area.caret_screen_position(arena).expect("caret").1 - down_y).abs() <= 0.5);

        assert!(text_area.handle_horizontal_arrow(arena, false));
        assert_eq!(text_area.cursor_char, upper_tail);
        let (_, caret_y, _) = text_area.caret_screen_position(arena).expect("caret");
        assert!((caret_y - up_y).abs() <= 0.5);
    });
}

#[test]
fn arrow_right_skips_hard_newline_slot_with_same_position() {
    let (mut arena, root) = wrapped_text_area("a\nb", 300.0);
    arena.with_element_taken(root, |el, arena| {
        let text_area = el
            .as_any_mut()
            .downcast_mut::<HostTextArea>()
            .expect("TextArea root");

        text_area.cursor_char = 1;
        text_area.cursor_affinity = CaretAffinity::Downstream;
        let (_, start_y, _) = text_area.caret_screen_position(arena).expect("caret");

        assert!(text_area.handle_horizontal_arrow(arena, true));
        assert_eq!(text_area.cursor_char, 2);
        assert_eq!(text_area.cursor_affinity, CaretAffinity::Downstream);
        let (_, target_y, _) = text_area.caret_screen_position(arena).expect("caret");
        assert!(
            target_y > start_y,
            "ArrowRight should skip the unpainted newline slot and land on the lower line",
        );
    });
}

#[test]
fn arrow_right_crosses_projected_hard_newline_to_tail_line() {
    let content = "First line with a long value that can wrap when auto wrap is enabled.{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\nTail line";
    let mut text_area = HostTextArea::new();
    text_area.content = content.to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.auto_wrap = true;
    text_area.is_focused = true;
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        let ranges = [(69..81), (91..102)];
        for range in ranges {
            let slice: String = content
                .chars()
                .skip(range.start)
                .take(range.len())
                .collect();
            render.range(range.clone(), move |_node| {
                let slice = slice.clone();
                crate::ui::RsxNode::tagged(
                    "Element",
                    crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                )
                .with_prop(
                    "style",
                    crate::view::ElementStylePropSchema {
                        padding: Some(
                            crate::style::Padding::uniform(crate::style::Length::px(0.0))
                                .x(crate::style::Length::px(20.0)),
                        ),
                        font_size: Some(crate::style::FontSize::Px(24.0)),
                        border: Some(crate::style::Border::uniform(
                            crate::style::Length::px(1.0),
                            &crate::style::Color::hex("#42566f"),
                        )),
                        ..Default::default()
                    },
                )
                .with_child(
                    crate::ui::RsxNode::tagged(
                        "Text",
                        crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                    )
                    .with_child(crate::ui::RsxNode::text(slice)),
                )
            });
        }
    }));

    let mut arena = crate::view::test_support::new_test_arena();
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(text_area) as Box<dyn ElementTrait>,
    );
    arena.with_element_taken(root, |el, _| {
        el.as_any_mut()
            .downcast_mut::<HostTextArea>()
            .expect("TextArea root")
            .set_self_node_key(root);
    });
    crate::view::test_support::measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 342.0,
            max_height: 176.0,
            viewport_width: 342.0,
            viewport_height: 176.0,
            percent_base_width: Some(342.0),
            percent_base_height: Some(176.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 342.0,
            available_height: 176.0,
            viewport_width: 342.0,
            viewport_height: 176.0,
            percent_base_width: Some(342.0),
            percent_base_height: Some(176.0),
        },
    );

    arena.with_element_taken(root, |el, arena| {
        let text_area = el
            .as_any_mut()
            .downcast_mut::<HostTextArea>()
            .expect("TextArea root");
        let newline = text_area
            .content
            .chars()
            .position(|ch| ch == '\n')
            .expect("newline");
        text_area.cursor_char = newline.saturating_sub(1);
        text_area.cursor_affinity = CaretAffinity::Downstream;

        while text_area.cursor_char < newline + 1 {
            assert!(text_area.handle_horizontal_arrow(arena, true));
        }
        assert_eq!(text_area.cursor_char, newline + 1);
        assert_eq!(text_area.cursor_affinity, CaretAffinity::Downstream);
        let (tail_start_x, tail_start_y, _) =
            text_area.caret_screen_position(arena).expect("tail start caret");

        assert!(text_area.handle_horizontal_arrow(arena, true));
        assert_eq!(text_area.cursor_char, newline + 2);
        let (tail_next_x, tail_next_y, _) =
            text_area.caret_screen_position(arena).expect("tail next caret");
        assert!(
            (tail_next_y - tail_start_y).abs() <= 0.5,
            "ArrowRight from Tail start must stay on Tail line, start_y={tail_start_y}, next_y={tail_next_y}",
        );
        assert!(
            tail_next_x > tail_start_x + 0.5,
            "ArrowRight from Tail start must move right, start_x={tail_start_x}, next_x={tail_next_x}",
        );
    });
}

#[test]
fn caret_map_omits_consumed_wrap_whitespace() {
    let (mut arena, root) = wrapped_text_area("the quick brown fox jumps over the lazy dog", 80.0);
    arena.with_element_taken(root, |el, arena| {
        let text_area = el
            .as_any_mut()
            .downcast_mut::<HostTextArea>()
            .expect("TextArea root");
        let map = CaretNavigationMap::build(text_area, arena);
        for pair in map.lines.windows(2) {
            let (Some(upper_tail), Some(lower_head)) =
                (pair[0].stops.last(), pair[1].stops.first())
            else {
                continue;
            };
            let consumed: String = text_area
                .content
                .chars()
                .skip(upper_tail.char_index)
                .take(lower_head.char_index.saturating_sub(upper_tail.char_index))
                .collect();
            if !consumed.is_empty() && consumed.chars().all(char::is_whitespace) {
                assert!(
                    upper_tail.char_index < lower_head.char_index,
                    "the consumed whitespace must not share a caret slot"
                );
                return;
            }
        }
        panic!("fixture should contain a wrap consuming whitespace");
    });
}

#[test]
fn vertical_arrow_preserves_content_x_when_horizontal_scroll_changes() {
    let long = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let content = format!("{long}\nshort\n{long}");
    let first_line_end = long.chars().count();
    let third_line_start = first_line_end + 1 + "short".chars().count() + 1;
    let (mut arena, root) = nowrap_text_area(&content, first_line_end, 80.0);

    let original_content_x = arena
        .with_element_taken_ref(root, |el, arena| {
            let text_area = el.as_any().downcast_ref::<HostTextArea>().unwrap();
            assert!(text_area.scroll_x > 0.0);
            let (x, _, _) = text_area.caret_screen_position(arena).expect("caret");
            x + text_area.scroll_x
        })
        .unwrap();

    arena.with_element_taken(root, |el, arena| {
        let text_area = el
            .as_any_mut()
            .downcast_mut::<HostTextArea>()
            .expect("TextArea root");
        assert!(text_area.handle_vertical_arrow(arena, VerticalDirection::Down, false));
        assert!(text_area.scroll_caret_into_view(arena));
    });
    place_nowrap_text_area(&mut arena, root, 80.0);

    let scroll_after_short_line = arena
        .with_element_taken_ref(root, |el, _| {
            el.as_any().downcast_ref::<HostTextArea>().unwrap().scroll_x
        })
        .unwrap();
    assert!(
        scroll_after_short_line < original_content_x - 80.0,
        "moving to the short line should reduce horizontal scroll enough to expose the stale-screen-x bug",
    );

    arena.with_element_taken(root, |el, arena| {
        let text_area = el
            .as_any_mut()
            .downcast_mut::<HostTextArea>()
            .expect("TextArea root");
        assert!(text_area.handle_vertical_arrow(arena, VerticalDirection::Down, false));
        assert!(
            text_area.cursor_char > third_line_start + first_line_end / 2,
            "second Down should return near the original far-right content column, got cursor_char={}",
            text_area.cursor_char,
        );
        let (x, _, _) = text_area.caret_screen_position(arena).expect("caret");
        let final_content_x = x + text_area.scroll_x;
        assert!(
            (final_content_x - original_content_x).abs() <= 8.0,
            "sticky x should be content-space: original={original_content_x}, final={final_content_x}",
        );
    });
}
