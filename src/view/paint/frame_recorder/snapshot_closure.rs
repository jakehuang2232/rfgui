use rustc_hash::{FxHashMap, FxHashSet};
use std::hash::Hash;

/// Walk only the not-yet-closed prefix. `known` is populated after a complete
/// successful walk, so an in-progress cycle can never masquerade as reuse.
/// The caller owns one immutable PropertyTrees snapshot for this entire closure.
pub(super) fn unseen_chain<K: Copy + Eq + Hash, S>(
    leaf: Option<K>,
    known: &FxHashMap<K, S>,
    mut fetch: impl FnMut(K) -> Option<S>,
    mut parent: impl FnMut(&S) -> Option<K>,
) -> Option<Vec<S>> {
    let mut result = Vec::new();
    let mut seen = FxHashSet::default();
    let mut cursor = leaf;
    while let Some(id) = cursor {
        if known.contains_key(&id) {
            break;
        }
        if !seen.insert(id) {
            return None;
        }
        let snapshot = fetch(id)?;
        cursor = parent(&snapshot);
        result.push(snapshot);
    }
    Some(result)
}

#[cfg(test)]
mod tests;
