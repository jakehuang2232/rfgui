#![allow(missing_docs)]
use rustc_hash::{FxHashMap, FxHashSet};

use super::{ComputedStyleConsumer, ElementCore, Position, Size, Text, TextInlineIfcStyleMetadata};
use crate::style::ColorLike;
use crate::style::{
    Align, AnchorName, BoxShadow, ClipMode, Collision, CollisionBoundary, Color, ComputedStyle,
    Cursor, FlowDirection, FlowWrap, JustifyContent, Layout, Length, PositionMode, ScrollDirection,
    SizeValue, Style, StyleComputeContext, Transform, TransformKind, TransformOrigin,
    TransitionProperty, TransitionTiming, compute_style_with_context,
    interpolate_transform_with_reference_box,
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
    BlurEvent, ClickEvent, FocusEvent, KeyDownEvent, KeyUpEvent, PointerButton as UiPointerButton,
    PointerDownEvent, PointerEnterEvent, PointerLeaveEvent, PointerMoveEvent, PointerUpEvent,
};
use crate::view::base_component::round_layout_value;
use crate::view::frame_graph::texture_resource::TextureHandle;
use crate::view::frame_graph::{AttachmentTarget, FrameGraph, ResourceLifetime, TextureDesc};
use crate::view::inline_formatting_context::{
    InlineIfcAtomicBoxPlacementPackage, InlineIfcAtomicMeasureConstraints, InlineIfcCacheKey,
    InlineIfcDecorationBoxInsets, InlineIfcDistributedElementPackages,
    InlineIfcElementDecorationDrawRectPackage, InlineIfcElementDecorationDrawRectStyle,
    InlineIfcElementDecorationPackageSource, InlineIfcElementRootCandidate,
    InlineIfcElementRootCandidateCache, InlineIfcElementRootSource,
    InlineIfcElementRootSourceBuilder, InlineIfcInvalidation, InlineIfcItem,
    InlineIfcMeasuredAtomicBox, InlineIfcSize, InlineIfcSourceId, InlineIfcStyle,
};
use crate::view::node_arena::{NodeArena, NodeKey};
use crate::view::promotion::{PromotedLayerUpdateKind, PromotionNodeInfo};
use crate::view::render_pass::draw_rect_pass::DrawRectInput;
use crate::view::render_pass::draw_rect_pass::{DrawRectOutput, RectPassParams};
use crate::view::render_pass::draw_rect_pass::{RenderTargetIn, RenderTargetOut, RenderTargetTag};
use crate::view::render_pass::render_target::GraphicsPassContext;
use crate::view::render_pass::{
    DrawRectPass, GraphicsPass, OpaqueRectPass, RectRenderMode, ShadowMesh, ShadowModuleSpec,
    ShadowParams, build_shadow_module,
};
use crate::view::viewport::ViewportControl;
use glam::{Mat4, Vec3, Vec4};
use std::cell::RefCell;
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
include!("event_handler_props.rs");
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
struct LayoutTransitionState {
    frame: Size,
    width_keeps_content_constraint: bool,
    height_keeps_content_constraint: bool,
}

#[derive(Clone, Copy, Debug)]
struct ResolvedLayoutSizes {
    target: Size,
    axis_measure_constraint: Size,
    axis_place_constraint: Size,
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
pub(crate) struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
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

fn round_layout_position(x: f32, y: f32) -> Position {
    Position {
        x: round_layout_value(x),
        y: round_layout_value(y),
    }
}

fn round_layout_size(width: f32, height: f32) -> Size {
    Size {
        width: round_layout_value(width.max(0.0)),
        height: round_layout_value(height.max(0.0)),
    }
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
    anchors: FxHashMap<String, AnchorSnapshot>,
    ancestor_stack: Vec<AnchorSnapshot>,
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
    pub skipped_child_place_calls: usize,
    pub absolute_child_place_calls: usize,
    pub update_content_size_ms: f64,
    pub clamp_scroll_ms: f64,
    pub recompute_hit_test_ms: f64,
    pub placement_skip_failures: PlacementSkipFailureCounters,
    pub axis_placement_eligibility: AxisPlacementEligibilityProfile,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AxisPlacementEligibilityProfile {
    pub candidate_child_places: usize,
    pub clean_subtree_child_places: usize,
    pub dirty_subtree_child_places: usize,
    pub potential_replay_child_places: usize,
    pub inline_child_places: usize,
    pub flex_child_places: usize,
    pub flow_child_places: usize,
    pub inline_potential_replay_child_places: usize,
    pub flex_potential_replay_child_places: usize,
    pub flow_potential_replay_child_places: usize,
    pub blockers: PlacementSkipFailureCounters,
}

impl AxisPlacementEligibilityProfile {
    pub(crate) fn record_candidate(&mut self, layout: Layout) {
        self.candidate_child_places += 1;
        match layout {
            Layout::Inline => self.inline_child_places += 1,
            Layout::Flex { .. } => self.flex_child_places += 1,
            Layout::Flow { .. } => self.flow_child_places += 1,
            Layout::Grid => {}
        }
    }

    pub(crate) fn record_clean_subtree(&mut self) {
        self.clean_subtree_child_places += 1;
    }

    pub(crate) fn record_dirty_subtree(&mut self) {
        self.dirty_subtree_child_places += 1;
        self.blockers
            .record(PlacementSkipFailureReason::DirtySubtree);
    }

    pub(crate) fn record_potential_replay_candidate(&mut self, layout: Layout) {
        self.potential_replay_child_places += 1;
        match layout {
            Layout::Inline => self.inline_potential_replay_child_places += 1,
            Layout::Flex { .. } => self.flex_potential_replay_child_places += 1,
            Layout::Flow { .. } => self.flow_potential_replay_child_places += 1,
            Layout::Grid => {}
        }
    }

    pub(crate) fn record_blocker(&mut self, reason: PlacementSkipFailureReason) {
        self.blockers.record(reason);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PlacementSkipFailureCounters {
    pub dirty_subtree: usize,
    pub non_base_element: usize,
    pub non_leaf: usize,
    pub anchor_name: usize,
    pub anchor_ref: usize,
    pub absolute_descendant: usize,
    pub runtime_state: usize,
    pub placement_mismatch: usize,
    pub placement_dirty_self: usize,
    pub hit_test_clip_mismatch: usize,
    pub anchor_parent_clip_mismatch: usize,
}

impl PlacementSkipFailureCounters {
    pub(crate) fn total(&self) -> usize {
        self.dirty_subtree
            + self.non_base_element
            + self.non_leaf
            + self.anchor_name
            + self.anchor_ref
            + self.absolute_descendant
            + self.runtime_state
            + self.placement_mismatch
            + self.placement_dirty_self
            + self.hit_test_clip_mismatch
            + self.anchor_parent_clip_mismatch
    }

    pub(crate) fn record(&mut self, reason: PlacementSkipFailureReason) {
        match reason {
            PlacementSkipFailureReason::DirtySubtree => self.dirty_subtree += 1,
            PlacementSkipFailureReason::NonBaseElement => self.non_base_element += 1,
            PlacementSkipFailureReason::AnchorName => self.anchor_name += 1,
            PlacementSkipFailureReason::AnchorRef => self.anchor_ref += 1,
            PlacementSkipFailureReason::AbsoluteDescendant => self.absolute_descendant += 1,
            PlacementSkipFailureReason::RuntimeState => self.runtime_state += 1,
            PlacementSkipFailureReason::PlacementMismatch => self.placement_mismatch += 1,
            PlacementSkipFailureReason::PlacementDirtySelf => self.placement_dirty_self += 1,
            PlacementSkipFailureReason::HitTestClipMismatch => self.hit_test_clip_mismatch += 1,
            PlacementSkipFailureReason::AnchorParentClipMismatch => {
                self.anchor_parent_clip_mismatch += 1;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlacementSkipFailureReason {
    DirtySubtree,
    NonBaseElement,
    AnchorName,
    AnchorRef,
    AbsoluteDescendant,
    RuntimeState,
    PlacementMismatch,
    PlacementDirtySelf,
    HitTestClipMismatch,
    AnchorParentClipMismatch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LayoutGateCandidateProfile {
    /// Clean child subtrees observed while checking the existing measure gate.
    /// Candidate counts are informational only; they do not imply a skip.
    pub measure_candidate_clean_children: usize,
    pub measure_dirty_children: usize,
    /// Clean child subtrees observed while checking the existing placement gate.
    /// Candidate counts are informational only; they do not imply a skip.
    pub placement_candidate_clean_children: usize,
    pub placement_dirty_children: usize,
}

thread_local! {
    static LAYOUT_PLACE_PROFILE: RefCell<LayoutPlaceProfile> =
        RefCell::new(LayoutPlaceProfile::default());
    static LAYOUT_GATE_CANDIDATE_PROFILE: RefCell<LayoutGateCandidateProfile> =
        RefCell::new(LayoutGateCandidateProfile::default());
}

pub(crate) fn reset_layout_place_profile() {
    LAYOUT_PLACE_PROFILE.with(|profile| {
        *profile.borrow_mut() = LayoutPlaceProfile::default();
    });
}

pub(crate) fn take_layout_place_profile() -> LayoutPlaceProfile {
    LAYOUT_PLACE_PROFILE.with(|profile| std::mem::take(&mut *profile.borrow_mut()))
}

/// Mutate the per-frame layout-place profile via a closure.
/// `pub(crate)` so layout-pipeline modules (e.g. `crate::view::layout::place`)
/// can record profile counters without exposing the thread-local directly.
pub(crate) fn with_layout_place_profile<R>(f: impl FnOnce(&mut LayoutPlaceProfile) -> R) -> R {
    LAYOUT_PLACE_PROFILE.with(|profile| f(&mut profile.borrow_mut()))
}

pub(crate) fn reset_layout_gate_candidate_profile() {
    LAYOUT_GATE_CANDIDATE_PROFILE.with(|profile| {
        *profile.borrow_mut() = LayoutGateCandidateProfile::default();
    });
}

pub(crate) fn take_layout_gate_candidate_profile() -> LayoutGateCandidateProfile {
    LAYOUT_GATE_CANDIDATE_PROFILE.with(|profile| std::mem::take(&mut *profile.borrow_mut()))
}

#[derive(Clone, Copy)]
enum LayoutGateCandidatePhase {
    Measure,
    Placement,
}

fn record_layout_gate_child_candidates(
    children: &[crate::view::node_arena::NodeKey],
    arena: &crate::view::node_arena::NodeArena,
    mask: DirtyFlags,
    phase: LayoutGateCandidatePhase,
) -> bool {
    let mut clean_children = 0;
    let mut dirty_children = 0;
    for &child_key in children {
        if arena.subtree_dirty_intersects(child_key, mask) {
            dirty_children += 1;
        } else {
            clean_children += 1;
        }
    }

    LAYOUT_GATE_CANDIDATE_PROFILE.with(|profile| {
        let mut profile = profile.borrow_mut();
        match phase {
            LayoutGateCandidatePhase::Measure => {
                profile.measure_candidate_clean_children += clean_children;
                profile.measure_dirty_children += dirty_children;
            }
            LayoutGateCandidatePhase::Placement => {
                profile.placement_candidate_clean_children += clean_children;
                profile.placement_dirty_children += dirty_children;
            }
        }
    });

    dirty_children > 0
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
    paint_offset: [f32; 2],
    promoted_node_ids: Arc<FxHashSet<u64>>,
    promoted_update_kinds: Arc<FxHashMap<u64, PromotedLayerUpdateKind>>,
    promoted_composition_update_kinds: Arc<FxHashMap<u64, PromotedLayerUpdateKind>>,
}

/// Mutable build state threaded through low-level render graph construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct DeferredRenderNode {
    pub key: crate::view::node_arena::NodeKey,
    pub stable_id: u64,
}

#[derive(Clone)]
pub struct BuildState {
    target: Option<RenderTargetOut>,
    depth_stencil_target: Option<AttachmentTarget>,
    target_pairs: FxHashMap<u32, AttachmentTarget>,
    scissor_rect: Option<[u32; 4]>,
    clip_id_stack: Vec<u8>,
    deferred_nodes: Vec<DeferredRenderNode>,
    dfs_opaque_rect_order: u32,
}

impl BuildState {
    pub fn current_target(&self) -> Option<RenderTargetOut> {
        self.target
    }

    pub(crate) fn for_layer_subtree_with_ancestor_clip(ancestor_clip: AncestorClipContext) -> Self {
        Self {
            target: None,
            depth_stencil_target: None,
            target_pairs: FxHashMap::default(),
            scissor_rect: ancestor_clip.scissor_rect,
            clip_id_stack: Vec::new(),
            deferred_nodes: Vec::new(),
            dfs_opaque_rect_order: 0,
        }
    }

    pub(crate) fn merge_child_side_effects(&mut self, child: &BuildState) {
        self.dfs_opaque_rect_order = self.dfs_opaque_rect_order.max(child.dfs_opaque_rect_order);
        for (&color_handle, &depth_target) in &child.target_pairs {
            self.target_pairs.insert(color_handle, depth_target);
        }
        for node in &child.deferred_nodes {
            if !self
                .deferred_nodes
                .iter()
                .any(|existing| existing.key == node.key)
            {
                self.deferred_nodes.push(node.clone());
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
                paint_offset: [0.0, 0.0],
                promoted_node_ids: Arc::new(FxHashSet::default()),
                promoted_update_kinds: Arc::new(FxHashMap::default()),
                promoted_composition_update_kinds: Arc::new(FxHashMap::default()),
            },
            state: BuildState {
                target: None,
                depth_stencil_target: Some(AttachmentTarget::Surface),
                target_pairs: FxHashMap::default(),
                scissor_rect: None,
                clip_id_stack: Vec::new(),
                deferred_nodes: Vec::new(),
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

    pub fn current_target(&self) -> Option<RenderTargetOut> {
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

    pub(crate) fn paint_offset(&self) -> [f32; 2] {
        self.viewport.paint_offset
    }

    pub(crate) fn translate_paint_offset(&mut self, dx: f32, dy: f32) {
        self.viewport.paint_offset[0] += dx;
        self.viewport.paint_offset[1] += dy;
    }

    pub(crate) fn set_paint_offset(&mut self, offset: [f32; 2]) {
        self.viewport.paint_offset = offset;
    }

    pub(crate) fn paint_point(&self, x: f32, y: f32) -> [f32; 2] {
        [
            x + self.viewport.paint_offset[0],
            y + self.viewport.paint_offset[1],
        ]
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

    /// Replace the active scissor rect outright, returning the previous value
    /// for later restoration. Use this when an element with
    /// `ClipMode::Viewport` (or similar escape-hatch semantics) must paint
    /// outside any ancestor scissor — `push_scissor_rect` would intersect
    /// with the ancestor and re-clip the element back into the parent box.
    pub(crate) fn replace_scissor_rect(
        &mut self,
        scissor_rect: Option<[u32; 4]>,
    ) -> Option<[u32; 4]> {
        let previous = self.state.scissor_rect;
        self.state.scissor_rect = scissor_rect;
        previous
    }

    pub(crate) fn restore_scissor_rect(&mut self, previous: Option<[u32; 4]>) {
        self.state.scissor_rect = previous;
    }

    pub(crate) fn append_to_defer(
        &mut self,
        key: crate::view::node_arena::NodeKey,
        stable_id: u64,
    ) {
        if !self.state.deferred_nodes.iter().any(|node| node.key == key) {
            self.state
                .deferred_nodes
                .push(DeferredRenderNode { key, stable_id });
        }
    }

    pub(crate) fn take_deferred_nodes(&mut self) -> Vec<DeferredRenderNode> {
        std::mem::take(&mut self.state.deferred_nodes)
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
        promoted_node_ids: Arc<FxHashSet<u64>>,
        promoted_update_kinds: Arc<FxHashMap<u64, PromotedLayerUpdateKind>>,
        promoted_composition_update_kinds: Arc<FxHashMap<u64, PromotedLayerUpdateKind>>,
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

    pub fn target_width(&self) -> u32 {
        self.target_width
    }

    pub fn target_height(&self) -> u32 {
        self.target_height
    }

    pub fn target_format(&self) -> wgpu::TextureFormat {
        self.target_format
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

/// Dirty masks consumed by each retained-engine pass.
///
/// These masks document pass dependencies before Phase 4 starts using
/// them for finer-grained traversal gating. They intentionally do not
/// change ownership: `Element::local_dirty_flags()` remains part of the
/// formal dirty truth while arena dirty is being migrated in.
pub(crate) struct DirtyPassMask;

impl DirtyPassMask {
    /// Measure/layout pass dependency.
    pub const LAYOUT: DirtyFlags = DirtyFlags::LAYOUT;
    /// Placement pass dependency. Box-model and hit-test data are derived
    /// from placement, but paint-only changes must not force placement.
    pub const PLACEMENT: DirtyFlags = DirtyFlags::PLACE
        .union(DirtyFlags::BOX_MODEL)
        .union(DirtyFlags::HIT_TEST);
    /// Box-model snapshot refresh dependency.
    pub const BOX_MODEL: DirtyFlags = DirtyFlags::BOX_MODEL;
    /// Hit-test data refresh dependency.
    pub const HIT_TEST: DirtyFlags = DirtyFlags::HIT_TEST;
    /// Render/damage dependency.
    pub const PAINT: DirtyFlags = DirtyFlags::PAINT;
    /// Runtime-only update dependency.
    pub const RUNTIME: DirtyFlags = DirtyFlags::RUNTIME;
}

#[cfg(test)]
mod dirty_pass_mask_tests {
    use super::{DirtyFlags, DirtyPassMask};

    #[test]
    fn dirty_pass_masks_encode_phase_4a_dependencies() {
        assert_eq!(DirtyPassMask::LAYOUT, DirtyFlags::LAYOUT);

        let placement = DirtyFlags::PLACE
            .union(DirtyFlags::BOX_MODEL)
            .union(DirtyFlags::HIT_TEST);
        assert_eq!(DirtyPassMask::PLACEMENT, placement);
        assert!(!DirtyPassMask::PLACEMENT.intersects(DirtyFlags::LAYOUT));
        assert!(!DirtyPassMask::PLACEMENT.intersects(DirtyFlags::PAINT));

        assert_eq!(DirtyPassMask::BOX_MODEL, DirtyFlags::BOX_MODEL);
        assert_eq!(DirtyPassMask::HIT_TEST, DirtyFlags::HIT_TEST);
        assert_eq!(DirtyPassMask::PAINT, DirtyFlags::PAINT);
        assert!(!DirtyPassMask::PAINT.intersects(DirtyPassMask::PLACEMENT));

        assert_eq!(
            DirtyPassMask::RUNTIME,
            DirtyPassMask::PLACEMENT.union(DirtyPassMask::PAINT)
        );
        assert_eq!(
            DirtyPassMask::RUNTIME,
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST)
                .union(DirtyFlags::PAINT)
        );
        assert!(!DirtyPassMask::RUNTIME.intersects(DirtyFlags::LAYOUT));
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InlineMeasureContext {
    pub first_available_width: f32,
    pub full_available_width: f32,
    pub available_height: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub percent_base_width: Option<f32>,
    pub percent_base_height: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InlineNodeSize {
    pub width: f32,
    pub height: f32,
    /// Distance from the fragment's top to its baseline (cross-axis).
    /// Surfaces typography baseline for `Layout::Inline` cross-axis
    /// alignment per `docs/design/inline-baseline.md`. Conventions:
    /// - Text / TextAreaTextRun fragment: text layout adapter baseline
    ///   for the first visual line.
    /// - Non-fragmentable Element: `height` (bottom edge).
    /// - Fragmentable Inline element fragment: that fragment's inner
    ///   `line_ascent` (relative to its line box top, excluding outer
    ///   vertical padding/border which paint outside the line box).
    /// - Default / empty: `0.0`.
    pub baseline: f32,
    /// Cross-axis alignment effective for this fragment. Initial
    /// `Baseline`; per `docs/design/inline-baseline.md` D5/D5a
    /// `Layoutable` producers fill this from their inherited value
    /// (`ComputedStyle.vertical_align` for Element, dedicated field for
    /// Text / TextAreaTextRun). Read by the inline place pipeline (D3).
    pub vertical_align: crate::style::VerticalAlign,
    /// Hard line break after this fragment. Honored by the inline solver
    /// even when `solver_wrap` (soft overflow wrap) is disabled — this is
    /// how `\n` paragraphs in `TextArea` produce new lines while
    /// `auto_wrap` is off.
    pub force_break_after: bool,
}

impl Default for InlineNodeSize {
    fn default() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            baseline: 0.0,
            vertical_align: crate::style::VerticalAlign::Baseline,
            force_break_after: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InlinePlacement {
    pub node_index: usize,
    pub x: f32,
    pub y: f32,
    pub offset_x: f32,
    pub offset_y: f32,
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

/// Flex-relevant properties for a single element, packed row/col so the flex
/// solver can pick the correct axis without issuing multiple trait calls.
///
/// `intrinsic_*` expresses content-sized behavior: `Text` / `Image` / `Svg`
/// set these; plain containers leave them `None`. `intrinsic_as_base` picks
/// between the two historical uses (Image: seed the flex base; Text: used as
/// `auto_min`).
#[derive(Clone, Copy, Debug)]
pub struct FlexProps {
    pub grow: f32,
    pub shrink: f32,
    pub basis: SizeValue,

    pub width: SizeValue,
    pub height: SizeValue,
    pub min_width: SizeValue,
    pub min_height: SizeValue,
    pub max_width: SizeValue,
    pub max_height: SizeValue,

    pub has_explicit_min_width: bool,
    pub has_explicit_min_height: bool,

    pub allows_cross_stretch_when_row: bool,
    pub allows_cross_stretch_when_col: bool,

    pub intrinsic_width: Option<f32>,
    pub intrinsic_height: Option<f32>,
    /// If true, `intrinsic_*` seeds `auto_min_main` when main size is `Auto`
    /// and no explicit min is set (Text / TextArea content-sized min).
    pub intrinsic_feeds_auto_min: bool,
    /// If true, `intrinsic_*` seeds `auto_base_main` (Image / Svg aspect-ratio
    /// preservation when flex-basis is auto and main size is auto).
    pub intrinsic_feeds_auto_base: bool,
}

impl Default for FlexProps {
    fn default() -> Self {
        Self {
            grow: 0.0,
            shrink: 1.0,
            basis: SizeValue::Auto,
            width: SizeValue::Auto,
            height: SizeValue::Auto,
            min_width: SizeValue::Length(Length::Px(0.0)),
            min_height: SizeValue::Length(Length::Px(0.0)),
            max_width: SizeValue::Auto,
            max_height: SizeValue::Auto,
            has_explicit_min_width: false,
            has_explicit_min_height: false,
            allows_cross_stretch_when_row: false,
            allows_cross_stretch_when_col: false,
            intrinsic_width: None,
            intrinsic_height: None,
            intrinsic_feeds_auto_min: false,
            intrinsic_feeds_auto_base: false,
        }
    }
}

impl FlexProps {
    #[inline]
    pub fn main_size(&self, is_row: bool) -> SizeValue {
        if is_row { self.width } else { self.height }
    }
    #[inline]
    pub fn min_main(&self, is_row: bool) -> SizeValue {
        if is_row {
            self.min_width
        } else {
            self.min_height
        }
    }
    #[inline]
    pub fn max_main(&self, is_row: bool) -> SizeValue {
        if is_row {
            self.max_width
        } else {
            self.max_height
        }
    }
    #[inline]
    pub fn has_explicit_min_main(&self, is_row: bool) -> bool {
        if is_row {
            self.has_explicit_min_width
        } else {
            self.has_explicit_min_height
        }
    }
    #[inline]
    pub fn allows_cross_stretch(&self, is_row: bool) -> bool {
        if is_row {
            self.allows_cross_stretch_when_row
        } else {
            self.allows_cross_stretch_when_col
        }
    }
    #[inline]
    pub fn intrinsic_main(&self, is_row: bool) -> Option<f32> {
        if is_row {
            self.intrinsic_width
        } else {
            self.intrinsic_height
        }
    }

    /// Derived auto_min_main: only content-sized elements opt in via
    /// `intrinsic_feeds_auto_min`. Gated on flex-basis = `Auto`, main
    /// size = `Auto`, and no explicit min so an authored basis remains
    /// the item's starting constraint instead of being clamped by content.
    #[inline]
    pub fn auto_min_main(&self, is_row: bool) -> Option<f32> {
        if !self.intrinsic_feeds_auto_min
            || self.has_explicit_min_main(is_row)
            || self.basis != SizeValue::Auto
            || self.main_size(is_row) != SizeValue::Auto
        {
            return None;
        }
        self.intrinsic_main(is_row)
    }

    /// Derived auto_base_main: only Image/Svg opt in via
    /// `intrinsic_feeds_auto_base`.
    #[inline]
    pub fn auto_base_main(&self, is_row: bool) -> Option<f32> {
        if self.intrinsic_feeds_auto_base {
            self.intrinsic_main(is_row).map(|v| v.max(0.0))
        } else {
            None
        }
    }
}

pub trait Layoutable {
    /// Per-frame flush for deferred arena mutations (e.g. projection subtree
    /// commits queued by imperative setters). Runs before measure. Default
    /// noop. Override when the element owns arena state that external setters
    /// cannot commit directly.
    fn sync_arena(&mut self, _arena: &mut crate::view::node_arena::NodeArena) {}
    fn measure(
        &mut self,
        constraints: LayoutConstraints,
        arena: &mut crate::view::node_arena::NodeArena,
    );
    fn place(&mut self, placement: LayoutPlacement, arena: &mut crate::view::node_arena::NodeArena);
    fn measured_size(&self) -> (f32, f32);
    fn layout_target_size(&self) -> (f32, f32) {
        self.measured_size()
    }
    fn set_layout_width(&mut self, width: f32);
    fn set_layout_height(&mut self, height: f32);
    fn flex_props(&self) -> FlexProps {
        FlexProps::default()
    }
    fn cross_alignment_size(
        &self,
        is_row: bool,
        stretched_cross: Option<f32>,
        _arena: &crate::view::node_arena::NodeArena,
    ) -> f32 {
        let (measured_w, measured_h) = self.measured_size();
        stretched_cross.unwrap_or(if is_row { measured_h } else { measured_w })
    }
    fn inline_relative_position(&self) -> (f32, f32) {
        (0.0, 0.0)
    }
    fn set_layout_offset(&mut self, _x: f32, _y: f32) {}
    fn measure_inline(
        &mut self,
        context: InlineMeasureContext,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        self.measure(
            LayoutConstraints {
                max_width: context.first_available_width.max(0.0),
                max_height: 1_000_000.0,
                viewport_width: context.viewport_width,
                viewport_height: context.viewport_height,
                percent_base_width: context.percent_base_width,
                percent_base_height: context.percent_base_height,
            },
            arena,
        );
    }
    fn get_inline_nodes_size(
        &self,
        _arena: &crate::view::node_arena::NodeArena,
    ) -> Vec<InlineNodeSize> {
        let (width, height) = self.measured_size();
        vec![InlineNodeSize {
            width,
            height,
            // Non-fragmentable element: baseline = bottom edge
            // (`docs/design/inline-baseline.md` D1). User wanting
            // text-baseline alignment for `<Element><Text/></Element>`
            // must drop the Element wrapper (Phase 2 will add
            // baseline-from-children).
            baseline: height,
            ..Default::default()
        }]
    }
    fn place_inline(
        &mut self,
        placement: InlinePlacement,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        self.set_layout_offset(placement.offset_x, placement.offset_y);
        self.place(
            LayoutPlacement {
                parent_x: placement.parent_x,
                parent_y: placement.parent_y,
                visual_offset_x: placement.visual_offset_x,
                visual_offset_y: placement.visual_offset_y,
                available_width: placement.available_width,
                available_height: placement.available_height,
                viewport_width: placement.viewport_width,
                viewport_height: placement.viewport_height,
                percent_base_width: placement.percent_base_width,
                percent_base_height: placement.percent_base_height,
            },
            arena,
        );
    }
}

pub trait EventTarget {
    fn dispatch_pointer_down(
        &mut self,
        _event: &mut PointerDownEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_pointer_up(
        &mut self,
        _event: &mut PointerUpEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_pointer_move(
        &mut self,
        _event: &mut PointerMoveEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_pointer_enter(
        &mut self,
        _event: &mut PointerEnterEvent,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_pointer_leave(
        &mut self,
        _event: &mut PointerLeaveEvent,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_click(
        &mut self,
        _event: &mut ClickEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_context_menu(
        &mut self,
        _event: &mut crate::ui::ContextMenuEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_wheel(
        &mut self,
        _event: &mut crate::ui::WheelEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_key_down(
        &mut self,
        _event: &mut KeyDownEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_key_up(
        &mut self,
        _event: &mut KeyUpEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_text_input(
        &mut self,
        _event: &mut crate::ui::TextInputEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_ime_preedit(
        &mut self,
        _event: &mut crate::ui::ImePreeditEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_focus(
        &mut self,
        _event: &mut FocusEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_blur(
        &mut self,
        _event: &mut BlurEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_ime_commit(
        &mut self,
        _event: &mut crate::ui::ImeCommitEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_ime_enabled(
        &mut self,
        _event: &mut crate::ui::ImeEnabledEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_ime_disabled(
        &mut self,
        _event: &mut crate::ui::ImeDisabledEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_drag_start(
        &mut self,
        _event: &mut crate::ui::DragStartEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_drag_over(
        &mut self,
        _event: &mut crate::ui::DragOverEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_drag_leave(
        &mut self,
        _event: &mut crate::ui::DragLeaveEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_drop(
        &mut self,
        _event: &mut crate::ui::DropEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_drag_end(
        &mut self,
        _event: &mut crate::ui::DragEndEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_copy(
        &mut self,
        _event: &mut crate::ui::CopyEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_cut(
        &mut self,
        _event: &mut crate::ui::CutEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }
    fn dispatch_paste(
        &mut self,
        _event: &mut crate::ui::PasteEvent,
        _control: &mut ViewportControl<'_>,
        _arena: &crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }

    /// TextArea v2: when `true`, key/text-input/IME/focus events that would
    /// dispatch to descendants of this node are short-circuited at this node
    /// — descendants do NOT receive them. Pointer events are unaffected so
    /// projection-internal widgets remain interactive. Default `false` keeps
    /// all existing components transparent.
    fn block_key_down_child_event(&self) -> bool {
        false
    }
    fn block_key_up_child_event(&self) -> bool {
        false
    }
    fn block_text_input_child_event(&self) -> bool {
        false
    }
    fn block_ime_preedit_child_event(&self) -> bool {
        false
    }
    fn block_focus_child_event(&self) -> bool {
        false
    }

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
    fn build(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        ctx: UiBuildContext,
    ) -> BuildState;
}

/// Dynamic type-name supertrait for [`ElementTrait`]. Implemented via a
/// blanket `impl<T: 'static>` so every concrete element type gets a
/// correct [`std::any::type_name`] entry in its vtable — callers holding
/// `&dyn ElementTrait` can then resolve the concrete type's name
/// (unlike [`std::any::type_name_of_val`] on a trait object, which
/// returns the *static* `"dyn …"` name).
pub trait ElementTypeName {
    fn element_type_name(&self) -> &'static str;
}

impl<T: 'static + ?Sized> ElementTypeName for T {
    #[inline]
    fn element_type_name(&self) -> &'static str {
        std::any::type_name::<T>()
    }
}

pub trait ElementTrait:
    Layoutable + EventTarget + Renderable + ElementTypeName + std::any::Any
{
    fn stable_id(&self) -> u64;
    fn box_model_snapshot(&self) -> BoxModelSnapshot;

    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    // Phase B: `snapshot_state` / `restore_state` removed. The
    // host-state save/restore hack lived here so the cold-rebuild
    // pipeline could re-seed Element-internal state (scroll offset,
    // hover, layout-transition snapshots, …) on every render. With
    // the incremental-commit path keeping Element instances alive
    // across renders, the hack is dead weight on the happy path;
    // remaining fallback paths accept the documented state loss.

    fn intercepts_pointer_at(&self, _viewport_x: f32, _viewport_y: f32) -> bool {
        false
    }

    fn hit_test_visible_at(&self, _viewport_x: f32, _viewport_y: f32) -> bool {
        true
    }

    fn promotion_node_info(&self) -> PromotionNodeInfo {
        PromotionNodeInfo::default()
    }

    /// Promotion contract: this host's render path walks promoted
    /// descendants and composites their layer textures back into its own
    /// render target. When `false`, the promotion runtime must NOT place
    /// any descendant of `self` into the promoted set — otherwise the
    /// promoted layer is allocated but never composited (orphan), and the
    /// orphan's `is_node_promoted` flag also makes ancestor base-walks
    /// skip the node, dropping the subtree.
    ///
    /// `Element` overrides to `true` (it implements
    /// `compose_promoted_descendants_only`). All other hosts default to
    /// `false` until they implement an equivalent compose path.
    fn supports_promoted_descendants(&self) -> bool {
        false
    }

    /// Does this host's subtree contain any node currently in the
    /// promoted set, OR any nested descendant whose subtree does? Used by
    /// promotion-aware ancestors to decide whether to enter the compose
    /// loop at all (skipping the loop is a hot-path optimization for
    /// subtrees with no promoted layer work to do).
    ///
    /// Default recursion walks `children()` and recurses via this same
    /// trait method — which is what lets a promotion-aware non-Element
    /// host (TextArea) expose its promoted descendants to its Element
    /// ancestor. Viewport-clip absolute Elements are skipped because
    /// they take the deferred path, not the ancestor's compose.
    fn has_composited_promoted_descendants(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        ctx: &UiBuildContext,
    ) -> bool {
        for child_key in self.children() {
            let Some(node) = arena.get(*child_key) else {
                continue;
            };
            let child = node.element.as_ref();
            if child
                .as_any()
                .downcast_ref::<Element>()
                .is_some_and(Element::should_append_to_root_viewport_render)
            {
                continue;
            }
            if ctx.is_node_promoted(child.stable_id()) {
                return true;
            }
            if child.has_composited_promoted_descendants(arena, ctx) {
                return true;
            }
        }
        false
    }

    fn has_active_animator(&self) -> bool {
        false
    }

    fn promotion_self_signature(&self) -> u64 {
        0
    }

    fn promotion_clip_intersection_signature(
        &self,
        _arena: &crate::view::node_arena::NodeArena,
    ) -> u64 {
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

    /// Child node keys. Default returns empty — leaf elements (Text,
    /// Image, Svg) don't need to override. Containers override to expose
    /// their arena-backed child list so sibling walkers can traverse.
    ///
    /// Note: historical dispatch/hit-test code used an `Option<Vec<Box<dyn
    /// ElementTrait>>>` shape. During the Approach-C migration those
    /// walkers are being rewritten against the arena, so this method
    /// returns the slot-key slice directly.
    fn children(&self) -> &[crate::view::node_arena::NodeKey] {
        &[]
    }

    /// Mutable child-key list. Default returns `None` — leaf elements
    /// don't have child lists. Containers override with `Some(&mut vec)`.
    fn children_mut(&mut self) -> Option<&mut Vec<crate::view::node_arena::NodeKey>> {
        None
    }

    /// Legacy u64 parent id. Retained for renderer_adapter compatibility
    /// during Approach-C migration.
    fn parent_id(&self) -> Option<u64> {
        None
    }

    /// Set the legacy u64 parent id. Default is a no-op for leaf elements
    /// that have no parent tracking.
    fn set_parent_id(&mut self, _parent_id: Option<u64>) {}

    /// 軌 1 #14: cold-path counterpart to `apply_prop`. Push every
    /// prop on `node.props` into `self`. Default is a no-op so leaf
    /// hosts that don't author any incremental prop schema yet keep
    /// the cold path's existing behaviour; hosts that have moved
    /// their schema onto `apply_prop` override this to share the
    /// single source of truth between cold convert and incremental
    /// commit.
    fn ingest_props(&mut self, _node: &crate::ui::RsxElementNode) -> Result<(), String> {
        Ok(())
    }

    /// 軌 1 #14 Phase 3: replay the ancestor text cascade onto this
    /// host. Default is a no-op (most hosts don't read inherited
    /// text props). Text / TextArea override to fill font / color /
    /// cursor / text_wrap that the author didn't explicitly set.
    fn apply_inherited(&mut self, _inherited: &crate::view::renderer_adapter::StyleCascadeContext) {
    }

    /// 軌 1 #14 Phase 3: produce the `StyleCascadeContext` to pass to
    /// this host's children. Default returns the parent cascade
    /// unchanged (leaves and non-Element containers don't introduce
    /// new cascade declarations). `Element` overrides to merge its
    /// own `parsed_style()` on top of the parent cascade.
    fn child_style_cascade(
        &self,
        parent: &crate::view::renderer_adapter::StyleCascadeContext,
    ) -> crate::view::renderer_adapter::StyleCascadeContext {
        parent.clone()
    }

    /// 軌 1 #14 Phase 4: receive freshly committed side-channel
    /// subtrees. `name` matches the `SideSlot.name` declared by the
    /// host's cold-path builder; `keys` are the arena keys for that
    /// slot's descriptor roots (parented to `self`, but kept off
    /// `Node.children`). Default no-op so hosts that don't author
    /// any side slots stay simple. Image / Svg override to fill
    /// `loading_slot` / `error_slot`.
    fn attach_side_slot(
        &mut self,
        _name: &'static str,
        _keys: Vec<crate::view::node_arena::NodeKey>,
    ) {
    }

    /// 軌 1 #14 Phase 4: hook that runs after this host's children
    /// and side-slots have been committed and `Node.parent` /
    /// `Node.children` are wired. Default no-op. TextArea overrides
    /// to record its own `NodeKey` (needed by projection rebuild +
    /// dispatch routing).
    fn after_commit(
        &mut self,
        _arena: &mut crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
    ) {
    }

    /// 軌 1 #14 Phase 5: build the descriptor list for this host's
    /// arena children. Default walks `node.children`, flattens
    /// fragments, and recurses through the adapter's
    /// `convert_node_desc`. Hosts whose child shape doesn't match
    /// the standard pattern override:
    /// - Text: empty (children collapse into the leaf's String content)
    /// - TextArea: spawn a `TextAreaTextRun` from `self.content` /
    ///   placeholder; projection segments rebuild later
    /// - Image / Svg: rejects non-empty children (use `loading` / `error`)
    fn build_children(
        &self,
        node: &crate::ui::RsxElementNode,
        path: &[u64],
        global_path: Option<&crate::view::renderer_adapter::GlobalNodePath>,
        inherited: &crate::view::renderer_adapter::StyleCascadeContext,
    ) -> Result<Vec<crate::view::renderer_adapter::ElementDescriptor>, String> {
        crate::view::renderer_adapter::walk_children_descriptors(node, path, global_path, inherited)
    }

    /// 軌 1 #11: dispatch a single changed prop to this host. Each host
    /// owns its own prop registry — fiber_work routes `(name, value)`
    /// pairs straight here without per-host helper functions.
    ///
    /// Default impl returns `UnknownProp` so leaf hosts that genuinely
    /// don't accept any incremental prop are no-ops; the caller logs
    /// and continues without falling back to cold rebuild.
    fn apply_prop(
        &mut self,
        _arena: &mut crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
        _ctx: &crate::view::fiber_work::ApplyContext<'_>,
        _name: &'static str,
        _value: crate::ui::PropValue,
    ) -> crate::view::fiber_work::PropApplyOutcome {
        crate::view::fiber_work::PropApplyOutcome::UnknownProp
    }

    /// 軌 1 #11: reset a removed prop to its cold-path default. Hosts
    /// only override for props whose removal semantics they actually
    /// model; the default `CannotReset` is a soft no-op (caller logs).
    fn reset_prop(
        &mut self,
        _arena: &mut crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
        _ctx: &crate::view::fiber_work::ApplyContext<'_>,
        name: &'static str,
    ) -> crate::view::fiber_work::PropApplyOutcome {
        crate::view::fiber_work::PropApplyOutcome::CannotReset(name)
    }
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
    #[allow(dead_code)]
    pub border_top_rgba: [u8; 4],
    #[allow(dead_code)]
    pub border_right_rgba: [u8; 4],
    #[allow(dead_code)]
    pub border_bottom_rgba: [u8; 4],
    #[allow(dead_code)]
    pub border_left_rgba: [u8; 4],
    pub opacity: f32,
    pub border_radius: f32,
}

type PointerDownHandler = Box<dyn FnMut(&mut PointerDownEvent, &mut ViewportControl<'_>)>;
type PointerUpHandler = Box<dyn FnMut(&mut PointerUpEvent, &mut ViewportControl<'_>)>;
type PointerMoveHandler = Box<dyn FnMut(&mut PointerMoveEvent, &mut ViewportControl<'_>)>;
type PointerEnterHandler = Box<dyn FnMut(&mut PointerEnterEvent)>;
type PointerLeaveHandler = Box<dyn FnMut(&mut PointerLeaveEvent)>;
type ClickHandler = Box<dyn FnMut(&mut ClickEvent, &mut ViewportControl<'_>)>;
type ContextMenuHandler =
    Box<dyn FnMut(&mut crate::ui::ContextMenuEvent, &mut ViewportControl<'_>)>;
type WheelHandler = Box<dyn FnMut(&mut crate::ui::WheelEvent, &mut ViewportControl<'_>)>;
type KeyDownHandler = Box<dyn FnMut(&mut KeyDownEvent, &mut ViewportControl<'_>)>;
type KeyUpHandler = Box<dyn FnMut(&mut KeyUpEvent, &mut ViewportControl<'_>)>;
type FocusHandler = Box<dyn FnMut(&mut FocusEvent, &mut ViewportControl<'_>)>;
type BlurHandler = Box<dyn FnMut(&mut BlurEvent, &mut ViewportControl<'_>)>;
type ImeCommitHandler = Box<dyn FnMut(&mut crate::ui::ImeCommitEvent, &mut ViewportControl<'_>)>;
type ImeEnabledHandler = Box<dyn FnMut(&mut crate::ui::ImeEnabledEvent, &mut ViewportControl<'_>)>;
type ImeDisabledHandler =
    Box<dyn FnMut(&mut crate::ui::ImeDisabledEvent, &mut ViewportControl<'_>)>;
type DragStartHandler = Box<dyn FnMut(&mut crate::ui::DragStartEvent, &mut ViewportControl<'_>)>;
type DragOverHandler = Box<dyn FnMut(&mut crate::ui::DragOverEvent, &mut ViewportControl<'_>)>;
type DragLeaveHandler = Box<dyn FnMut(&mut crate::ui::DragLeaveEvent, &mut ViewportControl<'_>)>;
type DropHandler = Box<dyn FnMut(&mut crate::ui::DropEvent, &mut ViewportControl<'_>)>;
type DragEndHandler = Box<dyn FnMut(&mut crate::ui::DragEndEvent, &mut ViewportControl<'_>)>;
type CopyHandler = Box<dyn FnMut(&mut crate::ui::CopyEvent, &mut ViewportControl<'_>)>;
type CutHandler = Box<dyn FnMut(&mut crate::ui::CutEvent, &mut ViewportControl<'_>)>;
type PasteHandler = Box<dyn FnMut(&mut crate::ui::PasteEvent, &mut ViewportControl<'_>)>;

/// Cold-path storage for event handlers. Boxed and lazily allocated so that
/// elements without handlers pay only 8 bytes (the `Option<Box<_>>` pointer).
#[derive(Default)]
struct ElementEventHandlers {
    pointer_down: Vec<PointerDownHandler>,
    pointer_up: Vec<PointerUpHandler>,
    pointer_move: Vec<PointerMoveHandler>,
    pointer_enter: Vec<PointerEnterHandler>,
    pointer_leave: Vec<PointerLeaveHandler>,
    click: Vec<ClickHandler>,
    context_menu: Vec<ContextMenuHandler>,
    wheel: Vec<WheelHandler>,
    key_down: Vec<KeyDownHandler>,
    key_up: Vec<KeyUpHandler>,
    focus: Vec<FocusHandler>,
    blur: Vec<BlurHandler>,
    ime_commit: Vec<ImeCommitHandler>,
    ime_enabled: Vec<ImeEnabledHandler>,
    ime_disabled: Vec<ImeDisabledHandler>,
    drag_start: Vec<DragStartHandler>,
    drag_over: Vec<DragOverHandler>,
    drag_leave: Vec<DragLeaveHandler>,
    drop: Vec<DropHandler>,
    drag_end: Vec<DragEndHandler>,
    copy: Vec<CopyHandler>,
    cut: Vec<CutHandler>,
    paste: Vec<PasteHandler>,
}

/// Cold-path storage for pending transition/animation requests. Boxed and
/// lazily allocated so that elements without active transitions pay only
/// 8 bytes.
#[derive(Default)]
struct ElementTransitionRequests {
    style: Vec<StyleTrackRequest>,
    animation: Vec<AnimationRequest>,
    layout: Vec<LayoutTrackRequest>,
    visual: Vec<VisualTrackRequest>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ElementInlineIfcMetadataCollectorInput {
    pub(crate) root_key: NodeKey,
    pub(crate) max_width: f32,
}

#[allow(dead_code)]
impl ElementInlineIfcMetadataCollectorInput {
    pub(crate) fn new(root_key: NodeKey, max_width: f32) -> Self {
        Self {
            root_key,
            max_width: max_width.max(1.0),
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ElementInlineIfcMetadataCollectorOutput {
    pub(crate) root_source: InlineIfcElementRootSource,
    pub(crate) sources_by_node: FxHashMap<NodeKey, InlineIfcSourceId>,
}

#[allow(dead_code)]
impl ElementInlineIfcMetadataCollectorOutput {
    pub(crate) fn source_for_node(&self, key: NodeKey) -> Option<InlineIfcSourceId> {
        self.sources_by_node.get(&key).copied()
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ElementInlineIfcCandidateLifecycleInput {
    pub(crate) root_key: NodeKey,
    pub(crate) max_width: f32,
    pub(crate) install_targets: Vec<NodeKey>,
}

#[allow(dead_code)]
impl ElementInlineIfcCandidateLifecycleInput {
    pub(crate) fn new(root_key: NodeKey, max_width: f32) -> Self {
        Self {
            root_key,
            max_width: max_width.max(1.0),
            install_targets: Vec::new(),
        }
    }

    pub(crate) fn with_install_targets(mut self, install_targets: Vec<NodeKey>) -> Self {
        self.install_targets = install_targets;
        self
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElementInlineIfcCandidateLifecycleInstallStatus {
    ObservedOnly,
    Installed,
    ClearedMissingSource,
    SkippedNonElement,
    MissingNode,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ElementInlineIfcCandidateLifecycleInstall {
    pub(crate) node_key: NodeKey,
    pub(crate) source: Option<InlineIfcSourceId>,
    pub(crate) status: ElementInlineIfcCandidateLifecycleInstallStatus,
    pub(crate) has_decoration_package: bool,
    pub(crate) has_atomic_package: bool,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ElementInlineIfcCandidateLifecycleOutput {
    pub(crate) cache_key: InlineIfcCacheKey,
    pub(crate) invalidation: InlineIfcInvalidation,
    pub(crate) rebuilt: bool,
    pub(crate) cache_len: usize,
    pub(crate) sources_by_node: FxHashMap<NodeKey, InlineIfcSourceId>,
    pub(crate) installs: Vec<ElementInlineIfcCandidateLifecycleInstall>,
}

#[allow(dead_code)]
impl ElementInlineIfcCandidateLifecycleOutput {
    pub(crate) fn source_for_node(&self, key: NodeKey) -> Option<InlineIfcSourceId> {
        self.sources_by_node.get(&key).copied()
    }

    pub(crate) fn install_for_node(
        &self,
        key: NodeKey,
    ) -> Option<&ElementInlineIfcCandidateLifecycleInstall> {
        self.installs.iter().find(|install| install.node_key == key)
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ElementInlineIfcLayoutCallSiteOptInMode {
    #[default]
    Disabled,
    ShadowObservation,
    DryRunCandidate,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ElementInlineIfcLayoutCallSiteGate {
    requested_mode: ElementInlineIfcLayoutCallSiteOptInMode,
}

#[allow(dead_code)]
impl ElementInlineIfcLayoutCallSiteGate {
    pub(crate) fn disabled() -> Self {
        Self {
            requested_mode: ElementInlineIfcLayoutCallSiteOptInMode::Disabled,
        }
    }

    pub(crate) fn explicit_dry_run_candidate() -> Self {
        Self {
            requested_mode: ElementInlineIfcLayoutCallSiteOptInMode::DryRunCandidate,
        }
    }

    pub(crate) fn explicit_shadow_observation() -> Self {
        Self {
            requested_mode: ElementInlineIfcLayoutCallSiteOptInMode::ShadowObservation,
        }
    }

    pub(crate) fn resolve(self) -> ElementInlineIfcLayoutCallSiteOptInMode {
        match self.requested_mode {
            ElementInlineIfcLayoutCallSiteOptInMode::Disabled => {
                ElementInlineIfcLayoutCallSiteOptInMode::Disabled
            }
            ElementInlineIfcLayoutCallSiteOptInMode::ShadowObservation => {
                ElementInlineIfcLayoutCallSiteOptInMode::ShadowObservation
            }
            ElementInlineIfcLayoutCallSiteOptInMode::DryRunCandidate => {
                ElementInlineIfcLayoutCallSiteOptInMode::DryRunCandidate
            }
        }
    }

    pub(crate) fn is_enabled(self) -> bool {
        matches!(
            self.resolve(),
            ElementInlineIfcLayoutCallSiteOptInMode::ShadowObservation
                | ElementInlineIfcLayoutCallSiteOptInMode::DryRunCandidate
        )
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ElementInlineIfcLayoutCallSiteRolloutPhase {
    #[default]
    Disabled,
    ProductionDefaultShadowRun,
    ControlledInstalledPackageCandidate,
    ExplicitDryRunCandidate,
}

#[allow(dead_code)]
impl ElementInlineIfcLayoutCallSiteRolloutPhase {
    pub(crate) fn mode(self) -> ElementInlineIfcLayoutCallSiteOptInMode {
        match self {
            Self::Disabled => ElementInlineIfcLayoutCallSiteOptInMode::Disabled,
            Self::ProductionDefaultShadowRun => {
                ElementInlineIfcLayoutCallSiteOptInMode::ShadowObservation
            }
            Self::ControlledInstalledPackageCandidate => {
                ElementInlineIfcLayoutCallSiteOptInMode::DryRunCandidate
            }
            Self::ExplicitDryRunCandidate => {
                ElementInlineIfcLayoutCallSiteOptInMode::DryRunCandidate
            }
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ElementInlineIfcLayoutCallSiteRolloutConfig {
    phase: ElementInlineIfcLayoutCallSiteRolloutPhase,
}

impl Default for ElementInlineIfcLayoutCallSiteRolloutConfig {
    fn default() -> Self {
        Self::production_default_shadow_run_phase()
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElementInlineIfcDefaultRolloutBlockedReason {
    RenderGateNotIndependent,
    LegacyFallbackMissing,
    UnsupportedRootAndTextAreaBoundaryUnconfirmed,
    InvalidationGuardUnconfirmed,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ElementInlineIfcDefaultRolloutDecisionInput {
    pub(crate) render_gate_independent: bool,
    pub(crate) legacy_fallback_available: bool,
    pub(crate) unsupported_root_and_text_area_boundary_confirmed: bool,
    pub(crate) invalidation_guard_confirmed: bool,
}

#[allow(dead_code)]
impl ElementInlineIfcDefaultRolloutDecisionInput {
    pub(crate) fn checklist_passed() -> Self {
        Self {
            render_gate_independent: true,
            legacy_fallback_available: true,
            unsupported_root_and_text_area_boundary_confirmed: true,
            invalidation_guard_confirmed: true,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ElementInlineIfcDefaultRolloutDecision {
    recommended_phase: ElementInlineIfcLayoutCallSiteRolloutPhase,
    blocked_reasons: Vec<ElementInlineIfcDefaultRolloutBlockedReason>,
}

#[allow(dead_code)]
impl ElementInlineIfcDefaultRolloutDecision {
    pub(crate) fn evaluate(input: ElementInlineIfcDefaultRolloutDecisionInput) -> Self {
        let mut blocked_reasons = Vec::new();
        if !input.render_gate_independent {
            blocked_reasons
                .push(ElementInlineIfcDefaultRolloutBlockedReason::RenderGateNotIndependent);
        }
        if !input.legacy_fallback_available {
            blocked_reasons
                .push(ElementInlineIfcDefaultRolloutBlockedReason::LegacyFallbackMissing);
        }
        if !input.unsupported_root_and_text_area_boundary_confirmed {
            blocked_reasons.push(
                ElementInlineIfcDefaultRolloutBlockedReason::
                    UnsupportedRootAndTextAreaBoundaryUnconfirmed,
            );
        }
        if !input.invalidation_guard_confirmed {
            blocked_reasons
                .push(ElementInlineIfcDefaultRolloutBlockedReason::InvalidationGuardUnconfirmed);
        }

        let recommended_phase = if blocked_reasons.is_empty() {
            ElementInlineIfcLayoutCallSiteRolloutPhase::ProductionDefaultShadowRun
        } else {
            ElementInlineIfcLayoutCallSiteRolloutPhase::Disabled
        };
        Self {
            recommended_phase,
            blocked_reasons,
        }
    }

    pub(crate) fn is_allowed(&self) -> bool {
        self.blocked_reasons.is_empty()
    }

    pub(crate) fn recommended_phase(&self) -> ElementInlineIfcLayoutCallSiteRolloutPhase {
        self.recommended_phase
    }

    pub(crate) fn recommended_config(&self) -> ElementInlineIfcLayoutCallSiteRolloutConfig {
        ElementInlineIfcLayoutCallSiteRolloutConfig {
            phase: self.recommended_phase,
        }
    }

    pub(crate) fn blocked_reasons(&self) -> &[ElementInlineIfcDefaultRolloutBlockedReason] {
        &self.blocked_reasons
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElementInlineIfcDefaultShadowRunAuditBlockedReason {
    DecisionBlocked,
    ProductionDefaultNotShadowOnlyObservation,
    ShadowObservationDiagnosticMissing,
    ShadowObservationInstalledPackages,
    LegacyFallbackNotObserved,
    RenderGateExplicitnessUnobserved,
    UnsupportedOrNonInlineNoOpUnobserved,
    TextAreaBoundaryUnobserved,
    MatrixInvalidationGuardUnobserved,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ElementInlineIfcDefaultShadowRunAdoptionAuditInput {
    pub(crate) decision: ElementInlineIfcDefaultRolloutDecision,
    pub(crate) production_default_config: ElementInlineIfcLayoutCallSiteRolloutConfig,
    pub(crate) shadow_observation_diagnostic_observed: bool,
    pub(crate) shadow_observation_installed_packages: bool,
    pub(crate) legacy_fallback_observed: bool,
    pub(crate) render_gate_explicit_observed: bool,
    pub(crate) unsupported_or_non_inline_no_op_observed: bool,
    pub(crate) text_area_boundary_observed: bool,
    pub(crate) matrix_invalidation_guard_observed: bool,
}

#[allow(dead_code)]
impl ElementInlineIfcDefaultShadowRunAdoptionAuditInput {
    pub(crate) fn with_confirmed_observations(
        decision: ElementInlineIfcDefaultRolloutDecision,
    ) -> Self {
        Self {
            decision,
            production_default_config: ElementInlineIfcLayoutCallSiteRolloutConfig::default(),
            shadow_observation_diagnostic_observed: true,
            shadow_observation_installed_packages: false,
            legacy_fallback_observed: true,
            render_gate_explicit_observed: true,
            unsupported_or_non_inline_no_op_observed: true,
            text_area_boundary_observed: true,
            matrix_invalidation_guard_observed: true,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElementInlineIfcDefaultShadowRunAuditReadiness {
    Blocked,
    ReadyForShadowOnlyObservation,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ElementInlineIfcDefaultShadowRunAdoptionAudit {
    readiness: ElementInlineIfcDefaultShadowRunAuditReadiness,
    recommended_config: ElementInlineIfcLayoutCallSiteRolloutConfig,
    blocked_reasons: Vec<ElementInlineIfcDefaultShadowRunAuditBlockedReason>,
}

#[allow(dead_code)]
impl ElementInlineIfcDefaultShadowRunAdoptionAudit {
    pub(crate) fn evaluate(input: ElementInlineIfcDefaultShadowRunAdoptionAuditInput) -> Self {
        let mut blocked_reasons = Vec::new();
        if !input.decision.is_allowed()
            || input.decision.recommended_phase()
                != ElementInlineIfcLayoutCallSiteRolloutPhase::ProductionDefaultShadowRun
        {
            blocked_reasons
                .push(ElementInlineIfcDefaultShadowRunAuditBlockedReason::DecisionBlocked);
        }
        if input.production_default_config.phase()
            != ElementInlineIfcLayoutCallSiteRolloutPhase::ProductionDefaultShadowRun
        {
            blocked_reasons.push(
                ElementInlineIfcDefaultShadowRunAuditBlockedReason::
                    ProductionDefaultNotShadowOnlyObservation,
            );
        }
        if !input.shadow_observation_diagnostic_observed {
            blocked_reasons.push(
                ElementInlineIfcDefaultShadowRunAuditBlockedReason::
                    ShadowObservationDiagnosticMissing,
            );
        }
        if input.shadow_observation_installed_packages {
            blocked_reasons.push(
                ElementInlineIfcDefaultShadowRunAuditBlockedReason::
                    ShadowObservationInstalledPackages,
            );
        }
        if !input.legacy_fallback_observed {
            blocked_reasons.push(
                ElementInlineIfcDefaultShadowRunAuditBlockedReason::LegacyFallbackNotObserved,
            );
        }
        if !input.render_gate_explicit_observed {
            blocked_reasons.push(
                ElementInlineIfcDefaultShadowRunAuditBlockedReason::
                    RenderGateExplicitnessUnobserved,
            );
        }
        if !input.unsupported_or_non_inline_no_op_observed {
            blocked_reasons.push(
                ElementInlineIfcDefaultShadowRunAuditBlockedReason::
                    UnsupportedOrNonInlineNoOpUnobserved,
            );
        }
        if !input.text_area_boundary_observed {
            blocked_reasons.push(
                ElementInlineIfcDefaultShadowRunAuditBlockedReason::TextAreaBoundaryUnobserved,
            );
        }
        if !input.matrix_invalidation_guard_observed {
            blocked_reasons.push(
                ElementInlineIfcDefaultShadowRunAuditBlockedReason::
                    MatrixInvalidationGuardUnobserved,
            );
        }

        let readiness = if blocked_reasons.is_empty() {
            ElementInlineIfcDefaultShadowRunAuditReadiness::ReadyForShadowOnlyObservation
        } else {
            ElementInlineIfcDefaultShadowRunAuditReadiness::Blocked
        };
        let recommended_config = if blocked_reasons.is_empty() {
            input.decision.recommended_config()
        } else {
            ElementInlineIfcLayoutCallSiteRolloutConfig::disabled()
        };
        Self {
            readiness,
            recommended_config,
            blocked_reasons,
        }
    }

    pub(crate) fn readiness(&self) -> ElementInlineIfcDefaultShadowRunAuditReadiness {
        self.readiness
    }

    pub(crate) fn is_ready_for_shadow_only_observation(&self) -> bool {
        matches!(
            self.readiness,
            ElementInlineIfcDefaultShadowRunAuditReadiness::ReadyForShadowOnlyObservation
        )
    }

    pub(crate) fn recommended_config(&self) -> ElementInlineIfcLayoutCallSiteRolloutConfig {
        self.recommended_config
    }

    pub(crate) fn blocked_reasons(&self) -> &[ElementInlineIfcDefaultShadowRunAuditBlockedReason] {
        &self.blocked_reasons
    }

    pub(crate) fn allows_render_candidate_default(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElementInlineIfcRenderDefaultAuditBlockedReason {
    ShadowRunAuditNotReady,
    RenderDefaultAlreadyCandidate,
    InstalledPackageLifecycleUnconfirmed,
    LegacyFallbackUnconfirmed,
    UnsupportedOrNonInlineBoundaryUnconfirmed,
    TextAreaBoundaryUnconfirmed,
    RollbackDisabledPathUnconfirmed,
    ExplicitRenderOptInUnconfirmed,
    MissingInstalledPackageFallbackUnconfirmed,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ElementInlineIfcRenderDefaultAuditInput {
    pub(crate) shadow_run_audit: ElementInlineIfcDefaultShadowRunAdoptionAudit,
    pub(crate) current_render_default: ElementInlineIfcRenderMode,
    pub(crate) installed_package_lifecycle_confirmed: bool,
    pub(crate) legacy_fallback_confirmed: bool,
    pub(crate) unsupported_or_non_inline_boundary_confirmed: bool,
    pub(crate) text_area_boundary_confirmed: bool,
    pub(crate) rollback_disabled_path_confirmed: bool,
    pub(crate) explicit_render_opt_in_confirmed: bool,
    pub(crate) missing_installed_package_fallback_confirmed: bool,
}

#[allow(dead_code)]
impl ElementInlineIfcRenderDefaultAuditInput {
    pub(crate) fn with_confirmed_observations(
        shadow_run_audit: ElementInlineIfcDefaultShadowRunAdoptionAudit,
    ) -> Self {
        Self {
            shadow_run_audit,
            current_render_default: ElementInlineIfcRenderMode::Disabled,
            installed_package_lifecycle_confirmed: true,
            legacy_fallback_confirmed: true,
            unsupported_or_non_inline_boundary_confirmed: true,
            text_area_boundary_confirmed: true,
            rollback_disabled_path_confirmed: true,
            explicit_render_opt_in_confirmed: true,
            missing_installed_package_fallback_confirmed: true,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElementInlineIfcRenderDefaultAuditReadiness {
    Blocked,
    ReadyForExplicitRenderCandidateEvaluation,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ElementInlineIfcRenderDefaultAudit {
    readiness: ElementInlineIfcRenderDefaultAuditReadiness,
    explicit_candidate_evaluation_mode: Option<ElementInlineIfcRenderMode>,
    blocked_reasons: Vec<ElementInlineIfcRenderDefaultAuditBlockedReason>,
}

#[allow(dead_code)]
impl ElementInlineIfcRenderDefaultAudit {
    pub(crate) fn evaluate(input: ElementInlineIfcRenderDefaultAuditInput) -> Self {
        let mut blocked_reasons = Vec::new();
        if !input
            .shadow_run_audit
            .is_ready_for_shadow_only_observation()
        {
            blocked_reasons
                .push(ElementInlineIfcRenderDefaultAuditBlockedReason::ShadowRunAuditNotReady);
        }
        if input.current_render_default != ElementInlineIfcRenderMode::Disabled {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultAuditBlockedReason::RenderDefaultAlreadyCandidate,
            );
        }
        if !input.installed_package_lifecycle_confirmed {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultAuditBlockedReason::
                    InstalledPackageLifecycleUnconfirmed,
            );
        }
        if !input.legacy_fallback_confirmed {
            blocked_reasons
                .push(ElementInlineIfcRenderDefaultAuditBlockedReason::LegacyFallbackUnconfirmed);
        }
        if !input.unsupported_or_non_inline_boundary_confirmed {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultAuditBlockedReason::
                    UnsupportedOrNonInlineBoundaryUnconfirmed,
            );
        }
        if !input.text_area_boundary_confirmed {
            blocked_reasons
                .push(ElementInlineIfcRenderDefaultAuditBlockedReason::TextAreaBoundaryUnconfirmed);
        }
        if !input.rollback_disabled_path_confirmed {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultAuditBlockedReason::RollbackDisabledPathUnconfirmed,
            );
        }
        if !input.explicit_render_opt_in_confirmed {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultAuditBlockedReason::ExplicitRenderOptInUnconfirmed,
            );
        }
        if !input.missing_installed_package_fallback_confirmed {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultAuditBlockedReason::
                    MissingInstalledPackageFallbackUnconfirmed,
            );
        }

        let readiness = if blocked_reasons.is_empty() {
            ElementInlineIfcRenderDefaultAuditReadiness::ReadyForExplicitRenderCandidateEvaluation
        } else {
            ElementInlineIfcRenderDefaultAuditReadiness::Blocked
        };
        let explicit_candidate_evaluation_mode = if blocked_reasons.is_empty() {
            Some(ElementInlineIfcRenderMode::DrawRectPackageCandidate)
        } else {
            None
        };
        Self {
            readiness,
            explicit_candidate_evaluation_mode,
            blocked_reasons,
        }
    }

    pub(crate) fn readiness(&self) -> ElementInlineIfcRenderDefaultAuditReadiness {
        self.readiness
    }

    pub(crate) fn is_ready_for_explicit_render_candidate_evaluation(&self) -> bool {
        matches!(
            self.readiness,
            ElementInlineIfcRenderDefaultAuditReadiness::ReadyForExplicitRenderCandidateEvaluation
        )
    }

    pub(crate) fn explicit_candidate_evaluation_mode(&self) -> Option<ElementInlineIfcRenderMode> {
        self.explicit_candidate_evaluation_mode
    }

    pub(crate) fn blocked_reasons(&self) -> &[ElementInlineIfcRenderDefaultAuditBlockedReason] {
        &self.blocked_reasons
    }

    pub(crate) fn allows_render_candidate_default(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElementInlineIfcRenderDefaultRolloutBlockedReason {
    RenderAuditNotReady,
    RenderDefaultAlreadyCandidate,
    ExplicitCandidateEvaluationUnobserved,
    InstalledPackageCandidateUnobserved,
    LegacyDefaultDecisionChanged,
    MissingInstalledPackageFallbackUnobserved,
    RollbackDisabledPathUnconfirmed,
    TextAreaBoundaryUnconfirmed,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ElementInlineIfcRenderDefaultRolloutDecisionInput {
    pub(crate) render_audit: ElementInlineIfcRenderDefaultAudit,
    pub(crate) current_render_default: ElementInlineIfcRenderMode,
    pub(crate) current_default_render_decision: ElementInlineIfcRenderDecision,
    pub(crate) explicit_candidate_evaluation_observed: bool,
    pub(crate) controlled_installed_package_candidate_observed: bool,
    pub(crate) missing_installed_package_fallback_observed: bool,
    pub(crate) rollback_disabled_path_confirmed: bool,
    pub(crate) text_area_boundary_confirmed: bool,
}

#[allow(dead_code)]
impl ElementInlineIfcRenderDefaultRolloutDecisionInput {
    pub(crate) fn with_confirmed_observations(
        render_audit: ElementInlineIfcRenderDefaultAudit,
    ) -> Self {
        Self {
            render_audit,
            current_render_default: ElementInlineIfcRenderMode::Disabled,
            current_default_render_decision:
                ElementInlineIfcRenderDecision::ExistingInlineFragments,
            explicit_candidate_evaluation_observed: true,
            controlled_installed_package_candidate_observed: true,
            missing_installed_package_fallback_observed: true,
            rollback_disabled_path_confirmed: true,
            text_area_boundary_confirmed: true,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElementInlineIfcRenderDefaultRolloutReadiness {
    Blocked,
    ReadyForControlledInstalledPackageCandidate,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ElementInlineIfcRenderDefaultRolloutDecision {
    readiness: ElementInlineIfcRenderDefaultRolloutReadiness,
    explicit_installed_package_candidate_mode: Option<ElementInlineIfcRenderMode>,
    blocked_reasons: Vec<ElementInlineIfcRenderDefaultRolloutBlockedReason>,
}

#[allow(dead_code)]
impl ElementInlineIfcRenderDefaultRolloutDecision {
    pub(crate) fn evaluate(input: ElementInlineIfcRenderDefaultRolloutDecisionInput) -> Self {
        let mut blocked_reasons = Vec::new();
        if !input
            .render_audit
            .is_ready_for_explicit_render_candidate_evaluation()
        {
            blocked_reasons
                .push(ElementInlineIfcRenderDefaultRolloutBlockedReason::RenderAuditNotReady);
        }
        if input.current_render_default != ElementInlineIfcRenderMode::Disabled {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultRolloutBlockedReason::RenderDefaultAlreadyCandidate,
            );
        }
        if !input.explicit_candidate_evaluation_observed {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultRolloutBlockedReason::
                    ExplicitCandidateEvaluationUnobserved,
            );
        }
        if !input.controlled_installed_package_candidate_observed {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultRolloutBlockedReason::
                    InstalledPackageCandidateUnobserved,
            );
        }
        if input.current_default_render_decision
            != ElementInlineIfcRenderDecision::ExistingInlineFragments
        {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultRolloutBlockedReason::LegacyDefaultDecisionChanged,
            );
        }
        if !input.missing_installed_package_fallback_observed {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultRolloutBlockedReason::
                    MissingInstalledPackageFallbackUnobserved,
            );
        }
        if !input.rollback_disabled_path_confirmed {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultRolloutBlockedReason::RollbackDisabledPathUnconfirmed,
            );
        }
        if !input.text_area_boundary_confirmed {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultRolloutBlockedReason::TextAreaBoundaryUnconfirmed,
            );
        }

        let readiness = if blocked_reasons.is_empty() {
            ElementInlineIfcRenderDefaultRolloutReadiness::
                ReadyForControlledInstalledPackageCandidate
        } else {
            ElementInlineIfcRenderDefaultRolloutReadiness::Blocked
        };
        let explicit_installed_package_candidate_mode = if blocked_reasons.is_empty() {
            Some(ElementInlineIfcRenderMode::DrawRectPackageCandidate)
        } else {
            None
        };
        Self {
            readiness,
            explicit_installed_package_candidate_mode,
            blocked_reasons,
        }
    }

    pub(crate) fn readiness(&self) -> ElementInlineIfcRenderDefaultRolloutReadiness {
        self.readiness
    }

    pub(crate) fn is_ready_for_controlled_installed_package_candidate(&self) -> bool {
        matches!(
            self.readiness,
            ElementInlineIfcRenderDefaultRolloutReadiness::
                ReadyForControlledInstalledPackageCandidate
        )
    }

    pub(crate) fn explicit_installed_package_candidate_mode(
        &self,
    ) -> Option<ElementInlineIfcRenderMode> {
        self.explicit_installed_package_candidate_mode
    }

    pub(crate) fn controlled_installed_package_candidate_config(
        &self,
    ) -> Option<ElementInlineIfcLayoutCallSiteRolloutConfig> {
        self.is_ready_for_controlled_installed_package_candidate()
            .then(
                ElementInlineIfcLayoutCallSiteRolloutConfig::controlled_installed_package_candidate,
            )
    }

    pub(crate) fn blocked_reasons(&self) -> &[ElementInlineIfcRenderDefaultRolloutBlockedReason] {
        &self.blocked_reasons
    }

    pub(crate) fn allows_render_candidate_default(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason {
    RolloutDecisionNotReady,
    ControlledInstalledPackageCandidateMissing,
    ControlledInstalledPackageDiagnosticMissing,
    DefaultPathPackageUnavailable,
    MissingInstalledPackageFallbackUnconfirmed,
    DisabledRollbackUnconfirmed,
    UnsupportedOrNonInlineNoOpUnconfirmed,
    TextAreaBoundaryUnconfirmed,
    LegacyFallbackUnconfirmed,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ElementInlineIfcRenderDefaultAdoptionAuditInput {
    pub(crate) rollout_decision: ElementInlineIfcRenderDefaultRolloutDecision,
    pub(crate) controlled_installed_package_candidate_observed: bool,
    pub(crate) controlled_installed_package_diagnostic_observed: bool,
    pub(crate) default_path_package_available: bool,
    pub(crate) missing_installed_package_fallback_confirmed: bool,
    pub(crate) disabled_rollback_confirmed: bool,
    pub(crate) unsupported_or_non_inline_no_op_confirmed: bool,
    pub(crate) text_area_boundary_confirmed: bool,
    pub(crate) legacy_fallback_confirmed: bool,
}

#[allow(dead_code)]
impl ElementInlineIfcRenderDefaultAdoptionAuditInput {
    pub(crate) fn with_confirmed_observations(
        rollout_decision: ElementInlineIfcRenderDefaultRolloutDecision,
    ) -> Self {
        Self {
            rollout_decision,
            controlled_installed_package_candidate_observed: true,
            controlled_installed_package_diagnostic_observed: true,
            default_path_package_available: true,
            missing_installed_package_fallback_confirmed: true,
            disabled_rollback_confirmed: true,
            unsupported_or_non_inline_no_op_confirmed: true,
            text_area_boundary_confirmed: true,
            legacy_fallback_confirmed: true,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElementInlineIfcRenderDefaultAdoptionAuditReadiness {
    Blocked,
    ReadyForInlineElementRenderDefault,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ElementInlineIfcRenderDefaultAdoptionAudit {
    readiness: ElementInlineIfcRenderDefaultAdoptionAuditReadiness,
    recommended_default_mode: Option<ElementInlineIfcRenderMode>,
    blocked_reasons: Vec<ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason>,
}

#[allow(dead_code)]
impl ElementInlineIfcRenderDefaultAdoptionAudit {
    pub(crate) fn evaluate(input: ElementInlineIfcRenderDefaultAdoptionAuditInput) -> Self {
        let mut blocked_reasons = Vec::new();
        if !input
            .rollout_decision
            .is_ready_for_controlled_installed_package_candidate()
        {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason::RolloutDecisionNotReady,
            );
        }
        if !input.controlled_installed_package_candidate_observed {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason::
                    ControlledInstalledPackageCandidateMissing,
            );
        }
        if !input.controlled_installed_package_diagnostic_observed {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason::
                    ControlledInstalledPackageDiagnosticMissing,
            );
        }
        if !input.default_path_package_available {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason::
                    DefaultPathPackageUnavailable,
            );
        }
        if !input.missing_installed_package_fallback_confirmed {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason::
                    MissingInstalledPackageFallbackUnconfirmed,
            );
        }
        if !input.disabled_rollback_confirmed {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason::DisabledRollbackUnconfirmed,
            );
        }
        if !input.unsupported_or_non_inline_no_op_confirmed {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason::
                    UnsupportedOrNonInlineNoOpUnconfirmed,
            );
        }
        if !input.text_area_boundary_confirmed {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason::TextAreaBoundaryUnconfirmed,
            );
        }
        if !input.legacy_fallback_confirmed {
            blocked_reasons.push(
                ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason::LegacyFallbackUnconfirmed,
            );
        }

        let readiness = if blocked_reasons.is_empty() {
            ElementInlineIfcRenderDefaultAdoptionAuditReadiness::ReadyForInlineElementRenderDefault
        } else {
            ElementInlineIfcRenderDefaultAdoptionAuditReadiness::Blocked
        };
        let recommended_default_mode = if blocked_reasons.is_empty() {
            Some(ElementInlineIfcRenderMode::DrawRectPackageCandidate)
        } else {
            None
        };
        Self {
            readiness,
            recommended_default_mode,
            blocked_reasons,
        }
    }

    pub(crate) fn readiness(&self) -> ElementInlineIfcRenderDefaultAdoptionAuditReadiness {
        self.readiness
    }

    pub(crate) fn is_ready_for_inline_element_render_default(&self) -> bool {
        matches!(
            self.readiness,
            ElementInlineIfcRenderDefaultAdoptionAuditReadiness::ReadyForInlineElementRenderDefault
        )
    }

    pub(crate) fn recommended_default_mode(&self) -> Option<ElementInlineIfcRenderMode> {
        self.recommended_default_mode
    }

    pub(crate) fn blocked_reasons(
        &self,
    ) -> &[ElementInlineIfcRenderDefaultAdoptionAuditBlockedReason] {
        &self.blocked_reasons
    }

    pub(crate) fn allows_render_candidate_default(&self) -> bool {
        self.is_ready_for_inline_element_render_default()
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaInlineIfcReadinessBlockedReason {
    EditableIfcPathUnwired,
    ProjectionIfcPathUnwired,
    ImeIfcPathUnwired,
    CaretAffinityIfcPathUnwired,
    ScrollFollowIfcPathUnwired,
    TextAreaTextRunBoundaryUnconfirmed,
    InlineElementRolloutBoundaryUnconfirmed,
    ReadOnlyTextPreparedPathSeparationUnconfirmed,
    LegacyFallbackUnconfirmed,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TextAreaInlineIfcReadinessInput {
    pub(crate) editable_ifc_path_wired: bool,
    pub(crate) projection_ifc_path_wired: bool,
    pub(crate) ime_ifc_path_wired: bool,
    pub(crate) caret_affinity_ifc_path_wired: bool,
    pub(crate) scroll_follow_ifc_path_wired: bool,
    pub(crate) text_area_text_run_boundary_confirmed: bool,
    pub(crate) inline_element_rollout_boundary_confirmed: bool,
    pub(crate) read_only_text_prepared_path_separated: bool,
    pub(crate) legacy_fallback_confirmed: bool,
}

#[allow(dead_code)]
impl TextAreaInlineIfcReadinessInput {
    pub(crate) fn current_p7_preflight_observations() -> Self {
        Self {
            editable_ifc_path_wired: false,
            projection_ifc_path_wired: false,
            ime_ifc_path_wired: false,
            caret_affinity_ifc_path_wired: false,
            scroll_follow_ifc_path_wired: false,
            text_area_text_run_boundary_confirmed: true,
            inline_element_rollout_boundary_confirmed: true,
            read_only_text_prepared_path_separated: true,
            legacy_fallback_confirmed: true,
        }
    }

    pub(crate) fn with_all_ifc_paths_wired() -> Self {
        Self {
            editable_ifc_path_wired: true,
            projection_ifc_path_wired: true,
            ime_ifc_path_wired: true,
            caret_affinity_ifc_path_wired: true,
            scroll_follow_ifc_path_wired: true,
            text_area_text_run_boundary_confirmed: true,
            inline_element_rollout_boundary_confirmed: true,
            read_only_text_prepared_path_separated: true,
            legacy_fallback_confirmed: true,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaInlineIfcReadinessState {
    Blocked,
    ReadyForEditableIfcEvaluation,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextAreaInlineIfcReadiness {
    readiness: TextAreaInlineIfcReadinessState,
    blocked_reasons: Vec<TextAreaInlineIfcReadinessBlockedReason>,
}

#[allow(dead_code)]
impl TextAreaInlineIfcReadiness {
    pub(crate) fn evaluate(input: TextAreaInlineIfcReadinessInput) -> Self {
        let mut blocked_reasons = Vec::new();
        if !input.editable_ifc_path_wired {
            blocked_reasons.push(TextAreaInlineIfcReadinessBlockedReason::EditableIfcPathUnwired);
        }
        if !input.projection_ifc_path_wired {
            blocked_reasons.push(TextAreaInlineIfcReadinessBlockedReason::ProjectionIfcPathUnwired);
        }
        if !input.ime_ifc_path_wired {
            blocked_reasons.push(TextAreaInlineIfcReadinessBlockedReason::ImeIfcPathUnwired);
        }
        if !input.caret_affinity_ifc_path_wired {
            blocked_reasons
                .push(TextAreaInlineIfcReadinessBlockedReason::CaretAffinityIfcPathUnwired);
        }
        if !input.scroll_follow_ifc_path_wired {
            blocked_reasons
                .push(TextAreaInlineIfcReadinessBlockedReason::ScrollFollowIfcPathUnwired);
        }
        if !input.text_area_text_run_boundary_confirmed {
            blocked_reasons
                .push(TextAreaInlineIfcReadinessBlockedReason::TextAreaTextRunBoundaryUnconfirmed);
        }
        if !input.inline_element_rollout_boundary_confirmed {
            blocked_reasons.push(
                TextAreaInlineIfcReadinessBlockedReason::InlineElementRolloutBoundaryUnconfirmed,
            );
        }
        if !input.read_only_text_prepared_path_separated {
            blocked_reasons.push(
                TextAreaInlineIfcReadinessBlockedReason::
                    ReadOnlyTextPreparedPathSeparationUnconfirmed,
            );
        }
        if !input.legacy_fallback_confirmed {
            blocked_reasons
                .push(TextAreaInlineIfcReadinessBlockedReason::LegacyFallbackUnconfirmed);
        }

        let readiness = if blocked_reasons.is_empty() {
            TextAreaInlineIfcReadinessState::ReadyForEditableIfcEvaluation
        } else {
            TextAreaInlineIfcReadinessState::Blocked
        };
        Self {
            readiness,
            blocked_reasons,
        }
    }

    pub(crate) fn readiness(&self) -> TextAreaInlineIfcReadinessState {
        self.readiness
    }

    pub(crate) fn is_ready_for_editable_ifc_evaluation(&self) -> bool {
        matches!(
            self.readiness,
            TextAreaInlineIfcReadinessState::ReadyForEditableIfcEvaluation
        )
    }

    pub(crate) fn blocked_reasons(&self) -> &[TextAreaInlineIfcReadinessBlockedReason] {
        &self.blocked_reasons
    }

    pub(crate) fn allows_text_area_default_rollout(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
impl ElementInlineIfcLayoutCallSiteRolloutConfig {
    pub(crate) fn disabled() -> Self {
        Self {
            phase: ElementInlineIfcLayoutCallSiteRolloutPhase::Disabled,
        }
    }

    pub(crate) fn explicit_dry_run_candidate() -> Self {
        Self {
            phase: ElementInlineIfcLayoutCallSiteRolloutPhase::ExplicitDryRunCandidate,
        }
    }

    pub(crate) fn explicit_shadow_observation() -> Self {
        Self::production_default_shadow_run_phase()
    }

    pub(crate) fn production_default_shadow_run_phase() -> Self {
        Self {
            phase: ElementInlineIfcLayoutCallSiteRolloutPhase::ProductionDefaultShadowRun,
        }
    }

    pub(crate) fn controlled_installed_package_candidate() -> Self {
        Self {
            phase: ElementInlineIfcLayoutCallSiteRolloutPhase::ControlledInstalledPackageCandidate,
        }
    }

    pub(crate) fn from_mode(mode: ElementInlineIfcLayoutCallSiteOptInMode) -> Self {
        match mode {
            ElementInlineIfcLayoutCallSiteOptInMode::Disabled => Self::disabled(),
            ElementInlineIfcLayoutCallSiteOptInMode::ShadowObservation => {
                Self::production_default_shadow_run_phase()
            }
            ElementInlineIfcLayoutCallSiteOptInMode::DryRunCandidate => {
                Self::explicit_dry_run_candidate()
            }
        }
    }

    pub(crate) fn phase(self) -> ElementInlineIfcLayoutCallSiteRolloutPhase {
        self.phase
    }

    pub(crate) fn mode(self) -> ElementInlineIfcLayoutCallSiteOptInMode {
        self.phase.mode()
    }

    pub(crate) fn gate(self) -> ElementInlineIfcLayoutCallSiteGate {
        match self.mode() {
            ElementInlineIfcLayoutCallSiteOptInMode::Disabled => {
                ElementInlineIfcLayoutCallSiteGate::disabled()
            }
            ElementInlineIfcLayoutCallSiteOptInMode::ShadowObservation => {
                ElementInlineIfcLayoutCallSiteGate::explicit_shadow_observation()
            }
            ElementInlineIfcLayoutCallSiteOptInMode::DryRunCandidate => {
                ElementInlineIfcLayoutCallSiteGate::explicit_dry_run_candidate()
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn for_scenario(scenario: ElementInlineIfcLayoutCallSiteScenario) -> Self {
        match scenario {
            ElementInlineIfcLayoutCallSiteScenario::DefaultLegacyFallback => Self::disabled(),
            ElementInlineIfcLayoutCallSiteScenario::DefaultCandidateShadowObservation => {
                Self::production_default_shadow_run_phase()
            }
            ElementInlineIfcLayoutCallSiteScenario::ControlledInstalledPackageCandidate => {
                Self::controlled_installed_package_candidate()
            }
            ElementInlineIfcLayoutCallSiteScenario::DemoLikeDryRunCandidate
            | ElementInlineIfcLayoutCallSiteScenario::ExamplesLikeDryRunCandidate
            | ElementInlineIfcLayoutCallSiteScenario::UnsupportedRootProbe => {
                Self::explicit_dry_run_candidate()
            }
        }
    }
}

#[cfg(test)]
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElementInlineIfcLayoutCallSiteScenario {
    DefaultLegacyFallback,
    DefaultCandidateShadowObservation,
    ControlledInstalledPackageCandidate,
    DemoLikeDryRunCandidate,
    ExamplesLikeDryRunCandidate,
    UnsupportedRootProbe,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ElementInlineIfcLayoutCallSiteOptInInput {
    pub(crate) root_key: NodeKey,
    pub(crate) max_width: f32,
    pub(crate) mode: ElementInlineIfcLayoutCallSiteOptInMode,
}

#[allow(dead_code)]
impl ElementInlineIfcLayoutCallSiteOptInInput {
    pub(crate) fn disabled(root_key: NodeKey, max_width: f32) -> Self {
        Self {
            root_key,
            max_width: max_width.max(1.0),
            mode: ElementInlineIfcLayoutCallSiteOptInMode::Disabled,
        }
    }

    pub(crate) fn dry_run_candidate(root_key: NodeKey, max_width: f32) -> Self {
        Self {
            root_key,
            max_width: max_width.max(1.0),
            mode: ElementInlineIfcLayoutCallSiteOptInMode::DryRunCandidate,
        }
    }

    pub(crate) fn shadow_observation(root_key: NodeKey, max_width: f32) -> Self {
        Self {
            root_key,
            max_width: max_width.max(1.0),
            mode: ElementInlineIfcLayoutCallSiteOptInMode::ShadowObservation,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElementInlineIfcLayoutCallSiteOptInStatus {
    Disabled,
    UnsupportedRoot,
    NoInstallTargets,
    LifecycleRan,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ElementInlineIfcLayoutCallSiteOptInOutput {
    pub(crate) root_key: NodeKey,
    pub(crate) mode: ElementInlineIfcLayoutCallSiteOptInMode,
    pub(crate) status: ElementInlineIfcLayoutCallSiteOptInStatus,
    pub(crate) install_targets: Vec<NodeKey>,
    pub(crate) lifecycle: Option<ElementInlineIfcCandidateLifecycleOutput>,
    pub(crate) fallback: ElementInlineIfcRenderFallback,
}

#[allow(dead_code)]
impl ElementInlineIfcLayoutCallSiteOptInOutput {
    fn no_op(
        input: &ElementInlineIfcLayoutCallSiteOptInInput,
        status: ElementInlineIfcLayoutCallSiteOptInStatus,
    ) -> Self {
        Self {
            root_key: input.root_key,
            mode: input.mode,
            status,
            install_targets: Vec::new(),
            lifecycle: None,
            fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
        }
    }

    pub(crate) fn lifecycle(&self) -> Option<&ElementInlineIfcCandidateLifecycleOutput> {
        self.lifecycle.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn diagnostic_for_test(
        &self,
        arena: &NodeArena,
        cache_len: usize,
    ) -> ElementInlineIfcLayoutCallSiteDiagnostic {
        let lifecycle = self.lifecycle();
        let mut target_installs = Vec::new();
        for &target in &self.install_targets {
            let install = lifecycle.and_then(|lifecycle| lifecycle.install_for_node(target));
            let render_decision = arena.get(target).and_then(|node| {
                node.element
                    .as_any()
                    .downcast_ref::<Element>()
                    .map(|element| element.inline_ifc_render_decision_for_test())
            });
            target_installs.push(ElementInlineIfcLayoutCallSiteTargetDiagnostic {
                node_key: target,
                source: install.and_then(|install| install.source),
                install_status: install.map(|install| install.status),
                has_decoration_package: install
                    .map(|install| install.has_decoration_package)
                    .unwrap_or(false),
                has_atomic_package: install
                    .map(|install| install.has_atomic_package)
                    .unwrap_or(false),
                render_decision,
            });
        }

        ElementInlineIfcLayoutCallSiteDiagnostic {
            root_key: self.root_key,
            mode: self.mode,
            status: self.status,
            fallback: self.fallback,
            cache_len,
            cache_key: lifecycle.map(|lifecycle| lifecycle.cache_key.clone()),
            invalidation: lifecycle.map(|lifecycle| lifecycle.invalidation),
            rebuilt: lifecycle.map(|lifecycle| lifecycle.rebuilt),
            install_targets: self.install_targets.clone(),
            target_installs,
        }
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ElementInlineIfcLayoutCallSiteTargetDiagnostic {
    pub(crate) node_key: NodeKey,
    pub(crate) source: Option<InlineIfcSourceId>,
    pub(crate) install_status: Option<ElementInlineIfcCandidateLifecycleInstallStatus>,
    pub(crate) has_decoration_package: bool,
    pub(crate) has_atomic_package: bool,
    pub(crate) render_decision: Option<ElementInlineIfcRenderDecision>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ElementInlineIfcLayoutCallSiteDiagnostic {
    pub(crate) root_key: NodeKey,
    pub(crate) mode: ElementInlineIfcLayoutCallSiteOptInMode,
    pub(crate) status: ElementInlineIfcLayoutCallSiteOptInStatus,
    pub(crate) fallback: ElementInlineIfcRenderFallback,
    pub(crate) cache_len: usize,
    pub(crate) cache_key: Option<InlineIfcCacheKey>,
    pub(crate) invalidation: Option<InlineIfcInvalidation>,
    pub(crate) rebuilt: Option<bool>,
    pub(crate) install_targets: Vec<NodeKey>,
    pub(crate) target_installs: Vec<ElementInlineIfcLayoutCallSiteTargetDiagnostic>,
}

#[cfg(test)]
impl ElementInlineIfcLayoutCallSiteDiagnostic {
    pub(crate) fn target(
        &self,
        key: NodeKey,
    ) -> Option<&ElementInlineIfcLayoutCallSiteTargetDiagnostic> {
        self.target_installs
            .iter()
            .find(|target| target.node_key == key)
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ElementInlineIfcLayoutCallSiteOptIn;

#[allow(dead_code)]
impl ElementInlineIfcLayoutCallSiteOptIn {
    pub(crate) fn run(
        arena: &mut NodeArena,
        input: ElementInlineIfcLayoutCallSiteOptInInput,
        cache: &mut InlineIfcElementRootCandidateCache,
    ) -> ElementInlineIfcLayoutCallSiteOptInOutput {
        if matches!(
            input.mode,
            ElementInlineIfcLayoutCallSiteOptInMode::Disabled
        ) {
            return ElementInlineIfcLayoutCallSiteOptInOutput::no_op(
                &input,
                ElementInlineIfcLayoutCallSiteOptInStatus::Disabled,
            );
        }

        if !element_inline_ifc_supports_layout_call_site_root(arena, input.root_key) {
            return ElementInlineIfcLayoutCallSiteOptInOutput::no_op(
                &input,
                ElementInlineIfcLayoutCallSiteOptInStatus::UnsupportedRoot,
            );
        }

        let install_targets =
            element_inline_ifc_layout_call_site_install_targets(arena, input.root_key);
        if install_targets.is_empty() {
            return ElementInlineIfcLayoutCallSiteOptInOutput::no_op(
                &input,
                ElementInlineIfcLayoutCallSiteOptInStatus::NoInstallTargets,
            );
        }

        let lifecycle_input =
            ElementInlineIfcCandidateLifecycleInput::new(input.root_key, input.max_width)
                .with_install_targets(install_targets.clone());
        let lifecycle = match input.mode {
            ElementInlineIfcLayoutCallSiteOptInMode::Disabled => None,
            ElementInlineIfcLayoutCallSiteOptInMode::ShadowObservation => {
                ElementInlineIfcCandidateLifecycle::observe(arena, lifecycle_input, cache)
            }
            ElementInlineIfcLayoutCallSiteOptInMode::DryRunCandidate => {
                ElementInlineIfcCandidateLifecycle::dry_run(arena, lifecycle_input, cache)
            }
        };
        let Some(lifecycle) = lifecycle else {
            return ElementInlineIfcLayoutCallSiteOptInOutput::no_op(
                &input,
                ElementInlineIfcLayoutCallSiteOptInStatus::UnsupportedRoot,
            );
        };

        ElementInlineIfcLayoutCallSiteOptInOutput {
            root_key: input.root_key,
            mode: input.mode,
            status: ElementInlineIfcLayoutCallSiteOptInStatus::LifecycleRan,
            install_targets,
            lifecycle: Some(lifecycle),
            fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
        }
    }
}

#[derive(Default)]
struct ElementInlineIfcLayoutCallSiteState {
    rollout_config: ElementInlineIfcLayoutCallSiteRolloutConfig,
    cache: InlineIfcElementRootCandidateCache,
    last_output: Option<ElementInlineIfcLayoutCallSiteOptInOutput>,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ElementInlineIfcCandidateLifecycle;

#[allow(dead_code)]
impl ElementInlineIfcCandidateLifecycle {
    pub(crate) fn dry_run(
        arena: &mut NodeArena,
        input: ElementInlineIfcCandidateLifecycleInput,
        cache: &mut InlineIfcElementRootCandidateCache,
    ) -> Option<ElementInlineIfcCandidateLifecycleOutput> {
        let collected = ElementInlineIfcMetadataCollector::collect(
            arena,
            ElementInlineIfcMetadataCollectorInput::new(input.root_key, input.max_width),
        )?;
        Some(Self::dry_run_collected(arena, input, collected, cache))
    }

    pub(crate) fn observe(
        arena: &NodeArena,
        input: ElementInlineIfcCandidateLifecycleInput,
        cache: &mut InlineIfcElementRootCandidateCache,
    ) -> Option<ElementInlineIfcCandidateLifecycleOutput> {
        let collected = ElementInlineIfcMetadataCollector::collect(
            arena,
            ElementInlineIfcMetadataCollectorInput::new(input.root_key, input.max_width),
        )?;
        Some(Self::observe_collected(input, collected, cache))
    }

    fn observe_collected(
        input: ElementInlineIfcCandidateLifecycleInput,
        collected: ElementInlineIfcMetadataCollectorOutput,
        cache: &mut InlineIfcElementRootCandidateCache,
    ) -> ElementInlineIfcCandidateLifecycleOutput {
        let candidate = cache.update(&collected.root_source);
        let mut install_targets = input.install_targets;
        if install_targets.is_empty() {
            install_targets = collected.sources_by_node.keys().copied().collect();
            install_targets.sort_by_key(|key| format!("{key:?}"));
        }

        let installs = install_targets
            .into_iter()
            .map(|target| {
                let source = collected.source_for_node(target);
                let packages = source.and_then(|source| candidate.package(source));
                ElementInlineIfcCandidateLifecycleInstall {
                    node_key: target,
                    source,
                    status: ElementInlineIfcCandidateLifecycleInstallStatus::ObservedOnly,
                    has_decoration_package: packages
                        .and_then(|packages| packages.decoration_draw_rect.as_ref())
                        .is_some_and(|package| !package.fragments.is_empty()),
                    has_atomic_package: packages
                        .and_then(|packages| packages.atomic_placement.as_ref())
                        .is_some_and(|package| !package.placements.is_empty()),
                }
            })
            .collect();

        ElementInlineIfcCandidateLifecycleOutput {
            cache_key: candidate.cache_key,
            invalidation: candidate.invalidation,
            rebuilt: candidate.rebuilt,
            cache_len: cache.len(),
            sources_by_node: collected.sources_by_node,
            installs,
        }
    }

    fn dry_run_collected(
        arena: &mut NodeArena,
        input: ElementInlineIfcCandidateLifecycleInput,
        collected: ElementInlineIfcMetadataCollectorOutput,
        cache: &mut InlineIfcElementRootCandidateCache,
    ) -> ElementInlineIfcCandidateLifecycleOutput {
        let candidate = cache.update(&collected.root_source);
        let mut install_targets = input.install_targets;
        if install_targets.is_empty() {
            install_targets = collected.sources_by_node.keys().copied().collect();
            install_targets.sort_by_key(|key| format!("{key:?}"));
        }

        let mut installs = Vec::new();
        for target in install_targets {
            let source = collected.source_for_node(target);
            let Some(mut node) = arena.get_mut(target) else {
                installs.push(ElementInlineIfcCandidateLifecycleInstall {
                    node_key: target,
                    source,
                    status: ElementInlineIfcCandidateLifecycleInstallStatus::MissingNode,
                    has_decoration_package: false,
                    has_atomic_package: false,
                });
                continue;
            };
            let Some(element) = node.element.as_any_mut().downcast_mut::<Element>() else {
                installs.push(ElementInlineIfcCandidateLifecycleInstall {
                    node_key: target,
                    source,
                    status: ElementInlineIfcCandidateLifecycleInstallStatus::SkippedNonElement,
                    has_decoration_package: false,
                    has_atomic_package: false,
                });
                continue;
            };

            let packages = source.and_then(|source| candidate.package(source));
            element.install_inline_ifc_rollout_packages_from_candidate(packages);
            let installed = element.inline_ifc_rollout_packages.clone();
            installs.push(ElementInlineIfcCandidateLifecycleInstall {
                node_key: target,
                source,
                status: if packages.is_some() {
                    ElementInlineIfcCandidateLifecycleInstallStatus::Installed
                } else {
                    ElementInlineIfcCandidateLifecycleInstallStatus::ClearedMissingSource
                },
                has_decoration_package: installed.has_draw_rect_decoration(),
                has_atomic_package: installed.has_atomic_placement(),
            });
        }

        ElementInlineIfcCandidateLifecycleOutput {
            cache_key: candidate.cache_key,
            invalidation: candidate.invalidation,
            rebuilt: candidate.rebuilt,
            cache_len: cache.len(),
            sources_by_node: collected.sources_by_node,
            installs,
        }
    }
}

fn element_inline_ifc_supports_layout_call_site_root(arena: &NodeArena, key: NodeKey) -> bool {
    arena
        .get(key)
        .and_then(|node| {
            node.element
                .as_any()
                .downcast_ref::<Element>()
                .map(|element| element.computed_style.layout == Layout::Inline)
        })
        .unwrap_or(false)
}

fn element_inline_ifc_layout_call_site_install_targets(
    arena: &NodeArena,
    root_key: NodeKey,
) -> Vec<NodeKey> {
    element_inline_ifc_layout_call_site_install_targets_from_children(
        arena,
        arena.children_of(root_key),
    )
}

fn element_inline_ifc_layout_call_site_install_targets_from_children(
    arena: &NodeArena,
    root_children: Vec<NodeKey>,
) -> Vec<NodeKey> {
    fn walk(
        arena: &NodeArena,
        key: NodeKey,
        seen: &mut FxHashSet<NodeKey>,
        out: &mut Vec<NodeKey>,
    ) {
        for child_key in arena.children_of(key) {
            if !seen.insert(child_key) {
                continue;
            }
            let is_inline_element = arena
                .get(child_key)
                .and_then(|node| {
                    node.element
                        .as_any()
                        .downcast_ref::<Element>()
                        .map(|element| element.computed_style.layout == Layout::Inline)
                })
                .unwrap_or(false);
            if is_inline_element {
                out.push(child_key);
            }
            walk(arena, child_key, seen, out);
        }
    }

    let mut seen = FxHashSet::default();
    let mut targets = Vec::new();
    for child_key in root_children {
        if !seen.insert(child_key) {
            continue;
        }
        let is_inline_element = arena
            .get(child_key)
            .and_then(|node| {
                node.element
                    .as_any()
                    .downcast_ref::<Element>()
                    .map(|element| element.computed_style.layout == Layout::Inline)
            })
            .unwrap_or(false);
        if is_inline_element {
            targets.push(child_key);
        }
        walk(arena, child_key, &mut seen, &mut targets);
    }
    targets
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ElementInlineIfcMetadataCollector;

#[allow(dead_code)]
impl ElementInlineIfcMetadataCollector {
    pub(crate) fn collect(
        arena: &NodeArena,
        input: ElementInlineIfcMetadataCollectorInput,
    ) -> Option<ElementInlineIfcMetadataCollectorOutput> {
        let root_source = element_inline_ifc_source_id_for_node(arena, input.root_key)?;
        let root_children = arena.children_of(input.root_key);
        let mut state = ElementInlineIfcMetadataCollectorState {
            arena,
            root_source,
            max_width: input.max_width.max(1.0),
            sources_by_node: FxHashMap::default(),
            decoration_sources: Vec::new(),
            atomic_sources: Vec::new(),
        };
        state.sources_by_node.insert(input.root_key, root_source);

        let mut builder = InlineIfcElementRootSourceBuilder::new().with_max_width(state.max_width);
        for child_key in root_children {
            if let Some(item) = state.collect_item(child_key, root_source) {
                builder.push_item(item);
            }
        }
        for source in state.decoration_sources {
            builder.add_decoration_source(source);
        }
        for source in state.atomic_sources {
            builder.add_atomic_source(source);
        }

        Some(ElementInlineIfcMetadataCollectorOutput {
            root_source: builder.build(),
            sources_by_node: state.sources_by_node,
        })
    }

    fn collect_for_taken_root(
        arena: &NodeArena,
        input: ElementInlineIfcMetadataCollectorInput,
        root: &Element,
    ) -> Option<ElementInlineIfcMetadataCollectorOutput> {
        if root.computed_style.layout != Layout::Inline {
            return None;
        }
        let root_source = element_inline_ifc_source_id(input.root_key, root);
        let mut state = ElementInlineIfcMetadataCollectorState {
            arena,
            root_source,
            max_width: input.max_width.max(1.0),
            sources_by_node: FxHashMap::default(),
            decoration_sources: Vec::new(),
            atomic_sources: Vec::new(),
        };
        state.sources_by_node.insert(input.root_key, root_source);

        let mut builder = InlineIfcElementRootSourceBuilder::new().with_max_width(state.max_width);
        for child_key in root.children.iter().copied() {
            if let Some(item) = state.collect_item(child_key, root_source) {
                builder.push_item(item);
            }
        }
        for source in state.decoration_sources {
            builder.add_decoration_source(source);
        }
        for source in state.atomic_sources {
            builder.add_atomic_source(source);
        }

        Some(ElementInlineIfcMetadataCollectorOutput {
            root_source: builder.build(),
            sources_by_node: state.sources_by_node,
        })
    }
}

struct ElementInlineIfcMetadataCollectorState<'a> {
    arena: &'a NodeArena,
    root_source: InlineIfcSourceId,
    max_width: f32,
    sources_by_node: FxHashMap<NodeKey, InlineIfcSourceId>,
    decoration_sources: Vec<InlineIfcElementDecorationPackageSource>,
    atomic_sources: Vec<InlineIfcSourceId>,
}

impl ElementInlineIfcMetadataCollectorState<'_> {
    fn collect_item(
        &mut self,
        key: NodeKey,
        inherited_source: InlineIfcSourceId,
    ) -> Option<InlineIfcItem> {
        enum CollectedNode {
            Text {
                source: InlineIfcSourceId,
                text: String,
                style: InlineIfcStyle,
            },
            InlineElement {
                source: InlineIfcSourceId,
                style: InlineIfcStyle,
                decoration_source: InlineIfcElementDecorationPackageSource,
                children: Vec<NodeKey>,
            },
            Atomic {
                source: InlineIfcSourceId,
                measurement: InlineIfcMeasuredAtomicBox,
            },
        }

        let collected = {
            let node = self.arena.get(key)?;
            let element = node.element.as_ref();
            let source = element_inline_ifc_source_id(key, element);
            let snapshot = element.box_model_snapshot();
            self.sources_by_node.insert(key, source);

            if let Some(text) = element.as_any().downcast_ref::<Text>() {
                CollectedNode::Text {
                    source,
                    text: text.content().to_string(),
                    style: inline_ifc_style_from_text_metadata(
                        text.inline_ifc_text_style_metadata(),
                        inherited_source,
                        self.root_source,
                    ),
                }
            } else if let Some(element) = element.as_any().downcast_ref::<Element>() {
                if element.is_fragmentable_inline_element() {
                    let style = element.inline_ifc_style_metadata();
                    CollectedNode::InlineElement {
                        source,
                        style: style.clone(),
                        decoration_source: element.inline_ifc_decoration_package_source(source),
                        children: node.children.clone(),
                    }
                } else {
                    CollectedNode::Atomic {
                        source,
                        measurement: self.atomic_measurement(snapshot),
                    }
                }
            } else {
                CollectedNode::Atomic {
                    source,
                    measurement: self.atomic_measurement(snapshot),
                }
            }
        };

        match collected {
            CollectedNode::Text {
                source,
                text,
                style,
            } => Some(InlineIfcItem::TextSpan {
                source,
                text,
                style: Some(style),
            }),
            CollectedNode::InlineElement {
                source,
                style,
                decoration_source,
                children,
            } => {
                let mut span_children = Vec::new();
                for child_key in children {
                    if let Some(item) = self.collect_item(child_key, source) {
                        span_children.push(item);
                    }
                }
                if span_children.is_empty() {
                    None
                } else {
                    self.decoration_sources.push(decoration_source);
                    Some(InlineIfcItem::Span {
                        source,
                        style: Some(style),
                        children: span_children,
                    })
                }
            }
            CollectedNode::Atomic {
                source,
                measurement,
            } => {
                self.atomic_sources.push(source);
                Some(InlineIfcItem::AtomicInlineBox {
                    source,
                    measurement,
                })
            }
        }
    }

    fn atomic_measurement(&self, snapshot: BoxModelSnapshot) -> InlineIfcMeasuredAtomicBox {
        InlineIfcMeasuredAtomicBox::new(
            InlineIfcSize::new(snapshot.width, snapshot.height),
            InlineIfcAtomicMeasureConstraints::new(Some(self.max_width)),
        )
    }
}

fn element_inline_ifc_source_id_for_node(
    arena: &NodeArena,
    key: NodeKey,
) -> Option<InlineIfcSourceId> {
    arena
        .get(key)
        .map(|node| element_inline_ifc_source_id(key, node.element.as_ref()))
}

fn element_inline_ifc_source_id(key: NodeKey, element: &dyn ElementTrait) -> InlineIfcSourceId {
    let stable_id = element.stable_id();
    if stable_id != 0 {
        return InlineIfcSourceId(stable_id);
    }
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    InlineIfcSourceId(hasher.finish())
}

fn inline_ifc_style_from_text_metadata(
    metadata: TextInlineIfcStyleMetadata,
    _inherited_source: InlineIfcSourceId,
    _root_source: InlineIfcSourceId,
) -> InlineIfcStyle {
    InlineIfcStyle {
        font_size: metadata.font_size,
        line_height: metadata.line_height,
        font_weight: metadata.font_weight,
        brush: metadata.brush,
        font_families: metadata.font_families,
    }
}

/// Snapshot of an Element's "previous frame" visual style. Used by
/// the style-transition emission path (`collect_style_transition_
/// requests` / `preserve_transform_transition_baseline`) to compute
/// from→to deltas. Phase B trimmed the layout/hover/transition fields
/// — those existed for the now-removed `restore_state` hack.
#[derive(Clone, Debug)]
pub(crate) struct ElementStyleSnapshot {
    opacity: f32,
    border_radius: f32,
    width: f32,
    height: f32,
    background_color: Color,
    foreground_color: Color,
    border_top_color: Color,
    border_right_color: Color,
    border_bottom_color: Color,
    border_left_color: Color,
    box_shadows: Vec<BoxShadow>,
    transform: Transform,
    transform_origin: TransformOrigin,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ElementInlineIfcRenderMode {
    Disabled,
    #[default]
    DrawRectPackageCandidate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElementInlineIfcRenderFallback {
    ExistingInlineFragments,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElementInlineIfcRenderDecision {
    ExistingInlineFragments,
    DrawRectPackageCandidate {
        fallback: ElementInlineIfcRenderFallback,
        has_atomic_placement_package: bool,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct ElementInlineIfcDrawRectPassMetadata {
    pub(crate) fill: RectPassParams,
    pub(crate) border: Option<RectPassParams>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ElementInlineIfcAtomicPlacementMetadata {
    pub(crate) package: InlineIfcAtomicBoxPlacementPackage,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ElementInlineIfcRolloutPackages {
    decoration_draw_rect: Option<InlineIfcElementDecorationDrawRectPackage>,
    atomic_placement: Option<InlineIfcAtomicBoxPlacementPackage>,
}

impl ElementInlineIfcRolloutPackages {
    #[allow(dead_code)]
    pub(crate) fn from_inline_ifc_distributed(
        package: &InlineIfcDistributedElementPackages,
    ) -> Self {
        Self {
            decoration_draw_rect: package.decoration_draw_rect.clone(),
            atomic_placement: package.atomic_placement.clone(),
        }
    }

    fn has_draw_rect_decoration(&self) -> bool {
        self.decoration_draw_rect
            .as_ref()
            .is_some_and(|package| !package.fragments.is_empty())
    }

    fn has_atomic_placement(&self) -> bool {
        self.atomic_placement
            .as_ref()
            .is_some_and(|package| !package.placements.is_empty())
    }
}

pub struct Element {
    core: ElementCore,
    anchor_name: Option<AnchorName>,
    pub(crate) layout_state: crate::view::layout::LayoutState,
    intrinsic_size_is_percent_base: bool,
    parsed_style: Style,
    text_cascade_style: Option<Style>,
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
    pending_inline_measure_context: Option<InlineMeasureContext>,
    last_inline_measure_context: Option<InlineMeasureContext>,
    inline_paint_fragments: Vec<Rect>,
    inline_ifc_render_mode: ElementInlineIfcRenderMode,
    inline_ifc_rollout_packages: ElementInlineIfcRolloutPackages,
    inline_ifc_layout_call_site: ElementInlineIfcLayoutCallSiteState,
    scrollbar_drag: Option<ScrollbarDragState>,
    last_scrollbar_interaction: Option<Instant>,
    scrollbar_shadow_blur_radius: f32,
    transition_requests: Option<Box<ElementTransitionRequests>>,
    last_started_animator: Option<crate::style::Animator>,
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
    event_handlers: Option<Box<ElementEventHandlers>>,
    layout_dirty: bool,
    dirty_flags: DirtyFlags,
    last_layout_placement: Option<LayoutPlacement>,
    last_layout_proposal: Option<LayoutProposal>,
    flex_info: Option<crate::view::layout::FlexLayoutInfo>,
    has_absolute_descendant_for_hit_test: bool,
    absolute_clip_rect: Option<Rect>,
    anchor_parent_clip_rect: Option<Rect>,
    hit_test_clip_rect: Option<Rect>,
    last_child_hit_test_clip_rect: Option<Rect>,
    children: Vec<crate::view::node_arena::NodeKey>,
}

impl Element {
    /// Parent id (legacy u64). Migration to `NodeKey`-based arena parent is
    /// tracked by Node.parent in the arena — this method stays for
    /// internal Element code that has not yet been ported.
    pub fn parent_id(&self) -> Option<u64> {
        self.core.parent_id
    }

    /// Sets legacy parent id. Kept for renderer_adapter compatibility
    /// during Approach-C migration; final state lives on `Node.parent`.
    pub fn set_parent_id(&mut self, parent_id: Option<u64>) {
        self.core.parent_id = parent_id;
    }

    /// Replace the child list wholesale.
    pub fn set_children(&mut self, children: Vec<crate::view::node_arena::NodeKey>) {
        self.children = children;
    }

    fn inline_ifc_layout_call_site_mode(&self) -> ElementInlineIfcLayoutCallSiteOptInMode {
        self.inline_ifc_layout_call_site_gate().resolve()
    }

    fn inline_ifc_layout_call_site_gate(&self) -> ElementInlineIfcLayoutCallSiteGate {
        self.inline_ifc_layout_call_site.rollout_config.gate()
    }

    fn inline_ifc_layout_call_site_is_enabled(&self) -> bool {
        self.inline_ifc_layout_call_site_gate().is_enabled()
    }

    fn inline_ifc_layout_call_site_dirty_gate(
        &self,
        arena: &NodeArena,
        placement: LayoutPlacement,
    ) -> bool {
        if !self.inline_ifc_layout_call_site_is_enabled() {
            return false;
        }
        if self.inline_ifc_layout_call_site.last_output.is_none() {
            return true;
        }
        if self.last_layout_placement != Some(placement) {
            return true;
        }
        let ifc_dirty_mask = DirtyPassMask::LAYOUT.union(DirtyPassMask::PAINT);
        if self.dirty_flags.intersects(ifc_dirty_mask) {
            return true;
        }
        record_refreshed_layout_gate_child_candidates(
            &self.children,
            arena,
            ifc_dirty_mask,
            LayoutGateCandidatePhase::Placement,
        )
    }

    fn run_inline_ifc_layout_call_site_opt_in_after_place(
        &mut self,
        arena: &mut NodeArena,
        max_width: f32,
    ) {
        let mode = self.inline_ifc_layout_call_site_mode();
        if matches!(mode, ElementInlineIfcLayoutCallSiteOptInMode::Disabled) {
            return;
        }

        let Some(root_key) = arena.find_by_stable_id(self.stable_id()) else {
            return;
        };
        let input = ElementInlineIfcLayoutCallSiteOptInInput {
            root_key,
            max_width: max_width.max(1.0),
            mode,
        };
        if self.computed_style.layout != Layout::Inline {
            self.inline_ifc_layout_call_site.last_output =
                Some(ElementInlineIfcLayoutCallSiteOptInOutput::no_op(
                    &input,
                    ElementInlineIfcLayoutCallSiteOptInStatus::UnsupportedRoot,
                ));
            return;
        }

        let install_targets = element_inline_ifc_layout_call_site_install_targets_from_children(
            arena,
            self.children.clone(),
        );
        if install_targets.is_empty() {
            self.inline_ifc_layout_call_site.last_output =
                Some(ElementInlineIfcLayoutCallSiteOptInOutput::no_op(
                    &input,
                    ElementInlineIfcLayoutCallSiteOptInStatus::NoInstallTargets,
                ));
            return;
        }

        let Some(collected) = ElementInlineIfcMetadataCollector::collect_for_taken_root(
            arena,
            ElementInlineIfcMetadataCollectorInput::new(root_key, input.max_width),
            self,
        ) else {
            self.inline_ifc_layout_call_site.last_output =
                Some(ElementInlineIfcLayoutCallSiteOptInOutput::no_op(
                    &input,
                    ElementInlineIfcLayoutCallSiteOptInStatus::UnsupportedRoot,
                ));
            return;
        };

        let lifecycle_input =
            ElementInlineIfcCandidateLifecycleInput::new(root_key, input.max_width)
                .with_install_targets(install_targets.clone());
        let lifecycle = match mode {
            ElementInlineIfcLayoutCallSiteOptInMode::Disabled => unreachable!(),
            ElementInlineIfcLayoutCallSiteOptInMode::ShadowObservation => {
                ElementInlineIfcCandidateLifecycle::observe_collected(
                    lifecycle_input,
                    collected,
                    &mut self.inline_ifc_layout_call_site.cache,
                )
            }
            ElementInlineIfcLayoutCallSiteOptInMode::DryRunCandidate => {
                ElementInlineIfcCandidateLifecycle::dry_run_collected(
                    arena,
                    lifecycle_input,
                    collected,
                    &mut self.inline_ifc_layout_call_site.cache,
                )
            }
        };

        self.inline_ifc_layout_call_site.last_output =
            Some(ElementInlineIfcLayoutCallSiteOptInOutput {
                root_key,
                mode,
                status: ElementInlineIfcLayoutCallSiteOptInStatus::LifecycleRan,
                install_targets,
                lifecycle: Some(lifecycle),
                fallback: ElementInlineIfcRenderFallback::ExistingInlineFragments,
            });
    }

    #[cfg(test)]
    pub(crate) fn set_inline_ifc_layout_call_site_opt_in_mode(
        &mut self,
        mode: ElementInlineIfcLayoutCallSiteOptInMode,
    ) {
        self.inline_ifc_layout_call_site.rollout_config =
            ElementInlineIfcLayoutCallSiteRolloutConfig::from_mode(mode);
    }

    #[cfg(test)]
    pub(crate) fn apply_inline_ifc_layout_call_site_rollout_config_for_test(
        &mut self,
        config: ElementInlineIfcLayoutCallSiteRolloutConfig,
    ) {
        self.inline_ifc_layout_call_site.rollout_config = config;
    }

    #[cfg(test)]
    pub(crate) fn inline_ifc_layout_call_site_last_output_for_test(
        &self,
    ) -> Option<&ElementInlineIfcLayoutCallSiteOptInOutput> {
        self.inline_ifc_layout_call_site.last_output.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn inline_ifc_layout_call_site_diagnostic_for_test(
        &self,
        arena: &NodeArena,
    ) -> Option<ElementInlineIfcLayoutCallSiteDiagnostic> {
        self.inline_ifc_layout_call_site
            .last_output
            .as_ref()
            .map(|output| {
                output.diagnostic_for_test(arena, self.inline_ifc_layout_call_site.cache.len())
            })
    }

    #[cfg(test)]
    pub(crate) fn inline_ifc_layout_call_site_cache_len_for_test(&self) -> usize {
        self.inline_ifc_layout_call_site.cache.len()
    }

    #[cfg(test)]
    pub(crate) fn inline_ifc_layout_call_site_gate_mode_for_test(
        &self,
    ) -> ElementInlineIfcLayoutCallSiteOptInMode {
        self.inline_ifc_layout_call_site_mode()
    }

    #[cfg(test)]
    pub(crate) fn inline_ifc_layout_call_site_rollout_phase_for_test(
        &self,
    ) -> ElementInlineIfcLayoutCallSiteRolloutPhase {
        self.inline_ifc_layout_call_site.rollout_config.phase()
    }

    fn is_fragmentable_inline_element(&self) -> bool {
        self.computed_style.layout == Layout::Inline
            && self.computed_style.width == SizeValue::Auto
            && self.computed_style.height == SizeValue::Auto
    }

    fn inline_ifc_style_metadata(&self) -> InlineIfcStyle {
        InlineIfcStyle {
            font_size: self.computed_style.font_size,
            line_height: self.computed_style.line_height,
            font_weight: self.computed_style.font_weight,
            brush: self.computed_style.color.to_rgba_u8(),
            font_families: self.computed_style.font_families.clone(),
        }
    }

    fn inline_ifc_decoration_package_source(
        &self,
        source: InlineIfcSourceId,
    ) -> InlineIfcElementDecorationPackageSource {
        let insets = InlineIfcDecorationBoxInsets::new(
            self.border_widths.left + self.padding.left,
            self.border_widths.right + self.padding.right,
            self.border_widths.top + self.padding.top,
            self.border_widths.bottom + self.padding.bottom,
        );
        let style = InlineIfcElementDecorationDrawRectStyle::new(
            crate::view::inline_formatting_context::InlineIfcPaintStyleKey {
                brush: self.computed_style.background_color.to_rgba_u8(),
            },
            self.background_color.as_ref().to_rgba_f32(),
            self.opacity,
            [
                self.border_widths.left,
                self.border_widths.right,
                self.border_widths.top,
                self.border_widths.bottom,
            ],
            self.border_colors.left.as_ref().to_rgba_f32(),
        );
        InlineIfcElementDecorationPackageSource::new(source, insets, style)
    }

    pub(crate) fn inline_fragment_rects(&self) -> &[Rect] {
        self.inline_paint_fragments.as_slice()
    }

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
            background_color: Color::rgba(bg_r, bg_g, bg_b, bg_a),
            foreground_color: self.foreground_color,
            border_top_color: Color::rgba(bt_r, bt_g, bt_b, bt_a),
            border_right_color: Color::rgba(br_r, br_g, br_b, br_a),
            border_bottom_color: Color::rgba(bb_r, bb_g, bb_b, bb_a),
            border_left_color: Color::rgba(bl_r, bl_g, bl_b, bl_a),
            box_shadows: self.box_shadows.clone(),
            transform: self.transform.clone(),
            transform_origin: self.transform_origin,
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

    fn transform_subtree_raster_bounds(
        &self,
        arena: &crate::view::node_arena::NodeArena,
    ) -> PromotionCompositeBounds {
        let mut bounds = self.untransformed_paint_bounds();
        for child_key in &self.children {
            let Some(child_node) = arena.get(*child_key) else {
                continue;
            };
            let child_bounds =
                if let Some(element) = child_node.element.as_any().downcast_ref::<Element>() {
                    element.transform_subtree_raster_bounds(arena)
                } else {
                    child_node.element.promotion_composite_bounds()
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
    fn stable_id(&self) -> u64 {
        self.core.id
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        let (width, height) = self.current_layout_frame_size();
        BoxModelSnapshot {
            node_id: self.core.id,
            parent_id: self.core.parent_id,
            x: self.layout_state.layout_position.x,
            y: self.layout_state.layout_position.y,
            width,
            height,
            border_radius: self.border_radius,
            should_render: self.layout_state.should_render,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    // Phase B: snapshot_state / restore_state removed (see trait def).

    fn intercepts_pointer_at(&self, viewport_x: f32, viewport_y: f32) -> bool {
        let local_x = viewport_x - self.layout_state.layout_position.x;
        let local_y = viewport_y - self.layout_state.layout_position.y;
        self.is_scrollbar_hit(local_x, local_y)
    }

    fn hit_test_visible_at(&self, viewport_x: f32, viewport_y: f32) -> bool {
        self.hit_test_clip_rect
            .map_or(true, |rect| rect.contains(viewport_x, viewport_y))
    }

    fn supports_promoted_descendants(&self) -> bool {
        true
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

    fn has_active_animator(&self) -> bool {
        self.last_started_animator.is_some()
    }

    fn promotion_self_signature(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.layout_state.should_render.hash(&mut hasher);
        self.core.should_paint.hash(&mut hasher);
        hash_f32(&mut hasher, self.layout_state.layout_position.x);
        hash_f32(&mut hasher, self.layout_state.layout_position.y);
        hash_f32(&mut hasher, self.layout_state.layout_size.width.max(0.0));
        hash_f32(&mut hasher, self.layout_state.layout_size.height.max(0.0));
        hash_f32(
            &mut hasher,
            self.layout_state.layout_inner_size.width.max(0.0),
        );
        hash_f32(
            &mut hasher,
            self.layout_state.layout_inner_size.height.max(0.0),
        );
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
        hash_f32(&mut hasher, self.layout_state.content_size.width.max(0.0));
        hash_f32(&mut hasher, self.layout_state.content_size.height.max(0.0));
        self.inline_paint_fragments.len().hash(&mut hasher);
        for fragment in &self.inline_paint_fragments {
            hash_f32(&mut hasher, fragment.x);
            hash_f32(&mut hasher, fragment.y);
            hash_f32(&mut hasher, fragment.width.max(0.0));
            hash_f32(&mut hasher, fragment.height.max(0.0));
        }
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

    fn promotion_clip_intersection_signature(
        &self,
        arena: &crate::view::node_arena::NodeArena,
    ) -> u64 {
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
        if !self.children.is_empty() {
            let overflow_child_indices: Vec<bool> = (0..self.children.len())
                .map(|idx| self.child_renders_outside_inner_clip(idx, arena))
                .collect();
            let outer_radii = normalize_corner_radii(
                self.border_radii,
                self.layout_state.layout_size.width.max(0.0),
                self.layout_state.layout_size.height.max(0.0),
            );
            let inner_radii = self.inner_clip_radii(outer_radii);
            let should_clip_children =
                self.should_clip_children(&overflow_child_indices, inner_radii, arena);
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

    fn children(&self) -> &[crate::view::node_arena::NodeKey] {
        &self.children
    }

    fn children_mut(&mut self) -> Option<&mut Vec<crate::view::node_arena::NodeKey>> {
        Some(&mut self.children)
    }

    fn parent_id(&self) -> Option<u64> {
        self.core.parent_id
    }

    fn set_parent_id(&mut self, parent_id: Option<u64>) {
        self.core.parent_id = parent_id;
    }

    fn apply_inherited(&mut self, inherited: &crate::view::renderer_adapter::StyleCascadeContext) {
        use crate::style::{LineHeight, ParsedValue, PropertyId};

        let authored = self.text_cascade_style();
        let mut next = self.parsed_style.clone();
        let mut changed = false;

        if authored.get(PropertyId::LineHeight).is_none() {
            let next_value = inherited
                .inherited_line_height()
                .map(|lh| ParsedValue::LineHeight(LineHeight::new(lh)));
            if next.get(PropertyId::LineHeight) != next_value.as_ref() {
                match next_value {
                    Some(value) => next.insert(PropertyId::LineHeight, value),
                    None => {
                        let _ = next.remove(PropertyId::LineHeight);
                    }
                }
                changed = true;
            }
        }

        if authored.get(PropertyId::VerticalAlign).is_none() {
            let next_value = inherited
                .inherited_vertical_align()
                .map(ParsedValue::VerticalAlign);
            if next.get(PropertyId::VerticalAlign) != next_value.as_ref() {
                match next_value {
                    Some(value) => next.insert(PropertyId::VerticalAlign, value),
                    None => {
                        let _ = next.remove(PropertyId::VerticalAlign);
                    }
                }
                changed = true;
            }
        }

        if authored.get(PropertyId::Cursor).is_none() {
            let next_value = inherited.inherited_cursor().map(ParsedValue::Cursor);
            if next.get(PropertyId::Cursor) != next_value.as_ref() {
                match next_value {
                    Some(value) => next.insert(PropertyId::Cursor, value),
                    None => {
                        let _ = next.remove(PropertyId::Cursor);
                    }
                }
                changed = true;
            }
        }

        if changed {
            self.parsed_style = next;
            self.recompute_style();
        }
    }

    fn child_style_cascade(
        &self,
        parent: &crate::view::renderer_adapter::StyleCascadeContext,
    ) -> crate::view::renderer_adapter::StyleCascadeContext {
        let mut child = parent.clone();
        child.merge_style(self.text_cascade_style());
        child
    }

    fn ingest_props(&mut self, node: &crate::ui::RsxElementNode) -> Result<(), String> {
        use crate::view::renderer_adapter::{as_f32, as_owned_string};
        for (key, value) in node.props.iter() {
            match *key {
                // Identity ("key") and layered "style" are owned by
                // the cold convert shell — it merges base + user style
                // before this hook runs. Skip both here.
                "key" | "style" => {}
                "anchor" => self.set_anchor_name(Some(crate::style::AnchorName::new(
                    as_owned_string(value, key)?,
                ))),
                "padding" => self.set_padding(as_f32(value, key)?),
                "padding_x" => self.set_padding_x(as_f32(value, key)?),
                "padding_y" => self.set_padding_y(as_f32(value, key)?),
                "padding_left" => self.set_padding_left(as_f32(value, key)?),
                "padding_right" => self.set_padding_right(as_f32(value, key)?),
                "padding_top" => self.set_padding_top(as_f32(value, key)?),
                "padding_bottom" => self.set_padding_bottom(as_f32(value, key)?),
                "opacity" => self.set_opacity(as_f32(value, key)?),
                other => {
                    if !try_assign_event_handler_prop(self, other, value)? {
                        return Err(format!("unknown prop `{}` on <{}>", key, node.tag));
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_prop(
        &mut self,
        arena: &mut crate::view::node_arena::NodeArena,
        self_key: crate::view::node_arena::NodeKey,
        ctx: &crate::view::fiber_work::ApplyContext<'_>,
        name: &'static str,
        value: crate::ui::PropValue,
    ) -> crate::view::fiber_work::PropApplyOutcome {
        use crate::view::fiber_work::PropApplyOutcome;
        use crate::view::renderer_adapter::{
            StyleCascadeContext, as_element_style, as_owned_string,
            element_base_style_from_inherited, style_cascade_at_parent,
        };

        match name {
            "style" => {
                let Ok(style) = as_element_style(&value, name) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                let inherited = arena.parent_of(self_key).map_or_else(
                    || {
                        StyleCascadeContext::from_viewport_style(
                            ctx.viewport_style,
                            ctx.viewport_width,
                            ctx.viewport_height,
                        )
                    },
                    |parent| {
                        style_cascade_at_parent(
                            arena,
                            parent,
                            ctx.viewport_style,
                            ctx.viewport_width,
                            ctx.viewport_height,
                        )
                    },
                );
                let effective_style = element_base_style_from_inherited(&inherited) + style.clone();
                self.replace_style(effective_style);
                self.set_text_cascade_style(style);
                PropApplyOutcome::Applied
            }
            "anchor" => {
                let Ok(name_str) = as_owned_string(&value, name) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.set_anchor_name(Some(crate::style::AnchorName::new(name_str)));
                PropApplyOutcome::Applied
            }
            other if RSX_EVENT_HANDLER_PROPS.contains(&other) => {
                // M4 #4: replace semantics for RSX event handlers.
                // Cold-path setters push onto a Vec; clear first to
                // avoid stacking duplicates across renders.
                self.clear_rsx_event_handler(other);
                match try_assign_event_handler_prop(self, other, &value) {
                    Ok(true) => PropApplyOutcome::Applied,
                    Ok(false) | Err(_) => PropApplyOutcome::DecodeFailed(other),
                }
            }
            _ => PropApplyOutcome::UnknownProp,
        }
    }

    fn reset_prop(
        &mut self,
        arena: &mut crate::view::node_arena::NodeArena,
        self_key: crate::view::node_arena::NodeKey,
        ctx: &crate::view::fiber_work::ApplyContext<'_>,
        name: &'static str,
    ) -> crate::view::fiber_work::PropApplyOutcome {
        use crate::view::fiber_work::PropApplyOutcome;
        use crate::view::renderer_adapter::{
            StyleCascadeContext, element_base_style_from_inherited, style_cascade_at_parent,
        };
        match name {
            "style" => {
                let inherited = arena.parent_of(self_key).map_or_else(
                    || {
                        StyleCascadeContext::from_viewport_style(
                            ctx.viewport_style,
                            ctx.viewport_width,
                            ctx.viewport_height,
                        )
                    },
                    |parent| {
                        style_cascade_at_parent(
                            arena,
                            parent,
                            ctx.viewport_style,
                            ctx.viewport_width,
                            ctx.viewport_height,
                        )
                    },
                );
                self.replace_style(element_base_style_from_inherited(&inherited));
                self.set_text_cascade_style(Style::new());
                PropApplyOutcome::Applied
            }
            "anchor" => {
                self.set_anchor_name(None);
                PropApplyOutcome::Applied
            }
            "opacity" => {
                self.set_opacity(1.0);
                PropApplyOutcome::Applied
            }
            other if RSX_EVENT_HANDLER_PROPS.contains(&other) => {
                self.clear_rsx_event_handler(other);
                PropApplyOutcome::Applied
            }
            _ => PropApplyOutcome::CannotReset(name),
        }
    }
}

impl Element {
    pub(crate) fn debug_render_state(&self) -> DebugElementRenderState {
        DebugElementRenderState {
            background_rgba: self.background_color.as_ref().to_rgba_u8(),
            foreground_rgba: self.foreground_color.to_rgba_u8(),
            border_top_rgba: self.border_colors.top.as_ref().to_rgba_u8(),
            border_right_rgba: self.border_colors.right.as_ref().to_rgba_u8(),
            border_bottom_rgba: self.border_colors.bottom.as_ref().to_rgba_u8(),
            border_left_rgba: self.border_colors.left.as_ref().to_rgba_u8(),
            opacity: self.opacity,
            border_radius: self.border_radius,
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn debug_transform(&self) -> &Transform {
        &self.transform
    }
}
