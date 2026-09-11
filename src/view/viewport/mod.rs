#![allow(missing_docs)]
use rustc_hash::{FxHashMap, FxHashSet};

#[cfg(test)]
mod artifact_style_pipeline_test_support;
#[cfg(test)]
pub(crate) use artifact_style_pipeline_test_support::layout_artifact_style_scene_for_test;
#[cfg(test)]
mod clipboard_tests;
mod compositor_sync;
mod debug;
pub(crate) mod dispatch;
mod frame;
mod gpu_resources;
pub(crate) use self::gpu_resources::PoolCanonicalArtifactSurfaceResidents;
#[cfg(test)]
mod incremental_tests;
mod input;
mod lifecycle;
mod render;
#[cfg(test)]
pub(crate) use render::{
    ArtifactSurfaceIntermediateReadbackForTest, AutoArtifactSurfaceEmissionForTest,
    emit_retained_auto_artifact_surface_for_test,
};
#[cfg(test)]
mod retained_auto_census_tests;
pub(crate) mod scene_helpers;
#[cfg(any())]
mod tests;
pub(crate) mod transitions_tick;

use crate::style::{ColorLike, Cursor, HexColor, PropertyId, Style};
use crate::time::Instant;
use crate::transition::{
    AnimationPlugin, CHANNEL_LAYOUT_HEIGHT, CHANNEL_LAYOUT_WIDTH, CHANNEL_LAYOUT_X,
    CHANNEL_LAYOUT_Y, CHANNEL_SCROLL_X, CHANNEL_SCROLL_Y, CHANNEL_STYLE_BACKGROUND_COLOR,
    CHANNEL_STYLE_BORDER_BOTTOM_COLOR, CHANNEL_STYLE_BORDER_LEFT_COLOR,
    CHANNEL_STYLE_BORDER_RADIUS, CHANNEL_STYLE_BORDER_RIGHT_COLOR, CHANNEL_STYLE_BORDER_TOP_COLOR,
    CHANNEL_STYLE_BOX_SHADOW, CHANNEL_STYLE_COLOR, CHANNEL_STYLE_OPACITY, CHANNEL_STYLE_TRANSFORM,
    CHANNEL_STYLE_TRANSFORM_ORIGIN, CHANNEL_VISUAL_X, CHANNEL_VISUAL_Y, ChannelId, ClaimMode,
    LayoutTransitionPlugin, ScrollAxis, ScrollTransition, ScrollTransitionPlugin, StyleField,
    StyleTransitionPlugin, StyleValue, TrackKey, TrackTarget, Transition, TransitionFrame,
    TransitionHost, TransitionPluginId, VisualTransitionPlugin,
};
use crate::ui::{
    BlurEvent, ClickEvent, EventCommand, EventMeta, FocusEvent, FromPropValue, ImePreeditEvent,
    KeyDownEvent, KeyEventData, KeyUpEvent, NodeId, Patch, PointerButtons as UiPointerButtons,
    PointerDownEvent, PointerEventData, PointerMoveEvent, PointerUpEvent, PropValue, RsxNode,
    TextInputEvent, peek_state_dirty, reconcile, take_state_dirty,
};
use crate::view::ElementStylePropSchema;
use crate::view::frame_graph::texture_resource::TextureDesc;
use crate::view::frame_graph::{AllocationId, BufferDesc, FrameGraph};
use crate::view::render_pass::render_target::{OffscreenRenderTargetPool, RenderTargetBundle};

use std::ops::Sub;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use wgpu::util::StagingBelt;
use wgpu::{
    Instance, Queue, TextureUsages,
    rwh::{HasDisplayHandle, HasWindowHandle},
};

use self::debug::{
    PostLayoutTransitionResult, TraceRenderNode, build_compile_trace_nodes,
    build_debug_overlay_geometry, build_execute_detail_trace_nodes, build_layout_place_trace_nodes,
    build_text_measure_trace_nodes, format_trace_render_tree, style_field_requires_relayout,
};
pub use self::dispatch::{
    dispatch_click_from_hit_test, dispatch_pointer_down_from_hit_test,
    dispatch_pointer_move_from_hit_test, dispatch_pointer_up_from_hit_test,
    dispatch_scroll_from_hit_test, get_scroll_offset_by_id, nearest_viewport_clip_ancestor_id,
    set_scroll_offset_by_id,
};
pub use self::frame::FrameParts;
use self::frame::{
    BeginFrameProfile, EndFrameProfile, FrameDisposition, FrameState, FrameStats, FrameTimings,
    LayoutPassResult,
};
use self::input::{DragState, InputState, PendingClick, is_valid_click_candidate};
pub use self::input::{PointerButton, ViewportDebugOptions};
use self::transitions_tick::{TransitionHostAdapter, active_channels_by_node};
use crate::app::App;
use crate::platform::{
    Modifiers, PlatformImePreedit, PlatformKeyEvent, PlatformPointerEvent,
    PlatformPointerEventKind, PlatformRequests, PlatformTextInput, PlatformWheelEvent, PointerType,
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

/// Selects the complete recorded-artifact renderer or the original Legacy renderer.
/// RetainedAuto falls back to whole-frame Legacy when recording or preparation
/// cannot certify the complete frame.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ViewportPaintRendererMode {
    Legacy,
    #[default]
    RetainedAuto,
}

/// Terminal frame-graph failure that permanently routes a requested
/// `RetainedAuto` viewport through whole-frame Legacy until an explicit mode
/// setter call resets the circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetainedAutoTerminalFailureStage {
    Compile,
    Execute,
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

    pub fn set_paint_renderer_mode(&mut self, mode: ViewportPaintRendererMode) {
        self.viewport.set_paint_renderer_mode(mode);
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
    paint_renderer_mode: ViewportPaintRendererMode,
    /// First terminal RetainedAuto failure. Selection observes this before any
    /// authority-specific graph mutation; it is never cleared by a successful
    /// Legacy recovery frame.
    retained_auto_terminal_failure: Option<RetainedAutoTerminalFailureStage>,
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
    app: Option<Box<dyn App>>,
    cached_rsx: Option<RsxNode>,
    needs_rebuild: bool,
    ready_dispatched: bool,
}

impl Drop for Viewport {
    fn drop(&mut self) {
        // Only release the process-wide cache entries owned by this Viewport.
        // Calling `release_render_resource_caches` here would incorrectly clear
        // unrelated global pass caches still used by other Viewports.
        crate::view::render_pass::texture_composite_pass::clear_texture_composite_resources_cache(
            self.render_resource_scope_id(),
        );
    }
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
    /// Interaction-ordered stack of viewport-clip absolute nodes. Single
    /// source of truth for both deferred render order and pointer
    /// hit-test priority. See [`crate::view::popup_stack::PopupStack`].
    popup_stack: super::popup_stack::PopupStack,
    scroll_offsets: FxHashMap<u64, (f32, f32)>,
    last_rsx_root: Option<RsxNode>,
    /// Incremental Fiber-commit (`FiberWork`) switch. It is enabled by
    /// default; `render_rsx` attempts the incremental path for eligible
    /// updates and falls back to the full rebuild pipeline whenever
    /// translation or application is not safe.
    use_incremental_commit: bool,
}

impl SceneState {
    fn new() -> Self {
        Self {
            node_arena: super::node_arena::NodeArena::new(),
            ui_root_keys: Vec::new(),
            popup_stack: super::popup_stack::PopupStack::new(),
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

    fn refresh_roots_for_cold_rebuild_after_incremental_failure(&mut self) {
        let mut cleanup_roots = self.node_arena.roots().to_vec();
        for stale_root in &self.ui_root_keys {
            if self.node_arena.contains_key(*stale_root) && !cleanup_roots.contains(stale_root) {
                cleanup_roots.push(*stale_root);
            }
        }
        self.ui_root_keys = cleanup_roots;
    }
}

/// Phase-7 extraction. Everything scoped to a single render frame: the
/// per-frame state, pooled GPU allocations, frame-graph cache, and debug
/// overlay geometry buffers. Non-pub; the viewport re-exposes whatever the
/// outside world needs through existing accessor methods.
struct FrameRuntime {
    frame_state: Option<FrameState>,
    offscreen_render_target_pool: OffscreenRenderTargetPool,
    sampled_texture_cache:
        FxHashMap<crate::view::sampled_texture::SampledTextureId, SampledTextureEntry>,
    sampled_texture_upload_count: u64,
    frame_buffer_pool: FxHashMap<u32, FrameBufferEntry>,
    draw_rect_uniform_pool: Vec<DrawRectUniformBufferEntry>,
    draw_rect_uniform_cursor: usize,
    draw_rect_uniform_offset: u64,
    gradient_stops_buffer: Option<GradientStopsBufferEntry>,
    gradient_stops_byte_cursor: u64,
    frame_stats: FrameStats,
    frame_presented: bool,
    #[cfg(test)]
    completion_counts: FrameCompletionCounts,
    last_frame_graph: Option<FrameGraph>,
    compile_cache: Option<CachedCompiledGraph>,
    debug_overlay_vertices: Vec<super::render_pass::debug_overlay_pass::DebugOverlayVertex>,
    debug_overlay_indices: Vec<u32>,
    last_retained_auto_debug: Option<crate::view::debug::DebugRetainedAutoCaptureInput>,
    /// Stash for `App::build()` elapsed time (ms) so the render trace tree
    /// can include RSX build cost.  Set in `render_frame`, consumed in
    /// `render_render_tree`.
    rsx_build_ms: f64,
    frame_number: u64,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct FrameCompletionCounts {
    submits: u64,
    presents: u64,
    aborts: u64,
}

impl FrameRuntime {
    fn new(trace_fps: bool) -> Self {
        Self {
            frame_state: None,
            offscreen_render_target_pool: OffscreenRenderTargetPool::new(),
            sampled_texture_cache: FxHashMap::default(),
            sampled_texture_upload_count: 0,
            frame_buffer_pool: FxHashMap::default(),
            draw_rect_uniform_pool: Vec::new(),
            draw_rect_uniform_cursor: 0,
            draw_rect_uniform_offset: 0,
            gradient_stops_buffer: None,
            gradient_stops_byte_cursor: 0,
            frame_stats: FrameStats::new(trace_fps),
            frame_presented: false,
            #[cfg(test)]
            completion_counts: FrameCompletionCounts::default(),
            last_frame_graph: None,
            compile_cache: None,
            debug_overlay_vertices: Vec::new(),
            debug_overlay_indices: Vec::new(),
            last_retained_auto_debug: None,
            rsx_build_ms: 0.0,
            frame_number: 0,
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
    /// Whether `transition_claims` was empty at the previous runtime-state
    /// reconcile; lets idle frames skip the whole-tree reconcile walk.
    claims_were_empty: bool,
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
            claims_were_empty: false,
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
    render_resource_scope_id: u64,
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
    #[cfg(not(target_arch = "wasm32"))]
    in_flight_submissions: std::collections::VecDeque<wgpu::SubmissionIndex>,
}

fn allocate_render_resource_scope_id(counter: &AtomicU64) -> u64 {
    let scope_id = counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .expect("render resource scope ID space exhausted");
    assert_ne!(scope_id, 0, "render resource scope allocator emitted zero");
    scope_id
}

fn next_render_resource_scope_id() -> u64 {
    static NEXT_RENDER_RESOURCE_SCOPE_ID: AtomicU64 = AtomicU64::new(1);
    allocate_render_resource_scope_id(&NEXT_RENDER_RESOURCE_SCOPE_ID)
}

#[cfg(test)]
mod render_resource_scope_id_tests {
    use super::allocate_render_resource_scope_id;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn allocator_is_non_zero_and_monotonic() {
        let counter = AtomicU64::new(1);
        assert_eq!(allocate_render_resource_scope_id(&counter), 1);
        assert_eq!(allocate_render_resource_scope_id(&counter), 2);
    }

    #[test]
    #[should_panic(expected = "render resource scope allocator emitted zero")]
    fn allocator_rejects_zero() {
        let counter = AtomicU64::new(0);
        let _ = allocate_render_resource_scope_id(&counter);
    }

    #[test]
    #[should_panic(expected = "render resource scope ID space exhausted")]
    fn allocator_fails_closed_at_exhaustion() {
        let counter = AtomicU64::new(u64::MAX);
        let _ = allocate_render_resource_scope_id(&counter);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RetainedSurfaceResidentState {
    entries: FxHashMap<
        crate::view::paint::RetainedSurfaceResidentKey,
        crate::view::paint::RetainedSurfaceRasterStamp,
    >,
}

impl RetainedSurfaceResidentState {
    #[cfg(test)]
    fn compile_action(
        &self,
        stamp: &crate::view::paint::RetainedSurfaceRasterStamp,
        key: crate::view::frame_graph::PersistentTextureKey,
        pair_compatible: bool,
    ) -> crate::view::paint::RetainedSurfaceCompileAction {
        let resident_key = stamp.identity.resident_key();
        if pair_compatible
            && stamp
                .target
                .has_canonical_descriptor_pair_for(stamp.identity)
            && key == stamp.identity.color_key
            && self.entries.get(&resident_key) == Some(stamp)
        {
            crate::view::paint::RetainedSurfaceCompileAction::Reuse
        } else {
            crate::view::paint::RetainedSurfaceCompileAction::Reraster
        }
    }
}

#[cfg(test)]
pub(crate) fn retained_surface_compile_action_against_resident_for_test(
    resident: crate::view::paint::RetainedSurfaceRasterStamp,
    candidate: &crate::view::paint::RetainedSurfaceRasterStamp,
) -> crate::view::paint::RetainedSurfaceCompileAction {
    let mut state = RetainedSurfaceResidentState::default();
    state
        .entries
        .insert(resident.identity.resident_key(), resident);
    state.compile_action(candidate, candidate.identity.color_key, true)
}

#[derive(Debug, PartialEq, Eq)]
enum PendingRetainedSurfaceTransaction {
    /// Compiler-sealed keys are preserved through pool staging and commit.
    CommitArtifactSurfaceSet {
        residents: PoolCanonicalArtifactSurfaceResidents,
    },
    Clear,
}

/// Opaque per-frame ownership proof for the shared retained staging slot.
/// The generation is private so a renderer can only finish the transaction
/// reserved by its matching `begin_retained_surface_frame_stage` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetainedSurfaceFrameStageOwner {
    generation: u64,
}

struct CompositorState {
    property_trees: crate::view::compositor::PropertyTrees,
    paint_generations: crate::view::compositor::PaintGenerationTracker,
    frame_box_models: Vec<super::base_component::BoxModelSnapshot>,
    frame_box_model_cache:
        FxHashMap<crate::view::node_arena::NodeKey, Vec<super::base_component::BoxModelSnapshot>>,
    retained_surfaces: RetainedSurfaceResidentState,
    pending_retained_surfaces: Option<PendingRetainedSurfaceTransaction>,
    pending_retained_surface_owner: Option<u64>,
    active_retained_surface_frame_owner: Option<u64>,
    next_retained_surface_owner: u64,
    #[cfg(test)]
    retained_surface_release_log: Vec<crate::view::frame_graph::PersistentTextureKey>,
    /// Test-only stand-in for the GPU pool's resident pair after a forced
    /// graph has been declared successful. It is produced only by
    /// `finish_retained_surface_transaction(true)` and is cleared by every
    /// production invalidation path.
    #[cfg(test)]
    retained_surface_pair_witnesses: FxHashSet<crate::view::frame_graph::PersistentTextureKey>,
    #[cfg(test)]
    box_model_refresh_stats: BoxModelRefreshStats,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct BoxModelRefreshStats {
    collected_roots: usize,
    reused_roots: usize,
    collected_snapshots: usize,
    reused_snapshots: usize,
}

impl CompositorState {
    fn new() -> Self {
        Self {
            property_trees: crate::view::compositor::PropertyTrees::default(),
            paint_generations: crate::view::compositor::PaintGenerationTracker::default(),
            frame_box_models: Vec::new(),
            frame_box_model_cache: FxHashMap::default(),
            retained_surfaces: RetainedSurfaceResidentState::default(),
            pending_retained_surfaces: None,
            pending_retained_surface_owner: None,
            active_retained_surface_frame_owner: None,
            next_retained_surface_owner: 1,
            #[cfg(test)]
            retained_surface_release_log: Vec::new(),
            #[cfg(test)]
            retained_surface_pair_witnesses: FxHashSet::default(),
            #[cfg(test)]
            box_model_refresh_stats: BoxModelRefreshStats::default(),
        }
    }
}

struct CachedCompiledGraph {
    topology_key: crate::view::frame_graph::TopologyCacheKey,
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
    pub(super) last_used_frame: u64,
    /// Cached bind groups keyed by layout_cache_key.  The bind group binds the buffer
    /// at offset 0 / size=slot_size; the per-draw dynamic offset is supplied separately,
    /// so one bind group is valid for *all* slots in this buffer.
    pub(super) bind_groups: FxHashMap<u64, wgpu::BindGroup>,
}

pub(super) struct GradientStopsBufferEntry {
    pub(super) buffer: wgpu::Buffer,
    pub(super) size: u64,
    pub(super) last_used_frame: u64,
    pub(super) last_high_usage_frame: u64,
}

pub(super) struct SampledTextureEntry {
    pub(super) texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) format: wgpu::TextureFormat,
    pub(super) alpha_mode: crate::view::sampled_texture::SampledTextureAlphaMode,
    pub(super) generation: u64,
    pub(super) byte_size: u64,
    pub(super) last_used_frame: u64,
}

impl Viewport {
    const DEFAULT_MSAA_SAMPLE_COUNT: u32 = 4;
    /// Skia GrResourceCache default: 96 MB.
    const SAMPLED_TEXTURE_PRESSURE_BYTES: u64 = 96 * 1024 * 1024;
    const SAMPLED_TEXTURE_EVICT_TO_BYTES: u64 = 72 * 1024 * 1024;
    /// Evict textures that have not actually been sampled for this many frames.
    const SAMPLED_TEXTURE_STALE_FRAMES: u64 = 300;

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
                render_resource_scope_id: next_render_resource_scope_id(),
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
                    color_space: wgpu::SurfaceColorSpace::Auto,
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
                #[cfg(not(target_arch = "wasm32"))]
                in_flight_submissions: std::collections::VecDeque::new(),
            },
            frame: FrameRuntime::new(debug_options.trace_fps),
            pending_size: None,
            needs_reconfigure: false,
            redraw_requested: false,
            debug_options,
            paint_renderer_mode: ViewportPaintRendererMode::default(),
            retained_auto_terminal_failure: None,
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
            app: None,
            cached_rsx: None,
            needs_rebuild: true,
            ready_dispatched: false,
        }
    }

    /// Toggle the incremental Fiber commit path. When enabled,
    /// `render_rsx` supports eligible Create/Delete/Move/Replace/Update/
    /// SetText work and falls back to a full rebuild when a patch cannot
    /// be translated or applied safely.
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

    /// Returns the current production paint rollout mode.
    pub fn paint_renderer_mode(&self) -> ViewportPaintRendererMode {
        self.paint_renderer_mode
    }

    /// Selects the production paint renderer mode. The default is
    /// [`ViewportPaintRendererMode::RetainedAuto`], while explicit Legacy mode
    /// remains available as a diagnostic and compatibility override. Automatic
    /// retained authority never permits per-root mixing with the legacy
    /// renderer. Calling this with the already-requested `RetainedAuto` mode
    /// manually resets an open terminal circuit breaker; ordinary same-mode
    /// calls remain no-ops.
    pub fn set_paint_renderer_mode(&mut self, mode: ViewportPaintRendererMode) {
        if self.paint_renderer_mode == mode && self.retained_auto_terminal_failure.is_none() {
            return;
        }
        self.invalidate_retained_surfaces();
        self.frame.compile_cache = None;
        self.frame.last_retained_auto_debug = None;
        self.paint_renderer_mode = mode;
        self.retained_auto_terminal_failure = None;
        self.request_redraw();
    }

    fn arm_retained_auto_terminal_failure(
        &mut self,
        stage: RetainedAutoTerminalFailureStage,
    ) -> bool {
        if self.paint_renderer_mode != ViewportPaintRendererMode::RetainedAuto
            || self.retained_auto_terminal_failure.is_some()
        {
            return false;
        }
        self.retained_auto_terminal_failure = Some(stage);
        self.request_redraw();
        true
    }

    pub fn capture_debug(
        &self,
        options: crate::view::debug::DebugCaptureOptions,
    ) -> crate::view::debug::DebugCapture {
        let retained_auto = options
            .include_retained_auto
            .then(|| self.frame.last_retained_auto_debug.clone())
            .flatten();
        crate::view::debug::DebugCapture::from_arena_with_retained_auto(
            options,
            &self.scene.node_arena,
            &self.scene.ui_root_keys,
            crate::view::debug::DebugViewportCaptureInput {
                logical_size: self.logical_size(),
                scale_factor: self.scale_factor(),
                focused_node: self.focused_node_id(),
                hovered_node: self.hovered_node_id(),
                pointer_capture_node: self.pointer_capture_node_id(),
                keyboard_capture_node: self.keyboard_capture_node_id(),
                pointer_position: self.pointer_position_viewport(),
                pressed_pointer_buttons: self.pressed_pointer_buttons().collect(),
            },
            retained_auto,
        )
    }

    /// Aggregate the last captured `RetainedAuto` attempt into a fallback
    /// census.
    ///
    /// Returns `None` when no attempt has been captured. Capture requires one
    /// of `retained_auto_census`, `retained_auto_overlay`, or
    /// `trace_render_time` in [`ViewportDebugOptions`]; the census-only flag
    /// exists so the overlay does not cover the scene being censused.
    ///
    /// The result describes the last attempt known to the viewport, which is
    /// independent from the frame currently on screen.
    pub fn capture_retained_auto_census(
        &self,
    ) -> Option<crate::view::debug::census::DebugFallbackCensus> {
        let options = crate::view::debug::DebugCaptureOptions {
            include_arena: false,
            include_layout: false,
            include_style: false,
            include_interaction: false,
            include_dirty: false,
            include_render: false,
            include_retained_auto: true,
            include_component: false,
        };
        self.capture_debug(options)
            .document()
            .viewport
            .retained_auto
            .as_ref()
            .map(crate::view::debug::census::DebugFallbackCensus::from_snapshot)
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
        self.debug_options.geometry_overlay || self.debug_options.retained_auto_overlay
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
        self.frame
            .debug_overlay_vertices
            .extend_from_slice(vertices);
        self.frame
            .debug_overlay_indices
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

    pub(crate) fn render_resource_scope_id(&self) -> u64 {
        self.gpu.render_resource_scope_id
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

    /// Format for intermediate/offscreen render targets. Matches the surface
    /// (sRGB-suffixed when the surface is sRGB) so 8-bit storage keeps dark
    /// precision. HW auto-decodes sampled values to linear and auto-encodes
    /// stored values, so blending math still runs in linear space.
    pub fn offscreen_format(&self) -> wgpu::TextureFormat {
        self.gpu.surface_target_format
    }

    pub fn surface_size(&self) -> (u32, u32) {
        (
            self.gpu.surface_config.width,
            self.gpu.surface_config.height,
        )
    }

    fn update_logical_size(&mut self, physical_width: u32, physical_height: u32) {
        let scale = self.scale_factor.max(0.0001);
        self.logical_width = (physical_width as f32 / scale).max(1.0);
        self.logical_height = (physical_height as f32 / scale).max(1.0);
    }

    pub fn frame_box_models(&self) -> &[super::base_component::BoxModelSnapshot] {
        &self.compositor.frame_box_models
    }

    #[cfg(test)]
    fn box_model_refresh_stats(&self) -> BoxModelRefreshStats {
        self.compositor.box_model_refresh_stats
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

    pub fn set_pointer_capture_node_id(
        &mut self,
        node_id: Option<crate::view::node_arena::NodeKey>,
    ) {
        self.input_state.pointer_capture_node_id = node_id;
    }

    pub fn pointer_capture_node_id(&self) -> Option<crate::view::node_arena::NodeKey> {
        self.input_state.pointer_capture_node_id
    }

    /// Node the pointer is currently hovering, if any. Used by
    /// [`crate::ui::EventTarget::state`] to report hover state back to
    /// handlers.
    pub fn hovered_node_id(&self) -> Option<crate::view::node_arena::NodeKey> {
        self.input_state.hovered_node_id
    }

    /// Shared read access to the node arena. Used by
    /// [`crate::ui::EventTarget`] lazy accessors (parent / ancestors /
    /// contains / state) to walk the tree without going through
    /// `ViewportControl`.
    pub fn node_arena(&self) -> &crate::view::node_arena::NodeArena {
        &self.scene.node_arena
    }

    /// Split the viewport into shared access to the arena and a
    /// [`ViewportControl`] holding `&mut self`. Used at every dispatch
    /// entry so bubble functions can walk `&NodeArena` while handlers
    /// mutate non-arena state (input / transitions / gpu / …) via
    /// `ViewportControl`.
    ///
    /// # Safety invariant
    ///
    /// `ViewportControl` must never touch `scene.node_arena` during a
    /// dispatch. Every current `ViewportControl` method mutates only
    /// disjoint fields (`input_state`, `transitions`, `redraw_requested`,
    /// `clipboard_fallback`, `debug_options`, `compositor`, `gpu`). New
    /// methods must preserve this invariant, otherwise the aliasing
    /// `&NodeArena` returned here becomes unsound.
    pub(crate) fn borrow_for_dispatch(
        &mut self,
    ) -> (&crate::view::node_arena::NodeArena, ViewportControl<'_>) {
        // SAFETY: we hand out `&NodeArena` derived from the same `&mut self`
        // that backs the returned `ViewportControl`. Soundness relies on
        // `ViewportControl` only mutating disjoint fields (audited above).
        // We take the shared reference via a raw pointer so Rust's borrow
        // checker does not see an overlap with the subsequent `&mut self`
        // reborrow inside `ViewportControl::new`.
        let arena_ptr: *const crate::view::node_arena::NodeArena = &self.scene.node_arena;
        let control = ViewportControl::new(self);
        // SAFETY: `arena_ptr` points into `self.scene`, which lives for
        // the returned `'a` lifetime (tied to the input `&mut self`).
        let arena = unsafe { &*arena_ptr };
        (arena, control)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) use render::SingleViewportFrameObservation;
