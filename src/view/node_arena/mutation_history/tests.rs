use super::*;
use crate::view::base_component::Element;

#[test]
fn readers_are_independent_and_lost_history_fails_closed() {
    let mut arena = NodeArena::new();
    let key = arena.insert(Node::new(Box::new(Element::new(0., 0., 10., 10.))));
    let start = arena.mutation_clock();
    drop(arena.get_mut(key));
    let middle = arena.mutation_clock();
    drop(arena.get_mut(key));
    assert_eq!(arena.mutated_nodes_since(start), Some(vec![key]));
    assert_eq!(arena.mutated_nodes_since(middle), Some(vec![key]));
    assert_eq!(arena.mutated_nodes_since(start), Some(vec![key]));
    for _ in 0..CAPACITY {
        drop(arena.get_mut(key));
    }
    assert!(arena.mutated_nodes_since(start).is_none());
    assert_eq!(
        arena.mutated_nodes_since(arena.mutation_clock()),
        Some(vec![])
    );
    assert!(
        arena
            .mutated_nodes_since(arena.mutation_clock() + 1)
            .is_none()
    );
    arena.mutation_clock.set(u64::MAX);
    assert!(arena.mutated_nodes_since(u64::MAX).is_none());
}

#[test]
fn removed_subtrees_remain_visible_to_readers_after_slots_disappear() {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new(0., 0., 10., 10.))));
    let child = arena.insert(Node::new(Box::new(Element::new(0., 0., 10., 10.))));
    arena.set_parent(child, Some(root));
    arena.set_children(root, vec![child]);
    let start = arena.mutation_clock();
    assert_eq!(arena.remove_subtree(root), 2);
    let changed = arena
        .mutated_nodes_since(start)
        .unwrap()
        .into_iter()
        .collect::<FxHashSet<_>>();
    assert_eq!(changed, [root, child].into_iter().collect());
}
