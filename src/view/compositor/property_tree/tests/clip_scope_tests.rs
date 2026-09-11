use super::*;

#[test]
fn anchor_parent_self_clip_is_stable_replace_and_generation_is_monotonic() {
    let (arena, root) = anchor_parent_clip_fixture(320.0);
    let id = ClipNodeId {
        owner: root,
        role: ClipNodeRole::SelfClip,
    };
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    let first = trees.clips[&id];
    assert_eq!(trees.states[&root].paint.clip, Some(id));
    assert_eq!(first.owner, root);
    assert_eq!(first.parent, None);
    assert_eq!(first.behavior, ClipBehavior::Replace);
    assert!(matches!(
        first.geometry,
        ClipGeometry::LogicalScissor([0, 0, 320, 240])
    ));
    assert!(trees.changes_for(root).contains(PropertyChangeFlags::CLIP));
    assert!(
        trees
            .changes_for(root)
            .contains(PropertyChangeFlags::TOPOLOGY)
    );

    trees.sync(&arena, &[root]);
    assert_eq!(trees.clips[&id].generation, first.generation);
    assert_eq!(trees.changes_for(root), PropertyChangeFlags::NONE);

    set_clip_mode(&arena, root, ClipMode::Parent);
    trees.sync(&arena, &[root]);
    assert!(!trees.clips.contains_key(&id));
    assert!(trees.changes_for(root).contains(PropertyChangeFlags::CLIP));
    assert!(
        trees
            .changes_for(root)
            .contains(PropertyChangeFlags::TOPOLOGY)
    );

    set_clip_mode(&arena, root, ClipMode::AnchorParent);
    trees.sync(&arena, &[root]);
    assert!(trees.clips[&id].generation > first.generation);
}

#[test]
fn nested_anchor_parent_leaf_is_exact_only_after_normal_siblings() {
    let (arena, root, normal, anchor) = nested_anchor_parent_fixture(false);
    let id = ClipNodeId {
        owner: anchor,
        role: ClipNodeRole::SelfClip,
    };
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);

    let clip = trees.clips[&id];
    assert_eq!(clip.parent, None);
    assert_eq!(clip.behavior, ClipBehavior::Replace);
    assert!(matches!(clip.geometry, ClipGeometry::LogicalScissor(_)));
    assert_eq!(trees.states[&normal].paint.clip, None);
    assert_eq!(trees.states[&anchor].paint.clip, Some(id));
    assert_eq!(trees.states[&anchor].descendants.clip, Some(id));
    assert_eq!(
        trees.authoritative_self_clip_for_owner(anchor, trees.states[&anchor].paint),
        Some(id)
    );

    trees.sync(&arena, &[root]);
    assert_eq!(trees.clips[&id].generation, clip.generation);

    let (arena, root, _, anchor) = nested_anchor_parent_fixture(true);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    assert!(!trees.clips.contains_key(&ClipNodeId {
        owner: anchor,
        role: ClipNodeRole::SelfClip,
    }));
    assert_eq!(trees.states[&anchor].paint.clip, None);

    let (arena, root, normal, anchor) = nested_anchor_parent_fixture(false);
    set_clip_mode(&arena, normal, ClipMode::Viewport);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    assert!(
        !trees.clips.contains_key(&ClipNodeId {
            owner: anchor,
            role: ClipNodeRole::SelfClip,
        }),
        "a deferred Viewport sibling invalidates the normal frame ordering witness"
    );
}

#[test]
fn nested_anchor_parent_replace_escapes_ancestor_contents_intersection() {
    let (mut arena, parent, _, anchor) = nested_anchor_parent_fixture(false);
    let outer = insert_contents_clip_host(&mut arena, 0x8d10, Some([12, 14, 20, 18]));
    arena.set_parent(parent, Some(outer));
    arena.set_children(outer, vec![parent]);

    let contents = ClipNodeId {
        owner: outer,
        role: ClipNodeRole::ContentsClip,
    };
    let own = ClipNodeId {
        owner: anchor,
        role: ClipNodeRole::SelfClip,
    };
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[outer]);

    assert_eq!(trees.states[&parent].paint.clip, Some(contents));
    assert_eq!(trees.clips[&own].parent, Some(contents));
    assert_eq!(trees.clips[&own].behavior, ClipBehavior::Replace);
    assert_eq!(trees.states[&anchor].paint.clip, Some(own));
    assert_eq!(
        trees
            .clip_snapshot_for(Some(own))
            .unwrap()
            .iter()
            .map(|clip| (clip.id, clip.behavior))
            .collect::<Vec<_>>(),
        vec![
            (own, ClipBehavior::Replace),
            (contents, ClipBehavior::Intersect),
        ]
    );
}

#[test]
fn anchor_parent_clip_tombstone_survives_inactive_roots_and_prunes_removed_owner() {
    let (mut arena, root) = anchor_parent_clip_fixture(320.0);
    let id = ClipNodeId {
        owner: root,
        role: ClipNodeRole::SelfClip,
    };
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    let first_generation = trees.clips[&id].generation;

    trees.sync(&arena, &[]);
    assert!(!trees.clips.contains_key(&id));
    assert!(!trees.states.contains_key(&root));
    assert!(trees.clip_generations.contains_key(&id));

    trees.sync(&arena, &[root]);
    assert!(trees.clips[&id].generation > first_generation);

    arena.remove(root);
    trees.sync(&arena, &[root]);
    assert!(!trees.clips.contains_key(&id));
    assert!(!trees.states.contains_key(&root));
    assert!(!trees.clip_generations.contains_key(&id));
}

#[test]
fn contents_clip_applies_only_to_descendants_and_is_inherited() {
    let mut arena = NodeArena::new();
    let root = insert_contents_clip_host(&mut arena, 0x8c00, Some([10, 20, 80, 40]));
    let child = insert_element(&mut arena, 0x8c01);
    append_child(&mut arena, root, child);
    let id = ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    };

    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);

    assert_eq!(trees.states[&root].paint.clip, None);
    assert_eq!(trees.states[&root].descendants.clip, Some(id));
    assert_eq!(trees.states[&child].paint.clip, Some(id));
    let clip = trees.clips[&id];
    assert_eq!(clip.owner, root);
    assert_eq!(clip.parent, None);
    assert_eq!(clip.behavior, ClipBehavior::Intersect);
    assert!(matches!(
        clip.geometry,
        ClipGeometry::LogicalScissor([10, 20, 80, 40])
    ));
}

#[test]
fn nested_contents_clips_intersect_in_owner_order_and_preserve_explicit_empty() {
    let mut arena = NodeArena::new();
    let outer = insert_contents_clip_host(&mut arena, 0x8c10, Some([0, 0, 100, 100]));
    let inner = insert_contents_clip_host(&mut arena, 0x8c11, Some([20, 30, 0, 0]));
    let leaf = insert_element(&mut arena, 0x8c12);
    append_child(&mut arena, outer, inner);
    append_child(&mut arena, inner, leaf);
    let outer_id = ClipNodeId {
        owner: outer,
        role: ClipNodeRole::ContentsClip,
    };
    let inner_id = ClipNodeId {
        owner: inner,
        role: ClipNodeRole::ContentsClip,
    };

    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[outer]);

    assert_eq!(trees.states[&inner].paint.clip, Some(outer_id));
    assert_eq!(trees.states[&inner].descendants.clip, Some(inner_id));
    assert_eq!(trees.states[&leaf].paint.clip, Some(inner_id));
    assert_eq!(trees.clips[&inner_id].parent, Some(outer_id));
    assert!(matches!(
        trees.clips[&inner_id].geometry,
        ClipGeometry::LogicalScissor([20, 30, 0, 0])
    ));
    assert_eq!(
        trees
            .clip_snapshot_for(Some(inner_id))
            .expect("complete nested clip chain")
            .iter()
            .map(|snapshot| snapshot.id)
            .collect::<Vec<_>>(),
        vec![inner_id, outer_id]
    );
}

#[test]
fn contents_clip_removal_and_reinsert_bump_generation_and_topology() {
    let mut arena = NodeArena::new();
    let root = insert_contents_clip_host(&mut arena, 0x8c20, Some([1, 2, 30, 40]));
    let id = ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    };
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    let first = trees.clips[&id].generation;

    arena
        .get_mut(root)
        .expect("contents host")
        .element
        .as_any_mut()
        .downcast_mut::<ContentsClipHost>()
        .expect("contents host")
        .scissor = None;
    trees.sync(&arena, &[root]);
    assert!(!trees.clips.contains_key(&id));
    assert!(trees.changes_for(root).contains(PropertyChangeFlags::CLIP));
    assert!(
        trees
            .changes_for(root)
            .contains(PropertyChangeFlags::TOPOLOGY)
    );

    arena
        .get_mut(root)
        .expect("contents host")
        .element
        .as_any_mut()
        .downcast_mut::<ContentsClipHost>()
        .expect("contents host")
        .scissor = Some([1, 2, 30, 40]);
    trees.sync(&arena, &[root]);
    assert!(trees.clips[&id].generation > first);
}

#[test]
fn neutral_component_does_not_invent_transform_clip_effect_or_scroll_nodes() {
    let mut arena = NodeArena::new();
    let key = arena.insert(Node::new(Box::new(NeutralCustomHost)));
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[key]);

    assert_eq!(trees.states[&key], NodePropertyState::default());
    assert!(
        trees
            .changes_for(key)
            .contains(PropertyChangeFlags::TOPOLOGY)
    );
    assert!(trees.transforms.is_empty());
    assert!(trees.clips.is_empty());
    assert!(trees.effects.is_empty());
    assert!(trees.scrolls.is_empty());

    trees.sync(&arena, &[]);
    assert!(!trees.states.contains_key(&key));
    trees.sync(&arena, &[key]);
    assert_eq!(trees.states[&key], NodePropertyState::default());
    assert!(
        trees
            .changes_for(key)
            .contains(PropertyChangeFlags::TOPOLOGY)
    );
}
