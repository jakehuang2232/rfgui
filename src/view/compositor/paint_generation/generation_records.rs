//! Journal only the exact local-generation projection consumed by recording.
//! Bookkeeping writes do not invalidate it; unclassified writes still do.
use super::{LocalPaintGenerations, NodeGenerationRecord, NodeKey};
use crate::view::compositor::property_tree::observed_map::{ObservedMap, Stamp};
use rustc_hash::FxHashMap;
use std::ops::{Deref, DerefMut};

#[derive(Default)]
pub(super) struct GenerationRecords {
    values: FxHashMap<NodeKey, NodeGenerationRecord>,
    history: ObservedMap<NodeKey, ()>,
}
fn projection(record: &NodeGenerationRecord) -> Option<LocalPaintGenerations> {
    record.active.then_some(LocalPaintGenerations {
        self_paint_revision: record.self_paint_revision,
        composite_revision: record.composite_revision,
        topology_revision: record.topology_revision,
    })
}
impl GenerationRecords {
    pub(super) fn stamp(&self) -> Stamp {
        self.history.stamp()
    }
    pub(super) fn changed_since(&self, old: &Stamp, visit: impl FnMut(NodeKey)) -> Option<()> {
        self.history.changed_since(old, visit)
    }
    pub(super) fn get_mut(&mut self, key: &NodeKey) -> Option<RecordWrite<'_>> {
        let record = self.values.get_mut(key)?;
        Some(RecordWrite {
            before: projection(record),
            record,
            key: *key,
            history: &mut self.history,
        })
    }
    pub(super) fn insert(
        &mut self,
        key: NodeKey,
        value: NodeGenerationRecord,
    ) -> Option<NodeGenerationRecord> {
        let after = projection(&value);
        let old = self.values.insert(key, value);
        if old.as_ref().and_then(projection) != after {
            self.history.write(key);
        }
        old
    }
    pub(super) fn remove(&mut self, key: &NodeKey) -> Option<NodeGenerationRecord> {
        let old = self.values.remove(key)?;
        if projection(&old).is_some() {
            self.history.write(*key);
        }
        Some(old)
    }
    pub(super) fn retain(&mut self, mut keep: impl FnMut(&NodeKey, &NodeGenerationRecord) -> bool) {
        let removed = self
            .values
            .iter()
            .filter_map(|(key, value)| (!keep(key, value)).then_some(*key))
            .collect::<Vec<_>>();
        for key in removed {
            self.remove(&key);
        }
    }
}
pub(super) struct RecordWrite<'a> {
    before: Option<LocalPaintGenerations>,
    record: &'a mut NodeGenerationRecord,
    key: NodeKey,
    history: &'a mut ObservedMap<NodeKey, ()>,
}
impl Deref for RecordWrite<'_> {
    type Target = NodeGenerationRecord;
    fn deref(&self) -> &Self::Target {
        self.record
    }
}
impl DerefMut for RecordWrite<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.record
    }
}
impl Drop for RecordWrite<'_> {
    fn drop(&mut self) {
        if self.before != projection(self.record) {
            self.history.write(self.key);
        }
    }
}
impl Deref for GenerationRecords {
    type Target = FxHashMap<NodeKey, NodeGenerationRecord>;
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}
impl DerefMut for GenerationRecords {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // A fresh identity rejects every older proof before bulk access escapes.
        self.history = ObservedMap::default();
        &mut self.values
    }
}
impl<'a> IntoIterator for &'a GenerationRecords {
    type Item = (&'a NodeKey, &'a NodeGenerationRecord);
    type IntoIter = std::collections::hash_map::Iter<'a, NodeKey, NodeGenerationRecord>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}
