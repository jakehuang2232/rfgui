use rustc_hash::{FxHashMap, FxHashSet};
use std::hash::Hash;

/// Walk only the not-yet-closed prefix. `known` is populated after a complete
/// successful walk, so an in-progress cycle can never masquerade as reuse.
/// The caller owns one immutable PropertyTrees snapshot for this entire closure.
#[cfg(test)]
pub(super) fn unseen_chain<K: Copy + Eq + Hash, S>(
    leaf: Option<K>,
    known: &FxHashMap<K, S>,
    fetch: impl FnMut(K) -> Option<S>,
    parent: impl FnMut(&S) -> Option<K>,
) -> Option<Vec<S>> {
    let mut scratch = ChainScratch::default();
    Some(scratch.unseen_chain(leaf, known, fetch, parent)?.collect())
}

/// One allocation set per snapshot family and closure invocation. Only the
/// completed `known` map may stop a walk; scratch is cleared even after errors.
pub(super) struct ChainScratch<K, S> {
    result: Vec<S>,
    seen: FxHashSet<K>,
}
impl<K, S> Default for ChainScratch<K, S> {
    fn default() -> Self {
        Self {
            result: Vec::new(),
            seen: FxHashSet::default(),
        }
    }
}
impl<K: Copy + Eq + Hash, S> ChainScratch<K, S> {
    pub(super) fn unseen_chain(
        &mut self,
        leaf: Option<K>,
        known: &FxHashMap<K, S>,
        mut fetch: impl FnMut(K) -> Option<S>,
        mut parent: impl FnMut(&S) -> Option<K>,
    ) -> Option<std::vec::Drain<'_, S>> {
        self.result.clear();
        self.seen.clear();
        let mut cursor = leaf;
        while let Some(id) = cursor {
            if known.contains_key(&id) {
                break;
            }
            if !self.seen.insert(id) {
                return None;
            }
            let snapshot = fetch(id)?;
            cursor = parent(&snapshot);
            self.result.push(snapshot);
        }
        Some(self.result.drain(..))
    }
}

#[cfg(test)]
mod tests;
