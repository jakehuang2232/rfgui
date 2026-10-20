//! Property observations of closed native storage. This certificate does not
//! certify paint payloads, external colors, resources or GPU residency.
use super::*;
use crate::view::base_component::{Element, Text};
use std::sync::{Arc, OnceLock};

pub(super) struct PruneProof {
    pub(super) seen: FxHashSet<NodeKey>,
    pub(super) stamp: PropertyStoreStamp,
}

#[derive(Default)]
pub(super) struct NativeSubtrees {
    arena: Option<Arc<()>>,
    roots: Vec<NodeKey>,
    entries: FxHashMap<NodeKey, Entry>,
}
/// One closed subtree. A stored owner's children were stored or replayed in
/// the same sync. An entry goes only when its owner is visited again or goes
/// unseen, and neither happens to a member while a stored ancestor replays,
/// so every member of a stored subtree has its own entry.
struct Entry {
    revision: u64,
    boundary: BoundaryInputs,
    inherited: PropertyTreeState,
    is_root: bool,
    children: Arc<[NodeKey]>,
    inactive_scroll_owner: bool,
    /// The whole subtree, flattened the first time it replays: a subtree
    /// that keeps replaying then costs one pass over a list per sync.
    members: OnceLock<Members>,
}
struct Members {
    keys: Box<[NodeKey]>,
    inactive_scroll_owners: Box<[NodeKey]>,
}
impl NativeSubtrees {
    pub(super) fn begin(&mut self, arena: &NodeArena, roots: &[NodeKey]) {
        let identity = arena.mutation_identity();
        if self
            .arena
            .as_ref()
            .is_none_or(|old| !Arc::ptr_eq(old, &identity))
            || self.roots != roots
        {
            self.entries.clear();
            self.arena = Some(identity);
            self.roots = roots.to_vec();
        }
    }
    /// Marks every member of `key`'s unchanged subtree seen and restores its
    /// inactive scroll owners. Returns how many members it covered.
    pub(super) fn replay(
        &self,
        key: NodeKey,
        revision: Option<u64>,
        boundary: BoundaryInputs,
        inherited: PropertyTreeState,
        is_root: bool,
        seen: &mut FxHashSet<NodeKey>,
        inactive_scroll_owners: &mut FxHashSet<NodeKey>,
    ) -> Option<usize> {
        let old = self.entries.get(&key)?;
        let matches = revision == Some(old.revision)
            && boundary == old.boundary
            && inherited == old.inherited
            && is_root == old.is_root;
        if !matches {
            return None;
        }
        let members = old.members.get_or_init(|| self.members(key));
        seen.extend(members.keys.iter().copied());
        inactive_scroll_owners.extend(members.inactive_scroll_owners.iter().copied());
        Some(members.keys.len())
    }
    fn members(&self, key: NodeKey) -> Members {
        let mut keys = Vec::new();
        let mut inactive_scroll_owners = Vec::new();
        let mut stack = vec![key];
        while let Some(member) = stack.pop() {
            keys.push(member);
            let Some(entry) = self.entries.get(&member) else {
                continue;
            };
            if entry.inactive_scroll_owner {
                inactive_scroll_owners.push(member);
            }
            stack.extend(entry.children.iter().copied());
        }
        Members {
            keys: keys.into(),
            inactive_scroll_owners: inactive_scroll_owners.into(),
        }
    }
    pub(super) fn store(
        &mut self,
        key: NodeKey,
        revision: u64,
        boundary: BoundaryInputs,
        inherited: PropertyTreeState,
        is_root: bool,
        children: Arc<[NodeKey]>,
        inactive_scroll_owner: bool,
    ) {
        self.entries.insert(
            key,
            Entry {
                revision,
                boundary,
                inherited,
                is_root,
                children,
                inactive_scroll_owner,
                members: OnceLock::new(),
            },
        );
    }
    pub(super) fn remove(&mut self, key: NodeKey) {
        self.entries.remove(&key);
    }
    pub(super) fn finish(&mut self, seen: &FxHashSet<NodeKey>, valid: bool) {
        self.entries.retain(|key, _| valid && seen.contains(key));
    }
}

pub(super) fn native_inputs_are_tracked(
    arena: &NodeArena,
    key: NodeKey,
    node: &crate::view::node_arena::NodeGuard<'_>,
) -> bool {
    let native = node.element.as_any();
    if !(native.is::<Element>() || native.is::<Text>())
        || arena.mutation_revision(key).is_none()
        || node.children() != node.element.children()
        || node
            .children()
            .iter()
            .any(|child| arena.parent_of(*child) != Some(key))
    {
        return false;
    }
    // Stable-ID lookups introduce dependencies outside the ancestor/subtree certificate.
    // Their original live observation remains authoritative.
    !node
        .element
        .compositor_spatial_placement_snapshot()
        .is_some_and(|snapshot| {
            matches!(
                snapshot.reference(),
                SpatialPositionReferenceSnapshot::Anchor(_)
                    | SpatialPositionReferenceSnapshot::LayoutParent(Some(_))
            )
        })
}

/// Inputs read outside a closed native subtree. Internal parent/sibling reads
/// are covered by its subtree revision; inherited property IDs are separate.
/// Numeric ancestor transforms/effects are derived from current trees later.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct BoundaryInputs {
    parent: Option<NodeKey>,
    reference_scroll: Option<ScrollNodeId>,
    visual_parent: Option<VisualOffsetNodeId>,
    pub(super) self_clip: Option<ClipGeometry>,
}
impl BoundaryInputs {
    pub(super) fn observe(
        trees: &PropertyTrees,
        arena: &NodeArena,
        key: NodeKey,
        node: &crate::view::node_arena::NodeGuard<'_>,
        is_frame_root: bool,
    ) -> Option<Self> {
        let native = node.element.as_any();
        if !(native.is::<Element>() || native.is::<Text>()) {
            return None;
        }
        let parent = arena.parent_of(key);
        Some(Self {
            parent,
            reference_scroll: parent.and_then(|parent| trees.spatial_parent_scroll(arena, parent)),
            visual_parent: trees.spatial_visual_parent(arena, key),
            // AnchorParent clipping also reads the parent's complete child phase
            // order, including siblings outside this subtree. Observe its exact
            // current result even when neither this node nor its parent changed.
            self_clip: node
                .element
                .exact_retained_self_clip_geometry(key, arena, is_frame_root)
                .or_else(|| {
                    node.element
                        .exact_generic_subtree_self_clip_geometry(key, arena, is_frame_root)
                }),
        })
    }
}
