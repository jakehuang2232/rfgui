use super::*;

#[test]
fn min_and_max_size_clamp_explicit_width_and_height() {
    let mut el = Element::new(0.0, 0.0, 320.0, 20.0);
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(320.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
    style.insert(PropertyId::MinWidth, ParsedValue::Length(Length::px(120.0)));
    style.insert(PropertyId::MaxWidth, ParsedValue::Length(Length::px(180.0)));
    style.insert(PropertyId::MinHeight, ParsedValue::Length(Length::px(40.0)));
    style.insert(PropertyId::MaxHeight, ParsedValue::Length(Length::px(60.0)));
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

    let snapshot = child_snapshot(&arena, key);
    assert_eq!(snapshot.width, 180.0);
    assert_eq!(snapshot.height, 40.0);
}

#[test]
fn percent_min_and_max_size_resolve_against_parent_inner_size() {
    let parent = Element::new(0.0, 0.0, 300.0, 200.0);
    let mut child = Element::new(0.0, 0.0, 500.0, 10.0);
    let mut child_style = Style::new();
    child_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(500.0)));
    child_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(10.0)));
    child_style.insert(
        PropertyId::MinWidth,
        ParsedValue::Length(Length::percent(50.0)),
    );
    child_style.insert(
        PropertyId::MaxWidth,
        ParsedValue::Length(Length::percent(60.0)),
    );
    child_style.insert(
        PropertyId::MinHeight,
        ParsedValue::Length(Length::percent(40.0)),
    );
    child_style.insert(
        PropertyId::MaxHeight,
        ParsedValue::Length(Length::percent(45.0)),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
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

    let snap = nth_child_snapshot(&arena, parent_key, 0);
    assert_eq!(snap.width, 180.0);
    assert_eq!(snap.height, 80.0);
}

#[test]
fn percent_min_and_max_size_apply_when_parent_auto_has_resolved_percent_base() {
    let mut parent = Element::new(0.0, 0.0, 100.0, 100.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Width, ParsedValue::Auto);
    parent_style.insert(PropertyId::Height, ParsedValue::Auto);
    parent.apply_style(parent_style);

    let mut child = Element::new(0.0, 0.0, 20.0, 20.0);
    let mut child_style = Style::new();
    child_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
    child_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
    child_style.insert(
        PropertyId::MinWidth,
        ParsedValue::Length(Length::percent(60.0)),
    );
    child_style.insert(
        PropertyId::MinHeight,
        ParsedValue::Length(Length::percent(70.0)),
    );
    child_style.insert(
        PropertyId::MaxWidth,
        ParsedValue::Length(Length::percent(10.0)),
    );
    child_style.insert(
        PropertyId::MaxHeight,
        ParsedValue::Length(Length::percent(10.0)),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
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

    let snap = nth_child_snapshot(&arena, parent_key, 0);
    assert_eq!(snap.width, 480.0);
    assert_eq!(snap.height, 420.0);
}

#[test]
fn min_greater_than_max_uses_min_as_effective_max() {
    let mut el = Element::new(0.0, 0.0, 10.0, 10.0);
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(80.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(30.0)));
    style.insert(PropertyId::MinWidth, ParsedValue::Length(Length::px(120.0)));
    style.insert(PropertyId::MaxWidth, ParsedValue::Length(Length::px(90.0)));
    style.insert(PropertyId::MinHeight, ParsedValue::Length(Length::px(50.0)));
    style.insert(PropertyId::MaxHeight, ParsedValue::Length(Length::px(40.0)));
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

    let snapshot = child_snapshot(&arena, key);
    assert_eq!(snapshot.width, 120.0);
    assert_eq!(snapshot.height, 50.0);
}
