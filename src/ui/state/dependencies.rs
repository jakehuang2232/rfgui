use super::*;

/// Allocation identity, never a pointer or component identity. Remounting a
/// slot allocates a new id even if its component key is reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct StateTargetId(u64);

thread_local! {
    static NEXT_TARGET: Cell<u64> = const { Cell::new(0) };
}

impl StateTargetId {
    pub(super) fn new() -> Self {
        NEXT_TARGET.with(|next| {
            let id = next
                .get()
                .checked_add(1)
                .expect("state target id exhausted");
            next.set(id);
            Self(id)
        })
    }
}

pub(super) struct ChangedState {
    pub target: StateTargetId,
    pub dirty: UiDirtyState,
    pub owner: Option<ComponentKey>,
}

impl StateStore {
    // Return retired user-owned props/nodes to drop outside the store borrow.
    pub(super) fn remove_memo(&mut self, key: &ComponentKey) -> Option<MemoEntry> {
        self.dirty_memo_components.remove(key);
        let entry = self.memo_cache.remove(key)?;
        for target in &entry.state_dependencies {
            remove_consumer(&mut self.target_consumers, target, key);
        }
        for component in entry.live_keys.iter().chain(std::iter::once(key)) {
            remove_consumer(&mut self.component_memos, component, key);
        }
        Some(entry)
    }

    pub(super) fn replace_memo(
        &mut self,
        key: ComponentKey,
        entry: MemoEntry,
    ) -> Option<MemoEntry> {
        let retired = self.remove_memo(&key);
        for target in &entry.state_dependencies {
            self.target_consumers
                .entry(*target)
                .or_default()
                .insert(key.clone());
        }
        // Resolved cached output includes descendant owners, even when they
        // didn't read their own state. Those ancestors must be re-entered.
        for component in entry.live_keys.iter().chain(std::iter::once(&key)) {
            self.component_memos
                .entry(component.clone())
                .or_default()
                .insert(key.clone());
        }
        self.memo_cache.insert(key, entry);
        retired
    }

    pub(super) fn collect_owner_memos(
        &self,
        owner: &ComponentKey,
        affected: &mut FxHashSet<ComponentKey>,
    ) {
        if let Some(memos) = self.component_memos.get(owner) {
            affected.extend(memos.iter().cloned());
        }
    }

    pub(super) fn invalidate_memos(&mut self, affected: FxHashSet<ComponentKey>) {
        let visits = affected.len();
        let mut invalidations = 0;
        for key in affected {
            invalidations += usize::from(self.dirty_memo_components.insert(key));
        }
        crate::ui::work_profile::count(|p| {
            p.memo_invalidation_visits += visits;
            p.memo_invalidations += invalidations;
        });
    }
}

fn remove_consumer<K: Eq + Hash>(
    index: &mut FxHashMap<K, FxHashSet<ComponentKey>>,
    source: &K,
    consumer: &ComponentKey,
) {
    if let Some(consumers) = index.get_mut(source) {
        consumers.remove(consumer);
        if consumers.is_empty() {
            index.remove(source);
        }
    }
}

pub(super) fn publish_changes(changes: &[ChangedState]) {
    if changes.is_empty() {
        return;
    }
    let dirty = changes
        .iter()
        .fold(UiDirtyState::NONE, |all, c| all.union(c.dirty));
    STATE_DIRTY.with(|state| state.set(state.get().union(dirty)));
    if dirty.needs_rebuild() {
        STORE.with(|store| {
            let mut store = store.borrow_mut();
            let mut affected = FxHashSet::default();
            let mut owners = FxHashSet::default();
            for change in changes.iter().filter(|c| c.dirty.needs_rebuild()) {
                if let Some(consumers) = store.target_consumers.get(&change.target) {
                    affected.extend(consumers.iter().cloned());
                }
                if let Some(owner) = &change.owner {
                    owners.insert(owner);
                }
            }
            for owner in owners {
                store.collect_owner_memos(owner, &mut affected);
            }
            store.invalidate_memos(affected);
        });
    }
}

#[cfg(test)]
mod tests;
