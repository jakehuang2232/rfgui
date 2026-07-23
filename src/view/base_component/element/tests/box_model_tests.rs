use super::*;

#[test]
fn justify_content_space_evenly_distributes_free_space() {
    let (start, gap) =
        main_axis_start_and_gap(100.0, 40.0, 0.0, 3, JustifyContent::SpaceEvenly);
    assert!((start - 15.0).abs() < 0.001);
    assert!((gap - 15.0).abs() < 0.001);
}

#[test]
fn child_layout_uses_parent_inner_box_with_padding() {
    let mut root = Element::new(10.0, 20.0, 200.0, 120.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root.apply_style(root_style);
    root.set_padding_left(8.0);
    root.set_padding_top(12.0);
    root.set_padding_right(16.0);
    root.set_padding_bottom(10.0);

    let mut arena = new_test_arena();
    let root_key = commit_element(&mut arena, Box::new(root));
    let child = Element::new(4.0, 6.0, 300.0, 300.0);
    let _child_key = commit_child(&mut arena, root_key, Box::new(child));

    measure_and_place(
        &mut arena,
        root_key,
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
    let snapshot = nth_child_snapshot(&arena, root_key, 0);

    assert_eq!(snapshot.x, 22.0);
    assert_eq!(snapshot.y, 38.0);
    assert_eq!(snapshot.width, 300.0);
    assert_eq!(snapshot.height, 300.0);
}

#[test]
fn box_shadow_spread_keeps_per_corner_radii() {
    let base = normalize_corner_radii(
        super::super::CornerRadii {
            top_left: 4.0,
            top_right: 12.0,
            bottom_right: 20.0,
            bottom_left: 8.0,
        },
        120.0,
        80.0,
    );
    let spread = 6.0;
    let shadow = expand_corner_radii_for_spread(base, spread, 120.0, 80.0);

    assert!((shadow.top_left - 10.0).abs() < 0.001);
    assert!((shadow.top_right - 18.0).abs() < 0.001);
    assert!((shadow.bottom_right - 26.0).abs() < 0.001);
    assert!((shadow.bottom_left - 14.0).abs() < 0.001);
}

#[test]
fn content_box_subtracts_border_and_padding() {
    let mut root = Element::new(10.0, 20.0, 200.0, 120.0);
    let mut style = Style::new();
    style.set_border(Border::uniform(Length::px(5.0), &Color::hex("#000000")));
    root.apply_style(style);
    root.set_padding_left(8.0);
    root.set_padding_top(12.0);
    root.set_padding_right(16.0);
    root.set_padding_bottom(10.0);

    let mut arena = new_test_arena();
    let root_key = commit_element(&mut arena, Box::new(root));
    let child = Element::new(0.0, 0.0, 300.0, 300.0);
    let _child_key = commit_child(&mut arena, root_key, Box::new(child));

    measure_and_place(
        &mut arena,
        root_key,
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
    let snapshot = nth_child_snapshot(&arena, root_key, 0);

    assert_eq!(snapshot.x, 23.0);
    assert_eq!(snapshot.y, 37.0);
    assert_eq!(snapshot.width, 300.0);
    assert_eq!(snapshot.height, 300.0);
}

#[test]
fn element_layout_preserves_fractional_box_metrics() {
    let mut root = Element::new(1.2, 2.4, 100.5, 50.5);
    let mut style = Style::new();
    style.set_padding(crate::style::Padding::new().xy(Length::px(3.25), Length::px(2.5)));
    root.apply_style(style);

    let mut arena = new_test_arena();
    let root_key = commit_element(&mut arena, Box::new(root));

    measure_and_place(
        &mut arena,
        root_key,
        crate::view::base_component::LayoutConstraints {
            max_width: 200.0,
            max_height: 200.0,
            viewport_width: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        },
        crate::view::base_component::LayoutPlacement {
            parent_x: 4.1,
            parent_y: 5.3,
            visual_offset_x: 0.2,
            visual_offset_y: -0.1,
            available_width: 200.0,
            available_height: 200.0,
            viewport_width: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        },
    );

    let root_el = crate::view::test_support::get_element::<Element>(&arena, root_key);
    let snapshot = root_el.box_model_snapshot();
    assert!((snapshot.x - 5.5).abs() < 0.01);
    assert!((snapshot.y - 7.6).abs() < 0.01);
    assert!((snapshot.width - 100.5).abs() < 0.01);
    assert!((snapshot.height - 50.5).abs() < 0.01);
    assert!((root_el.layout_state.layout_inner_position.x - 8.75).abs() < 0.01);
    assert!((root_el.layout_state.layout_inner_position.y - 10.1).abs() < 0.01);
    assert!((root_el.layout_state.layout_inner_size.width - 94.0).abs() < 0.01);
    assert!((root_el.layout_state.layout_inner_size.height - 45.5).abs() < 0.01);
}

#[test]
fn inner_clip_rect_uses_flex_assigned_width() {
    let mut parent = Element::new(0.0, 0.0, 240.0, 40.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(240.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flex().row().into()),
    );
    parent.apply_style(parent_style);

    let mut child = Element::new(0.0, 0.0, 0.0, 18.0);
    let mut child_style = Style::new();
    child_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
    child_style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(crate::style::flex().grow(1.0).shrink(1.0)),
    );
    child_style.set_border_radius(BorderRadius::uniform(Length::px(4.0)));
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let child_k = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        parent_key,
        crate::view::base_component::LayoutConstraints {
            max_width: 240.0,
            max_height: 40.0,
            viewport_width: 240.0,
            viewport_height: 40.0,
            percent_base_width: Some(240.0),
            percent_base_height: Some(40.0),
        },
        crate::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 240.0,
            available_height: 40.0,
            viewport_width: 240.0,
            viewport_height: 40.0,
            percent_base_width: Some(240.0),
            percent_base_height: Some(40.0),
        },
    );

    let child_el = crate::view::test_support::get_element::<Element>(&arena, child_k);
    let snapshot = child_el.box_model_snapshot();
    let inner = child_el.inner_clip_rect();

    assert!((snapshot.width - 240.0).abs() < 0.01);
    assert!((inner.width - 240.0).abs() < 0.01);
}
