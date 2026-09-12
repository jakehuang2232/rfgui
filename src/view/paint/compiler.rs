use crate::view::base_component::{
    Rect, RetainedSurfaceBounds, UiBuildContext, exact_logical_scissor_for_rect,
    paint_offset_after_owner_snap,
};
use crate::view::compositor::property_tree::{
    ClipBehavior, ClipNodeId, ClipNodeRole, ClipNodeSnapshot, EffectNodeId, EffectNodeSnapshot,
    PropertyTreeState, ScrollNodeId, ScrollNodeSnapshot, SpatialProjectionError,
    SpatialProjectionGraph, TransformNodeId, TransformNodeSnapshot,
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
use slotmap::Key;
use std::ops::Range;

use super::artifact::PaintChunkRasterIdentity;

use super::surface_dag::{
    ArtifactSurfaceCoverageForest, ArtifactSurfaceCoverageSpan, ArtifactSurfaceCoverageStep,
    LayerizationPolicy, SurfaceDag, SurfaceDagClipClosureProjection, SurfaceDagError,
    SurfaceDagExecutionNodeId, SurfaceDagExecutionOrder, SurfaceDagExecutionTargetId,
    SurfaceDagNodeId, SurfaceDagNodeKind,
};
use super::{
    PaintArtifact, PaintArtifactTarget, PaintChunkRole, PaintContentRevision, PaintOp,
    PaintOwnerSnapshot, PaintPayloadIdentity, PaintPropertyScope, PreparedImageIdentity,
    PreparedShadowOp, PreparedSvgIdentity, PreparedTextOp, TransitionError,
};

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
#[cfg(any(test, feature = "renderer-test-support"))]
pub(crate) use artifact_surface_executor::take_last_production_actions_for_test;
pub(crate) use artifact_surface_executor::{
    ArtifactSurfaceExecutionError, emit_prepared_artifact_surface_frame_from_pool,
};
pub(crate) use planning_cache::PlanningCache;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
enum ValidatedArtifactTarget {
    CurrentTarget,
    RootOpacityGroup {
        // Only the direct-command test compiler consumes these validated facts.
        #[cfg(test)]
        root: crate::view::node_arena::NodeKey,
        #[cfg(test)]
        effect: EffectNodeSnapshot,
    },
}

struct ValidatedArtifact {
    resolved_clips: Vec<ResolvedClip>,
    target: ValidatedArtifactTarget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArtifactStoreValidationPolicy {
    /// Direct command compilation used by the low-level paint tests.
    #[cfg(test)]
    General,
    /// Production generic recording retains all authored property references.
    SurfaceDag,
}

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
        );
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
/// placement belongs to the parent's nested dependency. Exact-shape legacy
/// dependencies cannot be represented here.
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArtifactCompileErrorKind {
    #[cfg(test)]
    InvalidStore,

    SurfaceExecution(ArtifactSurfaceExecutionError),
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
    max_texture_dimension_2d: u32,
    max_texture_bytes: u64,
}

impl ArtifactSurfaceRasterContext {
    pub(crate) fn new(
        scale_factor: f32,
        target_format: wgpu::TextureFormat,
        paint_offset: [f32; 2],
        incoming_scissor: Option<[u32; 4]>,
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
            max_texture_dimension_2d,
            max_texture_bytes,
        })
    }

    fn scale_factor(self) -> f32 {
        f32::from_bits(self.scale_factor_bits)
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
    EmptySurfaceBounds(SurfaceDagNodeId),
    InvalidSurfaceBounds(SurfaceDagNodeId),
    InvalidRasterOrigin(SurfaceDagNodeId),
    InvalidDescriptor(SurfaceDagNodeId),
    TextureBudgetExceeded(SurfaceDagNodeId),
    GpuSourceBudgetExceeded,
    InvalidGpuSource,
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

impl From<SurfaceDagError> for ArtifactSurfaceRasterPlanError {
    fn from(error: SurfaceDagError) -> Self {
        Self::SurfaceDag(error)
    }
}

/// Pure artifact-stage program. This is the only input accepted by the later
/// graph-inert raster descriptor stage; it contains no viewport facts.
#[derive(Clone, Debug)]
struct ValidatedArtifactSurfaceDagProgram {
    artifact: PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
    surface_dag: SurfaceDag,
    execution_order: SurfaceDagExecutionOrder,
    coverage: ArtifactSurfaceCoverageForest,
    host_placement: ArtifactSurfaceHostPlacementProjection,
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
    /// The single-construction closure emitted by the shared Surface DAG walk;
    /// preparation may resolve through it but never recomputes or merges it.
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

    pub(crate) fn resolved_receiver_clip(self) -> ArtifactSurfaceResolvedClip {
        match self {
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

    fn clip_space_matches_receiver(self, receiver: SurfaceDagExecutionTargetId) -> bool {
        match self.resolved_receiver_clip() {
            ArtifactSurfaceResolvedClip::Unclipped | ArtifactSurfaceResolvedClip::Empty => true,
            ArtifactSurfaceResolvedClip::Scissor(GraphicsPassScissor::Logical(_)) => {
                matches!(receiver, SurfaceDagExecutionTargetId::SceneRoot(_))
            }
            ArtifactSurfaceResolvedClip::Scissor(GraphicsPassScissor::TargetPhysical(_)) => {
                matches!(receiver, SurfaceDagExecutionTargetId::Surface(_))
            }
        }
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
                ..
            } => Self::Transform {
                source_bounds_bits,
                destination_bounds_bits,
                receiver_transform_bits,
                quad_offset_bits,
                receiver_clip,
                resolved_receiver_clip,
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
/// for test inspection after preparation has validated and applied them.
#[derive(Clone, Debug)]
pub(crate) struct PreparedArtifactSurfaceRasterPlan {
    #[cfg(test)]
    context: ArtifactSurfaceRasterContext,
    roots: Vec<PreparedArtifactSurfaceRasterRoot>,
    nodes: Vec<PreparedArtifactSurfaceRasterNode>,
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

    #[cfg(test)]
    pub(crate) fn force_first_role_for_test(
        &mut self,
        role: RetainedSurfaceRasterRole,
    ) -> Option<(SurfaceDagNodeId, RetainedSurfaceRasterRole)> {
        let node = self.nodes.first_mut()?;
        let source = node.source;
        let previous = node.identity.role;
        node.identity.role = role;
        Some((source, previous))
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

/// Typed rejection for the shared current-target artifact Surface DAG program.
/// Detached-surface admission belongs to the caller's typed raster-plan policy,
/// not to this artifact validation taxonomy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SingleTargetSurfaceDagPrepareError {
    InvalidArtifactStore,
    UnsupportedTarget(PaintArtifactTarget),
    SurfaceDag(SurfaceDagError),
}

fn validate_artifact_surface_dag_program(
    artifact: PaintArtifact,
    mut cache: Option<&mut PlanningCache>,
) -> Result<ValidatedArtifactSurfaceDagProgram, SingleTargetSurfaceDagPrepareError> {
    let _profile = crate::view::paint::work_profile::scope("validate_artifact_surface_dag_program");
    let Some(validated) = validate_artifact_store_with_cache(
        &artifact,
        ArtifactStoreValidationPolicy::SurfaceDag,
        cache.as_deref_mut(),
    ) else {
        return Err(SingleTargetSurfaceDagPrepareError::InvalidArtifactStore);
    };
    if !matches!(validated.target, ValidatedArtifactTarget::CurrentTarget) {
        return Err(SingleTargetSurfaceDagPrepareError::UnsupportedTarget(
            artifact.target,
        ));
    }

    if let Some(mut cached) = cache
        .as_deref_mut()
        .and_then(|cache| cache.geometry(&artifact))
    {
        cached.artifact = artifact;
        cached.resolved_clips = validated.resolved_clips;
        return Ok(cached);
    }

    let graphs = cache
        .as_deref_mut()
        .map(|cache| cache.graphs(&artifact))
        .transpose()
        .map_err(SurfaceDagError::Transition)
        .map_err(SingleTargetSurfaceDagPrepareError::SurfaceDag)?
        .flatten();
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
        .reconstruct(&events)
        .map_err(SingleTargetSurfaceDagPrepareError::SurfaceDag)?;
    let coverage = match cache
        .as_deref_mut()
        .and_then(|cache| cache.coverage(&artifact, &surface_dag))
    {
        Some(coverage) => coverage,
        None => inputs
            .coverage(&surface_dag)
            .map_err(SingleTargetSurfaceDagPrepareError::SurfaceDag)?,
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
        None => ArtifactSurfaceHostPlacementProjection::try_new(&artifact)
            .map_err(TransitionError::SpatialSnapshot)
            .map_err(SurfaceDagError::Transition)
            .map_err(SingleTargetSurfaceDagPrepareError::SurfaceDag)?,
    };

    let graphs = inputs.graphs();
    let program = ValidatedArtifactSurfaceDagProgram {
        artifact,
        resolved_clips: validated.resolved_clips,
        surface_dag,
        execution_order,
        coverage,
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
    owners: std::sync::Arc<[ArtifactSurfaceOwnerPlacement]>,
    // Shared only by clones of this exact immutable owner-position program.
    // Host offset is the remaining input; successful resolution is cached once.
    resolved: std::sync::Arc<std::sync::Mutex<Option<HostPlacementResolutionMemo>>>,
}

#[derive(Clone, Debug)]
struct HostPlacementResolutionMemo {
    owners: std::sync::Arc<[ArtifactSurfaceOwnerPlacement]>,
    offset_bits: [u32; 2],
    resolved: ResolvedArtifactSurfaceHostPlacement,
}

#[derive(Clone, Copy, Debug)]
struct ArtifactSurfaceOwnerPlacement {
    owner: NodeKey,
    parent: Option<NodeKey>,
    viewport_position_bits: [u32; 2],
}

#[derive(Clone, Debug)]
struct ResolvedArtifactSurfaceHostPlacement {
    owner_paint_offset_bits: std::sync::Arc<FxHashMap<NodeKey, [u32; 2]>>,
}

impl ArtifactSurfaceHostPlacementProjection {
    fn try_new(artifact: &PaintArtifact) -> Result<Self, SpatialProjectionError> {
        let graph = SpatialProjectionGraph::try_new(
            &artifact.transform_nodes,
            &artifact.layout_position_nodes,
            &artifact.visual_offset_nodes,
            &artifact.scroll_nodes,
        )?;
        let mut owners = Vec::with_capacity(artifact.owner_nodes.len());
        for snapshot in &artifact.owner_nodes {
            owners.push(ArtifactSurfaceOwnerPlacement {
                owner: snapshot.owner,
                parent: snapshot.parent,
                viewport_position_bits: graph
                    .derive_optional_owner_viewport_position(snapshot.owner)?
                    .to_array()
                    .map(f32::to_bits),
            });
        }
        Ok(Self {
            owners: owners.into(),
            resolved: Default::default(),
        })
    }

    fn resolve(
        &self,
        host_paint_offset: [f32; 2],
    ) -> Result<ResolvedArtifactSurfaceHostPlacement, SpatialProjectionError> {
        let offset_bits = host_paint_offset.map(f32::to_bits);
        if let Some(memo) = self
            .resolved
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .filter(|memo| {
                std::sync::Arc::ptr_eq(&memo.owners, &self.owners)
                    && memo.offset_bits == offset_bits
            })
        {
            return Ok(memo.resolved.clone());
        }
        let mut placements =
            FxHashMap::with_capacity_and_hasher(self.owners.len(), Default::default());
        for placement in self.owners.iter() {
            if placements.insert(placement.owner, *placement).is_some() {
                return Err(SpatialProjectionError::InvalidSnapshot(placement.owner));
            }
        }
        let mut owner_paint_offset_bits: FxHashMap<NodeKey, [u32; 2]> =
            FxHashMap::with_capacity_and_hasher(self.owners.len(), Default::default());
        let mut chain = Vec::new();
        let mut seen = FxHashSet::default();
        for placement in self.owners.iter() {
            if owner_paint_offset_bits.contains_key(&placement.owner) {
                continue;
            }
            chain.clear();
            seen.clear();
            let mut cursor = placement.owner;
            // Artifact owner stores preserve canonical traversal order, not a
            // parent-first topological order. Resolve the bounded ancestry
            // explicitly so a child-first store cannot change snap semantics.
            let mut parent_paint_offset = loop {
                if let Some(bits) = owner_paint_offset_bits.get(&cursor) {
                    break bits.map(f32::from_bits);
                }
                if chain.len() >= usize::from(u8::MAX) || !seen.insert(cursor) {
                    return Err(SpatialProjectionError::InvalidSnapshot(cursor));
                }
                let current = placements
                    .get(&cursor)
                    .copied()
                    .ok_or(SpatialProjectionError::InvalidSnapshot(cursor))?;
                chain.push(current);
                match current.parent {
                    Some(parent) => cursor = parent,
                    None => break host_paint_offset,
                }
            };
            for current in chain.drain(..).rev() {
                parent_paint_offset = paint_offset_after_owner_snap(
                    current.viewport_position_bits.map(f32::from_bits),
                    parent_paint_offset,
                )
                .ok_or(SpatialProjectionError::InvalidSnapshot(current.owner))?;
                owner_paint_offset_bits
                    .insert(current.owner, parent_paint_offset.map(f32::to_bits));
            }
        }
        let resolved = ResolvedArtifactSurfaceHostPlacement {
            owner_paint_offset_bits: std::sync::Arc::new(owner_paint_offset_bits),
        };
        *self
            .resolved
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(HostPlacementResolutionMemo {
            owners: self.owners.clone(),
            offset_bits,
            resolved: resolved.clone(),
        });
        Ok(resolved)
    }
}

impl ResolvedArtifactSurfaceHostPlacement {
    fn owner_paint_offset(&self, owner: NodeKey) -> Option<[f32; 2]> {
        self.owner_paint_offset_bits
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
        let physical_right = finite_f32_to_i64(((x + width) * scale).ceil())?;
        let physical_bottom = finite_f32_to_i64(((y + height) * scale).ceil())?;
        let target_width = u32::try_from(physical_right.checked_sub(physical_left)?).ok()?;
        let target_height = u32::try_from(physical_bottom.checked_sub(physical_top)?).ok()?;
        if target_width == 0 || target_height == 0 {
            return None;
        }
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
        if !seen.insert(cursor) || chain.len() >= usize::from(u8::MAX) {
            return None;
        }
        let snapshot = *clips.get(&cursor)?;
        chain.push(snapshot);
        let Some(parent) = snapshot.parent else {
            break;
        };
        cursor = parent;
    }
    let mut resolved = ResolvedClip::Unclipped;
    for snapshot in chain.iter().rev() {
        resolved = match snapshot.behavior {
            ClipBehavior::Replace => resolved_scissor(snapshot.logical_scissor),
            ClipBehavior::Intersect => intersect_resolved_clip(resolved, snapshot.logical_scissor),
        };
    }
    Some((resolved, chain))
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
) -> Result<&'a [super::PaintChunk], ArtifactSurfaceRasterPlanError> {
    let chunk_range = span.chunk_range();
    let op_range = span.op_range();
    let chunks = artifact
        .chunks
        .get(chunk_range)
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
fn artifact_surface_chunk_base_translation(
    boundary_root: Option<NodeKey>,
    chunk_owner: NodeKey,
    base_delta: [f32; 2],
) -> [f32; 2] {
    if boundary_root == Some(chunk_owner) {
        [0.0, 0.0]
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
) -> Result<Option<[u32; 4]>, ArtifactSurfaceRasterPlanError> {
    artifact_surface_chunk_range_raw_bounds(
        artifact,
        target,
        boundary_root,
        span.chunk_range(),
        base_delta,
    )
}

fn artifact_surface_chunk_range_raw_bounds(
    artifact: &PaintArtifact,
    target: ArtifactSurfaceRasterTargetId,
    boundary_root: Option<NodeKey>,
    chunk_range: Range<usize>,
    base_delta: [f32; 2],
) -> Result<Option<[u32; 4]>, ArtifactSurfaceRasterPlanError> {
    let chunks = artifact
        .chunks
        .get(chunk_range.clone())
        .ok_or(ArtifactSurfaceRasterPlanError::InvalidCoverageSpan(target))?;
    let mut bounds = None;
    for (local_index, chunk) in chunks.iter().enumerate() {
        let chunk_index = chunk_range.start + local_index;
        let delta = artifact_surface_chunk_base_translation(boundary_root, chunk.owner, base_delta);
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
        raster_origin: ArtifactSurfaceRasterOriginProjection,
    },
}

impl ArtifactSurfaceSpanPlacement<'_> {
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
                raster_origin,
            } => raster_origin
                .combined_translation(artifact_surface_chunk_base_translation(
                    Some(boundary_root),
                    owner,
                    base_delta,
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
            chunks,
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
    for snapshot in span.local_clips() {
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
                validated_artifact_chunk_carries_baked_color_opacity(chunk)
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
                        Some(opacity_bits) => {
                            neutralize_artifact_surface_opacity(localized, opacity_bits)
                        }
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
        local_clips: span.local_clips().to_vec(),
        chunks: prepared.into(),
    };
    if let Some(cache) = cache {
        cache.remember_raster_span(
            target,
            span,
            chunks,
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
    let mut translation = surface_raster_translation(source, kind, scrolls)?;
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
// Both chains are innermost-first parent paths in the same validated forest.
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
) -> Result<ArtifactSurfaceCompositeGeometryStamp, ArtifactSurfaceRasterPlanError> {
    // Consumption transitions may borrow the deepest descendant witness's
    // non-consumed dimensions. That descendant's self clip is raster-local
    // inside this surface, not an outer composite clip. Only the surface
    // owner's recorded paint scope can supply this receiver boundary.
    let receiver_clip = closure
        .map(SurfaceDagClipClosureProjection::receiver_clip)
        .map(Some)
        .unwrap_or_else(|| {
            artifact_surface_receiver_clip(owner_clip, node.transition().clip.to, clips)
        })
        .ok_or(ArtifactSurfaceRasterPlanError::InvalidReceiverClip(
            node.id(),
        ))?;
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
            let snapshot = transforms.get(&transform).copied().ok_or(
                ArtifactSurfaceRasterPlanError::MissingSurfaceSnapshot(node.id()),
            )?;
            // This snapshot is owner-only, not ancestor-composed (see
            // DerivedSpatialProjection). Receiver rasters share the logical
            // layout coordinate space before their own transform is applied.
            // Dividing by a receiver transform here would cancel an ancestor
            // that has not yet been applied. Each retained boundary applies
            // its authored matrix exactly once; raster-origin rebasing is
            // handled separately by the frozen quad and destination origin.
            let receiver_transform = snapshot.owner_viewport_transform;
            if !receiver_transform.is_finite() {
                return Err(ArtifactSurfaceRasterPlanError::InvalidSurfaceBounds(
                    node.id(),
                ));
            }
            let (destination_bounds_bits, quad_offset_bits) = transform_destination_projection(
                receiver_source_bounds_bits,
                receiver_transform,
                receiver_paint_offset,
            )
            .ok_or(ArtifactSurfaceRasterPlanError::InvalidSurfaceBounds(
                node.id(),
            ))?;
            Ok(ArtifactSurfaceCompositeGeometryStamp::Transform {
                source_bounds_bits: raster_source_bounds_bits,
                destination_bounds_bits,
                receiver_transform_bits: receiver_transform.to_cols_array().map(f32::to_bits),
                quad_offset_bits,
                receiver_clip,
                resolved_receiver_clip,
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
    receiver: SurfaceDagExecutionTargetId,
    receiver_paint_offset: [f32; 2],
    context: ArtifactSurfaceRasterContext,
    clips: &FxHashMap<ClipNodeId, ClipNodeSnapshot>,
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
    let Some(clip) = transfer.clip else {
        return Ok(geometry);
    };
    let contents_clip = clip.local_clip();
    let (resolved_clip, clip_chain) = resolve_artifact_surface_clip(Some(contents_clip), clips)
        .ok_or(ArtifactSurfaceRasterPlanError::InvalidReceiverClip(
            boundary,
        ))?;
    let resolved_clip = if matches!(receiver, SurfaceDagExecutionTargetId::SceneRoot(_)) {
        let translated =
            translate_artifact_surface_logical_clip(resolved_clip, receiver_paint_offset).ok_or(
                ArtifactSurfaceRasterPlanError::InvalidReceiverClip(boundary),
            )?;
        artifact_surface_terminal_clip(translated, &clip_chain, context.incoming_scissor)
    } else {
        resolved_clip
    };
    geometry.replace_receiver_clip(
        clip.receiver_clip(),
        ArtifactSurfaceResolvedClip::from_logical(resolved_clip),
    );
    Ok(geometry)
}

fn prepare_artifact_surface_raster_plan_from_program(
    program: ValidatedArtifactSurfaceDagProgram,
    context: ArtifactSurfaceRasterContext,
    mut cache: Option<&mut PlanningCache>,
) -> Result<PreparedArtifactSurfaceRasterPlan, ArtifactSurfaceRasterPlanError> {
    if let Some(cache) = cache.as_deref_mut() {
        cache.set_raster_environment(&program.artifact);
    }
    let host_placement = program
        .host_placement
        .resolve(context.paint_offset())
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
    let owner_states = program
        .artifact
        .owner_property_states
        .iter()
        .map(|snapshot| (snapshot.owner, snapshot.paint))
        .collect::<FxHashMap<_, _>>();
    let mut prepared_nodes: Vec<Option<PreparedArtifactSurfaceRasterNode>> =
        vec![None; program.execution_order.nodes().len()];
    // Source output and declared upload buffers share the aggregate limit
    // with materialized native targets, including warm sources. Shader/pipeline
    // driver allocations and sampled assets are not GPU-residency accounting.
    let mut gpu_sources = FxHashMap::default();
    let mut total_texture_bytes = 0_u64;
    for op in &program.artifact.ops {
        if let PaintOp::PreparedGpu(op) = op {
            let source = &op.source;
            if source.scale_bits != context.scale_factor_bits
                || source
                    .extent
                    .iter()
                    .any(|n| *n > context.max_texture_dimension_2d)
                || !source.matches_size([op.params.bounds[2], op.params.bounds[3]])
            {
                return Err(ArtifactSurfaceRasterPlanError::InvalidGpuSource);
            }
            if let Some(previous) = gpu_sources.insert(source.id, source) {
                if previous != source {
                    return Err(ArtifactSurfaceRasterPlanError::InvalidGpuSource);
                }
            } else {
                total_texture_bytes = total_texture_bytes
                    .checked_add(source.allocated_bytes())
                    .filter(|b| *b <= context.max_texture_bytes)
                    .ok_or(ArtifactSurfaceRasterPlanError::GpuSourceBudgetExceeded)?;
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
                    // final receiver projection. The child's receiver clip remains
                    // a fixed receiver boundary and must not inherit this delta.
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
        let content_bounds_bits =
            raw_bounds.ok_or(ArtifactSurfaceRasterPlanError::EmptySurfaceBounds(source))?;
        let receiver_paint_offset = host_placement
            .receiver_paint_offset(node.target(), execution.receiver())
            .ok_or(ArtifactSurfaceRasterPlanError::InvalidOwnerTopology {
                target: target_id,
                owner: node.target(),
            })?;
        let owner_state =
            owner_states
                .get(&node.target())
                .ok_or(ArtifactSurfaceRasterPlanError::SurfaceDag(
                    SurfaceDagError::Transition(TransitionError::MissingOwnerPropertyState(
                        node.target(),
                    )),
                ))?;
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
            )?;
            fold_materialized_composite_boundaries(
                geometry,
                folded_boundaries,
                &program.surface_dag,
                execution.receiver(),
                receiver_paint_offset,
                context,
                &clips,
            )
        };
        let full_origin = ArtifactSurfaceRasterOriginProjection::new(
            content_bounds_bits,
            context.scale_factor_bits,
        )
        .ok_or(ArtifactSurfaceRasterPlanError::InvalidRasterOrigin(source))?;
        // A fixed working-set threshold makes raster geometry independent of
        // the caller's aggregate budget. Lowering that budget cannot silently
        // choose smaller windows to evade an otherwise exact-byte rejection.
        // The complete envelope survives in the stamp; only a proved finite
        // receiver read can restrict the allocated raster region.
        const MAX_UNWINDOWED_BYTES: u64 = 32 * 1024 * 1024;
        let full_bytes =
            u64::from(full_origin.target_size[0]) * u64::from(full_origin.target_size[1]) * 12;
        let window = if full_origin.target_size.iter().any(|n| *n > 8192)
            || full_bytes > MAX_UNWINDOWED_BYTES
        {
            let geometry = geometry_for_bounds(
                content_bounds_bits,
                full_origin.normalized_source_bounds_bits,
            )?;
            raster_window::select(content_bounds_bits, geometry, context)
        } else {
            None
        };
        let raw_source_bounds_bits =
            window.map_or(content_bounds_bits, |window| window.raster_bounds_bits);
        let raster_origin = ArtifactSurfaceRasterOriginProjection::new(
            raw_source_bounds_bits,
            context.scale_factor_bits,
        )
        .ok_or(ArtifactSurfaceRasterPlanError::InvalidRasterOrigin(source))?;
        let mut steps = Vec::with_capacity(coverage.steps().len());
        for step in coverage.steps() {
            match step {
                ArtifactSurfaceCoverageStep::ArtifactSpan(span) => {
                    steps.push(PreparedArtifactSurfaceRasterStep::ArtifactSpan(
                        prepare_artifact_surface_span(
                            &program.artifact,
                            target_id,
                            span,
                            ArtifactSurfaceSpanPlacement::Surface {
                                boundary_root: node.target(),
                                base_delta,
                                raster_origin,
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
        for (child_execution, receiver_destination_bounds_bits) in nested_children {
            let child = prepared_nodes
                .get_mut(child_execution.index())
                .and_then(Option::as_mut)
                .ok_or(ArtifactSurfaceRasterPlanError::InvalidNestedSurface {
                    parent: source,
                    child: source,
                })?;
            child
                .geometry
                .finalize_surface_receiver(raster_origin, receiver_destination_bounds_bits)
                .ok_or(ArtifactSurfaceRasterPlanError::InvalidRasterOrigin(
                    child.source,
                ))?;
        }

        let source_bounds_bits = raster_origin.normalized_source_bounds_bits;
        let identity = surface_identity(node.kind(), node.target(), node.stable_id());
        let [x, y, width, height] = source_bounds_bits.map(f32::from_bits);
        let color = crate::view::base_component::texture_desc_for_logical_bounds(
            RetainedSurfaceBounds {
                x,
                y,
                width,
                height,
                corner_radii: [0.0; 4],
            },
            context.scale_factor(),
            None,
            context.target_format,
        );
        let (color, depth) = crate::view::base_component::persistent_target_texture_descriptors(
            color,
            identity.color_key,
        );
        if color.width() > context.max_texture_dimension_2d
            || color.height() > context.max_texture_dimension_2d
            || depth.width() > context.max_texture_dimension_2d
            || depth.height() > context.max_texture_dimension_2d
        {
            return Err(ArtifactSurfaceRasterPlanError::TextureBudgetExceeded(
                source,
            ));
        }
        let color_bytes = crate::view::raster_cost::texture_desc_payload_bytes(&color);
        let depth_bytes = crate::view::raster_cost::texture_desc_payload_bytes(&depth);
        if !color_bytes.confidence.budget_usable() || !depth_bytes.confidence.budget_usable() {
            return Err(ArtifactSurfaceRasterPlanError::InvalidDescriptor(source));
        }
        total_texture_bytes = total_texture_bytes
            .checked_add(color_bytes.bytes)
            .and_then(|bytes| bytes.checked_add(depth_bytes.bytes))
            .filter(|bytes| *bytes <= context.max_texture_bytes)
            .ok_or(ArtifactSurfaceRasterPlanError::TextureBudgetExceeded(
                source,
            ))?;
        let target = RetainedSurfaceRasterInputs {
            color,
            depth,
            scale_factor_bits: context.scale_factor_bits,
            source_bounds_bits,
        };
        if !target.has_canonical_descriptor_pair_for(identity)
            || target.color.origin() != (0, 0)
            || target.depth.origin() != (0, 0)
            || target.color.width() != raster_origin.target_size[0]
            || target.color.height() != raster_origin.target_size[1]
            || target.depth.width() != raster_origin.target_size[0]
            || target.depth.height() != raster_origin.target_size[1]
        {
            return Err(ArtifactSurfaceRasterPlanError::InvalidDescriptor(source));
        }
        // Receiver closure remains a structural obligation even though its
        // owner-only matrix is not an inverse for the child's projection.
        if let SurfaceDagExecutionTargetId::Surface(receiver) = execution.receiver() {
            let receiver_source = program.execution_order.source_node_id(receiver).ok_or(
                ArtifactSurfaceRasterPlanError::MissingSurfaceSnapshot(source),
            )?;
            let receiver = program
                .surface_dag
                .nodes()
                .get(receiver_source.index())
                .filter(|candidate| candidate.id() == receiver_source)
                .ok_or(ArtifactSurfaceRasterPlanError::MissingSurfaceSnapshot(
                    source,
                ))?;
            if !matches!(receiver.kind(), SurfaceDagNodeKind::Transform(_))
                && receiver.transition().transform.from != receiver.transition().transform.to
            {
                return Err(ArtifactSurfaceRasterPlanError::InvalidSurfaceBounds(source));
            }
        }
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
            clip_closure: coverage.clip_closure().cloned(),
            steps,
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

    if let Some(source) = nodes
        .iter()
        .find(|node| !node.geometry().clip_space_matches_receiver(node.receiver))
        .map(PreparedArtifactSurfaceRasterNode::source)
    {
        return Err(ArtifactSurfaceRasterPlanError::InvalidRasterOrigin(source));
    }

    Ok(PreparedArtifactSurfaceRasterPlan {
        #[cfg(test)]
        context,
        roots,
        nodes,
        #[cfg(test)]
        materialization_decisions: program.execution_order.decisions().to_vec(),
    })
}

pub(crate) fn prepare_artifact_surface_raster_plan(
    artifact: PaintArtifact,
    context: ArtifactSurfaceRasterContext,
) -> Result<PreparedArtifactSurfaceRasterPlan, ArtifactSurfaceRasterPlanError> {
    let _profile = crate::view::paint::work_profile::scope("prepare_artifact_surface_raster_plan");
    let program = validate_artifact_surface_dag_program(artifact, None)
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
    let result = validate_artifact_surface_dag_program(artifact, Some(cache))
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
    InvalidClipClosure(SurfaceDagNodeId),
    NonCanonicalSet,
}

/// Graph-inert full-set identity for every resident in one artifact Surface
/// DAG. It deliberately reuses [`RetainedSurfaceRasterStamp`], because the
/// resident pool and [`RetainedSurfaceResidentKey`] type must remain a single
/// authority across legacy and artifact producers. Artifact residents use the
/// generic role-tagged `Surface` variant; the legacy property-effect variant
/// remains confined to the retained planner until cutover.
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
    // Private immutable allocation validated by the sealer. A changed allocation
    // (including test corruption via make_mut) must pass the full validator again.
    validated_entries: std::sync::Arc<Vec<SealedArtifactSurfaceResidentEntry>>,
}

impl SealedArtifactSurfaceResidentSet {
    pub(crate) fn is_canonical(&self) -> bool {
        std::sync::Arc::ptr_eq(&self.ordered_entries, &self.validated_entries)
            || artifact_surface_resident_set_is_canonical(&self.ordered_entries)
    }

    pub(crate) fn ordered_entries(&self) -> &[SealedArtifactSurfaceResidentEntry] {
        &self.ordered_entries
    }

    pub(crate) fn len(&self) -> usize {
        self.ordered_entries.len()
    }

    pub(crate) fn into_ordered_entries(self) -> Vec<SealedArtifactSurfaceResidentEntry> {
        drop(self.validated_entries);
        std::sync::Arc::unwrap_or_clone(self.ordered_entries)
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

    #[cfg(test)]
    pub(crate) fn remove_first_span_boundary_owner_for_test(&mut self) -> bool {
        for entry in std::sync::Arc::make_mut(&mut self.ordered_entries) {
            let stamp = &mut entry.stamp;
            let Some(program) = stamp.artifact_surface_program.as_mut() else {
                continue;
            };
            for step in &mut program.steps {
                let ArtifactSurfaceRasterProgramStepStamp::ArtifactSpan(span) = step else {
                    continue;
                };
                let boundary = stamp.identity.boundary_root;
                let before = span.owner_topology.len();
                span.owner_topology
                    .retain(|snapshot| snapshot.owner != boundary);
                return span.owner_topology.len() != before;
            }
        }
        false
    }

    #[cfg(test)]
    pub(crate) fn zero_first_span_topology_revision_for_test(&mut self) -> bool {
        for entry in std::sync::Arc::make_mut(&mut self.ordered_entries) {
            let stamp = &mut entry.stamp;
            let Some(program) = stamp.artifact_surface_program.as_mut() else {
                continue;
            };
            for step in &mut program.steps {
                let ArtifactSurfaceRasterProgramStepStamp::ArtifactSpan(span) = step else {
                    continue;
                };
                let Some(chunk) = span.chunks.first_mut() else {
                    continue;
                };
                chunk.raster.topology_revision = 0;
                return true;
            }
        }
        false
    }

    #[cfg(test)]
    pub(crate) fn redirect_first_nested_surface_to_parent_for_test(&mut self) -> bool {
        for entry in std::sync::Arc::make_mut(&mut self.ordered_entries) {
            let stamp = &mut entry.stamp;
            let Some(program) = stamp.artifact_surface_program.as_mut() else {
                continue;
            };
            for step in &mut program.steps {
                let ArtifactSurfaceRasterProgramStepStamp::NestedSurface(dependency) = step else {
                    continue;
                };
                dependency.child_execution_id = program.execution_id;
                return true;
            }
        }
        false
    }

    #[cfg(test)]
    pub(crate) fn force_first_resolved_clip_empty_without_cursor_for_test(&mut self) -> bool {
        for entry in std::sync::Arc::make_mut(&mut self.ordered_entries) {
            let Some(program) = entry.stamp.artifact_surface_program.as_mut() else {
                continue;
            };
            for step in &mut program.steps {
                let ArtifactSurfaceRasterProgramStepStamp::ArtifactSpan(span) = step else {
                    continue;
                };
                let Some(chunk) = span.chunks.iter_mut().find(|chunk| {
                    chunk.clip_schedule.terminal_clip() != ArtifactSurfaceResolvedClip::Empty
                        && chunk.opaque_order_count != 0
                }) else {
                    continue;
                };
                chunk.clip_schedule = ArtifactSurfaceChunkClipSchedule::WholeChunk(
                    ArtifactSurfaceResolvedClip::Empty,
                );
                return true;
            }
        }
        false
    }

    #[cfg(test)]
    pub(crate) fn force_first_clip_prefix_out_of_range_for_test(&mut self) -> bool {
        for entry in std::sync::Arc::make_mut(&mut self.ordered_entries) {
            let Some(program) = entry.stamp.artifact_surface_program.as_mut() else {
                continue;
            };
            for step in &mut program.steps {
                let ArtifactSurfaceRasterProgramStepStamp::ArtifactSpan(span) = step else {
                    continue;
                };
                let Some(chunk) = span.chunks.first_mut() else {
                    continue;
                };
                chunk.clip_schedule = ArtifactSurfaceChunkClipSchedule::AfterShadowPrefix {
                    // Zero is outside the admitted non-empty shadow prefix
                    // range and fails before payload-shape validation.
                    prefix_op_count: 0,
                    suffix_clip: ArtifactSurfaceResolvedClip::Scissor(
                        GraphicsPassScissor::Logical([0, 0, 1, 1]),
                    ),
                };
                return true;
            }
        }
        false
    }
}

fn artifact_surface_geometry_matches(
    identity: RetainedSurfaceRasterIdentity,
    target: &RetainedSurfaceRasterInputs,
    geometry: ArtifactSurfaceCompositeGeometryStamp,
) -> bool {
    let source_bounds_bits = match (identity.role, geometry) {
        (
            RetainedSurfaceRasterRole::Transform,
            ArtifactSurfaceCompositeGeometryStamp::Transform {
                source_bounds_bits, ..
            },
        )
        | (
            RetainedSurfaceRasterRole::PropertyEffect,
            ArtifactSurfaceCompositeGeometryStamp::Effect {
                source_bounds_bits, ..
            },
        )
        | (
            RetainedSurfaceRasterRole::ScrollContent,
            ArtifactSurfaceCompositeGeometryStamp::ScrollContent {
                source_bounds_bits, ..
            },
        ) => source_bounds_bits,
        _ => return false,
    };
    source_bounds_bits == target.source_bounds_bits
        && target.has_canonical_descriptor_pair_for(identity)
        && (!matches!(
            geometry,
            ArtifactSurfaceCompositeGeometryStamp::Transform { .. }
        ) || geometry.transform_quad().is_some())
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

fn artifact_surface_program_geometry_receiver_clip(
    geometry: ArtifactSurfaceCompositeGeometryStamp,
) -> Option<ClipNodeId> {
    match geometry {
        ArtifactSurfaceCompositeGeometryStamp::Transform { receiver_clip, .. }
        | ArtifactSurfaceCompositeGeometryStamp::Effect { receiver_clip, .. }
        | ArtifactSurfaceCompositeGeometryStamp::ScrollContent { receiver_clip, .. } => {
            receiver_clip
        }
    }
}

fn artifact_surface_owner_topology_is_canonical(
    topology: &[PaintOwnerSnapshot],
    boundary_root: NodeKey,
    chunks: &[ArtifactSurfaceRasterProgramChunkStamp],
) -> bool {
    if topology.is_empty() || chunks.is_empty() {
        return false;
    }
    let mut owners = FxHashMap::with_capacity_and_hasher(topology.len(), Default::default());
    for (ordinal, snapshot) in topology.iter().copied().enumerate() {
        if snapshot.owner.is_null()
            || owners
                .insert(snapshot.owner, (snapshot.parent, ordinal))
                .is_some()
            || snapshot.owner == boundary_root && snapshot.parent.is_some()
        {
            return false;
        }
    }
    if owners.get(&boundary_root).map(|entry| entry.0) != Some(None)
        || owners
            .values()
            .filter(|(parent, _)| parent.is_none())
            .count()
            != 1
    {
        return false;
    }
    for snapshot in topology {
        if snapshot.owner == boundary_root {
            continue;
        }
        let Some(parent) = snapshot.parent else {
            return false;
        };
        if owners
            .get(&parent)
            .is_none_or(|(_, parent_ordinal)| *parent_ordinal >= owners[&snapshot.owner].1)
        {
            return false;
        }
    }
    let mut referenced = FxHashSet::default();
    for chunk in chunks {
        let mut cursor = chunk.raster.owner;
        let mut reached_boundary = false;
        for _ in 0..=topology.len() {
            // Every prior chain reached the boundary or this function returned
            // false. Its completed suffix can therefore be shared by later chunks.
            if !referenced.insert(cursor) || cursor == boundary_root {
                reached_boundary = true;
                break;
            }
            let Some(parent) = owners.get(&cursor).and_then(|entry| entry.0) else {
                return false;
            };
            cursor = parent;
        }
        if !reached_boundary {
            return false;
        }
    }
    referenced.len() == owners.len()
}

fn artifact_surface_clip_schedule_is_canonical(
    schedule: ArtifactSurfaceChunkClipSchedule,
    chunk: &RetainedSurfaceChunkStamp,
) -> bool {
    match schedule {
        ArtifactSurfaceChunkClipSchedule::WholeChunk(_) => true,
        ArtifactSurfaceChunkClipSchedule::AfterShadowPrefix {
            prefix_op_count,
            suffix_clip,
        } => {
            prefix_op_count != 0
                && prefix_op_count <= chunk.op_count
                && suffix_clip != ArtifactSurfaceResolvedClip::Unclipped
                && match &chunk.payload_identity {
                    PaintPayloadIdentity::PreparedShadows(shadows, _)
                    | PaintPayloadIdentity::ImageWithShadows(_, shadows, _)
                    | PaintPayloadIdentity::SvgWithShadows(_, shadows, _) => {
                        shadows.len() == prefix_op_count
                    }
                    PaintPayloadIdentity::Gpu(_)
                    | PaintPayloadIdentity::None
                    | PaintPayloadIdentity::Image(_, _)
                    | PaintPayloadIdentity::Svg(_, _)
                    | PaintPayloadIdentity::PreparedTexts(_)
                    | PaintPayloadIdentity::PreparedRects(_)
                    | PaintPayloadIdentity::TextSelection(_)
                    | PaintPayloadIdentity::PreparedScrollbarOverlay(_)
                    | PaintPayloadIdentity::InlineIfcDecorations(_, _) => false,
                }
        }
    }
}

fn artifact_surface_program_span_is_canonical(
    span: &ArtifactSurfaceRasterProgramSpanStamp,
    boundary_root: NodeKey,
    expected_step_index: usize,
    expected_start: u32,
) -> bool {
    if span.step_index != expected_step_index
        || span.opaque_order_span.start != expected_start
        || span.opaque_order_span.end < expected_start
        || span.op_count
            != span
                .chunks
                .iter()
                .map(|chunk| chunk.raster.op_count)
                .sum::<usize>()
        || span
            .opaque_order_span
            .end
            .checked_sub(span.opaque_order_span.start)
            != span.chunks.iter().try_fold(0_u32, |sum, chunk| {
                sum.checked_add(chunk.opaque_order_count)
            })
        || span.chunks.iter().any(|chunk| {
            !artifact_surface_clip_schedule_is_canonical(chunk.clip_schedule, &chunk.raster)
                || chunk.clip_schedule.terminal_clip() == ArtifactSurfaceResolvedClip::Empty
                    && chunk.opaque_order_count != 0
        })
        || !artifact_surface_owner_topology_is_canonical(
            &span.owner_topology,
            boundary_root,
            &span.chunks,
        )
    {
        return false;
    }
    let mut ids = FxHashSet::default();
    let mut clips = FxHashSet::default();
    span.clip_nodes
        .iter()
        .all(|clip| clip.id.owner == clip.owner && clip.generation != 0 && clips.insert(clip.id))
        && span.chunks.iter().all(|chunk| {
            let chunk = &chunk.raster;
            let bounds = chunk.bounds_bits.map(f32::from_bits);
            chunk.id.owner == chunk.owner
                && ids.insert(chunk.id)
                && bounds.into_iter().all(f32::is_finite)
                && bounds[2] >= 0.0
                && bounds[3] >= 0.0
                && chunk.topology_revision != 0
        })
}

fn artifact_surface_resident_set_is_canonical(
    entries: &[SealedArtifactSurfaceResidentEntry],
) -> bool {
    let _profile =
        crate::view::paint::work_profile::scope("artifact_surface_resident_set_is_canonical");
    let mut resident_keys = FxHashSet::default();
    let mut references = vec![0_usize; entries.len()];
    for (ordinal, entry) in entries.iter().enumerate() {
        let stamp = &entry.stamp;
        let Some(program) = stamp.artifact_surface_program.as_ref() else {
            return false;
        };
        if program
            .window
            .is_some_and(|window| !window.matches_target(&stamp.target))
            || program.execution_id.index() != ordinal
            || entry.resident_key != stamp.identity.artifact_surface_resident_key()
            || !resident_keys.insert(entry.resident_key)
            || stamp.clip_nodes.iter().any(|clip| clip.generation == 0)
            || match stamp.local_clip_generation_semantics {
                Some(LocalClipGenerationSemantics::ArtifactLive) => {
                    stamp.identity.role != RetainedSurfaceRasterRole::ScrollContent
                        || stamp.clip_nodes.is_empty()
                }
                None => {
                    stamp.identity.role == RetainedSurfaceRasterRole::ScrollContent
                        && !stamp.clip_nodes.is_empty()
                }
            }
        {
            return false;
        }

        let mut cursor = 0_u32;
        let mut owner_topology = stamp.owner_topology.iter();
        let mut span_clips = stamp.clip_nodes.iter();
        let mut seen_span_clips = FxHashSet::default();
        let mut chunks = stamp.chunks.iter();
        let mut op_count = 0_usize;
        for (step_index, step) in program.steps.iter().enumerate() {
            match step {
                ArtifactSurfaceRasterProgramStepStamp::ArtifactSpan(span) => {
                    if !span.is_canonical(stamp.identity.boundary_root, step_index, cursor) {
                        return false;
                    }
                    cursor = span.opaque_order_span.end;
                    if !span
                        .owner_topology
                        .iter()
                        .all(|owner| owner_topology.next() == Some(owner))
                    {
                        return false;
                    }
                    for clip in &span.clip_nodes {
                        if seen_span_clips.insert(clip.id) && span_clips.next() != Some(clip) {
                            return false;
                        }
                    }
                    if !span
                        .chunks
                        .iter()
                        .all(|chunk| chunks.next() == Some(&chunk.raster))
                    {
                        return false;
                    }
                    op_count = match op_count.checked_add(span.op_count) {
                        Some(count) => count,
                        None => return false,
                    };
                }
                ArtifactSurfaceRasterProgramStepStamp::NestedSurface(dependency) => {
                    let child_index = dependency.child_execution_id.index();
                    let Some(child) = entries.get(child_index).map(|entry| &entry.stamp) else {
                        return false;
                    };
                    let Some(child_program) = child.artifact_surface_program.as_ref() else {
                        return false;
                    };
                    let expected_after = artifact_surface_nested_parent_opaque_after(
                        cursor,
                        child,
                        dependency.child_composite_geometry,
                    );
                    if dependency.step_index != step_index
                        || child_index <= ordinal
                        || dependency.parent_opaque_order_before != cursor
                        || dependency.parent_opaque_order_after != expected_after
                        || dependency.child_stamp.as_ref() != child
                        || !artifact_surface_geometry_matches(
                            child.identity,
                            &child.target,
                            dependency.child_composite_geometry,
                        )
                        || child_program.receiver
                            != SurfaceDagExecutionTargetId::Surface(program.execution_id)
                    {
                        return false;
                    }
                    references[child_index] = match references[child_index].checked_add(1) {
                        Some(count) => count,
                        None => return false,
                    };
                    cursor = expected_after;
                }
            }
        }
        if stamp.opaque_order_span != (0..cursor)
            || owner_topology.next().is_some()
            || span_clips.next().is_some()
            || chunks.next().is_some()
            || stamp.op_count != op_count
        {
            return false;
        }
    }
    entries.iter().enumerate().all(|(ordinal, entry)| {
        let stamp = &entry.stamp;
        let program = stamp
            .artifact_surface_program
            .as_ref()
            .expect("checked above");
        match program.receiver {
            SurfaceDagExecutionTargetId::SceneRoot(_) => references[ordinal] == 0,
            SurfaceDagExecutionTargetId::Surface(_) => references[ordinal] == 1,
        }
    })
}

fn seal_artifact_surface_program_span(
    surface: SurfaceDagNodeId,
    boundary_root: NodeKey,
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
            // Revisions certify the recorded input, but global paint/composite
            // revisions also include layout position and ancestor placement.
            // After localization, complete typed payload identity, bounds,
            // clips, order and nested dependencies identify the actual raster.
            // Do not reintroduce those global placement proxies into equality.
            if chunk.source.owner != boundary_root
                && (chunk.content_revision.self_paint_revision == 0
                    || chunk.content_revision.composite_revision == 0)
            {
                return None;
            }

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
    let sealed = ArtifactSurfaceRasterProgramSpanStamp {
        step_index,
        owner_topology: span.owner_topology.clone(),
        clip_nodes: span.local_clips.clone(),
        op_count: chunks.iter().map(|chunk| chunk.raster.op_count).sum(),
        chunks,
        opaque_order_span: start..end,
    };
    artifact_surface_program_span_is_canonical(&sealed, boundary_root, step_index, start)
        .then_some(sealed)
        .ok_or(ArtifactSurfaceResidentSealError::InvalidArtifactSpan {
            surface,
            step_index,
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
                    let stamp = span.seal_cache.seal(
                        node.source,
                        node.identity.boundary_root,
                        step_index,
                        cursor,
                        span,
                    )?;
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
        if clip_nodes.iter().any(|clip| clip.generation == 0)
            || node.clip_closure.as_ref().is_some_and(|closure| {
                artifact_surface_program_geometry_receiver_clip(node.geometry())
                    != closure.receiver_clip()
            })
        {
            return Err(ArtifactSurfaceResidentSealError::InvalidClipClosure(
                node.source,
            ));
        }
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
        .collect::<Option<Vec<_>>>()
        .ok_or(ArtifactSurfaceResidentSealError::NonCanonicalSet)?;
    artifact_surface_resident_set_is_canonical(&ordered_entries)
        .then(|| {
            let ordered_entries = std::sync::Arc::new(ordered_entries);
            SealedArtifactSurfaceResidentSet {
                validated_entries: ordered_entries.clone(),
                ordered_entries,
            }
        })
        .ok_or(ArtifactSurfaceResidentSealError::NonCanonicalSet)
}

fn artifact_surface_frame_is_canonical(
    plan: &PreparedArtifactSurfaceRasterPlan,
    residents: &SealedArtifactSurfaceResidentSet,
) -> bool {
    let _profile = crate::view::paint::work_profile::scope("artifact_surface_frame_is_canonical");
    residents.is_canonical()
        && plan.nodes.len() == residents.len()
        && plan
            .nodes
            .iter()
            .zip(residents.ordered_entries())
            .all(|(node, resident)| {
                let stamp = resident.stamp();
                let Some(program) = stamp.artifact_surface_program.as_ref() else {
                    return false;
                };
                node.window == program.window
                    && node.execution_id == program.execution_id
                    && node.source == program.source
                    && node.receiver == program.receiver
                    && node.identity == stamp.identity
                    && node.target == stamp.target
                    && artifact_surface_geometry_matches(
                        node.identity,
                        &node.target,
                        node.geometry(),
                    )
                    && match node.receiver {
                        SurfaceDagExecutionTargetId::SceneRoot(_) => true,
                        SurfaceDagExecutionTargetId::Surface(parent) => {
                            residents
                                .ordered_entries()
                                .get(parent.index())
                                .and_then(|parent| parent.stamp().artifact_surface_program.as_ref())
                                .and_then(|parent| {
                                    parent.steps.iter().find_map(|step| match step {
                                    ArtifactSurfaceRasterProgramStepStamp::NestedSurface(
                                        dependency,
                                    ) if dependency.child_execution_id == node.execution_id => {
                                        Some(dependency.child_composite_geometry)
                                    }
                                    ArtifactSurfaceRasterProgramStepStamp::ArtifactSpan(_)
                                    | ArtifactSurfaceRasterProgramStepStamp::NestedSurface(_) => {
                                        None
                                    }
                                })
                                })
                                == Some(node.geometry())
                        }
                    }
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
    artifact_surface_frame_is_canonical(&raster_plan, &residents)
        .then_some(PreparedArtifactSurfaceFrame {
            raster_plan,
            residents,
        })
        .ok_or(ArtifactSurfaceResidentSealError::NonCanonicalSet)
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
/// surface ownership. The legacy artifact compiler and the Surface DAG
/// executor share this exhaustive seven-variant primitive so their payload
/// behavior cannot drift while their sealed scheduling remains independent.
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
                    mesh: op.mesh.as_ref().clone(),
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
/// legacy paint order.
fn exact_self_clip_shadow_prefix_len(
    artifact: &PaintArtifact,
    chunk: &super::PaintChunk,
) -> Option<usize> {
    if artifact.target != PaintArtifactTarget::CurrentTarget
        || artifact.chunks.len() != 1
        || artifact.chunks.first()?.id != chunk.id
        || artifact.owner_nodes.as_slice()
            != [PaintOwnerSnapshot {
                owner: chunk.owner,
                parent: None,
            }]
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
            logical_scissor: clip.logical_scissor,
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
            mesh: op.mesh.clone(),
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
    OpacityMismatch(ArtifactSurfacePaintOpKind),
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
            let mut mesh = shadow.mesh.as_ref().clone();
            for vertex in &mut mesh.vertices {
                translate_nested_scroll_position(vertex, delta).ok_or_else(invalid)?;
            }
            PreparedShadowOp::new(mesh, shadow.params)
                .map(PaintOp::PreparedShadow)
                .ok_or_else(invalid)
        }
        PaintOp::PreparedScrollbarOverlay(overlay) => overlay
            .translated_by(delta)
            .map(PaintOp::scrollbar_overlay)
            .ok_or_else(invalid),
        PaintOp::PreparedText(text) => {
            let mut params = text.params.as_ref().clone();
            if params.scissor_rect.is_some() || params.stencil_clip_id.is_some() {
                return Err(ArtifactSurfaceLocalizationError::EmbeddedClip(kind));
            }
            for fragment in &mut params.fragments {
                translate_nested_scroll_position(&mut fragment.origin, delta)
                    .ok_or_else(invalid)?;
            }
            for glyph in &mut params.staging_input.glyphs {
                let fragment = params
                    .fragments
                    .get(glyph.paint.fragment_index as usize)
                    .ok_or_else(invalid)?;
                glyph.final_paint_pos = [
                    fragment.origin[0] + glyph.paint.local_pos[0],
                    fragment.origin[1] + glyph.paint.local_pos[1],
                ];
                if glyph.final_paint_pos.iter().any(|value| !value.is_finite()) {
                    return Err(invalid());
                }
            }
            PreparedTextOp::new(params)
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
    expected_source_opacity_bits: u32,
) -> Result<PaintOp, ArtifactSurfaceLocalizationError> {
    // Artifact validation proves that an owner-local effect is baked exactly
    // once into every opacity carrier. The detached raster stores neutral 1.0
    // and the typed composite edge retains that same effect snapshot. This is
    // a forward exact-bit check, never a division or tolerance comparison.
    let kind = artifact_surface_paint_op_kind(&op);
    if !ops_have_baked_local_opacity(std::slice::from_ref(&op), expected_source_opacity_bits) {
        return Err(ArtifactSurfaceLocalizationError::OpacityMismatch(kind));
    }
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
            PreparedShadowOp::new(shadow.mesh, params)
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
    let source_opacity = f32::from_bits(expected_source_opacity_bits);
    if (neutral * source_opacity).to_bits() != expected_source_opacity_bits
        || !ops_have_baked_local_opacity(std::slice::from_ref(&rebuilt), neutral.to_bits())
    {
        return Err(ArtifactSurfaceLocalizationError::OpacityMismatch(kind));
    }
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
    expected_source_opacity_bits: u32,
) -> Result<PaintOp, ArtifactSurfaceLocalizationError> {
    neutralize_artifact_surface_opacity(op, expected_source_opacity_bits)
}

#[cfg(test)]
pub(crate) fn artifact_surface_op_has_baked_opacity_for_test(
    op: &PaintOp,
    expected_bits: u32,
) -> bool {
    ops_have_baked_local_opacity(std::slice::from_ref(op), expected_bits)
}

fn child_mask_radii_fit_bounds(radii: [[f32; 2]; 4], [width, height]: [f32; 2]) -> bool {
    // CSS constrains sums of adjacent corners on each edge, not each corner
    // to half the short side. An asymmetric 135px corner on a 150px square
    // is valid when its neighbors are 8px. Keep exact finite/nonnegative and
    // non-overlap checks; do not silently clamp malformed recorded geometry.
    radii.iter().flatten().all(|r| r.is_finite() && *r >= 0.0)
        && radii[0][0] + radii[1][0] <= width
        && radii[3][0] + radii[2][0] <= width
        && radii[0][1] + radii[3][1] <= height
        && radii[1][1] + radii[2][1] <= height
}

fn validate_artifact_store_with_cache(
    artifact: &PaintArtifact,
    policy: ArtifactStoreValidationPolicy,
    cache: Option<&mut PlanningCache>,
) -> Option<ValidatedArtifact> {
    let _profile = crate::view::paint::work_profile::scope("validate_artifact_store");
    let mut cursor = 0usize;
    // A unique (owner, phase, slot) also proves unique chunk ids after the
    // owner check below; role/scope variants may not share the same slot.
    let mut seen_slots =
        FxHashSet::with_capacity_and_hasher(artifact.chunks.len(), Default::default());
    let mut child_mask_stack = Vec::<(
        crate::view::node_arena::NodeKey,
        [u32; 4],
        &PaintPayloadIdentity,
    )>::new();
    for chunk in &artifact.chunks {
        if !super::has_canonical_paint_bounds(chunk.bounds)
            || chunk.id.owner != chunk.owner
            || !seen_slots.insert((chunk.owner, chunk.id.phase, chunk.id.slot))
            || chunk.op_range.start != cursor
            || chunk.op_range.start > chunk.op_range.end
            || chunk.op_range.end > artifact.ops.len()
        {
            return None;
        }
        let properties_are_valid = match policy {
            #[cfg(test)]
            ArtifactStoreValidationPolicy::General => {
                chunk.properties.transform.is_none() && chunk.properties.scroll.is_none()
            }
            ArtifactStoreValidationPolicy::SurfaceDag => true,
        };
        if !properties_are_valid {
            return None;
        }
        let ops = &artifact.ops[chunk.op_range.clone()];
        if chunk.id.slot == super::RETAINED_CHILD_MASK_SLOT {
            let [PaintOp::DrawRect(mask)] = ops else {
                return None;
            };
            let Some(logical_scissor) =
                crate::view::base_component::exact_logical_scissor_for_rect(chunk.bounds)
            else {
                return None;
            };
            let canonical = chunk.id.role == PaintChunkRole::SelfDecoration
                && chunk.id.scope == PaintPropertyScope::Contents
                && mask.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
                && mask.params.position == [chunk.bounds.x, chunk.bounds.y]
                && mask.params.size == [chunk.bounds.width, chunk.bounds.height]
                && mask
                    .params
                    .size
                    .iter()
                    .all(|value| value.is_finite() && *value > 0.0)
                && mask.params.fill_color == [0.0; 4]
                && mask.params.opacity.to_bits() == 1.0_f32.to_bits()
                && mask.params.border_widths == [0.0; 4]
                && child_mask_radii_fit_bounds(mask.params.border_radii, mask.params.size)
                && mask.params.gradient.is_none()
                && mask.params.border_gradient.is_none()
                && chunk.payload_identity.matches_rects([mask]);
            if !canonical {
                return None;
            }
            match chunk.id.phase {
                super::PaintNodePhase::BeforeChildren => {
                    if child_mask_stack.len() >= u8::MAX as usize {
                        return None;
                    }
                    child_mask_stack.push((chunk.owner, logical_scissor, &chunk.payload_identity));
                }
                super::PaintNodePhase::AfterChildren => {
                    if child_mask_stack.pop()
                        != Some((chunk.owner, logical_scissor, &chunk.payload_identity))
                    {
                        return None;
                    }
                }
            }
            cursor = chunk.op_range.end;
            continue;
        }
        match chunk.id.role {
            PaintChunkRole::GpuContent => {
                let [PaintOp::PreparedGpu(op)] = ops else {
                    return None;
                };
                if chunk.payload_identity != PaintPayloadIdentity::Gpu(op.identity()?)
                    || op.params.bounds
                        != [
                            chunk.bounds.x,
                            chunk.bounds.y,
                            chunk.bounds.width,
                            chunk.bounds.height,
                        ]
                    || chunk.id.scope != PaintPropertyScope::SelfPaint
                    || chunk.id.phase != super::PaintNodePhase::BeforeChildren
                    || chunk.id.slot != 0
                {
                    return None;
                }
            }

            PaintChunkRole::ImageContent => {
                if !validate_image_content_ops(ops, &chunk.payload_identity) {
                    return None;
                }
            }
            PaintChunkRole::SvgContent => {
                if !validate_svg_content_ops(ops, &chunk.payload_identity) {
                    return None;
                }
            }
            PaintChunkRole::SelfDecoration => {
                if ops
                    .iter()
                    .any(|op| matches!(op, PaintOp::PreparedImage(_) | PaintOp::PreparedSvg(_)))
                    || !validate_self_decoration_ops(ops, &chunk.payload_identity)
                {
                    return None;
                }
            }
            PaintChunkRole::TextGlyphs => {
                if !validate_text_glyph_ops(ops, &chunk.payload_identity) {
                    return None;
                }
            }
            PaintChunkRole::SelectionUnderlay => {
                let valid = validate_rect_phase_ops(ops, &chunk.payload_identity, false);
                if !valid {
                    return None;
                }
            }
            PaintChunkRole::TextDecoration => {
                if !validate_rect_phase_ops(ops, &chunk.payload_identity, false) {
                    return None;
                }
            }
            PaintChunkRole::Caret => {
                if !validate_rect_phase_ops(ops, &chunk.payload_identity, true) {
                    return None;
                }
            }
            PaintChunkRole::ScrollbarOverlay => {
                let allowed = match policy {
                    ArtifactStoreValidationPolicy::SurfaceDag => {
                        (ops.is_empty()
                            && chunk.payload_identity
                                == PaintPayloadIdentity::prepared_shadows(std::iter::empty()))
                            || matches!(
                                ops,
                                [PaintOp::PreparedScrollbarOverlay(overlay)]
                                    if overlay.has_canonical_identity()
                                        && chunk.payload_identity
                                            == PaintPayloadIdentity::prepared_scrollbar_overlay(
                                                overlay,
                                            )
                            )
                    }
                    #[cfg(test)]
                    ArtifactStoreValidationPolicy::General => false,
                };
                if !allowed {
                    return None;
                }
            }
        }
        cursor = chunk.op_range.end;
    }
    if !child_mask_stack.is_empty() {
        return None;
    }
    if cursor != artifact.ops.len() {
        return None;
    }

    if policy == ArtifactStoreValidationPolicy::SurfaceDag
        && let Some(validated) = cache.and_then(|cache| cache.relations(artifact))
    {
        // Relationships cannot prove that newly recorded ops carry the current
        // local opacity or keep the exact shadow-prefix grammar. Those checks
        // always consume this frame's commands, even after a cache hit.
        let effects = validate_effect_store(artifact)?;
        for chunk in &artifact.chunks {
            let opacity = match chunk.properties.effect {
                Some(id) => {
                    let effect = effects.get(&id)?;
                    if effect.owner == chunk.owner {
                        effect.opacity
                    } else {
                        1.0
                    }
                }
                None => 1.0,
            };
            if validated_artifact_chunk_carries_baked_color_opacity(chunk)
                && !ops_have_baked_local_opacity(
                    &artifact.ops[chunk.op_range.clone()],
                    opacity.to_bits(),
                )
            {
                return None;
            }
            if !chunk_has_valid_self_clip_shadow_prefix(artifact, chunk) {
                return None;
            }
        }
        return Some(validated);
    }

    let owner_nodes = validate_owner_store(artifact)?;
    let mut owner_ancestries = Vec::with_capacity(artifact.chunks.len());
    let mut referenced_owners = FxHashSet::default();
    let mut owner_ancestry_cache = FxHashMap::default();
    for chunk in &artifact.chunks {
        if let Some(ancestry) = owner_ancestry_cache.get(&chunk.owner) {
            owner_ancestries.push(std::sync::Arc::clone(ancestry));
            continue;
        }
        let mut ancestry = FxHashMap::default();
        let mut cursor = chunk.owner;
        let mut depth = 0usize;
        loop {
            if depth >= usize::from(u8::MAX) || ancestry.insert(cursor, depth).is_some() {
                return None;
            }
            let snapshot = *owner_nodes.get(&cursor)?;
            referenced_owners.insert(cursor);
            depth = depth.saturating_add(1);
            let Some(parent) = snapshot.parent else {
                break;
            };
            cursor = parent;
        }
        let ancestry = std::sync::Arc::new(ancestry);
        owner_ancestry_cache.insert(chunk.owner, ancestry.clone());
        owner_ancestries.push(ancestry);
    }
    if referenced_owners.len() != owner_nodes.len() {
        return None;
    }

    let effect_nodes = validate_effect_store(artifact)?;
    let validated_target = match artifact.target {
        PaintArtifactTarget::CurrentTarget => ValidatedArtifactTarget::CurrentTarget,
        PaintArtifactTarget::RootOpacityGroup { root, effect } => {
            if policy == ArtifactStoreValidationPolicy::SurfaceDag {
                return None;
            }
            if effect != EffectNodeId(root)
                || owner_nodes.get(&root)?.parent.is_some()
                || owner_nodes
                    .values()
                    .filter(|snapshot| snapshot.parent.is_none())
                    .count()
                    != 1
                || effect_nodes.len() != 1
            {
                return None;
            }
            let snapshot = *effect_nodes.get(&effect)?;
            if snapshot.owner != root || snapshot.parent.is_some() {
                return None;
            }
            for (chunk, ancestry) in artifact.chunks.iter().zip(&owner_ancestries) {
                if !ancestry.contains_key(&root)
                    || chunk.properties.effect != Some(effect)
                    || chunk.properties.transform.is_some()
                    || chunk.properties.scroll.is_some()
                {
                    return None;
                }
            }
            ValidatedArtifactTarget::RootOpacityGroup {
                #[cfg(test)]
                root,
                #[cfg(test)]
                effect: snapshot,
            }
        }
    };
    let mut referenced_effects = FxHashSet::default();
    let (raster_clips, raster_effects) =
        super::artifact::chunk_raster_property_snapshot_closure(artifact)?;
    let raster_clip_ids = raster_clips
        .iter()
        .map(|snapshot| snapshot.id)
        .collect::<FxHashSet<_>>();
    let raster_effect_ids = raster_effects
        .iter()
        .map(|snapshot| snapshot.id)
        .collect::<FxHashSet<_>>();
    let mut effects_by_owner = FxHashMap::<_, Vec<_>>::default();
    for snapshot in effect_nodes
        .values()
        .filter(|s| raster_effect_ids.contains(&s.id))
    {
        effects_by_owner
            .entry(snapshot.owner)
            .or_default()
            .push(snapshot.id);
    }
    for (chunk, owner_ancestry) in artifact.chunks.iter().zip(&owner_ancestries) {
        // Enumerate the same complete owner/effect relation from its index.
        // Depth sorting below preserves the original validation order.
        let mut expected_effect_chain = owner_ancestry
            .iter()
            .flat_map(|(owner, depth)| {
                effects_by_owner
                    .get(owner)
                    .into_iter()
                    .flatten()
                    .map(move |id| (*depth, *id))
            })
            .collect::<Vec<_>>();
        expected_effect_chain.sort_unstable_by_key(|(owner_depth, _)| *owner_depth);
        if chunk.properties.effect != expected_effect_chain.first().map(|(_, id)| *id) {
            return None;
        }
        for (index, &(_, id)) in expected_effect_chain.iter().enumerate() {
            let expected_parent = expected_effect_chain
                .get(index + 1)
                .map(|(_, parent)| *parent);
            if effect_nodes.get(&id)?.parent != expected_parent {
                return None;
            }
        }
        let baked_expected_opacity = match chunk.properties.effect {
            Some(leaf) => {
                let mut cursor = leaf;
                let mut chain_seen = FxHashSet::default();
                let mut depth = 0usize;
                let mut previous_owner_depth = None;
                loop {
                    if !chain_seen.insert(cursor) || depth >= usize::from(u8::MAX) {
                        return None;
                    }
                    let snapshot = *effect_nodes.get(&cursor)?;
                    let owner_depth = *owner_ancestry.get(&snapshot.owner)?;
                    if previous_owner_depth.is_some_and(|previous| owner_depth <= previous) {
                        return None;
                    }
                    previous_owner_depth = Some(owner_depth);
                    referenced_effects.insert(cursor);
                    depth = depth.saturating_add(1);
                    let Some(parent) = snapshot.parent else {
                        break;
                    };
                    cursor = parent;
                }
                let leaf = effect_nodes.get(&leaf)?;
                if owner_ancestry.get(&leaf.owner) == Some(&0) {
                    leaf.opacity
                } else {
                    1.0
                }
            }
            None => 1.0,
        };
        let expected_opacity = match (policy, validated_target) {
            (_, ValidatedArtifactTarget::CurrentTarget) => baked_expected_opacity,
            (_, ValidatedArtifactTarget::RootOpacityGroup { .. }) => 1.0,
        };
        if validated_artifact_chunk_carries_baked_color_opacity(chunk)
            && !ops_have_baked_local_opacity(
                &artifact.ops[chunk.op_range.clone()],
                expected_opacity.to_bits(),
            )
        {
            return None;
        }
    }
    for endpoint in &artifact.owner_property_states {
        for state in [endpoint.paint, endpoint.descendants] {
            let mut cursor = state.effect;
            let mut seen = FxHashSet::default();
            while let Some(id) = cursor {
                if !seen.insert(id) || seen.len() >= usize::from(u8::MAX) {
                    return None;
                }
                referenced_effects.insert(id);
                cursor = effect_nodes.get(&id)?.parent;
            }
        }
    }
    if referenced_effects.len() != effect_nodes.len() {
        return None;
    }

    let mut clip_nodes = FxHashMap::<ClipNodeId, ClipNodeSnapshot>::default();
    for snapshot in &artifact.clip_nodes {
        if snapshot.id.owner != snapshot.owner
            || !matches!(
                (snapshot.id.role, snapshot.behavior),
                (ClipNodeRole::SelfClip, ClipBehavior::Replace)
                    | (ClipNodeRole::ContentsClip, ClipBehavior::Intersect)
            )
            || snapshot.generation == 0
            || clip_nodes.insert(snapshot.id, *snapshot).is_some()
        {
            return None;
        }
    }

    let mut resolved = Vec::with_capacity(artifact.chunks.len());
    let mut referenced = FxHashSet::default();
    for (chunk, owner_ancestry) in artifact.chunks.iter().zip(&owner_ancestries) {
        let own_self = ClipNodeId {
            owner: chunk.owner,
            role: ClipNodeRole::SelfClip,
        };
        let own_contents = ClipNodeId {
            owner: chunk.owner,
            role: ClipNodeRole::ContentsClip,
        };
        let self_paint_leaf = if raster_clip_ids.contains(&own_self) {
            Some(own_self)
        } else if raster_clip_ids.contains(&own_contents) {
            let contents = clip_nodes.get(&own_contents)?;
            contents.parent
        } else {
            chunk.properties.clip
        };
        let expected_leaf = match chunk.id.scope {
            PaintPropertyScope::SelfPaint => self_paint_leaf,
            PaintPropertyScope::Contents => raster_clip_ids
                .contains(&own_contents)
                .then_some(own_contents)
                .or(self_paint_leaf),
        };
        if chunk.properties.clip != expected_leaf {
            return None;
        }
        // A chunk carrying its owner's `Replace` self clip and an outer-shadow
        // prefix needs phase-sensitive scissoring.  Only the exact, complete
        // single-owner grammar above is allowed to request that split; all
        // fragmented/multi-owner variants fail closed instead of clipping the
        // shadow or leaking unclipped decoration/media.
        if !chunk_has_valid_self_clip_shadow_prefix(artifact, chunk) {
            return None;
        }
        let Some(mut cursor) = expected_leaf else {
            resolved.push(ResolvedClip::Unclipped);
            continue;
        };
        let mut chain = Vec::new();
        let mut chain_seen = FxHashSet::default();
        let mut previous_owner = None;
        loop {
            if !chain_seen.insert(cursor) || chain.len() >= usize::from(u8::MAX) {
                return None;
            }
            let snapshot = *clip_nodes.get(&cursor)?;
            let owner_depth = *owner_ancestry.get(&snapshot.owner)?;
            if let Some((previous_depth, previous_role)) = previous_owner
                && (owner_depth < previous_depth
                    || (owner_depth == previous_depth
                        && !matches!(
                            (previous_role, snapshot.id.role),
                            (ClipNodeRole::ContentsClip, ClipNodeRole::SelfClip)
                        )))
            {
                return None;
            }
            previous_owner = Some((owner_depth, snapshot.id.role));
            referenced.insert(cursor);
            chain.push(snapshot);
            let Some(parent) = snapshot.parent else {
                break;
            };
            cursor = parent;
        }

        let mut clip = ResolvedClip::Unclipped;
        for snapshot in chain.into_iter().rev() {
            clip = match snapshot.behavior {
                ClipBehavior::Replace => resolved_scissor(snapshot.logical_scissor),
                ClipBehavior::Intersect => intersect_resolved_clip(clip, snapshot.logical_scissor),
            };
        }
        resolved.push(clip);
    }
    for endpoint in &artifact.owner_property_states {
        for state in [endpoint.paint, endpoint.descendants] {
            let mut cursor = state.clip;
            let mut seen = FxHashSet::default();
            while let Some(id) = cursor {
                if !seen.insert(id) || seen.len() >= usize::from(u8::MAX) {
                    return None;
                }
                referenced.insert(id);
                cursor = clip_nodes.get(&id)?.parent;
            }
        }
    }
    if referenced.len() != clip_nodes.len() {
        return None;
    }
    Some(ValidatedArtifact {
        resolved_clips: resolved,
        target: validated_target,
    })
}

fn chunk_has_valid_self_clip_shadow_prefix(
    artifact: &PaintArtifact,
    chunk: &super::PaintChunk,
) -> bool {
    let own_self = ClipNodeId {
        owner: chunk.owner,
        role: ClipNodeRole::SelfClip,
    };
    chunk.properties.clip != Some(own_self)
        || !artifact.ops[chunk.op_range.clone()]
            .iter()
            .any(|op| matches!(op, PaintOp::PreparedShadow(_)))
        || exact_self_clip_shadow_prefix_len(artifact, chunk).is_some()
}

fn validate_owner_store(
    artifact: &PaintArtifact,
) -> Option<FxHashMap<crate::view::node_arena::NodeKey, PaintOwnerSnapshot>> {
    let mut nodes = FxHashMap::default();
    for snapshot in &artifact.owner_nodes {
        if snapshot.owner.is_null() || nodes.insert(snapshot.owner, *snapshot).is_some() {
            return None;
        }
    }
    Some(nodes)
}

fn validate_effect_store(
    artifact: &PaintArtifact,
) -> Option<FxHashMap<EffectNodeId, EffectNodeSnapshot>> {
    let mut nodes = FxHashMap::default();
    for snapshot in &artifact.effect_nodes {
        if snapshot.id.0 != snapshot.owner
            || snapshot.generation == 0
            || !snapshot.opacity.is_finite()
            || !(0.0..=1.0).contains(&snapshot.opacity)
            || nodes.insert(snapshot.id, *snapshot).is_some()
        {
            return None;
        }
    }
    Some(nodes)
}

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

/// Whether a fully validated artifact chunk carries baked color opacity.
///
/// Callers must first pass the reserved child-mask slot through the complete
/// canonical child-mask gate. That slot is a stencil program whose opacity is
/// fixed at `1.0`; it never carries the owning Effect's color opacity. Keeping
/// this predicate shared by store validation and raster preparation prevents
/// those two stages from assigning conflicting semantics to the same chunk.
fn validated_artifact_chunk_carries_baked_color_opacity(chunk: &super::PaintChunk) -> bool {
    chunk.id.slot != super::RETAINED_CHILD_MASK_SLOT
}

fn validate_self_decoration_ops(ops: &[PaintOp], payload_identity: &PaintPayloadIdentity) -> bool {
    use crate::view::render_pass::draw_rect_pass::RectRenderMode;

    if matches!(
        payload_identity,
        PaintPayloadIdentity::InlineIfcDecorations(_, _)
    ) {
        let shadow_count = ops
            .iter()
            .take_while(|op| matches!(op, PaintOp::PreparedShadow(_)))
            .count();
        if !ops[..shadow_count].iter().all(
            |op| matches!(op, PaintOp::PreparedShadow(shadow) if shadow.has_canonical_identity()),
        ) {
            return false;
        }
        let decorations = &ops[shadow_count..];
        let mut header = None;
        let mut previous_order = None;
        let last_index = decorations.len().checked_sub(1);
        for (index, op) in decorations.iter().enumerate() {
            let PaintOp::PreparedInlineIfcDecoration(prepared) = op else {
                return false;
            };
            if !prepared.has_canonical_identity() {
                return false;
            }
            let descriptor = &prepared.descriptor;
            let current_header = (
                descriptor.source,
                descriptor.style_key,
                descriptor.slice_insets.map(f32::to_bits),
            );
            if header.is_some_and(|expected| expected != current_header)
                || descriptor.is_first_for_source != (index == 0)
                || descriptor.is_last_for_source != (Some(index) == last_index)
            {
                return false;
            }
            header.get_or_insert(current_header);
            let order = (
                descriptor.line_index,
                descriptor.range.start,
                descriptor.range.end,
            );
            if previous_order.is_some_and(|previous| previous >= order) {
                return false;
            }
            previous_order = Some(order);
        }
        return payload_identity.matches_inline_decorations(
            ops[..shadow_count].iter().filter_map(|op| match op {
                PaintOp::PreparedShadow(shadow) => Some(shadow),
                _ => None,
            }),
            decorations.iter().filter_map(|op| match op {
                PaintOp::PreparedInlineIfcDecoration(prepared) => Some(prepared.as_ref()),
                _ => None,
            }),
        );
    }

    // A non-rendering Element still owns a canonical empty decoration chunk.
    // This exemption is deliberately before shadow-prefix parsing so a
    // shadow-only chunk remains invalid.
    if ops.is_empty() {
        return payload_identity
            .matches_shadows_with_decoration(std::iter::empty(), std::iter::empty());
    }
    let shadow_count = ops
        .iter()
        .take_while(|op| matches!(op, PaintOp::PreparedShadow(_)))
        .count();
    if !ops[..shadow_count]
        .iter()
        .all(|op| matches!(op, PaintOp::PreparedShadow(shadow) if shadow.has_canonical_identity()))
    {
        return false;
    }
    if !payload_identity.matches_shadows_with_decoration(
        ops[..shadow_count].iter().filter_map(|op| match op {
            PaintOp::PreparedShadow(shadow) => Some(shadow),
            _ => None,
        }),
        ops[shadow_count..].iter().filter_map(|op| match op {
            PaintOp::DrawRect(rect) => Some(rect),
            _ => None,
        }),
    ) {
        return false;
    }
    match &ops[shadow_count..] {
        [PaintOp::DrawRect(fill)] => fill.mode == RectRenderMode::FillOnly,
        [PaintOp::DrawRect(fill), PaintOp::DrawRect(border)] => {
            fill.mode == RectRenderMode::FillOnly && border.mode == RectRenderMode::BorderOnly
        }
        _ => false,
    }
}

fn validate_text_glyph_ops(ops: &[PaintOp], payload_identity: &PaintPayloadIdentity) -> bool {
    if !ops.iter().all(|op| {
        matches!(
            op,
            PaintOp::PreparedText(prepared)
                if prepared.params.scissor_rect.is_none() && prepared.has_canonical_identity()
        )
    }) {
        return false;
    }
    payload_identity.matches_texts(ops.iter().filter_map(|op| match op {
        PaintOp::PreparedText(prepared) => Some(prepared),
        _ => None,
    }))
}

fn validate_rect_phase_ops(
    ops: &[PaintOp],
    payload_identity: &PaintPayloadIdentity,
    exactly_one: bool,
) -> bool {
    if ops.is_empty()
        || (exactly_one && ops.len() != 1)
        || !ops.iter().all(|op| {
            matches!(
                op,
                PaintOp::DrawRect(rect)
                    if rect.mode
                        == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
            )
        })
    {
        return false;
    }
    payload_identity.matches_rects(ops.iter().filter_map(|op| match op {
        PaintOp::DrawRect(rect) => Some(rect),
        _ => None,
    }))
}

fn validate_svg_content_ops(ops: &[PaintOp], payload_identity: &PaintPayloadIdentity) -> bool {
    use crate::view::render_pass::draw_rect_pass::RectRenderMode;
    use crate::view::sampled_texture::SampledTextureAlphaMode;

    let (prefix, prepared) = match ops.split_last() {
        Some((PaintOp::PreparedSvg(prepared), prefix)) => (prefix, prepared),
        _ => return false,
    };
    let shadow_count = prefix
        .iter()
        .take_while(|op| matches!(op, PaintOp::PreparedShadow(_)))
        .count();
    if !prefix[..shadow_count]
        .iter()
        .all(|op| matches!(op, PaintOp::PreparedShadow(shadow) if shadow.has_canonical_identity()))
    {
        return false;
    }
    let decoration = &prefix[shadow_count..];
    let decoration_is_valid = match decoration {
        [] => true,
        [PaintOp::DrawRect(fill)] => fill.mode == RectRenderMode::FillOnly,
        [PaintOp::DrawRect(fill), PaintOp::DrawRect(border)] => {
            fill.mode == RectRenderMode::FillOnly && border.mode == RectRenderMode::BorderOnly
        }
        _ => false,
    };
    if !decoration_is_valid {
        return false;
    }
    let params = prepared.params;
    let upload = &prepared.upload;
    if upload.validate_rgba8().is_none()
        || upload.alpha_mode != SampledTextureAlphaMode::Straight
        || params.source_is_premultiplied
        || params.use_mask
        || params.quad_positions.is_some()
        || params.mask_uv_bounds.is_some()
        || params.scissor_rect.is_some()
        || params.uv_bounds.is_none()
    {
        return false;
    }
    let Some(identity) = PreparedSvgIdentity::from_op(prepared) else {
        return false;
    };
    let Some(expected) = PaintPayloadIdentity::svg_with_shadows_and_decoration(
        identity,
        prefix[..shadow_count].iter().filter_map(|op| match op {
            PaintOp::PreparedShadow(shadow) => Some(shadow),
            _ => None,
        }),
        decoration.iter().filter_map(|op| match op {
            PaintOp::DrawRect(rect) => Some(rect),
            _ => None,
        }),
    ) else {
        return false;
    };
    payload_identity == &expected
}

fn validate_image_content_ops(ops: &[PaintOp], payload_identity: &PaintPayloadIdentity) -> bool {
    use crate::view::render_pass::draw_rect_pass::RectRenderMode;
    use crate::view::sampled_texture::{SampledTextureAlphaMode, SampledTextureId};

    let (prefix, prepared) = match ops.split_last() {
        Some((PaintOp::PreparedImage(prepared), prefix)) => (prefix, prepared),
        _ => return false,
    };
    let shadow_count = prefix
        .iter()
        .take_while(|op| matches!(op, PaintOp::PreparedShadow(_)))
        .count();
    if !prefix[..shadow_count]
        .iter()
        .all(|op| matches!(op, PaintOp::PreparedShadow(shadow) if shadow.has_canonical_identity()))
    {
        return false;
    }
    let decoration = &prefix[shadow_count..];
    let decoration_is_valid = match decoration {
        [] => true,
        [PaintOp::DrawRect(fill)] => fill.mode == RectRenderMode::FillOnly,
        [PaintOp::DrawRect(fill), PaintOp::DrawRect(border)] => {
            fill.mode == RectRenderMode::FillOnly && border.mode == RectRenderMode::BorderOnly
        }
        _ => false,
    };
    if !decoration_is_valid {
        return false;
    }
    let params = prepared.params;
    let upload = &prepared.upload;
    if upload.validate_rgba8().is_none()
        || !matches!(upload.id, SampledTextureId::Image(_))
        || upload.alpha_mode != SampledTextureAlphaMode::Straight
        || params.source_is_premultiplied
        || params.use_mask
        || params.quad_positions.is_some()
        || params.mask_uv_bounds.is_some()
        || params.scissor_rect.is_some()
        || params.uv_bounds.is_none()
    {
        return false;
    }
    let Some(expected) = PaintPayloadIdentity::image_with_shadows_and_decoration(
        PreparedImageIdentity::from_op(prepared),
        prefix[..shadow_count].iter().filter_map(|op| match op {
            PaintOp::PreparedShadow(shadow) => Some(shadow),
            _ => None,
        }),
        decoration.iter().filter_map(|op| match op {
            PaintOp::DrawRect(rect) => Some(rect),
            _ => None,
        }),
    ) else {
        return false;
    };
    payload_identity == &expected
}

#[cfg(test)]
pub(crate) fn validate_media_content_artifact_for_test(artifact: &PaintArtifact) -> bool {
    let [chunk] = artifact.chunks.as_slice() else {
        return false;
    };
    let Some(ops) = artifact.ops.get(chunk.op_range.clone()) else {
        return false;
    };
    match chunk.id.role {
        PaintChunkRole::ImageContent => validate_image_content_ops(ops, &chunk.payload_identity),
        PaintChunkRole::SvgContent => validate_svg_content_ops(ops, &chunk.payload_identity),
        _ => false,
    }
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
pub(crate) use direct_command_test_support::{compile_artifact, try_compile_artifact};

#[cfg(test)]
mod child_mask_tests;

mod raster_equivalence;
mod raster_window;

#[cfg(test)]
mod raster_test_support;
#[cfg(test)]
use raster_test_support::artifact_surface_op_corresponds_to_source;
#[cfg(test)]
use raster_test_support::validate_artifact_store_with_policy;

#[cfg(test)]
use super::surface_dag::SurfaceMaterializationDecision;
