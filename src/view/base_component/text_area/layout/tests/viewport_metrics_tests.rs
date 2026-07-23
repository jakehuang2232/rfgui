use super::*;

#[test]
fn place_preserves_text_area_origin_and_children_fractional_layout() {
    let mut text_area = TextArea::new();
    text_area.content = "snap me".to_string();

    let mut arena = crate::view::test_support::new_test_arena();
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(text_area) as Box<dyn ElementTrait>,
    );
    crate::view::test_support::measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 200.0,
            max_height: 40.0,
            viewport_width: 200.0,
            viewport_height: 40.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(40.0),
        },
        LayoutPlacement {
            parent_x: 10.25,
            parent_y: 20.75,
            visual_offset_x: 0.25,
            visual_offset_y: -0.25,
            available_width: 200.0,
            available_height: 40.0,
            viewport_width: 200.0,
            viewport_height: 40.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(40.0),
        },
    );

    arena.with_element_taken_ref(root, |el, arena| {
        let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
        assert_eq!(text_area.layout_state.layout_position.x, 10.5);
        assert_eq!(text_area.layout_state.layout_position.y, 20.5);
        let first_child = *text_area.children.first().expect("text run child");
        let child_snapshot = arena
            .get(first_child)
            .expect("child node")
            .element
            .box_model_snapshot();
        assert_eq!(child_snapshot.x, 10.5);
        assert_eq!(child_snapshot.y, 20.5);
    });
}

#[test]
fn short_content_hit_test_extends_to_viewport_width() {
    let content = "hi";
    let (arena, root) = placed_text_area(content, 0, 300.0, 40.0, true);

    arena.with_element_taken_ref(root, |el, arena| {
        let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
        assert!(
            text_area.layout_state.layout_size.width < 250.0,
            "fixture should keep content width narrower than the click x",
        );
        assert!(
            text_area.box_model_snapshot().width >= 300.0,
            "TextArea hit box should extend to its viewport width",
        );
        let target = text_area.cursor_target_at_screen(arena, 250.0, 8.0);
        assert_eq!(
            target.char_index,
            content.chars().count(),
            "clicking to the right of short content should place caret at line tail",
        );
    });

    assert_eq!(
        hit_test(&arena, root, 250.0, 8.0),
        Some(root),
        "TextArea should receive pointer hits across its configured width",
    );
}

#[test]
fn nowrap_keeps_content_width_as_reported_layout_width() {
    let content = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";
    let (arena, root) = placed_text_area(content, content.chars().count(), 80.0, 40.0, false);

    arena.with_element_taken_ref(root, |el, _| {
        let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
        assert!(text_area.viewport_size.width <= 80.0);
        assert!(
            text_area.layout_state.layout_size.width > text_area.viewport_size.width,
            "TextArea must keep reporting content width so the parent Element can clip overflow",
        );
    });
}
