#![allow(dead_code)]

use std::collections::hash_map::Entry;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::view::base_component::{
    RetainedScrollAtomicProjectionSelectionTextAreaSubtreeAdmissionSnapshot,
    RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot,
    RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot,
    RetainedScrollTextAreaSubtreeAdmissionSnapshot,
};
use crate::view::compositor::property_tree::{EffectNodeId, EffectNodeSnapshot};
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::node_arena::{NodeArena, NodeKey};

use super::coverage_manifest::{
    NestedScrollContentReceiverCutout, record_coverage_manifest_with_context,
    record_coverage_manifest_with_nested_scroll_receiver,
    record_coverage_manifest_with_property_authorities,
};

use super::{
    CoverageRecordingMode, EffectPropertySurfaceArtifactContract, LegacyPaintReason, PaintArtifact,
    PaintArtifactTarget, PaintBakedScrollHostWitness, PaintChunk, PaintCoverageItem,
    PaintCoverageValidationError, PaintNestedScrollContentWitness, PaintOpacityAuthority,
    PaintRecordingContext, PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness,
    PaintScrollAtomicProjectionTextAreaRecorderWitness as AtomicProjectionRecorderWitness,
    PaintScrollAtomicProjectionTextAreaSubtreeWitness, PaintScrollContentWitness,
    PaintScrollInteractiveTextAreaSubtreeWitness, PaintScrollTextAreaSubtreeWitness,
    PaintTransformSurfaceWitness, RecordedRetainedTextAreaCaretOverlay,
    RetainedTextAreaPreeditRasterSeal,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RendererMode {
    Legacy,
    Auto,
    StrictPlan,
    #[cfg(test)]
    ForcedForTests,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FrameArtifactFallbackReason {
    RendererLegacy,
    LegacyBoundary(LegacyPaintReason),
    PromotedBoundary,
    /// M6A production authority accepts only chunks whose property-tree
    /// identity is completely neutral. Later milestones will make each
    /// property family authoritative one at a time.
    PropertyBoundary(NodeKey),
    RootCount(usize),
    MissingRootEffect(NodeKey),
    InvalidRootEffect(NodeKey),
    NestedEffect(NodeKey),
    NonEffectProperty(NodeKey),
    DeferredBoundary(NodeKey),
    Validation(PaintCoverageValidationError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedAtomicProjectionChunkLiveRasterOracle {
    id: super::PaintChunkId,
    owner: NodeKey,
    bounds_bits: [u32; 4],
    properties: crate::view::compositor::property_tree::PropertyTreeState,
    payload_identity: super::PaintPayloadIdentity,
}

impl RetainedAtomicProjectionChunkLiveRasterOracle {
    pub(crate) fn id(&self) -> super::PaintChunkId {
        self.id
    }
    pub(crate) fn owner(&self) -> NodeKey {
        self.owner
    }
    pub(crate) fn bounds_bits(&self) -> [u32; 4] {
        self.bounds_bits
    }
    pub(crate) fn properties(&self) -> crate::view::compositor::property_tree::PropertyTreeState {
        self.properties
    }
    pub(crate) fn payload_identity(&self) -> &super::PaintPayloadIdentity {
        &self.payload_identity
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedAtomicProjectionTextAreaLiveRasterOracle {
    content_root: NodeKey,
    text_area_root: NodeKey,
    source_grammar:
        crate::view::base_component::text_area::RetainedAtomicProjectionTextAreaPaintGrammar,
    chunks: Vec<RetainedAtomicProjectionChunkLiveRasterOracle>,
    clip_nodes: Vec<crate::view::compositor::property_tree::ClipNodeSnapshot>,
    owner_nodes: Vec<super::PaintOwnerSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle {
    content_root: NodeKey,
    text_area_root: NodeKey,
    source_grammar: crate::view::base_component::text_area::RetainedAtomicProjectionSelectionTextAreaPaintGrammar,
    chunks: Vec<RetainedAtomicProjectionChunkLiveRasterOracle>,
    clip_nodes: Vec<crate::view::compositor::property_tree::ClipNodeSnapshot>,
    owner_nodes: Vec<super::PaintOwnerSnapshot>,
}

impl RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle {
    pub(crate) fn content_root(&self) -> NodeKey {
        self.content_root
    }

    pub(crate) fn text_area_root(&self) -> NodeKey {
        self.text_area_root
    }

    pub(crate) fn source_grammar(
        &self,
    ) -> &crate::view::base_component::text_area::RetainedAtomicProjectionSelectionTextAreaPaintGrammar{
        &self.source_grammar
    }

    pub(crate) fn chunks(&self) -> &[RetainedAtomicProjectionChunkLiveRasterOracle] {
        &self.chunks
    }

    pub(crate) fn clip_nodes(&self) -> &[crate::view::compositor::property_tree::ClipNodeSnapshot] {
        &self.clip_nodes
    }

    pub(crate) fn owner_nodes(&self) -> &[super::PaintOwnerSnapshot] {
        &self.owner_nodes
    }

    pub(crate) fn matches_artifact(&self, artifact: &PaintArtifact) -> bool {
        self.chunks.len() == artifact.chunks.len()
            && self
                .chunks
                .iter()
                .zip(&artifact.chunks)
                .all(|(oracle, chunk)| {
                    oracle.id == chunk.id
                        && oracle.owner == chunk.owner
                        && oracle.bounds_bits
                            == [
                                chunk.bounds.x,
                                chunk.bounds.y,
                                chunk.bounds.width,
                                chunk.bounds.height,
                            ]
                            .map(f32::to_bits)
                        && oracle.properties == chunk.properties
                        && oracle.payload_identity == chunk.payload_identity
                })
            && self.clip_nodes == artifact.clip_nodes
            && self.owner_nodes == artifact.owner_nodes
            && artifact.effect_nodes.is_empty()
    }
}

fn normalize_atomic_projection_selection_chunk(
    artifact: &mut PaintArtifact,
    oracle: &mut RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle,
    chunk_index: usize,
    grammar: crate::view::base_component::text_area::RetainedTextAreaPaintGrammar,
) -> Option<super::artifact::RetainedTextAreaSelectionRasterSeal> {
    let chunk = artifact.chunks.get_mut(chunk_index)?;
    let oracle_chunk = oracle.chunks.get_mut(chunk_index)?;
    let rects = artifact.ops[chunk.op_range.clone()]
        .iter()
        .map(|op| match op {
            super::PaintOp::DrawRect(rect) => Some(rect),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    let generic = super::PaintPayloadIdentity::prepared_rects(rects.iter().copied())?;
    let sealed =
        super::PaintPayloadIdentity::prepared_text_area_selection(grammar, rects.iter().copied())?;
    if (chunk.payload_identity != generic && chunk.payload_identity != sealed)
        || oracle_chunk.payload_identity != generic
    {
        return None;
    }
    let seal = sealed.retained_text_area_selection_seal()?;
    chunk.payload_identity = sealed.clone();
    oracle_chunk.payload_identity = sealed;
    Some(seal)
}

impl RetainedAtomicProjectionTextAreaLiveRasterOracle {
    pub(crate) fn content_root(&self) -> NodeKey {
        self.content_root
    }

    pub(crate) fn text_area_root(&self) -> NodeKey {
        self.text_area_root
    }

    pub(crate) fn source_grammar(
        &self,
    ) -> &crate::view::base_component::text_area::RetainedAtomicProjectionTextAreaPaintGrammar {
        &self.source_grammar
    }

    pub(crate) fn chunks(&self) -> &[RetainedAtomicProjectionChunkLiveRasterOracle] {
        &self.chunks
    }

    pub(crate) fn clip_nodes(&self) -> &[crate::view::compositor::property_tree::ClipNodeSnapshot] {
        &self.clip_nodes
    }

    pub(crate) fn owner_nodes(&self) -> &[super::PaintOwnerSnapshot] {
        &self.owner_nodes
    }

    pub(crate) fn matches_artifact(&self, artifact: &PaintArtifact) -> bool {
        self.chunks.len() == artifact.chunks.len()
            && self
                .chunks
                .iter()
                .zip(&artifact.chunks)
                .all(|(oracle, chunk)| {
                    oracle.id == chunk.id
                        && oracle.owner == chunk.owner
                        && oracle.bounds_bits
                            == [
                                chunk.bounds.x,
                                chunk.bounds.y,
                                chunk.bounds.width,
                                chunk.bounds.height,
                            ]
                            .map(f32::to_bits)
                        && oracle.properties == chunk.properties
                        && oracle.payload_identity == chunk.payload_identity
                })
            && self.clip_nodes == artifact.clip_nodes
            && self.owner_nodes == artifact.owner_nodes
            && artifact.effect_nodes.is_empty()
    }
}

#[derive(Clone, Debug)]
pub(super) struct RecordedRetainedAtomicProjectionTextAreaSubtree {
    artifact: PaintArtifact,
    raster_oracle: RetainedAtomicProjectionTextAreaLiveRasterOracle,
}

#[derive(Clone, Debug)]
pub(super) struct RecordedRetainedAtomicProjectionTextAreaHost {
    artifact: PaintArtifact,
    raster_oracle: RetainedAtomicProjectionTextAreaLiveRasterOracle,
    source_bounds_bits: [u32; 4],
    outer_scroll: crate::view::compositor::property_tree::ScrollNodeSnapshot,
    outer_contents_clip: crate::view::compositor::property_tree::ClipNodeSnapshot,
    local_contents_clip: crate::view::compositor::property_tree::ClipNodeSnapshot,
}

#[derive(Clone, Debug)]
pub(super) struct RecordedRetainedAtomicProjectionSelectionTextAreaSubtree {
    artifact: PaintArtifact,
    raster_oracle: RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle,
}

#[derive(Clone, Debug)]
pub(super) struct RecordedRetainedAtomicProjectionSelectionTextAreaHost {
    artifact: PaintArtifact,
    raster_oracle: RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle,
    source_bounds_bits: [u32; 4],
    outer_scroll: crate::view::compositor::property_tree::ScrollNodeSnapshot,
    outer_contents_clip: crate::view::compositor::property_tree::ClipNodeSnapshot,
    local_contents_clip: crate::view::compositor::property_tree::ClipNodeSnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AtomicProjectionSelectionBackingContract {
    Single,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AtomicProjectionSelectionPostCompositeContract {
    None,
}

#[derive(Clone, Debug)]
pub(super) struct ValidatedRecordedAtomicProjectionSelectionTextAreaAuthority {
    host: RecordedRetainedAtomicProjectionSelectionTextAreaHost,
    local: RecordedRetainedAtomicProjectionSelectionTextAreaSubtree,
    selection: super::artifact::RetainedTextAreaSelectionRasterSeal,
    backing: AtomicProjectionSelectionBackingContract,
    post_composite: AtomicProjectionSelectionPostCompositeContract,
    opaque_parent_delta: u8,
}

#[cfg(test)]
impl RecordedRetainedAtomicProjectionSelectionTextAreaHost {
    pub(crate) fn chunk_count_for_test(&self) -> usize {
        self.artifact.chunks.len()
    }

    pub(crate) fn is_canonical_for_test(&self) -> bool {
        self.raster_oracle.matches_artifact(&self.artifact)
    }

    pub(crate) fn tamper_order_for_test(mut self, first: usize, second: usize) -> Self {
        self.artifact.chunks.swap(first, second);
        self.raster_oracle.chunks.swap(first, second);
        self
    }

    pub(crate) fn tamper_selection_payload_for_test(mut self) -> Self {
        let replacement = self.artifact.chunks[3].payload_identity.clone();
        self.artifact.chunks[2].payload_identity = replacement.clone();
        self.raster_oracle.chunks[2].payload_identity = replacement;
        self
    }

    pub(crate) fn tamper_wrapper_bounds_for_test(mut self) -> Self {
        self.artifact.chunks[1].bounds.x += 1.0;
        self.raster_oracle.chunks[1].bounds_bits[0] = self.artifact.chunks[1].bounds.x.to_bits();
        self
    }

    pub(crate) fn tamper_source_line_for_test(mut self) -> Self {
        self.raster_oracle
            .source_grammar
            .atomic_source
            .atomic_line_index += 1;
        self
    }
}

#[cfg(test)]
impl RecordedRetainedAtomicProjectionSelectionTextAreaSubtree {
    pub(crate) fn chunk_count_for_test(&self) -> usize {
        self.artifact.chunks.len()
    }

    pub(crate) fn is_canonical_for_test(&self) -> bool {
        self.raster_oracle.matches_artifact(&self.artifact)
    }

    pub(crate) fn tamper_selection_payload_for_test(mut self) -> Self {
        let replacement = self.artifact.chunks[2].payload_identity.clone();
        self.artifact.chunks[1].payload_identity = replacement.clone();
        self.raster_oracle.chunks[1].payload_identity = replacement;
        self
    }

    pub(crate) fn tamper_local_clip_for_test(mut self) -> Self {
        self.artifact.clip_nodes[0].logical_scissor[0] ^= 1;
        self.raster_oracle.clip_nodes[0] = self.artifact.clip_nodes[0];
        self
    }

    pub(crate) fn tamper_wrapper_bounds_for_test(mut self) -> Self {
        self.artifact.chunks[0].bounds.x += 1.0;
        self.raster_oracle.chunks[0].bounds_bits[0] = self.artifact.chunks[0].bounds.x.to_bits();
        self
    }

    pub(crate) fn tamper_owner_parent_for_test(mut self) -> Self {
        self.artifact.owner_nodes[1].parent = None;
        self.raster_oracle.owner_nodes[1].parent = None;
        self
    }

    pub(crate) fn tamper_source_line_for_test(mut self) -> Self {
        self.raster_oracle
            .source_grammar
            .atomic_source
            .atomic_line_index += 1;
        self
    }
}

impl ValidatedRecordedAtomicProjectionSelectionTextAreaAuthority {
    fn is_canonical(&self) -> bool {
        let [_, _, host_selection, _, _, _] = self.host.artifact.chunks.as_slice() else {
            return false;
        };
        let [_, local_selection, _, _] = self.local.artifact.chunks.as_slice() else {
            return false;
        };
        self.host
            .raster_oracle
            .matches_artifact(&self.host.artifact)
            && self
                .local
                .raster_oracle
                .matches_artifact(&self.local.artifact)
            && self.local.raster_oracle.source_grammar.is_canonical()
            && self
                .selection
                .is_canonical_for_text_area(self.local.raster_oracle.source_grammar.selection)
            && host_selection
                .payload_identity
                .retained_text_area_selection_seal()
                .is_some()
            && local_selection
                .payload_identity
                .retained_text_area_selection_seal()
                .as_ref()
                == Some(&self.selection)
            && self.backing == AtomicProjectionSelectionBackingContract::Single
            && self.post_composite == AtomicProjectionSelectionPostCompositeContract::None
            && self.opaque_parent_delta == 0
    }

    #[cfg(test)]
    pub(crate) fn is_canonical_for_test(&self) -> bool {
        self.is_canonical()
    }

    #[cfg(test)]
    pub(crate) fn chunk_counts_for_test(&self) -> (usize, usize) {
        (
            self.host.artifact.chunks.len(),
            self.local.artifact.chunks.len(),
        )
    }

    #[cfg(test)]
    pub(crate) fn localized_selection_changed_for_test(&self) -> bool {
        let host = self.host.artifact.chunks[2]
            .payload_identity
            .retained_text_area_selection_seal();
        let local = self.local.artifact.chunks[1]
            .payload_identity
            .retained_text_area_selection_seal();
        host.is_some() && local.is_some() && host != local
    }
}

impl RecordedRetainedAtomicProjectionTextAreaHost {
    #[cfg(test)]
    pub(crate) fn chunk_count_for_test(&self) -> usize {
        self.artifact.chunks.len()
    }

    #[cfg(test)]
    pub(crate) fn is_canonical_for_test(&self) -> bool {
        self.raster_oracle.matches_artifact(&self.artifact)
    }

    #[cfg(test)]
    pub(crate) fn tamper_artifact_for_test(
        mut self,
        tamper: impl FnOnce(&mut PaintArtifact),
    ) -> Self {
        tamper(&mut self.artifact);
        self
    }

    /// Keeps the host live-oracle/artifact pair synchronized so focused tests
    /// exercise compiler cross-recording parity rather than the earlier
    /// recorder-oracle equality gate.
    #[cfg(test)]
    pub(crate) fn tamper_cross_parity_bounds_for_test(mut self, chunk_index: usize) -> Self {
        self.artifact.chunks[chunk_index].bounds.x += 1.0;
        self.raster_oracle.chunks[chunk_index].bounds_bits[0] =
            self.artifact.chunks[chunk_index].bounds.x.to_bits();
        self
    }

    #[cfg(test)]
    pub(crate) fn tamper_cross_parity_payload_for_test(
        mut self,
        target_chunk_index: usize,
        source_chunk_index: usize,
    ) -> Self {
        let source_op =
            self.artifact.ops[self.artifact.chunks[source_chunk_index].op_range.start].clone();
        let source_payload = self.artifact.chunks[source_chunk_index]
            .payload_identity
            .clone();
        let target_op = self.artifact.chunks[target_chunk_index].op_range.start;
        self.artifact.ops[target_op] = source_op;
        self.artifact.chunks[target_chunk_index].payload_identity = source_payload.clone();
        self.raster_oracle.chunks[target_chunk_index].payload_identity = source_payload;
        self
    }

    #[cfg(test)]
    pub(crate) fn tamper_cross_parity_order_for_test(
        mut self,
        first: usize,
        second: usize,
    ) -> Self {
        self.artifact.chunks.swap(first, second);
        self.raster_oracle.chunks.swap(first, second);
        self
    }
}

impl RecordedRetainedAtomicProjectionTextAreaSubtree {
    /// Keeps the local live-oracle/artifact pair synchronized so the bridge
    /// must bind geometry to the independent outer scroll witness.
    #[cfg(test)]
    pub(crate) fn tamper_cross_parity_bounds_for_test(mut self, chunk_index: usize) -> Self {
        self.artifact.chunks[chunk_index].bounds.x += 1.0;
        self.raster_oracle.chunks[chunk_index].bounds_bits[0] =
            self.artifact.chunks[chunk_index].bounds.x.to_bits();
        self
    }
}

impl RecordedRetainedAtomicProjectionTextAreaSubtree {
    #[cfg(test)]
    pub(crate) fn artifact_for_test(&self) -> &PaintArtifact {
        &self.artifact
    }

    #[cfg(test)]
    pub(crate) fn tamper_artifact_for_test(
        mut self,
        tamper: impl FnOnce(&mut PaintArtifact),
    ) -> Self {
        tamper(&mut self.artifact);
        self
    }
}

pub(super) fn validate_recorded_atomic_projection_text_area_subtree(
    recorded: RecordedRetainedAtomicProjectionTextAreaSubtree,
) -> Option<super::compiler::ValidatedScrollSceneAtomicProjectionTextAreaContentArtifact> {
    let RecordedRetainedAtomicProjectionTextAreaSubtree {
        artifact,
        raster_oracle,
    } = recorded;
    super::compiler::validate_scroll_scene_atomic_projection_text_area_content_artifact_parts(
        artifact,
        raster_oracle,
    )
}

/// C3a typed bridge. Both recorded authorities are consumed here so no raw
/// host/local artifact can escape to the scroll-scene planner. The compiler
/// independently validates the full five-chunk host, the three-chunk local
/// subtree, and their normalized cross-recording parity before returning one
/// opaque set of plan authorities.
pub(super) fn validate_recorded_atomic_projection_text_area_plan_parts(
    host: RecordedRetainedAtomicProjectionTextAreaHost,
    local: RecordedRetainedAtomicProjectionTextAreaSubtree,
) -> Option<super::compiler::ValidatedScrollSceneAtomicProjectionTextAreaPlanParts> {
    let RecordedRetainedAtomicProjectionTextAreaHost {
        artifact: host_artifact,
        raster_oracle: host_raster_oracle,
        source_bounds_bits,
        outer_scroll,
        outer_contents_clip,
        local_contents_clip,
    } = host;
    let RecordedRetainedAtomicProjectionTextAreaSubtree {
        artifact: local_artifact,
        raster_oracle: local_raster_oracle,
    } = local;
    super::compiler::validate_scroll_scene_atomic_projection_text_area_plan_parts(
        host_artifact,
        host_raster_oracle,
        source_bounds_bits,
        outer_scroll,
        outer_contents_clip,
        local_contents_clip,
        local_artifact,
        local_raster_oracle,
    )
}

/// Graph-inert consume-pair bridge for the selection grammar.  Both typed
/// recordings are consumed; no artifact or generic content token escapes.
pub(super) fn validate_recorded_atomic_projection_selection_text_area_authority(
    host: RecordedRetainedAtomicProjectionSelectionTextAreaHost,
    local: RecordedRetainedAtomicProjectionSelectionTextAreaSubtree,
) -> Option<ValidatedRecordedAtomicProjectionSelectionTextAreaAuthority> {
    if !host.raster_oracle.matches_artifact(&host.artifact)
        || !local.raster_oracle.matches_artifact(&local.artifact)
        || host.raster_oracle.chunks.len() != 6
        || local.raster_oracle.chunks.len() != 4
        || host.raster_oracle.content_root != local.raster_oracle.content_root
        || host.raster_oracle.text_area_root != local.raster_oracle.text_area_root
        || host.raster_oracle.source_grammar != local.raster_oracle.source_grammar
        || !local.raster_oracle.source_grammar.is_canonical()
    {
        return None;
    }
    let content_root = local.raster_oracle.content_root;
    let text_area_root = local.raster_oracle.text_area_root;
    let grammar = &local.raster_oracle.source_grammar;
    let boundary_root = host.outer_scroll.owner;
    PaintScrollContentWitness::new(
        boundary_root,
        content_root,
        host.outer_scroll,
        host.outer_contents_clip,
    )?;
    let [local_clip] = local.raster_oracle.clip_nodes.as_slice() else {
        return None;
    };
    let host_live_clip = host
        .raster_oracle
        .clip_nodes
        .iter()
        .find(|clip| clip.id == local_clip.id)?;
    if host.raster_oracle.clip_nodes.len() != 2
        || !host
            .raster_oracle
            .clip_nodes
            .contains(&host.outer_contents_clip)
        || host.local_contents_clip != *local_clip
        || host_live_clip.parent != Some(host.outer_contents_clip.id)
        || local_clip.parent.is_some()
        || host.outer_contents_clip == *local_clip
    {
        return None;
    }
    let [local_content_owner, local_tail @ ..] = local.raster_oracle.owner_nodes.as_slice() else {
        return None;
    };
    let [host_boundary_owner, host_content_owner, host_tail @ ..] =
        host.raster_oracle.owner_nodes.as_slice()
    else {
        return None;
    };
    if *local_content_owner
        != (super::PaintOwnerSnapshot {
            owner: content_root,
            parent: None,
        })
        || *host_boundary_owner
            != (super::PaintOwnerSnapshot {
                owner: boundary_root,
                parent: None,
            })
        || *host_content_owner
            != (super::PaintOwnerSnapshot {
                owner: content_root,
                parent: Some(boundary_root),
            })
        || host_tail != local_tail
    {
        return None;
    }
    let [
        root_before,
        host_wrapper,
        host_selection,
        host_root_glyph,
        host_projection_glyph,
        overlay,
    ] = host.artifact.chunks.as_slice()
    else {
        return None;
    };
    let [
        local_wrapper,
        local_selection,
        local_root_glyph,
        local_projection_glyph,
    ] = local.artifact.chunks.as_slice()
    else {
        return None;
    };
    let selection = local_selection
        .payload_identity
        .retained_text_area_selection_seal()?;
    host_selection
        .payload_identity
        .retained_text_area_selection_seal()?;
    if host_selection
        .payload_identity
        .retained_text_area_selection_grammar()
        != Some(grammar.selection)
        || local_selection
            .payload_identity
            .retained_text_area_selection_grammar()
            != Some(grammar.selection)
    {
        return None;
    }
    let outer_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(host.outer_contents_clip.id),
        scroll: Some(host.outer_scroll.id),
        ..Default::default()
    };
    let host_local_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(local_clip.id),
        scroll: Some(host.outer_scroll.id),
        ..Default::default()
    };
    let local_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(local_clip.id),
        ..Default::default()
    };
    let delta = [
        -f32::from_bits(grammar.atomic_source.last_unified_apply_bits.0),
        -f32::from_bits(grammar.atomic_source.last_unified_apply_bits.1),
    ];
    let pair_exact = |host_chunk: &super::PaintChunk, local_chunk: &super::PaintChunk| {
        let localized = host
            .artifact
            .ops
            .get(host_chunk.op_range.clone())?
            .iter()
            .map(|op| super::compiler::localize_exact_nested_scroll_leaf_op(op, delta))
            .collect::<Option<Vec<_>>>()?;
        let payload = if host_chunk.id.role == super::PaintChunkRole::SelectionUnderlay {
            super::PaintPayloadIdentity::prepared_text_area_selection(
                grammar.selection,
                localized.iter().filter_map(|op| match op {
                    super::PaintOp::DrawRect(rect) => Some(rect),
                    _ => None,
                }),
            )?
        } else {
            super::compiler::exact_nested_scroll_payload_identity(host_chunk.id.role, &localized)?
        };
        (host_chunk.id == local_chunk.id
            && host_chunk.owner == local_chunk.owner
            && payload == local_chunk.payload_identity
            && super::compiler::localized_atomic_projection_host_bounds(
                [
                    host_chunk.bounds.x,
                    host_chunk.bounds.y,
                    host_chunk.bounds.width,
                    host_chunk.bounds.height,
                ]
                .map(f32::to_bits),
                grammar.atomic_source.last_unified_apply_bits,
            ) == Some(
                [
                    local_chunk.bounds.x,
                    local_chunk.bounds.y,
                    local_chunk.bounds.width,
                    local_chunk.bounds.height,
                ]
                .map(f32::to_bits),
            ))
        .then_some(())
    };
    let content_zero_bounds = [
        host.outer_scroll.layout_content_bounds_at_zero.x.to_bits(),
        host.outer_scroll.layout_content_bounds_at_zero.y.to_bits(),
        host.outer_scroll
            .layout_content_bounds_at_zero
            .width
            .to_bits(),
        host.outer_scroll
            .layout_content_bounds_at_zero
            .height
            .to_bits(),
    ];
    let source_bounds_are_finite = host
        .source_bounds_bits
        .into_iter()
        .map(f32::from_bits)
        .all(f32::is_finite);
    if !source_bounds_are_finite
        || root_before.owner != boundary_root
        || root_before.id.owner != boundary_root
        || root_before.id.scope != super::PaintPropertyScope::SelfPaint
        || root_before.id.phase != super::PaintNodePhase::BeforeChildren
        || root_before.id.slot != 0
        || root_before.id.role != super::PaintChunkRole::SelfDecoration
        || root_before.properties != Default::default()
        || [
            root_before.bounds.x,
            root_before.bounds.y,
            root_before.bounds.width,
            root_before.bounds.height,
        ]
        .map(f32::to_bits)
            != host.source_bounds_bits
        || host_wrapper.properties != outer_state
        || host_selection.properties != host_local_state
        || host_root_glyph.properties != host_local_state
        || host_projection_glyph.properties != host_local_state
        || local_wrapper.properties != Default::default()
        || local_selection.properties != local_state
        || local_root_glyph.properties != local_state
        || local_projection_glyph.properties != local_state
        || [
            local_wrapper.bounds.x,
            local_wrapper.bounds.y,
            local_wrapper.bounds.width,
            local_wrapper.bounds.height,
        ]
        .map(f32::to_bits)
            != content_zero_bounds
        || host_selection.id.role != super::PaintChunkRole::SelectionUnderlay
        || host_selection.id.slot != 0
        || host_root_glyph.owner != text_area_root
        || host_projection_glyph.owner != grammar.atomic_source.projection_text_owner
        || overlay.owner != boundary_root
        || overlay.id.owner != boundary_root
        || overlay.id.scope != super::PaintPropertyScope::SelfPaint
        || overlay.id.phase != super::PaintNodePhase::AfterChildren
        || overlay.id.slot != 0
        || overlay.id.role != super::PaintChunkRole::ScrollbarOverlay
        || overlay.properties != Default::default()
        || [
            overlay.bounds.x,
            overlay.bounds.y,
            overlay.bounds.width,
            overlay.bounds.height,
        ]
        .map(f32::to_bits)
            != host.source_bounds_bits
        || pair_exact(host_wrapper, local_wrapper).is_none()
        || pair_exact(host_selection, local_selection).is_none()
        || pair_exact(host_root_glyph, local_root_glyph).is_none()
        || pair_exact(host_projection_glyph, local_projection_glyph).is_none()
    {
        return None;
    }
    Some(
        ValidatedRecordedAtomicProjectionSelectionTextAreaAuthority {
            host,
            local,
            selection,
            backing: AtomicProjectionSelectionBackingContract::Single,
            post_composite: AtomicProjectionSelectionPostCompositeContract::None,
            opaque_parent_delta: 0,
        },
    )
}

/// B1 typed compiler bridge. The validated pair is consumed in one step and
/// only an opaque fixed H/content/O plan authority can escape.
pub(super) fn validate_recorded_atomic_projection_selection_text_area_plan_parts(
    authority: ValidatedRecordedAtomicProjectionSelectionTextAreaAuthority,
) -> Option<super::compiler::ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts> {
    if !authority.is_canonical() {
        return None;
    }
    let ValidatedRecordedAtomicProjectionSelectionTextAreaAuthority {
        host,
        local,
        selection,
        backing: _,
        post_composite: _,
        opaque_parent_delta: _,
    } = authority;
    let RecordedRetainedAtomicProjectionSelectionTextAreaHost {
        artifact: host_artifact,
        raster_oracle: host_raster_oracle,
        source_bounds_bits,
        outer_scroll,
        outer_contents_clip,
        local_contents_clip,
    } = host;
    let RecordedRetainedAtomicProjectionSelectionTextAreaSubtree {
        artifact: local_artifact,
        raster_oracle: local_raster_oracle,
    } = local;
    super::compiler::validate_scroll_scene_atomic_projection_selection_text_area_plan_parts(
        host_artifact,
        host_raster_oracle,
        source_bounds_bits,
        outer_scroll,
        outer_contents_clip,
        local_contents_clip,
        local_artifact,
        local_raster_oracle,
        selection,
    )
}

#[derive(Clone, Debug, Default)]
pub(crate) struct FrameArtifactEligibility {
    pub(crate) eligible: bool,
    pub(crate) reasons: Vec<FrameArtifactFallbackReason>,
    pub(crate) chunk_count: usize,
    pub(crate) op_count: usize,
}

#[derive(Clone, Debug)]
pub(crate) enum FrameArtifactRecordOutcome {
    Artifact {
        artifact: PaintArtifact,
        eligibility: FrameArtifactEligibility,
    },
    WholeFrameLegacyFallback(FrameArtifactEligibility),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ForcedFrameArtifactError {
    pub(crate) reasons: Vec<FrameArtifactFallbackReason>,
}

pub(crate) fn record_frame_artifact(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    mode: RendererMode,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    record_frame_artifact_with_policy(
        arena,
        roots,
        &FxHashSet::default(),
        property_trees,
        paint_generations,
        mode,
        FrameArtifactAuthorityPolicy::ExistingBakedProperties,
        None,
        None,
    )
}

/// M6A production entry point. This is intentionally stricter than the
/// compatibility recorder above: the whole frame must be promotion-free,
/// deferred-free, and property-neutral before full artifact hooks run.
pub(crate) fn record_property_neutral_frame_artifact(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    mode: RendererMode,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    record_frame_artifact_with_policy(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        mode,
        FrameArtifactAuthorityPolicy::PropertyNeutral,
        None,
        None,
    )
}

/// Production baked-opacity authority that admits validated property-tree
/// clips while keeping every other property family on legacy. Unlike the
/// compatibility recorder, callers must supply the live promotion set so a
/// frame cannot mix promoted and artifact authority.
pub(crate) fn record_clip_enabled_frame_artifact(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    mode: RendererMode,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    record_frame_artifact_with_policy(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        mode,
        FrameArtifactAuthorityPolicy::ClipEnabled,
        None,
        None,
    )
}

/// M6C1 production entry point. One frame root and one root-owned effect become
/// the sole opacity authority; every recorded paint op is neutralized and the
/// compiler applies the owning effect exactly once at the group composite.
pub(crate) fn record_root_group_opacity_frame_artifact(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    mode: RendererMode,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    if mode == RendererMode::Legacy {
        return fallback_or_forced(
            mode,
            FrameArtifactEligibility {
                reasons: vec![FrameArtifactFallbackReason::RendererLegacy],
                ..FrameArtifactEligibility::default()
            },
        );
    }
    let plan = match root_opacity_group_plan(arena, roots, property_trees) {
        Ok(plan) => plan,
        Err(reasons) => {
            return fallback_or_forced(
                mode,
                FrameArtifactEligibility {
                    reasons,
                    ..FrameArtifactEligibility::default()
                },
            );
        }
    };
    record_frame_artifact_with_policy(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        mode,
        FrameArtifactAuthorityPolicy::RootOpacityGroup(plan),
        None,
        None,
    )
}

/// Planning-only recorder for one validated root transform surface. This does
/// not compile or emit the artifact and is intentionally not wired into the
/// production frame dispatch.
pub(super) fn record_transform_surface_artifact_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintTransformSurfaceWitness,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    match record_frame_artifact_with_policy(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        FrameArtifactAuthorityPolicy::TransformSurface(witness),
        None,
        None,
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => Ok(artifact),
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            Err(eligibility.reasons)
        }
        Err(error) => Err(error.reasons),
    }
}

pub(super) fn record_baked_scroll_host_artifact_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintBakedScrollHostWitness,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    match record_frame_artifact_with_policy(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        FrameArtifactAuthorityPolicy::BakedScrollHost(witness),
        None,
        None,
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => Ok(artifact),
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            Err(eligibility.reasons)
        }
        Err(error) => Err(error.reasons),
    }
}

fn text_area_matches_admitted_paint_grammar(
    text_area: &crate::view::base_component::TextArea,
    owner: NodeKey,
    arena: &NodeArena,
    paint_offset: [f32; 2],
    grammar: crate::view::base_component::text_area::RetainedTextAreaPaintGrammar,
) -> bool {
    if !grammar.is_canonical() {
        return false;
    }
    match grammar {
        crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly => {
            text_area.exact_retained_property_scroll_glyph_subtree(owner, arena, paint_offset)
        }
        crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
            ..
        } => {
            text_area.exact_retained_property_scroll_selection_glyph_subtree(
                owner,
                arena,
                paint_offset,
            ) == Some(grammar)
        }
    }
}

fn text_area_matches_admitted_interactive_paint_grammar(
    text_area: &crate::view::base_component::TextArea,
    owner: NodeKey,
    arena: &NodeArena,
    paint_offset: [f32; 2],
    grammar: crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar,
) -> bool {
    text_area.exact_retained_property_scroll_interactive_subtree(owner, arena, paint_offset)
        == Some(grammar)
}

pub(super) fn record_baked_scroll_interactive_text_area_subtree_host_artifact_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    admission: RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot,
    baked: PaintBakedScrollHostWitness,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    let invalid = |owner| vec![FrameArtifactFallbackReason::PropertyBoundary(owner)];
    let outer_clip_chain = property_trees
        .clip_snapshot_for(Some(baked.contents_clip()))
        .ok_or_else(|| invalid(admission.boundary_root))?;
    let [outer_clip] = outer_clip_chain.as_slice() else {
        return Err(invalid(admission.boundary_root));
    };
    let outer = PaintScrollContentWitness::new(
        admission.boundary_root,
        admission.content_wrapper,
        property_trees
            .scroll_snapshot_for(baked.scroll())
            .ok_or_else(|| invalid(admission.boundary_root))?,
        *outer_clip,
    )
    .ok_or_else(|| invalid(admission.boundary_root))?;
    let text_area_node = arena
        .get(admission.text_area_root)
        .ok_or_else(|| invalid(admission.text_area_root))?;
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::TextArea>()
        .ok_or_else(|| invalid(admission.text_area_root))?;
    let wrapper_node = arena
        .get(admission.content_wrapper)
        .ok_or_else(|| invalid(admission.content_wrapper))?;
    let wrapper = wrapper_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
        .ok_or_else(|| invalid(admission.content_wrapper))?;
    let recording_offset = wrapper
        .exact_retained_scroll_content_wrapper_recording_offset(outer.normalization_paint_offset())
        .ok_or_else(|| invalid(admission.content_wrapper))?;
    // The admission grammar is detached-content authority, but this host
    // recorder paints the live full tree before H/C/O extraction.  Preedit's
    // exact glyph/underline identities therefore need the live wrapper
    // offset; using the detached normalization would shift both identities by
    // the outer scroll amount and make full/local parity impossible.
    let live_recording_offset = wrapper
        .exact_retained_scroll_content_wrapper_recording_offset([0.0, 0.0])
        .ok_or_else(|| invalid(admission.content_wrapper))?;
    if !text_area_matches_admitted_interactive_paint_grammar(
        text_area,
        admission.text_area_root,
        arena,
        recording_offset,
        admission.paint_grammar,
    ) {
        return Err(invalid(admission.text_area_root));
    }
    let local_clip_id = crate::view::compositor::property_tree::ClipNodeId {
        owner: admission.text_area_root,
        role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
    };
    let live_chain = property_trees
        .clip_snapshot_for(Some(local_clip_id))
        .ok_or_else(|| invalid(admission.text_area_root))?;
    let [live_text_area_clip, live_outer_clip] = live_chain.as_slice() else {
        return Err(invalid(admission.text_area_root));
    };
    if live_outer_clip != outer_clip {
        return Err(invalid(admission.text_area_root));
    }
    let local_scissor = text_area
        .retained_property_scroll_local_contents_scissor(outer.normalization_paint_offset())
        .ok_or_else(|| invalid(admission.text_area_root))?;
    let text_area_witness = PaintScrollInteractiveTextAreaSubtreeWitness::new(
        outer,
        admission.text_area_root,
        *live_text_area_clip,
        local_scissor,
        admission.paint_grammar,
    )
    .ok_or_else(|| invalid(admission.text_area_root))?;
    let mut artifact = match record_frame_artifact_with_policy(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        FrameArtifactAuthorityPolicy::BakedScrollInteractiveTextAreaSubtreeHost(
            baked,
            text_area_witness,
        ),
        None,
        None,
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    let host_preedit_seal = if admission.paint_grammar.has_preedit() {
        Some(
            text_area
                .retained_interactive_preedit_raster_seal(
                    admission.text_area_root,
                    arena,
                    live_recording_offset,
                )
                .ok_or_else(|| invalid(admission.text_area_root))?,
        )
    } else {
        None
    };
    if let crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedSelectionGlyphs {
        start_char,
        end_char,
        color_rgba_bits,
    } = admission.paint_grammar
    {
        let Some(selection) = artifact.chunks.get_mut(2) else {
            return Err(invalid(admission.text_area_root));
        };
        let rects = artifact.ops[selection.op_range.clone()]
            .iter()
            .map(|op| match op {
                super::PaintOp::DrawRect(rect) => Some(rect),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| invalid(admission.text_area_root))?;
        let grammar =
            crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
                start_char,
                end_char,
                color_rgba_bits,
            };
        selection.payload_identity = super::PaintPayloadIdentity::prepared_text_area_selection(
            grammar,
            rects.into_iter(),
        )
        .ok_or_else(|| invalid(admission.text_area_root))?;
    }
    let outer_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(outer.contents_clip_snapshot().id),
        scroll: Some(outer.scroll_snapshot().id),
        ..Default::default()
    };
    let text_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(text_area_witness.live_contents_clip().id),
        scroll: Some(outer.scroll_snapshot().id),
        ..Default::default()
    };
    let exact_chunk =
        |chunk: &super::PaintChunk,
         owner: NodeKey,
         scope: super::PaintPropertyScope,
         phase: super::PaintNodePhase,
         slot: u16,
         role: super::PaintChunkRole,
         properties: crate::view::compositor::property_tree::PropertyTreeState| {
            chunk.owner == owner
                && chunk.id.owner == owner
                && chunk.id.scope == scope
                && chunk.id.phase == phase
                && chunk.id.slot == slot
                && chunk.id.role == role
                && chunk.properties == properties
                && chunk.op_range.end <= artifact.ops.len()
        };
    let exact_self_decoration = |chunk: &super::PaintChunk| {
        let rects = artifact.ops[chunk.op_range.clone()]
            .iter()
            .map(|op| match op {
                super::PaintOp::DrawRect(rect) => Some(rect),
                _ => None,
            })
            .collect::<Option<Vec<_>>>();
        let Some(rects) = rects.filter(|rects| matches!(rects.len(), 1 | 2)) else {
            return false;
        };
        let bounds_bits = [
            chunk.bounds.x,
            chunk.bounds.y,
            chunk.bounds.width,
            chunk.bounds.height,
        ]
        .map(f32::to_bits);
        rects[0].mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
            && rects.get(1).is_none_or(|rect| {
                rect.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::BorderOnly
            })
            && rects.iter().all(|rect| {
                [
                    rect.params.position[0],
                    rect.params.position[1],
                    rect.params.size[0],
                    rect.params.size[1],
                ]
                .map(f32::to_bits)
                    == bounds_bits
                    && rect.params.opacity.to_bits() == 1.0_f32.to_bits()
            })
            && super::PaintPayloadIdentity::prepared_shadows_with_decoration(
                std::iter::empty(),
                rects.into_iter(),
            )
            .as_ref()
                == Some(&chunk.payload_identity)
    };
    let root_before = |chunk: &super::PaintChunk| {
        exact_chunk(
            chunk,
            admission.boundary_root,
            super::PaintPropertyScope::SelfPaint,
            super::PaintNodePhase::BeforeChildren,
            0,
            super::PaintChunkRole::SelfDecoration,
            Default::default(),
        ) && exact_self_decoration(chunk)
    };
    let wrapper = |chunk: &super::PaintChunk| {
        exact_chunk(
            chunk,
            admission.content_wrapper,
            super::PaintPropertyScope::SelfPaint,
            super::PaintNodePhase::BeforeChildren,
            0,
            super::PaintChunkRole::SelfDecoration,
            outer_state,
        ) && exact_self_decoration(chunk)
    };
    let glyph = |chunk: &super::PaintChunk| {
        exact_chunk(
            chunk,
            admission.text_area_root,
            super::PaintPropertyScope::Contents,
            super::PaintNodePhase::BeforeChildren,
            1,
            super::PaintChunkRole::TextGlyphs,
            text_state,
        ) && matches!(&artifact.ops[chunk.op_range.clone()], [super::PaintOp::PreparedText(prepared)]
            if prepared.has_canonical_identity()
                && chunk.payload_identity
                    == super::PaintPayloadIdentity::prepared_texts([prepared]))
            && host_preedit_seal.as_ref().is_none_or(|seal| {
                chunk.payload_identity == seal.glyph_identity
                    && [
                        chunk.bounds.x,
                        chunk.bounds.y,
                        chunk.bounds.width,
                        chunk.bounds.height,
                    ]
                    .map(f32::to_bits)
                        == seal.glyph_bounds_bits
            })
    };
    let selection = |chunk: &super::PaintChunk| {
        exact_chunk(
            chunk,
            admission.text_area_root,
            super::PaintPropertyScope::Contents,
            super::PaintNodePhase::BeforeChildren,
            0,
            super::PaintChunkRole::SelectionUnderlay,
            text_state,
        ) && {
            let rects = artifact.ops[chunk.op_range.clone()]
                .iter()
                .map(|op| match op {
                    super::PaintOp::DrawRect(rect) => Some(rect),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>();
            let Some(rects) = rects.filter(|rects| !rects.is_empty()) else {
                return false;
            };
            let crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedSelectionGlyphs {
                start_char,
                end_char,
                color_rgba_bits,
            } = admission.paint_grammar else {
                return false;
            };
            let grammar = crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
                start_char,
                end_char,
                color_rgba_bits,
            };
            chunk.payload_identity.matches_exact_text_area_selection(
                grammar,
                rects.len(),
                [
                    chunk.bounds.x,
                    chunk.bounds.y,
                    chunk.bounds.width,
                    chunk.bounds.height,
                ]
                .map(f32::to_bits),
            ) && chunk
                .payload_identity
                .matches_exact_text_area_selection_ops(rects.into_iter())
        }
    };
    let underline = |chunk: &super::PaintChunk| {
        exact_chunk(
            chunk,
            admission.text_area_root,
            super::PaintPropertyScope::Contents,
            super::PaintNodePhase::AfterChildren,
            0,
            super::PaintChunkRole::TextDecoration,
            text_state,
        ) && {
            let Some(seal) = host_preedit_seal.as_ref() else {
                return false;
            };
            let rects = artifact.ops[chunk.op_range.clone()]
                .iter()
                .map(|op| match op {
                    super::PaintOp::DrawRect(rect) => Some(rect),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>();
            let Some(rects) = rects.filter(|rects| !rects.is_empty()) else {
                return false;
            };
            chunk.payload_identity == seal.underline_identity
                && super::PaintPayloadIdentity::prepared_rects(rects.iter().copied()).as_ref()
                    == Some(&chunk.payload_identity)
                && chunk.payload_identity.matches_exact_fill_rects(
                    rects.len(),
                    seal.foreground_color_bits,
                    seal.underline_bounds_bits,
                )
                && [
                    chunk.bounds.x,
                    chunk.bounds.y,
                    chunk.bounds.width,
                    chunk.bounds.height,
                ]
                .map(f32::to_bits)
                    == seal.underline_bounds_bits
        }
    };
    let overlay = |chunk: &super::PaintChunk| {
        exact_chunk(
            chunk,
            admission.boundary_root,
            super::PaintPropertyScope::SelfPaint,
            super::PaintNodePhase::AfterChildren,
            0,
            super::PaintChunkRole::ScrollbarOverlay,
            Default::default(),
        ) && match admission.scroll.scrollbar_overlay.paint_state {
            crate::view::base_component::ScrollbarPaintStateWitness::HiddenNow
            | crate::view::base_component::ScrollbarPaintStateWitness::NotPaintable => {
                chunk.op_range.is_empty()
                    && chunk.payload_identity
                        == super::PaintPayloadIdentity::prepared_shadows(std::iter::empty())
            }
            crate::view::base_component::ScrollbarPaintStateWitness::OpaqueNow
            | crate::view::base_component::ScrollbarPaintStateWitness::TranslucentNow => {
                matches!(
                    &artifact.ops[chunk.op_range.clone()],
                    [super::PaintOp::PreparedScrollbarOverlay(op)]
                        if chunk.payload_identity
                            == super::PaintPayloadIdentity::prepared_scrollbar_overlay(op)
                )
            }
        }
    };
    let chunks_match = match admission.paint_grammar {
        crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedGlyphs => {
            matches!(artifact.chunks.as_slice(), [a, b, c, d]
                if root_before(a) && wrapper(b) && glyph(c) && overlay(d))
        }
        crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedSelectionGlyphs { .. } => {
            matches!(artifact.chunks.as_slice(), [a, b, c, d, e]
                if root_before(a) && wrapper(b) && selection(c) && glyph(d) && overlay(e))
        }
        crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedPreeditGlyphs => {
            matches!(artifact.chunks.as_slice(), [a, b, c, d, e]
                if root_before(a) && wrapper(b) && glyph(c) && underline(d) && overlay(e))
        }
    };
    let op_ranges_are_closed = artifact.chunks.iter().try_fold(0usize, |cursor, chunk| {
        (chunk.op_range.start == cursor).then_some(chunk.op_range.end)
    }) == Some(artifact.ops.len());
    if roots != [admission.boundary_root]
        || baked.boundary_root() != admission.boundary_root
        || baked.child() != admission.content_wrapper
        || !matches!(artifact.target, PaintArtifactTarget::CurrentTarget)
        || !chunks_match
        || !op_ranges_are_closed
        || !artifact.effect_nodes.is_empty()
        || artifact.clip_nodes.as_slice()
            != [
                outer.contents_clip_snapshot(),
                text_area_witness.live_contents_clip(),
            ]
        || artifact.owner_nodes.as_slice()
            != [
                super::PaintOwnerSnapshot {
                    owner: admission.boundary_root,
                    parent: None,
                },
                super::PaintOwnerSnapshot {
                    owner: admission.content_wrapper,
                    parent: Some(admission.boundary_root),
                },
                super::PaintOwnerSnapshot {
                    owner: admission.text_area_root,
                    parent: Some(admission.content_wrapper),
                },
            ]
    {
        return Err(invalid(admission.boundary_root));
    }
    Ok(artifact)
}

/// C1/C2a host recorder. It records the exact root-before / wrapper / TextArea /
/// root-overlay order under live properties; callers extract only the two root
/// chunks after this full metadata/full pass succeeds.
pub(super) fn record_baked_scroll_text_area_subtree_host_artifact_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    admission: RetainedScrollTextAreaSubtreeAdmissionSnapshot,
    baked: PaintBakedScrollHostWitness,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    let outer_clip_chain = property_trees
        .clip_snapshot_for(Some(baked.contents_clip()))
        .ok_or_else(|| {
            vec![FrameArtifactFallbackReason::PropertyBoundary(
                admission.boundary_root,
            )]
        })?;
    let [outer_clip] = outer_clip_chain.as_slice() else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            admission.boundary_root,
        )]);
    };
    let outer = PaintScrollContentWitness::new(
        admission.boundary_root,
        admission.content_wrapper,
        property_trees
            .scroll_snapshot_for(baked.scroll())
            .ok_or_else(|| {
                vec![FrameArtifactFallbackReason::PropertyBoundary(
                    admission.boundary_root,
                )]
            })?,
        *outer_clip,
    )
    .ok_or_else(|| {
        vec![FrameArtifactFallbackReason::PropertyBoundary(
            admission.boundary_root,
        )]
    })?;
    let text_area_node = arena.get(admission.text_area_root).ok_or_else(|| {
        vec![FrameArtifactFallbackReason::PropertyBoundary(
            admission.text_area_root,
        )]
    })?;
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::TextArea>()
        .ok_or_else(|| {
            vec![FrameArtifactFallbackReason::PropertyBoundary(
                admission.text_area_root,
            )]
        })?;
    let wrapper_node = arena.get(admission.content_wrapper).ok_or_else(|| {
        vec![FrameArtifactFallbackReason::PropertyBoundary(
            admission.content_wrapper,
        )]
    })?;
    let wrapper = wrapper_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
        .ok_or_else(|| {
            vec![FrameArtifactFallbackReason::PropertyBoundary(
                admission.content_wrapper,
            )]
        })?;
    let recording_offset = wrapper
        .exact_retained_scroll_content_wrapper_recording_offset(outer.normalization_paint_offset())
        .ok_or_else(|| {
            vec![FrameArtifactFallbackReason::PropertyBoundary(
                admission.content_wrapper,
            )]
        })?;
    if !text_area_matches_admitted_paint_grammar(
        text_area,
        admission.text_area_root,
        arena,
        recording_offset,
        admission.paint_grammar,
    ) {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            admission.text_area_root,
        )]);
    }
    let local_clip_id = crate::view::compositor::property_tree::ClipNodeId {
        owner: admission.text_area_root,
        role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
    };
    let live_chain = property_trees
        .clip_snapshot_for(Some(local_clip_id))
        .ok_or_else(|| {
            vec![FrameArtifactFallbackReason::PropertyBoundary(
                admission.text_area_root,
            )]
        })?;
    let [live_text_area_clip, live_outer_clip] = live_chain.as_slice() else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            admission.text_area_root,
        )]);
    };
    if live_outer_clip != outer_clip {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            admission.text_area_root,
        )]);
    }
    let local_scissor = text_area
        .retained_property_scroll_local_contents_scissor(outer.normalization_paint_offset())
        .ok_or_else(|| {
            vec![FrameArtifactFallbackReason::PropertyBoundary(
                admission.text_area_root,
            )]
        })?;
    let text_area_witness = PaintScrollTextAreaSubtreeWitness::new(
        outer,
        admission.text_area_root,
        *live_text_area_clip,
        local_scissor,
        admission.paint_grammar,
    )
    .ok_or_else(|| {
        vec![FrameArtifactFallbackReason::PropertyBoundary(
            admission.text_area_root,
        )]
    })?;
    let artifact = match record_frame_artifact_with_policy(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        FrameArtifactAuthorityPolicy::BakedScrollTextAreaSubtreeHost(baked, text_area_witness),
        None,
        None,
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    let Some(first) = artifact.chunks.first() else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            admission.boundary_root,
        )]);
    };
    let Some(last) = artifact.chunks.last() else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            admission.boundary_root,
        )]);
    };
    let clips = artifact
        .clip_nodes
        .iter()
        .map(|clip| clip.id)
        .collect::<FxHashSet<_>>();
    if roots != [admission.boundary_root]
        || baked.boundary_root() != admission.boundary_root
        || baked.child() != admission.content_wrapper
        || first.owner != admission.boundary_root
        || first.id.phase != super::PaintNodePhase::BeforeChildren
        || first.id.role != super::PaintChunkRole::SelfDecoration
        || last.owner != admission.boundary_root
        || last.id.phase != super::PaintNodePhase::AfterChildren
        || last.id.role != super::PaintChunkRole::ScrollbarOverlay
        || !artifact.effect_nodes.is_empty()
        || clips
            != FxHashSet::from_iter([
                outer.contents_clip_snapshot().id,
                text_area_witness.live_contents_clip().id,
            ])
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            admission.boundary_root,
        )]);
    }
    Ok(artifact)
}

/// C3a full host grammar.  It is deliberately graph-inert: callers may test
/// and validate this artifact, but no scroll-scene selector consumes it.
pub(super) fn record_baked_scroll_atomic_projection_text_area_subtree_host_artifact_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    admission: &RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot,
    baked: PaintBakedScrollHostWitness,
) -> Result<RecordedRetainedAtomicProjectionTextAreaHost, Vec<FrameArtifactFallbackReason>> {
    let invalid = |owner| vec![FrameArtifactFallbackReason::PropertyBoundary(owner)];
    let outer_chain = property_trees
        .clip_snapshot_for(Some(baked.contents_clip()))
        .ok_or_else(|| invalid(admission.boundary_root))?;
    let [outer_clip] = outer_chain.as_slice() else {
        return Err(invalid(admission.boundary_root));
    };
    let outer = PaintScrollContentWitness::new(
        admission.boundary_root,
        admission.content_wrapper,
        property_trees
            .scroll_snapshot_for(baked.scroll())
            .ok_or_else(|| invalid(admission.boundary_root))?,
        *outer_clip,
    )
    .ok_or_else(|| invalid(admission.boundary_root))?;
    if roots != [admission.boundary_root]
        || baked.boundary_root() != admission.boundary_root
        || baked.child() != admission.content_wrapper
        || !admission.matches_scroll_node(outer.scroll_snapshot())
        || !promoted_node_ids.is_empty()
    {
        return Err(invalid(admission.boundary_root));
    }
    let wrapper_node = arena
        .get(admission.content_wrapper)
        .ok_or_else(|| invalid(admission.content_wrapper))?;
    let wrapper = wrapper_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
        .ok_or_else(|| invalid(admission.content_wrapper))?;
    let recording_offset = wrapper
        .exact_retained_scroll_content_wrapper_recording_offset(outer.normalization_paint_offset())
        .ok_or_else(|| invalid(admission.content_wrapper))?;
    let text_area_node = arena
        .get(admission.text_area_root)
        .ok_or_else(|| invalid(admission.text_area_root))?;
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::TextArea>()
        .ok_or_else(|| invalid(admission.text_area_root))?;
    if arena
        .get(admission.boundary_root)
        .is_none_or(|node| node.element.stable_id() != admission.stable_id)
        || wrapper_node.element.stable_id() != admission.content_wrapper_stable_id
        || text_area_node.element.stable_id() != admission.text_area_stable_id
    {
        return Err(invalid(admission.boundary_root));
    }
    let source_before = text_area
        .exact_retained_property_scroll_atomic_projection_subtree(
            admission.text_area_root,
            arena,
            recording_offset,
        )
        .filter(|grammar| grammar == &admission.paint_grammar)
        .ok_or_else(|| invalid(admission.text_area_root))?;
    let local_clip_id = crate::view::compositor::property_tree::ClipNodeId {
        owner: admission.text_area_root,
        role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
    };
    let live_chain = property_trees
        .clip_snapshot_for(Some(local_clip_id))
        .ok_or_else(|| invalid(admission.text_area_root))?;
    let [live_text_area_clip, live_outer_clip] = live_chain.as_slice() else {
        return Err(invalid(admission.text_area_root));
    };
    if live_outer_clip != outer_clip {
        return Err(invalid(admission.text_area_root));
    }
    let local_scissor = text_area
        .retained_property_scroll_local_contents_scissor(outer.normalization_paint_offset())
        .ok_or_else(|| invalid(admission.text_area_root))?;
    let recorder_authority = AtomicProjectionRecorderWitness::ExistingAtomicGlyph(
        PaintScrollAtomicProjectionTextAreaSubtreeWitness::new(
            outer,
            admission.text_area_root,
            *live_text_area_clip,
            local_scissor,
            &source_before,
        )
        .ok_or_else(|| invalid(admission.text_area_root))?,
    );
    let mut owners = vec![
        super::PaintOwnerSnapshot {
            owner: admission.boundary_root,
            parent: None,
        },
        super::PaintOwnerSnapshot {
            owner: admission.content_wrapper,
            parent: Some(admission.boundary_root),
        },
        super::PaintOwnerSnapshot {
            owner: admission.text_area_root,
            parent: Some(admission.content_wrapper),
        },
    ];
    for seal in source_before.topology.iter() {
        owners.push(super::PaintOwnerSnapshot {
            owner: seal.owner,
            parent: Some(admission.text_area_root),
        });
        if seal.owner == source_before.projection_owner {
            owners.push(super::PaintOwnerSnapshot {
                owner: source_before.projection_text_owner,
                parent: Some(seal.owner),
            });
        }
    }
    let policy = FrameArtifactAuthorityPolicy::BakedScrollAtomicProjectionTextAreaSubtreeHost(
        baked,
        recorder_authority,
    );
    let oracle_context = PaintRecordingContext {
        baked_scroll_host: Some(baked),
        baked_scroll_atomic_projection_text_area_subtree: Some(recorder_authority),
        opacity_authority: PaintOpacityAuthority::Baked,
        ..PaintRecordingContext::default()
    };
    let raster_before = record_atomic_projection_live_raster_oracle(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        policy,
        oracle_context,
        admission.content_wrapper,
        admission.text_area_root,
        &source_before,
        owners.clone(),
    )?;
    let mut artifact = match record_frame_artifact_with_policy(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        policy,
        None,
        None,
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    let source_after = text_area
        .exact_retained_property_scroll_atomic_projection_subtree(
            admission.text_area_root,
            arena,
            recording_offset,
        )
        .ok_or_else(|| {
            vec![FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::RecordingPassMismatch,
            )]
        })?;
    if source_before != source_after || source_after != admission.paint_grammar {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    // Coverage materializes chunk owners and ancestors. C3a also seals
    // generated no-paint siblings from the source oracle.
    artifact.owner_nodes = owners.clone();
    let raster_after = record_atomic_projection_live_raster_oracle(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        policy,
        oracle_context,
        admission.content_wrapper,
        admission.text_area_root,
        &source_after,
        owners.clone(),
    )?;
    if raster_before != raster_after || !raster_before.matches_artifact(&artifact) {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    let [
        root_before,
        wrapper_chunk,
        root_glyph,
        projection_glyph,
        overlay,
    ] = artifact.chunks.as_slice()
    else {
        return Err(invalid(admission.boundary_root));
    };
    let clips = artifact
        .clip_nodes
        .iter()
        .map(|clip| clip.id)
        .collect::<FxHashSet<_>>();
    let outer_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(baked.contents_clip()),
        scroll: Some(baked.scroll()),
        ..Default::default()
    };
    let glyph_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(recorder_authority.live_contents_clip().id),
        scroll: Some(baked.scroll()),
        ..Default::default()
    };
    let glyph_exact = |chunk: &super::PaintChunk, owner: NodeKey, scope| {
        matches!(&artifact.ops[chunk.op_range.clone()], [super::PaintOp::PreparedText(prepared)]
            if prepared.has_canonical_identity()
                && chunk.payload_identity == super::PaintPayloadIdentity::prepared_texts([prepared]))
            && chunk.owner == owner
            && chunk.id.owner == owner
            && chunk.id.scope == scope
            && chunk.id.phase == super::PaintNodePhase::BeforeChildren
            && chunk.id.slot == 1
            && chunk.id.role == super::PaintChunkRole::TextGlyphs
            && chunk.properties == glyph_state
    };
    if artifact.owner_nodes != owners
        || !artifact.effect_nodes.is_empty()
        || artifact.clip_nodes.len() != 2
        || clips
            != FxHashSet::from_iter([
                outer.contents_clip_snapshot().id,
                recorder_authority.live_contents_clip().id,
            ])
        || root_before.owner != admission.boundary_root
        || root_before.id.owner != admission.boundary_root
        || root_before.id.scope != super::PaintPropertyScope::SelfPaint
        || root_before.id.phase != super::PaintNodePhase::BeforeChildren
        || root_before.id.slot != 0
        || root_before.id.role != super::PaintChunkRole::SelfDecoration
        || wrapper_chunk.owner != admission.content_wrapper
        || wrapper_chunk.id.owner != admission.content_wrapper
        || wrapper_chunk.id.scope != super::PaintPropertyScope::SelfPaint
        || wrapper_chunk.id.phase != super::PaintNodePhase::BeforeChildren
        || wrapper_chunk.id.slot != 0
        || wrapper_chunk.id.role != super::PaintChunkRole::SelfDecoration
        || wrapper_chunk.properties != outer_state
        || !glyph_exact(
            root_glyph,
            admission.text_area_root,
            super::PaintPropertyScope::Contents,
        )
        || !glyph_exact(
            projection_glyph,
            source_before.projection_text_owner,
            super::PaintPropertyScope::SelfPaint,
        )
        || [
            projection_glyph.bounds.x,
            projection_glyph.bounds.y,
            projection_glyph.bounds.width,
            projection_glyph.bounds.height,
        ]
        .map(f32::to_bits)
            != source_before.projection_text_bounds_bits
        || overlay.owner != admission.boundary_root
        || overlay.id.owner != admission.boundary_root
        || overlay.id.scope != super::PaintPropertyScope::SelfPaint
        || overlay.id.phase != super::PaintNodePhase::AfterChildren
        || overlay.id.slot != 0
        || overlay.id.role != super::PaintChunkRole::ScrollbarOverlay
    {
        return Err(invalid(admission.boundary_root));
    }
    Ok(RecordedRetainedAtomicProjectionTextAreaHost {
        artifact,
        raster_oracle: raster_before,
        source_bounds_bits: [
            admission.source_bounds.x.to_bits(),
            admission.source_bounds.y.to_bits(),
            admission.source_bounds.width.to_bits(),
            admission.source_bounds.height.to_bits(),
        ],
        outer_scroll: outer.scroll_snapshot(),
        outer_contents_clip: outer.contents_clip_snapshot(),
        local_contents_clip: recorder_authority.local_contents_clip(),
    })
}

/// Graph-inert host recorder for the root-owned selection atomic-projection
/// grammar.  It records H -> wrapper -> selection -> root glyph -> projection
/// glyph -> O and returns no raw artifact access.
pub(super) fn record_baked_scroll_atomic_projection_selection_text_area_subtree_host_artifact_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    admission: &RetainedScrollAtomicProjectionSelectionTextAreaSubtreeAdmissionSnapshot,
    baked: PaintBakedScrollHostWitness,
) -> Result<RecordedRetainedAtomicProjectionSelectionTextAreaHost, Vec<FrameArtifactFallbackReason>>
{
    let invalid = |owner| vec![FrameArtifactFallbackReason::PropertyBoundary(owner)];
    let outer_chain = property_trees
        .clip_snapshot_for(Some(baked.contents_clip()))
        .ok_or_else(|| invalid(admission.boundary_root))?;
    let [outer_clip] = outer_chain.as_slice() else {
        return Err(invalid(admission.boundary_root));
    };
    let outer = PaintScrollContentWitness::new(
        admission.boundary_root,
        admission.content_wrapper,
        property_trees
            .scroll_snapshot_for(baked.scroll())
            .ok_or_else(|| invalid(admission.boundary_root))?,
        *outer_clip,
    )
    .ok_or_else(|| invalid(admission.boundary_root))?;
    if roots != [admission.boundary_root]
        || baked.boundary_root() != admission.boundary_root
        || baked.child() != admission.content_wrapper
        || !admission.matches_scroll_node(outer.scroll_snapshot())
        || !promoted_node_ids.is_empty()
    {
        return Err(invalid(admission.boundary_root));
    }
    let wrapper_node = arena
        .get(admission.content_wrapper)
        .ok_or_else(|| invalid(admission.content_wrapper))?;
    let wrapper = wrapper_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
        .ok_or_else(|| invalid(admission.content_wrapper))?;
    let recording_offset = wrapper
        .exact_retained_scroll_content_wrapper_recording_offset(outer.normalization_paint_offset())
        .ok_or_else(|| invalid(admission.content_wrapper))?;
    let text_area_node = arena
        .get(admission.text_area_root)
        .ok_or_else(|| invalid(admission.text_area_root))?;
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::TextArea>()
        .ok_or_else(|| invalid(admission.text_area_root))?;
    if arena
        .get(admission.boundary_root)
        .is_none_or(|node| node.element.stable_id() != admission.stable_id)
        || wrapper_node.element.stable_id() != admission.content_wrapper_stable_id
        || text_area_node.element.stable_id() != admission.text_area_stable_id
    {
        return Err(invalid(admission.boundary_root));
    }
    let source_before = text_area
        .exact_retained_property_scroll_atomic_projection_selection_subtree(
            admission.text_area_root,
            arena,
            recording_offset,
        )
        .filter(|grammar| grammar == &admission.paint_grammar)
        .ok_or_else(|| invalid(admission.text_area_root))?;
    let local_clip_id = crate::view::compositor::property_tree::ClipNodeId {
        owner: admission.text_area_root,
        role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
    };
    let live_chain = property_trees
        .clip_snapshot_for(Some(local_clip_id))
        .ok_or_else(|| invalid(admission.text_area_root))?;
    let [live_text_area_clip, live_outer_clip] = live_chain.as_slice() else {
        return Err(invalid(admission.text_area_root));
    };
    if live_outer_clip != outer_clip {
        return Err(invalid(admission.text_area_root));
    }
    let local_scissor = text_area
        .retained_property_scroll_local_contents_scissor(outer.normalization_paint_offset())
        .ok_or_else(|| invalid(admission.text_area_root))?;
    let recorder_witness = PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness::new(
        outer,
        admission.text_area_root,
        *live_text_area_clip,
        local_scissor,
        &source_before,
    )
    .ok_or_else(|| invalid(admission.text_area_root))?;
    let recorder_authority =
        AtomicProjectionRecorderWitness::AtomicProjectionSelection(recorder_witness);
    let mut owners = vec![
        super::PaintOwnerSnapshot {
            owner: admission.boundary_root,
            parent: None,
        },
        super::PaintOwnerSnapshot {
            owner: admission.content_wrapper,
            parent: Some(admission.boundary_root),
        },
        super::PaintOwnerSnapshot {
            owner: admission.text_area_root,
            parent: Some(admission.content_wrapper),
        },
    ];
    for seal in source_before.atomic_source.topology.iter() {
        owners.push(super::PaintOwnerSnapshot {
            owner: seal.owner,
            parent: Some(admission.text_area_root),
        });
        if seal.owner == source_before.atomic_source.projection_owner {
            owners.push(super::PaintOwnerSnapshot {
                owner: source_before.atomic_source.projection_text_owner,
                parent: Some(seal.owner),
            });
        }
    }
    let policy = FrameArtifactAuthorityPolicy::BakedScrollAtomicProjectionTextAreaSubtreeHost(
        baked,
        recorder_authority,
    );
    let oracle_context = PaintRecordingContext {
        baked_scroll_host: Some(baked),
        baked_scroll_atomic_projection_text_area_subtree: Some(recorder_authority),
        opacity_authority: PaintOpacityAuthority::Baked,
        ..PaintRecordingContext::default()
    };
    let mut raster_before = record_atomic_projection_selection_live_raster_oracle(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        policy,
        oracle_context,
        admission.content_wrapper,
        admission.text_area_root,
        &source_before,
        owners.clone(),
    )?;
    let mut artifact = match record_frame_artifact_with_policy(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        policy,
        None,
        None,
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    let source_after = text_area
        .exact_retained_property_scroll_atomic_projection_selection_subtree(
            admission.text_area_root,
            arena,
            recording_offset,
        )
        .ok_or_else(|| {
            vec![FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::RecordingPassMismatch,
            )]
        })?;
    if source_before != source_after || source_after != admission.paint_grammar {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    artifact.owner_nodes = owners.clone();
    normalize_atomic_projection_selection_chunk(
        &mut artifact,
        &mut raster_before,
        2,
        recorder_witness.selection,
    )
    .ok_or_else(|| invalid(admission.text_area_root))?;
    let mut raster_after = record_atomic_projection_selection_live_raster_oracle(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        policy,
        oracle_context,
        admission.content_wrapper,
        admission.text_area_root,
        &source_after,
        owners.clone(),
    )?;
    normalize_atomic_projection_selection_chunk(
        &mut artifact,
        &mut raster_after,
        2,
        recorder_witness.selection,
    )
    .ok_or_else(|| invalid(admission.text_area_root))?;
    if raster_before != raster_after || !raster_before.matches_artifact(&artifact) {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    let [
        root_before,
        wrapper_chunk,
        selection,
        root_glyph,
        projection_glyph,
        overlay,
    ] = artifact.chunks.as_slice()
    else {
        return Err(invalid(admission.boundary_root));
    };
    let glyph_exact = |chunk: &super::PaintChunk, owner, scope| {
        matches!(&artifact.ops[chunk.op_range.clone()], [super::PaintOp::PreparedText(prepared)]
            if prepared.has_canonical_identity()
                && chunk.payload_identity == super::PaintPayloadIdentity::prepared_texts([prepared]))
            && chunk.owner == owner
            && chunk.id.owner == owner
            && chunk.id.scope == scope
            && chunk.id.phase == super::PaintNodePhase::BeforeChildren
            && chunk.id.slot == 1
            && chunk.id.role == super::PaintChunkRole::TextGlyphs
    };
    let selection_exact = matches!(&artifact.ops[selection.op_range.clone()], ops
    if !ops.is_empty()
        && ops.iter().all(|op| matches!(op, super::PaintOp::DrawRect(_)))
        && selection.payload_identity.retained_text_area_selection_grammar()
            == Some(recorder_witness.selection)
        && selection.payload_identity.matches_exact_text_area_selection_ops(
            ops.iter().filter_map(|op| match op { super::PaintOp::DrawRect(rect) => Some(rect), _ => None })
        ));
    if artifact.owner_nodes != owners
        || !artifact.effect_nodes.is_empty()
        || artifact.clip_nodes.len() != 2
        || root_before.owner != admission.boundary_root
        || root_before.id.phase != super::PaintNodePhase::BeforeChildren
        || root_before.id.role != super::PaintChunkRole::SelfDecoration
        || wrapper_chunk.owner != admission.content_wrapper
        || wrapper_chunk.id.role != super::PaintChunkRole::SelfDecoration
        || selection.owner != admission.text_area_root
        || selection.id.owner != admission.text_area_root
        || selection.id.scope != super::PaintPropertyScope::Contents
        || selection.id.phase != super::PaintNodePhase::BeforeChildren
        || selection.id.slot != 0
        || selection.id.role != super::PaintChunkRole::SelectionUnderlay
        || !selection_exact
        || !glyph_exact(
            root_glyph,
            admission.text_area_root,
            super::PaintPropertyScope::Contents,
        )
        || !glyph_exact(
            projection_glyph,
            source_before.atomic_source.projection_text_owner,
            super::PaintPropertyScope::SelfPaint,
        )
        || [
            projection_glyph.bounds.x,
            projection_glyph.bounds.y,
            projection_glyph.bounds.width,
            projection_glyph.bounds.height,
        ]
        .map(f32::to_bits)
            != source_before.atomic_source.projection_text_bounds_bits
        || overlay.owner != admission.boundary_root
        || overlay.id.phase != super::PaintNodePhase::AfterChildren
        || overlay.id.role != super::PaintChunkRole::ScrollbarOverlay
    {
        return Err(invalid(admission.boundary_root));
    }
    Ok(RecordedRetainedAtomicProjectionSelectionTextAreaHost {
        artifact,
        raster_oracle: raster_before,
        source_bounds_bits: [
            admission.source_bounds.x.to_bits(),
            admission.source_bounds.y.to_bits(),
            admission.source_bounds.width.to_bits(),
            admission.source_bounds.height.to_bits(),
        ],
        outer_scroll: outer.scroll_snapshot(),
        outer_contents_clip: outer.contents_clip_snapshot(),
        local_contents_clip: recorder_authority.local_contents_clip(),
    })
}

pub(super) fn record_baked_scroll_host_artifact_with_stack_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintBakedScrollHostWitness,
    consumed_stack: super::ConsumedAncestorPropertyStackWitness,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    match record_frame_artifact_with_policy_and_stack(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        FrameArtifactAuthorityPolicy::BakedScrollHost(witness),
        None,
        Some(consumed_stack),
        None,
        None,
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => Ok(artifact),
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            Err(eligibility.reasons)
        }
        Err(error) => Err(error.reasons),
    }
}

/// Strict E->S checkpoint recorder. H/O keep the baked-scroll structural
/// witness while their inherited effect is projected by the exact stack and
/// neutralized by the same root-effect authority as the receiver artifact.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_effect_baked_scroll_host_artifact_with_stack_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintBakedScrollHostWitness,
    consumed_stack: super::ConsumedAncestorPropertyStackWitness,
    effect: EffectNodeSnapshot,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    if effect.id.0 != effect.owner
        || effect.parent.is_some()
        || effect.generation == 0
        || !effect.opacity.is_finite()
        || !(0.0..=1.0).contains(&effect.opacity)
    {
        return Err(vec![FrameArtifactFallbackReason::InvalidRootEffect(
            effect.owner,
        )]);
    }
    match record_frame_artifact_with_policy_and_stack(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        FrameArtifactAuthorityPolicy::BakedScrollHost(witness),
        None,
        Some(consumed_stack),
        Some(effect.id),
        None,
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => Ok(artifact),
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            Err(eligibility.reasons)
        }
        Err(error) => Err(error.reasons),
    }
}

/// Records only the scroll host's detached content subtree in offset-zero
/// geometry. The host's self paint and scrollbar overlay remain separate
/// scene artifacts and never enter this recorder.
pub(super) fn record_scroll_content_local_artifact_for_plan(
    arena: &NodeArena,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintScrollContentWitness,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    let boundary_root = witness.boundary_root();
    let content_root = witness.content_root();
    let scroll = witness.scroll_snapshot();
    let contents_clip = witness.contents_clip_snapshot();
    let normalization_offset = witness.normalization_paint_offset();
    let Some(boundary_node) = arena.get(boundary_root) else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    };
    let Some(content_node) = arena.get(content_root) else {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::MissingNode(content_root),
        )]);
    };
    let Some(content_element) = content_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
    else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    };
    let Some(required_paint_offset) =
        content_element.exact_retained_scroll_content_recording_offset(normalization_offset)
    else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    };
    let content_bounds =
        crate::view::base_component::ElementTrait::box_model_snapshot(content_element);
    let exact_property = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(contents_clip.id),
        scroll: Some(scroll.id),
        ..Default::default()
    };
    if boundary_node.element.children() != [content_root]
        || arena.parent_of(content_root) != Some(boundary_root)
        || (content_bounds.x + scroll.offset.x).to_bits()
            != scroll.layout_content_bounds_at_zero.x.to_bits()
        || (content_bounds.y + scroll.offset.y).to_bits()
            != scroll.layout_content_bounds_at_zero.y.to_bits()
        || content_bounds.width.to_bits() != scroll.content_size.width.to_bits()
        || content_bounds.height.to_bits() != scroll.content_size.height.to_bits()
        || !promoted_node_ids.is_empty()
        || !property_trees.validation_errors.is_empty()
        || property_trees.transforms.len() != 0
        || property_trees.effects.len() != 0
        || property_trees.scroll_snapshot_for(scroll.id) != Some(scroll)
        || property_trees
            .clip_snapshot_for(Some(contents_clip.id))
            .is_none_or(|snapshots| snapshots.as_slice() != [contents_clip])
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    }

    let mut stack = vec![(content_root, boundary_root)];
    let mut seen = FxHashSet::default();
    let mut expected_owner_parents = FxHashMap::default();
    while let Some((key, expected_parent)) = stack.pop() {
        if !seen.insert(key) {
            return Err(vec![FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::DuplicateNodeKey(key),
            )]);
        }
        let Some(node) = arena.get(key) else {
            return Err(vec![FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::MissingNode(key),
            )]);
        };
        if arena.parent_of(key) != Some(expected_parent) {
            return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(key)]);
        }
        expected_owner_parents.insert(key, (key != content_root).then_some(expected_parent));
        if node.element.is_deferred_to_root_viewport_render() {
            return Err(vec![FrameArtifactFallbackReason::DeferredBoundary(key)]);
        }
        if node
            .element
            .placement_eligibility_metadata()
            .contains_runtime_layout_state
            || property_trees.states.get(&key).is_none_or(|state| {
                state.paint != exact_property || state.descendants != exact_property
            })
        {
            return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(key)]);
        }
        stack.extend(
            node.element
                .children()
                .iter()
                .copied()
                .map(|child| (child, key)),
        );
    }

    let artifact = match record_frame_artifact_with_policy(
        arena,
        &[content_root],
        promoted_node_ids,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        FrameArtifactAuthorityPolicy::ScrollContentLocal(witness),
        None,
        Some(required_paint_offset.map(f32::to_bits)),
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    let detached_root_count = artifact
        .owner_nodes
        .iter()
        .filter(|snapshot| snapshot.parent.is_none())
        .count();
    let has_detached_content_root = artifact
        .owner_nodes
        .iter()
        .any(|snapshot| snapshot.owner == content_root && snapshot.parent.is_none());
    if artifact.chunks.is_empty()
        || !matches!(artifact.target, PaintArtifactTarget::CurrentTarget)
        || !artifact.clip_nodes.is_empty()
        || !artifact.effect_nodes.is_empty()
        || detached_root_count != 1
        || !has_detached_content_root
        || artifact.owner_nodes.len() != seen.len()
        || artifact.owner_nodes.iter().any(|snapshot| {
            expected_owner_parents.get(&snapshot.owner).copied() != Some(snapshot.parent)
        })
        || seen.iter().any(|owner| {
            !artifact
                .owner_nodes
                .iter()
                .any(|snapshot| snapshot.owner == *owner)
        })
        || artifact
            .chunks
            .iter()
            .any(|chunk| chunk.properties != Default::default())
        || artifact.chunks.iter().any(|chunk| {
            chunk.id.role == super::PaintChunkRole::ScrollbarOverlay || chunk.owner == boundary_root
        })
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    }
    Ok(artifact)
}

/// Records the closed C1/C2a `Element wrapper -> plain TextArea` content subtree.
/// The outer scroll/clip pair is consumed, while the TextArea contents clip is
/// retained as one localized, parentless artifact clip.
pub(super) fn record_scroll_text_area_subtree_local_artifact_for_plan(
    arena: &NodeArena,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    admission: RetainedScrollTextAreaSubtreeAdmissionSnapshot,
    outer: PaintScrollContentWitness,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    let content_root = admission.content_wrapper;
    let text_area_root = admission.text_area_root;
    let invalid = |owner| vec![FrameArtifactFallbackReason::PropertyBoundary(owner)];
    if outer.boundary_root() != admission.boundary_root
        || outer.content_root() != content_root
        || outer.scroll_snapshot().owner != admission.boundary_root
        || !admission.matches_scroll_node(outer.scroll_snapshot())
        || !promoted_node_ids.is_empty()
        || !property_trees.validation_errors.is_empty()
        || !property_trees.transforms.is_empty()
        || !property_trees.effects.is_empty()
    {
        return Err(invalid(content_root));
    }
    let content_node = arena
        .get(content_root)
        .ok_or_else(|| invalid(content_root))?;
    let content = content_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
        .ok_or_else(|| invalid(content_root))?;
    let required_paint_offset = content
        .exact_retained_scroll_content_wrapper_recording_offset(outer.normalization_paint_offset())
        .ok_or_else(|| invalid(content_root))?;
    let text_area_node = arena
        .get(text_area_root)
        .ok_or_else(|| invalid(text_area_root))?;
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::TextArea>()
        .ok_or_else(|| invalid(text_area_root))?;
    if !text_area_matches_admitted_paint_grammar(
        text_area,
        text_area_root,
        arena,
        required_paint_offset,
        admission.paint_grammar,
    ) {
        return Err(invalid(text_area_root));
    }
    let local_scissor = text_area
        .retained_property_scroll_local_contents_scissor(outer.normalization_paint_offset())
        .ok_or_else(|| invalid(text_area_root))?;
    let local_clip_id = crate::view::compositor::property_tree::ClipNodeId {
        owner: text_area_root,
        role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
    };
    let live_chain = property_trees
        .clip_snapshot_for(Some(local_clip_id))
        .ok_or_else(|| invalid(text_area_root))?;
    let [live_text_area_clip, live_outer_clip] = live_chain.as_slice() else {
        return Err(invalid(text_area_root));
    };
    if *live_outer_clip != outer.contents_clip_snapshot() {
        return Err(invalid(text_area_root));
    }
    let witness = PaintScrollTextAreaSubtreeWitness::new(
        outer,
        text_area_root,
        *live_text_area_clip,
        local_scissor,
        admission.paint_grammar,
    )
    .ok_or_else(|| invalid(text_area_root))?;
    let outer_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(outer.contents_clip_snapshot().id),
        scroll: Some(outer.scroll_snapshot().id),
        ..Default::default()
    };
    let text_area_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(local_clip_id),
        scroll: Some(outer.scroll_snapshot().id),
        ..Default::default()
    };
    if property_trees
        .states
        .get(&content_root)
        .is_none_or(|state| state.paint != outer_state || state.descendants != outer_state)
        || property_trees
            .states
            .get(&text_area_root)
            .is_none_or(|state| state.paint != outer_state || state.descendants != text_area_state)
    {
        return Err(invalid(text_area_root));
    }

    let mut stack = vec![(content_root, admission.boundary_root)];
    let mut seen = FxHashSet::default();
    while let Some((key, expected_parent)) = stack.pop() {
        if !seen.insert(key) {
            return Err(vec![FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::DuplicateNodeKey(key),
            )]);
        }
        let node = arena.get(key).ok_or_else(|| invalid(key))?;
        if arena.parent_of(key) != Some(expected_parent)
            || node.element.is_deferred_to_root_viewport_render()
            || node.element.has_active_animator()
        {
            return Err(invalid(key));
        }
        let expected = if key == content_root {
            (outer_state, outer_state)
        } else if key == text_area_root {
            (outer_state, text_area_state)
        } else {
            let generated = node
                .element
                .as_any()
                .is::<crate::view::base_component::text_area::TextAreaTextRun>()
                || node
                    .element
                    .as_any()
                    .is::<crate::view::base_component::text_area::TextAreaLineBreak>();
            if !generated || !node.element.children().is_empty() {
                return Err(invalid(key));
            }
            (text_area_state, text_area_state)
        };
        if property_trees
            .states
            .get(&key)
            .is_none_or(|state| (state.paint, state.descendants) != expected)
        {
            return Err(invalid(key));
        }
        stack.extend(
            node.element
                .children()
                .iter()
                .copied()
                .map(|child| (child, key)),
        );
    }

    let mut artifact = match record_frame_artifact_with_policy(
        arena,
        &[content_root],
        promoted_node_ids,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        FrameArtifactAuthorityPolicy::ScrollTextAreaSubtreeLocal(witness),
        None,
        Some(required_paint_offset.map(f32::to_bits)),
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    // Generic paint generations include TextArea promotion signatures, whose
    // screen-space layout origin changes when only the outer scroll moves.
    // C1's detached raster identity instead freezes one local topology token;
    // chunk payload/bounds/clip identities continue to own all visible text,
    // style and internal-scroll changes.
    let mut topology_revision = 0xcbf2_9ce4_8422_2325_u64;
    let mut topology_stack = vec![content_root];
    while let Some(owner) = topology_stack.pop() {
        let node = arena.get(owner).ok_or_else(|| invalid(owner))?;
        for value in [
            node.element.stable_id(),
            node.element.children().len() as u64,
            if owner == content_root {
                1
            } else if owner == text_area_root {
                2
            } else if node
                .element
                .as_any()
                .is::<crate::view::base_component::text_area::TextAreaTextRun>()
            {
                3
            } else {
                4
            },
        ] {
            topology_revision ^= value;
            topology_revision = topology_revision.wrapping_mul(0x100_0000_01b3);
        }
        topology_stack.extend(node.element.children().iter().rev().copied());
    }
    let normalized_revision = super::PaintContentRevision {
        self_paint_revision: 0,
        composite_revision: 0,
        topology_revision,
    };
    for chunk in &mut artifact.chunks {
        if chunk.owner == content_root || chunk.owner == text_area_root {
            chunk.content_revision = normalized_revision;
        }
    }
    if matches!(
        admission.paint_grammar,
        crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs { .. }
    ) {
        let Some(selection) = artifact.chunks.get(1) else {
            return Err(invalid(text_area_root));
        };
        let selection_range = selection.op_range.clone();
        let rects = artifact.ops[selection_range]
            .iter()
            .map(|op| match op {
                super::PaintOp::DrawRect(rect) => Some(rect),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| invalid(text_area_root))?;
        let generic_identity = super::PaintPayloadIdentity::prepared_rects(rects.iter().copied())
            .ok_or_else(|| invalid(text_area_root))?;
        if selection.payload_identity != generic_identity {
            return Err(invalid(text_area_root));
        }
        let sealed_identity = super::PaintPayloadIdentity::prepared_text_area_selection(
            admission.paint_grammar,
            rects.iter().copied(),
        )
        .ok_or_else(|| invalid(text_area_root))?;
        artifact.chunks[1].payload_identity = sealed_identity;
    }
    let local_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(local_clip_id),
        ..Default::default()
    };
    let wrapper_matches = |chunk: &super::PaintChunk| {
        chunk.owner == content_root
            && chunk.id.owner == content_root
            && chunk.id.scope == super::PaintPropertyScope::SelfPaint
            && chunk.id.phase == super::PaintNodePhase::BeforeChildren
            && chunk.id.slot == 0
            && chunk.id.role == super::PaintChunkRole::SelfDecoration
            && chunk.properties == Default::default()
    };
    let glyph_matches = |chunk: &super::PaintChunk| {
        let [super::PaintOp::PreparedText(prepared)] = &artifact.ops[chunk.op_range.clone()] else {
            return false;
        };
        chunk.owner == text_area_root
            && chunk.id.owner == text_area_root
            && chunk.id.scope == super::PaintPropertyScope::Contents
            && chunk.id.phase == super::PaintNodePhase::BeforeChildren
            && chunk.id.slot == 1
            && chunk.id.role == super::PaintChunkRole::TextGlyphs
            && chunk.properties == local_state
            && prepared.has_canonical_identity()
            && chunk.payload_identity == super::PaintPayloadIdentity::prepared_texts([prepared])
    };
    let selection_matches = |chunk: &super::PaintChunk| {
        let crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
            color_rgba_bits,
            ..
        } = admission.paint_grammar
        else {
            return false;
        };
        let ops = &artifact.ops[chunk.op_range.clone()];
        let rects = ops
            .iter()
            .map(|op| match op {
                super::PaintOp::DrawRect(rect)
                    if rect.mode
                        == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly =>
                {
                    Some(rect)
                }
                _ => None,
            })
            .collect::<Option<Vec<_>>>();
        let Some(rects) = rects.filter(|rects| !rects.is_empty()) else {
            return false;
        };
        let mut left = f32::INFINITY;
        let mut top = f32::INFINITY;
        let mut right = f32::NEG_INFINITY;
        let mut bottom = f32::NEG_INFINITY;
        for rect in &rects {
            left = left.min(rect.params.position[0]);
            top = top.min(rect.params.position[1]);
            right = right.max(rect.params.position[0] + rect.params.size[0]);
            bottom = bottom.max(rect.params.position[1] + rect.params.size[1]);
        }
        chunk.owner == text_area_root
            && chunk.id.owner == text_area_root
            && chunk.id.scope == super::PaintPropertyScope::Contents
            && chunk.id.phase == super::PaintNodePhase::BeforeChildren
            && chunk.id.slot == 0
            && chunk.id.role == super::PaintChunkRole::SelectionUnderlay
            && chunk.properties == local_state
            && rects.iter().all(|rect| {
                rect.params.fill_color.map(f32::to_bits) == color_rgba_bits
                    && rect.params.opacity.to_bits() == 1.0_f32.to_bits()
            })
            && [
                chunk.bounds.x,
                chunk.bounds.y,
                chunk.bounds.width,
                chunk.bounds.height,
            ]
            .map(f32::to_bits)
                == [left, top, right - left, bottom - top].map(f32::to_bits)
            && super::PaintPayloadIdentity::prepared_text_area_selection(
                admission.paint_grammar,
                rects.into_iter(),
            )
            .as_ref()
                == Some(&chunk.payload_identity)
    };
    let chunks_match_grammar = match admission.paint_grammar {
        crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly => {
            matches!(
                artifact.chunks.as_slice(),
                [wrapper, glyph] if wrapper_matches(wrapper) && glyph_matches(glyph)
            )
        }
        crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
            ..
        } => {
            matches!(
                artifact.chunks.as_slice(),
                [wrapper, selection, glyph]
                    if wrapper_matches(wrapper)
                        && selection_matches(selection)
                        && glyph_matches(glyph)
            )
        }
    };
    if !matches!(artifact.target, PaintArtifactTarget::CurrentTarget)
        || artifact.effect_nodes.len() != 0
        || artifact.clip_nodes.as_slice() != [witness.local_contents_clip()]
        || artifact.owner_nodes.as_slice()
            != [
                super::PaintOwnerSnapshot {
                    owner: content_root,
                    parent: None,
                },
                super::PaintOwnerSnapshot {
                    owner: text_area_root,
                    parent: Some(content_root),
                },
            ]
        || !chunks_match_grammar
    {
        return Err(invalid(content_root));
    }
    Ok(artifact)
}

/// C3a graph-inert recorder for the exact atomic-projection sibling.  Source
/// authority remains the live TextArea oracle and is checked on both sides of
/// the metadata/full pass; the Copy paint witness authorizes properties only.
#[allow(clippy::too_many_arguments)]
fn record_atomic_projection_live_raster_oracle(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    policy: FrameArtifactAuthorityPolicy,
    context: PaintRecordingContext,
    content_root: NodeKey,
    text_area_root: NodeKey,
    source_grammar: &crate::view::base_component::text_area::RetainedAtomicProjectionTextAreaPaintGrammar,
    owner_nodes: Vec<super::PaintOwnerSnapshot>,
) -> Result<RetainedAtomicProjectionTextAreaLiveRasterOracle, Vec<FrameArtifactFallbackReason>> {
    let manifest = record_coverage_manifest_with_context(
        arena,
        roots,
        promoted_node_ids,
        None,
        false,
        true,
        CoverageRecordingMode::MetadataOnly,
        property_trees,
        paint_generations,
        context,
        None,
        &super::PlannedBoundaryCutoutSet::default(),
    );
    let eligibility = assess_manifest(&manifest, policy);
    if !eligibility.eligible {
        return Err(eligibility.reasons);
    }
    let mut chunks = Vec::new();
    let mut clips = Vec::new();
    let mut seen_clips = FxHashMap::default();
    for item in manifest.items {
        if let PaintCoverageItem::ArtifactChunk {
            chunk,
            clip_snapshot,
            ops: None,
            ..
        } = item
        {
            chunks.push(RetainedAtomicProjectionChunkLiveRasterOracle {
                id: chunk.id,
                owner: chunk.owner,
                bounds_bits: [
                    chunk.bounds.x,
                    chunk.bounds.y,
                    chunk.bounds.width,
                    chunk.bounds.height,
                ]
                .map(f32::to_bits),
                properties: chunk.properties,
                payload_identity: chunk.payload_identity,
            });
            for snapshot in clip_snapshot {
                match merge_snapshot(&mut seen_clips, snapshot.id, snapshot) {
                    SnapshotMerge::Inserted => clips.push(snapshot),
                    SnapshotMerge::Identical => {}
                    SnapshotMerge::Conflict => {
                        return Err(vec![FrameArtifactFallbackReason::Validation(
                            PaintCoverageValidationError::ConflictingClipSnapshot(snapshot.id),
                        )]);
                    }
                }
            }
        }
    }
    Ok(RetainedAtomicProjectionTextAreaLiveRasterOracle {
        content_root,
        text_area_root,
        source_grammar: source_grammar.clone(),
        chunks,
        clip_nodes: clips,
        owner_nodes,
    })
}

#[allow(clippy::too_many_arguments)]
fn record_atomic_projection_selection_live_raster_oracle(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    policy: FrameArtifactAuthorityPolicy,
    context: PaintRecordingContext,
    content_root: NodeKey,
    text_area_root: NodeKey,
    source_grammar: &crate::view::base_component::text_area::RetainedAtomicProjectionSelectionTextAreaPaintGrammar,
    owner_nodes: Vec<super::PaintOwnerSnapshot>,
) -> Result<
    RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle,
    Vec<FrameArtifactFallbackReason>,
> {
    let manifest = record_coverage_manifest_with_context(
        arena,
        roots,
        promoted_node_ids,
        None,
        false,
        true,
        CoverageRecordingMode::MetadataOnly,
        property_trees,
        paint_generations,
        context,
        None,
        &super::PlannedBoundaryCutoutSet::default(),
    );
    let eligibility = assess_manifest(&manifest, policy);
    if !eligibility.eligible {
        return Err(eligibility.reasons);
    }
    let mut chunks = Vec::new();
    let mut clips = Vec::new();
    let mut seen_clips = FxHashMap::default();
    for item in manifest.items {
        if let PaintCoverageItem::ArtifactChunk {
            chunk,
            clip_snapshot,
            ops: None,
            ..
        } = item
        {
            chunks.push(RetainedAtomicProjectionChunkLiveRasterOracle {
                id: chunk.id,
                owner: chunk.owner,
                bounds_bits: [
                    chunk.bounds.x,
                    chunk.bounds.y,
                    chunk.bounds.width,
                    chunk.bounds.height,
                ]
                .map(f32::to_bits),
                properties: chunk.properties,
                payload_identity: chunk.payload_identity,
            });
            for snapshot in clip_snapshot {
                match merge_snapshot(&mut seen_clips, snapshot.id, snapshot) {
                    SnapshotMerge::Inserted => clips.push(snapshot),
                    SnapshotMerge::Identical => {}
                    SnapshotMerge::Conflict => {
                        return Err(vec![FrameArtifactFallbackReason::Validation(
                            PaintCoverageValidationError::ConflictingClipSnapshot(snapshot.id),
                        )]);
                    }
                }
            }
        }
    }
    Ok(RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle {
        content_root,
        text_area_root,
        source_grammar: source_grammar.clone(),
        chunks,
        clip_nodes: clips,
        owner_nodes,
    })
}

pub(super) fn record_scroll_atomic_projection_text_area_subtree_local_artifact_for_plan(
    arena: &NodeArena,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    admission: &RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot,
    outer: PaintScrollContentWitness,
) -> Result<RecordedRetainedAtomicProjectionTextAreaSubtree, Vec<FrameArtifactFallbackReason>> {
    use crate::view::base_component::text_area::{
        RetainedAtomicProjectionTextAreaTopologyKind as Kind, TextAreaLineBreak,
        TextAreaProjectionSegment, TextAreaTextRun,
    };
    let content_root = admission.content_wrapper;
    let text_area_root = admission.text_area_root;
    let invalid = |owner| vec![FrameArtifactFallbackReason::PropertyBoundary(owner)];
    if outer.boundary_root() != admission.boundary_root
        || outer.content_root() != content_root
        || outer.scroll_snapshot().owner != admission.boundary_root
        || !admission.matches_scroll_node(outer.scroll_snapshot())
        || !admission.paint_grammar.is_canonical()
        || !promoted_node_ids.is_empty()
        || !property_trees.validation_errors.is_empty()
        || !property_trees.transforms.is_empty()
        || !property_trees.effects.is_empty()
    {
        return Err(invalid(content_root));
    }
    let content_node = arena
        .get(content_root)
        .ok_or_else(|| invalid(content_root))?;
    let content = content_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
        .ok_or_else(|| invalid(content_root))?;
    let required_paint_offset = content
        .exact_retained_scroll_content_wrapper_recording_offset(outer.normalization_paint_offset())
        .ok_or_else(|| invalid(content_root))?;
    let text_area_node = arena
        .get(text_area_root)
        .ok_or_else(|| invalid(text_area_root))?;
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::TextArea>()
        .ok_or_else(|| invalid(text_area_root))?;
    if content_node.element.stable_id() != admission.content_wrapper_stable_id
        || text_area_node.element.stable_id() != admission.text_area_stable_id
        || arena
            .get(admission.boundary_root)
            .is_none_or(|node| node.element.stable_id() != admission.stable_id)
    {
        return Err(invalid(content_root));
    }
    let source_before = text_area
        .exact_retained_property_scroll_atomic_projection_subtree(
            text_area_root,
            arena,
            required_paint_offset,
        )
        .filter(|grammar| grammar == &admission.paint_grammar)
        .ok_or_else(|| invalid(text_area_root))?;
    let local_scissor = text_area
        .retained_property_scroll_local_contents_scissor(outer.normalization_paint_offset())
        .ok_or_else(|| invalid(text_area_root))?;
    let local_clip_id = crate::view::compositor::property_tree::ClipNodeId {
        owner: text_area_root,
        role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
    };
    let live_chain = property_trees
        .clip_snapshot_for(Some(local_clip_id))
        .ok_or_else(|| invalid(text_area_root))?;
    let [live_text_area_clip, live_outer_clip] = live_chain.as_slice() else {
        return Err(invalid(text_area_root));
    };
    if *live_outer_clip != outer.contents_clip_snapshot() {
        return Err(invalid(text_area_root));
    }
    let recorder_authority = AtomicProjectionRecorderWitness::ExistingAtomicGlyph(
        PaintScrollAtomicProjectionTextAreaSubtreeWitness::new(
            outer,
            text_area_root,
            *live_text_area_clip,
            local_scissor,
            &source_before,
        )
        .ok_or_else(|| invalid(text_area_root))?,
    );
    let outer_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(outer.contents_clip_snapshot().id),
        scroll: Some(outer.scroll_snapshot().id),
        ..Default::default()
    };
    let text_area_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(local_clip_id),
        scroll: Some(outer.scroll_snapshot().id),
        ..Default::default()
    };
    if arena.parent_of(content_root) != Some(admission.boundary_root)
        || arena.children_of(content_root) != [text_area_root]
        || property_trees
            .states
            .get(&content_root)
            .is_none_or(|state| (state.paint, state.descendants) != (outer_state, outer_state))
        || property_trees
            .states
            .get(&text_area_root)
            .is_none_or(|state| (state.paint, state.descendants) != (outer_state, text_area_state))
    {
        return Err(invalid(content_root));
    }
    let mut expected_owners = vec![
        super::PaintOwnerSnapshot {
            owner: content_root,
            parent: None,
        },
        super::PaintOwnerSnapshot {
            owner: text_area_root,
            parent: Some(content_root),
        },
    ];
    if arena.children_of(text_area_root).len() != source_before.topology.len() {
        return Err(invalid(text_area_root));
    }
    for seal in source_before.topology.iter() {
        let node = arena.get(seal.owner).ok_or_else(|| invalid(seal.owner))?;
        if arena.parent_of(seal.owner) != Some(text_area_root)
            || node.element.stable_id() != seal.stable_id
            || node.element.is_deferred_to_root_viewport_render()
            || node.element.has_active_animator()
            || property_trees.states.get(&seal.owner).is_none_or(|state| {
                (state.paint, state.descendants) != (text_area_state, text_area_state)
            })
        {
            return Err(invalid(seal.owner));
        }
        let type_and_children_are_exact = match seal.kind {
            Kind::TextRun => {
                node.element.as_any().is::<TextAreaTextRun>() && node.element.children().is_empty()
            }
            Kind::LineBreak => {
                node.element.as_any().is::<TextAreaLineBreak>()
                    && node.element.children().is_empty()
            }
            Kind::ProjectionSegment => {
                let [text] = node.element.children() else {
                    return Err(invalid(seal.owner));
                };
                let text = *text;
                let text_node = arena.get(text).ok_or_else(|| invalid(text))?;
                if !node.element.as_any().is::<TextAreaProjectionSegment>()
                    || text != source_before.projection_text_owner
                    || arena.parent_of(text) != Some(seal.owner)
                    || text_node.element.stable_id() != source_before.projection_text_stable_id
                    || !text_node
                        .element
                        .as_any()
                        .is::<crate::view::base_component::Text>()
                    || !text_node.element.children().is_empty()
                    || property_trees.states.get(&text).is_none_or(|state| {
                        (state.paint, state.descendants) != (text_area_state, text_area_state)
                    })
                {
                    return Err(invalid(text));
                }
                expected_owners.push(super::PaintOwnerSnapshot {
                    owner: seal.owner,
                    parent: Some(text_area_root),
                });
                expected_owners.push(super::PaintOwnerSnapshot {
                    owner: text,
                    parent: Some(seal.owner),
                });
                continue;
            }
        };
        if !type_and_children_are_exact {
            return Err(invalid(seal.owner));
        }
        expected_owners.push(super::PaintOwnerSnapshot {
            owner: seal.owner,
            parent: Some(text_area_root),
        });
    }

    let policy = FrameArtifactAuthorityPolicy::ScrollAtomicProjectionTextAreaSubtreeLocal(
        recorder_authority,
    );
    let oracle_context = PaintRecordingContext {
        paint_offset: recorder_authority.outer().normalization_paint_offset(),
        required_scroll_content_paint_offset_bits: Some(required_paint_offset.map(f32::to_bits)),
        scroll_atomic_projection_text_area_subtree: Some(recorder_authority),
        opacity_authority: PaintOpacityAuthority::Baked,
        ..PaintRecordingContext::default()
    };
    let raster_before = record_atomic_projection_live_raster_oracle(
        arena,
        &[content_root],
        promoted_node_ids,
        property_trees,
        paint_generations,
        policy,
        oracle_context,
        content_root,
        text_area_root,
        &source_before,
        expected_owners.clone(),
    )?;

    let mut artifact = match record_frame_artifact_with_policy(
        arena,
        &[content_root],
        promoted_node_ids,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        policy,
        None,
        Some(required_paint_offset.map(f32::to_bits)),
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    let source_after = text_area
        .exact_retained_property_scroll_atomic_projection_subtree(
            text_area_root,
            arena,
            required_paint_offset,
        )
        .ok_or_else(|| {
            vec![FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::RecordingPassMismatch,
            )]
        })?;
    if source_before != source_after || source_after != admission.paint_grammar {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    artifact.owner_nodes = expected_owners.clone();
    let raster_after = record_atomic_projection_live_raster_oracle(
        arena,
        &[content_root],
        promoted_node_ids,
        property_trees,
        paint_generations,
        policy,
        oracle_context,
        content_root,
        text_area_root,
        &source_after,
        expected_owners.clone(),
    )?;
    if raster_before != raster_after || !raster_before.matches_artifact(&artifact) {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    let local_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(local_clip_id),
        ..Default::default()
    };
    let [wrapper, root_glyph, projection_glyph] = artifact.chunks.as_slice() else {
        return Err(invalid(content_root));
    };
    let glyph_exact = |chunk: &super::PaintChunk, owner, scope| {
        matches!(&artifact.ops[chunk.op_range.clone()], [super::PaintOp::PreparedText(prepared)]
            if prepared.has_canonical_identity()
                && chunk.payload_identity == super::PaintPayloadIdentity::prepared_texts([prepared]))
            && chunk.owner == owner
            && chunk.id.owner == owner
            && chunk.id.scope == scope
            && chunk.id.phase == super::PaintNodePhase::BeforeChildren
            && chunk.id.slot == 1
            && chunk.id.role == super::PaintChunkRole::TextGlyphs
            && chunk.properties == local_state
    };
    if !matches!(artifact.target, PaintArtifactTarget::CurrentTarget)
        || !artifact.effect_nodes.is_empty()
        || artifact.clip_nodes.as_slice() != [recorder_authority.local_contents_clip()]
        || artifact.owner_nodes != expected_owners
        || wrapper.owner != content_root
        || wrapper.id.owner != content_root
        || wrapper.id.scope != super::PaintPropertyScope::SelfPaint
        || wrapper.id.phase != super::PaintNodePhase::BeforeChildren
        || wrapper.id.slot != 0
        || wrapper.id.role != super::PaintChunkRole::SelfDecoration
        || wrapper.properties != Default::default()
        || !glyph_exact(
            root_glyph,
            text_area_root,
            super::PaintPropertyScope::Contents,
        )
        || !glyph_exact(
            projection_glyph,
            source_before.projection_text_owner,
            super::PaintPropertyScope::SelfPaint,
        )
    {
        return Err(invalid(content_root));
    }
    Ok(RecordedRetainedAtomicProjectionTextAreaSubtree {
        artifact,
        raster_oracle: raster_before,
    })
}

pub(super) fn record_scroll_atomic_projection_selection_text_area_subtree_local_artifact_for_plan(
    arena: &NodeArena,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    admission: &RetainedScrollAtomicProjectionSelectionTextAreaSubtreeAdmissionSnapshot,
    outer: PaintScrollContentWitness,
) -> Result<
    RecordedRetainedAtomicProjectionSelectionTextAreaSubtree,
    Vec<FrameArtifactFallbackReason>,
> {
    let content_root = admission.content_wrapper;
    let text_area_root = admission.text_area_root;
    let invalid = |owner| vec![FrameArtifactFallbackReason::PropertyBoundary(owner)];
    if outer.boundary_root() != admission.boundary_root
        || outer.content_root() != content_root
        || outer.scroll_snapshot().owner != admission.boundary_root
        || !admission.matches_scroll_node(outer.scroll_snapshot())
        || !promoted_node_ids.is_empty()
        || !property_trees.validation_errors.is_empty()
        || !property_trees.transforms.is_empty()
        || !property_trees.effects.is_empty()
    {
        return Err(invalid(content_root));
    }
    let content_node = arena
        .get(content_root)
        .ok_or_else(|| invalid(content_root))?;
    let content = content_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
        .ok_or_else(|| invalid(content_root))?;
    let required_paint_offset = content
        .exact_retained_scroll_content_wrapper_recording_offset(outer.normalization_paint_offset())
        .ok_or_else(|| invalid(content_root))?;
    let text_area_node = arena
        .get(text_area_root)
        .ok_or_else(|| invalid(text_area_root))?;
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::TextArea>()
        .ok_or_else(|| invalid(text_area_root))?;
    if content_node.element.stable_id() != admission.content_wrapper_stable_id
        || text_area_node.element.stable_id() != admission.text_area_stable_id
        || arena
            .get(admission.boundary_root)
            .is_none_or(|node| node.element.stable_id() != admission.stable_id)
    {
        return Err(invalid(content_root));
    }
    let source_before = text_area
        .exact_retained_property_scroll_atomic_projection_selection_subtree(
            text_area_root,
            arena,
            required_paint_offset,
        )
        .filter(|grammar| grammar == &admission.paint_grammar)
        .ok_or_else(|| invalid(text_area_root))?;
    let local_scissor = text_area
        .retained_property_scroll_local_contents_scissor(outer.normalization_paint_offset())
        .ok_or_else(|| invalid(text_area_root))?;
    let local_clip_id = crate::view::compositor::property_tree::ClipNodeId {
        owner: text_area_root,
        role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
    };
    let live_chain = property_trees
        .clip_snapshot_for(Some(local_clip_id))
        .ok_or_else(|| invalid(text_area_root))?;
    let [live_text_area_clip, live_outer_clip] = live_chain.as_slice() else {
        return Err(invalid(text_area_root));
    };
    if *live_outer_clip != outer.contents_clip_snapshot() {
        return Err(invalid(text_area_root));
    }
    let recorder_witness = PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness::new(
        outer,
        text_area_root,
        *live_text_area_clip,
        local_scissor,
        &source_before,
    )
    .ok_or_else(|| invalid(text_area_root))?;
    let recorder_authority =
        AtomicProjectionRecorderWitness::AtomicProjectionSelection(recorder_witness);
    let outer_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(outer.contents_clip_snapshot().id),
        scroll: Some(outer.scroll_snapshot().id),
        ..Default::default()
    };
    let text_area_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(local_clip_id),
        scroll: Some(outer.scroll_snapshot().id),
        ..Default::default()
    };
    if arena.parent_of(content_root) != Some(admission.boundary_root)
        || arena.children_of(content_root) != [text_area_root]
        || property_trees
            .states
            .get(&content_root)
            .is_none_or(|state| (state.paint, state.descendants) != (outer_state, outer_state))
        || property_trees
            .states
            .get(&text_area_root)
            .is_none_or(|state| (state.paint, state.descendants) != (outer_state, text_area_state))
    {
        return Err(invalid(content_root));
    }
    let mut expected_owners = vec![
        super::PaintOwnerSnapshot {
            owner: content_root,
            parent: None,
        },
        super::PaintOwnerSnapshot {
            owner: text_area_root,
            parent: Some(content_root),
        },
    ];
    if arena.children_of(text_area_root).len() != source_before.atomic_source.topology.len() {
        return Err(invalid(text_area_root));
    }
    for seal in source_before.atomic_source.topology.iter() {
        let node = arena.get(seal.owner).ok_or_else(|| invalid(seal.owner))?;
        if arena.parent_of(seal.owner) != Some(text_area_root)
            || node.element.stable_id() != seal.stable_id
            || node.element.is_deferred_to_root_viewport_render()
            || node.element.has_active_animator()
            || property_trees.states.get(&seal.owner).is_none_or(|state| {
                (state.paint, state.descendants) != (text_area_state, text_area_state)
            })
        {
            return Err(invalid(seal.owner));
        }
        expected_owners.push(super::PaintOwnerSnapshot {
            owner: seal.owner,
            parent: Some(text_area_root),
        });
        if seal.owner == source_before.atomic_source.projection_owner {
            let text = source_before.atomic_source.projection_text_owner;
            let text_node = arena.get(text).ok_or_else(|| invalid(text))?;
            if arena.parent_of(text) != Some(seal.owner)
                || node.element.children() != [text]
                || text_node.element.stable_id()
                    != source_before.atomic_source.projection_text_stable_id
                || !text_node
                    .element
                    .as_any()
                    .is::<crate::view::base_component::Text>()
                || !text_node.element.children().is_empty()
                || property_trees.states.get(&text).is_none_or(|state| {
                    (state.paint, state.descendants) != (text_area_state, text_area_state)
                })
            {
                return Err(invalid(text));
            }
            expected_owners.push(super::PaintOwnerSnapshot {
                owner: text,
                parent: Some(seal.owner),
            });
        }
    }
    let policy = FrameArtifactAuthorityPolicy::ScrollAtomicProjectionTextAreaSubtreeLocal(
        recorder_authority,
    );
    let oracle_context = PaintRecordingContext {
        paint_offset: recorder_authority.outer().normalization_paint_offset(),
        required_scroll_content_paint_offset_bits: Some(required_paint_offset.map(f32::to_bits)),
        scroll_atomic_projection_text_area_subtree: Some(recorder_authority),
        opacity_authority: PaintOpacityAuthority::Baked,
        ..PaintRecordingContext::default()
    };
    let mut raster_before = record_atomic_projection_selection_live_raster_oracle(
        arena,
        &[content_root],
        promoted_node_ids,
        property_trees,
        paint_generations,
        policy,
        oracle_context,
        content_root,
        text_area_root,
        &source_before,
        expected_owners.clone(),
    )?;
    let mut artifact = match record_frame_artifact_with_policy(
        arena,
        &[content_root],
        promoted_node_ids,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        policy,
        None,
        Some(required_paint_offset.map(f32::to_bits)),
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    let source_after = text_area
        .exact_retained_property_scroll_atomic_projection_selection_subtree(
            text_area_root,
            arena,
            required_paint_offset,
        )
        .ok_or_else(|| {
            vec![FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::RecordingPassMismatch,
            )]
        })?;
    if source_before != source_after || source_after != admission.paint_grammar {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    artifact.owner_nodes = expected_owners.clone();
    let selection_seal = normalize_atomic_projection_selection_chunk(
        &mut artifact,
        &mut raster_before,
        1,
        recorder_witness.selection,
    )
    .ok_or_else(|| invalid(text_area_root))?;
    let mut raster_after = record_atomic_projection_selection_live_raster_oracle(
        arena,
        &[content_root],
        promoted_node_ids,
        property_trees,
        paint_generations,
        policy,
        oracle_context,
        content_root,
        text_area_root,
        &source_after,
        expected_owners.clone(),
    )?;
    let after_selection_seal = normalize_atomic_projection_selection_chunk(
        &mut artifact,
        &mut raster_after,
        1,
        recorder_witness.selection,
    )
    .ok_or_else(|| invalid(text_area_root))?;
    if selection_seal != after_selection_seal
        || raster_before != raster_after
        || !raster_before.matches_artifact(&artifact)
    {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    let local_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(local_clip_id),
        ..Default::default()
    };
    let [wrapper, selection, root_glyph, projection_glyph] = artifact.chunks.as_slice() else {
        return Err(invalid(content_root));
    };
    let glyph_exact = |chunk: &super::PaintChunk, owner, scope| {
        matches!(&artifact.ops[chunk.op_range.clone()], [super::PaintOp::PreparedText(prepared)]
            if prepared.has_canonical_identity()
                && chunk.payload_identity == super::PaintPayloadIdentity::prepared_texts([prepared]))
            && chunk.owner == owner
            && chunk.id.owner == owner
            && chunk.id.scope == scope
            && chunk.id.phase == super::PaintNodePhase::BeforeChildren
            && chunk.id.slot == 1
            && chunk.id.role == super::PaintChunkRole::TextGlyphs
            && chunk.properties == local_state
    };
    let selection_exact = matches!(&artifact.ops[selection.op_range.clone()], ops
    if !ops.is_empty()
        && ops.iter().all(|op| matches!(op, super::PaintOp::DrawRect(_)))
        && selection.payload_identity.retained_text_area_selection_grammar() == Some(recorder_witness.selection)
        && selection.payload_identity.matches_exact_text_area_selection_ops(
            ops.iter().filter_map(|op| match op { super::PaintOp::DrawRect(rect) => Some(rect), _ => None })
        ));
    if !matches!(artifact.target, PaintArtifactTarget::CurrentTarget)
        || !artifact.effect_nodes.is_empty()
        || artifact.clip_nodes.as_slice() != [recorder_authority.local_contents_clip()]
        || artifact.owner_nodes != expected_owners
        || wrapper.owner != content_root
        || wrapper.id.role != super::PaintChunkRole::SelfDecoration
        || wrapper.properties != Default::default()
        || selection.owner != text_area_root
        || selection.id.scope != super::PaintPropertyScope::Contents
        || selection.id.phase != super::PaintNodePhase::BeforeChildren
        || selection.id.slot != 0
        || selection.id.role != super::PaintChunkRole::SelectionUnderlay
        || selection.properties != local_state
        || !selection_exact
        || !glyph_exact(
            root_glyph,
            text_area_root,
            super::PaintPropertyScope::Contents,
        )
        || !glyph_exact(
            projection_glyph,
            source_before.atomic_source.projection_text_owner,
            super::PaintPropertyScope::SelfPaint,
        )
    {
        return Err(invalid(content_root));
    }
    Ok(RecordedRetainedAtomicProjectionSelectionTextAreaSubtree {
        artifact,
        raster_oracle: raster_before,
    })
}

#[derive(Clone, Debug)]
pub(super) struct RecordedRetainedInteractiveTextAreaSubtree {
    pub(super) artifact: PaintArtifact,
    pub(super) preedit_seal: Option<RetainedTextAreaPreeditRasterSeal>,
    pub(super) caret_overlay: RecordedRetainedTextAreaCaretOverlay,
}

pub(super) fn record_scroll_interactive_text_area_subtree_local_artifact_for_plan(
    arena: &NodeArena,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    admission: RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot,
    outer: PaintScrollContentWitness,
) -> Result<RecordedRetainedInteractiveTextAreaSubtree, Vec<FrameArtifactFallbackReason>> {
    let content_root = admission.content_wrapper;
    let text_area_root = admission.text_area_root;
    let invalid = |owner| vec![FrameArtifactFallbackReason::PropertyBoundary(owner)];
    if outer.boundary_root() != admission.boundary_root
        || outer.content_root() != content_root
        || outer.scroll_snapshot().owner != admission.boundary_root
        || !admission.matches_scroll_node(outer.scroll_snapshot())
        || !promoted_node_ids.is_empty()
        || !property_trees.validation_errors.is_empty()
        || !property_trees.transforms.is_empty()
        || !property_trees.effects.is_empty()
    {
        return Err(invalid(content_root));
    }
    let content_node = arena
        .get(content_root)
        .ok_or_else(|| invalid(content_root))?;
    let content = content_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
        .ok_or_else(|| invalid(content_root))?;
    let detached_paint_offset = content
        .exact_retained_scroll_content_wrapper_recording_offset(outer.normalization_paint_offset())
        .ok_or_else(|| invalid(content_root))?;
    let live_paint_offset = content
        .exact_retained_scroll_content_wrapper_recording_offset([0.0, 0.0])
        .ok_or_else(|| invalid(content_root))?;
    let text_area_node = arena
        .get(text_area_root)
        .ok_or_else(|| invalid(text_area_root))?;
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::TextArea>()
        .ok_or_else(|| invalid(text_area_root))?;
    if !text_area_matches_admitted_interactive_paint_grammar(
        text_area,
        text_area_root,
        arena,
        detached_paint_offset,
        admission.paint_grammar,
    ) {
        return Err(invalid(text_area_root));
    }
    let local_clip_id = crate::view::compositor::property_tree::ClipNodeId {
        owner: text_area_root,
        role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
    };
    let live_chain = property_trees
        .clip_snapshot_for(Some(local_clip_id))
        .ok_or_else(|| invalid(text_area_root))?;
    let [live_text_area_clip, live_outer_clip] = live_chain.as_slice() else {
        return Err(invalid(text_area_root));
    };
    if *live_outer_clip != outer.contents_clip_snapshot() {
        return Err(invalid(text_area_root));
    }
    let local_scissor = text_area
        .retained_property_scroll_local_contents_scissor(outer.normalization_paint_offset())
        .ok_or_else(|| invalid(text_area_root))?;
    let witness = PaintScrollInteractiveTextAreaSubtreeWitness::new(
        outer,
        text_area_root,
        *live_text_area_clip,
        local_scissor,
        admission.paint_grammar,
    )
    .ok_or_else(|| invalid(text_area_root))?;
    let outer_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(outer.contents_clip_snapshot().id),
        scroll: Some(outer.scroll_snapshot().id),
        ..Default::default()
    };
    let text_area_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(local_clip_id),
        scroll: Some(outer.scroll_snapshot().id),
        ..Default::default()
    };
    if property_trees
        .states
        .get(&content_root)
        .is_none_or(|state| state.paint != outer_state || state.descendants != outer_state)
        || property_trees
            .states
            .get(&text_area_root)
            .is_none_or(|state| state.paint != outer_state || state.descendants != text_area_state)
    {
        return Err(invalid(text_area_root));
    }
    let mut stack = vec![(content_root, admission.boundary_root)];
    let mut seen = FxHashSet::default();
    while let Some((key, expected_parent)) = stack.pop() {
        if !seen.insert(key) {
            return Err(vec![FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::DuplicateNodeKey(key),
            )]);
        }
        let node = arena.get(key).ok_or_else(|| invalid(key))?;
        if arena.parent_of(key) != Some(expected_parent)
            || node.element.is_deferred_to_root_viewport_render()
            || node.element.has_active_animator()
        {
            return Err(invalid(key));
        }
        let expected = if key == content_root {
            (outer_state, outer_state)
        } else if key == text_area_root {
            (outer_state, text_area_state)
        } else {
            let generated = node
                .element
                .as_any()
                .is::<crate::view::base_component::text_area::TextAreaTextRun>()
                || node
                    .element
                    .as_any()
                    .is::<crate::view::base_component::text_area::TextAreaLineBreak>();
            if !generated || !node.element.children().is_empty() {
                return Err(invalid(key));
            }
            (text_area_state, text_area_state)
        };
        if property_trees
            .states
            .get(&key)
            .is_none_or(|state| (state.paint, state.descendants) != expected)
        {
            return Err(invalid(key));
        }
        stack.extend(
            node.element
                .children()
                .iter()
                .copied()
                .map(|child| (child, key)),
        );
    }
    let caret_before = text_area
        .retained_interactive_caret_overlay(
            text_area_root,
            arena,
            detached_paint_offset,
            live_paint_offset,
            *live_text_area_clip,
            *live_outer_clip,
            admission.paint_grammar,
            admission.caret_oracle_bounds_bits,
        )
        .ok_or_else(|| invalid(text_area_root))?;
    let preedit_before = if admission.paint_grammar.has_preedit() {
        Some(
            text_area
                .retained_interactive_preedit_raster_seal(
                    text_area_root,
                    arena,
                    detached_paint_offset,
                )
                .ok_or_else(|| invalid(text_area_root))?,
        )
    } else {
        None
    };
    let mut artifact = match record_frame_artifact_with_policy(
        arena,
        &[content_root],
        promoted_node_ids,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        FrameArtifactAuthorityPolicy::ScrollInteractiveTextAreaSubtreeLocal(witness),
        None,
        Some(detached_paint_offset.map(f32::to_bits)),
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    let caret_after = text_area
        .retained_interactive_caret_overlay(
            text_area_root,
            arena,
            detached_paint_offset,
            live_paint_offset,
            *live_text_area_clip,
            *live_outer_clip,
            admission.paint_grammar,
            admission.caret_oracle_bounds_bits,
        )
        .ok_or_else(|| invalid(text_area_root))?;
    let preedit_after = if admission.paint_grammar.has_preedit() {
        Some(
            text_area
                .retained_interactive_preedit_raster_seal(
                    text_area_root,
                    arena,
                    detached_paint_offset,
                )
                .ok_or_else(|| invalid(text_area_root))?,
        )
    } else {
        None
    };
    if caret_before.identity != caret_after.identity
        || caret_before
            .op
            .as_ref()
            .and_then(|op| super::PaintPayloadIdentity::prepared_rects([op]))
            != caret_after
                .op
                .as_ref()
                .and_then(|op| super::PaintPayloadIdentity::prepared_rects([op]))
        || preedit_before != preedit_after
    {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    let mut topology_revision = 0xcbf2_9ce4_8422_2325_u64;
    let mut topology_stack = vec![content_root];
    while let Some(owner) = topology_stack.pop() {
        let node = arena.get(owner).ok_or_else(|| invalid(owner))?;
        for value in [
            node.element.stable_id(),
            node.element.children().len() as u64,
            if owner == content_root {
                1
            } else if owner == text_area_root {
                2
            } else if node
                .element
                .as_any()
                .is::<crate::view::base_component::text_area::TextAreaTextRun>()
            {
                3
            } else {
                4
            },
        ] {
            topology_revision ^= value;
            topology_revision = topology_revision.wrapping_mul(0x100_0000_01b3);
        }
        topology_stack.extend(node.element.children().iter().rev().copied());
    }
    let normalized_revision = super::PaintContentRevision {
        self_paint_revision: 0,
        composite_revision: 0,
        topology_revision,
    };
    for chunk in &mut artifact.chunks {
        if chunk.owner == content_root || chunk.owner == text_area_root {
            chunk.content_revision = normalized_revision;
        }
    }
    let local_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(local_clip_id),
        ..Default::default()
    };
    let wrapper_matches = |chunk: &super::PaintChunk| {
        chunk.owner == content_root
            && chunk.id.owner == content_root
            && chunk.id.scope == super::PaintPropertyScope::SelfPaint
            && chunk.id.phase == super::PaintNodePhase::BeforeChildren
            && chunk.id.slot == 0
            && chunk.id.role == super::PaintChunkRole::SelfDecoration
            && chunk.properties == Default::default()
    };
    let glyph_matches = |chunk: &super::PaintChunk| {
        let [super::PaintOp::PreparedText(prepared)] = &artifact.ops[chunk.op_range.clone()] else {
            return false;
        };
        chunk.owner == text_area_root
            && chunk.id.owner == text_area_root
            && chunk.id.scope == super::PaintPropertyScope::Contents
            && chunk.id.phase == super::PaintNodePhase::BeforeChildren
            && chunk.id.slot == 1
            && chunk.id.role == super::PaintChunkRole::TextGlyphs
            && chunk.properties == local_state
            && prepared.has_canonical_identity()
            && chunk.payload_identity == super::PaintPayloadIdentity::prepared_texts([prepared])
    };
    let rect_chunk_matches = |chunk: &super::PaintChunk,
                              phase: super::PaintNodePhase,
                              slot: u16,
                              role: super::PaintChunkRole| {
        let rects = artifact.ops[chunk.op_range.clone()]
            .iter()
            .map(|op| match op {
                super::PaintOp::DrawRect(rect)
                    if rect.mode
                        == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly =>
                {
                    Some(rect)
                }
                _ => None,
            })
            .collect::<Option<Vec<_>>>();
        let Some(rects) = rects.filter(|rects| !rects.is_empty()) else {
            return false;
        };
        chunk.owner == text_area_root
            && chunk.id.owner == text_area_root
            && chunk.id.scope == super::PaintPropertyScope::Contents
            && chunk.id.phase == phase
            && chunk.id.slot == slot
            && chunk.id.role == role
            && chunk.properties == local_state
            && super::PaintPayloadIdentity::prepared_rects(rects.into_iter()).as_ref()
                == Some(&chunk.payload_identity)
    };
    let (glyph_index, underline_index) = match admission.paint_grammar {
        crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedGlyphs => {
            if !matches!(artifact.chunks.as_slice(), [wrapper, glyph]
                if wrapper_matches(wrapper) && glyph_matches(glyph))
            {
                return Err(invalid(content_root));
            }
            (1, None)
        }
        crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedSelectionGlyphs {
            start_char,
            end_char,
            color_rgba_bits,
        } => {
            let [wrapper, selection, glyph] = artifact.chunks.as_mut_slice() else {
                return Err(invalid(content_root));
            };
            if !wrapper_matches(wrapper)
                || !rect_chunk_matches(
                    selection,
                    super::PaintNodePhase::BeforeChildren,
                    0,
                    super::PaintChunkRole::SelectionUnderlay,
                )
                || !glyph_matches(glyph)
            {
                return Err(invalid(content_root));
            }
            let rects = artifact.ops[selection.op_range.clone()]
                .iter()
                .map(|op| match op {
                    super::PaintOp::DrawRect(rect) => Some(rect),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| invalid(text_area_root))?;
            let selection_grammar =
                crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
                    start_char,
                    end_char,
                    color_rgba_bits,
                };
            selection.payload_identity = super::PaintPayloadIdentity::prepared_text_area_selection(
                selection_grammar,
                rects.into_iter(),
            )
            .ok_or_else(|| invalid(text_area_root))?;
            (2, None)
        }
        crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedPreeditGlyphs => {
            if !matches!(artifact.chunks.as_slice(), [wrapper, glyph, underline]
                if wrapper_matches(wrapper)
                    && glyph_matches(glyph)
                    && rect_chunk_matches(
                        underline,
                        super::PaintNodePhase::AfterChildren,
                        0,
                        super::PaintChunkRole::TextDecoration,
                    ))
            {
                return Err(invalid(content_root));
            }
            (1, Some(2))
        }
    };
    let preedit_seal = if let Some(underline_index) = underline_index {
        let seal = preedit_after.ok_or_else(|| invalid(text_area_root))?;
        if seal.glyph_identity != artifact.chunks[glyph_index].payload_identity
            || seal.underline_identity != artifact.chunks[underline_index].payload_identity
        {
            return Err(invalid(text_area_root));
        }
        Some(seal)
    } else {
        None
    };
    if !matches!(artifact.target, PaintArtifactTarget::CurrentTarget)
        || !artifact.effect_nodes.is_empty()
        || artifact.clip_nodes.as_slice() != [witness.local_contents_clip()]
        || artifact.owner_nodes.as_slice()
            != [
                super::PaintOwnerSnapshot {
                    owner: content_root,
                    parent: None,
                },
                super::PaintOwnerSnapshot {
                    owner: text_area_root,
                    parent: Some(content_root),
                },
            ]
    {
        return Err(invalid(content_root));
    }
    Ok(RecordedRetainedInteractiveTextAreaSubtree {
        artifact,
        preedit_seal,
        caret_overlay: caret_after,
    })
}

/// Stack-aware B4-2A content recorder.  Structural scroll geometry remains
/// owned by `PaintScrollContentWitness`; the extra stack only projects
/// already-owned receiver properties before the exact ScrollContents pair.
pub(super) fn record_scroll_content_local_artifact_with_stack_for_plan(
    arena: &NodeArena,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintScrollContentWitness,
    consumed_stack: super::ConsumedAncestorPropertyStackWitness,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    let content_root = witness.content_root();
    let Some(content_node) = arena.get(content_root) else {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::MissingNode(content_root),
        )]);
    };
    let Some(content_element) = content_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
    else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    };
    let Some(required_paint_offset) = content_element
        .exact_retained_scroll_content_recording_offset(witness.normalization_paint_offset())
    else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    };
    let artifact = match record_frame_artifact_with_policy_and_stack(
        arena,
        &[content_root],
        promoted_node_ids,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        FrameArtifactAuthorityPolicy::ScrollContentLocal(witness),
        None,
        Some(consumed_stack),
        None,
        Some(required_paint_offset.map(f32::to_bits)),
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    if artifact.chunks.is_empty()
        || !matches!(artifact.target, PaintArtifactTarget::CurrentTarget)
        || !artifact.clip_nodes.is_empty()
        || !artifact.effect_nodes.is_empty()
        || artifact
            .chunks
            .iter()
            .any(|chunk| chunk.properties != Default::default())
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    }
    Ok(artifact)
}

/// Strict E->S detached-content recorder. The stack must project Effect then
/// ScrollContents; the neutral authority must match the exact effect witness.
pub(super) fn record_effect_scroll_content_local_artifact_with_stack_for_plan(
    arena: &NodeArena,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintScrollContentWitness,
    consumed_stack: super::ConsumedAncestorPropertyStackWitness,
    effect: EffectNodeSnapshot,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    let content_root = witness.content_root();
    let Some(content_node) = arena.get(content_root) else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    };
    let Some(content_element) = content_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
    else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    };
    let Some(required_paint_offset) = content_element
        .exact_retained_scroll_content_recording_offset(witness.normalization_paint_offset())
    else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    };
    let artifact = match record_frame_artifact_with_policy_and_stack(
        arena,
        &[content_root],
        promoted_node_ids,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        FrameArtifactAuthorityPolicy::ScrollContentLocal(witness),
        None,
        Some(consumed_stack),
        Some(effect.id),
        Some(required_paint_offset.map(f32::to_bits)),
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    if artifact.chunks.is_empty()
        || !matches!(artifact.target, PaintArtifactTarget::CurrentTarget)
        || !artifact.clip_nodes.is_empty()
        || !artifact.effect_nodes.is_empty()
        || artifact
            .chunks
            .iter()
            .any(|chunk| chunk.properties != Default::default())
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    }
    Ok(artifact)
}

/// Planning-only recorder for one direct child opacity isolation whose
/// inherited transform is already owned by its parent retained surface.  The
/// live tree must match the witness exactly before either recording pass; the
/// artifact view projects only that consumed transform and neutralizes the
/// child root effect.
pub(super) fn record_transform_child_isolation_artifact_for_plan(
    arena: &NodeArena,
    parent_root: NodeKey,
    child_root: NodeKey,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    let transform = crate::view::compositor::property_tree::TransformNodeId(parent_root);
    let effect = EffectNodeId(child_root);
    let Some(witness) =
        super::ConsumedAncestorTransformWitness::new(parent_root, child_root, transform)
    else {
        return Err(vec![FrameArtifactFallbackReason::NonEffectProperty(
            child_root,
        )]);
    };
    let exact_transform = property_trees.transforms.len() == 1
        && property_trees
            .transforms
            .get(&transform)
            .is_some_and(|node| {
                node.owner == parent_root && node.parent.is_none() && node.generation != 0
            });
    let exact_effect = property_trees.effects.len() == 1
        && property_trees.effects.get(&effect).is_some_and(|node| {
            node.owner == child_root
                && node.parent.is_none()
                && node.generation != 0
                && node.opacity.is_finite()
                && (0.0..=1.0).contains(&node.opacity)
        });
    if arena.parent_of(child_root) != Some(parent_root)
        || !exact_transform
        || !property_trees.clips.is_empty()
        || !property_trees.scrolls.is_empty()
    {
        return Err(vec![FrameArtifactFallbackReason::NonEffectProperty(
            child_root,
        )]);
    }
    if !exact_effect {
        return Err(vec![FrameArtifactFallbackReason::InvalidRootEffect(
            child_root,
        )]);
    }

    let mut stack = vec![child_root];
    let mut seen = FxHashSet::default();
    while let Some(key) = stack.pop() {
        if !seen.insert(key) {
            return Err(vec![FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::DuplicateNodeKey(key),
            )]);
        }
        let Some(node) = arena.get(key) else {
            return Err(vec![FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::MissingNode(key),
            )]);
        };
        if node.element.is_deferred_to_root_viewport_render() {
            return Err(vec![FrameArtifactFallbackReason::DeferredBoundary(key)]);
        }
        if node
            .element
            .placement_eligibility_metadata()
            .contains_runtime_layout_state
        {
            return Err(vec![FrameArtifactFallbackReason::NonEffectProperty(key)]);
        }
        let exact_state = property_trees.states.get(&key).is_some_and(|state| {
            [state.paint, state.descendants]
                .into_iter()
                .all(|properties| {
                    properties.transform == Some(transform)
                        && properties.effect == Some(effect)
                        && properties.clip.is_none()
                        && properties.scroll.is_none()
                })
        });
        if !exact_state {
            return Err(vec![FrameArtifactFallbackReason::NonEffectProperty(key)]);
        }
        stack.extend(node.element.children().iter().copied());
    }

    let policy = FrameArtifactAuthorityPolicy::RootOpacityGroup(RootOpacityGroupPlan {
        root: child_root,
        effect,
    });
    match record_frame_artifact_with_policy(
        arena,
        &[child_root],
        promoted_node_ids,
        property_trees,
        paint_generations,
        RendererMode::StrictPlan,
        policy,
        Some(super::ConsumedAncestorProperty::Transform(witness)),
        None,
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => Ok(artifact),
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            Err(eligibility.reasons)
        }
        Err(error) => Err(error.reasons),
    }
}

#[derive(Clone, Debug)]
pub(super) enum RecordedTransformSurfaceStep {
    Artifact(PaintArtifact),
    Boundary(super::PlannedBoundary),
}

/// Planning-only ordered recorder for a transform surface with typed nested
/// cutouts. Metadata and full passes receive the identical cutout set; a
/// boundary marker flushes the current owning artifact and stops traversal
/// into the nested surface subtree.
pub(super) fn record_transform_surface_steps_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintTransformSurfaceWitness,
    paint_offset: [f32; 2],
    planned_boundary_cutouts: &super::PlannedBoundaryCutoutSet,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    record_ordered_property_steps_for_plan(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        paint_offset,
        planned_boundary_cutouts,
        Some(witness),
        None,
        None,
        PaintOpacityAuthority::Baked,
        FrameArtifactAuthorityPolicy::TransformSurface(witness),
        None,
    )
}

pub(super) fn record_property_scene_steps_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    paint_offset: [f32; 2],
    planned_boundary_cutouts: &super::PlannedBoundaryCutoutSet,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    record_ordered_property_steps_for_plan(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        paint_offset,
        planned_boundary_cutouts,
        None,
        None,
        None,
        PaintOpacityAuthority::Baked,
        FrameArtifactAuthorityPolicy::PropertyScene,
        None,
    )
}

pub(super) fn record_transform_property_surface_steps_for_plan(
    arena: &NodeArena,
    root: NodeKey,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintTransformSurfaceWitness,
    paint_offset: [f32; 2],
    planned_boundary_cutouts: &super::PlannedBoundaryCutoutSet,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    record_ordered_property_steps_for_plan(
        arena,
        &[root],
        promoted_node_ids,
        property_trees,
        paint_generations,
        paint_offset,
        planned_boundary_cutouts,
        Some(witness),
        None,
        None,
        PaintOpacityAuthority::Baked,
        FrameArtifactAuthorityPolicy::TransformPropertySurface(witness),
        None,
    )
}

/// S1 recorder for the scroll host around one direct transformed-content
/// cutout.  The result must be exactly host-before, one typed transform
/// marker, then overlay-after; the transform subtree is never traversed here.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_scroll_transform_host_steps_for_plan(
    arena: &NodeArena,
    scroll_root: NodeKey,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintBakedScrollHostWitness,
    paint_offset: [f32; 2],
    transform_cutout: super::PlannedBoundary,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    if witness.boundary_root() != scroll_root
        || transform_cutout.root != witness.child()
        || !matches!(
            transform_cutout.kind,
            super::PlannedBoundaryKind::Transform(transform)
                if transform == crate::view::compositor::property_tree::TransformNodeId(transform_cutout.root)
        )
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            transform_cutout.root,
        )]);
    }
    let cutouts =
        super::PlannedBoundaryCutoutSet::from_iter([(transform_cutout.root, transform_cutout)]);
    let steps = record_ordered_property_steps_for_plan(
        arena,
        &[scroll_root],
        promoted_node_ids,
        property_trees,
        paint_generations,
        paint_offset,
        &cutouts,
        None,
        None,
        None,
        PaintOpacityAuthority::Baked,
        FrameArtifactAuthorityPolicy::ScrollTransformHost(witness, transform_cutout),
        None,
    )?;
    let [
        RecordedTransformSurfaceStep::Artifact(host_before),
        RecordedTransformSurfaceStep::Boundary(marker),
        RecordedTransformSurfaceStep::Artifact(overlay_after),
    ] = steps.as_slice()
    else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            transform_cutout.root,
        )]);
    };
    (!host_before.chunks.is_empty()
        && *marker == transform_cutout
        && !overlay_after.chunks.is_empty())
    .then_some(steps)
    .ok_or_else(|| {
        vec![FrameArtifactFallbackReason::PropertyBoundary(
            transform_cutout.root,
        )]
    })
}

/// Ordered outer scope for the exact `S0 -> S1 -> leaf` scene.  S1 is a
/// genuine scroll boundary, so the existing typed planned-boundary machinery
/// remains authoritative and the subtree is never traversed in this scope.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_nested_scroll_outer_host_steps_for_plan(
    arena: &NodeArena,
    outer_root: NodeKey,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintBakedScrollHostWitness,
    inner_cutout: super::PlannedBoundary,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    if witness.boundary_root() != outer_root
        || inner_cutout.root != witness.child()
        || !matches!(
            inner_cutout.kind,
            super::PlannedBoundaryKind::Scroll(scroll) if scroll.0 == inner_cutout.root
        )
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            inner_cutout.root,
        )]);
    }
    let cutouts = super::PlannedBoundaryCutoutSet::from_iter([(inner_cutout.root, inner_cutout)]);
    let steps = record_ordered_property_steps_for_plan(
        arena,
        &[outer_root],
        promoted_node_ids,
        property_trees,
        paint_generations,
        [0.0, 0.0],
        &cutouts,
        None,
        None,
        None,
        PaintOpacityAuthority::Baked,
        FrameArtifactAuthorityPolicy::ScrollTransformHost(witness, inner_cutout),
        None,
    )?;
    let [
        RecordedTransformSurfaceStep::Artifact(host_before),
        RecordedTransformSurfaceStep::Boundary(marker),
        RecordedTransformSurfaceStep::Artifact(overlay_after),
    ] = steps.as_slice()
    else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            inner_cutout.root,
        )]);
    };
    (!host_before.chunks.is_empty() && *marker == inner_cutout && !overlay_after.chunks.is_empty())
        .then_some(steps)
        .ok_or_else(|| {
            vec![FrameArtifactFallbackReason::PropertyBoundary(
                inner_cutout.root,
            )]
        })
}

#[derive(Clone, Debug)]
pub(super) enum RecordedNestedScrollHostStep {
    Artifact(PaintArtifact),
    ContentReceiver(NestedScrollContentReceiverCutout),
}

fn nested_scroll_witness_matches_live_properties(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    witness: PaintNestedScrollContentWitness,
) -> bool {
    if arena.parent_of(witness.outer_boundary_root()).is_some()
        || arena.parent_of(witness.boundary_root()) != Some(witness.outer_boundary_root())
        || arena.parent_of(witness.content_root()) != Some(witness.boundary_root())
    {
        return false;
    }
    let Some(outer_scroll) = property_trees.scroll_snapshot_for(witness.outer_scroll()) else {
        return false;
    };
    let Some(inner_scroll) = property_trees.scroll_snapshot_for(witness.inner_scroll()) else {
        return false;
    };
    let Some(outer_clip) = property_trees
        .clip_snapshot_for(Some(witness.outer_contents_clip()))
        .and_then(|chain| chain.first().copied())
    else {
        return false;
    };
    let Some(inner_clip) = property_trees
        .clip_snapshot_for(Some(witness.inner_contents_clip()))
        .and_then(|chain| chain.first().copied())
    else {
        return false;
    };
    PaintNestedScrollContentWitness::new(
        witness.outer_boundary_root(),
        witness.boundary_root(),
        witness.content_root(),
        outer_scroll,
        outer_clip,
        inner_scroll,
        inner_clip,
    ) == Some(witness)
}

/// Inner host scope.  The already-owned S0/C0 pair is consumed before H1/O1
/// recording, while the leaf is represented by a dedicated typed receiver
/// marker in both metadata and full passes.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_nested_scroll_inner_host_steps_for_plan(
    arena: &NodeArena,
    inner_root: NodeKey,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    inner_host: PaintBakedScrollHostWitness,
    outer_content: PaintScrollContentWitness,
    content_stable_id: u64,
    content: PaintNestedScrollContentWitness,
) -> Result<Vec<RecordedNestedScrollHostStep>, Vec<FrameArtifactFallbackReason>> {
    if inner_host.boundary_root() != inner_root
        || inner_host.child() != content.content_root()
        || outer_content.content_root() != inner_root
        || outer_content.boundary_root() != content.outer_boundary_root()
        || outer_content.scroll_snapshot().id != content.outer_scroll()
        || outer_content.contents_clip_snapshot().id != content.outer_contents_clip()
        || content.boundary_root() != inner_root
        || content_stable_id == 0
        || !nested_scroll_witness_matches_live_properties(arena, property_trees, content)
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            inner_root,
        )]);
    }
    let receiver = NestedScrollContentReceiverCutout {
        stable_id: content_stable_id,
        witness: content,
    };
    let context = PaintRecordingContext {
        nested_scroll_host: Some(content),
        baked_scroll_host: Some(inner_host),
        ..PaintRecordingContext::default()
    };
    let record = |mode| {
        record_coverage_manifest_with_nested_scroll_receiver(
            arena,
            &[inner_root],
            promoted_node_ids,
            mode,
            property_trees,
            paint_generations,
            context,
            receiver,
        )
    };
    let metadata = record(CoverageRecordingMode::MetadataOnly);
    let full = record(CoverageRecordingMode::FullArtifact);
    if !metadata.validation_errors.is_empty()
        || !full.validation_errors.is_empty()
        || !canonical_manifest_matches(&metadata, &full)
    {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    let exact = |manifest: &super::PaintCoverageManifest| {
        let mut receiver_count = 0usize;
        manifest.items.iter().all(|item| match item {
            PaintCoverageItem::ArtifactChunk { chunk, .. } => {
                chunk.owner == inner_root && chunk.properties == Default::default()
            }
            PaintCoverageItem::TransparentNode {
                owner, properties, ..
            }
            | PaintCoverageItem::CulledSubtree {
                owner, properties, ..
            } => *owner == inner_root && *properties == Default::default(),
            PaintCoverageItem::NestedScrollContentReceiver { cutout, .. } => {
                receiver_count += 1;
                *cutout == receiver
            }
            _ => false,
        }) && receiver_count == 1
    };
    if !exact(&metadata) || !exact(&full) {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            inner_root,
        )]);
    }
    let receiver_index = full
        .items
        .iter()
        .position(|item| matches!(item, PaintCoverageItem::NestedScrollContentReceiver { .. }))
        .expect("exact nested host manifest has one receiver");
    let mut before = full.clone();
    before.items.truncate(receiver_index);
    let mut after = full;
    after.items.drain(..=receiver_index);
    let before_steps = materialize_transform_surface_steps(before)?;
    let [RecordedTransformSurfaceStep::Artifact(host_before)] = before_steps.as_slice() else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            inner_root,
        )]);
    };
    let host_before = host_before.clone();
    let after_steps = materialize_transform_surface_steps(after)?;
    let [RecordedTransformSurfaceStep::Artifact(overlay_after)] = after_steps.as_slice() else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            inner_root,
        )]);
    };
    Ok(vec![
        RecordedNestedScrollHostStep::Artifact(host_before),
        RecordedNestedScrollHostStep::ContentReceiver(receiver),
        RecordedNestedScrollHostStep::Artifact(overlay_after.clone()),
    ])
}

/// Leaf scope for nested scrolling.  Only S1/C1 is projected here; the
/// resulting artifact intentionally retains S0/C0 for the outer receiver.
pub(super) fn record_nested_scroll_content_artifact_for_plan(
    arena: &NodeArena,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintNestedScrollContentWitness,
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    let root = witness.content_root();
    if !nested_scroll_witness_matches_live_properties(arena, property_trees, witness) {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(root)]);
    }
    let expected = property_trees
        .node_state_for(witness.boundary_root())
        .map(|state| state.paint)
        .ok_or_else(|| vec![FrameArtifactFallbackReason::PropertyBoundary(root)])?;
    let context = PaintRecordingContext {
        paint_offset: witness.normalization_paint_offset(),
        nested_scroll_content: Some(witness),
        required_scroll_content_paint_offset_bits: Some(
            witness.normalization_paint_offset().map(f32::to_bits),
        ),
        ..PaintRecordingContext::default()
    };
    let record = |mode| {
        record_coverage_manifest_with_context(
            arena,
            &[root],
            promoted_node_ids,
            None,
            false,
            true,
            mode,
            property_trees,
            paint_generations,
            context,
            None,
            &Default::default(),
        )
    };
    let metadata = record(CoverageRecordingMode::MetadataOnly);
    let full = record(CoverageRecordingMode::FullArtifact);
    let exact = |manifest: &super::PaintCoverageManifest| {
        manifest.validation_errors.is_empty()
            && manifest.items.iter().all(|item| match item {
                PaintCoverageItem::ArtifactChunk { chunk, .. } => {
                    chunk.owner == root && chunk.properties == expected
                }
                PaintCoverageItem::TransparentNode {
                    owner, properties, ..
                }
                | PaintCoverageItem::CulledSubtree {
                    owner, properties, ..
                } => *owner == root && *properties == expected,
                _ => false,
            })
    };
    if !exact(&metadata) || !exact(&full) || !canonical_manifest_matches(&metadata, &full) {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    let steps = materialize_transform_surface_steps(full)?;
    let [RecordedTransformSurfaceStep::Artifact(artifact)] = steps.as_slice() else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(root)]);
    };
    (!artifact.chunks.is_empty())
        .then(|| artifact.clone())
        .ok_or_else(|| vec![FrameArtifactFallbackReason::PropertyBoundary(root)])
}

/// S1 offset-zero recorder for the direct transformed scroll content.  The
/// transform remains the surface authority while the inherited scroll and
/// contents clip are consumed atomically by the typed ancestor witness.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_scroll_transform_content_steps_for_plan(
    arena: &NodeArena,
    transform_content: NodeKey,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    transform_witness: PaintTransformSurfaceWitness,
    content_witness: PaintScrollContentWitness,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    if transform_witness.boundary_owner != transform_content
        || transform_witness.target_owner != transform_content
        || transform_witness.transform.0 != transform_content
        || content_witness.content_root() != transform_content
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            transform_content,
        )]);
    }
    let Some(content_node) = arena.get(transform_content) else {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::MissingNode(transform_content),
        )]);
    };
    let Some(content_element) = content_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
    else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            transform_content,
        )]);
    };
    let normalization = content_witness.normalization_paint_offset();
    let Some(required_paint_offset) =
        content_element.exact_retained_scroll_transform_content_recording_offset(normalization)
    else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            transform_content,
        )]);
    };
    let steps = record_ordered_property_steps_for_plan(
        arena,
        &[transform_content],
        promoted_node_ids,
        property_trees,
        paint_generations,
        normalization,
        &super::PlannedBoundaryCutoutSet::default(),
        Some(transform_witness),
        None,
        Some(content_witness.consumed_property()),
        PaintOpacityAuthority::Baked,
        FrameArtifactAuthorityPolicy::TransformPropertySurface(transform_witness),
        Some(required_paint_offset.map(f32::to_bits)),
    )?;
    let [RecordedTransformSurfaceStep::Artifact(artifact)] = steps.as_slice() else {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            transform_content,
        )]);
    };
    (!artifact.chunks.is_empty()
        && artifact.clip_nodes.is_empty()
        && artifact.effect_nodes.is_empty())
    .then_some(steps)
    .ok_or_else(|| {
        vec![FrameArtifactFallbackReason::PropertyBoundary(
            transform_content,
        )]
    })
}

/// B4-2A receiver recorder.  The scroll host is a typed cutout in both the
/// metadata and full passes; this function deliberately returns only the
/// receiver's surrounding artifacts plus the exact insertion marker.  It
/// does not record or bake the scroll subtree.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_property_scroll_receiver_steps_for_plan(
    arena: &NodeArena,
    receiver_root: NodeKey,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintTransformSurfaceWitness,
    paint_offset: [f32; 2],
    scroll_cutout: super::PlannedBoundary,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    if scroll_cutout.root == receiver_root
        || !matches!(scroll_cutout.kind, super::PlannedBoundaryKind::Scroll(_))
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            scroll_cutout.root,
        )]);
    }
    let cutouts = super::PlannedBoundaryCutoutSet::from_iter([(scroll_cutout.root, scroll_cutout)]);
    let steps = record_ordered_property_steps_for_plan(
        arena,
        &[receiver_root],
        promoted_node_ids,
        property_trees,
        paint_generations,
        paint_offset,
        &cutouts,
        Some(witness),
        None,
        None,
        PaintOpacityAuthority::Baked,
        FrameArtifactAuthorityPolicy::TransformPropertySurface(witness),
        None,
    )?;
    let markers = steps
        .iter()
        .filter(|step| {
            matches!(step, RecordedTransformSurfaceStep::Boundary(boundary) if *boundary == scroll_cutout)
        })
        .count();
    (markers == 1).then_some(steps).ok_or_else(|| {
        vec![FrameArtifactFallbackReason::PropertyBoundary(
            scroll_cutout.root,
        )]
    })
}

/// B4-2C checkpoint recorder for one direct `Effect -> ScrollContents`
/// receiver. The owning effect is neutralized by the supplied effect
/// contract, while the scroll host is emitted as one typed insertion marker.
/// Consequently neither the detached content nor the live scroll offset can
/// enter the effect receiver artifact identity.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_property_effect_scroll_receiver_steps_for_plan(
    arena: &NodeArena,
    receiver_root: NodeKey,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    contract: &EffectPropertySurfaceArtifactContract,
    paint_offset: [f32; 2],
    scroll_cutout: super::PlannedBoundary,
    consumed_transform: Option<super::ConsumedAncestorTransformWitness>,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    if contract.boundary_root() != receiver_root
        || scroll_cutout.root == receiver_root
        || !matches!(scroll_cutout.kind, super::PlannedBoundaryKind::Scroll(_))
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            scroll_cutout.root,
        )]);
    }
    let cutouts = super::PlannedBoundaryCutoutSet::from_iter([(scroll_cutout.root, scroll_cutout)]);
    let steps = record_effect_property_surface_steps_for_plan(
        arena,
        promoted_node_ids,
        property_trees,
        paint_generations,
        contract,
        paint_offset,
        &cutouts,
        consumed_transform,
    )?;
    let markers = steps
        .iter()
        .filter(|step| {
            matches!(step, RecordedTransformSurfaceStep::Boundary(boundary) if *boundary == scroll_cutout)
        })
        .count();
    (markers == 1).then_some(steps).ok_or_else(|| {
        vec![FrameArtifactFallbackReason::PropertyBoundary(
            scroll_cutout.root,
        )]
    })
}

/// Records one canonical effect surface at any property-forest depth. Direct
/// child surfaces are typed cutouts, so this pass never traverses or bakes a
/// descendant isolation. The exact live ancestor effect/clip suffixes are
/// detached by coverage before either artifact span is materialized.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_effect_property_surface_steps_for_plan(
    arena: &NodeArena,
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    contract: &EffectPropertySurfaceArtifactContract,
    paint_offset: [f32; 2],
    planned_boundary_cutouts: &super::PlannedBoundaryCutoutSet,
    consumed_transform: Option<super::ConsumedAncestorTransformWitness>,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    if !contract.is_canonical()
        || arena
            .get(contract.boundary_root())
            .is_none_or(|node| node.element.stable_id() != contract.stable_id())
        || property_trees
            .effect_snapshot_for(Some(contract.isolated_leaf().id))
            .as_deref()
            != Some(contract.live_effect_chain())
    {
        return Err(vec![FrameArtifactFallbackReason::InvalidRootEffect(
            contract.boundary_root(),
        )]);
    }
    let consumed = consumed_transform.map(super::ConsumedAncestorProperty::Transform);
    record_ordered_property_steps_for_plan(
        arena,
        &[contract.boundary_root()],
        promoted_node_ids,
        property_trees,
        paint_generations,
        paint_offset,
        planned_boundary_cutouts,
        None,
        Some(contract),
        consumed,
        PaintOpacityAuthority::NeutralRootEffect(contract.isolated_leaf().id),
        FrameArtifactAuthorityPolicy::EffectPropertySurface(contract.isolated_leaf().id),
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn record_ordered_property_steps_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    paint_offset: [f32; 2],
    planned_boundary_cutouts: &super::PlannedBoundaryCutoutSet,
    transform_surface_authority: Option<PaintTransformSurfaceWitness>,
    effect_surface_authority: Option<&EffectPropertySurfaceArtifactContract>,
    consumed_ancestor_property: Option<super::ConsumedAncestorProperty>,
    opacity_authority: PaintOpacityAuthority,
    policy: FrameArtifactAuthorityPolicy,
    required_scroll_content_paint_offset_bits: Option<[u32; 2]>,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    let context = PaintRecordingContext {
        paint_offset,
        consumed_ancestor_property,
        opacity_authority,
        required_scroll_content_paint_offset_bits,
        baked_scroll_host: baked_scroll_host_witness(policy),
        ..PaintRecordingContext::default()
    };
    let metadata = record_coverage_manifest_with_property_authorities(
        arena,
        roots,
        promoted_node_ids,
        None,
        false,
        true,
        CoverageRecordingMode::MetadataOnly,
        property_trees,
        paint_generations,
        context,
        transform_surface_authority,
        effect_surface_authority,
        planned_boundary_cutouts,
    );
    let metadata_eligibility = assess_manifest(&metadata, policy);
    if !metadata_eligibility.eligible {
        return Err(metadata_eligibility.reasons);
    }
    let full = record_coverage_manifest_with_property_authorities(
        arena,
        roots,
        promoted_node_ids,
        None,
        false,
        true,
        CoverageRecordingMode::FullArtifact,
        property_trees,
        paint_generations,
        context,
        transform_surface_authority,
        effect_surface_authority,
        planned_boundary_cutouts,
    );
    let full_eligibility = assess_manifest(&full, policy);
    if !full_eligibility.eligible {
        return Err(full_eligibility.reasons);
    }
    if !canonical_manifest_matches(&metadata, &full) {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    materialize_transform_surface_steps(full)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RootOpacityGroupPlan {
    root: NodeKey,
    effect: EffectNodeId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameArtifactAuthorityPolicy {
    ExistingBakedProperties,
    PropertyNeutral,
    ClipEnabled,
    PropertyScene,
    RootOpacityGroup(RootOpacityGroupPlan),
    TransformSurface(PaintTransformSurfaceWitness),
    TransformPropertySurface(PaintTransformSurfaceWitness),
    EffectPropertySurface(EffectNodeId),
    BakedScrollHost(PaintBakedScrollHostWitness),
    BakedScrollTextAreaSubtreeHost(
        PaintBakedScrollHostWitness,
        PaintScrollTextAreaSubtreeWitness,
    ),
    BakedScrollAtomicProjectionTextAreaSubtreeHost(
        PaintBakedScrollHostWitness,
        AtomicProjectionRecorderWitness,
    ),
    BakedScrollInteractiveTextAreaSubtreeHost(
        PaintBakedScrollHostWitness,
        PaintScrollInteractiveTextAreaSubtreeWitness,
    ),
    ScrollTransformHost(PaintBakedScrollHostWitness, super::PlannedBoundary),
    ScrollContentLocal(PaintScrollContentWitness),
    ScrollTextAreaSubtreeLocal(PaintScrollTextAreaSubtreeWitness),
    ScrollAtomicProjectionTextAreaSubtreeLocal(AtomicProjectionRecorderWitness),
    ScrollInteractiveTextAreaSubtreeLocal(PaintScrollInteractiveTextAreaSubtreeWitness),
}

fn baked_scroll_host_witness(
    policy: FrameArtifactAuthorityPolicy,
) -> Option<PaintBakedScrollHostWitness> {
    match policy {
        FrameArtifactAuthorityPolicy::BakedScrollHost(witness)
        | FrameArtifactAuthorityPolicy::BakedScrollTextAreaSubtreeHost(witness, _)
        | FrameArtifactAuthorityPolicy::BakedScrollAtomicProjectionTextAreaSubtreeHost(
            witness,
            _,
        )
        | FrameArtifactAuthorityPolicy::BakedScrollInteractiveTextAreaSubtreeHost(witness, _)
        | FrameArtifactAuthorityPolicy::ScrollTransformHost(witness, _) => Some(witness),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SnapshotMerge {
    Inserted,
    Identical,
    Conflict,
}

fn merge_snapshot<K, V>(store: &mut FxHashMap<K, V>, key: K, snapshot: V) -> SnapshotMerge
where
    K: Copy + Eq + std::hash::Hash,
    V: Copy + PartialEq,
{
    match store.entry(key) {
        Entry::Vacant(entry) => {
            entry.insert(snapshot);
            SnapshotMerge::Inserted
        }
        Entry::Occupied(entry) if *entry.get() == snapshot => SnapshotMerge::Identical,
        Entry::Occupied(_) => SnapshotMerge::Conflict,
    }
}

fn materialize_transform_surface_steps(
    manifest: super::PaintCoverageManifest,
) -> Result<Vec<RecordedTransformSurfaceStep>, Vec<FrameArtifactFallbackReason>> {
    struct SpanBuilder {
        artifact: PaintArtifact,
        clips: FxHashMap<
            crate::view::compositor::property_tree::ClipNodeId,
            crate::view::compositor::property_tree::ClipNodeSnapshot,
        >,
        effects: FxHashMap<
            crate::view::compositor::property_tree::EffectNodeId,
            crate::view::compositor::property_tree::EffectNodeSnapshot,
        >,
        owners: FxHashMap<NodeKey, super::PaintOwnerSnapshot>,
    }
    impl SpanBuilder {
        fn new() -> Self {
            Self {
                artifact: PaintArtifact {
                    target: PaintArtifactTarget::CurrentTarget,
                    ..PaintArtifact::default()
                },
                clips: FxHashMap::default(),
                effects: FxHashMap::default(),
                owners: FxHashMap::default(),
            }
        }

        fn flush(&mut self, out: &mut Vec<RecordedTransformSurfaceStep>) {
            if self.artifact.chunks.is_empty() {
                return;
            }
            out.push(RecordedTransformSurfaceStep::Artifact(std::mem::replace(
                &mut self.artifact,
                PaintArtifact {
                    target: PaintArtifactTarget::CurrentTarget,
                    ..PaintArtifact::default()
                },
            )));
            self.clips.clear();
            self.effects.clear();
            self.owners.clear();
        }
    }

    let conflict = |error| vec![FrameArtifactFallbackReason::Validation(error)];
    let mut steps = Vec::new();
    let mut span = SpanBuilder::new();
    for item in manifest.items {
        match item {
            PaintCoverageItem::ArtifactChunk {
                chunk,
                clip_snapshot,
                effect_snapshot,
                owner_snapshot,
                ops: Some(ops),
                ..
            } => {
                let start = span.artifact.ops.len();
                span.artifact.ops.extend(ops);
                let end = span.artifact.ops.len();
                span.artifact.chunks.push(PaintChunk {
                    id: chunk.id,
                    owner: chunk.owner,
                    op_range: start..end,
                    bounds: chunk.bounds,
                    properties: chunk.properties,
                    content_revision: chunk.content_revision,
                    payload_identity: chunk.payload_identity,
                });
                for snapshot in clip_snapshot {
                    match merge_snapshot(&mut span.clips, snapshot.id, snapshot) {
                        SnapshotMerge::Inserted => span.artifact.clip_nodes.push(snapshot),
                        SnapshotMerge::Identical => {}
                        SnapshotMerge::Conflict => {
                            return Err(conflict(
                                PaintCoverageValidationError::ConflictingClipSnapshot(snapshot.id),
                            ));
                        }
                    }
                }
                for snapshot in effect_snapshot {
                    match merge_snapshot(&mut span.effects, snapshot.id, snapshot) {
                        SnapshotMerge::Inserted => span.artifact.effect_nodes.push(snapshot),
                        SnapshotMerge::Identical => {}
                        SnapshotMerge::Conflict => {
                            return Err(conflict(
                                PaintCoverageValidationError::ConflictingEffectSnapshot(
                                    snapshot.id,
                                ),
                            ));
                        }
                    }
                }
                for snapshot in owner_snapshot {
                    match merge_snapshot(&mut span.owners, snapshot.owner, snapshot) {
                        SnapshotMerge::Inserted => span.artifact.owner_nodes.push(snapshot),
                        SnapshotMerge::Identical => {}
                        SnapshotMerge::Conflict => {
                            return Err(conflict(
                                PaintCoverageValidationError::ConflictingOwnerSnapshot(
                                    snapshot.owner,
                                ),
                            ));
                        }
                    }
                }
            }
            PaintCoverageItem::PlannedBoundary { boundary, .. } => {
                span.flush(&mut steps);
                steps.push(RecordedTransformSurfaceStep::Boundary(boundary));
            }
            PaintCoverageItem::TransparentNode { .. } | PaintCoverageItem::CulledSubtree { .. } => {
            }
            PaintCoverageItem::ArtifactChunk { ops: None, .. }
            | PaintCoverageItem::LegacyBoundary { .. }
            | PaintCoverageItem::PromotedBoundary { .. }
            | PaintCoverageItem::NestedScrollContentReceiver { .. } => unreachable!(
                "eligible full transform-surface manifest has only chunks, transparent nodes, culled nodes, and planned boundaries"
            ),
        }
    }
    span.flush(&mut steps);
    Ok(steps)
}

fn record_frame_artifact_with_policy(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    mode: RendererMode,
    policy: FrameArtifactAuthorityPolicy,
    consumed_ancestor_property: Option<super::ConsumedAncestorProperty>,
    required_scroll_content_paint_offset_bits: Option<[u32; 2]>,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    record_frame_artifact_with_policy_and_stack(
        arena,
        roots,
        promoted_node_ids,
        property_trees,
        paint_generations,
        mode,
        policy,
        consumed_ancestor_property,
        None,
        None,
        required_scroll_content_paint_offset_bits,
    )
}

#[allow(clippy::too_many_arguments)]
fn record_frame_artifact_with_policy_and_stack(
    arena: &NodeArena,
    roots: &[NodeKey],
    promoted_node_ids: &FxHashSet<u64>,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    mode: RendererMode,
    policy: FrameArtifactAuthorityPolicy,
    consumed_ancestor_property: Option<super::ConsumedAncestorProperty>,
    consumed_ancestor_property_stack: Option<super::ConsumedAncestorPropertyStackWitness>,
    neutral_effect_authority: Option<EffectNodeId>,
    required_scroll_content_paint_offset_bits: Option<[u32; 2]>,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    if mode == RendererMode::Legacy {
        return Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(
            FrameArtifactEligibility {
                eligible: false,
                reasons: vec![FrameArtifactFallbackReason::RendererLegacy],
                ..FrameArtifactEligibility::default()
            },
        ));
    }
    let initial_recording_context =
        PaintRecordingContext {
            paint_offset: match policy {
                FrameArtifactAuthorityPolicy::ScrollContentLocal(witness) => {
                    witness.normalization_paint_offset()
                }
                FrameArtifactAuthorityPolicy::ScrollTextAreaSubtreeLocal(witness) => {
                    witness.outer().normalization_paint_offset()
                }
                FrameArtifactAuthorityPolicy::ScrollAtomicProjectionTextAreaSubtreeLocal(
                    witness,
                ) => witness.outer().normalization_paint_offset(),
                FrameArtifactAuthorityPolicy::ScrollInteractiveTextAreaSubtreeLocal(witness) => {
                    witness.outer().normalization_paint_offset()
                }
                _ => [0.0, 0.0],
            },
            consumed_ancestor_property: match policy {
                FrameArtifactAuthorityPolicy::ScrollContentLocal(witness) => {
                    consumed_ancestor_property_stack
                        .is_none()
                        .then(|| witness.consumed_property())
                }
                FrameArtifactAuthorityPolicy::ScrollTextAreaSubtreeLocal(_) => None,
                FrameArtifactAuthorityPolicy::ScrollAtomicProjectionTextAreaSubtreeLocal(_) => None,
                FrameArtifactAuthorityPolicy::ScrollInteractiveTextAreaSubtreeLocal(_) => None,
                _ => consumed_ancestor_property,
            },
            consumed_ancestor_property_stack,
            required_scroll_content_paint_offset_bits,
            opacity_authority: if let Some(effect) = neutral_effect_authority {
                PaintOpacityAuthority::NeutralRootEffect(effect)
            } else {
                match policy {
                FrameArtifactAuthorityPolicy::RootOpacityGroup(plan) => {
                    PaintOpacityAuthority::NeutralRootEffect(plan.effect)
                }
                FrameArtifactAuthorityPolicy::EffectPropertySurface(effect) => {
                    PaintOpacityAuthority::NeutralRootEffect(effect)
                }
                FrameArtifactAuthorityPolicy::ExistingBakedProperties
                | FrameArtifactAuthorityPolicy::PropertyNeutral
                | FrameArtifactAuthorityPolicy::ClipEnabled
                | FrameArtifactAuthorityPolicy::PropertyScene
                | FrameArtifactAuthorityPolicy::TransformSurface(_)
                | FrameArtifactAuthorityPolicy::TransformPropertySurface(_)
                | FrameArtifactAuthorityPolicy::BakedScrollHost(_)
                | FrameArtifactAuthorityPolicy::BakedScrollTextAreaSubtreeHost(_, _)
                | FrameArtifactAuthorityPolicy::BakedScrollAtomicProjectionTextAreaSubtreeHost(_, _)
                | FrameArtifactAuthorityPolicy::BakedScrollInteractiveTextAreaSubtreeHost(_, _)
                | FrameArtifactAuthorityPolicy::ScrollTransformHost(_, _)
                | FrameArtifactAuthorityPolicy::ScrollContentLocal(_)
                | FrameArtifactAuthorityPolicy::ScrollTextAreaSubtreeLocal(_) => {
                    PaintOpacityAuthority::Baked
                }
                FrameArtifactAuthorityPolicy::ScrollAtomicProjectionTextAreaSubtreeLocal(_) => {
                    PaintOpacityAuthority::Baked
                }
                FrameArtifactAuthorityPolicy::ScrollInteractiveTextAreaSubtreeLocal(_) => {
                    PaintOpacityAuthority::Baked
                }
            }
            },
            baked_scroll_host: baked_scroll_host_witness(policy),
            scroll_text_area_subtree: match policy {
                FrameArtifactAuthorityPolicy::ScrollTextAreaSubtreeLocal(witness) => Some(witness),
                _ => None,
            },
            baked_scroll_text_area_subtree: match policy {
                FrameArtifactAuthorityPolicy::BakedScrollTextAreaSubtreeHost(_, witness) => {
                    Some(witness)
                }
                _ => None,
            },
            scroll_atomic_projection_text_area_subtree: match policy {
                FrameArtifactAuthorityPolicy::ScrollAtomicProjectionTextAreaSubtreeLocal(
                    witness,
                ) => Some(witness),
                _ => None,
            },
            baked_scroll_atomic_projection_text_area_subtree: match policy {
                FrameArtifactAuthorityPolicy::BakedScrollAtomicProjectionTextAreaSubtreeHost(
                    _,
                    witness,
                ) => Some(witness),
                _ => None,
            },
            scroll_interactive_text_area_subtree: match policy {
                FrameArtifactAuthorityPolicy::ScrollInteractiveTextAreaSubtreeLocal(witness) => {
                    Some(witness)
                }
                _ => None,
            },
            baked_scroll_interactive_text_area_subtree: match policy {
                FrameArtifactAuthorityPolicy::BakedScrollInteractiveTextAreaSubtreeHost(
                    _,
                    witness,
                ) => Some(witness),
                _ => None,
            },
            ..PaintRecordingContext::default()
        };
    let planned_boundary_cutouts = super::PlannedBoundaryCutoutSet::default();
    let preflight = record_coverage_manifest_with_context(
        arena,
        roots,
        promoted_node_ids,
        None,
        false,
        true,
        CoverageRecordingMode::MetadataOnly,
        property_trees,
        paint_generations,
        initial_recording_context,
        match policy {
            FrameArtifactAuthorityPolicy::TransformSurface(witness)
            | FrameArtifactAuthorityPolicy::TransformPropertySurface(witness) => Some(witness),
            _ => None,
        },
        &planned_boundary_cutouts,
    );
    let mut preflight_eligibility = assess_manifest(&preflight, policy);
    if matches!(
        policy,
        FrameArtifactAuthorityPolicy::PropertyNeutral | FrameArtifactAuthorityPolicy::ClipEnabled
    ) {
        for reason in production_property_boundary_reasons(arena, roots, property_trees, policy) {
            if !preflight_eligibility.reasons.contains(&reason) {
                preflight_eligibility.reasons.push(reason);
            }
        }
        preflight_eligibility.eligible = preflight_eligibility.reasons.is_empty();
    }
    if !preflight_eligibility.eligible {
        return fallback_or_forced(mode, preflight_eligibility);
    }

    let manifest = record_coverage_manifest_with_context(
        arena,
        roots,
        promoted_node_ids,
        None,
        false,
        true,
        CoverageRecordingMode::FullArtifact,
        property_trees,
        paint_generations,
        initial_recording_context,
        match policy {
            FrameArtifactAuthorityPolicy::TransformSurface(witness)
            | FrameArtifactAuthorityPolicy::TransformPropertySurface(witness) => Some(witness),
            _ => None,
        },
        &planned_boundary_cutouts,
    );
    let mut eligibility = assess_manifest(&manifest, policy);
    if eligibility.eligible && !canonical_manifest_matches(&preflight, &manifest) {
        eligibility.eligible = false;
        eligibility
            .reasons
            .push(FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::RecordingPassMismatch,
            ));
    }
    if !eligibility.eligible {
        return fallback_or_forced(mode, eligibility);
    }

    let mut artifact = PaintArtifact {
        target: match policy {
            FrameArtifactAuthorityPolicy::RootOpacityGroup(plan) => {
                PaintArtifactTarget::RootOpacityGroup {
                    root: plan.root,
                    effect: plan.effect,
                }
            }
            FrameArtifactAuthorityPolicy::ExistingBakedProperties
            | FrameArtifactAuthorityPolicy::PropertyNeutral
            | FrameArtifactAuthorityPolicy::ClipEnabled
            | FrameArtifactAuthorityPolicy::PropertyScene
            | FrameArtifactAuthorityPolicy::TransformSurface(_)
            | FrameArtifactAuthorityPolicy::TransformPropertySurface(_)
            | FrameArtifactAuthorityPolicy::EffectPropertySurface(_)
            | FrameArtifactAuthorityPolicy::BakedScrollHost(_)
            | FrameArtifactAuthorityPolicy::BakedScrollTextAreaSubtreeHost(_, _)
            | FrameArtifactAuthorityPolicy::BakedScrollAtomicProjectionTextAreaSubtreeHost(_, _)
            | FrameArtifactAuthorityPolicy::BakedScrollInteractiveTextAreaSubtreeHost(_, _)
            | FrameArtifactAuthorityPolicy::ScrollTransformHost(_, _)
            | FrameArtifactAuthorityPolicy::ScrollContentLocal(_)
            | FrameArtifactAuthorityPolicy::ScrollTextAreaSubtreeLocal(_)
            | FrameArtifactAuthorityPolicy::ScrollAtomicProjectionTextAreaSubtreeLocal(_)
            | FrameArtifactAuthorityPolicy::ScrollInteractiveTextAreaSubtreeLocal(_) => {
                PaintArtifactTarget::CurrentTarget
            }
        },
        ..PaintArtifact::default()
    };
    let mut seen_clip_nodes = FxHashMap::default();
    let mut seen_effect_nodes = FxHashMap::default();
    let mut seen_owner_nodes = FxHashMap::default();
    for item in manifest.items {
        let PaintCoverageItem::ArtifactChunk {
            chunk,
            clip_snapshot,
            effect_snapshot,
            owner_snapshot,
            ops: Some(ops),
            ..
        } = item
        else {
            if matches!(
                item,
                PaintCoverageItem::TransparentNode { .. }
                    | PaintCoverageItem::CulledSubtree { .. }
                    | PaintCoverageItem::PlannedBoundary { .. }
            ) {
                continue;
            }
            unreachable!("eligibility rejects all paint boundaries")
        };
        let start = artifact.ops.len();
        artifact.ops.extend(ops);
        let end = artifact.ops.len();
        artifact.chunks.push(PaintChunk {
            id: chunk.id,
            owner: chunk.owner,
            op_range: start..end,
            bounds: chunk.bounds,
            properties: chunk.properties,
            content_revision: chunk.content_revision,
            payload_identity: chunk.payload_identity,
        });
        for snapshot in clip_snapshot {
            match merge_snapshot(&mut seen_clip_nodes, snapshot.id, snapshot) {
                SnapshotMerge::Inserted => {
                    artifact.clip_nodes.push(snapshot);
                }
                SnapshotMerge::Conflict => {
                    eligibility.eligible = false;
                    eligibility
                        .reasons
                        .push(FrameArtifactFallbackReason::Validation(
                            PaintCoverageValidationError::ConflictingClipSnapshot(snapshot.id),
                        ));
                    return fallback_or_forced(mode, eligibility);
                }
                SnapshotMerge::Identical => {}
            }
        }
        for snapshot in effect_snapshot {
            match merge_snapshot(&mut seen_effect_nodes, snapshot.id, snapshot) {
                SnapshotMerge::Inserted => {
                    artifact.effect_nodes.push(snapshot);
                }
                SnapshotMerge::Conflict => {
                    eligibility.eligible = false;
                    eligibility
                        .reasons
                        .push(FrameArtifactFallbackReason::Validation(
                            PaintCoverageValidationError::ConflictingEffectSnapshot(snapshot.id),
                        ));
                    return fallback_or_forced(mode, eligibility);
                }
                SnapshotMerge::Identical => {}
            }
        }
        for snapshot in owner_snapshot {
            match merge_snapshot(&mut seen_owner_nodes, snapshot.owner, snapshot) {
                SnapshotMerge::Inserted => {
                    artifact.owner_nodes.push(snapshot);
                }
                SnapshotMerge::Conflict => {
                    eligibility.eligible = false;
                    eligibility
                        .reasons
                        .push(FrameArtifactFallbackReason::Validation(
                            PaintCoverageValidationError::ConflictingOwnerSnapshot(snapshot.owner),
                        ));
                    return fallback_or_forced(mode, eligibility);
                }
                SnapshotMerge::Identical => {}
            }
        }
    }
    Ok(FrameArtifactRecordOutcome::Artifact {
        artifact,
        eligibility,
    })
}

#[cfg(test)]
mod snapshot_merge_tests {
    use super::*;

    #[test]
    fn snapshot_merge_rejects_conflicting_duplicate_identity() {
        let mut store = FxHashMap::default();
        assert_eq!(
            merge_snapshot(&mut store, 7_u64, 11_u64),
            SnapshotMerge::Inserted
        );
        assert_eq!(merge_snapshot(&mut store, 7, 11), SnapshotMerge::Identical);
        assert_eq!(merge_snapshot(&mut store, 7, 12), SnapshotMerge::Conflict);
        assert_eq!(
            store[&7], 11,
            "conflict must not replace the canonical first snapshot"
        );
    }
}

fn assess_manifest(
    manifest: &super::PaintCoverageManifest,
    policy: FrameArtifactAuthorityPolicy,
) -> FrameArtifactEligibility {
    let mut reasons = manifest
        .validation_errors
        .iter()
        .cloned()
        .map(FrameArtifactFallbackReason::Validation)
        .collect::<Vec<_>>();
    let mut chunk_count = 0usize;
    let mut op_count = 0usize;
    for item in &manifest.items {
        match item {
            PaintCoverageItem::ArtifactChunk { chunk, ops, .. } => {
                chunk_count = chunk_count.saturating_add(1);
                op_count = op_count.saturating_add(ops.as_ref().map_or(0, Vec::len));
                if policy == FrameArtifactAuthorityPolicy::PropertyNeutral
                    && chunk.properties != Default::default()
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if matches!(policy, FrameArtifactAuthorityPolicy::ScrollContentLocal(_))
                    && chunk.properties != Default::default()
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let FrameArtifactAuthorityPolicy::ScrollTextAreaSubtreeLocal(witness) = policy
                    && !scroll_text_area_subtree_local_properties_are_exact(
                        chunk.properties,
                        witness,
                    )
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let FrameArtifactAuthorityPolicy::ScrollAtomicProjectionTextAreaSubtreeLocal(
                    witness,
                ) = policy
                    && !scroll_atomic_projection_text_area_subtree_local_properties_are_exact(
                        chunk.properties,
                        witness.property(),
                    )
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let FrameArtifactAuthorityPolicy::ScrollInteractiveTextAreaSubtreeLocal(witness) =
                    policy
                    && !scroll_interactive_text_area_subtree_local_properties_are_exact(
                        chunk.properties,
                        witness,
                    )
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if policy == FrameArtifactAuthorityPolicy::ClipEnabled
                    && (chunk.properties.transform.is_some()
                        || chunk.properties.effect.is_some()
                        || chunk.properties.scroll.is_some())
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if policy == FrameArtifactAuthorityPolicy::PropertyScene
                    && (chunk.properties.transform.is_some()
                        || chunk.properties.effect.is_some()
                        || chunk.properties.scroll.is_some())
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let FrameArtifactAuthorityPolicy::RootOpacityGroup(plan) = policy {
                    if chunk.properties.effect != Some(plan.effect) {
                        let reason = FrameArtifactFallbackReason::NestedEffect(chunk.owner);
                        if !reasons.contains(&reason) {
                            reasons.push(reason);
                        }
                    }
                    if chunk.properties.transform.is_some() || chunk.properties.scroll.is_some() {
                        let reason = FrameArtifactFallbackReason::NonEffectProperty(chunk.owner);
                        if !reasons.contains(&reason) {
                            reasons.push(reason);
                        }
                    }
                }
                if let FrameArtifactAuthorityPolicy::EffectPropertySurface(effect) = policy
                    && (chunk.properties.effect != Some(effect)
                        || chunk.properties.transform.is_some()
                        || chunk.properties.scroll.is_some())
                {
                    let reason = FrameArtifactFallbackReason::NonEffectProperty(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let FrameArtifactAuthorityPolicy::TransformSurface(witness) = policy
                    && !transform_surface_properties_are_exact(chunk.properties, witness)
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let FrameArtifactAuthorityPolicy::TransformPropertySurface(witness) = policy
                    && !transform_property_surface_properties_are_exact(chunk.properties, witness)
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let FrameArtifactAuthorityPolicy::BakedScrollTextAreaSubtreeHost(
                    baked,
                    text_area,
                ) = policy
                    && !baked_scroll_text_area_subtree_properties_are_exact(
                        chunk.owner,
                        chunk.properties,
                        baked,
                        text_area,
                    )
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                } else if let FrameArtifactAuthorityPolicy::BakedScrollAtomicProjectionTextAreaSubtreeHost(
                    baked,
                    text_area,
                ) = policy
                    && !baked_scroll_atomic_projection_text_area_subtree_properties_are_exact(
                        chunk.owner,
                        chunk.properties,
                        baked,
                        text_area.property(),
                    )
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(chunk.owner);
                    if !reasons.contains(&reason) { reasons.push(reason); }
                } else if let FrameArtifactAuthorityPolicy::BakedScrollInteractiveTextAreaSubtreeHost(
                    baked,
                    text_area,
                ) = policy
                    && !baked_scroll_interactive_text_area_subtree_properties_are_exact(
                        chunk.owner,
                        chunk.properties,
                        baked,
                        text_area,
                    )
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                } else if !matches!(
                    policy,
                    FrameArtifactAuthorityPolicy::BakedScrollTextAreaSubtreeHost(_, _)
                        | FrameArtifactAuthorityPolicy::BakedScrollAtomicProjectionTextAreaSubtreeHost(_, _)
                        | FrameArtifactAuthorityPolicy::BakedScrollInteractiveTextAreaSubtreeHost(_, _)
                ) && let Some(witness) = baked_scroll_host_witness(policy)
                    && !baked_scroll_host_properties_are_exact(
                        chunk.owner,
                        chunk.properties,
                        witness,
                    )
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(chunk.owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
            }
            PaintCoverageItem::TransparentNode {
                owner, properties, ..
            } => {
                if matches!(policy, FrameArtifactAuthorityPolicy::ScrollContentLocal(_))
                    && *properties != Default::default()
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(*owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let FrameArtifactAuthorityPolicy::ScrollTextAreaSubtreeLocal(witness) = policy
                    && !scroll_text_area_subtree_local_properties_are_exact(*properties, witness)
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(*owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let FrameArtifactAuthorityPolicy::ScrollAtomicProjectionTextAreaSubtreeLocal(
                    witness,
                ) = policy
                    && !scroll_atomic_projection_text_area_subtree_local_properties_are_exact(
                        *properties,
                        witness.property(),
                    )
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(*owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let FrameArtifactAuthorityPolicy::ScrollInteractiveTextAreaSubtreeLocal(witness) =
                    policy
                    && !scroll_interactive_text_area_subtree_local_properties_are_exact(
                        *properties,
                        witness,
                    )
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(*owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let FrameArtifactAuthorityPolicy::TransformSurface(witness) = policy
                    && !transform_surface_properties_are_exact(*properties, witness)
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(*owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if policy == FrameArtifactAuthorityPolicy::PropertyScene
                    && (properties.transform.is_some()
                        || properties.effect.is_some()
                        || properties.scroll.is_some())
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(*owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let FrameArtifactAuthorityPolicy::EffectPropertySurface(effect) = policy
                    && (properties.effect != Some(effect)
                        || properties.transform.is_some()
                        || properties.scroll.is_some())
                {
                    let reason = FrameArtifactFallbackReason::NonEffectProperty(*owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let FrameArtifactAuthorityPolicy::TransformPropertySurface(witness) = policy
                    && !transform_property_surface_properties_are_exact(*properties, witness)
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(*owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
                if let FrameArtifactAuthorityPolicy::BakedScrollTextAreaSubtreeHost(
                    baked,
                    text_area,
                ) = policy
                    && !baked_scroll_text_area_subtree_properties_are_exact(
                        *owner,
                        *properties,
                        baked,
                        text_area,
                    )
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(*owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                } else if let FrameArtifactAuthorityPolicy::BakedScrollAtomicProjectionTextAreaSubtreeHost(
                    baked,
                    text_area,
                ) = policy
                    && !baked_scroll_atomic_projection_text_area_subtree_properties_are_exact(
                        *owner,
                        *properties,
                        baked,
                        text_area.property(),
                    )
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(*owner);
                    if !reasons.contains(&reason) { reasons.push(reason); }
                } else if let FrameArtifactAuthorityPolicy::BakedScrollInteractiveTextAreaSubtreeHost(
                    baked,
                    text_area,
                ) = policy
                    && !baked_scroll_interactive_text_area_subtree_properties_are_exact(
                        *owner,
                        *properties,
                        baked,
                        text_area,
                    )
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(*owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                } else if !matches!(
                    policy,
                    FrameArtifactAuthorityPolicy::BakedScrollTextAreaSubtreeHost(_, _)
                        | FrameArtifactAuthorityPolicy::BakedScrollAtomicProjectionTextAreaSubtreeHost(_, _)
                        | FrameArtifactAuthorityPolicy::BakedScrollInteractiveTextAreaSubtreeHost(_, _)
                ) && let Some(witness) = baked_scroll_host_witness(policy)
                    && !baked_scroll_host_properties_are_exact(*owner, *properties, witness)
                {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(*owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
            }
            PaintCoverageItem::CulledSubtree {
                owner, properties, ..
            } => {
                let invalid = match policy {
                    FrameArtifactAuthorityPolicy::TransformSurface(witness) => {
                        !transform_surface_properties_are_exact(*properties, witness)
                    }
                    FrameArtifactAuthorityPolicy::TransformPropertySurface(witness) => {
                        !transform_property_surface_properties_are_exact(*properties, witness)
                    }
                    FrameArtifactAuthorityPolicy::PropertyScene => {
                        properties.transform.is_some()
                            || properties.effect.is_some()
                            || properties.scroll.is_some()
                    }
                    FrameArtifactAuthorityPolicy::EffectPropertySurface(effect) => {
                        properties.effect != Some(effect)
                            || properties.transform.is_some()
                            || properties.scroll.is_some()
                    }
                    FrameArtifactAuthorityPolicy::BakedScrollHost(witness) => {
                        !baked_scroll_host_properties_are_exact(*owner, *properties, witness)
                    }
                    FrameArtifactAuthorityPolicy::BakedScrollTextAreaSubtreeHost(
                        baked,
                        text_area,
                    ) => !baked_scroll_text_area_subtree_properties_are_exact(
                        *owner,
                        *properties,
                        baked,
                        text_area,
                    ),
                    FrameArtifactAuthorityPolicy::BakedScrollAtomicProjectionTextAreaSubtreeHost(
                        baked,
                        text_area,
                    ) => !baked_scroll_atomic_projection_text_area_subtree_properties_are_exact(
                        *owner,
                        *properties,
                        baked,
                        text_area.property(),
                    ),
                    FrameArtifactAuthorityPolicy::BakedScrollInteractiveTextAreaSubtreeHost(
                        baked,
                        text_area,
                    ) => !baked_scroll_interactive_text_area_subtree_properties_are_exact(
                        *owner,
                        *properties,
                        baked,
                        text_area,
                    ),
                    FrameArtifactAuthorityPolicy::ScrollTransformHost(witness, _) => {
                        !baked_scroll_host_properties_are_exact(*owner, *properties, witness)
                    }
                    FrameArtifactAuthorityPolicy::ScrollContentLocal(_) => {
                        *properties != Default::default()
                    }
                    FrameArtifactAuthorityPolicy::ScrollTextAreaSubtreeLocal(witness) => {
                        !scroll_text_area_subtree_local_properties_are_exact(*properties, witness)
                    }
                    FrameArtifactAuthorityPolicy::ScrollAtomicProjectionTextAreaSubtreeLocal(witness) => {
                        !scroll_atomic_projection_text_area_subtree_local_properties_are_exact(*properties, witness.property())
                    }
                    FrameArtifactAuthorityPolicy::ScrollInteractiveTextAreaSubtreeLocal(
                        witness,
                    ) => !scroll_interactive_text_area_subtree_local_properties_are_exact(
                        *properties,
                        witness,
                    ),
                    _ => {
                        properties.transform.is_some()
                            || properties.effect.is_some()
                            || properties.scroll.is_some()
                    }
                };
                if invalid {
                    let reason = FrameArtifactFallbackReason::PropertyBoundary(*owner);
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
            }
            PaintCoverageItem::LegacyBoundary { reason, .. } => {
                let reason = FrameArtifactFallbackReason::LegacyBoundary(*reason);
                if !reasons.contains(&reason) {
                    reasons.push(reason);
                }
            }
            PaintCoverageItem::PromotedBoundary { .. } => {
                if !reasons.contains(&FrameArtifactFallbackReason::PromotedBoundary) {
                    reasons.push(FrameArtifactFallbackReason::PromotedBoundary);
                }
            }
            PaintCoverageItem::PlannedBoundary { boundary, .. } => {
                let allowed = match policy {
                    FrameArtifactAuthorityPolicy::TransformSurface(_)
                    | FrameArtifactAuthorityPolicy::TransformPropertySurface(_)
                    | FrameArtifactAuthorityPolicy::EffectPropertySurface(_)
                    | FrameArtifactAuthorityPolicy::PropertyScene => true,
                    FrameArtifactAuthorityPolicy::ScrollTransformHost(_, expected) => {
                        *boundary == expected
                    }
                    _ => false,
                };
                if !allowed {
                    let reason = FrameArtifactFallbackReason::Validation(
                        PaintCoverageValidationError::RecordingPassMismatch,
                    );
                    if !reasons.contains(&reason) {
                        reasons.push(reason);
                    }
                }
            }
            PaintCoverageItem::NestedScrollContentReceiver { .. } => {
                let reason = FrameArtifactFallbackReason::Validation(
                    PaintCoverageValidationError::RecordingPassMismatch,
                );
                if !reasons.contains(&reason) {
                    reasons.push(reason);
                }
            }
        }
    }
    FrameArtifactEligibility {
        eligible: reasons.is_empty(),
        reasons: reasons.clone(),
        chunk_count,
        op_count,
    }
}

fn transform_surface_properties_are_exact(
    properties: crate::view::compositor::property_tree::PropertyTreeState,
    witness: PaintTransformSurfaceWitness,
) -> bool {
    properties.transform == Some(witness.transform)
        && properties.clip.is_none()
        && properties.effect.is_none()
        && properties.scroll.is_none()
}

fn transform_property_surface_properties_are_exact(
    properties: crate::view::compositor::property_tree::PropertyTreeState,
    witness: PaintTransformSurfaceWitness,
) -> bool {
    properties.transform == Some(witness.transform)
        && properties.effect.is_none()
        && properties.scroll.is_none()
}

fn baked_scroll_host_properties_are_exact(
    owner: NodeKey,
    properties: crate::view::compositor::property_tree::PropertyTreeState,
    witness: PaintBakedScrollHostWitness,
) -> bool {
    if owner == witness.boundary_root() {
        properties == Default::default()
    } else if owner == witness.child() {
        properties.transform.is_none()
            && properties.effect.is_none()
            && properties.scroll == Some(witness.scroll())
            && properties.clip == Some(witness.contents_clip())
    } else {
        false
    }
}

fn baked_scroll_text_area_subtree_properties_are_exact(
    owner: NodeKey,
    properties: crate::view::compositor::property_tree::PropertyTreeState,
    baked: PaintBakedScrollHostWitness,
    text_area: PaintScrollTextAreaSubtreeWitness,
) -> bool {
    if text_area.outer().boundary_root() != baked.boundary_root()
        || text_area.outer().content_root() != baked.child()
        || text_area.outer().scroll_snapshot().id != baked.scroll()
        || text_area.outer().contents_clip_snapshot().id != baked.contents_clip()
    {
        return false;
    }
    if owner == baked.boundary_root() {
        properties == Default::default()
    } else if owner == baked.child() {
        properties
            == crate::view::compositor::property_tree::PropertyTreeState {
                clip: Some(baked.contents_clip()),
                scroll: Some(baked.scroll()),
                ..Default::default()
            }
    } else {
        properties
            == crate::view::compositor::property_tree::PropertyTreeState {
                clip: Some(text_area.live_contents_clip().id),
                scroll: Some(baked.scroll()),
                ..Default::default()
            }
    }
}

fn scroll_text_area_subtree_local_properties_are_exact(
    properties: crate::view::compositor::property_tree::PropertyTreeState,
    witness: PaintScrollTextAreaSubtreeWitness,
) -> bool {
    properties == Default::default()
        || properties
            == crate::view::compositor::property_tree::PropertyTreeState {
                clip: Some(witness.local_contents_clip().id),
                ..Default::default()
            }
}

fn baked_scroll_atomic_projection_text_area_subtree_properties_are_exact(
    owner: NodeKey,
    properties: crate::view::compositor::property_tree::PropertyTreeState,
    baked: PaintBakedScrollHostWitness,
    text_area: PaintScrollAtomicProjectionTextAreaSubtreeWitness,
) -> bool {
    if text_area.outer().boundary_root() != baked.boundary_root()
        || text_area.outer().content_root() != baked.child()
        || text_area.outer().scroll_snapshot().id != baked.scroll()
        || text_area.outer().contents_clip_snapshot().id != baked.contents_clip()
    {
        return false;
    }
    if owner == baked.boundary_root() {
        properties == Default::default()
    } else if owner == baked.child() {
        properties
            == crate::view::compositor::property_tree::PropertyTreeState {
                clip: Some(baked.contents_clip()),
                scroll: Some(baked.scroll()),
                ..Default::default()
            }
    } else {
        properties
            == crate::view::compositor::property_tree::PropertyTreeState {
                clip: Some(text_area.live_contents_clip().id),
                scroll: Some(baked.scroll()),
                ..Default::default()
            }
    }
}

fn scroll_atomic_projection_text_area_subtree_local_properties_are_exact(
    properties: crate::view::compositor::property_tree::PropertyTreeState,
    witness: PaintScrollAtomicProjectionTextAreaSubtreeWitness,
) -> bool {
    properties == Default::default()
        || properties
            == crate::view::compositor::property_tree::PropertyTreeState {
                clip: Some(witness.local_contents_clip().id),
                ..Default::default()
            }
}

fn baked_scroll_interactive_text_area_subtree_properties_are_exact(
    owner: NodeKey,
    properties: crate::view::compositor::property_tree::PropertyTreeState,
    baked: PaintBakedScrollHostWitness,
    text_area: PaintScrollInteractiveTextAreaSubtreeWitness,
) -> bool {
    if text_area.outer().boundary_root() != baked.boundary_root()
        || text_area.outer().content_root() != baked.child()
        || text_area.outer().scroll_snapshot().id != baked.scroll()
        || text_area.outer().contents_clip_snapshot().id != baked.contents_clip()
    {
        return false;
    }
    if owner == baked.boundary_root() {
        properties == Default::default()
    } else if owner == baked.child() {
        properties
            == crate::view::compositor::property_tree::PropertyTreeState {
                clip: Some(baked.contents_clip()),
                scroll: Some(baked.scroll()),
                ..Default::default()
            }
    } else {
        properties
            == crate::view::compositor::property_tree::PropertyTreeState {
                clip: Some(text_area.live_contents_clip().id),
                scroll: Some(baked.scroll()),
                ..Default::default()
            }
    }
}

fn scroll_interactive_text_area_subtree_local_properties_are_exact(
    properties: crate::view::compositor::property_tree::PropertyTreeState,
    witness: PaintScrollInteractiveTextAreaSubtreeWitness,
) -> bool {
    properties == Default::default()
        || properties
            == crate::view::compositor::property_tree::PropertyTreeState {
                clip: Some(witness.local_contents_clip().id),
                ..Default::default()
            }
}

fn root_opacity_group_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
) -> Result<RootOpacityGroupPlan, Vec<FrameArtifactFallbackReason>> {
    let [root] = roots else {
        return Err(vec![FrameArtifactFallbackReason::RootCount(roots.len())]);
    };
    if arena.get(*root).is_none() {
        return Err(vec![FrameArtifactFallbackReason::MissingRootEffect(*root)]);
    }
    let effect = EffectNodeId(*root);
    let Some(root_effect) = property_trees.effects.get(&effect) else {
        return Err(vec![FrameArtifactFallbackReason::MissingRootEffect(*root)]);
    };
    let mut reasons = Vec::new();
    if root_effect.owner != *root
        || root_effect.parent.is_some()
        || root_effect.generation == 0
        || !root_effect.opacity.is_finite()
        || !(0.0..=1.0).contains(&root_effect.opacity)
    {
        reasons.push(FrameArtifactFallbackReason::InvalidRootEffect(*root));
    }
    for (&id, snapshot) in &property_trees.effects {
        if id != effect {
            let reason = FrameArtifactFallbackReason::NestedEffect(snapshot.owner);
            if !reasons.contains(&reason) {
                reasons.push(reason);
            }
        }
    }

    let mut stack = vec![*root];
    let mut seen = FxHashSet::default();
    while let Some(key) = stack.pop() {
        if !seen.insert(key) {
            continue;
        }
        let Some(node) = arena.get(key) else {
            continue;
        };
        if node.element.is_deferred_to_root_viewport_render() {
            reasons.push(FrameArtifactFallbackReason::DeferredBoundary(key));
        }
        match property_trees.states.get(&key) {
            Some(state) => {
                for properties in [state.paint, state.descendants] {
                    if properties.effect != Some(effect) {
                        let reason = FrameArtifactFallbackReason::NestedEffect(key);
                        if !reasons.contains(&reason) {
                            reasons.push(reason);
                        }
                    }
                    if properties.transform.is_some() || properties.scroll.is_some() {
                        let reason = FrameArtifactFallbackReason::NonEffectProperty(key);
                        if !reasons.contains(&reason) {
                            reasons.push(reason);
                        }
                    }
                }
            }
            None => reasons.push(FrameArtifactFallbackReason::MissingRootEffect(key)),
        }
        stack.extend(node.element.children().iter().copied());
    }
    reasons.sort_by_key(|reason| format!("{reason:?}"));
    reasons.dedup();
    if reasons.is_empty() {
        Ok(RootOpacityGroupPlan {
            root: *root,
            effect,
        })
    } else {
        Err(reasons)
    }
}

fn production_property_boundary_reasons(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    policy: FrameArtifactAuthorityPolicy,
) -> Vec<FrameArtifactFallbackReason> {
    let mut reasons = Vec::new();
    let mut stack = roots.to_vec();
    let mut seen = FxHashSet::default();
    while let Some(key) = stack.pop() {
        if !seen.insert(key) {
            continue;
        }
        let Some(node) = arena.get(key) else {
            continue;
        };
        if node.element.is_deferred_to_root_viewport_render() {
            let reason = FrameArtifactFallbackReason::LegacyBoundary(LegacyPaintReason::Deferred);
            if !reasons.contains(&reason) {
                reasons.push(reason);
            }
        }
        let property_boundary = property_trees.states.get(&key).is_some_and(|state| {
            [state.paint, state.descendants]
                .into_iter()
                .any(|properties| match policy {
                    FrameArtifactAuthorityPolicy::PropertyNeutral => {
                        properties != Default::default()
                    }
                    FrameArtifactAuthorityPolicy::ClipEnabled => {
                        properties.transform.is_some()
                            || properties.effect.is_some()
                            || properties.scroll.is_some()
                    }
                    FrameArtifactAuthorityPolicy::PropertyScene => {
                        properties.transform.is_some()
                            || properties.effect.is_some()
                            || properties.scroll.is_some()
                    }
                    FrameArtifactAuthorityPolicy::ExistingBakedProperties
                    | FrameArtifactAuthorityPolicy::RootOpacityGroup(_)
                    | FrameArtifactAuthorityPolicy::TransformSurface(_)
                    | FrameArtifactAuthorityPolicy::TransformPropertySurface(_)
                    | FrameArtifactAuthorityPolicy::EffectPropertySurface(_)
                    | FrameArtifactAuthorityPolicy::BakedScrollHost(_)
                    | FrameArtifactAuthorityPolicy::BakedScrollTextAreaSubtreeHost(_, _)
                    | FrameArtifactAuthorityPolicy::BakedScrollAtomicProjectionTextAreaSubtreeHost(_, _)
                    | FrameArtifactAuthorityPolicy::BakedScrollInteractiveTextAreaSubtreeHost(
                        _,
                        _,
                    )
                    | FrameArtifactAuthorityPolicy::ScrollTransformHost(_, _)
                    | FrameArtifactAuthorityPolicy::ScrollContentLocal(_)
                    | FrameArtifactAuthorityPolicy::ScrollTextAreaSubtreeLocal(_)
                    | FrameArtifactAuthorityPolicy::ScrollAtomicProjectionTextAreaSubtreeLocal(_)
                    | FrameArtifactAuthorityPolicy::ScrollInteractiveTextAreaSubtreeLocal(_) => {
                        false
                    }
                })
        });
        if property_boundary {
            reasons.push(FrameArtifactFallbackReason::PropertyBoundary(key));
        }
        stack.extend(node.element.children().iter().copied());
    }
    reasons
}

fn fallback_or_forced(
    mode: RendererMode,
    eligibility: FrameArtifactEligibility,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    match mode {
        RendererMode::StrictPlan => Err(ForcedFrameArtifactError {
            reasons: eligibility.reasons,
        }),
        #[cfg(test)]
        RendererMode::ForcedForTests => Err(ForcedFrameArtifactError {
            reasons: eligibility.reasons,
        }),
        RendererMode::Auto | RendererMode::Legacy => Ok(
            FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility),
        ),
    }
}

pub(super) fn canonical_manifest_matches(
    metadata: &super::PaintCoverageManifest,
    full: &super::PaintCoverageManifest,
) -> bool {
    if metadata.validation_errors != full.validation_errors
        || metadata.items.len() != full.items.len()
    {
        return false;
    }
    metadata
        .items
        .iter()
        .zip(&full.items)
        .all(|(left, right)| match (left, right) {
            (
                PaintCoverageItem::ArtifactChunk {
                    order: left_order,
                    chunk: left_chunk,
                    clip_snapshot: left_clip_snapshot,
                    effect_snapshot: left_effect_snapshot,
                    owner_snapshot: left_owner_snapshot,
                    ops: None,
                },
                PaintCoverageItem::ArtifactChunk {
                    order: right_order,
                    chunk: right_chunk,
                    clip_snapshot: right_clip_snapshot,
                    effect_snapshot: right_effect_snapshot,
                    owner_snapshot: right_owner_snapshot,
                    ops: Some(_),
                },
            ) => {
                left_order == right_order
                    && left_chunk.id == right_chunk.id
                    && left_chunk.owner == right_chunk.owner
                    && left_chunk.bounds.x.to_bits() == right_chunk.bounds.x.to_bits()
                    && left_chunk.bounds.y.to_bits() == right_chunk.bounds.y.to_bits()
                    && left_chunk.bounds.width.to_bits() == right_chunk.bounds.width.to_bits()
                    && left_chunk.bounds.height.to_bits() == right_chunk.bounds.height.to_bits()
                    && left_chunk.properties == right_chunk.properties
                    && left_chunk.content_revision == right_chunk.content_revision
                    && left_chunk.payload_identity == right_chunk.payload_identity
                    && left_clip_snapshot == right_clip_snapshot
                    && left_effect_snapshot == right_effect_snapshot
                    && left_owner_snapshot == right_owner_snapshot
            }
            (
                PaintCoverageItem::TransparentNode {
                    order: left_order,
                    owner: left_owner,
                    stable_id: left_stable_id,
                    properties: left_properties,
                    content_revision: left_revision,
                },
                PaintCoverageItem::TransparentNode {
                    order: right_order,
                    owner: right_owner,
                    stable_id: right_stable_id,
                    properties: right_properties,
                    content_revision: right_revision,
                },
            ) => {
                left_order == right_order
                    && left_owner == right_owner
                    && left_stable_id == right_stable_id
                    && left_properties == right_properties
                    && left_revision == right_revision
            }
            (
                PaintCoverageItem::CulledSubtree {
                    order: left_order,
                    owner: left_owner,
                    stable_id: left_stable_id,
                    properties: left_properties,
                    content_revision: left_revision,
                },
                PaintCoverageItem::CulledSubtree {
                    order: right_order,
                    owner: right_owner,
                    stable_id: right_stable_id,
                    properties: right_properties,
                    content_revision: right_revision,
                },
            ) => {
                left_order == right_order
                    && left_owner == right_owner
                    && left_stable_id == right_stable_id
                    && left_properties == right_properties
                    && left_revision == right_revision
            }
            (
                PaintCoverageItem::LegacyBoundary {
                    order: lo,
                    root: lr,
                    stable_id: ls,
                    reason: lreason,
                    span_index: lspan,
                    before_promoted: lb,
                    after_promoted: la,
                },
                PaintCoverageItem::LegacyBoundary {
                    order: ro,
                    root: rr,
                    stable_id: rs,
                    reason: rreason,
                    span_index: rspan,
                    before_promoted: rb,
                    after_promoted: ra,
                },
            ) => {
                lo == ro
                    && lr == rr
                    && ls == rs
                    && lreason == rreason
                    && lspan == rspan
                    && lb == rb
                    && la == ra
            }
            (
                PaintCoverageItem::PromotedBoundary {
                    order: lo,
                    root: lr,
                    stable_id: ls,
                },
                PaintCoverageItem::PromotedBoundary {
                    order: ro,
                    root: rr,
                    stable_id: rs,
                },
            ) => lo == ro && lr == rr && ls == rs,
            (
                PaintCoverageItem::PlannedBoundary {
                    order: left_order,
                    boundary: left_boundary,
                },
                PaintCoverageItem::PlannedBoundary {
                    order: right_order,
                    boundary: right_boundary,
                },
            ) => left_order == right_order && left_boundary == right_boundary,
            (
                PaintCoverageItem::NestedScrollContentReceiver {
                    order: left_order,
                    cutout: left_cutout,
                },
                PaintCoverageItem::NestedScrollContentReceiver {
                    order: right_order,
                    cutout: right_cutout,
                },
            ) => left_order == right_order && left_cutout == right_cutout,
            _ => false,
        })
}

#[cfg(test)]
mod nested_scroll_tests {
    use super::*;
    use crate::style::{Color, Layout, ParsedValue, PropertyId, ScrollDirection, Style};
    use crate::view::base_component::{DirtyPassMask, Element, Rect, Size};
    use crate::view::compositor::property_tree::{ClipNodeId, ClipNodeRole, ScrollNodeId};
    use crate::view::node_arena::{Node, NodeArena};

    fn install_geometry(arena: &NodeArena, key: NodeKey, rect: Rect, content: Size) {
        let mut node = arena.get_mut(key).unwrap();
        let element = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
        element.layout_state.layout_position.x = rect.x;
        element.layout_state.layout_position.y = rect.y;
        element.layout_state.layout_size = Size {
            width: rect.width,
            height: rect.height,
        };
        element.layout_state.layout_inner_position.x = rect.x;
        element.layout_state.layout_inner_position.y = rect.y;
        element.layout_state.layout_inner_size = Size {
            width: rect.width,
            height: rect.height,
        };
        element.layout_state.content_size = content;
        element.set_background_color_value(Color::rgb(24, 48, 72));
    }

    fn fixture() -> (
        NodeArena,
        NodeKey,
        NodeKey,
        NodeKey,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        let mut arena = NodeArena::new();
        let outer = arena.insert(Node::new(Box::new(Element::new_with_id(
            0x1250_00, 10.0, 20.0, 100.0, 80.0,
        ))));
        let inner = arena.insert(Node::new(Box::new(Element::new_with_id(
            0x1250_01, 10.0, 20.0, 100.0, 300.0,
        ))));
        let leaf = arena.insert(Node::new(Box::new(Element::new_with_id(
            0x1250_02, 10.0, 20.0, 100.0, 600.0,
        ))));
        arena.set_parent(inner, Some(outer));
        arena.push_child(outer, inner);
        arena.set_parent(leaf, Some(inner));
        arena.push_child(inner, leaf);
        for owner in [outer, inner] {
            let mut style = Style::new();
            style.insert(
                PropertyId::ScrollDirection,
                ParsedValue::ScrollDirection(ScrollDirection::Vertical),
            );
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            arena
                .get_mut(owner)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .unwrap()
                .apply_style(style);
        }
        install_geometry(
            &arena,
            outer,
            Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 80.0,
            },
            Size {
                width: 100.0,
                height: 300.0,
            },
        );
        install_geometry(
            &arena,
            inner,
            Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 300.0,
            },
            Size {
                width: 100.0,
                height: 600.0,
            },
        );
        install_geometry(
            &arena,
            leaf,
            Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 600.0,
            },
            Size {
                width: 100.0,
                height: 600.0,
            },
        );
        for key in [outer, inner, leaf] {
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        arena.refresh_subtree_dirty_cache(outer);
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[outer]);
        assert!(properties.validation_errors.is_empty());
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[outer], &properties);
        (arena, outer, inner, leaf, properties, generations)
    }

    #[test]
    fn nested_scroll_recorders_seal_h0_h1_receiver_o1_o0_and_two_scope_projection() {
        let (arena, outer, inner, leaf, properties, generations) = fixture();
        let admission = arena
            .get(outer)
            .unwrap()
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap()
            .exact_retained_nested_scroll_scene_admission(outer, &arena, 1.0)
            .expect("exact nested admission");
        assert_eq!(admission.inner_boundary_root, inner);
        assert_eq!(admission.content_leaf, leaf);

        let outer_scroll = properties.scroll_snapshot_for(ScrollNodeId(outer)).unwrap();
        let inner_scroll = properties.scroll_snapshot_for(ScrollNodeId(inner)).unwrap();
        let outer_clip_id = ClipNodeId {
            owner: outer,
            role: ClipNodeRole::ContentsClip,
        };
        let inner_clip_id = ClipNodeId {
            owner: inner,
            role: ClipNodeRole::ContentsClip,
        };
        let outer_clip = properties.clip_snapshot_for(Some(outer_clip_id)).unwrap()[0];
        let inner_clip = properties.clip_snapshot_for(Some(inner_clip_id)).unwrap()[0];
        let outer_state = crate::view::compositor::property_tree::PropertyTreeState {
            clip: Some(outer_clip_id),
            scroll: Some(outer_scroll.id),
            ..Default::default()
        };
        let inner_state = crate::view::compositor::property_tree::PropertyTreeState {
            clip: Some(inner_clip_id),
            scroll: Some(inner_scroll.id),
            ..Default::default()
        };
        assert_eq!(properties.states[&inner].paint, outer_state);
        assert_eq!(properties.states[&leaf].paint, inner_state);

        let outer_host =
            PaintBakedScrollHostWitness::new(outer, inner, outer_scroll, outer_clip_id).unwrap();
        let inner_cutout = super::super::PlannedBoundary {
            root: inner,
            stable_id: admission.inner_stable_id,
            kind: super::super::PlannedBoundaryKind::Scroll(inner_scroll.id),
        };
        let outer_steps = record_nested_scroll_outer_host_steps_for_plan(
            &arena,
            outer,
            &FxHashSet::default(),
            &properties,
            &generations,
            outer_host,
            inner_cutout,
        )
        .expect("H0-S1-O0");
        assert!(matches!(
            outer_steps.as_slice(),
            [
                RecordedTransformSurfaceStep::Artifact(_),
                RecordedTransformSurfaceStep::Boundary(found),
                RecordedTransformSurfaceStep::Artifact(_),
            ] if *found == inner_cutout
        ));

        let outer_content =
            PaintScrollContentWitness::new(outer, inner, outer_scroll, outer_clip).unwrap();
        let inner_host =
            PaintBakedScrollHostWitness::new(inner, leaf, inner_scroll, inner_clip_id).unwrap();
        let content = PaintNestedScrollContentWitness::new(
            outer,
            inner,
            leaf,
            outer_scroll,
            outer_clip,
            inner_scroll,
            inner_clip,
        )
        .unwrap();
        assert!(
            PaintNestedScrollContentWitness::new(
                outer,
                inner,
                outer,
                outer_scroll,
                outer_clip,
                inner_scroll,
                inner_clip,
            )
            .is_none(),
            "outer/content alias must not mint a nested chain witness"
        );
        let inner_steps = record_nested_scroll_inner_host_steps_for_plan(
            &arena,
            inner,
            &FxHashSet::default(),
            &properties,
            &generations,
            inner_host,
            outer_content,
            admission.content_leaf_stable_id,
            content,
        )
        .expect("H1-receiver-O1");
        assert!(matches!(
            inner_steps.as_slice(),
            [
                RecordedNestedScrollHostStep::Artifact(_),
                RecordedNestedScrollHostStep::ContentReceiver(_),
                RecordedNestedScrollHostStep::Artifact(_),
            ]
        ));

        let artifact = record_nested_scroll_content_artifact_for_plan(
            &arena,
            &FxHashSet::default(),
            &properties,
            &generations,
            content,
        )
        .expect("S1/C1 projects to S0/C0 in the leaf scope");
        assert!(!artifact.chunks.is_empty());
        assert!(
            artifact
                .chunks
                .iter()
                .all(|chunk| chunk.owner == leaf && chunk.properties == outer_state)
        );
    }

    #[test]
    fn nested_scroll_receiver_and_parent_chain_tamper_fail_closed() {
        let (mut arena, outer, inner, leaf, mut properties, generations) = fixture();
        let outer_scroll = properties.scroll_snapshot_for(ScrollNodeId(outer)).unwrap();
        let inner_scroll = properties.scroll_snapshot_for(ScrollNodeId(inner)).unwrap();
        let outer_clip_id = ClipNodeId {
            owner: outer,
            role: ClipNodeRole::ContentsClip,
        };
        let inner_clip_id = ClipNodeId {
            owner: inner,
            role: ClipNodeRole::ContentsClip,
        };
        let outer_clip = properties.clip_snapshot_for(Some(outer_clip_id)).unwrap()[0];
        let inner_clip = properties.clip_snapshot_for(Some(inner_clip_id)).unwrap()[0];
        let content = PaintNestedScrollContentWitness::new(
            outer,
            inner,
            leaf,
            outer_scroll,
            outer_clip,
            inner_scroll,
            inner_clip,
        )
        .unwrap();
        let inner_host =
            PaintBakedScrollHostWitness::new(inner, leaf, inner_scroll, inner_clip_id).unwrap();
        let outer_content =
            PaintScrollContentWitness::new(outer, inner, outer_scroll, outer_clip).unwrap();

        assert!(
            record_nested_scroll_inner_host_steps_for_plan(
                &arena,
                inner,
                &FxHashSet::default(),
                &properties,
                &generations,
                inner_host,
                outer_content,
                0,
                content,
            )
            .is_err()
        );

        arena.set_parent(leaf, Some(outer));
        assert!(
            record_nested_scroll_inner_host_steps_for_plan(
                &arena,
                inner,
                &FxHashSet::default(),
                &properties,
                &generations,
                inner_host,
                outer_content,
                arena.get(leaf).unwrap().element.stable_id(),
                content,
            )
            .is_err()
        );
        arena.set_parent(leaf, Some(inner));

        properties
            .scrolls
            .get_mut(&ScrollNodeId(inner))
            .unwrap()
            .parent = None;
        assert!(
            record_nested_scroll_content_artifact_for_plan(
                &arena,
                &FxHashSet::default(),
                &properties,
                &generations,
                content,
            )
            .is_err()
        );

        for drift in 0..5 {
            let (arena, outer, inner, leaf, mut properties, generations) = fixture();
            let outer_scroll = properties.scroll_snapshot_for(ScrollNodeId(outer)).unwrap();
            let inner_scroll = properties.scroll_snapshot_for(ScrollNodeId(inner)).unwrap();
            let outer_clip_id = ClipNodeId {
                owner: outer,
                role: ClipNodeRole::ContentsClip,
            };
            let inner_clip_id = ClipNodeId {
                owner: inner,
                role: ClipNodeRole::ContentsClip,
            };
            let outer_clip = properties.clip_snapshot_for(Some(outer_clip_id)).unwrap()[0];
            let inner_clip = properties.clip_snapshot_for(Some(inner_clip_id)).unwrap()[0];
            let content = PaintNestedScrollContentWitness::new(
                outer,
                inner,
                leaf,
                outer_scroll,
                outer_clip,
                inner_scroll,
                inner_clip,
            )
            .unwrap();
            match drift {
                0 => properties.clips.get_mut(&inner_clip_id).unwrap().parent = None,
                1 => properties.clips.get_mut(&inner_clip_id).unwrap().owner = outer,
                2 => properties.clips.get_mut(&inner_clip_id).unwrap().generation = 0,
                3 => {
                    properties
                        .scrolls
                        .get_mut(&ScrollNodeId(inner))
                        .unwrap()
                        .owner = outer
                }
                4 => {
                    properties
                        .scrolls
                        .get_mut(&ScrollNodeId(inner))
                        .unwrap()
                        .generation = 0
                }
                _ => unreachable!(),
            }
            assert!(
                record_nested_scroll_content_artifact_for_plan(
                    &arena,
                    &FxHashSet::default(),
                    &properties,
                    &generations,
                    content,
                )
                .is_err(),
                "live nested property drift case {drift} must fail closed"
            );
        }
    }
}

#[cfg(test)]
mod scroll_host_tests {
    use super::*;
    use crate::style::{Layout, ParsedValue, PropertyId, ScrollDirection, Style};
    use crate::view::base_component::{DirtyPassMask, Element, ElementTrait, EventTarget, Size};
    use crate::view::compositor::property_tree::{ClipNodeId, ClipNodeRole, ScrollNodeId};
    use crate::view::node_arena::{Node, NodeArena};
    use crate::view::paint::{PaintOp, PaintPayloadIdentity};

    fn fixture_with_scrollbar(
        hovered: bool,
        shadow_blur_radius: f32,
    ) -> (
        NodeArena,
        NodeKey,
        NodeKey,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(Element::new_with_id(
            81_001, 0.0, 0.0, 100.0, 80.0,
        ))));
        let child = arena.insert(Node::new(Box::new(Element::new_with_id(
            81_002, 0.0, -20.0, 100.0, 300.0,
        ))));
        arena.set_parent(child, Some(root));
        arena.push_child(root, child);
        let mut style = Style::new();
        style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        {
            let mut root_node = arena.get_mut(root).unwrap();
            let root_element = root_node
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .unwrap();
            root_element.apply_style(style);
            root_element.layout_state.content_size = Size {
                width: 100.0,
                height: 300.0,
            };
            root_element.set_scroll_offset((0.0, 20.0));
            root_element.set_scrollbar_shadow_blur_radius(shadow_blur_radius);
            root_element.set_hovered(hovered);
            root_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        arena
            .get_mut(child)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        arena.refresh_subtree_dirty_cache(root);
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        assert!(
            properties.validation_errors.is_empty(),
            "unexpected offset fixture property errors: {:?}",
            properties.validation_errors
        );
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        (arena, root, child, properties, generations)
    }

    fn fixture() -> (
        NodeArena,
        NodeKey,
        NodeKey,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        fixture_with_scrollbar(false, 3.0)
    }

    fn opaque_fixture() -> (
        NodeArena,
        NodeKey,
        NodeKey,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        fixture_with_scrollbar(true, 3.0)
    }

    fn translucent_fixture() -> (
        NodeArena,
        NodeKey,
        NodeKey,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        let (arena, root, child, _, _) = fixture();
        {
            let mut root_node = arena.get_mut(root).unwrap();
            let root_element = root_node
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .unwrap();
            root_element.set_hovered(true);
            root_element.set_hovered(false);
            let sampled_at = crate::time::Instant::now();
            let _ = root_element.tick_post_layout_animation_frame(sampled_at);
            let _ = root_element.tick_post_layout_animation_frame(
                sampled_at + crate::time::Duration::from_millis(1_000),
            );
            root_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        arena.refresh_subtree_dirty_cache(root);
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        assert!(
            properties.validation_errors.is_empty(),
            "unexpected offset fixture property errors: {:?}",
            properties.validation_errors
        );
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        (arena, root, child, properties, generations)
    }

    fn content_witness(
        root: NodeKey,
        child: NodeKey,
        properties: &PropertyTrees,
    ) -> PaintScrollContentWitness {
        let scroll = properties.scroll_snapshot_for(ScrollNodeId(root)).unwrap();
        let clip_id = ClipNodeId {
            owner: root,
            role: ClipNodeRole::ContentsClip,
        };
        let clip = properties
            .clip_snapshot_for(Some(clip_id))
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        PaintScrollContentWitness::new(root, child, scroll, clip).unwrap()
    }

    fn fixture_at_offset(
        offset: [f32; 2],
    ) -> (
        NodeArena,
        NodeKey,
        NodeKey,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(Element::new_with_id(
            81_101, 0.0, 0.0, 100.0, 80.0,
        ))));
        let child = arena.insert(Node::new(Box::new(Element::new_with_id(
            81_102, -offset[0], -offset[1], 300.0, 300.0,
        ))));
        arena.set_parent(child, Some(root));
        arena.push_child(root, child);
        let mut style = Style::new();
        style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        {
            let mut root_node = arena.get_mut(root).unwrap();
            let root_element = root_node
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .unwrap();
            root_element.apply_style(style);
            root_element.layout_state.content_size = Size {
                width: 300.0,
                height: 300.0,
            };
            root_element.set_scroll_offset((offset[0], offset[1]));
            root_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        {
            let mut child_node = arena.get_mut(child).unwrap();
            child_node
                .element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        arena.refresh_subtree_dirty_cache(root);
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        assert!(
            properties.validation_errors.is_empty(),
            "unexpected offset fixture property errors: {:?}",
            properties.validation_errors
        );
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        (arena, root, child, properties, generations)
    }

    #[test]
    fn scroll_content_recorder_detaches_child_and_neutralizes_scroll_clip_atomically() {
        let (arena, root, child, properties, generations) = fixture();
        let artifact = record_scroll_content_local_artifact_for_plan(
            &arena,
            &Default::default(),
            &properties,
            &generations,
            content_witness(root, child, &properties),
        )
        .unwrap();

        assert!(!artifact.chunks.is_empty());
        assert!(artifact.chunks.iter().all(|chunk| {
            chunk.owner == child
                && chunk.properties == Default::default()
                && chunk.id.role != super::super::PaintChunkRole::ScrollbarOverlay
        }));
        assert!(artifact.clip_nodes.is_empty());
        assert!(artifact.effect_nodes.is_empty());
        assert_eq!(
            artifact.owner_nodes,
            vec![super::super::PaintOwnerSnapshot {
                owner: child,
                parent: None,
            }]
        );
    }

    #[test]
    fn scroll_content_recorder_normalizes_the_complete_two_dimensional_offset() {
        fn assert_rect_params_bitwise_eq(
            left: &crate::view::render_pass::draw_rect_pass::RectPassParams,
            right: &crate::view::render_pass::draw_rect_pass::RectPassParams,
        ) {
            assert_eq!(
                left.position.map(f32::to_bits),
                right.position.map(f32::to_bits)
            );
            assert_eq!(left.size.map(f32::to_bits), right.size.map(f32::to_bits));
            assert_eq!(
                left.fill_color.map(f32::to_bits),
                right.fill_color.map(f32::to_bits)
            );
            assert_eq!(left.opacity.to_bits(), right.opacity.to_bits());
            assert_eq!(
                left.border_widths.map(f32::to_bits),
                right.border_widths.map(f32::to_bits)
            );
            assert_eq!(
                left.border_radii.map(|radius| radius.map(f32::to_bits)),
                right.border_radii.map(|radius| radius.map(f32::to_bits))
            );
            assert_eq!(
                left.border_color.map(f32::to_bits),
                right.border_color.map(f32::to_bits)
            );
            assert_eq!(
                left.border_side_colors.map(|color| color.map(f32::to_bits)),
                right
                    .border_side_colors
                    .map(|color| color.map(f32::to_bits))
            );
            assert_eq!(left.use_border_side_colors, right.use_border_side_colors);
            assert_eq!(left.depth.to_bits(), right.depth.to_bits());
            for (left, right) in [
                (left.gradient.as_ref(), right.gradient.as_ref()),
                (
                    left.border_gradient.as_ref(),
                    right.border_gradient.as_ref(),
                ),
            ] {
                match (left, right) {
                    (None, None) => {}
                    (Some(left), Some(right)) => {
                        assert_eq!(left.kind, right.kind);
                        assert_eq!(left.axis.map(f32::to_bits), right.axis.map(f32::to_bits));
                        assert_eq!(left.repeating, right.repeating);
                        assert_eq!(left.stops.len(), right.stops.len());
                        for (left, right) in left.stops.iter().zip(right.stops.iter()) {
                            assert_eq!(left.color.map(f32::to_bits), right.color.map(f32::to_bits));
                            assert_eq!(left.pos.map(f32::to_bits), right.pos.map(f32::to_bits));
                        }
                    }
                    _ => panic!("normalized rect gradient presence changed"),
                }
            }
        }

        fn assert_ops_bitwise_eq(left: &PaintOp, right: &PaintOp) {
            match (left, right) {
                (PaintOp::DrawRect(left), PaintOp::DrawRect(right)) => {
                    assert_eq!(left.mode, right.mode);
                    assert_rect_params_bitwise_eq(&left.params, &right.params);
                }
                (PaintOp::PreparedShadow(left), PaintOp::PreparedShadow(right)) => {
                    assert_eq!(
                        left.mesh
                            .vertices
                            .iter()
                            .map(|point| point.map(f32::to_bits))
                            .collect::<Vec<_>>(),
                        right
                            .mesh
                            .vertices
                            .iter()
                            .map(|point| point.map(f32::to_bits))
                            .collect::<Vec<_>>()
                    );
                    assert_eq!(left.mesh.indices, right.mesh.indices);
                    assert_eq!(
                        left.params.offset_x.to_bits(),
                        right.params.offset_x.to_bits()
                    );
                    assert_eq!(
                        left.params.offset_y.to_bits(),
                        right.params.offset_y.to_bits()
                    );
                    assert_eq!(
                        left.params.blur_radius.to_bits(),
                        right.params.blur_radius.to_bits()
                    );
                    assert_eq!(
                        left.params.color.map(f32::to_bits),
                        right.params.color.map(f32::to_bits)
                    );
                    assert_eq!(
                        left.params.opacity.to_bits(),
                        right.params.opacity.to_bits()
                    );
                    assert_eq!(left.params.spread.to_bits(), right.params.spread.to_bits());
                    assert_eq!(left.params.clip_to_geometry, right.params.clip_to_geometry);
                }
                _ => panic!("normalized scroll-content op kind changed"),
            }
        }

        let (zero_arena, zero_root, zero_child, zero_properties, zero_generations) =
            fixture_at_offset([0.0, 0.0]);
        let zero = record_scroll_content_local_artifact_for_plan(
            &zero_arena,
            &Default::default(),
            &zero_properties,
            &zero_generations,
            content_witness(zero_root, zero_child, &zero_properties),
        )
        .unwrap();
        let (moved_arena, moved_root, moved_child, moved_properties, moved_generations) =
            fixture_at_offset([3.5, 47.25]);
        let moved = record_scroll_content_local_artifact_for_plan(
            &moved_arena,
            &Default::default(),
            &moved_properties,
            &moved_generations,
            content_witness(moved_root, moved_child, &moved_properties),
        )
        .unwrap();
        let (
            negative_zero_arena,
            negative_zero_root,
            negative_zero_child,
            negative_zero_properties,
            negative_zero_generations,
        ) = fixture_at_offset([-0.0, -0.0]);
        let negative_zero = record_scroll_content_local_artifact_for_plan(
            &negative_zero_arena,
            &Default::default(),
            &negative_zero_properties,
            &negative_zero_generations,
            content_witness(
                negative_zero_root,
                negative_zero_child,
                &negative_zero_properties,
            ),
        )
        .unwrap();

        for candidate in [&moved, &negative_zero] {
            assert_eq!(zero.chunks.len(), candidate.chunks.len());
            for (zero, candidate) in zero.chunks.iter().zip(&candidate.chunks) {
                assert_eq!(
                    [
                        zero.bounds.x,
                        zero.bounds.y,
                        zero.bounds.width,
                        zero.bounds.height
                    ]
                    .map(f32::to_bits),
                    [
                        candidate.bounds.x,
                        candidate.bounds.y,
                        candidate.bounds.width,
                        candidate.bounds.height,
                    ]
                    .map(f32::to_bits)
                );
                assert_eq!(zero.payload_identity, candidate.payload_identity);
            }
            assert_eq!(zero.ops.len(), candidate.ops.len());
            assert!(!zero.ops.is_empty());
            for (zero, candidate) in zero.ops.iter().zip(&candidate.ops) {
                assert_ops_bitwise_eq(zero, candidate);
            }
        }
    }

    #[test]
    fn scroll_content_recorder_rejects_retargeted_edge_and_inline_ifc_leaf() {
        let (mut arena, root, child, properties, generations) = fixture();
        let witness = content_witness(root, child, &properties);
        let other_parent = arena.insert(Node::new(Box::new(Element::new_with_id(
            81_103, 0.0, 0.0, 1.0, 1.0,
        ))));
        arena.set_parent(child, Some(other_parent));
        assert_eq!(arena.get(root).unwrap().element.children(), [child]);
        assert!(
            record_scroll_content_local_artifact_for_plan(
                &arena,
                &Default::default(),
                &properties,
                &generations,
                witness,
            )
            .is_err()
        );

        let (arena, root, child, properties, generations) = fixture();
        let witness = content_witness(root, child, &properties);
        let mut inline_style = Style::new();
        inline_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        inline_style.insert(PropertyId::Width, ParsedValue::Auto);
        inline_style.insert(PropertyId::Height, ParsedValue::Auto);
        let mut child_node = arena.get_mut(child).unwrap();
        let child_element = child_node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap();
        child_element.apply_style(inline_style);
        child_element.layout_state.layout_size = Size {
            width: 0.0,
            height: 0.0,
        };
        drop(child_node);
        assert!(
            record_scroll_content_local_artifact_for_plan(
                &arena,
                &Default::default(),
                &properties,
                &generations,
                witness,
            )
            .is_err()
        );
    }

    #[test]
    fn scroll_content_recorder_rejects_half_property_mismatch_wrong_parent_and_nonfinite_offset() {
        let (arena, root, child, mut properties, generations) = fixture();
        let witness = content_witness(root, child, &properties);
        properties.states.get_mut(&child).unwrap().paint.clip = None;
        assert!(
            record_scroll_content_local_artifact_for_plan(
                &arena,
                &Default::default(),
                &properties,
                &generations,
                witness,
            )
            .is_err()
        );

        let (mut arena, root, child, properties, generations) = fixture();
        let witness = content_witness(root, child, &properties);
        arena.set_parent(child, None);
        assert!(
            record_scroll_content_local_artifact_for_plan(
                &arena,
                &Default::default(),
                &properties,
                &generations,
                witness,
            )
            .is_err()
        );

        let (arena, root, child, properties, _) = fixture();
        let mut scroll = properties.scroll_snapshot_for(ScrollNodeId(root)).unwrap();
        scroll.offset.x = f32::NAN;
        let clip_id = ClipNodeId {
            owner: root,
            role: ClipNodeRole::ContentsClip,
        };
        let clip = properties
            .clip_snapshot_for(Some(clip_id))
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        assert!(PaintScrollContentWitness::new(root, child, scroll, clip).is_none());
        drop(arena);
    }

    #[test]
    fn baked_scroll_host_recorder_preserves_order_properties_and_empty_overlay_parity() {
        let (arena, root, child, properties, generations) = fixture();
        let scroll = ScrollNodeId(root);
        let clip = ClipNodeId {
            owner: root,
            role: ClipNodeRole::ContentsClip,
        };
        let scroll_snapshot = properties.scroll_snapshot_for(scroll).unwrap();
        let witness = PaintBakedScrollHostWitness::new(root, child, scroll_snapshot, clip).unwrap();
        let artifact = record_baked_scroll_host_artifact_for_plan(
            &arena,
            &[root],
            &Default::default(),
            &properties,
            &generations,
            witness,
        )
        .unwrap();

        assert_eq!(artifact.chunks.len(), 3);
        assert_eq!(artifact.chunks[0].owner, root);
        assert_eq!(
            artifact.chunks[0].id.phase,
            super::super::PaintNodePhase::BeforeChildren
        );
        assert_eq!(artifact.chunks[0].properties, Default::default());
        assert_eq!(artifact.chunks[1].owner, child);
        assert_eq!(artifact.chunks[1].properties.scroll, Some(scroll));
        assert_eq!(artifact.chunks[1].properties.clip, Some(clip));
        let overlay = &artifact.chunks[2];
        assert_eq!(overlay.owner, root);
        assert_eq!(
            overlay.id.phase,
            super::super::PaintNodePhase::AfterChildren
        );
        assert_eq!(
            overlay.id.role,
            super::super::PaintChunkRole::ScrollbarOverlay
        );
        assert_eq!(overlay.properties, Default::default());
        assert!(overlay.op_range.is_empty());
        assert_eq!(overlay.op_range.start, artifact.ops.len());
    }

    #[test]
    fn opaque_scrollbar_recorder_freezes_one_exact_legacy_order_overlay() {
        let (arena, root, child, properties, generations) = opaque_fixture();
        let scroll_id = ScrollNodeId(root);
        let clip_id = ClipNodeId {
            owner: root,
            role: ClipNodeRole::ContentsClip,
        };
        let scroll = properties.scroll_snapshot_for(scroll_id).unwrap();
        let contents_clip = properties
            .clip_snapshot_for(Some(clip_id))
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let witness = PaintBakedScrollHostWitness::new(root, child, scroll, clip_id).unwrap();
        let artifact = record_baked_scroll_host_artifact_for_plan(
            &arena,
            &[root],
            &Default::default(),
            &properties,
            &generations,
            witness,
        )
        .unwrap();

        let overlay_chunk = artifact.chunks.last().unwrap();
        assert_eq!(overlay_chunk.op_range.len(), 1);
        let [PaintOp::PreparedScrollbarOverlay(overlay)] =
            &artifact.ops[overlay_chunk.op_range.clone()]
        else {
            panic!("opaque overlay must remain one typed op");
        };
        assert!(overlay.matches_vertical_witness(scroll.scrollbar_overlay));
        assert_eq!(
            overlay_chunk.payload_identity,
            PaintPayloadIdentity::prepared_scrollbar_overlay(overlay)
        );
        assert_eq!(
            overlay.track_shadow.params.color[3].to_bits(),
            0.5_f32.to_bits()
        );
        assert_eq!(
            overlay.track.params.fill_color[3].to_bits(),
            0.35_f32.to_bits()
        );
        assert_eq!(
            overlay.thumb_shadow.params.color[3].to_bits(),
            0.5_f32.to_bits()
        );
        assert_eq!(
            overlay.thumb.params.fill_color[3].to_bits(),
            0.58_f32.to_bits()
        );
        assert!(
            super::super::compiler::validate_baked_scroll_host_artifact(
                &artifact,
                root,
                child,
                scroll,
                contents_clip,
            )
            .is_some()
        );
        let mut wrong_geometry = scroll;
        wrong_geometry
            .scrollbar_overlay
            .vertical_track
            .as_mut()
            .unwrap()
            .x += 1.0;
        assert!(
            super::super::compiler::validate_baked_scroll_host_artifact(
                &artifact,
                root,
                child,
                wrong_geometry,
                contents_clip,
            )
            .is_none()
        );

        let validates = |artifact: &PaintArtifact| {
            super::super::compiler::validate_baked_scroll_host_artifact(
                artifact,
                root,
                child,
                scroll,
                contents_clip,
            )
            .is_some()
        };
        let mut malicious = artifact.clone();
        let PaintOp::PreparedScrollbarOverlay(overlay) =
            &mut malicious.ops[overlay_chunk.op_range.start]
        else {
            unreachable!()
        };
        overlay.track_shadow.params.blur_radius += 1.0;
        assert!(!validates(&malicious));

        malicious = artifact.clone();
        let PaintOp::PreparedScrollbarOverlay(overlay) =
            &mut malicious.ops[overlay_chunk.op_range.start]
        else {
            unreachable!()
        };
        std::mem::swap(&mut overlay.track.params, &mut overlay.thumb.params);
        assert!(!validates(&malicious));

        malicious = artifact.clone();
        malicious.chunks.last_mut().unwrap().payload_identity =
            PaintPayloadIdentity::prepared_shadows(std::iter::empty());
        assert!(!validates(&malicious));

        malicious = artifact.clone();
        let extra = malicious.ops[overlay_chunk.op_range.start].clone();
        malicious.ops.push(extra);
        malicious.chunks.last_mut().unwrap().op_range.end += 1;
        assert!(!validates(&malicious));

        malicious = artifact.clone();
        malicious.ops.clear();
        malicious.chunks.last_mut().unwrap().op_range = 0..0;
        assert!(!validates(&malicious));

        malicious = artifact.clone();
        let PaintOp::PreparedScrollbarOverlay(overlay) =
            malicious.ops[overlay_chunk.op_range.start].clone()
        else {
            unreachable!()
        };
        malicious.ops[overlay_chunk.op_range.start] = PaintOp::DrawRect(overlay.track);
        assert!(!validates(&malicious));

        for index in 0..artifact.chunks.len() {
            malicious = artifact.clone();
            malicious.chunks[index].id.slot = 1;
            assert!(!validates(&malicious), "chunk {index} slot drift must fail");

            malicious = artifact.clone();
            malicious.chunks[index].id.scope = super::super::PaintPropertyScope::Contents;
            assert!(
                !validates(&malicious),
                "chunk {index} scope drift must fail"
            );
        }
    }

    #[test]
    fn opaque_scrollbar_reuses_stable_stamp_and_blur_drift_rerasterizes() {
        let prepare = |blur_radius| {
            let (arena, root, _child, properties, generations) =
                fixture_with_scrollbar(true, blur_radius);
            let plan = super::super::plan_single_root_scroll_host_surface(
                &arena,
                &[root],
                &Default::default(),
                &properties,
                &generations,
                1.0,
                [0.0; 2],
                None,
            )
            .unwrap();
            let graph = crate::view::frame_graph::FrameGraph::new();
            let ctx = crate::view::base_component::UiBuildContext::new(
                100,
                80,
                wgpu::TextureFormat::Rgba8Unorm,
                1.0,
            );
            super::super::prepare_retained_scroll_host_stamp_for_test(&plan, &graph, &ctx).unwrap()
        };
        let baseline = prepare(3.0);
        let drifted = prepare(7.0);
        assert!(super::super::retained_surface_raster_stamp_is_canonical(
            &baseline
        ));
        assert!(super::super::retained_surface_raster_stamp_is_canonical(
            &drifted
        ));
        for index in 0..baseline.chunks.len() {
            let mut malicious = baseline.clone();
            malicious.chunks[index].id.slot = 1;
            let [super::super::RetainedSurfaceRasterStepStamp::ArtifactSpan(span)] =
                malicious.ordered_steps.as_mut_slice()
            else {
                panic!("scroll host stamp must have one artifact span");
            };
            span.chunks[index].id.slot = 1;
            assert!(!super::super::retained_surface_raster_stamp_is_canonical(
                &malicious
            ));

            let mut malicious = baseline.clone();
            malicious.chunks[index].id.scope = super::super::PaintPropertyScope::Contents;
            let [super::super::RetainedSurfaceRasterStepStamp::ArtifactSpan(span)] =
                malicious.ordered_steps.as_mut_slice()
            else {
                panic!("scroll host stamp must have one artifact span");
            };
            span.chunks[index].id.scope = super::super::PaintPropertyScope::Contents;
            assert!(!super::super::retained_surface_raster_stamp_is_canonical(
                &malicious
            ));
        }
        assert_eq!(
            crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
                baseline.clone(),
                &baseline,
            ),
            super::super::RetainedSurfaceCompileAction::Reuse
        );
        assert_eq!(
            crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
                baseline, &drifted,
            ),
            super::super::RetainedSurfaceCompileAction::Reraster
        );
    }

    #[test]
    fn translucent_scrollbar_freezes_exact_sampled_alpha_into_typed_overlay() {
        let (arena, root, child, properties, generations) = translucent_fixture();
        let scroll_id = ScrollNodeId(root);
        let clip_id = ClipNodeId {
            owner: root,
            role: ClipNodeRole::ContentsClip,
        };
        let scroll = properties.scroll_snapshot_for(scroll_id).unwrap();
        let contents_clip = properties
            .clip_snapshot_for(Some(clip_id))
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let alpha = scroll.scrollbar_overlay.sampled_alpha;
        assert!((0.0..1.0).contains(&alpha));

        let witness = PaintBakedScrollHostWitness::new(root, child, scroll, clip_id).unwrap();
        let artifact = record_baked_scroll_host_artifact_for_plan(
            &arena,
            &[root],
            &Default::default(),
            &properties,
            &generations,
            witness,
        )
        .unwrap();
        let [PaintOp::PreparedScrollbarOverlay(overlay)] =
            &artifact.ops[artifact.chunks.last().unwrap().op_range.clone()]
        else {
            panic!("translucent overlay must remain one typed op");
        };
        assert!(overlay.matches_vertical_witness(scroll.scrollbar_overlay));
        assert_eq!(
            overlay.track_shadow.params.color[3].to_bits(),
            (0.5 * alpha).to_bits()
        );
        assert_eq!(
            overlay.track.params.fill_color[3].to_bits(),
            (0.35 * alpha).to_bits()
        );
        assert_eq!(
            overlay.thumb.params.fill_color[3].to_bits(),
            (0.58 * alpha).to_bits()
        );
        assert!(
            super::super::compiler::validate_baked_scroll_host_artifact(
                &artifact,
                root,
                child,
                scroll,
                contents_clip,
            )
            .is_some()
        );
    }

    #[test]
    fn general_recorder_still_rejects_scroll_host_without_owned_witness() {
        let (arena, root, _child, properties, generations) = fixture();
        let error = record_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::StrictPlan,
        )
        .unwrap_err();
        assert!(error.reasons.iter().any(|reason| matches!(
            reason,
            FrameArtifactFallbackReason::LegacyBoundary(
                LegacyPaintReason::ScrollContainer | LegacyPaintReason::ChildClip
            )
        )));
    }

    #[test]
    fn scroll_host_planner_freezes_matching_live_and_property_payloads() {
        let (arena, root, child, properties, generations) = fixture();
        let plan = super::super::plan_single_root_scroll_host_surface(
            &arena,
            &[root],
            &Default::default(),
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
        )
        .expect("exact scroll fixture must plan");
        let [super::super::PaintPlanStep::RetainedSurface(surface)] = plan.steps() else {
            panic!("scroll plan must contain one retained surface");
        };
        let super::super::SurfaceKind::ScrollHost(scroll_plan) = surface.kind() else {
            panic!("scroll plan must retain the dedicated surface kind");
        };
        assert_eq!(scroll_plan.admission.child, child);
        assert!(
            scroll_plan
                .admission
                .matches_scroll_node(scroll_plan.scroll)
        );

        let graph = crate::view::frame_graph::FrameGraph::new();
        let ctx = crate::view::base_component::UiBuildContext::new(
            100,
            80,
            wgpu::TextureFormat::Rgba8Unorm,
            1.0,
        );
        let stamp = super::super::prepare_retained_scroll_host_stamp_for_test(&plan, &graph, &ctx)
            .expect("typed scroll plan must prepare before graph mutation");
        assert_eq!(
            stamp.identity.role,
            super::super::RetainedSurfaceRasterRole::ScrollHost
        );
        assert_eq!(stamp.scroll_host.unwrap().scroll, scroll_plan.scroll);

        let mut offset_ctx = crate::view::base_component::UiBuildContext::new(
            100,
            80,
            wgpu::TextureFormat::Rgba8Unorm,
            1.0,
        );
        offset_ctx.set_paint_offset([0.25, 0.0]);
        let untouched_graph = crate::view::frame_graph::FrameGraph::new();
        assert!(
            super::super::prepare_retained_scroll_host_stamp_for_test(
                &plan,
                &untouched_graph,
                &offset_ctx,
            )
            .is_err(),
            "prepare must independently reject nonzero incoming paint snap"
        );
    }

    #[test]
    fn scroll_host_planner_rejects_live_property_race_and_opaque_scrollbar() {
        let (arena, root, _child, mut properties, generations) = fixture();
        properties
            .scrolls
            .get_mut(&ScrollNodeId(root))
            .unwrap()
            .offset
            .y += 1.0;
        assert!(
            super::super::plan_single_root_scroll_host_surface(
                &arena,
                &[root],
                &Default::default(),
                &properties,
                &generations,
                1.0,
                [0.0; 2],
                None,
            )
            .is_err()
        );

        let (arena, root, _child, mut properties, generations) = fixture();
        properties
            .scrolls
            .get_mut(&ScrollNodeId(root))
            .unwrap()
            .scrollbar_overlay
            .paint_state = crate::view::base_component::ScrollbarPaintStateWitness::OpaqueNow;
        assert!(
            super::super::plan_single_root_scroll_host_surface(
                &arena,
                &[root],
                &Default::default(),
                &properties,
                &generations,
                1.0,
                [0.0; 2],
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn scroll_host_planner_accepts_frame_sampled_translucent_scrollbar() {
        let (arena, root, _child, properties, generations) = translucent_fixture();
        assert_eq!(
            properties
                .scroll_snapshot_for(ScrollNodeId(root))
                .unwrap()
                .scrollbar_overlay
                .paint_state,
            crate::view::base_component::ScrollbarPaintStateWitness::TranslucentNow
        );
        assert!(
            super::super::plan_single_root_scroll_host_surface(
                &arena,
                &[root],
                &Default::default(),
                &properties,
                &generations,
                1.0,
                [0.0; 2],
                None,
            )
            .is_ok()
        );
    }

    #[test]
    fn scroll_offset_is_an_explicit_raster_stamp_dependency() {
        let (arena, root, _child, properties, generations) = fixture();
        let plan = super::super::plan_single_root_scroll_host_surface(
            &arena,
            &[root],
            &Default::default(),
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
        )
        .unwrap();
        let graph = crate::view::frame_graph::FrameGraph::new();
        let ctx = crate::view::base_component::UiBuildContext::new(
            100,
            80,
            wgpu::TextureFormat::Rgba8Unorm,
            1.0,
        );
        let baseline =
            super::super::prepare_retained_scroll_host_stamp_for_test(&plan, &graph, &ctx).unwrap();
        let mut offset_only = baseline.clone();
        let dependency = offset_only.scroll_host.as_mut().unwrap();
        dependency.scroll.offset.y += 1.0;
        let (track, thumb) = crate::view::base_component::canonical_vertical_scrollbar_geometry(
            dependency.scroll.viewport,
            dependency.scroll.content_size.height,
            dependency.scroll.offset.y,
            false,
        )
        .unwrap();
        dependency.scroll.scrollbar_overlay.vertical_track = Some(track);
        dependency.scroll.scrollbar_overlay.vertical_thumb = Some(thumb);
        assert_ne!(baseline, offset_only);
        assert!(super::super::retained_surface_raster_stamp_is_canonical(
            &offset_only
        ));
        assert_eq!(
            crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
                baseline.clone(),
                &baseline,
            ),
            super::super::RetainedSurfaceCompileAction::Reuse
        );
        assert_eq!(
            crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
                baseline,
                &offset_only,
            ),
            super::super::RetainedSurfaceCompileAction::Reraster
        );
    }

    #[test]
    fn scroll_host_planner_rejects_non_identity_frame_context() {
        let (arena, root, _child, properties, generations) = fixture();
        let plan = |promoted: &FxHashSet<u64>, scale, offset, scissor| {
            super::super::plan_single_root_scroll_host_surface(
                &arena,
                &[root],
                promoted,
                &properties,
                &generations,
                scale,
                offset,
                scissor,
            )
        };
        assert!(plan(&Default::default(), 2.0, [0.0; 2], None).is_err());
        assert!(plan(&Default::default(), 1.0, [1.0, 0.0], None).is_err());
        assert!(plan(&Default::default(), 1.0, [0.0; 2], Some([0, 0, 100, 80]),).is_err());
        let mut promoted = FxHashSet::default();
        promoted.insert(81_001);
        assert!(plan(&promoted, 1.0, [0.0; 2], None).is_err());
    }

    #[test]
    fn baked_scroll_compiler_rejects_malicious_geometry_clip_and_scrollbar_tokens() {
        let (arena, root, child, properties, generations) = fixture();
        let scroll_id = ScrollNodeId(root);
        let clip_id = ClipNodeId {
            owner: root,
            role: ClipNodeRole::ContentsClip,
        };
        let scroll = properties.scroll_snapshot_for(scroll_id).unwrap();
        let contents_clip = properties
            .clip_snapshot_for(Some(clip_id))
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let witness = PaintBakedScrollHostWitness::new(root, child, scroll, clip_id).unwrap();
        let artifact = record_baked_scroll_host_artifact_for_plan(
            &arena,
            &[root],
            &Default::default(),
            &properties,
            &generations,
            witness,
        )
        .unwrap();
        let validates = |scroll, clip| {
            super::super::compiler::validate_baked_scroll_host_artifact(
                &artifact, root, child, scroll, clip,
            )
            .is_some()
        };
        assert!(validates(scroll, contents_clip));

        let mut malicious = scroll;
        malicious.scrollbar_overlay.paint_state =
            crate::view::base_component::ScrollbarPaintStateWitness::OpaqueNow;
        assert!(!validates(malicious, contents_clip));
        malicious = scroll;
        malicious.scrollbar_overlay.paint_state =
            crate::view::base_component::ScrollbarPaintStateWitness::TranslucentNow;
        assert!(!validates(malicious, contents_clip));
        malicious = scroll;
        malicious.offset.y = f32::NAN;
        assert!(!validates(malicious, contents_clip));
        malicious = scroll;
        malicious.offset.y = -1.0;
        assert!(!validates(malicious, contents_clip));
        malicious = scroll;
        malicious.offset.y = malicious.content_size.height;
        assert!(!validates(malicious, contents_clip));
        malicious = scroll;
        malicious.viewport.width = malicious.content_size.width + 1.0;
        assert!(!validates(malicious, contents_clip));
        malicious = scroll;
        malicious.viewport.x = -1.0;
        assert!(!validates(malicious, contents_clip));
        malicious = scroll;
        malicious.content_size.width = f32::NAN;
        assert!(!validates(malicious, contents_clip));
        malicious = scroll;
        malicious.layout_content_bounds_at_zero.x += 1.0;
        assert!(!validates(malicious, contents_clip));
        malicious = scroll;
        malicious.configured_axis = crate::view::base_component::ScrollAxisSnapshot::Horizontal;
        assert!(!validates(malicious, contents_clip));
        malicious = scroll;
        malicious.id = ScrollNodeId(child);
        assert!(!validates(malicious, contents_clip));
        malicious = scroll;
        malicious.owner = child;
        assert!(!validates(malicious, contents_clip));
        malicious = scroll;
        malicious.parent = Some(ScrollNodeId(child));
        assert!(!validates(malicious, contents_clip));
        malicious = scroll;
        malicious.generation = 0;
        assert!(!validates(malicious, contents_clip));
        malicious = scroll;
        let crate::view::base_component::ScrollContentsClipWitness::ExactRect(mut wrong_scissor) =
            malicious.contents_clip;
        wrong_scissor[0] += 1;
        malicious.contents_clip =
            crate::view::base_component::ScrollContentsClipWitness::ExactRect(wrong_scissor);
        assert!(!validates(malicious, contents_clip));

        let mut malicious_clip = contents_clip;
        malicious_clip.parent = Some(crate::view::compositor::property_tree::ClipNodeId {
            owner: root,
            role: crate::view::compositor::property_tree::ClipNodeRole::SelfClip,
        });
        assert!(!validates(scroll, malicious_clip));
        malicious_clip = contents_clip;
        malicious_clip.id.role = crate::view::compositor::property_tree::ClipNodeRole::SelfClip;
        assert!(!validates(scroll, malicious_clip));
        malicious_clip = contents_clip;
        malicious_clip.owner = child;
        assert!(!validates(scroll, malicious_clip));
        malicious_clip = contents_clip;
        malicious_clip.behavior = crate::view::compositor::property_tree::ClipBehavior::Replace;
        assert!(!validates(scroll, malicious_clip));
        malicious_clip = contents_clip;
        malicious_clip.generation = 0;
        assert!(!validates(scroll, malicious_clip));
        malicious_clip = contents_clip;
        malicious_clip.logical_scissor[0] += 1;
        assert!(!validates(scroll, malicious_clip));
    }
}

#[cfg(test)]
mod property_effect_artifact_tests {
    use super::*;
    use crate::view::base_component::Element;
    use crate::view::compositor::property_tree::{EffectNodeId, EffectNodeSnapshot};

    fn contract(
        arena: &NodeArena,
        property_trees: &PropertyTrees,
        generations: &PaintGenerationTracker,
        root: NodeKey,
        cutouts: &FxHashSet<NodeKey>,
    ) -> super::super::EffectPropertySurfaceArtifactContract {
        let live = property_trees
            .effect_snapshot_for(Some(EffectNodeId(root)))
            .expect("live effect chain");
        let isolated = EffectNodeSnapshot {
            parent: None,
            ..live[0]
        };
        let mut content = Vec::new();
        let mut stack = vec![root];
        while let Some(owner) = stack.pop() {
            if owner != root && cutouts.contains(&owner) {
                continue;
            }
            let node = arena.get(owner).expect("content owner");
            let revisions = generations
                .local_generations_for(owner)
                .expect("content generations");
            content.push(super::super::EffectPropertyContentWitness {
                owner,
                stable_id: node.element.stable_id(),
                parent: (owner != root).then(|| arena.parent_of(owner)).flatten(),
                self_paint_revision: revisions.self_paint_revision,
                topology_revision: revisions.topology_revision,
            });
            stack.extend(node.element.children().iter().rev().copied());
        }
        super::super::EffectPropertySurfaceArtifactContract::new(
            root,
            arena.get(root).unwrap().element.stable_id(),
            isolated,
            live.clone(),
            live[1..].to_vec(),
            Vec::new(),
            Vec::new(),
            content,
        )
        .expect("canonical effect contract")
    }

    #[test]
    fn property_effect_artifact_records_cutouts_and_detaches_ancestor_effects() {
        let (arena, root, mut properties, mut generations) =
            super::super::tests::exact_isolation_fixture(0.5);
        let child = arena.get(root).unwrap().element.children()[0];
        crate::view::test_support::get_element_mut::<Element>(&arena, child).set_opacity(0.25);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);

        let root_contract = contract(
            &arena,
            &properties,
            &generations,
            root,
            &FxHashSet::from_iter([child]),
        );
        let child_contract = contract(
            &arena,
            &properties,
            &generations,
            child,
            &FxHashSet::default(),
        );
        let cutouts = super::super::PlannedBoundaryCutoutSet::from_iter([(
            child,
            super::super::PlannedBoundary {
                root: child,
                stable_id: arena.get(child).unwrap().element.stable_id(),
                kind: super::super::PlannedBoundaryKind::Isolation(EffectNodeId(child)),
            },
        )]);
        let root_steps = record_effect_property_surface_steps_for_plan(
            &arena,
            &FxHashSet::default(),
            &properties,
            &generations,
            &root_contract,
            [0.0, 0.0],
            &cutouts,
            None,
        )
        .expect("root effect steps");
        assert!(matches!(
            root_steps.as_slice(),
            [RecordedTransformSurfaceStep::Artifact(_), RecordedTransformSurfaceStep::Boundary(boundary)]
                if boundary.root == child
        ));

        let child_steps = record_effect_property_surface_steps_for_plan(
            &arena,
            &FxHashSet::default(),
            &properties,
            &generations,
            &child_contract,
            [0.0, 0.0],
            &super::super::PlannedBoundaryCutoutSet::default(),
            None,
        )
        .expect("child effect steps");
        let [RecordedTransformSurfaceStep::Artifact(child_artifact)] = child_steps.as_slice()
        else {
            panic!("child effect surface must be one artifact span")
        };
        assert_eq!(
            child_artifact.effect_nodes.as_slice(),
            [child_contract.isolated_leaf()]
        );
        assert!(
            child_artifact
                .effect_nodes
                .iter()
                .all(|effect| effect.parent.is_none())
        );
        assert!(
            super::super::validate_effect_property_surface_artifact(
                child_artifact,
                &child_contract,
            )
            .is_some()
        );

        let mut leaked = child_artifact.clone();
        leaked.effect_nodes = child_contract.live_effect_chain().to_vec();
        assert!(
            super::super::validate_effect_property_surface_artifact(&leaked, &child_contract,)
                .is_none()
        );
    }
}
