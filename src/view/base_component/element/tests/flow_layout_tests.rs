use super::*;

#[test]
fn absolute_child_does_not_affect_auto_parent_size() {
    let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Width, ParsedValue::Auto);
    parent_style.insert(PropertyId::Height, ParsedValue::Auto);
    parent.apply_style(parent_style);

    let normal_child = Element::new(0.0, 0.0, 80.0, 40.0);
    let mut absolute_child = Element::new(0.0, 0.0, 300.0, 200.0);
    let mut absolute_style = Style::new();
    absolute_style.insert(
        PropertyId::Position,
        ParsedValue::Position(Position::absolute()),
    );
    absolute_child.apply_style(absolute_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _ = commit_child(&mut arena, parent_key, Box::new(normal_child));
    let _ = commit_child(&mut arena, parent_key, Box::new(absolute_child));

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        },
    );

    let snapshot = child_snapshot(&arena, parent_key);
    assert_eq!(snapshot.width, 80.0);
    assert_eq!(snapshot.height, 40.0);
}

#[test]
fn inline_measure_skips_absolute_child_for_remaining_width() {
    let mut parent = Element::new(0.0, 0.0, 200.0, 0.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Auto);
    parent.apply_style(parent_style);

    let leading = Element::new(0.0, 0.0, 190.0, 20.0);

    let mut popover = Element::new(0.0, 0.0, 0.0, 0.0);
    let mut popover_style = Style::new();
    popover_style.insert(PropertyId::Width, ParsedValue::Auto);
    popover_style.insert(PropertyId::Height, ParsedValue::Auto);
    popover_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flow().row().no_wrap().into()),
    );
    popover_style.insert(
        PropertyId::Position,
        ParsedValue::Position(Position::absolute().anchor(crate::style::Anchor::Viewport)),
    );
    popover.apply_style(popover_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _leading_key = commit_child(&mut arena, parent_key, Box::new(leading));
    let popover_key = commit_child(&mut arena, parent_key, Box::new(popover));
    let _popover_text_key = commit_child(
        &mut arena,
        popover_key,
        Box::new(Text::from_content("absolute snackbar message")),
    );

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 200.0,
            max_height: 200.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 200.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        },
    );

    let parent_snapshot = child_snapshot(&arena, parent_key);
    let popover_snapshot = nth_child_snapshot(&arena, parent_key, 1);

    assert!(
        popover_snapshot.width > 100.0,
        "absolute child should measure against the parent constraint, not the 10px inline remainder: {:?}",
        popover_snapshot
    );
    assert!(
        popover_snapshot.height < 40.0,
        "absolute child should not be made tall by remainder-width text wrapping: {:?}",
        popover_snapshot
    );
    assert_eq!(parent_snapshot.width, 200.0);
    assert_eq!(parent_snapshot.height, 20.0);
}

#[test]
fn inline_ifc_measure_clears_text_layout_dirty_without_plain_text_measure() {
    let mut parent = Element::new_with_id(9001, 0.0, 0.0, 160.0, 0.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(160.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Auto);
    parent.apply_style(parent_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let text_key = commit_child(
        &mut arena,
        parent_key,
        Box::new(Text::from_content_with_id(9002, "inline text")),
    );

    arena.with_element_taken(parent_key, |parent, arena| {
        parent.measure(
            LayoutConstraints {
                max_width: 160.0,
                max_height: 200.0,
                viewport_width: 160.0,
                viewport_height: 200.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(200.0),
            },
            arena,
        );
    });

    let text = crate::view::test_support::get_element::<Text>(&arena, text_key);
    assert!(
        !text.local_dirty_flags().intersects(DirtyFlags::LAYOUT),
        "inline IFC-owned Text should not keep parent layout gate dirty"
    );
}

#[test]
fn column_flow_auto_size_uses_cross_for_width_and_main_for_height() {
    let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flex().column().into()),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Auto);
    parent_style.insert(PropertyId::Height, ParsedValue::Auto);
    parent.apply_style(parent_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _ = commit_child(
        &mut arena,
        parent_key,
        Box::new(Element::new(0.0, 0.0, 80.0, 30.0)),
    );
    let _ = commit_child(
        &mut arena,
        parent_key,
        Box::new(Element::new(0.0, 0.0, 120.0, 10.0)),
    );

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        },
    );

    let snapshot = child_snapshot(&arena, parent_key);
    assert_eq!(snapshot.width, 120.0);
    assert_eq!(snapshot.height, 40.0);
}

#[test]
fn flow_align_centers_children_on_cross_axis() {
    let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flex().row().align(Align::Center).into()),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
    parent.apply_style(parent_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _ = commit_child(
        &mut arena,
        parent_key,
        Box::new(Element::new(0.0, 0.0, 80.0, 40.0)),
    );

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        },
    );

    let snapshot = nth_child_snapshot(&arena, parent_key, 0);
    assert_eq!(snapshot.x, 0.0);
    assert_eq!(snapshot.y, 40.0);
}

#[test]
fn flow_cross_size_stretch_skips_children_with_explicit_cross_size() {
    let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(
            Layout::flow()
                .row()
                .no_wrap()
                .cross_size(CrossSize::Stretch)
                .into(),
        ),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
    parent.apply_style(parent_style);

    let mut explicit_child = Element::new(0.0, 0.0, 80.0, 10.0);
    let mut explicit_child_style = Style::new();
    explicit_child_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(80.0)));
    explicit_child_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(10.0)));
    explicit_child.apply_style(explicit_child_style);

    let mut auto_child = Element::new(0.0, 0.0, 80.0, 40.0);
    let mut auto_child_style = Style::new();
    auto_child_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(80.0)));
    auto_child.apply_style(auto_child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _ = commit_child(&mut arena, parent_key, Box::new(explicit_child));
    let _ = commit_child(&mut arena, parent_key, Box::new(auto_child));

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        },
    );

    let explicit_snapshot = nth_child_snapshot(&arena, parent_key, 0);
    let auto_snapshot = nth_child_snapshot(&arena, parent_key, 1);

    assert_eq!(explicit_snapshot.height, 10.0);
    assert_eq!(auto_snapshot.height, 40.0);
}
