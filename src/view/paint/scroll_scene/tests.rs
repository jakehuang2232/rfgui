use super::*;

use crate::style::{
    Border, Color, Gradient, Layout, Length, ParsedValue, PropertyId, ScrollDirection,
    SideOrCorner, Style, Transition, TransitionProperty, Transitions,
};
use crate::view::base_component::{
    DirtyFlags, DirtyPassMask, Element, ElementTrait, EventTarget, Image, LayoutConstraints,
    LayoutPlacement, PaintResourcePreparationContext, ScrollbarPaintStateWitness, Size, Svg,
    Text, TextArea,
};
use crate::view::frame_graph::{FramePassTestPayload, RetainedTextureRole};
use crate::view::node_arena::Node;
use crate::view::paint::{PaintChunkRole, PaintNodePhase, PaintPropertyScope};
use crate::view::test_support::measure_and_place;

pub(crate) fn retained_auto_scroll_content_effect_fixture(
    outer_transform: bool,
    neutral_wrapper: bool,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    super::super::frame_plan::tests::scroll_content_effect_interleave_fixture(
        outer_transform,
        neutral_wrapper,
    )
}

fn window_layout_inputs() -> (LayoutConstraints, LayoutPlacement) {
    (
        LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
        },
        LayoutPlacement {
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
        },
    )
}

fn sampled_window_scroll_fixture(
    direction: ScrollDirection,
    sampled_width: f32,
) -> (NodeArena, Vec<NodeKey>, NodeKey) {
    let (mut arena, roots) = crate::view::paint::tests::window_like_native_showcase_fixture();
    let root = roots[0];
    let scroll = arena.children_of(root)[0];
    {
        let mut host = crate::view::test_support::get_element_mut::<Element>(&arena, scroll);
        let mut transition = Style::new();
        transition.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::single(Transition::new(
                TransitionProperty::Width,
                200,
            ))),
        );
        host.apply_style(transition);
        host.set_layout_transition_width(sampled_width);
    }
    let (constraints, placement) = window_layout_inputs();
    measure_and_place(&mut arena, root, constraints, placement);
    {
        let mut host = crate::view::test_support::get_element_mut::<Element>(&arena, scroll);
        host.set_scroll_direction_for_retained_test(direction);
        host.set_sampled_scrollbar_alpha_for_test(0.75);
        host.layout_state.content_size = Size {
            width: 760.0,
            height: 520.0,
        };
        host.set_scroll_offset((18.0, 22.0));
    }
    let mut pending = roots.clone();
    while let Some(owner) = pending.pop() {
        pending.extend(arena.children_of(owner));
        arena
            .get_mut(owner)
            .expect("sampled native owner")
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    for root in &roots {
        arena.clear_arena_dirty_subtree(*root, DirtyFlags::ALL);
        arena.refresh_subtree_dirty_cache(*root);
    }
    assert!(
        arena
            .get(scroll)
            .expect("sampled native scroll host")
            .element
            .retained_sampled_layout_transition_snapshot()
            .is_some()
    );
    (arena, roots, scroll)
}





fn compile_nested_scroll_fixture_parts(
    arena: &NodeArena,
    outer: NodeKey,
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
) -> ValidatedNestedScrollScene {
    let plan = super::super::frame_plan::plan_nested_scroll_scene_scaffold_with_context(
        arena,
        &[outer],
        properties,
        generations,
        1.0,
        super::super::frame_plan::TransformSurfacePlanContext::default(),
    )
    .unwrap();
    compile_nested_scroll_transaction(
        plan,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        ScrollSceneSingleTextureBudget::new(u32::MAX, u64::MAX).unwrap(),
    )
    .unwrap()
}

fn compiled_nested_scroll_fixture() -> ValidatedNestedScrollScene {
    let (arena, outer, _inner, _leaf, properties, generations) =
        super::super::frame_plan::tests::nested_scroll_plan_fixture();
    compile_nested_scroll_fixture_parts(&arena, outer, &properties, &generations)
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum NestedMediaLeafKind {
    Image,
    Svg,
}

fn layout_nested_media_leaf(arena: &mut NodeArena, leaf: NodeKey) {
    arena.with_element_taken(leaf, |element, arena| {
        element.sync_arena(arena);
        element.measure(
            LayoutConstraints {
                max_width: 100.0,
                max_height: 600.0,
                viewport_width: 640.0,
                viewport_height: 480.0,
                percent_base_width: Some(100.0),
                percent_base_height: Some(600.0),
            },
            arena,
        );
        element.place(
            LayoutPlacement {
                parent_x: 10.0,
                parent_y: 20.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 100.0,
                available_height: 600.0,
                viewport_width: 640.0,
                viewport_height: 480.0,
                percent_base_width: Some(100.0),
                percent_base_height: Some(600.0),
            },
            arena,
        );
        element.clear_local_dirty_flags(crate::view::base_component::DirtyFlags::ALL);
    });
    arena.clear_arena_dirty_subtree(leaf, crate::view::base_component::DirtyFlags::ALL);
}

fn prepare_nested_media_leaf(arena: &mut NodeArena, leaf: NodeKey, frame_number: u64) {
    arena.with_element_taken(leaf, |element, _arena| {
        element.prepare_paint_resources(PaintResourcePreparationContext {
            frame_number,
            device_scale: 1.0,
            now: crate::time::Instant::now(),
        });
    });
}

fn sync_nested_media_scene(
    arena: &mut NodeArena,
    outer: NodeKey,
    leaf: NodeKey,
    frame_number: u64,
) -> ValidatedNestedScrollScene {
    layout_nested_media_leaf(arena, leaf);
    prepare_nested_media_leaf(arena, leaf, frame_number);
    arena.refresh_subtree_dirty_cache(outer);
    let mut properties = PropertyTrees::default();
    properties.sync(arena, &[outer]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(arena, &[outer], &properties);
    compile_nested_scroll_fixture_parts(arena, outer, &properties, &generations)
}

fn nested_media_payload_identity(
    scene: &ValidatedNestedScrollScene,
) -> (
    crate::view::sampled_texture::SampledTextureId,
    u64,
    usize,
    super::super::PaintPayloadIdentity,
) {
    let scaffold = scene.plan.nested_scroll_planning_scaffold().unwrap();
    let super::super::frame_plan::NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
        &scaffold.schedule.steps[2]
    else {
        panic!("nested media fixture must retain one content receiver")
    };
    let artifact = receiver.artifact.artifact();
    let (id, generation, pixel_ptr) = match artifact.ops.last().unwrap() {
        super::super::PaintOp::PreparedImage(op) => (
            op.upload.id,
            op.upload.generation,
            op.upload.pixels.as_ptr() as usize,
        ),
        super::super::PaintOp::PreparedSvg(op) => (
            op.upload.id,
            op.upload.generation,
            op.upload.pixels.as_ptr() as usize,
        ),
        other => panic!("expected frozen media payload, got {other:?}"),
    };
    (
        id,
        generation,
        pixel_ptr,
        artifact.chunks[0].payload_identity.clone(),
    )
}

fn execute_nested_media_frame(
    viewport: &mut Viewport,
    geometry: PreparedNestedScrollReceiverGeometry,
    expected: RetainedSurfaceCompileAction,
) {
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let mut prepared = prepare_nested_scroll_scene_from_pool(
        viewport,
        geometry,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        owner,
    )
    .unwrap();
    prepared.refresh_action_from_committed_test_pool();
    assert_eq!(prepared.action_for_test(), expected);
    let outcome = emit_prepared_nested_scroll_scene(prepared);
    match expected {
        RetainedSurfaceCompileAction::Reraster => {
            assert_eq!(
                (outcome.trace.reraster_count, outcome.trace.reuse_count),
                (1, 0)
            );
        }
        RetainedSurfaceCompileAction::Reuse => {
            assert_eq!(
                (outcome.trace.reraster_count, outcome.trace.reuse_count),
                (0, 1)
            );
        }
    }
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
}

fn nested_scroll_media_fixture(
    kind: NestedMediaLeafKind,
) -> (
    NodeArena,
    NodeKey,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (mut arena, outer, inner, leaf, _properties, _generations) =
        super::super::frame_plan::tests::nested_scroll_plan_fixture();
    let stable_id = 0x1251_02;
    let mut media: Box<dyn ElementTrait> = match kind {
        NestedMediaLeafKind::Image => {
            let pixels: std::sync::Arc<[u8]> = std::sync::Arc::from([
                255_u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
            ]);
            Box::new(Image::new_with_id(
                stable_id,
                crate::view::ImageSource::Rgba {
                    width: 2,
                    height: 2,
                    pixels,
                },
            ))
        }
        NestedMediaLeafKind::Svg => {
            static NEXT_NESTED_MEDIA_SVG_FIXTURE: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(1);
            let fixture_id = NEXT_NESTED_MEDIA_SVG_FIXTURE
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let source = crate::view::SvgSource::Content(format!(
                r##"<svg width="100" height="600" xmlns="http://www.w3.org/2000/svg"><rect width="100" height="600" fill="#3366cc"/><desc>nested-r1-svg-slice-a-{fixture_id}</desc></svg>"##
            ));
            let document_key = crate::view::svg_resource::prime_svg_document_ready_for_test(
                &source, 100.0, 600.0,
            );
            let (width, height) = crate::view::svg_resource::quantize_svg_raster_size(100, 600);
            let request = crate::view::svg_resource::SvgRasterRequest::new(
                width,
                height,
                crate::view::svg_resource::SvgRasterMode::Fill,
            );
            let pixels: std::sync::Arc<[u8]> =
                std::sync::Arc::from(vec![0x80_u8; (width * height * 4) as usize]);
            crate::view::svg_resource::prime_svg_raster_ready_for_test(
                document_key,
                request,
                pixels,
            );
            let mut svg = Svg::new_with_id(stable_id, source);
            svg.set_fit(crate::view::ImageFit::Fill);
            Box::new(svg)
        }
    };
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(100.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(600.0)));
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(18, 36, 54)),
    );
    if let Some(image) = media.as_any_mut().downcast_mut::<Image>() {
        image.set_fit(crate::view::ImageFit::Fill);
        image.apply_style(style);
    } else if let Some(svg) = media.as_any_mut().downcast_mut::<Svg>() {
        svg.apply_style(style);
    }
    {
        let mut node = arena.get_mut(leaf).unwrap();
        *node.element = media;
    }
    arena.refresh_stable_id_index();
    layout_nested_media_leaf(&mut arena, leaf);
    prepare_nested_media_leaf(&mut arena, leaf, 1);
    if matches!(kind, NestedMediaLeafKind::Svg) {
        arena.with_element_taken(leaf, |element, arena| element.sync_arena(arena));
        // The first prepare binds the exact raster request. Re-run the
        // leaf layout shell after the resource sync just as a production
        // frame would before freezing the ready payload.
        layout_nested_media_leaf(&mut arena, leaf);
        prepare_nested_media_leaf(&mut arena, leaf, 2);
    }
    arena.refresh_subtree_dirty_cache(outer);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[outer]);
    assert!(
        properties.validation_errors.is_empty(),
        "{kind:?} property sync failed: {:?}",
        properties.validation_errors
    );
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[outer], &properties);
    (arena, outer, inner, leaf, properties, generations)
}

fn nested_scroll_text_fixture() -> (
    NodeArena,
    NodeKey,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (mut arena, outer, inner, leaf, _properties, _generations) =
        super::super::frame_plan::tests::nested_scroll_plan_fixture();
    let mut text = Text::new_with_id(
        0x1251_03,
        0.0,
        0.0,
        100.0,
        600.0,
        "standalone nested retained text at a fractional origin",
    );
    text.set_font("sans-serif");
    text.set_font_size(18.5);
    text.set_color(Color::rgb(31, 91, 173));
    text.set_opacity(1.0);
    {
        let mut node = arena.get_mut(leaf).unwrap();
        *node.element = Box::new(text);
    }
    arena.refresh_stable_id_index();
    layout_nested_media_leaf(&mut arena, leaf);
    arena.refresh_subtree_dirty_cache(outer);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[outer]);
    assert!(
        properties.validation_errors.is_empty(),
        "Text property sync failed: {:?}",
        properties.validation_errors
    );
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[outer], &properties);
    (arena, outer, inner, leaf, properties, generations)
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum NestedTextFallbackKind {
    MissingPrepared,
    InlineIfcOwned,
}

pub(crate) fn nested_scroll_unready_text_fixture_for_test(
    kind: NestedTextFallbackKind,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let (arena, outer, _inner, leaf, _properties, _generations) = nested_scroll_text_fixture();
    match kind {
        NestedTextFallbackKind::MissingPrepared => {
            arena
                .get_mut(leaf)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<Text>()
                .unwrap()
                .clear_prepared_standalone_text_for_test();
        }
        NestedTextFallbackKind::InlineIfcOwned => {
            let (paint_input, bounds) = {
                let node = arena.get(leaf).unwrap();
                let text = node.element.as_any().downcast_ref::<Text>().unwrap();
                let bounds = node.element.box_model_snapshot();
                (
                    text.shaped_context_for_test()
                        .unwrap()
                        .text_pass_paint_input(),
                    bounds,
                )
            };
            arena
                .get_mut(leaf)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<Text>()
                .unwrap()
                .install_inline_ifc_owned_geometry(
                    Vec::new(),
                    std::sync::Arc::new(paint_input),
                    crate::ui::Rect {
                        x: bounds.x,
                        y: bounds.y,
                        width: bounds.width,
                        height: bounds.height,
                    },
                );
        }
    }
    arena.refresh_subtree_dirty_cache(outer);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[outer]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[outer], &properties);
    (arena, outer, properties, generations)
}

pub(crate) fn nested_scroll_unready_media_fixture_for_test(
    kind: NestedMediaLeafKind,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let (mut arena, outer, _inner, leaf, ready_properties, ready_generations) =
        nested_scroll_media_fixture(kind);
    match kind {
        NestedMediaLeafKind::Image => {
            let scene = compile_nested_scroll_fixture_parts(
                &arena,
                outer,
                &ready_properties,
                &ready_generations,
            );
            let scaffold = scene.plan.nested_scroll_planning_scaffold().unwrap();
            let super::super::frame_plan::NestedScrollSceneScheduledStep::ContentReceiver(
                receiver,
            ) = &scaffold.schedule.steps[2]
            else {
                unreachable!()
            };
            let asset_id = receiver
                .artifact
                .artifact()
                .ops
                .iter()
                .find_map(|op| match op {
                    super::super::PaintOp::PreparedImage(op) => match op.upload.id {
                        crate::view::sampled_texture::SampledTextureId::Image(id) => Some(id),
                        crate::view::sampled_texture::SampledTextureId::SvgRaster(_) => None,
                    },
                    _ => None,
                })
                .expect("ready Image fixture owns one frozen upload");
            crate::view::image_resource::set_image_loading_for_test(asset_id);
            arena.with_element_taken(leaf, |element, arena| element.sync_arena(arena));
        }
        NestedMediaLeafKind::Svg => {
            static NEXT_NONEXACT_NESTED_SVG: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(1);
            let fixture_id =
                NEXT_NONEXACT_NESTED_SVG.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let mut node = arena.get_mut(leaf).unwrap();
            let svg = node.element.as_any_mut().downcast_mut::<Svg>().unwrap();
            svg.set_source(crate::view::SvgSource::Content(format!(
                r##"<svg width="100" height="600" xmlns="http://www.w3.org/2000/svg"><desc>nested-r1-nonexact-{fixture_id}</desc></svg>"##
            )));
        }
    }
    // Re-run the production layout shell after the resource transition so
    // PropertyTrees and paint generations are synchronized from a clean,
    // reachable S0 -> S1 topology rather than retained from the ready
    // fixture.
    layout_nested_media_leaf(&mut arena, leaf);
    arena.refresh_subtree_dirty_cache(outer);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[outer]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[outer], &properties);
    (arena, outer, properties, generations)
}

fn prepared_nested_scroll_geometry_fixture() -> PreparedNestedScrollReceiverGeometry {
    prepare_nested_scroll_receiver_geometry(
        compiled_nested_scroll_fixture(),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .expect("exact nested fixture has canonical executable geometry")
}

fn set_nested_scroll_position(element: &mut Element, x: f32, y: f32) {
    element.layout_state.layout_position.x = x;
    element.layout_state.layout_position.y = y;
    element.layout_state.layout_inner_position.x = x;
    element.layout_state.layout_inner_position.y = y;
    element.layout_state.layout_flow_position.x = x;
    element.layout_state.layout_flow_position.y = y;
    element.layout_state.layout_flow_inner_position.x = x;
    element.layout_state.layout_flow_inner_position.y = y;
    element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
}

fn move_nested_scroll_fixture(
    arena: &NodeArena,
    outer: NodeKey,
    inner: NodeKey,
    leaf: NodeKey,
) {
    let host_origin = [35.0, 51.0];
    let outer_offset_y = 37.0;
    let inner_offset_y = 53.0;
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(arena, outer);
        set_nested_scroll_position(&mut element, host_origin[0], host_origin[1]);
        element.set_scroll_offset((0.0, outer_offset_y));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(arena, inner);
        set_nested_scroll_position(
            &mut element,
            host_origin[0],
            host_origin[1] - outer_offset_y,
        );
        element.set_scroll_offset((0.0, inner_offset_y));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    set_nested_scroll_position(
        &mut crate::view::test_support::get_element_mut::<Element>(arena, leaf),
        host_origin[0],
        host_origin[1] - outer_offset_y - inner_offset_y,
    );
}

fn nested_scroll_test_gradient(start: &str, end: &str) -> Gradient {
    Gradient::linear(SideOrCorner::Right)
        .stop(Color::hex(start), Some(Length::percent(0.0)))
        .stop(Color::hex(end), Some(Length::percent(100.0)))
        .build()
}




















fn fixture_at_offset(
    offset: [f32; 2],
) -> (
    NodeArena,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    fixture_with_geometry(offset, [100.0, 80.0], [300.0, 300.0])
}

fn focused_atomic_projection_scroll_fixture(
    caret_visible: bool,
) -> (
    NodeArena,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let width = 132.0;
    let scroll_y = 0.0;
    let mut text_component = TextArea::new();
    text_component.content = "before projected after".to_string();
    text_component.font_size = 14.0;
    text_component.line_height = 1.25;
    text_component.is_focused = true;
    text_component.caret_visible = caret_visible;
    text_component.cursor_char = 0;
    text_component.on_render_handler = Some(crate::ui::on_text_area_render(|render| {
        render.range(7..16, |_text_area| crate::ui::RsxNode::text("projected"));
    }));

    let mut arena = NodeArena::new();
    let text_area = arena.insert(Node::new(Box::new(text_component)));
    arena.with_element_taken(text_area, |element, _| {
        element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap()
            .set_self_node_key(text_area);
    });
    crate::view::test_support::measure_and_place(
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

    let wrapper = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xc3a_4301, 0.0, -scroll_y, width, 300.0,
    ))));
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xc3a_4300, 0.0, 0.0, width, 80.0,
    ))));
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
    arena
        .get_mut(wrapper)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .apply_style(wrapper_style);

    let mut root_style = Style::new();
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut root_node = arena.get_mut(root).unwrap();
        let root_element = root_node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap();
        root_element.apply_style(root_style);
        root_element.layout_state.content_size = Size {
            width,
            height: 300.0,
        };
        root_element.set_scroll_offset((0.0, scroll_y));
        root_element.clear_local_dirty_flags(DirtyFlags::ALL);
    }
    for key in [wrapper, text_area] {
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);

    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(properties.validation_errors.is_empty());
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, text_area, properties, generations)
}

fn transform_scroll_fixture(
    matrix: glam::Mat4,
) -> (
    NodeArena,
    NodeKey,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    transform_scroll_fixture_at_offset(matrix, 20.0)
}

fn transform_scroll_fixture_at_offset(
    matrix: glam::Mat4,
    offset_y: f32,
) -> (
    NodeArena,
    NodeKey,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_2001, 0.0, 0.0, 120.0, 90.0,
    ))));
    let scroll = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_2002, 0.0, 0.0, 120.0, 90.0,
    ))));
    let content = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_2010, 0.0, -offset_y, 120.0, 240.0,
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
    let mut style = Style::new();
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, scroll);
        element.apply_style(style);
        element.layout_state.content_size = Size {
            width: 120.0,
            height: 240.0,
        };
        element.set_scroll_offset((0.0, offset_y));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    crate::view::test_support::get_element_mut::<Element>(&arena, content)
        .set_background_color_value(Color::rgb(24, 48, 72));
    let mut content_style = Style::new();
    content_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    crate::view::test_support::get_element_mut::<Element>(&arena, content)
        .apply_style(content_style);
    arena
        .get_mut(content)
        .unwrap()
        .element
        .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(properties.validation_errors.is_empty());
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, scroll, content, properties, generations)
}

fn validated_transform_scroll_fixture_scene(
    arena: &NodeArena,
    root: NodeKey,
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
    sampled_at: crate::time::Instant,
) -> ValidatedTransformScrollScene {
    plan_and_validate_transform_scroll_scene(
        arena,
        &[root],
        properties,
        generations,
        1.0,
        [0.0; 2],
        None,
        sampled_at,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .expect("exact translation-only T->S fixture")
}

fn same_owner_transform_scroll_fixture() -> (
    NodeArena,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (arena, root, properties, generations) =
        super::super::frame_plan::tests::same_owner_transform_scroll_fixture();
    let children = arena.children_of(root);
    let [content] = children.as_slice() else {
        panic!("same-owner T+S fixture owns one direct content child")
    };
    (arena, root, *content, properties, generations)
}

fn same_owner_effect_scroll_fixture() -> (
    NodeArena,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (arena, root, properties, generations) =
        super::super::frame_plan::tests::same_owner_effect_scroll_fixture();
    let children = arena.children_of(root);
    let [content] = children.as_slice() else {
        panic!("same-owner E+S fixture owns one direct content child")
    };
    (arena, root, *content, properties, generations)
}

fn transform_effect_scroll_fixture()
-> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let (mut arena, root, scroll, _content, _, _) =
        transform_scroll_fixture(glam::Mat4::from_translation(glam::Vec3::new(7.0, 3.0, 0.0)));
    let effect = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_2003, 0.0, 0.0, 120.0, 90.0,
    ))));
    let mut effect_style = Style::new();
    effect_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, effect);
        element.apply_style(effect_style);
        element.set_opacity(0.625);
        element.set_background_color_value(Color::rgb(32, 64, 96));
    }
    arena.set_children(root, vec![effect]);
    arena.set_parent(effect, Some(root));
    arena.set_children(effect, vec![scroll]);
    arena.set_parent(scroll, Some(effect));
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(properties.validation_errors.is_empty());
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, properties, generations)
}

fn same_owner_transform_effect_scroll_fixture()
-> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let (arena, root, _scroll, _content, _, _) =
        transform_scroll_fixture(glam::Mat4::from_translation(glam::Vec3::new(7.0, 3.0, 0.0)));
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, root);
        element.set_opacity(0.625);
        element.set_background_color_value(Color::rgb(32, 64, 96));
    }
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(properties.validation_errors.is_empty());
    assert!(properties.transforms.contains_key(&TransformNodeId(root)));
    assert!(
        properties
            .effects
            .contains_key(&crate::view::compositor::property_tree::EffectNodeId(root))
    );
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, properties, generations)
}

fn effect_transform_scroll_fixture()
-> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let (mut arena, transform, _scroll, _content, _, _) =
        transform_scroll_fixture(glam::Mat4::from_translation(glam::Vec3::new(7.0, 3.0, 0.0)));
    let effect = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_2100, 0.0, 0.0, 168.0, 112.0,
    ))));
    let mut effect_style = Style::new();
    effect_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, effect);
        element.apply_style(effect_style);
        element.set_opacity(0.625);
        element.set_background_color_value(Color::rgb(32, 64, 96));
    }
    arena.set_children(effect, vec![transform]);
    arena.set_parent(transform, Some(effect));
    arena.refresh_subtree_dirty_cache(effect);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[effect]);
    assert!(properties.validation_errors.is_empty());
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[effect], &properties);
    (arena, effect, properties, generations)
}

fn effect_transform_scroll_neutral_fixture()
-> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let (mut arena, effect, _, _) = effect_transform_scroll_fixture();
    let transform = arena.children_of(effect)[0];
    let scroll = arena.children_of(transform)[0];
    let outer_wrapper = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_2110, 0.0, 0.0, 140.0, 100.0,
    ))));
    let inner_wrapper = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_2111, 0.0, 0.0, 130.0, 95.0,
    ))));
    for wrapper in [outer_wrapper, inner_wrapper] {
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        let mut element =
            crate::view::test_support::get_element_mut::<Element>(&arena, wrapper);
        element.apply_style(style);
        element.set_background_color_value(Color::rgb(12, 24, 36));
    }
    arena.set_children(effect, vec![outer_wrapper]);
    arena.set_parent(outer_wrapper, Some(effect));
    arena.set_children(outer_wrapper, vec![transform]);
    arena.set_parent(transform, Some(outer_wrapper));
    arena.set_children(transform, vec![inner_wrapper]);
    arena.set_parent(inner_wrapper, Some(transform));
    arena.set_children(inner_wrapper, vec![scroll]);
    arena.set_parent(scroll, Some(inner_wrapper));
    arena.refresh_subtree_dirty_cache(effect);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[effect]);
    assert!(properties.validation_errors.is_empty());
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[effect], &properties);
    (arena, effect, properties, generations)
}









fn scroll_content_effect_native_leaf_fixture(
    kind: &str,
    state: &str,
    outer_transform: bool,
    neutral_wrapper: bool,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    const SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='48' height='24'><rect width='48' height='24' fill='#38bdf8'/></svg>";
    let (mut arena, root, _, _) =
        super::super::frame_plan::tests::scroll_content_effect_interleave_fixture(
            outer_transform,
            neutral_wrapper,
        );
    let leaf = arena.find_by_stable_id(0xb4_3021).unwrap();
    let stable_id = 0xb4_3021;
    let native: Box<dyn ElementTrait> = match kind {
        "text" => {
            let mut text =
                Text::new_with_id(stable_id, 12.0, -8.0, 48.0, 24.0, "retained native text");
            text.set_font("sans-serif");
            text.set_font_size(14.0);
            text.set_color(Color::rgb(24, 96, 192));
            Box::new(text)
        }
        "image" => {
            let source = if state == "ready" {
                crate::view::ImageSource::Rgba {
                    width: 2,
                    height: 2,
                    pixels: Arc::from([
                        255_u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
                    ]),
                }
            } else {
                crate::view::ImageSource::Path(format!("phase3-{state}.png").into())
            };
            let image = Image::new_with_id(stable_id, source);
            match state {
                "loading" => image.set_resource_loading_for_test(),
                "error" => image.set_resource_error_for_test(),
                _ => {}
            }
            Box::new(image)
        }
        "svg" => {
            let source = if state == "ready" {
                crate::view::SvgSource::Content(SVG.into())
            } else {
                crate::view::SvgSource::Path(format!("phase3-{state}.svg").into())
            };
            let svg = Svg::new_with_id(stable_id, source);
            match state {
                "loading" => svg.set_document_loading_for_transform_test(),
                "error" => svg.set_document_error_for_transform_test(),
                _ => {}
            }
            Box::new(svg)
        }
        _ => unreachable!(),
    };
    *arena.get_mut(leaf).unwrap().element = native;
    arena.refresh_stable_id_index();
    if kind == "text" {
        arena.with_element_taken(leaf, |element, arena| {
            element.sync_arena(arena);
            element.measure(
                LayoutConstraints {
                    max_width: 48.0,
                    max_height: 24.0,
                    viewport_width: 640.0,
                    viewport_height: 480.0,
                    percent_base_width: Some(48.0),
                    percent_base_height: Some(24.0),
                },
                arena,
            );
            element.place(
                LayoutPlacement {
                    parent_x: 12.0,
                    parent_y: -8.0,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: 48.0,
                    available_height: 24.0,
                    viewport_width: 640.0,
                    viewport_height: 480.0,
                    percent_base_width: Some(48.0),
                    percent_base_height: Some(24.0),
                },
                arena,
            );
            element.clear_local_dirty_flags(DirtyFlags::ALL);
        });
    }
    if matches!(kind, "image" | "svg") {
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(48.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(24.0)));
        {
            let mut node = arena.get_mut(leaf).unwrap();
            if let Some(image) = node.element.as_any_mut().downcast_mut::<Image>() {
                image.apply_style(style);
            } else {
                node.element
                    .as_any_mut()
                    .downcast_mut::<Svg>()
                    .unwrap()
                    .apply_style(style);
            }
        }
        arena.with_element_taken(leaf, |element, arena| {
            element.sync_arena(arena);
            element.measure(
                LayoutConstraints {
                    max_width: 48.0,
                    max_height: 24.0,
                    viewport_width: 640.0,
                    viewport_height: 480.0,
                    percent_base_width: Some(48.0),
                    percent_base_height: Some(24.0),
                },
                arena,
            );
            element.place(
                LayoutPlacement {
                    parent_x: 12.0,
                    parent_y: -8.0,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: 48.0,
                    available_height: 24.0,
                    viewport_width: 640.0,
                    viewport_height: 480.0,
                    percent_base_width: Some(48.0),
                    percent_base_height: Some(24.0),
                },
                arena,
            );
            element.prepare_paint_resources(PaintResourcePreparationContext {
                frame_number: 1,
                device_scale: 2.0,
                now: crate::time::Instant::now(),
            });
            element.clear_local_dirty_flags(DirtyFlags::ALL);
        });
        if kind == "svg" && state == "ready" {
            arena
                .get_mut(leaf)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<Svg>()
                .unwrap()
                .prepare_content_paint_for_test(SVG, (48.0, 24.0), 2.0)
                .unwrap();
        }
        arena.clear_arena_dirty_subtree(leaf, DirtyFlags::ALL);
    }
    arena
        .get_mut(leaf)
        .unwrap()
        .element
        .clear_local_dirty_flags(DirtyFlags::ALL);
    arena.clear_arena_dirty_subtree(leaf, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, properties, generations)
}



fn validated_transform_effect_scroll_fixture_scene(
    arena: &NodeArena,
    root: NodeKey,
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
    sampled_at: crate::time::Instant,
) -> ValidatedTransformEffectScrollScene {
    plan_and_validate_transform_effect_scroll_scene(
        arena,
        &[root],
        properties,
        generations,
        1.0,
        [0.0; 2],
        None,
        sampled_at,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .expect("exact T -> E -> Scroll fixture")
}



fn validated_effect_transform_scroll_fixture_scene(
    arena: &NodeArena,
    root: NodeKey,
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
    sampled_at: crate::time::Instant,
    scale_factor: f32,
) -> ValidatedEffectTransformScrollScene {
    plan_and_validate_effect_transform_scroll_scene(
        arena,
        &[root],
        properties,
        generations,
        scale_factor,
        [0.0; 2],
        None,
        sampled_at,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .expect("exact E -> T -> Scroll fixture")
}

fn prepare_and_emit_boundary_dag_fixture(
    viewport: &mut Viewport,
    scene: ValidatedPropertyBoundaryDagScene,
    refresh_actions: bool,
) -> (RetainedPropertyScrollSceneBuildTrace, usize) {
    assert!(scene.is_canonical());
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    let mut prepared = prepare_property_boundary_dag_scene_from_pool(
        viewport,
        scene,
        &mut graph,
        UiBuildContext::new(800, 600, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        owner,
    )
    .expect("DAG joint prepare reproduces the exact legacy grammar");
    assert_eq!(
        prepared.graph_build_state_snapshot_for_test(),
        graph_before,
        "joint prepare stays graph-inert"
    );
    if refresh_actions {
        prepared.refresh_actions_from_committed_test_pool();
    }
    assert!(prepared.action_set_is_exact_for_test());
    assert!(prepared.rejects_action_set_mismatch_for_test());
    assert_eq!(
        prepared.graph_build_state_snapshot_for_test(),
        graph_before,
        "action freeze and mismatch rejection stay graph-inert"
    );
    let outcome = emit_prepared_property_boundary_dag_scene(prepared);
    let (_, trace) = outcome.into_parts();
    assert!(
        !viewport.retained_property_scroll_scene_stage_is_available(),
        "recursive emit consumes the joint transaction stage exactly once"
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    let pass_count = graph.pass_descriptors().len();
    (trace, pass_count)
}

















#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScrollbarCase {
    Hidden,
    Opaque,
    Translucent,
}

impl ScrollbarCase {
    const ALL: [Self; 3] = [Self::Hidden, Self::Opaque, Self::Translucent];

    fn expected_paint_state(self) -> ScrollbarPaintStateWitness {
        match self {
            Self::Hidden => ScrollbarPaintStateWitness::HiddenNow,
            Self::Opaque => ScrollbarPaintStateWitness::OpaqueNow,
            Self::Translucent => ScrollbarPaintStateWitness::TranslucentNow,
        }
    }
}

fn fixture_with_geometry(
    offset: [f32; 2],
    viewport_size: [f32; 2],
    content_size: [f32; 2],
) -> (
    NodeArena,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    fixture_with_geometry_and_scrollbar(
        offset,
        viewport_size,
        content_size,
        ScrollbarCase::Hidden,
        3.0,
    )
}

fn fixture_with_geometry_and_scrollbar(
    offset: [f32; 2],
    viewport_size: [f32; 2],
    content_size: [f32; 2],
    scrollbar: ScrollbarCase,
    shadow_blur_radius: f32,
) -> (
    NodeArena,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    fixture_with_geometry_and_scrollbar_elapsed(
        offset,
        viewport_size,
        content_size,
        scrollbar,
        shadow_blur_radius,
        1_000,
    )
}

fn fixture_with_geometry_and_scrollbar_elapsed(
    offset: [f32; 2],
    viewport_size: [f32; 2],
    content_size: [f32; 2],
    scrollbar: ScrollbarCase,
    shadow_blur_radius: f32,
    fade_elapsed_ms: u64,
) -> (
    NodeArena,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        82_001,
        0.0,
        0.0,
        viewport_size[0],
        viewport_size[1],
    ))));
    let child = arena.insert(Node::new(Box::new(Element::new_with_id(
        82_002,
        -offset[0],
        -offset[1],
        content_size[0],
        content_size[1],
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
        let mut root_node = arena.get_mut(root).unwrap();
        let root_element = root_node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap();
        root_element.apply_style(style);
        root_element.layout_state.content_size = Size {
            width: content_size[0],
            height: content_size[1],
        };
        root_element.set_scroll_offset((offset[0], offset[1]));
        root_element.set_scrollbar_shadow_blur_radius(shadow_blur_radius);
        match scrollbar {
            ScrollbarCase::Hidden => {}
            ScrollbarCase::Opaque => {
                root_element.set_hovered(true);
            }
            ScrollbarCase::Translucent => {
                root_element.set_hovered(true);
                root_element.set_hovered(false);
                let sampled_at = crate::time::Instant::now();
                let _ = root_element.tick_post_layout_animation_frame(sampled_at);
                let _ = root_element.tick_post_layout_animation_frame(
                    sampled_at + crate::time::Duration::from_millis(fade_elapsed_ms),
                );
            }
        }
        root_element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena
        .get_mut(child)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .set_background_color_value(Color::rgb(24, 48, 72));
    arena
        .get_mut(child)
        .unwrap()
        .element
        .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(
        properties.validation_errors.is_empty(),
        "{:?}",
        properties.validation_errors
    );
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, child, properties, generations)
}

fn exact_multi_root_fixture_with_content_heights(
    content_heights: [f32; 2],
) -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    exact_multi_root_fixture_with_geometry(content_heights, [20.0, 36.0])
}

fn exact_multi_root_fixture_with_geometry(
    content_heights: [f32; 2],
    offsets: [f32; 2],
) -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut arena = NodeArena::new();
    let mut roots = Vec::new();
    for (ordinal, offset_y) in offsets.into_iter().enumerate() {
        let content_height = content_heights[ordinal];
        let stable_base = 91_000_u64 + u64::try_from(ordinal).unwrap() * 10;
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
            300.0,
            content_height,
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
            let mut root_node = arena.get_mut(root).unwrap();
            let root_element = root_node
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .unwrap();
            root_element.apply_style(style);
            root_element.layout_state.content_size = Size {
                width: 300.0,
                height: content_height,
            };
            root_element.set_scroll_offset((0.0, offset_y));
            root_element.set_background_color_value(Color::rgb(8, 16, 24));
            root_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        {
            let mut child_node = arena.get_mut(child).unwrap();
            let child_element = child_node
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .unwrap();
            child_element.set_background_color_value(Color::rgb(24, 48, 72));
            child_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        arena.refresh_subtree_dirty_cache(root);
        roots.push(root);
    }
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    assert!(properties.validation_errors.is_empty());
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    assert!(generations.matches_live_snapshot(&arena, &roots, &properties));
    (arena, roots, properties, generations)
}

fn exact_multi_root_fixture() -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    exact_multi_root_fixture_with_content_heights([300.0, 300.0])
}

fn plan_at_offset(offset: [f32; 2]) -> ScrollScenePlan {
    let (arena, root, _child, properties, generations) = fixture_at_offset(offset);
    plan_from_fixture(&arena, root, &properties, &generations)
}

fn plan_from_fixture(
    arena: &NodeArena,
    root: NodeKey,
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
) -> ScrollScenePlan {
    plan_single_root_scroll_scene(arena, &[root], properties, generations, 1.0, [0.0; 2], None)
        .unwrap()
}

fn generous_budget() -> ScrollSceneSingleTextureBudget {
    ScrollSceneSingleTextureBudget::new(8192, 128 * 1024 * 1024).unwrap()
}

fn tiled_plan() -> ScrollScenePlan {
    let (arena, root, _child, properties, generations) =
        fixture_with_geometry([0.0, 900.0], [100.0, 80.0], [300.0, 3000.0]);
    plan_from_fixture(&arena, root, &properties, &generations)
}

fn tiled_budget() -> ScrollSceneSingleTextureBudget {
    ScrollSceneSingleTextureBudget::new(2048, 64 * 1024 * 1024).unwrap()
}

fn property_scroll_plan_from_fixture(
    arena: &NodeArena,
    root: NodeKey,
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
    semantic_frame_time: crate::time::Instant,
    budget: ScrollSceneSingleTextureBudget,
) -> Result<PropertyScrollScenePlan, PropertyScrollScenePlanError> {
    plan_property_scroll_scene_scaffold(
        arena,
        &[root],
        properties,
        generations,
        1.0,
        [0.0; 2],
        None,
        semantic_frame_time,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        budget,
    )
}

fn translucent_fixture_at(
    fade_elapsed_ms: u64,
) -> (
    NodeArena,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
    crate::time::Instant,
) {
    assert!((901..1_250).contains(&fade_elapsed_ms));
    let (arena, root, _, mut properties, mut generations) = fixture_at_offset([0.0, 20.0]);
    let start = crate::time::Instant::now();
    let leave = start + crate::time::Duration::from_millis(10);
    let sampled_at = leave + crate::time::Duration::from_millis(fade_elapsed_ms);
    {
        let mut root_node = arena.get_mut(root).unwrap();
        let element = root_node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap();
        assert!(element.set_hovered(true));
        let _ = element.tick_post_layout_animation_frame(start);
        assert!(element.set_hovered(false));
        let _ = element.tick_post_layout_animation_frame(leave);
        let _ = element.tick_post_layout_animation_frame(sampled_at);
    }
    arena.refresh_subtree_dirty_cache(root);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let scroll = properties
        .scroll_snapshot_for(crate::view::compositor::property_tree::ScrollNodeId(root))
        .unwrap();
    assert_eq!(
        scroll.scrollbar_overlay.paint_state,
        ScrollbarPaintStateWitness::TranslucentNow
    );
    assert!(generations.matches_live_snapshot(&arena, &[root], &properties));
    (arena, root, properties, generations, sampled_at)
}

fn property_scroll_backing_mut(
    plan: &mut PropertyScrollScenePlan,
) -> &mut PropertyScrollBackingPlan {
    let ScrollBoundaryStep::ContentComposite { backing, .. } = &mut plan.steps[1] else {
        unreachable!("canonical B0 plan has one content-composite step");
    };
    backing
}

fn assert_property_scroll_plan_tamper_rejected(
    plan: &PropertyScrollScenePlan,
    tamper: impl FnOnce(&mut PropertyScrollScenePlan),
) {
    let mut tampered = plan.clone();
    tamper(&mut tampered);
    assert!(!tampered.is_canonical());
}

fn validated_property_scroll_boundary_from_fixture(
    arena: &NodeArena,
    root: NodeKey,
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
    semantic_frame_time: crate::time::Instant,
    budget: ScrollSceneSingleTextureBudget,
) -> ValidatedPropertyScrollBoundary {
    let plan = property_scroll_plan_from_fixture(
        arena,
        root,
        properties,
        generations,
        semantic_frame_time,
        budget,
    )
    .unwrap();
    validate_property_scroll_boundary(
        plan,
        arena,
        &[root],
        properties,
        generations,
        semantic_frame_time,
    )
    .unwrap()
}

fn fake_text_area_sidecar_from_direct(
    direct: RetainedScrollHostAdmissionSnapshot,
) -> RetainedScrollTextAreaSubtreeAdmissionSnapshot {
    RetainedScrollTextAreaSubtreeAdmissionSnapshot {
        boundary_root: direct.boundary_root,
        stable_id: direct.stable_id,
        content_wrapper: direct.child,
        content_wrapper_stable_id: direct.child_stable_id,
        text_area_root: direct.child,
        text_area_stable_id: direct.child_stable_id,
        paint_grammar:
            crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly,
        source_bounds: direct.source_bounds,
        scroll: direct.scroll,
    }
}


fn compiled_content_step(
    boundary: &ValidatedPropertyScrollBoundary,
) -> (
    &PaintArtifact,
    &PropertyScrollContentCompileStamp,
    PropertyScrollCompositeDependency,
    &PropertyScrollClipSplitWitness,
    u32,
    u32,
) {
    let PropertyScrollCompiledStep::DetachedContent {
        artifact,
        stamp,
        composite,
        clip_split,
        parent_before,
        parent_after,
        ..
    } = &boundary.steps[1]
    else {
        unreachable!("canonical B1 boundary has detached content at index 1");
    };
    (
        artifact,
        stamp,
        *composite,
        clip_split,
        *parent_before,
        *parent_after,
    )
}
















fn prepared_content_stamps(prepared: &PreparedScrollScene) -> Vec<RetainedSurfaceRasterStamp> {
    match &prepared.content_backing {
        PreparedScrollContentBacking::Single { stamp, .. } => vec![stamp.clone()],
        PreparedScrollContentBacking::Tiled { tiles, .. } => {
            tiles.iter().map(|tile| tile.stamp.clone()).collect()
        }
    }
}



#[derive(Clone, Copy, Debug)]
struct PoolMatrixCase {
    name: &'static str,
    scrollbar: ScrollbarCase,
    offset: [f32; 2],
    content_size: [f32; 2],
    backing: ScrollSceneBackingKind,
    max_dimension_2d: u32,
}

fn pool_matrix_fixture(
    case: PoolMatrixCase,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let (arena, root, _, _, _) = fixture_with_geometry_and_scrollbar(
        case.offset,
        [100.0, 80.0],
        case.content_size,
        case.scrollbar,
        0.0,
    );
    arena
        .get_mut(root)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .set_background_color_value(Color::rgb(13, 29, 47));
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(properties.validation_errors.is_empty());
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, properties, generations)
}

fn build_pool_matrix_frame(
    viewport: &mut Viewport,
    case: PoolMatrixCase,
    arena: &NodeArena,
    root: NodeKey,
) -> (
    FrameGraph,
    ScrollSceneBuildTrace,
    Option<crate::view::frame_graph::texture_resource::TextureHandle>,
    [u32; 4],
) {
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let parent = ctx.allocate_target(&mut graph);
    let parent_handle = parent.handle();
    ctx.set_current_target(parent);
    graph.add_graphics_pass(ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: parent,
        },
    ));
    let outcome = build_scroll_scene_from_pool_with_forced_pair_witness_for_test(
        viewport,
        arena,
        &[root],
        &mut graph,
        ctx,
        case.max_dimension_2d,
        64 * 1024 * 1024,
    )
    .expect("pool matrix scene must build");
    let (_, trace) = outcome.into_parts();
    (graph, trace, parent_handle, [0, 0, 100, 80])
}

fn assert_pool_matrix_pass_order(
    mut graph: FrameGraph,
    parent: Option<crate::view::frame_graph::texture_resource::TextureHandle>,
    contents_clip: [u32; 4],
    scrollbar: ScrollbarCase,
    tile_count: usize,
    label: &str,
) {
    let snapshot = graph.test_compile_snapshot().unwrap();
    let payloads = snapshot.pass_payloads();
    let host_before = payloads
        .iter()
        .position(|payload| {
            matches!(
                payload,
                FramePassTestPayload::DrawRect(rect)
                    if rect.output_target == parent
                        && rect.fill_color_bits[3] == 1.0_f32.to_bits()
            )
        })
        .expect("opaque host-before pass must be observable");
    let content_composites = payloads
        .iter()
        .enumerate()
        .filter_map(|(index, payload)| match payload {
            FramePassTestPayload::TextureComposite(composite)
                if !composite.use_mask
                    && composite.output_target == parent
                    && composite.effective_scissor_rect == Some(contents_clip) =>
            {
                Some((index, f32::from_bits(composite.bounds_bits[1])))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        content_composites.len(),
        tile_count,
        "{label}: one parent composite is required per content backing"
    );
    assert!(
        content_composites
            .iter()
            .all(|(index, _)| *index > host_before),
        "{label}: host-before must precede every content composite"
    );
    assert!(
        content_composites
            .windows(2)
            .all(|pair| pair[0].0 < pair[1].0 && pair[0].1 < pair[1].1),
        "{label}: tiled composites must remain row-major"
    );
    let overlay_start = content_composites.last().unwrap().0 + 1;
    let shadow_fill_alphas = payloads
        .iter()
        .filter_map(|payload| match payload {
            FramePassTestPayload::ShadowFill(fill) => Some(fill.color_bits[3]),
            _ => None,
        })
        .collect::<Vec<_>>();
    let overlay = payloads[overlay_start..]
        .iter()
        .map(|payload| match payload {
            FramePassTestPayload::TextureComposite(composite) if composite.use_mask => {
                "masked-shadow"
            }
            FramePassTestPayload::DrawRect(rect)
                if rect.output_target == parent
                    && rect.fill_color_bits[3] != 1.0_f32.to_bits() =>
            {
                "overlay-fill"
            }
            other => panic!("{label}: unexpected pass after content composite: {other:?}"),
        })
        .collect::<Vec<_>>();
    match scrollbar {
        ScrollbarCase::Hidden => {
            assert!(shadow_fill_alphas.is_empty());
            assert!(
                overlay.is_empty(),
                "{label}: hidden overlay must remain an empty terminal artifact"
            );
        }
        ScrollbarCase::Opaque | ScrollbarCase::Translucent => {
            assert_eq!(shadow_fill_alphas.len(), 4);
            assert_eq!(shadow_fill_alphas[0], shadow_fill_alphas[2]);
            assert_ne!(shadow_fill_alphas[0], 1.0_f32.to_bits());
            assert_eq!(shadow_fill_alphas[1], 1.0_f32.to_bits());
            assert_eq!(shadow_fill_alphas[3], 1.0_f32.to_bits());
            assert_eq!(
                overlay,
                [
                    "masked-shadow",
                    "overlay-fill",
                    "masked-shadow",
                    "overlay-fill",
                ],
                "{label}: parent-visible overlay must remain track shadow/fill then thumb shadow/fill and be last"
            );
        }
    }
}








fn prepare(
    plan: &ScrollScenePlan,
    graph: &FrameGraph,
    format: wgpu::TextureFormat,
) -> Result<PreparedScrollScene, ScrollScenePrepareError> {
    let ctx = UiBuildContext::new(640, 480, format, 1.0);
    prepare_scroll_scene(plan.clone(), graph, &ctx, generous_budget())
}

fn prepare_live(
    arena: &NodeArena,
    root: NodeKey,
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
    graph: &FrameGraph,
) -> Result<PreparedScrollScene, ScrollSceneFromLiveError> {
    let ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    prepare_scroll_scene_from_live(
        arena,
        &[root],
        properties,
        generations,
        graph,
        &ctx,
        generous_budget(),
    )
}


fn prepared_scene_for_emit(
    graph: &mut FrameGraph,
) -> (PreparedScrollScene, UiBuildContext, RenderTargetOut) {
    let plan = plan_at_offset([0.0, 20.0]);
    let mut ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let parent = ctx.allocate_target(graph);
    ctx.set_current_target(parent);
    let prepared =
        prepare_scroll_scene(plan, graph, &ctx, generous_budget()).expect("exact scene");
    (prepared, ctx, parent)
}












































fn direct_scroll_transform_dpr_fixture()
-> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xd2_5101, 0.0, 0.0, 120.0, 90.0,
    ))));
    let content = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xd2_5102, 0.0, -20.0, 120.0, 240.0,
    ))));
    arena.set_parent(content, Some(root));
    arena.push_child(root, content);
    let mut scroll_style = Style::new();
    scroll_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    scroll_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, root);
        element.apply_style(scroll_style);
        element.layout_state.content_size = Size {
            width: 120.0,
            height: 240.0,
        };
        element.set_scroll_offset((0.0, 20.0));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    {
        let mut element =
            crate::view::test_support::get_element_mut::<Element>(&arena, content);
        element.set_background_color_value(Color::rgb(24, 48, 72));
        element.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
            glam::Vec3::new(3.0, 0.0, 0.0),
        )));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(properties.validation_errors.is_empty());
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, properties, generations)
}

fn assert_dpr2_target(stamp: &RetainedSurfaceRasterStamp, logical_size: [u32; 2]) {
    assert_eq!(stamp.target.scale_factor_bits, 2.0_f32.to_bits());
    assert_eq!(stamp.target.color.width(), logical_size[0] * 2);
    assert_eq!(stamp.target.color.height(), logical_size[1] * 2);
    assert_eq!(stamp.target.depth.width(), logical_size[0] * 2);
    assert_eq!(stamp.target.depth.height(), logical_size[1] * 2);
}

mod frame_root_scroll_tests;
mod nested_scroll_corpus_tests;
mod nested_scroll_localizer_tests;
mod nested_scroll_executor_tests;
mod nested_scroll_preflight_tests;
mod scroll_content_effect_tests;
mod scroll_content_effect_reuse_tests;
mod property_boundary_dag_tests;
mod transform_effect_scroll_plan_tests;
mod transform_effect_scroll_prepare_tests;
mod transform_effect_scroll_action_tests;
mod effect_scroll_tests;
mod property_scroll_b1_tests;
mod property_scroll_b0_tests;
mod tiled_content_tests;
mod fused_live_prepare_tests;
mod content_artifact_prepare_tests;
mod property_scroll_b2_tests;
mod property_scroll_b4_tests;
mod same_owner_scroll_tests;
mod transform_scroll_action_tests;
mod nested_scroll_receiver_geometry_tests;
mod dpr2_device_target_tests;
