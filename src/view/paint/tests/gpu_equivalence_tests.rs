use super::*;

use crate::style::{ColorLike, ScrollDirection};
use crate::view::base_component::{DirtyPassMask, Size};
use crate::view::frame_graph::ExternalSinkKind;
use crate::view::render_pass::draw_rect_pass::{RenderTargetIn, RenderTargetOut};
use crate::view::render_pass::present_surface_pass::{
    PresentSurfaceInput, PresentSurfaceOutput, PresentSurfaceParams, PresentSurfacePass,
};
use crate::view::viewport::{Viewport, emit_retained_auto_artifact_surface_for_test};

const WIDTH: u32 = 67;
const HEIGHT: u32 = 64;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const BYTES_PER_PIXEL: u32 = 4;
// At DPR 1 the owner snap maps this fractional placement to the independently
// checkable physical-pixel delta [4, -2] used by the Transform oracle below.
const ARTIFACT_HOST_PLACEMENT_OFFSET: [f32; 2] = [3.5, -2.25];
const COPY_BYTES_PER_ROW_ALIGNMENT: u32 = 256;
const ROOT_GROUP_FIRST_COLOR: [f32; 4] = [1.0, 0.08, 0.02, 0.75];
const ROOT_GROUP_SECOND_COLOR: [f32; 4] = [0.02, 0.12, 1.0, 0.65];

struct NativeGpu {
    _instance: wgpu::Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter_info: wgpu::AdapterInfo,
}

fn is_hardware_gpu_adapter_type(device_type: wgpu::DeviceType) -> bool {
    matches!(
        device_type,
        wgpu::DeviceType::IntegratedGpu
            | wgpu::DeviceType::DiscreteGpu
            // wgpu defines VirtualGpu as "Virtual / Hosted" rather than CPU
            // software rendering. Allow it for hardware-backed passthrough
            // runners; Cpu and unknown Other adapters cannot prove this gate.
            | wgpu::DeviceType::VirtualGpu
    )
}

fn native_gpu_test_context() -> Result<std::sync::MutexGuard<'static, Option<NativeGpu>>, String> {
    static GPU: std::sync::Mutex<Option<NativeGpu>> = std::sync::Mutex::new(None);
    let mut gpu = GPU.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if gpu.is_none() {
        *gpu = Some(NativeGpu::request()?);
    }
    Ok(gpu)
}

impl NativeGpu {
    fn request() -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            flags: wgpu::InstanceFlags::empty(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|error| format!("native GPU adapter is required for pixel parity: {error:?}"))?;
        let adapter_info = adapter.get_info();
        if !is_hardware_gpu_adapter_type(adapter_info.device_type) {
            return Err(format!(
                "hardware GPU adapter is required for the native release gate: name={}, backend={:?}, device_type={:?}, driver={}",
                adapter_info.name,
                adapter_info.backend,
                adapter_info.device_type,
                adapter_info.driver,
            ));
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("rfgui native pixel parity device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        }))
        .map_err(|error| format!("failed to create pixel parity device: {error:?}"))?;
        Ok(Self {
            _instance: instance,
            device,
            queue,
            adapter_info,
        })
    }

    fn label(&self) -> String {
        format!(
            "{} ({:?}, {:?}, driver={})",
            self.adapter_info.name,
            self.adapter_info.backend,
            self.adapter_info.device_type,
            self.adapter_info.driver
        )
    }
}

fn fixture(with_border: bool) -> (NodeArena, Vec<NodeKey>) {
    let mut element =
        Element::new_with_id(if with_border { 202 } else { 201 }, 8.0, 8.0, 32.0, 24.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(40, 80, 160)),
    );
    if with_border {
        style.set_border(Border::uniform(Length::px(4.0), &Color::rgb(220, 60, 20)));
    }
    element.apply_style(style);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(element));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    (arena, vec![root])
}

fn self_clip_fixture() -> (NodeArena, Vec<NodeKey>) {
    let mut clipped = Element::new_with_id(301, 0.0, 0.0, 20.0, 16.0);
    let mut clipped_style = Style::new();
    clipped_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(220, 40, 30)),
    );
    clipped_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(30.0))
                .top(Length::px(8.0))
                .clip(ClipMode::AnchorParent),
        ),
    );
    clipped.apply_style(clipped_style);

    let mut sibling = Element::new_with_id(302, 30.0, 36.0, 20.0, 16.0);
    sibling.set_background_color_value(Color::rgb(30, 60, 220));

    let mut arena = new_test_arena();
    let clipped = commit_element(&mut arena, Box::new(clipped));
    let sibling = commit_element(&mut arena, Box::new(sibling));
    let measure = LayoutConstraints {
        max_width: WIDTH as f32,
        max_height: HEIGHT as f32,
        viewport_width: WIDTH as f32,
        viewport_height: HEIGHT as f32,
        percent_base_width: Some(WIDTH as f32),
        percent_base_height: Some(HEIGHT as f32),
    };
    let place = LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: WIDTH as f32,
        available_height: HEIGHT as f32,
        viewport_width: WIDTH as f32,
        viewport_height: HEIGHT as f32,
        percent_base_width: Some(WIDTH as f32),
        percent_base_height: Some(HEIGHT as f32),
    };
    measure_and_place(&mut arena, clipped, measure, place);
    measure_and_place(&mut arena, sibling, measure, place);
    (arena, vec![clipped, sibling])
}

fn graph_prelude() -> (FrameGraph, UiBuildContext, RenderTargetOut) {
    graph_prelude_with_format(FORMAT)
}

fn graph_prelude_with_format(
    format: wgpu::TextureFormat,
) -> (FrameGraph, UiBuildContext, RenderTargetOut) {
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(WIDTH, HEIGHT, format, 1.0);
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
    ctx.set_current_target(target.clone());
    (graph, ctx, target)
}

fn self_clip_graph_prelude() -> (FrameGraph, UiBuildContext, RenderTargetOut) {
    let (graph, mut ctx, target) = graph_prelude();
    ctx.replace_scissor_rect(Some([0, 0, 16, HEIGHT]));
    (graph, ctx, target)
}

fn add_present(graph: &mut FrameGraph, target: &RenderTargetOut) -> Result<(), String> {
    let handle = target
        .handle()
        .ok_or_else(|| "pixel parity target has no texture handle".to_string())?;
    let present = PresentSurfacePass::new(
        PresentSurfaceParams,
        PresentSurfaceInput {
            source: RenderTargetIn::with_handle(handle),
        },
        PresentSurfaceOutput,
    );
    let present_handle = graph.add_graphics_pass(present);
    graph
        .add_pass_sink(present_handle, ExternalSinkKind::SurfacePresent)
        .map_err(|error| format!("failed to register pixel parity sink: {error:?}"))?;
    Ok(())
}

fn artifact_graph(with_border: bool) -> Result<FrameGraph, String> {
    let (arena, roots) = fixture(with_border);
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) = whole_frame_artifact(&arena, &roots, &properties, &generations);
    if !eligibility.eligible {
        return Err(format!(
            "pixel fixture is not artifact eligible: {eligibility:?}"
        ));
    }
    let (mut graph, ctx, target) = graph_prelude();
    let _ = compile_artifact(&artifact, &mut graph, ctx);
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn zero_surface_v2_graph(with_border: bool) -> Result<FrameGraph, String> {
    let (arena, roots) = fixture(with_border);
    let (properties, generations) = sync_identity(&arena, &roots);
    let (mut graph, ctx, target) = graph_prelude();
    let mut viewport = Viewport::new();
    let emission = emit_retained_auto_artifact_surface_for_test(
        &mut viewport,
        &arena,
        &roots,
        &properties,
        &generations,
        &mut graph,
        &ctx,
    )?;
    if emission.surface_count != 0
        || emission.aggregate_texture_bytes != 0
        || !emission.actions.is_empty()
    {
        return Err(format!(
            "production zero-surface gate must stage no residents: surfaces={}, bytes={}, actions={:?}",
            emission.surface_count, emission.aggregate_texture_bytes, emission.actions,
        ));
    }
    if !viewport.finish_retained_surface_transaction_for_frame(Some(emission.frame_owner), true) {
        return Err("production zero-surface transaction owner was not current".to_owned());
    }
    add_present(&mut graph, &target)?;
    Ok(graph)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GpuScrollbarCase {
    Hidden,
}

fn transformed_rect_fixture() -> (NodeArena, NodeKey) {
    let mut element = Element::new_with_id(0xc3_a001, 10.0, 8.0, 28.0, 20.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(210, 55, 25)),
    );
    style.set_border(Border::uniform(Length::px(2.0), &Color::rgb(30, 190, 80)));
    style.set_transform(Transform::new([Translate::x(Length::px(6.0))]));
    element.apply_style(style);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(element));
    let measure = LayoutConstraints {
        max_width: WIDTH as f32,
        max_height: HEIGHT as f32,
        viewport_width: WIDTH as f32,
        viewport_height: HEIGHT as f32,
        percent_base_width: Some(WIDTH as f32),
        percent_base_height: Some(HEIGHT as f32),
    };
    let place = LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: WIDTH as f32,
        available_height: HEIGHT as f32,
        viewport_width: WIDTH as f32,
        viewport_height: HEIGHT as f32,
        percent_base_width: Some(WIDTH as f32),
        percent_base_height: Some(HEIGHT as f32),
    };
    measure_and_place(&mut arena, root, measure, place);
    (arena, root)
}

fn transformed_graph_prelude(
    scale_factor: f32,
    outer_scissor: Option<[u32; 4]>,
) -> (FrameGraph, UiBuildContext, RenderTargetOut) {
    transformed_graph_prelude_with_size(scale_factor, outer_scissor, [WIDTH, HEIGHT])
}

fn transformed_graph_prelude_with_size(
    scale_factor: f32,
    outer_scissor: Option<[u32; 4]>,
    [width, height]: [u32; 2],
) -> (FrameGraph, UiBuildContext, RenderTargetOut) {
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(width, height, FORMAT, scale_factor);
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
    if let Some(handle) = target.handle() {
        ctx.set_color_target(Some(handle));
    }
    ctx.push_scissor_rect(outer_scissor);
    ctx.set_current_target(target);
    (graph, ctx, target)
}

fn set_nested_scroll_gpu_position(element: &mut Element, x: f32, y: f32) {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NestedScrollGpuLeafKind {
    Rect,

    Svg,
}

fn nested_scroll_gpu_svg_source() -> SvgSource {
    let source = SvgSource::Content(
        r##"<svg width="100" height="600" xmlns="http://www.w3.org/2000/svg"><rect width="100" height="600" fill="#24d060"/><path d="M0 0 L100 100 L0 200 Z" fill="#2040e0"/><desc>nested-r1-gpu-closure</desc></svg>"##
            .to_string(),
    );
    static PRIMED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    PRIMED.get_or_init(|| prime_nested_scroll_gpu_svg(&source));
    source
}

fn prime_nested_scroll_gpu_svg(source: &SvgSource) {
    let document_key =
        crate::view::svg_resource::prime_svg_document_ready_for_test(source, 100.0, 600.0);
    let (width, height) = crate::view::svg_resource::quantize_svg_raster_size(100, 600);
    let request = crate::view::svg_resource::SvgRasterRequest::new(
        width,
        height,
        crate::view::svg_resource::SvgRasterMode::Fill,
    );
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let rgba = if (x / 12 + y / 24) % 2 == 0 {
                [36, 208, 96, 255]
            } else {
                [32, 64, 224, 255]
            };
            pixels.extend_from_slice(&rgba);
        }
    }
    crate::view::svg_resource::prime_svg_raster_ready_for_test(
        document_key,
        request,
        Arc::from(pixels),
    );
}

fn layout_nested_scroll_gpu_leaf(arena: &mut NodeArena, leaf: NodeKey) {
    arena.with_element_taken(leaf, |element, arena| {
        element.sync_arena(arena);
        element.measure(
            LayoutConstraints {
                max_width: 100.0,
                max_height: 600.0,
                viewport_width: WIDTH as f32,
                viewport_height: HEIGHT as f32,
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
                viewport_width: WIDTH as f32,
                viewport_height: HEIGHT as f32,
                percent_base_width: Some(100.0),
                percent_base_height: Some(600.0),
            },
            arena,
        );
        element.clear_local_dirty_flags(crate::view::base_component::DirtyFlags::ALL);
    });
    arena.clear_arena_dirty_subtree(leaf, crate::view::base_component::DirtyFlags::ALL);
}

fn prepare_nested_scroll_gpu_leaf(arena: &mut NodeArena, leaf: NodeKey, frame_number: u64) {
    arena.with_element_taken(leaf, |element, _arena| {
        element.prepare_paint_resources(
            crate::view::base_component::PaintResourcePreparationContext {
                frame_number,
                device_scale: 1.0,
                now: crate::time::Instant::now(),
            },
        );
    });
}

fn install_nested_scroll_gpu_leaf(
    arena: &mut NodeArena,
    leaf: NodeKey,
    kind: NestedScrollGpuLeafKind,
) {
    if kind == NestedScrollGpuLeafKind::Rect {
        return;
    }
    let stable_id = 0x1251_02;
    let replacement: Box<dyn ElementTrait> = match kind {
        NestedScrollGpuLeafKind::Rect => unreachable!(),

        NestedScrollGpuLeafKind::Svg => {
            let source = nested_scroll_gpu_svg_source();
            let mut svg = Svg::new_with_id(stable_id, source);
            svg.set_fit(crate::view::ImageFit::Fill);
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            style.insert(PropertyId::Width, ParsedValue::Length(Length::px(100.0)));
            style.insert(PropertyId::Height, ParsedValue::Length(Length::px(600.0)));
            svg.apply_style(style);
            Box::new(svg)
        }
    };
    *arena.get_mut(leaf).expect("nested GPU leaf exists").element = replacement;
    arena.refresh_stable_id_index();
    layout_nested_scroll_gpu_leaf(arena, leaf);
    prepare_nested_scroll_gpu_leaf(arena, leaf, 1);
    if kind == NestedScrollGpuLeafKind::Svg {
        arena.with_element_taken(leaf, |element, arena| element.sync_arena(arena));
        layout_nested_scroll_gpu_leaf(arena, leaf);
        prepare_nested_scroll_gpu_leaf(arena, leaf, 2);
    }
}

fn nested_scroll_gpu_leaf_fixture(
    kind: NestedScrollGpuLeafKind,
    outer_offset_y: f32,
    inner_offset_y: f32,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let (arena, outer, inner, leaf, mut properties, mut generations) = nested_scroll_plan_fixture();
    let mut arena = arena;
    install_nested_scroll_gpu_leaf(&mut arena, leaf, kind);
    let host_origin = [10.0, 20.0];
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, outer);
        set_nested_scroll_gpu_position(&mut element, host_origin[0], host_origin[1]);
        element.set_scroll_offset((0.0, outer_offset_y));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, inner);
        set_nested_scroll_gpu_position(
            &mut element,
            host_origin[0],
            host_origin[1] - outer_offset_y,
        );
        element.set_scroll_offset((0.0, inner_offset_y));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    {
        let target = [
            host_origin[0],
            host_origin[1] - outer_offset_y - inner_offset_y,
        ];
        let mut node = arena.get_mut(leaf).expect("nested GPU leaf exists");
        let bounds = node.element.box_model_snapshot();
        node.element
            .translate_in_place(target[0] - bounds.x, target[1] - bounds.y);
        node.element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena.refresh_subtree_dirty_cache(outer);
    properties.sync(&arena, &[outer]);
    generations.sync(&arena, &[outer], &properties);
    assert_eq!(properties.scrolls.len(), 2);
    (arena, outer, properties, generations)
}

fn root_group_overlap_rects() -> [RectPassParams; 2] {
    [
        RectPassParams {
            position: [8.0, 8.0],
            size: [30.0, 26.0],
            fill_color: ROOT_GROUP_FIRST_COLOR,
            opacity: 1.0,
            ..Default::default()
        },
        RectPassParams {
            position: [20.0, 16.0],
            size: [30.0, 26.0],
            fill_color: ROOT_GROUP_SECOND_COLOR,
            opacity: 1.0,
            ..Default::default()
        },
    ]
}

fn premultiply(color: [f32; 4]) -> [f32; 4] {
    [
        color[0] * color[3],
        color[1] * color[3],
        color[2] * color[3],
        color[3],
    ]
}

fn source_over(source: [f32; 4], destination: [f32; 4]) -> [f32; 4] {
    let destination_factor = 1.0 - source[3];
    [
        source[0] + destination[0] * destination_factor,
        source[1] + destination[1] * destination_factor,
        source[2] + destination[2] * destination_factor,
        source[3] + destination[3] * destination_factor,
    ]
}

fn scale_premultiplied(color: [f32; 4], opacity: f32) -> [f32; 4] {
    color.map(|channel| channel * opacity.clamp(0.0, 1.0))
}

fn premultiplied_to_readback_rgba8(color: [f32; 4]) -> [u8; 4] {
    if color[3] <= 0.000_001 {
        return [0; 4];
    }
    let straight = [
        color[0] / color[3],
        color[1] / color[3],
        color[2] / color[3],
        color[3],
    ];
    straight.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn quantize_premultiplied_rgba8(color: [f32; 4]) -> [f32; 4] {
    // Preserve the supplied f32 value while evaluating the UNORM conversion:
    // f32 multiplication by 255 can itself round a value just below a half
    // code onto the tie before the actual quantization (e.g. alpha 0.7).
    color.map(|v| ((f64::from(v.clamp(0.0, 1.0)) * 255.0).round() / 255.0) as f32)
}

fn root_group_raster_readback(color: [f32; 4], opacity: f32) -> [u8; 4] {
    // Both the reusable layer and the receiver are RGBA8 UNORM targets.
    // Quantize at both writes before the final straight-alpha readback; a
    // low-alpha unpremultiply can amplify a one-byte raster difference.
    let layer = quantize_premultiplied_rgba8(color);
    let receiver = quantize_premultiplied_rgba8(scale_premultiplied(layer, opacity));
    premultiplied_to_readback_rgba8(receiver)
}

fn root_group_anchor_oracle(opacity: f32) -> [[u8; 4]; 3] {
    let first = quantize_premultiplied_rgba8(premultiply(ROOT_GROUP_FIRST_COLOR));
    let second = premultiply(ROOT_GROUP_SECOND_COLOR);
    [
        root_group_raster_readback(first, opacity),
        root_group_raster_readback(source_over(second, first), opacity),
        root_group_raster_readback(second, opacity),
    ]
}

fn root_group_overlap_artifact(opacity: f32) -> PaintArtifact {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(
            0x6c50,
            0.0,
            0.0,
            WIDTH as f32,
            HEIGHT as f32,
        )),
    );
    let first = commit_child(
        &mut arena,
        root,
        Box::new(Element::new_with_id(0x6c51, 0.0, 0.0, 1.0, 1.0)),
    );
    let second = commit_child(
        &mut arena,
        root,
        Box::new(Element::new_with_id(0x6c52, 0.0, 0.0, 1.0, 1.0)),
    );
    let effect = EffectNodeId(root);
    let properties = PropertyTreeState {
        effect: Some(effect),
        ..Default::default()
    };
    let revision = PaintContentRevision {
        self_paint_revision: 1,
        composite_revision: 1,
        topology_revision: 1,
    };
    let rects = root_group_overlap_rects();
    // This hand-built fixture must carry the same exact payload identity the
    // recorder now requires; a missing identity is deliberately rejected.
    let identities = rects.each_ref().map(|params| {
        PaintPayloadIdentity::prepared_shadows_with_decoration(
            std::iter::empty(),
            [&DrawRectOp {
                params: params.clone(),
                mode: crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
            }],
        )
        .expect("canonical root-group rectangle")
    });
    PaintArtifact {
        target: PaintArtifactTarget::RootOpacityGroup { root, effect },
        chunks: vec![
            PaintChunk {
                frame: crate::view::paint::PaintChunkFrame::Viewport,
                id: PaintChunkId {
                    owner: first,
                    scope: crate::view::paint::PaintPropertyScope::SelfPaint,
                    phase: crate::view::paint::PaintNodePhase::BeforeChildren,
                    slot: 0,
                    role: PaintChunkRole::SelfDecoration,
                },
                owner: first,
                op_range: 0..1,
                bounds: Rect {
                    x: rects[0].position[0],
                    y: rects[0].position[1],
                    width: rects[0].size[0],
                    height: rects[0].size[1],
                },
                properties,
                content_revision: revision,
                payload_identity: identities[0].clone(),
            },
            PaintChunk {
                frame: crate::view::paint::PaintChunkFrame::Viewport,
                id: PaintChunkId {
                    owner: second,
                    scope: crate::view::paint::PaintPropertyScope::SelfPaint,
                    phase: crate::view::paint::PaintNodePhase::BeforeChildren,
                    slot: 0,
                    role: PaintChunkRole::SelfDecoration,
                },
                owner: second,
                op_range: 1..2,
                bounds: Rect {
                    x: rects[1].position[0],
                    y: rects[1].position[1],
                    width: rects[1].size[0],
                    height: rects[1].size[1],
                },
                properties,
                content_revision: revision,
                payload_identity: identities[1].clone(),
            },
        ]
        .into(),
        ops: rects
            .into_iter()
            .map(|params| {
                PaintOp::DrawRect(DrawRectOp {
                    params,
                    mode: crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
                })
            })
            .collect(),
        clip_nodes: Vec::new(),
        effect_nodes: vec![EffectNodeSnapshot {
            id: effect,
            owner: root,
            parent: None,
            opacity,
            generation: 1,
        }],
        transform_nodes: Vec::new(),
        layout_position_nodes: Vec::new(),
        visual_offset_nodes: Vec::new(),
        scroll_nodes: Vec::new(),
        owner_property_states: Vec::new(),
        owner_nodes: vec![
            PaintOwnerSnapshot {
                owner: root,
                parent: None,
                snap: crate::view::paint::PaintOwnerSnap::INHERIT,
            },
            PaintOwnerSnapshot {
                owner: first,
                parent: Some(root),
                snap: crate::view::paint::PaintOwnerSnap::INHERIT,
            },
            PaintOwnerSnapshot {
                owner: second,
                parent: Some(root),
                snap: crate::view::paint::PaintOwnerSnap::INHERIT,
            },
        ],
    }
}

fn artifact_root_group_overlap_graph(opacity: f32) -> Result<FrameGraph, String> {
    let artifact = root_group_overlap_artifact(opacity);
    let (mut graph, ctx, target) = graph_prelude();
    try_compile_artifact(&artifact, &mut graph, ctx).map_err(|error| {
        format!(
            "root opacity group artifact failed validation: {:?}",
            error.kind()
        )
    })?;
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn explicit_root_group_overlap_graph(opacity: f32) -> Result<FrameGraph, String> {
    use crate::view::render_pass::composite_layer_pass::{
        CompositeLayerInput, CompositeLayerOutput, CompositeLayerParams, CompositeLayerPass,
        LayerIn,
    };

    let (mut graph, mut ctx, parent_target) = graph_prelude();
    let mut layer_ctx = UiBuildContext::from_parts(
        ctx.viewport(),
        ctx.layer_subtree_state_with_ancestor_clip(
            crate::view::base_component::AncestorClipContext::default(),
        ),
    );
    let layer_target = layer_ctx.allocate_target(&mut graph);
    layer_ctx.set_current_target(layer_target);
    graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new([0.0, 0.0, 0.0, 0.0]),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: layer_ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: layer_target,
        },
    ));
    for params in root_group_overlap_rects() {
        let mut pass =
            DrawRectPass::new(params, DrawRectInput::default(), DrawRectOutput::default());
        pass.set_render_mode(crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly);
        layer_ctx.emit_draw_rect_pass(&mut graph, pass);
    }
    let layer_state = layer_ctx.into_state();
    ctx.merge_child_render_state(&layer_state);
    ctx.set_current_target(parent_target);
    graph.add_graphics_pass(CompositeLayerPass::new(
        CompositeLayerParams {
            rect_pos: [0.0, 0.0],
            rect_size: [WIDTH as f32, HEIGHT as f32],
            corner_radii: [0.0; 4],
            opacity,
            scissor_rect: None,
            clear_target: false,
        },
        CompositeLayerInput {
            layer: LayerIn::with_handle(
                layer_target
                    .handle()
                    .expect("explicit group layer target must have a texture handle"),
            ),
            pass_context: ctx.graphics_pass_context(),
            source_physical_origin: None,
        },
        CompositeLayerOutput {
            render_target: parent_target,
        },
    ));
    ctx.set_current_target(parent_target);
    add_present(&mut graph, &parent_target)?;
    Ok(graph)
}

fn artifact_outer_shadow_graph(opacity: f32) -> Result<FrameGraph, String> {
    let shadow_color = Color::rgba(51, 102, 204, 153);
    let (arena, root, properties, generations) = prepared_shadow_leaf(
        0x6d70,
        opacity,
        vec![BoxShadow::new().color(shadow_color).offset_x(-4.0)],
        false,
    );
    let artifact = if opacity.to_bits() == 1.0_f32.to_bits() {
        whole_frame_artifact(&arena, &[root], &properties, &generations).0
    } else {
        root_group_artifact(&arena, &[root], &properties, &generations).0
    };
    drop(arena);
    let (mut graph, ctx, target) = graph_prelude();
    try_compile_artifact(&artifact, &mut graph, ctx).map_err(|error| {
        format!(
            "outer shadow artifact failed validation: {:?}",
            error.kind()
        )
    })?;
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn outer_shadow_anchor_oracle(opacity: f32) -> [u8; 4] {
    // Fixture colors are sRGB bytes. FORMAT is linear Rgba8Unorm, and
    // PresentSurface divides its quantized premultiplied RGB by quantized
    // alpha. Compute the conversion independently of ColorLike and readback.
    let linear = [51.0_f32, 102.0, 204.0].map(|byte| {
        let encoded = byte / 255.0;
        if encoded <= 0.04045 {
            encoded / 12.92
        } else {
            ((encoded + 0.055) / 1.055).powf(2.4)
        }
    });
    let alpha = (153.0 / 255.0) * opacity;
    let quantized_alpha = (alpha * 255.0).round();
    if quantized_alpha == 0.0 {
        return [0; 4];
    }
    let rgb = linear.map(|channel| {
        let quantized_premultiplied = (channel * alpha * 255.0).round();
        (quantized_premultiplied / quantized_alpha * 255.0).round() as u8
    });
    [rgb[0], rgb[1], rgb[2], quantized_alpha as u8]
}

fn legacy_outer_shadow_graph(opacity: f32) -> Result<FrameGraph, String> {
    let (mut arena, root, _, _) = prepared_shadow_leaf(
        0x6d70,
        opacity,
        vec![
            BoxShadow::new()
                .color(Color::rgba(51, 102, 204, 153))
                .offset_x(-4.0),
        ],
        false,
    );
    let (mut graph, ctx, target) = graph_prelude();
    arena
        .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
        .ok_or_else(|| "legacy shadow root disappeared".to_string())?;
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn artifact_image_graph(
    pixels: Arc<[u8]>,
    fit: crate::view::ImageFit,
    sampling: crate::view::ImageSampling,
    opacity: f32,
    decorated: bool,
) -> Result<FrameGraph, String> {
    let (arena, roots) = if decorated {
        prepared_image_fixture(pixels, fit, sampling, opacity)
    } else {
        bare_image_fixture(pixels, fit, sampling, opacity)
    };
    let (properties, generations) = sync_identity(&arena, &roots);
    // Overlapping decoration and image require a group contract. The flat
    // WholeFrame target intentionally bakes opacity per op and is not a
    // correct pixel oracle for this scene (it used to match Legacy's bug).
    let (artifact, eligibility) = if decorated && opacity < 1.0 {
        root_group_artifact(&arena, &roots, &properties, &generations)
    } else {
        whole_frame_artifact(&arena, &roots, &properties, &generations)
    };
    if !eligibility.eligible {
        return Err(format!(
            "image fixture is not artifact eligible: {eligibility:?}"
        ));
    }
    let image_asset_id = artifact.ops.iter().find_map(|op| match op {
        PaintOp::PreparedImage(prepared) => match prepared.upload.id {
            crate::view::sampled_texture::SampledTextureId::Image(asset_id) => Some(asset_id),
            crate::view::sampled_texture::SampledTextureId::SvgRaster(_) => None,
        },
        PaintOp::DrawRect(_)
        | PaintOp::PreparedInlineIfcDecoration(_)
        | PaintOp::PreparedShadow(_)
        | PaintOp::PreparedScrollbarOverlay(_)
        | PaintOp::PreparedText(_)
        | PaintOp::PreparedSvg(_)
        | PaintOp::PreparedGpu(_) => None,
    });
    drop(arena);
    if let Some(asset_id) = image_asset_id {
        crate::view::image_resource::remove_image_entry_for_test(asset_id);
    }
    let (mut graph, ctx, target) = graph_prelude();
    let _ = compile_artifact(&artifact, &mut graph, ctx);
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn legacy_image_graph(
    pixels: Arc<[u8]>,
    fit: crate::view::ImageFit,
    sampling: crate::view::ImageSampling,
    opacity: f32,
    decorated: bool,
) -> Result<FrameGraph, String> {
    let (mut arena, roots) = if decorated {
        prepared_image_fixture(pixels, fit, sampling, opacity)
    } else {
        bare_image_fixture(pixels, fit, sampling, opacity)
    };
    let (mut graph, mut ctx, target) = graph_prelude();
    for root in roots {
        let child_ctx = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next = arena
            .with_element_taken(root, |element, arena| {
                element.build(&mut graph, arena, child_ctx)
            })
            .ok_or_else(|| "legacy image root disappeared".to_string())?;
        ctx.set_state(next);
    }
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn artifact_self_clip_graph() -> Result<FrameGraph, String> {
    let (arena, roots) = self_clip_fixture();
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) = whole_frame_artifact(&arena, &roots, &properties, &generations);
    if !eligibility.eligible {
        return Err(format!(
            "self-clip pixel fixture is not artifact eligible: {eligibility:?}"
        ));
    }
    drop(arena);
    let (mut graph, ctx, target) = self_clip_graph_prelude();
    let _ = compile_artifact(&artifact, &mut graph, ctx);
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn zero_surface_v2_self_clip_graph() -> Result<FrameGraph, String> {
    let (arena, roots) = self_clip_fixture();
    let (properties, generations) = sync_identity(&arena, &roots);
    let (mut graph, ctx, target) = self_clip_graph_prelude();
    let mut viewport = Viewport::new();
    let emission = emit_retained_auto_artifact_surface_for_test(
        &mut viewport,
        &arena,
        &roots,
        &properties,
        &generations,
        &mut graph,
        &ctx,
    )?;
    if emission.surface_count != 0
        || emission.aggregate_texture_bytes != 0
        || !emission.actions.is_empty()
    {
        return Err(format!(
            "production self-clip zero-surface gate must stage no residents: surfaces={}, bytes={}, actions={:?}",
            emission.surface_count, emission.aggregate_texture_bytes, emission.actions,
        ));
    }
    if !viewport.finish_retained_surface_transaction_for_frame(Some(emission.frame_owner), true) {
        return Err(
            "production self-clip zero-surface transaction owner was not current".to_owned(),
        );
    }
    drop(arena);
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn padded_bytes_per_row(width: u32) -> u32 {
    let unpadded = width.saturating_mul(BYTES_PER_PIXEL);
    unpadded.div_ceil(COPY_BYTES_PER_ROW_ALIGNMENT) * COPY_BYTES_PER_ROW_ALIGNMENT
}

fn remove_row_padding(
    mapped: &[u8],
    width: u32,
    height: u32,
    padded_bytes_per_row: u32,
) -> Result<Vec<u8>, String> {
    let row_bytes = width.saturating_mul(BYTES_PER_PIXEL) as usize;
    let padded = padded_bytes_per_row as usize;
    if padded < row_bytes {
        return Err(format!(
            "padded row is smaller than pixel payload: padded={padded}, payload={row_bytes}"
        ));
    }
    let required = padded.saturating_mul(height as usize);
    if mapped.len() < required {
        return Err(format!(
            "mapped readback is too small: mapped={}, required={required}",
            mapped.len()
        ));
    }
    let mut pixels = Vec::with_capacity(row_bytes.saturating_mul(height as usize));
    for row in 0..height as usize {
        let start = row * padded;
        pixels.extend_from_slice(&mapped[start..start + row_bytes]);
    }
    Ok(pixels)
}

fn render(graph: FrameGraph, gpu: &NativeGpu) -> Result<Vec<u8>, String> {
    render_with_config(graph, gpu, 1.0, FORMAT)
}

fn render_with_config(
    graph: FrameGraph,
    gpu: &NativeGpu,
    scale_factor: f32,
    format: wgpu::TextureFormat,
) -> Result<Vec<u8>, String> {
    let mut viewport = Viewport::new();
    render_on_viewport(graph, gpu, &mut viewport, scale_factor, format)
}

fn render_on_viewport(
    graph: FrameGraph,
    gpu: &NativeGpu,
    viewport: &mut Viewport,
    scale_factor: f32,
    format: wgpu::TextureFormat,
) -> Result<Vec<u8>, String> {
    render_on_viewport_with_size(graph, gpu, viewport, scale_factor, format, [WIDTH, HEIGHT])
}

fn render_on_viewport_with_size(
    mut graph: FrameGraph,
    gpu: &NativeGpu,
    viewport: &mut Viewport,
    scale_factor: f32,
    format: wgpu::TextureFormat,
    [width, height]: [u32; 2],
) -> Result<Vec<u8>, String> {
    let padded_bytes_per_row = padded_bytes_per_row(width);
    let buffer_size = padded_bytes_per_row as u64 * height as u64;
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rfgui pixel parity readback"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    viewport.begin_offscreen_test_frame(
        gpu.device.clone(),
        gpu.queue.clone(),
        width,
        height,
        format,
    )?;
    viewport.set_scale_factor(scale_factor);
    graph
        .compile_with_upload(viewport)
        .map_err(|error| format!("pixel graph compile failed: {error:?}"))?;
    graph
        .execute_profiled(viewport, false)
        .map_err(|error| format!("pixel graph execute failed: {error:?}"))?;
    viewport.encode_offscreen_test_readback(&readback, padded_bytes_per_row, width, height)?;
    viewport.end_offscreen_test_frame()?;

    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| format!("GPU wait failed during pixel readback: {error:?}"))?;
    receiver
        .recv()
        .map_err(|error| format!("pixel readback callback was lost: {error}"))?
        .map_err(|error| format!("pixel readback map failed: {error:?}"))?;

    let mapped = readback
        .slice(..)
        .get_mapped_range()
        .map_err(|error| format!("failed to access mapped pixel buffer: {error:?}"))?;
    let pixels = remove_row_padding(&mapped, width, height, padded_bytes_per_row)?;
    drop(mapped);
    readback.unmap();
    Ok(pixels)
}

fn direct_sampled_image_graph(
    upload: crate::view::sampled_texture::SampledTextureUpload,
    params: crate::view::render_pass::texture_composite_pass::TextureCompositeParams,
    format: wgpu::TextureFormat,
    force_transient_geometry: bool,
) -> Result<FrameGraph, String> {
    let (mut graph, ctx, target) = graph_prelude_with_format(format);
    let mut pass = crate::view::render_pass::TextureCompositePass::new(
        params,
        crate::view::render_pass::texture_composite_pass::TextureCompositeInput::from_sampled_texture(
            upload,
            Default::default(),
            ctx.graphics_pass_context(),
        ),
        crate::view::render_pass::texture_composite_pass::TextureCompositeOutput {
            render_target: target.clone(),
        },
    );
    if force_transient_geometry {
        pass.force_transient_geometry_fallback_for_test();
    }
    graph.add_graphics_pass(pass);
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn pixel_at(pixels: &[u8], x: u32, y: u32) -> Result<[u8; 4], String> {
    if x >= WIDTH || y >= HEIGHT {
        return Err(format!("pixel coordinate is outside output: ({x},{y})"));
    }
    let offset = ((y * WIDTH + x) * BYTES_PER_PIXEL) as usize;
    let slice = pixels
        .get(offset..offset + BYTES_PER_PIXEL as usize)
        .ok_or_else(|| format!("pixel buffer is truncated at ({x},{y})"))?;
    Ok([slice[0], slice[1], slice[2], slice[3]])
}

fn rgba8_unorm(color: Color) -> [u8; 4] {
    color
        .to_rgba_f32()
        .map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn assert_pixel_near(
    pixels: &[u8],
    x: u32,
    y: u32,
    expected: [u8; 4],
    tolerance: u8,
    case: &str,
) -> Result<(), String> {
    let actual = pixel_at(pixels, x, y)?;
    if actual
        .iter()
        .zip(expected)
        .any(|(actual, expected)| actual.abs_diff(expected) > tolerance)
    {
        return Err(format!(
            "{case} oracle failed at ({x},{y}): actual={actual:?}, expected={expected:?}, tolerance={tolerance}"
        ));
    }
    Ok(())
}

fn solid_upload(
    id: crate::view::sampled_texture::SampledTextureId,
    generation: u64,
    rgba: [u8; 4],
) -> crate::view::sampled_texture::SampledTextureUpload {
    let mut pixels = Vec::with_capacity(16);
    for _ in 0..4 {
        pixels.extend_from_slice(&rgba);
    }
    crate::view::sampled_texture::SampledTextureUpload {
        id,
        generation,
        width: 2,
        height: 2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        alpha_mode: crate::view::sampled_texture::SampledTextureAlphaMode::Straight,
        pixels: Arc::from(pixels),
        sampling: crate::view::ImageSampling::Nearest,
    }
}

fn direct_sampled_params(
    bounds: [f32; 4],
) -> crate::view::render_pass::texture_composite_pass::TextureCompositeParams {
    crate::view::render_pass::texture_composite_pass::TextureCompositeParams {
        bounds,
        uv_bounds: Some([0.0, 0.0, 2.0, 2.0]),
        opacity: 1.0,
        ..Default::default()
    }
}

fn srgb_byte_to_linear_surface_byte(channel: u8) -> u8 {
    let encoded = f32::from(channel) / 255.0;
    let linear = if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    };
    (linear * 255.0).round().clamp(0.0, 255.0) as u8
}

fn validate_nearest_fill_image_anchors(
    pixels: &[u8],
    path: &str,
    adapter: &str,
) -> Result<(), String> {
    for (x, y, expected, source) in [
        (5, 5, [255, 0, 0, 255], "top-left opaque red"),
        (40, 5, [0, 255, 0, 128], "top-right half-alpha green"),
        (5, 25, [0, 0, 255, 255], "bottom-left opaque blue"),
        (
            40,
            25,
            [255, 255, 0, 64],
            "bottom-right quarter-alpha yellow",
        ),
    ] {
        let actual = pixel_at(pixels, x, y)?;
        if actual
            .iter()
            .zip(expected)
            .any(|(actual, expected)| actual.abs_diff(expected) > 1)
        {
            return Err(format!(
                "prepared-image/{path} {source} anchor is wrong on {adapter}: actual={actual:?}, expected={expected:?}"
            ));
        }
    }
    Ok(())
}

/// Geometry oracle for an axis-aligned rect on whole pixels: pixels at least
/// one pixel inside `rect` read `inside(x, y)` exactly (`None` skips one), and
/// pixels at least one pixel outside it read transparent. The pixel on each
/// side of an edge is left to coverage and filtering.
fn assert_rect_geometry(
    pixels: &[u8],
    rect: [u32; 4],
    inside: impl Fn(u32, u32) -> Option<[u8; 4]>,
    adapter: &str,
    case: &str,
) -> Result<(), String> {
    let [left, top, width, height] = rect.map(i64::from);
    let (right, bottom) = (left + width, top + height);
    let mut mismatches = Vec::new();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let (px, py) = (i64::from(x), i64::from(y));
            let expected = if px > left && px + 1 < right && py > top && py + 1 < bottom {
                inside(x, y)
            } else if px + 1 < left || px > right || py + 1 < top || py > bottom {
                Some([0, 0, 0, 0])
            } else {
                None
            };
            let Some(expected) = expected else {
                continue;
            };
            let actual = pixel_at(pixels, x, y)?;
            if actual != expected {
                mismatches.push(([x, y], actual, expected));
            }
        }
    }
    match mismatches.first() {
        None => Ok(()),
        Some((at, actual, expected)) => Err(format!(
            "{case}: {} pixels break the {rect:?} geometry on {adapter}, first at {at:?}: actual={actual:?}, expected={expected:?}",
            mismatches.len()
        )),
    }
}

/// The `fixture` rect at [8, 8, 32, 24]: its fill and, when bordered, a
/// four-pixel border; the pixel either side of the border's inner edge is
/// skipped.
fn assert_fixture_geometry(
    pixels: &[u8],
    with_border: bool,
    adapter: &str,
    case: &str,
) -> Result<(), String> {
    let fill = rgba8_unorm(Color::rgb(40, 80, 160));
    let border = rgba8_unorm(Color::rgb(220, 60, 20));
    assert_rect_geometry(
        pixels,
        [8, 8, 32, 24],
        |x, y| {
            if !with_border {
                return Some(fill);
            }
            let inner = |value: u32, start: u32, end: u32| value >= start && value < end;
            if inner(x, 13, 35) && inner(y, 13, 27) {
                Some(fill)
            } else if !inner(x, 11, 37) || !inner(y, 11, 29) {
                Some(border)
            } else {
                None
            }
        },
        adapter,
        case,
    )
}

fn validate_color_anchors(
    pixels: &[u8],
    with_border: bool,
    path: &str,
    adapter: &str,
) -> Result<(), String> {
    let fill = rgba8_unorm(Color::rgb(40, 80, 160));
    let center = pixel_at(pixels, 20, 20)?;
    if center != fill {
        return Err(format!(
            "{path} center fill anchor is wrong on {adapter}: actual={center:?}, expected={fill:?}"
        ));
    }
    if with_border {
        let expected_border = rgba8_unorm(Color::rgb(220, 60, 20));
        let border = pixel_at(pixels, 10, 20)?;
        if border != expected_border {
            return Err(format!(
                "{path} border anchor is wrong on {adapter}: actual={border:?}, expected={expected_border:?}"
            ));
        }
    }
    let outside = pixel_at(pixels, 0, 0)?;
    if outside != [0, 0, 0, 0] {
        return Err(format!(
            "{path} transparent anchor is wrong on {adapter}: actual={outside:?}, expected=[0, 0, 0, 0]"
        ));
    }
    Ok(())
}

#[derive(Default)]
struct PixelDiff {
    mismatched_pixels: usize,
    max_channel_delta: u8,
    bounds: Option<[u32; 4]>,
}

fn compare_pixels(
    legacy: &[u8],
    artifact: &[u8],
    exact_interior: [u32; 4],
    adapter: &str,
    case: &str,
) -> Result<(), String> {
    if legacy.len() != artifact.len() {
        return Err(format!(
            "{case}: pixel buffer lengths differ on {adapter}: legacy={}, artifact={}",
            legacy.len(),
            artifact.len()
        ));
    }
    let [ix, iy, iw, ih] = exact_interior;
    let mut diff = PixelDiff::default();
    for pixel_index in 0..(WIDTH * HEIGHT) as usize {
        let x = pixel_index as u32 % WIDTH;
        let y = pixel_index as u32 / WIDTH;
        let inside = x >= ix && x < ix + iw && y >= iy && y < iy + ih;
        let offset = pixel_index * BYTES_PER_PIXEL as usize;
        let mut pixel_failed = false;
        for channel in 0..BYTES_PER_PIXEL as usize {
            let delta = legacy[offset + channel].abs_diff(artifact[offset + channel]);
            diff.max_channel_delta = diff.max_channel_delta.max(delta);
            if delta > 1 || (inside && delta != 0) {
                pixel_failed = true;
            }
        }
        if !pixel_failed {
            continue;
        }
        diff.mismatched_pixels += 1;
        diff.bounds = Some(match diff.bounds {
            None => [x, y, x, y],
            Some([left, top, right, bottom]) => {
                [left.min(x), top.min(y), right.max(x), bottom.max(y)]
            }
        });
    }
    if diff.mismatched_pixels == 0 {
        return Ok(());
    }
    Err(format!(
        "{case}: legacy/artifact pixel mismatch on {adapter}: mismatched_pixels={}, max_channel_delta={}, bounds={:?}, rule=interior exact and whole-frame delta<=1",
        diff.mismatched_pixels, diff.max_channel_delta, diff.bounds
    ))
}

#[derive(Clone, Copy, Debug)]
struct DirectScrollTransformGpuCase {
    label: &'static str,
    scroll_offset_y: f32,
    translation: [f32; 2],
}

impl DirectScrollTransformGpuCase {
    const BASELINE: Self = Self {
        label: "baseline",
        scroll_offset_y: 8.0,
        translation: [3.0, 0.0],
    };
    const SCROLL_ONLY: Self = Self {
        label: "scroll-only",
        scroll_offset_y: 16.0,
        translation: [3.0, 0.0],
    };
}

const DIRECT_SCROLL_TRANSFORM_SCROLLPORT: [u32; 2] = [48, 40];
const DIRECT_SCROLL_TRANSFORM_CONTENT_HEIGHT: f32 = 120.0;
const DIRECT_SCROLL_TRANSFORM_GRADIENT_TRANSITION_Y: f32 = 24.0;

fn direct_scroll_transform_gpu_fixture(
    case: DirectScrollTransformGpuCase,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    // Mirrors planning_tests::property_scroll_interleave_fixture's exact
    // ScrollTransform topology. The sharp gradient is deliberately stronger
    // than that CPU fixture's uniform fill: a scroll-only composite error must
    // move visible red/blue coverage instead of producing the same pixels.
    let mut root = Element::new_with_id(
        0xb4_3f01,
        0.0,
        0.0,
        DIRECT_SCROLL_TRANSFORM_SCROLLPORT[0] as f32,
        DIRECT_SCROLL_TRANSFORM_SCROLLPORT[1] as f32,
    );
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root.apply_style(root_style);
    root.layout_state.content_size = Size {
        width: DIRECT_SCROLL_TRANSFORM_SCROLLPORT[0] as f32,
        height: DIRECT_SCROLL_TRANSFORM_CONTENT_HEIGHT,
    };
    root.set_scroll_offset((0.0, case.scroll_offset_y));
    root.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));

    let mut content = Element::new_with_id(
        0xb4_3f02,
        0.0,
        -case.scroll_offset_y,
        DIRECT_SCROLL_TRANSFORM_SCROLLPORT[0] as f32,
        DIRECT_SCROLL_TRANSFORM_CONTENT_HEIGHT,
    );
    let transition_percent = DIRECT_SCROLL_TRANSFORM_GRADIENT_TRANSITION_Y
        / DIRECT_SCROLL_TRANSFORM_CONTENT_HEIGHT
        * 100.0;
    let gradient = Gradient::linear(SideOrCorner::Bottom)
        .stop(Color::rgb(224, 36, 28), Some(Length::percent(0.0)))
        .stop(
            Color::rgb(224, 36, 28),
            Some(Length::percent(transition_percent)),
        )
        .stop(
            Color::rgb(24, 72, 224),
            Some(Length::percent(transition_percent)),
        )
        .stop(Color::rgb(24, 72, 224), Some(Length::percent(100.0)))
        .build();
    let mut content_style = Style::new();
    content_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    content_style.set_background_image(gradient);
    content.apply_style(content_style);
    content.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
        case.translation[0],
        case.translation[1],
        0.0,
    ))));
    content.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));

    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(root)));
    let content = arena.insert(Node::new(Box::new(content)));
    arena.set_parent(content, Some(root));
    arena.push_child(root, content);
    arena.refresh_subtree_dirty_cache(root);

    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(
        properties.validation_errors.is_empty(),
        "direct S->T GPU fixture property errors: {:?}",
        properties.validation_errors
    );
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, properties, generations)
}

#[derive(Clone, Copy, Debug)]
struct DirectScrollTransformGradientCoverage {
    red: usize,
    blue: usize,
}

fn direct_scroll_transform_is_red(pixel: [u8; 4]) -> bool {
    pixel[0] > 160 && pixel[1] < 100 && pixel[2] < 100 && pixel[3] > 180
}

fn direct_scroll_transform_is_blue(pixel: [u8; 4]) -> bool {
    pixel[2] > 160 && pixel[0] < 100 && pixel[1] < 130 && pixel[3] > 180
}

fn validate_direct_scroll_transform_gradient_coverage(
    pixels: &[u8],
    case: DirectScrollTransformGpuCase,
    path: &str,
    adapter: &str,
) -> Result<DirectScrollTransformGradientCoverage, String> {
    let transition_y =
        case.translation[1] + DIRECT_SCROLL_TRANSFORM_GRADIENT_TRANSITION_Y - case.scroll_offset_y;
    let red_anchor = (
        (case.translation[0] + 4.0) as u32,
        (transition_y - 4.0).max(1.0) as u32,
    );
    let blue_anchor = (
        (case.translation[0] + 4.0) as u32,
        (transition_y + 4.0).min(DIRECT_SCROLL_TRANSFORM_SCROLLPORT[1] as f32 - 2.0) as u32,
    );
    let red_pixel = pixel_at(pixels, red_anchor.0, red_anchor.1)?;
    let blue_pixel = pixel_at(pixels, blue_anchor.0, blue_anchor.1)?;
    if !direct_scroll_transform_is_red(red_pixel) || !direct_scroll_transform_is_blue(blue_pixel) {
        return Err(format!(
            "direct S->T {} {path} sharp-gradient anchors drifted on {adapter}: red@{red_anchor:?}={red_pixel:?}, blue@{blue_anchor:?}={blue_pixel:?}",
            case.label
        ));
    }

    let mut coverage = DirectScrollTransformGradientCoverage { red: 0, blue: 0 };
    for y in 0..DIRECT_SCROLL_TRANSFORM_SCROLLPORT[1] {
        for x in 0..DIRECT_SCROLL_TRANSFORM_SCROLLPORT[0] {
            let pixel = pixel_at(pixels, x, y)?;
            coverage.red += usize::from(direct_scroll_transform_is_red(pixel));
            coverage.blue += usize::from(direct_scroll_transform_is_blue(pixel));
        }
    }
    if coverage.red < 32 || coverage.blue < 32 {
        return Err(format!(
            "direct S->T {} {path} lost non-uniform gradient coverage on {adapter}: {coverage:?}",
            case.label
        ));
    }
    Ok(coverage)
}

mod buffer_binding_tests;
mod native_pixel_oracle_tests;
mod oracle_tests;
mod rect_instancing_tests;
mod text_buffer_tests;

mod artifact_scroll_content_contract_tests;
mod native_artifact_scroll_content_tests;
mod native_artifact_surface_materialization_tests;
mod native_artifact_surface_tests;

mod native_root_effect_tests;

mod native_svg_pixel_tests;
mod native_transform_surface_tests;

#[derive(Clone, Copy, Debug)]
struct ScrollSceneGpuCase {
    offset_y: f32,
    content_height: f32,

    transition_local_y: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum NestedTextFallbackKind {
    InlineIfcOwned,
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

fn nested_scroll_text_fixture() -> (
    NodeArena,
    NodeKey,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (mut arena, outer, inner, leaf, _properties, _generations) =
        crate::view::paint::planning_tests::nested_scroll_plan_fixture();
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

pub(crate) fn nested_scroll_unready_text_fixture_for_test(
    kind: NestedTextFallbackKind,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let (arena, outer, _inner, leaf, _properties, _generations) = nested_scroll_text_fixture();
    match kind {
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
                    crate::view::node_arena::NodeChange::Mutation,
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

fn scroll_scene_gpu_fixture(
    case: ScrollSceneGpuCase,
    scrollbar: GpuScrollbarCase,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    const ROOT_X: f32 = 8.0;
    const ROOT_Y: f32 = 8.0;
    const SCROLLPORT_WIDTH: f32 = 48.0;
    const SCROLLPORT_HEIGHT: f32 = 40.0;

    let mut root = Element::new_with_id(
        0x5c_1101,
        ROOT_X,
        ROOT_Y,
        SCROLLPORT_WIDTH,
        SCROLLPORT_HEIGHT,
    );
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(crate::style::ScrollDirection::Vertical),
    );
    root_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(12, 18, 28)),
    );
    root.apply_style(root_style);

    let mut child = Element::new_with_id(
        0x5c_1102,
        ROOT_X,
        ROOT_Y - case.offset_y,
        SCROLLPORT_WIDTH,
        case.content_height,
    );
    let transition_percent =
        (case.transition_local_y / case.content_height * 100.0).clamp(0.0, 100.0);
    let sharp_gradient = Gradient::linear(SideOrCorner::Bottom)
        .stop(Color::rgb(224, 36, 28), Some(Length::percent(0.0)))
        .stop(
            Color::rgb(224, 36, 28),
            Some(Length::percent(transition_percent)),
        )
        .stop(
            Color::rgb(24, 72, 224),
            Some(Length::percent(transition_percent)),
        )
        .stop(Color::rgb(24, 72, 224), Some(Length::percent(100.0)))
        .build();
    let mut child_style = Style::new();
    child_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    child_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(224, 36, 28)),
    );
    child_style.set_background_image(sharp_gradient);
    child.apply_style(child_style);

    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(root)));
    let child = arena.insert(Node::new(Box::new(child)));
    arena.set_parent(child, Some(root));
    arena.push_child(root, child);
    {
        let mut root_node = arena.get_mut(root).unwrap();
        let root_element = root_node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap();
        root_element.layout_state.content_size = Size {
            width: SCROLLPORT_WIDTH,
            height: case.content_height,
        };
        root_element.set_scroll_offset((0.0, case.offset_y));
        root_element.set_scrollbar_shadow_blur_radius(3.0);
        match scrollbar {
            GpuScrollbarCase::Hidden => {}
        }
        root_element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
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
        "GPU scroll-scene fixture property errors: {:?}",
        properties.validation_errors
    );
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, properties, generations)
}
