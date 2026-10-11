//! Arena positions for the structural patches of one translated batch.
//!
//! The reconciler addresses children by rsx index, and the removals, moves
//! and insertions it emits for one child list each assume the earlier ones
//! applied. A fragment owns no arena node: its children sit in its parent's
//! arena child list, an empty fragment's in none. The tracker keeps, for
//! every child list a batch edits, which run of its arena parent's children
//! each rsx child holds, so every patch lands at the arena index it means.
//!
//! A list is read from the arena on first use, before any work applies.
//! Patches for a fragment's own children come before those of the list
//! holding it, so that list is still in its old order while they apply.

use super::{
    ArenaPathResolution, DescriptorContext, FiberWork, NodeArena, NodeKey, Patch, RsxNode,
    inserted_child_descriptors, replacement_descriptors, resolve_path, rsx_to_arena_path,
    walk_rsx_by_index_path,
};
use rustc_hash::FxHashMap;

/// One root's trees and arena handle for the patches being translated.
pub(super) struct RootScope<'a, 'c> {
    pub(super) arena: &'a NodeArena,
    pub(super) root: NodeKey,
    pub(super) ctx: Option<&'a DescriptorContext<'c>>,
    pub(super) old_rsx: &'a RsxNode,
    pub(super) new_rsx: Option<&'a RsxNode>,
    pub(super) new_root_index: Option<usize>,
}

/// The arena children one rsx child holds.
enum Slot {
    /// A child present before the batch.
    Old(NodeKey),
    /// Children this batch creates.
    New(usize),
    /// A fragment, whose own list is keyed by its old rsx path.
    Fragment(Vec<usize>),
}

struct ChildList {
    arena_parent: NodeKey,
    slots: Vec<Slot>,
}

#[derive(Default)]
pub(super) struct ChildSlotTracker {
    lists: FxHashMap<(NodeKey, Vec<usize>), ChildList>,
}

impl ChildSlotTracker {
    /// The works for a structural patch, at the arena indices it means.
    pub(super) fn translate(
        &mut self,
        patch: &Patch,
        scope: &RootScope<'_, '_>,
    ) -> Option<Vec<FiberWork>> {
        match patch {
            Patch::RemoveChild { parent_path, index } => {
                let list_key = self.list(scope, parent_path)?;
                let list = self.lists.get_mut(&list_key)?;
                let parent = list.arena_parent;
                if *index >= list.slots.len() {
                    return None;
                }
                let slot = list.slots.remove(*index);
                let keys = self.old_keys(scope.root, &slot)?;
                Some(
                    keys.into_iter()
                        .map(|key| FiberWork::Delete {
                            parent: Some(parent),
                            key,
                        })
                        .collect(),
                )
            }
            Patch::MoveChild {
                parent_path,
                from,
                to,
            } => {
                let list_key = self.list(scope, parent_path)?;
                let base = self.offset(scope, parent_path)?;
                let list = self.lists.get(&list_key)?;
                let parent = list.arena_parent;
                let keys = self.old_keys(scope.root, list.slots.get(*from)?)?;
                let start = base + self.leaves_of(scope.root, &list.slots[..*from]);
                let slot = self.lists.get_mut(&list_key)?.slots.remove(*from);
                let list = self.lists.get(&list_key)?;
                let target = base + self.leaves_of(scope.root, list.slots.get(..*to)?);
                self.lists.get_mut(&list_key)?.slots.insert(*to, slot);
                // Move the run one child at a time: forward from its far
                // end, backward from its near end, so each move finds the
                // rest of the run where it was.
                let moves = keys
                    .into_iter()
                    .enumerate()
                    .map(|(offset, key)| FiberWork::Move {
                        parent,
                        key,
                        from: start + offset,
                        to: target + offset,
                    });
                Some(if target > start {
                    moves.rev().collect()
                } else {
                    moves.collect()
                })
            }
            Patch::InsertChild {
                parent_path, index, ..
            } => {
                let list_key = self.list(scope, parent_path)?;
                let (parent, mut descriptors) = inserted_child_descriptors(
                    parent_path,
                    *index,
                    scope.arena,
                    scope.root,
                    scope.ctx,
                    Some(scope.old_rsx),
                    scope.new_rsx,
                    scope.new_root_index,
                )?;
                let base = self.offset(scope, parent_path)?;
                let list = self.lists.get(&list_key)?;
                if parent != list.arena_parent {
                    return None;
                }
                let at = base + self.leaves_of(scope.root, list.slots.get(..*index)?);
                self.lists
                    .get_mut(&list_key)?
                    .slots
                    .insert(*index, Slot::New(descriptors.len()));
                Some(match descriptors.len() {
                    0 => Vec::new(),
                    1 => {
                        let descriptor = descriptors.pop()?;
                        let stable_id = descriptor.element.stable_id();
                        vec![FiberWork::Create {
                            parent: Some(parent),
                            index: at,
                            descriptor,
                            stable_id,
                        }]
                    }
                    _ => vec![FiberWork::CreateMany {
                        parent,
                        index_start: at,
                        descriptors,
                    }],
                })
            }
            Patch::ReplaceNode { path, node } => {
                let (&index, parent_path) = path.split_last()?;
                let list_key = self.list(scope, parent_path)?;
                let (parent, descriptors) = replacement_descriptors(
                    path,
                    node,
                    scope.arena,
                    scope.root,
                    scope.ctx,
                    Some(scope.old_rsx),
                    scope.new_root_index,
                )?;
                let base = self.offset(scope, parent_path)?;
                let list = self.lists.get(&list_key)?;
                if parent != list.arena_parent {
                    return None;
                }
                let at = base + self.leaves_of(scope.root, &list.slots[..index]);
                let old = self.old_keys(scope.root, list.slots.get(index)?)?;
                self.lists.get_mut(&list_key)?.slots[index] = Slot::New(descriptors.len());
                // A single old child is replaced in place; a fragment's run
                // is deleted and the replacement created at its start.
                if old.len() == 1 && !descriptors.is_empty() {
                    return Some(vec![FiberWork::ReplaceNode {
                        parent,
                        index: at,
                        descriptors,
                    }]);
                }
                let mut works = old
                    .into_iter()
                    .map(|key| FiberWork::Delete {
                        parent: Some(parent),
                        key,
                    })
                    .collect::<Vec<_>>();
                if !descriptors.is_empty() {
                    works.push(FiberWork::CreateMany {
                        parent,
                        index_start: at,
                        descriptors,
                    });
                }
                Some(works)
            }
            _ => None,
        }
    }

    /// The key of the list at `path`, read from the arena on first use.
    fn list(&mut self, scope: &RootScope<'_, '_>, path: &[usize]) -> Option<(NodeKey, Vec<usize>)> {
        let list_key = (scope.root, path.to_vec());
        if self.lists.contains_key(&list_key) {
            return Some(list_key);
        }
        // The deepest element on the path owns the arena children of every
        // fragment list below it; read them all at once.
        let mut node = scope.old_rsx;
        let mut owner_len = 0;
        for (depth, &index) in path.iter().enumerate() {
            node = node.children()?.get(index)?;
            if !matches!(node, RsxNode::Fragment(_)) {
                owner_len = depth + 1;
            }
        }
        let owner_path = &path[..owner_len];
        if !self.lists.contains_key(&(scope.root, owner_path.to_vec())) {
            let owner = walk_rsx_by_index_path(scope.old_rsx, owner_path)?;
            let ArenaPathResolution::Arena(arena_path) =
                rsx_to_arena_path(scope.old_rsx, owner_path)
            else {
                return None;
            };
            let parent = resolve_path(scope.arena, scope.root, &arena_path)?;
            let keys = scope.arena.children_of(parent);
            let mut lists = FxHashMap::default();
            let mut next = 0;
            read_list(
                &mut lists,
                scope.root,
                parent,
                owner,
                owner_path.to_vec(),
                &keys,
                &mut next,
            )?;
            // A child list that does not cover its arena children exactly
            // (a host absorbing its text children) has no slot image.
            if next != keys.len() {
                return None;
            }
            self.lists.extend(lists);
        }
        self.lists.contains_key(&list_key).then_some(list_key)
    }

    /// The arena index, within its arena parent, of the list's first child.
    fn offset(&self, scope: &RootScope<'_, '_>, path: &[usize]) -> Option<usize> {
        let node = walk_rsx_by_index_path(scope.old_rsx, path)?;
        if !matches!(node, RsxNode::Fragment(_)) {
            return Some(0);
        }
        let (&index, parent_path) = path.split_last()?;
        let parent = self.lists.get(&(scope.root, parent_path.to_vec()))?;
        Some(
            self.offset(scope, parent_path)?
                + self.leaves_of(scope.root, parent.slots.get(..index)?),
        )
    }

    fn leaves_of(&self, root: NodeKey, slots: &[Slot]) -> usize {
        slots.iter().map(|slot| self.leaves(root, slot)).sum()
    }

    fn leaves(&self, root: NodeKey, slot: &Slot) -> usize {
        match slot {
            Slot::Old(_) => 1,
            Slot::New(count) => *count,
            Slot::Fragment(path) => self
                .lists
                .get(&(root, path.clone()))
                .map_or(0, |list| self.leaves_of(root, &list.slots)),
        }
    }

    /// The arena keys a slot holds, when all of them predate the batch.
    fn old_keys(&self, root: NodeKey, slot: &Slot) -> Option<Vec<NodeKey>> {
        match slot {
            Slot::Old(key) => Some(vec![*key]),
            Slot::New(0) => Some(Vec::new()),
            Slot::New(_) => None,
            Slot::Fragment(path) => {
                let list = self.lists.get(&(root, path.clone()))?;
                let mut keys = Vec::new();
                for slot in &list.slots {
                    keys.extend(self.old_keys(root, slot)?);
                }
                Some(keys)
            }
        }
    }
}

/// Read the list of `node`'s children, and every fragment list below it,
/// from `keys`, the arena children they share starting at `next`.
fn read_list(
    lists: &mut FxHashMap<(NodeKey, Vec<usize>), ChildList>,
    root: NodeKey,
    arena_parent: NodeKey,
    node: &RsxNode,
    path: Vec<usize>,
    keys: &[NodeKey],
    next: &mut usize,
) -> Option<()> {
    let mut slots = Vec::new();
    for (index, child) in node.children()?.iter().enumerate() {
        if matches!(child, RsxNode::Fragment(_)) {
            let mut child_path = path.clone();
            child_path.push(index);
            read_list(
                lists,
                root,
                arena_parent,
                child,
                child_path.clone(),
                keys,
                next,
            )?;
            slots.push(Slot::Fragment(child_path));
        } else {
            slots.push(Slot::Old(*keys.get(*next)?));
            *next += 1;
        }
    }
    lists.insert(
        (root, path),
        ChildList {
            arena_parent,
            slots,
        },
    );
    Some(())
}
