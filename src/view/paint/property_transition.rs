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
    DuplicateOwner(NodeKey),
    InvalidOwner(NodeKey),
    MissingOwnerParent(NodeKey),
    CyclicOwner(NodeKey),
    UnknownTarget(NodeKey),
    UnreferencedOwner(NodeKey),
    SceneRootOrdinalOverflow(NodeKey),
    InvalidArtifactCursor(usize),
    NonTerminalArtifactCursor(usize),
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

        Ok(Self {
            transforms,
            clips: clip_parents,
            effects: effect_parents,
            scrolls,
            layout_positions,
            visual_offsets,
        })
    }

    fn validate_state(&self, state: PropertyTreeState) -> Result<(), TransitionError> {
        validate_reference(
            state.transform,
            &self.transforms,
            TransitionError::UnknownTransformReference,
        )?;
        validate_reference(
            state.clip,
            &self.clips,
            TransitionError::UnknownClipReference,
        )?;
        validate_reference(
            state.effect,
            &self.effects,
            TransitionError::UnknownEffectReference,
        )?;
        validate_reference(
            state.scroll,
            &self.scrolls,
            TransitionError::UnknownScrollReference,
        )?;
        validate_reference(
            state.layout_position,
            &self.layout_positions,
            TransitionError::UnknownLayoutPositionReference,
        )?;
        validate_reference(
            state.visual_offset,
            &self.visual_offsets,
            TransitionError::UnknownVisualOffsetReference,
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

fn validate_reference<Id: Copy + Eq + std::hash::Hash>(
    id: Option<Id>,
    snapshots: &FxHashMap<Id, Option<Id>>,
    missing: impl FnOnce(Id) -> TransitionError,
) -> Result<(), TransitionError> {
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
    scene_root_ordinals: FxHashMap<NodeKey, u32>,
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
        for chunk in &artifact.chunks {
            let mut cursor = chunk.owner;
            loop {
                let parent = parents
                    .get(&cursor)
                    .copied()
                    .ok_or(TransitionError::UnknownTarget(cursor))?;
                referenced.insert(cursor);
                let Some(parent) = parent else {
                    let ordinal = root_ordinals
                        .get(&cursor)
                        .copied()
                        .ok_or(TransitionError::UnknownTarget(cursor))?;
                    scene_root_ordinals.insert(chunk.owner, ordinal);
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
            if scene_root_ordinals.contains_key(&snapshot.owner) {
                continue;
            }
            let mut cursor = snapshot.owner;
            loop {
                let parent = parents
                    .get(&cursor)
                    .copied()
                    .ok_or(TransitionError::UnknownTarget(cursor))?;
                let Some(parent) = parent else {
                    let ordinal = root_ordinals
                        .get(&cursor)
                        .copied()
                        .ok_or(TransitionError::UnknownTarget(cursor))?;
                    scene_root_ordinals.insert(snapshot.owner, ordinal);
                    break;
                };
                cursor = parent;
            }
        }

        Ok(Self {
            parents,
            scene_root_ordinals,
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
}

/// Capability proving that `target` belongs to one validated artifact scene
/// root and carrying that root's artifact-order ordinal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ArtifactSceneTarget {
    scene_root_ordinal: u32,
    target: NodeKey,
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
            scene_root_ordinal: scene_target.scene_root_ordinal,
            cursor,
            target: scene_target.target,
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

#[cfg(test)]
mod tests;
