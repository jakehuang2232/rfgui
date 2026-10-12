use std::ops::Range;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::view::ImageSampling;
use crate::view::base_component::{Rect, ScrollbarOverlayWitness, ScrollbarPaintStateWitness};
use crate::view::compositor::property_tree::PropertyTreeState;
use crate::view::compositor::property_tree::{
    ClipBehavior, ClipNodeId, ClipNodeRole, ClipNodeSnapshot, EffectNodeSnapshot,
    LayoutPositionNodeId, LayoutPositionNodeSnapshot, ScrollNodeSnapshot, TransformNodeSnapshot,
    VisualOffsetNodeSnapshot,
};
use crate::view::node_arena::NodeKey;
use crate::view::render_pass::draw_rect_pass::{
    GradientKindGpu, GradientPaint, RectPassParams, RectRenderMode,
};
use crate::view::render_pass::shadow_module::{ShadowParams, ShadowShape};
use crate::view::render_pass::text_pass::{
    TextPassPreparedFragment, TextPassPreparedParams, TextPassPreparedStagingGlyphInput,
};
use crate::view::render_pass::texture_composite_pass::TextureCompositeParams;
use crate::view::sampled_texture::{
    SampledTextureAlphaMode, SampledTextureId, SampledTextureUpload, SvgRasterAssetId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintDeferredViewportSelfClipWitness {
    target_owner: NodeKey,
    stable_id: u64,
    clip: ClipNodeSnapshot,
}

impl PaintDeferredViewportSelfClipWitness {
    pub(crate) fn new(
        target_owner: NodeKey,
        stable_id: u64,
        clip: ClipNodeSnapshot,
        geometry: crate::view::compositor::property_tree::ClipGeometry,
    ) -> Option<Self> {
        (stable_id != 0
            && clip.id.owner == target_owner
            && clip.id.role == ClipNodeRole::SelfClip
            && clip.owner == target_owner
            && clip.parent.is_none()
            && clip.geometry == geometry
            && clip.behavior == ClipBehavior::Replace
            && clip.generation != 0)
            .then_some(Self {
                target_owner,
                stable_id,
                clip,
            })
    }

    pub(super) fn is_canonical_for(
        self,
        owner: NodeKey,
        stable_id: u64,
        authoritative_self_clip: Option<ClipNodeId>,
    ) -> bool {
        self.target_owner == owner
            && self.stable_id == stable_id
            && authoritative_self_clip == Some(self.clip.id)
            && self.clip.id.owner == owner
            && self.clip.owner == owner
            && self.clip.id.role == ClipNodeRole::SelfClip
            && self.clip.parent.is_none()
            && self.clip.behavior == ClipBehavior::Replace
            && self.clip.generation != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintTextPreeditWitness {
    pub(crate) projection_owner: NodeKey,
    pub(crate) target_owner: NodeKey,
    pub(crate) target_stable_id: u64,
    pub(crate) local_start_char: usize,
    pub(crate) local_end_char: usize,
    pub(crate) target_start_byte: usize,
    pub(crate) target_end_byte: usize,
    pub(crate) target_caret_byte: usize,
    pub(crate) target_caret_char: usize,
}

impl PaintTextPreeditWitness {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaintTextSelectionWitness {
    pub(crate) target_owner: NodeKey,
    pub(crate) target_stable_id: u64,
    pub(crate) local_start: usize,
    pub(crate) local_end: usize,
    pub(crate) fill: [f32; 4],
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PaintArtifact {
    pub(crate) chunks: super::shared_sequence::SharedSequence<PaintChunk>,
    pub(crate) ops: super::shared_sequence::SharedSequence<PaintOp>,
    /// Complete, arena-independent transitive clip snapshot for every clip
    /// leaf referenced by `chunks` or `owner_property_states`.
    pub(crate) clip_nodes: Vec<ClipNodeSnapshot>,
    /// Complete, arena-independent transitive effect snapshot for every
    /// effect leaf referenced by `chunks` or `owner_property_states`. M6B
    /// validates this store but keeps the existing per-op baked opacity as
    /// visual authority.
    pub(crate) effect_nodes: Vec<EffectNodeSnapshot>,
    /// Complete arena-independent transform graph referenced by `chunks` or
    /// `owner_property_states`, including canonical derived owner projection.
    pub(crate) transform_nodes: Vec<TransformNodeSnapshot>,
    /// Complete transitive layout-position graph referenced by `chunks` or
    /// `owner_property_states`.
    pub(crate) layout_position_nodes: Vec<LayoutPositionNodeSnapshot>,
    /// Complete transitive owner-local visual-offset graph referenced by
    /// `chunks` or `owner_property_states`; parent links preserve ancestor
    /// animation composition.
    pub(crate) visual_offset_nodes: Vec<VisualOffsetNodeSnapshot>,
    /// Complete transitive scroll graph referenced by `chunks` or
    /// `owner_property_states`.
    pub(crate) scroll_nodes: Vec<ScrollNodeSnapshot>,
    /// Canonical frame-traversal ownership topology for every chunk owner and
    /// its transitive ancestors. Roots are explicitly parentless even if the
    /// same arena node has an out-of-scope parent.
    pub(crate) owner_nodes: Vec<PaintOwnerSnapshot>,
    /// Projected paint/descendants property endpoints for exactly the owners
    /// in `owner_nodes`. The producer derives both stores from the same
    /// chunk-owner ancestor closure; this exact key-set rule does not prove
    /// completeness for owners outside the artifact scene.
    pub(crate) owner_property_states: Vec<PaintOwnerPropertyStateSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintOwnerSnapshot {
    pub(crate) owner: NodeKey,
    pub(crate) parent: Option<NodeKey>,
    pub(crate) snap: PaintOwnerSnap,
}

/// Where an owner meets the pixel grid. Recording never snaps: the compiler
/// applies each owner's snap along the owner chain, so an owner paints at its
/// parent's snapped paint offset plus its own snap corrections.
///
/// Each point, in order, snaps the paint offset reaching it. Points are
/// relative to the origin of `frame`: the layout frame the owner's paint
/// state names (its own, or its nearest framed ancestor's), so they move
/// with the owner, or the viewport without one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintOwnerSnap {
    pub(crate) frame: Option<LayoutPositionNodeId>,
    pub(crate) points_bits: [Option<[u32; 2]>; 2],
}

impl PaintOwnerSnap {
    /// Paints at its parent's paint offset.
    pub(crate) const INHERIT: Self = Self {
        frame: None,
        points_bits: [None, None],
    };

    /// The snap of `host`, relative to the layout frame `frame`. A point that
    /// does not return exactly to itself from that frame is kept in viewport
    /// space instead, so the compiler snaps the very value layout produced.
    pub(crate) fn observe<H: crate::view::base_component::ElementTrait + ?Sized>(
        host: &H,
        arena: &crate::view::node_arena::NodeArena,
        frame: Option<LayoutPositionNodeId>,
    ) -> Self {
        let points = host.paint_snap_points(arena);
        if points == [None, None] {
            return Self::INHERIT;
        }
        let viewport = Self {
            frame: None,
            points_bits: points.map(|point| point.map(|point| point.map(f32::to_bits))),
        };
        let Some((frame, frame_node)) = frame.and_then(|frame| Some((frame, arena.get(frame.0)?)))
        else {
            return viewport;
        };
        let bounds = frame_node.element.box_model_snapshot();
        let origin = [bounds.x, bounds.y];
        let mut points_bits = [None; 2];
        for (relative, point) in points_bits.iter_mut().zip(points) {
            let Some(point) = point else {
                continue;
            };
            let local = [point[0] - origin[0], point[1] - origin[1]];
            if [origin[0] + local[0], origin[1] + local[1]].map(f32::to_bits)
                != point.map(f32::to_bits)
            {
                return viewport;
            }
            *relative = Some(local.map(f32::to_bits));
        }
        Self {
            frame: Some(frame),
            points_bits,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintOwnerPropertyStateSnapshot {
    pub(crate) owner: NodeKey,
    /// Persistent identity for this owner across frames. This lives beside
    /// the owner's property endpoints rather than on [`PaintOwnerSnapshot`]
    /// so the lightweight topology store does not become a second identity
    /// authority. A retained surface key still combines this value with its
    /// surface role; `stable_id` alone is not a complete persistent key.
    pub(crate) stable_id: u64,
    pub(crate) paint: PropertyTreeState,
    pub(crate) descendants: PropertyTreeState,
}

/// The frame a chunk's geometry — bounds, payload identity and commands — is
/// expressed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaintChunkFrame {
    /// Relative to the origin of the layout frame named by the chunk's
    /// `properties.layout_position`: the owner's own frame, or the nearest
    /// ancestor's for a host without one. Translating that frame leaves the
    /// chunk unchanged; the compiler places it at the frame's derived origin.
    Layout,
    /// Viewport space: hosts without a layout frame, and every chunk once the
    /// compiler has placed it.
    Viewport,
}

#[derive(Clone, Debug)]
pub(crate) struct PaintChunk {
    pub(crate) id: PaintChunkId,
    pub(crate) owner: NodeKey,
    pub(crate) op_range: Range<usize>,
    pub(crate) frame: PaintChunkFrame,
    pub(crate) bounds: Rect,
    /// Observational identity only in this slice. Property trees do not drive
    /// rendering until transform/clip/effect coverage is complete.
    pub(crate) properties: PropertyTreeState,
    /// Everything currently baked into this chunk's raster content. Opacity
    /// remains baked in Phase 4, so composite/topology revisions are part of
    /// the content key even though the eventual compositor will split them.
    pub(crate) content_revision: PaintContentRevision,
    pub(crate) payload_identity: PaintPayloadIdentity,
}

/// Artifact-space identity for one raster-bearing chunk.
///
/// This deliberately carries no retained-policy or component grammar. Durable
/// consumers use it to freeze the generic chunk facts that must survive
/// preparation without retaining the complete artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PaintChunkRasterIdentity {
    pub(crate) id: PaintChunkId,
    pub(crate) owner: NodeKey,
    pub(crate) bounds_bits: [u32; 4],
    pub(crate) payload_identity: PaintPayloadIdentity,
}

#[derive(Clone, Debug)]
pub(crate) struct PaintChunkMetadata {
    pub(crate) id: PaintChunkId,
    pub(crate) owner: NodeKey,
    pub(crate) frame: PaintChunkFrame,
    pub(crate) bounds: Rect,
    pub(crate) properties: PropertyTreeState,
    pub(crate) content_revision: PaintContentRevision,
    pub(crate) payload_identity: PaintPayloadIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PaintContentRevision {
    pub(crate) self_paint_revision: u64,
    pub(crate) composite_revision: u64,
    pub(crate) topology_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PaintChunkId {
    pub(crate) owner: NodeKey,
    pub(crate) scope: PaintPropertyScope,
    pub(crate) phase: PaintNodePhase,
    pub(crate) slot: u16,
    pub(crate) role: PaintChunkRole,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum PaintPropertyScope {
    #[default]
    SelfPaint,
    Contents,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum PaintNodePhase {
    #[default]
    BeforeChildren,
    AfterChildren,
}

/// Reserved `SelfDecoration` slot for the typed child-mask scope.
/// The phase distinguishes the begin and end chunks.
pub(crate) const RETAINED_CHILD_MASK_SLOT: u16 = u16::MAX;

#[derive(Clone, Debug)]
pub(crate) struct RetainedChildMaskPlan {
    bounds: Rect,
    logical_scissor: [u32; 4],
    op: DrawRectOp,
    payload_identity: PaintPayloadIdentity,
    in_scope_children: Arc<[NodeKey]>,
    overflow_children: Arc<[NodeKey]>,
}

impl RetainedChildMaskPlan {
    /// Compare the current complete mask geometry and ordered partition without
    /// allocating another pair of child lists. Eligibility is checked by the
    /// caller on every invocation; this does not certify a host's capability.
    pub(crate) fn matches_live_inputs(
        &self,
        bounds: Rect,
        logical_scissor: [u32; 4],
        op: &DrawRectOp,
        children: impl IntoIterator<Item = (NodeKey, bool)>,
    ) -> bool {
        if [bounds.x, bounds.y, bounds.width, bounds.height].map(f32::to_bits)
            != [
                self.bounds.x,
                self.bounds.y,
                self.bounds.width,
                self.bounds.height,
            ]
            .map(f32::to_bits)
            || self.logical_scissor != logical_scissor
            || !self.payload_identity.matches_rects([op])
        {
            return false;
        }
        let mut inside = self.in_scope_children.iter();
        let mut outside = self.overflow_children.iter();
        for (child, overflow) in children {
            let next = if overflow {
                outside.next()
            } else {
                inside.next()
            };
            if next != Some(&child) {
                return false;
            }
        }
        inside.next().is_none() && outside.next().is_none()
    }

    pub(crate) fn new(
        bounds: Rect,
        logical_scissor: [u32; 4],
        op: DrawRectOp,
        children: &[NodeKey],
        in_scope_children: Vec<NodeKey>,
        overflow_children: Vec<NodeKey>,
    ) -> Option<Self> {
        if crate::view::base_component::logical_scissor_for_clip_rect(bounds)
            != Some(logical_scissor)
            || op.mode != crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
            || op.params.position != [bounds.x, bounds.y]
            || op.params.size != [bounds.width, bounds.height]
            || op
                .params
                .size
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.0)
        {
            return None;
        }
        let mut reconstructed = Vec::with_capacity(children.len());
        let mut in_scope = in_scope_children.iter().copied();
        let mut overflow = overflow_children.iter().copied();
        let mut next_in_scope = in_scope.next();
        let mut next_overflow = overflow.next();
        for &child in children {
            if next_in_scope == Some(child) {
                reconstructed.push(child);
                next_in_scope = in_scope.next();
            } else if next_overflow == Some(child) {
                reconstructed.push(child);
                next_overflow = overflow.next();
            } else {
                return None;
            }
        }
        if next_in_scope.is_some()
            || next_overflow.is_some()
            || reconstructed.as_slice() != children
        {
            return None;
        }
        let payload_identity = PaintPayloadIdentity::prepared_rects([&op])?;
        Some(Self {
            bounds,
            logical_scissor,
            op,
            payload_identity,
            in_scope_children: in_scope_children.into(),
            overflow_children: overflow_children.into(),
        })
    }

    pub(crate) fn in_scope_children(&self) -> &[NodeKey] {
        &self.in_scope_children
    }

    pub(crate) fn overflow_children(&self) -> &[NodeKey] {
        &self.overflow_children
    }

    pub(crate) fn metadata(
        &self,
        owner: NodeKey,
        phase: PaintNodePhase,
        properties: PropertyTreeState,
        content_revision: PaintContentRevision,
    ) -> PaintChunkMetadata {
        PaintChunkMetadata {
            frame: crate::view::paint::PaintChunkFrame::Viewport,
            id: PaintChunkId {
                owner,
                scope: PaintPropertyScope::Contents,
                phase,
                slot: RETAINED_CHILD_MASK_SLOT,
                role: PaintChunkRole::SelfDecoration,
            },
            owner,
            bounds: self.bounds,
            properties,
            content_revision,
            payload_identity: self.payload_identity.clone(),
        }
    }

    pub(crate) fn artifact(
        &self,
        owner: NodeKey,
        phase: PaintNodePhase,
        properties: PropertyTreeState,
        content_revision: PaintContentRevision,
    ) -> PaintArtifact {
        PaintArtifact {
            chunks: vec![PaintChunk {
                frame: crate::view::paint::PaintChunkFrame::Viewport,
                id: PaintChunkId {
                    owner,
                    scope: PaintPropertyScope::Contents,
                    phase,
                    slot: RETAINED_CHILD_MASK_SLOT,
                    role: PaintChunkRole::SelfDecoration,
                },
                owner,
                op_range: 0..1,
                bounds: self.bounds,
                properties,
                content_revision,
                payload_identity: self.payload_identity.clone(),
            }]
            .into(),
            ops: vec![PaintOp::DrawRect(self.op.clone())].into(),
            clip_nodes: Vec::new(),
            effect_nodes: Vec::new(),
            transform_nodes: Vec::new(),
            layout_position_nodes: Vec::new(),
            visual_offset_nodes: Vec::new(),
            scroll_nodes: Vec::new(),
            owner_property_states: Vec::new(),
            owner_nodes: vec![PaintOwnerSnapshot {
                owner,
                parent: None,
                snap: PaintOwnerSnap::INHERIT,
            }],
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PaintNodePlan<T> {
    pub(crate) before_children: Vec<T>,
    pub(crate) after_children: Vec<T>,
}

impl<T> Default for PaintNodePlan<T> {
    fn default() -> Self {
        Self {
            before_children: Vec::new(),
            after_children: Vec::new(),
        }
    }
}

impl<T> PaintNodePlan<T> {
    pub(crate) fn single_before(item: T) -> Self {
        Self {
            before_children: vec![item],
            after_children: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PaintChunkRole {
    SelfDecoration,
    TextGlyphs,
    ImageContent,
    SvgContent,
    GpuContent,
    SelectionUnderlay,
    TextDecoration,
    Caret,
    ScrollbarOverlay,
}

#[derive(Clone, Debug)]
pub(crate) enum PaintOp {
    DrawRect(DrawRectOp),
    PreparedInlineIfcDecoration(Arc<PreparedInlineIfcDecorationOp>),
    PreparedShadow(PreparedShadowOp),
    PreparedScrollbarOverlay(Arc<PreparedScrollbarOverlayOp>),
    PreparedText(PreparedTextOp),
    PreparedImage(PreparedImageOp),
    PreparedSvg(PreparedSvgOp),
    PreparedGpu(PreparedGpuOp),
}

impl PaintOp {
    pub(crate) fn inline_decoration(op: PreparedInlineIfcDecorationOp) -> Self {
        Self::PreparedInlineIfcDecoration(Arc::new(op))
    }
    pub(crate) fn scrollbar_overlay(op: PreparedScrollbarOverlayOp) -> Self {
        Self::PreparedScrollbarOverlay(Arc::new(op))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DrawRectOp {
    pub(crate) params: RectPassParams,
    pub(crate) mode: RectRenderMode,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedInlineIfcDecorationOp {
    pub(crate) descriptor: PreparedInlineIfcDecorationDescriptor,
    pub(crate) fill: RectPassParams,
    pub(crate) border: Option<RectPassParams>,
    identity: PreparedInlineIfcDecorationIdentity,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedInlineIfcDecorationDescriptor {
    pub(crate) source: u64,
    pub(crate) line_index: usize,
    pub(crate) range: Range<usize>,
    pub(crate) style_key: [u8; 4],
    pub(crate) slice_insets: [f32; 4],
    pub(crate) is_first_for_source: bool,
    pub(crate) is_last_for_source: bool,
}

impl PreparedInlineIfcDecorationOp {
    pub(crate) fn new(
        descriptor: PreparedInlineIfcDecorationDescriptor,
        fill: RectPassParams,
        border: Option<RectPassParams>,
    ) -> Option<Self> {
        let identity =
            PreparedInlineIfcDecorationIdentity::from_parts(&descriptor, &fill, border.as_ref())?;
        Some(Self {
            descriptor,
            fill,
            border,
            identity,
        })
    }

    pub(crate) fn frozen_identity(&self) -> PreparedInlineIfcDecorationIdentity {
        self.identity.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedInlineIfcDecorationIdentity {
    source: u64,
    line_index: usize,
    range: Range<usize>,
    style_key: [u8; 4],
    slice_insets_bits: [u32; 4],
    is_first_for_source: bool,
    is_last_for_source: bool,
    fill: PreparedInlineIfcRectIdentity,
    border: Option<PreparedInlineIfcRectIdentity>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedInlineIfcRectIdentity {
    position_bits: [u32; 2],
    size_bits: [u32; 2],
    fill_color_bits: [u32; 4],
    opacity_bits: u32,
    border_width_bits: [u32; 4],
    border_radius_bits: [[u32; 2]; 4],
    border_color_bits: [u32; 4],
    border_side_color_bits: [[u32; 4]; 4],
    use_border_side_colors: bool,
    depth_bits: u32,
}

impl PreparedInlineIfcDecorationIdentity {
    fn from_parts(
        descriptor: &PreparedInlineIfcDecorationDescriptor,
        fill: &RectPassParams,
        border: Option<&RectPassParams>,
    ) -> Option<Self> {
        if descriptor.range.start > descriptor.range.end
            || descriptor
                .slice_insets
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
        {
            return None;
        }
        let fill_identity = PreparedInlineIfcRectIdentity::from_params(fill)?;
        let border_identity = match border {
            Some(border) => Some(PreparedInlineIfcRectIdentity::from_params(border)?),
            None => None,
        };
        let has_border = fill.border_widths.iter().any(|width| *width > 0.0);
        if has_border != border.is_some() {
            return None;
        }
        if let Some(border) = border {
            if fill.position.map(f32::to_bits) != border.position.map(f32::to_bits)
                || fill.size.map(f32::to_bits) != border.size.map(f32::to_bits)
                || fill.opacity.to_bits() != border.opacity.to_bits()
                || fill.border_widths.map(f32::to_bits) != border.border_widths.map(f32::to_bits)
                || fill.border_radii.map(|radius| radius.map(f32::to_bits))
                    != border.border_radii.map(|radius| radius.map(f32::to_bits))
                || fill.border_color.map(f32::to_bits) != border.border_color.map(f32::to_bits)
                || fill.border_side_colors.map(|color| color.map(f32::to_bits))
                    != border
                        .border_side_colors
                        .map(|color| color.map(f32::to_bits))
                || fill.use_border_side_colors != border.use_border_side_colors
                || fill.depth.to_bits() != border.depth.to_bits()
                || border.fill_color.map(f32::to_bits) != [0.0_f32.to_bits(); 4]
                || !border.use_border_side_colors
            {
                return None;
            }
        }
        Some(Self {
            source: descriptor.source,
            line_index: descriptor.line_index,
            range: descriptor.range.clone(),
            style_key: descriptor.style_key,
            slice_insets_bits: descriptor.slice_insets.map(f32::to_bits),
            is_first_for_source: descriptor.is_first_for_source,
            is_last_for_source: descriptor.is_last_for_source,
            fill: fill_identity,
            border: border_identity,
        })
    }
}

impl PreparedInlineIfcRectIdentity {
    fn from_params(params: &RectPassParams) -> Option<Self> {
        let colors_are_valid = params
            .fill_color
            .iter()
            .chain(params.border_color.iter())
            .chain(params.border_side_colors.iter().flatten())
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel));
        if params.position.iter().any(|value| !value.is_finite())
            || params
                .size
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.0)
            || !((params.position[0] + params.size[0]).is_finite()
                && (params.position[1] + params.size[1]).is_finite())
            || !colors_are_valid
            || !params.opacity.is_finite()
            || !(0.0..=1.0).contains(&params.opacity)
            || params
                .border_widths
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
            || params
                .border_radii
                .iter()
                .flatten()
                .any(|value| !value.is_finite() || *value < 0.0)
            || !params.depth.is_finite()
            || params.gradient.is_some()
            || params.border_gradient.is_some()
        {
            return None;
        }
        Some(Self {
            position_bits: params.position.map(f32::to_bits),
            size_bits: params.size.map(f32::to_bits),
            fill_color_bits: params.fill_color.map(f32::to_bits),
            opacity_bits: params.opacity.to_bits(),
            border_width_bits: params.border_widths.map(f32::to_bits),
            border_radius_bits: params.border_radii.map(|radius| radius.map(f32::to_bits)),
            border_color_bits: params.border_color.map(f32::to_bits),
            border_side_color_bits: params
                .border_side_colors
                .map(|color| color.map(f32::to_bits)),
            use_border_side_colors: params.use_border_side_colors,
            depth_bits: params.depth.to_bits(),
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedShadowOp {
    pub(crate) shape: ShadowShape,
    pub(crate) params: ShadowParams,
    pub(crate) identity: PreparedShadowIdentity,
}

impl PreparedShadowOp {
    pub(crate) fn new(shape: ShadowShape, params: ShadowParams) -> Option<Self> {
        let identity = PreparedShadowIdentity::from_parts(&shape, params)?;
        Some(Self {
            shape,
            params,
            identity,
        })
    }

    pub(crate) fn frozen_identity(&self) -> PreparedShadowIdentity {
        self.identity.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedShadowIdentity {
    /// x, y, width, height, then the four corner radii.
    shape_bits: [u32; 8],
    offset_bits: [u32; 2],
    blur_radius_bits: u32,
    color_bits: [u32; 4],
    opacity_bits: u32,
    clip_to_geometry: bool,
}

/// One indivisible legacy-order scrollbar overlay. Keeping the two shadows
/// and two fills behind one typed op prevents generic shadow/rect grammar from
/// being widened for the retained scroll-host canary.
#[derive(Clone, Debug)]
pub(crate) struct PreparedScrollbarOverlayOp {
    pub(crate) track_shadow: PreparedScrollbarShadowOp,
    pub(crate) track: DrawRectOp,
    pub(crate) thumb_shadow: PreparedScrollbarShadowOp,
    pub(crate) thumb: DrawRectOp,
    secondary: Option<Box<PreparedScrollbarAxisOp>>,
    identity: PreparedScrollbarOverlayIdentity,
}

#[derive(Clone, Debug)]
struct PreparedScrollbarAxisOp {
    track_shadow: PreparedScrollbarShadowOp,
    track: DrawRectOp,
    thumb_shadow: PreparedScrollbarShadowOp,
    thumb: DrawRectOp,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedScrollbarShadowOp {
    pub(crate) shape: ShadowShape,
    pub(crate) params: ShadowParams,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedScrollbarOverlayIdentity {
    track_shadow: PreparedScrollbarShadowIdentity,
    track: PreparedDrawRectIdentity,
    thumb_shadow: PreparedScrollbarShadowIdentity,
    thumb: PreparedDrawRectIdentity,
    secondary: Option<Box<PreparedScrollbarAxisIdentity>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedScrollbarAxisIdentity {
    track_shadow: PreparedScrollbarShadowIdentity,
    track: PreparedDrawRectIdentity,
    thumb_shadow: PreparedScrollbarShadowIdentity,
    thumb: PreparedDrawRectIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedScrollbarShadowIdentity(PreparedShadowIdentity);

impl PreparedScrollbarOverlayOp {
    pub(crate) fn from_witness(witness: ScrollbarOverlayWitness) -> Option<Self> {
        let alpha = witness.sampled_alpha;
        if !matches!(
            witness.paint_state,
            ScrollbarPaintStateWitness::OpaqueNow | ScrollbarPaintStateWitness::TranslucentNow
        ) || !alpha.is_finite()
            || alpha <= 0.0
            || alpha > 1.0
            || (witness.paint_state == ScrollbarPaintStateWitness::OpaqueNow
                && alpha.to_bits() != 1.0_f32.to_bits())
            || (witness.paint_state == ScrollbarPaintStateWitness::TranslucentNow
                && (alpha.to_bits() == 0.0_f32.to_bits() || alpha.to_bits() == 1.0_f32.to_bits()))
            || !witness.shadow_blur_radius.is_finite()
            || witness.shadow_blur_radius < 0.0
        {
            return None;
        }
        let mut axes = Vec::with_capacity(2);
        if let Some((track, thumb)) = witness.vertical_track.zip(witness.vertical_thumb) {
            axes.push(Self::axis(track, thumb, witness.shadow_blur_radius, alpha)?);
        } else if witness.vertical_track.is_some() || witness.vertical_thumb.is_some() {
            return None;
        }
        if let Some((track, thumb)) = witness.horizontal_track.zip(witness.horizontal_thumb) {
            axes.push(Self::axis(track, thumb, witness.shadow_blur_radius, alpha)?);
        } else if witness.horizontal_track.is_some() || witness.horizontal_thumb.is_some() {
            return None;
        }
        if axes.is_empty() {
            return None;
        }
        let first = axes.remove(0);
        let secondary = axes.pop().map(Box::new);
        let identity = PreparedScrollbarOverlayIdentity::from_parts(
            &first.track_shadow,
            &first.track,
            &first.thumb_shadow,
            &first.thumb,
            secondary.as_deref(),
        )?;
        Some(Self {
            track_shadow: first.track_shadow,
            track: first.track,
            thumb_shadow: first.thumb_shadow,
            thumb: first.thumb,
            secondary,
            identity,
        })
    }

    fn axis(
        track: Rect,
        thumb: Rect,
        shadow_blur_radius: f32,
        alpha: f32,
    ) -> Option<PreparedScrollbarAxisOp> {
        let track_shadow = Self::shadow(track, shadow_blur_radius, 0.5 * alpha)?;
        let track = Self::fill(track, [0.95, 0.95, 0.95, 0.35 * alpha])?;
        let thumb_shadow = Self::shadow(thumb, shadow_blur_radius, 0.5 * alpha)?;
        let thumb = Self::fill(thumb, [0.95, 0.95, 0.95, 0.58 * alpha])?;
        Some(PreparedScrollbarAxisOp {
            track_shadow,
            track,
            thumb_shadow,
            thumb,
        })
    }

    fn shadow(rect: Rect, blur_radius: f32, alpha: f32) -> Option<PreparedScrollbarShadowOp> {
        let radius = (rect.width.min(rect.height) * 0.5).max(0.0);
        let shadow = PreparedScrollbarShadowOp {
            shape: ShadowShape::rounded_rect(
                rect.x,
                rect.y,
                rect.width.max(0.0),
                rect.height.max(0.0),
                radius,
            ),
            params: ShadowParams {
                offset_x: 1.0,
                offset_y: 1.0,
                blur_radius,
                color: [0.0, 0.0, 0.0, alpha],
                opacity: 1.0,
                clip_to_geometry: true,
            },
        };
        PreparedScrollbarShadowIdentity::from_parts(&shadow.shape, shadow.params)?;
        Some(shadow)
    }

    fn fill(rect: Rect, color: [f32; 4]) -> Option<DrawRectOp> {
        let mut params = RectPassParams {
            position: [rect.x, rect.y],
            size: [rect.width, rect.height],
            fill_color: color,
            opacity: 1.0,
            ..Default::default()
        };
        params.set_border_width(0.0);
        params.set_border_radius((rect.width.min(rect.height) * 0.5).max(0.0));
        let op = DrawRectOp {
            params,
            mode: RectRenderMode::FillOnly,
        };
        PreparedDrawRectIdentity::from_op(&op)?;
        Some(op)
    }

    pub(crate) fn secondary_axis(
        &self,
    ) -> Option<(
        &PreparedScrollbarShadowOp,
        &DrawRectOp,
        &PreparedScrollbarShadowOp,
        &DrawRectOp,
    )> {
        self.secondary.as_deref().map(|axis| {
            (
                &axis.track_shadow,
                &axis.track,
                &axis.thumb_shadow,
                &axis.thumb,
            )
        })
    }

    pub(crate) fn frozen_identity(&self) -> PreparedScrollbarOverlayIdentity {
        self.identity.clone()
    }

    /// Rebuilds every spatial primitive in one indivisible overlay while
    /// retaining the axis order and semantic witness frozen by this type.
    pub(crate) fn translated_by(&self, delta: [f32; 2]) -> Option<Self> {
        if delta.into_iter().any(|value| !value.is_finite()) {
            return None;
        }
        let mut translated = self.clone();
        let translate_shadow = |shadow: &mut PreparedScrollbarShadowOp| -> Option<()> {
            shadow.shape = shadow.shape.translated(delta[0], delta[1]);
            shadow.shape.is_valid().then_some(())
        };
        let translate_rect = |rect: &mut DrawRectOp| -> Option<()> {
            rect.params.position[0] += delta[0];
            rect.params.position[1] += delta[1];
            rect.params
                .position
                .iter()
                .all(|value| value.is_finite())
                .then_some(())
        };
        translate_shadow(&mut translated.track_shadow)?;
        translate_rect(&mut translated.track)?;
        translate_shadow(&mut translated.thumb_shadow)?;
        translate_rect(&mut translated.thumb)?;
        if let Some(axis) = translated.secondary.as_deref_mut() {
            translate_shadow(&mut axis.track_shadow)?;
            translate_rect(&mut axis.track)?;
            translate_shadow(&mut axis.thumb_shadow)?;
            translate_rect(&mut axis.thumb)?;
        }
        translated.identity = PreparedScrollbarOverlayIdentity::from_parts(
            &translated.track_shadow,
            &translated.track,
            &translated.thumb_shadow,
            &translated.thumb,
            translated.secondary.as_deref(),
        )?;
        Some(translated)
    }

    #[cfg(test)]
    pub(crate) fn has_baked_opacity(&self, expected_bits: u32) -> bool {
        self.track_shadow.params.opacity.to_bits() == expected_bits
            && self.track.params.opacity.to_bits() == expected_bits
            && self.thumb_shadow.params.opacity.to_bits() == expected_bits
            && self.thumb.params.opacity.to_bits() == expected_bits
            && self.secondary.as_deref().is_none_or(|axis| {
                axis.track_shadow.params.opacity.to_bits() == expected_bits
                    && axis.track.params.opacity.to_bits() == expected_bits
                    && axis.thumb_shadow.params.opacity.to_bits() == expected_bits
                    && axis.thumb.params.opacity.to_bits() == expected_bits
            })
    }

    /// Rebuilds the frozen overlay identity after the surface raster planner
    /// removes the owner-local opacity that will be applied by the composite
    /// edge. Every primitive participates; a partially neutralized overlay is
    /// not representable.
    pub(crate) fn with_baked_opacity(&self, opacity: f32) -> Option<Self> {
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return None;
        }
        let mut rebuilt = self.clone();
        let set_axis = |track_shadow: &mut PreparedScrollbarShadowOp,
                        track: &mut DrawRectOp,
                        thumb_shadow: &mut PreparedScrollbarShadowOp,
                        thumb: &mut DrawRectOp| {
            track_shadow.params.opacity = opacity;
            track.params.opacity = opacity;
            thumb_shadow.params.opacity = opacity;
            thumb.params.opacity = opacity;
        };
        set_axis(
            &mut rebuilt.track_shadow,
            &mut rebuilt.track,
            &mut rebuilt.thumb_shadow,
            &mut rebuilt.thumb,
        );
        if let Some(axis) = rebuilt.secondary.as_deref_mut() {
            set_axis(
                &mut axis.track_shadow,
                &mut axis.track,
                &mut axis.thumb_shadow,
                &mut axis.thumb,
            );
        }
        rebuilt.identity = PreparedScrollbarOverlayIdentity::from_parts(
            &rebuilt.track_shadow,
            &rebuilt.track,
            &rebuilt.thumb_shadow,
            &rebuilt.thumb,
            rebuilt.secondary.as_deref(),
        )?;
        Some(rebuilt)
    }
}

impl PreparedScrollbarOverlayIdentity {
    fn from_parts(
        track_shadow: &PreparedScrollbarShadowOp,
        track: &DrawRectOp,
        thumb_shadow: &PreparedScrollbarShadowOp,
        thumb: &DrawRectOp,
        secondary: Option<&PreparedScrollbarAxisOp>,
    ) -> Option<Self> {
        Some(Self {
            track_shadow: PreparedScrollbarShadowIdentity::from_parts(
                &track_shadow.shape,
                track_shadow.params,
            )?,
            track: PreparedDrawRectIdentity::from_op(track)?,
            thumb_shadow: PreparedScrollbarShadowIdentity::from_parts(
                &thumb_shadow.shape,
                thumb_shadow.params,
            )?,
            thumb: PreparedDrawRectIdentity::from_op(thumb)?,
            secondary: match secondary {
                Some(axis) => Some(Box::new(PreparedScrollbarAxisIdentity::from_axis(axis)?)),
                None => None,
            },
        })
    }
}

impl PreparedScrollbarAxisIdentity {
    fn from_axis(axis: &PreparedScrollbarAxisOp) -> Option<Self> {
        Some(Self {
            track_shadow: PreparedScrollbarShadowIdentity::from_parts(
                &axis.track_shadow.shape,
                axis.track_shadow.params,
            )?,
            track: PreparedDrawRectIdentity::from_op(&axis.track)?,
            thumb_shadow: PreparedScrollbarShadowIdentity::from_parts(
                &axis.thumb_shadow.shape,
                axis.thumb_shadow.params,
            )?,
            thumb: PreparedDrawRectIdentity::from_op(&axis.thumb)?,
        })
    }
}

impl PreparedScrollbarShadowIdentity {
    fn from_parts(shape: &ShadowShape, params: ShadowParams) -> Option<Self> {
        PreparedShadowIdentity::from_parts(shape, params).map(Self)
    }
}

impl PreparedShadowIdentity {
    fn from_parts(shape: &ShadowShape, params: ShadowParams) -> Option<Self> {
        if !shape.is_valid()
            || !params.offset_x.is_finite()
            || !params.offset_y.is_finite()
            || !params.blur_radius.is_finite()
            || params.blur_radius.to_bits() != params.blur_radius.max(0.0).to_bits()
            || params
                .color
                .iter()
                .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(channel))
            || !params.opacity.is_finite()
            || !(0.0..=1.0).contains(&params.opacity)
        {
            return None;
        }
        let [tl, tr, br, bl] = shape.radii;
        Some(Self {
            shape_bits: [shape.x, shape.y, shape.width, shape.height, tl, tr, br, bl]
                .map(f32::to_bits),
            offset_bits: [params.offset_x.to_bits(), params.offset_y.to_bits()],
            blur_radius_bits: params.blur_radius.to_bits(),
            color_bits: params.color.map(f32::to_bits),
            opacity_bits: params.opacity.to_bits(),
            clip_to_geometry: params.clip_to_geometry,
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedTextOp {
    pub(crate) params: Arc<TextPassPreparedParams>,
    identity: PreparedTextIdentity,
}

impl PreparedTextOp {
    pub(crate) fn new(params: impl Into<Arc<TextPassPreparedParams>>) -> Option<Self> {
        let params = params.into();
        #[cfg(test)]
        prepared_text_identity_tests::note_construction();
        let identity = PreparedTextIdentity::from_params(&params)?;
        Some(Self { params, identity })
    }

    #[cfg(test)]
    pub(crate) fn has_baked_opacity(&self, expected_bits: u32) -> bool {
        self.params
            .staging_input
            .glyphs
            .iter()
            .all(|glyph| glyph.paint.opacity.to_bits() == expected_bits)
    }

    pub(crate) fn frozen_identity(&self) -> PreparedTextIdentity {
        self.identity.clone()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedImageOp {
    pub(crate) params: TextureCompositeParams,
    pub(crate) upload: SampledTextureUpload,
}

/// Arena- and registry-independent SVG raster payload frozen for one paint
/// artifact. SVG keeps a distinct op/identity from Image even though both
/// currently compile to `TextureCompositePass`; the typed boundary prevents
/// the two asset namespaces from becoming interchangeable by accident.
#[derive(Clone, Debug)]
pub(crate) struct PreparedSvgOp {
    pub(crate) params: TextureCompositeParams,
    pub(crate) upload: SampledTextureUpload,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedDrawRectIdentity {
    mode: RectRenderMode,
    params: PreparedDrawRectParamsIdentity,
}

/// Component-independent semantic and raster identity for a text selection
/// underlay.
///
/// The character range is intentionally duplicated beside the exact ordered
/// rectangle identities. The payload validates its own range, color, and draw
/// operations; no component-owned grammar is needed to make it canonical.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextSelectionPayloadIdentity {
    pub(crate) start_char: usize,
    pub(crate) end_char: usize,
    pub(crate) color_rgba_bits: [u32; 4],
    pub(crate) rects: Arc<[PreparedDrawRectIdentity]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaintArtifactContractViolation {
    CompositeOwner,
    CompositeBounds,
    CompositeClip,
    CompositePayload,
    SelectionRange,
    SelectionColor,
    SelectionRectIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintArtifactContractRejection {
    pub(crate) owner: NodeKey,
    pub(crate) violation: PaintArtifactContractViolation,
}

/// Component-independent source facts for one text-selection underlay.
///
/// Geometry and prepared draw identities are deliberately absent here: the
/// recorder must obtain those from the emitted artifact and bind them through
/// [`TextSelectionPayloadIdentity`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintTextSelectionSource {
    pub(crate) start_char: usize,
    pub(crate) end_char: usize,
    pub(crate) color_rgba_bits: [u32; 4],
}

impl PaintTextSelectionSource {
    pub(crate) fn is_canonical(self) -> bool {
        self.start_char < self.end_char
            && self
                .color_rgba_bits
                .map(f32::from_bits)
                .into_iter()
                .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
    }
}

/// Generic semantic source for a resident text payload. These cases describe
/// emitted text primitives, not a component or surface grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaintTextContentSource {
    Glyphs,
    Selection(PaintTextSelectionSource),
    Preedit,
}

impl PaintTextContentSource {
    pub(crate) fn is_canonical(self) -> bool {
        match self {
            Self::Glyphs | Self::Preedit => true,
            Self::Selection(selection) => selection.is_canonical(),
        }
    }

    pub(crate) fn selection(self) -> Option<PaintTextSelectionSource> {
        match self {
            Self::Selection(selection) => Some(selection),
            Self::Glyphs | Self::Preedit => None,
        }
    }
}

/// Artifact-facing facts for one admitted atomic projection.
///
/// This intentionally contains neither the component's exact-shape grammar
/// nor layout/measure internals. The exact admission remains a transient
/// selector predicate; recorder and compiler consume only the owner topology
/// and projection geometry that the emitted artifact must reproduce.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PaintAtomicProjectionArtifactSource {
    pub(crate) projection_text_owner: NodeKey,
    pub(crate) projection_text_bounds_bits: [u32; 4],
    /// `(owner, parent)` edges below the TextArea root.
    pub(crate) descendant_owner_topology: Arc<[(NodeKey, Option<NodeKey>)]>,
}

impl PaintAtomicProjectionArtifactSource {
    pub(crate) fn is_canonical_for(&self, text_area_root: NodeKey) -> bool {
        let [x, y, width, height] = self.projection_text_bounds_bits.map(f32::from_bits);
        if self.projection_text_owner == text_area_root
            || [x, y, width, height]
                .into_iter()
                .any(|value| !value.is_finite())
            || width <= 0.0
            || height <= 0.0
            || !(x + width).is_finite()
            || !(y + height).is_finite()
            || self.descendant_owner_topology.is_empty()
        {
            return false;
        }
        let mut parents = FxHashMap::default();
        if self
            .descendant_owner_topology
            .iter()
            .any(|&(owner, parent)| parents.insert(owner, parent).is_some())
        {
            return false;
        }
        let Some(Some(projection_root)) = parents.get(&self.projection_text_owner).copied() else {
            return false;
        };
        projection_root != text_area_root
            && parents.get(&projection_root) == Some(&Some(text_area_root))
            && self
                .descendant_owner_topology
                .iter()
                .all(|&(owner, parent)| {
                    owner != text_area_root
                        && parent.is_some_and(|parent| {
                            parent == text_area_root || parents.contains_key(&parent)
                        })
                })
    }
}

impl TextSelectionPayloadIdentity {
    fn validate_parts(&self) -> Result<(), PaintArtifactContractViolation> {
        if self.start_char >= self.end_char {
            return Err(PaintArtifactContractViolation::SelectionRange);
        }
        if self
            .color_rgba_bits
            .map(f32::from_bits)
            .into_iter()
            .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(&channel))
        {
            return Err(PaintArtifactContractViolation::SelectionColor);
        }
        if self.rects.is_empty()
            || self.rects.iter().any(|rect| {
                rect.mode != RectRenderMode::FillOnly
                    || rect.params.fill_color_bits != self.color_rgba_bits
                    || rect.params.opacity_bits != 1.0_f32.to_bits()
            })
        {
            return Err(PaintArtifactContractViolation::SelectionRectIdentity);
        }
        Ok(())
    }

    pub(crate) fn is_canonical(&self) -> bool {
        self.validate_parts().is_ok()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedDrawRectParamsIdentity {
    position_bits: [u32; 2],
    size_bits: [u32; 2],
    fill_color_bits: [u32; 4],
    opacity_bits: u32,
    border_width_bits: [u32; 4],
    border_radius_bits: [[u32; 2]; 4],
    border_color_bits: [u32; 4],
    border_side_color_bits: [[u32; 4]; 4],
    use_border_side_colors: bool,
    depth_bits: u32,
    gradient: Option<PreparedGradientIdentity>,
    border_gradient: Option<PreparedGradientIdentity>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedGradientIdentity {
    kind: GradientKindGpu,
    axis_bits: [u32; 4],
    repeating: bool,
    stops: Arc<[PreparedGradientStopIdentity]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PreparedGradientStopIdentity {
    color_bits: [u32; 4],
    position_bits: [u32; 4],
}

impl PreparedDrawRectIdentity {
    fn from_op(op: &DrawRectOp) -> Option<Self> {
        Some(Self {
            mode: op.mode,
            params: PreparedDrawRectParamsIdentity::from_params(&op.params)?,
        })
    }
}

impl PreparedDrawRectParamsIdentity {
    fn from_params(params: &RectPassParams) -> Option<Self> {
        let colors_are_valid = params
            .fill_color
            .iter()
            .chain(params.border_color.iter())
            .chain(params.border_side_colors.iter().flatten())
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel));
        if params.position.iter().any(|value| !value.is_finite())
            || params
                .size
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.0)
            || !((params.position[0] + params.size[0]).is_finite()
                && (params.position[1] + params.size[1]).is_finite())
            || !colors_are_valid
            || !params.opacity.is_finite()
            || !(0.0..=1.0).contains(&params.opacity)
            || params
                .border_widths
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
            || params
                .border_radii
                .iter()
                .flatten()
                .any(|value| !value.is_finite() || *value < 0.0)
            || !params.depth.is_finite()
        {
            return None;
        }
        Some(Self {
            position_bits: params.position.map(f32::to_bits),
            size_bits: params.size.map(f32::to_bits),
            fill_color_bits: params.fill_color.map(f32::to_bits),
            opacity_bits: params.opacity.to_bits(),
            border_width_bits: params.border_widths.map(f32::to_bits),
            border_radius_bits: params.border_radii.map(|radius| radius.map(f32::to_bits)),
            border_color_bits: params.border_color.map(f32::to_bits),
            border_side_color_bits: params
                .border_side_colors
                .map(|color| color.map(f32::to_bits)),
            use_border_side_colors: params.use_border_side_colors,
            depth_bits: params.depth.to_bits(),
            gradient: match params.gradient.as_ref() {
                Some(gradient) => Some(PreparedGradientIdentity::from_paint(gradient)?),
                None => None,
            },
            border_gradient: match params.border_gradient.as_ref() {
                Some(gradient) => Some(PreparedGradientIdentity::from_paint(gradient)?),
                None => None,
            },
        })
    }
}

impl PreparedGradientIdentity {
    fn from_paint(paint: &GradientPaint) -> Option<Self> {
        if paint.axis.iter().any(|value| !value.is_finite()) || paint.stops.is_empty() {
            return None;
        }
        let stops = paint
            .stops
            .iter()
            .map(|stop| {
                if stop
                    .color
                    .iter()
                    .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
                    || stop.pos.iter().any(|value| !value.is_finite())
                {
                    return None;
                }
                Some(PreparedGradientStopIdentity {
                    color_bits: stop.color.map(f32::to_bits),
                    position_bits: stop.pos.map(f32::to_bits),
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            kind: paint.kind,
            axis_bits: paint.axis.map(f32::to_bits),
            repeating: paint.repeating,
            stops: stops.into(),
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum PaintPayloadIdentity {
    #[default]
    None,
    Image(PreparedImageIdentity, Arc<[PreparedDrawRectIdentity]>),
    ImageWithShadows(
        PreparedImageIdentity,
        Arc<[PreparedShadowIdentity]>,
        Arc<[PreparedDrawRectIdentity]>,
    ),
    Gpu(PreparedGpuIdentity),
    Svg(PreparedSvgIdentity, Arc<[PreparedDrawRectIdentity]>),
    SvgWithShadows(
        PreparedSvgIdentity,
        Arc<[PreparedShadowIdentity]>,
        Arc<[PreparedDrawRectIdentity]>,
    ),
    PreparedShadows(
        Arc<[PreparedShadowIdentity]>,
        Arc<[PreparedDrawRectIdentity]>,
    ),
    PreparedTexts(Arc<[PreparedTextIdentity]>),
    PreparedRects(Arc<[PreparedDrawRectIdentity]>),
    TextSelection(TextSelectionPayloadIdentity),
    PreparedScrollbarOverlay(Arc<PreparedScrollbarOverlayIdentity>),
    InlineIfcDecorations(
        Arc<[PreparedShadowIdentity]>,
        Arc<[PreparedInlineIfcDecorationIdentity]>,
    ),
}

/// Exact source record for a projection between two artifact coordinate spaces.
///
/// This is geometry/composite-side data. It validates how an already-recorded
/// host artifact projects into another artifact space, and must not be folded
/// into persistent texture keys or resident raster identity. Structural
/// equality includes the source origins and semantic revision; it is not a
/// derived spatial-transition comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintArtifactSpaceTransition {
    from_origin_bits: [u32; 2],
    to_origin_bits: [u32; 2],
    semantic_revision: u64,
}

impl PaintArtifactSpaceTransition {
    pub(crate) fn from_bits(
        from_origin_bits: [u32; 2],
        to_origin_bits: [u32; 2],
        semantic_revision: u64,
    ) -> Option<Self> {
        let transition = Self {
            from_origin_bits,
            to_origin_bits,
            semantic_revision,
        };
        transition.is_canonical().then_some(transition)
    }

    pub(crate) fn is_canonical(self) -> bool {
        self.semantic_revision != 0
            && self
                .from_origin_bits
                .into_iter()
                .chain(self.to_origin_bits)
                .map(f32::from_bits)
                .all(f32::is_finite)
            && self.translation().is_some()
    }

    fn derived_translation(self) -> Option<[f32; 2]> {
        let from = self.from_origin_bits.map(f32::from_bits);
        let to = self.to_origin_bits.map(f32::from_bits);
        let translation = [to[0] - from[0], to[1] - from[1]];
        translation
            .into_iter()
            .all(f32::is_finite)
            .then_some(translation)
    }

    pub(crate) fn translation(self) -> Option<[f32; 2]> {
        self.derived_translation()
    }
}

pub(crate) fn preedit_glyph_identity_is_exact(
    identity: &PaintPayloadIdentity,
    bounds_bits: [u32; 4],
    foreground_color_bits: [u32; 4],
) -> bool {
    let PaintPayloadIdentity::PreparedTexts(texts) = identity else {
        return false;
    };
    let [text] = texts.as_ref() else {
        return false;
    };
    if text.glyphs.is_empty()
        || text.fragments.is_empty()
        || text.glyphs.iter().any(|glyph| {
            glyph.color_bits != foreground_color_bits || glyph.opacity_bits != 1.0_f32.to_bits()
        })
    {
        return false;
    }
    let mut left = f32::INFINITY;
    let mut top = f32::INFINITY;
    let mut right = f32::NEG_INFINITY;
    let mut bottom = f32::NEG_INFINITY;
    for fragment in text.fragments.iter() {
        let [x, y] = fragment.origin_bits.map(f32::from_bits);
        let [width, height] = fragment.size_bits.map(f32::from_bits);
        left = left.min(x);
        top = top.min(y);
        right = right.max(x + width);
        bottom = bottom.max(y + height);
    }
    [left, top, right - left, bottom - top].map(f32::to_bits) == bounds_bits
}

pub(crate) fn preedit_underline_identity_is_exact(
    identity: &PaintPayloadIdentity,
    bounds_bits: [u32; 4],
    foreground_color_bits: [u32; 4],
) -> bool {
    let PaintPayloadIdentity::PreparedRects(rects) = identity else {
        return false;
    };
    if rects.is_empty() {
        return false;
    }
    let mut left = f32::INFINITY;
    let mut top = f32::INFINITY;
    let mut right = f32::NEG_INFINITY;
    let mut bottom = f32::NEG_INFINITY;
    for rect in rects.iter() {
        if rect.mode != RectRenderMode::FillOnly
            || rect.params.fill_color_bits != foreground_color_bits
            || rect.params.opacity_bits != 1.0_f32.to_bits()
            || rect.params.size_bits[1] != 1.0_f32.to_bits()
        {
            return false;
        }
        let [x, y] = rect.params.position_bits.map(f32::from_bits);
        let [width, height] = rect.params.size_bits.map(f32::from_bits);
        left = left.min(x);
        top = top.min(y);
        right = right.max(x + width);
        bottom = bottom.max(y + height);
    }
    [left, top, right - left, bottom - top].map(f32::to_bits) == bounds_bits
}

impl PaintPayloadIdentity {
    /// Rebuilds the same semantic payload variant from already-localized ops.
    /// Spatial bits may change; asset/source identity and typed payload shape
    /// must remain the variant and cardinality frozen by `self`.
    pub(crate) fn rebuild_from_localized_ops(&self, ops: &[PaintOp]) -> Option<Self> {
        match self {
            Self::Gpu(expected) => {
                let [PaintOp::PreparedGpu(op)] = ops else {
                    return None;
                };
                let identity = op.identity()?;
                (identity.source == expected.source).then_some(Self::Gpu(identity))
            }

            Self::None => ops.is_empty().then_some(Self::None),
            Self::PreparedRects(expected) => {
                let rects = ops
                    .iter()
                    .map(|op| match op {
                        PaintOp::DrawRect(rect) => Some(rect),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                if rects.len() != expected.len() {
                    return None;
                }
                Self::prepared_rects(rects)
            }
            Self::TextSelection(selection) => {
                let rects = ops
                    .iter()
                    .map(|op| match op {
                        PaintOp::DrawRect(rect) => Some(rect),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                (rects.len() == selection.rects.len()).then(|| {
                    Self::prepared_text_selection(
                        selection.start_char,
                        selection.end_char,
                        selection.color_rgba_bits,
                        rects,
                    )
                })?
            }
            Self::PreparedShadows(expected_shadows, expected_rects) => {
                let (shadows, rects) = ops.split_at(expected_shadows.len());
                let shadows = shadows
                    .iter()
                    .map(|op| match op {
                        PaintOp::PreparedShadow(shadow) => Some(shadow),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                let rects = rects
                    .iter()
                    .map(|op| match op {
                        PaintOp::DrawRect(rect) => Some(rect),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                if rects.len() != expected_rects.len() {
                    return None;
                }
                Self::prepared_shadows_with_decoration(shadows, rects)
            }
            Self::PreparedTexts(expected) => {
                let texts = ops
                    .iter()
                    .map(|op| match op {
                        PaintOp::PreparedText(text) => Some(text),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                (texts.len() == expected.len()).then(|| Self::prepared_texts(texts))
            }
            Self::PreparedScrollbarOverlay(_) => match ops {
                [PaintOp::PreparedScrollbarOverlay(overlay)] => {
                    Some(Self::prepared_scrollbar_overlay(overlay))
                }
                _ => None,
            },
            Self::InlineIfcDecorations(expected_shadows, expected_decorations) => {
                let (shadows, decorations) = ops.split_at(expected_shadows.len());
                let shadows = shadows
                    .iter()
                    .map(|op| match op {
                        PaintOp::PreparedShadow(shadow) => Some(shadow),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                let decorations = decorations
                    .iter()
                    .map(|op| match op {
                        PaintOp::PreparedInlineIfcDecoration(decoration) => {
                            Some(decoration.as_ref())
                        }
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                (decorations.len() == expected_decorations.len())
                    .then(|| Self::inline_ifc_decorations_with_shadows(shadows, decorations))
            }
            Self::Image(_, expected_rects) | Self::ImageWithShadows(_, _, expected_rects) => {
                let (image, prefix) = ops.split_last()?;
                let PaintOp::PreparedImage(image) = image else {
                    return None;
                };
                let shadow_count = match self {
                    Self::ImageWithShadows(_, shadows, _) => shadows.len(),
                    Self::Image(_, _) => 0,
                    _ => unreachable!(),
                };
                let (shadows, rects) = prefix.split_at(shadow_count);
                let shadows = shadows
                    .iter()
                    .map(|op| match op {
                        PaintOp::PreparedShadow(shadow) => Some(shadow),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                let rects = rects
                    .iter()
                    .map(|op| match op {
                        PaintOp::DrawRect(rect) => Some(rect),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                (rects.len() == expected_rects.len()).then(|| {
                    Self::image_with_shadows_and_decoration(
                        PreparedImageIdentity::from_op(image),
                        shadows,
                        rects,
                    )
                })?
            }
            Self::Svg(_, expected_rects) | Self::SvgWithShadows(_, _, expected_rects) => {
                let (svg, prefix) = ops.split_last()?;
                let PaintOp::PreparedSvg(svg) = svg else {
                    return None;
                };
                let shadow_count = match self {
                    Self::SvgWithShadows(_, shadows, _) => shadows.len(),
                    Self::Svg(_, _) => 0,
                    _ => unreachable!(),
                };
                let (shadows, rects) = prefix.split_at(shadow_count);
                let shadows = shadows
                    .iter()
                    .map(|op| match op {
                        PaintOp::PreparedShadow(shadow) => Some(shadow),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                let rects = rects
                    .iter()
                    .map(|op| match op {
                        PaintOp::DrawRect(rect) => Some(rect),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                (rects.len() == expected_rects.len()).then(|| {
                    Self::svg_with_shadows_and_decoration(
                        PreparedSvgIdentity::from_op(svg)?,
                        shadows,
                        rects,
                    )
                })?
            }
        }
    }

    pub(crate) fn prepared_text_selection<'a>(
        start_char: usize,
        end_char: usize,
        color_rgba_bits: [u32; 4],
        rects: impl IntoIterator<Item = &'a DrawRectOp>,
    ) -> Option<Self> {
        let rects = Self::draw_rect_identities(rects)?;
        let selection = TextSelectionPayloadIdentity {
            start_char,
            end_char,
            color_rgba_bits,
            rects,
        };
        selection
            .is_canonical()
            .then_some(Self::TextSelection(selection))
    }

    /// Canonical ordered identity for generic rectangle-only paint phases.
    /// Cardinality and role-specific grammar remain compiler authority.
    pub(crate) fn prepared_rects<'a>(
        rects: impl IntoIterator<Item = &'a DrawRectOp>,
    ) -> Option<Self> {
        Some(Self::PreparedRects(Self::draw_rect_identities(rects)?))
    }

    pub(crate) fn prepared_shadows<'a>(
        shadows: impl IntoIterator<Item = &'a PreparedShadowOp>,
    ) -> Self {
        Self::PreparedShadows(
            shadows
                .into_iter()
                .map(PreparedShadowOp::frozen_identity)
                .collect::<Vec<_>>()
                .into(),
            Arc::from([]),
        )
    }

    pub(crate) fn prepared_scrollbar_overlay(op: &PreparedScrollbarOverlayOp) -> Self {
        Self::PreparedScrollbarOverlay(Arc::new(op.frozen_identity()))
    }

    pub(crate) fn prepared_shadows_with_decoration<'a, 'b>(
        shadows: impl IntoIterator<Item = &'a PreparedShadowOp>,
        decoration: impl IntoIterator<Item = &'b DrawRectOp>,
    ) -> Option<Self> {
        Some(Self::PreparedShadows(
            shadows
                .into_iter()
                .map(PreparedShadowOp::frozen_identity)
                .collect::<Vec<_>>()
                .into(),
            Self::draw_rect_identities(decoration)?,
        ))
    }

    pub(crate) fn image_with_shadows_and_decoration<'a, 'b>(
        image: PreparedImageIdentity,
        shadows: impl IntoIterator<Item = &'a PreparedShadowOp>,
        decoration: impl IntoIterator<Item = &'b DrawRectOp>,
    ) -> Option<Self> {
        let shadows = shadows
            .into_iter()
            .map(PreparedShadowOp::frozen_identity)
            .collect::<Vec<_>>();
        let decoration = Self::draw_rect_identities(decoration)?;
        if shadows.is_empty() {
            Some(Self::Image(image, decoration))
        } else {
            Some(Self::ImageWithShadows(image, shadows.into(), decoration))
        }
    }

    pub(crate) fn svg_with_shadows_and_decoration<'a, 'b>(
        svg: PreparedSvgIdentity,
        shadows: impl IntoIterator<Item = &'a PreparedShadowOp>,
        decoration: impl IntoIterator<Item = &'b DrawRectOp>,
    ) -> Option<Self> {
        let shadows = shadows
            .into_iter()
            .map(PreparedShadowOp::frozen_identity)
            .collect::<Vec<_>>();
        let decoration = Self::draw_rect_identities(decoration)?;
        if shadows.is_empty() {
            Some(Self::Svg(svg, decoration))
        } else {
            Some(Self::SvgWithShadows(svg, shadows.into(), decoration))
        }
    }

    fn draw_rect_identities<'a>(
        decoration: impl IntoIterator<Item = &'a DrawRectOp>,
    ) -> Option<Arc<[PreparedDrawRectIdentity]>> {
        Some(
            decoration
                .into_iter()
                .map(PreparedDrawRectIdentity::from_op)
                .collect::<Option<Vec<_>>>()?
                .into(),
        )
    }

    pub(crate) fn prepared_texts<'a>(texts: impl IntoIterator<Item = &'a PreparedTextOp>) -> Self {
        Self::PreparedTexts(
            texts
                .into_iter()
                .map(PreparedTextOp::frozen_identity)
                .collect::<Vec<_>>()
                .into(),
        )
    }

    pub(crate) fn inline_ifc_decorations_with_shadows<'a, 'b>(
        shadows: impl IntoIterator<Item = &'a PreparedShadowOp>,
        decorations: impl IntoIterator<Item = &'b PreparedInlineIfcDecorationOp>,
    ) -> Self {
        Self::InlineIfcDecorations(
            shadows
                .into_iter()
                .map(PreparedShadowOp::frozen_identity)
                .collect::<Vec<_>>()
                .into(),
            decorations
                .into_iter()
                .map(PreparedInlineIfcDecorationOp::frozen_identity)
                .collect::<Vec<_>>()
                .into(),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedTextIdentity {
    scale_factor_bits: u32,
    glyphs: Arc<[PreparedTextGlyphIdentity]>,
    fragments: Arc<[PreparedTextFragmentIdentity]>,
    scissor_rect: Option<[u32; 4]>,
    stencil_clip_id: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedTextGlyphIdentity {
    glyph_id: u32,
    font_size_bits: u32,
    font_data_id: u64,
    font_index: u32,
    normalized_coords_hash: u64,
    local_pos_bits: [u32; 2],
    color_bits: [u32; 4],
    opacity_bits: u32,
    fragment_index: u32,
    final_paint_pos_bits: [u32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PreparedTextFragmentIdentity {
    origin_bits: [u32; 2],
    size_bits: [u32; 2],
}

impl PreparedTextFragmentIdentity {
    fn from_fragment(fragment: &TextPassPreparedFragment) -> Option<Self> {
        if fragment.origin.iter().any(|value| !value.is_finite())
            || fragment
                .size
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.0)
        {
            return None;
        }
        Some(PreparedTextFragmentIdentity {
            origin_bits: fragment.origin.map(f32::to_bits),
            size_bits: fragment.size.map(f32::to_bits),
        })
    }
}

impl PreparedTextGlyphIdentity {
    fn from_glyph(
        glyph: &TextPassPreparedStagingGlyphInput,
        fragments: &[TextPassPreparedFragment],
    ) -> Option<Self> {
        let font_data = glyph.raster.font_data.as_ref()?;
        if font_data.data.id() != glyph.raster.font_data_id
            || font_data.index != glyph.raster.font_index
            || glyph.raster.glyph_id > u16::MAX as u32
            || !glyph.raster.font_size.is_finite()
            || glyph.raster.font_size <= 0.0
            || glyph.paint.fragment_index as usize >= fragments.len()
            || glyph.paint.local_pos.iter().any(|value| !value.is_finite())
            || glyph
                .paint
                .color
                .iter()
                .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
            || !glyph.paint.opacity.is_finite()
            || !(0.0..=1.0).contains(&glyph.paint.opacity)
            || glyph.final_paint_pos.iter().any(|value| !value.is_finite())
        {
            return None;
        }
        let fragment = fragments[glyph.paint.fragment_index as usize];
        let expected_final = [
            fragment.origin[0] + glyph.paint.local_pos[0],
            fragment.origin[1] + glyph.paint.local_pos[1],
        ];
        if glyph.final_paint_pos.map(f32::to_bits) != expected_final.map(f32::to_bits) {
            return None;
        }
        Some(PreparedTextGlyphIdentity {
            glyph_id: glyph.raster.glyph_id,
            font_size_bits: glyph.raster.font_size.to_bits(),
            font_data_id: glyph.raster.font_data_id,
            font_index: glyph.raster.font_index,
            normalized_coords_hash: glyph.raster.normalized_coords_hash,
            local_pos_bits: glyph.paint.local_pos.map(f32::to_bits),
            color_bits: glyph.paint.color.map(f32::to_bits),
            opacity_bits: glyph.paint.opacity.to_bits(),
            fragment_index: glyph.paint.fragment_index,
            final_paint_pos_bits: glyph.final_paint_pos.map(f32::to_bits),
        })
    }
}

impl PreparedTextIdentity {
    fn valid_header(
        scale_factor: f32,
        glyph_count: usize,
        fragments: &[TextPassPreparedFragment],
        scissor_rect: Option<[u32; 4]>,
    ) -> bool {
        scale_factor.is_finite()
            && scale_factor > 0.0
            && glyph_count != 0
            && !fragments.is_empty()
            && !scissor_rect.is_some_and(|[_, _, width, height]| width == 0 || height == 0)
    }

    fn from_params(params: &TextPassPreparedParams) -> Option<Self> {
        if !Self::valid_header(
            params.staging_input.scale_factor,
            params.staging_input.glyphs.len(),
            &params.fragments,
            params.scissor_rect,
        ) {
            return None;
        }
        let fragments = params
            .fragments
            .iter()
            .map(PreparedTextFragmentIdentity::from_fragment)
            .collect::<Option<Vec<_>>>()?;
        // The input length is exact. Avoid repeated growth through an
        // Option-collect adapter when preparing a long text run.
        let mut glyphs = Vec::with_capacity(params.staging_input.glyphs.len());
        for glyph in &params.staging_input.glyphs {
            glyphs.push(PreparedTextGlyphIdentity::from_glyph(
                glyph,
                &params.fragments,
            )?);
        }
        Some(Self {
            scale_factor_bits: params.staging_input.scale_factor.to_bits(),
            glyphs: glyphs.into(),
            fragments: fragments.into(),
            scissor_rect: params.scissor_rect,
            stencil_clip_id: params.stencil_clip_id,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PreparedImageIdentity {
    pub(crate) sampled_texture_id: SampledTextureId,
    pub(crate) generation: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) alpha_mode: SampledTextureAlphaMode,
    pub(crate) sampling: ImageSampling,
    pub(crate) pixel_len: usize,
    /// Identity of the immutable, frame-frozen pixel allocation. Together
    /// with generation, dimensions, and length this detects upload Arc
    /// replacement between metadata and full recording.
    pub(crate) pixel_ptr: usize,
    pub(crate) bounds_bits: [u32; 4],
    pub(crate) uv_bounds_bits: Option<[u32; 4]>,
    pub(crate) opacity_bits: u32,
    pub(crate) source_is_premultiplied: bool,
}

impl PreparedImageIdentity {
    pub(crate) fn from_op(op: &PreparedImageOp) -> Self {
        Self {
            sampled_texture_id: op.upload.id,
            generation: op.upload.generation,
            width: op.upload.width,
            height: op.upload.height,
            format: op.upload.format,
            alpha_mode: op.upload.alpha_mode,
            sampling: op.upload.sampling,
            pixel_len: op.upload.pixels.len(),
            pixel_ptr: op.upload.pixels.as_ptr() as usize,
            bounds_bits: op.params.bounds.map(f32::to_bits),
            uv_bounds_bits: op.params.uv_bounds.map(|bounds| bounds.map(f32::to_bits)),
            opacity_bits: op.params.opacity.to_bits(),
            source_is_premultiplied: op.params.source_is_premultiplied,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PreparedSvgIdentity {
    pub(crate) svg_raster_asset_id: SvgRasterAssetId,
    pub(crate) generation: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) alpha_mode: SampledTextureAlphaMode,
    pub(crate) sampling: ImageSampling,
    pub(crate) pixel_len: usize,
    /// Identity of the immutable, frame-frozen raster allocation.
    pub(crate) pixel_ptr: usize,
    pub(crate) bounds_bits: [u32; 4],
    pub(crate) uv_bounds_bits: Option<[u32; 4]>,
    pub(crate) opacity_bits: u32,
    pub(crate) source_is_premultiplied: bool,
}

impl PreparedSvgIdentity {
    pub(crate) fn from_op(op: &PreparedSvgOp) -> Option<Self> {
        let SampledTextureId::SvgRaster(svg_raster_asset_id) = op.upload.id else {
            return None;
        };
        Some(Self {
            svg_raster_asset_id,
            generation: op.upload.generation,
            width: op.upload.width,
            height: op.upload.height,
            format: op.upload.format,
            alpha_mode: op.upload.alpha_mode,
            sampling: op.upload.sampling,
            pixel_len: op.upload.pixels.len(),
            pixel_ptr: op.upload.pixels.as_ptr() as usize,
            bounds_bits: op.params.bounds.map(f32::to_bits),
            uv_bounds_bits: op.params.uv_bounds.map(|bounds| bounds.map(f32::to_bits)),
            opacity_bits: op.params.opacity.to_bits(),
            source_is_premultiplied: op.params.source_is_premultiplied,
        })
    }
}

mod layout_frame;
pub(crate) use layout_frame::{artifact_plan_in_layout_frame, metadata_plan_in_layout_frame};

#[cfg(test)]
mod prepared_text_identity_tests;

#[derive(Clone, Debug)]
pub(crate) struct PreparedGpuOp {
    pub(crate) params: TextureCompositeParams,
    pub(crate) source: crate::view::gpu_paint::GpuPaintSource,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedGpuIdentity {
    source: crate::view::gpu_paint::GpuPaintSource,
    bounds: [u32; 4],
    opacity: u32,
}
impl PreparedGpuOp {
    pub(crate) fn identity(&self) -> Option<PreparedGpuIdentity> {
        let p = &self.params;
        (p.bounds.iter().all(|v| v.is_finite())
            && p.bounds[2] > 0.0
            && p.bounds[3] > 0.0
            && p.opacity.is_finite()
            && (0.0..=1.0).contains(&p.opacity)
            && p.source_is_premultiplied
            && !p.use_mask
            && p.quad_positions.is_none()
            && p.uv_bounds.is_none()
            && p.mask_uv_bounds.is_none()
            && p.scissor_rect.is_none())
        .then(|| PreparedGpuIdentity {
            source: self.source.clone(),
            bounds: p.bounds.map(f32::to_bits),
            opacity: p.opacity.to_bits(),
        })
    }
}

mod identity_matches;
