//! Hierarchical artifact coverage before the Surface DAG executor is wired.

use super::*;
use crate::view::compositor::property_tree::{
    ClipBehavior, ClipNodeId, ClipNodeRole, ClipNodeSnapshot, PropertyTreeState,
};
use crate::view::paint::{
    ArtifactSurfaceCoverageForest, ArtifactSurfaceCoverageStep, SurfaceDag, SurfaceDagError,
    SurfaceDagNodeId, SurfaceDagNodeKind, SurfaceDagTargetId,
    classify_artifact_transition_sequence, reconstruct_surface_dag,
};

fn reconstruct(artifact: &PaintArtifact) -> SurfaceDag {
    let requests = derive_artifact_surface_transition_requests(
        artifact,
        LayerizationPolicy::ResolveMaterializedTargets,
    )
    .expect("coverage transition requests");
    let events = classify_artifact_transition_sequence(artifact, &requests)
        .expect("coverage transition events");
    reconstruct_surface_dag(
        artifact,
        &events,
        LayerizationPolicy::ResolveMaterializedTargets,
    )
    .expect("coverage Surface DAG")
}

fn coverage(artifact: &PaintArtifact, dag: &SurfaceDag) -> ArtifactSurfaceCoverageForest {
    derive_artifact_surface_coverage_forest(
        artifact,
        dag,
        LayerizationPolicy::ResolveMaterializedTargets,
    )
    .expect("artifact surface coverage")
}

fn direct_chunk_indices(forest: &ArtifactSurfaceCoverageForest) -> Vec<usize> {
    forest
        .roots()
        .iter()
        .flat_map(|root| root.steps())
        .chain(forest.nodes().iter().flat_map(|node| node.steps()))
        .filter_map(|step| match step {
            ArtifactSurfaceCoverageStep::ArtifactSpan(span) => Some(span.chunk_range()),
            ArtifactSurfaceCoverageStep::NestedSurface(_) => None,
        })
        .flat_map(|range| range)
        .collect()
}

fn steps_for<'a>(
    forest: &'a ArtifactSurfaceCoverageForest,
    receiver: SurfaceDagTargetId,
) -> &'a [ArtifactSurfaceCoverageStep] {
    match receiver {
        SurfaceDagTargetId::SceneRoot(root) => forest.roots()[root.index()].steps(),
        SurfaceDagTargetId::Surface(surface) => forest.nodes()[surface.index()].steps(),
    }
}

#[test]
fn coverage_step_taxonomy_is_exactly_artifact_span_or_nested_surface() {
    fn step_name(step: &ArtifactSurfaceCoverageStep) -> &'static str {
        match step {
            ArtifactSurfaceCoverageStep::ArtifactSpan(_) => "artifact-span",
            ArtifactSurfaceCoverageStep::NestedSurface(_) => "nested-surface",
        }
    }

    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::TransformScroll);
    let artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("two-step coverage fixture");
    let dag = reconstruct(&artifact);
    let forest = coverage(&artifact, &dag);
    let names = forest
        .roots()
        .iter()
        .flat_map(|root| root.steps())
        .chain(forest.nodes().iter().flat_map(|node| node.steps()))
        .map(step_name)
        .collect::<FxHashSet<_>>();
    assert_eq!(
        names,
        FxHashSet::from_iter(["artifact-span", "nested-surface"])
    );
}

#[test]
fn co_located_surfaces_nest_but_each_chunk_is_directly_owned_once() {
    let (arena, root, properties, generations) = same_owner_transform_effect_scroll_roles_fixture();
    let artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("co-located coverage fixture");
    let dag = reconstruct(&artifact);
    let forest = coverage(&artifact, &dag);
    assert_eq!(dag.nodes().len(), 3);
    assert_eq!(direct_chunk_indices(&forest).len(), artifact.chunks.len());
    assert_eq!(
        direct_chunk_indices(&forest)
            .into_iter()
            .collect::<FxHashSet<_>>()
            .len(),
        artifact.chunks.len(),
    );

    for pair in dag.nodes().windows(2) {
        let child = pair[1].id();
        assert_eq!(
            pair[1].receiver(),
            SurfaceDagTargetId::Surface(pair[0].id())
        );
        assert!(steps_for(&forest, pair[1].receiver()).iter().any(
            |step| matches!(step, ArtifactSurfaceCoverageStep::NestedSurface(id) if *id == child)
        ));
    }
}

#[test]
fn parent_coverage_preserves_before_nested_after_painter_order() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::TransformScroll);
    let mut artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("before-after coverage fixture");
    let mut after = artifact.chunks[0].clone();
    after.id.phase = PaintNodePhase::AfterChildren;
    after.id.slot = 1;
    artifact.chunks.push(after);
    let dag = reconstruct(&artifact);
    let forest = coverage(&artifact, &dag);
    let transform = dag
        .nodes()
        .iter()
        .find(|node| matches!(node.kind(), SurfaceDagNodeKind::Transform(_)))
        .expect("parent transform");
    let steps = forest.nodes()[transform.id().index()].steps();
    assert!(matches!(
        steps,
        [
            ArtifactSurfaceCoverageStep::ArtifactSpan(_),
            ArtifactSurfaceCoverageStep::NestedSurface(_),
            ArtifactSurfaceCoverageStep::ArtifactSpan(_),
        ]
    ));
    let ArtifactSurfaceCoverageStep::ArtifactSpan(after) = &steps[2] else {
        unreachable!()
    };
    assert_eq!(
        after.chunk_range(),
        artifact.chunks.len() - 1..artifact.chunks.len()
    );
}

#[test]
fn sibling_surfaces_follow_artifact_cursor_order_without_cross_ownership() {
    let (arena, roots, properties, generations) = native_scroll_forest_plan_fixture();
    let artifact =
        stage_c_classification_artifact_fixture(&arena, &roots, &properties, &generations)
            .expect("sibling coverage fixture");
    let dag = reconstruct(&artifact);
    let forest = coverage(&artifact, &dag);
    let mut siblings = FxHashMap::<SurfaceDagTargetId, Vec<SurfaceDagNodeId>>::default();
    for node in dag.nodes() {
        siblings.entry(node.receiver()).or_default().push(node.id());
    }
    let (receiver, children) = siblings
        .into_iter()
        .find(|(_, children)| children.len() >= 2)
        .expect("fixture owns sibling surfaces");
    let observed = steps_for(&forest, receiver)
        .iter()
        .filter_map(|step| match step {
            ArtifactSurfaceCoverageStep::NestedSurface(child) if children.contains(child) => {
                Some(*child)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut expected = children;
    expected.sort_by_key(|id| dag.nodes()[id.index()].cursor().chunk_index());
    assert_eq!(observed, expected);
    assert_eq!(direct_chunk_indices(&forest).len(), artifact.chunks.len());
}

#[test]
fn plain_roots_remain_direct_artifact_spans() {
    let (arena, roots, _, _, properties, generations) = stage_c_depth_four_scroll_fixture();
    let mut artifact =
        stage_c_classification_artifact_fixture(&arena, &roots, &properties, &generations)
            .expect("plain-root coverage fixture");
    artifact.transform_nodes.clear();
    artifact.effect_nodes.clear();
    artifact.scroll_nodes.clear();
    artifact.clip_nodes.clear();
    for endpoints in &mut artifact.owner_property_states {
        endpoints.paint = PropertyTreeState::default();
        endpoints.descendants = PropertyTreeState::default();
    }
    for chunk in &mut artifact.chunks {
        chunk.properties = PropertyTreeState::default();
    }
    let dag = reconstruct(&artifact);
    let forest = coverage(&artifact, &dag);
    assert!(forest.nodes().is_empty());
    assert_eq!(
        direct_chunk_indices(&forest),
        (0..artifact.chunks.len()).collect::<Vec<_>>()
    );
    assert!(forest.roots().iter().all(|root| {
        root.steps()
            .iter()
            .all(|step| matches!(step, ArtifactSurfaceCoverageStep::ArtifactSpan(_)))
    }));
}

#[test]
fn forward_receiver_remap_does_not_replace_artifact_painter_order() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::TransformScroll);
    let mut artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("forward receiver coverage fixture");
    super::stage_c_surface_dag_tests::reorder_artifact_leaf_first(&arena, &mut artifact);
    let dag = reconstruct(&artifact);
    assert!(dag.nodes().iter().any(|node| match node.receiver() {
        SurfaceDagTargetId::Surface(receiver) => receiver.index() > node.id().index(),
        SurfaceDagTargetId::SceneRoot(_) => false,
    }));
    let execution = dag
        .derive_execution_order()
        .expect("forward execution remap");
    assert_ne!(
        execution
            .nodes()
            .iter()
            .map(|node| node.source())
            .collect::<Vec<_>>(),
        dag.nodes().iter().map(|node| node.id()).collect::<Vec<_>>(),
    );
    let forest = coverage(&artifact, &dag);
    let mut painter = direct_chunk_indices(&forest);
    painter.sort_unstable();
    assert_eq!(painter, (0..artifact.chunks.len()).collect::<Vec<_>>());
}

#[test]
fn scroll_clip_closure_keeps_empty_as_neither_and_unions_siblings_in_painter_order() {
    let (arena, root, properties, generations) =
        property_scroll_interleave_fixture(ScrollInterleaveFixtureShape::FrameRootScroll);
    let artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("empty clip closure fixture");
    let dag = reconstruct(&artifact);
    let empty = coverage(&artifact, &dag);
    let scroll = dag
        .nodes()
        .iter()
        .find(|node| matches!(node.kind(), SurfaceDagNodeKind::ScrollContent { .. }))
        .expect("scroll surface");
    let empty_closure = empty.nodes()[scroll.id().index()]
        .clip_closure()
        .expect("scroll closure capability");
    assert!(empty_closure.local_clips().is_empty());
    assert_eq!(
        empty_closure.receiver_clip(),
        scroll.clip_rebase().unwrap().receiver_clip()
    );

    let mut siblings = artifact;
    let contents_clip = match scroll.kind() {
        SurfaceDagNodeKind::ScrollContent { contents_clip, .. } => contents_clip,
        _ => unreachable!(),
    };
    let first_clip = ClipNodeId {
        owner: root,
        role: ClipNodeRole::SelfClip,
    };
    siblings.clip_nodes.push(ClipNodeSnapshot {
        id: first_clip,
        owner: root,
        parent: Some(contents_clip),
        logical_scissor: [1, 2, 70, 60],
        behavior: ClipBehavior::Replace,
        generation: 51,
    });
    // A second SelfClip cannot share the same canonical owner/role id, so use
    // an existing descendant owner while retaining the same scroll boundary.
    let sibling_owner = arena.children_of(root)[0];
    let second_clip = ClipNodeId {
        owner: sibling_owner,
        role: ClipNodeRole::SelfClip,
    };
    siblings.clip_nodes.push(ClipNodeSnapshot {
        id: second_clip,
        owner: sibling_owner,
        parent: Some(contents_clip),
        logical_scissor: [3, 4, 50, 40],
        behavior: ClipBehavior::Replace,
        generation: 53,
    });
    let inside = siblings
        .chunks
        .iter_mut()
        .find(|chunk| chunk.properties.scroll == Some(ScrollNodeId(root)))
        .expect("scroll content chunk");
    inside.properties.clip = Some(first_clip);
    let mut second = inside.clone();
    second.id.owner = sibling_owner;
    second.owner = sibling_owner;
    second.id.slot = 1;
    second.properties.clip = Some(second_clip);
    siblings.chunks.push(second);
    let sibling_dag = reconstruct(&siblings);
    let sibling_forest = coverage(&siblings, &sibling_dag);
    let closure = sibling_forest.nodes()[scroll.id().index()]
        .clip_closure()
        .expect("merged scroll closure");
    assert_eq!(
        closure
            .local_clips()
            .iter()
            .map(|clip| clip.id)
            .collect::<Vec<_>>(),
        [first_clip, second_clip],
    );
    assert!(
        closure
            .local_clips()
            .iter()
            .all(|clip| clip.parent.is_none())
    );
    assert_eq!(closure.local_clips()[0].logical_scissor, [1, 2, 70, 60]);
    assert_eq!(closure.local_clips()[0].behavior, ClipBehavior::Replace);
    assert_eq!(closure.local_clips()[0].generation, 51);
    assert_eq!(closure.local_clips()[1].logical_scissor, [3, 4, 50, 40]);
    assert_eq!(closure.local_clips()[1].behavior, ClipBehavior::Replace);
    assert_eq!(closure.local_clips()[1].generation, 53);
}

#[test]
fn transform_effect_transform_ancestry_keeps_the_inner_transform_as_direct_owner() {
    let artifact = stage_c_transform_effect_transform_artifact_fixture();
    let dag = reconstruct(&artifact);
    let forest = coverage(&artifact, &dag);
    let transforms = dag
        .nodes()
        .iter()
        .filter(|node| matches!(node.kind(), SurfaceDagNodeKind::Transform(_)))
        .collect::<Vec<_>>();
    let effect = dag
        .nodes()
        .iter()
        .find(|node| matches!(node.kind(), SurfaceDagNodeKind::Effect(_)))
        .expect("middle effect surface");
    assert_eq!(transforms.len(), 2);
    let outer = transforms
        .iter()
        .copied()
        .find(|node| node.receiver() == SurfaceDagTargetId::SceneRoot(dag.roots()[0].id()))
        .expect("outer transform surface");
    let inner = transforms
        .iter()
        .copied()
        .find(|node| node.receiver() == SurfaceDagTargetId::Surface(effect.id()))
        .expect("inner transform surface");
    assert_eq!(effect.receiver(), SurfaceDagTargetId::Surface(outer.id()));

    let inner_transform = match inner.kind() {
        SurfaceDagNodeKind::Transform(transform) => transform,
        _ => unreachable!(),
    };
    let direct_chunk = artifact
        .chunks
        .iter()
        .position(|chunk| chunk.properties.transform == Some(inner_transform))
        .expect("chunk authored in the inner transform");
    let directly_contains = |surface: SurfaceDagNodeId| {
        forest.nodes()[surface.index()].steps().iter().any(|step| {
            matches!(
                step,
                ArtifactSurfaceCoverageStep::ArtifactSpan(span)
                    if span.chunk_range().contains(&direct_chunk)
            )
        })
    };
    assert!(directly_contains(inner.id()));
    assert!(!directly_contains(effect.id()));
    assert!(!directly_contains(outer.id()));
    assert!(forest.nodes()[outer.id().index()].steps().iter().any(
        |step| matches!(step, ArtifactSurfaceCoverageStep::NestedSurface(id) if *id == effect.id())
    ));
    assert!(forest.nodes()[effect.id().index()].steps().iter().any(
        |step| matches!(step, ArtifactSurfaceCoverageStep::NestedSurface(id) if *id == inner.id())
    ));
}

#[test]
fn coverage_rejects_a_receiver_chain_that_terminates_at_the_wrong_scene_root() {
    let (arena, roots, _, _, properties, generations) = stage_c_depth_four_scroll_fixture();
    let artifact =
        stage_c_classification_artifact_fixture(&arena, &roots, &properties, &generations)
            .expect("multi-root receiver-closure fixture");
    let mut dag = reconstruct(&artifact);
    let outer = dag
        .nodes()
        .iter()
        .find(|node| matches!(node.receiver(), SurfaceDagTargetId::SceneRoot(_)))
        .expect("top-level surface")
        .id();
    let expected_root = match dag.nodes()[outer.index()].receiver() {
        SurfaceDagTargetId::SceneRoot(root) => root,
        SurfaceDagTargetId::Surface(_) => unreachable!(),
    };
    let wrong_root = dag
        .roots()
        .iter()
        .map(|root| root.id())
        .find(|root| *root != expected_root)
        .expect("second scene root");
    dag.set_receiver_for_test(outer, SurfaceDagTargetId::SceneRoot(wrong_root));

    assert!(matches!(
        derive_artifact_surface_coverage_forest(
            &artifact,
            &dag,
            LayerizationPolicy::ResolveMaterializedTargets,
        ),
        Err(SurfaceDagError::NonReceiverClosedChunkSurfaceChain {
            expected_receiver: SurfaceDagTargetId::SceneRoot(root),
            ..
        }) if root == expected_root
    ));
}

#[test]
fn coverage_rejects_matched_ancestry_omitted_from_the_receiver_chain() {
    let artifact = stage_c_transform_effect_transform_artifact_fixture();
    let mut dag = reconstruct(&artifact);
    let effect = dag
        .nodes()
        .iter()
        .find(|node| matches!(node.kind(), SurfaceDagNodeKind::Effect(_)))
        .expect("middle effect surface")
        .id();
    let inner = dag
        .nodes()
        .iter()
        .find(|node| {
            matches!(node.kind(), SurfaceDagNodeKind::Transform(_))
                && node.receiver() == SurfaceDagTargetId::Surface(effect)
        })
        .expect("inner transform surface")
        .id();
    let outer = match dag.nodes()[effect.index()].receiver() {
        SurfaceDagTargetId::Surface(outer) => outer,
        SurfaceDagTargetId::SceneRoot(_) => unreachable!(),
    };
    dag.set_receiver_for_test(inner, SurfaceDagTargetId::Surface(outer));

    assert!(matches!(
        derive_artifact_surface_coverage_forest(
            &artifact,
            &dag,
            LayerizationPolicy::ResolveMaterializedTargets,
        ),
        Err(SurfaceDagError::NonReceiverClosedChunkSurfaceChain {
            surface,
            expected_receiver: SurfaceDagTargetId::Surface(receiver),
            ..
        }) if surface == inner && receiver == outer
    ));
}

#[test]
fn receiver_gap_rejects_before_an_unrepresentable_chunk_is_forced_into_a_span() {
    let (arena, root, properties, generations) = same_owner_transform_effect_scroll_roles_fixture();
    let mut artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("receiver-gap fixture");
    let dag = reconstruct(&artifact);
    let effect = dag
        .nodes()
        .iter()
        .find(|node| matches!(node.kind(), SurfaceDagNodeKind::Effect(_)))
        .expect("effect surface");
    let transform_receiver = effect.receiver();
    let effect_id = match effect.kind() {
        SurfaceDagNodeKind::Effect(effect) => effect,
        _ => unreachable!(),
    };
    artifact.chunks[0].properties = PropertyTreeState {
        effect: Some(effect_id),
        ..PropertyTreeState::default()
    };
    assert_eq!(
        derive_artifact_surface_coverage_forest(
            &artifact,
            &dag,
            LayerizationPolicy::ResolveMaterializedTargets,
        ),
        Err(SurfaceDagError::NonReceiverClosedChunkSurfaceChain {
            chunk_index: 0,
            surface: effect.id(),
            expected_receiver: transform_receiver,
        }),
    );
}
