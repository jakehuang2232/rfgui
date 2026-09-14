use super::*;

#[test]
fn unchanged_live_set_prunes_raw_external_writes_and_lost_history() {
    let (mut arena, root, normal, anchor) = nested_anchor_parent_fixture(false);
    let (scroll_root, scroll_child) = make_vertical_scroll_fixture(&mut arena, 803, 804);
    let outside = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(802, 0., 0., 40., 30.)),
    );
    set_transform(
        &arena,
        root,
        Transform::new([Translate::xy(Length::px(3.), Length::px(4.))]),
    );
    set_opacity(&arena, root, 0.5);
    clear_layout_dirty_for_subtree(&arena, root);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root, scroll_root]);
    let seen = [root, normal, anchor, scroll_root, scroll_child]
        .into_iter()
        .collect();
    let retained_lengths = [
        trees.transforms.len(),
        trees.layout_positions.len(),
        trees.visual_offsets.len(),
        trees.clips.len(),
        trees.effects.len(),
        trees.scrolls.len(),
        trees.states.len(),
    ];
    assert!(
        retained_lengths.iter().all(|&len| len > 0),
        "{retained_lengths:?}"
    );
    for round in 0..3 {
        macro_rules! insert_external {
            ($map:ident, $key:expr) => {{
                let value = *trees.$map.values().next().unwrap();
                trees.$map.insert($key, value);
            }};
        }
        insert_external!(transforms, TransformNodeId(outside));
        insert_external!(layout_positions, LayoutPositionNodeId(outside));
        insert_external!(visual_offsets, VisualOffsetNodeId(outside));
        for role in [ClipNodeRole::SelfClip, ClipNodeRole::ContentsClip] {
            insert_external!(
                clips,
                ClipNodeId {
                    owner: outside,
                    role
                }
            );
        }
        insert_external!(effects, EffectNodeId(outside));
        insert_external!(scrolls, ScrollNodeId(outside));
        insert_external!(states, outside);
        if round == 1 {
            // Unclassified mutable access invalidates the sparse proof.
            let _ = trees.states.iter_mut();
        } else if round == 2 {
            for _ in 0..4100 {
                trees.states.insert(outside, NodePropertyState::default());
            }
        }
        let independent_reader = trees.property_store_stamp();
        assert!(trees.prune_unseen(&arena, &seen));
        assert_eq!(
            retained_lengths,
            [
                trees.transforms.len(),
                trees.layout_positions.len(),
                trees.visual_offsets.len(),
                trees.clips.len(),
                trees.effects.len(),
                trees.scrolls.len(),
                trees.states.len(),
            ]
        );
        assert!(!trees.states.contains_key(&outside));
        // A second independent reader still sees pruning as a real map write.
        assert_eq!(
            trees.property_writes_since(&independent_reader).as_deref(),
            Some(&[outside].into_iter().collect())
        );
    }
    // Equal arena lifetime does not imply equal active roots.
    assert!(!trees.prune_unseen(&arena, &FxHashSet::default()));
    assert!(trees.states.is_empty());
    assert!(trees.transforms.is_empty());
    assert!(trees.layout_positions.is_empty());
    assert!(trees.visual_offsets.is_empty());
    assert!(trees.clips.is_empty());
    assert!(trees.effects.is_empty());
    assert!(trees.scrolls.is_empty());
    assert!(trees.effect_generations.contains_key(&EffectNodeId(root)));
    arena.remove_subtree(root);
    trees.prune_unseen(&arena, &FxHashSet::default());
    assert!(trees.effect_generations.is_empty());
}
