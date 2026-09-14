//! Bounded, multi-reader mutation history. Missing history rejects replay;
//! entries never authorize paint validity, topology or resource residency.
use super::*;
use std::collections::VecDeque;
const CAPACITY: usize = 4096;
#[derive(Default)]
pub(super) struct MutationHistory {
    entries: VecDeque<(u64, NodeKey)>,
}
impl MutationHistory {
    pub(super) fn record(&mut self, revision: u64, owner: NodeKey) {
        if self.entries.len() == CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back((revision, owner));
    }
    fn since(&self, revision: u64, current: u64) -> Option<Vec<NodeKey>> {
        if current == u64::MAX || revision > current {
            return None;
        }
        if revision == current {
            return Some(Vec::new());
        }
        if self.entries.front()?.0 > revision.checked_add(1)? {
            return None;
        }
        let mut seen = FxHashSet::default();
        Some(
            self.entries
                .iter()
                .rev()
                .take_while(|(clock, _)| *clock > revision)
                .filter_map(|(_, key)| seen.insert(*key).then_some(*key))
                .collect(),
        )
    }
}
impl NodeArena {
    /// Complete distinct owners since a reader's last observation, or None
    /// when the bounded history can no longer cover that interval.
    pub(crate) fn mutated_nodes_since(&self, revision: u64) -> Option<Vec<NodeKey>> {
        self.mutation_history
            .borrow()
            .since(revision, self.mutation_clock.get())
    }
}
#[cfg(test)]
mod tests;
