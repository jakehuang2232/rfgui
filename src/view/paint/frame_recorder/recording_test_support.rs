//! Helpers used only by unit tests.

use super::*;

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
    close_recorded_artifact_property_snapshots(outcome, property_trees, mode, None)
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

/// Turn an already-assessed coverage manifest into the artifact.
///
/// Policy-free by construction: every property assertion has run by the time
/// this is called, so the only failures left are snapshot conflicts between
/// two chunks that claim the same node or owner endpoint pair.
pub(in crate::view::paint) fn materialize_frame_artifact(
    manifest: super::super::PaintCoverageManifest,
    target: PaintArtifactTarget,
    mode: RendererMode,
    eligibility: FrameArtifactEligibility,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    materialize_frame_artifact_with_cache(manifest, target, mode, eligibility, None)
}

pub(super) fn root_opacity_group_plan(
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
