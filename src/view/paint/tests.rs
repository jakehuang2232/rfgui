#[cfg(not(target_arch = "wasm32"))]
mod gpu_equivalence_tests;

use std::any::Any;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use slotmap::Key;

use crate::style::{
    Border, BorderRadius, BoxShadow, ClipMode, Color, ColorLike, Gradient, Layout, Length,
    Opacity, ParsedValue, Position, PropertyId, ScrollDirection, SideOrCorner, Style,
    TextAlign, TextWrap, Transform, Translate,
};
use crate::view::base_component::text_area::{TextAreaProjectionSegment, TextAreaTextRun};
use crate::view::base_component::{
    BoxModelSnapshot, BuildState, CustomLeafPaintContext, CustomLeafPaintRecorder,
    CustomWrapperPaintContext, CustomWrapperPaintRecorder, DirtyFlags, Element, ElementTrait,
    EventTarget, Image, LayoutConstraints, LayoutPlacement, Layoutable,
    OwningInlineIfcRootWitnessDamage, Rect, Renderable, ShadowPaintBlocker,
    ShadowPaintRecordingCapability, Size, Svg, Text, TextArea, UiBuildContext,
};
use crate::view::compositor::property_tree::{
    ClipBehavior, ClipNodeId, ClipNodeRole, ClipNodeSnapshot, EffectNodeId, EffectNodeSnapshot,
    PropertyTreeState, TransformNodeId,
};
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::frame_graph::{FrameGraph, FrameGraphTestSnapshot, FramePassTestPayload};
use crate::view::node_arena::{Node, NodeArena, NodeKey};
use crate::view::render_pass::draw_rect_pass::{
    DrawRectInput, DrawRectOutput, DrawRectPass, RectPassParams, RectPassTestSnapshot,
    RectRenderMode,
};
use crate::view::test_support::{
    commit_child, commit_element, measure_and_place, new_test_arena,
};
use crate::view::{ImageSource, SvgSource};

use super::*;

pub(super) fn exact_isolation_fixture(
    opacity: f32,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let styled = |id, x, y, width, height, color| {
        let mut element = Element::new_with_id(id, x, y, width, height);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
        element.apply_style(style);
        element
    };
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(styled(
            0x9f_3001,
            8.0,
            7.0,
            120.0,
            90.0,
            Color::rgb(220, 40, 30),
        )),
    );
    commit_child(
        &mut arena,
        root,
        Box::new(styled(
            0x9f_3002,
            18.0,
            12.0,
            26.0,
            20.0,
            Color::rgb(20, 80, 230),
        )),
    );
    measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 160.0,
            max_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 160.0,
            available_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        },
    );
    crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(opacity);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, properties, generations)
}

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

fn leaf_element(id: u64, color: Color, opacity: f32, border: bool) -> Element {
    let mut element = Element::new_with_id(id, 10.25, 20.75, 80.0, 40.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    if border {
        style.set_border(Border::uniform(Length::px(3.0), &Color::hex("#102030")));
    }
    element.apply_style(style);
    element.set_opacity(opacity);
    element
}

fn gradient(start: &str, end: &str) -> Gradient {
    Gradient::linear(SideOrCorner::Right)
        .stop(Color::hex(start), Some(Length::percent(0.0)))
        .stop(Color::hex(end), Some(Length::percent(100.0)))
        .build()
}

fn apply_gradient_style(
    element: &mut Element,
    background_start: &str,
    background_end: &str,
    border_start: &str,
    border_end: &str,
) {
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(20, 20, 20)),
    );
    style.set_border(Border::uniform(Length::px(3.0), &Color::hex("#102030")));
    style.set_background_image(gradient(background_start, background_end));
    style.set_border_image(gradient(border_start, border_end));
    element.apply_style(style);
}

fn sync_identity(
    arena: &NodeArena,
    roots: &[NodeKey],
) -> (PropertyTrees, PaintGenerationTracker) {
    let mut properties = PropertyTrees::default();
    properties.sync(arena, roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(arena, roots, &properties);
    (properties, generations)
}

fn prepared_leaf(
    id: u64,
    color: Color,
    opacity: f32,
    border: bool,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(leaf_element(id, color, opacity, border)),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let (properties, generations) = sync_identity(&arena, &[root]);
    (arena, root, properties, generations)
}

fn prepared_shadow_leaf(
    id: u64,
    opacity: f32,
    shadows: Vec<BoxShadow>,
    border: bool,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let mut element = Element::new_with_id(id, 10.25, 20.75, 80.0, 40.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(40, 80, 160)),
    );
    style.set_box_shadow(shadows);
    if border {
        style.set_border(Border::uniform(Length::px(3.0), &Color::hex("#102030")));
    }
    element.apply_style(style);
    element.set_opacity(opacity);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(element));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let (properties, generations) = sync_identity(&arena, &[root]);
    (arena, root, properties, generations)
}

fn prepared_shadow_owner_tree(
    id: u64,
    opacity: f32,
) -> (
    NodeArena,
    NodeKey,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (mut arena, root, _, _) = prepared_shadow_leaf(id, opacity, two_outer_shadows(), true);
    let small_child = |id, color| {
        let mut child = Element::new_with_id(id, 0.0, 0.0, 8.0, 8.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(8.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(8.0)));
        style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
        child.apply_style(style);
        child
    };
    let first = commit_child(
        &mut arena,
        root,
        Box::new(small_child(id + 1, Color::rgb(20, 180, 40))),
    );
    let second = commit_child(
        &mut arena,
        root,
        Box::new(small_child(id + 2, Color::rgb(180, 40, 120))),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let (properties, generations) = sync_identity(&arena, &[root]);
    (arena, root, first, second, properties, generations)
}

fn anchor_parent_self_clip_roots(opacity: f32, border: bool) -> (NodeArena, Vec<NodeKey>) {
    let mut arena = new_test_arena();
    let mut clipped = leaf_element(210, Color::rgb(220, 40, 30), opacity, border);
    let mut position = Style::new();
    position.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(10.25))
                .top(Length::px(12.75))
                .clip(ClipMode::AnchorParent),
        ),
    );
    clipped.apply_style(position);
    clipped.set_opacity(opacity);
    let clipped = commit_element(&mut arena, Box::new(clipped));
    let sibling = commit_element(
        &mut arena,
        Box::new(leaf_element(211, Color::rgb(30, 60, 220), 1.0, false)),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, clipped, measure, place);
    measure_and_place(&mut arena, sibling, measure, place);
    (arena, vec![clipped, sibling])
}

fn anchor_parent_self_clip_shadow_root() -> (NodeArena, Vec<NodeKey>) {
    let mut arena = new_test_arena();
    let mut clipped = leaf_element(212, Color::rgb(220, 40, 30), 1.0, true);
    let mut style = Style::new();
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(10.25))
                .top(Length::px(12.75))
                .clip(ClipMode::AnchorParent),
        ),
    );
    style.set_box_shadow(vec![
        BoxShadow::new()
            .color(Color::rgb(20, 40, 220))
            .offset_x(-3.0)
            .offset_y(4.5),
    ]);
    clipped.apply_style(style);
    let clipped = commit_element(&mut arena, Box::new(clipped));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, clipped, measure, place);
    (arena, vec![clipped])
}

fn nested_anchor_parent_mixed_siblings(
    anchor_first: bool,
) -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let mut arena = new_test_arena();
    let mut root = Element::new_with_id(220, 0.0, 0.0, 320.0, 240.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root.apply_style(root_style);
    let root = commit_element(&mut arena, Box::new(root));

    let mut anchor = leaf_element(221, Color::rgb(220, 30, 20), 1.0, false);
    let mut anchor_style = Style::new();
    anchor_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(30.0))
                .top(Length::px(24.0))
                .clip(ClipMode::AnchorParent),
        ),
    );
    anchor.apply_style(anchor_style);
    anchor.set_background_color_value(Color::rgb(220, 30, 20));
    anchor.set_opacity(1.0);

    let mut normal = leaf_element(222, Color::rgb(20, 40, 220), 1.0, false);
    let mut normal_style = Style::new();
    normal_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(30.0))
                .top(Length::px(24.0))
                .clip(ClipMode::Parent),
        ),
    );
    normal.apply_style(normal_style);
    normal.set_background_color_value(Color::rgb(20, 40, 220));
    normal.set_opacity(1.0);

    let (anchor, normal) = if anchor_first {
        (
            commit_child(&mut arena, root, Box::new(anchor)),
            commit_child(&mut arena, root, Box::new(normal)),
        )
    } else {
        let normal = commit_child(&mut arena, root, Box::new(normal));
        let anchor = commit_child(&mut arena, root, Box::new(anchor));
        (anchor, normal)
    };
    let _ = normal;
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root], anchor)
}

fn nested_deferred_viewport_popups()
-> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey, NodeKey) {
    let mut arena = new_test_arena();
    let mut root = Element::new_with_id(0x8d20, 0.0, 0.0, 320.0, 240.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(24, 40, 72)),
    );
    root.apply_style(root_style);
    let root = commit_element(&mut arena, Box::new(root));

    let normal = commit_child(
        &mut arena,
        root,
        Box::new(leaf_element(0x8d21, Color::rgb(20, 80, 220), 1.0, false)),
    );

    let deferred = |id, color| {
        let mut element = Element::new_with_id(id, 0.0, 0.0, 160.0, 100.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(18.0))
                    .top(Length::px(16.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        element.apply_style(style);
        element
    };
    let first = commit_child(
        &mut arena,
        root,
        Box::new(deferred(0x8d22, Color::rgb(220, 40, 30))),
    );
    let nested_child = commit_child(
        &mut arena,
        first,
        Box::new(leaf_element(0x8d23, Color::rgb(30, 190, 80), 1.0, false)),
    );
    let second = commit_child(
        &mut arena,
        root,
        Box::new(deferred(0x8d24, Color::rgb(180, 40, 180))),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root], normal, first, nested_child, second)
}

fn mixed_native_anchor_parent_siblings(
    normal_last: bool,
) -> (NodeArena, Vec<NodeKey>, Vec<NodeKey>) {
    const SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><rect width='8' height='8' fill='#22c55e'/></svg>";
    let mut arena = new_test_arena();
    let mut root = Element::new_with_id(0x8d30, 0.0, 0.0, 320.0, 240.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root.apply_style(root_style);
    let root = commit_element(&mut arena, Box::new(root));

    let normal_element = || leaf_element(0x8d31, Color::rgb(20, 40, 220), 1.0, false);
    let anchor_style = || {
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(8.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(8.0)));
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(12.0))
                    .top(Length::px(14.0))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        style
    };
    let mut element_anchor = leaf_element(0x8d32, Color::rgb(220, 30, 20), 1.0, false);
    element_anchor.apply_style(anchor_style());
    let mut image_anchor = Image::new_with_id(
        0x8d33,
        ImageSource::Rgba {
            width: 1,
            height: 1,
            pixels: Arc::from([255, 255, 255, 255]),
        },
    );
    image_anchor.apply_style(anchor_style());
    let mut svg_anchor = Svg::new_with_id(0x8d34, SvgSource::Content(SVG.into()));
    svg_anchor.apply_style(anchor_style());

    let normal = if normal_last {
        None
    } else {
        Some(commit_child(&mut arena, root, Box::new(normal_element())))
    };
    let anchors = vec![
        commit_child(&mut arena, root, Box::new(element_anchor)),
        commit_child(&mut arena, root, Box::new(image_anchor)),
        commit_child(&mut arena, root, Box::new(svg_anchor)),
    ];
    let normal =
        normal.unwrap_or_else(|| commit_child(&mut arena, root, Box::new(normal_element())));
    let _ = normal;
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root], anchors)
}

fn artifact_graph(
    arena: &NodeArena,
    root: NodeKey,
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
) -> FrameGraph {
    let outcome = record_root(arena, root, properties, generations);
    let PaintRecordOutcome::Artifact(artifact) = outcome else {
        panic!("safe leaf should record an artifact: {outcome:?}");
    };
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    let _ = compile_artifact(&artifact, &mut graph, ctx);
    graph
}

fn legacy_graph(mut arena: NodeArena, root: NodeKey) -> FrameGraph {
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    let _ = arena
        .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
        .expect("legacy root should build");
    graph
}





fn fallback_reason(element: Box<dyn ElementTrait>) -> LegacyPaintReason {
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, element);
    let (properties, generations) = sync_identity(&arena, &[root]);
    let PaintRecordOutcome::LegacySubtree(legacy) =
        record_root(&arena, root, &properties, &generations)
    else {
        panic!("host should remain legacy");
    };
    legacy.reason
}


fn prepared_image_fixture(
    pixels: Arc<[u8]>,
    fit: crate::view::ImageFit,
    sampling: crate::view::ImageSampling,
    opacity: f32,
) -> (NodeArena, Vec<NodeKey>) {
    let mut image = Image::new_with_id(
        24,
        ImageSource::Rgba {
            width: 2,
            height: 2,
            pixels,
        },
    );
    image.set_fit(fit);
    image.set_sampling(sampling);
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(48.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(32.0)));
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(20, 40, 60)),
    );
    style.set_border(Border::uniform(Length::px(4.0), &Color::rgb(180, 30, 20)));
    style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(Opacity::new(opacity)),
    );
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(10.25))
                .top(Length::px(12.75)),
        ),
    );
    image.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(image));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root])
}

struct TransparentContentsClipParent {
    id: u64,
    opacity: f32,
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
        _recording_context: PaintRecordingContext,
    ) -> ShadowPaintRecordingCapability {
        ShadowPaintRecordingCapability::Transparent
    }

    fn contents_logical_scissor(&self) -> Option<[u32; 4]> {
        Some(self.scissor)
    }

    fn retained_paint_properties(
        &self,
    ) -> crate::view::base_component::RetainedPaintProperties {
        crate::view::base_component::RetainedPaintProperties {
            opacity: self.opacity,
            ..Default::default()
        }
    }

    fn children(&self) -> &[NodeKey] {
        &self.children
    }

    fn sync_children_mirror(&mut self, children: &[NodeKey]) {
        self.children.clear();
        self.children.extend_from_slice(children);
    }
}

fn root_opacity_contents_clip_fixture(
    scissor: [u32; 4],
) -> (
    NodeArena,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(TransparentContentsClipParent {
            id: 0x8c20,
            opacity: 0.5,
            scissor,
            children: Vec::new(),
        }),
    );
    let child = commit_child(
        &mut arena,
        root,
        Box::new(leaf_element(0x8c21, Color::rgb(30, 180, 90), 1.0, false)),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, child, measure, place);
    let (properties, generations) = sync_identity(&arena, &[root]);
    (arena, root, child, properties, generations)
}

fn bare_image_fixture(
    pixels: Arc<[u8]>,
    fit: crate::view::ImageFit,
    sampling: crate::view::ImageSampling,
    opacity: f32,
) -> (NodeArena, Vec<NodeKey>) {
    let mut image = Image::new_with_id(
        26,
        ImageSource::Rgba {
            width: 2,
            height: 2,
            pixels,
        },
    );
    image.set_fit(fit);
    image.set_sampling(sampling);
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(47.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(31.0)));
    style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(Opacity::new(opacity)),
    );
    image.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(image));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root])
}




fn assert_image_metadata_fallback(
    arena: &NodeArena,
    roots: &[NodeKey],
    expected: LegacyPaintReason,
) {
    let (properties, generations) = sync_identity(arena, roots);
    let _ = take_full_artifact_record_count();
    let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) =
        record_frame_artifact(arena, roots, &properties, &generations, RendererMode::Auto)
            .expect("auto fallback")
    else {
        panic!("image must remain legacy: expected {expected:?}")
    };
    assert!(
        eligibility
            .reasons
            .contains(&FrameArtifactFallbackReason::LegacyBoundary(expected)),
        "{eligibility:?}"
    );
    assert_eq!(take_full_artifact_record_count(), 0);
}








struct RecordingHost {
    id: u64,
    builds: Arc<AtomicUsize>,
    fill: Option<[f32; 4]>,
}

enum CustomLeafRecordMode {
    Fill { rgba: [f32; 4], opacity: f32 },
    InvalidBounds,
    DoubleFill,
    Drift { calls: Arc<AtomicUsize> },
}

struct CustomLeafPaintHost {
    id: u64,
    bounds: Rect,
    mode: CustomLeafRecordMode,
    children: Vec<NodeKey>,
    expose_children: bool,
    deferred: bool,
    active_animator: bool,
    retained_properties: crate::view::base_component::RetainedPaintProperties,
}

enum CustomWrapperRecordMode {
    Canonical,
    InvalidBounds,
    Empty,
    Overflow,
    Drift { calls: Arc<AtomicUsize> },
}

struct CustomWrapperPaintHost {
    id: u64,
    bounds: Rect,
    mode: CustomWrapperRecordMode,
    children: Vec<NodeKey>,
}

impl CustomWrapperPaintHost {
    fn canonical(id: u64) -> Self {
        Self {
            id,
            bounds: Rect {
                x: 3.0,
                y: 5.0,
                width: 24.0,
                height: 12.0,
            },
            mode: CustomWrapperRecordMode::Canonical,
            children: Vec::new(),
        }
    }

    fn emit_legacy_fill(
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        bounds: Rect,
        mut rgba: [f32; 4],
        opacity: f32,
    ) {
        rgba[3] *= opacity;
        let mut pass = DrawRectPass::new(
            RectPassParams {
                position: [bounds.x, bounds.y],
                size: [bounds.width, bounds.height],
                fill_color: rgba,
                opacity: 1.0,
                ..Default::default()
            },
            DrawRectInput::default(),
            DrawRectOutput::default(),
        );
        pass.set_render_mode(RectRenderMode::FillOnly);
        ctx.emit_draw_rect_pass(graph, pass);
    }
}

impl Layoutable for CustomWrapperPaintHost {
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

impl EventTarget for CustomWrapperPaintHost {}

impl Renderable for CustomWrapperPaintHost {
    fn build(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut NodeArena,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        for (rgba, opacity) in [([0.8, 0.0, 0.0, 1.0], 1.0), ([0.0, 0.8, 0.0, 1.0], 0.5)] {
            Self::emit_legacy_fill(graph, &mut ctx, self.bounds, rgba, opacity);
        }
        for child_key in self.children.clone() {
            let child_ctx = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
            if let Some(state) = arena.with_element_taken(child_key, |child, arena| {
                child.build(graph, arena, child_ctx)
            }) {
                ctx.set_state(state);
            }
        }
        for (rgba, opacity) in [([0.0, 0.0, 0.8, 1.0], 1.0), ([0.8, 0.8, 0.0, 1.0], 0.25)] {
            Self::emit_legacy_fill(graph, &mut ctx, self.bounds, rgba, opacity);
        }
        ctx.into_state()
    }
}

impl ElementTrait for CustomWrapperPaintHost {
    fn stable_id(&self) -> u64 {
        self.id
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.id,
            parent_id: None,
            x: self.bounds.x,
            y: self.bounds.y,
            width: self.bounds.width,
            height: self.bounds.height,
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

    fn record_custom_wrapper_paint(
        &self,
        context: CustomWrapperPaintContext,
        recorder: &mut CustomWrapperPaintRecorder,
    ) {
        let bounds = context.bounds();
        match &self.mode {
            CustomWrapperRecordMode::Canonical => {
                recorder.fill_rect_before_children(bounds, [0.8, 0.0, 0.0, 1.0], 1.0);
                recorder.fill_rect_before_children(bounds, [0.0, 0.8, 0.0, 1.0], 0.5);
                recorder.fill_rect_after_children(bounds, [0.0, 0.0, 0.8, 1.0], 1.0);
                recorder.fill_rect_after_children(bounds, [0.8, 0.8, 0.0, 1.0], 0.25);
            }
            CustomWrapperRecordMode::InvalidBounds => {
                recorder.fill_rect_before_children(
                    Rect {
                        x: f32::NAN,
                        ..bounds
                    },
                    [0.8, 0.0, 0.0, 1.0],
                    1.0,
                );
            }
            CustomWrapperRecordMode::Empty => {}
            CustomWrapperRecordMode::Overflow => {
                for _ in 0..=(u16::MAX as usize + 1) {
                    recorder.fill_rect_before_children(bounds, [0.8, 0.0, 0.0, 1.0], 1.0);
                }
            }
            CustomWrapperRecordMode::Drift { calls } => {
                let call = calls.fetch_add(1, Ordering::Relaxed);
                let green = if call < 2 { 0.2 } else { 0.7 };
                recorder.fill_rect_before_children(bounds, [0.8, green, 0.0, 1.0], 1.0);
                recorder.fill_rect_after_children(bounds, [0.0, 0.0, 0.8, 1.0], 1.0);
            }
        }
    }

    fn children(&self) -> &[NodeKey] {
        &self.children
    }

    fn sync_children_mirror(&mut self, children: &[NodeKey]) {
        self.children.clear();
        self.children.extend_from_slice(children);
    }
}

impl CustomLeafPaintHost {
    fn fill(id: u64) -> Self {
        Self {
            id,
            bounds: Rect {
                x: 4.0,
                y: 6.0,
                width: 20.0,
                height: 10.0,
            },
            mode: CustomLeafRecordMode::Fill {
                rgba: [0.1, 0.2, 0.3, 1.0],
                opacity: 0.75,
            },
            children: Vec::new(),
            expose_children: true,
            deferred: false,
            active_animator: false,
            retained_properties: Default::default(),
        }
    }
}

impl Layoutable for CustomLeafPaintHost {
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

impl EventTarget for CustomLeafPaintHost {}

impl Renderable for CustomLeafPaintHost {
    fn build(
        &mut self,
        graph: &mut FrameGraph,
        _arena: &mut NodeArena,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        if let CustomLeafRecordMode::Fill { rgba, opacity } = &self.mode {
            let pass = DrawRectPass::new(
                RectPassParams {
                    position: [self.bounds.x, self.bounds.y],
                    size: [self.bounds.width, self.bounds.height],
                    fill_color: *rgba,
                    opacity: *opacity,
                    ..Default::default()
                },
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            ctx.emit_draw_rect_pass(graph, pass);
        }
        ctx.into_state()
    }
}

impl ElementTrait for CustomLeafPaintHost {
    fn stable_id(&self) -> u64 {
        self.id
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.id,
            parent_id: None,
            x: self.bounds.x,
            y: self.bounds.y,
            width: self.bounds.width,
            height: self.bounds.height,
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

    fn record_custom_leaf_paint(
        &self,
        context: CustomLeafPaintContext,
        recorder: &mut CustomLeafPaintRecorder,
    ) {
        let bounds = context.bounds();
        match &self.mode {
            CustomLeafRecordMode::Fill { rgba, opacity } => {
                recorder.fill_rect(bounds, *rgba, *opacity);
            }
            CustomLeafRecordMode::InvalidBounds => recorder.fill_rect(
                Rect {
                    x: f32::NAN,
                    ..bounds
                },
                [0.1, 0.2, 0.3, 1.0],
                1.0,
            ),
            CustomLeafRecordMode::DoubleFill => {
                recorder.fill_rect(bounds, [0.1, 0.2, 0.3, 1.0], 1.0);
                recorder.fill_rect(bounds, [0.4, 0.5, 0.6, 1.0], 1.0);
            }
            CustomLeafRecordMode::Drift { calls } => {
                let call = calls.fetch_add(1, Ordering::Relaxed);
                let green = if call < 2 { 0.2 } else { 0.8 };
                recorder.fill_rect(bounds, [0.1, green, 0.3, 1.0], 1.0);
            }
        }
    }

    fn children(&self) -> &[NodeKey] {
        if self.expose_children {
            &self.children
        } else {
            &[]
        }
    }

    fn sync_children_mirror(&mut self, children: &[NodeKey]) {
        self.children.clear();
        self.children.extend_from_slice(children);
    }

    fn is_deferred_to_root_viewport_render(&self) -> bool {
        self.deferred
    }

    fn has_active_animator(&self) -> bool {
        self.active_animator
    }

    fn retained_paint_properties(
        &self,
    ) -> crate::view::base_component::RetainedPaintProperties {
        self.retained_properties
    }
}

#[derive(Clone, Copy)]
enum MalformedChunk {
    MetadataNaNBounds,
    MetadataNegativeBounds,
    MetadataProperties,
    MetadataRevision,
    FullOwner,
    FullChunkOwner,
    FullProperties,
    FullRevision,
    FullRange,
    FullBounds,
}

struct MalformedRecordingHost {
    id: u64,
    malformed: MalformedChunk,
    full_records: Arc<AtomicUsize>,
}

impl Layoutable for MalformedRecordingHost {
    fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
    fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
    fn measured_size(&self) -> (f32, f32) {
        (10.0, 10.0)
    }
    fn set_layout_width(&mut self, _width: f32) {}
    fn set_layout_height(&mut self, _height: f32) {}
}

impl EventTarget for MalformedRecordingHost {}

impl Renderable for MalformedRecordingHost {
    fn build(
        &mut self,
        _graph: &mut FrameGraph,
        _arena: &mut NodeArena,
        ctx: UiBuildContext,
    ) -> BuildState {
        ctx.into_state()
    }
}

impl MalformedRecordingHost {
    fn metadata(
        &self,
        owner: NodeKey,
        properties: PropertyTreeState,
        revision: PaintContentRevision,
    ) -> PaintChunkMetadata {
        let fake_owner = NodeKey::null();
        let metadata_properties = matches!(self.malformed, MalformedChunk::MetadataProperties);
        let metadata_revision = matches!(self.malformed, MalformedChunk::MetadataRevision);
        let bounds = match self.malformed {
            MalformedChunk::MetadataNaNBounds => Rect {
                x: f32::NAN,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            MalformedChunk::MetadataNegativeBounds => Rect {
                x: 0.0,
                y: 0.0,
                width: -1.0,
                height: 10.0,
            },
            _ => Rect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
        };
        PaintChunkMetadata {
            id: PaintChunkId {
                owner,
                scope: PaintPropertyScope::SelfPaint,
                phase: PaintNodePhase::BeforeChildren,
                slot: 0,
                role: PaintChunkRole::SelfDecoration,
            },
            owner,
            bounds,
            properties: if metadata_properties {
                PropertyTreeState {
                    transform: Some(TransformNodeId(fake_owner)),
                    ..PropertyTreeState::default()
                }
            } else {
                properties
            },
            content_revision: if metadata_revision {
                PaintContentRevision {
                    self_paint_revision: revision.self_paint_revision.wrapping_add(1),
                    ..revision
                }
            } else {
                revision
            },
            payload_identity: PaintPayloadIdentity::None,
        }
    }
}

impl ElementTrait for MalformedRecordingHost {
    fn stable_id(&self) -> u64 {
        self.id
    }
    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.id,
            parent_id: None,
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
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
        _recording_context: PaintRecordingContext,
    ) -> ShadowPaintRecordingCapability {
        ShadowPaintRecordingCapability::Recordable
    }
    fn record_shadow_paint_metadata(
        &self,
        owner: NodeKey,
        properties: PropertyTreeState,
        revision: PaintContentRevision,
        _arena: &NodeArena,
        _recording_context: PaintRecordingContext,
    ) -> Option<PaintChunkMetadata> {
        Some(self.metadata(owner, properties, revision))
    }
    fn record_shadow_paint_artifact(
        &self,
        owner: NodeKey,
        properties: PropertyTreeState,
        revision: PaintContentRevision,
        _arena: &NodeArena,
        _recording_context: PaintRecordingContext,
    ) -> Option<PaintArtifact> {
        self.full_records.fetch_add(1, Ordering::Relaxed);
        let mut chunk = self.metadata(owner, properties, revision);
        let fake_owner = NodeKey::null();
        match self.malformed {
            MalformedChunk::FullOwner => {
                chunk.id.owner = fake_owner;
                chunk.owner = fake_owner;
            }
            MalformedChunk::FullChunkOwner => chunk.owner = fake_owner,
            MalformedChunk::FullProperties => {
                chunk.properties.transform = Some(TransformNodeId(fake_owner));
            }
            MalformedChunk::FullRevision => {
                chunk.content_revision.self_paint_revision =
                    chunk.content_revision.self_paint_revision.wrapping_add(1);
            }
            _ => {}
        }
        if matches!(self.malformed, MalformedChunk::FullBounds) {
            chunk.bounds.width += 1.0;
        }
        Some(PaintArtifact {
            target: Default::default(),
            chunks: vec![PaintChunk {
                id: chunk.id,
                owner: chunk.owner,
                op_range: if matches!(self.malformed, MalformedChunk::FullRange) {
                    1..0
                } else {
                    0..0
                },
                bounds: chunk.bounds,
                properties: chunk.properties,
                content_revision: chunk.content_revision,
                payload_identity: chunk.payload_identity,
            }],
            ops: Vec::new(),
            clip_nodes: Vec::new(),
            effect_nodes: Vec::new(),
            owner_nodes: Vec::new(),
        })
    }
}

impl Layoutable for RecordingHost {
    fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
    fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
    fn measured_size(&self) -> (f32, f32) {
        (1.0, 1.0)
    }
    fn set_layout_width(&mut self, _width: f32) {}
    fn set_layout_height(&mut self, _height: f32) {}
}

impl EventTarget for RecordingHost {}

impl Renderable for RecordingHost {
    fn build(
        &mut self,
        _graph: &mut FrameGraph,
        _arena: &mut NodeArena,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        self.builds.fetch_add(1, Ordering::Relaxed);
        if let Some(fill_color) = self.fill {
            let pass = DrawRectPass::new(
                RectPassParams {
                    position: [0.0, 0.0],
                    size: [1.0, 1.0],
                    fill_color,
                    opacity: 1.0,
                    ..Default::default()
                },
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            ctx.emit_draw_rect_pass(_graph, pass);
        }
        ctx.into_state()
    }
}

impl ElementTrait for RecordingHost {
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
}


fn custom_leaf_fixture(
    host: CustomLeafPaintHost,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(host));
    let (properties, generations) = sync_identity(&arena, &[root]);
    (arena, root, properties, generations)
}








fn custom_wrapper_fixture(
    host: CustomWrapperPaintHost,
    child: Box<dyn ElementTrait>,
) -> (
    NodeArena,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(host));
    let child = commit_child(&mut arena, root, child);
    let (properties, generations) = sync_identity(&arena, &[root]);
    (arena, root, child, properties, generations)
}






fn malformed_host(
    malformed: MalformedChunk,
) -> (
    NodeArena,
    NodeKey,
    Arc<AtomicUsize>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let full_records = Arc::new(AtomicUsize::new(0));
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(MalformedRecordingHost {
            id: 45,
            malformed,
            full_records: full_records.clone(),
        }),
    );
    let (properties, generations) = sync_identity(&arena, &[root]);
    (arena, root, full_records, properties, generations)
}






fn compiler_test_artifact() -> PaintArtifact {
    let (arena, root, properties, generations) =
        prepared_leaf(47, Color::rgb(20, 30, 40), 1.0, false);
    let PaintRecordOutcome::Artifact(artifact) =
        record_root(&arena, root, &properties, &generations)
    else {
        panic!("safe leaf must record")
    };
    artifact
}

fn rect_phase_op(x: f32, color: [f32; 4]) -> DrawRectOp {
    DrawRectOp {
        params: RectPassParams {
            position: [x, 2.0],
            size: [5.0, 7.0],
            fill_color: color,
            opacity: 1.0,
            ..RectPassParams::default()
        },
        mode: crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
    }
}

fn compiler_rect_phase_artifact(role: PaintChunkRole, rects: Vec<DrawRectOp>) -> PaintArtifact {
    let mut artifact = compiler_test_artifact();
    let mut chunk = artifact.chunks[0].clone();
    chunk.id.role = role;
    chunk.op_range = 0..rects.len();
    chunk.payload_identity = PaintPayloadIdentity::prepared_rects(rects.iter())
        .expect("rect phase fixture must have canonical identity");
    artifact.ops = rects.into_iter().map(PaintOp::DrawRect).collect();
    artifact.chunks = vec![chunk];
    artifact
}

fn refresh_rect_phase_identity(artifact: &mut PaintArtifact) {
    let range = artifact.chunks[0].op_range.clone();
    artifact.chunks[0].payload_identity = PaintPayloadIdentity::prepared_rects(
        artifact.ops[range].iter().filter_map(|op| match op {
            PaintOp::DrawRect(rect) => Some(rect),
            _ => None,
        }),
    )
    .expect("remaining rect parameters must stay canonical");
}




fn compiler_image_test_artifact(with_decoration: bool) -> PaintArtifact {
    let pixels: Arc<[u8]> = Arc::from([
        255_u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
    ]);
    let (arena, roots) = if with_decoration {
        prepared_image_fixture(
            pixels,
            crate::view::ImageFit::Fill,
            crate::view::ImageSampling::Linear,
            1.0,
        )
    } else {
        bare_image_fixture(
            pixels,
            crate::view::ImageFit::Fill,
            crate::view::ImageSampling::Linear,
            1.0,
        )
    };
    let (properties, generations) = sync_identity(&arena, &roots);
    whole_frame_artifact(&arena, &roots, &properties, &generations).0
}

fn compiler_svg_test_artifact(with_decoration: bool) -> PaintArtifact {
    let mut artifact = compiler_image_test_artifact(with_decoration);
    let chunk = artifact
        .chunks
        .first_mut()
        .expect("image fixture must contain one content chunk");
    chunk.id.role = PaintChunkRole::SvgContent;
    let PaintPayloadIdentity::Image(_, decoration) = &chunk.payload_identity else {
        panic!("image fixture must carry composite identity")
    };
    let decoration = Arc::clone(decoration);
    let prepared = artifact
        .ops
        .last_mut()
        .expect("image fixture must end in a prepared payload");
    let PaintOp::PreparedImage(image) = prepared else {
        panic!("image fixture must end in PreparedImage")
    };
    image.upload.id = crate::view::sampled_texture::SampledTextureId::SvgRaster(
        crate::view::sampled_texture::SvgRasterAssetId::for_test(77),
    );
    let svg = PreparedSvgOp {
        params: image.params,
        upload: image.upload.clone(),
    };
    chunk.payload_identity = PaintPayloadIdentity::Svg(
        PreparedSvgIdentity::from_op(&svg).expect("fixture must have typed SVG identity"),
        decoration,
    );
    *prepared = PaintOp::PreparedSvg(svg);
    artifact
}

fn refresh_svg_standard_draw_rect_identity(artifact: &mut PaintArtifact) {
    let prepared = artifact.ops.iter().find_map(|op| match op {
        PaintOp::PreparedSvg(prepared) => Some(prepared),
        _ => None,
    });
    let identity = PreparedSvgIdentity::from_op(prepared.expect("prepared SVG")).unwrap();
    artifact.chunks[0].payload_identity = PaintPayloadIdentity::svg_with_decoration(
        identity,
        artifact.ops.iter().filter_map(|op| match op {
            PaintOp::DrawRect(rect) => Some(rect),
            _ => None,
        }),
    )
    .unwrap();
}










fn compiler_clip_test_artifact() -> PaintArtifact {
    let (arena, roots) = anchor_parent_self_clip_roots(1.0, false);
    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations).0;
    assert_eq!(artifact.clip_nodes.len(), 1);
    assert!(artifact.chunks[0].properties.clip.is_some());
    assert!(artifact.chunks[1].properties.clip.is_none());
    artifact
}

fn unique_synthetic_owner(artifact: &PaintArtifact) -> NodeKey {
    let mut arena = NodeArena::new();
    loop {
        let key = arena.insert(Node::new(Box::new(Element::new_with_id(
            0x8c00 + arena.len() as u64,
            0.0,
            0.0,
            1.0,
            1.0,
        ))));
        if artifact
            .owner_nodes
            .iter()
            .all(|snapshot| snapshot.owner != key)
        {
            return key;
        }
    }
}

fn add_inherited_contents_clip(
    artifact: &mut PaintArtifact,
    logical_scissor: [u32; 4],
) -> ClipNodeId {
    let owner = unique_synthetic_owner(artifact);
    for snapshot in &mut artifact.owner_nodes {
        if snapshot.parent.is_none() {
            snapshot.parent = Some(owner);
        }
    }
    artifact.owner_nodes.push(PaintOwnerSnapshot {
        owner,
        parent: None,
    });
    let id = ClipNodeId {
        owner,
        role: ClipNodeRole::ContentsClip,
    };
    artifact.clip_nodes.push(ClipNodeSnapshot {
        id,
        owner,
        parent: None,
        logical_scissor,
        behavior: ClipBehavior::Intersect,
        generation: 1,
    });
    for chunk in &mut artifact.chunks {
        chunk.properties.clip = Some(id);
    }
    id
}





fn compiler_effect_test_artifact(
    parent_opacity: f32,
    child_opacity: f32,
) -> (PaintArtifact, NodeKey, NodeKey) {
    let mut arena = new_test_arena();
    let parent = commit_element(
        &mut arena,
        Box::new(leaf_element(
            0x6b00,
            Color::rgb(200, 20, 30),
            parent_opacity,
            false,
        )),
    );
    let mut child_element = leaf_element(0x6b01, Color::rgb(20, 200, 30), child_opacity, false);
    child_element.set_position(0.0, 0.0);
    let child = commit_child(&mut arena, parent, Box::new(child_element));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, parent, measure, place);
    let roots = [parent];
    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations).0;
    (artifact, parent, child)
}

fn compiler_sibling_effect_artifact() -> (PaintArtifact, NodeKey, NodeKey, NodeKey) {
    let mut arena = new_test_arena();
    let first = commit_element(
        &mut arena,
        Box::new(leaf_element(0x6b21, Color::rgb(40, 50, 60), 0.5, false)),
    );
    let second = commit_element(
        &mut arena,
        Box::new(leaf_element(0x6b22, Color::rgb(70, 80, 90), 0.25, false)),
    );
    let synthetic_parent = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x6b20, 0.0, 0.0, 1.0, 1.0,
    ))));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, first, measure, place);
    measure_and_place(&mut arena, second, measure, place);
    let roots = [first, second];
    let (properties, generations) = sync_identity(&arena, &roots);
    let mut artifact = whole_frame_artifact(&arena, &roots, &properties, &generations).0;
    for snapshot in &mut artifact.owner_nodes {
        snapshot.parent = Some(synthetic_parent);
    }
    artifact.owner_nodes.push(PaintOwnerSnapshot {
        owner: synthetic_parent,
        parent: None,
    });
    // This compiler-only fixture models two canonical siblings without
    // introducing Element child-clip eligibility concerns.
    assert_eq!(
        compiled_whole_frame_graph(&artifact)
            .test_rect_pass_snapshots()
            .len(),
        2
    );
    (artifact, synthetic_parent, first, second)
}

fn compiler_three_level_effect_artifact() -> (PaintArtifact, NodeKey, NodeKey, NodeKey) {
    let mut arena = new_test_arena();
    let grandparent = commit_element(
        &mut arena,
        Box::new(leaf_element(0x6b30, Color::rgb(10, 20, 30), 0.5, false)),
    );
    let mut parent_element = leaf_element(0x6b31, Color::rgb(40, 50, 60), 0.25, false);
    parent_element.set_position(0.0, 0.0);
    let parent = commit_child(&mut arena, grandparent, Box::new(parent_element));
    let mut child_element = leaf_element(0x6b32, Color::rgb(70, 80, 90), 1.0, false);
    child_element.set_position(0.0, 0.0);
    let child = commit_child(&mut arena, parent, Box::new(child_element));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, grandparent, measure, place);
    let roots = [grandparent];
    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations).0;
    (artifact, grandparent, parent, child)
}

fn assert_compiler_rejects_before_emit(artifact: &PaintArtifact, case: &str) {
    let graph = compiled_whole_frame_graph(artifact);
    assert!(
        graph.test_rect_pass_snapshots().is_empty(),
        "{case} must reject the entire store before the first pass"
    );
}

fn refresh_inline_decoration_payload_identity(artifact: &mut PaintArtifact) {
    let range = artifact.chunks[0].op_range.clone();
    let identity = PaintPayloadIdentity::inline_ifc_decorations(
        artifact.ops[range].iter().filter_map(|op| match op {
            PaintOp::PreparedInlineIfcDecoration(prepared) => Some(prepared),
            _ => None,
        }),
    );
    artifact.chunks[0].payload_identity = identity;
}










fn distinct_chunk(mut chunk: PaintChunk) -> PaintChunk {
    chunk.id.owner = NodeKey::null();
    chunk.owner = NodeKey::null();
    chunk
}







fn whole_frame_artifact(
    arena: &NodeArena,
    roots: &[NodeKey],
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
) -> (PaintArtifact, FrameArtifactEligibility) {
    let FrameArtifactRecordOutcome::Artifact {
        artifact,
        eligibility,
    } = record_frame_artifact(
        arena,
        roots,
        properties,
        generations,
        RendererMode::ForcedForTests,
    )
    .expect("plain Element frame should be fully recordable")
    else {
        panic!("forced artifact recording cannot silently fall back")
    };
    (artifact, eligibility)
}

fn root_group_artifact(
    arena: &NodeArena,
    roots: &[NodeKey],
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
) -> (PaintArtifact, FrameArtifactEligibility) {
    let FrameArtifactRecordOutcome::Artifact {
        artifact,
        eligibility,
    } = record_root_group_opacity_frame_artifact(
        arena,
        roots,
        properties,
        generations,
        RendererMode::ForcedForTests,
    )
    .expect("single root effect should be fully recordable")
    else {
        panic!("forced root group recording cannot silently fall back")
    };
    (artifact, eligibility)
}

fn assert_neutral_opacity(op: &PaintOp) {
    match op {
        PaintOp::DrawRect(op) => assert_eq!(op.params.opacity.to_bits(), 1.0_f32.to_bits()),
        PaintOp::PreparedInlineIfcDecoration(op) => {
            assert_eq!(op.fill.opacity.to_bits(), 1.0_f32.to_bits());
            if let Some(border) = &op.border {
                assert_eq!(border.opacity.to_bits(), 1.0_f32.to_bits());
            }
        }
        PaintOp::PreparedShadow(op) => {
            assert_eq!(op.params.opacity.to_bits(), 1.0_f32.to_bits())
        }
        PaintOp::PreparedScrollbarOverlay(op) => {
            assert!(op.has_baked_opacity(1.0_f32.to_bits()))
        }
        PaintOp::PreparedText(op) => assert!(
            op.params
                .staging_input
                .glyphs
                .iter()
                .all(|glyph| glyph.paint.opacity.to_bits() == 1.0_f32.to_bits())
        ),
        PaintOp::PreparedImage(op) => {
            assert_eq!(op.params.opacity.to_bits(), 1.0_f32.to_bits())
        }
        PaintOp::PreparedSvg(op) => {
            assert_eq!(op.params.opacity.to_bits(), 1.0_f32.to_bits())
        }
    }
}

fn two_outer_shadows() -> Vec<BoxShadow> {
    vec![
        BoxShadow::new()
            .color(Color::rgb(220, 30, 20))
            .offset_x(1.5)
            .offset_y(-2.25),
        BoxShadow::new()
            .color(Color::rgb(20, 40, 220))
            .offset_x(-3.0)
            .offset_y(4.5),
    ]
}




















fn root_effect_raster_inputs() -> RootEffectRasterInputs {
    RootEffectRasterInputs {
        width: 320,
        height: 240,
        format: wgpu::TextureFormat::Bgra8Unorm,
        sample_count: 1,
        scale_factor_bits: 1.0_f32.to_bits(),
    }
}











fn compiled_whole_frame_graph(artifact: &PaintArtifact) -> FrameGraph {
    compiled_whole_frame_graph_with_config(artifact, PaintParityConfig::default())
}

#[derive(Clone, Copy)]
struct PaintParityConfig {
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    scale_factor: f32,
    initial_scissor: Option<[u32; 4]>,
}

impl Default for PaintParityConfig {
    fn default() -> Self {
        Self {
            width: 320,
            height: 240,
            format: wgpu::TextureFormat::Bgra8Unorm,
            scale_factor: 1.0,
            initial_scissor: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ViewportRasterFingerprint {
    logical_width_bits: u32,
    logical_height_bits: u32,
    target_width: u32,
    target_height: u32,
    target_format: wgpu::TextureFormat,
    scale_factor_bits: u32,
}

impl From<PaintParityConfig> for ViewportRasterFingerprint {
    fn from(config: PaintParityConfig) -> Self {
        let scale_factor = config.scale_factor.max(0.0001);
        Self {
            logical_width_bits: (config.width as f32 / scale_factor).to_bits(),
            logical_height_bits: (config.height as f32 / scale_factor).to_bits(),
            target_width: config.width,
            target_height: config.height,
            target_format: config.format,
            scale_factor_bits: scale_factor.to_bits(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PaintParitySnapshot {
    viewport: ViewportRasterFingerprint,
    graph: FrameGraphTestSnapshot,
}

fn strict_paint_snapshot(
    graph: &mut FrameGraph,
    config: PaintParityConfig,
) -> PaintParitySnapshot {
    PaintParitySnapshot {
        viewport: config.into(),
        graph: graph
            .test_compile_snapshot()
            .expect("paint parity graph must have complete strict test coverage"),
    }
}

fn compiled_whole_frame_graph_with_config(
    artifact: &PaintArtifact,
    config: PaintParityConfig,
) -> FrameGraph {
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(
        config.width,
        config.height,
        config.format,
        config.scale_factor,
    );
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target.clone());
    let clear = crate::view::frame_graph::ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new([0.0, 0.0, 0.0, 0.0]),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: target.clone(),
        },
    );
    if let Some(handle) = target.handle() {
        ctx.set_color_target(Some(handle));
    }
    graph.add_graphics_pass(clear);
    ctx.set_current_target(target);
    if let Some(scissor) = config.initial_scissor {
        ctx.replace_scissor_rect(Some(scissor));
    }
    let _ = compile_artifact(artifact, &mut graph, ctx);
    graph
}

fn legacy_roots_graph(arena: NodeArena, roots: &[NodeKey]) -> FrameGraph {
    legacy_roots_graph_with_config(arena, roots, PaintParityConfig::default())
}

fn legacy_roots_graph_with_config(
    mut arena: NodeArena,
    roots: &[NodeKey],
    config: PaintParityConfig,
) -> FrameGraph {
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(
        config.width,
        config.height,
        config.format,
        config.scale_factor,
    );
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target.clone());
    let clear = crate::view::frame_graph::ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new([0.0, 0.0, 0.0, 0.0]),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: target.clone(),
        },
    );
    if let Some(handle) = target.handle() {
        ctx.set_color_target(Some(handle));
    }
    graph.add_graphics_pass(clear);
    ctx.set_current_target(target);
    if let Some(scissor) = config.initial_scissor {
        ctx.replace_scissor_rect(Some(scissor));
    }
    for &root in roots {
        let child_ctx = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next = arena
            .with_element_taken(root, |element, arena| {
                element.build(&mut graph, arena, child_ctx)
            })
            .expect("legacy root should build");
        ctx.set_state(next);
    }
    graph
}

fn assert_whole_frame_structural_parity<F>(
    fixture: F,
    config: PaintParityConfig,
) -> Vec<RectPassTestSnapshot>
where
    F: Fn() -> (NodeArena, Vec<NodeKey>),
{
    let (artifact_arena, artifact_roots) = fixture();
    let (properties, generations) = sync_identity(&artifact_arena, &artifact_roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&artifact_arena, &artifact_roots, &properties, &generations);
    assert!(eligibility.eligible);
    drop(artifact_arena);
    let mut artifact_graph = compiled_whole_frame_graph_with_config(&artifact, config);

    let (legacy_arena, legacy_roots) = fixture();
    let mut legacy_graph = legacy_roots_graph_with_config(legacy_arena, &legacy_roots, config);

    let artifact_snapshot = strict_paint_snapshot(&mut artifact_graph, config);
    let legacy_snapshot = strict_paint_snapshot(&mut legacy_graph, config);
    assert_eq!(artifact_snapshot, legacy_snapshot);
    artifact_graph.test_rect_pass_snapshots()
}

fn prepared_text_tree(nested: bool) -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let mut arena = new_test_arena();
    let mut text = Text::new_with_id(
        180,
        3.4,
        5.6,
        if nested { 120.0 } else { 92.0 },
        if nested { 40.0 } else { 72.0 },
        if nested {
            "nested retained text"
        } else {
            "retained text wraps across lines\nwith alignment"
        },
    );
    text.set_color(Color::rgb(24, 96, 210));
    text.set_font("sans-serif");
    text.set_font_size(19.5);
    text.set_font_weight(650);
    text.set_line_height(1.35);
    text.set_text_align(TextAlign::Center);
    text.set_text_wrap(TextWrap::Wrap);
    text.set_opacity(0.72);

    let (roots, text_key) = if nested {
        let mut parent = Element::new_with_id(179, 10.25, 20.75, 300.0, 180.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().column().into()),
        );
        parent.apply_style(parent_style);
        let root = commit_element(&mut arena, Box::new(parent));
        let text_key = commit_child(&mut arena, root, Box::new(text));
        (vec![root], text_key)
    } else {
        let text_key = commit_element(&mut arena, Box::new(text));
        (vec![text_key], text_key)
    };
    let (measure, place) = constraints();
    for &root in &roots {
        measure_and_place(&mut arena, root, measure, place);
    }
    (arena, roots, text_key)
}

fn prepared_plain_text_area_tree_with(
    content: &str,
    placeholder: &str,
    width: f32,
    origin: [f32; 2],
) -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let mut arena = new_test_arena();
    let mut text_area = TextArea::with_stable_id(0x7e00);
    text_area.set_text(content.to_string());
    text_area.placeholder = placeholder.to_string();
    text_area.font_families = vec!["sans-serif".to_string()];
    text_area.font_size = 17.5;
    text_area.line_height = 1.3;
    text_area.set_layout_offset(0.35, 0.65);
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
        max_height: 240.0,
        viewport_width: 320.0,
        viewport_height: 240.0,
        percent_base_width: Some(320.0),
        percent_base_height: Some(240.0),
    };
    let place = LayoutPlacement {
        parent_x: origin[0],
        parent_y: origin[1],
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: width,
        available_height: 240.0,
        viewport_width: 320.0,
        viewport_height: 240.0,
        percent_base_width: Some(320.0),
        percent_base_height: Some(240.0),
    };
    measure_and_place(&mut arena, root, measure, place);
    let mut keys = vec![root];
    keys.extend(arena.children_of(root));
    for key in keys {
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

fn prepared_plain_text_area_tree(content: &str) -> (NodeArena, Vec<NodeKey>, NodeKey) {
    prepared_plain_text_area_tree_with(content, "", 108.0, [7.25, 11.75])
}

fn prepared_plain_text_area_selection_tree(
    content: &str,
    width: f32,
    anchor: usize,
    focus: usize,
) -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let (arena, roots, root) =
        prepared_plain_text_area_tree_with(content, "", width, [7.25, 11.75]);
    {
        let mut node = arena.get_mut(root).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.selection_anchor_char = Some(anchor);
        text_area.selection_focus_char = Some(focus);
    }
    (arena, roots, root)
}

fn prepared_plain_text_area_preedit_tree(
    content: &str,
    width: f32,
    cursor_char: usize,
    preedit: &str,
    preedit_cursor: Option<(usize, usize)>,
) -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let (mut arena, roots, root) =
        prepared_plain_text_area_tree_with(content, "", width, [7.25, 11.75]);
    arena.with_element_taken(root, |element, _arena| {
        let text_area = element.as_any_mut().downcast_mut::<TextArea>().unwrap();
        text_area.cursor_char = cursor_char;
        text_area.ime_preedit = preedit.to_string();
        text_area.ime_preedit_cursor = preedit_cursor;
        text_area.is_focused = true;
        text_area.caret_visible = true;
        text_area.caret_blink_epoch = None;
        text_area.children_dirty = true;
        text_area.bump_unified_ifc_source_revision();
        text_area.dirty_flags = DirtyFlags::ALL;
    });
    let measure = LayoutConstraints {
        max_width: width,
        max_height: 240.0,
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
        available_height: 240.0,
        viewport_width: 320.0,
        viewport_height: 240.0,
        percent_base_width: Some(320.0),
        percent_base_height: Some(240.0),
    };
    measure_and_place(&mut arena, root, measure, place);
    settle_plain_text_area(&arena, root);
    (arena, roots, root)
}

fn prepared_projection_text_area_tree_with(
    content: &'static str,
    projection_range: std::ops::Range<usize>,
    projected_content: &'static str,
) -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey) {
    let width = 132.0;
    let (mut arena, roots, root) =
        prepared_plain_text_area_tree_with(content, "", width, [7.25, 11.75]);
    arena.with_element_taken(root, |element, _arena| {
        let text_area = element.as_any_mut().downcast_mut::<TextArea>().unwrap();
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(projection_range.clone(), move |_text_area| {
                crate::ui::RsxNode::text(projected_content)
            })
        }));
        text_area.children_dirty = true;
        text_area.bump_unified_ifc_source_revision();
        text_area.dirty_flags = DirtyFlags::ALL;
    });
    let measure = LayoutConstraints {
        max_width: width,
        max_height: 240.0,
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
        available_height: 240.0,
        viewport_width: 320.0,
        viewport_height: 240.0,
        percent_base_width: Some(320.0),
        percent_base_height: Some(240.0),
    };
    measure_and_place(&mut arena, root, measure, place);
    let projection = arena
        .children_of(root)
        .into_iter()
        .find(|&key| {
            arena
                .get(key)
                .unwrap()
                .element
                .as_any()
                .is::<TextAreaProjectionSegment>()
        })
        .expect("fixture must build one projection wrapper");
    let mut projection_descendants = Vec::new();
    let mut stack = arena.children_of(projection);
    while let Some(key) = stack.pop() {
        stack.extend(arena.children_of(key));
        projection_descendants.push(key);
    }
    let projected_text = projection_descendants
        .iter()
        .copied()
        .find(|&key| arena.get(key).unwrap().element.as_any().is::<Text>())
        .expect("fixture projection must contain one Text descendant");
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
    (arena, roots, root, projection, projected_text)
}

fn prepared_projection_text_area_tree() -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey)
{
    prepared_projection_text_area_tree_with("before projected after", 7..16, "projected")
}

#[derive(Clone, Copy)]
struct AtomicProjectionScrollFixture {
    content: &'static str,
    projection_start: usize,
    projection_end: usize,
    projected_content: &'static str,
    font_size: f32,
    line_height: f32,
    width: f32,
    content_height: f32,
    scroll_y: f32,
}

impl AtomicProjectionScrollFixture {
    fn baseline(projected_content: &'static str, scroll_y: f32) -> Self {
        Self {
            content: "before projected after",
            projection_start: 7,
            projection_end: 16,
            projected_content,
            font_size: 14.0,
            line_height: 1.25,
            width: 132.0,
            content_height: 300.0,
            scroll_y,
        }
    }
}

fn prepared_atomic_projection_scroll_shell_with(
    projected_content: &'static str,
) -> (NodeArena, NodeKey, NodeKey, NodeKey) {
    prepared_atomic_projection_scroll_shell_fixture(AtomicProjectionScrollFixture::baseline(
        projected_content,
        20.0,
    ))
}

fn prepared_atomic_projection_scroll_shell_fixture(
    fixture: AtomicProjectionScrollFixture,
) -> (NodeArena, NodeKey, NodeKey, NodeKey) {
    let mut text_component = TextArea::new();
    text_component.content = fixture.content.to_string();
    text_component.font_size = fixture.font_size;
    text_component.line_height = fixture.line_height;
    let projection_start = fixture.projection_start;
    let projection_end = fixture.projection_end;
    let projected_content = fixture.projected_content;
    text_component.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        render.range(projection_start..projection_end, move |_text_area| {
            crate::ui::RsxNode::text(projected_content)
        });
    }));
    let mut arena = new_test_arena();
    let text_area = commit_element(&mut arena, Box::new(text_component));
    arena.with_element_taken(text_area, |element, _| {
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
            max_width: fixture.width,
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
            available_width: fixture.width,
            available_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        },
    );
    let scroll_y = fixture.scroll_y;
    let wrapper = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(
            0xc3a_3001,
            0.0,
            -scroll_y,
            fixture.width,
            fixture.content_height,
        )),
    );
    let root = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(
            0xc3a_3000,
            0.0,
            0.0,
            fixture.width,
            80.0,
        )),
    );
    arena.set_parent(text_area, Some(wrapper));
    arena.set_children(wrapper, vec![text_area]);
    arena.set_parent(wrapper, Some(root));
    arena.set_children(root, vec![wrapper]);
    arena.with_element_taken(text_area, |element, arena| {
        element.place(
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: -scroll_y,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: fixture.width,
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
            width: fixture.width,
            height: fixture.content_height,
        };
        root_element.set_scroll_offset((0.0, scroll_y));
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
    (arena, root, wrapper, text_area)
}

fn prepared_atomic_projection_scroll_shell() -> (NodeArena, NodeKey, NodeKey, NodeKey) {
    prepared_atomic_projection_scroll_shell_with("projected")
}

fn validated_atomic_projection_scroll_scene_at(
    projected_content: &'static str,
    scroll_y: f32,
) -> super::scroll_scene::ValidatedPropertyScrollScene {
    validated_atomic_projection_scroll_scene_fixture(AtomicProjectionScrollFixture::baseline(
        projected_content,
        scroll_y,
    ))
}

fn validated_atomic_projection_scroll_scene_fixture(
    fixture: AtomicProjectionScrollFixture,
) -> super::scroll_scene::ValidatedPropertyScrollScene {
    let (arena, root, _, _) = prepared_atomic_projection_scroll_shell_fixture(fixture);
    let (properties, generations) = sync_identity(&arena, &[root]);
    let budget =
        super::scroll_scene::ScrollSceneSingleTextureBudget::new(8192, 128 * 1024 * 1024)
            .unwrap();
    super::scroll_scene::plan_and_validate_property_scroll_scene(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        crate::time::Instant::now(),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        budget,
    )
    .expect("valid C3a fixture must compiler-seal one property-scroll scene")
}

fn validated_atomic_projection_selection_scroll_scene_at(
    selection_end: usize,
) -> super::scroll_scene::ValidatedPropertyScrollScene {
    validated_atomic_projection_selection_scroll_scene_fixture(
        AtomicProjectionScrollFixture::baseline("projected", 20.0),
        selection_end,
    )
}

fn validated_atomic_projection_selection_scroll_scene_fixture(
    fixture: AtomicProjectionScrollFixture,
    selection_end: usize,
) -> super::scroll_scene::ValidatedPropertyScrollScene {
    let (arena, root, _, text_area) = prepared_atomic_projection_scroll_shell_fixture(fixture);
    {
        let mut node = arena.get_mut(text_area).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.selection_anchor_char = Some(0);
        text_area.selection_focus_char = Some(selection_end);
    }
    let (properties, generations) = sync_identity(&arena, &[root]);
    let budget =
        super::scroll_scene::ScrollSceneSingleTextureBudget::new(8192, 128 * 1024 * 1024)
            .unwrap();
    super::scroll_scene::plan_and_validate_property_scroll_scene(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        crate::time::Instant::now(),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        budget,
    )
    .expect("valid selection grammar must selector-plan-compile one property-scroll scene")
}

fn atomic_projection_content_stamp_for_test(
    projected_content: &'static str,
    stable_id: u64,
) -> Option<RetainedSurfaceRasterStamp> {
    atomic_projection_emission_fixture_for_test(projected_content, stable_id)
        .map(|(_, stamp)| stamp)
}

fn atomic_projection_emission_fixture_for_test(
    projected_content: &'static str,
    stable_id: u64,
) -> Option<(
    std::sync::Arc<super::compiler::ValidatedScrollSceneAtomicProjectionTextAreaPlanParts>,
    RetainedSurfaceRasterStamp,
)> {
    let (arena, root, wrapper, _) =
        prepared_atomic_projection_scroll_shell_with(projected_content);
    let root_node = arena.get(root)?;
    let root_element = root_node.element.as_any().downcast_ref::<Element>()?;
    let admission = root_element
        .exact_retained_scroll_atomic_projection_text_area_subtree_admission(
            root, &arena, 1.0,
        )?;
    drop(root_node);
    let (properties, generations) = sync_identity(&arena, &[root]);
    let scroll = properties
        .scroll_snapshot_for(crate::view::compositor::property_tree::ScrollNodeId(root))?;
    let outer_clip = *properties
        .clip_snapshot_for(Some(ClipNodeId {
            owner: root,
            role: ClipNodeRole::ContentsClip,
        }))?
        .last()?;
    let outer = PaintScrollContentWitness::new(root, wrapper, scroll, outer_clip)?;
    let baked = PaintBakedScrollHostWitness::new(root, wrapper, scroll, outer_clip.id)?;
    let local = super::frame_recorder::record_scroll_atomic_projection_text_area_subtree_local_artifact_for_plan(
        &arena,
        &properties,
        &generations,
        &admission,
        outer,
    ).ok()?;
    let host = super::frame_recorder::record_baked_scroll_atomic_projection_text_area_subtree_host_artifact_for_plan(
        &arena,
        &[root],
        &properties,
        &generations,
        &admission,
        baked,
    ).ok()?;
    let plan_parts =
        super::frame_recorder::validate_recorded_atomic_projection_text_area_plan_parts(
            host, local,
        )?;
    let terminal = plan_parts.content_opaque_order_count()?;
    let span = plan_parts.content_artifact_span_stamp(0, 0..terminal)?;
    let [x, y, width, height] = plan_parts
        .resident()
        .wrapper_chunk
        .bounds_bits
        .map(f32::from_bits);
    let bounds = crate::view::base_component::RetainedSurfaceBounds {
        x,
        y,
        width,
        height,
        corner_radii: [0.0; 4],
    };
    let color_key = crate::view::base_component::scroll_content_layer_stable_key(stable_id);
    let color = crate::view::base_component::texture_desc_for_logical_bounds(
        bounds,
        1.0,
        None,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    );
    let (color, depth) =
        crate::view::base_component::persistent_target_texture_descriptors(color, color_key);
    let stamp =
        super::compiler::validated_scroll_atomic_projection_text_area_content_raster_stamp(
            wrapper,
            stable_id,
            RetainedSurfaceRasterInputs {
                color,
                depth,
                scale_factor_bits: 1.0_f32.to_bits(),
                source_bounds_bits: [x, y, width, height].map(f32::to_bits),
            },
            span,
            0..terminal,
            plan_parts.resident().clone(),
        )?;
    Some((std::sync::Arc::new(plan_parts), stamp))
}

fn atomic_projection_selection_content_stamp_for_test(
    selection_end: usize,
    stable_id: u64,
) -> Option<RetainedSurfaceRasterStamp> {
    atomic_projection_selection_emission_fixture_for_test(selection_end, stable_id)
        .map(|(_, stamp)| stamp)
}

fn atomic_projection_selection_emission_fixture_for_test(
    selection_end: usize,
    stable_id: u64,
) -> Option<(
    std::sync::Arc<
        super::compiler::ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts,
    >,
    RetainedSurfaceRasterStamp,
)> {
    let (arena, root, wrapper, text_area) = prepared_atomic_projection_scroll_shell();
    {
        let mut node = arena.get_mut(text_area)?;
        let text_area = node.element.as_any_mut().downcast_mut::<TextArea>()?;
        text_area.selection_anchor_char = Some(0);
        text_area.selection_focus_char = Some(selection_end);
    }
    let root_node = arena.get(root)?;
    let root_element = root_node.element.as_any().downcast_ref::<Element>()?;
    let admission = root_element
        .exact_retained_scroll_atomic_projection_selection_text_area_subtree_admission(
            root, &arena, 1.0,
        )?;
    drop(root_node);
    let (properties, generations) = sync_identity(&arena, &[root]);
    let scroll = properties
        .scroll_snapshot_for(crate::view::compositor::property_tree::ScrollNodeId(root))?;
    let outer_clip = *properties
        .clip_snapshot_for(Some(ClipNodeId {
            owner: root,
            role: ClipNodeRole::ContentsClip,
        }))?
        .last()?;
    let outer = PaintScrollContentWitness::new(root, wrapper, scroll, outer_clip)?;
    let baked = PaintBakedScrollHostWitness::new(root, wrapper, scroll, outer_clip.id)?;
    let local = super::frame_recorder::record_scroll_atomic_projection_selection_text_area_subtree_local_artifact_for_plan(
        &arena,
        &properties,
        &generations,
        &admission,
        outer,
    ).ok()?;
    let host = super::frame_recorder::record_baked_scroll_atomic_projection_selection_text_area_subtree_host_artifact_for_plan(
        &arena,
        &[root],
        &properties,
        &generations,
        &admission,
        baked,
    ).ok()?;
    let authority =
        super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_authority(
            host, local,
        )?;
    let plan_parts =
        super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_plan_parts(
            authority,
        )?;
    let terminal = plan_parts.content_opaque_order_count()?;
    let span = plan_parts.content_artifact_span_stamp(0, 0..terminal)?;
    let [x, y, width, height] = plan_parts
        .resident()
        .wrapper_chunk
        .bounds_bits
        .map(f32::from_bits);
    let bounds = crate::view::base_component::RetainedSurfaceBounds {
        x,
        y,
        width,
        height,
        corner_radii: [0.0; 4],
    };
    let color_key = crate::view::base_component::scroll_content_layer_stable_key(stable_id);
    let color = crate::view::base_component::texture_desc_for_logical_bounds(
        bounds,
        1.0,
        None,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    );
    let (color, depth) =
        crate::view::base_component::persistent_target_texture_descriptors(color, color_key);
    let stamp = super::compiler::validated_scroll_atomic_projection_selection_text_area_content_raster_stamp(
        wrapper,
        stable_id,
        RetainedSurfaceRasterInputs {
            color,
            depth,
            scale_factor_bits: 1.0_f32.to_bits(),
            source_bounds_bits: [x, y, width, height].map(f32::to_bits),
        },
        span,
        0..terminal,
        plan_parts.resident().clone(),
    )?;
    Some((std::sync::Arc::new(plan_parts), stamp))
}

fn prepared_projection_text_area_preedit_tree(
    cursor_char: usize,
    preedit: &str,
    preedit_cursor: Option<(usize, usize)>,
) -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey) {
    let width = 132.0;
    let (mut arena, roots, root, _, _) = prepared_projection_text_area_tree();
    arena.with_element_taken(root, |element, _arena| {
        let text_area = element.as_any_mut().downcast_mut::<TextArea>().unwrap();
        text_area.cursor_char = cursor_char;
        text_area.ime_preedit = preedit.to_string();
        text_area.ime_preedit_cursor = preedit_cursor;
        text_area.is_focused = true;
        text_area.caret_visible = true;
        text_area.children_dirty = true;
        text_area.bump_unified_ifc_source_revision();
        text_area.dirty_flags = DirtyFlags::ALL;
    });
    let measure = LayoutConstraints {
        max_width: width,
        max_height: 240.0,
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
        available_height: 240.0,
        viewport_width: 320.0,
        viewport_height: 240.0,
        percent_base_width: Some(320.0),
        percent_base_height: Some(240.0),
    };
    measure_and_place(&mut arena, root, measure, place);
    let projection = arena
        .children_of(root)
        .into_iter()
        .find(|&key| {
            arena
                .get(key)
                .unwrap()
                .element
                .as_any()
                .is::<TextAreaProjectionSegment>()
        })
        .unwrap();
    let projection_children = arena.children_of(projection);
    let [projected_text] = projection_children.as_slice() else {
        panic!("projection preedit fixture requires one direct Text")
    };
    let projected_text = *projected_text;
    assert!(
        arena
            .get(projected_text)
            .unwrap()
            .element
            .as_any()
            .is::<Text>()
    );
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
    (arena, roots, root, projection, projected_text)
}

fn settle_plain_text_area(arena: &NodeArena, root: NodeKey) {
    let mut keys = vec![root];
    keys.extend(arena.children_of(root));
    for key in keys {
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);
}

fn place_text_area_with_baked_scroll(
    arena: &mut NodeArena,
    root: NodeKey,
    width: f32,
    height: f32,
    scroll: [f32; 2],
) {
    arena.with_element_taken(root, |element, arena| {
        element.set_layout_height(height);
        {
            let text_area = element.as_any_mut().downcast_mut::<TextArea>().unwrap();
            let max_x = (text_area.layout_state.content_size.width
                - text_area.viewport_size.width)
                .max(0.0);
            let max_y = (text_area.layout_state.content_size.height
                - text_area.viewport_size.height)
                .max(0.0);
            assert!(
                scroll[0] <= max_x,
                "horizontal fixture scroll must be clamped"
            );
            assert!(
                scroll[1] <= max_y,
                "vertical fixture scroll must be clamped"
            );
            text_area.scroll_x = scroll[0];
            text_area.scroll_y = scroll[1];
        }
        element.place(
            LayoutPlacement {
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
            },
            arena,
        );
    });
    settle_plain_text_area(arena, root);
}

fn assert_text_area_fallback_before_full(
    arena: &NodeArena,
    roots: &[NodeKey],
) -> FrameArtifactEligibility {
    let (properties, generations) = sync_identity(arena, roots);
    take_full_artifact_record_count();
    let outcome =
        record_frame_artifact(arena, roots, &properties, &generations, RendererMode::Auto)
            .unwrap();
    let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
        panic!("unsafe TextArea state must fail metadata preflight")
    };
    assert_eq!(take_full_artifact_record_count(), 0);
    eligibility
}

#[derive(Clone, Copy)]
enum InlineOwnedTextDamage {
    None,
    MissingGlyphs,
    MissingFont,
}

fn prepared_inline_owned_text_tree(
    damage: InlineOwnedTextDamage,
) -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let (arena, roots, text_key) = prepared_text_tree(false);
    let mut paint_input = {
        let node = arena.get(text_key).unwrap();
        let text = node.element.as_any().downcast_ref::<Text>().unwrap();
        text.shaped_context_for_test()
            .unwrap()
            .text_pass_paint_input()
    };
    match damage {
        InlineOwnedTextDamage::None => {}
        InlineOwnedTextDamage::MissingGlyphs => {
            paint_input.glyphs.clear();
            paint_input.batches.clear();
        }
        InlineOwnedTextDamage::MissingFont => {
            paint_input
                .glyphs
                .first_mut()
                .expect("inline text fixture must contain a glyph")
                .font_data = None;
        }
    }
    arena
        .get_mut(text_key)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Text>()
        .unwrap()
        .install_inline_ifc_owned_geometry(
            Vec::new(),
            Arc::new(paint_input),
            crate::ui::Rect {
                x: 3.4,
                y: 5.6,
                width: 92.0,
                height: 72.0,
            },
        );
    (arena, roots, text_key)
}

fn prepared_wrapping_inline_span_tree() -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, usize) {
    prepared_wrapping_inline_span_tree_with_opacity(1.0)
}

fn wrapping_inline_span_constraints() -> (LayoutConstraints, LayoutPlacement) {
    let (mut measure, mut place) = constraints();
    measure.max_width = 92.0;
    measure.max_height = 220.0;
    place.available_width = 92.0;
    place.available_height = 220.0;
    (measure, place)
}

fn settle_wrapping_inline_span_frame(
    arena: &NodeArena,
    parent_key: NodeKey,
    span_key: NodeKey,
    text_key: NodeKey,
) {
    for key in [parent_key, span_key, text_key] {
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(parent_key, DirtyFlags::ALL);
}

fn prepared_wrapping_inline_span_tree_with_opacity(
    opacity: f32,
) -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, usize) {
    prepared_wrapping_inline_span_tree_with_opacity_and_shadows(opacity, Vec::new())
}

fn prepared_wrapping_inline_span_tree_with_opacity_and_shadows(
    opacity: f32,
    shadows: Vec<BoxShadow>,
) -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, usize) {
    let mut arena = new_test_arena();
    let mut parent = Element::new_with_id(0x7b00, 0.0, 0.0, 92.0, 0.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(92.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Auto);
    parent.apply_style(parent_style);
    let parent_key = commit_element(&mut arena, Box::new(parent));

    let mut span = Element::new_with_id(0x7b01, 0.0, 0.0, 0.0, 0.0);
    let mut span_style = Style::new();
    span_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    span_style.insert(PropertyId::Width, ParsedValue::Auto);
    span_style.insert(PropertyId::Height, ParsedValue::Auto);
    span_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#bfdbfe")),
    );
    span_style.set_border(Border::uniform(Length::px(2.0), &Color::hex("#2563eb")));
    span_style.set_padding(crate::style::Padding::uniform(Length::px(4.0)));
    span_style.set_box_shadow(shadows);
    span.apply_style(span_style);
    span.set_border_radius(5.0);
    span.set_opacity(opacity);
    let span_key = commit_child(&mut arena, parent_key, Box::new(span));
    let text_key = commit_child(
        &mut arena,
        span_key,
        Box::new(Text::new_with_id(
            0x7b02,
            0.0,
            0.0,
            0.0,
            0.0,
            "alpha beta gamma delta epsilon zeta",
        )),
    );

    let (measure, place) = wrapping_inline_span_constraints();
    measure_and_place(&mut arena, parent_key, measure, place);
    let fragment_count = arena
        .get(span_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<Element>()
        .unwrap()
        .inline_fragment_rects()
        .len();
    assert!(fragment_count >= 2, "M7B fixture must wrap the source span");
    (arena, vec![span_key], span_key, text_key, fragment_count)
}

fn prepared_owning_wrapping_inline_span_tree_with_opacity(
    opacity: f32,
) -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey, usize) {
    let mut arena = new_test_arena();
    let mut root = Element::new_with_id(0x7c00, 0.0, 0.0, 92.0, 0.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    root_style.insert(PropertyId::Width, ParsedValue::Auto);
    root_style.insert(PropertyId::Height, ParsedValue::Auto);
    root_style.set_padding(crate::style::Padding::uniform(Length::px(8.0)));
    root_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#f8fafc")),
    );
    root.apply_style(root_style);
    root.set_opacity(opacity);
    let root_key = commit_element(&mut arena, Box::new(root));

    let mut span = Element::new_with_id(0x7c01, 0.0, 0.0, 0.0, 0.0);
    let mut span_style = Style::new();
    span_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    span_style.insert(PropertyId::Width, ParsedValue::Auto);
    span_style.insert(PropertyId::Height, ParsedValue::Auto);
    span_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#fde68a")),
    );
    span.apply_style(span_style);
    let span_key = commit_child(&mut arena, root_key, Box::new(span));
    let text_key = commit_child(
        &mut arena,
        span_key,
        Box::new(Text::new_with_id(
            0x7c02,
            0.0,
            0.0,
            0.0,
            0.0,
            "alpha beta gamma delta epsilon zeta",
        )),
    );
    let (measure, place) = wrapping_inline_span_constraints();
    measure_and_place(&mut arena, root_key, measure, place);
    settle_wrapping_inline_span_frame(&arena, root_key, span_key, text_key);
    let fragment_count = arena
        .get(span_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<Element>()
        .unwrap()
        .inline_fragment_rects()
        .len();
    assert!(fragment_count >= 2);
    (
        arena,
        vec![root_key],
        root_key,
        span_key,
        text_key,
        fragment_count,
    )
}

fn prepared_owning_inline_text_root() -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey) {
    let mut arena = new_test_arena();
    let mut root = Element::new_with_id(0x7a10, 0.0, 0.0, 120.0, 40.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    style.insert(PropertyId::Width, ParsedValue::Auto);
    style.insert(PropertyId::Height, ParsedValue::Auto);
    root.apply_style(style);
    let root = commit_element(&mut arena, Box::new(root));
    let text = commit_child(
        &mut arena,
        root,
        Box::new(Text::new_with_id(
            0x7a11,
            0.0,
            0.0,
            100.0,
            30.0,
            "inline child",
        )),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    for key in [root, text] {
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    (arena, vec![root], root, text)
}

fn prepared_owning_inline_two_text_root() -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let mut arena = new_test_arena();
    let mut root = Element::new_with_id(0x7a18, 0.0, 0.0, 160.0, 40.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    style.insert(PropertyId::Width, ParsedValue::Auto);
    style.insert(PropertyId::Height, ParsedValue::Auto);
    root.apply_style(style);
    let root = commit_element(&mut arena, Box::new(root));
    let first = commit_child(
        &mut arena,
        root,
        Box::new(Text::new_with_id(
            0x7a19,
            0.0,
            0.0,
            0.0,
            0.0,
            "first payload ",
        )),
    );
    let second = commit_child(
        &mut arena,
        root,
        Box::new(Text::new_with_id(
            0x7a1a,
            0.0,
            0.0,
            0.0,
            0.0,
            "second payload is different",
        )),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    for key in [root, first, second] {
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    (arena, vec![root], root)
}

fn prepared_fixed_owning_inline_text_root() -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey) {
    let mut arena = new_test_arena();
    let mut root = Element::new_with_id(0x7c10, 0.0, 0.0, 120.0, 40.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
    root.apply_style(style);
    let root = commit_element(&mut arena, Box::new(root));
    let text = commit_child(
        &mut arena,
        root,
        Box::new(Text::new_with_id(
            0x7c11,
            0.0,
            0.0,
            100.0,
            30.0,
            "fixed inline child",
        )),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    for key in [root, text] {
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    (arena, vec![root], root, text)
}

fn prepared_percent_owning_inline_text_root() -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey) {
    let mut arena = new_test_arena();
    let mut root = Element::new_with_id(0x7c18, 0.0, 0.0, 160.0, 200.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    style.insert(
        PropertyId::Width,
        ParsedValue::Length(Length::percent(50.0)),
    );
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(200.0)));
    root.apply_style(style);
    let root = commit_element(&mut arena, Box::new(root));
    let text = commit_child(
        &mut arena,
        root,
        Box::new(Text::new_with_id(
            0x7c19,
            0.0,
            0.0,
            0.0,
            0.0,
            "alpha beta gamma delta epsilon zeta eta theta iota kappa",
        )),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    for key in [root, text] {
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    (arena, vec![root], root, text)
}

fn prepared_owning_inline_root_with_atomic()
-> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey, NodeKey) {
    let mut arena = new_test_arena();
    let mut root = Element::new_with_id(0x7c20, 0.0, 0.0, 160.0, 40.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    root_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(160.0)));
    root_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
    root.apply_style(root_style);
    let root = commit_element(&mut arena, Box::new(root));
    let before = commit_child(
        &mut arena,
        root,
        Box::new(Text::new_with_id(0x7c21, 0.0, 0.0, 0.0, 0.0, "before ")),
    );
    let mut atomic = Element::new_with_id(0x7c22, 0.0, 0.0, 24.0, 18.0);
    atomic.set_background_color_value(Color::rgb(34, 197, 94));
    let atomic = commit_child(&mut arena, root, Box::new(atomic));
    let after = commit_child(
        &mut arena,
        root,
        Box::new(Text::new_with_id(0x7c23, 0.0, 0.0, 0.0, 0.0, " after")),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    for key in [root, before, atomic, after] {
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    (arena, vec![root], root, before, atomic, after)
}

fn prepared_owning_inline_root_with_two_atomics() -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let mut arena = new_test_arena();
    let mut root = Element::new_with_id(0x7c28, 0.0, 0.0, 160.0, 40.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    root_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(160.0)));
    root_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
    root.apply_style(root_style);
    let root = commit_element(&mut arena, Box::new(root));
    let before = commit_child(
        &mut arena,
        root,
        Box::new(Text::new_with_id(0x7c29, 0.0, 0.0, 0.0, 0.0, "before ")),
    );
    let mut first = Element::new_with_id(0x7c2a, 0.0, 0.0, 20.0, 16.0);
    first.set_background_color_value(Color::rgb(34, 197, 94));
    let first = commit_child(&mut arena, root, Box::new(first));
    let mut second = Element::new_with_id(0x7c2b, 0.0, 0.0, 18.0, 14.0);
    second.set_background_color_value(Color::rgb(59, 130, 246));
    let second = commit_child(&mut arena, root, Box::new(second));
    let after = commit_child(
        &mut arena,
        root,
        Box::new(Text::new_with_id(0x7c2c, 0.0, 0.0, 0.0, 0.0, " after")),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    for key in [root, before, first, second, after] {
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    (arena, vec![root], root)
}

#[allow(clippy::type_complexity)]
fn prepared_owning_inline_root_with_atomic_subtree() -> (
    NodeArena,
    Vec<NodeKey>,
    NodeKey,
    NodeKey,
    NodeKey,
    NodeKey,
    NodeKey,
) {
    let mut arena = new_test_arena();
    let mut root = Element::new_with_id(0x7c60, 0.0, 0.0, 160.0, 40.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    root_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(160.0)));
    root_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
    root.apply_style(root_style);
    let root = commit_element(&mut arena, Box::new(root));
    let before = commit_child(
        &mut arena,
        root,
        Box::new(Text::new_with_id(0x7c61, 0.0, 0.0, 0.0, 0.0, "before ")),
    );
    let mut atomic = Element::new_with_id(0x7c62, 0.0, 0.0, 24.0, 18.0);
    let mut atomic_style = Style::new();
    atomic_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    atomic_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(24.0)));
    atomic_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
    atomic_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(34, 197, 94)),
    );
    atomic.apply_style(atomic_style);
    let atomic = commit_child(&mut arena, root, Box::new(atomic));
    let mut grandchild = Element::new_with_id(0x7c63, 0.0, 0.0, 8.0, 8.0);
    grandchild.set_background_color_value(Color::rgb(59, 130, 246));
    let grandchild = commit_child(&mut arena, atomic, Box::new(grandchild));
    let after = commit_child(
        &mut arena,
        root,
        Box::new(Text::new_with_id(0x7c64, 0.0, 0.0, 0.0, 0.0, " after")),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    for key in [root, before, atomic, grandchild, after] {
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    (arena, vec![root], root, before, atomic, grandchild, after)
}

fn prepared_owning_inline_root_with_image_atomic()
-> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey, NodeKey) {
    let mut arena = new_test_arena();
    let mut root = Element::new_with_id(0x7c40, 0.0, 0.0, 120.0, 0.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    root_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
    root_style.insert(PropertyId::Height, ParsedValue::Auto);
    root.apply_style(root_style);
    let root = commit_element(&mut arena, Box::new(root));
    let before = commit_child(
        &mut arena,
        root,
        Box::new(Text::new_with_id(0x7c41, 0.0, 0.0, 0.0, 0.0, "image ")),
    );
    let mut image = Image::new_with_id(
        0x7c42,
        ImageSource::Rgba {
            width: 1,
            height: 1,
            pixels: Arc::from([255, 0, 0, 255]),
        },
    );
    let mut image_style = Style::new();
    image_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    image_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
    image_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(16.0)));
    image.apply_style(image_style);
    let image = commit_child(&mut arena, root, Box::new(image));
    let after = commit_child(
        &mut arena,
        root,
        Box::new(Text::new_with_id(0x7c43, 0.0, 0.0, 0.0, 0.0, " tail")),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    for key in [root, before, image, after] {
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    (arena, vec![root], root, before, image, after)
}

#[cfg(not(target_arch = "wasm32"))]
fn prepared_owning_inline_root_with_svg_atomic()
-> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey, NodeKey) {
    const SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='20' height='16'><rect width='20' height='16' fill='#16a34a'/></svg>";
    let mut arena = new_test_arena();
    let mut root = Element::new_with_id(0x7c50, 0.0, 0.0, 120.0, 0.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    root_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
    root_style.insert(PropertyId::Height, ParsedValue::Auto);
    root.apply_style(root_style);
    let root = commit_element(&mut arena, Box::new(root));
    let before = commit_child(
        &mut arena,
        root,
        Box::new(Text::new_with_id(0x7c51, 0.0, 0.0, 0.0, 0.0, "svg ")),
    );
    let mut svg = Svg::new_with_id(0x7c52, SvgSource::Content(SVG.into()));
    let mut svg_style = Style::new();
    svg_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    svg_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
    svg_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(16.0)));
    svg.apply_style(svg_style);
    let svg = commit_child(&mut arena, root, Box::new(svg));
    let after = commit_child(
        &mut arena,
        root,
        Box::new(Text::new_with_id(0x7c53, 0.0, 0.0, 0.0, 0.0, " tail")),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    {
        let mut node = arena.get_mut(svg).unwrap();
        let svg_host = node.element.as_any_mut().downcast_mut::<Svg>().unwrap();
        svg_host
            .prepare_content_paint_for_test(SVG, (20.0, 16.0), 1.0)
            .unwrap();
        svg_host.clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.set_children(svg, Vec::new());
    for key in [root, before, svg, after] {
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    (arena, vec![root], root, before, svg, after)
}

#[allow(clippy::type_complexity)]
fn prepared_mixed_wrapping_inline_root() -> (
    NodeArena,
    Vec<NodeKey>,
    NodeKey,
    NodeKey,
    NodeKey,
    NodeKey,
    NodeKey,
    NodeKey,
    usize,
) {
    let mut arena = new_test_arena();
    let mut root = Element::new_with_id(0x7c30, 0.0, 0.0, 108.0, 0.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    root_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(108.0)));
    root_style.insert(PropertyId::Height, ParsedValue::Auto);
    root.apply_style(root_style);
    let root = commit_element(&mut arena, Box::new(root));

    // Allocate in an order deliberately different from the live DOM.
    // Neither NodeKey order nor install-plan order may become paint order.
    let mut atomic = Element::new_with_id(0x7c31, 0.0, 0.0, 22.0, 17.0);
    atomic.set_background_color_value(Color::rgb(34, 197, 94));
    let atomic = commit_child(&mut arena, root, Box::new(atomic));
    let after = commit_child(
        &mut arena,
        root,
        Box::new(Text::new_with_id(0x7c32, 0.0, 0.0, 0.0, 0.0, " tail")),
    );
    let before = commit_child(
        &mut arena,
        root,
        Box::new(Text::new_with_id(0x7c33, 0.0, 0.0, 0.0, 0.0, "head ")),
    );
    let mut span = Element::new_with_id(0x7c34, 0.0, 0.0, 0.0, 0.0);
    let mut span_style = Style::new();
    span_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    span_style.insert(PropertyId::Width, ParsedValue::Auto);
    span_style.insert(PropertyId::Height, ParsedValue::Auto);
    span_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#fde68a")),
    );
    span.apply_style(span_style);
    let span = commit_child(&mut arena, root, Box::new(span));
    let nested_text = commit_child(
        &mut arena,
        span,
        Box::new(Text::new_with_id(
            0x7c35,
            0.0,
            0.0,
            0.0,
            0.0,
            "alpha beta gamma delta epsilon",
        )),
    );
    arena.set_children(root, vec![before, span, atomic, after]);

    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let fragment_count = arena
        .get(span)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<Element>()
        .unwrap()
        .inline_fragment_rects()
        .len();
    for key in [root, before, span, nested_text, atomic, after] {
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    (
        arena,
        vec![root],
        root,
        before,
        span,
        nested_text,
        atomic,
        after,
        fragment_count,
    )
}

fn prepared_nested_inline_span_tree() -> (NodeArena, Vec<NodeKey>, Vec<NodeKey>) {
    let mut arena = new_test_arena();
    let mut parent = Element::new_with_id(0x7b10, 0.0, 0.0, 108.0, 0.0);
    let mut parent_style = Style::new();
    parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(108.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Auto);
    parent.apply_style(parent_style);
    let parent_key = commit_element(&mut arena, Box::new(parent));

    let mut outer = Element::new_with_id(0x7b11, 0.0, 0.0, 0.0, 0.0);
    let mut outer_style = Style::new();
    outer_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    outer_style.insert(PropertyId::Width, ParsedValue::Auto);
    outer_style.insert(PropertyId::Height, ParsedValue::Auto);
    outer_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#dbeafe")),
    );
    outer_style.set_padding(crate::style::Padding::uniform(Length::px(3.0)));
    outer.apply_style(outer_style);
    let outer_key = commit_child(&mut arena, parent_key, Box::new(outer));
    let before_key = commit_child(
        &mut arena,
        outer_key,
        Box::new(Text::new_with_id(
            0x7b12,
            0.0,
            0.0,
            0.0,
            0.0,
            "outer alpha ",
        )),
    );

    let mut inner = Element::new_with_id(0x7b13, 0.0, 0.0, 0.0, 0.0);
    let mut inner_style = Style::new();
    inner_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    inner_style.insert(PropertyId::Width, ParsedValue::Auto);
    inner_style.insert(PropertyId::Height, ParsedValue::Auto);
    inner_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#fecaca")),
    );
    inner_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#dc2626")));
    inner_style.set_padding(crate::style::Padding::uniform(Length::px(2.0)));
    inner.apply_style(inner_style);
    let inner_key = commit_child(&mut arena, outer_key, Box::new(inner));
    let inner_text_key = commit_child(
        &mut arena,
        inner_key,
        Box::new(Text::new_with_id(
            0x7b14,
            0.0,
            0.0,
            0.0,
            0.0,
            "inner beta gamma",
        )),
    );
    let after_key = commit_child(
        &mut arena,
        outer_key,
        Box::new(Text::new_with_id(
            0x7b15,
            0.0,
            0.0,
            0.0,
            0.0,
            " tail delta epsilon",
        )),
    );
    let (mut measure, mut place) = constraints();
    measure.max_width = 108.0;
    place.available_width = 108.0;
    measure_and_place(&mut arena, parent_key, measure, place);
    assert!(
        arena
            .get(outer_key)
            .unwrap()
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap()
            .inline_fragment_rects()
            .len()
            >= 2
    );
    (
        arena,
        vec![outer_key],
        vec![outer_key, before_key, inner_key, inner_text_key, after_key],
    )
}

fn first_text_color_bits(artifact: &PaintArtifact) -> [u32; 4] {
    artifact
        .ops
        .iter()
        .find_map(|op| match op {
            PaintOp::PreparedText(op) => op
                .params
                .staging_input
                .glyphs
                .first()
                .map(|glyph| glyph.paint.color.map(f32::to_bits)),
            PaintOp::DrawRect(_) => None,
            PaintOp::PreparedInlineIfcDecoration(_)
            | PaintOp::PreparedShadow(_)
            | PaintOp::PreparedScrollbarOverlay(_)
            | PaintOp::PreparedImage(_)
            | PaintOp::PreparedSvg(_) => None,
        })
        .expect("fixture must retain at least one prepared glyph")
}






fn prepared_plain_tree() -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let mut arena = new_test_arena();
    let first = commit_element(
        &mut arena,
        Box::new(leaf_element(100, Color::rgb(230, 20, 30), 1.0, false)),
    );
    let mut child_element = leaf_element(101, Color::rgb(20, 210, 40), 1.0, false);
    child_element.set_position(0.0, 0.0);
    let child = commit_child(&mut arena, first, Box::new(child_element));
    let second = commit_element(
        &mut arena,
        Box::new(leaf_element(102, Color::rgb(30, 40, 220), 1.0, false)),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, first, measure, place);
    measure_and_place(&mut arena, second, measure, place);
    (arena, vec![first, second], child)
}

fn prepared_asymmetric_border_tree() -> (NodeArena, Vec<NodeKey>) {
    let mut element = Element::new_with_id(103, 10.25, 20.75, 80.0, 40.0);
    let top = Color::hex("#ff0000");
    let right = Color::hex("#00ff00");
    let bottom = Color::hex("#0000ff");
    let left = Color::hex("#ffff00");
    let mut style = Style::new();
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#203040")),
    );
    style.set_border(
        Border::uniform(Length::px(1.0), &Color::hex("#ffffff"))
            .top(Some(Length::px(2.0)), Some(&top))
            .right(Some(Length::px(3.0)), Some(&right))
            .bottom(Some(Length::px(4.0)), Some(&bottom))
            .left(Some(Length::px(5.0)), Some(&left)),
    );
    style.set_border_radius(
        BorderRadius::uniform(Length::px(2.0))
            .top_right(Length::px(6.0))
            .bottom_right(Length::px(10.0))
            .bottom_left(Length::px(14.0)),
    );
    element.apply_style(style);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(element));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root])
}

fn prepared_gradient_tree() -> (NodeArena, Vec<NodeKey>) {
    let mut element = Element::new_with_id(104, 10.25, 20.75, 80.0, 40.0);
    apply_gradient_style(&mut element, "#ff0000", "#0000ff", "#ffffff", "#000000");
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(element));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root])
}

fn prepared_zero_opacity_tree() -> (NodeArena, Vec<NodeKey>) {
    let mut arena = new_test_arena();
    let mut empty_element = leaf_element(110, Color::rgb(255, 0, 0), 1.0, false);
    let mut empty_style = Style::new();
    empty_style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.0)));
    empty_element.apply_style(empty_style);
    let empty = commit_element(&mut arena, Box::new(empty_element));
    let visible = commit_element(
        &mut arena,
        Box::new(leaf_element(111, Color::rgb(0, 0, 255), 1.0, false)),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, empty, measure, place);
    measure_and_place(&mut arena, visible, measure, place);
    (arena, vec![empty, visible])
}





























#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn window_like_native_showcase_fixture() -> (NodeArena, Vec<NodeKey>) {
    const SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='96' height='64'><rect width='96' height='64' rx='12' fill='#22c55e'/></svg>";

    let positioned = |left: f32, top: f32, width: f32, height: f32| {
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(height)));
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(left))
                    .top(Length::px(top)),
            ),
        );
        style
    };

    let mut arena = new_test_arena();
    let mut window = Element::new_with_id(0x7f00, 0.0, 0.0, 480.0, 360.0);
    let mut window_style = positioned(12.0, 16.0, 480.0, 360.0);
    window_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(248, 250, 252)),
    );
    window_style.set_border(Border::uniform(Length::px(2.0), &Color::rgb(100, 116, 139)));
    window_style.set_border_radius(BorderRadius::uniform(Length::px(16.0)));
    window_style.set_box_shadow(vec![
        BoxShadow::new()
            .color(Color::rgba(15, 23, 42, 96))
            .offset_y(12.0)
            .blur(24.0)
            .spread(2.0),
    ]);
    window.apply_style(window_style);
    let root = commit_element(&mut arena, Box::new(window));

    let mut scroll = Element::new_with_id(0x7f01, 0.0, 0.0, 416.0, 260.0);
    let mut scroll_style = positioned(32.0, 68.0, 416.0, 260.0);
    scroll_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Both),
    );
    scroll_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(241, 245, 249)),
    );
    scroll_style.set_border_radius(BorderRadius::uniform(Length::px(18.0)));
    scroll.apply_style(scroll_style);
    let scroll = commit_child(&mut arena, root, Box::new(scroll));

    for (id, left, top, width, height) in [
        (0x7f10, -6.0, 44.0, 12.0, 272.0),
        (0x7f11, 474.0, 44.0, 12.0, 272.0),
        (0x7f12, 44.0, -6.0, 392.0, 12.0),
        (0x7f13, 436.0, 320.0, 52.0, 52.0),
    ] {
        let mut handle = Element::new_with_id(id, 0.0, 0.0, width, height);
        let mut handle_style = Style::new();
        handle_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        handle_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
        handle_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(height)));
        handle_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(left))
                    .top(Length::px(top))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        handle.apply_style(handle_style);
        commit_child(&mut arena, root, Box::new(handle));
    }

    let mut content = Element::new_with_id(0x7f02, 0.0, 0.0, 760.0, 520.0);
    let mut content_style = positioned(0.0, 0.0, 760.0, 520.0);
    content_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(226, 232, 240)),
    );
    content.apply_style(content_style);
    let content = commit_child(&mut arena, scroll, Box::new(content));

    let mut text = Text::new_with_id(
        0x7f03,
        28.0,
        24.0,
        300.0,
        72.0,
        "RetainedAuto native showcase\nText + Image + Svg + TextArea",
    );
    text.set_color(Color::rgb(15, 23, 42));
    text.set_font("sans-serif");
    text.set_font_size(20.0);
    text.set_text_wrap(TextWrap::Wrap);
    commit_child(&mut arena, content, Box::new(text));

    let image_pixels: Arc<[u8]> = Arc::from([
        239, 68, 68, 255, 59, 130, 246, 255, 34, 197, 94, 255, 250, 204, 21, 255,
    ]);
    let image_source = ImageSource::Rgba {
        width: 2,
        height: 2,
        pixels: image_pixels.clone(),
    };
    let image_handle = crate::view::image_resource::acquire_image_resource(&image_source);
    crate::view::image_resource::replace_ready_image_for_test(
        image_handle.asset_id(),
        2,
        2,
        image_pixels,
    );
    let mut image = Image::new_with_id(0x7f04, image_source);
    image.apply_style(positioned(28.0, 124.0, 128.0, 96.0));
    commit_child(&mut arena, content, Box::new(image));

    let mut svg = Svg::new_with_id(0x7f05, SvgSource::Content(SVG.into()));
    svg.apply_style(positioned(188.0, 124.0, 144.0, 96.0));
    let svg = commit_child(&mut arena, content, Box::new(svg));

    let mut text_area = TextArea::with_stable_id(0x7f06);
    text_area.set_text("Editable native content inside a two-axis scroll host.".to_string());
    text_area.font_families = vec!["sans-serif".to_string()];
    text_area.font_size = 17.0;
    text_area.line_height = 1.35;
    text_area.set_layout_offset(28.0, 256.0);
    let text_area = commit_child(&mut arena, content, Box::new(text_area));
    arena.with_element_taken(text_area, |element, _| {
        element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea host")
            .set_self_node_key(text_area);
    });

    let measure = LayoutConstraints {
        max_width: 800.0,
        max_height: 600.0,
        viewport_width: 800.0,
        viewport_height: 600.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
    };
    let place = LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 800.0,
        available_height: 600.0,
        viewport_width: 800.0,
        viewport_height: 600.0,
        percent_base_width: Some(800.0),
        percent_base_height: Some(600.0),
    };
    measure_and_place(&mut arena, root, measure, place);
    let mut plain = Element::new_with_id(0x7f20, 0.0, 0.0, 180.0, 96.0);
    let mut plain_style = positioned(548.0, 24.0, 180.0, 96.0);
    plain_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(219, 234, 254)),
    );
    plain.apply_style(plain_style);
    let plain_root = commit_element(&mut arena, Box::new(plain));
    measure_and_place(&mut arena, plain_root, measure, place);
    {
        let mut node = arena.get_mut(svg).expect("Svg node");
        node.element
            .as_any_mut()
            .downcast_mut::<Svg>()
            .expect("Svg host")
            .prepare_content_paint_for_test(SVG, (144.0, 96.0), 1.0)
            .expect("prepare SVG paint");
    }
    arena.prepare_registered_paint_resources(
        crate::view::base_component::PaintResourcePreparationContext {
            frame_number: 1,
            device_scale: 1.0,
            now: crate::time::Instant::now(),
        },
    );
    {
        let mut scroll_host =
            crate::view::test_support::get_element_mut::<Element>(&arena, scroll);
        scroll_host.layout_state.content_size = Size {
            width: 760.0,
            height: 520.0,
        };
        scroll_host.set_scroll_offset((18.0, 22.0));
    }
    let mut pending = vec![root, plain_root];
    while let Some(owner) = pending.pop() {
        pending.extend(arena.children_of(owner));
        arena
            .get_mut(owner)
            .expect("native showcase owner")
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    arena.clear_arena_dirty_subtree(plain_root, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);
    arena.refresh_subtree_dirty_cache(plain_root);

    (arena, vec![root, plain_root])
}






























































































fn hidden_element_subtree(root_id: u64, child_id: u64) -> (NodeArena, NodeKey, NodeKey) {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(root_id, 0.0, 0.0, 0.0, 10.0)),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    assert!(
        !arena
            .get(root)
            .unwrap()
            .element
            .box_model_snapshot()
            .should_render
    );
    let child = commit_child(
        &mut arena,
        root,
        Box::new(leaf_element(child_id, Color::rgb(220, 40, 30), 1.0, false)),
    );
    measure_and_place(&mut arena, child, measure, place);
    assert!(
        arena
            .get(child)
            .unwrap()
            .element
            .box_model_snapshot()
            .should_render
    );
    (arena, root, child)
}

mod artifact_identity_tests;
mod prepared_image_tests;
mod custom_leaf_tests;
mod custom_wrapper_tests;
mod metadata_preflight_tests;
mod compiler_rect_grammar_tests;
mod compiler_svg_grammar_tests;
mod contents_clip_tests;
mod effect_store_tests;
mod chunk_range_tests;
mod outer_shadow_tests;
mod child_mask_and_self_decoration_tests;
mod root_effect_tests;
mod text_artifact_tests;
mod structural_parity_tests;
mod anchor_parent_clip_tests;
mod whole_frame_tests;
mod inline_span_tests;
mod owning_inline_root_tests;
mod owning_inline_root_atomic_tests;
mod plain_text_area_tests;
mod plain_text_area_preedit_tests;
mod atomic_projection_emission_tests;
mod atomic_projection_record_tests;
mod atomic_projection_raster_stamp_tests;
mod atomic_projection_property_scroll_tests;
mod text_area_projection_preedit_tests;
mod text_area_projection_selection_tests;
mod text_area_state_tests;
mod culling_tests;
