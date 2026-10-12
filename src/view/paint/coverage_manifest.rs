#[cfg(test)]
use super::PaintNodePlan;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::view::base_component::ShadowPaintRecordingCapability;
use crate::view::compositor::property_tree::{
    ClipNodeId, ClipNodeSnapshot, EffectNodeId, EffectNodeSnapshot, PropertyTreeState, ScrollNodeId,
};
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::node_arena::{NodeArena, NodeKey};

use super::{
    PaintChunkMetadata, PaintContentRevision, PaintNodePhase, PaintOwnerPropertyStateSnapshot,
    PaintOwnerSnapshot, PaintPropertyScope, PaintRecordingContext,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CoverageOrder {
    pub(crate) root_index: usize,
    pub(crate) child_path: std::sync::Arc<[usize]>,
    pub(crate) phase: PaintNodePhase,
    pub(crate) slot: u16,
}

// `CoverageOrder` is a structural witness for metadata/full parity, not a
// sorting key. Manifest vector order remains raster order: in particular an
// AfterChildren item must stay after every child even though its shorter
// `child_path` would sort before a descendant path.

impl CoverageOrder {
    fn for_chunk(&self, phase: PaintNodePhase, slot: u16) -> Self {
        Self {
            phase,
            slot,
            ..self.clone()
        }
    }
}

/// Most native phases emit one chunk. Keep that chunk in the pending phase
/// value while both phases validate, without a heap allocation per owner.
/// Arbitrary multi-slot/custom schedules retain their exact original order.
#[derive(Default)]
struct PreparedPlanSide {
    first: Option<PaintCoverageItem>,
    remaining: Vec<PaintCoverageItem>,
}
impl PreparedPlanSide {
    fn push(&mut self, item: PaintCoverageItem) {
        if self.first.is_none() {
            self.first = Some(item);
        } else {
            self.remaining.push(item);
        }
    }
}
impl IntoIterator for PreparedPlanSide {
    type Item = PaintCoverageItem;
    type IntoIter = std::iter::Chain<
        std::option::IntoIter<PaintCoverageItem>,
        std::vec::IntoIter<PaintCoverageItem>,
    >;
    fn into_iter(self) -> Self::IntoIter {
        self.first.into_iter().chain(self.remaining)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CoverageRecordingMode {
    MetadataOnly,
    FullArtifact,
}

/// One immutable owner observation, shared by its chunks and descendants.
/// Ancestors are edges rather than copied closures. A fresh recorder owns the
/// entire graph, so this sharing cannot authorize data from another frame.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PaintOwnerScope {
    pub(crate) topology: PaintOwnerSnapshot,
    pub(crate) state: PaintOwnerPropertyStateSnapshot,
    pub(crate) clips: [std::sync::Arc<[ClipNodeSnapshot]>; 2],
    pub(crate) effects: [std::sync::Arc<[EffectNodeSnapshot]>; 2],
    pub(crate) parent: Option<std::sync::Arc<PaintOwnerScope>>,
}

impl PaintOwnerScope {
    pub(super) fn same_live_inputs(&self, fresh: &Self) -> bool {
        self.topology == fresh.topology
            && self.state == fresh.state
            && self.clips == fresh.clips
            && self.effects == fresh.effects
            && match (&self.parent, &fresh.parent) {
                (None, None) => true,
                (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
                _ => false,
            }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum PaintCoverageItem {
    ArtifactChunk {
        order: CoverageOrder,
        chunk: PaintChunkMetadata,
        clip_snapshot: std::sync::Arc<[ClipNodeSnapshot]>,
        effect_snapshot: std::sync::Arc<[EffectNodeSnapshot]>,
        owner_scope: std::sync::Arc<PaintOwnerScope>,
        ops: Option<std::sync::Arc<[super::PaintOp]>>,
    },
    /// Draws nothing of its own. Walks and the subtree cache still see it;
    /// tests read its fields.
    #[cfg_attr(not(test), allow(dead_code))]
    TransparentNode {
        order: CoverageOrder,
        owner: NodeKey,
        stable_id: u64,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: PaintContentRevision,
    },
    /// Draws nothing, nor does its subtree.
    #[cfg_attr(not(test), allow(dead_code))]
    CulledSubtree {
        order: CoverageOrder,
        owner: NodeKey,
        stable_id: u64,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: PaintContentRevision,
    },
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PaintCoverageManifest {
    pub(crate) items: super::shared_sequence::SharedSequence<PaintCoverageItem>,
    covered_nodes: std::sync::Arc<FxHashSet<NodeKey>>,
}

#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub(crate) struct PaintCoverageStats {
    pub(crate) total_nodes: usize,
    pub(crate) artifact_nodes: usize,
    pub(crate) artifact_chunks: usize,
    pub(crate) culled_subtrees: usize,
}

#[cfg(test)]
impl PaintCoverageManifest {
    pub(crate) fn stats(&self) -> PaintCoverageStats {
        let mut artifact_nodes = FxHashSet::default();
        let mut culled = FxHashSet::default();
        let mut artifact_chunks = 0usize;
        for item in &self.items {
            match item {
                PaintCoverageItem::ArtifactChunk { chunk, .. } => {
                    artifact_nodes.insert(chunk.owner);
                    artifact_chunks = artifact_chunks.saturating_add(1);
                }
                PaintCoverageItem::TransparentNode { owner, .. } => {
                    artifact_nodes.insert(*owner);
                }
                PaintCoverageItem::CulledSubtree { owner, .. } => {
                    artifact_nodes.insert(*owner);
                    culled.insert(*owner);
                }
            }
        }
        PaintCoverageStats {
            total_nodes: self.covered_nodes.len(),
            artifact_nodes: artifact_nodes.len(),
            artifact_chunks,
            culled_subtrees: culled.len(),
        }
    }
}

pub(super) fn exact_deferred_viewport_self_clip_witness(
    arena: &NodeArena,
    owner: NodeKey,
    property_trees: &PropertyTrees,
) -> Option<super::PaintDeferredViewportSelfClipWitness> {
    let node = arena.get(owner)?;
    let stable_id = node.element.stable_id();
    let id = ClipNodeId {
        owner,
        role: crate::view::compositor::property_tree::ClipNodeRole::SelfClip,
    };
    // A deferred host that reports no clip of its own paints under the
    // viewport clip the property tree gave it.
    let geometry = match node
        .element
        .exact_retained_deferred_viewport_self_clip_geometry(owner, arena)
    {
        Some(geometry) => geometry,
        None if node.element.is_deferred_to_root_viewport_render() => {
            property_trees.clips.get(&id)?.geometry
        }
        None => return None,
    };
    let state = property_trees.node_state_for(owner)?;
    let exact_clip_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(id),
        ..Default::default()
    };
    let effect_id = crate::view::compositor::property_tree::EffectNodeId(owner);
    let exact_clip_effect_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(id),
        effect: Some(effect_id),
        ..Default::default()
    };
    let exact_root_effect = property_trees
        .effects
        .get(&effect_id)
        .is_some_and(|effect| {
            effect.owner == owner
                && effect.parent.is_none()
                && effect.opacity.is_finite()
                && (0.0..=1.0).contains(&effect.opacity)
                && effect.generation != 0
        });
    let state_is_exact = (state.paint.legacy_boundary_eq(exact_clip_state)
        && state.descendants.legacy_boundary_eq(exact_clip_state))
        || (exact_root_effect
            && state.paint.legacy_boundary_eq(exact_clip_effect_state)
            && state
                .descendants
                .legacy_boundary_eq(exact_clip_effect_state));
    // This witness only proves the late Replace clip; the complete property
    // chains are frozen with the artifact.
    let generic_clip_scope = state.paint.clip == Some(id)
        && property_trees
            .clip_snapshot_for(state.descendants.clip)
            .is_some_and(|chain| {
                // Descendants may tighten this owner's scope, but cannot
                // replace it or borrow another owner's contents clip.
                chain.iter().position(|clip| clip.id == id).is_some_and(|at| {
                    chain[at].behavior == crate::view::compositor::property_tree::ClipBehavior::Replace
                    && chain[..at].iter().all(|clip| {
                        clip.owner == owner
                            && clip.id.owner == owner
                            && clip.id.role == crate::view::compositor::property_tree::ClipNodeRole::ContentsClip
                            && clip.behavior == crate::view::compositor::property_tree::ClipBehavior::Intersect
                            && clip.generation != 0
                    })
                })
            });
    if !generic_clip_scope && !state_is_exact {
        return None;
    }
    let clip_chain = property_trees.clip_snapshot_for(Some(id))?;
    let mut clip = *clip_chain.first()?;
    if generic_clip_scope {
        // A Replace self clip supersedes its inherited ancestors. The scope
        // token describes that replacement, while the artifact still freezes
        // the original complete chain for graph/coordinate validation.
        clip.parent = None;
    } else if clip_chain.len() != 1 {
        return None;
    }
    super::PaintDeferredViewportSelfClipWitness::new(owner, stable_id, clip, geometry)
}

#[cfg(test)]
pub(crate) fn record_coverage_manifest(
    arena: &NodeArena,
    roots: &[NodeKey],
    emit_deferred_late: bool,
    recording_mode: CoverageRecordingMode,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
) -> PaintCoverageManifest {
    record_coverage_manifest_with_context(
        arena,
        roots,
        emit_deferred_late,
        recording_mode,
        property_trees,
        paint_generations,
        PaintRecordingContext::default(),
    )
}

#[cfg(test)]
pub(super) fn record_coverage_manifest_with_context(
    arena: &NodeArena,
    roots: &[NodeKey],
    emit_deferred_late: bool,
    recording_mode: CoverageRecordingMode,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    initial_recording_context: PaintRecordingContext,
) -> PaintCoverageManifest {
    record_cached_coverage_manifest(
        arena,
        roots,
        emit_deferred_late,
        recording_mode,
        property_trees,
        paint_generations,
        initial_recording_context,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn record_cached_coverage_manifest(
    arena: &NodeArena,
    roots: &[NodeKey],
    emit_deferred_late: bool,
    recording_mode: CoverageRecordingMode,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    initial_recording_context: PaintRecordingContext,
    mut recording_cache: Option<&mut super::RecordingCache>,
) -> PaintCoverageManifest {
    let _profile = crate::view::paint::work_profile::scope("record_coverage_manifest");
    super::work_profile::count(
        "coverage_item_bytes",
        std::mem::size_of::<PaintCoverageItem>(),
    );
    super::work_profile::count(
        "recording_context_bytes",
        std::mem::size_of::<PaintRecordingContext>(),
    );
    let _walk = super::coverage_walk::begin();
    if let Some(cache) = recording_cache.as_deref_mut() {
        cache.subtrees.bind(arena, roots);
    }
    let subtree_replay = recording_mode == CoverageRecordingMode::MetadataOnly
        && emit_deferred_late
        && initial_recording_context == PaintRecordingContext::default();
    let mut manifest = PaintCoverageManifest::default();
    let cached_topology = recording_cache
        .as_deref_mut()
        .filter(|_| subtree_replay)
        .and_then(|cache| cache.topology.replay(arena, roots));
    let topology_replayed = cached_topology.is_some();
    let topology = if let Some(cached) = cached_topology {
        cached
    } else {
        let capacity = recording_cache
            .as_deref()
            .map_or(0, |cache| cache.owner_capacity_hint());
        let mut observed_children = FxHashMap::default();
        let mut owner_parents = FxHashMap::<NodeKey, Option<NodeKey>>::with_capacity_and_hasher(
            capacity,
            Default::default(),
        );
        let mut resolved_nodes = FxHashSet::with_capacity_and_hasher(capacity, Default::default());
        let mut stable_keys =
            FxHashMap::<u64, NodeKey>::with_capacity_and_hasher(capacity, Default::default());
        let mut stack = roots
            .iter()
            .copied()
            .map(|root| (root, None))
            .collect::<Vec<_>>();
        while let Some((key, traversal_parent)) = stack.pop() {
            owner_parents.insert(key, traversal_parent);
            let Some(node) = arena.get(key) else {
                continue;
            };
            resolved_nodes.insert(key);
            stable_keys.insert(node.element.stable_id(), key);
            let children = node.element.children();
            if subtree_replay {
                observed_children.insert(key, children.to_vec());
            }
            stack.extend(children.iter().copied().map(|child| (child, Some(key))));
        }

        fn collect_deferred(
            arena: &NodeArena,
            key: NodeKey,
            seen: &mut FxHashSet<NodeKey>,
            queue: &mut Vec<NodeKey>,
        ) {
            if !seen.insert(key) {
                return;
            }
            let Some(node) = arena.get(key) else {
                return;
            };
            if node.element.is_deferred_to_root_viewport_render() {
                queue.push(key);
            }
            for &child in node.element.children() {
                collect_deferred(arena, child, seen, queue);
            }
        }
        let mut deferred_roots = Vec::new();
        let mut deferred_seen =
            FxHashSet::with_capacity_and_hasher(owner_parents.len(), Default::default());
        for &root in roots {
            collect_deferred(arena, root, &mut deferred_seen, &mut deferred_roots);
        }
        let deferred_set = deferred_roots.iter().copied().collect::<FxHashSet<_>>();

        let snapshot =
            std::sync::Arc::new(super::recording_cache::topology_cache::TopologySnapshot {
                owner_parents: std::sync::Arc::new(owner_parents),
                covered: std::sync::Arc::new(resolved_nodes),
                deferred_roots,
                deferred: deferred_set,
            });
        if subtree_replay {
            if let Some(cache) = recording_cache.as_deref_mut() {
                cache.topology.remember(
                    arena,
                    roots,
                    snapshot.clone(),
                    &stable_keys,
                    &observed_children,
                );
            }
        }
        snapshot
    };
    manifest.covered_nodes = topology.covered.clone();
    let owner_parents = &topology.owner_parents;
    let deferred_roots = &topology.deferred_roots;
    let deferred_set = &topology.deferred;

    struct Recorder<'a> {
        subtree_replay: bool,
        subtree_capture_depth: usize,
        scope_pending: Vec<NodeKey>,
        scope_seen: FxHashSet<NodeKey>,
        owner_scopes: FxHashMap<NodeKey, std::sync::Arc<PaintOwnerScope>>,
        replayed_owner_inputs:
            FxHashMap<NodeKey, std::sync::Arc<super::recording_cache::subtree_cache::Snapshot>>,
        clip_chains: FxHashMap<Option<ClipNodeId>, Option<std::sync::Arc<[ClipNodeSnapshot]>>>,
        effect_chains:
            FxHashMap<Option<EffectNodeId>, Option<std::sync::Arc<[EffectNodeSnapshot]>>>,
        recording_cache: Option<&'a mut super::RecordingCache>,
        arena: &'a NodeArena,
        deferred_roots: &'a FxHashSet<NodeKey>,
        properties: &'a PropertyTrees,
        owner_parents: &'a FxHashMap<NodeKey, Option<NodeKey>>,
        observed_owner_property_states: &'a mut FxHashMap<NodeKey, PaintOwnerPropertyStateSnapshot>,
        generations: &'a PaintGenerationTracker,
        recording_mode: CoverageRecordingMode,
        items: &'a mut super::shared_sequence::SharedSequence<PaintCoverageItem>,
    }
    struct RecordedPlanItem {
        chunk: PaintChunkMetadata,
        ops: Option<std::sync::Arc<[super::PaintOp]>>,
    }
    // Keep metadata in its original allocation until consumed. Expanding it
    // into a second vector merely to append an absent ops field costs one
    // allocation per native owner on every warm frame.
    enum RecordedPlanSide {
        Metadata(std::vec::IntoIter<PaintChunkMetadata>),
        Full(std::vec::IntoIter<RecordedPlanItem>),
    }
    impl Iterator for RecordedPlanSide {
        type Item = RecordedPlanItem;
        fn next(&mut self) -> Option<Self::Item> {
            match self {
                Self::Metadata(items) => items
                    .next()
                    .map(|chunk| RecordedPlanItem { chunk, ops: None }),
                Self::Full(items) => items.next(),
            }
        }
        fn size_hint(&self) -> (usize, Option<usize>) {
            match self {
                Self::Metadata(items) => items.size_hint(),
                Self::Full(items) => items.size_hint(),
            }
        }
    }
    impl ExactSizeIterator for RecordedPlanSide {}
    struct RecordedPlan {
        before_children: RecordedPlanSide,
        after_children: RecordedPlanSide,
    }
    impl Recorder<'_> {
        fn walk(
            &mut self,
            key: NodeKey,
            root_index: usize,
            path: &mut Vec<usize>,
            deferred_phase_root: bool,
            parent_recording_context: &PaintRecordingContext,
        ) {
            let candidate = if self.subtree_replay && self.subtree_capture_depth == 0 {
                self.recording_cache.as_deref_mut().and_then(|cache| {
                    cache.subtree_key(
                        self.arena,
                        key,
                        parent_recording_context,
                        root_index,
                        path,
                        deferred_phase_root,
                    )
                })
            } else {
                None
            };
            if let Some((input, _)) = &candidate {
                let snapshot = self.recording_cache.as_deref_mut().and_then(|cache| {
                    cache
                        .subtrees
                        .replay(self.arena, key, input, self.properties, self.generations)
                });
                if let Some(snapshot) = snapshot {
                    self.recording_cache
                        .as_deref_mut()
                        .unwrap()
                        .restore_subtree(&snapshot, self.arena);
                    self.replayed_owner_inputs.insert(key, snapshot.clone());
                    self.items.append_shared(snapshot.items.clone());
                    crate::view::paint::work_profile::count("recorded_subtree_replays", 1);
                    return;
                }
            }
            let candidate = candidate.filter(|_| {
                self.recording_cache
                    .as_deref()
                    .is_some_and(|cache| cache.subtrees.should_capture(key))
            });
            let start = self.items.len();
            if candidate.is_some() {
                self.subtree_capture_depth += 1;
            }
            self.walk_uncached(
                key,
                root_index,
                path,
                deferred_phase_root,
                parent_recording_context,
            );
            if candidate.is_some() {
                self.subtree_capture_depth -= 1;
            }
            if let Some((input, members)) = candidate
                && let Some(cache) = self.recording_cache.as_deref_mut()
            {
                cache.remember_subtree(
                    key,
                    input,
                    self.arena,
                    &members,
                    &self.items[start..],
                    self.observed_owner_property_states,
                    &self.owner_scopes,
                    self.properties,
                    self.generations,
                );
            }
        }

        fn walk_uncached(
            &mut self,
            key: NodeKey,
            root_index: usize,
            path: &mut Vec<usize>,
            deferred_phase_root: bool,
            parent_recording_context: &PaintRecordingContext,
        ) {
            let context_profile = super::work_profile::scope("observe_recording_context");
            let Some(node) = self.arena.get(key) else {
                return;
            };
            if self.deferred_roots.contains(&key) && !deferred_phase_root {
                return;
            }
            let stable_id = node.element.stable_id();
            if !node.element.supports_retained_command_replay() {
                if let Some(cache) = self.recording_cache.as_deref_mut() {
                    cache.require_full_walk();
                }
            }
            let order = CoverageOrder {
                root_index,
                child_path: match self.recording_cache.as_deref_mut() {
                    Some(cache) => cache.intern_order_path(key, path),
                    None => path.as_slice().into(),
                },
                phase: PaintNodePhase::BeforeChildren,
                slot: 0,
            };
            let property_states = self
                .properties
                .node_state_for(key)
                .expect("synced property trees hold every walked node");
            let live_properties = property_states.paint;
            let live_contents_properties = property_states.descendants;
            let generations = self
                .generations
                .local_generations_for(key)
                .expect("synced paint generations cover every walked node");
            let revision = PaintContentRevision {
                self_paint_revision: generations.self_paint_revision,
                composite_revision: generations.composite_revision,
                topology_revision: generations.topology_revision,
            };
            let mut recording_context = node
                .element
                .shadow_paint_recording_context(parent_recording_context);
            recording_context.is_frame_root = path.is_empty() && !deferred_phase_root;
            recording_context.recording_owner = Some(key);
            recording_context.recording_owner_stable_id = Some(stable_id);
            let scroll = ScrollNodeId(key);
            recording_context.surface_dag_scroll =
                (live_contents_properties.scroll == Some(scroll)).then_some(scroll);
            recording_context.surface_dag_scroll_snapshot = recording_context
                .surface_dag_scroll
                .and_then(|id| self.properties.scroll_snapshot_for(id))
                .map(std::sync::Arc::new);
            let properties = live_properties;
            let contents_properties = live_contents_properties;
            let owner_property_state = PaintOwnerPropertyStateSnapshot {
                owner: key,
                stable_id,
                paint: properties,
                descendants: contents_properties,
            };
            self.restore_owner_inputs(key);
            self.observed_owner_property_states
                .entry(key)
                .or_insert(owner_property_state);
            recording_context.authoritative_self_clip = self
                .properties
                .authoritative_self_clip_for_owner(key, live_properties);
            recording_context.deferred_viewport_self_clip = None;
            if deferred_phase_root {
                let clip =
                    exact_deferred_viewport_self_clip_witness(self.arena, key, self.properties);
                recording_context.deferred_viewport_self_clip = clip.map(std::sync::Arc::new);
            }
            drop(context_profile);
            let capability_profile = super::work_profile::scope("observe_recording_capability");
            let capability = node.element.shadow_paint_recording_capability(
                self.arena,
                deferred_phase_root,
                &recording_context,
            );
            drop(capability_profile);
            match capability {
                ShadowPaintRecordingCapability::CulledSubtree => {
                    self.items.push(PaintCoverageItem::CulledSubtree {
                        order,
                        owner: key,
                        stable_id,
                        properties,
                        content_revision: revision,
                    })
                }
                ShadowPaintRecordingCapability::Transparent => {
                    self.items.push(PaintCoverageItem::TransparentNode {
                        order,
                        owner: key,
                        stable_id,
                        properties,
                        content_revision: revision,
                    });
                    let children = node.element.children();
                    for (index, &child) in children.iter().enumerate() {
                        let child_recording_context =
                            node.element.shadow_paint_recording_context_for_child(
                                child,
                                self.arena,
                                &recording_context,
                            );
                        path.push(index);
                        self.walk(child, root_index, path, false, &child_recording_context);
                        path.pop();
                    }
                }
                ShadowPaintRecordingCapability::Recordable => {
                    let mask_profile = super::work_profile::scope("observe_child_mask");
                    let retained_child_mask = node
                        .element
                        .retained_child_mask_plan(self.arena, &recording_context);
                    drop(mask_profile);
                    let plan = match self.recording_mode {
                        CoverageRecordingMode::MetadataOnly => {
                            let _profile = crate::view::paint::work_profile::scope("metadata_hook");
                            let plan = node.element.record_shadow_paint_metadata_plan(
                                key,
                                properties,
                                contents_properties,
                                revision,
                                self.arena,
                                &recording_context,
                            );
                            // A host with nothing of its own to draw records
                            // no chunk; its children still record.
                            let plan = plan.unwrap_or_default();
                            let plan = super::metadata_plan_in_layout_frame(plan, self.arena);
                            if node.element.supports_retained_command_replay() {
                                if let Some(cache) = self.recording_cache.as_deref_mut() {
                                    cache.metadata(
                                        key,
                                        stable_id,
                                        &plan,
                                        properties,
                                        contents_properties,
                                        revision,
                                        &recording_context,
                                    );
                                }
                            }
                            RecordedPlan {
                                before_children: RecordedPlanSide::Metadata(
                                    plan.before_children.into_iter(),
                                ),
                                after_children: RecordedPlanSide::Metadata(
                                    plan.after_children.into_iter(),
                                ),
                            }
                        }
                        CoverageRecordingMode::FullArtifact => {
                            let _profile = crate::view::paint::work_profile::scope("command_hook");
                            let replayed = self
                                .recording_cache
                                .as_deref_mut()
                                .and_then(|cache| cache.replay(key));
                            let was_replayed = replayed.is_some();
                            let plan = replayed
                                .or_else(|| {
                                    node.element
                                        .record_shadow_paint_artifact_plan(
                                            key,
                                            properties,
                                            contents_properties,
                                            revision,
                                            self.arena,
                                            &recording_context,
                                        )
                                        .and_then(|plan| {
                                            super::artifact_plan_in_layout_frame(plan, self.arena)
                                        })
                                })
                                .unwrap_or_default();
                            if !was_replayed && node.element.supports_retained_command_replay() {
                                if let Some(cache) = self.recording_cache.as_deref_mut() {
                                    cache.insert(key, plan.clone());
                                }
                            }
                            let before_children =
                                Self::record_artifact_plan_side(plan.before_children);
                            let after_children =
                                Self::record_artifact_plan_side(plan.after_children);
                            RecordedPlan {
                                before_children: RecordedPlanSide::Full(
                                    before_children.into_iter(),
                                ),
                                after_children: RecordedPlanSide::Full(after_children.into_iter()),
                            }
                        }
                    };
                    let before_children = self.prepare_plan_side(
                        key,
                        &order,
                        PaintNodePhase::BeforeChildren,
                        plan.before_children,
                    );
                    let after_children = self.prepare_plan_side(
                        key,
                        &order,
                        PaintNodePhase::AfterChildren,
                        plan.after_children,
                    );
                    self.items.extend(before_children);
                    let children = node.element.children();
                    // A child mask carries its own normal/overflow partition.
                    let escaping_children = if retained_child_mask.is_none() {
                        escaping_child_phases(self.arena, node.element.as_ref(), children)
                    } else {
                        None
                    };
                    let in_scope_children = retained_child_mask
                        .as_ref()
                        .map_or(children, |mask| mask.in_scope_children());
                    let mut next_in_scope_index = 0;
                    for (schedule_index, &child) in in_scope_children.iter().enumerate() {
                        if escaping_children
                            .as_ref()
                            .is_some_and(|escaping| escaping[schedule_index])
                        {
                            continue;
                        }
                        let index = if retained_child_mask.is_none() {
                            schedule_index
                        } else {
                            next_ordered_child_index(children, &mut next_in_scope_index, child)
                        };
                        let child_recording_context =
                            node.element.shadow_paint_recording_context_for_child(
                                child,
                                self.arena,
                                &recording_context,
                            );
                        path.push(index);
                        self.walk(child, root_index, path, false, &child_recording_context);
                        path.pop();
                    }
                    // Escaping children paint after every normal sibling and
                    // before the parent's after-children paint (scrollbars).
                    for (index, &child) in children.iter().enumerate() {
                        if !escaping_children
                            .as_ref()
                            .is_some_and(|escaping| escaping[index])
                        {
                            continue;
                        }
                        let child_recording_context =
                            node.element.shadow_paint_recording_context_for_child(
                                child,
                                self.arena,
                                &recording_context,
                            );
                        path.push(index);
                        self.walk(child, root_index, path, false, &child_recording_context);
                        path.pop();
                    }
                    self.items.extend(after_children);
                    if let Some(mask) = retained_child_mask {
                        let mut next_overflow_index = 0;
                        for &child in mask.overflow_children() {
                            let index =
                                next_ordered_child_index(children, &mut next_overflow_index, child);
                            let child_recording_context =
                                node.element.shadow_paint_recording_context_for_child(
                                    child,
                                    self.arena,
                                    &recording_context,
                                );
                            path.push(index);
                            self.walk(child, root_index, path, false, &child_recording_context);
                            path.pop();
                        }
                    }
                }
            }
        }

        /// Each host artifact is one chunk and the commands it draws.
        fn record_artifact_plan_side(
            artifacts: Vec<super::PaintArtifact>,
        ) -> Vec<RecordedPlanItem> {
            artifacts
                .into_iter()
                .map(|artifact| {
                    let [artifact_chunk] = artifact.chunks.as_slice() else {
                        unreachable!("a host artifact holds exactly one chunk")
                    };
                    RecordedPlanItem {
                        chunk: PaintChunkMetadata {
                            frame: artifact_chunk.frame,
                            id: artifact_chunk.id,
                            owner: artifact_chunk.owner,
                            bounds: artifact_chunk.bounds,
                            properties: artifact_chunk.properties,
                            content_revision: artifact_chunk.content_revision,
                            payload_identity: artifact_chunk.payload_identity.clone(),
                        },
                        ops: Some(artifact.ops.into()),
                    }
                })
                .collect()
        }

        fn prepare_plan_side(
            &mut self,
            key: NodeKey,
            node_order: &CoverageOrder,
            phase: PaintNodePhase,
            items: RecordedPlanSide,
        ) -> PreparedPlanSide {
            let _profile = crate::view::paint::work_profile::scope("prepare_plan_side");
            let mut prepared = PreparedPlanSide {
                first: None,
                remaining: Vec::with_capacity(items.len().saturating_sub(1)),
            };
            let scope = self.owner_scope_for(key);
            for RecordedPlanItem { chunk, ops } in items {
                let order = node_order.for_chunk(phase, chunk.id.slot);
                let endpoint = match chunk.id.scope {
                    PaintPropertyScope::SelfPaint => 0,
                    PaintPropertyScope::Contents => 1,
                };
                prepared.push(PaintCoverageItem::ArtifactChunk {
                    order,
                    chunk,
                    clip_snapshot: scope.clips[endpoint].clone(),
                    effect_snapshot: scope.effects[endpoint].clone(),
                    owner_scope: scope.clone(),
                    ops,
                });
            }
            prepared
        }

        fn clip_snapshot_for(
            &mut self,
            state: PropertyTreeState,
        ) -> Option<std::sync::Arc<[ClipNodeSnapshot]>> {
            if let Some(chain) = self.clip_chains.get(&state.clip) {
                return chain.clone();
            }
            let chain = self
                .properties
                .clip_snapshot_for(state.clip)
                .map(std::sync::Arc::from);
            self.clip_chains.insert(state.clip, chain.clone());
            chain
        }

        fn effect_snapshot_for(
            &mut self,
            state: PropertyTreeState,
        ) -> Option<std::sync::Arc<[EffectNodeSnapshot]>> {
            if let Some(chain) = self.effect_chains.get(&state.effect) {
                return chain.clone();
            }
            let chain = self
                .properties
                .effect_snapshot_for(state.effect)
                .map(std::sync::Arc::from);
            self.effect_chains.insert(state.effect, chain.clone());
            chain
        }

        /// Load only a requested owner from an already validated subtree.
        /// Captures never contain nested replays, so fresh capture still reads
        /// complete local maps. Deferred traversal may need an old ancestor;
        /// the canonical parent graph locates that snapshot without a census.
        fn restore_owner_inputs(&mut self, key: NodeKey) {
            if self.replayed_owner_inputs.is_empty()
                || self.observed_owner_property_states.contains_key(&key)
                || self.owner_scopes.contains_key(&key)
            {
                return;
            }
            let mut cursor = Some(key);
            for _ in 0..128 {
                let Some(owner) = cursor else {
                    break;
                };
                if let Some(snapshot) = self.replayed_owner_inputs.get(&owner) {
                    if let Some((_, state)) =
                        snapshot.states.iter().find(|(owner, _)| *owner == key)
                    {
                        self.observed_owner_property_states.insert(key, *state);
                    }
                    if let Some((_, scope)) =
                        snapshot.scopes.iter().find(|(owner, _)| *owner == key)
                    {
                        self.owner_scopes.insert(key, scope.clone());
                    }
                    return;
                }
                cursor = self.owner_parents.get(&owner).copied().flatten();
            }
        }

        /// Resolve each owner once in this traversal. The immutable property
        /// trees make endpoint chain reuse exact.
        fn owner_scope_for(&mut self, leaf: NodeKey) -> std::sync::Arc<PaintOwnerScope> {
            self.restore_owner_inputs(leaf);
            if let Some(scope) = self.owner_scopes.get(&leaf) {
                return scope.clone();
            }
            const WALKED: &str = "an owner is walked before its chunks are prepared";
            self.scope_pending.clear();
            self.scope_seen.clear();
            let mut cursor = Some(leaf);
            while let Some(owner) = cursor {
                self.restore_owner_inputs(owner);
                if self.owner_scopes.contains_key(&owner) || !self.scope_seen.insert(owner) {
                    break;
                }
                self.scope_pending.push(owner);
                cursor = *self.owner_parents.get(&owner).expect(WALKED);
            }
            while let Some(owner) = self.scope_pending.pop() {
                let parent_key = *self.owner_parents.get(&owner).expect(WALKED);
                let parent = parent_key.map(|key| self.owner_scopes[&key].clone());
                let state = self.observed_owner_property_states[&owner];
                // Store the two immutable endpoint chains directly. Copying
                // their complete ancestry into every owner merely duplicates
                // observations; materialization merges each shared chain once.
                const SYNCED: &str = "synced property trees hold every referenced chain";
                let clips = [
                    self.clip_snapshot_for(state.paint).expect(SYNCED),
                    self.clip_snapshot_for(state.descendants).expect(SYNCED),
                ];
                let effects = [
                    self.effect_snapshot_for(state.paint).expect(SYNCED),
                    self.effect_snapshot_for(state.descendants).expect(SYNCED),
                ];
                let snap = super::PaintOwnerSnap::observe(
                    self.arena.get(owner).expect(WALKED).element.as_ref(),
                    self.arena,
                    state.paint.layout_position,
                );
                let scope = PaintOwnerScope {
                    topology: PaintOwnerSnapshot {
                        owner,
                        parent: parent_key,
                        snap,
                    },
                    state,
                    clips,
                    effects,
                    parent,
                };
                let scope = match self.recording_cache.as_deref_mut() {
                    Some(cache) => cache.intern_scope(scope),
                    None => std::sync::Arc::new(scope),
                };
                self.owner_scopes.insert(owner, scope);
            }
            self.owner_scopes[&leaf].clone()
        }
    }

    // The canonical topology walk has already measured this exact frame's
    // owner count. Reserve once instead of repeatedly moving wide owner states
    // as the metadata walk fills its maps.
    let owner_count = owner_parents.len();
    // Native owners normally have at most one chunk per paint phase. Reserve
    // both phases to avoid moving wide coverage items during the common walk;
    // custom multi-slot schedules may still grow without any count limit.
    if topology_replayed {
        // Warm recording appends immutable subtree blocks. Reserving a flat
        // owner-sized payload Vec here would allocate it only to discard it.
        manifest.items =
            super::shared_sequence::SharedSequence::with_shared_capacity(owner_count.min(128));
    } else {
        manifest.items.reserve(owner_count.saturating_mul(2));
    }
    let mut observed_owner_property_states =
        FxHashMap::with_capacity_and_hasher(owner_count, Default::default());
    let mut recorder = Recorder {
        subtree_replay,
        subtree_capture_depth: 0,
        scope_pending: Vec::new(),
        scope_seen: FxHashSet::default(),
        owner_scopes: FxHashMap::with_capacity_and_hasher(owner_count, Default::default()),
        replayed_owner_inputs: FxHashMap::default(),
        clip_chains: FxHashMap::default(),
        effect_chains: FxHashMap::default(),
        recording_cache,
        arena,
        deferred_roots: &deferred_set,
        properties: property_trees,
        owner_parents: &owner_parents,
        observed_owner_property_states: &mut observed_owner_property_states,
        generations: paint_generations,
        recording_mode,
        items: &mut manifest.items,
    };
    let walk_profile = super::work_profile::scope("walk_manifest");
    for (root_index, &root) in roots.iter().enumerate() {
        recorder.walk(
            root,
            root_index,
            &mut Vec::new(),
            false,
            &initial_recording_context,
        );
    }
    if emit_deferred_late {
        for (deferred_index, deferred_root) in deferred_roots.iter().copied().enumerate() {
            recorder.walk(
                deferred_root,
                roots.len(),
                &mut vec![deferred_index],
                true,
                &initial_recording_context,
            );
        }
    }
    drop(walk_profile);
    drop(recorder);
    manifest
}

/// For a parent that paints in phases, which children escape its inner clip:
/// they paint after every normal sibling, in arena order. Deferred Viewport
/// children are included but skip themselves in
/// the walk; the late phase records them. `None` when no child escapes, so the
/// walk keeps plain arena order without allocating.
fn escaping_child_phases(
    arena: &NodeArena,
    parent: &dyn crate::view::base_component::ElementTrait,
    children: &[NodeKey],
) -> Option<Vec<bool>> {
    if !parent.retains_absolute_clip_child_phase_order() {
        return None;
    }
    let escapes = |child: NodeKey| {
        arena.get(child).is_some_and(|node| {
            node.element
                .retained_absolute_clip_mode_witness(child, arena)
                != crate::view::base_component::RetainedAbsoluteClipModeWitness::Normal
        })
    };
    if !children.iter().copied().any(escapes) {
        return None;
    }
    Some(children.iter().copied().map(escapes).collect())
}

/// Each canonical mask partition preserves original sibling order. Continue
/// searching at the previous match instead of rescanning every earlier child.
fn next_ordered_child_index(children: &[NodeKey], next: &mut usize, child: NodeKey) -> usize {
    let offset = children[*next..]
        .iter()
        .position(|candidate| *candidate == child)
        .expect("a child mask partition keeps sibling order");
    let index = *next + offset;
    *next = index + 1;
    index
}

#[cfg(test)]
mod tests;
