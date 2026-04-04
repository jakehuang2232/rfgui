#![allow(missing_docs)]

use super::{ElementCore, Position, Size};
use crate::ColorLike;
use crate::render_pass::draw_rect_pass::{DrawRectOutput, RectPassParams};
use crate::style::{
    Align, AnchorName, BoxShadow, ClipMode, Collision, CollisionBoundary, Color, ComputedStyle,
    CrossSize, Cursor, FlowDirection, FlowWrap, JustifyContent, Layout, Length, PositionMode,
    ScrollDirection, SizeValue, Style, Transform, TransformKind, TransformOrigin,
    TransitionProperty, TransitionTiming, compute_style, interpolate_transform_with_reference_box,
};
use crate::transition::{
    AnimationRequest, CHANNEL_LAYOUT_HEIGHT, CHANNEL_LAYOUT_WIDTH, CHANNEL_STYLE_BACKGROUND_COLOR,
    CHANNEL_STYLE_BORDER_BOTTOM_COLOR, CHANNEL_STYLE_BORDER_LEFT_COLOR,
    CHANNEL_STYLE_BORDER_RADIUS, CHANNEL_STYLE_BORDER_RIGHT_COLOR, CHANNEL_STYLE_BORDER_TOP_COLOR,
    CHANNEL_STYLE_BOX_SHADOW, CHANNEL_STYLE_COLOR, CHANNEL_STYLE_OPACITY, CHANNEL_STYLE_TRANSFORM,
    CHANNEL_STYLE_TRANSFORM_ORIGIN, CHANNEL_VISUAL_X, CHANNEL_VISUAL_Y, ChannelId, LayoutField,
    LayoutTrackRequest, LayoutTransition as RuntimeLayoutTransition, ScrollAxis, StyleField,
    StyleTrackRequest, StyleTransition as RuntimeStyleTransition, StyleValue, TimeFunction,
    VisualField, VisualTrackRequest, VisualTransition as RuntimeVisualTransition,
};
use crate::ui::{
    BlurEvent, ClickEvent, FocusEvent, KeyDownEvent, KeyUpEvent, MouseButton as UiMouseButton,
    MouseDownEvent, MouseEnterEvent, MouseLeaveEvent, MouseMoveEvent, MouseUpEvent,
};
use crate::view::frame_graph::texture_resource::TextureHandle;
use crate::view::frame_graph::{AttachmentTarget, FrameGraph, ResourceLifetime, TextureDesc};
use crate::view::promotion::{PromotedLayerUpdateKind, PromotionNodeInfo};
use crate::view::render_pass::draw_rect_pass::DrawRectInput;
use crate::view::render_pass::draw_rect_pass::{RenderTargetIn, RenderTargetOut, RenderTargetTag};
use crate::view::render_pass::render_target::GraphicsPassContext;
use crate::view::render_pass::{
    DrawRectPass, GraphicsPass, OpaqueRectPass, RectRenderMode, ShadowMesh, ShadowModuleSpec,
    ShadowParams, build_shadow_module,
};
use crate::view::viewport::ViewportControl;
use glam::{Mat4, Vec3, Vec4};
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};
include!("event_target.rs");
include!("layout_trait.rs");
include!("render_trait.rs");
include!("impl_core.rs");
include!("impl_scroll.rs");
include!("impl_render.rs");
include!("impl_layout.rs");
include!("helpers.rs");
include!("tests.rs");

use crate::time::{Duration, Instant};

trait DrawRectIoPass {
    fn draw_rect_input_mut(&mut self) -> &mut DrawRectInput;
    fn draw_rect_output_mut(&mut self) -> &mut DrawRectOutput;
    fn set_scissor_rect(&mut self, scissor_rect: Option<[u32; 4]>);
}

impl DrawRectIoPass for DrawRectPass {
    fn draw_rect_input_mut(&mut self) -> &mut DrawRectInput {
        DrawRectPass::draw_rect_input_mut(self)
    }

    fn draw_rect_output_mut(&mut self) -> &mut DrawRectOutput {
        DrawRectPass::draw_rect_output_mut(self)
    }

    fn set_scissor_rect(&mut self, scissor_rect: Option<[u32; 4]>) {
        DrawRectPass::set_scissor_rect(self, scissor_rect);
    }
}

impl DrawRectIoPass for OpaqueRectPass {
    fn draw_rect_input_mut(&mut self) -> &mut DrawRectInput {
        OpaqueRectPass::draw_rect_input_mut(self)
    }

    fn draw_rect_output_mut(&mut self) -> &mut DrawRectOutput {
        OpaqueRectPass::draw_rect_output_mut(self)
    }

    fn set_scissor_rect(&mut self, scissor_rect: Option<[u32; 4]>) {
        OpaqueRectPass::set_scissor_rect(self, scissor_rect);
    }
}

#[derive(Clone, Copy, Debug)]
struct EdgeInsets {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LayoutProposal {
    width: f32,
    height: f32,
    viewport_width: f32,
    viewport_height: f32,
    percent_base_width: Option<f32>,
    percent_base_height: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
struct LayoutFrame {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Clone, Copy, Debug)]
struct CornerRadii {
    top_left: f32,
    top_right: f32,
    bottom_right: f32,
    bottom_left: f32,
}

impl CornerRadii {
    const fn uniform(value: f32) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_right: value,
            bottom_left: value,
        }
    }

    const fn zero() -> Self {
        Self::uniform(0.0)
    }

    fn to_array(self) -> [f32; 4] {
        [
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        ]
    }

    fn has_any_rounding(self) -> bool {
        self.top_left > 0.0
            || self.top_right > 0.0
            || self.bottom_right > 0.0
            || self.bottom_left > 0.0
    }

    fn max(self) -> f32 {
        self.top_left
            .max(self.top_right)
            .max(self.bottom_right)
            .max(self.bottom_left)
    }
}

#[derive(Clone)]
struct EdgeColors {
    left: Box<dyn ColorLike>,
    right: Box<dyn ColorLike>,
    top: Box<dyn ColorLike>,
    bottom: Box<dyn ColorLike>,
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Rect {
    fn contains(self, px: f32, py: f32) -> bool {
        px >= self.x && py >= self.y && px <= self.x + self.width && py <= self.y + self.height
    }

    fn intersects(self, other: Rect) -> bool {
        self.width > 0.0
            && self.height > 0.0
            && self.x + self.width > other.x
            && self.x < other.x + other.width
            && self.y + self.height > other.y
            && self.y < other.y + other.height
    }
}

fn intersect_rect(a: Rect, b: Rect) -> Rect {
    let left = a.x.max(b.x);
    let top = a.y.max(b.y);
    let right = (a.x + a.width).min(b.x + b.width);
    let bottom = (a.y + a.height).min(b.y + b.height);
    Rect {
        x: left,
        y: top,
        width: (right - left).max(0.0),
        height: (bottom - top).max(0.0),
    }
}

fn css_perspective_matrix(depth: f32) -> Mat4 {
    if depth.abs() <= 0.000_001 {
        return Mat4::IDENTITY;
    }
    Mat4::from_cols(
        Vec4::new(1.0, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 1.0, 0.0, 0.0),
        Vec4::new(0.0, 0.0, 1.0, -1.0 / depth),
        Vec4::new(0.0, 0.0, 0.0, 1.0),
    )
}

fn hash_f32<H: Hasher>(state: &mut H, value: f32) {
    value.to_bits().hash(state);
}

pub(crate) fn promoted_layer_stable_key(node_id: u64) -> u64 {
    0xC0DE_0000_0000_0000u64 | node_id
}

pub(crate) fn promoted_clip_mask_stable_key(node_id: u64) -> u64 {
    0xC11E_0000_0000_0000u64 | node_id
}

pub(crate) fn promoted_final_layer_stable_key(node_id: u64) -> u64 {
    0xC0DE_F1A1_0000_0000u64 | node_id
}

pub(crate) fn transformed_layer_stable_key(node_id: u64) -> u64 {
    0x7F00_0000_0000_0000u64 | node_id
}

#[derive(Clone, Copy, Debug)]
struct AnchorSnapshot {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    parent_clip_rect: Rect,
}

#[derive(Default)]
struct PlacementRuntime {
    depth: usize,
    viewport_width: f32,
    viewport_height: f32,
    anchors: std::collections::HashMap<String, AnchorSnapshot>,
    child_clip_stack: Vec<Rect>,
    hit_test_clip_stack: Vec<Rect>,
}

thread_local! {
    static PLACEMENT_RUNTIME: RefCell<PlacementRuntime> = RefCell::new(PlacementRuntime::default());
}

#[derive(Clone, Debug, Default)]
pub(crate) struct LayoutPlaceProfile {
    pub node_count: usize,
    pub place_self_ms: f64,
    pub place_children_ms: f64,
    pub place_flex_children_ms: f64,
    pub place_layout_inline_ms: f64,
    pub place_layout_flex_ms: f64,
    pub place_layout_flow_ms: f64,
    pub non_axis_child_place_ms: f64,
    pub absolute_child_place_ms: f64,
    pub child_place_calls: usize,
    pub absolute_child_place_calls: usize,
    pub update_content_size_ms: f64,
    pub clamp_scroll_ms: f64,
    pub recompute_hit_test_ms: f64,
}

thread_local! {
    static LAYOUT_PLACE_PROFILE: RefCell<LayoutPlaceProfile> =
        RefCell::new(LayoutPlaceProfile::default());
}

pub(crate) fn reset_layout_place_profile() {
    LAYOUT_PLACE_PROFILE.with(|profile| {
        *profile.borrow_mut() = LayoutPlaceProfile::default();
    });
}

pub(crate) fn take_layout_place_profile() -> LayoutPlaceProfile {
    LAYOUT_PLACE_PROFILE.with(|profile| std::mem::take(&mut *profile.borrow_mut()))
}

#[derive(Clone, Copy, Debug)]
enum ScrollbarAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug)]
struct ScrollbarDragState {
    axis: ScrollbarAxis,
    grab_offset: f32,
    reanchor_on_first_move: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct ScrollbarGeometry {
    vertical_track: Option<Rect>,
    vertical_thumb: Option<Rect>,
    horizontal_track: Option<Rect>,
    horizontal_thumb: Option<Rect>,
}

#[derive(Clone, Copy, Debug)]
struct ChildClipScope {
    previous_scissor: Option<[u32; 4]>,
    parent_clip_id: u8,
    child_clip_id: u8,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AncestorClipContext {
    scissor_rect: Option<[u32; 4]>,
}

/// A snapshot of the viewport configuration exposed to low-level build code.
#[derive(Clone)]
pub struct ViewportContext {
    color_target: Option<TextureHandle>,
    depth_stencil_target: Option<AttachmentTarget>,
    target_width: u32,
    target_height: u32,
    target_format: wgpu::TextureFormat,
    scale_factor: f32,
    render_transform: Option<Mat4>,
    promoted_node_ids: Arc<std::collections::HashSet<u64>>,
    promoted_update_kinds: Arc<HashMap<u64, PromotedLayerUpdateKind>>,
    promoted_composition_update_kinds: Arc<HashMap<u64, PromotedLayerUpdateKind>>,
}

/// Mutable build state threaded through low-level render graph construction.
#[derive(Clone)]
pub struct BuildState {
    target: Option<RenderTargetOut>,
    depth_stencil_target: Option<AttachmentTarget>,
    target_pairs: HashMap<u32, AttachmentTarget>,
    scissor_rect: Option<[u32; 4]>,
    clip_id_stack: Vec<u8>,
    deferred_node_ids: Vec<u64>,
    dfs_opaque_rect_order: u32,
}

impl BuildState {
    pub(crate) fn current_target(&self) -> Option<RenderTargetOut> {
        self.target
    }

    pub(crate) fn for_layer_subtree_with_ancestor_clip(ancestor_clip: AncestorClipContext) -> Self {
        Self {
            target: None,
            depth_stencil_target: None,
            target_pairs: HashMap::new(),
            scissor_rect: ancestor_clip.scissor_rect,
            clip_id_stack: Vec::new(),
            deferred_node_ids: Vec::new(),
            dfs_opaque_rect_order: 0,
        }
    }

    pub(crate) fn merge_child_side_effects(&mut self, child: &BuildState) {
        self.dfs_opaque_rect_order = self.dfs_opaque_rect_order.max(child.dfs_opaque_rect_order);
        for (&color_handle, &depth_target) in &child.target_pairs {
            self.target_pairs.insert(color_handle, depth_target);
        }
        for node_id in &child.deferred_node_ids {
            if !self.deferred_node_ids.contains(node_id) {
                self.deferred_node_ids.push(*node_id);
            }
        }
    }
}

pub struct UiBuildContext {
    viewport: ViewportContext,
    state: BuildState,
}

fn texture_desc_for_logical_bounds(
    bounds: PromotionCompositeBounds,
    scale_factor: f32,
    _render_transform: Option<Mat4>,
    target_format: wgpu::TextureFormat,
) -> TextureDesc {
    let scale = scale_factor.max(0.0001);
    let origin_x = (bounds.x * scale).floor().max(0.0) as u32;
    let origin_y = (bounds.y * scale).floor().max(0.0) as u32;
    let max_x = ((bounds.x + bounds.width.max(0.0)) * scale).ceil().max(0.0) as u32;
    let max_y = ((bounds.y + bounds.height.max(0.0)) * scale)
        .ceil()
        .max(0.0) as u32;
    let width = max_x.saturating_sub(origin_x).max(1);
    let height = max_y.saturating_sub(origin_y).max(1);
    TextureDesc::new(width, height, target_format, wgpu::TextureDimension::D2)
        .with_origin(origin_x, origin_y)
}

fn promoted_layer_label(node_id: u64) -> String {
    format!("Promoted Layer [{node_id}]")
}

fn promoted_clip_mask_label(node_id: u64) -> String {
    format!("Promoted Clip Mask [{node_id}]")
}

fn promoted_final_layer_label(node_id: u64) -> String {
    format!("Promoted Final Layer [{node_id}]")
}

fn label_for_persistent_target(stable_key: u64) -> String {
    let node_id = stable_key & 0x0000_FFFF_FFFF_FFFF;
    if stable_key & 0xFFFF_0000_0000_0000 == 0xC11E_0000_0000_0000 {
        promoted_clip_mask_label(node_id)
    } else if stable_key & 0xFFFF_FFFF_0000_0000 == 0xC0DE_F1A1_0000_0000 {
        promoted_final_layer_label(node_id)
    } else if stable_key & 0xFFFF_0000_0000_0000 == 0xC0DE_0000_0000_0000 {
        promoted_layer_label(node_id)
    } else {
        format!("Persistent Render Target [{stable_key:#x}]")
    }
}

fn persistent_depth_stencil_stable_key(stable_key: u64) -> u64 {
    stable_key ^ 0xD3A7_0000_0000_0000
}

impl UiBuildContext {
    pub fn new(
        viewport_width: u32,
        viewport_height: u32,
        viewport_format: wgpu::TextureFormat,
        scale_factor: f32,
    ) -> Self {
        Self {
            viewport: ViewportContext {
                color_target: None,
                depth_stencil_target: Some(AttachmentTarget::Surface),
                target_width: viewport_width.max(1),
                target_height: viewport_height.max(1),
                target_format: viewport_format,
                scale_factor: scale_factor.max(0.0001),
                render_transform: None,
                promoted_node_ids: Arc::new(std::collections::HashSet::new()),
                promoted_update_kinds: Arc::new(HashMap::new()),
                promoted_composition_update_kinds: Arc::new(HashMap::new()),
            },
            state: BuildState {
                target: None,
                depth_stencil_target: Some(AttachmentTarget::Surface),
                target_pairs: HashMap::new(),
                scissor_rect: None,
                clip_id_stack: Vec::new(),
                deferred_node_ids: Vec::new(),
                dfs_opaque_rect_order: 0,
            },
        }
    }

    pub fn from_parts(viewport: ViewportContext, state: BuildState) -> Self {
        Self { viewport, state }
    }

    pub fn viewport(&self) -> ViewportContext {
        self.viewport.clone()
    }

    pub fn set_state(&mut self, state: BuildState) {
        self.state = state;
    }

    pub fn state_clone(&self) -> BuildState {
        self.state.clone()
    }

    pub fn into_state(self) -> BuildState {
        self.state
    }

    pub fn allocate_target(&mut self, graph: &mut FrameGraph) -> RenderTargetOut {
        self.next_target(graph)
    }

    pub(crate) fn allocate_promoted_layer_target(
        &mut self,
        graph: &mut FrameGraph,
        node_id: u64,
        bounds: PromotionCompositeBounds,
    ) -> RenderTargetOut {
        self.next_persistent_target_with_desc(
            graph,
            texture_desc_for_logical_bounds(
                bounds,
                self.viewport.scale_factor,
                self.viewport.render_transform,
                self.viewport.target_format,
            )
            .with_label(promoted_layer_label(node_id)),
            promoted_layer_stable_key(node_id),
        )
    }

    pub(crate) fn allocate_persistent_target_with_key(
        &mut self,
        graph: &mut FrameGraph,
        stable_key: u64,
        bounds: PromotionCompositeBounds,
    ) -> RenderTargetOut {
        self.next_persistent_target_with_desc(
            graph,
            texture_desc_for_logical_bounds(
                bounds,
                self.viewport.scale_factor,
                self.viewport.render_transform,
                self.viewport.target_format,
            ),
            stable_key,
        )
    }

    pub fn set_current_target(&mut self, target: RenderTargetOut) {
        self.state.depth_stencil_target = match target.handle() {
            Some(handle) => self.state.target_pairs.get(&handle.0).copied(),
            None => Some(AttachmentTarget::Surface),
        };
        self.state.target = Some(target);
    }

    pub(crate) fn current_target(&self) -> Option<RenderTargetOut> {
        self.state.target
    }

    fn next_target(&mut self, graph: &mut FrameGraph) -> RenderTargetOut {
        self.next_target_with_desc(
            graph,
            TextureDesc::new(
                self.viewport.target_width,
                self.viewport.target_height,
                self.viewport.target_format,
                wgpu::TextureDimension::D2,
            )
            .with_label("Offscreen Render Target"),
        )
    }

    fn next_target_with_desc(
        &mut self,
        graph: &mut FrameGraph,
        desc: TextureDesc,
    ) -> RenderTargetOut {
        let depth_label = desc
            .label()
            .map(|label| format!("{label} / Depth-Stencil"))
            .unwrap_or_else(|| "Offscreen Depth-Stencil".to_string());
        let color = graph.declare_texture::<RenderTargetTag>(desc.clone());
        let depth_stencil = graph.declare_texture::<RenderTargetTag>(
            TextureDesc::new(
                desc.width(),
                desc.height(),
                wgpu::TextureFormat::Depth24PlusStencil8,
                wgpu::TextureDimension::D2,
            )
            .with_usage(wgpu::TextureUsages::RENDER_ATTACHMENT)
            .with_sample_count(desc.sample_count())
            .with_label(depth_label),
        );
        if let (Some(color_handle), Some(depth_handle)) = (color.handle(), depth_stencil.handle()) {
            self.state
                .target_pairs
                .insert(color_handle.0, AttachmentTarget::Texture(depth_handle));
            graph.pair_texture_attachment(color_handle, AttachmentTarget::Texture(depth_handle));
        }
        color
    }

    fn next_persistent_target_with_desc(
        &mut self,
        graph: &mut FrameGraph,
        mut desc: TextureDesc,
        stable_key: u64,
    ) -> RenderTargetOut {
        if desc.label().is_none() {
            desc = desc.with_label(label_for_persistent_target(stable_key));
        }
        let depth_label = desc
            .label()
            .map(|label| format!("{label} / Depth-Stencil"))
            .unwrap_or_else(|| "Persistent Depth-Stencil".to_string());
        let color = graph.declare_texture_internal::<RenderTargetTag>(
            desc.clone(),
            ResourceLifetime::Persistent,
            Some(stable_key),
        );
        let depth_stencil = graph.declare_texture_internal::<RenderTargetTag>(
            TextureDesc::new(
                desc.width(),
                desc.height(),
                wgpu::TextureFormat::Depth24PlusStencil8,
                wgpu::TextureDimension::D2,
            )
            .with_usage(wgpu::TextureUsages::RENDER_ATTACHMENT)
            .with_sample_count(desc.sample_count())
            .with_label(depth_label),
            ResourceLifetime::Persistent,
            Some(persistent_depth_stencil_stable_key(stable_key)),
        );
        if let (Some(color_handle), Some(depth_handle)) = (color.handle(), depth_stencil.handle()) {
            self.state
                .target_pairs
                .insert(color_handle.0, AttachmentTarget::Texture(depth_handle));
            graph.pair_texture_attachment(color_handle, AttachmentTarget::Texture(depth_handle));
        }
        color
    }

    pub(crate) fn depth_stencil_target(&self) -> Option<AttachmentTarget> {
        if self.state.target.is_some() {
            self.state.depth_stencil_target
        } else {
            self.state
                .depth_stencil_target
                .or(self.viewport.depth_stencil_target)
        }
    }

    pub(crate) fn set_color_target(&mut self, color_target: Option<TextureHandle>) {
        self.viewport.color_target = color_target;
    }

    pub(crate) fn current_render_transform(&self) -> Option<Mat4> {
        self.viewport.render_transform
    }

    pub(crate) fn set_current_render_transform(&mut self, render_transform: Option<Mat4>) {
        self.viewport.render_transform = render_transform;
    }

    fn scissor_rect(&self) -> Option<[u32; 4]> {
        self.state.scissor_rect
    }

    pub(crate) fn ancestor_clip_context(&self) -> AncestorClipContext {
        AncestorClipContext {
            scissor_rect: self.scissor_rect(),
        }
    }

    fn current_clip_id(&self) -> u8 {
        self.state.clip_id_stack.last().copied().unwrap_or(0)
    }

    fn active_clip_id(&self) -> Option<u8> {
        self.state.clip_id_stack.last().copied()
    }

    pub(crate) fn push_clip_id(&mut self) -> Option<u8> {
        let current = self.current_clip_id();
        if current == u8::MAX {
            return None;
        }
        let next = current.saturating_add(1);
        self.state.clip_id_stack.push(next);
        Some(next)
    }

    pub(crate) fn pop_clip_id(&mut self) {
        let _ = self.state.clip_id_stack.pop();
    }

    pub(crate) fn push_scissor_rect(&mut self, scissor_rect: Option<[u32; 4]>) -> Option<[u32; 4]> {
        let previous = self.state.scissor_rect;
        self.state.scissor_rect = intersect_scissor_rects(self.state.scissor_rect, scissor_rect);
        previous
    }

    pub(crate) fn restore_scissor_rect(&mut self, previous: Option<[u32; 4]>) {
        self.state.scissor_rect = previous;
    }

    pub(crate) fn append_to_defer(&mut self, node_id: u64) {
        if !self.state.deferred_node_ids.contains(&node_id) {
            self.state.deferred_node_ids.push(node_id);
        }
    }

    pub(crate) fn take_deferred_node_ids(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.state.deferred_node_ids)
    }

    pub(crate) fn next_opaque_rect_order(&mut self) -> u32 {
        let order = self.state.dfs_opaque_rect_order;
        self.state.dfs_opaque_rect_order = self.state.dfs_opaque_rect_order.saturating_add(1);
        order
    }

    pub(crate) fn graphics_pass_context(&self) -> GraphicsPassContext {
        GraphicsPassContext {
            scissor_rect: self.scissor_rect(),
            stencil_clip_id: self.active_clip_id(),
            uses_depth_stencil: self.depth_stencil_target().is_some(),
        }
    }

    pub(crate) fn set_promoted_runtime(
        &mut self,
        promoted_node_ids: Arc<std::collections::HashSet<u64>>,
        promoted_update_kinds: Arc<HashMap<u64, PromotedLayerUpdateKind>>,
        promoted_composition_update_kinds: Arc<HashMap<u64, PromotedLayerUpdateKind>>,
    ) {
        self.viewport.promoted_node_ids = promoted_node_ids;
        self.viewport.promoted_update_kinds = promoted_update_kinds;
        self.viewport.promoted_composition_update_kinds = promoted_composition_update_kinds;
    }

    pub(crate) fn is_node_promoted(&self, node_id: u64) -> bool {
        self.viewport.promoted_node_ids.contains(&node_id)
    }

    pub(crate) fn promoted_update_kind(&self, node_id: u64) -> Option<PromotedLayerUpdateKind> {
        self.viewport.promoted_update_kinds.get(&node_id).copied()
    }

    pub(crate) fn promoted_composition_update_kind(
        &self,
        node_id: u64,
    ) -> Option<PromotedLayerUpdateKind> {
        self.viewport
            .promoted_composition_update_kinds
            .get(&node_id)
            .copied()
    }

    pub(crate) fn merge_child_state_side_effects(&mut self, child: &BuildState) {
        self.state.merge_child_side_effects(child);
    }
}

impl ViewportContext {
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }
}

/// Layout context resolved from constraints or placement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutContext {
    pub width: f32,
    pub height: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub percent_base_width: Option<f32>,
    pub percent_base_height: Option<f32>,
}

/// Constraints passed to [`Layoutable::measure`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutConstraints {
    pub max_width: f32,
    pub max_height: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub percent_base_width: Option<f32>,
    pub percent_base_height: Option<f32>,
}

/// Placement information passed to [`Layoutable::place`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutPlacement {
    pub parent_x: f32,
    pub parent_y: f32,
    pub visual_offset_x: f32,
    pub visual_offset_y: f32,
    pub available_width: f32,
    pub available_height: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub percent_base_width: Option<f32>,
    pub percent_base_height: Option<f32>,
}

/// Dirty bits used to decide which retained runtime work must be refreshed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyFlags(u8);

impl DirtyFlags {
    pub const NONE: Self = Self(0);
    pub const LAYOUT: Self = Self(1 << 0);
    pub const PLACE: Self = Self(1 << 1);
    pub const BOX_MODEL: Self = Self(1 << 2);
    pub const HIT_TEST: Self = Self(1 << 3);
    pub const PAINT: Self = Self(1 << 4);
    pub const RUNTIME: Self =
        Self(Self::PLACE.0 | Self::BOX_MODEL.0 | Self::HIT_TEST.0 | Self::PAINT.0);
    pub const ALL: Self =
        Self(Self::LAYOUT.0 | Self::PLACE.0 | Self::BOX_MODEL.0 | Self::HIT_TEST.0 | Self::PAINT.0);

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub const fn intersects(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

impl LayoutConstraints {
    fn context(self) -> LayoutContext {
        LayoutContext {
            width: self.max_width,
            height: self.max_height,
            viewport_width: self.viewport_width,
            viewport_height: self.viewport_height,
            percent_base_width: self.percent_base_width,
            percent_base_height: self.percent_base_height,
        }
    }
}

impl LayoutPlacement {
    fn context(self) -> LayoutContext {
        LayoutContext {
            width: self.available_width,
            height: self.available_height,
            viewport_width: self.viewport_width,
            viewport_height: self.viewport_height,
            percent_base_width: self.percent_base_width,
            percent_base_height: self.percent_base_height,
        }
    }
}

pub trait Layoutable {
    fn measure(&mut self, constraints: LayoutConstraints);
    fn place(&mut self, placement: LayoutPlacement);
    fn measured_size(&self) -> (f32, f32);
    fn set_layout_width(&mut self, width: f32);
    fn set_layout_height(&mut self, height: f32);
    fn allows_cross_stretch(&self, is_row: bool) -> bool;
    fn cross_alignment_size(&self, is_row: bool, stretched_cross: Option<f32>) -> f32 {
        let (measured_w, measured_h) = self.measured_size();
        stretched_cross.unwrap_or(if is_row { measured_h } else { measured_w })
    }
    fn flex_grow(&self) -> f32 {
        0.0
    }
    fn flex_shrink(&self) -> f32 {
        1.0
    }
    fn flex_basis(&self) -> SizeValue {
        SizeValue::Auto
    }
    fn flex_main_size(&self, _is_row: bool) -> SizeValue {
        SizeValue::Auto
    }
    fn flex_has_explicit_min_main_size(&self, _is_row: bool) -> bool {
        false
    }
    fn flex_auto_min_main_size(&self, _is_row: bool) -> Option<f32> {
        None
    }
    fn flex_auto_base_main_size(&self, _is_row: bool) -> Option<f32> {
        None
    }
    fn flex_min_main_size(&self, _is_row: bool) -> SizeValue {
        SizeValue::Length(Length::Px(0.0))
    }
    fn flex_max_main_size(&self, _is_row: bool) -> SizeValue {
        SizeValue::Auto
    }
    fn set_layout_offset(&mut self, _x: f32, _y: f32) {}
}

pub trait EventTarget {
    fn dispatch_mouse_down(
        &mut self,
        _event: &mut MouseDownEvent,
        _control: &mut ViewportControl<'_>,
    ) {
    }
    fn dispatch_mouse_up(&mut self, _event: &mut MouseUpEvent, _control: &mut ViewportControl<'_>) {
    }
    fn dispatch_mouse_move(
        &mut self,
        _event: &mut MouseMoveEvent,
        _control: &mut ViewportControl<'_>,
    ) {
    }
    fn dispatch_mouse_enter(&mut self, _event: &mut MouseEnterEvent) {}
    fn dispatch_mouse_leave(&mut self, _event: &mut MouseLeaveEvent) {}
    fn dispatch_click(&mut self, _event: &mut ClickEvent, _control: &mut ViewportControl<'_>) {}
    fn dispatch_key_down(&mut self, _event: &mut KeyDownEvent, _control: &mut ViewportControl<'_>) {
    }
    fn dispatch_key_up(&mut self, _event: &mut KeyUpEvent, _control: &mut ViewportControl<'_>) {}
    fn dispatch_text_input(
        &mut self,
        _event: &mut crate::ui::TextInputEvent,
        _control: &mut ViewportControl<'_>,
    ) {
    }
    fn dispatch_ime_preedit(
        &mut self,
        _event: &mut crate::ui::ImePreeditEvent,
        _control: &mut ViewportControl<'_>,
    ) {
    }
    fn dispatch_focus(&mut self, _event: &mut FocusEvent, _control: &mut ViewportControl<'_>) {}
    fn dispatch_blur(&mut self, _event: &mut BlurEvent, _control: &mut ViewportControl<'_>) {}
    fn cancel_pointer_interaction(&mut self) -> bool {
        false
    }
    fn set_hovered(&mut self, _hovered: bool) -> bool {
        false
    }
    fn scroll_by(&mut self, _dx: f32, _dy: f32) -> bool {
        false
    }
    fn can_scroll_by(&self, _dx: f32, _dy: f32) -> bool {
        false
    }
    fn get_scroll_offset(&self) -> (f32, f32) {
        (0.0, 0.0)
    }
    fn set_scroll_offset(&mut self, _offset: (f32, f32)) {}
    fn ime_cursor_rect(&self) -> Option<(f32, f32, f32, f32)> {
        None
    }
    fn cursor(&self) -> Cursor {
        Cursor::Default
    }
    fn wants_animation_frame(&self) -> bool {
        false
    }
    fn take_style_transition_requests(&mut self) -> Vec<StyleTrackRequest> {
        Vec::new()
    }
    fn take_animation_requests(&mut self) -> Vec<AnimationRequest> {
        Vec::new()
    }
    fn take_layout_transition_requests(&mut self) -> Vec<LayoutTrackRequest> {
        Vec::new()
    }
    fn take_visual_transition_requests(&mut self) -> Vec<VisualTrackRequest> {
        Vec::new()
    }
}

pub trait Renderable {
    fn build(&mut self, graph: &mut FrameGraph, ctx: UiBuildContext) -> BuildState;
}

pub trait ElementTrait: Layoutable + EventTarget + Renderable + std::any::Any {
    fn id(&self) -> u64;
    fn parent_id(&self) -> Option<u64>;
    fn set_parent_id(&mut self, parent_id: Option<u64>);
    fn box_model_snapshot(&self) -> BoxModelSnapshot;
    fn children(&self) -> Option<&[Box<dyn ElementTrait>]>;
    fn children_mut(&mut self) -> Option<&mut [Box<dyn ElementTrait>]>;

    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    fn snapshot_state(&self) -> Option<Box<dyn std::any::Any>> {
        None
    }

    fn restore_state(&mut self, _snapshot: &dyn std::any::Any) -> bool {
        false
    }

    fn intercepts_pointer_at(&self, _viewport_x: f32, _viewport_y: f32) -> bool {
        false
    }

    fn hit_test_visible_at(&self, _viewport_x: f32, _viewport_y: f32) -> bool {
        true
    }

    fn promotion_node_info(&self) -> PromotionNodeInfo {
        PromotionNodeInfo::default()
    }

    fn promotion_self_signature(&self) -> u64 {
        0
    }

    fn promotion_clip_intersection_signature(&self) -> u64 {
        0
    }

    fn promotion_composite_bounds(&self) -> PromotionCompositeBounds {
        let snapshot = self.box_model_snapshot();
        PromotionCompositeBounds {
            x: snapshot.x,
            y: snapshot.y,
            width: snapshot.width.max(0.0),
            height: snapshot.height.max(0.0),
            corner_radii: [0.0; 4],
        }
    }

    fn local_dirty_flags(&self) -> DirtyFlags {
        DirtyFlags::ALL
    }

    fn clear_local_dirty_flags(&mut self, _flags: DirtyFlags) {}
}

#[derive(Clone, Copy, Debug)]
pub struct BoxModelSnapshot {
    pub node_id: u64,
    pub parent_id: Option<u64>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub border_radius: f32,
    pub should_render: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct PromotionCompositeBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub corner_radii: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DebugElementRenderState {
    pub background_rgba: [u8; 4],
    pub foreground_rgba: [u8; 4],
    pub opacity: f32,
    pub border_radius: f32,
}

type MouseDownHandler = Box<dyn FnMut(&mut MouseDownEvent, &mut ViewportControl<'_>)>;
type MouseUpHandler = Box<dyn FnMut(&mut MouseUpEvent, &mut ViewportControl<'_>)>;
type MouseMoveHandler = Box<dyn FnMut(&mut MouseMoveEvent, &mut ViewportControl<'_>)>;
type MouseEnterHandler = Box<dyn FnMut(&mut MouseEnterEvent)>;
type MouseLeaveHandler = Box<dyn FnMut(&mut MouseLeaveEvent)>;
type ClickHandler = Box<dyn FnMut(&mut ClickEvent, &mut ViewportControl<'_>)>;
type KeyDownHandler = Box<dyn FnMut(&mut KeyDownEvent, &mut ViewportControl<'_>)>;
type KeyUpHandler = Box<dyn FnMut(&mut KeyUpEvent, &mut ViewportControl<'_>)>;
type FocusHandler = Box<dyn FnMut(&mut FocusEvent, &mut ViewportControl<'_>)>;
type BlurHandler = Box<dyn FnMut(&mut BlurEvent, &mut ViewportControl<'_>)>;

#[derive(Clone, Debug)]
struct FlexLayoutInfo {
    lines: Vec<Vec<usize>>,
    line_main_sum: Vec<f32>,
    line_cross_max: Vec<f32>,
    total_main: f32,
    total_cross: f32,
    child_sizes: Vec<(f32, f32)>,
}

#[derive(Clone, Copy, Debug)]
struct FlexItemPlan {
    index: usize,
    flex_base_main: f32,
    hypothetical_main: f32,
    used_main: f32,
    min_main: f32,
    max_main: Option<f32>,
    frozen: bool,
    cross: f32,
}

#[derive(Clone, Debug)]
struct ElementStyleSnapshot {
    opacity: f32,
    border_radius: f32,
    width: f32,
    height: f32,
    layout_width: f32,
    layout_height: f32,
    is_hovered: bool,
    background_color: Color,
    foreground_color: Color,
    border_top_color: Color,
    border_right_color: Color,
    border_bottom_color: Color,
    border_left_color: Color,
    box_shadows: Vec<BoxShadow>,
    transform: Transform,
    transform_origin: TransformOrigin,
    transition_snapshot: Option<TransitionSnapshot>,
}

#[derive(Clone, Copy, Debug)]
struct TransitionSnapshot {
    has_layout_snapshot: bool,
    layout_transition_visual_offset_x: f32,
    layout_transition_visual_offset_y: f32,
    layout_transition_override_width: Option<f32>,
    layout_transition_override_height: Option<f32>,
    layout_transition_target_x: Option<f32>,
    layout_transition_target_y: Option<f32>,
    layout_transition_target_width: Option<f32>,
    layout_transition_target_height: Option<f32>,
    last_parent_layout_x: f32,
    last_parent_layout_y: f32,
    layout_assigned_width: Option<f32>,
    layout_assigned_height: Option<f32>,
}

pub struct Element {
    core: ElementCore,
    anchor_name: Option<AnchorName>,
    layout_flow_position: Position,
    layout_inner_position: Position,
    layout_flow_inner_position: Position,
    layout_inner_size: Size,
    intrinsic_size_is_percent_base: bool,
    parsed_style: Style,
    computed_style: ComputedStyle,
    padding: EdgeInsets,
    background_color: Box<dyn ColorLike>,
    border_colors: EdgeColors,
    border_widths: EdgeInsets,
    border_radii: CornerRadii,
    border_radius: f32,
    box_shadows: Vec<BoxShadow>,
    transform: Transform,
    transform_origin: TransformOrigin,
    resolved_transform: Option<Mat4>,
    resolved_inverse_transform: Option<Mat4>,
    foreground_color: Color,
    opacity: f32,
    scroll_direction: ScrollDirection,
    scroll_offset: Position,
    content_size: Size,
    scrollbar_drag: Option<ScrollbarDragState>,
    last_scrollbar_interaction: Option<Instant>,
    scrollbar_shadow_blur_radius: f32,
    pending_style_transition_requests: Vec<StyleTrackRequest>,
    pending_animation_requests: Vec<AnimationRequest>,
    pending_layout_transition_requests: Vec<LayoutTrackRequest>,
    pending_visual_transition_requests: Vec<VisualTrackRequest>,
    last_started_animator: Option<crate::Animator>,
    has_style_snapshot: bool,
    has_layout_snapshot: bool,
    layout_transition_visual_offset_x: f32,
    layout_transition_visual_offset_y: f32,
    layout_transition_override_width: Option<f32>,
    layout_transition_override_height: Option<f32>,
    layout_transition_target_x: Option<f32>,
    layout_transition_target_y: Option<f32>,
    layout_transition_target_width: Option<f32>,
    layout_transition_target_height: Option<f32>,
    last_parent_layout_x: f32,
    last_parent_layout_y: f32,
    layout_assigned_width: Option<f32>,
    layout_assigned_height: Option<f32>,
    is_hovered: bool,
    mouse_down_handlers: Vec<MouseDownHandler>,
    mouse_up_handlers: Vec<MouseUpHandler>,
    mouse_move_handlers: Vec<MouseMoveHandler>,
    mouse_enter_handlers: Vec<MouseEnterHandler>,
    mouse_leave_handlers: Vec<MouseLeaveHandler>,
    click_handlers: Vec<ClickHandler>,
    key_down_handlers: Vec<KeyDownHandler>,
    key_up_handlers: Vec<KeyUpHandler>,
    focus_handlers: Vec<FocusHandler>,
    blur_handlers: Vec<BlurHandler>,
    layout_dirty: bool,
    dirty_flags: DirtyFlags,
    last_layout_placement: Option<LayoutPlacement>,
    last_layout_proposal: Option<LayoutProposal>,
    flex_info: Option<FlexLayoutInfo>,
    has_absolute_descendant_for_hit_test: bool,
    absolute_clip_rect: Option<Rect>,
    anchor_parent_clip_rect: Option<Rect>,
    hit_test_clip_rect: Option<Rect>,
    children: Vec<Box<dyn ElementTrait>>,
}

impl Element {
    fn capture_style_snapshot(&self) -> ElementStyleSnapshot {
        let [bg_r, bg_g, bg_b, bg_a] = self.background_color.as_ref().to_rgba_u8();
        let [bt_r, bt_g, bt_b, bt_a] = self.border_colors.top.as_ref().to_rgba_u8();
        let [br_r, br_g, br_b, br_a] = self.border_colors.right.as_ref().to_rgba_u8();
        let [bb_r, bb_g, bb_b, bb_a] = self.border_colors.bottom.as_ref().to_rgba_u8();
        let [bl_r, bl_g, bl_b, bl_a] = self.border_colors.left.as_ref().to_rgba_u8();
        ElementStyleSnapshot {
            opacity: self.opacity,
            border_radius: self.border_radius,
            width: self.core.size.width,
            height: self.core.size.height,
            layout_width: self.core.layout_size.width,
            layout_height: self.core.layout_size.height,
            is_hovered: self.is_hovered,
            background_color: Color::rgba(bg_r, bg_g, bg_b, bg_a),
            foreground_color: self.foreground_color,
            border_top_color: Color::rgba(bt_r, bt_g, bt_b, bt_a),
            border_right_color: Color::rgba(br_r, br_g, br_b, br_a),
            border_bottom_color: Color::rgba(bb_r, bb_g, bb_b, bb_a),
            border_left_color: Color::rgba(bl_r, bl_g, bl_b, bl_a),
            box_shadows: self.box_shadows.clone(),
            transform: self.transform.clone(),
            transform_origin: self.transform_origin,
            transition_snapshot: Some(TransitionSnapshot {
                has_layout_snapshot: self.has_layout_snapshot,
                layout_transition_visual_offset_x: self.layout_transition_visual_offset_x,
                layout_transition_visual_offset_y: self.layout_transition_visual_offset_y,
                layout_transition_override_width: self.layout_transition_override_width,
                layout_transition_override_height: self.layout_transition_override_height,
                layout_transition_target_x: self.layout_transition_target_x,
                layout_transition_target_y: self.layout_transition_target_y,
                layout_transition_target_width: self.layout_transition_target_width,
                layout_transition_target_height: self.layout_transition_target_height,
                last_parent_layout_x: self.last_parent_layout_x,
                last_parent_layout_y: self.last_parent_layout_y,
                layout_assigned_width: self.layout_assigned_width,
                layout_assigned_height: self.layout_assigned_height,
            }),
        }
    }

    pub(crate) fn map_viewport_to_paint_space(
        &self,
        viewport_x: f32,
        viewport_y: f32,
    ) -> Option<(f32, f32)> {
        let inverse = self.resolved_inverse_transform?;
        let mapped = inverse * Vec4::new(viewport_x, viewport_y, 0.0, 1.0);
        let w = if mapped.w.abs() <= 0.000_001 {
            1.0
        } else {
            mapped.w
        };
        Some((mapped.x / w, mapped.y / w))
    }

    fn transformed_bounding_rect_for_rect(&self, rect: Rect) -> Rect {
        let Some(matrix) = self.resolved_transform else {
            return rect;
        };
        let corners = [
            Vec3::new(rect.x, rect.y, 0.0),
            Vec3::new(rect.x + rect.width.max(0.0), rect.y, 0.0),
            Vec3::new(
                rect.x + rect.width.max(0.0),
                rect.y + rect.height.max(0.0),
                0.0,
            ),
            Vec3::new(rect.x, rect.y + rect.height.max(0.0), 0.0),
        ];
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for corner in corners {
            let transformed = matrix * corner.extend(1.0);
            let w = if transformed.w.abs() <= 0.000_001 {
                1.0
            } else {
                transformed.w
            };
            let x = transformed.x / w;
            let y = transformed.y / w;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        Rect {
            x: min_x,
            y: min_y,
            width: (max_x - min_x).max(0.0),
            height: (max_y - min_y).max(0.0),
        }
    }

    fn transformed_frame_bounding_rect(&self, frame: LayoutFrame) -> Rect {
        self.transformed_bounding_rect_for_rect(Rect {
            x: frame.x,
            y: frame.y,
            width: frame.width.max(0.0),
            height: frame.height.max(0.0),
        })
    }

    fn untransformed_paint_bounds(&self) -> PromotionCompositeBounds {
        let snapshot = self.box_model_snapshot();
        let mut min_x = snapshot.x;
        let mut min_y = snapshot.y;
        let mut max_x = snapshot.x + snapshot.width.max(0.0);
        let mut max_y = snapshot.y + snapshot.height.max(0.0);
        for shadow in &self.box_shadows {
            if shadow.inset {
                continue;
            }
            let blur_padding = shadow.blur.max(0.0) * 1.5;
            let spread = shadow.spread.max(0.0);
            min_x = min_x.min(snapshot.x + shadow.offset_x - spread - blur_padding);
            min_y = min_y.min(snapshot.y + shadow.offset_y - spread - blur_padding);
            max_x = max_x.max(
                snapshot.x + snapshot.width.max(0.0) + shadow.offset_x + spread + blur_padding,
            );
            max_y = max_y.max(
                snapshot.y + snapshot.height.max(0.0) + shadow.offset_y + spread + blur_padding,
            );
        }
        PromotionCompositeBounds {
            x: min_x,
            y: min_y,
            width: (max_x - min_x).max(0.0),
            height: (max_y - min_y).max(0.0),
            corner_radii: [0.0; 4],
        }
    }

    fn union_promotion_bounds(
        current: PromotionCompositeBounds,
        next: PromotionCompositeBounds,
    ) -> PromotionCompositeBounds {
        let min_x = current.x.min(next.x);
        let min_y = current.y.min(next.y);
        let max_x = (current.x + current.width.max(0.0)).max(next.x + next.width.max(0.0));
        let max_y = (current.y + current.height.max(0.0)).max(next.y + next.height.max(0.0));
        PromotionCompositeBounds {
            x: min_x,
            y: min_y,
            width: (max_x - min_x).max(0.0),
            height: (max_y - min_y).max(0.0),
            corner_radii: [0.0; 4],
        }
    }

    fn transform_subtree_raster_bounds(&self) -> PromotionCompositeBounds {
        let mut bounds = self.untransformed_paint_bounds();
        for child in &self.children {
            let child_bounds = if let Some(element) = child.as_any().downcast_ref::<Element>() {
                element.transform_subtree_raster_bounds()
            } else {
                child.promotion_composite_bounds()
            };
            bounds = Self::union_promotion_bounds(bounds, child_bounds);
        }
        bounds
    }
}

impl ElementStyleSnapshot {
    fn current_value_for(&self, current: &ComputedStyle, field: StyleField) -> StyleValue {
        match field {
            StyleField::Opacity => StyleValue::Scalar(current.opacity.clamp(0.0, 1.0)),
            StyleField::BorderRadius => {
                let radius_base = self.width.min(self.height).max(0.0);
                let top_left = resolve_px(current.border_radii.top_left, radius_base, 0.0, 0.0);
                let top_right = resolve_px(current.border_radii.top_right, radius_base, 0.0, 0.0);
                let bottom_right =
                    resolve_px(current.border_radii.bottom_right, radius_base, 0.0, 0.0);
                let bottom_left =
                    resolve_px(current.border_radii.bottom_left, radius_base, 0.0, 0.0);
                StyleValue::Scalar(
                    top_left
                        .max(top_right)
                        .max(bottom_right)
                        .max(bottom_left)
                        .max(0.0),
                )
            }
            StyleField::BackgroundColor => StyleValue::Color(current.background_color),
            StyleField::Color => StyleValue::Color(current.color),
            StyleField::BorderTopColor => StyleValue::Color(current.border_colors.top),
            StyleField::BorderRightColor => StyleValue::Color(current.border_colors.right),
            StyleField::BorderBottomColor => StyleValue::Color(current.border_colors.bottom),
            StyleField::BorderLeftColor => StyleValue::Color(current.border_colors.left),
            StyleField::BoxShadow => StyleValue::BoxShadow(current.box_shadow.clone()),
            StyleField::Transform => StyleValue::Transform(current.transform.clone()),
            StyleField::TransformOrigin => StyleValue::TransformOrigin(current.transform_origin),
        }
    }

    fn value_for(&self, field: StyleField) -> StyleValue {
        match field {
            StyleField::Opacity => StyleValue::Scalar(self.opacity),
            StyleField::BorderRadius => StyleValue::Scalar(self.border_radius),
            StyleField::BackgroundColor => StyleValue::Color(self.background_color),
            StyleField::Color => StyleValue::Color(self.foreground_color),
            StyleField::BorderTopColor => StyleValue::Color(self.border_top_color),
            StyleField::BorderRightColor => StyleValue::Color(self.border_right_color),
            StyleField::BorderBottomColor => StyleValue::Color(self.border_bottom_color),
            StyleField::BorderLeftColor => StyleValue::Color(self.border_left_color),
            StyleField::BoxShadow => StyleValue::BoxShadow(self.box_shadows.clone()),
            StyleField::Transform => StyleValue::Transform(self.transform.clone()),
            StyleField::TransformOrigin => StyleValue::TransformOrigin(self.transform_origin),
        }
    }

    fn diff(&self, current: &ComputedStyle) -> Vec<StyleField> {
        const FIELDS: [StyleField; 11] = [
            StyleField::Opacity,
            StyleField::BorderRadius,
            StyleField::BackgroundColor,
            StyleField::Color,
            StyleField::BorderTopColor,
            StyleField::BorderRightColor,
            StyleField::BorderBottomColor,
            StyleField::BorderLeftColor,
            StyleField::BoxShadow,
            StyleField::Transform,
            StyleField::TransformOrigin,
        ];
        let mut out = Vec::new();
        for field in FIELDS {
            let previous = self.value_for(field);
            let next = self.current_value_for(current, field);
            let changed = match (previous, next) {
                (StyleValue::Scalar(lhs), StyleValue::Scalar(rhs)) => !approx_eq(lhs, rhs),
                (StyleValue::Color(lhs), StyleValue::Color(rhs)) => lhs != rhs,
                (StyleValue::BoxShadow(lhs), StyleValue::BoxShadow(rhs)) => lhs != rhs,
                (StyleValue::Transform(lhs), StyleValue::Transform(rhs)) => lhs != rhs,
                (StyleValue::TransformOrigin(lhs), StyleValue::TransformOrigin(rhs)) => lhs != rhs,
                _ => true,
            };
            if changed {
                out.push(field);
            }
        }
        out
    }
}

impl ElementTrait for Element {
    fn id(&self) -> u64 {
        self.core.id
    }

    fn parent_id(&self) -> Option<u64> {
        self.core.parent_id
    }

    fn set_parent_id(&mut self, parent_id: Option<u64>) {
        self.core.parent_id = parent_id;
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.core.id,
            parent_id: self.core.parent_id,
            x: self.core.layout_position.x,
            y: self.core.layout_position.y,
            width: self.core.layout_size.width,
            height: self.core.layout_size.height,
            border_radius: self.border_radius,
            should_render: self.core.should_render,
        }
    }

    fn children(&self) -> Option<&[Box<dyn ElementTrait>]> {
        Some(&self.children)
    }

    fn children_mut(&mut self) -> Option<&mut [Box<dyn ElementTrait>]> {
        Some(&mut self.children)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn intercepts_pointer_at(&self, viewport_x: f32, viewport_y: f32) -> bool {
        let local_x = viewport_x - self.core.layout_position.x;
        let local_y = viewport_y - self.core.layout_position.y;
        self.is_scrollbar_hit(local_x, local_y)
    }

    fn hit_test_visible_at(&self, viewport_x: f32, viewport_y: f32) -> bool {
        self.hit_test_clip_rect
            .map_or(true, |rect| rect.contains(viewport_x, viewport_y))
    }

    fn promotion_node_info(&self) -> PromotionNodeInfo {
        let border_width_sum = self.border_widths.left
            + self.border_widths.right
            + self.border_widths.top
            + self.border_widths.bottom;
        PromotionNodeInfo {
            estimated_pass_count: (1
                + usize::from(border_width_sum > 0.0)
                + (self.box_shadows.len() * 3))
                .min(u16::MAX as usize) as u16,
            opacity: self.opacity,
            has_rounded_clip: self.border_radius > 0.0,
            has_box_shadow: !self.box_shadows.is_empty(),
            has_border: border_width_sum > 0.0,
            is_scroll_container: self.scroll_direction != ScrollDirection::None,
            is_hovered: self.is_hovered,
        }
    }

    fn promotion_self_signature(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.core.should_render.hash(&mut hasher);
        self.core.should_paint.hash(&mut hasher);
        hash_f32(&mut hasher, self.core.layout_position.x);
        hash_f32(&mut hasher, self.core.layout_position.y);
        hash_f32(&mut hasher, self.core.layout_size.width.max(0.0));
        hash_f32(&mut hasher, self.core.layout_size.height.max(0.0));
        hash_f32(&mut hasher, self.layout_inner_size.width.max(0.0));
        hash_f32(&mut hasher, self.layout_inner_size.height.max(0.0));
        hash_f32(&mut hasher, self.padding.left);
        hash_f32(&mut hasher, self.padding.right);
        hash_f32(&mut hasher, self.padding.top);
        hash_f32(&mut hasher, self.padding.bottom);
        match self.scroll_direction {
            ScrollDirection::None => 0_u8,
            ScrollDirection::Vertical => 1_u8,
            ScrollDirection::Horizontal => 2_u8,
            ScrollDirection::Both => 3_u8,
        }
        .hash(&mut hasher);
        hash_f32(&mut hasher, self.scroll_offset.x);
        hash_f32(&mut hasher, self.scroll_offset.y);
        hash_f32(&mut hasher, self.content_size.width.max(0.0));
        hash_f32(&mut hasher, self.content_size.height.max(0.0));
        hash_f32(&mut hasher, self.layout_transition_visual_offset_x);
        hash_f32(&mut hasher, self.layout_transition_visual_offset_y);
        self.layout_transition_override_width
            .map(f32::to_bits)
            .hash(&mut hasher);
        self.layout_transition_override_height
            .map(f32::to_bits)
            .hash(&mut hasher);
        self.resolved_transform.is_some().hash(&mut hasher);
        if let Some(matrix) = self.resolved_transform {
            for value in matrix.to_cols_array() {
                hash_f32(&mut hasher, value);
            }
        }
        self.background_color
            .as_ref()
            .to_rgba_u8()
            .hash(&mut hasher);
        self.foreground_color.to_rgba_u8().hash(&mut hasher);
        self.border_colors
            .top
            .as_ref()
            .to_rgba_u8()
            .hash(&mut hasher);
        self.border_colors
            .right
            .as_ref()
            .to_rgba_u8()
            .hash(&mut hasher);
        self.border_colors
            .bottom
            .as_ref()
            .to_rgba_u8()
            .hash(&mut hasher);
        self.border_colors
            .left
            .as_ref()
            .to_rgba_u8()
            .hash(&mut hasher);
        hash_f32(&mut hasher, self.border_widths.left);
        hash_f32(&mut hasher, self.border_widths.right);
        hash_f32(&mut hasher, self.border_widths.top);
        hash_f32(&mut hasher, self.border_widths.bottom);
        hash_f32(&mut hasher, self.border_radii.top_left);
        hash_f32(&mut hasher, self.border_radii.top_right);
        hash_f32(&mut hasher, self.border_radii.bottom_right);
        hash_f32(&mut hasher, self.border_radii.bottom_left);
        for shadow in &self.box_shadows {
            shadow.color.to_rgba_u8().hash(&mut hasher);
            hash_f32(&mut hasher, shadow.offset_x);
            hash_f32(&mut hasher, shadow.offset_y);
            hash_f32(&mut hasher, shadow.blur);
            hash_f32(&mut hasher, shadow.spread);
            shadow.inset.hash(&mut hasher);
        }
        let scrollbar_alpha =
            (self.scrollbar_visibility_alpha().clamp(0.0, 1.0) * 255.0).round() as u16;
        scrollbar_alpha.hash(&mut hasher);
        let scrollbar_geometry = self.scrollbar_geometry(0.0, 0.0);
        for rect in [
            scrollbar_geometry.vertical_track,
            scrollbar_geometry.vertical_thumb,
            scrollbar_geometry.horizontal_track,
            scrollbar_geometry.horizontal_thumb,
        ] {
            rect.is_some().hash(&mut hasher);
            if let Some(rect) = rect {
                hash_f32(&mut hasher, rect.width.max(0.0));
                hash_f32(&mut hasher, rect.height.max(0.0));
                hash_f32(&mut hasher, rect.x.max(0.0));
                hash_f32(&mut hasher, rect.y.max(0.0));
            }
        }
        hasher.finish()
    }

    fn promotion_clip_intersection_signature(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.absolute_clip_scissor_rect()
            .is_some()
            .hash(&mut hasher);
        if let Some([x, y, width, height]) = self.absolute_clip_scissor_rect() {
            let clip = Rect {
                x: x as f32,
                y: y as f32,
                width: width as f32,
                height: height as f32,
            };
            let bounds = self.promotion_composite_bounds();
            let element_rect = Rect {
                x: bounds.x,
                y: bounds.y,
                width: bounds.width.max(0.0),
                height: bounds.height.max(0.0),
            };
            let intersection = intersect_rect(clip, element_rect);
            hash_f32(&mut hasher, intersection.x);
            hash_f32(&mut hasher, intersection.y);
            hash_f32(&mut hasher, intersection.width);
            hash_f32(&mut hasher, intersection.height);
        }
        if !self.children.is_empty() && self.has_inner_render_area() {
            let overflow_child_indices: Vec<bool> = (0..self.children.len())
                .map(|idx| self.child_renders_outside_inner_clip(idx))
                .collect();
            let outer_radii = normalize_corner_radii(
                self.border_radii,
                self.core.layout_size.width.max(0.0),
                self.core.layout_size.height.max(0.0),
            );
            let inner_radii = self.inner_clip_radii(outer_radii);
            let should_clip_children =
                self.should_clip_children(&overflow_child_indices, inner_radii);
            should_clip_children.hash(&mut hasher);
            if should_clip_children {
                let inner = self.inner_clip_rect();
                hash_f32(&mut hasher, inner.x);
                hash_f32(&mut hasher, inner.y);
                hash_f32(&mut hasher, inner.width);
                hash_f32(&mut hasher, inner.height);
                hash_f32(&mut hasher, inner_radii.top_left);
                hash_f32(&mut hasher, inner_radii.top_right);
                hash_f32(&mut hasher, inner_radii.bottom_right);
                hash_f32(&mut hasher, inner_radii.bottom_left);
            }
        }
        hasher.finish()
    }

    fn promotion_composite_bounds(&self) -> PromotionCompositeBounds {
        let paint_bounds = self.untransformed_paint_bounds();
        let transformed = self.transformed_bounding_rect_for_rect(Rect {
            x: paint_bounds.x,
            y: paint_bounds.y,
            width: paint_bounds.width,
            height: paint_bounds.height,
        });
        PromotionCompositeBounds {
            x: transformed.x,
            y: transformed.y,
            width: transformed.width,
            height: transformed.height,
            corner_radii: [0.0; 4],
        }
    }

    fn local_dirty_flags(&self) -> DirtyFlags {
        self.dirty_flags
    }

    fn clear_local_dirty_flags(&mut self, flags: DirtyFlags) {
        self.dirty_flags = self.dirty_flags.without(flags);
    }

    fn snapshot_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(self.capture_style_snapshot()))
    }

    fn restore_state(&mut self, snapshot: &dyn std::any::Any) -> bool {
        let Some(snapshot) = snapshot.downcast_ref::<ElementStyleSnapshot>() else {
            return false;
        };

        self.core.set_width(snapshot.width);
        self.core.set_height(snapshot.height);
        self.core.layout_size = Size {
            width: snapshot.layout_width,
            height: snapshot.layout_height,
        };
        self.is_hovered = snapshot.is_hovered;
        self.opacity = snapshot.opacity;
        self.border_radius = snapshot.border_radius;
        self.background_color = Box::new(snapshot.background_color);
        self.foreground_color = snapshot.foreground_color;
        self.border_colors.top = Box::new(snapshot.border_top_color);
        self.border_colors.right = Box::new(snapshot.border_right_color);
        self.border_colors.bottom = Box::new(snapshot.border_bottom_color);
        self.border_colors.left = Box::new(snapshot.border_left_color);
        self.box_shadows = snapshot.box_shadows.clone();
        self.transform = snapshot.transform.clone();
        self.transform_origin = snapshot.transform_origin;
        self.update_resolved_transform();
        if let Some(transition_snapshot) = snapshot.transition_snapshot {
            self.has_layout_snapshot = transition_snapshot.has_layout_snapshot;
            self.layout_transition_visual_offset_x =
                transition_snapshot.layout_transition_visual_offset_x;
            self.layout_transition_visual_offset_y =
                transition_snapshot.layout_transition_visual_offset_y;
            self.layout_transition_override_width =
                transition_snapshot.layout_transition_override_width;
            self.layout_transition_override_height =
                transition_snapshot.layout_transition_override_height;
            self.layout_transition_target_x = transition_snapshot.layout_transition_target_x;
            self.layout_transition_target_y = transition_snapshot.layout_transition_target_y;
            self.layout_transition_target_width =
                transition_snapshot.layout_transition_target_width;
            self.layout_transition_target_height =
                transition_snapshot.layout_transition_target_height;
            self.last_parent_layout_x = transition_snapshot.last_parent_layout_x;
            self.last_parent_layout_y = transition_snapshot.last_parent_layout_y;
            self.layout_assigned_width = transition_snapshot.layout_assigned_width;
            self.layout_assigned_height = transition_snapshot.layout_assigned_height;
        }
        self.has_style_snapshot = true;
        self.recompute_style();
        true
    }
}

impl Element {
    pub(crate) fn debug_render_state(&self) -> DebugElementRenderState {
        DebugElementRenderState {
            background_rgba: self.background_color.as_ref().to_rgba_u8(),
            foreground_rgba: self.foreground_color.to_rgba_u8(),
            opacity: self.opacity,
            border_radius: self.border_radius,
        }
    }

    #[cfg(test)]
    pub(crate) fn debug_transform(&self) -> &Transform {
        &self.transform
    }
}
