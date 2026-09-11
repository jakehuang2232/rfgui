//! C1c classification gate over the complete C0c capability corpus.
//!
//! This gate proves that snapshot closure is sufficient to validate each
//! supplied six-dimensional identity edge and that artifact-derived cursors
//! agree with the first structural chunk of each target subtree. It does not
//! derive either side of an edge from the artifact; C2 owns that work.
//!
//! Request streams are legal when their first-subtree chunk indices are
//! non-decreasing. C1 intentionally does not require DFS preorder beyond that
//! check. Planner sequence and artifact cursor remain independent domains;
//! C1b compares their results but does not equate their ordinals.

use std::collections::BTreeSet;

use super::*;
use crate::view::compositor::property_tree::{
    LayoutPositionNodeId, PropertyStateTransition, SpatialPositionReference,
    SpatialProjectionError, VisualOffsetNodeId,
};
use crate::view::paint::{
    ArtifactTransitionRequest, ClassifiedTransitionEvent, TransitionError,
    classify_artifact_transition_sequence,
};

fn classify_fixture(
    cases: &[StageCScrollCapabilityCase],
    observed: &mut BTreeSet<StageCScrollCapabilityCase>,
    arena: &NodeArena,
    roots: &[NodeKey],
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
    requests: &[ArtifactTransitionRequest],
) -> (PaintArtifact, Vec<ClassifiedTransitionEvent>) {
    let artifact = stage_c_classification_artifact_fixture(arena, roots, properties, generations)
        .expect("C1c structural artifact must close over the synced property trees");
    let events = classify_closed_artifact(cases, observed, arena, &artifact, requests);
    (artifact, events)
}

fn classify_closed_artifact(
    cases: &[StageCScrollCapabilityCase],
    observed: &mut BTreeSet<StageCScrollCapabilityCase>,
    arena: &NodeArena,
    artifact: &PaintArtifact,
    requests: &[ArtifactTransitionRequest],
) -> Vec<ClassifiedTransitionEvent> {
    let events = classify_artifact_transition_sequence(artifact, requests)
        .unwrap_or_else(|error| panic!("{cases:?} capability edge must classify: {error:?}"));
    assert_stage_c_classified_cursors_match_artifact_traversal(arena, artifact, &events);
    for case in cases {
        assert!(
            observed.insert(*case),
            "a C0c capability case may be classified only once: {}",
            case.label(),
        );
    }
    events
}

fn owner_state_edge(
    properties: &PropertyTrees,
    owner: NodeKey,
) -> (PropertyTreeState, PropertyTreeState) {
    let state = properties
        .node_state_for(owner)
        .expect("C1c owner property state");
    (state.descendants, state.paint)
}

fn assert_exact_transition(
    event: ClassifiedTransitionEvent,
    target: NodeKey,
    from: PropertyTreeState,
    to: PropertyTreeState,
) {
    assert_eq!(event.target(), target);
    assert_eq!(
        event.transition(),
        PropertyStateTransition::between(from, to)
    );
}

#[test]
fn stage_c_all_eleven_scroll_capabilities_classify_from_closed_artifacts() {
    let mut observed = BTreeSet::new();

    let (arena, roots, _, fourth, properties, generations) = stage_c_depth_four_scroll_fixture();
    let (from, to) = owner_state_edge(&properties, fourth);
    let (_, events) = classify_fixture(
        &[StageCScrollCapabilityCase::DepthFour],
        &mut observed,
        &arena,
        &roots,
        &properties,
        &generations,
        &[ArtifactTransitionRequest::new(fourth, from, to)],
    );
    let [event] = events.as_slice() else {
        panic!("depth-four must classify one explicit scroll edge")
    };
    assert_exact_transition(*event, fourth, from, to);
    assert_eq!(event.transition().scroll.from, Some(ScrollNodeId(fourth)));
    assert_eq!(event.transition().scroll.to, to.scroll);

    let (arena, roots, properties, generations) = native_scroll_forest_plan_fixture();
    let forest = fixture_scroll_expectations(&arena, &roots, &properties);
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
    let (artifact, events) = classify_fixture(
        &[
            StageCScrollCapabilityCase::NestedScroll,
            StageCScrollCapabilityCase::BranchingSiblings,
            StageCScrollCapabilityCase::HeterogeneousRoots,
            StageCScrollCapabilityCase::ClipCrossingScroll,
            StageCScrollCapabilityCase::InterleavedSiblingOrder,
        ],
        &mut observed,
        &arena,
        &roots,
        &properties,
        &generations,
        &requests,
    );
    assert_eq!(events.len(), forest.boundaries.len());
    for (boundary, event) in forest.boundaries.iter().zip(&events) {
        assert_eq!(event.scene_root_ordinal(), boundary.scene_root_ordinal);
        assert_exact_transition(
            *event,
            boundary.boundary_root,
            boundary.projection.live_input,
            boundary.projection.projected_output,
        );
    }

    let nested_depth = |mut boundary: FixtureScrollId| {
        let mut depth = 1usize;
        while let Some(parent) = forest.boundaries[boundary.0 as usize].parent {
            depth += 1;
            boundary = parent;
        }
        depth
    };
    assert!(
        forest
            .boundaries
            .iter()
            .map(|boundary| nested_depth(boundary.id))
            .max()
            .is_some_and(|depth| depth >= 3),
    );

    let branch_children = forest
        .boundaries
        .iter()
        .filter(|boundary| boundary.parent == Some(FixtureScrollId(1)))
        .map(|boundary| boundary.id)
        .collect::<Vec<_>>();
    assert_eq!(branch_children, [FixtureScrollId(2), FixtureScrollId(3)],);
    let first_branch_cursor = events[branch_children[0].0 as usize].cursor().chunk_index();
    let second_branch_cursor = events[branch_children[1].0 as usize].cursor().chunk_index();
    assert!(first_branch_cursor < second_branch_cursor);

    assert_eq!(
        events
            .iter()
            .map(|event| event.scene_root_ordinal())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([0, 1]),
        "C1c multi-root input must exercise artifact-derived root ordinals",
    );

    let crossing = &forest.boundaries[2];
    let crossing_transition = events[2].transition();
    assert_eq!(
        crossing_transition.clip.from,
        crossing.projection.live_input.clip
    );
    assert_eq!(
        crossing_transition.clip.to,
        crossing.projection.projected_output.clip,
    );
    assert!(crossing_transition.clip.is_changed());

    assert!(
        artifact.chunks[first_branch_cursor + 1..second_branch_cursor]
            .iter()
            .any(|chunk| {
                !forest
                    .boundaries
                    .iter()
                    .any(|boundary| boundary.boundary_root == chunk.owner)
            }),
        "ordinary sibling chunks must remain between classified child boundaries",
    );

    let (arena, root, properties, generations) = same_owner_transform_effect_scroll_roles_fixture();
    // Explicit consumption requests from the authored owner's three property
    // snapshots. No historical planner supplies a cursor or transition.
    let mut state = properties.node_state_for(root).unwrap().descendants;
    let mut requests = Vec::new();
    let mut next = state;
    let scroll = properties.scrolls[&ScrollNodeId(root)];
    next.scroll = scroll.parent;
    next.clip = properties.clips[&ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    }]
        .parent;
    requests.push(ArtifactTransitionRequest::new(root, state, next));
    state = next;
    next.effect = properties.effects[&EffectNodeId(root)].parent;
    requests.push(ArtifactTransitionRequest::new(root, state, next));
    state = next;
    next.transform = properties.transforms[&TransformNodeId(root)].parent;
    requests.push(ArtifactTransitionRequest::new(root, state, next));
    let (_, events) = classify_fixture(
        &[StageCScrollCapabilityCase::CoLocatedProperties],
        &mut observed,
        &arena,
        &[root],
        &properties,
        &generations,
        &requests,
    );
    assert!(events.len() >= 3, "co-located T/E/S keeps three identities");
    assert!(events.iter().all(|event| event.target() == root));
    assert!(
        events
            .windows(2)
            .all(|pair| pair[0].cursor() == pair[1].cursor()),
        "co-located identities legally share one first-subtree cursor",
    );

    let (arena, root, anchor, child, properties, generations) =
        stage_c_named_anchor_classification_fixture();
    let (from, to) = owner_state_edge(&properties, child);
    let (artifact, events) = classify_fixture(
        &[StageCScrollCapabilityCase::NamedAnchor],
        &mut observed,
        &arena,
        &[root],
        &properties,
        &generations,
        &[ArtifactTransitionRequest::new(child, from, to)],
    );
    let [event] = events.as_slice() else {
        panic!("named anchor must classify one supplied edge")
    };
    assert_exact_transition(*event, child, from, to);
    assert!(artifact.layout_position_nodes.iter().any(|snapshot| {
        snapshot.id == LayoutPositionNodeId(child)
            && snapshot.reference == SpatialPositionReference::Anchor(anchor)
    }));
    assert!(
        artifact
            .visual_offset_nodes
            .iter()
            .any(|snapshot| snapshot.id == VisualOffsetNodeId(anchor)),
        "named-anchor auxiliary visual closure must reach C1",
    );

    let (arena, root, child, properties, generations) =
        stage_c_layout_position_reference_scroll_fixture();
    let (from, to) = owner_state_edge(&properties, child);
    let (artifact, events) = classify_fixture(
        &[StageCScrollCapabilityCase::LayoutPositionReferenceScroll],
        &mut observed,
        &arena,
        &[root],
        &properties,
        &generations,
        &[ArtifactTransitionRequest::new(child, from, to)],
    );
    let [event] = events.as_slice() else {
        panic!("reference-scroll must classify one supplied edge")
    };
    assert_exact_transition(*event, child, from, to);
    assert!(artifact.layout_position_nodes.iter().any(|snapshot| {
        snapshot.id == LayoutPositionNodeId(child)
            && snapshot.reference_scroll == Some(ScrollNodeId(root))
    }));

    let (arena, root, mut properties, mut generations) =
        scroll_content_effect_interleave_fixture(false, true);
    let content = arena.children_of(root)[0];
    let baseline = properties
        .node_state_for(content)
        .expect("C1c baseline scroll content")
        .paint;
    let baseline_offset =
        apply_stage_c_scroll_offset_delta(&arena, root, &mut properties, &mut generations, 7.0);
    let moved = properties
        .node_state_for(content)
        .expect("C1c moved scroll content")
        .paint;
    assert_eq!(
        baseline, moved,
        "offset delta preserves property identities"
    );
    assert_ne!(
        properties
            .scroll_snapshot_for(ScrollNodeId(root))
            .expect("C1c moved scroll snapshot")
            .offset,
        baseline_offset,
    );
    let (_, events) = classify_fixture(
        &[StageCScrollCapabilityCase::ScrollOffsetDelta],
        &mut observed,
        &arena,
        &[root],
        &properties,
        &generations,
        &[ArtifactTransitionRequest::new(content, baseline, moved)],
    );
    let [event] = events.as_slice() else {
        panic!("scroll offset must classify one identity-stable edge")
    };
    assert_exact_transition(*event, content, baseline, moved);

    let (arena, roots, properties, generations) = stage_c_visible_overlay_fixture();
    let FrameArtifactRecordOutcome::Artifact { artifact, .. } = record_surface_dag_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .unwrap() else {
        panic!("visible overlay must record")
    };
    // Keep the complete owner/chunk closure; classify only the after-children overlay edges.
    let overlay = artifact;
    // AfterChildren commands appear in postorder, but transition requests
    // are ordered by the first structural chunk of each owner subtree.
    // Author the request order from arena preorder, independently of cursors.
    let overlays = overlay
        .chunks
        .iter()
        .filter(|chunk| chunk.id.role == PaintChunkRole::ScrollbarOverlay)
        .map(|chunk| (chunk.owner, chunk.properties))
        .collect::<rustc_hash::FxHashMap<_, _>>();
    let mut stack = roots.iter().rev().copied().collect::<Vec<_>>();
    let mut requests = Vec::new();
    while let Some(owner) = stack.pop() {
        if let Some(&state) = overlays.get(&owner) {
            requests.push(ArtifactTransitionRequest::new(owner, state, state));
        }
        stack.extend(arena.children_of(owner).into_iter().rev());
    }
    let events = classify_closed_artifact(
        &[StageCScrollCapabilityCase::OverlayPhase],
        &mut observed,
        &arena,
        &overlay,
        &requests,
    );
    assert!(!events.is_empty());
    assert!(
        overlay
            .chunks
            .iter()
            .filter(|chunk| chunk.id.role == PaintChunkRole::ScrollbarOverlay)
            .all(|chunk| chunk.id.phase == PaintNodePhase::AfterChildren),
    );
    assert!(
        events
            .windows(2)
            .all(|pair| { pair[0].cursor().chunk_index() <= pair[1].cursor().chunk_index() })
    );
    assert_eq!(
        overlay.chunks.last().map(|chunk| chunk.op_range.end),
        Some(overlay.ops.len()),
    );
    assert_eq!(
        observed,
        STAGE_C_SCROLL_CAPABILITY_CASES.into_iter().collect(),
        "C1 closes only when every C0c capability body reaches classification",
    );
}

#[test]
fn stage_c_capability_rejections_preserve_typed_reason_and_owner() {
    let (arena, root, anchor, child, properties, generations) =
        stage_c_named_anchor_classification_fixture();
    let (from, to) = owner_state_edge(&properties, child);
    let request = [ArtifactTransitionRequest::new(child, from, to)];
    let mut artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("C1c named-anchor artifact");
    artifact
        .layout_position_nodes
        .retain(|snapshot| snapshot.id != LayoutPositionNodeId(anchor));
    assert_eq!(
        classify_artifact_transition_sequence(&artifact, &request),
        Err(TransitionError::SpatialSnapshot(
            SpatialProjectionError::MissingLayoutPosition(LayoutPositionNodeId(anchor)),
        )),
        "named-anchor closure rejection keeps the exact missing owner",
    );

    let (mut arena, root, child, properties, generations) =
        stage_c_layout_position_reference_scroll_fixture();
    let (from, to) = owner_state_edge(&properties, child);
    let request = [ArtifactTransitionRequest::new(child, from, to)];
    let mut artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("C1c reference-scroll artifact");
    artifact
        .scroll_nodes
        .retain(|snapshot| snapshot.id != ScrollNodeId(root));
    assert_eq!(
        classify_artifact_transition_sequence(&artifact, &request),
        Err(TransitionError::SpatialSnapshot(
            SpatialProjectionError::MissingScroll(ScrollNodeId(root)),
        )),
        "reference-scroll closure rejection keeps the exact missing owner",
    );

    let outside = arena.insert(Node::new(Box::new(Element::new_with_id(
        0xc1c0_ffff,
        0.0,
        0.0,
        1.0,
        1.0,
    ))));
    let artifact =
        stage_c_classification_artifact_fixture(&arena, &[root], &properties, &generations)
            .expect("C1c closed owner artifact");
    assert_eq!(
        classify_artifact_transition_sequence(
            &artifact,
            &[ArtifactTransitionRequest::new(
                outside,
                PropertyTreeState::default(),
                PropertyTreeState::default(),
            )],
        ),
        Err(TransitionError::UnknownTarget(outside)),
        "request rejection keeps the exact owner outside the artifact",
    );
}

// Expected parent/root relationships are read from the authored arena tree;
// transitions are the input owner's live descendants-to-paint snapshots.
// This fixture data neither records commands nor plans executable surfaces.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct FixtureScrollId(usize);
struct FixtureProjection {
    live_input: PropertyTreeState,
    projected_output: PropertyTreeState,
}
struct FixtureBoundary {
    id: FixtureScrollId,
    parent: Option<FixtureScrollId>,
    scene_root_ordinal: u32,
    boundary_root: NodeKey,
    projection: FixtureProjection,
}
struct FixtureForest {
    boundaries: Vec<FixtureBoundary>,
}
fn fixture_scroll_expectations(
    arena: &NodeArena,
    roots: &[NodeKey],
    properties: &PropertyTrees,
) -> FixtureForest {
    let mut boundaries = Vec::new();
    for (ordinal, &root) in roots.iter().enumerate() {
        let mut stack = vec![(root, None)];
        while let Some((owner, parent)) = stack.pop() {
            let next = if properties.scrolls.contains_key(&ScrollNodeId(owner)) {
                let state = properties.node_state_for(owner).unwrap();
                let id = FixtureScrollId(boundaries.len());
                boundaries.push(FixtureBoundary {
                    id,
                    parent,
                    scene_root_ordinal: ordinal as u32,
                    boundary_root: owner,
                    projection: FixtureProjection {
                        live_input: state.descendants,
                        projected_output: state.paint,
                    },
                });
                Some(id)
            } else {
                parent
            };
            stack.extend(
                arena
                    .children_of(owner)
                    .into_iter()
                    .rev()
                    .map(|child| (child, next)),
            );
        }
    }
    FixtureForest { boundaries }
}
