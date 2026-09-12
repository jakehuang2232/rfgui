#![allow(dead_code)]

#[cfg(test)]
use super::PaintNodePlan;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::view::base_component::{ShadowPaintBlocker, ShadowPaintRecordingCapability};
use crate::view::compositor::property_tree::{
    ClipNodeId, ClipNodeSnapshot, EffectNodeId, EffectNodeSnapshot, PropertyTreeState,
    ScrollNodeId, TransformNodeId,
};
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::node_arena::{NodeArena, NodeKey};

use super::{
    LegacyPaintReason, PaintChunkMetadata, PaintContentRevision, PaintNodePhase,
    PaintOwnerPropertyStateSnapshot, PaintOwnerSnapshot, PaintPropertyScope, PaintRecordingContext,
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
    fn node(root_index: usize, child_path: &[usize]) -> Self {
        Self {
            root_index,
            child_path: child_path.into(),
            phase: PaintNodePhase::BeforeChildren,
            slot: 0,
        }
    }

    fn chunk(root_index: usize, child_path: &[usize], phase: PaintNodePhase, slot: u16) -> Self {
        Self {
            root_index,
            child_path: child_path.into(),
            phase,
            slot,
        }
    }

    fn for_chunk(&self, phase: PaintNodePhase, slot: u16) -> Self {
        Self {
            phase,
            slot,
            ..self.clone()
        }
    }
}

// Native owners usually have one or two slots. Keep that exact duplicate
// proof inline; custom schedules beyond four entries retain hash-set scaling.
#[derive(Default)]
struct SeenChunkSlots {
    inline: [Option<(PaintNodePhase, u16)>; 4],
    spill: FxHashSet<(PaintNodePhase, u16)>,
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
impl SeenChunkSlots {
    fn insert(&mut self, slot: (PaintNodePhase, u16)) -> bool {
        if self.inline.contains(&Some(slot)) {
            return false;
        }
        if let Some(empty) = self.inline.iter_mut().find(|entry| entry.is_none()) {
            *empty = Some(slot);
            return true;
        }
        self.spill.insert(slot)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CoverageRecordingMode {
    MetadataOnly,
    FullArtifact,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PlannedBoundary {
    pub(crate) root: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) kind: PlannedBoundaryKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PlannedBoundaryKind {
    Transform(TransformNodeId),
    Isolation(crate::view::compositor::property_tree::EffectNodeId),
    /// Typed H/C/O insertion point owned by the B4 property/scroll planner.
    /// The recorder stops before the host subtree; a later compiler must
    /// materialize host-before, detached content, and overlay-after exactly
    /// once at this marker.
    Scroll(ScrollNodeId),
}

pub(crate) type PlannedBoundaryCutoutSet = FxHashMap<NodeKey, PlannedBoundary>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeScrollContentReceiverCutout {
    pub(super) stable_id: u64,
    pub(super) witness: super::PaintScrollForestEdgeWitness,
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
    depth: usize,
}

impl PaintOwnerScope {
    pub(super) fn same_live_inputs(&self, fresh: &Self) -> bool {
        self.topology == fresh.topology
            && self.state == fresh.state
            && self.clips == fresh.clips
            && self.effects == fresh.effects
            && self.depth == fresh.depth
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
    TransparentNode {
        order: CoverageOrder,
        owner: NodeKey,
        stable_id: u64,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: PaintContentRevision,
    },
    CulledSubtree {
        order: CoverageOrder,
        owner: NodeKey,
        stable_id: u64,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: PaintContentRevision,
    },
    LegacyBoundary {
        order: CoverageOrder,
        root: NodeKey,
        stable_id: u64,
        reason: LegacyPaintReason,
    },
    PlannedBoundary {
        order: CoverageOrder,
        boundary: PlannedBoundary,
    },
    NativeScrollContentReceiver {
        order: CoverageOrder,
        cutout: NativeScrollContentReceiverCutout,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PaintCoverageValidationError {
    MissingNode(NodeKey),
    DuplicateNodeKey(NodeKey),
    DuplicateStableId(u64),
    InvalidChunkIdOwner(NodeKey),
    InvalidChunkOwner(NodeKey),
    InvalidChunkBounds(NodeKey),
    InvalidChunkProperties(NodeKey),
    InvalidChunkRevision(NodeKey),
    InvalidChunkPhase {
        node: NodeKey,
        expected: PaintNodePhase,
        actual: PaintNodePhase,
    },
    DuplicateChunkSlot {
        node: NodeKey,
        phase: PaintNodePhase,
        slot: u16,
    },
    InvalidClipSnapshot(NodeKey),
    InvalidEffectSnapshot(NodeKey),
    InvalidOwnerSnapshot(NodeKey),
    ConflictingOwnerPropertyState(NodeKey),
    ConflictingClipSnapshot(ClipNodeId),
    ConflictingEffectSnapshot(EffectNodeId),
    ConflictingOwnerSnapshot(NodeKey),
    InvalidArtifactChunkCount {
        node: NodeKey,
        actual: usize,
    },
    InvalidArtifactOpRange {
        node: NodeKey,
        start: usize,
        end: usize,
        op_count: usize,
    },
    InvalidPlannedBoundary(NodeKey),
    RecordingPassMismatch,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PaintCoverageManifest {
    pub(crate) items: Vec<PaintCoverageItem>,
    pub(crate) validation_errors: Vec<PaintCoverageValidationError>,
    covered_nodes: FxHashSet<NodeKey>,
    legacy_coverage: FxHashMap<LegacyPaintReason, FxHashSet<NodeKey>>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PaintCoverageStats {
    pub(crate) total_nodes: usize,
    pub(crate) artifact_nodes: usize,
    pub(crate) artifact_chunks: usize,
    pub(crate) culled_subtrees: usize,
    pub(crate) legacy_boundaries: usize,
    pub(crate) legacy_covered_nodes: usize,
    pub(crate) legacy_by_reason: Vec<(LegacyPaintReason, usize)>,
    pub(crate) validation_errors: usize,
    pub(crate) authority_eligible: bool,
    pub(crate) authority_ineligible_reasons: Vec<&'static str>,
    covered_node_keys: FxHashSet<NodeKey>,
    artifact_node_keys: FxHashSet<NodeKey>,
    culled_node_keys: FxHashSet<NodeKey>,
    legacy_node_keys: FxHashSet<NodeKey>,
}

impl PaintCoverageManifest {
    pub(crate) fn stats(&self) -> PaintCoverageStats {
        let mut nodes = FxHashSet::default();
        let mut artifact_nodes = FxHashSet::default();
        let mut legacy = FxHashSet::default();
        let mut culled = FxHashSet::default();
        let mut by_reason = FxHashMap::<LegacyPaintReason, usize>::default();
        let mut artifact_chunks = 0usize;
        for item in &self.items {
            match item {
                PaintCoverageItem::ArtifactChunk { chunk, .. } => {
                    nodes.insert(chunk.owner);
                    artifact_nodes.insert(chunk.owner);
                    artifact_chunks = artifact_chunks.saturating_add(1);
                }
                PaintCoverageItem::TransparentNode { owner, .. } => {
                    nodes.insert(*owner);
                    artifact_nodes.insert(*owner);
                }
                PaintCoverageItem::CulledSubtree { owner, .. } => {
                    nodes.insert(*owner);
                    artifact_nodes.insert(*owner);
                    culled.insert(*owner);
                }
                PaintCoverageItem::LegacyBoundary { root, .. } => {
                    nodes.insert(*root);
                    legacy.insert(*root);
                }
                PaintCoverageItem::PlannedBoundary { boundary, .. } => {
                    nodes.insert(boundary.root);
                }
                PaintCoverageItem::NativeScrollContentReceiver { cutout, .. } => {
                    nodes.insert(cutout.witness.content_root());
                }
            }
        }
        let legacy_nodes = self
            .legacy_coverage
            .values()
            .flatten()
            .copied()
            .collect::<FxHashSet<_>>();
        for (&reason, keys) in &self.legacy_coverage {
            by_reason.insert(reason, keys.len());
        }
        let mut legacy_by_reason = by_reason.into_iter().collect::<Vec<_>>();
        legacy_by_reason.sort_by_key(|(reason, _)| format!("{reason:?}"));
        let mut authority_ineligible_reasons = vec!["shadow_only"];
        if !legacy.is_empty() {
            authority_ineligible_reasons.push("legacy_boundaries");
        }
        if !self.validation_errors.is_empty() {
            authority_ineligible_reasons.push("validation_errors");
        }
        PaintCoverageStats {
            total_nodes: self.covered_nodes.len(),
            artifact_nodes: artifact_nodes.len(),
            artifact_chunks,
            culled_subtrees: culled.len(),
            legacy_boundaries: legacy.len(),
            legacy_covered_nodes: legacy_nodes.len(),
            legacy_by_reason,
            validation_errors: self.validation_errors.len(),
            authority_eligible: false,
            authority_ineligible_reasons,
            covered_node_keys: self.covered_nodes.clone(),
            artifact_node_keys: artifact_nodes,
            culled_node_keys: culled,
            legacy_node_keys: legacy_nodes,
        }
    }
}

impl PaintCoverageStats {
    pub(crate) fn merge(&mut self, other: Self) {
        self.covered_node_keys.extend(other.covered_node_keys);
        self.artifact_node_keys.extend(other.artifact_node_keys);
        self.culled_node_keys.extend(other.culled_node_keys);
        self.legacy_node_keys.extend(other.legacy_node_keys);
        self.total_nodes = self.covered_node_keys.len();
        self.artifact_nodes = self.artifact_node_keys.len();
        self.culled_subtrees = self.culled_node_keys.len();
        self.legacy_covered_nodes = self.legacy_node_keys.len();
        self.artifact_chunks = self.artifact_chunks.saturating_add(other.artifact_chunks);
        self.legacy_boundaries = self
            .legacy_boundaries
            .saturating_add(other.legacy_boundaries);
        self.validation_errors = self
            .validation_errors
            .saturating_add(other.validation_errors);
        for (reason, count) in other.legacy_by_reason {
            if let Some((_, current)) = self
                .legacy_by_reason
                .iter_mut()
                .find(|(current_reason, _)| *current_reason == reason)
            {
                *current = current.saturating_add(count);
            } else {
                self.legacy_by_reason.push((reason, count));
            }
        }
        self.authority_eligible = false;
        for reason in other.authority_ineligible_reasons {
            if !self.authority_ineligible_reasons.contains(&reason) {
                self.authority_ineligible_reasons.push(reason);
            }
        }
    }
}

pub(super) fn exact_deferred_viewport_self_clip_witness(
    arena: &NodeArena,
    owner: NodeKey,
    property_trees: &PropertyTrees,
    generic: bool,
) -> Option<super::PaintDeferredViewportSelfClipWitness> {
    let node = arena.get(owner)?;
    let stable_id = node.element.stable_id();
    let logical_scissor = node
        .element
        .exact_retained_deferred_viewport_self_clip_scissor_rect(owner, arena)?;
    let id = ClipNodeId {
        owner,
        role: crate::view::compositor::property_tree::ClipNodeRole::SelfClip,
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
    // Generic recording validates and freezes the complete property chains
    // separately. This witness only proves the late Replace clip and must not
    // reject independent placement/effect obligations because they coexist.
    // Compatibility recording still needs its original complete-state proof.
    let generic_clip_scope = generic
        && state.paint.clip == Some(id)
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
    super::PaintDeferredViewportSelfClipWitness::new(owner, stable_id, clip, logical_scissor)
}

pub(crate) fn record_coverage_manifest(
    arena: &NodeArena,
    roots: &[NodeKey],
    force_legacy_roots: bool,
    emit_deferred_late: bool,
    recording_mode: CoverageRecordingMode,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
) -> PaintCoverageManifest {
    let planned_boundary_cutouts = PlannedBoundaryCutoutSet::default();
    record_coverage_manifest_with_context(
        arena,
        roots,
        force_legacy_roots,
        emit_deferred_late,
        recording_mode,
        property_trees,
        paint_generations,
        PaintRecordingContext::default(),
        None,
        &planned_boundary_cutouts,
    )
}

pub(super) fn record_coverage_manifest_with_context(
    arena: &NodeArena,
    roots: &[NodeKey],
    force_legacy_roots: bool,
    emit_deferred_late: bool,
    recording_mode: CoverageRecordingMode,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    initial_recording_context: PaintRecordingContext,
    transform_surface_authority: Option<super::PaintTransformSurfaceWitness>,
    planned_boundary_cutouts: &PlannedBoundaryCutoutSet,
) -> PaintCoverageManifest {
    record_coverage_manifest_with_property_authorities(
        arena,
        roots,
        force_legacy_roots,
        emit_deferred_late,
        recording_mode,
        property_trees,
        paint_generations,
        initial_recording_context,
        transform_surface_authority,
        None,
        planned_boundary_cutouts,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn record_cached_coverage_manifest(
    arena: &NodeArena,
    roots: &[NodeKey],
    force_legacy_roots: bool,
    emit_deferred_late: bool,
    recording_mode: CoverageRecordingMode,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    context: PaintRecordingContext,
    transform: Option<super::PaintTransformSurfaceWitness>,
    cutouts: &PlannedBoundaryCutoutSet,
    cache: Option<&mut super::RecordingCache>,
) -> PaintCoverageManifest {
    let _profile =
        crate::view::paint::work_profile::scope("record_retained_coverage_manifest_with_context");
    record_coverage_manifest_with_property_authorities_impl(
        arena,
        roots,
        force_legacy_roots,
        emit_deferred_late,
        recording_mode,
        property_trees,
        paint_generations,
        context,
        transform,
        None,
        None,
        cutouts,
        None,
        cache,
    )
}

/// Retained-authority recorder.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_retained_coverage_manifest_with_context(
    arena: &NodeArena,
    roots: &[NodeKey],
    force_legacy_roots: bool,
    emit_deferred_late: bool,
    recording_mode: CoverageRecordingMode,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    initial_recording_context: PaintRecordingContext,
    transform_surface_authority: Option<super::PaintTransformSurfaceWitness>,
    planned_boundary_cutouts: &PlannedBoundaryCutoutSet,
) -> PaintCoverageManifest {
    let _profile =
        crate::view::paint::work_profile::scope("record_retained_coverage_manifest_with_context");
    record_coverage_manifest_with_context(
        arena,
        roots,
        force_legacy_roots,
        emit_deferred_late,
        recording_mode,
        property_trees,
        paint_generations,
        initial_recording_context,
        transform_surface_authority,
        planned_boundary_cutouts,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn record_coverage_manifest_with_property_authorities(
    arena: &NodeArena,
    roots: &[NodeKey],
    force_legacy_roots: bool,
    emit_deferred_late: bool,
    recording_mode: CoverageRecordingMode,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    initial_recording_context: PaintRecordingContext,
    transform_surface_authority: Option<super::PaintTransformSurfaceWitness>,
    effect_surface_authority: Option<&super::EffectPropertySurfaceArtifactContract>,
    planned_boundary_cutouts: &PlannedBoundaryCutoutSet,
) -> PaintCoverageManifest {
    record_coverage_manifest_with_property_authorities_impl(
        arena,
        roots,
        force_legacy_roots,
        emit_deferred_late,
        recording_mode,
        property_trees,
        paint_generations,
        initial_recording_context,
        transform_surface_authority,
        effect_surface_authority,
        None,
        planned_boundary_cutouts,
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn record_retained_coverage_manifest_with_property_authorities(
    arena: &NodeArena,
    roots: &[NodeKey],
    force_legacy_roots: bool,
    emit_deferred_late: bool,
    recording_mode: CoverageRecordingMode,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    initial_recording_context: PaintRecordingContext,
    transform_surface_authority: Option<super::PaintTransformSurfaceWitness>,
    effect_surface_authority: Option<&super::EffectPropertySurfaceArtifactContract>,
    planned_boundary_cutouts: &PlannedBoundaryCutoutSet,
) -> PaintCoverageManifest {
    record_coverage_manifest_with_property_authorities(
        arena,
        roots,
        force_legacy_roots,
        emit_deferred_late,
        recording_mode,
        property_trees,
        paint_generations,
        initial_recording_context,
        transform_surface_authority,
        effect_surface_authority,
        planned_boundary_cutouts,
    )
}

#[allow(clippy::too_many_arguments)]
fn record_coverage_manifest_with_property_authorities_impl(
    arena: &NodeArena,
    roots: &[NodeKey],
    force_legacy_roots: bool,
    emit_deferred_late: bool,
    recording_mode: CoverageRecordingMode,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    initial_recording_context: PaintRecordingContext,
    transform_surface_authority: Option<super::PaintTransformSurfaceWitness>,
    effect_surface_authority: Option<&super::EffectPropertySurfaceArtifactContract>,
    property_forest_ancestor_chain: Option<&super::ConsumedPropertyForestAncestorChainWitness>,
    planned_boundary_cutouts: &PlannedBoundaryCutoutSet,
    native_scroll_receiver: Option<NativeScrollContentReceiverCutout>,
    recording_cache: Option<&mut super::RecordingCache>,
) -> PaintCoverageManifest {
    let mut manifest = PaintCoverageManifest::default();
    let capacity = recording_cache.as_deref().map_or(0, |cache| cache.owner_capacity_hint());
    let mut owner_parents = FxHashMap::<NodeKey, Option<NodeKey>>::with_capacity_and_hasher(
        capacity, Default::default(),
    );
    let mut resolved_nodes = FxHashSet::with_capacity_and_hasher(capacity, Default::default());
    let mut stable_keys = FxHashMap::<u64, NodeKey>::with_capacity_and_hasher(
        capacity, Default::default(),
    );
    let mut stack = roots
        .iter()
        .copied()
        .map(|root| (root, None))
        .collect::<Vec<_>>();
    while let Some((key, traversal_parent)) = stack.pop() {
        match owner_parents.entry(key) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(traversal_parent);
            }
            std::collections::hash_map::Entry::Occupied(_) => {
                manifest
                    .validation_errors
                    .push(PaintCoverageValidationError::DuplicateNodeKey(key));
                continue;
            }
        }
        let Some(node) = arena.get(key) else {
            manifest
                .validation_errors
                .push(PaintCoverageValidationError::MissingNode(key));
            continue;
        };
        resolved_nodes.insert(key);
        let stable_id = node.element.stable_id();
        if stable_keys.insert(stable_id, key).is_some() {
            manifest
                .validation_errors
                .push(PaintCoverageValidationError::DuplicateStableId(stable_id));
        }
        stack.extend(
            node.element
                .children()
                .iter()
                .copied()
                .map(|child| (child, Some(key))),
        );
    }
    manifest.covered_nodes = resolved_nodes;
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
    let mut deferred_seen = FxHashSet::with_capacity_and_hasher(
        owner_parents.len(), Default::default(),
    );
    for &root in roots {
        collect_deferred(arena, root, &mut deferred_seen, &mut deferred_roots);
    }
    let deferred_set = deferred_roots.iter().copied().collect::<FxHashSet<_>>();

    struct Recorder<'a> {
        scope_pending: Vec<NodeKey>,
        scope_seen: FxHashSet<NodeKey>,
        owner_scopes: FxHashMap<NodeKey, std::sync::Arc<PaintOwnerScope>>,
        clip_chains: FxHashMap<Option<ClipNodeId>, Option<std::sync::Arc<[ClipNodeSnapshot]>>>,
        effect_chains:
            FxHashMap<Option<EffectNodeId>, Option<std::sync::Arc<[EffectNodeSnapshot]>>>,
        recording_cache: Option<&'a mut super::RecordingCache>,
        arena: &'a NodeArena,
        force_legacy_roots: bool,
        deferred_roots: &'a FxHashSet<NodeKey>,
        properties: &'a PropertyTrees,
        owner_parents: &'a FxHashMap<NodeKey, Option<NodeKey>>,
        observed_owner_property_states: &'a mut FxHashMap<NodeKey, PaintOwnerPropertyStateSnapshot>,
        generations: &'a PaintGenerationTracker,
        recording_mode: CoverageRecordingMode,
        transform_surface_authority: Option<super::PaintTransformSurfaceWitness>,
        surface_dag: bool,
        effect_surface_authority: Option<&'a super::EffectPropertySurfaceArtifactContract>,
        property_forest_ancestor_chain:
            Option<&'a super::ConsumedPropertyForestAncestorChainWitness>,
        baked_scroll_host_authority: Option<super::PaintBakedScrollHostWitness>,
        consumed_ancestor_property: Option<super::ConsumedAncestorProperty>,
        consumed_ancestor_property_stack: Option<super::ConsumedAncestorPropertyStackWitness>,
        scroll_forest_host: Option<super::PaintScrollForestEdgeWitness>,
        /// Time-boxed bridge for the pre-V2 exact detached subtree grammars.
        /// It reaches the walker only through the private legacy entry point,
        /// never through a `record_coverage_manifest*` API, never through
        /// `PaintRecordingContext`, and never into the artifact. It is deleted
        /// with `legacy_admission` in the Stage C hard cutover.
        required_scroll_content_paint_offset_bits: Option<[u32; 2]>,
        opacity_authority: super::PaintOpacityAuthority,
        planned_boundary_cutouts: &'a PlannedBoundaryCutoutSet,
        native_scroll_receiver: Option<NativeScrollContentReceiverCutout>,
        items: &'a mut Vec<PaintCoverageItem>,
        validation_errors: &'a mut Vec<PaintCoverageValidationError>,
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
    enum CulledSubtreeBoundary {
        Deferred,
        Property(LegacyPaintReason),
    }
    impl Recorder<'_> {
        fn culled_subtree_boundary(&self, root: NodeKey) -> Option<CulledSubtreeBoundary> {
            let _profile = crate::view::paint::work_profile::scope("culled_subtree_boundary");
            let mut stack = self
                .arena
                .get(root)?
                .element
                .children()
                .iter()
                .copied()
                .collect::<Vec<_>>();
            let mut seen = FxHashSet::default();
            while let Some(key) = stack.pop() {
                if !seen.insert(key) {
                    continue;
                }
                if self.deferred_roots.contains(&key) {
                    if !self.surface_dag {
                        return Some(CulledSubtreeBoundary::Deferred);
                    }
                    // Generic recording independently walks every collected
                    // deferred root in the late phase, even when its authored
                    // ancestor is culled. Do not erase that separate work or
                    // reject the ancestor for containing it.
                    continue;
                }
                let Some(node) = self.arena.get(key) else {
                    continue;
                };
                // Generic recording freezes property scopes. A culled subtree
                // contributes no operations even under an effect/transform;
                // deferred roots above remain separate paint obligations.
                if !self.surface_dag
                    && let Some(state) = self.properties.node_state_for(key)
                {
                    for properties in [state.paint, state.descendants] {
                        if properties.transform.is_some() {
                            return Some(CulledSubtreeBoundary::Property(
                                LegacyPaintReason::Transform,
                            ));
                        }
                        if properties.scroll.is_some() {
                            return Some(CulledSubtreeBoundary::Property(
                                LegacyPaintReason::ScrollContainer,
                            ));
                        }
                        if properties.effect.is_some() {
                            return Some(CulledSubtreeBoundary::Property(
                                LegacyPaintReason::StatefulPaint,
                            ));
                        }
                    }
                }
                stack.extend(node.element.children().iter().copied());
            }
            None
        }

        fn walk(
            &mut self,
            key: NodeKey,
            root_index: usize,
            path: &mut Vec<usize>,
            deferred_phase_root: bool,
            parent_recording_context: &PaintRecordingContext,
        ) {
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
            if let Some(cutout) = self.native_scroll_receiver
                && key == cutout.witness.content_root()
            {
                if cutout.stable_id != stable_id
                    || self.owner_parents.get(&key).copied().flatten()
                        != Some(cutout.witness.boundary_root())
                {
                    self.validation_errors
                        .push(PaintCoverageValidationError::InvalidPlannedBoundary(key));
                    return;
                }
                self.items
                    .push(PaintCoverageItem::NativeScrollContentReceiver { order, cutout });
                return;
            }
            if let Some(boundary) = self.planned_boundary_cutouts.get(&key) {
                if boundary.root != key
                    || boundary.stable_id != stable_id
                    || match boundary.kind {
                        PlannedBoundaryKind::Transform(transform) => {
                            transform.0 != key
                                || !self.properties.transforms.get(&transform).is_some_and(
                                    |snapshot| snapshot.owner == key && snapshot.generation != 0,
                                )
                        }
                        PlannedBoundaryKind::Isolation(effect) => {
                            effect.0 != key
                                || !self
                                    .properties
                                    .effects
                                    .get(&effect)
                                    .is_some_and(|snapshot| {
                                        snapshot.owner == key
                                            && snapshot.generation != 0
                                            && snapshot.opacity.is_finite()
                                            && (0.0..=1.0).contains(&snapshot.opacity)
                                    })
                        }
                        PlannedBoundaryKind::Scroll(scroll) => {
                            scroll.0 != key
                                || !self
                                    .properties
                                    .scrolls
                                    .get(&scroll)
                                    .is_some_and(|snapshot| {
                                        snapshot.owner == key && snapshot.generation != 0
                                    })
                        }
                    }
                {
                    self.validation_errors
                        .push(PaintCoverageValidationError::InvalidPlannedBoundary(key));
                    return;
                }
                self.items.push(PaintCoverageItem::PlannedBoundary {
                    order,
                    boundary: *boundary,
                });
                return;
            }
            if self.force_legacy_roots && path.is_empty() && !node.element.children().is_empty() {
                self.push_legacy_boundary(key, stable_id, LegacyPaintReason::HasChildren, order);
                return;
            }
            let Some(property_states) = self.properties.node_state_for(key) else {
                self.push_legacy_boundary(
                    key,
                    stable_id,
                    LegacyPaintReason::MissingPaintIdentity,
                    order,
                );
                return;
            };
            let live_properties = property_states.paint;
            let live_contents_properties = property_states.descendants;
            let Some(generations) = self.generations.local_generations_for(key) else {
                self.push_legacy_boundary(
                    key,
                    stable_id,
                    LegacyPaintReason::MissingPaintIdentity,
                    order,
                );
                return;
            };
            let revision = PaintContentRevision {
                self_paint_revision: generations.self_paint_revision,
                composite_revision: generations.composite_revision,
                topology_revision: generations.topology_revision,
            };
            let mut recording_context = node
                .element
                .shadow_paint_recording_context(parent_recording_context);
            recording_context.required_scroll_content_paint_offset_bits =
                self.required_scroll_content_paint_offset_bits;
            if self
                .required_scroll_content_paint_offset_bits
                .is_some_and(|required| {
                    recording_context.paint_offset.map(f32::to_bits) != required
                })
            {
                self.push_legacy_boundary(
                    key,
                    stable_id,
                    LegacyPaintReason::MissingPaintIdentity,
                    order,
                );
                return;
            }
            recording_context.is_frame_root = path.is_empty() && !deferred_phase_root;
            recording_context.inline_root_recording = None;
            recording_context.recording_owner = Some(key);
            recording_context.recording_owner_stable_id = Some(stable_id);
            recording_context.authoritative_self_clip = None;
            recording_context.subtree_self_clip = None;
            // A component hook cannot mint or retarget consumed-property
            // authority.  Rebind the recorder-owned witness to this canonical
            // traversal owner after the hook returns.
            recording_context.consumed_ancestor_property = self
                .consumed_ancestor_property
                .map(|witness| witness.for_target(key));
            recording_context.consumed_ancestor_property_stack = self
                .consumed_ancestor_property_stack
                .map(|witness| witness.for_target(key));
            recording_context.property_forest_projection = self
                .property_forest_ancestor_chain
                .and_then(|witness| witness.projection_for_target(key));
            recording_context.scroll_forest_host = self.scroll_forest_host;
            // Retired detached-subtree flags cannot be inherited from component hooks.
            recording_context.scroll_content_local_owner = false;
            recording_context.descendant_contents_clip = false;
            recording_context.resident_caret_suppressed = false;
            // Opacity authority is a recorder policy, not ambient component
            // state. Rebind it after every node/child hook so a component
            // cannot bake a root-group opacity that the compositor will apply
            // again at the isolation boundary.
            recording_context.opacity_authority = self.opacity_authority;
            // Never inherit ambient transform authority. Only this recorder's
            // canonical surface policy may bind a witness to the current
            // traversal owner and exact inherited transform boundary.
            recording_context.transform_surface = None;
            recording_context.surface_dag = self.surface_dag;
            recording_context.surface_dag_transform = self
                .surface_dag
                .then_some(live_properties.transform)
                .flatten();
            recording_context.surface_dag_scroll = self
                .surface_dag
                .then(|| {
                    let scroll = ScrollNodeId(key);
                    (live_contents_properties.scroll == Some(scroll)).then_some(scroll)
                })
                .flatten();
            recording_context.surface_dag_scroll_snapshot = recording_context
                .surface_dag_scroll
                .and_then(|id| self.properties.scroll_snapshot_for(id));
            if let Some(witness) = self.transform_surface_authority
                && live_properties.transform == Some(witness.transform)
            {
                recording_context.transform_surface = Some(witness.for_target(key));
            }
            recording_context.baked_scroll_host = None;
            if let Some(witness) = self.baked_scroll_host_authority
                && (key == witness.boundary_root() || key == witness.child())
            {
                recording_context.baked_scroll_host = Some(witness.for_target(key));
            }
            let project = |live| recording_context.project_consumed_ancestor_property(live);
            let Some(mut properties) = project(live_properties) else {
                self.push_legacy_boundary(
                    key,
                    stable_id,
                    LegacyPaintReason::MissingPaintIdentity,
                    order,
                );
                return;
            };
            let Some(mut contents_properties) = project(live_contents_properties) else {
                self.push_legacy_boundary(
                    key,
                    stable_id,
                    LegacyPaintReason::MissingPaintIdentity,
                    order,
                );
                return;
            };
            if let Some(authority) = self.effect_surface_authority {
                let Some(paint_chain) = self.properties.clip_snapshot_for(properties.clip) else {
                    self.push_legacy_boundary(
                        key,
                        stable_id,
                        LegacyPaintReason::MissingPaintIdentity,
                        order,
                    );
                    return;
                };
                let Some(projected) = authority.project_clip_leaf(properties.clip, &paint_chain)
                else {
                    self.push_legacy_boundary(
                        key,
                        stable_id,
                        LegacyPaintReason::MissingPaintIdentity,
                        order,
                    );
                    return;
                };
                properties.clip = projected;

                let Some(contents_chain) =
                    self.properties.clip_snapshot_for(contents_properties.clip)
                else {
                    self.push_legacy_boundary(
                        key,
                        stable_id,
                        LegacyPaintReason::MissingPaintIdentity,
                        order,
                    );
                    return;
                };
                let Some(projected) =
                    authority.project_clip_leaf(contents_properties.clip, &contents_chain)
                else {
                    self.push_legacy_boundary(
                        key,
                        stable_id,
                        LegacyPaintReason::MissingPaintIdentity,
                        order,
                    );
                    return;
                };
                contents_properties.clip = projected;
            }
            // Rebind after component hooks and projection: a resource host may
            // preserve only this owner's exact generic recorded property state.
            recording_context.surface_dag_paint_state = self.surface_dag.then_some(properties);
            let owner_property_state = PaintOwnerPropertyStateSnapshot {
                owner: key,
                stable_id,
                paint: properties,
                descendants: contents_properties,
            };
            if let Some(existing) = self.observed_owner_property_states.get(&key) {
                if *existing != owner_property_state {
                    self.validation_errors.push(
                        PaintCoverageValidationError::ConflictingOwnerPropertyState(key),
                    );
                    self.push_legacy_boundary(
                        key,
                        stable_id,
                        LegacyPaintReason::MissingPaintIdentity,
                        order,
                    );
                    return;
                }
            } else {
                self.observed_owner_property_states
                    .insert(key, owner_property_state);
            }
            recording_context.authoritative_self_clip = self
                .properties
                .authoritative_self_clip_for_owner(key, live_properties);
            recording_context.subtree_self_clip = self
                .surface_dag
                .then(|| {
                    super::recording_context::PaintSubtreeSelfClipWitness::from_live_owner(
                        self.arena,
                        key,
                        self.properties,
                        recording_context.is_frame_root,
                    )
                })
                .flatten()
                .filter(|witness| witness.matches_recorded_scopes(properties, contents_properties));
            recording_context.deferred_viewport_self_clip = None;
            recording_context.deferred_viewport_effect = None;
            if deferred_phase_root {
                let clip = exact_deferred_viewport_self_clip_witness(
                    self.arena,
                    key,
                    self.properties,
                    self.surface_dag,
                );
                recording_context.deferred_viewport_self_clip = clip;
                recording_context.deferred_viewport_effect = clip.and_then(|clip| {
                    let contract = self.effect_surface_authority?;
                    (contract.boundary_root() == key).then_some(())?;
                    super::PaintDeferredViewportEffectWitness::new(clip, contract.isolated_leaf())
                });
            }
            let mut native_preflight = if self.recording_mode == CoverageRecordingMode::MetadataOnly
            {
                node.element
                    .as_any()
                    .downcast_ref::<crate::view::base_component::Element>()
                    .and_then(|element| {
                        element.record_inline_root_preflight(
                            key,
                            properties,
                            contents_properties,
                            revision,
                            self.arena,
                            deferred_phase_root,
                            &recording_context,
                        )
                    })
            } else {
                None
            };
            let capability = native_preflight
                .as_ref()
                .map(|(capability, _)| *capability)
                .unwrap_or_else(|| {
                    node.element.shadow_paint_recording_capability(
                        self.arena,
                        deferred_phase_root,
                        &recording_context,
                    )
                });
            match capability {
                ShadowPaintRecordingCapability::Unsupported => {
                    self.push_legacy_boundary(key, stable_id, LegacyPaintReason::UnknownHost, order)
                }
                ShadowPaintRecordingCapability::Legacy(blocker) => {
                    self.push_legacy_boundary(key, stable_id, legacy_reason(blocker), order)
                }
                ShadowPaintRecordingCapability::CulledSubtree => {
                    match self.culled_subtree_boundary(key) {
                        Some(CulledSubtreeBoundary::Deferred) => self.push_legacy_boundary(
                            key,
                            stable_id,
                            LegacyPaintReason::Deferred,
                            order,
                        ),
                        Some(CulledSubtreeBoundary::Property(reason)) => {
                            self.push_legacy_boundary(key, stable_id, reason, order)
                        }
                        None => self.items.push(PaintCoverageItem::CulledSubtree {
                            order,
                            owner: key,
                            stable_id,
                            properties,
                            content_revision: revision,
                        }),
                    }
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
                    let retained_child_mask = node
                        .element
                        .retained_child_mask_plan(self.arena, &recording_context);
                    if retained_child_mask.as_ref().is_some_and(|mask| {
                        !mask.is_canonical_for_children(node.element.children())
                    }) {
                        self.push_legacy_boundary(
                            key,
                            stable_id,
                            LegacyPaintReason::ChildClip,
                            order,
                        );
                        return;
                    }
                    let plan = match self.recording_mode {
                        CoverageRecordingMode::MetadataOnly => {
                            let _profile = crate::view::paint::work_profile::scope("metadata_hook");
                            let plan = match native_preflight.take() {
                                Some((_, plan)) => plan,
                                None => node.element.record_shadow_paint_metadata_plan(
                                    key,
                                    properties,
                                    contents_properties,
                                    revision,
                                    self.arena,
                                    &recording_context,
                                ),
                            };
                            let Some(plan) = plan else {
                                self.push_legacy_boundary(
                                    key,
                                    stable_id,
                                    LegacyPaintReason::MissingPaintIdentity,
                                    order,
                                );
                                return;
                            };
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
                            let Some(plan) = replayed.or_else(|| {
                                node.element.record_shadow_paint_artifact_plan(
                                    key,
                                    properties,
                                    contents_properties,
                                    revision,
                                    self.arena,
                                    &recording_context,
                                )
                            }) else {
                                self.push_legacy_boundary(
                                    key,
                                    stable_id,
                                    LegacyPaintReason::MissingPaintIdentity,
                                    order,
                                );
                                return;
                            };
                            if !was_replayed && node.element.supports_retained_command_replay() {
                                if let Some(cache) = self.recording_cache.as_deref_mut() {
                                    cache.insert(key, plan.clone());
                                }
                            }
                            let Some(before_children) = self.record_artifact_plan_side(
                                key,
                                stable_id,
                                root_index,
                                path,
                                PaintNodePhase::BeforeChildren,
                                plan.before_children,
                            ) else {
                                return;
                            };
                            let Some(after_children) = self.record_artifact_plan_side(
                                key,
                                stable_id,
                                root_index,
                                path,
                                PaintNodePhase::AfterChildren,
                                plan.after_children,
                            ) else {
                                return;
                            };
                            RecordedPlan {
                                before_children: RecordedPlanSide::Full(
                                    before_children.into_iter(),
                                ),
                                after_children: RecordedPlanSide::Full(after_children.into_iter()),
                            }
                        }
                    };
                    if plan.before_children.len() == 0 && plan.after_children.len() == 0 {
                        self.push_legacy_boundary(
                            key,
                            stable_id,
                            LegacyPaintReason::MissingPaintIdentity,
                            order,
                        );
                        return;
                    }
                    let mut seen_slots = SeenChunkSlots::default();
                    let Some(before_children) = self.prepare_plan_side(
                        key,
                        stable_id,
                        &order,
                        PaintNodePhase::BeforeChildren,
                        properties,
                        contents_properties,
                        revision,
                        plan.before_children,
                        &mut seen_slots,
                    ) else {
                        return;
                    };
                    let Some(after_children) = self.prepare_plan_side(
                        key,
                        stable_id,
                        &order,
                        PaintNodePhase::AfterChildren,
                        properties,
                        contents_properties,
                        revision,
                        plan.after_children,
                        &mut seen_slots,
                    ) else {
                        return;
                    };
                    self.items.extend(before_children);
                    let children = node.element.children();
                    let in_scope_children = retained_child_mask
                        .as_ref()
                        .map_or(children, |mask| mask.in_scope_children());
                    for (schedule_index, &child) in in_scope_children.iter().enumerate() {
                        let Some(index) = (if retained_child_mask.is_none() {
                            Some(schedule_index)
                        } else {
                            children.iter().position(|candidate| *candidate == child)
                        }) else {
                            self.push_legacy_boundary(
                                key,
                                stable_id,
                                LegacyPaintReason::ChildClip,
                                order.clone(),
                            );
                            return;
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
                    self.items.extend(after_children);
                    if let Some(mask) = retained_child_mask {
                        for &child in mask.overflow_children() {
                            let Some(index) =
                                children.iter().position(|candidate| *candidate == child)
                            else {
                                self.push_legacy_boundary(
                                    key,
                                    stable_id,
                                    LegacyPaintReason::ChildClip,
                                    order.clone(),
                                );
                                return;
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
                    }
                }
            }
        }

        fn record_artifact_plan_side(
            &mut self,
            key: NodeKey,
            stable_id: u64,
            root_index: usize,
            path: &[usize],
            phase: PaintNodePhase,
            artifacts: Vec<super::PaintArtifact>,
        ) -> Option<Vec<RecordedPlanItem>> {
            let mut recorded = Vec::with_capacity(artifacts.len());
            for artifact in artifacts {
                let slot = artifact.chunks.first().map_or(0, |chunk| chunk.id.slot);
                let order = CoverageOrder::chunk(root_index, path, phase, slot);
                let [artifact_chunk] = artifact.chunks.as_slice() else {
                    self.reject_invalid_chunk(
                        key,
                        stable_id,
                        order,
                        PaintCoverageValidationError::InvalidArtifactChunkCount {
                            node: key,
                            actual: artifact.chunks.len(),
                        },
                    );
                    return None;
                };
                if artifact_chunk.op_range != (0..artifact.ops.len()) {
                    self.reject_invalid_chunk(
                        key,
                        stable_id,
                        order,
                        PaintCoverageValidationError::InvalidArtifactOpRange {
                            node: key,
                            start: artifact_chunk.op_range.start,
                            end: artifact_chunk.op_range.end,
                            op_count: artifact.ops.len(),
                        },
                    );
                    return None;
                }
                recorded.push(RecordedPlanItem {
                    chunk: PaintChunkMetadata {
                        id: artifact_chunk.id,
                        owner: artifact_chunk.owner,
                        bounds: artifact_chunk.bounds,
                        properties: artifact_chunk.properties,
                        content_revision: artifact_chunk.content_revision,
                        payload_identity: artifact_chunk.payload_identity.clone(),
                    },
                    ops: Some(artifact.ops.into()),
                });
            }
            Some(recorded)
        }

        #[allow(clippy::too_many_arguments)]
        fn prepare_plan_side(
            &mut self,
            key: NodeKey,
            stable_id: u64,
            node_order: &CoverageOrder,
            phase: PaintNodePhase,
            self_properties: crate::view::compositor::property_tree::PropertyTreeState,
            contents_properties: crate::view::compositor::property_tree::PropertyTreeState,
            expected_revision: PaintContentRevision,
            items: RecordedPlanSide,
            seen_slots: &mut SeenChunkSlots,
        ) -> Option<PreparedPlanSide> {
            let _profile = crate::view::paint::work_profile::scope("prepare_plan_side");
            let mut prepared = PreparedPlanSide {
                first: None,
                remaining: Vec::with_capacity(items.len().saturating_sub(1)),
            };
            for RecordedPlanItem { chunk, ops } in items {
                let order = node_order.for_chunk(phase, chunk.id.slot);
                let Some(chunk) = self.validate_chunk_identity(
                    key,
                    stable_id,
                    &order,
                    phase,
                    self_properties,
                    contents_properties,
                    expected_revision,
                    chunk,
                ) else {
                    return None;
                };
                if !seen_slots.insert((phase, chunk.id.slot)) {
                    self.reject_invalid_chunk(
                        key,
                        stable_id,
                        order,
                        PaintCoverageValidationError::DuplicateChunkSlot {
                            node: key,
                            phase,
                            slot: chunk.id.slot,
                        },
                    );
                    return None;
                }
                let Some(clip_snapshot) = self.clip_snapshot_for(chunk.properties) else {
                    self.reject_invalid_chunk(
                        key,
                        stable_id,
                        order,
                        PaintCoverageValidationError::InvalidClipSnapshot(key),
                    );
                    return None;
                };
                let Some(effect_snapshot) = self.effect_snapshot_for(chunk.properties) else {
                    self.reject_invalid_chunk(
                        key,
                        stable_id,
                        order,
                        PaintCoverageValidationError::InvalidEffectSnapshot(key),
                    );
                    return None;
                };
                let scope = match self.owner_scope_for(key) {
                    Ok(scope) => scope,
                    Err(error) => {
                        self.reject_invalid_chunk(key, stable_id, order.clone(), error);
                        return None;
                    }
                };
                // Identity validation above binds this chunk to exactly one
                // of the owner's two states. A replayed scope compared both
                // fresh chains, so its immutable endpoint storage is equivalent
                // to the live chains just validated here.
                let endpoint = match chunk.id.scope {
                    PaintPropertyScope::SelfPaint => 0,
                    PaintPropertyScope::Contents => 1,
                };
                drop((clip_snapshot, effect_snapshot));
                prepared.push(PaintCoverageItem::ArtifactChunk {
                    order,
                    chunk,
                    clip_snapshot: scope.clips[endpoint].clone(),
                    effect_snapshot: scope.effects[endpoint].clone(),
                    owner_scope: scope,
                    ops,
                });
            }
            Some(prepared)
        }

        fn validate_chunk_identity(
            &mut self,
            key: NodeKey,
            stable_id: u64,
            order: &CoverageOrder,
            expected_phase: PaintNodePhase,
            self_properties: crate::view::compositor::property_tree::PropertyTreeState,
            contents_properties: crate::view::compositor::property_tree::PropertyTreeState,
            expected_revision: PaintContentRevision,
            chunk: PaintChunkMetadata,
        ) -> Option<PaintChunkMetadata> {
            let _profile = crate::view::paint::work_profile::scope("validate_chunk_identity");
            let expected_properties = match chunk.id.scope {
                PaintPropertyScope::SelfPaint => self_properties,
                PaintPropertyScope::Contents => contents_properties,
            };
            let error = if !super::has_canonical_paint_bounds(chunk.bounds) {
                Some(PaintCoverageValidationError::InvalidChunkBounds(key))
            } else if chunk.id.phase != expected_phase {
                Some(PaintCoverageValidationError::InvalidChunkPhase {
                    node: key,
                    expected: expected_phase,
                    actual: chunk.id.phase,
                })
            } else if chunk.id.owner != key {
                Some(PaintCoverageValidationError::InvalidChunkIdOwner(key))
            } else if chunk.owner != key {
                Some(PaintCoverageValidationError::InvalidChunkOwner(key))
            } else if chunk.properties != expected_properties {
                Some(PaintCoverageValidationError::InvalidChunkProperties(key))
            } else if chunk.content_revision != expected_revision {
                Some(PaintCoverageValidationError::InvalidChunkRevision(key))
            } else {
                None
            };
            if let Some(error) = error {
                self.reject_invalid_chunk(key, stable_id, order.clone(), error);
                None
            } else {
                Some(chunk)
            }
        }

        fn clip_snapshot_for(
            &mut self,
            state: PropertyTreeState,
        ) -> Option<std::sync::Arc<[ClipNodeSnapshot]>> {
            if let Some(chain) = self.clip_chains.get(&state.clip) {
                return chain.clone();
            }
            let chain = (|| {
                let snapshots = self.properties.clip_snapshot_for(state.clip)?;
                let snapshots = match self.effect_surface_authority {
                    Some(authority) => authority.detach_clip_snapshot(&snapshots)?,
                    None => snapshots,
                };
                Some(std::sync::Arc::from(snapshots))
            })();
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
            let chain = (|| {
                let snapshots = self.properties.effect_snapshot_for(state.effect)?;
                let snapshots = match self.effect_surface_authority {
                    Some(authority) => {
                        authority.detach_effect_snapshot(state.effect, &snapshots)?
                    }
                    None => snapshots,
                };
                Some(std::sync::Arc::from(snapshots))
            })();
            self.effect_chains.insert(state.effect, chain.clone());
            chain
        }

        /// Resolve each owner once in this traversal. The immutable property
        /// trees and fixed detachment authority also make endpoint chain reuse
        /// exact; later conflicting owner observations still reject upstream.
        fn owner_scope_for(
            &mut self,
            leaf: NodeKey,
        ) -> Result<std::sync::Arc<PaintOwnerScope>, PaintCoverageValidationError> {
            let invalid = || PaintCoverageValidationError::InvalidOwnerSnapshot(leaf);
            self.scope_pending.clear();
            self.scope_seen.clear();
            let mut cursor = Some(leaf);
            while let Some(owner) = cursor {
                if let Some(scope) = self.owner_scopes.get(&owner) {
                    if self.scope_pending.len() + scope.depth > usize::from(u8::MAX) {
                        return Err(invalid());
                    }
                    break;
                }
                if !self.scope_seen.insert(owner)
                    || self.scope_pending.len() >= usize::from(u8::MAX)
                {
                    return Err(invalid());
                }
                self.scope_pending.push(owner);
                cursor = *self.owner_parents.get(&owner).ok_or_else(invalid)?;
            }
            while let Some(owner) = self.scope_pending.pop() {
                let parent_key = *self.owner_parents.get(&owner).ok_or_else(invalid)?;
                let parent = parent_key
                    .map(|key| self.owner_scopes.get(&key).cloned().ok_or_else(invalid))
                    .transpose()?;
                let state = *self
                    .observed_owner_property_states
                    .get(&owner)
                    .ok_or_else(invalid)?;
                // Store the two immutable endpoint chains directly. Copying
                // their complete ancestry into every owner merely duplicates
                // observations; materialization merges each shared chain once.
                let clips = [
                    self.clip_snapshot_for(state.paint)
                        .ok_or(PaintCoverageValidationError::InvalidClipSnapshot(owner))?,
                    self.clip_snapshot_for(state.descendants)
                        .ok_or(PaintCoverageValidationError::InvalidClipSnapshot(owner))?,
                ];
                let effects = [
                    self.effect_snapshot_for(state.paint)
                        .ok_or(PaintCoverageValidationError::InvalidEffectSnapshot(owner))?,
                    self.effect_snapshot_for(state.descendants)
                        .ok_or(PaintCoverageValidationError::InvalidEffectSnapshot(owner))?,
                ];
                let scope = PaintOwnerScope {
                    topology: PaintOwnerSnapshot {
                        owner,
                        parent: parent_key,
                    },
                    state,
                    clips,
                    effects,
                    depth: parent.as_ref().map_or(1, |parent| parent.depth + 1),
                    parent,
                };
                let scope = match self.recording_cache.as_deref_mut() {
                    Some(cache) => cache.intern_scope(scope),
                    None => std::sync::Arc::new(scope),
                };
                self.owner_scopes.insert(owner, scope);
            }
            self.owner_scopes.get(&leaf).cloned().ok_or_else(invalid)
        }

        fn reject_invalid_chunk(
            &mut self,
            key: NodeKey,
            stable_id: u64,
            order: CoverageOrder,
            error: PaintCoverageValidationError,
        ) {
            self.validation_errors.push(error);
            self.push_legacy_boundary(
                key,
                stable_id,
                LegacyPaintReason::MissingPaintIdentity,
                order,
            );
        }

        fn push_legacy_boundary(
            &mut self,
            root: NodeKey,
            stable_id: u64,
            reason: LegacyPaintReason,
            order: CoverageOrder,
        ) {
            self.items.push(PaintCoverageItem::LegacyBoundary {
                order,
                root,
                stable_id,
                reason,
            });
        }
    }

    // The canonical topology walk has already measured this exact frame's
    // owner count. Reserve once instead of repeatedly moving wide owner states
    // as the metadata walk fills its maps.
    let owner_count = owner_parents.len();
    // Native owners normally have at most one chunk per paint phase. Reserve
    // both phases to avoid moving wide coverage items during the common walk;
    // custom multi-slot schedules may still grow without any count limit.
    manifest.items.reserve(owner_count.saturating_mul(2));
    let mut observed_owner_property_states =
        FxHashMap::with_capacity_and_hasher(owner_count, Default::default());
    let mut recorder = Recorder {
        scope_pending: Vec::new(),
        scope_seen: FxHashSet::default(),
        owner_scopes: FxHashMap::with_capacity_and_hasher(owner_count, Default::default()),
        clip_chains: FxHashMap::default(),
        effect_chains: FxHashMap::default(),
        recording_cache,
        arena,
        force_legacy_roots,
        deferred_roots: &deferred_set,
        properties: property_trees,
        owner_parents: &owner_parents,
        observed_owner_property_states: &mut observed_owner_property_states,
        generations: paint_generations,
        recording_mode,
        transform_surface_authority,
        surface_dag: initial_recording_context.surface_dag,
        effect_surface_authority,
        property_forest_ancestor_chain,
        baked_scroll_host_authority: initial_recording_context.baked_scroll_host,
        consumed_ancestor_property: initial_recording_context.consumed_ancestor_property,
        consumed_ancestor_property_stack: initial_recording_context
            .consumed_ancestor_property_stack,
        scroll_forest_host: initial_recording_context.scroll_forest_host,
        required_scroll_content_paint_offset_bits: initial_recording_context
            .required_scroll_content_paint_offset_bits,
        opacity_authority: initial_recording_context.opacity_authority,
        planned_boundary_cutouts,
        native_scroll_receiver,
        items: &mut manifest.items,
        validation_errors: &mut manifest.validation_errors,
    };
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
        for (deferred_index, deferred_root) in deferred_roots.into_iter().enumerate() {
            recorder.walk(
                deferred_root,
                roots.len(),
                &mut vec![deferred_index],
                true,
                &initial_recording_context,
            );
        }
    }
    drop(recorder);
    if let Some(receiver) = native_scroll_receiver
        && !manifest.items.iter().any(|item| {
            matches!(item, PaintCoverageItem::NativeScrollContentReceiver { cutout, .. } if *cutout == receiver)
        })
    {
        manifest
            .validation_errors
            .push(PaintCoverageValidationError::InvalidPlannedBoundary(
                receiver.witness.content_root(),
            ));
    }
    fn mark_legacy_coverage(
        arena: &NodeArena,
        key: NodeKey,
        root: NodeKey,
        deferred: &FxHashSet<NodeKey>,
        out: &mut FxHashSet<NodeKey>,
    ) {
        if key != root && deferred.contains(&key) {
            return;
        }
        let Some(node) = arena.get(key) else {
            return;
        };
        if !out.insert(key) {
            return;
        }
        for &child in node.element.children() {
            mark_legacy_coverage(arena, child, root, deferred, out);
        }
    }
    for item in &manifest.items {
        if let PaintCoverageItem::LegacyBoundary { root, reason, .. } = item {
            let coverage = manifest.legacy_coverage.entry(*reason).or_default();
            mark_legacy_coverage(arena, *root, *root, &deferred_set, coverage);
        }
    }
    manifest
}

fn legacy_reason(blocker: ShadowPaintBlocker) -> LegacyPaintReason {
    match blocker {
        ShadowPaintBlocker::Transform => LegacyPaintReason::Transform,
        ShadowPaintBlocker::BoxShadow => LegacyPaintReason::BoxShadow,
        ShadowPaintBlocker::InlineIfc => LegacyPaintReason::InlineIfc,
        ShadowPaintBlocker::ScrollContainer => LegacyPaintReason::ScrollContainer,
        ShadowPaintBlocker::SelfClip => LegacyPaintReason::SelfClip,
        ShadowPaintBlocker::ChildClip => LegacyPaintReason::ChildClip,
        ShadowPaintBlocker::Deferred => LegacyPaintReason::Deferred,
        ShadowPaintBlocker::LayoutTransition => LegacyPaintReason::LayoutTransition,
        ShadowPaintBlocker::StatefulPaint => LegacyPaintReason::StatefulPaint,
        ShadowPaintBlocker::MissingPreparedInlineDecoration => {
            LegacyPaintReason::MissingPreparedInlineDecoration
        }
        ShadowPaintBlocker::MissingPreparedInlineRoot => {
            LegacyPaintReason::MissingPreparedInlineRoot
        }
        ShadowPaintBlocker::MissingPreparedText => LegacyPaintReason::MissingPreparedText,
        ShadowPaintBlocker::MissingPreparedImage => LegacyPaintReason::MissingPreparedImage,
        ShadowPaintBlocker::MissingPreparedSvg => LegacyPaintReason::MissingPreparedSvg,
        ShadowPaintBlocker::TextAreaSelection => LegacyPaintReason::TextAreaSelection,
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod slot_storage_tests;
