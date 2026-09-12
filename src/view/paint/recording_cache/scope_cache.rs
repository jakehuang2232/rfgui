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
    effect_users: FxHashMap<
        crate::view::compositor::property_tree::EffectNodeId,
        rustc_hash::FxHashSet<NodeKey>,
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
            let Some(updates) = store.effect_updates(manifest) else {
                return false;
            };
            // Key shape, owner/clip stores and first-encounter order are fixed.
            // Only proven consistent current scalar effect observations change.
            for effect in &mut store.effects {
                if let Some(current) = updates.get(&effect.id) {
                    *effect = *current;
                }
            }
            self.scope_store_effect_updates += updates.len();
            // Keep unchanged strong references in place. Rebuilding this key
            // would clone/drop three Arcs per chunk for a two-owner animation.
            for ((owner, clip, effect), (now_owner, now_clip, now_effect)) in
                store.key.0.iter_mut().zip(observations(manifest))
            {
                if !Arc::ptr_eq(owner, now_owner) {
                    *owner = now_owner.clone();
                }
                if !Arc::ptr_eq(clip, now_clip) {
                    *clip = now_clip.clone();
                }
                if !Arc::ptr_eq(effect, now_effect) {
                    *effect = now_effect.clone();
                }
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
        let mut effect_users = FxHashMap::default();
        let mut seen = rustc_hash::FxHashSet::default();
        for (owner, _, _) in &key.0 {
            let mut cursor = Some(owner);
            while let Some(scope) = cursor {
                if !seen.insert(Arc::as_ptr(scope)) {
                    break;
                }
                for effect in scope.effects.iter().flat_map(|chain| chain.iter()) {
                    effect_users
                        .entry(effect.id)
                        .or_insert_with(rustc_hash::FxHashSet::default)
                        .insert(scope.topology.owner);
                }
                cursor = scope.parent.as_ref();
            }
        }
        self.scope_store = Some(ScopedSnapshotStore {
            key,
            effect_users,
            owners: artifact.owner_nodes.clone(),
            states: artifact.owner_property_states.clone(),
            clips: artifact.clip_nodes.clone(),
            effects: artifact.effect_nodes.clone(),
        });
    }
}

impl ScopedSnapshotStore {
    fn effect_updates(
        &self,
        manifest: &PaintCoverageManifest,
    ) -> Option<FxHashMap<crate::view::compositor::property_tree::EffectNodeId, EffectNodeSnapshot>>
    {
        let mut updates = FxHashMap::default();
        let mut changed_owners = rustc_hash::FxHashSet::default();
        let mut current = observations(manifest);
        let mut compared = rustc_hash::FxHashSet::default();
        for (old, old_clip, old_effect) in &self.key.0 {
            let (now, clip, effect) = current.next()?;
            if Arc::ptr_eq(old, now)
                && Arc::ptr_eq(old_clip, clip)
                && Arc::ptr_eq(old_effect, effect)
            {
                continue;
            }
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
                            changed_owners.insert(after.topology.owner);
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
        // Every previous consumer of a changed effect must have supplied a
        // changed current scope. Otherwise this new value conflicts with an
        // unchanged observation (including a transparent ancestor) and the
        // original merge must diagnose it. Changed edges use that full merge.
        if updates.keys().any(|id| {
            self.effect_users
                .get(id)
                .is_none_or(|users| !users.is_subset(&changed_owners))
        }) {
            return None;
        }
        // Owner membership alone is insufficient: the same owner may appear
        // in several chunks, or change a different effect in its chain. Check
        // every current occurrence of each patched id against its new value.
        let mut inspected = rustc_hash::FxHashSet::default();
        for (owner, _, effect) in observations(manifest) {
            if effect.iter().any(|snapshot| {
                updates
                    .get(&snapshot.id)
                    .is_some_and(|expected| expected != snapshot)
            }) {
                return None;
            }
            let mut cursor = Some(owner);
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
        Some(updates)
    }
}

#[cfg(test)]
mod tests;
