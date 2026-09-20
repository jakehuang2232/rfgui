//! Renderer-neutral compositor observation.
//!
//! Property trees and paint generations are shared inputs for retained paint
//! planning.

use super::*;

impl Viewport {
    /// Captures one coherent live compositor snapshot after layout and paint
    /// resource preparation. Property topology is frozen first because paint
    /// generation observations include the owning property generations.
    pub(super) fn sync_compositor_property_trees(&mut self) {
        let arena = &self.scene.node_arena;
        let roots = &self.scene.ui_root_keys;
        crate::view::base_component::profile_layout_place_time(
            crate::view::base_component::LayoutPlaceTiming::PropertySync,
            || self.compositor.property_trees.sync(arena, roots),
        );

        let property_trees = &self.compositor.property_trees;
        let tracker = &mut self.compositor.paint_generations;
        let visited = crate::view::base_component::profile_layout_place_time(
            crate::view::base_component::LayoutPlaceTiming::GenerationSync,
            || tracker.sync_arena(arena, roots, property_trees),
        );
        if self.debug_options.trace_compile_detail {
            let mut changed = property_trees.changes.iter().collect::<Vec<_>>();
            changed.sort_by_key(|(key, _)| **key);
            let dirty = arena
                .iter()
                .filter(|(key, _)| !arena.pending_render_changes(*key).is_empty())
                .count();
            eprintln!(
                "property-sync nodes={} visited={} observed={} replayed={} generation_replayed={} changed={} pending={} owners={:?}",
                arena.len(),
                visited,
                property_trees.observed_nodes,
                property_trees.replayed_nodes,
                tracker.native_observation_replays,
                changed.len(),
                dirty,
                &changed[..changed.len().min(24)]
            );
        }
    }

    #[cfg(test)]
    pub(super) fn compositor_property_tree_epoch(&self) -> u64 {
        self.compositor.property_trees.epoch()
    }
}

#[cfg(test)]
mod tests;
