use super::*;

#[test]
fn transition_override_keeps_inner_render_area_available() {
    let mut el = Element::new(0.0, 0.0, 20.0, 20.0);
    el.layout_state.layout_position = LayoutPosition { x: 0.0, y: 0.0 };
    el.layout_state.layout_size.width = 0.0;
    el.layout_state.layout_size.height = 0.0;
    el.layout_state.layout_inner_position = LayoutPosition { x: 0.0, y: 0.0 };
    el.layout_state.layout_inner_size.width = 0.0;
    el.layout_state.layout_inner_size.height = 0.0;
    el.layout_transition_override_width = Some(40.0);
    el.layout_transition_override_height = Some(30.0);

    assert!(el.has_inner_render_area());
    let transition_inner = el.transition_inner_rect();
    assert_eq!(transition_inner.width, 40.0);
    assert_eq!(transition_inner.height, 30.0);
    let inner = el.inner_clip_rect();
    assert_eq!(inner.width, 40.0);
    assert_eq!(inner.height, 30.0);
}

#[test]
fn box_model_snapshot_uses_active_layout_frame_size() {
    let mut el = Element::new(0.0, 0.0, 100.0, 80.0);
    el.layout_state.layout_position = LayoutPosition { x: 5.0, y: 7.0 };
    el.layout_state.layout_size.width = 100.0;
    el.layout_state.layout_size.height = 80.0;
    el.layout_transition_override_width = Some(48.0);
    el.layout_transition_override_height = Some(0.0);

    let snapshot = el.box_model_snapshot();
    assert_eq!(snapshot.x, 5.0);
    assert_eq!(snapshot.y, 7.0);
    assert_eq!(snapshot.width, 48.0);
    assert_eq!(snapshot.height, 0.0);
}

#[test]
fn box_model_snapshot_uses_rendered_size_without_polluting_layout_target() {
    let mut el = Element::new(0.0, 0.0, 100.0, 80.0);
    el.layout_state.layout_position = LayoutPosition { x: 5.0, y: 7.0 };
    el.layout_state.layout_size.width = 48.0;
    el.layout_state.layout_size.height = 30.0;

    let snapshot = el.box_model_snapshot();
    assert_eq!(snapshot.x, 5.0);
    assert_eq!(snapshot.y, 7.0);
    assert_eq!(snapshot.width, 48.0);
    assert_eq!(snapshot.height, 30.0);
    assert_eq!(el.layout_target_size(), (100.0, 80.0));
    assert_eq!(el.measured_size(), (100.0, 80.0));
}

#[test]
fn zero_height_layout_transition_still_clips_children() {
    let mut arena = new_test_arena();
    let mut parent = Element::new(0.0, 0.0, 100.0, 80.0);
    parent.layout_state.layout_position = LayoutPosition { x: 0.0, y: 0.0 };
    parent.layout_state.layout_size.width = 100.0;
    parent.layout_state.layout_size.height = 80.0;
    parent.layout_transition_override_width = Some(100.0);
    parent.layout_transition_override_height = Some(0.0);
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _ = commit_child(
        &mut arena,
        parent_key,
        Box::new(Element::new(0.0, 0.0, 40.0, 20.0)),
    );

    let parent = crate::view::test_support::get_element::<Element>(&arena, parent_key);
    assert!(!parent.has_inner_render_area());
    let inner_radii = parent.inner_clip_radii(normalize_corner_radii(
        parent.border_radii,
        parent.box_model_snapshot().width.max(0.0),
        parent.box_model_snapshot().height.max(0.0),
    ));
    assert!(parent.should_clip_children(&[false], inner_radii, &arena));
}

#[test]
fn child_hit_test_clip_uses_parent_transition_inner_size() {
    let mut arena = new_test_arena();
    let mut parent = Element::new(0.0, 0.0, 200.0, 100.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(100.0)));
    parent_style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(
            [
                Transition::new(TransitionProperty::Width, 200),
                Transition::new(TransitionProperty::Height, 200),
            ]
            .into(),
        ),
    );
    parent_style.set_border(Border::uniform(Length::px(10.0), &Color::hex("#000000")));
    parent.apply_style(parent_style);
    parent.set_padding_left(5.0);
    parent.set_padding_right(15.0);
    parent.set_padding_top(7.0);
    parent.set_padding_bottom(13.0);
    parent.layout_transition_override_width = Some(320.0);
    parent.layout_transition_override_height = Some(180.0);
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _ = commit_child(
        &mut arena,
        parent_key,
        Box::new(Element::new(0.0, 0.0, 40.0, 40.0)),
    );

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        },
    );

    let child_key = child_key(&arena, parent_key, 0);
    let child = crate::view::test_support::get_element::<Element>(&arena, child_key);
    let clip = child.hit_test_clip_rect.expect("hit-test clip");

    assert_eq!(clip.x, 15.0);
    assert_eq!(clip.y, 17.0);
    assert_eq!(clip.width, 280.0);
    assert_eq!(clip.height, 140.0);
}

#[test]
fn absolute_parent_clip_uses_parent_transition_inner_size() {
    let mut arena = new_test_arena();
    let mut parent = Element::new(0.0, 0.0, 200.0, 100.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(100.0)));
    parent_style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(
            [
                Transition::new(TransitionProperty::Width, 200),
                Transition::new(TransitionProperty::Height, 200),
            ]
            .into(),
        ),
    );
    parent_style.set_border(Border::uniform(Length::px(10.0), &Color::hex("#000000")));
    parent.apply_style(parent_style);
    parent.set_padding_left(5.0);
    parent.set_padding_right(15.0);
    parent.set_padding_top(7.0);
    parent.set_padding_bottom(13.0);
    parent.layout_transition_override_width = Some(320.0);
    parent.layout_transition_override_height = Some(180.0);
    let parent_key = commit_element(&mut arena, Box::new(parent));

    let mut child = Element::new(0.0, 0.0, 80.0, 40.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(Position::absolute().clip(ClipMode::Parent)),
    );
    child.apply_style(child_style);
    let _ = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        },
    );

    let ck = child_key(&arena, parent_key, 0);
    let child = crate::view::test_support::get_element::<Element>(&arena, ck);
    let clip = child.absolute_clip_rect.expect("absolute clip");

    assert_eq!(clip.x, 15.0);
    assert_eq!(clip.y, 17.0);
    assert_eq!(clip.width, 280.0);
    assert_eq!(clip.height, 140.0);
}

#[test]
fn anchor_parent_clip_uses_transitioning_parent_inner_size() {
    let mut arena = new_test_arena();
    let mut parent = Element::new(0.0, 0.0, 200.0, 100.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(100.0)));
    parent_style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(
            [
                Transition::new(TransitionProperty::Width, 200),
                Transition::new(TransitionProperty::Height, 200),
            ]
            .into(),
        ),
    );
    parent_style.set_border(Border::uniform(Length::px(10.0), &Color::hex("#000000")));
    parent.apply_style(parent_style);
    parent.set_padding_left(5.0);
    parent.set_padding_right(15.0);
    parent.set_padding_top(7.0);
    parent.set_padding_bottom(13.0);
    parent.layout_transition_override_width = Some(320.0);
    parent.layout_transition_override_height = Some(180.0);
    let parent_key = commit_element(&mut arena, Box::new(parent));

    let mut anchor = Element::new(30.0, 20.0, 40.0, 20.0);
    anchor.set_anchor_name(Some(AnchorName::new("menu_button")));
    let _ = commit_child(&mut arena, parent_key, Box::new(anchor));

    let mut child = Element::new(0.0, 0.0, 80.0, 40.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .anchor("menu_button")
                .left(Length::px(10.0))
                .top(Length::px(0.0))
                .clip(ClipMode::AnchorParent),
        ),
    );
    child.apply_style(child_style);
    let _ = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        },
    );

    let ck = child_key(&arena, parent_key, 1);
    let child = crate::view::test_support::get_element::<Element>(&arena, ck);
    let clip = child.absolute_clip_rect.expect("absolute clip");

    assert_eq!(clip.x, 15.0);
    assert_eq!(clip.y, 17.0);
    assert_eq!(clip.width, 280.0);
    assert_eq!(clip.height, 140.0);
}

#[test]
fn anchored_absolute_child_uses_anchor_visual_position_during_transition() {
    let mut arena = new_test_arena();
    let mut parent = Element::new(0.0, 0.0, 500.0, 200.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(500.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(200.0)));
    parent.apply_style(parent_style);
    let parent_key = commit_element(&mut arena, Box::new(parent));

    let mut anchor = Element::new(300.0, 20.0, 40.0, 20.0);
    let mut anchor_style = Style::new();
    anchor_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(300.0))
                .top(Length::px(20.0)),
        ),
    );
    anchor_style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(Transition::new(
            TransitionProperty::Position,
            200,
        ))),
    );
    anchor.apply_style(anchor_style);
    anchor.set_anchor_name(Some(AnchorName::new("menu_button")));
    let anchor_key = commit_child(&mut arena, parent_key, Box::new(anchor));

    let mut child = Element::new(0.0, 0.0, 80.0, 40.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .anchor("menu_button")
                .left(Length::px(10.0))
                .top(Length::px(0.0))
                .clip(ClipMode::AnchorParent),
        ),
    );
    child.apply_style(child_style);
    let child_k = commit_child(&mut arena, parent_key, Box::new(child));

    let constraints = LayoutConstraints {
        max_width: 800.0,
        max_height: 600.0,
        viewport_width: 800.0,
        viewport_height: 600.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
    };
    let placement = LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 800.0,
        available_height: 600.0,
        viewport_width: 800.0,
        viewport_height: 600.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
    };

    measure_and_place(&mut arena, parent_key, constraints, placement);
    arena.with_element_taken(parent_key, |el, _a| {
        let p = el.as_any_mut().downcast_mut::<Element>().unwrap();
        let _ = p.take_layout_transition_requests();
        let _ = p.take_visual_transition_requests();
    });

    arena.with_element_taken(anchor_key, |el, _a| {
        let anchor = el.as_any_mut().downcast_mut::<Element>().unwrap();
        let mut next_anchor_style = Style::new();
        next_anchor_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(340.0))
                    .top(Length::px(20.0)),
            ),
        );
        next_anchor_style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::single(Transition::new(
                TransitionProperty::Position,
                200,
            ))),
        );
        anchor.apply_style(next_anchor_style);
        anchor.layout_transition_visual_offset_x = -40.0;
        anchor.layout_transition_target_x = Some(340.0);
    });

    arena.with_element_taken(parent_key, |el, _a| {
        el.as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .mark_layout_dirty();
    });
    measure_and_place(&mut arena, parent_key, constraints, placement);

    let anchor = crate::view::test_support::get_element::<Element>(&arena, anchor_key);
    let child = crate::view::test_support::get_element::<Element>(&arena, child_k);
    assert!(
        (anchor.layout_state.layout_position.x - 300.0).abs() < 0.01,
        "anchor_x={}, child_x={}",
        anchor.layout_state.layout_position.x,
        child.layout_state.layout_position.x
    );
    assert!(
        (child.layout_state.layout_position.x - 310.0).abs() < 0.01,
        "anchor_x={}, child_x={}",
        anchor.layout_state.layout_position.x,
        child.layout_state.layout_position.x
    );
}
