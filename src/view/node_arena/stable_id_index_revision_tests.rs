use super::*;
use crate::view::base_component::Element;

fn element(id: u64) -> Node {
    Node::new(Box::new(Element::new_with_id(id, 0., 0., 10., 10.)))
}

#[test]
fn lookup_revision_tracks_mapping_changes_without_invalidating_paint_only_edits() {
    let mut arena = NodeArena::new();
    let first = arena.insert(element(10));
    let second = arena.insert_with_key(|_| element(20));
    let original = arena.stable_id_index_revision();
    arena
        .get_mut(first)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .set_opacity(0.5);
    arena.refresh_stable_id_index();
    assert_eq!(arena.stable_id_index_revision(), original);
    let alias = arena.insert(element(10));
    assert_eq!(arena.find_by_stable_id(10), Some(alias));
    assert_ne!(arena.stable_id_index_revision(), original);
    let aliased = arena.stable_id_index_revision();
    arena.remove(first); // The index already points at the alias.
    assert_eq!(arena.stable_id_index_revision(), aliased);
    arena.remove_subtree(alias);
    assert_ne!(arena.stable_id_index_revision(), aliased);
    let removed = arena.stable_id_index_revision();
    *arena.get_mut(second).unwrap().element = Box::new(Element::new_with_id(30, 0., 0., 10., 10.));
    arena.refresh_stable_id_index();
    assert_ne!(arena.stable_id_index_revision(), removed);
    assert_eq!(arena.find_by_stable_id(20), None);
    assert_eq!(arena.find_by_stable_id(30), Some(second));
    let refreshed = arena.stable_id_index_revision();
    arena.insert(Node::new(Box::new(Placeholder)));
    arena.refresh_stable_id_index();
    assert_eq!(arena.stable_id_index_revision(), refreshed);
}

#[test]
fn exhausted_lookup_revision_never_certifies_reuse() {
    let mut arena = NodeArena::new();
    arena.stable_id_index_revision = u64::MAX;
    assert_eq!(arena.stable_id_index_revision(), None);
    arena.insert(element(10));
    arena.refresh_stable_id_index();
    assert_eq!(arena.stable_id_index_revision(), None);
}
