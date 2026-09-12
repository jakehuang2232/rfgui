use super::*;

#[test]
fn deferred_build_uses_node_key_when_stable_ids_collide() {
    RECORDED_BUILDS.with(|builds| builds.borrow_mut().clear());

    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(RecordingElement::new(1, "root"))));
    let first = arena.insert(Node::new(Box::new(RecordingElement::new(42, "first"))));
    let second = arena.insert(Node::new(Box::new(RecordingElement::new(42, "second"))));
    link_child(&mut arena, root, first);
    link_child(&mut arena, root, second);

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(400, 300, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    ctx.register_deferred(second, 42);
    let deferred: Vec<_> = std::iter::from_fn(|| ctx.next_deferred()).collect();
    assert_eq!(deferred.len(), 1);

    crate::view::base_component::build_node_by_key(
        deferred[0].key,
        deferred[0].stable_id,
        &mut graph,
        &mut arena,
        &mut ctx,
    );

    RECORDED_BUILDS.with(|builds| {
        assert_eq!(&*builds.borrow(), &["second"]);
    });
}

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

#[test]
fn deferred_queue_deduplicates_repeated_node_registration() {
    let mut arena = NodeArena::new();
    let key = arena.insert(Node::new(Box::new(RecordingElement::new(42, "node"))));
    let mut ctx = UiBuildContext::new(400, 300, wgpu::TextureFormat::Bgra8Unorm, 1.0);

    ctx.register_deferred(key, 42);
    ctx.register_deferred(key, 99);

    assert_eq!(
        ctx.next_deferred().map(|node| (node.key, node.stable_id)),
        Some((key, 42))
    );
    assert_eq!(ctx.next_deferred(), None);
}

#[test]
fn deferred_queue_accepts_new_nodes_while_it_is_being_drained() {
    let mut arena = NodeArena::new();
    let first = arena.insert(Node::new(Box::new(RecordingElement::new(1, "first"))));
    let second = arena.insert(Node::new(Box::new(RecordingElement::new(2, "second"))));
    let mut ctx = UiBuildContext::new(400, 300, wgpu::TextureFormat::Bgra8Unorm, 1.0);

    ctx.register_deferred(first, 1);
    assert_eq!(ctx.next_deferred().map(|node| node.key), Some(first));

    ctx.register_deferred(second, 2);
    assert_eq!(ctx.next_deferred().map(|node| node.key), Some(second));
    assert_eq!(ctx.next_deferred(), None);
}

#[test]
fn deferred_queue_does_not_follow_a_cached_viewport_context_into_a_new_frame() {
    let mut arena = NodeArena::new();
    let old_node = arena.insert(Node::new(Box::new(RecordingElement::new(1, "old"))));
    let current_node = arena.insert(Node::new(Box::new(RecordingElement::new(2, "current"))));

    let mut old_frame = UiBuildContext::new(400, 300, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let cached_viewport = old_frame.viewport();
    old_frame.register_deferred(old_node, 1);

    let current_frame = UiBuildContext::new(400, 300, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let mut rebuilt = UiBuildContext::from_parts(cached_viewport, current_frame.into_state());
    rebuilt.register_deferred(current_node, 2);

    assert_eq!(
        rebuilt.next_deferred().map(|node| node.key),
        Some(current_node)
    );
    assert_eq!(rebuilt.next_deferred(), None);
    assert_eq!(
        old_frame.next_deferred().map(|node| node.key),
        Some(old_node)
    );
}

#[test]
fn deferred_queue_does_not_follow_a_cached_build_state_into_a_new_frame() {
    let mut arena = NodeArena::new();
    let old_node = arena.insert(Node::new(Box::new(RecordingElement::new(1, "old"))));
    let current_node = arena.insert(Node::new(Box::new(RecordingElement::new(2, "current"))));

    let mut old_frame = UiBuildContext::new(400, 300, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    old_frame.register_deferred(old_node, 1);
    let cached_state = old_frame.state_clone();

    let current_frame = UiBuildContext::new(400, 300, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let mut rebuilt = UiBuildContext::from_parts(current_frame.viewport(), cached_state);
    rebuilt.register_deferred(current_node, 2);

    assert_eq!(
        rebuilt.next_deferred().map(|node| node.key),
        Some(current_node)
    );
    assert_eq!(rebuilt.next_deferred(), None);
    assert_eq!(
        old_frame.next_deferred().map(|node| node.key),
        Some(old_node)
    );
}

#[test]
fn deferred_queue_is_shared_with_a_layer_subtree_context_in_the_current_frame() {
    let mut arena = NodeArena::new();
    let root_node = arena.insert(Node::new(Box::new(RecordingElement::new(1, "root"))));
    let layer_node = arena.insert(Node::new(Box::new(RecordingElement::new(2, "layer"))));
    let mut root_ctx = UiBuildContext::new(400, 300, wgpu::TextureFormat::Bgra8Unorm, 1.0);

    root_ctx.register_deferred(root_node, 1);
    let layer_state =
        root_ctx.layer_subtree_state_with_ancestor_clip(root_ctx.ancestor_clip_context());
    let mut layer_ctx = UiBuildContext::from_parts(root_ctx.viewport(), layer_state);
    layer_ctx.register_deferred(layer_node, 2);

    assert_eq!(
        root_ctx.next_deferred().map(|node| node.key),
        Some(root_node)
    );
    assert_eq!(
        root_ctx.next_deferred().map(|node| node.key),
        Some(layer_node)
    );
    assert_eq!(root_ctx.next_deferred(), None);
}
