use super::*;

#[test]
fn place_scrolls_viewport_down_to_caret() {
    let content = "one\ntwo\nthree\nfour\nfive";
    let (arena, root) = placed_text_area(content, content.chars().count(), 200.0, 35.0, true);

    arena.with_element_taken_ref(root, |el, arena| {
        let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
        let (_, caret_y, caret_h) = text_area.caret_screen_position(arena).expect("caret");
        let viewport_bottom =
            text_area.layout_state.layout_position.y + text_area.viewport_size.height;

        assert!(text_area.scroll_y > 0.0);
        assert!(caret_y + caret_h <= viewport_bottom + 0.5);
    });
}

#[test]
fn place_preserves_parent_assigned_height_for_vertical_caret_follow() {
    let content = "one\ntwo\nthree\nfour\nfive";
    let mut text_area = TextArea::new();
    text_area.content = content.to_string();
    text_area.cursor_char = content.chars().count();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.pending_caret_scroll = true;

    let mut arena = crate::view::test_support::new_test_arena();
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(text_area) as Box<dyn ElementTrait>,
    );
    arena.with_element_taken(root, |el, _| {
        let text_area = el.as_any_mut().downcast_mut::<TextArea>().unwrap();
        text_area.set_self_node_key(root);
    });
    arena.with_element_taken(root, |el, arena| {
        el.measure(
            LayoutConstraints {
                max_width: 200.0,
                max_height: 600.0,
                viewport_width: 200.0,
                viewport_height: 600.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(600.0),
            },
            arena,
        );
        el.set_layout_height(35.0);
        el.place(
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 200.0,
                available_height: 600.0,
                viewport_width: 200.0,
                viewport_height: 600.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(600.0),
            },
            arena,
        );
    });

    arena.with_element_taken_ref(root, |el, arena| {
        let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
        let (_, caret_y, caret_h) = text_area.caret_screen_position(arena).expect("caret");
        assert_eq!(text_area.viewport_size.height, 35.0);
        assert!(text_area.scroll_y > 0.0);
        assert!(caret_y + caret_h <= 35.5);
    });
}

#[test]
fn parent_relayout_scrolls_viewport_down_after_cursor_move() {
    let content = "one\ntwo\nthree\nfour\nfive";
    let mut text_area = TextArea::new();
    text_area.content = content.to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;

    let mut arena = crate::view::test_support::new_test_arena();
    let mut parent = crate::view::base_component::Element::new(0.0, 0.0, 200.0, 35.0);
    let mut parent_style = crate::style::Style::new();
    parent_style.insert(
        crate::style::PropertyId::ScrollDirection,
        crate::style::ParsedValue::ScrollDirection(crate::style::ScrollDirection::Vertical),
    );
    parent_style.insert(
        crate::style::PropertyId::Layout,
        crate::style::ParsedValue::Layout(
            crate::style::Layout::flow().column().no_wrap().into(),
        ),
    );
    parent.apply_style(parent_style);
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(parent) as Box<dyn ElementTrait>,
    );
    let spacer = crate::view::base_component::Element::new(0.0, 0.0, 200.0, 70.0);
    crate::view::test_support::commit_child(&mut arena, root, Box::new(spacer));
    let text_area_key =
        crate::view::test_support::commit_child(&mut arena, root, Box::new(text_area));
    arena.with_element_taken(text_area_key, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea child")
            .set_self_node_key(text_area_key);
    });

    crate::view::test_support::measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 200.0,
            max_height: 35.0,
            viewport_width: 200.0,
            viewport_height: 35.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(35.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 35.0,
            viewport_width: 200.0,
            viewport_height: 35.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(35.0),
        },
    );

    arena.with_element_taken(text_area_key, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea child")
            .move_cursor_to(content.chars().count());
    });
    arena.refresh_subtree_dirty_cache(root);
    crate::view::test_support::measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 200.0,
            max_height: 35.0,
            viewport_width: 200.0,
            viewport_height: 35.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(35.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 35.0,
            viewport_width: 200.0,
            viewport_height: 35.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(35.0),
        },
    );

    arena.with_element_taken_ref(text_area_key, |el, arena| {
        let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
        let (_, caret_y, caret_h) = text_area.caret_screen_position(arena).expect("caret");
        let viewport_bottom =
            text_area.layout_state.layout_position.y + text_area.viewport_size.height;
        assert!(caret_y + caret_h <= viewport_bottom + 0.5);
    });
}

#[test]
fn parent_relayout_scrolls_viewport_down_after_text_insert() {
    let mut text_area = TextArea::new();
    text_area.content = "one\ntwo\nthree".to_string();
    text_area.cursor_char = text_area.content.chars().count();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;

    let mut arena = crate::view::test_support::new_test_arena();
    let parent = crate::view::base_component::Element::new(0.0, 0.0, 200.0, 35.0);
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(parent) as Box<dyn ElementTrait>,
    );
    let text_area_key =
        crate::view::test_support::commit_child(&mut arena, root, Box::new(text_area));
    arena.with_element_taken(text_area_key, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea child")
            .set_self_node_key(text_area_key);
    });

    let constraints = LayoutConstraints {
        max_width: 200.0,
        max_height: 35.0,
        viewport_width: 200.0,
        viewport_height: 35.0,
        percent_base_width: Some(200.0),
        percent_base_height: Some(35.0),
    };
    let placement = LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 200.0,
        available_height: 35.0,
        viewport_width: 200.0,
        viewport_height: 35.0,
        percent_base_width: Some(200.0),
        percent_base_height: Some(35.0),
    };
    crate::view::test_support::measure_and_place(&mut arena, root, constraints, placement);

    arena.with_element_taken(text_area_key, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea child")
            .insert_text("\nfour\nfive");
    });
    arena.refresh_subtree_dirty_cache(root);
    crate::view::test_support::measure_and_place(&mut arena, root, constraints, placement);

    arena.with_element_taken_ref(text_area_key, |el, arena| {
        let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
        let (_, caret_y, caret_h) = text_area.caret_screen_position(arena).expect("caret");
        let viewport_bottom =
            text_area.layout_state.layout_position.y + text_area.viewport_size.height;

        assert!(caret_y + caret_h <= viewport_bottom + 0.5);
    });
}

#[test]
fn parent_relayout_scrolls_viewport_down_after_repeated_cursor_moves() {
    let content = "one\ntwo\nthree\nfour\nfive";
    let mut text_area = TextArea::new();
    text_area.content = content.to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;

    let mut arena = crate::view::test_support::new_test_arena();
    let parent = crate::view::base_component::Element::new(0.0, 0.0, 200.0, 35.0);
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(parent) as Box<dyn ElementTrait>,
    );
    let text_area_key =
        crate::view::test_support::commit_child(&mut arena, root, Box::new(text_area));
    arena.with_element_taken(text_area_key, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea child")
            .set_self_node_key(text_area_key);
    });

    let constraints = LayoutConstraints {
        max_width: 200.0,
        max_height: 35.0,
        viewport_width: 200.0,
        viewport_height: 35.0,
        percent_base_width: Some(200.0),
        percent_base_height: Some(35.0),
    };
    let placement = LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 200.0,
        available_height: 35.0,
        viewport_width: 200.0,
        viewport_height: 35.0,
        percent_base_width: Some(200.0),
        percent_base_height: Some(35.0),
    };
    crate::view::test_support::measure_and_place(&mut arena, root, constraints, placement);

    let line_ends = [
        "one".chars().count(),
        "one\ntwo".chars().count(),
        "one\ntwo\nthree".chars().count(),
        "one\ntwo\nthree\nfour".chars().count(),
        content.chars().count(),
    ];
    for cursor in line_ends {
        arena.with_element_taken(text_area_key, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea child")
                .move_cursor_to(cursor);
        });
        arena.refresh_subtree_dirty_cache(root);
        crate::view::test_support::measure_and_place(&mut arena, root, constraints, placement);
    }

    arena.with_element_taken_ref(text_area_key, |el, arena| {
        let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
        let (_, caret_y, caret_h) = text_area.caret_screen_position(arena).expect("caret");
        let viewport_bottom =
            text_area.layout_state.layout_position.y + text_area.viewport_size.height;

        assert_eq!(text_area.cursor_char, content.chars().count());
        assert!(caret_y + caret_h <= viewport_bottom + 0.5);
    });
}

#[test]
fn caret_follow_scrolls_vertical_parent_to_caret() {
    let content = "one\ntwo\nthree\nfour\nfive";
    let mut text_area = TextArea::new();
    text_area.content = content.to_string();
    text_area.cursor_char = content.chars().count();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.pending_caret_scroll = true;

    let mut arena = crate::view::test_support::new_test_arena();
    let mut parent = crate::view::base_component::Element::new(0.0, 0.0, 200.0, 35.0);
    let mut parent_style = crate::style::Style::new();
    parent_style.insert(
        crate::style::PropertyId::ScrollDirection,
        crate::style::ParsedValue::ScrollDirection(crate::style::ScrollDirection::Vertical),
    );
    parent_style.insert(
        crate::style::PropertyId::Layout,
        crate::style::ParsedValue::Layout(crate::style::Layout::flex().column().into()),
    );
    parent.apply_style(parent_style);
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(parent) as Box<dyn ElementTrait>,
    );
    let spacer = crate::view::base_component::Element::new(0.0, 0.0, 200.0, 70.0);
    crate::view::test_support::commit_child(&mut arena, root, Box::new(spacer));
    let text_area_key =
        crate::view::test_support::commit_child(&mut arena, root, Box::new(text_area));
    arena.with_element_taken(text_area_key, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea child")
            .set_self_node_key(text_area_key);
    });

    let constraints = LayoutConstraints {
        max_width: 200.0,
        max_height: 35.0,
        viewport_width: 200.0,
        viewport_height: 35.0,
        percent_base_width: Some(200.0),
        percent_base_height: Some(35.0),
    };
    let placement = LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 200.0,
        available_height: 35.0,
        viewport_width: 200.0,
        viewport_height: 35.0,
        percent_base_width: Some(200.0),
        percent_base_height: Some(35.0),
    };

    crate::view::test_support::measure_and_place(&mut arena, root, constraints, placement);
    arena.with_element_taken(text_area_key, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea child")
            .move_cursor_to(content.chars().count());
    });
    arena.with_element_taken(root, |el, _| {
        el.as_any_mut()
            .downcast_mut::<crate::view::base_component::Element>()
            .expect("parent")
            .layout_state
            .content_size
            .height = 160.0;
    });
    arena.with_element_taken(text_area_key, |el, arena| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea child")
            .scroll_caret_into_view(arena);
    });
    let parent_scroll = arena
        .with_element_taken_ref(root, |el, _| el.get_scroll_offset())
        .expect("parent scroll");
    assert!(parent_scroll.1 > 0.0, "parent_scroll={parent_scroll:?}");

    arena.with_element_taken(root, |el, arena| {
        el.place(placement, arena);
    });
    arena.with_element_taken_ref(text_area_key, |el, arena| {
        let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
        let (_, caret_y, caret_h) = text_area.caret_screen_position(arena).expect("caret");
        assert!(caret_y >= -0.5);
        assert!(caret_y + caret_h <= 35.5);
    });
}

#[test]
fn caret_follow_scrolls_horizontal_parent_to_caret() {
    let content = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";
    let mut text_area = TextArea::new();
    text_area.content = content.to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.auto_wrap = false;

    let mut arena = crate::view::test_support::new_test_arena();
    let mut parent = crate::view::base_component::Element::new(0.0, 0.0, 80.0, 40.0);
    let mut parent_style = crate::style::Style::new();
    parent_style.insert(
        crate::style::PropertyId::ScrollDirection,
        crate::style::ParsedValue::ScrollDirection(crate::style::ScrollDirection::Horizontal),
    );
    parent_style.insert(
        crate::style::PropertyId::Layout,
        crate::style::ParsedValue::Layout(crate::style::Layout::flex().row().into()),
    );
    parent.apply_style(parent_style);
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(parent) as Box<dyn ElementTrait>,
    );
    let spacer = crate::view::base_component::Element::new(0.0, 0.0, 120.0, 40.0);
    crate::view::test_support::commit_child(&mut arena, root, Box::new(spacer));
    let text_area_key =
        crate::view::test_support::commit_child(&mut arena, root, Box::new(text_area));
    arena.with_element_taken(text_area_key, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea child")
            .set_self_node_key(text_area_key);
    });

    let constraints = LayoutConstraints {
        max_width: 80.0,
        max_height: 40.0,
        viewport_width: 80.0,
        viewport_height: 40.0,
        percent_base_width: Some(80.0),
        percent_base_height: Some(40.0),
    };
    let placement = LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 80.0,
        available_height: 40.0,
        viewport_width: 80.0,
        viewport_height: 40.0,
        percent_base_width: Some(80.0),
        percent_base_height: Some(40.0),
    };

    crate::view::test_support::measure_and_place(&mut arena, root, constraints, placement);
    arena.with_element_taken(root, |el, _| {
        el.as_any_mut()
            .downcast_mut::<crate::view::base_component::Element>()
            .expect("parent")
            .layout_state
            .content_size
            .width = 280.0;
    });
    crate::view::viewport::dispatch::scroll_rect_into_view_from(
        &arena,
        text_area_key,
        crate::ui::Rect::new(240.0, 0.0, 1.0, 18.0),
        crate::ui::ScrollIntoViewOptions::default(),
        false,
        true,
    );
    let parent_scroll = arena
        .with_element_taken_ref(root, |el, _| el.get_scroll_offset())
        .expect("parent scroll");
    assert!(parent_scroll.0 > 0.0, "parent_scroll={parent_scroll:?}");
    assert!(240.0 - parent_scroll.0 >= -0.5);
    assert!(241.0 - parent_scroll.0 <= 80.5);
}

#[test]
fn place_scrolls_viewport_right_to_caret_when_nowrap() {
    let content = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";
    let (arena, root) = placed_text_area(content, content.chars().count(), 80.0, 40.0, false);

    arena.with_element_taken_ref(root, |el, arena| {
        let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
        let (caret_x, _, _) = text_area.caret_screen_position(arena).expect("caret");
        let viewport_right =
            text_area.layout_state.layout_position.x + text_area.viewport_size.width;

        assert!(text_area.scroll_x > 0.0);
        assert!(caret_x + 1.0 <= viewport_right + 0.5);
    });
}
