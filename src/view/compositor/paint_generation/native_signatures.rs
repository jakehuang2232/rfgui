//! Memoizes only the existing local signature computation. This is neither
//! complete command identity nor a resource-residency or renderer admission key.
use super::*;
use crate::view::base_component::{Element, Text};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct NativeSignatures {
    arena: Option<Arc<()>>,
    entries: FxHashMap<NodeKey, (u64, u64)>,
}
impl NativeSignatures {
    pub(super) fn observe(
        &mut self,
        arena: &NodeArena,
        key: NodeKey,
        element: &dyn ElementTrait,
    ) -> u64 {
        let identity = arena.mutation_identity();
        if self
            .arena
            .as_ref()
            .is_none_or(|old| !Arc::ptr_eq(old, &identity))
        {
            self.entries.clear();
            self.arena = Some(identity);
        }
        let revision = arena.mutation_revision(key);
        if let Some((old_revision, signature)) = self.entries.get(&key) {
            if revision == Some(*old_revision) {
                return *signature;
            }
        }
        let host = element.as_any();
        let tracked = host
            .downcast_ref::<Element>()
            .is_some_and(Element::paint_signature_inputs_are_tracked)
            || host
                .downcast_ref::<Text>()
                .is_some_and(Text::paint_signature_inputs_are_tracked);
        let signature = element.retained_paint_signature();
        if let Some(revision) = revision.filter(|_| tracked) {
            self.entries.insert(key, (revision, signature));
        } else {
            self.entries.remove(&key);
        }
        signature
    }
    pub(super) fn tracked_revision(&self, arena: &NodeArena, key: NodeKey) -> Option<u64> {
        let revision = arena.mutation_revision(key)?;
        (self
            .arena
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, &arena.mutation_identity()))
            && self
                .entries
                .get(&key)
                .is_some_and(|(old, _)| *old == revision))
        .then_some(revision)
    }
    pub(super) fn forget(&mut self, key: NodeKey) {
        self.entries.remove(&key);
    }
    pub(super) fn prune(&mut self, arena: &NodeArena) {
        self.entries.retain(|key, _| arena.contains_key(*key));
    }
}

#[cfg(test)]
mod tests;
