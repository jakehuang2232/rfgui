use super::*;
use crate::view::compositor::property_tree::{ClipNodeSnapshot, EffectNodeSnapshot};
use crate::view::paint::coverage_manifest::PaintOwnerScope;
use crate::view::paint::{PaintOwnerPropertyStateSnapshot, PaintOwnerSnapshot};

// Strong references certify immutable observations, never arena addresses.
// The ordered key determines the original merge's first-encounter store order.
pub(in super::super) struct ScopeStoreKey(
    Vec<(
        Arc<PaintOwnerScope>,
        Arc<[ClipNodeSnapshot]>,
        Arc<[EffectNodeSnapshot]>,
    )>,
);
pub(super) struct ScopedSnapshotStore {
    key: ScopeStoreKey,
    owners: Vec<PaintOwnerSnapshot>,
    states: Vec<PaintOwnerPropertyStateSnapshot>,
    clips: Vec<ClipNodeSnapshot>,
    effects: Vec<EffectNodeSnapshot>,
    effect_observations: FxHashMap<
        crate::view::compositor::property_tree::EffectNodeId,
        rustc_hash::FxHashSet<usize>,
    >,
}

fn observations(
    manifest: &PaintCoverageManifest,
) -> impl Iterator<
    Item = (
        &Arc<PaintOwnerScope>,
        &Arc<[ClipNodeSnapshot]>,
        &Arc<[EffectNodeSnapshot]>,
    ),
> {
    manifest.items.iter().filter_map(|item| match item {
        PaintCoverageItem::ArtifactChunk {
            owner_scope,
            clip_snapshot,
            effect_snapshot,
            ..
        } => Some((owner_scope, clip_snapshot, effect_snapshot)),
        _ => None,
    })
}
impl RecordingCache {
    pub(in super::super) fn replay_scope_store(
        &mut self,
        manifest: &PaintCoverageManifest,
        artifact: &mut PaintArtifact,
    ) -> bool {
        let _profile = super::super::work_profile::scope("replay_scope_store");
        let Some(store) = &mut self.scope_store else {
            return false;
        };
        let unchanged = {
            let mut current = observations(manifest);
            store.key.0.iter().all(|(owner, clip, effect)| {
                current.next().is_some_and(|(a, b, c)| {
                    Arc::ptr_eq(owner, a) && Arc::ptr_eq(clip, b) && Arc::ptr_eq(effect, c)
                })
            }) && current.next().is_none()
        };
        if !unchanged {
            let Some((updates, changed)) = store.effect_updates(manifest) else {
                return false;
            };
            for effect in &mut store.effects {
                if let Some(current) = updates.get(&effect.id) {
                    *effect = *current;
                }
            }
            self.scope_store_effect_updates += updates.len();
            for (index, now_owner, now_clip, now_effect) in changed {
                store.key.0[index] = (now_owner.clone(), now_clip.clone(), now_effect.clone());
            }
        }
        artifact.owner_nodes.clone_from(&store.owners);
        artifact.owner_property_states.clone_from(&store.states);
        artifact.clip_nodes.clone_from(&store.clips);
        artifact.effect_nodes.clone_from(&store.effects);
        self.scope_store_hits += 1;
        true
    }
    pub(in super::super) fn scope_store_key(manifest: &PaintCoverageManifest) -> ScopeStoreKey {
        ScopeStoreKey(
            observations(manifest)
                .map(|(a, b, c)| (a.clone(), b.clone(), c.clone()))
                .collect(),
        )
    }
    pub(in super::super) fn remember_scope_store(
        &mut self,
        key: ScopeStoreKey,
        artifact: &PaintArtifact,
    ) {
        // Called only after every observation merged without conflict. Spatial
        // closure still reads this frame's PropertyTrees after materialization.
        let mut effect_observations = FxHashMap::default();
        for (index, (owner, _, standalone)) in key.0.iter().enumerate() {
            for effect in standalone.iter() {
                effect_observations
                    .entry(effect.id)
                    .or_insert_with(rustc_hash::FxHashSet::default)
                    .insert(index);
            }
            let mut cursor = Some(owner);
            while let Some(scope) = cursor {
                for effect in scope.effects.iter().flat_map(|chain| chain.iter()) {
                    effect_observations
                        .entry(effect.id)
                        .or_insert_with(rustc_hash::FxHashSet::default)
                        .insert(index);
                }
                cursor = scope.parent.as_ref();
            }
        }
        self.scope_store = Some(ScopedSnapshotStore {
            key,
            effect_observations,
            owners: artifact.owner_nodes.clone(),
            states: artifact.owner_property_states.clone(),
            clips: artifact.clip_nodes.clone(),
            effects: artifact.effect_nodes.clone(),
        });
    }
}

type ChangedObservation<'a> = (
    usize,
    &'a Arc<PaintOwnerScope>,
    &'a Arc<[ClipNodeSnapshot]>,
    &'a Arc<[EffectNodeSnapshot]>,
);

impl ScopedSnapshotStore {
    fn effect_updates<'a>(
        &self,
        manifest: &'a PaintCoverageManifest,
    ) -> Option<(
        FxHashMap<crate::view::compositor::property_tree::EffectNodeId, EffectNodeSnapshot>,
        Vec<ChangedObservation<'a>>,
    )> {
        let mut updates = FxHashMap::default();
        let mut changed = Vec::new();
        let mut current = observations(manifest);
        let mut compared = rustc_hash::FxHashSet::default();
        for (index, (old, old_clip, old_effect)) in self.key.0.iter().enumerate() {
            let (now, clip, effect) = current.next()?;
            if Arc::ptr_eq(old, now)
                && Arc::ptr_eq(old_clip, clip)
                && Arc::ptr_eq(old_effect, effect)
            {
                continue;
            }
            changed.push((index, now, clip, effect));
            // Standalone observations must still bind to the owner endpoint.
            if old_clip != clip
                || !(0..2).any(|i| old.effects[i] == *old_effect && now.effects[i] == *effect)
            {
                return None;
            }
            let mut pair = Some((old, now));
            while let Some((before, after)) = pair {
                if Arc::ptr_eq(before, after)
                    || !compared.insert((Arc::as_ptr(before), Arc::as_ptr(after)))
                {
                    break;
                }
                if before.topology != after.topology
                    || before.state != after.state
                    || before.clips != after.clips
                {
                    return None;
                }
                for (before_chain, after_chain) in before.effects.iter().zip(&after.effects) {
                    if before_chain.len() != after_chain.len() {
                        return None;
                    }
                    for (a, b) in before_chain.iter().zip(after_chain.iter()) {
                        if a.id != b.id
                            || a.owner != b.owner
                            || a.parent != b.parent
                            || !b.opacity.is_finite()
                            || b.generation == 0
                        {
                            return None;
                        }
                        if a != b {
                            if updates
                                .insert(b.id, *b)
                                .is_some_and(|previous| previous != *b)
                            {
                                return None;
                            }
                        }
                    }
                }
                // A new parent allocation is not necessarily a new edge: an
                // ancestor's scalar effect update replaces its immutable scope.
                // Compare that scope too, including transparent ancestors. The
                // live recorder has already bounded and validated these chains.
                pair = match (&before.parent, &after.parent) {
                    (None, None) => None,
                    (Some(a), Some(b)) => Some((a, b)),
                    _ => return None,
                };
            }
        }
        if current.next().is_some() {
            return None;
        }
        // The original merge recorded every occurrence, not just owner ids.
        // Each updated effect must replace all old observations that consumed it.
        // This catches two chunks of one owner supplying conflicting snapshots,
        // without rescanning every unchanged scope chain on each animation tick.
        let changed_indices = changed
            .iter()
            .map(|(index, ..)| *index)
            .collect::<rustc_hash::FxHashSet<_>>();
        if updates.keys().any(|id| {
            self.effect_observations
                .get(id)
                .is_none_or(|indices| !indices.is_subset(&changed_indices))
        }) {
            return None;
        }
        let mut inspected = rustc_hash::FxHashSet::default();
        for (_, owner, _, effect) in &changed {
            if effect.iter().any(|snapshot| {
                updates
                    .get(&snapshot.id)
                    .is_some_and(|expected| expected != snapshot)
            }) {
                return None;
            }
            let mut cursor = Some(*owner);
            while let Some(scope) = cursor {
                if !inspected.insert(Arc::as_ptr(scope)) {
                    break;
                }
                if scope
                    .effects
                    .iter()
                    .flat_map(|chain| chain.iter())
                    .any(|snapshot| {
                        updates
                            .get(&snapshot.id)
                            .is_some_and(|expected| expected != snapshot)
                    })
                {
                    return None;
                }
                cursor = scope.parent.as_ref();
            }
        }
        Some((updates, changed))
    }
}

#[cfg(test)]
mod tests;
