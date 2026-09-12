use super::*;
use crate::view::base_component::Element;
use crate::view::node_arena::{Node, NodeKey};

fn viewport_with_root() -> (Viewport, NodeKey) {
    let mut viewport = Viewport::new();
    let root = viewport
        .scene
        .node_arena
        .insert(Node::new(Box::new(Element::new_with_id(
            1, 0.0, 0.0, 120.0, 80.0,
        ))));
    viewport.scene.node_arena.push_root(root);
    viewport.scene.ui_root_keys.push(root);
    (viewport, root)
}

#[test]
fn generic_sync_advances_property_and_paint_observations_together() {
    let (mut viewport, root) = viewport_with_root();

    viewport.sync_compositor_property_trees();

    assert_eq!(viewport.compositor_property_tree_epoch(), 1);
    assert_eq!(viewport.compositor.paint_generations.epoch(), 1);
    assert!(
        viewport
            .compositor
            .paint_generations
            .snapshot(root)
            .is_some()
    );
    assert!(viewport.compositor.paint_generations.matches_live_snapshot(
        &viewport.scene.node_arena,
        &viewport.scene.ui_root_keys,
        &viewport.compositor.property_trees,
    ));
}
