use super::*;

#[test]
fn percent_width_uses_layout_override_without_mutating_measured_width() {
    let mut a = arena();
    let mut text = Text::from_content("123");
    text.set_width(10.0);

    text.measure(
        LayoutConstraints {
            max_width: 200.0,
            max_height: 40.0,
            viewport_width: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(40.0),
            viewport_height: 40.0,
        },
        &mut a,
    );
    assert_eq!(text.measured_size().0, 10.0);

    text.set_layout_width(80.0);
    text.place(
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 40.0,
            viewport_width: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(40.0),
            viewport_height: 40.0,
        },
        &mut a,
    );
    assert_eq!(text.box_model_snapshot().width, 80.0);

    text.measure(
        LayoutConstraints {
            max_width: 200.0,
            max_height: 40.0,
            viewport_width: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(40.0),
            viewport_height: 40.0,
        },
        &mut a,
    );
    assert_eq!(text.measured_size().0, 10.0);
}

#[test]
fn auto_width_for_cjk_text_is_not_underestimated() {
    let mut a = arena();
    let mut text = Text::from_content("This is a Chinese text segment");
    text.measure(
        LayoutConstraints {
            max_width: 300.0,
            max_height: 200.0,
            viewport_width: 300.0,
            percent_base_width: Some(300.0),
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
            available_width: 300.0,
            available_height: 200.0,
            viewport_width: 300.0,
            percent_base_width: Some(300.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        },
        &mut a,
    );
    let snapshot = text.box_model_snapshot();
    assert!(snapshot.width >= 80.0);
}

#[test]
fn auto_width_with_space_includes_following_word() {
    let mut a = arena();
    let mut single = Text::from_content("Click");
    single.measure(
        LayoutConstraints {
            max_width: 400.0,
            max_height: 200.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        },
        &mut a,
    );
    single.place(
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 400.0,
            available_height: 200.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        },
        &mut a,
    );

    let mut spaced = Text::from_content("Click Me");
    spaced.measure(
        LayoutConstraints {
            max_width: 400.0,
            max_height: 200.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        },
        &mut a,
    );
    spaced.place(
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 400.0,
            available_height: 200.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        },
        &mut a,
    );

    let a = single.box_model_snapshot().width;
    let b = spaced.box_model_snapshot().width;
    assert!(
        b > a,
        "expected \"Click Me\" width > \"Click\", got {b} <= {a}"
    );
}

#[test]
fn auto_measured_text_size_preserves_fractional_precision() {
    let mut a = arena();
    let mut text = Text::from_content("rounded measurement");
    text.measure(
        LayoutConstraints {
            max_width: 300.0,
            max_height: 200.0,
            viewport_width: 300.0,
            percent_base_width: Some(300.0),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        },
        &mut a,
    );

    let (width, height) = text.measured_size();
    assert!(width.fract() > 0.0 || height.fract() > 0.0);
}

#[test]
fn auto_width_uses_precise_text_width_before_final_pixel_rounding() {
    let mut a = arena();
    let content = "Option 4";
    let (precise_width, precise_height) = measure_text_size(
        content,
        None,
        false,
        16.0,
        1.25,
        400,
        InlineIfcAlignment::Left,
        &[],
    );
    assert!(precise_width.fract() > 0.0);

    let mut text = Text::from_content(content);
    text.measure(
        LayoutConstraints {
            max_width: precise_width.ceil(),
            max_height: 200.0,
            viewport_width: precise_width.ceil(),
            percent_base_width: Some(precise_width.ceil()),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        },
        &mut a,
    );

    let (measured_width, measured_height) = text.measured_size();
    assert!(
        (measured_width - precise_width).abs() < 0.01,
        "expected precise width {precise_width}, got {measured_width}"
    );
    assert!(
        (measured_height - precise_height).abs() < 0.01,
        "expected single-line height {precise_height}, got {measured_height}"
    );
}

#[test]
fn text_measure_does_not_split_word_when_available_width_matches_precise_measurement() {
    let mut a = arena();
    let content = "Reset";
    let (precise_width, _) = measure_text_size(
        content,
        None,
        false,
        16.0,
        1.25,
        400,
        InlineIfcAlignment::Left,
        &[],
    );
    let mut text = Text::from_content(content);
    text.measure(
        LayoutConstraints {
            max_width: precise_width,
            max_height: 200.0,
            viewport_width: 400.0,
            viewport_height: 200.0,
            percent_base_width: Some(precise_width),
            percent_base_height: Some(200.0),
        },
        &mut a,
    );

    let (_, measured_height) = text.measured_size();
    assert!(
        measured_height <= 20.1,
        "word should stay on one line when precise width fits, height={measured_height}"
    );
}

#[test]
fn auto_height_uses_precise_auto_width_to_avoid_spurious_wrap_height() {
    let mut a = arena();
    let content = "Start";
    let (precise_width, precise_height) = measure_text_size(
        content,
        None,
        false,
        16.0,
        1.25,
        400,
        InlineIfcAlignment::Left,
        &[],
    );
    let mut text = Text::from_content(content);
    text.measure(
        LayoutConstraints {
            max_width: precise_width.ceil(),
            max_height: 200.0,
            viewport_width: 400.0,
            percent_base_width: Some(precise_width.ceil()),
            percent_base_height: Some(200.0),
            viewport_height: 200.0,
        },
        &mut a,
    );

    assert!((text.measured_size().1 - precise_height).abs() < 0.01);
}

#[test]
fn placed_text_box_preserves_fractional_layout_coordinates() {
    let mut a = arena();
    let mut text = Text::new(1.4, 2.6, 10.4, 20.6, "demo");
    text.place(
        LayoutPlacement {
            parent_x: 3.2,
            parent_y: 4.7,
            visual_offset_x: 0.3,
            visual_offset_y: -0.2,
            available_width: 100.0,
            available_height: 100.0,
            viewport_width: 100.0,
            percent_base_width: Some(100.0),
            percent_base_height: Some(100.0),
            viewport_height: 100.0,
        },
        &mut a,
    );

    let snapshot = text.box_model_snapshot();
    assert!((snapshot.x - 4.9).abs() < 0.01);
    assert!((snapshot.y - 7.1).abs() < 0.01);
    assert!((snapshot.width - 10.4).abs() < 0.01);
    assert!((snapshot.height - 20.6).abs() < 0.01);
}
