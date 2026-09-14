//! Keep the reachability epoch when exact topology is unchanged. Only native
//! mutable storage and current property generations can suppress observations;
//! opaque/external inputs are still observed in the original traversal order.
use super::*;
use crate::view::compositor::property_tree::GenerationStoreStamp;

pub(in super::super) struct SceneObservation {
    arena: Arc<()>,
    clock: u64,
    generations: GenerationStoreStamp,
    order: FxHashMap<NodeKey, usize>,
    volatile: FxHashSet<NodeKey>,
}
impl SceneObservation {
    pub(super) fn capture(
        tracker: &PaintGenerationTracker,
        arena: &NodeArena,
        trees: &PropertyTrees,
        order: Vec<NodeKey>,
    ) -> Option<Self> {
        let clock = arena.mutation_clock();
        if clock == u64::MAX {
            return None;
        }
        let mut volatile = FxHashSet::default();
        for &key in &order {
            let record = tracker.nodes.get(&key)?;
            let node = arena.get(key)?;
            if !record.active
                || record.observed_parent != node.parent()
                || record.observed_children != node.children()
            {
                return None;
            }
            if let Some((identity, revision)) = &record.native_observation {
                if !Arc::ptr_eq(identity, &arena.mutation_identity())
                    || arena.mutation_revision(key) != Some(*revision)
                {
                    return None;
                }
            }
            if record.native_observation.is_none() {
                volatile.insert(key);
            }
        }
        Some(Self {
            arena: arena.mutation_identity(),
            clock,
            generations: trees.generation_store_stamp(),
            order: order
                .into_iter()
                .enumerate()
                .map(|(i, key)| (key, i))
                .collect(),
            volatile,
        })
    }
}
impl PaintGenerationTracker {
    pub(super) fn sync_changed_observations(
        &mut self,
        arena: &NodeArena,
        roots: &[NodeKey],
        trees: &PropertyTrees,
    ) -> Option<usize> {
        let mut scene = self.native_scene.take()?;
        if self.observed_roots != roots || !Arc::ptr_eq(&scene.arena, &arena.mutation_identity()) {
            return None;
        }
        let mutated = arena.mutated_nodes_since(scene.clock)?;
        let mut changed = trees.generation_writes_since(&scene.generations)?;
        changed.extend(mutated.iter().copied());
        changed.extend(scene.volatile.iter().copied());
        let mut pending = changed
            .into_iter()
            .filter_map(|key| scene.order.get(&key).map(|rank| (*rank, key)))
            .collect::<Vec<_>>();
        pending.sort_unstable();
        // Check the entire changed topology before publishing any observation.
        // Insert/remove/reparent/reorder switches to the original full walk.
        for &(_, key) in &pending {
            let node = arena.get(key)?;
            let record = self.nodes.get(&key)?;
            if !record.active
                || record.observed_parent != node.parent()
                || record.observed_children != node.children()
            {
                return None;
            }
        }
        if self.next_revision > u64::MAX.saturating_sub(pending.len() as u64 * 3) {
            return None;
        }
        let observed_clock = arena.mutation_clock();
        let mut replayed = scene.order.len() - scene.volatile.len();
        for (_, key) in pending {
            let node = arena.get(key)?;
            let previous = self.nodes.get(&key)?;
            if previous.native_observation.is_some() {
                replayed = replayed.saturating_sub(1);
            }
            self.observe_native_node(
                arena,
                key,
                node.parent(),
                node.children(),
                node.element.as_ref(),
                trees,
            );
            if self.nodes.get(&key)?.native_observation.is_some() {
                scene.volatile.remove(&key);
            } else {
                scene.volatile.insert(key);
            }
        }
        // Unreachable records can be removed without sweeping every live node.
        for key in mutated {
            if !arena.contains_key(key) {
                self.nodes.remove(&key);
                self.native_signatures.get_mut().forget(key);
            }
        }
        if arena.mutation_clock() != observed_clock {
            return None;
        }
        scene.clock = observed_clock;
        scene.generations = trees.generation_store_stamp();
        let count = scene.order.len();
        self.native_observation_replays = replayed;
        // Epoch certifies unchanged reachability. It need not advance when the
        // exact same owner set remains active; local revisions still advance.
        self.native_scene = Some(scene);
        Some(count)
    }
}
