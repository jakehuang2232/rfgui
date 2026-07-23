use super::*;

#[test]
fn layout_clamps_to_parent_available_area() {
    let mut a = arena();
    let mut text = Text::new(0.0, 0.0, 10_000.0, 10_000.0, "demo");
    text.set_position(8.0, 4.0);
    text.measure(
        LayoutConstraints {
            max_width: 240.0,
            max_height: 140.0,
            viewport_width: 240.0,
            percent_base_width: Some(240.0),
            percent_base_height: Some(140.0),
            viewport_height: 140.0,
        },
        &mut a,
    );
    text.place(
        LayoutPlacement {
            parent_x: 40.0,
            parent_y: 40.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 240.0,
            available_height: 140.0,
            viewport_width: 240.0,
            percent_base_width: Some(240.0),
            percent_base_height: Some(140.0),
            viewport_height: 140.0,
        },
        &mut a,
    );

    let snapshot = text.box_model_snapshot();
    assert_eq!(snapshot.x, 48.0);
    assert_eq!(snapshot.y, 44.0);
    assert_eq!(snapshot.width, 232.0);
    assert_eq!(snapshot.height, 136.0);
}

#[test]
fn text_wraps_when_parent_width_is_constrained() {
    let mut a = arena();
    let mut text = Text::from_content("123456789012345678901234567890");
    text.set_width(60.0);
    text.set_auto_height(true);
    text.measure(
        LayoutConstraints {
            max_width: 60.0,
            max_height: 200.0,
            viewport_width: 60.0,
            percent_base_width: Some(60.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        },
        &mut a,
    );
    text.place(
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 60.0,
            available_height: 200.0,
            viewport_width: 60.0,
            percent_base_width: Some(60.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        },
        &mut a,
    );

    let snapshot = text.box_model_snapshot();
    assert_eq!(snapshot.width, 60.0);
    assert!(snapshot.height > 20.0);
}

#[test]
fn text_wrap_can_be_disabled_via_text_wrap_style() {
    let mut a = arena();
    let mut text = Text::from_content("123456789012345678901234567890");
    text.set_width(60.0);
    text.set_auto_height(true);
    text.set_text_wrap(TextWrap::NoWrap);
    text.measure(
        LayoutConstraints {
            max_width: 60.0,
            max_height: 200.0,
            viewport_width: 60.0,
            percent_base_width: Some(60.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        },
        &mut a,
    );
    text.place(
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 60.0,
            available_height: 200.0,
            viewport_width: 60.0,
            percent_base_width: Some(60.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        },
        &mut a,
    );

    let snapshot = text.box_model_snapshot();
    assert_eq!(snapshot.width, 60.0);
    assert!(snapshot.height <= 20.0);
}

#[test]
fn text_does_not_wrap_when_parent_width_is_unresolved() {
    let mut a = arena();
    let mut text = Text::from_content("Click Me Click Me");
    text.set_auto_width(true);
    text.set_auto_height(true);
    text.measure(
        LayoutConstraints {
            max_width: 60.0,
            max_height: 200.0,
            viewport_width: 60.0,
            percent_base_width: None,
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        },
        &mut a,
    );
    text.place(
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 400.0,
            available_height: 200.0,
            viewport_width: 400.0,
            percent_base_width: None,
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        },
        &mut a,
    );

    let snapshot = text.box_model_snapshot();
    assert!(snapshot.width > 60.0);
    assert!(snapshot.height <= 24.0);
}

#[test]
fn text_reflows_when_parent_width_changes() {
    let mut a = arena();
    let mut text =
        Text::from_content("This is a long sentence that should wrap to multiple lines.");
    text.set_auto_width(true);
    text.set_auto_height(true);

    text.measure(
        LayoutConstraints {
            max_width: 220.0,
            max_height: 300.0,
            viewport_width: 220.0,
            percent_base_width: Some(220.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        },
        &mut a,
    );
    text.place(
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 220.0,
            available_height: 300.0,
            viewport_width: 220.0,
            percent_base_width: Some(220.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        },
        &mut a,
    );
    let h_wide = text.box_model_snapshot().height;

    text.measure(
        LayoutConstraints {
            max_width: 90.0,
            max_height: 300.0,
            viewport_width: 90.0,
            percent_base_width: Some(90.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        },
        &mut a,
    );
    text.place(
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 90.0,
            available_height: 300.0,
            viewport_width: 90.0,
            percent_base_width: Some(90.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        },
        &mut a,
    );
    let h_narrow = text.box_model_snapshot().height;

    assert!(
        h_narrow > h_wide,
        "expected text to reflow when parent width shrinks: narrow={h_narrow}, wide={h_wide}"
    );
}

#[test]
fn text_wrap_epsilon_keeps_snug_content_on_one_line() {
    let mut a = arena();
    let content = "epsilon slack fit";
    let (intrinsic_width, single_line_height) = measure_text_size(
        content,
        None,
        false,
        16.0,
        1.25,
        400,
        InlineIfcAlignment::Left,
        &[],
    );
    let snug_width = intrinsic_width - 1.0;
    let mut text = Text::from_content(content);
    text.measure(
        LayoutConstraints {
            max_width: snug_width,
            max_height: 200.0,
            viewport_width: snug_width,
            viewport_height: 200.0,
            percent_base_width: Some(snug_width),
            percent_base_height: Some(200.0),
        },
        &mut a,
    );
    let (_, measured_height) = text.measured_size();
    assert!(
        (measured_height - single_line_height).abs() < 0.5,
        "content within the 2px wrap slack must stay on one line, height={measured_height}, single={single_line_height}"
    );
}
