//! A retained census of exactly the topology observed by preflight. Native
//! owned getters can use complete arena mutation history; opaque getters are
//! read on every attempt. This never authorizes property or command validity.
use super::*;
use crate::view::base_component::{Element, Text};
use rustc_hash::FxHashSet;

pub(crate) struct TopologySnapshot {
    pub(crate) owner_parents: Arc<FxHashMap<NodeKey, Option<NodeKey>>>,
    pub(crate) covered: Arc<FxHashSet<NodeKey>>,
    pub(crate) deferred_roots: Vec<NodeKey>,
    pub(crate) deferred: FxHashSet<NodeKey>,
}
#[derive(PartialEq, Eq)]
struct Observation {
    stable_id: u64,
    children: Vec<NodeKey>,
    arena_children: Vec<NodeKey>,
    parent: Option<NodeKey>,
    deferred: bool,
    native: bool,
}
impl Observation {
    fn read(arena: &NodeArena, key: NodeKey) -> Option<Self> {
        let node = arena.get(key)?;
        let element = node.element.as_ref();
        let children = element.children().to_vec();
        let host = element.as_any();
        let value = Self {
            stable_id: element.stable_id(),
            native: (host.is::<Element>() || host.is::<Text>()) && node.children() == children,
            arena_children: node.children().to_vec(),
            parent: node.parent(),
            children,
            deferred: element.is_deferred_to_root_viewport_render(),
        };
        // A getter that changes while being observed cannot form a retained
        // census, even if the arena's mutation history remained unchanged.
        (value.children == element.children()).then_some(value)
    }
}
struct Entry {
    arena: Arc<()>,
    roots: Vec<NodeKey>,
    revision: u64,
    snapshot: Arc<TopologySnapshot>,
    observations: FxHashMap<NodeKey, Observation>,
    volatile: Vec<NodeKey>,
    coherent_edges: bool,
}
#[derive(Default)]
pub(crate) struct TopologyCache {
    entry: Option<Entry>,
}
impl TopologyCache {
    pub(crate) fn replay(
        &mut self,
        arena: &NodeArena,
        roots: &[NodeKey],
    ) -> Option<Arc<TopologySnapshot>> {
        let entry = self.entry.as_mut()?;
        if !Arc::ptr_eq(&entry.arena, &arena.mutation_identity()) || entry.roots != roots {
            return None;
        }
        let mut changed = arena.mutated_nodes_since(entry.revision)?;
        changed.extend(entry.volatile.iter().copied());
        let mut seen = FxHashSet::default();
        for key in changed {
            if !seen.insert(key) {
                continue;
            }
            if let Some(previous) = entry.observations.get(&key) {
                if Observation::read(arena, key).as_ref() != Some(previous) {
                    return None;
                }
            }
        }
        entry.revision = arena.mutation_clock();
        super::super::work_profile::count("topology_census_replays", 1);
        Some(entry.snapshot.clone())
    }
    /// Reuse only the exact coherent census, rechecking changed and opaque
    /// observations after recording hooks. Native non-deferred owners have no
    /// other SurfaceDag boundary input; deferred/opaque owners stay live.
    pub(crate) fn boundary_nodes(
        &mut self,
        arena: &NodeArena,
        roots: &[NodeKey],
    ) -> Option<Vec<NodeKey>> {
        let snapshot = self.replay(arena, roots)?;
        let entry = self.entry.as_ref()?;
        if !entry.coherent_edges {
            return None;
        }
        let mut nodes = snapshot.deferred_roots.clone();
        nodes.extend(
            entry
                .volatile
                .iter()
                .copied()
                .filter(|key| !snapshot.deferred.contains(key)),
        );
        Some(nodes)
    }
    pub(crate) fn remember(
        &mut self,
        arena: &NodeArena,
        roots: &[NodeKey],
        snapshot: Arc<TopologySnapshot>,
        stable_keys: &FxHashMap<u64, NodeKey>,
        children: &FxHashMap<NodeKey, Vec<NodeKey>>,
    ) {
        self.entry = None;
        let revision = arena.mutation_clock();
        let mut observations = FxHashMap::default();
        let mut volatile = Vec::new();
        for &key in snapshot.covered.iter() {
            let Some(observation) = Observation::read(arena, key) else {
                return;
            };
            // Mint the proof only for the actual census just used by preflight,
            // including sibling order and deferred traversal membership.
            if stable_keys.get(&observation.stable_id) != Some(&key)
                || children.get(&key) != Some(&observation.children)
                || snapshot.deferred.contains(&key) != observation.deferred
            {
                return;
            }
            if !observation.native {
                volatile.push(key);
            }
            observations.insert(key, observation);
        }
        if revision != arena.mutation_clock() {
            return;
        }
        let coherent_edges = observations.iter().all(|(key, node)| {
            node.children == node.arena_children
                && node.children.iter().all(|child| {
                    observations
                        .get(child)
                        .is_some_and(|child| child.parent == Some(*key))
                })
        });
        self.entry = Some(Entry {
            arena: arena.mutation_identity(),
            roots: roots.to_vec(),
            revision,
            snapshot,
            observations,
            volatile,
            coherent_edges,
        });
    }
    pub(super) fn finish(&mut self, accepted: bool) {
        if !accepted {
            self.entry = None;
        }
    }
}

#[cfg(test)]
mod tests;
