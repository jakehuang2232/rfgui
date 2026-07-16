#![allow(missing_docs)]
use rustc_hash::{FxHashMap, FxHashSet};

#[cfg(test)]
mod clipboard_tests;
mod debug;
pub(crate) mod dispatch;
mod frame;
mod gpu_resources;
#[cfg(test)]
mod incremental_tests;
mod input;
mod lifecycle;
mod promotion_runtime;
mod render;
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
use crate::view::base_component::Renderable;
use crate::view::frame_graph::texture_resource::TextureDesc;
use crate::view::frame_graph::{AllocationId, BufferDesc, FrameGraph};
use crate::view::promotion::{
    PromotedLayerUpdate, PromotedLayerUpdateKind, PromotionDecision, PromotionState,
    ViewportPromotionConfig, active_channels_by_node, evaluate_promotion,
};
use crate::view::promotion_builder::{
    collect_promoted_layer_updates_with_generations, collect_promotion_candidates,
};
use crate::view::render_pass::render_target::{OffscreenRenderTargetPool, RenderTargetBundle};

use std::ops::Sub;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use wgpu::util::StagingBelt;
use wgpu::{
    Instance, Queue, TextureUsages,
    rwh::{HasDisplayHandle, HasWindowHandle},
};

pub(crate) use self::debug::{
    DebugReusePathContext, DebugReusePathRecord, begin_debug_reuse_path_frame,
    record_debug_reuse_path, set_debug_trace_enabled,
};
use self::debug::{
    DebugStyleSampleRecord, PostLayoutTransitionResult, TraceRenderNode, build_compile_trace_nodes,
    build_execute_detail_trace_nodes, build_layout_place_trace_nodes, build_reuse_overlay_geometry,
    build_text_measure_trace_nodes, format_promotion_trace, format_reuse_path_trace,
    format_style_field, format_style_promotion_trace, format_style_request_trace,
    format_style_sample_trace, format_style_value, format_trace_render_tree,
    record_debug_style_promotion, record_debug_style_request, record_debug_style_sample,
    record_debug_style_sample_record, reuse_overlay_color, style_field_requires_relayout,
    take_debug_reuse_path, take_debug_style_sample_records, trace_promoted_build_frame_marker,
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
use self::transitions_tick::TransitionHostAdapter;
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

/// Selects the viewport's production paint authority during the staged
/// artifact-renderer rollout.
///
/// `ArtifactCanary` is intentionally fail-closed: only an entirely
/// property-neutral, promotion-free, deferred-free frame uses the artifact
/// compiler. Every other frame stays wholly on the legacy renderer.
/// `RetainedTransformCanary` is a separate opt-in authority for the exact
/// single-root transform-surface contract; it never falls through to the
/// generic artifact canary. `RetainedSurfaceTreeCanary` independently opts
/// into the exact root-plus-one-direct-child retained-surface tree contract.
/// `RetainedEffectTreeCanary` is the independent exact Transform ->
/// NestedIsolation authority and does not widen either tree canary.
/// `RetainedScrollHostCanary` independently owns the exact single-root baked
/// vertical scroll-host contract; it never enters a transform/effect canary.
/// `RetainedScrollSceneCanary` is the independent detached-content authority;
/// it retains only the offset-zero content raster and paints host/overlay into
/// the parent target every frame.
/// `RetainedAuto` is the bounded M11A opt-in router. It selects exactly one of
/// the existing retained/artifact authorities before the common clear and
/// falls back to the whole-frame legacy renderer when that selected authority
/// cannot be prepared or compiled.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ViewportPaintRendererMode {
    /// Keep the established immediate legacy build path authoritative.
    #[default]
    Legacy,
    /// Opt into the M6A whole-frame property-neutral artifact canary.
    ArtifactCanary,
    /// Opt into the M10C retained single-root transform-surface canary.
    RetainedTransformCanary,
    /// Opt into the M10C5 exact depth-two retained-surface tree canary.
    RetainedSurfaceTreeCanary,
    /// Opt into the M9F3 exact root-opacity typed isolation canary.
    RetainedIsolationCanary,
    /// Opt into the M10D exact root-transform/direct-child-isolation tree canary.
    RetainedEffectTreeCanary,
    /// Opt into the M10E1A exact single-root retained scroll-host canary.
    RetainedScrollHostCanary,
    /// Opt into the M10E2A3 exact detached scroll-scene canary.
    RetainedScrollSceneCanary,
    /// Opt into the M11A whole-frame automatic retained authority router.
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

/// Phase-7 extraction. Groups everything the layer-promotion / compositor
/// pipeline owns — promotion state, cached signatures for reuse, and the
/// box-model snapshots produced each frame. Pure internal refactor; the
/// viewport public API is unchanged and nothing outside the viewport names
/// this type.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum RootEffectRetainedState {
    #[default]
    Invalid,
    Resident {
        stamp: crate::view::paint::RootEffectRasterStamp,
        key: crate::view::frame_graph::PersistentTextureKey,
    },
}

impl RootEffectRetainedState {
    fn compile_action(
        &self,
        stamp: &crate::view::paint::RootEffectRasterStamp,
        key: crate::view::frame_graph::PersistentTextureKey,
        pool_compatible: bool,
    ) -> crate::view::paint::RootEffectCompileAction {
        if pool_compatible
            && matches!(
                self,
                Self::Resident {
                    stamp: resident_stamp,
                    key: resident_key,
                } if resident_stamp == stamp && *resident_key == key
            )
        {
            crate::view::paint::RootEffectCompileAction::Reuse
        } else {
            crate::view::paint::RootEffectCompileAction::Reraster
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PendingRootEffectTransaction {
    Commit {
        stamp: crate::view::paint::RootEffectRasterStamp,
        key: crate::view::frame_graph::PersistentTextureKey,
        action: crate::view::paint::RootEffectCompileAction,
    },
    Clear,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RetainedSurfaceResidentState {
    entries: FxHashMap<
        crate::view::paint::RetainedSurfaceResidentKey,
        crate::view::paint::RetainedSurfaceRasterStamp,
    >,
    scroll_tiles: ScrollTileResidentCache,
    property_scroll: PropertyScrollResidentCache,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScrollTileResidentBudget {
    max_tiles: usize,
    max_pair_bytes: u64,
    max_idle_frames: u64,
}

impl ScrollTileResidentBudget {
    fn new(max_tiles: usize, max_pair_bytes: u64, max_idle_frames: u64) -> Option<Self> {
        (max_tiles > 0 && max_pair_bytes > 0).then_some(Self {
            max_tiles,
            max_pair_bytes,
            max_idle_frames,
        })
    }
}

impl Default for ScrollTileResidentBudget {
    fn default() -> Self {
        // Canary policy, not a GPU-pool capacity claim. The active scene is
        // admitted separately; this only bounds inactive tile retention.
        Self::new(32, 128 * 1024 * 1024, 120).expect("scroll tile budget is non-zero")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScrollTileContentGroup {
    content_root: crate::view::node_arena::NodeKey,
    content_stable_id: u64,
    content_bounds: [u32; 4],
    tile_edge: u32,
    gutter: u32,
    overscan: u32,
    scale_factor_bits: u32,
    color_format: wgpu::TextureFormat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScrollTileResidentEntry {
    stamp: crate::view::paint::RetainedSurfaceRasterStamp,
    last_used_frame: u64,
    pair_bytes: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ScrollTileResidentCache {
    group: Option<ScrollTileContentGroup>,
    entries: FxHashMap<crate::view::paint::RetainedSurfaceResidentKey, ScrollTileResidentEntry>,
    active: FxHashSet<crate::view::paint::RetainedSurfaceResidentKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PropertyScrollResidentGroupKey {
    content_root: crate::view::node_arena::NodeKey,
    content_stable_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyScrollResidentEntry {
    stamp: crate::view::paint::RetainedSurfaceRasterStamp,
    last_used_frame: u64,
    pair_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyScrollResidentGroup {
    signature: crate::view::paint::RetainedPropertyScrollGroupSignature,
    backing_rank: u8,
    entries: FxHashMap<crate::view::paint::RetainedSurfaceResidentKey, PropertyScrollResidentEntry>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PropertyScrollResidentCache {
    groups: FxHashMap<PropertyScrollResidentGroupKey, PropertyScrollResidentGroup>,
    /// Union of every group's active resident keys in the committed scene.
    active: FxHashSet<crate::view::paint::RetainedSurfaceResidentKey>,
}

impl RetainedSurfaceResidentState {
    #[allow(dead_code)] // C3 state/lifecycle landed before the C4 producer requests an action.
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
            && self.entries.get(&resident_key).or_else(|| {
                self.scroll_tiles
                    .entries
                    .get(&resident_key)
                    .map(|entry| &entry.stamp)
            }) == Some(stamp)
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

#[allow(dead_code)] // C3 transaction consumer lands before the C4 full-frame producer.
#[derive(Clone, Debug, PartialEq, Eq)]
enum PendingRetainedSurfaceTransaction {
    Commit {
        full_set: FxHashMap<
            crate::view::paint::RetainedSurfaceResidentKey,
            crate::view::paint::RetainedSurfaceRasterStamp,
        >,
    },
    CommitScrollTileActiveSet {
        manifest: crate::view::paint::ScrollContentTileSetTransactionStamp,
        active_set: FxHashMap<
            crate::view::paint::RetainedSurfaceResidentKey,
            crate::view::paint::RetainedSurfaceRasterStamp,
        >,
    },
    /// Exact ordered multi-root property-scene transaction. This deliberately
    /// does not reuse the generic full-set validator: that validator seals the
    /// older single-root, depth-1 canaries, while this capability is admitted
    /// only by the compiler-produced property-scene transaction stamp.
    CommitPropertyScene {
        transaction: crate::view::paint::RetainedPropertySceneTransaction,
        full_set: Vec<crate::view::paint::RetainedSurfaceRasterStamp>,
    },
    /// Unified exact property-scene transaction containing both generic
    /// retained surfaces and any number of structurally sealed scroll groups.
    CommitPropertyScrollScene {
        transaction: crate::view::paint::RetainedPropertyScrollSceneTransaction,
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
    shadow_layer_tree: crate::view::compositor::LayerTree,
    raster_cache: crate::view::compositor::RasterCache,
    raster_budget_readiness: crate::view::compositor::raster_cache::ShadowRasterBudgetReadiness,
    prospective_raster_plan: Option<crate::view::compositor::raster_cache::ProspectiveRasterPlan>,
    raster_plan_parity: crate::view::compositor::raster_cache::RasterPlanParity,
    shadow_promotion_evaluation: crate::view::compositor::raster_cache::ShadowPromotionEvaluation,
    shadow_promotion_policy_state: crate::view::promotion::ShadowPromotionPolicyState,
    shadow_policy_config: crate::view::promotion::ShadowPolicyConfig,
    shadow_rollout_safety: crate::view::compositor::raster_cache::ShadowRolloutSafetyState,
    promotion_state: PromotionState,
    promotion_config: ViewportPromotionConfig,
    promoted_layer_updates: Vec<PromotedLayerUpdate>,
    promoted_base_signatures: FxHashMap<u64, u64>,
    promoted_composition_signatures: FxHashMap<u64, u64>,
    promoted_base_generations: FxHashMap<crate::view::node_arena::NodeKey, u64>,
    promoted_composition_generations: FxHashMap<crate::view::node_arena::NodeKey, u64>,
    debug_previous_subtree_signatures: FxHashMap<u64, (u64, u64, u64, bool)>,
    promoted_reuse_cooldown_frames: u8,
    frame_box_models: Vec<super::base_component::BoxModelSnapshot>,
    frame_box_model_cache:
        FxHashMap<crate::view::node_arena::NodeKey, Vec<super::base_component::BoxModelSnapshot>>,
    root_effect_retained: RootEffectRetainedState,
    pending_root_effect: Option<PendingRootEffectTransaction>,
    retained_surfaces: RetainedSurfaceResidentState,
    scroll_tile_resident_budget: ScrollTileResidentBudget,
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
            shadow_layer_tree: crate::view::compositor::LayerTree::default(),
            raster_cache: crate::view::compositor::RasterCache::default(),
            raster_budget_readiness:
                crate::view::compositor::raster_cache::ShadowRasterBudgetReadiness::default(),
            prospective_raster_plan: None,
            raster_plan_parity: crate::view::compositor::raster_cache::RasterPlanParity::default(),
            shadow_promotion_evaluation:
                crate::view::compositor::raster_cache::ShadowPromotionEvaluation::default(),
            shadow_promotion_policy_state:
                crate::view::promotion::ShadowPromotionPolicyState::default(),
            shadow_policy_config: crate::view::promotion::ShadowPolicyConfig::default(),
            shadow_rollout_safety:
                crate::view::compositor::raster_cache::ShadowRolloutSafetyState::default(),
            promotion_state: PromotionState::default(),
            promotion_config: ViewportPromotionConfig::default(),
            promoted_layer_updates: Vec::new(),
            promoted_base_signatures: FxHashMap::default(),
            promoted_composition_signatures: FxHashMap::default(),
            promoted_base_generations: FxHashMap::default(),
            promoted_composition_generations: FxHashMap::default(),
            debug_previous_subtree_signatures: FxHashMap::default(),
            promoted_reuse_cooldown_frames: 0,
            frame_box_models: Vec::new(),
            frame_box_model_cache: FxHashMap::default(),
            root_effect_retained: RootEffectRetainedState::Invalid,
            pending_root_effect: None,
            retained_surfaces: RetainedSurfaceResidentState::default(),
            scroll_tile_resident_budget: ScrollTileResidentBudget::default(),
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

#[cfg(test)]
mod root_effect_retained_tests {
    use super::*;

    fn stamp(
        root: crate::view::node_arena::NodeKey,
        width: u32,
        scale: f32,
    ) -> crate::view::paint::RootEffectRasterStamp {
        crate::view::paint::RootEffectRasterStamp {
            root,
            target: crate::view::paint::RootEffectRasterInputs {
                width,
                height: 240,
                format: wgpu::TextureFormat::Bgra8Unorm,
                sample_count: 1,
                scale_factor_bits: scale.to_bits(),
            },
            owner_topology: Vec::new(),
            clip_nodes: Vec::new(),
            chunks: Vec::new(),
            op_count: 0,
        }
    }

    pub(super) fn tile_active_set(
        root: crate::view::node_arena::NodeKey,
        stable_id: u64,
        contents_clip: [u32; 4],
    ) -> (
        crate::view::paint::ScrollContentTileSetTransactionStamp,
        Vec<crate::view::paint::RetainedSurfaceRasterStamp>,
    ) {
        tile_active_set_with_overscan(root, stable_id, contents_clip, 0)
    }

    pub(super) fn tile_active_set_with_overscan(
        root: crate::view::node_arena::NodeKey,
        stable_id: u64,
        contents_clip: [u32; 4],
        overscan: u32,
    ) -> (
        crate::view::paint::ScrollContentTileSetTransactionStamp,
        Vec<crate::view::paint::RetainedSurfaceRasterStamp>,
    ) {
        let content_bounds = [0, 0, 300, 900];
        let manifest = crate::view::paint::plan_active_scroll_content_tiles_dpr1(
            content_bounds,
            [0.0, 0.0],
            contents_clip,
            128,
            1,
            overscan,
        )
        .unwrap();
        let token = crate::view::paint::ScrollContentTileSetTransactionStamp::from_active_manifest(
            root, stable_id, &manifest,
        )
        .unwrap();
        let stamps = manifest
            .tiles()
            .iter()
            .map(|&(index, bounds)| {
                let tile = crate::view::paint::ScrollContentTileRasterIdentity::new(
                    index,
                    content_bounds,
                    bounds,
                    128,
                    1,
                )
                .unwrap();
                let color_key = crate::view::base_component::scroll_content_tile_layer_stable_key(
                    stable_id,
                    index.column,
                    index.row,
                )
                .unwrap();
                let [x, y, width, height] = bounds.raster;
                let color = crate::view::base_component::texture_desc_for_logical_bounds(
                    crate::view::base_component::PromotionCompositeBounds {
                        x: x as f32,
                        y: y as f32,
                        width: width as f32,
                        height: height as f32,
                        corner_radii: [0.0; 4],
                    },
                    1.0,
                    None,
                    wgpu::TextureFormat::Bgra8Unorm,
                );
                let (color, depth) =
                    crate::view::base_component::persistent_target_texture_descriptors(
                        color, color_key,
                    );
                let chunk = crate::view::paint::RetainedSurfaceChunkStamp {
                    id: crate::view::paint::PaintChunkId {
                        owner: root,
                        role: crate::view::paint::PaintChunkRole::SelfDecoration,
                        phase: crate::view::paint::PaintNodePhase::BeforeChildren,
                        scope: crate::view::paint::PaintPropertyScope::SelfPaint,
                        slot: 0,
                    },
                    owner: root,
                    bounds_bits: content_bounds.map(|value| (value as f32).to_bits()),
                    clip: None,
                    non_boundary_self_paint_revision: None,
                    topology_revision: 1,
                    non_boundary_composite_revision: None,
                    payload_identity: crate::view::paint::PaintPayloadIdentity::None,
                    op_count: 1,
                };
                crate::view::paint::validated_scroll_content_tile_raster_stamp(
                    root,
                    stable_id,
                    tile,
                    crate::view::paint::RetainedSurfaceRasterInputs {
                        color,
                        depth,
                        scale_factor_bits: 1.0_f32.to_bits(),
                        source_bounds_bits: bounds.raster.map(|value| (value as f32).to_bits()),
                    },
                    crate::view::paint::RetainedSurfaceArtifactSpanStamp {
                        step_index: 0,
                        owner_topology: vec![crate::view::paint::PaintOwnerSnapshot {
                            owner: root,
                            parent: None,
                        }],
                        clip_nodes: Vec::new(),
                        chunks: vec![chunk],
                        op_count: 1,
                        opaque_order_span: 0..1,
                    },
                    0..1,
                )
                .unwrap()
            })
            .collect();
        (token, stamps)
    }

    #[test]
    fn planner_reuses_only_identical_stamp_key_and_pair_witness() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let root = slots.insert(());
        let key = crate::view::base_component::root_effect_stable_key(root);
        let baseline = stamp(root, 320, 1.0);
        let resident = RootEffectRetainedState::Resident {
            stamp: baseline.clone(),
            key,
        };

        assert_eq!(
            resident.compile_action(&baseline, key, true),
            crate::view::paint::RootEffectCompileAction::Reuse
        );
        assert_eq!(
            resident.compile_action(&baseline, key, false),
            crate::view::paint::RootEffectCompileAction::Reraster
        );

        let replacement = {
            slots.remove(root);
            slots.insert(())
        };
        let root_changed = stamp(replacement, 320, 1.0);
        assert_eq!(
            resident.compile_action(
                &root_changed,
                crate::view::base_component::root_effect_stable_key(replacement),
                true,
            ),
            crate::view::paint::RootEffectCompileAction::Reraster
        );
        assert_eq!(
            resident.compile_action(&stamp(root, 321, 1.0), key, true),
            crate::view::paint::RootEffectCompileAction::Reraster
        );
        assert_eq!(
            resident.compile_action(&stamp(root, 320, 2.0), key, true),
            crate::view::paint::RootEffectCompileAction::Reraster
        );
    }

    #[test]
    fn root_effect_transaction_stages_then_commits_or_invalidates() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let first = slots.insert(());
        let second = slots.insert(());
        let first_key = crate::view::base_component::root_effect_stable_key(first);
        let second_key = crate::view::base_component::root_effect_stable_key(second);
        let first_stamp = stamp(first, 320, 1.0);
        let second_stamp = stamp(second, 640, 2.0);
        let mut viewport = Viewport::new();
        viewport.compositor.root_effect_retained = RootEffectRetainedState::Resident {
            stamp: first_stamp.clone(),
            key: first_key,
        };

        viewport.stage_root_effect_transaction(PendingRootEffectTransaction::Commit {
            stamp: second_stamp.clone(),
            key: second_key,
            action: crate::view::paint::RootEffectCompileAction::Reraster,
        });
        assert_eq!(
            viewport.compositor.root_effect_retained,
            RootEffectRetainedState::Resident {
                stamp: first_stamp,
                key: first_key,
            },
            "staging must not mutate committed state"
        );
        viewport.finish_root_effect_transaction(true);
        assert_eq!(
            viewport.compositor.root_effect_retained,
            RootEffectRetainedState::Resident {
                stamp: second_stamp.clone(),
                key: second_key,
            }
        );

        viewport.stage_root_effect_transaction(PendingRootEffectTransaction::Commit {
            stamp: second_stamp,
            key: second_key,
            action: crate::view::paint::RootEffectCompileAction::Reuse,
        });
        viewport.finish_root_effect_transaction(false);
        assert_eq!(
            viewport.compositor.root_effect_retained,
            RootEffectRetainedState::Invalid
        );

        viewport.compositor.root_effect_retained = RootEffectRetainedState::Resident {
            stamp: stamp(first, 320, 1.0),
            key: first_key,
        };
        viewport.stage_root_effect_clear();
        assert!(matches!(
            viewport.compositor.root_effect_retained,
            RootEffectRetainedState::Resident { .. }
        ));
        viewport.finish_root_effect_transaction(true);
        assert_eq!(
            viewport.compositor.root_effect_retained,
            RootEffectRetainedState::Invalid
        );
    }

    #[test]
    fn render_resource_cache_release_invalidates_committed_and_pending_root_effect() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let root = slots.insert(());
        let key = crate::view::base_component::root_effect_stable_key(root);
        let retained_stamp = stamp(root, 320, 1.0);
        let mut viewport = Viewport::new();
        viewport.compositor.root_effect_retained = RootEffectRetainedState::Resident {
            stamp: retained_stamp.clone(),
            key,
        };
        viewport.stage_root_effect_transaction(PendingRootEffectTransaction::Commit {
            stamp: retained_stamp,
            key,
            action: crate::view::paint::RootEffectCompileAction::Reuse,
        });

        viewport.release_render_resource_caches();

        assert_eq!(
            viewport.compositor.root_effect_retained,
            RootEffectRetainedState::Invalid
        );
        assert!(viewport.compositor.pending_root_effect.is_none());
    }
}

#[cfg(test)]
mod retained_surface_state_tests {
    use super::root_effect_retained_tests::{tile_active_set, tile_active_set_with_overscan};
    use super::*;

    fn commit_tile_set(
        viewport: &mut Viewport,
        manifest: crate::view::paint::ScrollContentTileSetTransactionStamp,
        tiles: Vec<crate::view::paint::RetainedSurfaceRasterStamp>,
    ) {
        assert!(viewport.stage_retained_scroll_tile_active_set(manifest, tiles));
        viewport.finish_retained_surface_transaction(true);
    }

    fn nested_parent_stamp(
        mut parent: crate::view::paint::RetainedSurfaceRasterStamp,
        child: crate::view::paint::RetainedSurfaceRasterStamp,
    ) -> crate::view::paint::RetainedSurfaceRasterStamp {
        let child_source_bounds_bits = child.target.source_bounds_bits;
        let terminal = parent
            .opaque_order_span
            .end
            .max(child.opaque_order_span.end);
        parent.ordered_steps.push(
            crate::view::paint::RetainedSurfaceRasterStepStamp::NestedSurface(
                crate::view::paint::NestedSurfaceRasterDependency {
                    step_index: 1,
                    child_stamp: Box::new(child),
                    child_composite_geometry:
                        crate::view::paint::RetainedSurfaceCompositeGeometryStamp::Transform {
                            source_bounds_bits: child_source_bounds_bits,
                            source_corner_radii_bits: [0.0_f32.to_bits(); 4],
                            visual_bounds_bits: [
                                0.0_f32.to_bits(),
                                0.0_f32.to_bits(),
                                1.0_f32.to_bits(),
                                1.0_f32.to_bits(),
                            ],
                            visual_corner_radii_bits: [0.0_f32.to_bits(); 4],
                            viewport_transform_bits: glam::Mat4::IDENTITY
                                .to_cols_array()
                                .map(f32::to_bits),
                            quad_position_bits: [[0.0_f32.to_bits(); 2]; 4],
                            uv_bounds_bits: [
                                0.0_f32.to_bits(),
                                0.0_f32.to_bits(),
                                1.0_f32.to_bits(),
                                1.0_f32.to_bits(),
                            ],
                            outer_scissor_rect: None,
                        },
                    parent_opaque_order_before: parent.opaque_order_span.end,
                    parent_opaque_order_after: terminal,
                },
            ),
        );
        parent.opaque_order_span = 0..terminal;
        parent
    }

    fn stamp(
        root: crate::view::node_arena::NodeKey,
        stable_id: u64,
        self_paint_revision: u64,
    ) -> crate::view::paint::RetainedSurfaceRasterStamp {
        let color_key = crate::view::base_component::transformed_layer_stable_key(stable_id);
        let color = crate::view::frame_graph::TextureDesc::new(
            40,
            20,
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureDimension::D2,
        )
        .with_origin(8, 10);
        let (color, depth) =
            crate::view::base_component::persistent_target_texture_descriptors(color, color_key);
        let chunks = vec![crate::view::paint::RetainedSurfaceChunkStamp {
            id: crate::view::paint::PaintChunkId {
                owner: root,
                role: crate::view::paint::PaintChunkRole::SelfDecoration,
                phase: crate::view::paint::PaintNodePhase::BeforeChildren,
                scope: crate::view::paint::PaintPropertyScope::SelfPaint,
                slot: 0,
            },
            owner: root,
            bounds_bits: [
                4.0_f32.to_bits(),
                5.0_f32.to_bits(),
                20.0_f32.to_bits(),
                10.0_f32.to_bits(),
            ],
            clip: None,
            non_boundary_self_paint_revision: None,
            topology_revision: self_paint_revision,
            non_boundary_composite_revision: None,
            payload_identity: crate::view::paint::PaintPayloadIdentity::None,
            op_count: 1,
        }];
        let ordered_steps = vec![
            crate::view::paint::RetainedSurfaceRasterStepStamp::ArtifactSpan(
                crate::view::paint::RetainedSurfaceArtifactSpanStamp {
                    step_index: 0,
                    owner_topology: Vec::new(),
                    clip_nodes: Vec::new(),
                    chunks: chunks.clone(),
                    op_count: 1,
                    opaque_order_span: 0..1,
                },
            ),
        ];
        crate::view::paint::RetainedSurfaceRasterStamp {
            identity: crate::view::paint::RetainedSurfaceRasterIdentity {
                boundary_root: root,
                stable_id,
                color_key,
                role: crate::view::paint::RetainedSurfaceRasterRole::Transform,
                scroll_content_tile: None,
            },
            target: crate::view::paint::RetainedSurfaceRasterInputs {
                color,
                depth,
                scale_factor_bits: 2.0_f32.to_bits(),
                source_bounds_bits: [
                    4.0_f32.to_bits(),
                    5.0_f32.to_bits(),
                    20.0_f32.to_bits(),
                    10.0_f32.to_bits(),
                ],
            },
            owner_topology: Vec::new(),
            clip_nodes: Vec::new(),
            chunks,
            op_count: 1,
            opaque_order_span: 0..1,
            ordered_steps,
            text_area_paint_grammar: None,
            interactive_text_area_resident: None,
            atomic_projection_text_area_resident: None,
            scroll_host: None,
            property_effect: None,
        }
    }

    fn property_scene_stamp(
        root: crate::view::node_arena::NodeKey,
        stable_id: u64,
        self_paint_revision: u64,
    ) -> crate::view::paint::RetainedSurfaceRasterStamp {
        let mut stamp = stamp(root, stable_id, self_paint_revision);
        let owner = crate::view::paint::PaintOwnerSnapshot {
            owner: root,
            parent: None,
        };
        stamp.owner_topology = vec![owner];
        let [crate::view::paint::RetainedSurfaceRasterStepStamp::ArtifactSpan(span)] =
            stamp.ordered_steps.as_mut_slice()
        else {
            unreachable!("test property stamp starts with one artifact span")
        };
        span.owner_topology = vec![owner];
        stamp
    }

    fn single_property_scene_transaction(
        root: crate::view::node_arena::NodeKey,
        stamp: &crate::view::paint::RetainedSurfaceRasterStamp,
    ) -> crate::view::paint::RetainedPropertySceneTransactionStamp {
        crate::view::paint::RetainedPropertySceneTransactionStamp::new_for_test(
            crate::view::paint::PropertySceneTransactionWitness {
                roots: vec![crate::view::paint::PropertySceneTransactionRootWitness {
                    ordinal: 0,
                    root,
                    stable_id: stamp.identity.stable_id,
                    top_level_step_span: 0..1,
                }],
                surfaces: vec![crate::view::paint::PropertySceneTransactionSurfaceWitness {
                    ordinal: 0,
                    boundary_root: root,
                    stable_id: stamp.identity.stable_id,
                    persistent_color_key: stamp.identity.color_key,
                    parent_surface: None,
                    scene_root: root,
                    kind: crate::view::paint::PropertySceneTransactionSurfaceKind::Transform(
                        crate::view::compositor::property_tree::TransformNodeId(root),
                    ),
                    transform_viewport_matrix_bits: Some(
                        glam::Mat4::IDENTITY.to_cols_array().map(f32::to_bits),
                    ),
                    effect_composite: None,
                }],
                top_level_surfaces: vec![crate::view::paint::PropertySceneTopLevelSurfaceWitness {
                    step_index: 0,
                    surface_ordinal: 0,
                    scene_root_ordinal: 0,
                }],
                aggregate_opaque_order_span: 0..1,
                outer_scissor_rect: None,
            },
            std::slice::from_ref(stamp),
        )
        .expect("single exact transform property scene is canonical")
    }

    #[derive(Clone)]
    enum RetainedPendingProducerForTest {
        Generic(crate::view::paint::RetainedSurfaceRasterStamp),
        NamedTiles(
            crate::view::paint::ScrollContentTileSetTransactionStamp,
            Vec<crate::view::paint::RetainedSurfaceRasterStamp>,
        ),
        Property(
            crate::view::paint::RetainedPropertySceneTransactionStamp,
            Vec<crate::view::paint::RetainedSurfaceRasterStamp>,
        ),
        PropertyScroll(crate::view::paint::RetainedPropertyScrollSceneTransaction),
        Clear,
    }

    impl RetainedPendingProducerForTest {
        fn stage(&self, viewport: &mut Viewport) -> bool {
            match self {
                Self::Generic(stamp) => viewport.stage_retained_surface_full_set([stamp.clone()]),
                Self::NamedTiles(manifest, stamps) => {
                    viewport.stage_retained_scroll_tile_active_set(manifest.clone(), stamps.clone())
                }
                Self::Property(transaction, stamps) => {
                    viewport.stage_retained_property_scene(transaction.clone(), stamps.clone())
                }
                Self::PropertyScroll(transaction) => {
                    viewport.stage_retained_property_scroll_scene(transaction.clone())
                }
                Self::Clear => viewport.stage_retained_surface_clear(),
            }
        }

        fn color_keys(&self) -> FxHashSet<crate::view::frame_graph::PersistentTextureKey> {
            let stamps = match self {
                Self::Generic(stamp) => vec![stamp.clone()],
                Self::NamedTiles(_, stamps) | Self::Property(_, stamps) => stamps.clone(),
                Self::PropertyScroll(transaction) => {
                    transaction.ordered_stamps().into_iter().cloned().collect()
                }
                Self::Clear => Vec::new(),
            };
            stamps
                .into_iter()
                .map(|stamp| stamp.identity.color_key)
                .collect()
        }
    }

    #[test]
    fn retained_surface_action_requires_stamp_key_canonical_pair_and_pool_witness() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let root = slots.insert(());
        let baseline = stamp(root, 7001, 1);
        let resident_key = baseline.identity.resident_key();
        let mut resident = RetainedSurfaceResidentState::default();
        resident.entries.insert(resident_key, baseline.clone());

        assert_eq!(
            resident.compile_action(&baseline, baseline.identity.color_key, true),
            crate::view::paint::RetainedSurfaceCompileAction::Reuse
        );
        assert_eq!(
            resident.compile_action(&baseline, baseline.identity.color_key, false),
            crate::view::paint::RetainedSurfaceCompileAction::Reraster
        );
        assert_eq!(
            resident.compile_action(
                &baseline,
                crate::view::base_component::transformed_layer_stable_key(7002),
                true,
            ),
            crate::view::paint::RetainedSurfaceCompileAction::Reraster
        );
        assert_eq!(
            resident.compile_action(&stamp(root, 7001, 2), baseline.identity.color_key, true),
            crate::view::paint::RetainedSurfaceCompileAction::Reraster
        );

        let mut payload_changed = baseline.clone();
        payload_changed.chunks[0].payload_identity =
            crate::view::paint::PaintPayloadIdentity::PreparedRects(Arc::from([]));
        assert_eq!(
            resident.compile_action(&payload_changed, baseline.identity.color_key, true),
            crate::view::paint::RetainedSurfaceCompileAction::Reraster
        );

        let mut source_bounds_changed = baseline.clone();
        source_bounds_changed.target.source_bounds_bits[2] = 21.0_f32.to_bits();
        assert_eq!(
            resident.compile_action(&source_bounds_changed, baseline.identity.color_key, true),
            crate::view::paint::RetainedSurfaceCompileAction::Reraster
        );

        let mut descendant_composite_changed = baseline.clone();
        descendant_composite_changed.chunks[0].non_boundary_composite_revision = Some(2);
        assert_eq!(
            resident.compile_action(
                &descendant_composite_changed,
                baseline.identity.color_key,
                true,
            ),
            crate::view::paint::RetainedSurfaceCompileAction::Reraster
        );

        let mut noncanonical = baseline.clone();
        noncanonical.target.color = noncanonical
            .target
            .color
            .clone()
            .with_usage(wgpu::TextureUsages::RENDER_ATTACHMENT);
        assert_eq!(
            resident.compile_action(&noncanonical, baseline.identity.color_key, true),
            crate::view::paint::RetainedSurfaceCompileAction::Reraster
        );

        let replacement = {
            slots.remove(root);
            slots.insert(())
        };
        let replacement_stamp = stamp(replacement, 7001, 1);
        assert_eq!(
            resident.compile_action(
                &replacement_stamp,
                replacement_stamp.identity.color_key,
                true,
            ),
            crate::view::paint::RetainedSurfaceCompileAction::Reraster
        );

        let mut viewport = Viewport::new();
        viewport.compositor.retained_surfaces = resident;
        assert_eq!(
            viewport.retained_surface_compile_action_from_pool(&baseline),
            crate::view::paint::RetainedSurfaceCompileAction::Reraster,
            "without a compatible resident GPU pair, logical stamp equality is insufficient"
        );

        assert!(viewport.stage_retained_surface_full_set([baseline.clone()]));
        viewport.finish_retained_surface_transaction(true);
        assert_eq!(
            viewport.retained_surface_compile_action_from_pool(&baseline),
            crate::view::paint::RetainedSurfaceCompileAction::Reraster,
            "production pool-only authority must never consume the forced test witness"
        );
        assert_eq!(
            viewport.retained_surface_compile_action_for_forced_test(
                &baseline,
                baseline.identity.color_key,
            ),
            crate::view::paint::RetainedSurfaceCompileAction::Reuse,
            "the B1 harness keeps its explicit witness provider"
        );
    }

    #[test]
    fn retained_surface_full_frame_transaction_commits_releases_and_fails_closed() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let first_root = slots.insert(());
        let second_root = slots.insert(());
        let first = stamp(first_root, 7101, 1);
        let second = stamp(second_root, 7102, 1);
        let first_key = first.identity.resident_key();
        let second_key = second.identity.resident_key();
        let mut viewport = Viewport::new();
        viewport
            .compositor
            .retained_surfaces
            .entries
            .insert(first_key, first.clone());

        assert!(viewport.stage_retained_surface_full_set([second.clone()]));
        let pending_before_invalid = viewport.compositor.pending_retained_surfaces.clone();
        let resident_before_invalid = viewport.compositor.retained_surfaces.entries.clone();
        let mut invalid = second.clone();
        invalid.target.depth = invalid
            .target
            .depth
            .clone()
            .with_label("stale-invalid-depth");
        assert!(!viewport.stage_retained_surface_full_set([invalid]));
        assert_eq!(
            viewport.compositor.pending_retained_surfaces, pending_before_invalid,
            "invalid staging must preserve the older valid pending transaction"
        );
        assert_eq!(
            viewport.compositor.retained_surfaces.entries, resident_before_invalid,
            "invalid staging cannot mutate committed state"
        );
        assert!(!viewport.stage_retained_surface_full_set([second.clone(), second.clone(),]));
        assert_eq!(
            viewport.compositor.pending_retained_surfaces, pending_before_invalid,
            "duplicate staging must preserve the older valid pending transaction"
        );
        assert_eq!(
            viewport
                .compositor
                .retained_surfaces
                .entries
                .get(&first_key),
            Some(&first),
            "staging cannot mutate committed state"
        );
        viewport.finish_retained_surface_transaction(true);
        assert_eq!(
            viewport
                .compositor
                .retained_surfaces
                .entries
                .get(&second_key),
            Some(&second)
        );
        assert!(
            !viewport
                .compositor
                .retained_surfaces
                .entries
                .contains_key(&first_key)
        );
        assert_eq!(
            viewport.compositor.retained_surface_release_log,
            vec![first.identity.color_key]
        );

        viewport.compositor.retained_surface_release_log.clear();
        assert!(viewport.stage_retained_surface_full_set([second.clone()]));
        viewport.finish_retained_surface_transaction(false);
        assert!(viewport.compositor.retained_surfaces.entries.is_empty());
        assert!(viewport.compositor.pending_retained_surfaces.is_none());
        assert_eq!(
            viewport.compositor.retained_surface_release_log,
            vec![second.identity.color_key],
            "pending and committed references to one pair release exactly once"
        );

        viewport
            .compositor
            .retained_surfaces
            .entries
            .insert(second_key, second.clone());
        viewport.compositor.retained_surface_release_log.clear();
        assert!(viewport.stage_retained_surface_full_set(std::iter::empty::<
            crate::view::paint::RetainedSurfaceRasterStamp,
        >()));
        viewport.finish_retained_surface_transaction(true);
        assert!(viewport.compositor.retained_surfaces.entries.is_empty());
        assert_eq!(
            viewport.compositor.retained_surface_release_log,
            vec![second.identity.color_key],
            "successful empty full-set releases an unmounted surface pair"
        );

        viewport
            .compositor
            .retained_surfaces
            .entries
            .insert(first_key, first.clone());
        viewport.compositor.retained_surface_release_log.clear();
        viewport.finish_retained_surface_transaction(true);
        assert!(viewport.compositor.retained_surfaces.entries.is_empty());
        assert_eq!(
            viewport.compositor.retained_surface_release_log,
            vec![first.identity.color_key],
            "success without a staged full set is fail-closed"
        );
    }

    #[test]
    fn scroll_tile_active_transaction_rejects_nonexact_sets_and_generic_forest() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let root = slots.insert(());
        let foreign_root = slots.insert(());
        let (manifest, tiles) = tile_active_set(root, 8101, [0, 0, 100, 300]);
        assert!(tiles.len() >= 2);

        let viewport = Viewport::new();
        let actions = viewport
            .freeze_retained_scroll_tile_compile_actions_from_pool(&manifest, &tiles)
            .expect("exact row-major set freezes one action per tile");
        assert_eq!(actions.len(), tiles.len());
        assert!(actions.iter().all(|action| {
            *action == crate::view::paint::RetainedSurfaceCompileAction::Reraster
        }));
        assert!(
            viewport
                .freeze_retained_scroll_tile_compile_actions_from_pool(
                    &manifest,
                    &tiles[..tiles.len() - 1],
                )
                .is_none()
        );
        let mut reversed = tiles.clone();
        reversed.reverse();
        assert!(
            viewport
                .freeze_retained_scroll_tile_compile_actions_from_pool(&manifest, &reversed)
                .is_none()
        );

        let mut duplicate = tiles.clone();
        duplicate[1] = duplicate[0].clone();
        let mut staging = Viewport::new();
        assert!(staging.stage_retained_scroll_tile_active_set(manifest.clone(), tiles.clone()));
        let pending_before_invalid = staging.compositor.pending_retained_surfaces.clone();
        let resident_before_invalid = staging.compositor.retained_surfaces.entries.clone();
        assert!(!staging.stage_retained_scroll_tile_active_set(manifest.clone(), duplicate,));
        assert_eq!(
            staging.compositor.pending_retained_surfaces,
            pending_before_invalid
        );
        assert_eq!(
            staging.compositor.retained_surfaces.entries,
            resident_before_invalid
        );

        assert!(!staging.stage_retained_scroll_tile_active_set(manifest.clone(), reversed,));
        assert_eq!(
            staging.compositor.pending_retained_surfaces,
            pending_before_invalid
        );
        assert_eq!(
            staging.compositor.retained_surfaces.entries,
            resident_before_invalid
        );

        let (foreign_manifest, foreign_tiles) =
            tile_active_set(foreign_root, 8102, [0, 0, 100, 300]);
        assert!(!staging.stage_retained_scroll_tile_active_set(manifest.clone(), foreign_tiles,));
        assert!(!staging.stage_retained_scroll_tile_active_set(foreign_manifest, tiles.clone(),));

        let mut wrong_key = tiles.clone();
        wrong_key[0].identity.color_key = wrong_key[1].identity.color_key;
        assert!(!staging.stage_retained_scroll_tile_active_set(manifest.clone(), wrong_key,));
        assert_eq!(
            staging.compositor.pending_retained_surfaces,
            pending_before_invalid
        );
        assert_eq!(
            staging.compositor.retained_surfaces.entries,
            resident_before_invalid
        );
        staging.finish_retained_surface_transaction(true);
        assert!(staging.compositor.pending_retained_surfaces.is_none());
        assert_eq!(
            staging
                .compositor
                .retained_surfaces
                .scroll_tiles
                .entries
                .len(),
            tiles.len()
        );
        for tile in &tiles {
            assert_eq!(
                staging
                    .compositor
                    .retained_surfaces
                    .scroll_tiles
                    .entries
                    .get(&tile.identity.resident_key())
                    .map(|entry| &entry.stamp),
                Some(tile)
            );
        }

        assert!(
            !staging.stage_retained_surface_full_set(tiles.clone()),
            "generic surface-tree validator must reject a tile forest"
        );
        assert!(
            !staging.stage_retained_surface_full_set([tiles[0].clone()]),
            "generic validator must reject even a one-root tile discriminator"
        );
    }

    #[test]
    fn scroll_tile_active_commit_retains_departed_and_failure_clears_union() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let root = slots.insert(());
        let generic_root = slots.insert(());
        let (large_manifest, large_tiles) = tile_active_set(root, 8201, [0, 0, 100, 300]);
        let large_keys = large_tiles
            .iter()
            .map(|stamp| stamp.identity.color_key)
            .collect::<FxHashSet<_>>();
        let mut viewport = Viewport::new();
        assert!(viewport.stage_retained_scroll_tile_active_set(
            large_manifest.clone(),
            large_tiles.clone(),
        ));
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, Some(large_tiles.len()))
        );
        viewport.finish_retained_surface_transaction(true);
        assert_eq!(
            viewport
                .compositor
                .retained_surfaces
                .scroll_tiles
                .entries
                .len(),
            large_tiles.len()
        );

        let (small_manifest, small_tiles) = tile_active_set(root, 8201, [0, 0, 100, 100]);
        let small_keys = small_tiles
            .iter()
            .map(|stamp| stamp.identity.color_key)
            .collect::<FxHashSet<_>>();
        assert!(small_keys.is_subset(&large_keys));
        viewport.compositor.retained_surface_release_log.clear();
        assert!(viewport.stage_retained_scroll_tile_active_set(
            small_manifest.clone(),
            small_tiles.clone(),
        ));
        viewport.finish_retained_surface_transaction(true);
        assert_eq!(
            viewport
                .compositor
                .retained_surfaces
                .scroll_tiles
                .entries
                .len(),
            large_tiles.len(),
            "departed tiles remain resident until bounded eviction"
        );
        assert!(viewport.compositor.retained_surface_release_log.is_empty());
        assert_eq!(
            viewport
                .compositor
                .retained_surfaces
                .scroll_tiles
                .active
                .len(),
            small_tiles.len()
        );
        let departed = &large_tiles[1];
        assert_eq!(
            viewport.retained_surface_compile_action_for_forced_test(
                departed,
                departed.identity.color_key,
            ),
            crate::view::paint::RetainedSurfaceCompileAction::Reuse,
            "an inactive resident tile with a compatible pair is reusable"
        );
        assert_eq!(
            viewport.retained_surface_compile_action_from_pool(departed),
            crate::view::paint::RetainedSurfaceCompileAction::Reraster,
            "a logical resident without a real pool pair must reraster"
        );
        let mut descriptor_drift = departed.clone();
        descriptor_drift.target.color = descriptor_drift
            .target
            .color
            .clone()
            .with_label("drifted-tile-color");
        assert_eq!(
            viewport.retained_surface_compile_action_for_forced_test(
                &descriptor_drift,
                descriptor_drift.identity.color_key,
            ),
            crate::view::paint::RetainedSurfaceCompileAction::Reraster,
            "descriptor drift cannot consume the resident pair witness"
        );

        viewport.compositor.retained_surface_release_log.clear();
        assert!(
            viewport.stage_retained_scroll_tile_active_set(large_manifest, large_tiles.clone(),)
        );
        viewport.finish_retained_surface_transaction(false);
        assert!(viewport.compositor.retained_surfaces.entries.is_empty());
        assert!(
            viewport
                .compositor
                .retained_surfaces
                .scroll_tiles
                .entries
                .is_empty()
        );
        assert!(viewport.compositor.pending_retained_surfaces.is_none());
        assert_eq!(
            viewport
                .compositor
                .retained_surface_release_log
                .iter()
                .copied()
                .collect::<FxHashSet<_>>(),
            large_keys,
            "failure releases the union of committed and pending tile pairs exactly once"
        );

        let (manifest, tiles) = tile_active_set(root, 8201, [0, 0, 100, 300]);
        assert!(viewport.stage_retained_scroll_tile_active_set(manifest, tiles.clone()));
        viewport.finish_retained_surface_transaction(true);
        viewport.compositor.retained_surface_release_log.clear();
        let generic = stamp(generic_root, 8202, 1);
        assert!(viewport.stage_retained_surface_full_set([generic.clone()]));
        viewport.finish_retained_surface_transaction(true);
        assert_eq!(viewport.compositor.retained_surfaces.entries.len(), 1);
        assert_eq!(
            viewport
                .compositor
                .retained_surfaces
                .entries
                .get(&generic.identity.resident_key()),
            Some(&generic)
        );
        assert_eq!(
            viewport
                .compositor
                .retained_surface_release_log
                .iter()
                .copied()
                .collect::<FxHashSet<_>>(),
            tiles.iter().map(|stamp| stamp.identity.color_key).collect(),
            "switching to a generic/single group clears every active tile pair"
        );
    }

    #[test]
    fn scroll_tile_count_budget_pins_active_and_evicts_lru_with_row_column_tie_break() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let root = slots.insert(());
        let (large_manifest, large_tiles) = tile_active_set(root, 8301, [0, 0, 100, 300]);
        assert_eq!(large_tiles.len(), 3);
        let mut viewport = Viewport::new();
        viewport.set_scroll_tile_resident_budget_for_test(1, 1, u64::MAX);
        viewport.frame.frame_number = 10;
        commit_tile_set(&mut viewport, large_manifest, large_tiles.clone());
        assert_eq!(
            viewport
                .compositor
                .retained_surfaces
                .scroll_tiles
                .entries
                .len(),
            3,
            "the active set remains pinned even above the resident budget"
        );
        assert!(viewport.compositor.retained_surface_release_log.is_empty());

        viewport.set_scroll_tile_resident_budget_for_test(2, u64::MAX, u64::MAX);
        viewport.frame.frame_number = 11;
        viewport.compositor.retained_surface_release_log.clear();
        let (small_manifest, small_tiles) = tile_active_set(root, 8301, [0, 0, 100, 100]);
        commit_tile_set(&mut viewport, small_manifest, small_tiles);
        assert_eq!(
            viewport.compositor.retained_surface_release_log,
            vec![large_tiles[1].identity.color_key],
            "equal-age inactive tiles evict by row then column"
        );
        let cache = &viewport.compositor.retained_surfaces.scroll_tiles;
        assert!(
            cache
                .entries
                .contains_key(&large_tiles[0].identity.resident_key())
        );
        assert!(
            cache
                .active
                .contains(&large_tiles[0].identity.resident_key())
        );
        assert!(
            !cache
                .entries
                .contains_key(&large_tiles[1].identity.resident_key())
        );
        assert!(
            cache
                .entries
                .contains_key(&large_tiles[2].identity.resident_key())
        );
        assert_eq!(
            viewport.retained_surface_compile_action_for_forced_test(
                &large_tiles[1],
                large_tiles[1].identity.color_key,
            ),
            crate::view::paint::RetainedSurfaceCompileAction::Reraster,
            "an evicted tile must reraster when it returns"
        );
        assert_eq!(
            viewport.retained_surface_compile_action_for_forced_test(
                &large_tiles[2],
                large_tiles[2].identity.color_key,
            ),
            crate::view::paint::RetainedSurfaceCompileAction::Reuse
        );
    }

    #[test]
    fn scroll_tile_byte_and_idle_budgets_evict_only_inactive_in_deterministic_order() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let byte_root = slots.insert(());
        let (large_manifest, large_tiles) = tile_active_set(byte_root, 8401, [0, 0, 100, 300]);
        let mut byte_viewport = Viewport::new();
        byte_viewport.frame.frame_number = 1;
        commit_tile_set(&mut byte_viewport, large_manifest, large_tiles.clone());
        let active_pair_bytes = byte_viewport
            .compositor
            .retained_surfaces
            .scroll_tiles
            .entries
            .get(&large_tiles[0].identity.resident_key())
            .unwrap()
            .pair_bytes;
        byte_viewport.set_scroll_tile_resident_budget_for_test(
            usize::MAX,
            active_pair_bytes,
            u64::MAX,
        );
        byte_viewport.frame.frame_number = 2;
        byte_viewport
            .compositor
            .retained_surface_release_log
            .clear();
        let (small_manifest, small_tiles) = tile_active_set(byte_root, 8401, [0, 0, 100, 100]);
        commit_tile_set(&mut byte_viewport, small_manifest, small_tiles);
        assert_eq!(
            byte_viewport.compositor.retained_surface_release_log,
            vec![
                large_tiles[1].identity.color_key,
                large_tiles[2].identity.color_key,
            ]
        );
        assert_eq!(
            byte_viewport
                .compositor
                .retained_surfaces
                .scroll_tiles
                .entries
                .len(),
            1
        );

        let idle_root = slots.insert(());
        let (large_manifest, large_tiles) = tile_active_set(idle_root, 8402, [0, 0, 100, 300]);
        let mut idle_viewport = Viewport::new();
        idle_viewport.set_scroll_tile_resident_budget_for_test(usize::MAX, u64::MAX, 1);
        idle_viewport.frame.frame_number = 20;
        commit_tile_set(&mut idle_viewport, large_manifest, large_tiles.clone());
        idle_viewport.frame.frame_number = 22;
        idle_viewport
            .compositor
            .retained_surface_release_log
            .clear();
        let (small_manifest, small_tiles) = tile_active_set(idle_root, 8402, [0, 0, 100, 100]);
        commit_tile_set(&mut idle_viewport, small_manifest, small_tiles);
        assert_eq!(
            idle_viewport.compositor.retained_surface_release_log,
            vec![
                large_tiles[1].identity.color_key,
                large_tiles[2].identity.color_key,
            ]
        );
        let cache = &idle_viewport.compositor.retained_surfaces.scroll_tiles;
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(cache.active.len(), 1);
        assert!(
            cache
                .active
                .contains(&large_tiles[0].identity.resident_key())
        );
    }

    #[test]
    fn scroll_tile_group_generic_and_clear_transitions_release_the_whole_cache() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let first_root = slots.insert(());
        let second_root = slots.insert(());
        let generic_root = slots.insert(());
        let (first_manifest, first_tiles) = tile_active_set(first_root, 8501, [0, 0, 100, 300]);
        let mut viewport = Viewport::new();
        commit_tile_set(&mut viewport, first_manifest, first_tiles.clone());

        viewport.compositor.retained_surface_release_log.clear();
        let (second_manifest, second_tiles) = tile_active_set(second_root, 8502, [0, 0, 100, 100]);
        commit_tile_set(&mut viewport, second_manifest, second_tiles.clone());
        assert_eq!(
            viewport
                .compositor
                .retained_surface_release_log
                .iter()
                .copied()
                .collect::<FxHashSet<_>>(),
            first_tiles
                .iter()
                .map(|tile| tile.identity.color_key)
                .collect()
        );
        assert_eq!(
            viewport
                .compositor
                .retained_surfaces
                .scroll_tiles
                .entries
                .len(),
            second_tiles.len()
        );

        viewport.compositor.retained_surface_release_log.clear();
        let generic = stamp(generic_root, 8503, 1);
        assert!(viewport.stage_retained_surface_full_set([generic.clone()]));
        viewport.finish_retained_surface_transaction(true);
        assert!(
            viewport
                .compositor
                .retained_surfaces
                .scroll_tiles
                .entries
                .is_empty()
        );
        assert_eq!(viewport.compositor.retained_surfaces.entries.len(), 1);
        assert_eq!(
            viewport
                .compositor
                .retained_surface_release_log
                .iter()
                .copied()
                .collect::<FxHashSet<_>>(),
            second_tiles
                .iter()
                .map(|tile| tile.identity.color_key)
                .collect()
        );

        let (manifest, tiles) = tile_active_set(first_root, 8501, [0, 0, 100, 100]);
        commit_tile_set(&mut viewport, manifest, tiles.clone());
        viewport.compositor.retained_surface_release_log.clear();
        viewport.stage_retained_surface_clear();
        viewport.finish_retained_surface_transaction(true);
        assert!(
            viewport
                .compositor
                .retained_surfaces
                .scroll_tiles
                .entries
                .is_empty()
        );
        assert_eq!(
            viewport.compositor.retained_surface_release_log,
            vec![tiles[0].identity.color_key]
        );

        let (manifest, tiles) = tile_active_set(first_root, 8501, [0, 0, 100, 100]);
        commit_tile_set(&mut viewport, manifest, tiles.clone());
        viewport.compositor.retained_surface_release_log.clear();
        viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedIsolationCanary);
        assert!(
            viewport
                .compositor
                .retained_surfaces
                .scroll_tiles
                .entries
                .is_empty()
        );
        assert_eq!(
            viewport.compositor.retained_surface_release_log,
            vec![tiles[0].identity.color_key],
            "leaving the tiled retained mode clears the complete tile group"
        );
    }

    #[test]
    fn scroll_tile_group_seal_change_rerasterizes_active_and_clears_only_old_pairs() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let root = slots.insert(());
        let (first_manifest, first_tiles) =
            tile_active_set_with_overscan(root, 8601, [0, 0, 100, 300], 0);
        let mut viewport = Viewport::new();
        commit_tile_set(&mut viewport, first_manifest, first_tiles.clone());

        let (changed_manifest, changed_tiles) =
            tile_active_set_with_overscan(root, 8601, [0, 0, 100, 100], 1);
        assert_eq!(changed_tiles.len(), 1);
        assert_eq!(
            viewport
                .freeze_retained_scroll_tile_compile_actions_for_forced_test(
                    &changed_manifest,
                    &changed_tiles,
                )
                .unwrap(),
            vec![crate::view::paint::RetainedSurfaceCompileAction::Reraster],
            "a changed group seal cannot reuse an old logical resident"
        );
        viewport.compositor.retained_surface_release_log.clear();
        commit_tile_set(&mut viewport, changed_manifest, changed_tiles.clone());
        assert_eq!(
            viewport.compositor.retained_surface_release_log,
            vec![
                first_tiles[1].identity.color_key,
                first_tiles[2].identity.color_key,
            ],
            "inactive old-group pairs release deterministically while the freshly rerastered active pair stays alive"
        );
        let cache = &viewport.compositor.retained_surfaces.scroll_tiles;
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(cache.active.len(), 1);
        assert_eq!(
            cache
                .entries
                .get(&changed_tiles[0].identity.resident_key())
                .map(|entry| &entry.stamp),
            Some(&changed_tiles[0])
        );
    }

    #[test]
    fn retained_surface_clear_mode_switch_and_cache_release_invalidate_lifecycle() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let root = slots.insert(());
        let retained = stamp(root, 7201, 1);
        let resident_key = retained.identity.resident_key();
        let mut viewport = Viewport::new();

        viewport
            .compositor
            .retained_surfaces
            .entries
            .insert(resident_key, retained.clone());
        viewport.stage_retained_surface_clear();
        viewport.finish_retained_surface_transaction(true);
        assert!(viewport.compositor.retained_surfaces.entries.is_empty());
        assert_eq!(
            viewport.compositor.retained_surface_release_log,
            vec![retained.identity.color_key]
        );

        viewport
            .compositor
            .retained_surfaces
            .entries
            .insert(resident_key, retained.clone());
        viewport.compositor.retained_surface_release_log.clear();
        viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedIsolationCanary);
        assert!(viewport.compositor.retained_surfaces.entries.is_empty());
        assert_eq!(
            viewport.compositor.retained_surface_release_log,
            vec![retained.identity.color_key]
        );

        viewport
            .compositor
            .retained_surfaces
            .entries
            .insert(resident_key, retained.clone());
        viewport.compositor.retained_surface_release_log.clear();
        viewport.release_render_resource_caches();
        assert!(viewport.compositor.retained_surfaces.entries.is_empty());
        assert!(viewport.compositor.pending_retained_surfaces.is_none());
        assert_eq!(
            viewport.compositor.retained_surface_release_log,
            vec![retained.identity.color_key]
        );
    }

    #[test]
    fn retained_surface_nested_full_set_requires_exact_dependency_closure() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let parent_root = slots.insert(());
        let child_root = slots.insert(());
        let child = stamp(child_root, 7302, 2);
        let parent = nested_parent_stamp(stamp(parent_root, 7301, 1), child.clone());
        assert!(crate::view::paint::retained_surface_raster_stamp_is_canonical(&parent));

        let mut viewport = Viewport::new();
        assert!(!viewport.stage_retained_surface_full_set([parent.clone()]));
        assert!(viewport.compositor.pending_retained_surfaces.is_none());

        let stale_child = stamp(child_root, 7302, 99);
        assert!(!viewport.stage_retained_surface_full_set([parent.clone(), stale_child]));
        assert!(viewport.compositor.pending_retained_surfaces.is_none());

        assert!(viewport.stage_retained_surface_full_set([parent.clone(), child.clone()]));
        viewport.finish_retained_surface_transaction(true);
        assert_eq!(viewport.compositor.retained_surfaces.entries.len(), 2);

        assert!(viewport.stage_retained_surface_full_set([parent, child]));
        viewport.finish_retained_surface_transaction(false);
        assert!(viewport.compositor.retained_surfaces.entries.is_empty());
        assert!(viewport.compositor.pending_retained_surfaces.is_none());
    }

    #[test]
    fn property_scene_transaction_commits_exact_multi_root_deep_forest_atomically() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let first_root = slots.insert(());
        let child_root = slots.insert(());
        let grandchild_root = slots.insert(());
        let second_root = slots.insert(());
        let old_root = slots.insert(());

        let grandchild = property_scene_stamp(grandchild_root, 9103, 3);
        let child = nested_parent_stamp(
            property_scene_stamp(child_root, 9102, 2),
            grandchild.clone(),
        );
        let first = nested_parent_stamp(property_scene_stamp(first_root, 9101, 1), child.clone());
        let second = property_scene_stamp(second_root, 9201, 4);
        let ordered = vec![
            first.clone(),
            child.clone(),
            grandchild.clone(),
            second.clone(),
        ];
        let witness =
            crate::view::paint::PropertySceneTransactionWitness {
                roots: vec![
                    crate::view::paint::PropertySceneTransactionRootWitness {
                        ordinal: 0,
                        root: first_root,
                        stable_id: 9001,
                        top_level_step_span: 0..1,
                    },
                    crate::view::paint::PropertySceneTransactionRootWitness {
                        ordinal: 1,
                        root: second_root,
                        stable_id: 9002,
                        top_level_step_span: 1..2,
                    },
                ],
                surfaces: vec![
                    crate::view::paint::PropertySceneTransactionSurfaceWitness {
                        ordinal: 0,
                        boundary_root: first_root,
                        stable_id: 9101,
                        persistent_color_key:
                            crate::view::base_component::transformed_layer_stable_key(9101),
                        parent_surface: None,
                        scene_root: first_root,
                        kind: crate::view::paint::PropertySceneTransactionSurfaceKind::Transform(
                            crate::view::compositor::property_tree::TransformNodeId(first_root),
                        ),
                        transform_viewport_matrix_bits: Some(
                            glam::Mat4::IDENTITY.to_cols_array().map(f32::to_bits),
                        ),
                        effect_composite: None,
                    },
                    crate::view::paint::PropertySceneTransactionSurfaceWitness {
                        ordinal: 1,
                        boundary_root: child_root,
                        stable_id: 9102,
                        persistent_color_key:
                            crate::view::base_component::transformed_layer_stable_key(9102),
                        parent_surface: Some(first_root),
                        scene_root: first_root,
                        kind: crate::view::paint::PropertySceneTransactionSurfaceKind::Transform(
                            crate::view::compositor::property_tree::TransformNodeId(child_root),
                        ),
                        transform_viewport_matrix_bits: Some(
                            glam::Mat4::IDENTITY.to_cols_array().map(f32::to_bits),
                        ),
                        effect_composite: None,
                    },
                    crate::view::paint::PropertySceneTransactionSurfaceWitness {
                        ordinal: 2,
                        boundary_root: grandchild_root,
                        stable_id: 9103,
                        persistent_color_key:
                            crate::view::base_component::transformed_layer_stable_key(9103),
                        parent_surface: Some(child_root),
                        scene_root: first_root,
                        kind: crate::view::paint::PropertySceneTransactionSurfaceKind::Transform(
                            crate::view::compositor::property_tree::TransformNodeId(
                                grandchild_root,
                            ),
                        ),
                        transform_viewport_matrix_bits: Some(
                            glam::Mat4::IDENTITY.to_cols_array().map(f32::to_bits),
                        ),
                        effect_composite: None,
                    },
                    crate::view::paint::PropertySceneTransactionSurfaceWitness {
                        ordinal: 3,
                        boundary_root: second_root,
                        stable_id: 9201,
                        persistent_color_key:
                            crate::view::base_component::transformed_layer_stable_key(9201),
                        parent_surface: None,
                        scene_root: second_root,
                        kind: crate::view::paint::PropertySceneTransactionSurfaceKind::Transform(
                            crate::view::compositor::property_tree::TransformNodeId(second_root),
                        ),
                        transform_viewport_matrix_bits: Some(
                            glam::Mat4::IDENTITY.to_cols_array().map(f32::to_bits),
                        ),
                        effect_composite: None,
                    },
                ],
                top_level_surfaces: vec![
                    crate::view::paint::PropertySceneTopLevelSurfaceWitness {
                        step_index: 0,
                        surface_ordinal: 0,
                        scene_root_ordinal: 0,
                    },
                    crate::view::paint::PropertySceneTopLevelSurfaceWitness {
                        step_index: 1,
                        surface_ordinal: 3,
                        scene_root_ordinal: 1,
                    },
                ],
                aggregate_opaque_order_span: 0..1,
                outer_scissor_rect: None,
            };
        let transaction = crate::view::paint::RetainedPropertySceneTransactionStamp::new_for_test(
            witness, &ordered,
        )
        .expect("deep multi-root property scene has one exact transaction stamp");

        let old = stamp(old_root, 9000, 9);
        let mut viewport = Viewport::new();
        viewport
            .compositor
            .retained_surfaces
            .entries
            .insert(old.identity.resident_key(), old.clone());

        assert!(viewport.stage_retained_property_scene(transaction.clone(), ordered.clone()));
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (1, Some(4))
        );
        assert!(matches!(
            viewport.compositor.pending_retained_surfaces,
            Some(PendingRetainedSurfaceTransaction::CommitPropertyScene { .. })
        ));

        let mut reordered = ordered.clone();
        reordered.swap(0, 1);
        assert!(!viewport.stage_retained_property_scene(transaction.clone(), reordered));
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (1, Some(4)),
            "invalid restaging preserves the older exact pending transaction"
        );
        assert!(
            !viewport.stage_retained_surface_full_set(ordered.clone()),
            "the generic singleton/depth-1 checker remains sealed"
        );

        viewport.finish_retained_surface_transaction(true);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (4, None)
        );
        assert_eq!(
            viewport.compositor.retained_surface_release_log,
            vec![old.identity.color_key]
        );
        for stamp in &ordered {
            assert_eq!(
                viewport
                    .compositor
                    .retained_surfaces
                    .entries
                    .get(&stamp.identity.resident_key()),
                Some(stamp)
            );
        }

        viewport.compositor.retained_surface_release_log.clear();
        assert!(viewport.stage_retained_property_scene(transaction, ordered.clone()));
        viewport.finish_retained_surface_transaction(false);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, None)
        );
        assert_eq!(
            viewport
                .compositor
                .retained_surface_release_log
                .iter()
                .copied()
                .collect::<FxHashSet<_>>(),
            ordered
                .iter()
                .map(|stamp| stamp.identity.color_key)
                .collect::<FxHashSet<_>>(),
            "failure releases the committed/pending union exactly once"
        );
        assert_eq!(
            viewport.compositor.retained_surface_release_log.len(),
            ordered.len()
        );
    }

    #[test]
    fn property_scroll_pool_commits_generic_and_multiple_groups_as_one_union() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let generic_root = slots.insert(());
        let first_scroll_root = slots.insert(());
        let second_scroll_root = slots.insert(());
        let generic = stamp(generic_root, 9701, 1);
        let (_, first_tiles) = tile_active_set(first_scroll_root, 9702, [0, 0, 100, 260]);
        let (_, second_tiles) = tile_active_set(second_scroll_root, 9703, [0, 128, 100, 260]);
        let transaction =
            crate::view::paint::RetainedPropertyScrollSceneTransaction::new_for_pool_test(
                vec![generic.clone()],
                vec![first_tiles.clone(), second_tiles.clone()],
            )
            .expect("generic plus two exact scroll groups form one canonical transaction");
        let expected_len = 1 + first_tiles.len() + second_tiles.len();
        let mut viewport = Viewport::new();

        let cold = viewport
            .freeze_retained_property_scroll_scene_compile_actions_from_pool(&transaction)
            .expect("canonical cold transaction freezes exact coverage");
        assert_eq!(cold.len(), expected_len);
        assert!(cold.values().all(|action| {
            *action == crate::view::paint::RetainedSurfaceCompileAction::Reraster
        }));
        assert!(viewport.stage_retained_property_scroll_scene(transaction.clone()));
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, Some(expected_len))
        );
        let pending_before_valid_restage = viewport.compositor.pending_retained_surfaces.clone();
        let resident_before_valid_restage = viewport.compositor.retained_surfaces.clone();
        assert!(
            !viewport.stage_retained_property_scroll_scene(transaction.clone()),
            "a valid transaction cannot overwrite an occupied single-stage slot"
        );
        assert_eq!(
            viewport.compositor.pending_retained_surfaces, pending_before_valid_restage,
            "valid double-stage preserves the first pending transaction byte-for-byte"
        );
        assert_eq!(
            viewport.compositor.retained_surfaces, resident_before_valid_restage,
            "valid double-stage cannot mutate committed residents"
        );
        let pending_before_invalid = viewport.compositor.pending_retained_surfaces.clone();
        let resident_before_invalid = viewport.compositor.retained_surfaces.clone();
        assert!(
            !viewport.stage_retained_property_scroll_scene(transaction.invalid_for_pool_test())
        );
        assert_eq!(
            viewport.compositor.pending_retained_surfaces, pending_before_invalid,
            "invalid restaging preserves the older canonical pending transaction"
        );
        assert_eq!(
            viewport.compositor.retained_surfaces, resident_before_invalid,
            "invalid staging never mutates committed residents"
        );
        viewport.finish_retained_surface_transaction(true);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (expected_len, None)
        );
        assert_eq!(
            viewport
                .compositor
                .retained_surfaces
                .property_scroll
                .groups
                .len(),
            2
        );
        assert_eq!(
            viewport
                .compositor
                .retained_surfaces
                .property_scroll
                .active
                .len(),
            first_tiles.len() + second_tiles.len()
        );

        let warm = viewport
            .freeze_retained_property_scroll_scene_compile_actions_for_forced_test(&transaction)
            .expect("committed transaction retains exact pool coverage");
        assert!(
            warm.values().all(|action| {
                *action == crate::view::paint::RetainedSurfaceCompileAction::Reuse
            })
        );

        viewport.compositor.retained_surface_release_log.clear();
        assert!(viewport.stage_retained_property_scroll_scene(transaction));
        viewport.finish_retained_surface_transaction(false);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, None)
        );
        assert_eq!(
            viewport.compositor.retained_surface_release_log.len(),
            expected_len,
            "failure releases the committed/pending color-pair union once"
        );
        assert_eq!(
            viewport
                .compositor
                .retained_surface_release_log
                .iter()
                .copied()
                .collect::<FxHashSet<_>>()
                .len(),
            expected_len
        );
    }

    #[test]
    fn every_retained_producer_shares_one_compare_and_set_pending_slot() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let b2_generic_root = slots.insert(());
        let b2_scroll_root = slots.insert(());
        let generic_root = slots.insert(());
        let named_root = slots.insert(());
        let property_root = slots.insert(());

        let b2_generic = stamp(b2_generic_root, 9901, 1);
        let (_, b2_tiles) = tile_active_set(b2_scroll_root, 9902, [0, 0, 100, 260]);
        let b2 = RetainedPendingProducerForTest::PropertyScroll(
            crate::view::paint::RetainedPropertyScrollSceneTransaction::new_for_pool_test(
                vec![b2_generic],
                vec![b2_tiles],
            )
            .unwrap(),
        );
        let generic = RetainedPendingProducerForTest::Generic(stamp(generic_root, 9910, 1));
        let (named_manifest, named_tiles) = tile_active_set(named_root, 9920, [0, 0, 100, 260]);
        let named = RetainedPendingProducerForTest::NamedTiles(named_manifest, named_tiles);
        let property_stamp = property_scene_stamp(property_root, 9930, 1);
        let property = RetainedPendingProducerForTest::Property(
            single_property_scene_transaction(property_root, &property_stamp),
            vec![property_stamp],
        );
        let clear = RetainedPendingProducerForTest::Clear;

        for (label, other) in [
            ("generic", generic),
            ("named", named),
            ("property", property),
            ("clear", clear),
        ] {
            for (first_label, first, second) in [
                ("b2-first", b2.clone(), other.clone()),
                ("other-first", other.clone(), b2.clone()),
            ] {
                let mut viewport = Viewport::new();
                assert!(
                    first.stage(&mut viewport),
                    "{label}/{first_label}: first stage"
                );
                let pending_before = viewport.compositor.pending_retained_surfaces.clone();
                let resident_before = viewport.compositor.retained_surfaces.clone();
                assert!(
                    !second.stage(&mut viewport),
                    "{label}/{first_label}: occupied global slot rejects the second variant"
                );
                assert_eq!(
                    viewport.compositor.pending_retained_surfaces, pending_before,
                    "{label}/{first_label}: first pending owner is byte-for-byte preserved"
                );
                assert_eq!(
                    viewport.compositor.retained_surfaces, resident_before,
                    "{label}/{first_label}: rejected cross-variant stage is resident-inert"
                );
                viewport.finish_retained_surface_transaction(false);
                let expected = first.color_keys();
                assert_eq!(
                    viewport
                        .compositor
                        .retained_surface_release_log
                        .iter()
                        .copied()
                        .collect::<FxHashSet<_>>(),
                    expected,
                    "{label}/{first_label}: failure releases only the first owner union"
                );
                assert_eq!(
                    viewport.compositor.retained_surface_release_log.len(),
                    expected.len(),
                    "{label}/{first_label}: each first-owner pair releases exactly once"
                );
            }
        }
    }

    #[test]
    fn frame_owner_cannot_finish_foreign_pending_and_original_owner_can_commit() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let root = slots.insert(());
        let baseline = stamp(root, 9940, 1);
        let incoming = stamp(root, 9940, 2);
        let resident_key = baseline.identity.resident_key();
        let mut viewport = Viewport::new();

        assert!(viewport.stage_retained_surface_full_set([baseline.clone()]));
        viewport.finish_retained_surface_transaction(true);
        assert_eq!(
            viewport
                .compositor
                .retained_surfaces
                .entries
                .get(&resident_key),
            Some(&baseline)
        );

        assert!(viewport.stage_retained_surface_full_set([incoming.clone()]));
        let foreign_pending = viewport.compositor.pending_retained_surfaces.clone();
        let foreign_owner = viewport.compositor.pending_retained_surface_owner;
        let resident_before = viewport.compositor.retained_surfaces.clone();

        let frame_owner = viewport.begin_retained_surface_frame_stage();
        assert!(
            frame_owner.is_none(),
            "occupied CAS slot grants no frame owner"
        );
        assert!(!viewport.stage_retained_surface_clear());
        assert!(!viewport.finish_retained_surface_transaction_for_frame(frame_owner, true));
        assert_eq!(
            viewport.compositor.pending_retained_surfaces,
            foreign_pending
        );
        assert_eq!(
            viewport.compositor.pending_retained_surface_owner,
            foreign_owner
        );
        assert_eq!(viewport.compositor.retained_surfaces, resident_before);

        viewport.finish_retained_surface_transaction(true);
        assert_eq!(
            viewport
                .compositor
                .retained_surfaces
                .entries
                .get(&resident_key),
            Some(&incoming),
            "the original standalone owner remains finishable after Auto Legacy fallback"
        );
    }

    #[test]
    fn stale_frame_owner_finish_is_rejected_without_pending_or_resident_mutation() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let first_root = slots.insert(());
        let second_root = slots.insert(());
        let first = stamp(first_root, 9950, 1);
        let second = stamp(second_root, 9951, 1);
        let mut viewport = Viewport::new();

        let first_owner = viewport
            .begin_retained_surface_frame_stage()
            .expect("empty slot grants first owner");
        assert!(viewport.stage_retained_surface_full_set([first]));
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(first_owner), true));

        let second_owner = viewport
            .begin_retained_surface_frame_stage()
            .expect("finished frame grants a new owner generation");
        assert_ne!(first_owner, second_owner);
        assert!(viewport.stage_retained_surface_full_set([second.clone()]));
        let pending_before = viewport.compositor.pending_retained_surfaces.clone();
        let owner_before = viewport.compositor.pending_retained_surface_owner;
        let resident_before = viewport.compositor.retained_surfaces.clone();

        assert!(!viewport.finish_retained_surface_transaction_for_frame(Some(first_owner), true,));
        assert_eq!(
            viewport.compositor.pending_retained_surfaces,
            pending_before
        );
        assert_eq!(
            viewport.compositor.pending_retained_surface_owner,
            owner_before
        );
        assert_eq!(viewport.compositor.retained_surfaces, resident_before);

        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(second_owner), true));
        assert!(
            viewport
                .compositor
                .retained_surfaces
                .entries
                .values()
                .any(|stamp| stamp == &second)
        );
        assert!(!viewport.finish_retained_surface_transaction_for_frame(Some(second_owner), true,));
    }

    #[test]
    fn property_scroll_inactive_eviction_uses_the_full_deterministic_lru_key() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let high_root = slots.insert(());
        let low_root = slots.insert(());
        let active_root = slots.insert(());
        let (_, high_tiles) = tile_active_set(high_root, 9802, [0, 0, 100, 260]);
        let (_, low_tiles) = tile_active_set(low_root, 9801, [0, 0, 100, 260]);
        let (_, active_tiles) = tile_active_set(active_root, 9803, [0, 0, 100, 260]);
        let first = crate::view::paint::RetainedPropertyScrollSceneTransaction::new_for_pool_test(
            Vec::new(),
            vec![high_tiles.clone(), low_tiles.clone()],
        )
        .unwrap();
        let second = crate::view::paint::RetainedPropertyScrollSceneTransaction::new_for_pool_test(
            Vec::new(),
            vec![active_tiles.clone()],
        )
        .unwrap();
        let mut viewport = Viewport::new();
        assert!(viewport.stage_retained_property_scroll_scene(first));
        viewport.finish_retained_surface_transaction(true);

        viewport.set_scroll_tile_resident_budget_for_test(active_tiles.len(), u64::MAX, u64::MAX);
        viewport.compositor.retained_surface_release_log.clear();
        assert!(viewport.stage_retained_property_scroll_scene(second));
        viewport.finish_retained_surface_transaction(true);

        let expected_release_order = low_tiles
            .iter()
            .chain(&high_tiles)
            .map(|stamp| stamp.identity.color_key)
            .collect::<Vec<_>>();
        assert_eq!(
            viewport.compositor.retained_surface_release_log, expected_release_order,
            "same-age inactive residents sort by stable id, root, backing, row, then column"
        );
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (active_tiles.len(), None)
        );
        assert_eq!(
            viewport
                .compositor
                .retained_surfaces
                .property_scroll
                .groups
                .len(),
            1,
            "fully evicted inactive group shells are reclaimed"
        );
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
    const PROMOTED_REUSE_COOLDOWN_FRAMES: u8 = 2;
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

    /// Selects the production paint rollout mode. The default remains
    /// [`ViewportPaintRendererMode::Legacy`]; enabling the canary never
    /// permits per-root mixing with the legacy renderer. Calling this with the
    /// already-requested `RetainedAuto` mode manually resets an open terminal
    /// circuit breaker; ordinary same-mode calls remain no-ops.
    pub fn set_paint_renderer_mode(&mut self, mode: ViewportPaintRendererMode) {
        if self.paint_renderer_mode == mode && self.retained_auto_terminal_failure.is_none() {
            return;
        }
        self.invalidate_root_effect_retained();
        self.invalidate_retained_surfaces();
        self.frame.compile_cache = None;
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
        crate::view::debug::DebugCapture::from_arena(
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
        )
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
