//! Text unit tests.

#![cfg(test)]

use super::{ElementTrait, Text, measure_text_size};
use crate::style::{
    Color, ColorLike, FontFamily, FontSize, FontWeight, ParsedValue, PropertyId, Style, TextWrap,
    VerticalAlign,
};
use crate::view::base_component::{
    DirtyFlags, LayoutConstraints, LayoutPlacement, Layoutable, Renderable, UiBuildContext,
};
use crate::view::frame_graph::FrameGraph;
use crate::view::inline_formatting_context::InlineIfcAlignment;
use crate::view::node_arena::NodeArena;

fn arena() -> NodeArena {
    NodeArena::new()
}

fn place_text_for_read_only_ifc_test(text: &mut Text, width: f32, height: f32) {
    let mut a = arena();
    text.measure(
        LayoutConstraints {
            max_width: width,
            max_height: height,
            viewport_width: width,
            percent_base_width: Some(width),
            percent_base_height: Some(height),
            viewport_height: height,
        },
        &mut a,
    );
    text.place(
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: width,
            available_height: height,
            viewport_width: width,
            percent_base_width: Some(width),
            percent_base_height: Some(height),
            viewport_height: height,
        },
        &mut a,
    );
}

fn build_text_for_read_only_ifc_test(text: &mut Text) -> Vec<String> {
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(200, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    let mut arena = arena();

    text.build(&mut graph, &mut arena, ctx);

    graph
        .pass_descriptors()
        .into_iter()
        .map(|descriptor| descriptor.name.to_string())
        .collect()
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
fn shared_measure_cache_separates_wrap_and_nowrap_layouts() {
    let content = "shared cache wraps this sentence across several lines";
    let width = Some(82.0);
    let font_size = 14.0;
    let line_height = 1.25;
    let font_weight = 400;
    let align = InlineIfcAlignment::Left;
    let fonts: Vec<String> = Vec::new();

    let (_, wrap_height_first) = measure_text_size(
        content,
        width,
        true,
        font_size,
        line_height,
        font_weight,
        align,
        &fonts,
    );
    let (_, nowrap_height_second) = measure_text_size(
        content,
        width,
        false,
        font_size,
        line_height,
        font_weight,
        align,
        &fonts,
    );
    assert!(
        wrap_height_first > nowrap_height_second + 1.0,
        "nowrap measurement must not reuse the prior wrapped cache entry"
    );

    let content = "shared cache nowrap first still wraps later";
    let (_, nowrap_height_first) = measure_text_size(
        content,
        width,
        false,
        font_size,
        line_height,
        font_weight,
        align,
        &fonts,
    );
    let (_, wrap_height_second) = measure_text_size(
        content,
        width,
        true,
        font_size,
        line_height,
        font_weight,
        align,
        &fonts,
    );
    assert!(
        wrap_height_second > nowrap_height_first + 1.0,
        "wrapped measurement must not reuse the prior nowrap cache entry"
    );
}

#[test]
fn per_text_layout_cache_only_retains_recent_widths() {
    let mut text = Text::from_content("bounded per-node layout cache");
    for width in 20..40 {
        let _ = text.relayout_from_base(Some(width as f32), true);
    }

    assert_eq!(text.layout_cache.len(), 4);
}

#[test]
fn text_caret_position_uses_screen_coordinates_once() {
    let mut a = arena();
    let mut text = Text::from_content("XYZ");
    text.measure(
        LayoutConstraints {
            max_width: 120.0,
            max_height: 40.0,
            viewport_width: 300.0,
            viewport_height: 200.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(40.0),
        },
        &mut a,
    );
    text.place(
        LayoutPlacement {
            parent_x: 42.0,
            parent_y: 24.0,
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
fn text_measure_clears_layout_dirty() {
    let mut a = arena();
    let mut text = Text::from_content("measured text");
    text.measure(
        LayoutConstraints {
            max_width: 200.0,
            max_height: 120.0,
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
fn clean_text_measure_with_same_constraints_skips_relayout() {
    let mut a = arena();
    let mut text = Text::from_content("cached text");
    let constraints = LayoutConstraints {
        max_width: 200.0,
        max_height: 120.0,
        viewport_width: 200.0,
        viewport_height: 120.0,
        percent_base_width: Some(200.0),
        percent_base_height: Some(120.0),
    };
    text.measure(constraints, &mut a);

    crate::view::base_component::reset_text_measure_profile();
    crate::view::base_component::set_text_measure_profile_enabled(true);
    text.measure(constraints, &mut a);
    crate::view::base_component::set_text_measure_profile_enabled(false);
    let profile = crate::view::base_component::take_text_measure_profile();

    assert_eq!(profile.relayout_from_base_calls, 0);
    assert_eq!(profile.measure_text_layout_calls, 0);
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

#[test]
fn retained_transform_text_bounds_apply_nonzero_inherited_paint_offset() {
    let text = Text::new(3.25, 4.5, 10.0, 5.0, "offset bounds");
    let arena = arena();
    let exact = text
        .retained_transform_output_bounds(&arena, [0.2, -0.3])
        .expect("Text explicitly owns exact transformed-ancestor coverage");
    assert_eq!(
        [exact.x, exact.y, exact.width, exact.height].map(f32::to_bits),
        [3.45, 4.2, 10.0, 5.0].map(f32::to_bits)
    );
    let legacy = text
        .legacy_transform_output_bounds(&arena, [0.2, -0.3])
        .expect("legacy Text coverage");
    assert_eq!(
        [legacy.x, legacy.y, legacy.width, legacy.height].map(f32::to_bits),
        [exact.x, exact.y, exact.width, exact.height].map(f32::to_bits)
    );
}

#[test]
fn text_build_emits_prepared_input_pass_from_shaped_context() {
    let mut text = Text::new(0.0, 0.0, 92.0, 80.0, "prepared text");
    place_text_for_read_only_ifc_test(&mut text, 92.0, 120.0);

    let context = text
        .shaped_context_for_test()
        .expect("measure should install a shaped context")
        .clone();
    let staging = text
        .shaped_staging_input_for_test([0.0, 0.0])
        .expect("placed Text should stage glyphs");
    assert_eq!(
        staging.glyphs.len(),
        context.text_pass_paint_input().glyphs.len(),
        "render must stage exactly the glyphs of the measure-shaped context"
    );
    assert!(
        staging
            .glyphs
            .iter()
            .all(|glyph| glyph.paint.color == text.color.to_rgba_f32()),
        "live color must be injected at bridge time"
    );

    let pass_names = build_text_for_read_only_ifc_test(&mut text);
    assert_eq!(pass_names.len(), 1);
    assert!(pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"));
}

#[test]
fn text_build_skips_non_renderable_text() {
    let mut empty = Text::new(0.0, 0.0, 92.0, 80.0, "");
    place_text_for_read_only_ifc_test(&mut empty, 92.0, 120.0);
    assert!(build_text_for_read_only_ifc_test(&mut empty).is_empty());

    let mut transparent = Text::new(0.0, 0.0, 92.0, 80.0, "transparent candidate text");
    transparent.set_opacity(0.0);
    place_text_for_read_only_ifc_test(&mut transparent, 92.0, 120.0);
    assert!(build_text_for_read_only_ifc_test(&mut transparent).is_empty());

    let mut hidden = Text::new(0.0, 0.0, 92.0, 80.0, "hidden candidate text");
    place_text_for_read_only_ifc_test(&mut hidden, 92.0, 120.0);
    hidden.layout_state.should_render = false;
    assert!(build_text_for_read_only_ifc_test(&mut hidden).is_empty());
}

#[test]
fn text_build_promoted_visibility_uses_neutral_opacity_authority() {
    let stable_id = 0x7a11;
    let mut text = Text::new_with_id(stable_id, 0.0, 0.0, 92.0, 80.0, "promoted transparent text");
    text.set_opacity(0.0);
    place_text_for_read_only_ifc_test(&mut text, 92.0, 120.0);

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(200, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    ctx.set_promoted_runtime(
        std::sync::Arc::new(rustc_hash::FxHashSet::from_iter([stable_id])),
        Default::default(),
        Default::default(),
    );
    let mut arena = arena();
    text.build(&mut graph, &mut arena, ctx);

    assert!(graph.pass_descriptors().iter().any(|descriptor| {
        descriptor
            .name
            .ends_with("render_pass::text_pass::TextPreparedInputPass")
    }));
}

#[test]
fn text_opacity_marks_paint_and_composite_without_layout() {
    let mut text = Text::new(0.0, 0.0, 92.0, 80.0, "opacity");
    text.clear_local_dirty_flags(DirtyFlags::ALL);

    text.set_opacity(0.5);

    let dirty = text.local_dirty_flags();
    assert!(dirty.contains(DirtyFlags::PAINT));
    assert!(dirty.contains(DirtyFlags::COMPOSITE));
    assert!(!dirty.intersects(DirtyFlags::LAYOUT));
}

#[test]
fn text_align_shifts_glyphs_and_caret_consistently() {
    let build_aligned = |align: crate::style::TextAlign| {
        let mut text = Text::new(0.0, 0.0, 220.0, 40.0, "align me");
        text.set_text_align(align);
        place_text_for_read_only_ifc_test(&mut text, 220.0, 120.0);
        let staging = text
            .shaped_staging_input_for_test([0.0, 0.0])
            .expect("aligned Text should stage glyphs");
        let min_glyph_x = staging
            .glyphs
            .iter()
            .map(|glyph| glyph.paint.local_pos[0])
            .fold(f32::MAX, f32::min);
        let (caret_x, _, _) = text
            .local_char_to_screen_position(0)
            .expect("caret at first char");
        (min_glyph_x, caret_x)
    };

    let (left_glyph_x, left_caret_x) = build_aligned(crate::style::TextAlign::Left);
    let (center_glyph_x, center_caret_x) = build_aligned(crate::style::TextAlign::Center);
    let (right_glyph_x, right_caret_x) = build_aligned(crate::style::TextAlign::Right);
    assert!(center_glyph_x > left_glyph_x + 1.0, "center shifts glyphs");
    assert!(right_glyph_x > center_glyph_x + 1.0, "right shifts further");
    assert!(
        (left_caret_x - left_glyph_x).abs() < 1.0
            && (center_caret_x - center_glyph_x).abs() < 1.0
            && (right_caret_x - right_glyph_x).abs() < 1.0,
        "caret must sit on the aligned glyphs: caret=({left_caret_x},{center_caret_x},{right_caret_x}) glyphs=({left_glyph_x},{center_glyph_x},{right_glyph_x})"
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

#[test]
fn text_color_change_repaints_without_reshaping() {
    let mut a = arena();
    let mut text = Text::from_content("recolor me");
    let constraints = LayoutConstraints {
        max_width: 200.0,
        max_height: 60.0,
        viewport_width: 200.0,
        viewport_height: 60.0,
        percent_base_width: Some(200.0),
        percent_base_height: Some(60.0),
    };
    text.measure(constraints, &mut a);
    let before = text
        .shaped_context_for_test()
        .expect("shaped context installed")
        .clone();

    text.set_color(Color::rgba(200, 30, 30, 255));
    text.measure(constraints, &mut a);
    let after = text
        .shaped_context_for_test()
        .expect("shaped context installed")
        .clone();
    assert!(
        std::sync::Arc::ptr_eq(&before, &after),
        "color change must not reshape the text"
    );
    let staging = text
        .shaped_staging_input_for_test([0.0, 0.0])
        .expect("staged glyphs");
    assert!(
        staging
            .glyphs
            .iter()
            .all(|glyph| glyph.paint.color == text.color.to_rgba_f32()),
        "staged glyphs must carry the new color"
    );
}
