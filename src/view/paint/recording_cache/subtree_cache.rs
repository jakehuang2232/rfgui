//! Closed native subtrees can retain their successful metadata schedule.
//! Mutable access covers owned fields; immutable-color contracts close external
//! inputs. Current complete property snapshots are compared independently.
//! Custom/resource hosts, volatile colors and external spatial references keep
//! the live walk. This changes observation work, never renderer admission.
use super::super::coverage_manifest::{CoverageOrder, PaintOwnerScope};
use super::*;
use crate::view::base_component::{Element, SpatialPositionReferenceSnapshot, Text};
use crate::view::compositor::property_tree::*;

#[derive(Default)]
pub(crate) struct SubtreeCache {
    pub(crate) hits: usize,
    arena: Option<Arc<()>>,
    index_revision: Option<u64>,
    roots: Vec<NodeKey>,
    local_validation_clock: Option<(u64, u64)>,
    local_validations: FxHashMap<NodeKey, bool>,
    proofs: FxHashMap<NodeKey, (u64, Option<Arc<[NodeKey]>>)>,
    entries: FxHashMap<
        NodeKey,
        (
            Key,
            Arc<Snapshot>,
            bool,
            PropertyStoreStamp,
            crate::view::compositor::property_tree::observed_map::Stamp,
        ),
    >,
    // A repeatedly changing parent must not prevent its stable children from
    // retaining their own recordings. This affects cache granularity only.
    split: FxHashMap<NodeKey, Key>,
}
#[derive(Clone, PartialEq)]
pub(crate) struct Key {
    revision: u64,
    index_revision: u64,
    ancestors: Vec<AncestorInput>,
    context: PaintRecordingContext,
    offset: [u32; 2],
    order: CoverageOrder,
    deferred: bool,
    subtree_len: usize,
}
/// Native ancestor hooks have already produced this frame's complete metadata.
/// Keep exact data, not a signature hash. Missing observations retain the
/// conservative mutation dependency (for example outside-frame ancestors).
#[derive(Clone)]
enum AncestorInput {
    Recorded {
        owner: NodeKey,
        stable: u64,
        plan: Arc<PaintNodePlan<PaintChunkMetadata>>,
    },
    Unobserved {
        owner: NodeKey,
        revision: u64,
    },
}
impl PartialEq for AncestorInput {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Recorded {
                    owner: a,
                    stable: sa,
                    plan: pa,
                },
                Self::Recorded {
                    owner: b,
                    stable: sb,
                    plan: pb,
                },
            ) => a == b && sa == sb && (Arc::ptr_eq(pa, pb) || metadata_eq(pa, pb)),
            (
                Self::Unobserved {
                    owner: a,
                    revision: ra,
                },
                Self::Unobserved {
                    owner: b,
                    revision: rb,
                },
            ) => a == b && ra == rb,
            _ => false,
        }
    }
}

#[derive(Clone, PartialEq)]
struct Properties {
    state: Option<NodePropertyState>,
    transform: Option<TransformNodeSnapshot>,
    position: Option<LayoutPositionNodeSnapshot>,
    visual: Option<VisualOffsetNodeSnapshot>,
    effect: Option<EffectNodeSnapshot>,
    scroll: Option<ScrollNodeSnapshot>,
    clips: [Option<ClipNodeSnapshot>; 2],
}
impl Properties {
    fn observe(trees: &PropertyTrees, key: NodeKey) -> Self {
        Self {
            state: trees.node_state_for(key),
            transform: trees.transform_snapshot_for(TransformNodeId(key)),
            position: trees.layout_position_snapshot_for(LayoutPositionNodeId(key)),
            visual: trees.visual_offset_snapshot_for(VisualOffsetNodeId(key)),
            effect: trees.effect_node_snapshot_for(EffectNodeId(key)),
            scroll: trees.scroll_snapshot_for(ScrollNodeId(key)),
            clips: [ClipNodeRole::SelfClip, ClipNodeRole::ContentsClip]
                .map(|role| trees.clip_node_snapshot_for(ClipNodeId { owner: key, role })),
        }
    }

    fn dependencies(&self) -> impl Iterator<Item = NodeKey> {
        let mut owners = Vec::new();
        for state in self
            .state
            .into_iter()
            .flat_map(|state| [state.paint, state.descendants])
        {
            owners.extend(
                [
                    state.transform.map(|id| id.0),
                    state.clip.map(|id| id.owner),
                    state.effect.map(|id| id.0),
                    state.scroll.map(|id| id.0),
                    state.layout_position.map(|id| id.0),
                    state.visual_offset.map(|id| id.0),
                ]
                .into_iter()
                .flatten(),
            );
        }
        owners.extend(self.transform.and_then(|node| node.parent).map(|id| id.0));
        owners.extend(self.visual.and_then(|node| node.parent).map(|id| id.0));
        owners.extend(self.effect.and_then(|node| node.parent).map(|id| id.0));
        owners.extend(self.scroll.and_then(|node| node.parent).map(|id| id.0));
        for clip in self.clips.iter().flatten() {
            owners.extend(clip.parent.map(|id| id.owner));
        }
        if let Some(position) = self.position {
            owners.extend(position.reference_scroll.map(|id| id.0));
            match position.reference {
                SpatialPositionReference::LayoutParent(Some(owner))
                | SpatialPositionReference::Anchor(owner) => owners.push(owner),
                _ => {}
            }
        }
        owners.into_iter()
    }
}

fn property_closure(trees: &PropertyTrees, members: &[NodeKey]) -> Vec<(NodeKey, Properties)> {
    let mut pending = members.to_vec();
    let mut seen = rustc_hash::FxHashSet::default();
    let mut snapshots = Vec::new();
    while let Some(owner) = pending.pop() {
        if !seen.insert(owner) {
            continue;
        }
        let properties = Properties::observe(trees, owner);
        pending.extend(
            properties
                .dependencies()
                .filter(|owner| !seen.contains(owner)),
        );
        snapshots.push((owner, properties));
    }
    snapshots
}
pub(crate) struct Snapshot {
    owner: NodeKey,
    key: Key,
    completed: std::sync::OnceLock<Arc<[PaintCoverageItem]>>,
    pub(crate) items: Arc<[PaintCoverageItem]>,
    pub(crate) states: Vec<(NodeKey, super::super::PaintOwnerPropertyStateSnapshot)>,
    pub(crate) scopes: Vec<(NodeKey, Arc<PaintOwnerScope>)>,
    metadata: Vec<(NodeKey, u64, Arc<PaintNodePlan<PaintChunkMetadata>>)>,
    properties: FxHashMap<NodeKey, Properties>,
    generations: FxHashMap<
        NodeKey,
        Option<crate::view::compositor::paint_generation::LocalPaintGenerations>,
    >,
}
fn tracked_local(arena: &NodeArena, key: NodeKey) -> bool {
    let Some(node) = arena.get(key) else {
        return false;
    };
    let host = node.element.as_any();
    let tracked = host
        .downcast_ref::<Element>()
        .is_some_and(Element::paint_signature_inputs_are_tracked)
        || host
            .downcast_ref::<Text>()
            .is_some_and(Text::paint_signature_inputs_are_tracked);
    tracked
        // Nonzero native IDs must still resolve to their own tracked storage.
        // An opaque alias outside this subtree could otherwise change a lookup
        // without mutating any member. The index revision protects this proof
        // when the mapping itself changes between observations.
        && (node.element.stable_id() == 0
            || arena.find_by_stable_id(node.element.stable_id()) == Some(key))
        && node.children() == node.element.children()
        && node
            .children()
            .iter()
            .all(|child| arena.parent_of(*child) == Some(key))
        && !node
            .element
            .compositor_spatial_placement_snapshot()
            .is_some_and(|snapshot| {
                matches!(
                    snapshot.reference(),
                    SpatialPositionReferenceSnapshot::Anchor(_)
                        | SpatialPositionReferenceSnapshot::LayoutParent(Some(_))
                )
            })
}
impl SubtreeCache {
    pub(super) fn begin(&mut self) {
        self.hits = 0;
        for (_, _, seen, _, _) in self.entries.values_mut() {
            *seen = false;
        }
    }
    pub(crate) fn bind(&mut self, arena: &NodeArena, roots: &[NodeKey]) {
        let identity = arena.mutation_identity();
        let index_revision = arena.stable_id_index_revision();
        if self
            .arena
            .as_ref()
            .is_none_or(|old| !Arc::ptr_eq(old, &identity))
            || self.roots != roots
            || index_revision.is_none()
            || self.index_revision != index_revision
        {
            self.local_validations.clear();
            self.local_validation_clock = None;
            self.entries.clear();
            self.proofs.clear();
            self.split.clear();
            self.arena = Some(identity);
            self.index_revision = index_revision;
            self.roots = roots.to_vec();
        }
        self.proofs.retain(|key, _| arena.contains_key(*key));
        self.split.retain(|key, _| arena.contains_key(*key));
    }
    fn tracked_local(&mut self, arena: &NodeArena, owner: NodeKey) -> bool {
        let clock = arena.mutation_clock();
        let Some(index) = arena.stable_id_index_revision() else {
            return false;
        };
        if clock == u64::MAX {
            return false;
        }
        let stamp = (clock, index);
        if self.local_validation_clock != Some(stamp) {
            self.local_validations.clear();
            self.local_validation_clock = Some(stamp);
        }
        *self
            .local_validations
            .entry(owner)
            .or_insert_with(|| tracked_local(arena, owner))
    }
    fn closed(&mut self, arena: &NodeArena, key: NodeKey, depth: usize) -> Option<Arc<[NodeKey]>> {
        if depth > 128 {
            return None;
        }
        let revision = arena.subtree_mutation_revision(key)?;
        if let Some((old, members)) = self.proofs.get(&key) {
            if *old == revision {
                return members.clone();
            }
        }
        let members = (|| {
            if !self.tracked_local(arena, key) {
                return None;
            }
            let node = arena.get(key)?;
            let mut members = vec![key];
            for child in node.children() {
                members.extend(self.closed(arena, *child, depth + 1)?.iter().copied());
            }
            Some(Arc::from(members))
        })();
        self.proofs.insert(key, (revision, members.clone()));
        members
    }
    pub(crate) fn key(
        &mut self,
        arena: &NodeArena,
        owner: NodeKey,
        context: PaintRecordingContext,
        order: CoverageOrder,
        deferred: bool,
        metadata: &FxHashMap<NodeKey, (u64, Arc<PaintNodePlan<PaintChunkMetadata>>)>,
    ) -> Option<(Key, Arc<[NodeKey]>)> {
        let _profile = super::super::work_profile::scope("subtree_recording_key");
        let revision = arena.subtree_mutation_revision(owner)?;
        if let Some(previous) = self.split.get_mut(&owner)
            && previous.revision != revision
        {
            // A split parent that keeps changing cannot coalesce this frame.
            // Avoid rebuilding its closed-subtree/ancestor proof just to reject
            // it below. This only declines a cache probe: the live recorder
            // still visits the parent and can replay stable children. Once the
            // revision settles, all other key inputs are observed again before
            // capture; stale context here can only delay coalescing.
            previous.revision = revision;
            super::super::work_profile::count("subtree_probe_mutation_skips", 1);
            return None;
        }
        // ScrollNodeSnapshot compares every float by bits, including the
        // sampled overlay. Context equality therefore preserves the complete
        // parent scroll input as well as our separate paint-offset bit key.
        let members = self.closed(arena, owner, 0)?;
        let subtree_len = members.len();
        let mut ancestors = Vec::new();
        let mut parent = arena.parent_of(owner);
        let mut depth = 0;
        while let Some(key) = parent {
            depth += 1;
            if depth > 128 || !self.tracked_local(arena, key) {
                return None;
            }
            ancestors.push(match metadata.get(&key) {
                Some((stable, plan)) => AncestorInput::Recorded {
                    owner: key,
                    stable: *stable,
                    plan: plan.clone(),
                },
                None => AncestorInput::Unobserved {
                    owner: key,
                    revision: arena.mutation_revision(key)?,
                },
            });
            parent = arena.parent_of(key);
        }
        let key = Key {
            revision,
            index_revision: arena.stable_id_index_revision()?,
            ancestors,
            context,
            offset: context.paint_offset.map(f32::to_bits),
            order,
            deferred,
            subtree_len,
        };
        if let Some(previous) = self.split.get_mut(&owner) {
            if *previous != key {
                if previous.revision == key.revision && previous.ancestors != key.ancestors {
                    super::super::work_profile::count("subtree_split_ancestor_input_changes", 1);
                }
                *previous = key;
                return None;
            }
            // A settled parent can coalesce its children again. Startup layout
            // changes must not permanently fragment an otherwise static scene.
            self.split.remove(&owner);
        }
        Some((key, members))
    }
    pub(crate) fn replay(
        &mut self,
        owner: NodeKey,
        key: &Key,
        trees: &PropertyTrees,
        generations: &crate::view::compositor::PaintGenerationTracker,
    ) -> Option<Arc<Snapshot>> {
        let _profile = super::super::work_profile::scope("subtree_recording_validate");
        let (old, snapshot, seen, stamp, generation_stamp) = self.entries.get_mut(&owner)?;
        if old.revision == key.revision && old.ancestors != key.ancestors {
            super::super::work_profile::count("subtree_replay_ancestor_input_changes", 1);
        }
        if old != key {
            self.split.insert(owner, key.clone());
            return None;
        }
        let written = trees.property_writes_since(stamp);
        let properties_match = match &written {
            Some(owners) if owners.len() < snapshot.properties.len() => {
                owners.iter().all(|owner| {
                    snapshot
                        .properties
                        .get(owner)
                        .is_none_or(|old| *old == Properties::observe(trees, *owner))
                })
            }
            _ => snapshot.properties.iter().all(|(owner, old)| {
                written.as_ref().is_some_and(|keys| !keys.contains(owner))
                    || *old == Properties::observe(trees, *owner)
            }),
        };
        let generation_writes = generations.generation_writes_since(generation_stamp);
        let generations_match = match &generation_writes {
            Some(owners) if owners.len() < snapshot.generations.len() => {
                owners.iter().all(|owner| {
                    snapshot
                        .generations
                        .get(owner)
                        .is_none_or(|old| *old == generations.local_generations_for(*owner))
                })
            }
            _ => snapshot
                .generations
                .iter()
                .all(|(owner, old)| *old == generations.local_generations_for(*owner)),
        };
        if !properties_match || !generations_match {
            self.split.insert(owner, key.clone());
            return None;
        }
        *seen = true;
        *stamp = trees.property_store_stamp();
        *generation_stamp = generations.generation_store_stamp();
        self.hits += 1;
        Some(snapshot.clone())
    }
    pub(crate) fn should_capture(&self, owner: NodeKey) -> bool {
        !self.split.contains_key(&owner)
    }
    pub(super) fn finish(&mut self, accepted: bool) {
        self.entries
            .retain(|_, (_, _, seen, _, _)| accepted && *seen);
        if !accepted {
            self.local_validations.clear();
            self.local_validation_clock = None;
            self.proofs.clear();
            self.split.clear();
        }
    }
}
impl Snapshot {
    pub(super) fn completed_commands(
        &self,
        arena: &NodeArena,
        observed_clock: u64,
    ) -> Option<Arc<[PaintCoverageItem]>> {
        // The current attempt has already compared complete native metadata,
        // property dependencies and generation observations. This stamp belongs
        // to that replay, not the historical snapshot: any intervening arena
        // mutation rejects the completed block before it can be published.
        if observed_clock == u64::MAX
            || arena.mutation_clock() != observed_clock
            || arena.subtree_mutation_revision(self.owner) != Some(self.key.revision)
            || arena.stable_id_index_revision() != Some(self.key.index_revision)
        {
            return None;
        }
        self.completed.get().cloned()
    }
    pub(super) fn remember_completed_commands(&self, commands: Arc<[PaintCoverageItem]>) {
        // Called after the native command hook's exact metadata comparison and
        // complete replacement check. Rejection of the frame drops this proof.
        let _ = self.completed.set(commands);
    }
}
impl RecordingCache {
    pub(crate) fn subtree_key(
        &mut self,
        arena: &NodeArena,
        owner: NodeKey,
        context: PaintRecordingContext,
        order: CoverageOrder,
        deferred: bool,
    ) -> Option<(Key, Arc<[NodeKey]>)> {
        self.subtrees
            .key(arena, owner, context, order, deferred, &self.metadata)
    }

    pub(crate) fn restore_subtree(&mut self, snapshot: &Arc<Snapshot>, arena: &NodeArena) {
        let _profile = super::super::work_profile::scope("subtree_recording_restore");
        self.replayed_subtrees.insert(
            Arc::as_ptr(&snapshot.items) as *const () as usize,
            (snapshot.clone(), arena.mutation_clock()),
        );
        for (owner, stable, plan) in &snapshot.metadata {
            self.metadata.insert(*owner, (*stable, plan.clone()));
        }
        for (owner, scope) in &snapshot.scopes {
            if self
                .scopes
                .get(owner)
                .is_some_and(|(old, _)| Arc::ptr_eq(old, scope))
            {
                self.scope_hits += 1;
            }
            self.scopes.insert(*owner, (scope.clone(), true));
            if let Some((_, seen)) = self.order_paths.get_mut(owner) {
                *seen = true;
            }
        }
    }
    pub(super) fn mark_completed_subtree_seen(&mut self, snapshot: &Snapshot) {
        for (owner, _, plan) in &snapshot.metadata {
            if plan.before_children.is_empty() && plan.after_children.is_empty() {
                continue;
            }
            if let Some(entry) = self.entries.get_mut(owner) {
                entry.seen = true;
            }
            self.hits += 1;
        }
    }
    pub(crate) fn remember_subtree(
        &mut self,
        owner: NodeKey,
        key: Key,
        arena: &NodeArena,
        members: &[NodeKey],
        items: &[PaintCoverageItem],
        states: &FxHashMap<NodeKey, super::super::PaintOwnerPropertyStateSnapshot>,
        scopes: &FxHashMap<NodeKey, Arc<PaintOwnerScope>>,
        trees: &PropertyTrees,
        generations: &crate::view::compositor::PaintGenerationTracker,
    ) {
        let _profile = super::super::work_profile::scope("subtree_recording_store");
        if items.is_empty()
            || items.iter().any(|item| {
                !matches!(
                    item,
                    PaintCoverageItem::ArtifactChunk { .. }
                        | PaintCoverageItem::TransparentNode { .. }
                        | PaintCoverageItem::CulledSubtree { .. }
                )
            })
        {
            return;
        }
        // Keep the shared membership block on hits. Ancestral generation
        // dependencies are collected only when minting a new snapshot.
        let mut dependencies = members.to_vec();
        let mut parent = arena.parent_of(owner);
        for _ in 0..128 {
            let Some(key) = parent else { break };
            dependencies.push(key);
            parent = arena.parent_of(key);
        }
        let snapshot = Snapshot {
            owner,
            key: key.clone(),
            completed: Default::default(),
            items: items.into(),
            states: members
                .iter()
                .take(key.subtree_len)
                .filter_map(|key| states.get(key).map(|state| (*key, *state)))
                .collect(),
            scopes: members
                .iter()
                .take(key.subtree_len)
                .filter_map(|key| scopes.get(key).map(|scope| (*key, scope.clone())))
                .collect(),
            metadata: members
                .iter()
                .take(key.subtree_len)
                .filter_map(|key| {
                    self.metadata
                        .get(key)
                        .map(|(stable, plan)| (*key, *stable, plan.clone()))
                })
                .collect(),
            properties: property_closure(trees, &dependencies).into_iter().collect(),
            generations: dependencies
                .iter()
                .map(|key| (*key, generations.local_generations_for(*key)))
                .collect(),
        };
        self.subtrees.entries.insert(
            owner,
            (
                key,
                Arc::new(snapshot),
                true,
                trees.property_store_stamp(),
                generations.generation_store_stamp(),
            ),
        );
    }
}

#[cfg(test)]
mod tests;
