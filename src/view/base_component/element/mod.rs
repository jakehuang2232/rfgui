#![allow(missing_docs)]
use rustc_hash::{FxHashMap, FxHashSet};

use super::{
    ComputedStyleConsumer, ElementCore, Image, Position, Size, Svg, Text,
    TextInlineIfcStyleMetadata,
};
use crate::style::ColorLike;
use crate::style::{
    Align, AnchorName, BoxShadow, ClipMode, Collision, CollisionBoundary, Color, ComputedStyle,
    Cursor, FlowDirection, FlowWrap, JustifyContent, Layout, Length, PositionMode, ScrollDirection,
    SizeValue, Style, StyleComputeContext, TextWrap, Transform, TransformKind, TransformOrigin,
    TransitionProperty, TransitionTiming, VerticalAlign, compute_style_with_context,
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
use crate::view::frame_graph::{
    AttachmentTarget, FrameGraph, PersistentTextureKey, RetainedTextureRole, TextureDesc,
};
use crate::view::inline_formatting_context::{
    InlineFormattingContext, InlineIfcAtomicBoxPlacement, InlineIfcAtomicBoxPlacementPackage,
    InlineIfcAtomicMeasureConstraints, InlineIfcAtomicSizingRules, InlineIfcCacheKey,
    InlineIfcDecorationBoxInsets, InlineIfcDistributedElementPackages,
    InlineIfcElementDecorationDrawRectPackage, InlineIfcElementDecorationDrawRectStyle,
    InlineIfcElementDecorationPackageSource, InlineIfcElementRootCandidate,
    InlineIfcElementRootCandidateCache, InlineIfcElementRootSource,
    InlineIfcElementRootSourceBuilder, InlineIfcIntrinsicSize, InlineIfcItem,
    InlineIfcMeasuredAtomicBox, InlineIfcPaintRect, InlineIfcPercentBase, InlineIfcSize,
    InlineIfcSourceId, InlineIfcSourceKind, InlineIfcStyle, InlineIfcTextLayoutSnapshot,
    InlineIfcTextPassPaintInput,
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
use slotmap::Key;
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
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

fn emit_draw_rect_io_pass<P: GraphicsPass + DrawRectIoPass + 'static>(
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
    mut pass: P,
) {
    let input = ctx.current_target().unwrap_or_else(|| {
        let target = ctx.allocate_target(graph);
        ctx.set_current_target(target);
        target
    });
    if let Some(handle) = input.handle() {
        pass.draw_rect_input_mut().render_target = RenderTargetIn::with_handle(handle);
    }
    pass.draw_rect_input_mut().pass_context = ctx.graphics_pass_context();
    pass.set_scissor_rect(ctx.scissor_rect());
    pass.draw_rect_output_mut().render_target = input;
    graph.add_graphics_pass(pass);
    ctx.set_current_target(input);
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

/// Configured interaction and scrollbar axes owned by one layout-backed
/// scroll container snapshot.
///
/// `None` is deliberately absent: a host that is not a scroll container must
/// return no [`ScrollGeometrySnapshot`] at all. This value is not a
/// translation mask: projection must always consume the complete 2D offset.
#[doc(hidden)]
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAxisSnapshot {
    Vertical,
    Horizontal,
    Both,
}

/// Exact logical contents clip observed from the legacy layout/paint path.
///
/// M10E0 only admits rectangular scissor-backed scroll containers. Rounded
/// stencil clips remain unsupported rather than being approximated here.
#[doc(hidden)]
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollContentsClipWitness {
    ExactRect([u32; 4]),
}

/// Scrollbar interaction state sampled without consulting wall-clock time.
#[doc(hidden)]
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollbarInteractionWitness {
    pub hovered: bool,
    pub dragging_axis: Option<ScrollAxisSnapshot>,
    pub has_interaction_timestamp: bool,
}

/// Exact scrollbar paint state frozen from the viewport-owned frame sample.
#[doc(hidden)]
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarPaintStateWitness {
    NotPaintable,
    HiddenNow,
    OpaqueNow,
    TranslucentNow,
}

/// Deterministic legacy scrollbar geometry plus its exact sampled paint state.
#[doc(hidden)]
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct ScrollbarOverlayWitness {
    pub vertical_track: Option<Rect>,
    pub vertical_thumb: Option<Rect>,
    pub horizontal_track: Option<Rect>,
    pub horizontal_thumb: Option<Rect>,
    pub interaction: ScrollbarInteractionWitness,
    pub paint_state: ScrollbarPaintStateWitness,
    /// Exact alpha frozen by the viewport-owned frame-time sample.
    pub sampled_alpha: f32,
    pub shadow_blur_radius: f32,
}

pub(crate) const SCROLLBAR_THICKNESS: f32 = 6.0;
pub(crate) const SCROLLBAR_MARGIN: f32 = 3.0;
pub(crate) const SCROLLBAR_MIN_THUMB: f32 = 24.0;

/// Reconstructs the exact legacy vertical scrollbar geometry without reading
/// element state or wall-clock time. Both legacy paint and retained-scroll
/// validation use this helper so the compiler cannot accept a merely
/// plausible track/thumb pair.
pub(crate) fn canonical_vertical_scrollbar_geometry(
    viewport: Rect,
    content_height: f32,
    scroll_offset_y: f32,
    reserve_horizontal_scrollbar: bool,
) -> Option<(Rect, Rect)> {
    let max_scroll_y = (content_height - viewport.height).max(0.0);
    if max_scroll_y <= 0.0 {
        return None;
    }
    let reserve_h = if reserve_horizontal_scrollbar {
        SCROLLBAR_THICKNESS + SCROLLBAR_MARGIN
    } else {
        0.0
    };
    let track_x = viewport.x + viewport.width - SCROLLBAR_THICKNESS - SCROLLBAR_MARGIN;
    let track_y = viewport.y + SCROLLBAR_MARGIN;
    let track_h = (viewport.height - SCROLLBAR_MARGIN * 2.0 - reserve_h).max(0.0);
    if track_h <= 0.0 {
        return None;
    }
    let track = Rect {
        x: track_x,
        y: track_y,
        width: SCROLLBAR_THICKNESS,
        height: track_h,
    };
    let ratio = (viewport.height / content_height.max(1.0)).clamp(0.0, 1.0);
    let thumb_h = (track_h * ratio).clamp(SCROLLBAR_MIN_THUMB.min(track_h), track_h);
    let travel = (track_h - thumb_h).max(0.0);
    let thumb_offset = if max_scroll_y > 0.0 {
        (scroll_offset_y / max_scroll_y).clamp(0.0, 1.0) * travel
    } else {
        0.0
    };
    Some((
        track,
        Rect {
            x: track.x,
            y: track.y + thumb_offset,
            width: track.width,
            height: thumb_h,
        },
    ))
}

/// Owning, backend-independent observation of one layout scroll container.
///
/// `layout_content_bounds_at_zero` is the layout scroll extent projected at
/// offset zero. It is not paint overflow and must not be used as raster bounds.
#[doc(hidden)]
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct ScrollGeometrySnapshot {
    /// Configured input/scrollbar axes. Never mask either offset component
    /// with this field; scroll projection consumes the full 2D offset.
    pub configured_axis: ScrollAxisSnapshot,
    pub offset: [f32; 2],
    pub scrollport_rect: Rect,
    pub content_size: [f32; 2],
    pub layout_content_bounds_at_zero: Rect,
    pub contents_clip: ScrollContentsClipWitness,
    pub scrollbar_overlay: ScrollbarOverlayWitness,
}

/// Exact private-style admission frozen by the built-in `Element` host for
/// the first retained scroll-host canary. This is border-box raster geometry,
/// never the scroll content extent.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RetainedScrollHostAdmissionSnapshot {
    pub(crate) boundary_root: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) child: NodeKey,
    pub(crate) child_stable_id: u64,
    pub(crate) source_bounds: PromotionCompositeBounds,
    /// Complete live layout/paint observation frozen by admission. Planning
    /// must prove it matches the independently synchronized property-tree
    /// snapshot before recording an empty scrollbar phase.
    pub(crate) scroll: ScrollGeometrySnapshot,
}

/// Exact sibling admission for the first property-scroll TextArea subtree.
///
/// This deliberately does not widen `RetainedScrollHostAdmissionSnapshot` or
/// its direct-leaf oracle.  The admitted grammar is one scroll host, one
/// otherwise leaf-equivalent Element content wrapper, and one plain TextArea
/// subtree rooted at `text_area_root`. The frozen paint grammar distinguishes
/// C1 glyph-only content from C2a selection-underlay plus glyph content.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RetainedScrollTextAreaSubtreeAdmissionSnapshot {
    pub(crate) boundary_root: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) content_wrapper: NodeKey,
    pub(crate) content_wrapper_stable_id: u64,
    pub(crate) text_area_root: NodeKey,
    pub(crate) text_area_stable_id: u64,
    pub(crate) paint_grammar: super::text_area::RetainedTextAreaPaintGrammar,
    pub(crate) source_bounds: PromotionCompositeBounds,
    pub(crate) scroll: ScrollGeometrySnapshot,
}

/// Graph-inert C3a sibling admission for one realized atomic TextArea
/// projection whose user subtree is exactly one bare static Text leaf.
/// Recorder/compiler/scene authority intentionally does not consume this
/// snapshot yet.  A future recorder must rerun the live TextArea source
/// oracle both before and after recording and require complete grammar
/// equality; this snapshot alone is not paint authority.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Graph-inert until the separately reviewed C3a recorder segment.
pub(crate) struct RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot {
    pub(crate) boundary_root: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) content_wrapper: NodeKey,
    pub(crate) content_wrapper_stable_id: u64,
    pub(crate) text_area_root: NodeKey,
    pub(crate) text_area_stable_id: u64,
    pub(crate) paint_grammar: super::text_area::RetainedAtomicProjectionTextAreaPaintGrammar,
    pub(crate) source_bounds: PromotionCompositeBounds,
    pub(crate) scroll: ScrollGeometrySnapshot,
}

/// Graph-inert sibling admission for one root-owned nonempty TextArea
/// selection and one realized atomic projection.  This does not widen the
/// existing glyph-only atomic admission or any production scene selector.
#[derive(Clone, Debug)]
pub(crate) struct RetainedScrollAtomicProjectionSelectionTextAreaSubtreeAdmissionSnapshot {
    pub(crate) boundary_root: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) content_wrapper: NodeKey,
    pub(crate) content_wrapper_stable_id: u64,
    pub(crate) text_area_root: NodeKey,
    pub(crate) text_area_stable_id: u64,
    pub(crate) paint_grammar:
        super::text_area::RetainedAtomicProjectionSelectionTextAreaPaintGrammar,
    pub(crate) source_bounds: PromotionCompositeBounds,
    pub(crate) scroll: ScrollGeometrySnapshot,
}

/// Graph-inert focused-glyph sibling for exactly one realized atomic
/// projection.  The source grammar includes the post-children caret source
/// fact, but this admission does not select or compile a retained scene.
#[derive(Clone, Debug)]
pub(crate) struct RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot {
    pub(crate) boundary_root: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) content_wrapper: NodeKey,
    pub(crate) content_wrapper_stable_id: u64,
    pub(crate) text_area_root: NodeKey,
    pub(crate) text_area_stable_id: u64,
    pub(crate) paint_grammar: super::text_area::RetainedFocusedAtomicProjectionTextAreaPaintGrammar,
    pub(crate) source_bounds: PromotionCompositeBounds,
    pub(crate) scroll: ScrollGeometrySnapshot,
}

/// Exact sibling admission for focused plain TextArea retention. Its resident
/// base grammar excludes caret paint; the dynamic caret overlay is sealed by
/// the recorder/compiler chain.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot {
    pub(crate) boundary_root: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) content_wrapper: NodeKey,
    pub(crate) content_wrapper_stable_id: u64,
    pub(crate) text_area_root: NodeKey,
    pub(crate) text_area_stable_id: u64,
    pub(crate) paint_grammar: super::text_area::RetainedInteractiveTextAreaPaintGrammar,
    /// Independent source-oracle geometry. `None` is the exact hidden-caret
    /// result; `Some` is the caret-map-derived live bounds before clipping.
    pub(crate) caret_oracle_bounds_bits: Option<[u32; 4]>,
    pub(crate) source_bounds: PromotionCompositeBounds,
    pub(crate) scroll: ScrollGeometrySnapshot,
}

/// Exact admission for the first direct `ScrollContents -> Transform`
/// migration shape.  This deliberately has a sibling type instead of
/// widening `RetainedScrollHostAdmissionSnapshot`: the original B0 oracle
/// must continue to mean an untransformed content leaf.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RetainedScrollTransformHostAdmissionSnapshot {
    pub(crate) boundary_root: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) transform_content: NodeKey,
    pub(crate) transform_content_stable_id: u64,
    pub(crate) source_bounds: PromotionCompositeBounds,
    pub(crate) scroll: ScrollGeometrySnapshot,
}

/// Exact admission for the first bounded nested-scroll scene:
/// `S0 -> S1 -> leaf`.  This remains a sibling of the single-scroll B0
/// witness so neither the root oracle nor its untransformed-leaf meaning can
/// be widened accidentally.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RetainedNestedScrollSceneAdmissionSnapshot {
    pub(crate) outer_boundary_root: NodeKey,
    pub(crate) outer_stable_id: u64,
    pub(crate) inner_boundary_root: NodeKey,
    pub(crate) inner_stable_id: u64,
    pub(crate) content_leaf: NodeKey,
    pub(crate) content_leaf_stable_id: u64,
    pub(crate) outer_source_bounds: PromotionCompositeBounds,
    pub(crate) inner_source_bounds: PromotionCompositeBounds,
    pub(crate) outer_scroll: ScrollGeometrySnapshot,
    pub(crate) inner_scroll: ScrollGeometrySnapshot,
}

impl RetainedNestedScrollSceneAdmissionSnapshot {
    pub(crate) fn bitwise_eq(self, other: Self) -> bool {
        self.outer_boundary_root == other.outer_boundary_root
            && self.outer_stable_id == other.outer_stable_id
            && self.inner_boundary_root == other.inner_boundary_root
            && self.inner_stable_id == other.inner_stable_id
            && self.content_leaf == other.content_leaf
            && self.content_leaf_stable_id == other.content_leaf_stable_id
            && scroll_geometry_snapshots_bitwise_equal(self.outer_scroll, other.outer_scroll)
            && scroll_geometry_snapshots_bitwise_equal(self.inner_scroll, other.inner_scroll)
            && promotion_composite_bounds_bitwise_equal(
                self.outer_source_bounds,
                other.outer_source_bounds,
            )
            && promotion_composite_bounds_bitwise_equal(
                self.inner_source_bounds,
                other.inner_source_bounds,
            )
    }

    pub(crate) fn matches_scroll_nodes(
        self,
        outer: crate::view::compositor::property_tree::ScrollNodeSnapshot,
        inner: crate::view::compositor::property_tree::ScrollNodeSnapshot,
    ) -> bool {
        outer.id.0 == self.outer_boundary_root
            && outer.owner == self.outer_boundary_root
            && inner.id.0 == self.inner_boundary_root
            && inner.owner == self.inner_boundary_root
            && scroll_geometry_snapshot_matches_scroll_node(self.outer_scroll, outer)
            && scroll_geometry_snapshot_matches_scroll_node(self.inner_scroll, inner)
    }
}

fn promotion_composite_bounds_bitwise_equal(
    left: PromotionCompositeBounds,
    right: PromotionCompositeBounds,
) -> bool {
    [left.x, left.y, left.width, left.height].map(f32::to_bits)
        == [right.x, right.y, right.width, right.height].map(f32::to_bits)
        && left.corner_radii.map(f32::to_bits) == right.corner_radii.map(f32::to_bits)
}

impl RetainedScrollTransformHostAdmissionSnapshot {
    pub(crate) fn bitwise_eq(self, other: Self) -> bool {
        self.boundary_root == other.boundary_root
            && self.stable_id == other.stable_id
            && self.transform_content == other.transform_content
            && self.transform_content_stable_id == other.transform_content_stable_id
            && scroll_geometry_snapshots_bitwise_equal(self.scroll, other.scroll)
            && [
                self.source_bounds.x,
                self.source_bounds.y,
                self.source_bounds.width,
                self.source_bounds.height,
            ]
            .map(f32::to_bits)
                == [
                    other.source_bounds.x,
                    other.source_bounds.y,
                    other.source_bounds.width,
                    other.source_bounds.height,
                ]
                .map(f32::to_bits)
            && self.source_bounds.corner_radii.map(f32::to_bits)
                == other.source_bounds.corner_radii.map(f32::to_bits)
    }

    pub(crate) fn matches_scroll_node(
        self,
        snapshot: crate::view::compositor::property_tree::ScrollNodeSnapshot,
    ) -> bool {
        scroll_geometry_snapshot_matches_scroll_node(self.scroll, snapshot)
    }
}

impl RetainedScrollHostAdmissionSnapshot {
    pub(crate) fn bitwise_eq(self, other: Self) -> bool {
        self.boundary_root == other.boundary_root
            && self.stable_id == other.stable_id
            && self.child == other.child
            && self.child_stable_id == other.child_stable_id
            && scroll_geometry_snapshots_bitwise_equal(self.scroll, other.scroll)
            && [
                self.source_bounds.x,
                self.source_bounds.y,
                self.source_bounds.width,
                self.source_bounds.height,
            ]
            .map(f32::to_bits)
                == [
                    other.source_bounds.x,
                    other.source_bounds.y,
                    other.source_bounds.width,
                    other.source_bounds.height,
                ]
                .map(f32::to_bits)
            && self.source_bounds.corner_radii.map(f32::to_bits)
                == other.source_bounds.corner_radii.map(f32::to_bits)
    }

    pub(crate) fn matches_scroll_node(
        self,
        snapshot: crate::view::compositor::property_tree::ScrollNodeSnapshot,
    ) -> bool {
        scroll_geometry_snapshot_matches_scroll_node(self.scroll, snapshot)
    }
}

impl RetainedScrollTextAreaSubtreeAdmissionSnapshot {
    pub(crate) fn bitwise_eq(self, other: Self) -> bool {
        self.boundary_root == other.boundary_root
            && self.stable_id == other.stable_id
            && self.content_wrapper == other.content_wrapper
            && self.content_wrapper_stable_id == other.content_wrapper_stable_id
            && self.text_area_root == other.text_area_root
            && self.text_area_stable_id == other.text_area_stable_id
            && self.paint_grammar.is_canonical()
            && other.paint_grammar.is_canonical()
            && self.paint_grammar == other.paint_grammar
            && scroll_geometry_snapshots_bitwise_equal(self.scroll, other.scroll)
            && promotion_composite_bounds_bitwise_equal(self.source_bounds, other.source_bounds)
    }

    pub(crate) fn matches_scroll_node(
        self,
        snapshot: crate::view::compositor::property_tree::ScrollNodeSnapshot,
    ) -> bool {
        scroll_geometry_snapshot_matches_scroll_node(self.scroll, snapshot)
    }
}

impl RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot {
    pub(crate) fn bitwise_eq(&self, other: &Self) -> bool {
        self.boundary_root == other.boundary_root
            && self.stable_id == other.stable_id
            && self.content_wrapper == other.content_wrapper
            && self.content_wrapper_stable_id == other.content_wrapper_stable_id
            && self.text_area_root == other.text_area_root
            && self.text_area_stable_id == other.text_area_stable_id
            && self.paint_grammar.is_canonical()
            && other.paint_grammar.is_canonical()
            && self.paint_grammar == other.paint_grammar
            && scroll_geometry_snapshots_bitwise_equal(self.scroll, other.scroll)
            && promotion_composite_bounds_bitwise_equal(self.source_bounds, other.source_bounds)
    }

    #[allow(dead_code)] // Graph-inert until a reviewed scene selector consumes this sibling.
    pub(crate) fn matches_scroll_node(
        &self,
        snapshot: crate::view::compositor::property_tree::ScrollNodeSnapshot,
    ) -> bool {
        self.paint_grammar.is_canonical()
            && scroll_geometry_snapshot_matches_scroll_node(self.scroll, snapshot)
    }
}

impl RetainedScrollAtomicProjectionSelectionTextAreaSubtreeAdmissionSnapshot {
    #[allow(dead_code)] // Graph-inert until a reviewed scene selector consumes this sibling.
    pub(crate) fn bitwise_eq(&self, other: &Self) -> bool {
        self.boundary_root == other.boundary_root
            && self.stable_id == other.stable_id
            && self.content_wrapper == other.content_wrapper
            && self.content_wrapper_stable_id == other.content_wrapper_stable_id
            && self.text_area_root == other.text_area_root
            && self.text_area_stable_id == other.text_area_stable_id
            && self.paint_grammar.is_canonical()
            && other.paint_grammar.is_canonical()
            && self.paint_grammar == other.paint_grammar
            && scroll_geometry_snapshots_bitwise_equal(self.scroll, other.scroll)
            && promotion_composite_bounds_bitwise_equal(self.source_bounds, other.source_bounds)
    }

    pub(crate) fn matches_scroll_node(
        &self,
        snapshot: crate::view::compositor::property_tree::ScrollNodeSnapshot,
    ) -> bool {
        self.paint_grammar.is_canonical()
            && scroll_geometry_snapshot_matches_scroll_node(self.scroll, snapshot)
    }
}

impl RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot {
    #[allow(dead_code)] // Graph-inert until a reviewed scene selector consumes this sibling.
    pub(crate) fn bitwise_eq(&self, other: &Self) -> bool {
        self.boundary_root == other.boundary_root
            && self.stable_id == other.stable_id
            && self.content_wrapper == other.content_wrapper
            && self.content_wrapper_stable_id == other.content_wrapper_stable_id
            && self.text_area_root == other.text_area_root
            && self.text_area_stable_id == other.text_area_stable_id
            && self.paint_grammar.is_canonical()
            && other.paint_grammar.is_canonical()
            && self.paint_grammar == other.paint_grammar
            && scroll_geometry_snapshots_bitwise_equal(self.scroll, other.scroll)
            && promotion_composite_bounds_bitwise_equal(self.source_bounds, other.source_bounds)
    }

    #[allow(dead_code)] // Graph-inert until a reviewed scene selector consumes this sibling.
    pub(crate) fn matches_scroll_node(
        &self,
        snapshot: crate::view::compositor::property_tree::ScrollNodeSnapshot,
    ) -> bool {
        self.paint_grammar.is_canonical()
            && scroll_geometry_snapshot_matches_scroll_node(self.scroll, snapshot)
    }
}

impl RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot {
    pub(crate) fn bitwise_eq(self, other: Self) -> bool {
        self.boundary_root == other.boundary_root
            && self.stable_id == other.stable_id
            && self.content_wrapper == other.content_wrapper
            && self.content_wrapper_stable_id == other.content_wrapper_stable_id
            && self.text_area_root == other.text_area_root
            && self.text_area_stable_id == other.text_area_stable_id
            && self.paint_grammar.is_canonical()
            && other.paint_grammar.is_canonical()
            && self.paint_grammar == other.paint_grammar
            && self.caret_oracle_bounds_bits == other.caret_oracle_bounds_bits
            && scroll_geometry_snapshots_bitwise_equal(self.scroll, other.scroll)
            && promotion_composite_bounds_bitwise_equal(self.source_bounds, other.source_bounds)
    }

    pub(crate) fn matches_scroll_node(
        self,
        snapshot: crate::view::compositor::property_tree::ScrollNodeSnapshot,
    ) -> bool {
        scroll_geometry_snapshot_matches_scroll_node(self.scroll, snapshot)
    }
}

fn scroll_geometry_snapshot_matches_scroll_node(
    live: ScrollGeometrySnapshot,
    snapshot: crate::view::compositor::property_tree::ScrollNodeSnapshot,
) -> bool {
    live.configured_axis == snapshot.configured_axis
        && live.offset.map(f32::to_bits)
            == [snapshot.offset.x.to_bits(), snapshot.offset.y.to_bits()]
        && rects_bitwise_equal(live.scrollport_rect, snapshot.viewport)
        && live.content_size.map(f32::to_bits)
            == [
                snapshot.content_size.width.to_bits(),
                snapshot.content_size.height.to_bits(),
            ]
        && rects_bitwise_equal(
            live.layout_content_bounds_at_zero,
            snapshot.layout_content_bounds_at_zero,
        )
        && live.contents_clip == snapshot.contents_clip
        && scrollbar_overlays_bitwise_equal(live.scrollbar_overlay, snapshot.scrollbar_overlay)
}

fn rects_bitwise_equal(lhs: Rect, rhs: Rect) -> bool {
    [lhs.x, lhs.y, lhs.width, lhs.height].map(f32::to_bits)
        == [rhs.x, rhs.y, rhs.width, rhs.height].map(f32::to_bits)
}

fn optional_rects_bitwise_equal(lhs: Option<Rect>, rhs: Option<Rect>) -> bool {
    match (lhs, rhs) {
        (Some(lhs), Some(rhs)) => rects_bitwise_equal(lhs, rhs),
        (None, None) => true,
        _ => false,
    }
}

fn scrollbar_overlays_bitwise_equal(
    lhs: ScrollbarOverlayWitness,
    rhs: ScrollbarOverlayWitness,
) -> bool {
    optional_rects_bitwise_equal(lhs.vertical_track, rhs.vertical_track)
        && optional_rects_bitwise_equal(lhs.vertical_thumb, rhs.vertical_thumb)
        && optional_rects_bitwise_equal(lhs.horizontal_track, rhs.horizontal_track)
        && optional_rects_bitwise_equal(lhs.horizontal_thumb, rhs.horizontal_thumb)
        && lhs.interaction == rhs.interaction
        && lhs.paint_state == rhs.paint_state
        && lhs.sampled_alpha.to_bits() == rhs.sampled_alpha.to_bits()
        && lhs.shadow_blur_radius.to_bits() == rhs.shadow_blur_radius.to_bits()
}

fn scroll_geometry_snapshots_bitwise_equal(
    lhs: ScrollGeometrySnapshot,
    rhs: ScrollGeometrySnapshot,
) -> bool {
    lhs.configured_axis == rhs.configured_axis
        && lhs.offset.map(f32::to_bits) == rhs.offset.map(f32::to_bits)
        && rects_bitwise_equal(lhs.scrollport_rect, rhs.scrollport_rect)
        && lhs.content_size.map(f32::to_bits) == rhs.content_size.map(f32::to_bits)
        && rects_bitwise_equal(
            lhs.layout_content_bounds_at_zero,
            rhs.layout_content_bounds_at_zero,
        )
        && lhs.contents_clip == rhs.contents_clip
        && scrollbar_overlays_bitwise_equal(lhs.scrollbar_overlay, rhs.scrollbar_overlay)
}

fn scroll_content_bounds_match(element: &dyn ElementTrait, scroll: ScrollGeometrySnapshot) -> bool {
    let bounds = element.box_model_snapshot();
    (bounds.x + scroll.offset[0]).to_bits() == scroll.layout_content_bounds_at_zero.x.to_bits()
        && (bounds.y + scroll.offset[1]).to_bits()
            == scroll.layout_content_bounds_at_zero.y.to_bits()
        && bounds.width.to_bits() == scroll.content_size[0].to_bits()
        && bounds.height.to_bits() == scroll.content_size[1].to_bits()
}

/// Closed production corpus for the exact nested-scroll receiver leaf.
///
/// `Element` keeps its private style/layout oracle. Image, SVG and standalone
/// Text are admitted only as structurally neutral leaves whose component-owned
/// recorder reports one exact payload. Inline-IFC-owned Text is rejected by
/// the Text-owned oracle before the generic capability check. The later strict
/// recorder/compiler pass remains final payload authority.
fn is_exact_retained_nested_scroll_content_leaf(
    element: &dyn ElementTrait,
    arena: &NodeArena,
) -> bool {
    if let Some(element) = element.as_any().downcast_ref::<Element>() {
        return element.is_exact_retained_scroll_content_leaf();
    }
    let is_exact_component_leaf = element.as_any().is::<Image>()
        || element.as_any().is::<Svg>()
        || element
            .as_any()
            .downcast_ref::<Text>()
            .is_some_and(Text::is_exact_standalone_retained_text_leaf);
    if !is_exact_component_leaf {
        return false;
    }

    let bounds = element.box_model_snapshot();
    let promotion = element.promotion_node_info();
    element.children().is_empty()
        && bounds.should_render
        && [bounds.x, bounds.y, bounds.width, bounds.height]
            .into_iter()
            .all(f32::is_finite)
        && bounds.width > 0.0
        && bounds.height > 0.0
        && promotion.opacity.to_bits() == 1.0_f32.to_bits()
        && !promotion.has_rounded_clip
        && !promotion.has_box_shadow
        && !promotion.is_scroll_container
        && !element.has_active_animator()
        && !element.is_deferred_to_root_viewport_render()
        && !element
            .placement_eligibility_metadata()
            .contains_runtime_layout_state
        && matches!(
            element.shadow_paint_recording_capability(
                arena,
                false,
                crate::view::paint::PaintRecordingContext::default(),
            ),
            ShadowPaintRecordingCapability::Recordable
        )
}

/// Typed result of observing a declared scroll host.
#[doc(hidden)]
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub enum ScrollGeometryObservation {
    /// The declaration is valid, but legacy paint currently has no contents
    /// clip (for example, there is no overflow). No scroll authority is made.
    Inactive,
    /// One complete exact rectangular legacy scroll observation.
    Exact(ScrollGeometrySnapshot),
    /// The host is active but outside the narrow M10E0 contract.
    Unsupported,
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

/// Canonical logical scissor conversion shared by the legacy renderer and
/// observational property validation. The returned value remains the
/// authority; callers must not rebuild and substitute their own scissor.
pub(crate) fn exact_logical_scissor_for_rect(rect: Rect) -> Option<[u32; 4]> {
    if !rect.x.is_finite()
        || !rect.y.is_finite()
        || !rect.width.is_finite()
        || !rect.height.is_finite()
        || rect.width < 0.0
        || rect.height < 0.0
        || !(rect.x + rect.width).is_finite()
        || !(rect.y + rect.height).is_finite()
    {
        return None;
    }
    let left = rect.x.floor().max(0.0) as i64;
    let top = rect.y.floor().max(0.0) as i64;
    let right = (rect.x + rect.width).ceil().max(0.0) as i64;
    let bottom = (rect.y + rect.height).ceil().max(0.0) as i64;
    (right > left && bottom > top).then_some([
        left as u32,
        top as u32,
        (right - left) as u32,
        (bottom - top) as u32,
    ])
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

pub(crate) fn promoted_layer_stable_key(node_id: u64) -> PersistentTextureKey {
    PersistentTextureKey::retained(RetainedTextureRole::PromotedBaseColor, node_id)
}

pub(crate) fn root_effect_stable_key(root: NodeKey) -> PersistentTextureKey {
    PersistentTextureKey::retained(RetainedTextureRole::RootEffectColor, root.data().as_ffi())
}

pub(crate) fn promoted_clip_mask_stable_key(node_id: u64) -> PersistentTextureKey {
    PersistentTextureKey::retained(RetainedTextureRole::PromotedClipMaskColor, node_id)
}

pub(crate) fn promoted_final_layer_stable_key(node_id: u64) -> PersistentTextureKey {
    PersistentTextureKey::retained(RetainedTextureRole::PromotedFinalColor, node_id)
}

pub(crate) fn transformed_layer_stable_key(node_id: u64) -> PersistentTextureKey {
    PersistentTextureKey::retained(RetainedTextureRole::TransformedColor, node_id)
}

pub(crate) fn isolation_layer_stable_key(node_id: u64) -> PersistentTextureKey {
    PersistentTextureKey::retained(RetainedTextureRole::IsolationColor, node_id)
}

pub(crate) fn scroll_host_layer_stable_key(node_id: u64) -> PersistentTextureKey {
    PersistentTextureKey::retained(RetainedTextureRole::ScrollHostColor, node_id)
}

pub(crate) fn scroll_content_layer_stable_key(node_id: u64) -> PersistentTextureKey {
    PersistentTextureKey::retained(RetainedTextureRole::ScrollContentColor, node_id)
}

pub(crate) fn scroll_content_tile_layer_stable_key(
    node_id: u64,
    column: u32,
    row: u32,
) -> Option<PersistentTextureKey> {
    PersistentTextureKey::retained_scroll_content_tile(
        RetainedTextureRole::ScrollContentColor,
        node_id,
        column,
        row,
    )
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
    frame_build_token: u64,
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

#[derive(Default)]
pub(crate) struct FramePreparation {
    deferred_nodes: Vec<DeferredRenderNode>,
    deferred_seen: FxHashSet<crate::view::node_arena::NodeKey>,
    deferred_cursor: usize,
}

impl FramePreparation {
    fn register_deferred(&mut self, node: DeferredRenderNode) {
        if self.deferred_seen.insert(node.key) {
            self.deferred_nodes.push(node);
        }
    }

    fn next_deferred(&mut self) -> Option<DeferredRenderNode> {
        let node = self.deferred_nodes.get(self.deferred_cursor).copied()?;
        self.deferred_cursor = self.deferred_cursor.saturating_add(1);
        Some(node)
    }
}

#[derive(Clone)]
pub struct BuildState {
    frame_build_token: u64,
    target: Option<RenderTargetOut>,
    depth_stencil_target: Option<AttachmentTarget>,
    target_pairs: FxHashMap<u32, AttachmentTarget>,
    scissor_rect: Option<[u32; 4]>,
    clip_id_stack: Vec<u8>,
    dfs_opaque_rect_order: u32,
    frame_preparation: Arc<Mutex<FramePreparation>>,
}

impl BuildState {
    pub fn current_target(&self) -> Option<RenderTargetOut> {
        self.target
    }

    #[cfg(test)]
    pub(crate) fn opaque_rect_order_for_test(&self) -> u32 {
        self.dfs_opaque_rect_order
    }

    #[cfg(test)]
    pub(crate) fn target_pair_count_for_test(&self) -> usize {
        self.target_pairs.len()
    }

    pub(crate) fn opaque_rect_order(&self) -> u32 {
        self.dfs_opaque_rect_order
    }

    pub(crate) fn replay_opaque_rect_order_exact(&mut self, expected_start: u32, terminal: u32) {
        assert_eq!(
            self.dfs_opaque_rect_order, expected_start,
            "opaque replay must start at the prepared local cursor"
        );
        assert!(
            terminal >= expected_start,
            "opaque replay terminal cannot move the cursor backwards"
        );
        self.dfs_opaque_rect_order = terminal;
    }

    fn rebind_frame_preparation_if_needed(&mut self, frame_build_token: u64) {
        if self.frame_build_token == frame_build_token {
            return;
        }
        self.frame_build_token = frame_build_token;
        self.frame_preparation = Arc::new(Mutex::new(FramePreparation::default()));
    }

    fn for_layer_subtree_with_frame_preparation(
        ancestor_clip: AncestorClipContext,
        frame_build_token: u64,
        frame_preparation: Arc<Mutex<FramePreparation>>,
    ) -> Self {
        Self {
            frame_build_token,
            target: None,
            depth_stencil_target: None,
            target_pairs: FxHashMap::default(),
            scissor_rect: ancestor_clip.scissor_rect,
            clip_id_stack: Vec::new(),
            dfs_opaque_rect_order: 0,
            frame_preparation,
        }
    }

    pub(crate) fn merge_child_render_state(&mut self, child: &BuildState) {
        self.dfs_opaque_rect_order = self.dfs_opaque_rect_order.max(child.dfs_opaque_rect_order);
        for (&color_handle, &depth_target) in &child.target_pairs {
            self.target_pairs.insert(color_handle, depth_target);
        }
    }

    pub(crate) fn merge_child_render_state_exact(
        &mut self,
        child: &BuildState,
        expected_parent_before: u32,
        expected_child_terminal: u32,
        expected_parent_after: u32,
    ) {
        assert_eq!(
            self.dfs_opaque_rect_order, expected_parent_before,
            "child merge must start at the prepared parent cursor"
        );
        assert_eq!(
            child.dfs_opaque_rect_order, expected_child_terminal,
            "child merge must consume the prepared child terminal"
        );
        assert_eq!(
            expected_parent_after,
            expected_parent_before.max(expected_child_terminal),
            "prepared parent terminal must be the max of parent and child cursors"
        );
        self.merge_child_render_state(child);
        assert_eq!(
            self.dfs_opaque_rect_order, expected_parent_after,
            "exact child merge must reach the prepared parent cursor"
        );
    }

    pub(crate) fn merge_child_target_pairs(&mut self, child: &BuildState) {
        for (&color_handle, &depth_target) in &child.target_pairs {
            self.target_pairs.insert(color_handle, depth_target);
        }
    }
}

fn next_frame_build_token() -> u64 {
    static NEXT_FRAME_BUILD_TOKEN: AtomicU64 = AtomicU64::new(1);
    NEXT_FRAME_BUILD_TOKEN.fetch_add(1, Ordering::Relaxed)
}

pub struct UiBuildContext {
    viewport: ViewportContext,
    state: BuildState,
}

pub(crate) fn texture_desc_for_logical_bounds(
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

pub(crate) fn label_for_persistent_target(stable_key: PersistentTextureKey) -> String {
    match stable_key {
        PersistentTextureKey::Retained {
            role: RetainedTextureRole::RootEffectColor,
            stable_id,
        } => format!("Root Effect [{stable_id}]"),
        PersistentTextureKey::Retained {
            role: RetainedTextureRole::PromotedClipMaskColor,
            stable_id,
        } => promoted_clip_mask_label(stable_id),
        PersistentTextureKey::Retained {
            role: RetainedTextureRole::PromotedFinalColor,
            stable_id,
        } => promoted_final_layer_label(stable_id),
        PersistentTextureKey::Retained {
            role: RetainedTextureRole::PromotedBaseColor,
            stable_id,
        } => promoted_layer_label(stable_id),
        _ => format!("Persistent Render Target [{stable_key:?}]"),
    }
}

pub(crate) fn persistent_depth_stencil_stable_key(
    stable_key: PersistentTextureKey,
) -> Option<PersistentTextureKey> {
    stable_key.depth_stencil()
}

pub(crate) fn persistent_target_texture_descriptors(
    mut color: TextureDesc,
    stable_key: PersistentTextureKey,
) -> (TextureDesc, TextureDesc) {
    if color.label().is_none() {
        color = color.with_label(label_for_persistent_target(stable_key));
    }
    let depth_label = color
        .label()
        .map(|label| format!("{label} / Depth-Stencil"))
        .unwrap_or_else(|| "Persistent Depth-Stencil".to_string());
    let depth = TextureDesc::new(
        color.width(),
        color.height(),
        wgpu::TextureFormat::Depth24PlusStencil8,
        wgpu::TextureDimension::D2,
    )
    .with_usage(wgpu::TextureUsages::RENDER_ATTACHMENT)
    .with_sample_count(color.sample_count())
    .with_label(depth_label);
    (color, depth)
}

/// Canonical descriptor pair for one frame-local offscreen target.  Keeping
/// this derivation shared lets graph-inert preparation budget the exact pair
/// that `UiBuildContext` will eventually allocate without minting a stable
/// key or touching the persistent target pool.
pub(crate) fn transient_target_texture_descriptors(
    color: TextureDesc,
) -> (TextureDesc, TextureDesc) {
    let depth_label = color
        .label()
        .map(|label| format!("{label} / Depth-Stencil"))
        .unwrap_or_else(|| "Offscreen Depth-Stencil".to_string());
    let depth = TextureDesc::new(
        color.width(),
        color.height(),
        wgpu::TextureFormat::Depth24PlusStencil8,
        wgpu::TextureDimension::D2,
    )
    .with_usage(wgpu::TextureUsages::RENDER_ATTACHMENT)
    .with_sample_count(color.sample_count())
    .with_label(depth_label);
    (color, depth)
}

impl UiBuildContext {
    pub fn new(
        viewport_width: u32,
        viewport_height: u32,
        viewport_format: wgpu::TextureFormat,
        scale_factor: f32,
    ) -> Self {
        let frame_build_token = next_frame_build_token();
        Self {
            viewport: ViewportContext {
                frame_build_token,
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
                frame_build_token,
                target: None,
                depth_stencil_target: Some(AttachmentTarget::Surface),
                target_pairs: FxHashMap::default(),
                scissor_rect: None,
                clip_id_stack: Vec::new(),
                dfs_opaque_rect_order: 0,
                frame_preparation: Arc::new(Mutex::new(FramePreparation::default())),
            },
        }
    }

    pub fn from_parts(viewport: ViewportContext, mut state: BuildState) -> Self {
        state.rebind_frame_preparation_if_needed(viewport.frame_build_token);
        Self { viewport, state }
    }

    pub fn viewport(&self) -> ViewportContext {
        self.viewport.clone()
    }

    pub fn set_state(&mut self, mut state: BuildState) {
        state.rebind_frame_preparation_if_needed(self.viewport.frame_build_token);
        self.state = state;
    }

    pub fn state_clone(&self) -> BuildState {
        self.state.clone()
    }

    pub fn into_state(self) -> BuildState {
        self.state
    }

    pub(crate) fn layer_subtree_state_with_ancestor_clip(
        &self,
        ancestor_clip: AncestorClipContext,
    ) -> BuildState {
        BuildState::for_layer_subtree_with_frame_preparation(
            ancestor_clip,
            self.state.frame_build_token,
            self.state.frame_preparation.clone(),
        )
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
        let desc = texture_desc_for_logical_bounds(
            bounds,
            self.viewport.scale_factor,
            self.viewport.render_transform,
            self.viewport.target_format,
        )
        .with_label(promoted_layer_label(node_id));
        self.next_persistent_target_with_desc(graph, desc, promoted_layer_stable_key(node_id))
    }

    pub(crate) fn allocate_persistent_target_with_key(
        &mut self,
        graph: &mut FrameGraph,
        stable_key: PersistentTextureKey,
        bounds: PromotionCompositeBounds,
    ) -> RenderTargetOut {
        let desc = texture_desc_for_logical_bounds(
            bounds,
            self.viewport.scale_factor,
            self.viewport.render_transform,
            self.viewport.target_format,
        );
        self.next_persistent_target_with_desc(graph, desc, stable_key)
    }

    pub(crate) fn allocate_persistent_target_with_desc(
        &mut self,
        graph: &mut FrameGraph,
        desc: TextureDesc,
        stable_key: PersistentTextureKey,
    ) -> RenderTargetOut {
        self.next_persistent_target_with_desc(graph, desc, stable_key)
    }

    /// Declares one exact frame-local color/depth target pair without
    /// creating a stable key. The shared descriptor helper remains the sole
    /// authority for deriving the depth attachment.
    pub(crate) fn allocate_transient_target_with_desc(
        &mut self,
        graph: &mut FrameGraph,
        color_desc: TextureDesc,
    ) -> RenderTargetOut {
        self.next_target_with_desc(graph, color_desc)
    }

    pub(crate) fn allocate_persistent_full_viewport_target(
        &mut self,
        graph: &mut FrameGraph,
        stable_key: PersistentTextureKey,
    ) -> RenderTargetOut {
        let desc = self.persistent_full_viewport_target_desc(stable_key);
        self.next_persistent_target_with_desc(graph, desc, stable_key)
    }

    pub(crate) fn persistent_full_viewport_target_desc(
        &self,
        stable_key: PersistentTextureKey,
    ) -> TextureDesc {
        let desc = TextureDesc::new(
            self.viewport.target_width,
            self.viewport.target_height,
            self.viewport.target_format,
            wgpu::TextureDimension::D2,
        );
        persistent_target_texture_descriptors(desc, stable_key).0
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

    pub(crate) fn emit_draw_rect_pass(&mut self, graph: &mut FrameGraph, pass: DrawRectPass) {
        if pass.is_opaque_candidate() {
            let mut opaque: OpaqueRectPass = pass.into_opaque();
            opaque.set_depth_order(self.next_opaque_rect_order());
            emit_draw_rect_io_pass(graph, self, opaque);
        } else {
            emit_draw_rect_io_pass(graph, self, pass);
        }
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
        let (color_desc, depth_desc) = transient_target_texture_descriptors(desc);
        let color = graph.declare_texture::<RenderTargetTag>(color_desc);
        let depth_stencil = graph.declare_texture::<RenderTargetTag>(depth_desc);
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
        desc: TextureDesc,
        stable_key: PersistentTextureKey,
    ) -> RenderTargetOut {
        let (desc, depth_desc) = persistent_target_texture_descriptors(desc, stable_key);
        let color =
            graph.declare_persistent_texture_internal::<RenderTargetTag>(desc.clone(), stable_key);
        let depth_stencil = graph.declare_persistent_texture_internal::<RenderTargetTag>(
            depth_desc,
            persistent_depth_stencil_stable_key(stable_key)
                .expect("persistent color stable key must have a depth role"),
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

    pub(crate) fn register_deferred(
        &mut self,
        key: crate::view::node_arena::NodeKey,
        stable_id: u64,
    ) {
        self.state
            .frame_preparation
            .lock()
            .expect("frame preparation lock poisoned")
            .register_deferred(DeferredRenderNode { key, stable_id });
    }

    pub(crate) fn next_deferred(&mut self) -> Option<DeferredRenderNode> {
        self.state
            .frame_preparation
            .lock()
            .expect("frame preparation lock poisoned")
            .next_deferred()
    }

    pub(crate) fn next_opaque_rect_order(&mut self) -> u32 {
        let order = self.state.dfs_opaque_rect_order;
        self.state.dfs_opaque_rect_order = self.state.dfs_opaque_rect_order.saturating_add(1);
        order
    }

    pub(crate) fn opaque_rect_order(&self) -> u32 {
        self.state.opaque_rect_order()
    }

    pub(crate) fn replay_opaque_rect_order_exact(&mut self, expected_start: u32, terminal: u32) {
        self.state
            .replay_opaque_rect_order_exact(expected_start, terminal);
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

    pub(crate) fn merge_child_render_state(&mut self, child: &BuildState) {
        self.state.merge_child_render_state(child);
    }

    pub(crate) fn merge_child_render_state_exact(
        &mut self,
        child: &BuildState,
        expected_parent_before: u32,
        expected_child_terminal: u32,
        expected_parent_after: u32,
    ) {
        self.state.merge_child_render_state_exact(
            child,
            expected_parent_before,
            expected_child_terminal,
            expected_parent_after,
        );
    }

    pub(crate) fn merge_child_target_pairs(&mut self, child: &BuildState) {
        self.state.merge_child_target_pairs(child);
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
    pub const COMPOSITE: Self = Self(1 << 5);
    pub const RUNTIME: Self = Self(
        Self::PLACE.0 | Self::BOX_MODEL.0 | Self::HIT_TEST.0 | Self::PAINT.0 | Self::COMPOSITE.0,
    );
    pub const ALL: Self = Self(Self::LAYOUT.0 | Self::RUNTIME.0);

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
    /// Compositor-property update dependency. No retained pass consumes this
    /// shadow classification yet.
    pub const COMPOSITE: DirtyFlags = DirtyFlags::COMPOSITE;
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
        assert!(!DirtyPassMask::PLACEMENT.intersects(DirtyFlags::COMPOSITE));

        assert_eq!(DirtyPassMask::BOX_MODEL, DirtyFlags::BOX_MODEL);
        assert_eq!(DirtyPassMask::HIT_TEST, DirtyFlags::HIT_TEST);
        assert_eq!(DirtyPassMask::PAINT, DirtyFlags::PAINT);
        assert_eq!(DirtyPassMask::COMPOSITE, DirtyFlags::COMPOSITE);
        assert!(!DirtyPassMask::PAINT.intersects(DirtyPassMask::PLACEMENT));
        assert!(!DirtyPassMask::PAINT.intersects(DirtyFlags::COMPOSITE));

        assert_eq!(
            DirtyPassMask::RUNTIME,
            DirtyPassMask::PLACEMENT
                .union(DirtyPassMask::PAINT)
                .union(DirtyPassMask::COMPOSITE)
        );
        assert_eq!(
            DirtyPassMask::RUNTIME,
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST)
                .union(DirtyFlags::PAINT)
                .union(DirtyFlags::COMPOSITE)
        );
        assert!(!DirtyPassMask::RUNTIME.intersects(DirtyFlags::LAYOUT));
        assert!(DirtyPassMask::RUNTIME.contains(DirtyFlags::COMPOSITE));
        assert!(DirtyFlags::ALL.contains(DirtyFlags::COMPOSITE));
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

#[derive(Clone, Copy, Debug)]
pub struct PaintResourcePreparationContext {
    pub frame_number: u64,
    pub device_scale: f32,
    pub now: crate::time::Instant,
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
    /// Freeze paint-only resources after the frame's final layout pass.
    ///
    /// This hook intentionally has no arena access: it may request/snapshot
    /// resource data, but must not mutate child topology or invalidate the
    /// layout that selected the request. A completion that changes a host's
    /// loading/error slot is committed by the next frame's pre-layout
    /// `sync_arena` instead.
    fn prepare_paint_resources(&mut self, _context: PaintResourcePreparationContext) {}
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

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowPaintBlocker {
    Transform,
    BoxShadow,
    InlineIfc,
    ScrollContainer,
    SelfClip,
    ChildClip,
    Deferred,
    LayoutTransition,
    StatefulPaint,
    MissingPreparedInlineDecoration,
    MissingPreparedInlineRoot,
    MissingPreparedText,
    MissingPreparedImage,
    MissingPreparedSvg,
    TextAreaSelection,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowPaintRecordingCapability {
    Unsupported,
    /// This owner and its complete subtree are known to produce no paint for
    /// the current frame. Unlike `Transparent`, coverage must not recurse
    /// into children.
    CulledSubtree,
    Transparent,
    Recordable,
    Legacy(ShadowPaintBlocker),
}

/// Engine-owned paint context exposed to custom leaf recorders.
///
/// The bounds are already resolved into the same paint space used by the
/// retained artifact recorder. Custom hosts must record that exact rectangle;
/// transforms, clips, scrolling, effects, and child traversal remain engine
/// responsibilities and are deliberately absent from this API.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct CustomLeafPaintContext {
    bounds: Rect,
}

impl CustomLeafPaintContext {
    /// Returns the only rectangle this custom leaf may paint.
    pub fn bounds(self) -> Rect {
        self.bounds
    }
}

/// Typed, backend-independent commands accepted from a custom leaf.
///
/// M9F1 intentionally supports only one fill rectangle. The recorder rejects
/// missing, repeated, reordered, or non-canonical commands before they can
/// enter the internal paint artifact.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub enum CustomLeafPaintCommand {
    FillRect {
        rect: Rect,
        rgba: [f32; 4],
        opacity: f32,
    },
}

/// Narrow recorder passed to [`ElementTrait::record_custom_leaf_paint`].
///
/// Commands remain private to the engine. Invalid numeric values are retained
/// long enough for fail-closed validation; they are never clamped or emitted.
#[non_exhaustive]
#[derive(Debug, Default)]
pub struct CustomLeafPaintRecorder {
    commands: Vec<CustomLeafPaintCommand>,
}

impl CustomLeafPaintRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one linear-RGBA fill for the exact engine-provided bounds.
    pub fn fill_rect(&mut self, rect: Rect, rgba: [f32; 4], opacity: f32) {
        self.commands.push(CustomLeafPaintCommand::FillRect {
            rect,
            rgba,
            opacity,
        });
    }
}

/// Engine-owned paint context exposed to property-neutral custom wrappers.
///
/// The wrapper may paint the exact engine-provided bounds before and/or after
/// its canonical arena children. Child traversal and paint identity remain
/// engine-owned and are deliberately absent from this API.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct CustomWrapperPaintContext {
    bounds: Rect,
}

impl CustomWrapperPaintContext {
    /// Returns the only rectangle this custom wrapper may paint.
    pub fn bounds(self) -> Rect {
        self.bounds
    }
}

#[derive(Clone, Copy, Debug)]
struct CustomWrapperFillRect {
    rect: Rect,
    rgba: [f32; 4],
    opacity: f32,
}

/// Narrow typed recorder passed to
/// [`ElementTrait::record_custom_wrapper_paint`].
///
/// Commands are retained in separate engine-owned phase buckets. Their append
/// order becomes the canonical slot order within that phase; callers cannot
/// provide internal phases, slots, scopes, or roles.
#[non_exhaustive]
#[derive(Debug, Default)]
pub struct CustomWrapperPaintRecorder {
    before_children: Vec<CustomWrapperFillRect>,
    after_children: Vec<CustomWrapperFillRect>,
}

impl CustomWrapperPaintRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one linear-RGBA fill before canonical child traversal.
    pub fn fill_rect_before_children(&mut self, rect: Rect, rgba: [f32; 4], opacity: f32) {
        self.before_children.push(CustomWrapperFillRect {
            rect,
            rgba,
            opacity,
        });
    }

    /// Records one linear-RGBA fill after canonical child traversal.
    pub fn fill_rect_after_children(&mut self, rect: Rect, rgba: [f32; 4], opacity: f32) {
        self.after_children.push(CustomWrapperFillRect {
            rect,
            rgba,
            opacity,
        });
    }
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

    /// Advance retained visual-only animation state for one viewport frame.
    ///
    /// Implementations must not read the wall clock themselves. The viewport
    /// supplies one engine-time sample to the whole tree so metadata, full
    /// paint recording, and promotion observation remain pure reads of the
    /// retained result.
    fn tick_animation_frame(&mut self, _now: crate::time::Instant) -> DirtyFlags {
        DirtyFlags::NONE
    }

    /// Resolve visual-only state that depends on this frame's final layout.
    ///
    /// The viewport invokes this after any relayout and before property-tree
    /// synchronization. Implementations receive the same engine-time sample
    /// as the pre-layout animation tick and must not read a clock themselves.
    fn tick_post_layout_animation_frame(&mut self, _now: crate::time::Instant) -> DirtyFlags {
        DirtyFlags::NONE
    }

    /// Records complete paint for a property-neutral custom leaf.
    ///
    /// The default records nothing and therefore preserves the legacy
    /// `UnknownHost` fallback. Implementations must be pure reads: metadata
    /// preflight and full artifact recording call this hook independently and
    /// compare their canonical identities before the artifact may compile.
    fn record_custom_leaf_paint(
        &self,
        _context: CustomLeafPaintContext,
        _recorder: &mut CustomLeafPaintRecorder,
    ) {
    }

    /// Records complete self-paint around canonical children for a
    /// property-neutral custom wrapper.
    ///
    /// This hook is considered only when `children()` is non-empty. It must be
    /// a pure read because capability, metadata preflight, and full artifact
    /// recording invoke it independently and compare canonical identities.
    fn record_custom_wrapper_paint(
        &self,
        _context: CustomWrapperPaintContext,
        _recorder: &mut CustomWrapperPaintRecorder,
    ) {
    }

    #[doc(hidden)]
    #[allow(private_interfaces)]
    fn shadow_paint_recording_capability(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        deferred_phase_root: bool,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> ShadowPaintRecordingCapability {
        let prepared = if self.children().is_empty() {
            prepare_custom_leaf_paint(self, deferred_phase_root, recording_context).is_some()
        } else {
            prepare_custom_wrapper_paint(self, None, arena, deferred_phase_root, recording_context)
                .is_some()
        };
        if prepared {
            ShadowPaintRecordingCapability::Recordable
        } else {
            ShadowPaintRecordingCapability::Unsupported
        }
    }

    #[allow(private_interfaces)]
    #[doc(hidden)]
    fn record_shadow_paint_metadata(
        &self,
        owner: crate::view::node_arena::NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        arena: &crate::view::node_arena::NodeArena,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<crate::view::paint::PaintChunkMetadata> {
        let prepared =
            prepare_owned_custom_leaf_paint(self, owner, properties, arena, recording_context)?;
        Some(prepared.metadata(owner, properties, content_revision))
    }

    #[allow(private_interfaces)]
    #[doc(hidden)]
    fn record_shadow_paint_artifact(
        &self,
        owner: crate::view::node_arena::NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        arena: &crate::view::node_arena::NodeArena,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<crate::view::paint::PaintArtifact> {
        let prepared =
            prepare_owned_custom_leaf_paint(self, owner, properties, arena, recording_context)?;
        #[cfg(test)]
        crate::view::paint::note_full_artifact_record();
        Some(prepared.artifact(owner, properties, content_revision))
    }

    #[allow(private_interfaces)]
    #[doc(hidden)]
    fn record_shadow_paint_metadata_plan(
        &self,
        owner: crate::view::node_arena::NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        contents_properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        arena: &crate::view::node_arena::NodeArena,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<crate::view::paint::PaintNodePlan<crate::view::paint::PaintChunkMetadata>> {
        if let Some(metadata) = self.record_shadow_paint_metadata(
            owner,
            properties,
            content_revision,
            arena,
            recording_context,
        ) {
            let mut plan = crate::view::paint::PaintNodePlan::single_before(metadata);
            if let Some(scroll) =
                recording_context.baked_scroll_host_snapshot_for_root(self.stable_id())
            {
                let bounds = self.box_model_snapshot();
                let payload_identity = match scroll.scrollbar_overlay.paint_state {
                    ScrollbarPaintStateWitness::HiddenNow
                    | ScrollbarPaintStateWitness::NotPaintable => {
                        crate::view::paint::PaintPayloadIdentity::prepared_shadows(
                            std::iter::empty(),
                        )
                    }
                    ScrollbarPaintStateWitness::OpaqueNow
                    | ScrollbarPaintStateWitness::TranslucentNow => {
                        let overlay =
                            crate::view::paint::PreparedScrollbarOverlayOp::from_vertical_witness(
                                scroll.scrollbar_overlay,
                            )?;
                        crate::view::paint::PaintPayloadIdentity::prepared_scrollbar_overlay(
                            &overlay,
                        )
                    }
                };
                plan.after_children
                    .push(crate::view::paint::PaintChunkMetadata {
                        id: crate::view::paint::PaintChunkId {
                            owner,
                            scope: crate::view::paint::PaintPropertyScope::SelfPaint,
                            phase: crate::view::paint::PaintNodePhase::AfterChildren,
                            slot: 0,
                            role: crate::view::paint::PaintChunkRole::ScrollbarOverlay,
                        },
                        owner,
                        bounds: Rect {
                            x: bounds.x,
                            y: bounds.y,
                            width: bounds.width,
                            height: bounds.height,
                        },
                        properties,
                        content_revision,
                        payload_identity,
                    });
            }
            return Some(plan);
        }
        if !self.children().is_empty()
            && let Some(prepared) = prepare_owned_custom_wrapper_paint(
                self,
                owner,
                properties,
                contents_properties,
                arena,
                recording_context,
            )
        {
            return Some(prepared.metadata_plan(owner, properties, content_revision));
        }
        None
    }

    #[allow(private_interfaces)]
    #[doc(hidden)]
    fn record_shadow_paint_artifact_plan(
        &self,
        owner: crate::view::node_arena::NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        contents_properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        arena: &crate::view::node_arena::NodeArena,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<crate::view::paint::PaintNodePlan<crate::view::paint::PaintArtifact>> {
        if let Some(artifact) = self.record_shadow_paint_artifact(
            owner,
            properties,
            content_revision,
            arena,
            recording_context,
        ) {
            let mut plan = crate::view::paint::PaintNodePlan::single_before(artifact);
            if let Some(scroll) =
                recording_context.baked_scroll_host_snapshot_for_root(self.stable_id())
            {
                let bounds = self.box_model_snapshot();
                let (ops, payload_identity) = match scroll.scrollbar_overlay.paint_state {
                    ScrollbarPaintStateWitness::HiddenNow
                    | ScrollbarPaintStateWitness::NotPaintable => (
                        Vec::new(),
                        crate::view::paint::PaintPayloadIdentity::prepared_shadows(
                            std::iter::empty(),
                        ),
                    ),
                    ScrollbarPaintStateWitness::OpaqueNow
                    | ScrollbarPaintStateWitness::TranslucentNow => {
                        let overlay =
                            crate::view::paint::PreparedScrollbarOverlayOp::from_vertical_witness(
                                scroll.scrollbar_overlay,
                            )?;
                        let identity =
                            crate::view::paint::PaintPayloadIdentity::prepared_scrollbar_overlay(
                                &overlay,
                            );
                        (
                            vec![crate::view::paint::PaintOp::PreparedScrollbarOverlay(
                                overlay,
                            )],
                            identity,
                        )
                    }
                };
                let op_count = ops.len();
                plan.after_children.push(crate::view::paint::PaintArtifact {
                    target: Default::default(),
                    chunks: vec![crate::view::paint::PaintChunk {
                        id: crate::view::paint::PaintChunkId {
                            owner,
                            scope: crate::view::paint::PaintPropertyScope::SelfPaint,
                            phase: crate::view::paint::PaintNodePhase::AfterChildren,
                            slot: 0,
                            role: crate::view::paint::PaintChunkRole::ScrollbarOverlay,
                        },
                        owner,
                        op_range: 0..op_count,
                        bounds: Rect {
                            x: bounds.x,
                            y: bounds.y,
                            width: bounds.width,
                            height: bounds.height,
                        },
                        properties,
                        content_revision,
                        payload_identity,
                    }],
                    ops,
                    clip_nodes: Vec::new(),
                    effect_nodes: Vec::new(),
                    owner_nodes: vec![crate::view::paint::PaintOwnerSnapshot {
                        owner,
                        parent: None,
                    }],
                });
            }
            return Some(plan);
        }
        if !self.children().is_empty()
            && let Some(prepared) = prepare_owned_custom_wrapper_paint(
                self,
                owner,
                properties,
                contents_properties,
                arena,
                recording_context,
            )
        {
            #[cfg(test)]
            crate::view::paint::note_full_artifact_record();
            return Some(prepared.artifact_plan(owner, properties, content_revision));
        }
        None
    }

    /// Logical viewport-space scissor applied to this host's contents and
    /// arena children, but not to its own self-paint. `None` means the host
    /// has no contents clip; an explicit empty clip must be returned as
    /// `Some([x, y, 0, 0])`.
    #[doc(hidden)]
    fn contents_logical_scissor(&self) -> Option<[u32; 4]> {
        None
    }

    /// Classify one declared scroll host as inactive, exactly observed, or
    /// unsupported by the narrow M10E0 contract. Property sync stays
    /// fail-closed and never reconstructs fields from raw box geometry.
    #[doc(hidden)]
    fn scroll_geometry_observation(
        &self,
        _owner: crate::view::node_arena::NodeKey,
        _arena: &crate::view::node_arena::NodeArena,
    ) -> ScrollGeometryObservation {
        ScrollGeometryObservation::Unsupported
    }

    #[allow(private_interfaces)]
    #[doc(hidden)]
    fn shadow_paint_recording_context(
        &self,
        parent: crate::view::paint::PaintRecordingContext,
    ) -> crate::view::paint::PaintRecordingContext {
        parent
    }

    /// Derive path-specific recording authority for one direct child.
    ///
    /// The default deliberately drops TextArea child authority. Hosts that
    /// own the exact child relationship must opt in to forwarding or
    /// producing it, which keeps sibling traversal fail-closed.
    #[allow(private_interfaces)]
    #[doc(hidden)]
    fn shadow_paint_recording_context_for_child(
        &self,
        _child: crate::view::node_arena::NodeKey,
        _arena: &crate::view::node_arena::NodeArena,
        parent: crate::view::paint::PaintRecordingContext,
    ) -> crate::view::paint::PaintRecordingContext {
        parent.without_text_area_child_authority()
    }

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
        self.has_composited_descendants_matching(arena, &|stable_id| {
            ctx.is_node_promoted(stable_id)
        })
    }

    fn has_composited_descendants_matching(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        is_promoted: &dyn Fn(u64) -> bool,
    ) -> bool {
        for child_key in self.children() {
            let Some(node) = arena.get(*child_key) else {
                continue;
            };
            let child = node.element.as_ref();
            if child.is_deferred_to_root_viewport_render() {
                continue;
            }
            if is_promoted(child.stable_id()) {
                return true;
            }
            if child.has_composited_descendants_matching(arena, is_promoted) {
                return true;
            }
        }
        false
    }

    fn is_deferred_to_root_viewport_render(&self) -> bool {
        false
    }

    fn has_active_animator(&self) -> bool {
        false
    }

    fn promotion_self_signature(&self) -> u64 {
        0
    }

    /// Whether this host supplies a complete promotion paint identity.
    /// Opting in requires [`Self::promotion_self_signature`] to identify every
    /// raster-affecting state owned by the host, including transforms, scroll
    /// state, pixel resources, and their generations. Group opacity is the
    /// only property currently split out through the compositor effect
    /// generation. Scroll remains conservatively base-affecting.
    ///
    /// Clip intersection may live in
    /// [`Self::promotion_clip_intersection_signature`]; a custom host that
    /// does not override that method must include its raster-affecting clip
    /// state in [`Self::promotion_self_signature`]. Transform and clip property
    /// trees are still observational/neutral and are not substitutes for this
    /// contract.
    ///
    /// The safe default is `false`: unknown hosts reraster while promoted,
    /// or force their flattened promoted ancestor to reraster. Custom hosts
    /// may opt in only when these requirements are satisfied.
    fn promotion_signature_is_complete(&self) -> bool {
        false
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

    fn retained_transform_surface_bounds(
        &self,
        _arena: &crate::view::node_arena::NodeArena,
        _paint_offset: [f32; 2],
    ) -> Option<PromotionCompositeBounds> {
        None
    }

    /// Exact viewport-space paint coverage contributed to a transformed
    /// ancestor surface. The default is deliberately unsupported: custom
    /// hosts must opt in instead of being silently treated as a raw box.
    fn retained_transform_output_bounds(
        &self,
        _arena: &crate::view::node_arena::NodeArena,
        _paint_offset: [f32; 2],
    ) -> Option<PromotionCompositeBounds> {
        None
    }

    /// Compatibility-only coverage for the legacy renderer. Unknown hosts
    /// retain the historical raw-box fallback so an existing transformed
    /// ancestor does not disappear; retained planning must use the exact API
    /// above and reject this fallback.
    fn legacy_transform_output_bounds(
        &self,
        _arena: &crate::view::node_arena::NodeArena,
        _paint_offset: [f32; 2],
    ) -> Option<PromotionCompositeBounds> {
        Some(self.promotion_composite_bounds())
    }

    /// Seed used by the prospective planner's postorder transform-bounds DP.
    /// Non-Element hosts are opaque leaves to Element's authoritative bounds
    /// walk and therefore return `None`.
    fn retained_transform_raster_seed_bounds(&self) -> Option<PromotionCompositeBounds> {
        None
    }

    fn has_retained_transform_surface(&self) -> bool {
        false
    }

    /// Final viewport-space transform observed by the compositor property tree.
    ///
    /// The matrix is already resolved around the host's absolute layout origin;
    /// property-tree consumers must retain parent identity but must not multiply
    /// this payload by the parent transform again. `None` is the neutral default
    /// for hosts that do not participate in the built-in transform contract.
    #[allow(private_interfaces)]
    #[doc(hidden)]
    fn compositor_viewport_transform_snapshot(&self) -> Option<ViewportTransformSnapshot> {
        None
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

    /// Complete, retained measurement input for an atomic inline child.
    /// Unknown hosts deliberately return `None`: an owning IFC root cannot
    /// prove placement freshness without the exact proposal that produced the
    /// measured size.
    #[allow(private_interfaces)]
    #[doc(hidden)]
    fn inline_atomic_measurement_snapshot(&self) -> Option<InlineIfcMeasuredAtomicBox> {
        None
    }

    /// Resolved vertical alignment used when positioning an atomic inline
    /// child. The safe default is unsupported rather than CSS baseline: a
    /// wrapper host must explicitly delegate its Element style authority.
    #[doc(hidden)]
    fn inline_atomic_vertical_align(&self) -> Option<VerticalAlign> {
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

struct PreparedCustomLeafPaint {
    bounds: Rect,
    op: crate::view::paint::DrawRectOp,
    payload_identity: crate::view::paint::PaintPayloadIdentity,
}

impl PreparedCustomLeafPaint {
    fn chunk_id(owner: NodeKey) -> crate::view::paint::PaintChunkId {
        crate::view::paint::PaintChunkId {
            owner,
            scope: crate::view::paint::PaintPropertyScope::SelfPaint,
            phase: crate::view::paint::PaintNodePhase::BeforeChildren,
            slot: 0,
            role: crate::view::paint::PaintChunkRole::SelfDecoration,
        }
    }

    fn metadata(
        &self,
        owner: NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
    ) -> crate::view::paint::PaintChunkMetadata {
        crate::view::paint::PaintChunkMetadata {
            id: Self::chunk_id(owner),
            owner,
            bounds: self.bounds,
            properties,
            content_revision,
            payload_identity: self.payload_identity.clone(),
        }
    }

    fn artifact(
        self,
        owner: NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
    ) -> crate::view::paint::PaintArtifact {
        crate::view::paint::PaintArtifact {
            target: Default::default(),
            chunks: vec![crate::view::paint::PaintChunk {
                id: Self::chunk_id(owner),
                owner,
                op_range: 0..1,
                bounds: self.bounds,
                properties,
                content_revision,
                payload_identity: self.payload_identity,
            }],
            ops: vec![crate::view::paint::PaintOp::DrawRect(self.op)],
            clip_nodes: Vec::new(),
            effect_nodes: Vec::new(),
            owner_nodes: vec![crate::view::paint::PaintOwnerSnapshot {
                owner,
                parent: None,
            }],
        }
    }
}

fn prepare_custom_leaf_paint<T: ElementTrait + ?Sized>(
    element: &T,
    deferred_phase_root: bool,
    recording_context: crate::view::paint::PaintRecordingContext,
) -> Option<PreparedCustomLeafPaint> {
    if deferred_phase_root
        || element.is_deferred_to_root_viewport_render()
        || element.has_active_animator()
        || element
            .placement_eligibility_metadata()
            .contains_runtime_layout_state
        || !element.children().is_empty()
        || !matches!(
            recording_context.opacity_authority,
            crate::view::paint::PaintOpacityAuthority::Baked
        )
        || recording_context.inside_text_area
        || recording_context.text_area_selection.is_some()
        || recording_context.text_area_preedit.is_some()
    {
        return None;
    }

    let snapshot = element.box_model_snapshot();
    let promotion = element.promotion_node_info();
    if !snapshot.should_render
        || snapshot.node_id != element.stable_id()
        || !snapshot.border_radius.is_finite()
        || snapshot.border_radius.to_bits() != 0.0_f32.to_bits()
        || !promotion.opacity.is_finite()
        || promotion.opacity.to_bits() != 1.0_f32.to_bits()
        || promotion.has_rounded_clip
        || promotion.has_box_shadow
        || promotion.has_border
        || promotion.is_scroll_container
    {
        return None;
    }

    let bounds = Rect {
        x: snapshot.x + recording_context.paint_offset[0],
        y: snapshot.y + recording_context.paint_offset[1],
        width: snapshot.width,
        height: snapshot.height,
    };
    if !has_canonical_custom_leaf_bounds(bounds) {
        return None;
    }

    let context = CustomLeafPaintContext { bounds };
    let mut recorder = CustomLeafPaintRecorder::new();
    element.record_custom_leaf_paint(context, &mut recorder);
    let [
        CustomLeafPaintCommand::FillRect {
            rect,
            rgba,
            opacity,
        },
    ] = recorder.commands.as_slice()
    else {
        return None;
    };
    if !same_custom_leaf_rect_bits(*rect, bounds)
        || rgba
            .iter()
            .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(channel))
        || !opacity.is_finite()
        || !(0.0..=1.0).contains(opacity)
    {
        return None;
    }

    let mut fill_color = *rgba;
    fill_color[3] *= *opacity;
    let op = crate::view::paint::DrawRectOp {
        params: RectPassParams {
            position: [rect.x, rect.y],
            size: [rect.width, rect.height],
            fill_color,
            // Custom content opacity is folded into the fill alpha so the
            // internal op remains property-neutral and satisfies the same
            // compiler contract as an ordinary neutral leaf.
            opacity: 1.0,
            ..Default::default()
        },
        mode: RectRenderMode::FillOnly,
    };
    let payload_identity =
        crate::view::paint::PaintPayloadIdentity::prepared_shadows_with_decoration(
            std::iter::empty(),
            std::iter::once(&op),
        )?;
    Some(PreparedCustomLeafPaint {
        bounds,
        op,
        payload_identity,
    })
}

fn prepare_owned_custom_leaf_paint<T: ElementTrait + ?Sized>(
    element: &T,
    owner: NodeKey,
    properties: crate::view::compositor::property_tree::PropertyTreeState,
    arena: &NodeArena,
    recording_context: crate::view::paint::PaintRecordingContext,
) -> Option<PreparedCustomLeafPaint> {
    if properties != Default::default() {
        return None;
    }
    let owner_node = arena.get(owner)?;
    if !owner_node.children().is_empty()
        || !owner_node.element.children().is_empty()
        || !std::ptr::eq(owner_node.element.as_any(), element.as_any())
    {
        return None;
    }
    prepare_custom_leaf_paint(element, false, recording_context)
}

struct PreparedCustomWrapperFill {
    slot: u16,
    op: crate::view::paint::DrawRectOp,
    payload_identity: crate::view::paint::PaintPayloadIdentity,
}

struct PreparedCustomWrapperPaint {
    bounds: Rect,
    before_children: Vec<PreparedCustomWrapperFill>,
    after_children: Vec<PreparedCustomWrapperFill>,
}

impl PreparedCustomWrapperPaint {
    fn chunk_id(
        owner: NodeKey,
        phase: crate::view::paint::PaintNodePhase,
        slot: u16,
    ) -> crate::view::paint::PaintChunkId {
        crate::view::paint::PaintChunkId {
            owner,
            scope: crate::view::paint::PaintPropertyScope::SelfPaint,
            phase,
            slot,
            role: crate::view::paint::PaintChunkRole::SelfDecoration,
        }
    }

    fn metadata_for(
        &self,
        fill: &PreparedCustomWrapperFill,
        owner: NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        phase: crate::view::paint::PaintNodePhase,
    ) -> crate::view::paint::PaintChunkMetadata {
        crate::view::paint::PaintChunkMetadata {
            id: Self::chunk_id(owner, phase, fill.slot),
            owner,
            bounds: self.bounds,
            properties,
            content_revision,
            payload_identity: fill.payload_identity.clone(),
        }
    }

    fn metadata_plan(
        &self,
        owner: NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
    ) -> crate::view::paint::PaintNodePlan<crate::view::paint::PaintChunkMetadata> {
        crate::view::paint::PaintNodePlan {
            before_children: self
                .before_children
                .iter()
                .map(|fill| {
                    self.metadata_for(
                        fill,
                        owner,
                        properties,
                        content_revision,
                        crate::view::paint::PaintNodePhase::BeforeChildren,
                    )
                })
                .collect(),
            after_children: self
                .after_children
                .iter()
                .map(|fill| {
                    self.metadata_for(
                        fill,
                        owner,
                        properties,
                        content_revision,
                        crate::view::paint::PaintNodePhase::AfterChildren,
                    )
                })
                .collect(),
        }
    }

    fn artifact_for(
        bounds: Rect,
        fill: PreparedCustomWrapperFill,
        owner: NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        phase: crate::view::paint::PaintNodePhase,
    ) -> crate::view::paint::PaintArtifact {
        crate::view::paint::PaintArtifact {
            target: Default::default(),
            chunks: vec![crate::view::paint::PaintChunk {
                id: Self::chunk_id(owner, phase, fill.slot),
                owner,
                op_range: 0..1,
                bounds,
                properties,
                content_revision,
                payload_identity: fill.payload_identity,
            }],
            ops: vec![crate::view::paint::PaintOp::DrawRect(fill.op)],
            clip_nodes: Vec::new(),
            effect_nodes: Vec::new(),
            owner_nodes: vec![crate::view::paint::PaintOwnerSnapshot {
                owner,
                parent: None,
            }],
        }
    }

    fn artifact_plan(
        self,
        owner: NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
    ) -> crate::view::paint::PaintNodePlan<crate::view::paint::PaintArtifact> {
        let bounds = self.bounds;
        crate::view::paint::PaintNodePlan {
            before_children: self
                .before_children
                .into_iter()
                .map(|fill| {
                    Self::artifact_for(
                        bounds,
                        fill,
                        owner,
                        properties,
                        content_revision,
                        crate::view::paint::PaintNodePhase::BeforeChildren,
                    )
                })
                .collect(),
            after_children: self
                .after_children
                .into_iter()
                .map(|fill| {
                    Self::artifact_for(
                        bounds,
                        fill,
                        owner,
                        properties,
                        content_revision,
                        crate::view::paint::PaintNodePhase::AfterChildren,
                    )
                })
                .collect(),
        }
    }
}

fn custom_wrapper_topology_is_canonical<T: ElementTrait + ?Sized>(
    element: &T,
    owner: NodeKey,
    arena: &NodeArena,
) -> bool {
    let Some(owner_node) = arena.get(owner) else {
        return false;
    };
    if !std::ptr::eq(owner_node.element.as_any(), element.as_any())
        || owner_node.children().is_empty()
        || owner_node.element.children() != owner_node.children()
    {
        return false;
    }
    let children = owner_node.children().to_vec();
    drop(owner_node);

    let mut seen = FxHashSet::default();
    seen.insert(owner);
    let mut stack = children
        .into_iter()
        .rev()
        .map(|child| (owner, child))
        .collect::<Vec<_>>();
    while let Some((expected_parent, key)) = stack.pop() {
        if !seen.insert(key) {
            return false;
        }
        let Some(node) = arena.get(key) else {
            return false;
        };
        if node.parent() != Some(expected_parent) || node.element.children() != node.children() {
            return false;
        }
        let children = node.children().to_vec();
        drop(node);
        stack.extend(children.into_iter().rev().map(|child| (key, child)));
    }
    true
}

fn prepare_custom_wrapper_phase(
    bounds: Rect,
    commands: &[CustomWrapperFillRect],
) -> Option<Vec<PreparedCustomWrapperFill>> {
    commands
        .iter()
        .enumerate()
        .map(|(index, command)| {
            let slot = u16::try_from(index).ok()?;
            if !same_custom_leaf_rect_bits(command.rect, bounds)
                || command
                    .rgba
                    .iter()
                    .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(channel))
                || !command.opacity.is_finite()
                || !(0.0..=1.0).contains(&command.opacity)
            {
                return None;
            }
            let mut fill_color = command.rgba;
            fill_color[3] *= command.opacity;
            let op = crate::view::paint::DrawRectOp {
                params: RectPassParams {
                    position: [command.rect.x, command.rect.y],
                    size: [command.rect.width, command.rect.height],
                    fill_color,
                    opacity: 1.0,
                    ..Default::default()
                },
                mode: RectRenderMode::FillOnly,
            };
            let payload_identity =
                crate::view::paint::PaintPayloadIdentity::prepared_shadows_with_decoration(
                    std::iter::empty(),
                    std::iter::once(&op),
                )?;
            Some(PreparedCustomWrapperFill {
                slot,
                op,
                payload_identity,
            })
        })
        .collect()
}

fn prepare_custom_wrapper_paint<T: ElementTrait + ?Sized>(
    element: &T,
    expected_owner: Option<NodeKey>,
    arena: &NodeArena,
    deferred_phase_root: bool,
    recording_context: crate::view::paint::PaintRecordingContext,
) -> Option<PreparedCustomWrapperPaint> {
    if element.children().is_empty()
        || deferred_phase_root
        || element.is_deferred_to_root_viewport_render()
        || element.has_active_animator()
        || element
            .placement_eligibility_metadata()
            .contains_runtime_layout_state
        || !matches!(
            recording_context.opacity_authority,
            crate::view::paint::PaintOpacityAuthority::Baked
        )
        || recording_context.inside_text_area
        || recording_context.text_area_selection.is_some()
        || recording_context.text_area_preedit.is_some()
    {
        return None;
    }

    let indexed_owner = arena.find_by_stable_id(element.stable_id())?;
    let owner = expected_owner.unwrap_or(indexed_owner);
    if owner != indexed_owner || !custom_wrapper_topology_is_canonical(element, owner, arena) {
        return None;
    }

    let snapshot = element.box_model_snapshot();
    let promotion = element.promotion_node_info();
    if !snapshot.should_render
        || snapshot.node_id != element.stable_id()
        || !snapshot.border_radius.is_finite()
        || snapshot.border_radius.to_bits() != 0.0_f32.to_bits()
        || !promotion.opacity.is_finite()
        || promotion.opacity.to_bits() != 1.0_f32.to_bits()
        || promotion.has_rounded_clip
        || promotion.has_box_shadow
        || promotion.has_border
        || promotion.is_scroll_container
    {
        return None;
    }

    let bounds = Rect {
        x: snapshot.x + recording_context.paint_offset[0],
        y: snapshot.y + recording_context.paint_offset[1],
        width: snapshot.width,
        height: snapshot.height,
    };
    if !has_canonical_custom_leaf_bounds(bounds) {
        return None;
    }

    let mut recorder = CustomWrapperPaintRecorder::new();
    element.record_custom_wrapper_paint(CustomWrapperPaintContext { bounds }, &mut recorder);
    if recorder.before_children.is_empty() && recorder.after_children.is_empty() {
        return None;
    }
    Some(PreparedCustomWrapperPaint {
        bounds,
        before_children: prepare_custom_wrapper_phase(bounds, &recorder.before_children)?,
        after_children: prepare_custom_wrapper_phase(bounds, &recorder.after_children)?,
    })
}

fn prepare_owned_custom_wrapper_paint<T: ElementTrait + ?Sized>(
    element: &T,
    owner: NodeKey,
    properties: crate::view::compositor::property_tree::PropertyTreeState,
    contents_properties: crate::view::compositor::property_tree::PropertyTreeState,
    arena: &NodeArena,
    recording_context: crate::view::paint::PaintRecordingContext,
) -> Option<PreparedCustomWrapperPaint> {
    if properties != Default::default() || contents_properties != Default::default() {
        return None;
    }
    prepare_custom_wrapper_paint(element, Some(owner), arena, false, recording_context)
}

fn has_canonical_custom_leaf_bounds(rect: Rect) -> bool {
    rect.x.is_finite()
        && rect.y.is_finite()
        && rect.width.is_finite()
        && rect.width > 0.0
        && rect.height.is_finite()
        && rect.height > 0.0
        && (rect.x + rect.width).is_finite()
        && (rect.y + rect.height).is_finite()
}

fn same_custom_leaf_rect_bits(lhs: Rect, rhs: Rect) -> bool {
    lhs.x.to_bits() == rhs.x.to_bits()
        && lhs.y.to_bits() == rhs.y.to_bits()
        && lhs.width.to_bits() == rhs.width.to_bits()
        && lhs.height.to_bits() == rhs.height.to_bits()
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

/// Owning, backend-independent transform payload observed by the compositor.
///
/// Keeping the public trait boundary in raw column-major bits avoids exposing
/// `glam::Mat4` as part of the host contract while preserving every float bit,
/// including non-finite values that must remain visible to fail-closed checks.
#[doc(hidden)]
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct ViewportTransformSnapshot {
    matrix: [f32; 16],
}

impl ViewportTransformSnapshot {
    #[doc(hidden)]
    pub const fn from_cols_array(matrix: [f32; 16]) -> Self {
        Self { matrix }
    }

    #[doc(hidden)]
    pub const fn to_cols_array(self) -> [f32; 16] {
        self.matrix
    }

    pub(crate) fn from_matrix(matrix: Mat4) -> Self {
        Self::from_cols_array(matrix.to_cols_array())
    }
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
    /// Collector DFS order. This is diagnostic/install order only; paint
    /// traversal remains the coverage walk's live DOM DFS.
    pub(crate) node_order: Vec<NodeKey>,
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
    /// Exact max width used to build `cache_key`. Auto-sized roots can have a
    /// different final place-time inner width, so the consumer must not
    /// substitute that later value.
    inner_width: f32,
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
    /// Inner width that produced `cache_key` and the install plan.
    build_inner_width: f32,
    /// Place-time inner width to which this plan was last applied. Auto roots
    /// can settle to a different width than their shaping constraint.
    applied_inner_width: f32,
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
#[derive(Clone)]
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
        witness: InlineIfcAtomicInstallWitness,
    },
}

/// Origin-independent proof tying one retained atomic install to the exact
/// shaped inline-box mapping and the live child that supplied it.
#[derive(Clone, Debug)]
struct InlineIfcAtomicInstallWitness {
    node_key: NodeKey,
    stable_id: u64,
    source: InlineIfcSourceId,
    inline_box_id: u64,
    insertion_byte: usize,
    line_index: usize,
    measurement: InlineIfcMeasuredAtomicBox,
    raw_rect: InlineIfcPaintRect,
    aligned_rect: InlineIfcPaintRect,
    vertical_align: VerticalAlign,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum OwningInlineIfcRootWitnessDamage {
    MissingCurrent,
    Pending,
    ChildrenSnapshot,
    PlanMissing,
    PlanDuplicate,
    InstalledMissing,
    InstalledDuplicate,
    CacheKey,
    WrongKind,
    TextPlanPayloadSwap,
    AtomicStableId,
    AtomicSource,
    AtomicInlineBoxId,
    AtomicInsertionByte,
    AtomicLineIndex,
    AtomicMeasurementMaxWidth,
    AtomicMeasurementAvailableHeight,
    AtomicMeasurementViewport,
    AtomicMeasurementPercentBase,
    AtomicMeasurementSizing,
    AtomicMeasurementSize,
    AtomicRawRect,
    AtomicAlignedRect,
    AtomicVerticalAlign,
    AtomicPackageZeroPlacements,
    AtomicPackageDuplicatePlacements,
    LayoutDirty,
    PlacementDirty,
}

impl InlineIfcNodeInstallOp {
    fn node_key(&self) -> NodeKey {
        match self {
            Self::Span { node_key, .. } | Self::Text { node_key, .. } => *node_key,
            Self::Atomic { witness } => witness.node_key,
        }
    }
}

fn exact_inline_ifc_atomic_placement(
    package: &InlineIfcAtomicBoxPlacementPackage,
    source: InlineIfcSourceId,
) -> Option<&InlineIfcAtomicBoxPlacement> {
    if package.source != source || package.placements.len() != 1 {
        return None;
    }
    let placement = &package.placements[0];
    (placement.source == source).then_some(placement)
}

fn f32_bits_eq(left: f32, right: f32) -> bool {
    left.to_bits() == right.to_bits()
}

fn option_f32_bits_eq(left: Option<f32>, right: Option<f32>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => f32_bits_eq(left, right),
        (None, None) => true,
        _ => false,
    }
}

fn inline_ifc_size_bits_eq(left: InlineIfcSize, right: InlineIfcSize) -> bool {
    f32_bits_eq(left.width, right.width) && f32_bits_eq(left.height, right.height)
}

fn inline_ifc_rect_bits_eq(left: InlineIfcPaintRect, right: InlineIfcPaintRect) -> bool {
    f32_bits_eq(left.x, right.x)
        && f32_bits_eq(left.y, right.y)
        && f32_bits_eq(left.width, right.width)
        && f32_bits_eq(left.height, right.height)
}

fn inline_ifc_atomic_measurement_bits_eq(
    left: &InlineIfcMeasuredAtomicBox,
    right: &InlineIfcMeasuredAtomicBox,
) -> bool {
    let left_constraints = left.constraints;
    let right_constraints = right.constraints;
    option_f32_bits_eq(left_constraints.max_width, right_constraints.max_width)
        && option_f32_bits_eq(
            left_constraints.available_height,
            right_constraints.available_height,
        )
        && match (left_constraints.viewport, right_constraints.viewport) {
            (Some(left), Some(right)) => inline_ifc_size_bits_eq(left, right),
            (None, None) => true,
            _ => false,
        }
        && option_f32_bits_eq(
            left_constraints.percent_base.width,
            right_constraints.percent_base.width,
        )
        && option_f32_bits_eq(
            left_constraints.percent_base.height,
            right_constraints.percent_base.height,
        )
        && option_f32_bits_eq(
            left_constraints.sizing.min_width,
            right_constraints.sizing.min_width,
        )
        && option_f32_bits_eq(
            left_constraints.sizing.max_width,
            right_constraints.sizing.max_width,
        )
        && option_f32_bits_eq(
            left_constraints.sizing.min_height,
            right_constraints.sizing.min_height,
        )
        && option_f32_bits_eq(
            left_constraints.sizing.max_height,
            right_constraints.sizing.max_height,
        )
        && match (
            left_constraints.sizing.intrinsic_size,
            right_constraints.sizing.intrinsic_size,
        ) {
            (Some(left), Some(right)) => {
                f32_bits_eq(left.min_content_width, right.min_content_width)
                    && f32_bits_eq(left.max_content_width, right.max_content_width)
                    && option_f32_bits_eq(left.preferred_width, right.preferred_width)
                    && option_f32_bits_eq(left.preferred_height, right.preferred_height)
            }
            (None, None) => true,
            _ => false,
        }
        && inline_ifc_size_bits_eq(left.measured_size, right.measured_size)
}

fn inline_ifc_atomic_witness_bits_eq(
    left: &InlineIfcAtomicInstallWitness,
    right: &InlineIfcAtomicInstallWitness,
) -> bool {
    left.node_key == right.node_key
        && left.stable_id == right.stable_id
        && left.source == right.source
        && left.inline_box_id == right.inline_box_id
        && left.insertion_byte == right.insertion_byte
        && left.line_index == right.line_index
        && left.vertical_align == right.vertical_align
        && inline_ifc_atomic_measurement_bits_eq(&left.measurement, &right.measurement)
        && inline_ifc_rect_bits_eq(left.raw_rect, right.raw_rect)
        && inline_ifc_rect_bits_eq(left.aligned_rect, right.aligned_rect)
}

fn inline_ifc_atomic_layout_placement(
    flow_origin_x: f32,
    flow_origin_y: f32,
    visual_offset_x: f32,
    visual_offset_y: f32,
    content_top_offset: f32,
    root_placement: LayoutPlacement,
    aligned_rect: InlineIfcPaintRect,
) -> LayoutPlacement {
    LayoutPlacement {
        parent_x: flow_origin_x + aligned_rect.x,
        parent_y: flow_origin_y + aligned_rect.y - content_top_offset,
        visual_offset_x,
        visual_offset_y,
        available_width: aligned_rect.width.max(1.0),
        available_height: aligned_rect.height.max(1.0),
        viewport_width: root_placement.viewport_width,
        viewport_height: root_placement.viewport_height,
        percent_base_width: root_placement.percent_base_width,
        percent_base_height: root_placement.percent_base_height,
    }
}

fn layout_placement_bits_eq(left: LayoutPlacement, right: LayoutPlacement) -> bool {
    f32_bits_eq(left.parent_x, right.parent_x)
        && f32_bits_eq(left.parent_y, right.parent_y)
        && f32_bits_eq(left.visual_offset_x, right.visual_offset_x)
        && f32_bits_eq(left.visual_offset_y, right.visual_offset_y)
        && f32_bits_eq(left.available_width, right.available_width)
        && f32_bits_eq(left.available_height, right.available_height)
        && f32_bits_eq(left.viewport_width, right.viewport_width)
        && f32_bits_eq(left.viewport_height, right.viewport_height)
        && option_f32_bits_eq(left.percent_base_width, right.percent_base_width)
        && option_f32_bits_eq(left.percent_base_height, right.percent_base_height)
}

/// Atomic placement authority covers the whole hosted subtree, not just the
/// atomic shell. Read every node's local state directly: cached subtree
/// aggregates are pass-scoped and may not have been refreshed at paint
/// preflight time.
fn inline_ifc_atomic_subtree_layout_placement_clean(arena: &NodeArena, root: NodeKey) -> bool {
    let mask = DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT);
    let mut pending = vec![root];
    let mut visited = FxHashSet::default();
    while let Some(node_key) = pending.pop() {
        if !visited.insert(node_key) {
            return false;
        }
        let Some(node) = arena.get(node_key) else {
            return false;
        };
        if arena.arena_local_dirty(node_key).intersects(mask)
            || node.element.local_dirty_flags().intersects(mask)
        {
            return false;
        }
        pending.extend(node.children.iter().copied());
    }
    true
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
    /// Atomic inline box: complete shaped + alignment witness.
    Atomic {
        witness: InlineIfcAtomicInstallWitness,
    },
}

fn inline_ifc_root_geometry(
    context: Option<&InlineFormattingContext>,
    arena: &NodeArena,
    sources_by_node: &FxHashMap<NodeKey, InlineIfcSourceId>,
    node_order: &[NodeKey],
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
    let mut nodes = Vec::new();
    for &node_key in node_order.iter().filter(|&&node_key| node_key != root_key) {
        let source = sources_by_node.get(&node_key).copied()?;
        let node = arena.get(node_key)?;
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
        let placement = exact_inline_ifc_atomic_placement(&package, source)?;
        // Opaque hosts retain the legacy baseline layout default. Paint
        // promotion does not trust this fallback: the owning-root witness
        // separately requires the generic accessor to return `Some`.
        let vertical_align = node
            .element
            .inline_atomic_vertical_align()
            .unwrap_or(VerticalAlign::Baseline);
        let line = snapshot.lines.get(placement.line_index)?;
        let mut aligned_rect = placement.rect;
        let item_height = aligned_rect.height.max(0.0);
        let align_offset = baseline_cross_offset(
            line.baseline,
            line.height,
            item_height,
            item_height,
            vertical_align,
        );
        aligned_rect.y = line.y + align_offset;
        merge(aligned_rect, &mut content);
        nodes.push(InlineIfcRootNodeGeometry {
            node_key,
            kind: InlineIfcRootNodeGeometryKind::Atomic {
                witness: InlineIfcAtomicInstallWitness {
                    node_key,
                    stable_id: node.element.stable_id(),
                    source,
                    inline_box_id: placement.id,
                    insertion_byte: placement.insertion_byte,
                    line_index: placement.line_index,
                    measurement: placement.measurement.clone(),
                    raw_rect: placement.rect,
                    aligned_rect,
                    vertical_align,
                },
            },
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
            InlineIfcRootNodeGeometryKind::Atomic { witness } => {
                plan.push(InlineIfcNodeInstallOp::Atomic { witness });
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
            node_order: vec![input.root_key],
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
            node_order: state.node_order,
        })
    }

    fn collect_for_taken_root(
        arena: &NodeArena,
        input: ElementInlineIfcMetadataCollectorInput,
        root: &Element,
    ) -> Option<ElementInlineIfcMetadataCollectorOutput> {
        if !root.is_owning_inline_ifc_root_role() {
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
            node_order: vec![input.root_key],
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
            node_order: state.node_order,
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
    node_order: Vec<NodeKey>,
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
            self.node_order.push(key);

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
                        measurement: element.inline_atomic_measurement_snapshot().unwrap_or_else(
                            || self.legacy_atomic_measurement(element.measured_size()),
                        ),
                    }
                }
            } else {
                CollectedNode::Atomic {
                    source,
                    measurement: element
                        .inline_atomic_measurement_snapshot()
                        .unwrap_or_else(|| self.legacy_atomic_measurement(element.measured_size())),
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

    /// Layout-only compatibility for opaque atomic hosts. Their measured size
    /// must still participate in the IFC so legacy layout remains correct;
    /// paint promotion separately requires the exact generic snapshot and
    /// therefore rejects these hosts in the owning-root witness.
    fn legacy_atomic_measurement(&self, measured_size: (f32, f32)) -> InlineIfcMeasuredAtomicBox {
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

pub(crate) struct PreparedElementInlineIfcDecorationPayload {
    pub(crate) bounds: Rect,
    pub(crate) ops: Vec<crate::view::paint::PreparedInlineIfcDecorationOp>,
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
    scrollbar_interaction_pending: bool,
    sampled_scrollbar_alpha: f32,
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
    fn is_exact_retained_scroll_content_leaf(&self) -> bool {
        self.is_exact_retained_scroll_content_leaf_with_transform(false)
    }

    fn is_exact_retained_scroll_transform_content_leaf(&self) -> bool {
        self.is_exact_retained_scroll_content_leaf_with_transform(true)
    }

    fn is_exact_retained_scroll_content_leaf_with_transform(
        &self,
        requires_transform: bool,
    ) -> bool {
        self.children.is_empty()
            && self.layout_state.should_render
            && self.core.should_paint
            && self.layout_state.layout_position.x.is_finite()
            && self.layout_state.layout_position.y.is_finite()
            && self.layout_state.layout_size.width.is_finite()
            && self.layout_state.layout_size.height.is_finite()
            && self.layout_state.layout_size.width > 0.0
            && self.layout_state.layout_size.height > 0.0
            && self.opacity.to_bits() == 1.0_f32.to_bits()
            && self.scroll_direction == ScrollDirection::None
            && self.resolved_transform.is_some() == requires_transform
            && self.box_shadows.is_empty()
            && !self.has_active_layout_transition()
            && !self.has_active_animator()
            && !self.inline_ifc_owned_by_root
            && !self.is_owning_inline_ifc_root_role()
            && !self.is_fragmentable_inline_element()
            && !self.should_append_to_root_viewport_render()
            && self.absolute_clip_scissor_rect().is_none()
            && !self.promotion_node_info().has_rounded_clip
            && self.computed_style.position.mode() != PositionMode::Absolute
    }

    /// Recorder-owned oracle for the only content root admitted by the A1
    /// canary. It reproduces Element's existing pixel-snap derivation without
    /// trusting a component hook to preserve or reconstruct the offset.
    pub(crate) fn exact_retained_scroll_content_recording_offset(
        &self,
        parent_offset: [f32; 2],
    ) -> Option<[f32; 2]> {
        if !self.is_exact_retained_scroll_content_leaf()
            || parent_offset.iter().any(|value| !value.is_finite())
        {
            return None;
        }
        let paint_x = self.layout_state.layout_position.x + parent_offset[0];
        let paint_y = self.layout_state.layout_position.y + parent_offset[1];
        Some([
            parent_offset[0] + round_layout_value(paint_x) - paint_x,
            parent_offset[1] + round_layout_value(paint_y) - paint_y,
        ])
    }

    /// Offset-zero recorder oracle for the direct transformed content leaf.
    /// The sibling method keeps the original untransformed-leaf contract
    /// exact while sharing the same pixel-snap derivation.
    pub(crate) fn exact_retained_scroll_transform_content_recording_offset(
        &self,
        parent_offset: [f32; 2],
    ) -> Option<[f32; 2]> {
        if !self.is_exact_retained_scroll_transform_content_leaf()
            || parent_offset.iter().any(|value| !value.is_finite())
        {
            return None;
        }
        let paint_x = self.layout_state.layout_position.x + parent_offset[0];
        let paint_y = self.layout_state.layout_position.y + parent_offset[1];
        Some([
            parent_offset[0] + round_layout_value(paint_x) - paint_x,
            parent_offset[1] + round_layout_value(paint_y) - paint_y,
        ])
    }

    /// Strict private-style and layout gate for M10E1A. Property topology,
    /// promotion, and frame target checks remain planner/viewport authority.
    pub(crate) fn exact_retained_scroll_host_admission(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        scale_factor: f32,
    ) -> Option<RetainedScrollHostAdmissionSnapshot> {
        self.exact_retained_scroll_host_admission_with_parent(owner, arena, scale_factor, None)
    }

    /// Closed C1/C2a sibling of the direct-leaf scroll admission. Keeping this
    /// separate makes the original B0 admission continue to prove that its
    /// content child has no descendants.
    pub(crate) fn exact_retained_scroll_text_area_subtree_admission(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        scale_factor: f32,
    ) -> Option<RetainedScrollTextAreaSubtreeAdmissionSnapshot> {
        let (source_bounds, scroll, content_wrapper) =
            self.exact_retained_scroll_host_shell(owner, arena, scale_factor, None)?;
        let wrapper_node = arena.get(content_wrapper)?;
        let wrapper = wrapper_node.element.as_any().downcast_ref::<Element>()?;
        let [text_area_root] = wrapper.children.as_slice() else {
            return None;
        };
        let text_area_root = *text_area_root;
        let text_area_node = arena.get(text_area_root)?;
        let text_area = text_area_node
            .element
            .as_any()
            .downcast_ref::<super::TextArea>()?;
        let normalization = [scroll.offset[0], scroll.offset[1]];
        let wrapper_recording_offset =
            wrapper.exact_retained_scroll_content_wrapper_recording_offset(normalization)?;
        let paint_grammar = if text_area.exact_retained_property_scroll_glyph_subtree(
            text_area_root,
            arena,
            wrapper_recording_offset,
        ) {
            super::text_area::RetainedTextAreaPaintGrammar::GlyphOnly
        } else {
            text_area.exact_retained_property_scroll_selection_glyph_subtree(
                text_area_root,
                arena,
                wrapper_recording_offset,
            )?
        };
        if arena.parent_of(content_wrapper) != Some(owner)
            || arena.parent_of(text_area_root) != Some(content_wrapper)
            || arena.children_of(content_wrapper) != [text_area_root]
            || !scroll_content_bounds_match(wrapper, scroll)
        {
            return None;
        }
        Some(RetainedScrollTextAreaSubtreeAdmissionSnapshot {
            boundary_root: owner,
            stable_id: self.stable_id(),
            content_wrapper,
            content_wrapper_stable_id: wrapper.stable_id(),
            text_area_root,
            text_area_stable_id: text_area.stable_id(),
            paint_grammar,
            source_bounds,
            scroll,
        })
    }

    /// Graph-inert C3a sibling.  This preserves the established C1/C2
    /// admissions and freezes only the TextArea source oracle; no retained
    /// scene path selects it yet.
    #[allow(dead_code)] // Source-level authority only; no scene selector consumes it yet.
    pub(crate) fn exact_retained_scroll_atomic_projection_text_area_subtree_admission(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        scale_factor: f32,
    ) -> Option<RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot> {
        let (source_bounds, scroll, content_wrapper) =
            self.exact_retained_scroll_host_shell(owner, arena, scale_factor, None)?;
        let wrapper_node = arena.get(content_wrapper)?;
        let wrapper = wrapper_node.element.as_any().downcast_ref::<Element>()?;
        let [text_area_root] = wrapper.children.as_slice() else {
            return None;
        };
        let text_area_root = *text_area_root;
        let text_area_node = arena.get(text_area_root)?;
        let text_area = text_area_node
            .element
            .as_any()
            .downcast_ref::<super::TextArea>()?;
        let normalization = [scroll.offset[0], scroll.offset[1]];
        let wrapper_recording_offset =
            wrapper.exact_retained_scroll_content_wrapper_recording_offset(normalization)?;
        let paint_grammar = text_area.exact_retained_property_scroll_atomic_projection_subtree(
            text_area_root,
            arena,
            wrapper_recording_offset,
        )?;
        if arena.parent_of(content_wrapper) != Some(owner)
            || arena.parent_of(text_area_root) != Some(content_wrapper)
            || arena.children_of(content_wrapper) != [text_area_root]
            || !scroll_content_bounds_match(wrapper, scroll)
        {
            return None;
        }
        Some(
            RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot {
                boundary_root: owner,
                stable_id: self.stable_id(),
                content_wrapper,
                content_wrapper_stable_id: wrapper.stable_id(),
                text_area_root,
                text_area_stable_id: text_area.stable_id(),
                paint_grammar,
                source_bounds,
                scroll,
            },
        )
    }

    /// Graph-inert sibling for a root-owned selection plus one realized
    /// atomic projection.  The component oracle is rerun by recorders; this
    /// snapshot alone is never paint authority.
    #[allow(dead_code)] // Called only by focused recorder tests in this migration segment.
    pub(crate) fn exact_retained_scroll_atomic_projection_selection_text_area_subtree_admission(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        scale_factor: f32,
    ) -> Option<RetainedScrollAtomicProjectionSelectionTextAreaSubtreeAdmissionSnapshot> {
        let (source_bounds, scroll, content_wrapper) =
            self.exact_retained_scroll_host_shell(owner, arena, scale_factor, None)?;
        let wrapper_node = arena.get(content_wrapper)?;
        let wrapper = wrapper_node.element.as_any().downcast_ref::<Element>()?;
        let [text_area_root] = wrapper.children.as_slice() else {
            return None;
        };
        let text_area_root = *text_area_root;
        let text_area_node = arena.get(text_area_root)?;
        let text_area = text_area_node
            .element
            .as_any()
            .downcast_ref::<super::TextArea>()?;
        let normalization = [scroll.offset[0], scroll.offset[1]];
        let wrapper_recording_offset =
            wrapper.exact_retained_scroll_content_wrapper_recording_offset(normalization)?;
        let paint_grammar = text_area
            .exact_retained_property_scroll_atomic_projection_selection_subtree(
                text_area_root,
                arena,
                wrapper_recording_offset,
            )?;
        if arena.parent_of(content_wrapper) != Some(owner)
            || arena.parent_of(text_area_root) != Some(content_wrapper)
            || arena.children_of(content_wrapper) != [text_area_root]
            || !scroll_content_bounds_match(wrapper, scroll)
        {
            return None;
        }
        Some(
            RetainedScrollAtomicProjectionSelectionTextAreaSubtreeAdmissionSnapshot {
                boundary_root: owner,
                stable_id: self.stable_id(),
                content_wrapper,
                content_wrapper_stable_id: wrapper.stable_id(),
                text_area_root,
                text_area_stable_id: text_area.stable_id(),
                paint_grammar,
                source_bounds,
                scroll,
            },
        )
    }

    /// Graph-inert focused-glyph sibling for one atomic projection.  This
    /// freezes source and caret facts only; scene planning remains absent.
    #[allow(dead_code)]
    pub(crate) fn exact_retained_scroll_focused_atomic_projection_text_area_subtree_admission(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        scale_factor: f32,
    ) -> Option<RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot> {
        let (source_bounds, scroll, content_wrapper) =
            self.exact_retained_scroll_host_shell(owner, arena, scale_factor, None)?;
        let wrapper_node = arena.get(content_wrapper)?;
        let wrapper = wrapper_node.element.as_any().downcast_ref::<Element>()?;
        let [text_area_root] = wrapper.children.as_slice() else {
            return None;
        };
        let text_area_root = *text_area_root;
        let text_area_node = arena.get(text_area_root)?;
        let text_area = text_area_node
            .element
            .as_any()
            .downcast_ref::<super::TextArea>()?;
        let normalization = [scroll.offset[0], scroll.offset[1]];
        let wrapper_recording_offset =
            wrapper.exact_retained_scroll_content_wrapper_recording_offset(normalization)?;
        let paint_grammar = text_area
            .exact_retained_property_scroll_focused_atomic_projection_glyph_subtree(
                text_area_root,
                arena,
                wrapper_recording_offset,
            )?;
        if arena.parent_of(content_wrapper) != Some(owner)
            || arena.parent_of(text_area_root) != Some(content_wrapper)
            || arena.children_of(content_wrapper) != [text_area_root]
            || !scroll_content_bounds_match(wrapper, scroll)
        {
            return None;
        }
        Some(
            RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot {
                boundary_root: owner,
                stable_id: self.stable_id(),
                content_wrapper,
                content_wrapper_stable_id: wrapper.stable_id(),
                text_area_root,
                text_area_stable_id: text_area.stable_id(),
                paint_grammar,
                source_bounds,
                scroll,
            },
        )
    }

    pub(crate) fn exact_retained_scroll_interactive_text_area_subtree_admission(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        scale_factor: f32,
    ) -> Option<RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot> {
        let (source_bounds, scroll, content_wrapper) =
            self.exact_retained_scroll_host_shell(owner, arena, scale_factor, None)?;
        let wrapper_node = arena.get(content_wrapper)?;
        let wrapper = wrapper_node.element.as_any().downcast_ref::<Element>()?;
        let [text_area_root] = wrapper.children.as_slice() else {
            return None;
        };
        let text_area_root = *text_area_root;
        let text_area_node = arena.get(text_area_root)?;
        let text_area = text_area_node
            .element
            .as_any()
            .downcast_ref::<super::TextArea>()?;
        let normalization = [scroll.offset[0], scroll.offset[1]];
        let wrapper_recording_offset =
            wrapper.exact_retained_scroll_content_wrapper_recording_offset(normalization)?;
        let paint_grammar = text_area.exact_retained_property_scroll_interactive_subtree(
            text_area_root,
            arena,
            wrapper_recording_offset,
        )?;
        let live_wrapper_offset =
            wrapper.exact_retained_scroll_content_wrapper_recording_offset([0.0, 0.0])?;
        let caret_oracle_bounds_bits = text_area.retained_interactive_caret_oracle_bounds_bits(
            text_area_root,
            arena,
            wrapper_recording_offset,
            live_wrapper_offset,
            paint_grammar,
        )?;
        if arena.parent_of(content_wrapper) != Some(owner)
            || arena.parent_of(text_area_root) != Some(content_wrapper)
            || arena.children_of(content_wrapper) != [text_area_root]
            || !scroll_content_bounds_match(wrapper, scroll)
        {
            return None;
        }
        Some(RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot {
            boundary_root: owner,
            stable_id: self.stable_id(),
            content_wrapper,
            content_wrapper_stable_id: wrapper.stable_id(),
            text_area_root,
            text_area_stable_id: text_area.stable_id(),
            paint_grammar,
            caret_oracle_bounds_bits,
            source_bounds,
            scroll,
        })
    }

    /// Strict sibling admission for one parentless scroll host whose only
    /// content child owns a transform.  Property-tree planning separately
    /// proves that transform is a direct translation and is the sole target.
    pub(crate) fn exact_retained_scroll_transform_host_admission(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        scale_factor: f32,
    ) -> Option<RetainedScrollTransformHostAdmissionSnapshot> {
        let (source_bounds, scroll, child) =
            self.exact_retained_scroll_host_shell(owner, arena, scale_factor, None)?;
        let child_node = arena.get(child)?;
        let child_element = child_node.element.as_any().downcast_ref::<Element>()?;
        if !child_element.is_exact_retained_scroll_transform_content_leaf()
            || !scroll_content_bounds_match(child_element, scroll)
        {
            return None;
        }
        Some(RetainedScrollTransformHostAdmissionSnapshot {
            boundary_root: owner,
            stable_id: self.stable_id(),
            transform_content: child,
            transform_content_stable_id: child_element.stable_id(),
            source_bounds,
            scroll,
        })
    }

    /// Strict foundation oracle for exactly two directly nested vertical
    /// scroll hosts and one untransformed content leaf.  Property-tree parent
    /// chains and execution context remain separate planner/compiler proof.
    pub(crate) fn exact_retained_nested_scroll_scene_admission(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        scale_factor: f32,
    ) -> Option<RetainedNestedScrollSceneAdmissionSnapshot> {
        let (outer_source_bounds, outer_scroll, inner_boundary_root) =
            self.exact_retained_scroll_host_shell(owner, arena, scale_factor, None)?;
        let inner_node = arena.get(inner_boundary_root)?;
        let inner_element = inner_node.element.as_any().downcast_ref::<Element>()?;
        if !scroll_content_bounds_match(inner_element, outer_scroll) {
            return None;
        }
        let (inner_source_bounds, inner_scroll, content_leaf) = inner_element
            .exact_retained_scroll_host_shell(
                inner_boundary_root,
                arena,
                scale_factor,
                Some(owner),
            )?;
        let content_node = arena.get(content_leaf)?;
        let content_element = content_node.element.as_ref();
        if arena.parent_of(content_leaf) != Some(inner_boundary_root)
            || !is_exact_retained_nested_scroll_content_leaf(content_element, arena)
            || !scroll_content_bounds_match(content_element, inner_scroll)
        {
            return None;
        }
        Some(RetainedNestedScrollSceneAdmissionSnapshot {
            outer_boundary_root: owner,
            outer_stable_id: self.stable_id(),
            inner_boundary_root,
            inner_stable_id: inner_element.stable_id(),
            content_leaf,
            content_leaf_stable_id: content_element.stable_id(),
            outer_source_bounds,
            inner_source_bounds,
            outer_scroll,
            inner_scroll,
        })
    }

    /// B4-2B nested counterpart of the top-level scroll admission. The
    /// receiver relationship is explicit and exact; the older root oracle
    /// remains top-level-only and cannot be widened accidentally.
    pub(crate) fn exact_retained_transform_scroll_host_admission(
        &self,
        owner: NodeKey,
        receiver: NodeKey,
        arena: &NodeArena,
        scale_factor: f32,
    ) -> Option<RetainedScrollHostAdmissionSnapshot> {
        (owner != receiver && !receiver.is_null()).then_some(())?;
        self.exact_retained_scroll_host_admission_with_parent(
            owner,
            arena,
            scale_factor,
            Some(receiver),
        )
    }

    fn exact_retained_scroll_host_admission_with_parent(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        scale_factor: f32,
        expected_parent: Option<NodeKey>,
    ) -> Option<RetainedScrollHostAdmissionSnapshot> {
        let (source_bounds, scroll, child) =
            self.exact_retained_scroll_host_shell(owner, arena, scale_factor, expected_parent)?;
        let child_node = arena.get(child)?;
        let child_element = child_node.element.as_any().downcast_ref::<Element>()?;
        if !child_element.is_exact_retained_scroll_content_leaf()
            || !scroll_content_bounds_match(child_element, scroll)
        {
            return None;
        }
        Some(RetainedScrollHostAdmissionSnapshot {
            boundary_root: owner,
            stable_id: self.stable_id(),
            child,
            child_stable_id: child_element.stable_id(),
            source_bounds,
            scroll,
        })
    }

    pub(crate) fn exact_retained_scroll_content_wrapper_recording_offset(
        &self,
        parent_offset: [f32; 2],
    ) -> Option<[f32; 2]> {
        // The direct-leaf helper includes `children.is_empty()`. Spell the
        // otherwise identical wrapper contract here instead of weakening that
        // established oracle.
        if self.children.len() != 1
            || !self.layout_state.should_render
            || !self.core.should_paint
            || ![
                self.layout_state.layout_position.x,
                self.layout_state.layout_position.y,
                self.layout_state.layout_size.width,
                self.layout_state.layout_size.height,
            ]
            .into_iter()
            .all(f32::is_finite)
            || self.layout_state.layout_size.width <= 0.0
            || self.layout_state.layout_size.height <= 0.0
            || self.opacity.to_bits() != 1.0_f32.to_bits()
            || self.scroll_direction != ScrollDirection::None
            || self.resolved_transform.is_some()
            || !self.box_shadows.is_empty()
            || self.has_active_layout_transition()
            || self.has_active_animator()
            || self.inline_ifc_owned_by_root
            || self.is_owning_inline_ifc_root_role()
            || self.is_fragmentable_inline_element()
            || self.should_append_to_root_viewport_render()
            || self.absolute_clip_scissor_rect().is_some()
            || self.promotion_node_info().has_rounded_clip
            || self.computed_style.position.mode() == PositionMode::Absolute
            || parent_offset.iter().any(|value| !value.is_finite())
        {
            return None;
        }
        let paint_x = self.layout_state.layout_position.x + parent_offset[0];
        let paint_y = self.layout_state.layout_position.y + parent_offset[1];
        Some([
            parent_offset[0] + round_layout_value(paint_x) - paint_x,
            parent_offset[1] + round_layout_value(paint_y) - paint_y,
        ])
    }

    fn exact_retained_scroll_host_shell(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        scale_factor: f32,
        expected_parent: Option<NodeKey>,
    ) -> Option<(PromotionCompositeBounds, ScrollGeometrySnapshot, NodeKey)> {
        if scale_factor.to_bits() != 1.0_f32.to_bits()
            || arena.parent_of(owner) != expected_parent
            || self.children.len() != 1
            || self.scroll_direction != ScrollDirection::Vertical
            || !self.layout_state.should_render
            || !self.core.should_paint
            || self.opacity.to_bits() != 1.0_f32.to_bits()
            || self.resolved_transform.is_some()
            || !self.box_shadows.is_empty()
            || self.has_active_layout_transition()
            || self.has_active_animator()
            || self.inline_ifc_owned_by_root
            || self.is_owning_inline_ifc_root_role()
            || self.is_fragmentable_inline_element()
            || self.should_append_to_root_viewport_render()
            || self.absolute_clip_scissor_rect().is_some()
        {
            return None;
        }
        let outer_radii = normalize_corner_radii(
            self.border_radii,
            self.layout_state.layout_size.width.max(0.0),
            self.layout_state.layout_size.height.max(0.0),
        );
        if outer_radii.has_any_rounding() || self.inner_clip_radii(outer_radii).has_any_rounding() {
            return None;
        }
        let source_bounds = PromotionCompositeBounds {
            x: self.layout_state.layout_position.x,
            y: self.layout_state.layout_position.y,
            width: self.layout_state.layout_size.width,
            height: self.layout_state.layout_size.height,
            corner_radii: [0.0; 4],
        };
        let edges_are_integer = |rect: Rect| {
            [rect.x, rect.y, rect.x + rect.width, rect.y + rect.height]
                .iter()
                .all(|value| value.is_finite() && value.fract().to_bits() == 0.0_f32.to_bits())
        };
        if source_bounds.x < 0.0
            || source_bounds.y < 0.0
            || source_bounds.width <= 0.0
            || source_bounds.height <= 0.0
            || !edges_are_integer(Rect {
                x: source_bounds.x,
                y: source_bounds.y,
                width: source_bounds.width,
                height: source_bounds.height,
            })
        {
            return None;
        }
        let ScrollGeometryObservation::Exact(scroll) =
            self.scroll_geometry_observation(owner, arena)
        else {
            return None;
        };
        if scroll.configured_axis != ScrollAxisSnapshot::Vertical
            || !edges_are_integer(scroll.scrollport_rect)
            || !matches!(
                scroll.scrollbar_overlay.paint_state,
                ScrollbarPaintStateWitness::HiddenNow
                    | ScrollbarPaintStateWitness::NotPaintable
                    | ScrollbarPaintStateWitness::OpaqueNow
                    | ScrollbarPaintStateWitness::TranslucentNow
            )
        {
            return None;
        }
        let child = self.children[0];
        arena.get(child)?;
        Some((source_bounds, scroll, child))
    }

    #[cfg(test)]
    pub(crate) fn set_resolved_transform_for_test(&mut self, transform: Option<Mat4>) {
        self.resolved_transform = transform;
    }

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

    /// Snapshot the exact proposal and resolved sizing inputs that produced
    /// this host's retained atomic size. `intrinsic_size` is supplied by
    /// wrappers such as Image/Svg after their resource-aware measurement.
    pub(crate) fn inline_atomic_measurement_snapshot_with_intrinsic(
        &self,
        intrinsic_size: Option<(f32, f32)>,
    ) -> Option<InlineIfcMeasuredAtomicBox> {
        let proposal = self.last_layout_proposal?;
        let resolve = |value: SizeValue, percent_base: Option<f32>| match value {
            SizeValue::Auto => Some(None),
            SizeValue::Length(length) => resolve_px_with_base(
                length,
                percent_base,
                proposal.viewport_width,
                proposal.viewport_height,
            )
            .map(|value| Some(value.max(0.0))),
        };
        let min_width = resolve(self.computed_style.min_width, proposal.percent_base_width)?;
        let max_width = resolve(self.computed_style.max_width, proposal.percent_base_width)?;
        let min_height = resolve(self.computed_style.min_height, proposal.percent_base_height)?;
        let max_height = resolve(self.computed_style.max_height, proposal.percent_base_height)?;
        let preferred_width = resolve(self.computed_style.width, proposal.percent_base_width)?;
        let preferred_height = resolve(self.computed_style.height, proposal.percent_base_height)?;
        let intrinsic_size = intrinsic_size.map(|(width, height)| {
            InlineIfcIntrinsicSize::new(
                width,
                width,
                preferred_width.or(Some(width)),
                preferred_height.or(Some(height)),
            )
        });
        let measured_size = self.measured_size();
        Some(InlineIfcMeasuredAtomicBox::new(
            InlineIfcSize::new(measured_size.0, measured_size.1),
            InlineIfcAtomicMeasureConstraints {
                max_width: Some(proposal.width.max(0.0)),
                available_height: Some(proposal.height.max(0.0)),
                viewport: Some(InlineIfcSize::new(
                    proposal.viewport_width,
                    proposal.viewport_height,
                )),
                percent_base: InlineIfcPercentBase::new(
                    proposal.percent_base_width,
                    proposal.percent_base_height,
                ),
                sizing: InlineIfcAtomicSizingRules {
                    min_width,
                    max_width,
                    min_height,
                    max_height,
                    intrinsic_size,
                },
            },
        ))
    }

    /// Pure paint-preflight proof that this fragmentable `Element` is the
    /// live owner of a fully-installed, non-atomic IFC.  This deliberately
    /// derives authority from the current install plus a fresh collector
    /// snapshot; `inline_ifc_owned_by_root == false` alone is not ownership
    /// evidence, and the install vector order is not paint/DOM order.
    fn owning_inline_ifc_root_paint_witness(
        &self,
        arena: &NodeArena,
    ) -> Result<(), ShadowPaintBlocker> {
        let reject = || ShadowPaintBlocker::MissingPreparedInlineRoot;
        if !self.is_owning_inline_ifc_root_role()
            || self.layout_dirty
            || self
                .dirty_flags
                .intersects(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT))
            || self.inline_ifc_layout_call_site.pending.is_some()
        {
            return Err(reject());
        }
        let root_key = arena
            .find_by_stable_id(self.stable_id())
            .ok_or_else(reject)?;
        if arena
            .arena_local_dirty(root_key)
            .intersects(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT))
        {
            return Err(reject());
        }
        let install = self
            .inline_ifc_layout_call_site
            .current
            .as_ref()
            .ok_or_else(reject)?;
        let placement = self.last_layout_placement.ok_or_else(reject)?;
        if install.children_snapshot != self.children
            || install.viewport_width != placement.viewport_width
            || install.viewport_height != placement.viewport_height
            || install.applied_origins != self.inline_ifc_apply_origins()
            || !install.build_inner_width.is_finite()
            || install.build_inner_width <= 0.0
            || !install.applied_inner_width.is_finite()
            || install.applied_inner_width <= 0.0
        {
            return Err(reject());
        }

        let collected = ElementInlineIfcMetadataCollector::collect(
            arena,
            ElementInlineIfcMetadataCollectorInput::new(
                root_key,
                install.build_inner_width,
                install.viewport_width,
                install.viewport_height,
            ),
        )
        .ok_or_else(reject)?;
        if install.cache_key != collected.root_source.cache_key() {
            return Err(reject());
        }
        let current_geometry = inline_ifc_root_geometry(
            self.inline_ifc_layout_call_site
                .cache
                .context_for(&install.cache_key),
            arena,
            &collected.sources_by_node,
            &collected.node_order,
            root_key,
        )
        .ok_or_else(reject)?;
        let mut current_atomic_witnesses = current_geometry
            .nodes
            .iter()
            .filter_map(|node| match &node.kind {
                InlineIfcRootNodeGeometryKind::Atomic { witness } => Some((node.node_key, witness)),
                _ => None,
            })
            .collect::<FxHashMap<_, _>>();
        let expected_nodes = collected
            .sources_by_node
            .keys()
            .copied()
            .filter(|&key| key != root_key)
            .collect::<FxHashSet<_>>();
        let mut plan_nodes = FxHashSet::default();
        for op in &install.plan {
            let node_key = op.node_key();
            if !plan_nodes.insert(node_key)
                || arena
                    .arena_local_dirty(node_key)
                    .intersects(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT))
            {
                return Err(reject());
            }
            let node = arena.get(node_key).ok_or_else(reject)?;
            if node
                .element
                .local_dirty_flags()
                .intersects(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT))
            {
                return Err(reject());
            }
            match op {
                InlineIfcNodeInstallOp::Span {
                    package,
                    paint_fragments,
                    ..
                } => {
                    let Some(span) = node.element.as_any().downcast_ref::<Element>() else {
                        return Err(reject());
                    };
                    if !span.is_fragmentable_inline_element()
                        || !span.inline_ifc_owned_by_root
                        || span.layout_dirty
                    {
                        return Err(reject());
                    }
                    let origin_x = install.applied_origins.0;
                    let origin_y = install.applied_origins.1 - install.content_top_offset;
                    let mut expected_packages = package
                        .as_ref()
                        .map(ElementInlineIfcRolloutPackages::from_inline_ifc_distributed)
                        .unwrap_or_default();
                    if let Some(package) = expected_packages.decoration_draw_rect.as_mut() {
                        for fragment in &mut package.fragments {
                            fragment.metadata.position[0] += origin_x;
                            fragment.metadata.position[1] += origin_y;
                        }
                    }
                    let expected_fragments = paint_fragments
                        .iter()
                        .map(|rect| Rect {
                            x: origin_x + rect.x,
                            y: origin_y + rect.y,
                            width: rect.width,
                            height: rect.height,
                        })
                        .collect::<Vec<_>>();
                    let expected_shell_bounds = bounding_rect(
                        &expected_fragments
                            .iter()
                            .map(|rect| crate::ui::Rect {
                                x: rect.x,
                                y: rect.y,
                                width: rect.width,
                                height: rect.height,
                            })
                            .collect::<Vec<_>>(),
                    );
                    let rect_bits_eq = |left: &Rect, right: &Rect| {
                        left.x.to_bits() == right.x.to_bits()
                            && left.y.to_bits() == right.y.to_bits()
                            && left.width.to_bits() == right.width.to_bits()
                            && left.height.to_bits() == right.height.to_bits()
                    };
                    let shell_matches = span.layout_state.layout_position.x.to_bits()
                        == expected_shell_bounds.x.to_bits()
                        && span.layout_state.layout_position.y.to_bits()
                            == expected_shell_bounds.y.to_bits()
                        && span.layout_state.layout_flow_position.x.to_bits()
                            == expected_shell_bounds.x.to_bits()
                        && span.layout_state.layout_flow_position.y.to_bits()
                            == expected_shell_bounds.y.to_bits()
                        && span.layout_state.layout_inner_position.x.to_bits()
                            == expected_shell_bounds.x.to_bits()
                        && span.layout_state.layout_inner_position.y.to_bits()
                            == expected_shell_bounds.y.to_bits()
                        && span.layout_state.layout_size.width.to_bits()
                            == expected_shell_bounds.width.to_bits()
                        && span.layout_state.layout_size.height.to_bits()
                            == expected_shell_bounds.height.to_bits()
                        && span.layout_state.layout_inner_size.width.to_bits()
                            == expected_shell_bounds.width.to_bits()
                        && span.layout_state.layout_inner_size.height.to_bits()
                            == expected_shell_bounds.height.to_bits()
                        && span.layout_state.should_render
                            == (expected_shell_bounds.width > 0.0
                                && expected_shell_bounds.height > 0.0);
                    if span.inline_ifc_rollout_packages != expected_packages
                        || span.inline_paint_fragments.len() != expected_fragments.len()
                        || !span
                            .inline_paint_fragments
                            .iter()
                            .zip(&expected_fragments)
                            .all(|(left, right)| rect_bits_eq(left, right))
                        || !shell_matches
                    {
                        return Err(reject());
                    }
                }
                InlineIfcNodeInstallOp::Text {
                    lines,
                    paint_input,
                    paint_bounds,
                    ..
                } => {
                    let Some(text) = node.element.as_any().downcast_ref::<Text>() else {
                        return Err(reject());
                    };
                    let origin_x = install.applied_origins.0;
                    let origin_y = install.applied_origins.1 - install.content_top_offset;
                    let absolute_lines = lines
                        .iter()
                        .cloned()
                        .map(|line| line.shifted(origin_x, origin_y))
                        .collect::<Vec<_>>();
                    let expected_paint_bounds = crate::ui::Rect {
                        x: origin_x + paint_bounds.x,
                        y: origin_y + paint_bounds.y,
                        width: paint_bounds.width,
                        height: paint_bounds.height,
                    };
                    let mut expected_shell_bounds = bounding_rect(
                        &absolute_lines
                            .iter()
                            .map(|line| line.rect)
                            .collect::<Vec<_>>(),
                    );
                    if (expected_shell_bounds.width <= 0.0 || expected_shell_bounds.height <= 0.0)
                        && paint_bounds.width > 0.0
                        && paint_bounds.height > 0.0
                    {
                        expected_shell_bounds = expected_paint_bounds;
                    }
                    if !text.matches_inline_ifc_owned_install(
                        &absolute_lines,
                        paint_input.as_ref(),
                        expected_paint_bounds,
                        expected_shell_bounds,
                    ) {
                        return Err(reject());
                    }
                }
                InlineIfcNodeInstallOp::Atomic { witness } => {
                    if !inline_ifc_atomic_subtree_layout_placement_clean(arena, node_key) {
                        return Err(reject());
                    }
                    let current_witness = current_atomic_witnesses
                        .remove(&node_key)
                        .ok_or_else(reject)?;
                    let current_measurement = node
                        .element
                        .inline_atomic_measurement_snapshot()
                        .ok_or_else(reject)?;
                    let current_vertical_align = node
                        .element
                        .inline_atomic_vertical_align()
                        .ok_or_else(reject)?;
                    let expected_placement = inline_ifc_atomic_layout_placement(
                        install.applied_origins.2,
                        install.applied_origins.3,
                        self.layout_state.layout_position.x
                            - self.layout_state.layout_flow_position.x,
                        self.layout_state.layout_position.y
                            - self.layout_state.layout_flow_position.y,
                        install.content_top_offset,
                        placement,
                        witness.aligned_rect,
                    );
                    let actual_placement = node.element.last_placement().ok_or_else(reject)?;
                    if !inline_ifc_atomic_witness_bits_eq(witness, current_witness)
                        || witness.stable_id != node.element.stable_id()
                        || current_vertical_align != witness.vertical_align
                        || !inline_ifc_atomic_measurement_bits_eq(
                            &witness.measurement,
                            &current_measurement,
                        )
                        || !layout_placement_bits_eq(actual_placement, expected_placement)
                    {
                        return Err(reject());
                    }
                }
            }
        }
        let installed_nodes = install
            .installed_nodes
            .iter()
            .copied()
            .collect::<FxHashSet<_>>();
        if !current_atomic_witnesses.is_empty()
            || plan_nodes != expected_nodes
            || installed_nodes != plan_nodes
            || install.installed_nodes.len() != installed_nodes.len()
        {
            return Err(reject());
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn damage_owning_inline_ifc_root_witness_for_test(
        &mut self,
        damage: OwningInlineIfcRootWitnessDamage,
    ) {
        match damage {
            OwningInlineIfcRootWitnessDamage::MissingCurrent => {
                self.inline_ifc_layout_call_site.current = None;
            }
            OwningInlineIfcRootWitnessDamage::Pending => {
                let install = self.inline_ifc_layout_call_site.current.as_ref().unwrap();
                self.inline_ifc_layout_call_site.pending = Some(ElementInlineIfcPendingPlan {
                    cache_key: install.cache_key.clone(),
                    children_snapshot: install.children_snapshot.clone(),
                    content_top_offset: install.content_top_offset,
                    content_size: install.content_size,
                    inner_width: install.build_inner_width,
                    plan: Vec::new(),
                });
            }
            OwningInlineIfcRootWitnessDamage::ChildrenSnapshot => {
                self.inline_ifc_layout_call_site
                    .current
                    .as_mut()
                    .unwrap()
                    .children_snapshot
                    .push(NodeKey::null());
            }
            OwningInlineIfcRootWitnessDamage::PlanMissing => {
                self.inline_ifc_layout_call_site
                    .current
                    .as_mut()
                    .unwrap()
                    .plan
                    .pop();
            }
            OwningInlineIfcRootWitnessDamage::PlanDuplicate => {
                let install = self.inline_ifc_layout_call_site.current.as_mut().unwrap();
                install.plan.push(install.plan[0].clone());
            }
            OwningInlineIfcRootWitnessDamage::InstalledMissing => {
                self.inline_ifc_layout_call_site
                    .current
                    .as_mut()
                    .unwrap()
                    .installed_nodes
                    .pop();
            }
            OwningInlineIfcRootWitnessDamage::InstalledDuplicate => {
                let install = self.inline_ifc_layout_call_site.current.as_mut().unwrap();
                install.installed_nodes.push(install.installed_nodes[0]);
            }
            OwningInlineIfcRootWitnessDamage::CacheKey => {
                self.inline_ifc_layout_call_site
                    .current
                    .as_mut()
                    .unwrap()
                    .build_inner_width += 13.0;
            }
            OwningInlineIfcRootWitnessDamage::WrongKind => {
                let install = self.inline_ifc_layout_call_site.current.as_mut().unwrap();
                let node_key = install.plan[0].node_key();
                install.plan[0] = InlineIfcNodeInstallOp::Span {
                    node_key,
                    package: None,
                    paint_fragments: Vec::new(),
                };
            }
            OwningInlineIfcRootWitnessDamage::TextPlanPayloadSwap => {
                let install = self.inline_ifc_layout_call_site.current.as_mut().unwrap();
                let text_indices = install
                    .plan
                    .iter()
                    .enumerate()
                    .filter_map(|(index, op)| {
                        matches!(op, InlineIfcNodeInstallOp::Text { .. }).then_some(index)
                    })
                    .collect::<Vec<_>>();
                assert!(text_indices.len() >= 2);
                let second = text_indices[1];
                let (left, right) = install.plan.split_at_mut(second);
                let InlineIfcNodeInstallOp::Text {
                    lines: left_lines,
                    paint_input: left_input,
                    paint_bounds: left_bounds,
                    ..
                } = &mut left[text_indices[0]]
                else {
                    unreachable!()
                };
                let InlineIfcNodeInstallOp::Text {
                    lines: right_lines,
                    paint_input: right_input,
                    paint_bounds: right_bounds,
                    ..
                } = &mut right[0]
                else {
                    unreachable!()
                };
                std::mem::swap(left_lines, right_lines);
                std::mem::swap(left_input, right_input);
                std::mem::swap(left_bounds, right_bounds);
            }
            OwningInlineIfcRootWitnessDamage::AtomicStableId => {
                let install = self.inline_ifc_layout_call_site.current.as_mut().unwrap();
                let InlineIfcNodeInstallOp::Atomic { witness } = install
                    .plan
                    .iter_mut()
                    .find(|op| matches!(op, InlineIfcNodeInstallOp::Atomic { .. }))
                    .unwrap()
                else {
                    unreachable!()
                };
                witness.stable_id = witness.stable_id.wrapping_add(1);
            }
            OwningInlineIfcRootWitnessDamage::AtomicSource => {
                let install = self.inline_ifc_layout_call_site.current.as_mut().unwrap();
                let InlineIfcNodeInstallOp::Atomic { witness } = install
                    .plan
                    .iter_mut()
                    .find(|op| matches!(op, InlineIfcNodeInstallOp::Atomic { .. }))
                    .unwrap()
                else {
                    unreachable!()
                };
                witness.source = InlineIfcSourceId(witness.source.0.wrapping_add(1));
            }
            OwningInlineIfcRootWitnessDamage::AtomicInlineBoxId => {
                let install = self.inline_ifc_layout_call_site.current.as_mut().unwrap();
                let InlineIfcNodeInstallOp::Atomic { witness } = install
                    .plan
                    .iter_mut()
                    .find(|op| matches!(op, InlineIfcNodeInstallOp::Atomic { .. }))
                    .unwrap()
                else {
                    unreachable!()
                };
                witness.inline_box_id = witness.inline_box_id.wrapping_add(1);
            }
            OwningInlineIfcRootWitnessDamage::AtomicInsertionByte => {
                let install = self.inline_ifc_layout_call_site.current.as_mut().unwrap();
                let InlineIfcNodeInstallOp::Atomic { witness } = install
                    .plan
                    .iter_mut()
                    .find(|op| matches!(op, InlineIfcNodeInstallOp::Atomic { .. }))
                    .unwrap()
                else {
                    unreachable!()
                };
                witness.insertion_byte = witness.insertion_byte.wrapping_add(1);
            }
            OwningInlineIfcRootWitnessDamage::AtomicLineIndex => {
                let install = self.inline_ifc_layout_call_site.current.as_mut().unwrap();
                let InlineIfcNodeInstallOp::Atomic { witness } = install
                    .plan
                    .iter_mut()
                    .find(|op| matches!(op, InlineIfcNodeInstallOp::Atomic { .. }))
                    .unwrap()
                else {
                    unreachable!()
                };
                witness.line_index = witness.line_index.wrapping_add(1);
            }
            OwningInlineIfcRootWitnessDamage::AtomicMeasurementMaxWidth => {
                let witness = self.atomic_install_witness_mut_for_test();
                witness.measurement.constraints.max_width = Some(1.25);
            }
            OwningInlineIfcRootWitnessDamage::AtomicMeasurementAvailableHeight => {
                let witness = self.atomic_install_witness_mut_for_test();
                witness.measurement.constraints.available_height = Some(2.5);
            }
            OwningInlineIfcRootWitnessDamage::AtomicMeasurementViewport => {
                let witness = self.atomic_install_witness_mut_for_test();
                witness.measurement.constraints.viewport = Some(InlineIfcSize::new(3.0, 4.0));
            }
            OwningInlineIfcRootWitnessDamage::AtomicMeasurementPercentBase => {
                let witness = self.atomic_install_witness_mut_for_test();
                witness.measurement.constraints.percent_base =
                    InlineIfcPercentBase::new(Some(5.0), Some(6.0));
            }
            OwningInlineIfcRootWitnessDamage::AtomicMeasurementSizing => {
                let witness = self.atomic_install_witness_mut_for_test();
                witness.measurement.constraints.sizing.min_width = Some(7.0);
            }
            OwningInlineIfcRootWitnessDamage::AtomicMeasurementSize => {
                let witness = self.atomic_install_witness_mut_for_test();
                witness.measurement.measured_size.width += 1.0;
            }
            OwningInlineIfcRootWitnessDamage::AtomicRawRect => {
                let witness = self.atomic_install_witness_mut_for_test();
                witness.raw_rect.x += 1.0;
            }
            OwningInlineIfcRootWitnessDamage::AtomicAlignedRect => {
                let witness = self.atomic_install_witness_mut_for_test();
                witness.aligned_rect.y += 1.0;
            }
            OwningInlineIfcRootWitnessDamage::AtomicVerticalAlign => {
                let witness = self.atomic_install_witness_mut_for_test();
                witness.vertical_align = match witness.vertical_align {
                    VerticalAlign::Baseline => VerticalAlign::Top,
                    _ => VerticalAlign::Baseline,
                };
            }
            OwningInlineIfcRootWitnessDamage::AtomicPackageZeroPlacements
            | OwningInlineIfcRootWitnessDamage::AtomicPackageDuplicatePlacements => {
                let install = self.inline_ifc_layout_call_site.current.as_ref().unwrap();
                let cache_key = install.cache_key.clone();
                let source = install
                    .plan
                    .iter()
                    .find_map(|op| match op {
                        InlineIfcNodeInstallOp::Atomic { witness } => Some(witness.source),
                        _ => None,
                    })
                    .unwrap();
                self.inline_ifc_layout_call_site
                    .cache
                    .damage_atomic_package_cardinality_for_test(
                        &cache_key,
                        source,
                        matches!(
                            damage,
                            OwningInlineIfcRootWitnessDamage::AtomicPackageDuplicatePlacements
                        ),
                    );
            }
            OwningInlineIfcRootWitnessDamage::LayoutDirty => {
                self.layout_dirty = true;
                self.dirty_flags = self.dirty_flags.union(DirtyPassMask::LAYOUT);
            }
            OwningInlineIfcRootWitnessDamage::PlacementDirty => {
                self.dirty_flags = self.dirty_flags.union(DirtyPassMask::PLACEMENT);
            }
        }
    }

    #[cfg(test)]
    fn atomic_install_witness_mut_for_test(&mut self) -> &mut InlineIfcAtomicInstallWitness {
        let install = self.inline_ifc_layout_call_site.current.as_mut().unwrap();
        let InlineIfcNodeInstallOp::Atomic { witness } = install
            .plan
            .iter_mut()
            .find(|op| matches!(op, InlineIfcNodeInstallOp::Atomic { .. }))
            .unwrap()
        else {
            unreachable!()
        };
        witness
    }

    /// Placement-skip buster for inline IFC roots: a root whose layout
    /// inputs changed must re-run place so the shaped candidate, installed
    /// packages, and descendant geometry stay fresh.
    fn inline_ifc_layout_call_site_dirty_gate(
        &self,
        arena: &NodeArena,
        placement: LayoutPlacement,
    ) -> bool {
        if !self.is_owning_inline_ifc_root_role() {
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
            .any(|&child_key| arena.subtree_dirty_intersects(child_key, self_dirty_mask))
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
        if !self.is_owning_inline_ifc_root_role() {
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

        // A same-constraints measure pass can legitimately skip while a
        // descendant has paint-only damage.  Place is still woken by
        // `inline_ifc_layout_call_site_dirty_gate`; do not turn that wake-up
        // into a pure-move reuse of paint packages captured before the style
        // change.  The subtree cache was refreshed immediately before place.
        let paint_clean = !self.dirty_flags.intersects(DirtyPassMask::PAINT)
            && self
                .children
                .iter()
                .all(|&child_key| !arena.subtree_dirty_intersects(child_key, DirtyPassMask::PAINT));

        // 1. Measure stashed a fresh plan this frame (content/size changed):
        //    consume it.
        // 2. No pending and we already have an install with the same
        //    children: a pure move — reuse the cached plan, only origins
        //    changed. This is what makes dragging a window cheap.
        // 3. Otherwise (first install, or structural/paint change without a
        //    measure): shape from scratch.
        let (cache_key, children_snapshot, top_offset, content_size, plan_inner_width, plan) =
            if let Some(pending) = self.inline_ifc_layout_call_site.pending.take() {
                (
                    pending.cache_key,
                    pending.children_snapshot,
                    pending.content_top_offset,
                    pending.content_size,
                    pending.inner_width,
                    pending.plan,
                )
            } else if let Some(mut install) = self.inline_ifc_layout_call_site.current.take() {
                let viewport_unchanged = install.viewport_width == placement.viewport_width
                    && install.viewport_height == placement.viewport_height;
                let applied_width_unchanged =
                    install.applied_inner_width.to_bits() == inner_width.to_bits();
                if install.children_snapshot == self.children
                    && viewport_unchanged
                    && applied_width_unchanged
                    && paint_clean
                {
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
                // A paint-only wake-up must rebuild packages against the
                // exact width that produced the live shaping/cache key.
                // Auto roots can settle to a different final place-time
                // width; adopting that width here would silently re-shape
                // and then make each paint-only frame the next authority.
                let rebuild_inner_width = if install.children_snapshot == self.children
                    && viewport_unchanged
                    && applied_width_unchanged
                {
                    install.build_inner_width
                } else {
                    inner_width
                };
                // Children or paint inputs changed without a measure pass:
                // fall through to a full reshape, clearing the stale install
                // first.
                for node_key in install.installed_nodes {
                    clear_inline_ifc_node_install(arena, node_key);
                }
                match self.compute_inline_ifc_plan(
                    arena,
                    rebuild_inner_width,
                    placement.viewport_width,
                    placement.viewport_height,
                ) {
                    Some((cache_key, children, top_offset, content_size, plan)) => (
                        cache_key,
                        children,
                        top_offset,
                        content_size,
                        rebuild_inner_width,
                        plan,
                    ),
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
                    Some((cache_key, children, top_offset, content_size, plan)) => (
                        cache_key,
                        children,
                        top_offset,
                        content_size,
                        inner_width,
                        plan,
                    ),
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
            build_inner_width: plan_inner_width,
            applied_inner_width: inner_width,
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
            &collected.node_order,
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
                InlineIfcNodeInstallOp::Atomic { witness } => {
                    let child_placement = inline_ifc_atomic_layout_placement(
                        flow_origin_x,
                        flow_origin_y,
                        visual_offset_x,
                        visual_offset_y,
                        top_offset,
                        placement,
                        witness.aligned_rect,
                    );
                    arena.with_element_taken(witness.node_key, |child, arena| {
                        child.set_layout_offset(0.0, 0.0);
                        child.place(child_placement, arena);
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
                InlineIfcNodeInstallOp::Atomic { witness } => {
                    let child_placement = inline_ifc_atomic_layout_placement(
                        flow_origin_x,
                        flow_origin_y,
                        visual_offset_x,
                        visual_offset_y,
                        top_offset,
                        placement,
                        witness.aligned_rect,
                    );
                    arena.with_element_taken(witness.node_key, |child, arena| {
                        // Bake the line placement into the parent origin:
                        // not every atomic host honours set_layout_offset
                        // (TextAreaTextRun places at parent + visual only).
                        child.set_layout_offset(0.0, 0.0);
                        child.place(child_placement, arena);
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
        if !self.is_owning_inline_ifc_root_role() {
            self.inline_ifc_layout_call_site.pending = None;
            return None;
        }
        let paint_dirty = self.dirty_flags.intersects(DirtyPassMask::PAINT)
            || self
                .children
                .iter()
                .any(|&child_key| arena.subtree_dirty_intersects(child_key, DirtyPassMask::PAINT));
        // Cheapest path: the install is still valid (same children, same
        // width, and no descendant changed content/size) — the IFC shaping
        // is unchanged, so skip collect + cache hashing entirely. This is
        // what makes re-measuring an inline root during an ancestor move
        // (where the subtree is layout-clean) effectively free.
        if let Some(install) = self.inline_ifc_layout_call_site.current.as_ref() {
            let width_unchanged = (install.build_inner_width - inner_width).abs() <= f32::EPSILON;
            let viewport_unchanged = install.viewport_width == viewport_width
                && install.viewport_height == viewport_height;
            let ifc_dirty_mask = DirtyPassMask::LAYOUT.union(DirtyPassMask::PAINT);
            let layout_clean = !self.dirty_flags.intersects(ifc_dirty_mask)
                && !self
                    .children
                    .iter()
                    .any(|&child_key| arena.subtree_dirty_intersects(child_key, ifc_dirty_mask));
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
                && !paint_dirty
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
            &collected.node_order,
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
            inner_width,
            plan,
        });
        Some(content_size)
    }

    /// Test diagnostic for the retained shaping-width authority.
    #[cfg(test)]
    pub(crate) fn inline_ifc_root_build_width_for_test(&self) -> Option<f32> {
        self.inline_ifc_layout_call_site
            .current
            .as_ref()
            .map(|install| install.build_inner_width)
    }

    #[cfg(test)]
    pub(crate) fn inline_ifc_root_applied_width_for_test(&self) -> Option<f32> {
        self.inline_ifc_layout_call_site
            .current
            .as_ref()
            .map(|install| install.applied_inner_width)
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

    /// Explicit role predicate shared by the owning IFC layout pipeline and
    /// paint preflight. Fixed-size inline roots own the same distributed
    /// install as auto-sized roots; only descendants already owned by an
    /// ancestor IFC remain on the M7B span path.
    fn is_owning_inline_ifc_root_role(&self) -> bool {
        self.computed_style.layout == Layout::Inline
            && !self.inline_ifc_owned_by_root
            && !self.children.is_empty()
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

    pub(crate) fn union_promotion_bounds(
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
        paint_offset: [f32; 2],
        require_exact: bool,
    ) -> Option<PromotionCompositeBounds> {
        let child_paint_offset = self.paint_offset_after_own_snap(paint_offset)?;
        let mut bounds = self.untransformed_paint_bounds();
        if !Self::is_canonical_transform_surface_bounds(bounds) {
            return None;
        }
        for child_key in &self.children {
            let child_node = arena.get(*child_key)?;
            let child_bounds = if require_exact {
                child_node
                    .element
                    .retained_transform_output_bounds(arena, child_paint_offset)?
            } else {
                child_node
                    .element
                    .legacy_transform_output_bounds(arena, child_paint_offset)?
            };
            if !Self::is_canonical_transform_surface_bounds(child_bounds) {
                return None;
            }
            bounds = Self::checked_union_transform_surface_bounds(bounds, child_bounds)?;
        }
        Some(bounds)
    }

    pub(crate) fn legacy_transform_surface_bounds(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        paint_offset: [f32; 2],
    ) -> Option<PromotionCompositeBounds> {
        self.resolved_transform
            .and_then(|_| self.transform_subtree_raster_bounds(arena, paint_offset, false))
    }

    pub(crate) fn retained_transform_render_output_bounds(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        paint_offset: [f32; 2],
    ) -> Option<PromotionCompositeBounds> {
        if self.resolved_transform.is_some() {
            self.exact_transform_surface_geometry_snapshot(arena, paint_offset, None)?
                .quad_aabb()
        } else {
            self.transform_subtree_raster_bounds(arena, paint_offset, true)
        }
    }

    /// Exact retained output owned by a nested isolation boundary.
    ///
    /// The caller supplies the paint offset after the direct parent applied
    /// its own paint snap. This deliberately reuses only the retained
    /// transform-output contract: the caller/planner never independently
    /// recomputes raw layout bounds, and this path never uses the legacy
    /// unknown-host fallback.
    pub(crate) fn exact_nested_isolation_render_output_bounds(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        parent_snapped_paint_offset: [f32; 2],
    ) -> Option<PromotionCompositeBounds> {
        if self.resolved_transform.is_some() {
            return None;
        }
        let mut seen = FxHashSet::default();
        let mut stack = self.children.iter().copied().collect::<Vec<_>>();
        while let Some(key) = stack.pop() {
            if !seen.insert(key) {
                return None;
            }
            let node = arena.get(key)?;
            if node.element.has_retained_transform_surface() {
                return None;
            }
            stack.extend(node.element.children().iter().copied());
        }
        let bounds =
            self.retained_transform_render_output_bounds(arena, parent_snapped_paint_offset)?;
        (Self::is_canonical_transform_surface_bounds(bounds)
            && bounds.x >= 0.0
            && bounds.y >= 0.0
            && bounds.corner_radii.map(f32::to_bits) == [0.0_f32.to_bits(); 4])
            .then_some(bounds)
    }

    pub(crate) fn legacy_transform_render_output_bounds(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        paint_offset: [f32; 2],
    ) -> Option<PromotionCompositeBounds> {
        if self.resolved_transform.is_some() {
            self.transform_surface_geometry_snapshot(arena, paint_offset, None)?
                .quad_aabb()
        } else {
            self.transform_subtree_raster_bounds(arena, paint_offset, false)
        }
    }

    fn paint_offset_after_own_snap(&self, parent: [f32; 2]) -> Option<[f32; 2]> {
        let position = self.layout_state.layout_position;
        if parent.iter().any(|value| !value.is_finite())
            || !position.x.is_finite()
            || !position.y.is_finite()
        {
            return None;
        }
        let paint_x = position.x + parent[0];
        let paint_y = position.y + parent[1];
        if !paint_x.is_finite() || !paint_y.is_finite() {
            return None;
        }
        let next = [
            parent[0] + round_layout_value(paint_x) - paint_x,
            parent[1] + round_layout_value(paint_y) - paint_y,
        ];
        next.iter().all(|value| value.is_finite()).then_some(next)
    }

    pub(crate) fn retained_child_paint_offset(&self, parent: [f32; 2]) -> Option<[f32; 2]> {
        self.paint_offset_after_own_snap(parent)
    }

    fn is_canonical_transform_surface_bounds(bounds: PromotionCompositeBounds) -> bool {
        bounds.x.is_finite()
            && bounds.y.is_finite()
            && bounds.width.is_finite()
            && bounds.height.is_finite()
            && bounds.width > 0.0
            && bounds.height > 0.0
    }

    pub(crate) fn checked_union_transform_surface_bounds(
        current: PromotionCompositeBounds,
        next: PromotionCompositeBounds,
    ) -> Option<PromotionCompositeBounds> {
        if !Self::is_canonical_transform_surface_bounds(current)
            || !Self::is_canonical_transform_surface_bounds(next)
        {
            return None;
        }
        let current_max_x = current.x + current.width;
        let current_max_y = current.y + current.height;
        let next_max_x = next.x + next.width;
        let next_max_y = next.y + next.height;
        if [current_max_x, current_max_y, next_max_x, next_max_y]
            .iter()
            .any(|value| !value.is_finite())
        {
            return None;
        }
        let min_x = current.x.min(next.x);
        let min_y = current.y.min(next.y);
        let max_x = current_max_x.max(next_max_x);
        let max_y = current_max_y.max(next_max_y);
        let union = PromotionCompositeBounds {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
            corner_radii: [0.0; 4],
        };
        Self::is_canonical_transform_surface_bounds(union).then_some(union)
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

    fn tick_post_layout_animation_frame(&mut self, now: crate::time::Instant) -> DirtyFlags {
        if self.tick_scrollbar_visibility(now) {
            DirtyFlags::PAINT
        } else {
            DirtyFlags::NONE
        }
    }

    fn scroll_geometry_observation(
        &self,
        owner: crate::view::node_arena::NodeKey,
        arena: &crate::view::node_arena::NodeArena,
    ) -> ScrollGeometryObservation {
        let axis = match self.scroll_direction {
            ScrollDirection::None => return ScrollGeometryObservation::Inactive,
            ScrollDirection::Vertical => ScrollAxisSnapshot::Vertical,
            ScrollDirection::Horizontal => ScrollAxisSnapshot::Horizontal,
            ScrollDirection::Both => ScrollAxisSnapshot::Both,
        };
        let geometry_dirty = DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT);
        if self.dirty_flags.intersects(geometry_dirty)
            || arena.subtree_dirty_intersects(owner, geometry_dirty)
            || self.has_active_layout_transition()
            || !arena
                .get(owner)
                .is_some_and(|node| node.children() == self.children.as_slice())
        {
            return ScrollGeometryObservation::Unsupported;
        }

        let outer_radii = normalize_corner_radii(
            self.border_radii,
            self.layout_state.layout_size.width.max(0.0),
            self.layout_state.layout_size.height.max(0.0),
        );
        let inner_radii = self.inner_clip_radii(outer_radii);
        if inner_radii.has_any_rounding() {
            return ScrollGeometryObservation::Unsupported;
        }
        if self.children.is_empty() {
            return ScrollGeometryObservation::Inactive;
        }
        let overflow_child_indices: Vec<bool> = (0..self.children.len())
            .map(|idx| self.child_renders_outside_inner_clip(idx, arena))
            .collect();
        if !self.should_clip_children(&overflow_child_indices, inner_radii, arena) {
            return ScrollGeometryObservation::Inactive;
        }

        let scrollport_rect = self.inner_clip_rect();
        let Some(logical_scissor) = self.inner_clip_scissor_rect() else {
            return ScrollGeometryObservation::Unsupported;
        };
        let normalize_extent = |raw: f32, scrollport: f32| {
            if raw.is_finite() && raw >= 0.0 && scrollport.is_finite() && scrollport >= 0.0 {
                raw.max(scrollport)
            } else {
                raw
            }
        };
        let content_size = [
            normalize_extent(self.layout_state.content_size.width, scrollport_rect.width),
            normalize_extent(
                self.layout_state.content_size.height,
                scrollport_rect.height,
            ),
        ];
        let layout_content_bounds_at_zero = Rect {
            x: scrollport_rect.x,
            y: scrollport_rect.y,
            width: content_size[0],
            height: content_size[1],
        };

        let geometry = self.scrollbar_geometry(scrollport_rect.x, scrollport_rect.y);
        let dragging_axis = self.scrollbar_drag.map(|drag| match drag.axis {
            ScrollbarAxis::Horizontal => ScrollAxisSnapshot::Horizontal,
            ScrollbarAxis::Vertical => ScrollAxisSnapshot::Vertical,
        });
        let interaction = ScrollbarInteractionWitness {
            hovered: self.is_hovered,
            dragging_axis,
            has_interaction_timestamp: self.last_scrollbar_interaction.is_some(),
        };
        let has_geometry = geometry.vertical_track.is_some() || geometry.horizontal_track.is_some();
        let sampled_alpha = self.scrollbar_visibility_alpha();
        let paint_state = if !has_geometry {
            ScrollbarPaintStateWitness::NotPaintable
        } else if sampled_alpha.to_bits() == 1.0_f32.to_bits() {
            ScrollbarPaintStateWitness::OpaqueNow
        } else if sampled_alpha.to_bits() == 0.0_f32.to_bits() {
            ScrollbarPaintStateWitness::HiddenNow
        } else {
            ScrollbarPaintStateWitness::TranslucentNow
        };

        ScrollGeometryObservation::Exact(ScrollGeometrySnapshot {
            configured_axis: axis,
            offset: [self.scroll_offset.x, self.scroll_offset.y],
            scrollport_rect,
            content_size,
            layout_content_bounds_at_zero,
            contents_clip: ScrollContentsClipWitness::ExactRect(logical_scissor),
            scrollbar_overlay: ScrollbarOverlayWitness {
                vertical_track: geometry.vertical_track,
                vertical_thumb: geometry.vertical_thumb,
                horizontal_track: geometry.horizontal_track,
                horizontal_thumb: geometry.horizontal_thumb,
                interaction,
                paint_state,
                sampled_alpha,
                shadow_blur_radius: self.scrollbar_shadow_blur_radius,
            },
        })
    }

    #[allow(private_interfaces)]
    fn shadow_paint_recording_capability(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        deferred_phase_root: bool,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> ShadowPaintRecordingCapability {
        if !self.layout_state.should_render {
            let blocker = if self.resolved_transform.is_some()
                && !recording_context.authorizes_transform_surface_root(self.stable_id())
            {
                Some(ShadowPaintBlocker::Transform)
            } else if self.should_append_to_root_viewport_render() || deferred_phase_root {
                Some(ShadowPaintBlocker::Deferred)
            } else if self.scroll_direction != ScrollDirection::None {
                Some(ShadowPaintBlocker::ScrollContainer)
            } else if self.opacity.to_bits() != 1.0_f32.to_bits()
                || !matches!(
                    recording_context.opacity_authority,
                    crate::view::paint::PaintOpacityAuthority::Baked
                )
            {
                Some(ShadowPaintBlocker::StatefulPaint)
            } else if self.has_active_layout_transition() {
                Some(ShadowPaintBlocker::LayoutTransition)
            } else {
                None
            };
            return blocker.map_or(
                ShadowPaintRecordingCapability::CulledSubtree,
                ShadowPaintRecordingCapability::Legacy,
            );
        }
        if self.inline_ifc_owned_by_root {
            return match self.inline_ifc_owned_shadow_paint_blocker(
                arena,
                deferred_phase_root,
                recording_context,
            ) {
                Some(blocker) => ShadowPaintRecordingCapability::Legacy(blocker),
                None => ShadowPaintRecordingCapability::Recordable,
            };
        }
        match self.shadow_paint_blocker(
            arena,
            deferred_phase_root,
            recording_context.authorizes_self_clip_for(self.stable_id()),
            true,
            recording_context,
        ) {
            Some(blocker) => ShadowPaintRecordingCapability::Legacy(blocker),
            None => ShadowPaintRecordingCapability::Recordable,
        }
    }

    #[allow(private_interfaces)]
    fn record_shadow_paint_metadata(
        &self,
        owner: crate::view::node_arena::NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        arena: &crate::view::node_arena::NodeArena,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<crate::view::paint::PaintChunkMetadata> {
        if self.inline_ifc_owned_by_root {
            let payload = self
                .prepared_inline_ifc_decoration_payload(recording_context)
                .ok()?;
            return Some(crate::view::paint::PaintChunkMetadata {
                id: crate::view::paint::PaintChunkId {
                    owner,
                    scope: crate::view::paint::PaintPropertyScope::SelfPaint,
                    phase: crate::view::paint::PaintNodePhase::BeforeChildren,
                    slot: 0,
                    role: crate::view::paint::PaintChunkRole::SelfDecoration,
                },
                owner,
                bounds: payload.bounds,
                properties,
                content_revision,
                payload_identity: crate::view::paint::PaintPayloadIdentity::inline_ifc_decorations(
                    payload.ops.iter(),
                ),
            });
        }
        self.record_shadow_node_paint_metadata(
            owner,
            properties,
            content_revision,
            Some(arena),
            recording_context,
        )
        .ok()
    }

    #[allow(private_interfaces)]
    fn record_shadow_paint_artifact(
        &self,
        owner: crate::view::node_arena::NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        arena: &crate::view::node_arena::NodeArena,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<crate::view::paint::PaintArtifact> {
        if self.inline_ifc_owned_by_root {
            let payload = self
                .prepared_inline_ifc_decoration_payload(recording_context)
                .ok()?;
            #[cfg(test)]
            crate::view::paint::note_full_artifact_record();
            let payload_identity = crate::view::paint::PaintPayloadIdentity::inline_ifc_decorations(
                payload.ops.iter(),
            );
            let ops = payload
                .ops
                .into_iter()
                .map(crate::view::paint::PaintOp::PreparedInlineIfcDecoration)
                .collect::<Vec<_>>();
            return Some(crate::view::paint::PaintArtifact {
                target: Default::default(),
                chunks: vec![crate::view::paint::PaintChunk {
                    id: crate::view::paint::PaintChunkId {
                        owner,
                        scope: crate::view::paint::PaintPropertyScope::SelfPaint,
                        phase: crate::view::paint::PaintNodePhase::BeforeChildren,
                        slot: 0,
                        role: crate::view::paint::PaintChunkRole::SelfDecoration,
                    },
                    owner,
                    op_range: 0..ops.len(),
                    bounds: payload.bounds,
                    properties,
                    content_revision,
                    payload_identity,
                }],
                ops,
                clip_nodes: Vec::new(),
                effect_nodes: Vec::new(),
                owner_nodes: vec![crate::view::paint::PaintOwnerSnapshot {
                    owner,
                    parent: None,
                }],
            });
        }
        let artifact = self
            .record_shadow_node_paint_artifact(
                owner,
                properties,
                content_revision,
                arena,
                recording_context,
            )
            .ok()?;
        #[cfg(test)]
        crate::view::paint::note_full_artifact_record();
        Some(artifact)
    }

    #[allow(private_interfaces)]
    fn shadow_paint_recording_context(
        &self,
        mut parent: crate::view::paint::PaintRecordingContext,
    ) -> crate::view::paint::PaintRecordingContext {
        let paint_x = self.layout_state.layout_position.x + parent.paint_offset[0];
        let paint_y = self.layout_state.layout_position.y + parent.paint_offset[1];
        parent.paint_offset[0] += round_layout_value(paint_x) - paint_x;
        parent.paint_offset[1] += round_layout_value(paint_y) - paint_y;
        parent
    }

    fn placement_eligibility_metadata(
        &self,
    ) -> crate::view::node_arena::PlacementEligibilityMetadata {
        self.local_placement_eligibility_metadata()
    }

    fn last_placement(&self) -> Option<LayoutPlacement> {
        self.last_layout_placement
    }

    #[allow(private_interfaces)]
    fn inline_atomic_measurement_snapshot(&self) -> Option<InlineIfcMeasuredAtomicBox> {
        self.inline_atomic_measurement_snapshot_with_intrinsic(None)
    }

    fn inline_atomic_vertical_align(&self) -> Option<VerticalAlign> {
        Some(self.computed_style.vertical_align)
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
        let paint_width = self.layout_state.layout_size.width.max(0.0);
        let paint_height = self.layout_state.layout_size.height.max(0.0);
        hash_resolved_gradient_paint(
            &mut hasher,
            0xB1,
            self.computed_style.background_image.as_ref(),
            paint_width,
            paint_height,
        );
        hash_resolved_gradient_paint(
            &mut hasher,
            0xB2,
            self.computed_style.border_image.as_ref(),
            paint_width,
            paint_height,
        );
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
            for channel in shadow.color.to_rgba_f32() {
                hash_f32(&mut hasher, channel);
            }
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

    fn promotion_signature_is_complete(&self) -> bool {
        true
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

    fn retained_transform_surface_bounds(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        paint_offset: [f32; 2],
    ) -> Option<PromotionCompositeBounds> {
        self.resolved_transform
            .and_then(|_| self.transform_subtree_raster_bounds(arena, paint_offset, true))
    }

    fn retained_transform_output_bounds(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        paint_offset: [f32; 2],
    ) -> Option<PromotionCompositeBounds> {
        self.retained_transform_render_output_bounds(arena, paint_offset)
    }

    fn legacy_transform_output_bounds(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        paint_offset: [f32; 2],
    ) -> Option<PromotionCompositeBounds> {
        self.legacy_transform_render_output_bounds(arena, paint_offset)
    }

    fn retained_transform_raster_seed_bounds(&self) -> Option<PromotionCompositeBounds> {
        Some(self.untransformed_paint_bounds())
    }

    fn has_retained_transform_surface(&self) -> bool {
        self.resolved_transform.is_some()
    }

    fn compositor_viewport_transform_snapshot(&self) -> Option<ViewportTransformSnapshot> {
        self.resolved_transform
            .map(ViewportTransformSnapshot::from_matrix)
    }

    fn is_deferred_to_root_viewport_render(&self) -> bool {
        self.should_append_to_root_viewport_render()
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
