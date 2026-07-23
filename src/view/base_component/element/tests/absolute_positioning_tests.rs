use super::*;

#[test]
fn absolute_defaults_to_parent_anchor_and_zero_insets() {
    let parent = Element::new(40.0, 60.0, 200.0, 120.0);
    let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(Position::absolute()),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

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
    assert_eq!(snapshot.x, 40.0);
    assert_eq!(snapshot.y, 60.0);
}

#[test]
fn absolute_stretch_with_left_right_top_bottom() {
    let parent = Element::new(10.0, 20.0, 200.0, 120.0);
    let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(10.0))
                .right(Length::px(20.0))
                .top(Length::px(5.0))
                .bottom(Length::px(15.0)),
        ),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

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
    assert_eq!(snapshot.x, 20.0);
    assert_eq!(snapshot.y, 25.0);
    assert_eq!(snapshot.width, 170.0);
    assert_eq!(snapshot.height, 100.0);
}

#[test]
fn absolute_negative_insets_are_preserved() {
    let parent = Element::new(10.0, 20.0, 200.0, 120.0);
    let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(-10.0))
                .right(Length::px(20.0))
                .top(Length::px(-5.0))
                .bottom(Length::px(15.0)),
        ),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

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
    assert_eq!(snapshot.y, 15.0);
    assert_eq!(snapshot.width, 190.0);
    assert_eq!(snapshot.height, 110.0);
}

#[test]
fn absolute_self_origin_center_centers_on_inset_point() {
    let parent = Element::new(0.0, 0.0, 400.0, 300.0);
    let mut child = Element::new(0.0, 0.0, 40.0, 40.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(100.0))
                .top(Length::px(100.0))
                .origin(Origin::center()),
        ),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

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
    assert_eq!(snapshot.x, 80.0);
    assert_eq!(snapshot.y, 80.0);
    assert_eq!(snapshot.width, 40.0);
    assert_eq!(snapshot.height, 40.0);
}

#[test]
fn absolute_self_origin_bottom_right_aligns_to_anchor_corner() {
    let parent = Element::new(0.0, 0.0, 400.0, 300.0);
    let mut child = Element::new(0.0, 0.0, 50.0, 30.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(0.0))
                .top(Length::px(0.0))
                .origin(Origin::bottom_right()),
        ),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

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
    assert_eq!(snapshot.x, -50.0);
    assert_eq!(snapshot.y, -30.0);
}

#[test]
fn absolute_self_origin_px_offset() {
    let parent = Element::new(0.0, 0.0, 400.0, 300.0);
    let mut child = Element::new(0.0, 0.0, 60.0, 40.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(100.0))
                .top(Length::px(100.0))
                .origin(Origin::px(20.0, 30.0)),
        ),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

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
    assert_eq!(snapshot.x, 80.0);
    assert_eq!(snapshot.y, 70.0);
}

#[test]
fn absolute_self_origin_top_center_for_popover_pattern() {
    // Popover anchored to parent bottom-center: top: 100%, left: 50%,
    // origin: top_center → self top edge centered at parent's bottom-center.
    let parent = Element::new(0.0, 0.0, 200.0, 120.0);
    let mut child = Element::new(0.0, 0.0, 80.0, 50.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::percent(50.0))
                .top(Length::percent(100.0))
                .origin(Origin::top_center()),
        ),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

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
    // placement point = (50%, 100%) of parent = (100, 120)
    // self top-left = placement - (50%, 0%) of self = (100-40, 120-0) = (60, 120)
    assert_eq!(snapshot.x, 60.0);
    assert_eq!(snapshot.y, 120.0);
}

#[test]
fn absolute_self_origin_with_auto_size_via_child() {
    // Mirror tooltip pattern: absolute element with Auto width/height,
    // size determined by a fixed-size child after measure pass. Origin
    // shift must use the post-measure auto-size, not 0.
    let parent = Element::new(0.0, 0.0, 200.0, 60.0);
    let mut tooltip_box = Element::new(0.0, 0.0, 0.0, 0.0);
    let mut tooltip_style = Style::new();
    tooltip_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flow().row().no_wrap().into()),
    );
    tooltip_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::calc(
                    Length::percent(100.0),
                    Operator::plus,
                    Length::px(6.0),
                ))
                .top(Length::percent(50.0))
                .origin(Origin::center_left()),
        ),
    );
    tooltip_box.apply_style(tooltip_style);

    // Fixed-size grand-child standing in for the tooltip's text.
    let text_child = Element::new(0.0, 0.0, 80.0, 20.0);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let tooltip_key = commit_child(&mut arena, parent_key, Box::new(tooltip_box));
    let _text_key = commit_child(&mut arena, tooltip_key, Box::new(text_child));

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

    // parent 200x60. tooltip auto-sized to ~80x20 from text child.
    // left = 100% + 6 → tooltip.x = 200 + 6 = 206
    // top = 50% → 30; minus origin y (50% of 20 = 10) → tooltip.y = 20
    let snapshot = nth_child_snapshot(&arena, parent_key, 0);
    assert_eq!(snapshot.x, 206.0);
    assert_eq!(snapshot.width, 80.0);
    assert_eq!(snapshot.height, 20.0);
    assert_eq!(snapshot.y, 20.0);
}

#[test]
fn absolute_self_origin_left_placement_with_right_inset() {
    // Tooltip Left placement: right inset + origin center_left.
    // Right inset already shifts by self_w; origin x=0 leaves x alone,
    // origin y=50% centers vertically.
    let parent = Element::new(100.0, 100.0, 60.0, 30.0);
    let mut tooltip_box = Element::new(0.0, 0.0, 0.0, 0.0);
    let mut tooltip_style = Style::new();
    tooltip_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flow().row().no_wrap().into()),
    );
    tooltip_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .right(Length::calc(
                    Length::percent(100.0),
                    Operator::plus,
                    Length::px(6.0),
                ))
                .top(Length::percent(50.0))
                .origin(Origin::center_left()),
        ),
    );
    tooltip_box.apply_style(tooltip_style);
    let text_child = Element::new(0.0, 0.0, 80.0, 20.0);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let tooltip_key = commit_child(&mut arena, parent_key, Box::new(tooltip_box));
    let _text_key = commit_child(&mut arena, tooltip_key, Box::new(text_child));

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
    assert_eq!(snapshot.width, 80.0);
    assert_eq!(snapshot.height, 20.0);
    // anchor (parent) = 100x100..160x130.
    // right inset = 60+6 = 66 → target_rel_x = (100-0) + (60 - 66 - 80) = 100 - 86 = 14
    // tooltip right edge = 14 + 80 = 94 → anchor.left (100) - tooltip.right (94) = 6 = gap ✓
    assert_eq!(snapshot.x, 14.0);
    // top = 50% → target_rel_y = 100 + 15 = 115. origin oy = 10 → 105.
    // tooltip vertical center = 105 + 10 = 115 = anchor.y (100) + 0.5*30 = 115 ✓
    assert_eq!(snapshot.y, 105.0);
}

#[test]
fn relative_mode_ignores_self_origin() {
    let parent = Element::new(0.0, 0.0, 200.0, 120.0);
    let mut child = Element::new(0.0, 0.0, 40.0, 30.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(Position::relative().origin(Origin::center())),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

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
    // Relative element follows flow layout; origin must not shift it.
    assert_eq!(snapshot.x, 0.0);
    assert_eq!(snapshot.y, 0.0);
}

#[test]
fn absolute_collision_fit_viewport_clamps_into_view() {
    let mut el = Element::new(0.0, 0.0, 50.0, 30.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(390.0))
                .top(Length::px(295.0))
                .collision(Collision::Fit, CollisionBoundary::Viewport),
        ),
    );
    el.apply_style(style);

    let mut arena = new_test_arena();
    let key = commit_element(&mut arena, Box::new(el));
    measure_and_place(
        &mut arena,
        key,
        LayoutConstraints {
            max_width: 400.0,
            max_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 400.0,
            available_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        },
    );
    let snapshot = child_snapshot(&arena, key);
    assert_eq!(snapshot.x, 350.0);
    assert_eq!(snapshot.y, 270.0);
}
