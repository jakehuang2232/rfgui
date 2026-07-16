#![allow(dead_code)] // Planning-only M10C1 scaffold; production dispatch begins in C2.

use std::ops::Range;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::view::base_component::{
    Element, RetainedNestedScrollSceneAdmissionSnapshot, RetainedScrollHostAdmissionSnapshot,
    TransformSurfaceGeometrySnapshot,
};
use crate::view::compositor::property_tree::{
    ClipBehavior, ClipGeometry, ClipNodeId, ClipNodeRole, ClipNodeSnapshot, EffectNodeId,
    EffectNodeSnapshot, PropertyTreeState, PropertyTreeValidationError, ScrollNodeId,
    ScrollNodeSnapshot, TransformNodeId, TransformNodeSnapshot,
};
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::node_arena::{NodeArena, NodeKey};

use super::compiler::{
    PropertySceneArtifactPlanWitness, TransformPropertySurfaceArtifactPlanWitness,
};
use super::{
    ConsumedAncestorTransformWitness, EffectPropertyContentWitness,
    EffectPropertySurfaceArtifactContract, FrameArtifactFallbackReason, PaintArtifact, PaintOp,
    PaintOwnerSnapshot, PaintTransformSurfaceWitness,
};

#[derive(Clone, Debug)]
pub(crate) struct FramePaintPlan {
    steps: Vec<PaintPlanStep>,
    property_scene_roots: Option<Vec<PropertySceneRootWitness>>,
    property_scene_seal: Option<PropertyScenePlanSeal>,
}

#[derive(Clone, Debug)]
pub(crate) enum PaintPlanStep {
    ArtifactSpan(ArtifactSpanPlan),
    RetainedSurface(Box<RetainedSurfacePlan>),
}

#[derive(Clone, Debug)]
pub(crate) struct ArtifactSpanPlan {
    artifact: PaintArtifact,
    /// Surface-local opaque depth-order range owned by this artifact. Nested
    /// surfaces start their own counter at zero.
    opaque_order_span: Range<u32>,
}

/// Scene-local deterministic identity. The ordinal freezes DFS boundary
/// order, while owner and transform identity prevent stable-id aliases from
/// being mistaken for the same retained surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PropertySurfaceId {
    ordinal: u32,
    owner: NodeKey,
    transform: TransformNodeId,
}

impl PropertySurfaceId {
    fn new(ordinal: u32, owner: NodeKey, transform: TransformNodeId) -> Option<Self> {
        (owner == transform.0).then_some(Self {
            ordinal,
            owner,
            transform,
        })
    }

    pub(super) fn owner(self) -> NodeKey {
        self.owner
    }
}

#[derive(Clone, Debug)]
struct PropertyScenePlanSeal {
    roots: Vec<PropertySceneRootWitness>,
    context: TransformSurfacePlanContext,
    outer_scissor_rect: Option<[u32; 4]>,
    aggregate_opaque_order_span: Range<u32>,
    surface_count: usize,
    scene_artifact_validation: Vec<PropertySceneArtifactPlanWitness>,
    surfaces: FxHashMap<PropertySurfaceId, TransformPropertySurfaceContract>,
    /// M12A2-0 planning-only opacity/isolation proof. When present, the
    /// executable transform transaction is deliberately unavailable: the
    /// executor/pool/Auto wiring lands in the following slice.
    effect_scaffold: Option<PropertyEffectSceneScaffold>,
    /// M12B4-0 planning-only transform/effect/scroll interleave schedule.
    /// Presence of this scaffold deliberately disables every production
    /// property-scene getter until the joint scene transaction lands.
    scroll_schedule_scaffold: Option<PropertyScrollScheduleScaffold>,
    /// Planning-only exact `S0 -> S1 -> leaf` authority. This is deliberately
    /// separate from B4's one-scroll grammar and cannot mint a transaction,
    /// pool action or frame-graph handle.
    nested_scroll_scaffold: Option<NestedScrollSceneScaffold>,
}

#[derive(Clone, Debug)]
pub(super) struct NestedScrollSceneScaffold {
    pub(super) context: TransformSurfacePlanContext,
    pub(super) admission: RetainedNestedScrollSceneAdmissionSnapshot,
    pub(super) boundaries: Vec<NestedScrollBoundaryContract>,
    pub(super) schedule: NestedScrollSceneSchedule,
    planned_context: TransformSurfacePlanContext,
    planned_admission: RetainedNestedScrollSceneAdmissionSnapshot,
    planned_boundaries: Vec<NestedScrollBoundaryContract>,
    planned_schedule: NestedScrollSceneSchedule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NestedScrollBoundarySlot {
    Outer,
    Inner,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NestedScrollBoundaryContract {
    pub(super) slot: NestedScrollBoundarySlot,
    pub(super) boundary_root: NodeKey,
    pub(super) stable_id: u64,
    pub(super) parent: Option<NestedScrollBoundarySlot>,
    pub(super) scroll: ScrollNodeSnapshot,
    pub(super) contents_clip: ClipNodeSnapshot,
    pub(super) content_state: PropertyTreeState,
    pub(super) projected_receiver_state: PropertyTreeState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NestedScrollSceneSchedule {
    pub(super) steps: Vec<NestedScrollSceneScheduledStep>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NestedScrollSceneScheduledStep {
    HostBefore {
        boundary: NestedScrollBoundarySlot,
        artifact: NestedScrollArtifactSeal,
    },
    ContentReceiver(NestedScrollContentReceiverIdentity),
    OverlayAfter {
        boundary: NestedScrollBoundarySlot,
        artifact: NestedScrollArtifactSeal,
    },
}

#[derive(Clone, Debug)]
pub(super) struct NestedScrollArtifactSeal {
    recorded_artifact: PaintArtifact,
    pub(super) identity: PropertyScrollReceiverArtifactIdentity,
}

impl PartialEq for NestedScrollArtifactSeal {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}

impl Eq for NestedScrollArtifactSeal {}

impl NestedScrollArtifactSeal {
    pub(super) fn artifact(&self) -> &PaintArtifact {
        &self.recorded_artifact
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NestedScrollContentReceiverIdentity {
    pub(super) stable_id: u64,
    pub(super) witness: super::PaintNestedScrollContentWitness,
    pub(super) live_input: PropertyTreeState,
    pub(super) projected_output: PropertyTreeState,
    pub(super) artifact: NestedScrollArtifactSeal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropertyScrollScheduleScaffold {
    pub(super) context: TransformSurfacePlanContext,
    pub(super) roots: Vec<PropertyScrollScheduleRoot>,
    pub(super) schedule: PropertySceneSchedule,
    pub(super) boundaries: Vec<PropertyScrollBoundaryContract>,
    pub(super) receiver_insertions: Vec<PropertyScrollReceiverInsertionContract>,
    pub(super) effect_receiver_insertions: Vec<PropertyEffectScrollReceiverInsertionContract>,
    pub(super) transform_effect_receiver_insertions:
        Vec<PropertyTransformEffectScrollReceiverInsertionContract>,
    planned_context: TransformSurfacePlanContext,
    planned_roots: Vec<PropertyScrollScheduleRoot>,
    planned_schedule: PropertySceneSchedule,
    planned_boundaries: Vec<PropertyScrollBoundaryContract>,
    planned_receiver_insertions: Vec<PropertyScrollReceiverInsertionContract>,
    planned_effect_receiver_insertions: Vec<PropertyEffectScrollReceiverInsertionContract>,
    planned_transform_effect_receiver_insertions:
        Vec<PropertyTransformEffectScrollReceiverInsertionContract>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropertyScrollReceiverInsertionContract {
    pub(super) scene_root_ordinal: u32,
    pub(super) receiver: TransformNodeSnapshot,
    pub(super) receiver_stable_id: u64,
    pub(super) scroll_boundary_ordinal: u32,
    pub(super) scroll_cutout: super::PlannedBoundary,
    pub(super) insertion_index: usize,
    pub(super) before_span: Range<usize>,
    pub(super) after_span: Range<usize>,
    pub(super) receiver_opaque_before: u32,
    pub(super) receiver_opaque_after: u32,
    recorded_steps: Vec<PropertyScrollReceiverRecordedStepIdentity>,
}

/// Planning/compiler checkpoint for the strict direct `Effect ->
/// ScrollContents` receiver. `receiver` is final-composite authority only;
/// the raster identity intentionally lives in the sibling effect-neutral
/// artifact contract and ordered recorded steps, neither of which contains
/// the receiver's own opacity or effect generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropertyEffectScrollReceiverInsertionContract {
    pub(super) scene_root_ordinal: u32,
    pub(super) receiver: EffectNodeSnapshot,
    pub(super) receiver_stable_id: u64,
    pub(super) scroll_boundary_ordinal: u32,
    pub(super) scroll_cutout: super::PlannedBoundary,
    pub(super) insertion_index: usize,
    pub(super) before_span: Range<usize>,
    pub(super) after_span: Range<usize>,
    pub(super) receiver_opaque_before: u32,
    pub(super) receiver_opaque_after: u32,
    pub(super) raster_bounds_bits: [u32; 4],
    pub(super) artifact_contract: EffectPropertySurfaceArtifactContract,
    raster_identity: PropertyEffectScrollReceiverRasterIdentity,
    recorded_steps: Vec<PropertyScrollReceiverRecordedStepIdentity>,
}

/// Strict direct `Transform -> Effect -> ScrollContents` insertion authority.
/// The outer transform owns only its child-effect cutout and final translation;
/// the inner contract owns the effect-neutral H/C/O raster and scroll cutout.
#[derive(Clone, Debug)]
pub(super) struct PropertyTransformEffectScrollReceiverInsertionContract {
    pub(super) scene_root_ordinal: u32,
    pub(super) outer_receiver: TransformNodeSnapshot,
    pub(super) outer_stable_id: u64,
    pub(super) outer_geometry: TransformSurfaceGeometrySnapshot,
    pub(super) effect_cutout: super::PlannedBoundary,
    pub(super) outer_insertion_index: usize,
    pub(super) outer_before_span: Range<usize>,
    pub(super) outer_after_span: Range<usize>,
    pub(super) outer_opaque_before: u32,
    pub(super) outer_opaque_after: u32,
    pub(super) inner: PropertyEffectScrollReceiverInsertionContract,
    outer_recorded_steps: Vec<PropertyScrollReceiverRecordedStepIdentity>,
}

impl PartialEq for PropertyTransformEffectScrollReceiverInsertionContract {
    fn eq(&self, other: &Self) -> bool {
        self.scene_root_ordinal == other.scene_root_ordinal
            && self.outer_receiver == other.outer_receiver
            && self.outer_stable_id == other.outer_stable_id
            && self.outer_geometry.bitwise_eq(other.outer_geometry)
            && self.effect_cutout == other.effect_cutout
            && self.outer_insertion_index == other.outer_insertion_index
            && self.outer_before_span == other.outer_before_span
            && self.outer_after_span == other.outer_after_span
            && self.outer_opaque_before == other.outer_opaque_before
            && self.outer_opaque_after == other.outer_opaque_after
            && self.inner == other.inner
            && self.outer_recorded_steps == other.outer_recorded_steps
    }
}

impl Eq for PropertyTransformEffectScrollReceiverInsertionContract {}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyEffectScrollReceiverRasterIdentity {
    receiver_owner: NodeKey,
    receiver_stable_id: u64,
    raster_bounds_bits: [u32; 4],
    local_raster_clips: Vec<ClipNodeSnapshot>,
    content: Vec<EffectPropertyContentWitness>,
    recorded_steps: Vec<PropertyScrollReceiverRecordedStepIdentity>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PropertyScrollReceiverRecordedStepIdentity {
    Artifact(PropertyScrollReceiverArtifactIdentity),
    ScrollCutout(super::PlannedBoundary),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropertyScrollReceiverArtifactIdentity {
    owner_topology: Vec<PaintOwnerSnapshot>,
    clip_nodes: Vec<ClipNodeSnapshot>,
    effect_nodes: Vec<EffectNodeSnapshot>,
    chunks: Vec<PropertyScrollReceiverChunkIdentity>,
    op_count: usize,
    opaque_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyScrollReceiverChunkIdentity {
    id: super::PaintChunkId,
    owner: NodeKey,
    bounds_bits: [u32; 4],
    properties: PropertyTreeState,
    content_revision: super::PaintContentRevision,
    payload_identity: super::PaintPayloadIdentity,
    op_count: usize,
}

impl PropertyScrollReceiverInsertionContract {
    pub(super) fn validates_recorded_steps(
        &self,
        steps: &[super::frame_recorder::RecordedTransformSurfaceStep],
    ) -> bool {
        let identity = steps
            .iter()
            .map(|step| match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    property_scroll_receiver_artifact_identity(artifact)
                        .map(PropertyScrollReceiverRecordedStepIdentity::Artifact)
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(boundary) => Some(
                    PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(*boundary),
                ),
            })
            .collect::<Option<Vec<_>>>();
        identity.as_deref() == Some(self.recorded_steps.as_slice())
    }
}

impl PropertyEffectScrollReceiverInsertionContract {
    pub(super) fn validates_recorded_steps(
        &self,
        steps: &[super::frame_recorder::RecordedTransformSurfaceStep],
    ) -> bool {
        let identity = steps
            .iter()
            .map(|step| match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    property_scroll_receiver_artifact_identity(artifact)
                        .map(PropertyScrollReceiverRecordedStepIdentity::Artifact)
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(boundary) => Some(
                    PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(*boundary),
                ),
            })
            .collect::<Option<Vec<_>>>();
        identity.as_deref() == Some(self.recorded_steps.as_slice())
    }

    /// Effect composite authority is deliberately omitted. This witness is
    /// what the B4-2C compiler checkpoint compares across opacity-only edits.
    fn raster_identity_for_checkpoint(&self) -> &PropertyEffectScrollReceiverRasterIdentity {
        &self.raster_identity
    }

    pub(super) fn has_same_raster_identity(&self, other: &Self) -> bool {
        self.raster_identity == other.raster_identity
    }
}

impl PropertyTransformEffectScrollReceiverInsertionContract {
    pub(super) fn validates_outer_recorded_steps(
        &self,
        steps: &[super::frame_recorder::RecordedTransformSurfaceStep],
    ) -> bool {
        let identity = steps
            .iter()
            .map(|step| match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    property_scroll_receiver_artifact_identity(artifact)
                        .map(PropertyScrollReceiverRecordedStepIdentity::Artifact)
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(boundary) => Some(
                    PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(*boundary),
                ),
            })
            .collect::<Option<Vec<_>>>();
        identity.as_deref() == Some(self.outer_recorded_steps.as_slice())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropertySceneSchedule {
    pub(super) steps: Vec<PropertySceneScheduledStep>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropertyScrollScheduleRoot {
    pub(super) ordinal: u32,
    pub(super) root: NodeKey,
    pub(super) stable_id: u64,
    pub(super) step_span: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PropertySceneScheduledStep {
    RetainedSurface {
        boundary: PropertyScheduledSurfaceBoundary,
        parent: Option<PropertyScheduledSurfaceBoundaryId>,
    },
    ScrollBoundary {
        boundary_ordinal: u32,
        scroll: ScrollNodeId,
        basis: ScrollCompositeBasis,
        phase: PropertyScrollPhaseSchedule,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum PropertyScheduledSurfaceBoundaryId {
    Transform(TransformNodeId),
    Effect(EffectNodeId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PropertyScheduledSurfaceBoundary {
    Transform(TransformNodeSnapshot),
    Effect(EffectNodeSnapshot),
}

impl PropertyScheduledSurfaceBoundary {
    fn id(&self) -> PropertyScheduledSurfaceBoundaryId {
        match self {
            Self::Transform(snapshot) => PropertyScheduledSurfaceBoundaryId::Transform(snapshot.id),
            Self::Effect(snapshot) => PropertyScheduledSurfaceBoundaryId::Effect(snapshot.id),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropertyScrollBoundaryContract {
    pub(super) ordinal: u32,
    pub(super) scene_root_ordinal: u32,
    pub(super) scroll: ScrollNodeSnapshot,
    pub(super) contents_clip: ClipNodeSnapshot,
    pub(super) basis: ScrollCompositeBasis,
    pub(super) phase: PropertyScrollPhaseSchedule,
    pub(super) consumed_properties: ConsumedPropertyStack,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ScrollCompositeBasis {
    FrameRoot,
    Transform(TransformNodeSnapshot),
    Effect(EffectNodeSnapshot),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropertyScrollPhaseSchedule {
    pub(super) host_before: PropertyScrollPhaseSlot,
    pub(super) content_gap: PropertyScrollContentPhase,
    pub(super) overlay_after: PropertyScrollPhaseSlot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PropertyScrollPhaseSlot {
    pub(super) owner: NodeKey,
    pub(super) phase: PropertyScrollPhaseKind,
    pub(super) receiver_state: PropertyTreeState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PropertyScrollContentPhase {
    pub(super) owner: NodeKey,
    pub(super) phase: PropertyScrollPhaseKind,
    pub(super) content_state: PropertyTreeState,
    pub(super) projected_receiver_state: PropertyTreeState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PropertyScrollPhaseKind {
    HostBeforeChildren,
    DetachedContentComposite,
    OverlayAfterChildren,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ConsumedPropertyStack {
    pub(super) target_owner: NodeKey,
    pub(super) live_input: PropertyTreeState,
    pub(super) entries: Vec<ConsumedPropertyEntry>,
    pub(super) projected_output: PropertyTreeState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ConsumedPropertyEntry {
    pub(super) boundary: ConsumedPropertyBoundary,
    pub(super) expected_before: PropertyTreeState,
    pub(super) projected_after: PropertyTreeState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ConsumedPropertyBoundary {
    Transform(TransformNodeId),
    Effect(EffectNodeId),
    ScrollContents {
        scroll: ScrollNodeId,
        contents_clip: ClipNodeId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PropertyBoundaryId {
    Transform(TransformNodeId),
    Effect(EffectNodeId),
}

impl PropertyBoundaryId {
    fn owner(self) -> NodeKey {
        match self {
            Self::Transform(id) => id.0,
            Self::Effect(id) => id.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyEffectSceneScaffold {
    context: TransformSurfacePlanContext,
    outer_scissor_rect: Option<[u32; 4]>,
    planned_context: TransformSurfacePlanContext,
    planned_outer_scissor_rect: Option<[u32; 4]>,
    roots: Vec<PropertyEffectRootWitness>,
    surfaces: Vec<PropertyEffectSurfaceContract>,
    clip_forest: PropertyEffectClipForestContract,
    production_root_step_spans: Option<Vec<Range<usize>>>,
    planned_roots: Vec<PropertyEffectRootWitness>,
    planned_surfaces: Vec<PropertyEffectSurfaceContract>,
    planned_clip_forest: PropertyEffectClipForestContract,
    planned_production_root_step_spans: Option<Vec<Range<usize>>>,
}

/// Owning snapshot of the property clip forest and every reachable node's
/// clip leaves. Surface-local/composite clip partitions are derived from this
/// authority during sealing; they cannot be validated by resolving their own
/// lists alone.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyEffectClipForestContract {
    states: Vec<PropertyEffectClipStateWitness>,
    nodes: Vec<ClipNodeSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PropertyEffectClipStateWitness {
    owner: NodeKey,
    stable_id: u64,
    parent: Option<NodeKey>,
    paint_leaf: Option<ClipNodeId>,
    descendants_leaf: Option<ClipNodeId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyEffectRootWitness {
    ordinal: u32,
    root: NodeKey,
    stable_id: u64,
    boundary_ordinal_span: Range<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyEffectSurfaceContract {
    ordinal: u32,
    boundary: PropertyBoundaryId,
    stable_id: u64,
    parent_boundary_ordinal: Option<u32>,
    scene_root_ordinal: u32,
    kind: PropertyEffectSurfaceKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PropertyEffectSurfaceKind {
    Transform {
        snapshot: TransformNodeSnapshot,
        nested_effect_dependencies: Vec<PropertyIsolationNestedDependencySpec>,
    },
    Isolation(PropertyIsolationBoundaryContract),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropertyIsolationBoundaryContract {
    pub(super) effect_chain: AncestorEffectDetachmentWitness,
    pub(super) raster_space: PropertyIsolationRasterSpaceSnapshot,
    pub(super) composite: PropertyIsolationCompositeMappingSnapshot,
    pub(super) local_raster_clips: Vec<ClipNodeSnapshot>,
    pub(super) ancestor_composite_clips: Vec<ClipNodeSnapshot>,
    pub(super) raster_identity: PropertyIsolationRasterIdentitySpec,
    pub(super) nested_dependencies: Vec<PropertyIsolationNestedDependencySpec>,
    /// An isolation composite is translucent composition even when opacity is
    /// exactly one. It must never advance the parent's opaque-rect cursor.
    pub(super) parent_opaque_cursor_delta: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AncestorEffectDetachmentWitness {
    pub(super) live_leaf_to_root: Vec<EffectNodeSnapshot>,
    pub(super) isolated_leaf: EffectNodeSnapshot,
    pub(super) detached_ancestors: Vec<EffectNodeSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PropertyIsolationRasterSpaceSnapshot {
    pub(super) paint_offset_bits: [u32; 2],
    pub(super) source_bounds_bits: [u32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PropertyIsolationCompositeBasis {
    FrameRoot,
    ParentEffect(EffectNodeId),
    ParentTransform {
        transform: TransformNodeId,
        viewport_matrix_bits: [u32; 16],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PropertyIsolationCompositeMappingSnapshot {
    pub(super) basis: PropertyIsolationCompositeBasis,
    pub(super) rect_bits: [u32; 4],
    pub(super) opacity_bits: u32,
    pub(super) effect_generation: u64,
    pub(super) resolved_scissor: Option<[u32; 4]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PropertyIsolationContentGenerationWitness {
    pub(super) owner: NodeKey,
    pub(super) stable_id: u64,
    pub(super) parent: Option<NodeKey>,
    pub(super) self_paint_revision: u64,
    pub(super) topology_revision: u64,
}

/// Complete planning-time artifact-input fingerprint for one isolation raster.
/// It owns the surface identity, raster geometry, local clips, ordered content
/// topology, and recursively frozen nested-isolation inputs. Own opacity and
/// effect generation are intentionally absent; both are composite-only. Hidden
/// opacity-zero surfaces still retain exact content/topology generations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropertyIsolationRasterIdentitySpec {
    pub(super) boundary: EffectNodeId,
    pub(super) stable_id: u64,
    pub(super) raster_space: PropertyIsolationRasterSpaceSnapshot,
    pub(super) local_raster_clips: Vec<ClipNodeSnapshot>,
    pub(super) content: Vec<PropertyIsolationContentGenerationWitness>,
    pub(super) nested_dependencies: Vec<PropertyIsolationNestedDependencySpec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropertyIsolationNestedDependencySpec {
    pub(super) child_boundary_ordinal: u32,
    pub(super) child_effect: EffectNodeId,
    pub(super) child_stable_id: u64,
    pub(super) child_opacity_bits: u32,
    pub(super) child_effect_generation: u64,
    pub(super) child_rect_bits: [u32; 4],
    pub(super) child_raster_identity: Box<PropertyIsolationRasterIdentitySpec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertySceneRootWitness {
    ordinal: u32,
    root: NodeKey,
    stable_id: u64,
    owner: PaintOwnerSnapshot,
    top_level_step_span: Range<usize>,
}

/// Owning structural proof handed from the planner to the property-scene
/// executor.  It intentionally contains no artifact or frame-graph handle;
/// the executor must still validate every artifact and raster stamp before
/// the first graph mutation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PropertySceneTransactionWitness {
    pub(crate) roots: Vec<PropertySceneTransactionRootWitness>,
    pub(crate) surfaces: Vec<PropertySceneTransactionSurfaceWitness>,
    pub(crate) top_level_surfaces: Vec<PropertySceneTopLevelSurfaceWitness>,
    pub(crate) aggregate_opaque_order_span: Range<u32>,
    pub(crate) outer_scissor_rect: Option<[u32; 4]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PropertySceneTransactionRootWitness {
    pub(crate) ordinal: u32,
    pub(crate) root: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) top_level_step_span: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PropertySceneTransactionSurfaceWitness {
    pub(crate) ordinal: u32,
    pub(crate) boundary_root: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) persistent_color_key: crate::view::frame_graph::PersistentTextureKey,
    pub(crate) parent_surface: Option<NodeKey>,
    pub(crate) scene_root: NodeKey,
    pub(crate) kind: PropertySceneTransactionSurfaceKind,
    pub(crate) transform_viewport_matrix_bits: Option<[u32; 16]>,
    pub(crate) effect_composite: Option<PropertySceneEffectCompositeWitness>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PropertySceneEffectCompositeWitness {
    pub(super) mapping: PropertyIsolationCompositeMappingSnapshot,
    pub(super) ancestor_composite_clips: Vec<ClipNodeSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PropertySceneTransactionSurfaceKind {
    Transform(TransformNodeId),
    Effect(EffectNodeId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PropertySceneTopLevelSurfaceWitness {
    pub(crate) step_index: usize,
    pub(crate) surface_ordinal: u32,
    pub(crate) scene_root_ordinal: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct RetainedSurfacePlan {
    boundary_root: NodeKey,
    stable_id: u64,
    persistent_color_key: crate::view::frame_graph::PersistentTextureKey,
    kind: SurfaceKind,
    /// Owning raster stream for this surface. Artifact spans and nested
    /// surfaces appear in exact paint order; no step may call back into a
    /// live `Renderable`.
    raster_steps: Vec<PaintPlanStep>,
    /// Parent retained-surface boundary. This is a topology/dependency edge,
    /// never a request to derive an inverse-relative transform.
    parent_surface: Option<NodeKey>,
    /// Surface-local terminal counter, always `0..terminal`. A child surface
    /// returns its own terminal and the parent merges it with `max`; counters
    /// are never added across render targets.
    aggregate_opaque_order_span: Range<u32>,
}

#[derive(Clone, Debug)]
pub(crate) enum SurfaceKind {
    Transform(TransformSurfacePlan),
    Isolation(IsolationSurfacePlan),
    NestedIsolation(NestedIsolationSurfacePlan),
    ScrollHost(ScrollHostSurfacePlan),
}

#[derive(Clone, Debug)]
pub(crate) struct TransformSurfacePlan {
    pub(super) transform: TransformNodeId,
    pub(super) geometry: TransformSurfaceGeometrySnapshot,
    pub(super) context: TransformSurfacePlanContext,
    pub(super) planned_geometry_witness: TransformSurfaceGeometrySnapshot,
    pub(super) planned_context_witness: TransformSurfacePlanContext,
}

#[derive(Clone, Debug)]
struct TransformPropertySurfaceContract {
    id: PropertySurfaceId,
    parent: Option<PropertySurfaceId>,
    scene_root: NodeKey,
    stable_id: u64,
    transform: TransformNodeSnapshot,
    planned_transform_witness: TransformNodeSnapshot,
    /// Leaf-to-root exact ancestor clip chain, consumed only by the composite
    /// edge. Surface artifacts contain a detached local clip chain.
    ancestor_composite_clips: Vec<ClipNodeSnapshot>,
    resolved_composite_scissor: Option<[u32; 4]>,
    artifact_validation: Vec<TransformPropertySurfaceArtifactPlanWitness>,
}

#[derive(Clone, Debug)]
pub(crate) struct IsolationSurfacePlan {
    pub(super) effect: EffectNodeSnapshot,
    pub(super) geometry: IsolationSurfaceGeometrySnapshot,
    pub(super) planned_geometry_witness: IsolationSurfaceGeometrySnapshot,
}

#[derive(Clone, Debug)]
pub(crate) struct NestedIsolationSurfacePlan {
    pub(super) effect: EffectNodeSnapshot,
    pub(super) geometry: NestedIsolationSurfaceGeometrySnapshot,
    pub(super) planned_geometry_witness: NestedIsolationSurfaceGeometrySnapshot,
    /// Present only for M12A2 PropertyScene effect surfaces. Named effect-tree
    /// canaries keep this absent, so their exact depth-two gate cannot alias
    /// the arbitrary-depth production authority.
    pub(super) property_scene: Option<PropertyIsolationBoundaryContract>,
    /// Compiler-owned, cutout-aware artifact authority. Kept beside the
    /// structural contract so the validated token can borrow stable plan
    /// storage without constructing a self-referential prepared object.
    pub(super) property_scene_artifact: Option<EffectPropertySurfaceArtifactContract>,
}

#[derive(Clone, Debug)]
pub(crate) struct ScrollHostSurfacePlan {
    pub(super) scroll: ScrollNodeSnapshot,
    pub(super) contents_clip: ClipNodeSnapshot,
    pub(super) admission: RetainedScrollHostAdmissionSnapshot,
    pub(super) planned_scroll_witness: ScrollNodeSnapshot,
    pub(super) planned_clip_witness: ClipNodeSnapshot,
    pub(super) planned_admission_witness: RetainedScrollHostAdmissionSnapshot,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct IsolationSurfaceGeometrySnapshot {
    pub(super) source_bounds: crate::view::base_component::PromotionCompositeBounds,
    pub(super) logical_size: [f32; 2],
    pub(super) outer_scissor_rect: Option<[u32; 4]>,
}

/// Child-local raster and composite geometry for the one exact mixed tree
/// shape. Unlike the root isolation geometry, this is never viewport-sized:
/// it owns the direct child's exact retained render output verbatim.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NestedIsolationSurfaceGeometrySnapshot {
    pub(super) source_bounds: crate::view::base_component::PromotionCompositeBounds,
}

impl FramePaintPlan {
    pub(super) fn steps(&self) -> &[PaintPlanStep] {
        &self.steps
    }

    pub(super) fn property_scene_transaction_witness(
        &self,
    ) -> Option<PropertySceneTransactionWitness> {
        if !property_scene_plan_is_sealed(self) {
            return None;
        }
        let seal = self.property_scene_seal.as_ref()?;
        if seal.scroll_schedule_scaffold.is_some() || seal.nested_scroll_scaffold.is_some() {
            return None;
        }
        let roots = if let Some(scaffold) = &seal.effect_scaffold {
            let spans = scaffold.production_root_step_spans.as_ref()?;
            if spans.len() != scaffold.roots.len() {
                return None;
            }
            scaffold
                .roots
                .iter()
                .zip(spans)
                .map(|(root, span)| PropertySceneTransactionRootWitness {
                    ordinal: root.ordinal,
                    root: root.root,
                    stable_id: root.stable_id,
                    top_level_step_span: span.clone(),
                })
                .collect::<Vec<_>>()
        } else {
            seal.roots
                .iter()
                .map(|root| PropertySceneTransactionRootWitness {
                    ordinal: root.ordinal,
                    root: root.root,
                    stable_id: root.stable_id,
                    top_level_step_span: root.top_level_step_span.clone(),
                })
                .collect::<Vec<_>>()
        };
        let mut surfaces = if let Some(scaffold) = &seal.effect_scaffold {
            scaffold
                .surfaces
                .iter()
                .map(|contract| {
                    let (
                        kind,
                        persistent_color_key,
                        transform_viewport_matrix_bits,
                        effect_composite,
                    ) = match contract.boundary {
                        PropertyBoundaryId::Transform(id) => (
                            PropertySceneTransactionSurfaceKind::Transform(id),
                            crate::view::base_component::transformed_layer_stable_key(
                                contract.stable_id,
                            ),
                            match &contract.kind {
                                PropertyEffectSurfaceKind::Transform { snapshot, .. } => {
                                    Some(snapshot.viewport_matrix.to_cols_array().map(f32::to_bits))
                                }
                                PropertyEffectSurfaceKind::Isolation(_) => return None,
                            },
                            None,
                        ),
                        PropertyBoundaryId::Effect(id) => (
                            PropertySceneTransactionSurfaceKind::Effect(id),
                            crate::view::base_component::isolation_layer_stable_key(
                                contract.stable_id,
                            ),
                            None,
                            match &contract.kind {
                                PropertyEffectSurfaceKind::Isolation(isolation) => {
                                    Some(PropertySceneEffectCompositeWitness {
                                        mapping: isolation.composite,
                                        ancestor_composite_clips: isolation
                                            .ancestor_composite_clips
                                            .clone(),
                                    })
                                }
                                PropertyEffectSurfaceKind::Transform { .. } => return None,
                            },
                        ),
                    };
                    Some(PropertySceneTransactionSurfaceWitness {
                        ordinal: contract.ordinal,
                        boundary_root: contract.boundary.owner(),
                        stable_id: contract.stable_id,
                        persistent_color_key,
                        parent_surface: match contract.parent_boundary_ordinal {
                            Some(ordinal) => {
                                Some(scaffold.surfaces.get(ordinal as usize)?.boundary.owner())
                            }
                            None => None,
                        },
                        scene_root: scaffold
                            .roots
                            .get(contract.scene_root_ordinal as usize)?
                            .root,
                        kind,
                        transform_viewport_matrix_bits,
                        effect_composite,
                    })
                })
                .collect::<Option<Vec<_>>>()?
        } else {
            seal.surfaces
                .values()
                .map(|contract| PropertySceneTransactionSurfaceWitness {
                    ordinal: contract.id.ordinal,
                    boundary_root: contract.id.owner,
                    stable_id: contract.stable_id,
                    persistent_color_key: crate::view::base_component::transformed_layer_stable_key(
                        contract.stable_id,
                    ),
                    parent_surface: contract.parent.map(PropertySurfaceId::owner),
                    scene_root: contract.scene_root,
                    kind: PropertySceneTransactionSurfaceKind::Transform(contract.id.transform),
                    transform_viewport_matrix_bits: Some(
                        contract
                            .transform
                            .viewport_matrix
                            .to_cols_array()
                            .map(f32::to_bits),
                    ),
                    effect_composite: None,
                })
                .collect::<Vec<_>>()
        };
        surfaces.sort_unstable_by_key(|surface| surface.ordinal);
        let surface_ordinals = surfaces
            .iter()
            .map(|surface| (surface.boundary_root, surface.ordinal))
            .collect::<FxHashMap<_, _>>();
        let mut top_level_surfaces = Vec::new();
        for root in &roots {
            for step_index in root.top_level_step_span.clone() {
                let PaintPlanStep::RetainedSurface(surface) = &self.steps[step_index] else {
                    continue;
                };
                top_level_surfaces.push(PropertySceneTopLevelSurfaceWitness {
                    step_index,
                    surface_ordinal: *surface_ordinals.get(&surface.boundary_root())?,
                    scene_root_ordinal: root.ordinal,
                });
            }
        }
        Some(PropertySceneTransactionWitness {
            roots,
            surfaces,
            top_level_surfaces,
            aggregate_opaque_order_span: seal.aggregate_opaque_order_span.clone(),
            outer_scissor_rect: seal.outer_scissor_rect,
        })
    }

    pub(super) fn property_scene_context(&self) -> Option<TransformSurfacePlanContext> {
        property_scene_plan_is_sealed(self)
            .then(|| {
                self.property_scene_seal.as_ref().and_then(|seal| {
                    (seal.scroll_schedule_scaffold.is_none()
                        && seal.nested_scroll_scaffold.is_none()
                        && seal
                            .effect_scaffold
                            .as_ref()
                            .is_none_or(|scaffold| scaffold.production_root_step_spans.is_some()))
                    .then_some(seal.context)
                })
            })
            .flatten()
    }

    /// Planning-only B4-2A insertion authority.  This exposes no transaction,
    /// pool action, graph handle, or production context token; B4-2B must
    /// still materialize and compiler-seal the exact receiver dependency.
    pub(super) fn property_scroll_receiver_insertions(
        &self,
    ) -> Option<&[PropertyScrollReceiverInsertionContract]> {
        property_scene_plan_is_sealed(self).then_some(())?;
        self.property_scene_seal
            .as_ref()?
            .scroll_schedule_scaffold
            .as_ref()
            .map(|scaffold| scaffold.receiver_insertions.as_slice())
    }

    /// Graph-inert B4-2C compiler checkpoint. Production prepare/Auto must not
    /// infer executability from this getter.
    pub(super) fn property_effect_scroll_receiver_insertions(
        &self,
    ) -> Option<&[PropertyEffectScrollReceiverInsertionContract]> {
        property_scene_plan_is_sealed(self).then_some(())?;
        self.property_scene_seal
            .as_ref()?
            .scroll_schedule_scaffold
            .as_ref()
            .map(|scaffold| scaffold.effect_receiver_insertions.as_slice())
    }

    pub(super) fn property_transform_effect_scroll_receiver_insertions(
        &self,
    ) -> Option<&[PropertyTransformEffectScrollReceiverInsertionContract]> {
        property_scene_plan_is_sealed(self).then_some(())?;
        self.property_scene_seal
            .as_ref()?
            .scroll_schedule_scaffold
            .as_ref()
            .map(|scaffold| scaffold.transform_effect_receiver_insertions.as_slice())
    }

    pub(super) fn property_scroll_planning_scaffold(
        &self,
    ) -> Option<&PropertyScrollScheduleScaffold> {
        property_scene_plan_is_sealed(self).then_some(())?;
        self.property_scene_seal
            .as_ref()?
            .scroll_schedule_scaffold
            .as_ref()
    }

    /// Exact nested-scroll planner seal. It is intentionally graph-inert;
    /// callers receive only frozen structural and artifact identities.
    pub(super) fn nested_scroll_planning_scaffold(&self) -> Option<&NestedScrollSceneScaffold> {
        property_scene_plan_is_sealed(self).then_some(())?;
        self.property_scene_seal
            .as_ref()?
            .nested_scroll_scaffold
            .as_ref()
    }

    #[cfg(test)]
    fn property_effect_scaffold_is_sealed_for_test(&self) -> bool {
        self.property_scene_seal
            .as_ref()
            .is_some_and(|seal| seal.effect_scaffold.is_some())
            && property_scene_plan_is_sealed(self)
    }
}

impl RetainedSurfacePlan {
    pub(super) fn boundary_root(&self) -> NodeKey {
        self.boundary_root
    }

    pub(super) fn stable_id(&self) -> u64 {
        self.stable_id
    }

    pub(super) fn persistent_color_key(&self) -> crate::view::frame_graph::PersistentTextureKey {
        self.persistent_color_key
    }

    pub(super) fn kind(&self) -> &SurfaceKind {
        &self.kind
    }

    #[cfg(test)]
    pub(super) fn transform(&self) -> TransformNodeId {
        match &self.kind {
            SurfaceKind::Transform(plan) => plan.transform,
            SurfaceKind::Isolation(_) => {
                panic!("isolation surfaces do not carry a transform node")
            }
            SurfaceKind::NestedIsolation(_) => {
                panic!("nested isolation surfaces do not carry a transform node")
            }
            SurfaceKind::ScrollHost(_) => panic!("scroll surfaces do not carry a transform node"),
        }
    }

    #[cfg(test)]
    pub(super) fn geometry(&self) -> TransformSurfaceGeometrySnapshot {
        match &self.kind {
            SurfaceKind::Transform(plan) => plan.geometry,
            SurfaceKind::Isolation(_) => {
                panic!("isolation surfaces do not carry transform geometry")
            }
            SurfaceKind::NestedIsolation(_) => {
                panic!("nested isolation surfaces do not carry transform geometry")
            }
            SurfaceKind::ScrollHost(_) => panic!("scroll surfaces do not carry transform geometry"),
        }
    }

    #[cfg(test)]
    pub(super) fn context(&self) -> TransformSurfacePlanContext {
        match &self.kind {
            SurfaceKind::Transform(plan) => plan.context,
            SurfaceKind::Isolation(_) => {
                panic!("isolation surfaces do not carry transform context")
            }
            SurfaceKind::NestedIsolation(_) => {
                panic!("nested isolation surfaces do not carry transform context")
            }
            SurfaceKind::ScrollHost(_) => panic!("scroll surfaces do not carry transform context"),
        }
    }

    pub(super) fn source_bounds(&self) -> crate::view::base_component::PromotionCompositeBounds {
        match &self.kind {
            SurfaceKind::Transform(plan) => plan.geometry.source_bounds,
            SurfaceKind::Isolation(plan) => plan.geometry.source_bounds,
            SurfaceKind::NestedIsolation(plan) => plan.geometry.source_bounds,
            SurfaceKind::ScrollHost(plan) => plan.admission.source_bounds,
        }
    }

    pub(super) fn outer_scissor_rect(&self) -> Option<[u32; 4]> {
        match &self.kind {
            SurfaceKind::Transform(plan) => plan.geometry.outer_scissor_rect,
            SurfaceKind::Isolation(plan) => plan.geometry.outer_scissor_rect,
            SurfaceKind::NestedIsolation(_) => None,
            SurfaceKind::ScrollHost(_) => None,
        }
    }

    pub(super) fn raster_steps(&self) -> &[PaintPlanStep] {
        &self.raster_steps
    }

    pub(super) fn parent_surface(&self) -> Option<NodeKey> {
        self.parent_surface
    }

    pub(super) fn aggregate_opaque_order_span(&self) -> &Range<u32> {
        &self.aggregate_opaque_order_span
    }

    pub(super) fn matches_frozen_witness(&self) -> bool {
        match &self.kind {
            SurfaceKind::Transform(plan) => {
                plan.context == plan.planned_context_witness
                    && plan.geometry.bitwise_eq(plan.planned_geometry_witness)
            }
            SurfaceKind::Isolation(plan) => plan.geometry.bitwise_eq(plan.planned_geometry_witness),
            SurfaceKind::NestedIsolation(plan) => {
                plan.geometry.bitwise_eq(plan.planned_geometry_witness)
            }
            SurfaceKind::ScrollHost(plan) => {
                plan.scroll == plan.planned_scroll_witness
                    && plan.contents_clip == plan.planned_clip_witness
                    && plan.admission.bitwise_eq(plan.planned_admission_witness)
            }
        }
    }

    #[cfg(test)]
    fn transform_plan_for_test(&self) -> &TransformSurfacePlan {
        match &self.kind {
            SurfaceKind::Transform(plan) => plan,
            SurfaceKind::Isolation(_) => panic!("expected transform surface"),
            SurfaceKind::NestedIsolation(_) => panic!("expected transform surface"),
            SurfaceKind::ScrollHost(_) => panic!("expected transform surface"),
        }
    }

    #[cfg(test)]
    fn transform_plan_mut_for_test(&mut self) -> &mut TransformSurfacePlan {
        match &mut self.kind {
            SurfaceKind::Transform(plan) => plan,
            SurfaceKind::Isolation(_) => panic!("expected transform surface"),
            SurfaceKind::NestedIsolation(_) => panic!("expected transform surface"),
            SurfaceKind::ScrollHost(_) => panic!("expected transform surface"),
        }
    }
}

impl NestedIsolationSurfaceGeometrySnapshot {
    fn from_exact_retained_output(
        source_bounds: crate::view::base_component::PromotionCompositeBounds,
    ) -> Option<Self> {
        if source_bounds.x < 0.0
            || source_bounds.y < 0.0
            || [
                source_bounds.x,
                source_bounds.y,
                source_bounds.width,
                source_bounds.height,
            ]
            .iter()
            .any(|value| !value.is_finite())
            || source_bounds.width <= 0.0
            || source_bounds.height <= 0.0
            || source_bounds.corner_radii.map(f32::to_bits) != [0.0_f32.to_bits(); 4]
        {
            return None;
        }
        Some(Self { source_bounds })
    }

    pub(super) fn logical_size(self) -> [f32; 2] {
        [self.source_bounds.width, self.source_bounds.height]
    }

    pub(super) fn bitwise_eq(self, other: Self) -> bool {
        [
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
}

impl IsolationSurfaceGeometrySnapshot {
    fn new(viewport_width: u32, viewport_height: u32, scale_factor: f32) -> Option<Self> {
        if viewport_width == 0
            || viewport_height == 0
            || !scale_factor.is_finite()
            || scale_factor <= 0.0
        {
            return None;
        }
        let logical_size = [
            viewport_width as f32 / scale_factor,
            viewport_height as f32 / scale_factor,
        ];
        if logical_size
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
        {
            return None;
        }
        Some(Self {
            source_bounds: crate::view::base_component::PromotionCompositeBounds {
                x: 0.0,
                y: 0.0,
                width: logical_size[0],
                height: logical_size[1],
                corner_radii: [0.0; 4],
            },
            logical_size,
            outer_scissor_rect: None,
        })
    }

    pub(super) fn bitwise_eq(self, other: Self) -> bool {
        [
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
            && self.logical_size.map(f32::to_bits) == other.logical_size.map(f32::to_bits)
            && self.outer_scissor_rect == other.outer_scissor_rect
    }
}

impl ArtifactSpanPlan {
    pub(super) fn artifact(&self) -> &PaintArtifact {
        &self.artifact
    }

    pub(super) fn opaque_order_span(&self) -> &Range<u32> {
        &self.opaque_order_span
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TransformSurfacePlanContext {
    paint_offset_bits: [u32; 2],
    outer_scissor_rect: Option<[u32; 4]>,
}

impl TransformSurfacePlanContext {
    pub(crate) fn new(paint_offset: [f32; 2], outer_scissor_rect: Option<[u32; 4]>) -> Self {
        Self {
            paint_offset_bits: paint_offset.map(f32::to_bits),
            outer_scissor_rect,
        }
    }

    fn paint_offset(self) -> [f32; 2] {
        self.paint_offset_bits.map(f32::from_bits)
    }

    pub(super) fn matches_ui_context(
        self,
        ctx: &crate::view::base_component::UiBuildContext,
    ) -> bool {
        self.paint_offset_bits == ctx.paint_offset().map(f32::to_bits)
            && self.outer_scissor_rect == ctx.graphics_pass_context().scissor_rect
    }

    pub(super) fn outer_scissor_rect(self) -> Option<[u32; 4]> {
        self.outer_scissor_rect
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FramePaintPlanRejection {
    EmptyScene,
    DuplicateRoot(NodeKey),
    RootCount(usize),
    MissingRoot(NodeKey),
    UnknownRootHost(NodeKey),
    RootHasParent(NodeKey),
    TopologyMismatch(NodeKey),
    DuplicateNodeKey(NodeKey),
    InvalidStableId(NodeKey),
    DuplicateStableId(u64),
    PromotionPresent(u64),
    DeferredBoundary(NodeKey),
    LayoutTransition(NodeKey),
    PropertyTree(PropertyTreeValidationError),
    TransformNodeCount(usize),
    MissingRootTransform(NodeKey),
    InvalidRootTransform(NodeKey),
    NonAffineTransform(NodeKey),
    UnexpectedTransform(NodeKey),
    MissingPropertyState(NodeKey),
    UnexpectedPropertyState(NodeKey),
    WrongTransformBoundary(NodeKey),
    ClipBoundary(NodeKey),
    EffectBoundary(NodeKey),
    ScrollBoundary(NodeKey),
    InvalidSurfaceGeometry(NodeKey),
    /// The legacy transform target clamps a negative pixel origin without
    /// preserving the full raster span. C2 must reject this known crop case
    /// until target declaration and UV mapping share one corrected contract.
    NegativeSurfaceOrigin(NodeKey),
    Coverage(FrameArtifactFallbackReason),
    InvalidSurfaceArtifact(NodeKey),
    IsolationOuterScissor,
    InvalidIsolationEffect(NodeKey),
    InvalidScrollHost(NodeKey),
    InvalidPropertyScene,
    InvalidClipChain(NodeKey),
    CoLocatedTransformEffect(NodeKey),
    UnsupportedPropertyInterleave(NodeKey),
    InvalidEffectChain(NodeKey),
    InvalidIsolationGeometry(NodeKey),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FramePaintPlanError {
    pub(crate) reasons: Vec<FramePaintPlanRejection>,
}

struct PropertyScenePlanningIndex {
    ids_by_transform: FxHashMap<TransformNodeId, PropertySurfaceId>,
    direct_children: FxHashMap<PropertySurfaceId, Vec<PropertySurfaceId>>,
    paint_offsets: FxHashMap<NodeKey, [f32; 2]>,
}

/// M12A1-0 planning-only retained property scene. This deliberately has no
/// executor or viewport dispatch entry: it seals ordered scene/surface
/// artifacts and topology without allocating a target or mutating a graph.
pub(crate) fn plan_transform_property_scene_with_context(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    context: TransformSurfacePlanContext,
) -> Result<FramePaintPlan, FramePaintPlanError> {
    let (reachable, index) = validate_transform_property_scene_inputs(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        context.paint_offset(),
    )?;
    let reachable_set = reachable.iter().copied().collect::<FxHashSet<_>>();
    let top_level_ids = index
        .ids_by_transform
        .iter()
        .filter_map(|(&transform, &id)| {
            property_trees
                .transforms
                .get(&transform)
                .is_some_and(|node| node.parent.is_none())
                .then_some(id)
        })
        .collect::<FxHashSet<_>>();
    let record_error = |fallbacks: Vec<FrameArtifactFallbackReason>| FramePaintPlanError {
        reasons: fallbacks
            .into_iter()
            .map(FramePaintPlanRejection::Coverage)
            .collect(),
    };

    let mut contracts = FxHashMap::default();
    let mut built = FxHashSet::default();
    let mut scene_validation = Vec::new();
    let mut scene_steps = Vec::new();
    let mut root_witnesses = Vec::with_capacity(roots.len());
    let mut scene_cursor = 0_u32;
    let mut seen_top_level = FxHashSet::default();
    for (root_ordinal, &root) in roots.iter().enumerate() {
        let root_ids = top_level_ids
            .iter()
            .copied()
            .filter(|id| node_is_within_root(arena, id.owner, root))
            .collect::<FxHashSet<_>>();
        let root_cutouts = planned_transform_cutouts(arena, root_ids.iter().copied())?;
        let recorded = super::frame_recorder::record_property_scene_steps_for_plan(
            arena,
            &[root],
            promoted_node_ids,
            property_trees,
            paint_generations,
            context.paint_offset(),
            &root_cutouts,
        )
        .map_err(&record_error)?;
        let step_start = scene_steps.len();
        for item in recorded {
            match item {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    if artifact.ops.is_empty() {
                        if super::compiler::validate_property_scene_artifact_for_plan(&artifact)
                            .is_none()
                        {
                            return Err(property_scene_error());
                        }
                        continue;
                    }
                    let Some(witness) =
                        super::compiler::validate_property_scene_artifact_for_plan(&artifact)
                    else {
                        return Err(property_scene_error());
                    };
                    let end = scene_cursor
                        .checked_add(opaque_order_count(&artifact))
                        .ok_or_else(property_scene_error)?;
                    scene_validation.push(witness);
                    scene_steps.push(PaintPlanStep::ArtifactSpan(ArtifactSpanPlan {
                        artifact,
                        opaque_order_span: scene_cursor..end,
                    }));
                    scene_cursor = end;
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(boundary) => {
                    let transform = match boundary.kind {
                        super::PlannedBoundaryKind::Transform(transform) => transform,
                        super::PlannedBoundaryKind::Isolation(_)
                        | super::PlannedBoundaryKind::Scroll(_) => {
                            return Err(property_scene_error());
                        }
                    };
                    let Some(&id) = index.ids_by_transform.get(&transform) else {
                        return Err(property_scene_error());
                    };
                    if !root_ids.contains(&id)
                        || boundary.root != id.owner
                        || !seen_top_level.insert(id)
                    {
                        return Err(property_scene_error());
                    }
                    let surface = plan_transform_property_surface(
                        arena,
                        id,
                        None,
                        root,
                        promoted_node_ids,
                        property_trees,
                        paint_generations,
                        &index,
                        context.outer_scissor_rect,
                        &mut contracts,
                        &mut built,
                    )?;
                    scene_cursor = scene_cursor.max(surface.aggregate_opaque_order_span.end);
                    scene_steps.push(PaintPlanStep::RetainedSurface(Box::new(surface)));
                }
            }
        }
        let stable_id = arena
            .get(root)
            .ok_or_else(property_scene_error)?
            .element
            .stable_id();
        root_witnesses.push(PropertySceneRootWitness {
            ordinal: u32::try_from(root_ordinal).map_err(|_| property_scene_error())?,
            root,
            stable_id,
            owner: PaintOwnerSnapshot {
                owner: root,
                parent: None,
            },
            top_level_step_span: step_start..scene_steps.len(),
        });
    }
    if seen_top_level != top_level_ids
        || built.len() != index.ids_by_transform.len()
        || contracts.len() != index.ids_by_transform.len()
        || property_trees
            .states
            .keys()
            .any(|key| !reachable_set.contains(key))
    {
        return Err(property_scene_error());
    }
    let plan = FramePaintPlan {
        steps: scene_steps,
        property_scene_roots: Some(root_witnesses.clone()),
        property_scene_seal: Some(PropertyScenePlanSeal {
            roots: root_witnesses,
            context,
            outer_scissor_rect: context.outer_scissor_rect,
            aggregate_opaque_order_span: 0..scene_cursor,
            surface_count: contracts.len(),
            scene_artifact_validation: scene_validation,
            surfaces: contracts,
            effect_scaffold: None,
            scroll_schedule_scaffold: None,
            nested_scroll_scaffold: None,
        }),
    };
    if !property_scene_plan_is_sealed(&plan) {
        return Err(property_scene_error());
    }
    Ok(plan)
}

/// M12A2-0 planning-only opacity/isolation scaffold. It freezes one ordered
/// transform/effect boundary forest, but intentionally produces no executable
/// raster steps and no property-scene transaction capability.
pub(crate) fn plan_property_effect_scene_scaffold_with_context(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    context: TransformSurfacePlanContext,
) -> Result<FramePaintPlan, FramePaintPlanError> {
    if roots.is_empty() {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::EmptyScene],
        });
    }
    if !paint_generations.matches_live_snapshot(arena, roots, property_trees) {
        return Err(property_scene_error());
    }
    let mut reasons = property_trees
        .validation_errors
        .iter()
        .copied()
        .map(FramePaintPlanRejection::PropertyTree)
        .collect::<Vec<_>>();
    for &stable_id in promoted_node_ids {
        push_unique(
            &mut reasons,
            FramePaintPlanRejection::PromotionPresent(stable_id),
        );
    }
    if !property_trees.scrolls.is_empty() {
        for scroll in property_trees.scrolls.values() {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::ScrollBoundary(scroll.owner),
            );
        }
    }

    #[derive(Clone, Copy)]
    struct Seed {
        boundary: PropertyBoundaryId,
        parent_boundary_ordinal: Option<u32>,
        scene_root_ordinal: u32,
        paint_offset: [f32; 2],
    }

    #[allow(clippy::too_many_arguments)]
    fn walk(
        arena: &NodeArena,
        key: NodeKey,
        scene_root_ordinal: u32,
        parent_context: super::PaintRecordingContext,
        parent_boundary_ordinal: Option<u32>,
        property_trees: &PropertyTrees,
        seen: &mut FxHashSet<NodeKey>,
        stable_owners: &mut FxHashMap<u64, NodeKey>,
        reachable: &mut Vec<NodeKey>,
        seeds: &mut Vec<Seed>,
        reasons: &mut Vec<FramePaintPlanRejection>,
    ) {
        if !seen.insert(key) {
            push_unique(reasons, FramePaintPlanRejection::DuplicateNodeKey(key));
            return;
        }
        let Some(node) = arena.get(key) else {
            push_unique(reasons, FramePaintPlanRejection::MissingRoot(key));
            return;
        };
        if node.children() != node.element.children() {
            push_unique(reasons, FramePaintPlanRejection::TopologyMismatch(key));
        }
        let stable_id = node.element.stable_id();
        if stable_id == 0 {
            push_unique(reasons, FramePaintPlanRejection::InvalidStableId(key));
        } else if stable_owners.insert(stable_id, key).is_some() {
            push_unique(
                reasons,
                FramePaintPlanRejection::DuplicateStableId(stable_id),
            );
        }
        if node.element.is_deferred_to_root_viewport_render() {
            push_unique(reasons, FramePaintPlanRejection::DeferredBoundary(key));
        }
        if node
            .element
            .placement_eligibility_metadata()
            .contains_runtime_layout_state
        {
            push_unique(reasons, FramePaintPlanRejection::LayoutTransition(key));
        }
        if !property_trees.states.contains_key(&key) {
            push_unique(reasons, FramePaintPlanRejection::MissingPropertyState(key));
        }
        let recording_context = node.element.shadow_paint_recording_context(parent_context);
        reachable.push(key);
        let transform = property_trees.transforms.get(&TransformNodeId(key));
        let effect = property_trees.effects.get(&EffectNodeId(key));
        if transform.is_some() && effect.is_some() {
            push_unique(
                reasons,
                FramePaintPlanRejection::CoLocatedTransformEffect(key),
            );
        }
        let boundary = match (transform, effect) {
            (Some(_), None) => Some(PropertyBoundaryId::Transform(TransformNodeId(key))),
            (None, Some(_)) => Some(PropertyBoundaryId::Effect(EffectNodeId(key))),
            _ => None,
        };
        let next_parent_boundary = if let Some(boundary) = boundary {
            let Ok(ordinal) = u32::try_from(seeds.len()) else {
                push_unique(reasons, FramePaintPlanRejection::InvalidPropertyScene);
                return;
            };
            seeds.push(Seed {
                boundary,
                parent_boundary_ordinal,
                scene_root_ordinal,
                paint_offset: recording_context.paint_offset,
            });
            Some(ordinal)
        } else {
            parent_boundary_ordinal
        };
        for &child in node.element.children() {
            if arena.parent_of(child) != Some(key) {
                push_unique(reasons, FramePaintPlanRejection::TopologyMismatch(child));
            }
            let child_context = node.element.shadow_paint_recording_context_for_child(
                child,
                arena,
                recording_context,
            );
            walk(
                arena,
                child,
                scene_root_ordinal,
                child_context,
                next_parent_boundary,
                property_trees,
                seen,
                stable_owners,
                reachable,
                seeds,
                reasons,
            );
        }
    }

    let mut seen = FxHashSet::default();
    let mut stable_owners = FxHashMap::default();
    let mut reachable = Vec::new();
    let mut seeds = Vec::new();
    let mut root_ranges = Vec::with_capacity(roots.len());
    let mut root_seen = FxHashSet::default();
    for (root_ordinal, &root) in roots.iter().enumerate() {
        if !root_seen.insert(root) {
            push_unique(&mut reasons, FramePaintPlanRejection::DuplicateRoot(root));
        }
        if arena.parent_of(root).is_some() {
            push_unique(&mut reasons, FramePaintPlanRejection::RootHasParent(root));
        }
        let start = seeds.len();
        walk(
            arena,
            root,
            u32::try_from(root_ordinal).unwrap_or(u32::MAX),
            super::PaintRecordingContext {
                paint_offset: context.paint_offset(),
                ..Default::default()
            },
            None,
            property_trees,
            &mut seen,
            &mut stable_owners,
            &mut reachable,
            &mut seeds,
            &mut reasons,
        );
        root_ranges.push(start..seeds.len());
    }
    let reachable_set = reachable.iter().copied().collect::<FxHashSet<_>>();
    for &key in property_trees.states.keys() {
        if !reachable_set.contains(&key) {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::UnexpectedPropertyState(key),
            );
        }
    }
    for transform in property_trees.transforms.values() {
        if !reachable_set.contains(&transform.owner) {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::UnexpectedTransform(transform.owner),
            );
        }
    }
    for effect in property_trees.effects.values() {
        if !reachable_set.contains(&effect.owner) {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::EffectBoundary(effect.owner),
            );
        }
    }
    for (&id, clip) in &property_trees.clips {
        let exact = id.owner == clip.owner
            && reachable_set.contains(&clip.owner)
            && clip.generation != 0
            && matches!(clip.geometry, ClipGeometry::LogicalScissor(_))
            && matches!(
                (id.role, clip.behavior),
                (ClipNodeRole::SelfClip, ClipBehavior::Replace)
                    | (ClipNodeRole::ContentsClip, ClipBehavior::Intersect)
            )
            && property_trees.clip_snapshot_for(Some(id)).is_some();
        if !exact {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::InvalidClipChain(clip.owner),
            );
        }
    }

    // Coordinate authority is deliberately bounded for this scaffold. Pure
    // effect forests use the existing retained-output basis. The only mixed
    // shape admitted is the already-proven root Transform -> direct Effect.
    if !property_trees.transforms.is_empty() && !property_trees.effects.is_empty() {
        let mixed_transform = seeds.first().and_then(|seed| match seed.boundary {
            PropertyBoundaryId::Transform(id) => property_trees.transform_snapshot_for(id),
            PropertyBoundaryId::Effect(_) => None,
        });
        let mixed_is_proven = property_trees.transforms.len() == 1
            && property_trees.effects.len() == 1
            && roots.len() == 1
            && seeds.len() == 2
            && matches!(seeds[0].boundary, PropertyBoundaryId::Transform(_))
            && matches!(seeds[1].boundary, PropertyBoundaryId::Effect(_))
            && seeds[0].boundary.owner() == roots[0]
            && seeds[0].parent_boundary_ordinal.is_none()
            && seeds[1].parent_boundary_ordinal == Some(0)
            && arena.parent_of(seeds[1].boundary.owner()) == Some(seeds[0].boundary.owner())
            && mixed_transform.is_some_and(|snapshot| {
                snapshot.parent.is_none() && matrix_is_finite_affine(snapshot.viewport_matrix)
            });
        if !mixed_is_proven {
            let owner = seeds
                .iter()
                .find(|seed| matches!(seed.boundary, PropertyBoundaryId::Effect(_)))
                .map_or(roots[0], |seed| seed.boundary.owner());
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::UnsupportedPropertyInterleave(owner),
            );
        }
    }
    if property_trees.effects.is_empty() {
        push_unique(
            &mut reasons,
            FramePaintPlanRejection::InvalidEffectChain(roots[0]),
        );
    }
    if !reasons.is_empty() {
        return Err(FramePaintPlanError { reasons });
    }

    let clip_forest = freeze_property_effect_clip_forest(arena, &reachable, property_trees)?;

    let boundary_owners = seeds
        .iter()
        .map(|seed| seed.boundary.owner())
        .collect::<FxHashSet<_>>();
    let mut surfaces = Vec::with_capacity(seeds.len());
    for (ordinal, seed) in seeds.iter().copied().enumerate() {
        let owner = seed.boundary.owner();
        let node = arena.get(owner).ok_or_else(property_scene_error)?;
        let stable_id = node.element.stable_id();
        let kind = match seed.boundary {
            PropertyBoundaryId::Transform(id) => {
                let snapshot = property_trees
                    .transform_snapshot_for(id)
                    .ok_or_else(property_scene_error)?;
                PropertyEffectSurfaceKind::Transform {
                    snapshot,
                    nested_effect_dependencies: Vec::new(),
                }
            }
            PropertyBoundaryId::Effect(id) => {
                let live_leaf_to_root =
                    property_trees
                        .effect_snapshot_for(Some(id))
                        .ok_or_else(|| FramePaintPlanError {
                            reasons: vec![FramePaintPlanRejection::InvalidEffectChain(owner)],
                        })?;
                let Some(leaf) = live_leaf_to_root.first().copied() else {
                    return Err(property_scene_error());
                };
                if leaf.id != id || leaf.owner != owner {
                    return Err(FramePaintPlanError {
                        reasons: vec![FramePaintPlanRejection::InvalidEffectChain(owner)],
                    });
                }
                let isolated_leaf = EffectNodeSnapshot {
                    parent: None,
                    ..leaf
                };
                let detached_ancestors = live_leaf_to_root[1..].to_vec();
                let element = node
                    .element
                    .as_any()
                    .downcast_ref::<Element>()
                    .ok_or_else(|| FramePaintPlanError {
                        reasons: vec![FramePaintPlanRejection::UnknownRootHost(owner)],
                    })?;
                let bounds = element
                    .exact_nested_isolation_render_output_bounds(arena, seed.paint_offset)
                    .ok_or_else(|| FramePaintPlanError {
                        reasons: vec![FramePaintPlanRejection::InvalidIsolationGeometry(owner)],
                    })?;
                let source_bounds_bits =
                    [bounds.x, bounds.y, bounds.width, bounds.height].map(f32::to_bits);
                let ancestor_composite_clips =
                    ancestor_clip_chain_for_surface(property_trees, owner)?;
                let resolved_scissor = resolve_composite_scissor(
                    context.outer_scissor_rect(),
                    &ancestor_composite_clips,
                )?;
                let full_clips = property_trees
                    .clip_snapshot_for(
                        property_trees
                            .node_state_for(owner)
                            .and_then(|state| state.paint.clip),
                    )
                    .ok_or_else(property_scene_error)?;
                let ancestor_ids = ancestor_composite_clips
                    .iter()
                    .map(|clip| clip.id)
                    .collect::<FxHashSet<_>>();
                let local_raster_clips = full_clips
                    .into_iter()
                    .take_while(|clip| !ancestor_ids.contains(&clip.id))
                    .collect::<Vec<_>>();
                let basis = match seed.parent_boundary_ordinal {
                    None => PropertyIsolationCompositeBasis::FrameRoot,
                    Some(parent_ordinal) => match seeds[parent_ordinal as usize].boundary {
                        PropertyBoundaryId::Effect(parent) => {
                            PropertyIsolationCompositeBasis::ParentEffect(parent)
                        }
                        PropertyBoundaryId::Transform(parent) => {
                            let parent = property_trees
                                .transform_snapshot_for(parent)
                                .ok_or_else(property_scene_error)?;
                            PropertyIsolationCompositeBasis::ParentTransform {
                                transform: parent.id,
                                viewport_matrix_bits: parent
                                    .viewport_matrix
                                    .to_cols_array()
                                    .map(f32::to_bits),
                            }
                        }
                    },
                };
                let mut content = Vec::new();
                let mut stack = vec![owner];
                let mut content_seen = FxHashSet::default();
                while let Some(key) = stack.pop() {
                    if key != owner && boundary_owners.contains(&key) {
                        continue;
                    }
                    if !content_seen.insert(key) {
                        return Err(property_scene_error());
                    }
                    let generations = paint_generations
                        .local_generations_for(key)
                        .ok_or_else(property_scene_error)?;
                    content.push(PropertyIsolationContentGenerationWitness {
                        owner: key,
                        stable_id: arena
                            .get(key)
                            .ok_or_else(property_scene_error)?
                            .element
                            .stable_id(),
                        parent: (key != owner).then(|| arena.parent_of(key)).flatten(),
                        self_paint_revision: generations.self_paint_revision,
                        topology_revision: generations.topology_revision,
                    });
                    let current = arena.get(key).ok_or_else(property_scene_error)?;
                    stack.extend(current.element.children().iter().rev().copied());
                }
                PropertyEffectSurfaceKind::Isolation(PropertyIsolationBoundaryContract {
                    effect_chain: AncestorEffectDetachmentWitness {
                        live_leaf_to_root,
                        isolated_leaf,
                        detached_ancestors,
                    },
                    raster_space: PropertyIsolationRasterSpaceSnapshot {
                        paint_offset_bits: seed.paint_offset.map(f32::to_bits),
                        source_bounds_bits,
                    },
                    composite: PropertyIsolationCompositeMappingSnapshot {
                        basis,
                        rect_bits: source_bounds_bits,
                        opacity_bits: leaf.opacity.to_bits(),
                        effect_generation: leaf.generation,
                        resolved_scissor,
                    },
                    local_raster_clips: local_raster_clips.clone(),
                    ancestor_composite_clips,
                    raster_identity: PropertyIsolationRasterIdentitySpec {
                        boundary: id,
                        stable_id,
                        raster_space: PropertyIsolationRasterSpaceSnapshot {
                            paint_offset_bits: seed.paint_offset.map(f32::to_bits),
                            source_bounds_bits,
                        },
                        local_raster_clips,
                        content,
                        nested_dependencies: Vec::new(),
                    },
                    nested_dependencies: Vec::new(),
                    parent_opaque_cursor_delta: 0,
                })
            }
        };
        surfaces.push(PropertyEffectSurfaceContract {
            ordinal: u32::try_from(ordinal).map_err(|_| property_scene_error())?,
            boundary: seed.boundary,
            stable_id,
            parent_boundary_ordinal: seed.parent_boundary_ordinal,
            scene_root_ordinal: seed.scene_root_ordinal,
            kind,
        });
    }
    for child_index in (0..surfaces.len()).rev() {
        let Some(parent) = surfaces[child_index].parent_boundary_ordinal else {
            continue;
        };
        let PropertyEffectSurfaceKind::Isolation(isolation) = &surfaces[child_index].kind else {
            continue;
        };
        let dependency = PropertyIsolationNestedDependencySpec {
            child_boundary_ordinal: surfaces[child_index].ordinal,
            child_effect: isolation.effect_chain.isolated_leaf.id,
            child_stable_id: surfaces[child_index].stable_id,
            child_opacity_bits: isolation.composite.opacity_bits,
            child_effect_generation: isolation.composite.effect_generation,
            child_rect_bits: isolation.composite.rect_bits,
            child_raster_identity: Box::new(isolation.raster_identity.clone()),
        };
        match &mut surfaces[parent as usize].kind {
            PropertyEffectSurfaceKind::Transform {
                nested_effect_dependencies,
                ..
            } => nested_effect_dependencies.insert(0, dependency.clone()),
            PropertyEffectSurfaceKind::Isolation(parent) => {
                parent.nested_dependencies.insert(0, dependency.clone());
                parent
                    .raster_identity
                    .nested_dependencies
                    .insert(0, dependency);
            }
        }
    }

    let mut effect_roots = Vec::with_capacity(roots.len());
    let mut plan_roots = Vec::with_capacity(roots.len());
    for (ordinal, (&root, range)) in roots.iter().zip(root_ranges).enumerate() {
        let stable_id = arena
            .get(root)
            .ok_or_else(property_scene_error)?
            .element
            .stable_id();
        effect_roots.push(PropertyEffectRootWitness {
            ordinal: u32::try_from(ordinal).map_err(|_| property_scene_error())?,
            root,
            stable_id,
            boundary_ordinal_span: u32::try_from(range.start).map_err(|_| property_scene_error())?
                ..u32::try_from(range.end).map_err(|_| property_scene_error())?,
        });
        plan_roots.push(PropertySceneRootWitness {
            ordinal: u32::try_from(ordinal).map_err(|_| property_scene_error())?,
            root,
            stable_id,
            owner: PaintOwnerSnapshot {
                owner: root,
                parent: None,
            },
            top_level_step_span: 0..0,
        });
    }
    let scaffold = PropertyEffectSceneScaffold {
        context,
        outer_scissor_rect: context.outer_scissor_rect(),
        planned_context: context,
        planned_outer_scissor_rect: context.outer_scissor_rect(),
        planned_roots: effect_roots.clone(),
        planned_surfaces: surfaces.clone(),
        planned_clip_forest: clip_forest.clone(),
        planned_production_root_step_spans: None,
        roots: effect_roots,
        surfaces,
        clip_forest,
        production_root_step_spans: None,
    };
    let plan = FramePaintPlan {
        steps: Vec::new(),
        property_scene_roots: Some(plan_roots.clone()),
        property_scene_seal: Some(PropertyScenePlanSeal {
            roots: plan_roots,
            context,
            outer_scissor_rect: context.outer_scissor_rect(),
            aggregate_opaque_order_span: 0..0,
            surface_count: 0,
            scene_artifact_validation: Vec::new(),
            surfaces: FxHashMap::default(),
            effect_scaffold: Some(scaffold),
            scroll_schedule_scaffold: None,
            nested_scroll_scaffold: None,
        }),
    };
    property_scene_plan_is_sealed(&plan)
        .then_some(plan)
        .ok_or_else(property_scene_error)
}

/// Planning-only exact `S0 -> S1 -> leaf` scene. The bounded admission and
/// three recorder scopes are all revalidated here, but the result deliberately
/// contains no executable transaction or frame-graph/pool authority.
pub(crate) fn plan_nested_scroll_scene_scaffold_with_context(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
    context: TransformSurfacePlanContext,
) -> Result<FramePaintPlan, FramePaintPlanError> {
    if roots.len() != 1 {
        return Err(FramePaintPlanError {
            reasons: if roots.is_empty() {
                vec![FramePaintPlanRejection::EmptyScene]
            } else {
                vec![FramePaintPlanRejection::RootCount(roots.len())]
            },
        });
    }
    let root = roots[0];
    if scale_factor.to_bits() != 1.0_f32.to_bits()
        || context.paint_offset_bits != [0.0_f32.to_bits(); 2]
        || context.outer_scissor_rect().is_some()
        || !promoted_node_ids.is_empty()
        || !paint_generations.matches_live_snapshot(arena, roots, property_trees)
        || !property_trees.validation_errors.is_empty()
        || arena.parent_of(root).is_some()
    {
        return Err(property_scene_error());
    }
    let root_node = arena.get(root).ok_or_else(property_scene_error)?;
    let root_element = root_node
        .element
        .as_any()
        .downcast_ref::<Element>()
        .ok_or_else(property_scene_error)?;
    let admission = root_element
        .exact_retained_nested_scroll_scene_admission(root, arena, scale_factor)
        .ok_or_else(property_scene_error)?;
    let inner = admission.inner_boundary_root;
    let leaf = admission.content_leaf;
    let exact_keys = FxHashSet::from_iter([root, inner, leaf]);
    if root_node.children() != [inner]
        || root_node.element.children() != [inner]
        || arena.parent_of(inner) != Some(root)
        || arena.parent_of(leaf) != Some(inner)
        || arena
            .get(inner)
            .is_none_or(|node| node.children() != [leaf] || node.element.children() != [leaf])
        || arena
            .get(leaf)
            .is_none_or(|node| !node.children().is_empty() || !node.element.children().is_empty())
        || property_trees.states.len() != 3
        || property_trees
            .states
            .keys()
            .any(|key| !exact_keys.contains(key))
        || property_trees.scrolls.len() != 2
        || property_trees
            .scrolls
            .values()
            .any(|node| !exact_keys.contains(&node.owner) || node.owner == leaf)
        || property_trees.clips.len() != 2
        || property_trees
            .clips
            .values()
            .any(|node| !exact_keys.contains(&node.owner) || node.owner == leaf)
        || !property_trees.transforms.is_empty()
        || !property_trees.effects.is_empty()
    {
        return Err(property_scene_error());
    }
    for key in [root, inner, leaf] {
        let node = arena.get(key).ok_or_else(property_scene_error)?;
        if node.element.stable_id() == 0
            || node.element.is_deferred_to_root_viewport_render()
            || node
                .element
                .placement_eligibility_metadata()
                .contains_runtime_layout_state
        {
            return Err(property_scene_error());
        }
    }
    if admission.outer_stable_id == admission.inner_stable_id
        || admission.outer_stable_id == admission.content_leaf_stable_id
        || admission.inner_stable_id == admission.content_leaf_stable_id
    {
        return Err(property_scene_error());
    }

    let outer_scroll = property_trees
        .scroll_snapshot_for(ScrollNodeId(root))
        .ok_or_else(property_scene_error)?;
    let inner_scroll = property_trees
        .scroll_snapshot_for(ScrollNodeId(inner))
        .ok_or_else(property_scene_error)?;
    let outer_clip_id = ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    };
    let inner_clip_id = ClipNodeId {
        owner: inner,
        role: ClipNodeRole::ContentsClip,
    };
    let outer_clip = property_trees
        .clip_snapshot_for(Some(outer_clip_id))
        .and_then(|chain| chain.first().copied())
        .ok_or_else(property_scene_error)?;
    let inner_clip = property_trees
        .clip_snapshot_for(Some(inner_clip_id))
        .and_then(|chain| chain.first().copied())
        .ok_or_else(property_scene_error)?;
    if !admission.matches_scroll_nodes(outer_scroll, inner_scroll)
        || !outer_scroll.has_canonical_vertical_geometry_with_contents_clip(outer_clip)
        || !inner_scroll.has_canonical_nested_vertical_geometry_with_contents_clip(
            inner_clip,
            outer_scroll,
            outer_clip,
        )
    {
        return Err(property_scene_error());
    }
    let outer_content_state = PropertyTreeState {
        clip: Some(outer_clip.id),
        scroll: Some(outer_scroll.id),
        ..PropertyTreeState::default()
    };
    let inner_content_state = PropertyTreeState {
        clip: Some(inner_clip.id),
        scroll: Some(inner_scroll.id),
        ..PropertyTreeState::default()
    };
    let outer_state = property_trees
        .node_state_for(root)
        .ok_or_else(property_scene_error)?;
    let inner_state = property_trees
        .node_state_for(inner)
        .ok_or_else(property_scene_error)?;
    let leaf_state = property_trees
        .node_state_for(leaf)
        .ok_or_else(property_scene_error)?;
    if outer_state.paint != PropertyTreeState::default()
        || outer_state.descendants != outer_content_state
        || inner_state.paint != outer_content_state
        || inner_state.descendants != inner_content_state
        || leaf_state.paint != inner_content_state
        || leaf_state.descendants != inner_content_state
    {
        return Err(property_scene_error());
    }

    let outer_host =
        super::PaintBakedScrollHostWitness::new(root, inner, outer_scroll, outer_clip.id)
            .ok_or_else(property_scene_error)?;
    let inner_cutout = super::PlannedBoundary {
        root: inner,
        stable_id: admission.inner_stable_id,
        kind: super::PlannedBoundaryKind::Scroll(inner_scroll.id),
    };
    let outer_recorded = super::frame_recorder::record_nested_scroll_outer_host_steps_for_plan(
        arena,
        root,
        promoted_node_ids,
        property_trees,
        paint_generations,
        outer_host,
        inner_cutout,
    )
    .map_err(|reasons| FramePaintPlanError {
        reasons: reasons
            .into_iter()
            .map(FramePaintPlanRejection::Coverage)
            .collect(),
    })?;
    let [
        super::frame_recorder::RecordedTransformSurfaceStep::Artifact(outer_before),
        super::frame_recorder::RecordedTransformSurfaceStep::Boundary(recorded_cutout),
        super::frame_recorder::RecordedTransformSurfaceStep::Artifact(outer_after),
    ] = outer_recorded.as_slice()
    else {
        return Err(property_scene_error());
    };
    if *recorded_cutout != inner_cutout {
        return Err(property_scene_error());
    }

    let outer_content =
        super::PaintScrollContentWitness::new(root, inner, outer_scroll, outer_clip)
            .ok_or_else(property_scene_error)?;
    let inner_host =
        super::PaintBakedScrollHostWitness::new(inner, leaf, inner_scroll, inner_clip.id)
            .ok_or_else(property_scene_error)?;
    let witness = super::PaintNestedScrollContentWitness::new(
        root,
        inner,
        leaf,
        outer_scroll,
        outer_clip,
        inner_scroll,
        inner_clip,
    )
    .ok_or_else(property_scene_error)?;
    let inner_recorded = super::frame_recorder::record_nested_scroll_inner_host_steps_for_plan(
        arena,
        inner,
        promoted_node_ids,
        property_trees,
        paint_generations,
        inner_host,
        outer_content,
        admission.content_leaf_stable_id,
        witness,
    )
    .map_err(|reasons| FramePaintPlanError {
        reasons: reasons
            .into_iter()
            .map(FramePaintPlanRejection::Coverage)
            .collect(),
    })?;
    let [
        super::frame_recorder::RecordedNestedScrollHostStep::Artifact(inner_before),
        super::frame_recorder::RecordedNestedScrollHostStep::ContentReceiver(receiver),
        super::frame_recorder::RecordedNestedScrollHostStep::Artifact(inner_after),
    ] = inner_recorded.as_slice()
    else {
        return Err(property_scene_error());
    };
    if receiver.stable_id != admission.content_leaf_stable_id || receiver.witness != witness {
        return Err(property_scene_error());
    }
    let content = super::frame_recorder::record_nested_scroll_content_artifact_for_plan(
        arena,
        promoted_node_ids,
        property_trees,
        paint_generations,
        witness,
    )
    .map_err(|reasons| FramePaintPlanError {
        reasons: reasons
            .into_iter()
            .map(FramePaintPlanRejection::Coverage)
            .collect(),
    })?;
    let artifact_seal = |artifact: &PaintArtifact| {
        Ok::<_, FramePaintPlanError>(NestedScrollArtifactSeal {
            recorded_artifact: artifact.clone(),
            identity: property_scroll_receiver_artifact_identity(artifact)
                .ok_or_else(property_scene_error)?,
        })
    };
    let boundaries = vec![
        NestedScrollBoundaryContract {
            slot: NestedScrollBoundarySlot::Outer,
            boundary_root: root,
            stable_id: admission.outer_stable_id,
            parent: None,
            scroll: outer_scroll,
            contents_clip: outer_clip,
            content_state: outer_content_state,
            projected_receiver_state: PropertyTreeState::default(),
        },
        NestedScrollBoundaryContract {
            slot: NestedScrollBoundarySlot::Inner,
            boundary_root: inner,
            stable_id: admission.inner_stable_id,
            parent: Some(NestedScrollBoundarySlot::Outer),
            scroll: inner_scroll,
            contents_clip: inner_clip,
            content_state: inner_content_state,
            projected_receiver_state: outer_content_state,
        },
    ];
    let schedule = NestedScrollSceneSchedule {
        steps: vec![
            NestedScrollSceneScheduledStep::HostBefore {
                boundary: NestedScrollBoundarySlot::Outer,
                artifact: artifact_seal(outer_before)?,
            },
            NestedScrollSceneScheduledStep::HostBefore {
                boundary: NestedScrollBoundarySlot::Inner,
                artifact: artifact_seal(inner_before)?,
            },
            NestedScrollSceneScheduledStep::ContentReceiver(NestedScrollContentReceiverIdentity {
                stable_id: receiver.stable_id,
                witness,
                live_input: inner_content_state,
                projected_output: outer_content_state,
                artifact: artifact_seal(&content)?,
            }),
            NestedScrollSceneScheduledStep::OverlayAfter {
                boundary: NestedScrollBoundarySlot::Inner,
                artifact: artifact_seal(inner_after)?,
            },
            NestedScrollSceneScheduledStep::OverlayAfter {
                boundary: NestedScrollBoundarySlot::Outer,
                artifact: artifact_seal(outer_after)?,
            },
        ],
    };
    let root_witness = PropertySceneRootWitness {
        ordinal: 0,
        root,
        stable_id: admission.outer_stable_id,
        owner: PaintOwnerSnapshot {
            owner: root,
            parent: None,
        },
        top_level_step_span: 0..0,
    };
    let scaffold = NestedScrollSceneScaffold {
        context,
        admission,
        boundaries: boundaries.clone(),
        schedule: schedule.clone(),
        planned_context: context,
        planned_admission: admission,
        planned_boundaries: boundaries,
        planned_schedule: schedule,
    };
    let plan = FramePaintPlan {
        steps: Vec::new(),
        property_scene_roots: Some(vec![root_witness.clone()]),
        property_scene_seal: Some(PropertyScenePlanSeal {
            roots: vec![root_witness],
            context,
            outer_scissor_rect: None,
            aggregate_opaque_order_span: 0..0,
            surface_count: 0,
            scene_artifact_validation: Vec::new(),
            surfaces: FxHashMap::default(),
            effect_scaffold: None,
            scroll_schedule_scaffold: None,
            nested_scroll_scaffold: Some(scaffold),
        }),
    };
    property_scene_plan_is_sealed(&plan)
        .then_some(plan)
        .ok_or_else(property_scene_error)
}

/// M12B4-0 planning-only schedule for the first property/scroll interleave
/// grammar. This freezes ordering, composite basis and property consumption,
/// but intentionally cannot mint a production transaction or context token.
pub(crate) fn plan_property_scroll_interleave_scaffold_with_context(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    context: TransformSurfacePlanContext,
) -> Result<FramePaintPlan, FramePaintPlanError> {
    if roots.is_empty() {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::EmptyScene],
        });
    }
    if !paint_generations.matches_live_snapshot(arena, roots, property_trees) {
        return Err(property_scene_error());
    }
    let mut reasons = property_trees
        .validation_errors
        .iter()
        .copied()
        .map(FramePaintPlanRejection::PropertyTree)
        .collect::<Vec<_>>();
    for &stable_id in promoted_node_ids {
        push_unique(
            &mut reasons,
            FramePaintPlanRejection::PromotionPresent(stable_id),
        );
    }

    #[derive(Clone, Copy)]
    enum PathBoundary {
        Transform(TransformNodeId),
        Effect(EffectNodeId),
        Scroll(ScrollNodeId),
    }

    #[allow(clippy::too_many_arguments)]
    fn walk(
        arena: &NodeArena,
        key: NodeKey,
        scene_root_ordinal: u32,
        property_trees: &PropertyTrees,
        path: &mut Vec<PathBoundary>,
        seen: &mut FxHashSet<NodeKey>,
        stable_owners: &mut FxHashMap<u64, NodeKey>,
        reachable: &mut FxHashSet<NodeKey>,
        schedule: &mut Vec<PropertySceneScheduledStep>,
        boundaries: &mut Vec<PropertyScrollBoundaryContract>,
        reasons: &mut Vec<FramePaintPlanRejection>,
    ) {
        if !seen.insert(key) {
            push_unique(reasons, FramePaintPlanRejection::DuplicateNodeKey(key));
            return;
        }
        let Some(node) = arena.get(key) else {
            push_unique(reasons, FramePaintPlanRejection::MissingRoot(key));
            return;
        };
        reachable.insert(key);
        if node.children() != node.element.children() {
            push_unique(reasons, FramePaintPlanRejection::TopologyMismatch(key));
        }
        let stable_id = node.element.stable_id();
        if stable_id == 0 {
            push_unique(reasons, FramePaintPlanRejection::InvalidStableId(key));
        } else if stable_owners.insert(stable_id, key).is_some() {
            push_unique(
                reasons,
                FramePaintPlanRejection::DuplicateStableId(stable_id),
            );
        }
        if node.element.is_deferred_to_root_viewport_render() {
            push_unique(reasons, FramePaintPlanRejection::DeferredBoundary(key));
        }
        if node
            .element
            .placement_eligibility_metadata()
            .contains_runtime_layout_state
        {
            push_unique(reasons, FramePaintPlanRejection::LayoutTransition(key));
        }
        let Some(state) = property_trees.node_state_for(key) else {
            push_unique(reasons, FramePaintPlanRejection::MissingPropertyState(key));
            return;
        };
        let transform = property_trees.transforms.get(&TransformNodeId(key));
        let effect = property_trees.effects.get(&EffectNodeId(key));
        let scroll = property_trees.scrolls.get(&ScrollNodeId(key));
        if usize::from(transform.is_some())
            + usize::from(effect.is_some())
            + usize::from(scroll.is_some())
            > 1
        {
            push_unique(
                reasons,
                if transform.is_some() && effect.is_some() {
                    FramePaintPlanRejection::CoLocatedTransformEffect(key)
                } else {
                    FramePaintPlanRejection::UnsupportedPropertyInterleave(key)
                },
            );
        }

        let path_has_scroll = path
            .iter()
            .any(|entry| matches!(entry, PathBoundary::Scroll(_)));
        let pushed = match (transform, effect, scroll) {
            (Some(_), None, None) => {
                if path_has_scroll {
                    push_unique(
                        reasons,
                        FramePaintPlanRejection::UnsupportedPropertyInterleave(key),
                    );
                }
                let Some(snapshot) = property_trees.transform_snapshot_for(TransformNodeId(key))
                else {
                    push_unique(reasons, FramePaintPlanRejection::InvalidRootTransform(key));
                    return;
                };
                if snapshot.owner != key
                    || snapshot.generation == 0
                    || !matrix_is_finite_affine(snapshot.viewport_matrix)
                {
                    push_unique(reasons, FramePaintPlanRejection::InvalidRootTransform(key));
                }
                let parent = path.iter().rev().find_map(|entry| match entry {
                    PathBoundary::Transform(id) => {
                        Some(PropertyScheduledSurfaceBoundaryId::Transform(*id))
                    }
                    PathBoundary::Effect(id) => {
                        Some(PropertyScheduledSurfaceBoundaryId::Effect(*id))
                    }
                    PathBoundary::Scroll(_) => None,
                });
                schedule.push(PropertySceneScheduledStep::RetainedSurface {
                    boundary: PropertyScheduledSurfaceBoundary::Transform(snapshot),
                    parent,
                });
                path.push(PathBoundary::Transform(snapshot.id));
                true
            }
            (None, Some(_), None) => {
                if path_has_scroll {
                    push_unique(
                        reasons,
                        FramePaintPlanRejection::UnsupportedPropertyInterleave(key),
                    );
                }
                let snapshots = property_trees.effect_snapshot_for(Some(EffectNodeId(key)));
                let Some(snapshot) = snapshots.and_then(|nodes| nodes.first().copied()) else {
                    push_unique(reasons, FramePaintPlanRejection::InvalidEffectChain(key));
                    return;
                };
                if snapshot.owner != key
                    || snapshot.generation == 0
                    || !snapshot.opacity.is_finite()
                    || !(0.0..=1.0).contains(&snapshot.opacity)
                {
                    push_unique(reasons, FramePaintPlanRejection::InvalidEffectChain(key));
                }
                let parent = path.iter().rev().find_map(|entry| match entry {
                    PathBoundary::Transform(id) => {
                        Some(PropertyScheduledSurfaceBoundaryId::Transform(*id))
                    }
                    PathBoundary::Effect(id) => {
                        Some(PropertyScheduledSurfaceBoundaryId::Effect(*id))
                    }
                    PathBoundary::Scroll(_) => None,
                });
                schedule.push(PropertySceneScheduledStep::RetainedSurface {
                    boundary: PropertyScheduledSurfaceBoundary::Effect(snapshot),
                    parent,
                });
                path.push(PathBoundary::Effect(snapshot.id));
                true
            }
            (None, None, Some(_)) => {
                if path_has_scroll {
                    push_unique(reasons, FramePaintPlanRejection::ScrollBoundary(key));
                }
                let Some(scroll) = property_trees.scroll_snapshot_for(ScrollNodeId(key)) else {
                    push_unique(reasons, FramePaintPlanRejection::InvalidScrollHost(key));
                    return;
                };
                let clip_id = ClipNodeId {
                    owner: key,
                    role: ClipNodeRole::ContentsClip,
                };
                let Some(contents_clip) = property_trees
                    .clip_snapshot_for(Some(clip_id))
                    .and_then(|clips| (clips.len() == 1).then_some(clips[0]))
                else {
                    push_unique(reasons, FramePaintPlanRejection::InvalidScrollHost(key));
                    return;
                };
                if scroll.parent.is_some()
                    || (!scroll.is_canonical_with_contents_clip(contents_clip)
                        && !scroll.is_canonical_painted_with_contents_clip(contents_clip))
                    || state.descendants.scroll != Some(scroll.id)
                    || state.descendants.clip != Some(contents_clip.id)
                {
                    push_unique(reasons, FramePaintPlanRejection::InvalidScrollHost(key));
                }
                let basis = match path.last().copied() {
                    None => ScrollCompositeBasis::FrameRoot,
                    Some(PathBoundary::Transform(id)) => property_trees
                        .transform_snapshot_for(id)
                        .map(ScrollCompositeBasis::Transform)
                        .unwrap_or(ScrollCompositeBasis::FrameRoot),
                    Some(PathBoundary::Effect(id)) => property_trees
                        .effect_snapshot_for(Some(id))
                        .and_then(|nodes| nodes.first().copied())
                        .map(ScrollCompositeBasis::Effect)
                        .unwrap_or(ScrollCompositeBasis::FrameRoot),
                    Some(PathBoundary::Scroll(_)) => ScrollCompositeBasis::FrameRoot,
                };

                let live_input = state.descendants;
                let mut cursor = live_input;
                let mut entries = Vec::new();
                for ancestor in path.iter().copied() {
                    let before = cursor;
                    let boundary = match ancestor {
                        PathBoundary::Transform(id) if cursor.transform == Some(id) => {
                            cursor.transform = None;
                            ConsumedPropertyBoundary::Transform(id)
                        }
                        PathBoundary::Effect(id) if cursor.effect == Some(id) => {
                            cursor.effect = None;
                            ConsumedPropertyBoundary::Effect(id)
                        }
                        _ => {
                            push_unique(
                                reasons,
                                FramePaintPlanRejection::UnsupportedPropertyInterleave(key),
                            );
                            continue;
                        }
                    };
                    entries.push(ConsumedPropertyEntry {
                        boundary,
                        expected_before: before,
                        projected_after: cursor,
                    });
                }
                let before = cursor;
                if cursor.scroll != Some(scroll.id) || cursor.clip != Some(contents_clip.id) {
                    push_unique(reasons, FramePaintPlanRejection::InvalidScrollHost(key));
                }
                cursor.scroll = None;
                cursor.clip = contents_clip.parent;
                entries.push(ConsumedPropertyEntry {
                    boundary: ConsumedPropertyBoundary::ScrollContents {
                        scroll: scroll.id,
                        contents_clip: contents_clip.id,
                    },
                    expected_before: before,
                    projected_after: cursor,
                });
                if cursor != PropertyTreeState::default() {
                    push_unique(
                        reasons,
                        FramePaintPlanRejection::UnsupportedPropertyInterleave(key),
                    );
                }
                let consumed_properties = ConsumedPropertyStack {
                    target_owner: key,
                    live_input,
                    entries,
                    projected_output: cursor,
                };
                let phase = PropertyScrollPhaseSchedule {
                    host_before: PropertyScrollPhaseSlot {
                        owner: key,
                        phase: PropertyScrollPhaseKind::HostBeforeChildren,
                        receiver_state: cursor,
                    },
                    content_gap: PropertyScrollContentPhase {
                        owner: key,
                        phase: PropertyScrollPhaseKind::DetachedContentComposite,
                        content_state: live_input,
                        projected_receiver_state: cursor,
                    },
                    overlay_after: PropertyScrollPhaseSlot {
                        owner: key,
                        phase: PropertyScrollPhaseKind::OverlayAfterChildren,
                        receiver_state: cursor,
                    },
                };
                let Ok(ordinal) = u32::try_from(boundaries.len()) else {
                    push_unique(reasons, FramePaintPlanRejection::InvalidPropertyScene);
                    return;
                };
                boundaries.push(PropertyScrollBoundaryContract {
                    ordinal,
                    scene_root_ordinal,
                    scroll,
                    contents_clip,
                    basis: basis.clone(),
                    phase: phase.clone(),
                    consumed_properties,
                });
                schedule.push(PropertySceneScheduledStep::ScrollBoundary {
                    boundary_ordinal: ordinal,
                    scroll: scroll.id,
                    basis,
                    phase,
                });
                path.push(PathBoundary::Scroll(scroll.id));
                true
            }
            (None, None, None) => false,
            _ => false,
        };
        for &child in node.element.children() {
            if arena.parent_of(child) != Some(key) {
                push_unique(reasons, FramePaintPlanRejection::TopologyMismatch(child));
            }
            walk(
                arena,
                child,
                scene_root_ordinal,
                property_trees,
                path,
                seen,
                stable_owners,
                reachable,
                schedule,
                boundaries,
                reasons,
            );
        }
        if pushed {
            path.pop();
        }
    }

    let mut seen = FxHashSet::default();
    let mut stable_owners = FxHashMap::default();
    let mut reachable = FxHashSet::default();
    let mut schedule_steps = Vec::new();
    let mut boundaries = Vec::new();
    let mut schedule_roots = Vec::with_capacity(roots.len());
    let mut plan_roots = Vec::with_capacity(roots.len());
    let mut root_seen = FxHashSet::default();
    for (ordinal, &root) in roots.iter().enumerate() {
        if !root_seen.insert(root) {
            push_unique(&mut reasons, FramePaintPlanRejection::DuplicateRoot(root));
        }
        if arena.parent_of(root).is_some() {
            push_unique(&mut reasons, FramePaintPlanRejection::RootHasParent(root));
        }
        let start = schedule_steps.len();
        let boundary_start = boundaries.len();
        walk(
            arena,
            root,
            u32::try_from(ordinal).unwrap_or(u32::MAX),
            property_trees,
            &mut Vec::new(),
            &mut seen,
            &mut stable_owners,
            &mut reachable,
            &mut schedule_steps,
            &mut boundaries,
            &mut reasons,
        );
        let span = start..schedule_steps.len();
        if boundaries.len() != boundary_start + 1
            || !property_scroll_root_schedule_is_supported(&schedule_steps[span.clone()])
        {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::UnsupportedPropertyInterleave(root),
            );
        }
        let stable_id = arena.get(root).map_or(0, |node| node.element.stable_id());
        let root_witness = PropertyScrollScheduleRoot {
            ordinal: u32::try_from(ordinal).unwrap_or(u32::MAX),
            root,
            stable_id,
            step_span: span,
        };
        schedule_roots.push(root_witness);
        plan_roots.push(PropertySceneRootWitness {
            ordinal: u32::try_from(ordinal).unwrap_or(u32::MAX),
            root,
            stable_id,
            owner: PaintOwnerSnapshot {
                owner: root,
                parent: None,
            },
            top_level_step_span: 0..0,
        });
    }
    if property_trees
        .states
        .keys()
        .any(|owner| !reachable.contains(owner))
        || property_trees
            .transforms
            .values()
            .any(|node| !reachable.contains(&node.owner))
        || property_trees
            .effects
            .values()
            .any(|node| !reachable.contains(&node.owner))
        || property_trees
            .scrolls
            .values()
            .any(|node| !reachable.contains(&node.owner))
        || property_trees.clips.len() != boundaries.len()
        || property_trees.clips.keys().any(|id| {
            !boundaries
                .iter()
                .any(|boundary| boundary.contents_clip.id == *id)
        })
    {
        push_unique(&mut reasons, FramePaintPlanRejection::InvalidPropertyScene);
    }
    if !reasons.is_empty() {
        return Err(FramePaintPlanError { reasons });
    }
    let schedule = PropertySceneSchedule {
        steps: schedule_steps,
    };
    let receiver_insertions = plan_property_scroll_receiver_insertions(
        arena,
        promoted_node_ids,
        property_trees,
        paint_generations,
        context,
        &schedule_roots,
        &schedule,
        &boundaries,
    )?;
    let effect_receiver_insertions = plan_property_effect_scroll_receiver_insertions(
        arena,
        promoted_node_ids,
        property_trees,
        paint_generations,
        context,
        &schedule_roots,
        &schedule,
        &boundaries,
    )?;
    let transform_effect_receiver_insertions =
        plan_property_transform_effect_scroll_receiver_insertions(
            arena,
            promoted_node_ids,
            property_trees,
            paint_generations,
            context,
            &schedule_roots,
            &schedule,
            &boundaries,
        )?;
    let scaffold = PropertyScrollScheduleScaffold {
        context,
        roots: schedule_roots.clone(),
        schedule: schedule.clone(),
        boundaries: boundaries.clone(),
        receiver_insertions: receiver_insertions.clone(),
        effect_receiver_insertions: effect_receiver_insertions.clone(),
        transform_effect_receiver_insertions: transform_effect_receiver_insertions.clone(),
        planned_context: context,
        planned_roots: schedule_roots,
        planned_schedule: schedule,
        planned_boundaries: boundaries,
        planned_receiver_insertions: receiver_insertions,
        planned_effect_receiver_insertions: effect_receiver_insertions,
        planned_transform_effect_receiver_insertions: transform_effect_receiver_insertions,
    };
    let plan = FramePaintPlan {
        steps: Vec::new(),
        property_scene_roots: Some(plan_roots.clone()),
        property_scene_seal: Some(PropertyScenePlanSeal {
            roots: plan_roots,
            context,
            outer_scissor_rect: context.outer_scissor_rect(),
            aggregate_opaque_order_span: 0..0,
            surface_count: 0,
            scene_artifact_validation: Vec::new(),
            surfaces: FxHashMap::default(),
            effect_scaffold: None,
            scroll_schedule_scaffold: Some(scaffold),
            nested_scroll_scaffold: None,
        }),
    };
    property_scene_plan_is_sealed(&plan)
        .then_some(plan)
        .ok_or_else(property_scene_error)
}

fn property_scroll_root_schedule_is_supported(steps: &[PropertySceneScheduledStep]) -> bool {
    match steps {
        [
            PropertySceneScheduledStep::ScrollBoundary {
                basis: ScrollCompositeBasis::FrameRoot,
                ..
            },
        ] => true,
        [
            PropertySceneScheduledStep::RetainedSurface {
                boundary: PropertyScheduledSurfaceBoundary::Transform(transform),
                parent: None,
            },
            PropertySceneScheduledStep::ScrollBoundary {
                basis: ScrollCompositeBasis::Transform(basis),
                ..
            },
        ] => transform == basis,
        [
            PropertySceneScheduledStep::RetainedSurface {
                boundary: PropertyScheduledSurfaceBoundary::Effect(effect),
                parent: None,
            },
            PropertySceneScheduledStep::ScrollBoundary {
                basis: ScrollCompositeBasis::Effect(basis),
                ..
            },
        ] => effect == basis,
        [
            PropertySceneScheduledStep::RetainedSurface {
                boundary: PropertyScheduledSurfaceBoundary::Transform(transform),
                parent: None,
            },
            PropertySceneScheduledStep::RetainedSurface {
                boundary: PropertyScheduledSurfaceBoundary::Effect(effect),
                parent: Some(PropertyScheduledSurfaceBoundaryId::Transform(parent)),
            },
            PropertySceneScheduledStep::ScrollBoundary {
                basis: ScrollCompositeBasis::Effect(basis),
                ..
            },
        ] => transform.id == *parent && effect == basis,
        _ => false,
    }
}

pub(super) fn property_scroll_receiver_artifact_identity(
    artifact: &PaintArtifact,
) -> Option<PropertyScrollReceiverArtifactIdentity> {
    let mut cursor = 0usize;
    let mut chunks = Vec::with_capacity(artifact.chunks.len());
    for chunk in &artifact.chunks {
        if chunk.op_range.start != cursor || chunk.op_range.end > artifact.ops.len() {
            return None;
        }
        cursor = chunk.op_range.end;
        chunks.push(PropertyScrollReceiverChunkIdentity {
            id: chunk.id,
            owner: chunk.owner,
            bounds_bits: [
                chunk.bounds.x.to_bits(),
                chunk.bounds.y.to_bits(),
                chunk.bounds.width.to_bits(),
                chunk.bounds.height.to_bits(),
            ],
            properties: chunk.properties,
            content_revision: chunk.content_revision,
            payload_identity: chunk.payload_identity.clone(),
            op_count: chunk.op_range.len(),
        });
    }
    (cursor == artifact.ops.len()).then(|| PropertyScrollReceiverArtifactIdentity {
        owner_topology: artifact.owner_nodes.clone(),
        clip_nodes: artifact.clip_nodes.clone(),
        effect_nodes: artifact.effect_nodes.clone(),
        chunks,
        op_count: artifact.ops.len(),
        opaque_count: opaque_order_count(artifact),
    })
}

fn property_effect_scroll_receiver_raster_artifact_identity(
    artifact: &PaintArtifact,
    receiver: EffectNodeId,
) -> Option<PropertyScrollReceiverArtifactIdentity> {
    let mut identity = property_scroll_receiver_artifact_identity(artifact)?;
    identity.effect_nodes.retain(|effect| effect.id != receiver);
    for chunk in &mut identity.chunks {
        if chunk.properties.effect == Some(receiver) {
            chunk.properties.effect = None;
        }
        chunk.content_revision.composite_revision = 0;
    }
    Some(identity)
}

#[allow(clippy::too_many_arguments)]
fn plan_property_scroll_receiver_insertions(
    arena: &NodeArena,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    context: TransformSurfacePlanContext,
    roots: &[PropertyScrollScheduleRoot],
    schedule: &PropertySceneSchedule,
    boundaries: &[PropertyScrollBoundaryContract],
) -> Result<Vec<PropertyScrollReceiverInsertionContract>, FramePaintPlanError> {
    let mut insertions = Vec::new();
    for root in roots {
        let root_steps = schedule
            .steps
            .get(root.step_span.clone())
            .ok_or_else(property_scene_error)?;
        let [
            PropertySceneScheduledStep::RetainedSurface {
                boundary: PropertyScheduledSurfaceBoundary::Transform(receiver),
                parent: None,
            },
            PropertySceneScheduledStep::ScrollBoundary {
                boundary_ordinal,
                scroll,
                basis: ScrollCompositeBasis::Transform(basis),
                ..
            },
        ] = root_steps
        else {
            // Effect receivers remain planning-only in B4-2A.
            continue;
        };
        if receiver != basis || receiver.owner != root.root {
            return Err(property_scene_error());
        }
        let boundary = boundaries
            .get(*boundary_ordinal as usize)
            .ok_or_else(property_scene_error)?;
        if boundary.scroll.id != *scroll || boundary.scroll.owner == receiver.owner {
            return Err(property_scene_error());
        }
        let receiver_stable_id = arena
            .get(receiver.owner)
            .ok_or_else(property_scene_error)?
            .element
            .stable_id();
        let scroll_stable_id = arena
            .get(boundary.scroll.owner)
            .ok_or_else(property_scene_error)?
            .element
            .stable_id();
        let scroll_cutout = super::PlannedBoundary {
            root: boundary.scroll.owner,
            stable_id: scroll_stable_id,
            kind: super::PlannedBoundaryKind::Scroll(boundary.scroll.id),
        };
        let Ok(recorded) = super::frame_recorder::record_property_scroll_receiver_steps_for_plan(
            arena,
            receiver.owner,
            promoted_node_ids,
            property_trees,
            paint_generations,
            PaintTransformSurfaceWitness::canonical_root(receiver.owner),
            context.paint_offset(),
            scroll_cutout,
        ) else {
            // B4-0 remains a planning scaffold even when live paint payloads
            // have not reached the recorder-ready lifecycle yet. Production
            // B4-2B requires one exact insertion for every admitted T->S root.
            continue;
        };
        let recorded_steps = recorded
            .into_iter()
            .map(|step| match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    property_scroll_receiver_artifact_identity(&artifact)
                        .map(PropertyScrollReceiverRecordedStepIdentity::Artifact)
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker) => Some(
                    PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(marker),
                ),
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(property_scene_error)?;
        let markers = recorded_steps
            .iter()
            .enumerate()
            .filter_map(|(index, step)| {
                matches!(step, PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(marker) if *marker == scroll_cutout)
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        let [insertion_index] = markers.as_slice() else {
            return Err(property_scene_error());
        };
        let receiver_opaque_before = recorded_steps[..*insertion_index]
            .iter()
            .try_fold(0_u32, |cursor, step| match step {
                PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact) => {
                    cursor.checked_add(artifact.opaque_count)
                }
                PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(_) => None,
            })
            .ok_or_else(property_scene_error)?;
        let receiver_opaque_after = recorded_steps[*insertion_index + 1..]
            .iter()
            .try_fold(receiver_opaque_before, |cursor, step| match step {
                PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact) => {
                    cursor.checked_add(artifact.opaque_count)
                }
                PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(_) => None,
            })
            .ok_or_else(property_scene_error)?;
        insertions.push(PropertyScrollReceiverInsertionContract {
            scene_root_ordinal: root.ordinal,
            receiver: *receiver,
            receiver_stable_id,
            scroll_boundary_ordinal: *boundary_ordinal,
            scroll_cutout,
            insertion_index: *insertion_index,
            before_span: 0..*insertion_index,
            after_span: *insertion_index + 1..recorded_steps.len(),
            receiver_opaque_before,
            receiver_opaque_after,
            recorded_steps,
        });
    }
    Ok(insertions)
}

fn effect_scroll_receiver_raster_bounds(
    receiver_steps: &[super::frame_recorder::RecordedTransformSurfaceStep],
    scroll_viewport: crate::view::base_component::Rect,
) -> Option<[u32; 4]> {
    recorded_step_bounds_union(
        receiver_steps.iter().filter_map(|step| match step {
            super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                Some(artifact.chunks.iter().map(|chunk| chunk.bounds))
            }
            super::frame_recorder::RecordedTransformSurfaceStep::Boundary(_) => None,
        }),
        [
            scroll_viewport.x,
            scroll_viewport.y,
            scroll_viewport.width,
            scroll_viewport.height,
        ],
    )
}

fn recorded_step_bounds_union<I, J>(artifacts: I, seed: [f32; 4]) -> Option<[u32; 4]>
where
    I: IntoIterator<Item = J>,
    J: IntoIterator<Item = crate::view::base_component::Rect>,
{
    let [mut min_x, mut min_y, width, height] = seed;
    let mut max_x = min_x + width;
    let mut max_y = min_y + height;
    if [min_x, min_y, max_x, max_y]
        .iter()
        .any(|value| !value.is_finite())
        || width <= 0.0
        || height <= 0.0
    {
        return None;
    }
    for artifact in artifacts {
        for bounds in artifact {
            let right = bounds.x + bounds.width;
            let bottom = bounds.y + bounds.height;
            if [bounds.x, bounds.y, right, bottom]
                .iter()
                .any(|value| !value.is_finite())
                || bounds.width < 0.0
                || bounds.height < 0.0
            {
                return None;
            }
            min_x = min_x.min(bounds.x);
            min_y = min_y.min(bounds.y);
            max_x = max_x.max(right);
            max_y = max_y.max(bottom);
        }
    }
    let values = [min_x, min_y, max_x - min_x, max_y - min_y];
    (values[0] >= 0.0
        && values[1] >= 0.0
        && values[2] > 0.0
        && values[3] > 0.0
        && values.iter().all(|value| value.is_finite()))
    .then(|| values.map(f32::to_bits))
}

#[allow(clippy::too_many_arguments)]
fn plan_property_effect_scroll_receiver_insertions(
    arena: &NodeArena,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    context: TransformSurfacePlanContext,
    roots: &[PropertyScrollScheduleRoot],
    schedule: &PropertySceneSchedule,
    boundaries: &[PropertyScrollBoundaryContract],
) -> Result<Vec<PropertyEffectScrollReceiverInsertionContract>, FramePaintPlanError> {
    let mut insertions = Vec::new();
    for root in roots {
        let root_steps = schedule
            .steps
            .get(root.step_span.clone())
            .ok_or_else(property_scene_error)?;
        let [
            PropertySceneScheduledStep::RetainedSurface {
                boundary: PropertyScheduledSurfaceBoundary::Effect(receiver),
                parent: None,
            },
            PropertySceneScheduledStep::ScrollBoundary {
                boundary_ordinal,
                scroll,
                basis: ScrollCompositeBasis::Effect(basis),
                ..
            },
        ] = root_steps
        else {
            continue;
        };
        let boundary = boundaries
            .get(*boundary_ordinal as usize)
            .ok_or_else(property_scene_error)?;
        if receiver != basis
            || receiver.id.0 != root.root
            || receiver.owner != root.root
            || receiver.parent.is_some()
            || receiver.generation == 0
            || !receiver.opacity.is_finite()
            || !(0.0..=1.0).contains(&receiver.opacity)
            || boundary.scroll.id != *scroll
            || boundary.scroll.owner == receiver.owner
            || arena.children_of(receiver.owner) != [boundary.scroll.owner]
            || arena.parent_of(boundary.scroll.owner) != Some(receiver.owner)
            || context.paint_offset_bits != [0.0_f32.to_bits(); 2]
            || context.outer_scissor_rect().is_some()
        {
            return Err(property_scene_error());
        }
        // The first executable checkpoint admits no receiver-local clip. The
        // scroll host's own exact contents clip remains owned by the detached
        // H/C/O boundary and is not part of this effect artifact contract.
        let receiver_state = property_trees
            .node_state_for(receiver.owner)
            .ok_or_else(property_scene_error)?;
        if receiver_state.paint.clip.is_some()
            || receiver_state.paint.transform.is_some()
            || receiver_state.paint.scroll.is_some()
            || receiver_state.paint.effect != Some(receiver.id)
        {
            return Err(property_scene_error());
        }
        let generations = paint_generations
            .local_generations_for(receiver.owner)
            .ok_or_else(property_scene_error)?;
        let artifact_contract = EffectPropertySurfaceArtifactContract::new(
            receiver.owner,
            root.stable_id,
            EffectNodeSnapshot {
                parent: None,
                ..*receiver
            },
            vec![*receiver],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![EffectPropertyContentWitness {
                owner: receiver.owner,
                stable_id: root.stable_id,
                parent: None,
                self_paint_revision: generations.self_paint_revision,
                topology_revision: generations.topology_revision,
            }],
        )
        .ok_or_else(property_scene_error)?;
        let scroll_cutout = super::PlannedBoundary {
            root: boundary.scroll.owner,
            stable_id: arena
                .get(boundary.scroll.owner)
                .ok_or_else(property_scene_error)?
                .element
                .stable_id(),
            kind: super::PlannedBoundaryKind::Scroll(boundary.scroll.id),
        };
        let Ok(recorded) =
            super::frame_recorder::record_property_effect_scroll_receiver_steps_for_plan(
                arena,
                receiver.owner,
                promoted_node_ids,
                property_trees,
                paint_generations,
                &artifact_contract,
                context.paint_offset(),
                scroll_cutout,
                None,
            )
        else {
            // The B4 schedule remains a lifecycle-independent planning
            // scaffold. The strict compiler checkpoint later requires this
            // insertion and fails closed when recorder preparation is absent.
            continue;
        };
        let raster_bounds_bits =
            effect_scroll_receiver_raster_bounds(&recorded, boundary.scroll.viewport)
                .ok_or_else(property_scene_error)?;
        let recorded_steps = recorded
            .iter()
            .map(|step| match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    property_scroll_receiver_artifact_identity(artifact)
                        .map(PropertyScrollReceiverRecordedStepIdentity::Artifact)
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker) => Some(
                    PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(*marker),
                ),
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(property_scene_error)?;
        let markers = recorded_steps
            .iter()
            .enumerate()
            .filter_map(|(index, step)| {
                matches!(step, PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(marker) if *marker == scroll_cutout)
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        let [insertion_index] = markers.as_slice() else {
            return Err(property_scene_error());
        };
        let opaque_before = recorded_steps[..*insertion_index]
            .iter()
            .try_fold(0_u32, |cursor, step| match step {
                PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact) => {
                    cursor.checked_add(artifact.opaque_count)
                }
                PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(_) => None,
            })
            .ok_or_else(property_scene_error)?;
        let opaque_after = recorded_steps[*insertion_index + 1..]
            .iter()
            .try_fold(opaque_before, |cursor, step| match step {
                PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact) => {
                    cursor.checked_add(artifact.opaque_count)
                }
                PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(_) => None,
            })
            .ok_or_else(property_scene_error)?;
        let raster_recorded_steps = recorded
            .iter()
            .map(|step| match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    property_effect_scroll_receiver_raster_artifact_identity(artifact, receiver.id)
                        .map(PropertyScrollReceiverRecordedStepIdentity::Artifact)
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker) => Some(
                    PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(*marker),
                ),
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(property_scene_error)?;
        let raster_identity = PropertyEffectScrollReceiverRasterIdentity {
            receiver_owner: receiver.owner,
            receiver_stable_id: root.stable_id,
            raster_bounds_bits,
            local_raster_clips: artifact_contract.local_raster_clips().to_vec(),
            content: artifact_contract.content().to_vec(),
            recorded_steps: raster_recorded_steps,
        };
        insertions.push(PropertyEffectScrollReceiverInsertionContract {
            scene_root_ordinal: root.ordinal,
            receiver: *receiver,
            receiver_stable_id: root.stable_id,
            scroll_boundary_ordinal: *boundary_ordinal,
            scroll_cutout,
            insertion_index: *insertion_index,
            before_span: 0..*insertion_index,
            after_span: *insertion_index + 1..recorded_steps.len(),
            receiver_opaque_before: opaque_before,
            receiver_opaque_after: opaque_after,
            raster_bounds_bits,
            artifact_contract,
            raster_identity,
            recorded_steps,
        });
    }
    Ok(insertions)
}

#[allow(clippy::too_many_arguments)]
fn plan_property_transform_effect_scroll_receiver_insertions(
    arena: &NodeArena,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    context: TransformSurfacePlanContext,
    roots: &[PropertyScrollScheduleRoot],
    schedule: &PropertySceneSchedule,
    boundaries: &[PropertyScrollBoundaryContract],
) -> Result<Vec<PropertyTransformEffectScrollReceiverInsertionContract>, FramePaintPlanError> {
    let mut insertions = Vec::new();
    for root in roots {
        let root_steps = schedule
            .steps
            .get(root.step_span.clone())
            .ok_or_else(property_scene_error)?;
        let [
            PropertySceneScheduledStep::RetainedSurface {
                boundary: PropertyScheduledSurfaceBoundary::Transform(outer),
                parent: None,
            },
            PropertySceneScheduledStep::RetainedSurface {
                boundary: PropertyScheduledSurfaceBoundary::Effect(inner),
                parent: Some(PropertyScheduledSurfaceBoundaryId::Transform(parent)),
            },
            PropertySceneScheduledStep::ScrollBoundary {
                boundary_ordinal,
                scroll,
                basis: ScrollCompositeBasis::Effect(basis),
                ..
            },
        ] = root_steps
        else {
            continue;
        };
        let boundary = boundaries
            .get(*boundary_ordinal as usize)
            .ok_or_else(property_scene_error)?;
        if outer.id != *parent
            || inner != basis
            || outer.owner != root.root
            || outer.parent.is_some()
            || outer.generation == 0
            || inner.parent.is_some()
            || inner.generation == 0
            || !inner.opacity.is_finite()
            || !(0.0..=1.0).contains(&inner.opacity)
            || boundary.scroll.id != *scroll
            || arena.children_of(outer.owner) != [inner.owner]
            || arena.parent_of(inner.owner) != Some(outer.owner)
            || arena.children_of(inner.owner) != [boundary.scroll.owner]
            || arena.parent_of(boundary.scroll.owner) != Some(inner.owner)
            || context.paint_offset_bits != [0.0_f32.to_bits(); 2]
            || context.outer_scissor_rect().is_some()
        {
            return Err(property_scene_error());
        }
        let outer_state = property_trees
            .node_state_for(outer.owner)
            .ok_or_else(property_scene_error)?;
        let inner_state = property_trees
            .node_state_for(inner.owner)
            .ok_or_else(property_scene_error)?;
        if outer_state.paint
            != (PropertyTreeState {
                transform: Some(outer.id),
                ..Default::default()
            })
            || inner_state.paint
                != (PropertyTreeState {
                    transform: Some(outer.id),
                    effect: Some(inner.id),
                    ..Default::default()
                })
        {
            return Err(property_scene_error());
        }
        let outer_node = arena.get(outer.owner).ok_or_else(property_scene_error)?;
        let outer_element = outer_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .ok_or_else(property_scene_error)?;
        let outer_stable_id = arena
            .get(outer.owner)
            .ok_or_else(property_scene_error)?
            .element
            .stable_id();
        let inner_stable_id = arena
            .get(inner.owner)
            .ok_or_else(property_scene_error)?
            .element
            .stable_id();
        let inner_generations = paint_generations
            .local_generations_for(inner.owner)
            .ok_or_else(property_scene_error)?;
        let artifact_contract = EffectPropertySurfaceArtifactContract::new(
            inner.owner,
            inner_stable_id,
            *inner,
            vec![*inner],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![EffectPropertyContentWitness {
                owner: inner.owner,
                stable_id: inner_stable_id,
                parent: None,
                self_paint_revision: inner_generations.self_paint_revision,
                topology_revision: inner_generations.topology_revision,
            }],
        )
        .ok_or_else(property_scene_error)?;
        let effect_cutout = super::PlannedBoundary {
            root: inner.owner,
            stable_id: inner_stable_id,
            kind: super::PlannedBoundaryKind::Isolation(inner.id),
        };
        let scroll_cutout = super::PlannedBoundary {
            root: boundary.scroll.owner,
            stable_id: arena
                .get(boundary.scroll.owner)
                .ok_or_else(property_scene_error)?
                .element
                .stable_id(),
            kind: super::PlannedBoundaryKind::Scroll(boundary.scroll.id),
        };
        let outer_cutouts =
            super::PlannedBoundaryCutoutSet::from_iter([(inner.owner, effect_cutout)]);
        let record_error = |fallbacks: Vec<FrameArtifactFallbackReason>| FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        };
        let outer_recorded =
            super::frame_recorder::record_transform_property_surface_steps_for_plan(
                arena,
                outer.owner,
                promoted_node_ids,
                property_trees,
                paint_generations,
                PaintTransformSurfaceWitness::canonical_root(outer.owner),
                context.paint_offset(),
                &outer_cutouts,
            )
            .map_err(&record_error)?;
        let consumed_transform =
            ConsumedAncestorTransformWitness::new(outer.owner, inner.owner, outer.id)
                .ok_or_else(property_scene_error)?;
        let inner_recorded =
            super::frame_recorder::record_property_effect_scroll_receiver_steps_for_plan(
                arena,
                inner.owner,
                promoted_node_ids,
                property_trees,
                paint_generations,
                &artifact_contract,
                context.paint_offset(),
                scroll_cutout,
                Some(consumed_transform),
            )
            .map_err(&record_error)?;

        let seal_steps = |recorded: &[super::frame_recorder::RecordedTransformSurfaceStep],
                          marker: super::PlannedBoundary|
         -> Option<(
            Vec<PropertyScrollReceiverRecordedStepIdentity>,
            usize,
            u32,
            u32,
        )> {
            let identities = recorded
                .iter()
                .map(|step| match step {
                    super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                        property_scroll_receiver_artifact_identity(artifact)
                            .map(PropertyScrollReceiverRecordedStepIdentity::Artifact)
                    }
                    super::frame_recorder::RecordedTransformSurfaceStep::Boundary(boundary) => {
                        Some(PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(
                            *boundary,
                        ))
                    }
                })
                .collect::<Option<Vec<_>>>()?;
            let markers = identities
                .iter()
                .enumerate()
                .filter_map(|(index, step)| {
                    matches!(step, PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(found) if *found == marker)
                        .then_some(index)
                })
                .collect::<Vec<_>>();
            let [insertion] = markers.as_slice() else {
                return None;
            };
            let before =
                identities[..*insertion]
                    .iter()
                    .try_fold(0_u32, |cursor, step| match step {
                        PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact) => {
                            cursor.checked_add(artifact.opaque_count)
                        }
                        PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(_) => None,
                    })?;
            let after = identities[*insertion + 1..]
                .iter()
                .try_fold(before, |cursor, step| match step {
                    PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact) => {
                        cursor.checked_add(artifact.opaque_count)
                    }
                    PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(_) => None,
                })?;
            Some((identities, *insertion, before, after))
        };
        let (outer_recorded_steps, outer_insertion_index, outer_opaque_before, outer_opaque_after) =
            seal_steps(&outer_recorded, effect_cutout).ok_or_else(property_scene_error)?;
        let (inner_recorded_steps, inner_insertion_index, inner_opaque_before, inner_opaque_after) =
            seal_steps(&inner_recorded, scroll_cutout).ok_or_else(property_scene_error)?;
        let raster_bounds_bits =
            effect_scroll_receiver_raster_bounds(&inner_recorded, boundary.scroll.viewport)
                .ok_or_else(property_scene_error)?;
        // The outer transform composites the detached effect target, not the
        // effect subtree's live descendant geometry.  Build its source bounds
        // from the already cutout-aware E raster union so a negative scroll
        // content offset cannot leak back into T's admission geometry.
        let raster_bounds = raster_bounds_bits.map(f32::from_bits);
        let outer_raster_bounds_bits = recorded_step_bounds_union(
            outer_recorded.iter().filter_map(|step| match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    Some(artifact.chunks.iter().map(|chunk| chunk.bounds))
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(_) => None,
            }),
            raster_bounds,
        )
        .ok_or_else(property_scene_error)?;
        let outer_raster_bounds = outer_raster_bounds_bits.map(f32::from_bits);
        let outer_geometry = outer_element
            .exact_transform_receiver_geometry_snapshot_for_raster_bounds(
                crate::view::base_component::PromotionCompositeBounds {
                    x: outer_raster_bounds[0],
                    y: outer_raster_bounds[1],
                    width: outer_raster_bounds[2],
                    height: outer_raster_bounds[3],
                    corner_radii: [0.0; 4],
                },
                context.paint_offset(),
                None,
            )
            .ok_or_else(property_scene_error)?;
        if outer_geometry
            .viewport_transform
            .to_cols_array()
            .map(f32::to_bits)
            != outer.viewport_matrix.to_cols_array().map(f32::to_bits)
            || super::compiler::direct_translation_bits(outer_geometry.viewport_transform).is_none()
            || outer_geometry.outer_scissor_rect.is_some()
        {
            return Err(property_scene_error());
        }
        let raster_recorded_steps = inner_recorded
            .iter()
            .map(|step| match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    property_effect_scroll_receiver_raster_artifact_identity(artifact, inner.id)
                        .map(PropertyScrollReceiverRecordedStepIdentity::Artifact)
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker) => Some(
                    PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(*marker),
                ),
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(property_scene_error)?;
        let inner_insertion = PropertyEffectScrollReceiverInsertionContract {
            scene_root_ordinal: root.ordinal,
            receiver: *inner,
            receiver_stable_id: inner_stable_id,
            scroll_boundary_ordinal: *boundary_ordinal,
            scroll_cutout,
            insertion_index: inner_insertion_index,
            before_span: 0..inner_insertion_index,
            after_span: inner_insertion_index + 1..inner_recorded_steps.len(),
            receiver_opaque_before: inner_opaque_before,
            receiver_opaque_after: inner_opaque_after,
            raster_bounds_bits,
            raster_identity: PropertyEffectScrollReceiverRasterIdentity {
                receiver_owner: inner.owner,
                receiver_stable_id: inner_stable_id,
                raster_bounds_bits,
                local_raster_clips: artifact_contract.local_raster_clips().to_vec(),
                content: artifact_contract.content().to_vec(),
                recorded_steps: raster_recorded_steps,
            },
            artifact_contract,
            recorded_steps: inner_recorded_steps,
        };
        insertions.push(PropertyTransformEffectScrollReceiverInsertionContract {
            scene_root_ordinal: root.ordinal,
            outer_receiver: *outer,
            outer_stable_id,
            outer_geometry,
            effect_cutout,
            outer_insertion_index,
            outer_before_span: 0..outer_insertion_index,
            outer_after_span: outer_insertion_index + 1..outer_recorded_steps.len(),
            outer_opaque_before,
            outer_opaque_after,
            inner: inner_insertion,
            outer_recorded_steps,
        });
    }
    Ok(insertions)
}

/// Materializes the canonical M12A2 effect scaffold into the existing
/// PropertyScene transaction shape. The scaffold remains the owning topology
/// authority; this phase only records typed artifacts and freezes executable
/// surface steps. It never widens the generic retained-tree gate.
pub(crate) fn plan_property_effect_scene_with_context(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    context: TransformSurfacePlanContext,
) -> Result<FramePaintPlan, FramePaintPlanError> {
    let mut plan = plan_property_effect_scene_scaffold_with_context(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        context,
    )?;
    let scaffold = plan
        .property_scene_seal
        .as_ref()
        .and_then(|seal| seal.effect_scaffold.as_ref())
        .cloned()
        .ok_or_else(property_scene_error)?;
    let ordinals = scaffold
        .surfaces
        .iter()
        .map(|surface| (surface.boundary.owner(), surface.ordinal))
        .collect::<FxHashMap<_, _>>();
    let record_error = |fallbacks: Vec<FrameArtifactFallbackReason>| FramePaintPlanError {
        reasons: fallbacks
            .into_iter()
            .map(FramePaintPlanRejection::Coverage)
            .collect(),
    };

    let mut scene_steps = Vec::new();
    let mut scene_validation = Vec::new();
    let mut root_step_spans = Vec::with_capacity(roots.len());
    let mut scene_cursor = 0_u32;
    let mut built = FxHashSet::default();
    for (root_ordinal, &root) in roots.iter().enumerate() {
        let cutouts = property_effect_direct_cutouts(&scaffold, None, root_ordinal as u32)?;
        let recorded = super::frame_recorder::record_property_scene_steps_for_plan(
            arena,
            &[root],
            promoted_node_ids,
            property_trees,
            paint_generations,
            context.paint_offset(),
            &cutouts,
        )
        .map_err(&record_error)?;
        let start = scene_steps.len();
        for item in recorded {
            match item {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    let witness =
                        super::compiler::validate_property_scene_artifact_for_plan(&artifact)
                            .ok_or_else(property_scene_error)?;
                    if artifact.ops.is_empty() {
                        continue;
                    }
                    let end = scene_cursor
                        .checked_add(opaque_order_count(&artifact))
                        .ok_or_else(property_scene_error)?;
                    scene_validation.push(witness);
                    scene_steps.push(PaintPlanStep::ArtifactSpan(ArtifactSpanPlan {
                        artifact,
                        opaque_order_span: scene_cursor..end,
                    }));
                    scene_cursor = end;
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(boundary) => {
                    let ordinal = *ordinals
                        .get(&boundary.root)
                        .ok_or_else(property_scene_error)?;
                    let contract = &scaffold.surfaces[ordinal as usize];
                    if contract.parent_boundary_ordinal.is_some()
                        || contract.scene_root_ordinal as usize != root_ordinal
                        || !built.insert(ordinal)
                    {
                        return Err(property_scene_error());
                    }
                    let surface = materialize_property_effect_surface(
                        arena,
                        promoted_node_ids,
                        property_trees,
                        paint_generations,
                        &scaffold,
                        ordinal,
                        &ordinals,
                        &mut built,
                    )?;
                    if matches!(surface.kind, SurfaceKind::Transform(_)) {
                        scene_cursor = scene_cursor.max(surface.aggregate_opaque_order_span.end);
                    }
                    scene_steps.push(PaintPlanStep::RetainedSurface(Box::new(surface)));
                }
            }
        }
        root_step_spans.push(start..scene_steps.len());
    }
    if built.len() != scaffold.surfaces.len() {
        return Err(property_scene_error());
    }
    let seal = plan
        .property_scene_seal
        .as_mut()
        .ok_or_else(property_scene_error)?;
    let scaffold = seal
        .effect_scaffold
        .as_mut()
        .ok_or_else(property_scene_error)?;
    scaffold.production_root_step_spans = Some(root_step_spans.clone());
    scaffold.planned_production_root_step_spans = Some(root_step_spans);
    seal.surface_count = scaffold.surfaces.len();
    seal.aggregate_opaque_order_span = 0..scene_cursor;
    seal.scene_artifact_validation = scene_validation;
    plan.steps = scene_steps;
    property_scene_plan_is_sealed(&plan)
        .then_some(plan)
        .ok_or_else(property_scene_error)
}

fn property_effect_direct_cutouts(
    scaffold: &PropertyEffectSceneScaffold,
    parent: Option<u32>,
    scene_root_ordinal: u32,
) -> Result<super::PlannedBoundaryCutoutSet, FramePaintPlanError> {
    let mut cutouts = Vec::new();
    for surface in &scaffold.surfaces {
        if surface.parent_boundary_ordinal != parent
            || surface.scene_root_ordinal != scene_root_ordinal
        {
            continue;
        }
        let kind = match surface.boundary {
            PropertyBoundaryId::Transform(transform) => {
                super::PlannedBoundaryKind::Transform(transform)
            }
            PropertyBoundaryId::Effect(effect) => super::PlannedBoundaryKind::Isolation(effect),
        };
        cutouts.push((
            surface.boundary.owner(),
            super::PlannedBoundary {
                root: surface.boundary.owner(),
                stable_id: surface.stable_id,
                kind,
            },
        ));
    }
    Ok(super::PlannedBoundaryCutoutSet::from_iter(cutouts))
}

#[allow(clippy::too_many_arguments)]
fn materialize_property_effect_surface(
    arena: &NodeArena,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scaffold: &PropertyEffectSceneScaffold,
    ordinal: u32,
    ordinals: &FxHashMap<NodeKey, u32>,
    built: &mut FxHashSet<u32>,
) -> Result<RetainedSurfacePlan, FramePaintPlanError> {
    let surface = scaffold
        .surfaces
        .get(ordinal as usize)
        .ok_or_else(property_scene_error)?;
    let owner = surface.boundary.owner();
    let node = arena.get(owner).ok_or_else(property_scene_error)?;
    let parent_owner = surface
        .parent_boundary_ordinal
        .map(|parent| scaffold.surfaces[parent as usize].boundary.owner());
    let cutouts =
        property_effect_direct_cutouts(scaffold, Some(ordinal), surface.scene_root_ordinal)?;
    let record_error = |fallbacks: Vec<FrameArtifactFallbackReason>| FramePaintPlanError {
        reasons: fallbacks
            .into_iter()
            .map(FramePaintPlanRejection::Coverage)
            .collect(),
    };
    let (recorded, kind, persistent_color_key) = match &surface.kind {
        PropertyEffectSurfaceKind::Transform { snapshot, .. } => {
            let element = node
                .element
                .as_any()
                .downcast_ref::<Element>()
                .ok_or_else(property_scene_error)?;
            let geometry = exact_surface_geometry_for_plan(
                element,
                arena,
                owner,
                scaffold.context,
                Some(snapshot.viewport_matrix),
            )?;
            let witness = PaintTransformSurfaceWitness::canonical_root(owner);
            let recorded = super::frame_recorder::record_transform_property_surface_steps_for_plan(
                arena,
                owner,
                promoted_node_ids,
                property_trees,
                paint_generations,
                witness,
                scaffold.context.paint_offset(),
                &cutouts,
            )
            .map_err(&record_error)?;
            (
                recorded,
                SurfaceKind::Transform(TransformSurfacePlan {
                    transform: snapshot.id,
                    geometry,
                    context: scaffold.context,
                    planned_geometry_witness: geometry,
                    planned_context_witness: scaffold.context,
                }),
                crate::view::base_component::transformed_layer_stable_key(surface.stable_id),
            )
        }
        PropertyEffectSurfaceKind::Isolation(isolation) => {
            let content = isolation
                .raster_identity
                .content
                .iter()
                .map(|entry| EffectPropertyContentWitness {
                    owner: entry.owner,
                    stable_id: entry.stable_id,
                    parent: entry.parent,
                    self_paint_revision: entry.self_paint_revision,
                    topology_revision: entry.topology_revision,
                })
                .collect();
            let artifact_contract = EffectPropertySurfaceArtifactContract::new(
                owner,
                surface.stable_id,
                isolation.effect_chain.isolated_leaf,
                isolation.effect_chain.live_leaf_to_root.clone(),
                isolation.effect_chain.detached_ancestors.clone(),
                isolation.local_raster_clips.clone(),
                isolation.ancestor_composite_clips.clone(),
                content,
            )
            .ok_or_else(property_scene_error)?;
            let consumed_transform = surface.parent_boundary_ordinal.and_then(|parent| {
                let parent = &scaffold.surfaces[parent as usize];
                match parent.boundary {
                    PropertyBoundaryId::Transform(transform) => {
                        ConsumedAncestorTransformWitness::new(
                            parent.boundary.owner(),
                            owner,
                            transform,
                        )
                    }
                    PropertyBoundaryId::Effect(_) => None,
                }
            });
            let paint_offset = isolation.raster_space.paint_offset_bits.map(f32::from_bits);
            let recorded = super::frame_recorder::record_effect_property_surface_steps_for_plan(
                arena,
                promoted_node_ids,
                property_trees,
                paint_generations,
                &artifact_contract,
                paint_offset,
                &cutouts,
                consumed_transform,
            )
            .map_err(&record_error)?;
            let [x, y, width, height] = isolation
                .raster_space
                .source_bounds_bits
                .map(f32::from_bits);
            let geometry = NestedIsolationSurfaceGeometrySnapshot::from_exact_retained_output(
                crate::view::base_component::PromotionCompositeBounds {
                    x,
                    y,
                    width,
                    height,
                    corner_radii: [0.0; 4],
                },
            )
            .ok_or_else(property_scene_error)?;
            (
                recorded,
                SurfaceKind::NestedIsolation(NestedIsolationSurfacePlan {
                    effect: isolation.effect_chain.isolated_leaf,
                    geometry,
                    planned_geometry_witness: geometry,
                    property_scene: Some(isolation.clone()),
                    property_scene_artifact: Some(artifact_contract),
                }),
                crate::view::base_component::isolation_layer_stable_key(surface.stable_id),
            )
        }
    };

    let mut raster_steps = Vec::new();
    let mut cursor = 0_u32;
    let mut seen_children = FxHashSet::default();
    for (step_index, item) in recorded.into_iter().enumerate() {
        match item {
            super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                let valid = match &kind {
                    SurfaceKind::Transform(plan) => {
                        super::compiler::validate_transform_property_surface_artifact(
                            &artifact,
                            owner,
                            plan.transform,
                        )
                        .is_some()
                    }
                    SurfaceKind::NestedIsolation(plan) => plan
                        .property_scene_artifact
                        .as_ref()
                        .is_some_and(|contract| {
                            super::compiler::validate_effect_property_surface_artifact(
                                &artifact, contract,
                            )
                            .is_some()
                        }),
                    SurfaceKind::Isolation(_) | SurfaceKind::ScrollHost(_) => false,
                };
                if !valid {
                    return Err(property_scene_error());
                }
                if artifact.ops.is_empty() {
                    continue;
                }
                let end = cursor
                    .checked_add(opaque_order_count(&artifact))
                    .ok_or_else(property_scene_error)?;
                raster_steps.push(PaintPlanStep::ArtifactSpan(ArtifactSpanPlan {
                    artifact,
                    opaque_order_span: cursor..end,
                }));
                cursor = end;
            }
            super::frame_recorder::RecordedTransformSurfaceStep::Boundary(boundary) => {
                let child_ordinal = *ordinals
                    .get(&boundary.root)
                    .ok_or_else(property_scene_error)?;
                let child_contract = &scaffold.surfaces[child_ordinal as usize];
                if child_contract.parent_boundary_ordinal != Some(ordinal)
                    || !seen_children.insert(child_ordinal)
                    || !built.insert(child_ordinal)
                {
                    return Err(property_scene_error());
                }
                let child = materialize_property_effect_surface(
                    arena,
                    promoted_node_ids,
                    property_trees,
                    paint_generations,
                    scaffold,
                    child_ordinal,
                    ordinals,
                    built,
                )?;
                let _ = step_index;
                // Property effect composites are translucent and never consume
                // the owning parent's opaque cursor. Their full child stamp is
                // nevertheless embedded by executor preparation.
                raster_steps.push(PaintPlanStep::RetainedSurface(Box::new(child)));
            }
        }
    }
    let expected_children = scaffold
        .surfaces
        .iter()
        .filter(|child| child.parent_boundary_ordinal == Some(ordinal))
        .map(|child| child.ordinal)
        .collect::<FxHashSet<_>>();
    if seen_children != expected_children {
        return Err(property_scene_error());
    }
    Ok(RetainedSurfacePlan {
        boundary_root: owner,
        stable_id: surface.stable_id,
        persistent_color_key,
        kind,
        raster_steps,
        parent_surface: parent_owner,
        aggregate_opaque_order_span: 0..cursor,
    })
}

fn validate_transform_property_scene_inputs(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_offset: [f32; 2],
) -> Result<(Vec<NodeKey>, PropertyScenePlanningIndex), FramePaintPlanError> {
    if roots.is_empty() {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::EmptyScene],
        });
    }
    let mut reasons = property_trees
        .validation_errors
        .iter()
        .copied()
        .map(FramePaintPlanRejection::PropertyTree)
        .collect::<Vec<_>>();
    for &stable_id in promoted_node_ids {
        push_unique(
            &mut reasons,
            FramePaintPlanRejection::PromotionPresent(stable_id),
        );
    }
    let mut root_seen = FxHashSet::default();
    for &root in roots {
        if !root_seen.insert(root) {
            push_unique(&mut reasons, FramePaintPlanRejection::DuplicateRoot(root));
        }
        if arena.parent_of(root).is_some() {
            push_unique(&mut reasons, FramePaintPlanRejection::RootHasParent(root));
        }
    }

    fn walk(
        arena: &NodeArena,
        key: NodeKey,
        parent_context: super::PaintRecordingContext,
        seen: &mut FxHashSet<NodeKey>,
        stable_owners: &mut FxHashMap<u64, NodeKey>,
        reachable: &mut Vec<NodeKey>,
        paint_offsets: &mut FxHashMap<NodeKey, [f32; 2]>,
        reasons: &mut Vec<FramePaintPlanRejection>,
    ) {
        if !seen.insert(key) {
            push_unique(reasons, FramePaintPlanRejection::DuplicateNodeKey(key));
            return;
        }
        let Some(node) = arena.get(key) else {
            push_unique(reasons, FramePaintPlanRejection::MissingRoot(key));
            return;
        };
        if node.children() != node.element.children() {
            push_unique(reasons, FramePaintPlanRejection::TopologyMismatch(key));
        }
        let stable_id = node.element.stable_id();
        if stable_id == 0 {
            push_unique(reasons, FramePaintPlanRejection::InvalidStableId(key));
        } else if stable_owners.insert(stable_id, key).is_some() {
            push_unique(
                reasons,
                FramePaintPlanRejection::DuplicateStableId(stable_id),
            );
        }
        if node.element.is_deferred_to_root_viewport_render() {
            push_unique(reasons, FramePaintPlanRejection::DeferredBoundary(key));
        }
        if node
            .element
            .placement_eligibility_metadata()
            .contains_runtime_layout_state
        {
            push_unique(reasons, FramePaintPlanRejection::LayoutTransition(key));
        }
        let recording_context = node.element.shadow_paint_recording_context(parent_context);
        paint_offsets.insert(key, recording_context.paint_offset);
        reachable.push(key);
        for &child in node.element.children() {
            if arena.parent_of(child) != Some(key) {
                push_unique(reasons, FramePaintPlanRejection::TopologyMismatch(child));
            }
            let child_context = node.element.shadow_paint_recording_context_for_child(
                child,
                arena,
                recording_context,
            );
            walk(
                arena,
                child,
                child_context,
                seen,
                stable_owners,
                reachable,
                paint_offsets,
                reasons,
            );
        }
    }

    let mut reachable = Vec::new();
    let mut seen = FxHashSet::default();
    let mut stable_owners = FxHashMap::default();
    let mut paint_offsets = FxHashMap::default();
    for &root in roots {
        walk(
            arena,
            root,
            super::PaintRecordingContext {
                paint_offset,
                ..Default::default()
            },
            &mut seen,
            &mut stable_owners,
            &mut reachable,
            &mut paint_offsets,
            &mut reasons,
        );
    }
    let reachable_set = reachable.iter().copied().collect::<FxHashSet<_>>();
    for &key in &reachable {
        if !property_trees.states.contains_key(&key) {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::MissingPropertyState(key),
            );
        }
    }
    for &key in property_trees.states.keys() {
        if !reachable_set.contains(&key) {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::UnexpectedPropertyState(key),
            );
        }
    }
    for (&id, effect) in &property_trees.effects {
        let _ = id;
        push_unique(
            &mut reasons,
            FramePaintPlanRejection::EffectBoundary(effect.owner),
        );
    }
    for (&id, scroll) in &property_trees.scrolls {
        let _ = id;
        push_unique(
            &mut reasons,
            FramePaintPlanRejection::ScrollBoundary(scroll.owner),
        );
    }
    for (&id, clip) in &property_trees.clips {
        let exact = id.owner == clip.owner
            && reachable_set.contains(&clip.owner)
            && clip.generation != 0
            && matches!(clip.geometry, ClipGeometry::LogicalScissor(_))
            && matches!(
                (id.role, clip.behavior),
                (ClipNodeRole::SelfClip, ClipBehavior::Replace)
                    | (ClipNodeRole::ContentsClip, ClipBehavior::Intersect)
            )
            && property_trees.clip_snapshot_for(Some(id)).is_some();
        if !exact {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::InvalidClipChain(clip.owner),
            );
        }
    }

    let mut ids_by_transform = FxHashMap::default();
    let mut next_ordinal = 0_u32;
    for &key in &reachable {
        let id = TransformNodeId(key);
        let Some(snapshot) = property_trees.transform_snapshot_for(id) else {
            continue;
        };
        let valid = snapshot.owner == key
            && snapshot.id == id
            && snapshot.generation != 0
            && matrix_is_finite_affine(snapshot.viewport_matrix)
            && arena
                .get(key)
                .is_some_and(|node| node.element.as_any().downcast_ref::<Element>().is_some());
        if !valid {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::InvalidRootTransform(key),
            );
            continue;
        }
        let Some(surface_id) = PropertySurfaceId::new(next_ordinal, key, id) else {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::UnexpectedTransform(key),
            );
            continue;
        };
        next_ordinal = match next_ordinal.checked_add(1) {
            Some(next) => next,
            None => {
                push_unique(&mut reasons, FramePaintPlanRejection::InvalidPropertyScene);
                continue;
            }
        };
        ids_by_transform.insert(id, surface_id);
    }
    for (&id, transform) in &property_trees.transforms {
        if !reachable_set.contains(&transform.owner) || !ids_by_transform.contains_key(&id) {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::UnexpectedTransform(transform.owner),
            );
        }
        if let Some(parent) = transform.parent {
            let mut cursor = arena.parent_of(transform.owner);
            let mut parent_is_ancestor = false;
            while let Some(owner) = cursor {
                if owner == parent.0 {
                    parent_is_ancestor = true;
                    break;
                }
                cursor = arena.parent_of(owner);
            }
            if !parent_is_ancestor || !property_trees.transforms.contains_key(&parent) {
                push_unique(
                    &mut reasons,
                    FramePaintPlanRejection::WrongTransformBoundary(transform.owner),
                );
            }
        }
    }
    if !reasons.is_empty() {
        return Err(FramePaintPlanError { reasons });
    }
    let mut direct_children = FxHashMap::<PropertySurfaceId, Vec<PropertySurfaceId>>::default();
    for (&transform, &id) in &ids_by_transform {
        if let Some(parent_transform) = property_trees.transforms[&transform].parent {
            let Some(&parent) = ids_by_transform.get(&parent_transform) else {
                return Err(property_scene_error());
            };
            direct_children.entry(parent).or_default().push(id);
        }
    }
    for children in direct_children.values_mut() {
        children.sort_unstable_by_key(|id| id.ordinal);
    }
    Ok((
        reachable,
        PropertyScenePlanningIndex {
            ids_by_transform,
            direct_children,
            paint_offsets,
        },
    ))
}

fn planned_transform_cutouts(
    arena: &NodeArena,
    ids: impl IntoIterator<Item = PropertySurfaceId>,
) -> Result<super::PlannedBoundaryCutoutSet, FramePaintPlanError> {
    let mut cutouts = super::PlannedBoundaryCutoutSet::default();
    for id in ids {
        let Some(node) = arena.get(id.owner) else {
            return Err(property_scene_error());
        };
        let boundary = super::PlannedBoundary {
            root: id.owner,
            stable_id: node.element.stable_id(),
            kind: super::PlannedBoundaryKind::Transform(id.transform),
        };
        if cutouts.insert(id.owner, boundary).is_some() {
            return Err(property_scene_error());
        }
    }
    Ok(cutouts)
}

fn node_is_within_root(arena: &NodeArena, mut owner: NodeKey, root: NodeKey) -> bool {
    loop {
        if owner == root {
            return true;
        }
        let Some(parent) = arena.parent_of(owner) else {
            return false;
        };
        owner = parent;
    }
}

#[allow(clippy::too_many_arguments)]
fn plan_transform_property_surface(
    arena: &NodeArena,
    id: PropertySurfaceId,
    parent: Option<PropertySurfaceId>,
    scene_root: NodeKey,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    index: &PropertyScenePlanningIndex,
    incoming_scissor: Option<[u32; 4]>,
    contracts: &mut FxHashMap<PropertySurfaceId, TransformPropertySurfaceContract>,
    built: &mut FxHashSet<PropertySurfaceId>,
) -> Result<RetainedSurfacePlan, FramePaintPlanError> {
    if !built.insert(id) {
        return Err(property_scene_error());
    }
    let Some(node) = arena.get(id.owner) else {
        return Err(property_scene_error());
    };
    let Some(element) = node.element.as_any().downcast_ref::<Element>() else {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::UnknownRootHost(id.owner)],
        });
    };
    let Some(transform) = property_trees.transform_snapshot_for(id.transform) else {
        return Err(property_scene_error());
    };
    if transform.owner != id.owner || transform.id != id.transform {
        return Err(property_scene_error());
    }
    let ancestor_composite_clips = ancestor_clip_chain_for_surface(property_trees, id.owner)?;
    let resolved_composite_scissor =
        resolve_composite_scissor(incoming_scissor, &ancestor_composite_clips)?;
    let paint_offset = *index
        .paint_offsets
        .get(&id.owner)
        .ok_or_else(property_scene_error)?;
    let surface_context =
        TransformSurfacePlanContext::new(paint_offset, resolved_composite_scissor);
    let geometry = exact_surface_geometry_for_plan(
        element,
        arena,
        id.owner,
        surface_context,
        Some(transform.viewport_matrix),
    )?;
    let direct_children = index.direct_children.get(&id).cloned().unwrap_or_default();
    let cutouts = planned_transform_cutouts(arena, direct_children.iter().copied())?;
    let record_error = |fallbacks: Vec<FrameArtifactFallbackReason>| FramePaintPlanError {
        reasons: fallbacks
            .into_iter()
            .map(FramePaintPlanRejection::Coverage)
            .collect(),
    };
    let recorded = super::frame_recorder::record_transform_property_surface_steps_for_plan(
        arena,
        id.owner,
        promoted_node_ids,
        property_trees,
        paint_generations,
        PaintTransformSurfaceWitness::canonical_root(id.owner),
        paint_offset,
        &cutouts,
    )
    .map_err(record_error)?;
    let mut raster_steps = Vec::new();
    let mut cursor = 0_u32;
    let mut seen_children = FxHashSet::default();
    let mut artifact_validation = Vec::new();
    for item in recorded {
        match item {
            super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                let artifact = detach_ancestor_clip_chain(artifact, &ancestor_composite_clips)?;
                let Some(witness) =
                    super::compiler::validate_transform_property_surface_artifact_for_plan(
                        &artifact,
                        id.owner,
                        id.transform,
                    )
                else {
                    return Err(FramePaintPlanError {
                        reasons: vec![FramePaintPlanRejection::InvalidSurfaceArtifact(id.owner)],
                    });
                };
                let end = cursor
                    .checked_add(opaque_order_count(&artifact))
                    .ok_or_else(property_scene_error)?;
                artifact_validation.push(witness);
                raster_steps.push(PaintPlanStep::ArtifactSpan(ArtifactSpanPlan {
                    artifact,
                    opaque_order_span: cursor..end,
                }));
                cursor = end;
            }
            super::frame_recorder::RecordedTransformSurfaceStep::Boundary(boundary) => {
                let transform = match boundary.kind {
                    super::PlannedBoundaryKind::Transform(transform) => transform,
                    super::PlannedBoundaryKind::Isolation(_)
                    | super::PlannedBoundaryKind::Scroll(_) => {
                        return Err(property_scene_error());
                    }
                };
                let Some(&child_id) = index.ids_by_transform.get(&transform) else {
                    return Err(property_scene_error());
                };
                if !direct_children.contains(&child_id)
                    || boundary.root != child_id.owner
                    || !seen_children.insert(child_id)
                {
                    return Err(property_scene_error());
                }
                let child = plan_transform_property_surface(
                    arena,
                    child_id,
                    Some(id),
                    scene_root,
                    promoted_node_ids,
                    property_trees,
                    paint_generations,
                    index,
                    resolved_composite_scissor,
                    contracts,
                    built,
                )?;
                cursor = cursor.max(child.aggregate_opaque_order_span.end);
                raster_steps.push(PaintPlanStep::RetainedSurface(Box::new(child)));
            }
        }
    }
    if seen_children.len() != direct_children.len() {
        return Err(property_scene_error());
    }
    let contract = TransformPropertySurfaceContract {
        id,
        parent,
        scene_root,
        stable_id: node.element.stable_id(),
        transform,
        planned_transform_witness: transform,
        ancestor_composite_clips,
        resolved_composite_scissor,
        artifact_validation,
    };
    if contracts.insert(id, contract).is_some() {
        return Err(property_scene_error());
    }
    Ok(RetainedSurfacePlan {
        boundary_root: id.owner,
        stable_id: node.element.stable_id(),
        persistent_color_key: crate::view::base_component::transformed_layer_stable_key(
            node.element.stable_id(),
        ),
        kind: SurfaceKind::Transform(TransformSurfacePlan {
            transform: id.transform,
            geometry,
            context: surface_context,
            planned_geometry_witness: geometry,
            planned_context_witness: surface_context,
        }),
        raster_steps,
        parent_surface: parent.map(PropertySurfaceId::owner),
        aggregate_opaque_order_span: 0..cursor,
    })
}

fn ancestor_clip_chain_for_surface(
    property_trees: &PropertyTrees,
    owner: NodeKey,
) -> Result<Vec<ClipNodeSnapshot>, FramePaintPlanError> {
    let state = property_trees
        .node_state_for(owner)
        .ok_or_else(property_scene_error)?;
    let own_self = ClipNodeId {
        owner,
        role: ClipNodeRole::SelfClip,
    };
    let inherited_leaf = if state.paint.clip == Some(own_self) {
        property_trees
            .clips
            .get(&own_self)
            .ok_or_else(|| FramePaintPlanError {
                reasons: vec![FramePaintPlanRejection::InvalidClipChain(owner)],
            })?
            .parent
    } else {
        state.paint.clip
    };
    property_trees
        .clip_snapshot_for(inherited_leaf)
        .ok_or_else(|| FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::InvalidClipChain(owner)],
        })
}

fn freeze_property_effect_clip_forest(
    arena: &NodeArena,
    reachable: &[NodeKey],
    property_trees: &PropertyTrees,
) -> Result<PropertyEffectClipForestContract, FramePaintPlanError> {
    let mut states = Vec::with_capacity(reachable.len());
    let mut nodes = Vec::new();
    let mut frozen_nodes = FxHashMap::default();
    for &owner in reachable {
        let node = arena.get(owner).ok_or_else(property_scene_error)?;
        let state = property_trees
            .node_state_for(owner)
            .ok_or_else(property_scene_error)?;
        states.push(PropertyEffectClipStateWitness {
            owner,
            stable_id: node.element.stable_id(),
            parent: node.parent(),
            paint_leaf: state.paint.clip,
            descendants_leaf: state.descendants.clip,
        });
        for leaf in [state.paint.clip, state.descendants.clip] {
            for snapshot in property_trees
                .clip_snapshot_for(leaf)
                .ok_or_else(property_scene_error)?
            {
                match frozen_nodes.entry(snapshot.id) {
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert(snapshot);
                        nodes.push(snapshot);
                    }
                    std::collections::hash_map::Entry::Occupied(entry)
                        if *entry.get() == snapshot => {}
                    std::collections::hash_map::Entry::Occupied(_) => {
                        return Err(property_scene_error());
                    }
                }
            }
        }
    }
    Ok(PropertyEffectClipForestContract { states, nodes })
}

fn frozen_clip_chain(
    leaf: Option<ClipNodeId>,
    nodes: &FxHashMap<ClipNodeId, ClipNodeSnapshot>,
) -> Option<Vec<ClipNodeSnapshot>> {
    let mut chain = Vec::new();
    let mut cursor = leaf;
    let mut seen = FxHashSet::default();
    while let Some(id) = cursor {
        if !seen.insert(id) {
            return None;
        }
        let snapshot = *nodes.get(&id)?;
        chain.push(snapshot);
        cursor = snapshot.parent;
    }
    Some(chain)
}

pub(super) fn resolve_composite_scissor(
    incoming: Option<[u32; 4]>,
    leaf_to_root: &[ClipNodeSnapshot],
) -> Result<Option<[u32; 4]>, FramePaintPlanError> {
    let mut resolved = incoming;
    let mut expected_child = None;
    for snapshot in leaf_to_root {
        if snapshot.generation == 0
            || expected_child.is_some_and(|child: ClipNodeId| child != snapshot.id)
        {
            return Err(property_scene_error());
        }
        expected_child = snapshot.parent;
    }
    for snapshot in leaf_to_root.iter().rev() {
        resolved = match snapshot.behavior {
            ClipBehavior::Replace => Some(snapshot.logical_scissor),
            ClipBehavior::Intersect => Some(match resolved {
                Some(current) => intersect_scissors(current, snapshot.logical_scissor),
                None => snapshot.logical_scissor,
            }),
        };
    }
    Ok(resolved)
}

fn intersect_scissors(left: [u32; 4], right: [u32; 4]) -> [u32; 4] {
    let left_max_x = left[0].saturating_add(left[2]);
    let left_max_y = left[1].saturating_add(left[3]);
    let right_max_x = right[0].saturating_add(right[2]);
    let right_max_y = right[1].saturating_add(right[3]);
    let x = left[0].max(right[0]);
    let y = left[1].max(right[1]);
    let max_x = left_max_x.min(right_max_x);
    let max_y = left_max_y.min(right_max_y);
    [x, y, max_x.saturating_sub(x), max_y.saturating_sub(y)]
}

fn detach_ancestor_clip_chain(
    mut artifact: PaintArtifact,
    ancestor_clips: &[ClipNodeSnapshot],
) -> Result<PaintArtifact, FramePaintPlanError> {
    let ancestor_ids = ancestor_clips
        .iter()
        .map(|snapshot| snapshot.id)
        .collect::<FxHashSet<_>>();
    if ancestor_ids.len() != ancestor_clips.len() {
        return Err(property_scene_error());
    }
    let local_owners = artifact
        .owner_nodes
        .iter()
        .map(|snapshot| snapshot.owner)
        .collect::<FxHashSet<_>>();
    for chunk in &mut artifact.chunks {
        if chunk
            .properties
            .clip
            .is_some_and(|clip| ancestor_ids.contains(&clip))
        {
            chunk.properties.clip = None;
        }
    }
    artifact
        .clip_nodes
        .retain(|snapshot| !ancestor_ids.contains(&snapshot.id));
    let local_ids = artifact
        .clip_nodes
        .iter()
        .map(|snapshot| snapshot.id)
        .collect::<FxHashSet<_>>();
    for snapshot in &mut artifact.clip_nodes {
        if !local_owners.contains(&snapshot.owner) {
            return Err(FramePaintPlanError {
                reasons: vec![FramePaintPlanRejection::InvalidClipChain(snapshot.owner)],
            });
        }
        if snapshot
            .parent
            .is_some_and(|parent| ancestor_ids.contains(&parent))
        {
            snapshot.parent = None;
        } else if snapshot
            .parent
            .is_some_and(|parent| !local_ids.contains(&parent))
        {
            return Err(FramePaintPlanError {
                reasons: vec![FramePaintPlanRejection::InvalidClipChain(snapshot.owner)],
            });
        }
    }
    if artifact.chunks.iter().any(|chunk| {
        chunk
            .properties
            .clip
            .is_some_and(|clip| !local_ids.contains(&clip))
    }) {
        return Err(property_scene_error());
    }
    Ok(artifact)
}

fn property_scene_error() -> FramePaintPlanError {
    FramePaintPlanError {
        reasons: vec![FramePaintPlanRejection::InvalidPropertyScene],
    }
}

fn property_effect_scaffold_is_canonical(
    plan: &FramePaintPlan,
    seal: &PropertyScenePlanSeal,
    scaffold: &PropertyEffectSceneScaffold,
) -> bool {
    let production_spans = scaffold.production_root_step_spans.as_ref();
    if scaffold.production_root_step_spans != scaffold.planned_production_root_step_spans
        || (!seal.surfaces.is_empty())
        || production_spans.is_none()
            && (!plan.steps.is_empty()
                || seal.surface_count != 0
                || !seal.scene_artifact_validation.is_empty()
                || seal.aggregate_opaque_order_span != (0..0))
        || production_spans.is_some()
            && (seal.surface_count != scaffold.surfaces.len()
                || seal.aggregate_opaque_order_span.start != 0)
        || scaffold.roots.is_empty()
        || scaffold.surfaces.is_empty()
        || seal.context != scaffold.context
        || seal.outer_scissor_rect != scaffold.outer_scissor_rect
        || scaffold.context != scaffold.planned_context
        || scaffold.outer_scissor_rect != scaffold.planned_outer_scissor_rect
        || seal.context.outer_scissor_rect() != seal.outer_scissor_rect
        || scaffold.roots != scaffold.planned_roots
        || scaffold.surfaces != scaffold.planned_surfaces
        || scaffold.clip_forest != scaffold.planned_clip_forest
        || plan.property_scene_roots.as_ref() != Some(&seal.roots)
        || seal.roots.len() != scaffold.roots.len()
    {
        return false;
    }
    let mut next_boundary = 0_u32;
    let mut root_keys = FxHashSet::default();
    let mut root_stable_ids = FxHashSet::default();
    for (ordinal, (root, plan_root)) in scaffold.roots.iter().zip(&seal.roots).enumerate() {
        if root.ordinal as usize != ordinal
            || plan_root.ordinal != root.ordinal
            || plan_root.root != root.root
            || plan_root.stable_id != root.stable_id
            || plan_root.owner
                != (PaintOwnerSnapshot {
                    owner: root.root,
                    parent: None,
                })
            || plan_root.top_level_step_span != (0..0)
            || root.stable_id == 0
            || root.boundary_ordinal_span.start != next_boundary
            || root.boundary_ordinal_span.end < root.boundary_ordinal_span.start
            || root.boundary_ordinal_span.end as usize > scaffold.surfaces.len()
            || !root_keys.insert(root.root)
            || !root_stable_ids.insert(root.stable_id)
        {
            return false;
        }
        next_boundary = root.boundary_ordinal_span.end;
    }
    if next_boundary as usize != scaffold.surfaces.len() {
        return false;
    }

    let mut clip_nodes = FxHashMap::default();
    for snapshot in &scaffold.clip_forest.nodes {
        if snapshot.id.owner != snapshot.owner
            || snapshot.generation == 0
            || !matches!(
                (snapshot.id.role, snapshot.behavior),
                (ClipNodeRole::SelfClip, ClipBehavior::Replace)
                    | (ClipNodeRole::ContentsClip, ClipBehavior::Intersect)
            )
            || clip_nodes.insert(snapshot.id, *snapshot).is_some()
        {
            return false;
        }
    }
    if clip_nodes.values().any(|snapshot| {
        snapshot
            .parent
            .is_some_and(|parent| !clip_nodes.contains_key(&parent))
            || frozen_clip_chain(Some(snapshot.id), &clip_nodes).is_none()
    }) {
        return false;
    }
    let mut clip_states: FxHashMap<NodeKey, PropertyEffectClipStateWitness> = FxHashMap::default();
    let mut clip_state_stable_ids = FxHashSet::default();
    let mut referenced_clip_nodes = FxHashSet::default();
    for state in &scaffold.clip_forest.states {
        if state.stable_id == 0
            || !clip_state_stable_ids.insert(state.stable_id)
            || clip_states.contains_key(&state.owner)
        {
            return false;
        }
        let inherited = match state.parent {
            Some(parent) => {
                let Some(parent) = clip_states.get(&parent) else {
                    return false;
                };
                parent.descendants_leaf
            }
            None => {
                if scaffold
                    .roots
                    .iter()
                    .find(|root| root.root == state.owner)
                    .is_none_or(|root| root.stable_id != state.stable_id)
                {
                    return false;
                }
                None
            }
        };
        if state.paint_leaf != inherited {
            let own_self = ClipNodeId {
                owner: state.owner,
                role: ClipNodeRole::SelfClip,
            };
            if state.paint_leaf != Some(own_self)
                || clip_nodes
                    .get(&own_self)
                    .is_none_or(|snapshot| snapshot.parent != inherited)
            {
                return false;
            }
        }
        if state.descendants_leaf != state.paint_leaf {
            let own_contents = ClipNodeId {
                owner: state.owner,
                role: ClipNodeRole::ContentsClip,
            };
            if state.descendants_leaf != Some(own_contents)
                || clip_nodes
                    .get(&own_contents)
                    .is_none_or(|snapshot| snapshot.parent != state.paint_leaf)
            {
                return false;
            }
        }
        for leaf in [state.paint_leaf, state.descendants_leaf] {
            let Some(chain) = frozen_clip_chain(leaf, &clip_nodes) else {
                return false;
            };
            referenced_clip_nodes.extend(chain.into_iter().map(|snapshot| snapshot.id));
        }
        clip_states.insert(state.owner, *state);
    }
    if clip_states.len() != scaffold.clip_forest.states.len()
        || referenced_clip_nodes.len() != clip_nodes.len()
        || clip_nodes
            .keys()
            .any(|id| !referenced_clip_nodes.contains(id))
    {
        return false;
    }

    let mut owners = FxHashSet::default();
    let mut stable_ids = FxHashSet::default();
    for (ordinal, surface) in scaffold.surfaces.iter().enumerate() {
        if surface.ordinal as usize != ordinal
            || surface.stable_id == 0
            || !owners.insert(surface.boundary.owner())
            || !stable_ids.insert(surface.stable_id)
            || clip_states
                .get(&surface.boundary.owner())
                .is_none_or(|state| state.stable_id != surface.stable_id)
            || surface.scene_root_ordinal as usize >= scaffold.roots.len()
            || !scaffold.roots[surface.scene_root_ordinal as usize]
                .boundary_ordinal_span
                .contains(&surface.ordinal)
            || surface
                .parent_boundary_ordinal
                .is_some_and(|parent| parent >= surface.ordinal)
        {
            return false;
        }
        if let Some(parent) = surface.parent_boundary_ordinal {
            if scaffold.surfaces[parent as usize].scene_root_ordinal != surface.scene_root_ordinal {
                return false;
            }
        }
        match (&surface.boundary, &surface.kind) {
            (
                PropertyBoundaryId::Transform(id),
                PropertyEffectSurfaceKind::Transform {
                    snapshot,
                    nested_effect_dependencies,
                },
            ) => {
                if snapshot.id != *id
                    || snapshot.owner != id.0
                    || snapshot.generation == 0
                    || snapshot.parent.is_some()
                    || surface.parent_boundary_ordinal.is_some()
                    || surface.boundary.owner()
                        != scaffold.roots[surface.scene_root_ordinal as usize].root
                    || !matrix_is_finite_affine(snapshot.viewport_matrix)
                {
                    return false;
                }
                let expected_dependencies = scaffold
                    .surfaces
                    .iter()
                    .filter_map(|child| {
                        (child.parent_boundary_ordinal == Some(surface.ordinal)).then_some(child)
                    })
                    .filter_map(|child| {
                        let PropertyEffectSurfaceKind::Isolation(child_isolation) = &child.kind
                        else {
                            return None;
                        };
                        Some(PropertyIsolationNestedDependencySpec {
                            child_boundary_ordinal: child.ordinal,
                            child_effect: child_isolation.effect_chain.isolated_leaf.id,
                            child_stable_id: child.stable_id,
                            child_opacity_bits: child_isolation.composite.opacity_bits,
                            child_effect_generation: child_isolation.composite.effect_generation,
                            child_rect_bits: child_isolation.composite.rect_bits,
                            child_raster_identity: Box::new(
                                child_isolation.raster_identity.clone(),
                            ),
                        })
                    })
                    .collect::<Vec<_>>();
                if *nested_effect_dependencies != expected_dependencies {
                    return false;
                }
            }
            (PropertyBoundaryId::Effect(id), PropertyEffectSurfaceKind::Isolation(isolation)) => {
                let chain = &isolation.effect_chain;
                let Some(leaf) = chain.live_leaf_to_root.first() else {
                    return false;
                };
                if leaf.id != *id
                    || leaf.owner != id.0
                    || leaf.generation == 0
                    || !leaf.opacity.is_finite()
                    || !(0.0..=1.0).contains(&leaf.opacity)
                    || chain.isolated_leaf
                        != (EffectNodeSnapshot {
                            parent: None,
                            ..*leaf
                        })
                    || chain.detached_ancestors != chain.live_leaf_to_root[1..]
                    || isolation.composite.opacity_bits != leaf.opacity.to_bits()
                    || isolation.composite.effect_generation != leaf.generation
                    || isolation.composite.rect_bits != isolation.raster_space.source_bounds_bits
                    || isolation.parent_opaque_cursor_delta != 0
                    || isolation.raster_identity.boundary != *id
                    || isolation.raster_identity.stable_id != surface.stable_id
                    || isolation.raster_identity.raster_space != isolation.raster_space
                    || isolation.raster_identity.local_raster_clips != isolation.local_raster_clips
                    || isolation.raster_identity.nested_dependencies
                        != isolation.nested_dependencies
                    || isolation.raster_identity.content.is_empty()
                {
                    return false;
                }
                let mut chain_ids = FxHashSet::default();
                for (index, effect) in chain.live_leaf_to_root.iter().enumerate() {
                    if effect.id.0 != effect.owner
                        || effect.generation == 0
                        || !effect.opacity.is_finite()
                        || !(0.0..=1.0).contains(&effect.opacity)
                        || !chain_ids.insert(effect.id)
                        || effect.parent
                            != chain.live_leaf_to_root.get(index + 1).map(|next| next.id)
                    {
                        return false;
                    }
                }
                let bounds = isolation
                    .raster_space
                    .source_bounds_bits
                    .map(f32::from_bits);
                if bounds.iter().any(|value| !value.is_finite())
                    || bounds[0] < 0.0
                    || bounds[1] < 0.0
                    || bounds[2] <= 0.0
                    || bounds[3] <= 0.0
                    || isolation
                        .raster_space
                        .paint_offset_bits
                        .map(f32::from_bits)
                        .iter()
                        .any(|value| !value.is_finite())
                {
                    return false;
                }
                let expected_basis = match surface.parent_boundary_ordinal {
                    None => PropertyIsolationCompositeBasis::FrameRoot,
                    Some(parent) => match &scaffold.surfaces[parent as usize].kind {
                        PropertyEffectSurfaceKind::Transform { snapshot, .. } => {
                            PropertyIsolationCompositeBasis::ParentTransform {
                                transform: snapshot.id,
                                viewport_matrix_bits: snapshot
                                    .viewport_matrix
                                    .to_cols_array()
                                    .map(f32::to_bits),
                            }
                        }
                        PropertyEffectSurfaceKind::Isolation(parent) => {
                            PropertyIsolationCompositeBasis::ParentEffect(
                                parent.effect_chain.isolated_leaf.id,
                            )
                        }
                    },
                };
                let expected_effect_parent = surface.parent_boundary_ordinal.and_then(|parent| {
                    match &scaffold.surfaces[parent as usize].kind {
                        PropertyEffectSurfaceKind::Isolation(parent) => {
                            Some(parent.effect_chain.isolated_leaf.id)
                        }
                        PropertyEffectSurfaceKind::Transform { .. } => None,
                    }
                });
                let mut expected_detached_ancestors = Vec::new();
                let mut parent_cursor = surface.parent_boundary_ordinal;
                while let Some(parent) = parent_cursor {
                    let parent_surface = &scaffold.surfaces[parent as usize];
                    match &parent_surface.kind {
                        PropertyEffectSurfaceKind::Isolation(parent) => {
                            let mut snapshot = parent.effect_chain.isolated_leaf;
                            snapshot.parent = parent.effect_chain.live_leaf_to_root[0].parent;
                            expected_detached_ancestors.push(snapshot);
                        }
                        PropertyEffectSurfaceKind::Transform { .. } => {}
                    }
                    parent_cursor = parent_surface.parent_boundary_ordinal;
                }
                let Some(clip_state) = clip_states.get(&surface.boundary.owner()) else {
                    return false;
                };
                let own_self = ClipNodeId {
                    owner: surface.boundary.owner(),
                    role: ClipNodeRole::SelfClip,
                };
                let ancestor_leaf = if clip_state.paint_leaf == Some(own_self) {
                    let Some(snapshot) = clip_nodes.get(&own_self) else {
                        return false;
                    };
                    snapshot.parent
                } else {
                    clip_state.paint_leaf
                };
                let Some(full_clip_chain) = frozen_clip_chain(clip_state.paint_leaf, &clip_nodes)
                else {
                    return false;
                };
                let Some(expected_ancestor_clips) = frozen_clip_chain(ancestor_leaf, &clip_nodes)
                else {
                    return false;
                };
                if full_clip_chain.len() < expected_ancestor_clips.len()
                    || full_clip_chain[full_clip_chain.len() - expected_ancestor_clips.len()..]
                        != expected_ancestor_clips
                {
                    return false;
                }
                let expected_local_clips =
                    &full_clip_chain[..full_clip_chain.len() - expected_ancestor_clips.len()];
                if isolation.composite.basis != expected_basis
                    || leaf.parent != expected_effect_parent
                    || chain.detached_ancestors != expected_detached_ancestors
                    || isolation.local_raster_clips != expected_local_clips
                    || isolation.ancestor_composite_clips != expected_ancestor_clips
                    || resolve_composite_scissor(
                        seal.outer_scissor_rect,
                        &isolation.ancestor_composite_clips,
                    )
                    .ok()
                        != Some(isolation.composite.resolved_scissor)
                {
                    return false;
                }
                let local_ids = isolation
                    .local_raster_clips
                    .iter()
                    .map(|clip| clip.id)
                    .collect::<FxHashSet<_>>();
                let ancestor_ids = isolation
                    .ancestor_composite_clips
                    .iter()
                    .map(|clip| clip.id)
                    .collect::<FxHashSet<_>>();
                let content = &isolation.raster_identity.content;
                let mut content_owners = FxHashSet::default();
                let mut content_stable_ids = FxHashSet::default();
                for (index, entry) in content.iter().enumerate() {
                    if entry.stable_id == 0
                        || entry.self_paint_revision == 0
                        || entry.topology_revision == 0
                        || !content_stable_ids.insert(entry.stable_id)
                    {
                        return false;
                    }
                    if index == 0 {
                        if entry.owner != surface.boundary.owner()
                            || entry.stable_id != surface.stable_id
                            || entry.parent.is_some()
                        {
                            return false;
                        }
                    } else if entry.parent.is_none_or(|parent| {
                        parent == entry.owner || !content_owners.contains(&parent)
                    }) {
                        return false;
                    }
                    if !content_owners.insert(entry.owner) {
                        return false;
                    }
                }
                if !local_ids.is_disjoint(&ancestor_ids)
                    || isolation
                        .local_raster_clips
                        .iter()
                        .chain(&isolation.ancestor_composite_clips)
                        .any(|clip| clip.id.owner != clip.owner || clip.generation == 0)
                {
                    return false;
                }
                let expected_dependencies = scaffold
                    .surfaces
                    .iter()
                    .filter_map(|child| {
                        (child.parent_boundary_ordinal == Some(surface.ordinal)).then_some(child)
                    })
                    .filter_map(|child| {
                        let PropertyEffectSurfaceKind::Isolation(child_isolation) = &child.kind
                        else {
                            return None;
                        };
                        Some(PropertyIsolationNestedDependencySpec {
                            child_boundary_ordinal: child.ordinal,
                            child_effect: child_isolation.effect_chain.isolated_leaf.id,
                            child_stable_id: child.stable_id,
                            child_opacity_bits: child_isolation.composite.opacity_bits,
                            child_effect_generation: child_isolation.composite.effect_generation,
                            child_rect_bits: child_isolation.composite.rect_bits,
                            child_raster_identity: Box::new(
                                child_isolation.raster_identity.clone(),
                            ),
                        })
                    })
                    .collect::<Vec<_>>();
                if isolation.nested_dependencies != expected_dependencies {
                    return false;
                }
            }
            _ => return false,
        }
    }
    production_spans.is_none_or(|spans| {
        property_effect_production_plan_is_canonical(plan, seal, scaffold, spans)
    })
}

fn property_effect_production_plan_is_canonical(
    plan: &FramePaintPlan,
    seal: &PropertyScenePlanSeal,
    scaffold: &PropertyEffectSceneScaffold,
    root_spans: &[Range<usize>],
) -> bool {
    fn validate_surface(
        surface: &RetainedSurfacePlan,
        scaffold: &PropertyEffectSceneScaffold,
        expected_ordinal: u32,
        seen: &mut FxHashSet<u32>,
    ) -> Option<u32> {
        let contract = scaffold.surfaces.get(expected_ordinal as usize)?;
        if !seen.insert(expected_ordinal)
            || surface.boundary_root != contract.boundary.owner()
            || surface.stable_id != contract.stable_id
            || surface.parent_surface
                != contract
                    .parent_boundary_ordinal
                    .map(|parent| scaffold.surfaces[parent as usize].boundary.owner())
        {
            return None;
        }
        match (&contract.kind, &surface.kind) {
            (
                PropertyEffectSurfaceKind::Transform { snapshot, .. },
                SurfaceKind::Transform(plan),
            ) => {
                if plan.transform != snapshot.id
                    || snapshot.owner != surface.boundary_root
                    || surface.persistent_color_key
                        != crate::view::base_component::transformed_layer_stable_key(
                            surface.stable_id,
                        )
                    || !surface.matches_frozen_witness()
                    || plan.context != scaffold.context
                {
                    return None;
                }
            }
            (
                PropertyEffectSurfaceKind::Isolation(isolation),
                SurfaceKind::NestedIsolation(plan),
            ) => {
                let artifact_contract = plan.property_scene_artifact.as_ref()?;
                let expected_content = isolation
                    .raster_identity
                    .content
                    .iter()
                    .map(|entry| EffectPropertyContentWitness {
                        owner: entry.owner,
                        stable_id: entry.stable_id,
                        parent: entry.parent,
                        self_paint_revision: entry.self_paint_revision,
                        topology_revision: entry.topology_revision,
                    })
                    .collect::<Vec<_>>();
                if plan.effect != isolation.effect_chain.isolated_leaf
                    || plan.property_scene.as_ref() != Some(isolation)
                    || artifact_contract.boundary_root() != surface.boundary_root
                    || artifact_contract.stable_id() != surface.stable_id
                    || artifact_contract.isolated_leaf() != isolation.effect_chain.isolated_leaf
                    || artifact_contract.live_effect_chain()
                        != isolation.effect_chain.live_leaf_to_root
                    || artifact_contract.detached_ancestors()
                        != isolation.effect_chain.detached_ancestors
                    || artifact_contract.local_raster_clips() != isolation.local_raster_clips
                    || artifact_contract.detached_ancestor_clips()
                        != isolation.ancestor_composite_clips
                    || artifact_contract.content() != expected_content
                    || surface.persistent_color_key
                        != crate::view::base_component::isolation_layer_stable_key(
                            surface.stable_id,
                        )
                    || !surface.matches_frozen_witness()
                    || plan.geometry.source_bounds.x.to_bits()
                        != isolation.raster_space.source_bounds_bits[0]
                    || plan.geometry.source_bounds.y.to_bits()
                        != isolation.raster_space.source_bounds_bits[1]
                    || plan.geometry.source_bounds.width.to_bits()
                        != isolation.raster_space.source_bounds_bits[2]
                    || plan.geometry.source_bounds.height.to_bits()
                        != isolation.raster_space.source_bounds_bits[3]
                {
                    return None;
                }
            }
            _ => return None,
        }

        let expected_children = scaffold
            .surfaces
            .iter()
            .filter(|child| child.parent_boundary_ordinal == Some(expected_ordinal))
            .map(|child| child.ordinal)
            .collect::<Vec<_>>();
        let mut child_index = 0usize;
        let mut cursor = 0_u32;
        for step in &surface.raster_steps {
            match step {
                PaintPlanStep::ArtifactSpan(span) => {
                    if span.opaque_order_span.start != cursor
                        || span.opaque_order_span.end
                            != cursor.checked_add(opaque_order_count(&span.artifact))?
                    {
                        return None;
                    }
                    let valid = match &surface.kind {
                        SurfaceKind::Transform(plan) => {
                            super::compiler::validate_transform_property_surface_artifact(
                                &span.artifact,
                                surface.boundary_root,
                                plan.transform,
                            )
                            .is_some()
                        }
                        SurfaceKind::NestedIsolation(plan) => plan
                            .property_scene_artifact
                            .as_ref()
                            .is_some_and(|artifact_contract| {
                                super::compiler::validate_effect_property_surface_artifact(
                                    &span.artifact,
                                    artifact_contract,
                                )
                                .is_some()
                            }),
                        SurfaceKind::Isolation(_) | SurfaceKind::ScrollHost(_) => false,
                    };
                    if !valid {
                        return None;
                    }
                    cursor = span.opaque_order_span.end;
                }
                PaintPlanStep::RetainedSurface(child) => {
                    let expected_child = *expected_children.get(child_index)?;
                    child_index += 1;
                    validate_surface(child, scaffold, expected_child, seen)?;
                    // Effect children are translucent composition and cannot
                    // advance this surface's opaque cursor.
                }
            }
        }
        (child_index == expected_children.len()
            && surface.aggregate_opaque_order_span == (0..cursor))
            .then_some(cursor)
    }

    if root_spans.len() != scaffold.roots.len()
        || seal.surface_count != scaffold.surfaces.len()
        || seal.scene_artifact_validation.len()
            != plan
                .steps
                .iter()
                .filter(|step| matches!(step, PaintPlanStep::ArtifactSpan(_)))
                .count()
    {
        return false;
    }
    let mut next_step = 0usize;
    let mut scene_cursor = 0_u32;
    let mut artifact_index = 0usize;
    let mut seen = FxHashSet::default();
    for (root_ordinal, span) in root_spans.iter().enumerate() {
        if span.start != next_step || span.end < span.start || span.end > plan.steps.len() {
            return false;
        }
        for step_index in span.clone() {
            match &plan.steps[step_index] {
                PaintPlanStep::ArtifactSpan(span) => {
                    if span.opaque_order_span.start != scene_cursor
                        || span.opaque_order_span.end
                            != scene_cursor
                                .checked_add(opaque_order_count(&span.artifact))
                                .unwrap_or(u32::MAX)
                        || super::compiler::validate_property_scene_artifact_for_plan(
                            &span.artifact,
                        ) != seal.scene_artifact_validation.get(artifact_index).cloned()
                    {
                        return false;
                    }
                    artifact_index += 1;
                    scene_cursor = span.opaque_order_span.end;
                }
                PaintPlanStep::RetainedSurface(surface) => {
                    let Some(expected) = scaffold.surfaces.iter().find(|contract| {
                        contract.parent_boundary_ordinal.is_none()
                            && contract.scene_root_ordinal as usize == root_ordinal
                            && contract.boundary.owner() == surface.boundary_root
                    }) else {
                        return false;
                    };
                    let Some(terminal) =
                        validate_surface(surface, scaffold, expected.ordinal, &mut seen)
                    else {
                        return false;
                    };
                    if matches!(surface.kind, SurfaceKind::Transform(_)) {
                        scene_cursor = scene_cursor.max(terminal);
                    }
                }
            }
        }
        next_step = span.end;
    }
    next_step == plan.steps.len()
        && artifact_index == seal.scene_artifact_validation.len()
        && seen.len() == scaffold.surfaces.len()
        && seal.aggregate_opaque_order_span == (0..scene_cursor)
}

fn property_scroll_schedule_scaffold_is_canonical(
    plan: &FramePaintPlan,
    seal: &PropertyScenePlanSeal,
    scaffold: &PropertyScrollScheduleScaffold,
) -> bool {
    if !plan.steps.is_empty()
        || seal.surface_count != 0
        || !seal.surfaces.is_empty()
        || !seal.scene_artifact_validation.is_empty()
        || seal.aggregate_opaque_order_span != (0..0)
        || seal.context != scaffold.context
        || seal.outer_scissor_rect != scaffold.context.outer_scissor_rect()
        || scaffold.context != scaffold.planned_context
        || scaffold.roots != scaffold.planned_roots
        || scaffold.schedule != scaffold.planned_schedule
        || scaffold.boundaries != scaffold.planned_boundaries
        || scaffold.receiver_insertions != scaffold.planned_receiver_insertions
        || scaffold.effect_receiver_insertions != scaffold.planned_effect_receiver_insertions
        || scaffold.transform_effect_receiver_insertions
            != scaffold.planned_transform_effect_receiver_insertions
        || scaffold.roots.is_empty()
        || scaffold.boundaries.len() != scaffold.roots.len()
    {
        return false;
    }
    let Some(plan_roots) = &plan.property_scene_roots else {
        return false;
    };
    if plan_roots.len() != scaffold.roots.len() || seal.roots != *plan_roots {
        return false;
    }
    let mut next_step = 0usize;
    let mut root_keys = FxHashSet::default();
    let mut stable_ids = FxHashSet::default();
    let mut scroll_ids = FxHashSet::default();
    for (ordinal, root) in scaffold.roots.iter().enumerate() {
        let Some(plan_root) = plan_roots.get(ordinal) else {
            return false;
        };
        if root.ordinal as usize != ordinal
            || root.stable_id == 0
            || !root_keys.insert(root.root)
            || !stable_ids.insert(root.stable_id)
            || root.step_span.start != next_step
            || root.step_span.end <= root.step_span.start
            || root.step_span.end > scaffold.schedule.steps.len()
            || plan_root.ordinal != root.ordinal
            || plan_root.root != root.root
            || plan_root.stable_id != root.stable_id
            || plan_root.owner
                != (PaintOwnerSnapshot {
                    owner: root.root,
                    parent: None,
                })
            || plan_root.top_level_step_span != (0..0)
            || seal.roots.get(ordinal) != Some(plan_root)
            || !property_scroll_root_schedule_is_supported(
                &scaffold.schedule.steps[root.step_span.clone()],
            )
        {
            return false;
        }
        let root_boundaries = scaffold
            .boundaries
            .iter()
            .filter(|boundary| boundary.scene_root_ordinal as usize == ordinal)
            .collect::<Vec<_>>();
        let [boundary] = root_boundaries.as_slice() else {
            return false;
        };
        if boundary.ordinal as usize >= scaffold.boundaries.len()
            || scaffold.boundaries.get(boundary.ordinal as usize) != Some(*boundary)
            || !scroll_ids.insert(boundary.scroll.id)
        {
            return false;
        }
        let Some(scroll_step) = scaffold.schedule.steps[root.step_span.clone()]
            .iter()
            .find(|step| matches!(step, PropertySceneScheduledStep::ScrollBoundary { .. }))
        else {
            return false;
        };
        let PropertySceneScheduledStep::ScrollBoundary {
            boundary_ordinal,
            scroll,
            basis,
            phase,
        } = scroll_step
        else {
            return false;
        };
        if *boundary_ordinal != boundary.ordinal
            || *scroll != boundary.scroll.id
            || basis != &boundary.basis
            || phase != &boundary.phase
            || boundary.scroll.owner != boundary.scroll.id.0
            || boundary.scroll.parent.is_some()
            || boundary.scroll.generation == 0
            || boundary.contents_clip.id.owner != boundary.scroll.owner
            || boundary.contents_clip.id.role != ClipNodeRole::ContentsClip
            || boundary.contents_clip.owner != boundary.scroll.owner
            || boundary.contents_clip.parent.is_some()
            || boundary.contents_clip.behavior != ClipBehavior::Intersect
            || boundary.contents_clip.generation == 0
            || (!boundary
                .scroll
                .is_canonical_with_contents_clip(boundary.contents_clip)
                && !boundary
                    .scroll
                    .is_canonical_painted_with_contents_clip(boundary.contents_clip))
        {
            return false;
        }
        let stack = &boundary.consumed_properties;
        if stack.target_owner != boundary.scroll.owner
            || stack.entries.is_empty()
            || stack.entries.first().map(|entry| entry.expected_before) != Some(stack.live_input)
            || stack.entries.last().map(|entry| entry.projected_after)
                != Some(stack.projected_output)
            || stack.projected_output != PropertyTreeState::default()
        {
            return false;
        }
        let mut cursor = stack.live_input;
        for entry in &stack.entries {
            if entry.expected_before != cursor {
                return false;
            }
            match entry.boundary {
                ConsumedPropertyBoundary::Transform(id) if cursor.transform == Some(id) => {
                    cursor.transform = None;
                }
                ConsumedPropertyBoundary::Effect(id) if cursor.effect == Some(id) => {
                    cursor.effect = None;
                }
                ConsumedPropertyBoundary::ScrollContents {
                    scroll,
                    contents_clip,
                } if cursor.scroll == Some(scroll)
                    && cursor.clip == Some(contents_clip)
                    && scroll == boundary.scroll.id
                    && contents_clip == boundary.contents_clip.id =>
                {
                    cursor.scroll = None;
                    cursor.clip = boundary.contents_clip.parent;
                }
                _ => return false,
            }
            if entry.projected_after != cursor {
                return false;
            }
        }
        if cursor != stack.projected_output
            || boundary.phase.host_before
                != (PropertyScrollPhaseSlot {
                    owner: boundary.scroll.owner,
                    phase: PropertyScrollPhaseKind::HostBeforeChildren,
                    receiver_state: cursor,
                })
            || boundary.phase.content_gap
                != (PropertyScrollContentPhase {
                    owner: boundary.scroll.owner,
                    phase: PropertyScrollPhaseKind::DetachedContentComposite,
                    content_state: stack.live_input,
                    projected_receiver_state: cursor,
                })
            || boundary.phase.overlay_after
                != (PropertyScrollPhaseSlot {
                    owner: boundary.scroll.owner,
                    phase: PropertyScrollPhaseKind::OverlayAfterChildren,
                    receiver_state: cursor,
                })
        {
            return false;
        }
        next_step = root.step_span.end;
    }
    let eligible_receiver_insertions = scaffold
        .roots
        .iter()
        .filter(|root| {
            matches!(
                &scaffold.schedule.steps[root.step_span.clone()],
                [
                    PropertySceneScheduledStep::RetainedSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Transform(_),
                        parent: None,
                    },
                    PropertySceneScheduledStep::ScrollBoundary {
                        basis: ScrollCompositeBasis::Transform(_),
                        ..
                    },
                ]
            )
        })
        .count();
    let mut insertion_receivers = FxHashSet::default();
    let mut insertion_boundaries = FxHashSet::default();
    let insertions_are_canonical = scaffold.receiver_insertions.iter().all(|insertion| {
        insertion_receivers.insert(insertion.receiver.id)
            && insertion_boundaries.insert(insertion.scroll_boundary_ordinal)
            && property_scroll_receiver_insertion_is_canonical(scaffold, insertion)
    });
    let eligible_effect_receiver_insertions = scaffold
        .roots
        .iter()
        .filter(|root| {
            matches!(
                &scaffold.schedule.steps[root.step_span.clone()],
                [
                    PropertySceneScheduledStep::RetainedSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Effect(_),
                        parent: None,
                    },
                    PropertySceneScheduledStep::ScrollBoundary {
                        basis: ScrollCompositeBasis::Effect(_),
                        ..
                    },
                ]
            )
        })
        .count();
    let mut effect_insertion_receivers = FxHashSet::default();
    let mut effect_insertion_boundaries = FxHashSet::default();
    let effect_insertions_are_canonical =
        scaffold.effect_receiver_insertions.iter().all(|insertion| {
            effect_insertion_receivers.insert(insertion.receiver.id)
                && effect_insertion_boundaries.insert(insertion.scroll_boundary_ordinal)
                && property_effect_scroll_receiver_insertion_is_canonical(scaffold, insertion)
        });
    let eligible_transform_effect_insertions = scaffold
        .roots
        .iter()
        .filter(|root| {
            matches!(
                &scaffold.schedule.steps[root.step_span.clone()],
                [
                    PropertySceneScheduledStep::RetainedSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Transform(_),
                        parent: None,
                    },
                    PropertySceneScheduledStep::RetainedSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Effect(_),
                        parent: Some(PropertyScheduledSurfaceBoundaryId::Transform(_)),
                    },
                    PropertySceneScheduledStep::ScrollBoundary {
                        basis: ScrollCompositeBasis::Effect(_),
                        ..
                    },
                ]
            )
        })
        .count();
    let mut transform_effect_outer_receivers = FxHashSet::default();
    let mut transform_effect_inner_receivers = FxHashSet::default();
    let mut transform_effect_boundaries = FxHashSet::default();
    let transform_effect_insertions_are_canonical = scaffold
        .transform_effect_receiver_insertions
        .iter()
        .all(|insertion| {
            transform_effect_outer_receivers.insert(insertion.outer_receiver.id)
                && transform_effect_inner_receivers.insert(insertion.inner.receiver.id)
                && transform_effect_boundaries.insert(insertion.inner.scroll_boundary_ordinal)
                && property_transform_effect_scroll_receiver_insertion_is_canonical(
                    scaffold, insertion,
                )
        });
    next_step == scaffold.schedule.steps.len()
        && scaffold
            .boundaries
            .iter()
            .enumerate()
            .all(|(ordinal, boundary)| boundary.ordinal as usize == ordinal)
        && scaffold.receiver_insertions.len() <= eligible_receiver_insertions
        && insertions_are_canonical
        && scaffold.effect_receiver_insertions.len() <= eligible_effect_receiver_insertions
        && effect_insertions_are_canonical
        && scaffold.transform_effect_receiver_insertions.len()
            == eligible_transform_effect_insertions
        && transform_effect_insertions_are_canonical
}

fn property_transform_effect_scroll_receiver_insertion_is_canonical(
    scaffold: &PropertyScrollScheduleScaffold,
    insertion: &PropertyTransformEffectScrollReceiverInsertionContract,
) -> bool {
    let Some(root) = scaffold.roots.get(insertion.scene_root_ordinal as usize) else {
        return false;
    };
    let Some(root_steps) = scaffold.schedule.steps.get(root.step_span.clone()) else {
        return false;
    };
    let [
        PropertySceneScheduledStep::RetainedSurface {
            boundary: PropertyScheduledSurfaceBoundary::Transform(outer),
            parent: None,
        },
        PropertySceneScheduledStep::RetainedSurface {
            boundary: PropertyScheduledSurfaceBoundary::Effect(inner),
            parent: Some(PropertyScheduledSurfaceBoundaryId::Transform(parent)),
        },
        PropertySceneScheduledStep::ScrollBoundary {
            boundary_ordinal,
            scroll,
            basis: ScrollCompositeBasis::Effect(basis),
            ..
        },
    ] = root_steps
    else {
        return false;
    };
    let Some(boundary) = scaffold.boundaries.get(*boundary_ordinal as usize) else {
        return false;
    };
    let expected_outer_bounds = recorded_step_bounds_union(
        insertion
            .outer_recorded_steps
            .iter()
            .filter_map(|step| match step {
                PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact) => {
                    Some(artifact.chunks.iter().map(|chunk| {
                        let [x, y, width, height] = chunk.bounds_bits.map(f32::from_bits);
                        crate::view::base_component::Rect {
                            x,
                            y,
                            width,
                            height,
                        }
                    }))
                }
                PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(_) => None,
            }),
        insertion.inner.raster_bounds_bits.map(f32::from_bits),
    );
    let actual_outer_bounds = [
        insertion.outer_geometry.source_bounds.x,
        insertion.outer_geometry.source_bounds.y,
        insertion.outer_geometry.source_bounds.width,
        insertion.outer_geometry.source_bounds.height,
    ]
    .map(f32::to_bits);
    if insertion.outer_receiver != *outer
        || outer.id != *parent
        || insertion.inner.receiver != *inner
        || insertion.inner.receiver != *basis
        || insertion.outer_receiver.owner != root.root
        || insertion.outer_receiver.parent.is_some()
        || insertion.outer_stable_id != root.stable_id
        || insertion.effect_cutout.root != inner.owner
        || insertion.effect_cutout.stable_id != insertion.inner.receiver_stable_id
        || !matches!(insertion.effect_cutout.kind, super::PlannedBoundaryKind::Isolation(id) if id == inner.id)
        || insertion.inner.scene_root_ordinal != insertion.scene_root_ordinal
        || insertion.inner.scroll_boundary_ordinal != *boundary_ordinal
        || !insertion.outer_geometry.matches_rebuilt_contract()
        || expected_outer_bounds != Some(actual_outer_bounds)
        || insertion
            .outer_geometry
            .viewport_transform
            .to_cols_array()
            .map(f32::to_bits)
            != insertion
                .outer_receiver
                .viewport_matrix
                .to_cols_array()
                .map(f32::to_bits)
        || super::compiler::direct_translation_bits(insertion.outer_geometry.viewport_transform)
            .is_none()
        || insertion.outer_insertion_index >= insertion.outer_recorded_steps.len()
        || insertion.outer_before_span != (0..insertion.outer_insertion_index)
        || insertion.outer_after_span
            != (insertion.outer_insertion_index + 1..insertion.outer_recorded_steps.len())
        || boundary.scene_root_ordinal != insertion.scene_root_ordinal
        || boundary.scroll.id != *scroll
        || boundary.contents_clip.id.owner != boundary.scroll.owner
        || boundary.consumed_properties.target_owner != boundary.scroll.owner
        || boundary.consumed_properties.projected_output != PropertyTreeState::default()
        || !matches!(
            boundary.consumed_properties.entries.as_slice(),
            [
                ConsumedPropertyEntry {
                    boundary: ConsumedPropertyBoundary::Transform(transform),
                    ..
                },
                ConsumedPropertyEntry {
                    boundary: ConsumedPropertyBoundary::Effect(effect),
                    ..
                },
                ConsumedPropertyEntry {
                    boundary: ConsumedPropertyBoundary::ScrollContents {
                        scroll: consumed_scroll,
                        contents_clip,
                    },
                    ..
                },
            ] if *transform == outer.id
                && *effect == inner.id
                && *consumed_scroll == boundary.scroll.id
                && *contents_clip == boundary.contents_clip.id
        )
        || !nested_property_effect_scroll_receiver_insertion_is_canonical(
            scaffold,
            &insertion.inner,
            *inner,
            *boundary_ordinal,
            *scroll,
        )
    {
        return false;
    }
    let mut marker_count = 0usize;
    let mut cursor = 0_u32;
    for (index, step) in insertion.outer_recorded_steps.iter().enumerate() {
        match step {
            PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact) => {
                if index == insertion.outer_insertion_index
                    || artifact.op_count
                        != artifact
                            .chunks
                            .iter()
                            .map(|chunk| chunk.op_count)
                            .sum::<usize>()
                {
                    return false;
                }
                let Some(next) = cursor.checked_add(artifact.opaque_count) else {
                    return false;
                };
                cursor = next;
            }
            PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(marker) => {
                marker_count += 1;
                if index != insertion.outer_insertion_index
                    || *marker != insertion.effect_cutout
                    || cursor != insertion.outer_opaque_before
                {
                    return false;
                }
            }
        }
    }
    marker_count == 1 && cursor == insertion.outer_opaque_after
}

/// Canonicalizes the E -> Scroll half of an exact T -> E -> Scroll insertion
/// without pretending that E is a scene root.  The legacy direct-E helper
/// intentionally retains its stricter `[E, S]` root grammar.
fn nested_property_effect_scroll_receiver_insertion_is_canonical(
    scaffold: &PropertyScrollScheduleScaffold,
    insertion: &PropertyEffectScrollReceiverInsertionContract,
    receiver: EffectNodeSnapshot,
    boundary_ordinal: u32,
    scroll: ScrollNodeId,
) -> bool {
    let Some(boundary) = scaffold.boundaries.get(boundary_ordinal as usize) else {
        return false;
    };
    if insertion.receiver != receiver
        || insertion.receiver.id.0 != insertion.receiver.owner
        || insertion.receiver.parent.is_some()
        || insertion.receiver.generation == 0
        || !insertion.receiver.opacity.is_finite()
        || !(0.0..=1.0).contains(&insertion.receiver.opacity)
        || insertion.receiver_stable_id == 0
        || insertion.artifact_contract.boundary_root() != insertion.receiver.owner
        || insertion.artifact_contract.stable_id() != insertion.receiver_stable_id
        || insertion.artifact_contract.isolated_leaf() != insertion.receiver
        || insertion.artifact_contract.content().len() != 1
        || insertion.artifact_contract.content()[0].owner != insertion.receiver.owner
        || !insertion.artifact_contract.local_raster_clips().is_empty()
        || !insertion
            .artifact_contract
            .detached_ancestor_clips()
            .is_empty()
        || insertion.artifact_contract.live_effect_chain() != [insertion.receiver]
        || insertion.scroll_boundary_ordinal != boundary_ordinal
        || scroll != boundary.scroll.id
        || insertion.scroll_cutout.root != boundary.scroll.owner
        || insertion.scroll_cutout.stable_id == 0
        || !matches!(insertion.scroll_cutout.kind, super::PlannedBoundaryKind::Scroll(id) if id == boundary.scroll.id)
        || insertion.insertion_index >= insertion.recorded_steps.len()
        || insertion.before_span != (0..insertion.insertion_index)
        || insertion.after_span != (insertion.insertion_index + 1..insertion.recorded_steps.len())
        || insertion
            .raster_bounds_bits
            .iter()
            .any(|bits| !f32::from_bits(*bits).is_finite())
        || f32::from_bits(insertion.raster_bounds_bits[2]) <= 0.0
        || f32::from_bits(insertion.raster_bounds_bits[3]) <= 0.0
        || insertion.raster_identity
            != (PropertyEffectScrollReceiverRasterIdentity {
                receiver_owner: insertion.receiver.owner,
                receiver_stable_id: insertion.receiver_stable_id,
                raster_bounds_bits: insertion.raster_bounds_bits,
                local_raster_clips: insertion.artifact_contract.local_raster_clips().to_vec(),
                content: insertion.artifact_contract.content().to_vec(),
                recorded_steps: insertion
                    .recorded_steps
                    .iter()
                    .cloned()
                    .map(|step| match step {
                        PropertyScrollReceiverRecordedStepIdentity::Artifact(mut artifact) => {
                            artifact
                                .effect_nodes
                                .retain(|effect| effect.id != insertion.receiver.id);
                            for chunk in &mut artifact.chunks {
                                if chunk.properties.effect == Some(insertion.receiver.id) {
                                    chunk.properties.effect = None;
                                }
                                chunk.content_revision.composite_revision = 0;
                            }
                            PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact)
                        }
                        marker => marker,
                    })
                    .collect(),
            })
    {
        return false;
    }
    let mut marker_count = 0usize;
    let mut cursor = 0_u32;
    for (index, step) in insertion.recorded_steps.iter().enumerate() {
        match step {
            PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact) => {
                if index == insertion.insertion_index
                    || artifact.op_count
                        != artifact
                            .chunks
                            .iter()
                            .map(|chunk| chunk.op_count)
                            .sum::<usize>()
                {
                    return false;
                }
                let Some(next) = cursor.checked_add(artifact.opaque_count) else {
                    return false;
                };
                cursor = next;
            }
            PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(marker) => {
                marker_count += 1;
                if index != insertion.insertion_index
                    || *marker != insertion.scroll_cutout
                    || cursor != insertion.receiver_opaque_before
                {
                    return false;
                }
            }
        }
    }
    marker_count == 1 && cursor == insertion.receiver_opaque_after
}

fn property_effect_scroll_receiver_insertion_is_canonical(
    scaffold: &PropertyScrollScheduleScaffold,
    insertion: &PropertyEffectScrollReceiverInsertionContract,
) -> bool {
    let Some(root) = scaffold.roots.get(insertion.scene_root_ordinal as usize) else {
        return false;
    };
    let Some(boundary) = scaffold
        .boundaries
        .get(insertion.scroll_boundary_ordinal as usize)
    else {
        return false;
    };
    let Some(root_steps) = scaffold.schedule.steps.get(root.step_span.clone()) else {
        return false;
    };
    let [
        PropertySceneScheduledStep::RetainedSurface {
            boundary: PropertyScheduledSurfaceBoundary::Effect(receiver),
            parent: None,
        },
        PropertySceneScheduledStep::ScrollBoundary {
            boundary_ordinal,
            scroll,
            basis: ScrollCompositeBasis::Effect(basis),
            ..
        },
    ] = root_steps
    else {
        return false;
    };
    if insertion.receiver != *receiver
        || insertion.receiver != *basis
        || insertion.receiver.owner != root.root
        || insertion.receiver.id.0 != root.root
        || insertion.receiver.parent.is_some()
        || insertion.receiver.generation == 0
        || !insertion.receiver.opacity.is_finite()
        || !(0.0..=1.0).contains(&insertion.receiver.opacity)
        || insertion.receiver_stable_id != root.stable_id
        || insertion.artifact_contract.boundary_root() != root.root
        || insertion.artifact_contract.stable_id() != root.stable_id
        || insertion.artifact_contract.isolated_leaf().parent.is_some()
        || insertion.artifact_contract.content().len() != 1
        || insertion.artifact_contract.content()[0].owner != root.root
        || !insertion.artifact_contract.local_raster_clips().is_empty()
        || !insertion
            .artifact_contract
            .detached_ancestor_clips()
            .is_empty()
        || insertion.artifact_contract.live_effect_chain() != [insertion.receiver]
        || *boundary_ordinal != insertion.scroll_boundary_ordinal
        || *scroll != boundary.scroll.id
        || insertion.scroll_cutout.root != boundary.scroll.owner
        || insertion.scroll_cutout.stable_id == 0
        || !matches!(insertion.scroll_cutout.kind, super::PlannedBoundaryKind::Scroll(id) if id == boundary.scroll.id)
        || insertion.insertion_index >= insertion.recorded_steps.len()
        || insertion.before_span != (0..insertion.insertion_index)
        || insertion.after_span != (insertion.insertion_index + 1..insertion.recorded_steps.len())
        || insertion
            .raster_bounds_bits
            .iter()
            .any(|bits| !f32::from_bits(*bits).is_finite())
        || f32::from_bits(insertion.raster_bounds_bits[2]) <= 0.0
        || f32::from_bits(insertion.raster_bounds_bits[3]) <= 0.0
        || insertion.raster_identity
            != (PropertyEffectScrollReceiverRasterIdentity {
                receiver_owner: insertion.receiver.owner,
                receiver_stable_id: insertion.receiver_stable_id,
                raster_bounds_bits: insertion.raster_bounds_bits,
                local_raster_clips: insertion.artifact_contract.local_raster_clips().to_vec(),
                content: insertion.artifact_contract.content().to_vec(),
                recorded_steps: insertion
                    .recorded_steps
                    .iter()
                    .cloned()
                    .map(|step| match step {
                        PropertyScrollReceiverRecordedStepIdentity::Artifact(mut artifact) => {
                            artifact
                                .effect_nodes
                                .retain(|effect| effect.id != insertion.receiver.id);
                            for chunk in &mut artifact.chunks {
                                if chunk.properties.effect == Some(insertion.receiver.id) {
                                    chunk.properties.effect = None;
                                }
                                chunk.content_revision.composite_revision = 0;
                            }
                            PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact)
                        }
                        marker => marker,
                    })
                    .collect(),
            })
    {
        return false;
    }
    let mut marker_count = 0usize;
    let mut cursor = 0_u32;
    for (index, step) in insertion.recorded_steps.iter().enumerate() {
        match step {
            PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact) => {
                if index == insertion.insertion_index
                    || artifact.op_count
                        != artifact
                            .chunks
                            .iter()
                            .map(|chunk| chunk.op_count)
                            .sum::<usize>()
                {
                    return false;
                }
                let Some(next) = cursor.checked_add(artifact.opaque_count) else {
                    return false;
                };
                cursor = next;
            }
            PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(marker) => {
                marker_count += 1;
                if index != insertion.insertion_index || *marker != insertion.scroll_cutout {
                    return false;
                }
                if cursor != insertion.receiver_opaque_before {
                    return false;
                }
            }
        }
    }
    marker_count == 1 && cursor == insertion.receiver_opaque_after
}

fn property_scroll_receiver_insertion_is_canonical(
    scaffold: &PropertyScrollScheduleScaffold,
    insertion: &PropertyScrollReceiverInsertionContract,
) -> bool {
    let Some(root) = scaffold.roots.get(insertion.scene_root_ordinal as usize) else {
        return false;
    };
    let Some(boundary) = scaffold
        .boundaries
        .get(insertion.scroll_boundary_ordinal as usize)
    else {
        return false;
    };
    let Some(root_steps) = scaffold.schedule.steps.get(root.step_span.clone()) else {
        return false;
    };
    let [
        PropertySceneScheduledStep::RetainedSurface {
            boundary: PropertyScheduledSurfaceBoundary::Transform(receiver),
            parent: None,
        },
        PropertySceneScheduledStep::ScrollBoundary {
            boundary_ordinal,
            scroll,
            basis: ScrollCompositeBasis::Transform(basis),
            ..
        },
    ] = root_steps
    else {
        return false;
    };
    if insertion.receiver != *receiver
        || insertion.receiver != *basis
        || insertion.receiver.owner != root.root
        || insertion.receiver.generation == 0
        || insertion.receiver_stable_id != root.stable_id
        || *boundary_ordinal != insertion.scroll_boundary_ordinal
        || *scroll != boundary.scroll.id
        || insertion.scroll_cutout
            != (super::PlannedBoundary {
                root: boundary.scroll.owner,
                stable_id: insertion.scroll_cutout.stable_id,
                kind: super::PlannedBoundaryKind::Scroll(boundary.scroll.id),
            })
        || insertion.scroll_cutout.stable_id == 0
        || insertion.insertion_index >= insertion.recorded_steps.len()
        || insertion.before_span != (0..insertion.insertion_index)
        || insertion.after_span != (insertion.insertion_index + 1..insertion.recorded_steps.len())
    {
        return false;
    }
    let mut marker_count = 0usize;
    let mut cursor = 0_u32;
    for (index, step) in insertion.recorded_steps.iter().enumerate() {
        match step {
            PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact) => {
                if index == insertion.insertion_index
                    || artifact.op_count
                        != artifact
                            .chunks
                            .iter()
                            .map(|chunk| chunk.op_count)
                            .sum::<usize>()
                {
                    return false;
                }
                let Some(next) = cursor.checked_add(artifact.opaque_count) else {
                    return false;
                };
                cursor = next;
            }
            PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(marker) => {
                marker_count += 1;
                if index != insertion.insertion_index || *marker != insertion.scroll_cutout {
                    return false;
                }
                if cursor != insertion.receiver_opaque_before {
                    return false;
                }
            }
        }
    }
    marker_count == 1 && cursor == insertion.receiver_opaque_after
}

fn property_scene_plan_is_sealed(plan: &FramePaintPlan) -> bool {
    let Some(seal) = &plan.property_scene_seal else {
        return false;
    };
    if let Some(scaffold) = &seal.nested_scroll_scaffold {
        return seal.effect_scaffold.is_none()
            && seal.scroll_schedule_scaffold.is_none()
            && nested_scroll_scene_scaffold_is_canonical(plan, seal, scaffold);
    }
    if let Some(scaffold) = &seal.scroll_schedule_scaffold {
        return seal.effect_scaffold.is_none()
            && seal.nested_scroll_scaffold.is_none()
            && property_scroll_schedule_scaffold_is_canonical(plan, seal, scaffold);
    }
    if let Some(scaffold) = &seal.effect_scaffold {
        return seal.nested_scroll_scaffold.is_none()
            && property_effect_scaffold_is_canonical(plan, seal, scaffold);
    }
    let Some(plan_roots) = &plan.property_scene_roots else {
        return false;
    };
    if seal.roots.is_empty()
        || plan_roots != &seal.roots
        || seal.context.outer_scissor_rect() != seal.outer_scissor_rect
        || seal.aggregate_opaque_order_span.start != 0
    {
        return false;
    }
    let mut next_step = 0usize;
    let mut root_keys = FxHashSet::default();
    let mut root_stable_ids = FxHashSet::default();
    for (ordinal, root) in seal.roots.iter().enumerate() {
        if root.ordinal as usize != ordinal
            || root.stable_id == 0
            || root.owner
                != (PaintOwnerSnapshot {
                    owner: root.root,
                    parent: None,
                })
            || !root_keys.insert(root.root)
            || !root_stable_ids.insert(root.stable_id)
            || root.top_level_step_span.start != next_step
            || root.top_level_step_span.end < root.top_level_step_span.start
            || root.top_level_step_span.end > plan.steps.len()
        {
            return false;
        }
        for step in &plan.steps[root.top_level_step_span.clone()] {
            match step {
                PaintPlanStep::ArtifactSpan(span) => {
                    let mut parentless = span
                        .artifact
                        .owner_nodes
                        .iter()
                        .filter(|owner| owner.parent.is_none());
                    if parentless.next() != Some(&root.owner)
                        || parentless.any(|owner| *owner != root.owner)
                    {
                        return false;
                    }
                }
                PaintPlanStep::RetainedSurface(surface) => {
                    let Some(contract) = seal.surfaces.values().find(|contract| {
                        contract.id.owner == surface.boundary_root() && contract.parent.is_none()
                    }) else {
                        return false;
                    };
                    if contract.scene_root != root.root {
                        return false;
                    }
                }
            }
        }
        next_step = root.top_level_step_span.end;
    }
    if next_step != plan.steps.len() {
        return false;
    }
    let mut seen = FxHashSet::default();
    let mut stable_ids = FxHashSet::default();
    let mut persistent_keys = FxHashSet::default();
    let mut next_ordinal = 0_u32;
    let mut scene_artifact_index = 0usize;
    fn validate_steps(
        steps: &[PaintPlanStep],
        expected_parent: Option<PropertySurfaceId>,
        incoming_scissor: Option<[u32; 4]>,
        seal: &PropertyScenePlanSeal,
        seen: &mut FxHashSet<PropertySurfaceId>,
        stable_ids: &mut FxHashSet<u64>,
        persistent_keys: &mut FxHashSet<crate::view::frame_graph::PersistentTextureKey>,
        next_ordinal: &mut u32,
        scene_artifact_index: &mut usize,
    ) -> Option<u32> {
        let mut cursor = 0_u32;
        let mut local_artifact_index = 0usize;
        for step in steps {
            match step {
                PaintPlanStep::ArtifactSpan(span) => {
                    if span.opaque_order_span.start != cursor {
                        return None;
                    }
                    let witness = if let Some(parent) = expected_parent {
                        let contract = seal.surfaces.get(&parent)?;
                        let expected = contract.artifact_validation.get(local_artifact_index)?;
                        let actual =
                            super::compiler::validate_transform_property_surface_artifact_for_plan(
                                span.artifact(),
                                parent.owner,
                                parent.transform,
                            )?;
                        local_artifact_index = local_artifact_index.saturating_add(1);
                        (actual == *expected).then_some(())
                    } else {
                        let expected = seal.scene_artifact_validation.get(*scene_artifact_index)?;
                        let actual = super::compiler::validate_property_scene_artifact_for_plan(
                            span.artifact(),
                        )?;
                        *scene_artifact_index = scene_artifact_index.saturating_add(1);
                        (actual == *expected).then_some(())
                    };
                    witness?;
                    let end = cursor.checked_add(opaque_order_count(span.artifact()))?;
                    if span.opaque_order_span.end != end {
                        return None;
                    }
                    cursor = end;
                }
                PaintPlanStep::RetainedSurface(surface) => {
                    let contract = seal
                        .surfaces
                        .values()
                        .find(|contract| contract.id.owner == surface.boundary_root())?;
                    if contract.parent != expected_parent
                        || contract.id.ordinal != *next_ordinal
                        || contract.stable_id == 0
                        || surface.stable_id() != contract.stable_id
                        || surface.persistent_color_key()
                            != crate::view::base_component::transformed_layer_stable_key(
                                contract.stable_id,
                            )
                        || contract.transform.id != contract.id.transform
                        || contract.transform.owner != contract.id.owner
                        || contract.transform != contract.planned_transform_witness
                        || contract.transform.generation == 0
                        || contract.transform.parent
                            != expected_parent.map(|parent| parent.transform)
                        || expected_parent.is_some_and(|parent| {
                            seal.surfaces
                                .get(&parent)
                                .is_none_or(|parent| parent.scene_root != contract.scene_root)
                        })
                        || surface.parent_surface() != expected_parent.map(PropertySurfaceId::owner)
                        || !seen.insert(contract.id)
                        || !stable_ids.insert(surface.stable_id())
                        || !persistent_keys.insert(surface.persistent_color_key())
                    {
                        return None;
                    }
                    *next_ordinal = next_ordinal.checked_add(1)?;
                    let SurfaceKind::Transform(transform) = surface.kind() else {
                        return None;
                    };
                    if transform.transform != contract.id.transform
                        || transform.geometry.outer_scissor_rect
                            != contract.resolved_composite_scissor
                        || !transform
                            .geometry
                            .bitwise_eq(transform.planned_geometry_witness)
                        || transform.context != transform.planned_context_witness
                        || resolve_composite_scissor(
                            incoming_scissor,
                            &contract.ancestor_composite_clips,
                        )
                        .ok()
                            != Some(contract.resolved_composite_scissor)
                    {
                        return None;
                    }
                    let child_terminal = validate_steps(
                        surface.raster_steps(),
                        Some(contract.id),
                        contract.resolved_composite_scissor,
                        seal,
                        seen,
                        stable_ids,
                        persistent_keys,
                        next_ordinal,
                        scene_artifact_index,
                    )?;
                    if surface.aggregate_opaque_order_span() != &(0..child_terminal) {
                        return None;
                    }
                    cursor = cursor.max(child_terminal);
                }
            }
        }
        if let Some(parent) = expected_parent {
            if local_artifact_index != seal.surfaces.get(&parent)?.artifact_validation.len() {
                return None;
            }
        }
        Some(cursor)
    }
    let Some(terminal) = validate_steps(
        plan.steps(),
        None,
        seal.outer_scissor_rect,
        seal,
        &mut seen,
        &mut stable_ids,
        &mut persistent_keys,
        &mut next_ordinal,
        &mut scene_artifact_index,
    ) else {
        return false;
    };
    terminal == seal.aggregate_opaque_order_span.end
        && seen.len() == seal.surface_count
        && seen.len() == seal.surfaces.len()
        && next_ordinal as usize == seal.surface_count
        && scene_artifact_index == seal.scene_artifact_validation.len()
        && seal
            .surfaces
            .keys()
            .map(|id| id.ordinal)
            .collect::<FxHashSet<_>>()
            .len()
            == seal.surface_count
}

fn nested_scroll_artifact_identity_is_canonical(
    artifact: &NestedScrollArtifactSeal,
    owner: NodeKey,
    properties: PropertyTreeState,
    expected_clips: &[ClipNodeSnapshot],
) -> bool {
    let identity = &artifact.identity;
    if property_scroll_receiver_artifact_identity(&artifact.recorded_artifact).as_ref()
        != Some(identity)
    {
        return false;
    }
    identity.chunks.len() == 1
        && identity.owner_topology
            == [PaintOwnerSnapshot {
                owner,
                parent: None,
            }]
        && identity.clip_nodes == expected_clips
        && identity.effect_nodes.is_empty()
        && identity.op_count
            == identity
                .chunks
                .iter()
                .map(|chunk| chunk.op_count)
                .sum::<usize>()
        && identity.chunks.iter().all(|chunk| {
            let [x, y, width, height] = chunk.bounds_bits.map(f32::from_bits);
            chunk.id.owner == owner
                && chunk.owner == owner
                && chunk.properties == properties
                && super::has_canonical_paint_bounds(crate::view::base_component::Rect {
                    x,
                    y,
                    width,
                    height,
                })
        })
        && identity.opaque_count <= identity.op_count.saturating_mul(2) as u32
}

fn nested_scroll_scene_scaffold_is_canonical(
    plan: &FramePaintPlan,
    seal: &PropertyScenePlanSeal,
    scaffold: &NestedScrollSceneScaffold,
) -> bool {
    if !plan.steps.is_empty()
        || seal.surface_count != 0
        || !seal.surfaces.is_empty()
        || !seal.scene_artifact_validation.is_empty()
        || seal.aggregate_opaque_order_span != (0..0)
        || seal.context != scaffold.context
        || seal.outer_scissor_rect.is_some()
        || scaffold.context != scaffold.planned_context
        || scaffold.context != TransformSurfacePlanContext::default()
        || !scaffold.admission.bitwise_eq(scaffold.planned_admission)
        || scaffold.boundaries != scaffold.planned_boundaries
        || scaffold.schedule != scaffold.planned_schedule
    {
        return false;
    }
    let Some(plan_roots) = &plan.property_scene_roots else {
        return false;
    };
    let [plan_root] = plan_roots.as_slice() else {
        return false;
    };
    let [seal_root] = seal.roots.as_slice() else {
        return false;
    };
    let admission = scaffold.admission;
    if plan_root != seal_root
        || plan_root.ordinal != 0
        || plan_root.root != admission.outer_boundary_root
        || plan_root.stable_id != admission.outer_stable_id
        || plan_root.owner
            != (PaintOwnerSnapshot {
                owner: admission.outer_boundary_root,
                parent: None,
            })
        || plan_root.top_level_step_span != (0..0)
        || admission.outer_stable_id == 0
        || admission.inner_stable_id == 0
        || admission.content_leaf_stable_id == 0
        || admission.outer_stable_id == admission.inner_stable_id
        || admission.outer_stable_id == admission.content_leaf_stable_id
        || admission.inner_stable_id == admission.content_leaf_stable_id
        || admission.outer_boundary_root == admission.inner_boundary_root
        || admission.outer_boundary_root == admission.content_leaf
        || admission.inner_boundary_root == admission.content_leaf
    {
        return false;
    }
    for bounds in [admission.outer_source_bounds, admission.inner_source_bounds] {
        if [bounds.x, bounds.y, bounds.width, bounds.height]
            .into_iter()
            .any(|value| !value.is_finite())
            || bounds.width <= 0.0
            || bounds.height <= 0.0
            || bounds.x < 0.0
            || bounds.y < 0.0
            || bounds.x.fract() != 0.0
            || bounds.y.fract() != 0.0
            || (bounds.x + bounds.width).fract() != 0.0
            || (bounds.y + bounds.height).fract() != 0.0
            || bounds.corner_radii.map(f32::to_bits) != [0; 4]
        {
            return false;
        }
    }
    let [outer, inner] = scaffold.boundaries.as_slice() else {
        return false;
    };
    let outer_state = PropertyTreeState {
        clip: Some(outer.contents_clip.id),
        scroll: Some(outer.scroll.id),
        ..PropertyTreeState::default()
    };
    let inner_state = PropertyTreeState {
        clip: Some(inner.contents_clip.id),
        scroll: Some(inner.scroll.id),
        ..PropertyTreeState::default()
    };
    if outer.slot != NestedScrollBoundarySlot::Outer
        || outer.boundary_root != admission.outer_boundary_root
        || outer.stable_id != admission.outer_stable_id
        || outer.parent.is_some()
        || outer.scroll.id != ScrollNodeId(outer.boundary_root)
        || outer.scroll.owner != outer.boundary_root
        || outer.scroll.parent.is_some()
        || outer.scroll.generation == 0
        || outer.contents_clip.id
            != (ClipNodeId {
                owner: outer.boundary_root,
                role: ClipNodeRole::ContentsClip,
            })
        || outer.contents_clip.owner != outer.boundary_root
        || outer.contents_clip.parent.is_some()
        || outer.contents_clip.generation == 0
        || outer.contents_clip.behavior != ClipBehavior::Intersect
        || !outer
            .scroll
            .has_canonical_vertical_geometry_with_contents_clip(outer.contents_clip)
        || outer.content_state != outer_state
        || outer.projected_receiver_state != PropertyTreeState::default()
        || inner.slot != NestedScrollBoundarySlot::Inner
        || inner.boundary_root != admission.inner_boundary_root
        || inner.stable_id != admission.inner_stable_id
        || inner.parent != Some(NestedScrollBoundarySlot::Outer)
        || inner.scroll.id != ScrollNodeId(inner.boundary_root)
        || inner.scroll.owner != inner.boundary_root
        || inner.scroll.parent != Some(outer.scroll.id)
        || inner.scroll.generation == 0
        || inner.contents_clip.id
            != (ClipNodeId {
                owner: inner.boundary_root,
                role: ClipNodeRole::ContentsClip,
            })
        || inner.contents_clip.owner != inner.boundary_root
        || inner.contents_clip.parent != Some(outer.contents_clip.id)
        || inner.contents_clip.generation == 0
        || inner.contents_clip.behavior != ClipBehavior::Intersect
        || !inner
            .scroll
            .has_canonical_nested_vertical_geometry_with_contents_clip(
                inner.contents_clip,
                outer.scroll,
                outer.contents_clip,
            )
        || inner.content_state != inner_state
        || inner.projected_receiver_state != outer_state
        || !admission.matches_scroll_nodes(outer.scroll, inner.scroll)
    {
        return false;
    }
    let [
        NestedScrollSceneScheduledStep::HostBefore {
            boundary: NestedScrollBoundarySlot::Outer,
            artifact: outer_before,
        },
        NestedScrollSceneScheduledStep::HostBefore {
            boundary: NestedScrollBoundarySlot::Inner,
            artifact: inner_before,
        },
        NestedScrollSceneScheduledStep::ContentReceiver(receiver),
        NestedScrollSceneScheduledStep::OverlayAfter {
            boundary: NestedScrollBoundarySlot::Inner,
            artifact: inner_after,
        },
        NestedScrollSceneScheduledStep::OverlayAfter {
            boundary: NestedScrollBoundarySlot::Outer,
            artifact: outer_after,
        },
    ] = scaffold.schedule.steps.as_slice()
    else {
        return false;
    };
    let rebuilt_witness = super::PaintNestedScrollContentWitness::new(
        admission.outer_boundary_root,
        admission.inner_boundary_root,
        admission.content_leaf,
        outer.scroll,
        outer.contents_clip,
        inner.scroll,
        inner.contents_clip,
    );
    receiver.stable_id == admission.content_leaf_stable_id
        && rebuilt_witness == Some(receiver.witness)
        && receiver.witness.outer_boundary_root() == admission.outer_boundary_root
        && receiver.witness.boundary_root() == admission.inner_boundary_root
        && receiver.witness.content_root() == admission.content_leaf
        && receiver.witness.outer_scroll() == outer.scroll.id
        && receiver.witness.inner_scroll() == inner.scroll.id
        && receiver.witness.outer_contents_clip() == outer.contents_clip.id
        && receiver.witness.inner_contents_clip() == inner.contents_clip.id
        && receiver.live_input == inner_state
        && receiver.projected_output == outer_state
        && nested_scroll_artifact_identity_is_canonical(
            outer_before,
            admission.outer_boundary_root,
            PropertyTreeState::default(),
            &[],
        )
        && nested_scroll_artifact_identity_is_canonical(
            inner_before,
            admission.inner_boundary_root,
            PropertyTreeState::default(),
            &[],
        )
        && nested_scroll_artifact_identity_is_canonical(
            &receiver.artifact,
            admission.content_leaf,
            outer_state,
            &[outer.contents_clip],
        )
        && nested_scroll_artifact_identity_is_canonical(
            inner_after,
            admission.inner_boundary_root,
            PropertyTreeState::default(),
            &[],
        )
        && nested_scroll_artifact_identity_is_canonical(
            outer_after,
            admission.outer_boundary_root,
            PropertyTreeState::default(),
            &[],
        )
}

/// Builds an owning, arena-independent plan for the first exact transform
/// surface slice. This is validation and recording only: it never mutates the
/// arena, allocates a render target, emits a frame-graph pass, dispatches the
/// plan, or makes a reuse decision.
pub(crate) fn plan_single_root_transform_surface(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
) -> Result<FramePaintPlan, FramePaintPlanError> {
    plan_single_root_transform_surface_with_context(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        TransformSurfacePlanContext::default(),
    )
}

pub(crate) fn plan_single_root_transform_surface_with_context(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    context: TransformSurfacePlanContext,
) -> Result<FramePaintPlan, FramePaintPlanError> {
    let [root] = roots else {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::RootCount(roots.len())],
        });
    };
    let Some(root_node) = arena.get(*root) else {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::MissingRoot(*root)],
        });
    };
    let Some(root_element) = root_node.element.as_any().downcast_ref::<Element>() else {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::UnknownRootHost(*root)],
        });
    };

    let mut reasons = Vec::new();
    if arena.parent_of(*root).is_some() {
        push_unique(&mut reasons, FramePaintPlanRejection::RootHasParent(*root));
    }
    for &stable_id in promoted_node_ids {
        push_unique(
            &mut reasons,
            FramePaintPlanRejection::PromotionPresent(stable_id),
        );
    }
    for &error in &property_trees.validation_errors {
        push_unique(&mut reasons, FramePaintPlanRejection::PropertyTree(error));
    }

    let mut reachable = Vec::new();
    let mut seen = FxHashSet::default();
    let mut stable_owners = FxHashMap::default();
    let mut stack = vec![*root];
    while let Some(key) = stack.pop() {
        if !seen.insert(key) {
            push_unique(&mut reasons, FramePaintPlanRejection::DuplicateNodeKey(key));
            push_unique(&mut reasons, FramePaintPlanRejection::TopologyMismatch(key));
            continue;
        }
        let Some(node) = arena.get(key) else {
            push_unique(&mut reasons, FramePaintPlanRejection::MissingRoot(key));
            continue;
        };
        reachable.push(key);
        if node.children() != node.element.children() {
            push_unique(&mut reasons, FramePaintPlanRejection::TopologyMismatch(key));
        }
        let stable_id = node.element.stable_id();
        if stable_id == 0 {
            push_unique(&mut reasons, FramePaintPlanRejection::InvalidStableId(key));
        } else if stable_owners.insert(stable_id, key).is_some() {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::DuplicateStableId(stable_id),
            );
        }
        if node.element.is_deferred_to_root_viewport_render() {
            push_unique(&mut reasons, FramePaintPlanRejection::DeferredBoundary(key));
        }
        if node
            .element
            .placement_eligibility_metadata()
            .contains_runtime_layout_state
        {
            push_unique(&mut reasons, FramePaintPlanRejection::LayoutTransition(key));
        }
        for &child in node.element.children() {
            if arena.parent_of(child) != Some(key) {
                push_unique(
                    &mut reasons,
                    FramePaintPlanRejection::TopologyMismatch(child),
                );
            }
        }
        stack.extend(node.element.children().iter().rev().copied());
    }

    let transform = TransformNodeId(*root);
    if !(1..=2).contains(&property_trees.transforms.len()) {
        push_unique(
            &mut reasons,
            FramePaintPlanRejection::TransformNodeCount(property_trees.transforms.len()),
        );
    }
    let root_transform = property_trees.transforms.get(&transform);
    match root_transform {
        None => push_unique(
            &mut reasons,
            FramePaintPlanRejection::MissingRootTransform(*root),
        ),
        Some(snapshot)
            if snapshot.owner != *root
                || snapshot.parent.is_some()
                || snapshot.generation == 0
                || snapshot
                    .viewport_matrix
                    .to_cols_array()
                    .iter()
                    .any(|value| !value.is_finite()) =>
        {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::InvalidRootTransform(*root),
            );
        }
        Some(_) => {}
    }
    if let Some(snapshot) = root_transform
        && !matrix_is_finite_affine(snapshot.viewport_matrix)
    {
        push_unique(
            &mut reasons,
            FramePaintPlanRejection::NonAffineTransform(*root),
        );
    }
    let nested_transform = property_trees
        .transforms
        .iter()
        .find_map(|(&id, snapshot)| (id != transform).then_some((id, *snapshot)));
    let nested_root = nested_transform.map(|(_, snapshot)| snapshot.owner);
    if let Some((nested_id, snapshot)) = nested_transform {
        let nested_is_direct_element = arena.parent_of(snapshot.owner) == Some(*root)
            && arena
                .get(snapshot.owner)
                .is_some_and(|node| node.element.as_any().downcast_ref::<Element>().is_some());
        if !nested_is_direct_element
            || nested_id != TransformNodeId(snapshot.owner)
            || snapshot.parent != Some(transform)
            || snapshot.generation == 0
        {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::UnexpectedTransform(snapshot.owner),
            );
        }
        if !matrix_is_finite_affine(snapshot.viewport_matrix) {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::NonAffineTransform(snapshot.owner),
            );
        }
    }

    let mut nested_subtree = FxHashSet::default();
    if let Some(nested_root) = nested_root {
        let mut nested_stack = vec![nested_root];
        while let Some(key) = nested_stack.pop() {
            if !nested_subtree.insert(key) {
                continue;
            }
            if let Some(node) = arena.get(key) {
                nested_stack.extend(node.element.children().iter().copied());
            }
        }
    }

    let reachable_set = reachable.iter().copied().collect::<FxHashSet<_>>();
    for &key in &reachable {
        let Some(state) = property_trees.states.get(&key) else {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::MissingPropertyState(key),
            );
            continue;
        };
        let expected_transform = if nested_subtree.contains(&key) {
            nested_root.map(TransformNodeId)
        } else {
            Some(transform)
        };
        for properties in [state.paint, state.descendants] {
            if properties.transform != expected_transform {
                push_unique(
                    &mut reasons,
                    FramePaintPlanRejection::WrongTransformBoundary(key),
                );
            }
            if let Some(clip) = properties.clip {
                push_unique(
                    &mut reasons,
                    FramePaintPlanRejection::ClipBoundary(clip.owner),
                );
            }
            if let Some(effect) = properties.effect {
                push_unique(
                    &mut reasons,
                    FramePaintPlanRejection::EffectBoundary(effect.0),
                );
            }
            if let Some(scroll) = properties.scroll {
                push_unique(
                    &mut reasons,
                    FramePaintPlanRejection::ScrollBoundary(scroll.0),
                );
            }
        }
    }
    for &key in property_trees.states.keys() {
        if !reachable_set.contains(&key) {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::UnexpectedPropertyState(key),
            );
        }
    }
    for &clip in property_trees.clips.keys() {
        push_unique(
            &mut reasons,
            FramePaintPlanRejection::ClipBoundary(clip.owner),
        );
    }
    for &effect in property_trees.effects.keys() {
        push_unique(
            &mut reasons,
            FramePaintPlanRejection::EffectBoundary(effect.0),
        );
    }
    for &scroll in property_trees.scrolls.keys() {
        push_unique(
            &mut reasons,
            FramePaintPlanRejection::ScrollBoundary(scroll.0),
        );
    }

    if !reasons.is_empty() {
        return Err(FramePaintPlanError { reasons });
    }

    let record_error = |fallbacks: Vec<FrameArtifactFallbackReason>| FramePaintPlanError {
        reasons: fallbacks
            .into_iter()
            .map(FramePaintPlanRejection::Coverage)
            .collect(),
    };
    let root_surface = if let Some(child_root) = nested_root {
        let root_geometry = exact_surface_geometry_for_plan(
            root_element,
            arena,
            *root,
            context,
            root_transform.map(|snapshot| snapshot.viewport_matrix),
        )?;
        let child_node = arena.get(child_root).ok_or_else(|| FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::MissingRoot(child_root)],
        })?;
        let child_element = child_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .ok_or_else(|| FramePaintPlanError {
                reasons: vec![FramePaintPlanRejection::UnexpectedTransform(child_root)],
            })?;
        let child_transform = TransformNodeId(child_root);
        let child_paint_offset = root_element
            .retained_child_paint_offset(context.paint_offset())
            .ok_or_else(|| FramePaintPlanError {
                reasons: vec![FramePaintPlanRejection::InvalidSurfaceGeometry(child_root)],
            })?;
        let child_context = TransformSurfacePlanContext::new(child_paint_offset, None);
        let child_geometry = exact_surface_geometry_for_plan(
            child_element,
            arena,
            child_root,
            child_context,
            property_trees
                .transforms
                .get(&child_transform)
                .map(|snapshot| snapshot.viewport_matrix),
        )?;
        let boundary = super::PlannedBoundary {
            root: child_root,
            stable_id: child_element.stable_id(),
            kind: super::PlannedBoundaryKind::Transform(child_transform),
        };
        let cutouts = super::PlannedBoundaryCutoutSet::from_iter([(child_root, boundary)]);
        let parent_recorded = super::frame_recorder::record_transform_surface_steps_for_plan(
            arena,
            roots,
            promoted_node_ids,
            property_trees,
            paint_generations,
            PaintTransformSurfaceWitness::canonical_root(*root),
            context.paint_offset(),
            &cutouts,
        )
        .map_err(&record_error)?;
        let child_recorded = super::frame_recorder::record_transform_surface_steps_for_plan(
            arena,
            &[child_root],
            promoted_node_ids,
            property_trees,
            paint_generations,
            PaintTransformSurfaceWitness::canonical_root(child_root),
            child_context.paint_offset(),
            &Default::default(),
        )
        .map_err(&record_error)?;
        let [super::frame_recorder::RecordedTransformSurfaceStep::Artifact(child_artifact)] =
            child_recorded.as_slice()
        else {
            return Err(FramePaintPlanError {
                reasons: vec![FramePaintPlanRejection::InvalidSurfaceArtifact(child_root)],
            });
        };
        if !super::compiler::validate_transform_surface_artifact_for_plan(
            child_artifact,
            child_root,
            child_transform,
        ) {
            return Err(FramePaintPlanError {
                reasons: vec![FramePaintPlanRejection::InvalidSurfaceArtifact(child_root)],
            });
        }
        let child_terminal = opaque_order_count(child_artifact);
        let mut parent_cursor = 0_u32;
        let mut marker_count = 0usize;
        let mut raster_steps = Vec::new();
        for recorded in parent_recorded {
            match recorded {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    if !super::compiler::validate_transform_surface_artifact_for_plan(
                        &artifact, *root, transform,
                    ) {
                        return Err(FramePaintPlanError {
                            reasons: vec![FramePaintPlanRejection::InvalidSurfaceArtifact(*root)],
                        });
                    }
                    let count = opaque_order_count(&artifact);
                    let end = parent_cursor.saturating_add(count);
                    raster_steps.push(PaintPlanStep::ArtifactSpan(ArtifactSpanPlan {
                        artifact,
                        opaque_order_span: parent_cursor..end,
                    }));
                    parent_cursor = end;
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(actual)
                    if actual == boundary =>
                {
                    marker_count = marker_count.saturating_add(1);
                    raster_steps.push(PaintPlanStep::RetainedSurface(Box::new(
                        RetainedSurfacePlan {
                            boundary_root: child_root,
                            stable_id: child_element.stable_id(),
                            persistent_color_key:
                                crate::view::base_component::transformed_layer_stable_key(
                                    child_element.stable_id(),
                                ),
                            kind: SurfaceKind::Transform(TransformSurfacePlan {
                                transform: child_transform,
                                geometry: child_geometry,
                                context: child_context,
                                planned_geometry_witness: child_geometry,
                                planned_context_witness: child_context,
                            }),
                            raster_steps: vec![PaintPlanStep::ArtifactSpan(ArtifactSpanPlan {
                                artifact: child_artifact.clone(),
                                opaque_order_span: 0..child_terminal,
                            })],
                            parent_surface: Some(*root),
                            aggregate_opaque_order_span: 0..child_terminal,
                        },
                    )));
                    parent_cursor = parent_cursor.max(child_terminal);
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(_) => {
                    return Err(FramePaintPlanError {
                        reasons: vec![FramePaintPlanRejection::InvalidSurfaceArtifact(*root)],
                    });
                }
            }
        }
        if marker_count != 1 {
            return Err(FramePaintPlanError {
                reasons: vec![FramePaintPlanRejection::InvalidSurfaceArtifact(*root)],
            });
        }
        RetainedSurfacePlan {
            boundary_root: *root,
            stable_id: root_element.stable_id(),
            persistent_color_key: crate::view::base_component::transformed_layer_stable_key(
                root_element.stable_id(),
            ),
            kind: SurfaceKind::Transform(TransformSurfacePlan {
                transform,
                geometry: root_geometry,
                context,
                planned_geometry_witness: root_geometry,
                planned_context_witness: context,
            }),
            raster_steps,
            parent_surface: None,
            aggregate_opaque_order_span: 0..parent_cursor,
        }
    } else {
        // Exact bounds authority is intentionally checked after recordability
        // in the singleton path so UnknownHost retains its specific coverage
        // diagnostic instead of collapsing to InvalidSurfaceGeometry.
        let raster_artifact = super::frame_recorder::record_transform_surface_artifact_for_plan(
            arena,
            roots,
            promoted_node_ids,
            property_trees,
            paint_generations,
            PaintTransformSurfaceWitness::canonical_root(*root),
        )
        .map_err(&record_error)?;
        if !super::compiler::validate_transform_surface_artifact_for_plan(
            &raster_artifact,
            *root,
            transform,
        ) {
            return Err(FramePaintPlanError {
                reasons: vec![FramePaintPlanRejection::InvalidSurfaceArtifact(*root)],
            });
        }
        let root_geometry = exact_surface_geometry_for_plan(
            root_element,
            arena,
            *root,
            context,
            root_transform.map(|snapshot| snapshot.viewport_matrix),
        )?;
        let opaque_count = opaque_order_count(&raster_artifact);
        RetainedSurfacePlan {
            boundary_root: *root,
            stable_id: root_element.stable_id(),
            persistent_color_key: crate::view::base_component::transformed_layer_stable_key(
                root_element.stable_id(),
            ),
            kind: SurfaceKind::Transform(TransformSurfacePlan {
                transform,
                geometry: root_geometry,
                context,
                planned_geometry_witness: root_geometry,
                planned_context_witness: context,
            }),
            raster_steps: vec![PaintPlanStep::ArtifactSpan(ArtifactSpanPlan {
                artifact: raster_artifact,
                opaque_order_span: 0..opaque_count,
            })],
            parent_surface: None,
            aggregate_opaque_order_span: 0..opaque_count,
        }
    };
    Ok(FramePaintPlan {
        steps: vec![PaintPlanStep::RetainedSurface(Box::new(root_surface))],
        property_scene_roots: None,
        property_scene_seal: None,
    })
}

/// Plans the only admitted mixed retained tree: one root transform and one
/// direct-child isolation. The child artifact consumes the inherited root
/// transform while retaining the exact child-owned effect as its sole group
/// opacity authority.
pub(crate) fn plan_single_root_transform_child_isolation_surface(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
) -> Result<FramePaintPlan, FramePaintPlanError> {
    plan_single_root_transform_child_isolation_surface_with_context(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        TransformSurfacePlanContext::default(),
    )
}

pub(crate) fn plan_single_root_transform_child_isolation_surface_with_context(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    context: TransformSurfacePlanContext,
) -> Result<FramePaintPlan, FramePaintPlanError> {
    let [root] = roots else {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::RootCount(roots.len())],
        });
    };
    let Some(root_node) = arena.get(*root) else {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::MissingRoot(*root)],
        });
    };
    let Some(root_element) = root_node.element.as_any().downcast_ref::<Element>() else {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::UnknownRootHost(*root)],
        });
    };

    let transform = TransformNodeId(*root);
    let child_root = property_trees.effects.keys().next().map(|effect| effect.0);
    let mut reasons = Vec::new();
    if arena.parent_of(*root).is_some() {
        push_unique(&mut reasons, FramePaintPlanRejection::RootHasParent(*root));
    }
    if context.outer_scissor_rect().is_some() {
        push_unique(&mut reasons, FramePaintPlanRejection::IsolationOuterScissor);
    }
    for &stable_id in promoted_node_ids {
        push_unique(
            &mut reasons,
            FramePaintPlanRejection::PromotionPresent(stable_id),
        );
    }
    for &error in &property_trees.validation_errors {
        push_unique(&mut reasons, FramePaintPlanRejection::PropertyTree(error));
    }
    if property_trees.transforms.len() != 1 {
        push_unique(
            &mut reasons,
            FramePaintPlanRejection::TransformNodeCount(property_trees.transforms.len()),
        );
    }
    if property_trees.effects.len() != 1 {
        push_unique(
            &mut reasons,
            FramePaintPlanRejection::InvalidIsolationEffect(*root),
        );
    }
    for &clip in property_trees.clips.keys() {
        push_unique(
            &mut reasons,
            FramePaintPlanRejection::ClipBoundary(clip.owner),
        );
    }
    for &scroll in property_trees.scrolls.keys() {
        push_unique(
            &mut reasons,
            FramePaintPlanRejection::ScrollBoundary(scroll.0),
        );
    }

    let root_transform = property_trees.transforms.get(&transform).copied();
    match root_transform {
        None => push_unique(
            &mut reasons,
            FramePaintPlanRejection::MissingRootTransform(*root),
        ),
        Some(snapshot)
            if snapshot.owner != *root
                || snapshot.parent.is_some()
                || snapshot.generation == 0
                || !matrix_is_finite_affine(snapshot.viewport_matrix) =>
        {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::InvalidRootTransform(*root),
            );
        }
        Some(_) => {}
    }

    let child_is_direct_element = child_root.is_some_and(|child| {
        arena.parent_of(child) == Some(*root)
            && arena
                .get(child)
                .is_some_and(|node| node.element.as_any().downcast_ref::<Element>().is_some())
    });
    let effect_id = child_root.map(EffectNodeId);
    let effect = effect_id.and_then(|effect| property_trees.effects.get(&effect).copied());
    match (child_root, child_is_direct_element, effect) {
        (Some(child), true, Some(snapshot))
            if snapshot.owner == child
                && snapshot.parent.is_none()
                && snapshot.generation != 0
                && snapshot.opacity.is_finite()
                && (0.0..=1.0).contains(&snapshot.opacity) => {}
        (Some(child), _, _) => push_unique(
            &mut reasons,
            FramePaintPlanRejection::InvalidIsolationEffect(child),
        ),
        (None, _, _) => push_unique(
            &mut reasons,
            FramePaintPlanRejection::InvalidIsolationEffect(*root),
        ),
    }

    let mut reachable = Vec::new();
    let mut seen = FxHashSet::default();
    let mut stable_owners = FxHashMap::default();
    let mut stack = vec![*root];
    while let Some(key) = stack.pop() {
        if !seen.insert(key) {
            push_unique(&mut reasons, FramePaintPlanRejection::DuplicateNodeKey(key));
            push_unique(&mut reasons, FramePaintPlanRejection::TopologyMismatch(key));
            continue;
        }
        let Some(node) = arena.get(key) else {
            push_unique(&mut reasons, FramePaintPlanRejection::MissingRoot(key));
            continue;
        };
        reachable.push(key);
        if node.children() != node.element.children() {
            push_unique(&mut reasons, FramePaintPlanRejection::TopologyMismatch(key));
        }
        let stable_id = node.element.stable_id();
        if stable_id == 0 {
            push_unique(&mut reasons, FramePaintPlanRejection::InvalidStableId(key));
        } else if stable_owners.insert(stable_id, key).is_some() {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::DuplicateStableId(stable_id),
            );
        }
        if node.element.is_deferred_to_root_viewport_render() {
            push_unique(&mut reasons, FramePaintPlanRejection::DeferredBoundary(key));
        }
        if node
            .element
            .placement_eligibility_metadata()
            .contains_runtime_layout_state
        {
            push_unique(&mut reasons, FramePaintPlanRejection::LayoutTransition(key));
        }
        for &child in node.element.children() {
            if arena.parent_of(child) != Some(key) {
                push_unique(
                    &mut reasons,
                    FramePaintPlanRejection::TopologyMismatch(child),
                );
            }
        }
        stack.extend(node.element.children().iter().rev().copied());
    }

    let mut child_subtree = FxHashSet::default();
    if let Some(child) = child_root {
        let mut child_stack = vec![child];
        while let Some(key) = child_stack.pop() {
            if !child_subtree.insert(key) {
                continue;
            }
            if let Some(node) = arena.get(key) {
                child_stack.extend(node.element.children().iter().copied());
            }
        }
    }
    let reachable_set = reachable.iter().copied().collect::<FxHashSet<_>>();
    for &key in &reachable {
        let Some(state) = property_trees.states.get(&key) else {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::MissingPropertyState(key),
            );
            continue;
        };
        let expected_effect = if child_subtree.contains(&key) {
            child_root.map(EffectNodeId)
        } else {
            None
        };
        for properties in [state.paint, state.descendants] {
            if properties.transform != Some(transform) {
                push_unique(
                    &mut reasons,
                    FramePaintPlanRejection::WrongTransformBoundary(key),
                );
            }
            if properties.effect != expected_effect {
                push_unique(
                    &mut reasons,
                    FramePaintPlanRejection::InvalidIsolationEffect(key),
                );
            }
            if let Some(clip) = properties.clip {
                push_unique(
                    &mut reasons,
                    FramePaintPlanRejection::ClipBoundary(clip.owner),
                );
            }
            if let Some(scroll) = properties.scroll {
                push_unique(
                    &mut reasons,
                    FramePaintPlanRejection::ScrollBoundary(scroll.0),
                );
            }
        }
    }
    for &key in property_trees.states.keys() {
        if !reachable_set.contains(&key) {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::UnexpectedPropertyState(key),
            );
        }
    }
    if !reasons.is_empty() {
        return Err(FramePaintPlanError { reasons });
    }

    let child_root = child_root.expect("validated exact effect owner");
    let child_node = arena
        .get(child_root)
        .expect("validated direct child node remains reachable");
    let child_element = child_node
        .element
        .as_any()
        .downcast_ref::<Element>()
        .expect("validated direct child Element");
    let effect_id = EffectNodeId(child_root);
    let root_geometry = exact_surface_geometry_for_plan(
        root_element,
        arena,
        *root,
        context,
        root_transform.map(|snapshot| snapshot.viewport_matrix),
    )?;
    let child_paint_offset = root_element
        .retained_child_paint_offset(context.paint_offset())
        .ok_or_else(|| FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::InvalidSurfaceGeometry(child_root)],
        })?;
    let child_bounds = child_element
        .exact_nested_isolation_render_output_bounds(arena, child_paint_offset)
        .ok_or_else(|| FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::InvalidSurfaceGeometry(child_root)],
        })?;
    let child_geometry =
        NestedIsolationSurfaceGeometrySnapshot::from_exact_retained_output(child_bounds)
            .ok_or_else(|| FramePaintPlanError {
                reasons: vec![FramePaintPlanRejection::InvalidSurfaceGeometry(child_root)],
            })?;

    let boundary = super::PlannedBoundary {
        root: child_root,
        stable_id: child_element.stable_id(),
        kind: super::PlannedBoundaryKind::Isolation(effect_id),
    };
    let cutouts = super::PlannedBoundaryCutoutSet::from_iter([(child_root, boundary)]);
    let record_error = |fallbacks: Vec<FrameArtifactFallbackReason>| FramePaintPlanError {
        reasons: fallbacks
            .into_iter()
            .map(FramePaintPlanRejection::Coverage)
            .collect(),
    };
    let parent_recorded = super::frame_recorder::record_transform_surface_steps_for_plan(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        PaintTransformSurfaceWitness::canonical_root(*root),
        context.paint_offset(),
        &cutouts,
    )
    .map_err(&record_error)?;
    let child_artifact = super::frame_recorder::record_transform_child_isolation_artifact_for_plan(
        arena,
        *root,
        child_root,
        promoted_node_ids,
        property_trees,
        paint_generations,
    )
    .map_err(&record_error)?;
    let Some(effect_snapshot) = super::compiler::validate_isolation_surface_artifact_for_plan(
        &child_artifact,
        child_root,
        effect_id,
    ) else {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::InvalidSurfaceArtifact(child_root)],
        });
    };

    let child_terminal = opaque_order_count(&child_artifact);
    let mut parent_cursor = 0_u32;
    let mut marker_count = 0usize;
    let mut raster_steps = Vec::new();
    for recorded in parent_recorded {
        match recorded {
            super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                if !super::compiler::validate_transform_surface_artifact_for_plan(
                    &artifact, *root, transform,
                ) {
                    return Err(FramePaintPlanError {
                        reasons: vec![FramePaintPlanRejection::InvalidSurfaceArtifact(*root)],
                    });
                }
                let count = opaque_order_count(&artifact);
                let end = parent_cursor.saturating_add(count);
                raster_steps.push(PaintPlanStep::ArtifactSpan(ArtifactSpanPlan {
                    artifact,
                    opaque_order_span: parent_cursor..end,
                }));
                parent_cursor = end;
            }
            super::frame_recorder::RecordedTransformSurfaceStep::Boundary(actual)
                if actual == boundary =>
            {
                marker_count = marker_count.saturating_add(1);
                raster_steps.push(PaintPlanStep::RetainedSurface(Box::new(
                    RetainedSurfacePlan {
                        boundary_root: child_root,
                        stable_id: child_element.stable_id(),
                        persistent_color_key:
                            crate::view::base_component::isolation_layer_stable_key(
                                child_element.stable_id(),
                            ),
                        kind: SurfaceKind::NestedIsolation(NestedIsolationSurfacePlan {
                            effect: effect_snapshot,
                            geometry: child_geometry,
                            planned_geometry_witness: child_geometry,
                            property_scene: None,
                            property_scene_artifact: None,
                        }),
                        raster_steps: vec![PaintPlanStep::ArtifactSpan(ArtifactSpanPlan {
                            artifact: child_artifact.clone(),
                            opaque_order_span: 0..child_terminal,
                        })],
                        parent_surface: Some(*root),
                        aggregate_opaque_order_span: 0..child_terminal,
                    },
                )));
                parent_cursor = parent_cursor.max(child_terminal);
            }
            super::frame_recorder::RecordedTransformSurfaceStep::Boundary(_) => {
                return Err(FramePaintPlanError {
                    reasons: vec![FramePaintPlanRejection::InvalidSurfaceArtifact(*root)],
                });
            }
        }
    }
    if marker_count != 1 {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::InvalidSurfaceArtifact(*root)],
        });
    }

    Ok(FramePaintPlan {
        steps: vec![PaintPlanStep::RetainedSurface(Box::new(
            RetainedSurfacePlan {
                boundary_root: *root,
                stable_id: root_element.stable_id(),
                persistent_color_key: crate::view::base_component::transformed_layer_stable_key(
                    root_element.stable_id(),
                ),
                kind: SurfaceKind::Transform(TransformSurfacePlan {
                    transform,
                    geometry: root_geometry,
                    context,
                    planned_geometry_witness: root_geometry,
                    planned_context_witness: context,
                }),
                raster_steps,
                parent_surface: None,
                aggregate_opaque_order_span: 0..parent_cursor,
            },
        ))],
        property_scene_roots: None,
        property_scene_seal: None,
    })
}

/// Plans the first exact typed isolation island: one root-owned opacity
/// effect over the complete frame, with no other property boundary. Content
/// recording is delegated to the canonical root-group recorder so opacity
/// neutralization has one authority.
#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_single_root_isolation_surface(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    viewport_width: u32,
    viewport_height: u32,
    scale_factor: f32,
    outer_scissor_rect: Option<[u32; 4]>,
) -> Result<FramePaintPlan, FramePaintPlanError> {
    let [root] = roots else {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::RootCount(roots.len())],
        });
    };
    let Some(node) = arena.get(*root) else {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::MissingRoot(*root)],
        });
    };
    if node.element.as_any().downcast_ref::<Element>().is_none() {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::UnknownRootHost(*root)],
        });
    }
    let stable_id = node.element.stable_id();
    let effect_id = crate::view::compositor::property_tree::EffectNodeId(*root);
    let mut reasons = Vec::new();
    if arena.parent_of(*root).is_some() {
        reasons.push(FramePaintPlanRejection::RootHasParent(*root));
    }
    if stable_id == 0 {
        reasons.push(FramePaintPlanRejection::InvalidStableId(*root));
    }
    if outer_scissor_rect.is_some() {
        reasons.push(FramePaintPlanRejection::IsolationOuterScissor);
    }
    if !promoted_node_ids.is_empty()
        || !property_trees.transforms.is_empty()
        || !property_trees.clips.is_empty()
        || !property_trees.scrolls.is_empty()
        || property_trees.effects.len() != 1
    {
        reasons.push(FramePaintPlanRejection::InvalidIsolationEffect(*root));
    }
    let effect = property_trees.effects.get(&effect_id).copied();
    if effect.is_none_or(|effect| {
        effect.owner != *root
            || effect.parent.is_some()
            || effect.generation == 0
            || !effect.opacity.is_finite()
            || !(0.0..=1.0).contains(&effect.opacity)
    }) {
        reasons.push(FramePaintPlanRejection::InvalidIsolationEffect(*root));
    }
    for &error in &property_trees.validation_errors {
        reasons.push(FramePaintPlanRejection::PropertyTree(error));
    }
    let mut stack = vec![*root];
    let mut seen = FxHashSet::default();
    while let Some(key) = stack.pop() {
        if !seen.insert(key) {
            reasons.push(FramePaintPlanRejection::DuplicateNodeKey(key));
            continue;
        }
        let Some(current) = arena.get(key) else {
            reasons.push(FramePaintPlanRejection::MissingRoot(key));
            continue;
        };
        if current.element.is_deferred_to_root_viewport_render() {
            reasons.push(FramePaintPlanRejection::DeferredBoundary(key));
        }
        if current
            .element
            .placement_eligibility_metadata()
            .contains_runtime_layout_state
        {
            reasons.push(FramePaintPlanRejection::LayoutTransition(key));
        }
        let exact_effect = property_trees.states.get(&key).is_some_and(|state| {
            [state.paint, state.descendants]
                .into_iter()
                .all(|properties| {
                    properties.effect == Some(effect_id)
                        && properties.transform.is_none()
                        && properties.clip.is_none()
                        && properties.scroll.is_none()
                })
        });
        if !exact_effect {
            reasons.push(FramePaintPlanRejection::InvalidIsolationEffect(key));
        }
        stack.extend(current.element.children().iter().copied());
    }
    reasons.sort_by_key(|reason| format!("{reason:?}"));
    reasons.dedup();
    if !reasons.is_empty() {
        return Err(FramePaintPlanError { reasons });
    }
    let geometry =
        IsolationSurfaceGeometrySnapshot::new(viewport_width, viewport_height, scale_factor)
            .ok_or_else(|| FramePaintPlanError {
                reasons: vec![FramePaintPlanRejection::InvalidSurfaceGeometry(*root)],
            })?;
    let outcome = super::frame_recorder::record_root_group_opacity_frame_artifact(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        super::RendererMode::StrictPlan,
    )
    .map_err(|error| FramePaintPlanError {
        reasons: error
            .reasons
            .into_iter()
            .map(FramePaintPlanRejection::Coverage)
            .collect(),
    })?;
    let super::FrameArtifactRecordOutcome::Artifact { artifact, .. } = outcome else {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::InvalidSurfaceArtifact(*root)],
        });
    };
    let Some(effect_snapshot) =
        super::compiler::validate_isolation_surface_artifact_for_plan(&artifact, *root, effect_id)
    else {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::InvalidSurfaceArtifact(*root)],
        });
    };
    let terminal = opaque_order_count(&artifact);
    Ok(FramePaintPlan {
        steps: vec![PaintPlanStep::RetainedSurface(Box::new(
            RetainedSurfacePlan {
                boundary_root: *root,
                stable_id,
                persistent_color_key: crate::view::base_component::isolation_layer_stable_key(
                    stable_id,
                ),
                kind: SurfaceKind::Isolation(IsolationSurfacePlan {
                    effect: effect_snapshot,
                    geometry,
                    planned_geometry_witness: geometry,
                }),
                raster_steps: vec![PaintPlanStep::ArtifactSpan(ArtifactSpanPlan {
                    artifact,
                    opaque_order_span: 0..terminal,
                })],
                parent_surface: None,
                aggregate_opaque_order_span: 0..terminal,
            },
        ))],
        property_scene_roots: None,
        property_scene_seal: None,
    })
}

/// Plans the first scroll-host canary. Scroll remains baked into the child
/// artifact geometry; compiler and final composite consume identity only and
/// never apply a second translation.
pub(crate) fn plan_single_root_scroll_host_surface(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
    incoming_paint_offset: [f32; 2],
    outer_scissor_rect: Option<[u32; 4]>,
) -> Result<FramePaintPlan, FramePaintPlanError> {
    let [root] = roots else {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::RootCount(roots.len())],
        });
    };
    let node = arena.get(*root).ok_or_else(|| FramePaintPlanError {
        reasons: vec![FramePaintPlanRejection::MissingRoot(*root)],
    })?;
    let root_element = node
        .element
        .as_any()
        .downcast_ref::<Element>()
        .ok_or_else(|| FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::UnknownRootHost(*root)],
        })?;
    let admission = root_element
        .exact_retained_scroll_host_admission(*root, arena, scale_factor)
        .ok_or_else(|| FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::InvalidScrollHost(*root)],
        })?;
    let scroll_id = ScrollNodeId(*root);
    let clip_id = ClipNodeId {
        owner: *root,
        role: ClipNodeRole::ContentsClip,
    };
    let scroll = property_trees
        .scroll_snapshot_for(scroll_id)
        .ok_or_else(|| FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::InvalidScrollHost(*root)],
        })?;
    let contents_clip = property_trees
        .clip_snapshot_for(Some(clip_id))
        .and_then(|snapshot| snapshot.into_iter().next())
        .ok_or_else(|| FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::InvalidScrollHost(*root)],
        })?;
    let expected_contents = PropertyTreeState {
        clip: Some(clip_id),
        scroll: Some(scroll_id),
        ..Default::default()
    };
    let root_state = property_trees.states.get(root).copied();
    let child_state = property_trees.states.get(&admission.child).copied();
    let invalid = incoming_paint_offset.map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || outer_scissor_rect.is_some()
        || !promoted_node_ids.is_empty()
        || !property_trees.validation_errors.is_empty()
        || !property_trees.transforms.is_empty()
        || !property_trees.effects.is_empty()
        || property_trees.scrolls.len() != 1
        || property_trees.clips.len() != 1
        || !admission.matches_scroll_node(scroll)
        || !matches!(
            admission.scroll.scrollbar_overlay.paint_state,
            crate::view::base_component::ScrollbarPaintStateWitness::HiddenNow
                | crate::view::base_component::ScrollbarPaintStateWitness::NotPaintable
                | crate::view::base_component::ScrollbarPaintStateWitness::OpaqueNow
                | crate::view::base_component::ScrollbarPaintStateWitness::TranslucentNow
        )
        || !matches!(
            scroll.scrollbar_overlay.paint_state,
            crate::view::base_component::ScrollbarPaintStateWitness::HiddenNow
                | crate::view::base_component::ScrollbarPaintStateWitness::NotPaintable
                | crate::view::base_component::ScrollbarPaintStateWitness::OpaqueNow
                | crate::view::base_component::ScrollbarPaintStateWitness::TranslucentNow
        )
        || scroll.owner != *root
        || scroll.parent.is_some()
        || scroll.generation == 0
        || scroll.contents_clip
            != crate::view::base_component::ScrollContentsClipWitness::ExactRect(
                contents_clip.logical_scissor,
            )
        || contents_clip.id != clip_id
        || contents_clip.owner != *root
        || contents_clip.parent.is_some()
        || contents_clip.behavior
            != crate::view::compositor::property_tree::ClipBehavior::Intersect
        || root_state.is_none_or(|state| {
            state.paint != PropertyTreeState::default() || state.descendants != expected_contents
        })
        || child_state.is_none_or(|state| {
            state.paint != expected_contents || state.descendants != expected_contents
        });
    if invalid {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::InvalidScrollHost(*root)],
        });
    }
    let witness = super::PaintBakedScrollHostWitness::new(*root, admission.child, scroll, clip_id)
        .ok_or_else(|| FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::InvalidScrollHost(*root)],
        })?;
    let artifact = super::frame_recorder::record_baked_scroll_host_artifact_for_plan(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        witness,
    )
    .map_err(|fallbacks| FramePaintPlanError {
        reasons: fallbacks
            .into_iter()
            .map(FramePaintPlanRejection::Coverage)
            .collect(),
    })?;
    if !super::compiler::validate_baked_scroll_host_artifact_for_plan(
        &artifact,
        *root,
        admission.child,
        scroll,
        contents_clip,
    ) {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::InvalidSurfaceArtifact(*root)],
        });
    }
    let terminal = opaque_order_count(&artifact);
    Ok(FramePaintPlan {
        steps: vec![PaintPlanStep::RetainedSurface(Box::new(
            RetainedSurfacePlan {
                boundary_root: *root,
                stable_id: admission.stable_id,
                persistent_color_key: crate::view::base_component::scroll_host_layer_stable_key(
                    admission.stable_id,
                ),
                kind: SurfaceKind::ScrollHost(ScrollHostSurfacePlan {
                    scroll,
                    contents_clip,
                    admission,
                    planned_scroll_witness: scroll,
                    planned_clip_witness: contents_clip,
                    planned_admission_witness: admission,
                }),
                raster_steps: vec![PaintPlanStep::ArtifactSpan(ArtifactSpanPlan {
                    artifact,
                    opaque_order_span: 0..terminal,
                })],
                parent_surface: None,
                aggregate_opaque_order_span: 0..terminal,
            },
        ))],
        property_scene_roots: None,
        property_scene_seal: None,
    })
}

fn exact_surface_geometry_for_plan(
    element: &Element,
    arena: &NodeArena,
    root: NodeKey,
    context: TransformSurfacePlanContext,
    expected_viewport_matrix: Option<glam::Mat4>,
) -> Result<TransformSurfaceGeometrySnapshot, FramePaintPlanError> {
    let geometry = element
        .exact_transform_surface_geometry_snapshot(
            arena,
            context.paint_offset(),
            context.outer_scissor_rect,
        )
        .ok_or_else(|| FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::InvalidSurfaceGeometry(root)],
        })?;
    if geometry.source_bounds.x < 0.0 || geometry.source_bounds.y < 0.0 {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::NegativeSurfaceOrigin(root)],
        });
    }
    if expected_viewport_matrix.is_some_and(|expected| {
        expected.to_cols_array().map(f32::to_bits)
            != geometry
                .viewport_transform
                .to_cols_array()
                .map(f32::to_bits)
    }) {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::InvalidRootTransform(root)],
        });
    }
    Ok(geometry)
}

fn push_unique(reasons: &mut Vec<FramePaintPlanRejection>, reason: FramePaintPlanRejection) {
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

fn matrix_is_finite_affine(matrix: glam::Mat4) -> bool {
    let values = matrix.to_cols_array();
    values.iter().all(|value| value.is_finite())
        && values[3] == 0.0
        && values[7] == 0.0
        && values[11] == 0.0
        && values[15] == 1.0
}

pub(super) fn opaque_order_count(artifact: &PaintArtifact) -> u32 {
    artifact
        .ops
        .iter()
        .map(|op| match op {
            PaintOp::DrawRect(op) => u32::from(rect_is_opaque(&op.params, op.mode)),
            PaintOp::PreparedInlineIfcDecoration(op) => {
                u32::from(rect_is_opaque(
                    &op.fill,
                    crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
                )) + op.border.as_ref().map_or(0, |border| {
                    u32::from(rect_is_opaque(
                        border,
                        crate::view::render_pass::draw_rect_pass::RectRenderMode::BorderOnly,
                    ))
                })
            }
            PaintOp::PreparedShadow(_)
            | PaintOp::PreparedScrollbarOverlay(_)
            | PaintOp::PreparedText(_)
            | PaintOp::PreparedImage(_)
            | PaintOp::PreparedSvg(_) => 0,
        })
        .fold(0, u32::saturating_add)
}

fn rect_is_opaque(
    params: &crate::view::render_pass::draw_rect_pass::RectPassParams,
    mode: crate::view::render_pass::draw_rect_pass::RectRenderMode,
) -> bool {
    let mut pass = crate::view::render_pass::draw_rect_pass::DrawRectPass::new(
        params.clone(),
        Default::default(),
        Default::default(),
    );
    pass.set_render_mode(mode);
    pass.is_opaque_candidate()
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::any::Any;
    use std::sync::Arc;

    use slotmap::Key;

    use crate::style::{
        Angle, BoxShadow, Color, Layout, ParsedValue, PropertyId, Rotate, ScrollDirection, Style,
        Transform,
    };
    use crate::view::base_component::{
        BoxModelSnapshot, BuildState, DirtyPassMask, ElementTrait, EventTarget, Image,
        LayoutConstraints, LayoutPlacement, Layoutable, Rect, Renderable, Size, Svg,
        UiBuildContext,
    };
    use crate::view::frame_graph::{FrameGraph, FramePassTestPayload};
    use crate::view::node_arena::Node;
    use crate::view::paint::tests::exact_isolation_fixture;
    use crate::view::paint::{
        PaintBakedScrollHostWitness, PaintScrollContentWitness, PlannedBoundary,
        PlannedBoundaryKind,
    };
    use crate::view::test_support::{
        commit_child, commit_element, measure_and_place, new_test_arena,
    };
    use crate::view::viewport::Viewport;
    use crate::view::{ImageSource, SvgSource};

    struct UnknownHost {
        id: u64,
        width: f32,
        height: f32,
    }

    impl Layoutable for UnknownHost {
        fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
        fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
        fn measured_size(&self) -> (f32, f32) {
            (self.width, self.height)
        }
        fn set_layout_width(&mut self, width: f32) {
            self.width = width;
        }
        fn set_layout_height(&mut self, height: f32) {
            self.height = height;
        }
    }

    impl EventTarget for UnknownHost {}

    impl Renderable for UnknownHost {
        fn build(
            &mut self,
            graph: &mut FrameGraph,
            _arena: &mut NodeArena,
            mut ctx: UiBuildContext,
        ) -> BuildState {
            let mut pass = crate::view::render_pass::draw_rect_pass::DrawRectPass::new(
                crate::view::render_pass::draw_rect_pass::RectPassParams {
                    position: [0.0, 0.0],
                    size: [self.width, self.height],
                    fill_color: [0.2, 0.4, 0.6, 0.5],
                    opacity: 1.0,
                    ..Default::default()
                },
                Default::default(),
                Default::default(),
            );
            pass.set_render_mode(
                crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
            );
            ctx.emit_draw_rect_pass(graph, pass);
            ctx.into_state()
        }
    }

    impl ElementTrait for UnknownHost {
        fn stable_id(&self) -> u64 {
            self.id
        }

        fn box_model_snapshot(&self) -> BoxModelSnapshot {
            BoxModelSnapshot {
                node_id: self.id,
                parent_id: None,
                x: 0.0,
                y: 0.0,
                width: self.width,
                height: self.height,
                border_radius: 0.0,
                should_render: true,
            }
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    fn exact_transform_fixture_at_origin_with_ids(
        root_id: u64,
        child_id: u64,
        root_x: f32,
        root_y: f32,
    ) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
        let mut root = Element::new_with_id(root_id, root_x, root_y, 40.0, 24.0);
        let mut root_style = Style::new();
        root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        root_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(20, 40, 80)),
        );
        root_style.set_transform(Transform::new([Rotate::z(Angle::deg(12.0))]));
        root.apply_style(root_style);

        let mut child = Element::new_with_id(child_id, 0.0, 0.0, 18.0, 10.0);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(180, 60, 20)),
        );
        child.apply_style(child_style);

        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(root));
        commit_child(&mut arena, root, Box::new(child));
        measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 160.0,
                max_height: 120.0,
                viewport_width: 160.0,
                viewport_height: 120.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
                parent_x: root_x,
                parent_y: root_y,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 160.0,
                available_height: 120.0,
                viewport_width: 160.0,
                viewport_height: 120.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(120.0),
            },
        );
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        (arena, root, properties, generations)
    }

    fn exact_transform_fixture() -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
        exact_transform_fixture_at_origin_with_ids(0xc1_0001, 0xc1_0002, 4.25, 3.5)
    }

    fn nested_exact_transform_fixture() -> (
        NodeArena,
        NodeKey,
        NodeKey,
        NodeKey,
        NodeKey,
        NodeKey,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        let styled_element = |id, x, y, width, height, color| {
            let mut element = Element::new_with_id(id, x, y, width, height);
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
            element.apply_style(style);
            element
        };

        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(styled_element(
                0xc5_a100,
                0.25,
                0.25,
                40.0,
                24.0,
                Color::rgb(20, 40, 80),
            )),
        );
        let before = commit_child(
            &mut arena,
            root,
            Box::new(styled_element(
                0xc5_a101,
                1.25,
                1.25,
                2.0,
                2.0,
                Color::rgb(40, 120, 80),
            )),
        );
        let child = commit_child(
            &mut arena,
            root,
            Box::new(styled_element(
                0xc5_a102,
                4.25,
                1.5,
                18.0,
                10.0,
                Color::rgb(180, 60, 20),
            )),
        );
        let descendant = commit_child(
            &mut arena,
            child,
            Box::new(styled_element(
                0xc5_a103,
                5.0,
                1.75,
                1.0,
                1.0,
                Color::rgb(200, 160, 20),
            )),
        );
        let after = commit_child(
            &mut arena,
            root,
            Box::new(styled_element(
                0xc5_a104,
                2.25,
                5.25,
                2.0,
                2.0,
                Color::rgb(100, 80, 180),
            )),
        );

        measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 160.0,
                max_height: 120.0,
                viewport_width: 160.0,
                viewport_height: 120.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
                parent_x: 0.25,
                parent_y: 0.25,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 160.0,
                available_height: 120.0,
                viewport_width: 160.0,
                viewport_height: 120.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(120.0),
            },
        );

        let parent_matrix = glam::Mat4::from_translation(glam::Vec3::new(100.0, 0.0, 0.0));
        let child_matrix = glam::Mat4::from_cols_array(&[
            0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 30.0, 0.0, 0.0, 1.0,
        ]);
        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_resolved_transform_for_test(Some(parent_matrix));
        crate::view::test_support::get_element_mut::<Element>(&arena, child)
            .set_resolved_transform_for_test(Some(child_matrix));

        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        (
            arena,
            root,
            before,
            child,
            descendant,
            after,
            properties,
            generations,
        )
    }

    struct GeneralPropertySceneFixture {
        arena: NodeArena,
        roots: Vec<NodeKey>,
        outer: NodeKey,
        inner_a: NodeKey,
        deep: NodeKey,
        inner_b: NodeKey,
        second_root: NodeKey,
        properties: PropertyTrees,
        generations: PaintGenerationTracker,
    }

    #[derive(Clone, Copy)]
    enum ScrollInterleaveFixtureShape {
        FrameRootScroll,
        TransformScroll,
        EffectScroll,
        TransformEffectScroll,
        ScrollTransform,
        CoLocatedTransformScroll,
    }

    fn property_scroll_interleave_fixture(
        shape: ScrollInterleaveFixtureShape,
    ) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
        let mut arena = NodeArena::new();
        let wrapper = |id| Element::new_with_id(id, 0.0, 0.0, 120.0, 90.0);
        let root_element = if matches!(shape, ScrollInterleaveFixtureShape::TransformEffectScroll) {
            Element::new_with_id(0xb4_0001, 0.0, 0.0, 168.0, 112.0)
        } else {
            wrapper(0xb4_0001)
        };
        let root = arena.insert(Node::new(Box::new(root_element)));
        let (scroll, content) = match shape {
            ScrollInterleaveFixtureShape::FrameRootScroll
            | ScrollInterleaveFixtureShape::CoLocatedTransformScroll => {
                let content = arena.insert(Node::new(Box::new(Element::new_with_id(
                    0xb4_0010, 0.0, -20.0, 120.0, 240.0,
                ))));
                arena.set_parent(content, Some(root));
                arena.push_child(root, content);
                (root, content)
            }
            ScrollInterleaveFixtureShape::TransformScroll
            | ScrollInterleaveFixtureShape::EffectScroll => {
                let scroll = arena.insert(Node::new(Box::new(wrapper(0xb4_0002))));
                let content = arena.insert(Node::new(Box::new(Element::new_with_id(
                    0xb4_0010, 0.0, -20.0, 120.0, 240.0,
                ))));
                arena.set_parent(scroll, Some(root));
                arena.push_child(root, scroll);
                arena.set_parent(content, Some(scroll));
                arena.push_child(scroll, content);
                (scroll, content)
            }
            ScrollInterleaveFixtureShape::TransformEffectScroll => {
                let effect = arena.insert(Node::new(Box::new(wrapper(0xb4_0002))));
                let scroll = arena.insert(Node::new(Box::new(wrapper(0xb4_0003))));
                let content = arena.insert(Node::new(Box::new(Element::new_with_id(
                    0xb4_0010, 0.0, -20.0, 120.0, 240.0,
                ))));
                arena.set_parent(effect, Some(root));
                arena.push_child(root, effect);
                arena.set_parent(scroll, Some(effect));
                arena.push_child(effect, scroll);
                arena.set_parent(content, Some(scroll));
                arena.push_child(scroll, content);
                crate::view::test_support::get_element_mut::<Element>(&arena, effect)
                    .set_opacity(0.5);
                crate::view::test_support::get_element_mut::<Element>(&arena, effect)
                    .set_background_color_value(Color::rgb(32, 64, 96));
                let mut effect_style = Style::new();
                effect_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
                crate::view::test_support::get_element_mut::<Element>(&arena, effect)
                    .apply_style(effect_style);
                crate::view::test_support::get_element_mut::<Element>(&arena, effect)
                    .set_opacity(0.5);
                crate::view::test_support::get_element_mut::<Element>(&arena, effect)
                    .set_background_color_value(Color::rgb(32, 64, 96));
                (scroll, content)
            }
            ScrollInterleaveFixtureShape::ScrollTransform => {
                let transform = arena.insert(Node::new(Box::new(Element::new_with_id(
                    0xb4_0002, 0.0, -20.0, 120.0, 240.0,
                ))));
                arena.set_parent(transform, Some(root));
                arena.push_child(root, transform);
                crate::view::test_support::get_element_mut::<Element>(&arena, transform)
                    .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                        glam::Vec3::new(3.0, 0.0, 0.0),
                    )));
                (root, transform)
            }
        };
        match shape {
            ScrollInterleaveFixtureShape::TransformScroll
            | ScrollInterleaveFixtureShape::TransformEffectScroll => {
                crate::view::test_support::get_element_mut::<Element>(&arena, root)
                    .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                        glam::Vec3::new(7.0, 0.0, 0.0),
                    )));
                if matches!(shape, ScrollInterleaveFixtureShape::TransformEffectScroll) {
                    crate::view::test_support::get_element_mut::<Element>(&arena, root)
                        .set_background_color_value(Color::rgb(16, 32, 48));
                    let mut root_style = Style::new();
                    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
                    crate::view::test_support::get_element_mut::<Element>(&arena, root)
                        .apply_style(root_style);
                    crate::view::test_support::get_element_mut::<Element>(&arena, root)
                        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                            glam::Vec3::new(7.0, 0.0, 0.0),
                        )));
                    crate::view::test_support::get_element_mut::<Element>(&arena, root)
                        .set_background_color_value(Color::rgb(16, 32, 48));
                }
            }
            ScrollInterleaveFixtureShape::EffectScroll => {
                crate::view::test_support::get_element_mut::<Element>(&arena, root)
                    .set_opacity(0.5);
            }
            ScrollInterleaveFixtureShape::CoLocatedTransformScroll => {
                crate::view::test_support::get_element_mut::<Element>(&arena, root)
                    .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                        glam::Vec3::new(7.0, 0.0, 0.0),
                    )));
            }
            ScrollInterleaveFixtureShape::FrameRootScroll
            | ScrollInterleaveFixtureShape::ScrollTransform => {}
        }
        let mut style = Style::new();
        style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        {
            let mut scroll = crate::view::test_support::get_element_mut::<Element>(&arena, scroll);
            scroll.apply_style(style);
            scroll.layout_state.content_size = Size {
                width: 120.0,
                height: 240.0,
            };
            scroll.set_scroll_offset((0.0, 20.0));
            scroll.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        if matches!(
            shape,
            ScrollInterleaveFixtureShape::CoLocatedTransformScroll
        ) {
            crate::view::test_support::get_element_mut::<Element>(&arena, root)
                .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                    glam::Vec3::new(7.0, 0.0, 0.0),
                )));
        }
        crate::view::test_support::get_element_mut::<Element>(&arena, content)
            .set_background_color_value(Color::rgb(24, 48, 72));
        arena
            .get_mut(content)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        arena.refresh_subtree_dirty_cache(root);
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        assert!(
            properties.validation_errors.is_empty(),
            "{:?}",
            properties.validation_errors
        );
        if matches!(shape, ScrollInterleaveFixtureShape::TransformEffectScroll) {
            let children = arena.children_of(root);
            let [effect] = children.as_slice() else {
                panic!("T->E->S fixture owns one direct effect child")
            };
            assert!(properties.transforms.contains_key(&TransformNodeId(root)));
            assert!(properties.effects.contains_key(&EffectNodeId(*effect)));
        }
        if matches!(shape, ScrollInterleaveFixtureShape::ScrollTransform) {
            assert!(
                properties
                    .transforms
                    .contains_key(&TransformNodeId(content))
            );
        }
        if matches!(
            shape,
            ScrollInterleaveFixtureShape::CoLocatedTransformScroll
        ) {
            assert!(properties.transforms.contains_key(&TransformNodeId(root)));
        }
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        (arena, root, properties, generations)
    }

    pub(crate) fn nested_scroll_plan_fixture() -> (
        NodeArena,
        NodeKey,
        NodeKey,
        NodeKey,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        fn install_geometry(arena: &NodeArena, key: NodeKey, rect: Rect, content: Size) {
            let mut element = crate::view::test_support::get_element_mut::<Element>(arena, key);
            element.layout_state.layout_position.x = rect.x;
            element.layout_state.layout_position.y = rect.y;
            element.layout_state.layout_size = Size {
                width: rect.width,
                height: rect.height,
            };
            element.layout_state.layout_inner_position.x = rect.x;
            element.layout_state.layout_inner_position.y = rect.y;
            element.layout_state.layout_inner_size = Size {
                width: rect.width,
                height: rect.height,
            };
            element.layout_state.content_size = content;
            element.set_background_color_value(Color::rgb(24, 48, 72));
            element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }

        let mut arena = NodeArena::new();
        let outer = arena.insert(Node::new(Box::new(Element::new_with_id(
            0x1251_00, 10.0, 20.0, 100.0, 80.0,
        ))));
        let inner = arena.insert(Node::new(Box::new(Element::new_with_id(
            0x1251_01, 10.0, 20.0, 100.0, 300.0,
        ))));
        let leaf = arena.insert(Node::new(Box::new(Element::new_with_id(
            0x1251_02, 10.0, 20.0, 100.0, 600.0,
        ))));
        arena.set_parent(inner, Some(outer));
        arena.push_child(outer, inner);
        arena.set_parent(leaf, Some(inner));
        arena.push_child(inner, leaf);
        for owner in [outer, inner] {
            let mut style = Style::new();
            style.insert(
                PropertyId::ScrollDirection,
                ParsedValue::ScrollDirection(ScrollDirection::Vertical),
            );
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            crate::view::test_support::get_element_mut::<Element>(&arena, owner).apply_style(style);
        }
        install_geometry(
            &arena,
            outer,
            Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 80.0,
            },
            Size {
                width: 100.0,
                height: 300.0,
            },
        );
        install_geometry(
            &arena,
            inner,
            Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 300.0,
            },
            Size {
                width: 100.0,
                height: 600.0,
            },
        );
        install_geometry(
            &arena,
            leaf,
            Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 600.0,
            },
            Size {
                width: 100.0,
                height: 600.0,
            },
        );
        arena.refresh_subtree_dirty_cache(outer);
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[outer]);
        assert!(properties.validation_errors.is_empty());
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[outer], &properties);
        (arena, outer, inner, leaf, properties, generations)
    }

    #[test]
    fn nested_scroll_planner_seals_exact_graph_inert_schedule_and_state_projection() {
        let (arena, outer, inner, leaf, properties, generations) = nested_scroll_plan_fixture();
        let plan = plan_nested_scroll_scene_scaffold_with_context(
            &arena,
            &[outer],
            &FxHashSet::default(),
            &properties,
            &generations,
            1.0,
            TransformSurfacePlanContext::default(),
        )
        .expect("exact nested-scroll planning scaffold");
        assert!(property_scene_plan_is_sealed(&plan));
        assert!(plan.steps().is_empty());
        assert!(plan.property_scene_transaction_witness().is_none());
        assert!(plan.property_scene_context().is_none());
        assert!(plan.property_scroll_planning_scaffold().is_none());
        assert!(plan.property_scroll_receiver_insertions().is_none());
        assert!(plan.property_effect_scroll_receiver_insertions().is_none());
        assert!(
            plan.property_transform_effect_scroll_receiver_insertions()
                .is_none()
        );
        let scaffold = plan
            .nested_scroll_planning_scaffold()
            .expect("dedicated nested-scroll seal");
        assert_eq!(scaffold.boundaries.len(), 2);
        assert_eq!(scaffold.boundaries[0].boundary_root, outer);
        assert_eq!(scaffold.boundaries[0].parent, None);
        assert_eq!(scaffold.boundaries[1].boundary_root, inner);
        assert_eq!(
            scaffold.boundaries[1].parent,
            Some(NestedScrollBoundarySlot::Outer)
        );
        let outer_state = scaffold.boundaries[0].content_state;
        let inner_state = scaffold.boundaries[1].content_state;
        assert_eq!(
            scaffold.boundaries[0].projected_receiver_state,
            PropertyTreeState::default()
        );
        assert_eq!(scaffold.boundaries[1].projected_receiver_state, outer_state);
        assert!(matches!(
            scaffold.schedule.steps.as_slice(),
            [
                NestedScrollSceneScheduledStep::HostBefore {
                    boundary: NestedScrollBoundarySlot::Outer,
                    ..
                },
                NestedScrollSceneScheduledStep::HostBefore {
                    boundary: NestedScrollBoundarySlot::Inner,
                    ..
                },
                NestedScrollSceneScheduledStep::ContentReceiver(receiver),
                NestedScrollSceneScheduledStep::OverlayAfter {
                    boundary: NestedScrollBoundarySlot::Inner,
                    ..
                },
                NestedScrollSceneScheduledStep::OverlayAfter {
                    boundary: NestedScrollBoundarySlot::Outer,
                    ..
                },
            ] if receiver.witness.content_root() == leaf
                && receiver.live_input == inner_state
                && receiver.projected_output == outer_state
        ));
    }

    #[test]
    fn nested_scroll_planner_rejects_context_promotion_and_property_expansion() {
        let (arena, outer, _inner, _leaf, properties, generations) = nested_scroll_plan_fixture();
        let plan = |arena: &NodeArena,
                    properties: &PropertyTrees,
                    generations: &PaintGenerationTracker,
                    promoted: &FxHashSet<u64>,
                    scale_factor: f32,
                    context: TransformSurfacePlanContext| {
            plan_nested_scroll_scene_scaffold_with_context(
                arena,
                &[outer],
                promoted,
                properties,
                generations,
                scale_factor,
                context,
            )
        };
        assert!(
            plan(
                &arena,
                &properties,
                &generations,
                &FxHashSet::default(),
                2.0,
                TransformSurfacePlanContext::default(),
            )
            .is_err()
        );
        assert!(
            plan(
                &arena,
                &properties,
                &generations,
                &FxHashSet::default(),
                1.0,
                TransformSurfacePlanContext::new([1.0, 0.0], None),
            )
            .is_err()
        );
        assert!(
            plan(
                &arena,
                &properties,
                &generations,
                &FxHashSet::default(),
                1.0,
                TransformSurfacePlanContext::new([0.0, 0.0], Some([0, 0, 10, 10])),
            )
            .is_err()
        );
        assert!(
            plan(
                &arena,
                &properties,
                &generations,
                &FxHashSet::from_iter([0x1251_00]),
                1.0,
                TransformSurfacePlanContext::default(),
            )
            .is_err()
        );

        for expansion in 0..4 {
            let (mut arena, outer, inner, leaf, mut properties, mut generations) =
                nested_scroll_plan_fixture();
            match expansion {
                0 => crate::view::test_support::get_element_mut::<Element>(&arena, leaf)
                    .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                        glam::Vec3::new(1.0, 0.0, 0.0),
                    ))),
                1 => crate::view::test_support::get_element_mut::<Element>(&arena, leaf)
                    .set_opacity(0.5),
                2 => {
                    let sibling = arena.insert(Node::new(Box::new(Element::new_with_id(
                        0x1251_03, 10.0, 20.0, 10.0, 10.0,
                    ))));
                    arena.set_parent(sibling, Some(inner));
                    arena.push_child(inner, sibling);
                }
                3 => {
                    let mut style = Style::new();
                    style.insert(
                        PropertyId::ScrollDirection,
                        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
                    );
                    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
                    let mut leaf =
                        crate::view::test_support::get_element_mut::<Element>(&arena, leaf);
                    leaf.apply_style(style);
                    leaf.layout_state.content_size.height = 900.0;
                }
                _ => unreachable!(),
            }
            arena.refresh_subtree_dirty_cache(outer);
            properties.sync(&arena, &[outer]);
            generations.sync(&arena, &[outer], &properties);
            assert!(
                plan_nested_scroll_scene_scaffold_with_context(
                    &arena,
                    &[outer],
                    &FxHashSet::default(),
                    &properties,
                    &generations,
                    1.0,
                    TransformSurfacePlanContext::default(),
                )
                .is_err(),
                "property/topology expansion {expansion} must fail closed"
            );
        }
    }

    #[test]
    fn nested_scroll_seal_rejects_schedule_parent_generation_and_admission_drift() {
        let (arena, outer, _inner, _leaf, properties, generations) = nested_scroll_plan_fixture();
        let build = || {
            plan_nested_scroll_scene_scaffold_with_context(
                &arena,
                &[outer],
                &FxHashSet::default(),
                &properties,
                &generations,
                1.0,
                TransformSurfacePlanContext::default(),
            )
            .unwrap()
        };
        fn scaffold(plan: &mut FramePaintPlan) -> &mut NestedScrollSceneScaffold {
            plan.property_scene_seal
                .as_mut()
                .unwrap()
                .nested_scroll_scaffold
                .as_mut()
                .unwrap()
        }

        let mut reordered = build();
        let nested = scaffold(&mut reordered);
        nested.schedule.steps.swap(0, 1);
        nested.planned_schedule.steps.swap(0, 1);
        assert!(!property_scene_plan_is_sealed(&reordered));

        let mut dropped = build();
        let nested = scaffold(&mut dropped);
        nested.schedule.steps.pop();
        nested.planned_schedule.steps.pop();
        assert!(!property_scene_plan_is_sealed(&dropped));

        let mut duplicated = build();
        let nested = scaffold(&mut duplicated);
        let duplicate = nested.schedule.steps[0].clone();
        nested.schedule.steps.insert(1, duplicate.clone());
        nested.planned_schedule.steps.insert(1, duplicate);
        assert!(!property_scene_plan_is_sealed(&duplicated));

        let mut retargeted = build();
        let nested = scaffold(&mut retargeted);
        let NestedScrollSceneScheduledStep::HostBefore { boundary, .. } =
            &mut nested.schedule.steps[0]
        else {
            unreachable!()
        };
        *boundary = NestedScrollBoundarySlot::Inner;
        nested.planned_schedule = nested.schedule.clone();
        assert!(!property_scene_plan_is_sealed(&retargeted));

        let mut parent = build();
        let nested = scaffold(&mut parent);
        nested.boundaries[1].parent = None;
        nested.planned_boundaries[1].parent = None;
        assert!(!property_scene_plan_is_sealed(&parent));

        let mut generation = build();
        let nested = scaffold(&mut generation);
        nested.boundaries[1].scroll.generation = 0;
        nested.planned_boundaries[1].scroll.generation = 0;
        assert!(!property_scene_plan_is_sealed(&generation));

        let mut stable = build();
        scaffold(&mut stable).boundaries[1].stable_id += 1;
        assert!(!property_scene_plan_is_sealed(&stable));

        let mut admission = build();
        let nested = scaffold(&mut admission);
        nested.admission.outer_source_bounds.x += 0.5;
        nested.planned_admission.outer_source_bounds.x += 0.5;
        assert!(!property_scene_plan_is_sealed(&admission));
    }

    #[test]
    fn nested_scroll_seal_rejects_artifact_and_receiver_identity_drift() {
        let (arena, outer, _inner, _leaf, properties, generations) = nested_scroll_plan_fixture();
        let build = || {
            plan_nested_scroll_scene_scaffold_with_context(
                &arena,
                &[outer],
                &FxHashSet::default(),
                &properties,
                &generations,
                1.0,
                TransformSurfacePlanContext::default(),
            )
            .unwrap()
        };
        fn scaffold(plan: &mut FramePaintPlan) -> &mut NestedScrollSceneScaffold {
            plan.property_scene_seal
                .as_mut()
                .unwrap()
                .nested_scroll_scaffold
                .as_mut()
                .unwrap()
        }

        let mut artifact_plan = build();
        let nested = scaffold(&mut artifact_plan);
        let NestedScrollSceneScheduledStep::HostBefore {
            artifact: identity, ..
        } = &mut nested.schedule.steps[0]
        else {
            unreachable!()
        };
        identity.identity.op_count += 1;
        nested.planned_schedule = nested.schedule.clone();
        assert!(!property_scene_plan_is_sealed(&artifact_plan));

        let mut opaque = build();
        let nested = scaffold(&mut opaque);
        let NestedScrollSceneScheduledStep::HostBefore { artifact, .. } =
            &mut nested.schedule.steps[0]
        else {
            unreachable!()
        };
        artifact.identity.opaque_count = if artifact.identity.opaque_count == 0 {
            1
        } else {
            0
        };
        nested.planned_schedule = nested.schedule.clone();
        assert!(!property_scene_plan_is_sealed(&opaque));

        let mut topology = build();
        let nested = scaffold(&mut topology);
        let NestedScrollSceneScheduledStep::HostBefore { artifact, .. } =
            &mut nested.schedule.steps[0]
        else {
            unreachable!()
        };
        artifact.identity.owner_topology.push(PaintOwnerSnapshot {
            owner: nested.admission.inner_boundary_root,
            parent: None,
        });
        nested.planned_schedule = nested.schedule.clone();
        assert!(!property_scene_plan_is_sealed(&topology));

        let mut receiver_plan = build();
        let nested = scaffold(&mut receiver_plan);
        let NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
            &mut nested.schedule.steps[2]
        else {
            unreachable!()
        };
        receiver.projected_output = PropertyTreeState::default();
        nested.planned_schedule = nested.schedule.clone();
        assert!(!property_scene_plan_is_sealed(&receiver_plan));

        let mut receiver_artifact = build();
        let nested = scaffold(&mut receiver_artifact);
        let NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
            &mut nested.schedule.steps[2]
        else {
            unreachable!()
        };
        receiver.artifact.identity.chunks[0].properties = PropertyTreeState::default();
        nested.planned_schedule = nested.schedule.clone();
        assert!(!property_scene_plan_is_sealed(&receiver_artifact));

        let mut revision = build();
        let nested = scaffold(&mut revision);
        let NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
            &mut nested.schedule.steps[2]
        else {
            unreachable!()
        };
        receiver.artifact.identity.chunks[0]
            .content_revision
            .topology_revision += 1;
        nested.planned_schedule = nested.schedule.clone();
        assert!(!property_scene_plan_is_sealed(&revision));

        let mut clip_snapshot = build();
        let nested = scaffold(&mut clip_snapshot);
        let NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
            &mut nested.schedule.steps[2]
        else {
            unreachable!()
        };
        receiver.artifact.identity.clip_nodes[0].generation += 1;
        nested.planned_schedule = nested.schedule.clone();
        assert!(!property_scene_plan_is_sealed(&clip_snapshot));

        let mut effect_snapshot = build();
        let nested = scaffold(&mut effect_snapshot);
        let NestedScrollSceneScheduledStep::HostBefore { artifact, .. } =
            &mut nested.schedule.steps[0]
        else {
            unreachable!()
        };
        artifact.identity.effect_nodes.push(EffectNodeSnapshot {
            id: EffectNodeId(nested.admission.content_leaf),
            owner: nested.admission.content_leaf,
            parent: None,
            opacity: 1.0,
            generation: 1,
        });
        nested.planned_schedule = nested.schedule.clone();
        assert!(!property_scene_plan_is_sealed(&effect_snapshot));

        let mut payload = build();
        let nested = scaffold(&mut payload);
        let NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
            &mut nested.schedule.steps[2]
        else {
            unreachable!()
        };
        receiver.artifact.identity.chunks[0].payload_identity =
            if receiver.artifact.identity.chunks[0].payload_identity
                == crate::view::paint::PaintPayloadIdentity::None
            {
                crate::view::paint::PaintPayloadIdentity::PreparedTexts(Arc::from([]))
            } else {
                crate::view::paint::PaintPayloadIdentity::None
            };
        nested.planned_schedule = nested.schedule.clone();
        assert!(!property_scene_plan_is_sealed(&payload));

        let mut duplicate_chunk = build();
        let nested = scaffold(&mut duplicate_chunk);
        let NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
            &mut nested.schedule.steps[2]
        else {
            unreachable!()
        };
        receiver
            .artifact
            .identity
            .chunks
            .push(receiver.artifact.identity.chunks[0].clone());
        nested.planned_schedule = nested.schedule.clone();
        assert!(!property_scene_plan_is_sealed(&duplicate_chunk));
    }

    #[test]
    fn direct_scroll_transform_admission_is_isolated_from_the_b0_oracle() {
        let (plain_arena, plain_root, _, _) =
            property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::FrameRootScroll);
        let plain_node = plain_arena.get(plain_root).expect("plain scroll host");
        let plain = plain_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .expect("plain scroll host element");
        assert!(
            plain
                .exact_retained_scroll_host_admission(plain_root, &plain_arena, 1.0)
                .is_some()
        );
        assert!(
            plain
                .exact_retained_scroll_transform_host_admission(plain_root, &plain_arena, 1.0)
                .is_none()
        );

        let (transform_arena, transform_root, _, _) =
            property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
        let transform_node = transform_arena
            .get(transform_root)
            .expect("scroll-transform host");
        let transform = transform_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .expect("scroll-transform host element");
        assert!(
            transform
                .exact_retained_scroll_host_admission(transform_root, &transform_arena, 1.0)
                .is_none()
        );
        let admission = transform
            .exact_retained_scroll_transform_host_admission(transform_root, &transform_arena, 1.0)
            .expect("direct transformed content admission");
        assert_eq!(admission.boundary_root, transform_root);
        assert_eq!(admission.transform_content, transform.children()[0]);
    }

    #[test]
    fn direct_scroll_transform_recorders_seal_host_marker_overlay_and_offset_zero_content() {
        let (arena, root, properties, generations) =
            property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
        let root_node = arena.get(root).expect("scroll host");
        let root_element = root_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .expect("scroll host element");
        let admission = root_element
            .exact_retained_scroll_transform_host_admission(root, &arena, 1.0)
            .expect("direct S->T admission");
        let child = admission.transform_content;
        let scroll = properties
            .scroll_snapshot_for(ScrollNodeId(root))
            .expect("scroll snapshot");
        let clip_id = ClipNodeId {
            owner: root,
            role: ClipNodeRole::ContentsClip,
        };
        let clip = properties
            .clip_snapshot_for(Some(clip_id))
            .and_then(|chain| (chain.len() == 1).then(|| chain[0]))
            .expect("contents clip");
        let marker = PlannedBoundary {
            root: child,
            stable_id: admission.transform_content_stable_id,
            kind: PlannedBoundaryKind::Transform(TransformNodeId(child)),
        };
        let host_witness = PaintBakedScrollHostWitness::new(root, child, scroll, clip_id)
            .expect("baked host witness");
        let host_steps = super::super::frame_recorder::record_scroll_transform_host_steps_for_plan(
            &arena,
            root,
            &FxHashSet::default(),
            &properties,
            &generations,
            host_witness,
            [0.0, 0.0],
            marker,
        )
        .expect("exact H-marker-O host recording");
        assert!(matches!(
            host_steps.as_slice(),
            [
                super::super::frame_recorder::RecordedTransformSurfaceStep::Artifact(_),
                super::super::frame_recorder::RecordedTransformSurfaceStep::Boundary(found),
                super::super::frame_recorder::RecordedTransformSurfaceStep::Artifact(_),
            ] if *found == marker
        ));

        let content_witness = PaintScrollContentWitness::new(root, child, scroll, clip)
            .expect("scroll-content witness");
        let content_steps =
            super::super::frame_recorder::record_scroll_transform_content_steps_for_plan(
                &arena,
                child,
                &FxHashSet::default(),
                &properties,
                &generations,
                PaintTransformSurfaceWitness::canonical_root(child),
                content_witness,
            )
            .expect("offset-zero transformed content recording");
        let [super::super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact)] =
            content_steps.as_slice()
        else {
            panic!("one transformed-content artifact")
        };
        assert!(artifact.chunks.iter().all(|chunk| {
            chunk.properties.transform == Some(TransformNodeId(child))
                && chunk.properties.scroll.is_none()
                && chunk.properties.clip.is_none()
        }));
    }

    #[test]
    fn direct_scroll_transform_schedule_seals_only_s_then_direct_translation_content() {
        let (arena, root, properties, generations) =
            property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
        let plan = super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            1.0,
            [0.0, 0.0],
            None,
        )
        .expect("exact [S, T-content] schedule");
        assert!(plan.is_canonical());

        assert!(
            super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
                &arena,
                &[root],
                &FxHashSet::default(),
                &properties,
                &generations,
                2.0,
                [0.0, 0.0],
                None,
            )
            .is_err()
        );

        let (plain_arena, plain_root, plain_properties, plain_generations) =
            property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::FrameRootScroll);
        assert!(
            super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
                &plain_arena,
                &[plain_root],
                &FxHashSet::default(),
                &plain_properties,
                &plain_generations,
                1.0,
                [0.0, 0.0],
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn direct_scroll_transform_geometry_freezes_offset_zero_raster_and_one_xy_projection() {
        let (arena, root, _, _) =
            property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
        let child = arena.children_of(root)[0];
        {
            let mut root_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, root);
            root_element.layout_state.layout_position.x = 10.0;
            root_element.layout_state.layout_position.y = 20.0;
            root_element.layout_state.content_size.width = 240.0;
            root_element.set_scroll_offset((3.5, 47.25));
            root_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        {
            let mut child_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, child);
            child_element.layout_state.layout_position.x = 6.5;
            child_element.layout_state.layout_position.y = -27.25;
            child_element.layout_state.layout_size.width = 240.0;
            child_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        arena.refresh_subtree_dirty_cache(root);
        let observation = {
            let node = arena.get(root).unwrap();
            node.element.scroll_geometry_observation(root, &arena)
        };
        let crate::view::base_component::ScrollGeometryObservation::Exact(observation) =
            observation
        else {
            panic!("{observation:?}")
        };
        assert_eq!(
            observation.offset.map(f32::to_bits),
            [3.5, 47.25].map(f32::to_bits)
        );
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        assert!(
            properties.validation_errors.is_empty(),
            "{:?}",
            properties.validation_errors
        );
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        let scaffold = super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            1.0,
            [0.0, 0.0],
            None,
        )
        .expect("nonzero-origin full-2D S->T scaffold");
        assert_eq!(scaffold.overlay_op_count_for_test(), 0);
        let geometry = super::super::scroll_scene::plan_direct_scroll_transform_geometry(
            &arena,
            scaffold,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            super::super::scroll_scene::ScrollSceneSingleTextureBudget::new(u32::MAX, u64::MAX)
                .unwrap(),
        )
        .expect("single direct transformed-content backing");
        assert!(geometry.is_canonical());
        assert_eq!(
            [
                geometry.raster_bounds().x,
                geometry.raster_bounds().y,
                geometry.raster_bounds().width,
                geometry.raster_bounds().height,
            ]
            .map(f32::to_bits),
            [10.0, 20.0, 240.0, 240.0].map(f32::to_bits),
        );
        let params = geometry.composite_params();
        assert_eq!(
            params.bounds.map(f32::to_bits),
            [9.5, -27.25, 240.0, 240.0].map(f32::to_bits),
        );
        assert_ne!(
            params.bounds.map(f32::to_bits),
            [13.0, 20.0, 240.0, 240.0].map(f32::to_bits),
            "scroll projection must not be omitted",
        );
        assert_ne!(
            params.bounds.map(f32::to_bits),
            [6.0, -74.5, 240.0, 240.0].map(f32::to_bits),
            "scroll projection must not be applied twice",
        );
        assert_eq!(
            params
                .quad_positions
                .expect("direct transformed quad")
                .map(|point| point.map(f32::to_bits)),
            [
                [9.5, 212.75],
                [249.5, 212.75],
                [249.5, -27.25],
                [9.5, -27.25],
            ]
            .map(|point| point.map(f32::to_bits)),
        );
        assert_eq!(
            params.uv_bounds.expect("offset-zero UV").map(f32::to_bits),
            [10.0, 20.0, 240.0, 240.0].map(f32::to_bits),
        );
        assert_eq!(params.scissor_rect, Some([10, 20, 120, 90]));
    }

    #[test]
    fn direct_scroll_transform_geometry_rejects_rotation_and_tiling_fallback() {
        let (arena, root, _, _) =
            property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        let scaffold = super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            1.0,
            [0.0, 0.0],
            None,
        )
        .expect("direct translation scaffold");
        assert!(
            super::super::scroll_scene::plan_direct_scroll_transform_geometry(
                &arena,
                scaffold,
                wgpu::TextureFormat::Bgra8UnormSrgb,
                super::super::scroll_scene::ScrollSceneSingleTextureBudget::new(64, u64::MAX)
                    .unwrap(),
            )
            .is_err()
        );

        let child = arena.children_of(root)[0];
        crate::view::test_support::get_element_mut::<Element>(&arena, child)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_rotation_z(0.25)));
        let mut rotated_properties = PropertyTrees::default();
        rotated_properties.sync(&arena, &[root]);
        let mut rotated_generations = PaintGenerationTracker::default();
        rotated_generations.sync(&arena, &[root], &rotated_properties);
        assert!(
            super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
                &arena,
                &[root],
                &FxHashSet::default(),
                &rotated_properties,
                &rotated_generations,
                1.0,
                [0.0, 0.0],
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn direct_scroll_transform_frozen_artifact_geometry_and_backing_tamper_fail_closed() {
        let (arena, root, properties, generations) =
            property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
        let scaffold = super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            1.0,
            [0.0, 0.0],
            None,
        )
        .expect("sealed direct S->T scaffold");
        let mut artifact_tamper = scaffold.clone();
        artifact_tamper.tamper_content_artifact_bounds_for_test();
        assert!(!artifact_tamper.is_canonical());
        assert!(super::super::scroll_scene::plan_direct_scroll_transform_geometry(
            &arena,
            artifact_tamper,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            super::super::scroll_scene::ScrollSceneSingleTextureBudget::new(
                u32::MAX,
                u64::MAX,
            )
            .unwrap(),
        )
        .is_err());
        let mut host_tamper = scaffold.clone();
        host_tamper.tamper_host_artifact_bounds_for_test();
        assert!(!host_tamper.is_canonical());

        let geometry = super::super::scroll_scene::plan_direct_scroll_transform_geometry(
            &arena,
            scaffold.clone(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            super::super::scroll_scene::ScrollSceneSingleTextureBudget::new(u32::MAX, u64::MAX)
                .unwrap(),
        )
        .expect("sealed direct geometry");
        let mut geometry_tamper = geometry.clone();
        geometry_tamper.tamper_geometry_seal_for_test();
        assert!(!geometry_tamper.is_canonical());
        let mut backing_tamper = geometry;
        backing_tamper.tamper_backing_seal_for_test();
        assert!(!backing_tamper.is_canonical());

        let child = arena.children_of(root)[0];
        crate::view::test_support::get_element_mut::<Element>(&arena, child)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                4.0, 0.0, 0.0,
            ))));
        assert!(matches!(
            super::super::scroll_scene::plan_direct_scroll_transform_geometry(
                &arena,
                scaffold,
                wgpu::TextureFormat::Bgra8UnormSrgb,
                super::super::scroll_scene::ScrollSceneSingleTextureBudget::new(
                    u32::MAX,
                    u64::MAX,
                )
                .unwrap(),
            ),
            Err(super::super::scroll_scene::PropertyScrollScenePlanError::LiveSnapshotDrift)
        ));
    }

    #[test]
    fn direct_scroll_transform_transaction_is_one_generic_t_and_no_scroll_group() {
        let (arena, root, properties, generations) =
            property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
        let scaffold = super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            1.0,
            [0.0, 0.0],
            None,
        )
        .expect("direct S->T scaffold");
        let geometry = super::super::scroll_scene::plan_direct_scroll_transform_geometry(
            &arena,
            scaffold,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            super::super::scroll_scene::ScrollSceneSingleTextureBudget::new(u32::MAX, u64::MAX)
                .unwrap(),
        )
        .expect("direct S->T geometry");
        let transaction =
            super::super::scroll_scene::compile_direct_scroll_transform_transaction(geometry)
                .expect("authority-specific direct S->T transaction");
        assert!(transaction.is_canonical());
        assert_eq!(transaction.transaction_shape_for_test(), [1, 1, 1, 1, 0, 0]);
        let stamp = transaction.stamp_for_test();
        assert_eq!(
            stamp.identity.role,
            crate::view::paint::RetainedSurfaceRasterRole::Transform
        );
        assert!(stamp.scroll_host.is_none());
        assert!(stamp.property_effect.is_none());
        assert!(matches!(
            stamp.ordered_steps.as_slice(),
            [crate::view::paint::RetainedSurfaceRasterStepStamp::ArtifactSpan(_)]
        ));
        let base_stamp = stamp.clone();
        let canonical_transaction = transaction.clone();

        let mut binding_tamper = transaction.clone();
        binding_tamper.tamper_transaction_binding_for_test();
        assert!(!binding_tamper.is_canonical());
        let mut synchronized_tamper = transaction;
        synchronized_tamper.tamper_synchronized_root_contract_for_test();
        assert!(!synchronized_tamper.is_canonical());
        assert!(!synchronized_tamper.inner_transaction_is_canonical_for_test());

        for variant in 0..4 {
            let mut authority_tamper = canonical_transaction.clone();
            authority_tamper.tamper_authority_for_test(variant);
            assert!(!authority_tamper.inner_transaction_is_canonical_for_test());
            assert!(!authority_tamper.is_canonical());
        }
        for variant in 0..3 {
            let mut boundary_tamper = canonical_transaction.clone();
            boundary_tamper.tamper_boundary_for_test(variant);
            assert!(!boundary_tamper.inner_transaction_is_canonical_for_test());
            assert!(!boundary_tamper.is_canonical());
        }
        let mut root_owner_tamper = canonical_transaction.clone();
        root_owner_tamper.tamper_root_owner_for_test();
        assert!(!root_owner_tamper.inner_transaction_is_canonical_for_test());
        assert!(!root_owner_tamper.is_canonical());
        let mut source_tamper = canonical_transaction.clone();
        source_tamper.tamper_synchronized_source_bounds_for_test();
        assert!(!source_tamper.inner_transaction_is_canonical_for_test());
        assert!(!source_tamper.is_canonical());
        let mut descriptor_tamper = canonical_transaction.clone();
        descriptor_tamper.tamper_synchronized_descriptor_for_test();
        assert!(!descriptor_tamper.is_canonical());
        let mut span_tamper = canonical_transaction;
        span_tamper.tamper_synchronized_artifact_span_for_test();
        assert!(!span_tamper.inner_transaction_is_canonical_for_test());
        assert!(!span_tamper.is_canonical());

        let (moved_arena, moved_root, _, _) =
            property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
        let moved_child = moved_arena.children_of(moved_root)[0];
        {
            let mut root_element =
                crate::view::test_support::get_element_mut::<Element>(&moved_arena, moved_root);
            root_element.set_scroll_offset((0.0, 40.0));
            root_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        {
            let mut child_element =
                crate::view::test_support::get_element_mut::<Element>(&moved_arena, moved_child);
            child_element.layout_state.layout_position.y = -40.0;
            child_element.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                glam::Vec3::new(9.0, 0.0, 0.0),
            )));
            child_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        moved_arena.refresh_subtree_dirty_cache(moved_root);
        let mut moved_properties = PropertyTrees::default();
        moved_properties.sync(&moved_arena, &[moved_root]);
        let mut moved_generations = PaintGenerationTracker::default();
        moved_generations.sync(&moved_arena, &[moved_root], &moved_properties);
        let moved_scaffold =
            super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
                &moved_arena,
                &[moved_root],
                &FxHashSet::default(),
                &moved_properties,
                &moved_generations,
                1.0,
                [0.0, 0.0],
                None,
            )
            .expect("moved direct S->T scaffold");
        let moved_geometry = super::super::scroll_scene::plan_direct_scroll_transform_geometry(
            &moved_arena,
            moved_scaffold,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            super::super::scroll_scene::ScrollSceneSingleTextureBudget::new(u32::MAX, u64::MAX)
                .unwrap(),
        )
        .expect("moved direct S->T geometry");
        let moved_transaction =
            super::super::scroll_scene::compile_direct_scroll_transform_transaction(moved_geometry)
                .expect("moved direct S->T transaction");
        assert_eq!(
            moved_transaction.stamp_for_test(),
            &base_stamp,
            "T matrix and S offset stay out of the T raster stamp",
        );
    }

    fn direct_scroll_transform_transaction_from_fixture_for_test(
        arena: &NodeArena,
        root: NodeKey,
        properties: &PropertyTrees,
        generations: &PaintGenerationTracker,
    ) -> super::super::scroll_scene::ValidatedDirectScrollTransformTransaction {
        let scaffold = super::super::scroll_scene::plan_direct_scroll_transform_scene_scaffold(
            arena,
            &[root],
            &FxHashSet::default(),
            properties,
            generations,
            1.0,
            [0.0, 0.0],
            None,
        )
        .unwrap();
        let geometry = super::super::scroll_scene::plan_direct_scroll_transform_geometry(
            &arena,
            scaffold,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            super::super::scroll_scene::ScrollSceneSingleTextureBudget::new(u32::MAX, u64::MAX)
                .unwrap(),
        )
        .unwrap();
        super::super::scroll_scene::compile_direct_scroll_transform_transaction(geometry).unwrap()
    }

    fn exact_direct_scroll_transform_transaction_for_test()
    -> super::super::scroll_scene::ValidatedDirectScrollTransformTransaction {
        let (arena, root, properties, generations) =
            property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
        direct_scroll_transform_transaction_from_fixture_for_test(
            &arena,
            root,
            &properties,
            &generations,
        )
    }

    #[test]
    fn direct_scroll_transform_prepare_freezes_action_before_graph_mutation() {
        let transaction = exact_direct_scroll_transform_transaction_for_test();
        let mut viewport = Viewport::new();
        let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut invalid_graph = FrameGraph::new();
        let invalid = super::super::scroll_scene::prepare_direct_scroll_transform_scene_from_pool(
            &mut viewport,
            transaction.clone(),
            &mut invalid_graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Rgba8Unorm, 1.0),
            [0.0; 4],
            frame_owner,
        );
        assert!(matches!(
            invalid,
            Err(
                super::super::scroll_scene::RetainedPropertyScrollScenePrepareError::ContextMismatch
            )
        ));
        assert_eq!(invalid_graph.declared_persistent_texture_keys().count(), 0);
        assert!(viewport.retained_property_scroll_scene_stage_is_available());

        let mut graph = FrameGraph::new();
        let prepared = super::super::scroll_scene::prepare_direct_scroll_transform_scene_from_pool(
            &mut viewport,
            transaction,
            &mut graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.125, 0.25, 0.5, 1.0],
            frame_owner,
        )
        .expect("direct S->T preflight");
        assert_eq!(
            prepared.action_for_test(),
            crate::view::paint::RetainedSurfaceCompileAction::Reraster
        );
        assert_eq!(prepared.transaction_shape_for_test(), [1, 0]);
        assert_eq!(prepared.graph_declared_key_count_for_test(), 0);
        let parent_terminal = prepared.parent_terminal_for_test();
        let outcome =
            super::super::scroll_scene::emit_prepared_direct_scroll_transform_scene(prepared);
        let (state, _) = outcome.into_parts();
        assert_eq!(state.opaque_rect_order(), parent_terminal);
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::render_pass::ClearPass>()
                .len(),
            2
        );
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::render_pass::texture_composite_pass::TextureCompositePass>()
                .len(),
            1
        );
        assert!(!viewport.retained_property_scroll_scene_stage_is_available());
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true));
    }

    #[test]
    fn direct_scroll_transform_prepare_rejections_are_graph_pool_and_owner_atomic() {
        use super::super::scroll_scene::RetainedPropertyScrollScenePrepareError as PrepareError;

        macro_rules! reject_case {
            ($transaction:expr, $graph:expr, $ctx:expr, $clear:expr, $expected:expr) => {{
                let mut viewport = Viewport::new();
                let owner = viewport.begin_retained_surface_frame_stage().unwrap();
                let mut graph = $graph;
                let graph_before = graph.build_state_snapshot_for_test();
                let pool_before = viewport.retained_surface_transaction_shape_for_test();
                assert_eq!(
                    super::super::scroll_scene::prepare_direct_scroll_transform_scene_from_pool(
                        &mut viewport,
                        $transaction,
                        &mut graph,
                        $ctx,
                        $clear,
                        owner,
                    )
                    .err(),
                    Some($expected)
                );
                assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
                assert_eq!(
                    viewport.retained_surface_transaction_shape_for_test(),
                    pool_before
                );
                assert!(viewport.retained_surface_frame_stage_owner_is_active(owner));
                assert!(viewport.retained_property_scroll_scene_stage_is_available());
                assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), false));
            }};
        }

        let default_ctx =
            || UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        let base = exact_direct_scroll_transform_transaction_for_test();

        let mut descriptor = base.clone();
        descriptor.tamper_synchronized_descriptor_for_test();
        reject_case!(
            descriptor,
            FrameGraph::new(),
            default_ctx(),
            [0.0; 4],
            PrepareError::BoundaryDrift
        );
        for pair in [false, true] {
            let mut budget = base.clone();
            budget.tamper_synchronized_backing_budget_for_test(pair);
            reject_case!(
                budget,
                FrameGraph::new(),
                default_ctx(),
                [0.0; 4],
                PrepareError::DescriptorPair
            );
        }

        let (color_key, color_desc, depth_desc) = base.backing_for_test();
        let depth_key = color_key.depth_stencil().unwrap();
        for (key, desc) in [(color_key, color_desc), (depth_key, depth_desc)] {
            let mut graph = FrameGraph::new();
            let _ = graph.declare_persistent_texture_internal::<()>(desc, key);
            reject_case!(
                base.clone(),
                graph,
                default_ctx(),
                [0.0; 4],
                PrepareError::PersistentKeyAlreadyDeclared(color_key)
            );
        }

        let mut offset = default_ctx();
        offset.set_paint_offset([0.25, 0.0]);
        let mut scissor = default_ctx();
        scissor.replace_scissor_rect(Some([0, 0, 1, 1]));
        let mut cursor = default_ctx();
        let _ = cursor.next_opaque_rect_order();
        let mut transform = default_ctx();
        transform.set_current_render_transform(Some(glam::Mat4::IDENTITY));
        let contexts = [
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 2.0),
            offset,
            scissor,
            cursor,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Rgba8Unorm, 1.0),
            transform,
        ];
        for ctx in contexts {
            reject_case!(
                base.clone(),
                FrameGraph::new(),
                ctx,
                [0.0; 4],
                PrepareError::ContextMismatch
            );
        }
        reject_case!(
            base.clone(),
            FrameGraph::new(),
            default_ctx(),
            [f32::NAN, 0.0, 0.0, 0.0],
            PrepareError::ContextMismatch
        );

        let mut foreign_graph = FrameGraph::new();
        let mut foreign_ctx = default_ctx();
        let foreign_target = foreign_ctx.allocate_target(&mut foreign_graph);
        foreign_ctx.set_current_target(foreign_target);
        reject_case!(
            base.clone(),
            FrameGraph::new(),
            foreign_ctx,
            [0.0; 4],
            PrepareError::ParentTarget
        );

        let mut stale_viewport = Viewport::new();
        let stale_owner = stale_viewport.begin_retained_surface_frame_stage().unwrap();
        assert!(
            stale_viewport.finish_retained_surface_transaction_for_frame(Some(stale_owner), false)
        );
        let mut stale_graph = FrameGraph::new();
        let graph_before = stale_graph.build_state_snapshot_for_test();
        let pool_before = stale_viewport.retained_surface_transaction_shape_for_test();
        assert_eq!(
            super::super::scroll_scene::prepare_direct_scroll_transform_scene_from_pool(
                &mut stale_viewport,
                base,
                &mut stale_graph,
                default_ctx(),
                [0.0; 4],
                stale_owner,
            )
            .err(),
            Some(PrepareError::StageUnavailable)
        );
        assert_eq!(stale_graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            stale_viewport.retained_surface_transaction_shape_for_test(),
            pool_before
        );

        let mut occupied_viewport = Viewport::new();
        let occupied_owner = occupied_viewport
            .begin_retained_surface_frame_stage()
            .unwrap();
        let mut seed_graph = FrameGraph::new();
        let seed = super::super::scroll_scene::prepare_direct_scroll_transform_scene_from_pool(
            &mut occupied_viewport,
            exact_direct_scroll_transform_transaction_for_test(),
            &mut seed_graph,
            default_ctx(),
            [0.0; 4],
            occupied_owner,
        )
        .unwrap();
        let _ = super::super::scroll_scene::emit_prepared_direct_scroll_transform_scene(seed);
        assert!(!occupied_viewport.retained_property_scroll_scene_stage_is_available());
        let mut occupied_graph = FrameGraph::new();
        let graph_before = occupied_graph.build_state_snapshot_for_test();
        let pool_before = occupied_viewport.retained_surface_transaction_shape_for_test();
        let owner_active_before =
            occupied_viewport.retained_surface_frame_stage_owner_is_active(occupied_owner);
        assert_eq!(
            super::super::scroll_scene::prepare_direct_scroll_transform_scene_from_pool(
                &mut occupied_viewport,
                exact_direct_scroll_transform_transaction_for_test(),
                &mut occupied_graph,
                default_ctx(),
                [0.0; 4],
                occupied_owner,
            )
            .err(),
            Some(PrepareError::StageUnavailable)
        );
        assert_eq!(occupied_graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            occupied_viewport.retained_surface_transaction_shape_for_test(),
            pool_before
        );
        assert_eq!(
            occupied_viewport.retained_surface_frame_stage_owner_is_active(occupied_owner),
            owner_active_before
        );
        assert!(
            occupied_viewport
                .finish_retained_surface_transaction_for_frame(Some(occupied_owner), true)
        );
    }

    #[test]
    fn direct_scroll_transform_action_matrix_keeps_composite_inputs_dynamic() {
        let (arena, root, mut properties, mut generations) =
            property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
        let child = arena.children_of(root)[0];
        let ctx = || UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        let mut viewport = Viewport::new();

        let cold_transaction = direct_scroll_transform_transaction_from_fixture_for_test(
            &arena,
            root,
            &properties,
            &generations,
        );
        let color_key = cold_transaction.backing_for_test().0;
        let cold_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut cold_graph = FrameGraph::new();
        let cold = super::super::scroll_scene::prepare_direct_scroll_transform_scene_from_pool(
            &mut viewport,
            cold_transaction,
            &mut cold_graph,
            ctx(),
            [0.0; 4],
            cold_owner,
        )
        .unwrap();
        assert_eq!(
            cold.action_for_test(),
            crate::view::paint::RetainedSurfaceCompileAction::Reraster
        );
        let _ = super::super::take_artifact_compile_count();
        let _ = super::super::scroll_scene::emit_prepared_direct_scroll_transform_scene(cold);
        assert_eq!(super::super::take_artifact_compile_count(), 3);
        assert_eq!(
            cold_graph
                .test_graphics_passes::<crate::view::render_pass::ClearPass>()
                .len(),
            2
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true));

        let mut run =
            |transaction: super::super::scroll_scene::ValidatedDirectScrollTransformTransaction,
             expected,
             expected_content_clears| {
                let owner = viewport.begin_retained_surface_frame_stage().unwrap();
                let mut graph = FrameGraph::new();
                let mut prepared =
                    super::super::scroll_scene::prepare_direct_scroll_transform_scene_from_pool(
                        &mut viewport,
                        transaction,
                        &mut graph,
                        ctx(),
                        [0.0; 4],
                        owner,
                    )
                    .unwrap();
                prepared.refresh_action_from_committed_test_pool();
                assert_eq!(prepared.action_for_test(), expected);
                let composite = prepared.composite_params_for_test();
                let _ = super::super::take_artifact_compile_count();
                let _ = super::super::scroll_scene::emit_prepared_direct_scroll_transform_scene(
                    prepared,
                );
                assert_eq!(
                    super::super::take_artifact_compile_count(),
                    if expected == crate::view::paint::RetainedSurfaceCompileAction::Reraster {
                        3
                    } else {
                        2
                    }
                );
                let composites = graph.test_graphics_passes::<
                    crate::view::render_pass::texture_composite_pass::TextureCompositePass,
                >();
                let matching = composites
                    .iter()
                    .filter(|pass| {
                        let snapshot = pass.test_snapshot();
                        snapshot.bounds_bits == composite.bounds.map(f32::to_bits)
                            && snapshot.quad_position_bits
                                == composite
                                    .quad_positions
                                    .map(|quad| quad.map(|point| point.map(f32::to_bits)))
                            && snapshot.uv_bounds_bits
                                == composite.uv_bounds.map(|uv| uv.map(f32::to_bits))
                            && snapshot.explicit_scissor_rect == composite.scissor_rect
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    matching.len(),
                    1,
                    "final direct S->T composite is exact-once"
                );
                let content_target = matching[0]
                    .test_snapshot()
                    .source_handle
                    .expect("direct S->T composite samples its persistent T target");
                let content_clears = graph
                    .test_graphics_passes::<crate::view::render_pass::ClearPass>()
                    .iter()
                    .filter(|pass| pass.test_snapshot().output_target == Some(content_target))
                    .count();
                assert_eq!(content_clears, expected_content_clears);
                assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
            };
        let reuse = crate::view::paint::RetainedSurfaceCompileAction::Reuse;
        let reraster = crate::view::paint::RetainedSurfaceCompileAction::Reraster;

        run(
            direct_scroll_transform_transaction_from_fixture_for_test(
                &arena,
                root,
                &properties,
                &generations,
            ),
            reuse,
            0,
        );

        crate::view::test_support::get_element_mut::<Element>(&arena, child)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                11.0, 4.0, 0.0,
            ))));
        arena.refresh_subtree_dirty_cache(root);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        run(
            direct_scroll_transform_transaction_from_fixture_for_test(
                &arena,
                root,
                &properties,
                &generations,
            ),
            reuse,
            0,
        );

        {
            let mut root_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, root);
            root_element.set_scroll_offset((0.0, 37.0));
            root_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        {
            let mut child_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, child);
            child_element.layout_state.layout_position.y = -37.0;
            child_element.layout_state.layout_inner_position.y = -37.0;
            child_element.layout_state.layout_flow_position.y = -37.0;
            child_element.layout_state.layout_flow_inner_position.y = -37.0;
            child_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        arena.refresh_subtree_dirty_cache(root);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        run(
            direct_scroll_transform_transaction_from_fixture_for_test(
                &arena,
                root,
                &properties,
                &generations,
            ),
            reuse,
            0,
        );

        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_sampled_scrollbar_alpha_for_test(1.0);
        arena.refresh_subtree_dirty_cache(root);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        run(
            direct_scroll_transform_transaction_from_fixture_for_test(
                &arena,
                root,
                &properties,
                &generations,
            ),
            reuse,
            0,
        );

        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_background_color_value(Color::rgb(18, 36, 54));
        arena.refresh_subtree_dirty_cache(root);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        run(
            direct_scroll_transform_transaction_from_fixture_for_test(
                &arena,
                root,
                &properties,
                &generations,
            ),
            reuse,
            0,
        );

        {
            let mut root_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, root);
            root_element.layout_state.layout_size.height = 72.0;
            root_element.layout_state.layout_inner_size.height = 72.0;
            root_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        arena.refresh_subtree_dirty_cache(root);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        run(
            direct_scroll_transform_transaction_from_fixture_for_test(
                &arena,
                root,
                &properties,
                &generations,
            ),
            reuse,
            0,
        );

        crate::view::test_support::get_element_mut::<Element>(&arena, child)
            .set_background_color_value(Color::rgb(72, 48, 24));
        arena.refresh_subtree_dirty_cache(root);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        run(
            direct_scroll_transform_transaction_from_fixture_for_test(
                &arena,
                root,
                &properties,
                &generations,
            ),
            reraster,
            1,
        );

        drop(run);
        viewport.forget_retained_surface_pair_witness_for_test(color_key);
        let pair_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut pair_graph = FrameGraph::new();
        let mut pair = super::super::scroll_scene::prepare_direct_scroll_transform_scene_from_pool(
            &mut viewport,
            direct_scroll_transform_transaction_from_fixture_for_test(
                &arena,
                root,
                &properties,
                &generations,
            ),
            &mut pair_graph,
            ctx(),
            [0.0; 4],
            pair_owner,
        )
        .unwrap();
        pair.refresh_action_from_committed_test_pool();
        assert_eq!(pair.action_for_test(), reraster);
        let composite = pair.composite_params_for_test();
        let _ = super::super::take_artifact_compile_count();
        let _ = super::super::scroll_scene::emit_prepared_direct_scroll_transform_scene(pair);
        assert_eq!(super::super::take_artifact_compile_count(), 3);
        let pair_composites = pair_graph
            .test_graphics_passes::<crate::view::render_pass::texture_composite_pass::TextureCompositePass>();
        let matching = pair_composites
            .iter()
            .filter(|pass| {
                let snapshot = pass.test_snapshot();
                snapshot.bounds_bits == composite.bounds.map(f32::to_bits)
                    && snapshot.quad_position_bits
                        == composite
                            .quad_positions
                            .map(|quad| quad.map(|point| point.map(f32::to_bits)))
                    && snapshot.uv_bounds_bits == composite.uv_bounds.map(|uv| uv.map(f32::to_bits))
                    && snapshot.explicit_scissor_rect == composite.scissor_rect
            })
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1);
        let content_target = matching[0].test_snapshot().source_handle.unwrap();
        assert_eq!(
            pair_graph
                .test_graphics_passes::<crate::view::render_pass::ClearPass>()
                .iter()
                .filter(|pass| pass.test_snapshot().output_target == Some(content_target))
                .count(),
            1
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(pair_owner), true));
    }

    #[test]
    fn property_scroll_interleave_scaffold_seals_only_supported_planning_shapes() {
        for shape in [
            ScrollInterleaveFixtureShape::FrameRootScroll,
            ScrollInterleaveFixtureShape::TransformScroll,
            ScrollInterleaveFixtureShape::EffectScroll,
            ScrollInterleaveFixtureShape::TransformEffectScroll,
        ] {
            let (arena, root, properties, generations) = property_scroll_interleave_fixture(shape);
            let plan = plan_property_scroll_interleave_scaffold_with_context(
                &arena,
                &[root],
                &FxHashSet::default(),
                &properties,
                &generations,
                TransformSurfacePlanContext::default(),
            )
            .expect("supported B4-0 planning grammar");
            assert!(property_scene_plan_is_sealed(&plan));
            assert!(plan.property_scene_transaction_witness().is_none());
            assert!(plan.property_scene_context().is_none());
            let scaffold = plan
                .property_scene_seal
                .as_ref()
                .and_then(|seal| seal.scroll_schedule_scaffold.as_ref())
                .expect("sealed scroll schedule");
            assert_eq!(scaffold.boundaries.len(), 1);
            let boundary = &scaffold.boundaries[0];
            assert_eq!(
                boundary.phase.host_before.phase,
                PropertyScrollPhaseKind::HostBeforeChildren
            );
            assert_eq!(
                boundary.phase.content_gap.phase,
                PropertyScrollPhaseKind::DetachedContentComposite
            );
            assert_eq!(
                boundary.phase.overlay_after.phase,
                PropertyScrollPhaseKind::OverlayAfterChildren
            );
            assert_eq!(
                boundary.consumed_properties.projected_output,
                PropertyTreeState::default()
            );
        }
    }

    #[test]
    fn property_effect_scroll_checkpoint_freezes_cutout_geometry_and_effect_neutral_identity() {
        let (arena, root, properties, generations) =
            property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::EffectScroll);
        let plan = plan_property_scroll_interleave_scaffold_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::default(),
        )
        .expect("lifecycle-independent E->S schedule");
        let scaffold = plan.property_scroll_planning_scaffold().unwrap();
        assert!(property_scene_plan_is_sealed(&plan));
        assert!(scaffold.receiver_insertions.is_empty());
        assert!(scaffold.effect_receiver_insertions.len() <= 1);
        assert!(matches!(
            scaffold.schedule.steps.as_slice(),
            [
                PropertySceneScheduledStep::RetainedSurface {
                    boundary: PropertyScheduledSurfaceBoundary::Effect(_),
                    parent: None,
                },
                PropertySceneScheduledStep::ScrollBoundary {
                    basis: ScrollCompositeBasis::Effect(_),
                    ..
                }
            ]
        ));
    }

    #[test]
    fn property_effect_scroll_checkpoint_rejects_raster_and_marker_drift() {
        let (arena, root, properties, generations) =
            property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::EffectScroll);
        let build = || {
            plan_property_scroll_interleave_scaffold_with_context(
                &arena,
                &[root],
                &FxHashSet::default(),
                &properties,
                &generations,
                TransformSurfacePlanContext::default(),
            )
            .unwrap()
        };
        let mut schedule = build();
        let scaffold = schedule
            .property_scene_seal
            .as_mut()
            .unwrap()
            .scroll_schedule_scaffold
            .as_mut()
            .unwrap();
        scaffold.schedule.steps.swap(0, 1);
        assert!(!property_scene_plan_is_sealed(&schedule));
    }

    #[test]
    fn property_transform_effect_scroll_insertion_freezes_nested_receivers_and_stack() {
        let (arena, root, properties, generations) =
            property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::TransformEffectScroll);
        let plan = plan_property_scroll_interleave_scaffold_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::default(),
        )
        .expect("exact T->E->S planning scaffold");
        let scaffold = plan.property_scroll_planning_scaffold().unwrap();
        assert!(scaffold.receiver_insertions.is_empty());
        assert!(scaffold.effect_receiver_insertions.is_empty());
        let [insertion] = scaffold.transform_effect_receiver_insertions.as_slice() else {
            panic!("exact T->E->S owns one nested insertion")
        };
        assert!(
            crate::view::paint::compiler::direct_translation_bits(
                insertion.outer_geometry.viewport_transform
            )
            .is_some()
        );
        assert!(
            insertion.outer_geometry.source_bounds.width
                > f32::from_bits(insertion.inner.raster_bounds_bits[2])
        );
        assert_eq!(
            insertion.outer_geometry.source_bounds.y.to_bits(),
            0.0_f32.to_bits()
        );
        assert!(insertion.outer_geometry.source_bounds.height < 240.0);
        assert_eq!(insertion.inner.receiver.parent, None);
        assert_eq!(
            insertion.inner.artifact_contract.live_effect_chain(),
            [insertion.inner.receiver]
        );
        let boundary = &scaffold.boundaries[0];
        assert!(matches!(
            boundary.consumed_properties.entries.as_slice(),
            [
                ConsumedPropertyEntry {
                    boundary: ConsumedPropertyBoundary::Transform(_),
                    ..
                },
                ConsumedPropertyEntry {
                    boundary: ConsumedPropertyBoundary::Effect(_),
                    ..
                },
                ConsumedPropertyEntry {
                    boundary: ConsumedPropertyBoundary::ScrollContents { .. },
                    ..
                }
            ]
        ));
        assert_eq!(
            boundary.consumed_properties.projected_output,
            PropertyTreeState::default()
        );
        assert!(property_scene_plan_is_sealed(&plan));
    }

    #[test]
    fn property_scroll_interleave_scaffold_rejects_scroll_descendants_and_colocation() {
        for shape in [
            ScrollInterleaveFixtureShape::ScrollTransform,
            ScrollInterleaveFixtureShape::CoLocatedTransformScroll,
        ] {
            let (arena, root, properties, generations) = property_scroll_interleave_fixture(shape);
            let error = plan_property_scroll_interleave_scaffold_with_context(
                &arena,
                &[root],
                &FxHashSet::default(),
                &properties,
                &generations,
                TransformSurfacePlanContext::default(),
            )
            .expect_err("unsupported interleave must fail closed");
            assert!(error.reasons.iter().any(|reason| matches!(
                reason,
                FramePaintPlanRejection::UnsupportedPropertyInterleave(_)
                    | FramePaintPlanRejection::ScrollBoundary(_)
            )));
        }
    }

    #[test]
    fn property_scroll_interleave_scaffold_seal_rejects_schedule_stack_and_phase_drift() {
        let (arena, root, properties, generations) =
            property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::TransformEffectScroll);
        let build = || {
            plan_property_scroll_interleave_scaffold_with_context(
                &arena,
                &[root],
                &FxHashSet::default(),
                &properties,
                &generations,
                TransformSurfacePlanContext::default(),
            )
            .expect("sealed scaffold")
        };
        let mut schedule = build();
        schedule
            .property_scene_seal
            .as_mut()
            .unwrap()
            .scroll_schedule_scaffold
            .as_mut()
            .unwrap()
            .schedule
            .steps
            .swap(0, 1);
        assert!(!property_scene_plan_is_sealed(&schedule));

        let mut stack = build();
        stack
            .property_scene_seal
            .as_mut()
            .unwrap()
            .scroll_schedule_scaffold
            .as_mut()
            .unwrap()
            .boundaries[0]
            .consumed_properties
            .entries[0]
            .projected_after = PropertyTreeState::default();
        assert!(!property_scene_plan_is_sealed(&stack));

        let mut phase = build();
        phase
            .property_scene_seal
            .as_mut()
            .unwrap()
            .scroll_schedule_scaffold
            .as_mut()
            .unwrap()
            .boundaries[0]
            .phase
            .overlay_after
            .phase = PropertyScrollPhaseKind::HostBeforeChildren;
        assert!(!property_scene_plan_is_sealed(&phase));

        let mut incomplete = build();
        let incomplete_scaffold = incomplete
            .property_scene_seal
            .as_mut()
            .unwrap()
            .scroll_schedule_scaffold
            .as_mut()
            .unwrap();
        incomplete_scaffold
            .transform_effect_receiver_insertions
            .clear();
        incomplete_scaffold
            .planned_transform_effect_receiver_insertions
            .clear();
        assert!(!property_scene_plan_is_sealed(&incomplete));

        let mut reordered_stack = build();
        let reordered_scaffold = reordered_stack
            .property_scene_seal
            .as_mut()
            .unwrap()
            .scroll_schedule_scaffold
            .as_mut()
            .unwrap();
        reordered_scaffold.boundaries[0]
            .consumed_properties
            .entries
            .swap(0, 1);
        reordered_scaffold.planned_boundaries[0]
            .consumed_properties
            .entries
            .swap(0, 1);
        assert!(!property_scene_plan_is_sealed(&reordered_stack));

        let mut geometry = build();
        let geometry_scaffold = geometry
            .property_scene_seal
            .as_mut()
            .unwrap()
            .scroll_schedule_scaffold
            .as_mut()
            .unwrap();
        geometry_scaffold.transform_effect_receiver_insertions[0]
            .outer_geometry
            .source_bounds
            .width += 1.0;
        geometry_scaffold.planned_transform_effect_receiver_insertions[0]
            .outer_geometry
            .source_bounds
            .width += 1.0;
        assert!(!property_scene_plan_is_sealed(&geometry));
    }

    #[test]
    fn property_scroll_receiver_insertion_seal_rejects_drop_duplicate_reorder_and_retarget() {
        let (arena, root, properties, generations) =
            property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::TransformScroll);
        let build = || {
            let mut plan = plan_property_scroll_interleave_scaffold_with_context(
                &arena,
                &[root],
                &FxHashSet::default(),
                &properties,
                &generations,
                TransformSurfacePlanContext::default(),
            )
            .unwrap();
            let scaffold = plan
                .property_scene_seal
                .as_mut()
                .unwrap()
                .scroll_schedule_scaffold
                .as_mut()
                .unwrap();
            let [
                PropertySceneScheduledStep::RetainedSurface {
                    boundary: PropertyScheduledSurfaceBoundary::Transform(receiver),
                    ..
                },
                PropertySceneScheduledStep::ScrollBoundary {
                    boundary_ordinal, ..
                },
            ] = scaffold.schedule.steps.as_slice()
            else {
                panic!("T->S schedule")
            };
            let boundary = &scaffold.boundaries[*boundary_ordinal as usize];
            let artifact = PropertyScrollReceiverArtifactIdentity {
                owner_topology: Vec::new(),
                clip_nodes: Vec::new(),
                effect_nodes: Vec::new(),
                chunks: Vec::new(),
                op_count: 0,
                opaque_count: 0,
            };
            let cutout = super::super::PlannedBoundary {
                root: boundary.scroll.owner,
                stable_id: arena
                    .get(boundary.scroll.owner)
                    .unwrap()
                    .element
                    .stable_id(),
                kind: super::super::PlannedBoundaryKind::Scroll(boundary.scroll.id),
            };
            let insertion = PropertyScrollReceiverInsertionContract {
                scene_root_ordinal: 0,
                receiver: *receiver,
                receiver_stable_id: arena.get(root).unwrap().element.stable_id(),
                scroll_boundary_ordinal: *boundary_ordinal,
                scroll_cutout: cutout,
                insertion_index: 1,
                before_span: 0..1,
                after_span: 2..3,
                receiver_opaque_before: 0,
                receiver_opaque_after: 0,
                recorded_steps: vec![
                    PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact.clone()),
                    PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(cutout),
                    PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact),
                ],
            };
            scaffold.receiver_insertions = vec![insertion.clone()];
            scaffold.planned_receiver_insertions = vec![insertion];
            assert!(property_scene_plan_is_sealed(&plan));
            plan
        };

        let mut dropped = build();
        dropped
            .property_scene_seal
            .as_mut()
            .unwrap()
            .scroll_schedule_scaffold
            .as_mut()
            .unwrap()
            .receiver_insertions
            .clear();
        assert!(!property_scene_plan_is_sealed(&dropped));

        let mut duplicated = build();
        let scaffold = duplicated
            .property_scene_seal
            .as_mut()
            .unwrap()
            .scroll_schedule_scaffold
            .as_mut()
            .unwrap();
        scaffold
            .receiver_insertions
            .push(scaffold.receiver_insertions[0].clone());
        scaffold.planned_receiver_insertions = scaffold.receiver_insertions.clone();
        assert!(!property_scene_plan_is_sealed(&duplicated));

        let mut reordered = build();
        let insertion = &mut reordered
            .property_scene_seal
            .as_mut()
            .unwrap()
            .scroll_schedule_scaffold
            .as_mut()
            .unwrap()
            .receiver_insertions[0];
        insertion.recorded_steps.swap(0, 1);
        assert!(!property_scene_plan_is_sealed(&reordered));

        let mut retargeted = build();
        let scaffold = retargeted
            .property_scene_seal
            .as_mut()
            .unwrap()
            .scroll_schedule_scaffold
            .as_mut()
            .unwrap();
        let mut wrong_receiver = scaffold.receiver_insertions[0].receiver;
        wrong_receiver.owner = scaffold.boundaries[0].scroll.owner;
        scaffold.receiver_insertions[0].receiver = wrong_receiver;
        assert!(!property_scene_plan_is_sealed(&retargeted));
    }

    fn general_property_scene_fixture() -> GeneralPropertySceneFixture {
        let (mut arena, outer, _before, inner_a, deep, inner_b, _, _) =
            nested_exact_transform_fixture();
        crate::view::test_support::get_element_mut::<Element>(&arena, deep)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                4.0, 0.0, 0.0,
            ))));
        crate::view::test_support::get_element_mut::<Element>(&arena, inner_b)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                5.0, 0.0, 0.0,
            ))));

        let neutral_root = commit_element(
            &mut arena,
            Box::new(Element::new_with_id(0xd1_0001, 130.0, 10.0, 8.0, 8.0)),
        );
        let second_root = commit_element(
            &mut arena,
            Box::new(Element::new_with_id(0xd1_0020, 145.0, 10.0, 12.0, 10.0)),
        );
        let trailing_root = commit_element(
            &mut arena,
            Box::new(Element::new_with_id(0xd1_0030, 166.0, 10.0, 8.0, 8.0)),
        );
        let constraints = LayoutConstraints {
            max_width: 220.0,
            max_height: 140.0,
            viewport_width: 220.0,
            viewport_height: 140.0,
            percent_base_width: Some(220.0),
            percent_base_height: Some(140.0),
        };
        for (root, x, y) in [
            (neutral_root, 130.0, 10.0),
            (second_root, 145.0, 10.0),
            (trailing_root, 166.0, 10.0),
        ] {
            measure_and_place(
                &mut arena,
                root,
                constraints,
                LayoutPlacement {
                    parent_x: x,
                    parent_y: y,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: 220.0,
                    available_height: 140.0,
                    viewport_width: 220.0,
                    viewport_height: 140.0,
                    percent_base_width: Some(220.0),
                    percent_base_height: Some(140.0),
                },
            );
        }
        crate::view::test_support::get_element_mut::<Element>(&arena, second_root)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                6.0, 0.0, 0.0,
            ))));
        let roots = vec![neutral_root, outer, second_root, trailing_root];
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &roots, &properties);
        GeneralPropertySceneFixture {
            arena,
            roots,
            outer,
            inner_a,
            deep,
            inner_b,
            second_root,
            properties,
            generations,
        }
    }

    #[test]
    fn property_scene_plans_multi_root_three_level_and_sibling_transform_forest() {
        let fixture = general_property_scene_fixture();
        let plan = plan_transform_property_scene_with_context(
            &fixture.arena,
            &fixture.roots,
            &FxHashSet::default(),
            &fixture.properties,
            &fixture.generations,
            TransformSurfacePlanContext::default(),
        )
        .expect("general planning-only transform scene");
        assert!(property_scene_plan_is_sealed(&plan));
        let seal = plan
            .property_scene_seal
            .as_ref()
            .expect("general scene seal");
        assert_eq!(seal.surface_count, 5);
        let id_for = |owner| {
            seal.surfaces
                .values()
                .find(|contract| contract.id.owner == owner)
                .expect("surface contract")
                .id
        };
        let outer = id_for(fixture.outer);
        let inner_a = id_for(fixture.inner_a);
        let deep = id_for(fixture.deep);
        let inner_b = id_for(fixture.inner_b);
        let second_root = id_for(fixture.second_root);
        assert_eq!(
            [
                outer.ordinal,
                inner_a.ordinal,
                deep.ordinal,
                inner_b.ordinal,
                second_root.ordinal,
            ],
            [0, 1, 2, 3, 4]
        );
        assert_eq!(seal.surfaces[&outer].parent, None);
        assert_eq!(seal.surfaces[&inner_a].parent, Some(outer));
        assert_eq!(seal.surfaces[&deep].parent, Some(inner_a));
        assert_eq!(seal.surfaces[&inner_b].parent, Some(outer));
        assert_eq!(seal.surfaces[&second_root].parent, None);

        let top_level = plan
            .steps()
            .iter()
            .filter_map(|step| match step {
                PaintPlanStep::RetainedSurface(surface) => Some(surface.boundary_root()),
                PaintPlanStep::ArtifactSpan(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(top_level, vec![fixture.outer, fixture.second_root]);
        let roots = plan.property_scene_roots.as_ref().unwrap();
        assert_eq!(
            roots[0].top_level_step_span.start,
            roots[0].top_level_step_span.end
        );
        assert_eq!(
            roots[3].top_level_step_span.start,
            roots[3].top_level_step_span.end
        );
    }

    fn property_surface_mut(
        steps: &mut [PaintPlanStep],
        owner: NodeKey,
    ) -> Option<&mut RetainedSurfacePlan> {
        for step in steps {
            let PaintPlanStep::RetainedSurface(surface) = step else {
                continue;
            };
            if surface.boundary_root() == owner {
                return Some(surface);
            }
            if let Some(found) = property_surface_mut(&mut surface.raster_steps, owner) {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn property_scene_seal_rejects_topology_identity_reference_and_witness_drift() {
        let fixture = general_property_scene_fixture();
        let base = plan_transform_property_scene_with_context(
            &fixture.arena,
            &fixture.roots,
            &FxHashSet::default(),
            &fixture.properties,
            &fixture.generations,
            TransformSurfacePlanContext::default(),
        )
        .expect("sealed general property scene");

        let mut ordinal = base.clone();
        let seal = ordinal.property_scene_seal.as_mut().unwrap();
        let old_id = seal
            .surfaces
            .keys()
            .copied()
            .find(|id| id.owner == fixture.inner_a)
            .unwrap();
        let mut contract = seal.surfaces.remove(&old_id).unwrap();
        contract.id.ordinal += 7;
        seal.surfaces.insert(contract.id, contract);
        assert!(!property_scene_plan_is_sealed(&ordinal));

        let mut parent = base.clone();
        parent
            .property_scene_seal
            .as_mut()
            .unwrap()
            .surfaces
            .values_mut()
            .find(|contract| contract.id.owner == fixture.inner_a)
            .unwrap()
            .transform
            .parent = None;
        assert!(!property_scene_plan_is_sealed(&parent));

        let mut matrix = base.clone();
        matrix
            .property_scene_seal
            .as_mut()
            .unwrap()
            .surfaces
            .values_mut()
            .find(|contract| contract.id.owner == fixture.inner_a)
            .unwrap()
            .transform
            .viewport_matrix = glam::Mat4::from_translation(glam::Vec3::new(99.0, 1.0, 0.0));
        assert!(!property_scene_plan_is_sealed(&matrix));

        let mut generation = base.clone();
        generation
            .property_scene_seal
            .as_mut()
            .unwrap()
            .surfaces
            .values_mut()
            .find(|contract| contract.id.owner == fixture.inner_a)
            .unwrap()
            .transform
            .generation += 1;
        assert!(!property_scene_plan_is_sealed(&generation));

        let mut identity = base.clone();
        let outer_identity = {
            let outer = property_surface_mut(&mut identity.steps, fixture.outer).unwrap();
            (outer.stable_id, outer.persistent_color_key)
        };
        let inner = property_surface_mut(&mut identity.steps, fixture.inner_a).unwrap();
        inner.stable_id = outer_identity.0;
        inner.persistent_color_key = outer_identity.1;
        assert!(!property_scene_plan_is_sealed(&identity));

        let mut zero_stable = base.clone();
        property_surface_mut(&mut zero_stable.steps, fixture.inner_a)
            .unwrap()
            .stable_id = 0;
        assert!(!property_scene_plan_is_sealed(&zero_stable));

        let mut zero_contract_stable = base.clone();
        zero_contract_stable
            .property_scene_seal
            .as_mut()
            .unwrap()
            .surfaces
            .values_mut()
            .find(|contract| contract.id.owner == fixture.inner_a)
            .unwrap()
            .stable_id = 0;
        assert!(!property_scene_plan_is_sealed(&zero_contract_stable));

        let mut alternate_stable = base.clone();
        property_surface_mut(&mut alternate_stable.steps, fixture.inner_a)
            .unwrap()
            .stable_id = 0xf0_f0_f0;
        assert!(!property_scene_plan_is_sealed(&alternate_stable));

        let mut alternate_contract_stable = base.clone();
        alternate_contract_stable
            .property_scene_seal
            .as_mut()
            .unwrap()
            .surfaces
            .values_mut()
            .find(|contract| contract.id.owner == fixture.inner_a)
            .unwrap()
            .stable_id = 0xf0_f0_f0;
        assert!(!property_scene_plan_is_sealed(&alternate_contract_stable));

        let mut arbitrary_key = base.clone();
        property_surface_mut(&mut arbitrary_key.steps, fixture.inner_a)
            .unwrap()
            .persistent_color_key =
            crate::view::frame_graph::PersistentTextureKey::Generic(0xdead_beef);
        assert!(!property_scene_plan_is_sealed(&arbitrary_key));

        let mut missing = base.clone();
        missing.steps.retain(|step| {
            !matches!(step, PaintPlanStep::RetainedSurface(surface) if surface.boundary_root() == fixture.second_root)
        });
        assert!(!property_scene_plan_is_sealed(&missing));

        let mut duplicate = base.clone();
        let repeated = duplicate
            .steps
            .iter()
            .find(|step| {
                matches!(step, PaintPlanStep::RetainedSurface(surface) if surface.boundary_root() == fixture.second_root)
            })
            .cloned()
            .unwrap();
        duplicate.steps.push(repeated);
        assert!(!property_scene_plan_is_sealed(&duplicate));

        let mut artifact_witness = base.clone();
        artifact_witness
            .property_scene_seal
            .as_mut()
            .unwrap()
            .surfaces
            .values_mut()
            .find(|contract| !contract.artifact_validation.is_empty())
            .unwrap()
            .artifact_validation
            .pop();
        assert!(!property_scene_plan_is_sealed(&artifact_witness));

        let mut scissor = plan_transform_property_scene_with_context(
            &fixture.arena,
            &fixture.roots,
            &FxHashSet::default(),
            &fixture.properties,
            &fixture.generations,
            TransformSurfacePlanContext::new([0.0; 2], Some([3, 4, 80, 60])),
        )
        .expect("outer scissor is frozen by the scene seal");
        scissor
            .property_scene_seal
            .as_mut()
            .unwrap()
            .outer_scissor_rect = None;
        assert!(!property_scene_plan_is_sealed(&scissor));
    }

    #[test]
    fn property_scene_preserves_ordered_root_spans_and_rejects_root_witness_drift() {
        let mut fixture = general_property_scene_fixture();
        let painted_root = |id: u64, x: f32, color| {
            let mut element = Element::new_with_id(id, x, 30.0, 9.0, 7.0);
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
            element.apply_style(style);
            element
        };
        let painted_a = commit_element(
            &mut fixture.arena,
            Box::new(painted_root(0xd1_1001, 180.0, Color::rgb(20, 60, 100))),
        );
        let painted_b = commit_element(
            &mut fixture.arena,
            Box::new(painted_root(0xd1_1002, 195.0, Color::rgb(100, 60, 20))),
        );
        let constraints = LayoutConstraints {
            max_width: 220.0,
            max_height: 140.0,
            viewport_width: 220.0,
            viewport_height: 140.0,
            percent_base_width: Some(220.0),
            percent_base_height: Some(140.0),
        };
        for (root, x) in [(painted_a, 180.0), (painted_b, 195.0)] {
            measure_and_place(
                &mut fixture.arena,
                root,
                constraints,
                LayoutPlacement {
                    parent_x: x,
                    parent_y: 30.0,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: 220.0,
                    available_height: 140.0,
                    viewport_width: 220.0,
                    viewport_height: 140.0,
                    percent_base_width: Some(220.0),
                    percent_base_height: Some(140.0),
                },
            );
        }
        let transparent = fixture.roots[0];
        let trailing_transparent = fixture.roots[3];
        crate::view::test_support::get_element_mut::<Element>(&fixture.arena, transparent)
            .set_should_paint_for_test(false);
        crate::view::test_support::get_element_mut::<Element>(&fixture.arena, trailing_transparent)
            .set_should_paint_for_test(false);
        fixture.roots = vec![
            painted_a,
            painted_b,
            transparent,
            fixture.outer,
            fixture.second_root,
            trailing_transparent,
        ];
        fixture.properties = PropertyTrees::default();
        fixture.properties.sync(&fixture.arena, &fixture.roots);
        fixture.generations = PaintGenerationTracker::default();
        fixture
            .generations
            .sync(&fixture.arena, &fixture.roots, &fixture.properties);
        let base = plan_transform_property_scene_with_context(
            &fixture.arena,
            &fixture.roots,
            &FxHashSet::default(),
            &fixture.properties,
            &fixture.generations,
            TransformSurfacePlanContext::default(),
        )
        .expect("mixed roots retain their exact input order");
        assert!(property_scene_plan_is_sealed(&base));
        let roots = base.property_scene_roots.as_ref().unwrap();
        assert_eq!(
            roots.iter().map(|root| root.root).collect::<Vec<_>>(),
            fixture.roots
        );
        assert!(roots[0].top_level_step_span.start < roots[0].top_level_step_span.end);
        assert!(roots[1].top_level_step_span.start < roots[1].top_level_step_span.end);
        assert_eq!(
            roots[0].top_level_step_span.end,
            roots[1].top_level_step_span.start
        );
        assert_eq!(
            roots[2].top_level_step_span.start, roots[2].top_level_step_span.end,
            "transparent root keeps an explicit empty insertion span"
        );
        assert!(roots[3].top_level_step_span.start < roots[3].top_level_step_span.end);
        assert!(roots[4].top_level_step_span.start < roots[4].top_level_step_span.end);
        assert_eq!(
            roots[5].top_level_step_span.start,
            roots[5].top_level_step_span.end
        );

        let duplicate_input = plan_transform_property_scene_with_context(
            &fixture.arena,
            &[painted_a, painted_a],
            &FxHashSet::default(),
            &fixture.properties,
            &fixture.generations,
            TransformSurfacePlanContext::default(),
        )
        .expect_err("duplicate roots fail before materialization");
        assert!(
            duplicate_input
                .reasons
                .contains(&FramePaintPlanRejection::DuplicateRoot(painted_a))
        );

        let mut reordered_input = fixture.roots.clone();
        reordered_input.reverse();
        let reordered = plan_transform_property_scene_with_context(
            &fixture.arena,
            &reordered_input,
            &FxHashSet::default(),
            &fixture.properties,
            &fixture.generations,
            TransformSurfacePlanContext::default(),
        )
        .expect("a new input order gets a newly sealed exact order");
        assert_eq!(
            reordered
                .property_scene_roots
                .as_ref()
                .unwrap()
                .iter()
                .map(|root| root.root)
                .collect::<Vec<_>>(),
            reordered_input
        );

        let mut duplicate = base.clone();
        duplicate.property_scene_roots.as_mut().unwrap()[1].root = painted_a;
        assert!(!property_scene_plan_is_sealed(&duplicate));

        let mut reorder = base.clone();
        reorder.property_scene_roots.as_mut().unwrap().swap(0, 1);
        assert!(!property_scene_plan_is_sealed(&reorder));

        let mut stable = base.clone();
        stable.property_scene_roots.as_mut().unwrap()[0].stable_id += 1;
        assert!(!property_scene_plan_is_sealed(&stable));

        let mut owner = base.clone();
        owner.property_scene_roots.as_mut().unwrap()[0].owner.parent = Some(painted_b);
        assert!(!property_scene_plan_is_sealed(&owner));

        let mut span = base.clone();
        span.property_scene_roots.as_mut().unwrap()[0]
            .top_level_step_span
            .end += 1;
        assert!(!property_scene_plan_is_sealed(&span));
    }

    #[test]
    fn property_scene_rejects_effect_scroll_promotion_deferred_and_legacy_boundaries() {
        let fixture = general_property_scene_fixture();
        let promoted_id = fixture
            .arena
            .get(fixture.inner_a)
            .unwrap()
            .element
            .stable_id();
        let promoted = plan_transform_property_scene_with_context(
            &fixture.arena,
            &fixture.roots,
            &FxHashSet::from_iter([promoted_id]),
            &fixture.properties,
            &fixture.generations,
            TransformSurfacePlanContext::default(),
        )
        .expect_err("promotion authority cannot overlap the property scene");
        assert!(
            promoted
                .reasons
                .contains(&FramePaintPlanRejection::PromotionPresent(promoted_id))
        );

        for property in ["effect", "scroll"] {
            let property_fixture = general_property_scene_fixture();
            let mut properties = property_fixture.properties;
            let state = properties
                .states
                .get_mut(&property_fixture.inner_a)
                .unwrap();
            match property {
                "effect" => state.paint.effect = Some(EffectNodeId(property_fixture.inner_a)),
                "scroll" => state.paint.scroll = Some(ScrollNodeId(property_fixture.inner_a)),
                _ => unreachable!(),
            }
            let error = plan_transform_property_scene_with_context(
                &property_fixture.arena,
                &property_fixture.roots,
                &FxHashSet::default(),
                &properties,
                &property_fixture.generations,
                TransformSurfacePlanContext::default(),
            )
            .expect_err("unsupported property authority must fail closed");
            assert!(
                !error.reasons.is_empty()
                    && error
                        .reasons
                        .iter()
                        .all(|reason| matches!(reason, FramePaintPlanRejection::Coverage(_))),
                "{property}: {:?}",
                error.reasons
            );
        }

        let deferred_fixture = general_property_scene_fixture();
        let mut deferred_style = Style::new();
        deferred_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                crate::style::Position::absolute()
                    .left(crate::style::Length::px(0.0))
                    .clip(crate::style::ClipMode::Viewport),
            ),
        );
        crate::view::test_support::get_element_mut::<Element>(
            &deferred_fixture.arena,
            deferred_fixture.inner_a,
        )
        .apply_style(deferred_style);
        let deferred = plan_transform_property_scene_with_context(
            &deferred_fixture.arena,
            &deferred_fixture.roots,
            &FxHashSet::default(),
            &deferred_fixture.properties,
            &deferred_fixture.generations,
            TransformSurfacePlanContext::default(),
        )
        .expect_err("deferred ordering cannot enter a retained property scene");
        assert!(
            deferred
                .reasons
                .contains(&FramePaintPlanRejection::DeferredBoundary(
                    deferred_fixture.inner_a
                ))
        );

        let mut legacy_fixture = general_property_scene_fixture();
        let neutral_root = legacy_fixture.roots[0];
        commit_child(
            &mut legacy_fixture.arena,
            neutral_root,
            Box::new(Element::new_with_id(0xd1_00ff, 0.0, 0.0, 1.0, 1.0)),
        );
        measure_and_place(
            &mut legacy_fixture.arena,
            neutral_root,
            LayoutConstraints {
                max_width: 220.0,
                max_height: 140.0,
                viewport_width: 220.0,
                viewport_height: 140.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(140.0),
            },
            LayoutPlacement {
                parent_x: 130.0,
                parent_y: 10.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 220.0,
                available_height: 140.0,
                viewport_width: 220.0,
                viewport_height: 140.0,
                percent_base_width: Some(220.0),
                percent_base_height: Some(140.0),
            },
        );
        legacy_fixture.properties = PropertyTrees::default();
        legacy_fixture
            .properties
            .sync(&legacy_fixture.arena, &legacy_fixture.roots);
        legacy_fixture.generations = PaintGenerationTracker::default();
        legacy_fixture.generations.sync(
            &legacy_fixture.arena,
            &legacy_fixture.roots,
            &legacy_fixture.properties,
        );
        let mut rounded = Style::new();
        rounded.insert(
            PropertyId::BorderRadius,
            ParsedValue::Length(crate::style::Length::px(2.0)),
        );
        crate::view::test_support::get_element_mut::<Element>(&legacy_fixture.arena, neutral_root)
            .apply_style(rounded);
        let legacy = plan_transform_property_scene_with_context(
            &legacy_fixture.arena,
            &legacy_fixture.roots,
            &FxHashSet::default(),
            &legacy_fixture.properties,
            &legacy_fixture.generations,
            TransformSurfacePlanContext::default(),
        )
        .expect_err("unowned child clip remains legacy");
        assert!(
            legacy.reasons.iter().any(|reason| matches!(
                reason,
                FramePaintPlanRejection::Coverage(FrameArtifactFallbackReason::LegacyBoundary(_))
            )),
            "{:?}",
            legacy.reasons
        );
    }

    #[test]
    fn property_scene_freezes_exact_local_and_ancestor_rect_clips() {
        let mut fixture = general_property_scene_fixture();
        let clip_id = ClipNodeId {
            owner: fixture.outer,
            role: ClipNodeRole::SelfClip,
        };
        fixture.properties.clips.insert(
            clip_id,
            crate::view::compositor::property_tree::ClipNode {
                owner: fixture.outer,
                parent: None,
                geometry: ClipGeometry::LogicalScissor([7, 9, 31, 19]),
                behavior: ClipBehavior::Replace,
                generation: 1,
            },
        );
        let subtree = fixture
            .properties
            .states
            .keys()
            .copied()
            .filter(|&key| {
                let mut cursor = Some(key);
                while let Some(owner) = cursor {
                    if owner == fixture.outer {
                        return true;
                    }
                    cursor = fixture.arena.parent_of(owner);
                }
                false
            })
            .collect::<Vec<_>>();
        for key in subtree {
            let state = fixture.properties.states.get_mut(&key).unwrap();
            state.paint.clip = Some(clip_id);
            state.descendants.clip = Some(clip_id);
        }
        fixture.generations = PaintGenerationTracker::default();
        fixture
            .generations
            .sync(&fixture.arena, &fixture.roots, &fixture.properties);
        let base = plan_transform_property_scene_with_context(
            &fixture.arena,
            &fixture.roots,
            &FxHashSet::default(),
            &fixture.properties,
            &fixture.generations,
            TransformSurfacePlanContext::default(),
        )
        .expect("exact logical rect clips are admitted");
        assert!(property_scene_plan_is_sealed(&base));
        let seal = base.property_scene_seal.as_ref().unwrap();
        let inner = seal
            .surfaces
            .values()
            .find(|contract| contract.id.owner == fixture.inner_a)
            .unwrap();
        assert_eq!(inner.ancestor_composite_clips.len(), 1);
        assert_eq!(inner.resolved_composite_scissor, Some([7, 9, 31, 19]));

        let mut ancestor = base.clone();
        ancestor
            .property_scene_seal
            .as_mut()
            .unwrap()
            .surfaces
            .values_mut()
            .find(|contract| contract.id.owner == fixture.inner_a)
            .unwrap()
            .ancestor_composite_clips[0]
            .logical_scissor = [8, 9, 31, 19];
        assert!(!property_scene_plan_is_sealed(&ancestor));

        let mut local = base.clone();
        let outer = property_surface_mut(&mut local.steps, fixture.outer).unwrap();
        let snapshot = outer
            .raster_steps
            .iter_mut()
            .find_map(|step| match step {
                PaintPlanStep::ArtifactSpan(span) => span.artifact.clip_nodes.first_mut(),
                PaintPlanStep::RetainedSurface(_) => None,
            })
            .expect("outer artifact owns the exact local clip");
        snapshot.logical_scissor[0] += 1;
        assert!(!property_scene_plan_is_sealed(&local));
    }

    #[test]
    fn property_scene_executor_emits_arbitrary_depth_forest_and_stages_one_transaction() {
        let fixture = general_property_scene_fixture();
        let plan = plan_transform_property_scene_with_context(
            &fixture.arena,
            &fixture.roots,
            &FxHashSet::default(),
            &fixture.properties,
            &fixture.generations,
            TransformSurfacePlanContext::default(),
        )
        .expect("sealed general property scene");
        let terminal = plan
            .property_scene_seal
            .as_ref()
            .unwrap()
            .aggregate_opaque_order_span
            .end;
        let mut graph = FrameGraph::new();
        let (ctx, _) = parent_context_with_clear(&mut graph, 220, 140, 1.0);
        let mut viewport = Viewport::new();
        let prepared =
            super::super::prepare_retained_property_scene_from_pool(&viewport, &plan, &graph, &ctx)
                .expect("multi-root arbitrary-depth property-scene preflight");
        let outcome = super::super::emit_prepared_retained_property_scene(
            &mut viewport,
            prepared,
            &mut graph,
            ctx,
        );
        let (state, trace) = outcome.into_parts();
        assert_eq!(trace.root_count, fixture.roots.len());
        assert_eq!(trace.surface_count, 5);
        assert_eq!(trace.reraster_count, 5);
        assert_eq!(trace.reuse_count, 0);
        assert_eq!(state.opaque_rect_order_for_test(), terminal);
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
                .len(),
            5,
            "every transform edge is applied exactly once during the initial raster"
        );
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, Some(5))
        );
        viewport.finish_retained_surface_transaction(true);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (5, None)
        );

        let mut reuse_graph = FrameGraph::new();
        let (reuse_ctx, _) = parent_context_with_clear(&mut reuse_graph, 220, 140, 1.0);
        let reuse = super::super::build_retained_property_scene_with_forced_pool_for_test(
            &mut viewport,
            &plan,
            &mut reuse_graph,
            reuse_ctx,
        )
        .expect("identical scene reuses every compatible resident pair");
        let (_, reuse_trace) = reuse.into_parts();
        assert_eq!(reuse_trace.reraster_count, 0);
        assert_eq!(reuse_trace.reuse_count, 5);
        assert_eq!(
            reuse_graph
                .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
                .len(),
            2,
            "a reused forest composites only its two already-rasterized top-level surfaces"
        );
        viewport.finish_retained_surface_transaction(false);
    }

    #[test]
    fn property_scene_context_mismatch_rejects_before_graph_or_pool_mutation() {
        let fixture = general_property_scene_fixture();
        let plan = plan_transform_property_scene_with_context(
            &fixture.arena,
            &fixture.roots,
            &FxHashSet::default(),
            &fixture.properties,
            &fixture.generations,
            TransformSurfacePlanContext::default(),
        )
        .unwrap();
        let graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(220, 140, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        ctx.push_scissor_rect(Some([1, 2, 30, 40]));
        let graph_before = graph.build_state_snapshot_for_test();
        let viewport = Viewport::new();
        let transaction_before = viewport.retained_surface_transaction_shape_for_test();
        let error = match super::super::prepare_retained_property_scene_from_pool(
            &viewport, &plan, &graph, &ctx,
        ) {
            Ok(_) => panic!("mismatched live context cannot prepare the frozen scene"),
            Err(error) => error,
        };
        assert_eq!(
            error,
            super::super::ForcedTransformSurfaceError::ContextMismatch
        );
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            transaction_before
        );
    }

    fn exact_transform_child_isolation_fixture() -> (
        NodeArena,
        NodeKey,
        NodeKey,
        NodeKey,
        NodeKey,
        NodeKey,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        let (arena, root, before, child, descendant, after, _, _) =
            nested_exact_transform_fixture();
        {
            let mut child_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, child);
            child_element.set_resolved_transform_for_test(None);
            child_element.set_opacity(0.5);
        }
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        (
            arena,
            root,
            before,
            child,
            descendant,
            after,
            properties,
            generations,
        )
    }

    fn planning_only_nested_effect_fixture() -> (
        NodeArena,
        NodeKey,
        NodeKey,
        NodeKey,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        let (arena, root, _before, child, grandchild, _after, _, _) =
            nested_exact_transform_fixture();
        {
            let mut root_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, root);
            root_element.set_resolved_transform_for_test(None);
            root_element.set_opacity(0.5);
        }
        {
            let mut child_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, child);
            child_element.set_resolved_transform_for_test(None);
            child_element.set_opacity(0.0);
        }
        crate::view::test_support::get_element_mut::<Element>(&arena, grandchild).set_opacity(0.75);
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        (arena, root, child, grandchild, properties, generations)
    }

    #[test]
    fn property_effect_scaffold_freezes_nested_chain_and_opacity_zero_structure() {
        let (arena, root, child, grandchild, properties, generations) =
            planning_only_nested_effect_fixture();
        let plan = plan_property_effect_scene_scaffold_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect("pure nested opacity forest must produce a planning seal");
        assert!(plan.property_effect_scaffold_is_sealed_for_test());
        assert!(plan.property_scene_transaction_witness().is_none());
        let scaffold = plan
            .property_scene_seal
            .as_ref()
            .and_then(|seal| seal.effect_scaffold.as_ref())
            .expect("effect scaffold");
        assert_eq!(
            scaffold
                .surfaces
                .iter()
                .map(|surface| surface.boundary.owner())
                .collect::<Vec<_>>(),
            vec![root, child, grandchild]
        );
        let PropertyEffectSurfaceKind::Isolation(child_surface) = &scaffold.surfaces[1].kind else {
            panic!("child effect surface")
        };
        assert_eq!(child_surface.composite.opacity_bits, 0.0_f32.to_bits());
        assert_eq!(child_surface.effect_chain.live_leaf_to_root.len(), 2);
        assert_eq!(child_surface.effect_chain.detached_ancestors.len(), 1);
        assert_eq!(child_surface.effect_chain.isolated_leaf.parent, None);
        assert!(!child_surface.raster_identity.content.is_empty());
        assert_eq!(child_surface.parent_opaque_cursor_delta, 0);
        let PropertyEffectSurfaceKind::Isolation(parent_surface) = &scaffold.surfaces[0].kind
        else {
            panic!("parent effect surface")
        };
        assert_eq!(parent_surface.nested_dependencies.len(), 1);
        assert_eq!(
            parent_surface.nested_dependencies[0].child_opacity_bits,
            0.0_f32.to_bits()
        );
    }

    #[test]
    fn property_effect_scene_materializes_pure_nested_opacity_forest() {
        let (arena, root, child, grandchild, properties, generations) =
            planning_only_nested_effect_fixture();
        let plan = plan_property_effect_scene_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect("canonical nested opacity forest must materialize");
        assert!(property_scene_plan_is_sealed(&plan));
        let witness = plan
            .property_scene_transaction_witness()
            .expect("production effect scene transaction witness");
        assert_eq!(witness.roots.len(), 1);
        assert_eq!(witness.surfaces.len(), 3);
        assert_eq!(
            witness
                .surfaces
                .iter()
                .map(|surface| (surface.boundary_root, surface.parent_surface))
                .collect::<Vec<_>>(),
            vec![(root, None), (child, Some(root)), (grandchild, Some(child))]
        );
        assert!(
            witness.surfaces.iter().all(|surface| matches!(
                surface.kind,
                PropertySceneTransactionSurfaceKind::Effect(_)
            ))
        );
        assert_eq!(plan.steps.len(), 1);
        let PaintPlanStep::RetainedSurface(root_surface) = &plan.steps[0] else {
            panic!("effect root must be a top-level retained surface")
        };
        assert!(matches!(root_surface.kind, SurfaceKind::NestedIsolation(_)));

        let mut viewport = Viewport::new();
        let mut graph = FrameGraph::new();
        let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
        let before = graph.build_state_snapshot_for_test();
        let prepared_stamps = super::super::prepare_retained_property_scene_stamps_for_test(
            &viewport, &plan, &graph, &ctx,
        )
        .expect("effect stamps are fully prepared before graph mutation");
        assert_eq!(prepared_stamps.len(), 3);
        assert!(prepared_stamps.iter().all(|stamp| {
            stamp.identity.role == super::super::RetainedSurfaceRasterRole::PropertyEffect
                && stamp.property_effect.is_some()
        }));
        let root_dependency = prepared_stamps[0]
            .ordered_steps
            .iter()
            .find_map(|step| match step {
                super::super::RetainedSurfaceRasterStepStamp::NestedSurface(dependency) => {
                    Some(dependency)
                }
                super::super::RetainedSurfaceRasterStepStamp::ArtifactSpan(_) => None,
                super::super::RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
                | super::super::RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
                | super::super::RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => None,
            })
            .expect("root stamp embeds the direct child composite dependency");
        let super::super::RetainedSurfaceCompositeGeometryStamp::PropertyEffect {
            opacity_bits,
            effect_generation,
            basis,
            resolved_scissor,
            ancestor_composite_clips,
            ..
        } = &root_dependency.child_composite_geometry
        else {
            panic!("dedicated property-effect composite dependency")
        };
        assert_eq!(*opacity_bits, 0.0_f32.to_bits());
        assert_eq!(
            *effect_generation,
            properties.effects[&EffectNodeId(child)].generation
        );
        assert_eq!(
            *basis,
            super::super::compiler::PropertyEffectCompositeBasisStamp::ParentEffect(EffectNodeId(
                root
            ))
        );
        assert_eq!(*resolved_scissor, None);
        assert!(ancestor_composite_clips.is_empty());
        assert_eq!(graph.build_state_snapshot_for_test(), before);
        let outcome = super::super::build_retained_property_scene_with_forced_pool_for_test(
            &mut viewport,
            &plan,
            &mut graph,
            ctx,
        )
        .expect("pure nested effect scene must preflight and emit");
        let (state, trace) = outcome.into_parts();
        assert_eq!(trace.root_count, 1);
        assert_eq!(trace.surface_count, 3);
        assert_eq!(trace.reraster_count, 3);
        assert_eq!(
            state.opaque_rect_order_for_test(),
            0,
            "nested opacity composites must not leak their raster-local opaque cursors"
        );
        let composites = graph
            .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>();
        assert_eq!(composites.len(), 3);
        assert_eq!(
            composites
                .iter()
                .map(|pass| pass.test_snapshot().opacity_bits)
                .collect::<Vec<_>>(),
            vec![0.75_f32.to_bits(), 0.0_f32.to_bits(), 0.5_f32.to_bits()],
            "each effect opacity is applied once, on its own child-to-parent composite edge"
        );
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, Some(3)),
            "the arbitrary-depth forest stages one exact atomic transaction"
        );
        assert_ne!(graph.build_state_snapshot_for_test(), before);
        viewport.finish_retained_surface_transaction(true);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (3, None)
        );
    }

    #[test]
    fn property_effect_transaction_rejects_forged_terminal_shape_basis_and_scissor() {
        let (arena, root, child, grandchild, properties, generations) =
            planning_only_nested_effect_fixture();
        let plan = plan_property_effect_scene_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect("canonical effect transaction fixture");
        let viewport = Viewport::new();
        let mut graph = FrameGraph::new();
        let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
        let stamps = super::super::prepare_retained_property_scene_stamps_for_test(
            &viewport, &plan, &graph, &ctx,
        )
        .expect("canonical prepared stamps");
        let witness = plan
            .property_scene_transaction_witness()
            .expect("canonical effect witness");
        let step_count = plan.steps.len();
        let aggregate = witness.aggregate_opaque_order_span.clone();
        assert!(
            super::super::retained_surface_executor::RetainedPropertyEffectSceneTransactionStamp::new_for_test(
                witness.clone(),
                &stamps,
                step_count,
                aggregate.clone(),
            )
            .is_some()
        );

        let mut aggregate_drift = witness.clone();
        aggregate_drift.aggregate_opaque_order_span.end += 1;
        assert!(
            super::super::retained_surface_executor::RetainedPropertyEffectSceneTransactionStamp::new_for_test(
                aggregate_drift,
                &stamps,
                step_count,
                aggregate.clone(),
            )
            .is_none(),
            "the transaction must recompute and bind the actual opaque terminal"
        );

        let mut root_coverage_drift = witness.clone();
        root_coverage_drift.roots[0].top_level_step_span.end += 1;
        assert!(
            super::super::retained_surface_executor::RetainedPropertyEffectSceneTransactionStamp::new_for_test(
                root_coverage_drift,
                &stamps,
                step_count,
                aggregate.clone(),
            )
            .is_none(),
            "root spans must cover exactly the prepared plan step count"
        );

        let mut transform_transform_effect = witness.clone();
        for (surface, owner) in transform_transform_effect
            .surfaces
            .iter_mut()
            .take(2)
            .zip([root, child])
        {
            surface.kind = PropertySceneTransactionSurfaceKind::Transform(TransformNodeId(owner));
            surface.transform_viewport_matrix_bits =
                Some(glam::Mat4::IDENTITY.to_cols_array().map(f32::to_bits));
            surface.effect_composite = None;
        }
        assert_eq!(
            transform_transform_effect.surfaces[2].boundary_root,
            grandchild
        );
        assert!(
            super::super::retained_surface_executor::RetainedPropertyEffectSceneTransactionStamp::new_for_test(
                transform_transform_effect,
                &stamps,
                step_count,
                aggregate.clone(),
            )
            .is_none(),
            "effect transaction admission must reject Transform -> Transform -> Effect"
        );

        let mut basis_drift = stamps.clone();
        let dependency = basis_drift[0]
            .ordered_steps
            .iter_mut()
            .find_map(|step| match step {
                super::super::RetainedSurfaceRasterStepStamp::NestedSurface(dependency) => {
                    Some(dependency)
                }
                super::super::RetainedSurfaceRasterStepStamp::ArtifactSpan(_) => None,
                super::super::RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
                | super::super::RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
                | super::super::RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => None,
            })
            .expect("root effect embeds its child");
        let super::super::RetainedSurfaceCompositeGeometryStamp::PropertyEffect { basis, .. } =
            &mut dependency.child_composite_geometry
        else {
            panic!("effect dependency geometry")
        };
        *basis = super::super::compiler::PropertyEffectCompositeBasisStamp::ParentEffect(
            EffectNodeId(grandchild),
        );
        assert!(
            super::super::retained_surface_executor::RetainedPropertyEffectSceneTransactionStamp::new_for_test(
                witness.clone(),
                &basis_drift,
                step_count,
                aggregate.clone(),
            )
            .is_none(),
            "child composite basis must match the actual parent witness"
        );

        let mut scissor_drift = stamps.clone();
        let dependency = scissor_drift[0]
            .ordered_steps
            .iter_mut()
            .find_map(|step| match step {
                super::super::RetainedSurfaceRasterStepStamp::NestedSurface(dependency) => {
                    Some(dependency)
                }
                super::super::RetainedSurfaceRasterStepStamp::ArtifactSpan(_) => None,
                super::super::RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
                | super::super::RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
                | super::super::RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => None,
            })
            .expect("root effect embeds its child");
        let super::super::RetainedSurfaceCompositeGeometryStamp::PropertyEffect {
            resolved_scissor,
            ..
        } = &mut dependency.child_composite_geometry
        else {
            panic!("effect dependency geometry")
        };
        *resolved_scissor = Some([0, 0, 1, 1]);
        assert!(
            super::super::retained_surface_executor::RetainedPropertyEffectSceneTransactionStamp::new_for_test(
                witness,
                &scissor_drift,
                step_count,
                aggregate,
            )
            .is_none(),
            "resolved scissor must match the child surface witness exactly"
        );
    }

    #[test]
    fn property_effect_scene_mismatch_rejects_before_graph_pool_or_pending_mutation() {
        let (arena, root, _, _, properties, generations) = planning_only_nested_effect_fixture();
        let plan = plan_property_effect_scene_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect("canonical property effect scene");

        let mut viewport = Viewport::new();
        let mut baseline_graph = FrameGraph::new();
        let baseline_ctx = parent_context_without_clear(&mut baseline_graph, 160, 120, 1.0);
        super::super::build_retained_property_scene_with_forced_pool_for_test(
            &mut viewport,
            &plan,
            &mut baseline_graph,
            baseline_ctx,
        )
        .expect("baseline transaction");
        viewport.finish_retained_surface_transaction(true);

        let mut mismatch = plan.clone();
        let PaintPlanStep::RetainedSurface(root_surface) = &mut mismatch.steps[0] else {
            panic!("effect root surface")
        };
        let SurfaceKind::NestedIsolation(root_effect) = &mut root_surface.kind else {
            panic!("effect root kind")
        };
        root_effect
            .property_scene
            .as_mut()
            .expect("property contract")
            .composite
            .effect_generation += 1;

        let graph = FrameGraph::new();
        let ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let graph_before = graph.build_state_snapshot_for_test();
        let pool_before = viewport.retained_surface_transaction_shape_for_test();
        let error = match super::super::prepare_retained_property_scene_from_pool(
            &viewport, &mismatch, &graph, &ctx,
        ) {
            Ok(_) => panic!("drifted effect contract cannot mint a pre-clear token"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            super::super::ForcedTransformSurfaceError::PlanShape
                | super::super::ForcedTransformSurfaceError::GeometryContract
        ));
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            pool_before,
            "failed preparation preserves both resident pool and pending transaction"
        );
    }

    #[test]
    fn property_effect_scene_child_opacity_reuses_child_and_rerasterizes_direct_parent() {
        let (arena, root, child, grandchild, mut properties, mut generations) =
            planning_only_nested_effect_fixture();
        let build_plan = |properties: &PropertyTrees, generations: &PaintGenerationTracker| {
            plan_property_effect_scene_with_context(
                &arena,
                &[root],
                &FxHashSet::default(),
                properties,
                generations,
                TransformSurfacePlanContext::new([0.0, 0.0], None),
            )
            .expect("canonical effect scene")
        };
        let baseline = build_plan(&properties, &generations);
        let mut viewport = Viewport::new();
        let mut baseline_graph = FrameGraph::new();
        let baseline_ctx = parent_context_without_clear(&mut baseline_graph, 160, 120, 1.0);
        super::super::build_retained_property_scene_with_forced_pool_for_test(
            &mut viewport,
            &baseline,
            &mut baseline_graph,
            baseline_ctx,
        )
        .expect("baseline effect transaction");
        viewport.finish_retained_surface_transaction(true);

        crate::view::test_support::get_element_mut::<Element>(&arena, child).set_opacity(0.25);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let changed = build_plan(&properties, &generations);
        let mut changed_graph = FrameGraph::new();
        let changed_ctx = parent_context_without_clear(&mut changed_graph, 160, 120, 1.0);
        let changed = super::super::build_retained_property_scene_with_forced_pool_for_test(
            &mut viewport,
            &changed,
            &mut changed_graph,
            changed_ctx,
        )
        .expect("opacity-only effect transaction");
        let (_, trace) = changed.into_parts();
        let actions = trace
            .surfaces
            .iter()
            .map(|surface| (surface.boundary_root, surface.action))
            .collect::<FxHashMap<_, _>>();
        assert_eq!(
            actions[&child],
            super::super::RetainedSurfaceCompileAction::Reuse,
            "own opacity/effect generation are excluded from the child's own raster stamp"
        );
        assert_eq!(
            actions[&grandchild],
            super::super::RetainedSurfaceCompileAction::Reuse
        );
        assert_eq!(
            actions[&root],
            super::super::RetainedSurfaceCompileAction::Reraster,
            "direct parent dependency includes child opacity and effect generation"
        );
        assert_eq!(trace.reraster_count, 1);
        assert_eq!(trace.reuse_count, 2);
        viewport.finish_retained_surface_transaction(true);

        crate::view::test_support::get_element_mut::<Element>(&arena, grandchild).set_opacity(0.6);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let transitive = build_plan(&properties, &generations);
        let mut transitive_graph = FrameGraph::new();
        let transitive_ctx = parent_context_without_clear(&mut transitive_graph, 160, 120, 1.0);
        let transitive = super::super::build_retained_property_scene_with_forced_pool_for_test(
            &mut viewport,
            &transitive,
            &mut transitive_graph,
            transitive_ctx,
        )
        .expect("grandchild opacity-only transaction");
        let (_, trace) = transitive.into_parts();
        let actions = trace
            .surfaces
            .iter()
            .map(|surface| (surface.boundary_root, surface.action))
            .collect::<FxHashMap<_, _>>();
        assert_eq!(
            actions[&grandchild],
            super::super::RetainedSurfaceCompileAction::Reuse
        );
        assert_eq!(
            actions[&child],
            super::super::RetainedSurfaceCompileAction::Reraster
        );
        assert_eq!(
            actions[&root],
            super::super::RetainedSurfaceCompileAction::Reraster,
            "the child's changed full stamp propagates into its own parent dependency"
        );
        assert_eq!(trace.reraster_count, 2);
        assert_eq!(trace.reuse_count, 1);
    }

    #[test]
    fn property_effect_scene_hidden_content_reraster_then_nonzero_reuses_child() {
        let (arena, root, child, grandchild, mut properties, mut generations) =
            planning_only_nested_effect_fixture();
        let build_plan = |properties: &PropertyTrees, generations: &PaintGenerationTracker| {
            plan_property_effect_scene_with_context(
                &arena,
                &[root],
                &FxHashSet::default(),
                properties,
                generations,
                TransformSurfacePlanContext::new([0.0, 0.0], None),
            )
            .expect("canonical hidden effect scene")
        };
        let baseline = build_plan(&properties, &generations);
        let mut viewport = Viewport::new();
        let mut baseline_graph = FrameGraph::new();
        let baseline_ctx = parent_context_without_clear(&mut baseline_graph, 160, 120, 1.0);
        super::super::build_retained_property_scene_with_forced_pool_for_test(
            &mut viewport,
            &baseline,
            &mut baseline_graph,
            baseline_ctx,
        )
        .expect("hidden baseline");
        viewport.finish_retained_surface_transaction(true);

        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(12, 180, 90)),
        );
        crate::view::test_support::get_element_mut::<Element>(&arena, child).apply_style(style);
        crate::view::test_support::get_element_mut::<Element>(&arena, child).set_opacity(0.0);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let hidden_changed = build_plan(&properties, &generations);
        let mut hidden_graph = FrameGraph::new();
        let hidden_ctx = parent_context_without_clear(&mut hidden_graph, 160, 120, 1.0);
        let hidden = super::super::build_retained_property_scene_with_forced_pool_for_test(
            &mut viewport,
            &hidden_changed,
            &mut hidden_graph,
            hidden_ctx,
        )
        .expect("hidden content change must remain structurally paintable");
        let (_, hidden_trace) = hidden.into_parts();
        let hidden_actions = hidden_trace
            .surfaces
            .iter()
            .map(|surface| (surface.boundary_root, surface.action))
            .collect::<FxHashMap<_, _>>();
        assert_eq!(
            hidden_actions[&child],
            super::super::RetainedSurfaceCompileAction::Reraster
        );
        assert_eq!(
            hidden_actions[&root],
            super::super::RetainedSurfaceCompileAction::Reraster
        );
        assert_eq!(
            hidden_actions[&grandchild],
            super::super::RetainedSurfaceCompileAction::Reuse
        );
        viewport.finish_retained_surface_transaction(true);

        crate::view::test_support::get_element_mut::<Element>(&arena, child).set_opacity(0.4);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let revealed = build_plan(&properties, &generations);
        let mut revealed_graph = FrameGraph::new();
        let revealed_ctx = parent_context_without_clear(&mut revealed_graph, 160, 120, 1.0);
        let revealed = super::super::build_retained_property_scene_with_forced_pool_for_test(
            &mut viewport,
            &revealed,
            &mut revealed_graph,
            revealed_ctx,
        )
        .expect("revealing updated hidden content");
        let (_, revealed_trace) = revealed.into_parts();
        let revealed_actions = revealed_trace
            .surfaces
            .iter()
            .map(|surface| (surface.boundary_root, surface.action))
            .collect::<FxHashMap<_, _>>();
        assert_eq!(
            revealed_actions[&child],
            super::super::RetainedSurfaceCompileAction::Reuse,
            "0->nonzero changes only composite authority after hidden content was rerastered"
        );
        assert_eq!(
            revealed_actions[&root],
            super::super::RetainedSurfaceCompileAction::Reraster
        );
        assert_eq!(
            revealed_actions[&grandchild],
            super::super::RetainedSurfaceCompileAction::Reuse
        );
    }

    #[test]
    fn property_effect_scene_executes_only_proven_mixed_transform_effect_shape() {
        let (arena, root, _, child, _, _, properties, generations) =
            exact_transform_child_isolation_fixture();
        let plan = plan_property_effect_scene_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect("proven mixed transform/effect scene");
        let witness = plan
            .property_scene_transaction_witness()
            .expect("mixed transaction witness");
        assert!(matches!(
            witness.surfaces.as_slice(),
            [
                PropertySceneTransactionSurfaceWitness {
                    kind: PropertySceneTransactionSurfaceKind::Transform(_),
                    parent_surface: None,
                    ..
                },
                PropertySceneTransactionSurfaceWitness {
                    kind: PropertySceneTransactionSurfaceKind::Effect(_),
                    parent_surface: Some(parent),
                    boundary_root,
                    ..
                }
            ] if *parent == root && *boundary_root == child
        ));

        let mut viewport = Viewport::new();
        let mut graph = FrameGraph::new();
        let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
        let outcome = super::super::build_retained_property_scene_with_forced_pool_for_test(
            &mut viewport,
            &plan,
            &mut graph,
            ctx,
        )
        .expect("mixed scene preflight and infallible emit");
        let (state, trace) = outcome.into_parts();
        assert_eq!(trace.surface_count, 2);
        assert_eq!(trace.reraster_count, 2);
        assert_eq!(
            state.opaque_rect_order_for_test(),
            plan.property_scene_seal
                .as_ref()
                .expect("seal")
                .aggregate_opaque_order_span
                .end
        );
        let composites = graph
            .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>();
        assert_eq!(composites.len(), 1);
        assert_eq!(
            composites[0].test_snapshot().opacity_bits,
            0.5_f32.to_bits(),
            "the child effect applies opacity exactly once"
        );
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
                .len(),
            1,
            "the transform root composites exactly once"
        );
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, Some(2))
        );
    }

    #[test]
    fn property_effect_scaffold_rejects_colocated_and_unproven_interleave() {
        let (arena, root, mut properties, mut generations) = exact_transform_fixture();
        crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.5);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let error = plan_property_effect_scene_scaffold_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect_err("co-located transform/effect must fail closed");
        assert!(
            error
                .reasons
                .contains(&FramePaintPlanRejection::CoLocatedTransformEffect(root))
        );

        let (arena, root, child, _, mut properties, mut generations) =
            planning_only_nested_effect_fixture();
        crate::view::test_support::get_element_mut::<Element>(&arena, child)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                3.0, 0.0, 0.0,
            ))));
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let error = plan_property_effect_scene_scaffold_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect_err("unproven effect/transform interleave must fail closed");
        assert!(error.reasons.iter().any(|reason| matches!(
            reason,
            FramePaintPlanRejection::CoLocatedTransformEffect(_)
                | FramePaintPlanRejection::UnsupportedPropertyInterleave(_)
        )));
    }

    #[test]
    fn property_effect_scaffold_admits_only_proven_transform_direct_effect_mapping() {
        let (arena, root, _, child, _, _, properties, generations) =
            exact_transform_child_isolation_fixture();
        let plan = plan_property_effect_scene_scaffold_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect("existing Transform -> direct Effect coordinate contract is proven");
        let scaffold = plan
            .property_scene_seal
            .as_ref()
            .and_then(|seal| seal.effect_scaffold.as_ref())
            .expect("effect scaffold");
        assert_eq!(scaffold.surfaces.len(), 2);
        let PropertyEffectSurfaceKind::Transform {
            nested_effect_dependencies,
            ..
        } = &scaffold.surfaces[0].kind
        else {
            panic!("transform parent")
        };
        assert_eq!(nested_effect_dependencies.len(), 1);
        assert_eq!(
            nested_effect_dependencies[0].child_effect,
            EffectNodeId(child)
        );
        assert_eq!(
            nested_effect_dependencies[0].child_opacity_bits,
            0.5_f32.to_bits()
        );
        let PropertyEffectSurfaceKind::Isolation(isolation) = &scaffold.surfaces[1].kind else {
            panic!("effect child")
        };
        assert!(matches!(
            isolation.composite.basis,
            PropertyIsolationCompositeBasis::ParentTransform { transform, .. }
                if transform == TransformNodeId(root)
        ));
        assert_eq!(isolation.parent_opaque_cursor_delta, 0);
    }

    #[test]
    fn property_effect_scaffold_rejects_mixed_wrapper_multiroot_and_non_affine_shapes() {
        let (mut arena, root, _, _, _, _, mut properties, mut generations) =
            exact_transform_child_isolation_fixture();
        let wrapper = commit_element(
            &mut arena,
            Box::new(Element::new_with_id(0xea_3001, 0.0, 0.0, 160.0, 120.0)),
        );
        arena.set_parent(root, Some(wrapper));
        arena.set_children(wrapper, vec![root]);
        properties.sync(&arena, &[wrapper]);
        generations.sync(&arena, &[wrapper], &properties);
        let error = plan_property_effect_scene_scaffold_with_context(
            &arena,
            &[wrapper],
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect_err("non-root transform wrapper is outside proven mixed shape");
        assert!(error.reasons.iter().any(|reason| matches!(
            reason,
            FramePaintPlanRejection::UnsupportedPropertyInterleave(_)
        )));

        let (mut arena, root, _, _, _, _, mut properties, mut generations) =
            exact_transform_child_isolation_fixture();
        let side_root = commit_element(
            &mut arena,
            Box::new(Element::new_with_id(0xea_3002, 140.0, 0.0, 8.0, 8.0)),
        );
        let roots = [root, side_root];
        properties.sync(&arena, &roots);
        generations.sync(&arena, &roots, &properties);
        let error = plan_property_effect_scene_scaffold_with_context(
            &arena,
            &roots,
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect_err("mixed property scaffold is single-root only");
        assert!(error.reasons.iter().any(|reason| matches!(
            reason,
            FramePaintPlanRejection::UnsupportedPropertyInterleave(_)
        )));

        let (arena, root, _, _, _, _, mut properties, generations) =
            exact_transform_child_isolation_fixture();
        properties
            .transforms
            .get_mut(&TransformNodeId(root))
            .expect("transform")
            .viewport_matrix = glam::Mat4::from_cols_array(&[
            1.0, 0.0, 0.0, 0.25, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]);
        let error = plan_property_effect_scene_scaffold_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect_err("finite perspective matrix is not the proven affine contract");
        assert!(error.reasons.iter().any(|reason| matches!(
            reason,
            FramePaintPlanRejection::UnsupportedPropertyInterleave(_)
        )));
    }

    #[test]
    fn property_effect_scaffold_places_local_and_ancestor_clips_exactly() {
        let (arena, root, _child, _, mut properties, mut generations) =
            planning_only_nested_effect_fixture();
        let clip_id = ClipNodeId {
            owner: root,
            role: ClipNodeRole::SelfClip,
        };
        properties.clips.insert(
            clip_id,
            crate::view::compositor::property_tree::ClipNode {
                owner: root,
                parent: None,
                geometry: ClipGeometry::LogicalScissor([2, 3, 20, 18]),
                behavior: ClipBehavior::Replace,
                generation: 41,
            },
        );
        let state_keys = properties.states.keys().copied().collect::<Vec<_>>();
        for key in state_keys {
            let state = properties.states.get_mut(&key).expect("state");
            state.paint.clip = Some(clip_id);
            state.descendants.clip = Some(clip_id);
        }
        generations.sync(&arena, &[root], &properties);
        let plan = plan_property_effect_scene_scaffold_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect("exact clip placement scaffold");
        let scaffold = plan
            .property_scene_seal
            .as_ref()
            .and_then(|seal| seal.effect_scaffold.as_ref())
            .expect("effect scaffold");
        let PropertyEffectSurfaceKind::Isolation(root_surface) = &scaffold.surfaces[0].kind else {
            panic!("root isolation")
        };
        let PropertyEffectSurfaceKind::Isolation(child_surface) = &scaffold.surfaces[1].kind else {
            panic!("child isolation")
        };
        assert_eq!(root_surface.local_raster_clips[0].id, clip_id);
        assert!(root_surface.ancestor_composite_clips.is_empty());
        assert!(child_surface.local_raster_clips.is_empty());
        assert_eq!(child_surface.ancestor_composite_clips[0].id, clip_id);
        assert_eq!(
            child_surface.composite.resolved_scissor,
            Some([2, 3, 20, 18])
        );

        let production = plan_property_effect_scene_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect("exact clip split must materialize");
        let PaintPlanStep::RetainedSurface(production_root) = &production.steps[0] else {
            panic!("root effect surface")
        };
        let root_local_clip = production_root
            .raster_steps
            .iter()
            .find_map(|step| match step {
                PaintPlanStep::ArtifactSpan(span) => span.artifact.clip_nodes.first(),
                PaintPlanStep::RetainedSurface(_) => None,
            })
            .expect("root raster keeps its local clip");
        assert_eq!(root_local_clip.id, clip_id);
        assert_eq!(root_local_clip.parent, None);
        let mut viewport = Viewport::new();
        let mut graph = FrameGraph::new();
        let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
        super::super::build_retained_property_scene_with_forced_pool_for_test(
            &mut viewport,
            &production,
            &mut graph,
            ctx,
        )
        .expect("clip-bearing effect scene preflight and emit");
        let composite_scissors = graph
            .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
            .into_iter()
            .map(|pass| pass.test_snapshot().effective_scissor_rect)
            .collect::<Vec<_>>();
        assert_eq!(
            composite_scissors,
            vec![Some([2, 3, 20, 18]), Some([2, 3, 20, 18]), None],
            "inherited clip belongs on descendant composite edges, not root raster composite"
        );

        let mut clip_drift = plan.clone();
        let scaffold = clip_drift
            .property_scene_seal
            .as_mut()
            .and_then(|seal| seal.effect_scaffold.as_mut())
            .expect("effect scaffold");
        let PropertyEffectSurfaceKind::Isolation(root_surface) = &mut scaffold.surfaces[0].kind
        else {
            panic!("root isolation")
        };
        root_surface.local_raster_clips[0].generation += 1;
        assert!(!property_scene_plan_is_sealed(&clip_drift));

        let mut inherited_clip_erasure = plan.clone();
        let scaffold = inherited_clip_erasure
            .property_scene_seal
            .as_mut()
            .and_then(|seal| seal.effect_scaffold.as_mut())
            .expect("effect scaffold");
        for surfaces in [&mut scaffold.surfaces, &mut scaffold.planned_surfaces] {
            let PropertyEffectSurfaceKind::Isolation(child_surface) = &mut surfaces[1].kind else {
                panic!("child isolation")
            };
            child_surface.ancestor_composite_clips.clear();
            child_surface.composite.resolved_scissor = None;
        }
        assert!(!property_scene_plan_is_sealed(&inherited_clip_erasure));

        let mut local_parent_drift = plan.clone();
        let scaffold = local_parent_drift
            .property_scene_seal
            .as_mut()
            .and_then(|seal| seal.effect_scaffold.as_mut())
            .expect("effect scaffold");
        for surfaces in [&mut scaffold.surfaces, &mut scaffold.planned_surfaces] {
            let PropertyEffectSurfaceKind::Isolation(root_surface) = &mut surfaces[0].kind else {
                panic!("root isolation")
            };
            root_surface.local_raster_clips[0].parent = Some(clip_id);
            root_surface.raster_identity.local_raster_clips[0].parent = Some(clip_id);
        }
        assert!(!property_scene_plan_is_sealed(&local_parent_drift));

        let mut forest_terminal_drift = plan.clone();
        let scaffold = forest_terminal_drift
            .property_scene_seal
            .as_mut()
            .and_then(|seal| seal.effect_scaffold.as_mut())
            .expect("effect scaffold");
        for forest in [&mut scaffold.clip_forest, &mut scaffold.planned_clip_forest] {
            forest.nodes[0].parent = Some(forest.nodes[0].id);
        }
        assert!(!property_scene_plan_is_sealed(&forest_terminal_drift));

        let mut role_behavior_drift = plan.clone();
        let scaffold = role_behavior_drift
            .property_scene_seal
            .as_mut()
            .and_then(|seal| seal.effect_scaffold.as_mut())
            .expect("effect scaffold");
        for forest in [&mut scaffold.clip_forest, &mut scaffold.planned_clip_forest] {
            forest.nodes[0].behavior = ClipBehavior::Intersect;
        }
        for surfaces in [&mut scaffold.surfaces, &mut scaffold.planned_surfaces] {
            let PropertyEffectSurfaceKind::Isolation(root_surface) = &mut surfaces[0].kind else {
                panic!("root isolation")
            };
            root_surface.local_raster_clips[0].behavior = ClipBehavior::Intersect;
            root_surface.raster_identity.local_raster_clips[0].behavior = ClipBehavior::Intersect;
        }
        assert!(!property_scene_plan_is_sealed(&role_behavior_drift));
    }

    #[test]
    fn property_effect_scaffold_rejects_clip_role_behavior_mismatch_at_admission() {
        for (role, behavior) in [
            (ClipNodeRole::SelfClip, ClipBehavior::Intersect),
            (ClipNodeRole::ContentsClip, ClipBehavior::Replace),
        ] {
            let (arena, root, _, _, mut properties, generations) =
                planning_only_nested_effect_fixture();
            let clip_id = ClipNodeId { owner: root, role };
            properties.clips.insert(
                clip_id,
                crate::view::compositor::property_tree::ClipNode {
                    owner: root,
                    parent: None,
                    geometry: ClipGeometry::LogicalScissor([2, 3, 20, 18]),
                    behavior,
                    generation: 41,
                },
            );
            let state_keys = properties.states.keys().copied().collect::<Vec<_>>();
            for key in state_keys {
                let state = properties.states.get_mut(&key).expect("state");
                if key == root && role == ClipNodeRole::ContentsClip {
                    state.paint.clip = None;
                } else {
                    state.paint.clip = Some(clip_id);
                }
                state.descendants.clip = Some(clip_id);
            }
            let error = plan_property_effect_scene_scaffold_with_context(
                &arena,
                &[root],
                &FxHashSet::default(),
                &properties,
                &generations,
                TransformSurfacePlanContext::new([0.0, 0.0], None),
            )
            .expect_err("clip role and behavior pairing must be exact");
            assert!(
                error
                    .reasons
                    .contains(&FramePaintPlanRejection::InvalidClipChain(root))
            );
        }
    }

    #[test]
    fn property_effect_scaffold_rejects_stale_live_generation_fingerprint() {
        let (arena, root, _, _, properties, generations) = planning_only_nested_effect_fixture();
        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(210, 30, 70)),
        );
        crate::view::test_support::get_element_mut::<Element>(&arena, root).apply_style(style);
        assert!(!generations.matches_live_snapshot(&arena, &[root], &properties));
        let error = plan_property_effect_scene_scaffold_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect_err("stale generations cannot mint an artifact-input fingerprint");
        assert_eq!(
            error.reasons,
            vec![FramePaintPlanRejection::InvalidPropertyScene]
        );
    }

    #[test]
    fn property_effect_scaffold_seal_rejects_effect_clip_root_and_dependency_drift() {
        let (arena, root, _, _, properties, generations) = planning_only_nested_effect_fixture();
        let build = || {
            plan_property_effect_scene_scaffold_with_context(
                &arena,
                &[root],
                &FxHashSet::default(),
                &properties,
                &generations,
                TransformSurfacePlanContext::new([0.0, 0.0], None),
            )
            .expect("sealed scaffold")
        };
        let mut effect_drift = build();
        let scaffold = effect_drift
            .property_scene_seal
            .as_mut()
            .and_then(|seal| seal.effect_scaffold.as_mut())
            .expect("scaffold");
        let PropertyEffectSurfaceKind::Isolation(effect) = &mut scaffold.surfaces[1].kind else {
            panic!("effect")
        };
        effect.composite.effect_generation += 1;
        assert!(!property_scene_plan_is_sealed(&effect_drift));

        let mut reparent_drift = build();
        let scaffold = reparent_drift
            .property_scene_seal
            .as_mut()
            .and_then(|seal| seal.effect_scaffold.as_mut())
            .expect("scaffold");
        let PropertyEffectSurfaceKind::Isolation(effect) = &mut scaffold.surfaces[1].kind else {
            panic!("effect")
        };
        effect.effect_chain.live_leaf_to_root[0].parent = None;
        assert!(!property_scene_plan_is_sealed(&reparent_drift));

        let mut ancestor_snapshot_drift = build();
        let scaffold = ancestor_snapshot_drift
            .property_scene_seal
            .as_mut()
            .and_then(|seal| seal.effect_scaffold.as_mut())
            .expect("scaffold");
        for surfaces in [&mut scaffold.surfaces, &mut scaffold.planned_surfaces] {
            let PropertyEffectSurfaceKind::Isolation(effect) = &mut surfaces[2].kind else {
                panic!("grandchild effect")
            };
            effect.effect_chain.live_leaf_to_root[2].opacity = 0.625;
            effect.effect_chain.live_leaf_to_root[2].generation += 1;
            effect.effect_chain.detached_ancestors[1].opacity = 0.625;
            effect.effect_chain.detached_ancestors[1].generation += 1;
        }
        assert!(!property_scene_plan_is_sealed(&ancestor_snapshot_drift));

        let mut child_content_drift = build();
        let scaffold = child_content_drift
            .property_scene_seal
            .as_mut()
            .and_then(|seal| seal.effect_scaffold.as_mut())
            .expect("scaffold");
        for surfaces in [&mut scaffold.surfaces, &mut scaffold.planned_surfaces] {
            let PropertyEffectSurfaceKind::Isolation(effect) = &mut surfaces[1].kind else {
                panic!("child effect")
            };
            effect.raster_identity.content[0].self_paint_revision += 1;
        }
        assert!(!property_scene_plan_is_sealed(&child_content_drift));

        let mut root_content_topology_drift = build();
        let scaffold = root_content_topology_drift
            .property_scene_seal
            .as_mut()
            .and_then(|seal| seal.effect_scaffold.as_mut())
            .expect("scaffold");
        for surfaces in [&mut scaffold.surfaces, &mut scaffold.planned_surfaces] {
            let PropertyEffectSurfaceKind::Isolation(effect) = &mut surfaces[0].kind else {
                panic!("root effect")
            };
            assert!(effect.raster_identity.content.len() > 1);
            effect.raster_identity.content[1].parent = None;
        }
        assert!(!property_scene_plan_is_sealed(&root_content_topology_drift));

        let mut dependency_drift = build();
        let scaffold = dependency_drift
            .property_scene_seal
            .as_mut()
            .and_then(|seal| seal.effect_scaffold.as_mut())
            .expect("scaffold");
        let PropertyEffectSurfaceKind::Isolation(effect) = &mut scaffold.surfaces[0].kind else {
            panic!("effect")
        };
        effect.nested_dependencies[0].child_opacity_bits ^= 1;
        assert!(!property_scene_plan_is_sealed(&dependency_drift));

        let mut root_drift = build();
        root_drift
            .property_scene_seal
            .as_mut()
            .and_then(|seal| seal.effect_scaffold.as_mut())
            .expect("scaffold")
            .roots[0]
            .boundary_ordinal_span = 1..3;
        assert!(!property_scene_plan_is_sealed(&root_drift));

        let mut context_drift = build();
        context_drift
            .property_scene_seal
            .as_mut()
            .expect("seal")
            .context = TransformSurfacePlanContext::new([9.0, 0.0], None);
        assert!(!property_scene_plan_is_sealed(&context_drift));

        let mut outer_scissor_drift = build();
        outer_scissor_drift
            .property_scene_seal
            .as_mut()
            .expect("seal")
            .outer_scissor_rect = Some([1, 2, 3, 4]);
        assert!(!property_scene_plan_is_sealed(&outer_scissor_drift));

        let mut resolved_scissor_drift = build();
        let scaffold = resolved_scissor_drift
            .property_scene_seal
            .as_mut()
            .and_then(|seal| seal.effect_scaffold.as_mut())
            .expect("scaffold");
        let PropertyEffectSurfaceKind::Isolation(effect) = &mut scaffold.surfaces[0].kind else {
            panic!("effect")
        };
        effect.composite.resolved_scissor = Some([1, 2, 3, 4]);
        assert!(!property_scene_plan_is_sealed(&resolved_scissor_drift));
    }

    #[test]
    fn property_effect_scaffold_preserves_mixed_root_and_boundary_dfs_order() {
        let (mut arena, first_root, _, _, _, _) = planning_only_nested_effect_fixture();
        let mut second = Element::new_with_id(0xea_2001, 120.0, 4.0, 12.0, 9.0);
        second.set_opacity(0.25);
        let second_root = commit_element(&mut arena, Box::new(second));
        let neutral_root = commit_element(
            &mut arena,
            Box::new(Element::new_with_id(0xea_2002, 138.0, 4.0, 8.0, 8.0)),
        );
        let constraints = LayoutConstraints {
            max_width: 160.0,
            max_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        };
        for (root, x) in [(second_root, 120.0), (neutral_root, 138.0)] {
            measure_and_place(
                &mut arena,
                root,
                constraints,
                LayoutPlacement {
                    parent_x: x,
                    parent_y: 4.0,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: 160.0,
                    available_height: 120.0,
                    viewport_width: 160.0,
                    viewport_height: 120.0,
                    percent_base_width: Some(160.0),
                    percent_base_height: Some(120.0),
                },
            );
        }
        let roots = [second_root, neutral_root, first_root];
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &roots, &properties);
        let plan = plan_property_effect_scene_scaffold_with_context(
            &arena,
            &roots,
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect("multi-root effect scaffold");
        let scaffold = plan
            .property_scene_seal
            .as_ref()
            .and_then(|seal| seal.effect_scaffold.as_ref())
            .expect("effect scaffold");
        assert_eq!(
            scaffold
                .roots
                .iter()
                .map(|root| root.root)
                .collect::<Vec<_>>(),
            roots
        );
        assert_eq!(scaffold.roots[0].boundary_ordinal_span, 0..1);
        assert_eq!(scaffold.roots[1].boundary_ordinal_span, 1..1);
        assert_eq!(scaffold.roots[2].boundary_ordinal_span, 1..4);
        assert_eq!(scaffold.surfaces[0].boundary.owner(), second_root);
        assert_eq!(scaffold.surfaces[1].boundary.owner(), first_root);

        let production = plan_property_effect_scene_with_context(
            &arena,
            &roots,
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], None),
        )
        .expect("multi-root effect forest must materialize");
        let witness = production
            .property_scene_transaction_witness()
            .expect("multi-root effect transaction");
        assert_eq!(witness.roots.len(), 3);
        assert_eq!(witness.surfaces.len(), 4);
        assert!(witness.roots[1].top_level_step_span.is_empty());
        let mut viewport = Viewport::new();
        let mut graph = FrameGraph::new();
        let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
        let outcome = super::super::build_retained_property_scene_with_forced_pool_for_test(
            &mut viewport,
            &production,
            &mut graph,
            ctx,
        )
        .expect("multi-root effect forest preflight and emit");
        let (_, trace) = outcome.into_parts();
        assert_eq!(trace.root_count, 3);
        assert_eq!(trace.surface_count, 4);
    }

    fn nested_opaque_cursor_fixture(
        parent_before_opaque: usize,
        child_opaque: usize,
        parent_after_opaque: usize,
    ) -> (
        NodeArena,
        NodeKey,
        NodeKey,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        assert!(child_opaque > 0);
        let element = |id: u64, width: f32, height: f32, opaque: bool| {
            let mut element = Element::new_with_id(id, 0.0, 0.0, width, height);
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            if opaque {
                style.insert(
                    PropertyId::BackgroundColor,
                    ParsedValue::color_like(Color::rgb(40, 100, 180)),
                );
            }
            element.apply_style(style);
            element
        };

        let mut next_id = 0xc5_a200_u64;
        let mut take_id = || {
            let id = next_id;
            next_id += 1;
            id
        };
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(element(take_id(), 80.0, 60.0, parent_before_opaque > 0)),
        );
        for _ in 1..parent_before_opaque {
            commit_child(
                &mut arena,
                root,
                Box::new(element(take_id(), 4.0, 4.0, true)),
            );
        }
        let child = commit_child(
            &mut arena,
            root,
            Box::new(element(take_id(), 20.0, 16.0, true)),
        );
        for _ in 1..child_opaque {
            commit_child(
                &mut arena,
                child,
                Box::new(element(take_id(), 3.0, 3.0, true)),
            );
        }
        for _ in 0..parent_after_opaque {
            commit_child(
                &mut arena,
                root,
                Box::new(element(take_id(), 4.0, 4.0, true)),
            );
        }

        measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 160.0,
                max_height: 120.0,
                viewport_width: 160.0,
                viewport_height: 120.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 160.0,
                available_height: 120.0,
                viewport_width: 160.0,
                viewport_height: 120.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(120.0),
            },
        );
        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                10.0, 0.0, 0.0,
            ))));
        crate::view::test_support::get_element_mut::<Element>(&arena, child)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                20.0, 0.0, 0.0,
            ))));
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        (arena, root, child, properties, generations)
    }

    fn only_surface(plan: &FramePaintPlan) -> &RetainedSurfacePlan {
        let [PaintPlanStep::RetainedSurface(surface)] = plan.steps.as_slice() else {
            panic!("fixture must contain one retained surface")
        };
        surface
    }

    fn only_surface_mut(plan: &mut FramePaintPlan) -> &mut RetainedSurfacePlan {
        let [PaintPlanStep::RetainedSurface(surface)] = plan.steps.as_mut_slice() else {
            panic!("fixture must contain one retained surface")
        };
        surface
    }

    fn only_span(surface: &RetainedSurfacePlan) -> &ArtifactSpanPlan {
        let [PaintPlanStep::ArtifactSpan(span)] = surface.raster_steps.as_slice() else {
            panic!("fixture surface must contain one artifact span")
        };
        span
    }

    fn only_span_mut(surface: &mut RetainedSurfacePlan) -> &mut ArtifactSpanPlan {
        let [PaintPlanStep::ArtifactSpan(span)] = surface.raster_steps.as_mut_slice() else {
            panic!("fixture surface must contain one artifact span")
        };
        span
    }

    fn nested_surface_mut(plan: &mut FramePaintPlan) -> &mut RetainedSurfacePlan {
        let parent = only_surface_mut(plan);
        parent
            .raster_steps
            .iter_mut()
            .find_map(|step| match step {
                PaintPlanStep::RetainedSurface(surface) => Some(surface.as_mut()),
                PaintPlanStep::ArtifactSpan(_) => None,
            })
            .expect("fixture contains one nested surface")
    }

    fn isolation_plan_mut(plan: &mut FramePaintPlan) -> &mut IsolationSurfacePlan {
        match &mut only_surface_mut(plan).kind {
            SurfaceKind::Isolation(plan) => plan,
            SurfaceKind::Transform(_)
            | SurfaceKind::NestedIsolation(_)
            | SurfaceKind::ScrollHost(_) => {
                panic!("fixture must contain root isolation surface")
            }
        }
    }

    fn assert_forced_rejection_has_zero_graph_mutation(
        plan: &FramePaintPlan,
        graph: &mut FrameGraph,
        ctx: UiBuildContext,
        expected: super::super::ForcedTransformSurfaceError,
    ) {
        let before = graph.build_state_snapshot_for_test();
        let mut viewport = Viewport::new();
        let viewport_before = viewport.retained_surface_transaction_shape_for_test();
        let error = match super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            plan,
            graph,
            ctx,
        ) {
            Ok(_) => panic!("tampered forced plan must reject before emit"),
            Err(error) => error,
        };
        assert_eq!(error, expected);
        assert_eq!(graph.build_state_snapshot_for_test(), before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            viewport_before,
            "prepare rejection cannot stage or commit any retained-surface transaction"
        );
    }

    fn commit_forced_nested_plan(viewport: &mut Viewport, plan: &FramePaintPlan) {
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let outer = ctx.allocate_target(&mut graph);
        ctx.set_current_target(outer);
        super::super::execute_forced_transform_surface_for_test(viewport, plan, &mut graph, ctx)
            .expect("baseline nested R/R execution");
        viewport.finish_retained_surface_transaction(true);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (2, None)
        );
    }

    fn parent_context_with_clear(
        graph: &mut FrameGraph,
        width: u32,
        height: u32,
        scale: f32,
    ) -> (
        UiBuildContext,
        crate::view::render_pass::draw_rect_pass::RenderTargetOut,
    ) {
        let mut ctx = UiBuildContext::new(width, height, wgpu::TextureFormat::Bgra8Unorm, scale);
        let parent = ctx.allocate_target(graph);
        ctx.set_current_target(parent);
        graph.add_graphics_pass(crate::view::render_pass::ClearPass::new(
            crate::view::render_pass::clear_pass::ClearParams::new([0.0, 0.0, 0.0, 0.0]),
            crate::view::render_pass::clear_pass::ClearInput {
                pass_context: ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            crate::view::render_pass::clear_pass::ClearOutput {
                render_target: parent,
            },
        ));
        (ctx, parent)
    }

    fn parent_context_without_clear(
        graph: &mut FrameGraph,
        width: u32,
        height: u32,
        scale: f32,
    ) -> UiBuildContext {
        let mut ctx = UiBuildContext::new(width, height, wgpu::TextureFormat::Bgra8Unorm, scale);
        let parent = ctx.allocate_target(graph);
        ctx.set_current_target(parent);
        ctx
    }

    fn execute_forced_plan_graph(viewport: &mut Viewport, plan: &FramePaintPlan) -> FrameGraph {
        let mut graph = FrameGraph::new();
        let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
        super::super::execute_forced_transform_surface_for_test(viewport, plan, &mut graph, ctx)
            .expect("forced exact retained plan");
        graph
    }

    fn retained_surface_stamp(
        surface: &RetainedSurfacePlan,
        artifact: &PaintArtifact,
    ) -> Option<super::super::RetainedSurfaceRasterStamp> {
        let scale = 2.0_f32;
        let color_key = surface.persistent_color_key;
        let color = crate::view::base_component::texture_desc_for_logical_bounds(
            surface.geometry().source_bounds,
            scale,
            None,
            wgpu::TextureFormat::Bgra8Unorm,
        );
        let (color, depth) =
            crate::view::base_component::persistent_target_texture_descriptors(color, color_key);
        super::super::validated_retained_surface_raster_stamp(
            artifact,
            surface.boundary_root,
            surface.stable_id,
            surface.transform(),
            super::super::RetainedSurfaceRasterInputs {
                color,
                depth,
                scale_factor_bits: scale.to_bits(),
                source_bounds_bits: [
                    surface.geometry().source_bounds.x.to_bits(),
                    surface.geometry().source_bounds.y.to_bits(),
                    surface.geometry().source_bounds.width.to_bits(),
                    surface.geometry().source_bounds.height.to_bits(),
                ],
            },
            surface.aggregate_opaque_order_span.clone(),
        )
    }

    #[test]
    fn planner_rejects_negative_origin_known_legacy_crop_before_execution() {
        for (root_id, child_id, root_x, root_y, expected_source) in [
            (0xc2_a001, 0xc2_a002, -4.25, 3.5, [-8.5, 7.0]),
            (0xc2_a003, 0xc2_a004, 4.25, -3.5, [8.5, -7.0]),
        ] {
            let (arena, root, properties, generations) =
                exact_transform_fixture_at_origin_with_ids(root_id, child_id, root_x, root_y);
            let geometry = arena
                .get(root)
                .expect("root")
                .element
                .as_any()
                .downcast_ref::<Element>()
                .expect("Element root")
                .transform_surface_geometry_snapshot(&arena, [0.0, 0.0], None)
                .expect("finite negative-origin geometry remains representable");
            assert_eq!(
                [
                    geometry.source_bounds.x.to_bits(),
                    geometry.source_bounds.y.to_bits(),
                ],
                expected_source.map(f32::to_bits)
            );

            let error = plan_single_root_transform_surface(
                &arena,
                &[root],
                &FxHashSet::default(),
                &properties,
                &generations,
            )
            .expect_err("known legacy crop must not reach C2 target declaration");
            assert_eq!(
                error.reasons,
                vec![FramePaintPlanRejection::NegativeSurfaceOrigin(root)]
            );
        }
    }

    #[test]
    fn exact_single_root_transform_builds_one_planning_only_surface_step() {
        let (arena, root, properties, generations) = exact_transform_fixture();
        let plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("exact single-root transform subtree must be plan-eligible");

        let [PaintPlanStep::RetainedSurface(surface)] = plan.steps.as_slice() else {
            panic!("M10C1 must produce exactly one retained surface step")
        };
        assert_eq!(surface.boundary_root, root);
        assert_eq!(surface.transform(), TransformNodeId(root));
        assert_eq!(surface.parent_surface, None);
        assert!(surface.geometry().outer_scissor_rect.is_none());
        let span = only_span(surface);
        assert!(!span.artifact.chunks.is_empty());
        assert!(span.artifact.chunks.iter().all(|chunk| {
            chunk.properties.transform == Some(TransformNodeId(root))
                && chunk.properties.clip.is_none()
                && chunk.properties.effect.is_none()
                && chunk.properties.scroll.is_none()
        }));
        assert!(
            super::super::compiler::validate_transform_surface_artifact_for_plan(
                &span.artifact,
                root,
                TransformNodeId(root),
            )
        );
    }

    #[test]
    fn transform_child_isolation_recording_projects_only_inherited_transform_and_partitions_ownership()
     {
        let (arena, root, before, child, descendant, after, properties, generations) =
            exact_transform_child_isolation_fixture();
        let effect = crate::view::compositor::property_tree::EffectNodeId(child);
        let boundary = super::super::PlannedBoundary {
            root: child,
            stable_id: arena.get(child).unwrap().element.stable_id(),
            kind: super::super::PlannedBoundaryKind::Isolation(effect),
        };
        let cutouts = super::super::PlannedBoundaryCutoutSet::from_iter([(child, boundary)]);
        let parent_steps = super::super::frame_recorder::record_transform_surface_steps_for_plan(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            PaintTransformSurfaceWitness::canonical_root(root),
            [0.0, 0.0],
            &cutouts,
        )
        .expect("typed isolation cutout keeps parent transform stream recordable");
        let [
            super::super::frame_recorder::RecordedTransformSurfaceStep::Artifact(before_artifact),
            super::super::frame_recorder::RecordedTransformSurfaceStep::Boundary(actual),
            super::super::frame_recorder::RecordedTransformSurfaceStep::Artifact(after_artifact),
        ] = parent_steps.as_slice()
        else {
            panic!("parent stream must flush before and after exactly one isolation marker")
        };
        assert_eq!(*actual, boundary);

        let child_artifact =
            super::super::frame_recorder::record_transform_child_isolation_artifact_for_plan(
                &arena,
                root,
                child,
                &FxHashSet::default(),
                &properties,
                &generations,
            )
            .expect("exact child isolation must record with consumed parent transform");
        assert!(matches!(
            child_artifact.target,
            super::super::PaintArtifactTarget::RootOpacityGroup { root, effect: actual }
                if root == child && actual == effect
        ));
        assert_eq!(child_artifact.effect_nodes.len(), 1);
        assert_eq!(child_artifact.effect_nodes[0].id, effect);
        assert!(child_artifact.chunks.iter().all(|chunk| {
            chunk.properties.transform.is_none()
                && chunk.properties.effect == Some(effect)
                && chunk.properties.clip.is_none()
                && chunk.properties.scroll.is_none()
        }));
        child_artifact.ops.iter().for_each(|op| match op {
            PaintOp::DrawRect(op) => assert_eq!(op.params.opacity.to_bits(), 1.0_f32.to_bits()),
            PaintOp::PreparedInlineIfcDecoration(op) => {
                assert_eq!(op.fill.opacity.to_bits(), 1.0_f32.to_bits());
                if let Some(border) = &op.border {
                    assert_eq!(border.opacity.to_bits(), 1.0_f32.to_bits());
                }
            }
            PaintOp::PreparedShadow(op) => {
                assert_eq!(op.params.opacity.to_bits(), 1.0_f32.to_bits())
            }
            PaintOp::PreparedScrollbarOverlay(op) => {
                assert!(op.has_baked_opacity(1.0_f32.to_bits()))
            }
            PaintOp::PreparedText(op) => assert!(
                op.params
                    .staging_input
                    .glyphs
                    .iter()
                    .all(|glyph| glyph.paint.opacity.to_bits() == 1.0_f32.to_bits())
            ),
            PaintOp::PreparedImage(op) => {
                assert_eq!(op.params.opacity.to_bits(), 1.0_f32.to_bits())
            }
            PaintOp::PreparedSvg(op) => {
                assert_eq!(op.params.opacity.to_bits(), 1.0_f32.to_bits())
            }
        });

        let parent_chunk_owners = before_artifact
            .chunks
            .iter()
            .chain(&after_artifact.chunks)
            .map(|chunk| chunk.owner)
            .collect::<FxHashSet<_>>();
        let child_chunk_owners = child_artifact
            .chunks
            .iter()
            .map(|chunk| chunk.owner)
            .collect::<FxHashSet<_>>();
        assert!(parent_chunk_owners.is_disjoint(&child_chunk_owners));
        assert_eq!(
            parent_chunk_owners,
            FxHashSet::from_iter([root, before, after])
        );
        assert_eq!(
            child_chunk_owners,
            FxHashSet::from_iter([child, descendant])
        );
        let all = parent_chunk_owners
            .union(&child_chunk_owners)
            .copied()
            .collect::<FxHashSet<_>>();
        assert_eq!(
            all,
            FxHashSet::from_iter([root, before, child, descendant, after]),
            "parent spans plus child artifact must exhaust canonical paint ownership"
        );
    }

    #[test]
    fn transform_child_isolation_recording_rejects_wrong_boundary_and_live_projection_mismatch() {
        let (arena, root, before, child, _, _, mut properties, generations) =
            exact_transform_child_isolation_fixture();
        assert!(
            super::super::frame_recorder::record_transform_child_isolation_artifact_for_plan(
                &arena,
                before,
                child,
                &FxHashSet::default(),
                &properties,
                &generations,
            )
            .is_err(),
            "a non-parent boundary cannot mint consumed-transform authority"
        );

        properties.states.get_mut(&child).unwrap().paint.transform = None;
        assert!(
            super::super::frame_recorder::record_transform_child_isolation_artifact_for_plan(
                &arena,
                root,
                child,
                &FxHashSet::default(),
                &properties,
                &generations,
            )
            .is_err(),
            "live property mismatch must reject before projected recording"
        );

        let (arena, root, _, child, descendant, _, properties, generations) =
            exact_transform_child_isolation_fixture();
        let mut deferred_style = Style::new();
        deferred_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                crate::style::Position::absolute()
                    .left(crate::style::Length::px(0.0))
                    .clip(crate::style::ClipMode::Viewport),
            ),
        );
        crate::view::test_support::get_element_mut::<Element>(&arena, descendant)
            .apply_style(deferred_style);
        let _ = super::super::take_full_artifact_record_count();
        let error =
            super::super::frame_recorder::record_transform_child_isolation_artifact_for_plan(
                &arena,
                root,
                child,
                &FxHashSet::default(),
                &properties,
                &generations,
            )
            .expect_err("deferred descendants must fail before either artifact recording pass");
        assert_eq!(
            error,
            vec![super::super::FrameArtifactFallbackReason::DeferredBoundary(
                descendant
            )]
        );
        assert_eq!(
            super::super::take_full_artifact_record_count(),
            0,
            "deferred preflight must reject before the full artifact pass"
        );
    }

    #[test]
    fn transform_child_isolation_planner_freezes_exact_fractional_geometry_and_cursors() {
        let (arena, root, _, child, _, _, properties, generations) =
            exact_transform_child_isolation_fixture();
        let parent_snapped_offset = arena
            .get(root)
            .unwrap()
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap()
            .retained_child_paint_offset([0.0, 0.0])
            .unwrap();
        let exact_child_bounds = arena
            .get(child)
            .unwrap()
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap()
            .exact_nested_isolation_render_output_bounds(&arena, parent_snapped_offset)
            .unwrap();
        assert!(exact_child_bounds.x > 0.0 && exact_child_bounds.y > 0.0);
        assert!(
            exact_child_bounds.x.fract() != 0.0 || exact_child_bounds.y.fract() != 0.0,
            "fixture must retain a positive fractional nonzero child-local origin"
        );

        let plan = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("exact Transform -> direct child Isolation plan");
        let root_surface = only_surface(&plan);
        assert!(matches!(root_surface.kind(), SurfaceKind::Transform(_)));
        let [
            PaintPlanStep::ArtifactSpan(before),
            PaintPlanStep::RetainedSurface(child_surface),
            PaintPlanStep::ArtifactSpan(after),
        ] = root_surface.raster_steps()
        else {
            panic!("parent recorder must preserve before-marker-after order")
        };
        let SurfaceKind::NestedIsolation(nested) = child_surface.kind() else {
            panic!("typed marker must become a nested-isolation role")
        };
        assert_eq!(child_surface.boundary_root(), child);
        assert_eq!(child_surface.parent_surface(), Some(root));
        assert!(nested.geometry.bitwise_eq(nested.planned_geometry_witness));
        assert_eq!(
            [
                nested.geometry.source_bounds.x,
                nested.geometry.source_bounds.y,
                nested.geometry.source_bounds.width,
                nested.geometry.source_bounds.height,
            ]
            .map(f32::to_bits),
            [
                exact_child_bounds.x,
                exact_child_bounds.y,
                exact_child_bounds.width,
                exact_child_bounds.height,
            ]
            .map(f32::to_bits)
        );
        assert_eq!(
            nested.geometry.logical_size(),
            [exact_child_bounds.width, exact_child_bounds.height,]
        );
        assert_eq!(nested.geometry.source_bounds.corner_radii, [0.0; 4]);

        let [PaintPlanStep::ArtifactSpan(child_span)] = child_surface.raster_steps() else {
            panic!("nested isolation owns exactly one projected artifact")
        };
        let child_terminal = opaque_order_count(child_span.artifact());
        assert_eq!(child_span.opaque_order_span(), &(0..child_terminal));
        assert_eq!(
            child_surface.aggregate_opaque_order_span(),
            &(0..child_terminal)
        );
        let before_end = opaque_order_count(before.artifact());
        assert_eq!(before.opaque_order_span(), &(0..before_end));
        let expected_after_start = before_end.max(child_terminal);
        assert_eq!(after.opaque_order_span().start, expected_after_start);
        assert_eq!(
            after.opaque_order_span().end,
            expected_after_start + opaque_order_count(after.artifact())
        );
        assert_eq!(
            root_surface.aggregate_opaque_order_span(),
            &(0..after.opaque_order_span().end)
        );
    }

    #[test]
    fn transform_child_isolation_planner_hard_gates_shape_and_extra_properties() {
        let (arena, root, _, child, _, _, properties, generations) =
            exact_transform_child_isolation_fixture();
        let promoted = FxHashSet::from_iter([arena.get(child).unwrap().element.stable_id()]);
        let error = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &promoted,
            &properties,
            &generations,
        )
        .expect_err("promotion is outside the mixed exact slice");
        assert!(matches!(
            error.reasons.as_slice(),
            [FramePaintPlanRejection::PromotionPresent(_)]
        ));

        let (arena, root, _, child, _, _, mut properties, generations) =
            exact_transform_child_isolation_fixture();
        properties.transforms.insert(
            TransformNodeId(child),
            crate::view::compositor::property_tree::TransformNode {
                owner: child,
                parent: Some(TransformNodeId(root)),
                viewport_matrix: glam::Mat4::IDENTITY,
                generation: 1,
            },
        );
        let error = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("a second transform is an extra property boundary");
        assert!(
            error
                .reasons
                .contains(&FramePaintPlanRejection::TransformNodeCount(2))
        );

        let (arena, root, _, child, _, _, mut properties, generations) =
            exact_transform_child_isolation_fixture();
        properties.states.get_mut(&child).unwrap().paint.clip =
            Some(crate::view::compositor::property_tree::ClipNodeId {
                owner: child,
                role: crate::view::compositor::property_tree::ClipNodeRole::SelfClip,
            });
        let error = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("clip state is outside the mixed exact slice");
        assert!(
            error
                .reasons
                .contains(&FramePaintPlanRejection::ClipBoundary(child))
        );

        let (arena, root, _, child, _, _, mut properties, generations) =
            exact_transform_child_isolation_fixture();
        properties.effects.remove(&EffectNodeId(child));
        let error = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("missing child effect snapshot cannot mint a nested isolation");
        assert!(
            error
                .reasons
                .iter()
                .any(|reason| matches!(reason, FramePaintPlanRejection::InvalidIsolationEffect(_)))
        );

        let (arena, root, _, child, descendant, _, properties, generations) =
            exact_transform_child_isolation_fixture();
        crate::view::test_support::get_element_mut::<Element>(&arena, descendant)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                3.0, 2.0, 0.0,
            ))));
        let error = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err(
            "a live descendant transform added after property sync must not produce a plan",
        );
        assert_eq!(
            error.reasons,
            vec![FramePaintPlanRejection::InvalidSurfaceGeometry(child)],
            "child-local geometry must reject stale property trees before artifact recording"
        );
    }

    #[test]
    fn production_mixed_effect_tree_emits_frozen_child_geometry_and_atomic_stamps() {
        let (arena, root, _, child, _, _, properties, generations) =
            exact_transform_child_isolation_fixture();
        let plan = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("exact mixed plan");
        let root_surface = only_surface(&plan);
        let [_, PaintPlanStep::RetainedSurface(child_surface), _] = root_surface.raster_steps()
        else {
            panic!("mixed fixture keeps one typed child boundary")
        };
        let SurfaceKind::NestedIsolation(nested) = child_surface.kind() else {
            panic!("typed nested isolation")
        };
        let bounds = nested.geometry.source_bounds;

        let mut graph = FrameGraph::new();
        let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
        let mut viewport = Viewport::new();
        let outcome = super::super::build_retained_effect_tree_from_pool(
            &mut viewport,
            &plan,
            &mut graph,
            ctx,
        )
        .expect("production mixed executor");
        let (state, traces) = outcome.into_parts();
        assert_eq!(state.opaque_rect_order_for_test(), 3);
        assert_eq!(traces.len(), 2);
        assert_eq!(traces[0].boundary_root, root);
        assert_eq!(traces[1].boundary_root, child);
        assert!(
            traces.iter().all(|trace| {
                trace.action == super::super::RetainedSurfaceCompileAction::Reraster
            })
        );

        let child_key =
            crate::view::base_component::isolation_layer_stable_key(child_surface.stable_id());
        let child_desc = graph
            .declared_persistent_textures()
            .find_map(|(key, desc)| (key == child_key).then_some(desc))
            .expect("child persistent color descriptor");
        let expected_child_desc = crate::view::base_component::texture_desc_for_logical_bounds(
            bounds,
            1.0,
            None,
            wgpu::TextureFormat::Bgra8Unorm,
        );
        assert_eq!(
            [child_desc.width(), child_desc.height()],
            [expected_child_desc.width(), expected_child_desc.height()],
            "child target descriptor is derived only from frozen bounds and scale"
        );
        assert_eq!(child_desc.origin(), expected_child_desc.origin());
        assert_ne!(
            child_desc.origin(),
            (0, 0),
            "positive fractional fixture must preserve a nonzero child-local target origin"
        );
        assert_eq!(
            traces[1].descriptor_size,
            [expected_child_desc.width(), expected_child_desc.height()]
        );

        let composites = graph.test_graphics_passes::<
            crate::view::render_pass::composite_layer_pass::CompositeLayerPass,
        >();
        let [composite] = composites.as_slice() else {
            panic!("one child-local CompositeLayer")
        };
        assert_eq!(
            composite.test_params().rect_pos.map(f32::to_bits),
            [bounds.x.to_bits(), bounds.y.to_bits(),]
        );
        assert_eq!(
            composite.test_params().rect_size.map(f32::to_bits),
            [bounds.width.to_bits(), bounds.height.to_bits(),]
        );
        assert_eq!(composite.test_params().corner_radii, [0.0; 4]);
        assert_eq!(
            composite.test_params().opacity.to_bits(),
            nested.effect.opacity.to_bits()
        );
        assert_eq!(composite.test_params().scissor_rect, None);

        let final_composites =
            graph.test_graphics_passes::<crate::view::render_pass::TextureCompositePass>();
        let [final_composite] = final_composites.as_slice() else {
            panic!("root CSS transform is emitted exactly once")
        };
        let root_geometry = root_surface.transform_plan_for_test().geometry;
        let final_snapshot = final_composite.test_snapshot();
        assert_eq!(
            final_snapshot.quad_position_bits,
            Some(
                root_geometry
                    .quad_positions
                    .map(|point| point.map(f32::to_bits))
            )
        );
        assert_eq!(
            final_snapshot.uv_bounds_bits,
            Some(root_geometry.uv_bounds.map(f32::to_bits))
        );
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, Some(2))
        );
        viewport.finish_retained_surface_transaction(false);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, None)
        );
    }

    #[test]
    fn mixed_effect_tree_cpu_oracle_applies_group_opacity_once_after_source_over() {
        fn premultiplied(fill: [f32; 4], opacity: f32) -> [f32; 4] {
            let alpha = fill[3] * opacity;
            [fill[0] * alpha, fill[1] * alpha, fill[2] * alpha, alpha]
        }
        fn source_over(dst: [f32; 4], src: [f32; 4]) -> [f32; 4] {
            let remainder = 1.0 - src[3];
            [
                src[0] + dst[0] * remainder,
                src[1] + dst[1] * remainder,
                src[2] + dst[2] * remainder,
                src[3] + dst[3] * remainder,
            ]
        }
        fn scaled(color: [f32; 4], opacity: f32) -> [f32; 4] {
            color.map(|channel| channel * opacity)
        }

        let (arena, root, _, _, _, _, properties, generations) =
            exact_transform_child_isolation_fixture();
        let plan = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .unwrap();
        let [_, PaintPlanStep::RetainedSurface(child), _] = only_surface(&plan).raster_steps()
        else {
            panic!("mixed child")
        };
        let SurfaceKind::NestedIsolation(isolation) = child.kind() else {
            panic!("nested isolation")
        };
        let artifact = only_span(child).artifact();
        let rects = artifact
            .ops
            .iter()
            .filter_map(|op| match op {
                PaintOp::DrawRect(op) => Some(&op.params),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [bottom, top] = rects.as_slice() else {
            panic!("fixture child artifact owns two overlapping rects")
        };
        let overlap_width = (bottom.position[0] + bottom.size[0])
            .min(top.position[0] + top.size[0])
            - bottom.position[0].max(top.position[0]);
        let overlap_height = (bottom.position[1] + bottom.size[1])
            .min(top.position[1] + top.size[1])
            - bottom.position[1].max(top.position[1]);
        assert!(overlap_width > 0.0 && overlap_height > 0.0);
        assert_eq!(bottom.opacity.to_bits(), 1.0_f32.to_bits());
        assert_eq!(top.opacity.to_bits(), 1.0_f32.to_bits());

        let group = source_over(
            premultiplied(bottom.fill_color, bottom.opacity),
            premultiplied(top.fill_color, top.opacity),
        );
        let correct = scaled(group, isolation.effect.opacity);
        let manual_top_once = premultiplied(top.fill_color, isolation.effect.opacity);
        assert!(
            correct
                .iter()
                .zip(manual_top_once)
                .all(|(actual, expected)| (actual - expected).abs() <= f32::EPSILON),
            "opaque top rect resolves the group, then root opacity scales premultiplied RGBA once"
        );

        let incorrectly_baked_then_grouped = scaled(
            source_over(
                premultiplied(bottom.fill_color, bottom.opacity * isolation.effect.opacity),
                premultiplied(top.fill_color, top.opacity * isolation.effect.opacity),
            ),
            isolation.effect.opacity,
        );
        assert!(
            correct
                .iter()
                .zip(incorrectly_baked_then_grouped)
                .any(|(actual, wrong)| (actual - wrong).abs() > 0.000_001),
            "per-op baking plus CompositeLayer would double-apply and is not the oracle"
        );
    }

    #[test]
    fn mixed_effect_tree_stamp_excludes_child_opacity_but_parent_dependency_tracks_it() {
        fn child_dependency(
            stamp: &super::super::RetainedSurfaceRasterStamp,
        ) -> &super::super::NestedSurfaceRasterDependency {
            let super::super::RetainedSurfaceRasterStepStamp::NestedSurface(dependency) =
                &stamp.ordered_steps[1]
            else {
                panic!("middle step is nested isolation dependency")
            };
            dependency
        }

        let (arena, root, _, child, _, _, mut properties, mut generations) =
            exact_transform_child_isolation_fixture();
        let baseline = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .unwrap();
        let ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let baseline_stamp = super::super::prepare_forced_retained_surface_stamp_for_test(
            &baseline,
            &FrameGraph::new(),
            &ctx,
        )
        .unwrap();

        crate::view::test_support::get_element_mut::<Element>(&arena, child).set_opacity(0.25);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let changed = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .unwrap();
        let changed_stamp = super::super::prepare_forced_retained_surface_stamp_for_test(
            &changed,
            &FrameGraph::new(),
            &ctx,
        )
        .unwrap();
        let baseline_dependency = child_dependency(&baseline_stamp);
        let changed_dependency = child_dependency(&changed_stamp);
        assert_eq!(
            baseline_dependency.child_stamp, changed_dependency.child_stamp,
            "child isolation raster identity excludes its own group opacity"
        );
        assert_ne!(baseline_stamp, changed_stamp);
        let (
            super::super::RetainedSurfaceCompositeGeometryStamp::NestedIsolation {
                source_bounds_bits: baseline_bounds,
                opacity_bits: baseline_opacity,
            },
            super::super::RetainedSurfaceCompositeGeometryStamp::NestedIsolation {
                source_bounds_bits: changed_bounds,
                opacity_bits: changed_opacity,
            },
        ) = (
            &baseline_dependency.child_composite_geometry,
            &changed_dependency.child_composite_geometry,
        )
        else {
            panic!("mixed parent dependency uses the dedicated nested-isolation stamp")
        };
        assert_eq!(baseline_bounds, changed_bounds);
        assert_eq!(*baseline_opacity, 0.5_f32.to_bits());
        assert_eq!(*changed_opacity, 0.25_f32.to_bits());

        let mut tampered_parent = baseline_stamp.clone();
        let super::super::RetainedSurfaceRasterStepStamp::NestedSurface(tampered_dependency) =
            &mut tampered_parent.ordered_steps[1]
        else {
            panic!("mixed dependency")
        };
        tampered_dependency.child_stamp.identity.role =
            super::super::RetainedSurfaceRasterRole::RootIsolation;
        let tampered_child = tampered_dependency.child_stamp.as_ref().clone();
        assert!(!super::super::retained_surface_raster_stamp_is_canonical(
            &tampered_child
        ));
        assert!(!super::super::retained_surface_raster_stamp_is_canonical(
            &tampered_parent
        ));
        let mut viewport = Viewport::new();
        assert!(!viewport.stage_retained_surface_full_set([tampered_parent, tampered_child,]));
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, None)
        );

        let mut tampered_geometry_parent = baseline_stamp.clone();
        let super::super::RetainedSurfaceRasterStepStamp::NestedSurface(
            tampered_geometry_dependency,
        ) = &mut tampered_geometry_parent.ordered_steps[1]
        else {
            panic!("mixed dependency")
        };
        let super::super::RetainedSurfaceCompositeGeometryStamp::NestedIsolation {
            source_bounds_bits,
            ..
        } = &mut tampered_geometry_dependency.child_composite_geometry
        else {
            panic!("nested-isolation geometry")
        };
        source_bounds_bits[0] = (f32::from_bits(source_bounds_bits[0]) + 1.0).to_bits();
        let tampered_geometry_child = tampered_geometry_dependency.child_stamp.as_ref().clone();
        let mut viewport = Viewport::new();
        assert!(
            !viewport.stage_retained_surface_full_set([
                tampered_geometry_parent,
                tampered_geometry_child,
            ])
        );
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, None)
        );

        let mut duplicate_parent = baseline_stamp.clone();
        let mut duplicate_dependency = duplicate_parent.ordered_steps[1].clone();
        let super::super::RetainedSurfaceRasterStepStamp::NestedSurface(dependency) =
            &mut duplicate_dependency
        else {
            panic!("mixed dependency")
        };
        let duplicate_child = dependency.child_stamp.as_ref().clone();
        dependency.step_index = duplicate_parent.ordered_steps.len();
        dependency.parent_opaque_order_before = duplicate_parent.opaque_order_span.end;
        dependency.parent_opaque_order_after = duplicate_parent.opaque_order_span.end;
        duplicate_parent.ordered_steps.push(duplicate_dependency);
        assert!(
            super::super::retained_surface_raster_stamp_is_canonical(&duplicate_parent),
            "duplicate dependency remains a canonical parent stamp in isolation"
        );
        let mut viewport = Viewport::new();
        assert!(
            !viewport.stage_retained_surface_full_set([duplicate_parent, duplicate_child]),
            "one child member referenced twice cannot form a canonical full-set tree"
        );
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, None)
        );
    }

    #[test]
    fn mixed_effect_tree_forced_reuse_matrix_keeps_opacity_composite_only() {
        // Opacity-only: child raster U, parent dependency R.
        {
            let (arena, root, _, child, _, _, mut properties, mut generations) =
                exact_transform_child_isolation_fixture();
            let baseline = plan_single_root_transform_child_isolation_surface(
                &arena,
                &[root],
                &FxHashSet::default(),
                &properties,
                &generations,
            )
            .unwrap();
            let mut viewport = Viewport::new();
            assert_eq!(
                execute_forced_plan_graph(&mut viewport, &baseline)
                    .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                    .len(),
                2
            );
            viewport.finish_retained_surface_transaction(true);

            crate::view::test_support::get_element_mut::<Element>(&arena, child).set_opacity(0.25);
            properties.sync(&arena, &[root]);
            generations.sync(&arena, &[root], &properties);
            let changed = plan_single_root_transform_child_isolation_surface(
                &arena,
                &[root],
                &FxHashSet::default(),
                &properties,
                &generations,
            )
            .unwrap();
            let graph = execute_forced_plan_graph(&mut viewport, &changed);
            assert_eq!(
                graph
                    .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                    .len(),
                1,
                "opacity-only rerasterizes parent dependency, not child raster"
            );
            assert_eq!(graph.test_rect_pass_snapshots().len(), 3);
            let composites = graph.test_graphics_passes::<
                crate::view::render_pass::composite_layer_pass::CompositeLayerPass,
            >();
            assert_eq!(composites.len(), 1);
            assert_eq!(
                composites[0].test_params().opacity.to_bits(),
                0.25_f32.to_bits()
            );
            viewport.finish_retained_surface_transaction(false);
        }

        // Root transform-only: both rasters U, final matrix updates.
        {
            let (arena, root, _, _, _, _, mut properties, mut generations) =
                exact_transform_child_isolation_fixture();
            let baseline = plan_single_root_transform_child_isolation_surface(
                &arena,
                &[root],
                &FxHashSet::default(),
                &properties,
                &generations,
            )
            .unwrap();
            let mut viewport = Viewport::new();
            let baseline_graph = execute_forced_plan_graph(&mut viewport, &baseline);
            let baseline_final = baseline_graph
                .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()[0]
                .test_snapshot();
            viewport.finish_retained_surface_transaction(true);
            crate::view::test_support::get_element_mut::<Element>(&arena, root)
                .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                    glam::Vec3::new(101.0, 0.0, 0.0),
                )));
            properties.sync(&arena, &[root]);
            generations.sync(&arena, &[root], &properties);
            let changed = plan_single_root_transform_child_isolation_surface(
                &arena,
                &[root],
                &FxHashSet::default(),
                &properties,
                &generations,
            )
            .unwrap();
            let graph = execute_forced_plan_graph(&mut viewport, &changed);
            assert!(
                graph
                    .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                    .is_empty()
            );
            assert!(graph.test_rect_pass_snapshots().is_empty());
            assert!(
                graph
                    .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
                    .is_empty()
            );
            let finals =
                graph.test_graphics_passes::<crate::view::render_pass::TextureCompositePass>();
            assert_eq!(finals.len(), 1);
            assert_ne!(finals[0].test_snapshot(), baseline_final);
            viewport.finish_retained_surface_transaction(false);
        }

        // Child paint: child R and parent dependency R.
        {
            let (arena, root, _, child, _, _, mut properties, mut generations) =
                exact_transform_child_isolation_fixture();
            let baseline = plan_single_root_transform_child_isolation_surface(
                &arena,
                &[root],
                &FxHashSet::default(),
                &properties,
                &generations,
            )
            .unwrap();
            let mut viewport = Viewport::new();
            execute_forced_plan_graph(&mut viewport, &baseline);
            viewport.finish_retained_surface_transaction(true);
            crate::view::test_support::get_element_mut::<Element>(&arena, child)
                .set_background_color_value(Color::rgb(12, 220, 44));
            properties.sync(&arena, &[root]);
            generations.sync(&arena, &[root], &properties);
            let changed = plan_single_root_transform_child_isolation_surface(
                &arena,
                &[root],
                &FxHashSet::default(),
                &properties,
                &generations,
            )
            .unwrap();
            let graph = execute_forced_plan_graph(&mut viewport, &changed);
            assert_eq!(
                graph
                    .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                    .len(),
                2
            );
            assert_eq!(graph.test_rect_pass_snapshots().len(), 5);
            viewport.finish_retained_surface_transaction(false);
        }

        // Parent-only paint: parent R, child U.
        {
            let (arena, root, _, _, _, _, mut properties, mut generations) =
                exact_transform_child_isolation_fixture();
            let baseline = plan_single_root_transform_child_isolation_surface(
                &arena,
                &[root],
                &FxHashSet::default(),
                &properties,
                &generations,
            )
            .unwrap();
            let mut viewport = Viewport::new();
            execute_forced_plan_graph(&mut viewport, &baseline);
            viewport.finish_retained_surface_transaction(true);
            crate::view::test_support::get_element_mut::<Element>(&arena, root)
                .set_background_color_value(Color::rgb(220, 12, 44));
            properties.sync(&arena, &[root]);
            generations.sync(&arena, &[root], &properties);
            let changed = plan_single_root_transform_child_isolation_surface(
                &arena,
                &[root],
                &FxHashSet::default(),
                &properties,
                &generations,
            )
            .unwrap();
            let graph = execute_forced_plan_graph(&mut viewport, &changed);
            assert_eq!(
                graph
                    .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                    .len(),
                1
            );
            assert_eq!(graph.test_rect_pass_snapshots().len(), 3);
            assert_eq!(
                graph
                    .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
                    .len(),
                1
            );
            viewport.finish_retained_surface_transaction(false);
        }
    }

    #[test]
    fn mixed_root_isolation_and_transform_tree_executors_reject_cross_shape_atomically() {
        let (arena, root, _, _, _, _, properties, generations) =
            exact_transform_child_isolation_fixture();
        let mixed = plan_single_root_transform_child_isolation_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .unwrap();
        let (arena, transform_root, _, _, _, _, properties, generations) =
            nested_exact_transform_fixture();
        let transform_tree = plan_single_root_transform_surface(
            &arena,
            &[transform_root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .unwrap();
        let (arena, isolation_root, properties, generations) = exact_isolation_fixture(0.5);
        let root_isolation = plan_single_root_isolation_surface(
            &arena,
            &[isolation_root],
            &FxHashSet::default(),
            &properties,
            &generations,
            160,
            120,
            1.0,
            None,
        )
        .unwrap();
        let mut viewport = Viewport::new();

        let mut graph = FrameGraph::new();
        let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
        let graph_before = graph.build_state_snapshot_for_test();
        let transaction_before = viewport.retained_surface_transaction_shape_for_test();
        let error = match super::super::build_retained_surface_tree_from_pool(
            &mut viewport,
            &mixed,
            &mut graph,
            ctx,
        ) {
            Ok(_) => panic!("T->T executor cannot accept mixed effect tree"),
            Err(error) => error,
        };
        assert_eq!(error, super::super::ForcedTransformSurfaceError::PlanShape);
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            transaction_before
        );

        let mut graph = FrameGraph::new();
        let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
        let graph_before = graph.build_state_snapshot_for_test();
        let error = match super::super::build_retained_isolation_surface_from_pool(
            &mut viewport,
            &mixed,
            &mut graph,
            ctx,
        ) {
            Ok(_) => panic!("root isolation executor cannot accept mixed effect tree"),
            Err(error) => error,
        };
        assert_eq!(error, super::super::ForcedTransformSurfaceError::PlanShape);
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            transaction_before
        );

        for (label, plan) in [
            ("T->T", &transform_tree),
            ("root isolation", &root_isolation),
        ] {
            let mut graph = FrameGraph::new();
            let ctx = parent_context_without_clear(&mut graph, 160, 120, 1.0);
            let graph_before = graph.build_state_snapshot_for_test();
            let error = match super::super::build_retained_effect_tree_from_pool(
                &mut viewport,
                plan,
                &mut graph,
                ctx,
            ) {
                Ok(_) => panic!("mixed executor cannot accept {label}"),
                Err(error) => error,
            };
            assert_eq!(error, super::super::ForcedTransformSurfaceError::PlanShape);
            assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
            assert_eq!(
                viewport.retained_surface_transaction_shape_for_test(),
                transaction_before
            );
        }
    }

    #[test]
    fn nested_exact_transform_builds_ordered_owning_stream_and_absolute_matrix_golden() {
        let (mut arena, root, before, child, descendant, after, properties, generations) =
            nested_exact_transform_fixture();
        let plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("one direct transformed Element child must produce a nested owning plan");

        let [PaintPlanStep::RetainedSurface(parent)] = plan.steps.as_slice() else {
            panic!("one top-level retained surface")
        };
        let [
            PaintPlanStep::ArtifactSpan(before_span),
            PaintPlanStep::RetainedSurface(nested),
            PaintPlanStep::ArtifactSpan(after_span),
        ] = parent.raster_steps.as_slice()
        else {
            panic!("DFS order must be parent-before, child surface, parent-after")
        };
        let [PaintPlanStep::ArtifactSpan(child_span)] = nested.raster_steps.as_slice() else {
            panic!("C5A1 nested surface owns one complete child artifact")
        };

        assert_eq!(parent.boundary_root, root);
        assert_eq!(parent.parent_surface, None);
        assert_eq!(nested.boundary_root, child);
        assert_eq!(nested.parent_surface, Some(root));
        assert_eq!(nested.transform(), TransformNodeId(child));

        let chunk_owners = |span: &ArtifactSpanPlan| {
            span.artifact
                .chunks
                .iter()
                .map(|chunk| chunk.owner)
                .collect::<Vec<_>>()
        };
        assert_eq!(chunk_owners(before_span), vec![root, before]);
        assert_eq!(chunk_owners(child_span), vec![child, descendant]);
        assert_eq!(chunk_owners(after_span), vec![after]);

        let before_count = opaque_order_count(&before_span.artifact);
        let child_count = opaque_order_count(&child_span.artifact);
        let after_count = opaque_order_count(&after_span.artifact);
        assert_eq!((before_count, child_count, after_count), (2, 2, 1));
        assert_eq!(before_span.opaque_order_span, 0..2);
        assert_eq!(child_span.opaque_order_span, 0..2);
        assert_eq!(after_span.opaque_order_span, 2..3);
        assert_eq!(nested.aggregate_opaque_order_span, 0..2);
        assert_eq!(parent.aggregate_opaque_order_span, 0..3);

        let expected_child_matrix = properties.transforms[&TransformNodeId(child)].viewport_matrix;
        assert_eq!(
            nested
                .geometry()
                .viewport_transform
                .to_cols_array()
                .map(f32::to_bits),
            expected_child_matrix.to_cols_array().map(f32::to_bits),
            "child surface stores canonical absolute C; parent edge is topology only"
        );
        assert_eq!(
            parent
                .geometry()
                .viewport_transform
                .to_cols_array()
                .map(f32::to_bits),
            properties.transforms[&TransformNodeId(root)]
                .viewport_matrix
                .to_cols_array()
                .map(f32::to_bits)
        );
        let mapped = parent.geometry().viewport_transform
            * nested.geometry().viewport_transform
            * glam::Vec4::new(2.0, 3.0, 0.0, 1.0);
        assert_eq!(
            [mapped.x.to_bits(), mapped.y.to_bits(), mapped.w.to_bits()],
            [127.0_f32.to_bits(), 2.0_f32.to_bits(), 1.0_f32.to_bits()],
            "stored absolute matrices must compose as P*C with no inverse-derived child matrix"
        );
        let mut forced_graph = FrameGraph::new();
        let mut forced_ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let forced_outer_target = forced_ctx.allocate_target(&mut forced_graph);
        forced_ctx.set_current_target(forced_outer_target);
        let mut forced_viewport = Viewport::new();
        let forced_state = super::super::execute_forced_transform_surface_for_test(
            &mut forced_viewport,
            &plan,
            &mut forced_graph,
            forced_ctx,
        )
        .expect("C5B0 forced nested R/R execution");
        assert_eq!(forced_state.opaque_rect_order_for_test(), 3);

        let mut legacy_graph = FrameGraph::new();
        let mut legacy_ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let outer_target = legacy_ctx.allocate_target(&mut legacy_graph);
        legacy_ctx.set_current_target(outer_target);
        let legacy_state = arena
            .with_element_taken(root, |element, arena| {
                element.build(&mut legacy_graph, arena, legacy_ctx)
            })
            .expect("legacy nested transform build");
        let clears = legacy_graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .into_iter()
            .map(|clear| clear.test_snapshot())
            .collect::<Vec<_>>();
        let [parent_clear, child_clear] = clears.as_slice() else {
            panic!("legacy nested graph must own one target per transform surface")
        };
        let rects = legacy_graph.test_rect_pass_snapshots();
        assert_eq!(rects.len(), 5);
        assert_eq!(
            rects
                .iter()
                .map(|rect| (rect.output_target, rect.opaque_depth_order))
                .collect::<Vec<_>>(),
            vec![
                (parent_clear.output_target, Some(0)),
                (parent_clear.output_target, Some(1)),
                (child_clear.output_target, Some(0)),
                (child_clear.output_target, Some(1)),
                (parent_clear.output_target, Some(2)),
            ],
            "each surface starts at zero; child terminal is merged into the parent by max before the after span"
        );
        assert_eq!(
            legacy_state.opaque_rect_order_for_test(),
            3,
            "parent terminal is max(parent-before=2, child=2)+parent-after=1, not a global sum of five"
        );

        let surface_pass_names = |graph: &FrameGraph| {
            let clear = std::any::type_name::<crate::view::frame_graph::ClearPass>();
            let composite = std::any::type_name::<crate::view::render_pass::TextureCompositePass>();
            graph
                .pass_descriptors()
                .into_iter()
                .filter_map(|descriptor| {
                    (descriptor.name == clear)
                        .then_some("clear")
                        .or_else(|| (descriptor.name == composite).then_some("composite"))
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            surface_pass_names(&forced_graph),
            ["clear", "clear", "composite", "composite"]
        );
        assert_eq!(
            surface_pass_names(&forced_graph),
            surface_pass_names(&legacy_graph)
        );
        assert_eq!(
            forced_graph.test_rect_pass_snapshots(),
            legacy_graph.test_rect_pass_snapshots(),
            "forced nested artifact payload, targets, and depth orders must match legacy"
        );
        assert_eq!(
            forced_graph
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .into_iter()
                .map(|pass| pass.test_snapshot())
                .collect::<Vec<_>>(),
            legacy_graph
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .into_iter()
                .map(|pass| pass.test_snapshot())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            forced_graph
                .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
                .into_iter()
                .map(|pass| pass.test_snapshot())
                .collect::<Vec<_>>(),
            legacy_graph
                .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
                .into_iter()
                .map(|pass| pass.test_snapshot())
                .collect::<Vec<_>>(),
            "child P*C composite and parent composite payload must stay bit-identical"
        );
        assert_eq!(
            forced_graph
                .declared_persistent_textures()
                .collect::<Vec<_>>(),
            legacy_graph
                .declared_persistent_textures()
                .collect::<Vec<_>>()
        );
        assert_eq!(
            forced_viewport.retained_surface_transaction_shape_for_test(),
            (0, Some(2)),
            "the complete nested stamp set is staged exactly once"
        );
        forced_viewport.finish_retained_surface_transaction(false);
        assert_eq!(
            forced_viewport.retained_surface_transaction_shape_for_test(),
            (0, None),
            "a failed frame must clear the complete pending nested stamp set"
        );
    }

    #[test]
    fn production_isolation_first_frame_matches_canonical_root_group_oracle() {
        let (arena, root, properties, generations) = exact_isolation_fixture(0.5);
        let plan = plan_single_root_isolation_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            160,
            120,
            1.0,
            None,
        )
        .expect("exact root opacity isolation plan");
        let surface = only_surface(&plan);
        let SurfaceKind::Isolation(isolation) = surface.kind() else {
            panic!("M9F3 plan must carry typed isolation payload");
        };
        assert_eq!(isolation.effect.id.0, root);
        assert_eq!(isolation.effect.opacity.to_bits(), 0.5_f32.to_bits());
        assert_eq!(
            surface.persistent_color_key(),
            crate::view::base_component::isolation_layer_stable_key(0x9f_3001)
        );
        assert!(matches!(
            only_span(surface).artifact.target,
            crate::view::paint::PaintArtifactTarget::RootOpacityGroup { root: target, effect }
                if target == root && effect.0 == root
        ));

        let mut production_graph = FrameGraph::new();
        let (production_ctx, _) = parent_context_with_clear(&mut production_graph, 160, 120, 1.0);
        let mut viewport = Viewport::new();
        let outcome = super::super::build_retained_isolation_surface_from_pool(
            &mut viewport,
            &plan,
            &mut production_graph,
            production_ctx,
        )
        .expect("production isolation build");
        let (state, trace) = outcome.into_parts();
        assert_eq!(
            trace.action,
            super::super::RetainedSurfaceCompileAction::Reraster
        );
        assert_eq!(trace.descriptor_size, [160, 120]);
        assert_eq!(state.opaque_rect_order_for_test(), 2);

        let mut oracle_graph = FrameGraph::new();
        let (oracle_ctx, _) = parent_context_with_clear(&mut oracle_graph, 160, 120, 1.0);
        let oracle_state = match super::super::try_compile_root_effect_artifact(
            &only_span(surface).artifact,
            super::super::RootEffectCompileAction::Reraster,
            &mut oracle_graph,
            oracle_ctx,
        ) {
            Ok(state) => state,
            Err(_) => panic!("canonical root group oracle compiles"),
        };
        assert_eq!(oracle_state.opaque_rect_order_for_test(), 2);
        assert_eq!(
            production_graph.test_rect_pass_snapshots(),
            oracle_graph.test_rect_pass_snapshots(),
            "isolation raster payload and local opaque order reuse the canonical recorder/compiler"
        );
        assert_eq!(
            production_graph
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .into_iter()
                .map(|pass| pass.test_snapshot())
                .collect::<Vec<_>>(),
            oracle_graph
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .into_iter()
                .map(|pass| pass.test_snapshot())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            production_graph
                .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
                .into_iter()
                .map(|pass| pass.test_snapshot())
                .collect::<Vec<_>>(),
            oracle_graph
                .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
                .into_iter()
                .map(|pass| pass.test_snapshot())
                .collect::<Vec<_>>(),
            "full-viewport CompositeLayer opacity authority remains bit-exact"
        );
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, Some(1))
        );
        viewport.finish_retained_surface_transaction(true);
        let mut second_graph = FrameGraph::new();
        let (second_ctx, _) = parent_context_with_clear(&mut second_graph, 160, 120, 1.0);
        let second = super::super::build_retained_isolation_surface_from_pool(
            &mut viewport,
            &plan,
            &mut second_graph,
            second_ctx,
        )
        .expect("second production isolation build");
        let (_, second_trace) = second.into_parts();
        assert_eq!(
            second_trace.action,
            super::super::RetainedSurfaceCompileAction::Reraster,
            "production isolation cannot consume the test-only pair witness"
        );
        viewport.finish_retained_surface_transaction(false);
    }

    #[test]
    fn isolation_opacity_is_composite_only_but_future_parent_dependency_tracks_it() {
        let (arena, root, mut properties, mut generations) = exact_isolation_fixture(0.5);
        let baseline = plan_single_root_isolation_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            160,
            120,
            1.0,
            None,
        )
        .expect("baseline isolation");
        let graph = FrameGraph::new();
        let ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let baseline_stamp =
            super::super::prepare_forced_retained_surface_stamp_for_test(&baseline, &graph, &ctx)
                .expect("baseline stamp");
        let SurfaceKind::Isolation(baseline_isolation) = only_surface(&baseline).kind() else {
            panic!("isolation");
        };

        crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.25);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let changed = plan_single_root_isolation_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            160,
            120,
            1.0,
            None,
        )
        .expect("opacity-only isolation");
        let changed_stamp =
            super::super::prepare_forced_retained_surface_stamp_for_test(&changed, &graph, &ctx)
                .expect("changed stamp");
        assert_eq!(
            baseline_stamp, changed_stamp,
            "own isolation opacity is excluded from raster identity"
        );
        let SurfaceKind::Isolation(changed_isolation) = only_surface(&changed).kind() else {
            panic!("isolation");
        };
        let baseline_dependency = super::super::retained_isolation_composite_geometry_stamp(
            baseline_isolation.geometry.source_bounds,
            baseline_isolation.geometry.logical_size,
            baseline_isolation.effect.opacity,
            None,
        )
        .unwrap();
        let changed_dependency = super::super::retained_isolation_composite_geometry_stamp(
            changed_isolation.geometry.source_bounds,
            changed_isolation.geometry.logical_size,
            changed_isolation.effect.opacity,
            None,
        )
        .unwrap();
        assert_ne!(
            baseline_dependency, changed_dependency,
            "future parent raster dependency must include child isolation opacity"
        );

        let mut viewport = Viewport::new();
        let mut first_graph = FrameGraph::new();
        let (first_ctx, _) = parent_context_with_clear(&mut first_graph, 160, 120, 1.0);
        super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &baseline,
            &mut first_graph,
            first_ctx,
        )
        .expect("forced baseline isolation");
        viewport.finish_retained_surface_transaction(true);
        let mut changed_graph = FrameGraph::new();
        let (changed_ctx, _) = parent_context_with_clear(&mut changed_graph, 160, 120, 1.0);
        super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &changed,
            &mut changed_graph,
            changed_ctx,
        )
        .expect("forced opacity-only isolation reuse");
        assert_eq!(
            changed_graph
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .len(),
            1,
            "only the legal outer producer remains on isolation reuse"
        );
        assert!(changed_graph.test_rect_pass_snapshots().is_empty());
        let composites = changed_graph.test_graphics_passes::<
            crate::view::render_pass::composite_layer_pass::CompositeLayerPass,
        >();
        assert_eq!(composites.len(), 1);
        assert_eq!(
            composites[0].test_params().opacity.to_bits(),
            0.25_f32.to_bits()
        );
        viewport.finish_retained_surface_transaction(false);
    }

    #[test]
    fn isolation_planner_and_executor_reject_unsupported_or_tampered_state_atomically() {
        let (arena, root, properties, generations) = exact_isolation_fixture(0.5);
        let promoted = FxHashSet::from_iter([0x9f_3001]);
        let promoted_error = plan_single_root_isolation_surface(
            &arena,
            &[root],
            &promoted,
            &properties,
            &generations,
            160,
            120,
            1.0,
            None,
        )
        .expect_err("promotion cannot mix with typed isolation");
        assert!(
            promoted_error
                .reasons
                .contains(&FramePaintPlanRejection::InvalidIsolationEffect(root))
        );
        let scissor_error = plan_single_root_isolation_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            160,
            120,
            1.0,
            Some([1, 2, 3, 4]),
        )
        .expect_err("first isolation slice rejects outer scissor");
        assert!(
            scissor_error
                .reasons
                .contains(&FramePaintPlanRejection::IsolationOuterScissor)
        );

        let mut plan = plan_single_root_isolation_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            160,
            120,
            1.0,
            None,
        )
        .expect("baseline isolation");
        isolation_plan_mut(&mut plan).effect.opacity = f32::NAN;
        let mut graph = FrameGraph::new();
        let (ctx, _) = parent_context_with_clear(&mut graph, 160, 120, 1.0);
        let graph_before = graph.build_state_snapshot_for_test();
        let mut viewport = Viewport::new();
        let transaction_before = viewport.retained_surface_transaction_shape_for_test();
        let error = match super::super::build_retained_isolation_surface_from_pool(
            &mut viewport,
            &plan,
            &mut graph,
            ctx,
        ) {
            Ok(_) => panic!("tampered effect cannot emit"),
            Err(error) => error,
        };
        assert_eq!(
            error,
            super::super::ForcedTransformSurfaceError::GeometryContract
        );
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            transaction_before
        );
    }

    #[test]
    fn production_tree_canary_first_frame_matches_legacy_and_uses_pool_only_actions() {
        let (mut arena, root, _before, child, _descendant, _after, properties, generations) =
            nested_exact_transform_fixture();
        let outer_scissor = Some([3, 4, 50, 60]);
        let plan = plan_single_root_transform_surface_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], outer_scissor),
        )
        .expect("exact depth-two production plan");

        let mut production_graph = FrameGraph::new();
        let (mut production_ctx, _) =
            parent_context_with_clear(&mut production_graph, 160, 120, 1.0);
        production_ctx.push_scissor_rect(outer_scissor);
        let mut viewport = Viewport::new();
        let outcome = super::super::build_retained_surface_tree_from_pool(
            &mut viewport,
            &plan,
            &mut production_graph,
            production_ctx,
        )
        .expect("production tree executor accepts the exact planner shape");
        let (production_state, traces) = outcome.into_parts();
        assert_eq!(production_state.opaque_rect_order_for_test(), 3);
        assert_eq!(traces.len(), 2);
        assert_eq!(traces[0].boundary_root, root);
        assert_eq!(traces[1].boundary_root, child);
        assert!(traces.iter().all(|trace| {
            trace.action == super::super::RetainedSurfaceCompileAction::Reraster
                && trace.descriptor_size[0] > 0
                && trace.descriptor_size[1] > 0
                && trace.chunk_count > 0
                && trace.op_count > 0
        }));

        let mut legacy_graph = FrameGraph::new();
        let (mut legacy_ctx, _) = parent_context_with_clear(&mut legacy_graph, 160, 120, 1.0);
        legacy_ctx.push_scissor_rect(outer_scissor);
        let legacy_state = arena
            .with_element_taken(root, |element, arena| {
                element.build(&mut legacy_graph, arena, legacy_ctx)
            })
            .expect("legacy nested transform build");
        assert_eq!(legacy_state.opaque_rect_order_for_test(), 3);
        assert_eq!(
            production_graph.test_rect_pass_snapshots(),
            legacy_graph.test_rect_pass_snapshots()
        );
        assert_eq!(
            production_graph
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .into_iter()
                .map(|pass| pass.test_snapshot())
                .collect::<Vec<_>>(),
            legacy_graph
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .into_iter()
                .map(|pass| pass.test_snapshot())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            production_graph
                .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
                .into_iter()
                .map(|pass| pass.test_snapshot())
                .collect::<Vec<_>>(),
            legacy_graph
                .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
                .into_iter()
                .map(|pass| pass.test_snapshot())
                .collect::<Vec<_>>(),
            "production tree keeps P*C, target nesting, and root-only outer scissor parity"
        );
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, Some(2))
        );
        viewport.finish_retained_surface_transaction(true);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (2, None)
        );

        let mut second_graph = FrameGraph::new();
        let (mut second_ctx, _) = parent_context_with_clear(&mut second_graph, 160, 120, 1.0);
        second_ctx.push_scissor_rect(outer_scissor);
        let second = super::super::build_retained_surface_tree_from_pool(
            &mut viewport,
            &plan,
            &mut second_graph,
            second_ctx,
        )
        .expect("second production tree frame");
        let (_, second_traces) = second.into_parts();
        assert!(
            second_traces.iter().all(|trace| {
                trace.action == super::super::RetainedSurfaceCompileAction::Reraster
            }),
            "logical success alone cannot fabricate real GPU-pool residency"
        );
        viewport.finish_retained_surface_transaction(false);
    }

    #[test]
    fn production_singleton_and_tree_executors_reject_each_others_shape_before_mutation() {
        let (arena, root, _before, _child, _descendant, _after, properties, generations) =
            nested_exact_transform_fixture();
        let nested = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("nested plan");
        let mut graph = FrameGraph::new();
        let (ctx, _) = parent_context_with_clear(&mut graph, 160, 120, 1.0);
        let graph_before = graph.build_state_snapshot_for_test();
        let mut viewport = Viewport::new();
        let transaction_before = viewport.retained_surface_transaction_shape_for_test();
        let error = match super::super::build_retained_surface_from_pool(
            &mut viewport,
            &nested,
            &mut graph,
            ctx,
        ) {
            Ok(_) => panic!("singleton production executor cannot accept nested input"),
            Err(error) => error,
        };
        assert_eq!(
            error,
            super::super::ForcedTransformSurfaceError::NestedSurface
        );
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            transaction_before
        );

        let (arena, root, properties, generations) = exact_transform_fixture();
        let singleton = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("singleton plan");
        let mut graph = FrameGraph::new();
        let (ctx, _) = parent_context_with_clear(&mut graph, 160, 120, 1.0);
        let graph_before = graph.build_state_snapshot_for_test();
        let error = match super::super::build_retained_surface_tree_from_pool(
            &mut viewport,
            &singleton,
            &mut graph,
            ctx,
        ) {
            Ok(_) => panic!("tree production executor requires exact depth two"),
            Err(error) => error,
        };
        assert_eq!(error, super::super::ForcedTransformSurfaceError::PlanShape);
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            transaction_before
        );
    }

    #[test]
    fn forced_nested_outer_scissor_applies_only_to_parent_final_composite() {
        let (arena, root, _before, _child, _descendant, _after, properties, generations) =
            nested_exact_transform_fixture();
        let outer_scissor = Some([3, 4, 50, 60]);
        let plan = plan_single_root_transform_surface_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], outer_scissor),
        )
        .expect("nested retained surface with an outer scissor");
        let [PaintPlanStep::RetainedSurface(parent)] = plan.steps.as_slice() else {
            panic!("one parent surface")
        };
        let [
            PaintPlanStep::ArtifactSpan(_),
            PaintPlanStep::RetainedSurface(child),
            PaintPlanStep::ArtifactSpan(_),
        ] = parent.raster_steps.as_slice()
        else {
            panic!("parent-before, child surface, parent-after")
        };
        assert_eq!(parent.geometry().outer_scissor_rect, outer_scissor);
        assert_eq!(
            child.geometry().outer_scissor_rect,
            None,
            "the child raster/composite stays surface-local and cannot inherit the frame scissor"
        );

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let outer_target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(outer_target);
        ctx.push_scissor_rect(outer_scissor);
        let mut viewport = Viewport::new();
        let state = super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &plan,
            &mut graph,
            ctx,
        )
        .expect("forced nested R/R execution with outer scissor");
        assert_eq!(state.opaque_rect_order_for_test(), 3);
        assert!(graph.test_rect_pass_snapshots().iter().all(|snapshot| {
            snapshot.explicit_scissor_rect.is_none() && snapshot.effective_scissor_rect.is_none()
        }));

        let composites = graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .into_iter()
            .map(|pass| pass.test_snapshot())
            .collect::<Vec<_>>();
        let [child_composite, parent_composite] = composites.as_slice() else {
            panic!("child and parent final composites")
        };
        assert_eq!(child_composite.explicit_scissor_rect, None);
        assert_eq!(child_composite.effective_scissor_rect, None);
        assert_eq!(parent_composite.explicit_scissor_rect, outer_scissor);
        assert_eq!(parent_composite.effective_scissor_rect, outer_scissor);
    }

    #[test]
    fn forced_nested_child_transform_only_freezes_parent_reraster_child_reuse() {
        let (arena, root, _before, child, _descendant, _after, mut properties, mut generations) =
            nested_exact_transform_fixture();
        let baseline = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("baseline nested plan");
        let mut viewport = Viewport::new();
        let mut first_graph = FrameGraph::new();
        let mut first_ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let first_outer = first_ctx.allocate_target(&mut first_graph);
        first_ctx.set_current_target(first_outer);
        super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &baseline,
            &mut first_graph,
            first_ctx,
        )
        .expect("baseline nested R/R");
        viewport.finish_retained_surface_transaction(true);

        let child_matrix = glam::Mat4::from_cols_array(&[
            0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 31.0, 0.0, 0.0, 1.0,
        ]);
        crate::view::test_support::get_element_mut::<Element>(&arena, child)
            .set_resolved_transform_for_test(Some(child_matrix));
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let child_transform_only = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("child transform-only nested plan");

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let outer = ctx.allocate_target(&mut graph);
        ctx.set_current_target(outer);
        let state = super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &child_transform_only,
            &mut graph,
            ctx,
        )
        .expect("parent R / child U execution");
        assert_eq!(state.opaque_rect_order_for_test(), 3);

        let clears = graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .into_iter()
            .map(|pass| pass.test_snapshot())
            .collect::<Vec<_>>();
        let [parent_clear] = clears.as_slice() else {
            panic!("R/U clears only the parent pair")
        };
        let rects = graph.test_rect_pass_snapshots();
        assert_eq!(
            rects.len(),
            3,
            "the child resident pair emits no raster work"
        );
        assert!(rects.iter().all(|rect| {
            rect.output_target == parent_clear.output_target
                && rect.input_target == parent_clear.output_target
        }));
        assert_eq!(
            rects
                .iter()
                .map(|rect| rect.opaque_depth_order)
                .collect::<Vec<_>>(),
            [Some(0), Some(1), Some(2)],
            "child terminal is replayed by max before the parent after span"
        );

        let composites = graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .into_iter()
            .map(|pass| pass.test_snapshot())
            .collect::<Vec<_>>();
        let [child_composite, parent_composite] = composites.as_slice() else {
            panic!("R/U emits child-to-parent and parent-to-caller composites")
        };
        assert_eq!(child_composite.output_target, parent_clear.output_target);
        assert_ne!(child_composite.source_handle, parent_clear.output_target);
        assert_eq!(parent_composite.source_handle, parent_clear.output_target);
        assert_eq!(parent_composite.output_target, outer.handle());
        assert_eq!(
            graph.declared_persistent_texture_keys().count(),
            4,
            "R/U declares both complete persistent target pairs"
        );
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (2, Some(2)),
            "the frozen R/U action tree still stages the canonical full set"
        );
    }

    #[test]
    fn forced_nested_parent_transform_only_reuses_whole_tree_without_child_composite() {
        let (arena, root, _before, _child, _descendant, _after, mut properties, mut generations) =
            nested_exact_transform_fixture();
        let baseline = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("baseline nested plan");
        let mut viewport = Viewport::new();
        commit_forced_nested_plan(&mut viewport, &baseline);

        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                101.0, 0.0, 0.0,
            ))));
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let parent_transform_only = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("parent transform-only nested plan");

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let outer = ctx.allocate_target(&mut graph);
        ctx.set_current_target(outer);
        let state = super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &parent_transform_only,
            &mut graph,
            ctx,
        )
        .expect("parent U / child U execution");
        assert_eq!(state.opaque_rect_order_for_test(), 3);
        assert_eq!(state.target_pair_count_for_test(), 3);
        assert!(
            graph
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .is_empty()
        );
        assert!(graph.test_rect_pass_snapshots().is_empty());
        let composites = graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .into_iter()
            .map(|pass| pass.test_snapshot())
            .collect::<Vec<_>>();
        let [parent_composite] = composites.as_slice() else {
            panic!("U/U emits only the latest parent-to-caller composite")
        };
        assert_eq!(parent_composite.output_target, outer.handle());
        let parent = only_surface(&parent_transform_only);
        assert_eq!(
            parent_composite.quad_position_bits,
            Some(
                parent
                    .geometry()
                    .quad_positions
                    .map(|point| point.map(f32::to_bits)),
            ),
            "U/U final composite must use the latest parent transform"
        );
        assert_eq!(graph.declared_persistent_texture_keys().count(), 4);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (2, Some(2))
        );
        viewport.finish_retained_surface_transaction(false);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (0, None),
            "a failed U/U frame invalidates both logical stamps and pair witnesses"
        );
        let mut retry_graph = FrameGraph::new();
        let mut retry_ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let retry_outer = retry_ctx.allocate_target(&mut retry_graph);
        retry_ctx.set_current_target(retry_outer);
        super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &parent_transform_only,
            &mut retry_graph,
            retry_ctx,
        )
        .expect("retry after failed U/U frame");
        assert_eq!(
            retry_graph
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .len(),
            2
        );
        assert_eq!(retry_graph.test_rect_pass_snapshots().len(), 5);
        assert_eq!(
            retry_graph
                .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
                .len(),
            2,
            "finish(false) forces the next nested frame back to R/R"
        );
    }

    #[test]
    fn forced_nested_child_pool_miss_reraster_materializes_without_touching_cached_parent() {
        let (arena, root, _before, child, _descendant, _after, properties, generations) =
            nested_exact_transform_fixture();
        let plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("baseline nested plan");
        let mut viewport = Viewport::new();
        commit_forced_nested_plan(&mut viewport, &plan);
        let child_key = crate::view::base_component::transformed_layer_stable_key(
            arena.get(child).expect("child").element.stable_id(),
        );
        viewport.forget_retained_surface_pair_witness_for_test(child_key);

        let mut graph = FrameGraph::new();
        let (ctx, outer) = parent_context_with_clear(&mut graph, 160, 120, 1.0);
        let state = super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &plan,
            &mut graph,
            ctx,
        )
        .expect("parent U / child R_pool execution");
        assert_eq!(state.opaque_rect_order_for_test(), 3);
        assert_eq!(state.target_pair_count_for_test(), 3);
        let clears = graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .into_iter()
            .map(|pass| pass.test_snapshot())
            .collect::<Vec<_>>();
        let [_outer_clear, child_clear] = clears.as_slice() else {
            panic!("U/R_pool has one legal outer producer and clears only the missing child pair")
        };
        let rects = graph.test_rect_pass_snapshots();
        assert_eq!(rects.len(), 2);
        assert!(rects.iter().all(|rect| {
            rect.input_target == child_clear.output_target
                && rect.output_target == child_clear.output_target
        }));
        let composites = graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .into_iter()
            .map(|pass| pass.test_snapshot())
            .collect::<Vec<_>>();
        let [parent_composite] = composites.as_slice() else {
            panic!("U/R_pool cannot composite the child into the cached parent")
        };
        assert_ne!(parent_composite.source_handle, child_clear.output_target);
        assert_eq!(parent_composite.output_target, outer.handle());
        assert_eq!(graph.declared_persistent_texture_keys().count(), 4);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            (2, Some(2))
        );

        let present = graph.add_graphics_pass(
            crate::view::render_pass::present_surface_pass::PresentSurfacePass::new(
                crate::view::render_pass::present_surface_pass::PresentSurfaceParams,
                crate::view::render_pass::present_surface_pass::PresentSurfaceInput {
                    source: crate::view::render_pass::draw_rect_pass::RenderTargetIn::with_handle(
                        outer.handle().expect("outer target handle"),
                    ),
                },
                crate::view::render_pass::present_surface_pass::PresentSurfaceOutput,
            ),
        );
        graph
            .add_pass_sink(
                present,
                crate::view::frame_graph::ExternalSinkKind::SurfacePresent,
            )
            .expect("surface-present root");

        let compiled = graph
            .test_compile_snapshot()
            .expect("surface-present and persistent-materialization roots must compile together");
        assert_eq!(
            compiled
                .pass_payloads()
                .iter()
                .filter(|payload| matches!(payload, FramePassTestPayload::Clear(_)))
                .count(),
            2,
            "the legal outer depth producer and detached child clear both stay live"
        );
        assert_eq!(
            compiled
                .pass_payloads()
                .iter()
                .filter(|payload| matches!(payload, FramePassTestPayload::DrawRect(_)))
                .count(),
            2,
            "the persistent materialization sink keeps child raster writes live"
        );
        assert_eq!(
            compiled
                .pass_payloads()
                .iter()
                .filter(|payload| matches!(payload, FramePassTestPayload::TextureComposite(_)))
                .count(),
            1,
            "only parent-to-caller composite remains; child-to-cached-parent is forbidden"
        );
        assert_eq!(
            compiled
                .pass_payloads()
                .iter()
                .filter(|payload| matches!(payload, FramePassTestPayload::PresentSurface(_)))
                .count(),
            1,
            "the parent final-composite and present chain stays live beside materialization"
        );
    }

    #[test]
    fn forced_nested_parent_and_child_paint_changes_freeze_r_u_and_r_r() {
        let (arena, root, _before, _child, _descendant, _after, mut properties, mut generations) =
            nested_exact_transform_fixture();
        let baseline = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("parent-paint baseline");
        let mut viewport = Viewport::new();
        commit_forced_nested_plan(&mut viewport, &baseline);
        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_background_color_value(Color::rgb(90, 20, 140));
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let parent_paint = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("parent own-paint plan");
        let mut parent_graph = FrameGraph::new();
        let mut parent_ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let parent_outer = parent_ctx.allocate_target(&mut parent_graph);
        parent_ctx.set_current_target(parent_outer);
        let parent_state = super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &parent_paint,
            &mut parent_graph,
            parent_ctx,
        )
        .expect("parent paint R/U");
        assert_eq!(parent_state.opaque_rect_order_for_test(), 3);
        assert_eq!(parent_state.target_pair_count_for_test(), 3);
        assert_eq!(
            parent_graph
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .len(),
            1
        );
        assert_eq!(parent_graph.test_rect_pass_snapshots().len(), 3);
        assert_eq!(
            parent_graph
                .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
                .len(),
            2
        );
        assert_eq!(parent_graph.declared_persistent_texture_keys().count(), 4);

        let (arena, root, _before, child, _descendant, _after, mut properties, mut generations) =
            nested_exact_transform_fixture();
        let baseline = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("child-paint baseline");
        let mut viewport = Viewport::new();
        commit_forced_nested_plan(&mut viewport, &baseline);
        crate::view::test_support::get_element_mut::<Element>(&arena, child)
            .set_background_color_value(Color::rgb(12, 220, 44));
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let child_paint = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("child paint plan");
        let mut child_graph = FrameGraph::new();
        let mut child_ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let child_outer = child_ctx.allocate_target(&mut child_graph);
        child_ctx.set_current_target(child_outer);
        let child_state = super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &child_paint,
            &mut child_graph,
            child_ctx,
        )
        .expect("child paint R/R");
        assert_eq!(child_state.opaque_rect_order_for_test(), 3);
        assert_eq!(child_state.target_pair_count_for_test(), 3);
        assert_eq!(
            child_graph
                .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
                .len(),
            2
        );
        assert_eq!(child_graph.test_rect_pass_snapshots().len(), 5);
        assert_eq!(
            child_graph
                .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
                .len(),
            2
        );
        assert_eq!(child_graph.declared_persistent_texture_keys().count(), 4);
    }

    #[test]
    fn nested_transform_shape_and_affine_rejections_fail_closed() {
        let (arena, root, before, _child, _descendant, _after, _, _) =
            nested_exact_transform_fixture();
        crate::view::test_support::get_element_mut::<Element>(&arena, before)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                5.0, 0.0, 0.0,
            ))));
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        let multiple = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("two direct transformed children exceed the C5A1 exact shape");
        assert!(
            multiple
                .reasons
                .contains(&FramePaintPlanRejection::TransformNodeCount(3))
        );

        let (arena, root, _before, child, descendant, _after, _, _) =
            nested_exact_transform_fixture();
        crate::view::test_support::get_element_mut::<Element>(&arena, child)
            .set_resolved_transform_for_test(None);
        crate::view::test_support::get_element_mut::<Element>(&arena, descendant)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                7.0, 0.0, 0.0,
            ))));
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        let depth_three = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("a transformed grandchild is outside the direct-child C5A1 shape");
        assert!(
            depth_three
                .reasons
                .contains(&FramePaintPlanRejection::UnexpectedTransform(descendant))
        );

        let (arena, root, _before, child, _descendant, _after, mut properties, generations) =
            nested_exact_transform_fixture();
        let child_transform = properties
            .transforms
            .get_mut(&TransformNodeId(child))
            .expect("child transform");
        let mut perspective = child_transform.viewport_matrix.to_cols_array();
        perspective[3] = 0.25;
        child_transform.viewport_matrix = glam::Mat4::from_cols_array(&perspective);
        let non_affine = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("perspective child matrix is outside C5A1");
        assert!(
            non_affine
                .reasons
                .contains(&FramePaintPlanRejection::NonAffineTransform(child))
        );

        let (arena, root, _before, child, _descendant, _after, mut properties, generations) =
            nested_exact_transform_fixture();
        properties
            .transforms
            .get_mut(&TransformNodeId(child))
            .expect("child transform")
            .viewport_matrix = glam::Mat4::from_translation(glam::Vec3::new(31.0, 0.0, 0.0));
        let mismatched = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("planned C must match the Element canonical geometry matrix bit-for-bit");
        assert_eq!(
            mismatched.reasons,
            vec![FramePaintPlanRejection::InvalidRootTransform(child)]
        );
    }

    #[test]
    fn nested_opaque_spans_use_surface_local_cursor_and_max_child_terminal() {
        let (arena, root, child, properties, generations) = nested_opaque_cursor_fixture(3, 1, 1);
        let plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("parent-before dominant cursor fixture");
        let [PaintPlanStep::RetainedSurface(parent)] = plan.steps.as_slice() else {
            panic!("one parent surface")
        };
        let [
            PaintPlanStep::ArtifactSpan(before),
            PaintPlanStep::RetainedSurface(nested),
            PaintPlanStep::ArtifactSpan(after),
        ] = parent.raster_steps.as_slice()
        else {
            panic!("before, child, after roles must stay explicit")
        };
        let [PaintPlanStep::ArtifactSpan(child_span)] = nested.raster_steps.as_slice() else {
            panic!("one child-local artifact span")
        };
        assert_eq!(nested.boundary_root, child);
        assert_eq!(opaque_order_count(&before.artifact), 3);
        assert_eq!(opaque_order_count(&child_span.artifact), 1);
        assert_eq!(opaque_order_count(&after.artifact), 1);
        assert_eq!(before.opaque_order_span, 0..3);
        assert_eq!(child_span.opaque_order_span, 0..1);
        assert_eq!(nested.aggregate_opaque_order_span, 0..1);
        assert_eq!(after.opaque_order_span, 3..4);
        assert_eq!(parent.aggregate_opaque_order_span, 0..4);

        let (arena, root, child, properties, generations) = nested_opaque_cursor_fixture(0, 2, 1);
        let plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("child-dominant cursor fixture");
        let [PaintPlanStep::RetainedSurface(parent)] = plan.steps.as_slice() else {
            panic!("one parent surface")
        };
        let [
            PaintPlanStep::ArtifactSpan(before),
            PaintPlanStep::RetainedSurface(nested),
            PaintPlanStep::ArtifactSpan(after),
        ] = parent.raster_steps.as_slice()
        else {
            panic!("zero-count parent-before span must retain its owning role")
        };
        let [PaintPlanStep::ArtifactSpan(child_span)] = nested.raster_steps.as_slice() else {
            panic!("one child-local artifact span")
        };
        assert_eq!(nested.boundary_root, child);
        assert_eq!(opaque_order_count(&before.artifact), 0);
        assert_eq!(opaque_order_count(&child_span.artifact), 2);
        assert_eq!(opaque_order_count(&after.artifact), 1);
        assert_eq!(before.opaque_order_span, 0..0);
        assert_eq!(child_span.opaque_order_span, 0..2);
        assert_eq!(nested.aggregate_opaque_order_span, 0..2);
        assert_eq!(after.opaque_order_span, 2..3);
        assert_eq!(parent.aggregate_opaque_order_span, 0..3);
    }

    #[test]
    fn nested_stamp_tracks_child_raster_and_composite_geometry_but_not_parent_transform_only() {
        let (arena, root, _before, child, _descendant, _after, mut properties, mut generations) =
            nested_exact_transform_fixture();
        let plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("baseline nested plan");
        let ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let baseline = super::super::prepare_forced_retained_surface_stamp_for_test(
            &plan,
            &FrameGraph::new(),
            &ctx,
        )
        .expect("baseline nested stamp");
        let child_stamp = |stamp: &super::super::RetainedSurfaceRasterStamp| {
            let super::super::RetainedSurfaceRasterStepStamp::NestedSurface(dependency) =
                &stamp.ordered_steps[1]
            else {
                panic!("middle step is the exact child dependency")
            };
            dependency.child_stamp.as_ref().clone()
        };

        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                101.0, 0.0, 0.0,
            ))));
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let parent_transform_plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("parent transform-only nested plan");
        let parent_transform_stamp = super::super::prepare_forced_retained_surface_stamp_for_test(
            &parent_transform_plan,
            &FrameGraph::new(),
            &ctx,
        )
        .expect("parent transform-only stamp");
        assert_eq!(
            parent_transform_stamp, baseline,
            "parent final composite transform stays outside its own raster stamp"
        );

        let child_matrix = glam::Mat4::from_cols_array(&[
            0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 31.0, 0.0, 0.0, 1.0,
        ]);
        crate::view::test_support::get_element_mut::<Element>(&arena, child)
            .set_resolved_transform_for_test(Some(child_matrix));
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let child_transform_plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("child transform-only nested plan");
        let child_transform_stamp = super::super::prepare_forced_retained_surface_stamp_for_test(
            &child_transform_plan,
            &FrameGraph::new(),
            &ctx,
        )
        .expect("child transform-only stamp");
        assert_eq!(
            child_stamp(&child_transform_stamp),
            child_stamp(&baseline),
            "child final composite transform stays outside the child raster stamp"
        );
        assert_ne!(
            child_transform_stamp, baseline,
            "child composite geometry is an exact parent raster dependency"
        );

        crate::view::test_support::get_element_mut::<Element>(&arena, child)
            .set_background_color_value(Color::rgb(12, 220, 44));
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let child_paint_plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("child paint nested plan");
        let child_paint_stamp = super::super::prepare_forced_retained_surface_stamp_for_test(
            &child_paint_plan,
            &FrameGraph::new(),
            &ctx,
        )
        .expect("child paint stamp");
        assert_ne!(child_stamp(&child_paint_stamp), child_stamp(&baseline));
        assert_ne!(child_paint_stamp, baseline);
    }

    #[test]
    fn forced_rect_executor_emits_clear_raster_composite_to_distinct_targets() {
        let (arena, root, properties, generations) = exact_transform_fixture();
        let plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("exact rect surface plan");

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let parent_target = ctx.allocate_target(&mut graph);
        let parent_handle = parent_target.handle().expect("parent texture handle");
        ctx.set_current_target(parent_target);
        let mut viewport = Viewport::new();
        let state = super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &plan,
            &mut graph,
            ctx,
        )
        .expect("forced exact rect surface execution");
        assert_eq!(
            state.current_target().and_then(|target| target.handle()),
            Some(parent_handle)
        );

        let pass_names = graph
            .pass_descriptors()
            .into_iter()
            .map(|descriptor| descriptor.name)
            .collect::<Vec<_>>();
        assert_eq!(
            pass_names.first().copied(),
            Some(std::any::type_name::<crate::view::render_pass::ClearPass>())
        );
        assert_eq!(
            pass_names.last().copied(),
            Some(std::any::type_name::<
                crate::view::render_pass::TextureCompositePass,
            >())
        );
        assert!(pass_names[1..pass_names.len() - 1].iter().all(|name| {
            *name
                == std::any::type_name::<
                    crate::view::render_pass::draw_rect_pass::OpaqueRectPass,
                >()
                || *name
                    == std::any::type_name::<
                        crate::view::render_pass::draw_rect_pass::DrawRectPass,
                    >()
        }));

        let clears = graph.test_graphics_passes::<crate::view::render_pass::ClearPass>();
        let [clear] = clears.as_slice() else {
            panic!("forced surface emits one transparent clear")
        };
        let clear = clear.test_snapshot();
        assert_eq!(clear.color_bits, [0.0_f32.to_bits(); 4]);
        assert!(clear.clear_depth_stencil);
        assert_ne!(clear.output_target, Some(parent_handle));

        let composites =
            graph.test_graphics_passes::<crate::view::render_pass::TextureCompositePass>();
        let [composite] = composites.as_slice() else {
            panic!("forced surface emits one final composite")
        };
        let composite = composite.test_snapshot();
        assert_eq!(composite.source_handle, clear.output_target);
        assert_eq!(composite.output_target, Some(parent_handle));
        assert_eq!(
            graph.declared_persistent_texture_keys().collect::<Vec<_>>(),
            vec![
                crate::view::base_component::transformed_layer_stable_key(0xc1_0001),
                crate::view::base_component::transformed_layer_stable_key(0xc1_0001)
                    .depth_stencil()
                    .expect("transformed depth key"),
            ]
        );
    }

    #[test]
    fn retained_surface_stamp_excludes_transform_only_drift_and_tracks_raster_drift() {
        let (arena, root, mut properties, mut generations) = exact_transform_fixture();
        let first_plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("exact retained surface plan");
        let [PaintPlanStep::RetainedSurface(first_surface)] = first_plan.steps.as_slice() else {
            panic!("one retained surface")
        };
        let baseline = retained_surface_stamp(first_surface, &only_span(first_surface).artifact)
            .expect("validated retained raster stamp");
        let first_boundary_self_revision = only_span(first_surface)
            .artifact
            .chunks
            .iter()
            .find(|chunk| chunk.owner == root)
            .expect("boundary chunk")
            .content_revision
            .self_paint_revision;
        let first_viewport_transform = first_surface.geometry().viewport_transform;
        assert_eq!(baseline.identity.boundary_root, root);
        assert_eq!(baseline.identity.stable_id, 0xc1_0001);
        assert_eq!(
            baseline.identity.color_key,
            first_surface.persistent_color_key
        );
        assert!(
            baseline
                .target
                .has_canonical_descriptor_pair_for(baseline.identity)
        );
        assert_eq!(baseline.opaque_order_span, 0..2);

        let mut transform_style = Style::new();
        transform_style.set_transform(Transform::new([Rotate::z(Angle::deg(24.0))]));
        arena
            .get_mut(root)
            .expect("root")
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("Element root")
            .apply_style(transform_style);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let transform_plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("transform-only retained surface replan");
        let [PaintPlanStep::RetainedSurface(transform_surface)] = transform_plan.steps.as_slice()
        else {
            panic!("one retained surface")
        };
        let transform_boundary_self_revision = only_span(transform_surface)
            .artifact
            .chunks
            .iter()
            .find(|chunk| chunk.owner == root)
            .expect("boundary chunk")
            .content_revision
            .self_paint_revision;
        assert_ne!(
            transform_boundary_self_revision, first_boundary_self_revision,
            "real property-tree transform generation is conservatively folded into boundary self paint"
        );
        assert_ne!(
            transform_surface.geometry().viewport_transform,
            first_viewport_transform,
            "the second plan must carry the latest composite matrix"
        );
        assert_eq!(
            retained_surface_stamp(transform_surface, &only_span(transform_surface).artifact),
            Some(baseline.clone()),
            "a true transform sync/re-record must keep the raster stamp reusable"
        );

        let mut boundary_composite = only_span(transform_surface).artifact.clone();
        boundary_composite
            .chunks
            .iter_mut()
            .find(|chunk| chunk.owner == root)
            .expect("boundary chunk")
            .content_revision
            .composite_revision += 1;
        assert_eq!(
            retained_surface_stamp(transform_surface, &boundary_composite),
            Some(baseline.clone()),
            "boundary composite revision is consumed by the final composite"
        );

        let child = arena.get(root).expect("root").element.children()[0];
        let mut descendant_composite = only_span(transform_surface).artifact.clone();
        descendant_composite
            .chunks
            .iter_mut()
            .find(|chunk| chunk.owner == child)
            .expect("descendant chunk")
            .content_revision
            .composite_revision += 1;
        assert_ne!(
            retained_surface_stamp(transform_surface, &descendant_composite),
            Some(baseline.clone())
        );

        arena
            .get_mut(root)
            .expect("root")
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("Element root")
            .set_background_color_value(Color::rgb(90, 110, 130));
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let repaint_plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("root-fill retained surface replan");
        let [PaintPlanStep::RetainedSurface(repaint_surface)] = repaint_plan.steps.as_slice()
        else {
            panic!("one retained surface")
        };
        assert_ne!(
            retained_surface_stamp(repaint_surface, &only_span(repaint_surface).artifact),
            Some(baseline.clone()),
            "exact root paint payload identity must still veto reuse"
        );

        let mut invalid_store = only_span(transform_surface).artifact.clone();
        invalid_store.chunks[0].properties.transform = None;
        assert!(retained_surface_stamp(transform_surface, &invalid_store).is_none());
    }

    #[test]
    fn forced_retained_surface_reuses_only_after_success_and_composites_latest_transform() {
        let (arena, root, mut properties, mut generations) = exact_transform_fixture();
        let first_plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("first retained surface plan");
        let mut viewport = Viewport::new();
        let mut first_graph = FrameGraph::new();
        let first_ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let first_state = super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &first_plan,
            &mut first_graph,
            first_ctx,
        )
        .expect("first frame reraster");
        assert_eq!(first_state.opaque_rect_order_for_test(), 2);
        assert_eq!(
            first_graph
                .test_graphics_passes::<crate::view::render_pass::ClearPass>()
                .len(),
            1
        );
        assert_eq!(
            first_graph
                .test_graphics_passes::<crate::view::render_pass::draw_rect_pass::OpaqueRectPass>()
                .len()
                + first_graph
                    .test_graphics_passes::<crate::view::render_pass::draw_rect_pass::DrawRectPass>(
                    )
                    .len(),
            2
        );
        let first_composite = first_graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()[0]
            .test_snapshot();

        viewport.finish_retained_surface_transaction(true);

        let mut transform_style = Style::new();
        transform_style.set_transform(Transform::new([Rotate::z(Angle::deg(24.0))]));
        arena
            .get_mut(root)
            .expect("root")
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("Element root")
            .apply_style(transform_style);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let latest_scissor = Some([3, 4, 50, 60]);
        let second_plan = plan_single_root_transform_surface_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], latest_scissor),
        )
        .expect("transform-only retained surface replan");
        let mut second_graph = FrameGraph::new();
        let mut second_ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        second_ctx.push_scissor_rect(latest_scissor);
        let second_state = super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &second_plan,
            &mut second_graph,
            second_ctx,
        )
        .expect("second frame reuse");
        assert_eq!(
            second_state.opaque_rect_order_for_test(),
            2,
            "reuse skips raster operations but must replay the prepared opaque terminal"
        );
        assert!(
            second_graph
                .test_graphics_passes::<crate::view::render_pass::ClearPass>()
                .is_empty(),
            "reuse must not clear the resident pair"
        );
        assert!(
            second_graph
                .test_graphics_passes::<crate::view::render_pass::draw_rect_pass::OpaqueRectPass>()
                .is_empty()
        );
        assert!(
            second_graph
                .test_graphics_passes::<crate::view::render_pass::draw_rect_pass::DrawRectPass>()
                .is_empty()
        );
        assert_eq!(
            second_graph
                .declared_persistent_texture_keys()
                .collect::<Vec<_>>()
                .len(),
            2,
            "reuse still declares the canonical persistent color/depth pair"
        );
        let second_composites =
            second_graph.test_graphics_passes::<crate::view::render_pass::TextureCompositePass>();
        let [second_composite] = second_composites.as_slice() else {
            panic!("reuse emits exactly one final composite")
        };
        let second_composite = second_composite.test_snapshot();
        assert_ne!(
            second_composite.quad_position_bits, first_composite.quad_position_bits,
            "reuse composite must use the latest transform geometry"
        );
        assert_eq!(second_composite.explicit_scissor_rect, latest_scissor);
        assert_eq!(second_composite.effective_scissor_rect, latest_scissor);
        viewport.finish_retained_surface_transaction(true);

        arena
            .get_mut(root)
            .expect("root")
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("Element root")
            .set_background_color_value(Color::rgb(90, 110, 130));
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let repaint_plan = plan_single_root_transform_surface_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            TransformSurfacePlanContext::new([0.0, 0.0], latest_scissor),
        )
        .expect("root paint retained surface replan");
        let mut repaint_graph = FrameGraph::new();
        let mut repaint_ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        repaint_ctx.push_scissor_rect(latest_scissor);
        super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &repaint_plan,
            &mut repaint_graph,
            repaint_ctx,
        )
        .expect("root paint change reraster");
        assert_eq!(
            repaint_graph
                .test_graphics_passes::<crate::view::render_pass::ClearPass>()
                .len(),
            1,
            "a real root paint/payload change must veto reuse"
        );
    }

    #[test]
    fn forced_retained_surface_failed_frame_cannot_become_reusable() {
        let (arena, root, properties, generations) = exact_transform_fixture();
        let plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("retained surface plan");
        for (compiled, executed, failure) in [
            (false, false, "compile failure"),
            (true, false, "execute failure"),
        ] {
            let mut viewport = Viewport::new();
            let mut failed_graph = FrameGraph::new();
            super::super::execute_forced_transform_surface_for_test(
                &mut viewport,
                &plan,
                &mut failed_graph,
                UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0),
            )
            .expect("failed frame still builds a reraster graph");
            viewport.finish_retained_surface_transaction(compiled && executed);

            let mut retry_graph = FrameGraph::new();
            super::super::execute_forced_transform_surface_for_test(
                &mut viewport,
                &plan,
                &mut retry_graph,
                UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0),
            )
            .expect("retry frame reraster");
            assert_eq!(
                retry_graph
                    .test_graphics_passes::<crate::view::render_pass::ClearPass>()
                    .len(),
                1,
                "{failure} must not commit a reusable resident pair"
            );
            assert_eq!(
                retry_graph
                    .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
                    .len(),
                1
            );
        }
    }

    #[test]
    fn forced_rect_executor_locks_nonzero_context_descriptor_pair_and_opaque_span() {
        let (arena, root, properties, generations) = exact_transform_fixture();
        let frozen = TransformSurfacePlanContext::new([0.25, -0.25], Some([3, 4, 50, 60]));
        let plan = plan_single_root_transform_surface_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
            frozen,
        )
        .expect("nonzero frozen transform context");
        let [PaintPlanStep::RetainedSurface(surface)] = plan.steps.as_slice() else {
            panic!("one retained surface")
        };
        assert_eq!(surface.context(), frozen);
        assert_eq!(surface.aggregate_opaque_order_span, 0..2);
        assert_eq!(
            [
                surface.geometry().source_bounds.x.to_bits(),
                surface.geometry().source_bounds.y.to_bits(),
                surface.geometry().source_bounds.width.to_bits(),
                surface.geometry().source_bounds.height.to_bits(),
            ],
            [
                8.5_f32.to_bits(),
                7.0_f32.to_bits(),
                40.0_f32.to_bits(),
                24.0_f32.to_bits(),
            ]
        );
        assert_eq!(
            [
                surface.geometry().visual_bounds.x.to_bits(),
                surface.geometry().visual_bounds.y.to_bits(),
            ],
            [9.0_f32.to_bits(), 7.0_f32.to_bits()],
            "hard-coded paint-snap delta is (+0.5, 0.0)"
        );
        assert_eq!(surface.geometry().outer_scissor_rect, Some([3, 4, 50, 60]));

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 2.0);
        ctx.translate_paint_offset(0.25, -0.25);
        ctx.push_scissor_rect(Some([3, 4, 50, 60]));
        let mut viewport = Viewport::new();
        let state = super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &plan,
            &mut graph,
            ctx,
        )
        .expect("runtime context matches frozen plan bit-for-bit");
        assert_eq!(state.opaque_rect_order_for_test(), 2);

        let color_key = crate::view::base_component::transformed_layer_stable_key(0xc1_0001);
        let depth_key = color_key.depth_stencil().expect("transformed depth key");
        let declared = graph
            .declared_persistent_textures()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(declared.len(), 2);
        let color = declared.get(&color_key).expect("surface color descriptor");
        let depth = declared.get(&depth_key).expect("surface depth descriptor");
        assert_eq!((color.width(), color.height()), (80, 48));
        assert_eq!(color.origin(), (17, 14));
        assert_eq!(color.format(), wgpu::TextureFormat::Bgra8Unorm);
        assert_eq!(color.dimension(), wgpu::TextureDimension::D2);
        assert_eq!(color.sample_count(), 1);
        assert_eq!(
            color.usage(),
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
        );
        assert_eq!((depth.width(), depth.height()), (80, 48));
        assert_eq!(depth.origin(), (0, 0));
        assert_eq!(depth.format(), wgpu::TextureFormat::Depth24PlusStencil8);
        assert_eq!(depth.dimension(), wgpu::TextureDimension::D2);
        assert_eq!(depth.sample_count(), 1);
        assert_eq!(depth.usage(), wgpu::TextureUsages::RENDER_ATTACHMENT);

        let rects = graph.test_rect_pass_snapshots();
        let opaque_orders = rects
            .iter()
            .map(|snapshot| snapshot.opaque_depth_order)
            .collect::<Vec<_>>();
        assert_eq!(opaque_orders, vec![Some(0), Some(1)]);
        assert!(rects.iter().all(|snapshot| {
            snapshot.explicit_scissor_rect.is_none()
                && snapshot.effective_scissor_rect.is_none()
                && snapshot.input_target == snapshot.output_target
        }));
        let clears = graph.test_graphics_passes::<crate::view::render_pass::ClearPass>();
        let [clear] = clears.as_slice() else {
            panic!("one surface clear")
        };
        let clear = clear.test_snapshot();
        assert!(rects.iter().all(|snapshot| {
            snapshot.input_target == clear.output_target
                && snapshot.output_target == clear.output_target
        }));
        let composites =
            graph.test_graphics_passes::<crate::view::render_pass::TextureCompositePass>();
        let [composite] = composites.as_slice() else {
            panic!("one final composite")
        };
        let composite = composite.test_snapshot();
        assert_eq!(composite.source_handle, clear.output_target);
        assert_eq!(
            composite.output_target,
            state.current_target().and_then(|target| target.handle())
        );
        assert_ne!(composite.output_target, composite.source_handle);
        assert_eq!(composite.explicit_scissor_rect, Some([3, 4, 50, 60]));
        assert_eq!(
            composite.bounds_bits,
            [
                9.0_f32.to_bits(),
                7.0_f32.to_bits(),
                40.0_f32.to_bits(),
                24.0_f32.to_bits(),
            ]
        );
        assert_eq!(
            composite.uv_bounds_bits,
            Some([
                8.5_f32.to_bits(),
                7.0_f32.to_bits(),
                40.0_f32.to_bits(),
                24.0_f32.to_bits(),
            ])
        );
    }

    #[test]
    fn forced_executor_rejections_are_table_driven_and_graph_bit_identical() {
        use super::super::ForcedTransformSurfaceError as Error;

        let (arena, root, properties, generations) = exact_transform_fixture();
        let baseline = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("baseline forced plan");
        let default_ctx = || UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);

        let mut plan = baseline.clone();
        only_surface_mut(&mut plan)
            .transform_plan_mut_for_test()
            .geometry
            .visual_bounds
            .x = f32::NAN;
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::GeometryContract,
        );

        let mut plan = baseline.clone();
        only_surface_mut(&mut plan)
            .transform_plan_mut_for_test()
            .context = TransformSurfacePlanContext::new([0.25, 0.0], None);
        let mut matching_tampered_ctx = default_ctx();
        matching_tampered_ctx.translate_paint_offset(0.25, 0.0);
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            matching_tampered_ctx,
            Error::GeometryContract,
        );

        let mut plan = baseline.clone();
        only_surface_mut(&mut plan)
            .transform_plan_mut_for_test()
            .geometry
            .quad_positions[0][0] = f32::NAN;
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::GeometryContract,
        );

        let mut plan = baseline.clone();
        only_surface_mut(&mut plan)
            .transform_plan_mut_for_test()
            .geometry
            .uv_bounds[0] += 1.0;
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::GeometryContract,
        );

        let mut plan = baseline.clone();
        only_surface_mut(&mut plan)
            .transform_plan_mut_for_test()
            .geometry
            .outer_scissor_rect = Some([1, 2, 3, 4]);
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::GeometryContract,
        );

        let mut plan = baseline.clone();
        only_surface_mut(&mut plan).aggregate_opaque_order_span.end += 1;
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::OpaqueSpan,
        );

        let mut plan = baseline.clone();
        only_span_mut(only_surface_mut(&mut plan)).artifact.chunks[0]
            .properties
            .transform = None;
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::ArtifactStore,
        );

        let mut plan = baseline.clone();
        let surface = only_surface_mut(&mut plan);
        let boundary_root = surface.boundary_root;
        only_span_mut(surface).artifact.owner_nodes[0].parent = Some(boundary_root);
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::ArtifactStore,
        );

        let mut plan = baseline.clone();
        let surface = only_surface_mut(&mut plan);
        let span = only_span_mut(surface);
        let PaintOp::DrawRect(rect) = &mut span.artifact.ops[0] else {
            panic!("rect fixture starts with decoration")
        };
        rect.params.opacity = 0.25;
        span.opaque_order_span.end = opaque_order_count(&span.artifact);
        surface.aggregate_opaque_order_span = span.opaque_order_span.clone();
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::ArtifactStore,
        );

        let mut plan = baseline.clone();
        let surface = only_surface_mut(&mut plan);
        let boundary_root = surface.boundary_root;
        only_span_mut(surface).artifact.target =
            super::super::PaintArtifactTarget::RootOpacityGroup {
                root: boundary_root,
                effect: crate::view::compositor::property_tree::EffectNodeId(boundary_root),
            };
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::ArtifactTarget,
        );

        let mut plan = baseline.clone();
        only_surface_mut(&mut plan).stable_id = 999_999;
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::BoundaryIdentity,
        );

        let mut plan = baseline.clone();
        let nested = plan.steps[0].clone();
        only_surface_mut(&mut plan).raster_steps.push(nested);
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::NestedSurface,
        );

        let mut plan = baseline.clone();
        only_surface_mut(&mut plan).parent_surface = Some(root);
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::NestedSurface,
        );

        let mut plan = baseline.clone();
        let top_level_span = only_span(only_surface_mut(&mut plan)).clone();
        plan.steps[0] = PaintPlanStep::ArtifactSpan(top_level_span);
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::PlanShape,
        );

        let mut plan = baseline.clone();
        plan.steps.push(plan.steps[0].clone());
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::PlanShape,
        );

        let mut ctx = default_ctx();
        ctx.translate_paint_offset(0.25, 0.0);
        assert_forced_rejection_has_zero_graph_mutation(
            &baseline,
            &mut FrameGraph::new(),
            ctx,
            Error::ContextMismatch,
        );

        let mut graph = FrameGraph::new();
        let mut declaration_ctx = default_ctx();
        let color_key = crate::view::base_component::transformed_layer_stable_key(0xc1_0001);
        let _ = declaration_ctx.allocate_persistent_target_with_key(
            &mut graph,
            color_key,
            only_surface_mut(&mut baseline.clone())
                .transform_plan_for_test()
                .geometry
                .source_bounds,
        );
        assert_forced_rejection_has_zero_graph_mutation(
            &baseline,
            &mut graph,
            declaration_ctx,
            Error::PersistentKeyAlreadyDeclared(color_key),
        );

        let mut foreign_graph = FrameGraph::new();
        let mut foreign_ctx = default_ctx();
        let foreign_target = foreign_ctx.allocate_target(&mut foreign_graph);
        foreign_ctx.set_current_target(foreign_target);
        assert_forced_rejection_has_zero_graph_mutation(
            &baseline,
            &mut FrameGraph::new(),
            foreign_ctx,
            Error::ParentTarget,
        );

        let mut graph = FrameGraph::new();
        let bad_parent = graph
            .declare_texture::<crate::view::render_pass::draw_rect_pass::RenderTargetTag>(
                crate::view::frame_graph::TextureDesc::new(
                    16,
                    16,
                    wgpu::TextureFormat::Rgba8Unorm,
                    wgpu::TextureDimension::D1,
                )
                .with_usage(wgpu::TextureUsages::TEXTURE_BINDING),
            );
        let mut ctx = default_ctx();
        ctx.set_current_target(bad_parent);
        assert_forced_rejection_has_zero_graph_mutation(
            &baseline,
            &mut graph,
            ctx,
            Error::ParentTarget,
        );
    }

    #[test]
    fn forced_nested_prepare_rejections_are_deep_and_transactionally_inert() {
        use super::super::ForcedTransformSurfaceError as Error;

        let (arena, root, _before, child, _descendant, _after, properties, generations) =
            nested_exact_transform_fixture();
        let baseline = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("baseline nested plan");
        let default_ctx = || UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);

        let mut plan = baseline.clone();
        only_span_mut(nested_surface_mut(&mut plan)).artifact.chunks[0]
            .properties
            .transform = None;
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::ArtifactStore,
        );

        let mut plan = baseline.clone();
        nested_surface_mut(&mut plan).persistent_color_key =
            crate::view::base_component::transformed_layer_stable_key(0xdead_beef);
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::BoundaryIdentity,
        );

        let mut plan = baseline.clone();
        nested_surface_mut(&mut plan)
            .transform_plan_mut_for_test()
            .geometry
            .quad_positions[0][0] = f32::NAN;
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::GeometryContract,
        );

        let mut plan = baseline.clone();
        only_span_mut(nested_surface_mut(&mut plan))
            .opaque_order_span
            .end += 1;
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::OpaqueSpan,
        );

        let mut plan = baseline.clone();
        nested_surface_mut(&mut plan)
            .aggregate_opaque_order_span
            .end += 1;
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::OpaqueSpan,
        );

        let mut plan = baseline.clone();
        nested_surface_mut(&mut plan).parent_surface = None;
        assert_forced_rejection_has_zero_graph_mutation(
            &plan,
            &mut FrameGraph::new(),
            default_ctx(),
            Error::NestedSurface,
        );

        let child_key = crate::view::base_component::transformed_layer_stable_key(
            arena.get(child).expect("child").element.stable_id(),
        );
        let child_bounds = nested_surface_mut(&mut baseline.clone())
            .transform_plan_for_test()
            .geometry
            .source_bounds;
        let mut graph = FrameGraph::new();
        let mut declaration_ctx = default_ctx();
        let _ = declaration_ctx.allocate_persistent_target_with_key(
            &mut graph,
            child_key,
            child_bounds,
        );
        assert_forced_rejection_has_zero_graph_mutation(
            &baseline,
            &mut graph,
            declaration_ctx,
            Error::PersistentKeyAlreadyDeclared(child_key),
        );
    }

    #[test]
    fn forced_rect_graph_is_strictly_identical_to_legacy_graph() {
        let (mut legacy_arena, legacy_root, _, _) = exact_transform_fixture();
        let mut legacy_graph = FrameGraph::new();
        let (legacy_ctx, legacy_parent) =
            parent_context_with_clear(&mut legacy_graph, 160, 120, 1.0);
        legacy_arena
            .with_element_taken(legacy_root, |element, arena| {
                element.build(&mut legacy_graph, arena, legacy_ctx)
            })
            .expect("legacy transformed rect build");
        legacy_graph
            .add_texture_sink(
                &legacy_parent,
                crate::view::frame_graph::ExternalSinkKind::DebugCapture,
            )
            .expect("legacy parent sink");

        let (forced_arena, forced_root, properties, generations) = exact_transform_fixture();
        let forced_plan = plan_single_root_transform_surface(
            &forced_arena,
            &[forced_root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("forced transformed rect plan");
        let mut forced_graph = FrameGraph::new();
        let (forced_ctx, forced_parent) =
            parent_context_with_clear(&mut forced_graph, 160, 120, 1.0);
        let mut viewport = Viewport::new();
        super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &forced_plan,
            &mut forced_graph,
            forced_ctx,
        )
        .expect("forced transformed rect build");
        forced_graph
            .add_texture_sink(
                &forced_parent,
                crate::view::frame_graph::ExternalSinkKind::DebugCapture,
            )
            .expect("forced parent sink");

        let legacy = legacy_graph
            .test_compile_snapshot()
            .expect("strict legacy graph snapshot");
        let forced = forced_graph
            .test_compile_snapshot()
            .expect("strict forced graph snapshot");
        assert_eq!(forced, legacy);
    }

    #[test]
    fn forced_rect_nonzero_context_graph_is_strictly_identical_to_legacy_graph() {
        let frozen = TransformSurfacePlanContext::new([0.25, -0.25], Some([3, 4, 50, 60]));
        let (mut legacy_arena, legacy_root, _, _) = exact_transform_fixture();
        let mut legacy_graph = FrameGraph::new();
        let (mut legacy_ctx, legacy_parent) =
            parent_context_with_clear(&mut legacy_graph, 160, 120, 2.0);
        legacy_ctx.translate_paint_offset(0.25, -0.25);
        legacy_ctx.push_scissor_rect(Some([3, 4, 50, 60]));
        legacy_arena
            .with_element_taken(legacy_root, |element, arena| {
                element.build(&mut legacy_graph, arena, legacy_ctx)
            })
            .expect("legacy nonzero-context transformed rect build");
        legacy_graph
            .add_texture_sink(
                &legacy_parent,
                crate::view::frame_graph::ExternalSinkKind::DebugCapture,
            )
            .expect("legacy nonzero-context parent sink");

        let (forced_arena, forced_root, properties, generations) = exact_transform_fixture();
        let forced_plan = plan_single_root_transform_surface_with_context(
            &forced_arena,
            &[forced_root],
            &FxHashSet::default(),
            &properties,
            &generations,
            frozen,
        )
        .expect("forced nonzero-context transformed rect plan");
        let mut forced_graph = FrameGraph::new();
        let (mut forced_ctx, forced_parent) =
            parent_context_with_clear(&mut forced_graph, 160, 120, 2.0);
        forced_ctx.translate_paint_offset(0.25, -0.25);
        forced_ctx.push_scissor_rect(Some([3, 4, 50, 60]));
        let mut viewport = Viewport::new();
        super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &forced_plan,
            &mut forced_graph,
            forced_ctx,
        )
        .expect("forced nonzero-context transformed rect build");
        forced_graph
            .add_texture_sink(
                &forced_parent,
                crate::view::frame_graph::ExternalSinkKind::DebugCapture,
            )
            .expect("forced nonzero-context parent sink");

        assert_eq!(
            forced_graph
                .test_compile_snapshot()
                .expect("strict forced nonzero-context snapshot"),
            legacy_graph
                .test_compile_snapshot()
                .expect("strict legacy nonzero-context snapshot")
        );
    }

    #[test]
    fn forced_zero_blur_shadow_graph_is_strictly_identical_to_legacy_graph() {
        let mut root = Element::new_with_id(0xc2_b100, 20.0, 20.0, 30.0, 18.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(40, 80, 120)),
        );
        style.set_box_shadow(vec![
            BoxShadow::new().offset_x(3.0).offset_y(4.0).spread(2.0),
        ]);
        style.set_transform(Transform::new([Rotate::z(Angle::deg(6.0))]));
        root.apply_style(style);

        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(root));
        measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 160.0,
                max_height: 120.0,
                viewport_width: 160.0,
                viewport_height: 120.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
                parent_x: 20.0,
                parent_y: 20.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 160.0,
                available_height: 120.0,
                viewport_width: 160.0,
                viewport_height: 120.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(120.0),
            },
        );
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        let plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("zero-blur outer shadow surface plan");
        let [PaintPlanStep::RetainedSurface(surface)] = plan.steps.as_slice() else {
            panic!("one shadow surface")
        };
        assert!(
            only_span(surface)
                .artifact
                .ops
                .iter()
                .any(|op| matches!(op, PaintOp::PreparedShadow(_)))
        );

        let mut legacy_graph = FrameGraph::new();
        let (legacy_ctx, legacy_parent) =
            parent_context_with_clear(&mut legacy_graph, 160, 120, 1.0);
        arena
            .with_element_taken(root, |element, arena| {
                element.build(&mut legacy_graph, arena, legacy_ctx)
            })
            .expect("legacy zero-blur shadow build");
        legacy_graph
            .add_texture_sink(
                &legacy_parent,
                crate::view::frame_graph::ExternalSinkKind::DebugCapture,
            )
            .expect("legacy shadow sink");

        let mut forced_graph = FrameGraph::new();
        let (forced_ctx, forced_parent) =
            parent_context_with_clear(&mut forced_graph, 160, 120, 1.0);
        let mut viewport = Viewport::new();
        super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &plan,
            &mut forced_graph,
            forced_ctx,
        )
        .expect("forced zero-blur shadow build");
        forced_graph
            .add_texture_sink(
                &forced_parent,
                crate::view::frame_graph::ExternalSinkKind::DebugCapture,
            )
            .expect("forced shadow sink");

        assert_eq!(
            forced_graph
                .test_compile_snapshot()
                .expect("strict forced shadow snapshot"),
            legacy_graph
                .test_compile_snapshot()
                .expect("strict legacy shadow snapshot")
        );
    }

    #[test]
    fn ordinary_recorder_and_compiler_still_reject_the_same_transform_artifact() {
        let (arena, root, properties, generations) = exact_transform_fixture();
        let outcome = super::super::record_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            super::super::RendererMode::Auto,
        )
        .expect("auto recorder must return a whole-frame fallback");
        let super::super::FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) =
            outcome
        else {
            panic!("ordinary recorder must not acquire transform surface authority")
        };
        assert!(eligibility.reasons.contains(
            &super::super::FrameArtifactFallbackReason::LegacyBoundary(
                super::super::LegacyPaintReason::Transform,
            )
        ));

        let plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("surface-only planner owns the positive path");
        let PaintPlanStep::RetainedSurface(surface) = &plan.steps[0] else {
            panic!("one retained surface")
        };
        let mut graph = FrameGraph::new();
        let before = graph.build_state_snapshot_for_test();
        let result = super::super::try_compile_artifact(
            &only_span(surface).artifact,
            &mut graph,
            UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0),
        );
        let error = match result {
            Ok(_) => panic!("general compiler must reject transform chunks"),
            Err(error) => error,
        };
        assert_eq!(
            error.kind(),
            super::super::ArtifactCompileErrorKind::InvalidStore
        );
        assert_eq!(graph.build_state_snapshot_for_test(), before);
    }

    #[test]
    fn ambient_or_wrong_owner_transform_witness_cannot_escape_surface_policy() {
        let (arena, root, properties, generations) = exact_transform_fixture();
        let root_stable_id = arena.get(root).expect("root").element.stable_id();
        let ambient = super::super::PaintRecordingContext {
            recording_owner: Some(root),
            recording_owner_stable_id: Some(root_stable_id),
            transform_surface: Some(PaintTransformSurfaceWitness::canonical_root(root)),
            ..Default::default()
        };
        let manifest = super::super::coverage_manifest::record_coverage_manifest_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            None,
            false,
            true,
            super::super::CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
            ambient,
            None,
            &Default::default(),
        );
        assert!(matches!(
            manifest.items.as_slice(),
            [super::super::PaintCoverageItem::LegacyBoundary {
                reason: super::super::LegacyPaintReason::Transform,
                ..
            }]
        ));

        let child = arena
            .get(root)
            .expect("root")
            .element
            .children()
            .first()
            .copied()
            .expect("child");
        let wrong_owner = super::super::PaintRecordingContext {
            recording_owner: Some(root),
            recording_owner_stable_id: Some(root_stable_id),
            transform_surface: Some(
                PaintTransformSurfaceWitness::canonical_root(root).for_target(child),
            ),
            ..Default::default()
        };
        assert!(!wrong_owner.authorizes_transform_surface_root(root_stable_id));
        assert!(!wrong_owner.authorizes_transform_surface_owner(Some(TransformNodeId(root))));
    }

    #[test]
    fn planner_rejects_nested_subroot_and_arena_trait_topology_drift() {
        let (arena, root, properties, generations) = exact_transform_fixture();
        let child = arena
            .get(root)
            .expect("root")
            .element
            .children()
            .first()
            .copied()
            .expect("child");
        let nested = plan_single_root_transform_surface(
            &arena,
            &[child],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("an arena child cannot masquerade as a frame root");
        assert!(
            nested
                .reasons
                .contains(&FramePaintPlanRejection::RootHasParent(child))
        );

        let (mut parent_drift_arena, parent_drift_root, properties, generations) =
            exact_transform_fixture();
        let parent_drift_child = parent_drift_arena
            .get(parent_drift_root)
            .expect("root")
            .element
            .children()[0];
        parent_drift_arena.set_parent(parent_drift_child, None);
        let parent_drift = plan_single_root_transform_surface(
            &parent_drift_arena,
            &[parent_drift_root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("foreign child parent edge must fail closed");
        assert!(
            parent_drift
                .reasons
                .contains(&FramePaintPlanRejection::TopologyMismatch(
                    parent_drift_child
                ))
        );

        let (mut mirror_arena, mirror_root, properties, generations) = exact_transform_fixture();
        mirror_arena.set_arena_children_without_mirror_for_test(mirror_root, Vec::new());
        let mirror = plan_single_root_transform_surface(
            &mirror_arena,
            &[mirror_root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("arena/trait child mirror drift must fail closed");
        assert!(
            mirror
                .reasons
                .contains(&FramePaintPlanRejection::TopologyMismatch(mirror_root))
        );
    }

    #[test]
    fn planner_rejects_zero_stable_id_for_root_or_descendant() {
        let (arena, root, properties, generations) = exact_transform_fixture();
        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_stable_id_for_test(0);
        let root_error = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("stable id zero cannot own a persistent transform surface");
        assert!(
            root_error
                .reasons
                .contains(&FramePaintPlanRejection::InvalidStableId(root))
        );

        let (arena, root, properties, generations) = exact_transform_fixture();
        let child = arena.get(root).expect("root").element.children()[0];
        crate::view::test_support::get_element_mut::<Element>(&arena, child)
            .set_stable_id_for_test(0);
        let child_error = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("every reachable owner requires a nonzero paint identity");
        assert!(
            child_error
                .reasons
                .contains(&FramePaintPlanRejection::InvalidStableId(child))
        );
    }

    #[test]
    fn planner_rejects_property_promotion_identity_and_root_shape_boundaries() {
        let (arena, root, properties, generations) = exact_transform_fixture();
        let child = arena.get(root).expect("root").element.children()[0];
        let multi_root = plan_single_root_transform_surface(
            &arena,
            &[root, child],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("M10C1 is exact single-root only");
        assert_eq!(
            multi_root.reasons,
            vec![FramePaintPlanRejection::RootCount(2)]
        );

        let promoted = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::from_iter([arena.get(child).expect("child").element.stable_id()]),
            &properties,
            &generations,
        )
        .expect_err("surface planning cannot mix promotion authority");
        assert!(
            promoted
                .reasons
                .iter()
                .any(|reason| { matches!(reason, FramePaintPlanRejection::PromotionPresent(_)) })
        );

        let (arena, root, properties, generations) = exact_transform_fixture();
        let child = arena.get(root).expect("root").element.children()[0];
        let root_id = arena.get(root).expect("root").element.stable_id();
        crate::view::test_support::get_element_mut::<Element>(&arena, child)
            .set_stable_id_for_test(root_id);
        let duplicate = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("duplicate nonzero stable ids cannot prove owning identity");
        assert!(
            duplicate
                .reasons
                .contains(&FramePaintPlanRejection::DuplicateStableId(root_id))
        );

        for property in ["clip", "effect", "scroll"] {
            let (arena, root, mut properties, generations) = exact_transform_fixture();
            let child = arena.get(root).expect("root").element.children()[0];
            let state = properties.states.get_mut(&child).expect("child state");
            match property {
                "clip" => {
                    state.paint.clip = Some(crate::view::compositor::property_tree::ClipNodeId {
                        owner: child,
                        role: crate::view::compositor::property_tree::ClipNodeRole::SelfClip,
                    });
                }
                "effect" => {
                    state.paint.effect =
                        Some(crate::view::compositor::property_tree::EffectNodeId(child));
                }
                "scroll" => {
                    state.paint.scroll =
                        Some(crate::view::compositor::property_tree::ScrollNodeId(child));
                }
                _ => unreachable!(),
            }
            let error = plan_single_root_transform_surface(
                &arena,
                &[root],
                &FxHashSet::default(),
                &properties,
                &generations,
            )
            .expect_err("non-transform property authority must stay out of M10C1");
            assert!(error.reasons.iter().any(|reason| match (property, reason) {
                ("clip", FramePaintPlanRejection::ClipBoundary(owner))
                | ("effect", FramePaintPlanRejection::EffectBoundary(owner))
                | ("scroll", FramePaintPlanRejection::ScrollBoundary(owner)) => *owner == child,
                _ => false,
            }));
        }
    }

    #[test]
    fn planner_explicitly_rejects_transformed_descendant_before_execution() {
        let (arena, root, mut properties, generations) = exact_transform_fixture();
        let child = arena.get(root).expect("root").element.children()[0];
        let root_transform = properties.transforms[&TransformNodeId(root)];
        properties.transforms.insert(
            TransformNodeId(child),
            crate::view::compositor::property_tree::TransformNode {
                owner: child,
                parent: Some(TransformNodeId(root)),
                viewport_matrix: root_transform.viewport_matrix,
                generation: 1,
            },
        );
        properties
            .states
            .get_mut(&child)
            .expect("child state")
            .paint
            .transform = Some(TransformNodeId(child));
        let nested = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("a second transform boundary requires recursive planning in a later slice");
        assert!(
            nested
                .reasons
                .contains(&FramePaintPlanRejection::WrongTransformBoundary(child))
        );
        assert_eq!(
            nested.reasons,
            vec![FramePaintPlanRejection::WrongTransformBoundary(child)],
            "an incomplete nested property scope must fail closed before plan ownership is built"
        );
    }

    #[test]
    fn planner_rejects_nonfinite_parented_and_wrong_transform_boundaries() {
        let (arena, root, mut properties, generations) = exact_transform_fixture();
        properties
            .transforms
            .get_mut(&TransformNodeId(root))
            .expect("root transform")
            .viewport_matrix = glam::Mat4::from_cols_array(&[
            f32::NAN,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ]);
        let nonfinite = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("nonfinite transform evidence must fail before recording");
        assert!(
            nonfinite
                .reasons
                .contains(&FramePaintPlanRejection::InvalidRootTransform(root))
        );

        let (arena, root, mut properties, generations) = exact_transform_fixture();
        let child = arena.get(root).expect("root").element.children()[0];
        properties
            .transforms
            .get_mut(&TransformNodeId(root))
            .expect("root transform")
            .parent = Some(TransformNodeId(child));
        let parented = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("frame-root transform must be parentless");
        assert!(
            parented
                .reasons
                .contains(&FramePaintPlanRejection::InvalidRootTransform(root))
        );
    }

    #[test]
    fn planner_rejects_deferred_unknown_missing_and_cyclic_inputs() {
        let (arena, _root, properties, generations) = exact_transform_fixture();
        let missing = NodeKey::null();
        assert_eq!(
            plan_single_root_transform_surface(
                &arena,
                &[missing],
                &FxHashSet::default(),
                &properties,
                &generations,
            )
            .expect_err("missing root must fail closed")
            .reasons,
            vec![FramePaintPlanRejection::MissingRoot(missing)]
        );

        let mut unknown_arena = new_test_arena();
        let unknown = commit_element(
            &mut unknown_arena,
            Box::new(UnknownHost {
                id: 0xc1_3000,
                width: 10.0,
                height: 10.0,
            }),
        );
        assert_eq!(
            plan_single_root_transform_surface(
                &unknown_arena,
                &[unknown],
                &FxHashSet::default(),
                &PropertyTrees::default(),
                &PaintGenerationTracker::default(),
            )
            .expect_err("only concrete Element may own the first transform surface")
            .reasons,
            vec![FramePaintPlanRejection::UnknownRootHost(unknown)]
        );

        let (arena, root, properties, generations) = exact_transform_fixture();
        let child = arena.get(root).expect("root").element.children()[0];
        let mut deferred_style = Style::new();
        deferred_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                crate::style::Position::absolute()
                    .left(crate::style::Length::px(0.0))
                    .clip(crate::style::ClipMode::Viewport),
            ),
        );
        crate::view::test_support::get_element_mut::<Element>(&arena, child)
            .apply_style(deferred_style);
        let deferred = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("deferred subtree changes frame ordering");
        assert!(
            deferred
                .reasons
                .contains(&FramePaintPlanRejection::DeferredBoundary(child))
        );

        let (arena, root, mut properties, generations) = exact_transform_fixture();
        properties.transforms.remove(&TransformNodeId(root));
        let missing_transform = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("root transform identity is mandatory");
        assert!(
            missing_transform
                .reasons
                .contains(&FramePaintPlanRejection::MissingRootTransform(root))
        );

        let (arena, root, mut properties, generations) = exact_transform_fixture();
        let child = arena.get(root).expect("root").element.children()[0];
        properties.states.remove(&child);
        let missing_state = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("every reachable owner requires a property snapshot");
        assert!(
            missing_state
                .reasons
                .contains(&FramePaintPlanRejection::MissingPropertyState(child))
        );

        let (mut arena, root, properties, generations) = exact_transform_fixture();
        let child = arena.get(root).expect("root").element.children()[0];
        arena.push_child(child, root);
        arena.set_parent(root, Some(child));
        let cycle = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("cycle cannot prove canonical owner topology");
        assert!(
            cycle
                .reasons
                .contains(&FramePaintPlanRejection::RootHasParent(root))
        );
        assert!(
            cycle
                .reasons
                .contains(&FramePaintPlanRejection::DuplicateNodeKey(root))
        );
    }

    #[test]
    fn whole_subtree_must_be_recordable_and_surface_artifact_store_is_strict() {
        let mut root = Element::new_with_id(0xc1_3100, 0.0, 0.0, 30.0, 20.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.set_transform(Transform::new([Rotate::z(Angle::deg(4.0))]));
        root.apply_style(style);
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(root));
        commit_child(
            &mut arena,
            root,
            Box::new(UnknownHost {
                id: 0xc1_3101,
                width: 8.0,
                height: 6.0,
            }),
        );
        measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 80.0,
                max_height: 60.0,
                viewport_width: 80.0,
                viewport_height: 60.0,
                percent_base_width: Some(80.0),
                percent_base_height: Some(60.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 80.0,
                available_height: 60.0,
                viewport_width: 80.0,
                viewport_height: 60.0,
                percent_base_width: Some(80.0),
                percent_base_height: Some(60.0),
            },
        );
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        let error = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("one unknown descendant makes the whole surface ineligible");
        assert!(error.reasons.contains(&FramePaintPlanRejection::Coverage(
            super::super::FrameArtifactFallbackReason::LegacyBoundary(
                super::super::LegacyPaintReason::UnknownHost,
            )
        )));
        {
            let root_node = arena.get(root).expect("root node");
            let root_element = root_node
                .element
                .as_any()
                .downcast_ref::<Element>()
                .expect("Element root");
            assert!(
                root_element
                    .exact_transform_surface_geometry_snapshot(&arena, [0.0, 0.0], None)
                    .is_none(),
                "custom descendants do not silently acquire exact retained bounds authority"
            );
            assert!(
                root_element
                    .transform_surface_geometry_snapshot(&arena, [0.0, 0.0], None)
                    .is_some(),
                "legacy compatibility bounds must remain available"
            );
        }
        let mut graph = FrameGraph::new();
        let ctx = UiBuildContext::new(80, 60, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        arena
            .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
            .expect("legacy custom descendant build");
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::render_pass::draw_rect_pass::DrawRectPass>()
                .len(),
            1,
            "exact rejection must not blank the legacy custom draw"
        );

        let (arena, root, properties, generations) = exact_transform_fixture();
        let plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("baseline surface plan");
        let PaintPlanStep::RetainedSurface(surface) = &plan.steps[0] else {
            panic!("one retained surface")
        };
        let validate = |artifact: &PaintArtifact| {
            super::super::compiler::validate_transform_surface_artifact_for_plan(
                artifact,
                root,
                TransformNodeId(root),
            )
        };

        let mut wrong_transform = only_span(surface).artifact.clone();
        wrong_transform.chunks[0].properties.transform = None;
        assert!(!validate(&wrong_transform));

        let mut wrong_topology = only_span(surface).artifact.clone();
        let child = arena.get(root).expect("root").element.children()[0];
        wrong_topology
            .owner_nodes
            .iter_mut()
            .find(|snapshot| snapshot.owner == root)
            .expect("root owner snapshot")
            .parent = Some(child);
        assert!(!validate(&wrong_topology));

        let mut wrong_payload = only_span(surface).artifact.clone();
        let PaintOp::DrawRect(rect) = &mut wrong_payload.ops[0] else {
            panic!("fixture begins with a decoration draw")
        };
        rect.params.opacity = 0.25;
        assert!(!validate(&wrong_payload));
    }

    #[test]
    fn planning_is_validation_only_and_does_not_compile_or_mutate_inputs() {
        let (arena, root, properties, generations) = exact_transform_fixture();
        let children_before = arena.children_of(root);
        let root_parent_before = arena.parent_of(root);
        let property_epoch_before = properties.epoch;
        let state_count_before = properties.states.len();
        let _ = super::super::take_artifact_compile_count();
        let _ = super::super::take_full_artifact_record_count();

        let plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("pure planning succeeds");
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(super::super::take_artifact_compile_count(), 0);
        assert!(super::super::take_full_artifact_record_count() > 0);
        assert_eq!(arena.children_of(root), children_before);
        assert_eq!(arena.parent_of(root), root_parent_before);
        assert_eq!(properties.epoch, property_epoch_before);
        assert_eq!(properties.states.len(), state_count_before);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn image_and_svg_inherited_transform_are_authorized_only_by_surface_recording() {
        const SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='16' height='12'><rect width='16' height='12' fill='#22c55e'/></svg>";
        let mut root = Element::new_with_id(0xc1_1000, 0.0, 0.0, 48.0, 32.0);
        let mut root_style = Style::new();
        root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        root_style.set_transform(Transform::new([Rotate::z(Angle::deg(8.0))]));
        root.apply_style(root_style);

        let mut image = Image::new_with_id(
            0xc1_1001,
            ImageSource::Rgba {
                width: 1,
                height: 1,
                pixels: Arc::from([255_u8, 255, 255, 255]),
            },
        );
        let mut media_style = Style::new();
        media_style.insert(
            PropertyId::Width,
            ParsedValue::Length(crate::style::Length::px(16.0)),
        );
        media_style.insert(
            PropertyId::Height,
            ParsedValue::Length(crate::style::Length::px(12.0)),
        );
        image.apply_style(media_style.clone());
        let mut svg = Svg::new_with_id(0xc1_1002, SvgSource::Content(SVG.into()));
        svg.apply_style(media_style);

        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(root));
        let image = commit_child(&mut arena, root, Box::new(image));
        let svg = commit_child(&mut arena, root, Box::new(svg));
        let constraints = LayoutConstraints {
            max_width: 160.0,
            max_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        };
        let placement = LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 160.0,
            available_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        };
        measure_and_place(&mut arena, root, constraints, placement);
        arena
            .get_mut(svg)
            .expect("svg")
            .element
            .as_any_mut()
            .downcast_mut::<Svg>()
            .expect("Svg host")
            .prepare_content_paint_for_test(SVG, (16.0, 12.0), 1.0)
            .expect("prepare exact SVG paint");

        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        let transform = TransformNodeId(root);
        for owner in [image, svg] {
            let property = properties.paint_state_for(owner).expect("property state");
            let local = generations
                .local_generations_for(owner)
                .expect("paint generations");
            let revision = super::super::PaintContentRevision {
                self_paint_revision: local.self_paint_revision,
                composite_revision: local.composite_revision,
                topology_revision: local.topology_revision,
            };
            let node = arena.get(owner).expect("media child");
            assert!(
                node.element
                    .record_shadow_paint_metadata(
                        owner,
                        property,
                        revision,
                        &arena,
                        super::super::PaintRecordingContext::default(),
                    )
                    .is_none(),
                "ordinary metadata path must reject inherited transform"
            );
            let context = super::super::PaintRecordingContext {
                recording_owner: Some(owner),
                recording_owner_stable_id: Some(node.element.stable_id()),
                transform_surface: Some(
                    PaintTransformSurfaceWitness::canonical_root(root).for_target(owner),
                ),
                ..Default::default()
            };
            assert!(context.authorizes_transform_surface_owner(Some(transform)));
            assert!(
                node.element
                    .record_shadow_paint_metadata(owner, property, revision, &arena, context,)
                    .is_some(),
                "surface-scoped canonical owner witness must admit inherited transform"
            );
        }

        let plan = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect("surface planner must own Image/SVG inherited transform");
        let PaintPlanStep::RetainedSurface(surface) = &plan.steps[0] else {
            panic!("one retained surface")
        };
        let roles = only_span(surface)
            .artifact
            .chunks
            .iter()
            .map(|chunk| chunk.id.role)
            .collect::<Vec<_>>();
        assert!(roles.contains(&super::super::PaintChunkRole::ImageContent));
        assert!(roles.contains(&super::super::PaintChunkRole::SvgContent));

        let mut graph = FrameGraph::new();
        assert!(
            super::super::try_compile_artifact(
                &only_span(surface).artifact,
                &mut graph,
                UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0),
            )
            .is_err()
        );
        assert!(graph.pass_descriptors().is_empty());

        let mut legacy_graph = FrameGraph::new();
        let (legacy_ctx, legacy_parent) =
            parent_context_with_clear(&mut legacy_graph, 160, 120, 1.0);
        arena
            .with_element_taken(root, |element, arena| {
                element.build(&mut legacy_graph, arena, legacy_ctx)
            })
            .expect("legacy Image/SVG transformed build");
        legacy_graph
            .add_texture_sink(
                &legacy_parent,
                crate::view::frame_graph::ExternalSinkKind::DebugCapture,
            )
            .expect("legacy Image/SVG sink");

        let mut forced_graph = FrameGraph::new();
        let (forced_ctx, forced_parent) =
            parent_context_with_clear(&mut forced_graph, 160, 120, 1.0);
        let mut viewport = Viewport::new();
        super::super::execute_forced_transform_surface_for_test(
            &mut viewport,
            &plan,
            &mut forced_graph,
            forced_ctx,
        )
        .expect("forced Image/SVG transformed build");
        forced_graph
            .add_texture_sink(
                &forced_parent,
                crate::view::frame_graph::ExternalSinkKind::DebugCapture,
            )
            .expect("forced Image/SVG sink");
        assert_eq!(
            forced_graph
                .test_compile_snapshot()
                .expect("strict forced Image/SVG snapshot"),
            legacy_graph
                .test_compile_snapshot()
                .expect("strict legacy Image/SVG snapshot")
        );

        arena
            .get_mut(image)
            .expect("image")
            .element
            .as_any_mut()
            .downcast_mut::<Image>()
            .expect("Image host")
            .set_layout_transition_width_for_test(17.0);
        arena
            .get_mut(svg)
            .expect("svg")
            .element
            .as_any_mut()
            .downcast_mut::<Svg>()
            .expect("Svg host")
            .set_layout_transition_width_for_test(17.0);
        let error = plan_single_root_transform_surface(
            &arena,
            &[root],
            &FxHashSet::default(),
            &properties,
            &generations,
        )
        .expect_err("wrapper-forwarded runtime layout state must fail closed");
        assert!(
            error
                .reasons
                .contains(&FramePaintPlanRejection::LayoutTransition(image))
        );
        assert!(
            error
                .reasons
                .contains(&FramePaintPlanRejection::LayoutTransition(svg))
        );
    }
}
