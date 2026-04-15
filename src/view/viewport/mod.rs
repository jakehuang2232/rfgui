#![allow(missing_docs)]

mod debug;
mod frame;
mod input;
#[cfg(test)]
mod tests;

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
    BlurEvent, ClickEvent, EventMeta, FocusEvent, FromPropValue, ImePreeditEvent, KeyDownEvent,
    KeyEventData, KeyModifiers, KeyUpEvent, MouseButtons as UiMouseButtons, MouseDownEvent,
    MouseEventData, MouseMoveEvent, MouseUpEvent, MouseUpUntilHandler, Patch, PropValue, RsxNode,
    TextInputEvent, ViewportListenerAction, ViewportListenerHandle, reconcile, take_state_dirty,
};
use crate::view::base_component::Renderable;
use crate::view::frame_graph::texture_resource::TextureDesc;
use crate::view::frame_graph::{AllocationId, BufferDesc, FrameGraph};
use crate::view::promotion::{
    PromotedLayerUpdate, PromotedLayerUpdateKind, PromotionDecision, PromotionState,
    ViewportPromotionConfig, active_channels_by_node, evaluate_promotion,
};
use crate::view::promotion_builder::{
    collect_debug_subtree_signatures, collect_promoted_layer_updates, collect_promotion_candidates,
};
use crate::view::render_pass::render_target::{OffscreenRenderTargetPool, RenderTargetBundle};
use crate::{
    ColorLike, Cursor, ElementStylePropSchema, HexColor, PropertyId, Style, Transform,
    TransformOrigin,
};
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::ops::Sub;
use std::sync::Arc;
use wgpu::util::StagingBelt;
use wgpu::{
    Instance, Queue, TextureUsages,
    rwh::{HasDisplayHandle, HasWindowHandle},
};

pub(crate) use self::debug::{
    DebugReusePathContext, DebugReusePathRecord, begin_debug_reuse_path_frame,
    record_debug_reuse_path,
};
use self::debug::{
    DebugStyleSampleRecord, PostLayoutTransitionResult, TraceRenderNode, build_compile_trace_nodes,
    build_execute_detail_trace_nodes, build_layout_place_trace_nodes, build_reuse_overlay_geometry,
    format_promotion_trace, format_reuse_path_trace, format_style_field,
    format_style_promotion_trace, format_style_request_trace, format_style_sample_trace,
    format_style_value, format_trace_render_tree, record_debug_style_promotion,
    record_debug_style_request, record_debug_style_sample, record_debug_style_sample_record,
    snapshot_debug_reuse_path, snapshot_debug_style_sample_records, style_field_requires_relayout,
    trace_promoted_build_frame_marker,
};
pub use self::frame::FrameParts;
use self::frame::{BeginFrameProfile, EndFrameProfile, FrameState, FrameStats};
use self::input::{
    InputState, PendingClick, ViewportMouseUpListener, is_valid_click_candidate, to_ui_mouse_button,
};
pub use self::input::{MouseButton, ViewportDebugOptions};
use crate::platform::{
    PlatformImePreedit, PlatformKeyEvent, PlatformMouseButton, PlatformMouseEvent,
    PlatformMouseEventKind, PlatformRequests, PlatformTextInput, PlatformWheelEvent,
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

    pub fn set_focus(&mut self, node_id: Option<u64>) {
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

    pub fn set_pointer_capture(&mut self, node_id: u64) {
        self.viewport.set_pointer_capture_node_id(Some(node_id));
    }

    pub fn release_pointer_capture(&mut self, node_id: u64) {
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

    pub fn set_debug_trace_fps(&mut self, enabled: bool) {
        self.viewport.set_debug_trace_fps(enabled);
    }

    pub fn set_debug_trace_render_time(&mut self, enabled: bool) {
        self.viewport.set_debug_trace_render_time(enabled);
    }

    pub fn set_debug_trace_compile_detail(&mut self, enabled: bool) {
        self.viewport.set_debug_trace_compile_detail(enabled);
    }

    pub fn set_debug_trace_reuse_path(&mut self, enabled: bool) {
        self.viewport.set_debug_trace_reuse_path(enabled);
    }

    pub fn set_debug_options(&mut self, options: ViewportDebugOptions) {
        self.viewport.set_debug_options(options);
    }

    pub fn set_debug_geometry_overlay(&mut self, enabled: bool) {
        self.viewport.set_debug_geometry_overlay(enabled);
    }

    pub fn set_msaa_sample_count(&mut self, sample_count: u32) {
        self.viewport.set_msaa_sample_count(sample_count);
    }

    pub fn release_render_resource_caches(&mut self) {
        self.viewport.release_render_resource_caches();
    }
}

struct TransitionHostAdapter<'a> {
    registered_channels: &'a HashSet<ChannelId>,
    claims: &'a mut HashMap<TrackKey<TrackTarget>, TransitionPluginId>,
}

impl TransitionHost<TrackTarget> for TransitionHostAdapter<'_> {
    fn is_channel_registered(&self, channel: ChannelId) -> bool {
        self.registered_channels.contains(&channel)
    }

    fn claim_track(
        &mut self,
        plugin_id: TransitionPluginId,
        key: TrackKey<TrackTarget>,
        mode: ClaimMode,
    ) -> bool {
        if let Some(current) = self.claims.get(&key).copied() {
            if current == plugin_id {
                return true;
            }
            if matches!(mode, ClaimMode::Replace) {
                self.claims.insert(key, plugin_id);
                return true;
            }
            return false;
        }
        self.claims.insert(key, plugin_id);
        true
    }

    fn release_track_claim(&mut self, plugin_id: TransitionPluginId, key: TrackKey<TrackTarget>) {
        if self.claims.get(&key).copied() == Some(plugin_id) {
            self.claims.remove(&key);
        }
    }

    fn release_all_claims(&mut self, plugin_id: TransitionPluginId) {
        self.claims.retain(|_, owner| *owner != plugin_id);
    }
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
    dispatched_focus_node_id: Option<u64>,
    scene: SceneState,
    transitions: TransitionRuntime,
    cursor_override: Option<Cursor>,
    last_recorded_cursor: Option<Cursor>,
    pending_platform_requests: PlatformRequests,
    /// Set inside `render_rsx_with_dirty` whenever any transition or
    /// animation plugin reports `keep_running`. Cleared at the start of
    /// every render. Hosts query this via `is_animating()` to decide
    /// whether to pump another frame immediately or idle.
    is_animating: bool,
    viewport_mouse_move_listeners: Vec<crate::ui::MouseMoveHandlerProp>,
    viewport_mouse_up_listeners: Vec<ViewportMouseUpListener>,
}

/// Phase-7 extraction. The retained scene tree and the per-node state
/// layered on top of it: the concrete `ElementTrait` roots produced by the
/// last reconcile pass, ad-hoc scroll offsets, element-side snapshot
/// blobs, and the last `RsxNode` seen from the caller. Non-pub.
struct SceneState {
    ui_roots: Vec<Box<dyn super::base_component::ElementTrait>>,
    scroll_offsets: HashMap<u64, (f32, f32)>,
    element_snapshots: HashMap<u64, Box<dyn Any>>,
    last_rsx_root: Option<RsxNode>,
}

impl SceneState {
    fn new() -> Self {
        Self {
            ui_roots: Vec::new(),
            scroll_offsets: HashMap::new(),
            element_snapshots: HashMap::new(),
            last_rsx_root: None,
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
    sampled_texture_cache: HashMap<u64, SampledTextureEntry>,
    frame_buffer_pool: HashMap<u32, FrameBufferEntry>,
    draw_rect_uniform_pool: Vec<DrawRectUniformBufferEntry>,
    draw_rect_uniform_cursor: usize,
    draw_rect_uniform_offset: u64,
    frame_stats: FrameStats,
    frame_presented: bool,
    last_frame_graph: Option<FrameGraph>,
    compile_cache: Option<CachedCompiledGraph>,
    debug_overlay_vertices: Vec<super::render_pass::debug_overlay_pass::DebugOverlayVertex>,
    debug_overlay_indices: Vec<u32>,
}

impl FrameRuntime {
    fn new(trace_fps: bool) -> Self {
        Self {
            frame_state: None,
            offscreen_render_target_pool: OffscreenRenderTargetPool::new(),
            sampled_texture_cache: HashMap::new(),
            frame_buffer_pool: HashMap::new(),
            draw_rect_uniform_pool: Vec::new(),
            draw_rect_uniform_cursor: 0,
            draw_rect_uniform_offset: 0,
            frame_stats: FrameStats::new(trace_fps),
            frame_presented: false,
            last_frame_graph: None,
            compile_cache: None,
            debug_overlay_vertices: Vec::new(),
            debug_overlay_indices: Vec::new(),
        }
    }
}

/// Phase-7 extraction. Owns every transition and animation plugin plus the
/// shared channel / claim bookkeeping they all consume. The
/// `TransitionHostAdapter` built on every tick borrows `transition_channels`
/// immutably and `transition_claims` mutably from here, so field names are
/// preserved verbatim to keep the adapter sites mechanical.
struct TransitionRuntime {
    transition_channels: HashSet<ChannelId>,
    transition_claims: HashMap<TrackKey<TrackTarget>, TransitionPluginId>,
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
            transition_channels: HashSet::from([
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
            ]),
            transition_claims: HashMap::new(),
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
    device: Option<wgpu::Device>,
    instance: Option<Instance>,
    window: Option<Window>,
    surface_format_preference: SurfaceFormatPreference,
    queue: Option<Queue>,
    msaa_sample_count: u32,
    surface_msaa_texture: Option<wgpu::Texture>,
    surface_msaa_view: Option<wgpu::TextureView>,
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
    promoted_base_signatures: HashMap<u64, u64>,
    promoted_composition_signatures: HashMap<u64, u64>,
    debug_previous_subtree_signatures: HashMap<u64, (u64, u64, u64, bool)>,
    promoted_reuse_cooldown_frames: u8,
    frame_box_models: Vec<super::base_component::BoxModelSnapshot>,
    cached_root_box_models: HashMap<u64, Vec<super::base_component::BoxModelSnapshot>>,
}

impl CompositorState {
    fn new() -> Self {
        Self {
            promotion_state: PromotionState::default(),
            promotion_config: ViewportPromotionConfig::default(),
            promoted_layer_updates: Vec::new(),
            promoted_base_signatures: HashMap::new(),
            promoted_composition_signatures: HashMap::new(),
            debug_previous_subtree_signatures: HashMap::new(),
            promoted_reuse_cooldown_frames: 0,
            frame_box_models: Vec::new(),
            cached_root_box_models: HashMap::new(),
        }
    }
}

struct CachedCompiledGraph {
    topology_hash: u64,
    graph: super::frame_graph::CompiledGraph,
}

#[derive(Clone)]
struct FrameBufferEntry {
    buffer: wgpu::Buffer,
    size: u64,
    usage: wgpu::BufferUsages,
}

struct DrawRectUniformBufferEntry {
    buffer: wgpu::Buffer,
    size: u64,
    /// Cached bind groups keyed by layout_cache_key.  The bind group binds the buffer
    /// at offset 0 / size=slot_size; the per-draw dynamic offset is supplied separately,
    /// so one bind group is valid for *all* slots in this buffer.
    bind_groups: HashMap<u64, wgpu::BindGroup>,
}

struct SampledTextureEntry {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    byte_size: u64,
}

impl Viewport {
    const DEFAULT_MSAA_SAMPLE_COUNT: u32 = 4;
    const PROMOTED_REUSE_COOLDOWN_FRAMES: u8 = 2;
    const SAMPLED_TEXTURE_PRESSURE_BYTES: u64 = 128 * 1024 * 1024;
    const SAMPLED_TEXTURE_EVICT_TO_BYTES: u64 = 96 * 1024 * 1024;

    fn invalidate_promoted_layer_reuse(&mut self) {
        self.compositor.promoted_base_signatures.clear();
        self.compositor.promoted_composition_signatures.clear();
        self.compositor.debug_previous_subtree_signatures.clear();
        self.compositor.promoted_layer_updates.clear();
        self.compositor.promoted_reuse_cooldown_frames = Self::PROMOTED_REUSE_COOLDOWN_FRAMES;
    }

    fn normalize_msaa_sample_count(sample_count: u32) -> u32 {
        match sample_count {
            1 | 2 | 4 | 8 | 16 => sample_count,
            0 => 1,
            _ => Self::DEFAULT_MSAA_SAMPLE_COUNT,
        }
    }

    fn is_style_driven_transition_channel(channel: ChannelId) -> bool {
        matches!(
            channel,
            CHANNEL_VISUAL_X
                | CHANNEL_VISUAL_Y
                | CHANNEL_LAYOUT_X
                | CHANNEL_LAYOUT_Y
                | CHANNEL_LAYOUT_WIDTH
                | CHANNEL_LAYOUT_HEIGHT
                | CHANNEL_STYLE_OPACITY
                | CHANNEL_STYLE_BORDER_RADIUS
                | CHANNEL_STYLE_BACKGROUND_COLOR
                | CHANNEL_STYLE_COLOR
                | CHANNEL_STYLE_BORDER_TOP_COLOR
                | CHANNEL_STYLE_BORDER_RIGHT_COLOR
                | CHANNEL_STYLE_BORDER_BOTTOM_COLOR
                | CHANNEL_STYLE_BORDER_LEFT_COLOR
        )
    }

    fn cancel_track_by_owner(&mut self, key: TrackKey<TrackTarget>) -> bool {
        let Some(owner) = self.transitions.transition_claims.get(&key).copied() else {
            return false;
        };
        let mut host = TransitionHostAdapter {
            registered_channels: &self.transitions.transition_channels,
            claims: &mut self.transitions.transition_claims,
        };
        if owner == ScrollTransitionPlugin::BUILTIN_PLUGIN_ID {
            self.transitions.scroll_transition_plugin.cancel_track(key, &mut host);
            return true;
        }
        if owner == LayoutTransitionPlugin::BUILTIN_PLUGIN_ID {
            self.transitions.layout_transition_plugin.cancel_track(key, &mut host);
            return true;
        }
        if owner == StyleTransitionPlugin::BUILTIN_PLUGIN_ID {
            self.transitions.style_transition_plugin.cancel_track(key, &mut host);
            return true;
        }
        if owner == VisualTransitionPlugin::BUILTIN_PLUGIN_ID {
            self.transitions.visual_transition_plugin.cancel_track(key, &mut host);
            return true;
        }
        self.transitions.transition_claims.remove(&key);
        false
    }

    fn cancel_disallowed_transition_tracks(
        &mut self,
        roots: &[Box<dyn super::base_component::ElementTrait>],
    ) -> bool {
        let allowlist = super::base_component::collect_transition_track_allowlist(roots);
        let active_keys = self.transitions.transition_claims.keys().copied().collect::<Vec<_>>();
        let mut canceled = false;
        for key in active_keys {
            if !Self::is_style_driven_transition_channel(key.channel) {
                continue;
            }
            if allowlist.contains(&key) {
                continue;
            }
            canceled |= self.cancel_track_by_owner(key);
        }
        canceled
    }

    fn sync_layout_transition_claims(&mut self) {
        let active_keys = self
            .transitions
            .layout_transition_plugin
            .active_track_keys()
            .into_iter()
            .collect::<HashSet<_>>();
        self.transitions.transition_claims.retain(|key, owner| {
            if !matches!(
                key.channel,
                CHANNEL_LAYOUT_X | CHANNEL_LAYOUT_Y | CHANNEL_LAYOUT_WIDTH | CHANNEL_LAYOUT_HEIGHT
            ) {
                return true;
            }
            let _ = owner;
            active_keys.contains(key)
        });
    }

    fn present_mode_from_env() -> wgpu::PresentMode {
        let Ok(raw) = std::env::var("RFGUI_PRESENT_MODE") else {
            return wgpu::PresentMode::AutoVsync;
        };
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto_novsync" | "auto-no-vsync" | "no_vsync" | "no-vsync" | "novsync" => {
                wgpu::PresentMode::AutoNoVsync
            }
            "fifo" => wgpu::PresentMode::Fifo,
            "mailbox" => wgpu::PresentMode::Mailbox,
            "immediate" => wgpu::PresentMode::Immediate,
            _ => wgpu::PresentMode::AutoVsync,
        }
    }

    fn alpha_mode_from_capabilities(
        alpha_modes: &[wgpu::CompositeAlphaMode],
    ) -> wgpu::CompositeAlphaMode {
        for preferred in [
            wgpu::CompositeAlphaMode::PostMultiplied,
            wgpu::CompositeAlphaMode::PreMultiplied,
            wgpu::CompositeAlphaMode::Inherit,
            wgpu::CompositeAlphaMode::Auto,
            wgpu::CompositeAlphaMode::Opaque,
        ] {
            if alpha_modes.contains(&preferred) {
                return preferred;
            }
        }
        wgpu::CompositeAlphaMode::Auto
    }

    fn trace_style_sample_apply(
        &self,
        roots: &[Box<dyn super::base_component::ElementTrait>],
        target: u64,
        field: StyleField,
        value: StyleValue,
        applied: bool,
        before_signatures: Option<(u64, u64)>,
    ) {
        if !self.debug_options.trace_reuse_path {
            return;
        }
        let promoted_root = roots.iter().find_map(|root| {
            let root_id = root.id();
            if !self.compositor.promotion_state.promoted_node_ids.contains(&root_id) {
                return None;
            }
            if root_id == target
                || super::base_component::subtree_contains_node(root.as_ref(), root_id, target)
            {
                Some(root_id)
            } else {
                None
            }
        });
        let state = roots.iter().rev().find_map(|root| {
            super::base_component::get_debug_element_render_state_by_id(root.as_ref(), target)
        });
        let ancestry = roots
            .iter()
            .rev()
            .find_map(|root| super::base_component::get_node_ancestry_ids(root.as_ref(), target));
        let after_signatures = roots.iter().rev().find_map(|root| {
            super::base_component::get_debug_promotion_signatures_by_id(root.as_ref(), target)
        });
        let state_desc = match state {
            Some(state) => format!(
                "bg=rgba({},{},{},{}) fg=rgba({},{},{},{}) opacity={:.3} border_radius={:.3}",
                state.background_rgba[0],
                state.background_rgba[1],
                state.background_rgba[2],
                state.background_rgba[3],
                state.foreground_rgba[0],
                state.foreground_rgba[1],
                state.foreground_rgba[2],
                state.foreground_rgba[3],
                state.opacity,
                state.border_radius,
            ),
            None => "state=missing".to_string(),
        };
        let promoted_root_desc = promoted_root
            .map(|node_id| format!("promoted_root={node_id}"))
            .unwrap_or_else(|| "promoted_root=none".to_string());
        let signature_desc = match (before_signatures, after_signatures) {
            (Some((before_self, before_clip)), Some((after_self, after_clip))) => format!(
                "sig_self={}=>{} sig_clip={}=>{}",
                before_self, after_self, before_clip, after_clip
            ),
            (None, Some((after_self, after_clip))) => {
                format!(
                    "sig_self=missing=>{} sig_clip=missing=>{}",
                    after_self, after_clip
                )
            }
            (Some((before_self, before_clip)), None) => {
                format!(
                    "sig_self={}=>missing sig_clip={}=>missing",
                    before_self, before_clip
                )
            }
            (None, None) => "sig=missing".to_string(),
        };
        let ancestry_desc = ancestry
            .map(|path| {
                let joined = path
                    .into_iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join("->");
                format!("ancestry={joined}")
            })
            .unwrap_or_else(|| "ancestry=missing".to_string());
        record_debug_style_sample_record(DebugStyleSampleRecord {
            target,
            promoted_root,
        });
        record_debug_style_sample(format!(
            "node={} field={} sample={} applied={} {} {} {} {}",
            target,
            format_style_field(field),
            format_style_value(&value),
            applied,
            promoted_root_desc,
            ancestry_desc,
            signature_desc,
            state_desc,
        ));
    }

    fn cancel_pointer_interactions(
        roots: &mut [Box<dyn super::base_component::ElementTrait>],
    ) -> bool {
        let mut changed = false;
        for root in roots.iter_mut() {
            changed |= super::base_component::cancel_pointer_interactions(root.as_mut());
        }
        changed
    }

    fn start_scroll_track(
        &mut self,
        target: TrackTarget,
        axis: ScrollAxis,
        from: f32,
        to: f32,
    ) -> bool {
        if (to - from).abs() <= 0.001 {
            return false;
        }
        let mut host = TransitionHostAdapter {
            registered_channels: &self.transitions.transition_channels,
            claims: &mut self.transitions.transition_claims,
        };
        if self
            .transitions
            .scroll_transition_plugin
            .start_scroll_track(&mut host, target, axis, from, to, self.transitions.scroll_transition)
            .is_err()
        {
            return false;
        }
        self.request_redraw();
        true
    }

    fn cancel_scroll_track(&mut self, target: TrackTarget, axis: ScrollAxis) {
        let key = TrackKey {
            target,
            channel: axis.channel_id(),
        };
        let mut host = TransitionHostAdapter {
            registered_channels: &self.transitions.transition_channels,
            claims: &mut self.transitions.transition_claims,
        };
        self.transitions.scroll_transition_plugin.cancel_track(key, &mut host);
    }

    fn apply_hover_target(
        roots: &mut [Box<dyn super::base_component::ElementTrait>],
        target: Option<u64>,
    ) -> bool {
        let mut changed = false;
        for root in roots.iter_mut() {
            if super::base_component::update_hover_state(root.as_mut(), target) {
                changed = true;
            }
        }
        changed
    }

    fn sync_hover_target(
        roots: &mut [Box<dyn super::base_component::ElementTrait>],
        hovered_node_id: &mut Option<u64>,
        next_target: Option<u64>,
    ) -> (bool, bool) {
        let transition_dispatched =
            super::base_component::dispatch_hover_transition(roots, *hovered_node_id, next_target);
        *hovered_node_id = next_target;
        let hover_changed = Self::apply_hover_target(roots, next_target);
        (hover_changed, transition_dispatched)
    }

    fn sync_hover_visual_only(
        roots: &mut [Box<dyn super::base_component::ElementTrait>],
        hovered_node_id: &mut Option<u64>,
        next_target: Option<u64>,
    ) -> bool {
        *hovered_node_id = next_target;
        Self::apply_hover_target(roots, next_target)
    }

    fn save_scroll_states(
        roots: &[Box<dyn super::base_component::ElementTrait>],
        map: &mut HashMap<u64, (f32, f32)>,
    ) {
        for root in roots {
            let offset = root.get_scroll_offset();
            if offset != (0.0, 0.0) {
                map.insert(root.id(), offset);
            }
            if let Some(children) = root.children() {
                Self::save_scroll_states(children, map);
            }
        }
    }

    fn restore_scroll_states(
        roots: &mut [Box<dyn super::base_component::ElementTrait>],
        map: &HashMap<u64, (f32, f32)>,
    ) {
        for root in roots {
            if let Some(offset) = map.get(&root.id()) {
                root.set_scroll_offset(*offset);
            }
            if let Some(children) = root.children_mut() {
                Self::restore_scroll_states(children, map);
            }
        }
    }

    fn save_element_snapshots(
        roots: &[Box<dyn super::base_component::ElementTrait>],
        map: &mut HashMap<u64, Box<dyn Any>>,
    ) {
        for root in roots {
            if let Some(snapshot) = root.snapshot_state() {
                map.insert(root.id(), snapshot);
            }
            if let Some(children) = root.children() {
                Self::save_element_snapshots(children, map);
            }
        }
    }

    fn restore_element_snapshots(
        roots: &mut [Box<dyn super::base_component::ElementTrait>],
        map: &HashMap<u64, Box<dyn Any>>,
    ) {
        for root in roots {
            if let Some(snapshot) = map.get(&root.id()) {
                let _ = root.restore_state(snapshot.as_ref());
            }
            if let Some(children) = root.children_mut() {
                Self::restore_element_snapshots(children, map);
            }
        }
    }

    fn extract_style_prop(props: &[(&'static str, PropValue)]) -> Result<Option<Style>, String> {
        let Some((_, value)) = props.iter().find(|(key, _)| *key == "style") else {
            return Ok(None);
        };
        Self::extract_style_from_value(value)
    }

    fn extract_style_from_value(value: &PropValue) -> Result<Option<Style>, String> {
        let schema = ElementStylePropSchema::from_prop_value(value.clone())
            .map_err(|_| "prop `style` expects ElementStylePropSchema value".to_string())?;
        Ok(Some(schema.to_style()))
    }

    /// Returns `Ok(true)` when the only difference between old props and `changed`/`removed`
    /// is a change to transform/transform-origin inside the `style` prop.
    fn is_transform_only_update(
        old_props: &[(&'static str, PropValue)],
        changed: &[(&'static str, PropValue)],
        removed: &[&'static str],
    ) -> Result<bool, String> {
        // Removals or extra changes mean it's more than a transform update.
        if !removed.is_empty() || changed.len() != 1 || changed[0].0 != "style" {
            return Ok(false);
        }
        let old_style = Self::extract_style_prop(old_props)?.unwrap_or_default();
        let new_style = Self::extract_style_from_value(&changed[0].1)?.unwrap_or_default();
        let ignored = [PropertyId::Transform, PropertyId::TransformOrigin];
        if old_style.clone().without_properties_recursive(&ignored)
            != new_style.clone().without_properties_recursive(&ignored)
        {
            return Ok(false);
        }
        let old_transform = old_style.get(PropertyId::Transform);
        let new_transform = new_style.get(PropertyId::Transform);
        let old_origin = old_style.get(PropertyId::TransformOrigin);
        let new_origin = new_style.get(PropertyId::TransformOrigin);
        // Transform is in the diff but hasn't actually changed — nothing to do.
        if old_transform == new_transform && old_origin == new_origin {
            return Ok(false);
        }
        Ok(true)
    }

    fn apply_transform_style_by_node_id(
        roots: &mut [Box<dyn super::base_component::ElementTrait>],
        node_id: u64,
        style: &Style,
    ) -> bool {
        for root in roots.iter_mut() {
            if let Some(element) = Self::element_by_id_mut(root.as_mut(), node_id) {
                let mut transform_style = Style::new();
                transform_style.set_transform(match style.get(PropertyId::Transform) {
                    Some(crate::ParsedValue::Transform(transform)) => transform.clone(),
                    _ => Transform::default(),
                });
                transform_style.set_transform_origin(
                    match style.get(PropertyId::TransformOrigin) {
                        Some(crate::ParsedValue::TransformOrigin(origin)) => *origin,
                        _ => TransformOrigin::center(),
                    },
                );
                element.apply_style(transform_style);
                return true;
            }
        }
        false
    }

    fn try_apply_redraw_only_transform_updates(&mut self, root: &RsxNode) -> Result<bool, String> {
        let Some(previous_root) = self.scene.last_rsx_root.as_ref() else {
            return Ok(false);
        };
        let patches = reconcile(Some(previous_root), root);
        if patches.is_empty() {
            self.scene.last_rsx_root = Some(root.clone());
            return Ok(true);
        }

        let mut updates = Vec::new();
        for patch in &patches {
            let Patch::UpdateElementProps { path, changed, removed } = patch else {
                return Ok(false);
            };
            let old_node = Self::rsx_node_by_index_path(previous_root, path)
                .ok_or_else(|| "invalid old RSX node path".to_string())?;
            let RsxNode::Element(old_element) = old_node else {
                return Ok(false);
            };
            if !Self::is_transform_only_update(&old_element.props, changed, removed)? {
                return Ok(false);
            }
            let style = Self::extract_style_from_value(&changed[0].1)?.unwrap_or_default();
            let node_id = super::renderer_adapter::rendered_node_id_by_index_path(root, path)?
                .ok_or_else(|| "target redraw patch resolved to a fragment".to_string())?;
            updates.push((node_id, style));
        }

        for (node_id, style) in &updates {
            if !Self::apply_transform_style_by_node_id(&mut self.scene.ui_roots, *node_id, style) {
                return Ok(false);
            }
        }
        self.scene.last_rsx_root = Some(root.clone());
        Ok(true)
    }

    fn rsx_node_by_index_path<'a>(node: &'a RsxNode, path: &[usize]) -> Option<&'a RsxNode> {
        if path.is_empty() {
            return Some(node);
        }
        let children = node.children()?;
        let child = children.get(path[0])?;
        Self::rsx_node_by_index_path(child, &path[1..])
    }

    fn element_by_id_mut(
        root: &mut dyn crate::view::base_component::ElementTrait,
        node_id: u64,
    ) -> Option<&mut super::base_component::Element> {
        if root.id() == node_id {
            return root
                .as_any_mut()
                .downcast_mut::<super::base_component::Element>();
        }
        if let Some(children) = root.children_mut() {
            for child in children.iter_mut() {
                if let Some(found) = Self::element_by_id_mut(child.as_mut(), node_id) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn apply_scroll_sample(
        roots: &mut [Box<dyn super::base_component::ElementTrait>],
        target: TrackTarget,
        axis: ScrollAxis,
        value: f32,
    ) -> bool {
        for root in roots.iter_mut().rev() {
            if let Some((x, y)) =
                super::base_component::get_scroll_offset_by_id(root.as_ref(), target)
            {
                let next = match axis {
                    ScrollAxis::X => (value, y),
                    ScrollAxis::Y => (x, value),
                };
                return super::base_component::set_scroll_offset_by_id(root.as_mut(), target, next);
            }
        }
        false
    }

    fn refresh_frame_box_models(
        &mut self,
        roots: &mut [Box<dyn super::base_component::ElementTrait>],
    ) {
        self.compositor.frame_box_models.clear();
        let mut active_root_ids = HashSet::new();
        for root in roots.iter_mut() {
            let root_id = root.id();
            active_root_ids.insert(root_id);
            let dirty = super::base_component::subtree_dirty_flags(root.as_ref());
            let needs_refresh = dirty.intersects(
                crate::view::base_component::DirtyFlags::LAYOUT
                    .union(crate::view::base_component::DirtyFlags::PLACE)
                    .union(crate::view::base_component::DirtyFlags::BOX_MODEL)
                    .union(crate::view::base_component::DirtyFlags::HIT_TEST),
            ) || !self.compositor.cached_root_box_models.contains_key(&root_id);
            if needs_refresh {
                let snapshots = super::base_component::collect_box_models(root.as_ref());
                self.compositor.cached_root_box_models.insert(root_id, snapshots);
            }
            if let Some(snapshots) = self.compositor.cached_root_box_models.get(&root_id) {
                self.compositor.frame_box_models.extend_from_slice(snapshots);
            }
        }
        self.compositor.cached_root_box_models
            .retain(|root_id, _| active_root_ids.contains(root_id));
        for root in roots.iter_mut() {
            super::base_component::clear_subtree_dirty_flags(
                root.as_mut(),
                crate::view::base_component::DirtyFlags::BOX_MODEL
                    .union(crate::view::base_component::DirtyFlags::HIT_TEST),
            );
        }
    }

    fn transition_timing(&mut self) -> (f32, f64) {
        let now = Instant::now();
        let dt = self
            .transitions
            .last_transition_tick
            .map(|last| (now - last).as_secs_f32())
            .unwrap_or(0.0);
        self.transitions.last_transition_tick = Some(now);
        let epoch = self.transitions.transition_epoch.get_or_insert(now);
        let now_seconds = (now - *epoch).as_secs_f64();
        (dt, now_seconds)
    }

    fn run_pre_layout_transitions(
        &mut self,
        roots: &mut [Box<dyn super::base_component::ElementTrait>],
        dt: f32,
        now_seconds: f64,
    ) -> bool {
        let mut layout_requests = Vec::new();
        for root in roots.iter_mut() {
            super::base_component::take_layout_transition_requests(
                root.as_mut(),
                &mut layout_requests,
            );
        }
        if !layout_requests.is_empty() {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            for request in layout_requests {
                let _ = self.transitions.layout_transition_plugin.start_layout_track(
                    &mut host,
                    request.target,
                    request.field,
                    request.from,
                    request.to,
                    request.transition,
                );
            }
        }
        let layout_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            self.transitions.layout_transition_plugin.run_tracks(
                TransitionFrame {
                    dt_seconds: dt,
                    now_seconds,
                },
                &mut host,
            )
        };
        self.sync_layout_transition_claims();
        let mut changed = false;
        let layout_samples = self.transitions.layout_transition_plugin.take_samples();
        for sample in layout_samples {
            for root in roots.iter_mut().rev() {
                if super::base_component::set_layout_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value.clone(),
                ) {
                    changed = true;
                    break;
                }
            }
        }
        if layout_result.keep_running {
            self.request_redraw();
        }
        changed || layout_result.keep_running
    }

    fn run_post_layout_transitions(
        &mut self,
        roots: &mut [Box<dyn super::base_component::ElementTrait>],
        dt: f32,
        now_seconds: f64,
    ) -> PostLayoutTransitionResult {
        let live_node_ids = super::base_component::collect_node_id_allowlist(roots);
        self.transitions.animation_plugin.prune_targets(&live_node_ids);
        let mut animation_requests = Vec::new();
        for root in roots.iter_mut() {
            super::base_component::take_animation_requests(root.as_mut(), &mut animation_requests);
        }
        let mut style_requests = Vec::new();
        for root in roots.iter_mut() {
            super::base_component::take_style_transition_requests(
                root.as_mut(),
                &mut style_requests,
            );
        }
        let mut layout_requests = Vec::new();
        for root in roots.iter_mut() {
            super::base_component::take_layout_transition_requests(
                root.as_mut(),
                &mut layout_requests,
            );
        }
        let mut visual_requests = Vec::new();
        for root in roots.iter_mut() {
            super::base_component::take_visual_transition_requests(
                root.as_mut(),
                &mut visual_requests,
            );
        }
        for request in animation_requests {
            self.transitions.animation_plugin.start_animator(request);
        }
        if !style_requests.is_empty() {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            for request in style_requests {
                if self.debug_options.trace_reuse_path {
                    record_debug_style_request(format!(
                        "target={} field={} from={} to={} duration_ms={} delay_ms={}",
                        request.target,
                        format_style_field(request.field),
                        format_style_value(&request.from),
                        format_style_value(&request.to),
                        request.transition.duration_ms,
                        request.transition.delay_ms,
                    ));
                }
                let _ = self.transitions.style_transition_plugin.start_style_track(
                    &mut host,
                    request.target,
                    request.field,
                    request.from,
                    request.to,
                    request.transition,
                );
            }
        }
        if !layout_requests.is_empty() {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            for request in layout_requests {
                let _ = self.transitions.layout_transition_plugin.start_layout_track(
                    &mut host,
                    request.target,
                    request.field,
                    request.from,
                    request.to,
                    request.transition,
                );
            }
        }
        if !visual_requests.is_empty() {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            for request in visual_requests {
                let _ = self.transitions.visual_transition_plugin.start_visual_track(
                    &mut host,
                    request.target,
                    request.field,
                    request.from,
                    request.to,
                    request.transition,
                );
            }
        }

        let scroll_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            self.transitions.scroll_transition_plugin.run_tracks(
                TransitionFrame {
                    dt_seconds: dt,
                    now_seconds,
                },
                &mut host,
            )
        };
        let style_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            self.transitions.style_transition_plugin.run_tracks(
                TransitionFrame {
                    dt_seconds: dt,
                    now_seconds,
                },
                &mut host,
            )
        };
        let animation_result = self.transitions.animation_plugin.run_animations(dt, now_seconds);
        let visual_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            self.transitions.visual_transition_plugin.run_tracks(
                TransitionFrame {
                    dt_seconds: dt,
                    now_seconds,
                },
                &mut host,
            )
        };
        let layout_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            self.transitions.layout_transition_plugin.run_tracks(
                TransitionFrame {
                    dt_seconds: dt,
                    now_seconds,
                },
                &mut host,
            )
        };
        self.sync_layout_transition_claims();
        let samples = self.transitions.scroll_transition_plugin.take_samples();
        let mut redraw_changed = false;
        let mut relayout_required = false;
        for sample in samples {
            redraw_changed |=
                Self::apply_scroll_sample(roots, sample.target, sample.axis, sample.value);
        }
        let style_samples = self.transitions.style_transition_plugin.take_samples();
        for sample in style_samples {
            let before_signatures = roots.iter().rev().find_map(|root| {
                super::base_component::get_debug_promotion_signatures_by_id(
                    root.as_ref(),
                    sample.target,
                )
            });
            let mut applied = false;
            for root in roots.iter_mut().rev() {
                if super::base_component::set_style_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value.clone(),
                ) {
                    redraw_changed = true;
                    if style_field_requires_relayout(sample.field) {
                        relayout_required = true;
                    }
                    applied = true;
                    break;
                }
            }
            self.trace_style_sample_apply(
                roots,
                sample.target,
                sample.field,
                sample.value,
                applied,
                before_signatures,
            );
        }
        let animation_style_samples = self.transitions.animation_plugin.take_style_samples();
        for sample in animation_style_samples {
            for root in roots.iter_mut().rev() {
                if super::base_component::set_style_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value.clone(),
                ) {
                    redraw_changed = true;
                    if style_field_requires_relayout(sample.field) {
                        relayout_required = true;
                    }
                    break;
                }
            }
        }
        let visual_samples = self.transitions.visual_transition_plugin.take_samples();
        for sample in visual_samples {
            for root in roots.iter_mut().rev() {
                if super::base_component::set_visual_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value.clone(),
                ) {
                    redraw_changed = true;
                    break;
                }
            }
        }
        let layout_samples = self.transitions.layout_transition_plugin.take_samples();
        for sample in layout_samples {
            for root in roots.iter_mut().rev() {
                if super::base_component::set_layout_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value,
                ) {
                    redraw_changed = true;
                    relayout_required = true;
                    break;
                }
            }
        }
        let animation_layout_samples = self.transitions.animation_plugin.take_layout_samples();
        for sample in animation_layout_samples {
            for root in roots.iter_mut().rev() {
                if super::base_component::set_layout_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value,
                ) {
                    redraw_changed = true;
                    relayout_required = true;
                    break;
                }
            }
        }
        if scroll_result.keep_running
            || style_result.keep_running
            || animation_result.keep_running
            || visual_result.keep_running
            || layout_result.keep_running
        {
            self.request_redraw();
            self.is_animating = true;
        }
        PostLayoutTransitionResult {
            redraw_changed: redraw_changed
                || scroll_result.keep_running
                || style_result.keep_running
                || animation_result.keep_running
                || visual_result.keep_running
                || layout_result.keep_running,
            relayout_required,
        }
    }

    fn sync_inflight_transition_state(
        &mut self,
        roots: &mut [Box<dyn super::base_component::ElementTrait>],
    ) -> bool {
        let live_node_ids = super::base_component::collect_node_id_allowlist(roots);
        self.transitions.animation_plugin.prune_targets(&live_node_ids);
        let now = Instant::now();
        let epoch = self.transitions.transition_epoch.get_or_insert(now);
        let frame = TransitionFrame {
            dt_seconds: 0.0,
            now_seconds: (now - *epoch).as_secs_f64(),
        };
        let scroll_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            self.transitions.scroll_transition_plugin.run_tracks(frame, &mut host)
        };
        let style_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            self.transitions.style_transition_plugin.run_tracks(frame, &mut host)
        };
        let animation_result = self.transitions.animation_plugin.run_animations(0.0, frame.now_seconds);
        let visual_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            self.transitions.visual_transition_plugin.run_tracks(frame, &mut host)
        };
        let layout_result = {
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            self.transitions.layout_transition_plugin.run_tracks(frame, &mut host)
        };
        self.sync_layout_transition_claims();

        let mut changed = false;
        for sample in self.transitions.scroll_transition_plugin.take_samples() {
            changed |= Self::apply_scroll_sample(roots, sample.target, sample.axis, sample.value);
        }
        for sample in self.transitions.style_transition_plugin.take_samples() {
            let before_signatures = roots.iter().rev().find_map(|root| {
                super::base_component::get_debug_promotion_signatures_by_id(
                    root.as_ref(),
                    sample.target,
                )
            });
            let mut applied = false;
            for root in roots.iter_mut().rev() {
                if super::base_component::set_style_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value.clone(),
                ) {
                    changed = true;
                    applied = true;
                    break;
                }
            }
            self.trace_style_sample_apply(
                roots,
                sample.target,
                sample.field,
                sample.value,
                applied,
                before_signatures,
            );
        }
        for sample in self.transitions.animation_plugin.take_style_samples() {
            for root in roots.iter_mut().rev() {
                if super::base_component::set_style_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value.clone(),
                ) {
                    changed = true;
                    break;
                }
            }
        }
        for sample in self.transitions.visual_transition_plugin.take_samples() {
            for root in roots.iter_mut().rev() {
                if super::base_component::set_visual_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value,
                ) {
                    changed = true;
                    break;
                }
            }
        }
        for sample in self.transitions.layout_transition_plugin.take_samples() {
            for root in roots.iter_mut().rev() {
                if super::base_component::set_layout_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value,
                ) {
                    changed = true;
                    break;
                }
            }
        }
        for sample in self.transitions.animation_plugin.take_layout_samples() {
            for root in roots.iter_mut().rev() {
                if super::base_component::set_layout_field_by_id(
                    root.as_mut(),
                    sample.target,
                    sample.field,
                    sample.value,
                ) {
                    changed = true;
                    break;
                }
            }
        }

        changed
            || scroll_result.keep_running
            || style_result.keep_running
            || animation_result.keep_running
            || visual_result.keep_running
            || layout_result.keep_running
    }

    fn update_promotion_state(&mut self, roots: &[Box<dyn super::base_component::ElementTrait>]) {
        let previous_promoted_node_ids = self.compositor.promotion_state.promoted_node_ids.clone();
        let active_animator_hints = self.transitions.animation_plugin.active_promotion_hints();
        let active_channels = active_channels_by_node(&self.transitions.transition_claims);
        let candidates = collect_promotion_candidates(
            roots,
            &active_animator_hints,
            &active_channels,
            (self.logical_width, self.logical_height),
        );
        let next_promotion_state = evaluate_promotion(
            candidates,
            (self.logical_width, self.logical_height),
            self.compositor.promotion_config,
        );
        let promotion_topology_changed =
            previous_promoted_node_ids != next_promotion_state.promoted_node_ids;
        self.compositor.promotion_state = next_promotion_state;
        if promotion_topology_changed {
            self.compositor.promoted_base_signatures.clear();
            self.compositor.promoted_composition_signatures.clear();
            self.compositor.promoted_layer_updates.clear();
            self.compositor.promoted_reuse_cooldown_frames = Self::PROMOTED_REUSE_COOLDOWN_FRAMES;
        }
        let (mut updates, next_base_signatures, next_composition_signatures) =
            collect_promoted_layer_updates(
                roots,
                &self.compositor.promotion_state.promoted_node_ids,
                &self.compositor.promoted_base_signatures,
                &self.compositor.promoted_composition_signatures,
            );
        if self.debug_options.trace_reuse_path {
            let subtree_signatures =
                collect_debug_subtree_signatures(roots, &self.compositor.promotion_state.promoted_node_ids);
            let previous_subtree_signatures = &self.compositor.debug_previous_subtree_signatures;
            let mut sampled_roots = snapshot_debug_style_sample_records()
                .into_iter()
                .filter_map(|record| record.promoted_root.map(|root| (record.target, root)))
                .collect::<Vec<_>>();
            sampled_roots.sort_unstable();
            sampled_roots.dedup();
            for (target, root_id) in sampled_roots {
                if let Some(update) = updates.iter().find(|update| update.node_id == root_id) {
                    let ancestry = roots
                        .iter()
                        .rev()
                        .find_map(|root| {
                            super::base_component::get_node_ancestry_ids(root.as_ref(), target)
                        })
                        .unwrap_or_default();
                    let walk_desc = ancestry
                        .into_iter()
                        .filter_map(|node_id| {
                            subtree_signatures
                                .get(&node_id)
                                .map(|(base, comp, output, has_output)| {
                                    let prev = previous_subtree_signatures.get(&node_id).copied();
                                    let prev_desc = prev
                                        .map(|(prev_base, prev_comp, prev_output, prev_has_out)| {
                                            format!(
                                                "prev_base={prev_base},prev_comp={prev_comp},prev_out={prev_output},prev_has_out={prev_has_out},"
                                            )
                                        })
                                        .unwrap_or_default();
                                    format!(
                                        "{node_id}[{prev_desc}base={base},comp={comp},out={output},has_out={has_output}]"
                                    )
                                })
                        })
                        .collect::<Vec<_>>()
                        .join(" -> ");
                    record_debug_style_promotion(format!(
                        "target={} promoted_root={} kind={:?} prev_base={:?} base={} prev_comp={:?} comp={} walk={}",
                        target,
                        root_id,
                        update.kind,
                        update.previous_base_signature,
                        update.base_signature,
                        update.previous_composition_signature,
                        update.composition_signature,
                        walk_desc
                    ));
                }
            }
            self.compositor.debug_previous_subtree_signatures = subtree_signatures;
        } else {
            self.compositor.debug_previous_subtree_signatures.clear();
        }
        if self.compositor.promoted_reuse_cooldown_frames > 0 {
            for update in &mut updates {
                update.kind = PromotedLayerUpdateKind::Reraster;
                update.composition_kind = PromotedLayerUpdateKind::Reraster;
            }
            self.compositor.promoted_reuse_cooldown_frames =
                self.compositor.promoted_reuse_cooldown_frames.saturating_sub(1);
        }
        self.compositor.promoted_layer_updates = updates;
        self.compositor.promoted_base_signatures = next_base_signatures;
        self.compositor.promoted_composition_signatures = next_composition_signatures;
    }

    fn apply_promotion_runtime(&self, ctx: &mut super::base_component::UiBuildContext) {
        let promoted_update_kinds = self
            .compositor
            .promoted_layer_updates
            .iter()
            .map(|update| (update.node_id, update.kind))
            .collect::<HashMap<_, _>>();
        let promoted_composition_update_kinds = self
            .compositor
            .promoted_layer_updates
            .iter()
            .map(|update| (update.node_id, update.composition_kind))
            .collect::<HashMap<_, _>>();
        ctx.set_promoted_runtime(
            Arc::new(self.compositor.promotion_state.promoted_node_ids.clone()),
            Arc::new(promoted_update_kinds),
            Arc::new(promoted_composition_update_kinds),
        );
    }

    fn composite_promoted_root(
        &self,
        graph: &mut FrameGraph,
        ctx: &mut super::base_component::UiBuildContext,
        root: &dyn super::base_component::ElementTrait,
        layer_target: super::render_pass::draw_rect_pass::RenderTargetOut,
    ) {
        let composite_bounds = root.promotion_composite_bounds();
        let opacity = if root
            .as_any()
            .downcast_ref::<super::base_component::Element>()
            .is_some()
        {
            1.0
        } else {
            root.promotion_node_info().opacity.clamp(0.0, 1.0)
        };
        let parent_target = ctx.current_target().unwrap_or_else(|| {
            let target = ctx.allocate_target(graph);
            ctx.set_current_target(target);
            target
        });
        ctx.set_current_target(parent_target);
        let pass = super::render_pass::composite_layer_pass::CompositeLayerPass::new(
            super::render_pass::composite_layer_pass::CompositeLayerParams {
                rect_pos: [composite_bounds.x, composite_bounds.y],
                rect_size: [composite_bounds.width, composite_bounds.height],
                corner_radii: composite_bounds.corner_radii,
                opacity,
                scissor_rect: None,
                clear_target: false,
            },
            super::render_pass::composite_layer_pass::CompositeLayerInput {
                layer: super::render_pass::composite_layer_pass::LayerIn::with_handle(
                    layer_target
                        .handle()
                        .expect("promoted root layer target should exist"),
                ),
                pass_context: ctx.graphics_pass_context(),
            },
            super::render_pass::composite_layer_pass::CompositeLayerOutput {
                render_target: parent_target,
            },
        );
        graph.add_graphics_pass(pass);
        ctx.set_current_target(parent_target);
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
                device: None,
                instance: None,
                window: None,
                surface_format_preference: SurfaceFormatPreference::default(),
                queue: None,
                msaa_sample_count: Self::DEFAULT_MSAA_SAMPLE_COUNT,
                surface_msaa_texture: None,
                surface_msaa_view: None,
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
            viewport_mouse_move_listeners: Vec::new(),
            viewport_mouse_up_listeners: Vec::new(),
        }
    }

    pub fn dump_graph(&self) -> Option<String> {
        self.frame.last_frame_graph.as_ref().map(|graph| graph.to_dot())
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

    pub fn debug_trace_fps(&self) -> bool {
        self.debug_options.trace_fps
    }

    pub fn set_debug_trace_fps(&mut self, enabled: bool) {
        self.debug_options.trace_fps = enabled;
        self.frame.frame_stats.set_enabled(enabled);
    }

    pub fn debug_trace_render_time(&self) -> bool {
        self.debug_options.trace_render_time
    }

    pub fn set_debug_trace_render_time(&mut self, enabled: bool) {
        self.debug_options.trace_render_time = enabled;
    }

    pub fn debug_trace_compile_detail(&self) -> bool {
        self.debug_options.trace_compile_detail
    }

    pub fn set_debug_trace_compile_detail(&mut self, enabled: bool) {
        self.debug_options.trace_compile_detail = enabled;
    }

    pub fn debug_trace_reuse_path(&self) -> bool {
        self.debug_options.trace_reuse_path
    }

    pub fn set_debug_trace_reuse_path(&mut self, enabled: bool) {
        self.debug_options.trace_reuse_path = enabled;
    }

    pub fn debug_geometry_overlay(&self) -> bool {
        self.debug_options.geometry_overlay
    }

    pub(crate) fn debug_overlay_enabled(&self) -> bool {
        self.debug_options.geometry_overlay || self.debug_options.trace_reuse_path
    }

    pub fn set_debug_geometry_overlay(&mut self, enabled: bool) {
        self.debug_options.geometry_overlay = enabled;
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

    fn push_debug_reuse_overlay_geometry(&mut self) {
        if !self.debug_options.trace_reuse_path {
            return;
        }
        let scale = self.scale_factor.max(0.0001);
        let screen_w = self.gpu.surface_config.width.max(1) as f32;
        let screen_h = self.gpu.surface_config.height.max(1) as f32;
        let snapshots_by_id = self
            .compositor
            .frame_box_models
            .iter()
            .map(|snapshot| (snapshot.node_id, *snapshot))
            .collect::<HashMap<_, _>>();
        let mut overlay_batches = Vec::new();
        let promoted_node_ids = self.compositor.promotion_state.promoted_node_ids.clone();
        for record in snapshot_debug_reuse_path() {
            let Some(snapshot) = snapshots_by_id.get(&record.node_id).copied() else {
                continue;
            };
            if !snapshot.should_render {
                continue;
            }
            let color = match (record.actual, record.reason) {
                (PromotedLayerUpdateKind::Reuse, _) => [0.15, 0.95, 0.35, 0.95],
                (PromotedLayerUpdateKind::Reraster, Some("child-scissor-clip-inline")) => {
                    [1.0, 0.9, 0.15, 0.95]
                }
                (PromotedLayerUpdateKind::Reraster, Some("child-stencil-clip-inline")) => {
                    [1.0, 0.55, 0.15, 0.95]
                }
                (
                    PromotedLayerUpdateKind::Reraster,
                    Some("absolute-viewport-clip-inline" | "absolute-anchor-clip-inline"),
                ) => [1.0, 0.2, 0.2, 0.95],
                (PromotedLayerUpdateKind::Reraster, Some(reason))
                    if reason.ends_with("-inline") =>
                {
                    [1.0, 0.8, 0.35, 0.95]
                }
                (PromotedLayerUpdateKind::Reraster, _) => [1.0, 0.45, 0.1, 0.95],
            };
            let label = promoted_node_ids
                .contains(&record.node_id)
                .then(|| record.node_id.to_string());
            let (vertices, indices) = build_reuse_overlay_geometry(
                &snapshot,
                scale,
                screen_w,
                screen_h,
                color,
                label.as_deref(),
            );
            overlay_batches.push((vertices, indices));
        }
        for (vertices, indices) in overlay_batches {
            self.push_debug_overlay_geometry(&vertices, &indices);
        }
    }

    /// Attach a surface target to the viewport.
    ///
    /// Accepts any `SurfaceTarget` — on native this is typically
    /// `Arc<winit::window::Window>`, on wasm it is
    /// `crate::platform::web::WebCanvasSurfaceTarget`. Both paths go through
    /// the same entry point; the viewport itself has no knowledge of the
    /// concrete type.
    pub async fn attach<T>(&mut self, target: T)
    where
        T: WindowHandle + Send + Sync + 'static,
    {
        self.gpu.window = Some(Arc::new(target));
        if self.gpu.device.is_some() {
            self.create_surface().await;
        }
    }

    /// Legacy entry point kept as a thin wrapper over `attach` for
    /// back-compat with existing examples. New code should call `attach`.
    #[deprecated(note = "use Viewport::attach")]
    pub async fn set_window(&mut self, window: Window) {
        self.gpu.window = Some(window);
        if self.gpu.device.is_some() {
            self.create_surface().await;
        }
    }

    pub fn set_surface_format_preference(&mut self, pref: SurfaceFormatPreference) {
        self.gpu.surface_format_preference = pref;
    }

    pub fn set_size(&mut self, mut width: u32, mut height: u32) {
        if width == 0 {
            width = 1;
        }
        if height == 0 {
            height = 1;
        }
        self.update_logical_size(width, height);
        if self.gpu.surface_config.width == width
            && self.gpu.surface_config.height == height
            && self.pending_size.is_none()
        {
            return;
        }
        self.pending_size = Some((width, height));
        self.needs_reconfigure = true;
        self.invalidate_promoted_layer_reuse();
    }

    pub fn set_style(&mut self, style: Style) {
        self.style = style;
        self.scene.last_rsx_root = None;
        self.request_redraw();
    }

    pub fn style(&self) -> &Style {
        &self.style
    }

    pub fn set_clear_color(&mut self, clear_color: Box<dyn ColorLike>) {
        self.clear_color = clear_color;
    }

    pub fn set_cursor(&mut self, cursor: Option<Cursor>) {
        self.cursor_override = cursor;
    }

    /// Push text the viewport wants written to the host clipboard into the
    /// pending platform request queue, and mirror it to the in-memory
    /// fallback so immediate reads from within this frame still see it.
    pub fn set_clipboard_text(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.clipboard_fallback = Some(text.clone());
        self.pending_platform_requests.clipboard_write = Some(text);
    }

    /// Return the in-memory clipboard fallback. Actual host-clipboard reads
    /// are the backend's responsibility; the backend is expected to push
    /// their results in via `set_clipboard_fallback` before dispatching
    /// events.
    pub fn clipboard_text(&mut self) -> Option<String> {
        self.clipboard_fallback.clone()
    }

    /// Seed the viewport's clipboard fallback with text the backend just
    /// read from the host clipboard. Called by the platform backend before
    /// dispatching an event that may ask for clipboard contents (e.g. a
    /// paste shortcut).
    pub fn set_clipboard_fallback(&mut self, text: Option<String>) {
        self.clipboard_fallback = text;
    }

    /// Drain the outbound platform requests accumulated since the last
    /// drain. Backends call this after each render/event batch and apply
    /// the results to the real window/clipboard.
    pub fn drain_platform_requests(&mut self) -> PlatformRequests {
        // Fold the internal `redraw_requested` flag into the drain so the
        // backend only has to look in one place.
        if self.redraw_requested {
            self.pending_platform_requests.request_redraw = true;
            self.redraw_requested = false;
        }
        std::mem::take(&mut self.pending_platform_requests)
    }

    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factor = scale_factor.max(0.0001);
        self.update_logical_size(self.gpu.surface_config.width, self.gpu.surface_config.height);
        self.invalidate_promoted_layer_reuse();
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    pub fn logical_size(&self) -> (f32, f32) {
        (self.logical_width, self.logical_height)
    }

    pub fn logical_scissor_to_physical(
        &self,
        scissor_rect: [u32; 4],
        target_size: (u32, u32),
    ) -> Option<[u32; 4]> {
        let scale = self.scale_factor.max(0.0001);
        let [x, y, width, height] = scissor_rect;
        let left = (x as f32 * scale).floor().max(0.0) as i64;
        let top = (y as f32 * scale).floor().max(0.0) as i64;
        let right = ((x as f32 + width as f32) * scale).ceil().max(0.0) as i64;
        let bottom = ((y as f32 + height as f32) * scale).ceil().max(0.0) as i64;
        let max_w = target_size.0 as i64;
        let max_h = target_size.1 as i64;

        let clamped_left = left.clamp(0, max_w);
        let clamped_top = top.clamp(0, max_h);
        let clamped_right = right.clamp(0, max_w);
        let clamped_bottom = bottom.clamp(0, max_h);
        if clamped_right <= clamped_left || clamped_bottom <= clamped_top {
            return None;
        }

        Some([
            clamped_left as u32,
            clamped_top as u32,
            (clamped_right - clamped_left) as u32,
            (clamped_bottom - clamped_top) as u32,
        ])
    }

    pub fn physical_to_logical_point(&self, x: f32, y: f32) -> (f32, f32) {
        let scale = self.scale_factor.max(0.0001);
        (x / scale, y / scale)
    }

    pub fn logical_to_physical_rect(&self, x: f32, y: f32, w: f32, h: f32) -> (i32, i32, u32, u32) {
        let scale = self.scale_factor.max(0.0001);
        (
            (x.max(0.0) * scale).round() as i32,
            (y.max(0.0) * scale).round() as i32,
            (w.max(1.0) * scale).ceil() as u32,
            (h.max(1.0) * scale).ceil() as u32,
        )
    }

    pub fn request_redraw(&mut self) {
        self.redraw_requested = true;
    }

    pub fn redraw_requested(&self) -> bool {
        self.redraw_requested
    }

    /// Returns true when the most recent render reported that one or
    /// more transition / animation plugins still wanted additional
    /// frames. Hosts use this to decide whether to pump the next frame
    /// immediately or sleep until the next user event.
    pub fn is_animating(&self) -> bool {
        self.is_animating
    }

    pub fn take_redraw_request(&mut self) -> bool {
        std::mem::take(&mut self.redraw_requested)
    }

    pub async fn create_surface(&mut self) {
        let Some(surface_target) = self.gpu.window.clone() else {
            return;
        };
        {
            let backends = wgpu::Backends::all();

            let instance = Instance::new(wgpu::InstanceDescriptor {
                backends,
                flags: wgpu::InstanceFlags::empty(),
                memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
                backend_options: wgpu::BackendOptions::default(),
                display: None,
            });

            let surface = instance.create_surface(surface_target).unwrap();

            let Ok(adapter) = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::default(),
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                })
                .await
            else {
                eprintln!("[warn] failed to acquire a GPU adapter");
                return;
            };

            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    experimental_features: wgpu::ExperimentalFeatures::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                    trace: wgpu::Trace::Off,
                })
                .await
                .unwrap();

            let caps = surface.get_capabilities(&adapter);
            let format = Self::pick_surface_format(
                &caps.formats,
                self.gpu.surface_format_preference,
                self.gpu.surface_config.format,
            );
            let Some(format) = format else {
                eprintln!("[warn] surface reported no supported formats");
                return;
            };
            self.gpu.surface_config.format = format;
            self.gpu.surface_config.alpha_mode = Self::alpha_mode_from_capabilities(&caps.alpha_modes);
            self.gpu.surface_config.view_formats = vec![self.gpu.surface_config.format];
            if let Some((width, height)) = self.pending_size.take() {
                self.gpu.surface_config.width = width;
                self.gpu.surface_config.height = height;
            }

            surface.configure(&device, &self.gpu.surface_config);

            self.gpu.instance = Some(instance);
            self.gpu.surface = Some(surface);
            self.gpu.device = Some(device);
            self.gpu.queue = Some(queue);
            self.release_render_resource_caches();
            self.invalidate_promoted_layer_reuse();
            self.create_frame_attachments();
            self.needs_reconfigure = false;
            if let Some(device) = self.gpu.device.as_ref() {
                if let Some(queue) = self.gpu.queue.as_ref() {
                    crate::view::render_pass::prewarm_text_pipeline(
                        device,
                        queue,
                        self.gpu.surface_config.format,
                        self.gpu.msaa_sample_count,
                    );
                }
            }
        }
    }

    /// Data-driven surface format selection. `preference` decides whether
    /// sRGB or non-sRGB formats win in the tiebreaker; `current` is kept if
    /// it's present in the capability list, otherwise the first matching
    /// format by preference wins. No `cfg` — the viewport has no platform
    /// knowledge; callers supply the preference.
    fn pick_surface_format(
        available: &[wgpu::TextureFormat],
        preference: SurfaceFormatPreference,
        current: wgpu::TextureFormat,
    ) -> Option<wgpu::TextureFormat> {
        match preference {
            // Native default. Match the pre-phase-2 behavior exactly: first
            // srgb format, else first available. The caller's `current`
            // default (`Bgra8Unorm`) is ignored on purpose — it's
            // non-srgb and would wash out colors if picked on a device
            // that also advertises the srgb variant.
            SurfaceFormatPreference::PreferSrgb => available
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .or_else(|| available.first().copied()),
            // Web/wasm default. Prefer a non-srgb format so the browser
            // compositor doesn't double-apply gamma. Honors `current` if it
            // is advertised by the surface (matches pre-phase-2 wasm code).
            SurfaceFormatPreference::PreferNonSrgb => available
                .iter()
                .copied()
                .find(|f| *f == current)
                .or_else(|| available.iter().copied().find(|f| !f.is_srgb()))
                .or_else(|| available.iter().copied().find(|f| f.is_srgb()))
                .or_else(|| available.first().copied()),
        }
    }

    fn apply_pending_reconfigure(&mut self) -> bool {
        if !self.needs_reconfigure {
            return true;
        }
        if let Some((width, height)) = self.pending_size.take() {
            self.gpu.surface_config.width = width;
            self.gpu.surface_config.height = height;
        }
        let surface = match &self.gpu.surface {
            Some(surface) => surface,
            None => return false,
        };
        let device = match &self.gpu.device {
            Some(device) => device,
            None => return false,
        };
        surface.configure(device, &self.gpu.surface_config);
        let device_for_prewarm = device.clone();
        self.release_render_resource_caches();
        self.invalidate_promoted_layer_reuse();
        self.create_frame_attachments();
        if let Some(queue) = self.gpu.queue.as_ref() {
            crate::view::render_pass::prewarm_text_pipeline(
                &device_for_prewarm,
                queue,
                self.gpu.surface_config.format,
                self.gpu.msaa_sample_count,
            );
        }
        self.needs_reconfigure = false;
        true
    }

    fn create_frame_attachments(&mut self) {
        self.create_surface_msaa_texture();
        self.create_depth_texture();
    }

    fn create_surface_msaa_texture(&mut self) {
        if self.gpu.msaa_sample_count <= 1 {
            self.gpu.surface_msaa_texture = None;
            self.gpu.surface_msaa_view = None;
            return;
        }
        let device = match &self.gpu.device {
            Some(d) => d,
            None => return,
        };
        let size = wgpu::Extent3d {
            width: self.gpu.surface_config.width.max(1),
            height: self.gpu.surface_config.height.max(1),
            depth_or_array_layers: 1,
        };
        let desc = wgpu::TextureDescriptor {
            label: Some("Surface MSAA Texture"),
            size,
            mip_level_count: 1,
            sample_count: self.gpu.msaa_sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: self.gpu.surface_config.format,
            usage: TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        };
        let texture = device.create_texture(&desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.gpu.surface_msaa_texture = Some(texture);
        self.gpu.surface_msaa_view = Some(view);
    }

    fn create_depth_texture(&mut self) {
        let device = match &self.gpu.device {
            Some(d) => d,
            None => return,
        };

        let size = wgpu::Extent3d {
            width: self.gpu.surface_config.width,
            height: self.gpu.surface_config.height,
            depth_or_array_layers: 1,
        };
        let desc = wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size,
            mip_level_count: 1,
            sample_count: self.gpu.msaa_sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let texture = device.create_texture(&desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        self.gpu.depth_texture = Some(texture);
        self.gpu.depth_view = Some(view);
    }

    fn render_render_tree(
        &mut self,
        roots: &mut [Box<dyn super::base_component::ElementTrait>],
        dt: f32,
        now_seconds: f64,
    ) -> bool {
        let frame_start = Instant::now();
        trace_promoted_build_frame_marker();
        begin_debug_reuse_path_frame();
        let begin_frame_profile = match self.begin_frame() {
            Some(profile) => profile,
            None => {
                return false;
            }
        };
        let begin_frame_elapsed_ms = begin_frame_profile.total_ms;
        let begin_frame_children = vec![
            TraceRenderNode::new("acquire_surface_texture", begin_frame_profile.acquire_ms),
            TraceRenderNode::new("create_surface_view", begin_frame_profile.create_view_ms),
            TraceRenderNode::new(
                "create_command_encoder",
                begin_frame_profile.create_encoder_ms,
            ),
        ];
        let layout_started_at = Instant::now();
        self.compositor.frame_box_models.clear();
        super::base_component::set_text_measure_profile_enabled(
            self.debug_options.trace_render_time,
        );
        super::base_component::reset_text_measure_profile();
        let measure_started_at = Instant::now();
        for root in roots.iter_mut() {
            root.measure(super::base_component::LayoutConstraints {
                max_width: self.logical_width,
                max_height: self.logical_height,
                viewport_width: self.logical_width,
                viewport_height: self.logical_height,
                percent_base_width: Some(self.logical_width),
                percent_base_height: Some(self.logical_height),
            });
        }
        let layout_measure_elapsed_ms = measure_started_at.elapsed().as_secs_f64() * 1000.0;
        let layout_text_measure_profile = super::base_component::take_text_measure_profile();
        let place_started_at = Instant::now();
        super::base_component::reset_layout_place_profile();
        for root in roots.iter_mut() {
            root.place(super::base_component::LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: self.logical_width,
                available_height: self.logical_height,
                viewport_width: self.logical_width,
                viewport_height: self.logical_height,
                percent_base_width: Some(self.logical_width),
                percent_base_height: Some(self.logical_height),
            });
        }
        let layout_place_elapsed_ms = place_started_at.elapsed().as_secs_f64() * 1000.0;
        let layout_place_profile = super::base_component::take_layout_place_profile();
        let collect_box_models_started_at = Instant::now();
        self.refresh_frame_box_models(roots);
        let layout_collect_box_models_elapsed_ms =
            collect_box_models_started_at.elapsed().as_secs_f64() * 1000.0;
        let layout_elapsed_ms = layout_started_at.elapsed().as_secs_f64() * 1000.0;

        // After layout is resolved for this frame, immediately run visual/style/scroll transitions
        // so their updated endpoints are visible in the same frame.
        let post_layout_transition_started_at = Instant::now();
        let post_layout_transition = self.run_post_layout_transitions(roots, dt, now_seconds);
        let post_layout_transition_elapsed_ms =
            post_layout_transition_started_at.elapsed().as_secs_f64() * 1000.0;
        let relayout_after_transition_started_at = Instant::now();
        let mut relayout_measure_elapsed_ms = 0.0_f64;
        let mut relayout_place_elapsed_ms = 0.0_f64;
        let mut relayout_collect_box_models_elapsed_ms = 0.0_f64;
        let mut relayout_place_profile = super::base_component::LayoutPlaceProfile::default();
        if post_layout_transition.relayout_required {
            self.compositor.frame_box_models.clear();
            super::base_component::reset_text_measure_profile();
            let relayout_measure_started_at = Instant::now();
            for root in roots.iter_mut() {
                root.measure(super::base_component::LayoutConstraints {
                    max_width: self.logical_width,
                    max_height: self.logical_height,
                    viewport_width: self.logical_width,
                    viewport_height: self.logical_height,
                    percent_base_width: Some(self.logical_width),
                    percent_base_height: Some(self.logical_height),
                });
            }
            relayout_measure_elapsed_ms =
                relayout_measure_started_at.elapsed().as_secs_f64() * 1000.0;
            let _ = super::base_component::take_text_measure_profile();
            let relayout_place_started_at = Instant::now();
            super::base_component::reset_layout_place_profile();
            for root in roots.iter_mut() {
                root.place(super::base_component::LayoutPlacement {
                    parent_x: 0.0,
                    parent_y: 0.0,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: self.logical_width,
                    available_height: self.logical_height,
                    viewport_width: self.logical_width,
                    viewport_height: self.logical_height,
                    percent_base_width: Some(self.logical_width),
                    percent_base_height: Some(self.logical_height),
                });
            }
            relayout_place_elapsed_ms = relayout_place_started_at.elapsed().as_secs_f64() * 1000.0;
            relayout_place_profile = super::base_component::take_layout_place_profile();
            let relayout_collect_started_at = Instant::now();
            self.refresh_frame_box_models(roots);
            relayout_collect_box_models_elapsed_ms =
                relayout_collect_started_at.elapsed().as_secs_f64() * 1000.0;
        }
        let relayout_after_transition_elapsed_ms =
            relayout_after_transition_started_at.elapsed().as_secs_f64() * 1000.0;

        let update_promotion_started_at = Instant::now();
        self.update_promotion_state(roots);
        let update_promotion_elapsed_ms =
            update_promotion_started_at.elapsed().as_secs_f64() * 1000.0;

        let build_graph_started_at = Instant::now();
        self.clear_debug_overlay_geometry();
        let mut graph = FrameGraph::new();
        let mut ctx = super::base_component::UiBuildContext::new(
            self.gpu.surface_config.width,
            self.gpu.surface_config.height,
            self.gpu.surface_config.format,
            self.scale_factor,
        );
        self.apply_promotion_runtime(&mut ctx);
        let clear_uses_premultiplied_alpha = matches!(
            self.gpu.surface_config.alpha_mode,
            wgpu::CompositeAlphaMode::PostMultiplied | wgpu::CompositeAlphaMode::PreMultiplied
        );
        let mut clear_rgba = self.clear_color.to_rgba_f32();
        if clear_uses_premultiplied_alpha {
            let a = clear_rgba[3].clamp(0.0, 1.0);
            clear_rgba[0] *= a;
            clear_rgba[1] *= a;
            clear_rgba[2] *= a;
            clear_rgba[3] = a;
        }

        let output = ctx.allocate_target(&mut graph);
        let output_handle = output.handle();
        ctx.set_current_target(output.clone());
        let clear_pass = super::frame_graph::ClearPass::new(
            super::render_pass::clear_pass::ClearParams::new(clear_rgba),
            super::render_pass::clear_pass::ClearInput {
                pass_context: ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            super::render_pass::clear_pass::ClearOutput {
                render_target: output.clone(),
                ..Default::default()
            },
        );
        if let Some(handle) = output_handle {
            ctx.set_color_target(Some(handle));
        }
        graph.add_graphics_pass(clear_pass);
        ctx.set_current_target(output);
        for root in roots.iter_mut() {
            if ctx.is_node_promoted(root.id()) {
                let root_id = root.id();
                let requested_update = ctx
                    .promoted_update_kind(root_id)
                    .unwrap_or(PromotedLayerUpdateKind::Reraster);
                if let Some(element) = root
                    .as_any_mut()
                    .downcast_mut::<super::base_component::Element>()
                {
                    if let Some(reason) = element.inline_promotion_rendering_reason() {
                        if reason != "child-scissor-clip-inline"
                            && reason != "child-stencil-clip-inline"
                        {
                            record_debug_reuse_path(DebugReusePathRecord {
                                node_id: root_id,
                                context: DebugReusePathContext::Root,
                                requested: requested_update,
                                can_reuse: false,
                                actual: PromotedLayerUpdateKind::Reraster,
                                reason: Some(reason),
                                clip_rect: element.absolute_clip_scissor_rect(),
                            });
                            let next_state = element.build(
                                &mut graph,
                                super::base_component::UiBuildContext::from_parts(
                                    ctx.viewport(),
                                    ctx.state_clone(),
                                ),
                            );
                            ctx.set_state(next_state);
                            continue;
                        }
                    }
                }
                let update_kind = requested_update;
                let can_reuse_subtree =
                    super::base_component::can_reuse_promoted_subtree(root.as_ref(), &ctx);
                let can_reuse = matches!(
                    update_kind,
                    crate::view::promotion::PromotedLayerUpdateKind::Reuse
                ) && can_reuse_subtree;
                let mut root_ctx = super::base_component::UiBuildContext::from_parts(
                    ctx.viewport(),
                    super::base_component::BuildState::for_layer_subtree_with_ancestor_clip(
                        ctx.ancestor_clip_context(),
                    ),
                );
                let layer_target = root_ctx.allocate_promoted_layer_target(
                    &mut graph,
                    root_id,
                    root.promotion_composite_bounds(),
                );
                root_ctx.set_current_target(layer_target);
                let next_state = if let Some(element) =
                    root.as_any_mut()
                        .downcast_mut::<super::base_component::Element>()
                {
                    element.build_promoted_layer(
                        &mut graph,
                        root_ctx,
                        update_kind,
                        can_reuse,
                        DebugReusePathContext::Root,
                    )
                } else if can_reuse {
                    record_debug_reuse_path(DebugReusePathRecord {
                        node_id: root.id(),
                        context: DebugReusePathContext::Root,
                        requested: update_kind,
                        can_reuse,
                        actual: PromotedLayerUpdateKind::Reuse,
                        reason: None,
                        clip_rect: None,
                    });
                    root_ctx.into_state()
                } else {
                    record_debug_reuse_path(DebugReusePathRecord {
                        node_id: root.id(),
                        context: DebugReusePathContext::Root,
                        requested: update_kind,
                        can_reuse,
                        actual: PromotedLayerUpdateKind::Reraster,
                        reason: if matches!(update_kind, PromotedLayerUpdateKind::Reuse) {
                            Some("reuse-blocked")
                        } else {
                            None
                        },
                        clip_rect: None,
                    });
                    graph.add_graphics_pass(super::frame_graph::ClearPass::new(
                        super::render_pass::clear_pass::ClearParams::new([0.0, 0.0, 0.0, 0.0]),
                        super::render_pass::clear_pass::ClearInput {
                            pass_context: root_ctx.graphics_pass_context(),
                            clear_depth_stencil: true,
                        },
                        super::render_pass::clear_pass::ClearOutput {
                            render_target: layer_target,
                        },
                    ));
                    root.build(&mut graph, root_ctx)
                };
                ctx.merge_child_state_side_effects(&next_state);
                let layer_target = next_state.current_target().unwrap_or(layer_target);
                self.composite_promoted_root(&mut graph, &mut ctx, root.as_ref(), layer_target);
            } else {
                let next_state = root.build(
                    &mut graph,
                    super::base_component::UiBuildContext::from_parts(
                        ctx.viewport(),
                        ctx.state_clone(),
                    ),
                );
                ctx.set_state(next_state);
            }
        }
        let mut deferred_node_ids = ctx.take_deferred_node_ids();
        let mut deferred_index = 0usize;
        while deferred_index < deferred_node_ids.len() {
            let node_id = deferred_node_ids[deferred_index];
            deferred_index += 1;
            for root in roots.iter_mut() {
                if super::base_component::build_node_by_id(
                    root.as_mut(),
                    node_id,
                    &mut graph,
                    &mut ctx,
                ) {
                    break;
                }
            }
            let newly_deferred = ctx.take_deferred_node_ids();
            if !newly_deferred.is_empty() {
                deferred_node_ids.extend(newly_deferred);
            }
        }
        self.push_debug_reuse_overlay_geometry();
        let dependency_handle = ctx.current_target().and_then(|target| target.handle());
        if let Some(dep_handle) = dependency_handle {
            let present_pass = super::render_pass::present_surface_pass::PresentSurfacePass::new(
                super::render_pass::present_surface_pass::PresentSurfaceParams,
                super::render_pass::present_surface_pass::PresentSurfaceInput {
                    source: super::render_pass::draw_rect_pass::RenderTargetIn::with_handle(
                        dep_handle,
                    ),
                    ..Default::default()
                },
                super::render_pass::present_surface_pass::PresentSurfaceOutput::default(),
            );
            let present_handle = graph.add_graphics_pass(present_pass);
            graph
                .add_pass_sink(
                    present_handle,
                    super::frame_graph::ExternalSinkKind::SurfacePresent,
                )
                .expect("surface present sink should register");
        }
        let build_graph_elapsed_ms = build_graph_started_at.elapsed().as_secs_f64() * 1000.0;

        let mut compile_elapsed_ms = 0.0_f64;
        let mut compile_children: Vec<TraceRenderNode> = Vec::new();
        // Take the cache out (moves ownership) so we can pass self mutably to compile.
        // On cache hit the graph is reused in-place; on miss it is dropped. Either way
        // the returned compiled_graph is stored back for the next frame.
        let prior_cache = self
            .frame
            .compile_cache
            .take()
            .map(|c| (c.topology_hash, c.graph));
        let compiled = match graph.compile_with_upload_cached(self, prior_cache) {
            Ok((profile, topology_hash, compiled_graph)) => {
                compile_elapsed_ms = profile.total_ms;
                compile_children =
                    build_compile_trace_nodes(&profile, self.debug_trace_compile_detail());
                self.frame.compile_cache = Some(CachedCompiledGraph {
                    topology_hash,
                    graph: compiled_graph,
                });
                true
            }
            Err(err) => {
                eprintln!("[warn] frame graph compile failed: {:?}", err);
                // compile_cache already cleared by take() above
                false
            }
        };

        let mut execute_elapsed_ms = 0.0_f64;
        let mut execute_pass_count = 0_usize;
        let mut execute_ordered_passes: Vec<(String, f64, usize)> = Vec::new();
        let mut execute_detail_ordered_passes: Vec<(String, f64, usize)> = Vec::new();
        if compiled {
            if let Ok(profile) = graph.execute_profiled(self) {
                execute_elapsed_ms = profile.total_ms;
                execute_pass_count = profile.pass_count;
                execute_ordered_passes = profile.ordered_passes;
                execute_detail_ordered_passes = profile.detail_ordered;
            }
        }

        let end_frame_profile = self.end_frame();
        let end_frame_elapsed_ms = end_frame_profile.total_ms;
        let end_frame_children = vec![
            TraceRenderNode::new("queue_submit", end_frame_profile.submit_ms),
            TraceRenderNode::new("present", end_frame_profile.present_ms),
        ];

        let total_elapsed_ms = frame_start.elapsed().as_secs_f64() * 1000.0;
        if self.debug_options.trace_render_time {
            let mut layout_measure_children = Vec::new();
            if layout_text_measure_profile.measure_inline_calls > 0 {
                layout_measure_children.push(TraceRenderNode::new(
                    format!(
                        "text.measure_inline (calls={})",
                        layout_text_measure_profile.measure_inline_calls
                    ),
                    layout_text_measure_profile.measure_inline_ms,
                ));
            }
            if layout_text_measure_profile.collect_wrapped_inline_fragments_calls > 0 {
                layout_measure_children.push(TraceRenderNode::new(
                    format!(
                        "text.collect_wrapped_inline_fragments (calls={}, hits={})",
                        layout_text_measure_profile.collect_wrapped_inline_fragments_calls,
                        layout_text_measure_profile.collect_wrapped_inline_fragments_cache_hits
                    ),
                    layout_text_measure_profile.collect_wrapped_inline_fragments_ms,
                ));
            }
            if layout_text_measure_profile.first_wrapped_fragment_calls > 0 {
                layout_measure_children.push(TraceRenderNode::new(
                    format!(
                        "text.first_wrapped_fragment (calls={}, hits={})",
                        layout_text_measure_profile.first_wrapped_fragment_calls,
                        layout_text_measure_profile.first_wrapped_fragment_cache_hits
                    ),
                    layout_text_measure_profile.first_wrapped_fragment_ms,
                ));
            }
            if layout_text_measure_profile.wrapped_suffix_fragments_calls > 0 {
                layout_measure_children.push(TraceRenderNode::new(
                    format!(
                        "text.wrapped_suffix_fragments (calls={}, hits={})",
                        layout_text_measure_profile.wrapped_suffix_fragments_calls,
                        layout_text_measure_profile.wrapped_suffix_fragments_cache_hits
                    ),
                    layout_text_measure_profile.wrapped_suffix_fragments_ms,
                ));
            }
            if layout_text_measure_profile.relayout_from_base_calls > 0 {
                layout_measure_children.push(TraceRenderNode::new(
                    format!(
                        "text.relayout_from_base (calls={}, hits={})",
                        layout_text_measure_profile.relayout_from_base_calls,
                        layout_text_measure_profile.relayout_from_base_cache_hits
                    ),
                    layout_text_measure_profile.relayout_from_base_ms,
                ));
            }
            if layout_text_measure_profile.ensure_shaped_base_buffer_calls > 0 {
                layout_measure_children.push(TraceRenderNode::new(
                    format!(
                        "text.ensure_shaped_base_buffer (calls={}, hits={})",
                        layout_text_measure_profile.ensure_shaped_base_buffer_calls,
                        layout_text_measure_profile.ensure_shaped_base_buffer_cache_hits
                    ),
                    layout_text_measure_profile.ensure_shaped_base_buffer_ms,
                ));
            }
            if layout_text_measure_profile.measure_text_layout_calls > 0 {
                layout_measure_children.push(TraceRenderNode::new(
                    format!(
                        "text.measure_text_layout (calls={}, hits={})",
                        layout_text_measure_profile.measure_text_layout_calls,
                        layout_text_measure_profile.measure_text_layout_cache_hits
                    ),
                    layout_text_measure_profile.measure_text_layout_ms,
                ));
            }
            if layout_text_measure_profile.trimmed_suffix_shape_line_calls > 0 {
                layout_measure_children.push(TraceRenderNode::new(
                    format!(
                        "text.trimmed_suffix_shape_line (calls={}, hits={})",
                        layout_text_measure_profile.trimmed_suffix_shape_line_calls,
                        layout_text_measure_profile.trimmed_suffix_shape_line_cache_hits
                    ),
                    layout_text_measure_profile.trimmed_suffix_shape_line_ms,
                ));
            }
            let layout_with_transition_elapsed_ms = layout_elapsed_ms
                + post_layout_transition_elapsed_ms
                + relayout_after_transition_elapsed_ms;
            let mut execute_children = if execute_ordered_passes.is_empty() {
                vec![TraceRenderNode::new(
                    format!("passes ({execute_pass_count})"),
                    0.0,
                )]
            } else {
                build_execute_detail_trace_nodes(execute_ordered_passes)
            };
            if !execute_detail_ordered_passes.is_empty() {
                let detail_total_ms: f64 = execute_detail_ordered_passes
                    .iter()
                    .map(|(_, elapsed_ms, _)| *elapsed_ms)
                    .sum();
                let detail_children =
                    build_execute_detail_trace_nodes(execute_detail_ordered_passes);
                execute_children.push(TraceRenderNode::with_children(
                    "execute_detail",
                    detail_total_ms,
                    detail_children,
                ));
            }
            let trace_root = TraceRenderNode::with_children(
                "render_tree",
                total_elapsed_ms,
                vec![
                    TraceRenderNode::with_children(
                        "begin_frame",
                        begin_frame_elapsed_ms,
                        begin_frame_children,
                    ),
                    TraceRenderNode::with_children(
                        "layout",
                        layout_with_transition_elapsed_ms,
                        vec![
                            TraceRenderNode::with_children(
                                "measure",
                                layout_measure_elapsed_ms,
                                layout_measure_children,
                            ),
                            TraceRenderNode::with_children(
                                "place",
                                layout_place_elapsed_ms,
                                build_layout_place_trace_nodes(&layout_place_profile),
                            ),
                            TraceRenderNode::new(
                                "collect_box_models",
                                layout_collect_box_models_elapsed_ms,
                            ),
                            TraceRenderNode::new(
                                "post_layout_transition",
                                post_layout_transition_elapsed_ms,
                            ),
                            TraceRenderNode::with_children(
                                "relayout_after_transition",
                                relayout_after_transition_elapsed_ms,
                                vec![
                                    TraceRenderNode::new("measure", relayout_measure_elapsed_ms),
                                    TraceRenderNode::with_children(
                                        "place",
                                        relayout_place_elapsed_ms,
                                        build_layout_place_trace_nodes(&relayout_place_profile),
                                    ),
                                    TraceRenderNode::new(
                                        "collect_box_models",
                                        relayout_collect_box_models_elapsed_ms,
                                    ),
                                ],
                            ),
                        ],
                    ),
                    TraceRenderNode::new("update_promotion_state", update_promotion_elapsed_ms),
                    TraceRenderNode::new("build_graph", build_graph_elapsed_ms),
                    TraceRenderNode::with_children("compile", compile_elapsed_ms, compile_children),
                    TraceRenderNode::with_children(
                        format!("execute (passes={execute_pass_count})"),
                        execute_elapsed_ms,
                        execute_children,
                    ),
                    TraceRenderNode::with_children(
                        "end_frame",
                        end_frame_elapsed_ms,
                        end_frame_children,
                    ),
                ],
            );
            println!("{}", format_trace_render_tree(&trace_root));
            println!(
                "{}",
                format_promotion_trace(
                    &self.compositor.promotion_state.decisions,
                    &self.compositor.promoted_layer_updates,
                    self.compositor.promotion_config.base_threshold,
                )
            );
        }
        super::base_component::set_text_measure_profile_enabled(false);
        if self.debug_options.trace_reuse_path {
            println!("{}", format_reuse_path_trace());
            println!("{}", format_style_request_trace());
            println!("{}", format_style_sample_trace());
            println!("{}", format_style_promotion_trace());
        }
        self.frame.frame_stats.record_frame(frame_start.elapsed());
        self.frame.last_frame_graph = Some(graph);
        post_layout_transition.redraw_changed
    }

    pub fn render_rsx(&mut self, root: &RsxNode) -> Result<(), String> {
        self.render_rsx_with_dirty(root, take_state_dirty())
    }

    /// Drain the thread-local queue populated by `ui::use_viewport()` and
    /// apply each action to this viewport. Called at the top of
    /// `render_rsx_with_dirty` so event handlers from the prior frame land
    /// before dirty flags are read.
    fn apply_pending_viewport_actions(&mut self) {
        let actions = crate::ui::drain_viewport_actions();
        if actions.is_empty() {
            return;
        }
        let mut promotion_dirty = false;
        for action in actions {
            match action {
                crate::ui::ViewportAction::SetDebugTraceFps(on) => self.set_debug_trace_fps(on),
                crate::ui::ViewportAction::SetDebugTraceRenderTime(on) => {
                    self.set_debug_trace_render_time(on);
                }
                crate::ui::ViewportAction::SetDebugTraceCompileDetail(on) => {
                    self.set_debug_trace_compile_detail(on);
                }
                crate::ui::ViewportAction::SetDebugTraceReusePath(on) => {
                    self.set_debug_trace_reuse_path(on);
                }
                crate::ui::ViewportAction::SetDebugGeometryOverlay(on) => {
                    self.set_debug_geometry_overlay(on);
                }
                crate::ui::ViewportAction::SetPromotionEnabled(on) => {
                    let mut cfg = self.compositor.promotion_config.clone();
                    cfg.enabled = on;
                    // Scene that previously relied on the atomic threshold
                    // swap in 01_window gets the same behavior here: a
                    // large threshold effectively disables layer promotion
                    // even though the `enabled` flag remains true in
                    // other call paths.
                    cfg.base_threshold = if on {
                        ViewportPromotionConfig::default().base_threshold
                    } else {
                        1000
                    };
                    self.set_promotion_config(cfg);
                    promotion_dirty = true;
                }
                crate::ui::ViewportAction::SetClearColor(color) => {
                    self.set_clear_color(Box::new(color));
                }
                crate::ui::ViewportAction::RequestRedraw => self.request_redraw(),
            }
        }
        if promotion_dirty {
            self.invalidate_promoted_layer_reuse();
        }
    }

    pub fn render_rsx_with_dirty(
        &mut self,
        root: &RsxNode,
        state_dirty: crate::ui::UiDirtyState,
    ) -> Result<(), String> {
        // Apply any viewport mutations that component event handlers
        // enqueued via `use_viewport()` during the previous tick. Must
        // run before dirty evaluation so toggles like trace_render_time
        // take effect on the upcoming frame.
        self.apply_pending_viewport_actions();
        // Reset the animation flag — transition plugins below will set
        // it back to true if any of them still want more frames.
        self.is_animating = false;
        let resource_dirty = crate::view::image_resource::take_image_redraw_dirty()
            || crate::view::svg_resource::take_svg_redraw_dirty();
        let root_changed = self.scene.last_rsx_root.as_ref() != Some(root);
        let mut needs_rebuild = state_dirty.needs_rebuild() || root_changed;
        if root_changed
            && state_dirty.is_redraw_only()
            && self.try_apply_redraw_only_transform_updates(root)?
        {
            needs_rebuild = false;
        }
        if needs_rebuild {
            // Clear and save current scroll states
            self.scene.scroll_offsets.clear();
            Self::save_scroll_states(&self.scene.ui_roots, &mut self.scene.scroll_offsets);
            self.scene.element_snapshots.clear();
            Self::save_element_snapshots(&self.scene.ui_roots, &mut self.scene.element_snapshots);
            let layout_snapshots =
                super::base_component::collect_layout_transition_snapshots(&self.scene.ui_roots);
            let (converted_roots, conversion_errors) =
                super::renderer_adapter::rsx_to_elements_lossy_with_context(
                    root,
                    &self.style,
                    self.logical_width,
                    self.logical_height,
                );
            if !conversion_errors.is_empty() {
                eprintln!(
                    "[render_rsx] skipped {} invalid node(s):\n{}",
                    conversion_errors.len(),
                    conversion_errors.join("\n")
                );
            }
            if converted_roots.is_empty() {
                eprintln!("[render_rsx] no valid root nodes converted; keep previous render tree");
                self.scene.last_rsx_root = Some(root.clone());
                return Ok(());
            }
            self.scene.ui_roots = converted_roots;
            self.scene.last_rsx_root = Some(root.clone());

            // Restore scroll states into new elements
            Self::restore_scroll_states(&mut self.scene.ui_roots, &self.scene.scroll_offsets);
            Self::restore_element_snapshots(&mut self.scene.ui_roots, &self.scene.element_snapshots);
            super::base_component::seed_layout_transition_snapshots(
                &mut self.scene.ui_roots,
                &layout_snapshots,
            );
            let mut rebuilt_roots = std::mem::take(&mut self.scene.ui_roots);
            let canceled_tracks = self.cancel_disallowed_transition_tracks(&rebuilt_roots);
            let has_inflight_transition = self.sync_inflight_transition_state(&mut rebuilt_roots);
            let reconciled_transition_state =
                super::base_component::reconcile_transition_runtime_state(
                    &mut rebuilt_roots,
                    &active_channels_by_node(&self.transitions.transition_claims),
                );
            self.scene.ui_roots = rebuilt_roots;
            if canceled_tracks || has_inflight_transition || reconciled_transition_state {
                self.request_redraw();
            }
        }
        self.sync_focus_dispatch();
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let canceled_tracks = self.cancel_disallowed_transition_tracks(&roots);
        let reconciled_transition_state = super::base_component::reconcile_transition_runtime_state(
            &mut roots,
            &active_channels_by_node(&self.transitions.transition_claims),
        );
        let (dt, now_seconds) = self.transition_timing();
        let transition_changed_before_render = canceled_tracks
            || reconciled_transition_state
            || self.run_pre_layout_transitions(&mut roots, dt, now_seconds);
        let mut transition_changed_after_layout = false;
        if !roots.is_empty() {
            transition_changed_after_layout = self.render_render_tree(&mut roots, dt, now_seconds);
        }
        let next_hover_target = self.mouse_position_viewport().and_then(|(x, y)| {
            roots
                .iter()
                .rev()
                .find_map(|root| super::base_component::hit_test(root.as_ref(), x, y))
        });
        let hover_changed = Self::sync_hover_visual_only(
            &mut roots,
            &mut self.input_state.hovered_node_id,
            next_hover_target,
        );
        if resource_dirty
            || hover_changed
            || transition_changed_before_render
            || transition_changed_after_layout
        {
            self.request_redraw();
        }
        if roots
            .iter()
            .any(|root| super::base_component::has_animation_frame_request(root.as_ref()))
        {
            self.request_redraw();
        }
        self.scene.ui_roots = roots;
        if std::mem::take(&mut self.frame.frame_presented) {
            self.notify_cursor_handler();
        }
        Ok(())
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

    pub(crate) fn acquire_offscreen_render_target(
        &mut self,
        allocation_id: AllocationId,
        desc: TextureDesc,
    ) -> Option<RenderTargetBundle> {
        let device = self.gpu.device.as_ref()?;
        let sample_count = desc.sample_count().max(1);
        self.frame.offscreen_render_target_pool
            .acquire(device, allocation_id, desc, sample_count)
    }

    pub(crate) fn acquire_persistent_render_target(
        &mut self,
        stable_key: u64,
        desc: TextureDesc,
    ) -> Option<RenderTargetBundle> {
        let device = self.gpu.device.as_ref()?;
        let sample_count = desc.sample_count().max(1);
        self.frame.offscreen_render_target_pool
            .acquire_persistent(device, stable_key, desc, sample_count)
    }

    pub(crate) fn upload_sampled_texture_rgba(
        &mut self,
        stable_key: u64,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        bytes: &[u8],
    ) -> bool {
        let Some(device) = self.gpu.device.as_ref() else {
            return false;
        };
        let Some(queue) = self.gpu.queue.as_ref() else {
            return false;
        };
        let width = width.max(1);
        let height = height.max(1);
        let recreate = self
            .frame
            .sampled_texture_cache
            .get(&stable_key)
            .is_none_or(|entry| {
                entry.width != width || entry.height != height || entry.format != format
            });
        if recreate {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Sampled Image Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.frame.sampled_texture_cache.insert(
                stable_key,
                SampledTextureEntry {
                    texture,
                    view,
                    width,
                    height,
                    format,
                    byte_size: width as u64 * height as u64 * 4,
                },
            );
        }
        let Some(entry) = self.frame.sampled_texture_cache.get(&stable_key) else {
            return false;
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &entry.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width.saturating_mul(4)),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.evict_sampled_textures_under_pressure();
        true
    }

    pub(crate) fn sampled_texture_view(&self, stable_key: u64) -> Option<wgpu::TextureView> {
        self.frame.sampled_texture_cache
            .get(&stable_key)
            .map(|entry| entry.view.clone())
    }

    fn total_sampled_texture_bytes(&self) -> u64 {
        self.frame.sampled_texture_cache
            .values()
            .map(|entry| entry.byte_size)
            .sum()
    }

    fn evict_sampled_textures_under_pressure(&mut self) {
        let mut total_bytes = self.total_sampled_texture_bytes();
        if total_bytes <= Self::SAMPLED_TEXTURE_PRESSURE_BYTES {
            return;
        }

        let mut candidates = self
            .frame
            .sampled_texture_cache
            .keys()
            .filter_map(|key| {
                let retention = crate::view::image_resource::image_asset_retention_info(*key)
                    .or_else(|| crate::view::svg_resource::svg_asset_retention_info(*key))?;
                (retention.ref_count == 0).then_some((*key, retention.last_access_tick))
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|(_, tick)| *tick);

        for (key, _) in candidates {
            if total_bytes <= Self::SAMPLED_TEXTURE_EVICT_TO_BYTES {
                break;
            }
            if let Some(entry) = self.frame.sampled_texture_cache.remove(&key) {
                total_bytes = total_bytes.saturating_sub(entry.byte_size);
            }
        }
    }

    pub(crate) fn acquire_frame_buffer(
        &mut self,
        allocation_id: AllocationId,
        desc: BufferDesc,
    ) -> Option<wgpu::Buffer> {
        let device = self.gpu.device.as_ref()?;
        let key = allocation_id.0;
        let recreate = self
            .frame
            .frame_buffer_pool
            .get(&key)
            .is_none_or(|entry| entry.size != desc.size || entry.usage != desc.usage);
        if recreate {
            let usage = desc.usage | wgpu::BufferUsages::COPY_DST;
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: desc.label,
                size: desc.size.max(1),
                usage,
                mapped_at_creation: false,
            });
            self.frame.frame_buffer_pool.insert(
                key,
                FrameBufferEntry {
                    buffer: buffer.clone(),
                    size: desc.size.max(1),
                    usage: desc.usage,
                },
            );
        }
        self.frame.frame_buffer_pool
            .get(&key)
            .map(|entry| entry.buffer.clone())
    }

    pub(crate) fn upload_frame_buffer(
        &mut self,
        allocation_id: AllocationId,
        desc: BufferDesc,
        offset: u64,
        data: &[u8],
    ) -> bool {
        if data.is_empty() {
            return true;
        }
        if offset % wgpu::COPY_BUFFER_ALIGNMENT != 0 {
            return false;
        }
        let Some(buffer) = self.acquire_frame_buffer(allocation_id, desc) else {
            return false;
        };
        if self.gpu.upload_staging_belt.is_none() {
            let Some(device) = self.gpu.device.as_ref().cloned() else {
                return false;
            };
            self.gpu.upload_staging_belt = Some(StagingBelt::new(device, 1024 * 1024));
        }
        let Some(frame) = self.frame.frame_state.as_mut() else {
            return false;
        };
        let Some(staging_belt) = self.gpu.upload_staging_belt.as_mut() else {
            return false;
        };
        let align = wgpu::COPY_BUFFER_ALIGNMENT as usize;
        let rem = data.len() % align;
        let padded_len = if rem == 0 {
            data.len()
        } else {
            data.len() + (align - rem)
        };
        let end = offset.saturating_add(padded_len as u64);
        if end > desc.size.max(1) {
            return false;
        }
        let Some(size) = wgpu::BufferSize::new(padded_len as u64) else {
            return false;
        };
        let mut mapped = staging_belt.write_buffer(&mut frame.encoder, &buffer, offset, size);
        mapped.slice(..).fill(0);
        mapped.slice(..data.len()).copy_from_slice(data);
        drop(mapped);
        true
    }

    pub(crate) fn upload_draw_rect_uniform(
        &mut self,
        data: &[u8],
        slot_size: u64,
        chunk_size: u64,
    ) -> Option<(wgpu::Buffer, u32, usize)> {
        if data.is_empty() || data.len() as u64 > slot_size {
            return None;
        }
        let device = self.gpu.device.as_ref()?.clone();
        if self.gpu.upload_staging_belt.is_none() {
            self.gpu.upload_staging_belt = Some(StagingBelt::new(device.clone(), 1024 * 1024));
        }
        let required_size = chunk_size.max(slot_size).max(1);
        let has_current_capacity = self
            .frame
            .draw_rect_uniform_pool
            .get(self.frame.draw_rect_uniform_cursor)
            .is_some_and(|entry| {
                entry.size >= required_size
                    && self.frame.draw_rect_uniform_offset.saturating_add(slot_size) <= entry.size
            });
        if !has_current_capacity
            && self
                .frame
                .draw_rect_uniform_pool
                .get(self.frame.draw_rect_uniform_cursor)
                .is_some()
        {
            self.frame.draw_rect_uniform_cursor = self.frame.draw_rect_uniform_cursor.saturating_add(1);
            self.frame.draw_rect_uniform_offset = 0;
        }
        let target_index = self.frame.draw_rect_uniform_cursor;
        if self.frame.draw_rect_uniform_pool.len() <= target_index {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("DrawRect Uniform Ring Buffer"),
                size: required_size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.frame.draw_rect_uniform_pool.push(DrawRectUniformBufferEntry {
                buffer,
                size: required_size,
                bind_groups: HashMap::new(),
            });
        } else if self.frame.draw_rect_uniform_pool[target_index].size < required_size {
            // Buffer reallocated — invalidate all cached bind groups for this slot.
            self.frame.draw_rect_uniform_pool[target_index] = DrawRectUniformBufferEntry {
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("DrawRect Uniform Ring Buffer"),
                    size: required_size,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                size: required_size,
                bind_groups: HashMap::new(),
            };
        }
        let dynamic_offset = self.frame.draw_rect_uniform_offset;
        let Some(size) = wgpu::BufferSize::new(slot_size) else {
            return None;
        };
        let buffer = self.frame.draw_rect_uniform_pool[target_index].buffer.clone();
        let frame = self.frame.frame_state.as_mut()?;
        let staging_belt = self.gpu.upload_staging_belt.as_mut()?;
        let mut mapped =
            staging_belt.write_buffer(&mut frame.encoder, &buffer, dynamic_offset, size);
        mapped.slice(..).fill(0);
        mapped.slice(..data.len()).copy_from_slice(data);
        drop(mapped);
        self.frame.draw_rect_uniform_offset = self.frame.draw_rect_uniform_offset.saturating_add(slot_size);
        Some((buffer, dynamic_offset as u32, target_index))
    }

    /// Return a cached bind group for the given uniform pool slot and pipeline layout key,
    /// creating and storing it on the first call.  Bind groups bind the pool buffer at
    /// offset 0 / size=slot_size; dynamic offsets are supplied per-draw, so one bind group
    /// is valid for every slot in the same pool buffer.
    pub(crate) fn get_or_create_draw_rect_bind_group(
        &mut self,
        pool_index: usize,
        layout_cache_key: u64,
        layout: &wgpu::BindGroupLayout,
        slot_size: u64,
    ) -> Option<wgpu::BindGroup> {
        let entry = self.frame.draw_rect_uniform_pool.get(pool_index)?;
        if let Some(bg) = entry.bind_groups.get(&layout_cache_key) {
            return Some(bg.clone());
        }
        let device = self.gpu.device.as_ref()?;
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("DrawRect Bind Group (Cached)"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &entry.buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(slot_size),
                }),
            }],
        });
        // Re-borrow mutably to insert (split borrow is not possible across Option).
        self.frame.draw_rect_uniform_pool
            .get_mut(pool_index)?
            .bind_groups
            .insert(layout_cache_key, bg.clone());
        Some(bg)
    }

    pub fn release_render_resource_caches(&mut self) {
        crate::view::render_pass::draw_rect_pass::clear_draw_rect_resources_cache();
        crate::view::render_pass::shadow_module::clear_shadow_resources_cache();
        crate::view::render_pass::text_pass::clear_text_resources_cache();
        crate::view::render_pass::blur_module::clear_blur_resources_cache();
        crate::view::render_pass::composite_layer_pass::clear_composite_layer_resources_cache();
        crate::view::render_pass::texture_composite_pass::clear_texture_composite_resources_cache();
        crate::view::render_pass::present_surface_pass::clear_present_surface_resources_cache();
        self.frame.offscreen_render_target_pool.clear();
        self.frame.sampled_texture_cache.clear();
        crate::view::image_resource::invalidate_uploaded_images();
        crate::view::svg_resource::invalidate_uploaded_images();
        self.frame.frame_buffer_pool.clear();
        self.frame.draw_rect_uniform_pool.clear();
        self.frame.draw_rect_uniform_cursor = 0;
        self.frame.draw_rect_uniform_offset = 0;
        self.gpu.upload_staging_belt = None;
    }

    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.gpu.surface_config.format
    }

    pub fn surface_size(&self) -> (u32, u32) {
        (self.gpu.surface_config.width, self.gpu.surface_config.height)
    }

    fn update_logical_size(&mut self, physical_width: u32, physical_height: u32) {
        let scale = self.scale_factor.max(0.0001);
        self.logical_width = (physical_width as f32 / scale).max(1.0);
        self.logical_height = (physical_height as f32 / scale).max(1.0);
    }

    pub fn frame_texture(&self) -> Option<&wgpu::Texture> {
        self.frame.frame_state
            .as_ref()
            .map(|frame| &frame.render_texture.texture)
    }

    pub fn frame_box_models(&self) -> &[super::base_component::BoxModelSnapshot] {
        &self.compositor.frame_box_models
    }

    pub fn promotion_decisions(&self) -> &[PromotionDecision] {
        &self.compositor.promotion_state.decisions
    }

    pub fn promoted_layer_updates(&self) -> &[PromotedLayerUpdate] {
        &self.compositor.promoted_layer_updates
    }

    pub fn promotion_config(&self) -> ViewportPromotionConfig {
        self.compositor.promotion_config
    }

    pub fn set_promotion_config(&mut self, config: ViewportPromotionConfig) {
        self.compositor.promotion_config = config;
    }

    pub fn set_focused_node_id(&mut self, node_id: Option<u64>) {
        self.input_state.focused_node_id = node_id;
    }

    pub fn focused_node_id(&self) -> Option<u64> {
        self.input_state.focused_node_id
    }

    pub fn set_pointer_capture_node_id(&mut self, node_id: Option<u64>) {
        self.input_state.pointer_capture_node_id = node_id;
    }

    pub fn pointer_capture_node_id(&self) -> Option<u64> {
        self.input_state.pointer_capture_node_id
    }

    pub fn has_viewport_mouse_listeners(&self) -> bool {
        !self.viewport_mouse_move_listeners.is_empty()
            || !self.viewport_mouse_up_listeners.is_empty()
    }

    fn apply_viewport_listener_actions(&mut self, actions: Vec<ViewportListenerAction>) {
        let mut selection_changed = false;
        for action in actions {
            match action {
                ViewportListenerAction::AddMouseMoveListener(handler) => {
                    self.viewport_mouse_move_listeners.push(handler);
                }
                ViewportListenerAction::AddMouseUpListener(handler) => {
                    self.viewport_mouse_up_listeners
                        .push(ViewportMouseUpListener::Persistent(handler));
                }
                ViewportListenerAction::AddMouseUpListenerUntil(handler) => {
                    self.viewport_mouse_up_listeners
                        .push(ViewportMouseUpListener::Until(handler));
                }
                ViewportListenerAction::SetFocus(node_id) => {
                    self.set_focused_node_id(node_id);
                }
                ViewportListenerAction::SetCursor(cursor) => {
                    self.set_cursor(cursor);
                }
                ViewportListenerAction::SelectTextRangeAll(target_id) => {
                    for root in self.scene.ui_roots.iter_mut().rev() {
                        if super::base_component::select_all_text_by_id(root.as_mut(), target_id) {
                            selection_changed = true;
                            break;
                        }
                    }
                }
                ViewportListenerAction::SelectTextRange {
                    target_id,
                    start,
                    end,
                } => {
                    for root in self.scene.ui_roots.iter_mut().rev() {
                        if super::base_component::select_text_range_by_id(
                            root.as_mut(),
                            target_id,
                            start,
                            end,
                        ) {
                            selection_changed = true;
                            break;
                        }
                    }
                }
                ViewportListenerAction::RemoveListener(handle) => {
                    self.remove_viewport_listener(handle);
                }
            }
        }
        if selection_changed {
            self.request_redraw();
        }
    }

    fn remove_viewport_listener(&mut self, handle: ViewportListenerHandle) {
        self.viewport_mouse_move_listeners
            .retain(|listener| listener.id() != handle.0);
        self.viewport_mouse_up_listeners
            .retain(|listener| listener.id() != handle.0);
    }

    pub fn set_selects(&mut self, selects: Vec<u64>) {
        self.input_state.selects = selects;
    }

    pub fn selects(&self) -> &[u64] {
        &self.input_state.selects
    }

    pub fn set_mouse_position_viewport(&mut self, x: f32, y: f32) {
        self.input_state.mouse_position_viewport = Some((x, y));
    }

    pub fn clear_mouse_position_viewport(&mut self) {
        self.input_state.mouse_position_viewport = None;
        self.input_state.pointer_capture_node_id = None;
        let (hover_changed, hover_event_dispatched) = Self::sync_hover_target(
            &mut self.scene.ui_roots,
            &mut self.input_state.hovered_node_id,
            None,
        );
        let pointer_changed = Self::cancel_pointer_interactions(&mut self.scene.ui_roots);
        if hover_changed || hover_event_dispatched || pointer_changed {
            self.request_redraw();
        }
    }

    pub fn mouse_position_viewport(&self) -> Option<(f32, f32)> {
        self.input_state.mouse_position_viewport
    }

    pub fn set_mouse_button_pressed(&mut self, button: MouseButton, pressed: bool) {
        if pressed {
            self.input_state.pressed_mouse_buttons.insert(button);
        } else {
            self.input_state.pressed_mouse_buttons.remove(&button);
        }
    }

    pub fn is_mouse_button_pressed(&self, button: MouseButton) -> bool {
        self.input_state.pressed_mouse_buttons.contains(&button)
    }

    pub fn pressed_mouse_buttons(&self) -> impl Iterator<Item = MouseButton> + '_ {
        self.input_state.pressed_mouse_buttons.iter().copied()
    }

    pub fn set_key_pressed(&mut self, key: impl Into<String>, pressed: bool) {
        let key = key.into();
        if pressed {
            self.input_state.pressed_keys.insert(key);
        } else {
            self.input_state.pressed_keys.remove(&key);
        }
    }

    pub fn is_key_pressed(&self, key: &str) -> bool {
        self.input_state.pressed_keys.contains(key)
    }

    pub fn pressed_keys(&self) -> impl Iterator<Item = &str> {
        self.input_state.pressed_keys.iter().map(String::as_str)
    }

    pub fn clear_input_state(&mut self) {
        self.set_focused_node_id(None);
        self.sync_focus_dispatch();
        let previous_hovered_node_id = self.input_state.hovered_node_id;
        self.input_state = InputState::default();
        self.input_state.hovered_node_id = previous_hovered_node_id;
        self.viewport_mouse_move_listeners.clear();
        self.viewport_mouse_up_listeners.clear();
        self.dispatched_focus_node_id = None;
        let (hover_changed, hover_event_dispatched) = Self::sync_hover_target(
            &mut self.scene.ui_roots,
            &mut self.input_state.hovered_node_id,
            None,
        );
        let pointer_changed = Self::cancel_pointer_interactions(&mut self.scene.ui_roots);
        if hover_changed || hover_event_dispatched || pointer_changed {
            self.request_redraw();
        }
    }

    fn dispatch_viewport_mouse_move_listeners(&mut self, event: &mut MouseMoveEvent) -> bool {
        if self.viewport_mouse_move_listeners.is_empty() {
            return false;
        }
        let mut handled = false;
        for listener in &mut self.viewport_mouse_move_listeners {
            listener.call(event);
            handled = true;
            if event.meta.propagation_stopped() {
                break;
            }
        }
        handled
    }

    fn dispatch_viewport_mouse_up_listeners(&mut self, event: &mut MouseUpEvent) -> bool {
        if self.viewport_mouse_up_listeners.is_empty() {
            return false;
        }
        let mut handled = false;
        let mut remove_ids = Vec::new();
        for listener in &mut self.viewport_mouse_up_listeners {
            match listener {
                ViewportMouseUpListener::Persistent(handler) => {
                    handler.call(event);
                    handled = true;
                }
                ViewportMouseUpListener::Until(handler) => {
                    handled = true;
                    if handler.call(event) {
                        remove_ids.push(handler.id());
                    }
                }
            }
            if event.meta.propagation_stopped() {
                break;
            }
        }
        if !remove_ids.is_empty() {
            self.viewport_mouse_up_listeners
                .retain(|listener| !remove_ids.contains(&listener.id()));
        }
        handled
    }

    pub fn dispatch_mouse_down_event(&mut self, button: MouseButton) -> bool {
        let Some((x, y)) = self.mouse_position_viewport() else {
            return false;
        };
        self.input_state.pending_click = None;
        let focus_before = self.focused_node_id();
        let buttons = self.current_ui_mouse_buttons();
        let meta = EventMeta::new(0);
        let mut event = MouseDownEvent {
            meta: meta.clone(),
            mouse: MouseEventData {
                viewport_x: x,
                viewport_y: y,
                local_x: 0.0,
                local_y: 0.0,
                current_target_width: 0.0,
                current_target_height: 0.0,
                button: Some(to_ui_mouse_button(button)),
                buttons,
                modifiers: self.current_key_modifiers(),
            },
            viewport: meta.viewport(),
        };
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if super::base_component::dispatch_mouse_down_from_hit_test(
                    root.as_mut(),
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        self.scene.ui_roots = roots;
        if handled {
            self.input_state.pending_click = Some(PendingClick {
                button,
                target_id: event.meta.target_id(),
                viewport_x: x,
                viewport_y: y,
            });
        }
        if let Some(capture_target_id) = event.meta.pointer_capture_target_id() {
            self.input_state.pointer_capture_node_id = Some(capture_target_id);
        }
        self.apply_viewport_listener_actions(event.meta.take_viewport_listener_actions());
        self.sync_focus_dispatch();
        if handled {
            let clicked_target = event.meta.target_id();
            let keep_focus_requested = event.meta.keep_focus_requested();
            let focus_after = self.focused_node_id();
            let focus_changed_by_handler = focus_after != focus_before;
            let clicked_within_focused_subtree = focus_before.is_some_and(|focus_id| {
                self.scene.ui_roots.iter().rev().any(|root| {
                    super::base_component::subtree_contains_node(
                        root.as_ref(),
                        focus_id,
                        clicked_target,
                    )
                })
            });
            if !focus_changed_by_handler {
                if keep_focus_requested || clicked_within_focused_subtree {
                    // Keep existing focus during controlled interactions or subtree clicks.
                } else if Some(clicked_target) != focus_before {
                    self.set_focused_node_id(Some(clicked_target));
                    self.sync_focus_dispatch();
                }
            }
            self.request_redraw();
        } else if self.focused_node_id().is_some() {
            self.set_focused_node_id(None);
            self.sync_focus_dispatch();
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_mouse_up_event(&mut self, button: MouseButton) -> bool {
        let Some((x, y)) = self.mouse_position_viewport() else {
            self.input_state.pointer_capture_node_id = None;
            let changed = Self::cancel_pointer_interactions(&mut self.scene.ui_roots);
            if changed {
                self.request_redraw();
            }
            return false;
        };
        let buttons = self.current_ui_mouse_buttons();
        let meta = EventMeta::new(0);
        let mut event = MouseUpEvent {
            meta: meta.clone(),
            mouse: MouseEventData {
                viewport_x: x,
                viewport_y: y,
                local_x: 0.0,
                local_y: 0.0,
                current_target_width: 0.0,
                current_target_height: 0.0,
                button: Some(to_ui_mouse_button(button)),
                buttons,
                modifiers: self.current_key_modifiers(),
            },
            viewport: meta.viewport(),
        };
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            if let Some(target_id) = control.viewport.pointer_capture_node_id() {
                for root in roots.iter_mut().rev() {
                    if super::base_component::dispatch_mouse_up_to_target(
                        root.as_mut(),
                        target_id,
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                        break;
                    }
                }
                control.viewport.set_pointer_capture_node_id(None);
            } else {
                for root in roots.iter_mut().rev() {
                    if super::base_component::dispatch_mouse_up_from_hit_test(
                        root.as_mut(),
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                        break;
                    }
                }
            }
        }
        let listener_handled = self.dispatch_viewport_mouse_up_listeners(&mut event);
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.ui_roots = roots;
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        if handled || listener_handled {
            self.request_redraw();
        }
        handled || listener_handled
    }

    pub fn dispatch_mouse_move_event(&mut self) -> bool {
        let Some((x, y)) = self.mouse_position_viewport() else {
            return false;
        };
        let redraw_requested_before = self.redraw_requested;
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let hover_target = roots
            .iter()
            .rev()
            .find_map(|root| super::base_component::hit_test(root.as_ref(), x, y));
        let (hover_changed, hover_event_dispatched) = Self::sync_hover_target(
            &mut roots,
            &mut self.input_state.hovered_node_id,
            hover_target,
        );
        let buttons = self.current_ui_mouse_buttons();
        let meta = EventMeta::new(0);
        let mut event = MouseMoveEvent {
            meta: meta.clone(),
            mouse: MouseEventData {
                viewport_x: x,
                viewport_y: y,
                local_x: 0.0,
                local_y: 0.0,
                current_target_width: 0.0,
                current_target_height: 0.0,
                button: None,
                buttons,
                modifiers: self.current_key_modifiers(),
            },
            viewport: meta.viewport(),
        };
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            if let Some(target_id) = control.viewport.pointer_capture_node_id() {
                for root in roots.iter_mut().rev() {
                    if super::base_component::dispatch_mouse_move_to_target(
                        root.as_mut(),
                        target_id,
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                        break;
                    }
                }
                if !handled {
                    control.viewport.set_pointer_capture_node_id(None);
                }
            } else {
                for root in roots.iter_mut().rev() {
                    if super::base_component::dispatch_mouse_move_from_hit_test(
                        root.as_mut(),
                        &mut event,
                        &mut control,
                    ) {
                        handled = true;
                        break;
                    }
                }
            }
        }
        let listener_handled = self.dispatch_viewport_mouse_move_listeners(&mut event);
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.ui_roots = roots;
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        let redraw_requested_during_event = !redraw_requested_before && self.redraw_requested;
        if hover_changed || hover_event_dispatched || redraw_requested_during_event {
            self.request_redraw();
        }
        handled || hover_changed || hover_event_dispatched || listener_handled
    }

    pub fn dispatch_click_event(&mut self, button: MouseButton) -> bool {
        let Some((x, y)) = self.mouse_position_viewport() else {
            return false;
        };
        let Some(pending_click) = self.input_state.pending_click.take() else {
            return false;
        };
        if pending_click.button != button {
            return false;
        }
        let buttons = self.current_ui_mouse_buttons();
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let hit_target = roots
            .iter()
            .rev()
            .find_map(|root| super::base_component::hit_test(root.as_ref(), x, y));
        let is_valid_click = is_valid_click_candidate(pending_click, button, hit_target, x, y);
        if !is_valid_click {
            self.scene.ui_roots = roots;
            return false;
        }
        let mut event = ClickEvent {
            meta: EventMeta::new(0),
            mouse: MouseEventData {
                viewport_x: x,
                viewport_y: y,
                local_x: 0.0,
                local_y: 0.0,
                current_target_width: 0.0,
                current_target_height: 0.0,
                button: Some(to_ui_mouse_button(button)),
                buttons,
                modifiers: self.current_key_modifiers(),
            },
        };
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if super::base_component::dispatch_click_to_target(
                    root.as_mut(),
                    pending_click.target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.ui_roots = roots;
        self.apply_viewport_listener_actions(pending_actions);
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_mouse_wheel_event(&mut self, delta_x: f32, delta_y: f32) -> bool {
        let Some((x, y)) = self.mouse_position_viewport() else {
            return false;
        };
        let mut pending_scroll_track: Option<(TrackTarget, (f32, f32), (f32, f32))> = None;
        let Some((root_index, target_id)) =
            Self::find_scroll_handler_at_pointer(&self.scene.ui_roots, x, y, delta_x, delta_y)
        else {
            return false;
        };
        if let Some(root) = self.scene.ui_roots.get_mut(root_index) {
            let Some(from) =
                super::base_component::get_scroll_offset_by_id(root.as_ref(), target_id)
            else {
                return false;
            };
            let _ = super::base_component::dispatch_scroll_to_target(
                root.as_mut(),
                target_id,
                delta_x,
                delta_y,
            );
            let Some(to) = super::base_component::get_scroll_offset_by_id(root.as_ref(), target_id)
            else {
                return false;
            };
            let _ = super::base_component::set_scroll_offset_by_id(root.as_mut(), target_id, from);

            if (to.0 - from.0).abs() > 0.001 || (to.1 - from.1).abs() > 0.001 {
                pending_scroll_track = Some((target_id, from, to));
            }
        }
        let mut handled = false;
        if let Some((target_id, from, to)) = pending_scroll_track {
            let transition_spec = self.transitions.scroll_transition;
            let mut host = TransitionHostAdapter {
                registered_channels: &self.transitions.transition_channels,
                claims: &mut self.transitions.transition_claims,
            };
            if (to.0 - from.0).abs() > 0.001 {
                let _ = self.transitions.scroll_transition_plugin.start_scroll_track(
                    &mut host,
                    target_id,
                    ScrollAxis::X,
                    from.0,
                    to.0,
                    transition_spec,
                );
            }
            if (to.1 - from.1).abs() > 0.001 {
                let _ = self.transitions.scroll_transition_plugin.start_scroll_track(
                    &mut host,
                    target_id,
                    ScrollAxis::Y,
                    from.1,
                    to.1,
                    transition_spec,
                );
            }
            handled = true;
        }
        if handled {
            self.request_redraw();
        }
        handled
    }

    fn find_scroll_handler_at_pointer(
        roots: &[Box<dyn super::base_component::ElementTrait>],
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
    ) -> Option<(usize, u64)> {
        let hit_target = roots
            .iter()
            .rev()
            .find_map(|root| super::base_component::hit_test(root.as_ref(), x, y))?;
        let mut best_match: Option<(usize, u64, usize)> = None;

        for (root_index, root) in roots.iter().enumerate() {
            let Some(target_path) =
                super::base_component::get_node_ancestry_ids(root.as_ref(), hit_target)
            else {
                continue;
            };
            let Some(handler_id) = super::base_component::find_scroll_handler_from_target(
                root.as_ref(),
                hit_target,
                delta_x,
                delta_y,
            ) else {
                continue;
            };
            let Some(handler_path) =
                super::base_component::get_node_ancestry_ids(root.as_ref(), handler_id)
            else {
                continue;
            };
            let ancestor_distance = target_path.len().saturating_sub(handler_path.len());
            match best_match {
                Some((_, _, best_distance)) if ancestor_distance >= best_distance => {}
                _ => best_match = Some((root_index, handler_id, ancestor_distance)),
            }
        }

        best_match.map(|(root_index, handler_id, _)| (root_index, handler_id))
    }

    pub fn dispatch_key_down_event(&mut self, key: String, code: String, repeat: bool) -> bool {
        let Some(target_id) = self.focused_node_id() else {
            return false;
        };
        let mut event = KeyDownEvent {
            meta: EventMeta::new(target_id),
            key: KeyEventData {
                key,
                code,
                repeat,
                modifiers: self.current_key_modifiers(),
            },
        };
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if super::base_component::dispatch_key_down_bubble(
                    root.as_mut(),
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.ui_roots = roots;
        self.apply_viewport_listener_actions(pending_actions);
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_key_up_event(&mut self, key: String, code: String, repeat: bool) -> bool {
        let Some(target_id) = self.focused_node_id() else {
            return false;
        };
        let mut event = KeyUpEvent {
            meta: EventMeta::new(target_id),
            key: KeyEventData {
                key,
                code,
                repeat,
                modifiers: self.current_key_modifiers(),
            },
        };
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if super::base_component::dispatch_key_up_bubble(
                    root.as_mut(),
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.ui_roots = roots;
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_text_input_event(&mut self, text: String) -> bool {
        if text.is_empty() {
            return false;
        }
        let Some(target_id) = self.focused_node_id() else {
            return false;
        };
        let mut event = TextInputEvent {
            meta: EventMeta::new(target_id),
            text,
        };
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if super::base_component::dispatch_text_input_bubble(
                    root.as_mut(),
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.ui_roots = roots;
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_ime_preedit_event(
        &mut self,
        text: String,
        cursor: Option<(usize, usize)>,
    ) -> bool {
        let Some(target_id) = self.focused_node_id() else {
            return false;
        };
        let mut event = ImePreeditEvent {
            meta: EventMeta::new(target_id),
            text,
            cursor,
        };
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if super::base_component::dispatch_ime_preedit_bubble(
                    root.as_mut(),
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.ui_roots = roots;
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_focus_event(&mut self, target_id: u64) -> bool {
        let mut event = FocusEvent {
            meta: EventMeta::new(target_id),
        };
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if super::base_component::dispatch_focus_bubble(
                    root.as_mut(),
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.ui_roots = roots;
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        if handled {
            self.request_redraw();
        }
        handled
    }

    pub fn dispatch_blur_event(&mut self, target_id: u64) -> bool {
        let mut event = BlurEvent {
            meta: EventMeta::new(target_id),
        };
        let mut roots = std::mem::take(&mut self.scene.ui_roots);
        let mut handled = false;
        {
            let mut control = ViewportControl::new(self);
            for root in roots.iter_mut().rev() {
                if super::base_component::dispatch_blur_bubble(
                    root.as_mut(),
                    target_id,
                    &mut event,
                    &mut control,
                ) {
                    handled = true;
                    break;
                }
            }
        }
        let pending_actions = event.meta.take_viewport_listener_actions();
        self.scene.ui_roots = roots;
        self.apply_viewport_listener_actions(pending_actions);
        self.sync_focus_dispatch();
        if handled {
            self.request_redraw();
        }
        handled
    }

    /// Dispatch a platform-neutral mouse event.
    ///
    /// Canonical entry point for backends (winit, web, headless). Internally
    /// forwards to the legacy primitive-argument `dispatch_mouse_*` methods;
    /// those remain public for now so component tests and existing callers
    /// keep working. New backend code should only ever see this method.
    pub fn dispatch_platform_mouse_event(&mut self, event: &PlatformMouseEvent) -> bool {
        match event.kind {
            PlatformMouseEventKind::Down(button) => {
                self.dispatch_mouse_down_event(mouse_button_from_platform(button))
            }
            PlatformMouseEventKind::Up(button) => {
                self.dispatch_mouse_up_event(mouse_button_from_platform(button))
            }
            PlatformMouseEventKind::Move { x, y } => {
                self.set_mouse_position_viewport(x, y);
                self.dispatch_mouse_move_event()
            }
            PlatformMouseEventKind::Click(button) => {
                self.dispatch_click_event(mouse_button_from_platform(button))
            }
        }
    }

    pub fn dispatch_platform_wheel_event(&mut self, event: &PlatformWheelEvent) -> bool {
        self.dispatch_mouse_wheel_event(event.delta_x, event.delta_y)
    }

    pub fn dispatch_platform_key_event(&mut self, event: &PlatformKeyEvent) -> bool {
        if event.pressed {
            self.dispatch_key_down_event(event.key.clone(), event.code.clone(), event.repeat)
        } else {
            self.dispatch_key_up_event(event.key.clone(), event.code.clone(), event.repeat)
        }
    }

    pub fn dispatch_platform_text_input(&mut self, event: &PlatformTextInput) -> bool {
        self.dispatch_text_input_event(event.text.clone())
    }

    pub fn dispatch_platform_ime_preedit(&mut self, event: &PlatformImePreedit) -> bool {
        let cursor = match (event.cursor_start, event.cursor_end) {
            (Some(start), Some(end)) => Some((start, end)),
            _ => None,
        };
        self.dispatch_ime_preedit_event(event.text.clone(), cursor)
    }

    fn current_ui_mouse_buttons(&self) -> UiMouseButtons {
        UiMouseButtons {
            left: self.is_mouse_button_pressed(MouseButton::Left),
            right: self.is_mouse_button_pressed(MouseButton::Right),
            middle: self.is_mouse_button_pressed(MouseButton::Middle),
            back: self.is_mouse_button_pressed(MouseButton::Back),
            forward: self.is_mouse_button_pressed(MouseButton::Forward),
        }
    }

    pub fn focused_ime_cursor_rect(&self) -> Option<(f32, f32, f32, f32)> {
        let target_id = self.focused_node_id()?;
        for root in self.scene.ui_roots.iter().rev() {
            if let Some(rect) =
                super::base_component::get_ime_cursor_rect_by_id(root.as_ref(), target_id)
            {
                return Some(rect);
            }
        }
        None
    }

    fn current_key_modifiers(&self) -> KeyModifiers {
        KeyModifiers {
            alt: self.is_key_pressed("Named(Alt)")
                || self.is_key_pressed("Named(AltGraph)")
                || self.is_key_pressed("Code(AltLeft)")
                || self.is_key_pressed("Code(AltRight)"),
            ctrl: self.is_key_pressed("Named(Control)")
                || self.is_key_pressed("Code(ControlLeft)")
                || self.is_key_pressed("Code(ControlRight)"),
            shift: self.is_key_pressed("Named(Shift)")
                || self.is_key_pressed("Code(ShiftLeft)")
                || self.is_key_pressed("Code(ShiftRight)"),
            meta: self.is_key_pressed("Named(Super)")
                || self.is_key_pressed("Named(Meta)")
                || self.is_key_pressed("Code(SuperLeft)")
                || self.is_key_pressed("Code(SuperRight)")
                || self.is_key_pressed("Code(MetaLeft)")
                || self.is_key_pressed("Code(MetaRight)"),
        }
    }

    fn sync_focus_dispatch(&mut self) {
        if self.scene.ui_roots.is_empty() {
            return;
        }

        loop {
            let desired = self.input_state.focused_node_id;
            let dispatched = self.dispatched_focus_node_id;
            if desired == dispatched {
                break;
            }

            // Mark the in-flight target first so reentrant redraws triggered
            // by focus/blur handlers do not redispatch the same focus change.
            self.dispatched_focus_node_id = desired;

            if let Some(prev_id) = dispatched {
                let _ = self.dispatch_blur_event(prev_id);
            }
            if let Some(next_id) = desired {
                let _ = self.dispatch_focus_event(next_id);
            }
        }
    }

    fn resolve_cursor(&self) -> Cursor {
        if let Some(cursor) = self.cursor_override {
            return cursor;
        }
        let Some(target_id) = self.input_state.hovered_node_id else {
            return Cursor::Default;
        };
        for root in self.scene.ui_roots.iter().rev() {
            if let Some(cursor) = super::base_component::get_cursor_by_id(root.as_ref(), target_id)
            {
                return cursor;
            }
        }
        Cursor::Default
    }

    /// Record the currently-desired cursor into the pending platform
    /// request queue. Deduped against the last value recorded — the backend
    /// only sees changes.
    fn notify_cursor_handler(&mut self) {
        let cursor = self.resolve_cursor();
        if self.last_recorded_cursor == Some(cursor) {
            return;
        }
        self.last_recorded_cursor = Some(cursor);
        self.pending_platform_requests.cursor = Some(cursor);
    }

    fn begin_frame(&mut self) -> Option<BeginFrameProfile> {
        let total_started_at = Instant::now();
        if self.frame.frame_state.is_some() {
            return Some(BeginFrameProfile {
                total_ms: 0.0,
                acquire_ms: 0.0,
                create_view_ms: 0.0,
                create_encoder_ms: 0.0,
            });
        }
        if !self.apply_pending_reconfigure() {
            return None;
        }
        self.frame.offscreen_render_target_pool.begin_frame();
        self.frame.draw_rect_uniform_cursor = 0;
        self.frame.draw_rect_uniform_offset = 0;
        crate::view::render_pass::draw_rect_pass::begin_draw_rect_resources_frame();
        crate::view::render_pass::shadow_module::begin_shadow_resources_frame();

        let surface = match &self.gpu.surface {
            Some(s) => s,
            None => return None,
        };
        let device = match &self.gpu.device {
            Some(d) => d,
            None => return None,
        };

        let acquire_started_at = Instant::now();
        let render_texture = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                surface.configure(device, &self.gpu.surface_config);
                texture
            }
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                println!("[warn] surface lost, recreate render texture");
                surface.configure(device, &self.gpu.surface_config);
                match surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(texture)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
                    _ => return None,
                }
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return None,
        };
        let acquire_ms = acquire_started_at.elapsed().as_secs_f64() * 1000.0;

        let create_view_started_at = Instant::now();
        let surface_view = render_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let (view, resolve_view) = if self.gpu.msaa_sample_count > 1 {
            let Some(msaa_view) = self.gpu.surface_msaa_view.as_ref() else {
                return None;
            };
            (msaa_view.clone(), Some(surface_view))
        } else {
            (surface_view, None)
        };
        let create_view_ms = create_view_started_at.elapsed().as_secs_f64() * 1000.0;

        let create_encoder_started_at = Instant::now();
        let encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let create_encoder_ms = create_encoder_started_at.elapsed().as_secs_f64() * 1000.0;

        self.frame.frame_state = Some(FrameState {
            render_texture,
            view,
            resolve_view,
            encoder,
            depth_view: self.gpu.depth_view.clone(),
        });
        Some(BeginFrameProfile {
            total_ms: total_started_at.elapsed().as_secs_f64() * 1000.0,
            acquire_ms,
            create_view_ms,
            create_encoder_ms,
        })
    }

    fn end_frame(&mut self) -> EndFrameProfile {
        let total_started_at = Instant::now();
        let frame = match self.frame.frame_state.take() {
            Some(frame) => frame,
            None => {
                return EndFrameProfile {
                    total_ms: 0.0,
                    submit_ms: 0.0,
                    present_ms: 0.0,
                };
            }
        };
        if let Some(staging_belt) = self.gpu.upload_staging_belt.as_mut() {
            staging_belt.finish();
        }

        let submit_started_at = Instant::now();
        let queue = self.gpu.queue.as_ref().unwrap();
        queue.submit(Some(frame.encoder.finish()));
        if let Some(staging_belt) = self.gpu.upload_staging_belt.as_mut() {
            staging_belt.recall();
        }
        let submit_ms = submit_started_at.elapsed().as_secs_f64() * 1000.0;

        let present_started_at = Instant::now();
        frame.render_texture.present();
        let present_ms = present_started_at.elapsed().as_secs_f64() * 1000.0;
        self.frame.frame_presented = true;
        EndFrameProfile {
            total_ms: total_started_at.elapsed().as_secs_f64() * 1000.0,
            submit_ms,
            present_ms,
        }
    }
}

/// Convert a platform-neutral mouse button into the viewport-internal
/// `MouseButton` enum. Kept as a free function (rather than `From`) so the
/// viewport owns the mapping without leaking its internal type into the
/// platform crate.
fn mouse_button_from_platform(button: PlatformMouseButton) -> MouseButton {
    match button {
        PlatformMouseButton::Left => MouseButton::Left,
        PlatformMouseButton::Right => MouseButton::Right,
        PlatformMouseButton::Middle => MouseButton::Middle,
        PlatformMouseButton::Back => MouseButton::Back,
        PlatformMouseButton::Forward => MouseButton::Forward,
        PlatformMouseButton::Other(code) => MouseButton::Other(code),
    }
}

#[cfg(any())]
mod tests_legacy {
    use super::{
        MouseButton, PendingClick, append_overlay_label_geometry, build_reuse_overlay_geometry,
        is_valid_click_candidate,
    };
    use crate::transition::CHANNEL_STYLE_BOX_SHADOW;
    use crate::ui::{Binding, RsxNode, UiDirtyState};
    use crate::view::Element as HostElement;
    use crate::view::base_component::BoxModelSnapshot;
    use crate::view::base_component::{
        Element, LayoutConstraints, LayoutPlacement, Layoutable, get_scroll_offset_by_id,
        set_scroll_offset_by_id,
    };
    use crate::{
        Length, ParsedValue, PropertyId, ScrollDirection, Style, Transform, Transition,
        TransitionProperty, Transitions, Translate,
    };

    fn place_root(root: &mut Element, width: f32, height: f32) {
        root.measure(LayoutConstraints {
            max_width: width,
            max_height: height,
            viewport_width: width,
            percent_base_width: Some(width),
            percent_base_height: Some(height),
            viewport_height: height,
        });
        root.place(LayoutPlacement {
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
        });
    }

    fn element_by_id_mut(
        root: &mut dyn crate::view::base_component::ElementTrait,
        node_id: u64,
    ) -> Option<&mut Element> {
        if root.id() == node_id {
            return root.as_any_mut().downcast_mut::<Element>();
        }
        if let Some(children) = root.children_mut() {
            for child in children.iter_mut() {
                if let Some(found) = element_by_id_mut(child.as_mut(), node_id) {
                    return Some(found);
                }
            }
        }
        None
    }

    #[test]
    fn click_requires_same_button_and_target() {
        let pending = PendingClick {
            button: MouseButton::Left,
            target_id: 42,
            viewport_x: 10.0,
            viewport_y: 10.0,
        };

        assert!(is_valid_click_candidate(
            pending,
            MouseButton::Left,
            Some(42),
            12.0,
            12.0
        ));
        assert!(!is_valid_click_candidate(
            pending,
            MouseButton::Right,
            Some(42),
            12.0,
            12.0
        ));
        assert!(!is_valid_click_candidate(
            pending,
            MouseButton::Left,
            Some(99),
            12.0,
            12.0
        ));
    }

    #[test]
    fn click_rejects_large_pointer_travel() {
        let pending = PendingClick {
            button: MouseButton::Left,
            target_id: 7,
            viewport_x: 10.0,
            viewport_y: 10.0,
        };

        assert!(is_valid_click_candidate(
            pending,
            MouseButton::Left,
            Some(7),
            14.0,
            13.0
        ));
        assert!(!is_valid_click_candidate(
            pending,
            MouseButton::Left,
            Some(7),
            16.0,
            10.0
        ));
    }

    #[test]
    fn reuse_overlay_geometry_adds_node_id_label_when_requested() {
        let snapshot = BoxModelSnapshot {
            node_id: 42,
            parent_id: None,
            x: 10.0,
            y: 12.0,
            width: 50.0,
            height: 20.0,
            border_radius: 0.0,
            should_render: true,
        };

        let (plain_vertices, plain_indices) =
            build_reuse_overlay_geometry(&snapshot, 1.0, 200.0, 200.0, [1.0, 0.0, 0.0, 1.0], None);
        let (label_vertices, label_indices) = build_reuse_overlay_geometry(
            &snapshot,
            1.0,
            200.0,
            200.0,
            [1.0, 0.0, 0.0, 1.0],
            Some("42"),
        );

        assert!(label_vertices.len() > plain_vertices.len());
        assert!(label_indices.len() > plain_indices.len());
    }

    #[test]
    fn overlay_label_geometry_generates_background_and_digits() {
        let snapshot = BoxModelSnapshot {
            node_id: 7,
            parent_id: None,
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
            border_radius: 0.0,
            should_render: true,
        };
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        append_overlay_label_geometry(
            &mut vertices,
            &mut indices,
            &snapshot,
            "7",
            [0.0, 1.0, 0.0, 1.0],
            1.0,
            100.0,
            100.0,
        );

        assert!(!vertices.is_empty());
        assert!(!indices.is_empty());
    }

    #[test]
    fn reuse_overlay_geometry_scales_snapshot_coordinates_for_hidpi() {
        let snapshot = BoxModelSnapshot {
            node_id: 42,
            parent_id: None,
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0,
            border_radius: 0.0,
            should_render: true,
        };

        let (vertices, indices) =
            build_reuse_overlay_geometry(&snapshot, 2.0, 200.0, 200.0, [1.0, 0.0, 0.0, 1.0], None);

        assert!(!vertices.is_empty());
        assert!(!indices.is_empty());

        let expected_left = -0.8;
        let expected_top = 0.6;
        let min_x = vertices
            .iter()
            .map(|vertex| vertex.position[0])
            .fold(f32::INFINITY, f32::min);
        let max_y = vertices
            .iter()
            .map(|vertex| vertex.position[1])
            .fold(f32::NEG_INFINITY, f32::max);

        assert!((min_x - expected_left).abs() < 0.05);
        assert!((max_y - expected_top).abs() < 0.05);
    }

    #[test]
    fn wheel_uses_only_topmost_hit_target_ancestry() {
        let mut background = Element::new(0.0, 0.0, 100.0, 100.0);
        let background_id = background.id();
        let mut background_style = Style::new();
        background_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        background.apply_style(background_style);
        background.add_child(Box::new(Element::new(0.0, 0.0, 100.0, 300.0)));
        place_root(&mut background, 100.0, 100.0);

        let mut foreground = Element::new(0.0, 0.0, 100.0, 100.0);
        foreground.add_child(Box::new(Element::new(0.0, 0.0, 100.0, 100.0)));
        place_root(&mut foreground, 100.0, 100.0);

        let mut viewport = super::Viewport::new();
        viewport.ui_roots.push(Box::new(background));
        viewport.ui_roots.push(Box::new(foreground));
        viewport.set_mouse_position_viewport(50.0, 50.0);

        assert_eq!(
            super::Viewport::find_scroll_handler_at_pointer(
                &viewport.ui_roots,
                50.0,
                50.0,
                0.0,
                24.0
            ),
            None
        );
        assert!(!viewport.dispatch_mouse_wheel_event(0.0, 24.0));
        assert_eq!(
            get_scroll_offset_by_id(viewport.ui_roots[0].as_ref(), background_id),
            Some((0.0, 0.0))
        );
    }

    #[test]
    fn wheel_bubbles_to_ancestor_when_child_is_at_scroll_limit() {
        let mut root = Element::new(0.0, 0.0, 100.0, 100.0);
        let root_id = root.id();
        let mut root_style = Style::new();
        root_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        root.apply_style(root_style);

        let mut child = Element::new(0.0, 0.0, 100.0, 300.0);
        let child_id = child.id();
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        child.apply_style(child_style);
        child.add_child(Box::new(Element::new(0.0, 0.0, 100.0, 500.0)));

        root.add_child(Box::new(child));
        place_root(&mut root, 100.0, 100.0);

        assert_eq!(
            set_scroll_offset_by_id(&mut root, child_id, (0.0, 200.0)),
            true
        );
        assert_eq!(get_scroll_offset_by_id(&root, child_id), Some((0.0, 200.0)));
        assert_eq!(get_scroll_offset_by_id(&root, root_id), Some((0.0, 0.0)));

        let mut viewport = super::Viewport::new();
        viewport.ui_roots.push(Box::new(root));
        viewport.set_mouse_position_viewport(50.0, 50.0);

        assert_eq!(
            super::Viewport::find_scroll_handler_at_pointer(
                &viewport.ui_roots,
                50.0,
                50.0,
                0.0,
                24.0
            ),
            Some((0, root_id))
        );
        assert!(viewport.dispatch_mouse_wheel_event(0.0, 24.0));
        assert_eq!(
            get_scroll_offset_by_id(viewport.ui_roots[0].as_ref(), child_id),
            Some((0.0, 200.0))
        );
        assert_eq!(
            get_scroll_offset_by_id(viewport.ui_roots[0].as_ref(), root_id),
            Some((0.0, 0.0))
        );
    }

    #[test]
    fn hover_transform_transition_updates_live_element_in_viewport_flow() {
        let mut root = Element::new(0.0, 0.0, 240.0, 240.0);
        let mut child = Element::new(24.0, 24.0, 120.0, 80.0);
        let child_id = child.id();

        let mut style = Style::new();
        style.set_transform(Transform::default());
        style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transitions::from(vec![Transition::new(
                TransitionProperty::Transform,
                1000,
            )])),
        );
        let mut hover = Style::new();
        hover.set_transform(Transform::new([Translate::x(Length::px(40.0))]));
        style.set_hover(hover);
        child.apply_style(style);
        root.add_child(Box::new(child));
        place_root(&mut root, 240.0, 240.0);

        let mut viewport = super::Viewport::new();
        viewport.ui_roots.push(Box::new(root));

        let hover_changed = super::Viewport::sync_hover_visual_only(
            &mut viewport.ui_roots,
            &mut viewport.input_state.hovered_node_id,
            Some(child_id),
        );
        assert!(hover_changed);

        let mut roots = std::mem::take(&mut viewport.ui_roots);
        let result = viewport.run_post_layout_transitions(&mut roots, 0.5, 0.5);
        assert!(result.redraw_changed);

        let child = element_by_id_mut(roots[0].as_mut(), child_id).expect("child should exist");
        assert_ne!(child.debug_transform(), &Transform::default());
        assert_ne!(
            child.debug_transform(),
            &Transform::new([Translate::x(Length::px(40.0))])
        );
        viewport.ui_roots = roots;
    }

    fn redraw_only_transform_root(toggle: &Binding<bool>) -> RsxNode {
        let translated = toggle.get();
        crate::ui::rsx! {
            <HostElement style={{
                width: Length::px(120.0),
                height: Length::px(80.0),
                transform: if translated {
                    Transform::new([Translate::x(Length::px(48.0))])
                } else {
                    Transform::default()
                },
            }} />
        }
    }

    #[test]
    fn redraw_only_transform_sync_updates_live_tree_without_rebuild() {
        let toggle = Binding::new_with_dirty_state(false, UiDirtyState::REDRAW);
        let first = redraw_only_transform_root(&toggle);
        let second = {
            toggle.set(true);
            redraw_only_transform_root(&toggle)
        };

        let mut viewport = super::Viewport::new();
        viewport
            .render_rsx(&first)
            .expect("initial render should succeed");
        let original_id = viewport.ui_roots[0].id();

        viewport
            .render_rsx(&second)
            .expect("redraw-only transform render should succeed");

        assert_eq!(viewport.ui_roots[0].id(), original_id);
        let element = element_by_id_mut(viewport.ui_roots[0].as_mut(), original_id)
            .expect("root element should remain live");
        assert_eq!(
            element.debug_transform(),
            &Transform::new([Translate::x(Length::px(48.0))])
        );
    }

    #[test]
    fn viewport_registers_box_shadow_transition_channel() {
        let viewport = super::Viewport::new();
        assert!(
            viewport
                .transition_channels
                .contains(&CHANNEL_STYLE_BOX_SHADOW)
        );
    }
}
