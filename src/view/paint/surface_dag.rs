use std::ops::Range;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::view::{
    compositor::property_tree::{
        ClipNodeId, ClipNodeRole, ClipNodeSnapshot, EffectNodeId, EffectNodeSnapshot,
        PropertyStateTransition, PropertyTreeState, ScrollNodeId, ScrollNodeSnapshot,
        TransformNodeId, TransformNodeSnapshot,
    },
    node_arena::NodeKey,
};

use super::{
    ArtifactCursor, ArtifactOwnerGraph, ArtifactSceneTarget, ArtifactTransitionRequest,
    ClassifiedTransitionEvent, PaintArtifact, PaintChunk, PaintNodePhase, PropertySnapshotGraph,
    RETAINED_CHILD_MASK_SLOT, TransitionError, artifact_cursors, classify_property_transition,
};

#[cfg(test)]
mod materialization_tests;

/// Explicit materialization input for the retained target projection.
///
/// Logical property boundaries remain present in [`SurfaceDag`]. This policy
/// controls only whether a boundary needs its own physical raster target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayerizationPolicy {
    ResolveMaterializedTargets,
}

/// Durable surface-producing property families.
///
/// Clip, layout-position, and visual-offset identities remain coordinate-space
/// obligations and never become independent surface kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SurfaceDagNodeKind {
    Transform(TransformNodeId),
    Effect(EffectNodeId),
    ScrollContent {
        scroll: ScrollNodeId,
        contents_clip: ClipNodeId,
    },
}

/// Dense artifact-local surface identity assigned in artifact-store order.
///
/// This identity is not a topological ordinal: a node's receiver may have a
/// higher id. Consumers must traverse the receiver graph instead of inferring
/// receiver order from [`SurfaceDagNodeId::index`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SurfaceDagNodeId(u32);

impl SurfaceDagNodeId {
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

/// Scene-root identity derived from one validated artifact owner graph.
/// The ordinal cannot be constructed by a consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SurfaceDagSceneRootId(u32);

impl SurfaceDagSceneRootId {
    fn from_scene_target(target: ArtifactSceneTarget) -> Self {
        Self(target.scene_root_ordinal())
    }

    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

/// One complete scene identity admitted by the artifact owner graph.
///
/// Roots are retained even when they produce no surface. `stable_id` is
/// persistent identity payload, not an execution ordinal; it derives from the
/// same keyed owner snapshot as surface-node identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceDagSceneRoot {
    id: SurfaceDagSceneRootId,
    target: NodeKey,
    stable_id: u64,
}

impl SurfaceDagSceneRoot {
    #[cfg(test)]
    pub(crate) fn id(self) -> SurfaceDagSceneRootId {
        self.id
    }

    #[cfg(test)]
    pub(crate) fn target(self) -> NodeKey {
        self.target
    }

    #[cfg(test)]
    pub(crate) fn stable_id(self) -> u64 {
        self.stable_id
    }
}

/// Generic receiver identity for every surface kind.
///
/// There is deliberately no scroll-content receiver grammar variant. A
/// detached scroll-content surface receives another generic surface id just
/// like transform and effect surfaces do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SurfaceDagTargetId {
    SceneRoot(SurfaceDagSceneRootId),
    Surface(SurfaceDagNodeId),
}

impl SurfaceDagTargetId {
    #[cfg(test)]
    pub(crate) fn scene_root_ordinal(self) -> Option<u32> {
        match self {
            Self::SceneRoot(root) => Some(root.0),
            Self::Surface(_) => None,
        }
    }
}

/// Dense parent-before-child identity used only while executing a Surface DAG.
///
/// This is deliberately distinct from [`SurfaceDagNodeId`]. The latter stays
/// attached to artifact-store identity even when the execution projection
/// moves that source node to a different ordinal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SurfaceDagExecutionNodeId(u32);

impl SurfaceDagExecutionNodeId {
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

/// Receiver identity after the source graph has been projected into execution
/// order. A surface receiver always names an earlier execution node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SurfaceDagExecutionTargetId {
    SceneRoot(SurfaceDagSceneRootId),
    Surface(SurfaceDagExecutionNodeId),
}

/// One scene root and its contiguous execution-node range.
///
/// Roots do not receive a second execution identity: their existing ordinal is
/// already the required order. Only surface nodes are remapped; empty spans
/// preserve admitted plain roots that produce no surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceDagExecutionRoot {
    identity: SurfaceDagSceneRoot,
    node_span: Range<u32>,
}

impl SurfaceDagExecutionRoot {
    #[cfg(test)]
    pub(crate) fn identity(&self) -> SurfaceDagSceneRoot {
        self.identity
    }

    #[cfg(test)]
    pub(crate) fn node_span(&self) -> Range<u32> {
        self.node_span.clone()
    }
}

/// One materialized surface in parent-before-child execution order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceDagExecutionNode {
    id: SurfaceDagExecutionNodeId,
    source: SurfaceDagNodeId,
    scene_root: SurfaceDagSceneRootId,
    receiver: SurfaceDagExecutionTargetId,
}

impl SurfaceDagExecutionNode {
    pub(crate) fn id(&self) -> SurfaceDagExecutionNodeId {
        self.id
    }

    pub(crate) fn source(&self) -> SurfaceDagNodeId {
        self.source
    }

    #[cfg(test)]
    pub(crate) fn scene_root(&self) -> SurfaceDagSceneRootId {
        self.scene_root
    }

    pub(crate) fn receiver(&self) -> SurfaceDagExecutionTargetId {
        self.receiver
    }
}

/// Why one logical boundary did or did not receive a physical target.
///
/// The vocabulary intentionally describes materialization obligations rather
/// than property-family grammar. A future property kind must satisfy the same
/// target contract instead of adding another authority path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SurfaceMaterializationOutcome {
    RetainedOwnRasterContent,
    RetainedNestedTargetCount,
    RetainedIsolation,
    RetainedNonTranslation,
    RetainedClipTransfer,
    RetainedUncomposedBoundary,
    EliminatedPassThrough,
}

/// Frozen obligations consumed by the raster planner after target elimination.
/// Raster compensation and receiver placement are intentionally separate: a
/// scroll offset normalizes resident content but a transform moves composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceBoundaryTransfer {
    pub(crate) raster_translation_bits: [u32; 2],
    pub(crate) composite_translation_bits: [u32; 2],
    pub(crate) clip: Option<SurfaceDagClipRebase>,
}

// Exact numeric equality is intentional (signed zeros compare equal). Even
// tiny scale/shear/perspective residuals retain the target. An epsilon would
// broaden admission and requires a separate raster-identity and pixel proof.
fn finite_translation(matrix: glam::Mat4) -> Option<[f32; 2]> {
    let v = matrix.to_cols_array();
    let expected = glam::Mat4::from_translation(glam::Vec3::new(v[12], v[13], 0.0));
    (matrix.is_finite() && matrix == expected).then_some([v[12], v[13]])
}

fn boundary_transfer(
    kind: SurfaceDagNodeKind,
    transition: PropertyStateTransition,
    clip: Option<SurfaceDagClipRebase>,
    transforms: &FxHashMap<TransformNodeId, TransformNodeSnapshot>,
    effects: &FxHashMap<EffectNodeId, EffectNodeSnapshot>,
    scrolls: &FxHashMap<ScrollNodeId, ScrollNodeSnapshot>,
) -> Result<Result<SurfaceBoundaryTransfer, SurfaceMaterializationOutcome>, SurfaceDagError> {
    // Structural failure aborts DAG construction; the inner result describes
    // only a valid boundary's materialization obligations. Resolve required
    // snapshots before testing properties so malformed input cannot be hidden
    // behind an ordinary retained outcome.
    let missing = SurfaceDagError::MissingMaterializationSnapshot;
    match kind {
        SurfaceDagNodeKind::Transform(id) => {
            let snapshot = transforms.get(&id).ok_or(missing(kind))?;
            if let Some(parent) = snapshot.parent {
                transforms
                    .get(&parent)
                    .ok_or(missing(SurfaceDagNodeKind::Transform(parent)))?;
            }
        }
        SurfaceDagNodeKind::Effect(id) => {
            effects.get(&id).ok_or(missing(kind))?;
        }
        SurfaceDagNodeKind::ScrollContent { scroll, .. } => {
            scrolls.get(&scroll).ok_or(missing(kind))?;
        }
    }
    Ok((|| {
        use SurfaceMaterializationOutcome::*;
        let mut transfer = SurfaceBoundaryTransfer {
            raster_translation_bits: [0; 2],
            composite_translation_bits: [0; 2],
            clip: None,
        };
        match kind {
            SurfaceDagNodeKind::Transform(id) => {
                let snapshot = transforms.get(&id).expect("snapshot checked above");
                finite_translation(snapshot.local_matrix).ok_or(RetainedNonTranslation)?;
                let current = finite_translation(snapshot.owner_viewport_transform)
                    .ok_or(RetainedNonTranslation)?;
                let parent = match snapshot.parent {
                    Some(parent) => finite_translation(
                        transforms
                            .get(&parent)
                            .expect("parent snapshot checked above")
                            .owner_viewport_transform,
                    )
                    .ok_or(RetainedNonTranslation)?,
                    None => [0.0; 2],
                };
                let delta = [current[0] - parent[0], current[1] - parent[1]];
                if !delta.into_iter().all(f32::is_finite) {
                    return Err(RetainedNonTranslation);
                }
                transfer.composite_translation_bits = delta.map(f32::to_bits);
            }
            SurfaceDagNodeKind::Effect(id) => {
                let snapshot = effects.get(&id).expect("snapshot checked above");
                // Effect snapshots currently encode group opacity. Only the exact
                // identity has no remaining isolation/compositing obligation.
                if snapshot.opacity != 1.0 {
                    return Err(RetainedIsolation);
                }
            }
            SurfaceDagNodeKind::ScrollContent { scroll, .. } => {
                let offset = scrolls.get(&scroll).expect("snapshot checked above").offset;
                if !offset.is_finite() {
                    return Err(RetainedNonTranslation);
                }
                transfer.raster_translation_bits = offset.to_array().map(f32::to_bits);
                transfer.clip = Some(clip.ok_or(RetainedClipTransfer)?);
            }
        }
        // Unclipped boundaries need no clip transfer. A consumed scrollport has
        // its own validated transfer. Other clip composition needs a separate
        // proof; absence of that proof is not an isolation obligation.
        if transfer.clip.is_none()
            && (transition.clip.from.is_some() || transition.clip.to.is_some())
        {
            return Err(RetainedClipTransfer);
        }
        Ok(transfer)
    })())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceMaterializationDecision {
    source: SurfaceDagNodeId,
    target: NodeKey,
    nested_target_count: usize,
    has_own_raster_content: bool,
    outcome: SurfaceMaterializationOutcome,
}

impl SurfaceMaterializationDecision {
    #[cfg(test)]
    pub(crate) fn source(self) -> SurfaceDagNodeId {
        self.source
    }

    #[cfg(test)]
    pub(crate) fn target(self) -> NodeKey {
        self.target
    }

    #[cfg(test)]
    pub(crate) fn nested_target_count(self) -> usize {
        self.nested_target_count
    }

    #[cfg(test)]
    pub(crate) fn has_own_raster_content(self) -> bool {
        self.has_own_raster_content
    }

    #[cfg(test)]
    pub(crate) fn outcome(self) -> SurfaceMaterializationOutcome {
        self.outcome
    }

    fn is_eliminated(self) -> bool {
        self.outcome == SurfaceMaterializationOutcome::EliminatedPassThrough
    }
}

fn retain_uncomposed_adjacent_pass_through_boundaries(
    sole_nested_child: &mut [Option<SurfaceDagNodeId>],
    decisions: &mut [SurfaceMaterializationDecision],
) {
    // Snapshot before mutation is essential: consulting updated outcomes
    // makes elimination depend on artifact-store order and can erase adjacent
    // boundaries without a composed transfer proof.
    let elimination_candidates = decisions
        .iter()
        .map(|decision| decision.is_eliminated())
        .collect::<Vec<_>>();
    for decision in decisions.iter_mut() {
        if !elimination_candidates[decision.source.index()] {
            continue;
        }
        let Some(child) = sole_nested_child[decision.source.index()] else {
            continue;
        };
        if elimination_candidates[child.index()] {
            sole_nested_child[decision.source.index()] = None;
            decision.outcome = SurfaceMaterializationOutcome::RetainedUncomposedBoundary;
        }
    }
}

/// O(N) execution projection over one validated Surface DAG.
///
/// This supplies the parent-first and same-root-span preconditions required by
/// the retained executors. A C3 consumer must still iterate this order when it
/// wires compiler depth, prepare, emit, and execute; constructing the remap
/// alone does not discharge that wiring contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceDagExecutionOrder {
    roots: Vec<SurfaceDagExecutionRoot>,
    nodes: Vec<SurfaceDagExecutionNode>,
    source_to_execution: Vec<SurfaceDagExecutionNodeId>,
    folded_boundaries: Vec<Vec<SurfaceDagNodeId>>,
    decisions: Vec<SurfaceMaterializationDecision>,
}

impl SurfaceDagExecutionOrder {
    #[cfg(test)]
    pub(crate) fn roots(&self) -> &[SurfaceDagExecutionRoot] {
        &self.roots
    }

    pub(crate) fn nodes(&self) -> &[SurfaceDagExecutionNode] {
        &self.nodes
    }

    pub(crate) fn execution_id(
        &self,
        source: SurfaceDagNodeId,
    ) -> Option<SurfaceDagExecutionNodeId> {
        self.source_to_execution.get(source.index()).copied()
    }

    pub(crate) fn source_node_id(
        &self,
        execution: SurfaceDagExecutionNodeId,
    ) -> Option<SurfaceDagNodeId> {
        self.nodes
            .get(execution.index())
            .filter(|node| node.id == execution)
            .map(|node| node.source)
    }

    pub(crate) fn decisions(&self) -> &[SurfaceMaterializationDecision] {
        &self.decisions
    }

    pub(crate) fn folded_boundaries(
        &self,
        execution: SurfaceDagExecutionNodeId,
    ) -> Option<&[SurfaceDagNodeId]> {
        self.folded_boundaries
            .get(execution.index())
            .map(Vec::as_slice)
    }
}

/// The clip-space obligation carried by a detached scroll-content surface.
/// `receiver_clip` is the contents clip's immediate clip-forest parent and is
/// proven below to remain in receiver space. `local_clip` is the scroll
/// contents boundary where descendant raster-local clips are detached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceDagClipRebase {
    receiver_clip: Option<ClipNodeId>,
    local_clip: ClipNodeId,
}

impl SurfaceDagClipRebase {
    fn from_validated(
        local_clip: ClipNodeId,
        snapshots: &PropertySnapshotGraph,
    ) -> Result<Self, SurfaceDagError> {
        Ok(Self {
            receiver_clip: snapshots.clip_parent(local_clip)?,
            local_clip,
        })
    }

    pub(crate) fn receiver_clip(self) -> Option<ClipNodeId> {
        self.receiver_clip
    }

    pub(crate) fn local_clip(self) -> ClipNodeId {
        self.local_clip
    }
}

/// Surface-wide clip closure derived from every logically covered artifact
/// chunk in painter order. This is not a mergeable single-chain value: only
/// the shared artifact surface walk can construct the complete deterministic
/// union. An empty `local_clips` union
/// retains `receiver_clip`, but carries no local generation authority: the
/// eventual raster-generation semantics is not applicable (`None`), rather
/// than either `ArtifactLive` or `LegacyDetached`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceDagClipClosureProjection {
    receiver_clip: Option<ClipNodeId>,
    local_clips: Vec<ClipNodeSnapshot>,
}

impl SurfaceDagClipClosureProjection {
    pub(crate) fn receiver_clip(&self) -> Option<ClipNodeId> {
        self.receiver_clip
    }

    pub(crate) fn local_clips(&self) -> &[ClipNodeSnapshot] {
        &self.local_clips
    }
}

/// One direct painter-order run. Every chunk occurs in exactly one span at
/// its innermost active surface, or at the scene root when no surface is
/// active. Ancestors cover the span only through [`ArtifactSurfaceCoverageStep::NestedSurface`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ArtifactSurfaceCoverageSpan {
    chunk_range: Range<usize>,
    op_range: Range<usize>,
    localized_states: Vec<PropertyTreeState>,
    local_clips: Vec<ClipNodeSnapshot>,
}

impl ArtifactSurfaceCoverageSpan {
    pub(crate) fn chunk_range(&self) -> Range<usize> {
        self.chunk_range.clone()
    }

    pub(crate) fn op_range(&self) -> Range<usize> {
        self.op_range.clone()
    }

    pub(crate) fn localized_states(&self) -> &[PropertyTreeState] {
        &self.localized_states
    }

    pub(crate) fn local_clips(&self) -> &[ClipNodeSnapshot] {
        &self.local_clips
    }
}

/// The complete generic coverage vocabulary. The five exact-shape retained
/// raster dependencies remain live in the legacy executor, but cannot be
/// represented by this Stage C ownership forest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ArtifactSurfaceCoverageStep {
    ArtifactSpan(ArtifactSurfaceCoverageSpan),
    NestedSurface(SurfaceDagNodeId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ArtifactSurfaceCoverageRoot {
    scene_root: SurfaceDagSceneRootId,
    steps: Vec<ArtifactSurfaceCoverageStep>,
}

impl ArtifactSurfaceCoverageRoot {
    pub(crate) fn scene_root(&self) -> SurfaceDagSceneRootId {
        self.scene_root
    }

    pub(crate) fn steps(&self) -> &[ArtifactSurfaceCoverageStep] {
        &self.steps
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ArtifactSurfaceCoverageNode {
    surface: SurfaceDagNodeId,
    steps: Vec<ArtifactSurfaceCoverageStep>,
    receiver_mask_envelope_ranges: Vec<Range<usize>>,
    clip_closure: Option<SurfaceDagClipClosureProjection>,
}

impl ArtifactSurfaceCoverageNode {
    pub(crate) fn surface(&self) -> SurfaceDagNodeId {
        self.surface
    }

    pub(crate) fn steps(&self) -> &[ArtifactSurfaceCoverageStep] {
        &self.steps
    }

    /// Boundary-mask chunks execute in the receiver but retain their spatial
    /// envelope in this surface's raster-origin derivation. This preserves
    /// padding around direct content without admitting the mask op into the
    /// resident raster program.
    pub(crate) fn receiver_mask_envelope_ranges(&self) -> &[Range<usize>] {
        &self.receiver_mask_envelope_ranges
    }

    pub(crate) fn clip_closure(&self) -> Option<&SurfaceDagClipClosureProjection> {
        self.clip_closure.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ArtifactSurfaceCoverageForest {
    roots: Vec<ArtifactSurfaceCoverageRoot>,
    nodes: Vec<ArtifactSurfaceCoverageNode>,
}

impl ArtifactSurfaceCoverageForest {
    pub(crate) fn roots(&self) -> &[ArtifactSurfaceCoverageRoot] {
        &self.roots
    }

    pub(crate) fn nodes(&self) -> &[ArtifactSurfaceCoverageNode] {
        &self.nodes
    }
}

/// Durable C2 node shape with its independently classified consumption edge
/// and artifact-derived compositing receiver.
///
/// Nodes remain in artifact-store order, not topological order. `receiver` may
/// reference a higher [`SurfaceDagNodeId`]; graph traversal must follow that
/// receiver edge until it reaches a scene root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceDagNode {
    id: SurfaceDagNodeId,
    target: NodeKey,
    stable_id: u64,
    cursor: ArtifactCursor,
    kind: SurfaceDagNodeKind,
    receiver: SurfaceDagTargetId,
    transition: PropertyStateTransition,
    clip_rebase: Option<SurfaceDagClipRebase>,
    transfer: Result<SurfaceBoundaryTransfer, SurfaceMaterializationOutcome>,
}

impl SurfaceDagNode {
    pub(crate) fn transfer(&self) -> Option<SurfaceBoundaryTransfer> {
        self.transfer.ok()
    }

    pub(crate) fn id(&self) -> SurfaceDagNodeId {
        self.id
    }

    pub(crate) fn target(&self) -> NodeKey {
        self.target
    }

    /// Persistent owner identity only. A retained surface key must combine it
    /// with this node's [`SurfaceDagNodeKind`]; it is never an execution id.
    pub(crate) fn stable_id(&self) -> u64 {
        self.stable_id
    }

    #[cfg(test)]
    pub(crate) fn cursor(&self) -> ArtifactCursor {
        self.cursor
    }

    pub(crate) fn kind(&self) -> SurfaceDagNodeKind {
        self.kind
    }

    #[cfg(test)]
    pub(crate) fn receiver(&self) -> SurfaceDagTargetId {
        self.receiver
    }

    pub(crate) fn transition(&self) -> PropertyStateTransition {
        self.transition
    }

    #[cfg(test)]
    pub(crate) fn clip_rebase(&self) -> Option<SurfaceDagClipRebase> {
        self.clip_rebase
    }
}

/// Arena-independent ordered surface graph reconstructed from an artifact and
/// its separately classified consumption stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceDag {
    roots: Vec<SurfaceDagSceneRoot>,
    nodes: Vec<SurfaceDagNode>,
}

impl SurfaceDag {
    /// Complete admitted scene-root registry, including roots with no surface.
    /// This is complete only for the artifact owner set, not for the arena.
    #[cfg(test)]
    pub(crate) fn roots(&self) -> &[SurfaceDagSceneRoot] {
        &self.roots
    }

    pub(crate) fn nodes(&self) -> &[SurfaceDagNode] {
        &self.nodes
    }

    /// Projects artifact-store nodes into receiver-tree preorder.
    ///
    /// The source scan is already ordered by [`SurfaceDagNodeId`], so appending
    /// each child to its receiver's adjacency list fixes the sibling tie-break
    /// without any per-parent sort. Iterative preorder then visits every edge
    /// once, preserving O(N) time and O(N) storage.
    #[cfg(test)]
    pub(crate) fn derive_execution_order(
        &self,
    ) -> Result<SurfaceDagExecutionOrder, SurfaceDagError> {
        let mut root_children = vec![Vec::new(); self.roots.len()];
        let mut surface_children = vec![Vec::new(); self.nodes.len()];
        for node in &self.nodes {
            match node.receiver {
                SurfaceDagTargetId::SceneRoot(root) => {
                    self.scene_root(root)?;
                    root_children[root.index()].push(node.id);
                }
                SurfaceDagTargetId::Surface(parent) => {
                    self.surface_node(parent)?;
                    surface_children[parent.index()].push(node.id);
                }
            }
        }

        let execution_ordinal = |ordinal: usize| {
            u32::try_from(ordinal)
                .map(SurfaceDagExecutionNodeId)
                .map_err(|_| SurfaceDagError::ExecutionNodeOrdinalOverflow(ordinal))
        };
        let span_ordinal = |ordinal: usize| {
            u32::try_from(ordinal)
                .map_err(|_| SurfaceDagError::ExecutionNodeOrdinalOverflow(ordinal))
        };
        let mut roots = Vec::with_capacity(self.roots.len());
        let mut nodes = Vec::with_capacity(self.nodes.len());
        let mut source_to_execution = vec![None; self.nodes.len()];

        for root in &self.roots {
            self.scene_root(root.id)?;
            let start = span_ordinal(nodes.len())?;
            let mut stack = root_children[root.id.index()]
                .iter()
                .rev()
                .copied()
                .map(|source| (source, SurfaceDagExecutionTargetId::SceneRoot(root.id)))
                .collect::<Vec<_>>();
            while let Some((source, receiver)) = stack.pop() {
                self.surface_node(source)?;
                if source_to_execution[source.index()].is_some() {
                    return Err(SurfaceDagError::CyclicSurfaceReceiver(source));
                }
                let id = execution_ordinal(nodes.len())?;
                source_to_execution[source.index()] = Some(id);
                nodes.push(SurfaceDagExecutionNode {
                    id,
                    source,
                    scene_root: root.id,
                    receiver,
                });
                stack.extend(
                    surface_children[source.index()]
                        .iter()
                        .rev()
                        .copied()
                        .map(|child| (child, SurfaceDagExecutionTargetId::Surface(id))),
                );
            }
            let end = span_ordinal(nodes.len())?;
            roots.push(SurfaceDagExecutionRoot {
                identity: *root,
                node_span: start..end,
            });
        }

        let mut dense_source_to_execution = Vec::with_capacity(self.nodes.len());
        for (index, execution) in source_to_execution.into_iter().enumerate() {
            let Some(execution) = execution else {
                return Err(SurfaceDagError::CyclicSurfaceReceiver(self.nodes[index].id));
            };
            dense_source_to_execution.push(execution);
        }
        Ok(SurfaceDagExecutionOrder {
            roots,
            nodes,
            source_to_execution: dense_source_to_execution,
            folded_boundaries: vec![Vec::new(); self.nodes.len()],
            decisions: self
                .nodes
                .iter()
                .map(|node| SurfaceMaterializationDecision {
                    source: node.id,
                    target: node.target,
                    nested_target_count: 0,
                    has_own_raster_content: false,
                    outcome: SurfaceMaterializationOutcome::RetainedUncomposedBoundary,
                })
                .collect(),
        })
    }

    /// Projects the complete logical DAG into the physical retained-target
    /// graph. A pass-through boundary remains fully validated in `SurfaceDag`,
    /// but aliases its sole nested child in the execution projection.
    pub(crate) fn derive_materialized_execution_order(
        &self,
        coverage: &ArtifactSurfaceCoverageForest,
        policy: LayerizationPolicy,
    ) -> Result<SurfaceDagExecutionOrder, SurfaceDagError> {
        match policy {
            LayerizationPolicy::ResolveMaterializedTargets => {}
        }
        if coverage.nodes.len() != self.nodes.len() {
            return Err(SurfaceDagError::MaterializationCoverageCount {
                surfaces: self.nodes.len(),
                coverage: coverage.nodes.len(),
            });
        }

        let mut sole_nested_child = vec![None; self.nodes.len()];
        let mut decisions = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            let coverage = coverage
                .nodes
                .get(node.id.index())
                .filter(|candidate| candidate.surface == node.id)
                .ok_or(SurfaceDagError::UnknownSurfaceReceiver(node.id))?;
            let nested = coverage
                .steps
                .iter()
                .filter_map(|step| match step {
                    ArtifactSurfaceCoverageStep::NestedSurface(child) => Some(*child),
                    ArtifactSurfaceCoverageStep::ArtifactSpan(_) => None,
                })
                .collect::<Vec<_>>();
            let has_own_raster_content = coverage
                .steps
                .iter()
                .any(|step| matches!(step, ArtifactSurfaceCoverageStep::ArtifactSpan(_)));
            let outcome = if has_own_raster_content {
                SurfaceMaterializationOutcome::RetainedOwnRasterContent
            } else if nested.len() != 1 {
                SurfaceMaterializationOutcome::RetainedNestedTargetCount
            } else if let Err(reason) = node.transfer {
                reason
            } else {
                let child = self.surface_node(nested[0])?;
                let transfer = node.transfer.expect("checked transfer");
                let moves_composite = transfer
                    .composite_translation_bits
                    .map(f32::from_bits)
                    .into_iter()
                    .any(|delta| delta != 0.0);
                // Moving a clipped child requires proving clip-space transport,
                // including its consumed scrollport. Keep it until then.
                if moves_composite
                    && (child.transition.clip.from.is_some()
                        || child.transition.clip.to.is_some()
                        || child.clip_rebase.is_some())
                {
                    SurfaceMaterializationOutcome::RetainedClipTransfer
                } else {
                    sole_nested_child[node.id.index()] = nested.first().copied();
                    SurfaceMaterializationOutcome::EliminatedPassThrough
                }
            };
            decisions.push(SurfaceMaterializationDecision {
                source: node.id,
                target: node.target,
                nested_target_count: nested.len(),
                has_own_raster_content,
                outcome,
            });
        }

        // Do not simultaneously erase adjacent pass-through boundaries. One
        // erased boundary has a single typed clip/placement transfer. A chain
        // would require a separate composition proof for those obligations;
        // retaining its outer boundary is the fail-closed generic result, not
        // a property-family special case.
        retain_uncomposed_adjacent_pass_through_boundaries(&mut sole_nested_child, &mut decisions);

        let retained = decisions
            .iter()
            .map(|decision| !decision.is_eliminated())
            .collect::<Vec<_>>();
        let final_receiver = |source: SurfaceDagNodeId| {
            let mut receiver = self.surface_node(source)?.receiver;
            let mut folded = Vec::new();
            let mut seen = FxHashSet::default();
            while let SurfaceDagTargetId::Surface(parent) = receiver {
                if !seen.insert(parent) || seen.len() > usize::from(u8::MAX) {
                    return Err(SurfaceDagError::CyclicSurfaceReceiver(parent));
                }
                if retained[parent.index()] {
                    break;
                }
                folded.push(parent);
                receiver = self.surface_node(parent)?.receiver;
            }
            Ok((receiver, folded))
        };

        let mut root_children = vec![Vec::new(); self.roots.len()];
        let mut surface_children = vec![Vec::new(); self.nodes.len()];
        let mut folded_by_source = vec![Vec::new(); self.nodes.len()];
        for node in self.nodes.iter().filter(|node| retained[node.id.index()]) {
            let (receiver, folded) = final_receiver(node.id)?;
            folded_by_source[node.id.index()] = folded;
            match receiver {
                SurfaceDagTargetId::SceneRoot(root) => {
                    self.scene_root(root)?;
                    root_children[root.index()].push(node.id);
                }
                SurfaceDagTargetId::Surface(parent) => {
                    self.surface_node(parent)?;
                    surface_children[parent.index()].push(node.id);
                }
            }
        }

        let execution_ordinal = |ordinal: usize| {
            u32::try_from(ordinal)
                .map(SurfaceDagExecutionNodeId)
                .map_err(|_| SurfaceDagError::ExecutionNodeOrdinalOverflow(ordinal))
        };
        let span_ordinal = |ordinal: usize| {
            u32::try_from(ordinal)
                .map_err(|_| SurfaceDagError::ExecutionNodeOrdinalOverflow(ordinal))
        };
        let mut roots = Vec::with_capacity(self.roots.len());
        let mut nodes = Vec::with_capacity(retained.iter().filter(|retained| **retained).count());
        let mut source_to_execution = vec![None; self.nodes.len()];

        for root in &self.roots {
            let start = span_ordinal(nodes.len())?;
            let mut stack = root_children[root.id.index()]
                .iter()
                .rev()
                .copied()
                .map(|source| (source, SurfaceDagExecutionTargetId::SceneRoot(root.id)))
                .collect::<Vec<_>>();
            while let Some((source, receiver)) = stack.pop() {
                if source_to_execution[source.index()].is_some() {
                    return Err(SurfaceDagError::CyclicSurfaceReceiver(source));
                }
                let id = execution_ordinal(nodes.len())?;
                source_to_execution[source.index()] = Some(id);
                nodes.push(SurfaceDagExecutionNode {
                    id,
                    source,
                    scene_root: root.id,
                    receiver,
                });
                stack.extend(
                    surface_children[source.index()]
                        .iter()
                        .rev()
                        .copied()
                        .map(|child| (child, SurfaceDagExecutionTargetId::Surface(id))),
                );
            }
            roots.push(SurfaceDagExecutionRoot {
                identity: *root,
                node_span: start..span_ordinal(nodes.len())?,
            });
        }

        for node in self.nodes.iter().filter(|node| !retained[node.id.index()]) {
            let mut cursor = node.id;
            let mut seen = FxHashSet::default();
            let execution = loop {
                if !seen.insert(cursor) || seen.len() > usize::from(u8::MAX) {
                    return Err(SurfaceDagError::CyclicSurfaceReceiver(cursor));
                }
                if let Some(execution) = source_to_execution[cursor.index()] {
                    break execution;
                }
                cursor = sole_nested_child[cursor.index()]
                    .ok_or(SurfaceDagError::MissingMaterializedDescendant(cursor))?;
            };
            source_to_execution[node.id.index()] = Some(execution);
        }
        let source_to_execution = source_to_execution
            .into_iter()
            .enumerate()
            .map(|(index, execution)| {
                execution.ok_or(SurfaceDagError::MissingMaterializedDescendant(
                    self.nodes[index].id,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let folded_boundaries = nodes
            .iter()
            .map(|node| std::mem::take(&mut folded_by_source[node.source.index()]))
            .collect();
        Ok(SurfaceDagExecutionOrder {
            roots,
            nodes,
            source_to_execution,
            folded_boundaries,
            decisions,
        })
    }

    #[cfg(test)]
    pub(crate) fn roots_from_artifact_for_test(
        artifact: &PaintArtifact,
    ) -> Result<Vec<SurfaceDagSceneRoot>, SurfaceDagError> {
        let owners = ArtifactOwnerGraph::try_from_artifact(artifact)?;
        derive_surface_dag_scene_roots(&owners)
    }

    fn scene_root(
        &self,
        id: SurfaceDagSceneRootId,
    ) -> Result<&SurfaceDagSceneRoot, SurfaceDagError> {
        self.roots
            .get(id.index())
            .filter(|root| root.id == id)
            .ok_or(SurfaceDagError::UnknownSceneRootReceiver(id))
    }

    fn surface_node(&self, id: SurfaceDagNodeId) -> Result<&SurfaceDagNode, SurfaceDagError> {
        self.nodes
            .get(id.index())
            .filter(|node| node.id == id)
            .ok_or(SurfaceDagError::UnknownSurfaceReceiver(id))
    }

    /// Resolves a generic receiver to its surface node. This is why collapsing
    /// legacy `Surface(id)` and `ScrollContent(id)` receiver variants loses no
    /// information: the referenced node's own `kind` retains that distinction.
    #[cfg(test)]
    pub(crate) fn receiver_node(
        &self,
        receiver: SurfaceDagTargetId,
    ) -> Result<Option<&SurfaceDagNode>, SurfaceDagError> {
        match receiver {
            SurfaceDagTargetId::SceneRoot(id) => {
                self.scene_root(id)?;
                Ok(None)
            }
            SurfaceDagTargetId::Surface(id) => self.surface_node(id).map(Some),
        }
    }

    #[cfg(test)]
    pub(crate) fn omit_scene_root_for_test(&mut self, id: SurfaceDagSceneRootId) {
        self.roots.retain(|root| root.id != id);
    }

    #[cfg(test)]
    pub(crate) fn set_receiver_for_test(
        &mut self,
        id: SurfaceDagNodeId,
        receiver: SurfaceDagTargetId,
    ) {
        self.nodes[id.index()].receiver = receiver;
    }

    fn validate_receiver_acyclicity(&self) -> Result<(), SurfaceDagError> {
        for origin in &self.nodes {
            let mut receiver = origin.receiver;
            let mut reached_scene_root = false;
            for _ in 0..self.nodes.len() {
                match receiver {
                    SurfaceDagTargetId::SceneRoot(id) => {
                        self.scene_root(id)?;
                        reached_scene_root = true;
                        break;
                    }
                    SurfaceDagTargetId::Surface(id) => {
                        receiver = self.surface_node(id)?.receiver;
                    }
                }
            }
            if !reached_scene_root {
                return Err(SurfaceDagError::CyclicSurfaceReceiver(origin.id));
            }
        }
        Ok(())
    }
}

fn derive_surface_dag_scene_roots(
    owners: &ArtifactOwnerGraph,
) -> Result<Vec<SurfaceDagSceneRoot>, SurfaceDagError> {
    owners
        .scene_roots()
        .iter()
        .copied()
        .map(|scene_target| {
            Ok(SurfaceDagSceneRoot {
                id: SurfaceDagSceneRootId::from_scene_target(scene_target),
                target: scene_target.target(),
                stable_id: owners.stable_id(scene_target.target())?,
            })
        })
        .collect()
}

/// One artifact-derived surface boundary before receiver reconstruction.
///
/// `target` is always the generic artifact owner identity. Candidate order is
/// owner-store order, then Transform -> Effect -> ScrollContent for boundaries
/// co-located on one owner. No arena lookup or planner grammar is available.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ArtifactSurfaceCandidate {
    scene_target: ArtifactSceneTarget,
    cursor: ArtifactCursor,
    kind: SurfaceDagNodeKind,
    clip_rebase: Option<SurfaceDagClipRebase>,
}

impl ArtifactSurfaceCandidate {
    pub(crate) fn target(self) -> NodeKey {
        self.scene_target.target()
    }

    pub(crate) fn cursor(self) -> ArtifactCursor {
        self.cursor
    }

    pub(crate) fn kind(self) -> SurfaceDagNodeKind {
        self.kind
    }

    pub(crate) fn clip_rebase(self) -> Option<SurfaceDagClipRebase> {
        self.clip_rebase
    }

    pub(crate) fn scene_root_receiver(self) -> SurfaceDagTargetId {
        SurfaceDagTargetId::SceneRoot(SurfaceDagSceneRootId::from_scene_target(self.scene_target))
    }

    pub(crate) fn scene_root_ordinal(self) -> u32 {
        self.scene_target.scene_root_ordinal()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SurfaceDagError {
    MissingMaterializationSnapshot(SurfaceDagNodeKind),
    Transition(TransitionError),
    MissingScrollContentsClip {
        scroll: ScrollNodeId,
        expected: ClipNodeId,
    },
    TransitionCount {
        candidates: usize,
        events: usize,
    },
    TransitionSceneRoot {
        index: usize,
        expected: u32,
        actual: u32,
    },
    TransitionTarget {
        index: usize,
        expected: NodeKey,
        actual: NodeKey,
    },
    TransitionCursor {
        index: usize,
        expected: ArtifactCursor,
        actual: ArtifactCursor,
    },
    TransitionKind {
        index: usize,
        expected: SurfaceDagNodeKind,
    },
    ArtifactTransitionDerivationStalled {
        witness: NodeKey,
        state: PropertyTreeState,
    },
    ConflictingArtifactTransition {
        kind: SurfaceDagNodeKind,
        first_witness: NodeKey,
        conflicting_witness: NodeKey,
    },
    ArtifactTransitionTerminalMismatch {
        witness: NodeKey,
        expected: PropertyTreeState,
        actual: PropertyTreeState,
    },
    MissingArtifactTransition {
        kind: SurfaceDagNodeKind,
    },
    NonReceiverClosedChunkSurfaceChain {
        chunk_index: usize,
        surface: SurfaceDagNodeId,
        expected_receiver: SurfaceDagTargetId,
    },
    ClipRebaseOutsideBoundary {
        live: Option<ClipNodeId>,
        boundary: ClipNodeId,
    },
    SurfaceNodeOrdinalOverflow(usize),
    ExecutionNodeOrdinalOverflow(usize),
    UnknownSceneRootReceiver(SurfaceDagSceneRootId),
    UnknownSurfaceReceiver(SurfaceDagNodeId),
    CyclicSurfaceReceiver(SurfaceDagNodeId),
    MaterializationCoverageCount {
        surfaces: usize,
        coverage: usize,
    },
    MissingMaterializedDescendant(SurfaceDagNodeId),
}

impl From<TransitionError> for SurfaceDagError {
    fn from(error: TransitionError) -> Self {
        Self::Transition(error)
    }
}

/// Derives surface-producing candidates exclusively from a closed artifact.
///
/// This is not receiver reconstruction. It freezes the surface-kind mapping
/// and clip-rebase obligation while leaving consumption classification and
/// compositing receivers as separate inputs.
#[cfg(test)]
pub(crate) fn derive_artifact_surface_candidates(
    artifact: &PaintArtifact,
    policy: LayerizationPolicy,
) -> Result<Vec<ArtifactSurfaceCandidate>, SurfaceDagError> {
    let snapshots = PropertySnapshotGraph::try_from_artifact(artifact)?;
    let owners = ArtifactOwnerGraph::try_from_artifact(artifact)?;
    let cursors = artifact_cursors(artifact)?;
    derive_artifact_surface_candidates_from_validated(
        artifact, policy, &snapshots, &owners, &cursors,
    )
}

fn derive_artifact_surface_candidates_from_validated(
    artifact: &PaintArtifact,
    policy: LayerizationPolicy,
    snapshots: &PropertySnapshotGraph,
    owners: &ArtifactOwnerGraph,
    cursors: &[ArtifactCursor],
) -> Result<Vec<ArtifactSurfaceCandidate>, SurfaceDagError> {
    match policy {
        LayerizationPolicy::ResolveMaterializedTargets => {}
    }

    let transforms = artifact
        .transform_nodes
        .iter()
        .map(|snapshot| (snapshot.owner, snapshot.id))
        .collect::<FxHashMap<_, _>>();
    let effects = artifact
        .effect_nodes
        .iter()
        .map(|snapshot| (snapshot.owner, snapshot.id))
        .collect::<FxHashMap<_, _>>();
    let scrolls = artifact
        .scroll_nodes
        .iter()
        .map(|snapshot| (snapshot.owner, snapshot.id))
        .collect::<FxHashMap<_, _>>();

    let mut candidates = Vec::new();
    for owner in artifact.owner_nodes.iter().map(|snapshot| snapshot.owner) {
        let scene_target = owners.scene_target(owner)?;
        let cursor = owners.cursor_for_target(owner, cursors)?;
        if let Some(transform) = transforms.get(&owner).copied() {
            candidates.push(ArtifactSurfaceCandidate {
                scene_target,
                cursor,
                kind: SurfaceDagNodeKind::Transform(transform),
                clip_rebase: None,
            });
        }
        if let Some(effect) = effects.get(&owner).copied() {
            candidates.push(ArtifactSurfaceCandidate {
                scene_target,
                cursor,
                kind: SurfaceDagNodeKind::Effect(effect),
                clip_rebase: None,
            });
        }
        if let Some(scroll) = scrolls.get(&owner).copied() {
            // Scroll snapshots do not carry their contents-clip id. The
            // artifact contract derives it from the canonical owner/role
            // convention, then fails closed if that exact clip is absent.
            let contents_clip = ClipNodeId {
                owner,
                role: ClipNodeRole::ContentsClip,
            };
            snapshots.clip_parent(contents_clip).map_err(|_| {
                SurfaceDagError::MissingScrollContentsClip {
                    scroll,
                    expected: contents_clip,
                }
            })?;
            candidates.push(ArtifactSurfaceCandidate {
                scene_target,
                cursor,
                kind: SurfaceDagNodeKind::ScrollContent {
                    scroll,
                    contents_clip,
                },
                clip_rebase: Some(SurfaceDagClipRebase::from_validated(
                    contents_clip,
                    snapshots,
                )?),
            });
        }
    }
    Ok(candidates)
}

#[derive(Clone, Copy)]
struct DerivedArtifactTransition {
    witness: NodeKey,
    from: PropertyTreeState,
    to: PropertyTreeState,
}

#[derive(Clone, Copy)]
enum ArtifactSurfaceWalkMode {
    BoundaryTransition,
    ChunkCoverage,
}

struct ArtifactSurfaceWalk {
    edges: Vec<(SurfaceDagNodeKind, PropertyTreeState, PropertyTreeState)>,
    matched: Vec<SurfaceDagNodeKind>,
    localized_state: PropertyTreeState,
    local_clips: Vec<(SurfaceDagNodeKind, Vec<ClipNodeSnapshot>)>,
}

fn artifact_owner_path(
    owners: &ArtifactOwnerGraph,
    owner: NodeKey,
) -> Result<Vec<NodeKey>, SurfaceDagError> {
    let mut path = Vec::new();
    let mut cursor = Some(owner);
    while let Some(current) = cursor {
        path.push(current);
        cursor = owners.parent(current)?;
    }
    path.reverse();
    Ok(path)
}

fn local_clip_chain(
    artifact_clips: &FxHashMap<ClipNodeId, ClipNodeSnapshot>,
    live_clip: Option<ClipNodeId>,
    boundary: ClipNodeId,
) -> Result<Vec<ClipNodeSnapshot>, SurfaceDagError> {
    let mut local = Vec::new();
    let mut cursor = live_clip;
    while cursor != Some(boundary) {
        let Some(id) = cursor else {
            return Err(SurfaceDagError::ClipRebaseOutsideBoundary {
                live: live_clip,
                boundary,
            });
        };
        let snapshot = artifact_clips
            .get(&id)
            .copied()
            .ok_or(TransitionError::UnknownClipReference(id))?;
        local.push(snapshot);
        cursor = snapshot.parent;
    }
    if let Some(root) = local.last_mut() {
        root.parent = None;
    }
    Ok(local)
}

fn walk_artifact_surface_path(
    state: PropertyTreeState,
    owner_path: &[NodeKey],
    candidates: &[ArtifactSurfaceCandidate],
    snapshots: &PropertySnapshotGraph,
    artifact_clips: &FxHashMap<ClipNodeId, ClipNodeSnapshot>,
    mode: ArtifactSurfaceWalkMode,
) -> Result<ArtifactSurfaceWalk, SurfaceDagError> {
    // Boundary transitions describe only dimensions actually consumed by a
    // surface edge. Chunk coverage additionally includes ancestor membership:
    // a leaf property state belongs to every same-family surface on its
    // snapshot parent chain even though only the leaf emits an edge. The
    // existing root-to-owner candidate traversal remains the sole ordering
    // authority, so `matched` stays outer-to-inner and its last item remains
    // the direct raster owner. Ancestor membership changes neither `edges`,
    // `localized_state`, nor `local_clips`.
    let membership = match mode {
        ArtifactSurfaceWalkMode::BoundaryTransition => None,
        ArtifactSurfaceWalkMode::ChunkCoverage => Some(snapshots.surface_membership(state)?),
    };
    let mut receiver_state = state;
    let mut localized_state = state;
    let mut edges = Vec::new();
    let mut matched = Vec::new();
    let mut local_clips = Vec::new();

    for path_owner in owner_path {
        for candidate in candidates
            .iter()
            .copied()
            .filter(|candidate| candidate.target() == *path_owner)
        {
            let kind = candidate.kind();
            let is_member = membership.as_ref().is_some_and(|membership| match kind {
                SurfaceDagNodeKind::Transform(transform) => {
                    membership.contains_transform(transform)
                }
                SurfaceDagNodeKind::Effect(effect) => membership.contains_effect(effect),
                SurfaceDagNodeKind::ScrollContent { scroll, .. } => {
                    membership.contains_scroll(scroll)
                }
            });
            let from = receiver_state;
            let consumed = match kind {
                SurfaceDagNodeKind::Transform(transform)
                    if receiver_state.transform == Some(transform) =>
                {
                    receiver_state.transform = None;
                    localized_state.transform = None;
                    true
                }
                SurfaceDagNodeKind::Effect(effect) if receiver_state.effect == Some(effect) => {
                    receiver_state.effect = None;
                    localized_state.effect = None;
                    true
                }
                SurfaceDagNodeKind::ScrollContent {
                    scroll,
                    contents_clip,
                } if receiver_state.scroll == Some(scroll) => {
                    let clips = match mode {
                        ArtifactSurfaceWalkMode::BoundaryTransition => {
                            if receiver_state.clip != Some(contents_clip) {
                                continue;
                            }
                            Vec::new()
                        }
                        ArtifactSurfaceWalkMode::ChunkCoverage => {
                            local_clip_chain(artifact_clips, receiver_state.clip, contents_clip)?
                        }
                    };
                    receiver_state.scroll = None;
                    receiver_state.clip = snapshots.clip_parent(contents_clip)?;
                    localized_state.scroll = None;
                    localized_state.clip = clips.first().map(|clip| clip.id);
                    local_clips.push((candidate.kind(), clips));
                    true
                }
                SurfaceDagNodeKind::Transform(_)
                | SurfaceDagNodeKind::Effect(_)
                | SurfaceDagNodeKind::ScrollContent { .. } => false,
            };
            if consumed {
                edges.push((kind, from, receiver_state));
            }
            if (consumed || is_member) && !matched.contains(&kind) {
                matched.push(kind);
            }
        }
    }

    Ok(ArtifactSurfaceWalk {
        edges,
        matched,
        localized_state,
        local_clips,
    })
}

fn owner_is_ancestor_of(
    owners: &ArtifactOwnerGraph,
    ancestor: NodeKey,
    mut owner: NodeKey,
) -> Result<bool, SurfaceDagError> {
    loop {
        if owner == ancestor {
            return Ok(true);
        }
        let Some(parent) = owners.parent(owner)? else {
            return Ok(false);
        };
        owner = parent;
    }
}

fn consumed_dimensions_match(
    kind: SurfaceDagNodeKind,
    first: DerivedArtifactTransition,
    second: DerivedArtifactTransition,
) -> bool {
    match kind {
        SurfaceDagNodeKind::Transform(_) => {
            first.from.transform == second.from.transform
                && first.to.transform == second.to.transform
        }
        SurfaceDagNodeKind::Effect(_) => {
            first.from.effect == second.from.effect && first.to.effect == second.to.effect
        }
        SurfaceDagNodeKind::ScrollContent { .. } => {
            first.from.scroll == second.from.scroll
                && first.to.scroll == second.to.scroll
                && first.from.clip == second.from.clip
                && first.to.clip == second.to.clip
        }
    }
}

/// Derives the production consumption stream from artifact-owned endpoints.
///
/// Every surface-bearing owner is a witness. A witness starts in its
/// descendants space and scans the root-to-owner candidate path. Deeper
/// witnesses replace shallower same-branch context only after both edges agree
/// on the property dimensions actually consumed. When incomparable descendant
/// branches witness one surface, their non-consumed context may legitimately
/// differ; reconciliation uses the surface owner's own anchor edge after every
/// branch agrees on the consumed dimensions. The anchor is derived before
/// reconciliation and therefore does not depend on sibling enumeration order.
///
/// Transform, effect, and scroll have a receiver carrier, so consuming their
/// local surface identity clears that dimension and the receiver re-establishes
/// ancestor context. Clip has no receiver carrier, so a consumed scroll
/// boundary explicitly advances its contents clip to the clip parent.
///
/// Owners without a surface candidate emit no edge and carry no terminal
/// closure obligation. Their inline property handling remains a dependency on
/// the existing artifact compiler; this function does not establish it.
pub(crate) fn derive_artifact_surface_transition_requests(
    artifact: &PaintArtifact,
    policy: LayerizationPolicy,
) -> Result<Vec<ArtifactTransitionRequest>, SurfaceDagError> {
    let snapshots = PropertySnapshotGraph::try_from_artifact(artifact)?;
    let owners = ArtifactOwnerGraph::try_from_artifact(artifact)?;
    let candidates = derive_artifact_surface_candidates_from_validated(
        artifact,
        policy,
        &snapshots,
        &owners,
        &artifact_cursors(artifact)?,
    )?;
    let artifact_clips = artifact
        .clip_nodes
        .iter()
        .map(|snapshot| (snapshot.id, *snapshot))
        .collect::<FxHashMap<_, _>>();
    let endpoints = artifact
        .owner_property_states
        .iter()
        .map(|snapshot| (snapshot.owner, *snapshot))
        .collect::<FxHashMap<_, _>>();
    let candidate_targets = candidates
        .iter()
        .map(|candidate| (candidate.kind(), candidate.target()))
        .collect::<FxHashMap<_, _>>();
    let mut observed = FxHashMap::<SurfaceDagNodeKind, Vec<DerivedArtifactTransition>>::default();

    let mut witness_owners = Vec::new();
    for candidate in &candidates {
        if witness_owners.last().copied() != Some(candidate.target()) {
            witness_owners.push(candidate.target());
        }
    }

    for witness in witness_owners {
        let endpoint = endpoints
            .get(&witness)
            .copied()
            .ok_or(TransitionError::MissingOwnerPropertyState(witness))?;
        let owner_path = artifact_owner_path(&owners, witness)?;
        let walk = walk_artifact_surface_path(
            endpoint.descendants,
            &owner_path,
            &candidates,
            &snapshots,
            &artifact_clips,
            ArtifactSurfaceWalkMode::BoundaryTransition,
        )?;
        let cursor = walk
            .edges
            .last()
            .map(|(_, _, to)| *to)
            .unwrap_or(endpoint.descendants);
        let mut local_edges = walk.edges;
        if local_edges.is_empty() {
            return Err(SurfaceDagError::ArtifactTransitionDerivationStalled {
                witness,
                state: cursor,
            });
        }

        let mut projected_paint = endpoint.paint;
        for path_owner in &owner_path {
            for candidate in candidates
                .iter()
                .copied()
                .filter(|candidate| candidate.target() == *path_owner)
            {
                match candidate.kind() {
                    SurfaceDagNodeKind::Transform(transform)
                        if projected_paint.transform == Some(transform) =>
                    {
                        projected_paint.transform = None;
                    }
                    SurfaceDagNodeKind::Effect(effect)
                        if projected_paint.effect == Some(effect) =>
                    {
                        projected_paint.effect = None;
                    }
                    SurfaceDagNodeKind::ScrollContent { scroll, .. }
                        if projected_paint.scroll == Some(scroll) =>
                    {
                        projected_paint.scroll = None;
                    }
                    SurfaceDagNodeKind::Transform(_)
                    | SurfaceDagNodeKind::Effect(_)
                    | SurfaceDagNodeKind::ScrollContent { .. } => {}
                }
            }
        }

        if cursor.transform != projected_paint.transform
            || cursor.effect != projected_paint.effect
            || cursor.scroll != projected_paint.scroll
        {
            return Err(SurfaceDagError::ArtifactTransitionTerminalMismatch {
                witness,
                expected: projected_paint,
                actual: cursor,
            });
        }
        if cursor != projected_paint {
            let (_, _, terminal) = local_edges
                .last_mut()
                .expect("a surface-bearing witness matched at least one candidate");
            terminal.clip = projected_paint.clip;
            terminal.layout_position = projected_paint.layout_position;
            terminal.visual_offset = projected_paint.visual_offset;
        }

        for (kind, from, to) in local_edges {
            observed
                .entry(kind)
                .or_default()
                .push(DerivedArtifactTransition { witness, from, to });
        }
    }

    let mut derived = FxHashMap::<SurfaceDagNodeKind, DerivedArtifactTransition>::default();
    for candidate in &candidates {
        let kind = candidate.kind();
        let observations = observed
            .get(&kind)
            .ok_or(SurfaceDagError::MissingArtifactTransition { kind })?;
        let mut deepest = observations[0];
        let mut incomparable = None;
        for next in observations.iter().copied().skip(1) {
            if deepest.witness == next.witness {
                if deepest.from != next.from || deepest.to != next.to {
                    return Err(SurfaceDagError::ConflictingArtifactTransition {
                        kind,
                        first_witness: deepest.witness,
                        conflicting_witness: next.witness,
                    });
                }
                continue;
            }
            if !consumed_dimensions_match(kind, deepest, next) {
                return Err(SurfaceDagError::ConflictingArtifactTransition {
                    kind,
                    first_witness: deepest.witness,
                    conflicting_witness: next.witness,
                });
            }
            if owner_is_ancestor_of(&owners, deepest.witness, next.witness)? {
                deepest = next;
            } else if !owner_is_ancestor_of(&owners, next.witness, deepest.witness)? {
                incomparable.get_or_insert((deepest.witness, next.witness));
            }
        }
        let edge = if let Some((first_witness, conflicting_witness)) = incomparable {
            observations
                .iter()
                .copied()
                .find(|edge| {
                    edge.witness
                        == candidate_targets
                            .get(&kind)
                            .copied()
                            .expect("every candidate kind has one target")
                })
                .ok_or(SurfaceDagError::ConflictingArtifactTransition {
                    kind,
                    first_witness,
                    conflicting_witness,
                })?
        } else {
            deepest
        };
        derived.insert(kind, edge);
    }

    candidates
        .into_iter()
        .map(|candidate| {
            let edge = derived.get(&candidate.kind()).copied().ok_or(
                SurfaceDagError::MissingArtifactTransition {
                    kind: candidate.kind(),
                },
            )?;
            Ok(ArtifactTransitionRequest::new(
                candidate.target(),
                edge.from,
                edge.to,
            ))
        })
        .collect()
}

fn surface_kind_family(kind: SurfaceDagNodeKind) -> u8 {
    match kind {
        SurfaceDagNodeKind::Transform(_) => 0,
        SurfaceDagNodeKind::Effect(_) => 1,
        SurfaceDagNodeKind::ScrollContent { .. } => 2,
    }
}

fn append_coverage_span(
    steps: &mut Vec<ArtifactSurfaceCoverageStep>,
    chunk_index: usize,
    op_range: Range<usize>,
    localized_state: PropertyTreeState,
    local_clips: &[ClipNodeSnapshot],
) {
    if let Some(ArtifactSurfaceCoverageStep::ArtifactSpan(span)) = steps.last_mut()
        && span.chunk_range.end == chunk_index
        && span.op_range.end == op_range.start
    {
        span.chunk_range.end = chunk_index + 1;
        span.op_range.end = op_range.end;
        span.localized_states.push(localized_state);
        for clip in local_clips {
            if !span
                .local_clips
                .iter()
                .any(|existing| existing.id == clip.id)
            {
                span.local_clips.push(*clip);
            }
        }
        return;
    }
    steps.push(ArtifactSurfaceCoverageStep::ArtifactSpan(
        ArtifactSurfaceCoverageSpan {
            chunk_range: chunk_index..chunk_index + 1,
            op_range,
            localized_states: vec![localized_state],
            local_clips: local_clips.to_vec(),
        },
    ));
}

fn append_coverage_span_to_target(
    roots: &mut [ArtifactSurfaceCoverageRoot],
    nodes: &mut [ArtifactSurfaceCoverageNode],
    target: SurfaceDagTargetId,
    chunk_index: usize,
    op_range: Range<usize>,
    localized_state: PropertyTreeState,
    local_clips: &[ClipNodeSnapshot],
) {
    let steps = match target {
        SurfaceDagTargetId::SceneRoot(root) => &mut roots[root.index()].steps,
        SurfaceDagTargetId::Surface(surface) => &mut nodes[surface.index()].steps,
    };
    append_coverage_span(steps, chunk_index, op_range, localized_state, local_clips);
}

/// Returns the ScrollContent surface whose own boundary-mask sentinel this
/// chunk represents. That mask clips the composite in the receiver; it is not
/// part of the detached surface's offset-zero resident raster program.
fn scroll_boundary_mask_surface(
    chunk: &PaintChunk,
    logical_chain: &[SurfaceDagNodeId],
    surface_dag: &SurfaceDag,
) -> Result<Option<SurfaceDagNodeId>, SurfaceDagError> {
    if chunk.id.slot != RETAINED_CHILD_MASK_SLOT {
        return Ok(None);
    }
    for surface in logical_chain.iter().rev().copied() {
        let node = surface_dag.surface_node(surface)?;
        if matches!(
            node.kind,
            SurfaceDagNodeKind::ScrollContent { scroll, .. } if scroll.0 == chunk.id.owner
        ) {
            return Ok(Some(surface));
        }
    }
    Ok(None)
}

/// Derives hierarchical painter coverage without minting raster stamps or
/// admitting detached surfaces into production.
///
/// Artifact cursor order fixes direct span order. Surface receiver order fixes
/// logical nesting and is never used as a substitute painter ordinal. Each
/// chunk appears directly once at its innermost active surface; ancestors own
/// only a `NestedSurface` step. The output vocabulary deliberately contains no
/// exact-shape legacy raster dependency.
pub(crate) fn derive_artifact_surface_coverage_forest(
    artifact: &PaintArtifact,
    surface_dag: &SurfaceDag,
    policy: LayerizationPolicy,
) -> Result<ArtifactSurfaceCoverageForest, SurfaceDagError> {
    let snapshots = PropertySnapshotGraph::try_from_artifact(artifact)?;
    let owners = ArtifactOwnerGraph::try_from_artifact(artifact)?;
    let cursors = artifact_cursors(artifact)?;
    let candidates = derive_artifact_surface_candidates_from_validated(
        artifact, policy, &snapshots, &owners, &cursors,
    )?;
    if candidates.len() != surface_dag.nodes.len() {
        return Err(SurfaceDagError::TransitionCount {
            candidates: candidates.len(),
            events: surface_dag.nodes.len(),
        });
    }
    for (index, (candidate, node)) in candidates.iter().zip(&surface_dag.nodes).enumerate() {
        if candidate.target() != node.target {
            return Err(SurfaceDagError::TransitionTarget {
                index,
                expected: candidate.target(),
                actual: node.target,
            });
        }
        if candidate.cursor() != node.cursor {
            return Err(SurfaceDagError::TransitionCursor {
                index,
                expected: candidate.cursor(),
                actual: node.cursor,
            });
        }
        if candidate.kind() != node.kind {
            return Err(SurfaceDagError::TransitionKind {
                index,
                expected: candidate.kind(),
            });
        }
    }

    let artifact_clips = artifact
        .clip_nodes
        .iter()
        .map(|snapshot| (snapshot.id, *snapshot))
        .collect::<FxHashMap<_, _>>();
    let node_by_kind = surface_dag
        .nodes
        .iter()
        .map(|node| (node.kind, node.id))
        .collect::<FxHashMap<_, _>>();
    let mut roots = surface_dag
        .roots
        .iter()
        .map(|root| ArtifactSurfaceCoverageRoot {
            scene_root: root.id,
            steps: Vec::new(),
        })
        .collect::<Vec<_>>();
    let mut nodes = surface_dag
        .nodes
        .iter()
        .map(|node| ArtifactSurfaceCoverageNode {
            surface: node.id,
            steps: Vec::new(),
            receiver_mask_envelope_ranges: Vec::new(),
            clip_closure: node
                .clip_rebase
                .map(|rebase| SurfaceDagClipClosureProjection {
                    receiver_clip: rebase.receiver_clip,
                    local_clips: Vec::new(),
                }),
        })
        .collect::<Vec<_>>();
    let mut closure_clip_ids = vec![FxHashSet::default(); nodes.len()];
    let mut nested_inserted = vec![false; nodes.len()];

    for (chunk_index, chunk) in artifact.chunks.iter().enumerate() {
        let owner_path = artifact_owner_path(&owners, chunk.owner)?;
        let walk = walk_artifact_surface_path(
            chunk.properties,
            &owner_path,
            &candidates,
            &snapshots,
            &artifact_clips,
            ArtifactSurfaceWalkMode::ChunkCoverage,
        )?;
        let matched_ids = walk
            .matched
            .iter()
            .filter_map(|kind| node_by_kind.get(kind).copied())
            .collect::<Vec<_>>();
        let matched_set = matched_ids.iter().copied().collect::<FxHashSet<_>>();

        let scene_root =
            SurfaceDagSceneRootId::from_scene_target(owners.scene_target(chunk.owner)?);
        let logical_chain = if let Some(innermost) = matched_ids.last().copied() {
            let mut reversed = Vec::new();
            let mut cursor = SurfaceDagTargetId::Surface(innermost);
            let mut descendant_kind = surface_dag.surface_node(innermost)?.kind;
            loop {
                match cursor {
                    SurfaceDagTargetId::SceneRoot(root) => {
                        if root != scene_root {
                            return Err(SurfaceDagError::NonReceiverClosedChunkSurfaceChain {
                                chunk_index,
                                surface: innermost,
                                expected_receiver: SurfaceDagTargetId::SceneRoot(scene_root),
                            });
                        }
                        break;
                    }
                    SurfaceDagTargetId::Surface(id) => {
                        let node = surface_dag.surface_node(id)?;
                        if !matched_set.contains(&id)
                            && surface_kind_family(node.kind)
                                != surface_kind_family(descendant_kind)
                        {
                            return Err(SurfaceDagError::NonReceiverClosedChunkSurfaceChain {
                                chunk_index,
                                surface: innermost,
                                expected_receiver: SurfaceDagTargetId::Surface(id),
                            });
                        }
                        reversed.push(id);
                        descendant_kind = node.kind;
                        cursor = node.receiver;
                    }
                }
            }
            if matched_ids.iter().any(|id| !reversed.contains(id)) {
                return Err(SurfaceDagError::NonReceiverClosedChunkSurfaceChain {
                    chunk_index,
                    surface: innermost,
                    expected_receiver: surface_dag.surface_node(innermost)?.receiver,
                });
            }
            reversed.reverse();
            reversed
        } else {
            Vec::new()
        };

        for (kind, clips) in &walk.local_clips {
            let Some(surface) = node_by_kind.get(kind).copied() else {
                continue;
            };
            let Some(closure) = nodes[surface.index()].clip_closure.as_mut() else {
                continue;
            };
            for clip in clips {
                if closure_clip_ids[surface.index()].insert(clip.id) {
                    closure.local_clips.push(*clip);
                } else if !closure.local_clips.iter().any(|existing| existing == clip) {
                    return Err(SurfaceDagError::ClipRebaseOutsideBoundary {
                        live: Some(clip.id),
                        boundary: match kind {
                            SurfaceDagNodeKind::ScrollContent { contents_clip, .. } => {
                                *contents_clip
                            }
                            _ => clip.id,
                        },
                    });
                }
            }
        }

        let boundary_mask_surface =
            scroll_boundary_mask_surface(chunk, &logical_chain, surface_dag)?;
        if let Some(surface) = boundary_mask_surface
            && chunk.id.phase == PaintNodePhase::BeforeChildren
        {
            nodes[surface.index()]
                .receiver_mask_envelope_ranges
                .push(chunk_index..chunk_index + 1);
        }
        let direct_target = boundary_mask_surface
            .map(|surface| surface_dag.nodes[surface.index()].receiver)
            .unwrap_or_else(|| {
                logical_chain
                    .last()
                    .copied()
                    .map(SurfaceDagTargetId::Surface)
                    .unwrap_or(SurfaceDagTargetId::SceneRoot(scene_root))
            });
        let direct_local_clips = match direct_target {
            SurfaceDagTargetId::SceneRoot(_) => &[][..],
            SurfaceDagTargetId::Surface(surface) => walk
                .local_clips
                .iter()
                .find(|(kind, _)| node_by_kind.get(kind) == Some(&surface))
                .map(|(_, clips)| clips.as_slice())
                .unwrap_or(&[]),
        };

        // The opening boundary mask must execute in the receiver before the
        // nested ScrollContent composite. Its closing sentinel is appended
        // after the nested step below. Ordinary chunks keep their existing
        // direct-after-nesting order.
        if boundary_mask_surface.is_some() && chunk.id.phase == PaintNodePhase::BeforeChildren {
            append_coverage_span_to_target(
                &mut roots,
                &mut nodes,
                direct_target,
                chunk_index,
                chunk.op_range.clone(),
                walk.localized_state,
                direct_local_clips,
            );
        }

        for surface in &logical_chain {
            if nested_inserted[surface.index()] {
                continue;
            }
            let receiver = surface_dag.surface_node(*surface)?.receiver;
            match receiver {
                SurfaceDagTargetId::SceneRoot(root) => roots[root.index()]
                    .steps
                    .push(ArtifactSurfaceCoverageStep::NestedSurface(*surface)),
                SurfaceDagTargetId::Surface(parent) => nodes[parent.index()]
                    .steps
                    .push(ArtifactSurfaceCoverageStep::NestedSurface(*surface)),
            }
            nested_inserted[surface.index()] = true;
        }

        if boundary_mask_surface.is_none() || chunk.id.phase == PaintNodePhase::AfterChildren {
            append_coverage_span_to_target(
                &mut roots,
                &mut nodes,
                direct_target,
                chunk_index,
                chunk.op_range.clone(),
                walk.localized_state,
                direct_local_clips,
            )
        }
    }

    Ok(ArtifactSurfaceCoverageForest { roots, nodes })
}

/// Artifact-local attachment invariant between one classified endpoint pair
/// and one surface candidate. This is not an independent source differential:
/// both inputs originate in the artifact. It still rejects event/candidate
/// pairing mistakes because the transition must consume this candidate's
/// specific property id, not merely any property of the same family.
fn transition_consumes_kind(transition: PropertyStateTransition, kind: SurfaceDagNodeKind) -> bool {
    match kind {
        SurfaceDagNodeKind::Transform(transform) => {
            transition.transform.from == Some(transform) && transition.transform.is_changed()
        }
        SurfaceDagNodeKind::Effect(effect) => {
            transition.effect.from == Some(effect) && transition.effect.is_changed()
        }
        SurfaceDagNodeKind::ScrollContent {
            scroll,
            contents_clip,
        } => {
            transition.scroll.from == Some(scroll)
                && transition.scroll.is_changed()
                && transition.clip.from == Some(contents_clip)
                && transition.clip.is_changed()
        }
    }
}

fn transition_states(
    transition: PropertyStateTransition,
) -> (PropertyTreeState, PropertyTreeState) {
    (
        PropertyTreeState {
            transform: transition.transform.from,
            clip: transition.clip.from,
            effect: transition.effect.from,
            scroll: transition.scroll.from,
            layout_position: transition.layout_position.from,
            visual_offset: transition.visual_offset.from,
        },
        PropertyTreeState {
            transform: transition.transform.to,
            clip: transition.clip.to,
            effect: transition.effect.to,
            scroll: transition.scroll.to,
            layout_position: transition.layout_position.to,
            visual_offset: transition.visual_offset.to,
        },
    )
}

/// Reconstructs the ordered C2 surface graph without planner grammar or arena
/// access. Consumption classification and compositing receiver derivation are
/// deliberately separate: events supply the former, the artifact owner forest
/// supplies the latter. Candidate and event order remains artifact store order;
/// receiver resolution is a separate pass and therefore accepts both
/// ancestor-first fixtures and the production recorder's leaf-first owner
/// snapshots.
pub(crate) fn reconstruct_surface_dag(
    artifact: &PaintArtifact,
    events: &[ClassifiedTransitionEvent],
    policy: LayerizationPolicy,
) -> Result<SurfaceDag, SurfaceDagError> {
    let snapshots = PropertySnapshotGraph::try_from_artifact(artifact)?;
    let owners = ArtifactOwnerGraph::try_from_artifact(artifact)?;
    let cursors = artifact_cursors(artifact)?;
    let candidates = derive_artifact_surface_candidates_from_validated(
        artifact, policy, &snapshots, &owners, &cursors,
    )?;
    let roots = derive_surface_dag_scene_roots(&owners)?;
    if candidates.len() != events.len() {
        return Err(SurfaceDagError::TransitionCount {
            candidates: candidates.len(),
            events: events.len(),
        });
    }
    // The final candidate for one owner is its canonical innermost surface:
    // candidate derivation fixes the local order as Transform -> Effect ->
    // ScrollContent. This complete map makes ancestor lookup independent of
    // whether owner snapshots are ancestor-first or leaf-first.
    let mut ids = Vec::with_capacity(candidates.len());
    let mut innermost_id_by_owner = FxHashMap::default();
    for (index, candidate) in candidates.iter().enumerate() {
        let id = SurfaceDagNodeId(
            u32::try_from(index).map_err(|_| SurfaceDagError::SurfaceNodeOrdinalOverflow(index))?,
        );
        ids.push(id);
        innermost_id_by_owner.insert(candidate.target(), id);
    }

    let transforms = artifact
        .transform_nodes
        .iter()
        .map(|s| (s.id, *s))
        .collect();
    let effects = artifact.effect_nodes.iter().map(|s| (s.id, *s)).collect();
    let scrolls = artifact.scroll_nodes.iter().map(|s| (s.id, *s)).collect();
    let mut previous_id_by_owner = FxHashMap::default();
    let mut nodes = Vec::with_capacity(candidates.len());

    for (index, ((candidate, event), id)) in candidates.into_iter().zip(events).zip(ids).enumerate()
    {
        let scene_root_receiver = candidate.scene_root_receiver();
        let expected_scene_root = candidate.scene_root_ordinal();
        if event.scene_root_ordinal() != expected_scene_root {
            return Err(SurfaceDagError::TransitionSceneRoot {
                index,
                expected: expected_scene_root,
                actual: event.scene_root_ordinal(),
            });
        }
        if event.target() != candidate.target() {
            return Err(SurfaceDagError::TransitionTarget {
                index,
                expected: candidate.target(),
                actual: event.target(),
            });
        }
        if event.cursor() != candidate.cursor() {
            return Err(SurfaceDagError::TransitionCursor {
                index,
                expected: candidate.cursor(),
                actual: event.cursor(),
            });
        }
        let (from, to) = transition_states(event.transition());
        let transition = classify_property_transition(from, to, &snapshots)?;
        if !transition_consumes_kind(transition, candidate.kind()) {
            return Err(SurfaceDagError::TransitionKind {
                index,
                expected: candidate.kind(),
            });
        }

        let owner = candidate.target();
        let receiver = if let Some(id) = previous_id_by_owner.get(&owner).copied() {
            SurfaceDagTargetId::Surface(id)
        } else {
            let mut cursor = owners.parent(owner)?;
            let mut found = None;
            while let Some(ancestor) = cursor {
                if let Some(id) = innermost_id_by_owner.get(&ancestor).copied() {
                    found = Some(id);
                    break;
                }
                cursor = owners.parent(ancestor)?;
            }
            found.map_or(scene_root_receiver, SurfaceDagTargetId::Surface)
        };
        nodes.push(SurfaceDagNode {
            id,
            target: owner,
            stable_id: owners.stable_id(owner)?,
            cursor: candidate.cursor(),
            kind: candidate.kind(),
            receiver,
            transition,
            clip_rebase: candidate.clip_rebase(),
            transfer: boundary_transfer(
                candidate.kind(),
                transition,
                candidate.clip_rebase(),
                &transforms,
                &effects,
                &scrolls,
            )?,
        });
        previous_id_by_owner.insert(owner, id);
    }

    let surface_dag = SurfaceDag { roots, nodes };
    surface_dag.validate_receiver_acyclicity()?;
    Ok(surface_dag)
}
