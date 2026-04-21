//! Arena fixture helpers shared across unit-test modules.
//!
//! Tests used to build trees as `Box<dyn ElementTrait>` via
//! `Element::add_child(Box::new(...))`. Under Approach-C the retained
//! tree lives in `NodeArena` keyed by `NodeKey`, so tests have to
//! commit their fixtures into an arena and query back through it.
//!
//! These helpers are deliberately thin — just enough to cut the
//! ceremonial boilerplate (`commit_descriptor_tree(&mut arena, None,
//! ElementDescriptor { element: Box::new(x), children: vec![],
//! post_commit: None })`) down to a one-liner while leaving the
//! underlying primitives visible.
#![cfg(test)]
#![allow(dead_code)]

use crate::view::base_component::{
    ElementTrait, LayoutConstraints, LayoutPlacement,
};
use crate::view::node_arena::{NodeArena, NodeKey};
use crate::view::renderer_adapter::{
    ElementDescriptor, commit_descriptor_tree, rsx_to_descriptors_with_context,
};
use crate::Style;

/// Fresh empty arena.
pub(crate) fn new_test_arena() -> NodeArena {
    NodeArena::new()
}

/// Commit a single element at the root of the arena. Returns its key.
pub(crate) fn commit_element(
    arena: &mut NodeArena,
    element: Box<dyn ElementTrait>,
) -> NodeKey {
    commit_descriptor_tree(arena, None, ElementDescriptor::leaf(element))
}

/// Commit a single element as a child of `parent`. Returns the child's key.
///
/// Uses `commit_descriptor_tree` under the hood, which keeps both
/// `Node.children` and the parent `Element.children` mirror in sync.
pub(crate) fn commit_child(
    arena: &mut NodeArena,
    parent: NodeKey,
    element: Box<dyn ElementTrait>,
) -> NodeKey {
    // Go through arena_insert_child so the parent's Element.children
    // mirror is updated and the new node is appended at the end.
    let index = arena.children_of(parent).len();
    crate::view::renderer_adapter::arena_insert_child(
        arena,
        parent,
        index,
        ElementDescriptor::leaf(element),
    )
}

/// Commit a pre-built descriptor tree. Thin wrapper over
/// [`commit_descriptor_tree`] that keeps the call-site terse.
pub(crate) fn commit_descriptor(
    arena: &mut NodeArena,
    parent: Option<NodeKey>,
    desc: ElementDescriptor,
) -> NodeKey {
    commit_descriptor_tree(arena, parent, desc)
}

/// Commit an entire RSX tree into a fresh arena. Returns the committed
/// root keys (fragments are flattened). Errors from conversion panic —
/// tests use this for happy-path trees.
pub(crate) fn commit_rsx_tree(
    arena: &mut NodeArena,
    tree: &crate::ui::RsxNode,
) -> Vec<NodeKey> {
    commit_rsx_tree_with_context(arena, tree, &Style::new(), 0.0, 0.0)
}

pub(crate) fn commit_rsx_tree_with_context(
    arena: &mut NodeArena,
    tree: &crate::ui::RsxNode,
    viewport_style: &Style,
    viewport_width: f32,
    viewport_height: f32,
) -> Vec<NodeKey> {
    let (descs, errors) = rsx_to_descriptors_with_context(
        tree,
        viewport_style,
        viewport_width,
        viewport_height,
    );
    if !errors.is_empty() {
        panic!("commit_rsx_tree: rsx conversion errors: {:?}", errors);
    }
    descs
        .into_iter()
        .map(|d| commit_descriptor_tree(arena, None, d))
        .collect()
}

/// Run measure+place on a root key via `with_element_taken`.
pub(crate) fn measure_and_place(
    arena: &mut NodeArena,
    root: NodeKey,
    constraints: LayoutConstraints,
    placement: LayoutPlacement,
) {
    arena.with_element_taken(root, |el, a| {
        el.measure(constraints, a);
        el.place(placement, a);
    });
}

/// Walk `walk_layout`-style snapshot over an arena subtree.
pub(crate) fn walk_layout_snapshot(
    arena: &NodeArena,
    key: NodeKey,
    out: &mut Vec<(f32, f32, f32, f32)>,
) {
    let Some(node) = arena.get(key) else { return; };
    let s = node.element.box_model_snapshot();
    out.push((s.x, s.y, s.width, s.height));
    let children = node.children.clone();
    drop(node);
    for child in children {
        walk_layout_snapshot(arena, child, out);
    }
}

/// Downcast the element stored at `key` to `&T`.
pub(crate) fn get_element<'a, T: 'static>(arena: &'a NodeArena, key: NodeKey) -> std::cell::Ref<'a, T> {
    let node = arena
        .get(key)
        .expect("get_element: key not found in arena");
    std::cell::Ref::map(node, |n| {
        n.element
            .as_any()
            .downcast_ref::<T>()
            .expect("get_element: wrong element type")
    })
}

/// Downcast the element stored at `key` to `&mut T`.
pub(crate) fn get_element_mut<'a, T: 'static>(
    arena: &'a NodeArena,
    key: NodeKey,
) -> std::cell::RefMut<'a, T> {
    let node = arena
        .get_mut(key)
        .expect("get_element_mut: key not found in arena");
    std::cell::RefMut::map(node, |n| {
        n.element
            .as_any_mut()
            .downcast_mut::<T>()
            .expect("get_element_mut: wrong element type")
    })
}

/// Take a `box_model_snapshot()` of the element stored at `key`.
/// Returns an owned snapshot so the arena borrow is released immediately.
pub(crate) fn child_snapshot(
    arena: &NodeArena,
    key: NodeKey,
) -> crate::view::base_component::BoxModelSnapshot {
    arena
        .get(key)
        .expect("child_snapshot: key not found")
        .element
        .box_model_snapshot()
}

/// Fetch the i-th child key of `parent`.
pub(crate) fn child_key(arena: &NodeArena, parent: NodeKey, i: usize) -> NodeKey {
    arena.children_of(parent)[i]
}

/// Snapshot the i-th child of `parent`.
pub(crate) fn nth_child_snapshot(
    arena: &NodeArena,
    parent: NodeKey,
    i: usize,
) -> crate::view::base_component::BoxModelSnapshot {
    let k = child_key(arena, parent, i);
    child_snapshot(arena, k)
}
