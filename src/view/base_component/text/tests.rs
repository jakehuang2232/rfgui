//! Text unit tests.

#![cfg(test)]

use super::{
    ElementTrait, Text, TextReadOnlyIfcFallback, TextReadOnlyIfcRenderDecision,
    TextReadOnlyIfcStagingMode, measure_text_size,
};
use crate::style::{
    Color, ColorLike, FontFamily, FontSize, FontWeight, HexColor, ParsedValue, PropertyId, Style,
    TextWrap, VerticalAlign,
};
use crate::view::base_component::{
    DirtyFlags, InlineMeasureContext, InlinePlacement, LayoutConstraints, LayoutPlacement,
    Layoutable, Renderable, UiBuildContext,
};
use crate::view::frame_graph::{
    AttachmentLoadOp, AttachmentTarget, FrameGraph, GraphicsPassMergePolicy, PassDescriptor,
    PassDetails,
};
use crate::view::inline_text_pass_adapter::{
    TextReadOnlyIfcBridgeInput, inline_text_pass_prepare_comparable_package_for_test,
};
use crate::view::node_arena::NodeArena;
use crate::view::render_pass::text_pass::{
    TextPassFragment, TextPassParams, TextPassPreparedFragment, TextPassPreparedParams,
    TextPassRasterGlyphInput, build_text_pass_prepare_probe_for_test,
    build_text_prepared_input_pass_prepare_probe_for_test, text_raster_key_for_raster_input,
};
use crate::view::text_layout::{TextLayoutAlignment, build_text_layout};
use std::sync::Arc;

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

#[derive(Clone, Debug)]
struct TextReadOnlyIfcBuildIntegration {
    pass_names: Vec<String>,
    pass_descriptors: Vec<PassDescriptor>,
    expected_target: AttachmentTarget,
    compiled_pass_count: usize,
}

fn build_text_integration_for_read_only_ifc_test(
    text: &mut Text,
) -> TextReadOnlyIfcBuildIntegration {
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(200, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    let expected_target = target
        .handle()
        .map(AttachmentTarget::Texture)
        .unwrap_or(AttachmentTarget::Surface);
    ctx.set_current_target(target);
    graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new([0.0, 0.0, 0.0, 0.0]),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: target,
        },
    ));
    ctx.set_current_target(target);
    let mut arena = arena();

    text.build(&mut graph, &mut arena, ctx);

    graph
        .compile()
        .expect("Text read-only render integration graph should compile");
    let pass_descriptors = graph
        .pass_descriptors()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    let pass_names = pass_descriptors
        .iter()
        .map(|descriptor| descriptor.name.to_string())
        .collect();
    let compiled_pass_count = graph
        .compiled_graph()
        .map(|compiled| compiled.passes.len())
        .unwrap_or_default();

    TextReadOnlyIfcBuildIntegration {
        pass_names,
        pass_descriptors,
        expected_target,
        compiled_pass_count,
    }
}

fn assert_text_pass_descriptor_writes_expected_target(
    descriptor: &PassDescriptor,
    expected_name_suffix: &str,
    expected_target: AttachmentTarget,
) {
    assert!(
        descriptor.name.ends_with(expected_name_suffix),
        "expected pass name to end with {expected_name_suffix}, got {}",
        descriptor.name
    );
    let PassDetails::Graphics(graphics) = &descriptor.details else {
        panic!("Text read-only render integration pass should be graphics");
    };
    assert_eq!(
        graphics.merge_policy,
        GraphicsPassMergePolicy::Mergeable,
        "Text read-only render pass should stay mergeable"
    );
    assert_eq!(
        graphics.color_attachments.len(),
        1,
        "Text read-only render pass should write one color attachment"
    );
    let color = graphics.color_attachments[0];
    assert_eq!(color.target, expected_target);
    assert_eq!(color.load_op, AttachmentLoadOp::Load);
    assert!(graphics.requirements.requires_color_attachment);
    let depth_stencil = graphics
        .depth_stencil_attachment
        .expect("Text read-only render pass should keep the current target depth/stencil context");
    assert!(depth_stencil.depth.is_some());
    assert!(depth_stencil.stencil.is_some());
    assert!(graphics.requirements.uses_depth);
    assert!(graphics.requirements.uses_stencil);
}

fn existing_read_only_text_glyphs(
    input: &TextReadOnlyIfcBridgeInput,
) -> Vec<(TextPassRasterGlyphInput, [f32; 2])> {
    build_text_layout(
        &input.content,
        input.width_constraint,
        input.allow_wrap,
        input.style.font_size,
        input.style.line_height,
        input.style.font_weight,
        TextLayoutAlignment::Left,
        &input.style.font_families,
    )
    .layout
    .lines()
    .into_iter()
    .flat_map(|line| {
        let baseline_y = line.y + line.baseline;
        line.glyphs.into_iter().filter_map(move |glyph| {
            let raster = TextPassRasterGlyphInput::from_text_glyph(&glyph)?;
            Some((raster, [line.x + glyph.x, baseline_y + glyph.y]))
        })
    })
    .collect()
}

fn text_pass_params_from_read_only_ifc_input(input: &TextReadOnlyIfcBridgeInput) -> TextPassParams {
    let measured = build_text_layout(
        &input.content,
        input.width_constraint,
        input.allow_wrap,
        input.style.font_size,
        input.style.line_height,
        input.style.font_weight,
        TextLayoutAlignment::Left,
        &input.style.font_families,
    );
    TextPassParams {
        fragments: vec![TextPassFragment {
            content: input.content.clone(),
            x: input.origin[0],
            y: input.origin[1],
            width: input.layout_size[0],
            height: input.layout_size[1],
            color: input.text_color,
            opacity: input.opacity,
            text_layout: Some(Arc::new(measured.layout)),
        }],
        font_size: input.style.font_size,
        line_height: input.style.line_height,
        font_weight: input.style.font_weight,
        font_families: input.style.font_families.clone(),
        allow_wrap: input.allow_wrap,
        scissor_rect: None,
        stencil_clip_id: None,
    }
}

fn text_prepared_params_from_read_only_probe(
    probe: &super::TextReadOnlyIfcStagingProbe,
) -> TextPassPreparedParams {
    TextPassPreparedParams {
        staging_input: probe.text_pass_staging_input.clone(),
        fragments: vec![TextPassPreparedFragment {
            origin: probe.input.origin,
            size: probe.input.layout_size,
        }],
        scissor_rect: None,
        stencil_clip_id: None,
    }
}

#[derive(Clone, Copy, Debug)]
struct TextReadOnlyIfcVisualDemoSpec {
    label: &'static str,
    content: &'static str,
    width: f32,
    height: f32,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    color: &'static str,
    text_wrap: TextWrap,
    expected_candidate_pass_suffix: &'static str,
}

#[derive(Debug)]
struct TextReadOnlyIfcVisualDemoPair {
    spec: TextReadOnlyIfcVisualDemoSpec,
    legacy_pass_names: Vec<String>,
    candidate_pass_names: Vec<String>,
    candidate_probe: super::TextReadOnlyIfcStagingProbe,
}

fn text_read_only_ifc_visual_demo_specs() -> Vec<TextReadOnlyIfcVisualDemoSpec> {
    vec![
        TextReadOnlyIfcVisualDemoSpec {
            label: "read-only wrapped text: legacy above, prepared candidate below",
            content: "IFC prepared candidate wraps this read-only Text into multiple lines so the legacy TextPass and prepared path can be visually compared.",
            width: 156.0,
            height: 140.0,
            font_size: 17.0,
            line_height: 1.35,
            font_weight: 500,
            color: "#1f4f8f",
            text_wrap: TextWrap::Wrap,
            expected_candidate_pass_suffix: "render_pass::text_pass::TextPreparedInputPass",
        },
        TextReadOnlyIfcVisualDemoSpec {
            label: "read-only nowrap text: legacy above, prepared candidate below",
            content: "IFC prepared nowrap candidate",
            width: 92.0,
            height: 52.0,
            font_size: 15.0,
            line_height: 1.2,
            font_weight: 650,
            color: "#8f2f48",
            text_wrap: TextWrap::NoWrap,
            expected_candidate_pass_suffix: "render_pass::text_pass::TextPreparedInputPass",
        },
    ]
}

fn text_for_read_only_ifc_visual_demo(spec: TextReadOnlyIfcVisualDemoSpec) -> Text {
    let mut text = Text::new(0.0, 0.0, spec.width, spec.height, spec.content);
    text.set_font_size(spec.font_size);
    text.set_line_height(spec.line_height);
    text.set_font_weight(spec.font_weight);
    text.set_color(HexColor::new(spec.color));
    text.set_text_wrap(spec.text_wrap);
    text
}

fn build_text_read_only_ifc_visual_demo_pair(
    spec: TextReadOnlyIfcVisualDemoSpec,
) -> TextReadOnlyIfcVisualDemoPair {
    let mut legacy = text_for_read_only_ifc_visual_demo(spec);
    legacy.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::Disabled);
    place_text_for_read_only_ifc_test(&mut legacy, spec.width, spec.height);
    let legacy_pass_names = build_text_for_read_only_ifc_test(&mut legacy);

    let mut candidate = text_for_read_only_ifc_visual_demo(spec);
    place_text_for_read_only_ifc_test(&mut candidate, spec.width, spec.height);
    let candidate_pass_names = build_text_for_read_only_ifc_test(&mut candidate);
    let candidate_probe = candidate
        .text_read_only_ifc_staging_probe_for_test()
        .cloned()
        .expect("visual demo candidate should capture IFC prepared staging metadata");

    TextReadOnlyIfcVisualDemoPair {
        spec,
        legacy_pass_names,
        candidate_pass_names,
        candidate_probe,
    }
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 0.001,
        "expected {actual} to be close to {expected}"
    );
}

fn assert_vec2_close(actual: [f32; 2], expected: [f32; 2]) {
    assert_close(actual[0], expected[0]);
    assert_close(actual[1], expected[1]);
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
fn text_read_only_ifc_staging_input_uses_text_render_fields() {
    let mut text = Text::new(4.0, 6.0, 92.0, 80.0, "staging text");
    text.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::ProbeOnly);
    text.set_color(HexColor::new("#336699"));
    text.set_font_size(18.0);
    text.set_line_height(1.4);
    text.set_font_weight(650);
    text.set_fonts(["Inter", "system-ui"]);
    text.set_opacity(0.42);
    text.set_auto_height(true);
    place_text_for_read_only_ifc_test(&mut text, 92.0, 120.0);

    let snapshot = text.box_model_snapshot();
    let origin = [snapshot.x + 7.0, snapshot.y + 9.0];
    let input = text
        .text_read_only_ifc_bridge_input_for_test(origin, 5)
        .expect("read-only Text should produce an opt-in IFC staging input");

    assert_eq!(input.content, "staging text");
    assert_eq!(input.style.font_size, 18.0);
    assert_eq!(input.style.line_height, 1.4);
    assert_eq!(input.style.font_weight, 650);
    assert_eq!(input.style.font_families, vec!["Inter", "system-ui"]);
    assert_eq!(input.style.brush, [0x33, 0x66, 0x99, 0xff]);
    assert_eq!(input.text_color, text.color.to_rgba_f32());
    assert_eq!(input.opacity, 0.42);
    assert_eq!(input.fragment_index, 5);
    assert_eq!(input.origin, origin);
    assert_eq!(
        input.layout_size,
        [
            text.render_size
                .width
                .max(text.layout_state.layout_size.width),
            text.render_size
                .height
                .max(text.layout_state.layout_size.height),
        ]
    );
    assert_eq!(input.width_constraint, Some(input.layout_size[0]));
    assert_eq!(input.allow_wrap, text.allow_wrap);
}

#[test]
fn text_read_only_ifc_staging_default_uses_prepared_candidate() {
    let mut text = Text::new(0.0, 0.0, 92.0, 80.0, "staging text");
    place_text_for_read_only_ifc_test(&mut text, 92.0, 120.0);

    assert_eq!(
        text.text_read_only_ifc_render_decision(),
        TextReadOnlyIfcRenderDecision::PreparedCandidate {
            fallback: TextReadOnlyIfcFallback::ExistingTextPass,
        }
    );
    assert!(
        text.text_read_only_ifc_bridge_input([0.0, 0.0], 1)
            .is_some()
    );
    assert!(
        text.text_read_only_ifc_bridge_package([0.0, 0.0], 1)
            .is_some()
    );
    assert!(
        text.text_read_only_ifc_prepared_staging_input([0.0, 0.0], 1)
            .is_some()
    );

    let pass_names = build_text_for_read_only_ifc_test(&mut text);
    assert!(
        text.text_read_only_ifc_staging_probe_for_test().is_some(),
        "default read-only Text::build should capture IFC prepared staging metadata"
    );
    assert_eq!(pass_names.len(), 1);
    assert!(pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"));
}

#[test]
fn text_read_only_ifc_staging_disabled_mode_keeps_existing_text_pass() {
    let mut text = Text::new(0.0, 0.0, 92.0, 80.0, "staging text");
    text.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::Disabled);
    place_text_for_read_only_ifc_test(&mut text, 92.0, 120.0);

    assert_eq!(
        text.text_read_only_ifc_render_decision(),
        TextReadOnlyIfcRenderDecision::ExistingTextPass {
            capture_probe: false
        }
    );
    assert!(
        text.text_read_only_ifc_bridge_input([0.0, 0.0], 1)
            .is_none()
    );
    assert!(
        text.text_read_only_ifc_bridge_package([0.0, 0.0], 1)
            .is_none()
    );
    assert!(
        text.text_read_only_ifc_prepared_staging_input([0.0, 0.0], 1)
            .is_none()
    );

    let pass_names = build_text_for_read_only_ifc_test(&mut text);
    assert!(
        text.text_read_only_ifc_staging_probe_for_test().is_none(),
        "explicit disabled Text::build must not capture an IFC staging probe"
    );
    assert_eq!(pass_names.len(), 1);
    assert!(
        pass_names[0].ends_with("render_pass::text_pass::TextPass"),
        "Disabled should keep the existing TextPass, got {}",
        pass_names[0]
    );
}

#[test]
fn text_read_only_ifc_staging_formal_candidate_declares_existing_text_pass_fallback() {
    let mut probe_only = Text::new(0.0, 0.0, 92.0, 80.0, "fallback candidate text");
    probe_only.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::ProbeOnly);
    assert_eq!(
        probe_only.text_read_only_ifc_render_decision(),
        TextReadOnlyIfcRenderDecision::ExistingTextPass {
            capture_probe: true
        }
    );

    let mut candidate = Text::new(0.0, 0.0, 92.0, 80.0, "fallback candidate text");
    candidate.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::RenderPreparedInput);
    let decision = candidate.text_read_only_ifc_render_decision();
    assert_eq!(
        decision,
        TextReadOnlyIfcRenderDecision::PreparedCandidate {
            fallback: TextReadOnlyIfcFallback::ExistingTextPass,
        }
    );
    assert!(decision.captures_probe());
    assert!(decision.uses_prepared_render_pass());
    assert_eq!(
        decision.fallback(),
        Some(TextReadOnlyIfcFallback::ExistingTextPass)
    );
}

#[test]
fn text_read_only_ifc_staging_formal_candidate_skips_non_renderable_text() {
    let mut empty = Text::new(0.0, 0.0, 92.0, 80.0, "");
    empty.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::RenderPreparedInput);
    place_text_for_read_only_ifc_test(&mut empty, 92.0, 120.0);
    assert!(
        empty
            .text_read_only_ifc_bridge_input_for_test([0.0, 0.0], 0)
            .is_none()
    );
    assert!(
        empty
            .text_read_only_ifc_prepared_staging_input([0.0, 0.0], 0)
            .is_none()
    );
    assert!(build_text_for_read_only_ifc_test(&mut empty).is_empty());

    let mut transparent = Text::new(0.0, 0.0, 92.0, 80.0, "transparent candidate text");
    transparent.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::RenderPreparedInput);
    transparent.set_opacity(0.0);
    place_text_for_read_only_ifc_test(&mut transparent, 92.0, 120.0);
    assert!(
        transparent
            .text_read_only_ifc_bridge_input_for_test([0.0, 0.0], 0)
            .is_none(),
        "opacity 0 should not build IFC bridge metadata"
    );
    assert!(
        transparent
            .text_read_only_ifc_prepared_staging_input([0.0, 0.0], 0)
            .is_none(),
        "opacity 0 should not expose prepared candidate input"
    );
    assert!(build_text_for_read_only_ifc_test(&mut transparent).is_empty());

    let mut hidden = Text::new(0.0, 0.0, 92.0, 80.0, "hidden candidate text");
    hidden.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::RenderPreparedInput);
    place_text_for_read_only_ifc_test(&mut hidden, 92.0, 120.0);
    hidden.layout_state.should_render = false;
    assert!(
        hidden
            .text_read_only_ifc_bridge_input_for_test([0.0, 0.0], 0)
            .is_none()
    );
    assert!(
        hidden
            .text_read_only_ifc_prepared_staging_input([0.0, 0.0], 0)
            .is_none()
    );
    assert!(build_text_for_read_only_ifc_test(&mut hidden).is_empty());
}

#[test]
fn text_read_only_ifc_staging_probe_only_keeps_existing_text_build_output() {
    let mut disabled = Text::new(0.0, 0.0, 92.0, 80.0, "staging text");
    disabled.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::Disabled);
    place_text_for_read_only_ifc_test(&mut disabled, 92.0, 120.0);

    let mut text = Text::new(0.0, 0.0, 92.0, 80.0, "staging text");
    text.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::ProbeOnly);
    place_text_for_read_only_ifc_test(&mut text, 92.0, 120.0);
    assert!(
        text.text_read_only_ifc_bridge_package([0.0, 0.0], 1)
            .is_some()
    );
    assert!(
        text.text_read_only_ifc_prepared_staging_input([0.0, 0.0], 1)
            .is_none(),
        "ProbeOnly should not expose the render-prepared input helper"
    );

    let disabled_pass_names = build_text_for_read_only_ifc_test(&mut disabled);
    let enabled_pass_names = build_text_for_read_only_ifc_test(&mut text);
    let probe = text
        .text_read_only_ifc_staging_probe_for_test()
        .expect("ProbeOnly Text::build should capture an IFC staging probe");

    assert_eq!(probe.mode, TextReadOnlyIfcStagingMode::ProbeOnly);
    assert_eq!(enabled_pass_names, disabled_pass_names);
    assert_eq!(enabled_pass_names.len(), 1);
    assert!(
        enabled_pass_names[0].ends_with("render_pass::text_pass::TextPass"),
        "Text::build should still emit the existing TextPass, got {}",
        enabled_pass_names[0]
    );

    let comparable = inline_text_pass_prepare_comparable_package_for_test(&probe.package, 1.0);
    let existing = existing_read_only_text_glyphs(&probe.input);
    assert_eq!(comparable.glyphs.len(), existing.len());
    assert_eq!(probe.prepared_input.glyphs.len(), existing.len());
    assert_eq!(probe.prepared_input.batches, comparable.batches);
    assert_eq!(probe.prepared_input.scale_factor, 1.0);
    assert_eq!(
        probe.prepared_equivalent.prepared_input,
        probe.prepared_input
    );
    assert_eq!(probe.text_pass_staging_input.scale_factor, 1.0);
    assert_eq!(
        probe.text_pass_staging_input.glyphs.len(),
        probe.prepared_input.glyphs.len()
    );
    assert!(!probe.text_pass_staging_probe.glyphs.is_empty());
    assert!(
        probe.text_pass_staging_probe.glyphs.len() <= probe.prepared_input.glyphs.len(),
        "TextPass staging probe may skip glyphs that do not rasterize"
    );
    for ((glyph, prepared), (existing_raster, existing_local_pos)) in comparable
        .glyphs
        .iter()
        .zip(probe.prepared_input.glyphs.iter())
        .zip(existing)
    {
        assert_eq!(prepared.glyph_index, glyph.glyph_index);
        assert_eq!(prepared.batch_index, glyph.batch_index);
        assert_eq!(prepared.raster, glyph.raster);
        assert_eq!(prepared.paint, glyph.paint);
        assert_eq!(
            glyph.raster_key,
            text_raster_key_for_raster_input(&existing_raster, 1.0)
        );
        assert_vec2_close(glyph.paint.local_pos, existing_local_pos);
        assert_eq!(prepared.raster_key, glyph.raster_key);
        assert_eq!(prepared.paint.color, probe.input.text_color);
        assert_eq!(prepared.paint.opacity, probe.input.opacity);
        assert_eq!(prepared.paint.fragment_index, probe.input.fragment_index);
        let expected_final = [
            probe.input.origin[0] + existing_local_pos[0],
            probe.input.origin[1] + existing_local_pos[1],
        ];
        assert_vec2_close(
            probe.input.final_paint_pos(glyph.paint.local_pos),
            expected_final,
        );
        assert_vec2_close(prepared.final_paint_pos, expected_final);
        if let Some(staging) = probe
            .text_pass_staging_probe
            .glyphs
            .iter()
            .find(|staging| staging.glyph_index == prepared.glyph_index)
        {
            assert_eq!(staging.raster_key, prepared.raster_key);
            assert_eq!(staging.paint, prepared.paint);
            assert_vec2_close(staging.final_paint_pos, prepared.final_paint_pos);
            assert!(staging.instance_size[0] >= 1.0);
            assert!(staging.instance_size[1] >= 1.0);
        }
    }
}

#[test]
fn text_read_only_ifc_staging_render_prepared_input_uses_prepared_render_pass() {
    let mut disabled = Text::new(0.0, 0.0, 58.0, 120.0, "render prepared input wraps text");
    disabled.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::Disabled);
    disabled.set_font_size(16.0);
    disabled.set_line_height(1.25);
    disabled.set_opacity(0.66);
    disabled.set_auto_height(true);
    place_text_for_read_only_ifc_test(&mut disabled, 58.0, 150.0);

    let mut text = Text::new(0.0, 0.0, 58.0, 120.0, "render prepared input wraps text");
    text.set_font_size(16.0);
    text.set_line_height(1.25);
    text.set_opacity(0.66);
    text.set_auto_height(true);
    place_text_for_read_only_ifc_test(&mut text, 58.0, 150.0);

    let direct_staging_input = text
        .text_read_only_ifc_prepared_staging_input([0.0, 0.0], 0)
        .expect("RenderPreparedInput should expose TextPass prepared staging metadata");

    let disabled_pass_names = build_text_for_read_only_ifc_test(&mut disabled);
    let render_pass_names = build_text_for_read_only_ifc_test(&mut text);
    let probe = text
        .text_read_only_ifc_staging_probe_for_test()
        .expect("RenderPreparedInput Text::build should capture prepared metadata");

    assert_eq!(probe.mode, TextReadOnlyIfcStagingMode::RenderPreparedInput);
    assert_eq!(disabled_pass_names.len(), 1);
    assert!(
        disabled_pass_names[0].ends_with("render_pass::text_pass::TextPass"),
        "disabled read-only Text should keep the existing TextPass, got {}",
        disabled_pass_names[0]
    );
    assert_eq!(render_pass_names.len(), 1);
    assert!(
        render_pass_names[0].ends_with("render_pass::text_pass::TextPreparedInputPass"),
        "RenderPreparedInput should emit the prepared-input pass, got {}",
        render_pass_names[0]
    );
    assert_ne!(render_pass_names, disabled_pass_names);
    assert_eq!(direct_staging_input, probe.text_pass_staging_input);

    let comparable = inline_text_pass_prepare_comparable_package_for_test(&probe.package, 1.0);
    let existing = existing_read_only_text_glyphs(&probe.input);
    assert_eq!(probe.prepared_input.glyphs.len(), existing.len());
    assert_eq!(probe.text_pass_staging_input.glyphs.len(), existing.len());
    assert_eq!(comparable.glyphs.len(), existing.len());
    assert!(
        probe
            .prepared_input
            .glyphs
            .iter()
            .any(|glyph| glyph.paint.local_pos[1]
                > probe.prepared_input.glyphs[0].paint.local_pos[1]),
        "wrapped RenderPreparedInput metadata should include multi-line glyph positions"
    );

    for ((prepared, staging), (existing_raster, existing_local_pos)) in probe
        .prepared_input
        .glyphs
        .iter()
        .zip(probe.text_pass_staging_input.glyphs.iter())
        .zip(existing)
    {
        assert_eq!(staging.raster, prepared.raster);
        assert_eq!(staging.paint, prepared.paint);
        assert_vec2_close(staging.final_paint_pos, prepared.final_paint_pos);
        assert_eq!(
            prepared.raster_key,
            text_raster_key_for_raster_input(&existing_raster, 1.0)
        );
        assert_vec2_close(prepared.paint.local_pos, existing_local_pos);
        assert_eq!(prepared.paint.color, probe.input.text_color);
        assert_eq!(prepared.paint.opacity, probe.input.opacity);
        assert_eq!(prepared.paint.fragment_index, probe.input.fragment_index);
        assert_vec2_close(
            prepared.final_paint_pos,
            [
                probe.input.origin[0] + existing_local_pos[0],
                probe.input.origin[1] + existing_local_pos[1],
            ],
        );
    }
}

#[test]
fn text_read_only_ifc_staging_render_prepared_input_compiles_equivalent_render_target_wiring() {
    let mut disabled = Text::new(
        0.0,
        0.0,
        68.0,
        130.0,
        "render integration target wiring wraps text",
    );
    disabled.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::Disabled);
    disabled.set_font_size(16.0);
    disabled.set_line_height(1.25);
    disabled.set_auto_height(true);
    place_text_for_read_only_ifc_test(&mut disabled, 68.0, 150.0);

    let mut probe_only = Text::new(
        0.0,
        0.0,
        68.0,
        130.0,
        "render integration target wiring wraps text",
    );
    probe_only.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::ProbeOnly);
    probe_only.set_font_size(16.0);
    probe_only.set_line_height(1.25);
    probe_only.set_auto_height(true);
    place_text_for_read_only_ifc_test(&mut probe_only, 68.0, 150.0);

    let mut prepared = Text::new(
        0.0,
        0.0,
        68.0,
        130.0,
        "render integration target wiring wraps text",
    );
    prepared.set_font_size(16.0);
    prepared.set_line_height(1.25);
    prepared.set_auto_height(true);
    place_text_for_read_only_ifc_test(&mut prepared, 68.0, 150.0);

    let disabled_summary = build_text_integration_for_read_only_ifc_test(&mut disabled);
    let probe_summary = build_text_integration_for_read_only_ifc_test(&mut probe_only);
    let prepared_summary = build_text_integration_for_read_only_ifc_test(&mut prepared);

    assert_eq!(disabled_summary.pass_descriptors.len(), 2);
    assert_eq!(probe_summary.pass_descriptors.len(), 2);
    assert_eq!(prepared_summary.pass_descriptors.len(), 2);
    assert_eq!(disabled_summary.compiled_pass_count, 2);
    assert_eq!(probe_summary.compiled_pass_count, 2);
    assert_eq!(prepared_summary.compiled_pass_count, 2);
    assert!(disabled_summary.pass_names[0].ends_with("clear_pass::ClearPass"));
    assert!(probe_summary.pass_names[0].ends_with("clear_pass::ClearPass"));
    assert!(prepared_summary.pass_names[0].ends_with("clear_pass::ClearPass"));
    assert_eq!(probe_summary.pass_names[1], disabled_summary.pass_names[1]);
    assert_ne!(
        prepared_summary.pass_names[1],
        disabled_summary.pass_names[1]
    );

    assert_text_pass_descriptor_writes_expected_target(
        &disabled_summary.pass_descriptors[1],
        "render_pass::text_pass::TextPass",
        disabled_summary.expected_target,
    );
    assert_text_pass_descriptor_writes_expected_target(
        &probe_summary.pass_descriptors[1],
        "render_pass::text_pass::TextPass",
        probe_summary.expected_target,
    );
    assert_text_pass_descriptor_writes_expected_target(
        &prepared_summary.pass_descriptors[1],
        "render_pass::text_pass::TextPreparedInputPass",
        prepared_summary.expected_target,
    );
    assert!(
        probe_only
            .text_read_only_ifc_staging_probe_for_test()
            .is_some(),
        "ProbeOnly should still capture staging metadata during build"
    );
    assert!(
        prepared
            .text_read_only_ifc_staging_probe_for_test()
            .is_some(),
        "RenderPreparedInput should capture staging metadata during build"
    );
}

#[test]
fn text_read_only_ifc_staging_visual_demo_coverage_pairs_legacy_and_candidate() {
    for spec in text_read_only_ifc_visual_demo_specs() {
        let demo = build_text_read_only_ifc_visual_demo_pair(spec);

        assert!(
            demo.legacy_pass_names
                .iter()
                .any(|name| name.ends_with("render_pass::text_pass::TextPass")),
            "{} should keep a legacy/default TextPass comparator",
            demo.spec.label
        );
        assert!(
            demo.candidate_pass_names
                .iter()
                .any(|name| name.ends_with(demo.spec.expected_candidate_pass_suffix)),
            "{} should expose a RenderPreparedInput candidate comparator, got {:?}",
            demo.spec.label,
            demo.candidate_pass_names
        );
        assert_eq!(
            demo.candidate_probe.mode,
            TextReadOnlyIfcStagingMode::RenderPreparedInput
        );
        assert_eq!(demo.candidate_probe.input.content, demo.spec.content);
        assert_eq!(
            demo.candidate_probe.input.layout_size,
            [demo.spec.width, demo.spec.height],
            "{} should keep the visual comparison box stable",
            demo.spec.label
        );
        assert_eq!(
            demo.candidate_probe.input.width_constraint,
            Some(demo.spec.width)
        );
        assert_eq!(
            demo.candidate_probe.input.allow_wrap,
            matches!(demo.spec.text_wrap, TextWrap::Wrap)
        );
        assert_eq!(
            demo.candidate_probe.input.style.brush,
            HexColor::new(demo.spec.color).to_rgba_u8()
        );
        assert_eq!(
            demo.candidate_probe.input.style.font_size,
            demo.spec.font_size
        );
        assert_eq!(
            demo.candidate_probe.input.style.line_height,
            demo.spec.line_height
        );
        assert_eq!(
            demo.candidate_probe.input.style.font_weight,
            demo.spec.font_weight
        );
        assert!(
            !demo.candidate_probe.package.glyphs.is_empty(),
            "{} should produce visible glyph payload for manual comparison",
            demo.spec.label
        );
        assert!(
            !demo
                .candidate_probe
                .text_pass_staging_probe
                .glyphs
                .is_empty(),
            "{} should produce prepared glyph metadata for the candidate pass",
            demo.spec.label
        );
    }
}

#[test]
fn text_read_only_ifc_staging_render_prepared_input_does_not_switch_inline_fragments() {
    let mut a = arena();
    let mut text = Text::from_content("inline fragments stay on the old text pass");
    text.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::RenderPreparedInput);
    text.set_font_size(16.0);
    text.set_line_height(1.25);
    text.measure_inline(
        InlineMeasureContext {
            first_available_width: 72.0,
            full_available_width: 72.0,
            available_height: 1_000_000.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(72.0),
            percent_base_height: Some(120.0),
        },
        &mut a,
    );
    text.place_inline(
        InlinePlacement {
            node_index: 0,
            x: 8.0,
            y: 12.0,
            offset_x: 0.0,
            offset_y: 0.0,
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 72.0,
            available_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(72.0),
            percent_base_height: Some(120.0),
        },
        &mut a,
    );

    assert!(
        text.inline_plan.as_ref().is_some_and(|plan| plan
            .runs
            .iter()
            .any(|fragment| fragment.position.is_some() && !fragment.content.is_empty())),
        "test setup must exercise the inline-fragment Text render path"
    );
    assert!(
        text.text_read_only_ifc_bridge_input_for_test([0.0, 0.0], 0)
            .is_none(),
        "read-only IFC rollout gate must not capture positioned inline fragments"
    );
    assert!(
        text.text_read_only_ifc_prepared_staging_input([0.0, 0.0], 0)
            .is_none(),
        "RenderPreparedInput is only allowed for read-only non-inline Text"
    );

    let pass_names = build_text_for_read_only_ifc_test(&mut text);
    assert!(
        text.text_read_only_ifc_staging_probe_for_test().is_none(),
        "inline fragment rendering must not capture read-only IFC staging metadata"
    );
    assert_eq!(pass_names.len(), 1);
    assert!(
        pass_names[0].ends_with("render_pass::text_pass::TextPass"),
        "inline fragments should keep the existing TextPass, got {}",
        pass_names[0]
    );
}

#[test]
fn text_read_only_ifc_staging_render_prepare_probe_matches_existing_text_pass_when_wrapped() {
    let mut text = Text::new(
        0.0,
        0.0,
        56.0,
        130.0,
        "prepared render probe wraps across several words",
    );
    text.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::RenderPreparedInput);
    text.set_font_size(16.0);
    text.set_line_height(1.25);
    text.set_font_weight(500);
    text.set_opacity(0.72);
    text.set_auto_height(true);
    place_text_for_read_only_ifc_test(&mut text, 56.0, 150.0);

    let _ = build_text_for_read_only_ifc_test(&mut text);
    let probe = text
        .text_read_only_ifc_staging_probe_for_test()
        .expect("RenderPreparedInput Text::build should capture prepared metadata");
    assert!(
        probe
            .prepared_input
            .glyphs
            .iter()
            .any(|glyph| glyph.paint.local_pos[1]
                > probe.prepared_input.glyphs[0].paint.local_pos[1]),
        "wrapped prepare probe should exercise multi-line glyph placement"
    );

    let text_pass_params = text_pass_params_from_read_only_ifc_input(&probe.input);
    let prepared_params = text_prepared_params_from_read_only_probe(probe);
    let old_probe = build_text_pass_prepare_probe_for_test(&text_pass_params, 1.0);
    let prepared_probe =
        build_text_prepared_input_pass_prepare_probe_for_test(&prepared_params, 1.0);

    assert_eq!(old_probe.fragments, prepared_probe.fragments);
    assert_eq!(old_probe.glyphs, prepared_probe.glyphs);
    assert!(
        !old_probe.glyphs.is_empty(),
        "render prepare probe should prove the text path produces glyph output"
    );
    assert_eq!(old_probe.mask_draw, prepared_probe.mask_draw);
    assert_eq!(old_probe.color_draw, prepared_probe.color_draw);
    assert!(
        old_probe.mask_draw.is_some() || old_probe.color_draw.is_some(),
        "at least one atlas draw should be prepared for visible read-only text"
    );
}

#[test]
fn text_read_only_ifc_staging_render_prepare_probe_matches_existing_text_pass_when_nowrap() {
    let mut text = Text::new(0.0, 0.0, 46.0, 90.0, "prepared render probe remains nowrap");
    text.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::RenderPreparedInput);
    text.set_text_wrap(TextWrap::NoWrap);
    text.set_font_size(16.0);
    text.set_line_height(1.25);
    text.set_opacity(0.83);
    text.set_auto_height(true);
    place_text_for_read_only_ifc_test(&mut text, 46.0, 110.0);

    let _ = build_text_for_read_only_ifc_test(&mut text);
    let probe = text
        .text_read_only_ifc_staging_probe_for_test()
        .expect("RenderPreparedInput Text::build should capture prepared metadata");
    assert!(!probe.input.allow_wrap);
    let first_y = probe.prepared_input.glyphs[0].paint.local_pos[1];
    assert!(
        probe
            .prepared_input
            .glyphs
            .iter()
            .all(|glyph| (glyph.paint.local_pos[1] - first_y).abs() <= 0.001),
        "nowrap prepare probe should keep glyphs on one visual line"
    );

    let text_pass_params = text_pass_params_from_read_only_ifc_input(&probe.input);
    let prepared_params = text_prepared_params_from_read_only_probe(probe);
    let old_probe = build_text_pass_prepare_probe_for_test(&text_pass_params, 1.0);
    let prepared_probe =
        build_text_prepared_input_pass_prepare_probe_for_test(&prepared_params, 1.0);

    assert_eq!(old_probe.fragments, prepared_probe.fragments);
    assert_eq!(old_probe.glyphs, prepared_probe.glyphs);
    assert!(
        !old_probe.glyphs.is_empty(),
        "render prepare probe should prove the text path produces glyph output"
    );
    assert_eq!(old_probe.mask_draw, prepared_probe.mask_draw);
    assert_eq!(old_probe.color_draw, prepared_probe.color_draw);
}

#[test]
fn text_read_only_ifc_staging_package_matches_existing_text_layout_when_wrapped() {
    let mut text = Text::new(
        0.0,
        0.0,
        54.0,
        140.0,
        "read only staging wraps across lines",
    );
    text.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::RenderPreparedInput);
    text.set_font_size(16.0);
    text.set_line_height(1.25);
    text.set_font_weight(500);
    text.set_opacity(0.75);
    text.set_auto_height(true);
    place_text_for_read_only_ifc_test(&mut text, 54.0, 160.0);

    let origin = [11.0, 13.0];
    let input = text
        .text_read_only_ifc_bridge_input_for_test(origin, 9)
        .expect("wrapped read-only Text should produce an IFC staging input");
    let package = text
        .text_read_only_ifc_bridge_package_for_test(origin, 9)
        .expect("wrapped read-only Text should produce an IFC bridge package");
    let comparable = inline_text_pass_prepare_comparable_package_for_test(&package, 1.0);
    let existing = existing_read_only_text_glyphs(&input);

    assert!(input.allow_wrap);
    assert!(
        text.text_read_only_ifc_prepared_staging_input(origin, 9)
            .is_some()
    );
    assert_eq!(comparable.glyphs.len(), existing.len());
    assert!(
        comparable
            .glyphs
            .iter()
            .any(|glyph| glyph.paint.local_pos[1] > comparable.glyphs[0].paint.local_pos[1])
    );

    for (glyph, (existing_raster, existing_local_pos)) in comparable.glyphs.iter().zip(existing) {
        assert_eq!(glyph.paint.color, input.text_color);
        assert_eq!(glyph.paint.opacity, input.opacity);
        assert_eq!(glyph.paint.fragment_index, input.fragment_index);
        assert_eq!(glyph.raster.glyph_id, existing_raster.glyph_id);
        assert_eq!(
            glyph.raster_key,
            text_raster_key_for_raster_input(&existing_raster, 1.0)
        );
        assert_vec2_close(glyph.paint.local_pos, existing_local_pos);
        assert_vec2_close(
            input.final_paint_pos(glyph.paint.local_pos),
            [
                input.origin[0] + existing_local_pos[0],
                input.origin[1] + existing_local_pos[1],
            ],
        );
    }
}

#[test]
fn text_read_only_ifc_staging_package_respects_text_nowrap_state() {
    let mut text = Text::new(
        0.0,
        0.0,
        48.0,
        80.0,
        "read only staging should stay on one line",
    );
    text.set_read_only_ifc_staging_mode(TextReadOnlyIfcStagingMode::RenderPreparedInput);
    text.set_text_wrap(TextWrap::NoWrap);
    text.set_font_size(16.0);
    text.set_line_height(1.25);
    text.set_auto_height(true);
    place_text_for_read_only_ifc_test(&mut text, 48.0, 120.0);

    let input = text
        .text_read_only_ifc_bridge_input_for_test([3.0, 5.0], 10)
        .expect("nowrap read-only Text should produce an IFC staging input");
    let package = text
        .text_read_only_ifc_bridge_package_for_test([3.0, 5.0], 10)
        .expect("nowrap read-only Text should produce an IFC bridge package");
    let comparable = inline_text_pass_prepare_comparable_package_for_test(&package, 1.0);
    let existing = existing_read_only_text_glyphs(&input);

    assert!(!input.allow_wrap);
    assert!(
        text.text_read_only_ifc_prepared_staging_input([3.0, 5.0], 10)
            .is_some()
    );
    assert_eq!(comparable.glyphs.len(), existing.len());
    for (glyph, (existing_raster, existing_local_pos)) in comparable.glyphs.iter().zip(existing) {
        assert_eq!(
            glyph.raster_key,
            text_raster_key_for_raster_input(&existing_raster, 1.0)
        );
        assert_vec2_close(glyph.paint.local_pos, existing_local_pos);
    }
    let first_y = comparable.glyphs[0].paint.local_pos[1];
    assert!(
        comparable
            .glyphs
            .iter()
            .all(|glyph| (glyph.paint.local_pos[1] - first_y).abs() <= 0.001)
    );
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
