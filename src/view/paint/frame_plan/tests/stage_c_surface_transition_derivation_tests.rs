//! Production artifact-transition derivation before C3 admits surfaces.

use super::*;
use crate::view::compositor::property_tree::{
    ClipBehavior, ClipNodeId, ClipNodeRole, ClipNodeSnapshot, EffectNodeId, EffectNodeSnapshot,
};
use crate::view::paint::{
    LayerizationPolicy, SurfaceDagError, SurfaceDagNodeKind, classify_artifact_transition_sequence,
    reconstruct_surface_dag,
};

const ACCEPTED_REGRESSION_SHAPES: [(&str, ScrollInterleaveFixtureShape); 8] = [
    (
        "frame-root-scroll",
        ScrollInterleaveFixtureShape::FrameRootScroll,
    ),
    (
        "transform-scroll",
        ScrollInterleaveFixtureShape::TransformScroll,
    ),
    ("effect-scroll", ScrollInterleaveFixtureShape::EffectScroll),
    (
        "transform-effect-scroll",
        ScrollInterleaveFixtureShape::TransformEffectScroll,
    ),
    (
        "effect-transform-scroll",
        ScrollInterleaveFixtureShape::EffectTransformScroll,
    ),
    (
        "effect-neutral-transform-neutral-scroll",
        ScrollInterleaveFixtureShape::EffectNeutralTransformNeutralScroll,
    ),
    (
        "co-located-transform-scroll",
        ScrollInterleaveFixtureShape::CoLocatedTransformScroll,
    ),
    ("nested-scroll", ScrollInterleaveFixtureShape::NestedScroll),
];

fn production_requests(
    artifact: &PaintArtifact,
) -> Result<Vec<ArtifactTransitionRequest>, crate::view::paint::SurfaceDagError> {
    derive_artifact_surface_transition_requests(
        artifact,
        LayerizationPolicy::PreservePropertyBoundaries,
    )
}

#[test]
fn production_transition_derivation_matches_all_eight_accepted_scroll_oracles() {
    for (name, shape) in ACCEPTED_REGRESSION_SHAPES {
        let (arena, root, properties, generations) = property_scroll_interleave_fixture(shape);
        let artifact =
            stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
                .expect("C3b1a regression artifact");
        assert_eq!(
            production_requests(&artifact).expect("production consumption requests"),
            stage_c_artifact_surface_transition_requests(&artifact),
            "{name}",
        );
    }
}

#[test]
fn production_transition_derivation_closes_the_legacy_scroll_transform_rejection() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
    let artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("scroll-transform artifact");
    let requests = production_requests(&artifact).expect("scroll-transform production requests");
    let events = classify_artifact_transition_sequence(&artifact, &requests)
        .expect("scroll-transform production transitions");
    let dag = reconstruct_surface_dag(
        &artifact,
        &events,
        LayerizationPolicy::PreservePropertyBoundaries,
    )
    .expect("scroll-transform production DAG");
    assert_eq!(dag.nodes().len(), 2);
    assert!(matches!(
        dag.nodes()[0].kind(),
        SurfaceDagNodeKind::ScrollContent { .. }
    ));
    assert!(matches!(
        dag.nodes()[1].kind(),
        SurfaceDagNodeKind::Transform(_)
    ));
}

#[test]
fn deepest_referencing_witness_wins_across_two_surface_levels() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::TransformEffectScroll);
    let artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("three-level surface witness artifact");
    let requests = production_requests(&artifact).expect("three-level production requests");
    let events = classify_artifact_transition_sequence(&artifact, &requests)
        .expect("three-level production transitions");
    let transform = events
        .iter()
        .find(|event| event.transition().transform.from == Some(TransformNodeId(root)))
        .expect("root transform transition");
    let deepest_scroll = artifact
        .scroll_nodes
        .iter()
        .find(|scroll| scroll.owner != root)
        .expect("descendant scroll witness");
    assert_eq!(transform.transition().scroll.from, Some(deepest_scroll.id));
    assert_eq!(
        transform.transition().clip.from,
        artifact
            .owner_property_states
            .iter()
            .find(|snapshot| snapshot.owner == deepest_scroll.owner)
            .expect("deepest witness endpoints")
            .descendants
            .clip
    );
}

#[test]
fn deeper_witness_rejects_a_consumed_dimension_disagreement() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::ScrollTransform);
    let mut artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("scroll-transform disagreement artifact");
    let self_clip = ClipNodeId {
        owner: root,
        role: ClipNodeRole::SelfClip,
    };
    artifact.clip_nodes.push(ClipNodeSnapshot {
        id: self_clip,
        owner: root,
        parent: None,
        logical_scissor: [5, 7, 80, 60],
        behavior: ClipBehavior::Replace,
        generation: 43,
    });
    artifact
        .owner_property_states
        .iter_mut()
        .find(|snapshot| snapshot.owner == root)
        .expect("root scroll endpoints")
        .paint
        .clip = Some(self_clip);

    let disagreement = production_requests(&artifact);
    assert!(
        matches!(
            disagreement,
            Err(SurfaceDagError::ConflictingArtifactTransition {
                kind: SurfaceDagNodeKind::ScrollContent { scroll, .. },
                first_witness,
                conflicting_witness,
            }) if scroll == ScrollNodeId(root)
                && first_witness == root
                && conflicting_witness == arena.children_of(root)[0]
        ),
        "{disagreement:?}"
    );
}

#[test]
fn transform_and_effect_only_artifacts_derive_closed_surface_dags() {
    let (arena, root, properties, generations) = exact_transform_fixture();
    let transform_artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("transform-only artifact");
    let transform_requests =
        production_requests(&transform_artifact).expect("transform-only requests");
    let transform_events =
        classify_artifact_transition_sequence(&transform_artifact, &transform_requests)
            .expect("transform-only transitions");
    let transform_dag = reconstruct_surface_dag(
        &transform_artifact,
        &transform_events,
        LayerizationPolicy::PreservePropertyBoundaries,
    )
    .expect("transform-only surface DAG");
    assert!(!transform_dag.nodes().is_empty());
    assert!(
        transform_dag
            .nodes()
            .iter()
            .all(|node| matches!(node.kind(), SurfaceDagNodeKind::Transform(_)))
    );

    let (arena, root, _, _, properties, generations) = planning_only_nested_effect_fixture();
    let effect_artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("effect-only artifact");
    let effect_requests = production_requests(&effect_artifact).expect("effect-only requests");
    let effect_events = classify_artifact_transition_sequence(&effect_artifact, &effect_requests)
        .expect("effect-only transitions");
    let effect_dag = reconstruct_surface_dag(
        &effect_artifact,
        &effect_events,
        LayerizationPolicy::PreservePropertyBoundaries,
    )
    .expect("effect-only surface DAG");
    assert!(!effect_dag.nodes().is_empty());
    assert!(
        effect_dag
            .nodes()
            .iter()
            .all(|node| matches!(node.kind(), SurfaceDagNodeKind::Effect(_)))
    );
}

#[test]
fn owners_without_surface_candidates_emit_no_transition_requests() {
    let (arena, roots, _, fourth, properties, generations) = stage_c_depth_four_scroll_fixture();
    let mut artifact =
        stage_c_classification_artifact_fixture(&arena, &roots, &properties, &generations)
            .expect("mixed surface and inline owner artifact");
    artifact.transform_nodes.clear();
    artifact.effect_nodes.clear();
    artifact.scroll_nodes.clear();
    artifact.clip_nodes.clear();
    for endpoint in &mut artifact.owner_property_states {
        endpoint.paint = PropertyTreeState::default();
        endpoint.descendants = PropertyTreeState::default();
    }
    assert!(
        artifact
            .owner_nodes
            .iter()
            .any(|owner| owner.owner == fourth)
    );
    assert!(
        production_requests(&artifact)
            .expect("zero-candidate derivation")
            .is_empty()
    );
}

#[test]
fn scroll_terminal_closure_carries_a_self_clip_without_minting_a_surface() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::FrameRootScroll);
    let mut artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("frame-root scroll artifact");
    let self_clip = ClipNodeId {
        owner: root,
        role: ClipNodeRole::SelfClip,
    };
    artifact.clip_nodes.push(ClipNodeSnapshot {
        id: self_clip,
        owner: root,
        parent: None,
        logical_scissor: [2, 3, 90, 70],
        behavior: ClipBehavior::Replace,
        generation: 37,
    });
    artifact
        .owner_property_states
        .iter_mut()
        .find(|snapshot| snapshot.owner == root)
        .expect("root endpoints")
        .paint
        .clip = Some(self_clip);

    let requests = production_requests(&artifact).expect("self-clip terminal closure");
    let events =
        classify_artifact_transition_sequence(&artifact, &requests).expect("self-clip transition");
    let [event] = events.as_slice() else {
        panic!("frame-root scroll owns one surface edge")
    };
    assert_eq!(event.transition().clip.to, Some(self_clip));
    assert!(event.transition().clip.is_changed());
}

#[test]
fn derivation_rejections_cover_stalled_terminal_conflict_and_missing_edges() {
    let (arena, root, properties, generations) = exact_transform_fixture();
    let mut stalled =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("transform artifact");
    stalled
        .owner_property_states
        .iter_mut()
        .find(|snapshot| snapshot.owner == root)
        .expect("transform endpoints")
        .descendants
        .transform = None;
    assert!(matches!(
        production_requests(&stalled),
        Err(SurfaceDagError::ArtifactTransitionDerivationStalled {
            witness,
            ..
        }) if witness == root
    ));

    let child = arena.children_of(root)[0];
    let child_effect = EffectNodeId(child);
    let mut terminal =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("second transform artifact");
    terminal.effect_nodes.push(EffectNodeSnapshot {
        id: child_effect,
        owner: child,
        parent: None,
        opacity: 0.5,
        generation: 41,
    });
    terminal
        .owner_property_states
        .iter_mut()
        .find(|snapshot| snapshot.owner == root)
        .expect("root terminal endpoints")
        .paint
        .effect = Some(child_effect);
    assert!(matches!(
        production_requests(&terminal),
        Err(SurfaceDagError::ArtifactTransitionTerminalMismatch {
            witness,
            ..
        }) if witness == root
    ));

    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::TransformScroll);
    let mut missing =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("transform-scroll artifact");
    let scroll_owner = arena.children_of(root)[0];
    let endpoints = missing
        .owner_property_states
        .iter_mut()
        .find(|snapshot| snapshot.owner == scroll_owner)
        .expect("scroll endpoints");
    endpoints.descendants.scroll = None;
    endpoints.descendants.clip = None;
    endpoints.paint.scroll = None;
    endpoints.paint.clip = None;
    assert!(matches!(
        production_requests(&missing),
        Err(SurfaceDagError::MissingArtifactTransition {
            kind: SurfaceDagNodeKind::ScrollContent { scroll, .. },
        }) if scroll == ScrollNodeId(scroll_owner)
    ));

    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::TransformScroll);
    let mut conflicting =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("branching transform-scroll artifact");
    let first_scroll_owner = arena.children_of(root)[0];
    let sibling_scroll_owner = arena.children_of(first_scroll_owner)[0];
    conflicting
        .owner_nodes
        .iter_mut()
        .find(|snapshot| snapshot.owner == sibling_scroll_owner)
        .expect("second branch topology")
        .parent = Some(root);

    let first_scroll = conflicting
        .scroll_nodes
        .iter()
        .find(|snapshot| snapshot.owner == first_scroll_owner)
        .copied()
        .expect("first branch scroll");
    conflicting
        .scroll_nodes
        .push(crate::view::compositor::property_tree::ScrollNodeSnapshot {
            id: ScrollNodeId(sibling_scroll_owner),
            owner: sibling_scroll_owner,
            ..first_scroll
        });
    let first_clip = conflicting
        .clip_nodes
        .iter()
        .find(|snapshot| snapshot.owner == first_scroll_owner)
        .copied()
        .expect("first branch contents clip");
    let sibling_clip = ClipNodeId {
        owner: sibling_scroll_owner,
        role: ClipNodeRole::ContentsClip,
    };
    conflicting.clip_nodes.push(ClipNodeSnapshot {
        id: sibling_clip,
        owner: sibling_scroll_owner,
        ..first_clip
    });
    let first_endpoints = conflicting
        .owner_property_states
        .iter()
        .find(|snapshot| snapshot.owner == first_scroll_owner)
        .copied()
        .expect("first branch endpoints");
    let sibling_endpoints = conflicting
        .owner_property_states
        .iter_mut()
        .find(|snapshot| snapshot.owner == sibling_scroll_owner)
        .expect("second branch endpoints");
    sibling_endpoints.descendants = PropertyTreeState {
        scroll: Some(ScrollNodeId(sibling_scroll_owner)),
        clip: Some(sibling_clip),
        ..first_endpoints.descendants
    };
    sibling_endpoints.paint = first_endpoints.paint;
    let conflict = production_requests(&conflicting);
    assert!(
        matches!(
            conflict,
            Err(SurfaceDagError::ConflictingArtifactTransition {
                kind: SurfaceDagNodeKind::Transform(transform),
                ..
            }) if transform == TransformNodeId(root)
        ),
        "{conflict:?}"
    );
}
