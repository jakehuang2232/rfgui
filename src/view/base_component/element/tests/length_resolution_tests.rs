use super::*;

#[test]
fn percent_child_size_works_with_definite_containing_size() {
    let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Width, ParsedValue::Auto);
    parent_style.insert(PropertyId::Height, ParsedValue::Auto);
    parent.apply_style(parent_style);

    let mut child = Element::new(0.0, 0.0, 123.0, 77.0);
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
    let _child_key = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        parent_key,
        crate::view::base_component::LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        },
        crate::view::base_component::LayoutPlacement {
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
    let snapshot_unknown = nth_child_snapshot(&arena, parent_key, 0);
    assert_eq!(snapshot_unknown.width, 400.0);
    assert_eq!(snapshot_unknown.height, 300.0);

    let mut known_parent = Element::new(0.0, 0.0, 240.0, 120.0);
    let mut known_parent_style = Style::new();
    known_parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
    known_parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
    known_parent.apply_style(known_parent_style);

    let mut child2 = Element::new(0.0, 0.0, 123.0, 77.0);
    let mut child2_style = Style::new();
    child2_style.insert(
        PropertyId::Width,
        ParsedValue::Length(Length::percent(50.0)),
    );
    child2_style.insert(
        PropertyId::Height,
        ParsedValue::Length(Length::percent(50.0)),
    );
    child2.apply_style(child2_style);

    let known_parent_key = commit_element(&mut arena, Box::new(known_parent));
    let _child2_key = commit_child(&mut arena, known_parent_key, Box::new(child2));

    measure_and_place(
        &mut arena,
        known_parent_key,
        crate::view::base_component::LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        },
        crate::view::base_component::LayoutPlacement {
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
    let snapshot_known = nth_child_snapshot(&arena, known_parent_key, 0);
    assert_eq!(snapshot_known.width, 120.0);
    assert_eq!(snapshot_known.height, 60.0);
}

#[test]
fn calc_percent_and_px_resolves_against_parent_content_size() {
    let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
    parent.apply_style(parent_style);

    let mut child = Element::new(0.0, 0.0, 50.0, 40.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Width,
        ParsedValue::Length(Length::calc(
            Length::percent(100.0),
            Operator::subtract,
            Length::px(20.0),
        )),
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

    let snapshot = nth_child_snapshot(&arena, parent_key, 0);
    assert_eq!(snapshot.width, 220.0);
}

#[test]
fn calc_with_percent_resolves_when_containing_size_is_definite() {
    let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Width, ParsedValue::Auto);
    parent_style.insert(PropertyId::Height, ParsedValue::Auto);
    parent.apply_style(parent_style);

    let mut child = Element::new(0.0, 0.0, 77.0, 40.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Width,
        ParsedValue::Length(Length::calc(
            Length::percent(100.0),
            Operator::subtract,
            Length::px(20.0),
        )),
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

    let snapshot = nth_child_snapshot(&arena, parent_key, 0);
    assert_eq!(snapshot.width, 780.0);
}

#[test]
fn calc_with_percent_falls_back_to_auto_when_containing_size_is_indefinite() {
    let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Width, ParsedValue::Auto);
    parent_style.insert(PropertyId::Height, ParsedValue::Auto);
    parent.apply_style(parent_style);

    let mut child = Element::new(0.0, 0.0, 77.0, 40.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Width,
        ParsedValue::Length(Length::calc(
            Length::percent(100.0),
            Operator::subtract,
            Length::px(20.0),
        )),
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
            viewport_height: 600.0,
            percent_base_width: None,
            percent_base_height: None,
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
            percent_base_width: None,
            percent_base_height: None,
        },
    );

    let snapshot = nth_child_snapshot(&arena, parent_key, 0);
    assert_eq!(snapshot.width, 77.0);
}

#[test]
fn calc_nested_with_multiply_and_add_is_supported() {
    let mut el = Element::new(0.0, 0.0, 10.0, 10.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::Width,
        ParsedValue::Length(Length::calc(
            Length::percent(100.0),
            Operator::plus,
            Length::calc(Length::px(10.0), Operator::multiply, 5),
        )),
    );
    el.apply_style(style);

    let mut arena = new_test_arena();
    let key = commit_element(&mut arena, Box::new(el));
    measure_and_place(
        &mut arena,
        key,
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

    assert_eq!(child_snapshot(&arena, key).width, 850.0);
}

#[test]
fn vh_child_size_resolves_against_viewport_height() {
    let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
    parent.apply_style(parent_style);

    let mut child = Element::new(0.0, 0.0, 10.0, 10.0);
    let mut child_style = Style::new();
    child_style.insert(PropertyId::Width, ParsedValue::Length(Length::vh(50.0)));
    child_style.insert(PropertyId::Height, ParsedValue::Length(Length::vh(50.0)));
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
    assert_eq!(snapshot.width, 300.0);
    assert_eq!(snapshot.height, 300.0);
}

#[test]
fn vw_child_size_resolves_against_viewport_width() {
    let mut parent = Element::new(0.0, 0.0, 240.0, 120.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
    parent.apply_style(parent_style);

    let mut child = Element::new(0.0, 0.0, 10.0, 10.0);
    let mut child_style = Style::new();
    child_style.insert(PropertyId::Width, ParsedValue::Length(Length::vw(50.0)));
    child_style.insert(PropertyId::Height, ParsedValue::Length(Length::vw(50.0)));
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
    assert_eq!(snapshot.width, 400.0);
    assert_eq!(snapshot.height, 400.0);
}

#[test]
fn vh_falls_back_to_zero_when_viewport_is_unknown() {
    assert_eq!(
        resolve_px_with_base(Length::vh(50.0), None, 0.0, 0.0),
        Some(0.0)
    );
    assert_eq!(
        resolve_signed_px_with_base(Length::vh(-20.0), None, 0.0, 0.0),
        Some(0.0)
    );
    assert_eq!(
        resolve_px_with_base(Length::vw(50.0), None, 0.0, 0.0),
        Some(0.0)
    );
    assert_eq!(
        resolve_signed_px_with_base(Length::vw(-20.0), None, 0.0, 0.0),
        Some(0.0)
    );
}
