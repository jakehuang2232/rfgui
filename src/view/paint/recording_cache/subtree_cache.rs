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
            Arc<PropertyStoreStamp>,
            crate::view::compositor::property_tree::observed_map::Stamp,
        ),
    >,
    // A repeatedly changing parent must not prevent its stable children from
    // retaining their own recordings. This affects cache granularity only.
    split: FxHashMap<NodeKey, Key>,
    /// Owners outside the subtrees recorded this frame, such as a list
    /// shared by all its rows' recordings, observed once and shared.
    shared_properties: FxHashMap<NodeKey, Arc<Properties>>,
    /// Whether a shared recorded input still holds, checked once while the
    /// trees stay as `matched_stamp` says: every row of a scrolled list
    /// replays against the same list.
    matched_properties: FxHashMap<NodeKey, (Arc<Properties>, bool)>,
    matched_stamp: Option<Arc<PropertyStoreStamp>>,
    /// Counts frames, so a parent's ancestor inputs are read once a frame.
    epoch: u64,
    /// For each parent, its children's ancestor inputs as last read, with
    /// the frame and arena clock they were read at.
    ancestor_inputs: FxHashMap<NodeKey, ((u64, u64), Option<Ancestors>)>,
}
#[derive(Clone, PartialEq)]
pub(crate) struct Key {
    revision: u64,
    index_revision: u64,
    ancestors: Ancestors,
    context: PaintRecordingContext,
    order: CoverageOrder,
    deferred: bool,
    subtree_len: usize,
}
/// Native ancestor hooks have already produced this frame's complete metadata.
/// Keep exact data, not a signature hash. Missing observations retain the
/// conservative mutation dependency (for example outside-frame ancestors).
/// What an ancestor draws never reaches a subtree's recording, so only the
/// chunks it records and the frames and property states they paint in are
/// compared: a scroll container repainting its scrollbar keeps its rows.
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
            ) => a == b && sa == sb && (Arc::ptr_eq(pa, pb) || chunk_structure_eq(pa, pb)),
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

fn chunk_structure_eq(
    a: &PaintNodePlan<PaintChunkMetadata>,
    b: &PaintNodePlan<PaintChunkMetadata>,
) -> bool {
    [&a.before_children, &a.after_children]
        .into_iter()
        .zip([&b.before_children, &b.after_children])
        .all(|(a, b)| {
            a.len() == b.len()
                && a.iter().zip(b).all(|(a, b)| {
                    a.id == b.id
                        && a.owner == b.owner
                        && a.frame == b.frame
                        && a.properties == b.properties
                })
        })
}

/// The inputs of an owner's ancestors, nearest first. All children of a
/// parent share one list, kept from frame to frame while it holds.
#[derive(Clone)]
struct Ancestors(Arc<[AncestorInput]>);
impl PartialEq for Ancestors {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0[..] == other.0[..]
    }
}

/// One owner's property inputs as the trees hold them now.
#[derive(Clone, Copy, PartialEq)]
struct Observation {
    /// Whether the owner's layout-position and visual-offset edges, and
    /// where its contents are scrolled to, are part of this observation.
    /// They are for subtree members. Paint outside the subtree is recorded
    /// relative to layout frames at scroll zero, so moving or scrolling an
    /// ancestor or another dependency changes no recorded input; only
    /// viewport-frame chunks see a move, and those also depend on the
    /// subtree's translation.
    spatial: bool,
    state: Option<NodePropertyState>,
    transform: Option<TransformNodeSnapshot>,
    position: Option<LayoutPositionNodeSnapshot>,
    visual: Option<VisualOffsetNodeSnapshot>,
    effect: Option<EffectNodeSnapshot>,
    scroll: Option<ScrollNodeSnapshot>,
    clips: [Option<ClipNodeSnapshot>; 2],
}
impl Observation {
    fn observe(trees: &PropertyTrees, key: NodeKey, spatial: bool) -> Self {
        Self {
            spatial,
            state: trees.node_state_for(key),
            transform: trees.transform_snapshot_for(TransformNodeId(key)),
            position: spatial
                .then(|| trees.layout_position_snapshot_for(LayoutPositionNodeId(key)))
                .flatten(),
            visual: spatial
                .then(|| trees.visual_offset_snapshot_for(VisualOffsetNodeId(key)))
                .flatten(),
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

/// A recorded [`Observation`]. Transforms, effects, scrolls and clips belong
/// to few owners, so they are boxed: an owner without them stays small.
#[derive(PartialEq)]
struct Properties {
    spatial: bool,
    state: Option<NodePropertyState>,
    position: Option<LayoutPositionNodeSnapshot>,
    visual: Option<VisualOffsetNodeSnapshot>,
    transform: Option<Box<TransformNodeSnapshot>>,
    effect: Option<Box<EffectNodeSnapshot>>,
    scroll: Option<Box<ScrollNodeSnapshot>>,
    clips: [Option<Box<ClipNodeSnapshot>>; 2],
}
impl Properties {
    fn record(observed: &Observation) -> Self {
        Self {
            spatial: observed.spatial,
            state: observed.state,
            position: observed.position,
            visual: observed.visual,
            transform: observed.transform.map(Box::new),
            effect: observed.effect.map(Box::new),
            scroll: observed.scroll.map(Box::new),
            clips: observed.clips.map(|clip| clip.map(Box::new)),
        }
    }

    fn matches(&self, observed: &Observation) -> bool {
        self.spatial == observed.spatial
            && self.state == observed.state
            && self.position == observed.position
            && self.visual == observed.visual
            && self.transform.as_deref() == observed.transform.as_ref()
            && self.effect.as_deref() == observed.effect.as_ref()
            && match (self.scroll.as_deref(), observed.scroll.as_ref()) {
                (Some(old), Some(new)) if !self.spatial => old.eq_ignoring_scroll_position(new),
                (old, new) => old == new,
            }
            && self
                .clips
                .iter()
                .zip(&observed.clips)
                .all(|(clip, observed)| clip.as_deref() == observed.as_ref())
    }

    fn matches_trees(&self, trees: &PropertyTrees, owner: NodeKey) -> bool {
        self.matches(&Observation::observe(trees, owner, self.spatial))
    }
}

/// Property inputs of a subtree: its members with their spatial edges, then
/// every other owner they depend on (ancestors first) without them. Those
/// others come from `shared` when it holds their current inputs.
fn property_closure(
    trees: &PropertyTrees,
    members: &[NodeKey],
    dependencies: &[NodeKey],
    shared: &mut FxHashMap<NodeKey, Arc<Properties>>,
) -> Vec<(NodeKey, Arc<Properties>)> {
    let mut seen = rustc_hash::FxHashSet::default();
    let mut snapshots = Vec::new();
    let mut pending = Vec::new();
    for &member in members {
        if seen.insert(member) {
            let observed = Observation::observe(trees, member, true);
            pending.extend(observed.dependencies());
            snapshots.push((member, Arc::new(Properties::record(&observed))));
        }
    }
    pending.extend(dependencies.iter().rev().copied());
    while let Some(owner) = pending.pop() {
        if !seen.insert(owner) {
            continue;
        }
        let observed = Observation::observe(trees, owner, false);
        pending.extend(
            observed
                .dependencies()
                .filter(|owner| !seen.contains(owner)),
        );
        let properties = match shared.get(&owner) {
            Some(properties) if properties.matches(&observed) => properties.clone(),
            _ => {
                let properties = Arc::new(Properties::record(&observed));
                shared.insert(owner, properties.clone());
                properties
            }
        };
        snapshots.push((owner, properties));
    }
    snapshots
}
pub(crate) struct Snapshot {
    owner: NodeKey,
    key: Key,
    /// The subtree translation revision at capture when any recorded chunk
    /// stayed in viewport coordinates. Every other recorded input is relative
    /// to a layout frame and survives a translation of the subtree.
    viewport_translation: Option<u64>,
    completed: std::sync::OnceLock<Arc<[PaintCoverageItem]>>,
    pub(crate) items: Arc<[PaintCoverageItem]>,
    /// Members the capture left out with their subtrees, because their paint
    /// inputs were incomplete. The inputs below cover why, so a replay
    /// leaves them out again.
    pub(crate) left_out: Vec<NodeKey>,
    pub(crate) states: Vec<(NodeKey, super::super::PaintOwnerPropertyStateSnapshot)>,
    pub(crate) scopes: Vec<(NodeKey, Arc<PaintOwnerScope>)>,
    metadata: Vec<(NodeKey, u64, Arc<PaintNodePlan<PaintChunkMetadata>>)>,
    properties: OwnerMap<Arc<Properties>>,
    generations: OwnerMap<Option<crate::view::compositor::paint_generation::LocalPaintGenerations>>,
}

/// An owner-keyed map stored as a sorted slice. A snapshot is kept for every
/// recorded subtree and holds a handful of owners, where a hash table's empty
/// slots cost more memory than its lookups save.
struct OwnerMap<V>(Box<[(NodeKey, V)]>);

impl<V> OwnerMap<V> {
    fn new(entries: impl IntoIterator<Item = (NodeKey, V)>) -> Self {
        let mut entries = entries.into_iter().collect::<Vec<_>>();
        entries.sort_by_key(|(owner, _)| *owner);
        entries.dedup_by_key(|(owner, _)| *owner);
        let mut exact = Vec::with_capacity(entries.len());
        exact.extend(entries);
        Self(exact.into_boxed_slice())
    }

    fn get(&self, owner: &NodeKey) -> Option<&V> {
        self.0
            .binary_search_by_key(owner, |(key, _)| *key)
            .ok()
            .map(|index| &self.0[index].1)
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn iter(&self) -> impl Iterator<Item = (&NodeKey, &V)> {
        self.0.iter().map(|(owner, value)| (owner, value))
    }
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
        self.epoch += 1;
        self.matched_properties.clear();
        self.matched_stamp = None;
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
            self.ancestor_inputs.clear();
            self.arena = Some(identity);
            self.index_revision = index_revision;
            self.roots = roots.to_vec();
        }
        self.proofs.retain(|key, _| arena.contains_key(*key));
        self.split.retain(|key, _| arena.contains_key(*key));
        self.ancestor_inputs
            .retain(|key, _| arena.contains_key(*key));
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
    /// `root_index` and `path` name the owner's BeforeChildren coverage
    /// order. Both it and the context are copied into a key only after the
    /// cheap mutation checks pass; most probes of a changing tree stop there.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn key(
        &mut self,
        arena: &NodeArena,
        owner: NodeKey,
        context: &PaintRecordingContext,
        root_index: usize,
        path: Arc<[usize]>,
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
        // The context holds only what the parent hands down. The parent's
        // scroll input, apart from where it is scrolled to, is among the
        // snapshot's properties.
        let members = self.closed(arena, owner, 0)?;
        let subtree_len = members.len();
        let ancestors = self.ancestor_inputs(arena, arena.parent_of(owner), metadata)?;
        let key = Key {
            revision,
            index_revision: arena.stable_id_index_revision()?,
            ancestors,
            context: context.clone(),
            order: CoverageOrder {
                root_index,
                child_path: path,
                phase: super::super::PaintNodePhase::BeforeChildren,
                slot: 0,
            },
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
    /// The ancestor inputs of `parent`'s children: `parent` and every
    /// ancestor above it, read once a frame for all the children.
    fn ancestor_inputs(
        &mut self,
        arena: &NodeArena,
        parent: Option<NodeKey>,
        metadata: &FxHashMap<NodeKey, (u64, Arc<PaintNodePlan<PaintChunkMetadata>>)>,
    ) -> Option<Ancestors> {
        let Some(parent) = parent else {
            return Some(Ancestors(Arc::from([])));
        };
        let stamp = (self.epoch, arena.mutation_clock());
        if let Some((read_at, inputs)) = self.ancestor_inputs.get(&parent)
            && *read_at == stamp
        {
            return inputs.clone();
        }
        let read = (|| {
            let mut inputs = Vec::new();
            let mut cursor = Some(parent);
            while let Some(key) = cursor {
                if inputs.len() >= 128 || !self.tracked_local(arena, key) {
                    return None;
                }
                inputs.push(match metadata.get(&key) {
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
                cursor = arena.parent_of(key);
            }
            Some(inputs)
        })();
        let inputs = read.map(|inputs| match self.ancestor_inputs.get(&parent) {
            Some((_, Some(previous))) if previous.0[..] == inputs[..] => previous.clone(),
            _ => Ancestors(inputs.into()),
        });
        self.ancestor_inputs.insert(parent, (stamp, inputs.clone()));
        inputs
    }
    pub(crate) fn replay(
        &mut self,
        arena: &NodeArena,
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
        if old != key
            || snapshot
                .viewport_translation
                .is_some_and(|old| arena.subtree_translation_revision(owner) != Some(old))
        {
            self.split.insert(owner, key.clone());
            return None;
        }
        if !self
            .matched_stamp
            .as_deref()
            .is_some_and(|stamp| trees.property_store_is(stamp))
        {
            self.matched_properties.clear();
            self.matched_stamp = Some(Arc::new(trees.property_store_stamp()));
        }
        let now = self.matched_stamp.clone().unwrap();
        let matched = &mut self.matched_properties;
        let mut matches = |owner: NodeKey, old: &Arc<Properties>| match matched.get(&owner) {
            Some((checked, result)) if Arc::ptr_eq(checked, old) => *result,
            _ => {
                let result = old.matches_trees(trees, owner);
                matched.insert(owner, (old.clone(), result));
                result
            }
        };
        let written = trees.property_writes_since(stamp);
        let properties_match = match &written {
            Some(owners) if owners.len() < snapshot.properties.len() => {
                owners.iter().all(|owner| {
                    snapshot
                        .properties
                        .get(owner)
                        .is_none_or(|old| matches(*owner, old))
                })
            }
            _ => snapshot.properties.iter().all(|(owner, old)| {
                written.as_ref().is_some_and(|keys| !keys.contains(owner)) || matches(*owner, old)
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
            _ => snapshot.generations.iter().all(|(owner, old)| {
                generation_writes
                    .as_ref()
                    .is_some_and(|keys| !keys.contains(owner))
                    || *old == generations.local_generations_for(*owner)
            }),
        };
        if !properties_match || !generations_match {
            self.split.insert(owner, key.clone());
            return None;
        }
        *seen = true;
        *stamp = now;
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
        self.shared_properties.clear();
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
        context: &PaintRecordingContext,
        root_index: usize,
        path: &[usize],
        deferred: bool,
    ) -> Option<(Key, Arc<[NodeKey]>)> {
        let path = self.intern_order_path(owner, path);
        self.subtrees.key(
            arena,
            owner,
            context,
            root_index,
            path,
            deferred,
            &self.metadata,
        )
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
        left_out: Vec<NodeKey>,
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
        // Keep the shared membership block on hits. Ancestral property
        // dependencies are collected only when minting a new snapshot.
        let mut ancestors = Vec::new();
        let mut parent = arena.parent_of(owner);
        for _ in 0..128 {
            let Some(key) = parent else { break };
            ancestors.push(key);
            parent = arena.parent_of(key);
        }
        let has_viewport_chunk = items.iter().any(|item| {
            matches!(
                item,
                PaintCoverageItem::ArtifactChunk { chunk, .. }
                    if chunk.frame == super::super::PaintChunkFrame::Viewport
            )
        });
        let viewport_translation = match has_viewport_chunk {
            true => match arena.subtree_translation_revision(owner) {
                Some(revision) => Some(revision),
                None => return,
            },
            false => None,
        };
        let snapshot = Snapshot {
            owner,
            key: key.clone(),
            viewport_translation,
            completed: Default::default(),
            items: items.into(),
            left_out,
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
            properties: OwnerMap::new(property_closure(
                trees,
                members,
                &ancestors,
                &mut self.subtrees.shared_properties,
            )),
            // Only the members' own paint is recorded. An ancestor's reaches
            // none of it; its child order and property inputs are compared
            // through the key and the properties.
            generations: OwnerMap::new(
                members
                    .iter()
                    .map(|key| (*key, generations.local_generations_for(*key))),
            ),
        };
        self.subtrees.entries.insert(
            owner,
            (
                key,
                Arc::new(snapshot),
                true,
                Arc::new(trees.property_store_stamp()),
                generations.generation_store_stamp(),
            ),
        );
    }
}

#[cfg(test)]
mod tests;
