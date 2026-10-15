use crate::view::base_component::{
    Rect, UiBuildContext, exact_logical_scissor_for_rect, logical_scissor_for_clip_rect,
};
use crate::view::compositor::property_tree::{
    ClipBehavior, ClipGeometry, ClipNodeId, ClipNodeRole, ClipNodeSnapshot, EffectNodeId,
    EffectNodeSnapshot, PropertyTreeState, ScrollNodeId, ScrollNodeSnapshot,
    SpatialProjectionError, TransformNodeId, TransformNodeSnapshot,
};
use crate::view::frame_graph::FrameGraph;
use crate::view::node_arena::NodeKey;
use crate::view::render_pass::TextureCompositePass;
use crate::view::render_pass::draw_rect_pass::{DrawRectInput, DrawRectOutput, DrawRectPass};
use crate::view::render_pass::render_target::GraphicsPassScissor;
use crate::view::render_pass::text_pass::{TextInput, TextOutput, TextPreparedInputPass};
use crate::view::render_pass::texture_composite_pass::{
    TextureCompositeInput, TextureCompositeOutput,
};
use crate::view::render_pass::{ShadowModuleSpec, build_shadow_module};
use rustc_hash::{FxHashMap, FxHashSet};
use std::ops::Range;

use super::artifact::PaintChunkRasterIdentity;

use super::surface_dag::{
    ArtifactSurfaceCoverageForest, ArtifactSurfaceCoverageSpan, ArtifactSurfaceCoverageStep,
    LayerizationPolicy, SurfaceDag, SurfaceDagClipClosureProjection, SurfaceDagError,
    SurfaceDagExecutionNodeId, SurfaceDagExecutionOrder, SurfaceDagExecutionTargetId,
    SurfaceDagNodeId, SurfaceDagNodeKind,
};
use super::{
    PaintArtifact, PaintChunkRole, PaintContentRevision, PaintOp, PaintOwnerSnapshot,
    PaintPayloadIdentity, PaintPropertyScope, PreparedShadowOp, PreparedTextOp, TransitionError,
};
#[cfg(test)]
use super::{PreparedImageIdentity, PreparedSvgIdentity};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Fully evaluated clip outcome sealed before graph emission. The executor is
/// never given a clip-id map and therefore cannot introduce late rejection.
pub(crate) enum ResolvedClip {
    Unclipped,
    Scissor([u32; 4]),
    Empty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArtifactSurfaceResolvedClip {
    Unclipped,
    Scissor(GraphicsPassScissor),
    Empty,
}

impl ArtifactSurfaceResolvedClip {
    fn from_logical(clip: ResolvedClip) -> Self {
        match clip {
            ResolvedClip::Unclipped => Self::Unclipped,
            ResolvedClip::Scissor(scissor) => Self::Scissor(GraphicsPassScissor::Logical(scissor)),
            ResolvedClip::Empty => Self::Empty,
        }
    }

    fn project_for_surface_receiver(
        self,
        projection: ArtifactSurfaceRasterOriginProjection,
    ) -> Option<Self> {
        match self {
            Self::Unclipped | Self::Empty => Some(self),
            Self::Scissor(GraphicsPassScissor::Logical(scissor)) => {
                Some(projection.project_clip(ResolvedClip::Scissor(scissor)))
            }
            Self::Scissor(GraphicsPassScissor::TargetPhysical(_)) => None,
        }
    }
}

mod artifact_surface_executor;
mod planning_cache;
mod span_seal_cache;
pub(crate) use artifact_surface_executor::emit_prepared_artifact_surface_frame_from_pool;
#[cfg(any(test, feature = "renderer-test-support"))]
pub(crate) use artifact_surface_executor::take_last_production_actions_for_test;
pub(crate) use planning_cache::PlanningCache;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RetainedSurfaceResidentKey {
    Surface {
        boundary_root: crate::view::node_arena::NodeKey,
        stable_id: u64,
        role: RetainedSurfaceRasterRole,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RetainedSurfaceRasterRole {
    Transform,
    PropertyEffect,
    /// Offset-zero content; placement and receiver clips are composition state.
    ScrollContent,
}

/// Stable resident identity only. Transform matrices and generations, layout
/// or receiver-space position, scroll offset, and transition visual offset are
/// composition state and must not be added here. Surface-local raster geometry
/// belongs to [`RetainedSurfaceRasterInputs`] or the sealed raster stamp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetainedSurfaceRasterIdentity {
    pub(crate) boundary_root: crate::view::node_arena::NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) color_key: crate::view::frame_graph::PersistentTextureKey,
    pub(crate) role: RetainedSurfaceRasterRole,
}

impl RetainedSurfaceRasterIdentity {
    pub(crate) fn resident_key(self) -> RetainedSurfaceResidentKey {
        RetainedSurfaceResidentKey::Surface {
            boundary_root: self.boundary_root,
            stable_id: self.stable_id,
            role: self.role,
        }
    }
    fn artifact_surface_resident_key(self) -> RetainedSurfaceResidentKey {
        self.resident_key()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedSurfaceRasterInputs {
    pub(crate) color: crate::view::frame_graph::TextureDesc,
    pub(crate) depth: crate::view::frame_graph::TextureDesc,
    pub(crate) scale_factor_bits: u32,
    pub(crate) source_bounds_bits: [u32; 4],
}

impl RetainedSurfaceRasterInputs {
    pub(crate) fn has_canonical_descriptor_pair_for(
        &self,
        identity: RetainedSurfaceRasterIdentity,
    ) -> bool {
        let scale = f32::from_bits(self.scale_factor_bits);
        let [x, y, width, height] = self.source_bounds_bits.map(f32::from_bits);
        if !scale.is_finite()
            || scale <= 0.0
            || ![x, y, width, height].iter().all(|value| value.is_finite())
            || x < 0.0
            || y < 0.0
            || width <= 0.0
            || height <= 0.0
            || identity.stable_id == 0
            || identity.color_key
                != match identity.role {
                    RetainedSurfaceRasterRole::Transform => {
                        crate::view::base_component::transformed_layer_stable_key(
                            identity.stable_id,
                        )
                    }
                    RetainedSurfaceRasterRole::PropertyEffect => {
                        crate::view::base_component::isolation_layer_stable_key(identity.stable_id)
                    }
                    RetainedSurfaceRasterRole::ScrollContent => {
                        crate::view::base_component::scroll_content_layer_stable_key(
                            identity.stable_id,
                        )
                    }
                }
        {
            return false;
        }
        let expected_color = crate::view::base_component::texture_desc_for_logical_bounds(
            crate::view::base_component::RetainedSurfaceBounds {
                x,
                y,
                width,
                height,
                corner_radii: [0.0; 4],
            },
            scale,
            None,
            self.color.format(),
        )
        .with_raster_scale(scale);
        let (expected_color, expected_depth) =
            crate::view::base_component::persistent_target_texture_descriptors(
                expected_color,
                identity.color_key,
            );
        self.color == expected_color && self.depth == expected_depth
    }
}

/// Which authority produced a detached scroll-content raster's local clip
/// generations. The discriminator belongs to the complete raster stamp, not
/// [`RetainedSurfaceRasterIdentity`], so changing authority keeps the same
/// resident allocation key while forcing a raster refresh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LocalClipGenerationSemantics {
    ArtifactLive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedSurfaceRasterStamp {
    pub(crate) identity: RetainedSurfaceRasterIdentity,
    pub(crate) target: RetainedSurfaceRasterInputs,
    pub(crate) owner_topology: Vec<PaintOwnerSnapshot>,
    pub(crate) clip_nodes: Vec<ClipNodeSnapshot>,
    pub(crate) chunks: Vec<RetainedSurfaceChunkStamp>,
    pub(crate) op_count: usize,
    pub(crate) opaque_order_span: Range<u32>,
    local_clip_generation_semantics: Option<LocalClipGenerationSemantics>,
    /// Only the generic sealer can mint this complete raster program.
    /// Own placement is excluded so composition-only changes reuse pixels.
    artifact_surface_program: Option<ArtifactSurfaceRasterProgramStamp>,
}

impl RetainedSurfaceRasterStamp {
    #[cfg(test)]
    pub(crate) fn has_artifact_surface_program_for_test(&self) -> bool {
        self.artifact_surface_program.is_some()
    }

    #[cfg(test)]
    pub(crate) fn artifact_surface_program_step_names_for_test(&self) -> Option<Vec<&'static str>> {
        self.artifact_surface_program.as_ref().map(|program| {
            program
                .steps
                .iter()
                .map(|step| match step {
                    ArtifactSurfaceRasterProgramStepStamp::ArtifactSpan(_) => "artifact-span",
                    ArtifactSurfaceRasterProgramStepStamp::NestedSurface(_) => "nested-surface",
                })
                .collect()
        })
    }

    #[cfg(test)]
    pub(crate) fn artifact_surface_program_resolved_clips_for_test(
        &self,
    ) -> Option<Vec<ArtifactSurfaceResolvedClip>> {
        self.artifact_surface_program.as_ref().map(|program| {
            program
                .steps
                .iter()
                .flat_map(|step| match step {
                    ArtifactSurfaceRasterProgramStepStamp::ArtifactSpan(span) => span
                        .chunks
                        .iter()
                        .map(|chunk| chunk.clip_schedule.terminal_clip())
                        .collect(),
                    ArtifactSurfaceRasterProgramStepStamp::NestedSurface(_) => Vec::new(),
                })
                .collect()
        })
    }

    #[cfg(test)]
    pub(crate) fn local_clip_generation_semantics_name_for_test(&self) -> Option<&'static str> {
        self.local_clip_generation_semantics
            .map(|semantics| match semantics {
                LocalClipGenerationSemantics::ArtifactLive => "artifact-live",
            })
    }
}

/// Private two-step artifact raster program stored inside the one retained
/// resident stamp type. A surface's own composite geometry is deliberately
/// absent: top-level placement is a sealed plan fact, while surface-receiver
/// placement belongs to the parent's nested dependency.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ArtifactSurfaceRasterProgramStamp {
    window: Option<raster_window::RasterWindow>,
    execution_id: SurfaceDagExecutionNodeId,
    source: SurfaceDagNodeId,
    receiver: SurfaceDagExecutionTargetId,
    steps: Vec<ArtifactSurfaceRasterProgramStepStamp>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ArtifactSurfaceRasterProgramStepStamp {
    ArtifactSpan(span_seal_cache::SealedProgramSpan),
    NestedSurface(ArtifactSurfaceNestedRasterDependency),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ArtifactSurfaceRasterProgramSpanStamp {
    step_index: usize,
    owner_topology: Vec<PaintOwnerSnapshot>,
    clip_nodes: Vec<ClipNodeSnapshot>,
    /// Each chunk structurally owns its clip schedule and actual opaque cursor
    /// advance. This supersedes C3b3a's index-aligned `resolved_clips` and
    /// `opaque_order_counts` vectors: alignment can no longer drift.
    chunks: Vec<ArtifactSurfaceRasterProgramChunkStamp>,
    op_count: usize,
    opaque_order_span: Range<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ArtifactSurfaceRasterProgramChunkStamp {
    raster: RetainedSurfaceChunkStamp,
    clip_schedule: ArtifactSurfaceChunkClipSchedule,
    opaque_order_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ArtifactSurfaceNestedRasterDependency {
    step_index: usize,
    child_execution_id: SurfaceDagExecutionNodeId,
    child_stamp: std::sync::Arc<RetainedSurfaceRasterStamp>,
    /// Composite placement is an edge property. Keeping it on the receiver's
    /// dependency invalidates that receiver when a nested child moves without
    /// invalidating the child's unchanged raster content.
    child_composite_geometry: ArtifactSurfaceCompositeGeometryStamp,
    parent_opaque_order_before: u32,
    parent_opaque_order_after: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedSurfaceChunkStamp {
    pub(crate) id: super::PaintChunkId,
    pub(crate) owner: crate::view::node_arena::NodeKey,
    pub(crate) bounds_bits: [u32; 4],
    pub(crate) clip: Option<ClipNodeId>,
    pub(crate) topology_revision: u64,
    pub(crate) payload_identity: PaintPayloadIdentity,
    pub(crate) op_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetainedSurfaceCompileAction {
    Reraster,
    Reuse,
    /// Drawn into a frame-local target and not kept: the surface cache could
    /// not hold it this frame.
    Transient,
}

/// Minimal host facts required to seal detached raster descriptors and final
/// composite placement. Unlike artifact-derived capability tokens, this value
/// carries no claimed derivation result: every field has an independently
/// checkable invariant, so value validation is sufficient and provenance
/// gating would add no protection. Frame tokens, texture handles, persistent
/// pool state, and mutable targets are deliberately absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ArtifactSurfaceRasterContext {
    scale_factor_bits: u32,
    target_format: wgpu::TextureFormat,
    paint_offset_bits: [u32; 2],
    incoming_scissor: Option<[u32; 4]>,
    /// Physical size of the scene-root target. Nothing outside it is visible.
    target_size: [u32; 2],
    max_texture_dimension_2d: u32,
    max_texture_bytes: u64,
}

impl ArtifactSurfaceRasterContext {
    pub(crate) fn new(
        scale_factor: f32,
        target_format: wgpu::TextureFormat,
        paint_offset: [f32; 2],
        incoming_scissor: Option<[u32; 4]>,
        target_size: [u32; 2],
        max_texture_dimension_2d: u32,
        max_texture_bytes: u64,
    ) -> Option<Self> {
        if !scale_factor.is_finite()
            || scale_factor <= 0.0
            || paint_offset.into_iter().any(|value| !value.is_finite())
            || incoming_scissor.is_some_and(|[x, y, width, height]| {
                width == 0
                    || height == 0
                    || x.checked_add(width).is_none()
                    || y.checked_add(height).is_none()
            })
            || target_size.contains(&0)
            || max_texture_dimension_2d == 0
            || max_texture_bytes == 0
        {
            return None;
        }
        Some(Self {
            scale_factor_bits: scale_factor.to_bits(),
            target_format,
            paint_offset_bits: paint_offset.map(f32::to_bits),
            incoming_scissor,
            target_size,
            max_texture_dimension_2d,
            max_texture_bytes,
        })
    }

    fn scale_factor(self) -> f32 {
        f32::from_bits(self.scale_factor_bits)
    }

    /// The same facts for a raster drawn at `scale_factor`.
    fn with_scale_factor(self, scale_factor: f32) -> Self {
        Self {
            scale_factor_bits: scale_factor.to_bits(),
            ..self
        }
    }

    /// The scene-root target in logical viewport space.
    fn target_logical_rect(self) -> [f32; 4] {
        let scale = self.scale_factor();
        [
            0.0,
            0.0,
            self.target_size[0] as f32 / scale,
            self.target_size[1] as f32 / scale,
        ]
    }

    fn paint_offset(self) -> [f32; 2] {
        self.paint_offset_bits.map(f32::from_bits)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArtifactSurfaceRasterTargetId {
    SceneRoot(super::SurfaceDagSceneRootId),
    Surface(SurfaceDagNodeId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArtifactSurfaceRasterPlanError {
    ArtifactProgram(SingleTargetSurfaceDagPrepareError),
    SurfaceDag(SurfaceDagError),
    MissingSurfaceSnapshot(SurfaceDagNodeId),
    MissingCoverageNode(SurfaceDagNodeId),
    InvalidSurfaceBounds(SurfaceDagNodeId),
    InvalidRasterOrigin(SurfaceDagNodeId),
    InvalidCoverageSpan(ArtifactSurfaceRasterTargetId),
    InvalidOwnerTopology {
        target: ArtifactSurfaceRasterTargetId,
        owner: NodeKey,
    },
    InvalidChunkBounds {
        target: ArtifactSurfaceRasterTargetId,
        chunk_index: usize,
    },
    InvalidResolvedClip {
        target: ArtifactSurfaceRasterTargetId,
        chunk_index: usize,
    },
    InvalidReceiverClip(SurfaceDagNodeId),

    Localization {
        target: ArtifactSurfaceRasterTargetId,
        chunk_index: usize,
        op_index: usize,
        reason: ArtifactSurfaceLocalizationError,
    },
    LocalizedPayload {
        target: ArtifactSurfaceRasterTargetId,
        chunk_index: usize,
    },
    InvalidNestedSurface {
        parent: SurfaceDagNodeId,
        child: SurfaceDagNodeId,
    },
}

impl ArtifactSurfaceRasterPlanError {
    /// The node whose paint the plan could not use, when the error names
    /// one: the frame can be recorded again without it.
    pub(crate) fn owner(self) -> Option<NodeKey> {
        match self {
            Self::ArtifactProgram(SingleTargetSurfaceDagPrepareError::UnresolvedClip(owner))
            | Self::InvalidOwnerTopology { owner, .. } => Some(owner),
            Self::ArtifactProgram(SingleTargetSurfaceDagPrepareError::SurfaceDag(error))
            | Self::SurfaceDag(error) => error.owner(),
            Self::ArtifactProgram(SingleTargetSurfaceDagPrepareError::ChunkPlacement)
            | Self::MissingSurfaceSnapshot(_)
            | Self::MissingCoverageNode(_)
            | Self::InvalidSurfaceBounds(_)
            | Self::InvalidRasterOrigin(_)
            | Self::InvalidCoverageSpan(_)
            | Self::InvalidChunkBounds { .. }
            | Self::InvalidResolvedClip { .. }
            | Self::InvalidReceiverClip(_)
            | Self::Localization { .. }
            | Self::LocalizedPayload { .. }
            | Self::InvalidNestedSurface { .. } => None,
        }
    }
}

impl From<SurfaceDagError> for ArtifactSurfaceRasterPlanError {
    fn from(error: SurfaceDagError) -> Self {
        Self::SurfaceDag(error)
    }
}

/// Pure artifact-stage program. This is the only input accepted by the later
/// graph-inert raster descriptor stage; it contains no viewport facts.
#[derive(Clone, Debug)]
struct ArtifactSurfaceDagProgram {
    artifact: PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
    /// Each owner-local clip's rect before intake placed it in viewport
    /// space, where scissors cannot go negative.
    owner_local_clips: FxHashMap<ClipNodeId, Rect>,
    surface_dag: std::sync::Arc<SurfaceDag>,
    execution_order: std::sync::Arc<SurfaceDagExecutionOrder>,
    coverage: std::sync::Arc<ArtifactSurfaceCoverageForest>,
    spatial: std::sync::Arc<super::ArtifactSpatialProjection>,
    host_placement: std::sync::Arc<ArtifactSurfaceHostPlacementProjection>,
}

impl ArtifactSurfaceDagProgram {
    /// `clip` in a raster `translation` away from viewport space, such as
    /// scroll content. Owner-local clips are placed again from their
    /// unclamped rects; a scissor clamped to the viewport edge would change
    /// with every scroll offset. Viewport clips move their scissor.
    fn raster_clip(
        &self,
        clip: &ClipNodeSnapshot,
        translation: [f32; 2],
    ) -> Option<ClipNodeSnapshot> {
        let rect = match self.owner_local_clips.get(&clip.id) {
            Some(rect) => {
                let origin = self.spatial.frame_origin(clip.owner)?;
                Rect {
                    x: origin[0] + rect.x,
                    y: origin[1] + rect.y,
                    ..*rect
                }
            }
            None => {
                let [x, y, width, height] = clip.geometry.viewport_scissor()?;
                Rect {
                    x: x as f32,
                    y: y as f32,
                    width: width as f32,
                    height: height as f32,
                }
            }
        };
        let scissor = logical_scissor_for_clip_rect(Rect {
            x: rect.x + translation[0],
            y: rect.y + translation[1],
            ..rect
        })?;
        Some(ClipNodeSnapshot {
            geometry: ClipGeometry::Viewport(scissor),
            ..*clip
        })
    }
}

/// Sealed per-chunk scissor program for the future artifact executor.
///
/// A child-mask chunk remains a stencil program and is deliberately not
/// represented here: stencil increment/decrement cannot be reduced to the
/// `Unclipped` / `Scissor` / `Empty` scissor taxonomy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArtifactSurfaceChunkClipSchedule {
    WholeChunk(ArtifactSurfaceResolvedClip),
    /// The exact admitted self-clip shadow grammar paints its outer-shadow
    /// prefix against the incoming scissor, then applies the owner's Replace
    /// clip to decoration/media. Sealing the split prevents the executor from
    /// re-consulting `exact_self_clip_shadow_prefix_len` during emission.
    AfterShadowPrefix {
        prefix_op_count: usize,
        suffix_clip: ArtifactSurfaceResolvedClip,
    },
}

impl ArtifactSurfaceChunkClipSchedule {
    #[cfg(test)]
    fn terminal_clip(self) -> ArtifactSurfaceResolvedClip {
        match self {
            Self::WholeChunk(clip) => clip,
            Self::AfterShadowPrefix { suffix_clip, .. } => suffix_clip,
        }
    }

    #[cfg(test)]
    fn parts_for_test(self) -> (Option<usize>, ArtifactSurfaceResolvedClip) {
        match self {
            Self::WholeChunk(clip) => (None, clip),
            Self::AfterShadowPrefix {
                prefix_op_count,
                suffix_clip,
            } => (Some(prefix_op_count), suffix_clip),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedArtifactSurfaceRasterChunk {
    source: PaintChunkRasterIdentity,
    /// Copied from `PaintChunk::content_revision` before the artifact is
    /// consumed. These are artifact facts, not new host inputs.
    content_revision: PaintContentRevision,
    localized_bounds_bits: [u32; 4],
    localized_state: PropertyTreeState,
    localized_payload: PaintPayloadIdentity,
    localized_ops: std::sync::Arc<[PaintOp]>,
    /// Embedded rather than index-aligned beside the chunk, so preparation
    /// cannot seal a clip schedule for a different chunk.
    clip_schedule: ArtifactSurfaceChunkClipSchedule,
}

impl PreparedArtifactSurfaceRasterChunk {
    pub(crate) fn source(&self) -> &PaintChunkRasterIdentity {
        &self.source
    }

    pub(crate) fn localized_bounds_bits(&self) -> [u32; 4] {
        self.localized_bounds_bits
    }

    pub(crate) fn localized_ops(&self) -> &[PaintOp] {
        &self.localized_ops
    }

    #[cfg(test)]
    pub(crate) fn clip_schedule_for_test(&self) -> (Option<usize>, ArtifactSurfaceResolvedClip) {
        self.clip_schedule.parts_for_test()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedArtifactSurfaceRasterSpan {
    seal_cache: std::sync::Arc<span_seal_cache::SpanSealCache>,
    // Source range retained only for regression-test inspection.
    #[cfg(test)]
    op_range: Range<usize>,
    owner_topology: Vec<PaintOwnerSnapshot>,
    opaque_order_count: u32,
    /// The single-construction closure emitted by the shared Surface DAG walk,
    /// placed in the raster's space; preparation may resolve through it but
    /// never recomputes or merges it.
    local_clips: Vec<ClipNodeSnapshot>,
    chunks: std::sync::Arc<[PreparedArtifactSurfaceRasterChunk]>,
}

impl PreparedArtifactSurfaceRasterSpan {
    pub(crate) fn opaque_order_count(&self) -> u32 {
        self.opaque_order_count
    }

    pub(crate) fn chunks(&self) -> &[PreparedArtifactSurfaceRasterChunk] {
        &self.chunks
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArtifactSurfaceCompositeGeometryStamp {
    Transform {
        source_bounds_bits: [u32; 4],
        destination_bounds_bits: [u32; 4],
        receiver_transform_bits: [u32; 16],
        /// Projected corners relative to the destination AABB, in texture UV
        /// order. Freeze from the original source coordinates once: moving
        /// raster pixels to a normalized origin must not re-evaluate a matrix
        /// against different coordinates (or change perspective division).
        quad_offset_bits: [[u32; 2]; 4],
        receiver_clip: Option<ClipNodeId>,
        /// Sealed receiver result; the executor must not re-resolve the id.
        resolved_receiver_clip: ArtifactSurfaceResolvedClip,
        /// The matrix has no 2D quad for these bounds: the plane is seen
        /// edge-on, collapsed, or crosses the eye plane. Nothing of it is
        /// composited, whatever its receiver clip.
        culled: bool,
    },
    Effect {
        source_bounds_bits: [u32; 4],
        destination_bounds_bits: [u32; 4],
        opacity_bits: u32,
        generation: u64,
        receiver_clip: Option<ClipNodeId>,
        /// Sealed receiver result; the executor must not re-resolve the id.
        resolved_receiver_clip: ArtifactSurfaceResolvedClip,
    },
    ScrollContent {
        source_bounds_bits: [u32; 4],
        destination_bounds_bits: [u32; 4],
        offset_bits: [u32; 2],
        generation: u64,
        receiver_clip: Option<ClipNodeId>,
        /// Sealed final scrollport result. `receiver_clip` remains the
        /// post-transition closure parent, so the executor must not attempt to
        /// re-resolve that parent as the consumed contents boundary.
        resolved_receiver_clip: ArtifactSurfaceResolvedClip,
    },
}

impl ArtifactSurfaceCompositeGeometryStamp {
    fn transform_quad(self) -> Option<[[f32; 2]; 4]> {
        let Self::Transform {
            source_bounds_bits,
            destination_bounds_bits,
            receiver_transform_bits,
            quad_offset_bits,
            ..
        } = self
        else {
            return None;
        };
        let source = source_bounds_bits.map(f32::from_bits);
        let destination = destination_bounds_bits.map(f32::from_bits);
        if source
            .into_iter()
            .chain(destination)
            .any(|v| !v.is_finite())
            || source[2] <= 0.0
            || source[3] <= 0.0
            || destination[2] <= 0.0
            || destination[3] <= 0.0
            || receiver_transform_bits
                .into_iter()
                .map(f32::from_bits)
                .any(|v| !v.is_finite())
        {
            return None;
        }
        let offsets = quad_offset_bits.map(|p| p.map(f32::from_bits));
        for axis in 0..2 {
            if offsets
                .iter()
                .any(|p| !p[axis].is_finite() || p[axis] < 0.0)
            {
                return None;
            }
            let min = offsets
                .iter()
                .map(|p| p[axis])
                .fold(f32::INFINITY, f32::min);
            let max = offsets
                .iter()
                .map(|p| p[axis])
                .fold(f32::NEG_INFINITY, f32::max);
            if min != 0.0 || max.to_bits() != destination[axis + 2].to_bits() {
                return None;
            }
        }
        let quad = offsets.map(|p| [p[0] + destination[0], p[1] + destination[1]]);
        quad.iter().flatten().all(|v| v.is_finite()).then_some(quad)
    }

    fn destination_bounds_bits(self) -> [u32; 4] {
        match self {
            Self::Transform {
                destination_bounds_bits,
                ..
            }
            | Self::Effect {
                destination_bounds_bits,
                ..
            }
            | Self::ScrollContent {
                destination_bounds_bits,
                ..
            } => destination_bounds_bits,
        }
    }

    fn receiver_clip(self) -> Option<ClipNodeId> {
        match self {
            Self::Transform { receiver_clip, .. }
            | Self::Effect { receiver_clip, .. }
            | Self::ScrollContent { receiver_clip, .. } => receiver_clip,
        }
    }

    pub(crate) fn resolved_receiver_clip(self) -> ArtifactSurfaceResolvedClip {
        match self {
            Self::Transform { culled: true, .. } => ArtifactSurfaceResolvedClip::Empty,
            Self::Transform {
                resolved_receiver_clip,
                ..
            }
            | Self::Effect {
                resolved_receiver_clip,
                ..
            }
            | Self::ScrollContent {
                resolved_receiver_clip,
                ..
            } => resolved_receiver_clip,
        }
    }

    fn replace_receiver_clip(
        &mut self,
        receiver_clip: Option<ClipNodeId>,
        resolved_receiver_clip: ArtifactSurfaceResolvedClip,
    ) {
        match self {
            Self::Transform {
                receiver_clip: current_receiver_clip,
                resolved_receiver_clip: current_resolved_clip,
                ..
            }
            | Self::Effect {
                receiver_clip: current_receiver_clip,
                resolved_receiver_clip: current_resolved_clip,
                ..
            }
            | Self::ScrollContent {
                receiver_clip: current_receiver_clip,
                resolved_receiver_clip: current_resolved_clip,
                ..
            } => {
                *current_receiver_clip = receiver_clip;
                *current_resolved_clip = resolved_receiver_clip;
            }
        }
    }

    fn project_into_surface_receiver(
        &mut self,
        projection: ArtifactSurfaceRasterOriginProjection,
        receiver_destination_bounds_bits: [u32; 4],
    ) -> Option<()> {
        let (destination_bounds_bits, resolved_receiver_clip) = match self {
            Self::Transform {
                destination_bounds_bits,
                resolved_receiver_clip,
                ..
            }
            | Self::Effect {
                destination_bounds_bits,
                resolved_receiver_clip,
                ..
            }
            | Self::ScrollContent {
                destination_bounds_bits,
                resolved_receiver_clip,
                ..
            } => (destination_bounds_bits, resolved_receiver_clip),
        };
        *destination_bounds_bits =
            projection.project_bounds_bits(receiver_destination_bounds_bits)?;
        *resolved_receiver_clip =
            resolved_receiver_clip.project_for_surface_receiver(projection)?;
        Some(())
    }

    #[cfg(test)]
    fn with_resolved_receiver_clip_for_test(
        self,
        resolved_receiver_clip: ArtifactSurfaceResolvedClip,
    ) -> Self {
        match self {
            Self::Transform {
                source_bounds_bits,
                destination_bounds_bits,
                receiver_transform_bits,
                quad_offset_bits,
                receiver_clip,
                culled,
                ..
            } => Self::Transform {
                source_bounds_bits,
                destination_bounds_bits,
                receiver_transform_bits,
                quad_offset_bits,
                receiver_clip,
                resolved_receiver_clip,
                culled,
            },
            Self::Effect {
                source_bounds_bits,
                destination_bounds_bits,
                opacity_bits,
                generation,
                receiver_clip,
                ..
            } => Self::Effect {
                source_bounds_bits,
                destination_bounds_bits,
                opacity_bits,
                generation,
                receiver_clip,
                resolved_receiver_clip,
            },
            Self::ScrollContent {
                source_bounds_bits,
                destination_bounds_bits,
                offset_bits,
                generation,
                receiver_clip,
                ..
            } => Self::ScrollContent {
                source_bounds_bits,
                destination_bounds_bits,
                offset_bits,
                generation,
                receiver_clip,
                resolved_receiver_clip,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArtifactSurfaceCompositeGeometryState {
    Pending(ArtifactSurfaceCompositeGeometryStamp),
    Finalized(ArtifactSurfaceCompositeGeometryStamp),
}

impl ArtifactSurfaceCompositeGeometryState {
    fn pending(self) -> Option<ArtifactSurfaceCompositeGeometryStamp> {
        match self {
            Self::Pending(geometry) => Some(geometry),
            Self::Finalized(_) => None,
        }
    }

    fn finalize_surface_receiver(
        &mut self,
        projection: ArtifactSurfaceRasterOriginProjection,
        receiver_destination_bounds_bits: [u32; 4],
    ) -> Option<()> {
        let Self::Pending(mut geometry) = *self else {
            return None;
        };
        geometry.project_into_surface_receiver(projection, receiver_destination_bounds_bits)?;
        *self = Self::Finalized(geometry);
        Some(())
    }

    /// Re-resolves a pending child's receiver clip inside its receiver's
    /// raster, with `raster_scissor` giving each clip's scissor in that space.
    /// Only the chain below `boundary` belongs to that raster: the receiver
    /// applies `boundary` and the clips above it when it composites, so the
    /// child keeps the same clip whatever the receiver's transform or scroll.
    fn localize_receiver_clip(
        &mut self,
        composite_clip: Option<ClipNodeId>,
        boundary: ClipNodeId,
        clips: &FxHashMap<ClipNodeId, ClipNodeSnapshot>,
        raster_scissor: impl Fn(&ClipNodeSnapshot) -> Option<[u32; 4]>,
    ) -> Option<()> {
        let Self::Pending(geometry) = self else {
            return None;
        };
        let mut local = Vec::new();
        let mut cursor = composite_clip;
        while let Some(id) = cursor {
            if id == boundary {
                break;
            }
            // A chain longer than the clips it draws from has a cycle.
            if local.len() >= clips.len() {
                return None;
            }
            let snapshot = *clips.get(&id)?;
            local.push(snapshot);
            cursor = snapshot.parent;
        }
        let resolved = local
            .iter()
            .rev()
            .try_fold(ResolvedClip::Unclipped, |clip, snapshot| {
                let scissor = raster_scissor(snapshot)?;
                Some(match snapshot.behavior {
                    ClipBehavior::Replace => resolved_scissor(scissor),
                    ClipBehavior::Intersect => intersect_resolved_clip(clip, scissor),
                })
            })?;
        let receiver_clip = geometry.receiver_clip();
        geometry.replace_receiver_clip(
            receiver_clip,
            ArtifactSurfaceResolvedClip::from_logical(resolved),
        );
        Some(())
    }

    fn finalize_scene_root(&mut self) -> Option<()> {
        let Self::Pending(geometry) = *self else {
            return None;
        };
        *self = Self::Finalized(geometry);
        Some(())
    }

    fn finalized(self) -> Option<ArtifactSurfaceCompositeGeometryStamp> {
        match self {
            Self::Pending(_) => None,
            Self::Finalized(geometry) => Some(geometry),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum PreparedArtifactSurfaceRasterStep {
    ArtifactSpan(PreparedArtifactSurfaceRasterSpan),
    NestedSurface(SurfaceDagExecutionNodeId),
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedArtifactSurfaceRasterNode {
    window: Option<raster_window::RasterWindow>,
    execution_id: SurfaceDagExecutionNodeId,
    source: SurfaceDagNodeId,
    receiver: SurfaceDagExecutionTargetId,
    identity: RetainedSurfaceRasterIdentity,
    target: RetainedSurfaceRasterInputs,
    /// Plan-only placement capability used to seal target-physical child-mask
    /// scissors. It is deliberately absent from the resident stamp.
    raster_origin: ArtifactSurfaceRasterOriginProjection,
    geometry: ArtifactSurfaceCompositeGeometryState,
    clip_closure: Option<SurfaceDagClipClosureProjection>,
    steps: Vec<PreparedArtifactSurfaceRasterStep>,
    /// Whether the surface cache keeps this raster. A surface past the cache
    /// budget is still drawn, into a frame-local target.
    retained: bool,
}

impl PreparedArtifactSurfaceRasterNode {
    #[cfg(test)]
    pub(crate) fn raster_window_bounds_for_test(&self) -> Option<([f32; 4], [f32; 4])> {
        self.window.map(|w| {
            (
                w.content_bounds_bits.map(f32::from_bits),
                w.raster_bounds_bits.map(f32::from_bits),
            )
        })
    }

    pub(crate) fn execution_id(&self) -> SurfaceDagExecutionNodeId {
        self.execution_id
    }

    pub(crate) fn source(&self) -> SurfaceDagNodeId {
        self.source
    }

    pub(crate) fn identity(&self) -> RetainedSurfaceRasterIdentity {
        self.identity
    }

    pub(crate) fn target(&self) -> &RetainedSurfaceRasterInputs {
        &self.target
    }

    pub(crate) fn geometry(&self) -> ArtifactSurfaceCompositeGeometryStamp {
        self.geometry
            .finalized()
            .expect("prepared artifact node geometry is finalized")
    }

    pub(crate) fn steps(&self) -> &[PreparedArtifactSurfaceRasterStep] {
        &self.steps
    }

    pub(crate) fn retained(&self) -> bool {
        self.retained
    }

    #[cfg(test)]
    pub(crate) fn intermediate_readback_observation_for_test(
        &self,
    ) -> Option<(
        crate::view::frame_graph::PersistentTextureKey,
        u32,
        u32,
        [f32; 2],
    )> {
        let (source_bounds_bits, destination_bounds_bits) = match self.geometry() {
            ArtifactSurfaceCompositeGeometryStamp::Effect {
                source_bounds_bits,
                destination_bounds_bits,
                ..
            }
            | ArtifactSurfaceCompositeGeometryStamp::ScrollContent {
                source_bounds_bits,
                destination_bounds_bits,
                ..
            } => (source_bounds_bits, destination_bounds_bits),
            ArtifactSurfaceCompositeGeometryStamp::Transform { .. } => return None,
        };
        Some((
            self.identity.color_key,
            self.target.color.width(),
            self.target.color.height(),
            self.raster_origin
                .composite_source_physical_origin(source_bounds_bits, destination_bounds_bits)?,
        ))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedArtifactSurfaceRasterRoot {
    scene_root: super::SurfaceDagSceneRootId,
    steps: Vec<PreparedArtifactSurfaceRasterStep>,
}

impl PreparedArtifactSurfaceRasterRoot {
    pub(crate) fn scene_root(&self) -> super::SurfaceDagSceneRootId {
        self.scene_root
    }

    pub(crate) fn steps(&self) -> &[PreparedArtifactSurfaceRasterStep] {
        &self.steps
    }
}

/// Graph-inert descriptor and localization seal, consumed by resident sealing
/// before graph execution. Host inputs and logical decisions are retained only
/// for test inspection after preparation has applied them.
#[derive(Clone, Debug)]
pub(crate) struct PreparedArtifactSurfaceRasterPlan {
    #[cfg(test)]
    context: ArtifactSurfaceRasterContext,
    roots: Vec<PreparedArtifactSurfaceRasterRoot>,
    nodes: Vec<PreparedArtifactSurfaceRasterNode>,
    /// GPU sources past the cache budget, produced into frame-local targets.
    transient_gpu_sources: FxHashSet<u64>,
    #[cfg(test)]
    materialization_decisions: Vec<SurfaceMaterializationDecision>,
}

impl PreparedArtifactSurfaceRasterPlan {
    pub(crate) fn roots(&self) -> &[PreparedArtifactSurfaceRasterRoot] {
        &self.roots
    }

    pub(crate) fn nodes(&self) -> &[PreparedArtifactSurfaceRasterNode] {
        &self.nodes
    }

    pub(crate) fn transient_gpu_sources(&self) -> &FxHashSet<u64> {
        &self.transient_gpu_sources
    }

    #[cfg(test)]
    pub(crate) fn force_first_nested_receiver_clip_empty_for_test(
        &mut self,
    ) -> Option<(SurfaceDagExecutionNodeId, SurfaceDagExecutionNodeId)> {
        let pair = self.nodes.iter().find_map(|parent| {
            parent.steps.iter().find_map(|step| match step {
                PreparedArtifactSurfaceRasterStep::NestedSurface(child)
                    if self.nodes.get(child.index()).is_some_and(|node| {
                        node.identity.role != RetainedSurfaceRasterRole::PropertyEffect
                    }) =>
                {
                    Some((parent.execution_id, *child))
                }
                PreparedArtifactSurfaceRasterStep::ArtifactSpan(_)
                | PreparedArtifactSurfaceRasterStep::NestedSurface(_) => None,
            })
        });
        if let Some((parent, child)) = pair {
            let child_node = self.nodes.get_mut(child.index())?;
            let geometry = child_node.geometry.finalized()?;
            child_node.geometry = ArtifactSurfaceCompositeGeometryState::Finalized(
                geometry.with_resolved_receiver_clip_for_test(ArtifactSurfaceResolvedClip::Empty),
            );
            return Some((parent, child));
        }
        None
    }
}

/// Why the artifact Surface DAG program could not be derived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SingleTargetSurfaceDagPrepareError {
    /// A chunk's command range or placed geometry is malformed.
    ChunkPlacement,
    /// A clip of this owner has an incomplete or unplaced chain.
    UnresolvedClip(NodeKey),
    SurfaceDag(SurfaceDagError),
}

/// The artifact's one relative-to-absolute spatial projection, reused while
/// every spatial snapshot family is unchanged.
fn artifact_spatial_projection(
    artifact: &PaintArtifact,
    cache: Option<&mut PlanningCache>,
) -> Result<std::sync::Arc<super::ArtifactSpatialProjection>, SingleTargetSurfaceDagPrepareError> {
    if let Some(spatial) = cache.and_then(|cache| cache.spatial(artifact)) {
        return Ok(spatial);
    }
    super::ArtifactSpatialProjection::try_new(artifact)
        .map(std::sync::Arc::new)
        .map_err(spatial_prepare_error)
}

fn spatial_prepare_error(error: SpatialProjectionError) -> SingleTargetSurfaceDagPrepareError {
    SingleTargetSurfaceDagPrepareError::SurfaceDag(SurfaceDagError::Transition(
        TransitionError::SpatialSnapshot(error),
    ))
}

/// Compiler intake: every consumer below reads viewport geometry, so
/// owner-local clips and chunks are placed first.
fn place_artifact_geometry(
    artifact: &mut PaintArtifact,
    scale_factor: f32,
    mut cache: Option<&mut PlanningCache>,
) -> Result<std::sync::Arc<super::ArtifactSpatialProjection>, SingleTargetSurfaceDagPrepareError> {
    let spatial = artifact_spatial_projection(artifact, cache.as_deref_mut())?;
    spatial
        .place_clips(&mut artifact.clip_nodes)
        .map_err(spatial_prepare_error)?;
    place_artifact_chunks(artifact, &spatial, scale_factor, cache).map_err(
        |error| match error {
            ChunkPlacementError::MissingFrame(owner) => {
                spatial_prepare_error(SpatialProjectionError::InvalidSnapshot(owner))
            }
            ChunkPlacementError::Spatial(error) => spatial_prepare_error(error),
            ChunkPlacementError::InvalidChunk => SingleTargetSurfaceDagPrepareError::ChunkPlacement,
        },
    )?;
    Ok(spatial)
}

/// Places every layout-frame chunk and its commands at its frame's origin,
/// then every chunk at its owner's paint offset snapped to the physical pixel
/// grid of `scale_factor`. Command order and every chunk's op range are
/// unchanged. Recorded blocks are placed block by block, so an unmoved block
/// keeps the same placed allocation and every allocation-keyed proof
/// downstream still holds.
pub(crate) fn place_artifact_chunks(
    artifact: &mut PaintArtifact,
    spatial: &super::ArtifactSpatialProjection,
    scale_factor: f32,
    cache: Option<&mut PlanningCache>,
) -> Result<(), ChunkPlacementError> {
    let _profile = crate::view::paint::work_profile::scope("place_artifact_chunks");
    let snaps = spatial
        .owner_paint_offsets(scale_factor)
        .map_err(ChunkPlacementError::Spatial)?;
    let sources = ChunkPlacementSources {
        spatial,
        snaps: &snaps,
    };
    if artifact.chunks.iter().all(|chunk| {
        chunk.frame == super::PaintChunkFrame::Viewport
            && snaps
                .get(&chunk.owner)
                .is_some_and(|snap| snap.map(f32::to_bits) == [0.0_f32.to_bits(); 2])
    }) {
        return Ok(());
    }
    if let Some((chunks, ops)) = place_shared_chunk_blocks(artifact, sources, cache)? {
        artifact.chunks = chunks;
        artifact.ops = ops;
        return Ok(());
    }
    let mut ops = artifact.ops.iter().cloned().collect::<Vec<_>>();
    let mut chunks = super::shared_sequence::SharedSequence::new();
    for chunk in artifact.chunks.iter() {
        let start = chunk.op_range.start;
        let local_ops = ops
            .get(chunk.op_range.clone())
            .ok_or(ChunkPlacementError::InvalidChunk)?;
        let (placed, placed_ops) = place_chunk(chunk, local_ops, sources)?;
        if let Some(placed_ops) = placed_ops {
            ops[start..start + placed_ops.len()].clone_from_slice(&placed_ops);
        }
        chunks.push(placed);
    }
    let mut placed_ops = super::shared_sequence::SharedSequence::new();
    for op in ops {
        placed_ops.push(op);
    }
    artifact.chunks = chunks;
    artifact.ops = placed_ops;
    Ok(())
}

/// Why one artifact's chunks could not be placed.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ChunkPlacementError {
    /// A layout-frame chunk names no derivable layout frame, or a chunk's
    /// owner has no snap.
    MissingFrame(NodeKey),
    /// The owner snap chain is invalid.
    Spatial(SpatialProjectionError),
    /// A chunk's command range or translated geometry is malformed.
    InvalidChunk,
}

/// Where one chunk is placed: its layout frame's origin, for a layout-frame
/// chunk, and its owner's snapped paint offset.
type ChunkPlacement = (Option<[u32; 2]>, [u32; 2]);

/// The two inputs of chunk placement: frame origins and owner snaps.
#[derive(Clone, Copy)]
struct ChunkPlacementSources<'a> {
    spatial: &'a super::ArtifactSpatialProjection,
    snaps: &'a FxHashMap<NodeKey, [f32; 2]>,
}

fn chunk_placement(
    chunk: &super::PaintChunk,
    sources: ChunkPlacementSources<'_>,
) -> Result<ChunkPlacement, ChunkPlacementError> {
    let missing = ChunkPlacementError::MissingFrame(chunk.owner);
    let origin = match chunk.frame {
        super::PaintChunkFrame::Viewport => None,
        super::PaintChunkFrame::Layout => Some(
            chunk
                .properties
                .layout_position
                .and_then(|frame| sources.spatial.frame_origin(frame.0))
                .ok_or(missing)?,
        ),
    };
    let snap = *sources.snaps.get(&chunk.owner).ok_or(missing)?;
    Ok((
        origin.map(|origin| origin.map(f32::to_bits)),
        snap.map(f32::to_bits),
    ))
}

fn is_unmoved(placement: ChunkPlacement) -> bool {
    placement == (None, [0.0_f32.to_bits(); 2])
}

/// One chunk placed in viewport space, with its placed commands; a chunk
/// that does not move is returned unchanged with no commands to replace.
fn place_chunk(
    chunk: &super::PaintChunk,
    local_ops: &[PaintOp],
    sources: ChunkPlacementSources<'_>,
) -> Result<(super::PaintChunk, Option<Vec<PaintOp>>), ChunkPlacementError> {
    let placement = chunk_placement(chunk, sources)?;
    if is_unmoved(placement) {
        return Ok((chunk.clone(), None));
    }
    let invalid = ChunkPlacementError::InvalidChunk;
    if local_ops.len() != chunk.op_range.len() {
        return Err(invalid);
    }
    let (origin, snap) = placement;
    let (placed, ops) = chunk
        .placed(
            local_ops.iter().cloned(),
            origin.map(|origin| origin.map(f32::from_bits)),
            snap.map(f32::from_bits),
        )
        .ok_or(invalid)?;
    Ok((placed, Some(ops)))
}

/// Block-preserving placement. `None` means the recorded sequences are not a
/// one-to-one schedule of chunk and command blocks.
#[allow(clippy::type_complexity)]
fn place_shared_chunk_blocks(
    artifact: &PaintArtifact,
    sources: ChunkPlacementSources<'_>,
    mut cache: Option<&mut PlanningCache>,
) -> Result<
    Option<(
        super::shared_sequence::SharedSequence<super::PaintChunk>,
        super::shared_sequence::SharedSequence<PaintOp>,
    )>,
    ChunkPlacementError,
> {
    let Some(blocks) = artifact.chunks.shared_blocks() else {
        return Ok(None);
    };
    let mut chunks = super::shared_sequence::SharedSequence::with_shared_capacity(blocks.len());
    let mut ops = super::shared_sequence::SharedSequence::with_shared_capacity(blocks.len());
    for block in blocks {
        let (Some(first), Some(last)) = (block.first(), block.last()) else {
            continue;
        };
        let op_block = if first.op_range.start == last.op_range.end {
            None
        } else {
            match artifact.ops.shared_block_at(first.op_range.start) {
                Some((start, op_block))
                    if start == first.op_range.start
                        && start + op_block.len() == last.op_range.end =>
                {
                    Some(op_block)
                }
                _ => return Ok(None),
            }
        };
        let placements = block
            .iter()
            .map(|chunk| chunk_placement(chunk, sources))
            .collect::<Result<Vec<_>, _>>()?;
        let placed = if placements.iter().copied().all(is_unmoved) {
            Some((block.clone(), op_block.cloned()))
        } else {
            cache
                .as_deref_mut()
                .and_then(|cache| cache.placed_block(block, op_block, &placements))
        };
        let (placed_chunks, placed_ops) = match placed {
            Some(placed) => placed,
            None => {
                let base = first.op_range.start;
                let mut placed_ops = op_block.map(|ops| ops.to_vec()).unwrap_or_default();
                let mut placed_chunks = Vec::with_capacity(block.len());
                for chunk in block.iter() {
                    let range = chunk.op_range.start - base..chunk.op_range.end - base;
                    let local_ops = placed_ops
                        .get(range.clone())
                        .ok_or(ChunkPlacementError::InvalidChunk)?;
                    let (placed, chunk_ops) = place_chunk(chunk, local_ops, sources)?;
                    if let Some(chunk_ops) = chunk_ops {
                        placed_ops[range].clone_from_slice(&chunk_ops);
                    }
                    placed_chunks.push(placed);
                }
                let placed_chunks: std::sync::Arc<[super::PaintChunk]> = placed_chunks.into();
                let placed_ops = op_block.map(|_| std::sync::Arc::<[PaintOp]>::from(placed_ops));
                if let Some(cache) = cache.as_deref_mut() {
                    cache.remember_placed_block(
                        block,
                        op_block,
                        placements,
                        &placed_chunks,
                        placed_ops.as_ref(),
                    );
                }
                (placed_chunks, placed_ops)
            }
        };
        chunks.append_shared(placed_chunks);
        if let Some(placed_ops) = placed_ops {
            ops.append_shared(placed_ops);
        }
    }
    Ok(
        (chunks.len() == artifact.chunks.len() && ops.len() == artifact.ops.len())
            .then_some((chunks, ops)),
    )
}

fn derive_artifact_surface_dag_program(
    mut artifact: PaintArtifact,
    scale_factor: f32,
    mut cache: Option<&mut PlanningCache>,
) -> Result<ArtifactSurfaceDagProgram, SingleTargetSurfaceDagPrepareError> {
    let _profile = crate::view::paint::work_profile::scope("derive_artifact_surface_dag_program");
    let owner_local_clips = artifact
        .clip_nodes
        .iter()
        .filter_map(|clip| match clip.geometry {
            ClipGeometry::OwnerLocal(rect) => Some((clip.id, rect)),
            ClipGeometry::Viewport(_) => None,
        })
        .collect();
    let spatial = place_artifact_geometry(&mut artifact, scale_factor, cache.as_deref_mut())?;
    let resolved_clips = try_resolve_clip_values(&artifact)
        .map_err(SingleTargetSurfaceDagPrepareError::UnresolvedClip)?;

    if let Some(mut cached) = cache
        .as_deref_mut()
        .and_then(|cache| cache.geometry(&artifact))
    {
        cached.artifact = artifact;
        cached.resolved_clips = resolved_clips;
        cached.owner_local_clips = owner_local_clips;
        return Ok(cached);
    }

    let graphs = cache
        .as_deref_mut()
        .map(|cache| cache.graphs(&artifact))
        .transpose()
        .map_err(SurfaceDagError::Transition)
        .map_err(SingleTargetSurfaceDagPrepareError::SurfaceDag)?
        .flatten();
    // A warm structural plan retains only graph relationships; `graphs`
    // revalidated numeric snapshots. Transfer obligations and target
    // elimination stay current.
    let structure = if graphs.is_some() {
        cache
            .as_deref_mut()
            .map(|cache| cache.surface_structure(&artifact, &spatial))
            .transpose()
            .map_err(SingleTargetSurfaceDagPrepareError::SurfaceDag)?
            .flatten()
    } else {
        None
    };
    let (surface_dag, coverage, graphs) = if let Some((dag, coverage)) = structure {
        (
            dag,
            coverage,
            graphs.expect("structural replay requires current graph proof"),
        )
    } else {
        let inputs = super::surface_dag::ArtifactSurfaceInputs::with_graphs(
            &artifact,
            LayerizationPolicy::ResolveMaterializedTargets,
            graphs,
        )
        .map_err(SingleTargetSurfaceDagPrepareError::SurfaceDag)?;
        let requests = inputs
            .requests()
            .map_err(SingleTargetSurfaceDagPrepareError::SurfaceDag)?;
        let events = inputs
            .classify(&requests)
            .map_err(SurfaceDagError::Transition)
            .map_err(SingleTargetSurfaceDagPrepareError::SurfaceDag)?;
        let surface_dag = inputs
            .reconstruct(&events, &spatial)
            .map_err(SingleTargetSurfaceDagPrepareError::SurfaceDag)?;
        let coverage = match cache
            .as_deref_mut()
            .and_then(|cache| cache.coverage(&artifact, &surface_dag))
        {
            Some(coverage) => coverage,
            None => inputs
                .coverage(&surface_dag)
                .map_err(SingleTargetSurfaceDagPrepareError::SurfaceDag)?
                .into(),
        };
        let graphs = inputs.graphs();
        (surface_dag, coverage, graphs)
    };
    let execution_order = surface_dag
        .derive_materialized_execution_order(
            &coverage,
            LayerizationPolicy::ResolveMaterializedTargets,
        )
        .map_err(SingleTargetSurfaceDagPrepareError::SurfaceDag)?;
    let host_placement = match cache
        .as_deref_mut()
        .and_then(|cache| cache.host_placement(&artifact))
    {
        Some(placement) => placement,
        None => ArtifactSurfaceHostPlacementProjection::new(&spatial).into(),
    };

    let program = ArtifactSurfaceDagProgram {
        artifact,
        resolved_clips,
        owner_local_clips,
        surface_dag: surface_dag.into(),
        execution_order: execution_order.into(),
        coverage,
        spatial,
        host_placement,
    };
    if let Some(cache) = cache {
        cache.remember_graphs(graphs);
        cache.remember_geometry(&program);
    }
    Ok(program)
}

fn translated_chunk_bounds_bits(
    bounds: crate::view::base_component::Rect,
    delta: [f32; 2],
) -> Option<[u32; 4]> {
    translated_bounds_bits(
        [bounds.x, bounds.y, bounds.width, bounds.height].map(f32::to_bits),
        delta,
    )
}

fn translated_bounds_bits(bounds_bits: [u32; 4], delta: [f32; 2]) -> Option<[u32; 4]> {
    let [x, y, width, height] = bounds_bits.map(f32::from_bits);
    let translated = [x + delta[0], y + delta[1], width, height];
    (translated.into_iter().all(f32::is_finite)
        && translated[2] >= 0.0
        && translated[3] >= 0.0
        && (translated[0] + translated[2]).is_finite()
        && (translated[1] + translated[3]).is_finite())
    .then(|| translated.map(f32::to_bits))
}

fn union_bounds_bits(left: [u32; 4], right: [u32; 4]) -> Option<[u32; 4]> {
    let [lx, ly, lw, lh] = left.map(f32::from_bits);
    let [rx, ry, rw, rh] = right.map(f32::from_bits);
    let min_x = lx.min(rx);
    let min_y = ly.min(ry);
    let max_x = (lx + lw).max(rx + rw);
    let max_y = (ly + lh).max(ry + rh);
    let union = [min_x, min_y, max_x - min_x, max_y - min_y];
    (union.into_iter().all(f32::is_finite) && union[2] > 0.0 && union[3] > 0.0)
        .then(|| union.map(f32::to_bits))
}

fn append_bounds(accumulated: &mut Option<[u32; 4]>, next: [u32; 4]) -> Option<()> {
    *accumulated = Some(match *accumulated {
        Some(current) => union_bounds_bits(current, next)?,
        None => next,
    });
    Some(())
}

/// Artifact owner placement sealed before raster preparation. The raster plan
/// applies it only to paint and composite edges landing in the scene root;
/// detached raster content remains host-independent.
#[derive(Clone, Debug)]
struct ArtifactSurfaceHostPlacementProjection {
    owners: std::sync::Arc<[super::OwnerSnapPoints]>,
    // Shared only by clones of this exact immutable owner snap program.
    // Host offset is the remaining input; successful resolution is cached once.
    resolved: std::sync::Arc<std::sync::Mutex<Option<HostPlacementResolutionMemo>>>,
}

#[derive(Clone, Debug)]
struct HostPlacementResolutionMemo {
    owners: std::sync::Arc<[super::OwnerSnapPoints]>,
    offset_bits: [u32; 2],
    scale_factor_bits: u32,
    resolved: ResolvedArtifactSurfaceHostPlacement,
}

#[derive(Clone, Debug)]
struct ResolvedArtifactSurfaceHostPlacement {
    /// Per owner, the host offset's change to the owner's snapped paint
    /// offset; zero for a zero host.
    host_delta_bits: std::sync::Arc<FxHashMap<NodeKey, [u32; 2]>>,
}

impl ArtifactSurfaceHostPlacementProjection {
    fn new(spatial: &super::ArtifactSpatialProjection) -> Self {
        Self {
            owners: spatial.owner_snap_points().clone(),
            resolved: Default::default(),
        }
    }

    fn resolve(
        &self,
        host_paint_offset: [f32; 2],
        scale_factor: f32,
    ) -> Result<ResolvedArtifactSurfaceHostPlacement, SpatialProjectionError> {
        let offset_bits = host_paint_offset.map(f32::to_bits);
        let scale_factor_bits = scale_factor.to_bits();
        if let Some(memo) = self
            .resolved
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .filter(|memo| {
                std::sync::Arc::ptr_eq(&memo.owners, &self.owners)
                    && memo.offset_bits == offset_bits
                    && memo.scale_factor_bits == scale_factor_bits
            })
        {
            return Ok(memo.resolved.clone());
        }
        // Intake already placed every chunk at its owner's snapped paint
        // offset for a zero host. Placement adds only what this host offset
        // changes in that chain, so a snap is never applied twice.
        let zero_chain =
            super::snapped_owner_paint_offsets(&self.owners, [0.0, 0.0], scale_factor)?;
        let host_delta_bits = if offset_bits == [0.0_f32, 0.0_f32].map(f32::to_bits) {
            zero_chain
                .keys()
                .map(|owner| (*owner, [0.0_f32, 0.0_f32].map(f32::to_bits)))
                .collect()
        } else {
            let host_chain =
                super::snapped_owner_paint_offsets(&self.owners, host_paint_offset, scale_factor)?;
            host_chain
                .iter()
                .map(|(owner, host)| {
                    let zero = zero_chain
                        .get(owner)
                        .ok_or(SpatialProjectionError::InvalidSnapshot(*owner))?;
                    let delta = [host[0] - zero[0], host[1] - zero[1]];
                    delta
                        .iter()
                        .all(|value| value.is_finite())
                        .then(|| (*owner, delta.map(f32::to_bits)))
                        .ok_or(SpatialProjectionError::InvalidSnapshot(*owner))
                })
                .collect::<Result<FxHashMap<_, _>, _>>()?
        };
        let resolved = ResolvedArtifactSurfaceHostPlacement {
            host_delta_bits: std::sync::Arc::new(host_delta_bits),
        };
        *self
            .resolved
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(HostPlacementResolutionMemo {
            owners: self.owners.clone(),
            offset_bits,
            scale_factor_bits,
            resolved: resolved.clone(),
        });
        Ok(resolved)
    }
}

impl ResolvedArtifactSurfaceHostPlacement {
    fn owner_paint_offset(&self, owner: NodeKey) -> Option<[f32; 2]> {
        self.host_delta_bits
            .get(&owner)
            .copied()
            .map(|bits| bits.map(f32::from_bits))
    }

    fn receiver_paint_offset(
        &self,
        owner: NodeKey,
        receiver: SurfaceDagExecutionTargetId,
    ) -> Option<[f32; 2]> {
        match receiver {
            SurfaceDagExecutionTargetId::Surface(_) => Some([0.0, 0.0]),
            SurfaceDagExecutionTargetId::SceneRoot(_) => self.owner_paint_offset(owner),
        }
    }
}

/// One surface's sole receiver-space to texture-local projection.
///
/// The signed physical origin is a placement fact. It deliberately stays out
/// of resident identity and raster stamps; only the normalized raster inputs
/// produced through this value participate in equality and reuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ArtifactSurfaceRasterOriginProjection {
    scale_factor_bits: u32,
    physical_origin: [i64; 2],
    translation_bits: [u32; 2],
    normalized_source_bounds_bits: [u32; 4],
    target_size: [u32; 2],
}

impl ArtifactSurfaceRasterOriginProjection {
    fn new(raw_source_bounds_bits: [u32; 4], scale_factor_bits: u32) -> Option<Self> {
        let [x, y, width, height] = raw_source_bounds_bits.map(f32::from_bits);
        let scale = f32::from_bits(scale_factor_bits);
        if !scale.is_finite()
            || scale <= 0.0
            || ![x, y, width, height].into_iter().all(f32::is_finite)
            || width <= 0.0
            || height <= 0.0
        {
            return None;
        }
        let physical_left = finite_f32_to_i64((x * scale).floor())?;
        let physical_top = finite_f32_to_i64((y * scale).floor())?;
        let logical_origin = [physical_left as f32 / scale, physical_top as f32 / scale];
        let translation = [-logical_origin[0], -logical_origin[1]];
        let normalized = [x + translation[0], y + translation[1], width, height];
        if normalized.into_iter().any(|value| !value.is_finite())
            || normalized[0] < 0.0
            || normalized[1] < 0.0
            || normalized[0] >= 1.0 / scale
            || normalized[1] >= 1.0 / scale
        {
            return None;
        }
        // Size the target from the normalized bounds, exactly as a descriptor
        // derived from them is sized: at a scale that is not a whole number,
        // the raw bounds can round one pixel apart.
        let target_width = finite_f32_to_i64(((normalized[0] + width) * scale).ceil())
            .and_then(|n| u32::try_from(n).ok())?;
        let target_height = finite_f32_to_i64(((normalized[1] + height) * scale).ceil())
            .and_then(|n| u32::try_from(n).ok())?;
        if target_width == 0 || target_height == 0 {
            return None;
        }
        Some(Self {
            scale_factor_bits,
            physical_origin: [physical_left, physical_top],
            translation_bits: translation.map(f32::to_bits),
            normalized_source_bounds_bits: normalized.map(f32::to_bits),
            target_size: [target_width, target_height],
        })
    }

    fn translation(self) -> [f32; 2] {
        self.translation_bits.map(f32::from_bits)
    }

    fn composite_source_physical_origin(
        self,
        source_bounds_bits: [u32; 4],
        destination_bounds_bits: [u32; 4],
    ) -> Option<[f32; 2]> {
        if source_bounds_bits != self.normalized_source_bounds_bits {
            return None;
        }
        let source = source_bounds_bits.map(f32::from_bits);
        let destination = destination_bounds_bits.map(f32::from_bits);
        let scale = f32::from_bits(self.scale_factor_bits);
        let origin = [
            (destination[0] - source[0]) * scale,
            (destination[1] - source[1]) * scale,
        ];
        (scale.is_finite()
            && scale > 0.0
            && source.into_iter().all(f32::is_finite)
            && destination.into_iter().all(f32::is_finite)
            && origin.into_iter().all(f32::is_finite))
        .then_some(origin)
    }

    fn combined_translation(self, base: [f32; 2]) -> Option<[f32; 2]> {
        let projection = self.translation();
        let combined = [base[0] + projection[0], base[1] + projection[1]];
        combined.into_iter().all(f32::is_finite).then_some(combined)
    }

    fn project_bounds_bits(self, bounds_bits: [u32; 4]) -> Option<[u32; 4]> {
        let mut bounds = bounds_bits.map(f32::from_bits);
        let translation = self.translation();
        bounds[0] += translation[0];
        bounds[1] += translation[1];
        (bounds.into_iter().all(f32::is_finite) && bounds[2] > 0.0 && bounds[3] > 0.0)
            .then(|| bounds.map(f32::to_bits))
    }

    fn project_clip(self, clip: ResolvedClip) -> ArtifactSurfaceResolvedClip {
        match clip {
            ResolvedClip::Unclipped => ArtifactSurfaceResolvedClip::Unclipped,
            ResolvedClip::Empty => ArtifactSurfaceResolvedClip::Empty,
            ResolvedClip::Scissor([x, y, width, height]) => {
                let scale = f32::from_bits(self.scale_factor_bits);
                let left = (x as f32 * scale).floor().max(0.0) as i64 - self.physical_origin[0];
                let top = (y as f32 * scale).floor().max(0.0) as i64 - self.physical_origin[1];
                let right = ((x as f32 + width as f32) * scale).ceil().max(0.0) as i64
                    - self.physical_origin[0];
                let bottom = ((y as f32 + height as f32) * scale).ceil().max(0.0) as i64
                    - self.physical_origin[1];
                match clamp_signed_scissor_to_target([left, top, right, bottom], self.target_size) {
                    Some(scissor) => {
                        ArtifactSurfaceResolvedClip::Scissor(target_physical_scissor(scissor))
                    }
                    None => ArtifactSurfaceResolvedClip::Empty,
                }
            }
        }
    }

    fn target_physical_scissor_for_projected_bounds(
        self,
        bounds_bits: [u32; 4],
    ) -> Option<GraphicsPassScissor> {
        let [x, y, width, height] = bounds_bits.map(f32::from_bits);
        let scale = f32::from_bits(self.scale_factor_bits);
        let physical = [
            finite_f32_to_i64((x * scale).floor())?,
            finite_f32_to_i64((y * scale).floor())?,
            finite_f32_to_i64(((x + width) * scale).ceil())?,
            finite_f32_to_i64(((y + height) * scale).ceil())?,
        ];
        clamp_signed_scissor_to_target(physical, self.target_size).map(target_physical_scissor)
    }
}

fn finite_f32_to_i64(value: f32) -> Option<i64> {
    (value.is_finite() && value >= i64::MIN as f32 && value <= i64::MAX as f32)
        .then_some(value as i64)
}

fn clamp_signed_scissor_to_target(
    [left, top, right, bottom]: [i64; 4],
    [width, height]: [u32; 2],
) -> Option<[u32; 4]> {
    let width = i64::from(width);
    let height = i64::from(height);
    let left = left.clamp(0, width);
    let top = top.clamp(0, height);
    let right = right.clamp(0, width);
    let bottom = bottom.clamp(0, height);
    (right > left && bottom > top).then_some([
        left as u32,
        top as u32,
        (right - left) as u32,
        (bottom - top) as u32,
    ])
}

fn target_physical_scissor(scissor: [u32; 4]) -> GraphicsPassScissor {
    GraphicsPassScissor::TargetPhysical(scissor)
}

fn resolve_artifact_surface_clip(
    leaf: Option<ClipNodeId>,
    clips: &FxHashMap<ClipNodeId, ClipNodeSnapshot>,
) -> Option<(ResolvedClip, Vec<ClipNodeSnapshot>)> {
    let Some(mut cursor) = leaf else {
        return Some((ResolvedClip::Unclipped, Vec::new()));
    };
    let mut chain = Vec::new();
    let mut seen = FxHashSet::default();
    loop {
        if !seen.insert(cursor) {
            return None;
        }
        let snapshot = *clips.get(&cursor)?;
        chain.push(snapshot);
        let Some(parent) = snapshot.parent else {
            break;
        };
        cursor = parent;
    }
    Some((fold_resolved_clip_chain(&chain)?, chain))
}

fn intersect_optional_scissor(
    resolved: ResolvedClip,
    incoming_scissor: Option<[u32; 4]>,
) -> ResolvedClip {
    incoming_scissor
        .map(|scissor| intersect_resolved_clip(resolved, scissor))
        .unwrap_or(resolved)
}

fn artifact_surface_terminal_clip(
    resolved: ResolvedClip,
    chain: &[ClipNodeSnapshot],
    incoming_scissor: Option<[u32; 4]>,
) -> ResolvedClip {
    // `resolve_artifact_surface_clip` has already evaluated the complete
    // artifact chain correctly. Only an all-Intersect chain inherits the
    // frame scissor; any Replace severs that external input exactly as it
    // severs the upstream portion of the artifact chain.
    if chain
        .iter()
        .any(|snapshot| snapshot.behavior == ClipBehavior::Replace)
    {
        resolved
    } else {
        intersect_optional_scissor(resolved, incoming_scissor)
    }
}

fn translate_artifact_surface_logical_clip(
    clip: ResolvedClip,
    paint_offset: [f32; 2],
) -> Option<ResolvedClip> {
    match clip {
        ResolvedClip::Unclipped | ResolvedClip::Empty => Some(clip),
        ResolvedClip::Scissor([x, y, width, height]) => {
            let translated = Rect {
                x: x as f32 + paint_offset[0],
                y: y as f32 + paint_offset[1],
                width: width as f32,
                height: height as f32,
            };
            Some(
                exact_logical_scissor_for_rect(translated)
                    .map(ResolvedClip::Scissor)
                    .unwrap_or(ResolvedClip::Empty),
            )
        }
    }
}

fn artifact_surface_chunk_clip_schedule(
    artifact: &PaintArtifact,
    chunk: &super::PaintChunk,
    resolved: ResolvedClip,
    chain: &[ClipNodeSnapshot],
    incoming_scissor: Option<[u32; 4]>,
    raster_origin: Option<ArtifactSurfaceRasterOriginProjection>,
) -> ArtifactSurfaceChunkClipSchedule {
    let terminal_clip = artifact_surface_terminal_clip(resolved, chain, incoming_scissor);
    let terminal_clip = raster_origin
        .map(|projection| projection.project_clip(terminal_clip))
        .unwrap_or_else(|| ArtifactSurfaceResolvedClip::from_logical(terminal_clip));
    match exact_self_clip_shadow_prefix_len(artifact, chunk) {
        Some(prefix_op_count) => ArtifactSurfaceChunkClipSchedule::AfterShadowPrefix {
            prefix_op_count,
            suffix_clip: terminal_clip,
        },
        None => ArtifactSurfaceChunkClipSchedule::WholeChunk(terminal_clip),
    }
}

fn artifact_surface_chunk_opaque_order_count(
    schedule: ArtifactSurfaceChunkClipSchedule,
    ops: &[PaintOp],
) -> Option<u32> {
    let visible_ops = match schedule {
        ArtifactSurfaceChunkClipSchedule::WholeChunk(ArtifactSurfaceResolvedClip::Empty) => {
            &ops[..0]
        }
        ArtifactSurfaceChunkClipSchedule::AfterShadowPrefix {
            prefix_op_count,
            suffix_clip: ArtifactSurfaceResolvedClip::Empty,
        } => ops.get(..prefix_op_count)?,
        ArtifactSurfaceChunkClipSchedule::WholeChunk(
            ArtifactSurfaceResolvedClip::Unclipped | ArtifactSurfaceResolvedClip::Scissor(_),
        )
        | ArtifactSurfaceChunkClipSchedule::AfterShadowPrefix {
            suffix_clip:
                ArtifactSurfaceResolvedClip::Unclipped | ArtifactSurfaceResolvedClip::Scissor(_),
            ..
        } => ops,
    };
    visible_ops.iter().try_fold(0_u32, |count, op| {
        count.checked_add(retained_surface_op_opaque_order_count(op))
    })
}

#[cfg(test)]
pub(crate) fn resolve_artifact_surface_clip_for_test(
    leaf: Option<ClipNodeId>,
    clips: &[ClipNodeSnapshot],
    incoming_scissor: Option<[u32; 4]>,
) -> Option<ResolvedClip> {
    let clips = clips
        .iter()
        .copied()
        .map(|snapshot| (snapshot.id, snapshot))
        .collect::<FxHashMap<_, _>>();
    resolve_artifact_surface_clip(leaf, &clips)
        .map(|(resolved, chain)| artifact_surface_terminal_clip(resolved, &chain, incoming_scissor))
}

fn artifact_surface_span_owner_topology(
    artifact: &PaintArtifact,
    target: ArtifactSurfaceRasterTargetId,
    boundary_root: Option<NodeKey>,
    chunk_owners: impl IntoIterator<Item = NodeKey>,
) -> Result<Vec<PaintOwnerSnapshot>, ArtifactSurfaceRasterPlanError> {
    let Some(boundary_root) = boundary_root else {
        return Ok(Vec::new());
    };
    let owners = artifact
        .owner_nodes
        .iter()
        .copied()
        .map(|snapshot| (snapshot.owner, snapshot))
        .collect::<FxHashMap<_, _>>();
    if owners.len() != artifact.owner_nodes.len() {
        return Err(ArtifactSurfaceRasterPlanError::InvalidOwnerTopology {
            target,
            owner: boundary_root,
        });
    }
    let mut required = FxHashSet::default();
    let mut emitted = FxHashSet::default();
    let mut topology = Vec::new();
    for chunk_owner in chunk_owners {
        let mut chain = Vec::new();
        let mut cursor = chunk_owner;
        let mut reached_boundary = false;
        for _ in 0..=artifact.owner_nodes.len() {
            let snapshot = owners.get(&cursor).copied().ok_or(
                ArtifactSurfaceRasterPlanError::InvalidOwnerTopology {
                    target,
                    owner: cursor,
                },
            )?;
            required.insert(cursor);
            chain.push(snapshot);
            if cursor == boundary_root {
                reached_boundary = true;
                break;
            }
            cursor =
                snapshot
                    .parent
                    .ok_or(ArtifactSurfaceRasterPlanError::InvalidOwnerTopology {
                        target,
                        owner: chunk_owner,
                    })?;
        }
        if !reached_boundary {
            return Err(ArtifactSurfaceRasterPlanError::InvalidOwnerTopology {
                target,
                owner: chunk_owner,
            });
        }
        // The artifact store follows recording discovery, not parent-first
        // topology (transparent parents can be discovered after a paint
        // child). Order only this span's required closure from parent edges;
        // command order remains in chunks. Keep the sealer's parent-first
        // invariant instead of weakening it for a component family.
        for snapshot in chain.into_iter().rev() {
            if emitted.insert(snapshot.owner) {
                topology.push(PaintOwnerSnapshot {
                    parent: if snapshot.owner == boundary_root {
                        None
                    } else {
                        snapshot.parent
                    },
                    ..snapshot
                });
            }
        }
    }
    if topology.len() != required.len()
        || topology
            .iter()
            .find(|snapshot| snapshot.owner == boundary_root)
            .is_none_or(|snapshot| snapshot.parent.is_some())
    {
        return Err(ArtifactSurfaceRasterPlanError::InvalidOwnerTopology {
            target,
            owner: boundary_root,
        });
    }
    Ok(topology)
}

fn artifact_surface_span_chunks<'a>(
    artifact: &'a PaintArtifact,
    target: ArtifactSurfaceRasterTargetId,
    span: &ArtifactSurfaceCoverageSpan,
) -> Result<
    super::shared_sequence::SequenceView<'a, super::PaintChunk>,
    ArtifactSurfaceRasterPlanError,
> {
    let chunk_range = span.chunk_range();
    let op_range = span.op_range();
    let chunks = artifact
        .chunks
        .view(chunk_range)
        .ok_or(ArtifactSurfaceRasterPlanError::InvalidCoverageSpan(target))?;
    if chunks.len() != span.localized_states().len()
        || chunks.first().map(|chunk| chunk.op_range.start) != Some(op_range.start)
        || chunks.last().map(|chunk| chunk.op_range.end) != Some(op_range.end)
    {
        return Err(ArtifactSurfaceRasterPlanError::InvalidCoverageSpan(target));
    }
    Ok(chunks)
}

/// A surface boundary owner's own stencil/clip chunks stay in receiver
/// placement space; only descendant content consumes the surface-kind base
/// translation. Raster-origin projection is then applied to both through the
/// same second-stage projection.
/// A surface's own raster translation moves only its boundary root's
/// descendants; a folded boundary's moves every chunk, the root's included,
/// since the root sits inside the folded boundary's content.
fn artifact_surface_chunk_base_translation(
    boundary_root: Option<NodeKey>,
    chunk_owner: NodeKey,
    base_delta: [f32; 2],
    root_delta: [f32; 2],
) -> [f32; 2] {
    if boundary_root == Some(chunk_owner) {
        root_delta
    } else {
        base_delta
    }
}

fn artifact_surface_span_raw_bounds(
    artifact: &PaintArtifact,
    target: ArtifactSurfaceRasterTargetId,
    boundary_root: Option<NodeKey>,
    span: &ArtifactSurfaceCoverageSpan,
    base_delta: [f32; 2],
    root_delta: [f32; 2],
) -> Result<Option<[u32; 4]>, ArtifactSurfaceRasterPlanError> {
    artifact_surface_chunk_range_raw_bounds(
        artifact,
        target,
        boundary_root,
        span.chunk_range(),
        base_delta,
        root_delta,
    )
}

fn artifact_surface_chunk_range_raw_bounds(
    artifact: &PaintArtifact,
    target: ArtifactSurfaceRasterTargetId,
    boundary_root: Option<NodeKey>,
    chunk_range: Range<usize>,
    base_delta: [f32; 2],
    root_delta: [f32; 2],
) -> Result<Option<[u32; 4]>, ArtifactSurfaceRasterPlanError> {
    let chunks = artifact
        .chunks
        .view(chunk_range.clone())
        .ok_or(ArtifactSurfaceRasterPlanError::InvalidCoverageSpan(target))?;
    let mut bounds = None;
    for (local_index, chunk) in chunks.iter().enumerate() {
        let chunk_index = chunk_range.start + local_index;
        let delta = artifact_surface_chunk_base_translation(
            boundary_root,
            chunk.owner,
            base_delta,
            root_delta,
        );
        let translated = translated_chunk_bounds_bits(chunk.bounds, delta).ok_or(
            ArtifactSurfaceRasterPlanError::InvalidChunkBounds {
                target,
                chunk_index,
            },
        )?;
        let [_, _, width, height] = translated.map(f32::from_bits);
        if width > 0.0 && height > 0.0 {
            append_bounds(&mut bounds, translated)
                .ok_or(ArtifactSurfaceRasterPlanError::InvalidCoverageSpan(target))?;
        }
    }
    Ok(bounds)
}

/// Selects the only placement authority available to one prepared span.
/// Scene-root spans consume owner-scoped host placement; detached spans consume
/// only surface-local translation plus raster-origin normalization.
#[derive(Clone, Copy)]
enum ArtifactSurfaceSpanPlacement<'a> {
    SceneRoot(&'a ResolvedArtifactSurfaceHostPlacement),
    Surface {
        boundary_root: NodeKey,
        base_delta: [f32; 2],
        root_delta: [f32; 2],
        raster_origin: ArtifactSurfaceRasterOriginProjection,
        /// The span's local clips placed in the surface's raster space.
        local_clips: &'a [ClipNodeSnapshot],
    },
}

impl<'a> ArtifactSurfaceSpanPlacement<'a> {
    fn boundary_root(self) -> Option<NodeKey> {
        match self {
            Self::SceneRoot(_) => None,
            Self::Surface { boundary_root, .. } => Some(boundary_root),
        }
    }

    fn chunk_translation(
        self,
        target: ArtifactSurfaceRasterTargetId,
        owner: NodeKey,
    ) -> Result<[f32; 2], ArtifactSurfaceRasterPlanError> {
        match self {
            Self::SceneRoot(host_placement) => host_placement
                .owner_paint_offset(owner)
                .ok_or(ArtifactSurfaceRasterPlanError::InvalidOwnerTopology { target, owner }),
            Self::Surface {
                boundary_root,
                base_delta,
                root_delta,
                raster_origin,
                ..
            } => raster_origin
                .combined_translation(artifact_surface_chunk_base_translation(
                    Some(boundary_root),
                    owner,
                    base_delta,
                    root_delta,
                ))
                .ok_or(ArtifactSurfaceRasterPlanError::InvalidRasterOrigin(
                    match target {
                        ArtifactSurfaceRasterTargetId::Surface(source) => source,
                        ArtifactSurfaceRasterTargetId::SceneRoot(_) => {
                            unreachable!("scene-root spans do not own a raster-origin projection")
                        }
                    },
                )),
        }
    }

    fn raster_origin(self) -> Option<ArtifactSurfaceRasterOriginProjection> {
        match self {
            Self::SceneRoot(_) => None,
            Self::Surface { raster_origin, .. } => Some(raster_origin),
        }
    }

    /// Only a scroll content surface rebases clips below its scrollport, so
    /// scene-root spans have no local clips.
    fn local_clips(self) -> &'a [ClipNodeSnapshot] {
        match self {
            Self::SceneRoot(_) => &[],
            Self::Surface { local_clips, .. } => local_clips,
        }
    }
}

fn prepare_artifact_surface_span(
    artifact: &PaintArtifact,
    target: ArtifactSurfaceRasterTargetId,
    span: &ArtifactSurfaceCoverageSpan,
    placement: ArtifactSurfaceSpanPlacement<'_>,
    composite_effect: Option<EffectNodeSnapshot>,
    incoming_scissor: Option<[u32; 4]>,
    mut cache: Option<&mut PlanningCache>,
) -> Result<PreparedArtifactSurfaceRasterSpan, ArtifactSurfaceRasterPlanError> {
    let chunk_range = span.chunk_range();
    #[cfg(test)]
    let op_range = span.op_range();
    let chunks = artifact_surface_span_chunks(artifact, target, span)?;
    if let Some(prepared) = cache.as_deref_mut().and_then(|cache| {
        cache.raster_span(
            target,
            span,
            &chunks,
            placement,
            composite_effect,
            incoming_scissor,
        )
    }) {
        return Ok(prepared);
    }

    let owner_topology = artifact_surface_span_owner_topology(
        artifact,
        target,
        placement.boundary_root(),
        chunks.iter().map(|chunk| chunk.owner),
    )?;

    let mut clip_map = artifact
        .clip_nodes
        .iter()
        .copied()
        .map(|snapshot| (snapshot.id, snapshot))
        .collect::<FxHashMap<_, _>>();
    for snapshot in placement.local_clips() {
        clip_map.insert(snapshot.id, *snapshot);
    }
    let mut prepared = Vec::with_capacity(chunks.len());
    let mut opaque_order_count = 0_u32;
    for (local_index, (chunk, localized_state)) in
        chunks.iter().zip(span.localized_states()).enumerate()
    {
        let chunk_index = chunk_range.start + local_index;
        let delta = placement.chunk_translation(target, chunk.owner)?;
        let ops = artifact
            .ops
            .get(chunk.op_range.clone())
            .ok_or(ArtifactSurfaceRasterPlanError::InvalidCoverageSpan(target))?;
        let neutralized_opacity_bits = composite_effect
            .filter(|effect| {
                chunk_carries_baked_color_opacity(chunk)
                    && chunk.properties.effect == Some(effect.id)
                    && chunk.owner == effect.owner
            })
            .map(|effect| effect.opacity.to_bits());
        let cached = cache
            .as_deref_mut()
            .and_then(|cache| cache.localized(chunk, delta, neutralized_opacity_bits));
        let (localized_ops, localized_payload) = if let Some(cached) = cached {
            cached
        } else {
            let localized_ops = ops
                .iter()
                .enumerate()
                .map(|(op_offset, op)| {
                    let op_index = chunk.op_range.start + op_offset;
                    let localized = localize_artifact_surface_op(op, delta).map_err(|reason| {
                        ArtifactSurfaceRasterPlanError::Localization {
                            target,
                            chunk_index,
                            op_index,
                            reason,
                        }
                    })?;
                    let raster = match neutralized_opacity_bits {
                        Some(_) => neutralize_artifact_surface_opacity(localized),
                        None => Ok(localized),
                    }
                    .map_err(|reason| {
                        ArtifactSurfaceRasterPlanError::Localization {
                            target,
                            chunk_index,
                            op_index,
                            reason,
                        }
                    })?;
                    // The immutable result comes directly from the checked localizer;
                    // re-running that same function is not an independent proof.
                    Ok::<_, ArtifactSurfaceRasterPlanError>(raster)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let payload = chunk
                .payload_identity
                .rebuild_from_localized_ops(&localized_ops)
                .ok_or(ArtifactSurfaceRasterPlanError::LocalizedPayload {
                    target,
                    chunk_index,
                })?;
            let localized_ops: std::sync::Arc<[PaintOp]> = localized_ops.into();
            if let Some(cache) = cache.as_deref_mut() {
                cache.remember_localized(
                    chunk,
                    delta,
                    neutralized_opacity_bits,
                    &localized_ops,
                    &payload,
                );
            }
            (localized_ops, payload)
        };
        let (resolved_clip, clip_chain) =
            resolve_artifact_surface_clip(localized_state.clip, &clip_map).ok_or(
                ArtifactSurfaceRasterPlanError::InvalidResolvedClip {
                    target,
                    chunk_index,
                },
            )?;
        let clip_schedule = artifact_surface_chunk_clip_schedule(
            artifact,
            chunk,
            resolved_clip,
            &clip_chain,
            incoming_scissor,
            placement.raster_origin(),
        );
        opaque_order_count = opaque_order_count
            .checked_add(
                artifact_surface_chunk_opaque_order_count(clip_schedule, &localized_ops)
                    .ok_or(ArtifactSurfaceRasterPlanError::InvalidCoverageSpan(target))?,
            )
            .ok_or(ArtifactSurfaceRasterPlanError::InvalidCoverageSpan(target))?;
        let localized_bounds_bits = translated_chunk_bounds_bits(chunk.bounds, delta).ok_or(
            ArtifactSurfaceRasterPlanError::InvalidChunkBounds {
                target,
                chunk_index,
            },
        )?;
        prepared.push(PreparedArtifactSurfaceRasterChunk {
            source: PaintChunkRasterIdentity {
                id: chunk.id,
                owner: chunk.owner,
                bounds_bits: [
                    chunk.bounds.x.to_bits(),
                    chunk.bounds.y.to_bits(),
                    chunk.bounds.width.to_bits(),
                    chunk.bounds.height.to_bits(),
                ],
                payload_identity: chunk.payload_identity.clone(),
            },
            content_revision: chunk.content_revision,
            localized_bounds_bits,
            localized_state: *localized_state,
            localized_payload,
            localized_ops,
            clip_schedule,
        });
    }
    let prepared = PreparedArtifactSurfaceRasterSpan {
        seal_cache: Default::default(),
        #[cfg(test)]
        op_range,
        owner_topology,
        opaque_order_count,
        local_clips: placement.local_clips().to_vec(),
        chunks: prepared.into(),
    };
    if let Some(cache) = cache {
        cache.remember_raster_span(
            target,
            span,
            &chunks,
            placement,
            composite_effect,
            incoming_scissor,
            &prepared,
        );
    }
    Ok(prepared)
}

fn surface_raster_translation(
    surface: SurfaceDagNodeId,
    kind: SurfaceDagNodeKind,
    scrolls: &FxHashMap<ScrollNodeId, ScrollNodeSnapshot>,
) -> Result<[f32; 2], ArtifactSurfaceRasterPlanError> {
    match kind {
        SurfaceDagNodeKind::Transform(_) | SurfaceDagNodeKind::Effect(_) => Ok([0.0, 0.0]),
        SurfaceDagNodeKind::ScrollContent { scroll, .. } => scrolls
            .get(&scroll)
            .map(|snapshot| [snapshot.offset.x, snapshot.offset.y])
            .ok_or(ArtifactSurfaceRasterPlanError::MissingSurfaceSnapshot(
                surface,
            )),
    }
}

fn materialized_surface_raster_translation(
    source: SurfaceDagNodeId,
    kind: SurfaceDagNodeKind,
    folded_boundaries: &[SurfaceDagNodeId],
    surface_dag: &SurfaceDag,
    scrolls: &FxHashMap<ScrollNodeId, ScrollNodeSnapshot>,
) -> Result<[f32; 2], ArtifactSurfaceRasterPlanError> {
    if folded_boundaries.len() > 1 {
        // The materializer currently retains adjacent pass-through boundaries
        // until clip and placement composition has its own typed proof.
        return Err(ArtifactSurfaceRasterPlanError::InvalidSurfaceBounds(
            folded_boundaries[1],
        ));
    }
    let own = surface_raster_translation(source, kind, scrolls)?;
    let folded = folded_surface_raster_translation(folded_boundaries, surface_dag)?;
    let translation = [own[0] + folded[0], own[1] + folded[1]];
    if translation.into_iter().any(|value| !value.is_finite()) {
        return Err(ArtifactSurfaceRasterPlanError::InvalidSurfaceBounds(source));
    }
    Ok(translation)
}

/// The raster translation the boundaries folded into a surface carry.
fn folded_surface_raster_translation(
    folded_boundaries: &[SurfaceDagNodeId],
    surface_dag: &SurfaceDag,
) -> Result<[f32; 2], ArtifactSurfaceRasterPlanError> {
    let mut translation = [0.0, 0.0];
    for boundary in folded_boundaries {
        let node = surface_dag
            .nodes()
            .get(boundary.index())
            .filter(|node| node.id() == *boundary)
            .ok_or(ArtifactSurfaceRasterPlanError::MissingSurfaceSnapshot(
                *boundary,
            ))?;
        let transfer =
            node.transfer()
                .ok_or(ArtifactSurfaceRasterPlanError::InvalidSurfaceBounds(
                    *boundary,
                ))?;
        let delta = transfer.raster_translation_bits.map(f32::from_bits);
        translation[0] += delta[0];
        translation[1] += delta[1];
        if translation.into_iter().any(|value| !value.is_finite()) {
            return Err(ArtifactSurfaceRasterPlanError::InvalidSurfaceBounds(
                *boundary,
            ));
        }
    }
    Ok(translation)
}

fn transform_destination_projection(
    source_bounds_bits: [u32; 4],
    matrix: glam::Mat4,
    offset: [f32; 2],
) -> Option<([u32; 4], [[u32; 2]; 4])> {
    let [x, y, width, height] = source_bounds_bits.map(f32::from_bits);
    // Match texture UV order: bottom-left, bottom-right, top-right, top-left.
    let corners = [
        glam::Vec3::new(x, y + height, 0.0),
        glam::Vec3::new(x + width, y + height, 0.0),
        glam::Vec3::new(x + width, y, 0.0),
        glam::Vec3::new(x, y, 0.0),
    ];
    let mut points = [[0.0; 2]; 4];
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    for (index, corner) in corners.into_iter().enumerate() {
        let projected = matrix * corner.extend(1.0);
        if !projected.is_finite() || projected.w.abs() <= 0.000_001 {
            return None;
        }
        let point = [projected.x / projected.w, projected.y / projected.w];
        if point.into_iter().any(|value| !value.is_finite()) {
            return None;
        }
        for axis in 0..2 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
        points[index] = point;
    }
    // Placement translates the AABB origin only. Subtracting two translated
    // endpoints could change its width by rounding; normalized raster origins
    // must likewise never change the projected shape.
    let bounds = [
        min[0] + offset[0],
        min[1] + offset[1],
        max[0] - min[0],
        max[1] - min[1],
    ];
    if bounds.into_iter().any(|value| !value.is_finite()) || bounds[2] <= 0.0 || bounds[3] <= 0.0 {
        return None;
    }
    let relative = points.map(|point| [point[0] - min[0], point[1] - min[1]].map(f32::to_bits));
    Some((bounds.map(f32::to_bits), relative))
}

fn surface_identity(
    kind: SurfaceDagNodeKind,
    owner: NodeKey,
    stable_id: u64,
) -> RetainedSurfaceRasterIdentity {
    let (role, color_key) = match kind {
        SurfaceDagNodeKind::Transform(_) => (
            RetainedSurfaceRasterRole::Transform,
            crate::view::base_component::transformed_layer_stable_key(stable_id),
        ),
        SurfaceDagNodeKind::Effect(_) => (
            RetainedSurfaceRasterRole::PropertyEffect,
            crate::view::base_component::isolation_layer_stable_key(stable_id),
        ),
        SurfaceDagNodeKind::ScrollContent { .. } => (
            RetainedSurfaceRasterRole::ScrollContent,
            crate::view::base_component::scroll_content_layer_stable_key(stable_id),
        ),
    };
    RetainedSurfaceRasterIdentity {
        boundary_root: owner,
        stable_id,
        color_key,
        role,
    }
}

// Keep only receiver clips that belong to the surface owner's recorded scope.
// A deeper transition witness may add descendant clips; an earlier consumed
// boundary may remove ancestor clips. Intersecting the two chains preserves
// both obligations without reintroducing a detached scrollport.
// Both chains are innermost-first parent paths in the same clip forest.
// A common node therefore implies a common suffix: `find` selects the deepest
// common clip, whose remaining ancestry is exactly the chain intersection.
fn artifact_surface_receiver_clip(
    owner_clip: Option<ClipNodeId>,
    transitioned_clip: Option<ClipNodeId>,
    clips: &FxHashMap<ClipNodeId, ClipNodeSnapshot>,
) -> Option<Option<ClipNodeId>> {
    let (_, owner_chain) = resolve_artifact_surface_clip(owner_clip, clips)?;
    let (_, transitioned_chain) = resolve_artifact_surface_clip(transitioned_clip, clips)?;
    Some(
        transitioned_chain
            .iter()
            .find(|clip| {
                owner_chain
                    .iter()
                    .any(|owner_clip| owner_clip.id == clip.id)
            })
            .map(|clip| clip.id),
    )
}

#[cfg(test)]
mod subtree_receiver_clip_tests;

#[cfg(test)]
mod transform_projection_tests;

fn surface_composite_geometry(
    node: &super::SurfaceDagNode,
    owner_clip: Option<ClipNodeId>,
    raster_source_bounds_bits: [u32; 4],
    receiver_source_bounds_bits: [u32; 4],
    receiver: SurfaceDagExecutionTargetId,
    receiver_paint_offset: [f32; 2],
    context: ArtifactSurfaceRasterContext,
    transforms: &FxHashMap<TransformNodeId, TransformNodeSnapshot>,
    effects: &FxHashMap<EffectNodeId, EffectNodeSnapshot>,
    scrolls: &FxHashMap<ScrollNodeId, ScrollNodeSnapshot>,
    clips: &FxHashMap<ClipNodeId, ClipNodeSnapshot>,
    closure: Option<&SurfaceDagClipClosureProjection>,
    spatial: &super::ArtifactSpatialProjection,
    folded_raster_translation: [f32; 2],
) -> Result<ArtifactSurfaceCompositeGeometryStamp, ArtifactSurfaceRasterPlanError> {
    // A surface composites under its boundary clip and every clip above it:
    // a scroll's contents clip, or the clip a transform or effect's owner
    // paints under. Its receiver keeps the part of that chain inside its own
    // raster; every clip below the boundary lies inside this one.
    let receiver_clip = match node.kind() {
        // Consumption transitions may borrow the deepest descendant witness's
        // non-consumed dimensions. That descendant's self clip is raster-local
        // inside this surface, not an outer composite clip. Only the surface
        // owner's recorded paint scope can supply this receiver boundary.
        SurfaceDagNodeKind::ScrollContent { .. } => closure
            .map(SurfaceDagClipClosureProjection::receiver_clip)
            .map(Some)
            .unwrap_or_else(|| {
                artifact_surface_receiver_clip(owner_clip, node.transition().clip.to, clips)
            })
            .ok_or(ArtifactSurfaceRasterPlanError::InvalidReceiverClip(
                node.id(),
            ))?,
        SurfaceDagNodeKind::Transform(_) | SurfaceDagNodeKind::Effect(_) => owner_clip,
    };
    let composite_clip = match node.kind() {
        // ScrollContent consumes its contents clip while detaching the raster,
        // so the closure identity remains the post-transition parent while
        // the final composite still clips the complete surface to the consumed
        // scrollport boundary.
        SurfaceDagNodeKind::ScrollContent { contents_clip, .. } => Some(contents_clip),
        SurfaceDagNodeKind::Transform(_) | SurfaceDagNodeKind::Effect(_) => receiver_clip,
    };
    let (resolved_receiver_clip, receiver_clip_chain) =
        resolve_artifact_surface_clip(composite_clip, clips).ok_or(
            ArtifactSurfaceRasterPlanError::InvalidReceiverClip(node.id()),
        )?;
    let resolved_receiver_clip = if matches!(receiver, SurfaceDagExecutionTargetId::SceneRoot(_)) {
        let resolved_receiver_clip =
            translate_artifact_surface_logical_clip(resolved_receiver_clip, receiver_paint_offset)
                .ok_or(ArtifactSurfaceRasterPlanError::InvalidReceiverClip(
                    node.id(),
                ))?;
        artifact_surface_terminal_clip(
            resolved_receiver_clip,
            &receiver_clip_chain,
            context.incoming_scissor,
        )
    } else {
        // A surface receiver never mixes in the frame scissor. The resolver
        // has already applied every intra-chain Replace/Intersect operation,
        // so no chain fact remains for the executor to re-derive.
        resolved_receiver_clip
    };
    let resolved_receiver_clip = ArtifactSurfaceResolvedClip::from_logical(resolved_receiver_clip);
    match node.kind() {
        SurfaceDagNodeKind::Transform(transform) => {
            transforms.get(&transform).ok_or(
                ArtifactSurfaceRasterPlanError::MissingSurfaceSnapshot(node.id()),
            )?;
            // This snapshot is owner-only, not ancestor-composed (see
            // DerivedSpatialProjection). Receiver rasters share the logical
            // layout coordinate space before their own transform is applied.
            // Dividing by a receiver transform here would cancel an ancestor
            // that has not yet been applied. Each retained boundary applies
            // its authored matrix exactly once; raster-origin rebasing is
            // handled separately by the frozen quad and destination origin.
            let receiver_transform = spatial.owner_viewport_transform(transform).ok_or(
                ArtifactSurfaceRasterPlanError::MissingSurfaceSnapshot(node.id()),
            )?;
            // A folded scroll rasters this content in scrolled-content space;
            // the matrix maps the layout positions the content scrolled to.
            let [dx, dy] = folded_raster_translation;
            let projection = translated_bounds_bits(receiver_source_bounds_bits, [-dx, -dy])
                .filter(|_| receiver_transform.is_finite())
                .and_then(|layout_source_bounds_bits| {
                    transform_destination_projection(
                        layout_source_bounds_bits,
                        receiver_transform,
                        receiver_paint_offset,
                    )
                });
            let (destination_bounds_bits, quad_offset_bits, receiver_transform, culled) =
                match projection {
                    Some((destination, quad)) => (destination, quad, receiver_transform, false),
                    // A plane seen edge-on, collapsed, or with a corner on the
                    // eye plane has no 2D quad. Nothing of it is composited;
                    // an untransformed stand-in keeps every later placement
                    // finite.
                    None => {
                        let (destination, quad) = transform_destination_projection(
                            receiver_source_bounds_bits,
                            glam::Mat4::IDENTITY,
                            receiver_paint_offset,
                        )
                        .ok_or(
                            ArtifactSurfaceRasterPlanError::InvalidSurfaceBounds(node.id()),
                        )?;
                        (destination, quad, glam::Mat4::IDENTITY, true)
                    }
                };
            Ok(ArtifactSurfaceCompositeGeometryStamp::Transform {
                source_bounds_bits: raster_source_bounds_bits,
                destination_bounds_bits,
                receiver_transform_bits: receiver_transform.to_cols_array().map(f32::to_bits),
                quad_offset_bits,
                receiver_clip,
                resolved_receiver_clip,
                culled,
            })
        }
        SurfaceDagNodeKind::Effect(effect) => {
            let snapshot = effects.get(&effect).copied().ok_or(
                ArtifactSurfaceRasterPlanError::MissingSurfaceSnapshot(node.id()),
            )?;
            let mut destination = receiver_source_bounds_bits.map(f32::from_bits);
            destination[0] += receiver_paint_offset[0];
            destination[1] += receiver_paint_offset[1];
            if destination.into_iter().any(|value| !value.is_finite())
                || !snapshot.opacity.is_finite()
                || !(0.0..=1.0).contains(&snapshot.opacity)
                || snapshot.generation == 0
            {
                return Err(ArtifactSurfaceRasterPlanError::InvalidSurfaceBounds(
                    node.id(),
                ));
            }
            Ok(ArtifactSurfaceCompositeGeometryStamp::Effect {
                source_bounds_bits: raster_source_bounds_bits,
                destination_bounds_bits: destination.map(f32::to_bits),
                opacity_bits: snapshot.opacity.to_bits(),
                generation: snapshot.generation,
                receiver_clip,
                resolved_receiver_clip,
            })
        }
        SurfaceDagNodeKind::ScrollContent { scroll, .. } => {
            let snapshot = scrolls.get(&scroll).copied().ok_or(
                ArtifactSurfaceRasterPlanError::MissingSurfaceSnapshot(node.id()),
            )?;
            let offset = [snapshot.offset.x, snapshot.offset.y];
            let mut destination = receiver_source_bounds_bits.map(f32::from_bits);
            destination[0] = destination[0] - offset[0] + receiver_paint_offset[0];
            destination[1] = destination[1] - offset[1] + receiver_paint_offset[1];
            if destination.into_iter().any(|value| !value.is_finite()) || snapshot.generation == 0 {
                return Err(ArtifactSurfaceRasterPlanError::InvalidSurfaceBounds(
                    node.id(),
                ));
            }
            Ok(ArtifactSurfaceCompositeGeometryStamp::ScrollContent {
                source_bounds_bits: raster_source_bounds_bits,
                destination_bounds_bits: destination.map(f32::to_bits),
                offset_bits: offset.map(f32::to_bits),
                generation: snapshot.generation,
                receiver_clip,
                resolved_receiver_clip,
            })
        }
    }
}

fn fold_materialized_composite_boundaries(
    mut geometry: ArtifactSurfaceCompositeGeometryStamp,
    folded_boundaries: &[SurfaceDagNodeId],
    surface_dag: &SurfaceDag,
) -> Result<ArtifactSurfaceCompositeGeometryStamp, ArtifactSurfaceRasterPlanError> {
    let Some(boundary) = folded_boundaries.first().copied() else {
        return Ok(geometry);
    };
    let node = surface_dag
        .nodes()
        .get(boundary.index())
        .filter(|node| node.id() == boundary)
        .ok_or(ArtifactSurfaceRasterPlanError::MissingSurfaceSnapshot(
            boundary,
        ))?;
    let transfer = node
        .transfer()
        .ok_or(ArtifactSurfaceRasterPlanError::InvalidSurfaceBounds(
            boundary,
        ))?;
    // Transform geometry already resolves its accumulated viewport matrix
    // against the final receiver. Other geometry carries only local placement.
    if !matches!(
        geometry,
        ArtifactSurfaceCompositeGeometryStamp::Transform { .. }
    ) {
        let delta = transfer.composite_translation_bits.map(f32::from_bits);
        let destination = match &mut geometry {
            ArtifactSurfaceCompositeGeometryStamp::Effect {
                destination_bounds_bits,
                ..
            }
            | ArtifactSurfaceCompositeGeometryStamp::ScrollContent {
                destination_bounds_bits,
                ..
            } => destination_bounds_bits,
            ArtifactSurfaceCompositeGeometryStamp::Transform { .. } => unreachable!(),
        };
        *destination = translated_bounds_bits(*destination, delta).ok_or(
            ArtifactSurfaceRasterPlanError::InvalidSurfaceBounds(boundary),
        )?;
    }
    // The composite clip already runs through the folded boundary's clips:
    // it starts at this surface's own boundary and the receiver keeps what
    // lies inside its raster.
    Ok(geometry)
}

/// Each surface's visible rectangle in its receiver's space, by execution
/// index: its receiver clip, within what the receiver itself shows. A scene
/// root shows its target. Parents come first, so an unclipped nested surface
/// inherits its parent's visible part. A composite mapping does not depend on
/// content bounds, so unit bounds stand in for the not yet known envelopes.
/// `None` when nothing bounds what a surface shows. This only narrows raster
/// windows; the plan itself still validates every geometry.
fn surface_receiver_visible_rects(
    program: &ArtifactSurfaceDagProgram,
    context: ArtifactSurfaceRasterContext,
    host_placement: &ResolvedArtifactSurfaceHostPlacement,
    transforms: &FxHashMap<TransformNodeId, TransformNodeSnapshot>,
    effects: &FxHashMap<EffectNodeId, EffectNodeSnapshot>,
    scrolls: &FxHashMap<ScrollNodeId, ScrollNodeSnapshot>,
    clips: &FxHashMap<ClipNodeId, ClipNodeSnapshot>,
) -> Vec<Option<[f32; 4]>> {
    let nodes = program.execution_order.nodes();
    let mut receiver_visible = vec![None; nodes.len()];
    // Each surface's visible part in its own raster space, and the translation
    // from its receiver space into that raster space.
    let mut own_visible: Vec<Option<[f32; 4]>> = vec![None; nodes.len()];
    let mut base_deltas = vec![[0.0_f32; 2]; nodes.len()];
    let unit = [0.0_f32, 0.0, 1.0, 1.0];
    for execution in nodes.iter().copied() {
        let index = execution.id().index();
        let visible = (|| {
            let source = execution.source();
            let node = program
                .surface_dag
                .nodes()
                .get(source.index())
                .filter(|node| node.id() == source)?;
            let coverage = program
                .coverage
                .nodes()
                .get(source.index())
                .filter(|coverage| coverage.surface() == source)?;
            let folded_boundaries = program.execution_order.folded_boundaries(execution.id())?;
            base_deltas[index] = materialized_surface_raster_translation(
                source,
                node.kind(),
                folded_boundaries,
                &program.surface_dag,
                scrolls,
            )
            .ok()?;
            let folded =
                folded_surface_raster_translation(folded_boundaries, &program.surface_dag).ok()?;
            let receiver_paint_offset =
                host_placement.receiver_paint_offset(node.target(), execution.receiver())?;
            let geometry = surface_composite_geometry(
                node,
                node.owner_properties().clip,
                unit.map(f32::to_bits),
                unit.map(f32::to_bits),
                execution.receiver(),
                receiver_paint_offset,
                context,
                transforms,
                effects,
                scrolls,
                clips,
                coverage.clip_closure(),
                &program.spatial,
                folded,
            )
            .ok()?;
            let geometry = fold_materialized_composite_boundaries(
                geometry,
                folded_boundaries,
                &program.surface_dag,
            )
            .ok()?;
            let shown_by_receiver = match execution.receiver() {
                SurfaceDagExecutionTargetId::SceneRoot(_) => Some(context.target_logical_rect()),
                SurfaceDagExecutionTargetId::Surface(parent) => {
                    let [dx, dy] = base_deltas[parent.index()];
                    own_visible[parent.index()].map(|[x, y, w, h]| [x - dx, y - dy, w, h])
                }
            };
            let receiver_rect = match geometry.resolved_receiver_clip() {
                ArtifactSurfaceResolvedClip::Scissor(GraphicsPassScissor::Logical(clip)) => {
                    let clip = clip.map(|n| n as f32);
                    Some(shown_by_receiver.map_or(clip, |shown| intersect_rects(clip, shown)))
                }
                ArtifactSurfaceResolvedClip::Empty => Some([0.0; 4]),
                ArtifactSurfaceResolvedClip::Unclipped
                | ArtifactSurfaceResolvedClip::Scissor(GraphicsPassScissor::TargetPhysical(_)) => {
                    shown_by_receiver
                }
            }?;
            receiver_visible[index] = Some(receiver_rect);
            Some(raster_window::receiver_read(
                unit,
                geometry,
                receiver_rect,
                receiver_paint_offset,
                folded,
            ))
        })();
        own_visible[index] = visible.filter(|rect| rect.iter().all(|n| n.is_finite()));
    }
    receiver_visible
}

/// The scale each surface rasters at, by execution index: its receiver's,
/// times the scale its own transform shows content at, so a surface rasters at
/// the resolution it is displayed at. The scene root rasters at the device
/// scale. Parents come first.
fn surface_raster_scales(
    program: &ArtifactSurfaceDagProgram,
    context: ArtifactSurfaceRasterContext,
) -> Vec<f32> {
    let nodes = program.execution_order.nodes();
    let device = context.scale_factor();
    let mut scales = vec![device; nodes.len()];
    for execution in nodes.iter().copied() {
        let receiver = match execution.receiver() {
            SurfaceDagExecutionTargetId::SceneRoot(_) => device,
            SurfaceDagExecutionTargetId::Surface(parent) => scales[parent.index()],
        };
        let own = program
            .surface_dag
            .nodes()
            .get(execution.source().index())
            .filter(|node| node.id() == execution.source())
            .and_then(|node| match node.kind() {
                SurfaceDagNodeKind::Transform(transform) => program
                    .spatial
                    .owner_viewport_transform(transform)
                    .map(transform_display_scale),
                SurfaceDagNodeKind::Effect(_) | SurfaceDagNodeKind::ScrollContent { .. } => None,
            })
            .unwrap_or(1.0);
        // Bounds keep the physical projection of any finite layout
        // representable; the plan still fits each raster to the device limit.
        scales[execution.id().index()] = (receiver * own).clamp(device / 1024.0, device * 256.0);
    }
    scales
}

/// The factor a transform scales content by: the longer of its mapped unit
/// axes, so neither axis is drawn below its displayed resolution. A rotation
/// alone keeps 1.
fn transform_display_scale(matrix: glam::Mat4) -> f32 {
    let x = glam::Vec2::new(matrix.x_axis.x, matrix.x_axis.y).length();
    let y = glam::Vec2::new(matrix.y_axis.x, matrix.y_axis.y).length();
    let scale = x.max(y);
    if !scale.is_finite() || scale <= 0.0 || (scale - 1.0).abs() < 0.000_1 {
        1.0
    } else {
        scale
    }
}

/// The overlap of two rectangles; an empty overlap keeps zero size.
fn intersect_rects(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let x = a[0].max(b[0]);
    let y = a[1].max(b[1]);
    let right = (a[0] + a[2]).min(b[0] + b[2]);
    let bottom = (a[1] + a[3]).min(b[1] + b[3]);
    [x, y, (right - x).max(0.0), (bottom - y).max(0.0)]
}

fn prepare_artifact_surface_raster_plan_from_program(
    program: ArtifactSurfaceDagProgram,
    context: ArtifactSurfaceRasterContext,
    mut cache: Option<&mut PlanningCache>,
) -> Result<PreparedArtifactSurfaceRasterPlan, ArtifactSurfaceRasterPlanError> {
    if let Some(cache) = cache.as_deref_mut() {
        cache.set_raster_environment(&program.artifact);
    }
    let host_placement = program
        .host_placement
        .resolve(context.paint_offset(), context.scale_factor())
        .map_err(TransitionError::SpatialSnapshot)
        .map_err(SurfaceDagError::Transition)
        .map_err(ArtifactSurfaceRasterPlanError::SurfaceDag)?;
    let transforms = program
        .artifact
        .transform_nodes
        .iter()
        .map(|snapshot| (snapshot.id, *snapshot))
        .collect::<FxHashMap<_, _>>();
    let effects = program
        .artifact
        .effect_nodes
        .iter()
        .map(|snapshot| (snapshot.id, *snapshot))
        .collect::<FxHashMap<_, _>>();
    let scrolls = program
        .artifact
        .scroll_nodes
        .iter()
        .map(|snapshot| (snapshot.id, *snapshot))
        .collect::<FxHashMap<_, _>>();
    let clips = program
        .artifact
        .clip_nodes
        .iter()
        .map(|snapshot| (snapshot.id, *snapshot))
        .collect::<FxHashMap<_, _>>();
    let receiver_visible = surface_receiver_visible_rects(
        &program,
        context,
        &host_placement,
        &transforms,
        &effects,
        &scrolls,
        &clips,
    );
    let raster_scales = surface_raster_scales(&program, context);
    let mut prepared_nodes: Vec<Option<PreparedArtifactSurfaceRasterNode>> =
        vec![None; program.execution_order.nodes().len()];
    // The cache budget: GPU source outputs and surface rasters it can hold are
    // kept across frames; the rest are drawn every frame into frame-local
    // targets. Sources claim it first, then surfaces, children before parents.
    // Shader/pipeline driver allocations and sampled assets are not counted.
    let mut gpu_sources = FxHashSet::default();
    let mut transient_gpu_sources = FxHashSet::default();
    let mut total_texture_bytes = 0_u64;
    let mut claim_budget = |bytes: u64| {
        let total = total_texture_bytes
            .checked_add(bytes)
            .filter(|total| *total <= context.max_texture_bytes);
        if let Some(total) = total {
            total_texture_bytes = total;
        }
        total.is_some()
    };
    for op in &program.artifact.ops {
        if let PaintOp::PreparedGpu(op) = op {
            let source = &op.source;
            if gpu_sources.insert(source.id)
                && !claim_budget(source.allocated_bytes(context.max_texture_dimension_2d))
            {
                transient_gpu_sources.insert(source.id.get());
            }
        }
    }

    for execution in program.execution_order.nodes().iter().rev().copied() {
        let source = execution.source();
        let node = program
            .surface_dag
            .nodes()
            .get(source.index())
            .filter(|node| node.id() == source)
            .ok_or(ArtifactSurfaceRasterPlanError::MissingSurfaceSnapshot(
                source,
            ))?;
        let coverage = program
            .coverage
            .nodes()
            .get(source.index())
            .filter(|coverage| coverage.surface() == source)
            .ok_or(ArtifactSurfaceRasterPlanError::MissingCoverageNode(source))?;
        let folded_boundaries = program
            .execution_order
            .folded_boundaries(execution.id())
            .ok_or(ArtifactSurfaceRasterPlanError::MissingSurfaceSnapshot(
                source,
            ))?;
        let base_delta = materialized_surface_raster_translation(
            source,
            node.kind(),
            folded_boundaries,
            &program.surface_dag,
            &scrolls,
        )?;
        let root_delta =
            folded_surface_raster_translation(folded_boundaries, &program.surface_dag)?;
        let target_id = ArtifactSurfaceRasterTargetId::Surface(source);
        let mut raw_bounds = None;
        let mut nested_children = Vec::new();
        for chunk_range in coverage.receiver_mask_envelope_ranges() {
            if let Some(mask_bounds) = artifact_surface_chunk_range_raw_bounds(
                &program.artifact,
                target_id,
                Some(node.target()),
                chunk_range.clone(),
                base_delta,
                root_delta,
            )? {
                append_bounds(&mut raw_bounds, mask_bounds)
                    .ok_or(ArtifactSurfaceRasterPlanError::InvalidSurfaceBounds(source))?;
            }
        }
        for step in coverage.steps() {
            match step {
                ArtifactSurfaceCoverageStep::ArtifactSpan(span) => {
                    if let Some(span_bounds) = artifact_surface_span_raw_bounds(
                        &program.artifact,
                        target_id,
                        Some(node.target()),
                        span,
                        base_delta,
                        root_delta,
                    )? {
                        append_bounds(&mut raw_bounds, span_bounds)
                            .ok_or(ArtifactSurfaceRasterPlanError::InvalidSurfaceBounds(source))?;
                    }
                }
                ArtifactSurfaceCoverageStep::NestedSurface(child_source) => {
                    let child_execution = program
                        .execution_order
                        .execution_id(*child_source)
                        .ok_or(ArtifactSurfaceRasterPlanError::InvalidNestedSurface {
                            parent: source,
                            child: *child_source,
                        })?;
                    let child = prepared_nodes
                        .get(child_execution.index())
                        .and_then(Option::as_ref)
                        .ok_or(ArtifactSurfaceRasterPlanError::InvalidNestedSurface {
                            parent: source,
                            child: *child_source,
                        })?;
                    if child.receiver != SurfaceDagExecutionTargetId::Surface(execution.id()) {
                        return Err(ArtifactSurfaceRasterPlanError::InvalidNestedSurface {
                            parent: source,
                            child: *child_source,
                        });
                    }
                    let child_geometry = child.geometry.pending().ok_or(
                        ArtifactSurfaceRasterPlanError::InvalidNestedSurface {
                            parent: source,
                            child: *child_source,
                        },
                    )?;
                    // Direct spans already carry this surface's base translation.
                    // Seal the child's destination in that same parent space once,
                    // then reuse the exact value for both the raw-bounds union and
                    // final receiver projection. The child's receiver clip is
                    // localized into the same space when it is finalized below.
                    let receiver_destination_bounds_bits = translated_bounds_bits(
                        child_geometry.destination_bounds_bits(),
                        base_delta,
                    )
                    .ok_or(ArtifactSurfaceRasterPlanError::InvalidSurfaceBounds(source))?;
                    append_bounds(&mut raw_bounds, receiver_destination_bounds_bits)
                        .ok_or(ArtifactSurfaceRasterPlanError::InvalidSurfaceBounds(source))?;
                    nested_children.push((child_execution, receiver_destination_bounds_bits));
                }
            }
        }
        // A surface whose content has no area still rasters: one transparent
        // logical pixel composites to nothing.
        let content_bounds_bits = raw_bounds.unwrap_or([0.0_f32, 0.0, 1.0, 1.0].map(f32::to_bits));
        let receiver_paint_offset = host_placement
            .receiver_paint_offset(node.target(), execution.receiver())
            .ok_or(ArtifactSurfaceRasterPlanError::InvalidOwnerTopology {
                target: target_id,
                owner: node.target(),
            })?;
        // The structural program retains each boundary owner's
        // exact endpoints; unrelated paint owners need no per-frame lookup map.
        let owner_state = node.owner_properties();
        let geometry_for_bounds = |raw_source_bounds_bits, source_bounds_bits| {
            let geometry = surface_composite_geometry(
                node,
                owner_state.clip,
                source_bounds_bits,
                raw_source_bounds_bits,
                execution.receiver(),
                receiver_paint_offset,
                context,
                &transforms,
                &effects,
                &scrolls,
                &clips,
                coverage.clip_closure(),
                &program.spatial,
                root_delta,
            )?;
            fold_materialized_composite_boundaries(
                geometry,
                folded_boundaries,
                &program.surface_dag,
            )
        };
        let raster_context = context.with_scale_factor(raster_scales[execution.id().index()]);
        let full_origin = ArtifactSurfaceRasterOriginProjection::new(
            content_bounds_bits,
            raster_context.scale_factor_bits,
        )
        .ok_or(ArtifactSurfaceRasterPlanError::InvalidRasterOrigin(source))?;
        // A fixed working-set threshold makes raster geometry independent of
        // the cache budget: lowering that budget changes what is kept, never
        // what is drawn. The complete envelope survives in the stamp; only a
        // proved finite receiver read can restrict the allocated raster
        // region. A device whose texture limit is below 8192 windows from its
        // own limit.
        const MAX_UNWINDOWED_BYTES: u64 = 32 * 1024 * 1024;
        let full_bytes =
            u64::from(full_origin.target_size[0]) * u64::from(full_origin.target_size[1]) * 12;
        let max_unwindowed_dimension = context.max_texture_dimension_2d.min(8192);
        let window = if full_origin
            .target_size
            .iter()
            .any(|n| *n > max_unwindowed_dimension)
            || full_bytes > MAX_UNWINDOWED_BYTES
        {
            let geometry = geometry_for_bounds(
                content_bounds_bits,
                full_origin.normalized_source_bounds_bits,
            )?;
            raster_window::select(
                content_bounds_bits,
                geometry,
                receiver_visible[execution.id().index()],
                receiver_paint_offset,
                root_delta,
                raster_context,
            )
        } else {
            None
        };
        let raw_source_bounds_bits =
            window.map_or(content_bounds_bits, |window| window.raster_bounds_bits);
        let mut raster_context = raster_context;
        let mut raster_origin = ArtifactSurfaceRasterOriginProjection::new(
            raw_source_bounds_bits,
            raster_context.scale_factor_bits,
        )
        .ok_or(ArtifactSurfaceRasterPlanError::InvalidRasterOrigin(source))?;
        // What the device cannot hold in one texture is drawn at a lower
        // resolution rather than not at all.
        let longest = raster_origin.target_size[0].max(raster_origin.target_size[1]);
        if longest > context.max_texture_dimension_2d {
            let display_scale = raster_context.scale_factor();
            let fit = context.max_texture_dimension_2d as f32 / longest as f32;
            for shrink in [fit, fit * 0.99, fit * 0.98, fit * 0.95, fit * 0.9] {
                raster_context = raster_context.with_scale_factor(display_scale * shrink);
                raster_origin = ArtifactSurfaceRasterOriginProjection::new(
                    raw_source_bounds_bits,
                    raster_context.scale_factor_bits,
                )
                .ok_or(ArtifactSurfaceRasterPlanError::InvalidRasterOrigin(source))?;
                if raster_origin.target_size[0].max(raster_origin.target_size[1])
                    <= context.max_texture_dimension_2d
                {
                    break;
                }
            }
        }
        // Local clips lie below a scrollport and clip only its descendants'
        // content, which rasters `base_delta` away from viewport space.
        let raster_local_clip = |clip: &ClipNodeSnapshot| program.raster_clip(clip, base_delta);
        let mut steps = Vec::with_capacity(coverage.steps().len());
        for step in coverage.steps() {
            match step {
                ArtifactSurfaceCoverageStep::ArtifactSpan(span) => {
                    let local_clips = span
                        .local_clips()
                        .iter()
                        .map(raster_local_clip)
                        .collect::<Option<Vec<_>>>()
                        .ok_or(ArtifactSurfaceRasterPlanError::InvalidCoverageSpan(
                            target_id,
                        ))?;
                    steps.push(PreparedArtifactSurfaceRasterStep::ArtifactSpan(
                        prepare_artifact_surface_span(
                            &program.artifact,
                            target_id,
                            span,
                            ArtifactSurfaceSpanPlacement::Surface {
                                boundary_root: node.target(),
                                base_delta,
                                root_delta,
                                raster_origin,
                                local_clips: &local_clips,
                            },
                            match node.kind() {
                                SurfaceDagNodeKind::Effect(effect) => effects.get(&effect).copied(),
                                SurfaceDagNodeKind::Transform(_)
                                | SurfaceDagNodeKind::ScrollContent { .. } => None,
                            },
                            None,
                            cache.as_deref_mut(),
                        )?,
                    ));
                }
                ArtifactSurfaceCoverageStep::NestedSurface(child_source) => {
                    let child_execution = program
                        .execution_order
                        .execution_id(*child_source)
                        .ok_or(ArtifactSurfaceRasterPlanError::InvalidNestedSurface {
                            parent: source,
                            child: *child_source,
                        })?;
                    steps.push(PreparedArtifactSurfaceRasterStep::NestedSurface(
                        child_execution,
                    ));
                }
            }
        }
        // A nested child keeps only the part of its composite chain below this
        // surface's boundary: that part lies inside this raster, and the rest
        // this surface applies when it composites, in its receiver's space.
        // Scroll content rasters in content space, so a child's clip moves
        // there with `base_delta`, as its destination does.
        let raster_boundary = match node.kind() {
            SurfaceDagNodeKind::ScrollContent { contents_clip, .. } => Some(contents_clip),
            SurfaceDagNodeKind::Transform(_) | SurfaceDagNodeKind::Effect(_) => {
                node.owner_properties().clip
            }
        };
        for (child_execution, receiver_destination_bounds_bits) in nested_children {
            let child = prepared_nodes
                .get_mut(child_execution.index())
                .and_then(Option::as_mut)
                .ok_or(ArtifactSurfaceRasterPlanError::InvalidNestedSurface {
                    parent: source,
                    child: source,
                })?;
            if let Some(raster_boundary) = raster_boundary {
                let child_node = program
                    .surface_dag
                    .nodes()
                    .get(child.source.index())
                    .filter(|node| node.id() == child.source)
                    .ok_or(ArtifactSurfaceRasterPlanError::MissingSurfaceSnapshot(
                        child.source,
                    ))?;
                let composite_clip = match child_node.kind() {
                    SurfaceDagNodeKind::ScrollContent { contents_clip, .. } => Some(contents_clip),
                    SurfaceDagNodeKind::Transform(_) | SurfaceDagNodeKind::Effect(_) => child
                        .geometry
                        .pending()
                        .and_then(ArtifactSurfaceCompositeGeometryStamp::receiver_clip),
                };
                child
                    .geometry
                    .localize_receiver_clip(composite_clip, raster_boundary, &clips, |snapshot| {
                        program
                            .raster_clip(snapshot, base_delta)?
                            .geometry
                            .viewport_scissor()
                    })
                    .ok_or(ArtifactSurfaceRasterPlanError::InvalidReceiverClip(
                        child.source,
                    ))?;
            }
            child
                .geometry
                .finalize_surface_receiver(raster_origin, receiver_destination_bounds_bits)
                .ok_or(ArtifactSurfaceRasterPlanError::InvalidRasterOrigin(
                    child.source,
                ))?;
        }

        let source_bounds_bits = raster_origin.normalized_source_bounds_bits;
        let identity = surface_identity(node.kind(), node.target(), node.stable_id());
        // The projection's own physical size: logical bounds at a scale that
        // is not a whole number cannot round-trip to the same pixel count.
        let color = crate::view::frame_graph::texture_resource::TextureDesc::new(
            raster_origin.target_size[0],
            raster_origin.target_size[1],
            context.target_format,
            wgpu::TextureDimension::D2,
        )
        .with_raster_scale(raster_context.scale_factor());
        let (color, depth) = crate::view::base_component::persistent_target_texture_descriptors(
            color,
            identity.color_key,
        );
        let color_bytes = crate::view::raster_cost::texture_desc_payload_bytes(&color);
        let depth_bytes = crate::view::raster_cost::texture_desc_payload_bytes(&depth);
        let retained = color_bytes
            .bytes
            .checked_add(depth_bytes.bytes)
            .is_some_and(&mut claim_budget);
        let target = RetainedSurfaceRasterInputs {
            color,
            depth,
            scale_factor_bits: raster_context.scale_factor_bits,
            source_bounds_bits,
        };
        let geometry = geometry_for_bounds(raw_source_bounds_bits, source_bounds_bits)?;
        prepared_nodes[execution.id().index()] = Some(PreparedArtifactSurfaceRasterNode {
            window,
            execution_id: execution.id(),
            source,
            receiver: execution.receiver(),
            identity,
            target,
            raster_origin,
            geometry: ArtifactSurfaceCompositeGeometryState::Pending(geometry),
            clip_closure: match coverage.clip_closure() {
                Some(closure) => Some(closure.placed_local_clips(raster_local_clip).ok_or(
                    ArtifactSurfaceRasterPlanError::InvalidCoverageSpan(target_id),
                )?),
                None => None,
            },
            steps,
            retained,
        });
    }
    for node in prepared_nodes.iter_mut().flatten() {
        match node.receiver {
            SurfaceDagExecutionTargetId::SceneRoot(_) => {
                node.geometry.finalize_scene_root().ok_or(
                    ArtifactSurfaceRasterPlanError::InvalidRasterOrigin(node.source),
                )?
            }
            SurfaceDagExecutionTargetId::Surface(_) => {
                if node.geometry.finalized().is_none() {
                    return Err(ArtifactSurfaceRasterPlanError::InvalidRasterOrigin(
                        node.source,
                    ));
                }
            }
        }
    }
    let nodes = prepared_nodes
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            ArtifactSurfaceRasterPlanError::SurfaceDag(
                SurfaceDagError::ExecutionNodeOrdinalOverflow(
                    program.execution_order.nodes().len(),
                ),
            )
        })?;

    let mut roots = Vec::with_capacity(program.coverage.roots().len());
    for root in program.coverage.roots() {
        let target = ArtifactSurfaceRasterTargetId::SceneRoot(root.scene_root());
        let mut steps = Vec::with_capacity(root.steps().len());
        for step in root.steps() {
            match step {
                ArtifactSurfaceCoverageStep::ArtifactSpan(span) => steps.push(
                    PreparedArtifactSurfaceRasterStep::ArtifactSpan(prepare_artifact_surface_span(
                        &program.artifact,
                        target,
                        span,
                        ArtifactSurfaceSpanPlacement::SceneRoot(&host_placement),
                        None,
                        context.incoming_scissor,
                        cache.as_deref_mut(),
                    )?),
                ),
                ArtifactSurfaceCoverageStep::NestedSurface(child_source) => {
                    let child_execution = program
                        .execution_order
                        .execution_id(*child_source)
                        .ok_or(ArtifactSurfaceRasterPlanError::InvalidNestedSurface {
                            parent: *child_source,
                            child: *child_source,
                        })?;
                    let child = nodes.get(child_execution.index()).ok_or(
                        ArtifactSurfaceRasterPlanError::InvalidNestedSurface {
                            parent: *child_source,
                            child: *child_source,
                        },
                    )?;
                    if child.receiver != SurfaceDagExecutionTargetId::SceneRoot(root.scene_root()) {
                        return Err(ArtifactSurfaceRasterPlanError::InvalidNestedSurface {
                            parent: *child_source,
                            child: *child_source,
                        });
                    }
                    steps.push(PreparedArtifactSurfaceRasterStep::NestedSurface(
                        child_execution,
                    ));
                }
            }
        }
        roots.push(PreparedArtifactSurfaceRasterRoot {
            scene_root: root.scene_root(),
            steps,
        });
    }

    Ok(PreparedArtifactSurfaceRasterPlan {
        #[cfg(test)]
        context,
        roots,
        nodes,
        transient_gpu_sources,
        #[cfg(test)]
        materialization_decisions: program.execution_order.decisions().to_vec(),
    })
}

pub(crate) fn prepare_artifact_surface_raster_plan(
    artifact: PaintArtifact,
    context: ArtifactSurfaceRasterContext,
) -> Result<PreparedArtifactSurfaceRasterPlan, ArtifactSurfaceRasterPlanError> {
    let _profile = crate::view::paint::work_profile::scope("prepare_artifact_surface_raster_plan");
    let program = derive_artifact_surface_dag_program(artifact, context.scale_factor(), None)
        .map_err(ArtifactSurfaceRasterPlanError::ArtifactProgram)?;
    prepare_artifact_surface_raster_plan_from_program(program, context, None)
}

pub(crate) fn prepare_artifact_surface_raster_plan_cached(
    artifact: PaintArtifact,
    context: ArtifactSurfaceRasterContext,
    cache: &mut PlanningCache,
) -> Result<PreparedArtifactSurfaceRasterPlan, ArtifactSurfaceRasterPlanError> {
    let _profile = crate::view::paint::work_profile::scope("prepare_artifact_surface_raster_plan");
    cache.begin();
    let result =
        derive_artifact_surface_dag_program(artifact, context.scale_factor(), Some(cache))
            .map_err(ArtifactSurfaceRasterPlanError::ArtifactProgram)
            .and_then(|program| {
                prepare_artifact_surface_raster_plan_from_program(program, context, Some(cache))
            });
    cache.finish(result.is_ok());
    result
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArtifactSurfaceResidentSealError {
    MissingPreparedNode(SurfaceDagExecutionNodeId),
    DuplicateResidentKey(RetainedSurfaceResidentKey),
    InvalidArtifactSpan {
        surface: SurfaceDagNodeId,
        step_index: usize,
    },
    InvalidNestedSurface {
        parent: SurfaceDagNodeId,
        child: SurfaceDagExecutionNodeId,
    },
}

/// Graph-inert full-set identity for every resident in one artifact Surface
/// DAG. It deliberately reuses [`RetainedSurfaceRasterStamp`], because the
/// resident pool and [`RetainedSurfaceResidentKey`] type must remain a single
/// authority. Artifact residents use the generic role-tagged `Surface`
/// variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SealedArtifactSurfaceResidentEntry {
    resident_key: RetainedSurfaceResidentKey,
    stamp: RetainedSurfaceRasterStamp,
}

impl SealedArtifactSurfaceResidentEntry {
    pub(crate) fn resident_key(&self) -> RetainedSurfaceResidentKey {
        self.resident_key
    }

    pub(crate) fn stamp(&self) -> &RetainedSurfaceRasterStamp {
        &self.stamp
    }

    pub(crate) fn into_parts(self) -> (RetainedSurfaceResidentKey, RetainedSurfaceRasterStamp) {
        (self.resident_key, self.stamp)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SealedArtifactSurfaceResidentSet {
    ordered_entries: std::sync::Arc<Vec<SealedArtifactSurfaceResidentEntry>>,
}

impl SealedArtifactSurfaceResidentSet {
    pub(crate) fn ordered_entries(&self) -> &[SealedArtifactSurfaceResidentEntry] {
        &self.ordered_entries
    }

    pub(crate) fn len(&self) -> usize {
        self.ordered_entries.len()
    }

    pub(crate) fn into_ordered_entries(self) -> Vec<SealedArtifactSurfaceResidentEntry> {
        std::sync::Arc::unwrap_or_clone(self.ordered_entries)
    }

    /// The entries `keep` selects, in order: what the surface cache holds
    /// after a frame that drew some surfaces without keeping them. Each entry
    /// stays as sealed; the subset is only committed, never emitted.
    pub(crate) fn retaining(
        self,
        keep: impl FnMut(&SealedArtifactSurfaceResidentEntry) -> bool,
    ) -> Self {
        Self {
            ordered_entries: std::sync::Arc::new(
                self.into_ordered_entries()
                    .into_iter()
                    .filter(keep)
                    .collect(),
            ),
        }
    }

    #[cfg(test)]
    pub(crate) fn resident_keys_for_test(&self) -> Vec<RetainedSurfaceResidentKey> {
        self.ordered_entries
            .iter()
            .map(SealedArtifactSurfaceResidentEntry::resident_key)
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn nested_dependencies_for_test(
        &self,
    ) -> Vec<(
        SurfaceDagExecutionNodeId,
        SurfaceDagExecutionNodeId,
        ArtifactSurfaceResolvedClip,
        u32,
        u32,
    )> {
        self.ordered_entries
            .iter()
            .flat_map(|entry| {
                let program = entry
                    .stamp
                    .artifact_surface_program
                    .as_ref()
                    .expect("artifact resident owns an artifact program");
                program.steps.iter().filter_map(move |step| match step {
                    ArtifactSurfaceRasterProgramStepStamp::NestedSurface(dependency) => Some((
                        program.execution_id,
                        dependency.child_execution_id,
                        dependency.child_composite_geometry.resolved_receiver_clip(),
                        dependency.parent_opaque_order_before,
                        dependency.parent_opaque_order_after,
                    )),
                    ArtifactSurfaceRasterProgramStepStamp::ArtifactSpan(_) => None,
                })
            })
            .collect()
    }
}

fn artifact_surface_nested_parent_opaque_after(
    before: u32,
    child: &RetainedSurfaceRasterStamp,
    child_geometry: ArtifactSurfaceCompositeGeometryStamp,
) -> u32 {
    if child.identity.role == RetainedSurfaceRasterRole::PropertyEffect
        || child_geometry.resolved_receiver_clip() == ArtifactSurfaceResolvedClip::Empty
    {
        before
    } else {
        before.max(child.opaque_order_span.end)
    }
}

fn seal_artifact_surface_program_span(
    surface: SurfaceDagNodeId,
    step_index: usize,
    start: u32,
    span: &PreparedArtifactSurfaceRasterSpan,
) -> Result<ArtifactSurfaceRasterProgramSpanStamp, ArtifactSurfaceResidentSealError> {
    let end = start.checked_add(span.opaque_order_count).ok_or(
        ArtifactSurfaceResidentSealError::InvalidArtifactSpan {
            surface,
            step_index,
        },
    )?;
    let chunks = span
        .chunks
        .iter()
        .map(|chunk| {
            // Global paint/composite revisions include layout position and
            // ancestor placement. After localization, complete typed payload
            // identity, bounds, clips, order and nested dependencies identify
            // the actual raster. Do not add those placement proxies to equality.
            let opaque_order_count = artifact_surface_chunk_opaque_order_count(
                chunk.clip_schedule,
                &chunk.localized_ops,
            )?;
            Some(ArtifactSurfaceRasterProgramChunkStamp {
                raster: RetainedSurfaceChunkStamp {
                    id: chunk.source.id,
                    owner: chunk.source.owner,
                    bounds_bits: chunk.localized_bounds_bits,
                    clip: chunk.localized_state.clip,
                    topology_revision: chunk.content_revision.topology_revision,
                    payload_identity: chunk.localized_payload.clone(),
                    op_count: chunk.localized_ops.len(),
                },
                clip_schedule: chunk.clip_schedule,
                opaque_order_count,
            })
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(ArtifactSurfaceResidentSealError::InvalidArtifactSpan {
            surface,
            step_index,
        })?;
    Ok(ArtifactSurfaceRasterProgramSpanStamp {
        step_index,
        owner_topology: span.owner_topology.clone(),
        clip_nodes: span.local_clips.clone(),
        op_count: chunks.iter().map(|chunk| chunk.raster.op_count).sum(),
        chunks,
        opaque_order_span: start..end,
    })
}

/// Borrows a graph-inert raster plan and seals one ordered `(resident key,
/// stamp)` pair per execution node. The plan retains the localized paint ops;
/// the identity-only resident set cannot replace it as raster input.
fn seal_artifact_surface_resident_set(
    plan: &PreparedArtifactSurfaceRasterPlan,
) -> Result<SealedArtifactSurfaceResidentSet, ArtifactSurfaceResidentSealError> {
    let mut sealed: Vec<Option<SealedArtifactSurfaceResidentEntry>> = vec![None; plan.nodes.len()];
    let mut resident_keys = FxHashSet::default();
    for node in plan.nodes.iter().rev() {
        let mut cursor = 0_u32;
        let mut program_steps = Vec::with_capacity(node.steps.len());
        let mut owner_topology = Vec::new();
        let mut chunks = Vec::new();
        let mut op_count = 0_usize;
        for (step_index, step) in node.steps.iter().enumerate() {
            match step {
                PreparedArtifactSurfaceRasterStep::ArtifactSpan(span) => {
                    let stamp = span
                        .seal_cache
                        .seal(node.source, step_index, cursor, span)?;
                    cursor = stamp.opaque_order_span.end;
                    owner_topology.extend(stamp.owner_topology.iter().copied());
                    chunks.extend(stamp.chunks.iter().map(|chunk| chunk.raster.clone()));
                    op_count = op_count.checked_add(stamp.op_count).ok_or(
                        ArtifactSurfaceResidentSealError::InvalidArtifactSpan {
                            surface: node.source,
                            step_index,
                        },
                    )?;
                    program_steps.push(ArtifactSurfaceRasterProgramStepStamp::ArtifactSpan(stamp));
                }
                PreparedArtifactSurfaceRasterStep::NestedSurface(child_execution_id) => {
                    let child_node = plan
                        .nodes
                        .get(child_execution_id.index())
                        .filter(|child| child.execution_id == *child_execution_id)
                        .ok_or(ArtifactSurfaceResidentSealError::InvalidNestedSurface {
                            parent: node.source,
                            child: *child_execution_id,
                        })?;
                    let child = sealed
                        .get(child_execution_id.index())
                        .and_then(Option::as_ref)
                        .ok_or(ArtifactSurfaceResidentSealError::InvalidNestedSurface {
                            parent: node.source,
                            child: *child_execution_id,
                        })?;
                    let child_stamp = &child.stamp;
                    let child_program = child_stamp.artifact_surface_program.as_ref().ok_or(
                        ArtifactSurfaceResidentSealError::InvalidNestedSurface {
                            parent: node.source,
                            child: *child_execution_id,
                        },
                    )?;
                    if child_program.receiver
                        != SurfaceDagExecutionTargetId::Surface(node.execution_id)
                    {
                        return Err(ArtifactSurfaceResidentSealError::InvalidNestedSurface {
                            parent: node.source,
                            child: *child_execution_id,
                        });
                    }
                    let parent_after = artifact_surface_nested_parent_opaque_after(
                        cursor,
                        child_stamp,
                        child_node.geometry(),
                    );
                    program_steps.push(ArtifactSurfaceRasterProgramStepStamp::NestedSurface(
                        ArtifactSurfaceNestedRasterDependency {
                            step_index,
                            child_execution_id: *child_execution_id,
                            // Child stamps are immutable after sealing. Sharing nested
                            // dependencies prevents recursive subtree copies when a
                            // parent stamp is retained; equality still compares values.
                            child_stamp: std::sync::Arc::new(child_stamp.clone()),
                            child_composite_geometry: child_node.geometry(),
                            parent_opaque_order_before: cursor,
                            parent_opaque_order_after: parent_after,
                        },
                    ));
                    cursor = parent_after;
                }
            }
        }
        let clip_nodes = node
            .clip_closure
            .as_ref()
            .map(|closure| closure.local_clips().to_vec())
            .unwrap_or_default();
        let local_clip_generation_semantics = (node.identity.role
            == RetainedSurfaceRasterRole::ScrollContent
            && !clip_nodes.is_empty())
        .then_some(LocalClipGenerationSemantics::ArtifactLive);
        let stamp = RetainedSurfaceRasterStamp {
            identity: node.identity,
            target: node.target.clone(),
            owner_topology,
            clip_nodes,
            chunks,
            op_count,
            opaque_order_span: 0..cursor,
            local_clip_generation_semantics,
            artifact_surface_program: Some(ArtifactSurfaceRasterProgramStamp {
                window: node.window,
                execution_id: node.execution_id,
                source: node.source,
                receiver: node.receiver,
                steps: program_steps,
            }),
        };
        let resident_key = stamp.identity.artifact_surface_resident_key();
        if !resident_keys.insert(resident_key) {
            return Err(ArtifactSurfaceResidentSealError::DuplicateResidentKey(
                resident_key,
            ));
        }
        let slot = sealed.get_mut(node.execution_id.index()).ok_or(
            ArtifactSurfaceResidentSealError::MissingPreparedNode(node.execution_id),
        )?;
        if slot
            .replace(SealedArtifactSurfaceResidentEntry {
                resident_key,
                stamp,
            })
            .is_some()
        {
            return Err(ArtifactSurfaceResidentSealError::MissingPreparedNode(
                node.execution_id,
            ));
        }
    }
    let ordered_entries = sealed
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            entry.ok_or(ArtifactSurfaceResidentSealError::MissingPreparedNode(
                plan.nodes[index].execution_id,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SealedArtifactSurfaceResidentSet {
        ordered_entries: std::sync::Arc::new(ordered_entries),
    })
}

/// Single-owner preparation capability for the future artifact Surface DAG
/// executor. It preserves localized paint ops and the exact ordered resident
/// pairs together; neither half can be independently substituted after seal.
#[derive(Debug)]
pub(crate) struct PreparedArtifactSurfaceFrame {
    raster_plan: PreparedArtifactSurfaceRasterPlan,
    residents: SealedArtifactSurfaceResidentSet,
}

impl PreparedArtifactSurfaceFrame {
    pub(crate) fn raster_plan(&self) -> &PreparedArtifactSurfaceRasterPlan {
        &self.raster_plan
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        PreparedArtifactSurfaceRasterPlan,
        SealedArtifactSurfaceResidentSet,
    ) {
        (self.raster_plan, self.residents)
    }
}

pub(crate) fn seal_prepared_artifact_surface_frame(
    raster_plan: PreparedArtifactSurfaceRasterPlan,
) -> Result<PreparedArtifactSurfaceFrame, ArtifactSurfaceResidentSealError> {
    let _profile = crate::view::paint::work_profile::scope("seal_prepared_artifact_surface_frame");
    let residents = seal_artifact_surface_resident_set(&raster_plan)?;
    Ok(PreparedArtifactSurfaceFrame {
        raster_plan,
        residents,
    })
}

fn retained_surface_op_opaque_order_count(op: &PaintOp) -> u32 {
    // The artifact clip schedule relies on this exhaustive match: a retained
    // outer-shadow prefix is structurally non-opaque and advances no opaque
    // order before its clipped suffix begins.
    match op {
        PaintOp::DrawRect(op) => u32::from(retained_surface_rect_is_opaque(&op.params, op.mode)),
        PaintOp::PreparedInlineIfcDecoration(op) => {
            u32::from(retained_surface_rect_is_opaque(
                &op.fill,
                crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
            )) + op.border.as_ref().map_or(0, |border| {
                u32::from(retained_surface_rect_is_opaque(
                    border,
                    crate::view::render_pass::draw_rect_pass::RectRenderMode::BorderOnly,
                ))
            })
        }
        PaintOp::PreparedShadow(_)
        | PaintOp::PreparedScrollbarOverlay(_)
        | PaintOp::PreparedText(_)
        | PaintOp::PreparedImage(_)
        | PaintOp::PreparedSvg(_)
        | PaintOp::PreparedGpu(_) => 0,
    }
}

fn retained_surface_rect_is_opaque(
    params: &crate::view::render_pass::draw_rect_pass::RectPassParams,
    mode: crate::view::render_pass::draw_rect_pass::RectRenderMode,
) -> bool {
    let mut pass = DrawRectPass::new(params.clone(), Default::default(), Default::default());
    pass.set_render_mode(mode);
    pass.is_opaque_candidate()
}

/// Emits one already-prepared paint operation without deriving clip, mask, or
/// surface ownership. The Surface DAG executor and the direct-command test
/// compiler share this exhaustive primitive so their payload behavior cannot
/// drift while their scheduling remains independent.
fn emit_artifact_surface_paint_op(op: &PaintOp, graph: &mut FrameGraph, ctx: &mut UiBuildContext) {
    match op {
        PaintOp::PreparedGpu(op) => op.source.emit(graph, ctx, op.params),
        PaintOp::DrawRect(op) => {
            let mut pass = DrawRectPass::new(
                op.params.clone(),
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            pass.set_render_mode(op.mode);
            ctx.emit_draw_rect_pass(graph, pass);
        }
        PaintOp::PreparedInlineIfcDecoration(op) => {
            let mut fill = DrawRectPass::new(
                op.fill.clone(),
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            fill.set_render_mode(
                crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
            );
            ctx.emit_draw_rect_pass(graph, fill);
            if let Some(params) = &op.border {
                let mut border = DrawRectPass::new(
                    params.clone(),
                    DrawRectInput::default(),
                    DrawRectOutput::default(),
                );
                border.set_render_mode(
                    crate::view::render_pass::draw_rect_pass::RectRenderMode::BorderOnly,
                );
                ctx.emit_draw_rect_pass(graph, border);
            }
        }
        PaintOp::PreparedShadow(op) => {
            let output = ctx.current_target().unwrap_or_else(|| {
                let target = ctx.allocate_target(graph);
                ctx.set_current_target(target);
                target
            });
            let viewport = ctx.viewport();
            if build_shadow_module(
                graph,
                ShadowModuleSpec {
                    shape: op.shape,
                    params: op.params,
                    viewport_width: viewport.target_width(),
                    viewport_height: viewport.target_height(),
                    scale_factor: viewport.scale_factor(),
                    pass_context: ctx.graphics_pass_context(),
                    output,
                },
            ) {
                ctx.set_current_target(output);
            }
        }
        PaintOp::PreparedScrollbarOverlay(op) => {
            emit_prepared_scrollbar_shadow(&op.track_shadow, graph, ctx);
            let mut track = DrawRectPass::new(
                op.track.params.clone(),
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            track.set_render_mode(op.track.mode);
            ctx.emit_draw_rect_pass(graph, track);
            emit_prepared_scrollbar_shadow(&op.thumb_shadow, graph, ctx);
            let mut thumb = DrawRectPass::new(
                op.thumb.params.clone(),
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            thumb.set_render_mode(op.thumb.mode);
            ctx.emit_draw_rect_pass(graph, thumb);
            if let Some((track_shadow, track, thumb_shadow, thumb)) = op.secondary_axis() {
                emit_prepared_scrollbar_shadow(track_shadow, graph, ctx);
                let track_mode = track.mode;
                let mut track = DrawRectPass::new(
                    track.params.clone(),
                    DrawRectInput::default(),
                    DrawRectOutput::default(),
                );
                track.set_render_mode(track_mode);
                ctx.emit_draw_rect_pass(graph, track);
                emit_prepared_scrollbar_shadow(thumb_shadow, graph, ctx);
                let thumb_mode = thumb.mode;
                let mut thumb = DrawRectPass::new(
                    thumb.params.clone(),
                    DrawRectInput::default(),
                    DrawRectOutput::default(),
                );
                thumb.set_render_mode(thumb_mode);
                ctx.emit_draw_rect_pass(graph, thumb);
            }
        }
        PaintOp::PreparedText(op) => {
            let Some(input_target) = ctx.current_target() else {
                return;
            };
            graph.add_graphics_pass(TextPreparedInputPass::new(
                op.params.clone(),
                TextInput {
                    pass_context: ctx.graphics_pass_context(),
                },
                TextOutput {
                    render_target: input_target,
                },
            ));
            ctx.set_current_target(input_target);
        }
        PaintOp::PreparedImage(op) => {
            let Some(input_target) = ctx.current_target() else {
                return;
            };
            graph.add_graphics_pass(TextureCompositePass::new(
                op.params,
                TextureCompositeInput::from_sampled_texture(
                    op.upload.clone(),
                    Default::default(),
                    ctx.graphics_pass_context(),
                ),
                TextureCompositeOutput {
                    render_target: input_target,
                },
            ));
            ctx.set_current_target(input_target);
        }
        PaintOp::PreparedSvg(op) => {
            let Some(input_target) = ctx.current_target() else {
                return;
            };
            graph.add_graphics_pass(TextureCompositePass::new(
                op.params,
                TextureCompositeInput::from_sampled_texture(
                    op.upload.clone(),
                    Default::default(),
                    ctx.graphics_pass_context(),
                ),
                TextureCompositeOutput {
                    render_target: input_target,
                },
            ));
            ctx.set_current_target(input_target);
        }
    }
}

/// Returns the length of the outer-shadow prefix for the one deliberately
/// narrow self-clip grammar supported by the retained compiler.  Outer
/// shadows paint against the incoming parent scissor; the owner's exact
/// `Replace` self clip begins at decoration/media.  Keeping this proof strict
/// prevents a fragmented or incomplete clip store from silently changing
/// paint order.
fn exact_self_clip_shadow_prefix_len(
    artifact: &PaintArtifact,
    chunk: &super::PaintChunk,
) -> Option<usize> {
    if artifact.chunks.len() != 1
        || artifact.chunks.first()?.id != chunk.id
        || !matches!(
            artifact.owner_nodes.as_slice(),
            [PaintOwnerSnapshot {
                owner,
                parent: None,
                ..
            }] if *owner == chunk.owner
        )
        || !artifact.effect_nodes.is_empty()
        || chunk.id.scope != PaintPropertyScope::SelfPaint
        || chunk.id.phase != super::PaintNodePhase::BeforeChildren
        || chunk.id.slot != 0
    {
        return None;
    }

    let self_clip = ClipNodeId {
        owner: chunk.owner,
        role: ClipNodeRole::SelfClip,
    };
    let [clip] = artifact.clip_nodes.as_slice() else {
        return None;
    };
    if *clip
        != (ClipNodeSnapshot {
            id: self_clip,
            owner: chunk.owner,
            parent: None,
            geometry: clip.geometry,
            behavior: ClipBehavior::Replace,
            generation: clip.generation,
        })
        || clip.generation == 0
        || !chunk.properties.legacy_boundary_eq(PropertyTreeState {
            clip: Some(self_clip),
            ..Default::default()
        })
    {
        return None;
    }

    let shadow_count = match (&chunk.id.role, &chunk.payload_identity) {
        (PaintChunkRole::SelfDecoration, PaintPayloadIdentity::PreparedShadows(shadows, _))
        | (PaintChunkRole::ImageContent, PaintPayloadIdentity::ImageWithShadows(_, shadows, _))
        | (PaintChunkRole::SvgContent, PaintPayloadIdentity::SvgWithShadows(_, shadows, _)) => {
            shadows.len()
        }
        _ => return None,
    };
    if shadow_count == 0 {
        return None;
    }
    let ops = artifact.ops.get(chunk.op_range.clone())?;
    if ops.len() < shadow_count
        || !ops[..shadow_count]
            .iter()
            .all(|op| matches!(op, PaintOp::PreparedShadow(_)))
        || ops[shadow_count..]
            .iter()
            .any(|op| matches!(op, PaintOp::PreparedShadow(_)))
    {
        return None;
    }
    Some(shadow_count)
}

fn emit_prepared_scrollbar_shadow(
    op: &super::artifact::PreparedScrollbarShadowOp,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    let output = ctx.current_target().unwrap_or_else(|| {
        let target = ctx.allocate_target(graph);
        ctx.set_current_target(target);
        target
    });
    let viewport = ctx.viewport();
    if build_shadow_module(
        graph,
        ShadowModuleSpec {
            shape: op.shape,
            params: op.params,
            viewport_width: viewport.target_width(),
            viewport_height: viewport.target_height(),
            scale_factor: viewport.scale_factor(),
            pass_context: ctx.graphics_pass_context(),
            output,
        },
    ) {
        ctx.set_current_target(output);
    }
}

#[cfg(test)]
thread_local! {
    static ARTIFACT_COMPILE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn take_artifact_compile_count() -> usize {
    ARTIFACT_COMPILE_COUNT.with(|count| count.replace(0))
}

fn translate_nested_scroll_position(position: &mut [f32; 2], delta: [f32; 2]) -> Option<()> {
    position[0] += delta[0];
    position[1] += delta[1];
    position.iter().all(|value| value.is_finite()).then_some(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArtifactSurfacePaintOpKind {
    DrawRect,
    InlineIfcDecoration,
    Shadow,
    ScrollbarOverlay,
    Text,
    Image,
    Svg,
    Gpu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArtifactSurfaceLocalizationError {
    NonFiniteTranslation,
    EmbeddedClip(ArtifactSurfacePaintOpKind),
    InvalidLocalizedOp(ArtifactSurfacePaintOpKind),
}

fn artifact_surface_paint_op_kind(op: &PaintOp) -> ArtifactSurfacePaintOpKind {
    match op {
        PaintOp::DrawRect(_) => ArtifactSurfacePaintOpKind::DrawRect,
        PaintOp::PreparedInlineIfcDecoration(_) => ArtifactSurfacePaintOpKind::InlineIfcDecoration,
        PaintOp::PreparedShadow(_) => ArtifactSurfacePaintOpKind::Shadow,
        PaintOp::PreparedScrollbarOverlay(_) => ArtifactSurfacePaintOpKind::ScrollbarOverlay,
        PaintOp::PreparedText(_) => ArtifactSurfacePaintOpKind::Text,
        PaintOp::PreparedImage(_) => ArtifactSurfacePaintOpKind::Image,
        PaintOp::PreparedSvg(_) => ArtifactSurfacePaintOpKind::Svg,
        PaintOp::PreparedGpu(_) => ArtifactSurfacePaintOpKind::Gpu,
    }
}

fn translate_artifact_surface_texture_params(
    params: &mut crate::view::render_pass::texture_composite_pass::TextureCompositeParams,
    delta: [f32; 2],
    kind: ArtifactSurfacePaintOpKind,
) -> Result<(), ArtifactSurfaceLocalizationError> {
    if params.scissor_rect.is_some() {
        return Err(ArtifactSurfaceLocalizationError::EmbeddedClip(kind));
    }
    params.bounds[0] += delta[0];
    params.bounds[1] += delta[1];
    if let Some(quad) = &mut params.quad_positions {
        for point in quad {
            point[0] += delta[0];
            point[1] += delta[1];
        }
    }
    params
        .bounds
        .iter()
        .copied()
        .chain(params.quad_positions.iter().flatten().flatten().copied())
        .all(f32::is_finite)
        .then_some(())
        .ok_or(ArtifactSurfaceLocalizationError::InvalidLocalizedOp(kind))
}

/// Translates one generic artifact op into a detached surface's raster space.
/// All seven op variants are represented. Embedded per-op clips reject with a
/// typed reason because clip localization belongs to the coverage closure,
/// not to an opaque prepared payload.
pub(crate) fn localize_artifact_surface_op(
    op: &PaintOp,
    delta: [f32; 2],
) -> Result<PaintOp, ArtifactSurfaceLocalizationError> {
    if delta.into_iter().any(|value| !value.is_finite()) {
        return Err(ArtifactSurfaceLocalizationError::NonFiniteTranslation);
    }
    let kind = artifact_surface_paint_op_kind(op);
    let invalid = || ArtifactSurfaceLocalizationError::InvalidLocalizedOp(kind);
    match op {
        PaintOp::PreparedGpu(gpu) => {
            let mut localized = gpu.clone();
            translate_artifact_surface_texture_params(&mut localized.params, delta, kind)?;
            Ok(PaintOp::PreparedGpu(localized))
        }

        PaintOp::DrawRect(rect) => {
            let mut localized = rect.clone();
            translate_nested_scroll_position(&mut localized.params.position, delta)
                .ok_or_else(invalid)?;
            Ok(PaintOp::DrawRect(localized))
        }
        PaintOp::PreparedInlineIfcDecoration(decoration) => {
            let mut fill = decoration.fill.clone();
            translate_nested_scroll_position(&mut fill.position, delta).ok_or_else(invalid)?;
            let border = match &decoration.border {
                Some(border) => {
                    let mut border = border.clone();
                    translate_nested_scroll_position(&mut border.position, delta)
                        .ok_or_else(invalid)?;
                    Some(border)
                }
                None => None,
            };
            super::PreparedInlineIfcDecorationOp::new(decoration.descriptor.clone(), fill, border)
                .map(PaintOp::inline_decoration)
                .ok_or_else(invalid)
        }
        PaintOp::PreparedShadow(shadow) => {
            let mut origin = [shadow.shape.x, shadow.shape.y];
            translate_nested_scroll_position(&mut origin, delta).ok_or_else(invalid)?;
            let shape = crate::view::render_pass::ShadowShape {
                x: origin[0],
                y: origin[1],
                ..shadow.shape
            };
            PreparedShadowOp::new(shape, shadow.params)
                .map(PaintOp::PreparedShadow)
                .ok_or_else(invalid)
        }
        PaintOp::PreparedScrollbarOverlay(overlay) => overlay
            .translated_by(delta)
            .map(PaintOp::scrollbar_overlay)
            .ok_or_else(invalid),
        PaintOp::PreparedText(text) => {
            if text.params.scissor_rect.is_some() || text.params.stencil_clip_id.is_some() {
                return Err(ArtifactSurfaceLocalizationError::EmbeddedClip(kind));
            }
            text.translated(delta)
                .map(PaintOp::PreparedText)
                .ok_or_else(invalid)
        }
        PaintOp::PreparedImage(image) => {
            let mut localized = image.clone();
            translate_artifact_surface_texture_params(&mut localized.params, delta, kind)?;
            Ok(PaintOp::PreparedImage(localized))
        }
        PaintOp::PreparedSvg(svg) => {
            let mut localized = svg.clone();
            translate_artifact_surface_texture_params(&mut localized.params, delta, kind)?;
            Ok(PaintOp::PreparedSvg(localized))
        }
    }
}

fn neutralize_artifact_surface_opacity(
    op: PaintOp,
) -> Result<PaintOp, ArtifactSurfaceLocalizationError> {
    // The recorder bakes an owner-local effect into every opacity carrier.
    // The detached raster stores neutral 1.0 and the composite edge applies
    // the effect once.
    let kind = artifact_surface_paint_op_kind(&op);
    let neutral = 1.0_f32;
    let rebuilt = match op {
        PaintOp::PreparedGpu(mut gpu) => {
            gpu.params.opacity = neutral;
            PaintOp::PreparedGpu(gpu)
        }
        PaintOp::DrawRect(mut rect) => {
            rect.params.opacity = neutral;
            PaintOp::DrawRect(rect)
        }
        PaintOp::PreparedInlineIfcDecoration(decoration) => {
            let decoration = std::sync::Arc::unwrap_or_clone(decoration);
            let mut fill = decoration.fill;
            fill.opacity = neutral;
            let border = decoration.border.map(|mut border| {
                border.opacity = neutral;
                border
            });
            super::PreparedInlineIfcDecorationOp::new(decoration.descriptor, fill, border)
                .map(PaintOp::inline_decoration)
                .ok_or(ArtifactSurfaceLocalizationError::InvalidLocalizedOp(kind))?
        }
        PaintOp::PreparedShadow(shadow) => {
            let mut params = shadow.params;
            params.opacity = neutral;
            PreparedShadowOp::new(shadow.shape, params)
                .map(PaintOp::PreparedShadow)
                .ok_or(ArtifactSurfaceLocalizationError::InvalidLocalizedOp(kind))?
        }
        PaintOp::PreparedScrollbarOverlay(overlay) => overlay
            .with_baked_opacity(neutral)
            .map(PaintOp::scrollbar_overlay)
            .ok_or(ArtifactSurfaceLocalizationError::InvalidLocalizedOp(kind))?,
        PaintOp::PreparedText(text) => {
            let mut params = std::sync::Arc::unwrap_or_clone(text.params);
            for glyph in &mut params.staging_input.glyphs {
                glyph.paint.opacity = neutral;
            }
            PreparedTextOp::new(params)
                .map(PaintOp::PreparedText)
                .ok_or(ArtifactSurfaceLocalizationError::InvalidLocalizedOp(kind))?
        }
        PaintOp::PreparedImage(mut image) => {
            image.params.opacity = neutral;
            PaintOp::PreparedImage(image)
        }
        PaintOp::PreparedSvg(mut svg) => {
            svg.params.opacity = neutral;
            PaintOp::PreparedSvg(svg)
        }
    };
    Ok(rebuilt)
}

#[cfg(test)]
pub(crate) fn artifact_surface_op_corresponds_to_source_for_test(
    source: &PaintOp,
    raster: &PaintOp,
    delta: [f32; 2],
    neutralized_opacity_bits: Option<u32>,
) -> bool {
    artifact_surface_op_corresponds_to_source(source, raster, delta, neutralized_opacity_bits)
}

#[cfg(test)]
pub(crate) fn neutralize_artifact_surface_opacity_for_test(
    op: PaintOp,
) -> Result<PaintOp, ArtifactSurfaceLocalizationError> {
    neutralize_artifact_surface_opacity(op)
}

#[cfg(test)]
pub(crate) fn artifact_surface_op_has_baked_opacity_for_test(
    op: &PaintOp,
    expected_bits: u32,
) -> bool {
    ops_have_baked_local_opacity(std::slice::from_ref(op), expected_bits)
}

#[cfg(test)]
fn ops_have_baked_local_opacity(ops: &[PaintOp], expected_bits: u32) -> bool {
    ops.iter().all(|op| match op {
        PaintOp::DrawRect(op) => op.params.opacity.to_bits() == expected_bits,
        PaintOp::PreparedInlineIfcDecoration(op) => {
            op.fill.opacity.to_bits() == expected_bits
                && op
                    .border
                    .as_ref()
                    .is_none_or(|border| border.opacity.to_bits() == expected_bits)
        }
        PaintOp::PreparedShadow(op) => op.params.opacity.to_bits() == expected_bits,
        PaintOp::PreparedScrollbarOverlay(op) => op.has_baked_opacity(expected_bits),
        PaintOp::PreparedText(op) => op.has_baked_opacity(expected_bits),
        PaintOp::PreparedImage(op) => op.params.opacity.to_bits() == expected_bits,
        PaintOp::PreparedSvg(op) => op.params.opacity.to_bits() == expected_bits,
        PaintOp::PreparedGpu(op) => op.params.opacity.to_bits() == expected_bits,
    })
}

/// Folds one leaf-to-root clip chain into the scissor its chunk rasterizes
/// with. Every clip must already be placed.
fn fold_resolved_clip_chain(chain: &[ClipNodeSnapshot]) -> Option<ResolvedClip> {
    chain
        .iter()
        .rev()
        .try_fold(ResolvedClip::Unclipped, |clip, snapshot| {
            let scissor = snapshot.geometry.viewport_scissor()?;
            Some(match snapshot.behavior {
                ClipBehavior::Replace => resolved_scissor(scissor),
                ClipBehavior::Intersect => intersect_resolved_clip(clip, scissor),
            })
        })
}

/// Resolves every chunk's clip from its placed clip chain. Chunks sharing a
/// leaf share its result. The error names the owner of a clip whose chain
/// is incomplete, cyclic or unplaced.
fn try_resolve_clip_values(artifact: &PaintArtifact) -> Result<Vec<ResolvedClip>, NodeKey> {
    let clip_nodes = artifact
        .clip_nodes
        .iter()
        .map(|snapshot| (snapshot.id, *snapshot))
        .collect::<FxHashMap<_, _>>();
    let mut by_leaf = FxHashMap::<ClipNodeId, ResolvedClip>::default();
    let mut chain = Vec::new();
    artifact
        .chunks
        .iter()
        .map(|chunk| {
            let Some(leaf) = chunk.properties.clip else {
                return Ok(ResolvedClip::Unclipped);
            };
            if let Some(resolved) = by_leaf.get(&leaf) {
                return Ok(*resolved);
            }
            chain.clear();
            let mut cursor = Some(leaf);
            while let Some(id) = cursor {
                // A chain longer than the clips it draws from has a cycle.
                if chain.len() >= clip_nodes.len() {
                    return Err(leaf.owner);
                }
                let snapshot = *clip_nodes.get(&id).ok_or(id.owner)?;
                chain.push(snapshot);
                cursor = snapshot.parent;
            }
            let resolved = fold_resolved_clip_chain(&chain).ok_or_else(|| {
                chain
                    .iter()
                    .find(|clip| clip.geometry.viewport_scissor().is_none())
                    .map_or(leaf.owner, |clip| clip.owner)
            })?;
            by_leaf.insert(leaf, resolved);
            Ok(resolved)
        })
        .collect()
}

/// Whether a chunk carries its owner effect's opacity baked into its colors.
/// The reserved child-mask slot is a stencil program whose opacity is fixed
/// at `1.0`; it never carries the owning effect's color opacity.
fn chunk_carries_baked_color_opacity(chunk: &super::PaintChunk) -> bool {
    chunk.id.slot != super::RETAINED_CHILD_MASK_SLOT
}

fn resolved_scissor([x, y, width, height]: [u32; 4]) -> ResolvedClip {
    if width == 0 || height == 0 {
        ResolvedClip::Empty
    } else {
        ResolvedClip::Scissor([x, y, width, height])
    }
}

fn intersect_resolved_clip(current: ResolvedClip, next: [u32; 4]) -> ResolvedClip {
    let ResolvedClip::Scissor([current_x, current_y, current_width, current_height]) = current
    else {
        return match current {
            ResolvedClip::Unclipped => resolved_scissor(next),
            ResolvedClip::Empty => ResolvedClip::Empty,
            ResolvedClip::Scissor(_) => unreachable!(),
        };
    };
    let [next_x, next_y, next_width, next_height] = next;
    if current_width == 0 || current_height == 0 || next_width == 0 || next_height == 0 {
        return ResolvedClip::Empty;
    }
    let left = u64::from(current_x.max(next_x));
    let top = u64::from(current_y.max(next_y));
    let right = (u64::from(current_x) + u64::from(current_width))
        .min(u64::from(next_x) + u64::from(next_width));
    let bottom = (u64::from(current_y) + u64::from(current_height))
        .min(u64::from(next_y) + u64::from(next_height));
    if right <= left || bottom <= top {
        return ResolvedClip::Empty;
    }
    ResolvedClip::Scissor([
        u32::try_from(left).unwrap_or(u32::MAX),
        u32::try_from(top).unwrap_or(u32::MAX),
        u32::try_from(right - left).unwrap_or(u32::MAX),
        u32::try_from(bottom - top).unwrap_or(u32::MAX),
    ])
}

#[cfg(test)]
mod direct_command_test_support;
#[cfg(test)]
pub(crate) use direct_command_test_support::{
    compile_artifact, try_compile_artifact, with_placed_chunks, with_placed_geometry,
};

mod raster_equivalence;
mod raster_window;

#[cfg(test)]
mod raster_test_support;
#[cfg(test)]
#[cfg(test)]
use super::surface_dag::SurfaceMaterializationDecision;
#[cfg(test)]
use raster_test_support::artifact_surface_op_corresponds_to_source;
