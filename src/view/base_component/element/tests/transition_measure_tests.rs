use super::*;

#[test]
fn axis_layout_measure_uses_target_size_not_transition_override_for_distribution() {
    let mut parent = Element::new(0.0, 0.0, 200.0, 100.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flex().row().into()),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(100.0)));
    parent.apply_style(parent_style);
    parent.layout_transition_override_width = Some(320.0);
    parent.layout_transition_override_height = Some(180.0);

    let mut first = Element::new(0.0, 0.0, 20.0, 20.0);
    let mut first_style = Style::new();
    first_style.set_flex(crate::style::flex().grow(1.0).basis(Length::px(50.0)));
    first.apply_style(first_style);

    let mut second = Element::new(0.0, 0.0, 20.0, 20.0);
    let mut second_style = Style::new();
    second_style.set_flex(crate::style::flex().grow(1.0).basis(Length::px(50.0)));
    second.apply_style(second_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _first_k = commit_child(&mut arena, parent_key, Box::new(first));
    let _second_k = commit_child(&mut arena, parent_key, Box::new(second));

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

    let first_snapshot = nth_child_snapshot(&arena, parent_key, 0);
    let second_snapshot = nth_child_snapshot(&arena, parent_key, 1);
    assert_eq!(first_snapshot.width, 100.0);
    assert_eq!(second_snapshot.width, 100.0);
}

#[test]
fn flow_measure_uses_target_size_not_transition_override_for_percent_children() {
    let mut parent = Element::new(0.0, 0.0, 200.0, 100.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flow().row().into()),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(100.0)));
    parent.apply_style(parent_style);
    parent.layout_transition_override_width = Some(320.0);
    parent.layout_transition_override_height = Some(180.0);

    let mut child = Element::new(0.0, 0.0, 20.0, 20.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Width,
        ParsedValue::Length(Length::percent(50.0)),
    );
    child_style.insert(
        PropertyId::Height,
        ParsedValue::Length(Length::percent(50.0)),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _child_k = commit_child(&mut arena, parent_key, Box::new(child));

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
    assert_eq!(snapshot.width, 100.0);
    assert_eq!(snapshot.height, 50.0);
}

#[test]
fn auto_axis_layout_measures_and_places_children_against_constraint_not_stale_zero() {
    let mut parent = Element::new(0.0, 0.0, 200.0, 0.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flex().column().into()),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Auto);
    parent_style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(Transition::new(
            TransitionProperty::Height,
            200,
        ))),
    );
    parent.apply_style(parent_style);
    parent.has_layout_snapshot = true;
    parent.layout_state.layout_size.height = 0.0;

    let child = Element::new(0.0, 0.0, 100.0, 32.0);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _child_k = commit_child(&mut arena, parent_key, Box::new(child));

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

    let parent_snapshot = child_snapshot(&arena, parent_key);
    let child_snapshot = nth_child_snapshot(&arena, parent_key, 0);
    assert_eq!(parent_snapshot.height, 0.0);
    assert_eq!(child_snapshot.height, 32.0);

    let parent_ref = crate::view::test_support::get_element::<Element>(&arena, parent_key);
    assert_eq!(parent_ref.core.size.height, 32.0);
}

#[test]
fn auto_axis_layout_places_children_against_target_not_parent_proposal() {
    let mut parent = Element::new(0.0, 0.0, 200.0, 0.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flow().row().no_wrap().align(Align::Center).into()),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Auto);
    parent.apply_style(parent_style);

    let child = Element::new(0.0, 0.0, 80.0, 20.0);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _child_k = commit_child(&mut arena, parent_key, Box::new(child));

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

    let parent_snapshot = child_snapshot(&arena, parent_key);
    let child_snapshot = nth_child_snapshot(&arena, parent_key, 0);
    assert_eq!(parent_snapshot.height, 20.0);
    assert_eq!(child_snapshot.y, 0.0);
    assert_eq!(child_snapshot.height, 20.0);
}

#[test]
fn explicit_zero_axis_layout_without_transition_reports_zero() {
    let mut parent = Element::new(0.0, 0.0, 200.0, 40.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flex().column().into()),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::Zero));
    parent.apply_style(parent_style);
    parent.has_layout_snapshot = true;
    parent.layout_state.layout_size.height = 40.0;

    let child = Element::new(0.0, 0.0, 100.0, 32.0);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _child_k = commit_child(&mut arena, parent_key, Box::new(child));

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

    let parent_snapshot = child_snapshot(&arena, parent_key);
    let child_snapshot = nth_child_snapshot(&arena, parent_key, 0);
    assert_eq!(parent_snapshot.height, 0.0);
    assert_eq!(child_snapshot.height, 0.0);
}

#[test]
fn flow_places_expanding_height_transition_child_at_target_size() {
    let mut parent = Element::new(0.0, 0.0, 200.0, 0.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flow().column().no_wrap().into()),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Auto);
    parent.apply_style(parent_style);

    let mut expanding = Element::new(0.0, 0.0, 200.0, 0.0);
    let mut expanding_style = Style::new();
    expanding_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flex().column().into()),
    );
    expanding_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
    expanding_style.insert(PropertyId::Height, ParsedValue::Auto);
    expanding_style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(Transition::new(
            TransitionProperty::Height,
            200,
        ))),
    );
    expanding.apply_style(expanding_style);
    expanding.has_layout_snapshot = true;
    expanding.layout_state.layout_size.height = 0.0;

    let content_child = Element::new(0.0, 0.0, 100.0, 32.0);
    let sibling = Element::new(0.0, 0.0, 200.0, 20.0);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let expanding_key = commit_child(&mut arena, parent_key, Box::new(expanding));
    let _content_key = commit_child(&mut arena, expanding_key, Box::new(content_child));
    let _sibling_k = commit_child(&mut arena, parent_key, Box::new(sibling));

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

    let parent_snapshot = child_snapshot(&arena, parent_key);
    let expanding_snapshot = nth_child_snapshot(&arena, parent_key, 0);
    let sibling_snapshot = nth_child_snapshot(&arena, parent_key, 1);
    assert_eq!(parent_snapshot.height, 20.0);
    assert_eq!(expanding_snapshot.height, 0.0);
    assert_eq!(sibling_snapshot.y, 0.0);

    let expanding_ref =
        crate::view::test_support::get_element::<Element>(&arena, expanding_key);
    assert_eq!(expanding_ref.core.size.height, 32.0);
    drop(expanding_ref);

    let reqs = crate::view::test_support::get_element_mut::<Element>(&arena, expanding_key)
        .take_layout_transition_requests();
    let h_req = reqs
        .iter()
        .find(|req| req.field == LayoutField::Height)
        .expect("expanding child should request a height transition");
    assert_eq!(h_req.from, 0.0);
    assert_eq!(h_req.to, 32.0);
}

#[test]
fn explicit_height_transition_start_reports_current_size_to_parent_measure() {
    let mut parent = Element::new(0.0, 0.0, 200.0, 0.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flow().column().no_wrap().into()),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Auto);
    parent.apply_style(parent_style);

    let mut collapsing = Element::new(0.0, 0.0, 200.0, 0.0);
    let mut collapsing_style = Style::new();
    collapsing_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flex().column().into()),
    );
    collapsing_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(200.0)));
    collapsing_style.insert(PropertyId::Height, ParsedValue::Length(Length::Zero));
    collapsing_style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(Transition::new(
            TransitionProperty::Height,
            200,
        ))),
    );
    collapsing.apply_style(collapsing_style);
    collapsing.has_layout_snapshot = true;
    collapsing.layout_state.layout_size.height = 80.0;

    let content_child = Element::new(0.0, 0.0, 100.0, 32.0);
    let sibling = Element::new(0.0, 0.0, 200.0, 20.0);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let collapsing_key = commit_child(&mut arena, parent_key, Box::new(collapsing));
    let content_key = commit_child(&mut arena, collapsing_key, Box::new(content_child));
    let _sibling_k = commit_child(&mut arena, parent_key, Box::new(sibling));

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

    let parent_snapshot = child_snapshot(&arena, parent_key);
    let collapsing_snapshot = nth_child_snapshot(&arena, parent_key, 0);
    let content_snapshot = child_snapshot(&arena, content_key);
    let sibling_snapshot = nth_child_snapshot(&arena, parent_key, 1);
    assert_eq!(parent_snapshot.height, 100.0);
    assert_eq!(collapsing_snapshot.height, 80.0);
    assert_eq!(content_snapshot.height, 32.0);
    assert_eq!(sibling_snapshot.y, 80.0);
}

#[test]
fn width_transition_on_flow_child_repositions_following_sibling() {
    let mut parent = Element::new(0.0, 0.0, 80.0, 40.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flow().row().no_wrap().into()),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(80.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
    parent.apply_style(parent_style);

    let mut spacer = Element::new_with_id(1, 0.0, 0.0, 0.0, 20.0);
    let mut spacer_style = Style::new();
    spacer_style.insert(PropertyId::Width, ParsedValue::Length(Length::Zero));
    spacer_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
    spacer_style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(Transition::new(
            TransitionProperty::Width,
            180,
        ))),
    );
    spacer.apply_style(spacer_style);

    let mut thumb = Element::new_with_id(2, 0.0, 0.0, 20.0, 20.0);
    let mut thumb_style = Style::new();
    thumb_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
    thumb_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
    thumb.apply_style(thumb_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let spacer_key = commit_child(&mut arena, parent_key, Box::new(spacer));
    let _ = commit_child(&mut arena, parent_key, Box::new(thumb));

    let constraints = LayoutConstraints {
        max_width: 800.0,
        max_height: 600.0,
        viewport_width: 800.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
        viewport_height: 600.0,
    };
    let placement = LayoutPlacement {
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
    };

    measure_and_place(&mut arena, parent_key, constraints, placement);
    let _ = crate::view::test_support::get_element_mut::<Element>(&arena, parent_key)
        .take_layout_transition_requests();

    let mut next_spacer_style = Style::new();
    next_spacer_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
    crate::view::test_support::get_element_mut::<Element>(&arena, spacer_key)
        .apply_style(next_spacer_style);

    crate::view::test_support::get_element_mut::<Element>(&arena, parent_key)
        .mark_layout_dirty();
    measure_and_place(&mut arena, parent_key, constraints, placement);

    let reqs = crate::view::test_support::get_element_mut::<Element>(&arena, spacer_key)
        .take_layout_transition_requests();
    assert!(reqs.iter().any(|req| req.field == LayoutField::Width));

    crate::view::test_support::get_element_mut::<Element>(&arena, spacer_key)
        .set_layout_transition_width(10.0);
    crate::view::test_support::get_element_mut::<Element>(&arena, parent_key)
        .mark_layout_dirty();
    measure_and_place(&mut arena, parent_key, constraints, placement);

    let thumb_snapshot = nth_child_snapshot(&arena, parent_key, 1);
    assert!((thumb_snapshot.x - 10.0).abs() < 0.01);
}
