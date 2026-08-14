#![allow(dead_code)]

use std::collections::hash_map::Entry;

use rustc_hash::{FxHashMap, FxHashSet};

fn chunk_bounds_bits(chunk: &super::PaintChunk) -> [u32; 4] {
    [
        chunk.bounds.x,
        chunk.bounds.y,
        chunk.bounds.width,
        chunk.bounds.height,
    ]
    .map(f32::to_bits)
}

use crate::view::compositor::property_tree::{
    ClipNodeRole, ClipNodeSnapshot, EffectNodeId, EffectNodeSnapshot, LayoutPositionNodeId,
    LayoutPositionNodeSnapshot, ScrollNodeId, ScrollNodeSnapshot, SpatialPositionReference,
    TransformNodeId, TransformNodeSnapshot, VisualOffsetNodeId, VisualOffsetNodeSnapshot,
};
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::node_arena::{NodeArena, NodeKey};

use super::coverage_manifest::{
    NativeScrollContentReceiverCutout,
    exact_deferred_viewport_self_clip_witness, record_retained_coverage_manifest_with_context,
    record_retained_coverage_manifest_with_native_scroll_receiver,
    record_retained_coverage_manifest_with_property_authorities,
};

use super::{
    CoverageRecordingMode, EffectPropertySurfaceArtifactContract, LegacyPaintReason, PaintArtifact,
    PaintArtifactTarget, PaintBakedScrollHostWitness, PaintChunk, PaintCoverageItem,
    PaintCoverageValidationError, PaintOpacityAuthority, PaintRecordingContext,
    PaintScrollContentWitness, PaintTransformSurfaceWitness,
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

/// Planning-only recorder for one validated root transform surface. This does
/// not compile or emit the artifact and is intentionally not wired into the
/// production frame dispatch.
pub(super) fn record_transform_surface_artifact_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintTransformSurfaceWitness,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    match record_frame_artifact_with_policy(
        arena,
        roots,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        FrameArtifactAuthorityPolicy::TransformSurface(witness),
        None,
        None,
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => Ok(artifact),
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            Err(eligibility.reasons)
        }
        Err(error) => Err(error.reasons),
    }
}

pub(super) fn record_baked_scroll_host_artifact_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintBakedScrollHostWitness,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    match record_frame_artifact_with_policy(
        arena,
        roots,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        FrameArtifactAuthorityPolicy::BakedScrollHost(witness),
        None,
        None,
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => Ok(artifact),
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            Err(eligibility.reasons)
        }
        Err(error) => Err(error.reasons),
    }
}



/// C3a full host grammar.  It is deliberately graph-inert: callers may test
/// and validate this artifact, but no scroll-scene selector consumes it.
pub(super) fn record_baked_scroll_host_artifact_with_stack_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintBakedScrollHostWitness,
    consumed_stack: super::ConsumedAncestorPropertyStackWitness,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    match record_frame_artifact_with_policy_and_stack(
        arena,
        roots,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        FrameArtifactAuthorityPolicy::BakedScrollHost(witness),
        None,
        Some(consumed_stack),
        None,
        None,
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => Ok(artifact),
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            Err(eligibility.reasons)
        }
        Err(error) => Err(error.reasons),
    }
}

/// Same-owner T+S host recorder. The dedicated witness projects only the
/// owner's transform while `BakedScrollHost` keeps the established H/O vs C
/// split. No generic ancestor transform capability accepts this shape.
pub(super) fn record_same_owner_transform_scroll_host_artifact_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintBakedScrollHostWitness,
    consumed_transform: super::ConsumedSameOwnerTransformBoundaryWitness,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    if roots != [consumed_transform.owner]
        || witness.boundary_root() != consumed_transform.owner
        || consumed_transform.transform.0 != consumed_transform.owner
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            consumed_transform.owner,
        )]);
    }
    let steps = record_ordered_property_steps_for_plan(
        arena,
        roots,
        property_trees,
        paint_generations,
        [0.0; 2],
        &super::PlannedBoundaryCutoutSet::default(),
        Some(PaintTransformSurfaceWitness::canonical_root(
            consumed_transform.owner,
        )),
        None,
        Some(super::ConsumedAncestorProperty::SameOwnerTransformBoundary(
            consumed_transform,
        )),
        PaintOpacityAuthority::Baked,
        FrameArtifactAuthorityPolicy::BakedScrollHost(witness),
        None,
    )?;
    let [RecordedTransformSurfaceStep::Artifact(artifact)] = steps.as_slice() else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            consumed_transform.owner,
        )]);
    };
    Ok(artifact.clone())
}

/// Same-owner E+S host recorder. The dedicated effect witness projects the
/// owner's effect while the effect-surface contract detaches its chain and
/// `BakedScrollHost` preserves the H/O vs C split. The final E composite is
/// therefore the only opacity authority.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_same_owner_effect_scroll_host_artifact_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintBakedScrollHostWitness,
    contract: &EffectPropertySurfaceArtifactContract,
    consumed_effect: super::ConsumedSameOwnerEffectBoundaryWitness,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    if roots != [consumed_effect.owner]
        || witness.boundary_root() != consumed_effect.owner
        || contract.boundary_root() != consumed_effect.owner
        || contract.isolated_leaf() != consumed_effect.effect
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            consumed_effect.owner,
        )]);
    }
    let steps = record_ordered_property_steps_for_plan(
        arena,
        roots,
        property_trees,
        paint_generations,
        [0.0; 2],
        &super::PlannedBoundaryCutoutSet::default(),
        None,
        None,
        Some(super::ConsumedAncestorProperty::SameOwnerEffectBoundary(
            consumed_effect,
        )),
        PaintOpacityAuthority::NeutralRootEffect(consumed_effect.effect.id),
        FrameArtifactAuthorityPolicy::BakedScrollHost(witness),
        None,
    )?;
    let [RecordedTransformSurfaceStep::Artifact(artifact)] = steps.as_slice() else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            consumed_effect.owner,
        )]);
    };
    Ok(artifact.clone())
}

/// Same-owner T+E+S H/O recorder. Both property roles are projected by one
/// sealed stack; `BakedScrollHost` keeps S live only for the C phase. The
/// resulting target is effect- and transform-neutral.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_same_owner_transform_effect_scroll_host_artifact_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintBakedScrollHostWitness,
    consumed_transform: super::ConsumedSameOwnerTransformBoundaryWitness,
    consumed_effect: super::ConsumedSameOwnerEffectBoundaryWitness,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    if roots != [consumed_transform.owner]
        || consumed_transform.owner != consumed_effect.owner
        || witness.boundary_root() != consumed_transform.owner
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            consumed_transform.owner,
        )]);
    }
    let stack = super::ConsumedAncestorPropertyStackWitness::new_same_owner_transform_effect_host(
        consumed_transform.owner,
        consumed_transform,
        consumed_effect,
    )
    .ok_or_else(|| {
        vec![FrameArtifactFallbackReason::PropertyBoundary(
            consumed_transform.owner,
        )]
    })?;
    let steps = record_ordered_property_steps_with_stack_for_plan(
        arena,
        roots,
        property_trees,
        paint_generations,
        [0.0; 2],
        &super::PlannedBoundaryCutoutSet::default(),
        Some(PaintTransformSurfaceWitness::canonical_root(
            consumed_transform.owner,
        )),
        None,
        None,
        Some(stack),
        None,
        PaintOpacityAuthority::NeutralRootEffect(consumed_effect.effect.id),
        FrameArtifactAuthorityPolicy::BakedScrollHost(witness),
        None,
    )?;
    let [RecordedTransformSurfaceStep::Artifact(artifact)] = steps.as_slice() else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            consumed_transform.owner,
        )]);
    };
    Ok(artifact.clone())
}

/// Strict E->S checkpoint recorder. H/O keep the baked-scroll structural
/// witness while their inherited effect is projected by the exact stack and
/// neutralized by the same root-effect authority as the receiver artifact.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_effect_baked_scroll_host_artifact_with_stack_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintBakedScrollHostWitness,
    consumed_stack: super::ConsumedAncestorPropertyStackWitness,
    effect: EffectNodeSnapshot,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    if effect.id.0 != effect.owner
        || effect.parent.is_some()
        || effect.generation == 0
        || !effect.opacity.is_finite()
        || !(0.0..=1.0).contains(&effect.opacity)
    {
        return Err(vec![FrameArtifactFallbackReason::InvalidRootEffect(
            effect.owner,
        )]);
    }
    match record_frame_artifact_with_policy_and_stack(
        arena,
        roots,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        FrameArtifactAuthorityPolicy::BakedScrollHost(witness),
        None,
        Some(consumed_stack),
        Some(effect.id),
        None,
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => Ok(artifact),
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            Err(eligibility.reasons)
        }
        Err(error) => Err(error.reasons),
    }
}

/// Records only the scroll host's detached content subtree in offset-zero
/// geometry. The host's self paint and scrollbar overlay remain separate
/// scene artifacts and never enter this recorder.
pub(super) fn record_scroll_content_local_artifact_for_plan(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintScrollContentWitness,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    let boundary_root = witness.boundary_root();
    let content_root = witness.content_root();
    let scroll = witness.scroll_snapshot();
    let contents_clip = witness.contents_clip_snapshot();
    let normalization_offset = witness.normalization_paint_offset();
    let Some(boundary_node) = arena.get(boundary_root) else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    };
    let Some(content_node) = arena.get(content_root) else {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::MissingNode(content_root),
        )]);
    };
    let Some(content_element) = content_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
    else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    };
    let Some(required_paint_offset) =
        content_element.exact_retained_scroll_content_recording_offset(normalization_offset)
    else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    };
    let content_bounds =
        crate::view::base_component::ElementTrait::box_model_snapshot(content_element);
    let exact_property = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(contents_clip.id),
        scroll: Some(scroll.id),
        ..Default::default()
    };
    if boundary_node.element.children() != [content_root]
        || arena.parent_of(content_root) != Some(boundary_root)
        || (content_bounds.x + scroll.offset.x).to_bits()
            != scroll.layout_content_bounds_at_zero.x.to_bits()
        || (content_bounds.y + scroll.offset.y).to_bits()
            != scroll.layout_content_bounds_at_zero.y.to_bits()
        || content_bounds.width.to_bits() != scroll.content_size.width.to_bits()
        || content_bounds.height.to_bits() != scroll.content_size.height.to_bits()
        || !property_trees.validation_errors.is_empty()
        || property_trees.transforms.len() != 0
        || property_trees.effects.len() != 0
        || property_trees.scroll_snapshot_for(scroll.id) != Some(scroll)
        || property_trees
            .clip_snapshot_for(Some(contents_clip.id))
            .is_none_or(|snapshots| snapshots.as_slice() != [contents_clip])
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    }

    let mut stack = vec![(content_root, boundary_root)];
    let mut seen = FxHashSet::default();
    let mut expected_owner_parents = FxHashMap::default();
    while let Some((key, expected_parent)) = stack.pop() {
        if !seen.insert(key) {
            return Err(vec![FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::DuplicateNodeKey(key),
            )]);
        }
        let Some(node) = arena.get(key) else {
            return Err(vec![FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::MissingNode(key),
            )]);
        };
        if arena.parent_of(key) != Some(expected_parent) {
            return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(key)]);
        }
        expected_owner_parents.insert(key, (key != content_root).then_some(expected_parent));
        if node.element.is_deferred_to_root_viewport_render() {
            return Err(vec![FrameArtifactFallbackReason::DeferredBoundary(key)]);
        }
        if node
            .element
            .placement_eligibility_metadata()
            .contains_runtime_layout_state
            || property_trees.states.get(&key).is_none_or(|state| {
                !state.paint.legacy_boundary_eq(exact_property)
                    || !state.descendants.legacy_boundary_eq(exact_property)
            })
        {
            return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(key)]);
        }
        stack.extend(
            node.element
                .children()
                .iter()
                .copied()
                .map(|child| (child, key)),
        );
    }

    let artifact = match record_frame_artifact_with_policy(
        arena,
        &[content_root],
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        FrameArtifactAuthorityPolicy::ScrollContentLocal(witness),
        None,
        Some(required_paint_offset.map(f32::to_bits)),
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    let detached_root_count = artifact
        .owner_nodes
        .iter()
        .filter(|snapshot| snapshot.parent.is_none())
        .count();
    let has_detached_content_root = artifact
        .owner_nodes
        .iter()
        .any(|snapshot| snapshot.owner == content_root && snapshot.parent.is_none());
    if artifact.chunks.is_empty()
        || !matches!(artifact.target, PaintArtifactTarget::CurrentTarget)
        || !artifact.clip_nodes.is_empty()
        || !artifact.effect_nodes.is_empty()
        || detached_root_count != 1
        || !has_detached_content_root
        || artifact.owner_nodes.len() != seen.len()
        || artifact.owner_nodes.iter().any(|snapshot| {
            expected_owner_parents.get(&snapshot.owner).copied() != Some(snapshot.parent)
        })
        || seen.iter().any(|owner| {
            !artifact
                .owner_nodes
                .iter()
                .any(|snapshot| snapshot.owner == *owner)
        })
        || artifact
            .chunks
            .iter()
            .any(|chunk| chunk.properties.legacy_boundary_dimensions() != Default::default())
        || artifact.chunks.iter().any(|chunk| {
            chunk.id.role == super::PaintChunkRole::ScrollbarOverlay || chunk.owner == boundary_root
        })
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    }
    Ok(artifact)
}

/// Generalized descendant-scroll content recorder. The frame-plan witness
/// owns the surrounding receiver and exact clip partition; this function
/// consumes only the scroll pair and records the complete native content
/// subtree at the supplied, planner-verified offset-zero basis.
pub(super) fn record_generalized_scroll_content_artifact_for_plan(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintScrollContentWitness,
    required_paint_offset: [f32; 2],
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    let content_root = witness.content_root();
    if required_paint_offset.iter().any(|value| !value.is_finite())
        || arena.parent_of(content_root) != Some(witness.boundary_root())
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    }
    let artifact = match record_frame_artifact_with_policy(
        arena,
        &[content_root],
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        FrameArtifactAuthorityPolicy::ScrollContentLocal(witness),
        None,
        Some(required_paint_offset.map(f32::to_bits)),
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    (!artifact.chunks.is_empty() && matches!(artifact.target, PaintArtifactTarget::CurrentTarget))
        .then_some(artifact)
        .ok_or_else(|| vec![FrameArtifactFallbackReason::PropertyBoundary(content_root)])
}


pub(super) fn record_scroll_content_local_artifact_with_stack_for_plan(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintScrollContentWitness,
    consumed_stack: super::ConsumedAncestorPropertyStackWitness,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    let content_root = witness.content_root();
    let Some(content_node) = arena.get(content_root) else {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::MissingNode(content_root),
        )]);
    };
    let Some(content_element) = content_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
    else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    };
    let Some(required_paint_offset) = content_element
        .exact_retained_scroll_content_recording_offset(witness.normalization_paint_offset())
    else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    };
    let artifact = match record_frame_artifact_with_policy_and_stack(
        arena,
        &[content_root],
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        FrameArtifactAuthorityPolicy::ScrollContentLocal(witness),
        None,
        Some(consumed_stack),
        None,
        Some(required_paint_offset.map(f32::to_bits)),
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    if artifact.chunks.is_empty()
        || !matches!(artifact.target, PaintArtifactTarget::CurrentTarget)
        || !artifact.clip_nodes.is_empty()
        || !artifact.effect_nodes.is_empty()
        || artifact
            .chunks
            .iter()
            .any(|chunk| chunk.properties.legacy_boundary_dimensions() != Default::default())
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    }
    Ok(artifact)
}

/// Strict E->S detached-content recorder. The stack must project Effect then
/// ScrollContents; the neutral authority must match the exact effect witness.
pub(super) fn record_effect_scroll_content_local_artifact_with_stack_for_plan(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintScrollContentWitness,
    consumed_stack: super::ConsumedAncestorPropertyStackWitness,
    effect: EffectNodeSnapshot,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    let content_root = witness.content_root();
    let Some(content_node) = arena.get(content_root) else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    };
    let Some(content_element) = content_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
    else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    };
    let Some(required_paint_offset) = content_element
        .exact_retained_scroll_content_recording_offset(witness.normalization_paint_offset())
    else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    };
    let artifact = match record_frame_artifact_with_policy_and_stack(
        arena,
        &[content_root],
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        FrameArtifactAuthorityPolicy::ScrollContentLocal(witness),
        None,
        Some(consumed_stack),
        Some(effect.id),
        Some(required_paint_offset.map(f32::to_bits)),
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    if artifact.chunks.is_empty()
        || !matches!(artifact.target, PaintArtifactTarget::CurrentTarget)
        || !artifact.clip_nodes.is_empty()
        || !artifact.effect_nodes.is_empty()
        || artifact
            .chunks
            .iter()
            .any(|chunk| chunk.properties.legacy_boundary_dimensions() != Default::default())
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    }
    Ok(artifact)
}

/// Planning-only recorder for one direct child opacity isolation whose
/// inherited transform is already owned by its parent retained surface.  The
/// live tree must match the witness exactly before either recording pass; the
/// artifact view projects only that consumed transform and neutralizes the
/// child root effect.
pub(super) fn record_transform_child_isolation_artifact_for_plan(
    arena: &NodeArena,
    parent_root: NodeKey,
    child_root: NodeKey,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    let transform = crate::view::compositor::property_tree::TransformNodeId(parent_root);
    let effect = EffectNodeId(child_root);
    let Some(witness) =
        super::ConsumedAncestorTransformWitness::new(parent_root, child_root, transform)
    else {
        return Err(vec![FrameArtifactFallbackReason::NonEffectProperty(
            child_root,
        )]);
    };
    let exact_transform = property_trees.transforms.len() == 1
        && property_trees
            .transforms
            .get(&transform)
            .is_some_and(|node| {
                node.owner == parent_root && node.parent.is_none() && node.generation != 0
            });
    let exact_effect = property_trees.effects.len() == 1
        && property_trees.effects.get(&effect).is_some_and(|node| {
            node.owner == child_root
                && node.parent.is_none()
                && node.generation != 0
                && node.opacity.is_finite()
                && (0.0..=1.0).contains(&node.opacity)
        });
    if arena.parent_of(child_root) != Some(parent_root)
        || !exact_transform
        || !property_trees.clips.is_empty()
        || !property_trees.scrolls.is_empty()
    {
        return Err(vec![FrameArtifactFallbackReason::NonEffectProperty(
            child_root,
        )]);
    }
    if !exact_effect {
        return Err(vec![FrameArtifactFallbackReason::InvalidRootEffect(
            child_root,
        )]);
    }

    let mut stack = vec![child_root];
    let mut seen = FxHashSet::default();
    while let Some(key) = stack.pop() {
        if !seen.insert(key) {
            return Err(vec![FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::DuplicateNodeKey(key),
            )]);
        }
        let Some(node) = arena.get(key) else {
            return Err(vec![FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::MissingNode(key),
            )]);
        };
        if node.element.is_deferred_to_root_viewport_render() {
            return Err(vec![FrameArtifactFallbackReason::DeferredBoundary(key)]);
        }
        if !super::frame_plan::sampled_layout_transition_is_exact(node.element.as_ref()) {
            return Err(vec![FrameArtifactFallbackReason::NonEffectProperty(key)]);
        }
        let exact_state = property_trees.states.get(&key).is_some_and(|state| {
            [state.paint, state.descendants]
                .into_iter()
                .all(|properties| {
                    properties.transform == Some(transform)
                        && properties.effect == Some(effect)
                        && properties.clip.is_none()
                        && properties.scroll.is_none()
                })
        });
        if !exact_state {
            return Err(vec![FrameArtifactFallbackReason::NonEffectProperty(key)]);
        }
        stack.extend(node.element.children().iter().copied());
    }

    let policy = FrameArtifactAuthorityPolicy::RootOpacityGroup(RootOpacityGroupPlan {
        root: child_root,
        effect,
    });
    match record_frame_artifact_with_policy(
        arena,
        &[child_root],
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        policy,
        Some(super::ConsumedAncestorProperty::Transform(witness)),
        None,
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => Ok(artifact),
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            Err(eligibility.reasons)
        }
        Err(error) => Err(error.reasons),
    }
}

#[derive(Clone, Debug)]
pub(super) enum RecordedTransformSurfaceStep {
    Artifact(PaintArtifact),
    Boundary(super::PlannedBoundary),
}

/// Planning-only ordered recorder for a transform surface with typed nested
/// cutouts. Metadata and full passes receive the identical cutout set; a
/// boundary marker flushes the current owning artifact and stops traversal
/// into the nested surface subtree.
pub(super) fn record_transform_surface_steps_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintTransformSurfaceWitness,
    paint_offset: [f32; 2],
    planned_boundary_cutouts: &super::PlannedBoundaryCutoutSet,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    record_ordered_property_steps_for_plan(
        arena,
        roots,
        property_trees,
        paint_generations,
        paint_offset,
        planned_boundary_cutouts,
        Some(witness),
        None,
        None,
        PaintOpacityAuthority::Baked,
        FrameArtifactAuthorityPolicy::TransformSurface(witness),
        None,
    )
}

pub(super) fn record_property_scene_steps_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    paint_offset: [f32; 2],
    planned_boundary_cutouts: &super::PlannedBoundaryCutoutSet,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    record_ordered_property_steps_for_plan(
        arena,
        roots,
        property_trees,
        paint_generations,
        paint_offset,
        planned_boundary_cutouts,
        None,
        None,
        None,
        PaintOpacityAuthority::Baked,
        FrameArtifactAuthorityPolicy::PropertyScene,
        None,
    )
}

/// Records only the native scroll host phases around a detached content
/// marker. The child subtree is intentionally never visited: its resident is
/// recorded by the paired scroll-content authority. Rounded child-mask begin
/// and end chunks therefore remain in this sequence and span the marker.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_frame_root_scroll_host_steps_for_plan(
    arena: &NodeArena,
    boundary_root: NodeKey,
    content_root: NodeKey,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scroll: crate::view::compositor::property_tree::ScrollNodeSnapshot,
    contents_clip: crate::view::compositor::property_tree::ClipNodeSnapshot,
    paint_offset: [f32; 2],
    content_marker: super::PlannedBoundary,
    consumed_transform: Option<super::ConsumedAncestorTransformWitness>,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    let invalid = || vec![FrameArtifactFallbackReason::PropertyBoundary(boundary_root)];
    let node = arena.get(boundary_root).ok_or_else(invalid)?;
    if arena.parent_of(content_root) != Some(boundary_root)
        || node.element.children() != [content_root]
        || scroll.owner != boundary_root
        || scroll.id.0 != boundary_root
        || contents_clip.owner != boundary_root
        || contents_clip.id.owner != boundary_root
        || contents_clip.id.role
            != crate::view::compositor::property_tree::ClipNodeRole::ContentsClip
        || scroll.contents_clip
            != crate::view::base_component::ScrollContentsClipWitness::ExactRect(
                contents_clip.logical_scissor,
            )
        || content_marker.root != boundary_root
        || content_marker.stable_id != node.element.stable_id()
        || content_marker.kind != super::PlannedBoundaryKind::Scroll(scroll.id)
    {
        return Err(invalid());
    }
    let state = property_trees
        .node_state_for(boundary_root)
        .ok_or_else(invalid)?;
    let expected_contents = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(contents_clip.id),
        scroll: Some(scroll.id),
        ..Default::default()
    };
    let expected_paint = consumed_transform
        .map(
            |transform| crate::view::compositor::property_tree::PropertyTreeState {
                transform: Some(transform.transform),
                ..Default::default()
            },
        )
        .unwrap_or_default();
    let expected_live_contents = crate::view::compositor::property_tree::PropertyTreeState {
        transform: expected_paint.transform,
        ..expected_contents
    };
    if consumed_transform.is_some_and(|transform| {
        transform.parent_boundary != transform.transform.0
            || transform.child_boundary != boundary_root
    }) || !state.paint.legacy_boundary_eq(expected_paint)
        || !state.descendants.legacy_boundary_eq(expected_live_contents)
    {
        return Err(invalid());
    }
    let generation = paint_generations
        .local_generations_for(boundary_root)
        .ok_or_else(invalid)?;
    let revision = super::PaintContentRevision {
        self_paint_revision: generation.self_paint_revision,
        composite_revision: generation.composite_revision,
        topology_revision: generation.topology_revision,
    };
    let baked = super::PaintBakedScrollHostWitness::new(
        boundary_root,
        content_root,
        scroll,
        contents_clip.id,
    )
    .ok_or_else(invalid)?;
    let mut recording_context =
        node.element
            .shadow_paint_recording_context(super::PaintRecordingContext {
                paint_offset,
                opacity_authority: super::PaintOpacityAuthority::Baked,
                ..Default::default()
            });
    recording_context.recording_owner = Some(boundary_root);
    recording_context.recording_owner_stable_id = Some(node.element.stable_id());
    recording_context.baked_scroll_host = Some(baked.for_target(boundary_root));
    recording_context.consumed_ancestor_property = consumed_transform.map(|transform| {
        super::ConsumedAncestorProperty::Transform(transform.for_target(boundary_root))
    });
    recording_context.frame_root_scroll_host_child_mask = true;
    recording_context.opacity_authority = super::PaintOpacityAuthority::Baked;

    let metadata = node
        .element
        .record_shadow_paint_metadata_plan(
            boundary_root,
            Default::default(),
            Default::default(),
            revision,
            arena,
            recording_context,
        )
        .ok_or_else(invalid)?;
    let artifacts = node
        .element
        .record_shadow_paint_artifact_plan(
            boundary_root,
            Default::default(),
            Default::default(),
            revision,
            arena,
            recording_context,
        )
        .ok_or_else(invalid)?;
    let metadata_matches = |metadata: &super::PaintChunkMetadata,
                            artifact: &super::PaintArtifact| {
        let [chunk] = artifact.chunks.as_slice() else {
            return false;
        };
        artifact.target == super::PaintArtifactTarget::CurrentTarget
            && artifact.clip_nodes.is_empty()
            && artifact.effect_nodes.is_empty()
            && artifact.owner_nodes.as_slice()
                == [super::PaintOwnerSnapshot {
                    owner: boundary_root,
                    parent: None,
                }]
            && chunk.id == metadata.id
            && chunk.owner == metadata.owner
            && [
                chunk.bounds.x,
                chunk.bounds.y,
                chunk.bounds.width,
                chunk.bounds.height,
            ]
            .map(f32::to_bits)
                == [
                    metadata.bounds.x,
                    metadata.bounds.y,
                    metadata.bounds.width,
                    metadata.bounds.height,
                ]
                .map(f32::to_bits)
            && chunk.properties == metadata.properties
            && chunk.content_revision == metadata.content_revision
            && chunk.payload_identity == metadata.payload_identity
            && chunk.op_range == (0..artifact.ops.len())
    };
    if metadata.before_children.len() != artifacts.before_children.len()
        || metadata.after_children.len() != artifacts.after_children.len()
        || !metadata
            .before_children
            .iter()
            .zip(&artifacts.before_children)
            .all(|(metadata, artifact)| metadata_matches(metadata, artifact))
        || !metadata
            .after_children
            .iter()
            .zip(&artifacts.after_children)
            .all(|(metadata, artifact)| metadata_matches(metadata, artifact))
    {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            super::PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    if artifacts.before_children.is_empty()
        || artifacts.after_children.last().is_none_or(|artifact| {
            artifact.chunks.first().is_none_or(|chunk| {
                chunk.owner != boundary_root
                    || chunk.id.phase != super::PaintNodePhase::AfterChildren
                    || chunk.id.role != super::PaintChunkRole::ScrollbarOverlay
            })
        })
    {
        return Err(invalid());
    }
    let mut steps = artifacts
        .before_children
        .into_iter()
        .map(RecordedTransformSurfaceStep::Artifact)
        .collect::<Vec<_>>();
    steps.push(RecordedTransformSurfaceStep::Boundary(content_marker));
    steps.extend(
        artifacts
            .after_children
            .into_iter()
            .map(RecordedTransformSurfaceStep::Artifact),
    );
    Ok(steps)
}

pub(super) fn record_transform_property_surface_steps_for_plan(
    arena: &NodeArena,
    root: NodeKey,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintTransformSurfaceWitness,
    paint_offset: [f32; 2],
    planned_boundary_cutouts: &super::PlannedBoundaryCutoutSet,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    record_ordered_property_steps_for_plan(
        arena,
        &[root],
        property_trees,
        paint_generations,
        paint_offset,
        planned_boundary_cutouts,
        Some(witness),
        None,
        None,
        PaintOpacityAuthority::Baked,
        FrameArtifactAuthorityPolicy::TransformPropertySurface(witness),
        None,
    )
}

/// Records a transform surface nested below one already-separated effect.
/// The transform remains the raster owner while the exact ancestor effect is
/// projected out, so opacity can be applied once by the outer E composite.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_effect_transform_property_surface_steps_for_plan(
    arena: &NodeArena,
    root: NodeKey,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintTransformSurfaceWitness,
    paint_offset: [f32; 2],
    planned_boundary_cutouts: &super::PlannedBoundaryCutoutSet,
    consumed_effect: super::ConsumedAncestorEffectWitness,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    record_ordered_property_steps_for_plan(
        arena,
        &[root],
        property_trees,
        paint_generations,
        paint_offset,
        planned_boundary_cutouts,
        Some(witness),
        None,
        Some(super::ConsumedAncestorProperty::Effect(consumed_effect)),
        PaintOpacityAuthority::NeutralRootEffect(consumed_effect.effect.id),
        FrameArtifactAuthorityPolicy::TransformPropertySurface(witness),
        None,
    )
}

/// S1 recorder for the scroll host around one direct transformed-content
/// cutout.  The result must be exactly host-before, one typed transform
/// marker, then overlay-after; the transform subtree is never traversed here.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_scroll_transform_host_steps_for_plan(
    arena: &NodeArena,
    scroll_root: NodeKey,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintBakedScrollHostWitness,
    paint_offset: [f32; 2],
    transform_cutout: super::PlannedBoundary,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    if witness.boundary_root() != scroll_root
        || transform_cutout.root != witness.child()
        || !matches!(
            transform_cutout.kind,
            super::PlannedBoundaryKind::Transform(transform)
                if transform == crate::view::compositor::property_tree::TransformNodeId(transform_cutout.root)
        )
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            transform_cutout.root,
        )]);
    }
    let cutouts =
        super::PlannedBoundaryCutoutSet::from_iter([(transform_cutout.root, transform_cutout)]);
    let steps = record_ordered_property_steps_for_plan(
        arena,
        &[scroll_root],
        property_trees,
        paint_generations,
        paint_offset,
        &cutouts,
        None,
        None,
        None,
        PaintOpacityAuthority::Baked,
        FrameArtifactAuthorityPolicy::ScrollTransformHost(witness, transform_cutout),
        None,
    )?;
    let [
        RecordedTransformSurfaceStep::Artifact(host_before),
        RecordedTransformSurfaceStep::Boundary(marker),
        RecordedTransformSurfaceStep::Artifact(overlay_after),
    ] = steps.as_slice()
    else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            transform_cutout.root,
        )]);
    };
    (!host_before.chunks.is_empty()
        && *marker == transform_cutout
        && !overlay_after.chunks.is_empty())
    .then_some(steps)
    .ok_or_else(|| {
        vec![FrameArtifactFallbackReason::PropertyBoundary(
            transform_cutout.root,
        )]
    })
}

#[derive(Clone, Debug)]
pub(super) enum RecordedNativeScrollHostStep {
    Artifact(PaintArtifact),
    ContentReceiver(NativeScrollContentReceiverCutout),
}

/// Records one forest boundary's host around a typed, non-leaf-limited
/// content receiver. A nested boundary consumes only its parent scroll edge;
/// the boundary's own S/C pair remains the receiver's authority.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_native_scroll_forest_host_steps_for_plan(
    arena: &NodeArena,
    boundary_root: NodeKey,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    host: PaintBakedScrollHostWitness,
    edge: super::PaintScrollForestEdgeWitness,
    consumed_parent: Option<super::ConsumedAncestorScrollContentsWitness>,
    content_stable_id: u64,
) -> Result<Vec<RecordedNativeScrollHostStep>, Vec<FrameArtifactFallbackReason>> {
    if host.boundary_root() != boundary_root
        || host.child() != edge.content_root()
        || host.scroll() != edge.scroll_snapshot().id
        || host.contents_clip() != edge.contents_clip_snapshot().id
        || edge.boundary_root() != boundary_root
        || content_stable_id == 0
        || consumed_parent.is_some_and(|parent| parent.target_owner != boundary_root)
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            boundary_root,
        )]);
    }
    let receiver = NativeScrollContentReceiverCutout {
        stable_id: content_stable_id,
        witness: edge,
    };
    let context = PaintRecordingContext {
        baked_scroll_host: Some(host),
        scroll_forest_host: Some(edge),
        frame_root_scroll_host_child_mask: true,
        ..PaintRecordingContext::default()
    };
    let record = |mode| {
        record_retained_coverage_manifest_with_native_scroll_receiver(
            arena,
            &[boundary_root],
            mode,
            property_trees,
            paint_generations,
            context,
            receiver,
        )
    };
    let metadata = record(CoverageRecordingMode::MetadataOnly);
    let full = record(CoverageRecordingMode::FullArtifact);
    if !metadata.validation_errors.is_empty()
        || !full.validation_errors.is_empty()
        || !canonical_manifest_matches(&metadata, &full)
    {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    let exact = |manifest: &super::PaintCoverageManifest| {
        let mut receiver_count = 0usize;
        manifest.items.iter().all(|item| match item {
            PaintCoverageItem::ArtifactChunk { chunk, .. } => {
                chunk.owner == boundary_root
                    && chunk.properties.legacy_boundary_dimensions() == Default::default()
            }
            PaintCoverageItem::TransparentNode {
                owner, properties, ..
            }
            | PaintCoverageItem::CulledSubtree {
                owner, properties, ..
            } => {
                *owner == boundary_root
                    && properties.legacy_boundary_dimensions() == Default::default()
            }
            PaintCoverageItem::NativeScrollContentReceiver { cutout, .. } => {
                receiver_count += 1;
                *cutout == receiver
            }
            _ => false,
        }) && receiver_count == 1
    };
    if !exact(&metadata) || !exact(&full) {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            boundary_root,
        )]);
    }
    let receiver_index = full
        .items
        .iter()
        .position(|item| matches!(item, PaintCoverageItem::NativeScrollContentReceiver { .. }))
        .expect("exact native scroll host has one receiver");
    let mut before = full.clone();
    before.items.truncate(receiver_index);
    let mut after = full;
    after.items.drain(..=receiver_index);
    let before_steps = materialize_transform_surface_steps(before)?;
    let [RecordedTransformSurfaceStep::Artifact(host_before)] = before_steps.as_slice() else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            boundary_root,
        )]);
    };
    let after_steps = materialize_transform_surface_steps(after)?;
    let [RecordedTransformSurfaceStep::Artifact(overlay_after)] = after_steps.as_slice() else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            boundary_root,
        )]);
    };
    Ok(vec![
        RecordedNativeScrollHostStep::Artifact(host_before.clone()),
        RecordedNativeScrollHostStep::ContentReceiver(receiver),
        RecordedNativeScrollHostStep::Artifact(overlay_after.clone()),
    ])
}

/// Records the detached content program for one forest edge. Immediate child
/// scroll boundaries become exact ordered markers; neutral wrapper artifacts
/// before, between, and after sibling markers remain in the same program.
pub(super) fn record_native_scroll_forest_content_steps_for_plan(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    edge: super::PaintScrollForestEdgeWitness,
    child_cutouts: &[super::PlannedBoundary],
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    let cutouts = super::PlannedBoundaryCutoutSet::from_iter(
        child_cutouts
            .iter()
            .copied()
            .map(|cutout| (cutout.root, cutout)),
    );
    if cutouts.len() != child_cutouts.len()
        || child_cutouts.iter().any(|cutout| {
            !matches!(cutout.kind, super::PlannedBoundaryKind::Scroll(scroll) if scroll.0 == cutout.root)
        })
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            edge.content_root(),
        )]);
    }
    let steps = record_ordered_property_steps_for_plan(
        arena,
        &[edge.content_root()],
        property_trees,
        paint_generations,
        edge.normalization_paint_offset(),
        &cutouts,
        None,
        None,
        Some(edge.consumed_property()),
        PaintOpacityAuthority::Baked,
        FrameArtifactAuthorityPolicy::NativeScrollForestContent(edge),
        Some(edge.normalization_paint_offset().map(f32::to_bits)),
    )?;
    let actual = steps
        .iter()
        .filter_map(|step| match step {
            RecordedTransformSurfaceStep::Boundary(boundary) => Some(*boundary),
            RecordedTransformSurfaceStep::Artifact(_) => None,
        })
        .collect::<Vec<_>>();
    (actual == child_cutouts).then_some(steps).ok_or_else(|| {
        vec![FrameArtifactFallbackReason::PropertyBoundary(
            edge.content_root(),
        )]
    })
}

/// S1 offset-zero recorder for the direct transformed scroll content.  The
/// transform remains the surface authority while the inherited scroll and
/// contents clip are consumed atomically by the typed ancestor witness.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_scroll_transform_content_steps_for_plan(
    arena: &NodeArena,
    transform_content: NodeKey,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    transform_witness: PaintTransformSurfaceWitness,
    content_witness: PaintScrollContentWitness,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    if transform_witness.boundary_owner != transform_content
        || transform_witness.target_owner != transform_content
        || transform_witness.transform.0 != transform_content
        || content_witness.content_root() != transform_content
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            transform_content,
        )]);
    }
    let Some(content_node) = arena.get(transform_content) else {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::MissingNode(transform_content),
        )]);
    };
    let Some(content_element) = content_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
    else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            transform_content,
        )]);
    };
    let normalization = content_witness.normalization_paint_offset();
    let Some(required_paint_offset) =
        content_element.exact_retained_scroll_transform_content_recording_offset(normalization)
    else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            transform_content,
        )]);
    };
    let steps = record_ordered_property_steps_for_plan(
        arena,
        &[transform_content],
        property_trees,
        paint_generations,
        normalization,
        &super::PlannedBoundaryCutoutSet::default(),
        Some(transform_witness),
        None,
        Some(content_witness.consumed_property()),
        PaintOpacityAuthority::Baked,
        FrameArtifactAuthorityPolicy::TransformPropertySurface(transform_witness),
        Some(required_paint_offset.map(f32::to_bits)),
    )?;
    let [RecordedTransformSurfaceStep::Artifact(artifact)] = steps.as_slice() else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            transform_content,
        )]);
    };
    (!artifact.chunks.is_empty()
        && artifact.clip_nodes.is_empty()
        && artifact.effect_nodes.is_empty())
    .then_some(steps)
    .ok_or_else(|| {
        vec![FrameArtifactFallbackReason::PropertyBoundary(
            transform_content,
        )]
    })
}

/// B4-2A receiver recorder.  The scroll host is a typed cutout in both the
/// metadata and full passes; this function deliberately returns only the
/// receiver's surrounding artifacts plus the exact insertion marker.  It
/// does not record or bake the scroll subtree.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_property_scroll_receiver_steps_for_plan(
    arena: &NodeArena,
    receiver_root: NodeKey,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintTransformSurfaceWitness,
    paint_offset: [f32; 2],
    scroll_cutout: super::PlannedBoundary,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    if scroll_cutout.root == receiver_root
        || !matches!(scroll_cutout.kind, super::PlannedBoundaryKind::Scroll(_))
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            scroll_cutout.root,
        )]);
    }
    let cutouts = super::PlannedBoundaryCutoutSet::from_iter([(scroll_cutout.root, scroll_cutout)]);
    let steps = record_ordered_property_steps_for_plan(
        arena,
        &[receiver_root],
        property_trees,
        paint_generations,
        paint_offset,
        &cutouts,
        Some(witness),
        None,
        None,
        PaintOpacityAuthority::Baked,
        FrameArtifactAuthorityPolicy::TransformPropertySurface(witness),
        None,
    )?;
    let markers = steps
        .iter()
        .filter(|step| {
            matches!(step, RecordedTransformSurfaceStep::Boundary(boundary) if *boundary == scroll_cutout)
        })
        .count();
    (markers == 1).then_some(steps).ok_or_else(|| {
        vec![FrameArtifactFallbackReason::PropertyBoundary(
            scroll_cutout.root,
        )]
    })
}

/// Seals the receiver program for a native node that owns both T and S.
///
/// This recorder intentionally emits no artifact span: owner self paint is
/// delegated to the scroll H/O recorder, descendants to the offset-zero C
/// recorder, and the outer T target contains exactly one typed S insertion.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_same_owner_transform_scroll_receiver_steps_for_plan(
    arena: &NodeArena,
    receiver_root: NodeKey,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    transform: TransformNodeSnapshot,
    scroll: ScrollNodeSnapshot,
    contents_clip: ClipNodeSnapshot,
    scroll_cutout: super::PlannedBoundary,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    let invalid = || vec![FrameArtifactFallbackReason::PropertyBoundary(receiver_root)];
    let node = arena.get(receiver_root).ok_or_else(invalid)?;
    let [content_root] = node.element.children() else {
        return Err(invalid());
    };
    let state = property_trees
        .node_state_for(receiver_root)
        .ok_or_else(invalid)?;
    let generations = paint_generations
        .local_generations_for(receiver_root)
        .ok_or_else(invalid)?;
    if transform.owner != receiver_root
        || transform.id.0 != receiver_root
        || transform.parent.is_some()
        || transform.generation == 0
        || scroll.owner != receiver_root
        || scroll.id.0 != receiver_root
        || scroll.parent.is_some()
        || scroll.generation == 0
        || contents_clip.owner != receiver_root
        || contents_clip.id.owner != receiver_root
        || contents_clip.id.role != ClipNodeRole::ContentsClip
        || contents_clip.generation == 0
        || scroll_cutout.root != receiver_root
        || scroll_cutout.stable_id != node.element.stable_id()
        || scroll_cutout.kind != super::PlannedBoundaryKind::Scroll(scroll.id)
        || state.paint.transform != Some(transform.id)
        || state.paint.effect.is_some()
        || state.paint.scroll.is_some()
        || state.descendants.transform != Some(transform.id)
        || state.descendants.scroll != Some(scroll.id)
        || state.descendants.clip != Some(contents_clip.id)
        || generations.topology_revision == 0
        || node.element.is_deferred_to_root_viewport_render()
        || node
            .element
            .retained_scroll_normalized_paint_capability()
            .is_none()
        || arena.parent_of(*content_root) != Some(receiver_root)
    {
        return Err(invalid());
    }
    Ok(vec![RecordedTransformSurfaceStep::Boundary(scroll_cutout)])
}

/// B4-2C checkpoint recorder for one direct `Effect -> ScrollContents`
/// receiver. The owning effect is neutralized by the supplied effect
/// contract, while the scroll host is emitted as one typed insertion marker.
/// Consequently neither the detached content nor the live scroll offset can
/// enter the effect receiver artifact identity.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_property_effect_scroll_receiver_steps_for_plan(
    arena: &NodeArena,
    receiver_root: NodeKey,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    contract: &EffectPropertySurfaceArtifactContract,
    paint_offset: [f32; 2],
    scroll_cutout: super::PlannedBoundary,
    consumed_transform: Option<super::ConsumedAncestorTransformWitness>,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    if contract.boundary_root() != receiver_root
        || scroll_cutout.root == receiver_root
        || !matches!(scroll_cutout.kind, super::PlannedBoundaryKind::Scroll(_))
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            scroll_cutout.root,
        )]);
    }
    let cutouts = super::PlannedBoundaryCutoutSet::from_iter([(scroll_cutout.root, scroll_cutout)]);
    let steps = record_effect_property_surface_steps_for_plan(
        arena,
        property_trees,
        paint_generations,
        contract,
        paint_offset,
        &cutouts,
        consumed_transform,
    )?;
    let markers = steps
        .iter()
        .filter(|step| {
            matches!(step, RecordedTransformSurfaceStep::Boundary(boundary) if *boundary == scroll_cutout)
        })
        .count();
    (markers == 1).then_some(steps).ok_or_else(|| {
        vec![FrameArtifactFallbackReason::PropertyBoundary(
            scroll_cutout.root,
        )]
    })
}

/// Seals the receiver program for a native node that owns both E and S.
/// H/C/O owns all self and descendant paint; this effect-neutral receiver
/// therefore contains exactly one typed S insertion and applies opacity only
/// when the assembled target is composited.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_same_owner_effect_scroll_receiver_steps_for_plan(
    arena: &NodeArena,
    receiver_root: NodeKey,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    contract: &EffectPropertySurfaceArtifactContract,
    effect: EffectNodeSnapshot,
    scroll: ScrollNodeSnapshot,
    contents_clip: ClipNodeSnapshot,
    scroll_cutout: super::PlannedBoundary,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    let invalid = || vec![FrameArtifactFallbackReason::PropertyBoundary(receiver_root)];
    let node = arena.get(receiver_root).ok_or_else(invalid)?;
    let [content_root] = node.element.children() else {
        return Err(invalid());
    };
    let state = property_trees
        .node_state_for(receiver_root)
        .ok_or_else(invalid)?;
    let generations = paint_generations
        .local_generations_for(receiver_root)
        .ok_or_else(invalid)?;
    if !contract.is_canonical()
        || contract.boundary_root() != receiver_root
        || contract.stable_id() != node.element.stable_id()
        || contract.isolated_leaf() != effect
        || effect.owner != receiver_root
        || effect.id.0 != receiver_root
        || effect.parent.is_some()
        || effect.generation == 0
        || !effect.opacity.is_finite()
        || !(0.0..=1.0).contains(&effect.opacity)
        || scroll.owner != receiver_root
        || scroll.id.0 != receiver_root
        || scroll.parent.is_some()
        || scroll.generation == 0
        || contents_clip.owner != receiver_root
        || contents_clip.id.owner != receiver_root
        || contents_clip.id.role != ClipNodeRole::ContentsClip
        || contents_clip.generation == 0
        || scroll_cutout.root != receiver_root
        || scroll_cutout.stable_id != node.element.stable_id()
        || scroll_cutout.kind != super::PlannedBoundaryKind::Scroll(scroll.id)
        || state.paint.transform.is_some()
        || state.paint.effect != Some(effect.id)
        || state.paint.scroll.is_some()
        || state.descendants.transform.is_some()
        || state.descendants.effect != Some(effect.id)
        || state.descendants.scroll != Some(scroll.id)
        || state.descendants.clip != Some(contents_clip.id)
        || generations.topology_revision == 0
        || node.element.is_deferred_to_root_viewport_render()
        || node
            .element
            .retained_scroll_normalized_paint_capability()
            .is_none()
        || arena.parent_of(*content_root) != Some(receiver_root)
    {
        return Err(invalid());
    }
    Ok(vec![RecordedTransformSurfaceStep::Boundary(scroll_cutout)])
}

/// Outer marker-only receiver for one native owner carrying T+E+S.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_same_owner_transform_effect_scroll_outer_steps_for_plan(
    arena: &NodeArena,
    owner: NodeKey,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    transform: TransformNodeSnapshot,
    effect: EffectNodeSnapshot,
    scroll: ScrollNodeSnapshot,
    contents_clip: ClipNodeSnapshot,
    effect_cutout: super::PlannedBoundary,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    let invalid = || vec![FrameArtifactFallbackReason::PropertyBoundary(owner)];
    let node = arena.get(owner).ok_or_else(invalid)?;
    let [content_root] = node.element.children() else {
        return Err(invalid());
    };
    let state = property_trees.node_state_for(owner).ok_or_else(invalid)?;
    let generations = paint_generations
        .local_generations_for(owner)
        .ok_or_else(invalid)?;
    if transform.owner != owner
        || transform.id.0 != owner
        || transform.parent.is_some()
        || transform.generation == 0
        || effect.owner != owner
        || effect.id.0 != owner
        || effect.parent.is_some()
        || effect.generation == 0
        || scroll.owner != owner
        || scroll.id.0 != owner
        || scroll.parent.is_some()
        || scroll.generation == 0
        || contents_clip.owner != owner
        || contents_clip.id.owner != owner
        || contents_clip.id.role != ClipNodeRole::ContentsClip
        || contents_clip.generation == 0
        || effect_cutout.root != owner
        || effect_cutout.stable_id != node.element.stable_id()
        || effect_cutout.kind != super::PlannedBoundaryKind::Isolation(effect.id)
        || state.paint.transform != Some(transform.id)
        || state.paint.effect != Some(effect.id)
        || state.paint.scroll.is_some()
        || state.descendants.transform != Some(transform.id)
        || state.descendants.effect != Some(effect.id)
        || state.descendants.scroll != Some(scroll.id)
        || state.descendants.clip != Some(contents_clip.id)
        || generations.topology_revision == 0
        || arena.parent_of(*content_root) != Some(owner)
    {
        return Err(invalid());
    }
    Ok(vec![RecordedTransformSurfaceStep::Boundary(
        effect_cutout,
    )])
}

/// Inner marker-only E receiver paired with the outer same-owner T receiver.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_same_owner_transform_effect_scroll_effect_steps_for_plan(
    arena: &NodeArena,
    owner: NodeKey,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    transform: TransformNodeSnapshot,
    effect: EffectNodeSnapshot,
    scroll: ScrollNodeSnapshot,
    contents_clip: ClipNodeSnapshot,
    scroll_cutout: super::PlannedBoundary,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    let invalid = || vec![FrameArtifactFallbackReason::PropertyBoundary(owner)];
    let node = arena.get(owner).ok_or_else(invalid)?;
    let [content_root] = node.element.children() else {
        return Err(invalid());
    };
    let state = property_trees.node_state_for(owner).ok_or_else(invalid)?;
    let generations = paint_generations
        .local_generations_for(owner)
        .ok_or_else(invalid)?;
    if transform.owner != owner
        || transform.id.0 != owner
        || transform.parent.is_some()
        || transform.generation == 0
        || effect.owner != owner
        || effect.id.0 != owner
        || effect.parent.is_some()
        || effect.generation == 0
        || scroll.owner != owner
        || scroll.id.0 != owner
        || scroll.parent.is_some()
        || scroll.generation == 0
        || contents_clip.owner != owner
        || contents_clip.id.owner != owner
        || contents_clip.id.role != ClipNodeRole::ContentsClip
        || contents_clip.generation == 0
        || scroll_cutout.root != owner
        || scroll_cutout.stable_id != node.element.stable_id()
        || scroll_cutout.kind != super::PlannedBoundaryKind::Scroll(scroll.id)
        || state.paint.transform != Some(transform.id)
        || state.paint.effect != Some(effect.id)
        || state.paint.scroll.is_some()
        || state.descendants.transform != Some(transform.id)
        || state.descendants.effect != Some(effect.id)
        || state.descendants.scroll != Some(scroll.id)
        || state.descendants.clip != Some(contents_clip.id)
        || generations.topology_revision == 0
        || arena.parent_of(*content_root) != Some(owner)
    {
        return Err(invalid());
    }
    Ok(vec![RecordedTransformSurfaceStep::Boundary(
        scroll_cutout,
    )])
}

/// Records one canonical effect surface at any property-forest depth. Direct
/// child surfaces are typed cutouts, so this pass never traverses or bakes a
/// descendant isolation. The exact live ancestor effect/clip suffixes are
/// detached by coverage before either artifact span is materialized.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_effect_property_surface_steps_for_plan(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    contract: &EffectPropertySurfaceArtifactContract,
    paint_offset: [f32; 2],
    planned_boundary_cutouts: &super::PlannedBoundaryCutoutSet,
    consumed_transform: Option<super::ConsumedAncestorTransformWitness>,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    if !contract.is_canonical()
        || arena
            .get(contract.boundary_root())
            .is_none_or(|node| node.element.stable_id() != contract.stable_id())
        || property_trees
            .effect_snapshot_for(Some(contract.isolated_leaf().id))
            .as_deref()
            != Some(contract.live_effect_chain())
    {
        return Err(vec![FrameArtifactFallbackReason::InvalidRootEffect(
            contract.boundary_root(),
        )]);
    }
    let consumed = consumed_transform.map(super::ConsumedAncestorProperty::Transform);
    record_ordered_property_steps_for_plan(
        arena,
        &[contract.boundary_root()],
        property_trees,
        paint_generations,
        paint_offset,
        planned_boundary_cutouts,
        None,
        Some(contract),
        consumed,
        PaintOpacityAuthority::NeutralRootEffect(contract.isolated_leaf().id),
        FrameArtifactAuthorityPolicy::EffectPropertySurface(contract.isolated_leaf().id),
        None,
    )
}

/// Records the inner effect half of one sealed same-owner
/// `Transform -> Effect` boundary pair. The transform is projected only by
/// the dedicated same-owner capability; the generic ancestor witness remains
/// unavailable when both boundaries belong to the same node.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_same_owner_transform_effect_surface_steps_for_plan(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    contract: &EffectPropertySurfaceArtifactContract,
    paint_offset: [f32; 2],
    planned_boundary_cutouts: &super::PlannedBoundaryCutoutSet,
    consumed_transform: super::ConsumedSameOwnerTransformBoundaryWitness,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    if !contract.is_canonical()
        || contract.boundary_root() != consumed_transform.owner
        || contract.isolated_leaf().owner != consumed_transform.owner
        || arena
            .get(contract.boundary_root())
            .is_none_or(|node| node.element.stable_id() != contract.stable_id())
        || property_trees
            .effect_snapshot_for(Some(contract.isolated_leaf().id))
            .as_deref()
            != Some(contract.live_effect_chain())
    {
        return Err(vec![FrameArtifactFallbackReason::InvalidRootEffect(
            contract.boundary_root(),
        )]);
    }
    record_ordered_property_steps_for_plan(
        arena,
        &[contract.boundary_root()],
        property_trees,
        paint_generations,
        paint_offset,
        planned_boundary_cutouts,
        Some(PaintTransformSurfaceWitness::canonical_root(
            contract.boundary_root(),
        )),
        Some(contract),
        Some(super::ConsumedAncestorProperty::SameOwnerTransformBoundary(
            consumed_transform,
        )),
        PaintOpacityAuthority::NeutralRootEffect(contract.isolated_leaf().id),
        FrameArtifactAuthorityPolicy::EffectPropertySurface(contract.isolated_leaf().id),
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn record_ordered_property_steps_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    paint_offset: [f32; 2],
    planned_boundary_cutouts: &super::PlannedBoundaryCutoutSet,
    transform_surface_authority: Option<PaintTransformSurfaceWitness>,
    effect_surface_authority: Option<&EffectPropertySurfaceArtifactContract>,
    consumed_ancestor_property: Option<super::ConsumedAncestorProperty>,
    opacity_authority: PaintOpacityAuthority,
    policy: FrameArtifactAuthorityPolicy,
    required_scroll_content_paint_offset_bits: Option<[u32; 2]>,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    record_ordered_property_steps_with_stack_for_plan(
        arena,
        roots,
        property_trees,
        paint_generations,
        paint_offset,
        planned_boundary_cutouts,
        transform_surface_authority,
        effect_surface_authority,
        consumed_ancestor_property,
        None,
        None,
        opacity_authority,
        policy,
        required_scroll_content_paint_offset_bits,
    )
}

#[allow(clippy::too_many_arguments)]
fn record_ordered_property_steps_with_stack_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    paint_offset: [f32; 2],
    planned_boundary_cutouts: &super::PlannedBoundaryCutoutSet,
    transform_surface_authority: Option<PaintTransformSurfaceWitness>,
    effect_surface_authority: Option<&EffectPropertySurfaceArtifactContract>,
    consumed_ancestor_property: Option<super::ConsumedAncestorProperty>,
    consumed_ancestor_property_stack: Option<super::ConsumedAncestorPropertyStackWitness>,
    property_forest_ancestor_chain: Option<&super::ConsumedPropertyForestAncestorChainWitness>,
    opacity_authority: PaintOpacityAuthority,
    policy: FrameArtifactAuthorityPolicy,
    required_scroll_content_paint_offset_bits: Option<[u32; 2]>,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    let context = PaintRecordingContext {
        paint_offset,
        consumed_ancestor_property,
        consumed_ancestor_property_stack,
        opacity_authority,
        required_scroll_content_paint_offset_bits,
        baked_scroll_host: baked_scroll_host_witness(policy),
        ..PaintRecordingContext::default()
    };
    let record = |mode| {
        if let Some(chain) = property_forest_ancestor_chain {
            super::coverage_manifest::record_retained_coverage_manifest_with_property_forest_authorities(
                arena,
                roots,
                false,
                true,
                mode,
                property_trees,
                paint_generations,
                context,
                transform_surface_authority,
                effect_surface_authority,
                chain,
                planned_boundary_cutouts,
            )
        } else {
            record_retained_coverage_manifest_with_property_authorities(
                arena,
                roots,
                false,
                true,
                mode,
                property_trees,
                paint_generations,
                context,
                transform_surface_authority,
                effect_surface_authority,
                planned_boundary_cutouts,
            )
        }
    };
    let metadata = record(CoverageRecordingMode::MetadataOnly);
    let metadata_eligibility = assess_manifest(&metadata, policy);
    if !metadata_eligibility.eligible {
        return Err(metadata_eligibility.reasons);
    }
    let full = record(CoverageRecordingMode::FullArtifact);
    let full_eligibility = assess_manifest(&full, policy);
    if !full_eligibility.eligible {
        return Err(full_eligibility.reasons);
    }
    if !canonical_manifest_matches(&metadata, &full) {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    materialize_transform_surface_steps(full)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn record_property_forest_transform_surface_steps_for_plan(
    arena: &NodeArena,
    root: NodeKey,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintTransformSurfaceWitness,
    paint_offset: [f32; 2],
    planned_boundary_cutouts: &super::PlannedBoundaryCutoutSet,
    ancestor_chain: &super::ConsumedPropertyForestAncestorChainWitness,
    parent_effect: EffectNodeId,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    record_ordered_property_steps_with_stack_for_plan(
        arena,
        &[root],
        property_trees,
        paint_generations,
        paint_offset,
        planned_boundary_cutouts,
        Some(witness),
        None,
        None,
        None,
        Some(ancestor_chain),
        PaintOpacityAuthority::NeutralRootEffect(parent_effect),
        FrameArtifactAuthorityPolicy::TransformPropertySurface(witness),
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn record_property_forest_effect_surface_steps_for_plan(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    contract: &EffectPropertySurfaceArtifactContract,
    paint_offset: [f32; 2],
    planned_boundary_cutouts: &super::PlannedBoundaryCutoutSet,
    ancestor_chain: &super::ConsumedPropertyForestAncestorChainWitness,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    if !contract.is_canonical()
        || arena
            .get(contract.boundary_root())
            .is_none_or(|node| node.element.stable_id() != contract.stable_id())
        || property_trees
            .effect_snapshot_for(Some(contract.isolated_leaf().id))
            .as_deref()
            != Some(contract.live_effect_chain())
    {
        return Err(vec![FrameArtifactFallbackReason::InvalidRootEffect(
            contract.boundary_root(),
        )]);
    }
    record_ordered_property_steps_with_stack_for_plan(
        arena,
        &[contract.boundary_root()],
        property_trees,
        paint_generations,
        paint_offset,
        planned_boundary_cutouts,
        None,
        Some(contract),
        None,
        None,
        Some(ancestor_chain),
        PaintOpacityAuthority::NeutralRootEffect(contract.isolated_leaf().id),
        FrameArtifactAuthorityPolicy::EffectPropertySurface(contract.isolated_leaf().id),
        None,
    )
}

/// Records one detached scroll-content receiver with exactly one descendant
/// effect cutout. ScrollContents is projected from both spans while the live
/// offset is represented only by `witness.normalization_paint_offset()`.
pub(super) fn record_scroll_content_effect_receiver_steps_for_plan(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintScrollContentWitness,
    effect_cutout: super::PlannedBoundary,
    consumed_transform: Option<super::ConsumedAncestorTransformWitness>,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    let content_root = witness.content_root();
    if effect_cutout.root == content_root
        || !matches!(effect_cutout.kind, super::PlannedBoundaryKind::Isolation(_))
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            effect_cutout.root,
        )]);
    }
    let content_node = arena.get(content_root).ok_or_else(|| {
        vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::MissingNode(content_root),
        )]
    })?;
    let content_element = content_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
        .ok_or_else(|| vec![FrameArtifactFallbackReason::PropertyBoundary(content_root)])?;
    let required_paint_offset = content_element
        .exact_retained_scroll_content_wrapper_recording_offset(
            witness.normalization_paint_offset(),
        )
        .ok_or_else(|| vec![FrameArtifactFallbackReason::PropertyBoundary(content_root)])?;
    let consumed_entries = consumed_transform
        .map(|transform| {
            vec![
                super::ConsumedAncestorProperty::Transform(transform),
                witness.consumed_property(),
            ]
        })
        .unwrap_or_else(|| vec![witness.consumed_property()]);
    let consumed_stack =
        super::ConsumedAncestorPropertyStackWitness::new(content_root, &consumed_entries)
            .ok_or_else(|| vec![FrameArtifactFallbackReason::PropertyBoundary(content_root)])?;
    let cutouts = super::PlannedBoundaryCutoutSet::from_iter([(effect_cutout.root, effect_cutout)]);
    let steps = record_ordered_property_steps_with_stack_for_plan(
        arena,
        &[content_root],
        property_trees,
        paint_generations,
        witness.normalization_paint_offset(),
        &cutouts,
        None,
        None,
        None,
        Some(consumed_stack),
        None,
        PaintOpacityAuthority::Baked,
        FrameArtifactAuthorityPolicy::ScrollContentEffectReceiver(witness, effect_cutout),
        Some(required_paint_offset.map(f32::to_bits)),
    )?;
    let marker_count = steps.iter().filter(|step| {
        matches!(step, RecordedTransformSurfaceStep::Boundary(marker) if *marker == effect_cutout)
    }).count();
    (marker_count == 1).then_some(steps).ok_or_else(|| {
        vec![FrameArtifactFallbackReason::PropertyBoundary(
            effect_cutout.root,
        )]
    })
}

/// Records the isolated descendant E surface in offset-zero scroll-content
/// space. The scroll/clip pair is projected but its offset never enters the E
/// artifact identity; opacity remains the effect surface's composite input.
pub(super) fn record_scroll_content_effect_surface_steps_for_plan(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintScrollContentWitness,
    contract: &EffectPropertySurfaceArtifactContract,
    consumed_transform: Option<super::ConsumedAncestorTransformWitness>,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    let effect_root = contract.boundary_root();
    let content_root = witness.content_root();
    let required_paint_offset = arena
        .get(content_root)
        .and_then(|node| {
            node.element
                .as_any()
                .downcast_ref::<crate::view::base_component::Element>()
                .and_then(|element| {
                    element.exact_retained_scroll_content_wrapper_recording_offset(
                        witness.normalization_paint_offset(),
                    )
                })
        })
        .ok_or_else(|| vec![FrameArtifactFallbackReason::PropertyBoundary(content_root)])?;
    let scroll_property = witness.consumed_property().for_target(effect_root);
    let consumed_entries = consumed_transform
        .map(|transform| {
            vec![
                super::ConsumedAncestorProperty::Transform(transform),
                scroll_property,
            ]
        })
        .unwrap_or_else(|| vec![scroll_property]);
    let consumed_stack =
        super::ConsumedAncestorPropertyStackWitness::new(effect_root, &consumed_entries)
            .ok_or_else(|| vec![FrameArtifactFallbackReason::PropertyBoundary(effect_root)])?;
    record_ordered_property_steps_with_stack_for_plan(
        arena,
        &[effect_root],
        property_trees,
        paint_generations,
        witness.normalization_paint_offset(),
        &super::PlannedBoundaryCutoutSet::default(),
        None,
        Some(contract),
        None,
        Some(consumed_stack),
        None,
        PaintOpacityAuthority::NeutralRootEffect(contract.isolated_leaf().id),
        FrameArtifactAuthorityPolicy::EffectPropertySurface(contract.isolated_leaf().id),
        Some(required_paint_offset.map(f32::to_bits)),
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
    RootOpacityGroup(RootOpacityGroupPlan),
    TransformSurface(PaintTransformSurfaceWitness),
    TransformPropertySurface(PaintTransformSurfaceWitness),
    EffectPropertySurface(EffectNodeId),
    BakedScrollHost(PaintBakedScrollHostWitness),
    ScrollTransformHost(PaintBakedScrollHostWitness, super::PlannedBoundary),
    ScrollContentLocal(PaintScrollContentWitness),
    ScrollContentEffectReceiver(PaintScrollContentWitness, super::PlannedBoundary),
    NativeScrollForestContent(super::PaintScrollForestEdgeWitness),
}

fn baked_scroll_host_witness(
    policy: FrameArtifactAuthorityPolicy,
) -> Option<PaintBakedScrollHostWitness> {
    match policy {
        FrameArtifactAuthorityPolicy::BakedScrollHost(witness)
        | FrameArtifactAuthorityPolicy::ScrollTransformHost(witness, _) => Some(witness),
        _ => None,
    }
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
        for snapshot in property_trees
            .transform_snapshot_chain_for(state.transform)
            .ok_or_else(invalid)?
        {
            match merge_snapshot(&mut transforms, snapshot.id, snapshot) {
                SnapshotMerge::Inserted => artifact.transform_nodes.push(snapshot),
                SnapshotMerge::Identical => {}
                SnapshotMerge::Conflict => return Err(invalid()),
            }
        }
        for snapshot in property_trees
            .layout_position_snapshot_chain_for(state.layout_position)
            .ok_or_else(invalid)?
        {
            if let SpatialPositionReference::Anchor(anchor) = snapshot.reference {
                anchor_visual_roots.push(VisualOffsetNodeId(anchor));
            }
            if let Some(scroll) = snapshot.reference_scroll {
                for scroll_snapshot in property_trees
                    .scroll_snapshot_chain_for(Some(scroll))
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
        for snapshot in property_trees
            .visual_offset_snapshot_chain_for(state.visual_offset)
            .ok_or_else(invalid)?
        {
            match merge_snapshot(&mut visuals, snapshot.id, snapshot) {
                SnapshotMerge::Inserted => artifact.visual_offset_nodes.push(snapshot),
                SnapshotMerge::Identical => {}
                SnapshotMerge::Conflict => return Err(invalid()),
            }
        }
        for anchor in anchor_visual_roots {
            for snapshot in property_trees
                .visual_offset_snapshot_chain_for(Some(anchor))
                .ok_or_else(invalid)?
            {
                match merge_snapshot(&mut visuals, snapshot.id, snapshot) {
                    SnapshotMerge::Inserted => artifact.visual_offset_nodes.push(snapshot),
                    SnapshotMerge::Identical => {}
                    SnapshotMerge::Conflict => return Err(invalid()),
                }
            }
        }
        for snapshot in property_trees
            .scroll_snapshot_chain_for(state.scroll)
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

fn materialize_transform_surface_steps(
    manifest: super::PaintCoverageManifest,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    struct SpanBuilder {
        artifact: PaintArtifact,
        clips: FxHashMap<
            crate::view::compositor::property_tree::ClipNodeId,
            crate::view::compositor::property_tree::ClipNodeSnapshot,
        >,
        effects: FxHashMap<
            crate::view::compositor::property_tree::EffectNodeId,
            crate::view::compositor::property_tree::EffectNodeSnapshot,
        >,
        owners: FxHashMap<NodeKey, super::PaintOwnerSnapshot>,
        owner_property_states: FxHashMap<NodeKey, super::PaintOwnerPropertyStateSnapshot>,
    }
    impl SpanBuilder {
        fn new() -> Self {
            Self {
                artifact: PaintArtifact {
                    target: PaintArtifactTarget::CurrentTarget,
                    ..PaintArtifact::default()
                },
                clips: FxHashMap::default(),
                effects: FxHashMap::default(),
                owners: FxHashMap::default(),
                owner_property_states: FxHashMap::default(),
            }
        }

        fn flush(&mut self, out: &mut Vec<RecordedTransformSurfaceStep>) {
            if self.artifact.chunks.is_empty() {
                return;
            }
            out.push(RecordedTransformSurfaceStep::Artifact(std::mem::replace(
                &mut self.artifact,
                PaintArtifact {
                    target: PaintArtifactTarget::CurrentTarget,
                    ..PaintArtifact::default()
                },
            )));
            self.clips.clear();
            self.effects.clear();
            self.owners.clear();
            self.owner_property_states.clear();
        }
    }

    let conflict = |error| vec![FrameArtifactFallbackReason::Validation(error)];
    let mut steps = Vec::new();
    let mut span = SpanBuilder::new();
    for item in manifest.items {
        match item {
            PaintCoverageItem::ArtifactChunk {
                chunk,
                clip_snapshot,
                effect_snapshot,
                owner_snapshot,
                owner_property_state_snapshot,
                ops: Some(ops),
                ..
            } => {
                let start = span.artifact.ops.len();
                span.artifact.ops.extend(ops);
                let end = span.artifact.ops.len();
                span.artifact.chunks.push(PaintChunk {
                    id: chunk.id,
                    owner: chunk.owner,
                    op_range: start..end,
                    bounds: chunk.bounds,
                    properties: chunk.properties,
                    content_revision: chunk.content_revision,
                    payload_identity: chunk.payload_identity,
                });
                for snapshot in clip_snapshot {
                    match merge_snapshot(&mut span.clips, snapshot.id, snapshot) {
                        SnapshotMerge::Inserted => span.artifact.clip_nodes.push(snapshot),
                        SnapshotMerge::Identical => {}
                        SnapshotMerge::Conflict => {
                            return Err(conflict(
                                PaintCoverageValidationError::ConflictingClipSnapshot(snapshot.id),
                            ));
                        }
                    }
                }
                for snapshot in effect_snapshot {
                    match merge_snapshot(&mut span.effects, snapshot.id, snapshot) {
                        SnapshotMerge::Inserted => span.artifact.effect_nodes.push(snapshot),
                        SnapshotMerge::Identical => {}
                        SnapshotMerge::Conflict => {
                            return Err(conflict(
                                PaintCoverageValidationError::ConflictingEffectSnapshot(
                                    snapshot.id,
                                ),
                            ));
                        }
                    }
                }
                for snapshot in owner_snapshot {
                    match merge_snapshot(&mut span.owners, snapshot.owner, snapshot) {
                        SnapshotMerge::Inserted => span.artifact.owner_nodes.push(snapshot),
                        SnapshotMerge::Identical => {}
                        SnapshotMerge::Conflict => {
                            return Err(conflict(
                                PaintCoverageValidationError::ConflictingOwnerSnapshot(
                                    snapshot.owner,
                                ),
                            ));
                        }
                    }
                }
                for snapshot in owner_property_state_snapshot {
                    match merge_snapshot(&mut span.owner_property_states, snapshot.owner, snapshot)
                    {
                        SnapshotMerge::Inserted => {
                            span.artifact.owner_property_states.push(snapshot)
                        }
                        SnapshotMerge::Identical => {}
                        SnapshotMerge::Conflict => {
                            return Err(conflict(
                                PaintCoverageValidationError::ConflictingOwnerPropertyState(
                                    snapshot.owner,
                                ),
                            ));
                        }
                    }
                }
            }
            PaintCoverageItem::PlannedBoundary { boundary, .. } => {
                span.flush(&mut steps);
                steps.push(RecordedTransformSurfaceStep::Boundary(boundary));
            }
            PaintCoverageItem::TransparentNode { .. } | PaintCoverageItem::CulledSubtree { .. } => {
            }
            PaintCoverageItem::ArtifactChunk { ops: None, .. }
            | PaintCoverageItem::LegacyBoundary { .. }
            | PaintCoverageItem::NativeScrollContentReceiver { .. } => unreachable!(
                "eligible full transform-surface manifest has only chunks, transparent nodes, culled nodes, and planned boundaries"
            ),
        }
    }
    span.flush(&mut steps);
    Ok(steps)
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
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    if mode == RendererMode::Legacy {
        return Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(
            FrameArtifactEligibility {
                eligible: false,
                reasons: vec![FrameArtifactFallbackReason::RendererLegacy],
                ..FrameArtifactEligibility::default()
            },
        ));
    }
    let initial_recording_context =
        PaintRecordingContext {
            paint_offset: match policy {
                FrameArtifactAuthorityPolicy::ScrollContentLocal(witness)
                | FrameArtifactAuthorityPolicy::ScrollContentEffectReceiver(witness, _) => {
                    witness.normalization_paint_offset()
                }
                _ => [0.0, 0.0],
            },
            consumed_ancestor_property: match policy {
                FrameArtifactAuthorityPolicy::ScrollContentLocal(witness)
                | FrameArtifactAuthorityPolicy::ScrollContentEffectReceiver(witness, _) => {
                    consumed_ancestor_property_stack
                        .is_none()
                        .then(|| witness.consumed_property())
                }
                _ => consumed_ancestor_property,
            },
            consumed_ancestor_property_stack,
            required_scroll_content_paint_offset_bits,
            opacity_authority: if let Some(effect) = neutral_effect_authority {
                PaintOpacityAuthority::NeutralRootEffect(effect)
            } else {
                match policy {
                FrameArtifactAuthorityPolicy::RootOpacityGroup(plan) => {
                    PaintOpacityAuthority::NeutralRootEffect(plan.effect)
                }
                FrameArtifactAuthorityPolicy::EffectPropertySurface(effect) => {
                    PaintOpacityAuthority::NeutralRootEffect(effect)
                }
                FrameArtifactAuthorityPolicy::ExistingBakedProperties
                | FrameArtifactAuthorityPolicy::PropertyNeutral
                | FrameArtifactAuthorityPolicy::ClipEnabled
                | FrameArtifactAuthorityPolicy::PropertyScene
                | FrameArtifactAuthorityPolicy::TransformSurface(_)
                | FrameArtifactAuthorityPolicy::TransformPropertySurface(_)
                | FrameArtifactAuthorityPolicy::BakedScrollHost(_)
                | FrameArtifactAuthorityPolicy::ScrollTransformHost(_, _)
                | FrameArtifactAuthorityPolicy::ScrollContentLocal(_)
                | FrameArtifactAuthorityPolicy::ScrollContentEffectReceiver(_, _)
                | FrameArtifactAuthorityPolicy::NativeScrollForestContent(_) => {
                    PaintOpacityAuthority::Baked
                }
            }
            },
            baked_scroll_host: baked_scroll_host_witness(policy),
            ..PaintRecordingContext::default()
        };
    let planned_boundary_cutouts = super::PlannedBoundaryCutoutSet::default();
    let preflight = record_retained_coverage_manifest_with_context(
        arena,
        roots,
        false,
        true,
        CoverageRecordingMode::MetadataOnly,
        property_trees,
        paint_generations,
        initial_recording_context,
        match policy {
            FrameArtifactAuthorityPolicy::TransformSurface(witness)
            | FrameArtifactAuthorityPolicy::TransformPropertySurface(witness) => Some(witness),
            _ => None,
        },
        &planned_boundary_cutouts,
    );
    let mut preflight_eligibility = assess_manifest(&preflight, policy);
    if matches!(
        policy,
        FrameArtifactAuthorityPolicy::PropertyNeutral | FrameArtifactAuthorityPolicy::ClipEnabled
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

    let manifest = record_retained_coverage_manifest_with_context(
        arena,
        roots,
        false,
        true,
        CoverageRecordingMode::FullArtifact,
        property_trees,
        paint_generations,
        initial_recording_context,
        match policy {
            FrameArtifactAuthorityPolicy::TransformSurface(witness)
            | FrameArtifactAuthorityPolicy::TransformPropertySurface(witness) => Some(witness),
            _ => None,
        },
        &planned_boundary_cutouts,
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
        | FrameArtifactAuthorityPolicy::TransformSurface(_)
        | FrameArtifactAuthorityPolicy::TransformPropertySurface(_)
        | FrameArtifactAuthorityPolicy::EffectPropertySurface(_)
        | FrameArtifactAuthorityPolicy::BakedScrollHost(_)
        | FrameArtifactAuthorityPolicy::ScrollTransformHost(_, _)
        | FrameArtifactAuthorityPolicy::ScrollContentLocal(_)
        | FrameArtifactAuthorityPolicy::ScrollContentEffectReceiver(_, _)
        | FrameArtifactAuthorityPolicy::NativeScrollForestContent(_) => {
            PaintArtifactTarget::CurrentTarget
        }
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
    let mut planned_boundary_count = 0usize;
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
                if matches!(
                    policy,
                    FrameArtifactAuthorityPolicy::ScrollContentLocal(_)
                        | FrameArtifactAuthorityPolicy::ScrollContentEffectReceiver(_, _)
                        | FrameArtifactAuthorityPolicy::NativeScrollForestContent(_)
                ) && chunk.properties.legacy_boundary_dimensions() != Default::default()
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
                if let FrameArtifactAuthorityPolicy::EffectPropertySurface(effect) = policy
                    && (chunk.properties.effect != Some(effect)
                        || chunk.properties.transform.is_some()
                        || chunk.properties.scroll.is_some())
                {
                    let reason = FrameArtifactFallbackReason::NonEffectProperty(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let FrameArtifactAuthorityPolicy::TransformSurface(witness) = policy
                    && !transform_surface_properties_are_exact(chunk.properties, witness)
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let FrameArtifactAuthorityPolicy::TransformPropertySurface(witness) = policy
                    && !transform_property_surface_properties_are_exact(chunk.properties, witness)
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let Some(witness) = baked_scroll_host_witness(policy)
                    && !baked_scroll_host_properties_are_exact(
                        chunk.owner,
                        chunk.properties,
                        witness,
                    )
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
            }
            PaintCoverageItem::TransparentNode {
                owner, properties, ..
            } => {
                if matches!(
                    policy,
                    FrameArtifactAuthorityPolicy::ScrollContentLocal(_)
                        | FrameArtifactAuthorityPolicy::ScrollContentEffectReceiver(_, _)
                        | FrameArtifactAuthorityPolicy::NativeScrollForestContent(_)
                ) && properties.legacy_boundary_dimensions() != Default::default()
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(*owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let FrameArtifactAuthorityPolicy::TransformSurface(witness) = policy
                    && !transform_surface_properties_are_exact(*properties, witness)
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(*owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
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
                if let FrameArtifactAuthorityPolicy::EffectPropertySurface(effect) = policy
                    && (properties.effect != Some(effect)
                        || properties.transform.is_some()
                        || properties.scroll.is_some())
                {
                    let reason = FrameArtifactFallbackReason::NonEffectProperty(*owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let FrameArtifactAuthorityPolicy::TransformPropertySurface(witness) = policy
                    && !transform_property_surface_properties_are_exact(*properties, witness)
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(*owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let Some(witness) = baked_scroll_host_witness(policy)
                    && !baked_scroll_host_properties_are_exact(*owner, *properties, witness)
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
                let invalid = match policy {
                    FrameArtifactAuthorityPolicy::TransformSurface(witness) => {
                        !transform_surface_properties_are_exact(*properties, witness)
                    }
                    FrameArtifactAuthorityPolicy::TransformPropertySurface(witness) => {
                        !transform_property_surface_properties_are_exact(*properties, witness)
                    }
                    FrameArtifactAuthorityPolicy::PropertyScene => {
                        properties.transform.is_some()
                            || properties.effect.is_some()
                            || properties.scroll.is_some()
                    }
                    FrameArtifactAuthorityPolicy::EffectPropertySurface(effect) => {
                        properties.effect != Some(effect)
                            || properties.transform.is_some()
                            || properties.scroll.is_some()
                    }
                    FrameArtifactAuthorityPolicy::BakedScrollHost(witness) => {
                        !baked_scroll_host_properties_are_exact(*owner, *properties, witness)
                    }
                    FrameArtifactAuthorityPolicy::ScrollTransformHost(witness, _) => {
                        !baked_scroll_host_properties_are_exact(*owner, *properties, witness)
                    }
                    FrameArtifactAuthorityPolicy::ScrollContentLocal(_)
                    | FrameArtifactAuthorityPolicy::ScrollContentEffectReceiver(_, _)
                    | FrameArtifactAuthorityPolicy::NativeScrollForestContent(_) => {
                        properties.legacy_boundary_dimensions() != Default::default()
                    }
                    _ => {
                        properties.transform.is_some()
                            || properties.effect.is_some()
                            || properties.scroll.is_some()
                    }
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
            PaintCoverageItem::PlannedBoundary { boundary, .. } => {
                planned_boundary_count = planned_boundary_count.saturating_add(1);
                let allowed = match policy {
                    FrameArtifactAuthorityPolicy::TransformSurface(_)
                    | FrameArtifactAuthorityPolicy::TransformPropertySurface(_)
                    | FrameArtifactAuthorityPolicy::EffectPropertySurface(_)
                    | FrameArtifactAuthorityPolicy::PropertyScene => true,
                    FrameArtifactAuthorityPolicy::ScrollTransformHost(_, expected) => {
                        *boundary == expected
                    }
                    FrameArtifactAuthorityPolicy::ScrollContentEffectReceiver(_, expected) => {
                        *boundary == expected
                    }
                    FrameArtifactAuthorityPolicy::NativeScrollForestContent(_) => {
                        matches!(boundary.kind, super::PlannedBoundaryKind::Scroll(scroll) if scroll.0 == boundary.root)
                    }
                    _ => false,
                };
                if !allowed {
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
    if matches!(
        policy,
        FrameArtifactAuthorityPolicy::ScrollContentEffectReceiver(_, _)
    ) && planned_boundary_count != 1
    {
        let reason = FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        );
        if !reasons.contains(&reason) {
            reasons.push(reason);
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

fn transform_surface_properties_are_exact(
    properties: crate::view::compositor::property_tree::PropertyTreeState,
    witness: PaintTransformSurfaceWitness,
) -> bool {
    properties.transform == Some(witness.transform)
        && properties.clip.is_none()
        && properties.effect.is_none()
        && properties.scroll.is_none()
}

fn transform_property_surface_properties_are_exact(
    properties: crate::view::compositor::property_tree::PropertyTreeState,
    witness: PaintTransformSurfaceWitness,
) -> bool {
    properties.transform == Some(witness.transform)
        && properties.effect.is_none()
        && properties.scroll.is_none()
}

fn baked_scroll_host_properties_are_exact(
    owner: NodeKey,
    properties: crate::view::compositor::property_tree::PropertyTreeState,
    witness: PaintBakedScrollHostWitness,
) -> bool {
    let properties = properties.legacy_boundary_dimensions();
    if owner == witness.boundary_root() {
        properties == Default::default()
    } else if owner == witness.child() {
        properties.transform.is_none()
            && properties.effect.is_none()
            && properties.scroll == Some(witness.scroll())
            && properties.clip == Some(witness.contents_clip())
    } else {
        false
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
        let exact_deferred_viewport_root =
            exact_deferred_viewport_self_clip_witness(arena, key, property_trees).is_some();
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
                    FrameArtifactAuthorityPolicy::ExistingBakedProperties
                    | FrameArtifactAuthorityPolicy::RootOpacityGroup(_)
                    | FrameArtifactAuthorityPolicy::TransformSurface(_)
                    | FrameArtifactAuthorityPolicy::TransformPropertySurface(_)
                    | FrameArtifactAuthorityPolicy::EffectPropertySurface(_)
                    | FrameArtifactAuthorityPolicy::BakedScrollHost(_)
                    | FrameArtifactAuthorityPolicy::ScrollTransformHost(_, _)
                    | FrameArtifactAuthorityPolicy::ScrollContentLocal(_)
                    | FrameArtifactAuthorityPolicy::ScrollContentEffectReceiver(_, _)
                    | FrameArtifactAuthorityPolicy::NativeScrollForestContent(_) => false,
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
mod scroll_host_tests;

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
