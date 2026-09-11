//! C2 ordered receiver reconstruction over artifact-derived inputs.
//!
//! Consumption transitions are classified from the artifact's keyed owner
//! endpoints. This module verifies their attachment to artifact-derived
//! surface nodes and the separate reconstruction of generic receivers. It
//! does not prove clip-space rebasing, which remains a later C2 gate.

use std::cmp::Reverse;

use super::*;
use crate::view::paint::{
    LayerizationPolicy, SurfaceDag, SurfaceDagError, SurfaceDagExecutionOrder,
    SurfaceDagExecutionTargetId, SurfaceDagNodeKind, SurfaceDagTargetId,
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

pub(super) fn reorder_artifact_leaf_first(arena: &NodeArena, artifact: &mut PaintArtifact) {
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

fn reconstruct_artifact_surface_dag(artifact: &PaintArtifact) -> SurfaceDag {
    let requests = stage_c_artifact_surface_transition_requests(artifact);
    let events = classify_artifact_transition_sequence(artifact, &requests)
        .expect("closed artifact transition stream");
    reconstruct_surface_dag(
        artifact,
        &events,
        LayerizationPolicy::ResolveMaterializedTargets,
    )
    .expect("closed artifact surface DAG")
}

fn assert_execution_order_contract(surface_dag: &SurfaceDag, execution: &SurfaceDagExecutionOrder) {
    assert_eq!(execution.roots().len(), surface_dag.roots().len());
    assert_eq!(execution.nodes().len(), surface_dag.nodes().len());
    let mut next_node = 0_u32;
    let mut seen_sources = vec![false; surface_dag.nodes().len()];
    for (source_root, execution_root) in surface_dag.roots().iter().zip(execution.roots()) {
        assert_eq!(execution_root.identity(), *source_root);
        let span = execution_root.node_span();
        assert_eq!(span.start, next_node);
        assert!(span.end >= span.start);
        assert!(span.end as usize <= execution.nodes().len());
        for ordinal in span.clone() {
            let node = execution.nodes()[ordinal as usize];
            assert_eq!(node.id().index(), ordinal as usize);
            assert_eq!(node.scene_root(), source_root.id());
            assert_eq!(execution.execution_id(node.source()), Some(node.id()));
            assert_eq!(execution.source_node_id(node.id()), Some(node.source()));
            let source_node = surface_dag.nodes()[node.source().index()];
            assert_eq!(source_node.id(), node.source());
            match (source_node.receiver(), node.receiver()) {
                (
                    SurfaceDagTargetId::SceneRoot(source_receiver),
                    SurfaceDagExecutionTargetId::SceneRoot(execution_receiver),
                ) => assert_eq!(execution_receiver, source_receiver),
                (
                    SurfaceDagTargetId::Surface(source_receiver),
                    SurfaceDagExecutionTargetId::Surface(execution_receiver),
                ) => assert_eq!(
                    execution.execution_id(source_receiver),
                    Some(execution_receiver),
                ),
                _ => panic!("execution remap must preserve receiver kind"),
            }
            assert!(!seen_sources[node.source().index()]);
            seen_sources[node.source().index()] = true;
            match node.receiver() {
                SurfaceDagExecutionTargetId::SceneRoot(root) => {
                    assert_eq!(root, source_root.id());
                }
                SurfaceDagExecutionTargetId::Surface(parent) => {
                    assert!(parent.index() < node.id().index());
                    assert!(span.start as usize <= parent.index());
                    assert!(parent.index() < span.end as usize);
                }
            }
        }
        next_node = span.end;
    }
    assert_eq!(next_node as usize, execution.nodes().len());
    assert!(seen_sources.into_iter().all(|seen| seen));
}

#[test]
fn mixed_property_inputs_reconstruct_complete_generic_receivers() {
    for shape in [
        ScrollInterleaveFixtureShape::FrameRootScroll,
        ScrollInterleaveFixtureShape::TransformScroll,
        ScrollInterleaveFixtureShape::EffectScroll,
        ScrollInterleaveFixtureShape::TransformEffectScroll,
        ScrollInterleaveFixtureShape::EffectTransformScroll,
        ScrollInterleaveFixtureShape::EffectNeutralTransformNeutralScroll,
        ScrollInterleaveFixtureShape::CoLocatedTransformScroll,
        ScrollInterleaveFixtureShape::NestedScroll,
    ] {
        let (arena, root, properties, generations) = property_scroll_interleave_fixture(shape);
        let artifact =
            stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
                .unwrap();
        let dag = reconstruct_artifact_surface_dag(&artifact);
        assert_eq!(
            dag.nodes().len(),
            properties.transforms.len() + properties.effects.len() + properties.scrolls.len()
        );
        for node in dag.nodes() {
            match node.kind() {
                SurfaceDagNodeKind::Transform(transform) => {
                    assert_eq!(properties.transforms[&transform].owner, node.target())
                }
                SurfaceDagNodeKind::Effect(effect) => {
                    assert_eq!(properties.effects[&effect].owner, node.target())
                }
                SurfaceDagNodeKind::ScrollContent { scroll, .. } => {
                    assert_eq!(properties.scrolls[&scroll].owner, node.target())
                }
            }
        }
        let execution = dag.derive_execution_order().unwrap();
        assert_execution_order_contract(&dag, &execution);
    }
}

#[test]
fn stage_c_native_forest_reconstructs_branch_and_multi_root_receivers() {
    let (arena, roots, properties, generations) = native_scroll_forest_plan_fixture();
    let artifact =
        stage_c_classification_artifact_fixture(&arena, &roots, &properties, &generations).unwrap();
    let dag = reconstruct_artifact_surface_dag(&artifact);
    assert_eq!(dag.nodes().len(), properties.scrolls.len());
    assert_eq!(dag.roots().len(), 2);
    let mut nested = 0;
    let mut parent_counts = std::collections::HashMap::new();
    for node in dag.nodes() {
        assert!(
            matches!(node.kind(),SurfaceDagNodeKind::ScrollContent{scroll,..} if properties.scrolls[&scroll].owner==node.target())
        );
        let mut parent = arena.parent_of(node.target());
        while parent.is_some_and(|owner| !properties.scrolls.contains_key(&ScrollNodeId(owner))) {
            parent = arena.parent_of(parent.unwrap());
        }
        if let Some(parent) = parent {
            let receiver = dag.receiver_node(node.receiver()).unwrap().unwrap();
            assert_eq!(receiver.target(), parent);
            nested += 1;
            *parent_counts.entry(parent).or_insert(0) += 1;
        } else {
            assert!(node.receiver().scene_root_ordinal().is_some());
        }
    }
    assert!(nested >= 3);
    assert!(parent_counts.values().any(|count| *count >= 2));
    assert_execution_order_contract(&dag, &dag.derive_execution_order().unwrap());
}

#[test]
fn stage_c_surface_dag_rejects_misaligned_consumption_with_a_closed_taxonomy() {
    fn error_name(error: SurfaceDagError) -> &'static str {
        match error {
            SurfaceDagError::Transition(_) => "transition",
            SurfaceDagError::MissingMaterializationSnapshot(_) => {
                "missing-materialization-snapshot"
            }
            SurfaceDagError::MissingScrollContentsClip { .. } => "missing-scroll-contents-clip",
            SurfaceDagError::TransitionCount { .. } => "transition-count",
            SurfaceDagError::TransitionSceneRoot { .. } => "transition-scene-root",
            SurfaceDagError::TransitionTarget { .. } => "transition-target",
            SurfaceDagError::TransitionCursor { .. } => "transition-cursor",
            SurfaceDagError::TransitionKind { .. } => "transition-kind",
            SurfaceDagError::ArtifactTransitionDerivationStalled { .. } => {
                "artifact-transition-derivation-stalled"
            }
            SurfaceDagError::ConflictingArtifactTransition { .. } => {
                "conflicting-artifact-transition"
            }
            SurfaceDagError::ArtifactTransitionTerminalMismatch { .. } => {
                "artifact-transition-terminal-mismatch"
            }
            SurfaceDagError::MissingArtifactTransition { .. } => "missing-artifact-transition",
            SurfaceDagError::NonReceiverClosedChunkSurfaceChain { .. } => {
                "non-receiver-closed-chunk-surface-chain"
            }
            SurfaceDagError::ClipRebaseOutsideBoundary { .. } => "clip-rebase-outside-boundary",
            SurfaceDagError::SurfaceNodeOrdinalOverflow(_) => "surface-node-ordinal-overflow",
            SurfaceDagError::ExecutionNodeOrdinalOverflow(_) => "execution-node-ordinal-overflow",
            SurfaceDagError::UnknownSceneRootReceiver(_) => "unknown-scene-root-receiver",
            SurfaceDagError::UnknownSurfaceReceiver(_) => "unknown-surface-receiver",
            SurfaceDagError::CyclicSurfaceReceiver(_) => "cyclic-surface-receiver",
            SurfaceDagError::MaterializationCoverageCount { .. } => {
                "materialization-coverage-count"
            }
            SurfaceDagError::MissingMaterializedDescendant(_) => "missing-materialized-descendant",
            SurfaceDagError::InvalidScrollMaskScope { .. } => "invalid-scroll-mask-scope",
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
        LayerizationPolicy::ResolveMaterializedTargets,
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
            LayerizationPolicy::ResolveMaterializedTargets,
        ),
        Err(SurfaceDagError::TransitionKind {
            index: 0,
            expected: SurfaceDagNodeKind::Transform(TransformNodeId(root)),
        }),
        "co-located target/cursor equality cannot hide a kind-order mismatch",
    );
}

#[test]
fn stage_c_execution_order_remaps_a_leaf_first_forward_receiver() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::TransformScroll);
    let mut artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("leaf-first execution artifact");
    reorder_artifact_leaf_first(&arena, &mut artifact);
    let surface_dag = reconstruct_artifact_surface_dag(&artifact);
    let execution = surface_dag
        .derive_execution_order()
        .expect("leaf-first execution order");
    assert_execution_order_contract(&surface_dag, &execution);

    let [scroll, transform] = surface_dag.nodes() else {
        panic!("leaf-first transform-scroll must retain two source nodes")
    };
    assert_eq!(
        execution
            .nodes()
            .iter()
            .map(|node| node.source())
            .collect::<Vec<_>>(),
        vec![transform.id(), scroll.id()],
    );
    assert_eq!(
        execution.nodes()[1].receiver(),
        SurfaceDagExecutionTargetId::Surface(execution.nodes()[0].id()),
    );
}

#[test]
fn stage_c_execution_order_remaps_a_leaf_first_nested_scroll_chain() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::NestedScroll);
    let mut artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("leaf-first nested execution artifact");
    reorder_artifact_leaf_first(&arena, &mut artifact);
    let surface_dag = reconstruct_artifact_surface_dag(&artifact);
    let execution = surface_dag
        .derive_execution_order()
        .expect("leaf-first nested execution order");
    assert_execution_order_contract(&surface_dag, &execution);

    let [inner, outer] = surface_dag.nodes() else {
        panic!("leaf-first nested scroll must retain two source nodes")
    };
    assert_eq!(
        execution
            .nodes()
            .iter()
            .map(|node| node.source())
            .collect::<Vec<_>>(),
        vec![outer.id(), inner.id()],
    );
    assert_eq!(
        execution.nodes()[1].receiver(),
        SurfaceDagExecutionTargetId::Surface(execution.nodes()[0].id()),
    );
}

#[test]
fn stage_c_execution_order_preserves_multi_root_preorder_and_sibling_tie_break() {
    let (arena, roots, properties, generations) = native_scroll_forest_plan_fixture();
    let artifact =
        stage_c_classification_artifact_fixture(&arena, &roots, &properties, &generations)
            .expect("native execution artifact");
    let surface_dag = reconstruct_artifact_surface_dag(&artifact);
    let execution = surface_dag
        .derive_execution_order()
        .expect("native execution order");
    assert_execution_order_contract(&surface_dag, &execution);

    assert_eq!(
        execution
            .roots()
            .iter()
            .map(|root| root.node_span())
            .collect::<Vec<_>>(),
        vec![0..4, 4..6],
    );
    let parent = execution
        .execution_id(surface_dag.nodes()[1].id())
        .expect("shared branch parent execution id");
    let left = execution
        .execution_id(surface_dag.nodes()[2].id())
        .expect("left sibling execution id");
    let right = execution
        .execution_id(surface_dag.nodes()[3].id())
        .expect("right sibling execution id");
    assert!(left.index() < right.index());
    assert_eq!(
        [
            execution.nodes()[left.index()].receiver(),
            execution.nodes()[right.index()].receiver()
        ],
        [
            SurfaceDagExecutionTargetId::Surface(parent),
            SurfaceDagExecutionTargetId::Surface(parent),
        ],
        "siblings keep source-ID order while naming the same earlier parent",
    );
}

#[test]
fn stage_c_execution_order_retains_empty_plain_root_spans() {
    let (mut arena, native_roots, mut properties, mut generations) =
        native_scroll_forest_plan_fixture();
    let plain_before = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xc3_5201, 0.0, 0.0, 20.0, 20.0,
    ))));
    let plain_between = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xc3_5202, 0.0, 0.0, 20.0, 20.0,
    ))));
    let plain_after = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xc3_5203, 0.0, 0.0, 20.0, 20.0,
    ))));
    let roots = vec![
        plain_before,
        native_roots[0],
        plain_between,
        native_roots[1],
        plain_after,
    ];
    properties.sync(&arena, &roots);
    generations.sync(&arena, &roots, &properties);
    let artifact =
        stage_c_classification_artifact_fixture(&arena, &roots, &properties, &generations)
            .expect("plain-root execution artifact");
    let surface_dag = reconstruct_artifact_surface_dag(&artifact);
    let execution = surface_dag
        .derive_execution_order()
        .expect("plain-root execution order");
    assert_execution_order_contract(&surface_dag, &execution);

    assert_eq!(
        execution
            .roots()
            .iter()
            .map(|root| (root.identity().target(), root.node_span()))
            .collect::<Vec<_>>(),
        vec![
            (plain_before, 0..0),
            (native_roots[0], 0..4),
            (plain_between, 4..4),
            (native_roots[1], 4..6),
            (plain_after, 6..6),
        ],
    );
}

#[test]
fn stage_c_surface_dag_retains_plain_roots_without_minting_surfaces() {
    let fixture = plain_root_fixture();
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
        LayerizationPolicy::ResolveMaterializedTargets,
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
        LayerizationPolicy::ResolveMaterializedTargets,
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
    let scroll_owner = properties.scrolls.values().next().unwrap().owner;
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
        LayerizationPolicy::ResolveMaterializedTargets,
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
        LayerizationPolicy::ResolveMaterializedTargets,
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

pub(super) struct PlainRootFixture {
    pub(super) arena: NodeArena,
    pub(super) roots: Vec<NodeKey>,
    pub(super) property_roots: [NodeKey; 2],
    pub(super) properties: PropertyTrees,
    pub(super) generations: PaintGenerationTracker,
}

fn root_element(stable_id: u64, color: Color) -> Element {
    let mut element = Element::new_with_id(stable_id, 0.0, 0.0, 104.0, 78.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

pub(super) fn plain_root_fixture() -> PlainRootFixture {
    let mut arena = new_test_arena();
    let plain_before = commit_element(
        &mut arena,
        Box::new(root_element(0xf5_3101, Color::rgb(35, 75, 115))),
    );
    let property_a = commit_element(
        &mut arena,
        Box::new(root_element(0xf5_3102, Color::rgb(25, 55, 95))),
    );
    let child_a = commit_child(
        &mut arena,
        property_a,
        Box::new(root_element(0xf5_3103, Color::rgb(165, 65, 35))),
    );
    let plain_between = commit_element(
        &mut arena,
        Box::new(root_element(0xf5_3104, Color::rgb(45, 125, 85))),
    );
    let property_b = commit_element(
        &mut arena,
        Box::new(root_element(0xf5_3105, Color::rgb(125, 75, 155))),
    );
    let child_b = commit_child(
        &mut arena,
        property_b,
        Box::new(root_element(0xf5_3106, Color::rgb(55, 135, 175))),
    );
    let plain_after = commit_element(
        &mut arena,
        Box::new(root_element(0xf5_3107, Color::rgb(145, 95, 45))),
    );
    let roots = vec![
        plain_before,
        property_a,
        plain_between,
        property_b,
        plain_after,
    ];
    let (measure, place) = {
        let constraints = LayoutConstraints {
            max_width: 360.0,
            max_height: 260.0,
            viewport_width: 360.0,
            viewport_height: 260.0,
            percent_base_width: Some(360.0),
            percent_base_height: Some(260.0),
        };
        let placement = LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 360.0,
            available_height: 260.0,
            viewport_width: 360.0,
            viewport_height: 260.0,
            percent_base_width: Some(360.0),
            percent_base_height: Some(260.0),
        };
        (constraints, placement)
    };
    for &root in &roots {
        measure_and_place(&mut arena, root, measure, place);
    }
    crate::view::test_support::get_element_mut::<Element>(&arena, property_a)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            2.0, 1.0, 0.0,
        ))));
    crate::view::test_support::get_element_mut::<Element>(&arena, child_a).set_opacity(0.57);
    crate::view::test_support::get_element_mut::<Element>(&arena, property_b).set_opacity(0.63);
    crate::view::test_support::get_element_mut::<Element>(&arena, child_b)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            4.0, 1.0, 0.0,
        ))));
    for &root in &roots {
        arena.refresh_subtree_dirty_cache(root);
    }
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    PlainRootFixture {
        arena,
        roots,
        property_roots: [property_a, property_b],
        properties,
        generations,
    }
}
