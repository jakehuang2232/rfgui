use super::*;

#[test]
fn text_style_cold_path_uses_computed_bridge_for_supported_fields() {
    let mut style = Style::new();
    style.insert(
        PropertyId::Color,
        ParsedValue::Color(Color::rgb(0x12, 0x34, 0x56).into()),
    );
    style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::em(1.5)),
    );
    style.insert(
        PropertyId::FontWeight,
        ParsedValue::FontWeight(FontWeight::new(650)),
    );
    style.insert(
        PropertyId::FontFamily,
        ParsedValue::FontFamily(FontFamily::new(["Inter", "system-ui"])),
    );
    style.insert(
        PropertyId::TextWrap,
        ParsedValue::TextWrap(TextWrap::NoWrap),
    );

    let mut inherited_style = Style::new();
    inherited_style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::px(20.0)),
    );
    let inherited = crate::view::renderer_adapter::StyleCascadeContext::from_viewport_style(
        &inherited_style,
        0.0,
        0.0,
    );

    let mut text = Text::from_content("computed");
    text.apply_style_cold(Some(&style), &inherited)
        .expect("text computed style bridge should apply");

    assert_eq!(text.color.to_rgba_u8(), [0x12, 0x34, 0x56, 0xff]);
    assert!((text.font_size() - 30.0).abs() < f32::EPSILON);
    assert_eq!(text.font_weight, 650);
    assert_eq!(text.font_families, vec!["Inter", "system-ui"]);
    assert_eq!(text.text_wrap(), TextWrap::NoWrap);
}

#[test]
fn text_style_transform_lifecycle_resolves_and_resets_real_state() {
    let mut style = Style::new();
    style.set_transform(Transform::new([Translate::xy(
        Length::px(7.0),
        Length::px(11.0),
    )]));
    style.set_transform_origin(TransformOrigin::px(3.0, 5.0));
    let inherited = crate::view::renderer_adapter::StyleCascadeContext::from_viewport_style(
        &Style::new(),
        320.0,
        240.0,
    );
    let mut text = Text::new(10.0, 20.0, 80.0, 24.0, "transform");

    text.apply_style_cold(Some(&style), &inherited)
        .expect("Text transform style");
    place_text_for_read_only_ifc_test(&mut text, 320.0, 240.0);

    assert_eq!(text.transform().as_slice().len(), 1);
    assert_eq!(text.transform_origin(), TransformOrigin::px(3.0, 5.0));
    let matrix = text
        .compositor_viewport_transform_snapshot()
        .expect("resolved Text transform")
        .to_cols_array();
    assert_eq!(matrix[12].to_bits(), 7.0_f32.to_bits());
    assert_eq!(matrix[13].to_bits(), 11.0_f32.to_bits());

    text.apply_style_incremental(None, &inherited);
    assert!(text.transform().as_slice().is_empty());
    assert_eq!(text.transform_origin(), TransformOrigin::center());
    assert!(text.compositor_viewport_transform_snapshot().is_none());
}

#[test]
fn text_transform_setters_mark_runtime_and_update_exact_bounds() {
    let mut text = Text::new(10.0, 20.0, 80.0, 24.0, "transform");
    place_text_for_read_only_ifc_test(&mut text, 320.0, 240.0);
    text.clear_local_dirty_flags(DirtyFlags::ALL);

    text.set_transform(Transform::new([Scale::uniform(2.0)]));

    assert!(text.local_dirty_flags().contains(DirtyFlags::RUNTIME));
    let arena = arena();
    let source = text
        .retained_transform_surface_bounds(&arena, [0.0, 0.0])
        .expect("direct Text transform source bounds");
    let output = text
        .retained_transform_output_bounds(&arena, [0.0, 0.0])
        .expect("direct Text transform output bounds");
    assert_eq!(source.width.to_bits(), 80.0_f32.to_bits());
    assert_eq!(source.height.to_bits(), 24.0_f32.to_bits());
    assert_eq!(output.width.to_bits(), 160.0_f32.to_bits());
    assert_eq!(output.height.to_bits(), 48.0_f32.to_bits());
    let seed = text
        .retained_transform_raster_seed_bounds()
        .expect("direct Text transform raster seed");
    assert_eq!(
        [seed.x, seed.y, seed.width, seed.height].map(f32::to_bits),
        [source.x, source.y, source.width, source.height].map(f32::to_bits)
    );
}

#[test]
fn text_rotate_matrix_and_nonfinite_transform_observations_fail_closed() {
    let mut text = Text::new(10.0, 20.0, 80.0, 24.0, "transform");
    place_text_for_read_only_ifc_test(&mut text, 320.0, 240.0);
    let arena = arena();

    text.set_transform(Transform::new([Rotate::deg(90.0)]));
    let rotated = text
        .retained_transform_output_bounds(&arena, [0.0, 0.0])
        .expect("finite rotation bounds");
    assert!((rotated.width - 24.0).abs() < 0.001);
    assert!((rotated.height - 80.0).abs() < 0.001);

    text.set_transform(Transform::new([TransformEntry::from_matrix([
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        f32::NAN,
        0.0,
        0.0,
        1.0,
    ])]));
    assert!(text.has_retained_transform_surface());
    assert!(text.compositor_viewport_transform_snapshot().is_some());
    assert!(
        text.retained_transform_output_bounds(&arena, [0.0, 0.0])
            .is_none(),
        "non-finite geometry must not produce a sealable output bound"
    );
}

#[test]
fn text_style_cold_path_applies_authored_line_height_and_vertical_align() {
    let mut style = Style::new();
    style.insert(
        PropertyId::LineHeight,
        ParsedValue::LineHeight(crate::style::LineHeight::new(1.6)),
    );
    style.insert(
        PropertyId::VerticalAlign,
        ParsedValue::VerticalAlign(VerticalAlign::Top),
    );
    let inherited = crate::view::renderer_adapter::StyleCascadeContext::from_viewport_style(
        &Style::new(),
        0.0,
        0.0,
    );

    let mut text = Text::from_content("authored inline style");
    text.apply_style_cold(Some(&style), &inherited)
        .expect("text computed style bridge should apply");

    assert!((text.line_height_value() - 1.6).abs() < f32::EPSILON);
    assert_eq!(text.vertical_align(), VerticalAlign::Top);
}

#[test]
fn explicit_text_vertical_align_survives_ancestor_recascade() {
    let mut inherited_style = Style::new();
    inherited_style.insert(
        PropertyId::VerticalAlign,
        ParsedValue::VerticalAlign(VerticalAlign::Bottom),
    );
    let inherited = crate::view::renderer_adapter::StyleCascadeContext::from_viewport_style(
        &inherited_style,
        0.0,
        0.0,
    );
    let mut text = Text::from_content("explicit vertical align");
    text.set_vertical_align(VerticalAlign::Top);

    text.apply_inherited(&inherited);

    assert_eq!(text.vertical_align(), VerticalAlign::Top);
}

#[test]
fn text_style_computed_bridge_does_not_apply_unauthored_computed_defaults() {
    let mut style = Style::new();
    style.insert(
        PropertyId::Color,
        ParsedValue::Color(Color::rgb(0x22, 0x44, 0x66).into()),
    );

    let mut inherited_style = Style::new();
    inherited_style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::px(18.0)),
    );
    inherited_style.insert(
        PropertyId::FontWeight,
        ParsedValue::FontWeight(FontWeight::new(500)),
    );
    inherited_style.insert(
        PropertyId::TextWrap,
        ParsedValue::TextWrap(TextWrap::NoWrap),
    );
    let inherited = crate::view::renderer_adapter::StyleCascadeContext::from_viewport_style(
        &inherited_style,
        0.0,
        0.0,
    );

    let mut text = Text::from_content("masked");
    text.apply_style_cold(Some(&style), &inherited)
        .expect("text computed style bridge should apply");

    assert_eq!(text.color.to_rgba_u8(), [0x22, 0x44, 0x66, 0xff]);
    assert!((text.font_size() - 18.0).abs() < f32::EPSILON);
    assert_eq!(text.font_weight, 500);
    assert_eq!(text.text_wrap(), TextWrap::NoWrap);
}

#[test]
fn text_style_computed_bridge_resolves_em_from_computed_parent() {
    let mut style = Style::new();
    style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::em(1.25)),
    );

    let mut inherited_style = Style::new();
    inherited_style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::px(24.0)),
    );
    inherited_style.insert(
        PropertyId::LineHeight,
        ParsedValue::LineHeight(crate::style::LineHeight::new(1.7)),
    );
    inherited_style.insert(
        PropertyId::VerticalAlign,
        ParsedValue::VerticalAlign(VerticalAlign::Bottom),
    );
    let inherited = crate::view::renderer_adapter::StyleCascadeContext::from_viewport_style(
        &inherited_style,
        0.0,
        0.0,
    );

    let mut text = Text::from_content("computed parent");
    text.apply_style_cold(Some(&style), &inherited)
        .expect("text computed style bridge should apply");

    assert!((text.font_size() - 30.0).abs() < f32::EPSILON);
    assert!((text.line_height_value() - 1.7).abs() < f32::EPSILON);
    assert_eq!(text.vertical_align(), VerticalAlign::Bottom);
}
