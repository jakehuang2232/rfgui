#![allow(dead_code)] // Planning-only M10C1 scaffold; production dispatch begins in C2.

use std::ops::Range;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::view::base_component::{
    Element, ElementTrait, RetainedNestedScrollSceneAdmissionSnapshot,
    RetainedScrollForestHostAdmissionSnapshot, RetainedScrollHostAdmissionSnapshot,
    TransformSurfaceGeometrySnapshot, is_exact_retained_scroll_forest_content_node,
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
    /// Arbitrary-depth/multi-root native scroll forest.  This remains a
    /// graph-inert sibling of the fixed two-boundary oracle until its joint
    /// compiler/pool transaction is materialized.
    native_scroll_forest_scaffold: Option<NativeScrollForestScaffold>,
}

/// Dense forest-local scroll boundary identity.  Ordinals are assigned in
/// scene-root DFS order and are never derived from generational `NodeKey`
/// storage details.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct NativeScrollBoundaryId(pub(super) u32);

#[derive(Clone, Debug)]
pub(super) struct NativeScrollForestScaffold {
    pub(super) context: TransformSurfacePlanContext,
    pub(super) scale_factor_bits: u32,
    pub(super) roots: Vec<NativeScrollForestRoot>,
    pub(super) boundaries: Vec<NativeScrollForestBoundaryContract>,
    pub(super) schedule: NativeScrollForestSchedule,
    pub(super) programs: Vec<NativeScrollForestBoundaryProgram>,
    planned_context: TransformSurfacePlanContext,
    planned_scale_factor_bits: u32,
    planned_roots: Vec<NativeScrollForestRoot>,
    planned_boundaries: Vec<NativeScrollForestBoundaryContract>,
    planned_schedule: NativeScrollForestSchedule,
    planned_programs: Vec<NativeScrollForestBoundaryProgram>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NativeScrollForestRoot {
    pub(super) ordinal: u32,
    pub(super) root: NodeKey,
    pub(super) stable_id: u64,
    pub(super) boundary_span: Range<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NativeScrollProjectionWitness {
    pub(super) live_input: PropertyTreeState,
    pub(super) projected_output: PropertyTreeState,
    pub(super) parent_scroll: Option<ScrollNodeId>,
    pub(super) parent_clip: Option<ClipNodeId>,
}

#[derive(Clone, Debug)]
pub(super) struct NativeScrollForestBoundaryContract {
    pub(super) id: NativeScrollBoundaryId,
    pub(super) scene_root_ordinal: u32,
    pub(super) boundary_root: NodeKey,
    pub(super) stable_id: u64,
    pub(super) parent: Option<NativeScrollBoundaryId>,
    pub(super) admission: RetainedScrollForestHostAdmissionSnapshot,
    pub(super) scroll: ScrollNodeSnapshot,
    pub(super) contents_clip: ClipNodeSnapshot,
    pub(super) projection: NativeScrollProjectionWitness,
}

impl PartialEq for NativeScrollForestBoundaryContract {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.scene_root_ordinal == other.scene_root_ordinal
            && self.boundary_root == other.boundary_root
            && self.stable_id == other.stable_id
            && self.parent == other.parent
            && self.admission.bitwise_eq(other.admission)
            && self.scroll == other.scroll
            && self.contents_clip == other.contents_clip
            && self.projection == other.projection
    }
}

impl Eq for NativeScrollForestBoundaryContract {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NativeScrollArtifactPhase {
    HostBefore,
    OverlayAfter,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NativeScrollForestSchedule {
    pub(super) steps: Vec<NativeScrollForestScheduledStep>,
}

/// Structural DFS program. `Artifact` freezes the two host artifact slots;
/// the compiler phase will replace each slot with a sealed artifact identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NativeScrollForestScheduledStep {
    Artifact {
        boundary: NativeScrollBoundaryId,
        phase: NativeScrollArtifactPhase,
    },
    ChildBoundary {
        parent: Option<NativeScrollBoundaryId>,
        child: NativeScrollBoundaryId,
    },
    ContentReceiver {
        boundary: NativeScrollBoundaryId,
        content_root: NodeKey,
        stable_id: u64,
        projection: NativeScrollProjectionWitness,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NativeScrollForestBoundaryProgram {
    pub(super) boundary: NativeScrollBoundaryId,
    pub(super) receiver_stable_id: u64,
    pub(super) edge: super::PaintScrollForestEdgeWitness,
    pub(super) host_before: NestedScrollArtifactSeal,
    pub(super) content_steps: Vec<NativeScrollForestContentProgramStep>,
    pub(super) overlay_after: NestedScrollArtifactSeal,
    pub(super) compiler_stamp: super::compiler::NativeScrollForestCompilerStamp,
    pub(super) child_dependencies: Vec<NativeScrollForestChildRasterDependency>,
    pub(super) content_program_opaque_terminal: u32,
}

/// Exact raster dependency contributed by one child scroll boundary to its
/// parent's offset-zero content program. Child content remains a separate
/// resident, while H/O, final composite geometry and the live scroll edge are
/// parent-raster identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeScrollForestChildRasterDependency {
    pub(super) child: NativeScrollBoundaryId,
    pub(super) boundary_root: NodeKey,
    pub(super) content_root: NodeKey,
    pub(super) content_stable_id: u64,
    pub(super) child_raster_identity: Box<NativeScrollForestContentRasterProgramIdentity>,
    pub(super) host_identity: PropertyScrollReceiverArtifactIdentity,
    pub(super) overlay_identity: PropertyScrollReceiverArtifactIdentity,
    pub(super) scroll: ScrollNodeSnapshot,
    pub(super) contents_clip: ClipNodeSnapshot,
    pub(super) source_bounds_bits: [u32; 4],
    pub(super) offset_bits: [u32; 2],
    pub(super) composite_scissor: [u32; 4],
    pub(super) parent_opaque_before: u32,
    pub(super) parent_opaque_after: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeScrollForestContentRasterProgramIdentity {
    pub(super) content_root: NodeKey,
    pub(super) content_stable_id: u64,
    pub(super) artifact_span: super::compiler::RetainedSurfaceArtifactSpanStamp,
    pub(super) opaque_terminal: u32,
    pub(super) child_dependencies: Vec<NativeScrollForestChildRasterDependency>,
}

impl NativeScrollForestBoundaryProgram {
    fn content_raster_identity(
        &self,
        content_root: NodeKey,
    ) -> NativeScrollForestContentRasterProgramIdentity {
        NativeScrollForestContentRasterProgramIdentity {
            content_root,
            content_stable_id: self.receiver_stable_id,
            artifact_span: self.compiler_stamp.content_artifact_span.clone(),
            opaque_terminal: self.content_program_opaque_terminal,
            child_dependencies: self.child_dependencies.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NativeScrollForestContentProgramStep {
    Artifact(NestedScrollArtifactSeal),
    ChildBoundary(NativeScrollBoundaryId),
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
    pub(super) same_owner_transform_scroll_insertions:
        Vec<PropertySameOwnerTransformScrollReceiverInsertionContract>,
    pub(super) same_owner_effect_scroll_insertions:
        Vec<PropertySameOwnerEffectScrollReceiverInsertionContract>,
    pub(super) frame_receiver_insertions: Vec<PropertyFrameScrollReceiverInsertionContract>,
    pub(super) effect_receiver_insertions: Vec<PropertyEffectScrollReceiverInsertionContract>,
    pub(super) transform_effect_receiver_insertions:
        Vec<PropertyTransformEffectScrollReceiverInsertionContract>,
    pub(super) effect_transform_receiver_insertions:
        Vec<PropertyEffectTransformScrollReceiverInsertionContract>,
    pub(super) scroll_content_effect_insertions: Vec<PropertyScrollContentEffectInsertionContract>,
    pub(super) boundary_dag: PropertyBoundaryDag,
    planned_context: TransformSurfacePlanContext,
    planned_roots: Vec<PropertyScrollScheduleRoot>,
    planned_schedule: PropertySceneSchedule,
    planned_boundaries: Vec<PropertyScrollBoundaryContract>,
    planned_receiver_insertions: Vec<PropertyScrollReceiverInsertionContract>,
    planned_same_owner_transform_scroll_insertions:
        Vec<PropertySameOwnerTransformScrollReceiverInsertionContract>,
    planned_same_owner_effect_scroll_insertions:
        Vec<PropertySameOwnerEffectScrollReceiverInsertionContract>,
    planned_frame_receiver_insertions: Vec<PropertyFrameScrollReceiverInsertionContract>,
    planned_effect_receiver_insertions: Vec<PropertyEffectScrollReceiverInsertionContract>,
    planned_transform_effect_receiver_insertions:
        Vec<PropertyTransformEffectScrollReceiverInsertionContract>,
    planned_effect_transform_receiver_insertions:
        Vec<PropertyEffectTransformScrollReceiverInsertionContract>,
    planned_scroll_content_effect_insertions: Vec<PropertyScrollContentEffectInsertionContract>,
    planned_boundary_dag: PropertyBoundaryDag,
}

/// Typed projection of the already-admitted property/scroll path grammar.
///
/// Unlike `PropertySceneSchedule`, receiver scope is explicit: a boundary may
/// target the frame, another retained surface, or (for a later grammar slice)
/// a scroll boundary's detached-content scope. Phase 0 projects only the
/// existing S, T->S, E->S and T->E->S shapes; `ScrollContent` is sealed now so
/// adding a post-scroll property later cannot silently reuse `FrameRoot`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropertyBoundaryDag {
    pub(super) roots: Vec<PropertyBoundaryDagRoot>,
    pub(super) nodes: Vec<PropertyBoundaryDagNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PropertyBoundaryDagGrammar {
    FrameRootScroll,
    TransformScroll,
    EffectScroll,
    TransformEffectScroll,
    EffectTransformScroll,
    ScrollEffect,
    TransformScrollEffect,
}

impl PropertyBoundaryDag {
    /// Classifies only the four grammars already executable before the DAG
    /// projection. Empty roots are accepted solely as siblings of the
    /// frame-root S grammar; every surface grammar remains exact and
    /// homogeneous across the forest.
    pub(super) fn existing_grammar(&self) -> Option<PropertyBoundaryDagGrammar> {
        let mut grammar = None;
        let mut saw_empty_root = false;
        for root in &self.roots {
            let nodes = self.nodes.get(root.node_span.clone())?;
            let root_grammar = match nodes {
                [] => {
                    saw_empty_root = true;
                    continue;
                }
                [
                    PropertyBoundaryDagNode {
                        receiver: PropertyBoundaryReceiverScope::FrameRoot { .. },
                        kind: PropertyBoundaryDagNodeKind::Scroll(_),
                        ..
                    },
                ] => PropertyBoundaryDagGrammar::FrameRootScroll,
                [
                    PropertyBoundaryDagNode {
                        id: outer_id,
                        receiver: PropertyBoundaryReceiverScope::FrameRoot { .. },
                        kind: PropertyBoundaryDagNodeKind::Transform(_),
                        ..
                    },
                    PropertyBoundaryDagNode {
                        receiver: PropertyBoundaryReceiverScope::Surface(receiver),
                        kind: PropertyBoundaryDagNodeKind::Scroll(_),
                        ..
                    },
                ] if receiver == outer_id => PropertyBoundaryDagGrammar::TransformScroll,
                [
                    PropertyBoundaryDagNode {
                        id: outer_id,
                        receiver: PropertyBoundaryReceiverScope::FrameRoot { .. },
                        kind: PropertyBoundaryDagNodeKind::Effect(_),
                        ..
                    },
                    PropertyBoundaryDagNode {
                        receiver: PropertyBoundaryReceiverScope::Surface(receiver),
                        kind: PropertyBoundaryDagNodeKind::Scroll(_),
                        ..
                    },
                ] if receiver == outer_id => PropertyBoundaryDagGrammar::EffectScroll,
                [
                    PropertyBoundaryDagNode {
                        id: transform_id,
                        receiver: PropertyBoundaryReceiverScope::FrameRoot { .. },
                        kind: PropertyBoundaryDagNodeKind::Transform(_),
                        ..
                    },
                    PropertyBoundaryDagNode {
                        id: effect_id,
                        receiver: PropertyBoundaryReceiverScope::Surface(effect_receiver),
                        kind: PropertyBoundaryDagNodeKind::Effect(_),
                        ..
                    },
                    PropertyBoundaryDagNode {
                        receiver: PropertyBoundaryReceiverScope::Surface(scroll_receiver),
                        kind: PropertyBoundaryDagNodeKind::Scroll(_),
                        ..
                    },
                ] if effect_receiver == transform_id && scroll_receiver == effect_id => {
                    PropertyBoundaryDagGrammar::TransformEffectScroll
                }
                [
                    PropertyBoundaryDagNode {
                        id: effect_id,
                        receiver: PropertyBoundaryReceiverScope::FrameRoot { .. },
                        kind: PropertyBoundaryDagNodeKind::Effect(_),
                        ..
                    },
                    PropertyBoundaryDagNode {
                        id: transform_id,
                        receiver: PropertyBoundaryReceiverScope::Surface(transform_receiver),
                        kind: PropertyBoundaryDagNodeKind::Transform(_),
                        ..
                    },
                    PropertyBoundaryDagNode {
                        receiver: PropertyBoundaryReceiverScope::Surface(scroll_receiver),
                        kind: PropertyBoundaryDagNodeKind::Scroll(_),
                        ..
                    },
                ] if transform_receiver == effect_id && scroll_receiver == transform_id => {
                    PropertyBoundaryDagGrammar::EffectTransformScroll
                }
                [
                    PropertyBoundaryDagNode {
                        id: scroll_id,
                        receiver: PropertyBoundaryReceiverScope::FrameRoot { .. },
                        kind: PropertyBoundaryDagNodeKind::Scroll(_),
                        ..
                    },
                    PropertyBoundaryDagNode {
                        receiver: PropertyBoundaryReceiverScope::ScrollContent(receiver),
                        kind: PropertyBoundaryDagNodeKind::Effect(_),
                        ..
                    },
                ] if receiver == scroll_id => PropertyBoundaryDagGrammar::ScrollEffect,
                [
                    PropertyBoundaryDagNode {
                        id: transform_id,
                        receiver: PropertyBoundaryReceiverScope::FrameRoot { .. },
                        kind: PropertyBoundaryDagNodeKind::Transform(_),
                        ..
                    },
                    PropertyBoundaryDagNode {
                        id: scroll_id,
                        receiver: PropertyBoundaryReceiverScope::Surface(receiver),
                        kind: PropertyBoundaryDagNodeKind::Scroll(_),
                        ..
                    },
                    PropertyBoundaryDagNode {
                        receiver: PropertyBoundaryReceiverScope::ScrollContent(content_receiver),
                        kind: PropertyBoundaryDagNodeKind::Effect(_),
                        ..
                    },
                ] if receiver == transform_id && content_receiver == scroll_id => {
                    PropertyBoundaryDagGrammar::TransformScrollEffect
                }
                _ => return None,
            };
            if grammar
                .replace(root_grammar)
                .is_some_and(|seen| seen != root_grammar)
            {
                return None;
            }
        }
        let grammar = grammar?;
        if saw_empty_root && grammar != PropertyBoundaryDagGrammar::FrameRootScroll {
            return None;
        }
        Some(grammar)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropertyBoundaryDagRoot {
    pub(super) scene_root_ordinal: u32,
    pub(super) root: NodeKey,
    pub(super) stable_id: u64,
    pub(super) node_span: Range<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct PropertyBoundaryDagNodeId(pub(super) u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PropertyBoundaryReceiverScope {
    FrameRoot { scene_root_ordinal: u32 },
    Surface(PropertyBoundaryDagNodeId),
    ScrollContent(PropertyBoundaryDagNodeId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PropertyBoundaryDagNodeKind {
    Transform(TransformNodeSnapshot),
    Effect(EffectNodeSnapshot),
    Scroll(PropertyScrollBoundaryContract),
}

impl PropertyBoundaryDagNodeKind {
    fn owner(&self) -> NodeKey {
        match self {
            Self::Transform(snapshot) => snapshot.owner,
            Self::Effect(snapshot) => snapshot.owner,
            Self::Scroll(boundary) => boundary.scroll.owner,
        }
    }

    fn planned_boundary(&self) -> super::PlannedBoundaryKind {
        match self {
            Self::Transform(snapshot) => super::PlannedBoundaryKind::Transform(snapshot.id),
            Self::Effect(snapshot) => super::PlannedBoundaryKind::Isolation(snapshot.id),
            Self::Scroll(boundary) => super::PlannedBoundaryKind::Scroll(boundary.scroll.id),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropertyBoundaryDagNode {
    pub(super) id: PropertyBoundaryDagNodeId,
    pub(super) scene_root_ordinal: u32,
    pub(super) owner: NodeKey,
    pub(super) stable_id: u64,
    pub(super) receiver: PropertyBoundaryReceiverScope,
    pub(super) kind: PropertyBoundaryDagNodeKind,
    pub(super) consumption: ConsumedPropertyEntry,
    pub(super) placement: PropertyBoundaryDagPlacement,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PropertyBoundaryDagPlacement {
    Root,
    Cutout {
        marker: super::PlannedBoundary,
        neutral_path: Vec<PropertyBoundaryPathOwnerWitness>,
        sealed: Option<PropertyBoundaryInsertionSeal>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PropertyBoundaryPathOwnerWitness {
    pub(super) owner: NodeKey,
    pub(super) stable_id: u64,
}

/// Shape-neutral receiver insertion identity. The optional seal preserves the
/// existing planning-only lifecycle: T->S/E->S may be structurally admitted
/// before their recorder payload is ready, while production compilation still
/// requires the seal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropertyBoundaryInsertionSeal {
    pub(super) insertion_index: usize,
    pub(super) before_span: Range<usize>,
    pub(super) after_span: Range<usize>,
    pub(super) receiver_opaque_before: u32,
    pub(super) receiver_opaque_after: u32,
    recorded_steps: Vec<PropertyScrollReceiverRecordedStepIdentity>,
}

/// Exact Phase3 descendant effect insertion inside one detached ScrollContent
/// receiver. The C program and E program are sealed independently so neither
/// scroll offset nor final scrollbar overlay paint can enter E raster identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropertyScrollContentEffectInsertionContract {
    pub(super) scene_root_ordinal: u32,
    pub(super) scroll_boundary_ordinal: u32,
    pub(super) content_root: NodeKey,
    pub(super) content_stable_id: u64,
    pub(super) effect: EffectNodeSnapshot,
    pub(super) effect_stable_id: u64,
    pub(super) effect_cutout: super::PlannedBoundary,
    pub(super) insertion_index: usize,
    pub(super) before_span: Range<usize>,
    pub(super) after_span: Range<usize>,
    pub(super) receiver_opaque_before: u32,
    pub(super) receiver_opaque_after: u32,
    pub(super) effect_raster_bounds_bits: [u32; 4],
    pub(super) artifact_contract: EffectPropertySurfaceArtifactContract,
    pub(super) consumed_transform: Option<ConsumedAncestorTransformWitness>,
    pub(super) outer_transform: Option<PropertyScrollContentOuterTransformInsertionContract>,
    receiver_recorded_steps: Vec<PropertyScrollReceiverRecordedStepIdentity>,
    effect_recorded_steps: Vec<PropertyScrollReceiverArtifactIdentity>,
}

#[derive(Clone, Debug)]
pub(super) struct PropertyScrollContentOuterTransformInsertionContract {
    pub(super) receiver: PropertyScrollReceiverInsertionContract,
    pub(super) geometry: TransformSurfaceGeometrySnapshot,
}

impl PartialEq for PropertyScrollContentOuterTransformInsertionContract {
    fn eq(&self, other: &Self) -> bool {
        self.receiver == other.receiver && self.geometry.bitwise_eq(other.geometry)
    }
}

impl Eq for PropertyScrollContentOuterTransformInsertionContract {}

/// Exact Phase2 `Effect -> Transform -> ScrollContents` receiver pair. The
/// inner T raster consumes E, then the outer E recorder owns the typed T
/// cutout and applies opacity only at final composition.
#[derive(Clone, Debug)]
pub(super) struct PropertyEffectTransformScrollReceiverInsertionContract {
    pub(super) scene_root_ordinal: u32,
    pub(super) outer_receiver: EffectNodeSnapshot,
    pub(super) outer_stable_id: u64,
    pub(super) outer_artifact_contract: EffectPropertySurfaceArtifactContract,
    pub(super) outer_raster_bounds_bits: [u32; 4],
    pub(super) transform_cutout: super::PlannedBoundary,
    pub(super) outer_insertion_index: usize,
    pub(super) outer_before_span: Range<usize>,
    pub(super) outer_after_span: Range<usize>,
    pub(super) outer_opaque_before: u32,
    pub(super) outer_opaque_after: u32,
    pub(super) inner_geometry: TransformSurfaceGeometrySnapshot,
    pub(super) inner: PropertyScrollReceiverInsertionContract,
    outer_recorded_steps: Vec<PropertyScrollReceiverRecordedStepIdentity>,
}

impl PartialEq for PropertyEffectTransformScrollReceiverInsertionContract {
    fn eq(&self, other: &Self) -> bool {
        self.scene_root_ordinal == other.scene_root_ordinal
            && self.outer_receiver == other.outer_receiver
            && self.outer_stable_id == other.outer_stable_id
            && self.outer_artifact_contract == other.outer_artifact_contract
            && self.outer_raster_bounds_bits == other.outer_raster_bounds_bits
            && self.transform_cutout == other.transform_cutout
            && self.outer_insertion_index == other.outer_insertion_index
            && self.outer_before_span == other.outer_before_span
            && self.outer_after_span == other.outer_after_span
            && self.outer_opaque_before == other.outer_opaque_before
            && self.outer_opaque_after == other.outer_opaque_after
            && self.inner_geometry.bitwise_eq(other.inner_geometry)
            && self.inner == other.inner
            && self.outer_recorded_steps == other.outer_recorded_steps
    }
}

impl Eq for PropertyEffectTransformScrollReceiverInsertionContract {}

/// Exact scene-root receiver schedule around one descendant ScrollContents
/// cutout. Unlike the transform/effect siblings this receiver stays on the
/// frame target; the ordered artifacts retain root decoration and child-mask
/// begin/end ownership around the detached content composite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropertyFrameScrollReceiverInsertionContract {
    pub(super) scene_root_ordinal: u32,
    pub(super) receiver_root: NodeKey,
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

/// Sealed self-role authority for one native owner that owns both T and S.
/// The outer transform receiver intentionally contains only the typed scroll
/// insertion. All owner self paint belongs to S host/overlay recording, while
/// descendants belong to the offset-zero C recorder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropertySameOwnerTransformScrollReceiverInsertionContract {
    pub(super) receiver: PropertyScrollReceiverInsertionContract,
    pub(super) owner: NodeKey,
    pub(super) stable_id: u64,
    pub(super) transform: TransformNodeSnapshot,
    pub(super) scroll: ScrollNodeSnapshot,
    pub(super) contents_clip: ClipNodeSnapshot,
    pub(super) content_root: NodeKey,
    pub(super) content_stable_id: u64,
}

impl PropertySameOwnerTransformScrollReceiverInsertionContract {
    pub(super) fn is_canonical(&self) -> bool {
        self.owner == self.receiver.receiver.owner
            && self.owner == self.receiver.receiver.id.0
            && self.owner == self.transform.owner
            && self.owner == self.transform.id.0
            && self.owner == self.scroll.owner
            && self.owner == self.scroll.id.0
            && self.owner == self.contents_clip.owner
            && self.owner == self.contents_clip.id.owner
            && self.stable_id != 0
            && self.stable_id == self.receiver.receiver_stable_id
            && self.transform == self.receiver.receiver
            && self.scroll.id
                == match self.receiver.scroll_cutout.kind {
                    super::PlannedBoundaryKind::Scroll(id) => id,
                    _ => return false,
                }
            && self.receiver.scroll_cutout.root == self.owner
            && self.receiver.scroll_cutout.stable_id == self.stable_id
            && self.contents_clip.id.role == ClipNodeRole::ContentsClip
            && self.contents_clip.generation != 0
            && self.content_root != self.owner
            && self.content_stable_id != 0
            && self.receiver.insertion_index == 0
            && self.receiver.before_span == (0..0)
            && self.receiver.after_span == (1..1)
            && self.receiver.receiver_opaque_before == 0
            && self.receiver.receiver_opaque_after == 0
            && self.receiver.recorded_steps
                == [PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(
                    self.receiver.scroll_cutout,
                )]
    }
}

/// Sealed self-role authority for one native owner that owns both E and S.
/// The receiver program contains only the scroll insertion; H/C/O raster is
/// assembled effect-neutral, then the owning opacity is applied once at the
/// final E composite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PropertySameOwnerEffectScrollReceiverInsertionContract {
    pub(super) receiver: PropertyEffectScrollReceiverInsertionContract,
    pub(super) owner: NodeKey,
    pub(super) stable_id: u64,
    pub(super) effect: EffectNodeSnapshot,
    pub(super) scroll: ScrollNodeSnapshot,
    pub(super) contents_clip: ClipNodeSnapshot,
    pub(super) content_root: NodeKey,
    pub(super) content_stable_id: u64,
}

impl PropertySameOwnerEffectScrollReceiverInsertionContract {
    pub(super) fn is_canonical(&self) -> bool {
        self.owner == self.receiver.receiver.owner
            && self.owner == self.receiver.receiver.id.0
            && self.owner == self.effect.owner
            && self.owner == self.effect.id.0
            && self.owner == self.scroll.owner
            && self.owner == self.scroll.id.0
            && self.owner == self.contents_clip.owner
            && self.owner == self.contents_clip.id.owner
            && self.stable_id != 0
            && self.stable_id == self.receiver.receiver_stable_id
            && self.effect == self.receiver.receiver
            && self.scroll.id
                == match self.receiver.scroll_cutout.kind {
                    super::PlannedBoundaryKind::Scroll(id) => id,
                    _ => return false,
                }
            && self.receiver.scroll_cutout.root == self.owner
            && self.receiver.scroll_cutout.stable_id == self.stable_id
            && self.contents_clip.id.role == ClipNodeRole::ContentsClip
            && self.contents_clip.generation != 0
            && self.content_root != self.owner
            && self.content_stable_id != 0
            && self.receiver.insertion_index == 0
            && self.receiver.before_span == (0..0)
            && self.receiver.after_span == (1..1)
            && self.receiver.receiver_opaque_before == 0
            && self.receiver.receiver_opaque_after == 0
            && self.receiver.raster_bounds_bits
                == [
                    self.scroll.viewport.x.to_bits(),
                    self.scroll.viewport.y.to_bits(),
                    self.scroll.viewport.width.to_bits(),
                    self.scroll.viewport.height.to_bits(),
                ]
            && self.receiver.recorded_steps
                == [PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(
                    self.receiver.scroll_cutout,
                )]
    }
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

impl PropertyFrameScrollReceiverInsertionContract {
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

impl PropertyEffectTransformScrollReceiverInsertionContract {
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
    /// A property surface rastered and composited inside one detached scroll
    /// content target. Keeping this distinct from `RetainedSurface` prevents
    /// a post-scroll property from being reinterpreted as a frame receiver.
    ScrollContentSurface {
        boundary: PropertyScheduledSurfaceBoundary,
        scroll: ScrollNodeId,
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
    /// Complete leaf-to-root chain above `contents_clip`. These clips stay
    /// on the final composite and are never baked into detached content.
    pub(super) ancestor_composite_clips: Vec<ClipNodeSnapshot>,
    /// Exact clips owned by the detached scroll-content subtree (for example
    /// TextArea's local viewport clip). These remain raster-local.
    pub(super) local_content_clips: Vec<ClipNodeSnapshot>,
    /// Exact clips owned by the surrounding scene-root receiver, including
    /// AnchorParent child-mask handles on either side of the scroll cutout.
    pub(super) receiver_clips: Vec<ClipNodeSnapshot>,
    pub(super) basis: ScrollCompositeBasis,
    pub(super) phase: PropertyScrollPhaseSchedule,
    pub(super) consumed_properties: ConsumedPropertyStack,
}

impl PropertyScrollBoundaryContract {
    /// Self-contained seal for consumers that retain a boundary after the
    /// schedule scaffold has been consumed. This mirrors the boundary-local
    /// portion of `property_scroll_schedule_scaffold_is_canonical` so a later
    /// prepare step never has to trust planner-only clip/scroll geometry.
    pub(super) fn is_canonical(&self) -> bool {
        if self.scroll.owner != self.scroll.id.0
            || self.scroll.parent.is_some()
            || self.scroll.generation == 0
            || self.contents_clip.id.owner != self.scroll.owner
            || self.contents_clip.id.role != ClipNodeRole::ContentsClip
            || self.contents_clip.owner != self.scroll.owner
            || self.contents_clip.behavior != ClipBehavior::Intersect
            || self.contents_clip.generation == 0
            || !self
                .scroll
                .is_canonical_with_ancestor_contents_clip(self.contents_clip)
        {
            return false;
        }

        let mut expected_parent = self.contents_clip.parent;
        for ancestor in &self.ancestor_composite_clips {
            if Some(ancestor.id) != expected_parent
                || ancestor.owner != ancestor.id.owner
                || ancestor.generation == 0
            {
                return false;
            }
            expected_parent = ancestor.parent;
        }
        if expected_parent.is_some() {
            return false;
        }

        let mut clip_ids = FxHashSet::default();
        if !clip_ids.insert(self.contents_clip.id) {
            return false;
        }
        for clip in self
            .ancestor_composite_clips
            .iter()
            .chain(&self.local_content_clips)
            .chain(&self.receiver_clips)
        {
            if !clip_ids.insert(clip.id)
                || clip.owner != clip.id.owner
                || clip.generation == 0
                || clip
                    .parent
                    .is_some_and(|parent| !clip_ids.contains(&parent))
            {
                return false;
            }
        }

        let stack = &self.consumed_properties;
        if stack.target_owner != self.scroll.owner
            || stack.entries.is_empty()
            || stack.entries.first().map(|entry| entry.expected_before) != Some(stack.live_input)
            || stack.entries.last().map(|entry| entry.projected_after)
                != Some(stack.projected_output)
            || stack.projected_output.clip != self.contents_clip.parent
            || stack.projected_output.transform.is_some()
            || stack.projected_output.effect.is_some()
            || stack.projected_output.scroll.is_some()
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
                    && scroll == self.scroll.id
                    && contents_clip == self.contents_clip.id =>
                {
                    cursor.scroll = None;
                    cursor.clip = self.contents_clip.parent;
                }
                _ => return false,
            }
            if entry.projected_after != cursor {
                return false;
            }
        }
        cursor == stack.projected_output
            && self.phase.host_before
                == (PropertyScrollPhaseSlot {
                    owner: self.scroll.owner,
                    phase: PropertyScrollPhaseKind::HostBeforeChildren,
                    receiver_state: cursor,
                })
            && self.phase.content_gap
                == (PropertyScrollContentPhase {
                    owner: self.scroll.owner,
                    phase: PropertyScrollPhaseKind::DetachedContentComposite,
                    content_state: stack.live_input,
                    projected_receiver_state: cursor,
                })
            && self.phase.overlay_after
                == (PropertyScrollPhaseSlot {
                    owner: self.scroll.owner,
                    phase: PropertyScrollPhaseKind::OverlayAfterChildren,
                    receiver_state: cursor,
                })
    }
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
    production_root_step_schedule: Option<Vec<Vec<PropertyEffectRootStepKind>>>,
    planned_roots: Vec<PropertyEffectRootWitness>,
    planned_surfaces: Vec<PropertyEffectSurfaceContract>,
    planned_clip_forest: PropertyEffectClipForestContract,
    planned_production_root_step_spans: Option<Vec<Range<usize>>>,
    planned_production_root_step_schedule: Option<Vec<Vec<PropertyEffectRootStepKind>>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PropertyEffectRootStepKind {
    NormalArtifact,
    LateBoundary(PropertyBoundaryId),
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
    pub(super) source_bounds: crate::view::base_component::RetainedSurfaceBounds,
    pub(super) logical_size: [f32; 2],
    pub(super) outer_scissor_rect: Option<[u32; 4]>,
}

/// Child-local raster and composite geometry for the one exact mixed tree
/// shape. Unlike the root isolation geometry, this is never viewport-sized:
/// it owns the direct child's exact retained render output verbatim.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NestedIsolationSurfaceGeometrySnapshot {
    pub(super) source_bounds: crate::view::base_component::RetainedSurfaceBounds,
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
        if seal.scroll_schedule_scaffold.is_some()
            || seal.nested_scroll_scaffold.is_some()
            || seal.native_scroll_forest_scaffold.is_some()
        {
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
            .map(|surface| {
                let boundary = match surface.kind {
                    PropertySceneTransactionSurfaceKind::Transform(id) => {
                        PropertyBoundaryId::Transform(id)
                    }
                    PropertySceneTransactionSurfaceKind::Effect(id) => {
                        PropertyBoundaryId::Effect(id)
                    }
                };
                (boundary, surface.ordinal)
            })
            .collect::<FxHashMap<_, _>>();
        let mut top_level_surfaces = Vec::new();
        for root in &roots {
            for step_index in root.top_level_step_span.clone() {
                let PaintPlanStep::RetainedSurface(surface) = &self.steps[step_index] else {
                    continue;
                };
                let boundary = match surface.kind() {
                    SurfaceKind::Transform(plan) => PropertyBoundaryId::Transform(plan.transform),
                    SurfaceKind::Isolation(plan) => PropertyBoundaryId::Effect(plan.effect.id),
                    SurfaceKind::NestedIsolation(plan) => {
                        PropertyBoundaryId::Effect(plan.effect.id)
                    }
                    SurfaceKind::ScrollHost(_) => return None,
                };
                top_level_surfaces.push(PropertySceneTopLevelSurfaceWitness {
                    step_index,
                    surface_ordinal: *surface_ordinals.get(&boundary)?,
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
                        && seal.native_scroll_forest_scaffold.is_none()
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

    /// Graph-inert arbitrary-depth native scroll forest seal.
    pub(super) fn native_scroll_forest_planning_scaffold(
        &self,
    ) -> Option<&NativeScrollForestScaffold> {
        property_scene_plan_is_sealed(self).then_some(())?;
        self.property_scene_seal
            .as_ref()?
            .native_scroll_forest_scaffold
            .as_ref()
    }

    #[cfg(test)]
    pub(crate) fn tamper_native_scroll_forest_prepare_seal_for_test(&mut self, kind: &str) {
        let scaffold = self
            .property_scene_seal
            .as_mut()
            .and_then(|seal| seal.native_scroll_forest_scaffold.as_mut())
            .expect("test plan owns a native scroll forest");
        match kind {
            "stamp" => {
                scaffold.programs[1].child_dependencies[0].composite_scissor[0] += 1;
                scaffold.planned_programs = scaffold.programs.clone();
            }
            "descriptor" => {
                scaffold.programs[0].receiver_stable_id ^= 0x1000;
                scaffold.planned_programs = scaffold.programs.clone();
            }
            "geometry" => {
                scaffold.boundaries[1].contents_clip.logical_scissor[0] += 1;
                scaffold.planned_boundaries = scaffold.boundaries.clone();
            }
            _ => panic!("unknown native forest prepare tamper"),
        }
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

    pub(super) fn source_bounds(&self) -> crate::view::base_component::RetainedSurfaceBounds {
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
        source_bounds: crate::view::base_component::RetainedSurfaceBounds,
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
            source_bounds: crate::view::base_component::RetainedSurfaceBounds {
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
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    context: TransformSurfacePlanContext,
) -> Result<FramePaintPlan, FramePaintPlanError> {
    let (reachable, index) = validate_transform_property_scene_inputs(
        arena,
        roots,
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
            native_scroll_forest_scaffold: None,
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
        let exact_deferred_root_effect = (|| {
            let logical_scissor = node
                .element
                .exact_retained_deferred_viewport_self_clip_scissor_rect(key, arena)?;
            let clip_id = ClipNodeId {
                owner: key,
                role: ClipNodeRole::SelfClip,
            };
            let effect_id = EffectNodeId(key);
            let exact_state = PropertyTreeState {
                clip: Some(clip_id),
                effect: Some(effect_id),
                ..Default::default()
            };
            let state = property_trees.node_state_for(key)?;
            let clip = property_trees.clips.get(&clip_id)?;
            let effect = property_trees.effects.get(&effect_id)?;
            (state.paint == exact_state
                && state.descendants == exact_state
                && clip.owner == key
                && clip.parent.is_none()
                && clip.behavior == ClipBehavior::Replace
                && matches!(
                    clip.geometry,
                    ClipGeometry::LogicalScissor(scissor) if scissor == logical_scissor
                )
                && clip.generation != 0
                && effect.owner == key
                && effect.parent.is_none()
                && effect.opacity.is_finite()
                && (0.0..=1.0).contains(&effect.opacity)
                && effect.generation != 0)
                .then_some(())
        })()
        .is_some();
        if node.element.is_deferred_to_root_viewport_render() && !exact_deferred_root_effect {
            push_unique(reasons, FramePaintPlanRejection::DeferredBoundary(key));
        }
        if !sampled_layout_transition_is_exact(node.element.as_ref()) {
            push_unique(reasons, FramePaintPlanRejection::LayoutTransition(key));
        }
        if !property_trees.states.contains_key(&key) {
            push_unique(reasons, FramePaintPlanRejection::MissingPropertyState(key));
        }
        let recording_context = node.element.shadow_paint_recording_context(parent_context);
        reachable.push(key);
        let transform = property_trees.transforms.get(&TransformNodeId(key));
        let effect = property_trees.effects.get(&EffectNodeId(key));
        let mut push_boundary = |boundary, parent_boundary_ordinal, seeds: &mut Vec<Seed>| {
            let Ok(ordinal) = u32::try_from(seeds.len()) else {
                push_unique(reasons, FramePaintPlanRejection::InvalidPropertyScene);
                return None;
            };
            seeds.push(Seed {
                boundary,
                parent_boundary_ordinal,
                scene_root_ordinal,
                paint_offset: recording_context.paint_offset,
            });
            Some(ordinal)
        };
        // A co-located pair has one fixed compositor order: opacity owns the
        // local raster, then the transform maps that raster into its parent.
        // Represent that as Transform(outer) -> Effect(inner); descendants
        // therefore inherit the inner effect boundary.
        let next_parent_boundary = match (transform, effect) {
            (Some(_), Some(_)) => push_boundary(
                PropertyBoundaryId::Transform(TransformNodeId(key)),
                parent_boundary_ordinal,
                seeds,
            )
            .and_then(|transform_ordinal| {
                push_boundary(
                    PropertyBoundaryId::Effect(EffectNodeId(key)),
                    Some(transform_ordinal),
                    seeds,
                )
            }),
            (Some(_), None) => push_boundary(
                PropertyBoundaryId::Transform(TransformNodeId(key)),
                parent_boundary_ordinal,
                seeds,
            ),
            (None, Some(_)) => push_boundary(
                PropertyBoundaryId::Effect(EffectNodeId(key)),
                parent_boundary_ordinal,
                seeds,
            ),
            (None, None) => parent_boundary_ordinal,
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
        let (direct_child_pair, same_owner_pair) =
            seeds
                .first()
                .zip(seeds.get(1))
                .map_or((false, false), |(outer, inner)| {
                    (
                        outer.boundary.owner() != inner.boundary.owner()
                            && arena.parent_of(inner.boundary.owner())
                                == Some(outer.boundary.owner()),
                        outer.boundary.owner() == inner.boundary.owner()
                            && arena
                                .get(outer.boundary.owner())
                                .is_some_and(|_| outer.scene_root_ordinal == 0),
                    )
                });
        let mixed_is_proven = property_trees.transforms.len() == 1
            && property_trees.effects.len() == 1
            && roots.len() == 1
            && seeds.len() == 2
            && matches!(seeds[0].boundary, PropertyBoundaryId::Transform(_))
            && matches!(seeds[1].boundary, PropertyBoundaryId::Effect(_))
            && seeds[0].parent_boundary_ordinal.is_none()
            && seeds[1].parent_boundary_ordinal == Some(0)
            && ((direct_child_pair && seeds[0].boundary.owner() == roots[0]) || same_owner_pair)
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
                let bounds = match seed.parent_boundary_ordinal.and_then(|parent| {
                    let parent = seeds.get(parent as usize)?;
                    (parent.boundary == PropertyBoundaryId::Transform(TransformNodeId(owner)))
                        .then_some(())
                }) {
                    Some(()) => {
                        exact_surface_geometry_for_plan(
                            node.element.as_ref(),
                            arena,
                            owner,
                            context,
                            property_trees
                                .transform_snapshot_for(TransformNodeId(owner))
                                .map(|snapshot| snapshot.viewport_matrix),
                        )?
                        .source_bounds
                    }
                    None => node
                        .element
                        .exact_nested_isolation_render_output_bounds(
                            owner,
                            arena,
                            seed.paint_offset,
                        )
                        .ok_or_else(|| FramePaintPlanError {
                            reasons: vec![FramePaintPlanRejection::InvalidIsolationGeometry(owner)],
                        })?,
                };
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
        planned_production_root_step_schedule: None,
        roots: effect_roots,
        surfaces,
        clip_forest,
        production_root_step_spans: None,
        production_root_step_schedule: None,
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
            native_scroll_forest_scaffold: None,
        }),
    };
    property_scene_plan_is_sealed(&plan)
        .then_some(plan)
        .ok_or_else(property_scene_error)
}

#[allow(clippy::too_many_arguments)]
fn collect_native_scroll_forest_node(
    arena: &NodeArena,
    node_key: NodeKey,
    scene_root_ordinal: u32,
    nearest_scroll: Option<NativeScrollBoundaryId>,
    property_trees: &PropertyTrees,
    scale_factor: f32,
    reachable: &mut FxHashSet<NodeKey>,
    stable_ids: &mut FxHashSet<u64>,
    boundaries: &mut Vec<NativeScrollForestBoundaryContract>,
    schedule: &mut Vec<NativeScrollForestScheduledStep>,
) -> Result<(), FramePaintPlanError> {
    let invalid = || property_scene_error();
    if !reachable.insert(node_key) {
        return Err(invalid());
    }
    let node = arena.get(node_key).ok_or_else(invalid)?;
    if node.children() != node.element.children()
        || node.element.stable_id() == 0
        || !stable_ids.insert(node.element.stable_id())
        || node.element.is_deferred_to_root_viewport_render()
        || node
            .element
            .placement_eligibility_metadata()
            .contains_runtime_layout_state
    {
        return Err(invalid());
    }
    let state = property_trees
        .node_state_for(node_key)
        .ok_or_else(invalid)?;
    if let Some(scroll) = property_trees.scroll_snapshot_for(ScrollNodeId(node_key)) {
        let element = node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .ok_or_else(invalid)?;
        let admission = element
            .exact_retained_scroll_forest_host_admission(node_key, arena, scale_factor)
            .ok_or_else(invalid)?;
        let contents_clip_id = ClipNodeId {
            owner: node_key,
            role: ClipNodeRole::ContentsClip,
        };
        let contents_clip = property_trees
            .clip_snapshot_for(Some(contents_clip_id))
            .and_then(|chain| chain.first().copied())
            .ok_or_else(invalid)?;
        let (parent_scroll, parent_clip, projected_output) = match nearest_scroll {
            Some(parent) => {
                let parent = boundaries.get(parent.0 as usize).ok_or_else(invalid)?;
                (
                    Some(parent.scroll.id),
                    Some(parent.contents_clip.id),
                    parent.projection.live_input,
                )
            }
            None => (None, None, PropertyTreeState::default()),
        };
        let live_input = PropertyTreeState {
            clip: Some(contents_clip.id),
            scroll: Some(scroll.id),
            ..PropertyTreeState::default()
        };
        let geometry_is_canonical = scroll.has_canonical_geometry_with_contents_clip_parent_ids(
            contents_clip,
            parent_scroll,
            parent_clip,
        );
        if !admission.matches_scroll_node(scroll)
            || admission.boundary_root != node_key
            || admission.stable_id != node.element.stable_id()
            || scroll.id.0 != node_key
            || scroll.owner != node_key
            || scroll.parent != parent_scroll
            || contents_clip.id != contents_clip_id
            || contents_clip.owner != node_key
            || contents_clip.parent != parent_clip
            || contents_clip.behavior != ClipBehavior::Intersect
            || scroll.generation == 0
            || contents_clip.generation == 0
            || !geometry_is_canonical
            || state.paint != projected_output
            || state.descendants != live_input
        {
            return Err(invalid());
        }
        let id = NativeScrollBoundaryId(u32::try_from(boundaries.len()).map_err(|_| invalid())?);
        let projection = NativeScrollProjectionWitness {
            live_input,
            projected_output,
            parent_scroll,
            parent_clip,
        };
        boundaries.push(NativeScrollForestBoundaryContract {
            id,
            scene_root_ordinal,
            boundary_root: node_key,
            stable_id: admission.stable_id,
            parent: nearest_scroll,
            admission,
            scroll,
            contents_clip,
            projection: projection.clone(),
        });
        schedule.push(NativeScrollForestScheduledStep::ChildBoundary {
            parent: nearest_scroll,
            child: id,
        });
        schedule.push(NativeScrollForestScheduledStep::Artifact {
            boundary: id,
            phase: NativeScrollArtifactPhase::HostBefore,
        });
        collect_native_scroll_forest_node(
            arena,
            admission.content_root,
            scene_root_ordinal,
            Some(id),
            property_trees,
            scale_factor,
            reachable,
            stable_ids,
            boundaries,
            schedule,
        )?;
        schedule.push(NativeScrollForestScheduledStep::Artifact {
            boundary: id,
            phase: NativeScrollArtifactPhase::OverlayAfter,
        });
        return Ok(());
    }

    if !is_exact_retained_scroll_forest_content_node(node.element.as_ref(), arena) {
        return Err(invalid());
    }
    let expected = nearest_scroll
        .and_then(|id| boundaries.get(id.0 as usize))
        .map(|boundary| boundary.projection.live_input)
        .unwrap_or_default();
    if state.paint != expected || state.descendants != expected {
        return Err(invalid());
    }
    if node.children().is_empty() {
        let boundary = nearest_scroll.ok_or_else(invalid)?;
        let projection = boundaries
            .get(boundary.0 as usize)
            .ok_or_else(invalid)?
            .projection
            .clone();
        schedule.push(NativeScrollForestScheduledStep::ContentReceiver {
            boundary,
            content_root: node_key,
            stable_id: node.element.stable_id(),
            projection,
        });
    } else {
        for &child in node.children() {
            if arena.parent_of(child) != Some(node_key) {
                return Err(invalid());
            }
            collect_native_scroll_forest_node(
                arena,
                child,
                scene_root_ordinal,
                nearest_scroll,
                property_trees,
                scale_factor,
                reachable,
                stable_ids,
                boundaries,
                schedule,
            )?;
        }
    }
    Ok(())
}

/// Plans an arbitrary-depth/multi-root native scroll forest without granting
/// compiler, pool, transaction, or frame-graph authority.
pub(crate) fn plan_native_scroll_forest_scaffold_with_context(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
    context: TransformSurfacePlanContext,
) -> Result<FramePaintPlan, FramePaintPlanError> {
    if roots.is_empty()
        || !scale_factor.is_finite()
        || scale_factor <= 0.0
        || context.paint_offset_bits != [0.0_f32.to_bits(); 2]
        || context.outer_scissor_rect().is_some()
        || !property_trees.validation_errors.is_empty()
        || !property_trees.transforms.is_empty()
        || !property_trees.effects.is_empty()
        || property_trees.scrolls.len() < 3
        || !paint_generations.matches_live_snapshot(arena, roots, property_trees)
    {
        return Err(property_scene_error());
    }
    let mut seen_roots = FxHashSet::default();
    let mut reachable = FxHashSet::default();
    let mut stable_ids = FxHashSet::default();
    let mut boundaries = Vec::new();
    let mut steps = Vec::new();
    let mut forest_roots = Vec::with_capacity(roots.len());
    let mut plan_roots = Vec::with_capacity(roots.len());
    for (ordinal, &root) in roots.iter().enumerate() {
        if !seen_roots.insert(root) || arena.parent_of(root).is_some() {
            return Err(property_scene_error());
        }
        let start = u32::try_from(boundaries.len()).map_err(|_| property_scene_error())?;
        collect_native_scroll_forest_node(
            arena,
            root,
            u32::try_from(ordinal).map_err(|_| property_scene_error())?,
            None,
            property_trees,
            scale_factor,
            &mut reachable,
            &mut stable_ids,
            &mut boundaries,
            &mut steps,
        )?;
        let end = u32::try_from(boundaries.len()).map_err(|_| property_scene_error())?;
        if start == end {
            return Err(property_scene_error());
        }
        let stable_id = arena
            .get(root)
            .map(|node| node.element.stable_id())
            .filter(|stable_id| *stable_id != 0)
            .ok_or_else(property_scene_error)?;
        forest_roots.push(NativeScrollForestRoot {
            ordinal: ordinal as u32,
            root,
            stable_id,
            boundary_span: start..end,
        });
        plan_roots.push(PropertySceneRootWitness {
            ordinal: ordinal as u32,
            root,
            stable_id,
            owner: PaintOwnerSnapshot {
                owner: root,
                parent: None,
            },
            top_level_step_span: 0..0,
        });
    }
    if boundaries.len() != property_trees.scrolls.len()
        || boundaries.len() != property_trees.clips.len()
        || reachable.len() != property_trees.states.len()
        || property_trees
            .states
            .keys()
            .any(|key| !reachable.contains(key))
    {
        return Err(property_scene_error());
    }
    let artifact_seal = |artifact: &PaintArtifact| {
        Ok::<_, FramePaintPlanError>(NestedScrollArtifactSeal {
            recorded_artifact: artifact.clone(),
            identity: property_scroll_receiver_artifact_identity(artifact)
                .ok_or_else(property_scene_error)?,
        })
    };
    let mut programs = Vec::with_capacity(boundaries.len());
    for boundary in &boundaries {
        let edge = super::PaintScrollForestEdgeWitness::new(
            boundary.boundary_root,
            boundary.admission.content_root,
            boundary.scroll,
            boundary.contents_clip,
            boundary.projection.parent_scroll,
            boundary.projection.parent_clip,
        )
        .ok_or_else(property_scene_error)?;
        let host = super::PaintBakedScrollHostWitness::new(
            boundary.boundary_root,
            boundary.admission.content_root,
            boundary.scroll,
            boundary.contents_clip.id,
        )
        .ok_or_else(property_scene_error)?;
        let consumed_parent = boundary
            .parent
            .map(|parent| {
                let parent = &boundaries[parent.0 as usize];
                super::ConsumedAncestorScrollContentsWitness::new(
                    parent.boundary_root,
                    boundary.boundary_root,
                    parent.scroll.id,
                    parent.contents_clip.id,
                )
                .ok_or_else(property_scene_error)
            })
            .transpose()?;
        let content_stable_id = arena
            .get(boundary.admission.content_root)
            .map(|node| node.element.stable_id())
            .filter(|stable_id| *stable_id != 0)
            .ok_or_else(property_scene_error)?;
        let host_steps = super::frame_recorder::record_native_scroll_forest_host_steps_for_plan(
            arena,
            boundary.boundary_root,
            property_trees,
            paint_generations,
            host,
            edge,
            consumed_parent,
            content_stable_id,
        )
        .map_err(|reasons| FramePaintPlanError {
            reasons: reasons
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })?;
        let [
            super::frame_recorder::RecordedNativeScrollHostStep::Artifact(host_before),
            super::frame_recorder::RecordedNativeScrollHostStep::ContentReceiver(receiver),
            super::frame_recorder::RecordedNativeScrollHostStep::Artifact(overlay_after),
        ] = host_steps.as_slice()
        else {
            return Err(property_scene_error());
        };
        if receiver.stable_id != content_stable_id || receiver.witness != edge {
            return Err(property_scene_error());
        }
        let child_boundaries = boundaries
            .iter()
            .filter(|child| child.parent == Some(boundary.id))
            .map(|child| {
                (
                    child.id,
                    super::PlannedBoundary {
                        root: child.boundary_root,
                        stable_id: child.stable_id,
                        kind: super::PlannedBoundaryKind::Scroll(child.scroll.id),
                    },
                )
            })
            .collect::<Vec<_>>();
        let child_cutouts = child_boundaries
            .iter()
            .map(|(_, cutout)| *cutout)
            .collect::<Vec<_>>();
        let recorded_content =
            super::frame_recorder::record_native_scroll_forest_content_steps_for_plan(
                arena,
                property_trees,
                paint_generations,
                edge,
                &child_cutouts,
            )
            .map_err(|reasons| FramePaintPlanError {
                reasons: reasons
                    .into_iter()
                    .map(FramePaintPlanRejection::Coverage)
                    .collect(),
            })?;
        let mut marker_cursor = 0usize;
        let content_steps = recorded_content
            .iter()
            .map(|step| match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    artifact_seal(artifact).map(NativeScrollForestContentProgramStep::Artifact)
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker) => {
                    let Some((child, expected)) = child_boundaries.get(marker_cursor) else {
                        return Err(property_scene_error());
                    };
                    if marker != expected {
                        return Err(property_scene_error());
                    }
                    marker_cursor += 1;
                    Ok(NativeScrollForestContentProgramStep::ChildBoundary(*child))
                }
            })
            .collect::<Result<Vec<_>, FramePaintPlanError>>()?;
        if marker_cursor != child_boundaries.len() || content_steps.is_empty() {
            return Err(property_scene_error());
        }
        let source_bounds = boundary.admission.source_bounds;
        let compiler_stamp =
            super::compiler::compile_native_scroll_forest_boundary_program_for_plan(
                boundary.boundary_root,
                boundary.admission.content_root,
                boundary.scroll,
                [
                    source_bounds.x.to_bits(),
                    source_bounds.y.to_bits(),
                    source_bounds.width.to_bits(),
                    source_bounds.height.to_bits(),
                ],
                host_before,
                &recorded_content,
                &child_cutouts,
                overlay_after,
            )
            .ok_or_else(property_scene_error)?;
        programs.push(NativeScrollForestBoundaryProgram {
            boundary: boundary.id,
            receiver_stable_id: content_stable_id,
            edge,
            host_before: artifact_seal(host_before)?,
            content_steps,
            overlay_after: artifact_seal(overlay_after)?,
            compiler_stamp,
            child_dependencies: Vec::new(),
            content_program_opaque_terminal: 0,
        });
    }
    for program_index in (0..programs.len()).rev() {
        let (dependencies, terminal) = {
            let program = &programs[program_index];
            let mut cursor = 0_u32;
            let mut dependencies = Vec::new();
            for step in &program.content_steps {
                match step {
                    NativeScrollForestContentProgramStep::Artifact(artifact) => {
                        cursor = cursor
                            .checked_add(opaque_order_count(artifact.artifact()))
                            .ok_or_else(property_scene_error)?;
                    }
                    NativeScrollForestContentProgramStep::ChildBoundary(child) => {
                        let child_program = programs
                            .get(child.0 as usize)
                            .ok_or_else(property_scene_error)?;
                        let child_boundary = boundaries
                            .get(child.0 as usize)
                            .ok_or_else(property_scene_error)?;
                        let before = cursor;
                        cursor = cursor
                            .checked_add(child_program.compiler_stamp.host_opaque_count)
                            .and_then(|cursor| {
                                cursor
                                    .checked_add(child_program.compiler_stamp.overlay_opaque_count)
                            })
                            .ok_or_else(property_scene_error)?;
                        let source = child_boundary.scroll.layout_content_bounds_at_zero;
                        dependencies.push(NativeScrollForestChildRasterDependency {
                            child: *child,
                            boundary_root: child_boundary.boundary_root,
                            content_root: child_boundary.admission.content_root,
                            content_stable_id: child_program.receiver_stable_id,
                            child_raster_identity: Box::new(
                                child_program
                                    .content_raster_identity(child_boundary.admission.content_root),
                            ),
                            host_identity: child_program.host_before.identity.clone(),
                            overlay_identity: child_program.overlay_after.identity.clone(),
                            scroll: child_boundary.scroll,
                            contents_clip: child_boundary.contents_clip,
                            source_bounds_bits: [
                                source.x.to_bits(),
                                source.y.to_bits(),
                                source.width.to_bits(),
                                source.height.to_bits(),
                            ],
                            offset_bits: [
                                child_boundary.scroll.offset.x.to_bits(),
                                child_boundary.scroll.offset.y.to_bits(),
                            ],
                            composite_scissor: child_boundary.contents_clip.logical_scissor,
                            parent_opaque_before: before,
                            parent_opaque_after: cursor,
                        });
                    }
                }
            }
            (dependencies, cursor)
        };
        let program = &mut programs[program_index];
        program.child_dependencies = dependencies;
        program.content_program_opaque_terminal = terminal;
    }
    let schedule = NativeScrollForestSchedule { steps };
    let scaffold = NativeScrollForestScaffold {
        context,
        scale_factor_bits: scale_factor.to_bits(),
        roots: forest_roots.clone(),
        boundaries: boundaries.clone(),
        schedule: schedule.clone(),
        programs: programs.clone(),
        planned_context: context,
        planned_scale_factor_bits: scale_factor.to_bits(),
        planned_roots: forest_roots,
        planned_boundaries: boundaries,
        planned_schedule: schedule,
        planned_programs: programs,
    };
    let plan = FramePaintPlan {
        steps: Vec::new(),
        property_scene_roots: Some(plan_roots.clone()),
        property_scene_seal: Some(PropertyScenePlanSeal {
            roots: plan_roots,
            context,
            outer_scissor_rect: None,
            aggregate_opaque_order_span: 0..0,
            surface_count: 0,
            scene_artifact_validation: Vec::new(),
            surfaces: FxHashMap::default(),
            effect_scaffold: None,
            scroll_schedule_scaffold: None,
            nested_scroll_scaffold: None,
            native_scroll_forest_scaffold: Some(scaffold),
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
            native_scroll_forest_scaffold: None,
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
        scene_root: NodeKey,
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
        if !sampled_layout_transition_is_exact(node.element.as_ref()) {
            push_unique(reasons, FramePaintPlanRejection::LayoutTransition(key));
        }
        let Some(state) = property_trees.node_state_for(key) else {
            push_unique(reasons, FramePaintPlanRejection::MissingPropertyState(key));
            return;
        };
        let transform = property_trees.transforms.get(&TransformNodeId(key));
        let effect = property_trees.effects.get(&EffectNodeId(key));
        let scroll = property_trees.scrolls.get(&ScrollNodeId(key));
        let co_located_transform_effect =
            transform.is_some() && effect.is_some() && scroll.is_none();
        let co_located_transform_scroll =
            transform.is_some() && effect.is_none() && scroll.is_some();
        let co_located_effect_scroll = transform.is_none() && effect.is_some() && scroll.is_some();
        let co_located_transform_effect_scroll =
            transform.is_some() && effect.is_some() && scroll.is_some();
        if usize::from(transform.is_some())
            + usize::from(effect.is_some())
            + usize::from(scroll.is_some())
            > 1
            && !co_located_transform_effect
            && !co_located_transform_scroll
            && !co_located_effect_scroll
            && !co_located_transform_effect_scroll
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

        let mut pre_pushed = 0usize;
        if co_located_transform_scroll
            || co_located_effect_scroll
            || co_located_transform_effect_scroll
        {
            if path
                .iter()
                .any(|entry| matches!(entry, PathBoundary::Scroll(_)))
            {
                push_unique(
                    reasons,
                    FramePaintPlanRejection::UnsupportedPropertyInterleave(key),
                );
            }
            let mut parent = path.iter().rev().find_map(|entry| match entry {
                PathBoundary::Transform(id) => {
                    Some(PropertyScheduledSurfaceBoundaryId::Transform(*id))
                }
                PathBoundary::Effect(id) => Some(PropertyScheduledSurfaceBoundaryId::Effect(*id)),
                PathBoundary::Scroll(_) => None,
            });
            if transform.is_some() {
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
                schedule.push(PropertySceneScheduledStep::RetainedSurface {
                    boundary: PropertyScheduledSurfaceBoundary::Transform(snapshot),
                    parent,
                });
                path.push(PathBoundary::Transform(snapshot.id));
                parent = Some(PropertyScheduledSurfaceBoundaryId::Transform(snapshot.id));
                pre_pushed += 1;
            }
            if effect.is_some() {
                let snapshot = property_trees
                    .effect_snapshot_for(Some(EffectNodeId(key)))
                    .and_then(|nodes| nodes.first().copied());
                let Some(snapshot) = snapshot else {
                    push_unique(reasons, FramePaintPlanRejection::InvalidEffectChain(key));
                    return;
                };
                if snapshot.owner != key
                    || snapshot.parent.is_some()
                    || snapshot.generation == 0
                    || !snapshot.opacity.is_finite()
                    || !(0.0..=1.0).contains(&snapshot.opacity)
                {
                    push_unique(reasons, FramePaintPlanRejection::InvalidEffectChain(key));
                }
                schedule.push(PropertySceneScheduledStep::RetainedSurface {
                    boundary: PropertyScheduledSurfaceBoundary::Effect(snapshot),
                    parent,
                });
                path.push(PathBoundary::Effect(snapshot.id));
                pre_pushed += 1;
            }
        }
        let transform = (!(co_located_transform_scroll || co_located_transform_effect_scroll))
            .then_some(transform)
            .flatten();
        let effect = (!(co_located_effect_scroll || co_located_transform_effect_scroll))
            .then_some(effect)
            .flatten();
        let path_has_scroll = path
            .iter()
            .any(|entry| matches!(entry, PathBoundary::Scroll(_)));
        let pushed = match (transform, effect, scroll) {
            (Some(_), Some(_), None) => {
                if path_has_scroll {
                    push_unique(
                        reasons,
                        FramePaintPlanRejection::UnsupportedPropertyInterleave(key),
                    );
                }
                let Some(transform) = property_trees.transform_snapshot_for(TransformNodeId(key))
                else {
                    push_unique(reasons, FramePaintPlanRejection::InvalidRootTransform(key));
                    return;
                };
                let effect = property_trees
                    .effect_snapshot_for(Some(EffectNodeId(key)))
                    .and_then(|nodes| nodes.first().copied());
                let Some(effect) = effect else {
                    push_unique(reasons, FramePaintPlanRejection::InvalidEffectChain(key));
                    return;
                };
                if transform.owner != key
                    || transform.generation == 0
                    || !matrix_is_finite_affine(transform.viewport_matrix)
                    || effect.owner != key
                    || effect.parent.is_some()
                    || effect.generation == 0
                    || !effect.opacity.is_finite()
                    || !(0.0..=1.0).contains(&effect.opacity)
                {
                    push_unique(
                        reasons,
                        FramePaintPlanRejection::UnsupportedPropertyInterleave(key),
                    );
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
                    boundary: PropertyScheduledSurfaceBoundary::Transform(transform),
                    parent,
                });
                schedule.push(PropertySceneScheduledStep::RetainedSurface {
                    boundary: PropertyScheduledSurfaceBoundary::Effect(effect),
                    parent: Some(PropertyScheduledSurfaceBoundaryId::Transform(transform.id)),
                });
                path.push(PathBoundary::Transform(transform.id));
                path.push(PathBoundary::Effect(effect.id));
                2
            }
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
                1
            }
            (None, Some(_), None) => {
                let scroll_content_receiver = path
                    .iter()
                    .position(|entry| matches!(entry, PathBoundary::Scroll(_)));
                if scroll_content_receiver.is_some()
                    && path
                        .iter()
                        .skip(scroll_content_receiver.unwrap_or_default() + 1)
                        .any(|entry| !matches!(entry, PathBoundary::Scroll(_)))
                {
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
                if let Some(scroll) = path.iter().rev().find_map(|entry| match entry {
                    PathBoundary::Scroll(id) => Some(*id),
                    _ => None,
                }) {
                    schedule.push(PropertySceneScheduledStep::ScrollContentSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Effect(snapshot),
                        scroll,
                    });
                } else {
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
                }
                path.push(PathBoundary::Effect(snapshot.id));
                1
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
                let Some(clip_chain) = property_trees.clip_snapshot_for(Some(clip_id)) else {
                    push_unique(reasons, FramePaintPlanRejection::InvalidScrollHost(key));
                    return;
                };
                let Some((&contents_clip, ancestor_composite_clips)) = clip_chain.split_first()
                else {
                    push_unique(reasons, FramePaintPlanRejection::InvalidScrollHost(key));
                    return;
                };
                if scroll.parent.is_some()
                    || !scroll.is_canonical_with_ancestor_contents_clip(contents_clip)
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
                let mut expected_receiver_state = state.paint;
                for ancestor in path.iter().copied() {
                    match ancestor {
                        PathBoundary::Transform(id)
                            if expected_receiver_state.transform == Some(id) =>
                        {
                            expected_receiver_state.transform = None;
                        }
                        PathBoundary::Effect(id) if expected_receiver_state.effect == Some(id) => {
                            expected_receiver_state.effect = None;
                        }
                        _ => {
                            push_unique(
                                reasons,
                                FramePaintPlanRejection::UnsupportedPropertyInterleave(key),
                            );
                        }
                    }
                }
                if cursor != expected_receiver_state {
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
                let mut content_owners = FxHashSet::default();
                let mut content_pending = node.element.children().to_vec();
                while let Some(owner) = content_pending.pop() {
                    if !content_owners.insert(owner) {
                        push_unique(reasons, FramePaintPlanRejection::DuplicateNodeKey(owner));
                        continue;
                    }
                    let Some(content_node) = arena.get(owner) else {
                        push_unique(reasons, FramePaintPlanRejection::MissingRoot(owner));
                        continue;
                    };
                    content_pending.extend(content_node.element.children().iter().copied());
                }
                let mut local_content_clips = Vec::new();
                let mut receiver_clips = Vec::new();
                for (&id, clip) in &property_trees.clips {
                    let mut clip_root = clip.owner;
                    while let Some(parent) = arena.parent_of(clip_root) {
                        clip_root = parent;
                    }
                    if clip_root != scene_root {
                        continue;
                    }
                    if id == contents_clip.id
                        || ancestor_composite_clips
                            .iter()
                            .any(|ancestor| ancestor.id == id)
                    {
                        continue;
                    }
                    let ClipGeometry::LogicalScissor(logical_scissor) = clip.geometry else {
                        push_unique(reasons, FramePaintPlanRejection::InvalidPropertyScene);
                        continue;
                    };
                    let snapshot = ClipNodeSnapshot {
                        id,
                        owner: clip.owner,
                        parent: clip.parent,
                        logical_scissor,
                        behavior: clip.behavior,
                        generation: clip.generation,
                    };
                    if content_owners.contains(&clip.owner) {
                        local_content_clips.push(snapshot);
                    } else {
                        receiver_clips.push(snapshot);
                    }
                }
                let clip_sort_key = |clip: &ClipNodeSnapshot| {
                    (
                        arena
                            .get(clip.owner)
                            .map_or(0, |node| node.element.stable_id()),
                        matches!(clip.id.role, ClipNodeRole::ContentsClip),
                    )
                };
                local_content_clips.sort_by_key(clip_sort_key);
                receiver_clips.sort_by_key(clip_sort_key);
                boundaries.push(PropertyScrollBoundaryContract {
                    ordinal,
                    scene_root_ordinal,
                    scroll,
                    contents_clip,
                    ancestor_composite_clips: ancestor_composite_clips.to_vec(),
                    local_content_clips,
                    receiver_clips,
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
                1
            }
            (None, None, None) => 0,
            _ => 0,
        };
        for &child in node.element.children() {
            if arena.parent_of(child) != Some(key) {
                push_unique(reasons, FramePaintPlanRejection::TopologyMismatch(child));
            }
            walk(
                arena,
                child,
                scene_root,
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
        for _ in 0..pushed + pre_pushed {
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
        let root_boundary_count = boundaries.len().saturating_sub(boundary_start);
        if root_boundary_count > 1
            || (root_boundary_count == 1
                && !property_scroll_root_schedule_is_supported(&schedule_steps[span.clone()]))
            || (root_boundary_count == 0 && !schedule_steps[span.clone()].is_empty())
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
        || property_trees.clips.keys().any(|id| {
            !boundaries.iter().any(|boundary| {
                boundary.contents_clip.id == *id
                    || boundary
                        .ancestor_composite_clips
                        .iter()
                        .chain(&boundary.local_content_clips)
                        .chain(&boundary.receiver_clips)
                        .any(|clip| clip.id == *id)
            })
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
        property_trees,
        paint_generations,
        context,
        &schedule_roots,
        &schedule,
        &boundaries,
    )?;
    let same_owner_transform_scroll_insertions =
        plan_same_owner_transform_scroll_receiver_insertions(
            arena,
            property_trees,
            paint_generations,
            &schedule_roots,
            &schedule,
            &boundaries,
        )?;
    let frame_receiver_insertions = plan_property_frame_scroll_receiver_insertions(
        arena,
        property_trees,
        paint_generations,
        context,
        &schedule_roots,
        &schedule,
        &boundaries,
    )?;
    let effect_receiver_insertions = plan_property_effect_scroll_receiver_insertions(
        arena,
        property_trees,
        paint_generations,
        context,
        &schedule_roots,
        &schedule,
        &boundaries,
    )?;
    let same_owner_effect_scroll_insertions = plan_same_owner_effect_scroll_receiver_insertions(
        arena,
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
            property_trees,
            paint_generations,
            context,
            &schedule_roots,
            &schedule,
            &boundaries,
        )?;
    let effect_transform_receiver_insertions =
        plan_property_effect_transform_scroll_receiver_insertions(
            arena,
            property_trees,
            paint_generations,
            context,
            &schedule_roots,
            &schedule,
            &boundaries,
        )?;
    let scroll_content_effect_insertions = plan_property_scroll_content_effect_insertions(
        arena,
        property_trees,
        paint_generations,
        context,
        &schedule_roots,
        &schedule,
        &boundaries,
    )?;
    let boundary_dag = project_property_boundary_dag(
        arena,
        property_trees,
        &schedule_roots,
        &schedule,
        &boundaries,
        &receiver_insertions,
        &same_owner_transform_scroll_insertions,
        &same_owner_effect_scroll_insertions,
        &frame_receiver_insertions,
        &effect_receiver_insertions,
        &transform_effect_receiver_insertions,
        &effect_transform_receiver_insertions,
        &scroll_content_effect_insertions,
    )?;
    let scaffold = PropertyScrollScheduleScaffold {
        context,
        roots: schedule_roots.clone(),
        schedule: schedule.clone(),
        boundaries: boundaries.clone(),
        receiver_insertions: receiver_insertions.clone(),
        same_owner_transform_scroll_insertions: same_owner_transform_scroll_insertions.clone(),
        same_owner_effect_scroll_insertions: same_owner_effect_scroll_insertions.clone(),
        frame_receiver_insertions: frame_receiver_insertions.clone(),
        effect_receiver_insertions: effect_receiver_insertions.clone(),
        transform_effect_receiver_insertions: transform_effect_receiver_insertions.clone(),
        effect_transform_receiver_insertions: effect_transform_receiver_insertions.clone(),
        scroll_content_effect_insertions: scroll_content_effect_insertions.clone(),
        boundary_dag: boundary_dag.clone(),
        planned_context: context,
        planned_roots: schedule_roots,
        planned_schedule: schedule,
        planned_boundaries: boundaries,
        planned_receiver_insertions: receiver_insertions,
        planned_same_owner_transform_scroll_insertions: same_owner_transform_scroll_insertions,
        planned_same_owner_effect_scroll_insertions: same_owner_effect_scroll_insertions,
        planned_frame_receiver_insertions: frame_receiver_insertions,
        planned_effect_receiver_insertions: effect_receiver_insertions,
        planned_transform_effect_receiver_insertions: transform_effect_receiver_insertions,
        planned_effect_transform_receiver_insertions: effect_transform_receiver_insertions,
        planned_scroll_content_effect_insertions: scroll_content_effect_insertions,
        planned_boundary_dag: boundary_dag,
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
            native_scroll_forest_scaffold: None,
        }),
    };
    property_scene_plan_is_sealed(&plan)
        .then_some(plan)
        .ok_or_else(property_scene_error)
}

fn property_scroll_root_schedule_is_supported(steps: &[PropertySceneScheduledStep]) -> bool {
    match steps {
        [] => true,
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
        [
            PropertySceneScheduledStep::RetainedSurface {
                boundary: PropertyScheduledSurfaceBoundary::Effect(effect),
                parent: None,
            },
            PropertySceneScheduledStep::RetainedSurface {
                boundary: PropertyScheduledSurfaceBoundary::Transform(transform),
                parent: Some(PropertyScheduledSurfaceBoundaryId::Effect(parent)),
            },
            PropertySceneScheduledStep::ScrollBoundary {
                basis: ScrollCompositeBasis::Transform(basis),
                ..
            },
        ] => effect.id == *parent && transform == basis,
        [
            PropertySceneScheduledStep::ScrollBoundary {
                scroll,
                basis: ScrollCompositeBasis::FrameRoot,
                ..
            },
            PropertySceneScheduledStep::ScrollContentSurface {
                boundary: PropertyScheduledSurfaceBoundary::Effect(_),
                scroll: receiver,
            },
        ] => scroll == receiver,
        [
            PropertySceneScheduledStep::RetainedSurface {
                boundary: PropertyScheduledSurfaceBoundary::Transform(transform),
                parent: None,
            },
            PropertySceneScheduledStep::ScrollBoundary {
                scroll,
                basis: ScrollCompositeBasis::Transform(basis),
                ..
            },
            PropertySceneScheduledStep::ScrollContentSurface {
                boundary: PropertyScheduledSurfaceBoundary::Effect(_),
                scroll: receiver,
            },
        ] => transform == basis && scroll == receiver,
        _ => false,
    }
}

fn property_boundary_insertion_seal(
    insertion_index: usize,
    before_span: Range<usize>,
    after_span: Range<usize>,
    receiver_opaque_before: u32,
    receiver_opaque_after: u32,
    recorded_steps: &[PropertyScrollReceiverRecordedStepIdentity],
) -> PropertyBoundaryInsertionSeal {
    PropertyBoundaryInsertionSeal {
        insertion_index,
        before_span,
        after_span,
        receiver_opaque_before,
        receiver_opaque_after,
        recorded_steps: recorded_steps.to_vec(),
    }
}

fn property_boundary_neutral_path(
    arena: &NodeArena,
    receiver_owner: NodeKey,
    boundary_owner: NodeKey,
) -> Option<Vec<PropertyBoundaryPathOwnerWitness>> {
    if receiver_owner == boundary_owner {
        return Some(Vec::new());
    }
    let mut reverse = Vec::new();
    let mut current = arena.parent_of(boundary_owner)?;
    while current != receiver_owner {
        let node = arena.get(current)?;
        let stable_id = node.element.stable_id();
        if stable_id == 0 {
            return None;
        }
        reverse.push(PropertyBoundaryPathOwnerWitness {
            owner: current,
            stable_id,
        });
        current = arena.parent_of(current)?;
    }
    reverse.reverse();
    Some(reverse)
}

fn property_boundary_consumption(
    boundary: &PropertyScrollBoundaryContract,
    kind: ConsumedPropertyBoundary,
) -> Option<ConsumedPropertyEntry> {
    boundary
        .consumed_properties
        .entries
        .iter()
        .copied()
        .find(|entry| entry.boundary == kind)
}

fn property_scroll_content_effect_consumption(
    property_trees: &PropertyTrees,
    effect: EffectNodeSnapshot,
) -> Option<ConsumedPropertyEntry> {
    let mut projected_after = property_trees.node_state_for(effect.owner)?.descendants;
    let expected_before = projected_after;
    if projected_after.effect != Some(effect.id) {
        return None;
    }
    projected_after.effect = effect.parent;
    Some(ConsumedPropertyEntry {
        boundary: ConsumedPropertyBoundary::Effect(effect.id),
        expected_before,
        projected_after,
    })
}

#[allow(clippy::too_many_arguments)]
fn push_property_boundary_dag_node(
    arena: &NodeArena,
    nodes: &mut Vec<PropertyBoundaryDagNode>,
    scene_root_ordinal: u32,
    receiver: PropertyBoundaryReceiverScope,
    kind: PropertyBoundaryDagNodeKind,
    consumption: ConsumedPropertyEntry,
    placement: PropertyBoundaryDagPlacement,
) -> Result<PropertyBoundaryDagNodeId, FramePaintPlanError> {
    let owner = kind.owner();
    let stable_id = arena
        .get(owner)
        .map(|node| node.element.stable_id())
        .filter(|stable_id| *stable_id != 0)
        .ok_or_else(property_scene_error)?;
    let id =
        PropertyBoundaryDagNodeId(u32::try_from(nodes.len()).map_err(|_| property_scene_error())?);
    nodes.push(PropertyBoundaryDagNode {
        id,
        scene_root_ordinal,
        owner,
        stable_id,
        receiver,
        kind,
        consumption,
        placement,
    });
    Ok(id)
}

fn project_property_boundary_dag(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    roots: &[PropertyScrollScheduleRoot],
    schedule: &PropertySceneSchedule,
    boundaries: &[PropertyScrollBoundaryContract],
    receiver_insertions: &[PropertyScrollReceiverInsertionContract],
    same_owner_transform_scroll_insertions:
        &[PropertySameOwnerTransformScrollReceiverInsertionContract],
    same_owner_effect_scroll_insertions:
        &[PropertySameOwnerEffectScrollReceiverInsertionContract],
    frame_receiver_insertions: &[PropertyFrameScrollReceiverInsertionContract],
    effect_receiver_insertions: &[PropertyEffectScrollReceiverInsertionContract],
    transform_effect_receiver_insertions: &[PropertyTransformEffectScrollReceiverInsertionContract],
    effect_transform_receiver_insertions: &[PropertyEffectTransformScrollReceiverInsertionContract],
    scroll_content_effect_insertions: &[PropertyScrollContentEffectInsertionContract],
) -> Result<PropertyBoundaryDag, FramePaintPlanError> {
    let mut dag_roots = Vec::with_capacity(roots.len());
    let mut nodes = Vec::new();
    for root in roots {
        let node_start = nodes.len();
        let root_steps = schedule
            .steps
            .get(root.step_span.clone())
            .ok_or_else(property_scene_error)?;
        let root_boundary = boundaries
            .iter()
            .find(|boundary| boundary.scene_root_ordinal == root.ordinal);
        if root_steps.is_empty() {
            if root_boundary.is_some() {
                return Err(property_scene_error());
            }
        } else {
            let boundary = root_boundary.ok_or_else(property_scene_error)?;
            let scroll_marker = super::PlannedBoundary {
                root: boundary.scroll.owner,
                stable_id: arena
                    .get(boundary.scroll.owner)
                    .map(|node| node.element.stable_id())
                    .filter(|stable_id| *stable_id != 0)
                    .ok_or_else(property_scene_error)?,
                kind: super::PlannedBoundaryKind::Scroll(boundary.scroll.id),
            };
            let scroll_consumption = property_boundary_consumption(
                boundary,
                ConsumedPropertyBoundary::ScrollContents {
                    scroll: boundary.scroll.id,
                    contents_clip: boundary.contents_clip.id,
                },
            )
            .ok_or_else(property_scene_error)?;
            match root_steps {
                [PropertySceneScheduledStep::ScrollBoundary { .. }] => {
                    let insertion = frame_receiver_insertions.iter().find(|insertion| {
                        insertion.scene_root_ordinal == root.ordinal
                            && insertion.scroll_boundary_ordinal == boundary.ordinal
                    });
                    let sealed = insertion.map(|insertion| {
                        property_boundary_insertion_seal(
                            insertion.insertion_index,
                            insertion.before_span.clone(),
                            insertion.after_span.clone(),
                            insertion.receiver_opaque_before,
                            insertion.receiver_opaque_after,
                            &insertion.recorded_steps,
                        )
                    });
                    let neutral_path =
                        property_boundary_neutral_path(arena, root.root, boundary.scroll.owner)
                            .ok_or_else(property_scene_error)?;
                    push_property_boundary_dag_node(
                        arena,
                        &mut nodes,
                        root.ordinal,
                        PropertyBoundaryReceiverScope::FrameRoot {
                            scene_root_ordinal: root.ordinal,
                        },
                        PropertyBoundaryDagNodeKind::Scroll(boundary.clone()),
                        scroll_consumption,
                        PropertyBoundaryDagPlacement::Cutout {
                            marker: scroll_marker,
                            neutral_path,
                            sealed,
                        },
                    )?;
                }
                [
                    PropertySceneScheduledStep::RetainedSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Transform(transform),
                        parent: None,
                    },
                    PropertySceneScheduledStep::ScrollBoundary { .. },
                ] => {
                    let transform_consumption = property_boundary_consumption(
                        boundary,
                        ConsumedPropertyBoundary::Transform(transform.id),
                    )
                    .ok_or_else(property_scene_error)?;
                    let transform_id = push_property_boundary_dag_node(
                        arena,
                        &mut nodes,
                        root.ordinal,
                        PropertyBoundaryReceiverScope::FrameRoot {
                            scene_root_ordinal: root.ordinal,
                        },
                        PropertyBoundaryDagNodeKind::Transform(*transform),
                        transform_consumption,
                        PropertyBoundaryDagPlacement::Root,
                    )?;
                    let insertion = receiver_insertions
                        .iter()
                        .find(|insertion| {
                            insertion.scene_root_ordinal == root.ordinal
                                && insertion.scroll_boundary_ordinal == boundary.ordinal
                        })
                        .or_else(|| {
                            same_owner_transform_scroll_insertions
                                .iter()
                                .map(|insertion| &insertion.receiver)
                                .find(|insertion| {
                                    insertion.scene_root_ordinal == root.ordinal
                                        && insertion.scroll_boundary_ordinal == boundary.ordinal
                                })
                        });
                    let sealed = insertion.map(|insertion| {
                        property_boundary_insertion_seal(
                            insertion.insertion_index,
                            insertion.before_span.clone(),
                            insertion.after_span.clone(),
                            insertion.receiver_opaque_before,
                            insertion.receiver_opaque_after,
                            &insertion.recorded_steps,
                        )
                    });
                    push_property_boundary_dag_node(
                        arena,
                        &mut nodes,
                        root.ordinal,
                        PropertyBoundaryReceiverScope::Surface(transform_id),
                        PropertyBoundaryDagNodeKind::Scroll(boundary.clone()),
                        scroll_consumption,
                        PropertyBoundaryDagPlacement::Cutout {
                            marker: scroll_marker,
                            neutral_path: property_boundary_neutral_path(
                                arena,
                                transform.owner,
                                boundary.scroll.owner,
                            )
                            .ok_or_else(property_scene_error)?,
                            sealed,
                        },
                    )?;
                }
                [
                    PropertySceneScheduledStep::RetainedSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Effect(effect),
                        parent: None,
                    },
                    PropertySceneScheduledStep::ScrollBoundary { .. },
                ] => {
                    let effect_consumption = property_boundary_consumption(
                        boundary,
                        ConsumedPropertyBoundary::Effect(effect.id),
                    )
                    .ok_or_else(property_scene_error)?;
                    let effect_id = push_property_boundary_dag_node(
                        arena,
                        &mut nodes,
                        root.ordinal,
                        PropertyBoundaryReceiverScope::FrameRoot {
                            scene_root_ordinal: root.ordinal,
                        },
                        PropertyBoundaryDagNodeKind::Effect(*effect),
                        effect_consumption,
                        PropertyBoundaryDagPlacement::Root,
                    )?;
                    let insertion = effect_receiver_insertions
                        .iter()
                        .find(|insertion| {
                            insertion.scene_root_ordinal == root.ordinal
                                && insertion.scroll_boundary_ordinal == boundary.ordinal
                        })
                        .or_else(|| {
                            same_owner_effect_scroll_insertions
                                .iter()
                                .map(|insertion| &insertion.receiver)
                                .find(|insertion| {
                                    insertion.scene_root_ordinal == root.ordinal
                                        && insertion.scroll_boundary_ordinal == boundary.ordinal
                                })
                        });
                    let sealed = insertion.map(|insertion| {
                        property_boundary_insertion_seal(
                            insertion.insertion_index,
                            insertion.before_span.clone(),
                            insertion.after_span.clone(),
                            insertion.receiver_opaque_before,
                            insertion.receiver_opaque_after,
                            &insertion.recorded_steps,
                        )
                    });
                    push_property_boundary_dag_node(
                        arena,
                        &mut nodes,
                        root.ordinal,
                        PropertyBoundaryReceiverScope::Surface(effect_id),
                        PropertyBoundaryDagNodeKind::Scroll(boundary.clone()),
                        scroll_consumption,
                        PropertyBoundaryDagPlacement::Cutout {
                            marker: scroll_marker,
                            neutral_path: property_boundary_neutral_path(
                                arena,
                                effect.owner,
                                boundary.scroll.owner,
                            )
                            .ok_or_else(property_scene_error)?,
                            sealed,
                        },
                    )?;
                }
                [
                    PropertySceneScheduledStep::RetainedSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Transform(transform),
                        parent: None,
                    },
                    PropertySceneScheduledStep::RetainedSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Effect(effect),
                        parent: Some(PropertyScheduledSurfaceBoundaryId::Transform(parent)),
                    },
                    PropertySceneScheduledStep::ScrollBoundary { .. },
                ] if *parent == transform.id => {
                    let insertion = transform_effect_receiver_insertions
                        .iter()
                        .find(|insertion| {
                            insertion.scene_root_ordinal == root.ordinal
                                && insertion.inner.scroll_boundary_ordinal == boundary.ordinal
                        })
                        .ok_or_else(property_scene_error)?;
                    let transform_consumption = property_boundary_consumption(
                        boundary,
                        ConsumedPropertyBoundary::Transform(transform.id),
                    )
                    .ok_or_else(property_scene_error)?;
                    let transform_id = push_property_boundary_dag_node(
                        arena,
                        &mut nodes,
                        root.ordinal,
                        PropertyBoundaryReceiverScope::FrameRoot {
                            scene_root_ordinal: root.ordinal,
                        },
                        PropertyBoundaryDagNodeKind::Transform(*transform),
                        transform_consumption,
                        PropertyBoundaryDagPlacement::Root,
                    )?;
                    let effect_marker = insertion.effect_cutout;
                    let effect_consumption = property_boundary_consumption(
                        boundary,
                        ConsumedPropertyBoundary::Effect(effect.id),
                    )
                    .ok_or_else(property_scene_error)?;
                    let effect_id = push_property_boundary_dag_node(
                        arena,
                        &mut nodes,
                        root.ordinal,
                        PropertyBoundaryReceiverScope::Surface(transform_id),
                        PropertyBoundaryDagNodeKind::Effect(*effect),
                        effect_consumption,
                        PropertyBoundaryDagPlacement::Cutout {
                            marker: effect_marker,
                            neutral_path: property_boundary_neutral_path(
                                arena,
                                transform.owner,
                                effect.owner,
                            )
                            .ok_or_else(property_scene_error)?,
                            sealed: Some(property_boundary_insertion_seal(
                                insertion.outer_insertion_index,
                                insertion.outer_before_span.clone(),
                                insertion.outer_after_span.clone(),
                                insertion.outer_opaque_before,
                                insertion.outer_opaque_after,
                                &insertion.outer_recorded_steps,
                            )),
                        },
                    )?;
                    push_property_boundary_dag_node(
                        arena,
                        &mut nodes,
                        root.ordinal,
                        PropertyBoundaryReceiverScope::Surface(effect_id),
                        PropertyBoundaryDagNodeKind::Scroll(boundary.clone()),
                        scroll_consumption,
                        PropertyBoundaryDagPlacement::Cutout {
                            marker: scroll_marker,
                            neutral_path: property_boundary_neutral_path(
                                arena,
                                effect.owner,
                                boundary.scroll.owner,
                            )
                            .ok_or_else(property_scene_error)?,
                            sealed: Some(property_boundary_insertion_seal(
                                insertion.inner.insertion_index,
                                insertion.inner.before_span.clone(),
                                insertion.inner.after_span.clone(),
                                insertion.inner.receiver_opaque_before,
                                insertion.inner.receiver_opaque_after,
                                &insertion.inner.recorded_steps,
                            )),
                        },
                    )?;
                }
                [
                    PropertySceneScheduledStep::RetainedSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Effect(effect),
                        parent: None,
                    },
                    PropertySceneScheduledStep::RetainedSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Transform(transform),
                        parent: Some(PropertyScheduledSurfaceBoundaryId::Effect(parent)),
                    },
                    PropertySceneScheduledStep::ScrollBoundary { .. },
                ] if *parent == effect.id => {
                    let insertion = effect_transform_receiver_insertions
                        .iter()
                        .find(|insertion| {
                            insertion.scene_root_ordinal == root.ordinal
                                && insertion.inner.scroll_boundary_ordinal == boundary.ordinal
                        })
                        .ok_or_else(property_scene_error)?;
                    let effect_consumption = property_boundary_consumption(
                        boundary,
                        ConsumedPropertyBoundary::Effect(effect.id),
                    )
                    .ok_or_else(property_scene_error)?;
                    let effect_id = push_property_boundary_dag_node(
                        arena,
                        &mut nodes,
                        root.ordinal,
                        PropertyBoundaryReceiverScope::FrameRoot {
                            scene_root_ordinal: root.ordinal,
                        },
                        PropertyBoundaryDagNodeKind::Effect(*effect),
                        effect_consumption,
                        PropertyBoundaryDagPlacement::Root,
                    )?;
                    let transform_consumption = property_boundary_consumption(
                        boundary,
                        ConsumedPropertyBoundary::Transform(transform.id),
                    )
                    .ok_or_else(property_scene_error)?;
                    let transform_id = push_property_boundary_dag_node(
                        arena,
                        &mut nodes,
                        root.ordinal,
                        PropertyBoundaryReceiverScope::Surface(effect_id),
                        PropertyBoundaryDagNodeKind::Transform(*transform),
                        transform_consumption,
                        PropertyBoundaryDagPlacement::Cutout {
                            marker: insertion.transform_cutout,
                            neutral_path: property_boundary_neutral_path(
                                arena,
                                effect.owner,
                                transform.owner,
                            )
                            .ok_or_else(property_scene_error)?,
                            sealed: Some(property_boundary_insertion_seal(
                                insertion.outer_insertion_index,
                                insertion.outer_before_span.clone(),
                                insertion.outer_after_span.clone(),
                                insertion.outer_opaque_before,
                                insertion.outer_opaque_after,
                                &insertion.outer_recorded_steps,
                            )),
                        },
                    )?;
                    push_property_boundary_dag_node(
                        arena,
                        &mut nodes,
                        root.ordinal,
                        PropertyBoundaryReceiverScope::Surface(transform_id),
                        PropertyBoundaryDagNodeKind::Scroll(boundary.clone()),
                        scroll_consumption,
                        PropertyBoundaryDagPlacement::Cutout {
                            marker: scroll_marker,
                            neutral_path: property_boundary_neutral_path(
                                arena,
                                transform.owner,
                                boundary.scroll.owner,
                            )
                            .ok_or_else(property_scene_error)?,
                            sealed: Some(property_boundary_insertion_seal(
                                insertion.inner.insertion_index,
                                insertion.inner.before_span.clone(),
                                insertion.inner.after_span.clone(),
                                insertion.inner.receiver_opaque_before,
                                insertion.inner.receiver_opaque_after,
                                &insertion.inner.recorded_steps,
                            )),
                        },
                    )?;
                }
                [
                    PropertySceneScheduledStep::ScrollBoundary { .. },
                    PropertySceneScheduledStep::ScrollContentSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Effect(effect),
                        scroll,
                    },
                ] if *scroll == boundary.scroll.id => {
                    let content_insertion = scroll_content_effect_insertions
                        .iter()
                        .find(|insertion| {
                            insertion.scene_root_ordinal == root.ordinal
                                && insertion.scroll_boundary_ordinal == boundary.ordinal
                                && insertion.effect == *effect
                        })
                        .ok_or_else(property_scene_error)?;
                    let insertion = frame_receiver_insertions.iter().find(|insertion| {
                        insertion.scene_root_ordinal == root.ordinal
                            && insertion.scroll_boundary_ordinal == boundary.ordinal
                    });
                    let scroll_id = push_property_boundary_dag_node(
                        arena,
                        &mut nodes,
                        root.ordinal,
                        PropertyBoundaryReceiverScope::FrameRoot {
                            scene_root_ordinal: root.ordinal,
                        },
                        PropertyBoundaryDagNodeKind::Scroll(boundary.clone()),
                        scroll_consumption,
                        PropertyBoundaryDagPlacement::Cutout {
                            marker: scroll_marker,
                            neutral_path: property_boundary_neutral_path(
                                arena,
                                root.root,
                                boundary.scroll.owner,
                            )
                            .ok_or_else(property_scene_error)?,
                            sealed: insertion.map(|insertion| {
                                property_boundary_insertion_seal(
                                    insertion.insertion_index,
                                    insertion.before_span.clone(),
                                    insertion.after_span.clone(),
                                    insertion.receiver_opaque_before,
                                    insertion.receiver_opaque_after,
                                    &insertion.recorded_steps,
                                )
                            }),
                        },
                    )?;
                    let effect_consumption =
                        property_scroll_content_effect_consumption(property_trees, *effect)
                            .ok_or_else(property_scene_error)?;
                    let effect_marker = super::PlannedBoundary {
                        root: effect.owner,
                        stable_id: arena
                            .get(effect.owner)
                            .map(|node| node.element.stable_id())
                            .filter(|stable_id| *stable_id != 0)
                            .ok_or_else(property_scene_error)?,
                        kind: super::PlannedBoundaryKind::Isolation(effect.id),
                    };
                    push_property_boundary_dag_node(
                        arena,
                        &mut nodes,
                        root.ordinal,
                        PropertyBoundaryReceiverScope::ScrollContent(scroll_id),
                        PropertyBoundaryDagNodeKind::Effect(*effect),
                        effect_consumption,
                        PropertyBoundaryDagPlacement::Cutout {
                            marker: effect_marker,
                            neutral_path: property_boundary_neutral_path(
                                arena,
                                boundary.scroll.owner,
                                effect.owner,
                            )
                            .ok_or_else(property_scene_error)?,
                            sealed: Some(property_boundary_insertion_seal(
                                content_insertion.insertion_index,
                                content_insertion.before_span.clone(),
                                content_insertion.after_span.clone(),
                                content_insertion.receiver_opaque_before,
                                content_insertion.receiver_opaque_after,
                                &content_insertion.receiver_recorded_steps,
                            )),
                        },
                    )?;
                }
                [
                    PropertySceneScheduledStep::RetainedSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Transform(transform),
                        parent: None,
                    },
                    PropertySceneScheduledStep::ScrollBoundary { .. },
                    PropertySceneScheduledStep::ScrollContentSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Effect(effect),
                        scroll,
                    },
                ] if *scroll == boundary.scroll.id => {
                    let content_insertion = scroll_content_effect_insertions
                        .iter()
                        .find(|insertion| {
                            insertion.scene_root_ordinal == root.ordinal
                                && insertion.scroll_boundary_ordinal == boundary.ordinal
                                && insertion.effect == *effect
                        })
                        .ok_or_else(property_scene_error)?;
                    let transform_consumption = property_boundary_consumption(
                        boundary,
                        ConsumedPropertyBoundary::Transform(transform.id),
                    )
                    .ok_or_else(property_scene_error)?;
                    let transform_id = push_property_boundary_dag_node(
                        arena,
                        &mut nodes,
                        root.ordinal,
                        PropertyBoundaryReceiverScope::FrameRoot {
                            scene_root_ordinal: root.ordinal,
                        },
                        PropertyBoundaryDagNodeKind::Transform(*transform),
                        transform_consumption,
                        PropertyBoundaryDagPlacement::Root,
                    )?;
                    let insertion = receiver_insertions.iter().find(|insertion| {
                        insertion.scene_root_ordinal == root.ordinal
                            && insertion.scroll_boundary_ordinal == boundary.ordinal
                    });
                    let scroll_id = push_property_boundary_dag_node(
                        arena,
                        &mut nodes,
                        root.ordinal,
                        PropertyBoundaryReceiverScope::Surface(transform_id),
                        PropertyBoundaryDagNodeKind::Scroll(boundary.clone()),
                        scroll_consumption,
                        PropertyBoundaryDagPlacement::Cutout {
                            marker: scroll_marker,
                            neutral_path: property_boundary_neutral_path(
                                arena,
                                transform.owner,
                                boundary.scroll.owner,
                            )
                            .ok_or_else(property_scene_error)?,
                            sealed: insertion.map(|insertion| {
                                property_boundary_insertion_seal(
                                    insertion.insertion_index,
                                    insertion.before_span.clone(),
                                    insertion.after_span.clone(),
                                    insertion.receiver_opaque_before,
                                    insertion.receiver_opaque_after,
                                    &insertion.recorded_steps,
                                )
                            }),
                        },
                    )?;
                    let effect_consumption =
                        property_scroll_content_effect_consumption(property_trees, *effect)
                            .ok_or_else(property_scene_error)?;
                    let effect_marker = super::PlannedBoundary {
                        root: effect.owner,
                        stable_id: arena
                            .get(effect.owner)
                            .map(|node| node.element.stable_id())
                            .filter(|stable_id| *stable_id != 0)
                            .ok_or_else(property_scene_error)?,
                        kind: super::PlannedBoundaryKind::Isolation(effect.id),
                    };
                    push_property_boundary_dag_node(
                        arena,
                        &mut nodes,
                        root.ordinal,
                        PropertyBoundaryReceiverScope::ScrollContent(scroll_id),
                        PropertyBoundaryDagNodeKind::Effect(*effect),
                        effect_consumption,
                        PropertyBoundaryDagPlacement::Cutout {
                            marker: effect_marker,
                            neutral_path: property_boundary_neutral_path(
                                arena,
                                boundary.scroll.owner,
                                effect.owner,
                            )
                            .ok_or_else(property_scene_error)?,
                            sealed: Some(property_boundary_insertion_seal(
                                content_insertion.insertion_index,
                                content_insertion.before_span.clone(),
                                content_insertion.after_span.clone(),
                                content_insertion.receiver_opaque_before,
                                content_insertion.receiver_opaque_after,
                                &content_insertion.receiver_recorded_steps,
                            )),
                        },
                    )?;
                }
                _ => return Err(property_scene_error()),
            }
        }
        dag_roots.push(PropertyBoundaryDagRoot {
            scene_root_ordinal: root.ordinal,
            root: root.root,
            stable_id: root.stable_id,
            node_span: node_start..nodes.len(),
        });
    }
    Ok(PropertyBoundaryDag {
        roots: dag_roots,
        nodes,
    })
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
        if boundary.scroll.id != *scroll {
            return Err(property_scene_error());
        }
        if boundary.scroll.owner == receiver.owner {
            continue;
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

#[allow(clippy::too_many_arguments)]
fn plan_same_owner_transform_scroll_receiver_insertions(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    roots: &[PropertyScrollScheduleRoot],
    schedule: &PropertySceneSchedule,
    boundaries: &[PropertyScrollBoundaryContract],
) -> Result<Vec<PropertySameOwnerTransformScrollReceiverInsertionContract>, FramePaintPlanError> {
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
            continue;
        };
        let boundary = boundaries
            .get(*boundary_ordinal as usize)
            .ok_or_else(property_scene_error)?;
        if receiver != basis
            || receiver.owner != root.root
            || receiver.owner != boundary.scroll.owner
            || boundary.scroll.id != *scroll
            || boundary.contents_clip.owner != receiver.owner
        {
            continue;
        }
        let receiver_node = arena.get(receiver.owner).ok_or_else(property_scene_error)?;
        let [content_root] = receiver_node.element.children() else {
            return Err(property_scene_error());
        };
        let content_node = arena.get(*content_root).ok_or_else(property_scene_error)?;
        let scroll_cutout = super::PlannedBoundary {
            root: receiver.owner,
            stable_id: root.stable_id,
            kind: super::PlannedBoundaryKind::Scroll(boundary.scroll.id),
        };
        let recorded =
            super::frame_recorder::record_same_owner_transform_scroll_receiver_steps_for_plan(
                arena,
                receiver.owner,
                property_trees,
                paint_generations,
                *receiver,
                boundary.scroll,
                boundary.contents_clip,
                scroll_cutout,
            )
            .map_err(|fallbacks| FramePaintPlanError {
                reasons: fallbacks
                    .into_iter()
                    .map(FramePaintPlanRejection::Coverage)
                    .collect(),
            })?;
        let [super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker)] =
            recorded.as_slice()
        else {
            return Err(property_scene_error());
        };
        if *marker != scroll_cutout {
            return Err(property_scene_error());
        }
        let receiver_contract = PropertyScrollReceiverInsertionContract {
            scene_root_ordinal: root.ordinal,
            receiver: *receiver,
            receiver_stable_id: root.stable_id,
            scroll_boundary_ordinal: *boundary_ordinal,
            scroll_cutout,
            insertion_index: 0,
            before_span: 0..0,
            after_span: 1..1,
            receiver_opaque_before: 0,
            receiver_opaque_after: 0,
            recorded_steps: vec![PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(
                scroll_cutout,
            )],
        };
        let contract = PropertySameOwnerTransformScrollReceiverInsertionContract {
            receiver: receiver_contract,
            owner: receiver.owner,
            stable_id: root.stable_id,
            transform: *receiver,
            scroll: boundary.scroll,
            contents_clip: boundary.contents_clip,
            content_root: *content_root,
            content_stable_id: content_node.element.stable_id(),
        };
        if !contract.is_canonical() {
            return Err(property_scene_error());
        }
        insertions.push(contract);
    }
    Ok(insertions)
}

fn plan_property_frame_scroll_receiver_insertions(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    context: TransformSurfacePlanContext,
    roots: &[PropertyScrollScheduleRoot],
    schedule: &PropertySceneSchedule,
    boundaries: &[PropertyScrollBoundaryContract],
) -> Result<Vec<PropertyFrameScrollReceiverInsertionContract>, FramePaintPlanError> {
    let mut insertions = Vec::new();
    for root in roots {
        let root_steps = schedule
            .steps
            .get(root.step_span.clone())
            .ok_or_else(property_scene_error)?;
        let [
            PropertySceneScheduledStep::ScrollBoundary {
                boundary_ordinal,
                scroll,
                basis: ScrollCompositeBasis::FrameRoot,
                ..
            },
        ] = root_steps
        else {
            continue;
        };
        let boundary = boundaries
            .get(*boundary_ordinal as usize)
            .ok_or_else(property_scene_error)?;
        if boundary.scroll.id != *scroll
            || context.paint_offset_bits != [0.0_f32.to_bits(); 2]
            || context.outer_scissor_rect().is_some()
        {
            return Err(property_scene_error());
        }
        let scroll_cutout = super::PlannedBoundary {
            root: boundary.scroll.owner,
            stable_id: arena
                .get(boundary.scroll.owner)
                .ok_or_else(property_scene_error)?
                .element
                .stable_id(),
            kind: super::PlannedBoundaryKind::Scroll(boundary.scroll.id),
        };
        let cutouts =
            super::PlannedBoundaryCutoutSet::from_iter([(scroll_cutout.root, scroll_cutout)]);
        let recorded = super::frame_recorder::record_property_scene_steps_for_plan(
            arena,
            &[root.root],
            property_trees,
            paint_generations,
            context.paint_offset(),
            &cutouts,
        )
        .map_err(|fallbacks| FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })?;
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
        insertions.push(PropertyFrameScrollReceiverInsertionContract {
            scene_root_ordinal: root.ordinal,
            receiver_root: root.root,
            receiver_stable_id: root.stable_id,
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
        if boundary.scroll.owner == receiver.owner {
            // Equal-owner E+S is admitted only by the dedicated typed
            // self-role planner below. The generic recorder keeps rejecting
            // self cutouts.
            continue;
        }
        if receiver != basis
            || receiver.id.0 != root.root
            || receiver.owner != root.root
            || receiver.parent.is_some()
            || receiver.generation == 0
            || !receiver.opacity.is_finite()
            || !(0.0..=1.0).contains(&receiver.opacity)
            || boundary.scroll.id != *scroll
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
fn plan_same_owner_effect_scroll_receiver_insertions(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    context: TransformSurfacePlanContext,
    roots: &[PropertyScrollScheduleRoot],
    schedule: &PropertySceneSchedule,
    boundaries: &[PropertyScrollBoundaryContract],
) -> Result<Vec<PropertySameOwnerEffectScrollReceiverInsertionContract>, FramePaintPlanError> {
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
            || boundary.scroll.owner != receiver.owner
            || boundary.contents_clip.owner != receiver.owner
            || context.paint_offset_bits != [0.0_f32.to_bits(); 2]
            || context.outer_scissor_rect().is_some()
        {
            continue;
        }
        let children = arena.children_of(receiver.owner);
        let [content_root] = children.as_slice() else {
            return Err(property_scene_error());
        };
        let content_stable_id = arena
            .get(*content_root)
            .map(|node| node.element.stable_id())
            .filter(|stable_id| *stable_id != 0)
            .ok_or_else(property_scene_error)?;
        let generations = paint_generations
            .local_generations_for(receiver.owner)
            .ok_or_else(property_scene_error)?;
        let artifact_contract = EffectPropertySurfaceArtifactContract::new(
            receiver.owner,
            root.stable_id,
            *receiver,
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
            root: receiver.owner,
            stable_id: root.stable_id,
            kind: super::PlannedBoundaryKind::Scroll(boundary.scroll.id),
        };
        let recorded =
            super::frame_recorder::record_same_owner_effect_scroll_receiver_steps_for_plan(
                arena,
                receiver.owner,
                property_trees,
                paint_generations,
                &artifact_contract,
                *receiver,
                boundary.scroll,
                boundary.contents_clip,
                scroll_cutout,
            )
            .map_err(|fallbacks| FramePaintPlanError {
                reasons: fallbacks
                    .into_iter()
                    .map(FramePaintPlanRejection::Coverage)
                    .collect(),
            })?;
        let [super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker)] =
            recorded.as_slice()
        else {
            return Err(property_scene_error());
        };
        if *marker != scroll_cutout {
            return Err(property_scene_error());
        }
        let raster_bounds_bits = [
            boundary.scroll.viewport.x.to_bits(),
            boundary.scroll.viewport.y.to_bits(),
            boundary.scroll.viewport.width.to_bits(),
            boundary.scroll.viewport.height.to_bits(),
        ];
        let recorded_steps = vec![PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(
            scroll_cutout,
        )];
        let receiver_contract = PropertyEffectScrollReceiverInsertionContract {
            scene_root_ordinal: root.ordinal,
            receiver: *receiver,
            receiver_stable_id: root.stable_id,
            scroll_boundary_ordinal: *boundary_ordinal,
            scroll_cutout,
            insertion_index: 0,
            before_span: 0..0,
            after_span: 1..1,
            receiver_opaque_before: 0,
            receiver_opaque_after: 0,
            raster_bounds_bits,
            artifact_contract: artifact_contract.clone(),
            raster_identity: PropertyEffectScrollReceiverRasterIdentity {
                receiver_owner: receiver.owner,
                receiver_stable_id: root.stable_id,
                raster_bounds_bits,
                local_raster_clips: Vec::new(),
                content: artifact_contract.content().to_vec(),
                recorded_steps: recorded_steps.clone(),
            },
            recorded_steps,
        };
        let insertion = PropertySameOwnerEffectScrollReceiverInsertionContract {
            receiver: receiver_contract,
            owner: receiver.owner,
            stable_id: root.stable_id,
            effect: *receiver,
            scroll: boundary.scroll,
            contents_clip: boundary.contents_clip,
            content_root: *content_root,
            content_stable_id,
        };
        if !insertion.is_canonical() {
            return Err(property_scene_error());
        }
        insertions.push(insertion);
    }
    Ok(insertions)
}

#[allow(clippy::too_many_arguments)]
fn plan_property_scroll_content_effect_insertions(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    context: TransformSurfacePlanContext,
    roots: &[PropertyScrollScheduleRoot],
    schedule: &PropertySceneSchedule,
    boundaries: &[PropertyScrollBoundaryContract],
) -> Result<Vec<PropertyScrollContentEffectInsertionContract>, FramePaintPlanError> {
    let mut insertions = Vec::new();
    for root in roots {
        let root_steps = schedule
            .steps
            .get(root.step_span.clone())
            .ok_or_else(property_scene_error)?;
        let (boundary_ordinal, scroll, effect, outer_transform) = match root_steps {
            [
                PropertySceneScheduledStep::ScrollBoundary {
                    boundary_ordinal,
                    scroll,
                    basis: ScrollCompositeBasis::FrameRoot,
                    ..
                },
                PropertySceneScheduledStep::ScrollContentSurface {
                    boundary: PropertyScheduledSurfaceBoundary::Effect(effect),
                    scroll: receiver,
                },
            ] if scroll == receiver => (*boundary_ordinal, *scroll, *effect, None),
            [
                PropertySceneScheduledStep::RetainedSurface {
                    boundary: PropertyScheduledSurfaceBoundary::Transform(transform),
                    parent: None,
                },
                PropertySceneScheduledStep::ScrollBoundary {
                    boundary_ordinal,
                    scroll,
                    basis: ScrollCompositeBasis::Transform(_),
                    ..
                },
                PropertySceneScheduledStep::ScrollContentSurface {
                    boundary: PropertyScheduledSurfaceBoundary::Effect(effect),
                    scroll: receiver,
                },
            ] if scroll == receiver => (*boundary_ordinal, *scroll, *effect, Some(*transform)),
            _ => continue,
        };
        let boundary = boundaries
            .get(boundary_ordinal as usize)
            .filter(|boundary| {
                boundary.scene_root_ordinal == root.ordinal && boundary.scroll.id == scroll
            })
            .ok_or_else(property_scene_error)?;
        let scroll_host = arena
            .get(boundary.scroll.owner)
            .ok_or_else(property_scene_error)?;
        let [content_root] = scroll_host.element.children() else {
            return Err(property_scene_error());
        };
        let content_root = *content_root;
        let content_stable_id = arena
            .get(content_root)
            .map(|node| node.element.stable_id())
            .filter(|stable_id| *stable_id != 0)
            .ok_or_else(property_scene_error)?;
        if effect.id.0 != effect.owner
            || effect.parent.is_some()
            || effect.generation == 0
            || !effect.opacity.is_finite()
            || !(0.0..=1.0).contains(&effect.opacity)
            || effect.owner == content_root
            || property_boundary_neutral_path(arena, content_root, effect.owner).is_none()
        {
            return Err(property_scene_error());
        }
        let effect_stable_id = arena
            .get(effect.owner)
            .map(|node| node.element.stable_id())
            .filter(|stable_id| *stable_id != 0)
            .ok_or_else(property_scene_error)?;
        let effect_chain = property_trees
            .effect_snapshot_for(Some(effect.id))
            .filter(|chain| chain.as_slice() == [effect])
            .ok_or_else(property_scene_error)?;
        let effect_state = property_trees
            .node_state_for(effect.owner)
            .ok_or_else(property_scene_error)?;
        if effect_state.descendants.effect != Some(effect.id)
            || effect_state.descendants.scroll != Some(boundary.scroll.id)
            || effect_state.descendants.clip != Some(boundary.contents_clip.id)
        {
            return Err(property_scene_error());
        }
        let live_clips = property_trees
            .clip_snapshot_for(effect_state.descendants.clip)
            .ok_or_else(property_scene_error)?;
        let contents_index = live_clips
            .iter()
            .position(|clip| clip.id == boundary.contents_clip.id)
            .ok_or_else(property_scene_error)?;
        let local_raster_clips = live_clips[..contents_index].to_vec();
        let detached_ancestor_clips = live_clips[contents_index..].to_vec();
        if detached_ancestor_clips.first() != Some(&boundary.contents_clip) {
            return Err(property_scene_error());
        }

        let mut content = Vec::new();
        let mut pending = vec![(effect.owner, None)];
        let mut seen = FxHashSet::default();
        while let Some((owner, parent)) = pending.pop() {
            if !seen.insert(owner) {
                return Err(property_scene_error());
            }
            let node = arena.get(owner).ok_or_else(property_scene_error)?;
            let stable_id = node.element.stable_id();
            let generations = paint_generations
                .local_generations_for(owner)
                .ok_or_else(property_scene_error)?;
            if stable_id == 0
                || node.element.is_deferred_to_root_viewport_render()
                || (owner != effect.owner
                    && (property_trees
                        .transforms
                        .contains_key(&TransformNodeId(owner))
                        || property_trees.effects.contains_key(&EffectNodeId(owner))
                        || property_trees.scrolls.contains_key(&ScrollNodeId(owner))))
            {
                return Err(property_scene_error());
            }
            content.push(EffectPropertyContentWitness {
                owner,
                stable_id,
                parent,
                self_paint_revision: generations.self_paint_revision,
                topology_revision: generations.topology_revision,
            });
            for &child in node.element.children().iter().rev() {
                pending.push((child, Some(owner)));
            }
        }
        let artifact_contract = EffectPropertySurfaceArtifactContract::new(
            effect.owner,
            effect_stable_id,
            effect,
            effect_chain,
            Vec::new(),
            local_raster_clips,
            detached_ancestor_clips,
            content,
        )
        .ok_or_else(property_scene_error)?;
        let content_witness = super::PaintScrollContentWitness::new(
            boundary.scroll.owner,
            content_root,
            boundary.scroll,
            boundary.contents_clip,
        )
        .ok_or_else(property_scene_error)?;
        let consumed_transform = outer_transform
            .map(|transform| {
                ConsumedAncestorTransformWitness::new(
                    transform.owner,
                    boundary.scroll.owner,
                    transform.id,
                )
                .ok_or_else(property_scene_error)
            })
            .transpose()?;
        let effect_cutout = super::PlannedBoundary {
            root: effect.owner,
            stable_id: effect_stable_id,
            kind: super::PlannedBoundaryKind::Isolation(effect.id),
        };
        let receiver_recorded =
            super::frame_recorder::record_scroll_content_effect_receiver_steps_for_plan(
                arena,
                property_trees,
                paint_generations,
                content_witness,
                effect_cutout,
                consumed_transform,
            )
            .map_err(|fallbacks| FramePaintPlanError {
                reasons: fallbacks
                    .into_iter()
                    .map(FramePaintPlanRejection::Coverage)
                    .collect(),
            })?;
        let effect_recorded =
            super::frame_recorder::record_scroll_content_effect_surface_steps_for_plan(
                arena,
                property_trees,
                paint_generations,
                content_witness,
                &artifact_contract,
                consumed_transform,
            )
            .map_err(|fallbacks| FramePaintPlanError {
                reasons: fallbacks
                    .into_iter()
                    .map(FramePaintPlanRejection::Coverage)
                    .collect(),
            })?;
        if effect_recorded.iter().any(|step| {
            matches!(
                step,
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(_)
            )
        }) {
            return Err(property_scene_error());
        }
        let receiver_recorded_steps = receiver_recorded
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
        let effect_recorded_steps = effect_recorded
            .iter()
            .map(|step| match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    if super::compiler::validate_effect_property_surface_artifact(
                        artifact,
                        &artifact_contract,
                    )
                    .is_none()
                    {
                        return None;
                    }
                    property_scroll_receiver_artifact_identity(artifact)
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(_) => None,
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(property_scene_error)?;
        let markers = receiver_recorded_steps
            .iter()
            .enumerate()
            .filter_map(|(index, step)| {
                matches!(step, PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(marker) if *marker == effect_cutout)
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        let [insertion_index] = markers.as_slice() else {
            return Err(property_scene_error());
        };
        let opaque_before = receiver_recorded_steps[..*insertion_index]
            .iter()
            .try_fold(0_u32, |cursor, step| match step {
                PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact) => {
                    cursor.checked_add(artifact.opaque_count)
                }
                PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(_) => None,
            })
            .ok_or_else(property_scene_error)?;
        let opaque_after = receiver_recorded_steps[*insertion_index + 1..]
            .iter()
            .try_fold(opaque_before, |cursor, step| match step {
                PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact) => {
                    cursor.checked_add(artifact.opaque_count)
                }
                PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(_) => None,
            })
            .ok_or_else(property_scene_error)?;
        let first_bounds = effect_recorded
            .iter()
            .find_map(|step| match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    artifact.chunks.first().map(|chunk| chunk.bounds)
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(_) => None,
            })
            .ok_or_else(property_scene_error)?;
        let effect_raster_bounds_bits = recorded_step_bounds_union(
            effect_recorded.iter().filter_map(|step| match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    Some(artifact.chunks.iter().map(|chunk| chunk.bounds))
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(_) => None,
            }),
            [
                first_bounds.x,
                first_bounds.y,
                first_bounds.width,
                first_bounds.height,
            ],
        )
        .ok_or_else(property_scene_error)?;
        let outer_transform = outer_transform
            .map(|transform| {
                if transform.id.0 != root.root
                    || transform.owner != root.root
                    || transform.parent.is_some()
                    || transform.generation == 0
                    || transform.viewport_matrix.to_cols_array().map(f32::to_bits)
                        != property_trees
                            .transform_snapshot_for(transform.id)
                            .map(|live| live.viewport_matrix.to_cols_array().map(f32::to_bits))
                            .ok_or_else(property_scene_error)?
                {
                    return Err(property_scene_error());
                }
                let receiver_node = arena
                    .get(transform.owner)
                    .ok_or_else(property_scene_error)?;
                let scroll_stable_id = arena
                    .get(boundary.scroll.owner)
                    .map(|node| node.element.stable_id())
                    .filter(|stable_id| *stable_id != 0)
                    .ok_or_else(property_scene_error)?;
                let scroll_cutout = super::PlannedBoundary {
                    root: boundary.scroll.owner,
                    stable_id: scroll_stable_id,
                    kind: super::PlannedBoundaryKind::Scroll(boundary.scroll.id),
                };
                let recorded =
                    super::frame_recorder::record_property_scroll_receiver_steps_for_plan(
                        arena,
                        transform.owner,
                        property_trees,
                        paint_generations,
                        PaintTransformSurfaceWitness::canonical_root(transform.owner),
                        context.paint_offset(),
                        scroll_cutout,
                    )
                    .map_err(|fallbacks| FramePaintPlanError {
                        reasons: fallbacks
                            .into_iter()
                            .map(FramePaintPlanRejection::Coverage)
                            .collect(),
                    })?;
                let raster_bounds_bits =
                    effect_scroll_receiver_raster_bounds(&recorded, boundary.scroll.viewport)
                        .ok_or_else(property_scene_error)?;
                let [x, y, width, height] = raster_bounds_bits.map(f32::from_bits);
                let receiver_element = receiver_node
                    .element
                    .as_any()
                    .downcast_ref::<crate::view::base_component::Element>()
                    .ok_or_else(property_scene_error)?;
                let geometry = receiver_element
                    .exact_transform_receiver_geometry_snapshot_for_raster_bounds(
                        crate::view::base_component::RetainedSurfaceBounds {
                            x,
                            y,
                            width,
                            height,
                            corner_radii: [0.0; 4],
                        },
                        context.paint_offset(),
                        context.outer_scissor_rect,
                    )
                    .ok_or_else(property_scene_error)?;
                if geometry
                    .viewport_transform
                    .to_cols_array()
                    .map(f32::to_bits)
                    != transform.viewport_matrix.to_cols_array().map(f32::to_bits)
                {
                    return Err(property_scene_error());
                }
                let recorded_steps = recorded
                    .iter()
                    .map(|step| match step {
                        super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                            property_scroll_receiver_artifact_identity(artifact)
                                .map(PropertyScrollReceiverRecordedStepIdentity::Artifact)
                        }
                        super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker) => {
                            Some(PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(*marker))
                        }
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
                Ok(PropertyScrollContentOuterTransformInsertionContract {
                    receiver: PropertyScrollReceiverInsertionContract {
                        scene_root_ordinal: root.ordinal,
                        receiver: transform,
                        receiver_stable_id: root.stable_id,
                        scroll_boundary_ordinal: boundary_ordinal,
                        scroll_cutout,
                        insertion_index: *insertion_index,
                        before_span: 0..*insertion_index,
                        after_span: *insertion_index + 1..recorded_steps.len(),
                        receiver_opaque_before,
                        receiver_opaque_after,
                        recorded_steps,
                    },
                    geometry,
                })
            })
            .transpose()?;
        insertions.push(PropertyScrollContentEffectInsertionContract {
            scene_root_ordinal: root.ordinal,
            scroll_boundary_ordinal: boundary_ordinal,
            content_root,
            content_stable_id,
            effect,
            effect_stable_id,
            effect_cutout,
            insertion_index: *insertion_index,
            before_span: 0..*insertion_index,
            after_span: *insertion_index + 1..receiver_recorded_steps.len(),
            receiver_opaque_before: opaque_before,
            receiver_opaque_after: opaque_after,
            effect_raster_bounds_bits,
            artifact_contract,
            consumed_transform,
            outer_transform,
            receiver_recorded_steps,
            effect_recorded_steps,
        });
    }
    Ok(insertions)
}

#[allow(clippy::too_many_arguments)]
fn plan_property_transform_effect_scroll_receiver_insertions(
    arena: &NodeArena,
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
        let same_owner = outer.owner == inner.owner;
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
            || (!same_owner
                && (arena.children_of(outer.owner) != [inner.owner]
                    || arena.parent_of(inner.owner) != Some(outer.owner)))
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
        let expected_outer_state = PropertyTreeState {
            transform: Some(outer.id),
            effect: same_owner.then_some(inner.id),
            ..Default::default()
        };
        if outer_state.paint != expected_outer_state
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
                property_trees,
                paint_generations,
                PaintTransformSurfaceWitness::canonical_root(outer.owner),
                context.paint_offset(),
                &outer_cutouts,
            )
            .map_err(&record_error)?;
        let inner_recorded = if same_owner {
            let consumed_transform =
                super::ConsumedSameOwnerTransformBoundaryWitness::new(outer.owner, outer.id)
                    .ok_or_else(property_scene_error)?;
            super::frame_recorder::record_same_owner_transform_effect_surface_steps_for_plan(
                arena,
                property_trees,
                paint_generations,
                &artifact_contract,
                context.paint_offset(),
                &super::PlannedBoundaryCutoutSet::from_iter([(scroll_cutout.root, scroll_cutout)]),
                consumed_transform,
            )
            .map_err(&record_error)?
        } else {
            let consumed_transform =
                ConsumedAncestorTransformWitness::new(outer.owner, inner.owner, outer.id)
                    .ok_or_else(property_scene_error)?;
            super::frame_recorder::record_property_effect_scroll_receiver_steps_for_plan(
                arena,
                inner.owner,
                property_trees,
                paint_generations,
                &artifact_contract,
                context.paint_offset(),
                scroll_cutout,
                Some(consumed_transform),
            )
            .map_err(&record_error)?
        };

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
                crate::view::base_component::RetainedSurfaceBounds {
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

#[allow(clippy::too_many_arguments)]
fn plan_property_effect_transform_scroll_receiver_insertions(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    context: TransformSurfacePlanContext,
    roots: &[PropertyScrollScheduleRoot],
    schedule: &PropertySceneSchedule,
    boundaries: &[PropertyScrollBoundaryContract],
) -> Result<Vec<PropertyEffectTransformScrollReceiverInsertionContract>, FramePaintPlanError> {
    let mut insertions = Vec::new();
    for root in roots {
        let root_steps = schedule
            .steps
            .get(root.step_span.clone())
            .ok_or_else(property_scene_error)?;
        let [
            PropertySceneScheduledStep::RetainedSurface {
                boundary: PropertyScheduledSurfaceBoundary::Effect(outer),
                parent: None,
            },
            PropertySceneScheduledStep::RetainedSurface {
                boundary: PropertyScheduledSurfaceBoundary::Transform(inner),
                parent: Some(PropertyScheduledSurfaceBoundaryId::Effect(parent)),
            },
            PropertySceneScheduledStep::ScrollBoundary {
                boundary_ordinal,
                scroll,
                basis: ScrollCompositeBasis::Transform(basis),
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
            || !outer.opacity.is_finite()
            || !(0.0..=1.0).contains(&outer.opacity)
            || inner.parent.is_some()
            || inner.generation == 0
            || super::compiler::direct_translation_bits(inner.viewport_matrix).is_none()
            || boundary.scroll.id != *scroll
            || property_boundary_neutral_path(arena, outer.owner, inner.owner).is_none()
            || property_boundary_neutral_path(arena, inner.owner, boundary.scroll.owner).is_none()
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
        if outer_state.paint.transform.is_some()
            || outer_state.paint.scroll.is_some()
            || outer_state.paint.effect != Some(outer.id)
            || inner_state.paint.transform != Some(inner.id)
            || inner_state.paint.effect != Some(outer.id)
            || inner_state.paint.scroll.is_some()
        {
            return Err(property_scene_error());
        }

        let outer_stable_id = arena
            .get(outer.owner)
            .map(|node| node.element.stable_id())
            .filter(|stable_id| *stable_id != 0)
            .ok_or_else(property_scene_error)?;
        let inner_stable_id = arena
            .get(inner.owner)
            .map(|node| node.element.stable_id())
            .filter(|stable_id| *stable_id != 0)
            .ok_or_else(property_scene_error)?;
        let mut content = Vec::new();
        let mut pending = vec![outer.owner];
        let mut content_seen = FxHashSet::default();
        while let Some(owner) = pending.pop() {
            if owner == inner.owner {
                continue;
            }
            if !content_seen.insert(owner) {
                return Err(property_scene_error());
            }
            let node = arena.get(owner).ok_or_else(property_scene_error)?;
            let generations = paint_generations
                .local_generations_for(owner)
                .ok_or_else(property_scene_error)?;
            content.push(EffectPropertyContentWitness {
                owner,
                stable_id: node.element.stable_id(),
                parent: (owner != outer.owner)
                    .then(|| arena.parent_of(owner))
                    .flatten(),
                self_paint_revision: generations.self_paint_revision,
                topology_revision: generations.topology_revision,
            });
            pending.extend(node.element.children().iter().rev().copied());
        }
        let live_effect_chain = property_trees
            .effect_snapshot_for(Some(outer.id))
            .ok_or_else(property_scene_error)?;
        let isolated_outer = EffectNodeSnapshot {
            parent: None,
            ..*outer
        };
        let outer_clips = property_trees
            .clip_snapshot_for(outer_state.paint.clip)
            .unwrap_or_default();
        let outer_artifact_contract = EffectPropertySurfaceArtifactContract::new(
            outer.owner,
            outer_stable_id,
            isolated_outer,
            live_effect_chain.clone(),
            live_effect_chain[1..].to_vec(),
            outer_clips,
            Vec::new(),
            content,
        )
        .ok_or_else(property_scene_error)?;

        let transform_cutout = super::PlannedBoundary {
            root: inner.owner,
            stable_id: inner_stable_id,
            kind: super::PlannedBoundaryKind::Transform(inner.id),
        };
        let scroll_cutout = super::PlannedBoundary {
            root: boundary.scroll.owner,
            stable_id: arena
                .get(boundary.scroll.owner)
                .map(|node| node.element.stable_id())
                .filter(|stable_id| *stable_id != 0)
                .ok_or_else(property_scene_error)?,
            kind: super::PlannedBoundaryKind::Scroll(boundary.scroll.id),
        };
        let record_error = |fallbacks: Vec<FrameArtifactFallbackReason>| FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        };
        let outer_cutouts =
            super::PlannedBoundaryCutoutSet::from_iter([(inner.owner, transform_cutout)]);
        let outer_recorded = super::frame_recorder::record_effect_property_surface_steps_for_plan(
            arena,
            property_trees,
            paint_generations,
            &outer_artifact_contract,
            context.paint_offset(),
            &outer_cutouts,
            None,
        )
        .map_err(record_error)?;
        let inner_cutouts =
            super::PlannedBoundaryCutoutSet::from_iter([(boundary.scroll.owner, scroll_cutout)]);
        let consumed_effect = crate::view::paint::ConsumedAncestorEffectWitness::new(
            outer.owner,
            inner.owner,
            *outer,
            Some(outer.id),
            outer.parent,
        )
        .ok_or_else(property_scene_error)?;
        let inner_recorded =
            super::frame_recorder::record_effect_transform_property_surface_steps_for_plan(
                arena,
                inner.owner,
                property_trees,
                paint_generations,
                PaintTransformSurfaceWitness::canonical_root(inner.owner),
                context.paint_offset(),
                &inner_cutouts,
                consumed_effect,
            )
            .map_err(record_error)?;

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
        let (outer_steps, outer_index, outer_before, outer_after) =
            seal_steps(&outer_recorded, transform_cutout).ok_or_else(property_scene_error)?;
        let (inner_steps, inner_index, inner_before, inner_after) =
            seal_steps(&inner_recorded, scroll_cutout).ok_or_else(property_scene_error)?;

        let inner_raster_bounds_bits =
            effect_scroll_receiver_raster_bounds(&inner_recorded, boundary.scroll.viewport)
                .ok_or_else(property_scene_error)?;
        let inner_bounds = inner_raster_bounds_bits.map(f32::from_bits);
        let inner_node = arena.get(inner.owner).ok_or_else(property_scene_error)?;
        let inner_element = inner_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .ok_or_else(property_scene_error)?;
        let inner_geometry = inner_element
            .exact_transform_receiver_geometry_snapshot_for_raster_bounds(
                crate::view::base_component::RetainedSurfaceBounds {
                    x: inner_bounds[0],
                    y: inner_bounds[1],
                    width: inner_bounds[2],
                    height: inner_bounds[3],
                    corner_radii: [0.0; 4],
                },
                context.paint_offset(),
                None,
            )
            .ok_or_else(property_scene_error)?;
        let min_x = inner_geometry
            .quad_positions
            .iter()
            .map(|point| point[0])
            .fold(f32::INFINITY, f32::min);
        let min_y = inner_geometry
            .quad_positions
            .iter()
            .map(|point| point[1])
            .fold(f32::INFINITY, f32::min);
        let max_x = inner_geometry
            .quad_positions
            .iter()
            .map(|point| point[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = inner_geometry
            .quad_positions
            .iter()
            .map(|point| point[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let outer_raster_bounds_bits = recorded_step_bounds_union(
            outer_recorded.iter().filter_map(|step| match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    Some(artifact.chunks.iter().map(|chunk| chunk.bounds))
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(_) => None,
            }),
            [min_x, min_y, max_x - min_x, max_y - min_y],
        )
        .ok_or_else(property_scene_error)?;

        let inner_insertion = PropertyScrollReceiverInsertionContract {
            scene_root_ordinal: root.ordinal,
            receiver: *inner,
            receiver_stable_id: inner_stable_id,
            scroll_boundary_ordinal: *boundary_ordinal,
            scroll_cutout,
            insertion_index: inner_index,
            before_span: 0..inner_index,
            after_span: inner_index + 1..inner_steps.len(),
            receiver_opaque_before: inner_before,
            receiver_opaque_after: inner_after,
            recorded_steps: inner_steps,
        };
        insertions.push(PropertyEffectTransformScrollReceiverInsertionContract {
            scene_root_ordinal: root.ordinal,
            outer_receiver: *outer,
            outer_stable_id,
            outer_artifact_contract,
            outer_raster_bounds_bits,
            transform_cutout,
            outer_insertion_index: outer_index,
            outer_before_span: 0..outer_index,
            outer_after_span: outer_index + 1..outer_steps.len(),
            outer_opaque_before: outer_before,
            outer_opaque_after: outer_after,
            inner_geometry,
            inner: inner_insertion,
            outer_recorded_steps: outer_steps,
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
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    context: TransformSurfacePlanContext,
) -> Result<FramePaintPlan, FramePaintPlanError> {
    let mut plan = plan_property_effect_scene_scaffold_with_context(
        arena,
        roots,
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
        .map(|surface| (surface.boundary, surface.ordinal))
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
    let mut root_step_schedule = Vec::with_capacity(roots.len());
    let mut scene_cursor = 0_u32;
    let mut built = FxHashSet::default();
    for (root_ordinal, &root) in roots.iter().enumerate() {
        let cutouts = property_effect_direct_cutouts(&scaffold, None, root_ordinal as u32)?;
        let recorded = super::frame_recorder::record_property_scene_steps_for_plan(
            arena,
            &[root],
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
                    let boundary_id = match boundary.kind {
                        super::PlannedBoundaryKind::Transform(id) => {
                            PropertyBoundaryId::Transform(id)
                        }
                        super::PlannedBoundaryKind::Isolation(id) => PropertyBoundaryId::Effect(id),
                        super::PlannedBoundaryKind::Scroll(_) => {
                            return Err(property_scene_error());
                        }
                    };
                    let ordinal = *ordinals
                        .get(&boundary_id)
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
        let root_schedule = scene_steps[start..]
            .iter()
            .map(|step| match step {
                PaintPlanStep::ArtifactSpan(_) => Some(PropertyEffectRootStepKind::NormalArtifact),
                PaintPlanStep::RetainedSurface(surface) => {
                    let boundary = match surface.kind() {
                        SurfaceKind::Transform(plan) => {
                            PropertyBoundaryId::Transform(plan.transform)
                        }
                        SurfaceKind::Isolation(plan) => PropertyBoundaryId::Effect(plan.effect.id),
                        SurfaceKind::NestedIsolation(plan) => {
                            PropertyBoundaryId::Effect(plan.effect.id)
                        }
                        SurfaceKind::ScrollHost(_) => return None,
                    };
                    Some(PropertyEffectRootStepKind::LateBoundary(boundary))
                }
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(property_scene_error)?;
        root_step_schedule.push(root_schedule);
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
    scaffold.production_root_step_schedule = Some(root_step_schedule.clone());
    scaffold.planned_production_root_step_schedule = Some(root_step_schedule);
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
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scaffold: &PropertyEffectSceneScaffold,
    ordinal: u32,
    ordinals: &FxHashMap<PropertyBoundaryId, u32>,
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
            let geometry = exact_surface_geometry_for_plan(
                node.element.as_ref(),
                arena,
                owner,
                scaffold.context,
                Some(snapshot.viewport_matrix),
            )?;
            let witness = PaintTransformSurfaceWitness::canonical_root(owner);
            let recorded = super::frame_recorder::record_transform_property_surface_steps_for_plan(
                arena,
                owner,
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
            let parent_transform = surface.parent_boundary_ordinal.and_then(|parent| {
                let parent = &scaffold.surfaces[parent as usize];
                match parent.boundary {
                    PropertyBoundaryId::Transform(transform) => Some(transform),
                    PropertyBoundaryId::Effect(_) => None,
                }
            });
            let paint_offset = isolation.raster_space.paint_offset_bits.map(f32::from_bits);
            let recorded = if let Some(transform) = parent_transform
                && transform.0 == owner
            {
                let witness =
                    super::ConsumedSameOwnerTransformBoundaryWitness::new(owner, transform)
                        .ok_or_else(property_scene_error)?;
                super::frame_recorder::record_same_owner_transform_effect_surface_steps_for_plan(
                    arena,
                    property_trees,
                    paint_generations,
                    &artifact_contract,
                    paint_offset,
                    &cutouts,
                    witness,
                )
                .map_err(&record_error)?
            } else {
                let consumed_transform = parent_transform.and_then(|transform| {
                    ConsumedAncestorTransformWitness::new(transform.0, owner, transform)
                });
                super::frame_recorder::record_effect_property_surface_steps_for_plan(
                    arena,
                    property_trees,
                    paint_generations,
                    &artifact_contract,
                    paint_offset,
                    &cutouts,
                    consumed_transform,
                )
                .map_err(&record_error)?
            };
            let [x, y, width, height] = isolation
                .raster_space
                .source_bounds_bits
                .map(f32::from_bits);
            let geometry = NestedIsolationSurfaceGeometrySnapshot::from_exact_retained_output(
                crate::view::base_component::RetainedSurfaceBounds {
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
                let boundary_id = match boundary.kind {
                    super::PlannedBoundaryKind::Transform(id) => PropertyBoundaryId::Transform(id),
                    super::PlannedBoundaryKind::Isolation(id) => PropertyBoundaryId::Effect(id),
                    super::PlannedBoundaryKind::Scroll(_) => {
                        return Err(property_scene_error());
                    }
                };
                let child_ordinal = *ordinals
                    .get(&boundary_id)
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
        if !sampled_layout_transition_is_exact(node.element.as_ref()) {
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
            && arena.get(key).is_some_and(|node| {
                node.element.has_retained_transform_surface()
                    && node
                        .element
                        .compositor_viewport_transform_snapshot()
                        .is_some_and(|live| {
                            live.to_cols_array().map(f32::to_bits)
                                == snapshot.viewport_matrix.to_cols_array().map(f32::to_bits)
                        })
            });
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
        node.element.as_ref(),
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
    let production_schedule = scaffold.production_root_step_schedule.as_ref();
    if scaffold.production_root_step_spans != scaffold.planned_production_root_step_spans
        || scaffold.production_root_step_schedule != scaffold.planned_production_root_step_schedule
        || production_spans.is_some() != production_schedule.is_some()
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
        let same_owner_effect_half = ordinal.checked_sub(1).is_some_and(|parent_ordinal| {
            let parent = &scaffold.surfaces[parent_ordinal];
            matches!(parent.boundary, PropertyBoundaryId::Transform(_))
                && matches!(surface.boundary, PropertyBoundaryId::Effect(_))
                && surface.parent_boundary_ordinal == Some(parent.ordinal)
                && surface.boundary.owner() == parent.boundary.owner()
                && surface.stable_id == parent.stable_id
        });
        if surface.ordinal as usize != ordinal
            || surface.stable_id == 0
            || (!owners.insert(surface.boundary.owner()) && !same_owner_effect_half)
            || (!stable_ids.insert(surface.stable_id) && !same_owner_effect_half)
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
        property_effect_production_plan_is_canonical(
            plan,
            seal,
            scaffold,
            spans,
            production_schedule.expect("production span/schedule presence checked"),
        )
    })
}

fn property_effect_production_plan_is_canonical(
    plan: &FramePaintPlan,
    seal: &PropertyScenePlanSeal,
    scaffold: &PropertyEffectSceneScaffold,
    root_spans: &[Range<usize>],
    root_schedule: &[Vec<PropertyEffectRootStepKind>],
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
        || root_schedule.len() != scaffold.roots.len()
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
        if span.start != next_step
            || span.end < span.start
            || span.end > plan.steps.len()
            || root_schedule[root_ordinal].len() != span.len()
        {
            return false;
        }
        for (schedule_index, step_index) in span.clone().enumerate() {
            let actual_kind = match &plan.steps[step_index] {
                PaintPlanStep::ArtifactSpan(_) => PropertyEffectRootStepKind::NormalArtifact,
                PaintPlanStep::RetainedSurface(surface) => match surface.kind() {
                    SurfaceKind::Transform(plan) => PropertyEffectRootStepKind::LateBoundary(
                        PropertyBoundaryId::Transform(plan.transform),
                    ),
                    SurfaceKind::Isolation(plan) => PropertyEffectRootStepKind::LateBoundary(
                        PropertyBoundaryId::Effect(plan.effect.id),
                    ),
                    SurfaceKind::NestedIsolation(plan) => PropertyEffectRootStepKind::LateBoundary(
                        PropertyBoundaryId::Effect(plan.effect.id),
                    ),
                    SurfaceKind::ScrollHost(_) => return false,
                },
            };
            if root_schedule[root_ordinal].get(schedule_index) != Some(&actual_kind) {
                return false;
            }
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

fn property_boundary_insertion_seal_is_canonical(
    seal: &PropertyBoundaryInsertionSeal,
    marker: super::PlannedBoundary,
) -> bool {
    if seal.insertion_index >= seal.recorded_steps.len()
        || seal.before_span != (0..seal.insertion_index)
        || seal.after_span != (seal.insertion_index + 1..seal.recorded_steps.len())
    {
        return false;
    }
    let mut marker_count = 0usize;
    let mut opaque = 0_u32;
    for (index, step) in seal.recorded_steps.iter().enumerate() {
        match step {
            PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact) => {
                if index == seal.insertion_index {
                    return false;
                }
                let Some(next) = opaque.checked_add(artifact.opaque_count) else {
                    return false;
                };
                opaque = next;
            }
            PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(recorded_marker) => {
                marker_count += 1;
                if index != seal.insertion_index
                    || *recorded_marker != marker
                    || opaque != seal.receiver_opaque_before
                {
                    return false;
                }
            }
        }
    }
    marker_count == 1 && opaque == seal.receiver_opaque_after
}

fn property_boundary_consumption_is_canonical(node: &PropertyBoundaryDagNode) -> bool {
    let mut projected = node.consumption.expected_before;
    match (&node.kind, node.consumption.boundary) {
        (
            PropertyBoundaryDagNodeKind::Transform(transform),
            ConsumedPropertyBoundary::Transform(id),
        ) if id == transform.id && projected.transform == Some(id) => {
            projected.transform = None;
        }
        (PropertyBoundaryDagNodeKind::Effect(effect), ConsumedPropertyBoundary::Effect(id))
            if id == effect.id && projected.effect == Some(id) =>
        {
            projected.effect = None;
        }
        (
            PropertyBoundaryDagNodeKind::Scroll(boundary),
            ConsumedPropertyBoundary::ScrollContents {
                scroll,
                contents_clip,
            },
        ) if scroll == boundary.scroll.id
            && contents_clip == boundary.contents_clip.id
            && projected.scroll == Some(scroll)
            && projected.clip == Some(contents_clip) =>
        {
            projected.scroll = None;
            projected.clip = boundary.contents_clip.parent;
        }
        _ => return false,
    }
    projected == node.consumption.projected_after
}

fn property_boundary_dag_is_canonical(scaffold: &PropertyScrollScheduleScaffold) -> bool {
    if scaffold.boundary_dag.roots.len() != scaffold.roots.len() {
        return false;
    }
    let mut next_node = 0usize;
    let mut represented_scroll_boundaries = FxHashSet::default();
    for (root_index, (dag_root, schedule_root)) in scaffold
        .boundary_dag
        .roots
        .iter()
        .zip(&scaffold.roots)
        .enumerate()
    {
        if dag_root.scene_root_ordinal != schedule_root.ordinal
            || dag_root.scene_root_ordinal as usize != root_index
            || dag_root.root != schedule_root.root
            || dag_root.stable_id != schedule_root.stable_id
            || dag_root.stable_id == 0
            || dag_root.node_span.start != next_node
            || dag_root.node_span.end < dag_root.node_span.start
            || dag_root.node_span.end > scaffold.boundary_dag.nodes.len()
        {
            return false;
        }
        let Some(schedule_steps) = scaffold.schedule.steps.get(schedule_root.step_span.clone())
        else {
            return false;
        };
        let Some(dag_nodes) = scaffold.boundary_dag.nodes.get(dag_root.node_span.clone()) else {
            return false;
        };
        if dag_nodes.len() != schedule_steps.len() {
            return false;
        }
        for (local_index, (node, step)) in dag_nodes.iter().zip(schedule_steps).enumerate() {
            let absolute_index = dag_root.node_span.start + local_index;
            if node.id != PropertyBoundaryDagNodeId(absolute_index as u32)
                || node.scene_root_ordinal != dag_root.scene_root_ordinal
                || node.owner != node.kind.owner()
                || node.stable_id == 0
                || !property_boundary_consumption_is_canonical(node)
            {
                return false;
            }
            let expected_receiver = match step {
                PropertySceneScheduledStep::RetainedSurface { parent: None, .. } => {
                    PropertyBoundaryReceiverScope::FrameRoot {
                        scene_root_ordinal: dag_root.scene_root_ordinal,
                    }
                }
                PropertySceneScheduledStep::RetainedSurface {
                    parent: Some(parent),
                    ..
                } => {
                    let Some(parent_index) =
                        dag_nodes[..local_index].iter().position(|candidate| {
                            match (parent, &candidate.kind) {
                                (
                                    PropertyScheduledSurfaceBoundaryId::Transform(id),
                                    PropertyBoundaryDagNodeKind::Transform(transform),
                                ) => *id == transform.id,
                                (
                                    PropertyScheduledSurfaceBoundaryId::Effect(id),
                                    PropertyBoundaryDagNodeKind::Effect(effect),
                                ) => *id == effect.id,
                                _ => false,
                            }
                        })
                    else {
                        return false;
                    };
                    PropertyBoundaryReceiverScope::Surface(dag_nodes[parent_index].id)
                }
                PropertySceneScheduledStep::ScrollBoundary { basis, .. } => match basis {
                    ScrollCompositeBasis::FrameRoot => PropertyBoundaryReceiverScope::FrameRoot {
                        scene_root_ordinal: dag_root.scene_root_ordinal,
                    },
                    ScrollCompositeBasis::Transform(transform) => {
                        let Some(receiver) = dag_nodes[..local_index].iter().find(|candidate| {
                            matches!(&candidate.kind, PropertyBoundaryDagNodeKind::Transform(candidate) if candidate.id == transform.id)
                        }) else {
                            return false;
                        };
                        PropertyBoundaryReceiverScope::Surface(receiver.id)
                    }
                    ScrollCompositeBasis::Effect(effect) => {
                        let Some(receiver) = dag_nodes[..local_index].iter().find(|candidate| {
                            matches!(&candidate.kind, PropertyBoundaryDagNodeKind::Effect(candidate) if candidate.id == effect.id)
                        }) else {
                            return false;
                        };
                        PropertyBoundaryReceiverScope::Surface(receiver.id)
                    }
                },
                PropertySceneScheduledStep::ScrollContentSurface { scroll, .. } => {
                    let Some(receiver) = dag_nodes[..local_index].iter().find(|candidate| {
                        matches!(&candidate.kind, PropertyBoundaryDagNodeKind::Scroll(candidate) if candidate.scroll.id == *scroll)
                    }) else {
                        return false;
                    };
                    PropertyBoundaryReceiverScope::ScrollContent(receiver.id)
                }
            };
            if node.receiver != expected_receiver {
                return false;
            }
            match (step, &node.kind) {
                (
                    PropertySceneScheduledStep::RetainedSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Transform(expected),
                        ..
                    },
                    PropertyBoundaryDagNodeKind::Transform(actual),
                ) if actual == expected => {}
                (
                    PropertySceneScheduledStep::RetainedSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Effect(expected),
                        ..
                    },
                    PropertyBoundaryDagNodeKind::Effect(actual),
                ) if actual == expected => {}
                (
                    PropertySceneScheduledStep::ScrollBoundary {
                        boundary_ordinal,
                        scroll,
                        ..
                    },
                    PropertyBoundaryDagNodeKind::Scroll(actual),
                ) if actual.ordinal == *boundary_ordinal
                    && actual.scroll.id == *scroll
                    && scaffold.boundaries.get(*boundary_ordinal as usize) == Some(actual) =>
                {
                    if !represented_scroll_boundaries.insert(*boundary_ordinal) {
                        return false;
                    }
                }
                (
                    PropertySceneScheduledStep::ScrollContentSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Effect(expected),
                        scroll,
                    },
                    PropertyBoundaryDagNodeKind::Effect(actual),
                ) if actual == expected
                    && dag_nodes[..local_index].iter().any(|candidate| {
                        matches!(&candidate.kind, PropertyBoundaryDagNodeKind::Scroll(boundary) if boundary.scroll.id == *scroll)
                    }) => {}
                _ => return false,
            }
            match (&node.placement, &node.receiver, &node.kind) {
                (
                    PropertyBoundaryDagPlacement::Root,
                    PropertyBoundaryReceiverScope::FrameRoot { .. },
                    PropertyBoundaryDagNodeKind::Transform(_)
                    | PropertyBoundaryDagNodeKind::Effect(_),
                ) => {}
                (
                    PropertyBoundaryDagPlacement::Cutout {
                        marker,
                        neutral_path,
                        sealed,
                    },
                    _,
                    _,
                ) => {
                    if marker.root != node.owner
                        || marker.stable_id != node.stable_id
                        || marker.kind != node.kind.planned_boundary()
                    {
                        return false;
                    }
                    let mut neutral_owners = FxHashSet::default();
                    if neutral_path.iter().any(|witness| {
                        witness.stable_id == 0
                            || witness.owner == node.owner
                            || !neutral_owners.insert(witness.owner)
                    }) {
                        return false;
                    }
                    if sealed.as_ref().is_some_and(|seal| {
                        !property_boundary_insertion_seal_is_canonical(seal, *marker)
                    }) {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        next_node = dag_root.node_span.end;
    }
    next_node == scaffold.boundary_dag.nodes.len()
        && represented_scroll_boundaries.len() == scaffold.boundaries.len()
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
        || scaffold.same_owner_transform_scroll_insertions
            != scaffold.planned_same_owner_transform_scroll_insertions
        || scaffold.same_owner_effect_scroll_insertions
            != scaffold.planned_same_owner_effect_scroll_insertions
        || scaffold.frame_receiver_insertions != scaffold.planned_frame_receiver_insertions
        || scaffold.effect_receiver_insertions != scaffold.planned_effect_receiver_insertions
        || scaffold.transform_effect_receiver_insertions
            != scaffold.planned_transform_effect_receiver_insertions
        || scaffold.effect_transform_receiver_insertions
            != scaffold.planned_effect_transform_receiver_insertions
        || scaffold.scroll_content_effect_insertions
            != scaffold.planned_scroll_content_effect_insertions
        || scaffold.boundary_dag != scaffold.planned_boundary_dag
        || scaffold.roots.is_empty()
        || scaffold.boundaries.is_empty()
        || scaffold.boundaries.len() > scaffold.roots.len()
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
            || root.step_span.end < root.step_span.start
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
        if root.step_span.is_empty() {
            if !root_boundaries.is_empty() {
                return false;
            }
            next_step = root.step_span.end;
            continue;
        }
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
            || boundary.contents_clip.behavior != ClipBehavior::Intersect
            || boundary.contents_clip.generation == 0
            || !boundary
                .scroll
                .is_canonical_with_ancestor_contents_clip(boundary.contents_clip)
        {
            return false;
        }
        let mut expected_parent = boundary.contents_clip.parent;
        for ancestor in &boundary.ancestor_composite_clips {
            if Some(ancestor.id) != expected_parent
                || ancestor.owner != ancestor.id.owner
                || ancestor.generation == 0
            {
                return false;
            }
            expected_parent = ancestor.parent;
        }
        if expected_parent.is_some() {
            return false;
        }
        let mut clip_ids = FxHashSet::default();
        let extra_clips = boundary
            .ancestor_composite_clips
            .iter()
            .chain(&boundary.local_content_clips)
            .chain(&boundary.receiver_clips)
            .collect::<Vec<_>>();
        if !clip_ids.insert(boundary.contents_clip.id)
            || extra_clips.iter().any(|clip| !clip_ids.insert(clip.id))
            || extra_clips.iter().any(|clip| {
                clip.owner != clip.id.owner
                    || clip.generation == 0
                    || clip
                        .parent
                        .is_some_and(|parent| !clip_ids.contains(&parent))
            })
        {
            return false;
        }
        let stack = &boundary.consumed_properties;
        if stack.target_owner != boundary.scroll.owner
            || stack.entries.is_empty()
            || stack.entries.first().map(|entry| entry.expected_before) != Some(stack.live_input)
            || stack.entries.last().map(|entry| entry.projected_after)
                != Some(stack.projected_output)
            || stack.projected_output.clip != boundary.contents_clip.parent
            || stack.projected_output.transform.is_some()
            || stack.projected_output.effect.is_some()
            || stack.projected_output.scroll.is_some()
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
            let steps = &scaffold.schedule.steps[root.step_span.clone()];
            matches!(
                steps,
                [
                    PropertySceneScheduledStep::RetainedSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Transform(transform),
                        parent: None,
                    },
                    PropertySceneScheduledStep::ScrollBoundary {
                        basis: ScrollCompositeBasis::Transform(_),
                        ..
                    },
                ]
                if steps.iter().find_map(|step| match step {
                    PropertySceneScheduledStep::ScrollBoundary { scroll, .. } => Some(scroll.0),
                    _ => None,
                }) != Some(transform.owner)
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
    let eligible_same_owner_transform_scroll_insertions = scaffold
        .roots
        .iter()
        .filter(|root| {
            matches!(
                &scaffold.schedule.steps[root.step_span.clone()],
                [
                    PropertySceneScheduledStep::RetainedSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Transform(transform),
                        parent: None,
                    },
                    PropertySceneScheduledStep::ScrollBoundary { scroll, .. },
                ] if transform.owner == scroll.0
            )
        })
        .count();
    let mut same_owner_receivers = FxHashSet::default();
    let mut same_owner_boundaries = FxHashSet::default();
    let same_owner_insertions_are_canonical = scaffold
        .same_owner_transform_scroll_insertions
        .iter()
        .all(|insertion| {
            same_owner_receivers.insert(insertion.owner)
                && same_owner_boundaries.insert(insertion.receiver.scroll_boundary_ordinal)
                && property_same_owner_transform_scroll_insertion_is_canonical(scaffold, insertion)
        });
    let eligible_same_owner_effect_scroll_insertions = scaffold
        .roots
        .iter()
        .filter(|root| {
            matches!(
                &scaffold.schedule.steps[root.step_span.clone()],
                [
                    PropertySceneScheduledStep::RetainedSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Effect(effect),
                        parent: None,
                    },
                    PropertySceneScheduledStep::ScrollBoundary { scroll, .. },
                ] if effect.owner == scroll.0
            )
        })
        .count();
    let mut same_owner_effect_receivers = FxHashSet::default();
    let mut same_owner_effect_boundaries = FxHashSet::default();
    let same_owner_effect_insertions_are_canonical = scaffold
        .same_owner_effect_scroll_insertions
        .iter()
        .all(|insertion| {
            same_owner_effect_receivers.insert(insertion.owner)
                && same_owner_effect_boundaries.insert(insertion.receiver.scroll_boundary_ordinal)
                && property_same_owner_effect_scroll_insertion_is_canonical(scaffold, insertion)
        });
    let eligible_frame_receiver_insertions = scaffold
        .roots
        .iter()
        .filter(|root| {
            matches!(
                &scaffold.schedule.steps[root.step_span.clone()],
                [PropertySceneScheduledStep::ScrollBoundary {
                    basis: ScrollCompositeBasis::FrameRoot,
                    ..
                }]
            )
        })
        .count();
    let mut frame_insertion_roots = FxHashSet::default();
    let mut frame_insertion_boundaries = FxHashSet::default();
    let frame_insertions_are_canonical =
        scaffold.frame_receiver_insertions.iter().all(|insertion| {
            frame_insertion_roots.insert(insertion.receiver_root)
                && frame_insertion_boundaries.insert(insertion.scroll_boundary_ordinal)
                && property_frame_scroll_receiver_insertion_is_canonical(scaffold, insertion)
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
    let eligible_effect_transform_insertions = scaffold
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
                    PropertySceneScheduledStep::RetainedSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Transform(_),
                        parent: Some(PropertyScheduledSurfaceBoundaryId::Effect(_)),
                    },
                    PropertySceneScheduledStep::ScrollBoundary {
                        basis: ScrollCompositeBasis::Transform(_),
                        ..
                    },
                ]
            )
        })
        .count();
    let mut effect_transform_outer_receivers = FxHashSet::default();
    let mut effect_transform_inner_receivers = FxHashSet::default();
    let mut effect_transform_boundaries = FxHashSet::default();
    let effect_transform_insertions_are_canonical = scaffold
        .effect_transform_receiver_insertions
        .iter()
        .all(|insertion| {
            effect_transform_outer_receivers.insert(insertion.outer_receiver.id)
                && effect_transform_inner_receivers.insert(insertion.inner.receiver.id)
                && effect_transform_boundaries.insert(insertion.inner.scroll_boundary_ordinal)
                && property_effect_transform_scroll_receiver_insertion_is_canonical(
                    scaffold, insertion,
                )
        });
    let eligible_scroll_content_effect_insertions = scaffold
        .roots
        .iter()
        .filter(|root| {
            matches!(
                &scaffold.schedule.steps[root.step_span.clone()],
                [
                    PropertySceneScheduledStep::ScrollBoundary { .. },
                    PropertySceneScheduledStep::ScrollContentSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Effect(_),
                        ..
                    },
                ] | [
                    PropertySceneScheduledStep::RetainedSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Transform(_),
                        parent: None,
                    },
                    PropertySceneScheduledStep::ScrollBoundary { .. },
                    PropertySceneScheduledStep::ScrollContentSurface {
                        boundary: PropertyScheduledSurfaceBoundary::Effect(_),
                        ..
                    },
                ]
            )
        })
        .count();
    let mut scroll_content_effects = FxHashSet::default();
    let mut scroll_content_effect_boundaries = FxHashSet::default();
    let scroll_content_effect_insertions_are_canonical = scaffold
        .scroll_content_effect_insertions
        .iter()
        .all(|insertion| {
            scroll_content_effects.insert(insertion.effect.id)
                && scroll_content_effect_boundaries.insert(insertion.scroll_boundary_ordinal)
                && property_scroll_content_effect_insertion_is_canonical(scaffold, insertion)
        });
    next_step == scaffold.schedule.steps.len()
        && scaffold
            .boundaries
            .iter()
            .enumerate()
            .all(|(ordinal, boundary)| boundary.ordinal as usize == ordinal)
        && scaffold.receiver_insertions.len() <= eligible_receiver_insertions
        && insertions_are_canonical
        && scaffold.same_owner_transform_scroll_insertions.len()
            == eligible_same_owner_transform_scroll_insertions
        && same_owner_insertions_are_canonical
        && scaffold.same_owner_effect_scroll_insertions.len()
            == eligible_same_owner_effect_scroll_insertions
        && same_owner_effect_insertions_are_canonical
        && scaffold.frame_receiver_insertions.len() == eligible_frame_receiver_insertions
        && frame_insertions_are_canonical
        && scaffold.effect_receiver_insertions.len() <= eligible_effect_receiver_insertions
        && effect_insertions_are_canonical
        && scaffold.transform_effect_receiver_insertions.len()
            == eligible_transform_effect_insertions
        && transform_effect_insertions_are_canonical
        && scaffold.effect_transform_receiver_insertions.len()
            == eligible_effect_transform_insertions
        && effect_transform_insertions_are_canonical
        && scaffold.scroll_content_effect_insertions.len()
            == eligible_scroll_content_effect_insertions
        && scroll_content_effect_insertions_are_canonical
        && property_boundary_dag_is_canonical(scaffold)
}

fn property_scroll_content_effect_insertion_is_canonical(
    scaffold: &PropertyScrollScheduleScaffold,
    insertion: &PropertyScrollContentEffectInsertionContract,
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
    let schedule_matches = match root_steps {
        [
            PropertySceneScheduledStep::ScrollBoundary {
                boundary_ordinal,
                scroll,
                basis: ScrollCompositeBasis::FrameRoot,
                ..
            },
            PropertySceneScheduledStep::ScrollContentSurface {
                boundary: PropertyScheduledSurfaceBoundary::Effect(effect),
                scroll: receiver,
            },
        ] => {
            *boundary_ordinal == insertion.scroll_boundary_ordinal
                && *scroll == boundary.scroll.id
                && receiver == scroll
                && *effect == insertion.effect
        }
        [
            PropertySceneScheduledStep::RetainedSurface {
                boundary: PropertyScheduledSurfaceBoundary::Transform(transform),
                parent: None,
            },
            PropertySceneScheduledStep::ScrollBoundary {
                boundary_ordinal,
                scroll,
                basis: ScrollCompositeBasis::Transform(basis),
                ..
            },
            PropertySceneScheduledStep::ScrollContentSurface {
                boundary: PropertyScheduledSurfaceBoundary::Effect(effect),
                scroll: receiver,
            },
        ] => {
            transform == basis
                && *boundary_ordinal == insertion.scroll_boundary_ordinal
                && *scroll == boundary.scroll.id
                && receiver == scroll
                && *effect == insertion.effect
        }
        _ => false,
    };
    let expected_consumed_transform = match root_steps {
        [
            PropertySceneScheduledStep::ScrollBoundary { .. },
            PropertySceneScheduledStep::ScrollContentSurface { .. },
        ] => None,
        [
            PropertySceneScheduledStep::RetainedSurface {
                boundary: PropertyScheduledSurfaceBoundary::Transform(transform),
                parent: None,
            },
            PropertySceneScheduledStep::ScrollBoundary { .. },
            PropertySceneScheduledStep::ScrollContentSurface { .. },
        ] => ConsumedAncestorTransformWitness::new(
            transform.owner,
            boundary.scroll.owner,
            transform.id,
        ),
        _ => None,
    };
    let outer_transform_is_canonical = match (root_steps, &insertion.outer_transform) {
        (
            [
                PropertySceneScheduledStep::ScrollBoundary { .. },
                PropertySceneScheduledStep::ScrollContentSurface { .. },
            ],
            None,
        ) => true,
        (
            [
                PropertySceneScheduledStep::RetainedSurface {
                    boundary: PropertyScheduledSurfaceBoundary::Transform(transform),
                    parent: None,
                },
                PropertySceneScheduledStep::ScrollBoundary { .. },
                PropertySceneScheduledStep::ScrollContentSurface { .. },
            ],
            Some(outer),
        ) => property_scroll_content_outer_transform_insertion_is_canonical(
            root,
            boundary,
            *transform,
            outer,
            scaffold.context,
        ),
        _ => false,
    };
    if !schedule_matches
        || boundary.scene_root_ordinal != insertion.scene_root_ordinal
        || insertion.content_stable_id == 0
        || insertion.effect.id.0 != insertion.effect.owner
        || insertion.effect.parent.is_some()
        || insertion.effect.generation == 0
        || insertion.effect_stable_id == 0
        || insertion.effect_cutout.root != insertion.effect.owner
        || insertion.effect_cutout.stable_id != insertion.effect_stable_id
        || insertion.effect_cutout.kind
            != super::PlannedBoundaryKind::Isolation(insertion.effect.id)
        || insertion.artifact_contract.boundary_root() != insertion.effect.owner
        || insertion.artifact_contract.stable_id() != insertion.effect_stable_id
        || insertion.artifact_contract.isolated_leaf() != insertion.effect
        || !insertion.artifact_contract.is_canonical()
        || insertion.consumed_transform != expected_consumed_transform
        || !outer_transform_is_canonical
        || insertion.effect_recorded_steps.is_empty()
        || insertion
            .effect_raster_bounds_bits
            .map(f32::from_bits)
            .iter()
            .any(|value| !value.is_finite())
        || f32::from_bits(insertion.effect_raster_bounds_bits[2]) <= 0.0
        || f32::from_bits(insertion.effect_raster_bounds_bits[3]) <= 0.0
    {
        return false;
    }
    property_boundary_insertion_seal_is_canonical(
        &property_boundary_insertion_seal(
            insertion.insertion_index,
            insertion.before_span.clone(),
            insertion.after_span.clone(),
            insertion.receiver_opaque_before,
            insertion.receiver_opaque_after,
            &insertion.receiver_recorded_steps,
        ),
        insertion.effect_cutout,
    )
}

fn property_effect_transform_scroll_receiver_insertion_is_canonical(
    scaffold: &PropertyScrollScheduleScaffold,
    insertion: &PropertyEffectTransformScrollReceiverInsertionContract,
) -> bool {
    let Some(root) = scaffold.roots.get(insertion.scene_root_ordinal as usize) else {
        return false;
    };
    let Some(root_steps) = scaffold.schedule.steps.get(root.step_span.clone()) else {
        return false;
    };
    let [
        PropertySceneScheduledStep::RetainedSurface {
            boundary: PropertyScheduledSurfaceBoundary::Effect(outer),
            parent: None,
        },
        PropertySceneScheduledStep::RetainedSurface {
            boundary: PropertyScheduledSurfaceBoundary::Transform(inner),
            parent: Some(PropertyScheduledSurfaceBoundaryId::Effect(parent)),
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
    let Some(boundary) = scaffold.boundaries.get(*boundary_ordinal as usize) else {
        return false;
    };
    let inner_min_x = insertion
        .inner_geometry
        .quad_positions
        .iter()
        .map(|point| point[0])
        .fold(f32::INFINITY, f32::min);
    let inner_min_y = insertion
        .inner_geometry
        .quad_positions
        .iter()
        .map(|point| point[1])
        .fold(f32::INFINITY, f32::min);
    let inner_max_x = insertion
        .inner_geometry
        .quad_positions
        .iter()
        .map(|point| point[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let inner_max_y = insertion
        .inner_geometry
        .quad_positions
        .iter()
        .map(|point| point[1])
        .fold(f32::NEG_INFINITY, f32::max);
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
        [
            inner_min_x,
            inner_min_y,
            inner_max_x - inner_min_x,
            inner_max_y - inner_min_y,
        ],
    );
    if insertion.outer_receiver != *outer
        || outer.id != *parent
        || insertion.inner.receiver != *inner
        || insertion.inner.receiver != *basis
        || insertion.outer_receiver.owner != root.root
        || insertion.outer_receiver.parent.is_some()
        || insertion.outer_receiver.generation == 0
        || !insertion.outer_receiver.opacity.is_finite()
        || !(0.0..=1.0).contains(&insertion.outer_receiver.opacity)
        || insertion.outer_stable_id != root.stable_id
        || !insertion.outer_artifact_contract.is_canonical()
        || insertion.outer_artifact_contract.boundary_root() != root.root
        || insertion.outer_artifact_contract.stable_id() != root.stable_id
        || insertion.outer_artifact_contract.isolated_leaf() != insertion.outer_receiver
        || insertion.outer_artifact_contract.live_effect_chain() != [insertion.outer_receiver]
        || insertion.transform_cutout.root != inner.owner
        || insertion.transform_cutout.stable_id != insertion.inner.receiver_stable_id
        || !matches!(insertion.transform_cutout.kind, super::PlannedBoundaryKind::Transform(id) if id == inner.id)
        || insertion.inner.scene_root_ordinal != insertion.scene_root_ordinal
        || insertion.inner.scroll_boundary_ordinal != *boundary_ordinal
        || !insertion.inner_geometry.matches_rebuilt_contract()
        || insertion
            .inner_geometry
            .viewport_transform
            .to_cols_array()
            .map(f32::to_bits)
            != insertion
                .inner
                .receiver
                .viewport_matrix
                .to_cols_array()
                .map(f32::to_bits)
        || super::compiler::direct_translation_bits(insertion.inner_geometry.viewport_transform)
            .is_none()
        || expected_outer_bounds != Some(insertion.outer_raster_bounds_bits)
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
                    boundary: ConsumedPropertyBoundary::Effect(effect),
                    ..
                },
                ConsumedPropertyEntry {
                    boundary: ConsumedPropertyBoundary::Transform(transform),
                    ..
                },
                ConsumedPropertyEntry {
                    boundary: ConsumedPropertyBoundary::ScrollContents {
                        scroll: consumed_scroll,
                        contents_clip,
                    },
                    ..
                },
            ] if *effect == outer.id
                && *transform == inner.id
                && *consumed_scroll == boundary.scroll.id
                && *contents_clip == boundary.contents_clip.id
        )
        || insertion.inner.scroll_cutout.root != boundary.scroll.owner
        || insertion.inner.scroll_cutout.stable_id == 0
        || !matches!(insertion.inner.scroll_cutout.kind, super::PlannedBoundaryKind::Scroll(id) if id == boundary.scroll.id)
        || insertion.inner.insertion_index >= insertion.inner.recorded_steps.len()
        || insertion.inner.before_span != (0..insertion.inner.insertion_index)
        || insertion.inner.after_span
            != (insertion.inner.insertion_index + 1..insertion.inner.recorded_steps.len())
    {
        return false;
    }

    let seal_is_canonical = |steps: &[PropertyScrollReceiverRecordedStepIdentity],
                             marker: super::PlannedBoundary,
                             insertion_index: usize,
                             opaque_before: u32,
                             opaque_after: u32| {
        let mut marker_count = 0usize;
        let mut cursor = 0_u32;
        for (index, step) in steps.iter().enumerate() {
            match step {
                PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact) => {
                    if index == insertion_index
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
                PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(found) => {
                    marker_count += 1;
                    if index != insertion_index || *found != marker || cursor != opaque_before {
                        return false;
                    }
                }
            }
        }
        marker_count == 1 && cursor == opaque_after
    };
    seal_is_canonical(
        &insertion.outer_recorded_steps,
        insertion.transform_cutout,
        insertion.outer_insertion_index,
        insertion.outer_opaque_before,
        insertion.outer_opaque_after,
    ) && seal_is_canonical(
        &insertion.inner.recorded_steps,
        insertion.inner.scroll_cutout,
        insertion.inner.insertion_index,
        insertion.inner.receiver_opaque_before,
        insertion.inner.receiver_opaque_after,
    )
}

fn property_frame_scroll_receiver_insertion_is_canonical(
    scaffold: &PropertyScrollScheduleScaffold,
    insertion: &PropertyFrameScrollReceiverInsertionContract,
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
    let markers = insertion
        .recorded_steps
        .iter()
        .enumerate()
        .filter_map(|(index, step)| {
            matches!(step, PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(marker) if *marker == insertion.scroll_cutout)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    let [marker] = markers.as_slice() else {
        return false;
    };
    insertion.receiver_root == root.root
        && insertion.receiver_stable_id == root.stable_id
        && boundary.scene_root_ordinal == root.ordinal
        && insertion.scroll_cutout.root == boundary.scroll.owner
        && insertion.scroll_cutout.stable_id != 0
        && insertion.scroll_cutout.kind == super::PlannedBoundaryKind::Scroll(boundary.scroll.id)
        && insertion.insertion_index == *marker
        && insertion.before_span == (0..*marker)
        && insertion.after_span == (*marker + 1..insertion.recorded_steps.len())
        && insertion.receiver_opaque_before
            == insertion.recorded_steps[..*marker]
                .iter()
                .filter_map(|step| match step {
                    PropertyScrollReceiverRecordedStepIdentity::Artifact(identity) => {
                        Some(identity.opaque_count)
                    }
                    PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(_) => None,
                })
                .sum::<u32>()
        && insertion.receiver_opaque_after
            == insertion.recorded_steps[*marker + 1..]
                .iter()
                .filter_map(|step| match step {
                    PropertyScrollReceiverRecordedStepIdentity::Artifact(identity) => {
                        Some(identity.opaque_count)
                    }
                    PropertyScrollReceiverRecordedStepIdentity::ScrollCutout(_) => None,
                })
                .fold(insertion.receiver_opaque_before, u32::saturating_add)
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

fn property_same_owner_transform_scroll_insertion_is_canonical(
    scaffold: &PropertyScrollScheduleScaffold,
    insertion: &PropertySameOwnerTransformScrollReceiverInsertionContract,
) -> bool {
    let receiver = &insertion.receiver;
    let Some(root) = scaffold.roots.get(receiver.scene_root_ordinal as usize) else {
        return false;
    };
    let Some(boundary) = scaffold
        .boundaries
        .get(receiver.scroll_boundary_ordinal as usize)
    else {
        return false;
    };
    let Some(root_steps) = scaffold.schedule.steps.get(root.step_span.clone()) else {
        return false;
    };
    matches!(
        root_steps,
        [
            PropertySceneScheduledStep::RetainedSurface {
                boundary: PropertyScheduledSurfaceBoundary::Transform(transform),
                parent: None,
            },
            PropertySceneScheduledStep::ScrollBoundary {
                boundary_ordinal,
                scroll,
                basis: ScrollCompositeBasis::Transform(basis),
                ..
            },
        ] if *transform == insertion.transform
            && *basis == insertion.transform
            && *boundary_ordinal == receiver.scroll_boundary_ordinal
            && *scroll == insertion.scroll.id
    ) && insertion.is_canonical()
        && insertion.owner == root.root
        && insertion.stable_id == root.stable_id
        && insertion.scroll == boundary.scroll
        && insertion.contents_clip == boundary.contents_clip
        && insertion.receiver.scroll_boundary_ordinal == boundary.ordinal
        && insertion.content_root != insertion.owner
}

fn property_same_owner_effect_scroll_insertion_is_canonical(
    scaffold: &PropertyScrollScheduleScaffold,
    insertion: &PropertySameOwnerEffectScrollReceiverInsertionContract,
) -> bool {
    let receiver = &insertion.receiver;
    let Some(root) = scaffold.roots.get(receiver.scene_root_ordinal as usize) else {
        return false;
    };
    let Some(boundary) = scaffold
        .boundaries
        .get(receiver.scroll_boundary_ordinal as usize)
    else {
        return false;
    };
    let Some(root_steps) = scaffold.schedule.steps.get(root.step_span.clone()) else {
        return false;
    };
    matches!(
        root_steps,
        [
            PropertySceneScheduledStep::RetainedSurface {
                boundary: PropertyScheduledSurfaceBoundary::Effect(effect),
                parent: None,
            },
            PropertySceneScheduledStep::ScrollBoundary {
                boundary_ordinal,
                scroll,
                basis: ScrollCompositeBasis::Effect(basis),
                ..
            },
        ] if *effect == insertion.effect
            && *basis == insertion.effect
            && *boundary_ordinal == receiver.scroll_boundary_ordinal
            && *scroll == insertion.scroll.id
    ) && insertion.is_canonical()
        && insertion.owner == root.root
        && insertion.stable_id == root.stable_id
        && insertion.scroll == boundary.scroll
        && insertion.contents_clip == boundary.contents_clip
        && insertion.receiver.scroll_boundary_ordinal == boundary.ordinal
        && insertion.content_root != insertion.owner
}

fn property_scroll_content_outer_transform_insertion_is_canonical(
    root: &PropertyScrollScheduleRoot,
    boundary: &PropertyScrollBoundaryContract,
    expected: TransformNodeSnapshot,
    insertion: &PropertyScrollContentOuterTransformInsertionContract,
    context: TransformSurfacePlanContext,
) -> bool {
    let receiver = &insertion.receiver;
    if receiver.scene_root_ordinal != root.ordinal
        || receiver.receiver != expected
        || receiver.receiver.owner != root.root
        || receiver.receiver.id.0 != root.root
        || receiver.receiver.parent.is_some()
        || receiver.receiver.generation == 0
        || receiver.receiver_stable_id != root.stable_id
        || receiver.scroll_boundary_ordinal != boundary.ordinal
        || receiver.scroll_cutout.root != boundary.scroll.owner
        || receiver.scroll_cutout.stable_id == 0
        || receiver.scroll_cutout.kind != super::PlannedBoundaryKind::Scroll(boundary.scroll.id)
        || receiver.insertion_index >= receiver.recorded_steps.len()
        || receiver.before_span != (0..receiver.insertion_index)
        || receiver.after_span != (receiver.insertion_index + 1..receiver.recorded_steps.len())
        || !insertion.geometry.matches_rebuilt_contract()
        || insertion
            .geometry
            .viewport_transform
            .to_cols_array()
            .map(f32::to_bits)
            != expected.viewport_matrix.to_cols_array().map(f32::to_bits)
        || insertion.geometry.outer_scissor_rect != context.outer_scissor_rect
    {
        return false;
    }
    let mut marker_count = 0usize;
    let mut cursor = 0_u32;
    for (index, step) in receiver.recorded_steps.iter().enumerate() {
        match step {
            PropertyScrollReceiverRecordedStepIdentity::Artifact(artifact) => {
                if index == receiver.insertion_index
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
                if index != receiver.insertion_index || *marker != receiver.scroll_cutout {
                    return false;
                }
                if cursor != receiver.receiver_opaque_before {
                    return false;
                }
            }
        }
    }
    marker_count == 1 && cursor == receiver.receiver_opaque_after
}

fn property_scene_plan_is_sealed(plan: &FramePaintPlan) -> bool {
    let Some(seal) = &plan.property_scene_seal else {
        return false;
    };
    if let Some(scaffold) = &seal.native_scroll_forest_scaffold {
        return seal.effect_scaffold.is_none()
            && seal.scroll_schedule_scaffold.is_none()
            && seal.nested_scroll_scaffold.is_none()
            && native_scroll_forest_scaffold_is_canonical(plan, seal, scaffold);
    }
    if let Some(scaffold) = &seal.nested_scroll_scaffold {
        return seal.effect_scaffold.is_none()
            && seal.scroll_schedule_scaffold.is_none()
            && seal.native_scroll_forest_scaffold.is_none()
            && nested_scroll_scene_scaffold_is_canonical(plan, seal, scaffold);
    }
    if let Some(scaffold) = &seal.scroll_schedule_scaffold {
        return seal.effect_scaffold.is_none()
            && seal.nested_scroll_scaffold.is_none()
            && seal.native_scroll_forest_scaffold.is_none()
            && property_scroll_schedule_scaffold_is_canonical(plan, seal, scaffold);
    }
    if let Some(scaffold) = &seal.effect_scaffold {
        return seal.nested_scroll_scaffold.is_none()
            && seal.native_scroll_forest_scaffold.is_none()
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

fn native_scroll_forest_scaffold_is_canonical(
    plan: &FramePaintPlan,
    seal: &PropertyScenePlanSeal,
    scaffold: &NativeScrollForestScaffold,
) -> bool {
    if !plan.steps.is_empty()
        || seal.surface_count != 0
        || !seal.surfaces.is_empty()
        || !seal.scene_artifact_validation.is_empty()
        || seal.aggregate_opaque_order_span != (0..0)
        || seal.context != scaffold.context
        || seal.outer_scissor_rect.is_some()
        || scaffold.context != scaffold.planned_context
        || scaffold.scale_factor_bits != scaffold.planned_scale_factor_bits
        || !f32::from_bits(scaffold.scale_factor_bits).is_finite()
        || f32::from_bits(scaffold.scale_factor_bits) <= 0.0
        || scaffold.context.paint_offset_bits != [0.0_f32.to_bits(); 2]
        || scaffold.context.outer_scissor_rect().is_some()
        || scaffold.roots != scaffold.planned_roots
        || scaffold.boundaries != scaffold.planned_boundaries
        || scaffold.schedule != scaffold.planned_schedule
        || scaffold.programs != scaffold.planned_programs
        || scaffold.roots.is_empty()
        || scaffold.boundaries.len() < 3
    {
        return false;
    }
    let Some(plan_roots) = &plan.property_scene_roots else {
        return false;
    };
    if plan_roots != &seal.roots || plan_roots.len() != scaffold.roots.len() {
        return false;
    }
    let mut next_boundary = 0_u32;
    let mut roots = FxHashSet::default();
    let mut root_stable_ids = FxHashSet::default();
    for (ordinal, (root, witness)) in scaffold.roots.iter().zip(plan_roots).enumerate() {
        if root.ordinal != ordinal as u32
            || witness.ordinal != root.ordinal
            || witness.root != root.root
            || witness.stable_id != root.stable_id
            || witness.owner
                != (PaintOwnerSnapshot {
                    owner: root.root,
                    parent: None,
                })
            || witness.top_level_step_span != (0..0)
            || root.stable_id == 0
            || !roots.insert(root.root)
            || !root_stable_ids.insert(root.stable_id)
            || root.boundary_span.start != next_boundary
            || root.boundary_span.start >= root.boundary_span.end
            || root.boundary_span.end as usize > scaffold.boundaries.len()
        {
            return false;
        }
        next_boundary = root.boundary_span.end;
    }
    if next_boundary as usize != scaffold.boundaries.len() {
        return false;
    }

    let mut boundary_roots = FxHashSet::default();
    let mut boundary_stable_ids = FxHashSet::default();
    for (index, boundary) in scaffold.boundaries.iter().enumerate() {
        let id = NativeScrollBoundaryId(index as u32);
        let root = scaffold.roots.get(boundary.scene_root_ordinal as usize);
        let expected_projected = boundary
            .parent
            .and_then(|parent| scaffold.boundaries.get(parent.0 as usize))
            .map(|parent| parent.projection.live_input)
            .unwrap_or_default();
        let expected_parent_scroll = boundary
            .parent
            .and_then(|parent| scaffold.boundaries.get(parent.0 as usize))
            .map(|parent| parent.scroll.id);
        let expected_parent_clip = boundary
            .parent
            .and_then(|parent| scaffold.boundaries.get(parent.0 as usize))
            .map(|parent| parent.contents_clip.id);
        let expected_live = PropertyTreeState {
            clip: Some(boundary.contents_clip.id),
            scroll: Some(boundary.scroll.id),
            ..PropertyTreeState::default()
        };
        let geometry_is_canonical = boundary.parent.is_none_or(|parent| {
            parent.0 < id.0
                && scaffold.boundaries[parent.0 as usize].scene_root_ordinal
                    == boundary.scene_root_ordinal
        }) && boundary
            .scroll
            .has_canonical_geometry_with_contents_clip_parent_ids(
                boundary.contents_clip,
                expected_parent_scroll,
                expected_parent_clip,
            );
        if boundary.id != id
            || root.is_none_or(|root| !root.boundary_span.contains(&id.0))
            || boundary.boundary_root != boundary.scroll.owner
            || boundary.scroll.id.0 != boundary.boundary_root
            || boundary.contents_clip.id
                != (ClipNodeId {
                    owner: boundary.boundary_root,
                    role: ClipNodeRole::ContentsClip,
                })
            || boundary.contents_clip.owner != boundary.boundary_root
            || boundary.stable_id == 0
            || boundary.stable_id != boundary.admission.stable_id
            || boundary.admission.boundary_root != boundary.boundary_root
            || boundary.admission.content_root_stable_id == 0
            || !boundary.admission.matches_scroll_node(boundary.scroll)
            || boundary.scroll.parent != expected_parent_scroll
            || boundary.contents_clip.parent != expected_parent_clip
            || boundary.contents_clip.behavior != ClipBehavior::Intersect
            || boundary.scroll.generation == 0
            || boundary.contents_clip.generation == 0
            || boundary.projection.live_input != expected_live
            || boundary.projection.projected_output != expected_projected
            || boundary.projection.parent_scroll != expected_parent_scroll
            || boundary.projection.parent_clip != expected_parent_clip
            || !geometry_is_canonical
            || !boundary_roots.insert(boundary.boundary_root)
            || !boundary_stable_ids.insert(boundary.stable_id)
        {
            return false;
        }
    }

    if scaffold.programs.len() != scaffold.boundaries.len() {
        return false;
    }
    for (index, program) in scaffold.programs.iter().enumerate() {
        let boundary = &scaffold.boundaries[index];
        let expected_children = scaffold
            .boundaries
            .iter()
            .filter(|child| child.parent == Some(boundary.id))
            .map(|child| {
                (
                    child.id,
                    super::PlannedBoundary {
                        root: child.boundary_root,
                        stable_id: child.stable_id,
                        kind: super::PlannedBoundaryKind::Scroll(child.scroll.id),
                    },
                )
            })
            .collect::<Vec<_>>();
        let mut marker_cursor = 0usize;
        let recorded_content = program
            .content_steps
            .iter()
            .map(|step| match step {
                NativeScrollForestContentProgramStep::Artifact(artifact) => {
                    (property_scroll_receiver_artifact_identity(artifact.artifact()).as_ref()
                        == Some(&artifact.identity))
                    .then(|| {
                        super::frame_recorder::RecordedTransformSurfaceStep::Artifact(
                            artifact.artifact().clone(),
                        )
                    })
                }
                NativeScrollForestContentProgramStep::ChildBoundary(child) => {
                    let (expected_id, expected) = expected_children.get(marker_cursor)?;
                    if child != expected_id {
                        return None;
                    }
                    marker_cursor += 1;
                    Some(super::frame_recorder::RecordedTransformSurfaceStep::Boundary(*expected))
                }
            })
            .collect::<Option<Vec<_>>>();
        let source_bounds = boundary.admission.source_bounds;
        let expected_cutouts = expected_children
            .iter()
            .map(|(_, cutout)| *cutout)
            .collect::<Vec<_>>();
        let expected_edge = super::PaintScrollForestEdgeWitness::new(
            boundary.boundary_root,
            boundary.admission.content_root,
            boundary.scroll,
            boundary.contents_clip,
            boundary.projection.parent_scroll,
            boundary.projection.parent_clip,
        );
        let mut dependency_cursor = 0_u32;
        let expected_dependencies = program
            .content_steps
            .iter()
            .map(|step| match step {
                NativeScrollForestContentProgramStep::Artifact(artifact) => {
                    dependency_cursor =
                        dependency_cursor.checked_add(opaque_order_count(artifact.artifact()))?;
                    Some(None)
                }
                NativeScrollForestContentProgramStep::ChildBoundary(child) => {
                    let child_program = scaffold.programs.get(child.0 as usize)?;
                    let child_boundary = scaffold.boundaries.get(child.0 as usize)?;
                    let before = dependency_cursor;
                    dependency_cursor = dependency_cursor
                        .checked_add(child_program.compiler_stamp.host_opaque_count)?
                        .checked_add(child_program.compiler_stamp.overlay_opaque_count)?;
                    let source = child_boundary.scroll.layout_content_bounds_at_zero;
                    Some(Some(NativeScrollForestChildRasterDependency {
                        child: *child,
                        boundary_root: child_boundary.boundary_root,
                        content_root: child_boundary.admission.content_root,
                        content_stable_id: child_program.receiver_stable_id,
                        child_raster_identity: Box::new(
                            child_program
                                .content_raster_identity(child_boundary.admission.content_root),
                        ),
                        host_identity: child_program.host_before.identity.clone(),
                        overlay_identity: child_program.overlay_after.identity.clone(),
                        scroll: child_boundary.scroll,
                        contents_clip: child_boundary.contents_clip,
                        source_bounds_bits: [
                            source.x.to_bits(),
                            source.y.to_bits(),
                            source.width.to_bits(),
                            source.height.to_bits(),
                        ],
                        offset_bits: [
                            child_boundary.scroll.offset.x.to_bits(),
                            child_boundary.scroll.offset.y.to_bits(),
                        ],
                        composite_scissor: child_boundary.contents_clip.logical_scissor,
                        parent_opaque_before: before,
                        parent_opaque_after: dependency_cursor,
                    }))
                }
            })
            .collect::<Option<Vec<_>>>()
            .map(|items| items.into_iter().flatten().collect::<Vec<_>>());
        if program.boundary != boundary.id
            || program.receiver_stable_id != boundary.admission.content_root_stable_id
            || Some(program.edge) != expected_edge
            || property_scroll_receiver_artifact_identity(program.host_before.artifact()).as_ref()
                != Some(&program.host_before.identity)
            || property_scroll_receiver_artifact_identity(program.overlay_after.artifact()).as_ref()
                != Some(&program.overlay_after.identity)
            || marker_cursor != expected_children.len()
            || recorded_content.as_ref().is_none_or(Vec::is_empty)
            || expected_dependencies.as_ref() != Some(&program.child_dependencies)
            || program.content_program_opaque_terminal != dependency_cursor
            || recorded_content.as_ref().and_then(|content| {
                super::compiler::compile_native_scroll_forest_boundary_program_for_plan(
                    boundary.boundary_root,
                    boundary.admission.content_root,
                    boundary.scroll,
                    [
                        source_bounds.x.to_bits(),
                        source_bounds.y.to_bits(),
                        source_bounds.width.to_bits(),
                        source_bounds.height.to_bits(),
                    ],
                    program.host_before.artifact(),
                    content,
                    &expected_cutouts,
                    program.overlay_after.artifact(),
                )
            }) != Some(program.compiler_stamp.clone())
        {
            return false;
        }
    }

    let mut stack = Vec::<NativeScrollBoundaryId>::new();
    let mut pending = None;
    let mut opened = FxHashSet::default();
    let mut closed = FxHashSet::default();
    let mut receivers = FxHashSet::default();
    let mut boundary_has_body = vec![false; scaffold.boundaries.len()];
    for step in &scaffold.schedule.steps {
        match step {
            NativeScrollForestScheduledStep::ChildBoundary { parent, child } => {
                if pending.is_some()
                    || scaffold
                        .boundaries
                        .get(child.0 as usize)
                        .is_none_or(|boundary| {
                            boundary.parent != *parent
                                || stack.last().copied() != *parent
                                || opened.contains(child)
                        })
                {
                    return false;
                }
                if let Some(parent) = parent {
                    boundary_has_body[parent.0 as usize] = true;
                }
                pending = Some(*child);
            }
            NativeScrollForestScheduledStep::Artifact {
                boundary,
                phase: NativeScrollArtifactPhase::HostBefore,
            } => {
                if pending != Some(*boundary) || !opened.insert(*boundary) {
                    return false;
                }
                pending = None;
                stack.push(*boundary);
            }
            NativeScrollForestScheduledStep::Artifact {
                boundary,
                phase: NativeScrollArtifactPhase::OverlayAfter,
            } => {
                if pending.is_some()
                    || stack.pop() != Some(*boundary)
                    || !boundary_has_body[boundary.0 as usize]
                    || !closed.insert(*boundary)
                {
                    return false;
                }
            }
            NativeScrollForestScheduledStep::ContentReceiver {
                boundary,
                content_root,
                stable_id,
                projection,
            } => {
                if pending.is_some()
                    || stack.last() != Some(boundary)
                    || *stable_id == 0
                    || boundary_roots.contains(content_root)
                    || !receivers.insert((*content_root, *stable_id))
                    || scaffold
                        .boundaries
                        .get(boundary.0 as usize)
                        .is_none_or(|contract| {
                            contract.projection != *projection
                                || contract.admission.content_root == *content_root
                                    && contract.boundary_root == *content_root
                        })
                {
                    return false;
                }
                boundary_has_body[boundary.0 as usize] = true;
            }
        }
    }
    pending.is_none()
        && stack.is_empty()
        && opened.len() == scaffold.boundaries.len()
        && closed.len() == scaffold.boundaries.len()
        && receivers.iter().all(|(_, stable_id)| {
            !root_stable_ids.contains(stable_id) && !boundary_stable_ids.contains(stable_id)
        })
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
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
) -> Result<FramePaintPlan, FramePaintPlanError> {
    plan_single_root_transform_surface_with_context(
        arena,
        roots,
        property_trees,
        paint_generations,
        TransformSurfacePlanContext::default(),
    )
}

pub(crate) fn plan_single_root_transform_surface_with_context(
    arena: &NodeArena,
    roots: &[NodeKey],
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
    let mut reasons = Vec::new();
    if arena.parent_of(*root).is_some() {
        push_unique(&mut reasons, FramePaintPlanRejection::RootHasParent(*root));
    }
    if !root_node.element.has_retained_transform_surface() {
        push_unique(
            &mut reasons,
            FramePaintPlanRejection::UnknownRootHost(*root),
        );
        return Err(FramePaintPlanError { reasons });
    }
    let root_element = root_node.element.as_ref();

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
        if !sampled_layout_transition_is_exact(node.element.as_ref()) {
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
        let nested_element = arena.get(snapshot.owner).filter(|node| {
            arena.parent_of(snapshot.owner) == Some(*root)
                && node.element.has_retained_transform_surface()
        });
        if nested_element.is_none()
            || nested_id != TransformNodeId(snapshot.owner)
            || snapshot.parent != Some(transform)
            || snapshot.generation == 0
        {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::UnexpectedTransform(snapshot.owner),
            );
        }
        if nested_element.is_some_and(|node| {
            !node
                .element
                .compositor_viewport_transform_snapshot()
                .is_some_and(|live| {
                    live.to_cols_array().map(f32::to_bits)
                        == snapshot.viewport_matrix.to_cols_array().map(f32::to_bits)
                })
        }) {
            push_unique(
                &mut reasons,
                FramePaintPlanRejection::InvalidRootTransform(snapshot.owner),
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
        if !child_node.element.has_retained_transform_surface() {
            return Err(FramePaintPlanError {
                reasons: vec![FramePaintPlanRejection::UnexpectedTransform(child_root)],
            });
        }
        let child_element = child_node.element.as_ref();
        let child_transform = TransformNodeId(child_root);
        let root_recording_context =
            root_element.shadow_paint_recording_context(super::PaintRecordingContext {
                paint_offset: context.paint_offset(),
                ..Default::default()
            });
        let child_paint_offset = root_element
            .shadow_paint_recording_context_for_child(child_root, arena, root_recording_context)
            .paint_offset;
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
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
) -> Result<FramePaintPlan, FramePaintPlanError> {
    plan_single_root_transform_child_isolation_surface_with_context(
        arena,
        roots,
        property_trees,
        paint_generations,
        TransformSurfacePlanContext::default(),
    )
}

pub(crate) fn plan_single_root_transform_child_isolation_surface_with_context(
    arena: &NodeArena,
    roots: &[NodeKey],
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

    let child_is_direct_host = child_root
        .is_some_and(|child| arena.parent_of(child) == Some(*root) && arena.get(child).is_some());
    let effect_id = child_root.map(EffectNodeId);
    let effect = effect_id.and_then(|effect| property_trees.effects.get(&effect).copied());
    match (child_root, child_is_direct_host, effect) {
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
        if !sampled_layout_transition_is_exact(node.element.as_ref()) {
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
    let child_element = child_node.element.as_ref();
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
        .exact_nested_isolation_render_output_bounds(child_root, arena, child_paint_offset)
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
    if !property_trees.transforms.is_empty()
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
        if !sampled_layout_transition_is_exact(current.element.as_ref()) {
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
    element: &dyn ElementTrait,
    arena: &NodeArena,
    root: NodeKey,
    context: TransformSurfacePlanContext,
    expected_viewport_matrix: Option<glam::Mat4>,
) -> Result<TransformSurfaceGeometrySnapshot, FramePaintPlanError> {
    let source_bounds = element
        .retained_transform_surface_bounds(arena, context.paint_offset())
        .ok_or_else(|| FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::InvalidSurfaceGeometry(root)],
        })?;
    let viewport_transform = element
        .compositor_viewport_transform_snapshot()
        .map(|snapshot| glam::Mat4::from_cols_array(&snapshot.to_cols_array()))
        .ok_or_else(|| FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::InvalidRootTransform(root)],
        })?;
    let visual_bounds = crate::view::viewport::scene_helpers::paint_snapped_retained_surface_bounds(
        element,
        source_bounds,
        context.paint_offset(),
    );
    let geometry = TransformSurfaceGeometrySnapshot::new(
        source_bounds,
        visual_bounds,
        viewport_transform,
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

pub(super) fn sampled_layout_transition_is_exact(element: &dyn ElementTrait) -> bool {
    if !element
        .placement_eligibility_metadata()
        .contains_runtime_layout_state
    {
        return true;
    }
    let Some(witness) = element.retained_sampled_layout_transition_snapshot() else {
        return false;
    };
    let bounds = element.box_model_snapshot();
    let option_bits_are_finite = |values: [Option<u32>; 2]| {
        values
            .into_iter()
            .flatten()
            .all(|bits| f32::from_bits(bits).is_finite())
    };
    witness.stable_id != 0
        && witness.stable_id == element.stable_id()
        && witness.stable_id == bounds.node_id
        && witness.bounds_bits
            == [bounds.x, bounds.y, bounds.width, bounds.height].map(f32::to_bits)
        && witness
            .bounds_bits
            .iter()
            .all(|bits| f32::from_bits(*bits).is_finite())
        && f32::from_bits(witness.bounds_bits[2]) >= 0.0
        && f32::from_bits(witness.bounds_bits[3]) >= 0.0
        && witness
            .visual_offset_bits
            .iter()
            .all(|bits| f32::from_bits(*bits).is_finite())
        && option_bits_are_finite(witness.override_size_bits)
        && option_bits_are_finite(witness.target_position_bits)
        && option_bits_are_finite(witness.target_size_bits)
        && witness
            .override_size_bits
            .into_iter()
            .chain(witness.target_size_bits)
            .flatten()
            .all(|bits| f32::from_bits(bits) >= 0.0)
        && element.retained_paint_signature_is_complete()
        && witness.paint_signature == element.retained_paint_signature()
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
pub(super) mod tests;
