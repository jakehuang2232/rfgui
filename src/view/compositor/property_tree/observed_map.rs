//! Exact mutation history for stored property values, including direct edits.
//! This certifies absence of writes, not validity of a value or GPU residency.
use super::*;
use std::collections::VecDeque;
use std::hash::Hash;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

const HISTORY_LIMIT: usize = 4096;

#[derive(Debug)]
pub(crate) struct ObservedMap<K, V> {
    values: FxHashMap<K, V>,
    identity: Arc<()>,
    clock: u64,
    floor: u64,
    writes: VecDeque<(u64, K)>,
}
impl<K, V> Default for ObservedMap<K, V> {
    fn default() -> Self {
        Self {
            values: FxHashMap::default(),
            identity: Arc::new(()),
            clock: 0,
            floor: 0,
            writes: VecDeque::new(),
        }
    }
}
#[derive(Clone)]
pub(crate) struct Stamp {
    identity: Arc<()>,
    clock: u64,
}
impl PartialEq for Stamp {
    fn eq(&self, other: &Self) -> bool {
        self.clock == other.clock && Arc::ptr_eq(&self.identity, &other.identity)
    }
}
pub(super) struct WriteQuery {
    before: PropertyStoreStamp,
    after: PropertyStoreStamp,
    owners: Arc<FxHashSet<NodeKey>>,
}
impl<K: Copy + Eq + Hash, V> ObservedMap<K, V> {
    pub(crate) fn stamp(&self) -> Stamp {
        Stamp {
            identity: self.identity.clone(),
            clock: self.clock,
        }
    }
    pub(crate) fn write(&mut self, key: K) {
        self.clock = self.clock.saturating_add(1);
        self.writes.push_back((self.clock, key));
        if self.writes.len() > HISTORY_LIMIT {
            self.floor = self.writes.pop_front().unwrap().0;
        }
    }
    pub(crate) fn changed_since(&self, old: &Stamp, mut visit: impl FnMut(K)) -> Option<()> {
        if !Arc::ptr_eq(&self.identity, &old.identity)
            || self.clock == u64::MAX
            || old.clock > self.clock
            || old.clock < self.floor
        {
            return None;
        }
        for &(clock, key) in self.writes.iter().rev() {
            if clock <= old.clock {
                break;
            }
            visit(key);
        }
        Some(())
    }
    pub(crate) fn entry(&mut self, key: K) -> std::collections::hash_map::Entry<'_, K, V> {
        self.write(key);
        self.values.entry(key)
    }
    pub(crate) fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.write(key);
        self.values.insert(key, value)
    }
    pub(crate) fn remove(&mut self, key: &K) -> Option<V> {
        let value = self.values.remove(key)?;
        self.write(*key);
        Some(value)
    }
    pub(crate) fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        if !self.values.contains_key(key) {
            return None;
        }
        self.write(*key);
        self.values.get_mut(key)
    }
    pub(crate) fn values_mut(&mut self) -> std::collections::hash_map::ValuesMut<'_, K, V> {
        let keys = self.values.keys().copied().collect::<Vec<_>>();
        for key in keys {
            self.write(key);
        }
        self.values.values_mut()
    }
    // Read-only predicates cannot conceal a write to a retained value.
    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&K, &V) -> bool) {
        let removed = self
            .values
            .iter()
            .filter_map(|(k, v)| (!keep(k, v)).then_some(*k))
            .collect::<Vec<_>>();
        for key in removed {
            self.remove(&key);
        }
    }
}
impl<K, V> Deref for ObservedMap<K, V> {
    type Target = FxHashMap<K, V>;
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}
impl<K: Eq + Hash, V: PartialEq> PartialEq for ObservedMap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}
impl<'a, K, V> IntoIterator for &'a ObservedMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = std::collections::hash_map::Iter<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}
impl<K, V> DerefMut for ObservedMap<K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // Unclassified mutable access (entry, iter_mut, replacement, etc.)
        // invalidates every older proof before the mutable reference escapes.
        self.clock = self.clock.saturating_add(1);
        self.floor = self.clock;
        self.writes.clear();
        &mut self.values
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct PropertyStoreStamp([Stamp; 7]);
#[derive(Clone)]
pub(crate) struct GenerationStoreStamp([Stamp; 3]);
impl PropertyTrees {
    pub(crate) fn generation_store_stamp(&self) -> GenerationStoreStamp {
        GenerationStoreStamp([
            self.transform_generations.stamp(),
            self.effect_generations.stamp(),
            self.scroll_generations.stamp(),
        ])
    }
    pub(crate) fn generation_writes_since(
        &self,
        old: &GenerationStoreStamp,
    ) -> Option<FxHashSet<NodeKey>> {
        let mut owners = FxHashSet::default();
        self.transform_generations.changed_since(&old.0[0], |id| {
            owners.insert(id.0);
        })?;
        self.effect_generations.changed_since(&old.0[1], |id| {
            owners.insert(id.0);
        })?;
        self.scroll_generations.changed_since(&old.0[2], |id| {
            owners.insert(id.0);
        })?;
        Some(owners)
    }

    pub(crate) fn property_store_stamp(&self) -> PropertyStoreStamp {
        PropertyStoreStamp([
            self.transforms.stamp(),
            self.layout_positions.stamp(),
            self.visual_offsets.stamp(),
            self.clips.stamp(),
            self.effects.stamp(),
            self.scrolls.stamp(),
            self.states.stamp(),
        ])
    }
    /// All writes since the supplied stamp, including removals and changes to
    /// raw snapshots without generation bumps. None requires a complete read.
    pub(crate) fn property_writes_since(
        &self,
        old: &PropertyStoreStamp,
    ) -> Option<Arc<FxHashSet<NodeKey>>> {
        let after = self.property_store_stamp();
        if let Some(query) = self.recent_write_query.borrow().as_ref() {
            if query.before == *old && query.after == after {
                return Some(query.owners.clone());
            }
        }
        let mut owners = FxHashSet::default();
        self.transforms.changed_since(&old.0[0], |id| {
            owners.insert(id.0);
        })?;
        self.layout_positions.changed_since(&old.0[1], |id| {
            owners.insert(id.0);
        })?;
        self.visual_offsets.changed_since(&old.0[2], |id| {
            owners.insert(id.0);
        })?;
        self.clips.changed_since(&old.0[3], |id| {
            owners.insert(id.owner);
        })?;
        self.effects.changed_since(&old.0[4], |id| {
            owners.insert(id.0);
        })?;
        self.scrolls.changed_since(&old.0[5], |id| {
            owners.insert(id.0);
        })?;
        self.states.changed_since(&old.0[6], |owner| {
            owners.insert(owner);
        })?;
        let owners = Arc::new(owners);
        *self.recent_write_query.borrow_mut() = Some(WriteQuery {
            before: old.clone(),
            after,
            owners: owners.clone(),
        });
        Some(owners)
    }
}

#[cfg(test)]
mod tests;
