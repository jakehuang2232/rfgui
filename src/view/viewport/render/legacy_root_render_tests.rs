use crate::style::{
    Border, BoxShadow, ClipMode, Color, Layout, Length, ParsedValue, Position, PropertyId,
    Rotate, Scale, ScrollDirection, Style, Transform, TransformEntry, Transition,
    TransitionProperty, Transitions, Translate,
};
use crate::view::base_component::{
    BoxModelSnapshot, BuildState, DirtyFlags, DirtyPassMask, Element, ElementTrait,
    EventTarget, Image, LayoutConstraints, LayoutPlacement, Layoutable, Renderable,
    ShadowPaintRecordingCapability, Size, Svg, Text, TextArea, UiBuildContext,
};
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::frame_graph::FrameGraph;
use crate::view::node_arena::{Node, NodeArena, NodeKey};
use crate::view::test_support::{
    commit_child, commit_element, measure_and_place, new_test_arena,
};
use crate::view::viewport::ViewportPaintRendererMode;
use crate::view::{ImageSource, SvgSource, image_resource};
use std::any::Any;
use std::sync::Arc;

struct UnknownOverlayHost {
    id: u64,
    bounds: BoxModelSnapshot,
}

impl Layoutable for UnknownOverlayHost {
    fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}

    fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}

    fn measured_size(&self) -> (f32, f32) {
        (self.bounds.width, self.bounds.height)
    }

    fn set_layout_width(&mut self, width: f32) {
        self.bounds.width = width;
    }

    fn set_layout_height(&mut self, height: f32) {
        self.bounds.height = height;
    }
}

impl EventTarget for UnknownOverlayHost {}

impl Renderable for UnknownOverlayHost {
    fn build(
        &mut self,
        _graph: &mut FrameGraph,
        _arena: &mut NodeArena,
        ctx: UiBuildContext,
    ) -> BuildState {
        ctx.into_state()
    }
}

impl ElementTrait for UnknownOverlayHost {
    fn stable_id(&self) -> u64 {
        self.id
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        self.bounds
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

use super::{
    AutoAuthorityDecision, AutoAuthorityKind, AutoAuthorityRejection, AutoAuthorityTrace,
    CachedCompiledGraph, FrameDisposition, PaintAuthorityFallbackStage, PaintAuthorityKind,
    PaintAuthorityTelemetry, PendingRootEffectTransaction, PropertyNeutralArtifactAttempt,
    RecordedArtifactCandidate, RetainedAutoTerminalFailureStage,
    RetainedTransformCanarySelection, RootEffectBuildPlan, RootEffectRetainedState, Viewport,
    begin_paint_authority_telemetry_attempt, build_root_legacy, debug_legacy_fallback,
    direct_scroll_transform_prepare_rejection_dispatch,
    direct_scroll_transform_prepare_rejection_fallback_stage,
    enable_paint_authority_test_capture, finish_frame_dirty_lifecycle, frame_disposition,
    nested_scroll_prepare_rejection_dispatch, nested_scroll_prepare_rejection_fallback_stage,
    nested_scroll_success_trace, paint_authority_test_capture_enabled,
    preflight_direct_scroll_transform_selection, preflight_nested_scroll_selection,
    preflight_transform_effect_scroll_selection, retained_auto_circuit_breaker_selection,
    retained_auto_fallback_overlay_records, retained_auto_overlay_label,
    retained_auto_terminal_fallback_stage, select_retained_auto_authority,
    select_retained_transform_canary, should_store_compile_cache,
    store_paint_authority_test_snapshot, take_paint_authority_test_snapshot,
    terminal_failure_stage, transform_effect_scroll_prepare_rejection_dispatch,
    transform_effect_scroll_prepare_rejection_fallback_stage,
    try_build_property_neutral_artifact_frame, try_compile_recorded_artifact_frame,
};

fn constraints() -> (LayoutConstraints, LayoutPlacement) {
    (
        LayoutConstraints {
            max_width: 320.0,
            max_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 320.0,
            available_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        },
    )
}

fn seed_empty_compile_cache(viewport: &mut Viewport) {
    let mut graph = FrameGraph::new();
    graph
        .compile()
        .expect("empty graph compiles for cache fixture");
    let topology_key = graph.topology_cache_key_for_test();
    let compiled_graph = graph
        .take_compiled_graph()
        .expect("compiled empty graph owns a topology cache payload");
    viewport.frame.compile_cache = Some(CachedCompiledGraph {
        topology_key,
        graph: compiled_graph,
    });
}

fn colored_element(id: u64, x: f32, color: Color) -> Element {
    let mut element = Element::new_with_id(id, x, 20.0, 80.0, 40.0);
    let mut style = Style::new();
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn prepared_safe_leaf() -> (NodeArena, Vec<NodeKey>) {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(colored_element(1, 10.0, Color::rgb(230, 20, 30))),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root])
}

fn prepared_sampled_inline_span_layout_transition() -> (NodeArena, Vec<NodeKey>) {
    let mut arena = new_test_arena();
    let mut parent = Element::new_with_id(0xe2_a180, 0.0, 0.0, 92.0, 0.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(92.0)));
    parent.apply_style(parent_style);
    let parent_key = commit_element(&mut arena, Box::new(parent));

    let mut span = Element::new_with_id(0xe2_a181, 0.0, 0.0, 0.0, 0.0);
    let mut span_style = Style::new();
    span_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    span_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#bfdbfe")),
    );
    span_style.set_border(Border::uniform(Length::px(2.0), &Color::hex("#2563eb")));
    span.apply_style(span_style);
    let span_key = commit_child(&mut arena, parent_key, Box::new(span));
    commit_child(
        &mut arena,
        span_key,
        Box::new(Text::new_with_id(
            0xe2_a182,
            0.0,
            0.0,
            0.0,
            0.0,
            "alpha beta gamma delta epsilon zeta",
        )),
    );

    let (mut measure, mut place) = constraints();
    measure.max_width = 92.0;
    measure.max_height = 220.0;
    place.available_width = 92.0;
    place.available_height = 220.0;
    measure_and_place(&mut arena, parent_key, measure, place);
    {
        let mut node = arena.get_mut(span_key).unwrap();
        let span = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
        assert!(
            span.inline_fragment_rects().len() >= 2,
            "fixture must exercise a wrapping inline-owned Element"
        );
        span.set_layout_transition_width(71.0);
        span.set_layout_transition_height(39.0);
        span.clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(span_key, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(span_key);
    (arena, vec![span_key])
}

fn prepared_deferred_viewport_leaf() -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let mut deferred = colored_element(0xe2_a310, 10.0, Color::rgb(230, 20, 30));
    let mut style = Style::new();
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(4.0))
                .clip(ClipMode::Viewport),
        ),
    );
    deferred.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(deferred));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root], root)
}

fn prepared_native_text() -> (NodeArena, Vec<NodeKey>) {
    prepared_native_text_with_opacity(1.0)
}

fn prepared_native_text_with_opacity(opacity: f32) -> (NodeArena, Vec<NodeKey>) {
    let mut arena = new_test_arena();
    let mut text = Text::new_with_id(
        0xd3_a010,
        3.25,
        5.5,
        180.0,
        48.0,
        "native Text retained closure",
    );
    text.set_font("sans-serif");
    text.set_font_size(18.0);
    text.set_color(Color::rgb(30, 80, 210));
    text.set_opacity(opacity);
    let root = commit_element(&mut arena, Box::new(text));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root])
}

fn prepared_native_text_transform(
    transform: Transform,
    nested: bool,
    sampled_parent: bool,
) -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let mut text = Text::new_with_id(0xd3_a012, 3.0, 5.0, 96.0, 28.0, "native Text transform");
    text.set_font("sans-serif");
    text.set_font_size(18.0);
    text.set_color(Color::rgb(30, 80, 210));
    text.set_transform(transform);
    let mut arena = new_test_arena();
    if !nested {
        let root = commit_element(&mut arena, Box::new(text));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        return (arena, vec![root], root);
    }

    let mut parent = Element::new_with_id(0xd3_a013, 0.0, 0.0, 180.0, 72.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    if sampled_parent {
        parent_style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::single(Transition::new(
                TransitionProperty::Width,
                200,
            ))),
        );
    }
    parent.apply_style(parent_style);
    let root = commit_element(&mut arena, Box::new(parent));
    let child = commit_child(&mut arena, root, Box::new(text));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    if sampled_parent {
        let mut node = arena.get_mut(root).expect("sampled Text transform parent");
        let parent = node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("Element parent");
        parent.set_layout_transition_width(164.0);
        parent.set_layout_transition_height(64.0);
        drop(node);
        measure_and_place(&mut arena, root, measure, place);
    }
    (arena, vec![root], child)
}

fn prepared_transparent_native_text() -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let mut text = Text::new_with_id(
        0xd3_a011,
        3.25,
        5.5,
        180.0,
        48.0,
        "transparent native Text retained closure",
    );
    text.set_font("sans-serif");
    text.set_font_size(18.0);
    text.set_color(Color::rgb(30, 80, 210));
    text.set_opacity(0.0);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(text));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root], root)
}

fn prepared_empty_native_text() -> (NodeArena, Vec<NodeKey>) {
    let mut arena = new_test_arena();
    let mut text = Text::new_with_id(0xd3_a011, 3.25, 5.5, 180.0, 48.0, "");
    text.set_font("sans-serif");
    text.set_font_size(18.0);
    let root = commit_element(&mut arena, Box::new(text));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root])
}

fn prepared_native_image() -> (NodeArena, Vec<NodeKey>) {
    prepared_native_image_with_opacity(1.0)
}

fn prepared_native_image_with_opacity(opacity: f32) -> (NodeArena, Vec<NodeKey>) {
    let mut image = Image::new_with_id(
        0xd3_a020,
        ImageSource::Rgba {
            width: 2,
            height: 2,
            pixels: Arc::from([
                255_u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ]),
        },
    );
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(48.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(32.0)));
    style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(crate::style::Opacity::new(opacity)),
    );
    image.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(image));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root])
}

fn prepared_native_image_path_state(
    label: &str,
    opacity: f32,
    error: bool,
) -> (NodeArena, Vec<NodeKey>) {
    let source = ImageSource::Path(std::path::PathBuf::from(format!(
        "/rfgui-retained-root-opacity-{label}.png"
    )));
    let handle = image_resource::acquire_image_resource(&source);
    let mut image = Image::new_with_id(0xd3_a021, source);
    if error {
        image_resource::set_image_error_for_test(
            handle.asset_id(),
            "synthetic root-opacity image error",
        );
    } else {
        image_resource::set_image_loading_for_test(handle.asset_id());
    }
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(48.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(32.0)));
    style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(crate::style::Opacity::new(opacity)),
    );
    image.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(image));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root])
}

#[cfg(not(target_arch = "wasm32"))]
fn prepared_native_svg() -> (NodeArena, Vec<NodeKey>) {
    prepared_native_svg_with_opacity(1.0)
}

#[cfg(not(target_arch = "wasm32"))]
fn prepared_native_svg_with_opacity(opacity: f32) -> (NodeArena, Vec<NodeKey>) {
    const SVG: &str = r##"<svg width="24" height="18" viewBox="0 0 24 18" xmlns="http://www.w3.org/2000/svg"><rect width="24" height="18" fill="#22c55e"/></svg>"##;
    let mut svg = Svg::new_with_id(0xd3_a030, SvgSource::Content(SVG.to_string()));
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(48.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(36.0)));
    style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(crate::style::Opacity::new(opacity)),
    );
    svg.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(svg));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    arena.with_element_taken(root, |element, _arena| {
        element
            .as_any_mut()
            .downcast_mut::<Svg>()
            .expect("native Svg fixture")
            .prepare_content_paint_for_test(SVG, (24.0, 18.0), 1.0)
            .expect("prepare exact native Svg raster");
    });
    (arena, vec![root])
}

#[cfg(not(target_arch = "wasm32"))]
fn prepared_native_svg_path_state(
    label: &str,
    opacity: f32,
    error: bool,
) -> (NodeArena, Vec<NodeKey>) {
    let source = SvgSource::Path(std::path::PathBuf::from(format!(
        "/rfgui-retained-root-opacity-{label}.svg"
    )));
    let source_key = crate::view::svg_resource::acquire_svg_document(&source);
    let mut svg = Svg::new_with_id(0xd3_a031, source);
    if error {
        crate::view::svg_resource::set_svg_document_error_for_test(source_key);
    } else {
        crate::view::svg_resource::set_svg_document_loading_for_test(source_key);
    }
    crate::view::svg_resource::release_svg_document(source_key);
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(48.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(36.0)));
    style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(crate::style::Opacity::new(opacity)),
    );
    svg.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(svg));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root])
}

#[cfg(not(target_arch = "wasm32"))]
fn prepared_native_media_transform(host: &str, state: &str) -> (NodeArena, Vec<NodeKey>) {
    const SVG: &str = r##"<svg width="24" height="18" viewBox="0 0 24 18" xmlns="http://www.w3.org/2000/svg"><rect width="24" height="18" fill="#0ea5e9"/></svg>"##;
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(48.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(36.0)));
    style.set_transform(Transform::new([Translate::x(Length::px(7.0))]));
    let stable_id = if host == "Image" {
        0xd3_a040
    } else {
        0xd3_a050
    } + match state {
        "ready" => 0,
        "loading" => 1,
        "error" => 2,
        _ => panic!("unknown native media state"),
    };
    let native: Box<dyn ElementTrait> = match host {
        "Image" => {
            let source = if state == "ready" {
                ImageSource::Rgba {
                    width: 1,
                    height: 1,
                    pixels: Arc::from([14_u8, 165, 233, 255]),
                }
            } else {
                ImageSource::Path(format!("/rfgui-retained-transform-image-{state}.png").into())
            };
            let mut image = Image::new_with_id(stable_id, source);
            image.apply_style(style);
            match state {
                "loading" => image.set_resource_loading_for_test(),
                "error" => image.set_resource_error_for_test(),
                _ => {}
            }
            Box::new(image)
        }
        "Svg" => {
            let source = if state == "ready" {
                SvgSource::Content(SVG.into())
            } else {
                SvgSource::Path(format!("/rfgui-retained-transform-svg-{state}.svg").into())
            };
            let mut svg = Svg::new_with_id(stable_id, source);
            svg.apply_style(style);
            match state {
                "loading" => svg.set_document_loading_for_transform_test(),
                "error" => svg.set_document_error_for_transform_test(),
                _ => {}
            }
            Box::new(svg)
        }
        _ => panic!("unknown native media host"),
    };
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, native);
    arena
        .with_element_taken(root, |element, arena| element.sync_arena(arena))
        .expect("freeze native media transform state");
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    if host == "Svg" && state == "ready" {
        arena.with_element_taken(root, |element, _arena| {
            element
                .as_any_mut()
                .downcast_mut::<Svg>()
                .expect("native transformed Svg")
                .prepare_content_paint_for_test(SVG, (24.0, 18.0), 1.0)
                .expect("prepare transformed Svg raster");
        });
    }
    (arena, vec![root])
}

#[cfg(not(target_arch = "wasm32"))]
fn prepared_nested_native_effect(
    host: &str,
    state: &str,
) -> (NodeArena, Vec<NodeKey>, NodeKey) {
    const SVG: &str = r##"<svg width="18" height="14" viewBox="0 0 18 14" xmlns="http://www.w3.org/2000/svg"><rect width="18" height="14" fill="#22c55e"/></svg>"##;
    let mut parent = Element::new_with_id(0xd3_a060, 0.0, 0.0, 64.0, 40.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    parent_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(15, 23, 42)),
    );
    parent.apply_style(parent_style);

    let mut child_style = Style::new();
    child_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(18.0)));
    child_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(14.0)));
    child_style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(crate::style::Opacity::new(0.5)),
    );
    let child_id = match host {
        "Text" => 0xd3_a061,
        "Image" => 0xd3_a062,
        "Svg" => 0xd3_a063,
        _ => panic!("unknown nested native host"),
    };
    let child: Box<dyn ElementTrait> = match host {
        "Text" => {
            let mut text = Text::new_with_id(child_id, 0.0, 0.0, 18.0, 14.0, "nested");
            text.set_opacity(0.5);
            Box::new(text)
        }
        "Image" => {
            let source = if state == "ready" {
                ImageSource::Rgba {
                    width: 1,
                    height: 1,
                    pixels: Arc::from([34_u8, 197, 94, 255]),
                }
            } else {
                ImageSource::Path(format!("/rfgui-retained-effect-image-{state}.png").into())
            };
            let mut image = Image::new_with_id(child_id, source);
            image.apply_style(child_style);
            match state {
                "loading" => image.set_resource_loading_for_test(),
                "error" => image.set_resource_error_for_test(),
                _ => {}
            }
            Box::new(image)
        }
        "Svg" => {
            let source = if state == "ready" {
                SvgSource::Content(SVG.into())
            } else {
                SvgSource::Path(format!("/rfgui-retained-effect-svg-{state}.svg").into())
            };
            let mut svg = Svg::new_with_id(child_id, source);
            svg.apply_style(child_style);
            match state {
                "loading" => svg.set_document_loading_for_transform_test(),
                "error" => svg.set_document_error_for_transform_test(),
                _ => {}
            }
            Box::new(svg)
        }
        _ => unreachable!(),
    };

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(parent));
    let child = commit_child(&mut arena, root, child);
    arena
        .with_element_taken(child, |element, arena| element.sync_arena(arena))
        .expect("freeze nested native effect state");
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    if host == "Svg" && state == "ready" {
        arena.with_element_taken(child, |element, _arena| {
            element
                .as_any_mut()
                .downcast_mut::<Svg>()
                .expect("nested effect Svg")
                .prepare_content_paint_for_test(SVG, (18.0, 14.0), 1.0)
                .expect("prepare nested effect Svg raster");
        });
    }
    (arena, vec![root], child)
}

fn prepared_auto_text_area(
    scroll_y: f32,
    pending_caret_scroll: bool,
) -> (NodeArena, Vec<NodeKey>, NodeKey) {
    prepared_auto_text_area_with_content(
        "RetainedAuto must admit bounded internal TextArea scrolling without a ScrollNode",
        scroll_y,
        pending_caret_scroll,
    )
}

fn prepared_auto_text_area_with_content(
    content: &str,
    scroll_y: f32,
    pending_caret_scroll: bool,
) -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let width = 108.0;
    let height = 28.0;
    let mut arena = new_test_arena();
    let mut text_area = TextArea::with_stable_id(0xd3_a100);
    text_area.set_text(content.to_string());
    text_area.font_size = 17.5;
    text_area.line_height = 1.3;
    let root = commit_element(&mut arena, Box::new(text_area));
    arena.with_element_taken(root, |element, _arena| {
        element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap()
            .set_self_node_key(root);
    });
    let measure = LayoutConstraints {
        max_width: width,
        max_height: height,
        viewport_width: 320.0,
        viewport_height: 240.0,
        percent_base_width: Some(320.0),
        percent_base_height: Some(240.0),
    };
    let place = LayoutPlacement {
        parent_x: 7.25,
        parent_y: 11.75,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: width,
        available_height: height,
        viewport_width: 320.0,
        viewport_height: 240.0,
        percent_base_width: Some(320.0),
        percent_base_height: Some(240.0),
    };
    measure_and_place(&mut arena, root, measure, place);
    arena.with_element_taken(root, |element, arena| {
        {
            let text_area = element.as_any_mut().downcast_mut::<TextArea>().unwrap();
            let max_y = (text_area.layout_state.content_size.height
                - text_area.viewport_size.height)
                .max(0.0);
            assert!(scroll_y.is_nan() || scroll_y <= max_y);
            text_area.scroll_y = scroll_y;
        }
        if scroll_y.is_finite() {
            element.place(place, arena);
        }
        element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap()
            .pending_caret_scroll = pending_caret_scroll;
    });
    let mut stack = vec![root];
    while let Some(key) = stack.pop() {
        stack.extend(arena.children_of(key));
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);
    (arena, vec![root], root)
}

fn prepared_transform_leaf() -> (NodeArena, Vec<NodeKey>) {
    let mut element = colored_element(0xc4_b001, 10.0, Color::rgb(230, 20, 30));
    let mut style = Style::new();
    style.set_transform(Transform::new([Translate::x(Length::px(6.0))]));
    element.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(element));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root])
}

fn prepared_nested_transform_tree() -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let nested_colored = |id, x, width, height, color| {
        let mut element = Element::new_with_id(id, x, 1.0, width, height);
        let mut style = Style::new();
        style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(crate::style::Layout::Grid),
        );
        style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
        element.apply_style(style);
        element
    };
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(nested_colored(
            0xc5_c001,
            4.0,
            40.0,
            24.0,
            Color::rgb(20, 40, 80),
        )),
    );
    let child = commit_child(
        &mut arena,
        root,
        Box::new(nested_colored(
            0xc5_c002,
            8.0,
            18.0,
            10.0,
            Color::rgb(180, 60, 20),
        )),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            10.0, 0.0, 0.0,
        ))));
    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            20.0, 0.0, 0.0,
        ))));
    (arena, vec![root], child)
}

fn prepared_general_transform_scene() -> (NodeArena, Vec<NodeKey>) {
    let general_colored = |id, x, width, height, color| {
        let mut element = Element::new_with_id(id, x, 4.0, width, height);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
        element.apply_style(style);
        element
    };
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(general_colored(
            0xc5_d001,
            4.0,
            120.0,
            120.0,
            Color::rgb(20, 40, 80),
        )),
    );
    let child = commit_child(
        &mut arena,
        root,
        Box::new(general_colored(
            0xc5_d002,
            8.0,
            70.0,
            70.0,
            Color::rgb(180, 60, 20),
        )),
    );
    let deep = commit_child(
        &mut arena,
        child,
        Box::new(general_colored(
            0xc5_d003,
            12.0,
            20.0,
            20.0,
            Color::rgb(40, 180, 20),
        )),
    );
    let sibling = commit_child(
        &mut arena,
        root,
        Box::new(general_colored(
            0xc5_d004,
            44.0,
            20.0,
            20.0,
            Color::rgb(80, 20, 180),
        )),
    );
    let second_root = commit_element(
        &mut arena,
        Box::new(general_colored(
            0xc5_d005,
            140.0,
            40.0,
            40.0,
            Color::rgb(200, 120, 20),
        )),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    measure_and_place(&mut arena, second_root, measure, place);
    for (node, x) in [
        (root, 5.0),
        (child, 7.0),
        (deep, 9.0),
        (sibling, 11.0),
        (second_root, 13.0),
    ] {
        crate::view::test_support::get_element_mut::<Element>(&arena, node)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                glam::Vec3::new(x, 0.0, 0.0),
            )));
    }
    (arena, vec![root, second_root])
}

fn prepared_transform_child_isolation_tree()
-> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey) {
    let mixed_element = |id, x, y, width, height, color| {
        let mut element = Element::new_with_id(id, x, y, width, height);
        let mut style = Style::new();
        style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(crate::style::Layout::Grid),
        );
        style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
        element.apply_style(style);
        element
    };
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(mixed_element(
            0xd1_b100,
            0.25,
            0.25,
            40.0,
            24.0,
            Color::rgb(20, 40, 80),
        )),
    );
    let child = commit_child(
        &mut arena,
        root,
        Box::new(mixed_element(
            0xd1_b101,
            4.25,
            1.5,
            18.0,
            10.0,
            Color::rgb(180, 60, 20),
        )),
    );
    let descendant = commit_child(
        &mut arena,
        child,
        Box::new(mixed_element(
            0xd1_b102,
            5.0,
            1.75,
            1.0,
            1.0,
            Color::rgb(200, 160, 20),
        )),
    );
    let (measure, mut place) = constraints();
    place.parent_x = 0.25;
    place.parent_y = 0.25;
    measure_and_place(&mut arena, root, measure, place);
    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            100.0, 0.0, 0.0,
        ))));
    crate::view::test_support::get_element_mut::<Element>(&arena, child).set_opacity(0.5);
    (arena, vec![root], root, child, descendant)
}

fn prepared_nested_opacity_tree() -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey) {
    let (arena, roots, root, child, descendant) = prepared_transform_child_isolation_tree();
    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_resolved_transform_for_test(None);
    crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.5);
    crate::view::test_support::get_element_mut::<Element>(&arena, child).set_opacity(0.25);
    crate::view::test_support::get_element_mut::<Element>(&arena, descendant).set_opacity(0.75);
    (arena, roots, root, child, descendant)
}

fn prepared_transform_scroll_scene(
    matrix: glam::Mat4,
) -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xe2_c300, 0.0, 0.0, 120.0, 90.0,
    ))));
    let scroll = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xe2_c301, 0.0, 0.0, 120.0, 90.0,
    ))));
    let content = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xe2_c302, 0.0, -20.0, 120.0, 240.0,
    ))));
    arena.set_parent(scroll, Some(root));
    arena.push_child(root, scroll);
    arena.set_parent(content, Some(scroll));
    arena.push_child(scroll, content);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    crate::view::test_support::get_element_mut::<Element>(&arena, root).apply_style(root_style);
    crate::view::test_support::get_element_mut::<Element>(&arena, root)
        .set_resolved_transform_for_test(Some(matrix));
    let mut scroll_style = Style::new();
    scroll_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    scroll_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, scroll);
        element.apply_style(scroll_style);
        element.layout_state.content_size = Size {
            width: 120.0,
            height: 240.0,
        };
        element.set_scroll_offset((0.0, 20.0));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    crate::view::test_support::get_element_mut::<Element>(&arena, content)
        .set_background_color_value(Color::rgb(24, 48, 72));
    arena
        .get_mut(content)
        .unwrap()
        .element
        .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    arena.refresh_subtree_dirty_cache(root);
    let roots = vec![root];
    let (properties, generations) = synced_paint_state(&arena, &roots);
    (arena, roots, properties, generations)
}

fn prepared_same_owner_transform_scroll_scene() -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (mut arena, roots, _, _) = prepared_transform_scroll_scene(
        glam::Mat4::from_translation(glam::Vec3::new(7.0, 5.0, 0.0)),
    );
    let root = roots[0];
    let scroll = arena.children_of(root)[0];
    let content = arena.children_of(scroll)[0];
    arena.set_children(root, vec![content]);
    arena.set_parent(content, Some(root));
    arena.set_parent(scroll, None);
    let mut root_style = Style::new();
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, root);
        element.apply_style(root_style);
        element.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
            glam::Vec3::new(7.0, 5.0, 0.0),
        )));
        element.layout_state.content_size = Size {
            width: 120.0,
            height: 240.0,
        };
        element.set_scroll_offset((0.0, 20.0));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena.refresh_subtree_dirty_cache(root);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    (arena, roots, properties, generations)
}

fn prepared_same_owner_effect_scroll_scene() -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (arena, roots, _, _) = prepared_same_owner_transform_scroll_scene();
    let root = roots[0];
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, root);
        element.set_resolved_transform_for_test(None);
        element.set_opacity(0.625);
    }
    arena.refresh_subtree_dirty_cache(root);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    (arena, roots, properties, generations)
}

fn prepared_transform_effect_scroll_scene() -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (mut arena, roots, _, _) = prepared_transform_scroll_scene(
        glam::Mat4::from_translation(glam::Vec3::new(3.0, 0.0, 0.0)),
    );
    let transform_root = roots[0];
    let scroll = arena.children_of(transform_root)[0];
    let mut effect = Element::new_with_id(0xe2_c3f0, 0.0, 0.0, 120.0, 90.0);
    let mut effect_style = Style::new();
    effect_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    effect.apply_style(effect_style);
    effect.set_opacity(0.5);
    let effect = arena.insert(Node::new(Box::new(effect)));
    arena.set_parent(effect, Some(transform_root));
    arena.set_children(transform_root, vec![effect]);
    arena.set_parent(scroll, Some(effect));
    arena.set_children(effect, vec![scroll]);
    arena.refresh_subtree_dirty_cache(transform_root);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    (arena, roots, properties, generations)
}

fn prepared_exact_scroll_scene() -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xe2_a300, 0.0, 0.0, 100.0, 80.0,
    ))));
    let child = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xe2_a301, 0.0, -20.0, 100.0, 300.0,
    ))));
    arena.set_parent(child, Some(root));
    arena.push_child(root, child);
    let mut style = Style::new();
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut root_node = arena.get_mut(root).expect("scroll root");
        let root_element = root_node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("Element scroll root");
        root_element.apply_style(style);
        root_element.layout_state.content_size = Size {
            width: 100.0,
            height: 300.0,
        };
        root_element.set_scroll_offset((0.0, 20.0));
        root_element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena
        .get_mut(child)
        .expect("scroll content")
        .element
        .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    arena.refresh_subtree_dirty_cache(root);
    let (properties, generations) = synced_paint_state(&arena, &[root]);
    (arena, vec![root], properties, generations)
}

fn prepared_scroll_text_area_scene() -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    prepared_scroll_text_area_scene_with(
        20.0,
        9.0,
        "RetainedAuto must admit bounded internal TextArea scrolling without a ScrollNode",
    )
}

fn prepared_focused_atomic_projection_scroll_text_area_scene() -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    prepared_focused_atomic_projection_scroll_text_area_scene_with_preedit(None)
}

fn prepared_focused_atomic_projection_scroll_text_area_scene_with_preedit(
    preedit: Option<(&str, Option<(usize, usize)>)>,
) -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let width = 108.0;
    let outer_scroll_y = 20.0;
    let content_height = 300.0;
    let content = "before projected after";
    let mut arena = new_test_arena();
    let mut text_area = TextArea::with_stable_id(0xd3_a1c3);
    text_area.set_text(content.to_string());
    text_area.font_size = 17.5;
    text_area.line_height = 1.3;
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(|render| {
        render.range(7..16, |_text_area| crate::ui::RsxNode::text("projected"));
    }));
    text_area.is_focused = true;
    text_area.caret_visible = true;
    text_area.cursor_char = if preedit.is_some() { 8 } else { 7 };
    if let Some((preedit, cursor)) = preedit {
        text_area.ime_preedit = preedit.to_string();
        text_area.ime_preedit_cursor = cursor;
        text_area.children_dirty = true;
        text_area.bump_unified_ifc_source_revision();
        text_area.dirty_flags = DirtyFlags::ALL;
    }

    let text_area = commit_element(&mut arena, Box::new(text_area));
    arena.with_element_taken(text_area, |element, _arena| {
        element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap()
            .set_self_node_key(text_area);
    });
    measure_and_place(
        &mut arena,
        text_area,
        LayoutConstraints {
            max_width: width,
            max_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: width,
            available_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        },
    );

    let wrapper = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(
            0xe2_a3c1,
            0.0,
            -outer_scroll_y,
            width,
            content_height,
        )),
    );
    let root = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xe2_a3c0, 0.0, 0.0, width, 80.0)),
    );
    arena.set_parent(text_area, Some(wrapper));
    arena.set_children(wrapper, vec![text_area]);
    arena.set_parent(wrapper, Some(root));
    arena.set_children(root, vec![wrapper]);
    arena.with_element_taken(text_area, |element, arena| {
        element.place(
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: -outer_scroll_y,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: width,
                available_height: 240.0,
                viewport_width: 320.0,
                viewport_height: 240.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(240.0),
            },
            arena,
        );
    });

    let mut wrapper_style = Style::new();
    wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    crate::view::test_support::get_element_mut::<Element>(&arena, wrapper)
        .apply_style(wrapper_style);

    let mut root_style = Style::new();
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut root_element =
            crate::view::test_support::get_element_mut::<Element>(&arena, root);
        root_element.apply_style(root_style);
        root_element.layout_state.content_size = Size {
            width,
            height: content_height,
        };
        root_element.set_scroll_offset((0.0, outer_scroll_y));
        root_element.clear_local_dirty_flags(DirtyFlags::ALL);
    }

    let mut stack = vec![wrapper];
    while let Some(key) = stack.pop() {
        stack.extend(arena.children_of(key));
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);
    let roots = vec![root];
    let (properties, generations) = synced_paint_state(&arena, &roots);
    (arena, roots, properties, generations)
}

fn prepared_scroll_text_area_scene_with(
    outer_scroll_y: f32,
    local_scroll_y: f32,
    content: &str,
) -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (mut arena, _, text_area) =
        prepared_auto_text_area_with_content(content, local_scroll_y, false);
    let wrapper = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xe2_a311,
        0.0,
        -outer_scroll_y,
        100.0,
        300.0,
    ))));
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xe2_a310, 0.0, 0.0, 100.0, 80.0,
    ))));
    arena.set_parent(text_area, Some(wrapper));
    arena.set_children(wrapper, vec![text_area]);
    arena.set_parent(wrapper, Some(root));
    arena.set_children(root, vec![wrapper]);

    let text_area_place = LayoutPlacement {
        parent_x: 0.0,
        parent_y: -outer_scroll_y,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 108.0,
        available_height: 28.0,
        viewport_width: 320.0,
        viewport_height: 240.0,
        percent_base_width: Some(320.0),
        percent_base_height: Some(240.0),
    };
    arena.with_element_taken(text_area, |element, arena| {
        element.place(text_area_place, arena);
    });
    let mut wrapper_style = Style::new();
    wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    crate::view::test_support::get_element_mut::<Element>(&arena, wrapper)
        .apply_style(wrapper_style);

    let mut root_style = Style::new();
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut root_element =
            crate::view::test_support::get_element_mut::<Element>(&arena, root);
        root_element.apply_style(root_style);
        root_element.layout_state.content_size = Size {
            width: 100.0,
            height: 300.0,
        };
        root_element.set_scroll_offset((0.0, outer_scroll_y));
        root_element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena
        .get_mut(wrapper)
        .expect("scroll content wrapper")
        .element
        .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    let mut stack = vec![text_area];
    while let Some(key) = stack.pop() {
        stack.extend(arena.children_of(key));
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);
    let roots = vec![root];
    let (properties, generations) = synced_paint_state(&arena, &roots);
    (arena, roots, properties, generations)
}

fn update_prepared_scroll_text_area_scene(
    arena: &mut NodeArena,
    roots: &[NodeKey],
    properties: &mut PropertyTrees,
    generations: &mut PaintGenerationTracker,
    outer_scroll_y: f32,
    local_scroll_y: f32,
) {
    let [root] = roots else {
        panic!("C1 fixture must have one root")
    };
    let root_children = arena.children_of(*root);
    let [wrapper] = root_children.as_slice() else {
        panic!("C1 root must have one content wrapper")
    };
    let wrapper = *wrapper;
    let wrapper_children = arena.children_of(wrapper);
    let [text_area] = wrapper_children.as_slice() else {
        panic!("C1 wrapper must have one TextArea")
    };
    let text_area = *text_area;
    crate::view::test_support::get_element_mut::<Element>(arena, wrapper)
        .layout_state
        .layout_position
        .y = -outer_scroll_y;
    {
        let mut root_element =
            crate::view::test_support::get_element_mut::<Element>(arena, *root);
        root_element.set_scroll_offset((0.0, outer_scroll_y));
    }
    let text_area_place = LayoutPlacement {
        parent_x: 0.0,
        parent_y: -outer_scroll_y,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 108.0,
        available_height: 28.0,
        viewport_width: 320.0,
        viewport_height: 240.0,
        percent_base_width: Some(320.0),
        percent_base_height: Some(240.0),
    };
    arena.refresh_subtree_dirty_cache(text_area);
    arena.with_element_taken(text_area, |element, arena| {
        let text_area = element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("C1 child remains TextArea");
        text_area.scroll_y = local_scroll_y;
        element.place(text_area_place, arena);
        let text_area = element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("C1 child remains TextArea");
        text_area.pending_caret_scroll = false;
        text_area.caret_visible = false;
    });
    let mut stack = vec![*root];
    while let Some(key) = stack.pop() {
        stack.extend(arena.children_of(key));
        arena
            .get_mut(key)
            .expect("C1 fixture owner")
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(*root, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(*root);
    properties.sync(arena, roots);
    generations.sync(arena, roots, properties);
}

fn update_prepared_scroll_text_area_selection(
    arena: &NodeArena,
    roots: &[NodeKey],
    properties: &mut PropertyTrees,
    generations: &mut PaintGenerationTracker,
    selection: (Option<usize>, Option<usize>),
    color: Option<Color>,
) {
    let [root] = roots else {
        panic!("C2a fixture must have one root")
    };
    let wrapper = arena.children_of(*root)[0];
    let text_area = arena.children_of(wrapper)[0];
    let mut node = arena.get_mut(text_area).expect("C2a TextArea");
    let text_area = node
        .element
        .as_any_mut()
        .downcast_mut::<TextArea>()
        .expect("C2a TextArea type");
    text_area.selection_anchor_char = selection.0;
    text_area.selection_focus_char = selection.1;
    if let Some(color) = color {
        text_area.selection_background_color = color;
    }
    drop(node);
    properties.sync(arena, roots);
    generations.sync(arena, roots, properties);
}

fn prepared_exact_nested_scroll_scene() -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (arena, outer, _inner, _leaf, properties, generations) =
        crate::view::paint::nested_scroll_plan_fixture();
    (arena, vec![outer], properties, generations)
}

fn prepared_exact_multi_scroll_scene() -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut arena = NodeArena::new();
    let mut roots = Vec::new();
    for (ordinal, offset_y) in [20.0_f32, 36.0].into_iter().enumerate() {
        let stable_base = 0xe2_b300 + u64::try_from(ordinal).unwrap() * 10;
        let root = arena.insert(Node::new(Box::new(Element::new_with_id(
            stable_base,
            0.0,
            0.0,
            100.0,
            80.0,
        ))));
        let child = arena.insert(Node::new(Box::new(Element::new_with_id(
            stable_base + 1,
            0.0,
            -offset_y,
            100.0,
            300.0,
        ))));
        arena.set_parent(child, Some(root));
        arena.push_child(root, child);
        let mut style = Style::new();
        style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        {
            let mut root_node = arena.get_mut(root).expect("scroll root");
            let root_element = root_node
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .expect("Element scroll root");
            root_element.apply_style(style);
            root_element.layout_state.content_size = Size {
                width: 100.0,
                height: 300.0,
            };
            root_element.set_scroll_offset((0.0, offset_y));
            root_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        arena
            .get_mut(child)
            .expect("scroll content")
            .element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        arena.refresh_subtree_dirty_cache(root);
        roots.push(root);
    }
    let (properties, generations) = synced_paint_state(&arena, &roots);
    (arena, roots, properties, generations)
}

fn synced_paint_state(
    arena: &NodeArena,
    roots: &[NodeKey],
) -> (PropertyTrees, PaintGenerationTracker) {
    let mut properties = PropertyTrees::default();
    properties.sync(arena, roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(arena, roots, &properties);
    (properties, generations)
}

fn auto_decision(
    arena: &NodeArena,
    roots: &[NodeKey],
    ctx: &UiBuildContext,
) -> AutoAuthorityDecision {
    let (properties, generations) = synced_paint_state(arena, roots);
    select_retained_auto_authority(arena, roots, &properties, &generations, ctx, true)
}

fn telemetry_for_auto_decision(decision: AutoAuthorityDecision) -> PaintAuthorityTelemetry {
    let (selection, authority, trace) = match decision {
        AutoAuthorityDecision::NativeScrollForest { plan, trace } => (
            RetainedTransformCanarySelection::NativeScrollForestPlanned(plan),
            AutoAuthorityKind::NativeScrollForest,
            trace,
        ),
        AutoAuthorityDecision::PropertyBoundaryDagScene { scene, trace } => (
            RetainedTransformCanarySelection::PropertyBoundaryDagScenePlanned(scene),
            AutoAuthorityKind::PropertyScene,
            trace,
        ),
        AutoAuthorityDecision::NestedScrollScene { prepared, trace } => (
            RetainedTransformCanarySelection::NestedScrollScenePlanned(prepared),
            AutoAuthorityKind::PropertyScene,
            trace,
        ),
        AutoAuthorityDecision::DirectScrollTransformScene { scene, trace } => (
            RetainedTransformCanarySelection::DirectScrollTransformScenePlanned(scene),
            AutoAuthorityKind::PropertyScene,
            trace,
        ),
        AutoAuthorityDecision::PropertyScrollScene { scene, trace } => (
            RetainedTransformCanarySelection::PropertyScrollScenePlanned(scene),
            AutoAuthorityKind::PropertyScene,
            trace,
        ),
        AutoAuthorityDecision::FrameRootScrollScene { scene, trace } => (
            RetainedTransformCanarySelection::FrameRootScrollScenePlanned(scene),
            AutoAuthorityKind::PropertyScene,
            trace,
        ),
        AutoAuthorityDecision::TransformScrollScene { scene, trace } => (
            RetainedTransformCanarySelection::TransformScrollScenePlanned(scene),
            AutoAuthorityKind::PropertyScene,
            trace,
        ),
        AutoAuthorityDecision::EffectScrollScene { scene, trace } => (
            RetainedTransformCanarySelection::EffectScrollScenePlanned(scene),
            AutoAuthorityKind::PropertyScene,
            trace,
        ),
        AutoAuthorityDecision::TransformEffectScrollScene { scene, trace } => (
            RetainedTransformCanarySelection::TransformEffectScrollScenePlanned(scene),
            AutoAuthorityKind::PropertyScene,
            trace,
        ),
        AutoAuthorityDecision::PropertyScene { plan, trace } => (
            RetainedTransformCanarySelection::PropertyScenePlanned(plan),
            AutoAuthorityKind::PropertyScene,
            trace,
        ),
        AutoAuthorityDecision::Artifact { candidate, trace } => (
            RetainedTransformCanarySelection::AutoArtifact(candidate),
            AutoAuthorityKind::Artifact,
            trace,
        ),
        AutoAuthorityDecision::Legacy { trace } => (
            RetainedTransformCanarySelection::AutoLegacy,
            AutoAuthorityKind::Legacy,
            trace,
        ),
    };
    PaintAuthorityTelemetry::from_selection(
        ViewportPaintRendererMode::RetainedAuto,
        &selection,
        Some((authority, trace)),
    )
}

fn auto_authority_kind(decision: &AutoAuthorityDecision) -> AutoAuthorityKind {
    match decision {
        AutoAuthorityDecision::NativeScrollForest { .. } => {
            AutoAuthorityKind::NativeScrollForest
        }
        AutoAuthorityDecision::PropertyBoundaryDagScene { .. } => {
            AutoAuthorityKind::PropertyScene
        }
        AutoAuthorityDecision::NestedScrollScene { .. } => AutoAuthorityKind::PropertyScene,
        AutoAuthorityDecision::DirectScrollTransformScene { .. } => {
            AutoAuthorityKind::PropertyScene
        }
        AutoAuthorityDecision::PropertyScrollScene { .. } => AutoAuthorityKind::PropertyScene,
        AutoAuthorityDecision::FrameRootScrollScene { .. } => AutoAuthorityKind::PropertyScene,
        AutoAuthorityDecision::TransformScrollScene { .. } => AutoAuthorityKind::PropertyScene,
        AutoAuthorityDecision::EffectScrollScene { .. } => AutoAuthorityKind::PropertyScene,
        AutoAuthorityDecision::TransformEffectScrollScene { .. } => {
            AutoAuthorityKind::PropertyScene
        }
        AutoAuthorityDecision::PropertyScene { .. } => AutoAuthorityKind::PropertyScene,
        AutoAuthorityDecision::Artifact { .. } => AutoAuthorityKind::Artifact,
        AutoAuthorityDecision::Legacy { .. } => AutoAuthorityKind::Legacy,
    }
}

fn auto_authority_trace(decision: &AutoAuthorityDecision) -> &super::AutoAuthorityTrace {
    match decision {
        AutoAuthorityDecision::NativeScrollForest { trace, .. }
        | AutoAuthorityDecision::PropertyBoundaryDagScene { trace, .. }
        | AutoAuthorityDecision::NestedScrollScene { trace, .. }
        | AutoAuthorityDecision::DirectScrollTransformScene { trace, .. }
        | AutoAuthorityDecision::PropertyScrollScene { trace, .. }
        | AutoAuthorityDecision::FrameRootScrollScene { trace, .. }
        | AutoAuthorityDecision::TransformScrollScene { trace, .. }
        | AutoAuthorityDecision::EffectScrollScene { trace, .. }
        | AutoAuthorityDecision::TransformEffectScrollScene { trace, .. }
        | AutoAuthorityDecision::PropertyScene { trace, .. }
        | AutoAuthorityDecision::Artifact { trace, .. }
        | AutoAuthorityDecision::Legacy { trace } => trace,
    }
}

fn assert_native_host_retained_closure(
    host: &str,
    arena: &NodeArena,
    roots: &[NodeKey],
    ctx: &UiBuildContext,
) {
    let (properties, generations) = synced_paint_state(arena, roots);
    let record = |mode| {
        crate::view::paint::record_coverage_manifest(
            arena,
            roots,
            false,
            true,
            mode,
            &properties,
            &generations,
        )
    };
    let metadata = record(crate::view::paint::CoverageRecordingMode::MetadataOnly);
    let full = record(crate::view::paint::CoverageRecordingMode::FullArtifact);

    assert!(
        metadata.validation_errors.is_empty(),
        "{host} metadata manifest validation failed: {:?}",
        metadata.validation_errors
    );
    assert!(
        full.validation_errors.is_empty(),
        "{host} full manifest validation failed: {:?}",
        full.validation_errors
    );
    for (pass, manifest) in [("metadata", &metadata), ("full", &full)] {
        assert!(
            manifest.items.iter().all(|item| !matches!(
                item,
                crate::view::paint::PaintCoverageItem::LegacyBoundary { .. }
            )),
            "{host} {pass} manifest must not contain a native LegacyBoundary: {:?}",
            manifest.items
        );
    }
    assert!(
        crate::view::paint::canonical_manifest_matches_for_test(&metadata, &full),
        "{host} metadata/full manifests must be canonical"
    );

    let decision =
        select_retained_auto_authority(arena, roots, &properties, &generations, ctx, true);
    if let AutoAuthorityDecision::Legacy { trace } = decision {
        panic!(
            "native {host} must not select Legacy under RetainedAuto: {:?}",
            trace
                .rejections
                .iter()
                .map(AutoAuthorityRejection::debug_label)
                .collect::<Vec<_>>()
        );
    }
}






fn assert_native_root_opacity_artifact(
    host: &str,
    arena: &NodeArena,
    roots: &[NodeKey],
    opacity: f32,
) {
    let root = roots[0];
    let (properties, generations) = synced_paint_state(arena, roots);
    let metadata = crate::view::paint::record_coverage_manifest(
        arena,
        roots,
        false,
        true,
        crate::view::paint::CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
    );
    let full = crate::view::paint::record_coverage_manifest(
        arena,
        roots,
        false,
        true,
        crate::view::paint::CoverageRecordingMode::FullArtifact,
        &properties,
        &generations,
    );
    assert!(metadata.validation_errors.is_empty(), "{host}: metadata");
    assert!(full.validation_errors.is_empty(), "{host}: full artifact");
    assert!(
        crate::view::paint::canonical_manifest_matches_for_test(&metadata, &full),
        "{host}: metadata/full canonical identity"
    );

    let selection_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let AutoAuthorityDecision::Artifact { candidate, trace } = select_retained_auto_authority(
        arena,
        roots,
        &properties,
        &generations,
        &selection_ctx,
        true,
    ) else {
        panic!("{host}: native root opacity must select artifact authority")
    };
    assert!(candidate.eligibility.eligible, "{host}: eligibility");
    assert!(trace.rejections.is_empty(), "{host}: {trace:?}");
    if opacity.to_bits() == 1.0_f32.to_bits() {
        assert!(matches!(
            candidate.artifact.target,
            crate::view::paint::PaintArtifactTarget::CurrentTarget
        ));
    } else {
        assert!(matches!(
            candidate.artifact.target,
            crate::view::paint::PaintArtifactTarget::RootOpacityGroup {
                root: owner,
                effect,
            } if owner == root
                && effect == crate::view::compositor::property_tree::EffectNodeId(root)
        ));
        assert!(candidate.artifact.effect_nodes.iter().any(|snapshot| {
            snapshot.id == crate::view::compositor::property_tree::EffectNodeId(root)
                && snapshot.opacity.to_bits() == opacity.to_bits()
                && snapshot.generation != 0
        }));
    }

    let mut graph = FrameGraph::new();
    let mut compile_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = compile_ctx.allocate_target(&mut graph);
    compile_ctx.set_current_target(target);
    let root_effect_plan = (opacity.to_bits() != 1.0_f32.to_bits()).then(|| {
        let key = crate::view::base_component::root_effect_stable_key(root);
        let desc = compile_ctx.persistent_full_viewport_target_desc(key);
        RootEffectBuildPlan {
            committed: RootEffectRetainedState::Invalid,
            key,
            target: crate::view::paint::RootEffectRasterInputs {
                width: desc.width(),
                height: desc.height(),
                format: desc.format(),
                sample_count: desc.sample_count(),
                scale_factor_bits: compile_ctx.viewport().scale_factor().to_bits(),
            },
            pair_resident: false,
        }
    });
    assert!(matches!(
        try_compile_recorded_artifact_frame(
            &mut graph,
            candidate,
            &compile_ctx,
            root_effect_plan.as_ref(),
        ),
        PropertyNeutralArtifactAttempt::Compiled { .. }
    ));
    let composites = graph.test_graphics_passes::<
        crate::view::render_pass::composite_layer_pass::CompositeLayerPass,
    >();
    if opacity.to_bits() == 1.0_f32.to_bits() {
        assert!(composites.is_empty(), "{host}: neutral opacity");
    } else {
        assert_eq!(composites.len(), 1, "{host}: one root composite");
        assert_eq!(
            composites[0].test_params().opacity.to_bits(),
            opacity.to_bits(),
            "{host}: final composite opacity"
        );
    }
}

fn assert_native_property_scene_authority(
    host: &str,
    arena: &NodeArena,
    roots: &[NodeKey],
    emit_transaction: bool,
) {
    let selection_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (properties, generations) = synced_paint_state(arena, roots);
    let AutoAuthorityDecision::PropertyScene { plan, trace } = select_retained_auto_authority(
        arena,
        roots,
        &properties,
        &generations,
        &selection_ctx,
        true,
    ) else {
        panic!("{host}: native property topology must select PropertyScene")
    };
    assert!(
        !trace.rejections.iter().any(|rejection| matches!(
            rejection,
            AutoAuthorityRejection::Plan {
                authority: AutoAuthorityKind::PropertyScene,
                ..
            }
        )),
        "{host}: selected PropertyScene cannot contain its own terminal rejection: {trace:?}"
    );
    if !emit_transaction {
        return;
    }

    let mut viewport = Viewport::new();
    let frame_owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("native property scene frame stage");
    let mut graph = FrameGraph::new();
    let mut execution_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = execution_ctx.allocate_target(&mut graph);
    execution_ctx.set_current_target(target);
    graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: execution_ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: target,
        },
    ));
    let prepared = crate::view::paint::prepare_retained_property_scene_from_pool(
        &viewport,
        &plan,
        &graph,
        &execution_ctx,
    )
    .unwrap_or_else(|error| panic!("{host}: property-scene preflight failed: {error:?}"));
    let outcome = crate::view::paint::emit_prepared_retained_property_scene(
        &mut viewport,
        prepared,
        &mut graph,
        execution_ctx,
    );
    let (_state, build_trace) = outcome.into_parts();
    assert!(
        !build_trace.surfaces.is_empty(),
        "{host}: retained surfaces"
    );
    assert!(
        graph.test_compile_snapshot().is_ok(),
        "{host}: graph compile"
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true,));
}








































































struct TransparentContentsClipParent {
    id: u64,
    scissor: [u32; 4],
    children: Vec<NodeKey>,
}

impl Layoutable for TransparentContentsClipParent {
    fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
    fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
    fn measured_size(&self) -> (f32, f32) {
        (1.0, 1.0)
    }
    fn set_layout_width(&mut self, _width: f32) {}
    fn set_layout_height(&mut self, _height: f32) {}
}

impl EventTarget for TransparentContentsClipParent {}

impl Renderable for TransparentContentsClipParent {
    fn build(
        &mut self,
        _graph: &mut FrameGraph,
        _arena: &mut NodeArena,
        ctx: UiBuildContext,
    ) -> BuildState {
        ctx.into_state()
    }
}

impl ElementTrait for TransparentContentsClipParent {
    fn stable_id(&self) -> u64 {
        self.id
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.id,
            parent_id: None,
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            border_radius: 0.0,
            should_render: true,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn shadow_paint_recording_capability(
        &self,
        _arena: &NodeArena,
        _deferred_phase_root: bool,
        _recording_context: crate::view::paint::PaintRecordingContext,
    ) -> ShadowPaintRecordingCapability {
        ShadowPaintRecordingCapability::Transparent
    }

    fn contents_logical_scissor(&self) -> Option<[u32; 4]> {
        Some(self.scissor)
    }

    fn children(&self) -> &[NodeKey] {
        &self.children
    }

    fn sync_children_mirror(&mut self, children: &[NodeKey]) {
        self.children.clear();
        self.children.extend_from_slice(children);
    }
}

fn prepared_contents_clipped_leaf() -> (NodeArena, Vec<NodeKey>) {
    let mut arena = new_test_arena();
    let parent = commit_element(
        &mut arena,
        Box::new(TransparentContentsClipParent {
            id: 0x8c20,
            scissor: [4, 6, 24, 18],
            children: Vec::new(),
        }),
    );
    let child = commit_child(
        &mut arena,
        parent,
        Box::new(colored_element(0x8c21, 10.0, Color::rgb(230, 20, 30))),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, child, measure, place);
    (arena, vec![parent])
}

fn prepared_outer_shadow_leaf(opacity: f32, blur: f32) -> (NodeArena, Vec<NodeKey>) {
    let mut element = colored_element(0x6d50, 10.25, Color::rgb(230, 20, 30));
    let mut style = Style::new();
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(230, 20, 30)),
    );
    style.set_box_shadow(vec![
        BoxShadow::new()
            .color(Color::rgb(20, 40, 220))
            .offset_x(2.0)
            .offset_y(3.0)
            .blur(blur),
    ]);
    element.apply_style(style);
    element.set_opacity(opacity);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(element));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root])
}


fn prepared_mixed_eligibility_roots() -> (NodeArena, Vec<NodeKey>) {
    let mut arena = new_test_arena();
    let safe_leaf = commit_element(
        &mut arena,
        Box::new(colored_element(10, 10.0, Color::rgb(230, 20, 30))),
    );
    let legacy_subtree = commit_element(
        &mut arena,
        Box::new(colored_element(20, 110.0, Color::rgb(20, 210, 40))),
    );
    commit_child(
        &mut arena,
        legacy_subtree,
        Box::new(colored_element(21, 10.0, Color::rgb(30, 40, 220))),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, safe_leaf, measure, place);
    measure_and_place(&mut arena, legacy_subtree, measure, place);
    (arena, vec![safe_leaf, legacy_subtree])
}

fn build_roots_graph(
    mut arena: NodeArena,
    roots: &[NodeKey],
    through_production_dispatch: bool,
) -> FrameGraph {
    if through_production_dispatch {
        return build_roots_graph_with_renderer_mode(
            arena,
            roots,
            ViewportPaintRendererMode::Legacy,
        );
    }
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    for &root_key in roots {
        let child_ctx = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next_state = arena
            .with_element_taken(root_key, |root, arena| {
                root.build(&mut graph, arena, child_ctx)
            })
            .expect("legacy root should exist");
        ctx.set_state(next_state);
    }
    graph
}

fn build_roots_graph_with_renderer_mode(
    mut arena: NodeArena,
    roots: &[NodeKey],
    mode: ViewportPaintRendererMode,
) -> FrameGraph {
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, roots, &properties);

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
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
    let root_effect_plan = roots.first().copied().and_then(|root| {
        (roots.len() == 1).then(|| {
            let key = crate::view::base_component::root_effect_stable_key(root);
            let desc = ctx.persistent_full_viewport_target_desc(key);
            RootEffectBuildPlan {
                committed: RootEffectRetainedState::Invalid,
                key,
                target: crate::view::paint::RootEffectRasterInputs {
                    width: desc.width(),
                    height: desc.height(),
                    format: desc.format(),
                    sample_count: desc.sample_count(),
                    scale_factor_bits: ctx.viewport().scale_factor().to_bits(),
                },
                pair_resident: false,
            }
        })
    });
    let attempt = try_build_property_neutral_artifact_frame(
        &mut graph,
        &arena,
        roots,
        &properties,
        &generations,
        mode,
        &ctx,
        root_effect_plan.as_ref(),
    );
    match attempt {
        PropertyNeutralArtifactAttempt::Compiled { state, .. } => ctx.set_state(state),
        PropertyNeutralArtifactAttempt::WholeFrameLegacy { .. }
        | PropertyNeutralArtifactAttempt::CompileRejected(_) => {
            for &root_key in roots {
                let child_ctx = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
                let next_state = build_root_legacy(&mut graph, &mut arena, root_key, child_ctx);
                ctx.set_state(next_state);
            }
        }
    }
    graph
}

fn artifact_canary_attempt(
    arena: &NodeArena,
    roots: &[NodeKey],
) -> PropertyNeutralArtifactAttempt {
    let mut properties = PropertyTrees::default();
    properties.sync(arena, roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(arena, roots, &properties);
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    try_build_property_neutral_artifact_frame(
        &mut graph,
        arena,
        roots,
        &properties,
        &generations,
        ViewportPaintRendererMode::ArtifactCanary,
        &ctx,
        None,
    )
}

fn preflight_fallback_reasons(
    arena: &NodeArena,
    roots: &[NodeKey],
) -> Vec<crate::view::paint::FrameArtifactFallbackReason> {
    crate::view::paint::take_full_artifact_record_count();
    let attempt = artifact_canary_attempt(arena, roots);
    let PropertyNeutralArtifactAttempt::WholeFrameLegacy { eligibility } = attempt else {
        panic!("unsupported production property must fall back during metadata preflight")
    };
    assert_eq!(
        crate::view::paint::take_full_artifact_record_count(),
        0,
        "metadata rejection must happen before every full hook",
    );
    eligibility.reasons
}

fn observe_compositor_state(
    arena: &NodeArena,
    roots: &[NodeKey],
    properties: &mut PropertyTrees,
    generations: &mut PaintGenerationTracker,
) {
    properties.sync(arena, roots);
    generations.sync(arena, roots, properties);
}

fn set_opacity_with_invalidation(arena: &mut NodeArena, key: NodeKey, opacity: f32) {
    arena
        .mutate_element_with_invalidation(key, |element, cx| {
            element
                .as_any_mut()
                .downcast_mut::<Element>()
                .expect("test root should be Element")
                .set_opacity_with_invalidation(opacity, cx);
        })
        .expect("test root should exist");
}

fn assert_consumed_dirty_cleared(arena: &NodeArena, key: NodeKey) {
    let consumed = DirtyFlags::PAINT.union(DirtyFlags::COMPOSITE);
    assert!(
        !arena
            .get(key)
            .expect("test node should exist")
            .element
            .local_dirty_flags()
            .intersects(consumed)
    );
    assert!(!arena.arena_local_dirty(key).intersects(consumed));
    assert!(!arena.cached_subtree_dirty(key).intersects(consumed));
}

fn assert_composite_dirty_preserved(arena: &NodeArena, key: NodeKey) {
    assert!(
        arena
            .get(key)
            .expect("test node should exist")
            .element
            .local_dirty_flags()
            .contains(DirtyFlags::COMPOSITE)
    );
    assert!(arena.arena_local_dirty(key).contains(DirtyFlags::COMPOSITE));
    assert!(
        arena
            .cached_subtree_dirty(key)
            .contains(DirtyFlags::COMPOSITE)
    );
}

mod native_authority_tests;
mod text_transform_tests;
mod window_showcase_tests;
mod telemetry_tests;
mod scroll_forest_tests;
mod nested_scroll_tests;
mod text_area_scene_tests;
mod text_area_caret_reuse_tests;
mod text_area_interaction_tests;
mod scroll_topology_tests;
mod scroll_production_dispatch_tests;
mod mode_and_failure_tests;
mod canary_tests;
mod production_artifact_tests;
mod composite_dirty_tests;
