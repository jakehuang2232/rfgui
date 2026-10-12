use super::*;

#[test]
fn viewport_deferred_collection_uses_trait_and_preserves_nested_dfs_order() {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(RecordingElement::new(1, "root"))));
    let first = arena.insert(Node::new(Box::new(
        RecordingElement::new(2, "first").deferred(),
    )));
    let nested_normal = arena.insert(Node::new(Box::new(RecordingElement::new(
        3,
        "nested-normal",
    ))));
    let nested_deferred = arena.insert(Node::new(Box::new(
        RecordingElement::new(4, "nested-deferred").deferred(),
    )));
    let second = arena.insert(Node::new(Box::new(
        RecordingElement::new(5, "second").deferred(),
    )));
    arena.push_root(root);
    link_child(&mut arena, root, first);
    link_child(&mut arena, first, nested_normal);
    link_child(&mut arena, first, nested_deferred);
    link_child(&mut arena, root, second);

    assert_eq!(
        arena
            .collect_viewport_clip_nodes()
            .into_iter()
            .map(|node| (node.key, node.stable_id))
            .collect::<Vec<_>>(),
        vec![(first, 2), (nested_deferred, 4), (second, 5)]
    );
}
