use super::*;
use crate::view::node_arena::Node;

fn insert_owned_subtree(arena: &mut NodeArena, owner: NodeKey, id: u64) -> (NodeKey, NodeKey) {
    let root = arena.insert(Node::with_parent(
        Box::new(Element::new_with_id(id, 0.0, 0.0, 1.0, 1.0)),
        Some(owner),
    ));
    let child = arena.insert(Node::with_parent(
        Box::new(Element::new_with_id(id + 1, 0.0, 0.0, 1.0, 1.0)),
        Some(root),
    ));
    arena.set_children(root, vec![child]);
    (root, child)
}

fn assert_children_mirror(arena: &NodeArena, owner: NodeKey, element: &Element) {
    assert_eq!(arena.children_of(owner), element.children());
    for child in element.children() {
        assert_eq!(arena.parent_of(*child), Some(owner));
    }
}

#[test]
fn active_and_inactive_loading_error_replacements_preserve_slot_ownership() {
    let mut arena = NodeArena::new();
    let owner = arena.insert(Node::new(Box::new(Element::new_with_id(
        1, 0.0, 0.0, 10.0, 10.0,
    ))));
    let mut element = Element::new_with_id(1, 0.0, 0.0, 10.0, 10.0);
    let (old_loading, old_loading_child) = insert_owned_subtree(&mut arena, owner, 10);
    let (old_error, old_error_child) = insert_owned_subtree(&mut arena, owner, 20);
    let (new_loading, _) = insert_owned_subtree(&mut arena, owner, 30);
    let (new_error, _) = insert_owned_subtree(&mut arena, owner, 40);
    let (newer_loading, _) = insert_owned_subtree(&mut arena, owner, 50);
    let (newer_error, _) = insert_owned_subtree(&mut arena, owner, 60);
    let mut loading = vec![old_loading];
    let mut error = vec![old_error];
    let mut active = ActiveSlot::None;

    sync_active_slot(
        &mut arena,
        Some(owner),
        &mut element,
        &mut loading,
        &mut error,
        &mut active,
        ActiveSlot::Loading,
    );
    assert_children_mirror(&arena, owner, &element);

    // Error is inactive while Loading is active. Replacing it must not
    // remove the loading subtree that is returned to latent storage.
    replace_slot(
        &mut arena,
        owner,
        &mut element,
        &mut loading,
        &mut error,
        &mut active,
        ActiveSlot::Error,
        &[new_error],
    )
    .unwrap();
    assert_eq!(active, ActiveSlot::None);
    assert_eq!(loading, vec![old_loading]);
    assert_eq!(error, vec![new_error]);
    assert!(arena.contains_key(old_loading));
    assert!(arena.contains_key(old_loading_child));
    assert!(!arena.contains_key(old_error));
    assert!(!arena.contains_key(old_error_child));
    assert_children_mirror(&arena, owner, &element);

    // Error target active.
    sync_active_slot(
        &mut arena,
        Some(owner),
        &mut element,
        &mut loading,
        &mut error,
        &mut active,
        ActiveSlot::Error,
    );
    replace_slot(
        &mut arena,
        owner,
        &mut element,
        &mut loading,
        &mut error,
        &mut active,
        ActiveSlot::Error,
        &[newer_error],
    )
    .unwrap();
    assert!(!arena.contains_key(new_error));
    assert_eq!(error, vec![newer_error]);

    // Error active, Loading inactive; then Loading target active.
    sync_active_slot(
        &mut arena,
        Some(owner),
        &mut element,
        &mut loading,
        &mut error,
        &mut active,
        ActiveSlot::Error,
    );
    replace_slot(
        &mut arena,
        owner,
        &mut element,
        &mut loading,
        &mut error,
        &mut active,
        ActiveSlot::Loading,
        &[new_loading],
    )
    .unwrap();
    assert!(arena.contains_key(newer_error));
    assert!(!arena.contains_key(old_loading));
    assert!(!arena.contains_key(old_loading_child));
    sync_active_slot(
        &mut arena,
        Some(owner),
        &mut element,
        &mut loading,
        &mut error,
        &mut active,
        ActiveSlot::Loading,
    );
    replace_slot(
        &mut arena,
        owner,
        &mut element,
        &mut loading,
        &mut error,
        &mut active,
        ActiveSlot::Loading,
        &[newer_loading],
    )
    .unwrap();
    assert!(!arena.contains_key(new_loading));
    assert_eq!(loading, vec![newer_loading]);
    assert_children_mirror(&arena, owner, &element);
    assert_eq!(arena.parent_of(newer_loading), Some(owner));
    assert!(arena.arena_local_dirty(owner).contains(DirtyFlags::LAYOUT));
}

#[test]
fn invalid_new_roots_fail_closed_without_mutating_old_slot_or_topology() {
    let mut arena = NodeArena::new();
    let owner = arena.insert(Node::new(Box::new(Element::new_with_id(
        1, 0.0, 0.0, 10.0, 10.0,
    ))));
    let other_owner = arena.insert(Node::new(Box::new(Element::new_with_id(
        2, 0.0, 0.0, 10.0, 10.0,
    ))));
    let mut element = Element::new_with_id(1, 0.0, 0.0, 10.0, 10.0);
    let (old_loading, old_loading_child) = insert_owned_subtree(&mut arena, owner, 10);
    let (old_error, _) = insert_owned_subtree(&mut arena, owner, 20);
    let (valid, _) = insert_owned_subtree(&mut arena, owner, 30);
    let (wrong_parent, _) = insert_owned_subtree(&mut arena, other_owner, 40);
    let missing = arena.insert(Node::with_parent(
        Box::new(Element::new_with_id(50, 0.0, 0.0, 1.0, 1.0)),
        Some(owner),
    ));
    arena.remove_subtree(missing);
    let mut loading = vec![old_loading];
    let mut error = vec![old_error];
    let mut active = ActiveSlot::None;

    for (roots, expected) in [
        (
            vec![valid, valid],
            SlotReplacementError::DuplicateRoot(valid),
        ),
        (vec![missing], SlotReplacementError::MissingRoot(missing)),
        (
            vec![wrong_parent],
            SlotReplacementError::WrongParent {
                root: wrong_parent,
                actual: Some(other_owner),
            },
        ),
        (
            vec![old_error],
            SlotReplacementError::AliasedRoot(old_error),
        ),
    ] {
        let before_len = arena.len();
        assert_eq!(
            replace_slot(
                &mut arena,
                owner,
                &mut element,
                &mut loading,
                &mut error,
                &mut active,
                ActiveSlot::Loading,
                &roots,
            ),
            Err(expected)
        );
        assert_eq!(arena.len(), before_len);
        assert_eq!(loading, vec![old_loading]);
        assert_eq!(error, vec![old_error]);
        assert_eq!(active, ActiveSlot::None);
        assert!(arena.contains_key(old_loading));
        assert!(arena.contains_key(old_loading_child));
        assert_children_mirror(&arena, owner, &element);
    }

    // Exact-list no-op is still subject to the complete preflight. Stale
    // latent topology must not be accepted merely because the slices are
    // byte-for-byte equal.
    loading = vec![valid, valid];
    assert_eq!(
        replace_slot(
            &mut arena,
            owner,
            &mut element,
            &mut loading,
            &mut error,
            &mut active,
            ActiveSlot::Loading,
            &[valid, valid],
        ),
        Err(SlotReplacementError::DuplicateRoot(valid))
    );
    assert_eq!(loading, vec![valid, valid]);

    loading = vec![missing];
    assert_eq!(
        replace_slot(
            &mut arena,
            owner,
            &mut element,
            &mut loading,
            &mut error,
            &mut active,
            ActiveSlot::Loading,
            &[missing],
        ),
        Err(SlotReplacementError::MissingRoot(missing))
    );
    assert_eq!(loading, vec![missing]);

    loading = vec![wrong_parent];
    assert_eq!(
        replace_slot(
            &mut arena,
            owner,
            &mut element,
            &mut loading,
            &mut error,
            &mut active,
            ActiveSlot::Loading,
            &[wrong_parent],
        ),
        Err(SlotReplacementError::WrongParent {
            root: wrong_parent,
            actual: Some(other_owner),
        })
    );
    assert_eq!(loading, vec![wrong_parent]);
}

#[test]
#[should_panic(expected = "cold slot attachment requires an inactive host")]
fn cold_attach_rejects_active_host_in_all_build_profiles() {
    attach_slot_cold(ActiveSlot::Loading, &mut Vec::new(), Vec::new());
}

#[test]
#[should_panic(expected = "cold slot attachment must not replace an existing slot")]
fn cold_attach_rejects_nonempty_target_in_all_build_profiles() {
    let mut arena = NodeArena::new();
    let key = arena.insert(Node::new(Box::new(Element::new_with_id(
        1, 0.0, 0.0, 1.0, 1.0,
    ))));
    attach_slot_cold(ActiveSlot::None, &mut vec![key], Vec::new());
}
