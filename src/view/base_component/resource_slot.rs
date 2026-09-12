use rustc_hash::FxHashSet;

use super::{DirtyFlags, Element, ElementTrait};
use crate::view::node_arena::{NodeArena, NodeKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ActiveSlot {
    None,
    Loading,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SlotReplacementError {
    MissingOwner,
    DuplicateRoot(NodeKey),
    MissingRoot(NodeKey),
    WrongParent {
        root: NodeKey,
        actual: Option<NodeKey>,
    },
    AliasedRoot(NodeKey),
    ChildrenMirrorMismatch,
    UnexpectedActiveChildren,
}

pub(super) fn attach_slot_cold(
    active_slot: ActiveSlot,
    target: &mut Vec<NodeKey>,
    roots: Vec<NodeKey>,
) {
    assert_eq!(
        active_slot,
        ActiveSlot::None,
        "cold slot attachment requires an inactive host"
    );
    assert!(
        target.is_empty(),
        "cold slot attachment must not replace an existing slot"
    );
    *target = roots;
}

pub(super) fn sync_active_slot(
    arena: &mut NodeArena,
    owner: Option<NodeKey>,
    element: &mut Element,
    loading_slot: &mut Vec<NodeKey>,
    error_slot: &mut Vec<NodeKey>,
    active_slot: &mut ActiveSlot,
    next_slot: ActiveSlot,
) {
    if *active_slot == next_slot {
        return;
    }

    let next_children = match next_slot {
        ActiveSlot::None => Vec::new(),
        ActiveSlot::Loading => std::mem::take(loading_slot),
        ActiveSlot::Error => std::mem::take(error_slot),
    };
    let previous_children = element.replace_children(arena, next_children);
    if let Some(owner) = owner {
        arena.set_children(owner, element.children().to_vec());
    }
    match *active_slot {
        ActiveSlot::None => {}
        ActiveSlot::Loading => *loading_slot = previous_children,
        ActiveSlot::Error => *error_slot = previous_children,
    }
    *active_slot = next_slot;
}

pub(super) fn replace_slot(
    arena: &mut NodeArena,
    owner: NodeKey,
    element: &mut Element,
    loading_slot: &mut Vec<NodeKey>,
    error_slot: &mut Vec<NodeKey>,
    active_slot: &mut ActiveSlot,
    target_slot: ActiveSlot,
    new_roots: &[NodeKey],
) -> Result<(), SlotReplacementError> {
    // All fallible validation precedes topology mutation. Callers may safely
    // retain the current slot tree when this returns `Err`.
    if !arena.contains_key(owner) {
        return Err(SlotReplacementError::MissingOwner);
    }

    let arena_children = arena.children_of(owner);
    if arena_children != element.children() {
        return Err(SlotReplacementError::ChildrenMirrorMismatch);
    }
    if *active_slot == ActiveSlot::None && !arena_children.is_empty() {
        return Err(SlotReplacementError::UnexpectedActiveChildren);
    }

    let (target_roots, other_roots) = match target_slot {
        ActiveSlot::Loading => (
            if *active_slot == ActiveSlot::Loading {
                element.children()
            } else {
                loading_slot.as_slice()
            },
            if *active_slot == ActiveSlot::Error {
                element.children()
            } else {
                error_slot.as_slice()
            },
        ),
        ActiveSlot::Error => (
            if *active_slot == ActiveSlot::Error {
                element.children()
            } else {
                error_slot.as_slice()
            },
            if *active_slot == ActiveSlot::Loading {
                element.children()
            } else {
                loading_slot.as_slice()
            },
        ),
        ActiveSlot::None => unreachable!("None is not a replaceable resource slot"),
    };

    let exact_noop = new_roots == target_roots;
    let target_roots: FxHashSet<NodeKey> = target_roots.iter().copied().collect();
    let other_roots: FxHashSet<NodeKey> = other_roots.iter().copied().collect();
    let mut unique_roots = FxHashSet::default();
    for &root in new_roots {
        if !unique_roots.insert(root) {
            return Err(SlotReplacementError::DuplicateRoot(root));
        }
        if !arena.contains_key(root) {
            return Err(SlotReplacementError::MissingRoot(root));
        }
        let actual_parent = arena.parent_of(root);
        if actual_parent != Some(owner) {
            return Err(SlotReplacementError::WrongParent {
                root,
                actual: actual_parent,
            });
        }
        if (!exact_noop && target_roots.contains(&root)) || other_roots.contains(&root) {
            return Err(SlotReplacementError::AliasedRoot(root));
        }
    }
    if exact_noop {
        return Ok(());
    }

    // Topology changes only here or in the pre-layout sync above. Resource
    // preparation after layout has no arena access and cannot enter this path.
    sync_active_slot(
        arena,
        Some(owner),
        element,
        loading_slot,
        error_slot,
        active_slot,
        ActiveSlot::None,
    );

    let target = match target_slot {
        ActiveSlot::Loading => loading_slot,
        ActiveSlot::Error => error_slot,
        ActiveSlot::None => unreachable!("None is not a replaceable resource slot"),
    };
    let old_roots = std::mem::take(target);
    for old_root in old_roots {
        arena.remove_subtree(old_root);
    }
    *target = new_roots.to_vec();
    element.mark_layout_dirty();
    arena.mark_dirty(owner, DirtyFlags::ALL);
    Ok(())
}

#[cfg(test)]
mod tests;
