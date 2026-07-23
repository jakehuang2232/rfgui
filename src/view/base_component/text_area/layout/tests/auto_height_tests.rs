use super::*;

#[test]
fn auto_height_parent_grows_after_trailing_newline_insert() {
    let line_height = 14.0 * 1.25;
    let mut text_area = TextArea::new();
    text_area.content = "abc".to_string();
    text_area.cursor_char = text_area.content.chars().count();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;

    let mut parent = crate::view::base_component::Element::new(0.0, 0.0, 200.0, 0.0);
    let mut parent_style = crate::style::Style::new();
    parent_style.insert(
        crate::style::PropertyId::Layout,
        crate::style::ParsedValue::Layout(
            crate::style::Layout::flow().column().no_wrap().into(),
        ),
    );
    parent_style.insert(
        crate::style::PropertyId::Width,
        crate::style::ParsedValue::Length(crate::style::Length::px(200.0)),
    );
    parent_style.insert(
        crate::style::PropertyId::Height,
        crate::style::ParsedValue::Auto,
    );
    parent.apply_style(parent_style);

    let mut arena = crate::view::test_support::new_test_arena();
    let parent_key = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(parent) as Box<dyn ElementTrait>,
    );
    let text_area_key =
        crate::view::test_support::commit_child(&mut arena, parent_key, Box::new(text_area));
    arena.with_element_taken(text_area_key, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea child")
            .set_self_node_key(text_area_key);
    });

    let constraints = LayoutConstraints {
        max_width: 200.0,
        max_height: 600.0,
        viewport_width: 200.0,
        viewport_height: 600.0,
        percent_base_width: Some(200.0),
        percent_base_height: Some(600.0),
    };
    let placement = LayoutPlacement {
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
    };

    crate::view::test_support::measure_and_place(
        &mut arena,
        parent_key,
        constraints,
        placement,
    );
    let before = arena
        .get(parent_key)
        .expect("parent")
        .element
        .box_model_snapshot()
        .height;

    arena.with_element_taken(text_area_key, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea child")
            .insert_text("\n");
    });
    arena.refresh_subtree_dirty_cache(parent_key);
    crate::view::test_support::measure_and_place(
        &mut arena,
        parent_key,
        constraints,
        placement,
    );

    let after = arena
        .get(parent_key)
        .expect("parent")
        .element
        .box_model_snapshot()
        .height;
    assert!(
        after >= before + line_height - 0.5,
        "expected trailing newline to grow auto-height parent by about one line: before={before}, after={after}, line_height={line_height}",
    );
}

#[test]
fn auto_height_parent_grows_for_each_trailing_newline_insert() {
    let line_height = 14.0 * 1.25;
    let mut text_area = TextArea::new();
    text_area.content = "abc".to_string();
    text_area.cursor_char = text_area.content.chars().count();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;

    let mut parent = crate::view::base_component::Element::new(0.0, 0.0, 200.0, 0.0);
    let mut parent_style = crate::style::Style::new();
    parent_style.insert(
        crate::style::PropertyId::Layout,
        crate::style::ParsedValue::Layout(
            crate::style::Layout::flow().column().no_wrap().into(),
        ),
    );
    parent_style.insert(
        crate::style::PropertyId::Width,
        crate::style::ParsedValue::Length(crate::style::Length::px(200.0)),
    );
    parent_style.insert(
        crate::style::PropertyId::Height,
        crate::style::ParsedValue::Auto,
    );
    parent.apply_style(parent_style);

    let mut arena = crate::view::test_support::new_test_arena();
    let parent_key = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(parent) as Box<dyn ElementTrait>,
    );
    let text_area_key =
        crate::view::test_support::commit_child(&mut arena, parent_key, Box::new(text_area));
    arena.with_element_taken(text_area_key, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea child")
            .set_self_node_key(text_area_key);
    });

    let constraints = LayoutConstraints {
        max_width: 200.0,
        max_height: 600.0,
        viewport_width: 200.0,
        viewport_height: 600.0,
        percent_base_width: Some(200.0),
        percent_base_height: Some(600.0),
    };
    let placement = LayoutPlacement {
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
    };

    crate::view::test_support::measure_and_place(
        &mut arena,
        parent_key,
        constraints,
        placement,
    );
    let before = arena
        .get(parent_key)
        .expect("parent")
        .element
        .box_model_snapshot()
        .height;

    arena.with_element_taken(text_area_key, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea child")
            .insert_text("\n\n\n");
    });
    arena.refresh_subtree_dirty_cache(parent_key);
    crate::view::test_support::measure_and_place(
        &mut arena,
        parent_key,
        constraints,
        placement,
    );

    let after = arena
        .get(parent_key)
        .expect("parent")
        .element
        .box_model_snapshot()
        .height;
    assert!(
        after >= before + line_height * 3.0 - 0.5,
        "expected each trailing newline to grow auto-height parent: before={before}, after={after}, line_height={line_height}",
    );
    arena.with_element_taken_ref(text_area_key, |el, arena| {
        let text_area = el.as_any().downcast_ref::<TextArea>().expect("TextArea child");
        let (_, caret_y, caret_h) = text_area.caret_screen_position(arena).expect("caret");
        let text_area_bottom =
            text_area.layout_state.layout_position.y + text_area.layout_state.layout_size.height;
        assert!(
            caret_y + caret_h <= text_area_bottom + 0.5,
            "caret must fit inside auto-height TextArea: caret_y={caret_y}, caret_h={caret_h}, text_area_bottom={text_area_bottom}",
        );
    });
}
