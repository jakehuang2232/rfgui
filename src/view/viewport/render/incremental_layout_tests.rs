use super::*;
use crate::view::base_component::{DirtyFlags, Element};
use crate::view::test_support::{commit_child, commit_element};

#[test]
fn clean_native_root_preserves_revision_but_resize_and_dirty_child_run_layout() {
    let mut viewport = Viewport::new();
    viewport.logical_width = 120.;
    viewport.logical_height = 80.;
    let arena = &mut viewport.scene.node_arena;
    let root = commit_element(arena, Box::new(Element::new(0., 0., 100., 60.)));
    let child = commit_child(arena, root, Box::new(Element::new(0., 0., 20., 16.)));
    viewport.scene.ui_root_keys = vec![root];
    viewport.run_layout_pass();
    for key in [root, child] {
        viewport
            .scene
            .node_arena
            .clear_element_dirty_flags(key, DirtyFlags::ALL);
    }
    let before = viewport.scene.node_arena.mutation_revision(root);
    viewport.run_layout_pass();
    assert_eq!(viewport.scene.node_arena.mutation_revision(root), before);
    viewport.logical_width = 140.;
    viewport.run_layout_pass();
    assert_ne!(viewport.scene.node_arena.mutation_revision(root), before);
    let before = viewport.scene.node_arena.mutation_revision(root);
    viewport
        .scene
        .node_arena
        .mark_dirty(child, DirtyFlags::LAYOUT);
    viewport.run_layout_pass();
    assert_ne!(viewport.scene.node_arena.mutation_revision(root), before);
}
