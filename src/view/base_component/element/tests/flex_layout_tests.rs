use super::*;

#[test]
fn flex_row_grow_distributes_remaining_space_to_children() {
    let mut parent = Element::new(0.0, 0.0, 300.0, 120.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flex().row().align(Align::Center).into()),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(300.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
    parent.apply_style(parent_style);

    let mut first = Element::new(0.0, 0.0, 40.0, 20.0);
    let mut first_style = Style::new();
    first_style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(crate::style::flex().basis(Length::px(40.0)).grow(1.0)),
    );
    first.apply_style(first_style);

    let mut second = Element::new(0.0, 0.0, 40.0, 30.0);
    let mut second_style = Style::new();
    second_style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(crate::style::flex().basis(Length::px(40.0)).grow(2.0)),
    );
    second.apply_style(second_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _ = commit_child(&mut arena, parent_key, Box::new(first));
    let _ = commit_child(&mut arena, parent_key, Box::new(second));

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
    assert_eq!(first_snapshot.width, 113.333336);
    assert_eq!(second_snapshot.width, 186.66667);
    assert_eq!(first_snapshot.y, 50.0);
    assert_eq!(second_snapshot.y, 45.0);
}

#[test]
fn flex_row_shrink_uses_basis_when_content_overflows() {
    let mut parent = Element::new(0.0, 0.0, 150.0, 80.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flex().row().into()),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(150.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(80.0)));
    parent.apply_style(parent_style);

    let mut first = Element::new(0.0, 0.0, 80.0, 20.0);
    let mut first_style = Style::new();
    first_style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(crate::style::flex().basis(Length::px(100.0)).shrink(1.0)),
    );
    first.apply_style(first_style);

    let mut second = Element::new(0.0, 0.0, 80.0, 20.0);
    let mut second_style = Style::new();
    second_style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(crate::style::flex().basis(Length::px(100.0)).shrink(1.0)),
    );
    second.apply_style(second_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _ = commit_child(&mut arena, parent_key, Box::new(first));
    let _ = commit_child(&mut arena, parent_key, Box::new(second));

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

    assert!((first_snapshot.width - 75.0).abs() < 0.01);
    assert!((second_snapshot.width - 75.0).abs() < 0.01);
    assert!((second_snapshot.x - 75.0).abs() < 0.01);
}

#[test]
fn flex_measure_does_not_feed_distributed_main_size_back_into_auto_basis() {
    let mut parent = Element::new(0.0, 0.0, 100.0, 100.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flex().row().into()),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(100.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(100.0)));
    parent.apply_style(parent_style);

    let mut first = Element::new(0.0, 0.0, 10.0, 20.0);
    let mut first_style = Style::new();
    first_style.insert(PropertyId::Width, ParsedValue::Auto);
    first_style.insert(PropertyId::Height, ParsedValue::Auto);
    first_style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(crate::style::flex().shrink(1.0)),
    );
    first.apply_style(first_style);

    let mut second = Element::new(0.0, 0.0, 120.0, 20.0);
    let mut second_style = Style::new();
    second_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
    second_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
    second_style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(crate::style::flex().shrink(1.0)),
    );
    second.apply_style(second_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let first_key = commit_child(&mut arena, parent_key, Box::new(first));
    let _first_leaf = commit_child(
        &mut arena,
        first_key,
        Box::new(Element::new(0.0, 0.0, 20.0, 20.0)),
    );
    let _second_key = commit_child(&mut arena, parent_key, Box::new(second));

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
    let first_snapshot = nth_child_snapshot(&arena, parent_key, 0);
    let second_snapshot = nth_child_snapshot(&arena, parent_key, 1);
    assert!((first_snapshot.width - 20.0).abs() < 0.01);
    assert!((second_snapshot.width - 80.0).abs() < 0.01);

    crate::view::test_support::get_element_mut::<Element>(&arena, parent_key)
        .mark_layout_dirty();
    measure_and_place(&mut arena, parent_key, constraints, placement);
    let first_snapshot = nth_child_snapshot(&arena, parent_key, 0);
    let second_snapshot = nth_child_snapshot(&arena, parent_key, 1);
    assert!((first_snapshot.width - 20.0).abs() < 0.01);
    assert!((second_snapshot.width - 80.0).abs() < 0.01);
}

#[test]
fn flex_grow_redistributes_remaining_space_after_max_width_clamp() {
    let mut parent = Element::new(0.0, 0.0, 100.0, 40.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flex().row().into()),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(100.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
    parent.apply_style(parent_style);

    let mut first = Element::new(0.0, 0.0, 20.0, 20.0);
    let mut first_style = Style::new();
    first_style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(crate::style::flex().grow(1.0)),
    );
    first_style.insert(PropertyId::MaxWidth, ParsedValue::Length(Length::px(30.0)));
    first.apply_style(first_style);

    let mut second = Element::new(0.0, 0.0, 20.0, 20.0);
    let mut second_style = Style::new();
    second_style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(crate::style::flex().grow(1.0)),
    );
    second.apply_style(second_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _ = commit_child(&mut arena, parent_key, Box::new(first));
    let _ = commit_child(&mut arena, parent_key, Box::new(second));
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
    assert!((first_snapshot.width - 30.0).abs() < 0.01);
    assert!((second_snapshot.width - 70.0).abs() < 0.01);
}

#[test]
fn flex_shrink_redistributes_remaining_space_after_min_width_clamp() {
    let mut parent = Element::new(0.0, 0.0, 80.0, 40.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flex().row().into()),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(80.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
    parent.apply_style(parent_style);

    let mut first = Element::new(0.0, 0.0, 60.0, 20.0);
    let mut first_style = Style::new();
    first_style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(crate::style::flex().shrink(1.0)),
    );
    first_style.insert(PropertyId::MinWidth, ParsedValue::Length(Length::px(50.0)));
    first.apply_style(first_style);

    let mut second = Element::new(0.0, 0.0, 60.0, 20.0);
    let mut second_style = Style::new();
    second_style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(crate::style::flex().shrink(1.0)),
    );
    second.apply_style(second_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _ = commit_child(&mut arena, parent_key, Box::new(first));
    let _ = commit_child(&mut arena, parent_key, Box::new(second));
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
    assert!((first_snapshot.width - 50.0).abs() < 0.01);
    assert!((second_snapshot.width - 30.0).abs() < 0.01);
}

#[test]
fn flex_auto_min_main_size_uses_measured_size_for_auto_main_axis_items() {
    let mut parent = Element::new(0.0, 0.0, 80.0, 40.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flex().row().into()),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(80.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
    parent.apply_style(parent_style);

    let mut first = Element::new(0.0, 0.0, 10.0, 20.0);
    let mut first_style = Style::new();
    first_style.insert(PropertyId::Width, ParsedValue::Auto);
    first_style.insert(PropertyId::Height, ParsedValue::Auto);
    first_style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(crate::style::flex().shrink(1.0)),
    );
    first.apply_style(first_style);

    let mut second = Element::new(0.0, 0.0, 60.0, 20.0);
    let mut second_style = Style::new();
    second_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(60.0)));
    second_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
    second_style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(crate::style::flex().shrink(1.0)),
    );
    second.apply_style(second_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let first_key = commit_child(&mut arena, parent_key, Box::new(first));
    let _ = commit_child(
        &mut arena,
        first_key,
        Box::new(Element::new(0.0, 0.0, 60.0, 20.0)),
    );
    let _ = commit_child(&mut arena, parent_key, Box::new(second));
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
    assert!((first_snapshot.width - 60.0).abs() < 0.01);
    assert!((second_snapshot.width - 20.0).abs() < 0.01);
}

#[test]
fn explicit_flex_basis_is_not_clamped_by_intrinsic_auto_min_main() {
    let mut parent = Element::new(0.0, 0.0, 409.0, 40.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flex().row().into()),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(409.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
    parent_style.insert(PropertyId::Gap, ParsedValue::Length(Length::px(4.0)));
    parent.apply_style(parent_style);

    let mut track = Element::new(0.0, 0.0, 155.0, 18.0);
    let mut track_style = Style::new();
    track_style.insert(PropertyId::Width, ParsedValue::Auto);
    track_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
    track_style.insert(PropertyId::MinWidth, ParsedValue::Length(Length::Zero));
    track_style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(crate::style::flex().grow(3.0).shrink(1.0)),
    );
    track.apply_style(track_style);

    let mut label = Element::new(0.0, 0.0, 250.0, 18.0);
    let mut label_style = Style::new();
    label_style.insert(PropertyId::Width, ParsedValue::Auto);
    label_style.insert(PropertyId::Height, ParsedValue::Auto);
    label_style.insert(PropertyId::MaxWidth, ParsedValue::Length(Length::px(250.0)));
    label_style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(
            crate::style::flex()
                .grow(1.0)
                .shrink(1.0)
                .basis(Length::px(80.0)),
        ),
    );
    label.apply_style(label_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _ = commit_child(&mut arena, parent_key, Box::new(track));
    let label_key = commit_child(&mut arena, parent_key, Box::new(label));
    let _ = commit_child(
        &mut arena,
        label_key,
        Box::new(Element::new(0.0, 0.0, 250.0, 18.0)),
    );

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 409.0,
            max_height: 40.0,
            viewport_width: 800.0,
            percent_base_width: Some(409.0),
            percent_base_height: Some(40.0),
            viewport_height: 600.0,
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 409.0,
            available_height: 40.0,
            viewport_width: 800.0,
            percent_base_width: Some(409.0),
            percent_base_height: Some(40.0),
            viewport_height: 600.0,
        },
    );

    let track_snapshot = nth_child_snapshot(&arena, parent_key, 0);
    let label_snapshot = nth_child_snapshot(&arena, parent_key, 1);
    assert!(
        (track_snapshot.width - 243.75).abs() < 0.01,
        "track width should grow from zero basis, got {}",
        track_snapshot.width
    );
    assert!(
        (label_snapshot.width - 161.25).abs() < 0.01,
        "label width should grow from 80px basis, not clamp to intrinsic 250px, got {}",
        label_snapshot.width
    );
}

#[test]
fn flex_basis_auto_uses_zero_when_child_main_size_is_indefinite() {
    let mut parent = Element::new(0.0, 0.0, 80.0, 40.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flex().row().into()),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(80.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
    parent.apply_style(parent_style);

    let mut first = Element::new(0.0, 0.0, 10.0, 20.0);
    let mut first_style = Style::new();
    first_style.insert(PropertyId::Width, ParsedValue::Auto);
    first_style.insert(PropertyId::Height, ParsedValue::Auto);
    first_style.insert(PropertyId::MinWidth, ParsedValue::Length(Length::Zero));
    first_style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(crate::style::flex().shrink(1.0)),
    );
    first.apply_style(first_style);

    let mut second = Element::new(0.0, 0.0, 60.0, 20.0);
    let mut second_style = Style::new();
    second_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(60.0)));
    second_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
    second_style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(crate::style::flex().shrink(1.0)),
    );
    second.apply_style(second_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let first_key = commit_child(&mut arena, parent_key, Box::new(first));
    let _ = commit_child(
        &mut arena,
        first_key,
        Box::new(Element::new(0.0, 0.0, 60.0, 20.0)),
    );
    let _ = commit_child(&mut arena, parent_key, Box::new(second));
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
    assert!((first_snapshot.width - 0.0).abs() < 0.01);
    assert!((second_snapshot.width - 60.0).abs() < 0.01);
}

/// CSS inline rule: a fragmentable inline wrapper's vertical
/// padding/border MUST NOT contribute to the line height seen by
/// unified inline placement. Two sibling padded wrappers wrapping
/// multi-line text under a common Inline parent should produce
/// per-line text Y intervals equal to (text ascent + descent),
/// NOT (ascent + descent + v_inset). Regression guard against
/// cba6a24 which folded v_inset into the line-height contribution.
#[test]
fn measure_recomputes_when_child_layout_dirty_under_same_proposal() {
    let constraints = LayoutConstraints {
        max_width: 240.0,
        max_height: 120.0,
        viewport_width: 240.0,
        viewport_height: 120.0,
        percent_base_width: Some(240.0),
        percent_base_height: Some(120.0),
    };

    let mut arena = new_test_arena();
    let mut wrapper = Element::new(0.0, 0.0, 0.0, 0.0);
    let mut wrapper_style = Style::new();
    wrapper_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flow().row().no_wrap().into()),
    );
    wrapper_style.insert(PropertyId::Width, ParsedValue::Auto);
    wrapper_style.insert(PropertyId::Height, ParsedValue::Auto);
    wrapper.apply_style(wrapper_style);
    let wrapper_key = commit_element(&mut arena, Box::new(wrapper));

    let child_key_val =
        commit_child(&mut arena, wrapper_key, Box::new(Text::from_content("a")));
    arena.with_element_taken(wrapper_key, |el, a| el.measure(constraints, a));
    let before_width = {
        let w = crate::view::test_support::get_element::<Element>(&arena, wrapper_key);
        w.measured_size().0
    };

    {
        let mut child =
            crate::view::test_support::get_element_mut::<Text>(&arena, child_key_val);
        child.set_text("a much longer child");
    }

    arena.with_element_taken(wrapper_key, |el, a| el.measure(constraints, a));
    let after_width = {
        let w = crate::view::test_support::get_element::<Element>(&arena, wrapper_key);
        w.measured_size().0
    };
    assert!(after_width > before_width + 1.0);
}

#[test]
fn flex_place_retains_measured_plan_for_placement_only_reuse() {
    let constraints = LayoutConstraints {
        max_width: 240.0,
        max_height: 120.0,
        viewport_width: 240.0,
        viewport_height: 120.0,
        percent_base_width: Some(240.0),
        percent_base_height: Some(120.0),
    };
    let placement = LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 240.0,
        available_height: 120.0,
        viewport_width: 240.0,
        viewport_height: 120.0,
        percent_base_width: Some(240.0),
        percent_base_height: Some(120.0),
    };

    let mut arena = new_test_arena();
    let mut root = Element::new(0.0, 0.0, 240.0, 120.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flex().row().into()),
    );
    root.apply_style(style);
    let root_key = commit_element(&mut arena, Box::new(root));
    commit_child(
        &mut arena,
        root_key,
        Box::new(Text::from_content("retained flex plan")),
    );

    arena.with_element_taken(root_key, |root, arena| {
        root.measure(constraints, arena);
        root.place(placement, arena);
    });

    let root = crate::view::test_support::get_element::<Element>(&arena, root_key);
    assert!(root.flex_info.is_some());
}
