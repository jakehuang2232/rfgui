use super::*;

#[test]
fn text_transform_style_flows_through_cold_apply_and_reset_lifecycle() {
    let initial_style = TextStylePropSchema {
        transform: Some(Transform::new([Translate::x(Length::px(7.0))])),
        transform_origin: Some(TransformOrigin::px(3.0, 4.0)),
        ..Default::default()
    };
    let mut arena = crate::view::test_support::new_test_arena();
    let root = commit_rsx_tree(
        &mut arena,
        &host_text_node()
            .with_prop("style", initial_style)
            .with_child(RsxNode::text("transform")),
    )[0];
    let text = crate::view::test_support::get_element::<Text>(&arena, root);
    assert_eq!(text.transform().as_slice().len(), 1);
    assert_eq!(text.transform_origin(), TransformOrigin::px(3.0, 4.0));
    drop(text);

    let viewport_style = Style::new();
    let ctx = crate::view::fiber_work::ApplyContext {
        viewport_style: &viewport_style,
        viewport_width: 800.0,
        viewport_height: 600.0,
    };
    arena.with_element_taken(root, |element, arena_ref| {
        assert_eq!(
            element.apply_prop(
                arena_ref,
                root,
                &ctx,
                "style",
                TextStylePropSchema {
                    transform: Some(Transform::new([Scale::uniform(1.5)])),
                    ..Default::default()
                }
                .into_prop_value(),
            ),
            crate::view::fiber_work::PropApplyOutcome::Applied
        );
    });
    assert_eq!(
        crate::view::test_support::get_element::<Text>(&arena, root)
            .transform()
            .as_slice()
            .len(),
        1
    );

    arena.with_element_taken(root, |element, arena_ref| {
        assert_eq!(
            element.reset_prop(arena_ref, root, &ctx, "style"),
            crate::view::fiber_work::PropApplyOutcome::Applied
        );
    });
    let text = crate::view::test_support::get_element::<Text>(&arena, root);
    assert!(text.transform().as_slice().is_empty());
    assert_eq!(text.transform_origin(), TransformOrigin::center());
}

#[test]
fn computed_parent_from_style_cascade_maps_text_cascade_fields() {
    let mut style = Style::new();
    style.insert(
        PropertyId::FontFamily,
        ParsedValue::FontFamily(FontFamily::new(["Inter", "Arial"])),
    );
    style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::px(18.0)),
    );
    style.insert(
        PropertyId::FontWeight,
        ParsedValue::FontWeight(FontWeight::new(650)),
    );
    style.insert(
        PropertyId::Color,
        ParsedValue::Color(Color::rgb(0x12, 0x34, 0x56).into()),
    );
    style.insert(PropertyId::Cursor, ParsedValue::Cursor(Cursor::Pointer));
    style.insert(
        PropertyId::TextWrap,
        ParsedValue::TextWrap(TextWrap::NoWrap),
    );
    style.insert(
        PropertyId::LineHeight,
        ParsedValue::LineHeight(crate::style::LineHeight::new(1.6)),
    );
    style.insert(
        PropertyId::VerticalAlign,
        ParsedValue::VerticalAlign(VerticalAlign::Middle),
    );

    let cascade = StyleCascadeContext::from_viewport_style(&style, 0.0, 0.0);
    let parent = computed_parent_from_style_cascade(&cascade);

    assert_eq!(
        parent.font_families,
        vec!["Inter".to_string(), "Arial".to_string()]
    );
    assert!((parent.font_size - 18.0).abs() < f32::EPSILON);
    assert_eq!(parent.font_weight, 650);
    assert_eq!(parent.color.to_rgba_u8(), [0x12, 0x34, 0x56, 0xff]);
    assert_eq!(parent.cursor, Cursor::Pointer);
    assert_eq!(parent.text_wrap, TextWrap::NoWrap);
    assert!((parent.line_height - 1.6).abs() < f32::EPSILON);
    assert_eq!(parent.vertical_align, VerticalAlign::Middle);
}

#[test]
fn computed_parent_from_style_cascade_falls_back_to_root_font_size() {
    let mut style = Style::new();
    style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::px(20.0)),
    );

    let cascade = StyleCascadeContext::from_viewport_style(&style, 0.0, 0.0);
    let parent = computed_parent_from_style_cascade(&cascade);

    assert!((parent.font_size - 20.0).abs() < f32::EPSILON);
}

#[test]
fn as_text_style_accepts_text_style_props() {
    let value = TextStylePropSchema {
        color: Some(Box::new(IntoColor::<Color>::into_color(Color::hex(
            "#123456",
        )))),
        font_size: Some(FontSize::px(18.0)),
        cursor: Some(Cursor::Text),
        ..empty_text_style()
    }
    .into_prop_value();

    let style = as_text_style(&value, "style").expect("text style should validate");

    assert!(matches!(
        style.get(PropertyId::Color),
        Some(ParsedValue::Color(_))
    ));
    assert!(matches!(
        style.get(PropertyId::FontSize),
        Some(ParsedValue::FontSize(_))
    ));
    assert_eq!(
        style.get(PropertyId::Cursor),
        Some(&ParsedValue::Cursor(Cursor::Text))
    );
}

#[test]
fn text_style_set_rejects_unsupported_normalized_fields() {
    let mut style = Style::new();
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#123456")),
    );

    assert_eq!(
        validate_style::<TextStyleSet>(&style),
        Err(StylePropError::UnsupportedProperty {
            property: PropertyId::BackgroundColor,
        })
    );
}

#[test]
fn as_element_style_accepts_box_style_props() {
    let value = ElementStylePropSchema {
        layout: Some(Layout::Inline),
        background_color: Some(Box::new(IntoColor::<Color>::into_color(Color::hex(
            "#123456",
        )))),
        ..empty_element_style()
    }
    .into_prop_value();

    let style = as_element_style(&value, "style").expect("element style should validate");

    assert_eq!(
        style.get(PropertyId::Layout),
        Some(&ParsedValue::Layout(Layout::Inline))
    );
    assert!(matches!(
        style.get(PropertyId::BackgroundColor),
        Some(ParsedValue::Color(_))
    ));
}
