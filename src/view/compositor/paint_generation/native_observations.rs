//! Reuse the original local generation observation of immutable native inputs.
//! Property generations remain current reads; no paint payload, eligibility or
//! GPU residency is inferred from this mutation certificate.
use super::*;
use std::sync::Arc;
mod scene_observation;
pub(super) use scene_observation::SceneObservation;

impl PaintGenerationTracker {
    pub(crate) fn sync_arena(
        &mut self,
        arena: &NodeArena,
        roots: &[NodeKey],
        properties: &PropertyTrees,
    ) -> usize {
        if let Some(count) = self.sync_changed_observations(arena, roots, properties) {
            return count;
        }
        self.begin_frame(roots);
        self.native_observation_replays = 0;
        let identity = arena.mutation_identity();
        let mut seen = FxHashSet::default();
        let mut order = Vec::new();
        let mut complete = true;
        let mut pending = roots.iter().rev().copied().collect::<Vec<_>>();
        while let Some(key) = pending.pop() {
            if !seen.insert(key) {
                complete = false;
                continue;
            }
            order.push(key);
            let replayed = arena.mutation_revision(key).is_some_and(|revision| {
                let Some(mut record) = self.nodes.get_mut(&key) else {
                    return false;
                };
                if record.active
                    && record
                        .native_observation
                        .as_ref()
                        .is_some_and(|(old, observed)| {
                            Arc::ptr_eq(old, &identity) && *observed == revision
                        })
                    && record.observed_transform_generation
                        == properties.transform_generation_for_owner(key)
                    && record.observed_effect_generation
                        == properties.effect_generation_for_owner(key)
                    && record.observed_scroll_generation
                        == properties.scroll_generation_for_owner(key)
                {
                    record.last_seen_epoch = self.epoch;
                    pending.extend(record.observed_children.iter().rev().copied());
                    true
                } else {
                    false
                }
            });
            if replayed {
                self.native_observation_replays += 1;
                continue;
            }
            let Some(node) = arena.get(key) else {
                complete = false;
                continue;
            };
            self.observe_native_node(
                arena,
                key,
                node.parent(),
                node.children(),
                node.element.as_ref(),
                properties,
            );
            pending.extend(node.children().iter().rev().copied());
        }
        self.finish_frame(arena);
        if complete {
            self.native_scene = SceneObservation::capture(self, arena, properties, order);
        }
        seen.len()
    }
}

#[cfg(test)]
mod tests;
