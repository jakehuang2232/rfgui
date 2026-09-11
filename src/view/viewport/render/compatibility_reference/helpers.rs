use super::*;

pub(super) fn record_auto_artifact_candidate(
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
    paint_generations: &crate::view::compositor::PaintGenerationTracker,
    raster_context: crate::view::paint::ArtifactSurfaceRasterContext,
) -> Result<RecordedArtifactCandidate, RecordedArtifactCandidateRejection> {
    attempts::record("compatibility-record");
    // Historical root-effect/current-target recording.
    let has_single_root_effect = roots.first().is_some_and(|root| {
        roots.len() == 1
            && property_trees
                .paint_state_for(*root)
                .is_some_and(|properties| properties.effect.is_some())
    });
    let outcome = if has_single_root_effect {
        crate::view::paint::record_root_group_opacity_frame_artifact(
            arena,
            roots,
            property_trees,
            paint_generations,
            crate::view::paint::RendererMode::Auto,
        )
    } else {
        crate::view::paint::record_closed_single_target_frame_artifact(
            arena,
            roots,
            property_trees,
            paint_generations,
            crate::view::paint::RendererMode::Auto,
        )
    }
    .expect("automatic production selection never forces artifact recording");
    prepare_recorded_artifact_candidate(
        outcome,
        raster_context,
        RecordedArtifactSurfaceRequirement::ZeroResident,
    )
}

/// Pure selector attempt. This function has no graph, pool, or mutable
/// viewport handle; rejection may therefore continue to the retained planner.
/// Once dispatch enters `try_compile_auto_artifact_frame`, fallback is no
/// longer permitted because graph and pool mutation may have begun.
pub(super) fn try_select_auto_detached_surface_candidate(
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
    paint_generations: &crate::view::compositor::PaintGenerationTracker,
    raster_context: crate::view::paint::ArtifactSurfaceRasterContext,
    requirement: RecordedArtifactSurfaceRequirement,
    trace: &mut AutoAuthorityTrace,
) -> Option<RecordedArtifactCandidate> {
    match record_auto_detached_surface_candidate(
        arena,
        roots,
        property_trees,
        paint_generations,
        raster_context,
        requirement,
    ) {
        Ok(candidate) => Some(candidate),
        Err(RecordedArtifactCandidateRejection::Eligibility(eligibility)) => {
            trace.capture(|| AutoAuthorityRejection::Artifact { eligibility });
            None
        }
        Err(RecordedArtifactCandidateRejection::Prepare(error)) => {
            trace.capture(|| AutoAuthorityRejection::ArtifactPrepare { error });
            None
        }
    }
}

pub(super) fn is_exact_native_root_opacity_artifact(
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
) -> bool {
    let [root] = roots else {
        return false;
    };
    if !property_trees.transforms.is_empty()
        || !property_trees.clips.is_empty()
        || !property_trees.scrolls.is_empty()
        || property_trees.effects.len() != 1
    {
        return false;
    }
    let Some(node) = arena.get(*root) else {
        return false;
    };
    let effect = crate::view::compositor::property_tree::EffectNodeId(*root);
    let exact_state = crate::view::compositor::property_tree::PropertyTreeState {
        effect: Some(effect),
        ..Default::default()
    };
    node.element.admits_exact_retained_root_opacity_artifact()
        && !node.element.is_deferred_to_root_viewport_render()
        && !node
            .element
            .placement_eligibility_metadata()
            .contains_runtime_layout_state
        && property_trees.effects.get(&effect).is_some_and(|snapshot| {
            snapshot.owner == *root
                && snapshot.parent.is_none()
                && snapshot.generation != 0
                && snapshot.opacity.is_finite()
                && (0.0..=1.0).contains(&snapshot.opacity)
        })
        && property_trees.node_state_for(*root).is_some_and(|state| {
            state.paint.legacy_boundary_eq(exact_state)
                && state.descendants.legacy_boundary_eq(exact_state)
        })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in super::super) struct RetainedAutoReachableTreeFacts {
    pub(in super::super) has_scroll_container: bool,
    pub(in super::super) has_text_area_paint_family: bool,
}

pub(super) fn retained_auto_paint_kind_is_text_area_family(
    kind: crate::view::base_component::RetainedScrollNormalizedPaintKind,
) -> bool {
    use crate::view::base_component::RetainedScrollNormalizedPaintKind;

    match kind {
        RetainedScrollNormalizedPaintKind::Element
        | RetainedScrollNormalizedPaintKind::Text
        | RetainedScrollNormalizedPaintKind::Image
        | RetainedScrollNormalizedPaintKind::Svg => false,
        RetainedScrollNormalizedPaintKind::TextArea
        | RetainedScrollNormalizedPaintKind::TextAreaProjectionSegment
        | RetainedScrollNormalizedPaintKind::TextAreaTextRun
        | RetainedScrollNormalizedPaintKind::TextAreaLineBreak => true,
    }
}

pub(in super::super) fn retained_auto_reachable_tree_facts(
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
) -> RetainedAutoReachableTreeFacts {
    let mut pending = roots.to_vec();
    let mut seen = FxHashSet::default();
    let mut facts = RetainedAutoReachableTreeFacts::default();
    while let Some(key) = pending.pop() {
        if !seen.insert(key) {
            continue;
        }
        let Some(node) = arena.get(key) else {
            continue;
        };
        if node.element.retained_paint_properties().is_scroll_container {
            facts.has_scroll_container = true;
        }
        if node
            .element
            .retained_scroll_normalized_paint_capability()
            .is_some_and(|capability| {
                retained_auto_paint_kind_is_text_area_family(capability.kind())
            })
        {
            facts.has_text_area_paint_family = true;
        }
        pending.extend(node.children().iter().copied());
    }
    facts
}

/// Historical admission predicate. Production selection no longer consumes topology.
pub(in super::super) fn native_scroll_forest_topology_is_branching_or_multi_root(
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
) -> bool {
    if roots.len() > 1 {
        return true;
    }
    let mut seen_parent = FxHashSet::default();
    let mut has_scroll_root = false;
    for scroll in property_trees.scrolls.values() {
        match scroll.parent {
            Some(parent) if !seen_parent.insert(parent) => return true,
            Some(_) => {}
            None if has_scroll_root => return true,
            None => has_scroll_root = true,
        }
    }
    false
}
