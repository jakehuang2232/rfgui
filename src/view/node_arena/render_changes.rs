use super::*;
use crate::view::base_component::{DirtyPassMask, Element, Text};

/// Owned by one arena and one render attempt. Layout flag consumption cannot
/// acknowledge this capture. A later mutation preserves pending work even when
/// it repeats the same dirty bit. This journal does not establish GPU validity.
pub(crate) struct RenderChangeCapture {
    identity: Arc<()>,
    owners: Vec<(NodeKey, DirtyFlags, [u64; 8])>,
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
            node.mutation_revision.set(revision);
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
            node.record_render_causes(added, revision);
        }
    }

    /// After final layout/resource/property observation, before paint hooks.
    /// The existing layout prepass preserves consumed local causes; this also
    /// observes post-layout resource preparation without consuming it.
    pub(crate) fn capture_render_changes(&self) -> RenderChangeCapture {
        let owners = self
            .slots
            .iter()
            .map(|(key, node)| {
                self.observe_render_causes(key, self.pending_render_changes(key));
                (
                    key,
                    node.pending_render_changes.get(),
                    node.render_change_versions.get(),
                )
            })
            .collect();
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
        let native_bookkeeping = {
            let element = node.element.borrow();
            element.as_any().is::<Element>() || element.as_any().is::<Text>()
        };
        if native_bookkeeping {
            node.element.borrow_mut().clear_local_dirty_flags(flags);
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
