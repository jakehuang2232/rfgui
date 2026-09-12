use std::collections::hash_map::Entry;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::view::compositor::property_tree::{
    EffectNodeId, LayoutPositionNodeId, LayoutPositionNodeSnapshot, ScrollNodeId,
    ScrollNodeSnapshot, SpatialPositionReference, TransformNodeId, TransformNodeSnapshot,
    VisualOffsetNodeId, VisualOffsetNodeSnapshot,
};
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::node_arena::{NodeArena, NodeKey};

use super::coverage_manifest::exact_deferred_viewport_self_clip_witness;

use super::{
    CoverageRecordingMode, LegacyPaintReason, PaintArtifact, PaintArtifactTarget, PaintChunk,
    PaintCoverageItem, PaintCoverageValidationError, PaintOpacityAuthority, PaintRecordingContext,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RendererMode {
    Legacy,
    Auto,
    StrictPlan,
    #[cfg(test)]
    ForcedForTests,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FrameArtifactFallbackReason {
    RendererLegacy,
    LegacyBoundary(LegacyPaintReason),
    /// M6A production authority accepts only chunks whose property-tree
    /// identity is completely neutral. Later milestones will make each
    /// property family authoritative one at a time.
    PropertyBoundary(NodeKey),
    RootCount(usize),
    MissingRootEffect(NodeKey),
    InvalidRootEffect(NodeKey),
    NestedEffect(NodeKey),
    NonEffectProperty(NodeKey),
    DeferredBoundary(NodeKey),
    Validation(PaintCoverageValidationError),
}

/// B1 typed compiler bridge. The validated pair is consumed in one step and
/// only an opaque fixed H/content/O plan authority can escape.
pub(crate) fn record_frame_artifact(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    mode: RendererMode,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    record_frame_artifact_with_policy(
        arena,
        roots,
        property_trees,
        paint_generations,
        mode,
        FrameArtifactAuthorityPolicy::ExistingBakedProperties,
        None,
        None,
    )
}

/// M6A production entry point. This is intentionally stricter than the
/// compatibility recorder above: the whole frame must be deferred-free and
/// property-neutral before full artifact hooks run.
pub(crate) fn record_property_neutral_frame_artifact(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    mode: RendererMode,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    record_frame_artifact_with_policy(
        arena,
        roots,
        property_trees,
        paint_generations,
        mode,
        FrameArtifactAuthorityPolicy::PropertyNeutral,
        None,
        None,
    )
}

/// Production baked-opacity authority that admits validated property-tree
/// clips while keeping every other property family on legacy.
pub(crate) fn record_clip_enabled_frame_artifact(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    mode: RendererMode,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    record_frame_artifact_with_policy(
        arena,
        roots,
        property_trees,
        paint_generations,
        mode,
        FrameArtifactAuthorityPolicy::ClipEnabled,
        None,
        None,
    )
}

/// C3a current-target producer entry point. It is the only pre-cutover
/// production path allowed to close the artifact's transitive spatial
/// snapshots; existing retained and ArtifactCanary recorders deliberately
/// keep their frozen stores unchanged.
pub(crate) fn record_closed_single_target_frame_artifact(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    mode: RendererMode,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    let outcome =
        record_clip_enabled_frame_artifact(arena, roots, property_trees, paint_generations, mode)?;
    close_recorded_artifact_property_snapshots(outcome, property_trees, mode)
}

/// Generic current-target Surface DAG producer.
///
/// This policy carries no exact-shape witness, closes the same transitive
/// artifact snapshot store as the zero-surface C3a producer, and already has
/// a production caller for transform/effect surfaces. Scroll contracts are
/// admitted here, but remain unwired in the production scroll selector until
/// its pixel/reuse cutover lands.
pub(crate) fn record_surface_dag_frame_artifact(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    mode: RendererMode,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    let _profile = crate::view::paint::work_profile::scope("record_surface_dag_frame_artifact");
    let outcome = record_frame_artifact_with_policy(
        arena,
        roots,
        property_trees,
        paint_generations,
        mode,
        FrameArtifactAuthorityPolicy::SurfaceDag,
        None,
        None,
    )?;
    close_recorded_artifact_property_snapshots(outcome, property_trees, mode)
}

pub(crate) fn record_surface_dag_frame_artifact_cached(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    cache: &mut super::RecordingCache,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    cache.begin();
    let outcome = record_frame_artifact_with_policy_and_stack(
        arena,
        roots,
        property_trees,
        paint_generations,
        RendererMode::Auto,
        FrameArtifactAuthorityPolicy::SurfaceDag,
        None,
        None,
        None,
        None,
        Some(cache),
    )
    .and_then(|outcome| {
        close_recorded_artifact_property_snapshots(outcome, property_trees, RendererMode::Auto)
    });
    cache.finish(matches!(
        &outcome,
        Ok(FrameArtifactRecordOutcome::Artifact { .. })
    ));
    outcome
}

fn close_recorded_artifact_property_snapshots(
    outcome: FrameArtifactRecordOutcome,
    property_trees: &PropertyTrees,
    mode: RendererMode,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    let FrameArtifactRecordOutcome::Artifact {
        mut artifact,
        mut eligibility,
    } = outcome
    else {
        return Ok(outcome);
    };
    if let Err(reasons) = populate_referenced_property_snapshots(&mut artifact, property_trees) {
        eligibility.eligible = false;
        for reason in reasons {
            if !eligibility.reasons.contains(&reason) {
                eligibility.reasons.push(reason);
            }
        }
        return fallback_or_forced(mode, eligibility);
    }
    Ok(FrameArtifactRecordOutcome::Artifact {
        artifact,
        eligibility,
    })
}

/// M6C1 production entry point. One frame root and one root-owned effect become
/// the sole opacity authority; every recorded paint op is neutralized and the
/// compiler applies the owning effect exactly once at the group composite.
pub(crate) fn record_root_group_opacity_frame_artifact(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    mode: RendererMode,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    if mode == RendererMode::Legacy {
        return fallback_or_forced(
            mode,
            FrameArtifactEligibility {
                reasons: vec![FrameArtifactFallbackReason::RendererLegacy],
                ..FrameArtifactEligibility::default()
            },
        );
    }
    let plan = match root_opacity_group_plan(arena, roots, property_trees) {
        Ok(plan) => plan,
        Err(reasons) => {
            return fallback_or_forced(
                mode,
                FrameArtifactEligibility {
                    reasons,
                    ..FrameArtifactEligibility::default()
                },
            );
        }
    };
    record_frame_artifact_with_policy(
        arena,
        roots,
        property_trees,
        paint_generations,
        mode,
        FrameArtifactAuthorityPolicy::RootOpacityGroup(plan),
        None,
        None,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RootOpacityGroupPlan {
    root: NodeKey,
    effect: EffectNodeId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameArtifactAuthorityPolicy {
    ExistingBakedProperties,
    PropertyNeutral,
    ClipEnabled,
    PropertyScene,
    /// Complete recording for the production generic Surface DAG.
    SurfaceDag,
    RootOpacityGroup(RootOpacityGroupPlan),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SnapshotMerge {
    Inserted,
    Identical,
    Conflict,
}

fn merge_snapshot<K, V>(store: &mut FxHashMap<K, V>, key: K, snapshot: V) -> SnapshotMerge
where
    K: Copy + Eq + std::hash::Hash,
    V: Copy + PartialEq,
{
    match store.entry(key) {
        Entry::Vacant(entry) => {
            entry.insert(snapshot);
            SnapshotMerge::Inserted
        }
        Entry::Occupied(entry) if *entry.get() == snapshot => SnapshotMerge::Identical,
        Entry::Occupied(_) => SnapshotMerge::Conflict,
    }
}

/// V2 candidate preflight. Before cutover it is called only by
/// [`record_closed_single_target_frame_artifact`]; existing retained and
/// ArtifactCanary recorder paths must not call it or change their frozen
/// artifact stores.
/// On success the artifact owns every transitive property snapshot needed by
/// its chunk states and recorded owner endpoints, and no later stage needs the
/// arena. This closure is complete only for the artifact's admitted owner set;
/// it is not a global property-tree completeness claim.
pub(super) fn populate_referenced_property_snapshots(
    artifact: &mut PaintArtifact,
    property_trees: &PropertyTrees,
) -> Result<(), Vec<FrameArtifactFallbackReason>> {
    let _profile =
        crate::view::paint::work_profile::scope("populate_referenced_property_snapshots");
    artifact.transform_nodes.clear();
    artifact.layout_position_nodes.clear();
    artifact.visual_offset_nodes.clear();
    artifact.scroll_nodes.clear();

    let referenced_states = artifact
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

    let mut clips = FxHashMap::default();
    for snapshot in artifact.clip_nodes.iter().copied() {
        if merge_snapshot(&mut clips, snapshot.id, snapshot) == SnapshotMerge::Conflict {
            return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
                snapshot.owner,
            )]);
        }
    }
    let mut effects = FxHashMap::default();
    for snapshot in artifact.effect_nodes.iter().copied() {
        if merge_snapshot(&mut effects, snapshot.id, snapshot) == SnapshotMerge::Conflict {
            return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
                snapshot.owner,
            )]);
        }
    }
    let mut transforms = FxHashMap::<TransformNodeId, TransformNodeSnapshot>::default();
    let mut positions = FxHashMap::<LayoutPositionNodeId, LayoutPositionNodeSnapshot>::default();
    let mut visuals = FxHashMap::<VisualOffsetNodeId, VisualOffsetNodeSnapshot>::default();
    let mut scrolls = FxHashMap::<ScrollNodeId, ScrollNodeSnapshot>::default();
    for (owner, state) in referenced_states {
        let invalid = || vec![FrameArtifactFallbackReason::PropertyBoundary(owner)];
        for snapshot in property_trees
            .clip_snapshot_for(state.clip)
            .ok_or_else(invalid)?
        {
            match merge_snapshot(&mut clips, snapshot.id, snapshot) {
                SnapshotMerge::Inserted => artifact.clip_nodes.push(snapshot),
                SnapshotMerge::Identical => {}
                SnapshotMerge::Conflict => return Err(invalid()),
            }
        }
        for snapshot in property_trees
            .effect_snapshot_for(state.effect)
            .ok_or_else(invalid)?
        {
            match merge_snapshot(&mut effects, snapshot.id, snapshot) {
                SnapshotMerge::Inserted => artifact.effect_nodes.push(snapshot),
                SnapshotMerge::Identical => {}
                SnapshotMerge::Conflict => return Err(invalid()),
            }
        }
        let mut anchor_visual_roots = Vec::new();
        for snapshot in snapshot_closure::unseen_chain(
            state.transform,
            &transforms,
            |id| property_trees.transform_snapshot_for(id),
            |s| s.parent,
        )
        .ok_or_else(invalid)?
        {
            match merge_snapshot(&mut transforms, snapshot.id, snapshot) {
                SnapshotMerge::Inserted => artifact.transform_nodes.push(snapshot),
                SnapshotMerge::Identical => {}
                SnapshotMerge::Conflict => return Err(invalid()),
            }
        }
        for snapshot in snapshot_closure::unseen_chain(
            state.layout_position,
            &positions,
            |id| property_trees.layout_position_snapshot_for(id),
            |s| match s.reference {
                SpatialPositionReference::Viewport
                | SpatialPositionReference::LayoutParent(None) => None,
                SpatialPositionReference::LayoutParent(Some(parent))
                | SpatialPositionReference::Anchor(parent) => Some(LayoutPositionNodeId(parent)),
            },
        )
        .ok_or_else(invalid)?
        {
            if let SpatialPositionReference::Anchor(anchor) = snapshot.reference {
                anchor_visual_roots.push(VisualOffsetNodeId(anchor));
            }
            if let Some(scroll) = snapshot.reference_scroll {
                for scroll_snapshot in snapshot_closure::unseen_chain(
                    Some(scroll),
                    &scrolls,
                    |id| property_trees.scroll_snapshot_for(id),
                    |s| s.parent,
                )
                .ok_or_else(invalid)?
                {
                    match merge_snapshot(&mut scrolls, scroll_snapshot.id, scroll_snapshot) {
                        SnapshotMerge::Inserted => artifact.scroll_nodes.push(scroll_snapshot),
                        SnapshotMerge::Identical => {}
                        SnapshotMerge::Conflict => return Err(invalid()),
                    }
                }
            }
            match merge_snapshot(&mut positions, snapshot.id, snapshot) {
                SnapshotMerge::Inserted => artifact.layout_position_nodes.push(snapshot),
                SnapshotMerge::Identical => {}
                SnapshotMerge::Conflict => return Err(invalid()),
            }
        }
        for snapshot in snapshot_closure::unseen_chain(
            state.visual_offset,
            &visuals,
            |id| property_trees.visual_offset_snapshot_for(id),
            |s| s.parent,
        )
        .ok_or_else(invalid)?
        {
            match merge_snapshot(&mut visuals, snapshot.id, snapshot) {
                SnapshotMerge::Inserted => artifact.visual_offset_nodes.push(snapshot),
                SnapshotMerge::Identical => {}
                SnapshotMerge::Conflict => return Err(invalid()),
            }
        }
        for anchor in anchor_visual_roots {
            for snapshot in snapshot_closure::unseen_chain(
                Some(anchor),
                &visuals,
                |id| property_trees.visual_offset_snapshot_for(id),
                |s| s.parent,
            )
            .ok_or_else(invalid)?
            {
                match merge_snapshot(&mut visuals, snapshot.id, snapshot) {
                    SnapshotMerge::Inserted => artifact.visual_offset_nodes.push(snapshot),
                    SnapshotMerge::Identical => {}
                    SnapshotMerge::Conflict => return Err(invalid()),
                }
            }
        }
        for snapshot in snapshot_closure::unseen_chain(
            state.scroll,
            &scrolls,
            |id| property_trees.scroll_snapshot_for(id),
            |s| s.parent,
        )
        .ok_or_else(invalid)?
        {
            match merge_snapshot(&mut scrolls, snapshot.id, snapshot) {
                SnapshotMerge::Inserted => artifact.scroll_nodes.push(snapshot),
                SnapshotMerge::Identical => {}
                SnapshotMerge::Conflict => return Err(invalid()),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn populate_referenced_property_snapshots_for_test(
    artifact: &mut PaintArtifact,
    property_trees: &PropertyTrees,
) -> Result<(), Vec<FrameArtifactFallbackReason>> {
    populate_referenced_property_snapshots(artifact, property_trees)
}

fn record_frame_artifact_with_policy(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    mode: RendererMode,
    policy: FrameArtifactAuthorityPolicy,
    consumed_ancestor_property: Option<super::ConsumedAncestorProperty>,
    required_scroll_content_paint_offset_bits: Option<[u32; 2]>,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    record_frame_artifact_with_policy_and_stack(
        arena,
        roots,
        property_trees,
        paint_generations,
        mode,
        policy,
        consumed_ancestor_property,
        None,
        None,
        required_scroll_content_paint_offset_bits,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn record_frame_artifact_with_policy_and_stack(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    mode: RendererMode,
    policy: FrameArtifactAuthorityPolicy,
    consumed_ancestor_property: Option<super::ConsumedAncestorProperty>,
    consumed_ancestor_property_stack: Option<super::ConsumedAncestorPropertyStackWitness>,
    neutral_effect_authority: Option<EffectNodeId>,
    required_scroll_content_paint_offset_bits: Option<[u32; 2]>,
    mut recording_cache: Option<&mut super::RecordingCache>,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    let _profile =
        crate::view::paint::work_profile::scope("record_frame_artifact_with_policy_and_stack");
    if mode == RendererMode::Legacy {
        return Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(
            FrameArtifactEligibility {
                eligible: false,
                reasons: vec![FrameArtifactFallbackReason::RendererLegacy],
                ..FrameArtifactEligibility::default()
            },
        ));
    }
    let initial_recording_context = PaintRecordingContext {
        consumed_ancestor_property,
        consumed_ancestor_property_stack,
        required_scroll_content_paint_offset_bits,
        opacity_authority: match (neutral_effect_authority, policy) {
            (Some(effect), _) => PaintOpacityAuthority::NeutralRootEffect(effect),
            (_, FrameArtifactAuthorityPolicy::RootOpacityGroup(plan)) => {
                PaintOpacityAuthority::NeutralRootEffect(plan.effect)
            }
            _ => PaintOpacityAuthority::Baked,
        },
        surface_dag: policy == FrameArtifactAuthorityPolicy::SurfaceDag,
        ..PaintRecordingContext::default()
    };
    let planned_boundary_cutouts = super::PlannedBoundaryCutoutSet::default();
    let mut preflight = super::coverage_manifest::record_cached_coverage_manifest(
        arena,
        roots,
        false,
        true,
        CoverageRecordingMode::MetadataOnly,
        property_trees,
        paint_generations,
        initial_recording_context,
        None,
        &planned_boundary_cutouts,
        recording_cache.as_deref_mut(),
    );
    let mut preflight_eligibility = assess_manifest(&preflight, policy);
    if matches!(
        policy,
        FrameArtifactAuthorityPolicy::PropertyNeutral
            | FrameArtifactAuthorityPolicy::ClipEnabled
            | FrameArtifactAuthorityPolicy::SurfaceDag
    ) {
        for reason in production_property_boundary_reasons(arena, roots, property_trees, policy) {
            if !preflight_eligibility.reasons.contains(&reason) {
                preflight_eligibility.reasons.push(reason);
            }
        }
        preflight_eligibility.eligible = preflight_eligibility.reasons.is_empty();
    }
    if !preflight_eligibility.eligible {
        return fallback_or_forced(mode, preflight_eligibility);
    }

    let filled_preflight = recording_cache
        .as_deref_mut()
        .and_then(|cache| cache.materialize(arena, &mut preflight))
        .is_some();
    let (manifest, eligibility) = if filled_preflight {
        // The cache replaces ops only, after matching every native hook's exact
        // metadata. Metadata/ordering equality is guaranteed by construction.
        let eligibility = assess_manifest(&preflight, policy);
        (preflight, eligibility)
    } else {
        let manifest = super::coverage_manifest::record_cached_coverage_manifest(
            arena,
            roots,
            false,
            true,
            CoverageRecordingMode::FullArtifact,
            property_trees,
            paint_generations,
            initial_recording_context,
            None,
            &planned_boundary_cutouts,
            recording_cache.as_deref_mut(),
        );
        let mut eligibility = assess_manifest(&manifest, policy);
        if eligibility.eligible && !canonical_manifest_matches(&preflight, &manifest) {
            eligibility.eligible = false;
            eligibility
                .reasons
                .push(FrameArtifactFallbackReason::Validation(
                    PaintCoverageValidationError::RecordingPassMismatch,
                ));
        }
        (manifest, eligibility)
    };
    if !eligibility.eligible {
        return fallback_or_forced(mode, eligibility);
    }

    let target = match policy {
        FrameArtifactAuthorityPolicy::RootOpacityGroup(plan) => {
            PaintArtifactTarget::RootOpacityGroup {
                root: plan.root,
                effect: plan.effect,
            }
        }
        FrameArtifactAuthorityPolicy::ExistingBakedProperties
        | FrameArtifactAuthorityPolicy::PropertyNeutral
        | FrameArtifactAuthorityPolicy::ClipEnabled
        | FrameArtifactAuthorityPolicy::PropertyScene
        | FrameArtifactAuthorityPolicy::SurfaceDag => PaintArtifactTarget::CurrentTarget,
    };
    materialize_frame_artifact(manifest, target, mode, eligibility)
}

/// Turn an already-assessed coverage manifest into the artifact.
///
/// Policy-free by construction: every property assertion has run by the time
/// this is called, so the only failures left are snapshot conflicts between
/// two chunks that claim the same node or owner endpoint pair.
pub(super) fn materialize_frame_artifact(
    manifest: super::PaintCoverageManifest,
    target: PaintArtifactTarget,
    mode: RendererMode,
    mut eligibility: FrameArtifactEligibility,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    let _profile = crate::view::paint::work_profile::scope("materialize_frame_artifact");
    let mut artifact = PaintArtifact {
        target,
        ..PaintArtifact::default()
    };
    let mut seen_clip_nodes = FxHashMap::default();
    let mut seen_effect_nodes = FxHashMap::default();
    let mut seen_owner_nodes = FxHashMap::default();
    let mut seen_owner_property_states = FxHashMap::default();
    for item in manifest.items {
        let PaintCoverageItem::ArtifactChunk {
            chunk,
            clip_snapshot,
            effect_snapshot,
            owner_snapshot,
            owner_property_state_snapshot,
            ops: Some(ops),
            ..
        } = item
        else {
            if matches!(
                item,
                PaintCoverageItem::TransparentNode { .. }
                    | PaintCoverageItem::CulledSubtree { .. }
                    | PaintCoverageItem::PlannedBoundary { .. }
            ) {
                continue;
            }
            unreachable!("eligibility rejects all paint boundaries")
        };
        let start = artifact.ops.len();
        artifact.ops.extend(ops);
        let end = artifact.ops.len();
        artifact.chunks.push(PaintChunk {
            id: chunk.id,
            owner: chunk.owner,
            op_range: start..end,
            bounds: chunk.bounds,
            properties: chunk.properties,
            content_revision: chunk.content_revision,
            payload_identity: chunk.payload_identity,
        });
        for snapshot in clip_snapshot {
            match merge_snapshot(&mut seen_clip_nodes, snapshot.id, snapshot) {
                SnapshotMerge::Inserted => {
                    artifact.clip_nodes.push(snapshot);
                }
                SnapshotMerge::Conflict => {
                    eligibility.eligible = false;
                    eligibility
                        .reasons
                        .push(FrameArtifactFallbackReason::Validation(
                            PaintCoverageValidationError::ConflictingClipSnapshot(snapshot.id),
                        ));
                    return fallback_or_forced(mode, eligibility);
                }
                SnapshotMerge::Identical => {}
            }
        }
        for snapshot in effect_snapshot {
            match merge_snapshot(&mut seen_effect_nodes, snapshot.id, snapshot) {
                SnapshotMerge::Inserted => {
                    artifact.effect_nodes.push(snapshot);
                }
                SnapshotMerge::Conflict => {
                    eligibility.eligible = false;
                    eligibility
                        .reasons
                        .push(FrameArtifactFallbackReason::Validation(
                            PaintCoverageValidationError::ConflictingEffectSnapshot(snapshot.id),
                        ));
                    return fallback_or_forced(mode, eligibility);
                }
                SnapshotMerge::Identical => {}
            }
        }
        for snapshot in owner_snapshot {
            match merge_snapshot(&mut seen_owner_nodes, snapshot.owner, snapshot) {
                SnapshotMerge::Inserted => {
                    artifact.owner_nodes.push(snapshot);
                }
                SnapshotMerge::Conflict => {
                    eligibility.eligible = false;
                    eligibility
                        .reasons
                        .push(FrameArtifactFallbackReason::Validation(
                            PaintCoverageValidationError::ConflictingOwnerSnapshot(snapshot.owner),
                        ));
                    return fallback_or_forced(mode, eligibility);
                }
                SnapshotMerge::Identical => {}
            }
        }
        for snapshot in owner_property_state_snapshot {
            match merge_snapshot(&mut seen_owner_property_states, snapshot.owner, snapshot) {
                SnapshotMerge::Inserted => {
                    artifact.owner_property_states.push(snapshot);
                }
                SnapshotMerge::Conflict => {
                    eligibility.eligible = false;
                    eligibility
                        .reasons
                        .push(FrameArtifactFallbackReason::Validation(
                            PaintCoverageValidationError::ConflictingOwnerPropertyState(
                                snapshot.owner,
                            ),
                        ));
                    return fallback_or_forced(mode, eligibility);
                }
                SnapshotMerge::Identical => {}
            }
        }
    }
    Ok(FrameArtifactRecordOutcome::Artifact {
        artifact,
        eligibility,
    })
}

#[cfg(test)]
mod snapshot_merge_tests;

fn assess_manifest(
    manifest: &super::PaintCoverageManifest,
    policy: FrameArtifactAuthorityPolicy,
) -> FrameArtifactEligibility {
    let mut reasons = manifest
        .validation_errors
        .iter()
        .cloned()
        .map(FrameArtifactFallbackReason::Validation)
        .collect::<Vec<_>>();
    let mut chunk_count = 0usize;
    let mut op_count = 0usize;
    let mut debug_boundaries = Vec::new();
    for item in &manifest.items {
        match item {
            PaintCoverageItem::ArtifactChunk { chunk, ops, .. } => {
                chunk_count = chunk_count.saturating_add(1);
                op_count = op_count.saturating_add(ops.as_ref().map_or(0, Vec::len));
                if policy == FrameArtifactAuthorityPolicy::PropertyNeutral
                    && chunk.properties.legacy_boundary_dimensions() != Default::default()
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if policy == FrameArtifactAuthorityPolicy::ClipEnabled
                    && (chunk.properties.transform.is_some()
                        || chunk.properties.effect.is_some()
                        || chunk.properties.scroll.is_some())
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if policy == FrameArtifactAuthorityPolicy::PropertyScene
                    && (chunk.properties.transform.is_some()
                        || chunk.properties.effect.is_some()
                        || chunk.properties.scroll.is_some())
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let FrameArtifactAuthorityPolicy::RootOpacityGroup(plan) = policy {
                    if chunk.properties.effect != Some(plan.effect) {
                        let reason = FrameArtifactFallbackReason::NestedEffect(chunk.owner);
                        if !reasons.contains(&reason) {
                            reasons.push(reason);
                        }
                    }
                    if chunk.properties.transform.is_some() || chunk.properties.scroll.is_some() {
                        let reason = FrameArtifactFallbackReason::NonEffectProperty(chunk.owner);
                        if !reasons.contains(&reason) {
                            reasons.push(reason);
                        }
                    }
                }
            }
            PaintCoverageItem::TransparentNode {
                owner, properties, ..
            } => {
                if policy == FrameArtifactAuthorityPolicy::PropertyScene
                    && (properties.transform.is_some()
                        || properties.effect.is_some()
                        || properties.scroll.is_some())
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(*owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
            }
            PaintCoverageItem::CulledSubtree {
                owner, properties, ..
            } => {
                let invalid = if policy == FrameArtifactAuthorityPolicy::SurfaceDag {
                    // Culling is the absence of paint, not a property-family
                    // restriction. Generic recording validates the complete
                    // live property graph before assessing this manifest.
                    // Re-entry creates chunks again and invalidates their
                    // raster coverage; an inherited scroll is not a fallback.
                    false
                } else {
                    properties.transform.is_some()
                        || properties.effect.is_some()
                        || properties.scroll.is_some()
                };
                if invalid {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(*owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
            }
            PaintCoverageItem::LegacyBoundary { root, reason, .. } => {
                let boundary = FrameArtifactDebugBoundary {
                    owner: *root,
                    kind: FrameArtifactDebugBoundaryKind::Legacy(*reason),
                };
                if !debug_boundaries.contains(&boundary) {
                    debug_boundaries.push(boundary);
                }
                let reason = FrameArtifactFallbackReason::LegacyBoundary(*reason);
                if !reasons.contains(&reason) {
                    reasons.push(reason);
                }
            }
            PaintCoverageItem::PlannedBoundary { .. } => {
                if policy != FrameArtifactAuthorityPolicy::PropertyScene {
                    let reason = FrameArtifactFallbackReason::Validation(
                        PaintCoverageValidationError::RecordingPassMismatch,
                    );
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
            }
            PaintCoverageItem::NativeScrollContentReceiver { .. } => {
                let reason = FrameArtifactFallbackReason::Validation(
                    PaintCoverageValidationError::RecordingPassMismatch,
                );
                if !reasons.contains(&reason) {
                    reasons.push(reason);
                }
            }
        }
    }
    FrameArtifactEligibility {
        eligible: reasons.is_empty(),
        reasons: reasons.clone(),
        chunk_count,
        op_count,
        debug_boundaries,
    }
}

fn root_opacity_group_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
) -> Result<RootOpacityGroupPlan, Vec<FrameArtifactFallbackReason>> {
    let [root] = roots else {
        return Err(vec![FrameArtifactFallbackReason::RootCount(roots.len())]);
    };
    if arena.get(*root).is_none() {
        return Err(vec![FrameArtifactFallbackReason::MissingRootEffect(*root)]);
    }
    let effect = EffectNodeId(*root);
    let Some(root_effect) = property_trees.effects.get(&effect) else {
        return Err(vec![FrameArtifactFallbackReason::MissingRootEffect(*root)]);
    };
    let mut reasons = Vec::new();
    if root_effect.owner != *root
        || root_effect.parent.is_some()
        || root_effect.generation == 0
        || !root_effect.opacity.is_finite()
        || !(0.0..=1.0).contains(&root_effect.opacity)
    {
        reasons.push(FrameArtifactFallbackReason::InvalidRootEffect(*root));
    }
    for (&id, snapshot) in &property_trees.effects {
        if id != effect {
            let reason = FrameArtifactFallbackReason::NestedEffect(snapshot.owner);
            if !reasons.contains(&reason) {
                reasons.push(reason);
            }
        }
    }

    let mut stack = vec![*root];
    let mut seen = FxHashSet::default();
    while let Some(key) = stack.pop() {
        if !seen.insert(key) {
            continue;
        }
        let Some(node) = arena.get(key) else {
            continue;
        };
        if node.element.children() != node.children()
            || node
                .children()
                .iter()
                .any(|child| arena.parent_of(*child) != Some(key))
        {
            reasons.push(FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::InvalidOwnerSnapshot(key),
            ));
        }
        if node.element.is_deferred_to_root_viewport_render() {
            reasons.push(FrameArtifactFallbackReason::DeferredBoundary(key));
        }
        match property_trees.states.get(&key) {
            Some(state) => {
                for properties in [state.paint, state.descendants] {
                    if properties.effect != Some(effect) {
                        let reason = FrameArtifactFallbackReason::NestedEffect(key);
                        if !reasons.contains(&reason) {
                            reasons.push(reason);
                        }
                    }
                    if properties.transform.is_some() || properties.scroll.is_some() {
                        let reason = FrameArtifactFallbackReason::NonEffectProperty(key);
                        if !reasons.contains(&reason) {
                            reasons.push(reason);
                        }
                    }
                }
            }
            None => reasons.push(FrameArtifactFallbackReason::MissingRootEffect(key)),
        }
        stack.extend(node.children().iter().copied());
    }
    reasons.sort_by_key(|reason| format!("{reason:?}"));
    reasons.dedup();
    if reasons.is_empty() {
        Ok(RootOpacityGroupPlan {
            root: *root,
            effect,
        })
    } else {
        Err(reasons)
    }
}

fn production_property_boundary_reasons(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    policy: FrameArtifactAuthorityPolicy,
) -> Vec<FrameArtifactFallbackReason> {
    let mut reasons = Vec::new();
    let mut stack = roots.to_vec();
    let mut seen = FxHashSet::default();
    while let Some(key) = stack.pop() {
        if !seen.insert(key) {
            continue;
        }
        let Some(node) = arena.get(key) else {
            continue;
        };
        if policy == FrameArtifactAuthorityPolicy::SurfaceDag
            && (node.element.children() != node.children()
                || node
                    .children()
                    .iter()
                    .any(|child| arena.parent_of(*child) != Some(key)))
        {
            reasons.push(FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::InvalidOwnerSnapshot(key),
            ));
        }
        let exact_deferred_viewport_root = exact_deferred_viewport_self_clip_witness(
            arena,
            key,
            property_trees,
            policy == FrameArtifactAuthorityPolicy::SurfaceDag,
        )
        .is_some();
        if node.element.is_deferred_to_root_viewport_render() && !exact_deferred_viewport_root {
            let reason = FrameArtifactFallbackReason::LegacyBoundary(LegacyPaintReason::Deferred);
            if !reasons.contains(&reason) {
                reasons.push(reason);
            }
        }
        let property_boundary = property_trees.states.get(&key).is_some_and(|state| {
            [state.paint, state.descendants]
                .into_iter()
                .any(|properties| match policy {
                    FrameArtifactAuthorityPolicy::PropertyNeutral => {
                        properties.legacy_boundary_dimensions() != Default::default()
                    }
                    FrameArtifactAuthorityPolicy::ClipEnabled => {
                        properties.transform.is_some()
                            || properties.effect.is_some()
                            || properties.scroll.is_some()
                    }
                    FrameArtifactAuthorityPolicy::PropertyScene => {
                        properties.transform.is_some()
                            || properties.effect.is_some()
                            || properties.scroll.is_some()
                    }
                    FrameArtifactAuthorityPolicy::SurfaceDag => false,
                    FrameArtifactAuthorityPolicy::ExistingBakedProperties
                    | FrameArtifactAuthorityPolicy::RootOpacityGroup(_) => false,
                })
        });
        if property_boundary {
            reasons.push(FrameArtifactFallbackReason::PropertyBoundary(key));
        }
        stack.extend(node.element.children().iter().copied());
    }
    reasons
}

fn fallback_or_forced(
    mode: RendererMode,
    eligibility: FrameArtifactEligibility,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    match mode {
        RendererMode::StrictPlan => Err(ForcedFrameArtifactError {
            reasons: eligibility.reasons,
        }),
        #[cfg(test)]
        RendererMode::ForcedForTests => Err(ForcedFrameArtifactError {
            reasons: eligibility.reasons,
        }),
        RendererMode::Auto | RendererMode::Legacy => Ok(
            FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility),
        ),
    }
}

pub(super) fn canonical_manifest_matches(
    metadata: &super::PaintCoverageManifest,
    full: &super::PaintCoverageManifest,
) -> bool {
    if metadata.validation_errors != full.validation_errors
        || metadata.items.len() != full.items.len()
    {
        return false;
    }
    metadata
        .items
        .iter()
        .zip(&full.items)
        .all(|(left, right)| match (left, right) {
            (
                PaintCoverageItem::ArtifactChunk {
                    order: left_order,
                    chunk: left_chunk,
                    clip_snapshot: left_clip_snapshot,
                    effect_snapshot: left_effect_snapshot,
                    owner_snapshot: left_owner_snapshot,
                    owner_property_state_snapshot: left_owner_property_state_snapshot,
                    ops: None,
                },
                PaintCoverageItem::ArtifactChunk {
                    order: right_order,
                    chunk: right_chunk,
                    clip_snapshot: right_clip_snapshot,
                    effect_snapshot: right_effect_snapshot,
                    owner_snapshot: right_owner_snapshot,
                    owner_property_state_snapshot: right_owner_property_state_snapshot,
                    ops: Some(_),
                },
            ) => {
                left_order == right_order
                    && left_chunk.id == right_chunk.id
                    && left_chunk.owner == right_chunk.owner
                    && left_chunk.bounds.x.to_bits() == right_chunk.bounds.x.to_bits()
                    && left_chunk.bounds.y.to_bits() == right_chunk.bounds.y.to_bits()
                    && left_chunk.bounds.width.to_bits() == right_chunk.bounds.width.to_bits()
                    && left_chunk.bounds.height.to_bits() == right_chunk.bounds.height.to_bits()
                    && left_chunk.properties == right_chunk.properties
                    && left_chunk.content_revision == right_chunk.content_revision
                    && left_chunk.payload_identity == right_chunk.payload_identity
                    && left_clip_snapshot == right_clip_snapshot
                    && left_effect_snapshot == right_effect_snapshot
                    && left_owner_snapshot == right_owner_snapshot
                    && left_owner_property_state_snapshot == right_owner_property_state_snapshot
            }
            (
                PaintCoverageItem::TransparentNode {
                    order: left_order,
                    owner: left_owner,
                    stable_id: left_stable_id,
                    properties: left_properties,
                    content_revision: left_revision,
                },
                PaintCoverageItem::TransparentNode {
                    order: right_order,
                    owner: right_owner,
                    stable_id: right_stable_id,
                    properties: right_properties,
                    content_revision: right_revision,
                },
            ) => {
                left_order == right_order
                    && left_owner == right_owner
                    && left_stable_id == right_stable_id
                    && left_properties == right_properties
                    && left_revision == right_revision
            }
            (
                PaintCoverageItem::CulledSubtree {
                    order: left_order,
                    owner: left_owner,
                    stable_id: left_stable_id,
                    properties: left_properties,
                    content_revision: left_revision,
                },
                PaintCoverageItem::CulledSubtree {
                    order: right_order,
                    owner: right_owner,
                    stable_id: right_stable_id,
                    properties: right_properties,
                    content_revision: right_revision,
                },
            ) => {
                left_order == right_order
                    && left_owner == right_owner
                    && left_stable_id == right_stable_id
                    && left_properties == right_properties
                    && left_revision == right_revision
            }
            (
                PaintCoverageItem::LegacyBoundary {
                    order: lo,
                    root: lr,
                    stable_id: ls,
                    reason: lreason,
                },
                PaintCoverageItem::LegacyBoundary {
                    order: ro,
                    root: rr,
                    stable_id: rs,
                    reason: rreason,
                },
            ) => lo == ro && lr == rr && ls == rs && lreason == rreason,
            (
                PaintCoverageItem::PlannedBoundary {
                    order: left_order,
                    boundary: left_boundary,
                },
                PaintCoverageItem::PlannedBoundary {
                    order: right_order,
                    boundary: right_boundary,
                },
            ) => left_order == right_order && left_boundary == right_boundary,
            (
                PaintCoverageItem::NativeScrollContentReceiver {
                    order: left_order,
                    cutout: left_cutout,
                },
                PaintCoverageItem::NativeScrollContentReceiver {
                    order: right_order,
                    cutout: right_cutout,
                },
            ) => left_order == right_order && left_cutout == right_cutout,
            _ => false,
        })
}

#[cfg(test)]
mod property_effect_artifact_tests;

#[derive(Clone, Debug, Default)]
pub(crate) struct FrameArtifactEligibility {
    pub(crate) eligible: bool,
    pub(crate) reasons: Vec<FrameArtifactFallbackReason>,
    pub(crate) chunk_count: usize,
    pub(crate) op_count: usize,
    /// Exact coverage boundaries retained for diagnostics. Authority selection
    /// continues to use `reasons`; this witness only preserves the owner that
    /// would otherwise be lost when a `LegacyBoundary` is collapsed by kind.
    pub(crate) debug_boundaries: Vec<FrameArtifactDebugBoundary>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameArtifactDebugBoundary {
    pub(crate) owner: NodeKey,
    pub(crate) kind: FrameArtifactDebugBoundaryKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameArtifactDebugBoundaryKind {
    Legacy(LegacyPaintReason),
}

#[derive(Clone, Debug)]
pub(crate) enum FrameArtifactRecordOutcome {
    Artifact {
        artifact: PaintArtifact,
        eligibility: FrameArtifactEligibility,
    },
    WholeFrameLegacyFallback(FrameArtifactEligibility),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ForcedFrameArtifactError {
    pub(crate) reasons: Vec<FrameArtifactFallbackReason>,
}

pub(super) fn sampled_layout_transition_is_exact(
    element: &dyn crate::view::base_component::ElementTrait,
) -> bool {
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

mod snapshot_closure;
