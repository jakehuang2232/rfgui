#![allow(dead_code)] // The C2 graph remains graph-inert until the C3 consumer lands.

use std::ops::Range;

use rustc_hash::FxHashMap;

use crate::view::{
    compositor::property_tree::{
        ClipNodeId, ClipNodeRole, ClipNodeSnapshot, EffectNodeId, PropertyStateTransition,
        PropertyTreeState, ScrollNodeId, TransformNodeId,
    },
    node_arena::NodeKey,
};

use super::{
    ArtifactCursor, ArtifactOwnerGraph, ArtifactSceneTarget, ClassifiedTransitionEvent,
    PaintArtifact, PropertySnapshotGraph, TransitionError, artifact_cursors,
    classify_property_transition,
};

/// Explicit C2 layerization input. The first policy preserves every authored
/// property boundary; adding another policy is an enum extension rather than
/// a hidden global behavior change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayerizationPolicy {
    PreservePropertyBoundaries,
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
    pub(crate) fn id(self) -> SurfaceDagSceneRootId {
        self.id
    }

    pub(crate) fn target(self) -> NodeKey {
        self.target
    }

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
    pub(crate) fn identity(&self) -> SurfaceDagSceneRoot {
        self.identity
    }

    pub(crate) fn node_span(&self) -> Range<u32> {
        self.node_span.clone()
    }
}

/// One source surface in parent-before-child execution order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceDagExecutionNode {
    id: SurfaceDagExecutionNodeId,
    source: SurfaceDagNodeId,
    scene_root: SurfaceDagSceneRootId,
    receiver: SurfaceDagExecutionTargetId,
}

impl SurfaceDagExecutionNode {
    pub(crate) fn id(self) -> SurfaceDagExecutionNodeId {
        self.id
    }

    pub(crate) fn source(self) -> SurfaceDagNodeId {
        self.source
    }

    pub(crate) fn scene_root(self) -> SurfaceDagSceneRootId {
        self.scene_root
    }

    pub(crate) fn receiver(self) -> SurfaceDagExecutionTargetId {
        self.receiver
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
}

impl SurfaceDagExecutionOrder {
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

    pub(crate) fn try_from_artifact(
        artifact: &PaintArtifact,
        local_clip: ClipNodeId,
    ) -> Result<Self, SurfaceDagError> {
        let snapshots = PropertySnapshotGraph::try_from_artifact(artifact)?;
        let scroll = ScrollNodeId(local_clip.owner);
        let expected = ClipNodeId {
            owner: local_clip.owner,
            role: ClipNodeRole::ContentsClip,
        };
        if local_clip != expected
            || !artifact
                .scroll_nodes
                .iter()
                .any(|snapshot| snapshot.id == scroll && snapshot.owner == local_clip.owner)
        {
            return Err(SurfaceDagError::MissingScrollContentsClip { scroll, expected });
        }
        Self::from_validated(local_clip, &snapshots)
    }

    pub(crate) fn receiver_clip(self) -> Option<ClipNodeId> {
        self.receiver_clip
    }

    pub(crate) fn local_clip(self) -> ClipNodeId {
        self.local_clip
    }

    /// Splits one live clip chain at this scroll contents boundary.
    ///
    /// The boundary and its ancestor suffix stay in receiver space. The
    /// descendant prefix remains raster-local, with its nearest-boundary node
    /// detached into a local root. Spatial payload comes only from the
    /// artifact snapshots; neither PropertyTrees nor a legacy witness is
    /// available here.
    ///
    /// Local snapshot generations deliberately remain live generations. The
    /// specialized legacy TextArea path instead requires
    /// `DETACHED_LOCAL_CLIP_GENERATION`; reconciling or retiring those compiler
    /// admission predicates belongs to the C3b pre-reuse admission
    /// reconciliation batch and must precede any C3b reuse claim.
    pub(crate) fn project_clip_space(
        self,
        artifact: &PaintArtifact,
        live: PropertyTreeState,
    ) -> Result<SurfaceDagClipProjection, SurfaceDagError> {
        let snapshots = PropertySnapshotGraph::try_from_artifact(artifact)?;
        snapshots.validate_state(live)?;
        let scroll = ScrollNodeId(self.local_clip.owner);
        if live.scroll != Some(scroll) {
            return Err(SurfaceDagError::ClipRebaseScroll {
                expected: scroll,
                actual: live.scroll,
            });
        }
        if snapshots.clip_parent(self.local_clip)? != self.receiver_clip {
            return Err(SurfaceDagError::ClipRebaseOutsideBoundary {
                live: live.clip,
                boundary: self.local_clip,
            });
        }

        let clips = artifact
            .clip_nodes
            .iter()
            .map(|snapshot| (snapshot.id, *snapshot))
            .collect::<FxHashMap<_, _>>();
        let mut local_clips = Vec::new();
        let mut cursor = live.clip;
        while cursor != Some(self.local_clip) {
            let Some(id) = cursor else {
                return Err(SurfaceDagError::ClipRebaseOutsideBoundary {
                    live: live.clip,
                    boundary: self.local_clip,
                });
            };
            let snapshot = clips
                .get(&id)
                .copied()
                .ok_or(TransitionError::UnknownClipReference(id))?;
            local_clips.push(snapshot);
            cursor = snapshot.parent;
        }
        if let Some(root) = local_clips.last_mut() {
            root.parent = None;
        }
        let local_clip = local_clips.first().map(|snapshot| snapshot.id);
        Ok(SurfaceDagClipProjection {
            receiver_clip: self.receiver_clip,
            local_state: PropertyTreeState {
                clip: local_clip,
                scroll: None,
                ..live
            },
            local_clips,
        })
    }
}

/// Artifact-derived split between receiver and detached raster clip spaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceDagClipProjection {
    receiver_clip: Option<ClipNodeId>,
    local_state: PropertyTreeState,
    local_clips: Vec<ClipNodeSnapshot>,
}

impl SurfaceDagClipProjection {
    pub(crate) fn receiver_clip(&self) -> Option<ClipNodeId> {
        self.receiver_clip
    }

    pub(crate) fn local_state(&self) -> PropertyTreeState {
        self.local_state
    }

    pub(crate) fn local_clips(&self) -> &[ClipNodeSnapshot] {
        &self.local_clips
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
}

impl SurfaceDagNode {
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

    pub(crate) fn cursor(&self) -> ArtifactCursor {
        self.cursor
    }

    pub(crate) fn kind(&self) -> SurfaceDagNodeKind {
        self.kind
    }

    pub(crate) fn receiver(&self) -> SurfaceDagTargetId {
        self.receiver
    }

    pub(crate) fn transition(&self) -> PropertyStateTransition {
        self.transition
    }

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
    ClipRebaseScroll {
        expected: ScrollNodeId,
        actual: Option<ScrollNodeId>,
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
        LayerizationPolicy::PreservePropertyBoundaries => {}
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
        });
        previous_id_by_owner.insert(owner, id);
    }

    let surface_dag = SurfaceDag { roots, nodes };
    surface_dag.validate_receiver_acyclicity()?;
    Ok(surface_dag)
}
