use super::*;
use crate::view::base_component::{DirtyPassMask, Element, Text};

/// Owned by one arena and one render attempt. Layout flag consumption cannot
/// acknowledge this capture. A later mutation preserves pending work even when
/// it repeats the same dirty bit. This journal does not establish GPU validity.
pub(crate) struct RenderChangeCapture {
    identity: Arc<()>,
    owners: Vec<(NodeKey, DirtyFlags, [u64; 8])>,
}

/// Reuses the arena's bounded mutation history. Candidates retain failed or
/// unconsumed work and hosts whose dirty getters can change without mutation.
pub(super) struct RenderChangeObservation {
    revision: u64,
    // A dense list keeps sparse captures proportional to owners, not the
    // capacity of a hash table left over from an earlier full observation.
    candidates: Vec<NodeKey>,
}

const CAUSES: [DirtyFlags; 8] = [
    DirtyFlags::LAYOUT,
    DirtyFlags::PLACE,
    DirtyFlags::BOX_MODEL,
    DirtyFlags::HIT_TEST,
    DirtyFlags::PAINT,
    DirtyFlags::COMPOSITE,
    DirtyFlags::RECORDING_TOPOLOGY,
    DirtyFlags::RESOURCE,
];

impl Node {
    pub(super) fn record_render_causes(&self, flags: DirtyFlags, revision: u64) {
        let mut versions = self.render_change_versions.get();
        for (index, cause) in CAUSES.iter().enumerate() {
            if flags.intersects(*cause) {
                versions[index] = revision
            }
        }
        self.render_change_versions.set(versions);
        self.pending_render_changes
            .set(self.pending_render_changes.get().union(flags));
    }
}

impl NodeArena {
    pub(super) fn note_mutation(&self, key: NodeKey) {
        if let Some(node) = self.slots.get(key) {
            let revision = self.mutation_clock.get().saturating_add(1);
            self.mutation_clock.set(revision);
            self.mutation_history.borrow_mut().record(revision, key);
            node.mutation_revision.set(revision);
            self.propagate_subtree_mutation(key, revision);
            // Unclassified mutable access requests input observation, not a
            // raster invalidation. Exact metadata can still establish equality.
            node.record_render_causes(DirtyFlags::PAINT, revision);
        }
    }

    pub(super) fn note_topology_change(&self, key: NodeKey) {
        self.note_mutation(key);
        if let Some(node) = self.slots.get(key) {
            // Wiring APIs historically leave layout/redraw scheduling to the
            // reconciler. Record the cause without changing that public contract.
            node.record_render_causes(DirtyFlags::RECORDING_TOPOLOGY, self.mutation_clock.get());
        }
    }

    /// Mutable access invalidates a native observation even if a caller edits
    /// a field directly and then clears dirty flags. External resources and
    /// custom getters still require their own live observations.
    pub(crate) fn mutation_revision(&self, key: NodeKey) -> Option<u64> {
        let revision = self.slots.get(key)?.mutation_revision.get();
        (revision != u64::MAX).then_some(revision)
    }

    pub(crate) fn mutation_clock(&self) -> u64 {
        self.mutation_clock.get()
    }

    /// Only a change detector. Consumers must additionally certify tracked
    /// native inputs, coherent edges, inherited inputs and arena identity.
    pub(crate) fn subtree_mutation_revision(&self, key: NodeKey) -> Option<u64> {
        if self.mutation_clock.get() == u64::MAX {
            return None;
        }
        self.slots
            .get(key)
            .map(|node| node.subtree_mutation_revision.get())
    }

    fn propagate_subtree_mutation(&self, key: NodeKey, revision: u64) {
        let mut cursor = Some(key);
        // Corrupt parent graphs still terminate. They cannot acquire a valid
        // subtree proof; normal trees require only the ancestor depth here.
        for _ in 0..self.slots.len() {
            let Some(node) = cursor.and_then(|key| self.slots.get(key)) else {
                break;
            };
            if node.subtree_mutation_revision.get() == revision {
                break;
            }
            node.subtree_mutation_revision.set(revision);
            cursor = node.parent;
        }
    }

    pub(crate) fn mutation_identity(&self) -> Arc<()> {
        self.mutation_identity.clone()
    }

    pub(crate) fn pending_render_changes(&self, key: NodeKey) -> DirtyFlags {
        self.slots.get(key).map_or(DirtyFlags::NONE, |node| {
            node.pending_render_changes
                .get()
                .union(node.arena_local_dirty.get())
                .union(node.element.borrow().local_dirty_flags())
        })
    }

    pub(super) fn observe_render_causes(&self, key: NodeKey, flags: DirtyFlags) {
        let Some(node) = self.slots.get(key) else {
            return;
        };
        let added = flags.without(node.pending_render_changes.get());
        if !added.is_empty() {
            let revision = self.mutation_clock.get().saturating_add(1);
            self.mutation_clock.set(revision);
            self.mutation_history.borrow_mut().record(revision, key);
            node.record_render_causes(added, revision);
        }
    }

    /// After final layout/resource/property observation, before paint hooks.
    /// The existing layout prepass preserves consumed local causes; this also
    /// observes post-layout resource preparation without consuming it.
    pub(crate) fn capture_render_changes(&self) -> RenderChangeCapture {
        let _profile = crate::view::base_component::layout_profile_scope(
            crate::view::base_component::LayoutPlaceTiming::ChangeCapture,
        );
        let previous = self.render_change_observation.borrow_mut().take();
        let mut candidates = match previous {
            Some(mut previous) => match self.mutated_nodes_since(previous.revision) {
                Some(changed) => {
                    previous.candidates.extend(changed);
                    previous.candidates
                }
                None => self.slots.keys().collect(),
            },
            None => self.slots.keys().collect(),
        };
        // Start before invoking getters: mutations during observation must
        // remain visible to the next capture, including other owners.
        let revision = self.mutation_clock();
        let mut owners = Vec::new();
        let mut seen = FxHashSet::default();
        candidates.retain(|&key| {
            if !seen.insert(key) {
                return false;
            }
            let Some(node) = self.slots.get(key) else {
                return false;
            };
            crate::ui::work_profile::count(|p| p.render_change_observations += 1);
            self.observe_render_causes(key, self.pending_render_changes(key));
            let flags = node.pending_render_changes.get();
            if !flags.is_empty() {
                owners.push((key, flags, node.render_change_versions.get()));
            }
            !flags.is_empty() || !node.element.borrow().dirty_observation_is_tracked()
        });
        *self.render_change_observation.borrow_mut() = Some(RenderChangeObservation {
            revision,
            candidates,
        });
        RenderChangeCapture {
            identity: self.mutation_identity(),
            owners,
        }
    }

    pub(crate) fn commit_render_changes(&self, capture: RenderChangeCapture) {
        if !Arc::ptr_eq(&capture.identity, &self.mutation_identity) {
            return;
        }
        for (key, captured_flags, captured_versions) in capture.owners {
            let Some(node) = self.slots.get(key) else {
                continue;
            };
            let current_versions = node.render_change_versions.get();
            let mut pending = node.pending_render_changes.get();
            for (index, cause) in CAUSES.iter().enumerate() {
                if captured_flags.intersects(*cause)
                    && captured_versions[index] != u64::MAX
                    && current_versions[index] == captured_versions[index]
                {
                    pending = pending.without(*cause);
                }
            }
            node.pending_render_changes.set(pending);
        }
    }

    /// These exact native implementations only subtract dirty bits. Unknown
    /// implementations still receive their hook with tracked mutable access:
    /// a custom clear hook may change content or another node.
    pub(crate) fn clear_element_dirty_flags(&self, key: NodeKey, flags: DirtyFlags) -> bool {
        let Some(node) = self.slots.get(key) else {
            return false;
        };
        crate::ui::work_profile::count(|p| p.dirty_clear_visits += 1);
        let native_bookkeeping = {
            let element = node.element.borrow();
            element.as_any().is::<Element>() || element.as_any().is::<Text>()
        };
        if native_bookkeeping {
            let mut element = node.element.borrow_mut();
            let before = element.local_dirty_flags();
            element.clear_local_dirty_flags(flags);
            if element.local_dirty_flags() != before {
                self.invalidate_dirty_observation(key);
            }
        } else if let Some(mut node) = self.get_mut(key) {
            node.element.clear_local_dirty_flags(flags);
        }
        true
    }

    pub(crate) fn render_consumption_mask() -> DirtyFlags {
        DirtyPassMask::RECORDING.union(DirtyFlags::COMPOSITE)
    }
}

#[cfg(test)]
mod tests;
