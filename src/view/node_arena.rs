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

use rustc_hash::FxHashMap;
use slotmap::SlotMap;
use std::cell::{Ref, RefCell, RefMut};

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
    pub element: Box<dyn ElementTrait>,
    pub parent: Option<NodeKey>,
    pub children: Vec<NodeKey>,
    /// Arena-owned local dirty bits for the node itself.
    ///
    /// This is scaffold for migrating invalidation ownership out of
    /// elements. The formal layout pass still reads
    /// `element.local_dirty_flags()` through
    /// [`NodeArena::refresh_subtree_dirty_cache`]; this field is only
    /// updated by the new arena invalidation APIs for now.
    pub arena_local_dirty: DirtyFlags,
    /// Aggregate of `element.local_dirty_flags()` unioned with every
    /// descendant's flags, refreshed once per layout pass by
    /// [`NodeArena::refresh_subtree_dirty_cache`]. Lets the layout hot loops
    /// short-circuit what used to be an O(N²) subtree walk into an O(1)
    /// field read.
    ///
    /// Default is `DirtyFlags::ALL` so newly inserted nodes are always seen
    /// as dirty until the next pre-pass runs.
    pub cached_subtree_dirty: crate::view::base_component::DirtyFlags,
    /// Aggregate placement-replay eligibility metadata for this subtree.
    ///
    /// This is Phase 5b scaffold. It is refreshed with the existing
    /// subtree-dirty pre-pass so observation can avoid recursively scanning
    /// candidate subtrees, but it is not a standalone skip truth: callers
    /// must still check dirty bits, placement keys, clip/anchor context, and
    /// runtime state guards where relevant.
    pub(crate) cached_placement_eligibility: PlacementEligibilityMetadata,
}

impl Node {
    pub fn new(element: Box<dyn ElementTrait>) -> Self {
        Self {
            element,
            parent: None,
            children: Vec::new(),
            arena_local_dirty: DirtyFlags::NONE,
            cached_subtree_dirty: crate::view::base_component::DirtyFlags::ALL,
            cached_placement_eligibility: PlacementEligibilityMetadata::unknown(),
        }
    }

    pub fn with_parent(element: Box<dyn ElementTrait>, parent: Option<NodeKey>) -> Self {
        Self {
            element,
            parent,
            children: Vec::new(),
            arena_local_dirty: DirtyFlags::NONE,
            cached_subtree_dirty: crate::view::base_component::DirtyFlags::ALL,
            cached_placement_eligibility: PlacementEligibilityMetadata::unknown(),
        }
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
    slots: SlotMap<NodeKey, RefCell<Node>>,
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
    pub fn insert(&mut self, node: Node) -> NodeKey {
        let sid = node.element.stable_id();
        let key = self.slots.insert(RefCell::new(node));
        if sid != 0 {
            self.stable_id_index.insert(sid, key);
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
        let key = self.slots.insert_with_key(|k| RefCell::new(f(k)));
        if let Some(cell) = self.slots.get(key) {
            let sid = cell.borrow().element.stable_id();
            if sid != 0 {
                self.stable_id_index.insert(sid, key);
            }
        }
        key
    }

    /// Remove an element and its `Node` wrapper. Does **not** cascade to
    /// children — callers must walk the subtree (use
    /// [`Self::remove_subtree`] for recursive removal).
    pub fn remove(&mut self, key: NodeKey) -> Option<Node> {
        let node = self.slots.remove(key).map(RefCell::into_inner)?;
        let sid = node.element.stable_id();
        if sid != 0 {
            // Only clear if the index still points at `key` — a prior
            // `refresh_stable_id_index` or id collision may have remapped it.
            if self.stable_id_index.get(&sid).copied() == Some(key) {
                self.stable_id_index.remove(&sid);
            }
        }
        Some(node)
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
            if let Some(cell) = self.slots.remove(k) {
                let sid = cell.borrow().element.stable_id();
                if sid != 0 && self.stable_id_index.get(&sid).copied() == Some(k) {
                    self.stable_id_index.remove(&sid);
                }
                removed += 1;
            }
        }
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
        self.slots.get(key).map(|_| key)
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
        for (key, cell) in self.slots.iter() {
            let sid = cell.borrow().element.stable_id();
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
            let children: Vec<NodeKey> = self
                .slots
                .get(root_key)
                .map(|cell| cell.borrow().children.clone())
                .unwrap_or_default();
            for child_key in children {
                self.collect_viewport_clip_nodes_subtree(child_key, &mut out);
            }
        }
        out
    }

    fn collect_viewport_clip_nodes_subtree(
        &self,
        key: NodeKey,
        out: &mut Vec<crate::view::base_component::DeferredRenderNode>,
    ) {
        let Some(cell) = self.slots.get(key) else {
            return;
        };
        let node = cell.borrow();
        let children: Vec<NodeKey> = node.children.clone();
        if let Some(element) = node
            .element
            .as_any()
            .downcast_ref::<crate::view::base_component::Element>()
        {
            if element.should_append_to_root_viewport_render() {
                let sid = element.stable_id();
                if sid != 0 {
                    out.push(crate::view::base_component::DeferredRenderNode {
                        key,
                        stable_id: sid,
                    });
                }
            }
        }
        drop(node);
        for child_key in children {
            self.collect_viewport_clip_nodes_subtree(child_key, out);
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
            ctx.append_to_defer(node.key, node.stable_id);
        }
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
        self.slots
            .get(key)
            .and_then(|cell| cell.try_borrow_mut().ok())
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

    /// Allocation-free child access for hot whole-tree walks: one slot
    /// borrow per lookup instead of cloning the child Vec per node.
    pub fn child_key_at(&self, key: NodeKey, index: usize) -> Option<NodeKey> {
        self.slots
            .get(key)
            .and_then(|cell| cell.borrow().children.get(index).copied())
    }

    pub fn parent_of(&self, key: NodeKey) -> Option<NodeKey> {
        self.slots.get(key).and_then(|cell| cell.borrow().parent)
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

    /// Pre-order walk rooted at `key` that calls `sync_arena` on every
    /// element. Traversal is external (not recursive inside each element's
    /// impl) so each element's `sync_arena` only handles its own state.
    /// Children are re-read from the arena after each call so elements that
    /// mutate the arena during sync (e.g. TextArea projection rebuild) still
    /// get their freshly committed subtree walked.
    pub fn sync_subtree(&mut self, key: NodeKey) {
        self.with_element_taken(key, |el, arena| el.sync_arena(arena));
        for child in self.children_of(key) {
            self.sync_subtree(child);
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
        let Some(cell) = self.slots.get(key) else {
            return DirtyFlags::NONE;
        };
        // Clone child list without holding a borrow into the cell, so we
        // can recurse and then re-borrow to write the caches.
        let (children, mut aggregate, mut placement_eligibility) = {
            let node = cell.borrow();
            (
                node.children.clone(),
                node.element
                    .local_dirty_flags()
                    .union(node.arena_local_dirty),
                Self::local_placement_eligibility_metadata(&node),
            )
        };
        for child in children {
            aggregate = aggregate.union(self.refresh_subtree_dirty_cache(child));
            placement_eligibility =
                placement_eligibility.union(self.cached_placement_eligibility_metadata(child));
        }
        {
            let mut node = cell.borrow_mut();
            node.cached_subtree_dirty = aggregate;
            node.cached_placement_eligibility = placement_eligibility;
        }
        aggregate
    }

    fn local_placement_eligibility_metadata(node: &Node) -> PlacementEligibilityMetadata {
        // Each node declares the blockers it personally contributes; leaves
        // (Text/Image/Svg) and pass-through wrappers default to transparent
        // so a clean stationary subtree containing them stays skippable.
        // Descendant blockers are unioned separately by the subtree walk.
        node.element.placement_eligibility_metadata()
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
            .map(|cell| cell.borrow().cached_placement_eligibility)
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

        let Some(cell) = self.slots.get(key) else {
            return;
        };
        {
            let mut node = cell.borrow_mut();
            node.arena_local_dirty = node.arena_local_dirty.union(flags);
        }
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
            let Some(cell) = self.slots.get(node_key) else {
                return;
            };
            let mut node = cell.borrow_mut();
            node.cached_subtree_dirty = node.cached_subtree_dirty.union(flags);
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
            let Some(cell) = self.slots.get(node_key) else {
                return last;
            };

            let (child_count, parent, previous, mut aggregate) = {
                let node = cell.borrow();
                (
                    node.children.len(),
                    node.parent,
                    node.cached_subtree_dirty,
                    node.element
                        .local_dirty_flags()
                        .union(node.arena_local_dirty),
                )
            };

            for index in 0..child_count {
                let Some(child) = self.child_key_at(node_key, index) else {
                    break;
                };
                aggregate = aggregate.union(self.cached_subtree_dirty(child));
            }

            cell.borrow_mut().cached_subtree_dirty = aggregate;
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

        let Some(cell) = self.slots.get(key) else {
            return;
        };
        {
            let mut node = cell.borrow_mut();
            node.arena_local_dirty = node.arena_local_dirty.without(flags);
        }
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
        self.collect_subtree_keys(key, &mut subtree);
        if subtree.is_empty() {
            return;
        }

        for node_key in &subtree {
            if let Some(cell) = self.slots.get(*node_key) {
                let mut node = cell.borrow_mut();
                node.arena_local_dirty = node.arena_local_dirty.without(flags);
            }
        }

        for node_key in &subtree {
            let Some(cell) = self.slots.get(*node_key) else {
                continue;
            };
            let (children, mut aggregate) = {
                let node = cell.borrow();
                (
                    node.children.clone(),
                    node.element
                        .local_dirty_flags()
                        .union(node.arena_local_dirty),
                )
            };

            for child in children {
                aggregate = aggregate.union(self.cached_subtree_dirty(child));
            }

            cell.borrow_mut().cached_subtree_dirty = aggregate;
        }

        // The post-order loop above already wrote fresh aggregates for the
        // whole subtree (including `key`), so the ancestor climb must start
        // at the parent: repair's no-change early-out would otherwise see
        // `key` unchanged and stop before reaching stale ancestors.
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
            .map(|cell| cell.borrow().cached_subtree_dirty)
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
            .map(|cell| cell.borrow().cached_subtree_dirty.intersects(flags))
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
            .map(|cell| cell.borrow().cached_subtree_dirty.contains(flags))
            .unwrap_or(false)
    }

    /// Fast read of the arena-owned local dirty bits for `key`.
    pub fn arena_local_dirty(&self, key: NodeKey) -> DirtyFlags {
        self.slots
            .get(key)
            .map(|cell| cell.borrow().arena_local_dirty)
            .unwrap_or(DirtyFlags::NONE)
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
        let slot_ref = self.slots.get(key)?;
        let taken: Box<dyn ElementTrait> = {
            let mut node = slot_ref.borrow_mut();
            std::mem::replace(&mut node.element, Box::new(Placeholder))
        };

        struct Guard<'a> {
            arena: &'a NodeArena,
            key: NodeKey,
            taken: Option<Box<dyn ElementTrait>>,
        }
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                if let Some(element) = self.taken.take() {
                    if let Some(cell) = self.arena.slots.get(self.key) {
                        cell.borrow_mut().element = element;
                    }
                }
            }
        }

        let mut guard = Guard {
            arena: self,
            key,
            taken: Some(taken),
        };

        let mut element = guard.taken.take().expect("guard initialised with element");
        let result = f(&mut element, guard.arena);
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
            Self { stable_id, label }
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

    fn link_child(arena: &NodeArena, parent: NodeKey, child: NodeKey) {
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
    fn deferred_build_uses_node_key_when_stable_ids_collide() {
        RECORDED_BUILDS.with(|builds| builds.borrow_mut().clear());

        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(RecordingElement::new(1, "root"))));
        let first = arena.insert(Node::new(Box::new(RecordingElement::new(42, "first"))));
        let second = arena.insert(Node::new(Box::new(RecordingElement::new(42, "second"))));
        link_child(&arena, root, first);
        link_child(&arena, root, second);

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(400, 300, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        ctx.append_to_defer(second, 42);
        let deferred = ctx.take_deferred_nodes();
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
        link_child(&arena, root, child);

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
        link_child(&arena, root, child);

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
        link_child(&arena, root, child);

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
        link_child(&arena, root, child);
        link_child(&arena, child, grandchild);

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
        link_child(&arena, root, dirty_sibling);
        link_child(&arena, root, clean_sibling);

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
        link_child(&arena, root, child);
        link_child(&arena, child, grandchild);

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
        link_child(&arena, root, child);
        link_child(&arena, child, grandchild);

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
        link_child(&arena, root, child);
        link_child(&arena, child, grandchild);

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
        link_child(&arena, root, child);

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
        link_child(&arena, root, left);
        link_child(&arena, root, right);

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
        link_child(&arena, root, child);
        link_child(&arena, child, grandchild);

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
        link_child(&arena, root, child);
        link_child(&arena, child, grandchild);

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
        link_child(&arena, root, child);
        link_child(&arena, child, grandchild);

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
    fn clear_arena_dirty_subtree_keeps_paint_from_element_local_dirty() {
        let mut arena = NodeArena::new();
        let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
        let child = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
        let grandchild = insert_test_node(&mut arena, 3, DirtyFlags::PAINT);
        link_child(&arena, root, child);
        link_child(&arena, child, grandchild);

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
        link_child(&arena, root, left);
        link_child(&arena, left, left_child);
        link_child(&arena, root, right);

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
        link_child(&arena, root, child);

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
        link_child(&arena, root, child);

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
        link_child(&arena, root, child);

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

    #[test]
    fn element_background_color_with_invalidation_updates_value_and_paint_dirty() {
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(clean_element())));
        let child = arena.insert(Node::new(Box::new(clean_element())));
        let color = Color::rgba(12, 34, 56, 200);
        link_child(&arena, root, child);

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
        link_child(&arena, root, child);

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
                link_child(&arena, root, child);

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
        link_child(&arena, root, child);

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
        link_child(&arena, root, child);

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
        link_child(&arena, root, child);

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
