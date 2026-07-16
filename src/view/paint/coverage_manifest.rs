#![allow(dead_code)]

use rustc_hash::{FxHashMap, FxHashSet};

use crate::view::base_component::{ShadowPaintBlocker, ShadowPaintRecordingCapability};
use crate::view::compositor::property_tree::{
    ClipNodeId, ClipNodeSnapshot, EffectNodeId, EffectNodeSnapshot, ScrollNodeId, TransformNodeId,
};
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::node_arena::{NodeArena, NodeKey};

use super::{
    LegacyPaintReason, PaintChunkMetadata, PaintContentRevision, PaintNodePhase, PaintNodePlan,
    PaintOwnerSnapshot, PaintPropertyScope, PaintRecordingContext,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CoverageOrder {
    pub(crate) root_index: usize,
    pub(crate) child_path: Vec<usize>,
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
            child_path: child_path.to_vec(),
            phase: PaintNodePhase::BeforeChildren,
            slot: 0,
        }
    }

    fn chunk(root_index: usize, child_path: &[usize], phase: PaintNodePhase, slot: u16) -> Self {
        Self {
            root_index,
            child_path: child_path.to_vec(),
            phase,
            slot,
        }
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
pub(crate) struct NestedScrollContentReceiverCutout {
    pub(super) stable_id: u64,
    pub(super) witness: super::PaintNestedScrollContentWitness,
}

#[derive(Clone, Debug)]
pub(crate) enum PaintCoverageItem {
    ArtifactChunk {
        order: CoverageOrder,
        chunk: PaintChunkMetadata,
        clip_snapshot: Vec<ClipNodeSnapshot>,
        effect_snapshot: Vec<EffectNodeSnapshot>,
        owner_snapshot: Vec<PaintOwnerSnapshot>,
        ops: Option<Vec<super::PaintOp>>,
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
        span_index: usize,
        before_promoted: Option<NodeKey>,
        after_promoted: Option<NodeKey>,
    },
    PromotedBoundary {
        order: CoverageOrder,
        root: NodeKey,
        stable_id: u64,
    },
    PlannedBoundary {
        order: CoverageOrder,
        boundary: PlannedBoundary,
    },
    NestedScrollContentReceiver {
        order: CoverageOrder,
        cutout: NestedScrollContentReceiverCutout,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PaintCoverageValidationError {
    MissingNode(NodeKey),
    DuplicateNodeKey(NodeKey),
    DuplicateStableId(u64),
    MissingPromotedStableId(u64),
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

#[cfg(test)]
pub(crate) fn nested_scroll_receiver_manifest_for_layerizer_test(
    outer: NodeKey,
    inner: NodeKey,
    content: NodeKey,
    stable_id: u64,
) -> PaintCoverageManifest {
    let witness = super::PaintNestedScrollContentWitness::for_layerizer_test(outer, inner, content)
        .expect("layerizer receiver test uses pairwise-distinct roots");
    PaintCoverageManifest {
        items: vec![PaintCoverageItem::NestedScrollContentReceiver {
            order: CoverageOrder::node(0, &[]),
            cutout: NestedScrollContentReceiverCutout { stable_id, witness },
        }],
        covered_nodes: FxHashSet::from_iter([outer, inner, content]),
        ..PaintCoverageManifest::default()
    }
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
    pub(crate) promoted_boundaries: usize,
    pub(crate) validation_errors: usize,
    pub(crate) authority_eligible: bool,
    pub(crate) authority_ineligible_reasons: Vec<&'static str>,
    covered_node_keys: FxHashSet<NodeKey>,
    artifact_node_keys: FxHashSet<NodeKey>,
    culled_node_keys: FxHashSet<NodeKey>,
    legacy_node_keys: FxHashSet<NodeKey>,
    promoted_node_keys: FxHashSet<NodeKey>,
}

impl PaintCoverageManifest {
    pub(crate) fn stats(&self) -> PaintCoverageStats {
        let mut nodes = FxHashSet::default();
        let mut artifact_nodes = FxHashSet::default();
        let mut legacy = FxHashSet::default();
        let mut promoted = FxHashSet::default();
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
                PaintCoverageItem::LegacyBoundary {
                    root, span_index, ..
                } => {
                    nodes.insert(*root);
                    if *span_index == 0 {
                        legacy.insert(*root);
                    }
                }
                PaintCoverageItem::PromotedBoundary { root, .. } => {
                    nodes.insert(*root);
                    promoted.insert(*root);
                }
                PaintCoverageItem::PlannedBoundary { boundary, .. } => {
                    nodes.insert(boundary.root);
                }
                PaintCoverageItem::NestedScrollContentReceiver { cutout, .. } => {
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
            promoted_boundaries: promoted.len(),
            validation_errors: self.validation_errors.len(),
            authority_eligible: false,
            authority_ineligible_reasons,
            covered_node_keys: self.covered_nodes.clone(),
            artifact_node_keys: artifact_nodes,
            culled_node_keys: culled,
            legacy_node_keys: legacy_nodes,
            promoted_node_keys: promoted,
        }
    }
}

impl PaintCoverageStats {
    pub(crate) fn merge(&mut self, other: Self) {
        self.covered_node_keys.extend(other.covered_node_keys);
        self.artifact_node_keys.extend(other.artifact_node_keys);
        self.culled_node_keys.extend(other.culled_node_keys);
        self.legacy_node_keys.extend(other.legacy_node_keys);
        self.promoted_node_keys.extend(other.promoted_node_keys);
        self.total_nodes = self.covered_node_keys.len();
        self.artifact_nodes = self.artifact_node_keys.len();
        self.culled_subtrees = self.culled_node_keys.len();
        self.legacy_covered_nodes = self.legacy_node_keys.len();
        self.promoted_boundaries = self.promoted_node_keys.len();
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

pub(crate) fn record_coverage_manifest(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    promoted_root_exemption: Option<NodeKey>,
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
        promoted_node_ids,
        promoted_root_exemption,
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
    promoted_node_ids: &FxHashSet<u64>,
    promoted_root_exemption: Option<NodeKey>,
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
        promoted_node_ids,
        promoted_root_exemption,
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
pub(super) fn record_coverage_manifest_with_property_authorities(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    promoted_root_exemption: Option<NodeKey>,
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
        promoted_node_ids,
        promoted_root_exemption,
        force_legacy_roots,
        emit_deferred_late,
        recording_mode,
        property_trees,
        paint_generations,
        initial_recording_context,
        transform_surface_authority,
        effect_surface_authority,
        planned_boundary_cutouts,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn record_coverage_manifest_with_nested_scroll_receiver(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    recording_mode: CoverageRecordingMode,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    initial_recording_context: PaintRecordingContext,
    receiver: NestedScrollContentReceiverCutout,
) -> PaintCoverageManifest {
    record_coverage_manifest_with_property_authorities_impl(
        arena,
        roots,
        promoted_node_ids,
        None,
        false,
        true,
        recording_mode,
        property_trees,
        paint_generations,
        initial_recording_context,
        None,
        None,
        &PlannedBoundaryCutoutSet::default(),
        Some(receiver),
    )
}

#[allow(clippy::too_many_arguments)]
fn record_coverage_manifest_with_property_authorities_impl(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    promoted_root_exemption: Option<NodeKey>,
    force_legacy_roots: bool,
    emit_deferred_late: bool,
    recording_mode: CoverageRecordingMode,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    initial_recording_context: PaintRecordingContext,
    transform_surface_authority: Option<super::PaintTransformSurfaceWitness>,
    effect_surface_authority: Option<&super::EffectPropertySurfaceArtifactContract>,
    planned_boundary_cutouts: &PlannedBoundaryCutoutSet,
    nested_scroll_receiver: Option<NestedScrollContentReceiverCutout>,
) -> PaintCoverageManifest {
    let mut manifest = PaintCoverageManifest::default();
    let mut seen_keys = FxHashSet::default();
    let mut owner_parents = FxHashMap::<NodeKey, Option<NodeKey>>::default();
    let mut resolved_nodes = FxHashSet::default();
    let mut stable_keys = FxHashMap::<u64, NodeKey>::default();
    let mut stack = roots
        .iter()
        .copied()
        .map(|root| (root, None))
        .collect::<Vec<_>>();
    while let Some((key, traversal_parent)) = stack.pop() {
        if !seen_keys.insert(key) {
            manifest
                .validation_errors
                .push(PaintCoverageValidationError::DuplicateNodeKey(key));
            continue;
        }
        owner_parents.insert(key, traversal_parent);
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
    for &stable_id in promoted_node_ids {
        if !stable_keys.contains_key(&stable_id) {
            manifest
                .validation_errors
                .push(PaintCoverageValidationError::MissingPromotedStableId(
                    stable_id,
                ));
        }
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
    let mut deferred_seen = FxHashSet::default();
    for &root in roots {
        collect_deferred(arena, root, &mut deferred_seen, &mut deferred_roots);
    }
    let deferred_set = deferred_roots.iter().copied().collect::<FxHashSet<_>>();

    struct Recorder<'a> {
        arena: &'a NodeArena,
        promoted: &'a FxHashSet<u64>,
        exemption: Option<NodeKey>,
        force_legacy_roots: bool,
        deferred_roots: &'a FxHashSet<NodeKey>,
        properties: &'a PropertyTrees,
        owner_parents: &'a FxHashMap<NodeKey, Option<NodeKey>>,
        generations: &'a PaintGenerationTracker,
        recording_mode: CoverageRecordingMode,
        transform_surface_authority: Option<super::PaintTransformSurfaceWitness>,
        effect_surface_authority: Option<&'a super::EffectPropertySurfaceArtifactContract>,
        baked_scroll_host_authority: Option<super::PaintBakedScrollHostWitness>,
        consumed_ancestor_property: Option<super::ConsumedAncestorProperty>,
        consumed_ancestor_property_stack: Option<super::ConsumedAncestorPropertyStackWitness>,
        nested_scroll_content: Option<super::PaintNestedScrollContentWitness>,
        nested_scroll_host: Option<super::PaintNestedScrollContentWitness>,
        scroll_text_area_subtree: Option<super::PaintScrollTextAreaSubtreeWitness>,
        baked_scroll_text_area_subtree: Option<super::PaintScrollTextAreaSubtreeWitness>,
        scroll_atomic_projection_text_area_subtree:
            Option<super::PaintScrollAtomicProjectionTextAreaRecorderWitness>,
        baked_scroll_atomic_projection_text_area_subtree:
            Option<super::PaintScrollAtomicProjectionTextAreaRecorderWitness>,
        scroll_interactive_text_area_subtree:
            Option<super::PaintScrollInteractiveTextAreaSubtreeWitness>,
        baked_scroll_interactive_text_area_subtree:
            Option<super::PaintScrollInteractiveTextAreaSubtreeWitness>,
        required_scroll_content_paint_offset_bits: Option<[u32; 2]>,
        opacity_authority: super::PaintOpacityAuthority,
        planned_boundary_cutouts: &'a PlannedBoundaryCutoutSet,
        nested_scroll_receiver: Option<NestedScrollContentReceiverCutout>,
        items: &'a mut Vec<PaintCoverageItem>,
        validation_errors: &'a mut Vec<PaintCoverageValidationError>,
    }
    struct RecordedPlanItem {
        chunk: PaintChunkMetadata,
        ops: Option<Vec<super::PaintOp>>,
    }
    enum CulledSubtreeBoundary {
        Promoted { root: NodeKey, stable_id: u64 },
        Deferred,
        Property(LegacyPaintReason),
    }
    impl Recorder<'_> {
        fn culled_subtree_boundary(&self, root: NodeKey) -> Option<CulledSubtreeBoundary> {
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
                    return Some(CulledSubtreeBoundary::Deferred);
                }
                let Some(node) = self.arena.get(key) else {
                    continue;
                };
                if let Some(state) = self.properties.node_state_for(key) {
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
                let stable_id = node.element.stable_id();
                if self.promoted.contains(&stable_id) {
                    return Some(CulledSubtreeBoundary::Promoted {
                        root: key,
                        stable_id,
                    });
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
            parent_recording_context: PaintRecordingContext,
        ) {
            let Some(node) = self.arena.get(key) else {
                return;
            };
            let invocation_exemption = self.exemption == Some(key);
            if self.deferred_roots.contains(&key) && !deferred_phase_root && !invocation_exemption {
                return;
            }
            let stable_id = node.element.stable_id();
            let order = CoverageOrder::node(root_index, path);
            if let Some(cutout) = self.nested_scroll_receiver
                && key == cutout.witness.content_root()
            {
                if cutout.stable_id != stable_id
                    || self.owner_parents.get(&key).copied().flatten()
                        != Some(cutout.witness.boundary_root())
                    || !node.element.children().is_empty()
                {
                    self.validation_errors
                        .push(PaintCoverageValidationError::InvalidPlannedBoundary(key));
                    return;
                }
                self.items
                    .push(PaintCoverageItem::NestedScrollContentReceiver { order, cutout });
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
            if invocation_exemption {
                self.push_legacy_spans(key, stable_id, LegacyPaintReason::Promoted, order);
                return;
            }
            if self.exemption != Some(key) && self.promoted.contains(&stable_id) {
                self.items.push(PaintCoverageItem::PromotedBoundary {
                    order,
                    root: key,
                    stable_id,
                });
                return;
            }
            if self.force_legacy_roots && path.is_empty() && !node.element.children().is_empty() {
                self.push_legacy_spans(key, stable_id, LegacyPaintReason::HasChildren, order);
                return;
            }
            let Some(property_states) = self.properties.node_state_for(key) else {
                self.push_legacy_spans(
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
                self.push_legacy_spans(
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
                self.push_legacy_spans(
                    key,
                    stable_id,
                    LegacyPaintReason::MissingPaintIdentity,
                    order,
                );
                return;
            }
            recording_context.is_frame_root = path.is_empty() && !deferred_phase_root;
            recording_context.recording_owner = Some(key);
            recording_context.recording_owner_stable_id = Some(stable_id);
            recording_context.authoritative_self_clip = None;
            // A component hook cannot mint or retarget consumed-property
            // authority.  Rebind the recorder-owned witness to this canonical
            // traversal owner after the hook returns.
            recording_context.consumed_ancestor_property = self
                .consumed_ancestor_property
                .map(|witness| witness.for_target(key));
            recording_context.consumed_ancestor_property_stack = self
                .consumed_ancestor_property_stack
                .map(|witness| witness.for_target(key));
            recording_context.nested_scroll_content = self.nested_scroll_content;
            recording_context.nested_scroll_host = self.nested_scroll_host;
            recording_context.scroll_text_area_subtree = self
                .scroll_text_area_subtree
                .map(|witness| witness.for_target(key));
            recording_context.baked_scroll_text_area_subtree = self
                .baked_scroll_text_area_subtree
                .map(|witness| witness.for_target(key));
            recording_context.scroll_atomic_projection_text_area_subtree = self
                .scroll_atomic_projection_text_area_subtree
                .map(|witness| witness.for_target(key));
            recording_context.baked_scroll_atomic_projection_text_area_subtree = self
                .baked_scroll_atomic_projection_text_area_subtree
                .map(|witness| witness.for_target(key));
            recording_context.scroll_interactive_text_area_subtree = self
                .scroll_interactive_text_area_subtree
                .map(|witness| witness.for_target(key));
            recording_context.baked_scroll_interactive_text_area_subtree = self
                .baked_scroll_interactive_text_area_subtree
                .map(|witness| witness.for_target(key));
            // Opacity authority is a recorder policy, not ambient component
            // state. Rebind it after every node/child hook so a component
            // cannot bake a root-group opacity that the compositor will apply
            // again at the isolation boundary.
            recording_context.opacity_authority = self.opacity_authority;
            // Never inherit ambient transform authority. Only this recorder's
            // canonical surface policy may bind a witness to the current
            // traversal owner and exact inherited transform boundary.
            recording_context.transform_surface = None;
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
            let Some(mut properties) =
                recording_context.project_consumed_ancestor_property(live_properties)
            else {
                self.push_legacy_spans(
                    key,
                    stable_id,
                    LegacyPaintReason::MissingPaintIdentity,
                    order,
                );
                return;
            };
            let Some(mut contents_properties) =
                recording_context.project_consumed_ancestor_property(live_contents_properties)
            else {
                self.push_legacy_spans(
                    key,
                    stable_id,
                    LegacyPaintReason::MissingPaintIdentity,
                    order,
                );
                return;
            };
            if let Some(authority) = self.effect_surface_authority {
                let Some(paint_chain) = self.properties.clip_snapshot_for(properties.clip) else {
                    self.push_legacy_spans(
                        key,
                        stable_id,
                        LegacyPaintReason::MissingPaintIdentity,
                        order,
                    );
                    return;
                };
                let Some(projected) = authority.project_clip_leaf(properties.clip, &paint_chain)
                else {
                    self.push_legacy_spans(
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
                    self.push_legacy_spans(
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
                    self.push_legacy_spans(
                        key,
                        stable_id,
                        LegacyPaintReason::MissingPaintIdentity,
                        order,
                    );
                    return;
                };
                contents_properties.clip = projected;
            }
            recording_context.authoritative_self_clip = self
                .properties
                .authoritative_self_clip_for_owner(key, live_properties);
            match node.element.shadow_paint_recording_capability(
                self.arena,
                deferred_phase_root,
                recording_context,
            ) {
                ShadowPaintRecordingCapability::Unsupported => {
                    self.push_legacy_spans(key, stable_id, LegacyPaintReason::UnknownHost, order)
                }
                ShadowPaintRecordingCapability::Legacy(blocker) => {
                    self.push_legacy_spans(key, stable_id, legacy_reason(blocker), order)
                }
                ShadowPaintRecordingCapability::CulledSubtree => {
                    match self.culled_subtree_boundary(key) {
                        Some(CulledSubtreeBoundary::Promoted { root, stable_id }) => {
                            self.items.push(PaintCoverageItem::PromotedBoundary {
                                order,
                                root,
                                stable_id,
                            })
                        }
                        Some(CulledSubtreeBoundary::Deferred) => self.push_legacy_spans(
                            key,
                            stable_id,
                            LegacyPaintReason::Deferred,
                            order,
                        ),
                        Some(CulledSubtreeBoundary::Property(reason)) => {
                            self.push_legacy_spans(key, stable_id, reason, order)
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
                    let children = node.element.children().to_vec();
                    for (index, child) in children.into_iter().enumerate() {
                        let child_recording_context =
                            node.element.shadow_paint_recording_context_for_child(
                                child,
                                self.arena,
                                recording_context,
                            );
                        path.push(index);
                        self.walk(child, root_index, path, false, child_recording_context);
                        path.pop();
                    }
                }
                ShadowPaintRecordingCapability::Recordable => {
                    let plan = match self.recording_mode {
                        CoverageRecordingMode::MetadataOnly => {
                            let Some(plan) = node.element.record_shadow_paint_metadata_plan(
                                key,
                                properties,
                                contents_properties,
                                revision,
                                self.arena,
                                recording_context,
                            ) else {
                                self.push_legacy_spans(
                                    key,
                                    stable_id,
                                    LegacyPaintReason::MissingPaintIdentity,
                                    order,
                                );
                                return;
                            };
                            PaintNodePlan {
                                before_children: plan
                                    .before_children
                                    .into_iter()
                                    .map(|chunk| RecordedPlanItem { chunk, ops: None })
                                    .collect(),
                                after_children: plan
                                    .after_children
                                    .into_iter()
                                    .map(|chunk| RecordedPlanItem { chunk, ops: None })
                                    .collect(),
                            }
                        }
                        CoverageRecordingMode::FullArtifact => {
                            let Some(plan) = node.element.record_shadow_paint_artifact_plan(
                                key,
                                properties,
                                contents_properties,
                                revision,
                                self.arena,
                                recording_context,
                            ) else {
                                self.push_legacy_spans(
                                    key,
                                    stable_id,
                                    LegacyPaintReason::MissingPaintIdentity,
                                    order,
                                );
                                return;
                            };
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
                            PaintNodePlan {
                                before_children,
                                after_children,
                            }
                        }
                    };
                    if plan.is_empty() {
                        self.push_legacy_spans(
                            key,
                            stable_id,
                            LegacyPaintReason::MissingPaintIdentity,
                            order,
                        );
                        return;
                    }
                    let mut seen_slots = FxHashSet::default();
                    let Some(before_children) = self.prepare_plan_side(
                        key,
                        stable_id,
                        root_index,
                        path,
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
                        root_index,
                        path,
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
                    let children = node.element.children().to_vec();
                    for (index, child) in children.into_iter().enumerate() {
                        let child_recording_context =
                            node.element.shadow_paint_recording_context_for_child(
                                child,
                                self.arena,
                                recording_context,
                            );
                        path.push(index);
                        self.walk(child, root_index, path, false, child_recording_context);
                        path.pop();
                    }
                    self.items.extend(after_children);
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
                    ops: Some(artifact.ops),
                });
            }
            Some(recorded)
        }

        #[allow(clippy::too_many_arguments)]
        fn prepare_plan_side(
            &mut self,
            key: NodeKey,
            stable_id: u64,
            root_index: usize,
            path: &[usize],
            phase: PaintNodePhase,
            self_properties: crate::view::compositor::property_tree::PropertyTreeState,
            contents_properties: crate::view::compositor::property_tree::PropertyTreeState,
            expected_revision: PaintContentRevision,
            items: Vec<RecordedPlanItem>,
            seen_slots: &mut FxHashSet<(PaintNodePhase, u16)>,
        ) -> Option<Vec<PaintCoverageItem>> {
            let mut prepared = Vec::with_capacity(items.len());
            for RecordedPlanItem { chunk, ops } in items {
                let order = CoverageOrder::chunk(root_index, path, phase, chunk.id.slot);
                let Some(chunk) = self.validate_chunk_identity(
                    key,
                    stable_id,
                    order.clone(),
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
                let Some(mut clip_snapshot) =
                    self.properties.clip_snapshot_for(chunk.properties.clip)
                else {
                    self.reject_invalid_chunk(
                        key,
                        stable_id,
                        order,
                        PaintCoverageValidationError::InvalidClipSnapshot(key),
                    );
                    return None;
                };
                let Some(mut effect_snapshot) =
                    self.properties.effect_snapshot_for(chunk.properties.effect)
                else {
                    self.reject_invalid_chunk(
                        key,
                        stable_id,
                        order,
                        PaintCoverageValidationError::InvalidEffectSnapshot(key),
                    );
                    return None;
                };
                if let Some(authority) = self.scroll_text_area_subtree {
                    let Some(detached) = authority.detach_clip_snapshot(&clip_snapshot) else {
                        self.reject_invalid_chunk(
                            key,
                            stable_id,
                            order.clone(),
                            PaintCoverageValidationError::InvalidClipSnapshot(key),
                        );
                        return None;
                    };
                    clip_snapshot = detached;
                }
                if let Some(authority) = self.scroll_atomic_projection_text_area_subtree {
                    let Some(detached) = authority.detach_clip_snapshot(&clip_snapshot) else {
                        self.reject_invalid_chunk(
                            key,
                            stable_id,
                            order.clone(),
                            PaintCoverageValidationError::InvalidClipSnapshot(key),
                        );
                        return None;
                    };
                    clip_snapshot = detached;
                }
                if let Some(authority) = self.scroll_interactive_text_area_subtree {
                    let Some(detached) = authority.detach_clip_snapshot(&clip_snapshot) else {
                        self.reject_invalid_chunk(
                            key,
                            stable_id,
                            order.clone(),
                            PaintCoverageValidationError::InvalidClipSnapshot(key),
                        );
                        return None;
                    };
                    clip_snapshot = detached;
                }
                if let Some(authority) = self.effect_surface_authority {
                    let Some(detached) =
                        authority.detach_effect_snapshot(chunk.properties.effect, &effect_snapshot)
                    else {
                        self.reject_invalid_chunk(
                            key,
                            stable_id,
                            order.clone(),
                            PaintCoverageValidationError::InvalidEffectSnapshot(key),
                        );
                        return None;
                    };
                    effect_snapshot = detached;
                    let Some(detached) = authority.detach_clip_snapshot(&clip_snapshot) else {
                        self.reject_invalid_chunk(
                            key,
                            stable_id,
                            order.clone(),
                            PaintCoverageValidationError::InvalidClipSnapshot(key),
                        );
                        return None;
                    };
                    clip_snapshot = detached;
                }
                let Some(owner_snapshot) = self.owner_snapshot_for(key) else {
                    self.reject_invalid_chunk(
                        key,
                        stable_id,
                        order,
                        PaintCoverageValidationError::InvalidOwnerSnapshot(key),
                    );
                    return None;
                };
                prepared.push(PaintCoverageItem::ArtifactChunk {
                    order,
                    chunk,
                    clip_snapshot,
                    effect_snapshot,
                    owner_snapshot,
                    ops,
                });
            }
            Some(prepared)
        }

        fn validate_chunk_identity(
            &mut self,
            key: NodeKey,
            stable_id: u64,
            order: CoverageOrder,
            expected_phase: PaintNodePhase,
            self_properties: crate::view::compositor::property_tree::PropertyTreeState,
            contents_properties: crate::view::compositor::property_tree::PropertyTreeState,
            expected_revision: PaintContentRevision,
            chunk: PaintChunkMetadata,
        ) -> Option<PaintChunkMetadata> {
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
                self.reject_invalid_chunk(key, stable_id, order, error);
                None
            } else {
                Some(chunk)
            }
        }

        fn owner_snapshot_for(&self, leaf: NodeKey) -> Option<Vec<PaintOwnerSnapshot>> {
            let mut snapshots = Vec::new();
            let mut seen = FxHashSet::default();
            let mut cursor = Some(leaf);
            while let Some(owner) = cursor {
                if !seen.insert(owner) || snapshots.len() >= usize::from(u8::MAX) {
                    return None;
                }
                let parent = *self.owner_parents.get(&owner)?;
                snapshots.push(PaintOwnerSnapshot { owner, parent });
                cursor = parent;
            }
            Some(snapshots)
        }

        fn reject_invalid_chunk(
            &mut self,
            key: NodeKey,
            stable_id: u64,
            order: CoverageOrder,
            error: PaintCoverageValidationError,
        ) {
            self.validation_errors.push(error);
            self.push_legacy_spans(
                key,
                stable_id,
                LegacyPaintReason::MissingPaintIdentity,
                order,
            );
        }

        fn push_legacy_spans(
            &mut self,
            root: NodeKey,
            stable_id: u64,
            reason: LegacyPaintReason,
            order: CoverageOrder,
        ) {
            let mut cutouts = Vec::new();
            self.collect_promoted_cutouts(root, &mut cutouts);
            for span_index in 0..=cutouts.len() {
                let before_promoted = span_index
                    .checked_sub(1)
                    .and_then(|index| cutouts.get(index).copied());
                let after_promoted = cutouts.get(span_index).copied();
                self.items.push(PaintCoverageItem::LegacyBoundary {
                    order: order.clone(),
                    root,
                    stable_id,
                    reason,
                    span_index,
                    before_promoted,
                    after_promoted,
                });
                if let Some(promoted_root) = after_promoted {
                    let promoted_id = self
                        .arena
                        .get(promoted_root)
                        .map(|node| node.element.stable_id())
                        .unwrap_or_default();
                    self.items.push(PaintCoverageItem::PromotedBoundary {
                        order: order.clone(),
                        root: promoted_root,
                        stable_id: promoted_id,
                    });
                }
            }
        }

        fn collect_promoted_cutouts(&self, root: NodeKey, out: &mut Vec<NodeKey>) {
            let Some(node) = self.arena.get(root) else {
                return;
            };
            for &child in node.element.children() {
                let Some(child_node) = self.arena.get(child) else {
                    continue;
                };
                if self.deferred_roots.contains(&child) {
                    continue;
                }
                if self.promoted.contains(&child_node.element.stable_id()) {
                    out.push(child);
                } else {
                    self.collect_promoted_cutouts(child, out);
                }
            }
        }
    }

    let mut recorder = Recorder {
        arena,
        promoted: promoted_node_ids,
        exemption: promoted_root_exemption,
        force_legacy_roots,
        deferred_roots: &deferred_set,
        properties: property_trees,
        owner_parents: &owner_parents,
        generations: paint_generations,
        recording_mode,
        transform_surface_authority,
        effect_surface_authority,
        baked_scroll_host_authority: initial_recording_context.baked_scroll_host,
        consumed_ancestor_property: initial_recording_context.consumed_ancestor_property,
        consumed_ancestor_property_stack: initial_recording_context
            .consumed_ancestor_property_stack,
        nested_scroll_content: initial_recording_context.nested_scroll_content,
        nested_scroll_host: initial_recording_context.nested_scroll_host,
        scroll_text_area_subtree: initial_recording_context.scroll_text_area_subtree,
        baked_scroll_text_area_subtree: initial_recording_context.baked_scroll_text_area_subtree,
        scroll_atomic_projection_text_area_subtree: initial_recording_context
            .scroll_atomic_projection_text_area_subtree,
        baked_scroll_atomic_projection_text_area_subtree: initial_recording_context
            .baked_scroll_atomic_projection_text_area_subtree,
        scroll_interactive_text_area_subtree: initial_recording_context
            .scroll_interactive_text_area_subtree,
        baked_scroll_interactive_text_area_subtree: initial_recording_context
            .baked_scroll_interactive_text_area_subtree,
        required_scroll_content_paint_offset_bits: initial_recording_context
            .required_scroll_content_paint_offset_bits,
        opacity_authority: initial_recording_context.opacity_authority,
        planned_boundary_cutouts,
        nested_scroll_receiver,
        items: &mut manifest.items,
        validation_errors: &mut manifest.validation_errors,
    };
    for (root_index, &root) in roots.iter().enumerate() {
        recorder.walk(
            root,
            root_index,
            &mut Vec::new(),
            false,
            initial_recording_context,
        );
    }
    if emit_deferred_late {
        for (deferred_index, deferred_root) in deferred_roots.into_iter().enumerate() {
            recorder.walk(
                deferred_root,
                roots.len(),
                &mut vec![deferred_index],
                true,
                initial_recording_context,
            );
        }
    }
    drop(recorder);
    if let Some(receiver) = nested_scroll_receiver
        && !manifest.items.iter().any(|item| {
            matches!(item, PaintCoverageItem::NestedScrollContentReceiver { cutout, .. } if *cutout == receiver)
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
        promoted: &FxHashSet<u64>,
        deferred: &FxHashSet<NodeKey>,
        out: &mut FxHashSet<NodeKey>,
    ) {
        if key != root && deferred.contains(&key) {
            return;
        }
        let Some(node) = arena.get(key) else {
            return;
        };
        if key != root && promoted.contains(&node.element.stable_id()) {
            return;
        }
        if !out.insert(key) {
            return;
        }
        for &child in node.element.children() {
            mark_legacy_coverage(arena, child, root, promoted, deferred, out);
        }
    }
    for item in &manifest.items {
        if let PaintCoverageItem::LegacyBoundary {
            root,
            reason,
            span_index: 0,
            ..
        } = item
        {
            let coverage = manifest.legacy_coverage.entry(*reason).or_default();
            mark_legacy_coverage(
                arena,
                *root,
                *root,
                promoted_node_ids,
                &deferred_set,
                coverage,
            );
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
mod tests {
    use super::*;
    use std::any::Any;

    use crate::style::{
        Angle, ClipMode, Layout, Length, ParsedValue, Position, PropertyId, Rotate, Style,
        Transform,
    };
    use crate::view::base_component::{
        BoxModelSnapshot, BuildState, Element, ElementTrait, EventTarget, LayoutConstraints,
        LayoutPlacement, Layoutable, Renderable, ShadowPaintRecordingCapability, Text,
        UiBuildContext,
    };
    use crate::view::frame_graph::FrameGraph;
    use crate::view::node_arena::Node;
    use slotmap::Key;

    #[derive(Clone)]
    struct PlanShape {
        before: Vec<(PaintNodePhase, u16)>,
        after: Vec<(PaintNodePhase, u16)>,
    }

    impl PlanShape {
        fn new(before: &[u16], after: &[u16]) -> Self {
            Self {
                before: before
                    .iter()
                    .copied()
                    .map(|slot| (PaintNodePhase::BeforeChildren, slot))
                    .collect(),
                after: after
                    .iter()
                    .copied()
                    .map(|slot| (PaintNodePhase::AfterChildren, slot))
                    .collect(),
            }
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum PlanHostMode {
        Recordable,
        Transparent,
    }

    #[derive(Clone, Copy)]
    enum ConsumedAuthorityAttack {
        Clear,
        Replace,
    }

    struct PlanHost {
        id: u64,
        mode: PlanHostMode,
        metadata_scope: PaintPropertyScope,
        full_scope: PaintPropertyScope,
        metadata: PlanShape,
        full: PlanShape,
        children: Vec<NodeKey>,
        deferred: bool,
        contents_scissor: Option<[u32; 4]>,
        consumed_authority_attack: Option<ConsumedAuthorityAttack>,
        clear_paint_offset_for_node: bool,
        clear_opacity_authority_for_node: bool,
        clear_opacity_authority_for_child: bool,
        required_opacity_authority: Option<super::super::PaintOpacityAuthority>,
    }

    impl PlanHost {
        fn recordable(id: u64, before: &[u16], after: &[u16]) -> Self {
            let shape = PlanShape::new(before, after);
            Self {
                id,
                mode: PlanHostMode::Recordable,
                metadata_scope: PaintPropertyScope::SelfPaint,
                full_scope: PaintPropertyScope::SelfPaint,
                metadata: shape.clone(),
                full: shape,
                children: Vec::new(),
                deferred: false,
                contents_scissor: None,
                consumed_authority_attack: None,
                clear_paint_offset_for_node: false,
                clear_opacity_authority_for_node: false,
                clear_opacity_authority_for_child: false,
                required_opacity_authority: None,
            }
        }

        fn transparent(id: u64) -> Self {
            Self {
                id,
                mode: PlanHostMode::Transparent,
                metadata_scope: PaintPropertyScope::SelfPaint,
                full_scope: PaintPropertyScope::SelfPaint,
                metadata: PlanShape::new(&[], &[]),
                full: PlanShape::new(&[], &[]),
                children: Vec::new(),
                deferred: false,
                contents_scissor: None,
                consumed_authority_attack: None,
                clear_paint_offset_for_node: false,
                clear_opacity_authority_for_node: false,
                clear_opacity_authority_for_child: false,
                required_opacity_authority: None,
            }
        }

        fn metadata_for(
            owner: NodeKey,
            properties: crate::view::compositor::property_tree::PropertyTreeState,
            revision: PaintContentRevision,
            scope: PaintPropertyScope,
            phase: PaintNodePhase,
            slot: u16,
        ) -> PaintChunkMetadata {
            PaintChunkMetadata {
                id: crate::view::paint::PaintChunkId {
                    owner,
                    scope,
                    phase,
                    slot,
                    role: crate::view::paint::PaintChunkRole::SelfDecoration,
                },
                owner,
                bounds: crate::view::base_component::Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                },
                properties,
                content_revision: revision,
                payload_identity: crate::view::paint::PaintPayloadIdentity::prepared_shadows(
                    std::iter::empty::<&crate::view::paint::PreparedShadowOp>(),
                ),
            }
        }

        fn metadata_plan(
            shape: &PlanShape,
            owner: NodeKey,
            self_properties: crate::view::compositor::property_tree::PropertyTreeState,
            contents_properties: crate::view::compositor::property_tree::PropertyTreeState,
            revision: PaintContentRevision,
            scope: PaintPropertyScope,
        ) -> PaintNodePlan<PaintChunkMetadata> {
            let properties = match scope {
                PaintPropertyScope::SelfPaint => self_properties,
                PaintPropertyScope::Contents => contents_properties,
            };
            PaintNodePlan {
                before_children: shape
                    .before
                    .iter()
                    .map(|&(phase, slot)| {
                        Self::metadata_for(owner, properties, revision, scope, phase, slot)
                    })
                    .collect(),
                after_children: shape
                    .after
                    .iter()
                    .map(|&(phase, slot)| {
                        Self::metadata_for(owner, properties, revision, scope, phase, slot)
                    })
                    .collect(),
            }
        }

        fn artifact_plan(
            shape: &PlanShape,
            owner: NodeKey,
            self_properties: crate::view::compositor::property_tree::PropertyTreeState,
            contents_properties: crate::view::compositor::property_tree::PropertyTreeState,
            revision: PaintContentRevision,
            scope: PaintPropertyScope,
        ) -> PaintNodePlan<crate::view::paint::PaintArtifact> {
            let metadata = Self::metadata_plan(
                shape,
                owner,
                self_properties,
                contents_properties,
                revision,
                scope,
            );
            let artifact = |chunk: PaintChunkMetadata| crate::view::paint::PaintArtifact {
                target: Default::default(),
                chunks: vec![crate::view::paint::PaintChunk {
                    id: chunk.id,
                    owner: chunk.owner,
                    op_range: 0..0,
                    bounds: chunk.bounds,
                    properties: chunk.properties,
                    content_revision: chunk.content_revision,
                    payload_identity: chunk.payload_identity,
                }],
                ops: Vec::new(),
                clip_nodes: Vec::new(),
                effect_nodes: Vec::new(),
                owner_nodes: Vec::new(),
            };
            PaintNodePlan {
                before_children: metadata
                    .before_children
                    .into_iter()
                    .map(&artifact)
                    .collect(),
                after_children: metadata.after_children.into_iter().map(artifact).collect(),
            }
        }
    }

    impl Layoutable for PlanHost {
        fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
        fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
        fn measured_size(&self) -> (f32, f32) {
            (1.0, 1.0)
        }
        fn set_layout_width(&mut self, _width: f32) {}
        fn set_layout_height(&mut self, _height: f32) {}
    }

    impl EventTarget for PlanHost {}

    impl Renderable for PlanHost {
        fn build(
            &mut self,
            _graph: &mut FrameGraph,
            _arena: &mut NodeArena,
            ctx: UiBuildContext,
        ) -> BuildState {
            ctx.into_state()
        }
    }

    impl ElementTrait for PlanHost {
        fn stable_id(&self) -> u64 {
            self.id
        }

        fn box_model_snapshot(&self) -> BoxModelSnapshot {
            BoxModelSnapshot {
                node_id: self.id,
                parent_id: None,
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                border_radius: 0.0,
                should_render: true,
            }
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        fn shadow_paint_recording_capability(
            &self,
            _arena: &NodeArena,
            _deferred_phase_root: bool,
            recording_context: PaintRecordingContext,
        ) -> ShadowPaintRecordingCapability {
            if self
                .required_opacity_authority
                .is_some_and(|required| recording_context.opacity_authority != required)
            {
                return ShadowPaintRecordingCapability::Unsupported;
            }
            match self.mode {
                PlanHostMode::Recordable => ShadowPaintRecordingCapability::Recordable,
                PlanHostMode::Transparent => ShadowPaintRecordingCapability::Transparent,
            }
        }

        fn record_shadow_paint_metadata_plan(
            &self,
            owner: NodeKey,
            properties: crate::view::compositor::property_tree::PropertyTreeState,
            contents_properties: crate::view::compositor::property_tree::PropertyTreeState,
            revision: PaintContentRevision,
            _arena: &NodeArena,
            _recording_context: PaintRecordingContext,
        ) -> Option<PaintNodePlan<PaintChunkMetadata>> {
            (self.mode == PlanHostMode::Recordable).then(|| {
                Self::metadata_plan(
                    &self.metadata,
                    owner,
                    properties,
                    contents_properties,
                    revision,
                    self.metadata_scope,
                )
            })
        }

        fn record_shadow_paint_artifact_plan(
            &self,
            owner: NodeKey,
            properties: crate::view::compositor::property_tree::PropertyTreeState,
            contents_properties: crate::view::compositor::property_tree::PropertyTreeState,
            revision: PaintContentRevision,
            _arena: &NodeArena,
            _recording_context: PaintRecordingContext,
        ) -> Option<PaintNodePlan<crate::view::paint::PaintArtifact>> {
            (self.mode == PlanHostMode::Recordable).then(|| {
                Self::artifact_plan(
                    &self.full,
                    owner,
                    properties,
                    contents_properties,
                    revision,
                    self.full_scope,
                )
            })
        }

        fn children(&self) -> &[NodeKey] {
            &self.children
        }

        fn sync_children_mirror(&mut self, children: &[NodeKey]) {
            self.children.clear();
            self.children.extend_from_slice(children);
        }

        fn is_deferred_to_root_viewport_render(&self) -> bool {
            self.deferred
        }

        fn shadow_paint_recording_context(
            &self,
            mut parent: PaintRecordingContext,
        ) -> PaintRecordingContext {
            if self.clear_paint_offset_for_node {
                parent.paint_offset = [0.0, 0.0];
            }
            if self.clear_opacity_authority_for_node {
                parent.opacity_authority = super::super::PaintOpacityAuthority::Baked;
            }
            parent
        }

        fn contents_logical_scissor(&self) -> Option<[u32; 4]> {
            self.contents_scissor
        }

        fn shadow_paint_recording_context_for_child(
            &self,
            child: NodeKey,
            _arena: &NodeArena,
            mut parent: PaintRecordingContext,
        ) -> PaintRecordingContext {
            match self.consumed_authority_attack {
                None => {}
                Some(ConsumedAuthorityAttack::Clear) => {
                    parent.consumed_ancestor_property = None;
                }
                Some(ConsumedAuthorityAttack::Replace) => {
                    parent.consumed_ancestor_property =
                        Some(super::super::ConsumedAncestorProperty::Transform(
                            super::super::ConsumedAncestorTransformWitness {
                                parent_boundary: child,
                                child_boundary: child,
                                transform: TransformNodeId(child),
                                target_owner: child,
                            },
                        ));
                }
            }
            if self.clear_opacity_authority_for_child {
                parent.opacity_authority = super::super::PaintOpacityAuthority::Baked;
            }
            parent
        }
    }

    fn insert(arena: &mut NodeArena, id: u64) -> NodeKey {
        let mut element = Element::new_with_id(id, 0.0, 0.0, 10.0, 10.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        element.apply_style(style);
        arena.insert(Node::new(Box::new(element)))
    }

    fn append(arena: &mut NodeArena, parent: NodeKey, child: NodeKey) {
        arena.set_parent(child, Some(parent));
        arena.push_child(parent, child);
    }

    fn identity(arena: &NodeArena, roots: &[NodeKey]) -> (PropertyTrees, PaintGenerationTracker) {
        let mut properties = PropertyTrees::default();
        properties.sync(arena, roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(arena, roots, &properties);
        (properties, generations)
    }

    fn record(
        arena: &NodeArena,
        roots: &[NodeKey],
        promoted: &FxHashSet<u64>,
        force_legacy_roots: bool,
    ) -> PaintCoverageManifest {
        let (properties, generations) = identity(arena, roots);
        record_coverage_manifest(
            arena,
            roots,
            promoted,
            None,
            force_legacy_roots,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
        )
    }

    fn insert_plan(arena: &mut NodeArena, host: PlanHost) -> NodeKey {
        arena.insert(Node::new(Box::new(host)))
    }

    #[test]
    fn phase_plan_preserves_vector_order_around_dom_children() {
        let mut arena = NodeArena::new();
        let root = insert_plan(&mut arena, PlanHost::recordable(0x8f00, &[0, 1], &[0, 1]));
        let child = insert_plan(&mut arena, PlanHost::recordable(0x8f01, &[0], &[]));
        append(&mut arena, root, child);

        let manifest = record(&arena, &[root], &FxHashSet::default(), false);
        let sequence = manifest
            .items
            .iter()
            .map(|item| match item {
                PaintCoverageItem::ArtifactChunk { chunk, order, .. } => {
                    (chunk.owner, order.phase, order.slot)
                }
                other => panic!("unexpected coverage item: {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            sequence,
            vec![
                (root, PaintNodePhase::BeforeChildren, 0),
                (root, PaintNodePhase::BeforeChildren, 1),
                (child, PaintNodePhase::BeforeChildren, 0),
                (root, PaintNodePhase::AfterChildren, 0),
                (root, PaintNodePhase::AfterChildren, 1),
            ]
        );

        let (properties, generations) = identity(&arena, &[root]);
        let crate::view::paint::FrameArtifactRecordOutcome::Artifact {
            artifact,
            eligibility,
        } = crate::view::paint::record_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            crate::view::paint::RendererMode::Auto,
        )
        .expect("phase plan recording")
        else {
            panic!("phase plan must be whole-frame eligible")
        };
        assert!(eligibility.eligible);
        assert_eq!(artifact.chunks.len(), 5);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .filter(|chunk| chunk.owner == root)
                .count(),
            4,
            "same owner must retain every distinct phase/slot chunk"
        );
        let mut graph = FrameGraph::new();
        let ctx = UiBuildContext::new(16, 16, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        assert!(
            crate::view::paint::try_compile_artifact(&artifact, &mut graph, ctx).is_ok(),
            "compiler/store must accept same-owner chunks with distinct phase/slot ids"
        );
    }

    #[test]
    fn planned_boundary_is_typed_canonical_and_stops_both_recording_passes() {
        let mut arena = NodeArena::new();
        let root = insert_plan(&mut arena, PlanHost::recordable(0x8f08, &[0], &[0]));
        let boundary_root = insert_plan(&mut arena, PlanHost::recordable(0x8f09, &[0], &[0]));
        let hidden_descendant = insert_plan(&mut arena, PlanHost::recordable(0x8f0a, &[0], &[]));
        append(&mut arena, root, boundary_root);
        append(&mut arena, boundary_root, hidden_descendant);
        let (mut properties, generations) = identity(&arena, &[root]);
        properties.transforms.insert(
            TransformNodeId(boundary_root),
            crate::view::compositor::property_tree::TransformNode {
                owner: boundary_root,
                parent: None,
                viewport_matrix: glam::Mat4::IDENTITY,
                generation: 1,
            },
        );
        let boundary = PlannedBoundary {
            root: boundary_root,
            stable_id: 0x8f09,
            kind: PlannedBoundaryKind::Transform(TransformNodeId(boundary_root)),
        };
        let cutouts = PlannedBoundaryCutoutSet::from_iter([(boundary_root, boundary)]);
        let record = |mode| {
            record_coverage_manifest_with_context(
                &arena,
                &[root],
                &FxHashSet::default(),
                None,
                false,
                true,
                mode,
                &properties,
                &generations,
                PaintRecordingContext::default(),
                None,
                &cutouts,
            )
        };
        let metadata = record(CoverageRecordingMode::MetadataOnly);
        let full = record(CoverageRecordingMode::FullArtifact);

        for manifest in [&metadata, &full] {
            assert!(matches!(
                manifest.items.as_slice(),
                [
                    PaintCoverageItem::ArtifactChunk { chunk: before, .. },
                    PaintCoverageItem::PlannedBoundary { boundary: actual, .. },
                    PaintCoverageItem::ArtifactChunk { chunk: after, .. },
                ] if before.owner == root
                    && before.id.phase == PaintNodePhase::BeforeChildren
                    && *actual == boundary
                    && after.owner == root
                    && after.id.phase == PaintNodePhase::AfterChildren
            ));
            assert!(manifest.items.iter().all(|item| match item {
                PaintCoverageItem::ArtifactChunk { chunk, .. } => {
                    chunk.owner != boundary_root && chunk.owner != hidden_descendant
                }
                _ => true,
            }));
        }
        assert!(super::super::frame_recorder::canonical_manifest_matches(
            &metadata, &full
        ));

        let mut mismatched = full.clone();
        let PaintCoverageItem::PlannedBoundary {
            boundary: mismatched_boundary,
            ..
        } = &mut mismatched.items[1]
        else {
            panic!("fixture marker")
        };
        mismatched_boundary.stable_id ^= 1;
        assert!(!super::super::frame_recorder::canonical_manifest_matches(
            &metadata,
            &mismatched
        ));
        mismatched.items.remove(1);
        assert!(!super::super::frame_recorder::canonical_manifest_matches(
            &metadata,
            &mismatched
        ));

        let invalid_boundary = PlannedBoundary {
            stable_id: boundary.stable_id ^ 1,
            ..boundary
        };
        let invalid_cutouts =
            PlannedBoundaryCutoutSet::from_iter([(boundary_root, invalid_boundary)]);
        let invalid = record_coverage_manifest_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            None,
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
            PaintRecordingContext::default(),
            None,
            &invalid_cutouts,
        );
        assert_eq!(
            invalid.validation_errors,
            vec![PaintCoverageValidationError::InvalidPlannedBoundary(
                boundary_root
            )]
        );

        let isolation_boundary = PlannedBoundary {
            root: boundary_root,
            stable_id: boundary.stable_id,
            kind: PlannedBoundaryKind::Isolation(
                crate::view::compositor::property_tree::EffectNodeId(boundary_root),
            ),
        };
        let isolation_cutouts =
            PlannedBoundaryCutoutSet::from_iter([(boundary_root, isolation_boundary)]);
        let missing_isolation_snapshot = record_coverage_manifest_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            None,
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
            PaintRecordingContext::default(),
            None,
            &isolation_cutouts,
        );
        assert_eq!(
            missing_isolation_snapshot.validation_errors,
            vec![PaintCoverageValidationError::InvalidPlannedBoundary(
                boundary_root
            )],
            "a transform snapshot cannot authorize an isolation marker"
        );
        properties.effects.insert(
            EffectNodeId(boundary_root),
            crate::view::compositor::property_tree::EffectNode {
                owner: boundary_root,
                parent: None,
                opacity: 0.5,
                generation: 1,
            },
        );
        for mode in [
            CoverageRecordingMode::MetadataOnly,
            CoverageRecordingMode::FullArtifact,
        ] {
            let manifest = record_coverage_manifest_with_context(
                &arena,
                &[root],
                &FxHashSet::default(),
                None,
                false,
                true,
                mode,
                &properties,
                &generations,
                PaintRecordingContext::default(),
                None,
                &isolation_cutouts,
            );
            assert!(matches!(
                manifest.items.as_slice(),
                [
                    PaintCoverageItem::ArtifactChunk { chunk: before, .. },
                    PaintCoverageItem::PlannedBoundary { boundary: actual, .. },
                    PaintCoverageItem::ArtifactChunk { chunk: after, .. },
                ] if before.owner == root
                    && *actual == isolation_boundary
                    && after.owner == root
            ));
            assert!(manifest.items.iter().all(|item| match item {
                PaintCoverageItem::ArtifactChunk { chunk, .. } => {
                    chunk.owner != boundary_root && chunk.owner != hidden_descendant
                }
                _ => true,
            }));
        }

        let wrong_effect_boundary = PlannedBoundary {
            kind: PlannedBoundaryKind::Isolation(
                crate::view::compositor::property_tree::EffectNodeId(root),
            ),
            ..isolation_boundary
        };
        let wrong_effect_cutouts =
            PlannedBoundaryCutoutSet::from_iter([(boundary_root, wrong_effect_boundary)]);
        let invalid = record_coverage_manifest_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            None,
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
            PaintRecordingContext::default(),
            None,
            &wrong_effect_cutouts,
        );
        assert_eq!(
            invalid.validation_errors,
            vec![PaintCoverageValidationError::InvalidPlannedBoundary(
                boundary_root
            )]
        );

        properties
            .transforms
            .remove(&TransformNodeId(boundary_root));
        let missing_transform_snapshot = record_coverage_manifest_with_context(
            &arena,
            &[root],
            &FxHashSet::default(),
            None,
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
            PaintRecordingContext::default(),
            None,
            &cutouts,
        );
        assert_eq!(
            missing_transform_snapshot.validation_errors,
            vec![PaintCoverageValidationError::InvalidPlannedBoundary(
                boundary_root
            )],
            "an effect snapshot cannot authorize a transform marker"
        );
    }

    #[test]
    fn component_child_context_cannot_clear_replace_or_retarget_recorder_owned_consumed_authority()
    {
        for attack in [
            ConsumedAuthorityAttack::Clear,
            ConsumedAuthorityAttack::Replace,
        ] {
            let mut arena = NodeArena::new();
            let parent = insert_plan(&mut arena, PlanHost::transparent(0x8f20));
            let child = insert_plan(&mut arena, PlanHost::recordable(0x8f21, &[0], &[]));
            let descendant = insert_plan(&mut arena, PlanHost::recordable(0x8f22, &[0], &[]));
            append(&mut arena, parent, child);
            append(&mut arena, child, descendant);
            arena
                .get_mut(child)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<PlanHost>()
                .unwrap()
                .consumed_authority_attack = Some(attack);

            let (mut properties, generations) = identity(&arena, &[parent]);
            let transform = TransformNodeId(parent);
            properties.transforms.insert(
                transform,
                crate::view::compositor::property_tree::TransformNode {
                    owner: parent,
                    parent: None,
                    viewport_matrix: glam::Mat4::IDENTITY,
                    generation: 1,
                },
            );
            for owner in [child, descendant] {
                let state = properties.states.get_mut(&owner).unwrap();
                state.paint.transform = Some(transform);
                state.descendants.transform = Some(transform);
            }
            let witness =
                super::super::ConsumedAncestorTransformWitness::new(parent, child, transform)
                    .unwrap();
            let context = PaintRecordingContext {
                consumed_ancestor_property: Some(
                    super::super::ConsumedAncestorProperty::Transform(witness),
                ),
                ..Default::default()
            };
            let record = |mode| {
                record_coverage_manifest_with_context(
                    &arena,
                    &[child],
                    &FxHashSet::default(),
                    None,
                    false,
                    false,
                    mode,
                    &properties,
                    &generations,
                    context,
                    None,
                    &Default::default(),
                )
            };
            let metadata = record(CoverageRecordingMode::MetadataOnly);
            let full = record(CoverageRecordingMode::FullArtifact);
            assert!(metadata.validation_errors.is_empty());
            assert!(full.validation_errors.is_empty());
            assert!(super::super::frame_recorder::canonical_manifest_matches(
                &metadata, &full
            ));
            for manifest in [&metadata, &full] {
                let owners = manifest
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        PaintCoverageItem::ArtifactChunk { chunk, .. } => {
                            assert_eq!(chunk.properties.transform, None);
                            Some(chunk.owner)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(owners, vec![child, descendant]);
            }
        }
    }

    #[test]
    fn component_hooks_cannot_clear_or_replace_recorder_owned_scroll_contents_authority() {
        for attack in [
            ConsumedAuthorityAttack::Clear,
            ConsumedAuthorityAttack::Replace,
        ] {
            let mut arena = NodeArena::new();
            let parent = insert_plan(&mut arena, PlanHost::transparent(0x8f24));
            let child = insert_plan(&mut arena, PlanHost::recordable(0x8f25, &[0], &[]));
            let descendant = insert_plan(&mut arena, PlanHost::recordable(0x8f26, &[0], &[]));
            append(&mut arena, parent, child);
            append(&mut arena, child, descendant);
            arena
                .get_mut(child)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<PlanHost>()
                .unwrap()
                .consumed_authority_attack = Some(attack);

            let (mut properties, generations) = identity(&arena, &[parent]);
            let scroll = crate::view::compositor::property_tree::ScrollNodeId(parent);
            let clip = crate::view::compositor::property_tree::ClipNodeId {
                owner: parent,
                role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
            };
            for owner in [child, descendant] {
                let state = properties.states.get_mut(&owner).unwrap();
                state.paint.scroll = Some(scroll);
                state.paint.clip = Some(clip);
                state.descendants.scroll = Some(scroll);
                state.descendants.clip = Some(clip);
            }
            let witness = super::super::ConsumedAncestorScrollContentsWitness::new(
                parent, child, scroll, clip,
            )
            .unwrap();
            let context = PaintRecordingContext {
                consumed_ancestor_property: Some(
                    super::super::ConsumedAncestorProperty::ScrollContents(witness),
                ),
                ..Default::default()
            };
            let record = |mode| {
                record_coverage_manifest_with_context(
                    &arena,
                    &[child],
                    &FxHashSet::default(),
                    None,
                    false,
                    false,
                    mode,
                    &properties,
                    &generations,
                    context,
                    None,
                    &Default::default(),
                )
            };
            let metadata = record(CoverageRecordingMode::MetadataOnly);
            let full = record(CoverageRecordingMode::FullArtifact);
            assert!(metadata.validation_errors.is_empty());
            assert!(full.validation_errors.is_empty());
            assert!(super::super::frame_recorder::canonical_manifest_matches(
                &metadata, &full
            ));
            for manifest in [&metadata, &full] {
                let chunks = manifest
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        PaintCoverageItem::ArtifactChunk { chunk, .. } => Some(chunk),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(chunks.len(), 2);
                assert!(
                    chunks
                        .iter()
                        .all(|chunk| chunk.properties == Default::default())
                );
            }
        }
    }

    #[test]
    fn component_hook_cannot_clear_required_scroll_content_paint_offset() {
        let mut arena = NodeArena::new();
        let root = insert_plan(&mut arena, PlanHost::recordable(0x8f2f, &[0], &[]));
        arena
            .get_mut(root)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<PlanHost>()
            .unwrap()
            .clear_paint_offset_for_node = true;
        let (properties, generations) = identity(&arena, &[root]);
        let context = PaintRecordingContext {
            paint_offset: [3.5, 47.25],
            required_scroll_content_paint_offset_bits: Some([3.5_f32, 47.25_f32].map(f32::to_bits)),
            ..Default::default()
        };
        for mode in [
            CoverageRecordingMode::MetadataOnly,
            CoverageRecordingMode::FullArtifact,
        ] {
            let manifest = record_coverage_manifest_with_context(
                &arena,
                &[root],
                &FxHashSet::default(),
                None,
                false,
                false,
                mode,
                &properties,
                &generations,
                context,
                None,
                &Default::default(),
            );
            assert!(manifest.items.iter().any(|item| matches!(
                item,
                PaintCoverageItem::LegacyBoundary {
                    reason: LegacyPaintReason::MissingPaintIdentity,
                    ..
                }
            )));
            assert!(
                !manifest
                    .items
                    .iter()
                    .any(|item| matches!(item, PaintCoverageItem::ArtifactChunk { .. }))
            );
        }
    }

    #[test]
    fn component_node_and_child_hooks_cannot_clear_recorder_owned_opacity_authority() {
        let mut arena = NodeArena::new();
        let root = insert_plan(&mut arena, PlanHost::transparent(0x8f30));
        let child = insert_plan(&mut arena, PlanHost::recordable(0x8f31, &[0], &[]));
        append(&mut arena, root, child);
        let effect = EffectNodeId(root);
        {
            let mut node = arena.get_mut(root).unwrap();
            let host = node
                .element
                .as_any_mut()
                .downcast_mut::<PlanHost>()
                .unwrap();
            host.clear_opacity_authority_for_node = true;
            host.clear_opacity_authority_for_child = true;
            host.required_opacity_authority = Some(
                super::super::PaintOpacityAuthority::NeutralRootEffect(effect),
            );
        }
        arena
            .get_mut(child)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<PlanHost>()
            .unwrap()
            .required_opacity_authority = Some(
            super::super::PaintOpacityAuthority::NeutralRootEffect(effect),
        );

        let (properties, generations) = identity(&arena, &[root]);
        let context = PaintRecordingContext {
            opacity_authority: super::super::PaintOpacityAuthority::NeutralRootEffect(effect),
            ..Default::default()
        };
        let record = |mode| {
            record_coverage_manifest_with_context(
                &arena,
                &[root],
                &FxHashSet::default(),
                None,
                false,
                false,
                mode,
                &properties,
                &generations,
                context,
                None,
                &Default::default(),
            )
        };
        let metadata = record(CoverageRecordingMode::MetadataOnly);
        let full = record(CoverageRecordingMode::FullArtifact);
        assert!(metadata.validation_errors.is_empty());
        assert!(full.validation_errors.is_empty());
        assert!(super::super::frame_recorder::canonical_manifest_matches(
            &metadata, &full
        ));
        for manifest in [&metadata, &full] {
            assert!(matches!(
                manifest.items.as_slice(),
                [
                    PaintCoverageItem::TransparentNode { owner, .. },
                    PaintCoverageItem::ArtifactChunk { chunk, .. },
                ] if *owner == root && chunk.owner == child
            ));
        }
    }

    #[test]
    fn transparent_parent_and_leaf_are_canonical_coverage_without_chunks() {
        let mut arena = NodeArena::new();
        let parent = insert_plan(&mut arena, PlanHost::transparent(0x8f10));
        let child = insert_plan(&mut arena, PlanHost::recordable(0x8f11, &[0], &[]));
        append(&mut arena, parent, child);
        let leaf = insert_plan(&mut arena, PlanHost::transparent(0x8f12));
        let roots = [parent, leaf];
        let (properties, generations) = identity(&arena, &roots);

        let metadata = record_coverage_manifest(
            &arena,
            &roots,
            &FxHashSet::default(),
            None,
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
        );
        let full = record_coverage_manifest(
            &arena,
            &roots,
            &FxHashSet::default(),
            None,
            false,
            true,
            CoverageRecordingMode::FullArtifact,
            &properties,
            &generations,
        );
        assert!(matches!(
            metadata.items.as_slice(),
            [
                PaintCoverageItem::TransparentNode { owner: a, .. },
                PaintCoverageItem::ArtifactChunk { chunk, .. },
                PaintCoverageItem::TransparentNode { owner: b, .. },
            ] if *a == parent && chunk.owner == child && *b == leaf
        ));
        assert!(super::super::frame_recorder::canonical_manifest_matches(
            &metadata, &full
        ));
        let stats = metadata.stats();
        assert_eq!(stats.total_nodes, 3);
        assert_eq!(stats.artifact_nodes, 3);
        assert_eq!(stats.artifact_chunks, 1);
    }

    #[test]
    fn contents_scope_uses_descendants_properties_and_compiles_intersect_clip() {
        let mut arena = NodeArena::new();
        let mut host = PlanHost::recordable(0x8f18, &[0], &[]);
        host.metadata_scope = PaintPropertyScope::Contents;
        host.full_scope = PaintPropertyScope::Contents;
        host.contents_scissor = Some([3, 4, 20, 10]);
        let root = insert_plan(&mut arena, host);
        let (properties, generations) = identity(&arena, &[root]);
        let states = properties.node_state_for(root).expect("property states");
        assert_eq!(states.paint.clip, None);
        assert!(matches!(
            states.descendants.clip,
            Some(crate::view::compositor::property_tree::ClipNodeId {
                owner,
                role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
            }) if owner == root
        ));

        let crate::view::paint::FrameArtifactRecordOutcome::Artifact { artifact, .. } =
            crate::view::paint::record_frame_artifact(
                &arena,
                &[root],
                &properties,
                &generations,
                crate::view::paint::RendererMode::Auto,
            )
            .expect("contents artifact")
        else {
            panic!("contents scope must be artifact eligible")
        };
        assert_eq!(artifact.chunks[0].id.scope, PaintPropertyScope::Contents);
        assert_eq!(artifact.chunks[0].properties, states.descendants);
        let mut graph = FrameGraph::new();
        let ctx = UiBuildContext::new(32, 32, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        assert!(crate::view::paint::try_compile_artifact(&artifact, &mut graph, ctx).is_ok());
    }

    #[test]
    fn metadata_full_phase_and_slot_drift_or_duplicates_are_rejected() {
        fn manifests(host: PlanHost) -> (PaintCoverageManifest, PaintCoverageManifest) {
            let mut arena = NodeArena::new();
            let root = insert_plan(&mut arena, host);
            let (properties, generations) = identity(&arena, &[root]);
            let metadata = record_coverage_manifest(
                &arena,
                &[root],
                &FxHashSet::default(),
                None,
                false,
                true,
                CoverageRecordingMode::MetadataOnly,
                &properties,
                &generations,
            );
            let full = record_coverage_manifest(
                &arena,
                &[root],
                &FxHashSet::default(),
                None,
                false,
                true,
                CoverageRecordingMode::FullArtifact,
                &properties,
                &generations,
            );
            (metadata, full)
        }

        let mut missing = PlanHost::recordable(0x8f20, &[0, 1], &[]);
        missing.full = PlanShape::new(&[0], &[]);
        let (metadata, full) = manifests(missing);
        assert!(!super::super::frame_recorder::canonical_manifest_matches(
            &metadata, &full
        ));

        let mut swapped = PlanHost::recordable(0x8f21, &[0, 1], &[]);
        swapped.full.before.swap(0, 1);
        let (metadata, full) = manifests(swapped);
        assert!(!super::super::frame_recorder::canonical_manifest_matches(
            &metadata, &full
        ));

        let mut wrong_phase = PlanHost::recordable(0x8f22, &[0], &[]);
        wrong_phase.full.before[0].0 = PaintNodePhase::AfterChildren;
        let (_, full) = manifests(wrong_phase);
        assert!(full.validation_errors.iter().any(|error| matches!(
            error,
            PaintCoverageValidationError::InvalidChunkPhase {
                expected: PaintNodePhase::BeforeChildren,
                actual: PaintNodePhase::AfterChildren,
                ..
            }
        )));

        let mut duplicate = PlanHost::recordable(0x8f23, &[0], &[]);
        duplicate
            .full
            .before
            .push((PaintNodePhase::BeforeChildren, 0));
        let (_, full) = manifests(duplicate);
        assert!(full.validation_errors.iter().any(|error| matches!(
            error,
            PaintCoverageValidationError::DuplicateChunkSlot {
                phase: PaintNodePhase::BeforeChildren,
                slot: 0,
                ..
            }
        )));

        let mut scope_drift = PlanHost::recordable(0x8f24, &[0], &[]);
        scope_drift.contents_scissor = Some([0, 0, 10, 10]);
        scope_drift.full_scope = PaintPropertyScope::Contents;
        let (metadata, full) = manifests(scope_drift);
        assert!(!super::super::frame_recorder::canonical_manifest_matches(
            &metadata, &full
        ));
    }

    #[test]
    fn promoted_cutout_stays_between_host_phases_and_deferred_stays_late() {
        let mut arena = NodeArena::new();
        let root = insert_plan(&mut arena, PlanHost::recordable(0x8f30, &[0], &[0]));
        let promoted = insert_plan(&mut arena, PlanHost::recordable(0x8f31, &[0], &[]));
        append(&mut arena, root, promoted);
        let promoted_manifest = record(&arena, &[root], &FxHashSet::from_iter([0x8f31]), false);
        assert!(matches!(
            promoted_manifest.items.as_slice(),
            [
                PaintCoverageItem::ArtifactChunk { chunk: before, .. },
                PaintCoverageItem::PromotedBoundary { root: cutout, .. },
                PaintCoverageItem::ArtifactChunk { chunk: after, .. },
            ] if before.owner == root
                && before.id.phase == PaintNodePhase::BeforeChildren
                && *cutout == promoted
                && after.owner == root
                && after.id.phase == PaintNodePhase::AfterChildren
        ));

        let mut arena = NodeArena::new();
        let root = insert_plan(&mut arena, PlanHost::recordable(0x8f40, &[0], &[0]));
        let mut deferred_host = PlanHost::recordable(0x8f41, &[0], &[0]);
        deferred_host.deferred = true;
        let deferred = insert_plan(&mut arena, deferred_host);
        append(&mut arena, root, deferred);
        let normal_root = insert_plan(&mut arena, PlanHost::recordable(0x8f42, &[0], &[]));
        let manifest = record(&arena, &[root, normal_root], &FxHashSet::default(), false);
        let sequence = manifest
            .items
            .iter()
            .filter_map(|item| match item {
                PaintCoverageItem::ArtifactChunk { chunk, .. } => {
                    Some((chunk.owner, chunk.id.phase))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            sequence,
            vec![
                (root, PaintNodePhase::BeforeChildren),
                (root, PaintNodePhase::AfterChildren),
                (normal_root, PaintNodePhase::BeforeChildren),
                (deferred, PaintNodePhase::BeforeChildren),
                (deferred, PaintNodePhase::AfterChildren),
            ]
        );
    }

    #[test]
    fn recursive_manifest_preserves_root_order_and_self_before_children() {
        let mut arena = NodeArena::new();
        let a = insert(&mut arena, 1);
        let child = insert(&mut arena, 2);
        append(&mut arena, a, child);
        let b = insert(&mut arena, 3);
        let manifest = record(&arena, &[a, b], &FxHashSet::default(), false);
        let owners = manifest
            .items
            .iter()
            .filter_map(|item| match item {
                PaintCoverageItem::ArtifactChunk { chunk, .. } => Some(chunk.owner),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(owners, vec![a, child, b]);
        let orders = manifest
            .items
            .iter()
            .filter_map(|item| match item {
                PaintCoverageItem::ArtifactChunk { order, .. } => Some(order.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(orders[0].root_index, 0);
        assert_eq!(orders[1].child_path, vec![0]);
        assert_eq!(orders[2].root_index, 1);
    }

    #[test]
    fn metadata_mode_never_records_or_retains_full_ops() {
        let mut arena = NodeArena::new();
        let root = insert(&mut arena, 4);
        let (properties, generations) = identity(&arena, &[root]);
        crate::view::paint::take_full_artifact_record_count();

        let metadata = record_coverage_manifest(
            &arena,
            &[root],
            &FxHashSet::default(),
            None,
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
        );
        assert_eq!(crate::view::paint::take_full_artifact_record_count(), 0);
        assert!(matches!(
            metadata.items.as_slice(),
            [PaintCoverageItem::ArtifactChunk { ops: None, .. }]
        ));

        let full = record_coverage_manifest(
            &arena,
            &[root],
            &FxHashSet::default(),
            None,
            false,
            true,
            CoverageRecordingMode::FullArtifact,
            &properties,
            &generations,
        );
        assert_eq!(crate::view::paint::take_full_artifact_record_count(), 1);
        assert!(matches!(
            full.items.as_slice(),
            [PaintCoverageItem::ArtifactChunk { ops: Some(_), .. }]
        ));
    }

    #[test]
    fn artifact_promoted_artifact_and_legacy_span_cutouts_keep_order() {
        let mut arena = NodeArena::new();
        let root = insert(&mut arena, 10);
        let left = insert(&mut arena, 11);
        let promoted = insert(&mut arena, 12);
        let right = insert(&mut arena, 13);
        append(&mut arena, root, left);
        append(&mut arena, root, promoted);
        append(&mut arena, root, right);
        let promoted_ids = FxHashSet::from_iter([12]);
        let manifest = record(&arena, &[root], &promoted_ids, false);
        assert!(matches!(manifest.items.as_slice(), [
            PaintCoverageItem::ArtifactChunk { chunk: root_chunk, .. },
            PaintCoverageItem::ArtifactChunk { chunk: left_chunk, .. },
            PaintCoverageItem::PromotedBoundary { root: promoted_root, .. },
            PaintCoverageItem::ArtifactChunk { chunk: right_chunk, .. },
        ] if root_chunk.owner == root && left_chunk.owner == left && *promoted_root == promoted && right_chunk.owner == right));

        let spans = record(&arena, &[root], &promoted_ids, true);
        assert!(matches!(spans.items.as_slice(), [
            PaintCoverageItem::LegacyBoundary { span_index: 0, after_promoted: Some(a), .. },
            PaintCoverageItem::PromotedBoundary { root: b, .. },
            PaintCoverageItem::LegacyBoundary { span_index: 1, before_promoted: Some(c), after_promoted: None, .. },
        ] if *a == promoted && *b == promoted && *c == promoted));
    }

    #[test]
    fn artifact_legacy_artifact_sequence_reports_unprepared_text_boundary() {
        let mut arena = NodeArena::new();
        let root = insert(&mut arena, 14);
        let left = insert(&mut arena, 15);
        let unknown = arena.insert(Node::new(Box::new(Text::new(0.0, 0.0, 10.0, 10.0, "text"))));
        let right = insert(&mut arena, 16);
        append(&mut arena, root, left);
        append(&mut arena, root, unknown);
        append(&mut arena, root, right);
        let manifest = record(&arena, &[root], &FxHashSet::default(), false);
        assert!(matches!(manifest.items.as_slice(), [
            PaintCoverageItem::ArtifactChunk { chunk: root_chunk, .. },
            PaintCoverageItem::ArtifactChunk { chunk: left_chunk, .. },
            PaintCoverageItem::LegacyBoundary { root: legacy_root, reason: LegacyPaintReason::MissingPreparedText, .. },
            PaintCoverageItem::ArtifactChunk { chunk: right_chunk, .. },
        ] if root_chunk.owner == root && left_chunk.owner == left && *legacy_root == unknown && right_chunk.owner == right));
    }

    fn deferred_element(id: u64) -> Element {
        let mut element = Element::new_with_id(id, 0.0, 0.0, 10.0, 10.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(0.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        element.apply_style(style);
        element
    }

    #[test]
    fn deferred_promoted_descendant_is_after_normal_sibling_and_nested_deferred_is_unique() {
        let mut arena = NodeArena::new();
        let root = insert(&mut arena, 60);
        let deferred = arena.insert(Node::new(Box::new(deferred_element(61))));
        let promoted = insert(&mut arena, 62);
        append(&mut arena, deferred, promoted);
        let normal_q = insert(&mut arena, 63);
        append(&mut arena, root, deferred);
        append(&mut arena, root, normal_q);
        let nested_deferred = arena.insert(Node::new(Box::new(deferred_element(64))));
        append(&mut arena, deferred, nested_deferred);

        let manifest = record(&arena, &[root], &FxHashSet::from_iter([62]), false);
        let q_index = manifest
            .items
            .iter()
            .position(|item| matches!(item, PaintCoverageItem::ArtifactChunk { chunk, .. } if chunk.owner == normal_q))
            .expect("normal sibling Q");
        let p_index = manifest
            .items
            .iter()
            .position(|item| matches!(item, PaintCoverageItem::PromotedBoundary { root, .. } if *root == promoted))
            .expect("late promoted P");
        assert!(q_index < p_index, "normal Q must precede deferred P");
        assert_eq!(
            manifest
                .items
                .iter()
                .filter(|item| matches!(item, PaintCoverageItem::LegacyBoundary { root, span_index: 0, .. } if *root == deferred))
                .count(),
            1
        );
        assert_eq!(
            manifest
                .items
                .iter()
                .filter(|item| matches!(item, PaintCoverageItem::LegacyBoundary { root, span_index: 0, .. } if *root == nested_deferred))
                .count(),
            1
        );
    }

    #[test]
    fn deferred_promoted_invocation_root_uses_identity_exemption_without_late_phase() {
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(deferred_element(65))));
        let (properties, generations) = identity(&arena, &[root]);
        let manifest = record_coverage_manifest(
            &arena,
            &[root],
            &FxHashSet::from_iter([65]),
            Some(root),
            true,
            false,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
        );
        assert!(matches!(manifest.items.as_slice(), [
            PaintCoverageItem::LegacyBoundary {
                root: boundary,
                reason: LegacyPaintReason::Promoted,
                ..
            }
        ] if *boundary == root));
    }

    #[test]
    fn coverage_stats_count_entire_legacy_subtree_and_promoted_leaf_once() {
        let mut arena = NodeArena::new();
        let mut transformed = Element::new_with_id(70, 0.0, 0.0, 10.0, 10.0);
        let mut style = Style::new();
        style.set_transform(Transform::new([Rotate::z(Angle::deg(10.0))]));
        transformed.apply_style(style);
        let root = arena.insert(Node::new(Box::new(transformed)));
        let mut parent = root;
        for id in 71..80 {
            let child = insert(&mut arena, id);
            append(&mut arena, parent, child);
            parent = child;
        }
        let promoted = insert(&mut arena, 80);
        append(&mut arena, parent, promoted);
        let manifest = record(&arena, &[root], &FxHashSet::from_iter([80]), false);
        let stats = manifest.stats();
        assert_eq!(stats.total_nodes, 11);
        assert_eq!(stats.legacy_covered_nodes, 10);
        assert_eq!(stats.promoted_boundaries, 1);
    }

    #[test]
    fn fallback_boundary_does_not_record_descendant_and_deferred_is_single_late_boundary() {
        let mut arena = NodeArena::new();
        let mut transformed = Element::new_with_id(20, 0.0, 0.0, 10.0, 10.0);
        let mut style = Style::new();
        style.set_transform(Transform::new([Rotate::z(Angle::deg(10.0))]));
        transformed.apply_style(style);
        let root = arena.insert(Node::new(Box::new(transformed)));
        let child = insert(&mut arena, 21);
        append(&mut arena, root, child);
        let manifest = record(&arena, &[root], &FxHashSet::default(), false);
        assert_eq!(manifest.items.len(), 1);
        assert!(
            matches!(manifest.items[0], PaintCoverageItem::LegacyBoundary { root: boundary, reason: LegacyPaintReason::Transform, .. } if boundary == root)
        );

        let mut deferred = Element::new_with_id(30, 0.0, 0.0, 10.0, 10.0);
        let mut deferred_style = Style::new();
        deferred_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(0.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        deferred.apply_style(deferred_style);
        let deferred_root = arena.insert(Node::new(Box::new(deferred)));
        let deferred_manifest = record(&arena, &[deferred_root], &FxHashSet::default(), false);
        assert!(
            matches!(
                deferred_manifest.items.as_slice(),
                [PaintCoverageItem::LegacyBoundary {
                    reason: LegacyPaintReason::SelfClip,
                    ..
                }]
            ),
            "{:#?}",
            deferred_manifest.items
        );
    }

    #[test]
    fn validation_reports_missing_duplicate_key_and_stable_id() {
        let mut arena = NodeArena::new();
        let a = insert(&mut arena, 40);
        let b = insert(&mut arena, 40);
        let manifest = record_coverage_manifest(
            &arena,
            &[a, a, b, NodeKey::null()],
            &FxHashSet::from_iter([999]),
            None,
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &PropertyTrees::default(),
            &PaintGenerationTracker::default(),
        );
        assert!(
            manifest
                .validation_errors
                .contains(&PaintCoverageValidationError::DuplicateNodeKey(a))
        );
        assert!(
            manifest
                .validation_errors
                .contains(&PaintCoverageValidationError::DuplicateStableId(40))
        );
        assert!(
            manifest
                .validation_errors
                .contains(&PaintCoverageValidationError::MissingPromotedStableId(999))
        );
        assert!(
            manifest
                .validation_errors
                .contains(&PaintCoverageValidationError::MissingNode(NodeKey::null()))
        );
        assert_eq!(manifest.stats().total_nodes, 2);
    }

    #[test]
    fn recording_is_side_effect_free_and_deterministic() {
        let mut arena = NodeArena::new();
        let root = insert(&mut arena, 50);
        let first = record(&arena, &[root], &FxHashSet::default(), false);
        let second = record(&arena, &[root], &FxHashSet::default(), false);
        assert_eq!(format!("{:?}", first.items), format!("{:?}", second.items));
        assert_eq!(first.validation_errors, second.validation_errors);
    }
}
