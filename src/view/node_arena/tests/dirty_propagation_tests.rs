use super::*;

#[test]
fn subtree_dirty_queries_return_false_for_missing_key() {
    let mut arena = NodeArena::new();
    let missing = insert_test_node(&mut arena, 1, DirtyFlags::PAINT);
    arena.remove(missing).expect("node exists");

    assert!(!arena.subtree_dirty_intersects(missing, DirtyFlags::PAINT));
    assert!(!arena.subtree_dirty_contains(missing, DirtyFlags::PAINT));
}

#[test]
fn subtree_dirty_query_sees_element_local_dirty_after_refresh() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let child = insert_test_node(&mut arena, 2, DirtyFlags::PAINT);
    link_child(&mut arena, root, child);

    arena.refresh_subtree_dirty_cache(root);

    assert!(arena.subtree_dirty_intersects(child, DirtyFlags::PAINT));
    assert!(arena.subtree_dirty_contains(child, DirtyFlags::PAINT));
    assert!(arena.subtree_dirty_intersects(root, DirtyFlags::PAINT));
    assert!(arena.subtree_dirty_contains(root, DirtyFlags::PAINT));
    assert!(!arena.subtree_dirty_intersects(child, DirtyFlags::HIT_TEST));
}

#[test]
fn subtree_dirty_query_sees_arena_shadow_dirty_after_mark_dirty() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let child = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
    link_child(&mut arena, root, child);

    arena.refresh_subtree_dirty_cache(root);
    arena.mark_dirty(child, DirtyFlags::PAINT);

    assert!(arena.subtree_dirty_intersects(child, DirtyFlags::PAINT));
    assert!(arena.subtree_dirty_contains(child, DirtyFlags::PAINT));
    assert!(arena.subtree_dirty_intersects(root, DirtyFlags::PAINT));
    assert!(arena.subtree_dirty_contains(root, DirtyFlags::PAINT));
}

#[test]
fn subtree_dirty_query_reflects_clear_arena_dirty() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let child = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
    link_child(&mut arena, root, child);

    arena.refresh_subtree_dirty_cache(root);
    arena.mark_dirty(child, DirtyFlags::PAINT);
    arena.clear_arena_dirty(child, DirtyFlags::PAINT);

    assert!(!arena.subtree_dirty_intersects(child, DirtyFlags::PAINT));
    assert!(!arena.subtree_dirty_contains(child, DirtyFlags::PAINT));
    assert!(!arena.subtree_dirty_intersects(root, DirtyFlags::PAINT));
    assert!(!arena.subtree_dirty_contains(root, DirtyFlags::PAINT));
}

#[test]
fn subtree_dirty_query_reflects_clear_arena_dirty_subtree() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let child = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
    let grandchild = insert_test_node(&mut arena, 3, DirtyFlags::NONE);
    link_child(&mut arena, root, child);
    link_child(&mut arena, child, grandchild);

    arena.refresh_subtree_dirty_cache(root);
    arena.mark_dirty(child, DirtyFlags::PAINT);
    arena.mark_dirty(grandchild, DirtyFlags::PAINT);
    arena.clear_arena_dirty_subtree(child, DirtyFlags::PAINT);

    assert!(!arena.subtree_dirty_intersects(grandchild, DirtyFlags::PAINT));
    assert!(!arena.subtree_dirty_contains(grandchild, DirtyFlags::PAINT));
    assert!(!arena.subtree_dirty_intersects(child, DirtyFlags::PAINT));
    assert!(!arena.subtree_dirty_contains(child, DirtyFlags::PAINT));
    assert!(!arena.subtree_dirty_intersects(root, DirtyFlags::PAINT));
    assert!(!arena.subtree_dirty_contains(root, DirtyFlags::PAINT));
}

#[test]
fn subtree_dirty_query_scopes_sibling_dirty_to_ancestor() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let dirty_sibling = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
    let clean_sibling = insert_test_node(&mut arena, 3, DirtyFlags::NONE);
    link_child(&mut arena, root, dirty_sibling);
    link_child(&mut arena, root, clean_sibling);

    arena.refresh_subtree_dirty_cache(root);
    arena.mark_dirty(dirty_sibling, DirtyFlags::PAINT);

    assert!(arena.subtree_dirty_intersects(root, DirtyFlags::PAINT));
    assert!(arena.subtree_dirty_contains(root, DirtyFlags::PAINT));
    assert!(arena.subtree_dirty_intersects(dirty_sibling, DirtyFlags::PAINT));
    assert!(!arena.subtree_dirty_intersects(clean_sibling, DirtyFlags::PAINT));
    assert!(!arena.subtree_dirty_contains(clean_sibling, DirtyFlags::PAINT));
}

#[test]
fn descendant_dirty_bubble_updates_ancestor_cached_subtree_dirty() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let child = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
    let grandchild = insert_test_node(&mut arena, 3, DirtyFlags::NONE);
    link_child(&mut arena, root, child);
    link_child(&mut arena, child, grandchild);

    arena.refresh_subtree_dirty_cache(root);
    assert!(
        !arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );

    arena.bubble_cached_subtree_dirty(grandchild, DirtyFlags::PAINT);

    assert!(
        arena
            .cached_subtree_dirty(grandchild)
            .intersects(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(child)
            .intersects(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );
}

#[test]
fn mark_dirty_updates_arena_local_dirty_and_ancestor_cached_subtree_dirty() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let child = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
    let grandchild = insert_test_node(&mut arena, 3, DirtyFlags::NONE);
    link_child(&mut arena, root, child);
    link_child(&mut arena, child, grandchild);

    arena.refresh_subtree_dirty_cache(root);
    assert_eq!(arena.arena_local_dirty(grandchild), DirtyFlags::NONE);
    assert!(
        !arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );

    arena.mark_dirty(grandchild, DirtyFlags::PAINT);

    assert!(
        arena
            .arena_local_dirty(grandchild)
            .contains(DirtyFlags::PAINT)
    );
    assert_eq!(arena.arena_local_dirty(child), DirtyFlags::NONE);
    assert_eq!(arena.arena_local_dirty(root), DirtyFlags::NONE);
    assert!(
        arena
            .cached_subtree_dirty(grandchild)
            .intersects(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(child)
            .intersects(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );
}

#[test]
fn clear_arena_dirty_removes_ancestor_paint_when_element_local_is_clean() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let child = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
    let grandchild = insert_test_node(&mut arena, 3, DirtyFlags::NONE);
    link_child(&mut arena, root, child);
    link_child(&mut arena, child, grandchild);

    arena.refresh_subtree_dirty_cache(root);
    arena.mark_dirty(grandchild, DirtyFlags::PAINT);
    assert!(
        arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );

    arena.clear_arena_dirty(grandchild, DirtyFlags::PAINT);

    assert_eq!(arena.arena_local_dirty(grandchild), DirtyFlags::NONE);
    assert_cached_paint_clean(&arena, grandchild);
    assert_cached_paint_clean(&arena, child);
    assert_cached_paint_clean(&arena, root);
}

#[test]
fn clear_arena_dirty_keeps_ancestor_paint_when_element_local_is_dirty() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let child = insert_test_node(&mut arena, 2, DirtyFlags::PAINT);
    link_child(&mut arena, root, child);

    arena.refresh_subtree_dirty_cache(root);
    arena.mark_dirty(child, DirtyFlags::PAINT);
    assert!(arena.arena_local_dirty(child).contains(DirtyFlags::PAINT));

    arena.clear_arena_dirty(child, DirtyFlags::PAINT);

    assert_eq!(arena.arena_local_dirty(child), DirtyFlags::NONE);
    assert!(
        arena
            .get(child)
            .expect("child exists")
            .element
            .local_dirty_flags()
            .contains(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(child)
            .intersects(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );
}

#[test]
fn clear_arena_dirty_keeps_ancestor_paint_from_dirty_sibling() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let left = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
    let right = insert_test_node(&mut arena, 3, DirtyFlags::NONE);
    link_child(&mut arena, root, left);
    link_child(&mut arena, root, right);

    arena.refresh_subtree_dirty_cache(root);
    arena.mark_dirty(left, DirtyFlags::PAINT);
    arena.mark_dirty(right, DirtyFlags::PAINT);

    arena.clear_arena_dirty(left, DirtyFlags::PAINT);

    assert_eq!(arena.arena_local_dirty(left), DirtyFlags::NONE);
    assert!(arena.arena_local_dirty(right).contains(DirtyFlags::PAINT));
    assert_cached_paint_clean(&arena, left);
    assert!(
        arena
            .cached_subtree_dirty(right)
            .intersects(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );
}

#[test]
fn clear_arena_dirty_keeps_ancestor_paint_from_dirty_descendant() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let child = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
    let grandchild = insert_test_node(&mut arena, 3, DirtyFlags::NONE);
    link_child(&mut arena, root, child);
    link_child(&mut arena, child, grandchild);

    arena.refresh_subtree_dirty_cache(root);
    arena.mark_dirty(child, DirtyFlags::PAINT);
    arena.mark_dirty(grandchild, DirtyFlags::PAINT);

    arena.clear_arena_dirty(child, DirtyFlags::PAINT);

    assert_eq!(arena.arena_local_dirty(child), DirtyFlags::NONE);
    assert!(
        arena
            .arena_local_dirty(grandchild)
            .contains(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(grandchild)
            .intersects(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(child)
            .intersects(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );
}

#[test]
fn clear_arena_dirty_subtree_keeps_root_local_arena_dirty() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let child = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
    let grandchild = insert_test_node(&mut arena, 3, DirtyFlags::NONE);
    link_child(&mut arena, root, child);
    link_child(&mut arena, child, grandchild);

    arena.refresh_subtree_dirty_cache(root);
    arena.mark_dirty(root, DirtyFlags::PAINT);
    arena.mark_dirty(child, DirtyFlags::PAINT);
    arena.mark_dirty(grandchild, DirtyFlags::PAINT);

    arena.clear_arena_dirty_subtree(child, DirtyFlags::PAINT);

    assert!(arena.arena_local_dirty(root).contains(DirtyFlags::PAINT));
    assert_eq!(arena.arena_local_dirty(child), DirtyFlags::NONE);
    assert_eq!(arena.arena_local_dirty(grandchild), DirtyFlags::NONE);
    assert_cached_paint_clean(&arena, grandchild);
    assert_cached_paint_clean(&arena, child);
    assert!(
        arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );
}

#[test]
fn clear_arena_dirty_subtree_root_removes_paint_when_elements_are_clean() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let child = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
    let grandchild = insert_test_node(&mut arena, 3, DirtyFlags::NONE);
    link_child(&mut arena, root, child);
    link_child(&mut arena, child, grandchild);

    arena.refresh_subtree_dirty_cache(root);
    arena.mark_dirty(root, DirtyFlags::PAINT);
    arena.mark_dirty(child, DirtyFlags::PAINT);
    arena.mark_dirty(grandchild, DirtyFlags::PAINT);

    arena.clear_arena_dirty_subtree(root, DirtyFlags::PAINT);

    assert_eq!(arena.arena_local_dirty(root), DirtyFlags::NONE);
    assert_eq!(arena.arena_local_dirty(child), DirtyFlags::NONE);
    assert_eq!(arena.arena_local_dirty(grandchild), DirtyFlags::NONE);
    assert_cached_paint_clean(&arena, grandchild);
    assert_cached_paint_clean(&arena, child);
    assert_cached_paint_clean(&arena, root);
}

#[test]
fn clear_cached_arena_dirty_subtree_repairs_only_matching_dirty_branches() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let layout_branch = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
    let paint_branch = insert_test_node(&mut arena, 3, DirtyFlags::NONE);
    let layout_leaf = insert_test_node(&mut arena, 4, DirtyFlags::NONE);
    link_child(&mut arena, root, layout_branch);
    link_child(&mut arena, root, paint_branch);
    link_child(&mut arena, layout_branch, layout_leaf);

    arena.refresh_subtree_dirty_cache(root);
    arena.mark_dirty(layout_leaf, DirtyFlags::LAYOUT);
    arena.mark_dirty(paint_branch, DirtyFlags::PAINT);

    arena.clear_cached_arena_dirty_subtree(root, DirtyFlags::LAYOUT);

    assert_eq!(arena.arena_local_dirty(layout_leaf), DirtyFlags::NONE);
    assert!(!arena.subtree_dirty_intersects(root, DirtyFlags::LAYOUT));
    assert!(
        arena
            .arena_local_dirty(paint_branch)
            .contains(DirtyFlags::PAINT)
    );
    assert!(arena.subtree_dirty_intersects(root, DirtyFlags::PAINT));
}

#[test]
fn clear_arena_dirty_subtree_keeps_paint_from_element_local_dirty() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let child = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
    let grandchild = insert_test_node(&mut arena, 3, DirtyFlags::PAINT);
    link_child(&mut arena, root, child);
    link_child(&mut arena, child, grandchild);

    arena.refresh_subtree_dirty_cache(root);
    arena.mark_dirty(child, DirtyFlags::PAINT);
    arena.mark_dirty(grandchild, DirtyFlags::PAINT);

    arena.clear_arena_dirty_subtree(child, DirtyFlags::PAINT);

    assert_eq!(arena.arena_local_dirty(child), DirtyFlags::NONE);
    assert_eq!(arena.arena_local_dirty(grandchild), DirtyFlags::NONE);
    assert!(
        arena
            .get(grandchild)
            .expect("grandchild exists")
            .element
            .local_dirty_flags()
            .contains(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(grandchild)
            .intersects(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(child)
            .intersects(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );
}

#[test]
fn clear_arena_dirty_subtree_keeps_sibling_dirty_and_ancestor_aggregate() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let left = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
    let left_child = insert_test_node(&mut arena, 3, DirtyFlags::NONE);
    let right = insert_test_node(&mut arena, 4, DirtyFlags::NONE);
    link_child(&mut arena, root, left);
    link_child(&mut arena, left, left_child);
    link_child(&mut arena, root, right);

    arena.refresh_subtree_dirty_cache(root);
    arena.mark_dirty(left, DirtyFlags::PAINT);
    arena.mark_dirty(left_child, DirtyFlags::PAINT);
    arena.mark_dirty(right, DirtyFlags::PAINT);

    arena.clear_arena_dirty_subtree(left, DirtyFlags::PAINT);

    assert_eq!(arena.arena_local_dirty(left), DirtyFlags::NONE);
    assert_eq!(arena.arena_local_dirty(left_child), DirtyFlags::NONE);
    assert!(arena.arena_local_dirty(right).contains(DirtyFlags::PAINT));
    assert_cached_paint_clean(&arena, left_child);
    assert_cached_paint_clean(&arena, left);
    assert!(
        arena
            .cached_subtree_dirty(right)
            .intersects(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );
}

#[test]
fn repair_after_child_local_and_arena_dirty_clear_removes_stale_ancestor_flags() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let child = insert_test_node(&mut arena, 2, DirtyFlags::PAINT);
    link_child(&mut arena, root, child);

    arena.refresh_subtree_dirty_cache(root);
    assert!(
        arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );

    arena
        .get_mut(child)
        .expect("child exists")
        .element
        .clear_local_dirty_flags(DirtyFlags::PAINT);
    arena.clear_arena_dirty(child, DirtyFlags::PAINT);

    assert!(
        !arena
            .cached_subtree_dirty(child)
            .intersects(DirtyFlags::PAINT)
    );
    assert!(
        !arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );
}

#[test]
fn new_nodes_default_cached_subtree_dirty_to_all() {
    let mut arena = NodeArena::new();
    let inserted = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let with_key =
        arena.insert_with_key(|_| Node::new(Box::new(TestElement::new(2, DirtyFlags::NONE))));

    assert_eq!(arena.cached_subtree_dirty(inserted), DirtyFlags::ALL);
    assert_eq!(arena.cached_subtree_dirty(with_key), DirtyFlags::ALL);
    assert_eq!(arena.arena_local_dirty(inserted), DirtyFlags::NONE);
    assert_eq!(arena.arena_local_dirty(with_key), DirtyFlags::NONE);
}
