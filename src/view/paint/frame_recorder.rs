#[cfg(test)]
use super::PaintChunk;
pub(super) mod property_closure_cache;
use std::collections::hash_map::Entry;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::view::compositor::property_tree::{
    ClipNodeSnapshot, EffectNodeId, LayoutPositionNodeId, LayoutPositionNodeSnapshot,
    PropertyTreeState, ScrollNodeId, ScrollNodeSnapshot, SpatialPositionReference, TransformNodeId,
    TransformNodeSnapshot, VisualOffsetNodeId, VisualOffsetNodeSnapshot,
};
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::node_arena::{NodeArena, NodeKey};

use super::{CoverageRecordingMode, PaintArtifact, PaintCoverageItem, PaintRecordingContext};

/// Records the frame: what every node draws, with every property snapshot
/// the compiler needs. Each node in `left_out` is skipped with its subtree,
/// and so is each node found unable to paint, which is added to `left_out`;
/// the rest of the frame always records. `cache` reuses what earlier frames
/// and passes recorded.
pub(crate) fn record_frame_artifact(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    mut cache: Option<&mut super::RecordingCache>,
    left_out: &mut FxHashSet<NodeKey>,
) -> PaintArtifact {
    loop {
        let recorded = match cache.as_deref_mut() {
            Some(cache) => record_with_cache(
                arena,
                roots,
                property_trees,
                paint_generations,
                cache,
                left_out,
            ),
            None => record_without_cache(arena, roots, property_trees, paint_generations, left_out),
        };
        let incomplete = match recorded {
            Ok(artifact) => return artifact,
            Err(incomplete) => incomplete,
        };
        // Record again without the owners whose chains are incomplete. A
        // left-out node never records again, so each pass leaves out more;
        // with the roots left out nothing records.
        let before = left_out.len();
        left_out.extend(incomplete);
        if left_out.len() == before {
            left_out.extend(roots.iter().copied());
        }
    }
}

/// [`record_frame_artifact`] with nothing left out beforehand.
#[cfg(test)]
pub(crate) fn record_surface_dag_frame_artifact(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
) -> PaintArtifact {
    record_frame_artifact(
        arena,
        roots,
        property_trees,
        paint_generations,
        None,
        &mut FxHashSet::default(),
    )
}

/// [`record_surface_dag_frame_artifact`] reusing what `cache` recorded in
/// earlier frames.
#[cfg(test)]
pub(crate) fn record_surface_dag_frame_artifact_cached(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    cache: &mut super::RecordingCache,
) -> PaintArtifact {
    record_frame_artifact(
        arena,
        roots,
        property_trees,
        paint_generations,
        Some(cache),
        &mut FxHashSet::default(),
    )
}

/// One recording pass. The error names the owners whose chains the property
/// trees miss.
fn record_without_cache(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    left_out: &mut FxHashSet<NodeKey>,
) -> Result<PaintArtifact, FxHashSet<NodeKey>> {
    let _profile = crate::view::paint::work_profile::scope("record_surface_dag_frame_artifact");
    let manifest = record_full_manifest(
        arena,
        roots,
        property_trees,
        paint_generations,
        None,
        left_out,
    );
    let mut artifact = materialize_frame_artifact_with_cache(manifest, None);
    populate_referenced_property_snapshots(&mut artifact, property_trees)?;
    Ok(artifact)
}

/// [`record_without_cache`] reusing what `cache` recorded in earlier frames
/// and passes.
fn record_with_cache(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    cache: &mut super::RecordingCache,
    left_out: &mut FxHashSet<NodeKey>,
) -> Result<PaintArtifact, FxHashSet<NodeKey>> {
    cache.begin();
    // A metadata pass finds the owners whose commands the cache still holds;
    // when it holds them all, their recorded commands complete the frame.
    let mut preflight = super::coverage_manifest::record_cached_coverage_manifest(
        arena,
        roots,
        true,
        CoverageRecordingMode::MetadataOnly,
        property_trees,
        paint_generations,
        recording_context(),
        Some(&mut *cache),
        left_out,
    );
    let manifest = if cache.materialize(arena, &mut preflight).is_some() {
        preflight
    } else {
        record_full_manifest(
            arena,
            roots,
            property_trees,
            paint_generations,
            Some(&mut *cache),
            left_out,
        )
    };
    let mut artifact = materialize_frame_artifact_with_cache(manifest, Some(&mut *cache));
    let populated = property_closure_cache::populate(&mut artifact, property_trees, cache);
    // A pass that has to leave an owner out still recorded valid inputs; the
    // cache keeps them for the next pass.
    cache.finish(true);
    populated.map(|()| artifact)
}

fn recording_context() -> PaintRecordingContext {
    PaintRecordingContext::default()
}

fn record_full_manifest(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    cache: Option<&mut super::RecordingCache>,
    left_out: &mut FxHashSet<NodeKey>,
) -> super::PaintCoverageManifest {
    super::coverage_manifest::record_cached_coverage_manifest(
        arena,
        roots,
        true,
        CoverageRecordingMode::FullArtifact,
        property_trees,
        paint_generations,
        recording_context(),
        cache,
        left_out,
    )
}

/// Adds `snapshot` under `key` unless the store already holds that node.
fn insert_snapshot<K, V>(store: &mut FxHashMap<K, V>, key: K, snapshot: V) -> bool
where
    K: Copy + Eq + std::hash::Hash,
{
    match store.entry(key) {
        Entry::Vacant(entry) => {
            entry.insert(snapshot);
            true
        }
        Entry::Occupied(_) => false,
    }
}

/// On success the artifact owns every transitive property snapshot needed by
/// its chunk states and recorded owner endpoints, and no later stage needs the
/// arena. Otherwise the error names each owner whose chains the property
/// trees miss.
pub(super) fn populate_referenced_property_snapshots(
    artifact: &mut PaintArtifact,
    property_trees: &PropertyTrees,
) -> Result<(), FxHashSet<NodeKey>> {
    let _profile =
        crate::view::paint::work_profile::scope("populate_referenced_property_snapshots");
    artifact.transform_nodes.clear();
    artifact.layout_position_nodes.clear();
    artifact.visual_offset_nodes.clear();
    artifact.scroll_nodes.clear();

    let mut referenced_states = artifact
        .chunks
        .iter()
        .map(|chunk| (chunk.owner, chunk.properties))
        .chain(artifact.owner_property_states.iter().flat_map(|snapshot| {
            [
                (snapshot.owner, snapshot.paint),
                (snapshot.owner, snapshot.descendants),
            ]
        }))
        .collect::<Vec<_>>();
    // The compiler places an owner-local clip at its owner's layout frame, so
    // that owner's spatial chains belong to the closure even when no state
    // references them. Clips found while walking are appended the same way.
    let clip_frame = |clip: &ClipNodeSnapshot| {
        clip.geometry.is_owner_local().then(|| {
            (
                clip.owner,
                PropertyTreeState {
                    layout_position: Some(LayoutPositionNodeId(clip.owner)),
                    visual_offset: Some(VisualOffsetNodeId(clip.owner)),
                    ..PropertyTreeState::default()
                },
            )
        })
    };
    referenced_states.extend(artifact.clip_nodes.iter().filter_map(clip_frame));

    let mut clips = FxHashMap::default();
    for snapshot in artifact.clip_nodes.iter().copied() {
        insert_snapshot(&mut clips, snapshot.id, snapshot);
    }
    let mut effects = FxHashMap::default();
    for snapshot in artifact.effect_nodes.iter().copied() {
        insert_snapshot(&mut effects, snapshot.id, snapshot);
    }
    // The property trees are immutable for this call. After one complete
    // chain has been checked against the artifact's supplied observations,
    // another reference to the same leaf cannot introduce a new snapshot.
    // Do not seed these sets from the artifact: supplied data still needs the
    // first live-tree comparison, including ancestors and bounded termination.
    let mut checked_clip_leaves = FxHashSet::default();
    let mut checked_effect_leaves = FxHashSet::default();
    let mut transforms = FxHashMap::<TransformNodeId, TransformNodeSnapshot>::default();
    let mut positions = FxHashMap::<LayoutPositionNodeId, LayoutPositionNodeSnapshot>::default();
    let mut visuals = FxHashMap::<VisualOffsetNodeId, VisualOffsetNodeSnapshot>::default();
    let mut scrolls = FxHashMap::<ScrollNodeId, ScrollNodeSnapshot>::default();
    let mut transform_scratch = snapshot_closure::ChainScratch::default();
    let mut position_scratch = snapshot_closure::ChainScratch::default();
    let mut visual_scratch = snapshot_closure::ChainScratch::default();
    let mut scroll_scratch = snapshot_closure::ChainScratch::default();
    let mut anchor_visual_roots = Vec::new();
    let mut incomplete = FxHashSet::default();
    let mut next_state = 0;
    while let Some(&(owner, state)) = referenced_states.get(next_state) {
        next_state += 1;
        // A leaf counts as checked only once its whole chain was found, so
        // every owner under a missing chain is reported.
        let complete = 'state: {
            if !checked_clip_leaves.contains(&state.clip) {
                let Some(chain) = property_trees.clip_snapshot_for(state.clip) else {
                    break 'state false;
                };
                checked_clip_leaves.insert(state.clip);
                for snapshot in chain {
                    if insert_snapshot(&mut clips, snapshot.id, snapshot) {
                        artifact.clip_nodes.push(snapshot);
                        referenced_states.extend(clip_frame(&snapshot));
                    }
                }
            }
            if !checked_effect_leaves.contains(&state.effect) {
                let Some(chain) = property_trees.effect_snapshot_for(state.effect) else {
                    break 'state false;
                };
                checked_effect_leaves.insert(state.effect);
                for snapshot in chain {
                    if insert_snapshot(&mut effects, snapshot.id, snapshot) {
                        artifact.effect_nodes.push(snapshot);
                    }
                }
            }
            anchor_visual_roots.clear();
            let Some(chain) = transform_scratch.unseen_chain(
                state.transform,
                &transforms,
                |id| property_trees.transform_snapshot_for(id),
                |s| s.parent,
            ) else {
                break 'state false;
            };
            for snapshot in chain {
                if insert_snapshot(&mut transforms, snapshot.id, snapshot) {
                    artifact.transform_nodes.push(snapshot);
                }
            }
            let Some(chain) = position_scratch.unseen_chain(
                state.layout_position,
                &positions,
                |id| property_trees.layout_position_snapshot_for(id),
                |s| match s.reference {
                    SpatialPositionReference::Viewport
                    | SpatialPositionReference::LayoutParent(None) => None,
                    SpatialPositionReference::LayoutParent(Some(parent))
                    | SpatialPositionReference::Anchor(parent) => {
                        Some(LayoutPositionNodeId(parent))
                    }
                },
            ) else {
                break 'state false;
            };
            for snapshot in chain {
                if let SpatialPositionReference::Anchor(anchor) = snapshot.reference {
                    anchor_visual_roots.push(VisualOffsetNodeId(anchor));
                }
                if let Some(scroll) = snapshot.reference_scroll {
                    let Some(scroll_chain) = scroll_scratch.unseen_chain(
                        Some(scroll),
                        &scrolls,
                        |id| property_trees.scroll_snapshot_for(id),
                        |s| s.parent,
                    ) else {
                        break 'state false;
                    };
                    for scroll_snapshot in scroll_chain {
                        if insert_snapshot(&mut scrolls, scroll_snapshot.id, scroll_snapshot) {
                            artifact.scroll_nodes.push(scroll_snapshot);
                        }
                    }
                }
                if insert_snapshot(&mut positions, snapshot.id, snapshot) {
                    artifact.layout_position_nodes.push(snapshot);
                }
            }
            let Some(chain) = visual_scratch.unseen_chain(
                state.visual_offset,
                &visuals,
                |id| property_trees.visual_offset_snapshot_for(id),
                |s| s.parent,
            ) else {
                break 'state false;
            };
            for snapshot in chain {
                if insert_snapshot(&mut visuals, snapshot.id, snapshot) {
                    artifact.visual_offset_nodes.push(snapshot);
                }
            }
            for anchor in anchor_visual_roots.drain(..) {
                let Some(chain) = visual_scratch.unseen_chain(
                    Some(anchor),
                    &visuals,
                    |id| property_trees.visual_offset_snapshot_for(id),
                    |s| s.parent,
                ) else {
                    break 'state false;
                };
                for snapshot in chain {
                    if insert_snapshot(&mut visuals, snapshot.id, snapshot) {
                        artifact.visual_offset_nodes.push(snapshot);
                    }
                }
            }
            let Some(chain) = scroll_scratch.unseen_chain(
                state.scroll,
                &scrolls,
                |id| property_trees.scroll_snapshot_for(id),
                |s| s.parent,
            ) else {
                break 'state false;
            };
            for snapshot in chain {
                if insert_snapshot(&mut scrolls, snapshot.id, snapshot) {
                    artifact.scroll_nodes.push(snapshot);
                }
            }
            true
        };
        if !complete {
            incomplete.insert(owner);
        }
    }
    if incomplete.is_empty() {
        Ok(())
    } else {
        Err(incomplete)
    }
}

#[cfg(test)]
pub(crate) fn populate_referenced_property_snapshots_for_test(
    artifact: &mut PaintArtifact,
    property_trees: &PropertyTrees,
) {
    let result = populate_referenced_property_snapshots(artifact, property_trees);
    assert!(result.is_ok(), "incomplete property chains: {result:?}");
}

fn materialize_frame_artifact_with_cache(
    mut manifest: super::PaintCoverageManifest,
    mut cache: Option<&mut super::RecordingCache>,
) -> PaintArtifact {
    let _profile = crate::view::paint::work_profile::scope("materialize_frame_artifact");
    // The manifest length is a safe capacity hint. Commands stay in their
    // existing immutable blocks, so no flat payload allocation/count pass is needed.
    let chunk_count = manifest.items.len();
    let mut artifact = PaintArtifact {
        chunks: super::shared_sequence::SharedSequence::with_shared_capacity(chunk_count),
        ops: super::shared_sequence::SharedSequence::with_shared_capacity(chunk_count),
        ..PaintArtifact::default()
    };
    let replayed_store = cache
        .as_deref_mut()
        .is_some_and(|cache| cache.replay_scope_store(&manifest, &mut artifact));
    if let Some(cache) = cache.as_deref_mut() {
        cache.assemble_artifact_blocks(&mut manifest, &mut artifact);
    } else {
        super::RecordingCache::default().assemble_artifact_blocks(&mut manifest, &mut artifact);
    }
    if replayed_store {
        return artifact;
    }
    let pending_key = (!replayed_store && cache.is_some())
        .then(|| super::RecordingCache::scope_store_key(&manifest));
    let mut seen_clip_nodes = FxHashMap::default();
    let mut seen_effect_nodes = FxHashMap::default();
    let mut seen_owner_nodes = FxHashMap::default();
    let mut seen_owner_property_states = FxHashMap::default();
    let mut merged_scopes = rustc_hash::FxHashSet::default();
    let mut merged_clip_chains = rustc_hash::FxHashSet::default();
    let mut merged_effect_chains = rustc_hash::FxHashSet::default();
    for item in &manifest.items {
        let PaintCoverageItem::ArtifactChunk {
            clip_snapshot,
            effect_snapshot,
            owner_scope,
            ops: Some(_),
            ..
        } = item
        else {
            continue;
        };
        let mut scopes = Vec::new();
        let mut cursor = Some(owner_scope);
        while let Some(scope) = cursor {
            // Pointer identity is used only for the same immutable allocation
            // in this manifest, never as a cross-frame content identity. A
            // separately allocated observation of the same owner merges by id.
            if !merged_scopes.insert(std::sync::Arc::as_ptr(scope)) {
                break;
            }
            scopes.push(scope.as_ref());
            cursor = scope.parent.as_ref();
        }
        for snapshot in std::iter::once(clip_snapshot)
            .chain(scopes.iter().flat_map(|scope| &scope.clips))
            .filter(|chain| merged_clip_chains.insert(std::sync::Arc::as_ptr(chain)))
            .flat_map(|chain| chain.iter().copied())
        {
            if insert_snapshot(&mut seen_clip_nodes, snapshot.id, snapshot) {
                artifact.clip_nodes.push(snapshot);
            }
        }
        for snapshot in std::iter::once(effect_snapshot)
            .chain(scopes.iter().flat_map(|scope| &scope.effects))
            .filter(|chain| merged_effect_chains.insert(std::sync::Arc::as_ptr(chain)))
            .flat_map(|chain| chain.iter().copied())
        {
            if insert_snapshot(&mut seen_effect_nodes, snapshot.id, snapshot) {
                artifact.effect_nodes.push(snapshot);
            }
        }
        for snapshot in scopes.iter().map(|scope| scope.topology) {
            if insert_snapshot(&mut seen_owner_nodes, snapshot.owner, snapshot) {
                artifact.owner_nodes.push(snapshot);
            }
        }
        for snapshot in scopes.iter().map(|scope| scope.state) {
            if insert_snapshot(&mut seen_owner_property_states, snapshot.owner, snapshot) {
                artifact.owner_property_states.push(snapshot);
            }
        }
    }
    if let (Some(cache), Some(key)) = (cache, pending_key) {
        cache.remember_scope_store(key, &artifact);
    }
    artifact
}

#[cfg(test)]
mod snapshot_merge_tests;

mod snapshot_closure;

#[cfg(test)]
mod recording_test_support;
#[cfg(test)]
pub(crate) use recording_test_support::canonical_manifest_matches;
