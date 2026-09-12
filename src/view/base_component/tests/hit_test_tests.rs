use super::*;

#[test]
fn hit_test_allows_absolute_viewport_clip_outside_parent() {
    let mut root = Element::new(0.0, 0.0, 400.0, 300.0);
    root.set_background_color_value(Color::rgb(16, 16, 16));
    let parent = Element::new(0.0, 0.0, 100.0, 80.0);
    let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#ff0000")),
    );
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(130.0))
                .top(Length::px(10.0))
                .clip(ClipMode::Viewport),
        ),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let root_key = commit_element(&mut arena, Box::new(root));
    let parent_key = commit_child(&mut arena, root_key, Box::new(parent));
    let child_key = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        root_key,
        constraints(400.0, 300.0),
        placement(400.0, 300.0),
    );

    assert_eq!(hit_test(&arena, root_key, 135.0, 15.0), Some(child_key));
}

#[test]
fn hit_test_maps_points_through_translated_parent_transform() {
    let root = Element::new(0.0, 0.0, 400.0, 300.0);
    let mut parent = Element::new(0.0, 0.0, 100.0, 100.0);
    let mut parent_style = Style::new();
    parent_style.set_transform(Transform::new([Translate::x(Length::px(100.0))]));
    parent.apply_style(parent_style);

    let mut child = Element::new(10.0, 10.0, 20.0, 20.0);
    child.set_background_color_value(Color::rgb(255, 0, 0));

    let mut arena = new_test_arena();
    let root_key = commit_element(&mut arena, Box::new(root));
    let parent_key = commit_child(&mut arena, root_key, Box::new(parent));
    let child_key = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        root_key,
        constraints(400.0, 300.0),
        placement(400.0, 300.0),
    );

    assert_eq!(hit_test(&arena, root_key, 115.0, 15.0), Some(child_key));
}

#[test]
fn hit_test_maps_points_through_rotated_parent_transform() {
    let root = Element::new(0.0, 0.0, 400.0, 300.0);
    let mut parent = Element::new(0.0, 0.0, 100.0, 100.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    parent_style.set_transform(Transform::new([Rotate::z(Angle::deg(90.0))]));
    parent_style.set_transform_origin(TransformOrigin::center());
    parent.apply_style(parent_style);

    let mut child = Element::new(70.0, 10.0, 20.0, 20.0);
    child.set_background_color_value(Color::rgb(255, 0, 0));

    let mut arena = new_test_arena();
    let root_key = commit_element(&mut arena, Box::new(root));
    let parent_key = commit_child(&mut arena, root_key, Box::new(parent));
    let child_key = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        root_key,
        constraints(400.0, 300.0),
        placement(400.0, 300.0),
    );

    assert_eq!(hit_test(&arena, root_key, 80.0, 80.0), Some(child_key));
}

#[test]
fn hit_test_allows_absolute_viewport_clip_when_parent_not_rendered() {
    let mut root = Element::new(0.0, 0.0, 400.0, 300.0);
    root.set_anchor_name(Some(AnchorName::new("root_anchor")));
    root.set_background_color_value(Color::rgb(16, 16, 16));
    let mut parent = Element::new(0.0, 0.0, 100.0, 80.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(500.0))
                .top(Length::px(0.0))
                .clip(ClipMode::Parent),
        ),
    );
    parent.apply_style(parent_style);
    let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#ff0000")),
    );
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(130.0))
                .top(Length::px(10.0))
                .anchor("root_anchor")
                .clip(ClipMode::Viewport),
        ),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let root_key = commit_element(&mut arena, Box::new(root));
    let parent_key = commit_child(&mut arena, root_key, Box::new(parent));
    let child_key = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        root_key,
        constraints(400.0, 300.0),
        placement(400.0, 300.0),
    );

    assert_eq!(hit_test(&arena, root_key, 135.0, 15.0), Some(child_key));
}

#[test]
fn hit_test_blocks_absolute_parent_clip_outside_parent() {
    let root = Element::new(0.0, 0.0, 400.0, 300.0);
    let parent = Element::new(0.0, 0.0, 100.0, 80.0);
    let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(130.0))
                .top(Length::px(10.0))
                .clip(ClipMode::Parent),
        ),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let root_key = commit_element(&mut arena, Box::new(root));
    let parent_key = commit_child(&mut arena, root_key, Box::new(parent));
    let child_key = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        root_key,
        constraints(400.0, 300.0),
        placement(400.0, 300.0),
    );

    assert_ne!(hit_test(&arena, root_key, 135.0, 15.0), Some(child_key));
}

#[test]
fn hit_test_prefers_scrollbar_over_children() {
    let mut root = Element::new(0.0, 0.0, 120.0, 120.0);
    let mut root_style = Style::new();
    root_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#101010")),
    );
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root.apply_style(root_style);
    let mut child = Element::new(0.0, 0.0, 120.0, 360.0);
    child.set_background_color_value(Color::rgb(255, 0, 0));

    let mut arena = new_test_arena();
    let root_key = commit_element(&mut arena, Box::new(root));
    let _child_key = commit_child(&mut arena, root_key, Box::new(child));

    measure_and_place(
        &mut arena,
        root_key,
        constraints(120.0, 120.0),
        placement(120.0, 120.0),
    );
    arena.with_element_taken(root_key, |el, _a| {
        if let Some(e) = el.as_any_mut().downcast_mut::<Element>() {
            let _ = e.set_hovered(true);
        }
    });

    assert_eq!(hit_test(&arena, root_key, 115.0, 60.0), Some(root_key));
}

#[test]
fn overflow_child_hit_bubbles_but_parent_is_not_targetable_outside_clip() {
    let mut root = Element::new(0.0, 0.0, 200.0, 160.0);
    root.set_background_color_value(Color::rgb(16, 16, 16));
    let mut clip_parent = Element::new(0.0, 0.0, 100.0, 80.0);
    clip_parent.set_background_color_value(Color::rgb(32, 32, 32));
    let mut parent = Element::new(0.0, 0.0, 100.0, 80.0);
    let parent_clicks = Rc::new(Cell::new(0));
    let parent_clicks_binding = parent_clicks.clone();
    parent.on_click(move |_event, _control| {
        parent_clicks_binding.set(parent_clicks_binding.get() + 1);
    });
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(50.0))
                .top(Length::px(0.0))
                .clip(ClipMode::Parent),
        ),
    );
    parent.apply_style(parent_style);

    let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
    let child_clicks = Rc::new(Cell::new(0));
    let child_clicks_binding = child_clicks.clone();
    child.on_click(move |_event, _control| {
        child_clicks_binding.set(child_clicks_binding.get() + 1);
    });
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#ff0000")),
    );
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(60.0))
                .top(Length::px(10.0))
                .clip(ClipMode::Viewport),
        ),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let root_key = commit_element(&mut arena, Box::new(root));
    let clip_parent_key = commit_child(&mut arena, root_key, Box::new(clip_parent));
    let parent_key = commit_child(&mut arena, clip_parent_key, Box::new(parent));
    let child_key = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        root_key,
        constraints(200.0, 160.0),
        placement(200.0, 160.0),
    );

    assert_eq!(hit_test(&arena, root_key, 115.0, 15.0), Some(child_key));
    assert_eq!(hit_test(&arena, root_key, 145.0, 15.0), Some(root_key));

    let mut viewport = Viewport::new();
    let mut control = ViewportControl::new(&mut viewport);
    let mut click_child = ClickEvent {
        meta: EventMeta::new(NodeId::default()),
        pointer: PointerEventData {
            viewport_x: 115.0,
            viewport_y: 15.0,
            local_x: 0.0,
            local_y: 0.0,
            button: Some(PointerButton::Left),
            buttons: PointerButtons::default(),
            modifiers: Modifiers::default(),
            pointer_id: 0,
            pointer_type: crate::platform::input::PointerType::Mouse,
            pressure: 0.0,
            timestamp: crate::time::Instant::now(),
        },
        click_count: 1,
    };
    assert!(dispatch_click_from_hit_test(
        &mut arena,
        root_key,
        &mut click_child,
        &mut control
    ));
    assert_eq!(child_clicks.get(), 1);
    assert_eq!(parent_clicks.get(), 1);

    let mut click_outside = ClickEvent {
        meta: EventMeta::new(NodeId::default()),
        pointer: PointerEventData {
            viewport_x: 145.0,
            viewport_y: 15.0,
            local_x: 0.0,
            local_y: 0.0,
            button: Some(PointerButton::Left),
            buttons: PointerButtons::default(),
            modifiers: Modifiers::default(),
            pointer_id: 0,
            pointer_type: crate::platform::input::PointerType::Mouse,
            pressure: 0.0,
            timestamp: crate::time::Instant::now(),
        },
        click_count: 1,
    };
    let _ = dispatch_click_from_hit_test(&mut arena, root_key, &mut click_outside, &mut control);
    assert_eq!(child_clicks.get(), 1);
    assert_eq!(parent_clicks.get(), 1);
}

#[test]
fn hit_test_roots_respects_later_root_over_anchor_parent_overflow_handle() {
    let mut lower_root = Element::new(0.0, 0.0, 100.0, 80.0);
    lower_root.set_background_color_value(Color::rgb(16, 16, 16));
    let mut handle = Element::new(0.0, 0.0, 4.0, 80.0);
    let mut handle_style = Style::new();
    handle_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#ff0000")),
    );
    handle_style.insert(
        PropertyId::Cursor,
        ParsedValue::Cursor(crate::style::Cursor::EwResize),
    );
    handle_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .right(Length::px(-2.0))
                .top(Length::px(0.0))
                .clip(ClipMode::AnchorParent),
        ),
    );
    handle.apply_style(handle_style);

    let mut higher_root = Element::new(50.0, 0.0, 100.0, 80.0);
    higher_root.set_background_color_value(Color::rgb(32, 32, 32));

    let mut arena = new_test_arena();
    let lower_key = commit_element(&mut arena, Box::new(lower_root));
    let handle_key = commit_child(&mut arena, lower_key, Box::new(handle));
    let higher_key = commit_element(&mut arena, Box::new(higher_root));

    let root_keys = [lower_key, higher_key];
    for &root_key in &root_keys {
        measure_and_place(
            &mut arena,
            root_key,
            constraints(200.0, 160.0),
            placement(200.0, 160.0),
        );
    }

    assert_eq!(hit_test(&arena, lower_key, 101.0, 20.0), Some(handle_key));
    assert_eq!(
        hit_test_roots(&arena, &root_keys, 101.0, 20.0),
        Some((1, higher_key)),
        "root children follow sibling stacking; an earlier root's overflow handle is not a top layer"
    );
}

#[test]
fn hit_test_window_like_anchor_parent_resize_handles_all_edges() {
    let mut root = Element::new(0.0, 0.0, 100.0, 80.0);
    let mut root_style = Style::new();
    root_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flow().column().into()),
    );
    root_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(20.0))
                .top(Length::px(30.0)),
        ),
    );
    root.apply_style(root_style);

    let mut content = Element::new(0.0, 0.0, 100.0, 80.0);
    content.set_background_color_value(Color::rgb(32, 32, 32));

    fn resize_handle(position: Position, cursor: crate::style::Cursor) -> Element {
        let mut handle = Element::new(0.0, 0.0, 0.0, 0.0);
        let mut style = Style::new();
        style.insert(PropertyId::Position, ParsedValue::Position(position));
        style.insert(PropertyId::Cursor, ParsedValue::Cursor(cursor));
        match cursor {
            crate::style::Cursor::EwResize => {
                style.insert(PropertyId::Width, ParsedValue::Length(Length::px(4.0)));
            }
            crate::style::Cursor::NsResize => {
                style.insert(PropertyId::Height, ParsedValue::Length(Length::px(4.0)));
            }
            _ => {}
        }
        handle.apply_style(style);
        handle
    }

    let left = resize_handle(
        Position::absolute()
            .left(Length::px(-2.0))
            .top(Length::px(0.0))
            .bottom(Length::px(0.0))
            .clip(ClipMode::AnchorParent),
        crate::style::Cursor::EwResize,
    );
    let right = resize_handle(
        Position::absolute()
            .right(Length::px(-2.0))
            .top(Length::px(0.0))
            .bottom(Length::px(0.0))
            .clip(ClipMode::AnchorParent),
        crate::style::Cursor::EwResize,
    );
    let top = resize_handle(
        Position::absolute()
            .left(Length::px(0.0))
            .right(Length::px(0.0))
            .top(Length::px(-2.0))
            .clip(ClipMode::AnchorParent),
        crate::style::Cursor::NsResize,
    );
    let bottom = resize_handle(
        Position::absolute()
            .left(Length::px(0.0))
            .right(Length::px(0.0))
            .bottom(Length::px(-2.0))
            .clip(ClipMode::AnchorParent),
        crate::style::Cursor::NsResize,
    );

    let mut arena = new_test_arena();
    let root_key = commit_element(&mut arena, Box::new(root));
    let _content_key = commit_child(&mut arena, root_key, Box::new(content));
    let left_key = commit_child(&mut arena, root_key, Box::new(left));
    let right_key = commit_child(&mut arena, root_key, Box::new(right));
    let top_key = commit_child(&mut arena, root_key, Box::new(top));
    let bottom_key = commit_child(&mut arena, root_key, Box::new(bottom));

    measure_and_place(
        &mut arena,
        root_key,
        constraints(200.0, 160.0),
        placement(200.0, 160.0),
    );

    let left_snapshot = arena
        .get(left_key)
        .expect("left handle")
        .element
        .box_model_snapshot();
    let right_snapshot = arena
        .get(right_key)
        .expect("right handle")
        .element
        .box_model_snapshot();
    let top_snapshot = arena
        .get(top_key)
        .expect("top handle")
        .element
        .box_model_snapshot();
    let bottom_snapshot = arena
        .get(bottom_key)
        .expect("bottom handle")
        .element
        .box_model_snapshot();
    assert_eq!(
        (left_snapshot.width, left_snapshot.height),
        (4.0, 80.0),
        "left edge snapshot should use the placed frame size"
    );
    assert_eq!(
        (right_snapshot.width, right_snapshot.height),
        (4.0, 80.0),
        "right edge snapshot should use the placed frame size"
    );
    assert_eq!(
        (top_snapshot.width, top_snapshot.height),
        (100.0, 4.0),
        "top edge snapshot should use the placed frame size"
    );
    assert_eq!(
        (bottom_snapshot.width, bottom_snapshot.height),
        (100.0, 4.0),
        "bottom edge snapshot should use the placed frame size"
    );

    assert_eq!(hit_test(&arena, root_key, 19.0, 50.0), Some(left_key));
    assert_eq!(hit_test(&arena, root_key, 121.0, 50.0), Some(right_key));
    assert_eq!(hit_test(&arena, root_key, 50.0, 29.0), Some(top_key));
    assert_eq!(hit_test(&arena, root_key, 50.0, 111.0), Some(bottom_key));
}
