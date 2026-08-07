//! Legacy retained recording bridge.
//!
//! Transport and proof tokens for the pre-V2 exact-shape retained middle
//! layer: recorded host/subtree pairs, their live raster oracles, and the
//! record/validate bridge that turns them into compiler tokens. The V2
//! layerizer consumes `PaintArtifact` and the six-dimensional property
//! snapshots, never these wrappers.
//!
//! Typed proof is preserved deliberately — these tokens still own host/local
//! parity and exact-once consumption until the old path is deleted. This whole
//! file goes in the Stage C hard-cutover change set. Do not add items here,
//! and do not widen oracle fields for outside access.

use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::view::base_component::{
    RetainedScrollAtomicProjectionSelectionTextAreaSubtreeAdmissionSnapshot,
    RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot,
    RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot,
    RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot,
    RetainedScrollTextAreaSubtreeAdmissionSnapshot,
};
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::node_arena::{NodeArena, NodeKey};

use super::frame_recorder::{
    ForcedFrameArtifactError, FrameArtifactDebugBoundary, FrameArtifactDebugBoundaryKind,
    FrameArtifactEligibility, FrameArtifactFallbackReason, FrameArtifactRecordOutcome,
    canonical_manifest_matches, materialize_frame_artifact,
};
use super::legacy_admission::{
    PaintLegacyTextAreaCoverageAuthority,
    PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness,
    PaintScrollAtomicProjectionTextAreaRecorderWitness as AtomicProjectionRecorderWitness,
    PaintScrollDetachedProjectionSubtreeWitness,
    PaintScrollFocusedAtomicProjectionTextAreaSubtreeWitness,
    PaintScrollInteractiveTextAreaSubtreeWitness, PaintScrollTextAreaSubtreeWitness,
};
use super::coverage_manifest::record_legacy_text_area_coverage_manifest;
use super::{
    CoverageRecordingMode, PaintCoverageItem,
    PaintArtifactTarget, PaintBakedScrollHostWitness, PaintCompositeEdge,
    PaintOpacityAuthority, RendererMode, TextPreeditPayloadIdentity,
    PaintArtifact, PaintChunk, PaintCoverageValidationError, PaintRecordingContext,
    PaintScrollContentWitness,
};

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
    pub(crate) fn payload_identity(&self) -> &super::PaintPayloadIdentity {
        &self.payload_identity
    }
}




pub(super) fn normalize_atomic_projection_selection_chunk(
    artifact: &mut PaintArtifact,
    oracle: &mut RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle,
    chunk_index: usize,
    source: super::PaintTextSelectionSource,
) -> Option<super::artifact::TextSelectionPayloadIdentity> {
    if !source.is_canonical() {
        return None;
    }
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
    let sealed = super::PaintPayloadIdentity::prepared_text_selection(
        source.start_char,
        source.end_char,
        source.color_rgba_bits,
        rects.iter().copied(),
    )?;
    if (chunk.payload_identity != generic && chunk.payload_identity != sealed)
        || oracle_chunk.payload_identity != generic
    {
        return None;
    }
    let seal = sealed.text_selection_identity()?;
    source
        .validate_payload_for_owner(chunk.owner, &seal)
        .ok()?;
    chunk.payload_identity = sealed.clone();
    oracle_chunk.payload_identity = sealed;
    Some(seal)
}


fn artifact_without_chunk(mut artifact: PaintArtifact, index: usize) -> Option<PaintArtifact> {
    let chunk = artifact.chunks.get(index)?.clone();
    let removed_start = chunk.op_range.start;
    let removed_end = chunk.op_range.end;
    let removed_len = removed_end.checked_sub(removed_start)?;
    (removed_end <= artifact.ops.len()).then_some(())?;
    artifact.ops.drain(removed_start..removed_end);
    artifact.chunks.remove(index);
    for chunk in &mut artifact.chunks {
        if chunk.op_range.start >= removed_end {
            chunk.op_range.start = chunk.op_range.start.checked_sub(removed_len)?;
            chunk.op_range.end = chunk.op_range.end.checked_sub(removed_len)?;
        } else if chunk.op_range.end > removed_start {
            return None;
        }
    }
    Some(artifact)
}

fn strip_preedit_underline_resident_view(
    artifact: PaintArtifact,
    oracle: RetainedAtomicProjectionTextAreaLiveRasterOracle,
    text_area_root: NodeKey,
    paint_offset_bits: [u32; 2],
    preedit: Option<&crate::view::base_component::text_area::FocusedAtomicPreeditSourceSeal>,
) -> Option<(
    PaintArtifact,
    RetainedAtomicProjectionTextAreaLiveRasterOracle,
)> {
    let Some(preedit) = preedit else {
        return Some((artifact, oracle));
    };
    preedit.is_canonical().then_some(())?;
    let [x, y, width, height] = preedit.underline_bounds_bits.map(f32::from_bits);
    let [dx, dy] = paint_offset_bits.map(f32::from_bits);
    let expected_bounds_bits = [
        (x + dx).to_bits(),
        (y + dy).to_bits(),
        width.to_bits(),
        height.to_bits(),
    ];
    let underline_index = artifact.chunks.iter().position(|chunk| {
        chunk.owner == text_area_root
            && chunk.id.owner == text_area_root
            && chunk.id.scope == super::PaintPropertyScope::Contents
            && chunk.id.phase == super::PaintNodePhase::AfterChildren
            && chunk.id.slot == 0
            && chunk.id.role == super::PaintChunkRole::TextDecoration
            && [
                chunk.bounds.x,
                chunk.bounds.y,
                chunk.bounds.width,
                chunk.bounds.height,
            ]
            .map(f32::to_bits)
                == expected_bounds_bits
    });
    let underline_index = underline_index?;
    let ops = artifact
        .ops
        .get(artifact.chunks[underline_index].op_range.clone())?;
    let rects = ops
        .iter()
        .map(|op| match op {
            super::PaintOp::DrawRect(rect) => Some(rect),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    (super::PaintPayloadIdentity::prepared_rects(rects.iter().copied()).as_ref()
        == Some(&artifact.chunks[underline_index].payload_identity))
    .then_some(())?;
    let artifact_len = artifact.chunks.len();
    let oracle_len = oracle.chunks.len();
    let artifact = artifact_without_chunk(artifact, underline_index)?;
    let oracle = match oracle_len.checked_add(1) {
        Some(len) if len == artifact_len => oracle,
        _ => oracle.without_chunk(underline_index)?,
    };
    Some((artifact, oracle))
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
pub(super) struct RecordedRetainedFocusedAtomicProjectionTextAreaSubtree {
    artifact: PaintArtifact,
    raster_oracle: RetainedAtomicProjectionTextAreaLiveRasterOracle,
    caret: crate::view::base_component::text_area::FocusedAtomicCaretSourceSeal,
    preedit: Option<crate::view::base_component::text_area::FocusedAtomicPreeditSourceSeal>,
}

#[derive(Clone, Debug)]
pub(super) struct RecordedRetainedFocusedAtomicProjectionTextAreaHost {
    artifact: PaintArtifact,
    raster_oracle: RetainedAtomicProjectionTextAreaLiveRasterOracle,
    caret: crate::view::base_component::text_area::FocusedAtomicCaretSourceSeal,
    preedit: Option<crate::view::base_component::text_area::FocusedAtomicPreeditSourceSeal>,
    source_bounds_bits: [u32; 4],
    outer_scroll: crate::view::compositor::property_tree::ScrollNodeSnapshot,
    outer_contents_clip: crate::view::compositor::property_tree::ClipNodeSnapshot,
    local_contents_clip: crate::view::compositor::property_tree::ClipNodeSnapshot,
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
    selection: super::artifact::TextSelectionPayloadIdentity,
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
        let selection = self
            .artifact
            .chunks
            .iter()
            .position(|chunk| chunk.id.role == super::PaintChunkRole::SelectionUnderlay)
            .unwrap();
        let replacement = self
            .artifact
            .chunks
            .iter()
            .find(|chunk| chunk.id.role == super::PaintChunkRole::TextGlyphs)
            .unwrap()
            .payload_identity
            .clone();
        self.artifact.chunks[selection].payload_identity = replacement.clone();
        self.raster_oracle.chunks[selection].payload_identity = replacement;
        self
    }

    pub(crate) fn tamper_wrapper_bounds_for_test(mut self) -> Self {
        self.artifact.chunks[1].bounds.x += 1.0;
        self.raster_oracle.chunks[1].bounds_bits[0] = self.artifact.chunks[1].bounds.x.to_bits();
        self
    }

    pub(crate) fn tamper_source_line_for_test(mut self) -> Self {
        self.raster_oracle.artifact_source.projection_text_bounds_bits[0] ^= 1;
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
        let selection = self
            .artifact
            .chunks
            .iter()
            .position(|chunk| chunk.id.role == super::PaintChunkRole::SelectionUnderlay)
            .unwrap();
        let replacement = self
            .artifact
            .chunks
            .iter()
            .find(|chunk| chunk.id.role == super::PaintChunkRole::TextGlyphs)
            .unwrap()
            .payload_identity
            .clone();
        self.artifact.chunks[selection].payload_identity = replacement.clone();
        self.raster_oracle.chunks[selection].payload_identity = replacement;
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
        self.raster_oracle.artifact_source.projection_text_bounds_bits[0] ^= 1;
        self
    }
}

impl ValidatedRecordedAtomicProjectionSelectionTextAreaAuthority {
    fn is_canonical(&self) -> bool {
        fn unique_selection(chunks: &[super::PaintChunk]) -> Option<&super::PaintChunk> {
            let mut matches = chunks
                .iter()
                .filter(|chunk| chunk.id.role == super::PaintChunkRole::SelectionUnderlay);
            matches.next().filter(|_| matches.next().is_none())
        }
        let Some(host_selection) = unique_selection(&self.host.artifact.chunks) else {
            return false;
        };
        let Some(local_selection) = unique_selection(&self.local.artifact.chunks) else {
            return false;
        };
        self.host
            .raster_oracle
            .matches_artifact(&self.host.artifact)
            && self
                .local
                .raster_oracle
                .matches_artifact(&self.local.artifact)
            && self
                .local
                .raster_oracle
                .selection_source
                .matches_payload(&self.selection)
            && host_selection
                .payload_identity
                .text_selection_identity()
                .is_some()
            && local_selection
                .payload_identity
                .text_selection_identity()
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
        let host = self
            .host
            .artifact
            .chunks
            .iter()
            .find(|chunk| chunk.id.role == super::PaintChunkRole::SelectionUnderlay)
            .unwrap()
            .payload_identity
            .text_selection_identity();
        let local = self
            .local
            .artifact
            .chunks
            .iter()
            .find(|chunk| chunk.id.role == super::PaintChunkRole::SelectionUnderlay)
            .unwrap()
            .payload_identity
            .text_selection_identity();
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

    #[cfg(test)]
    pub(crate) fn tamper_artifact_space_transition_for_test(mut self) -> Self {
        let revision = self
            .raster_oracle
            .artifact_space_transition
            .semantic_revision();
        self.raster_oracle.artifact_space_transition =
            super::PaintArtifactSpaceTransition::from_bits(
                [1234.0_f32.to_bits(), 0.0_f32.to_bits()],
                [0.0_f32.to_bits(), 0.0_f32.to_bits()],
                revision,
            )
            .unwrap();
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

#[cfg(test)]
impl RecordedRetainedFocusedAtomicProjectionTextAreaSubtree {
    pub(crate) fn artifact_for_test(&self) -> &PaintArtifact {
        &self.artifact
    }

    pub(crate) fn is_canonical_for_test(&self) -> bool {
        self.caret.is_canonical() && self.raster_oracle.matches_artifact(&self.artifact)
    }

    pub(crate) fn caret_for_test(
        &self,
    ) -> &crate::view::base_component::text_area::FocusedAtomicCaretSourceSeal {
        &self.caret
    }

}

#[cfg(test)]
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

/// Focused glyph-only typed bridge. The resident raster authority is delegated
/// to the existing C3a glyph validator; the focused caret remains a separate
/// sealed source fact and never becomes a resident dependency.
pub(super) fn validate_recorded_focused_atomic_projection_text_area_plan_parts(
    host: RecordedRetainedFocusedAtomicProjectionTextAreaHost,
    local: RecordedRetainedFocusedAtomicProjectionTextAreaSubtree,
) -> Option<super::compiler::ValidatedScrollSceneFocusedAtomicProjectionTextAreaPlanParts> {
    let RecordedRetainedFocusedAtomicProjectionTextAreaHost {
        artifact: host_artifact,
        raster_oracle: host_raster_oracle,
        caret: host_caret,
        preedit: host_preedit,
        source_bounds_bits,
        outer_scroll,
        outer_contents_clip,
        local_contents_clip,
    } = host;
    let RecordedRetainedFocusedAtomicProjectionTextAreaSubtree {
        artifact: local_artifact,
        raster_oracle: local_raster_oracle,
        caret: local_caret,
        preedit: local_preedit,
    } = local;
    let (host_artifact, host_raster_oracle) = strip_preedit_underline_resident_view(
        host_artifact,
        host_raster_oracle,
        host_caret.owner,
        [
            (-outer_scroll.offset.x).to_bits(),
            (-outer_scroll.offset.y).to_bits(),
        ],
        host_preedit.as_ref(),
    )?;
    let (local_artifact, local_raster_oracle) = strip_preedit_underline_resident_view(
        local_artifact,
        local_raster_oracle,
        local_caret.owner,
        [0.0_f32.to_bits(), 0.0_f32.to_bits()],
        local_preedit.as_ref(),
    )?;
    super::compiler::validate_scroll_scene_focused_atomic_projection_text_area_plan_parts(
        host_artifact,
        host_raster_oracle,
        host_caret,
        host_preedit,
        source_bounds_bits,
        outer_scroll,
        outer_contents_clip,
        local_contents_clip,
        local_artifact,
        local_raster_oracle,
        local_caret,
        local_preedit,
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
        || !matches!(
            (
                host.raster_oracle.chunks.len(),
                local.raster_oracle.chunks.len()
            ),
            (6, 4) | (8, 6)
        )
        || host.raster_oracle.content_root != local.raster_oracle.content_root
        || host.raster_oracle.text_area_root != local.raster_oracle.text_area_root
        || host.raster_oracle.artifact_source != local.raster_oracle.artifact_source
        || host.raster_oracle.selection_source != local.raster_oracle.selection_source
        || !local
            .raster_oracle
            .artifact_source
            .is_canonical_for(local.raster_oracle.text_area_root)
        || !local.raster_oracle.selection_source.is_canonical()
    {
        return None;
    }
    let content_root = local.raster_oracle.content_root;
    let text_area_root = local.raster_oracle.text_area_root;
    let selection_source = local.raster_oracle.selection_source;
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
    let outer_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(host.outer_contents_clip.id),
        scroll: Some(host.outer_scroll.id),
        ..Default::default()
    };
    let (root_before, host_tail) = host.artifact.chunks.split_first()?;
    let (overlay, host_content_chunks) = host_tail.split_last()?;
    let Some((host_wrapper, [host_selection, host_root_glyph, host_projection_glyph])) =
        classify_optional_child_mask_semantics(
            &host.artifact,
            host_content_chunks,
            content_root,
            outer_state,
        )
    else {
        return None;
    };
    let Some((local_wrapper, [local_selection, local_root_glyph, local_projection_glyph])) =
        classify_optional_child_mask_semantics(
            &local.artifact,
            local.artifact.chunks.as_slice(),
            content_root,
            Default::default(),
        )
    else {
        return None;
    };
    let selection = local_selection
        .payload_identity
        .text_selection_identity()?;
    host_selection
        .payload_identity
        .text_selection_identity()?;
    if !host_selection
        .payload_identity
        .matches_text_selection_source(
            selection_source.start_char,
            selection_source.end_char,
            selection_source.color_rgba_bits,
        )
        || !local_selection
            .payload_identity
            .matches_text_selection_source(
                selection_source.start_char,
                selection_source.end_char,
                selection_source.color_rgba_bits,
            )
    {
        return None;
    }
    let host_local_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(local_clip.id),
        scroll: Some(host.outer_scroll.id),
        ..Default::default()
    };
    let local_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(local_clip.id),
        ..Default::default()
    };
    let transition = local.raster_oracle.artifact_space_transition;
    if host.raster_oracle.artifact_space_transition != transition {
        return None;
    }
    let delta = transition.translation()?;
    let pair_exact = |host_chunk: &super::PaintChunk, local_chunk: &super::PaintChunk| {
        let localized = host
            .artifact
            .ops
            .get(host_chunk.op_range.clone())?
            .iter()
            .map(|op| super::compiler::localize_exact_nested_scroll_leaf_op(op, delta))
            .collect::<Option<Vec<_>>>()?;
        let payload = if host_chunk.id.slot == super::RETAINED_CHILD_MASK_SLOT {
            super::PaintPayloadIdentity::prepared_rects(localized.iter().filter_map(
                |op| match op {
                    super::PaintOp::DrawRect(rect) => Some(rect),
                    _ => None,
                },
            ))?
        } else if host_chunk.id.role == super::PaintChunkRole::SelectionUnderlay {
            super::PaintPayloadIdentity::prepared_text_selection(
                selection_source.start_char,
                selection_source.end_char,
                selection_source.color_rgba_bits,
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
                transition,
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
    let mask_variant_parity = match (host.artifact.chunks.len(), local.artifact.chunks.len()) {
        (6, 4) => true,
        (8, 6) => {
            pair_exact(&host.artifact.chunks[2], &local.artifact.chunks[1]).is_some()
                && pair_exact(&host.artifact.chunks[6], &local.artifact.chunks[5]).is_some()
        }
        _ => false,
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
        || !mask_variant_parity
        || root_before.owner != boundary_root
        || root_before.id.owner != boundary_root
        || root_before.id.scope != super::PaintPropertyScope::SelfPaint
        || root_before.id.phase != super::PaintNodePhase::BeforeChildren
        || root_before.id.slot != 0
        || root_before.id.role != super::PaintChunkRole::SelfDecoration
        || root_before.properties.legacy_boundary_dimensions() != Default::default()
        || [
            root_before.bounds.x,
            root_before.bounds.y,
            root_before.bounds.width,
            root_before.bounds.height,
        ]
        .map(f32::to_bits)
            != host.source_bounds_bits
        || !host_wrapper.properties.legacy_boundary_eq(outer_state)
        || !host_selection.properties.legacy_boundary_eq(host_local_state)
        || !host_root_glyph.properties.legacy_boundary_eq(host_local_state)
        || !host_projection_glyph
            .properties
            .legacy_boundary_eq(host_local_state)
        || local_wrapper.properties.legacy_boundary_dimensions() != Default::default()
        || !local_selection.properties.legacy_boundary_eq(local_state)
        || !local_root_glyph.properties.legacy_boundary_eq(local_state)
        || !local_projection_glyph
            .properties
            .legacy_boundary_eq(local_state)
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
        || host_projection_glyph.owner
            != local.raster_oracle.artifact_source.projection_text_owner
        || [
            host_projection_glyph.bounds.x,
            host_projection_glyph.bounds.y,
            host_projection_glyph.bounds.width,
            host_projection_glyph.bounds.height,
        ]
        .map(f32::to_bits)
            != local
                .raster_oracle
                .artifact_source
                .projection_text_bounds_bits
        || overlay.owner != boundary_root
        || overlay.id.owner != boundary_root
        || overlay.id.scope != super::PaintPropertyScope::SelfPaint
        || overlay.id.phase != super::PaintNodePhase::AfterChildren
        || overlay.id.slot != 0
        || overlay.id.role != super::PaintChunkRole::ScrollbarOverlay
        || overlay.properties.legacy_boundary_dimensions() != Default::default()
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

#[derive(Clone, Debug)]
pub(super) struct RecordedRetainedInteractiveTextAreaSubtree {
    artifact: PaintArtifact,
    preedit_seal: Option<TextPreeditPayloadIdentity>,
    composite_edges: Arc<[PaintCompositeEdge]>,
}

impl RecordedRetainedInteractiveTextAreaSubtree {
    /// Read-only views for the legacy planner. Callers may inspect the recorded
    /// pair but never mutate it: the token owns host/local parity until the old
    /// path is deleted.
    pub(super) fn artifact(&self) -> &PaintArtifact {
        &self.artifact
    }

    pub(super) fn preedit_seal(&self) -> Option<&TextPreeditPayloadIdentity> {
        self.preedit_seal.as_ref()
    }

    pub(super) fn composite_edges(&self) -> &[PaintCompositeEdge] {
        &self.composite_edges
    }

    /// Consumes the recorded pair. Exact-once by construction: the planner gets
    /// the artifact and the composite edges together, or neither.
    pub(super) fn into_parts(self) -> (PaintArtifact, Arc<[PaintCompositeEdge]>) {
        (self.artifact, self.composite_edges)
    }
}

pub(super) fn record_scroll_interactive_text_area_subtree_local_artifact_for_plan(
    arena: &NodeArena,
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
    if !admission.matches_live_source(text_area, arena, detached_paint_offset) {
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
        admission.paint_source,
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
        .interactive_caret_composite_edge(
            text_area_root,
            arena,
            detached_paint_offset,
            live_paint_offset,
            *live_text_area_clip,
            *live_outer_clip,
            text_area_state,
            admission.paint_source,
            admission.caret_oracle_bounds_bits,
        )
        .ok_or_else(|| invalid(text_area_root))?;
    let preedit_before = if admission.paint_source.has_preedit() {
        Some(
            text_area
                .text_preedit_payload_identity(
                    text_area_root,
                    arena,
                    detached_paint_offset,
                )
                .ok_or_else(|| invalid(text_area_root))?,
        )
    } else {
        None
    };
    let mut artifact = match record_legacy_text_area_frame_artifact(
        arena,
        &[content_root],
        property_trees,
        paint_generations,
        PaintLegacyTextAreaCoverageAuthority::InteractiveLocal(witness),
        None,
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
        .interactive_caret_composite_edge(
            text_area_root,
            arena,
            detached_paint_offset,
            live_paint_offset,
            *live_text_area_clip,
            *live_outer_clip,
            text_area_state,
            admission.paint_source,
            admission.caret_oracle_bounds_bits,
        )
        .ok_or_else(|| invalid(text_area_root))?;
    let preedit_after = if admission.paint_source.has_preedit() {
        Some(
            text_area
                .text_preedit_payload_identity(
                    text_area_root,
                    arena,
                    detached_paint_offset,
                )
                .ok_or_else(|| invalid(text_area_root))?,
        )
    } else {
        None
    };
    if caret_before != caret_after || preedit_before != preedit_after
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
            && chunk.properties.legacy_boundary_dimensions() == Default::default()
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
            && chunk.properties.legacy_boundary_eq(local_state)
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
            && chunk.properties.legacy_boundary_eq(local_state)
            && super::PaintPayloadIdentity::prepared_rects(rects.into_iter()).as_ref()
                == Some(&chunk.payload_identity)
    };
    let Some((wrapper, semantic)) = classify_optional_child_mask_semantics(
        &artifact,
        artifact.chunks.as_slice(),
        content_root,
        Default::default(),
    ) else {
        return Err(invalid(content_root));
    };
    if !wrapper_matches(wrapper) {
        return Err(invalid(content_root));
    }
    let preedit_seal = match admission.paint_source {
        super::PaintTextContentSource::Glyphs => {
            let [glyph] = semantic else {
                return Err(invalid(content_root));
            };
            if !glyph_matches(glyph) {
                return Err(invalid(content_root));
            }
            None
        }
        super::PaintTextContentSource::Selection(_) => {
            let [selection, glyph] = semantic else {
                return Err(invalid(content_root));
            };
            if !rect_chunk_matches(
                    selection,
                    super::PaintNodePhase::BeforeChildren,
                    0,
                    super::PaintChunkRole::SelectionUnderlay,
                )
                || !glyph_matches(glyph)
            {
                return Err(invalid(content_root));
            }
            None
        }
        super::PaintTextContentSource::Preedit => {
            let [glyph, underline] = semantic else {
                return Err(invalid(content_root));
            };
            if !glyph_matches(glyph)
                || !rect_chunk_matches(
                        underline,
                        super::PaintNodePhase::AfterChildren,
                        0,
                        super::PaintChunkRole::TextDecoration,
                    )
            {
                return Err(invalid(content_root));
            }
            let seal = preedit_after.ok_or_else(|| invalid(text_area_root))?;
            if seal.glyph_identity != glyph.payload_identity
                || seal.underline_identity != underline.payload_identity
            {
                return Err(invalid(text_area_root));
            }
            Some(seal)
        }
    };
    if let Some(selection_source) = admission.paint_source.selection() {
        let mut selection_indices = artifact
            .chunks
            .iter()
            .enumerate()
            .filter_map(|(index, chunk)| {
                (chunk.owner == text_area_root
                    && chunk.id.role == super::PaintChunkRole::SelectionUnderlay)
                    .then_some(index)
            });
        let selection_index = selection_indices
            .next()
            .filter(|_| selection_indices.next().is_none())
            .ok_or_else(|| invalid(text_area_root))?;
        let rects = artifact.ops[artifact.chunks[selection_index].op_range.clone()]
            .iter()
            .map(|op| match op {
                super::PaintOp::DrawRect(rect) => Some(rect),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| invalid(text_area_root))?;
        artifact.chunks[selection_index].payload_identity =
            super::PaintPayloadIdentity::prepared_text_selection(
                selection_source.start_char,
                selection_source.end_char,
                selection_source.color_rgba_bits,
                rects.into_iter(),
            )
            .ok_or_else(|| invalid(text_area_root))?;
    }
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
        composite_edges: caret_after.into_iter().collect::<Vec<_>>().into(),
    })
}

// ---- live raster oracles (moved from legacy_admission.rs) ----

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedAtomicProjectionTextAreaLiveRasterOracle {
    content_root: NodeKey,
    text_area_root: NodeKey,
    artifact_space_transition: super::PaintArtifactSpaceTransition,
    artifact_source: super::PaintAtomicProjectionArtifactSource,
    chunks: Vec<RetainedAtomicProjectionChunkLiveRasterOracle>,
    clip_nodes: Vec<crate::view::compositor::property_tree::ClipNodeSnapshot>,
    owner_nodes: Vec<super::PaintOwnerSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle {
    content_root: NodeKey,
    text_area_root: NodeKey,
    artifact_space_transition: super::PaintArtifactSpaceTransition,
    artifact_source: super::PaintAtomicProjectionArtifactSource,
    selection_source: super::PaintTextSelectionSource,
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

    pub(crate) fn artifact_space_transition(&self) -> super::PaintArtifactSpaceTransition {
        self.artifact_space_transition
    }

    pub(crate) fn artifact_source(&self) -> &super::PaintAtomicProjectionArtifactSource {
        &self.artifact_source
    }

    pub(crate) fn selection_source(&self) -> super::PaintTextSelectionSource {
        self.selection_source
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

impl RetainedAtomicProjectionTextAreaLiveRasterOracle {
    pub(crate) fn content_root(&self) -> NodeKey {
        self.content_root
    }

    pub(crate) fn text_area_root(&self) -> NodeKey {
        self.text_area_root
    }

    pub(crate) fn artifact_space_transition(&self) -> super::PaintArtifactSpaceTransition {
        self.artifact_space_transition
    }

    pub(crate) fn artifact_source(&self) -> &super::PaintAtomicProjectionArtifactSource {
        &self.artifact_source
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

    pub(super) fn without_chunk(mut self, index: usize) -> Option<Self> {
        (index < self.chunks.len()).then_some(())?;
        self.chunks.remove(index);
        Some(self)
    }
}


// ---- legacy record/validate bridge (moved from frame_recorder.rs) ----

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



pub(super) fn record_baked_scroll_atomic_projection_text_area_subtree_host_artifact_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
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
    if !admission.matches_live_source(text_area, arena, recording_offset) {
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
    let recorder_authority = AtomicProjectionRecorderWitness::ExistingAtomicGlyph(
        PaintScrollDetachedProjectionSubtreeWitness::new(
            outer,
            admission.text_area_root,
            *live_text_area_clip,
            local_scissor,
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
    owners.extend(
        admission
            .artifact_source
            .descendant_owner_topology
            .iter()
            .copied(),
    );
    let authority =
        PaintLegacyTextAreaCoverageAuthority::AtomicProjectionBakedHost(recorder_authority);
    let oracle_context = PaintRecordingContext {
        baked_scroll_host: Some(baked),
        opacity_authority: PaintOpacityAuthority::Baked,
        ..PaintRecordingContext::default()
    };
    let raster_before = record_atomic_projection_live_raster_oracle(
        arena,
        roots,
        property_trees,
        paint_generations,
        authority,
        oracle_context,
        admission.content_wrapper,
        admission.text_area_root,
        &admission.artifact_source,
        admission.artifact_space_transition,
        owners.clone(),
    )?;
    let mut artifact = match record_legacy_text_area_frame_artifact(
        arena,
        roots,
        property_trees,
        paint_generations,
        authority,
        oracle_context.baked_scroll_host,
        None,
        None,
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    if !admission.matches_live_source(text_area, arena, recording_offset) {
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
        property_trees,
        paint_generations,
        authority,
        oracle_context,
        admission.content_wrapper,
        admission.text_area_root,
        &admission.artifact_source,
        admission.artifact_space_transition,
        owners.clone(),
    )?;
    if raster_before != raster_after || !raster_before.matches_artifact(&artifact) {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    let Some((root_before, tail)) = artifact.chunks.split_first() else {
        return Err(invalid(admission.boundary_root));
    };
    let Some((overlay, content_chunks)) = tail.split_last() else {
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
    let Some((wrapper_chunk, [root_glyph, projection_glyph])) =
        classify_optional_child_mask_semantics(
            &artifact,
            content_chunks,
            admission.content_wrapper,
            outer_state,
        )
    else {
        return Err(invalid(admission.boundary_root));
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
            && chunk.properties.legacy_boundary_eq(glyph_state)
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
        || !wrapper_chunk.properties.legacy_boundary_eq(outer_state)
        || !glyph_exact(
            root_glyph,
            admission.text_area_root,
            super::PaintPropertyScope::Contents,
        )
        || !glyph_exact(
            projection_glyph,
            admission.artifact_source.projection_text_owner,
            super::PaintPropertyScope::SelfPaint,
        )
        || [
            projection_glyph.bounds.x,
            projection_glyph.bounds.y,
            projection_glyph.bounds.width,
            projection_glyph.bounds.height,
        ]
        .map(f32::to_bits)
            != admission.artifact_source.projection_text_bounds_bits
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

/// Focused C3 glyph host recorder. It records the same glyph-only resident
/// chunks as C3a while preserving the focused caret as a source sidecar.

pub(super) fn record_baked_scroll_focused_atomic_projection_text_area_subtree_host_artifact_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    admission: &RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot,
    baked: PaintBakedScrollHostWitness,
) -> Result<RecordedRetainedFocusedAtomicProjectionTextAreaHost, Vec<FrameArtifactFallbackReason>> {
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
    if !admission.matches_live_source(text_area, arena, recording_offset) {
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
    let recorder_witness = PaintScrollFocusedAtomicProjectionTextAreaSubtreeWitness::new(
        outer,
        admission.text_area_root,
        *live_text_area_clip,
        local_scissor,
    )
    .ok_or_else(|| invalid(admission.text_area_root))?;
    let recorder_authority =
        AtomicProjectionRecorderWitness::FocusedAtomicProjectionGlyph(recorder_witness);
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
    owners.extend(
        admission
            .artifact_source
            .descendant_owner_topology
            .iter()
            .copied(),
    );
    let authority =
        PaintLegacyTextAreaCoverageAuthority::AtomicProjectionBakedHost(recorder_authority);
    let oracle_context = PaintRecordingContext {
        baked_scroll_host: Some(baked),
        opacity_authority: PaintOpacityAuthority::Baked,
        ..PaintRecordingContext::default()
    };
    let raster_before = record_atomic_projection_live_raster_oracle(
        arena,
        roots,
        property_trees,
        paint_generations,
        authority,
        oracle_context,
        admission.content_wrapper,
        admission.text_area_root,
        &admission.artifact_source,
        admission.artifact_space_transition,
        owners.clone(),
    )?;
    let mut artifact = match record_legacy_text_area_frame_artifact(
        arena,
        roots,
        property_trees,
        paint_generations,
        authority,
        oracle_context.baked_scroll_host,
        None,
        None,
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    if !admission.matches_live_source(text_area, arena, recording_offset) {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    artifact.owner_nodes = owners.clone();
    let raster_after = record_atomic_projection_live_raster_oracle(
        arena,
        roots,
        property_trees,
        paint_generations,
        authority,
        oracle_context,
        admission.content_wrapper,
        admission.text_area_root,
        &admission.artifact_source,
        admission.artifact_space_transition,
        owners,
    )?;
    if raster_before != raster_after || !raster_before.matches_artifact(&artifact) {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    Ok(RecordedRetainedFocusedAtomicProjectionTextAreaHost {
        artifact,
        raster_oracle: raster_before,
        caret: admission.caret_source().clone(),
        preedit: admission.preedit_source().cloned(),
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
    if !admission.matches_live_source(text_area, arena, recording_offset) {
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
    let recorder_witness = PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness::new(
        outer,
        admission.text_area_root,
        *live_text_area_clip,
        local_scissor,
        admission.selection_source,
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
    owners.extend(
        admission
            .artifact_source
            .descendant_owner_topology
            .iter()
            .copied(),
    );
    let authority =
        PaintLegacyTextAreaCoverageAuthority::AtomicProjectionBakedHost(recorder_authority);
    let oracle_context = PaintRecordingContext {
        baked_scroll_host: Some(baked),
        opacity_authority: PaintOpacityAuthority::Baked,
        ..PaintRecordingContext::default()
    };
    let mut raster_before = record_atomic_projection_selection_live_raster_oracle(
        arena,
        roots,
        property_trees,
        paint_generations,
        authority,
        oracle_context,
        admission.content_wrapper,
        admission.text_area_root,
        &admission.artifact_source,
        admission.selection_source,
        admission.artifact_space_transition,
        owners.clone(),
    )?;
    let mut artifact = match record_legacy_text_area_frame_artifact(
        arena,
        roots,
        property_trees,
        paint_generations,
        authority,
        oracle_context.baked_scroll_host,
        None,
        None,
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    if !admission.matches_live_source(text_area, arena, recording_offset) {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    artifact.owner_nodes = owners.clone();
    let mut selection_indices = artifact
        .chunks
        .iter()
        .enumerate()
        .filter_map(|(index, chunk)| {
            (chunk.owner == admission.text_area_root
                && chunk.id.role == super::PaintChunkRole::SelectionUnderlay)
                .then_some(index)
        });
    let selection_index = selection_indices
        .next()
        .filter(|_| selection_indices.next().is_none())
        .ok_or_else(|| invalid(admission.text_area_root))?;
    normalize_atomic_projection_selection_chunk(
        &mut artifact,
        &mut raster_before,
        selection_index,
        recorder_witness.selection,
    )
    .ok_or_else(|| invalid(admission.text_area_root))?;
    let mut raster_after = record_atomic_projection_selection_live_raster_oracle(
        arena,
        roots,
        property_trees,
        paint_generations,
        authority,
        oracle_context,
        admission.content_wrapper,
        admission.text_area_root,
        &admission.artifact_source,
        admission.selection_source,
        admission.artifact_space_transition,
        owners.clone(),
    )?;
    normalize_atomic_projection_selection_chunk(
        &mut artifact,
        &mut raster_after,
        selection_index,
        recorder_witness.selection,
    )
    .ok_or_else(|| invalid(admission.text_area_root))?;
    if raster_before != raster_after || !raster_before.matches_artifact(&artifact) {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    let Some((root_before, tail)) = artifact.chunks.split_first() else {
        return Err(invalid(admission.boundary_root));
    };
    let Some((overlay, content_chunks)) = tail.split_last() else {
        return Err(invalid(admission.boundary_root));
    };
    let outer_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(baked.contents_clip()),
        scroll: Some(baked.scroll()),
        ..Default::default()
    };
    let Some((wrapper_chunk, [selection, root_glyph, projection_glyph])) =
        classify_optional_child_mask_semantics(
            &artifact,
            content_chunks,
            admission.content_wrapper,
            outer_state,
        )
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
    let selection_source = recorder_witness.selection;
    let selection_exact = matches!(&artifact.ops[selection.op_range.clone()], ops
    if !ops.is_empty()
        && ops.iter().all(|op| matches!(op, super::PaintOp::DrawRect(_)))
        && selection.payload_identity.matches_text_selection_source(
            selection_source.start_char,
            selection_source.end_char,
            selection_source.color_rgba_bits,
        )
        && selection.payload_identity.matches_exact_text_selection_ops(
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
            admission.artifact_source.projection_text_owner,
            super::PaintPropertyScope::SelfPaint,
        )
        || [
            projection_glyph.bounds.x,
            projection_glyph.bounds.y,
            projection_glyph.bounds.width,
            projection_glyph.bounds.height,
        ]
        .map(f32::to_bits)
            != admission.artifact_source.projection_text_bounds_bits
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


fn record_atomic_projection_live_raster_oracle(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    authority: PaintLegacyTextAreaCoverageAuthority,
    context: PaintRecordingContext,
    content_root: NodeKey,
    text_area_root: NodeKey,
    artifact_source: &super::PaintAtomicProjectionArtifactSource,
    artifact_space_transition: super::PaintArtifactSpaceTransition,
    owner_nodes: Vec<super::PaintOwnerSnapshot>,
) -> Result<RetainedAtomicProjectionTextAreaLiveRasterOracle, Vec<FrameArtifactFallbackReason>> {
    if !artifact_space_transition.is_canonical()
        || !artifact_source.is_canonical_for(text_area_root)
    {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    let manifest = record_legacy_metadata_manifest(
        arena,
        roots,
        property_trees,
        paint_generations,
        context,
        authority,
    );
    let eligibility = assess_legacy_manifest(&manifest, authority, context.baked_scroll_host);
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
        artifact_space_transition,
        artifact_source: artifact_source.clone(),
        chunks,
        clip_nodes: clips,
        owner_nodes,
    })
}

#[allow(clippy::too_many_arguments)]

fn record_atomic_projection_selection_live_raster_oracle(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    authority: PaintLegacyTextAreaCoverageAuthority,
    context: PaintRecordingContext,
    content_root: NodeKey,
    text_area_root: NodeKey,
    artifact_source: &super::PaintAtomicProjectionArtifactSource,
    selection_source: super::PaintTextSelectionSource,
    artifact_space_transition: super::PaintArtifactSpaceTransition,
    owner_nodes: Vec<super::PaintOwnerSnapshot>,
) -> Result<
    RetainedAtomicProjectionSelectionTextAreaLiveRasterOracle,
    Vec<FrameArtifactFallbackReason>,
> {
    if !artifact_space_transition.is_canonical()
        || !artifact_source.is_canonical_for(text_area_root)
        || !selection_source.is_canonical()
    {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    let manifest = record_legacy_metadata_manifest(
        arena,
        roots,
        property_trees,
        paint_generations,
        context,
        authority,
    );
    let eligibility = assess_legacy_manifest(&manifest, authority, context.baked_scroll_host);
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
        artifact_space_transition,
        artifact_source: artifact_source.clone(),
        selection_source,
        chunks,
        clip_nodes: clips,
        owner_nodes,
    })
}


pub(super) fn record_scroll_atomic_projection_text_area_subtree_local_artifact_for_plan(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    admission: &RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot,
    outer: PaintScrollContentWitness,
) -> Result<RecordedRetainedAtomicProjectionTextAreaSubtree, Vec<FrameArtifactFallbackReason>> {
    let content_root = admission.content_wrapper;
    let text_area_root = admission.text_area_root;
    let invalid = |owner| vec![FrameArtifactFallbackReason::PropertyBoundary(owner)];
    if outer.boundary_root() != admission.boundary_root
        || outer.content_root() != content_root
        || outer.scroll_snapshot().owner != admission.boundary_root
        || !admission.matches_scroll_node(outer.scroll_snapshot())
        || !admission.artifact_source.is_canonical_for(text_area_root)
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
    if !admission.matches_live_source(text_area, arena, required_paint_offset) {
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
    let recorder_authority = AtomicProjectionRecorderWitness::ExistingAtomicGlyph(
        PaintScrollDetachedProjectionSubtreeWitness::new(
            outer,
            text_area_root,
            *live_text_area_clip,
            local_scissor,
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
    if !atomic_projection_owner_topology_is_live(
        arena,
        property_trees,
        text_area_root,
        text_area_state,
        &admission.artifact_source,
    ) {
        return Err(invalid(text_area_root));
    }
    expected_owners.extend(
        admission
            .artifact_source
            .descendant_owner_topology
            .iter()
            .copied(),
    );

    let authority =
        PaintLegacyTextAreaCoverageAuthority::AtomicProjectionLocal(recorder_authority);
    let oracle_context = PaintRecordingContext {
        paint_offset: recorder_authority.outer().normalization_paint_offset(),
        required_scroll_content_paint_offset_bits: Some(required_paint_offset.map(f32::to_bits)),
        opacity_authority: PaintOpacityAuthority::Baked,
        ..PaintRecordingContext::default()
    };
    let raster_before = record_atomic_projection_live_raster_oracle(
        arena,
        &[content_root],
        property_trees,
        paint_generations,
        authority,
        oracle_context,
        content_root,
        text_area_root,
        &admission.artifact_source,
        admission.artifact_space_transition,
        expected_owners.clone(),
    )?;

    let mut artifact = match record_legacy_text_area_frame_artifact(
        arena,
        &[content_root],
        property_trees,
        paint_generations,
        authority,
        oracle_context.baked_scroll_host,
        None,
        Some(required_paint_offset.map(f32::to_bits)),
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    if !admission.matches_live_source(text_area, arena, required_paint_offset) {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    artifact.owner_nodes = expected_owners.clone();
    let raster_after = record_atomic_projection_live_raster_oracle(
        arena,
        &[content_root],
        property_trees,
        paint_generations,
        authority,
        oracle_context,
        content_root,
        text_area_root,
        &admission.artifact_source,
        admission.artifact_space_transition,
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
    let Some((wrapper, [root_glyph, projection_glyph])) = classify_optional_child_mask_semantics(
        &artifact,
        artifact.chunks.as_slice(),
        content_root,
        Default::default(),
    ) else {
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
            && chunk.properties.legacy_boundary_eq(local_state)
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
        || wrapper.properties.legacy_boundary_dimensions() != Default::default()
        || !glyph_exact(
            root_glyph,
            text_area_root,
            super::PaintPropertyScope::Contents,
        )
        || !glyph_exact(
            projection_glyph,
            admission.artifact_source.projection_text_owner,
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


pub(super) fn record_scroll_focused_atomic_projection_text_area_subtree_local_artifact_for_plan(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    admission: &RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot,
    outer: PaintScrollContentWitness,
) -> Result<RecordedRetainedFocusedAtomicProjectionTextAreaSubtree, Vec<FrameArtifactFallbackReason>>
{
    let content_root = admission.content_wrapper;
    let text_area_root = admission.text_area_root;
    let invalid = |owner| vec![FrameArtifactFallbackReason::PropertyBoundary(owner)];
    if outer.boundary_root() != admission.boundary_root
        || outer.content_root() != content_root
        || outer.scroll_snapshot().owner != admission.boundary_root
        || !admission.matches_scroll_node(outer.scroll_snapshot())
        || !admission.artifact_source.is_canonical_for(text_area_root)
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
    if !admission.matches_live_source(text_area, arena, required_paint_offset) {
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
    let recorder_authority = AtomicProjectionRecorderWitness::FocusedAtomicProjectionGlyph(
        PaintScrollFocusedAtomicProjectionTextAreaSubtreeWitness::new(
            outer,
            text_area_root,
            *live_text_area_clip,
            local_scissor,
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
    if !atomic_projection_owner_topology_is_live(
        arena,
        property_trees,
        text_area_root,
        text_area_state,
        &admission.artifact_source,
    ) {
        return Err(invalid(text_area_root));
    }
    expected_owners.extend(
        admission
            .artifact_source
            .descendant_owner_topology
            .iter()
            .copied(),
    );

    let authority =
        PaintLegacyTextAreaCoverageAuthority::AtomicProjectionLocal(recorder_authority);
    let oracle_context = PaintRecordingContext {
        paint_offset: recorder_authority.outer().normalization_paint_offset(),
        required_scroll_content_paint_offset_bits: Some(required_paint_offset.map(f32::to_bits)),
        opacity_authority: PaintOpacityAuthority::Baked,
        ..PaintRecordingContext::default()
    };
    let raster_before = record_atomic_projection_live_raster_oracle(
        arena,
        &[content_root],
        property_trees,
        paint_generations,
        authority,
        oracle_context,
        content_root,
        text_area_root,
        &admission.artifact_source,
        admission.artifact_space_transition,
        expected_owners.clone(),
    )?;

    let mut artifact = match record_legacy_text_area_frame_artifact(
        arena,
        &[content_root],
        property_trees,
        paint_generations,
        authority,
        oracle_context.baked_scroll_host,
        None,
        Some(required_paint_offset.map(f32::to_bits)),
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    if !admission.matches_live_source(text_area, arena, required_paint_offset) {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    artifact.owner_nodes = expected_owners.clone();
    let raster_after = record_atomic_projection_live_raster_oracle(
        arena,
        &[content_root],
        property_trees,
        paint_generations,
        authority,
        oracle_context,
        content_root,
        text_area_root,
        &admission.artifact_source,
        admission.artifact_space_transition,
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
    let Some((wrapper, semantic)) = classify_optional_child_mask_semantics(
        &artifact,
        artifact.chunks.as_slice(),
        content_root,
        Default::default(),
    ) else {
        return Err(invalid(content_root));
    };
    let (root_glyph, preedit_underline, projection_glyph) = match semantic {
        [root_glyph, projection_glyph] if admission.preedit_source().is_none() => {
            (root_glyph, None, projection_glyph)
        }
        [root_glyph, projection_glyph, underline] if admission.preedit_source().is_some() => {
            (root_glyph, Some(underline), projection_glyph)
        }
        _ => return Err(invalid(content_root)),
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
            && chunk.properties.legacy_boundary_eq(local_state)
    };
    let underline_exact = |chunk: &super::PaintChunk| {
        let Some(preedit) = admission.preedit_source() else {
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
        chunk.owner == text_area_root
            && chunk.id.owner == text_area_root
            && chunk.id.scope == super::PaintPropertyScope::Contents
            && chunk.id.phase == super::PaintNodePhase::AfterChildren
            && chunk.id.slot == 0
            && chunk.id.role == super::PaintChunkRole::TextDecoration
            && chunk.properties.legacy_boundary_eq(local_state)
            && chunk.payload_identity == preedit.underline_identity
            && [
                chunk.bounds.x,
                chunk.bounds.y,
                chunk.bounds.width,
                chunk.bounds.height,
            ]
            .map(f32::to_bits)
                == preedit.underline_bounds_bits
            && super::PaintPayloadIdentity::prepared_rects(rects.iter().copied()).as_ref()
                == Some(&chunk.payload_identity)
    };
    let local_shape_is_exact = matches!(artifact.target, PaintArtifactTarget::CurrentTarget)
        && artifact.effect_nodes.is_empty()
        && artifact.clip_nodes.as_slice() == [recorder_authority.local_contents_clip()]
        && artifact.owner_nodes == expected_owners
        && wrapper.owner == content_root
        && wrapper.id.owner == content_root
        && wrapper.id.scope == super::PaintPropertyScope::SelfPaint
        && wrapper.id.phase == super::PaintNodePhase::BeforeChildren
        && wrapper.id.slot == 0
        && wrapper.id.role == super::PaintChunkRole::SelfDecoration
        && wrapper.properties.legacy_boundary_dimensions() == Default::default()
        && glyph_exact(
            root_glyph,
            text_area_root,
            super::PaintPropertyScope::Contents,
        )
        && glyph_exact(
            projection_glyph,
            admission.artifact_source.projection_text_owner,
            super::PaintPropertyScope::SelfPaint,
        )
        && !preedit_underline.is_some_and(|underline| !underline_exact(underline));
    if !local_shape_is_exact {
        return Err(invalid(content_root));
    }
    Ok(RecordedRetainedFocusedAtomicProjectionTextAreaSubtree {
        artifact,
        raster_oracle: raster_before,
        caret: admission.caret_source().clone(),
        preedit: admission.preedit_source().cloned(),
    })
}


pub(super) fn record_scroll_atomic_projection_selection_text_area_subtree_local_artifact_for_plan(
    arena: &NodeArena,
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
    if !admission.matches_live_source(text_area, arena, required_paint_offset) {
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
    let recorder_witness = PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness::new(
        outer,
        text_area_root,
        *live_text_area_clip,
        local_scissor,
        admission.selection_source,
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
    if !atomic_projection_owner_topology_is_live(
        arena,
        property_trees,
        text_area_root,
        text_area_state,
        &admission.artifact_source,
    ) {
        return Err(invalid(text_area_root));
    }
    expected_owners.extend(
        admission
            .artifact_source
            .descendant_owner_topology
            .iter()
            .copied(),
    );
    let authority =
        PaintLegacyTextAreaCoverageAuthority::AtomicProjectionLocal(recorder_authority);
    let oracle_context = PaintRecordingContext {
        paint_offset: recorder_authority.outer().normalization_paint_offset(),
        required_scroll_content_paint_offset_bits: Some(required_paint_offset.map(f32::to_bits)),
        opacity_authority: PaintOpacityAuthority::Baked,
        ..PaintRecordingContext::default()
    };
    let mut raster_before = record_atomic_projection_selection_live_raster_oracle(
        arena,
        &[content_root],
        property_trees,
        paint_generations,
        authority,
        oracle_context,
        content_root,
        text_area_root,
        &admission.artifact_source,
        admission.selection_source,
        admission.artifact_space_transition,
        expected_owners.clone(),
    )?;
    let mut artifact = match record_legacy_text_area_frame_artifact(
        arena,
        &[content_root],
        property_trees,
        paint_generations,
        authority,
        oracle_context.baked_scroll_host,
        None,
        Some(required_paint_offset.map(f32::to_bits)),
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    if !admission.matches_live_source(text_area, arena, required_paint_offset) {
        return Err(vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch,
        )]);
    }
    artifact.owner_nodes = expected_owners.clone();
    let mut selection_indices = artifact
        .chunks
        .iter()
        .enumerate()
        .filter_map(|(index, chunk)| {
            (chunk.owner == text_area_root
                && chunk.id.role == super::PaintChunkRole::SelectionUnderlay)
                .then_some(index)
        });
    let selection_index = selection_indices
        .next()
        .filter(|_| selection_indices.next().is_none())
        .ok_or_else(|| invalid(text_area_root))?;
    let selection_seal = normalize_atomic_projection_selection_chunk(
        &mut artifact,
        &mut raster_before,
        selection_index,
        recorder_witness.selection,
    )
    .ok_or_else(|| invalid(text_area_root))?;
    let mut raster_after = record_atomic_projection_selection_live_raster_oracle(
        arena,
        &[content_root],
        property_trees,
        paint_generations,
        authority,
        oracle_context,
        content_root,
        text_area_root,
        &admission.artifact_source,
        admission.selection_source,
        admission.artifact_space_transition,
        expected_owners.clone(),
    )?;
    let after_selection_seal = normalize_atomic_projection_selection_chunk(
        &mut artifact,
        &mut raster_after,
        selection_index,
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
    let Some((wrapper, [selection, root_glyph, projection_glyph])) =
        classify_optional_child_mask_semantics(
            &artifact,
            artifact.chunks.as_slice(),
            content_root,
            Default::default(),
        )
    else {
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
            && chunk.properties.legacy_boundary_eq(local_state)
    };
    let selection_source = recorder_witness.selection;
    let selection_exact = matches!(&artifact.ops[selection.op_range.clone()], ops
    if !ops.is_empty()
        && ops.iter().all(|op| matches!(op, super::PaintOp::DrawRect(_)))
        && selection.payload_identity.matches_text_selection_source(
            selection_source.start_char,
            selection_source.end_char,
            selection_source.color_rgba_bits,
        )
        && selection.payload_identity.matches_exact_text_selection_ops(
            ops.iter().filter_map(|op| match op { super::PaintOp::DrawRect(rect) => Some(rect), _ => None })
        ));
    if !matches!(artifact.target, PaintArtifactTarget::CurrentTarget)
        || !artifact.effect_nodes.is_empty()
        || artifact.clip_nodes.as_slice() != [recorder_authority.local_contents_clip()]
        || artifact.owner_nodes != expected_owners
        || wrapper.owner != content_root
        || wrapper.id.role != super::PaintChunkRole::SelfDecoration
        || wrapper.properties.legacy_boundary_dimensions() != Default::default()
        || selection.owner != text_area_root
        || selection.id.scope != super::PaintPropertyScope::Contents
        || selection.id.phase != super::PaintNodePhase::BeforeChildren
        || selection.id.slot != 0
        || selection.id.role != super::PaintChunkRole::SelectionUnderlay
        || !selection.properties.legacy_boundary_eq(local_state)
        || !selection_exact
        || !glyph_exact(
            root_glyph,
            text_area_root,
            super::PaintPropertyScope::Contents,
        )
        || !glyph_exact(
            projection_glyph,
            admission.artifact_source.projection_text_owner,
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


pub(super) fn record_baked_scroll_interactive_text_area_subtree_host_artifact_for_plan(
    arena: &NodeArena,
    roots: &[NodeKey],
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
    if !admission.matches_live_source(text_area, arena, recording_offset) {
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
        admission.paint_source,
    )
    .ok_or_else(|| invalid(admission.text_area_root))?;
    let mut artifact = match record_legacy_text_area_frame_artifact(
        arena,
        roots,
        property_trees,
        paint_generations,
        PaintLegacyTextAreaCoverageAuthority::InteractiveBakedHost(text_area_witness),
        Some(baked),
        None,
        None,
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    let host_preedit_seal = if admission.paint_source.has_preedit() {
        Some(
            text_area
                .text_preedit_payload_identity(
                    admission.text_area_root,
                    arena,
                    live_recording_offset,
                )
                .ok_or_else(|| invalid(admission.text_area_root))?,
        )
    } else {
        None
    };
    if let Some(selection_source) = admission.paint_source.selection() {
        let Some(selection) = artifact.chunks.get_mut(3) else {
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
        selection.payload_identity = super::PaintPayloadIdentity::prepared_text_selection(
            selection_source.start_char,
            selection_source.end_char,
            selection_source.color_rgba_bits,
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
                && chunk.properties.legacy_boundary_eq(properties)
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
    let child_mask = |chunk: &super::PaintChunk, phase: super::PaintNodePhase| {
        let [super::PaintOp::DrawRect(mask)] = &artifact.ops[chunk.op_range.clone()] else {
            return false;
        };
        chunk.owner == admission.content_wrapper
            && chunk.id.owner == admission.content_wrapper
            && chunk.id.scope == super::PaintPropertyScope::Contents
            && chunk.id.phase == phase
            && chunk.id.slot == super::RETAINED_CHILD_MASK_SLOT
            && chunk.id.role == super::PaintChunkRole::SelfDecoration
            && chunk.properties.legacy_boundary_eq(outer_state)
            && mask.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
            && mask.params.position == [chunk.bounds.x, chunk.bounds.y]
            && mask.params.size == [chunk.bounds.width, chunk.bounds.height]
            && mask.params.fill_color == [0.0; 4]
            && mask.params.opacity.to_bits() == 1.0_f32.to_bits()
            && super::PaintPayloadIdentity::prepared_rects([mask]).as_ref()
                == Some(&chunk.payload_identity)
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
            let Some(source) = admission.paint_source.selection() else {
                return false;
            };
            chunk.payload_identity.matches_exact_text_selection(
                source.start_char,
                source.end_char,
                source.color_rgba_bits,
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
                .matches_exact_text_selection_ops(rects.into_iter())
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
    let chunks_match = match admission.paint_source {
        super::PaintTextContentSource::Glyphs => {
            matches!(artifact.chunks.as_slice(), [a, b, mask_begin, c, mask_end, d]
                if root_before(a)
                    && wrapper(b)
                    && child_mask(mask_begin, super::PaintNodePhase::BeforeChildren)
                    && glyph(c)
                    && child_mask(mask_end, super::PaintNodePhase::AfterChildren)
                    && chunk_bounds_bits(mask_begin) == chunk_bounds_bits(mask_end)
                    && mask_begin.payload_identity == mask_end.payload_identity
                    && overlay(d))
        }
        super::PaintTextContentSource::Selection(_) => {
            matches!(artifact.chunks.as_slice(), [a, b, mask_begin, c, d, mask_end, e]
                if root_before(a)
                    && wrapper(b)
                    && child_mask(mask_begin, super::PaintNodePhase::BeforeChildren)
                    && selection(c)
                    && glyph(d)
                    && child_mask(mask_end, super::PaintNodePhase::AfterChildren)
                    && chunk_bounds_bits(mask_begin) == chunk_bounds_bits(mask_end)
                    && mask_begin.payload_identity == mask_end.payload_identity
                    && overlay(e))
        }
        super::PaintTextContentSource::Preedit => {
            matches!(artifact.chunks.as_slice(), [a, b, mask_begin, c, d, mask_end, e]
                if root_before(a)
                    && wrapper(b)
                    && child_mask(mask_begin, super::PaintNodePhase::BeforeChildren)
                    && glyph(c)
                    && underline(d)
                    && child_mask(mask_end, super::PaintNodePhase::AfterChildren)
                    && chunk_bounds_bits(mask_begin) == chunk_bounds_bits(mask_end)
                    && mask_begin.payload_identity == mask_end.payload_identity
                    && overlay(e))
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
    if !admission.matches_live_source(text_area, arena, recording_offset) {
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
        admission.paint_source,
    )
    .ok_or_else(|| {
        vec![FrameArtifactFallbackReason::PropertyBoundary(
            admission.text_area_root,
        )]
    })?;
    let artifact = match record_legacy_text_area_frame_artifact(
        arena,
        roots,
        property_trees,
        paint_generations,
        PaintLegacyTextAreaCoverageAuthority::BakedHost(text_area_witness),
        Some(baked),
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

/// Records the closed C1/C2a `Element wrapper -> plain TextArea` content subtree.
/// The outer scroll/clip pair is consumed, while the TextArea contents clip is
/// retained as one localized, parentless artifact clip.
pub(super) fn record_scroll_text_area_subtree_local_artifact_for_plan(
    arena: &NodeArena,
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
    if !admission.matches_live_source(text_area, arena, required_paint_offset) {
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
        admission.paint_source,
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

    let mut artifact = match record_legacy_text_area_frame_artifact(
        arena,
        &[content_root],
        property_trees,
        paint_generations,
        PaintLegacyTextAreaCoverageAuthority::Local(witness),
        None,
        None,
        Some(required_paint_offset.map(f32::to_bits)),
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    // Generic paint generations include screen-space TextArea paint identity,
    // whose layout origin changes when only the outer scroll moves.
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
    if let Some(selection_source) = admission.paint_source.selection() {
        let mut selection_indices =
            artifact
                .chunks
                .iter()
                .enumerate()
                .filter_map(|(index, chunk)| {
                    (chunk.owner == text_area_root
                        && chunk.id.role == super::PaintChunkRole::SelectionUnderlay)
                        .then_some(index)
                });
        let selection_index = selection_indices
            .next()
            .filter(|_| selection_indices.next().is_none())
            .ok_or_else(|| invalid(text_area_root))?;
        let selection = &artifact.chunks[selection_index];
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
        let sealed_identity = super::PaintPayloadIdentity::prepared_text_selection(
            selection_source.start_char,
            selection_source.end_char,
            selection_source.color_rgba_bits,
            rects.iter().copied(),
        )
        .ok_or_else(|| invalid(text_area_root))?;
        artifact.chunks[selection_index].payload_identity = sealed_identity;
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
            && chunk.properties.legacy_boundary_dimensions() == Default::default()
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
            && chunk.properties.legacy_boundary_eq(local_state)
            && prepared.has_canonical_identity()
            && chunk.payload_identity == super::PaintPayloadIdentity::prepared_texts([prepared])
    };
    let selection_matches = |chunk: &super::PaintChunk| {
        let Some(source) = admission.paint_source.selection() else {
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
            && chunk.properties.legacy_boundary_eq(local_state)
            && rects.iter().all(|rect| {
                rect.params.fill_color.map(f32::to_bits) == source.color_rgba_bits
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
            && super::PaintPayloadIdentity::prepared_text_selection(
                source.start_char,
                source.end_char,
                source.color_rgba_bits,
                rects.into_iter(),
            )
            .as_ref()
                == Some(&chunk.payload_identity)
    };
    let Some((wrapper, semantic)) = classify_optional_child_mask_semantics(
        &artifact,
        artifact.chunks.as_slice(),
        content_root,
        Default::default(),
    ) else {
        return Err(invalid(content_root));
    };
    let chunks_match_grammar = wrapper_matches(wrapper) && match admission.paint_source {
        super::PaintTextContentSource::Glyphs => {
            matches!(semantic, [glyph] if glyph_matches(glyph))
        }
        super::PaintTextContentSource::Selection(_) => {
            matches!(semantic, [selection, glyph]
                if selection_matches(selection) && glyph_matches(glyph))
        }
        super::PaintTextContentSource::Preedit => false,
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


// ---- recorder internals relocated out of the durable frame recorder ----
//
// These serve the exact-shape middle layer only. They live here so
// `frame_recorder.rs` keeps no component-specific property assertion, no exact
// grammar dispatch, and no dependency on `legacy_admission`. All of it is
// deleted with this file in the Stage C hard cutover.

fn classify_optional_child_mask_semantics<'a>(
    artifact: &super::PaintArtifact,
    chunks: &'a [super::PaintChunk],
    content_root: NodeKey,
    mask_properties: crate::view::compositor::property_tree::PropertyTreeState,
) -> Option<(&'a super::PaintChunk, &'a [super::PaintChunk])> {
    let (wrapper, tail) = chunks.split_first()?;
    let is_mask = |chunk: &super::PaintChunk| chunk.id.slot == super::RETAINED_CHILD_MASK_SLOT;
    let has_boundary_mask = tail.first().is_some_and(|chunk| is_mask(chunk))
        || tail.last().is_some_and(|chunk| is_mask(chunk));
    if !has_boundary_mask {
        return tail
            .iter()
            .all(|chunk| !is_mask(chunk))
            .then_some((wrapper, tail));
    }
    let (mask_end, with_begin) = tail.split_last()?;
    let (mask_begin, semantic) = with_begin.split_first()?;
    let mask_exact = |chunk: &super::PaintChunk, phase: super::PaintNodePhase| {
        let [super::PaintOp::DrawRect(mask)] = &artifact.ops[chunk.op_range.clone()] else {
            return false;
        };
        chunk.owner == content_root
            && chunk.id.owner == content_root
            && chunk.id.scope == super::PaintPropertyScope::Contents
            && chunk.id.phase == phase
            && chunk.id.slot == super::RETAINED_CHILD_MASK_SLOT
            && chunk.id.role == super::PaintChunkRole::SelfDecoration
            && chunk.properties.legacy_boundary_eq(mask_properties)
            && mask.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
            && mask.params.position == [chunk.bounds.x, chunk.bounds.y]
            && mask.params.size == [chunk.bounds.width, chunk.bounds.height]
            && mask.params.fill_color == [0.0; 4]
            && mask.params.opacity.to_bits() == 1.0_f32.to_bits()
            && super::PaintPayloadIdentity::prepared_rects([mask]).as_ref()
                == Some(&chunk.payload_identity)
    };
    (semantic.iter().all(|chunk| !is_mask(chunk))
        && mask_exact(mask_begin, super::PaintNodePhase::BeforeChildren)
        && mask_exact(mask_end, super::PaintNodePhase::AfterChildren)
        && chunk_bounds_bits(mask_begin) == chunk_bounds_bits(mask_end)
        && mask_begin.payload_identity == mask_end.payload_identity)
        .then_some((wrapper, semantic))
}

fn chunk_bounds_bits(chunk: &PaintChunk) -> [u32; 4] {
    [
        chunk.bounds.x.to_bits(),
        chunk.bounds.y.to_bits(),
        chunk.bounds.width.to_bits(),
        chunk.bounds.height.to_bits(),
    ]
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
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(snapshot);
            SnapshotMerge::Inserted
        }
        std::collections::hash_map::Entry::Occupied(entry) if *entry.get() == snapshot => {
            SnapshotMerge::Identical
        }
        std::collections::hash_map::Entry::Occupied(_) => SnapshotMerge::Conflict,
    }
}

/// C3a graph-inert recorder for the exact atomic-projection sibling.  Source
/// authority remains the live TextArea oracle and is checked on both sides of
/// the metadata/full pass; the Copy paint witness authorizes properties only.
#[allow(clippy::too_many_arguments)]
pub(super) fn atomic_projection_owner_topology_is_live(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    text_area_root: NodeKey,
    text_area_state: crate::view::compositor::property_tree::PropertyTreeState,
    source: &super::PaintAtomicProjectionArtifactSource,
) -> bool {
    use crate::view::base_component::text_area::{
        TextAreaLineBreak, TextAreaProjectionSegment, TextAreaTextRun,
    };

    if !source.is_canonical_for(text_area_root) {
        return false;
    }
    let direct_owner_count = source
        .descendant_owner_topology
        .iter()
        .filter(|owner| owner.parent == Some(text_area_root))
        .count();
    if arena.children_of(text_area_root).len() != direct_owner_count {
        return false;
    }
    source.descendant_owner_topology.iter().all(|owner| {
        let Some(node) = arena.get(owner.owner) else {
            return false;
        };
        if arena.parent_of(owner.owner) != owner.parent
            || node.element.is_deferred_to_root_viewport_render()
            || node.element.has_active_animator()
            || property_trees.states.get(&owner.owner).is_none_or(|state| {
                !state.paint.legacy_boundary_eq(text_area_state)
                    || !state.descendants.legacy_boundary_eq(text_area_state)
            })
        {
            return false;
        }
        if owner.owner == source.projection_text_owner {
            return node
                .element
                .as_any()
                .is::<crate::view::base_component::Text>()
                && node.element.children().is_empty();
        }
        if owner.parent != Some(text_area_root) {
            return false;
        }
        if node.element.as_any().is::<TextAreaProjectionSegment>() {
            return node.element.children() == [source.projection_text_owner];
        }
        (node.element.as_any().is::<TextAreaTextRun>()
            || node.element.as_any().is::<TextAreaLineBreak>())
            && node.element.children().is_empty()
    })
}

fn baked_scroll_text_area_subtree_properties_are_exact(
    owner: NodeKey,
    properties: crate::view::compositor::property_tree::PropertyTreeState,
    baked: PaintBakedScrollHostWitness,
    text_area: PaintScrollTextAreaSubtreeWitness,
) -> bool {
    let properties = properties.legacy_boundary_dimensions();
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
    let properties = properties.legacy_boundary_dimensions();
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
    text_area: PaintScrollDetachedProjectionSubtreeWitness,
) -> bool {
    let properties = properties.legacy_boundary_dimensions();
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
    witness: PaintScrollDetachedProjectionSubtreeWitness,
) -> bool {
    let properties = properties.legacy_boundary_dimensions();
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
    let properties = properties.legacy_boundary_dimensions();
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
    let properties = properties.legacy_boundary_dimensions();
    properties == Default::default()
        || properties
            == crate::view::compositor::property_tree::PropertyTreeState {
                clip: Some(witness.local_contents_clip().id),
                ..Default::default()
            }
}

/// The exact property assertion for the one grammar this recording admits.
///
/// It replaces the per-variant arms `assess_manifest` used to carry: the
/// durable assessor now knows only generic policies, and the exactness that
/// used to select between them lives with the witness that proves it.
fn legacy_properties_are_exact(
    authority: PaintLegacyTextAreaCoverageAuthority,
    baked_scroll_host: Option<PaintBakedScrollHostWitness>,
    owner: NodeKey,
    properties: crate::view::compositor::property_tree::PropertyTreeState,
) -> bool {
    match (authority, baked_scroll_host) {
        (PaintLegacyTextAreaCoverageAuthority::Local(witness), None) => {
            scroll_text_area_subtree_local_properties_are_exact(properties, witness)
        }
        (PaintLegacyTextAreaCoverageAuthority::BakedHost(witness), Some(baked)) => {
            baked_scroll_text_area_subtree_properties_are_exact(owner, properties, baked, witness)
        }
        (PaintLegacyTextAreaCoverageAuthority::AtomicProjectionLocal(witness), None) => {
            scroll_atomic_projection_text_area_subtree_local_properties_are_exact(
                properties,
                witness.property(),
            )
        }
        (PaintLegacyTextAreaCoverageAuthority::AtomicProjectionBakedHost(witness), Some(baked)) => {
            baked_scroll_atomic_projection_text_area_subtree_properties_are_exact(
                owner,
                properties,
                baked,
                witness.property(),
            )
        }
        (PaintLegacyTextAreaCoverageAuthority::InteractiveLocal(witness), None) => {
            scroll_interactive_text_area_subtree_local_properties_are_exact(properties, witness)
        }
        (PaintLegacyTextAreaCoverageAuthority::InteractiveBakedHost(witness), Some(baked)) => {
            baked_scroll_interactive_text_area_subtree_properties_are_exact(
                owner, properties, baked, witness,
            )
        }
        _ => false,
    }
}

/// Manifest assessment for an exact detached-subtree recording.
///
/// The generic accounting is the same as the durable assessor's — validation
/// errors, chunk/op counts, legacy boundaries — but every property assertion
/// comes from `legacy_properties_are_exact`, and no planned boundary or native
/// scroll receiver is admissible in any of these grammars.
fn assess_legacy_manifest(
    manifest: &super::PaintCoverageManifest,
    authority: PaintLegacyTextAreaCoverageAuthority,
    baked_scroll_host: Option<PaintBakedScrollHostWitness>,
) -> FrameArtifactEligibility {
    let mut reasons = manifest
        .validation_errors
        .iter()
        .cloned()
        .map(FrameArtifactFallbackReason::Validation)
        .collect::<Vec<_>>();
    let mut chunk_count = 0usize;
    let mut op_count = 0usize;
    let mut debug_boundaries = Vec::new();
    fn push(reasons: &mut Vec<FrameArtifactFallbackReason>, reason: FrameArtifactFallbackReason) {
        if !reasons.contains(&reason) {
            reasons.push(reason);
        }
    }
    for item in &manifest.items {
        match item {
            PaintCoverageItem::ArtifactChunk { chunk, ops, .. } => {
                chunk_count = chunk_count.saturating_add(1);
                op_count = op_count.saturating_add(ops.as_ref().map_or(0, Vec::len));
                if !legacy_properties_are_exact(
                    authority,
                    baked_scroll_host,
                    chunk.owner,
                    chunk.properties,
                ) {
                    push(
                        &mut reasons,
                        FrameArtifactFallbackReason::PropertyBoundary(chunk.owner),
                    );
                }
            }
            PaintCoverageItem::TransparentNode {
                owner, properties, ..
            }
            | PaintCoverageItem::CulledSubtree {
                owner, properties, ..
            } => {
                if !legacy_properties_are_exact(authority, baked_scroll_host, *owner, *properties) {
                    push(
                        &mut reasons,
                        FrameArtifactFallbackReason::PropertyBoundary(*owner),
                    );
                }
            }
            PaintCoverageItem::LegacyBoundary { root, reason, .. } => {
                let boundary = FrameArtifactDebugBoundary {
                    owner: *root,
                    kind: FrameArtifactDebugBoundaryKind::Legacy(*reason),
                };
                if !debug_boundaries.contains(&boundary) {
                    debug_boundaries.push(boundary);
                }
                push(
                    &mut reasons,
                    FrameArtifactFallbackReason::LegacyBoundary(*reason),
                );
            }
            PaintCoverageItem::PlannedBoundary { .. }
            | PaintCoverageItem::NativeScrollContentReceiver { .. } => {
                push(
                    &mut reasons,
                    FrameArtifactFallbackReason::Validation(
                        PaintCoverageValidationError::RecordingPassMismatch,
                    ),
                );
            }
        }
    }
    FrameArtifactEligibility {
        eligible: reasons.is_empty(),
        reasons: reasons.clone(),
        chunk_count,
        op_count,
        debug_boundaries,
    }
}

/// Metadata-only coverage for the live raster oracles, which need the exact
/// chunk identity on both sides of the full pass without materializing ops.
fn record_legacy_metadata_manifest(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    context: PaintRecordingContext,
    authority: PaintLegacyTextAreaCoverageAuthority,
) -> super::PaintCoverageManifest {
    record_legacy_text_area_coverage_manifest(
        arena,
        roots,
        CoverageRecordingMode::MetadataOnly,
        property_trees,
        paint_generations,
        context,
        authority,
    )
}

/// Recording driver for the exact detached-subtree grammars.
///
/// It mirrors the durable driver's preflight / full / cross-pass structure but
/// keeps the exactness here: the durable policy enum carries no grammar
/// variant, and the authority reaches coverage only through the private legacy
/// entry point. Every caller records under `RendererMode::StrictPlan`, so a
/// failed assessment is a forced error rather than a whole-frame fallback.
fn record_legacy_text_area_frame_artifact(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    authority: PaintLegacyTextAreaCoverageAuthority,
    baked_scroll_host: Option<PaintBakedScrollHostWitness>,
    consumed_ancestor_property: Option<super::ConsumedAncestorProperty>,
    required_scroll_content_paint_offset_bits: Option<[u32; 2]>,
) -> Result<FrameArtifactRecordOutcome, ForcedFrameArtifactError> {
    let initial_recording_context = PaintRecordingContext {
        paint_offset: if authority.detaches_clip_snapshot() {
            authority.outer().normalization_paint_offset()
        } else {
            [0.0, 0.0]
        },
        consumed_ancestor_property,
        required_scroll_content_paint_offset_bits,
        opacity_authority: PaintOpacityAuthority::Baked,
        baked_scroll_host,
        ..PaintRecordingContext::default()
    };
    let preflight = record_legacy_text_area_coverage_manifest(
        arena,
        roots,
        CoverageRecordingMode::MetadataOnly,
        property_trees,
        paint_generations,
        initial_recording_context,
        authority,
    );
    let preflight_eligibility = assess_legacy_manifest(&preflight, authority, baked_scroll_host);
    if !preflight_eligibility.eligible {
        return Err(ForcedFrameArtifactError {
            reasons: preflight_eligibility.reasons,
        });
    }
    let manifest = record_legacy_text_area_coverage_manifest(
        arena,
        roots,
        CoverageRecordingMode::FullArtifact,
        property_trees,
        paint_generations,
        initial_recording_context,
        authority,
    );
    let mut eligibility = assess_legacy_manifest(&manifest, authority, baked_scroll_host);
    if eligibility.eligible && !canonical_manifest_matches(&preflight, &manifest) {
        eligibility.eligible = false;
        eligibility
            .reasons
            .push(FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::RecordingPassMismatch,
            ));
    }
    if !eligibility.eligible {
        return Err(ForcedFrameArtifactError {
            reasons: eligibility.reasons,
        });
    }
    materialize_frame_artifact(
        manifest,
        PaintArtifactTarget::CurrentTarget,
        RendererMode::StrictPlan,
        eligibility,
    )
}


/// Descendant-scroll content recorder for the one content subtree that still
/// carries an exact detached TextArea clip. The generic sibling in
/// `frame_recorder` records the same boundary when no such clip is present.
pub(super) fn record_scroll_content_text_area_subtree_artifact_for_plan(
    arena: &NodeArena,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    witness: PaintScrollContentWitness,
    text_area_witness: PaintScrollTextAreaSubtreeWitness,
    required_paint_offset: [f32; 2],
) -> Result<PaintArtifact, Vec<FrameArtifactFallbackReason>> {
    let content_root = witness.content_root();
    if required_paint_offset.iter().any(|value| !value.is_finite())
        || arena.parent_of(content_root) != Some(witness.boundary_root())
        || text_area_witness.outer() != witness
    {
        return Err(vec![FrameArtifactFallbackReason::PropertyBoundary(
            content_root,
        )]);
    }
    let artifact = match record_legacy_text_area_frame_artifact(
        arena,
        &[content_root],
        property_trees,
        paint_generations,
        PaintLegacyTextAreaCoverageAuthority::Local(text_area_witness),
        None,
        Some(witness.consumed_property()),
        Some(required_paint_offset.map(f32::to_bits)),
    ) {
        Ok(FrameArtifactRecordOutcome::Artifact { artifact, .. }) => artifact,
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility)) => {
            return Err(eligibility.reasons);
        }
        Err(error) => return Err(error.reasons),
    };
    (!artifact.chunks.is_empty() && matches!(artifact.target, PaintArtifactTarget::CurrentTarget))
        .then_some(artifact)
        .ok_or_else(|| vec![FrameArtifactFallbackReason::PropertyBoundary(content_root)])
}
