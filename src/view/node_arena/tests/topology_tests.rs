use super::*;

#[test]
fn remove_subtree_follows_parent_owned_side_slots() {
    let mut arena = NodeArena::new();
    let owner = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let side_root = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
    let side_child = insert_test_node(&mut arena, 3, DirtyFlags::NONE);

    // Side roots are owned by the host but intentionally absent from its
    // active Node.children list until the host selects that slot.
    arena.set_parent(side_root, Some(owner));
    arena.set_parent(side_child, Some(side_root));
    arena.push_child(side_root, side_child);

    assert_eq!(arena.remove_subtree(owner), 3);
    assert!(arena.is_empty());
    assert_eq!(arena.find_by_stable_id(2), None);
    assert_eq!(arena.find_by_stable_id(3), None);
}

#[test]
fn remove_subtree_detaches_surviving_parent_and_root_registry() {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(clean_element())));
    let child = arena.insert(Node::new(Box::new(clean_element())));
    arena.push_root(root);
    arena.set_parent(child, Some(root));
    arena.set_children(root, vec![child]);

    assert_eq!(arena.remove_subtree(child), 1);
    assert_eq!(arena.children_of(root), Vec::<NodeKey>::new());
    assert!(
        arena
            .get(root)
            .expect("root survives")
            .element
            .children()
            .is_empty()
    );

    assert_eq!(arena.remove_subtree(root), 1);
    assert!(arena.roots().is_empty());
}

#[test]
fn removing_currently_taken_element_cleans_stable_index() {
    let mut arena = NodeArena::new();
    let key = insert_test_node(&mut arena, 77, DirtyFlags::NONE);

    arena.with_element_taken(key, |_element, arena| {
        assert_eq!(arena.remove_subtree(key), 1);
    });

    assert_eq!(arena.find_by_stable_id(77), None);
    assert!(!arena.contains_key(key));
}

#[test]
fn stable_id_lookup_rejects_in_place_identity_drift() {
    let mut arena = NodeArena::new();
    let key = insert_test_node(&mut arena, 7, DirtyFlags::NONE);
    *arena.get_mut(key).expect("node exists").element = Box::new(Placeholder);

    assert_eq!(arena.find_by_stable_id(7), None);
}

#[test]
fn with_element_taken_restores_element_before_resuming_panic() {
    let mut arena = NodeArena::new();
    let key = insert_test_node(&mut arena, 9, DirtyFlags::NONE);

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        arena.with_element_taken(key, |_element, _arena| {
            panic!("intentional test panic");
        });
    }));

    assert!(panic.is_err());
    assert_eq!(arena.find_by_stable_id(9), Some(key));
    assert!(
        arena
            .get(key)
            .expect("node survives panic")
            .element
            .as_any()
            .is::<TestElement>()
    );
}

#[test]
fn structural_children_mutation_keeps_compatibility_mirror_in_sync() {
    let mut arena = NodeArena::new();
    let parent = arena.insert(Node::new(Box::new(clean_element())));
    let child = arena.insert(Node::new(Box::new(clean_element())));

    arena.set_parent(child, Some(parent));
    arena.set_children(parent, vec![child]);

    let node = arena.get(parent).expect("parent exists");
    assert_eq!(node.children(), &[child]);
    assert_eq!(node.element.children(), &[child]);
}

#[test]
fn sync_arena_visits_only_registered_hosts() {
    RECORDED_BUILDS.with(|builds| builds.borrow_mut().clear());
    let mut arena = NodeArena::new();
    arena.insert(Node::new(Box::new(TestElement::new(1, DirtyFlags::NONE))));
    arena.insert(Node::new(Box::new(RecordingElement::new(2, "sync"))));

    arena.sync_registered_elements();

    RECORDED_BUILDS.with(|builds| assert_eq!(&*builds.borrow(), &["sync"]));
}
