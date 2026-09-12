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
        self.compositor.property_trees.sync(arena, roots);

        let property_trees = &self.compositor.property_trees;
        let tracker = &mut self.compositor.paint_generations;
        tracker.begin_frame(roots);
        let mut seen = FxHashSet::default();
        let mut pending = roots.iter().rev().copied().collect::<Vec<_>>();
        while let Some(key) = pending.pop() {
            if !seen.insert(key) {
                continue;
            }
            let Some(node) = arena.get(key) else {
                continue;
            };
            let children = node.children().to_vec();
            tracker.observe_node(
                key,
                node.parent(),
                &children,
                node.element.as_ref(),
                property_trees,
            );
            pending.extend(children.into_iter().rev());
        }
        tracker.finish_frame(arena);
    }

    #[cfg(test)]
    pub(super) fn compositor_property_tree_epoch(&self) -> u64 {
        self.compositor.property_trees.epoch()
    }
}

#[cfg(test)]
mod tests;
