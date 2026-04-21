#![allow(missing_docs)]
use rustc_hash::{FxHashMap, FxHashSet};

mod debug;
mod dispatch;
mod frame;
mod input;
mod gpu_resources;
mod lifecycle;
mod promotion_runtime;
mod render;
mod scene_helpers;
mod transitions_tick;
#[cfg(any())]
mod tests;
#[cfg(test)]
mod m2_incremental_tests;

use crate::time::Instant;
use crate::transition::{
    AnimationPlugin, ChannelId, ClaimMode, LayoutTransitionPlugin,
    ScrollAxis, ScrollTransition, ScrollTransitionPlugin, StyleField,
    StyleTransitionPlugin, StyleValue,
    TrackKey, TrackTarget, Transition,
    TransitionFrame, TransitionHost, TransitionPluginId, VisualTransitionPlugin,
    CHANNEL_LAYOUT_HEIGHT, CHANNEL_LAYOUT_WIDTH, CHANNEL_LAYOUT_X, CHANNEL_LAYOUT_Y, CHANNEL_SCROLL_X,
    CHANNEL_SCROLL_Y, CHANNEL_STYLE_BACKGROUND_COLOR, CHANNEL_STYLE_BORDER_BOTTOM_COLOR, CHANNEL_STYLE_BORDER_LEFT_COLOR, CHANNEL_STYLE_BORDER_RADIUS,
    CHANNEL_STYLE_BORDER_RIGHT_COLOR, CHANNEL_STYLE_BORDER_TOP_COLOR, CHANNEL_STYLE_BOX_SHADOW, CHANNEL_STYLE_COLOR, CHANNEL_STYLE_OPACITY, CHANNEL_STYLE_TRANSFORM,
    CHANNEL_STYLE_TRANSFORM_ORIGIN, CHANNEL_VISUAL_X, CHANNEL_VISUAL_Y,
};
use crate::ui::{
    peek_state_dirty, reconcile, take_state_dirty, BlurEvent, ClickEvent, EventCommand, EventMeta,
    FocusEvent, FromPropValue, ImePreeditEvent, KeyDownEvent, KeyEventData,
    KeyUpEvent, NodeId, Patch, PointerButtons as UiPointerButtons, PointerDownEvent,
    PointerEventData, PointerMoveEvent, PointerUpEvent, PointerUpUntilHandler, PropValue,
    RsxNode, TextInputEvent, ViewportListenerHandle,
};
use crate::view::base_component::Renderable;
use crate::view::frame_graph::texture_resource::TextureDesc;
use crate::view::frame_graph::{AllocationId, BufferDesc, FrameGraph};
use crate::view::promotion::{
    active_channels_by_node, evaluate_promotion, PromotedLayerUpdate, PromotedLayerUpdateKind,
    PromotionDecision, PromotionState, ViewportPromotionConfig,
};
use crate::view::promotion_builder::{
    collect_debug_subtree_signatures, collect_promoted_layer_updates, collect_promotion_candidates,
};
use crate::view::render_pass::render_target::{OffscreenRenderTargetPool, RenderTargetBundle};
use crate::{
    ColorLike, Cursor, ElementStylePropSchema, HexColor, PropertyId, Style,
};

use std::ops::Sub;
use std::sync::Arc;
use wgpu::util::StagingBelt;
use wgpu::{
    rwh::{HasDisplayHandle, HasWindowHandle}, Instance, Queue,
    TextureUsages,
};

pub(crate) use self::debug::{
    begin_debug_reuse_path_frame, record_debug_reuse_path, set_debug_trace_enabled,
    DebugReusePathContext, DebugReusePathRecord,
};
use self::debug::{
    build_compile_trace_nodes, build_execute_detail_trace_nodes, build_layout_place_trace_nodes, build_reuse_overlay_geometry,
    build_text_measure_trace_nodes, format_promotion_trace,
    format_reuse_path_trace, format_style_field,
    format_style_promotion_trace, format_style_request_trace, format_style_sample_trace,
    format_style_value, format_trace_render_tree, record_debug_style_promotion,
    record_debug_style_request, record_debug_style_sample, record_debug_style_sample_record,
    style_field_requires_relayout, take_debug_reuse_path, take_debug_style_sample_records,
    trace_promoted_build_frame_marker, DebugStyleSampleRecord, PostLayoutTransitionResult,
    TraceRenderNode,
};
pub use self::frame::FrameParts;
use self::frame::{
    BeginFrameProfile, EndFrameProfile, FrameState, FrameStats, FrameTimings, LayoutPassResult,
};
use self::input::{
    is_valid_click_candidate, DragState, InputState, PendingClick, ViewportPointerUpListener,
};
pub use self::input::{PointerButton, ViewportDebugOptions};
use self::transitions_tick::TransitionHostAdapter;
use crate::app::App;
use crate::platform::{
    Modifiers, PlatformImePreedit, PlatformKeyEvent, PlatformPointerEvent,
    PlatformPointerEventKind, PlatformRequests, PlatformTextInput, PlatformWheelEvent,
    PointerType,
};

pub trait WindowHandle: HasWindowHandle + HasDisplayHandle {}
impl<T: HasWindowHandle + HasDisplayHandle> WindowHandle for T {}

pub type Window = Arc<dyn WindowHandle + Send + Sync>;

/// How the viewport should pick a surface format from the adapter's
/// capabilities. Native normally prefers sRGB; the browser surface on wasm
/// usually wants a non-sRGB format for correct color reproduction. The
/// preference is data — the viewport itself has no `cfg(wasm32)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceFormatPreference {
    PreferSrgb,
    PreferNonSrgb,
}

impl Default for SurfaceFormatPreference {
    fn default() -> Self {
        Self::PreferSrgb
    }
}

pub struct ViewportControl<'a> {
    viewport: &'a mut Viewport,
}

impl<'a> ViewportControl<'a> {
    pub fn new(viewport: &'a mut Viewport) -> Self {
        Self { viewport }
    }

    pub fn request_redraw(&mut self) {
        self.viewport.request_redraw();
    }

    pub fn set_focus(&mut self, node_id: Option<crate::view::node_arena::NodeKey>) {
        self.viewport.set_focused_node_id(node_id);
    }

    pub fn set_scroll_transition(&mut self, transition: ScrollTransition) {
        self.viewport.transitions.scroll_transition = transition;
    }

    pub fn set_selects(&mut self, selects: Vec<u64>) {
        self.viewport.set_selects(selects);
    }

    pub fn start_scroll_track(
        &mut self,
        target: TrackTarget,
        axis: ScrollAxis,
        from: f32,
        to: f32,
    ) -> bool {
        self.viewport.start_scroll_track(target, axis, from, to)
    }

    pub fn cancel_scroll_track(&mut self, target: TrackTarget, axis: ScrollAxis) {
        self.viewport.cancel_scroll_track(target, axis);
    }

    pub fn set_pointer_capture(&mut self, node_id: crate::view::node_arena::NodeKey) {
        self.viewport.set_pointer_capture_node_id(Some(node_id));
    }

    pub fn release_pointer_capture(&mut self, node_id: crate::view::node_arena::NodeKey) {
        if self.viewport.pointer_capture_node_id() == Some(node_id) {
            self.viewport.set_pointer_capture_node_id(None);
        }
    }

    pub fn set_clipboard_text(&mut self, text: impl Into<String>) {
        self.viewport.set_clipboard_text(text);
    }

    pub fn clipboard_text(&mut self) -> Option<String> {
        self.viewport.clipboard_text()
    }

    pub fn set_debug_options(&mut self, options: ViewportDebugOptions) {
        self.viewport.set_debug_options(options);
    }

    pub fn set_msaa_sample_count(&mut self, sample_count: u32) {
        self.viewport.set_msaa_sample_count(sample_count);
    }

    pub fn release_render_resource_caches(&mut self) {
        self.viewport.release_render_resource_caches();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderFrameResult {
    Ok,
    NeedsRetry,
}

pub struct Viewport {
    style: Style,
    clear_color: Box<dyn ColorLike>,
    scale_factor: f32,
    logical_width: f32,
    logical_height: f32,
    gpu: GpuContext,
    frame: FrameRuntime,
    pending_size: Option<(u32, u32)>,
    needs_reconfigure: bool,
    redraw_requested: bool,
    debug_options: ViewportDebugOptions,
    compositor: CompositorState,
    input_state: InputState,
    clipboard_fallback: Option<String>,
    dispatched_focus_node_id: Option<crate::view::node_arena::NodeKey>,
    scene: SceneState,
    transitions: TransitionRuntime,
    cursor_override: Option<Cursor>,
    last_recorded_cursor: Option<Cursor>,
    pending_platform_requests: PlatformRequests,
    /// Set inside `render_rsx` whenever any transition or
    /// animation plugin reports `keep_running`. Cleared at the start of
    /// every render. Hosts query this via `is_animating()` to decide
    /// whether to pump another frame immediately or idle.
    is_animating: bool,
    viewport_pointer_move_listeners: Vec<crate::ui::PointerMoveHandlerProp>,
    viewport_pointer_up_listeners: Vec<ViewportPointerUpListener>,
    app: Option<Box<dyn App>>,
    cached_rsx: Option<RsxNode>,
    needs_rebuild: bool,
    ready_dispatched: bool,
}

/// Phase-7 extraction. The retained scene tree and the per-node state
/// layered on top of it: the concrete `ElementTrait` roots produced by the
/// last reconcile pass, ad-hoc scroll offsets, element-side snapshot
/// blobs, and the last `RsxNode` seen from the caller. Non-pub.
struct SceneState {
    /// Arena-backed retained UI tree. Replaced `ui_roots` in the
    /// Approach-C migration; all layout/render/dispatch walks go through
    /// this arena via [`SceneState::ui_root_keys`].
    node_arena: super::node_arena::NodeArena,
    ui_root_keys: Vec<super::node_arena::NodeKey>,
    scroll_offsets: FxHashMap<u64, (f32, f32)>,
    last_rsx_root: Option<RsxNode>,
    /// Phase A M1 dark-launch flag for the Fiber-commit (`FiberWork`)
    /// path. Defaults to `false`; M1 never reads it from `render_rsx`
    /// (the plumbing is in place but not yet wired). M2 flips the
    /// switch and compares output against the legacy `apply_patch`
    /// pipeline.
    use_incremental_commit: bool,
}

impl SceneState {
    fn new() -> Self {
        Self {
            node_arena: super::node_arena::NodeArena::new(),
            ui_root_keys: Vec::new(),
            scroll_offsets: FxHashMap::default(),
            last_rsx_root: None,
            // M5: flag-on by default. Every failure mode in the
            // incremental path (non-committable work, translation
            // None, descriptor build error) is caught in
            // `render_rsx` and falls through to the legacy
            // full-rebuild pipeline, so the default-true setting
            // trades no correctness for reduced per-frame work on
            // the happy path. Setters (`set_use_incremental_commit`)
            // still let call sites flip it off for A/B testing or
            // regression bisection.
            use_incremental_commit: true,
        }
    }
}

/// Phase-7 extraction. Everything scoped to a single render frame: the
/// per-frame state, pooled GPU allocations, frame-graph cache, and debug
/// overlay geometry buffers. Non-pub; the viewport re-exposes whatever the
/// outside world needs through existing accessor methods.
struct FrameRuntime {
    frame_state: Option<FrameState>,
    offscreen_render_target_pool: OffscreenRenderTargetPool,
    sampled_texture_cache: FxHashMap<u64, SampledTextureEntry>,
    frame_buffer_pool: FxHashMap<u32, FrameBufferEntry>,
    draw_rect_uniform_pool: Vec<DrawRectUniformBufferEntry>,
    draw_rect_uniform_cursor: usize,
    draw_rect_uniform_offset: u64,
    gradient_stops_buffer: Option<GradientStopsBufferEntry>,
    gradient_stops_byte_cursor: u64,
    frame_stats: FrameStats,
    frame_presented: bool,
    last_frame_graph: Option<FrameGraph>,
    compile_cache: Option<CachedCompiledGraph>,
    debug_overlay_vertices: Vec<super::render_pass::debug_overlay_pass::DebugOverlayVertex>,
    debug_overlay_indices: Vec<u32>,
    /// Stash for `App::build()` elapsed time (ms) so the render trace tree
    /// can include RSX build cost.  Set in `render_frame`, consumed in
    /// `render_render_tree`.
    rsx_build_ms: f64,
}

impl FrameRuntime {
    fn new(trace_fps: bool) -> Self {
        Self {
            frame_state: None,
            offscreen_render_target_pool: OffscreenRenderTargetPool::new(),
            sampled_texture_cache: FxHashMap::default(),
            frame_buffer_pool: FxHashMap::default(),
            draw_rect_uniform_pool: Vec::new(),
            draw_rect_uniform_cursor: 0,
            draw_rect_uniform_offset: 0,
            gradient_stops_buffer: None,
            gradient_stops_byte_cursor: 0,
            frame_stats: FrameStats::new(trace_fps),
            frame_presented: false,
            last_frame_graph: None,
            compile_cache: None,
            debug_overlay_vertices: Vec::new(),
            debug_overlay_indices: Vec::new(),
            rsx_build_ms: 0.0,
        }
    }
}

/// Phase-7 extraction. Owns every transition and animation plugin plus the
/// shared channel / claim bookkeeping they all consume. The
/// `TransitionHostAdapter` built on every tick borrows `transition_channels`
/// immutably and `transition_claims` mutably from here, so field names are
/// preserved verbatim to keep the adapter sites mechanical.
struct TransitionRuntime {
    transition_channels: FxHashSet<ChannelId>,
    transition_claims: FxHashMap<TrackKey<TrackTarget>, TransitionPluginId>,
    scroll_transition_plugin: ScrollTransitionPlugin,
    layout_transition_plugin: LayoutTransitionPlugin,
    visual_transition_plugin: VisualTransitionPlugin,
    style_transition_plugin: StyleTransitionPlugin,
    animation_plugin: AnimationPlugin,
    scroll_transition: ScrollTransition,
    last_transition_tick: Option<Instant>,
    transition_epoch: Option<Instant>,
}

impl TransitionRuntime {
    fn new() -> Self {
        Self {
            transition_channels: [
                CHANNEL_SCROLL_X,
                CHANNEL_SCROLL_Y,
                CHANNEL_LAYOUT_X,
                CHANNEL_LAYOUT_Y,
                CHANNEL_LAYOUT_WIDTH,
                CHANNEL_LAYOUT_HEIGHT,
                CHANNEL_VISUAL_X,
                CHANNEL_VISUAL_Y,
                CHANNEL_STYLE_OPACITY,
                CHANNEL_STYLE_BORDER_RADIUS,
                CHANNEL_STYLE_BACKGROUND_COLOR,
                CHANNEL_STYLE_COLOR,
                CHANNEL_STYLE_BORDER_TOP_COLOR,
                CHANNEL_STYLE_BORDER_RIGHT_COLOR,
                CHANNEL_STYLE_BORDER_BOTTOM_COLOR,
                CHANNEL_STYLE_BORDER_LEFT_COLOR,
                CHANNEL_STYLE_BOX_SHADOW,
                CHANNEL_STYLE_TRANSFORM,
                CHANNEL_STYLE_TRANSFORM_ORIGIN,
            ]
            .into_iter()
            .collect(),
            transition_claims: FxHashMap::default(),
            scroll_transition_plugin: ScrollTransitionPlugin::new(),
            layout_transition_plugin: LayoutTransitionPlugin::new(),
            visual_transition_plugin: VisualTransitionPlugin::new(),
            style_transition_plugin: StyleTransitionPlugin::new(),
            animation_plugin: AnimationPlugin::new(),
            scroll_transition: ScrollTransition::new(250).ease_out(),
            last_transition_tick: None,
            transition_epoch: None,
        }
    }
}

/// Phase-7 extraction. Groups the wgpu surface / device / queue / attachments
/// plus their configuration knobs. Everything the renderer needs to talk to
/// the GPU lives here. No public API depends on the struct — accessor methods
/// on `Viewport` still return `&wgpu::Device` and friends.
struct GpuContext {
    surface: Option<wgpu::Surface<'static>>,
    surface_config: wgpu::SurfaceConfiguration,
    /// Format pipelines writing to the surface compile against and that the
    /// per-frame surface view is created with. Equals `surface_config.format`
    /// when the adapter advertises an sRGB format directly (native path); on
    /// WebGPU the canvas storage is a non-sRGB format so this points at the
    /// sRGB view variant (e.g. `Bgra8UnormSrgb`) listed in `view_formats`,
    /// giving the GPU linear→sRGB encoding on store.
    surface_target_format: wgpu::TextureFormat,
    device: Option<wgpu::Device>,
    instance: Option<Instance>,
    window: Option<Window>,
    surface_format_preference: SurfaceFormatPreference,
    queue: Option<Queue>,
    msaa_sample_count: u32,
    depth_texture: Option<wgpu::Texture>,
    depth_view: Option<wgpu::TextureView>,
    upload_staging_belt: Option<StagingBelt>,
}

/// Phase-7 extraction. Groups everything the layer-promotion / compositor
/// pipeline owns — promotion state, cached signatures for reuse, and the
/// box-model snapshots produced each frame. Pure internal refactor; the
/// viewport public API is unchanged and nothing outside the viewport names
/// this type.
struct CompositorState {
    promotion_state: PromotionState,
    promotion_config: ViewportPromotionConfig,
    promoted_layer_updates: Vec<PromotedLayerUpdate>,
    promoted_base_signatures: FxHashMap<u64, u64>,
    promoted_composition_signatures: FxHashMap<u64, u64>,
    debug_previous_subtree_signatures: FxHashMap<u64, (u64, u64, u64, bool)>,
    promoted_reuse_cooldown_frames: u8,
    frame_box_models: Vec<super::base_component::BoxModelSnapshot>,
}

impl CompositorState {
    fn new() -> Self {
        Self {
            promotion_state: PromotionState::default(),
            promotion_config: ViewportPromotionConfig::default(),
            promoted_layer_updates: Vec::new(),
            promoted_base_signatures: FxHashMap::default(),
            promoted_composition_signatures: FxHashMap::default(),
            debug_previous_subtree_signatures: FxHashMap::default(),
            promoted_reuse_cooldown_frames: 0,
            frame_box_models: Vec::new(),
        }
    }
}

struct CachedCompiledGraph {
    topology_hash: u64,
    graph: super::frame_graph::CompiledGraph,
}

#[derive(Clone)]
pub(super) struct FrameBufferEntry {
    pub(super) buffer: wgpu::Buffer,
    pub(super) size: u64,
    pub(super) usage: wgpu::BufferUsages,
}

pub(super) struct DrawRectUniformBufferEntry {
    pub(super) buffer: wgpu::Buffer,
    pub(super) size: u64,
    /// Cached bind groups keyed by layout_cache_key.  The bind group binds the buffer
    /// at offset 0 / size=slot_size; the per-draw dynamic offset is supplied separately,
    /// so one bind group is valid for *all* slots in this buffer.
    pub(super) bind_groups: FxHashMap<u64, wgpu::BindGroup>,
}

pub(super) struct GradientStopsBufferEntry {
    pub(super) buffer: wgpu::Buffer,
    pub(super) size: u64,
}

pub(super) struct SampledTextureEntry {
    pub(super) texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) format: wgpu::TextureFormat,
    pub(super) byte_size: u64,
}

impl Viewport {
    const DEFAULT_MSAA_SAMPLE_COUNT: u32 = 4;
    const PROMOTED_REUSE_COOLDOWN_FRAMES: u8 = 2;
    /// Skia GrResourceCache default: 96 MB.
    const SAMPLED_TEXTURE_PRESSURE_BYTES: u64 = 96 * 1024 * 1024;
    const SAMPLED_TEXTURE_EVICT_TO_BYTES: u64 = 72 * 1024 * 1024;
    /// Evict unreferenced textures idle for this many ticks (~5 s @60 fps).
    const SAMPLED_TEXTURE_STALE_TICKS: u64 = 300;

    fn normalize_msaa_sample_count(sample_count: u32) -> u32 {
        match sample_count {
            1 | 2 | 4 | 8 | 16 => sample_count,
            0 => 1,
            _ => Self::DEFAULT_MSAA_SAMPLE_COUNT,
        }
    }

    pub fn new() -> Self {
        let debug_options = ViewportDebugOptions::from_env();
        Viewport {
            style: Style::new(),
            clear_color: Box::new(HexColor::new("#000000")),
            scale_factor: 1.0,
            logical_width: 1.0,
            logical_height: 1.0,
            gpu: GpuContext {
                surface: None,
                surface_config: wgpu::SurfaceConfiguration {
                    usage: TextureUsages::RENDER_ATTACHMENT
                        | TextureUsages::COPY_SRC
                        | TextureUsages::COPY_DST,
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    width: 1,
                    height: 1,
                    present_mode: Self::present_mode_from_env(),
                    desired_maximum_frame_latency: 2,
                    alpha_mode: wgpu::CompositeAlphaMode::Auto,
                    view_formats: vec![],
                },
                surface_target_format: wgpu::TextureFormat::Bgra8Unorm,
                device: None,
                instance: None,
                window: None,
                surface_format_preference: SurfaceFormatPreference::default(),
                queue: None,
                msaa_sample_count: Self::DEFAULT_MSAA_SAMPLE_COUNT,
                depth_texture: None,
                depth_view: None,
                upload_staging_belt: None,
            },
            frame: FrameRuntime::new(debug_options.trace_fps),
            pending_size: None,
            needs_reconfigure: false,
            redraw_requested: false,
            debug_options,
            compositor: CompositorState::new(),
            input_state: InputState::default(),
            clipboard_fallback: None,
            dispatched_focus_node_id: None,
            scene: SceneState::new(),
            transitions: TransitionRuntime::new(),
            cursor_override: None,
            last_recorded_cursor: None,
            pending_platform_requests: PlatformRequests::default(),
            is_animating: false,
            viewport_pointer_move_listeners: Vec::new(),
            viewport_pointer_up_listeners: Vec::new(),
            app: None,
            cached_rsx: None,
            needs_rebuild: true,
            ready_dispatched: false,
        }
    }

    /// Phase A M1 dark-launch switch for the incremental Fiber commit
    /// path. Off by default. `render_rsx` will continue to use the
    /// legacy `apply_patch` pipeline regardless until M2 wires the
    /// flag through the commit site.
    pub fn set_use_incremental_commit(&mut self, on: bool) {
        self.scene.use_incremental_commit = on;
    }

    /// Read the current setting of
    /// [`Self::set_use_incremental_commit`].
    pub fn use_incremental_commit(&self) -> bool {
        self.scene.use_incremental_commit
    }

    pub fn set_app(&mut self, app: Box<dyn App>) {
        self.app = Some(app);
        self.cached_rsx = None;
        self.needs_rebuild = true;
        self.ready_dispatched = false;
    }

    pub fn debug_options(&self) -> ViewportDebugOptions {
        self.debug_options
    }

    pub fn msaa_sample_count(&self) -> u32 {
        self.gpu.msaa_sample_count
    }

    pub fn set_msaa_sample_count(&mut self, sample_count: u32) {
        let normalized = Self::normalize_msaa_sample_count(sample_count);
        if self.gpu.msaa_sample_count == normalized {
            return;
        }
        self.gpu.msaa_sample_count = normalized;
        self.invalidate_promoted_layer_reuse();
        self.needs_reconfigure = true;
        if self.gpu.surface.is_some() && self.gpu.device.is_some() {
            self.create_frame_attachments();
        }
        self.request_redraw();
    }

    pub fn set_debug_options(&mut self, options: ViewportDebugOptions) {
        self.debug_options = options;
        self.frame.frame_stats.set_enabled(options.trace_fps);
    }

    pub(crate) fn debug_overlay_enabled(&self) -> bool {
        self.debug_options.geometry_overlay || self.debug_options.trace_reuse_path
    }

    pub(crate) fn clear_debug_overlay_geometry(&mut self) {
        self.frame.debug_overlay_vertices.clear();
        self.frame.debug_overlay_indices.clear();
    }

    pub(crate) fn push_debug_overlay_geometry(
        &mut self,
        vertices: &[super::render_pass::debug_overlay_pass::DebugOverlayVertex],
        indices: &[u32],
    ) {
        if vertices.is_empty() || indices.is_empty() {
            return;
        }
        let base = self.frame.debug_overlay_vertices.len() as u32;
        self.frame.debug_overlay_vertices.extend_from_slice(vertices);
        self.frame.debug_overlay_indices
            .extend(indices.iter().map(|index| base + *index));
    }

    pub(crate) fn take_debug_overlay_geometry(
        &mut self,
    ) -> (
        Vec<super::render_pass::debug_overlay_pass::DebugOverlayVertex>,
        Vec<u32>,
    ) {
        (
            std::mem::take(&mut self.frame.debug_overlay_vertices),
            std::mem::take(&mut self.frame.debug_overlay_indices),
        )
    }

    pub fn frame_parts(&mut self) -> Option<FrameParts<'_>> {
        let frame = self.frame.frame_state.as_mut()?;
        Some(FrameParts {
            encoder: &mut frame.encoder,
            view: &frame.view,
            resolve_view: frame.resolve_view.as_ref(),
            depth_view: frame.depth_view.as_ref(),
        })
    }

    pub fn device(&self) -> Option<&wgpu::Device> {
        self.gpu.device.as_ref()
    }

    pub fn queue(&self) -> Option<&Queue> {
        self.gpu.queue.as_ref()
    }

    /// Format pipelines writing to the surface compile against. Equals the
    /// sRGB variant when the compositor wants linear→sRGB encoding, even if
    /// the underlying canvas storage (`surface_config.format`) is non-sRGB.
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.gpu.surface_target_format
    }

    /// Format for intermediate/offscreen render targets. Stays non-sRGB so
    /// the pipeline performs blending in a single color space; the final
    /// linear→sRGB conversion only happens when writing the surface via
    /// `present_surface_pass`.
    pub fn offscreen_format(&self) -> wgpu::TextureFormat {
        self.gpu.surface_target_format.remove_srgb_suffix()
    }

    pub fn surface_size(&self) -> (u32, u32) {
        (self.gpu.surface_config.width, self.gpu.surface_config.height)
    }

    fn update_logical_size(&mut self, physical_width: u32, physical_height: u32) {
        let scale = self.scale_factor.max(0.0001);
        self.logical_width = (physical_width as f32 / scale).max(1.0);
        self.logical_height = (physical_height as f32 / scale).max(1.0);
    }

    pub fn frame_box_models(&self) -> &[super::base_component::BoxModelSnapshot] {
        &self.compositor.frame_box_models
    }

    pub(crate) fn set_promotion_config(&mut self, config: ViewportPromotionConfig) {
        self.compositor.promotion_config = config;
    }

    pub fn set_focused_node_id(&mut self, node_id: Option<crate::view::node_arena::NodeKey>) {
        self.input_state.focused_node_id = node_id;
    }

    pub fn focused_node_id(&self) -> Option<crate::view::node_arena::NodeKey> {
        self.input_state.focused_node_id
    }

    /// Node currently holding keyboard capture, if any. Returns `None`
    /// when no handler has requested capture via
    /// [`crate::ui::EventViewport::acquire_keyboard_capture`].
    pub fn keyboard_capture_node_id(&self) -> Option<crate::view::node_arena::NodeKey> {
        self.input_state.keyboard_capture_node_id
    }

    /// Target for key / text / IME dispatch: keyboard capture takes
    /// precedence over focus. Used by all `dispatch_key_*` /
    /// `dispatch_text_input_*` / `dispatch_ime_*` entry points.
    pub fn keyboard_dispatch_target(&self) -> Option<crate::view::node_arena::NodeKey> {
        self.input_state
            .keyboard_capture_node_id
            .or(self.input_state.focused_node_id)
    }

    pub fn set_pointer_capture_node_id(&mut self, node_id: Option<crate::view::node_arena::NodeKey>) {
        self.input_state.pointer_capture_node_id = node_id;
    }

    pub fn pointer_capture_node_id(&self) -> Option<crate::view::node_arena::NodeKey> {
        self.input_state.pointer_capture_node_id
    }

}
