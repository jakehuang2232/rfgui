//! C2b ordered receiver reconstruction over the frozen C0a and C0c inputs.
//!
//! Consumption transitions remain the independently classified C1 stream.
//! This module verifies only their attachment to artifact-derived surface
//! nodes and the separate reconstruction of generic receivers. It does not
//! derive transition endpoints from chunks or prove clip-space rebasing; both
//! remain later C2 gates.

use std::collections::BTreeSet;

use super::*;
use crate::view::paint::{
    ArtifactTransitionRequest, LayerizationPolicy, SurfaceDag, SurfaceDagError, SurfaceDagNodeKind,
    classify_artifact_transition_sequence, reconstruct_surface_dag,
};

fn assert_receiver_matches_legacy(
    surface_dag: &SurfaceDag,
    actual: &crate::view::paint::SurfaceDagNode,
    expected: PropertyBoundaryReceiverScope,
) {
    match expected {
        PropertyBoundaryReceiverScope::FrameRoot { scene_root_ordinal } => {
            assert_eq!(
                actual.receiver().scene_root_ordinal(),
                Some(scene_root_ordinal),
            );
            assert_eq!(
                surface_dag
                    .receiver_node(actual.receiver())
                    .expect("validated scene-root receiver"),
                None,
            );
        }
        PropertyBoundaryReceiverScope::Surface(expected) => {
            let receiver = surface_dag
                .receiver_node(actual.receiver())
                .expect("validated surface receiver")
                .expect("legacy surface receivers remain surfaces");
            assert_eq!(receiver.id().index(), expected.0 as usize);
        }
        PropertyBoundaryReceiverScope::ScrollContent(expected) => {
            let receiver = surface_dag
                .receiver_node(actual.receiver())
                .expect("validated scroll-content receiver")
                .expect("legacy scroll-content receivers remain surfaces");
            assert_eq!(receiver.id().index(), expected.0 as usize);
            assert!(matches!(
                receiver.kind(),
                SurfaceDagNodeKind::ScrollContent { .. }
            ));
        }
    }
}

#[test]
fn stage_c_eight_legacy_success_shapes_reconstruct_the_same_generic_receivers() {
    let shapes = [
        ScrollInterleaveFixtureShape::FrameRootScroll,
        ScrollInterleaveFixtureShape::TransformScroll,
        ScrollInterleaveFixtureShape::EffectScroll,
        ScrollInterleaveFixtureShape::TransformEffectScroll,
        ScrollInterleaveFixtureShape::EffectTransformScroll,
        ScrollInterleaveFixtureShape::EffectNeutralTransformNeutralScroll,
        ScrollInterleaveFixtureShape::CoLocatedTransformScroll,
        ScrollInterleaveFixtureShape::NestedScroll,
    ];

    for shape in shapes {
        let (arena, root, properties, generations) = property_scroll_interleave_fixture(shape);
        let plan = plan_property_scroll_interleave_scaffold_with_context(
            &arena,
            &[root],
            &properties,
            &generations,
            TransformSurfacePlanContext::default(),
        )
        .expect("C2b legacy success fixture");
        let legacy = &plan
            .property_scroll_planning_scaffold()
            .expect("C2b legacy boundary DAG")
            .boundary_dag;
        let artifact =
            stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
                .expect("C2b closed artifact");
        let requests = legacy
            .nodes
            .iter()
            .map(|node| {
                ArtifactTransitionRequest::new(
                    node.owner,
                    node.consumption.expected_before,
                    node.consumption.projected_after,
                )
            })
            .collect::<Vec<_>>();
        let events = classify_artifact_transition_sequence(&artifact, &requests)
            .expect("C2b classified consumption stream");
        let surface_dag = reconstruct_surface_dag(
            &artifact,
            &events,
            LayerizationPolicy::PreservePropertyBoundaries,
        )
        .expect("C2b reconstructed surface DAG");
        assert_eq!(surface_dag.nodes().len(), legacy.nodes.len());

        for ((legacy_node, event), actual) in
            legacy.nodes.iter().zip(&events).zip(surface_dag.nodes())
        {
            assert_eq!(actual.target(), legacy_node.owner);
            assert_eq!(actual.cursor(), event.cursor());
            assert_eq!(actual.transition(), event.transition());
            match (&legacy_node.kind, actual.kind()) {
                (
                    PropertyBoundaryDagNodeKind::Transform(expected),
                    SurfaceDagNodeKind::Transform(actual),
                ) => assert_eq!(actual, expected.id),
                (
                    PropertyBoundaryDagNodeKind::Effect(expected),
                    SurfaceDagNodeKind::Effect(actual),
                ) => assert_eq!(actual, expected.id),
                (
                    PropertyBoundaryDagNodeKind::Scroll(expected),
                    SurfaceDagNodeKind::ScrollContent {
                        scroll,
                        contents_clip,
                    },
                ) => {
                    assert_eq!(scroll, expected.scroll.id);
                    assert_eq!(contents_clip, expected.contents_clip.id);
                    assert!(actual.clip_rebase().is_some());
                }
                pair => panic!("legacy/C2 surface-kind mismatch: {pair:?}"),
            }
            assert_receiver_matches_legacy(&surface_dag, actual, legacy_node.receiver);
        }
    }
}

#[test]
fn stage_c_native_forest_reconstructs_branch_and_multi_root_receivers() {
    let (arena, roots, properties, generations) = native_scroll_forest_plan_fixture();
    let plan = plan_native_scroll_forest_scaffold_with_context(
        &arena,
        &roots,
        &properties,
        &generations,
        1.0,
        TransformSurfacePlanContext::default(),
    )
    .expect("C2b native forest");
    let forest = plan
        .native_scroll_forest_planning_scaffold()
        .expect("C2b native scroll scaffold");
    let artifact =
        stage_c_classification_artifact_fixture(&arena, &roots, &properties, &generations)
            .expect("C2b native closed artifact");
    let requests = forest
        .boundaries
        .iter()
        .map(|boundary| {
            ArtifactTransitionRequest::new(
                boundary.boundary_root,
                boundary.projection.live_input,
                boundary.projection.projected_output,
            )
        })
        .collect::<Vec<_>>();
    let events = classify_artifact_transition_sequence(&artifact, &requests)
        .expect("C2b native classified transitions");
    let surface_dag = reconstruct_surface_dag(
        &artifact,
        &events,
        LayerizationPolicy::PreservePropertyBoundaries,
    )
    .expect("C2b native surface DAG");
    assert_eq!(surface_dag.nodes().len(), forest.boundaries.len());

    for (boundary, node) in forest.boundaries.iter().zip(surface_dag.nodes()) {
        assert_eq!(node.target(), boundary.boundary_root);
        assert!(matches!(
            node.kind(),
            SurfaceDagNodeKind::ScrollContent { scroll, .. } if scroll == boundary.scroll.id
        ));
        match boundary.parent {
            None => assert_eq!(
                node.receiver().scene_root_ordinal(),
                Some(boundary.scene_root_ordinal),
            ),
            Some(parent) => {
                let receiver = surface_dag
                    .receiver_node(node.receiver())
                    .expect("validated native receiver")
                    .expect("nested native boundary receiver");
                assert_eq!(receiver.id().index(), parent.0 as usize);
                assert!(matches!(
                    receiver.kind(),
                    SurfaceDagNodeKind::ScrollContent { .. }
                ));
            }
        }
    }

    assert_eq!(
        surface_dag
            .nodes()
            .iter()
            .filter_map(|node| node.receiver().scene_root_ordinal())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([0, 1]),
        "both artifact-derived scene-root capabilities remain distinct",
    );
    let siblings = [&surface_dag.nodes()[2], &surface_dag.nodes()[3]];
    assert_eq!(
        siblings.map(|node| {
            surface_dag
                .receiver_node(node.receiver())
                .expect("validated sibling receiver")
                .expect("branch parent")
                .id()
                .index()
        }),
        [1, 1],
        "branch siblings target their shared ancestor, not the previous sibling",
    );
}

#[test]
fn stage_c_surface_dag_rejects_misaligned_consumption_with_a_closed_taxonomy() {
    fn error_name(error: SurfaceDagError) -> &'static str {
        match error {
            SurfaceDagError::Transition(_) => "transition",
            SurfaceDagError::MissingScrollContentsClip { .. } => "missing-scroll-contents-clip",
            SurfaceDagError::TransitionCount { .. } => "transition-count",
            SurfaceDagError::TransitionSceneRoot { .. } => "transition-scene-root",
            SurfaceDagError::TransitionTarget { .. } => "transition-target",
            SurfaceDagError::TransitionCursor { .. } => "transition-cursor",
            SurfaceDagError::TransitionKind { .. } => "transition-kind",
            SurfaceDagError::ReceiverOwnerOutOfOrder { .. } => "receiver-owner-out-of-order",
            SurfaceDagError::SurfaceNodeOrdinalOverflow(_) => "surface-node-ordinal-overflow",
            SurfaceDagError::UnknownSurfaceReceiver(_) => "unknown-surface-receiver",
        }
    }

    let (arena, root, properties, generations) = same_owner_transform_effect_scroll_roles_fixture();
    let plan = plan_property_scroll_interleave_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::default(),
    )
    .expect("C2b co-located fixture");
    let legacy = &plan
        .property_scroll_planning_scaffold()
        .expect("C2b co-located DAG")
        .boundary_dag;
    let artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("C2b co-located artifact");
    let requests = legacy
        .nodes
        .iter()
        .map(|node| {
            ArtifactTransitionRequest::new(
                node.owner,
                node.consumption.expected_before,
                node.consumption.projected_after,
            )
        })
        .collect::<Vec<_>>();
    let events = classify_artifact_transition_sequence(&artifact, &requests)
        .expect("C2b co-located transitions");

    let count_error = reconstruct_surface_dag(
        &artifact,
        &events[..events.len() - 1],
        LayerizationPolicy::PreservePropertyBoundaries,
    )
    .expect_err("an incomplete consumption stream must fail closed");
    assert_eq!(
        count_error,
        SurfaceDagError::TransitionCount {
            candidates: 3,
            events: 2,
        },
    );
    assert_eq!(error_name(count_error), "transition-count");

    let mut wrong_kind = events.clone();
    wrong_kind.swap(0, 1);
    assert_eq!(
        reconstruct_surface_dag(
            &artifact,
            &wrong_kind,
            LayerizationPolicy::PreservePropertyBoundaries,
        ),
        Err(SurfaceDagError::TransitionKind {
            index: 0,
            expected: SurfaceDagNodeKind::Transform(TransformNodeId(root)),
        }),
        "co-located target/cursor equality cannot hide a kind-order mismatch",
    );
}

#[test]
fn stage_c_surface_dag_rejects_a_surface_owner_before_its_receiver_owner() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::TransformScroll);
    let plan = plan_property_scroll_interleave_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::default(),
    )
    .expect("C2b transform-scroll fixture");
    let legacy = &plan
        .property_scroll_planning_scaffold()
        .expect("C2b transform-scroll DAG")
        .boundary_dag;
    let scroll_owner = legacy.nodes[1].owner;
    let mut artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("C2b transform-scroll artifact");
    let requests = legacy
        .nodes
        .iter()
        .map(|node| {
            ArtifactTransitionRequest::new(
                node.owner,
                node.consumption.expected_before,
                node.consumption.projected_after,
            )
        })
        .collect::<Vec<_>>();
    let mut events = classify_artifact_transition_sequence(&artifact, &requests)
        .expect("C2b transform-scroll transitions");
    let root_index = artifact
        .owner_nodes
        .iter()
        .position(|snapshot| snapshot.owner == root)
        .expect("root owner snapshot");
    let scroll_index = artifact
        .owner_nodes
        .iter()
        .position(|snapshot| snapshot.owner == scroll_owner)
        .expect("scroll owner snapshot");
    artifact.owner_nodes.swap(root_index, scroll_index);
    events.swap(0, 1);

    assert_eq!(
        reconstruct_surface_dag(
            &artifact,
            &events,
            LayerizationPolicy::PreservePropertyBoundaries,
        ),
        Err(SurfaceDagError::ReceiverOwnerOutOfOrder {
            owner: scroll_owner,
            receiver_owner: root,
        }),
    );
}
