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

use rustc_hash::{FxHashMap, FxHashSet};
use slotmap::SlotMap;
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::ops::Deref;

use crate::view::base_component::{DirtyFlags, ElementTrait, PlacementSkipFailureReason};

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
    fn stable_id(&self) -> u64 {
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
    element: RefCell<Box<dyn ElementTrait>>,
    pub(crate) parent: Option<NodeKey>,
    pub(crate) children: Vec<NodeKey>,
    /// Arena-owned local dirty bits for the node itself.
    ///
    /// This is scaffold for migrating invalidation ownership out of
    /// elements. The formal layout pass still reads
    /// `element.local_dirty_flags()` through
    /// [`NodeArena::refresh_subtree_dirty_cache`]; this field is only
    /// updated by the new arena invalidation APIs for now.
    pub(crate) arena_local_dirty: Cell<DirtyFlags>,
    /// Aggregate of `element.local_dirty_flags()` unioned with every
    /// descendant's flags, refreshed once per layout pass by
    /// [`NodeArena::refresh_subtree_dirty_cache`]. Lets the layout hot loops
    /// short-circuit what used to be an O(N²) subtree walk into an O(1)
    /// field read.
    ///
    /// Default is `DirtyFlags::ALL` so newly inserted nodes are always seen
    /// as dirty until the next pre-pass runs.
    pub(crate) cached_subtree_dirty: Cell<crate::view::base_component::DirtyFlags>,
    /// Aggregate placement-replay eligibility metadata for this subtree.
    ///
    /// This is Phase 5b scaffold. It is refreshed with the existing
    /// subtree-dirty pre-pass so observation can avoid recursively scanning
    /// candidate subtrees, but it is not a standalone skip truth: callers
    /// must still check dirty bits, placement keys, clip/anchor context, and
    /// runtime state guards where relevant.
    pub(crate) cached_placement_eligibility: Cell<PlacementEligibilityMetadata>,
}

impl Node {
    pub fn new(element: Box<dyn ElementTrait>) -> Self {
        Self {
            element: RefCell::new(element),
            parent: None,
            children: Vec::new(),
            arena_local_dirty: Cell::new(DirtyFlags::NONE),
            cached_subtree_dirty: Cell::new(crate::view::base_component::DirtyFlags::ALL),
            cached_placement_eligibility: Cell::new(PlacementEligibilityMetadata::unknown()),
        }
    }

    pub fn with_parent(element: Box<dyn ElementTrait>, parent: Option<NodeKey>) -> Self {
        Self {
            element: RefCell::new(element),
            parent,
            children: Vec::new(),
            arena_local_dirty: Cell::new(DirtyFlags::NONE),
            cached_subtree_dirty: Cell::new(crate::view::base_component::DirtyFlags::ALL),
            cached_placement_eligibility: Cell::new(PlacementEligibilityMetadata::unknown()),
        }
    }

    pub fn parent(&self) -> Option<NodeKey> {
        self.parent
    }

    pub fn children(&self) -> &[NodeKey] {
        &self.children
    }

    fn borrow(&self) -> NodeGuard<'_> {
        NodeGuard {
            node: self,
            element: self.element.borrow(),
        }
    }

    fn borrow_mut(&self) -> NodeMutGuard<'_> {
        NodeMutGuard {
            node: self,
            element: self.element.borrow_mut(),
        }
    }

    fn try_borrow_mut(&self) -> Option<NodeMutGuard<'_>> {
        Some(NodeMutGuard {
            node: self,
            element: self.element.try_borrow_mut().ok()?,
        })
    }
}

pub struct NodeGuard<'a> {
    node: &'a Node,
    pub element: Ref<'a, Box<dyn ElementTrait>>,
}

impl Deref for NodeGuard<'_> {
    type Target = Node;

    fn deref(&self) -> &Self::Target {
        self.node
    }
}

pub struct NodeMutGuard<'a> {
    node: &'a Node,
    pub element: RefMut<'a, Box<dyn ElementTrait>>,
}

impl Deref for NodeMutGuard<'_> {
    type Target = Node;

    fn deref(&self) -> &Self::Target {
        self.node
    }
}

/// Cached subtree metadata for future placement replay eligibility checks.
///
/// This deliberately excludes placement-local dirty. Dirty state already has
/// a separate arena cache and must remain the first guard before this
/// metadata is consulted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlacementEligibilityMetadata {
    pub contains_non_base_element: bool,
    pub contains_anchor_name: bool,
    pub contains_anchor_ref: bool,
    pub contains_absolute_descendant: bool,
    pub contains_runtime_layout_state: bool,
    /// True if the subtree contains any host that has not opted into the
    /// translation fast-path (`ElementTrait::translate_in_place`). A pure
    /// ancestor move can only be replayed as a cheap subtree translation
    /// when every node knows how to shift its own absolute geometry; an
    /// un-opted host forces the full re-place fallback. Independent of
    /// `first_blocker` so the existing placement-skip paths are unaffected.
    pub contains_non_translatable_host: bool,
}

impl Default for PlacementEligibilityMetadata {
    fn default() -> Self {
        Self::unknown()
    }
}

impl PlacementEligibilityMetadata {
    /// A fully transparent, translation-capable leaf: no placement-skip
    /// blockers and safe to shift via `translate_in_place`. Used by hosts
    /// that have opted into the translation fast-path (`Element`, `Text`,
    /// `Image`, `Svg`).
    pub(crate) const fn empty() -> Self {
        Self {
            contains_non_base_element: false,
            contains_anchor_name: false,
            contains_anchor_ref: false,
            contains_absolute_descendant: false,
            contains_runtime_layout_state: false,
            contains_non_translatable_host: false,
        }
    }

    /// A transparent leaf that has NOT opted into the translation
    /// fast-path: it contributes no placement-skip blocker (so stationary
    /// subtrees containing it can still skip) but forces the full re-place
    /// fallback for a moving ancestor. This is the `ElementTrait` default.
    pub(crate) const fn opaque_to_translation() -> Self {
        Self {
            contains_non_translatable_host: true,
            ..Self::empty()
        }
    }

    pub(crate) const fn unknown() -> Self {
        Self {
            contains_non_base_element: true,
            contains_anchor_name: false,
            contains_anchor_ref: false,
            contains_absolute_descendant: false,
            contains_runtime_layout_state: false,
            contains_non_translatable_host: true,
        }
    }

    /// A node that deliberately blocks placement-skip for its subtree
    /// (no anchor/absolute/runtime claim, just "do not skip me").
    pub const fn non_base_blocker() -> Self {
        Self {
            contains_non_base_element: true,
            contains_anchor_name: false,
            contains_anchor_ref: false,
            contains_absolute_descendant: false,
            contains_runtime_layout_state: false,
            contains_non_translatable_host: true,
        }
    }

    /// Whether a pure ancestor move over this subtree can be replayed as a
    /// cheap translation instead of a full re-place. Requires no
    /// placement-skip blocker AND every host opted into translation.
    pub(crate) fn is_translatable(self) -> bool {
        self.first_blocker().is_none() && !self.contains_non_translatable_host
    }

    fn union(self, rhs: Self) -> Self {
        Self {
            contains_non_base_element: self.contains_non_base_element
                || rhs.contains_non_base_element,
            contains_anchor_name: self.contains_anchor_name || rhs.contains_anchor_name,
            contains_anchor_ref: self.contains_anchor_ref || rhs.contains_anchor_ref,
            contains_absolute_descendant: self.contains_absolute_descendant
                || rhs.contains_absolute_descendant,
            contains_runtime_layout_state: self.contains_runtime_layout_state
                || rhs.contains_runtime_layout_state,
            contains_non_translatable_host: self.contains_non_translatable_host
                || rhs.contains_non_translatable_host,
        }
    }

    pub(crate) fn first_blocker(self) -> Option<PlacementSkipFailureReason> {
        if self.contains_anchor_name {
            return Some(PlacementSkipFailureReason::AnchorName);
        }
        if self.contains_anchor_ref {
            return Some(PlacementSkipFailureReason::AnchorRef);
        }
        if self.contains_absolute_descendant {
            return Some(PlacementSkipFailureReason::AbsoluteDescendant);
        }
        if self.contains_runtime_layout_state {
            return Some(PlacementSkipFailureReason::RuntimeState);
        }
        if self.contains_non_base_element {
            return Some(PlacementSkipFailureReason::NonBaseElement);
        }
        None
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
    slots: SlotMap<NodeKey, Node>,
    /// Top-level nodes (one per RSX root). Kept here rather than on
    /// individual elements so the arena itself is enough to traverse.
    roots: Vec<NodeKey>,
    /// Secondary index: `ElementTrait::stable_id()` → `NodeKey`. Powers
    /// the Phase A React-alignment Fiber lookup (`patch_to_fiber_work`
    /// needs to translate `Patch` paths anchored on stable ids into
    /// arena keys without a full tree walk).
    ///
    /// Invariants:
    /// - Only non-zero stable_ids are indexed (Placeholder returns 0;
    ///   legacy stubs may also return 0 — indexing would collide).
    /// - Updated on `insert` / `insert_with_key` / `remove` /
    ///   `remove_subtree`. `with_element_taken` leaves the slot's
    ///   stable_id invariant (placeholder swap-back is transparent).
    /// - Callers that rebuild an element's identity in place should
    ///   refresh via [`Self::refresh_stable_id_index`].
    stable_id_index: FxHashMap<u64, NodeKey>,
    /// Deterministic insertion-order list of hosts that explicitly opted into
    /// the pre-layout `sync_arena` hook.
    arena_sync_nodes: Vec<NodeKey>,
    /// Nesting depth for slots temporarily holding `Placeholder` during an
    /// element callback. Stable-id lookup may trust the wrapper index only for
    /// these explicitly tracked transient placeholders.
    taken_depths: RefCell<FxHashMap<NodeKey, u32>>,
}

/// Mutation-scoped handle for recording arena-owned invalidation.
///
/// The context intentionally exposes only invalidation, not arbitrary arena
/// mutation. Callers that need structural edits should continue using the
/// existing arena APIs until their call sites are migrated deliberately.
pub struct InvalidationContext<'a> {
    arena: &'a mut NodeArena,
    key: NodeKey,
}

impl InvalidationContext<'_> {
    pub(crate) fn arena(&mut self) -> &mut NodeArena {
        self.arena
    }

    pub fn invalidate(&mut self, flags: DirtyFlags) {
        self.arena.mark_dirty(self.key, flags);
    }

    /// Clear only the arena-owned dirty shadow for this node.
    ///
    /// This does not clear `Element::local_dirty_flags()`; element-owned
    /// dirty state remains part of the cached subtree dirty union while the
    /// invalidation migration is in progress.
    pub fn clear_arena_dirty(&mut self, flags: DirtyFlags) {
        self.arena.clear_arena_dirty(self.key, flags);
    }
}

/// Read-side mutation-scoped handle for recording arena-owned invalidation.
pub struct RefInvalidationContext<'a> {
    arena: &'a NodeArena,
    key: NodeKey,
}

impl RefInvalidationContext<'_> {
    pub(crate) fn arena(&self) -> &NodeArena {
        self.arena
    }

    pub fn invalidate(&mut self, flags: DirtyFlags) {
        self.arena.mark_dirty(self.key, flags);
    }

    /// Clear only the arena-owned dirty shadow for this node.
    ///
    /// This does not clear `Element::local_dirty_flags()`; element-owned
    /// dirty state remains part of the cached subtree dirty union while the
    /// invalidation migration is in progress.
    pub fn clear_arena_dirty(&mut self, flags: DirtyFlags) {
        self.arena.clear_arena_dirty(self.key, flags);
    }
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
    pub fn insert(&mut self, mut node: Node) -> NodeKey {
        let sid = node.element.get_mut().stable_id();
        let requires_arena_sync = node.element.get_mut().requires_arena_sync();
        let key = self.slots.insert(node);
        if sid != 0 {
            self.stable_id_index.insert(sid, key);
        }
        if requires_arena_sync {
            self.arena_sync_nodes.push(key);
        }
        key
    }

    /// Two-phase init: reserve a key, then let the closure build the node
    /// with that key already known. Useful when the element stores its own
    /// key (currently rare — `Node.parent/children` cover most uses).
    pub fn insert_with_key<F>(&mut self, f: F) -> NodeKey
    where
        F: FnOnce(NodeKey) -> Node,
    {
        let key = self.slots.insert_with_key(f);
        if let Some(node) = self.slots.get(key) {
            let element = node.element.borrow();
            let sid = element.stable_id();
            if sid != 0 {
                self.stable_id_index.insert(sid, key);
            }
            if element.requires_arena_sync() {
                self.arena_sync_nodes.push(key);
            }
        }
        key
    }

    /// Remove an element and its `Node` wrapper. Does **not** cascade to
    /// children — callers must walk the subtree (use
    /// [`Self::remove_subtree`] for recursive removal).
    pub fn remove(&mut self, key: NodeKey) -> Option<Node> {
        let node = self.slots.remove(key)?;
        self.arena_sync_nodes.retain(|&candidate| candidate != key);
        self.stable_id_index
            .retain(|_, indexed_key| *indexed_key != key);
        Some(node)
    }

    /// Recursively remove `key` and all descendants. Returns the number of
    /// nodes actually removed.
    pub fn remove_subtree(&mut self, key: NodeKey) -> usize {
        // `Node.children` is the active layout/render edge list, not the
        // complete ownership graph: inactive Image/Svg side slots deliberately
        // stay outside it. Build the ownership adjacency from `Node.parent`
        // once so removing a host also removes every detached side-slot tree.
        let mut owned_children: FxHashMap<NodeKey, Vec<NodeKey>> = FxHashMap::default();
        for (node_key, node) in self.slots.iter() {
            if let Some(parent) = node.parent {
                owned_children.entry(parent).or_default().push(node_key);
            }
        }

        // Collect keys depth-first before removing so we do not walk while
        // mutating the SlotMap.
        let mut to_remove = Vec::new();
        Self::collect_owned_subtree_keys(key, &owned_children, &mut to_remove);
        let removed_keys: FxHashSet<NodeKey> = to_remove.iter().copied().collect();

        // Keep the surviving topology coherent even when callers invoke this
        // low-level API directly rather than detaching through renderer_adapter.
        self.roots.retain(|root| !removed_keys.contains(root));
        if let Some(parent) = self.parent_of(key)
            && !removed_keys.contains(&parent)
        {
            let mut siblings = self.children_of(parent);
            siblings.retain(|child| !removed_keys.contains(child));
            self.set_children(parent, siblings);
        }

        self.stable_id_index
            .retain(|_, indexed_key| !removed_keys.contains(indexed_key));
        let mut removed = 0;
        for k in to_remove {
            if self.slots.remove(k).is_some() {
                removed += 1;
            }
        }
        self.arena_sync_nodes
            .retain(|candidate| self.slots.contains_key(*candidate));
        removed
    }

    /// Look up a node key by its element's `stable_id()`.
    ///
    /// Returns `None` when:
    /// - `id` is 0 (sentinel; never indexed).
    /// - No node with that stable id is currently in the arena.
    /// - The stable id has collided and the index currently points at a
    ///   different slot (callers that care should
    ///   [`Self::refresh_stable_id_index`] after any operation that may
    ///   rename stable ids in place).
    pub fn find_by_stable_id(&self, id: u64) -> Option<NodeKey> {
        if id == 0 {
            return None;
        }
        let key = *self.stable_id_index.get(&id)?;
        // Defensive: verify the slot still exists and matches. Stale
        // index entries should be impossible (insert/remove maintain the
        // invariant), but a missed refresh after a rare in-place id
        // change is cheaper to detect here than to debug later.
        let node = self.slots.get(key)?;
        let actual_id = node.element.borrow().stable_id();
        if actual_id == id {
            return Some(key);
        }
        (actual_id == 0 && self.taken_depths.borrow().contains_key(&key)).then_some(key)
    }

    /// Borrow the full stable-id → NodeKey index. Used by the Phase A
    /// incremental commit path (`fiber_work`) which wants a
    /// `&FxHashMap<u64, NodeKey>` to pass into
    /// [`crate::view::fiber_work::patch_to_fiber_work`].
    pub(crate) fn stable_id_index(&self) -> &FxHashMap<u64, NodeKey> {
        &self.stable_id_index
    }

    /// Full rescan of live slots that rebuilds `stable_id_index` from
    /// scratch. Use after any bulk mutation that bypasses the normal
    /// `insert` / `remove` path (e.g. a cold boot replay, or a fallback
    /// from the incremental-commit path back to a full rebuild).
    ///
    /// Non-zero stable ids win by last-write order; duplicates produce a
    /// single index entry pointing at whichever slot is visited last.
    pub fn refresh_stable_id_index(&mut self) {
        self.stable_id_index.clear();
        for (key, node) in self.slots.iter() {
            let sid = node.element.borrow().stable_id();
            if sid != 0 {
                self.stable_id_index.insert(sid, key);
            }
        }
    }

    /// Collect every element that must render through
    /// the deferred root-viewport phase (`Position::absolute()` +
    /// `ClipMode::Viewport`) in document order (DFS, root descendants only).
    ///
    /// Roots themselves are rendered through the main walk, so they are
    /// **excluded** — appending them would render the same element twice
    /// (once inline, once via the deferred phase) and scramble z-ordering
    /// for top-level viewport-clip shells like `<Window>`.
    pub(crate) fn collect_viewport_clip_nodes(
        &self,
    ) -> Vec<crate::view::base_component::DeferredRenderNode> {
        let mut out = Vec::new();
        for &root_key in &self.roots {
            let child_count = self
                .slots
                .get(root_key)
                .map(|node| node.children.len())
                .unwrap_or(0);
            for index in 0..child_count {
                if let Some(child_key) = self.child_key_at(root_key, index) {
                    self.collect_viewport_clip_nodes_subtree(child_key, &mut out);
                }
            }
        }
        out
    }

    fn collect_viewport_clip_nodes_subtree(
        &self,
        key: NodeKey,
        out: &mut Vec<crate::view::base_component::DeferredRenderNode>,
    ) {
        let Some(node) = self.slots.get(key) else {
            return;
        };
        let child_count = node.children.len();
        let element = node.element.borrow();
        if element.is_deferred_to_root_viewport_render() {
            let sid = element.stable_id();
            if sid != 0 {
                out.push(crate::view::base_component::DeferredRenderNode {
                    key,
                    stable_id: sid,
                });
            }
        }
        drop(element);
        for index in 0..child_count {
            if let Some(child_key) = self.child_key_at(key, index) {
                self.collect_viewport_clip_nodes_subtree(child_key, out);
            }
        }
    }

    /// Seed `ctx`'s deferred render list using the popup stack as
    /// Render order is **document order** (DFS over root descendants,
    /// parent before child, earlier sibling before later). z-order is a
    /// property of tree position, not interaction history.
    ///
    /// As a side effect, mutates `popup_stack`:
    ///
    /// 1. Compact: drop ids that no longer resolve in the arena.
    /// 2. Auto-register: every collected viewport-clip id is appended
    ///    to the top of the stack if not already present.
    ///
    /// The stack drives **hit-test priority only** — render emits in
    /// document order regardless of stack position.
    pub fn seed_defer_render_with_stack(
        &self,
        popup_stack: &mut crate::view::popup_stack::PopupStack,
        ctx: &mut crate::view::base_component::UiBuildContext,
    ) {
        popup_stack.compact(self);
        let collected = self.collect_viewport_clip_nodes();
        for node in &collected {
            popup_stack.register(node.stable_id);
        }
        for node in &collected {
            ctx.register_deferred(node.key, node.stable_id);
        }
    }

    fn collect_owned_subtree_keys(
        key: NodeKey,
        owned_children: &FxHashMap<NodeKey, Vec<NodeKey>>,
        out: &mut Vec<NodeKey>,
    ) {
        if let Some(children) = owned_children.get(&key) {
            for &child in children {
                Self::collect_owned_subtree_keys(child, owned_children, out);
            }
        }
        out.push(key);
    }

    fn collect_active_subtree_keys(&self, key: NodeKey, out: &mut Vec<NodeKey>) {
        let child_count = self
            .slots
            .get(key)
            .map(|node| node.children.len())
            .unwrap_or(0);
        for index in 0..child_count {
            if let Some(child) = self.child_key_at(key, index) {
                self.collect_active_subtree_keys(child, out);
            }
        }
        if self.slots.contains_key(key) {
            out.push(key);
        }
    }

    pub fn get(&self, key: NodeKey) -> Option<NodeGuard<'_>> {
        self.slots.get(key).map(Node::borrow)
    }

    pub fn get_mut(&self, key: NodeKey) -> Option<NodeMutGuard<'_>> {
        self.slots.get(key).map(Node::borrow_mut)
    }

    /// Fallible mutable borrow — returns `None` when the slot is already
    /// borrowed. Use inside dispatch when a handler may recursively query
    /// its own element so the call returns gracefully instead of panicking.
    pub fn try_get_mut(&self, key: NodeKey) -> Option<NodeMutGuard<'_>> {
        self.slots.get(key).and_then(Node::try_borrow_mut)
    }

    pub fn contains_key(&self, key: NodeKey) -> bool {
        self.slots.contains_key(key)
    }

    #[cfg(test)]
    pub(crate) fn arena_sync_node_count_for_test(&self) -> usize {
        self.arena_sync_nodes.len()
    }

    /// Clone the child list of `key`. Returning owned `Vec` lets the caller
    /// iterate and recurse without holding a `Ref` into the arena.
    pub fn children_of(&self, key: NodeKey) -> Vec<NodeKey> {
        self.slots
            .get(key)
            .map(|node| node.children.clone())
            .unwrap_or_default()
    }

    /// Allocation-free child access for hot whole-tree walks: one slot
    /// borrow per lookup instead of cloning the child Vec per node.
    pub fn child_key_at(&self, key: NodeKey, index: usize) -> Option<NodeKey> {
        self.slots
            .get(key)
            .and_then(|node| node.children.get(index).copied())
    }

    pub fn parent_of(&self, key: NodeKey) -> Option<NodeKey> {
        self.slots.get(key).and_then(|node| node.parent)
    }

    /// Walk the parent chain from `key` until reaching a node with no
    /// parent. Returns the topmost ancestor (the containing root) — or
    /// `key` itself if it is already a root or not present in the arena.
    pub fn root_for(&self, key: NodeKey) -> NodeKey {
        let mut current = key;
        while let Some(parent) = self.parent_of(current) {
            current = parent;
        }
        current
    }

    pub fn set_parent(&mut self, key: NodeKey, parent: Option<NodeKey>) {
        if let Some(node) = self.slots.get_mut(key) {
            node.parent = parent;
        }
    }

    pub fn set_children(&mut self, key: NodeKey, children: Vec<NodeKey>) {
        if let Some(node) = self.slots.get_mut(key) {
            node.children = children.clone();
            node.element.get_mut().sync_children_mirror(&children);
        }
    }

    #[cfg(test)]
    pub(crate) fn set_arena_children_without_mirror_for_test(
        &mut self,
        key: NodeKey,
        children: Vec<NodeKey>,
    ) {
        if let Some(node) = self.slots.get_mut(key) {
            node.children = children;
        }
    }

    pub fn push_child(&mut self, parent: NodeKey, child: NodeKey) {
        if let Some(node) = self.slots.get_mut(parent) {
            node.children.push(child);
            node.element.get_mut().sync_children_mirror(&node.children);
        }
    }

    /// Run the pre-layout sync hook only for hosts that opted in through
    /// `Layoutable::requires_arena_sync`.
    pub fn sync_registered_elements(&mut self) {
        // A sync hook may structurally mutate the arena, so iterate a stable
        // snapshot rather than borrowing the registration list across calls.
        let registered = self.arena_sync_nodes.clone();
        for key in registered {
            self.with_element_taken(key, |element, arena| element.sync_arena(arena));
        }
    }

    /// Freeze paint-only resources for registered hosts after final layout.
    /// The element hook receives no arena reference, so this pass cannot
    /// invalidate layout by changing child topology.
    pub fn prepare_registered_paint_resources(
        &mut self,
        context: crate::view::base_component::PaintResourcePreparationContext,
    ) {
        let registered = self.arena_sync_nodes.clone();
        for key in registered {
            self.with_element_taken(key, |element, _arena| {
                element.prepare_paint_resources(context)
            });
        }
    }

    /// Post-order walk rooted at `key` that refreshes
    /// [`Node::cached_subtree_dirty`] on every visited node. Each cache
    /// entry is `element.local_dirty_flags() ∪ arena_local_dirty ∪
    /// union(child.cached_subtree_dirty)`.
    ///
    /// Call once at the top of each layout pass so the subsequent
    /// measure/place hot loops can read the cache in O(1) instead of
    /// walking the whole subtree per node (the O(N²) trap that bit the
    /// arena refactor).
    pub fn refresh_subtree_dirty_cache(
        &self,
        key: NodeKey,
    ) -> crate::view::base_component::DirtyFlags {
        use crate::view::base_component::DirtyFlags;
        let Some(node) = self.slots.get(key) else {
            return DirtyFlags::NONE;
        };
        // Read only the count before recursion. Child keys are fetched one at
        // a time so this twice-per-layout-pass walk does not allocate a cloned
        // Vec for every container.
        let (child_count, mut aggregate, mut placement_eligibility) = {
            let element = node.element.borrow();
            (
                node.children.len(),
                element
                    .local_dirty_flags()
                    .union(node.arena_local_dirty.get()),
                element.placement_eligibility_metadata(),
            )
        };
        for index in 0..child_count {
            if let Some(child) = self.child_key_at(key, index) {
                aggregate = aggregate.union(self.refresh_subtree_dirty_cache(child));
                placement_eligibility =
                    placement_eligibility.union(self.cached_placement_eligibility_metadata(child));
            }
        }
        node.cached_subtree_dirty.set(aggregate);
        node.cached_placement_eligibility.set(placement_eligibility);
        aggregate
    }

    /// Fast read of Phase 5b cached placement eligibility metadata.
    ///
    /// The cache is refreshed by [`Self::refresh_subtree_dirty_cache`]. A
    /// caller that has not run that pre-pass must treat this as stale or
    /// pessimistic scaffold data, not as permission to skip placement.
    pub(crate) fn cached_placement_eligibility_metadata(
        &self,
        key: NodeKey,
    ) -> PlacementEligibilityMetadata {
        self.slots
            .get(key)
            .map(|node| node.cached_placement_eligibility.get())
            .unwrap_or_else(PlacementEligibilityMetadata::unknown)
    }

    /// Record arena-owned local dirty bits for `key` and bubble the same
    /// bits into the cached subtree aggregate for its ancestors.
    ///
    /// This is a shadow invalidation path for future migration work. It
    /// does not mutate `element.local_dirty_flags()` and does not affect
    /// the formal full-root layout refresh, which still uses
    /// [`Self::refresh_subtree_dirty_cache`].
    pub fn mark_dirty(&self, key: NodeKey, flags: DirtyFlags) {
        if flags.is_empty() {
            return;
        }

        let Some(node) = self.slots.get(key) else {
            return;
        };
        node.arena_local_dirty
            .set(node.arena_local_dirty.get().union(flags));
        self.bubble_cached_subtree_dirty(key, flags);
    }

    /// Shadow dirty propagation for incremental layout experiments.
    ///
    /// ORs `flags` into `Node::cached_subtree_dirty` for `key` and each
    /// ancestor. This does not mutate the element's own dirty flags and
    /// does not change the formal layout pass, which still uses
    /// [`Self::refresh_subtree_dirty_cache`] as its full refresh path.
    #[allow(dead_code)]
    pub(crate) fn bubble_cached_subtree_dirty(
        &self,
        key: NodeKey,
        flags: crate::view::base_component::DirtyFlags,
    ) {
        if flags.is_empty() {
            return;
        }

        let mut current = Some(key);
        while let Some(node_key) = current {
            let Some(node) = self.slots.get(node_key) else {
                return;
            };
            node.cached_subtree_dirty
                .set(node.cached_subtree_dirty.get().union(flags));
            current = node.parent;
        }
    }

    /// Recompute cached subtree dirty flags for `key`, then repeat for
    /// each ancestor.
    ///
    /// Use after a node clears local dirty flags in an incremental path.
    /// Unlike [`Self::bubble_cached_subtree_dirty`], this can remove stale
    /// aggregate bits because each visited node is rebuilt from its current
    /// local dirty state and direct children caches.
    ///
    /// This is still shadow-state repair: while the formal layout pass has
    /// not moved ownership away from `Element::local_dirty_flags()`, the
    /// incremental cache must reflect `element.local_dirty_flags() ∪
    /// arena_local_dirty ∪ children.cached_subtree_dirty` so arena-owned mark
    /// and clear operations do not fight each other.
    #[allow(dead_code)]
    pub(crate) fn repair_cached_subtree_dirty_ancestors(
        &self,
        key: NodeKey,
    ) -> crate::view::base_component::DirtyFlags {
        use crate::view::base_component::DirtyFlags;

        let mut current = Some(key);
        let mut last = DirtyFlags::NONE;
        while let Some(node_key) = current {
            let Some(node) = self.slots.get(node_key) else {
                return last;
            };

            let (child_count, parent, previous, mut aggregate) = {
                let element = node.element.borrow();
                (
                    node.children.len(),
                    node.parent,
                    node.cached_subtree_dirty.get(),
                    element
                        .local_dirty_flags()
                        .union(node.arena_local_dirty.get()),
                )
            };

            for index in 0..child_count {
                let Some(child) = self.child_key_at(node_key, index) else {
                    break;
                };
                aggregate = aggregate.union(self.cached_subtree_dirty(child));
            }

            node.cached_subtree_dirty.set(aggregate);
            last = aggregate;
            // Ancestor aggregates take this node's cache as their input;
            // when it did not change they are already consistent — stop
            // the climb. This keeps the common no-dirty-change access
            // O(children) instead of O(depth × children).
            if aggregate == previous {
                break;
            }
            current = parent;
        }
        last
    }

    /// Clear arena-owned local dirty bits for `key`, then repair the cached
    /// subtree aggregate from that node up to the root.
    ///
    /// This only mutates [`Node::arena_local_dirty`]. Element-owned dirty
    /// flags remain the formal source for the current layout pass, and repair
    /// keeps both sources unioned in the shadow cache while migration is in
    /// progress.
    pub fn clear_arena_dirty(&self, key: NodeKey, flags: DirtyFlags) {
        if flags.is_empty() {
            return;
        }

        let Some(node) = self.slots.get(key) else {
            return;
        };
        node.arena_local_dirty
            .set(node.arena_local_dirty.get().without(flags));
        self.repair_cached_subtree_dirty_ancestors(key);
    }

    /// Clear arena-owned local dirty bits for every node in `key`'s subtree,
    /// then repair cached subtree dirty aggregates once from `key` upward.
    ///
    /// This does not mutate `Element::local_dirty_flags()`. Element-owned
    /// dirty bits remain part of each cached aggregate while the invalidation
    /// migration is in progress.
    pub fn clear_arena_dirty_subtree(&self, key: NodeKey, flags: DirtyFlags) {
        if flags.is_empty() {
            return;
        }

        let mut subtree = Vec::new();
        self.collect_active_subtree_keys(key, &mut subtree);
        if subtree.is_empty() {
            return;
        }

        for node_key in &subtree {
            if let Some(node) = self.slots.get(*node_key) {
                node.arena_local_dirty
                    .set(node.arena_local_dirty.get().without(flags));
            }
        }

        for node_key in &subtree {
            let Some(node) = self.slots.get(*node_key) else {
                continue;
            };
            let (child_count, mut aggregate) = {
                let element = node.element.borrow();
                (
                    node.children.len(),
                    element
                        .local_dirty_flags()
                        .union(node.arena_local_dirty.get()),
                )
            };

            for index in 0..child_count {
                if let Some(child) = self.child_key_at(*node_key, index) {
                    aggregate = aggregate.union(self.cached_subtree_dirty(child));
                }
            }

            node.cached_subtree_dirty.set(aggregate);
        }

        // The post-order loop above already wrote fresh aggregates for the
        // whole subtree (including `key`), so the ancestor climb must start
        // at the parent: repair's no-change early-out would otherwise see
        // `key` unchanged and stop before reaching stale ancestors.
        if let Some(parent) = self.parent_of(key) {
            self.repair_cached_subtree_dirty_ancestors(parent);
        }
    }

    /// Clear arena-owned dirty bits only along branches whose refreshed
    /// subtree cache intersects `flags`.
    ///
    /// Unlike [`Self::clear_arena_dirty_subtree`], this is a layout-pass hot
    /// path and requires a current (or conservatively dirty) subtree cache.
    /// Clean sibling branches are left untouched and keep their already-valid
    /// aggregate, so clearing one dirty leaf costs O(dirty branches + depth)
    /// rather than O(the entire root subtree).
    pub(crate) fn clear_cached_arena_dirty_subtree(&self, key: NodeKey, flags: DirtyFlags) {
        if flags.is_empty() || !self.subtree_dirty_intersects(key, flags) {
            return;
        }

        fn collect_dirty_postorder(
            arena: &NodeArena,
            key: NodeKey,
            flags: DirtyFlags,
            out: &mut Vec<NodeKey>,
        ) {
            if !arena.subtree_dirty_intersects(key, flags) {
                return;
            }
            let child_count = arena
                .slots
                .get(key)
                .map(|node| node.children.len())
                .unwrap_or(0);
            for index in 0..child_count {
                if let Some(child) = arena.child_key_at(key, index) {
                    collect_dirty_postorder(arena, child, flags, out);
                }
            }
            if arena.slots.contains_key(key) {
                out.push(key);
            }
        }

        let mut dirty_subtree = Vec::new();
        collect_dirty_postorder(self, key, flags, &mut dirty_subtree);
        for node_key in &dirty_subtree {
            if let Some(node) = self.slots.get(*node_key) {
                node.arena_local_dirty
                    .set(node.arena_local_dirty.get().without(flags));
            }
        }
        for node_key in &dirty_subtree {
            let Some(node) = self.slots.get(*node_key) else {
                continue;
            };
            let element = node.element.borrow();
            let mut aggregate = element
                .local_dirty_flags()
                .union(node.arena_local_dirty.get());
            drop(element);
            for index in 0..node.children.len() {
                if let Some(child) = self.child_key_at(*node_key, index) {
                    aggregate = aggregate.union(self.cached_subtree_dirty(child));
                }
            }
            node.cached_subtree_dirty.set(aggregate);
        }
        if let Some(parent) = self.parent_of(key) {
            self.repair_cached_subtree_dirty_ancestors(parent);
        }
    }

    /// Fast O(1) read of the cached aggregate dirty flags for the subtree
    /// rooted at `key`. The cache is stale unless
    /// [`Self::refresh_subtree_dirty_cache`] has been called this pass.
    pub fn cached_subtree_dirty(&self, key: NodeKey) -> crate::view::base_component::DirtyFlags {
        self.slots
            .get(key)
            .map(|node| node.cached_subtree_dirty.get())
            .unwrap_or(crate::view::base_component::DirtyFlags::NONE)
    }

    /// Read-side API for future incremental traversal gating.
    ///
    /// This is an O(1) query against the current cached subtree aggregate; it
    /// does not walk or refresh the subtree. Correctness depends on callers
    /// having already refreshed or repaired the cache through the appropriate
    /// dirty-cache path. This does not change formal layout/render traversal
    /// behavior.
    pub fn subtree_dirty_intersects(&self, key: NodeKey, flags: DirtyFlags) -> bool {
        self.slots
            .get(key)
            .map(|node| node.cached_subtree_dirty.get().intersects(flags))
            .unwrap_or(false)
    }

    /// Read-side API for future incremental traversal gating.
    ///
    /// This is an O(1) query against the current cached subtree aggregate; it
    /// does not walk or refresh the subtree. Correctness depends on callers
    /// having already refreshed or repaired the cache through the appropriate
    /// dirty-cache path. This does not change formal layout/render traversal
    /// behavior.
    pub fn subtree_dirty_contains(&self, key: NodeKey, flags: DirtyFlags) -> bool {
        self.slots
            .get(key)
            .map(|node| node.cached_subtree_dirty.get().contains(flags))
            .unwrap_or(false)
    }

    /// Fast read of the arena-owned local dirty bits for `key`.
    pub fn arena_local_dirty(&self, key: NodeKey) -> DirtyFlags {
        self.slots
            .get(key)
            .map(|node| node.arena_local_dirty.get())
            .unwrap_or(DirtyFlags::NONE)
    }

    /// Iterator over every live (key, Node) pair. Each yielded item holds a
    /// `Ref` — release it before mutating the same slot.
    pub fn iter(&self) -> impl Iterator<Item = (NodeKey, NodeGuard<'_>)> {
        self.slots.iter().map(|(key, node)| (key, node.borrow()))
    }

    fn begin_element_take(&self, key: NodeKey) {
        let mut depths = self.taken_depths.borrow_mut();
        *depths.entry(key).or_insert(0) += 1;
    }

    fn finish_element_take(&self, key: NodeKey) {
        let mut depths = self.taken_depths.borrow_mut();
        let Some(depth) = depths.get_mut(&key) else {
            return;
        };
        if *depth <= 1 {
            depths.remove(&key);
        } else {
            *depth -= 1;
        }
    }

    /// Take the element out of slot `key`, run `f` with exclusive access
    /// to the element plus an unaliased `&mut NodeArena` (the slot
    /// temporarily holds a [`Placeholder`]), then put the real element
    /// back. If `f` panics, the element is restored before the panic resumes,
    /// so a host-level unwind boundary cannot observe a placeholder slot.
    ///
    /// Returns `None` if `key` is missing. During `f`, looking up `key`
    /// again in the arena yields the placeholder — callers are expected
    /// not to re-enter their own slot.
    pub fn with_element_taken<R>(
        &mut self,
        key: NodeKey,
        f: impl FnOnce(&mut Box<dyn ElementTrait>, &mut NodeArena) -> R,
    ) -> Option<R> {
        // Phase 1: swap the real element out for a placeholder. Mutable arena
        // access reaches the element RefCell through `get_mut()` without a
        // runtime borrow check.
        let taken: Box<dyn ElementTrait> = {
            let node = self.slots.get_mut(key)?;
            std::mem::replace(node.element.get_mut(), Box::new(Placeholder))
        };
        self.begin_element_take(key);

        // RAII guard owns the real element throughout the callback and restores
        // it on both normal return and unwinding.
        struct Guard<'a> {
            arena: &'a mut NodeArena,
            key: NodeKey,
            taken: Option<Box<dyn ElementTrait>>,
        }
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                if let Some(element) = self.taken.take() {
                    // Normal path: put the real element back.
                    if let Some(node) = self.arena.slots.get_mut(self.key) {
                        *node.element.get_mut() = element;
                    }
                }
                self.arena.finish_element_take(self.key);
            }
        }

        let mut guard = Guard {
            arena: self,
            key,
            taken: Some(taken),
        };

        let result = f(
            guard
                .taken
                .as_mut()
                .expect("guard initialised with element"),
            guard.arena,
        );
        drop(guard);
        Some(result)
    }

    /// Take an element for mutation and provide a scoped invalidation
    /// context that records arena-owned dirty bits.
    ///
    /// This is intentionally opt-in scaffold for future setter migration;
    /// existing call sites keep using [`Self::with_element_taken`] until
    /// they are moved one by one.
    pub fn mutate_element_with_invalidation<R>(
        &mut self,
        key: NodeKey,
        f: impl FnOnce(&mut Box<dyn ElementTrait>, &mut InvalidationContext<'_>) -> R,
    ) -> Option<R> {
        let result = self.with_element_taken(key, |element, arena| {
            let mut cx = InvalidationContext { arena, key };
            f(element, &mut cx)
        });
        if result.is_some() {
            self.repair_cached_subtree_dirty_ancestors(key);
        }
        result
    }

    /// Read-side variant of [`Self::mutate_element_with_invalidation`].
    ///
    /// This mirrors [`Self::with_element_taken_ref`] for dispatch paths
    /// that hold `&NodeArena` while still allowing controlled arena-owned
    /// dirty propagation through the scoped invalidation context.
    pub fn mutate_element_ref_with_invalidation<R>(
        &self,
        key: NodeKey,
        f: impl FnOnce(&mut Box<dyn ElementTrait>, &mut RefInvalidationContext<'_>) -> R,
    ) -> Option<R> {
        let result = self.with_element_taken_ref(key, |element, arena| {
            let mut cx = RefInvalidationContext { arena, key };
            f(element, &mut cx)
        });
        if result.is_some() {
            self.repair_cached_subtree_dirty_ancestors(key);
        }
        result
    }

    /// Read-side counterpart of [`Self::with_element_taken`] for dispatch
    /// paths that only need `&NodeArena` inside the callback. Takes
    /// `&self` so dispatch sites can share-borrow the arena while a
    /// handler walks the tree (see `EventTarget` lazy accessors).
    ///
    /// The element is still swapped in place via the slot's inner
    /// `RefCell` so the callback receives `&mut Box<dyn ElementTrait>`
    /// for mutation of element-internal state. Structural mutation
    /// (insert / remove) remains on the `&mut self` API.
    pub fn with_element_taken_ref<R>(
        &self,
        key: NodeKey,
        f: impl FnOnce(&mut Box<dyn ElementTrait>, &NodeArena) -> R,
    ) -> Option<R> {
        let node = self.slots.get(key)?;
        let taken: Box<dyn ElementTrait> = {
            let mut element = node.element.borrow_mut();
            std::mem::replace(&mut *element, Box::new(Placeholder))
        };
        self.begin_element_take(key);

        struct Guard<'a> {
            arena: &'a NodeArena,
            key: NodeKey,
            taken: Option<Box<dyn ElementTrait>>,
        }
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                if let Some(element) = self.taken.take() {
                    if let Some(node) = self.arena.slots.get(self.key) {
                        *node.element.borrow_mut() = element;
                    }
                }
                self.arena.finish_element_take(self.key);
            }
        }

        let mut guard = Guard {
            arena: self,
            key,
            taken: Some(taken),
        };

        let result = f(
            guard
                .taken
                .as_mut()
                .expect("guard initialised with element"),
            guard.arena,
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Color;
    use crate::view::base_component::{
        BoxModelSnapshot, BuildState, DirtyFlags, Element, ElementTrait, EventTarget,
        LayoutConstraints, LayoutPlacement, Layoutable, Renderable, UiBuildContext,
    };
    use crate::view::frame_graph::FrameGraph;

    struct TestElement {
        stable_id: u64,
        dirty_flags: DirtyFlags,
    }

    thread_local! {
        static RECORDED_BUILDS: std::cell::RefCell<Vec<&'static str>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }

    struct RecordingElement {
        stable_id: u64,
        label: &'static str,
        deferred: bool,
    }

    impl TestElement {
        fn new(stable_id: u64, dirty_flags: DirtyFlags) -> Self {
            Self {
                stable_id,
                dirty_flags,
            }
        }
    }

    impl RecordingElement {
        fn new(stable_id: u64, label: &'static str) -> Self {
            Self {
                stable_id,
                label,
                deferred: false,
            }
        }

        fn deferred(mut self) -> Self {
            self.deferred = true;
            self
        }
    }

    impl Layoutable for TestElement {
        fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
        fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
        fn measured_size(&self) -> (f32, f32) {
            (0.0, 0.0)
        }
        fn set_layout_width(&mut self, _width: f32) {}
        fn set_layout_height(&mut self, _height: f32) {}
    }

    impl EventTarget for TestElement {}

    impl Renderable for TestElement {
        fn build(
            &mut self,
            _graph: &mut FrameGraph,
            _arena: &mut NodeArena,
            ctx: UiBuildContext,
        ) -> BuildState {
            ctx.into_state()
        }
    }

    impl ElementTrait for TestElement {
        fn stable_id(&self) -> u64 {
            self.stable_id
        }

        fn box_model_snapshot(&self) -> BoxModelSnapshot {
            BoxModelSnapshot {
                node_id: self.stable_id,
                parent_id: None,
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
                border_radius: 0.0,
                should_render: false,
            }
        }

        fn local_dirty_flags(&self) -> DirtyFlags {
            self.dirty_flags
        }

        fn clear_local_dirty_flags(&mut self, flags: DirtyFlags) {
            self.dirty_flags = self.dirty_flags.without(flags);
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    impl Layoutable for RecordingElement {
        fn sync_arena(&mut self, _arena: &mut NodeArena) {
            RECORDED_BUILDS.with(|builds| builds.borrow_mut().push(self.label));
        }
        fn requires_arena_sync(&self) -> bool {
            true
        }
        fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
        fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
        fn measured_size(&self) -> (f32, f32) {
            (0.0, 0.0)
        }
        fn set_layout_width(&mut self, _width: f32) {}
        fn set_layout_height(&mut self, _height: f32) {}
    }

    impl EventTarget for RecordingElement {}

    impl Renderable for RecordingElement {
        fn build(
            &mut self,
            _graph: &mut FrameGraph,
            _arena: &mut NodeArena,
            ctx: UiBuildContext,
        ) -> BuildState {
            RECORDED_BUILDS.with(|builds| builds.borrow_mut().push(self.label));
            ctx.into_state()
        }
    }

    impl ElementTrait for RecordingElement {
        fn stable_id(&self) -> u64 {
            self.stable_id
        }

        fn box_model_snapshot(&self) -> BoxModelSnapshot {
            BoxModelSnapshot {
                node_id: self.stable_id,
                parent_id: None,
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
                border_radius: 0.0,
                should_render: false,
            }
        }

        fn local_dirty_flags(&self) -> DirtyFlags {
            DirtyFlags::NONE
        }

        fn clear_local_dirty_flags(&mut self, _flags: DirtyFlags) {}

        fn is_deferred_to_root_viewport_render(&self) -> bool {
            self.deferred
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    fn insert_test_node(arena: &mut NodeArena, stable_id: u64, dirty_flags: DirtyFlags) -> NodeKey {
        arena.insert(Node::new(Box::new(TestElement::new(
            stable_id,
            dirty_flags,
        ))))
    }

    fn clean_element() -> Element {
        let mut element = Element::new(0.0, 0.0, 10.0, 10.0);
        element.clear_local_dirty_flags(DirtyFlags::ALL);
        element
    }

    fn link_child(arena: &mut NodeArena, parent: NodeKey, child: NodeKey) {
        arena.set_parent(child, Some(parent));
        arena.push_child(parent, child);
    }

    fn assert_element_and_arena_paint_dirty(arena: &NodeArena, root: NodeKey, child: NodeKey) {
        assert!(
            arena
                .get(child)
                .expect("child exists")
                .element
                .local_dirty_flags()
                .contains(DirtyFlags::PAINT)
        );
        assert!(arena.arena_local_dirty(child).contains(DirtyFlags::PAINT));
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

    fn assert_cached_paint_clean(arena: &NodeArena, key: NodeKey) {
        assert!(
            !arena
                .cached_subtree_dirty(key)
                .intersects(DirtyFlags::PAINT)
        );
    }

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
    fn mutate_element_with_invalidation_bubbles_invalidated_flags() {
        let mut arena = NodeArena::new();
        let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
        let child = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
        link_child(&mut arena, root, child);

        arena.refresh_subtree_dirty_cache(root);
        assert!(
            !arena
                .cached_subtree_dirty(root)
                .intersects(DirtyFlags::PAINT)
        );

        arena
            .mutate_element_with_invalidation(child, |_element, cx| {
                cx.invalidate(DirtyFlags::PAINT);
            })
            .expect("child exists");

        assert!(arena.arena_local_dirty(child).contains(DirtyFlags::PAINT));
        assert!(
            arena
                .cached_subtree_dirty(root)
                .intersects(DirtyFlags::PAINT)
        );
    }

    #[test]
    fn mutate_element_with_invalidation_context_can_clear_arena_dirty() {
        let mut arena = NodeArena::new();
        let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
        let child = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
        link_child(&mut arena, root, child);

        arena.refresh_subtree_dirty_cache(root);
        assert_cached_paint_clean(&arena, child);
        assert_cached_paint_clean(&arena, root);

        arena
            .mutate_element_with_invalidation(child, |_element, cx| {
                cx.invalidate(DirtyFlags::PAINT);
                cx.clear_arena_dirty(DirtyFlags::PAINT);
            })
            .expect("child exists");

        assert_eq!(arena.arena_local_dirty(child), DirtyFlags::NONE);
        assert_cached_paint_clean(&arena, child);
        assert_cached_paint_clean(&arena, root);
    }

    #[test]
    fn element_opacity_with_invalidation_updates_local_and_arena_dirty() {
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(clean_element())));
        let child = arena.insert(Node::new(Box::new(clean_element())));
        link_child(&mut arena, root, child);

        arena.refresh_subtree_dirty_cache(root);
        assert!(
            !arena
                .get(child)
                .expect("child exists")
                .element
                .local_dirty_flags()
                .intersects(DirtyFlags::PAINT)
        );
        assert_eq!(arena.arena_local_dirty(child), DirtyFlags::NONE);
        assert!(
            !arena
                .cached_subtree_dirty(root)
                .intersects(DirtyFlags::PAINT)
        );

        arena
            .mutate_element_with_invalidation(child, |element, cx| {
                element
                    .as_any_mut()
                    .downcast_mut::<Element>()
                    .expect("element")
                    .set_opacity_with_invalidation(0.5, cx);
            })
            .expect("child exists");

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
                .get(child)
                .expect("child exists")
                .element
                .local_dirty_flags()
                .contains(DirtyFlags::COMPOSITE)
        );
        assert!(arena.arena_local_dirty(child).contains(DirtyFlags::PAINT));
        assert!(
            arena
                .arena_local_dirty(child)
                .contains(DirtyFlags::COMPOSITE)
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
        assert!(
            arena
                .cached_subtree_dirty(child)
                .contains(DirtyFlags::COMPOSITE)
        );
        assert!(
            arena
                .cached_subtree_dirty(root)
                .contains(DirtyFlags::COMPOSITE)
        );
    }

    #[test]
    fn composite_shadow_dirty_bubbles_and_clears_independently() {
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(clean_element())));
        let child = arena.insert(Node::new(Box::new(clean_element())));
        link_child(&mut arena, root, child);
        arena.refresh_subtree_dirty_cache(root);

        arena.mark_dirty(child, DirtyFlags::COMPOSITE);
        assert!(
            arena
                .arena_local_dirty(child)
                .contains(DirtyFlags::COMPOSITE)
        );
        assert!(
            arena
                .cached_subtree_dirty(root)
                .contains(DirtyFlags::COMPOSITE)
        );
        assert!(
            !arena
                .cached_subtree_dirty(root)
                .intersects(DirtyFlags::PAINT)
        );

        arena.clear_arena_dirty(child, DirtyFlags::COMPOSITE);
        assert!(
            !arena
                .arena_local_dirty(child)
                .intersects(DirtyFlags::COMPOSITE)
        );
        assert!(
            !arena
                .cached_subtree_dirty(root)
                .intersects(DirtyFlags::COMPOSITE)
        );
    }

    #[test]
    fn element_background_color_with_invalidation_updates_value_and_paint_dirty() {
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(clean_element())));
        let child = arena.insert(Node::new(Box::new(clean_element())));
        let color = Color::rgba(12, 34, 56, 200);
        link_child(&mut arena, root, child);

        arena.refresh_subtree_dirty_cache(root);
        assert!(
            !arena
                .cached_subtree_dirty(root)
                .intersects(DirtyFlags::PAINT)
        );

        arena
            .mutate_element_with_invalidation(child, |element, cx| {
                element
                    .as_any_mut()
                    .downcast_mut::<Element>()
                    .expect("element")
                    .set_background_color_value_with_invalidation(color, cx);
            })
            .expect("child exists");

        let render_state = arena
            .get(child)
            .expect("child exists")
            .element
            .as_any()
            .downcast_ref::<Element>()
            .expect("element")
            .debug_render_state();
        assert_eq!(render_state.background_rgba, color.to_rgba_u8());
        assert_element_and_arena_paint_dirty(&arena, root, child);
    }

    #[test]
    fn element_foreground_color_with_invalidation_updates_value_and_paint_dirty() {
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(clean_element())));
        let child = arena.insert(Node::new(Box::new(clean_element())));
        let color = Color::rgb(90, 80, 70);
        link_child(&mut arena, root, child);

        arena.refresh_subtree_dirty_cache(root);
        assert!(
            !arena
                .cached_subtree_dirty(root)
                .intersects(DirtyFlags::PAINT)
        );

        arena
            .mutate_element_with_invalidation(child, |element, cx| {
                element
                    .as_any_mut()
                    .downcast_mut::<Element>()
                    .expect("element")
                    .set_foreground_color_with_invalidation(color, cx);
            })
            .expect("child exists");

        let render_state = arena
            .get(child)
            .expect("child exists")
            .element
            .as_any()
            .downcast_ref::<Element>()
            .expect("element")
            .debug_render_state();
        assert_eq!(render_state.foreground_rgba, color.to_rgba_u8());
        assert_element_and_arena_paint_dirty(&arena, root, child);
    }

    macro_rules! border_color_with_invalidation_test {
        ($name:ident, $setter:ident, $field:ident, $color:expr) => {
            #[test]
            fn $name() {
                let mut arena = NodeArena::new();
                let root = arena.insert(Node::new(Box::new(clean_element())));
                let child = arena.insert(Node::new(Box::new(clean_element())));
                let color = $color;
                link_child(&mut arena, root, child);

                arena.refresh_subtree_dirty_cache(root);
                assert!(
                    !arena
                        .cached_subtree_dirty(root)
                        .intersects(DirtyFlags::PAINT)
                );

                arena
                    .mutate_element_with_invalidation(child, |element, cx| {
                        element
                            .as_any_mut()
                            .downcast_mut::<Element>()
                            .expect("element")
                            .$setter(color, cx);
                    })
                    .expect("child exists");

                let render_state = arena
                    .get(child)
                    .expect("child exists")
                    .element
                    .as_any()
                    .downcast_ref::<Element>()
                    .expect("element")
                    .debug_render_state();
                assert_eq!(render_state.$field, color.to_rgba_u8());
                assert_element_and_arena_paint_dirty(&arena, root, child);
            }
        };
    }

    border_color_with_invalidation_test!(
        element_border_top_color_with_invalidation_updates_value_and_paint_dirty,
        set_border_top_color_with_invalidation,
        border_top_rgba,
        Color::rgba(11, 22, 33, 210)
    );
    border_color_with_invalidation_test!(
        element_border_right_color_with_invalidation_updates_value_and_paint_dirty,
        set_border_right_color_with_invalidation,
        border_right_rgba,
        Color::rgba(44, 55, 66, 220)
    );
    border_color_with_invalidation_test!(
        element_border_bottom_color_with_invalidation_updates_value_and_paint_dirty,
        set_border_bottom_color_with_invalidation,
        border_bottom_rgba,
        Color::rgba(77, 88, 99, 230)
    );
    border_color_with_invalidation_test!(
        element_border_left_color_with_invalidation_updates_value_and_paint_dirty,
        set_border_left_color_with_invalidation,
        border_left_rgba,
        Color::rgba(101, 112, 123, 240)
    );

    #[test]
    fn mutate_element_ref_with_invalidation_bubbles_invalidated_flags() {
        let mut arena = NodeArena::new();
        let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
        let child = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
        link_child(&mut arena, root, child);

        arena.refresh_subtree_dirty_cache(root);
        assert_eq!(arena.arena_local_dirty(child), DirtyFlags::NONE);
        assert!(
            !arena
                .cached_subtree_dirty(root)
                .intersects(DirtyFlags::HIT_TEST)
        );

        arena
            .mutate_element_ref_with_invalidation(child, |_element, cx| {
                cx.invalidate(DirtyFlags::HIT_TEST);
            })
            .expect("child exists");

        assert!(
            arena
                .arena_local_dirty(child)
                .contains(DirtyFlags::HIT_TEST)
        );
        assert!(
            arena
                .cached_subtree_dirty(child)
                .intersects(DirtyFlags::HIT_TEST)
        );
        assert!(
            arena
                .cached_subtree_dirty(root)
                .intersects(DirtyFlags::HIT_TEST)
        );
    }

    #[test]
    fn mutate_element_ref_with_invalidation_context_can_clear_arena_dirty() {
        let mut arena = NodeArena::new();
        let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
        let child = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
        link_child(&mut arena, root, child);

        arena.refresh_subtree_dirty_cache(root);
        assert_cached_paint_clean(&arena, child);
        assert_cached_paint_clean(&arena, root);

        arena
            .mutate_element_ref_with_invalidation(child, |_element, cx| {
                cx.invalidate(DirtyFlags::PAINT);
                cx.clear_arena_dirty(DirtyFlags::PAINT);
            })
            .expect("child exists");

        assert_eq!(arena.arena_local_dirty(child), DirtyFlags::NONE);
        assert_cached_paint_clean(&arena, child);
        assert_cached_paint_clean(&arena, root);
    }

    #[test]
    fn mutate_element_without_invalidation_keeps_arena_local_dirty_unchanged() {
        let mut arena = NodeArena::new();
        let node = insert_test_node(&mut arena, 1, DirtyFlags::NONE);

        let before = arena.arena_local_dirty(node);
        arena
            .mutate_element_with_invalidation(node, |element, _cx| {
                element
                    .as_any_mut()
                    .downcast_mut::<TestElement>()
                    .expect("test element")
                    .dirty_flags = DirtyFlags::PAINT;
            })
            .expect("node exists");

        assert_eq!(arena.arena_local_dirty(node), before);
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
    guard: NodeGuard<'a>,
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
