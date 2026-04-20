//! Flat, index-based storage for the retained element tree.
//!
//! Approach C of the event-system architecture migration: elements live in
//! a `SlotMap` keyed by [`NodeKey`]. Parent/child wiring lives on the
//! per-slot [`Node`] wrapper rather than on `ElementTrait`, so custom
//! components stay focused on behaviour (layout / render / dispatch) and
//! don't have to know about arena plumbing.
//!
//! Why this exists:
//! - Dispatch needs `&Viewport` while a handler holds `&mut` to the
//!   current element. Nested ownership makes that impossible; an arena
//!   behind a shared reference does not.
//! - Handler APIs (`target.parent()`, `closest(pred)`, `data::<T>()`) need
//!   live access to other nodes — not a snapshot.
//! - Future features (a11y tree, devtools inspector, arbitrary
//!   cross-subtree queries) fall out naturally.

use slotmap::SlotMap;
use std::cell::{Ref, RefCell, RefMut};

use crate::view::base_component::ElementTrait;

/// Placeholder element swapped into a slot while its real element is
/// being operated on outside the arena (see
/// [`NodeArena::with_element_taken`]). Exists only to keep slot
/// invariants intact during the take-process-commit dance; its methods
/// are minimal stubs and must not be invoked in normal flow.
pub struct Placeholder;

impl crate::view::base_component::Layoutable for Placeholder {
    fn measure(
        &mut self,
        _constraints: crate::view::base_component::LayoutConstraints,
        _arena: &mut NodeArena,
    ) {
    }
    fn place(
        &mut self,
        _placement: crate::view::base_component::LayoutPlacement,
        _arena: &mut NodeArena,
    ) {
    }
    fn measured_size(&self) -> (f32, f32) {
        (0.0, 0.0)
    }
    fn set_layout_width(&mut self, _width: f32) {}
    fn set_layout_height(&mut self, _height: f32) {}
}
impl crate::view::base_component::EventTarget for Placeholder {}
impl crate::view::base_component::Renderable for Placeholder {
    fn build(
        &mut self,
        _graph: &mut crate::view::frame_graph::FrameGraph,
        _arena: &mut NodeArena,
        ctx: crate::view::base_component::UiBuildContext,
    ) -> crate::view::base_component::BuildState {
        ctx.into_state()
    }
}
impl ElementTrait for Placeholder {
    fn id(&self) -> u64 {
        0
    }
    fn box_model_snapshot(&self) -> crate::view::base_component::BoxModelSnapshot {
        crate::view::base_component::BoxModelSnapshot {
            node_id: 0,
            parent_id: None,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            border_radius: 0.0,
            should_render: false,
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

slotmap::new_key_type! {
    /// Generational handle for an element stored in [`NodeArena`].
    ///
    /// Stable for the element's lifetime, invalidated on removal. Reused
    /// indices carry a bumped generation, so a stale handle read after
    /// removal returns `None` instead of aliasing a new element.
    pub struct NodeKey;
}

/// Tree-wiring wrapper around an element.
///
/// Owns the `Box<dyn ElementTrait>` plus the structural metadata
/// (parent / children) that was previously scattered across each element
/// type. Keeping wiring here means `ElementTrait` stays behaviour-only
/// and custom components don't grow boilerplate fields.
pub struct Node {
    pub element: Box<dyn ElementTrait>,
    pub parent: Option<NodeKey>,
    pub children: Vec<NodeKey>,
    /// Aggregate of `element.local_dirty_flags()` unioned with every
    /// descendant's flags, refreshed once per layout pass by
    /// [`NodeArena::refresh_subtree_dirty_cache`]. Lets the layout hot loops
    /// short-circuit what used to be an O(N²) subtree walk into an O(1)
    /// field read.
    ///
    /// Default is `DirtyFlags::ALL` so newly inserted nodes are always seen
    /// as dirty until the next pre-pass runs.
    pub cached_subtree_dirty: crate::view::base_component::DirtyFlags,
}

impl Node {
    pub fn new(element: Box<dyn ElementTrait>) -> Self {
        Self {
            element,
            parent: None,
            children: Vec::new(),
            cached_subtree_dirty: crate::view::base_component::DirtyFlags::ALL,
        }
    }

    pub fn with_parent(element: Box<dyn ElementTrait>, parent: Option<NodeKey>) -> Self {
        Self {
            element,
            parent,
            children: Vec::new(),
            cached_subtree_dirty: crate::view::base_component::DirtyFlags::ALL,
        }
    }
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Node")
            .field("parent", &self.parent)
            .field("children", &self.children)
            .finish_non_exhaustive()
    }
}

/// Owns all retained tree nodes.
#[derive(Default)]
pub struct NodeArena {
    slots: SlotMap<NodeKey, RefCell<Node>>,
    /// Top-level nodes (one per RSX root). Kept here rather than on
    /// individual elements so the arena itself is enough to traverse.
    roots: Vec<NodeKey>,
}

impl NodeArena {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Root node keys in insertion order. One entry per top-level RSX tree.
    pub fn roots(&self) -> &[NodeKey] {
        &self.roots
    }

    pub fn set_roots(&mut self, roots: Vec<NodeKey>) {
        self.roots = roots;
    }

    pub fn push_root(&mut self, key: NodeKey) {
        self.roots.push(key);
    }

    pub fn clear_roots(&mut self) {
        self.roots.clear();
    }

    /// Insert a pre-built `Node`. Prefer [`Self::insert_with_key`] when the
    /// node's own storage needs to reference its key.
    pub fn insert(&mut self, node: Node) -> NodeKey {
        self.slots.insert(RefCell::new(node))
    }

    /// Two-phase init: reserve a key, then let the closure build the node
    /// with that key already known. Useful when the element stores its own
    /// key (currently rare — `Node.parent/children` cover most uses).
    pub fn insert_with_key<F>(&mut self, f: F) -> NodeKey
    where
        F: FnOnce(NodeKey) -> Node,
    {
        self.slots.insert_with_key(|k| RefCell::new(f(k)))
    }

    /// Remove an element and its `Node` wrapper. Does **not** cascade to
    /// children — callers must walk the subtree (use
    /// [`Self::remove_subtree`] for recursive removal).
    pub fn remove(&mut self, key: NodeKey) -> Option<Node> {
        self.slots.remove(key).map(RefCell::into_inner)
    }

    /// Recursively remove `key` and all descendants. Returns the number of
    /// nodes actually removed.
    pub fn remove_subtree(&mut self, key: NodeKey) -> usize {
        // Collect keys depth-first before removing so we do not walk while
        // mutating.
        let mut to_remove = Vec::new();
        self.collect_subtree_keys(key, &mut to_remove);
        let mut removed = 0;
        for k in to_remove {
            if self.slots.remove(k).is_some() {
                removed += 1;
            }
        }
        removed
    }

    fn collect_subtree_keys(&self, key: NodeKey, out: &mut Vec<NodeKey>) {
        let Some(cell) = self.slots.get(key) else {
            return;
        };
        let children: Vec<NodeKey> = cell.borrow().children.clone();
        for child in children {
            self.collect_subtree_keys(child, out);
        }
        out.push(key);
    }

    pub fn get(&self, key: NodeKey) -> Option<Ref<'_, Node>> {
        self.slots.get(key).map(|cell| cell.borrow())
    }

    pub fn get_mut(&self, key: NodeKey) -> Option<RefMut<'_, Node>> {
        self.slots.get(key).map(|cell| cell.borrow_mut())
    }

    /// Fallible mutable borrow — returns `None` when the slot is already
    /// borrowed. Use inside dispatch when a handler may recursively query
    /// its own element so the call returns gracefully instead of panicking.
    pub fn try_get_mut(&self, key: NodeKey) -> Option<RefMut<'_, Node>> {
        self.slots.get(key).and_then(|cell| cell.try_borrow_mut().ok())
    }

    pub fn contains_key(&self, key: NodeKey) -> bool {
        self.slots.contains_key(key)
    }

    /// Clone the child list of `key`. Returning owned `Vec` lets the caller
    /// iterate and recurse without holding a `Ref` into the arena.
    pub fn children_of(&self, key: NodeKey) -> Vec<NodeKey> {
        self.slots
            .get(key)
            .map(|cell| cell.borrow().children.clone())
            .unwrap_or_default()
    }

    pub fn parent_of(&self, key: NodeKey) -> Option<NodeKey> {
        self.slots.get(key).and_then(|cell| cell.borrow().parent)
    }

    pub fn set_parent(&self, key: NodeKey, parent: Option<NodeKey>) {
        if let Some(cell) = self.slots.get(key) {
            cell.borrow_mut().parent = parent;
        }
    }

    pub fn set_children(&self, key: NodeKey, children: Vec<NodeKey>) {
        if let Some(cell) = self.slots.get(key) {
            cell.borrow_mut().children = children;
        }
    }

    pub fn push_child(&self, parent: NodeKey, child: NodeKey) {
        if let Some(cell) = self.slots.get(parent) {
            cell.borrow_mut().children.push(child);
        }
    }

    /// Post-order walk rooted at `key` that refreshes
    /// [`Node::cached_subtree_dirty`] on every visited node. Each cache
    /// entry is `element.local_dirty_flags() ∪ union(child.cached_subtree_dirty)`.
    ///
    /// Call once at the top of each layout pass so the subsequent
    /// measure/place hot loops can read the cache in O(1) instead of
    /// walking the whole subtree per node (the O(N²) trap that bit the
    /// arena refactor).
    pub fn refresh_subtree_dirty_cache(&self, key: NodeKey) -> crate::view::base_component::DirtyFlags {
        use crate::view::base_component::DirtyFlags;
        let Some(cell) = self.slots.get(key) else {
            return DirtyFlags::NONE;
        };
        // Clone child list without holding a borrow into the cell, so we
        // can recurse and then re-borrow to write the cache.
        let children: Vec<NodeKey> = cell.borrow().children.clone();
        let mut aggregate = cell.borrow().element.local_dirty_flags();
        for child in children {
            aggregate = aggregate.union(self.refresh_subtree_dirty_cache(child));
        }
        cell.borrow_mut().cached_subtree_dirty = aggregate;
        aggregate
    }

    /// Fast O(1) read of the cached aggregate dirty flags for the subtree
    /// rooted at `key`. The cache is stale unless
    /// [`Self::refresh_subtree_dirty_cache`] has been called this pass.
    pub fn cached_subtree_dirty(&self, key: NodeKey) -> crate::view::base_component::DirtyFlags {
        self.slots
            .get(key)
            .map(|cell| cell.borrow().cached_subtree_dirty)
            .unwrap_or(crate::view::base_component::DirtyFlags::NONE)
    }

    /// Iterator over every live (key, Node) pair. Each yielded item holds a
    /// `Ref` — release it before mutating the same slot.
    pub fn iter(&self) -> impl Iterator<Item = (NodeKey, Ref<'_, Node>)> {
        self.slots.iter().map(|(k, cell)| (k, cell.borrow()))
    }

    /// Take the element out of slot `key`, run `f` with exclusive access
    /// to the element plus an unaliased `&mut NodeArena` (the slot
    /// temporarily holds a [`Placeholder`]), then put the real element
    /// back. Panics inside `f` leave the placeholder in place rather than
    /// poisoning the slot, so arena invariants survive even on unwind.
    ///
    /// Returns `None` if `key` is missing. During `f`, looking up `key`
    /// again in the arena yields the placeholder — callers are expected
    /// not to re-enter their own slot.
    pub fn with_element_taken<R>(
        &mut self,
        key: NodeKey,
        f: impl FnOnce(&mut Box<dyn ElementTrait>, &mut NodeArena) -> R,
    ) -> Option<R> {
        // Phase 1: swap the real element out for a placeholder.
        // We need &mut access to the slot's RefCell contents. `get_mut`
        // on SlotMap gives &mut RefCell<Node>; `.get_mut()` on RefCell
        // gives &mut Node without runtime borrow tracking.
        let taken: Box<dyn ElementTrait> = {
            let cell = self.slots.get_mut(key)?;
            let node: &mut Node = cell.get_mut();
            std::mem::replace(&mut node.element, Box::new(Placeholder))
        };

        // RAII guard: if `f` panics, keep the placeholder permanently in
        // the slot (we can't recover the real element anyway — it's
        // being unwound with the panic). That preserves the invariant
        // that every live slot has *some* element in it.
        struct Guard<'a> {
            arena: &'a mut NodeArena,
            key: NodeKey,
            taken: Option<Box<dyn ElementTrait>>,
        }
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                if let Some(element) = self.taken.take() {
                    // Normal path: put the real element back.
                    if let Some(cell) = self.arena.slots.get_mut(self.key) {
                        cell.get_mut().element = element;
                    }
                }
                // Panic path: `taken` is None because we moved it into
                // `f`; the placeholder stays in place.
            }
        }

        let mut guard = Guard {
            arena: self,
            key,
            taken: Some(taken),
        };

        // Move the element out of the guard while we run `f`, so that on
        // panic `guard.taken` is already `None` and the guard leaves the
        // placeholder in place.
        let mut element = guard.taken.take().expect("guard initialised with element");
        let result = f(&mut element, guard.arena);
        // Re-stash for the guard's Drop to put back.
        guard.taken = Some(element);
        drop(guard);

        Some(result)
    }
}

impl std::fmt::Debug for NodeArena {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeArena")
            .field("len", &self.slots.len())
            .field("roots", &self.roots)
            .finish()
    }
}

// -----------------------------------------------------------------------------
// Read-only view exposed to event handlers.

/// Read-only view of the arena handed to handlers via [`EventTarget`].
///
/// Shares the arena borrow without granting mutation — handlers can walk
/// ancestors, query state, and look up sibling nodes without fighting the
/// dispatcher for a mutable borrow.
#[derive(Clone, Copy)]
pub struct ViewportRef<'a> {
    arena: &'a NodeArena,
}

impl<'a> ViewportRef<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        Self { arena }
    }

    pub fn arena(&self) -> &'a NodeArena {
        self.arena
    }

    pub fn node(&self, key: NodeKey) -> Option<NodeRef<'a>> {
        self.arena.get(key).map(|guard| NodeRef {
            key,
            guard,
            viewport: *self,
        })
    }
}

/// Transient handle to a node for the duration of a handler call.
///
/// Wraps a `Ref` into the arena slot; parent / ancestor / sibling lookups
/// are handled by re-borrowing through `viewport`, so `NodeRef` does not
/// keep the arena globally pinned beyond its own lifetime.
///
/// Phase A1 carries key + guard only. Full accessors (`parent`,
/// `ancestors`, `tag`, `role`, `state`, `closest`, `data`) land with
/// Phase C once element behaviour exposes the required queries.
pub struct NodeRef<'a> {
    key: NodeKey,
    guard: Ref<'a, Node>,
    viewport: ViewportRef<'a>,
}

impl<'a> NodeRef<'a> {
    pub fn key(&self) -> NodeKey {
        self.key
    }

    /// Viewport handle this `NodeRef` was obtained from — handy for
    /// traversing to other nodes without plumbing the ref separately.
    pub fn viewport(&self) -> ViewportRef<'a> {
        self.viewport
    }

    /// Parent node, if any.
    pub fn parent(&self) -> Option<NodeRef<'a>> {
        self.guard.parent.and_then(|p| self.viewport.node(p))
    }

    /// Walk the parent chain lazily, starting from `self.parent()`.
    pub fn ancestors(&self) -> Ancestors<'a> {
        Ancestors {
            next: self.guard.parent,
            viewport: self.viewport,
        }
    }

    /// Child keys as a slice — zero-copy.
    pub fn children(&self) -> &[NodeKey] {
        &self.guard.children
    }

    /// Borrow the underlying element trait object.
    pub fn element(&self) -> &dyn ElementTrait {
        self.guard.element.as_ref()
    }
}

impl std::fmt::Debug for NodeRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeRef").field("key", &self.key).finish()
    }
}

/// Iterator returned by [`NodeRef::ancestors`]. Walks parent links until
/// either a root (no parent) or a stale/missing slot is reached.
pub struct Ancestors<'a> {
    next: Option<NodeKey>,
    viewport: ViewportRef<'a>,
}

impl<'a> Iterator for Ancestors<'a> {
    type Item = NodeRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let key = self.next?;
        let node = self.viewport.node(key)?;
        self.next = node.guard.parent;
        Some(node)
    }
}
