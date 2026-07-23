use super::*;

use crate::style::{ColorLike, ScrollDirection};
use crate::view::base_component::{DirtyPassMask, Size};
use crate::view::frame_graph::ExternalSinkKind;
use crate::view::render_pass::draw_rect_pass::{RenderTargetIn, RenderTargetOut};
use crate::view::render_pass::present_surface_pass::{
    PresentSurfaceInput, PresentSurfaceOutput, PresentSurfaceParams, PresentSurfacePass,
};
use crate::view::viewport::Viewport;

const WIDTH: u32 = 67;
const HEIGHT: u32 = 64;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const BYTES_PER_PIXEL: u32 = 4;
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

fn legacy_graph(with_border: bool) -> Result<FrameGraph, String> {
    let (mut arena, roots) = fixture(with_border);
    let (mut graph, mut ctx, target) = graph_prelude();
    for root in roots {
        let child_ctx = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next = arena
            .with_element_taken(root, |element, arena| {
                element.build(&mut graph, arena, child_ctx)
            })
            .ok_or_else(|| "legacy pixel fixture root disappeared".to_string())?;
        ctx.set_state(next);
    }
    add_present(&mut graph, &target)?;
    Ok(graph)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GpuScrollbarCase {
    Hidden,
    Opaque,
    Translucent,
}

impl GpuScrollbarCase {
    const ALL: [Self; 3] = [Self::Hidden, Self::Opaque, Self::Translucent];
}

#[derive(Clone, Copy, Debug)]
struct ScrollSceneGpuCase {
    name: &'static str,
    offset_y: f32,
    content_height: f32,
    backing: ScrollSceneBackingKind,
    max_dimension_2d: u32,
    transition_local_y: f32,
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
            GpuScrollbarCase::Opaque => {
                root_element.set_hovered(true);
            }
            GpuScrollbarCase::Translucent => {
                root_element.set_hovered(true);
                root_element.set_hovered(false);
                let sampled_at = crate::time::Instant::now();
                let _ = root_element.tick_post_layout_animation_frame(sampled_at);
                let _ = root_element.tick_post_layout_animation_frame(
                    sampled_at + crate::time::Duration::from_millis(1_000),
                );
            }
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

fn legacy_scroll_scene_graph(
    case: ScrollSceneGpuCase,
    scrollbar: GpuScrollbarCase,
) -> Result<FrameGraph, String> {
    let (mut arena, root, _, _) = scroll_scene_gpu_fixture(case, scrollbar);
    let (mut graph, ctx, target) = graph_prelude();
    arena
        .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
        .ok_or_else(|| "legacy scroll-scene root disappeared".to_string())?;
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn retained_scroll_scene_graph(
    viewport: &mut Viewport,
    case: ScrollSceneGpuCase,
    scrollbar: GpuScrollbarCase,
) -> Result<(FrameGraph, ScrollSceneBuildTrace), String> {
    let (arena, root, properties, generations) = scroll_scene_gpu_fixture(case, scrollbar);
    viewport.install_scroll_scene_live_authorities_for_test(properties, generations);
    let (mut graph, ctx, target) = graph_prelude();
    let outcome = build_scroll_scene_from_pool_with_budget_for_test(
        viewport,
        &arena,
        &[root],
        &mut graph,
        ctx,
        case.max_dimension_2d,
        64 * 1024 * 1024,
    )
    .map_err(|error| format!("retained scroll-scene build rejected: {error:?}"))?;
    let (_, trace) = outcome.into_parts();
    add_present(&mut graph, &target)?;
    Ok((graph, trace))
}

fn focused_atomic_projection_scroll_fixture(
    caret_visible: bool,
    preedit: Option<(&str, Option<(usize, usize)>)>,
) -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let width = 60.0;
    let height = 40.0;
    let content_height = 120.0;
    let mut text_area = crate::view::base_component::TextArea::new();
    text_area.content = "before projected after".to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.is_focused = true;
    text_area.caret_visible = caret_visible;
    text_area.cursor_char = if preedit.is_some() { 8 } else { 7 };
    if let Some((preedit, cursor)) = preedit {
        text_area.ime_preedit = preedit.to_string();
        text_area.ime_preedit_cursor = cursor;
        text_area.children_dirty = true;
        text_area.bump_unified_ifc_source_revision();
        text_area.dirty_flags = DirtyFlags::ALL;
    }
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(|render| {
        render.range(7..16, |_text_area| crate::ui::RsxNode::text("projected"));
    }));

    let mut arena = NodeArena::new();
    let text_area = arena.insert(Node::new(Box::new(text_area)));
    arena.with_element_taken(text_area, |element, _| {
        element
            .as_any_mut()
            .downcast_mut::<crate::view::base_component::TextArea>()
            .unwrap()
            .set_self_node_key(text_area);
    });
    crate::view::test_support::measure_and_place(
        &mut arena,
        text_area,
        LayoutConstraints {
            max_width: width,
            max_height: content_height,
            viewport_width: WIDTH as f32,
            viewport_height: HEIGHT as f32,
            percent_base_width: Some(WIDTH as f32),
            percent_base_height: Some(HEIGHT as f32),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: width,
            available_height: content_height,
            viewport_width: WIDTH as f32,
            viewport_height: HEIGHT as f32,
            percent_base_width: Some(WIDTH as f32),
            percent_base_height: Some(HEIGHT as f32),
        },
    );

    let wrapper = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x5c_4301,
        0.0,
        0.0,
        width,
        content_height,
    ))));
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x5c_4300, 0.0, 0.0, width, height,
    ))));
    arena.set_parent(text_area, Some(wrapper));
    arena.set_children(wrapper, vec![text_area]);
    arena.set_parent(wrapper, Some(root));
    arena.set_children(root, vec![wrapper]);
    arena.with_element_taken(text_area, |element, arena| {
        element.place(
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: width,
                available_height: content_height,
                viewport_width: WIDTH as f32,
                viewport_height: HEIGHT as f32,
                percent_base_width: Some(WIDTH as f32),
                percent_base_height: Some(HEIGHT as f32),
            },
            arena,
        );
    });

    let mut wrapper_style = Style::new();
    wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    crate::view::test_support::get_element_mut::<Element>(&arena, wrapper)
        .apply_style(wrapper_style);

    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    {
        let mut root = crate::view::test_support::get_element_mut::<Element>(&arena, root);
        root.apply_style(root_style);
        root.layout_state.content_size = Size {
            width,
            height: content_height,
        };
        root.set_scroll_offset((0.0, 0.0));
        root.clear_local_dirty_flags(DirtyFlags::ALL);
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

    let roots = vec![root];
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    assert!(
        properties.validation_errors.is_empty(),
        "focused atomic projection fixture property errors: {:?}",
        properties.validation_errors
    );
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    assert!(generations.matches_live_snapshot(&arena, &roots, &properties));
    (arena, roots, properties, generations)
}

fn legacy_focused_atomic_projection_scroll_graph(
    caret_visible: bool,
    preedit: Option<(&str, Option<(usize, usize)>)>,
) -> Result<FrameGraph, String> {
    let (mut arena, roots, _, _) = focused_atomic_projection_scroll_fixture(caret_visible, preedit);
    let (mut graph, mut ctx, target) = graph_prelude();
    for root in roots {
        let child_ctx = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next = arena
            .with_element_taken(root, |element, arena| {
                element.build(&mut graph, arena, child_ctx)
            })
            .ok_or_else(|| "legacy focused atomic projection root disappeared".to_string())?;
        ctx.set_state(next);
    }
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn retained_focused_atomic_projection_scroll_graph(
    viewport: &mut Viewport,
    caret_visible: bool,
    preedit: Option<(&str, Option<(usize, usize)>)>,
) -> Result<
    (
        FrameGraph,
        RetainedPropertyScrollSceneBuildTrace,
        crate::view::viewport::RetainedSurfaceFrameStageOwner,
        Vec<ScrollForestResident>,
    ),
    String,
> {
    let (arena, roots, properties, generations) =
        focused_atomic_projection_scroll_fixture(caret_visible, preedit);
    let scene = plan_and_validate_property_scroll_scene(
        &arena,
        &roots,
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        crate::time::Instant::now(),
        FORMAT,
        ScrollSceneSingleTextureBudget::new(8192, 64 * 1024 * 1024)
            .expect("focused atomic projection GPU budget is non-zero"),
    )
    .map_err(|error| format!("focused atomic projection planner rejected: {error:?}"))?;
    if scene.boundary_count() != 1 {
        return Err(format!(
            "focused atomic projection planner returned {} boundaries",
            scene.boundary_count()
        ));
    }
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .ok_or_else(|| "focused atomic projection retained stage is unavailable".to_string())?;
    let mut graph = FrameGraph::new();
    let prepared = prepare_retained_property_scroll_forest_from_pool(
        viewport,
        scene,
        &mut graph,
        UiBuildContext::new(WIDTH, HEIGHT, FORMAT, 1.0),
        [0.0; 4],
        owner,
    )
    .map_err(|error| format!("focused atomic projection prepare rejected: {error:?}"))?;
    let outcome = emit_prepared_retained_property_scroll_forest(prepared);
    let (state, trace) = outcome.into_parts();
    let target = state
        .current_target()
        .ok_or_else(|| "focused atomic projection emitted no root target".to_string())?;
    let residents = scroll_forest_residents(&graph)?;
    add_present(&mut graph, &target)?;
    Ok((graph, trace, owner, residents))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScrollForestContentVersion {
    Baseline,
    FirstRootMutated,
}

const SCROLL_FOREST_MAX_DIMENSION: u32 = 2048;
const SCROLL_FOREST_PAIR_BUDGET_BYTES: u64 = 64 * 1024 * 1024;
const SCROLL_FOREST_ROOT_Y: f32 = 8.0;
const SCROLL_FOREST_ROOT_WIDTH: f32 = 27.0;
const SCROLL_FOREST_ROOT_HEIGHT: f32 = 40.0;
const SCROLL_FOREST_ROOT_X: [f32; 2] = [4.0, 36.0];
const SCROLL_FOREST_OFFSETS: [f32; 2] = [20.0, 1000.0];
const SCROLL_FOREST_CONTENT_HEIGHTS: [f32; 2] = [300.0, 3000.0];
const SCROLL_FOREST_TRANSITIONS: [f32; 2] = [36.0, 1024.0];

fn scroll_forest_gpu_fixture(
    version: ScrollForestContentVersion,
) -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut arena = NodeArena::new();
    let mut roots = Vec::with_capacity(2);
    for ordinal in 0..2 {
        let stable_base = 0x5c_2100 + ordinal as u64 * 0x10;
        let root_x = SCROLL_FOREST_ROOT_X[ordinal];
        let offset_y = SCROLL_FOREST_OFFSETS[ordinal];
        let content_height = SCROLL_FOREST_CONTENT_HEIGHTS[ordinal];
        let transition_percent = SCROLL_FOREST_TRANSITIONS[ordinal] / content_height * 100.0;
        let (host_color, first_color, second_color) = match (ordinal, version) {
            (0, ScrollForestContentVersion::Baseline) => (
                Color::rgb(18, 28, 42),
                Color::rgb(224, 36, 28),
                Color::rgb(30, 196, 72),
            ),
            (0, ScrollForestContentVersion::FirstRootMutated) => (
                Color::rgb(18, 28, 42),
                Color::rgb(208, 36, 196),
                Color::rgb(24, 188, 208),
            ),
            (1, _) => (
                Color::rgb(38, 30, 18),
                Color::rgb(24, 72, 224),
                Color::rgb(224, 188, 24),
            ),
            _ => unreachable!("the forest fixture has exactly two roots"),
        };

        let mut root = Element::new_with_id(
            stable_base,
            root_x,
            SCROLL_FOREST_ROOT_Y,
            SCROLL_FOREST_ROOT_WIDTH,
            SCROLL_FOREST_ROOT_HEIGHT,
        );
        let mut root_style = Style::new();
        root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        root_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        root_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(host_color),
        );
        root.apply_style(root_style);

        let mut content = Element::new_with_id(
            stable_base + 1,
            root_x,
            SCROLL_FOREST_ROOT_Y - offset_y,
            SCROLL_FOREST_ROOT_WIDTH,
            content_height,
        );
        let gradient = Gradient::linear(SideOrCorner::Bottom)
            .stop(first_color.clone(), Some(Length::percent(0.0)))
            .stop(
                first_color.clone(),
                Some(Length::percent(transition_percent)),
            )
            .stop(
                second_color.clone(),
                Some(Length::percent(transition_percent)),
            )
            .stop(second_color, Some(Length::percent(100.0)))
            .build();
        let mut content_style = Style::new();
        content_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        content_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(first_color),
        );
        content_style.set_background_image(gradient);
        content.apply_style(content_style);

        let root = arena.insert(Node::new(Box::new(root)));
        let content = arena.insert(Node::new(Box::new(content)));
        arena.set_parent(content, Some(root));
        arena.push_child(root, content);
        {
            let mut root_node = arena.get_mut(root).unwrap();
            let root_element = root_node
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .unwrap();
            root_element.layout_state.content_size = Size {
                width: SCROLL_FOREST_ROOT_WIDTH,
                height: content_height,
            };
            root_element.set_scroll_offset((0.0, offset_y));
            root_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        arena
            .get_mut(content)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        arena.refresh_subtree_dirty_cache(root);
        roots.push(root);
    }

    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    assert!(
        properties.validation_errors.is_empty(),
        "GPU scroll-forest fixture property errors: {:?}",
        properties.validation_errors
    );
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    assert!(generations.matches_live_snapshot(&arena, &roots, &properties));
    (arena, roots, properties, generations)
}

fn legacy_scroll_forest_graph(version: ScrollForestContentVersion) -> Result<FrameGraph, String> {
    let (mut arena, roots, _, _) = scroll_forest_gpu_fixture(version);
    let (mut graph, mut ctx, target) = graph_prelude();
    for root in roots {
        let child_ctx = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next = arena
            .with_element_taken(root, |element, arena| {
                element.build(&mut graph, arena, child_ctx)
            })
            .ok_or_else(|| "legacy scroll-forest root disappeared".to_string())?;
        ctx.set_state(next);
    }
    add_present(&mut graph, &target)?;
    Ok(graph)
}

type ScrollForestResident = (
    crate::view::frame_graph::PersistentTextureKey,
    crate::view::frame_graph::TextureDesc,
);

fn scroll_forest_residents(graph: &FrameGraph) -> Result<Vec<ScrollForestResident>, String> {
    let declared = graph
        .declared_persistent_textures()
        .map(|(key, desc)| (key, desc.clone()))
        .collect::<Vec<_>>();
    let colors = declared
        .iter()
        .filter(|(key, _)| key.depth_stencil().is_some())
        .cloned()
        .collect::<Vec<_>>();
    if colors.is_empty()
        || colors.len() * 2 != declared.len()
        || colors.iter().any(|(color, _)| {
            color
                .depth_stencil()
                .is_none_or(|depth| !declared.iter().any(|(key, _)| *key == depth))
        })
    {
        return Err(format!(
            "scroll-forest declarations are not complete color/depth pairs: {declared:?}"
        ));
    }
    Ok(colors)
}

fn validate_scroll_forest_resident_topology(
    residents: &[ScrollForestResident],
) -> Result<(), String> {
    let expected = [
        crate::view::base_component::scroll_content_layer_stable_key(0x5c_2101),
        crate::view::base_component::scroll_content_tile_layer_stable_key(0x5c_2111, 0, 0)
            .expect("scroll-forest row-0 tile key is canonical"),
        crate::view::base_component::scroll_content_tile_layer_stable_key(0x5c_2111, 0, 1)
            .expect("scroll-forest row-1 tile key is canonical"),
    ];
    if residents.len() != expected.len()
        || expected
            .iter()
            .any(|expected| !residents.iter().any(|(key, _)| key == expected))
    {
        return Err(format!(
            "scroll-forest resident topology must be one single left-root pair and two row-adjacent right-root tile pairs: expected={expected:?}, actual={residents:?}"
        ));
    }
    Ok(())
}

fn same_scroll_forest_residents(
    left: &[ScrollForestResident],
    right: &[ScrollForestResident],
) -> bool {
    left.len() == right.len() && left.iter().all(|resident| right.contains(resident))
}

fn production_scroll_forest_graph(
    viewport: &mut Viewport,
    version: ScrollForestContentVersion,
    semantic_frame_time: crate::time::Instant,
) -> Result<
    (
        FrameGraph,
        RetainedPropertyScrollSceneBuildTrace,
        crate::view::viewport::RetainedSurfaceFrameStageOwner,
        Vec<ScrollForestResident>,
    ),
    String,
> {
    let (arena, roots, properties, generations) = scroll_forest_gpu_fixture(version);
    let budget = ScrollSceneSingleTextureBudget::new(
        SCROLL_FOREST_MAX_DIMENSION,
        SCROLL_FOREST_PAIR_BUDGET_BYTES,
    )
    .expect("scroll-forest GPU budget is non-zero");
    let scene = plan_and_validate_property_scroll_scene(
        &arena,
        &roots,
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        semantic_frame_time,
        FORMAT,
        budget,
    )
    .map_err(|error| format!("production scroll-forest planner rejected: {error:?}"))?;
    if scene.boundary_count() != 2 {
        return Err(format!(
            "production scroll-forest planner returned {} boundaries",
            scene.boundary_count()
        ));
    }
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .ok_or_else(|| "scroll-forest retained stage is unavailable".to_string())?;
    let mut graph = FrameGraph::new();
    let prepared = prepare_retained_property_scroll_forest_from_pool(
        viewport,
        scene,
        &mut graph,
        UiBuildContext::new(WIDTH, HEIGHT, FORMAT, 1.0),
        [0.0; 4],
        owner,
    )
    .map_err(|error| format!("production scroll-forest preflight rejected: {error:?}"))?;
    let outcome = emit_prepared_retained_property_scroll_forest(prepared);
    let (state, trace) = outcome.into_parts();
    let target = state
        .current_target()
        .ok_or_else(|| "production scroll-forest emitted no root target".to_string())?;
    let residents = scroll_forest_residents(&graph)?;
    add_present(&mut graph, &target)?;
    Ok((graph, trace, owner, residents))
}

fn validate_scroll_forest_graph_shape(
    graph: &FrameGraph,
    trace: &RetainedPropertyScrollSceneBuildTrace,
    expected_reraster: usize,
    expected_reuse: usize,
) -> Result<(), String> {
    if trace.root_count != 2
        || trace.scroll_group_count != 2
        || trace.backing != ScrollSceneBackingKind::Tiled
        || trace.tile_count <= 2
        || trace.reraster_count != expected_reraster
        || trace.reuse_count != expected_reuse
        || expected_reraster + expected_reuse != trace.tile_count
    {
        return Err(format!("scroll-forest trace is not exact: {trace:?}"));
    }
    let clear_count = graph
        .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
        .len();
    let composite_count = graph
        .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
        .len();
    if clear_count != 1 + expected_reraster || composite_count != trace.tile_count {
        return Err(format!(
            "scroll-forest graph shape drifted: clears={clear_count}, composites={composite_count}, trace={trace:?}"
        ));
    }
    let residents = scroll_forest_residents(graph)?;
    validate_scroll_forest_resident_topology(&residents)?;
    if residents.len() != trace.tile_count
        || graph.declared_persistent_texture_keys().count() != trace.tile_count * 2
    {
        return Err(format!(
            "scroll-forest resident declaration count drifted: residents={}, keys={}, trace={trace:?}",
            residents.len(),
            graph.declared_persistent_texture_keys().count()
        ));
    }
    Ok(())
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

fn nested_transformed_rect_fixture(
    parent_translate_x: f32,
    child_translate_y: f32,
) -> (NodeArena, NodeKey) {
    let styled_element = |id, x, y, width, height, color| {
        let mut element = Element::new_with_id(id, x, y, width, height);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
        element.apply_style(style);
        element
    };

    let mut arena = new_test_arena();
    let mut root = styled_element(0xc5_b001, 4.0, 5.0, 42.0, 30.0, Color::rgb(25, 55, 105));
    let mut root_transform = Style::new();
    root_transform.set_transform(Transform::new([Translate::x(Length::px(
        parent_translate_x,
    ))]));
    root.apply_style(root_transform);
    let root = commit_element(&mut arena, Box::new(root));
    commit_child(
        &mut arena,
        root,
        Box::new(styled_element(
            0xc5_b002,
            1.0,
            1.0,
            5.0,
            5.0,
            Color::rgb(35, 175, 80),
        )),
    );
    let mut child = styled_element(0xc5_b003, 7.0, 6.0, 20.0, 14.0, Color::rgb(205, 60, 25));
    let mut child_transform = Style::new();
    child_transform.set_transform(Transform::new([Translate::xy(
        Length::Zero,
        Length::px(child_translate_y),
    )]));
    child.apply_style(child_transform);
    let child = commit_child(&mut arena, root, Box::new(child));
    commit_child(
        &mut arena,
        child,
        Box::new(styled_element(
            0xc5_b004,
            2.0,
            2.0,
            6.0,
            5.0,
            Color::rgb(230, 185, 30),
        )),
    );
    commit_child(
        &mut arena,
        root,
        Box::new(styled_element(
            0xc5_b005,
            29.0,
            20.0,
            7.0,
            6.0,
            Color::rgb(125, 75, 195),
        )),
    );

    let measure = LayoutConstraints {
        max_width: WIDTH as f32,
        max_height: HEIGHT as f32,
        viewport_width: WIDTH as f32,
        viewport_height: HEIGHT as f32,
        percent_base_width: Some(WIDTH as f32),
        percent_base_height: Some(HEIGHT as f32),
    };
    let place = LayoutPlacement {
        parent_x: 4.0,
        parent_y: 5.0,
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
    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(WIDTH, HEIGHT, FORMAT, scale_factor);
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

fn legacy_transformed_rect_graph(
    scale_factor: f32,
    outer_scissor: Option<[u32; 4]>,
) -> Result<FrameGraph, String> {
    let (mut arena, root) = transformed_rect_fixture();
    let (mut graph, ctx, target) = transformed_graph_prelude(scale_factor, outer_scissor);
    arena
        .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
        .ok_or_else(|| "legacy transformed rect root disappeared".to_string())?;
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn forced_transformed_rect_graph(
    scale_factor: f32,
    outer_scissor: Option<[u32; 4]>,
) -> Result<FrameGraph, String> {
    let (arena, root) = transformed_rect_fixture();
    let roots = [root];
    let (properties, generations) = sync_identity(&arena, &roots);
    let plan = plan_single_root_transform_surface_with_context(
        &arena,
        &roots,
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], outer_scissor),
    )
    .map_err(|error| format!("forced transformed rect plan rejected: {error:?}"))?;
    let (mut graph, ctx, target) = transformed_graph_prelude(scale_factor, outer_scissor);
    let mut viewport = Viewport::new();
    execute_forced_transform_surface_for_test(&mut viewport, &plan, &mut graph, ctx)
        .map_err(|error| format!("forced transformed rect execute rejected: {error:?}"))?;
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn legacy_nested_transformed_rect_graph(
    scale_factor: f32,
    outer_scissor: Option<[u32; 4]>,
) -> Result<FrameGraph, String> {
    legacy_nested_transformed_rect_graph_with_transforms(scale_factor, outer_scissor, 7.0, 5.0)
}

fn legacy_nested_transformed_rect_graph_with_transforms(
    scale_factor: f32,
    outer_scissor: Option<[u32; 4]>,
    parent_translate_x: f32,
    child_translate_y: f32,
) -> Result<FrameGraph, String> {
    let (mut arena, root) = nested_transformed_rect_fixture(parent_translate_x, child_translate_y);
    let (mut graph, ctx, target) = transformed_graph_prelude(scale_factor, outer_scissor);
    arena
        .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
        .ok_or_else(|| "legacy nested transformed rect root disappeared".to_string())?;
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn forced_nested_transformed_rect_graph(
    scale_factor: f32,
    outer_scissor: Option<[u32; 4]>,
) -> Result<FrameGraph, String> {
    let mut viewport = Viewport::new();
    forced_nested_transformed_rect_graph_on_viewport(
        &mut viewport,
        scale_factor,
        outer_scissor,
        7.0,
        5.0,
    )
}

fn forced_nested_transformed_rect_graph_on_viewport(
    viewport: &mut Viewport,
    scale_factor: f32,
    outer_scissor: Option<[u32; 4]>,
    parent_translate_x: f32,
    child_translate_y: f32,
) -> Result<FrameGraph, String> {
    let (arena, root) = nested_transformed_rect_fixture(parent_translate_x, child_translate_y);
    let roots = [root];
    let (properties, generations) = sync_identity(&arena, &roots);
    let plan = plan_single_root_transform_surface_with_context(
        &arena,
        &roots,
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], outer_scissor),
    )
    .map_err(|error| format!("forced nested transformed rect plan rejected: {error:?}"))?;
    let (mut graph, ctx, target) = transformed_graph_prelude(scale_factor, outer_scissor);
    execute_forced_transform_surface_for_test(viewport, &plan, &mut graph, ctx)
        .map_err(|error| format!("forced nested transformed rect execute rejected: {error:?}"))?;
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn production_transformed_rect_graph(
    viewport: &mut Viewport,
    scale_factor: f32,
    outer_scissor: Option<[u32; 4]>,
) -> Result<(FrameGraph, RetainedSurfaceBuildTrace), String> {
    let (arena, root) = transformed_rect_fixture();
    let roots = [root];
    let (properties, generations) = sync_identity(&arena, &roots);
    let plan = plan_single_root_transform_surface_with_context(
        &arena,
        &roots,
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], outer_scissor),
    )
    .map_err(|error| format!("production transformed rect plan rejected: {error:?}"))?;
    let (mut graph, ctx, target) = transformed_graph_prelude(scale_factor, outer_scissor);
    let outcome = build_retained_surface_from_pool(viewport, &plan, &mut graph, ctx)
        .map_err(|error| format!("production transformed rect execute rejected: {error:?}"))?;
    let (_, trace) = outcome.into_parts();
    add_present(&mut graph, &target)?;
    Ok((graph, trace))
}

fn production_nested_transformed_rect_graph(
    viewport: &mut Viewport,
    scale_factor: f32,
    outer_scissor: Option<[u32; 4]>,
) -> Result<(FrameGraph, Vec<RetainedSurfaceBuildTrace>), String> {
    let (arena, root) = nested_transformed_rect_fixture(7.0, 5.0);
    let roots = [root];
    let (properties, generations) = sync_identity(&arena, &roots);
    let plan = plan_single_root_transform_surface_with_context(
        &arena,
        &roots,
        &properties,
        &generations,
        TransformSurfacePlanContext::new([0.0, 0.0], outer_scissor),
    )
    .map_err(|error| format!("production nested transform plan rejected: {error:?}"))?;
    let (mut graph, ctx, target) = transformed_graph_prelude(scale_factor, outer_scissor);
    let outcome = build_retained_surface_tree_from_pool(viewport, &plan, &mut graph, ctx)
        .map_err(|error| format!("production nested transform execute rejected: {error:?}"))?;
    let (_, traces) = outcome.into_parts();
    add_present(&mut graph, &target)?;
    Ok((graph, traces))
}

fn production_isolation_graph(
    viewport: &mut Viewport,
    opacity: f32,
) -> Result<(FrameGraph, RetainedSurfaceBuildTrace), String> {
    let (arena, root, properties, generations) = exact_isolation_fixture(opacity);
    let plan = plan_single_root_isolation_surface(
        &arena,
        &[root],
        &properties,
        &generations,
        WIDTH,
        HEIGHT,
        1.0,
        None,
    )
    .map_err(|error| format!("production isolation plan rejected: {error:?}"))?;
    let (mut graph, ctx, target) = transformed_graph_prelude(1.0, None);
    let outcome = build_retained_isolation_surface_from_pool(viewport, &plan, &mut graph, ctx)
        .map_err(|error| format!("production isolation execute rejected: {error:?}"))?;
    let (_, trace) = outcome.into_parts();
    add_present(&mut graph, &target)?;
    Ok((graph, trace))
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
    Image,
    Svg,
    Text,
}

impl NestedScrollGpuLeafKind {
    const GPU_CLOSURE: [Self; 3] = [Self::Image, Self::Svg, Self::Text];

    fn label(self) -> &'static str {
        match self {
            Self::Rect => "rect",
            Self::Image => "image",
            Self::Svg => "svg",
            Self::Text => "text",
        }
    }

    fn expected_cold_composite_count(self) -> usize {
        match self {
            Self::Image | Self::Svg => 3,
            Self::Rect | Self::Text => 2,
        }
    }
}

fn nested_scroll_gpu_image_pixels() -> Arc<[u8]> {
    static PIXELS: std::sync::OnceLock<Arc<[u8]>> = std::sync::OnceLock::new();
    PIXELS
        .get_or_init(|| {
            let mut pixels = Vec::with_capacity(4 * 4 * 4);
            for y in 0..4 {
                for x in 0..4 {
                    let rgba = if (x + y) % 2 == 0 {
                        [232, 48, 28, 255]
                    } else {
                        [248, 168, 24, 255]
                    };
                    pixels.extend_from_slice(&rgba);
                }
            }
            Arc::from(pixels)
        })
        .clone()
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
        NestedScrollGpuLeafKind::Image => {
            let mut image = Image::new_with_id(
                stable_id,
                ImageSource::Rgba {
                    width: 4,
                    height: 4,
                    pixels: nested_scroll_gpu_image_pixels(),
                },
            );
            image.set_fit(crate::view::ImageFit::Fill);
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            style.insert(PropertyId::Width, ParsedValue::Length(Length::px(100.0)));
            style.insert(PropertyId::Height, ParsedValue::Length(Length::px(600.0)));
            image.apply_style(style);
            Box::new(image)
        }
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
        NestedScrollGpuLeafKind::Text => {
            let mut text = Text::new_with_id(stable_id, 0.0, 0.0, 100.0, 600.0, "R1");
            text.set_font("sans-serif");
            text.set_font_size(24.0);
            text.set_color(Color::rgb(248, 224, 32));
            text.set_opacity(1.0);
            Box::new(text)
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

fn legacy_nested_scroll_graph(
    outer_offset_y: f32,
    inner_offset_y: f32,
    outer_scissor: Option<[u32; 4]>,
) -> Result<FrameGraph, String> {
    legacy_nested_scroll_leaf_graph(
        NestedScrollGpuLeafKind::Rect,
        outer_offset_y,
        inner_offset_y,
        outer_scissor,
    )
}

fn legacy_nested_scroll_leaf_graph(
    kind: NestedScrollGpuLeafKind,
    outer_offset_y: f32,
    inner_offset_y: f32,
    outer_scissor: Option<[u32; 4]>,
) -> Result<FrameGraph, String> {
    let (mut arena, outer, _, _) =
        nested_scroll_gpu_leaf_fixture(kind, outer_offset_y, inner_offset_y);
    let (mut graph, ctx, target) = transformed_graph_prelude(1.0, outer_scissor);
    arena
        .with_element_taken(outer, |element, arena| {
            element.build(&mut graph, arena, ctx)
        })
        .ok_or_else(|| "legacy nested-scroll root disappeared".to_string())?;
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn production_nested_scroll_graph(
    viewport: &mut Viewport,
    outer_offset_y: f32,
    inner_offset_y: f32,
    outer_scissor: Option<[u32; 4]>,
) -> Result<
    (
        FrameGraph,
        RetainedPropertyScrollSceneBuildTrace,
        crate::view::viewport::RetainedSurfaceFrameStageOwner,
        crate::view::frame_graph::PersistentTextureKey,
        crate::view::frame_graph::TextureDesc,
    ),
    String,
> {
    production_nested_scroll_leaf_graph(
        viewport,
        NestedScrollGpuLeafKind::Rect,
        outer_offset_y,
        inner_offset_y,
        outer_scissor,
    )
}

fn production_nested_scroll_leaf_graph(
    viewport: &mut Viewport,
    kind: NestedScrollGpuLeafKind,
    outer_offset_y: f32,
    inner_offset_y: f32,
    outer_scissor: Option<[u32; 4]>,
) -> Result<
    (
        FrameGraph,
        RetainedPropertyScrollSceneBuildTrace,
        crate::view::viewport::RetainedSurfaceFrameStageOwner,
        crate::view::frame_graph::PersistentTextureKey,
        crate::view::frame_graph::TextureDesc,
    ),
    String,
> {
    let (arena, outer, properties, generations) =
        nested_scroll_gpu_leaf_fixture(kind, outer_offset_y, inner_offset_y);
    let mut ctx = UiBuildContext::new(WIDTH, HEIGHT, FORMAT, 1.0);
    ctx.push_scissor_rect(outer_scissor);
    let geometry = plan_and_prepare_nested_scroll_scene(
        &arena,
        &[outer],
        &properties,
        &generations,
        1.0,
        ctx.paint_offset(),
        ctx.graphics_pass_context().scissor_rect,
        FORMAT,
        ScrollSceneSingleTextureBudget::new(
            wgpu::Limits::default().max_texture_dimension_2d,
            128 * 1024 * 1024,
        )
        .expect("native nested-scroll budget is non-zero"),
    )
    .map_err(|error| {
        format!(
            "nested-scroll {} production wrapper rejected: {error:?}",
            kind.label()
        )
    })?;
    let (leaf_key, leaf_desc) = geometry.leaf_target_for_test();
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .ok_or_else(|| "nested-scroll retained stage is unavailable".to_string())?;
    let mut graph = FrameGraph::new();
    let prepared = prepare_nested_scroll_scene_from_pool(
        viewport,
        geometry,
        &mut graph,
        ctx,
        [0.0, 0.0, 0.0, 0.0],
        owner,
    )
    .map_err(|error| {
        format!(
            "nested-scroll {} production preflight rejected: {error:?}",
            kind.label()
        )
    })?;
    let outcome = emit_prepared_nested_scroll_scene(prepared);
    let (state, trace) = outcome.into_parts();
    let target = state
        .current_target()
        .ok_or_else(|| "nested-scroll emission did not produce a root target".to_string())?;
    let depth_key = leaf_key
        .depth_stencil()
        .ok_or_else(|| "nested-scroll R1 key has no depth pair".to_string())?;
    let declared = graph
        .declared_persistent_texture_keys()
        .collect::<rustc_hash::FxHashSet<_>>();
    let expected = [leaf_key, depth_key]
        .into_iter()
        .collect::<rustc_hash::FxHashSet<_>>();
    if declared != expected {
        return Err(format!(
            "nested-scroll must persist only R1 color/depth; A0 must remain transient: declared={declared:?} expected={expected:?}"
        ));
    }
    add_present(&mut graph, &target)?;
    Ok((graph, trace, owner, leaf_key, leaf_desc))
}

fn validate_nested_scroll_leaf_graph_shape(
    graph: &FrameGraph,
    kind: NestedScrollGpuLeafKind,
    cold: bool,
) -> Result<(), String> {
    let clear_count = graph
        .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
        .len();
    let expected_clears = if cold { 3 } else { 2 };
    if clear_count != expected_clears {
        return Err(format!(
            "nested-scroll {} {} graph clears={clear_count}, expected={expected_clears}",
            kind.label(),
            if cold { "cold" } else { "warm" },
        ));
    }
    let composite_count = graph
        .test_graphics_passes::<crate::view::render_pass::texture_composite_pass::TextureCompositePass>(
        )
        .len();
    let expected_composites = if cold {
        kind.expected_cold_composite_count()
    } else {
        2
    };
    if composite_count != expected_composites {
        return Err(format!(
            "nested-scroll {} {} graph composites={composite_count}, expected={expected_composites}",
            kind.label(),
            if cold { "cold" } else { "warm" },
        ));
    }
    let text_count = graph
        .test_graphics_passes::<crate::view::render_pass::text_pass::TextPreparedInputPass>()
        .len();
    let expected_text = usize::from(cold && kind == NestedScrollGpuLeafKind::Text);
    if text_count != expected_text {
        return Err(format!(
            "nested-scroll {} {} graph text passes={text_count}, expected={expected_text}",
            kind.label(),
            if cold { "cold" } else { "warm" },
        ));
    }
    Ok(())
}

fn validate_nested_scroll_legacy_leaf_graph_shape(
    graph: &FrameGraph,
    kind: NestedScrollGpuLeafKind,
) -> Result<(), String> {
    let composites = graph
        .test_graphics_passes::<crate::view::render_pass::texture_composite_pass::TextureCompositePass>(
        )
        .len();
    let text = graph
        .test_graphics_passes::<crate::view::render_pass::text_pass::TextPreparedInputPass>()
        .len();
    let valid = match kind {
        NestedScrollGpuLeafKind::Image | NestedScrollGpuLeafKind::Svg => {
            composites == 1 && text == 0
        }
        NestedScrollGpuLeafKind::Text => composites == 0 && text == 1,
        NestedScrollGpuLeafKind::Rect => composites == 0 && text == 0,
    };
    valid.then_some(()).ok_or_else(|| {
        format!(
            "legacy nested-scroll {} leaf passes are not exact: composites={composites}, text={text}",
            kind.label()
        )
    })
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

fn root_group_anchor_oracle(opacity: f32) -> [[u8; 4]; 3] {
    let first = premultiply(ROOT_GROUP_FIRST_COLOR);
    let second = premultiply(ROOT_GROUP_SECOND_COLOR);
    [
        premultiplied_to_readback_rgba8(scale_premultiplied(first, opacity)),
        premultiplied_to_readback_rgba8(scale_premultiplied(source_over(second, first), opacity)),
        premultiplied_to_readback_rgba8(scale_premultiplied(second, opacity)),
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
    PaintArtifact {
        target: PaintArtifactTarget::RootOpacityGroup { root, effect },
        chunks: vec![
            PaintChunk {
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
                payload_identity: PaintPayloadIdentity::None,
            },
            PaintChunk {
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
                payload_identity: PaintPayloadIdentity::None,
            },
        ],
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
        owner_nodes: vec![
            PaintOwnerSnapshot {
                owner: root,
                parent: None,
            },
            PaintOwnerSnapshot {
                owner: first,
                parent: Some(root),
            },
            PaintOwnerSnapshot {
                owner: second,
                parent: Some(root),
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

fn retained_root_effect_witness(
    artifact: &PaintArtifact,
) -> Result<
    (
        RootEffectRasterStamp,
        crate::view::frame_graph::PersistentTextureKey,
        crate::view::frame_graph::texture_resource::TextureDesc,
    ),
    String,
> {
    let PaintArtifactTarget::RootOpacityGroup { root, .. } = artifact.target else {
        return Err("retained root-effect fixture must target a root opacity group".to_string());
    };
    let key = crate::view::base_component::root_effect_stable_key(root);
    let ctx = UiBuildContext::new(WIDTH, HEIGHT, FORMAT, 1.0);
    let color_desc = ctx.persistent_full_viewport_target_desc(key);
    let stamp = validated_root_effect_raster_stamp(
        artifact,
        RootEffectRasterInputs {
            width: color_desc.width(),
            height: color_desc.height(),
            format: color_desc.format(),
            sample_count: color_desc.sample_count(),
            scale_factor_bits: 1.0_f32.to_bits(),
        },
    )
    .ok_or_else(|| "retained root-effect fixture failed strict stamp validation".to_string())?;
    Ok((stamp, key, color_desc))
}

fn retained_root_group_graph(
    artifact: &PaintArtifact,
    action: RootEffectCompileAction,
) -> Result<FrameGraph, String> {
    let (mut graph, ctx, target) = graph_prelude();
    try_compile_root_effect_artifact(artifact, action, &mut graph, ctx).map_err(|error| {
        format!(
            "retained root opacity group artifact failed validation: {:?}",
            error.kind()
        )
    })?;
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn assert_retained_root_effect_graph_shape(
    graph: &FrameGraph,
    expected_clear_count: usize,
    expected_raster_count: usize,
    case: &str,
) -> Result<(), String> {
    let clear_count = graph
        .test_graphics_passes::<crate::view::render_pass::ClearPass>()
        .len();
    let raster_count = graph.test_graphics_passes::<DrawRectPass>().len();
    let composite_count = graph
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>(
        )
        .len();
    if clear_count != expected_clear_count
        || raster_count != expected_raster_count
        || composite_count != 1
    {
        return Err(format!(
            "{case}: unexpected retained root-effect graph shape: clears={clear_count} (expected {expected_clear_count}), raster_rects={raster_count} (expected {expected_raster_count}), composites={composite_count} (expected 1)"
        ));
    }
    Ok(())
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
    premultiplied_to_readback_rgba8(scale_premultiplied(
        premultiply([51.0 / 255.0, 102.0 / 255.0, 204.0 / 255.0, 153.0 / 255.0]),
        opacity,
    ))
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
    let (artifact, eligibility) = whole_frame_artifact(&arena, &roots, &properties, &generations);
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
        | PaintOp::PreparedSvg(_) => None,
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

fn legacy_self_clip_graph() -> Result<FrameGraph, String> {
    let (mut arena, roots) = self_clip_fixture();
    let (mut graph, mut ctx, target) = self_clip_graph_prelude();
    for root in roots {
        let child_ctx = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next = arena
            .with_element_taken(root, |element, arena| {
                element.build(&mut graph, arena, child_ctx)
            })
            .ok_or_else(|| "legacy self-clip pixel fixture root disappeared".to_string())?;
        ctx.set_state(next);
    }
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
    mut graph: FrameGraph,
    gpu: &NativeGpu,
    viewport: &mut Viewport,
    scale_factor: f32,
    format: wgpu::TextureFormat,
) -> Result<Vec<u8>, String> {
    let padded_bytes_per_row = padded_bytes_per_row(WIDTH);
    let buffer_size = padded_bytes_per_row as u64 * HEIGHT as u64;
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rfgui pixel parity readback"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    viewport.begin_offscreen_test_frame(
        gpu.device.clone(),
        gpu.queue.clone(),
        WIDTH,
        HEIGHT,
        format,
    )?;
    viewport.set_scale_factor(scale_factor);
    graph
        .compile_with_upload(viewport)
        .map_err(|error| format!("pixel graph compile failed: {error:?}"))?;
    graph
        .execute_profiled(viewport, false)
        .map_err(|error| format!("pixel graph execute failed: {error:?}"))?;
    viewport.encode_offscreen_test_readback(&readback, padded_bytes_per_row, WIDTH, HEIGHT)?;
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
    let pixels = remove_row_padding(&mapped, WIDTH, HEIGHT, padded_bytes_per_row)?;
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

fn validate_nested_scroll_leaf_anchor(
    pixels: &[u8],
    kind: NestedScrollGpuLeafKind,
) -> Result<(), String> {
    let predicate = |pixel: [u8; 4]| match kind {
        NestedScrollGpuLeafKind::Image => pixel[0] > 180 && pixel[1] > 30 && pixel[2] < 80,
        NestedScrollGpuLeafKind::Svg => {
            (pixel[1] > 140 && pixel[0] < 100) || (pixel[2] > 150 && pixel[0] < 100)
        }
        NestedScrollGpuLeafKind::Text => pixel[0] > 120 && pixel[1] > 100 && pixel[2] < 100,
        NestedScrollGpuLeafKind::Rect => pixel == [24, 48, 72, 255],
    };
    let mut count = 0usize;
    let mut bounds = [u32::MAX, u32::MAX, 0, 0];
    for y in 20..HEIGHT {
        for x in 10..WIDTH {
            if predicate(pixel_at(pixels, x, y)?) {
                count += 1;
                bounds[0] = bounds[0].min(x);
                bounds[1] = bounds[1].min(y);
                bounds[2] = bounds[2].max(x);
                bounds[3] = bounds[3].max(y);
            }
        }
    }
    if count == 0 {
        return Err(format!(
            "nested-scroll {} output has no recognizable leaf anchor",
            kind.label()
        ));
    }
    if kind == NestedScrollGpuLeafKind::Text {
        let width = bounds[2] - bounds[0] + 1;
        let height = bounds[3] - bounds[1] + 1;
        if count < 3 || width > 40 || height > 28 {
            return Err(format!(
                "nested-scroll Text glyph anchor is not localized: pixels={count}, bounds={bounds:?}"
            ));
        }
    }
    Ok(())
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

fn compare_scroll_scene_pixels(
    legacy: &[u8],
    retained: &[u8],
    transition_screen_y: f32,
    adapter: &str,
    case: &str,
) -> Result<(), String> {
    if legacy.len() != retained.len() {
        return Err(format!(
            "{case}: pixel buffer lengths differ on {adapter}: legacy={}, retained={}",
            legacy.len(),
            retained.len()
        ));
    }
    let mut diff = PixelDiff::default();
    for pixel_index in 0..(WIDTH * HEIGHT) as usize {
        let x = pixel_index as u32 % WIDTH;
        let y = pixel_index as u32 / WIDTH;
        let in_fractional_or_seam_band =
            (8..56).contains(&x) && ((y as f32 + 0.5) - transition_screen_y).abs() <= 1.5;
        let allowed = if in_fractional_or_seam_band { 2 } else { 1 };
        let offset = pixel_index * BYTES_PER_PIXEL as usize;
        let mut pixel_failed = false;
        for channel in 0..BYTES_PER_PIXEL as usize {
            let delta = legacy[offset + channel].abs_diff(retained[offset + channel]);
            diff.max_channel_delta = diff.max_channel_delta.max(delta);
            if delta > allowed {
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
        "{case}: legacy/RetainedScrollScene mismatch on {adapter}: mismatched_pixels={}, max_channel_delta={}, bounds={:?}, transition_y={transition_screen_y}, rule=delta<=1 outside the 1px transition/seam band and <=2 inside",
        diff.mismatched_pixels, diff.max_channel_delta, diff.bounds
    ))
}

fn compare_scroll_forest_pixels(
    legacy: &[u8],
    retained: &[u8],
    adapter: &str,
    case: &str,
) -> Result<(), String> {
    if legacy.len() != retained.len() {
        return Err(format!(
            "{case}: scroll-forest pixel buffer lengths differ on {adapter}: legacy={}, retained={}",
            legacy.len(),
            retained.len()
        ));
    }
    let transition_screen_y = [
        SCROLL_FOREST_ROOT_Y + SCROLL_FOREST_TRANSITIONS[0] - SCROLL_FOREST_OFFSETS[0],
        SCROLL_FOREST_ROOT_Y + SCROLL_FOREST_TRANSITIONS[1] - SCROLL_FOREST_OFFSETS[1],
    ];
    let mut diff = PixelDiff::default();
    for pixel_index in 0..(WIDTH * HEIGHT) as usize {
        let x = pixel_index as u32 % WIDTH;
        let y = pixel_index as u32 / WIDTH;
        let in_transition_band = (0..2).any(|ordinal| {
            let left = SCROLL_FOREST_ROOT_X[ordinal] as u32;
            let right = (SCROLL_FOREST_ROOT_X[ordinal] + SCROLL_FOREST_ROOT_WIDTH) as u32;
            (left..right).contains(&x)
                && ((y as f32 + 0.5) - transition_screen_y[ordinal]).abs() <= 1.5
        });
        let allowed = if in_transition_band { 2 } else { 1 };
        let offset = pixel_index * BYTES_PER_PIXEL as usize;
        let mut pixel_failed = false;
        for channel in 0..BYTES_PER_PIXEL as usize {
            let delta = legacy[offset + channel].abs_diff(retained[offset + channel]);
            diff.max_channel_delta = diff.max_channel_delta.max(delta);
            if delta > allowed {
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
        "{case}: legacy/production scroll-forest mismatch on {adapter}: mismatched_pixels={}, max_channel_delta={}, bounds={:?}",
        diff.mismatched_pixels, diff.max_channel_delta, diff.bounds
    ))
}

fn validate_scroll_forest_anchors(
    pixels: &[u8],
    version: ScrollForestContentVersion,
    adapter: &str,
    case: &str,
) -> Result<(), String> {
    let clear = pixel_at(pixels, 1, 1)?;
    let left_before = pixel_at(pixels, 12, 16)?;
    let left_after = pixel_at(pixels, 12, 36)?;
    let right_before = pixel_at(pixels, 44, 20)?;
    let right_after = pixel_at(pixels, 44, 40)?;
    let left_matches = match version {
        ScrollForestContentVersion::Baseline => {
            left_before[0] > 170
                && left_before[1] < 100
                && left_before[2] < 100
                && left_after[0] < 100
                && left_after[1] > 140
                && left_after[2] < 130
        }
        ScrollForestContentVersion::FirstRootMutated => {
            left_before[0] > 140
                && left_before[1] < 100
                && left_before[2] > 130
                && left_after[0] < 100
                && left_after[1] > 130
                && left_after[2] > 130
        }
    };
    let right_matches = right_before[0] < 100
        && right_before[1] < 130
        && right_before[2] > 150
        && right_after[0] > 150
        && right_after[1] > 130
        && right_after[2] < 100;
    if clear != [0, 0, 0, 0] || !left_matches || !right_matches {
        return Err(format!(
            "{case}: scroll-forest anchors drifted on {adapter}: clear={clear:?}, left_before={left_before:?}, left_after={left_after:?}, right_before={right_before:?}, right_after={right_after:?}"
        ));
    }
    Ok(())
}

fn validate_scroll_forest_right_root_unchanged(
    before: &[u8],
    after: &[u8],
    adapter: &str,
) -> Result<(), String> {
    for y in SCROLL_FOREST_ROOT_Y as u32..(SCROLL_FOREST_ROOT_Y + SCROLL_FOREST_ROOT_HEIGHT) as u32
    {
        for x in SCROLL_FOREST_ROOT_X[1] as u32
            ..(SCROLL_FOREST_ROOT_X[1] + SCROLL_FOREST_ROOT_WIDTH) as u32
        {
            let old = pixel_at(before, x, y)?;
            let new = pixel_at(after, x, y)?;
            if old
                .into_iter()
                .zip(new)
                .any(|(old, new)| old.abs_diff(new) > 1)
            {
                return Err(format!(
                    "scroll-forest first-root mutation changed the tiled sibling at ({x},{y}) on {adapter}: before={old:?}, after={new:?}"
                ));
            }
        }
    }
    Ok(())
}

fn run_native_scroll_scene_case(
    gpu: &NativeGpu,
    case: ScrollSceneGpuCase,
    scrollbar: GpuScrollbarCase,
) -> Result<(), String> {
    let adapter = gpu.label();
    let label = format!("{}/{scrollbar:?}", case.name);
    let mut viewport = Viewport::new();

    let (first_graph, first_trace) = retained_scroll_scene_graph(&mut viewport, case, scrollbar)?;
    if first_trace.backing != case.backing
        || first_trace.action != RetainedSurfaceCompileAction::Reraster
        || first_trace.reraster_count != first_trace.tile_count
        || first_trace.reuse_count != 0
    {
        return Err(format!(
            "{label}: frame 1 did not fully reraster {:?} backing on {adapter}: {first_trace:?}",
            case.backing
        ));
    }
    if (case.backing == ScrollSceneBackingKind::Single && first_trace.tile_count != 1)
        || (case.backing == ScrollSceneBackingKind::Tiled && first_trace.tile_count < 2)
    {
        return Err(format!(
            "{label}: unexpected frame-1 tile count on {adapter}: {first_trace:?}"
        ));
    }
    let first_pixels = render_on_viewport(first_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    viewport.finish_retained_surface_transaction(true);

    let (second_graph, second_trace) = retained_scroll_scene_graph(&mut viewport, case, scrollbar)?;
    if second_trace.backing != case.backing
        || second_trace.action != RetainedSurfaceCompileAction::Reuse
        || second_trace.reraster_count != 0
        || second_trace.reuse_count != second_trace.tile_count
        || second_trace.tile_count != first_trace.tile_count
    {
        return Err(format!(
            "{label}: frame 2 did not fully reuse {:?} backing on {adapter}: first={first_trace:?}, second={second_trace:?}",
            case.backing
        ));
    }
    let second_pixels = render_on_viewport(second_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    viewport.finish_retained_surface_transaction(true);

    let legacy_pixels = render(legacy_scroll_scene_graph(case, scrollbar)?, gpu)?;
    let transition_screen_y = 8.0 + case.transition_local_y - case.offset_y;
    compare_scroll_scene_pixels(
        &legacy_pixels,
        &first_pixels,
        transition_screen_y,
        &adapter,
        &format!("{label}/frame-1-reraster"),
    )?;
    compare_scroll_scene_pixels(
        &legacy_pixels,
        &second_pixels,
        transition_screen_y,
        &adapter,
        &format!("{label}/frame-2-reuse"),
    )?;
    compare_pixels(
        &first_pixels,
        &second_pixels,
        [0, 0, WIDTH, HEIGHT],
        &adapter,
        &format!("{label}/retained-frame-stability"),
    )?;
    Ok(())
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
    const TRANSFORM_ONLY: Self = Self {
        label: "transform-only",
        scroll_offset_y: 16.0,
        translation: [9.0, 4.0],
    };

    const GRAPH_BUILD_CASES: [Self; 3] = [Self::BASELINE, Self::SCROLL_ONLY, Self::TRANSFORM_ONLY];
}

const DIRECT_SCROLL_TRANSFORM_SCROLLPORT: [u32; 2] = [48, 40];
const DIRECT_SCROLL_TRANSFORM_CONTENT_HEIGHT: f32 = 120.0;
const DIRECT_SCROLL_TRANSFORM_GRADIENT_TRANSITION_Y: f32 = 24.0;

fn direct_scroll_transform_gpu_fixture(
    case: DirectScrollTransformGpuCase,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    // Mirrors frame_plan::tests::property_scroll_interleave_fixture's exact
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

fn legacy_direct_scroll_transform_graph(
    case: DirectScrollTransformGpuCase,
) -> Result<FrameGraph, String> {
    let (mut arena, root, _, _) = direct_scroll_transform_gpu_fixture(case);
    let (mut graph, ctx, target) = transformed_graph_prelude(1.0, None);
    arena
        .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
        .ok_or_else(|| format!("legacy direct S->T {} root disappeared", case.label))?;
    add_present(&mut graph, &target)?;
    Ok(graph)
}

type DirectScrollTransformResident = (
    crate::view::frame_graph::PersistentTextureKey,
    crate::view::frame_graph::TextureDesc,
);

fn direct_scroll_transform_resident(
    graph: &FrameGraph,
) -> Result<DirectScrollTransformResident, String> {
    let declared = graph
        .declared_persistent_textures()
        .map(|(key, desc)| (key, desc.clone()))
        .collect::<Vec<_>>();
    if declared.len() != 2 {
        return Err(format!(
            "direct S->T must declare exactly one color/depth pair: {declared:?}"
        ));
    }
    let colors = declared
        .iter()
        .filter(|(key, _)| key.depth_stencil().is_some())
        .cloned()
        .collect::<Vec<_>>();
    let [resident] = colors.as_slice() else {
        return Err(format!(
            "direct S->T declarations do not contain exactly one color key: {declared:?}"
        ));
    };
    let Some(depth_key) = resident.0.depth_stencil() else {
        unreachable!("filtered direct S->T resident owns a depth key")
    };
    if !declared.iter().any(|(key, _)| *key == depth_key) {
        return Err(format!(
            "direct S->T color key has no declared depth partner: {declared:?}"
        ));
    }
    Ok(resident.clone())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DirectScrollTransformCompositeShape {
    bounds_bits: [u32; 4],
    quad_position_bits: [[u32; 2]; 4],
    uv_bounds_bits: Option<[u32; 4]>,
    scissor_rect: Option<[u32; 4]>,
}

fn direct_scroll_transform_composite_shape(
    graph: &FrameGraph,
) -> Result<DirectScrollTransformCompositeShape, String> {
    let composites = graph.test_graphics_passes::<
        crate::view::render_pass::texture_composite_pass::TextureCompositePass,
    >();
    let [composite] = composites.as_slice() else {
        return Err(format!(
            "direct S->T must emit exactly one final texture composite, got {}",
            composites.len()
        ));
    };
    let snapshot = composite.test_snapshot();
    let Some(quad_position_bits) = snapshot.quad_position_bits else {
        return Err("direct S->T final composite must own explicit quad positions".to_string());
    };
    Ok(DirectScrollTransformCompositeShape {
        bounds_bits: snapshot.bounds_bits,
        quad_position_bits,
        uv_bounds_bits: snapshot.uv_bounds_bits,
        scissor_rect: snapshot.explicit_scissor_rect,
    })
}

fn production_direct_scroll_transform_graph(
    viewport: &mut Viewport,
    case: DirectScrollTransformGpuCase,
) -> Result<
    (
        FrameGraph,
        RetainedPropertyScrollSceneBuildTrace,
        crate::view::viewport::RetainedSurfaceFrameStageOwner,
        DirectScrollTransformResident,
        DirectScrollTransformCompositeShape,
    ),
    String,
> {
    // The admitted direct S->T production contract is deliberately exact:
    // DPR 1, incoming paint offset zero, and no external scissor.
    let (arena, root, properties, generations) = direct_scroll_transform_gpu_fixture(case);
    let budget = ScrollSceneSingleTextureBudget::new(
        wgpu::Limits::default().max_texture_dimension_2d,
        128 * 1024 * 1024,
    )
    .expect("direct S->T GPU budget is non-zero");
    let scene = plan_and_validate_direct_scroll_transform_scene(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        FORMAT,
        budget,
    )
    .map_err(|error| format!("direct S->T {} planner rejected: {error:?}", case.label))?;
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .ok_or_else(|| format!("direct S->T {} retained stage is unavailable", case.label))?;
    let mut graph = FrameGraph::new();
    let ctx = UiBuildContext::new(WIDTH, HEIGHT, FORMAT, 1.0);
    let prepared = prepare_direct_scroll_transform_scene_from_pool(
        viewport, scene, &mut graph, ctx, [0.0; 4], owner,
    )
    .map_err(|error| format!("direct S->T {} preflight rejected: {error:?}", case.label))?;
    let outcome = emit_prepared_direct_scroll_transform_scene(prepared);
    let (state, trace) = outcome.into_parts();
    let target = state.current_target().ok_or_else(|| {
        format!(
            "direct S->T {} emission produced no root target",
            case.label
        )
    })?;
    let resident = direct_scroll_transform_resident(&graph)?;
    let composite = direct_scroll_transform_composite_shape(&graph)?;
    add_present(&mut graph, &target)?;
    Ok((graph, trace, owner, resident, composite))
}

fn validate_direct_scroll_transform_graph_shape(
    graph: &FrameGraph,
    trace: RetainedPropertyScrollSceneBuildTrace,
    cold: bool,
    path: &str,
) -> Result<(), String> {
    let clears = graph
        .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
        .len();
    let draw_rects = graph.test_rect_pass_snapshots();
    let composite_passes = graph
        .test_graphics_passes::<
            crate::view::render_pass::texture_composite_pass::TextureCompositePass,
        >();
    let content_target = composite_passes
        .first()
        .and_then(|pass| pass.test_snapshot().source_handle);
    let content_gradient_draws = content_target.map_or(0, |content_target| {
        draw_rects
            .iter()
            .filter(|draw| draw.output_target == Some(content_target) && draw.gradient.is_some())
            .count()
    });
    let layer_composites = graph
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>(
        )
        .len();
    let expected_trace = if cold { (1, 0) } else { (0, 1) };
    let expected_clears = if cold { 2 } else { 1 };
    // The transparent host-before artifact is intentionally replayed every
    // frame, so it contributes one DrawRect even on U. Only the persistent T
    // target's gradient payload disappears on reuse.
    let expected_draw_rects = if cold { 2 } else { 1 };
    let expected_content_gradient_draws = usize::from(cold);
    if (trace.reraster_count, trace.reuse_count) != expected_trace
        || clears != expected_clears
        || draw_rects.len() != expected_draw_rects
        || content_gradient_draws != expected_content_gradient_draws
        || composite_passes.len() != 1
        || layer_composites != 0
        || graph.declared_persistent_texture_keys().count() != 2
    {
        return Err(format!(
            "direct S->T {path} graph shape drifted: trace=({},{}) clears={clears}, draw_rects={}, content_gradient_draws={content_gradient_draws}, texture_composites={}, layer_composites={layer_composites}, persistent_keys={}",
            trace.reraster_count,
            trace.reuse_count,
            draw_rects.len(),
            composite_passes.len(),
            graph.declared_persistent_texture_keys().count(),
        ));
    }
    Ok(())
}

fn validate_direct_scroll_transform_composite_delta(
    before: DirectScrollTransformCompositeShape,
    after: DirectScrollTransformCompositeShape,
    expected_delta: [f32; 2],
    path: &str,
) -> Result<(), String> {
    if before.uv_bounds_bits != after.uv_bounds_bits || before.scissor_rect != after.scissor_rect {
        return Err(format!(
            "direct S->T {path} changed offset-zero UV/scissor: before={before:?}, after={after:?}"
        ));
    }
    for (before_point, after_point) in before
        .quad_position_bits
        .iter()
        .zip(after.quad_position_bits.iter())
    {
        let actual = [
            f32::from_bits(after_point[0]) - f32::from_bits(before_point[0]),
            f32::from_bits(after_point[1]) - f32::from_bits(before_point[1]),
        ];
        if actual.map(f32::to_bits) != expected_delta.map(f32::to_bits) {
            return Err(format!(
                "direct S->T {path} quad delta drifted: expected={expected_delta:?}, actual={actual:?}"
            ));
        }
    }
    let actual_bounds_delta = [
        f32::from_bits(after.bounds_bits[0]) - f32::from_bits(before.bounds_bits[0]),
        f32::from_bits(after.bounds_bits[1]) - f32::from_bits(before.bounds_bits[1]),
    ];
    if actual_bounds_delta.map(f32::to_bits) != expected_delta.map(f32::to_bits)
        || before.bounds_bits[2..] != after.bounds_bits[2..]
    {
        return Err(format!(
            "direct S->T {path} bounds delta drifted: expected={expected_delta:?}, actual={actual_bounds_delta:?}"
        ));
    }
    Ok(())
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



#[derive(Clone, Copy, Debug)]
enum DirectPropertyScrollGpuGrammar {
    Transform { translation: [f32; 2] },
    Effect { opacity: f32 },
}

impl DirectPropertyScrollGpuGrammar {
    fn label(self) -> &'static str {
        match self {
            Self::Transform { .. } => "transform-scroll",
            Self::Effect { .. } => "effect-scroll",
        }
    }

    fn nonblank_anchor(self) -> (u32, u32) {
        match self {
            Self::Transform { translation } => {
                (translation[0] as u32 + 4, translation[1] as u32 + 4)
            }
            Self::Effect { .. } => (4, 4),
        }
    }
}

fn direct_property_scroll_gpu_fixture(
    grammar: DirectPropertyScrollGpuGrammar,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_2f01, 0.0, 0.0, 120.0, 90.0,
    ))));
    let scroll = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_2f02, 0.0, 0.0, 120.0, 90.0,
    ))));
    let content = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_2f10, 0.0, -20.0, 120.0, 240.0,
    ))));
    arena.set_parent(scroll, Some(root));
    arena.push_child(root, scroll);
    arena.set_parent(content, Some(scroll));
    arena.push_child(scroll, content);

    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, root);
        element.apply_style(root_style);
        match grammar {
            DirectPropertyScrollGpuGrammar::Transform { translation } => {
                element.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                    glam::Vec3::new(translation[0], translation[1], 0.0),
                )));
            }
            DirectPropertyScrollGpuGrammar::Effect { opacity } => {
                element.set_resolved_transform_for_test(None);
                element.set_opacity(opacity);
            }
        }
    }

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

    let mut content_style = Style::new();
    content_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, content);
        element.apply_style(content_style);
        element.set_background_color_value(Color::rgb(24, 48, 72));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena.refresh_subtree_dirty_cache(root);

    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(
        properties.validation_errors.is_empty(),
        "direct property-scroll GPU fixture property errors: {:?}",
        properties.validation_errors
    );
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, properties, generations)
}

fn legacy_direct_property_scroll_graph(
    grammar: DirectPropertyScrollGpuGrammar,
) -> Result<FrameGraph, String> {
    let (mut arena, root, _, _) = direct_property_scroll_gpu_fixture(grammar);
    let (mut graph, ctx, target) = transformed_graph_prelude(1.0, None);
    arena
        .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
        .ok_or_else(|| format!("legacy {} root disappeared", grammar.label()))?;
    add_present(&mut graph, &target)?;
    Ok(graph)
}

type DirectPropertyScrollResident = (
    crate::view::frame_graph::PersistentTextureKey,
    crate::view::frame_graph::TextureDesc,
);

fn direct_property_scroll_residents(
    graph: &FrameGraph,
) -> Result<Vec<DirectPropertyScrollResident>, String> {
    let declared = graph
        .declared_persistent_textures()
        .map(|(key, desc)| (key, desc.clone()))
        .collect::<Vec<_>>();
    if declared.is_empty() || declared.len() % 2 != 0 {
        return Err(format!(
            "direct property-scroll declarations must contain complete color/depth pairs: {declared:?}"
        ));
    }
    let colors = declared
        .iter()
        .filter(|(key, _)| key.depth_stencil().is_some())
        .cloned()
        .collect::<Vec<_>>();
    if colors.len() * 2 != declared.len()
        || colors.iter().any(|(color, _)| {
            color
                .depth_stencil()
                .is_none_or(|depth| !declared.iter().any(|(key, _)| *key == depth))
        })
    {
        return Err(format!(
            "direct property-scroll persistent declarations are not complete pairs: {declared:?}"
        ));
    }
    Ok(colors)
}

fn production_direct_property_scroll_graph(
    viewport: &mut Viewport,
    grammar: DirectPropertyScrollGpuGrammar,
    sampled_at: crate::time::Instant,
) -> Result<
    (
        FrameGraph,
        RetainedPropertyScrollSceneBuildTrace,
        crate::view::viewport::RetainedSurfaceFrameStageOwner,
        Vec<DirectPropertyScrollResident>,
    ),
    String,
> {
    // These production wrappers currently admit only scale=1, paint offset=0,
    // and no external scissor. Keep this closure pinned to that exact contract.
    let (arena, root, properties, generations) = direct_property_scroll_gpu_fixture(grammar);
    let budget = ScrollSceneSingleTextureBudget::new(
        wgpu::Limits::default().max_texture_dimension_2d,
        128 * 1024 * 1024,
    )
    .expect("direct property-scroll GPU budget is non-zero");
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .ok_or_else(|| format!("{} retained stage is unavailable", grammar.label()))?;
    let mut graph = FrameGraph::new();
    let ctx = UiBuildContext::new(WIDTH, HEIGHT, FORMAT, 1.0);
    let outcome = match grammar {
        DirectPropertyScrollGpuGrammar::Transform { .. } => {
            let scene = plan_and_validate_transform_scroll_scene(
                &arena,
                &[root],
                &properties,
                &generations,
                1.0,
                [0.0; 2],
                None,
                sampled_at,
                FORMAT,
                budget,
            )
            .map_err(|error| format!("T->S production wrapper rejected: {error:?}"))?;
            let prepared = prepare_retained_transform_scroll_scene_from_pool(
                viewport, scene, &mut graph, ctx, [0.0; 4], owner,
            )
            .map_err(|error| format!("T->S production preflight rejected: {error:?}"))?;
            emit_prepared_retained_transform_scroll_scene(prepared)
        }
        DirectPropertyScrollGpuGrammar::Effect { .. } => {
            let scene = plan_and_validate_effect_scroll_scene_checkpoint(
                &arena,
                &[root],
                &properties,
                &generations,
                1.0,
                [0.0; 2],
                None,
                sampled_at,
                FORMAT,
                budget,
            )
            .map_err(|error| format!("E->S production wrapper rejected: {error:?}"))?;
            let prepared = prepare_retained_effect_scroll_scene_from_pool(
                viewport, scene, &mut graph, ctx, [0.0; 4], owner,
            )
            .map_err(|error| format!("E->S production preflight rejected: {error:?}"))?;
            emit_prepared_retained_effect_scroll_scene(prepared)
        }
    };
    let (state, trace) = outcome.into_parts();
    let target = state
        .current_target()
        .ok_or_else(|| format!("{} emission produced no root target", grammar.label()))?;
    let residents = direct_property_scroll_residents(&graph)?;
    add_present(&mut graph, &target)?;
    Ok((graph, trace, owner, residents))
}

fn validate_direct_property_scroll_graph_shape(
    graph: &FrameGraph,
    grammar: DirectPropertyScrollGpuGrammar,
    cold: bool,
) -> Result<(), String> {
    let expected_clears = if cold { 3 } else { 1 };
    let clears = graph
        .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
        .len();
    let texture_composites = graph
        .test_graphics_passes::<
            crate::view::render_pass::texture_composite_pass::TextureCompositePass,
        >()
        .len();
    let layer_composites = graph
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>(
        )
        .len();
    let expected_texture_composites = match (grammar, cold) {
        (DirectPropertyScrollGpuGrammar::Transform { .. }, true) => 2,
        (DirectPropertyScrollGpuGrammar::Transform { .. }, false) => 1,
        (DirectPropertyScrollGpuGrammar::Effect { .. }, true) => 1,
        (DirectPropertyScrollGpuGrammar::Effect { .. }, false) => 0,
    };
    let expected_layer_composites = usize::from(matches!(
        grammar,
        DirectPropertyScrollGpuGrammar::Effect { .. }
    ));
    let expected_persistent_keys = if cold { 4 } else { 2 };
    if clears != expected_clears
        || texture_composites != expected_texture_composites
        || layer_composites != expected_layer_composites
        || graph.declared_persistent_texture_keys().count() != expected_persistent_keys
    {
        return Err(format!(
            "{} {} graph shape drifted: clears={clears}, texture_composites={texture_composites}, layer_composites={layer_composites}, persistent_keys={}",
            grammar.label(),
            if cold { "cold" } else { "warm" },
            graph.declared_persistent_texture_keys().count(),
        ));
    }
    Ok(())
}

fn validate_direct_property_scroll_nonblank_anchor(
    pixels: &[u8],
    grammar: DirectPropertyScrollGpuGrammar,
    path: &str,
    adapter: &str,
) -> Result<(), String> {
    let (x, y) = grammar.nonblank_anchor();
    let actual = pixel_at(pixels, x, y)?;
    if actual == [0; 4] || actual[3] == 0 {
        return Err(format!(
            "{} {path} anchor is blank on {adapter}: ({x},{y})={actual:?}",
            grammar.label()
        ));
    }
    Ok(())
}

fn warm_direct_property_scroll_receiver_matches_cold(
    cold: &[DirectPropertyScrollResident],
    warm: &[DirectPropertyScrollResident],
) -> bool {
    warm.len() == 1 && cold.iter().any(|candidate| candidate == &warm[0])
}



#[derive(Clone, Copy, Debug)]
struct TransformEffectScrollGpuFrame {
    translation: [f32; 2],
}

impl TransformEffectScrollGpuFrame {
    fn nonblank_anchor(self) -> (u32, u32) {
        (
            self.translation[0] as u32 + 4,
            self.translation[1] as u32 + 4,
        )
    }
}

fn transform_effect_scroll_gpu_fixture(
    frame: TransformEffectScrollGpuFrame,
) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_3f01, 0.0, 0.0, 120.0, 90.0,
    ))));
    let effect = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_3f02, 0.0, 0.0, 120.0, 90.0,
    ))));
    let scroll = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_3f03, 0.0, 0.0, 120.0, 90.0,
    ))));
    let content = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xb4_3f10, 0.0, -20.0, 120.0, 240.0,
    ))));
    arena.set_parent(effect, Some(root));
    arena.push_child(root, effect);
    arena.set_parent(scroll, Some(effect));
    arena.push_child(effect, scroll);
    arena.set_parent(content, Some(scroll));
    arena.push_child(scroll, content);

    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, root);
        element.apply_style(root_style);
        element.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
            glam::Vec3::new(frame.translation[0], frame.translation[1], 0.0),
        )));
    }

    let mut effect_style = Style::new();
    effect_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, effect);
        element.apply_style(effect_style);
        element.set_opacity(0.625);
        element.set_background_color_value(Color::rgb(32, 64, 96));
    }

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

    let mut content_style = Style::new();
    content_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, content);
        element.apply_style(content_style);
        element.set_background_color_value(Color::rgb(24, 48, 72));
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena.refresh_subtree_dirty_cache(root);

    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(
        properties.validation_errors.is_empty(),
        "T->E->S GPU fixture property errors: {:?}",
        properties.validation_errors
    );
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, properties, generations)
}

fn legacy_transform_effect_scroll_graph(
    frame: TransformEffectScrollGpuFrame,
) -> Result<FrameGraph, String> {
    let (mut arena, root, _, _) = transform_effect_scroll_gpu_fixture(frame);
    let (mut graph, ctx, target) = transformed_graph_prelude(1.0, None);
    arena
        .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
        .ok_or_else(|| "legacy T->E->S root disappeared".to_string())?;
    add_present(&mut graph, &target)?;
    Ok(graph)
}

fn production_transform_effect_scroll_graph(
    viewport: &mut Viewport,
    frame: TransformEffectScrollGpuFrame,
    sampled_at: crate::time::Instant,
) -> Result<
    (
        FrameGraph,
        RetainedPropertyScrollSceneBuildTrace,
        crate::view::viewport::RetainedSurfaceFrameStageOwner,
        Vec<DirectPropertyScrollResident>,
    ),
    String,
> {
    // The exact production grammar currently admits only scale=1, paint
    // offset=0 and no external scissor.
    let (arena, root, properties, generations) = transform_effect_scroll_gpu_fixture(frame);
    let scene = plan_and_validate_transform_effect_scroll_scene(
        &arena,
        &[root],
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        sampled_at,
        FORMAT,
        ScrollSceneSingleTextureBudget::new(
            wgpu::Limits::default().max_texture_dimension_2d,
            128 * 1024 * 1024,
        )
        .expect("T->E->S GPU budget is non-zero"),
    )
    .map_err(|error| format!("T->E->S production wrapper rejected: {error:?}"))?;
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .ok_or_else(|| "T->E->S retained stage is unavailable".to_string())?;
    let mut graph = FrameGraph::new();
    let prepared = prepare_retained_transform_effect_scroll_scene_from_pool(
        viewport,
        scene,
        &mut graph,
        UiBuildContext::new(WIDTH, HEIGHT, FORMAT, 1.0),
        [0.0; 4],
        owner,
    )
    .map_err(|error| format!("T->E->S production preflight rejected: {error:?}"))?;
    let outcome = emit_prepared_retained_transform_effect_scroll_scene(prepared);
    let (state, trace) = outcome.into_parts();
    let target = state
        .current_target()
        .ok_or_else(|| "T->E->S emission produced no root target".to_string())?;
    let residents = direct_property_scroll_residents(&graph)?;
    add_present(&mut graph, &target)?;
    Ok((graph, trace, owner, residents))
}

fn validate_transform_effect_scroll_graph_shape(
    graph: &FrameGraph,
    cold: bool,
) -> Result<(), String> {
    let clears = graph
        .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
        .len();
    let texture_composites = graph
        .test_graphics_passes::<
            crate::view::render_pass::texture_composite_pass::TextureCompositePass,
        >()
        .len();
    let layer_composites = graph
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>(
        )
        .len();
    let persistent_keys = graph.declared_persistent_texture_keys().count();
    let expected = if cold { (4, 2, 1, 6) } else { (1, 1, 0, 4) };
    if (
        clears,
        texture_composites,
        layer_composites,
        persistent_keys,
    ) != expected
    {
        return Err(format!(
            "T->E->S {} graph shape drifted: clears={clears}, texture_composites={texture_composites}, layer_composites={layer_composites}, persistent_keys={persistent_keys}, expected={expected:?}",
            if cold { "cold" } else { "warm" }
        ));
    }
    Ok(())
}

fn transform_effect_scroll_warm_declarations_match_cold(
    cold: &[DirectPropertyScrollResident],
    warm: &[DirectPropertyScrollResident],
) -> bool {
    if !transform_effect_scroll_resident_roles_are_exact(cold, true)
        || !transform_effect_scroll_resident_roles_are_exact(warm, false)
        || !warm
            .iter()
            .all(|pair| cold.iter().any(|cold_pair| cold_pair == pair))
    {
        return false;
    }
    let omitted = cold
        .iter()
        .filter(|pair| !warm.iter().any(|warm_pair| warm_pair == *pair))
        .collect::<Vec<_>>();
    matches!(
        omitted.as_slice(),
        [(
            crate::view::frame_graph::PersistentTextureKey::Retained {
                role: crate::view::frame_graph::RetainedTextureRole::ScrollContentColor,
                ..
            },
            _
        )]
    )
}

fn transform_effect_scroll_resident_roles_are_exact(
    residents: &[DirectPropertyScrollResident],
    include_scroll_content: bool,
) -> bool {
    let mut transformed = 0;
    let mut isolation = 0;
    let mut scroll_content = 0;
    for (key, _) in residents {
        match key {
            crate::view::frame_graph::PersistentTextureKey::Retained {
                role: crate::view::frame_graph::RetainedTextureRole::TransformedColor,
                ..
            } => transformed += 1,
            crate::view::frame_graph::PersistentTextureKey::Retained {
                role: crate::view::frame_graph::RetainedTextureRole::IsolationColor,
                ..
            } => isolation += 1,
            crate::view::frame_graph::PersistentTextureKey::Retained {
                role: crate::view::frame_graph::RetainedTextureRole::ScrollContentColor,
                ..
            } => scroll_content += 1,
            _ => return false,
        }
    }
    transformed == 1
        && isolation == 1
        && scroll_content == usize::from(include_scroll_content)
        && residents.len() == if include_scroll_content { 3 } else { 2 }
}

fn validate_transform_effect_scroll_nonblank_anchor(
    pixels: &[u8],
    frame: TransformEffectScrollGpuFrame,
    path: &str,
    adapter: &str,
) -> Result<(), String> {
    let (x, y) = frame.nonblank_anchor();
    let actual = pixel_at(pixels, x, y)?;
    if actual == [0; 4] || actual[3] == 0 {
        return Err(format!(
            "T->E->S {path} anchor is blank on {adapter}: ({x},{y})={actual:?}"
        ));
    }
    Ok(())
}









mod oracle_tests;
mod scroll_graph_build_tests;
mod native_pixel_oracle_tests;

mod native_scroll_scene_pixel_tests;
mod native_scroll_forest_tests;
mod native_transform_surface_tests;
mod native_nested_scroll_tests;
mod native_scroll_boundary_tests;
mod native_root_effect_tests;
mod native_svg_pixel_tests;
