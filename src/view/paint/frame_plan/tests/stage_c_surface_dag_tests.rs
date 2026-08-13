//! C2 ordered receiver reconstruction over artifact-derived inputs.
//!
//! Consumption transitions are classified from the artifact's keyed owner
//! endpoints. This module verifies their attachment to artifact-derived
//! surface nodes and the separate reconstruction of generic receivers. It
//! does not prove clip-space rebasing, which remains a later C2 gate.

use std::{cmp::Reverse, collections::BTreeSet};

use super::*;
use crate::view::paint::{
    LayerizationPolicy, SurfaceDag, SurfaceDagError, SurfaceDagNodeKind,
    classify_artifact_transition_sequence, reconstruct_surface_dag,
};

fn owner_depth(arena: &NodeArena, mut owner: NodeKey) -> usize {
    let mut depth = 0;
    while let Some(parent) = arena.parent_of(owner) {
        depth += 1;
        owner = parent;
    }
    depth
}

fn reorder_artifact_leaf_first(arena: &NodeArena, artifact: &mut PaintArtifact) {
    artifact
        .owner_nodes
        .sort_by_key(|snapshot| Reverse(owner_depth(arena, snapshot.owner)));
    artifact
        .chunks
        .sort_by_key(|chunk| Reverse(owner_depth(arena, chunk.owner)));

    let old_ops = artifact.ops.clone();
    let mut reordered_ops = Vec::with_capacity(old_ops.len());
    for chunk in &mut artifact.chunks {
        let source = chunk.op_range.clone();
        let start = reordered_ops.len();
        reordered_ops.extend_from_slice(&old_ops[source]);
        chunk.op_range = start..reordered_ops.len();
    }
    artifact.ops = reordered_ops;
}

fn assert_receiver_chain_reaches_scene_root(
    surface_dag: &SurfaceDag,
    origin: &crate::view::paint::SurfaceDagNode,
) {
    let mut receiver = origin.receiver();
    for _ in 0..surface_dag.nodes().len() {
        match surface_dag
            .receiver_node(receiver)
            .expect("receiver-chain gate rejects unknown surface ids")
        {
            None => return,
            Some(node) => receiver = node.receiver(),
        }
    }
    panic!("receiver-chain gate must reach a scene root within the node bound");
}

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
        let requests = stage_c_artifact_surface_transition_requests(&artifact);
        let events = classify_artifact_transition_sequence(&artifact, &requests)
            .expect("C2b classified consumption stream");
        let surface_dag = reconstruct_surface_dag(
            &artifact,
            &events,
            LayerizationPolicy::PreservePropertyBoundaries,
        )
        .expect("C2b reconstructed surface DAG");
        assert_eq!(surface_dag.nodes().len(), legacy.nodes.len());
        let stable_ids = artifact
            .owner_property_states
            .iter()
            .map(|snapshot| (snapshot.owner, snapshot.stable_id))
            .collect::<FxHashMap<_, _>>();

        for ((legacy_node, event), actual) in
            legacy.nodes.iter().zip(&events).zip(surface_dag.nodes())
        {
            assert_eq!(actual.target(), legacy_node.owner);
            assert_eq!(actual.stable_id(), stable_ids[&legacy_node.owner]);
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
    let requests = stage_c_artifact_surface_transition_requests(&artifact);
    let events = classify_artifact_transition_sequence(&artifact, &requests)
        .expect("C2b native classified transitions");
    let surface_dag = reconstruct_surface_dag(
        &artifact,
        &events,
        LayerizationPolicy::PreservePropertyBoundaries,
    )
    .expect("C2b native surface DAG");
    assert_eq!(surface_dag.nodes().len(), forest.boundaries.len());
    assert_eq!(surface_dag.roots().len(), roots.len());
    for (ordinal, (root, expected_target)) in surface_dag.roots().iter().zip(&roots).enumerate() {
        assert_eq!(root.id().index(), ordinal);
        assert_eq!(root.target(), *expected_target);
        assert_eq!(
            root.stable_id(),
            arena
                .get(*expected_target)
                .expect("native scene root")
                .element
                .stable_id(),
        );
    }

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
            SurfaceDagError::ClipRebaseScroll { .. } => "clip-rebase-scroll",
            SurfaceDagError::ClipRebaseOutsideBoundary { .. } => "clip-rebase-outside-boundary",
            SurfaceDagError::SurfaceNodeOrdinalOverflow(_) => "surface-node-ordinal-overflow",
            SurfaceDagError::UnknownSceneRootReceiver(_) => "unknown-scene-root-receiver",
            SurfaceDagError::UnknownSurfaceReceiver(_) => "unknown-surface-receiver",
            SurfaceDagError::CyclicSurfaceReceiver(_) => "cyclic-surface-receiver",
        }
    }

    let (arena, root, properties, generations) = same_owner_transform_effect_scroll_roles_fixture();
    let artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("C2b co-located artifact");
    let requests = stage_c_artifact_surface_transition_requests(&artifact);
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
fn stage_c_surface_dag_retains_plain_roots_without_minting_surfaces() {
    let fixture = super::property_boundary_forest_plain_root_tests::plain_root_fixture();
    let artifact = stage_c_classification_artifact_fixture(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
    )
    .expect("C3 identity graph fixture");
    let roots = SurfaceDag::roots_from_artifact_for_test(&artifact)
        .expect("C3 complete scene-root identity registry");
    let candidates = derive_artifact_surface_candidates(
        &artifact,
        LayerizationPolicy::PreservePropertyBoundaries,
    )
    .expect("C3 artifact surface candidates");

    assert_eq!(roots.len(), 5);
    assert_eq!(candidates.len(), 4);
    assert_eq!(
        roots
            .iter()
            .map(|root| (root.id().index(), root.target(), root.stable_id()))
            .collect::<Vec<_>>(),
        fixture
            .roots
            .iter()
            .enumerate()
            .map(|(ordinal, root)| {
                (
                    ordinal,
                    *root,
                    fixture
                        .arena
                        .get(*root)
                        .expect("scene root")
                        .element
                        .stable_id(),
                )
            })
            .collect::<Vec<_>>(),
    );
    let plain_roots = [fixture.roots[0], fixture.roots[2], fixture.roots[4]];
    assert!(
        candidates
            .iter()
            .all(|candidate| !plain_roots.contains(&candidate.target())),
        "three plain roots must remain identities without minting surfaces",
    );
}

#[test]
fn stage_c_surface_dag_rejects_an_unknown_scene_root_receiver() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::FrameRootScroll);
    let artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("C3 scene-root fixture");
    let requests = stage_c_artifact_surface_transition_requests(&artifact);
    let events = classify_artifact_transition_sequence(&artifact, &requests)
        .expect("C3 scene-root transitions");
    let mut surface_dag = reconstruct_surface_dag(
        &artifact,
        &events,
        LayerizationPolicy::PreservePropertyBoundaries,
    )
    .expect("C3 scene-root DAG");
    let root_id = surface_dag.roots()[0].id();
    let receiver = surface_dag.nodes()[0].receiver();
    surface_dag.omit_scene_root_for_test(root_id);

    assert_eq!(
        surface_dag.receiver_node(receiver),
        Err(SurfaceDagError::UnknownSceneRootReceiver(root_id)),
    );
}

#[test]
fn stage_c_surface_dag_accepts_the_production_leaf_first_owner_order() {
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
    reorder_artifact_leaf_first(&arena, &mut artifact);
    let requests = stage_c_artifact_surface_transition_requests(&artifact);
    let events = classify_artifact_transition_sequence(&artifact, &requests)
        .expect("C2c leaf-first transition stream");
    let surface_dag = reconstruct_surface_dag(
        &artifact,
        &events,
        LayerizationPolicy::PreservePropertyBoundaries,
    )
    .expect("C2c leaf-first surface DAG");
    let [scroll, transform] = surface_dag.nodes() else {
        panic!("leaf-first transform-scroll must retain two nodes")
    };
    assert_eq!(scroll.target(), scroll_owner);
    assert!(matches!(
        scroll.kind(),
        SurfaceDagNodeKind::ScrollContent { .. }
    ));
    let receiver = surface_dag
        .receiver_node(scroll.receiver())
        .expect("validated forward receiver")
        .expect("scroll targets ancestor transform");
    assert_eq!(receiver.id(), transform.id());
    assert_eq!(receiver.target(), root);
    assert!(matches!(receiver.kind(), SurfaceDagNodeKind::Transform(_)));
    assert!(
        scroll.id().index() < receiver.id().index(),
        "leaf-first store order permits a forward receiver id without reordering nodes",
    );
    assert_eq!(transform.receiver().scene_root_ordinal(), Some(0));
    assert_receiver_chain_reaches_scene_root(&surface_dag, scroll);
}

#[test]
fn stage_c_surface_dag_accepts_a_leaf_first_nested_scroll_chain() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::NestedScroll);
    let mut artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("C2c nested-scroll artifact");
    reorder_artifact_leaf_first(&arena, &mut artifact);
    let requests = stage_c_artifact_surface_transition_requests(&artifact);
    let events = classify_artifact_transition_sequence(&artifact, &requests)
        .expect("C2c leaf-first nested-scroll transitions");
    let surface_dag = reconstruct_surface_dag(
        &artifact,
        &events,
        LayerizationPolicy::PreservePropertyBoundaries,
    )
    .expect("C2c leaf-first nested-scroll DAG");
    let [inner, outer] = surface_dag.nodes() else {
        panic!("leaf-first nested scroll must retain two nodes")
    };
    assert!(matches!(
        (inner.kind(), outer.kind()),
        (
            SurfaceDagNodeKind::ScrollContent { .. },
            SurfaceDagNodeKind::ScrollContent { .. }
        )
    ));
    let receiver = surface_dag
        .receiver_node(inner.receiver())
        .expect("validated nested forward receiver")
        .expect("inner scroll targets outer scroll content");
    assert_eq!(receiver.id(), outer.id());
    assert!(inner.id().index() < receiver.id().index());
    assert_eq!(outer.receiver().scene_root_ordinal(), Some(0));
    assert_receiver_chain_reaches_scene_root(&surface_dag, inner);
}
