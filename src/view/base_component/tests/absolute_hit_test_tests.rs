use super::*;

#[test]
fn diagnostic_absolute_viewport_clip_without_anchor_respects_later_root_body() {
    let (arena, root_keys, popup_key) = absolute_diagnostic_roots(
        Position::absolute()
            .left(Length::px(100.0))
            .top(Length::px(10.0))
            .clip(ClipMode::Viewport),
    );

    assert_eq!(hit_test(&arena, root_keys[0], 105.0, 15.0), Some(popup_key));
    assert_eq!(
        hit_test_roots(&arena, &root_keys, 105.0, 15.0),
        Some((1, root_keys[1])),
        "clip:Viewport escapes the parent gate but does not cross root sibling stacking"
    );
}

#[test]
fn diagnostic_absolute_anchor_parent_without_anchor_respects_later_root_body() {
    let (arena, root_keys, popup_key) = absolute_diagnostic_roots(
        Position::absolute()
            .left(Length::px(100.0))
            .top(Length::px(10.0))
            .clip(ClipMode::AnchorParent),
    );

    assert_eq!(hit_test(&arena, root_keys[0], 105.0, 15.0), Some(popup_key));
    assert_eq!(
        hit_test_roots(&arena, &root_keys, 105.0, 15.0),
        Some((1, root_keys[1])),
        "clip:AnchorParent escapes the parent gate but does not cross root sibling stacking"
    );
}

#[test]
fn diagnostic_absolute_parent_clip_overflow_does_not_escape_parent_hit_region() {
    let (arena, root_keys, popup_key) = absolute_diagnostic_roots(
        Position::absolute()
            .left(Length::px(100.0))
            .top(Length::px(10.0))
            .clip(ClipMode::Parent),
    );

    assert_eq!(
        hit_test(&arena, root_keys[0], 75.0, 15.0),
        Some(root_keys[0])
    );
    assert_eq!(hit_test(&arena, root_keys[0], 105.0, 15.0), None);
    assert_ne!(
        hit_test_roots(&arena, &root_keys, 105.0, 15.0),
        Some((0, popup_key)),
        "ClipMode::Parent absolute overflow is clipped by design"
    );
}

#[test]
fn diagnostic_root_level_absolute_viewport_clip_loses_to_later_root_body() {
    let mut lower_root = absolute_diagnostic_element(
        Position::absolute()
            .left(Length::px(100.0))
            .top(Length::px(10.0))
            .clip(ClipMode::Viewport),
        crate::style::Cursor::Crosshair,
    );
    lower_root.set_anchor_name(Some(AnchorName::new("diagnostic_root_popup")));
    let mut higher_root = Element::new(90.0, 0.0, 80.0, 80.0);
    higher_root.set_background_color_value(Color::rgb(32, 32, 32));

    let mut arena = new_test_arena();
    let popup_root_key = commit_element(&mut arena, Box::new(lower_root));
    let higher_key = commit_element(&mut arena, Box::new(higher_root));
    let root_keys = [popup_root_key, higher_key];
    for &root_key in &root_keys {
        measure_and_place(
            &mut arena,
            root_key,
            constraints(220.0, 120.0),
            placement(220.0, 120.0),
        );
    }

    assert_eq!(
        hit_test(&arena, popup_root_key, 105.0, 15.0),
        Some(popup_root_key)
    );
    assert_eq!(
        hit_test_roots(&arena, &root_keys, 105.0, 15.0),
        Some((1, higher_key)),
        "root-level absolute roots follow root stacking; a later root body wins"
    );
}

#[test]
fn diagnostic_transformed_absolute_viewport_clip_respects_later_root_body() {
    let mut lower_root = Element::new(0.0, 0.0, 80.0, 80.0);
    lower_root.set_background_color_value(Color::rgb(16, 16, 16));
    let mut popup = Element::new(0.0, 0.0, 20.0, 20.0);
    let mut popup_style = Style::new();
    popup_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(0.0))
                .top(Length::px(10.0))
                .clip(ClipMode::Viewport),
        ),
    );
    popup_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
    popup_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
    popup_style.insert(
        PropertyId::Cursor,
        ParsedValue::Cursor(crate::style::Cursor::Crosshair),
    );
    popup_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#ff00ff")),
    );
    popup_style.set_transform(Transform::new([Translate::x(Length::px(100.0))]));
    popup.apply_style(popup_style);

    let mut higher_root = Element::new(90.0, 0.0, 80.0, 80.0);
    higher_root.set_background_color_value(Color::rgb(32, 32, 32));

    let mut arena = new_test_arena();
    let lower_key = commit_element(&mut arena, Box::new(lower_root));
    let popup_key = commit_child(&mut arena, lower_key, Box::new(popup));
    let higher_key = commit_element(&mut arena, Box::new(higher_root));
    let root_keys = [lower_key, higher_key];
    for &root_key in &root_keys {
        measure_and_place(
            &mut arena,
            root_key,
            constraints(220.0, 120.0),
            placement(220.0, 120.0),
        );
    }
    assert_eq!(hit_test(&arena, lower_key, 105.0, 15.0), Some(popup_key));
    assert_eq!(
        hit_test_roots(&arena, &root_keys, 105.0, 15.0),
        Some((1, higher_key)),
        "transformed escape absolute remains owned by its root stacking context"
    );
}

#[test]
fn diagnostic_named_anchor_anchor_parent_respects_later_root_body() {
    let mut lower_root = Element::new(0.0, 0.0, 80.0, 80.0);
    lower_root.set_background_color_value(Color::rgb(16, 16, 16));
    lower_root.set_anchor_name(Some(AnchorName::new("diagnostic_anchor")));
    let popup = absolute_diagnostic_element(
        Position::absolute()
            .anchor(Anchor::Name(AnchorName::new("diagnostic_anchor")))
            .right(Length::px(-20.0))
            .top(Length::px(10.0))
            .clip(ClipMode::AnchorParent),
        crate::style::Cursor::Crosshair,
    );

    let mut higher_root = Element::new(90.0, 0.0, 80.0, 80.0);
    higher_root.set_background_color_value(Color::rgb(32, 32, 32));

    let mut arena = new_test_arena();
    let lower_key = commit_element(&mut arena, Box::new(lower_root));
    let popup_key = commit_child(&mut arena, lower_key, Box::new(popup));
    let higher_key = commit_element(&mut arena, Box::new(higher_root));
    let root_keys = [lower_key, higher_key];
    for &root_key in &root_keys {
        measure_and_place(
            &mut arena,
            root_key,
            constraints(220.0, 120.0),
            placement(220.0, 120.0),
        );
    }

    assert_eq!(hit_test(&arena, lower_key, 95.0, 15.0), Some(popup_key));
    assert_eq!(
        hit_test_roots(&arena, &root_keys, 95.0, 15.0),
        Some((1, higher_key)),
        "named AnchorParent escape remains owned by its root stacking context"
    );
}
