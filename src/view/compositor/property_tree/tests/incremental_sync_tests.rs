use super::*;
use crate::view::base_component::EventTarget;

fn assert_same(incremental: &PropertyTrees, full: &PropertyTrees, arena: &NodeArena) {
    assert_eq!(incremental.states, full.states);
    assert_eq!(incremental.changes, full.changes);
    assert_eq!(
        incremental.inactive_scroll_owners,
        full.inactive_scroll_owners
    );
    assert_eq!(incremental.validation_errors, full.validation_errors);
    assert_eq!(
        incremental.spatial_validation_errors,
        full.spatial_validation_errors
    );
    for (key, _) in arena.iter() {
        assert_eq!(
            incremental.transform_snapshot_for(TransformNodeId(key)),
            full.transform_snapshot_for(TransformNodeId(key))
        );
        assert_eq!(
            incremental.layout_position_snapshot_for(LayoutPositionNodeId(key)),
            full.layout_position_snapshot_for(LayoutPositionNodeId(key))
        );
        assert_eq!(
            incremental.visual_offset_snapshot_for(VisualOffsetNodeId(key)),
            full.visual_offset_snapshot_for(VisualOffsetNodeId(key))
        );
        assert_eq!(
            incremental.effect_node_snapshot_for(EffectNodeId(key)),
            full.effect_node_snapshot_for(EffectNodeId(key))
        );
        assert_eq!(
            incremental.scroll_snapshot_for(ScrollNodeId(key)),
            full.scroll_snapshot_for(ScrollNodeId(key))
        );
        for role in [ClipNodeRole::SelfClip, ClipNodeRole::ContentsClip] {
            let id = ClipNodeId { owner: key, role };
            assert_eq!(
                incremental.clip_node_snapshot_for(id),
                full.clip_node_snapshot_for(id)
            );
        }
    }
}

fn sync_pair(
    incremental: &mut PropertyTrees,
    full: &mut PropertyTrees,
    arena: &NodeArena,
    roots: &[NodeKey],
) {
    incremental.sync(arena, roots);
    full.native_subtrees = Default::default();
    full.prune_proof = None;
    full.sync(arena, roots);
    assert_same(incremental, full, arena);
}

#[test]
fn native_subtrees_match_full_sync_across_edits_reparent_removal_and_root_changes() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 1);
    let sibling = insert_element(&mut arena, 2);
    let child = insert_element(&mut arena, 3);
    append_child(&mut arena, root, child);
    let mut incremental = PropertyTrees::default();
    let mut full = PropertyTrees::default();
    for _ in 0..2 {
        sync_pair(&mut incremental, &mut full, &arena, &[root, sibling]);
    }
    assert_eq!(incremental.observed_nodes, 0);
    assert_eq!(incremental.replayed_nodes, 3);
    set_opacity(&arena, child, 0.5);
    arena.clear_element_dirty_flags(child, DirtyFlags::ALL);
    sync_pair(&mut incremental, &mut full, &arena, &[root, sibling]);
    assert_eq!(incremental.observed_nodes, 2);
    assert_eq!(incremental.replayed_nodes, 1);
    set_opacity(&arena, root, 0.4);
    sync_pair(&mut incremental, &mut full, &arena, &[root, sibling]);
    arena.set_children(root, vec![]);
    append_child(&mut arena, sibling, child);
    sync_pair(&mut incremental, &mut full, &arena, &[root, sibling]);
    sync_pair(&mut incremental, &mut full, &arena, &[sibling, root]);
    arena.remove_subtree(child);
    sync_pair(&mut incremental, &mut full, &arena, &[sibling, root]);
    assert!(!incremental.states.contains_key(&child));
    // The same generational keys in a different arena cannot replay old inputs.
    let mut other = NodeArena::new();
    let other_root = insert_element(&mut other, 9);
    set_opacity(&other, other_root, 0.7);
    sync_pair(&mut incremental, &mut full, &other, &[other_root]);
}

#[test]
fn volatile_custom_property_hooks_keep_observing_live_values() {
    let mut arena = NodeArena::new();
    let custom = insert_contents_clip_host(&mut arena, 1, Some([0, 0, 20, 20]));
    let child = insert_element(&mut arena, 2);
    append_child(&mut arena, custom, child);
    let mut incremental = PropertyTrees::default();
    let mut full = PropertyTrees::default();
    for _ in 0..3 {
        sync_pair(&mut incremental, &mut full, &arena, &[custom]);
    }
    assert_eq!(incremental.replayed_nodes, 0);
}

#[test]
fn property_observation_count_depends_on_changed_branch_not_static_sibling_size() {
    for count in [20, 2000] {
        let mut arena = NodeArena::new();
        let root = insert_element(&mut arena, 1);
        let active = insert_element(&mut arena, 2);
        for id in 3..count + 3 {
            let child = insert_element(&mut arena, id);
            append_child(&mut arena, root, child);
        }
        let mut incremental = PropertyTrees::default();
        let mut full = PropertyTrees::default();
        sync_pair(&mut incremental, &mut full, &arena, &[root, active]);
        set_opacity(&arena, active, 0.5);
        sync_pair(&mut incremental, &mut full, &arena, &[root, active]);
        assert_eq!(incremental.observed_nodes, 1);
        assert_eq!(incremental.replayed_nodes, count as usize + 1);
    }
}

#[test]
fn unchanged_children_replay_after_ancestor_borrow_and_numeric_property_changes() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 501);
    let child = insert_element(&mut arena, 502);
    append_child(&mut arena, root, child);
    set_transform(
        &arena,
        root,
        Transform::new([crate::style::Translate::xy(
            Length::px(3.0),
            Length::px(4.0),
        )]),
    );
    set_opacity(&arena, root, 0.5);
    let mut incremental = PropertyTrees::default();
    let mut full = PropertyTrees::default();
    sync_pair(&mut incremental, &mut full, &arena, &[root]);
    for frame in 0..4 {
        match frame {
            0 => drop(arena.get_mut(root)),
            1 => set_opacity(&arena, root, 0.75),
            2 => {
                set_transform(
                    &arena,
                    root,
                    Transform::new([crate::style::Translate::xy(
                        Length::px(9.0),
                        Length::px(10.0),
                    )]),
                );
                set_opacity(&arena, root, 0.75);
            }
            _ => set_opacity(&arena, root, 1.0),
        }
        sync_pair(&mut incremental, &mut full, &arena, &[root]);
        if frame < 3 {
            assert_eq!(incremental.observed_nodes, 1, "frame {frame}");
            assert_eq!(incremental.replayed_nodes, 1, "frame {frame}");
        } else {
            assert_eq!(incremental.observed_nodes, 2, "changed inherited effect ID");
        }
    }
}

#[test]
fn parent_scroll_edge_changes_match_full_sync_without_child_mutation() {
    let (arena, root, child, _) = nested_anchor_parent_fixture(false);
    let mut incremental = PropertyTrees::default();
    let mut full = PropertyTrees::default();
    sync_pair(&mut incremental, &mut full, &arena, &[root]);
    assert!(
        incremental
            .layout_position_snapshot_for(LayoutPositionNodeId(child))
            .is_some()
    );
    let child_revision = arena.mutation_revision(child);
    for direction in [
        ScrollDirection::None,
        ScrollDirection::Vertical,
        ScrollDirection::None,
    ] {
        set_scroll_direction(&arena, root, direction);
        arena.clear_element_dirty_flags(root, DirtyFlags::ALL);
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        sync_pair(&mut incremental, &mut full, &arena, &[root]);
        sync_pair(&mut incremental, &mut full, &arena, &[root]);
    }
    for offset in [5.0, 0.0] {
        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_scroll_offset((0.0, offset));
        sync_pair(&mut incremental, &mut full, &arena, &[root]);
        assert_eq!(
            incremental
                .layout_position_snapshot_for(LayoutPositionNodeId(child))
                .unwrap()
                .reference_scroll,
            (offset != 0.0).then_some(ScrollNodeId(root)),
        );
    }
    assert_eq!(arena.mutation_revision(child), child_revision);
}

#[test]
fn sibling_clip_phase_changes_keep_exact_self_clip_observations() {
    let (arena, root, normal, _) = nested_anchor_parent_fixture(false);
    let mut incremental = PropertyTrees::default();
    let mut full = PropertyTrees::default();
    sync_pair(&mut incremental, &mut full, &arena, &[root]);
    for mode in [ClipMode::Viewport, ClipMode::AnchorParent, ClipMode::Parent] {
        set_clip_mode(&arena, normal, mode);
        sync_pair(&mut incremental, &mut full, &arena, &[root]);
    }
}
