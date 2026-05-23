//! Text unit tests.

#![cfg(test)]

use super::{ElementTrait, Text, measure_text_size};
use crate::style::{
    Color, FontFamily, FontSize, FontWeight, ParsedValue, PropertyId, Style, TextWrap,
    VerticalAlign,
};
use crate::view::base_component::{
    DirtyFlags, InlineMeasureContext, LayoutConstraints, LayoutPlacement, Layoutable,
};
use crate::view::node_arena::NodeArena;
use crate::view::text_layout::TextLayoutAlignment;

fn arena() -> NodeArena {
    NodeArena::new()
}

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
fn inline_text_caret_position_uses_fragment_screen_coordinates_once() {
    let mut a = arena();
    let mut text = Text::from_content("XYZ");
    text.measure_inline(
        InlineMeasureContext {
            first_available_width: 120.0,
            full_available_width: 120.0,
            available_height: 1_000_000.0,
            viewport_width: 300.0,
            viewport_height: 200.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(40.0),
        },
        &mut a,
    );
    text.place_inline(
        crate::view::base_component::InlinePlacement {
            node_index: 0,
            x: 42.0,
            y: 24.0,
            offset_x: 0.0,
            offset_y: 0.0,
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 120.0,
            available_height: 40.0,
            viewport_width: 300.0,
            viewport_height: 200.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(40.0),
        },
        &mut a,
    );

    let snapshot = text.box_model_snapshot();
    let (x, y, _) = text
        .local_char_to_screen_position(2)
        .expect("caret position");
    assert!(
        x >= snapshot.x - 0.5 && x <= snapshot.x + snapshot.width + 0.5,
        "caret x should stay inside text bounds: x={x}, snapshot=({}, {})",
        snapshot.x,
        snapshot.width
    );
    assert!(
        y >= snapshot.y - 0.5 && y <= snapshot.y + snapshot.height + 0.5,
        "caret y should stay inside text bounds: y={y}, snapshot=({}, {})",
        snapshot.y,
        snapshot.height
    );
    assert_eq!(text.screen_position_to_local_char(x, y), Some(2));
}

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
fn inline_measure_clears_layout_dirty() {
    let mut a = arena();
    let mut text = Text::from_content("inline text");
    text.measure_inline(
        InlineMeasureContext {
            first_available_width: 200.0,
            full_available_width: 200.0,
            available_height: 1_000_000.0,
            viewport_width: 200.0,
            viewport_height: 120.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(120.0),
        },
        &mut a,
    );

    assert!(!text.local_dirty_flags().intersects(DirtyFlags::LAYOUT));
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
        TextLayoutAlignment::Left,
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
fn inline_measure_does_not_split_word_when_available_width_matches_precise_measurement() {
    let mut a = arena();
    let content = "Reset";
    let (precise_width, _) = measure_text_size(
        content,
        None,
        false,
        16.0,
        1.25,
        400,
        TextLayoutAlignment::Left,
        &[],
    );
    let mut text = Text::from_content(content);
    text.measure_inline(
        InlineMeasureContext {
            first_available_width: precise_width,
            full_available_width: precise_width,
            available_height: 1_000_000.0,
            viewport_width: 400.0,
            viewport_height: 200.0,
            percent_base_width: Some(precise_width),
            percent_base_height: Some(200.0),
        },
        &mut a,
    );

    let nodes = text.get_inline_nodes_size(&a);
    assert_eq!(
        nodes.len(),
        1,
        "word should stay on one line when precise width fits"
    );
}

#[test]
fn inline_wrap_uses_one_fragment_per_wrapped_line() {
    let mut a = arena();
    let content = "alpha beta gamma delta";
    let available_width = 64.0;
    let mut text = Text::from_content(content);
    text.measure_inline(
        InlineMeasureContext {
            first_available_width: available_width,
            full_available_width: available_width,
            available_height: 1_000_000.0,
            viewport_width: 400.0,
            viewport_height: 200.0,
            percent_base_width: Some(available_width),
            percent_base_height: Some(200.0),
        },
        &mut a,
    );

    let nodes = text.get_inline_nodes_size(&a);
    assert!(
        nodes.len() > 1,
        "expected wrapped text to produce multiple line fragments"
    );
    assert!(
        nodes
            .iter()
            .all(|node| node.width <= available_width + 0.01)
    );
}

#[test]
fn inline_wrap_uses_first_available_width_for_first_fragment() {
    let mut a = arena();
    let content = "alpha beta gamma";
    let mut text = Text::from_content(content);
    text.measure_inline(
        InlineMeasureContext {
            first_available_width: 48.0,
            full_available_width: 160.0,
            available_height: 1_000_000.0,
            viewport_width: 200.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        },
        &mut a,
    );

    let nodes = text.get_inline_nodes_size(&a);
    assert!(
        nodes.len() >= 2,
        "expected first-line constraint to force wrapping"
    );
    assert!(nodes[0].width <= 48.01);
}

#[test]
fn inline_wrap_moves_first_token_to_full_width_line_when_remaining_width_too_small() {
    let mut a = arena();
    let mut text = Text::from_content("{{API_HOST}}");
    text.measure_inline(
        InlineMeasureContext {
            first_available_width: 12.0,
            full_available_width: 160.0,
            available_height: 1_000_000.0,
            viewport_width: 200.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        },
        &mut a,
    );

    let fragments: Vec<_> = text
        .inline_plan
        .as_ref()
        .expect("inline plan should be built")
        .runs
        .iter()
        .map(|fragment| fragment.content.clone())
        .collect();
    assert_eq!(
        fragments.first().map(String::as_str),
        Some("{{API_HOST}}"),
        "first inline fragment should move the whole first token to a full-width line, got {fragments:?}",
    );
}

#[test]
fn wrapped_inline_fragments_force_break_so_parent_does_not_pack_them() {
    let mut a = arena();
    let mut text = Text::from_content("note note note note note note note");
    text.set_font_size(14.0);
    text.measure_inline(
        InlineMeasureContext {
            first_available_width: 52.0,
            full_available_width: 52.0,
            available_height: 1_000_000.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(52.0),
            percent_base_height: Some(600.0),
        },
        &mut a,
    );
    let nodes = text.get_inline_nodes_size(&a);
    assert!(
        nodes.len() > 1,
        "test fixture should produce multiple wrap fragments"
    );
    let last = nodes.len() - 1;
    for (idx, node) in nodes.iter().enumerate() {
        if idx < last {
            assert!(
                node.force_break_after,
                "fragment {idx} (not last) must force a break — otherwise parent inline solver \
                 packs adjacent pre-wrapped fragments side-by-side"
            );
        } else {
            assert!(
                !node.force_break_after,
                "last fragment must NOT force a break so subsequent inline siblings can \
                 continue on the same line"
            );
        }
    }
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
        TextLayoutAlignment::Left,
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
