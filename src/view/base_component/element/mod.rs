#![allow(missing_docs)]
use rustc_hash::{FxHashMap, FxHashSet};

use super::{ComputedStyleConsumer, ElementCore, Position, Size, Text, TextInlineIfcStyleMetadata};
use crate::style::ColorLike;
use crate::style::{
    Align, AnchorName, BoxShadow, ClipMode, Collision, CollisionBoundary, Color, ComputedStyle,
    Cursor, FlowDirection, FlowWrap, JustifyContent, Layout, Length, PositionMode, ScrollDirection,
    SizeValue, Style, StyleComputeContext, TextWrap, Transform, TransformKind, TransformOrigin,
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
use crate::view::base_component::text::TextIfcOwnedLine;
use crate::view::frame_graph::texture_resource::TextureHandle;
use crate::view::frame_graph::{AttachmentTarget, FrameGraph, ResourceLifetime, TextureDesc};
use crate::view::inline_formatting_context::{
    InlineFormattingContext, InlineIfcAtomicBoxPlacementPackage, InlineIfcAtomicMeasureConstraints,
    InlineIfcCacheKey, InlineIfcDecorationBoxInsets, InlineIfcDistributedElementPackages,
    InlineIfcElementDecorationDrawRectPackage, InlineIfcElementDecorationDrawRectStyle,
    InlineIfcElementDecorationPackageSource, InlineIfcElementRootCandidate,
    InlineIfcElementRootCandidateCache, InlineIfcElementRootSource,
    InlineIfcElementRootSourceBuilder, InlineIfcItem, InlineIfcMeasuredAtomicBox,
    InlineIfcPaintRect, InlineIfcSize, InlineIfcSourceId, InlineIfcSourceKind, InlineIfcStyle,
    InlineIfcTextLayoutSnapshot, InlineIfcTextPassPaintInput,
};
#[cfg(test)]
use crate::view::inline_text_pass_adapter::inline_ifc_paint_input_to_text_pass_staging_input;
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
/// Resolved clip bounds exposed by [`ElementTrait`] for translation checks.
///
/// Although the value is engine-internal in practice, it is part of the
/// public trait contract so downstream element implementations can opt into
/// the translation fast path.
pub struct Rect {
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
    /// Translation fast-path: subtree roots replayed as a cheap shift
    /// instead of a full re-place, and the total nodes shifted across them.
    pub translated_subtree_roots: usize,
    pub translated_subtree_nodes: usize,
    pub inline_ifc_root_install_ms: f64,
    pub inline_ifc_root_install_calls: usize,
    pub inline_ifc_root_install_reuse_calls: usize,
    pub update_content_size_ms: f64,
    pub clamp_scroll_ms: f64,
    pub recompute_hit_test_ms: f64,
    pub placement_skip_failures: PlacementSkipFailureCounters,
    pub axis_placement_eligibility: AxisPlacementEligibilityProfile,
    /// Inline-IFC measure outcomes: cheap reuse (no collect), content-size
    /// short-circuit (collect, no geometry), full reshape.
    pub ifc_measure_cheap: usize,
    pub ifc_measure_shortcircuit: usize,
    pub ifc_measure_full: usize,
    /// Why each measured element did not skip: its own LAYOUT dirt, a
    /// dirty descendant, or a changed proposal (size constraint).
    pub measure_ran_self_dirty: usize,
    pub measure_ran_child_dirty: usize,
    pub measure_ran_proposal_changed: usize,
    pub proposal_changed_size: usize,
    pub proposal_changed_viewport: usize,
    pub proposal_changed_percent_base: usize,
    pub proposal_changed_first: usize,
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
    static LAYOUT_PLACE_TIMING_STACK: RefCell<Vec<LayoutPlaceTimingScope>> =
        RefCell::new(Vec::new());
    static LAYOUT_GATE_CANDIDATE_PROFILE: RefCell<LayoutGateCandidateProfile> =
        RefCell::new(LayoutGateCandidateProfile::default());
}

thread_local! {
    static LAYOUT_PLACE_PROFILE_ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Gate for the layout/place profiling instrumentation. When disabled
/// (the default), the hot loops skip every timestamp and counter update;
/// the viewport enables it only while the render-time trace is on.
/// Thread-local like the profile itself, so parallel tests stay isolated.
pub(crate) fn set_layout_place_profile_enabled(enabled: bool) {
    LAYOUT_PLACE_PROFILE_ENABLED.with(|cell| cell.set(enabled));
}

pub(crate) fn layout_place_profile_enabled() -> bool {
    LAYOUT_PLACE_PROFILE_ENABLED.with(|cell| cell.get())
}

thread_local! {
    static TRANSITION_REQUESTS_PENDING: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
}

/// True when any element queued a transition/animation request since the
/// last [`take_transition_requests_pending`]. The per-frame request
/// collection walks visit the whole tree; this flag lets idle frames skip
/// them entirely.
pub(crate) fn transition_requests_pending() -> bool {
    TRANSITION_REQUESTS_PENDING.with(|cell| cell.get())
}

pub(crate) fn take_transition_requests_pending() -> bool {
    TRANSITION_REQUESTS_PENDING.with(|cell| cell.replace(false))
}

fn mark_transition_requests_pending() {
    TRANSITION_REQUESTS_PENDING.with(|cell| cell.set(true));
}

/// Queue accessor for an element's transition/animation requests; flags
/// the frame-level pending marker so the per-frame collection walks know
/// there is work to pick up. Borrows only the queue field so callers can
/// keep reading sibling fields in the same expression.
fn queue_transition_requests(
    requests: &mut Option<Box<ElementTransitionRequests>>,
) -> &mut ElementTransitionRequests {
    mark_transition_requests_pending();
    requests.get_or_insert_with(Default::default)
}

pub(crate) fn reset_layout_place_profile() {
    LAYOUT_PLACE_PROFILE.with(|profile| {
        *profile.borrow_mut() = LayoutPlaceProfile::default();
    });
    LAYOUT_PLACE_TIMING_STACK.with(|stack| {
        stack.borrow_mut().clear();
    });
}

pub(crate) fn take_layout_place_profile() -> LayoutPlaceProfile {
    LAYOUT_PLACE_PROFILE.with(|profile| std::mem::take(&mut *profile.borrow_mut()))
}

/// Mutate the per-frame layout-place profile via a closure.
/// `pub(crate)` so layout-pipeline modules (e.g. `crate::view::layout::place`)
/// can record profile counters without exposing the thread-local directly.
pub(crate) fn with_layout_place_profile(f: impl FnOnce(&mut LayoutPlaceProfile)) {
    if !layout_place_profile_enabled() {
        return;
    }
    LAYOUT_PLACE_PROFILE.with(|profile| f(&mut profile.borrow_mut()))
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum LayoutPlaceTiming {
    PlaceSelf,
    PlaceChildren,
    PlaceFlexChildren,
    PlaceLayoutInline,
    PlaceLayoutFlex,
    PlaceLayoutFlow,
    ChildPlace,
    AbsoluteChildPlace,
    InlineIfcRootInstall,
    UpdateContentSize,
    ClampScroll,
    RecomputeHitTest,
}

struct LayoutPlaceTimingScope {
    metric: LayoutPlaceTiming,
    started_at: Instant,
    child_elapsed_ms: f64,
}

struct LayoutPlaceTimingGuard {
    active: bool,
}

impl LayoutPlaceTimingGuard {
    fn new(metric: LayoutPlaceTiming) -> Self {
        if !layout_place_profile_enabled() {
            return Self { active: false };
        }
        LAYOUT_PLACE_TIMING_STACK.with(|stack| {
            stack.borrow_mut().push(LayoutPlaceTimingScope {
                metric,
                started_at: Instant::now(),
                child_elapsed_ms: 0.0,
            });
        });
        Self { active: true }
    }
}

impl Drop for LayoutPlaceTimingGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let (metric, exclusive_ms) = LAYOUT_PLACE_TIMING_STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            let scope = stack
                .pop()
                .expect("layout place timing scope stack underflow");
            let elapsed_ms = scope.started_at.elapsed().as_secs_f64() * 1000.0;
            if let Some(parent) = stack.last_mut() {
                parent.child_elapsed_ms += elapsed_ms;
            }
            (scope.metric, (elapsed_ms - scope.child_elapsed_ms).max(0.0))
        });
        with_layout_place_profile(|profile| match metric {
            LayoutPlaceTiming::PlaceSelf => profile.place_self_ms += exclusive_ms,
            LayoutPlaceTiming::PlaceChildren => profile.place_children_ms += exclusive_ms,
            LayoutPlaceTiming::PlaceFlexChildren => profile.place_flex_children_ms += exclusive_ms,
            LayoutPlaceTiming::PlaceLayoutInline => profile.place_layout_inline_ms += exclusive_ms,
            LayoutPlaceTiming::PlaceLayoutFlex => profile.place_layout_flex_ms += exclusive_ms,
            LayoutPlaceTiming::PlaceLayoutFlow => profile.place_layout_flow_ms += exclusive_ms,
            LayoutPlaceTiming::ChildPlace => profile.non_axis_child_place_ms += exclusive_ms,
            LayoutPlaceTiming::AbsoluteChildPlace => {
                profile.absolute_child_place_ms += exclusive_ms
            }
            LayoutPlaceTiming::InlineIfcRootInstall => {
                profile.inline_ifc_root_install_ms += exclusive_ms
            }
            LayoutPlaceTiming::UpdateContentSize => profile.update_content_size_ms += exclusive_ms,
            LayoutPlaceTiming::ClampScroll => profile.clamp_scroll_ms += exclusive_ms,
            LayoutPlaceTiming::RecomputeHitTest => profile.recompute_hit_test_ms += exclusive_ms,
        });
    }
}

pub(crate) fn profile_layout_place_time<R>(metric: LayoutPlaceTiming, f: impl FnOnce() -> R) -> R {
    let _guard = LayoutPlaceTimingGuard::new(metric);
    f()
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

    if !layout_place_profile_enabled() {
        return dirty_children > 0;
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

    /// If `self` differs from `previous` *only* by the parent origin
    /// (`parent_x` / `parent_y`), return that `(dx, dy)` delta — the signal
    /// that this placement is a pure translation and can be replayed as a
    /// cheap subtree shift instead of a full re-place. Every other field
    /// (available size, viewport, percent base, visual offset) must match
    /// exactly, since those feed the relative layout that translation must
    /// leave untouched. Returns `None` when anything else changed.
    pub(crate) fn translation_only_delta(self, previous: LayoutPlacement) -> Option<(f32, f32)> {
        let same_non_origin = self.visual_offset_x == previous.visual_offset_x
            && self.visual_offset_y == previous.visual_offset_y
            && self.available_width == previous.available_width
            && self.available_height == previous.available_height
            && self.viewport_width == previous.viewport_width
            && self.viewport_height == previous.viewport_height
            && self.percent_base_width == previous.percent_base_width
            && self.percent_base_height == previous.percent_base_height;
        if !same_non_origin {
            return None;
        }
        Some((
            self.parent_x - previous.parent_x,
            self.parent_y - previous.parent_y,
        ))
    }
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
    /// Opt in when [`Self::sync_arena`] performs real work. Registered hosts
    /// are visited once before layout; the default keeps ordinary elements
    /// out of what used to be an unconditional whole-tree sync traversal.
    fn requires_arena_sync(&self) -> bool {
        false
    }
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

    fn promotion_requires_mask_surface(&self, _arena: &crate::view::node_arena::NodeArena) -> bool {
        false
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

    /// Placement-skip eligibility this node contributes to its own subtree
    /// aggregate. The default is a transparent leaf — no blockers — so a
    /// stationary, placement-clean subtree containing it (e.g. a `Text`
    /// label, `Image`, or `Svg`) can still skip re-placement when an
    /// ancestor re-places. A node only needs to override this if its own
    /// placement depends on something other than its parent's position
    /// (anchors, absolute positioning, active layout-transition state);
    /// `Element` does exactly that. Descendant blockers are unioned
    /// separately by the arena walk, so a transparent wrapper never hides
    /// a real blocker beneath it.
    fn placement_eligibility_metadata(
        &self,
    ) -> crate::view::node_arena::PlacementEligibilityMetadata {
        // Default: transparent to placement-skip, but opaque to the
        // translation fast-path. A host opts into cheap ancestor-move
        // replay by overriding both this (to `empty()`/translatable) AND
        // `translate_in_place`. Keeping the default opaque means a new
        // host type is correct-by-default (falls back to full re-place).
        crate::view::node_arena::PlacementEligibilityMetadata::opaque_to_translation()
    }

    /// Shift this node's already-placed absolute geometry by `(dx, dy)`
    /// without re-running layout. Called by the translation fast-path when
    /// a pure ancestor move leaves relative layout unchanged. Default is a
    /// no-op; only reached for hosts that advertised themselves as
    /// translatable via `placement_eligibility_metadata`, so the default
    /// is never invoked on the fast path in practice.
    fn translate_in_place(&mut self, _dx: f32, _dy: f32) {}

    /// The `LayoutPlacement` this node was last placed with, if any.
    /// Powers the translation fast-path's "differs only by a uniform
    /// translation?" check. Default `None` (host opted out / never placed).
    fn last_placement(&self) -> Option<LayoutPlacement> {
        None
    }

    /// This node's resolved hit-test clip rect, if it keeps one. The
    /// translation fast-path uses it to confirm the inherited clip moved
    /// rigidly with the subtree. Default `None` (leaf hosts without a clip,
    /// e.g. `Text`, hit-test by box model alone, so they impose no clip
    /// constraint on translation).
    fn hit_test_clip_rect(&self) -> Option<Rect> {
        None
    }

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

    /// Update the compatibility child mirror after the arena commits the
    /// authoritative structural list. Hosts must not expose independent
    /// mutation of this mirror.
    fn sync_children_mirror(&mut self, _children: &[crate::view::node_arena::NodeKey]) {}

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
    pub(crate) viewport_width: f32,
    pub(crate) viewport_height: f32,
}

#[allow(dead_code)]
impl ElementInlineIfcMetadataCollectorInput {
    pub(crate) fn new(
        root_key: NodeKey,
        max_width: f32,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Self {
        Self {
            root_key,
            max_width: max_width.max(1.0),
            viewport_width,
            viewport_height,
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

#[derive(Default)]
struct ElementInlineIfcLayoutCallSiteState {
    cache: InlineIfcElementRootCandidateCache,
    current: Option<ElementInlineIfcRootInstall>,
    /// Plan computed during measure (origin-independent). When measure
    /// runs it reshapes the IFC and stashes the fresh plan here; place
    /// consumes it. When measure is skipped (a pure move), this stays
    /// `None` and place reuses `current` with new origins.
    pending: Option<ElementInlineIfcPendingPlan>,
}

struct ElementInlineIfcPendingPlan {
    cache_key: InlineIfcCacheKey,
    children_snapshot: Vec<NodeKey>,
    content_top_offset: f32,
    content_size: (f32, f32),
    plan: Vec<InlineIfcNodeInstallOp>,
}

/// Live install record for an inline IFC root: the cache key whose shaped
/// context backs the root's glyph pass, the origin-independent install plan
/// (so a pure move re-applies it without re-shaping), and the children
/// snapshot it was built from (so a structural change invalidates it).
struct ElementInlineIfcRootInstall {
    cache_key: InlineIfcCacheKey,
    children_snapshot: Vec<NodeKey>,
    content_top_offset: f32,
    /// IFC content size (insets excluded) for this shaping, so measure can
    /// return it without rebuilding geometry when the IFC is unchanged.
    content_size: (f32, f32),
    /// Inner width this shaping was built at; a width change reshapes.
    inner_width: f32,
    /// Viewport used to resolve viewport-relative inline gap values.
    viewport_width: f32,
    viewport_height: f32,
    plan: Vec<InlineIfcNodeInstallOp>,
    installed_nodes: Vec<NodeKey>,
    /// Origins the plan was last applied at: (origin_x, origin_y,
    /// flow_origin_x, flow_origin_y). A pure move re-applies as an
    /// in-place delta shift; identical origins skip the apply entirely.
    applied_origins: (f32, f32, f32, f32),
}

/// One descendant's install in IFC content coordinates (origin-independent).
/// Applying it shifts everything by the root's current origin, so a window
/// move reuses the plan and only re-applies origins.
enum InlineIfcNodeInstallOp {
    Span {
        node_key: NodeKey,
        /// Decoration package in content coordinates; apply only adds the
        /// current root origin.
        package: Option<InlineIfcDistributedElementPackages>,
        /// Content-coordinate paint fragments.
        paint_fragments: Vec<InlineIfcPaintRect>,
    },
    Text {
        node_key: NodeKey,
        /// Content-coord owned lines (shifted to absolute at apply time).
        lines: Vec<TextIfcOwnedLine>,
        /// Source-filtered glyph payload rebased to this Text shell.
        paint_input: Arc<InlineIfcTextPassPaintInput>,
        /// Content-coordinate glyph bounds used only by TextPass clipping.
        paint_bounds: InlineIfcPaintRect,
    },
    Atomic {
        node_key: NodeKey,
        rect: InlineIfcPaintRect,
    },
}

impl InlineIfcNodeInstallOp {
    fn node_key(&self) -> NodeKey {
        match self {
            Self::Span { node_key, .. }
            | Self::Text { node_key, .. }
            | Self::Atomic { node_key, .. } => *node_key,
        }
    }
}

/// Union of absolute rects; zero rect when empty.
fn bounding_rect(rects: &[crate::ui::Rect]) -> crate::ui::Rect {
    let mut iter = rects.iter();
    let Some(first) = iter.next() else {
        return crate::ui::Rect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        };
    };
    let mut left = first.x;
    let mut top = first.y;
    let mut right = first.x + first.width.max(0.0);
    let mut bottom = first.y + first.height.max(0.0);
    for rect in iter {
        left = left.min(rect.x);
        top = top.min(rect.y);
        right = right.max(rect.x + rect.width.max(0.0));
        bottom = bottom.max(rect.y + rect.height.max(0.0));
    }
    crate::ui::Rect {
        x: left,
        y: top,
        width: (right - left).max(0.0),
        height: (bottom - top).max(0.0),
    }
}

/// Clear any inline-IFC install state from a descendant node.
fn clear_inline_ifc_node_install(arena: &mut NodeArena, node_key: NodeKey) {
    arena.with_element_taken(node_key, |child, _arena| {
        if let Some(element) = child.as_any_mut().downcast_mut::<Element>() {
            element.install_inline_ifc_rollout_packages_from_candidate(None);
            element.inline_paint_fragments = Vec::new();
            element.inline_ifc_owned_by_root = false;
        } else if let Some(text) = child.as_any_mut().downcast_mut::<Text>() {
            text.clear_inline_ifc_owned_geometry();
        }
    });
}

/// Geometry extracted from a shaped inline IFC root context, in content
/// coordinates (before the content-top normalization shift).
struct InlineIfcRootGeometry {
    content_top_offset: f32,
    content_size: (f32, f32),
    nodes: Vec<InlineIfcRootNodeGeometry>,
}

struct InlineIfcRootNodeGeometry {
    node_key: NodeKey,
    kind: InlineIfcRootNodeGeometryKind,
}

enum InlineIfcRootNodeGeometryKind {
    /// Fragmentable inline element: one rect per visual line it spans.
    Span { fragments: Vec<InlineIfcPaintRect> },
    /// Text node owned by the root: per-line rects plus caret stops.
    Text {
        lines: Vec<TextIfcOwnedLine>,
        paint_input: Arc<InlineIfcTextPassPaintInput>,
        paint_bounds: InlineIfcPaintRect,
    },
    /// Atomic inline box: its vertical-align-adjusted line placement.
    Atomic { rect: InlineIfcPaintRect },
}

fn inline_ifc_root_geometry(
    context: Option<&InlineFormattingContext>,
    arena: &NodeArena,
    sources_by_node: &FxHashMap<NodeKey, InlineIfcSourceId>,
    root_key: NodeKey,
) -> Option<InlineIfcRootGeometry> {
    let context = context?;
    let snapshot = context.text_layout_snapshot_ref();
    let content_top_offset = snapshot
        .lines
        .iter()
        .map(|line| line.y)
        .fold(0.0f32, f32::min);

    let mut content: Option<InlineIfcPaintRect> = None;
    let merge = |rect: InlineIfcPaintRect, content: &mut Option<InlineIfcPaintRect>| {
        let merged = match content.take() {
            None => rect,
            Some(current) => {
                let left = current.x.min(rect.x);
                let top = current.y.min(rect.y);
                let right = (current.x + current.width.max(0.0)).max(rect.x + rect.width.max(0.0));
                let bottom =
                    (current.y + current.height.max(0.0)).max(rect.y + rect.height.max(0.0));
                InlineIfcPaintRect {
                    x: left,
                    y: top,
                    width: (right - left).max(0.0),
                    height: (bottom - top).max(0.0),
                }
            }
        };
        *content = Some(merged);
    };
    let mut ordered: Vec<(NodeKey, InlineIfcSourceId)> = sources_by_node
        .iter()
        .map(|(&node_key, &source)| (node_key, source))
        .filter(|&(node_key, _)| node_key != root_key)
        .collect();
    ordered.sort_by_key(|(node_key, _)| format!("{node_key:?}"));

    let mut nodes = Vec::new();
    for (node_key, source) in ordered {
        let Some(node) = arena.get(node_key) else {
            continue;
        };
        if node.element.as_any().is::<Text>() {
            let lines = text_ifc_owned_lines_for_source(context, &snapshot, source);
            let paint_input = context.text_pass_paint_input_for_source(source);
            let glyph_rects = context
                .source_text_line_rects(source)
                .into_iter()
                .map(|(_, rect)| crate::ui::Rect {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                })
                .collect::<Vec<_>>();
            let glyph_bounds = bounding_rect(&glyph_rects);
            let paint_bounds = if glyph_bounds.width > 0.0 && glyph_bounds.height > 0.0 {
                InlineIfcPaintRect {
                    x: glyph_bounds.x,
                    y: glyph_bounds.y,
                    width: glyph_bounds.width,
                    height: glyph_bounds.height,
                }
            } else {
                let fallback =
                    bounding_rect(&lines.iter().map(|line| line.rect).collect::<Vec<_>>());
                InlineIfcPaintRect {
                    x: fallback.x,
                    y: fallback.y,
                    width: fallback.width,
                    height: fallback.height,
                }
            };
            merge(paint_bounds, &mut content);
            for line in &lines {
                merge(
                    InlineIfcPaintRect {
                        x: line.rect.x,
                        y: line.rect.y,
                        width: line.rect.width,
                        height: line.rect.height,
                    },
                    &mut content,
                );
            }
            nodes.push(InlineIfcRootNodeGeometry {
                node_key,
                kind: InlineIfcRootNodeGeometryKind::Text {
                    lines,
                    paint_input,
                    paint_bounds,
                },
            });
            continue;
        }
        let is_span = node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .is_some_and(Element::is_fragmentable_inline_element);
        if is_span {
            // The decoration distributor already computed this span's
            // per-line extents (its own glyphs plus nested children).
            let raw_fragments = snapshot
                .decorations
                .iter()
                .filter(|fragment| fragment.source == source)
                .collect::<Vec<_>>();
            let fragments = raw_fragments
                .iter()
                .map(|fragment| fragment.rect)
                .collect::<Vec<_>>();
            for rect in &fragments {
                merge(*rect, &mut content);
            }
            nodes.push(InlineIfcRootNodeGeometry {
                node_key,
                kind: InlineIfcRootNodeGeometryKind::Span { fragments },
            });
            continue;
        }

        let package = context.atomic_box_placement_package(source);
        let Some(placement) = package.placements.first() else {
            continue;
        };
        let vertical_align = node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .map(|element| element.computed_style.vertical_align)
            .unwrap_or(crate::style::VerticalAlign::Baseline);
        let mut rect = placement.rect;
        if let Some(line) = snapshot.lines.get(placement.line_index) {
            let item_height = rect.height.max(0.0);
            let align_offset = baseline_cross_offset(
                line.baseline,
                line.height,
                item_height,
                item_height,
                vertical_align,
            );
            rect.y = line.y + align_offset;
        }
        merge(rect, &mut content);
        nodes.push(InlineIfcRootNodeGeometry {
            node_key,
            kind: InlineIfcRootNodeGeometryKind::Atomic { rect },
        });
    }

    let content_size = content
        .map(|rect| {
            (
                (rect.x + rect.width).max(0.0),
                (rect.y + rect.height - content_top_offset).max(0.0),
            )
        })
        .unwrap_or((0.0, 0.0));

    Some(InlineIfcRootGeometry {
        content_top_offset,
        content_size,
        nodes,
    })
}

/// Convert geometry into an origin-independent install plan so applying it
/// is a pure origin shift.
fn build_inline_ifc_install_plan(
    geometry: InlineIfcRootGeometry,
    collected: &ElementInlineIfcMetadataCollectorOutput,
    candidate: &InlineIfcElementRootCandidate,
) -> Vec<InlineIfcNodeInstallOp> {
    let mut plan = Vec::with_capacity(geometry.nodes.len());
    for node_geometry in geometry.nodes {
        let node_key = node_geometry.node_key;
        match node_geometry.kind {
            InlineIfcRootNodeGeometryKind::Span { fragments } => {
                let package = collected
                    .source_for_node(node_key)
                    .and_then(|source| candidate.package(source).cloned());
                // Paint fragments already use the aligned inline content
                // box. Vertical border/padding expands from that box; do
                // not mix its y with the outer line-box height.
                let paint_fragments = package
                    .as_ref()
                    .and_then(|package| package.decoration_draw_rect.as_ref())
                    .map(|package| {
                        package
                            .fragments
                            .iter()
                            .map(|fragment| fragment.rect)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or(fragments);
                plan.push(InlineIfcNodeInstallOp::Span {
                    node_key,
                    package,
                    paint_fragments,
                });
            }
            InlineIfcRootNodeGeometryKind::Text {
                lines,
                paint_input,
                paint_bounds,
            } => {
                let mut paint_input = (*paint_input).clone();
                for line in &mut paint_input.lines {
                    line.x -= paint_bounds.x;
                    line.y -= paint_bounds.y;
                }
                for glyph in &mut paint_input.glyphs {
                    glyph.x -= paint_bounds.x;
                    glyph.baseline_y -= paint_bounds.y;
                }
                plan.push(InlineIfcNodeInstallOp::Text {
                    node_key,
                    lines,
                    paint_input: Arc::new(paint_input),
                    paint_bounds,
                });
            }
            InlineIfcRootNodeGeometryKind::Atomic { rect } => {
                plan.push(InlineIfcNodeInstallOp::Atomic { node_key, rect });
            }
        }
    }
    plan
}

/// Per-line geometry for a Text node shaped inside an inline IFC root:
/// line rects plus caret-stop x positions for every char boundary, all in
/// IFC content coordinates (caller shifts to absolute).
fn text_ifc_owned_lines_for_source(
    context: &InlineFormattingContext,
    snapshot: &InlineIfcTextLayoutSnapshot,
    source: InlineIfcSourceId,
) -> Vec<TextIfcOwnedLine> {
    let Some(byte_range) = context
        .source_ranges()
        .iter()
        .find(|range| range.source == source && range.kind == InlineIfcSourceKind::Text)
        .map(|range| range.range.clone())
    else {
        return Vec::new();
    };
    let text = &context.backing_text()[byte_range.clone()];

    let mut lines: Vec<(usize, Vec<(usize, f32)>, TextIfcOwnedLine)> = Vec::new();
    for stop in context
        .visual_caret_stops_ref()
        .iter()
        .filter(|stop| stop.source == source)
    {
        if stop.byte_index < byte_range.start || stop.byte_index > byte_range.end {
            continue;
        }
        let local_char = text[..stop.byte_index.saturating_sub(byte_range.start)]
            .chars()
            .count();
        let line_rect = snapshot
            .lines
            .get(stop.line_index)
            .map(|line| crate::ui::Rect {
                x: line.x,
                y: line.y,
                width: line.width,
                height: line.height,
            })
            .unwrap_or(crate::ui::Rect {
                x: stop.x,
                y: stop.y,
                width: 0.0,
                height: stop.height,
            });
        match lines
            .iter_mut()
            .find(|(line_index, _, _)| *line_index == stop.line_index)
        {
            Some((_, stops, line)) => {
                stops.push((local_char, stop.x));
                line.char_range.start = line.char_range.start.min(local_char);
                line.char_range.end = line.char_range.end.max(local_char + 1);
            }
            _ => {
                lines.push((
                    stop.line_index,
                    vec![(local_char, stop.x)],
                    TextIfcOwnedLine {
                        rect: line_rect,
                        text_rect: line_rect,
                        char_range: local_char..local_char + 1,
                        caret_xs: Vec::new(),
                    },
                ));
            }
        }
    }

    // Each line keeps both the line box (`rect`, used for layout bounds and
    // caret height) and the baseline-aligned text box (`text_rect`, where
    // the glyphs actually paint — used for fragment-position observation
    // and selection). A tall sibling inline box inflates the former only.
    let text_line_rects = context.source_text_line_rects(source);
    let mut out: Vec<TextIfcOwnedLine> = Vec::with_capacity(lines.len());
    for (line_index, mut stops, mut line) in lines {
        stops.sort_by_key(|(local_char, _)| *local_char);
        stops.dedup_by_key(|(local_char, _)| *local_char);
        line.caret_xs = stops.into_iter().map(|(_, x)| x).collect();
        if let Some((_, rect)) = text_line_rects
            .iter()
            .find(|(index, _)| *index == line_index)
        {
            line.text_rect = crate::ui::Rect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            };
        } else {
            line.text_rect = line.rect;
        }
        // Tighten x to the caret extent (covers trailing whitespace the
        // glyph run omits); glyphless boundaries keep the line rect.
        if let (Some(&first), Some(&last)) = (line.caret_xs.first(), line.caret_xs.last()) {
            let left = first.min(last);
            let right = first.max(last);
            if right > left {
                line.rect.x = left;
                line.rect.width = right - left;
                line.text_rect.x = left;
                line.text_rect.width = right - left;
            }
        }
        out.push(line);
    }
    out
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
            viewport_width: input.viewport_width,
            viewport_height: input.viewport_height,
            sources_by_node: FxHashMap::default(),
            decoration_sources: Vec::new(),
            atomic_sources: Vec::new(),
        };
        state.sources_by_node.insert(input.root_key, root_source);

        let (allow_wrap, gap) = arena
            .get(input.root_key)
            .and_then(|node| {
                node.element.as_any().downcast_ref::<Element>().map(|root| {
                    (
                        root.computed_style.text_wrap != TextWrap::NoWrap,
                        state.resolved_gap(root),
                    )
                })
            })
            .unwrap_or((true, 0.0));
        let mut builder = InlineIfcElementRootSourceBuilder::new()
            .with_max_width(state.max_width)
            .with_allow_wrap(allow_wrap);
        for item in state.collect_children(root_children, root_source, gap) {
            builder.push_item(item);
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
            viewport_width: input.viewport_width,
            viewport_height: input.viewport_height,
            sources_by_node: FxHashMap::default(),
            decoration_sources: Vec::new(),
            atomic_sources: Vec::new(),
        };
        state.sources_by_node.insert(input.root_key, root_source);

        let allow_wrap = root.computed_style.text_wrap != TextWrap::NoWrap;
        let gap = state.resolved_gap(root);
        let mut builder = InlineIfcElementRootSourceBuilder::new()
            .with_max_width(state.max_width)
            .with_allow_wrap(allow_wrap);
        for item in state.collect_children(root.children.iter().copied(), root_source, gap) {
            builder.push_item(item);
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
    viewport_width: f32,
    viewport_height: f32,
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
                gap: f32,
            },
            Atomic {
                source: InlineIfcSourceId,
                measurement: InlineIfcMeasuredAtomicBox,
            },
        }

        let collected = {
            let node = self.arena.get(key)?;
            let element = node.element.as_ref();
            if element
                .as_any()
                .downcast_ref::<Element>()
                .is_some_and(|element| {
                    element.computed_style.position.mode() == PositionMode::Absolute
                })
            {
                return None;
            }
            let source = element_inline_ifc_source_id(key, element);
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
                        gap: self.resolved_gap(element),
                    }
                } else {
                    CollectedNode::Atomic {
                        source,
                        measurement: self.atomic_measurement(element.measured_size()),
                    }
                }
            } else {
                CollectedNode::Atomic {
                    source,
                    measurement: self.atomic_measurement(element.measured_size()),
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
                gap,
            } => {
                let span_children = self.collect_children(children, source, gap);
                if span_children.is_empty() {
                    None
                } else {
                    let edge_insets = [
                        decoration_source.slice_insets.left,
                        decoration_source.slice_insets.right,
                    ];
                    self.decoration_sources.push(decoration_source);
                    Some(InlineIfcItem::Span {
                        source,
                        style: Some(style),
                        children: span_children,
                        edge_insets,
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

    fn collect_children(
        &mut self,
        children: impl IntoIterator<Item = NodeKey>,
        parent_source: InlineIfcSourceId,
        gap: f32,
    ) -> Vec<InlineIfcItem> {
        let mut items = Vec::new();
        for child_key in children {
            let Some(item) = self.collect_item(child_key, parent_source) else {
                continue;
            };
            if !items.is_empty() && gap > 0.0 {
                items.push(InlineIfcItem::GapSpacer {
                    source: parent_source,
                    width: gap,
                });
            }
            items.push(item);
        }
        items
    }

    fn resolved_gap(&self, element: &Element) -> f32 {
        resolve_px(
            element.computed_style.gap,
            self.max_width,
            self.viewport_width,
            self.viewport_height,
        )
    }

    fn atomic_measurement(&self, measured_size: (f32, f32)) -> InlineIfcMeasuredAtomicBox {
        InlineIfcMeasuredAtomicBox::new(
            InlineIfcSize::new(measured_size.0, measured_size.1),
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
        font_families: metadata.font_families.into(),
        vertical_align: metadata.vertical_align,
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
    pub(crate) fn from_inline_ifc_distributed(
        package: &InlineIfcDistributedElementPackages,
    ) -> Self {
        Self {
            decoration_draw_rect: package.decoration_draw_rect.clone(),
            atomic_placement: package.atomic_placement.clone(),
        }
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
    inline_paint_fragments: Vec<Rect>,
    /// True while an ancestor inline IFC root owns this fragmentable
    /// inline element's geometry: its measure/place become shells and its
    /// fragments/packages are installed by the owning root.
    inline_ifc_owned_by_root: bool,
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

    /// Placement-skip buster for inline IFC roots: a root whose layout
    /// inputs changed must re-run place so the shaped candidate, installed
    /// packages, and descendant geometry stay fresh.
    fn inline_ifc_layout_call_site_dirty_gate(
        &self,
        arena: &NodeArena,
        placement: LayoutPlacement,
    ) -> bool {
        if self.computed_style.layout != Layout::Inline || self.inline_ifc_owned_by_root {
            return false;
        }
        if self.inline_ifc_layout_call_site.current.is_none() {
            return true;
        }
        if self.last_layout_placement != Some(placement) {
            return true;
        }
        let self_dirty_mask = DirtyPassMask::LAYOUT.union(DirtyPassMask::PAINT);
        if self.dirty_flags.intersects(self_dirty_mask) {
            return true;
        }
        // The placement gate that runs just before this one already
        // refreshed the per-pass subtree dirty cache; query it without
        // recording another round of gate candidates.
        self.children
            .iter()
            .any(|&child_key| arena.subtree_dirty_intersects(child_key, DirtyPassMask::LAYOUT))
    }

    /// The inline IFC root pipeline, run after `place_self`: shape (or
    /// reuse) the candidate, distribute decoration/atomic packages to
    /// descendant inline elements, hand text geometry to descendant Text
    /// nodes, and place atomic inline boxes at their line positions.
    fn run_inline_ifc_root_after_place(
        &mut self,
        arena: &mut NodeArena,
        placement: LayoutPlacement,
        inner_width: f32,
    ) {
        if self.computed_style.layout != Layout::Inline || self.inline_ifc_owned_by_root {
            self.clear_inline_ifc_root_installs(arena);
            return;
        }
        if arena.find_by_stable_id(self.stable_id()).is_none() {
            self.clear_inline_ifc_root_installs(arena);
            return;
        }
        with_layout_place_profile(|profile| {
            profile.inline_ifc_root_install_calls += 1;
        });

        // 1. Measure stashed a fresh plan this frame (content/size changed):
        //    consume it.
        // 2. No pending and we already have an install with the same
        //    children: a pure move — reuse the cached plan, only origins
        //    changed. This is what makes dragging a window cheap.
        // 3. Otherwise (first install, structural change without a measure):
        //    shape from scratch.
        let (cache_key, children_snapshot, top_offset, content_size, plan) =
            if let Some(pending) = self.inline_ifc_layout_call_site.pending.take() {
                (
                    pending.cache_key,
                    pending.children_snapshot,
                    pending.content_top_offset,
                    pending.content_size,
                    pending.plan,
                )
            } else if let Some(mut install) = self.inline_ifc_layout_call_site.current.take() {
                let viewport_unchanged = install.viewport_width == placement.viewport_width
                    && install.viewport_height == placement.viewport_height;
                if install.children_snapshot == self.children && viewport_unchanged {
                    with_layout_place_profile(|profile| {
                        profile.inline_ifc_root_install_reuse_calls += 1;
                    });
                    // Same plan, so re-applying it can only change the
                    // origin shift baked into each absolute coordinate.
                    // Identical origins mean every installed value is
                    // already correct; a move is an in-place delta shift
                    // (no package clones, no absolute-geometry rebuild).
                    // Atomic boxes still go through child.place so their
                    // own placement gate decides whether to skip.
                    let origins = self.inline_ifc_apply_origins();
                    let has_atomic = install
                        .plan
                        .iter()
                        .any(|op| matches!(op, InlineIfcNodeInstallOp::Atomic { .. }));
                    if origins == install.applied_origins && !has_atomic {
                        self.inline_ifc_layout_call_site.current = Some(install);
                        return;
                    }
                    self.shift_inline_ifc_install_plan(
                        arena,
                        &install.plan,
                        origins.0 - install.applied_origins.0,
                        origins.1 - install.applied_origins.1,
                        install.content_top_offset,
                        placement,
                        has_atomic,
                    );
                    install.applied_origins = origins;
                    self.inline_ifc_layout_call_site.current = Some(install);
                    return;
                }
                // Children changed without a measure pass: fall through to a
                // full reshape, clearing the stale install first.
                for node_key in install.installed_nodes {
                    clear_inline_ifc_node_install(arena, node_key);
                }
                match self.compute_inline_ifc_plan(
                    arena,
                    inner_width,
                    placement.viewport_width,
                    placement.viewport_height,
                ) {
                    Some(built) => built,
                    None => {
                        self.dirty_flags = self.dirty_flags.union(DirtyPassMask::PAINT);
                        return;
                    }
                }
            } else {
                match self.compute_inline_ifc_plan(
                    arena,
                    inner_width,
                    placement.viewport_width,
                    placement.viewport_height,
                ) {
                    Some(built) => built,
                    None => return,
                }
            };

        let previously_installed: Vec<NodeKey> = self
            .inline_ifc_layout_call_site
            .current
            .take()
            .map(|install| install.installed_nodes)
            .unwrap_or_default();

        let installed_nodes =
            self.apply_inline_ifc_install_plan(arena, &plan, top_offset, placement);

        for stale_key in previously_installed {
            if installed_nodes.contains(&stale_key) {
                continue;
            }
            clear_inline_ifc_node_install(arena, stale_key);
        }

        self.inline_ifc_layout_call_site.current = Some(ElementInlineIfcRootInstall {
            cache_key,
            children_snapshot,
            content_top_offset: top_offset,
            content_size,
            inner_width,
            viewport_width: placement.viewport_width,
            viewport_height: placement.viewport_height,
            plan,
            installed_nodes,
            applied_origins: self.inline_ifc_apply_origins(),
        });

        // Children just moved; the scroll-content extent computed during
        // place_children is stale.
        let absolute_mask = self.compute_children_absolute_mask(arena);
        self.update_content_size_from_children(arena, &absolute_mask);
    }

    /// Shape the IFC and build an origin-independent install plan from
    /// scratch. Used by place when there is no measure-stashed plan and no
    /// reusable install (first install, or a structural change).
    fn compute_inline_ifc_plan(
        &mut self,
        arena: &NodeArena,
        inner_width: f32,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Option<(
        InlineIfcCacheKey,
        Vec<NodeKey>,
        f32,
        (f32, f32),
        Vec<InlineIfcNodeInstallOp>,
    )> {
        let root_key = arena.find_by_stable_id(self.stable_id())?;
        let collected = ElementInlineIfcMetadataCollector::collect_for_taken_root(
            arena,
            ElementInlineIfcMetadataCollectorInput::new(
                root_key,
                inner_width,
                viewport_width,
                viewport_height,
            ),
            self,
        )?;
        let candidate = self
            .inline_ifc_layout_call_site
            .cache
            .update(&collected.root_source);
        let geometry = inline_ifc_root_geometry(
            self.inline_ifc_layout_call_site
                .cache
                .context_for(&candidate.cache_key),
            arena,
            &collected.sources_by_node,
            root_key,
        )?;
        let top_offset = geometry.content_top_offset;
        let content_size = geometry.content_size;
        let plan = build_inline_ifc_install_plan(geometry, &collected, &candidate);
        Some((
            candidate.cache_key,
            self.children.clone(),
            top_offset,
            content_size,
            plan,
        ))
    }

    /// Origins every install apply bakes into absolute coordinates:
    /// (origin_x, origin_y, flow_origin_x, flow_origin_y), scroll folded.
    fn inline_ifc_apply_origins(&self) -> (f32, f32, f32, f32) {
        (
            self.layout_state.layout_inner_position.x - self.scroll_offset.x,
            self.layout_state.layout_inner_position.y - self.scroll_offset.y,
            self.layout_state.layout_flow_inner_position.x - self.scroll_offset.x,
            self.layout_state.layout_flow_inner_position.y - self.scroll_offset.y,
        )
    }

    /// Pure-move fast path for an unchanged install plan: shift owned
    /// span/text geometry in place by the origin delta and re-place
    /// atomic boxes at their line positions. Semantically identical to
    /// `apply_inline_ifc_install_plan` with the same plan, minus the
    /// per-op package clones and absolute-geometry rebuilds.
    #[allow(clippy::too_many_arguments)]
    fn shift_inline_ifc_install_plan(
        &mut self,
        arena: &mut NodeArena,
        plan: &[InlineIfcNodeInstallOp],
        dx: f32,
        dy: f32,
        top_offset: f32,
        placement: LayoutPlacement,
        has_atomic: bool,
    ) {
        let flow_origin_x = self.layout_state.layout_flow_inner_position.x - self.scroll_offset.x;
        let flow_origin_y = self.layout_state.layout_flow_inner_position.y - self.scroll_offset.y;
        let visual_offset_x =
            self.layout_state.layout_position.x - self.layout_state.layout_flow_position.x;
        let visual_offset_y =
            self.layout_state.layout_position.y - self.layout_state.layout_flow_position.y;
        if has_atomic {
            let child_parent_hit_test_clip = self.current_child_hit_test_clip_rect();
            self.push_hit_test_clip_scope(child_parent_hit_test_clip);
            let overscan = Self::SHOULD_RENDER_OVERSCAN_PX.max(0.0);
            self.push_child_clip_scope(Rect {
                x: self.layout_state.layout_inner_position.x - overscan,
                y: self.layout_state.layout_inner_position.y - overscan,
                width: (self.layout_state.layout_inner_size.width + overscan * 2.0).max(0.0),
                height: (self.layout_state.layout_inner_size.height + overscan * 2.0).max(0.0),
            });
        }
        let moved = dx != 0.0 || dy != 0.0;
        for op in plan {
            match op {
                InlineIfcNodeInstallOp::Span { node_key, .. } => {
                    if !moved {
                        continue;
                    }
                    arena.with_element_taken(*node_key, |child, _arena| {
                        if let Some(element) = child.as_any_mut().downcast_mut::<Element>() {
                            element.shift_inline_ifc_owned_geometry(dx, dy);
                        }
                    });
                }
                InlineIfcNodeInstallOp::Text { node_key, .. } => {
                    if !moved {
                        continue;
                    }
                    arena.with_element_taken(*node_key, |child, _arena| {
                        if let Some(text) = child.as_any_mut().downcast_mut::<Text>() {
                            text.shift_inline_ifc_owned_geometry(dx, dy);
                        }
                    });
                }
                InlineIfcNodeInstallOp::Atomic { node_key, rect } => {
                    arena.with_element_taken(*node_key, |child, arena| {
                        child.set_layout_offset(0.0, 0.0);
                        child.place(
                            LayoutPlacement {
                                parent_x: flow_origin_x + rect.x,
                                parent_y: flow_origin_y + rect.y - top_offset,
                                visual_offset_x,
                                visual_offset_y,
                                available_width: rect.width.max(1.0),
                                available_height: rect.height.max(1.0),
                                viewport_width: placement.viewport_width,
                                viewport_height: placement.viewport_height,
                                percent_base_width: placement.percent_base_width,
                                percent_base_height: placement.percent_base_height,
                            },
                            arena,
                        );
                    });
                }
            }
        }
        if has_atomic {
            self.pop_child_clip_scope();
            self.pop_hit_test_clip_scope();
            // Atomic boxes may have moved relative to this root; the
            // scroll-content extent from place_children is stale. Pure
            // span/text translation keeps relative extents unchanged.
            let absolute_mask = self.compute_children_absolute_mask(arena);
            self.update_content_size_from_children(arena, &absolute_mask);
        }
    }

    /// In-place delta shift of span geometry owned by an inline IFC
    /// root: decoration fragment positions, paint fragments, and the
    /// shell box adopted from the fragment union.
    pub(crate) fn shift_inline_ifc_owned_geometry(&mut self, dx: f32, dy: f32) {
        if let Some(package) = self
            .inline_ifc_rollout_packages
            .decoration_draw_rect
            .as_mut()
        {
            for fragment in &mut package.fragments {
                fragment.metadata.position[0] += dx;
                fragment.metadata.position[1] += dy;
            }
        }
        for rect in &mut self.inline_paint_fragments {
            rect.x += dx;
            rect.y += dy;
        }
        self.layout_state.layout_position.x += dx;
        self.layout_state.layout_position.y += dy;
        self.layout_state.layout_flow_position = self.layout_state.layout_position;
        self.layout_state.layout_inner_position = self.layout_state.layout_position;
    }

    /// Apply an origin-independent install plan: shift each op by the root's
    /// current origin and write the geometry into descendants. Returns the
    /// installed node keys (plan order).
    fn apply_inline_ifc_install_plan(
        &mut self,
        arena: &mut NodeArena,
        plan: &[InlineIfcNodeInstallOp],
        top_offset: f32,
        placement: LayoutPlacement,
    ) -> Vec<NodeKey> {
        let origin_x = self.layout_state.layout_inner_position.x - self.scroll_offset.x;
        let origin_y = self.layout_state.layout_inner_position.y - self.scroll_offset.y;
        let flow_origin_x = self.layout_state.layout_flow_inner_position.x - self.scroll_offset.x;
        let flow_origin_y = self.layout_state.layout_flow_inner_position.y - self.scroll_offset.y;
        let visual_offset_x =
            self.layout_state.layout_position.x - self.layout_state.layout_flow_position.x;
        let visual_offset_y =
            self.layout_state.layout_position.y - self.layout_state.layout_flow_position.y;

        // Children placed here must see the same clip scopes that
        // place_children gives in-flow children, or their should_render
        // and hit-test clips resolve against the wrong ancestor state.
        let child_parent_hit_test_clip = self.current_child_hit_test_clip_rect();
        self.push_hit_test_clip_scope(child_parent_hit_test_clip);
        let overscan = Self::SHOULD_RENDER_OVERSCAN_PX.max(0.0);
        self.push_child_clip_scope(Rect {
            x: self.layout_state.layout_inner_position.x - overscan,
            y: self.layout_state.layout_inner_position.y - overscan,
            width: (self.layout_state.layout_inner_size.width + overscan * 2.0).max(0.0),
            height: (self.layout_state.layout_inner_size.height + overscan * 2.0).max(0.0),
        });

        let mut installed_nodes = Vec::with_capacity(plan.len());
        for op in plan {
            installed_nodes.push(op.node_key());
            match op {
                InlineIfcNodeInstallOp::Span {
                    node_key,
                    package,
                    paint_fragments,
                } => {
                    let mut package = package.clone();
                    if let Some(package) = package
                        .as_mut()
                        .and_then(|package| package.decoration_draw_rect.as_mut())
                    {
                        for fragment in &mut package.fragments {
                            fragment.metadata.position[0] += origin_x;
                            fragment.metadata.position[1] += origin_y - top_offset;
                        }
                    }
                    let absolute = paint_fragments
                        .iter()
                        .map(|rect| Rect {
                            x: origin_x + rect.x,
                            y: origin_y + rect.y - top_offset,
                            width: rect.width,
                            height: rect.height,
                        })
                        .collect::<Vec<_>>();
                    let bounds = bounding_rect(
                        &absolute
                            .iter()
                            .map(|rect| crate::ui::Rect {
                                x: rect.x,
                                y: rect.y,
                                width: rect.width,
                                height: rect.height,
                            })
                            .collect::<Vec<_>>(),
                    );
                    arena.with_element_taken(*node_key, |child, _arena| {
                        if let Some(element) = child.as_any_mut().downcast_mut::<Element>() {
                            element.install_inline_ifc_rollout_packages_from_candidate(
                                package.as_ref(),
                            );
                            element.inline_ifc_owned_by_root = true;
                            element.place_as_inline_ifc_owned_box(bounds);
                            element.inline_paint_fragments = absolute;
                        }
                    });
                }
                InlineIfcNodeInstallOp::Text {
                    node_key,
                    lines,
                    paint_input,
                    paint_bounds,
                } => {
                    let absolute = lines
                        .iter()
                        .cloned()
                        .map(|line| line.shifted(origin_x, origin_y - top_offset))
                        .collect::<Vec<_>>();
                    arena.with_element_taken(*node_key, |child, _arena| {
                        if let Some(text) = child.as_any_mut().downcast_mut::<Text>() {
                            let mut bounds = bounding_rect(
                                &absolute.iter().map(|line| line.rect).collect::<Vec<_>>(),
                            );
                            if (bounds.width <= 0.0 || bounds.height <= 0.0)
                                && paint_bounds.width > 0.0
                                && paint_bounds.height > 0.0
                            {
                                bounds = crate::ui::Rect {
                                    x: origin_x + paint_bounds.x,
                                    y: origin_y + paint_bounds.y - top_offset,
                                    width: paint_bounds.width,
                                    height: paint_bounds.height,
                                };
                            }
                            text.place_as_inline_ifc_owned_box(bounds);
                            text.install_inline_ifc_owned_geometry(
                                absolute,
                                Arc::clone(paint_input),
                                crate::ui::Rect {
                                    x: origin_x + paint_bounds.x,
                                    y: origin_y + paint_bounds.y - top_offset,
                                    width: paint_bounds.width,
                                    height: paint_bounds.height,
                                },
                            );
                        }
                    });
                }
                InlineIfcNodeInstallOp::Atomic { node_key, rect } => {
                    arena.with_element_taken(*node_key, |child, arena| {
                        // Bake the line placement into the parent origin:
                        // not every atomic host honours set_layout_offset
                        // (TextAreaTextRun places at parent + visual only).
                        child.set_layout_offset(0.0, 0.0);
                        child.place(
                            LayoutPlacement {
                                parent_x: flow_origin_x + rect.x,
                                parent_y: flow_origin_y + rect.y - top_offset,
                                visual_offset_x,
                                visual_offset_y,
                                available_width: rect.width.max(1.0),
                                available_height: rect.height.max(1.0),
                                viewport_width: placement.viewport_width,
                                viewport_height: placement.viewport_height,
                                percent_base_width: placement.percent_base_width,
                                percent_base_height: placement.percent_base_height,
                            },
                            arena,
                        );
                    });
                }
            }
        }

        self.pop_child_clip_scope();
        self.pop_hit_test_clip_scope();
        installed_nodes
    }

    /// Shell placement for a node whose geometry is owned by an inline
    /// IFC root: adopt the bounding box so arena hit-testing and bbox
    /// queries see the fragment union, without running a layout pass.
    pub(crate) fn place_as_inline_ifc_owned_box(&mut self, bounds: crate::ui::Rect) {
        self.layout_state.layout_position = Position {
            x: bounds.x,
            y: bounds.y,
        };
        self.layout_state.layout_flow_position = self.layout_state.layout_position;
        self.layout_state.layout_inner_position = self.layout_state.layout_position;
        self.layout_state.layout_size = Size {
            width: bounds.width,
            height: bounds.height,
        };
        self.layout_state.layout_inner_size = self.layout_state.layout_size;
        self.layout_state.should_render = bounds.width > 0.0 && bounds.height > 0.0;
        // The install is this owned node's placement pass: clear the same
        // bits `Element::place` clears, or the stale local PLACEMENT dirt
        // keeps every ancestor's subtree aggregate dirty forever and the
        // whole tree re-places (and re-installs) on every frame.
        self.dirty_flags = self.dirty_flags.without(DirtyPassMask::PLACEMENT);
    }

    /// Tear down a previous install when this element stops being an
    /// inline IFC root (layout switch, children removed, collector miss).
    fn clear_inline_ifc_root_installs(&mut self, arena: &mut NodeArena) {
        let Some(install) = self.inline_ifc_layout_call_site.current.take() else {
            return;
        };
        for node_key in install.installed_nodes {
            clear_inline_ifc_node_install(arena, node_key);
        }
        self.dirty_flags = self.dirty_flags.union(DirtyPassMask::PAINT);
    }

    /// Shape (or reuse) the candidate during measure and return the IFC
    /// content size this inline root should adopt as its auto size
    /// (insets not included).
    fn measure_inline_ifc_root_content_size(
        &mut self,
        arena: &NodeArena,
        inner_width: f32,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Option<(f32, f32)> {
        if self.computed_style.layout != Layout::Inline || self.inline_ifc_owned_by_root {
            self.inline_ifc_layout_call_site.pending = None;
            return None;
        }
        // Cheapest path: the install is still valid (same children, same
        // width, and no descendant changed content/size) — the IFC shaping
        // is unchanged, so skip collect + cache hashing entirely. This is
        // what makes re-measuring an inline root during an ancestor move
        // (where the subtree is layout-clean) effectively free.
        if let Some(install) = self.inline_ifc_layout_call_site.current.as_ref() {
            let width_unchanged = (install.inner_width - inner_width).abs() <= f32::EPSILON;
            let viewport_unchanged = install.viewport_width == viewport_width
                && install.viewport_height == viewport_height;
            let layout_clean = !self.dirty_flags.intersects(DirtyPassMask::LAYOUT)
                && !self.children.iter().any(|&child_key| {
                    arena.subtree_dirty_intersects(child_key, DirtyPassMask::LAYOUT)
                });
            if install.children_snapshot == self.children
                && width_unchanged
                && viewport_unchanged
                && layout_clean
            {
                self.inline_ifc_layout_call_site.pending = None;
                LAYOUT_PLACE_PROFILE.with(|p| p.borrow_mut().ifc_measure_cheap += 1);
                return Some(install.content_size);
            }
        }
        let Some(root_key) = arena.find_by_stable_id(self.stable_id()) else {
            self.inline_ifc_layout_call_site.pending = None;
            return None;
        };
        let Some(collected) = ElementInlineIfcMetadataCollector::collect_for_taken_root(
            arena,
            ElementInlineIfcMetadataCollectorInput::new(
                root_key,
                inner_width,
                viewport_width,
                viewport_height,
            ),
            self,
        ) else {
            self.inline_ifc_layout_call_site.pending = None;
            return None;
        };
        let candidate = self
            .inline_ifc_layout_call_site
            .cache
            .update(&collected.root_source);
        // If the shaping and children are identical to the current install,
        // the geometry/plan are unchanged — return the cached content size
        // and leave `pending` clear so place reuses the existing plan. This
        // is the common case while only positions change around the root.
        if let Some(install) = self.inline_ifc_layout_call_site.current.as_ref() {
            if install.cache_key == candidate.cache_key
                && install.children_snapshot == self.children
            {
                self.inline_ifc_layout_call_site.pending = None;
                LAYOUT_PLACE_PROFILE.with(|p| p.borrow_mut().ifc_measure_shortcircuit += 1);
                return Some(install.content_size);
            }
        }
        LAYOUT_PLACE_PROFILE.with(|p| p.borrow_mut().ifc_measure_full += 1);
        let Some(geometry) = inline_ifc_root_geometry(
            self.inline_ifc_layout_call_site
                .cache
                .context_for(&candidate.cache_key),
            arena,
            &collected.sources_by_node,
            root_key,
        ) else {
            self.inline_ifc_layout_call_site.pending = None;
            return None;
        };
        // Stash the origin-independent plan so the upcoming place phase
        // reuses this shaping instead of redoing collect + geometry.
        let content_size = geometry.content_size;
        let top_offset = geometry.content_top_offset;
        let plan = build_inline_ifc_install_plan(geometry, &collected, &candidate);
        self.inline_ifc_layout_call_site.pending = Some(ElementInlineIfcPendingPlan {
            cache_key: candidate.cache_key,
            children_snapshot: self.children.clone(),
            content_top_offset: top_offset,
            content_size,
            plan,
        });
        Some(content_size)
    }

    /// Test diagnostic for the full root-shaped payload before it is split
    /// into source-filtered Text passes.
    #[cfg(test)]
    pub(crate) fn inline_ifc_root_render_input(
        &self,
    ) -> Option<(InlineIfcTextPassPaintInput, f32)> {
        let install = self.inline_ifc_layout_call_site.current.as_ref()?;
        let context = self
            .inline_ifc_layout_call_site
            .cache
            .context_for(&install.cache_key)?;
        Some((context.text_pass_paint_input(), install.content_top_offset))
    }

    /// Test diagnostic staging input for the full root payload, with
    /// content-top normalization applied. Prepared passes position glyphs from
    /// `paint.local_pos` plus the fragment origin; `final_paint_pos` only
    /// feeds probes, so both must carry the offset to stay in sync.
    #[cfg(test)]
    pub(crate) fn inline_ifc_root_staging_input(
        &self,
        origin: [f32; 2],
        opacity: f32,
    ) -> Option<crate::view::render_pass::text_pass::TextPassPreparedStagingInput> {
        let install = self.inline_ifc_layout_call_site.current.as_ref()?;
        let context = self
            .inline_ifc_layout_call_site
            .cache
            .context_for(&install.cache_key)?;
        let top_offset = install.content_top_offset;
        let mut staging_input = inline_ifc_paint_input_to_text_pass_staging_input(
            context.text_pass_paint_input_ref(),
            origin,
            opacity,
            0,
            1.0,
        );
        for staged in &mut staging_input.glyphs {
            staged.paint.local_pos[1] -= top_offset;
            staged.final_paint_pos[1] -= top_offset;
        }
        Some(staging_input)
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
            font_families: self.computed_style.font_families.clone().into(),
            vertical_align: self.computed_style.vertical_align,
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
        let style = InlineIfcElementDecorationDrawRectStyle::new_with_side_colors(
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
            [
                self.border_colors.left.as_ref().to_rgba_f32(),
                self.border_colors.right.as_ref().to_rgba_f32(),
                self.border_colors.top.as_ref().to_rgba_f32(),
                self.border_colors.bottom.as_ref().to_rgba_f32(),
            ],
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

    fn placement_eligibility_metadata(
        &self,
    ) -> crate::view::node_arena::PlacementEligibilityMetadata {
        self.local_placement_eligibility_metadata()
    }

    fn last_placement(&self) -> Option<LayoutPlacement> {
        self.last_layout_placement
    }

    fn hit_test_clip_rect(&self) -> Option<Rect> {
        self.hit_test_clip_rect
    }

    fn translate_in_place(&mut self, dx: f32, dy: f32) {
        self.translate_placed_geometry(dx, dy);
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

    fn promotion_requires_mask_surface(&self, arena: &crate::view::node_arena::NodeArena) -> bool {
        if self.children.is_empty() {
            return false;
        }
        let overflow_child_indices: Vec<bool> = (0..self.children.len())
            .map(|idx| self.child_renders_outside_inner_clip(idx, arena))
            .collect();
        let outer_radii = normalize_corner_radii(
            self.border_radii,
            self.layout_state.layout_size.width.max(0.0),
            self.layout_state.layout_size.height.max(0.0),
        );
        let inner_radii = self.inner_clip_radii(outer_radii);
        inner_radii.has_any_rounding()
            && self.should_clip_children(&overflow_child_indices, inner_radii, arena)
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

    fn sync_children_mirror(&mut self, children: &[crate::view::node_arena::NodeKey]) {
        self.children.clear();
        self.children.extend_from_slice(children);
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
