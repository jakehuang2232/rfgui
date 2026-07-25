//! Diagnostics for a drifted paint generation snapshot.
//!
//! `matches_live_snapshot` gates the only scroll grammar without a blanket
//! transform/effect exclusion, so when it fails the reported node and field
//! are the whole diagnosis.

use super::{LiveSnapshotField, PaintGenerationTracker};
use crate::view::base_component::Element;
use crate::view::compositor::property_tree::PropertyTrees;
use crate::view::node_arena::{Node, NodeArena, NodeKey};

fn observed_scene() -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        1, 0.0, 0.0, 100.0, 100.0,
    ))));
    let child = arena.insert(Node::with_parent(
        Box::new(Element::new_with_id(2, 0.0, 0.0, 50.0, 50.0)),
        Some(root),
    ));
    arena.push_child(root, child);

    let roots = vec![root];
    let property_trees = PropertyTrees::default();
    let mut tracker = PaintGenerationTracker::default();
    tracker.begin_frame(&roots);
    for key in [root, child] {
        let node = arena.get(key).expect("node stays live");
        let parent = node.parent();
        let children = node.children().to_vec();
        tracker.observe_node(
            key,
            parent,
            &children,
            node.element.as_ref(),
            &property_trees,
        );
    }
    (arena, roots, property_trees, tracker)
}

#[test]
fn an_observed_scene_reports_no_mismatch() {
    let (arena, roots, property_trees, tracker) = observed_scene();

    assert_eq!(
        tracker.live_snapshot_mismatch(&arena, &roots, &property_trees),
        None
    );
    assert!(tracker.matches_live_snapshot(&arena, &roots, &property_trees));
}

#[test]
fn the_boolean_check_agrees_with_the_diagnostic() {
    let (arena, roots, property_trees, tracker) = observed_scene();
    let duplicated = vec![roots[0], roots[0]];

    for candidate in [roots.as_slice(), duplicated.as_slice(), &[]] {
        assert_eq!(
            tracker.matches_live_snapshot(&arena, candidate, &property_trees),
            tracker
                .live_snapshot_mismatch(&arena, candidate, &property_trees)
                .is_none(),
        );
    }
}

#[test]
fn changed_roots_report_a_whole_scene_mismatch_with_no_owner() {
    let (arena, roots, property_trees, tracker) = observed_scene();
    let child = arena
        .get(roots[0])
        .expect("root stays live")
        .children()
        .first()
        .copied()
        .expect("root has one child");

    let mismatch = tracker
        .live_snapshot_mismatch(&arena, &[child], &property_trees)
        .expect("different roots must drift");

    assert_eq!(mismatch.owner, None);
    assert_eq!(mismatch.field, LiveSnapshotField::ObservedRoots);
    assert_eq!(mismatch.field.code(), "live-snapshot-observed-roots");
}

#[test]
fn a_node_added_after_observation_is_named_by_its_parent() {
    let (mut arena, roots, property_trees, tracker) = observed_scene();
    let added = arena.insert(Node::with_parent(
        Box::new(Element::new_with_id(3, 0.0, 0.0, 10.0, 10.0)),
        Some(roots[0]),
    ));
    arena.push_child(roots[0], added);

    let mismatch = tracker
        .live_snapshot_mismatch(&arena, &roots, &property_trees)
        .expect("an unobserved child must drift");

    assert_eq!(mismatch.owner, Some(roots[0]));
    assert_eq!(mismatch.field, LiveSnapshotField::Children);
    assert_eq!(mismatch.field.code(), "live-snapshot-children");
}

#[test]
fn every_field_has_a_distinct_stable_code() {
    let fields = [
        LiveSnapshotField::ObservedRoots,
        LiveSnapshotField::RepeatedNode,
        LiveSnapshotField::MissingNode,
        LiveSnapshotField::MissingRecord,
        LiveSnapshotField::InactiveRecord,
        LiveSnapshotField::Epoch,
        LiveSnapshotField::Parent,
        LiveSnapshotField::Children,
        LiveSnapshotField::Coverage,
        LiveSnapshotField::SelfSignature,
        LiveSnapshotField::TransformGeneration,
        LiveSnapshotField::EffectGeneration,
        LiveSnapshotField::ScrollGeneration,
        LiveSnapshotField::UnreachableRecord,
    ];
    let codes = fields.iter().map(|field| field.code()).collect::<Vec<_>>();

    let mut unique = codes.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), codes.len());
    assert!(codes.iter().all(|code| {
        code.starts_with("live-snapshot-")
            && code.chars().all(|c| c.is_ascii_lowercase() || c == '-')
    }));
}
