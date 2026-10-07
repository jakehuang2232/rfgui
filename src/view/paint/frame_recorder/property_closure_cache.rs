use super::*;
use crate::view::compositor::property_tree::{
    ClipNodeId, ClipNodeSnapshot, EffectNodeSnapshot, PropertyTreeState,
};

struct Inputs {
    seeds: Vec<(NodeKey, PropertyTreeState)>,
    clips: Vec<ClipNodeId>,
    effects: Vec<EffectNodeId>,
}
fn seeds(artifact: &PaintArtifact) -> impl Iterator<Item = (NodeKey, PropertyTreeState)> + '_ {
    artifact
        .chunks
        .iter()
        .map(|chunk| (chunk.owner, chunk.properties))
        .chain(
            artifact
                .owner_property_states
                .iter()
                .flat_map(|state| [(state.owner, state.paint), (state.owner, state.descendants)]),
        )
}
impl Inputs {
    fn new(artifact: &PaintArtifact) -> Self {
        Self {
            seeds: seeds(artifact).collect(),
            clips: artifact.clip_nodes.iter().map(|node| node.id).collect(),
            effects: artifact.effect_nodes.iter().map(|node| node.id).collect(),
        }
    }
    fn matches(&self, artifact: &PaintArtifact) -> bool {
        self.seeds.iter().copied().eq(seeds(artifact))
            && self
                .clips
                .iter()
                .copied()
                .eq(artifact.clip_nodes.iter().map(|n| n.id))
            && self
                .effects
                .iter()
                .copied()
                .eq(artifact.effect_nodes.iter().map(|n| n.id))
    }
}

/// Retain closure topology/order and exact values whose stores were not written.
/// Changed or unproven stores are read again, including invalid generations and
/// nonfinite values that the downstream planner must reject. Dirty flags are
/// never an absence-of-write proof.
pub(in super::super) struct PropertyClosureCache {
    stamp: crate::view::compositor::property_tree::PropertyStoreStamp,
    inputs: Inputs,
    clips: Vec<ClipNodeSnapshot>,
    effects: Vec<EffectNodeSnapshot>,
    transforms: Vec<TransformNodeSnapshot>,
    positions: Vec<LayoutPositionNodeSnapshot>,
    visuals: Vec<VisualOffsetNodeSnapshot>,
    scrolls: Vec<ScrollNodeSnapshot>,
    supplied_clip_indices: Vec<usize>,
    supplied_effect_indices: Vec<usize>,
}
fn refresh<S: Copy>(
    old: &[S],
    writes: Option<&rustc_hash::FxHashSet<NodeKey>>,
    owner: impl Fn(&S) -> NodeKey,
    mut fetch: impl FnMut(&S) -> Option<S>,
    same_edges: impl Fn(&S, &S) -> bool,
) -> Option<Vec<S>> {
    old.iter()
        .map(|before| {
            if writes.is_some_and(|keys| !keys.contains(&owner(before))) {
                return Some(*before);
            }
            let now = fetch(before)?;
            same_edges(before, &now).then_some(now)
        })
        .collect()
}
impl PropertyClosureCache {
    fn replay(&mut self, artifact: &mut PaintArtifact, trees: &PropertyTrees) -> Option<()> {
        if !self.inputs.matches(artifact) {
            return None;
        }
        let writes = trees.property_writes_since(&self.stamp);
        // An owner-local clip adds its owner's spatial chains to the closure.
        let clips = refresh(
            &self.clips,
            writes.as_deref(),
            |n| n.owner,
            |n| trees.clip_node_snapshot_for(n.id),
            |a, b| {
                a.id == b.id
                    && a.owner == b.owner
                    && a.parent == b.parent
                    && a.geometry.is_owner_local() == b.geometry.is_owner_local()
            },
        )?;
        let effects = refresh(
            &self.effects,
            writes.as_deref(),
            |n| n.owner,
            |n| trees.effect_node_snapshot_for(n.id),
            |a, b| a.id == b.id && a.owner == b.owner && a.parent == b.parent,
        )?;
        // Supplied observations still have to equal this frame's live values;
        // old topology cannot hide a conflicting component-provided snapshot.
        if !self
            .supplied_clip_indices
            .iter()
            .zip(&artifact.clip_nodes)
            .all(|(&index, supplied)| clips.get(index) == Some(supplied))
            || !self
                .supplied_effect_indices
                .iter()
                .zip(&artifact.effect_nodes)
                .all(|(&index, supplied)| effects.get(index) == Some(supplied))
        {
            return None;
        }
        let transforms = refresh(
            &self.transforms,
            writes.as_deref(),
            |n| n.owner,
            |n| trees.transform_snapshot_for(n.id),
            |a, b| a.id == b.id && a.owner == b.owner && a.parent == b.parent,
        )?;
        let positions = refresh(
            &self.positions,
            writes.as_deref(),
            |n| n.owner,
            |n| trees.layout_position_snapshot_for(n.id),
            |a, b| {
                a.id == b.id
                    && a.owner == b.owner
                    && a.reference == b.reference
                    && a.reference_scroll == b.reference_scroll
            },
        )?;
        let visuals = refresh(
            &self.visuals,
            writes.as_deref(),
            |n| n.owner,
            |n| trees.visual_offset_snapshot_for(n.id),
            |a, b| a.id == b.id && a.owner == b.owner && a.parent == b.parent,
        )?;
        let scrolls = refresh(
            &self.scrolls,
            writes.as_deref(),
            |n| n.owner,
            |n| trees.scroll_snapshot_for(n.id),
            |a, b| a.id == b.id && a.owner == b.owner && a.parent == b.parent,
        )?;
        // The same seeds plus every same parent/reference edge imply the same
        // complete closure and bounded termination proven on the miss path.
        // Publish only after all fetches/comparisons succeed; misses retain the
        // original traversal and its exact owner-attributed rejection behavior.
        self.clips.clone_from(&clips);
        self.effects.clone_from(&effects);
        self.transforms.clone_from(&transforms);
        self.positions.clone_from(&positions);
        self.visuals.clone_from(&visuals);
        self.scrolls.clone_from(&scrolls);
        self.stamp = trees.property_store_stamp();
        artifact.clip_nodes = clips;
        artifact.effect_nodes = effects;
        artifact.transform_nodes = transforms;
        artifact.layout_position_nodes = positions;
        artifact.visual_offset_nodes = visuals;
        artifact.scroll_nodes = scrolls;
        Some(())
    }
}

pub(super) fn populate(
    artifact: &mut PaintArtifact,
    trees: &PropertyTrees,
    cache: &mut crate::view::paint::RecordingCache,
) -> Result<(), Vec<FrameArtifactFallbackReason>> {
    if cache
        .property_closure
        .as_mut()
        .and_then(|entry| entry.replay(artifact, trees))
        .is_some()
    {
        cache.property_closure_hits += 1;
        return Ok(());
    }
    let inputs = Inputs::new(artifact);
    populate_referenced_property_snapshots(artifact, trees)?;
    let supplied_clip_indices = inputs
        .clips
        .iter()
        .map(|id| {
            artifact
                .clip_nodes
                .iter()
                .position(|n| n.id == *id)
                .expect("successful closure keeps supplied clips")
        })
        .collect();
    let supplied_effect_indices = inputs
        .effects
        .iter()
        .map(|id| {
            artifact
                .effect_nodes
                .iter()
                .position(|n| n.id == *id)
                .expect("successful closure keeps supplied effects")
        })
        .collect();
    cache.property_closure = Some(PropertyClosureCache {
        stamp: trees.property_store_stamp(),
        inputs,
        supplied_clip_indices,
        supplied_effect_indices,
        clips: artifact.clip_nodes.clone(),
        effects: artifact.effect_nodes.clone(),
        transforms: artifact.transform_nodes.clone(),
        positions: artifact.layout_position_nodes.clone(),
        visuals: artifact.visual_offset_nodes.clone(),
        scrolls: artifact.scroll_nodes.clone(),
    });
    Ok(())
}
