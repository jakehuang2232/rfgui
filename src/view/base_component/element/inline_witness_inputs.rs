//! Dependencies of a successful native IFC install validation. Mutation
//! revisions cover private node storage even when callers clear dirty flags.
//! External ColorLike values are polled on every observation. Resource/custom
//! hosts do not participate: their preparation/getters need separate contracts.
//! This caches an install proof, never the owner's paint capability or commands.
use super::*;
use std::sync::Arc;

pub(super) struct NativeInlineWitnessInputs {
    arena: Arc<()>,
    owners: Vec<(NodeKey, u64, VolatileInput)>,
}

#[derive(PartialEq)]
enum VolatileInput {
    Text([u8; 4]),
    Element([[u32; 4]; 5]),
}

impl NativeInlineWitnessInputs {
    pub(super) fn observe(root: &Element, arena: &NodeArena) -> Option<Self> {
        if !root.is_owning_inline_ifc_root_role() {
            return None;
        }
        let clock = arena.mutation_clock();
        if clock == u64::MAX {
            return None;
        }
        let root_key = arena.find_by_stable_id(root.stable_id())?;
        let mut pending = vec![root_key];
        let mut owners = Vec::new();
        let mut seen = FxHashSet::default();
        while let Some(key) = pending.pop() {
            if !seen.insert(key) {
                return None;
            }
            let node = arena.get(key)?;
            let element = node.element.as_ref();
            if node.children() != element.children()
                || element
                    .local_dirty_flags()
                    .union(arena.arena_local_dirty(key))
                    .intersects(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT))
            {
                return None;
            }
            let volatile = if let Some(native) = element.as_any().downcast_ref::<Element>() {
                if key == root_key && !std::ptr::eq(root, native) {
                    return None;
                }
                let colors = [
                    native.background_color.as_ref(),
                    native.border_colors.left.as_ref(),
                    native.border_colors.right.as_ref(),
                    native.border_colors.top.as_ref(),
                    native.border_colors.bottom.as_ref(),
                ]
                .map(ColorLike::to_rgba_f32);
                if colors.iter().flatten().any(|value| !value.is_finite()) {
                    return None;
                }
                VolatileInput::Element(colors.map(|rgba| rgba.map(f32::to_bits)))
            } else if let Some(text) = element.as_any().downcast_ref::<Text>() {
                VolatileInput::Text(text.color.to_rgba_u8())
            } else {
                return None;
            };
            owners.push((key, arena.mutation_revision(key)?, volatile));
            pending.extend_from_slice(node.children());
        }
        // A ColorLike implementation can call back into the arena. A mutation
        // while collecting dependencies must not publish a mixed-time proof.
        if arena.mutation_clock() != clock {
            return None;
        }
        Some(Self {
            arena: arena.mutation_identity(),
            owners,
        })
    }

    pub(super) fn same_inputs(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.arena, &other.arena) && self.owners == other.owners
    }
}
