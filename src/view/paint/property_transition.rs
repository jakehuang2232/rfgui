#![allow(dead_code)] // C1 classifier; the C2 reconstructor becomes its first production caller.

use rustc_hash::{FxHashMap, FxHashSet};
use slotmap::Key;

use crate::view::{
    compositor::property_tree::{
        ClipBehavior, ClipNodeId, ClipNodeRole, EffectNodeId, LayoutPositionNodeId,
        PropertyStateTransition, PropertyTreeState, ScrollNodeId, SpatialPositionReference,
        SpatialProjectionError, SpatialProjectionGraph, TransformNodeId, VisualOffsetNodeId,
    },
    node_arena::NodeKey,
};

use super::PaintArtifact;

/// Closed C1 rejection taxonomy for snapshot-backed transition classification.
///
/// The spatial graph keeps its lower-layer typed reason intact. All six parent
/// forests start from their artifact-store `Vec` order even though lookup is
/// hash-based, so a cycle always reports the same repeated identity. Clip and
/// effect have no shared compositor graph constructor and are validated at
/// this artifact boundary with the same deterministic rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransitionError {
    SpatialSnapshot(SpatialProjectionError),
    DuplicateClip(ClipNodeId),
    InvalidClip(ClipNodeId),
    MissingClip(ClipNodeId),
    CyclicClip(ClipNodeId),
    DuplicateEffect(EffectNodeId),
    InvalidEffect(EffectNodeId),
    MissingEffect(EffectNodeId),
    CyclicEffect(EffectNodeId),
    UnknownTransformReference(TransformNodeId),
    UnknownClipReference(ClipNodeId),
    UnknownEffectReference(EffectNodeId),
    UnknownScrollReference(ScrollNodeId),
    UnknownLayoutPositionReference(LayoutPositionNodeId),
    UnknownVisualOffsetReference(VisualOffsetNodeId),
    DuplicateOwnerPropertyState(NodeKey),
    MissingOwnerPropertyState(NodeKey),
    UnreferencedOwnerPropertyState(NodeKey),
    InvalidOwnerStableId {
        owner: NodeKey,
        stable_id: u64,
    },
    DuplicateOwnerStableId {
        stable_id: u64,
        first_owner: NodeKey,
        duplicate_owner: NodeKey,
    },
    InvalidOwnerPropertyState {
        owner: NodeKey,
        endpoint: OwnerPropertyStateEndpoint,
        reason: PropertyStateReferenceError,
    },
    DuplicateOwner(NodeKey),
    InvalidOwner(NodeKey),
    MissingOwnerParent(NodeKey),
    CyclicOwner(NodeKey),
    UnknownTarget(NodeKey),
    UnreferencedOwner(NodeKey),
    SceneRootOrdinalOverflow(NodeKey),
    InvalidArtifactCursor(usize),
    NonTerminalArtifactCursor(usize),
    OutOfOrderArtifactCursor {
        previous_chunk: usize,
        current_chunk: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnerPropertyStateEndpoint {
    Paint,
    Descendants,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PropertyStateReferenceError {
    UnknownTransform(TransformNodeId),
    UnknownClip(ClipNodeId),
    UnknownEffect(EffectNodeId),
    UnknownScroll(ScrollNodeId),
    UnknownLayoutPosition(LayoutPositionNodeId),
    UnknownVisualOffset(VisualOffsetNodeId),
}

/// Validates the one keyed owner store shared by property-state and owner
/// graph construction. Keeping exact owner coverage and persistent identity
/// checks here prevents the two artifact views from accepting different
/// owner sets or duplicate stable identities.
fn validate_owner_property_state_store(
    artifact: &PaintArtifact,
) -> Result<FxHashMap<NodeKey, u64>, TransitionError> {
    let mut owner_property_states = FxHashSet::default();
    for snapshot in &artifact.owner_property_states {
        if !owner_property_states.insert(snapshot.owner) {
            return Err(TransitionError::DuplicateOwnerPropertyState(snapshot.owner));
        }
    }
    let owner_nodes = artifact
        .owner_nodes
        .iter()
        .map(|snapshot| snapshot.owner)
        .collect::<FxHashSet<_>>();
    if let Some(snapshot) = artifact
        .owner_property_states
        .iter()
        .find(|snapshot| !owner_nodes.contains(&snapshot.owner))
    {
        return Err(TransitionError::UnreferencedOwnerPropertyState(
            snapshot.owner,
        ));
    }
    if let Some(snapshot) = artifact
        .owner_nodes
        .iter()
        .find(|snapshot| !owner_property_states.contains(&snapshot.owner))
    {
        return Err(TransitionError::MissingOwnerPropertyState(snapshot.owner));
    }

    let mut owners_by_stable_id = FxHashMap::default();
    let mut stable_ids = FxHashMap::default();
    for snapshot in &artifact.owner_property_states {
        if snapshot.stable_id == 0 {
            return Err(TransitionError::InvalidOwnerStableId {
                owner: snapshot.owner,
                stable_id: snapshot.stable_id,
            });
        }
        if let Some(first_owner) = owners_by_stable_id.insert(snapshot.stable_id, snapshot.owner) {
            return Err(TransitionError::DuplicateOwnerStableId {
                stable_id: snapshot.stable_id,
                first_owner,
                duplicate_owner: snapshot.owner,
            });
        }
        stable_ids.insert(snapshot.owner, snapshot.stable_id);
    }
    Ok(stable_ids)
}

/// Validated, arena-independent property snapshot graph backing C1.
///
/// `NodeKey` values are opaque property identities here. This type contains no
/// `NodeArena` and offers no path that can reinterpret an identity as an arena
/// handle. Parent links are retained after validation because C2 clip rebasing
/// needs ancestry; surface policy and receiver grammar remain absent.
pub(crate) struct PropertySnapshotGraph {
    transforms: FxHashMap<TransformNodeId, Option<TransformNodeId>>,
    clips: FxHashMap<ClipNodeId, Option<ClipNodeId>>,
    effects: FxHashMap<EffectNodeId, Option<EffectNodeId>>,
    scrolls: FxHashMap<ScrollNodeId, Option<ScrollNodeId>>,
    layout_positions: FxHashMap<LayoutPositionNodeId, Option<LayoutPositionNodeId>>,
    visual_offsets: FxHashMap<VisualOffsetNodeId, Option<VisualOffsetNodeId>>,
}

impl PropertySnapshotGraph {
    pub(crate) fn try_from_artifact(artifact: &PaintArtifact) -> Result<Self, TransitionError> {
        SpatialProjectionGraph::try_new(
            &artifact.transform_nodes,
            &artifact.layout_position_nodes,
            &artifact.visual_offset_nodes,
            &artifact.scroll_nodes,
        )
        .map_err(TransitionError::SpatialSnapshot)?;

        let transforms = artifact
            .transform_nodes
            .iter()
            .map(|snapshot| (snapshot.id, snapshot.parent))
            .collect();
        let layout_positions = artifact
            .layout_position_nodes
            .iter()
            .map(|snapshot| {
                let parent = match snapshot.reference {
                    SpatialPositionReference::Viewport
                    | SpatialPositionReference::LayoutParent(None) => None,
                    SpatialPositionReference::LayoutParent(Some(parent))
                    | SpatialPositionReference::Anchor(parent) => {
                        Some(LayoutPositionNodeId(parent))
                    }
                };
                (snapshot.id, parent)
            })
            .collect();
        let visual_offsets = artifact
            .visual_offset_nodes
            .iter()
            .map(|snapshot| (snapshot.id, snapshot.parent))
            .collect();
        let scrolls = artifact
            .scroll_nodes
            .iter()
            .map(|snapshot| (snapshot.id, snapshot.parent))
            .collect();

        let mut clip_parents = FxHashMap::default();
        for snapshot in &artifact.clip_nodes {
            if clip_parents.insert(snapshot.id, snapshot.parent).is_some() {
                return Err(TransitionError::DuplicateClip(snapshot.id));
            }
            let canonical_role = matches!(
                (snapshot.id.role, snapshot.behavior),
                (ClipNodeRole::SelfClip, ClipBehavior::Replace)
                    | (ClipNodeRole::ContentsClip, ClipBehavior::Intersect)
            );
            if snapshot.id.owner != snapshot.owner
                || snapshot.owner.is_null()
                || snapshot.generation == 0
                || !canonical_role
            {
                return Err(TransitionError::InvalidClip(snapshot.id));
            }
        }
        validate_parent_store(
            &clip_parents,
            artifact.clip_nodes.iter().map(|snapshot| snapshot.id),
            TransitionError::MissingClip,
            TransitionError::CyclicClip,
        )?;

        let mut effect_parents = FxHashMap::default();
        for snapshot in &artifact.effect_nodes {
            if effect_parents
                .insert(snapshot.id, snapshot.parent)
                .is_some()
            {
                return Err(TransitionError::DuplicateEffect(snapshot.id));
            }
            if snapshot.id.0 != snapshot.owner
                || snapshot.owner.is_null()
                || snapshot.generation == 0
                || !snapshot.opacity.is_finite()
                || !(0.0..=1.0).contains(&snapshot.opacity)
            {
                return Err(TransitionError::InvalidEffect(snapshot.id));
            }
        }
        validate_parent_store(
            &effect_parents,
            artifact.effect_nodes.iter().map(|snapshot| snapshot.id),
            TransitionError::MissingEffect,
            TransitionError::CyclicEffect,
        )?;

        let graph = Self {
            transforms,
            clips: clip_parents,
            effects: effect_parents,
            scrolls,
            layout_positions,
            visual_offsets,
        };

        validate_owner_property_state_store(artifact)?;
        for snapshot in &artifact.owner_property_states {
            for (endpoint, state) in [
                (OwnerPropertyStateEndpoint::Paint, snapshot.paint),
                (
                    OwnerPropertyStateEndpoint::Descendants,
                    snapshot.descendants,
                ),
            ] {
                graph.validate_state_references(state).map_err(|reason| {
                    TransitionError::InvalidOwnerPropertyState {
                        owner: snapshot.owner,
                        endpoint,
                        reason,
                    }
                })?;
            }
        }

        Ok(graph)
    }

    pub(crate) fn validate_state(&self, state: PropertyTreeState) -> Result<(), TransitionError> {
        self.validate_state_references(state)
            .map_err(|reason| match reason {
                PropertyStateReferenceError::UnknownTransform(id) => {
                    TransitionError::UnknownTransformReference(id)
                }
                PropertyStateReferenceError::UnknownClip(id) => {
                    TransitionError::UnknownClipReference(id)
                }
                PropertyStateReferenceError::UnknownEffect(id) => {
                    TransitionError::UnknownEffectReference(id)
                }
                PropertyStateReferenceError::UnknownScroll(id) => {
                    TransitionError::UnknownScrollReference(id)
                }
                PropertyStateReferenceError::UnknownLayoutPosition(id) => {
                    TransitionError::UnknownLayoutPositionReference(id)
                }
                PropertyStateReferenceError::UnknownVisualOffset(id) => {
                    TransitionError::UnknownVisualOffsetReference(id)
                }
            })
    }

    fn validate_state_references(
        &self,
        state: PropertyTreeState,
    ) -> Result<(), PropertyStateReferenceError> {
        validate_reference(
            state.transform,
            &self.transforms,
            PropertyStateReferenceError::UnknownTransform,
        )?;
        validate_reference(
            state.clip,
            &self.clips,
            PropertyStateReferenceError::UnknownClip,
        )?;
        validate_reference(
            state.effect,
            &self.effects,
            PropertyStateReferenceError::UnknownEffect,
        )?;
        validate_reference(
            state.scroll,
            &self.scrolls,
            PropertyStateReferenceError::UnknownScroll,
        )?;
        validate_reference(
            state.layout_position,
            &self.layout_positions,
            PropertyStateReferenceError::UnknownLayoutPosition,
        )?;
        validate_reference(
            state.visual_offset,
            &self.visual_offsets,
            PropertyStateReferenceError::UnknownVisualOffset,
        )
    }

    pub(crate) fn clip_parent(
        &self,
        id: ClipNodeId,
    ) -> Result<Option<ClipNodeId>, TransitionError> {
        self.clips
            .get(&id)
            .copied()
            .ok_or(TransitionError::UnknownClipReference(id))
    }
}

fn validate_reference<Id: Copy + Eq + std::hash::Hash, Error>(
    id: Option<Id>,
    snapshots: &FxHashMap<Id, Option<Id>>,
    missing: impl FnOnce(Id) -> Error,
) -> Result<(), Error> {
    if let Some(id) = id
        && !snapshots.contains_key(&id)
    {
        return Err(missing(id));
    }
    Ok(())
}

fn validate_parent_store<Id: Copy + Eq + std::hash::Hash>(
    parents: &FxHashMap<Id, Option<Id>>,
    starts: impl IntoIterator<Item = Id>,
    missing: impl Fn(Id) -> TransitionError,
    cyclic: impl Fn(Id) -> TransitionError,
) -> Result<(), TransitionError> {
    let starts = starts.into_iter().collect::<Vec<_>>();
    for start in &starts {
        if let Some(parent) = parents.get(start).copied().flatten()
            && !parents.contains_key(&parent)
        {
            return Err(missing(parent));
        }
    }
    let mut complete = FxHashSet::default();
    for start in starts {
        if complete.contains(&start) {
            continue;
        }
        let mut seen = FxHashSet::default();
        let mut path = Vec::new();
        let mut cursor = Some(start);
        while let Some(id) = cursor {
            if complete.contains(&id) {
                break;
            }
            if !seen.insert(id) {
                return Err(cyclic(id));
            }
            path.push(id);
            cursor = parents.get(&id).copied().flatten();
        }
        complete.extend(path);
    }
    Ok(())
}

/// Pure six-dimensional C1 classifier.
///
/// Snapshot validation is intentionally separate from reconstruction. The
/// result records the exact identities on both sides and contains no surface
/// policy, grammar label, traversal order, or receiver-specific variant.
pub(crate) fn classify_property_transition(
    from: PropertyTreeState,
    to: PropertyTreeState,
    snapshots: &PropertySnapshotGraph,
) -> Result<PropertyStateTransition, TransitionError> {
    snapshots.validate_state(from)?;
    snapshots.validate_state(to)?;
    Ok(PropertyStateTransition::between(from, to))
}

/// Artifact-local traversal position: `chunk_index` indexes `PaintArtifact::chunks`
/// and `op_index` is that chunk's start in `PaintArtifact::ops`.
///
/// Neither field is an arena position, stable component id, or planner node
/// ordinal. C2 may rely on artifact store ordering only after this cursor list
/// has been validated against the artifact's complete chunk/op traversal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ArtifactCursor {
    chunk_index: usize,
    op_index: usize,
}

impl ArtifactCursor {
    pub(crate) fn chunk_index(self) -> usize {
        self.chunk_index
    }

    pub(crate) fn op_index(self) -> usize {
        self.op_index
    }
}

/// Validates the complete artifact traversal and returns one cursor per chunk.
pub(crate) fn artifact_cursors(
    artifact: &PaintArtifact,
) -> Result<Vec<ArtifactCursor>, TransitionError> {
    let mut expected_op = 0usize;
    let mut cursors = Vec::with_capacity(artifact.chunks.len());
    for (chunk_index, chunk) in artifact.chunks.iter().enumerate() {
        if chunk.id.owner != chunk.owner
            || chunk.op_range.start != expected_op
            || chunk.op_range.start > chunk.op_range.end
            || chunk.op_range.end > artifact.ops.len()
        {
            return Err(TransitionError::InvalidArtifactCursor(chunk_index));
        }
        cursors.push(ArtifactCursor {
            chunk_index,
            op_index: chunk.op_range.start,
        });
        expected_op = chunk.op_range.end;
    }
    if expected_op != artifact.ops.len() {
        return Err(TransitionError::NonTerminalArtifactCursor(expected_op));
    }
    Ok(cursors)
}

/// Validated artifact owner topology with a deterministic scene-root ordinal
/// for every owner reachable from a chunk.
pub(crate) struct ArtifactOwnerGraph {
    parents: FxHashMap<NodeKey, Option<NodeKey>>,
    stable_ids: FxHashMap<NodeKey, u64>,
    scene_root_ordinals: FxHashMap<NodeKey, u32>,
    first_chunk_indices: FxHashMap<NodeKey, usize>,
}

impl ArtifactOwnerGraph {
    pub(crate) fn try_from_artifact(artifact: &PaintArtifact) -> Result<Self, TransitionError> {
        let mut parents = FxHashMap::default();
        for snapshot in &artifact.owner_nodes {
            if snapshot.owner.is_null() {
                return Err(TransitionError::InvalidOwner(snapshot.owner));
            }
            if parents.insert(snapshot.owner, snapshot.parent).is_some() {
                return Err(TransitionError::DuplicateOwner(snapshot.owner));
            }
        }
        validate_parent_store(
            &parents,
            artifact.owner_nodes.iter().map(|snapshot| snapshot.owner),
            TransitionError::MissingOwnerParent,
            TransitionError::CyclicOwner,
        )?;
        let stable_ids = validate_owner_property_state_store(artifact)?;

        let mut root_ordinals = FxHashMap::default();
        for snapshot in &artifact.owner_nodes {
            if snapshot.parent.is_none() {
                let ordinal = u32::try_from(root_ordinals.len())
                    .map_err(|_| TransitionError::SceneRootOrdinalOverflow(snapshot.owner))?;
                root_ordinals.insert(snapshot.owner, ordinal);
            }
        }

        let mut scene_root_ordinals = FxHashMap::default();
        let mut referenced = FxHashSet::default();
        let mut first_chunk_indices = FxHashMap::default();
        for (chunk_index, chunk) in artifact.chunks.iter().enumerate() {
            let mut cursor = chunk.owner;
            loop {
                first_chunk_indices.entry(cursor).or_insert(chunk_index);
                if !referenced.insert(cursor) {
                    break;
                }
                // Parent closure and acyclicity were proven above. Only the
                // first chunk-owner lookup can be absent; every later lookup
                // is an already-validated parent edge.
                let parent = parents
                    .get(&cursor)
                    .copied()
                    .ok_or(TransitionError::UnknownTarget(cursor))?;
                let Some(parent) = parent else {
                    break;
                };
                cursor = parent;
            }
        }
        if let Some(snapshot) = artifact
            .owner_nodes
            .iter()
            .find(|snapshot| !referenced.contains(&snapshot.owner))
        {
            return Err(TransitionError::UnreferencedOwner(snapshot.owner));
        }

        for snapshot in &artifact.owner_nodes {
            let mut path = Vec::new();
            let mut cursor = snapshot.owner;
            let ordinal = loop {
                if let Some(ordinal) = scene_root_ordinals.get(&cursor).copied() {
                    break ordinal;
                }
                path.push(cursor);
                // These two branches are unreachable after parent-forest
                // validation and root collection. They remain typed checks
                // at the artifact boundary instead of becoming assumptions.
                let parent = parents
                    .get(&cursor)
                    .copied()
                    .ok_or(TransitionError::UnknownTarget(cursor))?;
                let Some(parent) = parent else {
                    break root_ordinals
                        .get(&cursor)
                        .copied()
                        .ok_or(TransitionError::UnknownTarget(cursor))?;
                };
                cursor = parent;
            };
            for owner in path {
                scene_root_ordinals.insert(owner, ordinal);
            }
        }

        Ok(Self {
            parents,
            stable_ids,
            scene_root_ordinals,
            first_chunk_indices,
        })
    }

    pub(crate) fn scene_target(
        &self,
        target: NodeKey,
    ) -> Result<ArtifactSceneTarget, TransitionError> {
        if !self.parents.contains_key(&target) {
            return Err(TransitionError::UnknownTarget(target));
        }
        Ok(ArtifactSceneTarget {
            scene_root_ordinal: self
                .scene_root_ordinals
                .get(&target)
                .copied()
                .ok_or(TransitionError::UnknownTarget(target))?,
            target,
        })
    }

    pub(crate) fn parent(&self, owner: NodeKey) -> Result<Option<NodeKey>, TransitionError> {
        self.parents
            .get(&owner)
            .copied()
            .ok_or(TransitionError::UnknownTarget(owner))
    }

    /// Persistent owner identity only. A retained surface key must also carry
    /// the surface role derived from its `SurfaceDagNodeKind`.
    pub(crate) fn stable_id(&self, owner: NodeKey) -> Result<u64, TransitionError> {
        self.stable_ids
            .get(&owner)
            .copied()
            .ok_or(TransitionError::UnknownTarget(owner))
    }

    pub(crate) fn cursor_for_target(
        &self,
        target: NodeKey,
        cursors: &[ArtifactCursor],
    ) -> Result<ArtifactCursor, TransitionError> {
        let chunk_index = self
            .first_chunk_indices
            .get(&target)
            .copied()
            // Unreachable after the closed owner-store check: every retained
            // owner is a chunk owner or one of its ancestors.
            .ok_or(TransitionError::UnknownTarget(target))?;
        cursors
            .get(chunk_index)
            .copied()
            .ok_or(TransitionError::InvalidArtifactCursor(chunk_index))
    }
}

/// Capability proving that `target` belongs to one validated artifact scene
/// root and carrying that root's artifact-order ordinal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ArtifactSceneTarget {
    scene_root_ordinal: u32,
    target: NodeKey,
}

impl ArtifactSceneTarget {
    pub(crate) fn scene_root_ordinal(self) -> u32 {
        self.scene_root_ordinal
    }

    pub(crate) fn target(self) -> NodeKey {
        self.target
    }
}

/// One ordered, grammar-free request to classify a property-state edge for a
/// generic artifact owner. Root ordinal and cursor are deliberately absent;
/// both are derived from the validated artifact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ArtifactTransitionRequest {
    target: NodeKey,
    from: PropertyTreeState,
    to: PropertyTreeState,
}

impl ArtifactTransitionRequest {
    pub(crate) fn new(target: NodeKey, from: PropertyTreeState, to: PropertyTreeState) -> Self {
        Self { target, from, to }
    }
}

/// Paint-sequenced envelope around the compositor-owned property delta.
///
/// `target` is always the generic `NodeKey` receiver identity. There is no
/// scroll-content or component grammar receiver variant for C2 to dispatch on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ClassifiedTransitionEvent {
    scene_root_ordinal: u32,
    cursor: ArtifactCursor,
    target: NodeKey,
    transition: PropertyStateTransition,
}

impl ClassifiedTransitionEvent {
    pub(crate) fn new(
        scene_target: ArtifactSceneTarget,
        cursor: ArtifactCursor,
        from: PropertyTreeState,
        to: PropertyTreeState,
        snapshots: &PropertySnapshotGraph,
    ) -> Result<Self, TransitionError> {
        Ok(Self {
            scene_root_ordinal: scene_target.scene_root_ordinal(),
            cursor,
            target: scene_target.target(),
            transition: classify_property_transition(from, to, snapshots)?,
        })
    }

    pub(crate) fn scene_root_ordinal(self) -> u32 {
        self.scene_root_ordinal
    }

    pub(crate) fn cursor(self) -> ArtifactCursor {
        self.cursor
    }

    pub(crate) fn target(self) -> NodeKey {
        self.target
    }

    pub(crate) fn transition(self) -> PropertyStateTransition {
        self.transition
    }
}

/// Classifies an ordered request stream against one complete artifact.
///
/// A request stream is legal when target first-subtree chunk indices are
/// non-decreasing; co-located targets may share one index. Legal request order
/// is preserved exactly. Each event's scene root and cursor are independently
/// derived from the artifact owner forest and chunk traversal; no planner
/// ordinal or receiver grammar enters the production result.
pub(crate) fn classify_artifact_transition_sequence(
    artifact: &PaintArtifact,
    requests: &[ArtifactTransitionRequest],
) -> Result<Vec<ClassifiedTransitionEvent>, TransitionError> {
    let snapshots = PropertySnapshotGraph::try_from_artifact(artifact)?;
    let owners = ArtifactOwnerGraph::try_from_artifact(artifact)?;
    let cursors = artifact_cursors(artifact)?;
    let mut events = Vec::with_capacity(requests.len());
    let mut previous = None;
    for request in requests {
        let cursor = owners.cursor_for_target(request.target, &cursors)?;
        if let Some(previous) = previous
            && cursor.chunk_index < previous
        {
            return Err(TransitionError::OutOfOrderArtifactCursor {
                previous_chunk: previous,
                current_chunk: cursor.chunk_index,
            });
        }
        events.push(ClassifiedTransitionEvent::new(
            owners.scene_target(request.target)?,
            cursor,
            request.from,
            request.to,
            &snapshots,
        )?);
        previous = Some(cursor.chunk_index);
    }
    Ok(events)
}

#[cfg(test)]
mod tests;
